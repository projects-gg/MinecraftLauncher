//! Integration tests against local copies of the Cloud AFK Worker and the friends service (the
//! Studio repository's `services/cloud-afk` and `services/social`), each with its fakes. Never point
//! these at the production services.
//!
//! ```sh
//! # services/social:    pnpm dev:discord; pnpm dev
//! # services/cloud-afk: pnpm dev:leaderos; pnpm dev   (both local D1 schemas migrated)
//! cargo test -p pl-cloud-afk --test live -- --ignored --test-threads=1
//! ```
//!
//! `PL_SOCIAL_URL` and `PL_CLOUD_AFK_URL` override the services' addresses (defaults: the `pnpm dev`
//! ports). The account parts need a stopped account of the signed-in player on a machine that is
//! not connected, in the Cloud AFK Worker's local D1: its id in `PL_CLOUD_AFK_ACCOUNT`, the
//! player's Discord id in `PL_CLOUD_AFK_DISCORD_ID` (default: the fake LeaderOS user's).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use pl_cloud_afk::{
    Bearer, CloudConfig, CloudEdit, CloudEditCommand, CloudEditRule, CloudEvery, CloudLiveEvent,
    CloudRowText, CloudSaveOutcome, CloudService, CloudSession, Copies, LiveSink, api_url,
};
use pl_social::{SocialClient, SocialConfig, auth_code_from_deep_link};
use tokio::runtime::Handle;
use url::Url;

const USER_AGENT: &str = "ProjectsLauncher/integration-test";

/// A local address only.
fn local(var: &str, default: &str) -> Url {
    let url = Url::parse(&std::env::var(var).unwrap_or_else(|_| default.into()))
        .unwrap_or_else(|_| panic!("{var} is a URL"));
    assert!(
        matches!(url.host_str(), Some("127.0.0.1" | "localhost")),
        "integration tests only run against a local wrangler dev"
    );
    url
}

/// Plays the browser through the friends service's sign-in with the fake Discord and claims the
/// session as the launcher does.
async fn sign_in(social: &SocialClient, user_id: &str, name: &str) -> Bearer {
    let attempt = social.begin_login();
    let browser = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let start = browser
        .get(attempt.browser_url().clone())
        .send()
        .await
        .unwrap();
    assert_eq!(start.status().as_u16(), 302, "start redirects to Discord");
    let cookie = start.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let mut authorize = Url::parse(start.headers()["location"].to_str().unwrap()).unwrap();
    authorize
        .query_pairs_mut()
        .append_pair("user", &format!("{user_id}:{name}"));
    let approve = browser.get(authorize).send().await.unwrap();
    assert_eq!(approve.status().as_u16(), 302, "the fake Discord approves");
    let page = browser
        .get(approve.headers()["location"].to_str().unwrap())
        .header("cookie", cookie)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let link_start = page
        .find("href=\"projects://auth/done?c=")
        .expect("the page links back to the launcher")
        + "href=\"".len();
    let link_end = link_start + page[link_start..].find('"').unwrap();
    let code = auth_code_from_deep_link(&page[link_start..link_end]).unwrap();
    let signed_in = social
        .complete_login(&attempt, &code, "Integration PC")
        .await
        .unwrap();
    Bearer::new(signed_in.token.expose())
}

fn service(dir: &std::path::Path, events: Arc<Mutex<Vec<CloudLiveEvent>>>) -> CloudService {
    let base = local("PL_CLOUD_AFK_URL", "http://127.0.0.1:8790/");
    let store = Arc::new(pl_afk::MemorySecretStore::new());
    let copies = Copies::new(store, dir.join("copies.json"));
    let sink: LiveSink = Arc::new(move |event| events.lock().unwrap().push(event));
    CloudService::new(
        reqwest::Client::new(),
        CloudConfig::new(api_url(Some(base.as_str())), USER_AGENT),
        copies,
        Handle::current(),
        sink,
    )
}

fn edit(rev: u64) -> CloudEdit {
    CloudEdit {
        rev,
        name: None,
        server: None,
        version: None,
        lang: None,
        config: None,
    }
}

#[tokio::test]
#[ignore = "needs the local Workers, see the module docs"]
async fn the_launcher_against_the_local_worker() {
    let dir = tempfile::tempdir().unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let cloud = service(dir.path(), events.clone());

    // Without a session: the offer only, as the AFK page asks first.
    let hidden = cloud.overview(None, true).await;
    assert!(hidden.available, "the local Worker sells: {hidden:?}");
    assert_eq!(hidden.mode, "on");
    assert!(hidden.month_price.is_some());
    assert_eq!(hidden.session, CloudSession::SignedOut);
    assert!(hidden.accounts.is_empty());

    let social = SocialClient::new(SocialConfig::new(
        local("PL_SOCIAL_URL", "http://127.0.0.1:8797/"),
        USER_AGENT,
    ))
    .unwrap();
    let discord_id = std::env::var("PL_CLOUD_AFK_DISCORD_ID")
        .unwrap_or_else(|_| "100000000000000001".to_owned());
    let token = sign_in(&social, &discord_id, "oyuncu").await;
    let overview = cloud.overview(Some(&token), true).await;
    assert_eq!(overview.session, CloudSession::SignedIn, "{overview:?}");
    let Ok(id) = std::env::var("PL_CLOUD_AFK_ACCOUNT") else {
        eprintln!("PL_CLOUD_AFK_ACCOUNT unset: the account parts are skipped");
        return;
    };
    let listed = overview
        .accounts
        .iter()
        .find(|account| account.id == id)
        .expect("the player's account is listed");
    assert_eq!(listed.desired, "stop");

    // The editor: a login command sealed here, a plain command, an auto-reply.
    let view = cloud.edit_view(&token, &id).await.unwrap();
    let mut config = view.config.clone();
    config.commands = vec![
        CloudEditCommand {
            id: "c1".into(),
            enabled: true,
            on: "join".into(),
            delay: Some(3),
            every: None,
            at: None,
            words: CloudRowText::Secret {
                text: "/login Deneme123".into(),
                kept: false,
            },
        },
        CloudEditCommand {
            id: "c2".into(),
            enabled: true,
            on: "interval".into(),
            delay: None,
            every: Some(CloudEvery { min: 300, max: 600 }),
            at: None,
            words: CloudRowText::Plain {
                text: "/afk".into(),
            },
        },
    ];
    config.rules = vec![CloudEditRule {
        id: "r1".into(),
        enabled: true,
        pattern: "selam".into(),
        regex: false,
        from: "player".into(),
        owner_only: false,
        cooldown: 60,
        reply: CloudRowText::Plain {
            text: "Selam! Şu an AFK'yım.".into(),
        },
    }];
    let outcome = cloud
        .save(
            &token,
            &id,
            CloudEdit {
                config: Some(config),
                ..edit(view.account.rev)
            },
        )
        .await
        .unwrap();
    let CloudSaveOutcome::Saved { account: saved } = outcome else {
        panic!("not saved: {outcome:?}");
    };
    assert_eq!(saved.rev, view.account.rev + 1);
    let secret = &saved.config.commands[0];
    assert!(secret.secret && secret.text.is_none(), "{secret:?}");
    assert_eq!(secret.preview.as_deref(), Some("/login ******"));
    let fp = secret.fp.clone().expect("the Worker names the seal");
    assert_eq!(fp.len(), 16);

    // This computer kept the words under the same fingerprint: the editor shows them.
    let again = cloud.edit_view(&token, &id).await.unwrap();
    assert_eq!(
        again.config.commands[0].words,
        CloudRowText::Secret {
            text: "/login Deneme123".into(),
            kept: true
        }
    );

    // An edit from an older revision is a conflict, never an overwrite.
    let stale = cloud
        .save(
            &token,
            &id,
            CloudEdit {
                name: Some("Madenci08".into()),
                ..edit(view.account.rev)
            },
        )
        .await
        .unwrap();
    assert!(
        matches!(stale, CloudSaveOutcome::Conflict { .. }),
        "{stale:?}"
    );

    // Saving the shown words unchanged keeps the stored seal.
    let renamed = cloud
        .save(
            &token,
            &id,
            CloudEdit {
                name: Some("Madenci08".into()),
                config: Some(again.config.clone()),
                ..edit(saved.rev)
            },
        )
        .await
        .unwrap();
    let CloudSaveOutcome::Saved { account: renamed } = renamed else {
        panic!("not saved: {renamed:?}");
    };
    assert_eq!(renamed.name, "Madenci08");
    assert_eq!(renamed.config.commands[0].fp.as_deref(), Some(fp.as_str()));

    // A private address never leaves this computer.
    let private = cloud
        .save(
            &token,
            &id,
            CloudEdit {
                server: Some(pl_cloud_afk::CloudServer {
                    host: "192.168.1.20".into(),
                    port: 25565,
                }),
                ..edit(renamed.rev)
            },
        )
        .await
        .unwrap();
    assert!(
        matches!(private, CloudSaveOutcome::Invalid { .. }),
        "{private:?}"
    );

    // Start and stop are kept with the machine away.
    assert_eq!(cloud.start(&token, &id).await.unwrap().desired, "run");
    assert_eq!(cloud.stop(&token, &id).await.unwrap().desired, "stop");

    // The machine is away: "Göster" cannot be answered now (the launcher says so, `Offline`).
    match cloud.reveal(&token, &id, "c1").await {
        Err(error) if matches!(error.code(), "host_offline" | "unavailable") => {}
        other => panic!("reveal with the machine away: {:?}", other.map(|_| "shown")),
    }
    // A plain console line with the machine away: refused the same way.
    let line = cloud.send(&token, &id, "selam").await;
    assert!(
        matches!(&line, Err(error) if matches!(error.code(), "host_offline" | "not_running")),
        "{line:?}"
    );
    let logs = cloud.logs(&token, &id, None).await;
    eprintln!("logs with the machine away: {logs:?}");

    // The live console: the socket opens and says the machine is away.
    cloud.watch(&token, &id).await.unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let hello = loop {
        if let Some(hello) = events
            .lock()
            .unwrap()
            .iter()
            .find(|event| matches!(event, CloudLiveEvent::Hello { .. }))
            .cloned()
        {
            break hello;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "no hello: {:?}",
            events.lock().unwrap()
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    assert!(
        matches!(&hello, CloudLiveEvent::Hello { online: false, .. }),
        "{hello:?}"
    );
    cloud.unwatch(&id);

    // Back as it was for the next run.
    let back = cloud.edit_view(&token, &id).await.unwrap();
    let mut empty = back.config.clone();
    empty.commands.clear();
    empty.rules.clear();
    let reset = cloud
        .save(
            &token,
            &id,
            CloudEdit {
                name: Some("Madenci07".into()),
                config: Some(empty),
                ..edit(back.account.rev)
            },
        )
        .await
        .unwrap();
    assert!(matches!(reset, CloudSaveOutcome::Saved { .. }), "{reset:?}");
}

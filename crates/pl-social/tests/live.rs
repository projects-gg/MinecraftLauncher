//! Integration tests against a local `wrangler dev` of the friends service (`services/social` in
//! the Studio repository) with its fake Discord. Never point these at the production service.
//!
//! ```sh
//! cd services/social   # in the Studio repository
//! cp .dev.vars.example .dev.vars
//! pnpm db:migrate:local
//! pnpm dev:discord     # fake Discord on 127.0.0.1:8798
//! pnpm dev             # Worker on 127.0.0.1:8797
//! cargo test -p pl-social --test live -- --ignored --test-threads=1   # here
//! ```
//!
//! `PL_SOCIAL_URL` overrides the service URL (default `http://127.0.0.1:8797/`). The chat tests
//! need chat on in the service (its seal key `CHAT_MAC_KEY` in `.dev.vars`, as in the example).

use std::sync::Arc;
use std::sync::atomic::AtomicI64;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use pl_social::{
    CallAnswer, CallEndReason, CallKind, ChatRejection, ChatSetup, CrewKey, CrewMuteEdit,
    ErrorCode, FEATURE_CHAT, GameLocation, InviteOutcome, JoinMode, JoinOutcome, LimitedKey,
    LocalStatus, LoginAttempt, PermissionLevel, PermissionsEdit, PlanAnswer, PresenceEvent,
    PresenceHandle, PresenceOptions, PresenceState, ProfileUpdate, Relation, ReportReason,
    RequestOutcome, Session, SettingsPrecondition, SocialClient, SocialConfig, SocialError,
    StatusMode, StopReason, TEAM_FEATURE_CALL_SERVER, TEAM_FEATURE_CALLS, TEAM_FEATURE_LINK,
    TEAM_FEATURE_LOOK, TEAM_FEATURE_MULTI, TEAM_FEATURE_MUTE, TEAM_FEATURE_PLANS,
    TEAM_FEATURE_REQUESTS, TEAM_FEATURE_ROLES, TeamEdit, TeamLook, TeamNotifications,
    TeamNotificationsEdit, TeamRelation, auth_code_from_deep_link, crew_code, crew_conversation,
    direct_conversation, normalize_crew_link,
};
use serde_json::json;
use url::Url;

fn client() -> SocialClient {
    let base = std::env::var("PL_SOCIAL_URL").unwrap_or_else(|_| "http://127.0.0.1:8797/".into());
    let base = Url::parse(&base).expect("PL_SOCIAL_URL is a URL");
    assert!(
        matches!(base.host_str(), Some("127.0.0.1" | "localhost")),
        "integration tests only run against a local wrangler dev"
    );
    SocialClient::new(SocialConfig::new(base, "ProjectsLauncher/integration-test")).unwrap()
}

/// A fresh fake Discord id (17+ digits) per call.
fn new_user_id(n: u32) -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    format!("9{millis}{n:03}")
}

/// A Discord account made just now (a Discord id carries the time it was made): the service
/// treats it as new, a limited account (SPAM1).
fn new_discord_account_id(n: u64) -> String {
    let millis = u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap();
    (((millis - 1_420_070_400_000) << 22) + n).to_string()
}

fn unix_now() -> i64 {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    )
    .unwrap()
}

/// Plays the browser: start → fake Discord consent → callback (with the state cookie). Returns
/// the one-time code of the `projects://auth/done?c=…` link the page opens.
async fn approve_in_browser(attempt: &LoginAttempt, user_id: &str, name: &str) -> String {
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
    assert_eq!(approve.status().as_u16(), 302, "fake Discord approves");
    let callback = browser
        .get(approve.headers()["location"].to_str().unwrap())
        .header("cookie", cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(callback.status().as_u16(), 200);
    let page = callback.text().await.unwrap();
    assert!(page.contains("Giriş tamamlandı"));
    assert!(!page.contains("pls_"), "no token in the page");
    let link_start = page
        .find("href=\"projects://auth/done?c=")
        .expect("the page links back to the launcher")
        + "href=\"".len();
    let link_end = link_start + page[link_start..].find('"').expect("the link ends");
    auth_code_from_deep_link(&page[link_start..link_end]).expect("the link carries a one-time code")
}

/// A full sign-in as the launcher does it: browser step, then the claim with verifier and code.
async fn sign_in(client: &SocialClient, user_id: &str, name: &str) -> Session {
    let attempt = client.begin_login();
    let code = approve_in_browser(&attempt, user_id, name).await;
    let signed_in = client
        .complete_login(&attempt, &code, "Integration PC")
        .await
        .unwrap();
    assert_eq!(signed_in.me.profile.id, user_id);
    client.session(signed_in.token)
}

/// Waits for the first event matching `predicate`, skipping others.
async fn expect(
    handle: &mut PresenceHandle,
    what: &str,
    predicate: impl Fn(&PresenceEvent) -> bool,
) -> PresenceEvent {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(left, handle.next_event()).await {
            Ok(Some(event)) if predicate(&event) => return event,
            Ok(Some(_)) => continue,
            Ok(None) => panic!("presence stopped while waiting for {what}"),
            Err(_) => panic!("timed out waiting for {what}"),
        }
    }
}

fn options() -> PresenceOptions {
    PresenceOptions::default()
}

/// A status change goes out on its own (the socket task sends it when it sees it): give it a
/// moment before a request that depends on it.
async fn settle() {
    tokio::time::sleep(Duration::from_millis(400)).await;
}

/// Chat takes about one message a second per player (bursts of 5): keep under it.
async fn pace() {
    tokio::time::sleep(Duration::from_millis(1100)).await;
}

/// Chat on the socket, syncing from `since` (0: a new device).
fn chat_from(since: i64) -> ChatSetup {
    ChatSetup {
        since: Arc::new(AtomicI64::new(since)),
    }
}

/// Connects with chat and waits for the connect and its first sync; returns the chat state.
async fn connect_chat(session: &Session) -> (PresenceHandle, pl_social::ChatStatus) {
    let mut handle = session.connect_presence_with_chat(options(), chat_from(0));
    let chat = match expect(&mut handle, "connected with chat", |e| {
        matches!(e, PresenceEvent::Connected { .. })
    })
    .await
    {
        PresenceEvent::Connected { chat, .. } => chat.expect("the service took chat"),
        _ => unreachable!(),
    };
    expect(&mut handle, "the first sync", |e| {
        matches!(e, PresenceEvent::ChatSynced { .. })
    })
    .await;
    (handle, chat)
}

#[tokio::test]
#[ignore = "needs `pnpm dev` and `pnpm dev:discord` in services/social"]
async fn friends_presence_invites_settings_and_deletion() {
    let client = client();
    client.health().await.expect("wrangler dev is running");
    let ayse_id = new_user_id(1);
    let bora_id = new_user_id(2);
    let ayse = sign_in(&client, &ayse_id, "ayse").await;
    let bora = sign_in(&client, &bora_id, "bora").await;
    let ayse_me = ayse.me().await.unwrap();
    assert_eq!(ayse_me.status, StatusMode::Online);

    // Ayşe is online; Bora finds her by code and asks.
    let mut ayse_presence = ayse.connect_presence(options());
    expect(&mut ayse_presence, "connected", |e| {
        matches!(e, PresenceEvent::Connected { .. })
    })
    .await;
    let lookup = bora.lookup_code(&ayse_me.friend_code).await.unwrap();
    assert_eq!(lookup.relation, Relation::None);
    assert_eq!(lookup.user.id, ayse_id);
    let sent = bora.send_request(&ayse_me.friend_code).await.unwrap();
    assert!(matches!(sent, RequestOutcome::Requested { .. }));
    expect(
        &mut ayse_presence,
        "request",
        |e| matches!(e, PresenceEvent::RequestReceived { from } if from.id == bora_id),
    )
    .await;
    let friend = ayse.accept_request(&bora_id).await.unwrap();
    assert_eq!(friend.profile.id, bora_id);
    expect(
        &mut ayse_presence,
        "friend added",
        |e| matches!(e, PresenceEvent::FriendAdded { friend, .. } if friend.id == bora_id),
    )
    .await;

    // Bora comes online and starts playing on projects.gg.
    let mut bora_presence = bora.connect_presence(options());
    match expect(&mut bora_presence, "connected", |e| {
        matches!(e, PresenceEvent::Connected { .. })
    })
    .await
    {
        PresenceEvent::Connected { friends, .. } => {
            assert!(friends.iter().any(|p| p.id == ayse_id));
        }
        _ => unreachable!(),
    }
    expect(&mut ayse_presence, "bora online", |e| {
        matches!(e, PresenceEvent::Presence(p) if p.id == bora_id && p.state == PresenceState::Launcher)
    })
    .await;
    bora_presence.set_status(LocalStatus::Playing {
        host: Some("projects.gg".into()),
    });
    match expect(&mut ayse_presence, "bora playing", |e| {
        matches!(e, PresenceEvent::Presence(p) if p.id == bora_id && p.state == PresenceState::Playing)
    })
    .await
    {
        PresenceEvent::Presence(p) => assert!(p.game.unwrap().joinable()),
        _ => unreachable!(),
    }

    // Birlikte oyna, then the per-friend limit.
    assert_eq!(
        bora_presence.invite(&ayse_id).await.unwrap(),
        InviteOutcome::Delivered
    );
    match expect(&mut ayse_presence, "invite", |e| {
        matches!(e, PresenceEvent::Invite(_))
    })
    .await
    {
        PresenceEvent::Invite(invite) => {
            assert_eq!(invite.from, bora_id);
            assert!(invite.server.unwrap().joinable());
        }
        _ => unreachable!(),
    }
    let limited = bora_presence.invite(&ayse_id).await.unwrap_err();
    assert!(matches!(
        limited,
        SocialError::RateLimited {
            retry_after: Some(_)
        }
    ));

    // Invisible: Ayşe sees Bora go offline.
    bora.update_profile(&ProfileUpdate::default().status(StatusMode::Invisible))
        .await
        .unwrap();
    expect(&mut ayse_presence, "bora invisible", |e| {
        matches!(e, PresenceEvent::Presence(p) if p.id == bora_id && p.state == PresenceState::Offline)
    })
    .await;

    // Settings backup with optimistic concurrency.
    let first = bora
        .put_settings(
            "Integration PC",
            &serde_json::json!({"theme": "dark"}),
            SettingsPrecondition::Create,
        )
        .await
        .unwrap();
    let second = bora
        .put_settings(
            "Laptop",
            &serde_json::json!({"theme": "light"}),
            SettingsPrecondition::Replace(first.version),
        )
        .await
        .unwrap();
    assert_eq!(second.version, first.version + 1);
    match bora
        .put_settings(
            "Integration PC",
            &serde_json::json!({"theme": "dark"}),
            SettingsPrecondition::Replace(first.version),
        )
        .await
    {
        Err(SocialError::SettingsConflict {
            current: Some(meta),
        }) => {
            assert_eq!(meta.device, "Laptop");
        }
        other => panic!("expected a conflict, got {other:?}"),
    }
    let backup = bora.settings().await.unwrap().unwrap();
    assert_eq!(backup.data["theme"], "light");
    assert_eq!(
        bora.me().await.unwrap().settings.unwrap().version,
        second.version
    );
    let secret = bora
        .put_settings(
            "PC",
            &serde_json::json!({"password": "x"}),
            SettingsPrecondition::Replace(second.version),
        )
        .await
        .unwrap_err();
    assert_eq!(secret.code(), Some(&ErrorCode::SettingsSecretKey));

    // Blocking ends the friendship on both sides at once.
    ayse.block(&bora_id).await.unwrap();
    expect(
        &mut ayse_presence,
        "friend removed",
        |e| matches!(e, PresenceEvent::FriendRemoved { id } if *id == bora_id),
    )
    .await;
    expect(
        &mut bora_presence,
        "friend removed",
        |e| matches!(e, PresenceEvent::FriendRemoved { id } if *id == ayse_id),
    )
    .await;
    let hidden = bora.lookup_code(&ayse_me.friend_code).await.unwrap_err();
    assert_eq!(hidden.code(), Some(&ErrorCode::CodeNotFound));

    // "Verilerimi sil" closes Bora's socket for good; logging out ends Ayşe's.
    bora.delete_account().await.unwrap();
    expect(&mut bora_presence, "stopped", |e| {
        *e == PresenceEvent::Stopped(StopReason::AccountDeleted)
    })
    .await;
    assert!(matches!(bora.me().await, Err(SocialError::Unauthorized)));
    ayse.logout().await.unwrap();
    expect(&mut ayse_presence, "stopped", |e| {
        *e == PresenceEvent::Stopped(StopReason::SignedOut)
    })
    .await;
}

#[tokio::test]
#[ignore = "needs `pnpm dev` and `pnpm dev:discord` in services/social"]
async fn sessions_codes_and_errors() {
    let client = client();
    let id = new_user_id(3);
    let first = sign_in(&client, &id, "deniz").await;
    let second = sign_in(&client, &id, "deniz").await;
    let sessions = second.sessions().await.unwrap();
    assert_eq!(sessions.len(), 2);
    assert_eq!(sessions.iter().filter(|s| s.current).count(), 1);
    assert_eq!(second.revoke_other_sessions().await.unwrap(), 1);
    assert!(matches!(first.me().await, Err(SocialError::Unauthorized)));

    let me = second.me().await.unwrap();
    let own = second.send_request(&me.friend_code).await.unwrap_err();
    assert_eq!(own.code(), Some(&ErrorCode::SelfTarget));
    let unknown = second.lookup_code("PRJ-0000-0000").await.unwrap_err();
    assert_eq!(unknown.code(), Some(&ErrorCode::CodeNotFound));
    // Malformed codes fail locally. ("not a code" would not: it normalizes to PRJ-N0TA-C0DE.)
    assert!(matches!(
        second.lookup_code("nope").await,
        Err(SocialError::InvalidInput(_))
    ));
    let rotated = second.rotate_friend_code().await.unwrap();
    assert_ne!(rotated, me.friend_code);
    assert_eq!(
        client.friend_share_link(&rotated).unwrap().path(),
        format!("/f/{rotated}")
    );

    // A claim needs the verifier and the one-time code of the same sign-in. Errors are typed so
    // the launcher can explain them; any failed claim uses the attempt up.
    let unknown = client.begin_login();
    let error = client
        .complete_login(&unknown, &"C".repeat(43), "PC")
        .await
        .unwrap_err();
    assert_eq!(error.code(), Some(&ErrorCode::AuthPendingNotFound));
    let attempt = client.begin_login();
    let code = approve_in_browser(&attempt, &id, "deniz").await;
    assert!(matches!(
        client.complete_login(&attempt, "not a code", "PC").await,
        Err(SocialError::InvalidInput(_))
    ));
    let wrong = client
        .complete_login(&attempt, &"C".repeat(43), "PC")
        .await
        .unwrap_err();
    assert_eq!(wrong.code(), Some(&ErrorCode::AuthPendingNotFound));
    let used_up = client
        .complete_login(&attempt, &code, "PC")
        .await
        .unwrap_err();
    assert_eq!(used_up.code(), Some(&ErrorCode::AuthPendingNotFound));
    second.delete_account().await.unwrap();
}

/// Crews against a local Worker: looks, codes, moderators and permissions, and "Ekibi çağır" where
/// the service offers them (`teamFeatures`); against a Worker from before them (point
/// `PL_SOCIAL_URL` at one), what the launcher relies on there: no features, today's look, codes
/// without a prefix.
#[tokio::test]
#[ignore = "needs `pnpm dev` and `pnpm dev:discord` in services/social"]
async fn crews_look_roles_and_calls() {
    let client = client();
    client.health().await.expect("wrangler dev is running");
    let leader_id = new_user_id(4);
    let mate_id = new_user_id(5);
    let leader = sign_in(&client, &leader_id, "lider").await;
    let mate = sign_in(&client, &mate_id, "uye").await;
    let features = leader.teams().await.unwrap().team_features;
    let look = features.iter().any(|f| f == TEAM_FEATURE_LOOK);
    let roles = features.iter().any(|f| f == TEAM_FEATURE_ROLES);
    let calls = features.iter().any(|f| f == TEAM_FEATURE_CALLS);
    let mute = features.iter().any(|f| f == TEAM_FEATURE_MUTE);
    let call_server = features.iter().any(|f| f == TEAM_FEATURE_CALL_SERVER);
    let multi = features.iter().any(|f| f == TEAM_FEATURE_MULTI);

    // Founding: the look goes only where the service has looks.
    let wanted = TeamLook {
        color: "teal".into(),
        frame: "banner".into(),
        motto: "Birlikte güçlüyüz!".into(),
    };
    let emblem = if look { "controller" } else { "sword" };
    let created = leader
        .create_team(
            "Gece Kartalları",
            "GKR",
            emblem,
            look.then_some(&wanted),
            multi,
        )
        .await
        .unwrap()
        .team
        .unwrap();
    if look {
        assert_eq!(
            (
                created.color.as_str(),
                created.frame.as_str(),
                created.motto.as_str()
            ),
            ("teal", "banner", "Birlikte güçlüyüz!")
        );
    } else {
        assert_eq!(
            (created.color.as_str(), created.frame.as_str()),
            ("gold", "cut")
        );
        assert!(created.motto.is_empty());
    }

    // The code as players see it (EKP-) finds the crew on every service: it goes without prefix.
    let shown = crew_code(&created.code);
    assert!(shown.starts_with("EKP-"));
    let lookup = mate.lookup_team(&shown, multi).await.unwrap();
    assert_eq!(lookup.team.id, created.id);
    assert_eq!(lookup.team.color, if look { "teal" } else { "gold" });
    let joined = mate.join_team(&shown, multi).await.unwrap().team.unwrap();
    assert_eq!(joined.members.len(), 2);

    if look {
        let edit = TeamEdit {
            color: Some("crimson".into()),
            motto: Some(String::new()),
            ..TeamEdit::default()
        };
        let team = leader
            .update_team(&created.id, &edit)
            .await
            .unwrap()
            .team
            .unwrap();
        assert_eq!((team.color.as_str(), team.motto.as_str()), ("crimson", ""));
        let bad = TeamEdit {
            motto: Some("discord.gg/abc".into()),
            ..TeamEdit::default()
        };
        let refused = leader.update_team(&created.id, &bad).await.unwrap_err();
        assert_eq!(refused.code(), Some(&ErrorCode::InvalidTeamMotto));
    }

    if roles {
        // The leader names a moderator and lets moderators edit the look.
        let named = leader
            .set_team_moderator(&created.id, &mate_id, true)
            .await
            .unwrap()
            .team
            .unwrap();
        assert!(
            named
                .members
                .iter()
                .any(|m| m.profile.id == mate_id && m.moderator)
        );
        let recolour = TeamEdit {
            color: Some("navy".into()),
            ..TeamEdit::default()
        };
        let not_yet = mate.update_team(&created.id, &recolour).await.unwrap_err();
        assert_eq!(not_yet.code(), Some(&ErrorCode::TeamNotPermitted));
        let open = TeamEdit {
            permissions: Some(PermissionsEdit {
                edit: Some(PermissionLevel::Moderators),
                ..PermissionsEdit::default()
            }),
            ..TeamEdit::default()
        };
        let opened = leader
            .update_team(&created.id, &open)
            .await
            .unwrap()
            .team
            .unwrap();
        assert_eq!(opened.permissions.edit, PermissionLevel::Moderators);
        let recoloured = mate
            .update_team(&created.id, &recolour)
            .await
            .unwrap()
            .team
            .unwrap();
        assert_eq!(recoloured.color, "navy");
        // The name stays the leader's.
        let rename = TeamEdit {
            name: Some("Başka Ad".into()),
            ..TeamEdit::default()
        };
        let refused = mate.update_team(&created.id, &rename).await.unwrap_err();
        assert_eq!(refused.code(), Some(&ErrorCode::TeamFounderOnly));
        leader
            .set_team_moderator(&created.id, &mate_id, false)
            .await
            .unwrap();
    }

    // "Ekibi çağır" on the socket, where the service has it.
    let mut leader_presence = leader.connect_presence(options());
    let mut mate_presence = mate.connect_presence(options());
    for presence in [&mut leader_presence, &mut mate_presence] {
        expect(presence, "connected", |e| {
            matches!(e, PresenceEvent::Connected { .. })
        })
        .await;
    }
    if calls {
        if call_server {
            // CALL1: not from the launcher; on another server only while it is shared.
            let refused = mate_presence.call_crew(None).await.unwrap_err();
            assert_eq!(refused.code(), Some(&ErrorCode::CallNeedsServer));
            mate_presence.set_status(LocalStatus::Playing {
                host: Some("mc.example.com".into()),
            });
            settle().await;
            let hidden = mate_presence.call_crew(None).await.unwrap_err();
            assert_eq!(hidden.code(), Some(&ErrorCode::CallServerHidden));
            mate_presence.set_status(LocalStatus::Playing {
                host: Some("play.projects.gg".into()),
            });
            settle().await;
        }
        mate_presence.call_crew(None).await.unwrap();
        let call = match expect(&mut leader_presence, "the call", |e| {
            matches!(e, PresenceEvent::Call { .. })
        })
        .await
        {
            PresenceEvent::Call { call, quiet, clock } => {
                assert!(!quiet, "a teammate's live call is announced");
                assert!(clock.is_some(), "the hub's clock comes with it");
                assert_eq!(call.from, mate_id);
                if call_server {
                    // Always "join", with where it is and (several crews) the crews it names.
                    assert_eq!(call.kind, CallKind::Join);
                    assert_eq!(
                        call.server.as_ref().map(|s| s.location),
                        Some(GameLocation::Projects)
                    );
                } else {
                    assert_eq!(call.kind, CallKind::Ready);
                }
                if multi {
                    assert_eq!(call.crews, vec![created.id.clone()]);
                }
                assert_eq!(call.until - call.at, 20 * 60);
                call
            }
            _ => unreachable!(),
        };
        leader_presence
            .answer_call(&call.id, CallAnswer::Yes)
            .await
            .unwrap();
        expect(&mut mate_presence, "the answer", |e| {
            matches!(e, PresenceEvent::CallAnswered { by, answer: CallAnswer::Yes, .. } if *by == leader_id)
        })
        .await;
        if !call_server {
            // The caller goes to projects.gg: the call turns into a "join" call.
            mate_presence.set_status(LocalStatus::Playing {
                host: Some("play.projects.gg".into()),
            });
            expect(
                &mut leader_presence,
                "the join call",
                |e| matches!(e, PresenceEvent::Call { call, .. } if call.kind == CallKind::Join),
            )
            .await;
        }
        // One call at a time, and not again so soon (from a server: CALL1 comes first).
        if call_server {
            leader_presence.set_status(LocalStatus::Playing {
                host: Some("play.projects.gg".into()),
            });
            settle().await;
        }
        let again = leader_presence.call_crew(None).await.unwrap_err();
        assert_eq!(again.code(), Some(&ErrorCode::CallActive));
        mate_presence.end_call(&call.id).await.unwrap();
        expect(
            &mut leader_presence,
            "the end",
            |e| matches!(e, PresenceEvent::CallEnded { call: id, .. } if *id == call.id),
        )
        .await;
        let soon = mate_presence.call_crew(None).await.unwrap_err();
        assert!(matches!(
            soon,
            SocialError::RateLimited {
                retry_after: Some(_)
            }
        ));
        let gone = leader_presence
            .answer_call(&call.id, CallAnswer::No)
            .await
            .unwrap_err();
        assert_eq!(gone.code(), Some(&ErrorCode::CallNotFound));
    } else {
        // A service without calls refuses the message (the launcher never sends it there).
        let refused = mate_presence.call_crew(None).await.unwrap_err();
        assert_eq!(refused.code(), Some(&ErrorCode::BadRequest));
    }

    // "Sessize al": the player's own settings, on all their devices; a muted player's crew calls
    // arrive quiet (and still arrive).
    let settings = mate.teams().await.unwrap().team_notifications;
    if mute {
        assert_eq!(settings, Some(TeamNotifications::default()));
        let now = i64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        )
        .unwrap();
        let hour = TeamNotificationsEdit {
            muted: Some(true),
            mute_until: Some(now + 3_600),
            ..TeamNotificationsEdit::default()
        };
        let muted = mate.set_team_notifications(&hour).await.unwrap();
        assert!(muted.calls && muted.muted && muted.mute_until == Some(now + 3_600));
        expect(
            &mut mate_presence,
            "the settings on the socket",
            |e| matches!(e, PresenceEvent::CrewNotifications(settings) if settings.muted),
        )
        .await;
        // The same again changes nothing; an end without a mute is refused.
        assert_eq!(mate.set_team_notifications(&hour).await.unwrap(), muted);
        let stray = TeamNotificationsEdit {
            mute_until: Some(now + 3_600),
            ..TeamNotificationsEdit::default()
        };
        let refused = mate.set_team_notifications(&stray).await.unwrap_err();
        assert_eq!(refused.code(), Some(&ErrorCode::BadRequest));
        if calls {
            leader_presence.call_crew(None).await.unwrap();
            match expect(
                &mut mate_presence,
                "the muted call",
                |e| matches!(e, PresenceEvent::Call { call, .. } if call.from == leader_id),
            )
            .await
            {
                PresenceEvent::Call { quiet, .. } => {
                    assert!(quiet, "a muted player's call is quiet")
                }
                _ => unreachable!(),
            }
        }
        let open = mate
            .set_team_notifications(&TeamNotificationsEdit {
                muted: Some(false),
                ..TeamNotificationsEdit::default()
            })
            .await
            .unwrap();
        assert_eq!(open, TeamNotifications::default());
        let forever = mate
            .set_team_notifications(&TeamNotificationsEdit {
                muted: Some(true),
                ..TeamNotificationsEdit::default()
            })
            .await
            .unwrap();
        assert!(forever.muted && forever.mute_until.is_none());
        let calls_off = mate
            .set_team_notifications(&TeamNotificationsEdit {
                calls: Some(false),
                muted: Some(false),
                ..TeamNotificationsEdit::default()
            })
            .await
            .unwrap();
        assert!(!calls_off.calls && !calls_off.muted);
        // The friend list carries them too (outside a crew as well).
        let listed = mate.friends().await.unwrap().team_notifications.unwrap();
        assert_eq!(listed, calls_off);
    } else {
        assert_eq!(settings, None);
        // An older service has no such route (the launcher never calls it there).
        let edit = TeamNotificationsEdit {
            muted: Some(true),
            ..TeamNotificationsEdit::default()
        };
        assert!(mate.set_team_notifications(&edit).await.is_err());
    }

    mate.delete_account().await.unwrap();
    leader.delete_account().await.unwrap();
}

/// Several crews (`multi`): founding and joining one more, the order and the primary crew, a
/// mute per crew, and one call to two crews that reaches each crewmate through their own.
#[tokio::test]
#[ignore = "needs `pnpm dev` and `pnpm dev:discord` in services/social"]
async fn several_crews_their_order_mutes_and_calls() {
    let client = client();
    client.health().await.expect("wrangler dev is running");
    let leader_id = new_user_id(6);
    let mate_id = new_user_id(7);
    let leader = sign_in(&client, &leader_id, "lider").await;
    let mate = sign_in(&client, &mate_id, "uye").await;
    let overview = leader.teams().await.unwrap();
    if !overview
        .team_features
        .iter()
        .any(|f| f == TEAM_FEATURE_MULTI)
    {
        eprintln!("this service has one crew per player: nothing to try");
        mate.delete_account().await.unwrap();
        leader.delete_account().await.unwrap();
        return;
    }
    let limit = overview
        .crew_limit
        .expect("a service with several crews says its limit");
    assert!((1..=10).contains(&limit));

    // Two crews of the leader's; without `multi` the second is refused the old way.
    let first = leader
        .create_team("Gece Kartalları", "GKR", "sword", None, true)
        .await
        .unwrap()
        .team
        .unwrap();
    let old_way = leader
        .create_team("PvP", "PVP", "bolt", None, false)
        .await
        .unwrap_err();
    assert_eq!(old_way.code(), Some(&ErrorCode::AlreadyInTeam));
    let second = leader
        .create_team("PvP", "PVP", "bolt", None, true)
        .await
        .unwrap()
        .team
        .unwrap();
    let crews = leader.friends().await.unwrap();
    let ids: Vec<String> = crews
        .crews
        .as_ref()
        .unwrap()
        .iter()
        .map(|c| c.team.id.clone())
        .collect();
    assert_eq!(ids, vec![first.id.clone(), second.id.clone()]);
    // The primary crew is the first; older launchers read it as `team`.
    assert_eq!(crews.team.unwrap().id, first.id);

    // The mate joins the first crew by its code (one crew, below the limit).
    let shown = crew_code(&first.code);
    assert_eq!(
        mate.lookup_team(&shown, true).await.unwrap().relation,
        TeamRelation::None
    );
    mate.join_team(&shown, true).await.unwrap();

    // The leader puts PvP first: it becomes the primary crew, on every device.
    let mut leader_presence = leader.connect_presence(options());
    let mut mate_presence = mate.connect_presence(options());
    for presence in [&mut leader_presence, &mut mate_presence] {
        expect(presence, "connected", |e| {
            matches!(e, PresenceEvent::Connected { .. })
        })
        .await;
    }
    let order = vec![second.id.clone(), first.id.clone()];
    assert_eq!(leader.order_crews(&order).await.unwrap(), order);
    expect(
        &mut leader_presence,
        "the order on the socket",
        |e| matches!(e, PresenceEvent::CrewOrder(o) if *o == order),
    )
    .await;
    assert_eq!(leader.teams().await.unwrap().team.unwrap().id, second.id);
    let stale = leader
        .order_crews(std::slice::from_ref(&first.id))
        .await
        .unwrap_err();
    assert_eq!(stale.code(), Some(&ErrorCode::CrewsChanged));

    // The mate mutes the crew for an hour: on the socket, and in the list.
    let now = i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    )
    .unwrap();
    let hour = CrewMuteEdit {
        muted: true,
        mute_until: Some(now + 3_600),
    };
    let muted = mate.set_crew_mute(&first.id, &hour).await.unwrap();
    assert!(muted.muted && muted.mute_until == Some(now + 3_600));
    expect(
        &mut mate_presence,
        "the crew's mute on the socket",
        |e| matches!(e, PresenceEvent::CrewMuted { crew, notifications } if *crew == first.id && notifications.muted),
    )
    .await;
    let listed = mate.friends().await.unwrap().crews.unwrap();
    assert!(listed[0].notifications.muted);

    // One call to both crews from Projects: the mate sees it through the one they share, quiet
    // while that crew is muted.
    leader_presence.set_status(LocalStatus::Playing {
        host: Some("play.projects.gg".into()),
    });
    settle().await;
    leader_presence
        .call_crew(Some(vec![first.id.clone(), second.id.clone()]))
        .await
        .unwrap();
    let call = match expect(
        &mut mate_presence,
        "the call",
        |e| matches!(e, PresenceEvent::Call { call, .. } if call.from == leader_id),
    )
    .await
    {
        PresenceEvent::Call { call, quiet, .. } => {
            assert!(quiet, "the only crew it names for the mate is muted");
            assert_eq!(call.crews, vec![first.id.clone()]);
            call
        }
        _ => unreachable!(),
    };
    // The mate leaves that crew: the call ends for them.
    mate.leave_team(&first.id).await.unwrap();
    expect(
        &mut mate_presence,
        "the call's end",
        |e| matches!(e, PresenceEvent::CallEnded { call: id, reason: CallEndReason::Left } if *id == call.id),
    )
    .await;
    // The leader goes back to the launcher: their call ends with the server reason.
    leader_presence.set_status(LocalStatus::Launcher);
    expect(&mut leader_presence, "the end of the own call", |e| {
        matches!(
            e,
            PresenceEvent::CallEnded {
                reason: CallEndReason::Server,
                ..
            }
        )
    })
    .await;

    mate.delete_account().await.unwrap();
    leader.delete_account().await.unwrap();
}

/// Chat (CHAT1) and reports (MOD1): direct messages both ways with their seal and key, a resend
/// that gives the message already taken, texts the service turns away, an edit, a delete for
/// everyone, a new device's sync, crew chat where the leader deletes anyone's message, a block
/// that ends direct messages, and reports of a message, a player and a crew.
#[tokio::test]
#[ignore = "needs `pnpm dev` and `pnpm dev:discord` in services/social"]
async fn chat_messages_edits_deletes_syncs_and_reports() {
    let client = client();
    client.health().await.expect("wrangler dev is running");
    let ayse_id = new_user_id(21);
    let bora_id = new_user_id(22);
    let ayse = sign_in(&client, &ayse_id, "ayse").await;
    let bora = sign_in(&client, &bora_id, "bora").await;
    let features = ayse.friends().await.unwrap().features;
    assert!(
        features.iter().any(|f| f == FEATURE_CHAT),
        "chat is on (CHAT_MAC_KEY in .dev.vars)"
    );

    // Direct messages go only between friends.
    let ayse_me = ayse.me().await.unwrap();
    bora.send_request(&ayse_me.friend_code).await.unwrap();
    ayse.accept_request(&bora_id).await.unwrap();
    let (mut ayse_live, chat) = connect_chat(&ayse).await;
    assert!(chat.on && chat.mailbox);
    assert_eq!((chat.per_day, chat.muted_until), (500, None));
    let (mut bora_live, _) = connect_chat(&bora).await;
    let ayse_chat = ayse_live.control();
    let bora_chat = bora_live.control();
    let conv = direct_conversation(&ayse_id, &bora_id);

    // Ayşe writes: the service cleans the text and seals it; Bora gets it without her key.
    let sent = ayse_chat
        .chat_send(&conv, "  Akşam bedwars'a   girelim mi? ", "live1a")
        .await
        .unwrap();
    assert_eq!(sent.text, "Akşam bedwars'a girelim mi?");
    assert_eq!(sent.mac.len(), 22);
    assert_eq!(sent.nonce.as_deref(), Some("live1a"));
    let got = match expect(
        &mut bora_live,
        "the message",
        |e| matches!(e, PresenceEvent::ChatMessage(m) if m.id == sent.id),
    )
    .await
    {
        PresenceEvent::ChatMessage(message) => message,
        _ => unreachable!(),
    };
    assert_eq!(
        (got.conv.as_str(), got.from.as_str(), got.text.as_str()),
        (conv.as_str(), ayse_id.as_str(), sent.text.as_str())
    );
    assert_eq!(got.nonce, None, "the key goes only to the sender's devices");
    // The same key again (as after an answer lost with the connection): the message taken.
    let again = ayse_chat
        .chat_send(&conv, "Akşam bedwars'a girelim mi?", "live1a")
        .await
        .unwrap();
    assert_eq!(again.id, sent.id);

    // Bora answers; ids are the order.
    let answer = bora_chat
        .chat_send(&conv, "Olur, 9'da", "live1b")
        .await
        .unwrap();
    assert!(answer.id > sent.id);
    expect(
        &mut ayse_live,
        "the answer",
        |e| matches!(e, PresenceEvent::ChatMessage(m) if m.id == answer.id),
    )
    .await;

    // What the service turns away.
    pace().await;
    for (text, nonce, wanted) in [
        ("discord.gg/abc", "live1r1", ChatRejection::Link),
        ("0532 123 45 67", "live1r2", ChatRejection::Phone),
    ] {
        let refused = ayse_chat.chat_send(&conv, text, nonce).await.unwrap_err();
        assert!(
            matches!(refused, SocialError::ChatRejected { reason } if reason == wanted),
            "{text}: {refused:?}"
        );
    }

    // Ayşe edits hers: the same id, a new seal, "düzenlendi"; Bora gets the new text.
    pace().await;
    let edited = ayse_chat
        .chat_edit(&sent, "Akşam bedwars'a girelim mi? 9'da")
        .await
        .unwrap();
    assert_eq!(edited.id, sent.id);
    assert!(edited.edited_at.is_some());
    assert_ne!(edited.mac, sent.mac);
    match expect(
        &mut bora_live,
        "the edit",
        |e| matches!(e, PresenceEvent::ChatEdited(m) if m.id == sent.id),
    )
    .await
    {
        PresenceEvent::ChatEdited(message) => {
            assert_eq!(message.text, edited.text);
            assert_eq!(message.edited_at, edited.edited_at);
        }
        _ => unreachable!(),
    }
    // Bora's message is not hers to edit.
    let not_hers = ayse_chat.chat_edit(&answer, "Olur").await.unwrap_err();
    assert!(
        matches!(
            not_hers,
            SocialError::Refused {
                code: ErrorCode::ChatNotPermitted,
                ..
            }
        ),
        "{not_hers:?}"
    );

    // "Rapor et" on Bora's message as Ayşe's device holds it, with what came before; the same
    // target again while the first is open is a duplicate. Then the player.
    let report = ayse
        .report(
            json!({ "kind": "message", "message": answer.sealed(), "before": [edited.sealed()] }),
            ReportReason::Inappropriate,
            Some("canlı test"),
            false,
        )
        .await
        .unwrap();
    assert!(!report.duplicate && !report.id.is_empty());
    let duplicate = ayse
        .report(
            json!({ "kind": "message", "message": answer.sealed(), "before": [] }),
            ReportReason::Spam,
            None,
            false,
        )
        .await
        .unwrap();
    assert!(duplicate.duplicate);
    let player = ayse
        .report(
            json!({ "kind": "player", "id": bora_id }),
            ReportReason::Harassment,
            None,
            false,
        )
        .await
        .unwrap();
    assert!(!player.duplicate);

    // Ayşe deletes hers for everyone: Bora is told who did; her socket gets only the answer.
    ayse_chat.chat_delete(&edited).await.unwrap();
    match expect(
        &mut bora_live,
        "the delete",
        |e| matches!(e, PresenceEvent::ChatDeleted { id, .. } if *id == sent.id),
    )
    .await
    {
        PresenceEvent::ChatDeleted { conv: at, by, .. } => {
            assert_eq!(at, conv);
            assert_eq!(by, ayse_id);
        }
        _ => unreachable!(),
    }

    // Bora's new computer syncs from nothing: his message with his key, her deletion.
    let bora_new = sign_in(&client, &bora_id, "bora").await;
    let mut bora_new_live = bora_new.connect_presence_with_chat(options(), chat_from(0));
    expect(&mut bora_new_live, "connected", |e| {
        matches!(e, PresenceEvent::Connected { chat: Some(_), .. })
    })
    .await;
    match expect(
        &mut bora_new_live,
        "the conversation's sync",
        |e| matches!(e, PresenceEvent::ChatSync { conv: at, .. } if *at == conv),
    )
    .await
    {
        PresenceEvent::ChatSync {
            messages, deleted, ..
        } => {
            assert!(
                messages.iter().any(|m| m.id == answer.id
                    && m.conv == conv
                    && m.nonce.as_deref() == Some("live1b")),
                "{messages:?}"
            );
            assert!(messages.iter().all(|m| m.id != sent.id));
            assert!(deleted.iter().any(|d| d.id == sent.id), "{deleted:?}");
        }
        _ => unreachable!(),
    }
    expect(&mut bora_new_live, "the sync's end", |e| {
        matches!(e, PresenceEvent::ChatSynced { .. })
    })
    .await;
    bora_new_live.shutdown().await;

    // Crew chat: everyone in the crew gets it; the leader deletes anyone's message.
    let multi = ayse
        .teams()
        .await
        .unwrap()
        .team_features
        .iter()
        .any(|f| f == TEAM_FEATURE_MULTI);
    let crew = ayse
        .create_team("Gece Kartalları", "GKR", "sword", None, multi)
        .await
        .unwrap()
        .team
        .unwrap();
    bora.join_team(&crew_code(&crew.code), multi).await.unwrap();
    let crew_conv = crew_conversation(&crew.id);
    expect(&mut ayse_live, "bora in the crew", |e| {
        matches!(e, PresenceEvent::TeamChanged { team: Some(team), .. } if team.members.len() == 2)
    })
    .await;
    pace().await;
    let coming = bora_chat
        .chat_send(&crew_conv, "Geliyorum", "live1c")
        .await
        .unwrap();
    expect(
        &mut ayse_live,
        "the crew message",
        |e| matches!(e, PresenceEvent::ChatMessage(m) if m.id == coming.id && m.conv == crew_conv),
    )
    .await;
    ayse_chat.chat_delete(&coming).await.unwrap();
    expect(
        &mut bora_live,
        "deleted by the leader",
        |e| matches!(e, PresenceEvent::ChatDeleted { id, by, .. } if *id == coming.id && *by == ayse_id),
    )
    .await;
    let crew_report = bora
        .report(
            json!({ "kind": "crew", "id": crew.id }),
            ReportReason::Name,
            None,
            false,
        )
        .await
        .unwrap();
    assert!(!crew_report.duplicate);

    // A block ends direct messages both ways.
    bora.block(&ayse_id).await.unwrap();
    pace().await;
    let blocked = ayse_chat
        .chat_send(&conv, "Orada mısın?", "live1d")
        .await
        .unwrap_err();
    assert!(
        matches!(blocked, SocialError::Refused { .. }),
        "{blocked:?}"
    );

    ayse_live.shutdown().await;
    bora_live.shutdown().await;
    bora.delete_account().await.unwrap();
    ayse.delete_account().await.unwrap();
}

/// A Discord account made just now is limited (SPAM1): the profile says until when and what; a
/// crew cannot be founded, the settings backup is off, code lookups stop at 10 a day and chat
/// at 100 messages; reports still go.
#[tokio::test]
#[ignore = "needs `pnpm dev` and `pnpm dev:discord` in services/social"]
async fn limited_accounts_and_their_limits() {
    let client = client();
    client.health().await.expect("wrangler dev is running");
    let fresh_id = new_discord_account_id(31);
    let fresh = sign_in(&client, &fresh_id, "yeni").await;
    let limited = fresh
        .me()
        .await
        .unwrap()
        .limited
        .expect("a new account is limited");
    assert!(limited.until > unix_now());
    let per_day = |key: LimitedKey| {
        limited
            .items
            .iter()
            .find(|item| item.key == key)
            .map(|item| item.per_day)
    };
    assert_eq!(per_day(LimitedKey::CrewFound), Some(None));
    assert_eq!(per_day(LimitedKey::CrewInvites), Some(Some(5)));
    assert_eq!(per_day(LimitedKey::CodeLookups), Some(Some(10)));
    assert_eq!(per_day(LimitedKey::SettingsBackup), Some(None));
    let chat = fresh
        .friends()
        .await
        .unwrap()
        .features
        .iter()
        .any(|f| f == FEATURE_CHAT);
    assert_eq!(per_day(LimitedKey::ChatMessages), chat.then_some(Some(100)));

    // Said where it happens, with the item and the date.
    let multi = fresh
        .teams()
        .await
        .unwrap()
        .team_features
        .iter()
        .any(|f| f == TEAM_FEATURE_MULTI);
    let found = fresh
        .create_team("Yeni Ekip", "YNE", "sword", None, multi)
        .await
        .unwrap_err();
    assert!(
        matches!(
            found,
            SocialError::Limited { item: LimitedKey::CrewFound, until: Some(until), .. }
                if until == limited.until
        ),
        "{found:?}"
    );
    let backup = fresh
        .put_settings(
            "Integration PC",
            &json!({ "v": 1 }),
            SettingsPrecondition::Create,
        )
        .await
        .unwrap_err();
    assert!(
        matches!(
            backup,
            SocialError::Limited {
                item: LimitedKey::SettingsBackup,
                ..
            }
        ),
        "{backup:?}"
    );
    // Ten code lookups a day (unknown codes count too), then until tomorrow.
    let mut stopped = None;
    for attempt in 0..12 {
        match fresh.lookup_code("PRJ-ZZZZ-ZZZZ").await {
            Err(SocialError::Limited {
                item: LimitedKey::CodeLookups,
                retry_after,
                ..
            }) => {
                stopped = Some((attempt, retry_after));
                break;
            }
            Err(_) => {}
            Ok(found) => panic!("nobody has this code: {found:?}"),
        }
    }
    let (attempt, retry_after) = stopped.expect("lookups stop");
    assert_eq!(attempt, 10);
    assert!(retry_after.is_some());

    if chat {
        let (live, chat) = connect_chat(&fresh).await;
        assert_eq!(chat.per_day, 100);
        live.shutdown().await;
    }

    fresh.delete_account().await.unwrap();
}

/// The crews' second wave (TEAM5): a crew that approves newcomers, a join request by invite link
/// that reaches the leader live and is taken, a withdrawn request, the link renewed when the crew
/// opens again (the old one stops working), a plan made, answered and cancelled over the socket.
#[tokio::test]
#[ignore = "needs `pnpm dev` and `pnpm dev:discord` in services/social"]
async fn crew_join_requests_links_and_plans() {
    let client = client();
    client.health().await.expect("wrangler dev is running");
    let leader_id = new_user_id(11);
    let asker_id = new_user_id(12);
    let other_id = new_user_id(13);
    let leader = sign_in(&client, &leader_id, "lider").await;
    let asker = sign_in(&client, &asker_id, "istekli").await;
    let other = sign_in(&client, &other_id, "baska").await;
    let features = leader.teams().await.unwrap().team_features;
    let has = |name: &str| features.iter().any(|f| f == name);
    if !(has(TEAM_FEATURE_REQUESTS) && has(TEAM_FEATURE_LINK) && has(TEAM_FEATURE_PLANS)) {
        eprintln!("this service has no join requests, links or plans: nothing to try");
        for session in [other, asker, leader] {
            session.delete_account().await.unwrap();
        }
        return;
    }
    let crew = leader
        .create_team("Gece Kartalları", "GKR", "sword", None, true)
        .await
        .unwrap()
        .team
        .unwrap();
    assert_eq!(crew.join, JoinMode::Open);
    assert_eq!(crew.permissions.admit, PermissionLevel::Moderators);
    let approval = TeamEdit {
        join: Some(JoinMode::Approval),
        ..TeamEdit::default()
    };
    leader.update_team(&crew.id, &approval).await.unwrap();

    // The invite link: 12 symbols on the service's own page.
    let made = leader.create_team_link(&crew.id).await.unwrap();
    assert_eq!(
        normalize_crew_link(&made.link).as_deref(),
        Some(made.link.as_str())
    );
    assert!(
        made.url.ends_with(&format!("/e/{}", made.link)),
        "{}",
        made.url
    );
    let link = CrewKey::Link(made.link.to_lowercase().replace('-', " "));

    let mut leader_presence = leader.connect_presence(options());
    expect(&mut leader_presence, "the leader connected", |e| {
        matches!(e, PresenceEvent::Connected { plans: Some(_), .. })
    })
    .await;

    // The asker finds the crew by its link and asks: the leader hears it live.
    let found = asker.lookup_crew(&link, true).await.unwrap();
    assert_eq!(found.team.id, crew.id);
    assert_eq!(found.team.join, JoinMode::Approval);
    assert_eq!(found.relation, TeamRelation::None);
    assert!(found.request.is_none());
    let sent_at = match asker.join_crew(&link, true, true).await.unwrap() {
        JoinOutcome::Requested(request) => {
            assert_eq!(request.team.id, crew.id);
            request.sent_at
        }
        JoinOutcome::Joined(_) => panic!("an approving crew takes a request"),
    };
    // Asking again answers the same request.
    match asker.join_crew(&link, true, true).await.unwrap() {
        JoinOutcome::Requested(again) => assert_eq!(again.sent_at, sent_at),
        JoinOutcome::Joined(_) => panic!("still a request"),
    }
    // Without `request` an approving crew says so (what older launchers get).
    let old_way = other.join_crew(&link, true, false).await.unwrap_err();
    assert_eq!(old_way.code(), Some(&ErrorCode::TeamApprovalRequired));
    expect(&mut leader_presence, "the request on the socket", |e| {
        matches!(
            e,
            PresenceEvent::TeamRequests { crew: id, requests, added }
                if *id == crew.id
                    && requests.len() == 1
                    && added.as_deref() == Some(asker_id.as_str())
        )
    })
    .await;
    let waiting = asker.lookup_crew(&link, true).await.unwrap();
    assert_eq!(waiting.request.map(|r| r.sent_at), Some(sent_at));
    let own = asker.teams().await.unwrap().team_requests;
    assert_eq!(own.len(), 1);
    assert_eq!(own[0].team.id, crew.id);
    let listed = leader.friends().await.unwrap().crews.unwrap();
    assert_eq!(listed[0].requests.len(), 1);
    assert_eq!(listed[0].requests[0].from.id, asker_id);

    // A request by code, withdrawn.
    let code = CrewKey::Code(crew_code(&crew.code));
    assert!(matches!(
        other.join_crew(&code, true, true).await.unwrap(),
        JoinOutcome::Requested(_)
    ));
    other.withdraw_team_request(&crew.id).await.unwrap();
    assert!(other.teams().await.unwrap().team_requests.is_empty());
    let gone = other.withdraw_team_request(&crew.id).await.unwrap_err();
    assert_eq!(gone.code(), Some(&ErrorCode::TeamRequestNotFound));

    // The leader takes the asker in.
    let taken = leader
        .accept_team_request(&crew.id, &asker_id)
        .await
        .unwrap();
    assert!(
        taken
            .team
            .unwrap()
            .members
            .iter()
            .any(|m| m.profile.id == asker_id)
    );
    expect(&mut leader_presence, "the emptied list", |e| {
        matches!(
            e,
            PresenceEvent::TeamRequests { crew: id, requests, .. }
                if *id == crew.id && requests.is_empty()
        )
    })
    .await;
    assert!(asker.teams().await.unwrap().team_requests.is_empty());

    // Open again: the link is renewed, the old one looks unknown, the new one lets anyone in.
    let open = TeamEdit {
        join: Some(JoinMode::Open),
        ..TeamEdit::default()
    };
    let renewed = leader
        .update_team(&crew.id, &open)
        .await
        .unwrap()
        .team
        .unwrap()
        .link
        .expect("the crew keeps a link");
    assert_ne!(renewed, made.link);
    let stale = other.lookup_crew(&link, true).await.unwrap_err();
    assert_eq!(stale.code(), Some(&ErrorCode::TeamNotFound));
    assert!(matches!(
        other
            .join_crew(&CrewKey::Link(renewed.clone()), true, true)
            .await
            .unwrap(),
        JoinOutcome::Joined(_)
    ));

    // A plan: made by the leader, seen by the asker (not quiet), answered, then cancelled.
    let mut asker_presence = asker.connect_presence(options());
    expect(&mut asker_presence, "the asker connected", |e| {
        matches!(e, PresenceEvent::Connected { plans: Some(_), .. })
    })
    .await;
    let at = (unix_now() / 900 + 5) * 900;
    let plan = leader_presence
        .control()
        .make_plan(&crew.id, at)
        .await
        .unwrap();
    assert_eq!(plan.at, at);
    assert_eq!(plan.until, at + 7_200);
    assert_eq!(plan.answers.get(&leader_id), Some(&PlanAnswer::Yes));
    let seen = expect(
        &mut asker_presence,
        "the new plan",
        |e| matches!(e, PresenceEvent::Plan { plan: p, .. } if p.id == plan.id),
    )
    .await;
    assert!(matches!(seen, PresenceEvent::Plan { quiet: false, .. }));
    let twice = leader_presence
        .control()
        .make_plan(&crew.id, at)
        .await
        .unwrap_err();
    assert_eq!(twice.code(), Some(&ErrorCode::PlanExists));
    let odd = leader_presence
        .control()
        .make_plan(&crew.id, at + 60)
        .await
        .unwrap_err();
    assert_eq!(odd.code(), Some(&ErrorCode::PlanTime));
    asker_presence
        .control()
        .answer_plan(&plan.id, PlanAnswer::Maybe)
        .await
        .unwrap();
    expect(&mut leader_presence, "the answer", |e| {
        matches!(
            e,
            PresenceEvent::Plan { plan: p, .. }
                if p.answers.get(&asker_id) == Some(&PlanAnswer::Maybe)
        )
    })
    .await;
    // Only its maker or the crew's leaders cancel it.
    let not_theirs = asker_presence
        .control()
        .cancel_plan(&plan.id)
        .await
        .unwrap_err();
    assert_eq!(not_theirs.code(), Some(&ErrorCode::TeamNotPermitted));
    leader_presence
        .control()
        .cancel_plan(&plan.id)
        .await
        .unwrap();
    expect(
        &mut asker_presence,
        "the plan's end",
        |e| matches!(e, PresenceEvent::PlanRemoved { plan: id, .. } if *id == plan.id),
    )
    .await;

    // The link off: nobody gets in by it any more.
    leader.delete_team_link(&crew.id).await.unwrap();
    assert!(leader.teams().await.unwrap().team.unwrap().link.is_none());

    asker_presence.shutdown().await;
    leader_presence.shutdown().await;
    for session in [other, asker, leader] {
        session.delete_account().await.unwrap();
    }
}

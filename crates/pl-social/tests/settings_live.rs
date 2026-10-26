//! How fast a player's friends settings show and spread (SOC-LAG), through the launcher's own
//! friends service against a local `wrangler dev` of the service (the stack of `tests/live.rs`;
//! `PL_SOCIAL_URL` names it, never the production service). Ayşe runs two launchers (two
//! computers), Bora one; they are friends and share a crew. For every setting the test times, from
//! the moment Ayşe changes it: her own view, the service's answer, her other computer, Bora's
//! friend list and Bora's crew page. It prints the table and then checks the promise: her own view
//! at once, everyone else within two seconds of the service's answer.
//!
//! ```sh
//! cargo test -p pl-social --test settings_live -- --ignored --nocapture
//! ```

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use pl_social::{
    Connection, GameLocation, LastSeen, LoginAttempt, MemoryTokenStore, PresenceState,
    ProfileUpdate, ServiceConfig, Session, SocialClient, SocialConfig, SocialService, SocialUpdate,
    SocialView, StatusMode, TokenStore, UpdateSink, auth_code_from_deep_link, crew_code,
    direct_conversation,
};
use url::Url;

/// The local service (`PL_SOCIAL_URL`), never another.
fn base_url() -> Url {
    let base = std::env::var("PL_SOCIAL_URL").unwrap_or_else(|_| "http://127.0.0.1:8797/".into());
    let base = Url::parse(&base).expect("PL_SOCIAL_URL is a URL");
    assert!(
        matches!(base.host_str(), Some("127.0.0.1" | "localhost")),
        "integration tests only run against a local wrangler dev"
    );
    base
}

fn client() -> SocialClient {
    SocialClient::new(SocialConfig::new(
        base_url(),
        "ProjectsLauncher/settings-test",
    ))
    .unwrap()
}

fn new_user_id(n: u32) -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    format!("9{millis}{n:03}")
}

/// Plays the browser of a sign-in against the fake Discord; returns the one-time code.
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
    let page = browser
        .get(approve.headers()["location"].to_str().unwrap())
        .header("cookie", cookie)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let start = page
        .find("href=\"projects://auth/done?c=")
        .expect("the page links back to the launcher")
        + "href=\"".len();
    let end = start + page[start..].find('"').unwrap();
    auth_code_from_deep_link(&page[start..end]).unwrap()
}

async fn sign_in(client: &SocialClient, user_id: &str, name: &str, device: &str) -> Session {
    let attempt = client.begin_login();
    let code = approve_in_browser(&attempt, user_id, name).await;
    let signed_in = client
        .complete_login(&attempt, &code, device)
        .await
        .unwrap();
    client.session(signed_in.token)
}

/// What one launcher told its UI, each with the moment it came.
type Log = Arc<Mutex<Vec<(Instant, SocialUpdate)>>>;

/// A launcher signed in as `session`'s player, with chat and crews on; waits until it is online.
async fn launcher(session: &Session, chat_dir: &Path) -> (SocialService, Log) {
    let tokens = Arc::new(MemoryTokenStore::new());
    tokens.save(session.token()).unwrap();
    let log = Log::default();
    let sink: UpdateSink = {
        let log = log.clone();
        Arc::new(move |update: SocialUpdate| log.lock().unwrap().push((Instant::now(), update)))
    };
    let service = SocialService::new(
        reqwest::Client::new(),
        ServiceConfig {
            user_agent: "ProjectsLauncher/settings-test".into(),
            device: "Settings PC".into(),
            presence: Default::default(),
        },
        tokens,
        tokio::runtime::Handle::current(),
        sink,
    );
    service.keep_chat_in(chat_dir);
    service.set_chat_enabled(true);
    service.set_teams_enabled(true);
    service.configure(Some(base_url()));
    until(&service, "online", |view| {
        view.loaded && view.teams && view.connection == Connection::Online && view.chat.is_some()
    })
    .await;
    (service, log)
}

/// Waits (at most 15 s) until the launcher's view is as `wanted`.
async fn until(service: &SocialService, what: &str, wanted: impl Fn(&SocialView) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !wanted(&service.view()) {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// From `t0`, the time to the first update after it that `pick` accepts, waiting at most `limit`.
async fn first(
    log: &Log,
    t0: Instant,
    limit: Duration,
    pick: impl Fn(&SocialUpdate) -> bool,
) -> Option<Duration> {
    loop {
        let found = log
            .lock()
            .unwrap()
            .iter()
            .find(|(at, update)| *at >= t0 && pick(update))
            .map(|(at, _)| at.duration_since(t0));
        if found.is_some() {
            return found;
        }
        if t0.elapsed() > limit {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
}

/// The first view after `t0` that `wanted` accepts.
async fn view_after(
    log: &Log,
    t0: Instant,
    limit: Duration,
    wanted: impl Fn(&SocialView) -> bool,
) -> Option<Duration> {
    first(
        log,
        t0,
        limit,
        |update| matches!(update, SocialUpdate::View(view) if wanted(view)),
    )
    .await
}

/// One row of the table: Ayşe's own view, the service's answer, her other computer, Bora's list,
/// Bora's crew page.
struct Row {
    setting: &'static str,
    own: Cell,
    service: Cell,
    other: Cell,
    friend: Cell,
    crew: Cell,
}

#[derive(Clone, Copy)]
enum Cell {
    At(Duration),
    /// Not within the wait (it comes only with a later refetch, if ever).
    Never,
    /// Does not apply to this observer.
    Na,
}

impl Cell {
    fn of(found: Option<Duration>) -> Self {
        found.map_or(Self::Never, Self::At)
    }

    fn text(self) -> String {
        match self {
            Self::At(d) => format!("{} ms", d.as_millis()),
            Self::Never => "never (refetch only)".into(),
            Self::Na => "–".into(),
        }
    }

    fn within(self, bound: Duration) -> bool {
        match self {
            Self::At(d) => d <= bound,
            Self::Never => false,
            Self::Na => true,
        }
    }
}

const WAIT: Duration = Duration::from_secs(6);
/// The promise: Ayşe sees her own change before the service could have answered.
const OWN_BOUND: Duration = Duration::from_millis(50);
/// The promise: everyone else within two seconds of the service's answer.
const SPREAD_BOUND: Duration = Duration::from_secs(2);

struct Players {
    ayse_id: String,
    a1: SocialService,
    a1_log: Log,
    a2_log: Log,
    b_log: Log,
}

impl Players {
    fn friend_of_bora<'v>(&self, view: &'v SocialView) -> Option<&'v pl_social::FriendView> {
        view.friends.iter().find(|f| f.id == self.ayse_id)
    }

    fn crew_member<'v>(&self, view: &'v SocialView) -> Option<&'v pl_social::TeamMemberView> {
        view.crews
            .first()
            .and_then(|crew| crew.members.iter().find(|m| m.id == self.ayse_id))
    }

    /// Changes one setting on Ayşe's first launcher and times every observer.
    #[allow(clippy::too_many_arguments)]
    async fn change(
        &self,
        setting: &'static str,
        update: ProfileUpdate,
        own: impl Fn(&SocialView) -> bool,
        other: Option<&dyn Fn(&SocialView) -> bool>,
        friend: Option<&dyn Fn(&SocialView) -> bool>,
        crew: Option<&dyn Fn(&SocialView) -> bool>,
    ) -> Row {
        let t0 = Instant::now();
        let answered = self.a1.update_profile(update).await;
        let service = t0.elapsed();
        assert!(answered.is_ok(), "{setting}: {answered:?}");
        let own = view_after(&self.a1_log, t0, WAIT, own).await;
        let other = match other {
            Some(wanted) => Cell::of(view_after(&self.a2_log, t0, WAIT, wanted).await),
            None => Cell::Na,
        };
        let friend = match friend {
            Some(wanted) => Cell::of(view_after(&self.b_log, t0, WAIT, wanted).await),
            None => Cell::Na,
        };
        let crew = match crew {
            Some(wanted) => Cell::of(view_after(&self.b_log, t0, WAIT, wanted).await),
            None => Cell::Na,
        };
        Row {
            setting,
            own: Cell::of(own),
            service: Cell::At(service),
            other,
            friend,
            crew,
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the local friends service (see tests/live.rs)"]
async fn settings_reach_the_player_their_devices_and_friends_quickly() {
    let client = client();
    client.health().await.expect("wrangler dev is running");
    let ayse_id = new_user_id(81);
    let bora_id = new_user_id(82);
    let ayse = sign_in(&client, &ayse_id, "ayse", "Ev PC").await;
    let ayse_laptop = sign_in(&client, &ayse_id, "ayse", "Dizüstü").await;
    let bora = sign_in(&client, &bora_id, "bora", "Bora PC").await;
    let code = ayse.me().await.unwrap().friend_code;
    bora.send_request(&code).await.unwrap();
    ayse.accept_request(&bora_id).await.unwrap();
    let crew = ayse
        .create_team("Gece Kartalları", "GKR", "sword", None, true)
        .await
        .unwrap()
        .team
        .unwrap();
    bora.join_team(&crew_code(&crew.code), true).await.unwrap();

    let folders = tempfile::tempdir().unwrap();
    let (a1, a1_log) = launcher(&ayse, &folders.path().join("a1")).await;
    let (_a2, a2_log) = launcher(&ayse_laptop, &folders.path().join("a2")).await;
    let (b, b_log) = launcher(&bora, &folders.path().join("b")).await;
    let players = Players {
        ayse_id: ayse_id.clone(),
        a1,
        a1_log,
        a2_log,
        b_log,
    };
    until(&b, "Ayşe online at Bora's", |view| {
        players
            .friend_of_bora(view)
            .is_some_and(|f| f.state == PresenceState::Launcher)
            && players
                .crew_member(view)
                .is_some_and(|m| m.state == PresenceState::Launcher)
    })
    .await;
    let mut rows = Vec::new();

    // Durum: Rahatsız etmeyin, Görünmez, Çevrimiçi.
    rows.push(
        players
            .change(
                "Durum → Rahatsız etmeyin",
                ProfileUpdate::default().status(StatusMode::Dnd),
                |v| v.me.as_ref().is_some_and(|me| me.status == StatusMode::Dnd),
                Some(&|v: &SocialView| {
                    v.me.as_ref().is_some_and(|me| me.status == StatusMode::Dnd)
                }),
                Some(&|v: &SocialView| players.friend_of_bora(v).is_some_and(|f| f.dnd)),
                Some(&|v: &SocialView| players.crew_member(v).is_some_and(|m| m.dnd)),
            )
            .await,
    );
    rows.push(
        players
            .change(
                "Durum → Görünmez",
                ProfileUpdate::default().status(StatusMode::Invisible),
                |v| {
                    v.me.as_ref()
                        .is_some_and(|me| me.status == StatusMode::Invisible)
                },
                Some(&|v: &SocialView| {
                    v.me.as_ref()
                        .is_some_and(|me| me.status == StatusMode::Invisible)
                }),
                Some(&|v: &SocialView| {
                    players
                        .friend_of_bora(v)
                        .is_some_and(|f| f.state == PresenceState::Offline)
                }),
                Some(&|v: &SocialView| {
                    players
                        .crew_member(v)
                        .is_some_and(|m| m.state == PresenceState::Offline)
                }),
            )
            .await,
    );
    rows.push(
        players
            .change(
                "Durum → Çevrimiçi",
                ProfileUpdate::default().status(StatusMode::Online),
                |v| {
                    v.me.as_ref()
                        .is_some_and(|me| me.status == StatusMode::Online)
                },
                Some(&|v: &SocialView| {
                    v.me.as_ref()
                        .is_some_and(|me| me.status == StatusMode::Online)
                }),
                Some(&|v: &SocialView| {
                    players
                        .friend_of_bora(v)
                        .is_some_and(|f| f.state == PresenceState::Launcher && !f.dnd)
                }),
                Some(&|v: &SocialView| {
                    players
                        .crew_member(v)
                        .is_some_and(|m| m.state == PresenceState::Launcher && !m.dnd)
                }),
            )
            .await,
    );

    // "Başka sunuculardaysam adresini göster", while she plays on another server.
    players.a1.game_started(None);
    players.a1.game_line("Connecting to mc.example.org, 25565");
    until(&b, "Ayşe playing elsewhere at Bora's", |view| {
        players.friend_of_bora(view).is_some_and(|f| {
            f.state == PresenceState::Playing
                && f.game
                    .as_ref()
                    .is_some_and(|g| g.location == GameLocation::Other)
        })
    })
    .await;
    let shown = |v: &SocialView| {
        players.friend_of_bora(v).is_some_and(|f| {
            f.game
                .as_ref()
                .is_some_and(|g| g.host.as_deref() == Some("mc.example.org"))
        })
    };
    rows.push(
        players
            .change(
                "Adresimi göster → açık",
                ProfileUpdate::default().share_server(true),
                |v| v.me.as_ref().is_some_and(|me| me.privacy.share_server),
                Some(&|v: &SocialView| v.me.as_ref().is_some_and(|me| me.privacy.share_server)),
                Some(&shown),
                None,
            )
            .await,
    );
    rows.push(
        players
            .change(
                "Adresimi göster → kapalı",
                ProfileUpdate::default().share_server(false),
                |v| v.me.as_ref().is_some_and(|me| !me.privacy.share_server),
                Some(&|v: &SocialView| v.me.as_ref().is_some_and(|me| !me.privacy.share_server)),
                Some(&|v: &SocialView| {
                    players.friend_of_bora(v).is_some_and(|f| {
                        f.game.as_ref().is_some_and(|g| g.host.is_none())
                            && f.state == PresenceState::Playing
                    })
                }),
                None,
            )
            .await,
    );
    players.a1.game_stopped();

    // Profil fotoğrafı.
    let fox = "preset:fox";
    let picture_changed = Instant::now();
    rows.push(
        players
            .change(
                "Profil fotoğrafı",
                ProfileUpdate {
                    avatar: Some(fox.into()),
                    ..ProfileUpdate::default()
                },
                |v| v.me.as_ref().is_some_and(|me| me.avatar == fox),
                Some(&|v: &SocialView| v.me.as_ref().is_some_and(|me| me.avatar == fox)),
                Some(&|v: &SocialView| players.friend_of_bora(v).is_some_and(|f| f.avatar == fox)),
                Some(&|v: &SocialView| players.crew_member(v).is_some_and(|m| m.avatar == fox)),
            )
            .await,
    );
    // Her own entry on her crew page.
    let own_crew_page = view_after(&players.a1_log, picture_changed, WAIT, |v| {
        v.crews
            .first()
            .and_then(|c| c.members.iter().find(|m| m.me))
            .is_some_and(|m| m.avatar == fox)
    })
    .await;

    // "Son görülmeyi göster": off while she is online, then she goes invisible.
    until(&b, "Bora knows when Ayşe was last seen", |view| {
        players.friend_of_bora(view).is_some()
    })
    .await;
    rows.push(
        players
            .change(
                "Son görülme → kapalı",
                ProfileUpdate::default().share_last_seen(false),
                |v| v.me.as_ref().is_some_and(|me| !me.privacy.share_last_seen),
                Some(&|v: &SocialView| v.me.as_ref().is_some_and(|me| !me.privacy.share_last_seen)),
                None,
                None,
            )
            .await,
    );
    let hidden_when_gone = {
        let t0 = Instant::now();
        players
            .a1
            .update_profile(ProfileUpdate::default().status(StatusMode::Invisible))
            .await
            .unwrap();
        view_after(&players.b_log, t0, WAIT, |v| {
            players
                .friend_of_bora(v)
                .is_some_and(|f| f.state == PresenceState::Offline && f.last_seen.is_none())
        })
        .await
    };
    // On again while invisible (says nothing: an invisible player's change never shows), then she
    // comes back and goes away again.
    rows.push(
        players
            .change(
                "Son görülme → açık (görünmezken)",
                ProfileUpdate::default().share_last_seen(true),
                |v| v.me.as_ref().is_some_and(|me| me.privacy.share_last_seen),
                Some(&|v: &SocialView| v.me.as_ref().is_some_and(|me| me.privacy.share_last_seen)),
                None,
                None,
            )
            .await,
    );
    players
        .a1
        .update_profile(ProfileUpdate::default().status(StatusMode::Online))
        .await
        .unwrap();
    let shown_when_gone = {
        let t0 = Instant::now();
        players
            .a1
            .update_profile(ProfileUpdate::default().status(StatusMode::Invisible))
            .await
            .unwrap();
        view_after(&players.b_log, t0, WAIT, |v| {
            players.friend_of_bora(v).is_some_and(|f| {
                f.state == PresenceState::Offline && f.last_seen == Some(LastSeen::Today)
            })
        })
        .await
    };
    players
        .a1
        .update_profile(ProfileUpdate::default().status(StatusMode::Online))
        .await
        .unwrap();

    // "Görüldü bilgisi": Bora writes, Ayşe reads; Bora sees "Görüldü" while she shares it.
    let dm = direct_conversation(&ayse_id, &bora_id);
    let read_shared = async |text: &str| {
        b.chat_send(&dm, text).unwrap();
        until(&players.a1, "Bora's message at Ayşe's", |view| {
            view.chat.as_ref().is_some_and(|chat| {
                chat.conversations
                    .iter()
                    .any(|c| c.id == dm && c.last.as_ref().is_some_and(|l| l.text == text))
            })
        })
        .await;
        let before = peer_read(&b.view(), &dm);
        let t0 = Instant::now();
        players.a1.chat_read(&dm);
        view_after(&players.b_log, t0, Duration::from_secs(3), |v| {
            peer_read(v, &dm) > before
        })
        .await
    };
    let read_before = read_shared("selam").await;
    tokio::time::sleep(Duration::from_millis(2_100)).await;
    rows.push(
        players
            .change(
                "Görüldü bilgisi → kapalı",
                ProfileUpdate::default().share_read(false),
                |v| {
                    v.me.as_ref()
                        .is_some_and(|me| me.privacy.share_read == Some(false))
                },
                Some(&|v: &SocialView| {
                    v.me.as_ref()
                        .is_some_and(|me| me.privacy.share_read == Some(false))
                }),
                None,
                None,
            )
            .await,
    );
    tokio::time::sleep(Duration::from_millis(1_100)).await;
    let read_after = read_shared("orada mısın").await;

    // "Yazıyor bilgisi": Bora writes; Ayşe sees "yazıyor…" while she shares it (both ways).
    let typing_shown = async || {
        let t0 = Instant::now();
        b.chat_typing(&dm);
        first(
            &players.a1_log,
            t0,
            Duration::from_secs(3),
            |update| matches!(update, SocialUpdate::Typing(typing) if typing.conv == dm),
        )
        .await
    };
    let typing_before = typing_shown().await;
    tokio::time::sleep(Duration::from_millis(4_200)).await;
    rows.push(
        players
            .change(
                "Yazıyor bilgisi → kapalı",
                ProfileUpdate::default().share_typing(false),
                |v| {
                    v.me.as_ref()
                        .is_some_and(|me| me.privacy.share_typing == Some(false))
                },
                Some(&|v: &SocialView| {
                    v.me.as_ref()
                        .is_some_and(|me| me.privacy.share_typing == Some(false))
                }),
                None,
                None,
            )
            .await,
    );
    tokio::time::sleep(Duration::from_millis(4_200)).await;
    let typing_after = typing_shown().await;

    println!();
    println!(
        "| Ayar | (a) kendi ekranı | (b) servis cevabı | öbür bilgisayarı | arkadaş listesi | ekip sayfası |"
    );
    println!("| --- | --- | --- | --- | --- | --- |");
    for row in &rows {
        println!(
            "| {} | {} | {} | {} | {} | {} |",
            row.setting,
            row.own.text(),
            row.service.text(),
            row.other.text(),
            row.friend.text(),
            row.crew.text()
        );
    }
    println!(
        "| Kendi ekip sayfasındaki fotoğrafı | {} | | | | |",
        Cell::of(own_crew_page).text()
    );
    println!(
        "| Son görülme kapalıyken görünmez olunca arkadaşta | | | | {} | |",
        Cell::of(hidden_when_gone).text()
    );
    println!(
        "| Son görülme açıkken görünmez olunca arkadaşta | | | | {} | |",
        Cell::of(shown_when_gone).text()
    );
    println!(
        "| Görüldü: açıkken / kapattıktan sonra arkadaşta | | | | {} / {} | |",
        Cell::of(read_before).text(),
        Cell::of(read_after).text()
    );
    println!(
        "| Yazıyor: açıkken / kapattıktan sonra (kendisinde) | {} / {} | | | | |",
        Cell::of(typing_before).text(),
        Cell::of(typing_after).text()
    );
    println!();

    for session in [&bora, &ayse_laptop, &ayse] {
        let _ = session.delete_account().await;
    }

    // The promise.
    for row in &rows {
        assert!(row.own.within(OWN_BOUND), "{}: own view", row.setting);
        let spread = match row.service {
            Cell::At(answer) => answer + SPREAD_BOUND,
            _ => SPREAD_BOUND,
        };
        for (who, cell) in [
            ("other", row.other),
            ("friend", row.friend),
            ("crew", row.crew),
        ] {
            assert!(cell.within(spread), "{}: {who}", row.setting);
        }
    }
    assert!(
        own_crew_page.is_some(),
        "her own crew page shows the new picture"
    );
    assert!(
        hidden_when_gone.is_some(),
        "a hidden last seen leaves Bora's list"
    );
    assert!(shown_when_gone.is_some(), "a shown last seen comes back");
    assert!(
        read_before.is_some() && read_after.is_none(),
        "Görüldü follows the switch"
    );
    assert!(
        typing_before.is_some() && typing_after.is_none(),
        "yazıyor follows the switch"
    );
}

fn peer_read(view: &SocialView, conv: &str) -> Option<i64> {
    view.chat
        .as_ref()?
        .conversations
        .iter()
        .find(|c| c.id == conv)?
        .peer_read
}

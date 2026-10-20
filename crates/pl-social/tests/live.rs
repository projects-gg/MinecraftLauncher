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
//! `PL_SOCIAL_URL` overrides the service URL (default `http://127.0.0.1:8797/`).

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use pl_social::{
    CallAnswer, CallKind, ErrorCode, InviteOutcome, LocalStatus, LoginAttempt, PermissionLevel,
    PermissionsEdit, PresenceEvent, PresenceHandle, PresenceOptions, PresenceState, ProfileUpdate,
    Relation, RequestOutcome, Session, SettingsPrecondition, SocialClient, SocialConfig,
    SocialError, StatusMode, StopReason, TEAM_FEATURE_CALLS, TEAM_FEATURE_LOOK, TEAM_FEATURE_MUTE,
    TEAM_FEATURE_ROLES, TeamEdit, TeamLook, TeamNotifications, TeamNotificationsEdit,
    auth_code_from_deep_link, crew_code,
};
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

    // Founding: the look goes only where the service has looks.
    let wanted = TeamLook {
        color: "teal".into(),
        frame: "banner".into(),
        motto: "Birlikte güçlüyüz!".into(),
    };
    let emblem = if look { "controller" } else { "sword" };
    let created = leader
        .create_team("Gece Kartalları", "GKR", emblem, look.then_some(&wanted))
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
    let lookup = mate.lookup_team(&shown).await.unwrap();
    assert_eq!(lookup.team.id, created.id);
    assert_eq!(lookup.team.color, if look { "teal" } else { "gold" });
    let joined = mate.join_team(&shown).await.unwrap().team.unwrap();
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
        mate_presence.call_crew().await.unwrap();
        let call = match expect(&mut leader_presence, "the call", |e| {
            matches!(e, PresenceEvent::Call { .. })
        })
        .await
        {
            PresenceEvent::Call { call, quiet, clock } => {
                assert!(!quiet, "a teammate's live call is announced");
                assert!(clock.is_some(), "the hub's clock comes with it");
                assert_eq!(call.from, mate_id);
                assert_eq!(call.kind, CallKind::Ready);
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
        // One call at a time, and not again so soon.
        let again = leader_presence.call_crew().await.unwrap_err();
        assert_eq!(again.code(), Some(&ErrorCode::CallActive));
        mate_presence.end_call(&call.id).await.unwrap();
        expect(
            &mut leader_presence,
            "the end",
            |e| matches!(e, PresenceEvent::CallEnded { call: id, .. } if *id == call.id),
        )
        .await;
        let soon = mate_presence.call_crew().await.unwrap_err();
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
        let refused = mate_presence.call_crew().await.unwrap_err();
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
            leader_presence.call_crew().await.unwrap();
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

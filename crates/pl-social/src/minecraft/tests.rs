//! `ProjectsSkins` against a mock friends service, with Mojang's hosts on the same mock server.
//! The presence socket is refused there (as in an outage), so `mc` messages are handed in the way
//! the friends service hands them (`McSignal`), and jobs sent from here are asked about by HTTP.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use pl_auth::{MojangEndpoints, SkinVariant};
use sha2::{Digest, Sha256};
use tokio::runtime::Handle;
use url::Url;

use super::*;
use crate::auth::SessionToken;
use crate::model::{ModNotice, NoticeKind};
use crate::presence::{PresenceEvent, PresenceOptions};
use crate::protocol::ServerMessage;
use crate::service::{ServiceConfig, UpdateSink};
use crate::state::SocialState;
use crate::store::{MemoryTokenStore, TokenStore};
use crate::testutil::{MockReply, MockServer, Recorded};
use crate::view::AuthPhase;

const ME: &str = r#"{"id":"123456789012345678","username":"ayse","displayName":"Ayşe","avatar":"discord",
    "avatarHash":null,"discordAvatarHash":null,"friendCode":"PRJ-ABCD-EFGH","status":"online",
    "privacy":{"shareServer":false,"shareLastSeen":true},"createdOn":"2026-10-16","settings":null}"#;
const FRIENDS: &str = r#"{"friends":[],"incoming":[],"outgoing":[],"blocked":[],
    "limits":{"friends":200,"outgoingRequests":50},"features":["mcLink","skins"]}"#;

const JOB_A: &str = "jAAAAAAAAAAAAAAAAAAAAAA";
const JOB_B: &str = "jBBBBBBBBBBBBBBBBBBBBBB";
const JOB_C: &str = "jCCCCCCCCCCCCCCCCCCCCCC";
const TEXTURE: &str = "1a4af718455d4aab528e7a61f86fa25e6a369d1768dcb13f7df319a713eb810b";
const DREAM_ID: &str = "ec70bcaf702f4bb8b48d276fa52a780c";

// --- pictures --------------------------------------------------------------------------------

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// A real RGBA PNG (stored deflate blocks: no compression library needed here).
fn png(width: u32, height: u32, paint: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
    let mut raw = Vec::new();
    for y in 0..height {
        raw.push(0);
        for x in 0..width {
            raw.extend_from_slice(&paint(x, y));
        }
    }
    let mut zlib = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65_535).collect();
    for (index, block) in blocks.iter().enumerate() {
        zlib.push(u8::from(index + 1 == blocks.len()));
        let len = block.len() as u16;
        zlib.extend_from_slice(&len.to_le_bytes());
        zlib.extend_from_slice(&(!len).to_le_bytes());
        zlib.extend_from_slice(block);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in &raw {
        a = (a + u32::from(byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    zlib.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let chunk = |kind: &[u8], body: &[u8]| {
        let mut out = (body.len() as u32).to_be_bytes().to_vec();
        let mut typed = kind.to_vec();
        typed.extend_from_slice(body);
        out.extend_from_slice(&typed);
        out.extend_from_slice(&crc32(&typed).to_be_bytes());
        out
    };
    let mut ihdr = width.to_be_bytes().to_vec();
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    out.extend(chunk(b"IHDR", &ihdr));
    out.extend(chunk(b"IDAT", &zlib));
    out.extend(chunk(b"IEND", &[]));
    out
}

fn skin(shade: u8) -> Vec<u8> {
    png(64, 64, |x, y| {
        if y < 32 || (16..48).contains(&x) {
            [shade, 100, 120, 255]
        } else {
            [0, 0, 0, 0]
        }
    })
}

fn textures_value(model: &str) -> String {
    STANDARD.encode(
        serde_json::json!({
            "profileId": DREAM_ID,
            "profileName": "Dream",
            "textures": {"SKIN": {
                "url": format!("http://textures.minecraft.net/texture/{TEXTURE}"),
                "metadata": {"model": model}
            }}
        })
        .to_string(),
    )
}

// --- the mock service --------------------------------------------------------------------------

/// What the mock answers; tests change it as they go.
struct Mock {
    /// `GET /v1/mc`: status and body.
    state: (u16, String),
    /// Overrides for the routes that open jobs: status and body.
    put: Option<(u16, String)>,
    link: Option<(u16, String)>,
    unlink: Option<(u16, String)>,
    check: Option<(u16, String)>,
    cancel: Option<(u16, String)>,
    /// `GET /v1/mc/jobs/:id`.
    job: Option<serde_json::Value>,
    /// The id of the next job opened.
    next: Vec<&'static str>,
}

fn json(value: &serde_json::Value) -> (u16, String) {
    (200, value.to_string())
}

fn state_json(skin_texture: &str, left: u32) -> serde_json::Value {
    serde_json::json!({
        "features": ["mcLink", "skins"],
        "health": {"bot": true, "server": false},
        "link": {"state": "linked", "name": "Ahmet", "at": 1792500000, "checkedAt": 1792600000},
        "skin": {"kind": "png", "texture": skin_texture, "variant": "slim", "premium": null,
                 "at": 1792600100, "by": "launcher"},
        "jobs": [],
        "limits": {"skinsPerDay": 3, "skinsLeft": left, "resetAt": 1792627200, "linkTriesLeft": 5},
        "blocked": null,
        "limited": null,
        "codeless": {"declined": []}
    })
}

fn opened(id: &str, kind: &str, name: Option<&str>) -> String {
    serde_json::json!({"job": {"id": id, "kind": kind, "state": "pending", "name": name,
        "source": if kind == "skin" { serde_json::json!("png") } else { serde_json::Value::Null },
        "createdAt": 1792600000000u64, "updatedAt": 1792600000000u64}})
    .to_string()
}

fn error(status: u16, body: serde_json::Value) -> Option<(u16, String)> {
    Some((status, serde_json::json!({"error": body}).to_string()))
}

fn answer(mock: &Mutex<Mock>, request: &Recorded) -> MockReply {
    let path = request.path.split('?').next().unwrap_or_default();
    let method = request.method.as_str();
    if path.starts_with("/v1/presence") {
        return MockReply::json(503, r#"{"error":{"code":"unavailable","message":"x"}}"#)
            .with_header("Retry-After", "600");
    }
    let mut mock = mock.lock().unwrap();
    let reply = |(status, body): (u16, String)| {
        if body.is_empty() {
            MockReply::empty(status)
        } else {
            MockReply::json(status, &body)
        }
    };
    let next = |mock: &mut Mock| {
        if mock.next.len() > 1 {
            mock.next.remove(0)
        } else {
            mock.next[0]
        }
    };
    match (method, path) {
        ("GET", "/v1/me") => MockReply::json(200, ME),
        ("GET", "/v1/friends") => MockReply::json(200, FRIENDS),
        ("GET", "/v1/mc") => reply(mock.state.clone()),
        ("PUT", "/v1/mc/skin") => match mock.put.clone() {
            Some(over) => reply(over),
            None => {
                let id = next(&mut mock);
                reply((202, opened(id, "skin", Some("Ahmet"))))
            }
        },
        ("POST", "/v1/mc/link") => match mock.link.clone() {
            Some(over) => reply(over),
            None => {
                let id = next(&mut mock);
                reply((202, opened(id, "link", Some("Ahmet"))))
            }
        },
        ("DELETE", "/v1/mc/link") => match mock.unlink.clone() {
            Some(over) => reply(over),
            None => {
                let id = next(&mut mock);
                reply((202, opened(id, "unlink", None)))
            }
        },
        ("POST", "/v1/mc/link/check") => match mock.check.clone() {
            Some(over) => reply(over),
            None => {
                let id = next(&mut mock);
                reply((202, opened(id, "check", None)))
            }
        },
        ("GET", job) if job.starts_with("/v1/mc/jobs/") => match &mock.job {
            Some(job) => MockReply::json(200, &serde_json::json!({ "job": job }).to_string()),
            None => MockReply::json(404, r#"{"error":{"code":"not_found","message":"x"}}"#),
        },
        ("DELETE", job) if job.starts_with("/v1/mc/jobs/") => {
            reply(mock.cancel.clone().unwrap_or((204, String::new())))
        }
        // Mojang
        ("GET", "/minecraft/profile/lookup/name/Dream")
        | ("GET", "/minecraft/profile/lookup/name/dream") => MockReply::json(
            200,
            &serde_json::json!({"id": DREAM_ID, "name": "Dream"}).to_string(),
        ),
        ("GET", profile) if profile == format!("/session/minecraft/profile/{DREAM_ID}") => {
            MockReply::json(
                200,
                &serde_json::json!({"id": DREAM_ID, "name": "Dream", "properties": [{
                    "name": "textures", "value": textures_value("slim"), "signature": "c2lnbg=="
                }]})
                .to_string(),
            )
        }
        ("GET", texture) if texture == format!("/texture/{TEXTURE}") => {
            MockReply::bytes(200, "image/png", skin(9))
        }
        _ => MockReply::json(404, r#"{"error":{"code":"not_found","message":"x"}}"#),
    }
}

struct Harness {
    server: MockServer,
    mock: Arc<Mutex<Mock>>,
    social: SocialService,
    skins: ProjectsSkins,
    changes: Arc<Mutex<Vec<ProjectsSkinChange>>>,
    dir: tempfile::TempDir,
    account: SkinAccount,
}

impl Harness {
    async fn start(state: serde_json::Value) -> Self {
        let mock = Arc::new(Mutex::new(Mock {
            state: json(&state),
            put: None,
            link: None,
            unlink: None,
            check: None,
            cancel: None,
            job: None,
            next: vec![JOB_A, JOB_B, JOB_C],
        }));
        let server = MockServer::start({
            let mock = mock.clone();
            move |request: &Recorded| answer(&mock, request)
        })
        .await;
        let tokens = Arc::new(MemoryTokenStore::new());
        tokens
            .save(&SessionToken::new(format!("pls_{}", "A".repeat(43))))
            .unwrap();
        let sink: UpdateSink = Arc::new(|_| {});
        let social = SocialService::new(
            reqwest::Client::new(),
            ServiceConfig {
                user_agent: "ProjectsLauncher/test".into(),
                device: "Ev PC".into(),
                presence: PresenceOptions::default(),
            },
            tokens,
            Handle::current(),
            sink,
        );
        social.configure(Some(Url::parse(&server.base()).unwrap()));
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !(social.view().auth == AuthPhase::SignedIn && social.view().loaded) {
            assert!(std::time::Instant::now() < deadline, "never signed in");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let dir = tempfile::tempdir().unwrap();
        let changes = Arc::new(Mutex::new(Vec::new()));
        let account = SkinAccount {
            id: "acc1".into(),
            name: "Ahmet".into(),
        };
        let skins = ProjectsSkins::new(
            &social,
            reqwest::Client::new(),
            SkinsConfig {
                store_dir: dir.path().join("skins").join("projects"),
                versions_dir: dir.path().join("versions"),
                mojang: MojangEndpoints::with_base(&server.base()),
            },
            Handle::current(),
            {
                let changes = changes.clone();
                Arc::new(move |change| changes.lock().unwrap().push(change))
            },
            {
                let account = account.clone();
                Arc::new(move || vec![account.clone()])
            },
        );
        Self {
            server,
            mock,
            social,
            skins,
            changes,
            dir,
            account,
        }
    }

    fn mock(&self) -> std::sync::MutexGuard<'_, Mock> {
        self.mock.lock().unwrap()
    }

    fn hits(&self, method: &str, path: &str) -> usize {
        self.server
            .requests()
            .iter()
            .filter(|request| request.method == method && request.path == path)
            .count()
    }

    fn last(&self, method: &str, path: &str) -> serde_json::Value {
        self.server
            .requests()
            .into_iter()
            .rev()
            .find(|request| request.method == method && request.path == path)
            .expect("the request was made")
            .json()
    }

    fn changes(&self) -> Vec<ProjectsSkinChange> {
        self.changes.lock().unwrap().clone()
    }

    async fn until_change(
        &self,
        what: &str,
        found: impl Fn(&ProjectsSkinChange) -> bool,
    ) -> ProjectsSkinChange {
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        loop {
            if let Some(change) = self.changes().into_iter().find(|change| found(change)) {
                return change;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "no {what}: {:?}",
                self.changes()
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    fn tell(&self, message: serde_json::Value) {
        let message: McMessage = serde_json::from_value(message).unwrap();
        self.skins.on_signal(McSignal::Message(Box::new(message)));
    }
}

fn closed_skin(id: &str, code: &str, texture: Option<&str>) -> serde_json::Value {
    let mut result = serde_json::json!({ "code": code });
    if let Some(texture) = texture {
        result["texture"] = texture.into();
        result["variant"] = "slim".into();
    }
    serde_json::json!({"id": id, "kind": "skin", "state": if code == "applied" { "done" } else { "failed" },
        "name": "Ahmet", "source": "png", "result": result,
        "createdAt": 1792600000000u64, "updatedAt": 1792600005000u64})
}

fn mc(job: Option<serde_json::Value>, link: serde_json::Value) -> serde_json::Value {
    serde_json::json!({"job": job, "link": link, "skin": null, "blocked": null})
}

fn linked_to(name: &str) -> serde_json::Value {
    serde_json::json!({"state": "linked", "name": name, "at": 1, "checkedAt": 2})
}

fn job_changes(harness: &Harness) -> Vec<ProjectsSkinChange> {
    harness
        .changes()
        .into_iter()
        .filter(|change| matches!(change, ProjectsSkinChange::Job { .. }))
        .collect()
}

// --- tests ---------------------------------------------------------------------------------------

#[tokio::test]
async fn the_entry_points_read_the_service_once_in_the_background() {
    let harness = Harness::start(state_json(TEXTURE, 2)).await;
    // Not shown: nothing is read.
    let hidden = harness.skins.overview();
    assert!(!hidden.shown);
    assert_eq!(
        hidden.service,
        Some(ProjectsSkinService {
            link: true,
            skins: true
        })
    );
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(harness.hits("GET", "/v1/mc"), 0);

    harness.skins.set_shown(true);
    let first = harness.skins.overview();
    assert!(first.shown);
    assert_eq!(first.link, None, "nothing known yet");
    harness
        .until_change("a state change", |change| {
            *change == ProjectsSkinChange::State
        })
        .await;
    let second = harness.skins.overview();
    let link = second.link.unwrap();
    assert_eq!(
        (link.state, link.name.as_deref()),
        (ProjectsLinkState::Linked, Some("Ahmet"))
    );
    // Known now: no more background reads.
    harness.skins.overview();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(harness.hits("GET", "/v1/mc"), 1);

    // Signed out: nothing at all.
    harness.social.sign_out().await;
    let out = harness.skins.overview();
    assert_eq!((out.service, out.link), (None, None));
}

#[tokio::test]
async fn the_panel_shows_the_service_and_the_skin_of_the_account() {
    let mut state = state_json(TEXTURE, 2);
    state["jobs"] = serde_json::json!([
        {"id": JOB_A, "kind": "skin", "state": "queued", "name": "Ahmet", "source": "default",
         "createdAt": 3, "updatedAt": 4},
        closed_skin(JOB_B, "applied", Some(TEXTURE)),
        {"id": JOB_C, "kind": "check", "state": "working", "createdAt": 1, "updatedAt": 2}
    ]);
    state["blocked"] = serde_json::json!({"until": 99});
    state["limited"] = serde_json::json!({"until": 123});
    let harness = Harness::start(state).await;
    let view = harness.skins.state(&harness.account).await;
    assert_eq!(view.access, ProjectsSkinAccess::Ready);
    assert!(view.server_away);
    assert_eq!(view.link.state, ProjectsLinkState::Linked);
    let skin = view.skin.unwrap();
    assert_eq!(
        (skin.kind, skin.variant, skin.by),
        (
            ProjectsSkinKind::Png,
            SkinVariant::Slim,
            ProjectsSkinBy::Launcher
        )
    );
    assert!(skin.picture.unwrap().starts_with("data:image/png;base64,"));
    assert_eq!(view.job.unwrap().state, ProjectsSkinJobState::Queued);
    assert_eq!(
        view.last_job.unwrap().result.unwrap().code,
        ProjectsSkinResultCode::Applied
    );
    assert_eq!(view.link_job.unwrap().kind, ProjectsSkinJobKind::Check);
    assert_eq!(
        view.limits,
        ProjectsSkinLimits {
            per_day: 3,
            left: 2,
            reset_at: 1792627200,
            link_tries_left: 5,
            link_retry_at: None
        }
    );
    assert_eq!(view.blocked, Some(SkinBlockView { until: Some(99) }));
    assert_eq!(view.limited, Some(ProjectsSkinLimited { until: 123 }));
    assert!(!view.can_undo && !view.on_projects);
    // The texture came from Mojang's texture host once.
    harness.skins.state(&harness.account).await;
    assert_eq!(harness.hits("GET", &format!("/texture/{TEXTURE}")), 1);

    // Another account: the link is not its name, so no skin and no skin jobs.
    let other = SkinAccount {
        id: "acc2".into(),
        name: "Mehmet".into(),
    };
    let view = harness.skins.state(&other).await;
    assert!(view.skin.is_none() && view.job.is_none() && view.last_job.is_none());
    assert!(
        view.link_job.is_some(),
        "the link family is the Discord account's"
    );
}

#[tokio::test]
async fn the_panels_access_follows_the_service() {
    let harness = Harness::start(state_json(TEXTURE, 2)).await;
    let access = |harness: &Harness| {
        let skins = harness.skins.clone();
        let account = harness.account.clone();
        async move { skins.state(&account).await.access }
    };
    harness.mock().state = (
        404,
        r#"{"error":{"code":"not_found","message":"x"}}"#.into(),
    );
    assert_eq!(access(&harness).await, ProjectsSkinAccess::Off);
    harness.mock().state = error(
        503,
        serde_json::json!({"code": "unavailable", "message": "x", "reason": "bot"}),
    )
    .unwrap();
    assert_eq!(access(&harness).await, ProjectsSkinAccess::Unreachable);
    let mut state = state_json(TEXTURE, 2);
    state["features"] = serde_json::json!(["mcLink"]);
    harness.mock().state = json(&state);
    assert_eq!(access(&harness).await, ProjectsSkinAccess::Off);
    let mut state = state_json(TEXTURE, 2);
    state["health"]["bot"] = false.into();
    harness.mock().state = json(&state);
    assert_eq!(access(&harness).await, ProjectsSkinAccess::Unreachable);
    harness.social.sign_out().await;
    assert_eq!(access(&harness).await, ProjectsSkinAccess::SignedOut);
}

#[tokio::test]
async fn a_picked_file_is_saved_applied_kept_and_can_be_put_back() {
    let harness = Harness::start(state_json(TEXTURE, 2)).await;
    let file = harness.dir.path().join("my skin.png");
    std::fs::write(&file, skin(1)).unwrap();
    let ProjectsSkinPick::Ready { choice } = harness.skins.pick_file(file).await else {
        panic!("a fine skin");
    };
    assert_eq!(
        (choice.kind, choice.name.as_str(), choice.variant),
        (
            ProjectsSkinChoiceKind::Png,
            "my skin.png",
            SkinVariant::Classic
        )
    );
    assert!(!choice.legacy && !choice.black_parts && !choice.own);
    let ProjectsSkinOutcome::Sent { job } = harness
        .skins
        .save(
            &harness.account,
            ProjectsSkinSave::Choice {
                id: choice.id.clone(),
                variant: SkinVariant::Slim,
            },
        )
        .await
    else {
        panic!("sent");
    };
    assert_eq!(
        (job.id.as_str(), job.kind),
        (JOB_A, ProjectsSkinJobKind::Skin)
    );
    assert_eq!(job.variant, Some(SkinVariant::Slim));
    assert_eq!(job.picture.as_deref(), Some(choice.picture.as_str()));
    let put = harness.last("PUT", "/v1/mc/skin");
    assert_eq!(put["name"], "Ahmet");
    assert_eq!(put["source"]["kind"], "png");
    assert_eq!(put["source"]["variant"], "slim");
    let first_png = STANDARD
        .decode(put["source"]["png"].as_str().unwrap())
        .unwrap();
    assert!(first_png.len() <= pl_auth::CLEAN_SKIN_MAX_BYTES);
    let digest: String = Sha256::digest(&first_png)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(put["source"]["sha256"], digest.as_str());

    // The socket says it was applied: told once, with the account, nothing to put back yet.
    harness.tell(mc(
        Some(closed_skin(JOB_A, "applied", Some("aa11"))),
        linked_to("Ahmet"),
    ));
    let told = harness
        .until_change("the job", |change| {
            matches!(change, ProjectsSkinChange::Job { .. })
        })
        .await;
    let ProjectsSkinChange::Job {
        job,
        account_id,
        undo,
        on_projects,
    } = told
    else {
        unreachable!();
    };
    assert_eq!(job.result.unwrap().code, ProjectsSkinResultCode::Applied);
    assert_eq!(account_id.as_deref(), Some("acc1"));
    assert!(!undo && !on_projects);
    assert!(job.picture.is_some(), "the picture this computer sent");
    harness.tell(mc(
        Some(closed_skin(JOB_A, "applied", Some("aa11"))),
        linked_to("Ahmet"),
    ));
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert_eq!(job_changes(&harness).len(), 1, "told once");

    // The panel shows this computer's copy of the applied skin, not Mojang's.
    harness.mock().state = json(&state_json("aa11", 1));
    let view = harness.skins.state(&harness.account).await;
    assert!(view.skin.unwrap().picture.is_some());
    assert_eq!(harness.hits("GET", "/texture/aa11"), 0);
    assert!(!view.can_undo);

    // A second skin: once applied, "Geri al" can follow (a change is left today).
    let file = harness.dir.path().join("second.png");
    std::fs::write(&file, skin(2)).unwrap();
    let ProjectsSkinPick::Ready { choice } = harness.skins.pick_file(file).await else {
        panic!("a fine skin");
    };
    harness
        .skins
        .save(
            &harness.account,
            ProjectsSkinSave::Choice {
                id: choice.id,
                variant: SkinVariant::Classic,
            },
        )
        .await;
    harness.tell(mc(
        Some(closed_skin(JOB_B, "applied", Some("bb22"))),
        linked_to("Ahmet"),
    ));
    let told = harness
        .until_change(
            "the second job",
            |change| matches!(change, ProjectsSkinChange::Job { job, .. } if job.id == JOB_B),
        )
        .await;
    assert!(matches!(told, ProjectsSkinChange::Job { undo: true, .. }));
    harness.mock().state = json(&state_json("bb22", 1));
    assert!(harness.skins.state(&harness.account).await.can_undo);

    // "Geri al" sends the first skin again, as it was.
    let ProjectsSkinOutcome::Sent { job } = harness
        .skins
        .save(&harness.account, ProjectsSkinSave::Undo)
        .await
    else {
        panic!("sent");
    };
    assert_eq!(job.id, JOB_C);
    let put = harness.last("PUT", "/v1/mc/skin");
    assert_eq!(
        STANDARD
            .decode(put["source"]["png"].as_str().unwrap())
            .unwrap(),
        first_png
    );
    assert_eq!(put["source"]["variant"], "slim");
}

#[tokio::test]
async fn a_change_that_closed_while_the_launcher_was_closed_is_kept() {
    let mut state = state_json("dd44", 1);
    state["jobs"] = serde_json::json!([closed_skin(JOB_B, "applied", Some("dd44"))]);
    let harness = Harness::start(state).await;
    // An earlier run saved one skin, then sent another and closed before it was applied.
    let store = store::SkinStore::new(harness.dir.path().join("skins").join("projects"));
    let png_source = |sha: char| store::KeptSource::Png {
        variant: SkinVariant::Slim,
        sha256: sha.to_string().repeat(64),
        texture: None,
    };
    store
        .set_pending("acc1", "Ahmet", JOB_A, &png_source('a'), Some(&skin(1)))
        .unwrap();
    store.applied("acc1", "Ahmet", JOB_A, Some("cc33")).unwrap();
    store
        .set_pending("acc1", "Ahmet", JOB_B, &png_source('b'), Some(&skin(2)))
        .unwrap();

    let view = harness.skins.state(&harness.account).await;
    assert!(view.can_undo, "the earlier skin can come back");
    assert!(view.skin.unwrap().picture.is_some());
    assert_eq!(
        harness.hits("GET", "/texture/dd44"),
        0,
        "the copy kept here is the applied skin"
    );
    let kept = store.read("acc1", "Ahmet");
    assert_eq!(kept.pending, None);
    assert_eq!(
        kept.current.as_ref().and_then(store::KeptSource::texture),
        Some("dd44")
    );
    // Nothing is told: the panel's last job says it.
    assert!(job_changes(&harness).is_empty());
}

#[tokio::test]
async fn a_failed_change_is_forgotten_and_told_for_its_account() {
    let harness = Harness::start(state_json(TEXTURE, 2)).await;
    harness
        .skins
        .save(&harness.account, ProjectsSkinSave::Default)
        .await;
    assert_eq!(
        harness.last("PUT", "/v1/mc/skin")["source"],
        serde_json::json!({"kind": "default"})
    );
    harness.tell(mc(
        Some(closed_skin(JOB_A, "skin_blocked", None)),
        linked_to("Ahmet"),
    ));
    let ProjectsSkinChange::Job { job, undo, .. } = harness
        .until_change("the job", |change| {
            matches!(change, ProjectsSkinChange::Job { .. })
        })
        .await
    else {
        unreachable!();
    };
    assert!(!undo);
    assert_eq!(
        job.result.unwrap().code,
        ProjectsSkinResultCode::SkinBlocked
    );
    // Nothing was applied: nothing to put back.
    assert_eq!(
        harness
            .skins
            .save(&harness.account, ProjectsSkinSave::Undo)
            .await,
        ProjectsSkinOutcome::refused(ProjectsSkinRefusalCode::ChoiceGone)
    );
}

#[tokio::test]
async fn refusals_come_back_as_answers() {
    let harness = Harness::start(state_json(TEXTURE, 2)).await;
    harness.mock().put = error(
        429,
        serde_json::json!({"code": "rate_limited", "message": "x", "retryAfter": 3600,
                           "item": "skin_changes"}),
    );
    let ProjectsSkinOutcome::Refused { refusal } = harness
        .skins
        .save(&harness.account, ProjectsSkinSave::Default)
        .await
    else {
        panic!("refused");
    };
    assert_eq!(
        (refusal.code, refusal.retry_after),
        (ProjectsSkinRefusalCode::Quota, Some(3600))
    );
    // A choice this run does not have.
    let puts = harness.hits("PUT", "/v1/mc/skin");
    assert_eq!(
        harness
            .skins
            .save(
                &harness.account,
                ProjectsSkinSave::Choice {
                    id: "c999".into(),
                    variant: SkinVariant::Slim
                }
            )
            .await,
        ProjectsSkinOutcome::refused(ProjectsSkinRefusalCode::ChoiceGone)
    );
    assert_eq!(harness.hits("PUT", "/v1/mc/skin"), puts);

    // A code that is not six digits never leaves.
    assert_eq!(
        harness.skins.link(&harness.account, "12a456").await,
        ProjectsSkinOutcome::refused(ProjectsSkinRefusalCode::InvalidPin)
    );
    assert_eq!(harness.hits("POST", "/v1/mc/link"), 0);
    harness.mock().link = error(
        409,
        serde_json::json!({"code": "already_linked", "message": "x", "name": "Ahmet"}),
    );
    let ProjectsSkinOutcome::Refused { refusal } =
        harness.skins.link(&harness.account, "482913").await
    else {
        panic!("refused");
    };
    assert_eq!(
        (refusal.code, refusal.name.as_deref()),
        (ProjectsSkinRefusalCode::AlreadyLinked, Some("Ahmet"))
    );
    // Too many tries: told, kept, and not asked again until then.
    harness.mock().link = error(
        429,
        serde_json::json!({"code": "rate_limited", "message": "x", "retryAfter": 1800,
                           "item": "link_tries"}),
    );
    let ProjectsSkinOutcome::Refused { refusal } =
        harness.skins.link(&harness.account, "482913").await
    else {
        panic!("refused");
    };
    assert_eq!(
        (refusal.code, refusal.retry_after),
        (ProjectsSkinRefusalCode::Tries, Some(1800))
    );
    let asked = harness.hits("POST", "/v1/mc/link");
    let ProjectsSkinOutcome::Refused { refusal } =
        harness.skins.link(&harness.account, "482913").await
    else {
        panic!("refused");
    };
    assert_eq!(refusal.code, ProjectsSkinRefusalCode::Tries);
    assert!(refusal.retry_after.is_some_and(|wait| wait > 1700));
    assert_eq!(harness.hits("POST", "/v1/mc/link"), asked);
    let retry_at = harness
        .skins
        .state(&harness.account)
        .await
        .limits
        .link_retry_at
        .unwrap();
    assert!(retry_at > unix_now() + 1700);

    harness.mock().unlink = error(
        404,
        serde_json::json!({"code": "not_linked", "message": "x"}),
    );
    assert_eq!(
        harness.skins.unlink().await,
        ProjectsSkinOutcome::refused(ProjectsSkinRefusalCode::NotLinked)
    );
    harness.mock().put = error(
        403,
        serde_json::json!({"code": "skin_blocked", "message": "x", "until": null}),
    );
    let ProjectsSkinOutcome::Refused { refusal } = harness
        .skins
        .save(&harness.account, ProjectsSkinSave::Default)
        .await
    else {
        panic!("refused");
    };
    assert_eq!(
        (refusal.code, refusal.until),
        (ProjectsSkinRefusalCode::Blocked, Some(None))
    );
}

#[tokio::test]
async fn premium_skins_by_name_and_the_default_look() {
    let harness = Harness::start(state_json(TEXTURE, 2)).await;
    let ProjectsSkinLookup::Found { choice } =
        harness.skins.lookup(&harness.account, " Dream ").await
    else {
        panic!("found");
    };
    assert_eq!(
        (
            choice.kind,
            choice.name.as_str(),
            choice.variant,
            choice.own
        ),
        (
            ProjectsSkinChoiceKind::Premium,
            "Dream",
            SkinVariant::Slim,
            false
        )
    );
    harness
        .skins
        .save(
            &harness.account,
            ProjectsSkinSave::Choice {
                id: choice.id,
                variant: SkinVariant::Classic,
            },
        )
        .await;
    let premium = &harness.last("PUT", "/v1/mc/skin")["source"]["premium"];
    assert_eq!(premium["name"], "Dream");
    assert_eq!(premium["id"], DREAM_ID);
    assert_eq!(premium["texture"], TEXTURE);
    assert_eq!(
        premium["variant"], "slim",
        "a premium skin keeps its own arms"
    );
    assert_eq!(premium["value"], textures_value("slim").as_str());
    assert_eq!(premium["signature"], "c2lnbg==");

    assert_eq!(
        harness.skins.lookup(&harness.account, "Nobody").await,
        ProjectsSkinLookup::NoAccount
    );
    // The account's own name is a premium account's: it lends its skin.
    let dream = SkinAccount {
        id: "acc3".into(),
        name: "Dream".into(),
    };
    let look = harness.skins.default_look(&dream).await.unwrap();
    assert!(look.premium && look.variant == SkinVariant::Slim);
    let ProjectsSkinLookup::Found { choice } = harness.skins.lookup(&dream, "dream").await else {
        panic!("found");
    };
    assert!(choice.own);
    // No premium account and no client jar here: nothing to show.
    assert_eq!(harness.skins.default_look(&harness.account).await, None);
}

#[tokio::test]
async fn messages_say_what_changed_elsewhere() {
    let mut state = state_json(TEXTURE, 2);
    state["link"] = serde_json::json!({"state": "none", "name": null, "at": null, "checkedAt": 5});
    state["skin"] = serde_json::Value::Null;
    let harness = Harness::start(state).await;
    // Nothing known yet: a link message is only "read again".
    harness.tell(mc(None, linked_to("Ahmet")));
    harness
        .until_change("state", |change| *change == ProjectsSkinChange::State)
        .await;
    harness.changes.lock().unwrap().clear();

    harness.skins.state(&harness.account).await;
    harness.tell(mc(None, linked_to("Ahmet")));
    assert_eq!(
        harness
            .until_change("linked", |change| matches!(
                change,
                ProjectsSkinChange::Linked { .. }
            ))
            .await,
        ProjectsSkinChange::Linked {
            name: "Ahmet".into()
        }
    );
    // A staff reset carries the same link: only "read again".
    harness.changes.lock().unwrap().clear();
    harness.tell(serde_json::json!({"link": linked_to("ahmet"),
        "skin": {"kind": "default", "texture": null, "variant": null, "at": 9, "by": "staff"},
        "blocked": {"until": null}}));
    harness
        .until_change("state", |change| *change == ProjectsSkinChange::State)
        .await;
    assert!(
        !harness
            .changes()
            .iter()
            .any(|change| matches!(change, ProjectsSkinChange::Linked { .. }))
    );
    // A job of another computer's: told with the account of its name.
    harness.tell(mc(
        Some(closed_skin(JOB_B, "applied", Some("cc"))),
        linked_to("Ahmet"),
    ));
    let ProjectsSkinChange::Job {
        job,
        account_id,
        undo,
        ..
    } = harness
        .until_change("the job", |change| {
            matches!(change, ProjectsSkinChange::Job { .. })
        })
        .await
    else {
        unreachable!();
    };
    assert_eq!(
        (job.id.as_str(), account_id.as_deref()),
        (JOB_B, Some("acc1"))
    );
    assert!(!undo && job.picture.is_none());
}

#[tokio::test]
async fn without_the_socket_a_sent_job_is_asked_about() {
    let harness = Harness::start(state_json(TEXTURE, 2)).await;
    let ProjectsSkinOutcome::Sent { job } = harness.skins.link(&harness.account, "482913").await
    else {
        panic!("sent");
    };
    assert_eq!(job.kind, ProjectsSkinJobKind::Link);
    assert_eq!(
        harness.last("POST", "/v1/mc/link"),
        serde_json::json!({"name": "Ahmet", "pin": "482913"})
    );
    harness.mock().job = Some(
        serde_json::json!({"id": JOB_A, "kind": "link", "state": "done",
        "name": "Ahmet", "result": {"code": "linked"}, "createdAt": 1, "updatedAt": 2}),
    );
    let ProjectsSkinChange::Job { job, .. } = harness
        .until_change("the polled job", |change| {
            matches!(change, ProjectsSkinChange::Job { .. })
        })
        .await
    else {
        unreachable!();
    };
    assert_eq!(job.result.unwrap().code, ProjectsSkinResultCode::Linked);
    assert!(harness.hits("GET", &format!("/v1/mc/jobs/{JOB_A}")) >= 1);
    // Told: no more asking.
    let asked = harness.hits("GET", &format!("/v1/mc/jobs/{JOB_A}"));
    tokio::time::sleep(Duration::from_millis(3_500)).await;
    assert_eq!(harness.hits("GET", &format!("/v1/mc/jobs/{JOB_A}")), asked);
}

#[tokio::test]
async fn cancel_and_check() {
    let harness = Harness::start(state_json(TEXTURE, 2)).await;
    harness
        .skins
        .save(&harness.account, ProjectsSkinSave::Default)
        .await;
    assert!(harness.skins.cancel(JOB_A).await.unwrap());
    harness
        .until_change("state", |change| *change == ProjectsSkinChange::State)
        .await;
    harness.mock().cancel = error(
        409,
        serde_json::json!({"code": "job_taken", "message": "x"}),
    );
    assert!(!harness.skins.cancel(JOB_B).await.unwrap());
    assert!(
        harness.skins.cancel("not a job").await.unwrap(),
        "nothing to cancel"
    );

    // A check the service answers from what it learned lately.
    harness.skins.state(&harness.account).await;
    harness.changes.lock().unwrap().clear();
    harness.mock().check = Some((
        200,
        serde_json::json!({"link": {"state": "none", "name": null, "at": null, "checkedAt": 7}})
            .to_string(),
    ));
    harness.skins.link_check().await;
    assert_eq!(harness.changes(), vec![ProjectsSkinChange::State]);
    assert_eq!(
        harness.skins.overview().link.unwrap().state,
        ProjectsLinkState::None
    );
}

#[test]
fn the_socket_message_and_the_staff_notice_read() {
    let message: ServerMessage = serde_json::from_str(
        r#"{"t":"mc","job":{"id":"jAAAAAAAAAAAAAAAAAAAAAA","kind":"skin","state":"done",
            "name":"Ahmet","source":"png","result":{"code":"applied","texture":"ff","variant":"slim"},
            "createdAt":1,"updatedAt":2},
            "link":{"state":"linked","name":"Ahmet","at":1,"checkedAt":2},
            "skin":{"kind":"png","texture":"ff","variant":"slim","premium":null,"at":3,"by":"launcher"},
            "blocked":null}"#,
    )
    .unwrap();
    let ServerMessage::Mc(message) = message else {
        panic!("an mc message");
    };
    assert!(message.job.as_ref().unwrap().applied());

    let mut state = SocialState::default();
    let notice = |json: &str| -> ModNotice { serde_json::from_str(json).unwrap() };
    for (json, blocked) in [
        (
            r#"{"id":"n1","kind":"skin_reset","at":5,"until":null}"#,
            Some(SkinBlockView { until: None }),
        ),
        (
            r#"{"id":"n2","kind":"skin_reset","at":5,"until":1800000000}"#,
            Some(SkinBlockView {
                until: Some(1800000000),
            }),
        ),
        (r#"{"id":"n3","kind":"skin_reset","at":5}"#, None),
        (r#"{"id":"n4","kind":"warning","at":5,"until":7}"#, None),
        (
            r#"{"id":"n5","kind":"skin_reset","at":5,"until":"soon"}"#,
            None,
        ),
    ] {
        state.apply(&PresenceEvent::Notice(notice(json)), 10);
        let shown = state.notices();
        let last = shown.last().unwrap();
        assert_eq!(last.blocked, blocked, "{json}");
    }
    assert!(
        state
            .notices()
            .iter()
            .take(3)
            .all(|notice| notice.kind == NoticeKind::SkinReset)
    );
    assert_eq!(
        crate::model::LimitedKey::parse("skin_change"),
        crate::model::LimitedKey::SkinChange
    );
}

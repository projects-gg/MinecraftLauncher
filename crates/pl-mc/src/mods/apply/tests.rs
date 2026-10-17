use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use pl_net::{HashAlgo, Http, HttpConfig, hash_bytes};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use url::Url;

use super::*;
use crate::addons::{ADDON_INDEX_FILE, AddonRecord, AddonSource};
use crate::gamefiles::fsops::modified_ms;
use crate::mods::check::Severity;
use crate::mods::repair::{ReinstallTarget, plan_reinstall};
use crate::mods::scan::test_jars::{fabric_jar, toml_jar};

// --- a small HTTP server that serves bytes ---------------------------------------------------

type Routes = Arc<dyn Fn(&str, &Url, &str) -> (u16, Vec<u8>) + Send + Sync>;

/// One request per connection, routed through `routes(method, url, host)`.
async fn serve(routes: Routes) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let host = format!("http://{}", listener.local_addr().unwrap());
    let base = host.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let routes = Arc::clone(&routes);
            let host = base.clone();
            tokio::spawn(async move {
                let mut raw = Vec::new();
                let mut buf = [0u8; 4096];
                let header_end = loop {
                    let n = socket.read(&mut buf).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    raw.extend_from_slice(&buf[..n]);
                    if let Some(pos) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                        break pos + 4;
                    }
                };
                let head = String::from_utf8_lossy(&raw[..header_end]).into_owned();
                let length = head
                    .lines()
                    .find_map(|l| {
                        let (k, v) = l.split_once(':')?;
                        k.eq_ignore_ascii_case("content-length")
                            .then(|| v.trim().parse::<usize>().ok())?
                    })
                    .unwrap_or(0);
                while raw.len() < header_end + length {
                    let n = socket.read(&mut buf).await.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    raw.extend_from_slice(&buf[..n]);
                }
                let mut parts = head.split_whitespace();
                let method = parts.next().unwrap_or("GET").to_owned();
                let target = parts.next().unwrap_or("/").to_owned();
                let url = Url::parse(&format!("http://mock{target}")).unwrap();
                let (status, body) = routes(&method, &url, &host);
                let response = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.write_all(&body).await;
                let _ = socket.shutdown().await;
            });
        }
    });
    host
}

fn client(host: &str) -> ModrinthClient {
    ModrinthClient::with_base_url(
        Http::new(HttpConfig::new("pl-mc-tests")).unwrap(),
        format!("{host}/v2"),
    )
}

fn downloader() -> Downloader {
    Downloader::new(Http::new(HttpConfig::new("pl-mc-tests")).unwrap())
}

fn json_body(value: Value) -> (u16, Vec<u8>) {
    (200, value.to_string().into_bytes())
}

fn version(host: &str, id: &str, project: &str, file: &str, content: &[u8], number: &str) -> Value {
    json!({
        "id": id, "project_id": project, "author_id": "a", "name": format!("{project} {id}"),
        "version_number": number, "changelog": null, "dependencies": [],
        "game_versions": ["1.21.1"], "version_type": "release", "loaders": ["fabric"],
        "featured": false, "status": "listed", "date_published": "2024-05-01T00:00:00Z",
        "downloads": 1,
        "files": [{
            "hashes": {
                "sha1": hash_bytes(content, HashAlgo::Sha1),
                "sha512": hash_bytes(content, HashAlgo::Sha512)
            },
            "url": format!("{host}/files/{file}"), "filename": file, "primary": true,
            "size": content.len(), "file_type": null
        }]
    })
}

fn project(id: &str, slug: &str, title: &str) -> Value {
    json!({
        "id": id, "slug": slug, "project_type": "mod", "team": "t", "title": title,
        "description": "d", "body": "", "published": "2024-01-01T00:00:00Z",
        "updated": "2024-01-01T00:00:00Z", "status": "approved", "license": null,
        "client_side": "required", "server_side": "optional", "downloads": 1, "followers": 1,
        "categories": [], "additional_categories": [], "game_versions": ["1.21.1"],
        "loaders": ["fabric"], "versions": [], "icon_url": null,
        "issues_url": null, "source_url": null, "wiki_url": null, "discord_url": null,
        "donation_urls": [], "gallery": [], "color": null
    })
}

/// `/v2/projects?ids=[...]`: the listed projects among `known` (by id or slug).
fn projects_for(url: &Url, known: &[Value]) -> (u16, Vec<u8>) {
    let ids: Vec<String> = url
        .query_pairs()
        .find(|(k, _)| k == "ids")
        .and_then(|(_, v)| serde_json::from_str(&v).ok())
        .unwrap_or_default();
    let found: Vec<&Value> = known
        .iter()
        .filter(|p| {
            ids.iter()
                .any(|id| p["id"] == id.as_str() || p["slug"] == id.as_str())
        })
        .collect();
    json_body(json!(found))
}

// --- a fake launcher ---------------------------------------------------------------------------

#[derive(Default)]
struct Host {
    pack: bool,
    pack_files: Vec<String>,
    /// (item, project id, slug)
    pack_items: Vec<(String, String, String)>,
    loader_versions: Vec<LoaderVersion>,
    /// What `use_java` reports.
    java: Option<u32>,
    log: Mutex<Vec<String>>,
    requests: Mutex<Vec<PackRequest>>,
    /// Files the fake pack sync writes.
    on_sync: Mutex<Vec<(PathBuf, Vec<u8>)>>,
}

impl RepairHost for Host {
    fn has_pack(&self) -> bool {
        self.pack
    }

    async fn pack_files(&self) -> Vec<String> {
        self.pack_files.clone()
    }

    fn pack_item_for(&self, project_id: &str, slugs: &[String]) -> Option<String> {
        self.pack_items
            .iter()
            .find(|(_, project, slug)| project == project_id || slugs.contains(slug))
            .map(|(item, _, _)| item.clone())
    }

    async fn sync_pack(&self, request: PackRequest, progress: PercentFn) -> Result<()> {
        progress(50.0);
        let writes: Vec<(PathBuf, Vec<u8>)> = self.on_sync.lock().unwrap().drain(..).collect();
        for (path, bytes) in writes {
            std::fs::write(path, bytes).unwrap();
        }
        self.requests.lock().unwrap().push(request);
        Ok(())
    }

    async fn loader_versions(&self) -> Result<Vec<LoaderVersion>> {
        Ok(self.loader_versions.clone())
    }

    async fn set_loader_version(&self, version: &str) -> Result<bool> {
        self.log.lock().unwrap().push(format!("loader {version}"));
        Ok(true)
    }

    async fn use_java(&self, major: u32) -> Result<Option<u32>> {
        self.log.lock().unwrap().push(format!("java {major}"));
        Ok(self.java)
    }
}

fn env() -> CheckEnvironment {
    check_environment("1.21.1", LoaderKind::Fabric, Some("0.16.10"), Some(21))
}

struct Game {
    _dir: tempfile::TempDir,
    game: PathBuf,
    mods: PathBuf,
    index: PathBuf,
}

fn game(files: &[(&str, Vec<u8>)]) -> Game {
    let dir = tempfile::tempdir().unwrap();
    let game = dir.path().join("game");
    let mods = game.join("mods");
    std::fs::create_dir_all(&mods).unwrap();
    for (name, bytes) in files {
        std::fs::write(mods.join(name), bytes).unwrap();
    }
    let index = dir.path().join("launcher").join(ADDON_INDEX_FILE);
    Game {
        _dir: dir,
        game,
        mods,
        index,
    }
}

impl Game {
    fn addons(&self) -> Addons {
        Addons::new(&self.game, &self.index).without_trash()
    }

    fn has(&self, name: &str) -> bool {
        self.mods.join(name).is_file()
    }

    /// The check and plan the launcher would make for this folder.
    async fn plan(&self, host: &Host, env: &CheckEnvironment) -> RepairPlan {
        let snapshot = ModScanner::new().scan_dir(&self.mods);
        let origins = file_origins(&snapshot, &host.pack_files, &AddonIndex::default());
        let report = check_mods(&snapshot, env, &origins);
        let ctx = RepairContext {
            env,
            origins: &origins,
            has_pack: host.pack,
        };
        plan_repair(&ctx, Some(&report), None)
    }

    fn job<'a>(
        &self,
        host: &'a Host,
        env: CheckEnvironment,
        client: ModrinthClient,
        progress: Option<RepairProgressFn>,
    ) -> RepairJob<'a, Host> {
        RepairJob {
            host,
            mods_dir: self.mods.clone(),
            env,
            addons: self.addons(),
            client,
            downloader: downloader(),
            cancel: CancellationToken::new(),
            progress,
        }
    }
}

fn status_of<'a>(outcome: &'a RepairOutcome, key: &str) -> (&'a StepStatus, Option<StepNote>) {
    let state = outcome
        .steps
        .iter()
        .find(|s| s.key == key)
        .unwrap_or_else(|| panic!("no step {key}: {:#?}", outcome.steps));
    (&state.status, state.note)
}

// --- inputs ------------------------------------------------------------------------------------

#[tokio::test]
async fn origins_prefer_the_pack_and_need_current_records() {
    let g = game(&[
        ("pack.jar", fabric_jar("packmod", "1.0", "")),
        ("mr.jar.disabled", fabric_jar("mrmod", "1.0", "")),
        ("stale.jar", fabric_jar("stale", "1.0", "")),
        ("manual.jar", fabric_jar("manual", "1.0", "")),
    ]);
    let record = |name: &str, project: &str, size_delta: u64| {
        let meta = std::fs::metadata(g.mods.join(name)).unwrap();
        AddonRecord {
            size: meta.len() + size_delta,
            modified_ms: modified_ms(&meta),
            sha1: "x".into(),
            source: Some(AddonSource {
                project_id: project.into(),
                version_id: format!("{project}-v"),
                ..AddonSource::default()
            }),
            looked_up_ms: None,
        }
    };
    let mut index = AddonIndex::default();
    index
        .files
        .insert("mods/mr.jar".into(), record("mr.jar.disabled", "MR", 0));
    index
        .files
        .insert("mods/stale.jar".into(), record("stale.jar", "OLD", 7));
    index
        .files
        .insert("mods/pack.jar".into(), record("pack.jar", "PACK", 0));
    let snapshot = ModScanner::new().scan_dir(&g.mods);
    let origins = file_origins(
        &snapshot,
        &["mods/pack.jar".into(), "config/x.json".into()],
        &index,
    );
    assert_eq!(
        origins.get("pack.jar"),
        Some(&FileOrigin::Pack {
            path: "mods/pack.jar".into()
        })
    );
    assert_eq!(
        origins.get("mr.jar"),
        Some(&FileOrigin::Modrinth {
            project_id: "MR".into(),
            version_id: Some("MR-v".into())
        })
    );
    // A record for another size describes an older file: unknown origin.
    assert_eq!(origins.get("stale.jar"), None);
    assert_eq!(origins.get("manual.jar"), None);
}

#[test]
fn environment_inputs() {
    assert_eq!(
        java_major_from_version_json(br#"{"id":"1.21.1","javaVersion":{"majorVersion":21}}"#),
        Some(21)
    );
    assert_eq!(
        java_major_from_version_json(br#"{"id":"1.12.2","type":"release"}"#),
        Some(8)
    );
    assert_eq!(
        java_major_from_version_json(br#"{"id":"fabric-loader","inheritsFrom":"1.21.1"}"#),
        None
    );
    assert_eq!(java_major_from_version_json(b"not json"), None);

    let forge = |v| loader_version_for_check(LoaderKind::Forge, "1.20.1", Some(v));
    assert_eq!(forge("47.4.10").as_deref(), Some("47.4.10"));
    assert_eq!(forge("1.20.1-47.4.10").as_deref(), Some("47.4.10"));
    assert_eq!(forge("  "), None);
    assert_eq!(
        loader_version_for_check(LoaderKind::Vanilla, "1.21.1", Some("x")),
        None
    );
    assert_eq!(
        loader_version_for_check(LoaderKind::Fabric, "1.21.1", Some("0.16.10")).as_deref(),
        Some("0.16.10")
    );
    let env = check_environment(
        " 1.21.1 ",
        LoaderKind::NeoForge,
        Some("1.21.1-21.1.77"),
        None,
    );
    assert_eq!(env.minecraft, "1.21.1");
    assert_eq!(env.loader_version.as_deref(), Some("21.1.77"));
}

fn mv(id: &str, number: &str, kind: ModrinthVersionType) -> ModrinthVersion {
    ModrinthVersion {
        id: id.into(),
        project_id: "P".into(),
        author_id: None,
        name: id.into(),
        version_number: number.into(),
        changelog: None,
        dependencies: Vec::new(),
        game_versions: vec!["1.21.1".into()],
        version_type: kind,
        loaders: vec!["fabric".into()],
        featured: false,
        status: None,
        date_published: String::new(),
        downloads: 0,
        files: Vec::new(),
    }
}

#[test]
fn versions_are_picked_by_requirement_and_type() {
    use ModrinthVersionType::{Alpha, Beta, Release};
    let target = ModrinthResolveTarget::for_loader(LoaderKind::Fabric, "1.21.1");
    let mut other_game = mv("x", "9.0.0", Release);
    other_game.game_versions = vec!["1.20.1".into()];
    let versions = vec![
        other_game,
        mv("b3", "3.1.0-beta.1", Beta),
        mv("r2", "2.5.0", Release),
        mv("weird", "mc1.21-2.6", Release),
        mv("r1", "1.9.0", Release),
    ];
    let pick = |reqs: &[&str]| {
        let reqs: Vec<String> = reqs.iter().map(|s| (*s).to_owned()).collect();
        pick_version(&versions, &target, LoaderKind::Fabric, &reqs).map(|v| v.id.as_str())
    };
    // Newest release the target accepts, before the beta.
    assert_eq!(pick(&[]), Some("r2"));
    // Certainly meets the requirement, even if older.
    assert_eq!(pick(&["<2.0"]), Some("r1"));
    // Only the beta certainly fits, so it wins over a release that may.
    assert_eq!(pick(&[">=3.0-"]), Some("b3"));
    // Nothing certain: a version whose number cannot be read may still fit.
    assert_eq!(pick(&[">=2.6"]), Some("b3"));
    assert_eq!(pick(&[">=4"]), Some("weird"));
    let only_alpha = vec![mv("a1", "1.0.0-alpha", Alpha)];
    assert_eq!(
        pick_version(&only_alpha, &target, LoaderKind::Fabric, &[]).map(|v| v.id.as_str()),
        Some("a1")
    );

    let loaders = vec![
        LoaderVersion {
            version: "0.17.0-beta.1".into(),
            stable: false,
            recommended: false,
        },
        LoaderVersion {
            version: "0.16.10".into(),
            stable: true,
            recommended: true,
        },
        LoaderVersion {
            version: "0.15.11".into(),
            stable: true,
            recommended: false,
        },
    ];
    let loader = |reqs: &[&str]| {
        let reqs: Vec<String> = reqs.iter().map(|s| (*s).to_owned()).collect();
        pick_loader_version(&loaders, LoaderKind::Fabric, &reqs)
    };
    assert_eq!(loader(&[">=0.16.0"]).as_deref(), Some("0.16.10"));
    assert_eq!(loader(&[">=0.15", "<0.16"]).as_deref(), Some("0.15.11"));
    assert_eq!(loader(&[">=0.18"]), None);
}

#[test]
fn network_steps_still_needed() {
    let snapshot = ModSnapshot::new(
        "mods",
        vec![
            crate::mods::scan::ScannedJar::from_bytes(
                "api.jar",
                &fabric_jar("fabric-api", "1", ""),
            ),
            crate::mods::scan::ScannedJar::from_bytes(
                "old.jar.disabled",
                &fabric_jar("old", "1", ""),
            ),
            crate::mods::scan::ScannedJar::from_bytes("on.jar", &fabric_jar("on", "1", "")),
        ],
    );
    let fetch = |id: &str| RepairAction::FetchDependency {
        mod_id: id.into(),
        required_by: Vec::new(),
        lookup: crate::mods::repair::ModrinthLookup {
            project_id: None,
            slugs: vec![id.into()],
            loader: LoaderKind::Fabric,
            minecraft: "1.21.1".into(),
            version_reqs: Vec::new(),
        },
    };
    let update = |file: &str| RepairAction::UpdateToCompatible {
        file: file.into(),
        mod_id: "x".into(),
        disable_if_unavailable: false,
        lookup: crate::mods::repair::ModrinthLookup {
            project_id: None,
            slugs: Vec::new(),
            loader: LoaderKind::Fabric,
            minecraft: "1.21.1".into(),
            version_reqs: Vec::new(),
        },
    };
    assert!(!still_needed(&fetch("fabric-api"), &snapshot));
    assert!(still_needed(&fetch("sodium"), &snapshot));
    assert!(!still_needed(&update("old.jar"), &snapshot));
    assert!(!still_needed(&update("gone.jar"), &snapshot));
    assert!(still_needed(&update("on.jar"), &snapshot));
}

// --- repairs -----------------------------------------------------------------------------------

/// Everything in one run: a broken pack file goes back through the pack sync, a disabled
/// dependency comes back on, the older duplicate goes off, a Forge jar Modrinth does not know
/// is turned off, and the missing Fabric API comes from Modrinth. A handful of requests.
#[tokio::test]
async fn a_repair_runs_every_phase_in_order() {
    let g = game(&[
        ("broken.jar", b"<html>".to_vec()),
        ("dup-old.jar", fabric_jar("dup", "0.5.0", "")),
        ("dup-new.jar", fabric_jar("dup", "0.6.0", "")),
        (
            "needy.jar",
            fabric_jar("needy", "1.0", r#""depends": {"lib": "*"}"#),
        ),
        ("lib.jar.disabled", fabric_jar("lib", "1.0", "")),
        (
            "wants-api.jar",
            fabric_jar("wantsapi", "1.0", r#""depends": {"fabric-api": "*"}"#),
        ),
        ("forge-mod.jar", toml_jar(false, "create", "0.5.1", "")),
    ]);
    let host = Host {
        pack: true,
        pack_files: vec!["mods/broken.jar".into()],
        ..Host::default()
    };
    host.on_sync
        .lock()
        .unwrap()
        .push((g.mods.join("broken.jar"), fabric_jar("broken", "1.0", "")));

    let api_jar = fabric_jar("fabric-api", "0.100.0", "");
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    let jar = api_jar.clone();
    let routes: Routes = Arc::new(move |method, url, host| {
        if url.path().starts_with("/v2/") {
            counted.fetch_add(1, Ordering::SeqCst);
        }
        let known = [project("API1", "fabric-api", "Fabric API")];
        match (method, url.path()) {
            ("POST", "/v2/version_files") => json_body(json!({})),
            ("GET", "/v2/projects") => projects_for(url, &known),
            ("GET", "/v2/project/API1/version") => json_body(json!([version(
                host,
                "apiv1",
                "API1",
                "fabric-api-0.100.jar",
                &jar,
                "0.100.0+1.21.1"
            )])),
            ("GET", "/files/fabric-api-0.100.jar") => (200, jar.clone()),
            _ => (404, b"{}".to_vec()),
        }
    });
    let server = serve(routes).await;

    let env = env();
    let plan = g.plan(&host, &env).await;
    let keys: Vec<String> = plan.steps.iter().map(|s| step_key(&s.action)).collect();
    assert_eq!(
        keys,
        [
            "pack",
            "pack-file:mods/broken.jar",
            "enable:lib.jar.disabled",
            "disable:dup-old.jar",
            "update:forge-mod.jar",
            "fetch:fabric-api"
        ]
    );

    let seen = Arc::new(Mutex::new(Vec::<RepairProgress>::new()));
    let record = Arc::clone(&seen);
    let progress: RepairProgressFn =
        Arc::new(move |p: &RepairProgress| record.lock().unwrap().push(p.clone()));
    let job = g.job(&host, env, client(&server), Some(progress));
    let outcome = run_repair(&job, plan, None).await.unwrap();

    assert!(!outcome.cancelled);
    for key in [
        "pack",
        "pack-file:mods/broken.jar",
        "enable:lib.jar.disabled",
        "disable:dup-old.jar",
        "fetch:fabric-api",
    ] {
        assert_eq!(status_of(&outcome, key), (&StepStatus::Done, None), "{key}");
    }
    assert_eq!(
        status_of(&outcome, "update:forge-mod.jar"),
        (&StepStatus::Done, Some(StepNote::DisabledInstead))
    );
    assert!(
        !outcome.report.has_blocking(),
        "{:#?}",
        outcome.report.issues
    );
    assert!(g.has("lib.jar") && !g.has("lib.jar.disabled"));
    assert!(g.has("dup-old.jar.disabled") && g.has("dup-new.jar"));
    assert!(g.has("forge-mod.jar.disabled"));
    assert!(g.has("fabric-api-0.100.jar"));

    // One pack sync that verifies and puts the broken file back.
    let requests = host.requests.lock().unwrap().clone();
    assert_eq!(
        requests,
        [PackRequest {
            sync: true,
            revert: vec!["mods/broken.jar".into()],
            ..PackRequest::default()
        }]
    );
    let api_calls = calls.load(Ordering::SeqCst);
    assert!(api_calls <= 8, "{api_calls} Modrinth requests");

    // The UI saw the steps run and a check between the phases.
    let seen = seen.lock().unwrap();
    assert!(seen.iter().any(|p| p.checking));
    assert!(seen.iter().any(|p| p.percent.is_some()));
    assert!(
        seen.iter()
            .any(|p| p.steps.iter().any(|s| s.status == StepStatus::Running))
    );
}

/// A missing dependency the official pack offers is switched on in the pack, not downloaded.
#[tokio::test]
async fn a_dependency_the_pack_offers_comes_from_the_pack() {
    let g = game(&[(
        "wants-api.jar",
        fabric_jar("wantsapi", "1.0", r#""depends": {"fabric-api": "*"}"#),
    )]);
    let host = Host {
        pack: true,
        pack_items: vec![("api-item".into(), "API1".into(), "fabric-api".into())],
        ..Host::default()
    };
    let downloads = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&downloads);
    let routes: Routes = Arc::new(move |method, url, _| {
        if url.path().starts_with("/files/") {
            counted.fetch_add(1, Ordering::SeqCst);
        }
        let known = [project("API1", "fabric-api", "Fabric API")];
        match (method, url.path()) {
            ("GET", "/v2/projects") => projects_for(url, &known),
            _ => (404, b"{}".to_vec()),
        }
    });
    let server = serve(routes).await;
    let env = env();
    let plan = g.plan(&host, &env).await;
    let job = g.job(&host, env, client(&server), None);
    let outcome = run_repair(&job, plan, None).await.unwrap();
    assert_eq!(
        status_of(&outcome, "fetch:fabric-api"),
        (&StepStatus::Done, Some(StepNote::FromPack))
    );
    let requests = host.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 2, "{requests:#?}");
    assert_eq!(requests[1].enable_items, vec!["api-item".to_owned()]);
    assert_eq!(downloads.load(Ordering::SeqCst), 0);
}

/// An own mod made for another Minecraft version with no fitting version on Modrinth is turned
/// off; its project comes from the add-on index, so no hash lookup is made.
#[tokio::test]
async fn an_update_without_a_fitting_version_turns_the_mod_off() {
    let g = game(&[(
        "oldmc.jar",
        fabric_jar("oldmc", "1.0", r#""depends": {"minecraft": "1.20.x"}"#),
    )]);
    let meta = std::fs::metadata(g.mods.join("oldmc.jar")).unwrap();
    let mut index = AddonIndex::default();
    index.files.insert(
        "mods/oldmc.jar".into(),
        AddonRecord {
            size: meta.len(),
            modified_ms: modified_ms(&meta),
            sha1: "x".into(),
            source: Some(AddonSource {
                project_id: "OLD1".into(),
                version_id: "V1".into(),
                ..AddonSource::default()
            }),
            looked_up_ms: Some(1),
        },
    );
    index.save(&g.index).await.unwrap();
    let hashes = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&hashes);
    let routes: Routes = Arc::new(move |method, url, _| match (method, url.path()) {
        ("POST", "/v2/version_files") => {
            counted.fetch_add(1, Ordering::SeqCst);
            json_body(json!({}))
        }
        ("GET", "/v2/project/OLD1/version") => json_body(json!([])),
        _ => (404, b"{}".to_vec()),
    });
    let server = serve(routes).await;
    let host = Host::default();
    let env = env();
    // The launcher plans with the index's origins.
    let snapshot = ModScanner::new().scan_dir(&g.mods);
    let origins = file_origins(&snapshot, &[], &index);
    let report = check_mods(&snapshot, &env, &origins);
    let ctx = RepairContext {
        env: &env,
        origins: &origins,
        has_pack: false,
    };
    let plan = plan_repair(&ctx, Some(&report), None);
    let job = g.job(&host, env, client(&server), None);
    let outcome = run_repair(&job, plan, None).await.unwrap();
    assert_eq!(
        status_of(&outcome, "update:oldmc.jar"),
        (&StepStatus::Done, Some(StepNote::DisabledInstead))
    );
    assert!(g.has("oldmc.jar.disabled"));
    assert_eq!(hashes.load(Ordering::SeqCst), 0);
    assert!(outcome.report.issues.is_empty());
}

/// A newer loader is chosen from the stable versions; a Java the game cannot have is reported.
#[tokio::test]
async fn loader_and_java_steps() {
    let g = game(&[(
        "modern.jar",
        fabric_jar(
            "modern",
            "1.0",
            r#""depends": {"fabricloader": ">=0.16.0", "java": ">=25"}"#,
        ),
    )]);
    let host = Host {
        loader_versions: vec![
            LoaderVersion {
                version: "0.17.0-beta.1".into(),
                stable: false,
                recommended: false,
            },
            LoaderVersion {
                version: "0.16.10".into(),
                stable: true,
                recommended: true,
            },
        ],
        java: Some(21),
        ..Host::default()
    };
    let env = check_environment("1.21.1", LoaderKind::Fabric, Some("0.15.0"), Some(21));
    let plan = g.plan(&host, &env).await;
    let job = g.job(&host, env, client("http://127.0.0.1:9"), None);
    let outcome = run_repair(&job, plan, None).await.unwrap();
    assert_eq!(
        status_of(&outcome, "loader:Fabric"),
        (&StepStatus::Done, None)
    );
    assert_eq!(
        status_of(&outcome, "java"),
        (&StepStatus::Failed, Some(StepNote::NotPossible))
    );
    assert_eq!(
        host.log.lock().unwrap().clone(),
        ["loader 0.16.10", "java 25"]
    );
    // The loader is fine now; Java is what is left.
    let left: Vec<_> = outcome.report.issues.iter().map(|i| i.kind).collect();
    assert_eq!(left, [crate::mods::check::IssueKind::JavaVersion]);
    assert_eq!(outcome.report.issues[0].severity, Severity::Blocking);
}

#[tokio::test]
async fn a_cancelled_repair_changes_nothing() {
    let g = game(&[
        ("dup-old.jar", fabric_jar("dup", "0.5.0", "")),
        ("dup-new.jar", fabric_jar("dup", "0.6.0", "")),
    ]);
    let host = Host::default();
    let env = env();
    let plan = g.plan(&host, &env).await;
    let job = g.job(&host, env, client("http://127.0.0.1:9"), None);
    job.cancel.cancel();
    let outcome = run_repair(&job, plan, None).await.unwrap();
    assert!(outcome.cancelled);
    assert_eq!(
        status_of(&outcome, "disable:dup-old.jar"),
        (&StepStatus::Skipped, Some(StepNote::Cancelled))
    );
    assert!(g.has("dup-old.jar"));
    assert!(outcome.report.has_blocking());
}

// --- reinstall ---------------------------------------------------------------------------------

/// The set goes to the trash, unknown jars are kept aside, the pack and the Modrinth mods come
/// back (turned off when they were off), and a mod without a version gets its old file back.
#[tokio::test]
async fn a_reinstall_starts_the_set_again_and_loses_nothing() {
    let g = game(&[
        ("pack.jar", fabric_jar("packmod", "1.0", "")),
        ("mr.jar.disabled", fabric_jar("mrmod", "1.0", "")),
        ("gone.jar", fabric_jar("gonemod", "1.0", "")),
        ("manual.jar", fabric_jar("manual", "1.0", "")),
    ]);
    let config = g.game.join("config");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(config.join("sodium-options.json"), b"{}").unwrap();
    let host = Host {
        pack: true,
        pack_files: vec!["mods/pack.jar".into()],
        ..Host::default()
    };
    host.on_sync
        .lock()
        .unwrap()
        .push((g.mods.join("pack.jar"), fabric_jar("packmod", "1.0", "")));
    let mr_jar = fabric_jar("mrmod", "2.0", "");
    let jar = mr_jar.clone();
    let routes: Routes = Arc::new(move |method, url, host| {
        let known = [project("MR", "mrmod", "MR Mod")];
        match (method, url.path()) {
            ("POST", "/v2/version_files") => json_body(json!({})),
            ("GET", "/v2/projects") => projects_for(url, &known),
            ("GET", "/v2/project/MR/version") => json_body(json!([version(
                host,
                "mrv2",
                "MR",
                "mr-2.0.jar",
                &jar,
                "2.0"
            )])),
            ("GET", "/v2/project/GONE/version") => json_body(json!([])),
            ("GET", "/files/mr-2.0.jar") => (200, jar.clone()),
            _ => (404, b"{}".to_vec()),
        }
    });
    let server = serve(routes).await;

    let snapshot = ModScanner::new().scan_dir(&g.mods);
    let mut origins = FileOrigins::new();
    origins.insert(
        "pack.jar".into(),
        FileOrigin::Pack {
            path: "mods/pack.jar".into(),
        },
    );
    for (file, project) in [("mr.jar", "MR"), ("gone.jar", "GONE")] {
        origins.insert(
            file.into(),
            FileOrigin::Modrinth {
                project_id: project.into(),
                version_id: Some(format!("{project}-old")),
            },
        );
    }
    let keep_dir = g.game.join(".projects").join("kept-mods").join("1.21.1");
    let target = ReinstallTarget {
        minecraft: "1.21.1".into(),
        loader: LoaderKind::Fabric,
        mods_dir: g.mods.clone(),
        config_dir: config.clone(),
        keep_dir: keep_dir.clone(),
        has_pack: true,
    };
    let plan = plan_reinstall(&target, &snapshot, &origins);
    let trash_dir = g.game.parent().unwrap().join("trash");
    let phases = Arc::new(Mutex::new(Vec::<ReinstallProgress>::new()));
    let record = Arc::clone(&phases);
    let job = ReinstallJob {
        host: &host,
        plan,
        trash: LauncherTrash::new(&trash_dir),
        instance_id: "inst".into(),
        env: env(),
        addons: g.addons(),
        client: client(&server),
        downloader: downloader(),
        cancel: CancellationToken::new(),
        progress: Some(Arc::new(move |p: &ReinstallProgress| {
            record.lock().unwrap().push(p.clone());
        })),
    };
    let outcome = run_reinstall(&job).await.unwrap();

    assert!(!outcome.cancelled);
    assert!(outcome.pack_synced);
    assert!(outcome.settings_reset);
    assert_eq!(outcome.reinstalled, ["mrmod name"]);
    assert_eq!(outcome.restored, ["gonemod name"]);
    assert_eq!(outcome.kept.len(), 1);
    assert_eq!(outcome.kept[0].to, keep_dir.join("manual.jar"));
    assert!(keep_dir.join("manual.jar").is_file());
    // The new set: the pack file, the Modrinth mod (still off), the old file of the mod with
    // no version; the unknown jar only in the keep folder.
    assert!(g.has("pack.jar"));
    assert!(g.has("mr-2.0.jar.disabled"));
    assert!(g.has("gone.jar"));
    assert!(!g.has("manual.jar") && !g.has("mr.jar.disabled"));
    assert!(config.is_dir());
    assert_eq!(std::fs::read_dir(&config).unwrap().count(), 0);
    let trashed = LauncherTrash::new(&trash_dir)
        .list(Some("inst"))
        .await
        .unwrap();
    assert_eq!(trashed.len(), 2);
    assert!(trashed.iter().all(|e| e.kind == TrashKind::ModSet));
    assert_eq!(
        host.requests.lock().unwrap().clone(),
        [PackRequest {
            sync: true,
            reinstall: true,
            ..PackRequest::default()
        }]
    );
    assert!(!outcome.report.has_blocking());
    let phases = phases.lock().unwrap();
    let order: Vec<ReinstallPhase> = phases.iter().map(|p| p.phase).fold(Vec::new(), |mut v, p| {
        if v.last() != Some(&p) {
            v.push(p);
        }
        v
    });
    assert_eq!(
        order,
        [
            ReinstallPhase::KeepAside,
            ReinstallPhase::Trash,
            ReinstallPhase::Pack,
            ReinstallPhase::Modrinth,
            ReinstallPhase::Check
        ]
    );
    assert!(phases[0].cancellable && !phases.last().unwrap().cancellable);
}

#[tokio::test]
async fn a_reinstall_stopped_before_the_move_changes_nothing() {
    let g = game(&[("manual.jar", fabric_jar("manual", "1.0", ""))]);
    let host = Host::default();
    let snapshot = ModScanner::new().scan_dir(&g.mods);
    let target = ReinstallTarget {
        minecraft: "1.21.1".into(),
        loader: LoaderKind::Fabric,
        mods_dir: g.mods.clone(),
        config_dir: g.game.join("config"),
        keep_dir: g.game.join("kept"),
        has_pack: false,
    };
    let plan = plan_reinstall(&target, &snapshot, &FileOrigins::new());
    let job = ReinstallJob {
        host: &host,
        plan,
        trash: LauncherTrash::new(g.game.join("trash")),
        instance_id: "inst".into(),
        env: env(),
        addons: g.addons(),
        client: client("http://127.0.0.1:9"),
        downloader: downloader(),
        cancel: CancellationToken::new(),
        progress: None,
    };
    job.cancel.cancel();
    let outcome = run_reinstall(&job).await.unwrap();
    assert!(outcome.cancelled);
    assert!(g.has("manual.jar"));
    assert!(Path::new(&g.game.join("kept").join("manual.jar")).is_file());
}

use std::sync::Arc;

use pl_net::{Downloader, HashAlgo, Http, HttpConfig, hash_bytes};
use serde_json::{Value, json};

use super::*;
use crate::modrinth::tests::{Handler, client_for, mock_api};
use crate::modrinth::{ModrinthClient, ModrinthResolveTarget};

fn sha1(data: &[u8]) -> String {
    hash_bytes(data, HashAlgo::Sha1)
}

fn sha512(data: &[u8]) -> String {
    hash_bytes(data, HashAlgo::Sha512)
}

/// A version of `project` whose primary file `name` has `content`, served by the mock server.
#[allow(clippy::too_many_arguments)]
fn version(
    host: &str,
    id: &str,
    project: &str,
    name: &str,
    content: &[u8],
    deps: Value,
    date: &str,
    kind: &str,
) -> Value {
    json!({
        "id": id, "project_id": project, "author_id": "a", "name": format!("{project} {id}"),
        "version_number": format!("{id}-number"), "changelog": null, "dependencies": deps,
        "game_versions": ["1.21.1"], "version_type": kind, "loaders": ["fabric"],
        "featured": false, "status": "listed", "date_published": date, "downloads": 1,
        "files": [{
            "hashes": {"sha1": sha1(content), "sha512": sha512(content)},
            "url": format!("{host}/files/{name}"), "filename": name, "primary": true,
            "size": content.len(), "file_type": null
        }]
    })
}

fn project(id: &str, title: &str) -> Value {
    json!({
        "id": id, "slug": id.to_lowercase(), "project_type": "mod", "team": "t", "title": title,
        "description": "d", "body": "", "published": "2024-01-01T00:00:00Z",
        "updated": "2024-01-01T00:00:00Z", "status": "approved", "license": null,
        "client_side": "required", "server_side": "optional", "downloads": 1, "followers": 1,
        "categories": [], "additional_categories": [], "game_versions": ["1.21.1"],
        "loaders": ["fabric"], "versions": [],
        "icon_url": format!("https://cdn.modrinth.com/data/{id}/icon.png"),
        "issues_url": null, "source_url": null, "wiki_url": null, "discord_url": null,
        "donation_urls": [], "gallery": [], "color": null
    })
}

fn setup(dir: &Path) -> Addons {
    Addons::new(dir.join("game"), dir.join("index").join(ADDON_INDEX_FILE)).without_trash()
}

fn downloader() -> Downloader {
    Downloader::new(Http::new(HttpConfig::new("pl-mc-tests")).unwrap())
}

fn target() -> ModrinthResolveTarget {
    ModrinthResolveTarget {
        loaders: vec!["fabric".into()],
        game_versions: vec!["1.21.1".into()],
    }
}

#[tokio::test]
async fn index_round_trip_and_cheap_lookups() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("addons.json");
    assert_eq!(AddonIndex::load(&path).await, AddonIndex::default());

    let mut index = AddonIndex::default();
    index.files.insert(
        index_key("mods", "sodium.jar.disabled"),
        AddonRecord {
            size: 10,
            modified_ms: Some(5),
            sha1: "ab".into(),
            source: Some(AddonSource {
                project_id: "AANobbMI".into(),
                version_id: "v".into(),
                version_number: Some("1".into()),
                title: Some("Sodium".into()),
                icon_url: None,
            }),
            looked_up_ms: Some(1),
        },
    );
    index.save(&path).await.unwrap();
    let loaded = AddonIndex::load(&path).await;
    assert_eq!(loaded, index);
    assert!(loaded.files.contains_key("mods/sodium.jar"));
    // Enabled and disabled spellings share a record; size and time must still match.
    let hit = loaded.source_for("mods", "sodium.jar", 10, Some(5));
    assert_eq!(hit.and_then(|s| s.title.as_deref()), Some("Sodium"));
    assert!(
        loaded
            .source_for("mods", "sodium.jar", 11, Some(5))
            .is_none()
    );
    assert!(
        loaded
            .source_for("mods", "sodium.jar", 10, Some(6))
            .is_none()
    );
    assert!(loaded.source_for("mods", "sodium.jar", 10, None).is_some());

    std::fs::write(&path, b"{ not json").unwrap();
    assert_eq!(AddonIndex::load(&path).await, AddonIndex::default());
}

#[tokio::test]
async fn scan_hashes_files_reuses_hashes_and_prunes_gone_files() {
    let dir = tempfile::tempdir().unwrap();
    let addons = setup(dir.path());
    let mods = dir.path().join("game").join("mods");
    std::fs::create_dir_all(&mods).unwrap();
    std::fs::write(mods.join("a.jar"), b"aaa").unwrap();
    std::fs::write(mods.join("b.jar.disabled"), b"bbb").unwrap();
    std::fs::write(mods.join("notes.txt"), b"x").unwrap();
    std::fs::write(mods.join(".a.jar.part"), b"x").unwrap();
    let packs = dir.path().join("game").join("resourcepacks");
    std::fs::create_dir_all(packs.join("folder-pack")).unwrap();
    std::fs::write(packs.join("p.zip"), b"ppp").unwrap();

    let entries = addons.scan(ImportKind::Mod).await.unwrap();
    let names: Vec<_> = entries.iter().map(|e| e.file_name.as_str()).collect();
    assert_eq!(names, vec!["a.jar", "b.jar.disabled"]);
    assert_eq!(entries[0].sha1, sha1(b"aaa"));
    assert!(!entries[1].enabled);

    // A stale hash is kept while size and time match (proves the cache is used).
    let mut index = addons.load_index().await;
    index.files.get_mut("mods/a.jar").unwrap().sha1 = "cached".into();
    addons.save_index(&index).await.unwrap();
    let entries = addons.scan(ImportKind::Mod).await.unwrap();
    assert_eq!(entries[0].sha1, "cached");

    std::fs::remove_file(mods.join("b.jar.disabled")).unwrap();
    addons.scan(ImportKind::Mod).await.unwrap();
    let index = addons.load_index().await;
    assert!(!index.files.contains_key("mods/b.jar"));

    let packs = addons.scan(ImportKind::ResourcePack).await.unwrap();
    assert_eq!(packs.len(), 1);
    assert_eq!(packs[0].file_name, "p.zip");
    // Scanning one folder never drops records of another.
    assert!(addons.load_index().await.files.contains_key("mods/a.jar"));
}

/// Root `new1` of project P1 replaces the installed (disabled) `old1` file and pulls in the
/// required project P2; everything is verified, recorded and titled.
#[tokio::test]
async fn install_replaces_old_files_and_installs_dependencies() {
    let dir = tempfile::tempdir().unwrap();
    let addons = setup(dir.path());
    let mods = dir.path().join("game").join("mods");
    std::fs::create_dir_all(&mods).unwrap();
    let old_content = b"old p1 jar".to_vec();
    std::fs::write(mods.join("p1-old.jar.disabled"), &old_content).unwrap();
    std::fs::write(mods.join("other.jar"), b"unrelated").unwrap();

    let p1_new = b"new p1 jar".to_vec();
    let p2 = b"p2 library".to_vec();
    let host = Arc::new(parking_lot::Mutex::new(String::new()));
    let handler_host = Arc::clone(&host);
    let old_sha1 = sha1(&old_content);
    let handler: Handler = Arc::new(move |method, url| {
        let host = handler_host.lock().clone();
        let path = url.path();
        let ok = |body: Value| (200, Vec::new(), body.to_string());
        let old = version(
            &host,
            "old1",
            "P1",
            "p1-old.jar",
            b"old p1 jar",
            json!([]),
            "2024-01-01T00:00:00Z",
            "release",
        );
        match (method, path) {
            ("POST", "/v2/version_files") => ok(json!({ old_sha1.clone(): old })),
            ("GET", "/v2/projects") => ok(json!([
                project("P1", "Project One"),
                project("P2", "Library Two")
            ])),
            ("GET", "/v2/project/P2/version") => ok(json!([version(
                &host,
                "v2",
                "P2",
                "p2.jar",
                b"p2 library",
                json!([]),
                "2024-02-01T00:00:00Z",
                "release"
            )])),
            ("GET", "/files/p1-new.jar") => (200, Vec::new(), "new p1 jar".into()),
            ("GET", "/files/p2.jar") => (200, Vec::new(), "p2 library".into()),
            _ => (404, Vec::new(), "{}".into()),
        }
    });
    let base = mock_api(handler).await;
    *host.lock() = base.trim_end_matches("/v2").to_owned();
    let client: ModrinthClient = client_for(&base);

    let root_json = version(
        &host.lock(),
        "new1",
        "P1",
        "p1-new.jar",
        &p1_new,
        json!([{"project_id": "P2", "version_id": null, "file_name": null, "dependency_type": "required"}]),
        "2024-03-01T00:00:00Z",
        "release",
    );
    let mut root_value = root_json;
    crate::modrinth::camelize_keys(&mut root_value);
    let root: ModrinthVersion = serde_json::from_value(root_value).unwrap();

    let seen = Arc::new(parking_lot::Mutex::new(Vec::new()));
    let seen_progress = Arc::clone(&seen);
    let report = addons
        .install(
            &client,
            &downloader(),
            ImportKind::Mod,
            vec![root.clone()],
            &target(),
            AddonInstallOptions {
                progress: Some(Arc::new(move |p| seen_progress.lock().push(p))),
                ..AddonInstallOptions::default()
            },
        )
        .await
        .unwrap();

    let installed: Vec<_> = report
        .installed
        .iter()
        .map(|i| (i.file_name.as_str(), i.dependency, i.title.as_deref()))
        .collect();
    assert_eq!(
        installed,
        vec![
            ("p1-new.jar.disabled", false, Some("Project One")),
            ("p2.jar", true, Some("Library Two")),
        ]
    );
    assert_eq!(report.replaced, vec!["p1-old.jar.disabled"]);
    assert!(!mods.join("p1-old.jar.disabled").exists());
    assert_eq!(
        std::fs::read(mods.join("p1-new.jar.disabled")).unwrap(),
        p1_new
    );
    assert_eq!(std::fs::read(mods.join("p2.jar")).unwrap(), p2);
    assert!(mods.join("other.jar").exists());
    {
        let progress = seen.lock();
        assert_eq!(progress.first(), Some(&AddonProgress::Resolving));
        assert_eq!(progress.last(), Some(&AddonProgress::Finishing));
    }

    let index = addons.load_index().await;
    let p1 = index.files.get("mods/p1-new.jar").unwrap();
    assert_eq!(p1.sha1, sha1(&p1_new));
    let source = p1.source.as_ref().unwrap();
    assert_eq!(
        (source.project_id.as_str(), source.version_id.as_str()),
        ("P1", "new1")
    );
    assert!(!index.files.contains_key("mods/p1-old.jar"));

    // Installing the same version again changes nothing.
    let again = addons
        .install(
            &client,
            &downloader(),
            ImportKind::Mod,
            vec![root],
            &target(),
            AddonInstallOptions::default(),
        )
        .await
        .unwrap();
    assert!(again.installed.is_empty());
    assert_eq!(again.already_installed.len(), 1);

    // Removal deletes the file and forgets it.
    addons.remove(ImportKind::Mod, "p2.jar").await.unwrap();
    assert!(!mods.join("p2.jar").exists());
    assert!(!addons.load_index().await.files.contains_key("mods/p2.jar"));
    assert!(
        addons
            .remove(ImportKind::Mod, "../escape.jar")
            .await
            .is_err()
    );
}

#[tokio::test]
async fn updates_only_offer_newer_versions_of_the_same_type() {
    let dir = tempfile::tempdir().unwrap();
    let addons = setup(dir.path());
    let mods = dir.path().join("game").join("mods");
    std::fs::create_dir_all(&mods).unwrap();
    std::fs::write(mods.join("a.jar"), b"a-1").unwrap();
    std::fs::write(mods.join("b.jar"), b"b-2").unwrap();
    std::fs::write(mods.join("c.jar"), b"custom").unwrap();
    let (a_sha, b_sha) = (sha1(b"a-1"), sha1(b"b-2"));

    let handler: Handler = Arc::new(move |method, url| {
        let ok = |body: Value| (200, Vec::new(), body.to_string());
        let h = "http://unused";
        let a1 = version(
            h,
            "a1",
            "A",
            "a.jar",
            b"a-1",
            json!([]),
            "2024-01-01T00:00:00Z",
            "release",
        );
        let a2 = version(
            h,
            "a2",
            "A",
            "a2.jar",
            b"a-2",
            json!([]),
            "2024-05-01T00:00:00Z",
            "release",
        );
        let b2 = version(
            h,
            "b2",
            "B",
            "b.jar",
            b"b-2",
            json!([]),
            "2024-05-01T00:00:00Z",
            "release",
        );
        let b1 = version(
            h,
            "b1",
            "B",
            "b1.jar",
            b"b-1",
            json!([]),
            "2024-01-01T00:00:00Z",
            "release",
        );
        match (method, url.path()) {
            ("POST", "/v2/version_files") => ok(json!({ a_sha.clone(): a1, b_sha.clone(): b2 })),
            ("GET", "/v2/projects") => ok(json!([project("A", "Alpha"), project("B", "Beta")])),
            ("GET", "/v2/versions") => ok(json!([a1, b2])),
            // B's "latest" is older than what is installed: not an update.
            ("POST", "/v2/version_files/update") => {
                ok(json!({ a_sha.clone(): a2, b_sha.clone(): b1 }))
            }
            _ => (404, Vec::new(), "{}".into()),
        }
    });
    let base = mock_api(handler).await;
    let client = client_for(&base);
    let scan = addons
        .check_updates(&client, ImportKind::Mod, &target())
        .await
        .unwrap();
    assert!(!scan.offline);
    let by_name: HashMap<&str, &AddonEntry> = scan
        .entries
        .iter()
        .map(|e| (e.file_name.as_str(), e))
        .collect();
    let a = by_name["a.jar"];
    assert_eq!(a.source.as_ref().unwrap().title.as_deref(), Some("Alpha"));
    assert_eq!(a.update.as_ref().map(|u| u.version_id.as_str()), Some("a2"));
    assert!(by_name["b.jar"].update.is_none());
    assert!(by_name["c.jar"].source.is_none());

    // Unknown files are not asked about again right away.
    let index = addons.load_index().await;
    assert!(index.files["mods/c.jar"].looked_up_ms.is_some());
}

#[tokio::test]
async fn offline_lookups_keep_local_knowledge() {
    let dir = tempfile::tempdir().unwrap();
    let addons = setup(dir.path());
    let mods = dir.path().join("game").join("mods");
    std::fs::create_dir_all(&mods).unwrap();
    std::fs::write(mods.join("a.jar"), b"a").unwrap();
    // Nothing listens on this port.
    let client = client_for("http://127.0.0.1:9/v2");
    let scan = addons.identify(&client, ImportKind::Mod).await.unwrap();
    assert!(scan.offline);
    assert_eq!(scan.entries.len(), 1);
    assert!(scan.entries[0].source.is_none());
    // Not marked as looked up, so the next online scan asks again.
    assert!(
        addons.load_index().await.files["mods/a.jar"]
            .looked_up_ms
            .is_none()
    );
}

#[test]
fn icon_names_and_ids() {
    assert_eq!(
        icon_extension("https://cdn.modrinth.com/data/x/icon.webp"),
        "webp"
    );
    assert_eq!(
        icon_extension("https://cdn.modrinth.com/data/x/a_96.WEBP?x=1"),
        "webp"
    );
    assert_eq!(
        icon_extension("https://cdn.modrinth.com/data/x/icon"),
        "png"
    );
    assert!(is_plain_id("AANobbMI"));
    assert!(!is_plain_id("../x"));
    assert!(cached_icon(Path::new("."), "../../etc").is_none());
    assert_eq!(kind_for_folder("shaderpacks"), Some(ImportKind::ShaderPack));
    assert_eq!(kind_for_folder("config"), None);
    assert!(is_later("2024-05-01T00:00:00.5Z", "2024-05-01T00:00:00Z"));
}

/// Live check against Modrinth: installs Mod Menu (which needs Fabric API and Placeholder API)
/// into a temporary Fabric 1.21.1 game directory, then updates and removes.
/// Run with `cargo test -p pl-mc -- --ignored addons_live`.
#[tokio::test]
#[ignore = "needs the internet and downloads a few MB"]
async fn addons_live_install_with_dependencies() {
    let dir = tempfile::tempdir().unwrap();
    let addons = setup(dir.path());
    let http = Http::new(HttpConfig::new(
        "ProjectsLauncher-tests (+https://projects.gg)",
    ))
    .unwrap();
    let client = ModrinthClient::new(http.clone());
    let target = target();
    let root = client
        .best_version("modmenu", &target)
        .await
        .unwrap()
        .expect("Mod Menu has a 1.21.1 Fabric version");
    let report = addons
        .install(
            &client,
            &Downloader::new(http),
            ImportKind::Mod,
            vec![root],
            &target,
            AddonInstallOptions::default(),
        )
        .await
        .unwrap();
    let projects: HashSet<&str> = report
        .installed
        .iter()
        .map(|i| i.project_id.as_str())
        .collect();
    assert!(projects.contains("mOgUt4GM"), "Mod Menu: {report:?}");
    assert!(projects.contains("P7dR8mSH"), "Fabric API: {report:?}");
    let scan = addons.identify(&client, ImportKind::Mod).await.unwrap();
    assert!(scan.entries.iter().all(|e| e.source.is_some()));
}

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use pl_net::{Http, HttpConfig};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::resolve::pick_best;
use super::*;
use crate::loaders::LoaderKind;

type Handler = Arc<dyn Fn(&str, &Url) -> (u16, Vec<(&'static str, String)>, String) + Send + Sync>;

/// Minimal HTTP/1.1 server: one request per connection, routed through `handler`.
async fn mock_api(handler: Handler) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let handler = Arc::clone(&handler);
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
                let content_length = head
                    .lines()
                    .find_map(|l| {
                        let (k, v) = l.split_once(':')?;
                        k.eq_ignore_ascii_case("content-length")
                            .then(|| v.trim().parse::<usize>().ok())?
                    })
                    .unwrap_or(0);
                while raw.len() < header_end + content_length {
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
                let (status, headers, body) = handler(&method, &url);
                let mut response = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
                    body.len()
                );
                for (name, value) in headers {
                    response.push_str(&format!("{name}: {value}\r\n"));
                }
                response.push_str("\r\n");
                response.push_str(&body);
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.shutdown().await;
            });
        }
    });
    format!("http://{addr}/v2")
}

fn client_for(base: &str) -> ModrinthClient {
    ModrinthClient::with_base_url(Http::new(HttpConfig::new("pl-mc-tests")).unwrap(), base)
}

fn dep(project: Option<&str>, version: Option<&str>, kind: &str) -> Value {
    json!({"project_id": project, "version_id": version, "file_name": null, "dependency_type": kind})
}

fn version_json(id: &str, project: &str, kind: &str, deps: Value, sha1: &str) -> Value {
    json!({
        "id": id, "project_id": project, "author_id": "author", "name": format!("Version {id}"),
        "version_number": "1.0.0", "changelog": null, "changelog_url": null,
        "dependencies": deps, "game_versions": ["1.20.1"], "version_type": kind,
        "loaders": ["fabric"], "featured": false, "status": "listed", "requested_status": null,
        "date_published": "2024-01-01T00:00:00Z", "downloads": 10,
        "files": [{
            "hashes": {"sha1": sha1, "sha512": format!("{sha1}-512")},
            "url": format!("https://cdn.modrinth.com/data/{project}/{id}.jar"),
            "filename": format!("{id}.jar"), "primary": true, "size": 1234, "file_type": null
        }]
    })
}

fn decode_version(value: Value) -> ModrinthVersion {
    decode(value, "test version").unwrap()
}

/// Tauri async commands need `Send` futures and shareable state.
#[test]
fn client_and_futures_are_send() {
    fn send<T: Send>(_: &T) {}
    fn send_sync<T: Send + Sync>(_: &T) {}
    let client = client_for("http://127.0.0.1:9/v2");
    send_sync(&client);
    let target = ModrinthResolveTarget::default();
    let installed = ModrinthInstalled::default();
    send(&client.search(&ModrinthSearchQuery::default()));
    send(&client.project_versions("x", &[], &[]));
    send(&client.version_files(&[], ModrinthHashAlgorithm::Sha1));
    send(&client.available_updates(&[], &target));
    send(&client.resolve_dependencies(&[], &target, &installed));
}

#[test]
fn snake_case_keys_become_camel_case() {
    assert_eq!(snake_to_camel("project_id"), "projectId");
    assert_eq!(snake_to_camel("total_hits"), "totalHits");
    assert_eq!(snake_to_camel("sha1"), "sha1");
    assert_eq!(snake_to_camel("_private"), "_private");
    let mut value = json!({"a_b": [{"c_d": 1}], "deadbeef": {"x_y": true}});
    camelize_keys(&mut value);
    assert_eq!(value, json!({"aB": [{"cD": 1}], "deadbeef": {"xY": true}}));
}

#[test]
fn api_json_decodes_with_unknown_enum_values() {
    let mut raw = version_json(
        "v1",
        "proj",
        "release",
        json!([dep(Some("p2"), None, "required")]),
        "abc",
    );
    raw["version_type"] = json!("nightly");
    raw["dependencies"][0]["dependency_type"] = json!("recommended");
    let version = decode_version(raw);
    assert_eq!(version.project_id, "proj");
    assert_eq!(version.version_type, ModrinthVersionType::Unknown);
    assert_eq!(
        version.dependencies[0].dependency_type,
        ModrinthDependencyType::Unknown
    );
    assert_eq!(version.primary_file().unwrap().filename, "v1.jar");
    // The UI shape is camelCase and round-trips.
    let ui = serde_json::to_value(&version).unwrap();
    assert!(ui.get("projectId").is_some());
    assert_eq!(ui["versionType"], "unknown");
    let back: ModrinthVersion = serde_json::from_value(ui).unwrap();
    assert_eq!(back, version);

    let search: ModrinthSearchResults = decode(
        json!({
            "hits": [{
                "project_id": "AANobbMI", "project_type": "mod", "slug": "sodium", "author": "jellysquid3",
                "title": "Sodium", "description": "Fast", "categories": ["optimization"],
                "display_categories": ["optimization"], "versions": ["1.20.1"], "downloads": 1, "follows": 2,
                "icon_url": null, "date_created": "x", "date_modified": "y", "latest_version": "z",
                "license": "LGPL-3.0-only", "client_side": "required", "server_side": "unsupported",
                "gallery": [], "featured_gallery": null, "color": 8703084, "monetization_status": "monetized"
            }],
            "offset": 0, "limit": 10, "total_hits": 1
        }),
        "search",
    )
    .unwrap();
    assert_eq!(search.hits[0].server_side, ModrinthSideSupport::Unsupported);
    assert_eq!(search.total_hits, 1);
}

#[test]
fn search_facets() {
    let query = ModrinthSearchQuery {
        query: Some("sodium".into()),
        project_type: Some(ModrinthProjectType::Mod),
        loaders: vec!["fabric".into(), "quilt".into()],
        game_versions: vec!["1.20.1".into()],
        categories: vec!["optimization".into(), " ".into()],
        ..ModrinthSearchQuery::default()
    };
    let facets: Value = serde_json::from_str(&query.facets().unwrap()).unwrap();
    assert_eq!(
        facets,
        json!([
            ["project_type:mod"],
            ["categories:fabric", "categories:quilt"],
            ["versions:1.20.1"],
            ["categories:optimization"]
        ])
    );
    assert_eq!(ModrinthSearchQuery::default().facets(), None);
}

#[test]
fn rate_limit_header_parsing() {
    let mut headers = HeaderMap::new();
    assert_eq!(rate_limit_wait(&headers), Duration::from_secs(2));
    headers.insert("x-ratelimit-reset", "17".parse().unwrap());
    assert_eq!(rate_limit_wait(&headers), Duration::from_secs(17));
    headers.insert("x-ratelimit-reset", "0".parse().unwrap());
    assert_eq!(rate_limit_wait(&headers), Duration::from_secs(1));
    headers.insert("x-ratelimit-reset", "9999".parse().unwrap());
    assert_eq!(rate_limit_wait(&headers), Duration::from_secs(60));
}

#[test]
fn targets_and_version_choice() {
    let quilt = ModrinthResolveTarget::for_loader(LoaderKind::Quilt, "1.20.1");
    assert_eq!(quilt.loaders, vec!["quilt", "fabric"]);
    assert_eq!(
        compatible_loaders(LoaderKind::NeoForge, "1.20.1"),
        vec!["neoforge", "forge"]
    );
    assert_eq!(
        compatible_loaders(LoaderKind::NeoForge, "1.21.1"),
        vec!["neoforge"]
    );
    assert!(compatible_loaders(LoaderKind::Vanilla, "1.21").is_empty());

    let beta = decode_version(version_json("beta", "p", "beta", json!([]), "1"));
    let release = decode_version(version_json("rel", "p", "release", json!([]), "2"));
    let mut wrong_game = decode_version(version_json("old", "p", "release", json!([]), "3"));
    wrong_game.game_versions = vec!["1.19.2".into()];
    let versions = vec![wrong_game, beta.clone(), release.clone()];
    assert_eq!(pick_best(&versions, &quilt).unwrap().id, "rel");
    assert_eq!(pick_best(&versions[..2], &quilt).unwrap().id, "beta");
    let forge = ModrinthResolveTarget::for_loader(LoaderKind::Forge, "1.20.1");
    assert!(pick_best(&versions, &forge).is_none());
}

#[tokio::test]
async fn resolver_handles_pins_cycles_installed_and_external() {
    let root_deps = json!([
        dep(Some("a"), None, "required"),
        dep(Some("b"), Some("b1"), "required"),
        dep(Some("c"), None, "required"),
        {"project_id": null, "version_id": null, "file_name": "external.jar", "dependency_type": "required"},
        dep(Some("d"), None, "incompatible"),
        dep(Some("opt"), None, "optional"),
    ]);
    let root = decode_version(version_json("r1", "root", "release", root_deps, "root-sha"));

    let requests = Arc::new(parking_lot::Mutex::new(Vec::<String>::new()));
    let log = Arc::clone(&requests);
    let handler: Handler = Arc::new(move |_method, url| {
        log.lock().push(url.path().to_owned());
        let ok = |v: Value| (200, Vec::new(), v.to_string());
        match url.path() {
            "/v2/project/a/version" => {
                let query: String = url.query().unwrap_or_default().to_owned();
                assert!(query.contains("fabric"), "loader filter missing: {query}");
                ok(json!([
                    version_json("a-beta", "a", "beta", json!([]), "a-beta-sha"),
                    version_json(
                        "a-rel",
                        "a",
                        "release",
                        json!([
                            dep(Some("root"), None, "required"),
                            dep(Some("b"), None, "required"),
                            dep(Some("x"), None, "required"),
                        ]),
                        "a-rel-sha"
                    ),
                ]))
            }
            "/v2/versions" => ok(json!([version_json(
                "b1",
                "b",
                "release",
                json!([
                    dep(Some("a"), None, "required"),
                    dep(Some("h"), None, "required"),
                ]),
                "b1-sha"
            )])),
            "/v2/project/h/version" => ok(json!([version_json(
                "h1",
                "h",
                "release",
                json!([]),
                "installed-sha"
            )])),
            "/v2/project/x/version" => ok(json!([])),
            "/v2/projects" => ok(json!([
                {"id": "a", "slug": "alpha", "project_type": "mod", "team": "t", "title": "Alpha",
                 "description": "", "body": "", "published": "", "updated": "", "status": "approved",
                 "license": {"id": "MIT", "name": "MIT", "url": null}, "client_side": "required",
                 "server_side": "optional", "downloads": 1, "followers": 1, "categories": [],
                 "game_versions": [], "loaders": [], "versions": [], "icon_url": "https://icon",
                 "issues_url": null, "source_url": null, "wiki_url": null, "discord_url": null,
                 "donation_urls": [], "gallery": [], "color": null}
            ])),
            other => (
                404,
                Vec::new(),
                json!({"error": "not_found", "path": other}).to_string(),
            ),
        }
    });
    let client = client_for(&mock_api(handler).await);
    let installed = ModrinthInstalled {
        project_ids: HashSet::from(["c".to_owned(), "d".to_owned()]),
        hashes: HashSet::from(["installed-sha".to_owned()]),
    };
    let target = ModrinthResolveTarget::for_loader(LoaderKind::Fabric, "1.20.1");
    let result = client
        .resolve_dependencies(&[root], &target, &installed)
        .await
        .unwrap();

    let mut chosen: Vec<(&str, &str)> = result
        .to_install
        .iter()
        .map(|d| (d.project_id.as_str(), d.version.id.as_str()))
        .collect();
    chosen.sort();
    assert_eq!(chosen, vec![("a", "a-rel"), ("b", "b1")]);
    let alpha = result
        .to_install
        .iter()
        .find(|d| d.project_id == "a")
        .unwrap();
    assert_eq!(alpha.title.as_deref(), Some("Alpha"));
    assert_eq!(alpha.icon_url.as_deref(), Some("https://icon"));
    assert_eq!(alpha.required_by, "root");

    let mut installed_ids = result.already_installed.clone();
    installed_ids.sort();
    assert_eq!(installed_ids, vec!["c", "h"]);

    assert_eq!(result.unresolved.len(), 2);
    assert!(
        result
            .unresolved
            .iter()
            .any(|u| u.reason == ModrinthUnresolvedReason::External
                && u.file_name.as_deref() == Some("external.jar"))
    );
    assert!(result.unresolved.iter().any(|u| u.reason
        == ModrinthUnresolvedReason::NoCompatibleVersion
        && u.project_id.as_deref() == Some("x")
        && u.required_by == "a"));
    assert_eq!(result.conflicts.len(), 1);
    assert_eq!(result.conflicts[0].project_id, "d");

    // Each project was looked up once; the cycle back to root and the diamond on b stopped.
    let log = requests.lock();
    assert_eq!(
        log.iter().filter(|p| *p == "/v2/project/a/version").count(),
        1
    );
    assert_eq!(log.iter().filter(|p| *p == "/v2/versions").count(), 1);
    assert!(!log.iter().any(|p| p.contains("/project/root")));
}

#[tokio::test]
async fn rate_limit_is_retried_after_reset_and_404_is_not_found() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    let handler: Handler = Arc::new(move |_method, url| {
        if url.path() == "/v2/project/missing" {
            return (404, Vec::new(), "{}".into());
        }
        if url.path().starts_with("/v2/version_file/") {
            return (404, Vec::new(), "{}".into());
        }
        if counter.fetch_add(1, Ordering::SeqCst) == 0 {
            return (429, vec![("X-Ratelimit-Reset", "1".into())], "{}".into());
        }
        (
            200,
            Vec::new(),
            json!({"hits": [], "offset": 0, "limit": 10, "total_hits": 0}).to_string(),
        )
    });
    let client = client_for(&mock_api(handler).await);
    let started = Instant::now();
    let results = client
        .search(&ModrinthSearchQuery::default())
        .await
        .unwrap();
    assert!(results.hits.is_empty());
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(started.elapsed() >= Duration::from_millis(900));

    assert!(matches!(
        client.project("missing").await,
        Err(Error::NotFound(_))
    ));
    let none = client
        .version_from_hash("deadbeef", ModrinthHashAlgorithm::Sha1)
        .await
        .unwrap();
    assert!(none.is_none());
    assert!(client.project("..").await.is_err());
}

// --- Live tests. Run with: cargo test -p pl-mc -- --ignored modrinth_live ------------------

fn live_client() -> ModrinthClient {
    ModrinthClient::new(
        Http::new(HttpConfig::new(
            "ProjectsLauncher-tests/2.0 (+https://projects.gg)",
        ))
        .unwrap(),
    )
}

const FABRIC_API: &str = "P7dR8mSH";
const ARCHITECTURY: &str = "lhGA9TYQ";
const CLOTH_CONFIG: &str = "9s6osm5g";

#[tokio::test]
#[ignore = "needs the internet"]
async fn modrinth_live_search_sodium() {
    let client = live_client();
    let results = client
        .search(&ModrinthSearchQuery {
            query: Some("sodium".into()),
            project_type: Some(ModrinthProjectType::Mod),
            loaders: vec!["fabric".into()],
            ..ModrinthSearchQuery::default()
        })
        .await
        .unwrap();
    println!(
        "search sodium: {} hits, first: {:?}",
        results.total_hits,
        results
            .hits
            .iter()
            .take(3)
            .map(|h| &h.title)
            .collect::<Vec<_>>()
    );
    assert!(
        results
            .hits
            .iter()
            .any(|h| h.slug.as_deref() == Some("sodium"))
    );
    let categories = client.categories().await.unwrap();
    assert!(categories.iter().any(|c| c.name == "optimization"));
}

#[tokio::test]
#[ignore = "needs the internet"]
async fn modrinth_live_dependency_resolution() {
    // Roughly Enough Items (Fabric, 1.20.1) requires Fabric API, Architectury and Cloth Config;
    // Architectury requires Fabric API again (a diamond that must not produce duplicates).
    let client = live_client();
    let target = ModrinthResolveTarget::for_loader(LoaderKind::Fabric, "1.20.1");
    let versions = client
        .project_versions("rei", &target.loaders, &target.game_versions)
        .await
        .unwrap();
    let root = pick_best(&versions, &target).unwrap().clone();
    let result = client
        .resolve_dependencies(
            std::slice::from_ref(&root),
            &target,
            &ModrinthInstalled::default(),
        )
        .await
        .unwrap();
    println!(
        "rei {} needs: {:?}; unresolved: {:?}",
        root.version_number,
        result
            .to_install
            .iter()
            .map(|d| (d.title.clone(), d.version.version_number.clone()))
            .collect::<Vec<_>>(),
        result.unresolved
    );
    for required in [FABRIC_API, ARCHITECTURY, CLOTH_CONFIG] {
        assert!(
            result.to_install.iter().any(|d| d.project_id == required),
            "{required} missing"
        );
    }
    let mut ids: Vec<&str> = result
        .to_install
        .iter()
        .map(|d| d.project_id.as_str())
        .collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), result.to_install.len(), "duplicates in {ids:?}");
    assert!(result.to_install.iter().all(|d| target.accepts(&d.version)));
    assert!(result.to_install.iter().all(|d| d.title.is_some()));

    // With Fabric API installed it is skipped everywhere in the graph.
    let installed = ModrinthInstalled {
        project_ids: HashSet::from([FABRIC_API.to_owned()]),
        hashes: HashSet::new(),
    };
    let again = client
        .resolve_dependencies(&[root], &target, &installed)
        .await
        .unwrap();
    assert!(!again.to_install.iter().any(|d| d.project_id == FABRIC_API));
    assert!(again.already_installed.iter().any(|p| p == FABRIC_API));
    assert!(
        again
            .to_install
            .iter()
            .any(|d| d.project_id == ARCHITECTURY)
    );
}

#[tokio::test]
#[ignore = "needs the internet"]
async fn modrinth_live_hash_lookup_and_updates() {
    let client = live_client();
    let target = ModrinthResolveTarget::for_loader(LoaderKind::Fabric, "1.20.1");
    let versions = client
        .project_versions("sodium", &target.loaders, &target.game_versions)
        .await
        .unwrap();
    let oldest = versions.last().unwrap();
    let sha1 = oldest.primary_file().unwrap().hashes.sha1.clone().unwrap();
    let found = client
        .version_files(std::slice::from_ref(&sha1), ModrinthHashAlgorithm::Sha1)
        .await
        .unwrap();
    assert_eq!(
        found.get(&sha1).map(|v| v.id.as_str()),
        Some(oldest.id.as_str())
    );
    let single = client
        .version_from_hash(&sha1, ModrinthHashAlgorithm::Sha1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(single.id, oldest.id);

    let updates = client
        .available_updates(std::slice::from_ref(&sha1), &target)
        .await
        .unwrap();
    println!(
        "sodium {} ({sha1}) → {:?}",
        oldest.version_number,
        updates
            .iter()
            .map(|u| &u.latest.version_number)
            .collect::<Vec<_>>()
    );
    assert_eq!(updates.len(), 1);
    assert_ne!(updates[0].latest.id, oldest.id);
}

/// Downloads three real jars (Fabric, Forge, NeoForge) and parses them.
/// Files go to `$PL_MC_TEST_DIR` or `D:/ProLauncher-build/mc-periphery`.
#[tokio::test]
#[ignore = "needs the internet and downloads a few MB"]
async fn modrinth_live_real_jars_parse() {
    let root = std::env::var_os("PL_MC_TEST_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("D:/ProLauncher-build/mc-periphery"));
    let dir = root.join("mods-live");
    std::fs::create_dir_all(&dir).unwrap();
    let http = Http::new(HttpConfig::new(
        "ProjectsLauncher-tests/2.0 (+https://projects.gg)",
    ))
    .unwrap();
    let client = ModrinthClient::new(http.clone());
    let downloader = pl_net::Downloader::new(http);
    let cases = [
        ("modmenu", LoaderKind::Fabric, "1.20.1"),
        ("jei", LoaderKind::Forge, "1.20.1"),
        ("jei", LoaderKind::NeoForge, "1.21.1"),
    ];
    for (slug, loader, game) in cases {
        let target = ModrinthResolveTarget::for_loader(loader, game);
        let target = ModrinthResolveTarget {
            loaders: target.loaders[..1].to_vec(),
            ..target
        };
        let versions = client
            .project_versions(slug, &target.loaders, &target.game_versions)
            .await
            .unwrap();
        let version = pick_best(&versions, &target).unwrap();
        let file = version.primary_file().unwrap();
        let dest = dir.join(&file.filename);
        let item = pl_net::DownloadItem::new(&file.url, &dest)
            .sha1(file.hashes.sha1.clone().unwrap())
            .size(Some(file.size));
        downloader
            .download(item, &pl_net::DownloadOptions::default())
            .await
            .unwrap();
        let info = crate::mods::read_mod(&dest, true).await.unwrap();
        let meta = info.metadata.clone().unwrap();
        println!(
            "{}: id={:?} name={:?} version={:?} loaders={:?} authors={:?} icon={} bytes depends={:?}",
            file.filename,
            meta.id,
            meta.name,
            meta.version,
            meta.loaders,
            meta.authors,
            meta.icon_png.as_ref().map_or(0, Vec::len),
            meta.depends
        );
        assert!(meta.id.is_some() && meta.name.is_some());
        assert!(meta.version.as_deref().is_some_and(|v| !v.contains("${")));
        assert!(
            meta.loaders.contains(&loader),
            "{:?} should include {loader:?}",
            meta.loaders
        );
        assert_eq!(info.sha1.as_deref(), file.hashes.sha1.as_deref());
    }
}

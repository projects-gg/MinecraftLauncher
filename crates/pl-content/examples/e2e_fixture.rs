//! Builds a signed content tree for local end-to-end tests of the launcher. Never publish it.
//!
//! `cargo run -p pl-content --example e2e_fixture -- <out-dir> [1|2]`
//!
//! Writes `<out-dir>/v1/...` signed with a throwaway key stored in `<out-dir>/test-signing.key`
//! (created on the first run, reused afterwards). The pack family `projects` has two members,
//! `projects` on Minecraft 1.21.1 (what new installs get) and `projects-1.21.4`. Both are Fabric
//! packs with a mod center catalog (items) whose files come from Modrinth (real URLs and hashes)
//! plus a small config file the test server itself serves
//! (`http://127.0.0.1:8787/v1/files/<sha1>`). 1.21.4 offers one mod 1.21.1 does not, so a first
//! visit shows a "Yeni" mod. Release 2 adds Lithium to 1.21.1 (new for existing players),
//! updates Entity Culling and changes the changelog, so the update path can be exercised.
//!
//! Serve `<out-dir>` on port 8787 (`node D:/ProLauncher-build/shots/static-server.mjs <out-dir>
//! 8787`) and start a debug launcher with `PROJECTS_CONTENT_MIRRORS=http://127.0.0.1:8787/v1/`
//! and `PROJECTS_CONTENT_TEST_KEY=<printed public key>`. To serve it on another port, pass
//! `--files-url http://127.0.0.1:<port>/v1/files` so the config file's download points there too.
//!
//! `--feed-key <base64 public key>` also enables the Discord bot's news feed in the config with
//! that key (`feed_fixture` prints one; `--feed-mirror` sets the listed mirror, which a debug
//! launcher replaces with `PROJECTS_FEED_MIRRORS`). The config always has one Studio event two
//! days ahead, so the event card can show the merge of both sources.
//!
//! `--overrides <feed.json>` adds what Studio's news and home management publishes (R2): a pinned
//! Studio copy of the feed's newest patch notes (second in the pinned order), a hidden entry for
//! its community item, a hidden copy of its first event, two banners, a hero button with a link,
//! and two Studio events that overlap now (a week-long one with priority -1 and an hour-long one),
//! so the event card shows the short one with "+N etkinlik".
//!
//! `--join-event` adds a Studio event on the server (`join`) that runs now, so the event card offers
//! "Katıl".
//!
//! `--afk <zip>` publishes an AFK client build for this computer's platform (the zip as the
//! client's CI makes it: `Projects-Konsol-Hesap(.exe)` at its root) as version `--afk-version`
//! (default `9.9.9-e2e`), served from `<out-dir>/v1/afk/<version>/<platform>.zip`. A run without it
//! keeps the AFK entry of the previous index; `--no-afk` drops it. A fresh `<out-dir>` has none, so
//! "the index predates the client's first publish" is: run once, start the launcher, run again
//! with `--afk`, connect an account.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use pl_content::{IndexBuilder, LauncherConfig, NewsFeed, PackManifest, RootIndex};
use pl_net::HashAlgo;
use serde_json::{Value, json};

const FABRIC_LOADER: &str = "0.19.5";
/// Where the test server serves `<out-dir>/v1/files/` (`--files-url` replaces it).
const LOCAL_FILES: &str = "http://127.0.0.1:8787/v1/files";
static FILES_URL: std::sync::OnceLock<String> = std::sync::OnceLock::new();
const FAMILY: &str = "projects";

/// A mod of the catalog.
struct CatalogMod {
    slug: &'static str,
    title: &'static str,
    summary: &'static str,
    category: &'static str,
    default_enabled: bool,
    library: bool,
    /// Slugs of the mods it needs.
    requires: &'static [&'static str],
}

const FABRIC_API: CatalogMod = CatalogMod {
    slug: "fabric-api",
    title: "Fabric API",
    summary: "Diğer modların kullandığı ortak kitaplık.",
    category: "library",
    default_enabled: false,
    library: true,
    requires: &[],
};
const SODIUM: CatalogMod = CatalogMod {
    slug: "sodium",
    title: "Sodium",
    summary: "Daha yüksek FPS, daha akıcı oyun.",
    category: "performance",
    default_enabled: true,
    library: false,
    requires: &[],
};
const ENTITY_CULLING: CatalogMod = CatalogMod {
    slug: "entityculling",
    title: "Entity Culling",
    summary: "Görünmeyen varlıkları çizmez, kalabalık yerlerde FPS artar.",
    category: "performance",
    default_enabled: true,
    library: false,
    requires: &["fabric-api"],
};
const IRIS: CatalogMod = CatalogMod {
    slug: "iris",
    title: "Iris",
    summary: "Shader paketlerini çalıştırır; güçlü bir ekran kartı ister.",
    category: "visual",
    default_enabled: false,
    library: false,
    requires: &["sodium"],
};
const FERRITE_CORE: CatalogMod = CatalogMod {
    slug: "ferrite-core",
    title: "FerriteCore",
    summary: "Bellek kullanımını azaltır.",
    category: "performance",
    default_enabled: false,
    library: false,
    requires: &[],
};
const LITHIUM: CatalogMod = CatalogMod {
    slug: "lithium",
    title: "Lithium",
    summary: "Oyun mantığını hızlandırır.",
    category: "performance",
    default_enabled: true,
    library: false,
    requires: &[],
};
const IMMEDIATELY_FAST: CatalogMod = CatalogMod {
    slug: "immediatelyfast",
    title: "ImmediatelyFast",
    summary: "Arayüzü ve yazıları daha hızlı çizer.",
    category: "interface",
    default_enabled: true,
    library: false,
    requires: &[],
};

/// The Fabric builds of a Modrinth project for `minecraft`, newest first.
async fn modrinth_versions(http: &pl_net::Http, slug: &str, minecraft: &str) -> Vec<Value> {
    let url = format!(
        "https://api.modrinth.com/v2/project/{slug}/version?loaders=%5B%22fabric%22%5D&game_versions=%5B%22{minecraft}%22%5D"
    );
    let versions: Vec<Value> = http.get_json(&url).await.expect("Modrinth versions");
    assert!(
        !versions.is_empty(),
        "{slug} has no Fabric build for {minecraft}"
    );
    versions
}

/// A pack file entry for a Modrinth version, the version's project id and its version id.
fn file_entry(version: &Value) -> (Value, String, String) {
    let files = version["files"].as_array().expect("files");
    let file = files
        .iter()
        .find(|f| f["primary"].as_bool() == Some(true))
        .or_else(|| files.first())
        .expect("a file");
    let entry = json!({
        "path": format!("mods/{}", file["filename"].as_str().expect("filename")),
        "hashes": { "sha1": file["hashes"]["sha1"], "sha512": file["hashes"]["sha512"] },
        "env": { "client": "required", "server": "optional" },
        "downloads": [file["url"]],
        "fileSize": file["size"],
    });
    let project = version["project_id"]
        .as_str()
        .expect("project id")
        .to_owned();
    let id = version["id"].as_str().expect("version id").to_owned();
    (entry, project, id)
}

/// A config file the test server serves from `<v1>/files/<sha1>`.
fn config_file(v1: &Path, minecraft: &str, release: u32) -> Value {
    let bytes = format!(
        "{{\n  \"set\": \"{minecraft}\",\n  \"release\": {release},\n  \"note\": \"Projects uçtan uca test ayarı\"\n}}\n"
    )
    .into_bytes();
    let sha1 = pl_net::hash_bytes(&bytes, HashAlgo::Sha1);
    let sha512 = pl_net::hash_bytes(&bytes, HashAlgo::Sha512);
    write(v1, &format!("files/{sha1}"), &bytes);
    let files_url = FILES_URL.get().map_or(LOCAL_FILES, String::as_str);
    json!({
        "path": "config/projects-e2e.json",
        "hashes": { "sha1": sha1, "sha512": sha512 },
        "downloads": [format!("{files_url}/{sha1}")],
        "fileSize": bytes.len(),
    })
}

fn write(base: &Path, relative: &str, bytes: &[u8]) {
    let path = base.join(relative);
    std::fs::create_dir_all(path.parent().expect("parent")).expect("create folder");
    std::fs::write(&path, bytes).expect("write file");
}

/// One family member: its manifest built from the catalog. `older` lists slugs that use their
/// second newest build (so a later release has an update for them).
#[allow(clippy::too_many_arguments)]
async fn member(
    http: &pl_net::Http,
    v1: &Path,
    pack_id: &str,
    minecraft: &str,
    release: u32,
    catalog: &[&CatalogMod],
    older: &[&str],
    changelog: &str,
) -> PackManifest {
    let mut files = Vec::new();
    let mut paths: HashMap<&str, (String, String)> = HashMap::new();
    let mut version_ids: HashMap<&str, String> = HashMap::new();
    for entry in catalog {
        let versions = modrinth_versions(http, entry.slug, minecraft).await;
        let pick = if older.contains(&entry.slug) && versions.len() > 1 {
            1
        } else {
            0
        };
        let (file, project, version_id) = file_entry(&versions[pick]);
        let path = file["path"].as_str().expect("path").to_owned();
        paths.insert(entry.slug, (project, path));
        version_ids.insert(entry.slug, version_id);
        files.push(file);
    }
    files.push(config_file(v1, minecraft, release));

    let ids: Vec<&str> = paths
        .values()
        .map(|(project, _)| project.as_str())
        .collect();
    let url = format!(
        "https://api.modrinth.com/v2/projects?ids={}",
        url_encode(&serde_json::to_string(&ids).expect("ids"))
    );
    let projects: Vec<Value> = http.get_json(&url).await.expect("Modrinth projects");
    let icons: HashMap<String, String> = projects
        .iter()
        .filter_map(|p| {
            Some((
                p["id"].as_str()?.to_owned(),
                p["icon_url"].as_str()?.to_owned(),
            ))
        })
        .collect();

    let items: Vec<Value> = catalog
        .iter()
        .map(|entry| {
            let (project, path) = &paths[entry.slug];
            let requires: Vec<&str> = entry
                .requires
                .iter()
                .map(|slug| paths[slug].0.as_str())
                .collect();
            let mut item = json!({
                "id": project,
                "path": path,
                "title": entry.title,
                "summary": entry.summary,
                "category": entry.category,
                "modrinth": { "projectId": project, "versionId": version_ids[entry.slug] },
                "defaultEnabled": entry.default_enabled,
                "requires": requires,
                "library": entry.library,
            });
            if let Some(icon) = icons.get(project).filter(|url| url.starts_with("https://")) {
                item["iconUrl"] = json!(icon);
            }
            item
        })
        .collect();

    serde_json::from_value(json!({
        "formatVersion": 1,
        "game": "minecraft",
        "versionId": format!("1.{}.0", release - 1),
        "name": "Projects",
        "summary": "Uçtan uca test paketi",
        "files": files,
        "dependencies": { "minecraft": minecraft, "fabric-loader": FABRIC_LOADER },
        "projects": {
            "packId": pack_id,
            "family": FAMILY,
            "changelog": changelog,
            "items": items,
            "configPolicy": {
                "default": "overwrite",
                "rules": [{ "glob": "config/**", "policy": "preserveUserChanges" }]
            },
            "servers": [{ "name": "Projects", "address": "play.projects.gg" }],
            "recommendedMemoryMb": 4096
        }
    }))
    .expect("pack manifest")
}

/// Studio's news and home management on top of a feed (see the module docs, `--overrides`).
fn add_overrides(feed_path: &Path, config: &mut LauncherConfig, news: &mut NewsFeed) {
    let bytes = std::fs::read(feed_path).expect("read feed.json");
    let (feed, _) = pl_content::BotFeed::parse(&bytes).expect("feed.json");
    let at = |hours: i64| {
        (jiff::Timestamp::now() + jiff::SignedDuration::from_hours(hours))
            .round(jiff::Unit::Second)
            .expect("round")
            .to_string()
    };
    let record = |fingerprint: String, url: Option<String>| json!({ "fingerprint": fingerprint, "sourceUrl": url });
    let source_url = |item: &pl_content::FeedItem| item.source.as_ref().and_then(|s| s.url.clone());

    // A pinned Studio copy of the newest patch notes, second after the welcome item.
    if let Some(notes) = feed
        .items
        .iter()
        .filter(|i| i.category == pl_content::NewsCategory::PatchNotes)
        .max_by_key(|i| i.published_at)
    {
        let mut copy = notes.to_news_item();
        copy.title = format!("{} (Studio)", notes.title);
        copy.pinned = true;
        copy.pin_order = Some(2);
        copy.feed_override =
            serde_json::from_value(record(notes.fingerprint(), source_url(notes))).expect("record");
        news.items.push(copy);
    }
    // The community item hidden (what "Gizle" saves).
    if let Some(item) = feed
        .items
        .iter()
        .find(|i| i.category == pl_content::NewsCategory::Community)
    {
        news.items.push(
            serde_json::from_value(json!({
                "id": item.id,
                "title": item.title,
                "category": "community",
                "publishedAt": item.published_at,
                "expiresAt": item.published_at,
                "hidden": true,
                "feedOverride": record(item.fingerprint(), source_url(item)),
            }))
            .expect("hidden item"),
        );
    }
    // The feed's first event hidden; a week-long event Studio lowered and an hour-long one that
    // overlays it now.
    if let Some(event) = feed.events.first() {
        let end = event.end();
        config.events.push(
            serde_json::from_value(json!({
                "id": event.id,
                "title": event.title,
                "startsAt": event.starts_at,
                "endsAt": end,
                "hidden": true,
                "feedOverride": { "fingerprint": event.fingerprint() },
            }))
            .expect("hidden event"),
        );
    }
    for event in [
        json!({ "id": "e2e-hafta", "title": "Yeni oyuncu haftası", "startsAt": at(-26), "endsAt": at(24 * 5), "priority": -1 }),
        json!({ "id": "e2e-kisa", "title": "Kısa turnuva", "startsAt": at(-1), "endsAt": at(1), "newsId": "e2e-hosgeldin" }),
    ] {
        config
            .events
            .push(serde_json::from_value(event).expect("event"));
    }
    config.banners = serde_json::from_value(json!([
        { "id": "e2e-xp", "level": "event", "text": "Hafta sonu boyunca iki kat XP.", "url": "https://projects.gg", "dismissible": true },
        { "id": "e2e-bakim", "level": "warning", "text": "Bu gece 03.00'te kısa bakım var.", "dismissible": false }
    ]))
    .expect("banners");
    config.hero.cta =
        serde_json::from_value(json!({ "label": "Sezonu keşfet", "url": "https://projects.gg" }))
            .expect("hero button");
}

fn url_encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let Some(out) = args.next().map(PathBuf::from) else {
        eprintln!(
            "usage: e2e_fixture <out-dir> [1|2] [--feed-key <public key> [--feed-mirror <url>]] \
             [--overrides <feed.json>] [--files-url <url>] [--join-event] \
             [--afk <client zip> [--afk-version <version>] | --no-afk]"
        );
        std::process::exit(2);
    };
    let release: u32 = args.next().map_or(1, |v| v.parse().expect("1 or 2"));
    // Optional news feed (`feed_fixture` prints its throwaway key). The launcher reads it from
    // `PROJECTS_FEED_MIRRORS` in debug builds; the mirror here only has to be a valid one.
    let mut feed_key = None;
    let mut feed_mirror = "https://projects.gg/launcher-feed/v1/".to_owned();
    let mut overrides: Option<PathBuf> = None;
    // A Studio event on the server that runs now (`join`): the event card offers "Katıl".
    let mut join_event = false;
    // An AFK client build to publish, or the previous index's to drop.
    let mut afk_zip: Option<PathBuf> = None;
    let mut afk_version = "9.9.9-e2e".to_owned();
    let mut no_afk = false;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--feed-key" => feed_key = args.next(),
            "--feed-mirror" => feed_mirror = args.next().expect("--feed-mirror <url>"),
            "--overrides" => overrides = args.next().map(PathBuf::from),
            "--join-event" => join_event = true,
            "--afk" => afk_zip = Some(PathBuf::from(args.next().expect("--afk <zip>"))),
            "--afk-version" => afk_version = args.next().expect("--afk-version <version>"),
            "--no-afk" => no_afk = true,
            "--files-url" => {
                let url = args.next().expect("--files-url <url>");
                FILES_URL
                    .set(url.trim_end_matches('/').to_owned())
                    .expect("--files-url once");
            }
            other => panic!("unknown argument {other}"),
        }
    }
    let v1 = out.join("v1");
    std::fs::create_dir_all(&v1).expect("create output");

    let key_path = out.join("test-signing.key");
    let signing = match std::fs::read_to_string(&key_path) {
        Ok(text) => pl_content::decode_signing_key(text.trim()).expect("test key"),
        Err(_) => {
            let key = pl_content::generate_signing_key().expect("OS random source");
            std::fs::write(&key_path, pl_content::encode_signing_key(&key)).expect("save test key");
            key
        }
    };

    let http = pl_net::Http::new(pl_net::HttpConfig::new(
        "ProjectsLauncher-e2e-fixture (+https://projects.gg)",
    ))
    .expect("http client");

    let mut catalog_1211: Vec<&CatalogMod> =
        vec![&FABRIC_API, &SODIUM, &ENTITY_CULLING, &IRIS, &FERRITE_CORE];
    let mut changelog =
        "- Fabric API, Sodium ve Entity Culling ile ilk sürüm\n- Iris ve FerriteCore isteğe bağlı"
            .to_owned();
    // Release 1 ships an older Entity Culling so release 2 brings an update for it.
    let mut older = vec![ENTITY_CULLING.slug];
    if release >= 2 {
        catalog_1211.push(&LITHIUM);
        older.clear();
        changelog = "- Lithium önerilenlere eklendi\n- Entity Culling güncellendi".to_owned();
    }
    let v1211 = member(
        &http,
        &v1,
        "projects",
        "1.21.1",
        release,
        &catalog_1211,
        &older,
        &changelog,
    )
    .await;
    let v1214 = member(
        &http,
        &v1,
        "projects-1.21.4",
        "1.21.4",
        release,
        &[
            &FABRIC_API,
            &SODIUM,
            &ENTITY_CULLING,
            &IRIS,
            &FERRITE_CORE,
            &LITHIUM,
            &IMMEDIATELY_FAST,
        ],
        &[],
        "- 1.21.4 için Projects paketi",
    )
    .await;

    let mut config: LauncherConfig = serde_json::from_value(json!({
        "hero": {
            "title": "Test sezonu",
            "subtitle": "Bu içerik yerel test sunucusundan geliyor."
        },
        "recommendedPack": "projects",
        "servers": [{
            "id": "projects",
            "name": "Projects",
            "address": "play.projects.gg",
            "featured": true
        }],
        "events": [{
            "id": "e2e-turnuva",
            "title": "Studio turnuvası",
            "startsAt": (jiff::Timestamp::now() + jiff::SignedDuration::from_hours(50))
                .round(jiff::Unit::Second)
                .expect("round")
                .to_string(),
            "endsAt": (jiff::Timestamp::now() + jiff::SignedDuration::from_hours(53))
                .round(jiff::Unit::Second)
                .expect("round")
                .to_string()
        }]
    }))
    .expect("launcher config");
    if join_event {
        let at = |minutes: i64| {
            (jiff::Timestamp::now() + jiff::SignedDuration::from_mins(minutes))
                .round(jiff::Unit::Second)
                .expect("round")
                .to_string()
        };
        config.events.push(
            serde_json::from_value(json!({
                "id": "e2e-sunucu",
                "title": "Sunucuda çifte XP",
                "startsAt": at(-10),
                "endsAt": at(150),
                "join": true
            }))
            .expect("join event"),
        );
    }
    // Release 1 leaves the recommended Minecraft version to the launcher's built-in default;
    // release 2 names one, so a refresh moves the "Önerilen" badge.
    if release >= 2 {
        config.recommended_minecraft_version = Some("1.21.4".into());
    }
    if let Some(public_key) = &feed_key {
        let key = pl_content::FeedKey::from_public_key(public_key).expect("feed public key");
        config.news_feed = Some(pl_content::NewsFeedConfig {
            enabled: true,
            mirrors: vec![feed_mirror.clone()],
            keys: vec![key],
            max_age_days: 60,
        });
        let problems = config.news_feed.as_ref().expect("feed").problems();
        assert!(problems.is_empty(), "news feed config: {problems:?}");
    }

    let mut news: NewsFeed = serde_json::from_value(json!({
        "items": [
            {
                "id": "e2e-hosgeldin",
                "title": "Uçtan uca test haberi",
                "summary": "İmzalı içerik yerel aynadan doğrulanarak geldi.",
                "body": "Bu haber **e2e_fixture** örneğiyle üretildi.\n\n- İmza doğrulandı\n- Paket hazır",
                "category": "announcement",
                "publishedAt": "2026-10-15T12:00:00Z",
                "pinned": true,
                "pinOrder": 1
            },
            {
                "id": "e2e-yama",
                "title": format!("Projects paketi 1.{}.0", release - 1),
                "summary": "Paket notları ana ekranda görünür.",
                "body": changelog,
                "category": "patchNotes",
                "publishedAt": "2026-10-15T13:00:00Z"
            }
        ]
    }))
    .expect("news feed");
    if let Some(path) = &overrides {
        add_overrides(path, &mut config, &mut news);
    }

    let mut builder = match std::fs::read(v1.join("index.json")) {
        Ok(bytes) => {
            let previous: RootIndex = serde_json::from_slice(&bytes).expect("previous index");
            IndexBuilder::from_previous(&previous)
        }
        Err(_) => IndexBuilder::new(),
    };
    builder.set_config(&config).expect("config");
    builder.set_news(&news).expect("news");
    builder.set_pack("Projects", &v1211).expect("1.21.1 pack");
    builder
        .set_pack("Projects (1.21.4)", &v1214)
        .expect("1.21.4 pack");
    builder
        .set_family_default("projects")
        .expect("default member");
    if no_afk {
        builder.set_afk(None);
    }
    if let Some(zip) = &afk_zip {
        // Immutable like the real builds: a new build gets a new version.
        let bytes = std::fs::read(zip).expect("AFK client zip");
        let platform = pl_content::current_platform_key();
        let path = pl_content::studio::afk_build_path(&afk_version, &platform);
        write(&v1, &path, &bytes);
        let build = pl_content::FileRef {
            path: Some(path),
            urls: Vec::new(),
            sha256: pl_net::hash_bytes(&bytes, HashAlgo::Sha256),
            sha1: None,
            size: bytes.len() as u64,
        };
        builder.set_afk(Some(pl_content::AfkRelease {
            version: afk_version.clone(),
            builds: [(platform, build)].into_iter().collect(),
            minecraft_versions: Vec::new(),
            recommended_minecraft_version: None,
        }));
    }
    let built = builder.build(jiff::Timestamp::now()).expect("index");

    // Immutable documents first, then the signature, then the index (the commit point).
    for doc in &built.documents {
        write(&v1, &doc.path, &doc.bytes);
    }
    let (_, signature) = built.sign(&signing).expect("sign");
    write(&v1, "index.json.sig", &signature);
    write(&v1, "index.json", &built.index_bytes);

    println!("content written to {}", v1.display());
    println!("sequence:   {}", built.index.sequence);
    for manifest in [&v1211, &v1214] {
        println!(
            "pack:       {} {} (Minecraft {}, {} items)",
            manifest.projects.pack_id,
            manifest.version_id,
            manifest.dependencies.minecraft,
            manifest.projects.items.len()
        );
    }
    match &built.index.afk {
        Some(afk) => println!(
            "afk client: {} ({})",
            afk.version,
            afk.builds.keys().cloned().collect::<Vec<_>>().join(", ")
        ),
        None => println!("afk client: none"),
    }
    println!(
        "public key: {}",
        pl_content::encode_verifying_key(&signing.verifying_key())
    );
}

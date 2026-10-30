//! Test helpers: a tiny HTTP/1.1 server (with ETag support and hit counters) and fixtures.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use pl_net::{HashAlgo, Http, HttpConfig};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::model::{
    Banner, BannerLevel, ConfigPolicy, Cta, FilePolicy, ItemCategory, LauncherConfig, MediaRef,
    ModSet, ModSetCatalog, ModSetPurpose, ModSetVariant, ModrinthRef, NewsCategory, NewsFeed,
    NewsItem, PackDependencies, PackFile, PackItem, PackManifest, PolicyRule, ProjectsExtension,
};
use crate::signing::SigningKey;
use crate::studio::{BuiltIndex, IndexBuilder, hosted_file_path, pack_hashes};

#[derive(Clone)]
struct Route {
    body: Vec<u8>,
    etag: String,
}

#[derive(Default)]
struct Shared {
    routes: HashMap<String, Route>,
    hits: HashMap<String, usize>,
}

pub(crate) struct TestServer {
    addr: SocketAddr,
    shared: Arc<Mutex<Shared>>,
    task: tokio::task::JoinHandle<()>,
}

impl TestServer {
    pub(crate) async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let shared = Arc::new(Mutex::new(Shared::default()));
        let state = Arc::clone(&shared);
        let task = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let state = Arc::clone(&state);
                tokio::spawn(async move {
                    let _ = handle(stream, state).await;
                });
            }
        });
        Self { addr, shared, task }
    }

    /// Content base URL (`http://127.0.0.1:<port>/v1/`).
    pub(crate) fn base(&self) -> String {
        format!("http://{}/v1/", self.addr)
    }

    pub(crate) fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.addr)
    }

    /// Serves `body` at `path` (absolute, e.g. `/v1/index.json`) with a content ETag.
    pub(crate) fn put(&self, path: &str, body: Vec<u8>) {
        let etag = format!("\"{}\"", &crate::fsutil::sha256_hex(&body)[..16]);
        self.shared
            .lock()
            .unwrap()
            .routes
            .insert(path.to_owned(), Route { body, etag });
    }

    pub(crate) fn hits(&self, path: &str) -> usize {
        self.shared
            .lock()
            .unwrap()
            .hits
            .get(path)
            .copied()
            .unwrap_or(0)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn handle(mut stream: TcpStream, shared: Arc<Mutex<Shared>>) -> std::io::Result<()> {
    let mut request = Vec::new();
    let mut buf = [0u8; 2048];
    while !request.windows(4).any(|w| w == b"\r\n\r\n") {
        let n = stream.read(&mut buf).await?;
        if n == 0 || request.len() > 64 * 1024 {
            return Ok(());
        }
        request.extend_from_slice(&buf[..n]);
    }
    let text = String::from_utf8_lossy(&request);
    let mut lines = text.split("\r\n");
    let target = lines
        .next()
        .and_then(|line| line.split(' ').nth(1))
        .unwrap_or("/");
    let path = target.split('?').next().unwrap_or(target).to_owned();
    let if_none_match = lines
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.trim().eq_ignore_ascii_case("if-none-match"))
        .map(|(_, value)| value.trim().to_owned());

    let route = {
        let mut shared = shared.lock().unwrap();
        *shared.hits.entry(path.clone()).or_default() += 1;
        shared.routes.get(&path).cloned()
    };
    let response = match route {
        None => {
            b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()
        }
        Some(route) if if_none_match.as_deref() == Some(route.etag.as_str()) => format!(
            "HTTP/1.1 304 Not Modified\r\nETag: {}\r\nConnection: close\r\n\r\n",
            route.etag
        )
        .into_bytes(),
        Some(route) => {
            let mut response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nETag: {}\r\nConnection: close\r\n\r\n",
                route.body.len(),
                route.etag
            )
            .into_bytes();
            response.extend_from_slice(&route.body);
            response
        }
    };
    stream.write_all(&response).await?;
    stream.shutdown().await
}

/// HTTP client without retries so failure paths stay fast.
pub(crate) fn test_http() -> Http {
    Http::new(HttpConfig {
        retries: 0,
        ..HttpConfig::new("pl-content-tests")
    })
    .unwrap()
}

/// A base URL on a port nothing listens on.
pub(crate) async fn dead_base() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    format!("http://127.0.0.1:{port}/v1/")
}

pub(crate) fn sha1_hex(bytes: &[u8]) -> String {
    pl_net::hash_bytes(bytes, HashAlgo::Sha1)
}

pub(crate) fn sample_config(hero: Option<MediaRef>) -> LauncherConfig {
    let mut config = LauncherConfig {
        banners: vec![Banner {
            id: "welcome".into(),
            level: BannerLevel::Event,
            text: "Kış etkinliği başladı!".into(),
            url: None,
            starts_at: None,
            ends_at: None,
            dismissible: true,
        }],
        recommended_pack: Some("projects".into()),
        ..LauncherConfig::default()
    };
    config.hero.media = hero;
    config.hero.title = "Projects".into();
    config.hero.cta = Some(Cta {
        label: "Oyna".into(),
        url: None,
        news_id: Some("patch-1".into()),
    });
    config
}

pub(crate) fn sample_news(cover: Option<MediaRef>) -> NewsFeed {
    NewsFeed {
        schema: 1,
        items: vec![NewsItem {
            id: "patch-1".into(),
            title: "Yama notları".into(),
            summary: "Yenilikler".into(),
            body: "# Değişiklikler\n\n- Daha hızlı açılış".into(),
            cover,
            category: NewsCategory::PatchNotes,
            tags: vec!["güncelleme".into()],
            published_at: "2026-01-01T00:00:00Z".parse().unwrap(),
            expires_at: None,
            pinned: false,
            pin_order: None,
            featured: true,
            hidden: false,
            cta: None,
            author: Some("Projects".into()),
            feed_override: None,
        }],
    }
}

/// A pack manifest whose files are served by `server` at `/files/<sha1>`
/// (`config/**` uses `preserveUserChanges`).
pub(crate) fn sample_manifest(server: &TestServer, files: &[(&str, &[u8])]) -> PackManifest {
    let files = files
        .iter()
        .map(|(path, bytes)| {
            let hashes = pack_hashes(bytes);
            let remote = format!("/{}", hosted_file_path(&hashes.sha1));
            server.put(&remote, bytes.to_vec());
            PackFile {
                path: (*path).to_owned(),
                hashes,
                env: None,
                downloads: vec![server.url(&remote)],
                file_size: bytes.len() as u64,
            }
        })
        .collect();
    PackManifest {
        format_version: 1,
        game: "minecraft".into(),
        version_id: "1.0.0".into(),
        name: "Projects".into(),
        summary: None,
        files,
        dependencies: PackDependencies {
            minecraft: "1.21.1".into(),
            fabric_loader: Some("0.16.9".into()),
            ..PackDependencies::default()
        },
        projects: ProjectsExtension {
            pack_id: "projects".into(),
            config_policy: ConfigPolicy {
                default: FilePolicy::Overwrite,
                rules: vec![PolicyRule {
                    glob: "config/**".into(),
                    policy: FilePolicy::PreserveUserChanges,
                }],
            },
            ..ProjectsExtension::default()
        },
    }
}

/// A set item for `path` (Modrinth project `id`, version `<id>-v1`), on by default.
pub(crate) fn set_item(id: &str, path: &str) -> PackItem {
    PackItem {
        id: id.into(),
        path: path.into(),
        title: id.to_uppercase(),
        summary: None,
        category: ItemCategory::Other,
        icon_url: None,
        page_url: None,
        modrinth: Some(ModrinthRef {
            project_id: id.into(),
            version_id: format!("{id}-v1"),
        }),
        default_enabled: true,
        requires: Vec::new(),
        library: false,
    }
}

/// A Fabric variant for `minecraft` whose files are served by `server` (when given) at
/// `/files/<sha1>`; items are named after the file stem. Without a server the files point at
/// Modrinth's CDN (valid for the catalog rules, never downloaded).
pub(crate) fn sample_variant(
    server: Option<&TestServer>,
    minecraft: &str,
    files: &[(&str, &[u8])],
) -> ModSetVariant {
    let mut variant = ModSetVariant {
        dependencies: PackDependencies {
            minecraft: minecraft.into(),
            fabric_loader: Some("0.19.5".into()),
            ..PackDependencies::default()
        },
        files: Vec::new(),
        items: Vec::new(),
    };
    for (path, bytes) in files {
        let hashes = pack_hashes(bytes);
        let url = match server {
            Some(server) => {
                let remote = format!("/{}", hosted_file_path(&hashes.sha1));
                server.put(&remote, bytes.to_vec());
                server.url(&remote)
            }
            None => format!("https://cdn.modrinth.com/data/x/versions/y/{path}"),
        };
        let stem = path
            .rsplit('/')
            .next()
            .and_then(|name| name.split('.').next())
            .unwrap_or(path);
        variant.files.push(PackFile {
            path: (*path).to_owned(),
            hashes,
            env: None,
            downloads: vec![url],
            file_size: bytes.len() as u64,
        });
        variant.items.push(set_item(stem, path));
    }
    variant
}

/// A catalog with one PvP set holding `variant`.
pub(crate) fn sample_mod_sets(variant: ModSetVariant) -> ModSetCatalog {
    ModSetCatalog {
        schema: 1,
        sets: vec![ModSet {
            id: "pvp".into(),
            name: "PvP".into(),
            summary: Some("Net görüntü ve sabit FPS.".into()),
            purpose: ModSetPurpose::Pvp,
            en: None,
            variants: vec![variant],
            min_launcher_version: None,
        }],
    }
}

/// One publish: documents, signed index, served under `/v1/`.
/// A small item art document and its PNG files (`(path, bytes)`), as Studio publishes them.
pub(crate) fn sample_item_art() -> (crate::ItemArt, Vec<(String, Vec<u8>)>) {
    use crate::fsutil::sha256_hex;
    use crate::studio::{item_art_atlas_path, item_art_glyph_path};
    use crate::{DocRef, GlyphImage, ItemArt, ItemArtAtlas, ItemArtEntry, ItemArtGlyph};
    let file = |path: String, bytes: &[u8]| DocRef {
        path,
        sha256: sha256_hex(bytes),
        size: bytes.len() as u64,
    };
    let atlas16: &[u8] = b"atlas sixteen";
    let atlas32: &[u8] = b"atlas thirty-two";
    let glyph: &[u8] = b"market background";
    let atlas16_ref = file(item_art_atlas_path(16, &sha256_hex(atlas16)), atlas16);
    let atlas32_ref = file(item_art_atlas_path(32, &sha256_hex(atlas32)), atlas32);
    let glyph_ref = file(item_art_glyph_path(&sha256_hex(glyph)), glyph);
    let files = vec![
        (atlas16_ref.path.clone(), atlas16.to_vec()),
        (atlas32_ref.path.clone(), atlas32.to_vec()),
        (glyph_ref.path.clone(), glyph.to_vec()),
    ];
    let art = ItemArt {
        atlases: vec![
            ItemArtAtlas {
                size: 16,
                columns: 32,
                cells: 1,
                file: atlas16_ref,
            },
            ItemArtAtlas {
                size: 32,
                columns: 32,
                cells: 1,
                file: atlas32_ref,
            },
        ],
        items: vec![ItemArtEntry {
            item: Some("minecraft:paper".into()),
            cmd: Some(35),
            model: None,
            icon: 0,
            id: "pro-items-special:eskisayfa1".into(),
            name: Some("Eski Sayfa 1".into()),
            pack: Some("pro-items-special".into()),
        }],
        glyphs: vec![ItemArtGlyph {
            ch: "\u{E052}".into(),
            advance: 257,
            image: Some(GlyphImage {
                file: glyph_ref,
                width: 256,
                height: 256,
                ascent: 36,
                id: "pro-guis:market".into(),
                pack: Some("pro-guis".into()),
            }),
        }],
        ..ItemArt::default()
    };
    (art, files)
}

pub(crate) struct Publish {
    pub(crate) sequence: u64,
    pub(crate) config: LauncherConfig,
    pub(crate) news: NewsFeed,
    pub(crate) pack: Option<PackManifest>,
    pub(crate) mod_sets: Option<ModSetCatalog>,
    pub(crate) item_art: Option<crate::ItemArt>,
    pub(crate) chat_emojis: Option<crate::ChatEmojis>,
    pub(crate) overlay: Option<crate::OverlayRelease>,
}

impl Publish {
    pub(crate) fn new(sequence: u64) -> Self {
        Self {
            sequence,
            config: sample_config(None),
            news: sample_news(None),
            pack: None,
            mod_sets: None,
            item_art: None,
            chat_emojis: None,
            overlay: None,
        }
    }

    pub(crate) fn put(&self, server: &TestServer, key: &SigningKey) -> BuiltIndex {
        let mut builder = IndexBuilder::new();
        builder.set_config(&self.config).unwrap();
        builder.set_news(&self.news).unwrap();
        if let Some(pack) = &self.pack {
            builder.set_pack("Projects", pack).unwrap();
        }
        builder.set_mod_sets(self.mod_sets.as_ref()).unwrap();
        builder.set_item_art(self.item_art.as_ref()).unwrap();
        builder.set_chat_emojis(self.chat_emojis.as_ref()).unwrap();
        builder.set_overlay(self.overlay.clone()).unwrap();
        builder.set_sequence(self.sequence);
        let built = builder
            .build("2026-10-15T12:00:00Z".parse().unwrap())
            .unwrap();
        for doc in &built.documents {
            server.put(&format!("/v1/{}", doc.path), doc.bytes.clone());
        }
        let (_, signature) = built.sign(key).unwrap();
        server.put("/v1/index.json.sig", signature);
        server.put("/v1/index.json", built.index_bytes.clone());
        built
    }
}

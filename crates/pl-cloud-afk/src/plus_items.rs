//! AFK+'s item data (README "AFK+ player API" → Item data; FREE-ITEMS `pl_items::data`): the
//! game's own items of the account's Minecraft version (names in both languages, stack sizes,
//! durability, the enchantments that fit), from our CDN. The manifest names each version's file by
//! its SHA-256; the bytes are checked against it before anything reads them and kept in the
//! launcher's cache folder (`data-<version>-<first 12 hex>.json`, the newest file per version), so
//! a panel opened again, or without a connection, has them at once. The request goes from here, not
//! from the page: the launcher's CSP keeps the CDN out of the page.
//!
//! Nothing of the game is in the repository: the files are built from Mojang's published files by
//! `pl-items` and served from the CDN only.

use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use pl_items::data::{DATA_BASE, DataFile, DataManifest, ItemData, MAX_DATA_BYTES};
use serde::Serialize;
use sha2::{Digest, Sha256};

/// The manifest is read again after this (its own `max-age`).
const MANIFEST_FRESH: Duration = Duration::from_secs(300);
/// A manifest that could not be read is not asked again sooner.
const MANIFEST_FAILED: Duration = Duration::from_secs(30);
const MANIFEST_FILE: &str = "manifest.json";
/// The last good manifest, beside the files (ITEMART's atlases have their own folders there).
const MANIFEST_COPY: &str = "itemdata-manifest.json";

/// One version's item data as the page reads it (`@pl/ui/items` `itemDataFromText`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PlusItemData {
    /// The version whose file this is (the account's own, or the nearest the CDN has).
    pub minecraft: String,
    /// The file's text, its SHA-256 checked.
    pub text: String,
}

/// The item data read this session: the manifest, and each version's parsed data (AFK notices'
/// item names come from here without asking anything). A version's text is kept with the hash it
/// was checked against (the whole SHA-256, or the 12 hex digits of its cached name), so a newer
/// file the manifest names is read instead of it.
#[derive(Default)]
pub(crate) struct ItemDataCache {
    manifest: Mutex<Option<(Instant, Option<DataManifest>)>>,
    data: Mutex<HashMap<String, Arc<ItemData>>>,
    texts: Mutex<HashMap<String, (String, Arc<str>)>>,
}

/// A version as a file name part: only `[0-9A-Za-z._-]`, not starting with a dot.
fn safe_version(version: &str) -> bool {
    !version.is_empty()
        && version.len() <= 32
        && !version.starts_with('.')
        && version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

/// `data-<version>-<first 12 hex of its SHA-256>.json`.
pub fn cache_file_name(version: &str, file: &DataFile) -> Option<String> {
    let hash = file.sha256.get(..12)?;
    safe_version(version).then(|| format!("data-{version}-{hash}.json"))
}

/// A cached file's version and hash (`data-<version>-<12 hex>.json`): read from the end, so
/// `data-26.1-…` is never `data-26.1-rc1-…`'s.
fn cached_name(name: &str) -> Option<(&str, &str)> {
    let (version, hash) = name
        .strip_prefix("data-")?
        .strip_suffix(".json")?
        .rsplit_once('-')?;
    (hash.len() == 12 && hash.bytes().all(|b| b.is_ascii_hexdigit())).then_some((version, hash))
}

/// Writes a file whole or not at all: its own temporary name (two loads at once never write into
/// each other's), then a rename.
fn write_whole(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let mut temp = path.as_os_str().to_owned();
    temp.push(format!(".{}-{n}.part", std::process::id()));
    let temp = PathBuf::from(temp);
    std::fs::write(&temp, bytes)?;
    std::fs::rename(&temp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&temp);
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(DIGITS[usize::from(b >> 4)] as char);
        out.push(DIGITS[usize::from(b & 15)] as char);
    }
    out
}

/// Whether bytes are the manifest's file: its length and SHA-256.
pub fn verified(bytes: &[u8], file: &DataFile) -> bool {
    u64::try_from(bytes.len()).is_ok_and(|n| n == file.bytes) && sha256_hex(bytes) == file.sha256
}

/// The cached file of a version, if it is still the manifest's (checked again: a file changed on
/// disk is not read).
fn read_cached(dir: &Path, version: &str, file: &DataFile) -> Option<Vec<u8>> {
    let name = cache_file_name(version, file)?;
    let bytes = std::fs::read(dir.join(name)).ok()?;
    verified(&bytes, file).then_some(bytes)
}

/// Keeps a version's checked file; older files of that version go.
fn write_cached(dir: &Path, version: &str, file: &DataFile, bytes: &[u8]) -> std::io::Result<()> {
    let Some(name) = cache_file_name(version, file) else {
        return Ok(());
    };
    std::fs::create_dir_all(dir)?;
    write_whole(&dir.join(&name), bytes)?;
    for entry in std::fs::read_dir(dir)?.flatten() {
        let other = entry.file_name().to_string_lossy().into_owned();
        if other != name && cached_name(&other).is_some_and(|(of, _)| of == version) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    Ok(())
}

/// The newest cached file of a version, checked by the hash it was saved under: what is read
/// when the manifest, or the file it names, cannot be fetched (offline). Its hash comes along.
fn any_cached(dir: &Path, version: &str) -> Option<(String, Vec<u8>)> {
    if !safe_version(version) {
        return None;
    }
    let (entry, hash) = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let (of, hash) = cached_name(&name)?;
            (of == version).then(|| (entry, hash.to_owned()))
        })
        .max_by_key(|(entry, _)| entry.metadata().and_then(|m| m.modified()).ok())?;
    let bytes = std::fs::read(entry.path()).ok()?;
    sha256_hex(&bytes)
        .starts_with(&hash)
        .then_some((hash, bytes))
}

impl ItemDataCache {
    /// The manifest: read again after five minutes; the last good copy on disk when the CDN
    /// cannot be reached.
    async fn manifest<F, Fut>(&self, dir: &Path, get: &F) -> Option<DataManifest>
    where
        F: Fn(String) -> Fut,
        Fut: Future<Output = Option<Vec<u8>>>,
    {
        if let Some((at, known)) = self.manifest.lock().as_ref() {
            let fresh = if known.is_some() {
                MANIFEST_FRESH
            } else {
                MANIFEST_FAILED
            };
            if at.elapsed() < fresh {
                return known.clone();
            }
        }
        let fetched = get(format!("{DATA_BASE}{MANIFEST_FILE}"))
            .await
            .and_then(|bytes| DataManifest::parse(&bytes).ok().map(|m| (m, bytes)));
        let manifest = match fetched {
            Some((manifest, bytes)) => {
                let _ = std::fs::create_dir_all(dir)
                    .and_then(|()| write_whole(&dir.join(MANIFEST_COPY), &bytes));
                Some(manifest)
            }
            None => std::fs::read(dir.join(MANIFEST_COPY))
                .ok()
                .and_then(|bytes| DataManifest::parse(&bytes).ok()),
        };
        *self.manifest.lock() = Some((Instant::now(), manifest.clone()));
        manifest
    }

    /// The item data for a Minecraft version (the account's; None: the manifest's default),
    /// checked and cached. None when neither the CDN nor the cache has it: the order form says the
    /// list could not be loaded and offers to try again (a typed check still works: the machine
    /// names the items).
    pub(crate) async fn load<F, Fut>(
        &self,
        dir: &Path,
        minecraft: Option<&str>,
        get: F,
    ) -> Option<PlusItemData>
    where
        F: Fn(String) -> Fut,
        Fut: Future<Output = Option<Vec<u8>>>,
    {
        let Some(manifest) = self.manifest(dir, &get).await else {
            let version = minecraft?.to_owned();
            if let Some(known) = self.known(&version, None) {
                return Some(known);
            }
            let (hash, bytes) = any_cached(dir, &version)?;
            return self.keep(version, hash, bytes);
        };
        let version = manifest.pick(minecraft)?.to_owned();
        let file = manifest.versions.get(&version)?.clone();
        if let Some(known) = self.known(&version, Some(file.sha256.as_str())) {
            return Some(known);
        }
        if file.bytes > MAX_DATA_BYTES as u64 {
            return None;
        }
        let fetched = match read_cached(dir, &version, &file) {
            Some(bytes) => Some(bytes),
            None => {
                let url = format!("{DATA_BASE}{}", file.file);
                match get(url).await {
                    Some(bytes) if verified(&bytes, &file) => {
                        if let Err(error) = write_cached(dir, &version, &file, &bytes) {
                            tracing::debug!(%error, "the AFK+ item data could not be cached");
                        }
                        Some(bytes)
                    }
                    Some(_) => {
                        tracing::warn!(version, "AFK+ item data failed its SHA-256 check");
                        None
                    }
                    None => None,
                }
            }
        };
        match fetched {
            Some(bytes) => self.keep(version, file.sha256.clone(), bytes),
            // The manifest's newer file cannot be fetched now: the older one kept here still names
            // the items (the form works offline).
            None => {
                let (hash, bytes) = any_cached(dir, &version)?;
                self.keep(version, hash, bytes)
            }
        }
    }

    /// The text read this session for a version, when it is the file `sha256` names (any, without
    /// a manifest).
    fn known(&self, version: &str, sha256: Option<&str>) -> Option<PlusItemData> {
        let texts = self.texts.lock();
        let (hash, text) = texts.get(version)?;
        sha256
            .is_none_or(|sha256| sha256.starts_with(hash.as_str()))
            .then(|| PlusItemData {
                minecraft: version.to_owned(),
                text: text.to_string(),
            })
    }

    /// Parses the checked bytes (a file that does not parse is not handed on) and keeps both, with
    /// the hash they were checked against.
    fn keep(&self, version: String, hash: String, bytes: Vec<u8>) -> Option<PlusItemData> {
        let data = ItemData::parse(&bytes).ok()?;
        let text: Arc<str> = String::from_utf8(bytes).ok()?.into();
        self.data.lock().insert(version.clone(), Arc::new(data));
        self.texts
            .lock()
            .insert(version.clone(), (hash, text.clone()));
        Some(PlusItemData {
            minecraft: version,
            text: text.to_string(),
        })
    }

    /// An item's names (Turkish, English) from the item data read this session, any version.
    pub(crate) fn item_names(&self, id: &str) -> Option<(String, String)> {
        let data = self.data.lock();
        let mut versions: Vec<&String> = data.keys().collect();
        versions.sort_by(|a, b| pl_items::data::compare_versions(b, a));
        versions.into_iter().find_map(|version| {
            data[version]
                .item(id)
                .map(|item| (item.name.tr.clone(), item.name.en.clone()))
        })
    }

    /// Forgets what was read (a sign-out keeps the files: they are the game's, not the player's).
    pub(crate) fn clear(&self) {
        *self.manifest.lock() = None;
    }
}

/// Where the item data is cached (`<data>/cache/items`, beside ITEMART's atlases).
pub fn item_data_dir(cache_root: &Path) -> PathBuf {
    cache_root.join("items")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    /// A tiny format-1 file (the vectors' synthetic data has the same shape).
    fn data_file(mc: &str) -> Vec<u8> {
        format!(
            "{{\"format\":1,\"mc\":\"{mc}\",\"src\":{{\"server\":\"{z}\",\"tr\":\"{z}\",\"en\":\"{z}\"}},\
             \"items\":[[\"minecraft:elytra\",1,432,\"Elytra\",\"Elytra\",0],\
             [\"minecraft:diamond\",64,0,\"Elmas\",\"Diamond\",-1]],\
             \"ench\":[[\"minecraft:unbreaking\",3,\"Kırılmazlık\",\"Unbreaking\"]],\
             \"groups\":[[0]]}}",
            z = "0".repeat(40)
        )
        .into_bytes()
    }

    fn manifest_for(files: &[(&str, &[u8])]) -> (Vec<u8>, BTreeMap<String, Vec<u8>>) {
        let mut versions = serde_json::Map::new();
        let mut served = BTreeMap::new();
        for (mc, bytes) in files {
            let sha = sha256_hex(bytes);
            let file = format!("{mc}/items.{}.json", &sha[..12]);
            served.insert(format!("{DATA_BASE}{file}"), bytes.to_vec());
            versions.insert(
                (*mc).to_owned(),
                serde_json::json!({"file": file, "sha256": sha, "bytes": bytes.len()}),
            );
        }
        let manifest =
            serde_json::json!({"format": 1, "default": "26.3", "versions": versions}).to_string();
        served.insert(
            format!("{DATA_BASE}{MANIFEST_FILE}"),
            manifest.clone().into_bytes(),
        );
        (manifest.into_bytes(), served)
    }

    #[tokio::test]
    async fn loads_checks_caches_and_names() {
        let dir = tempfile::tempdir().unwrap();
        let one = data_file("26.1.2");
        let two = data_file("26.3");
        let (_, served) = manifest_for(&[("26.1.2", &one), ("26.3", &two)]);
        let asked = AtomicUsize::new(0);
        let get = |url: String| {
            asked.fetch_add(1, Ordering::SeqCst);
            let answer = served.get(&url).cloned();
            async move { answer }
        };
        let cache = ItemDataCache::default();
        // The account's version, then a version between two files (the newest older one).
        let data = cache.load(dir.path(), Some("26.1.2"), &get).await.unwrap();
        assert_eq!(data.minecraft, "26.1.2");
        assert_eq!(data.text.as_bytes(), one.as_slice());
        let data = cache.load(dir.path(), Some("26.2"), &get).await.unwrap();
        assert_eq!(data.minecraft, "26.1.2", "the newest older file");
        let data = cache.load(dir.path(), None, &get).await.unwrap();
        assert_eq!(data.minecraft, "26.3", "the manifest's default");
        assert_eq!(cache.item_names("minecraft:diamond").unwrap().0, "Elmas");
        assert_eq!(cache.item_names("minecraft:stone"), None);
        // Manifest once, each file once.
        assert_eq!(asked.load(Ordering::SeqCst), 3);
        let cached: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(cached.iter().any(|n| n.starts_with("data-26.1.2-")));
        assert!(cached.contains(&MANIFEST_COPY.to_owned()));

        // A new session without the CDN reads the cached manifest and files.
        let offline = ItemDataCache::default();
        let none = |_: String| async { None };
        let data = offline.load(dir.path(), Some("26.3"), none).await.unwrap();
        assert_eq!(data.text.as_bytes(), two.as_slice());
    }

    #[tokio::test]
    async fn a_file_that_fails_its_hash_is_never_read() {
        let dir = tempfile::tempdir().unwrap();
        let good = data_file("26.3");
        let (_, mut served) = manifest_for(&[("26.3", &good)]);
        for (url, bytes) in &mut served {
            if !url.ends_with(MANIFEST_FILE) {
                bytes[10] ^= 1;
            }
        }
        let get = |url: String| {
            let answer = served.get(&url).cloned();
            async move { answer }
        };
        let cache = ItemDataCache::default();
        assert_eq!(cache.load(dir.path(), Some("26.3"), &get).await, None);
        assert!(
            !std::fs::read_dir(dir.path()).unwrap().any(|e| e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("data-")),
            "nothing unchecked is kept"
        );
        // A cached file changed on disk is fetched again, not read.
        let cache = ItemDataCache::default();
        let (_, served) = manifest_for(&[("26.3", &good)]);
        let get = |url: String| {
            let answer = served.get(&url).cloned();
            async move { answer }
        };
        cache.load(dir.path(), Some("26.3"), &get).await.unwrap();
        let name = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("data-")
            })
            .unwrap();
        std::fs::write(&name, b"{}").unwrap();
        let fresh = ItemDataCache::default();
        let data = fresh.load(dir.path(), Some("26.3"), &get).await.unwrap();
        assert_eq!(data.text.as_bytes(), good.as_slice());
    }

    #[tokio::test]
    async fn a_newer_file_out_of_reach_leaves_the_kept_one() {
        let dir = tempfile::tempdir().unwrap();
        let old = data_file("26.3");
        let (_, served) = manifest_for(&[("26.3", &old)]);
        let get = |url: String| {
            let answer = served.get(&url).cloned();
            async move { answer }
        };
        let cache = ItemDataCache::default();
        cache.load(dir.path(), Some("26.3"), &get).await.unwrap();
        // The manifest names a newer file the CDN cannot give now: the one kept here is read.
        let new = data_file("26.3.1");
        let (manifest, _) = manifest_for(&[("26.3", &new)]);
        let only_manifest = |url: String| {
            let answer = url.ends_with(MANIFEST_FILE).then(|| manifest.clone());
            async move { answer }
        };
        let later = ItemDataCache::default();
        let data = later
            .load(dir.path(), Some("26.3"), only_manifest)
            .await
            .unwrap();
        assert_eq!(data.text.as_bytes(), old.as_slice());
        // Once it can be fetched, the newer file replaces the old one, here and on disk.
        let (_, served) = manifest_for(&[("26.3", &new)]);
        let get = |url: String| {
            let answer = served.get(&url).cloned();
            async move { answer }
        };
        let fresh = ItemDataCache::default();
        let data = fresh.load(dir.path(), Some("26.3"), &get).await.unwrap();
        assert_eq!(data.text.as_bytes(), new.as_slice());
        let kept: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with("data-"))
            .collect();
        assert_eq!(kept.len(), 1, "{kept:?}");
    }

    #[test]
    fn a_version_never_takes_another_versions_file() {
        let dir = tempfile::tempdir().unwrap();
        let rc = data_file("26.1-rc1");
        let rc_file = DataFile {
            file: "x".into(),
            sha256: sha256_hex(&rc),
            bytes: rc.len() as u64,
        };
        write_cached(dir.path(), "26.1-rc1", &rc_file, &rc).unwrap();
        let release = data_file("26.1");
        let release_file = DataFile {
            file: "y".into(),
            sha256: sha256_hex(&release),
            bytes: release.len() as u64,
        };
        write_cached(dir.path(), "26.1", &release_file, &release).unwrap();
        assert!(
            read_cached(dir.path(), "26.1-rc1", &rc_file).is_some(),
            "26.1 kept 26.1-rc1's"
        );
        assert_eq!(any_cached(dir.path(), "26.1").unwrap().1, release);
        assert_eq!(
            cached_name("data-26.1-rc1-0123456789ab.json"),
            Some(("26.1-rc1", "0123456789ab"))
        );
        assert_eq!(cached_name("data-26.1-rc1.json"), None);
    }

    #[test]
    fn file_names_stay_in_the_folder() {
        let file = DataFile {
            file: "26.3/items.3f13ec021480.json".into(),
            sha256: format!("3f13ec021480{}", "0".repeat(52)),
            bytes: 1,
        };
        assert_eq!(
            cache_file_name("26.3", &file).as_deref(),
            Some("data-26.3-3f13ec021480.json")
        );
        assert_eq!(cache_file_name("../x", &file), None);
        assert_eq!(cache_file_name(".hidden", &file), None);
        assert_eq!(cache_file_name("", &file), None);
    }
}

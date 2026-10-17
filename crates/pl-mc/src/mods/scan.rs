//! Detailed jar scanning for the mod health check ([`super::check`]) and crash attribution
//! ([`crate::launch::crash::attribute_crash`]): every mod a jar declares (all `[[mods]]`
//! entries, jar-in-jar mods), its dependencies with version requirements, breaks and
//! incompatibilities, provided ids, mixin configs and the class packages it ships.
//!
//! [`ModScanner`] caches what it read by path (without `.disabled`), size and modification
//! time, so a warm scan of a mods folder costs one directory listing and a `stat` per file.
//! Reading never panics: files that are not zips, locked files, huge files and odd descriptors
//! end up as a [`JarState`] or as fields left empty.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufReader, Cursor, Read, Seek, SeekFrom};
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use parking_lot::Mutex;
use serde_json::Value;
use zip::ZipArchive;
use zip::result::ZipError;

use super::lenient_json;
use super::metadata::{
    clean_version, manifest_attribute, parse_mcmod_info, parse_mods_toml_fallback,
};
use super::version::VersionReq;
use crate::gamefiles::fsops::{MAX_METADATA_BYTES, modified_ms};
use crate::loaders::LoaderKind;

/// Jar-in-jar nesting followed at most this deep.
const MAX_NESTED_DEPTH: usize = 3;
/// Nested jars larger than this are skipped.
const MAX_NESTED_JAR_BYTES: u64 = 32 * 1024 * 1024;
/// Per top-level jar: nested jars read and bytes read for them.
const MAX_NESTED_JARS: usize = 512;
const MAX_NESTED_TOTAL_BYTES: u64 = 256 * 1024 * 1024;
/// Class entries looked at for the package index, and packages kept per jar.
const MAX_CLASS_ENTRIES: usize = 200_000;
const MAX_PACKAGES: usize = 512;
/// Packages are kept up to this many segments (`me/jellysquid/mods/sodium`).
const PACKAGE_DEPTH: usize = 4;
/// The cache is dropped when it grows past this many files (many instances scanned).
const MAX_CACHE_ENTRIES: usize = 8192;

const DISABLED_SUFFIX: &str = ".disabled";

/// The descriptor file a mod was declared in; it decides which loaders read the mod.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DescriptorKind {
    /// `fabric.mod.json` (read by Fabric and Quilt).
    Fabric,
    /// `quilt.mod.json`.
    Quilt,
    /// `META-INF/mods.toml` (Forge 1.13+, NeoForge up to 1.20.4).
    ModsToml,
    /// `META-INF/neoforge.mods.toml` (NeoForge 1.20.5+).
    NeoForgeToml,
    /// `mcmod.info` (Forge up to 1.12.2).
    McmodInfo,
}

/// How one mod relates to another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RelationKind {
    /// Required: the loader refuses to start without it.
    Depends,
    /// Fabric `recommends`/`suggests`, Forge optional dependencies: never reported.
    Optional,
    /// Fabric/Quilt `breaks`, NeoForge `incompatible`: the loader refuses to start with it.
    Breaks,
    /// Fabric `conflicts`, NeoForge `discouraged`: the loader only warns.
    Conflicts,
}

/// A dependency or incompatibility declared by a mod.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relation {
    pub id: String,
    pub kind: RelationKind,
    pub req: VersionReq,
    /// The requirement as written (`>=0.5`, `[1.20,1.21)`); `None` when any version will do.
    pub text: Option<String>,
    /// False for Forge/NeoForge dependencies that only apply on a dedicated server.
    pub client: bool,
}

/// One mod declared by a jar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredMod {
    pub id: String,
    pub name: Option<String>,
    pub version: Option<String>,
    pub descriptor: DescriptorKind,
    /// The loader the declaration is written for (a `mods.toml` that requires NeoForge is a
    /// NeoForge mod).
    pub loader: LoaderKind,
    /// Other ids this mod answers to (Fabric/Quilt `provides`).
    pub provides: Vec<String>,
    pub relations: Vec<Relation>,
    /// False for mods the client does not load (Fabric `"environment": "server"`, Quilt
    /// `dedicated_server`).
    pub client: bool,
    pub mixin_configs: Vec<String>,
    /// Jar-in-jar path(s) this mod comes from, outermost first; empty for top-level mods.
    pub nested_in: Vec<String>,
}

impl DeclaredMod {
    /// The mod's own name, else its id.
    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.id)
    }

    /// True when the mod answers to `id` (its own id or a provided one).
    pub fn answers_to(&self, id: &str) -> bool {
        self.id == id || self.provides.iter().any(|p| p == id)
    }
}

/// Whether a jar could be read.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum JarState {
    #[default]
    Ok,
    /// Not a valid zip (an interrupted download, a web page saved as `.jar`, an empty file).
    /// `certain` is true when the file has no zip directory at all, so every loader fails on it.
    Corrupt { detail: String, certain: bool },
    /// Could not be opened (locked by another program, no permission). Not cached.
    Unreadable { detail: String },
}

/// What a jar contains, independent of its file name and enabled state.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct JarContents {
    pub state: JarState,
    /// Descriptor files present, including ones that could not be parsed.
    pub descriptors: Vec<DescriptorKind>,
    /// Descriptor files present but unreadable (invalid JSON or TOML, no mod id).
    pub broken_descriptors: Vec<DescriptorKind>,
    /// Mods declared at the top level.
    pub mods: Vec<DeclaredMod>,
    /// Mods from jar-in-jar files, depth first.
    pub nested: Vec<DeclaredMod>,
    /// Mixin configs named by `MixinConfigs` in the manifest (Forge).
    pub manifest_mixins: Vec<String>,
    /// `FMLModType` from the manifest (`LIBRARY`, `GAMELIBRARY`), if any.
    pub fml_mod_type: Option<String>,
    /// Class package prefixes (`me/jellysquid/mods/sodium`), for crash attribution.
    pub packages: Vec<String>,
}

impl JarContents {
    /// Top-level mods first, then nested ones.
    pub fn all_mods(&self) -> impl Iterator<Item = &DeclaredMod> {
        self.mods.iter().chain(&self.nested)
    }

    /// Every mixin config the jar or its nested mods name.
    pub fn mixin_configs(&self) -> impl Iterator<Item = &str> {
        self.all_mods()
            .flat_map(|m| m.mixin_configs.iter())
            .chain(&self.manifest_mixins)
            .map(String::as_str)
    }
}

/// One `*.jar` or `*.jar.disabled` file of a mods folder.
#[derive(Debug, Clone)]
pub struct ScannedJar {
    /// File name as on disk (with `.disabled` when disabled).
    pub file_name: String,
    pub path: PathBuf,
    pub enabled: bool,
    pub size: u64,
    pub modified_ms: Option<i64>,
    pub contents: Arc<JarContents>,
}

impl ScannedJar {
    /// A jar read from memory (tests, callers that already hold the bytes).
    pub fn from_bytes(file_name: &str, bytes: &[u8]) -> Self {
        Self {
            file_name: file_name.to_owned(),
            path: PathBuf::from(file_name),
            enabled: jar_name_state(file_name).unwrap_or(true),
            size: bytes.len() as u64,
            modified_ms: None,
            contents: Arc::new(scan_jar_bytes(bytes)),
        }
    }

    /// The file name without `.disabled`.
    pub fn base_name(&self) -> &str {
        strip_disabled(&self.file_name)
    }

    /// The first declared mod's name, else the file name without extensions.
    pub fn display_name(&self) -> &str {
        self.contents
            .mods
            .first()
            .map(DeclaredMod::display_name)
            .unwrap_or_else(|| file_stem(&self.file_name))
    }

    /// The name players read: the first declared mod's name, else one read from the file name
    /// (`sodium-fabric-0.6.13+mc1.21.4.jar` → `Sodium`, see
    /// [`super::repair::display_name_for_file`]).
    pub fn readable_name(&self) -> String {
        match self.contents.mods.first() {
            Some(m) => m.display_name().to_owned(),
            None => super::repair::display_name_for_file(&self.file_name),
        }
    }
}

/// `x.jar.disabled` → `x.jar` (any case).
pub fn strip_disabled(name: &str) -> &str {
    let cut = name.len().saturating_sub(DISABLED_SUFFIX.len());
    match name.get(cut..) {
        Some(tail) if tail.eq_ignore_ascii_case(DISABLED_SUFFIX) => &name[..cut],
        _ => name,
    }
}

/// `Sodium-0.5.jar.disabled` → `Sodium-0.5`.
pub fn file_stem(name: &str) -> &str {
    let base = strip_disabled(name);
    base.rsplit_once('.').map_or(base, |(stem, _)| stem)
}

/// `Some(true)` for `*.jar`, `Some(false)` for `*.jar.disabled`, `None` for anything else
/// (and for hidden files, which the loaders skip).
pub fn jar_name_state(name: &str) -> Option<bool> {
    if name.starts_with('.') {
        return None;
    }
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".jar") {
        Some(true)
    } else if lower.ends_with(".jar.disabled") {
        Some(false)
    } else {
        None
    }
}

/// A scan of one mods folder.
#[derive(Debug, Clone, Default)]
pub struct ModSnapshot {
    pub dir: PathBuf,
    /// Every `*.jar` and `*.jar.disabled` file, sorted by file name.
    pub jars: Vec<ScannedJar>,
}

/// Classes of these packages belong to the game, the loaders or Java, never to a mod.
const PLATFORM_PACKAGES: [&str; 13] = [
    "net/minecraft/",
    "com/mojang/",
    "java/",
    "javax/",
    "jdk/",
    "sun/",
    "net/fabricmc/loader/",
    "org/quiltmc/loader/",
    "net/minecraftforge/",
    "net/neoforged/",
    "cpw/mods/",
    "org/spongepowered/",
    "com/llamalad7/mixinextras/",
];

impl ModSnapshot {
    pub fn new(dir: impl Into<PathBuf>, mut jars: Vec<ScannedJar>) -> Self {
        jars.sort_by_cached_key(|j| (j.file_name.to_lowercase(), j.file_name.clone()));
        Self {
            dir: dir.into(),
            jars,
        }
    }

    pub fn enabled(&self) -> impl Iterator<Item = &ScannedJar> {
        self.jars.iter().filter(|j| j.enabled)
    }

    pub fn disabled(&self) -> impl Iterator<Item = &ScannedJar> {
        self.jars.iter().filter(|j| !j.enabled)
    }

    /// A jar by file name, with or without `.disabled`.
    pub fn by_file(&self, name: &str) -> Option<&ScannedJar> {
        let name = name.trim();
        let base = strip_disabled(name);
        self.jars.iter().find(|j| j.file_name == name).or_else(|| {
            self.jars
                .iter()
                .find(|j| j.base_name().eq_ignore_ascii_case(base))
        })
    }

    /// Mods answering to `id` (top level and nested), enabled jars first.
    pub fn declaring<'a>(
        &'a self,
        id: &str,
    ) -> impl Iterator<Item = (&'a ScannedJar, &'a DeclaredMod)> {
        self.enabled().chain(self.disabled()).flat_map(move |jar| {
            jar.contents
                .all_mods()
                .filter(move |m| m.answers_to(id))
                .map(move |m| (jar, m))
        })
    }

    /// The jar (and mod, when known) that ships a mixin config such as `sodium.mixins.json`.
    pub fn by_mixin_config(&self, config: &str) -> Option<(&ScannedJar, Option<&DeclaredMod>)> {
        let config = config.trim();
        for jar in self.enabled().chain(self.disabled()) {
            if let Some(m) = jar
                .contents
                .all_mods()
                .find(|m| m.mixin_configs.iter().any(|c| c == config))
            {
                return Some((jar, Some(m)));
            }
            if jar.contents.manifest_mixins.iter().any(|c| c == config) {
                return Some((jar, jar.contents.mods.first()));
            }
        }
        None
    }

    /// A top-level mod by display name or id (any case), enabled jars first.
    pub fn by_name(&self, name: &str) -> Option<(&ScannedJar, &DeclaredMod)> {
        let name = name.trim();
        self.enabled().chain(self.disabled()).find_map(|jar| {
            jar.contents
                .mods
                .iter()
                .find(|m| {
                    m.display_name().eq_ignore_ascii_case(name) || m.id.eq_ignore_ascii_case(name)
                })
                .map(|m| (jar, m))
        })
    }

    /// The enabled jar shipping the package of `class` (`a.b.C`, `a/b/C` or `a/b/C.class`),
    /// by the longest matching package. `None` for game and loader classes and when several
    /// jars ship that package.
    pub fn by_class(&self, class: &str) -> Option<&ScannedJar> {
        let path = class.trim().trim_end_matches(".class").replace('.', "/");
        let (package, _) = path.rsplit_once('/')?;
        let package = format!("{package}/");
        if PLATFORM_PACKAGES.iter().any(|p| package.starts_with(p)) {
            return None;
        }
        let matched = |jar: &ScannedJar| {
            jar.contents
                .packages
                .iter()
                .filter(|p| {
                    package
                        .strip_prefix(p.as_str())
                        .is_some_and(|rest| rest.starts_with('/'))
                })
                .map(String::len)
                .max()
        };
        let mut best: Option<(usize, &ScannedJar)> = None;
        let mut tied = false;
        for jar in self.enabled() {
            let Some(len) = matched(jar) else {
                continue;
            };
            match best {
                Some((best_len, _)) if best_len > len => {}
                Some((best_len, _)) if best_len == len => tied = true,
                _ => {
                    best = Some((len, jar));
                    tied = false;
                }
            }
        }
        best.filter(|_| !tied).map(|(_, jar)| jar)
    }
}

/// Read one jar from memory.
pub fn scan_jar_bytes(bytes: &[u8]) -> JarContents {
    guarded(|| match ZipArchive::new(Cursor::new(bytes)) {
        Ok(mut archive) => read_archive(&mut archive, &[], &mut Budget::default(), true),
        Err(error) => {
            let certain = bytes.len() < 22 || !contains_eocd(bytes);
            JarContents {
                state: JarState::Corrupt {
                    detail: error.to_string(),
                    certain,
                },
                ..JarContents::default()
            }
        }
    })
}

/// Read one jar from disk (uncached).
pub fn scan_jar_file(path: &Path) -> JarContents {
    guarded(|| {
        let file = match File::open(path) {
            Ok(file) => file,
            Err(error) => return unreadable(error.to_string()),
        };
        let len = file.metadata().map(|m| m.len()).unwrap_or(0);
        if len == 0 {
            return JarContents {
                state: JarState::Corrupt {
                    detail: "empty file".to_owned(),
                    certain: true,
                },
                ..JarContents::default()
            };
        }
        let mut reader = BufReader::new(file);
        let error = match ZipArchive::new(&mut reader) {
            Ok(mut archive) => {
                return read_archive(&mut archive, &[], &mut Budget::default(), true);
            }
            Err(error) => error,
        };
        match error {
            ZipError::Io(error) if error.kind() != std::io::ErrorKind::UnexpectedEof => {
                unreadable(error.to_string())
            }
            error => {
                let certain = len < 22 || !file_has_eocd(reader.get_mut()).unwrap_or(true);
                JarContents {
                    state: JarState::Corrupt {
                        detail: error.to_string(),
                        certain,
                    },
                    ..JarContents::default()
                }
            }
        }
    })
}

fn unreadable(detail: String) -> JarContents {
    JarContents {
        state: JarState::Unreadable { detail },
        ..JarContents::default()
    }
}

/// Zip readers on untrusted files must not take the scan down: a panic becomes "unreadable".
fn guarded(read: impl FnOnce() -> JarContents) -> JarContents {
    std::panic::catch_unwind(AssertUnwindSafe(read)).unwrap_or_else(|_| {
        tracing::warn!("mod jar reader panicked");
        unreadable("reader panicked".to_owned())
    })
}

const EOCD: &[u8; 4] = b"PK\x05\x06";
/// The end-of-central-directory record sits in the last 64 KiB + 22 bytes of a zip.
const EOCD_SEARCH: u64 = 65_557;

fn contains_eocd(bytes: &[u8]) -> bool {
    let start = bytes.len().saturating_sub(EOCD_SEARCH as usize);
    bytes[start..].windows(4).any(|w| w == EOCD)
}

fn file_has_eocd(file: &mut File) -> std::io::Result<bool> {
    let len = file.seek(SeekFrom::End(0))?;
    file.seek(SeekFrom::Start(len.saturating_sub(EOCD_SEARCH)))?;
    let mut tail = Vec::new();
    file.take(EOCD_SEARCH).read_to_end(&mut tail)?;
    Ok(contains_eocd(&tail))
}

#[derive(Default)]
struct Budget {
    jars: usize,
    bytes: u64,
}

enum EntryText {
    Missing,
    Text(String),
    /// Present but unreadable (decompression error, too large).
    Failed,
}

fn entry_path(path: &str) -> String {
    path.trim()
        .trim_start_matches("./")
        .trim_start_matches('/')
        .to_owned()
}

fn read_text<R: Read + Seek>(archive: &mut ZipArchive<R>, name: &str) -> EntryText {
    if archive.index_for_name(name).is_none() {
        return EntryText::Missing;
    }
    let Ok(mut file) = archive.by_name(name) else {
        return EntryText::Failed;
    };
    if file.is_dir() {
        return EntryText::Missing;
    }
    if file.size() > MAX_METADATA_BYTES {
        return EntryText::Failed;
    }
    let mut bytes = Vec::new();
    match (&mut file)
        .take(MAX_METADATA_BYTES + 1)
        .read_to_end(&mut bytes)
    {
        Ok(_) if bytes.len() as u64 <= MAX_METADATA_BYTES => {
            EntryText::Text(String::from_utf8_lossy(&bytes).into_owned())
        }
        _ => EntryText::Failed,
    }
}

fn read_bytes<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
    limit: u64,
) -> Option<Vec<u8>> {
    let file = archive.by_name(name).ok()?;
    if file.is_dir() || file.size() > limit {
        return None;
    }
    let mut bytes = Vec::with_capacity(usize::try_from(file.size().min(limit)).unwrap_or(0));
    file.take(limit + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() as u64 <= limit).then_some(bytes)
}

fn read_archive<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    nested_in: &[String],
    budget: &mut Budget,
    top_level: bool,
) -> JarContents {
    let mut contents = JarContents::default();
    let manifest = match read_text(archive, "META-INF/MANIFEST.MF") {
        EntryText::Text(text) => Some(text),
        _ => None,
    };
    let jar_version = manifest
        .as_deref()
        .and_then(|m| manifest_attribute(m, "Implementation-Version"));
    if let Some(manifest) = &manifest {
        contents.manifest_mixins = manifest_attribute(manifest, "MixinConfigs")
            .map(|list| split_list(&list))
            .unwrap_or_default();
        contents.fml_mod_type = manifest_attribute(manifest, "FMLModType");
    }

    let mut nested_paths: Vec<String> = Vec::new();
    let note = |contents: &mut JarContents, kind: DescriptorKind, parsed: bool| {
        contents.descriptors.push(kind);
        if !parsed {
            contents.broken_descriptors.push(kind);
        }
    };

    match read_text(archive, "fabric.mod.json") {
        EntryText::Missing => {}
        EntryText::Failed => note(&mut contents, DescriptorKind::Fabric, false),
        EntryText::Text(text) => {
            let parsed = parse_fabric_json(&text, nested_in);
            note(&mut contents, DescriptorKind::Fabric, parsed.is_some());
            if let Some((declared, jars)) = parsed {
                contents.mods.push(declared);
                nested_paths.extend(jars);
            }
        }
    }
    match read_text(archive, "quilt.mod.json") {
        EntryText::Missing => {}
        EntryText::Failed => note(&mut contents, DescriptorKind::Quilt, false),
        EntryText::Text(text) => {
            let parsed = parse_quilt_json(&text, nested_in);
            note(&mut contents, DescriptorKind::Quilt, parsed.is_some());
            if let Some((declared, jars)) = parsed {
                contents.mods.push(declared);
                nested_paths.extend(jars);
            }
        }
    }
    for (name, kind) in [
        ("META-INF/neoforge.mods.toml", DescriptorKind::NeoForgeToml),
        ("META-INF/mods.toml", DescriptorKind::ModsToml),
    ] {
        match read_text(archive, name) {
            EntryText::Missing => {}
            EntryText::Failed => note(&mut contents, kind, false),
            EntryText::Text(text) => {
                let parsed = parse_mods_toml(&text, kind, jar_version.as_deref(), nested_in);
                note(&mut contents, kind, parsed.is_some());
                contents.mods.extend(parsed.unwrap_or_default());
            }
        }
    }
    match read_text(archive, "mcmod.info") {
        EntryText::Missing => {}
        EntryText::Failed => note(&mut contents, DescriptorKind::McmodInfo, false),
        EntryText::Text(text) => {
            let parsed = parse_mcmod_info(&text).and_then(|d| {
                let id = d.meta.id.filter(|id| valid_id(id))?;
                Some(DeclaredMod {
                    id,
                    name: d.meta.name,
                    version: d.meta.version,
                    descriptor: DescriptorKind::McmodInfo,
                    loader: LoaderKind::Forge,
                    provides: Vec::new(),
                    relations: Vec::new(),
                    client: true,
                    mixin_configs: Vec::new(),
                    nested_in: nested_in.to_vec(),
                })
            });
            note(&mut contents, DescriptorKind::McmodInfo, parsed.is_some());
            contents.mods.extend(parsed);
        }
    }
    if let EntryText::Text(text) = read_text(archive, "META-INF/jarjar/metadata.json") {
        nested_paths.extend(jarjar_paths(&text));
    }

    if nested_in.len() < MAX_NESTED_DEPTH {
        let mut seen = HashSet::new();
        for path in nested_paths {
            if !seen.insert(path.clone())
                || budget.jars >= MAX_NESTED_JARS
                || budget.bytes >= MAX_NESTED_TOTAL_BYTES
            {
                continue;
            }
            let limit = MAX_NESTED_JAR_BYTES.min(MAX_NESTED_TOTAL_BYTES - budget.bytes);
            let Some(bytes) = read_bytes(archive, &path, limit) else {
                continue;
            };
            budget.jars += 1;
            budget.bytes += bytes.len() as u64;
            let Ok(mut inner) = ZipArchive::new(Cursor::new(bytes)) else {
                continue;
            };
            let mut chain = nested_in.to_vec();
            chain.push(path);
            let inner = read_archive(&mut inner, &chain, budget, false);
            contents.nested.extend(inner.mods);
            contents.nested.extend(inner.nested);
        }
    }
    if top_level {
        contents.packages = class_packages(archive);
    }
    contents
}

fn class_packages<R: Read + Seek>(archive: &ZipArchive<R>) -> Vec<String> {
    let mut packages: HashSet<String> = HashSet::new();
    for name in archive
        .file_names()
        .take(MAX_CLASS_ENTRIES)
        .filter_map(Result::ok)
    {
        let Some((dir, _)) = name.strip_suffix(".class").and_then(|n| n.rsplit_once('/')) else {
            continue;
        };
        if dir.starts_with("META-INF/") || dir.is_empty() {
            continue;
        }
        let end = dir
            .match_indices('/')
            .nth(PACKAGE_DEPTH - 1)
            .map_or(dir.len(), |(i, _)| i);
        let prefix = &dir[..end];
        if !packages.contains(prefix) {
            if packages.len() >= MAX_PACKAGES {
                break;
            }
            packages.insert(prefix.to_owned());
        }
    }
    let mut sorted: Vec<String> = packages.into_iter().collect();
    sorted.sort();
    sorted
}

// --- descriptor parsing ------------------------------------------------------------------------

fn json_string(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(s) => Some(s.trim().to_owned()).filter(|s| !s.is_empty()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn json_strings(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(items)) => items.iter().filter_map(|i| json_string(Some(i))).collect(),
        Some(Value::String(s)) if !s.trim().is_empty() => vec![s.trim().to_owned()],
        _ => Vec::new(),
    }
}

/// A usable mod id: not empty, no unexpanded build placeholder, no whitespace.
fn valid_id(id: &str) -> bool {
    !id.is_empty() && !id.contains("${") && !id.contains(char::is_whitespace)
}

/// `MixinConfigs: a.mixins.json, b.mixins.json`
fn split_list(list: &str) -> Vec<String> {
    list.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

fn fabric_requirement(value: &Value) -> (VersionReq, Option<String>) {
    match value {
        Value::String(text) => {
            let req = VersionReq::fabric(text);
            let text = (!req.is_any()).then(|| text.trim().to_owned());
            (req, text)
        }
        Value::Array(items) => match items.iter().map(Value::as_str).collect::<Option<Vec<_>>>() {
            Some(texts) => {
                let req = VersionReq::fabric_any(texts.iter().copied());
                let text = (!req.is_any()).then(|| texts.join(" || "));
                (req, text)
            }
            None => (VersionReq::Unknown, None),
        },
        _ => (VersionReq::Unknown, None),
    }
}

/// `mixins` as names or `{config, environment}` objects (schema 1), or the schema 0 object of
/// lists.
fn fabric_mixins(value: Option<&Value>) -> Vec<String> {
    let one = |item: &Value| match item {
        Value::Object(obj) => json_string(obj.get("config")),
        other => json_string(Some(other)),
    };
    match value {
        Some(Value::Array(items)) => items.iter().filter_map(one).collect(),
        Some(Value::Object(sides)) => sides
            .values()
            .flat_map(|list| json_strings(Some(list)))
            .collect(),
        Some(other) => one(other).into_iter().collect(),
        None => Vec::new(),
    }
}

pub(crate) fn parse_fabric_json(
    text: &str,
    nested_in: &[String],
) -> Option<(DeclaredMod, Vec<String>)> {
    let value = lenient_json::parse(text)?;
    let obj = value.as_object()?;
    let id = json_string(obj.get("id")).filter(|id| valid_id(id))?;
    let schema = obj
        .get("schemaVersion")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let mut relations = Vec::new();
    // Schema 0 (early 2019 mods) words requirements differently; only its ids are used.
    if schema >= 1 {
        for (key, kind) in [
            ("depends", RelationKind::Depends),
            ("recommends", RelationKind::Optional),
            ("suggests", RelationKind::Optional),
            ("breaks", RelationKind::Breaks),
            ("conflicts", RelationKind::Conflicts),
        ] {
            let Some(Value::Object(map)) = obj.get(key) else {
                continue;
            };
            for (dep, requirement) in map {
                let dep = dep.trim();
                if !valid_id(dep) {
                    continue;
                }
                let (req, text) = fabric_requirement(requirement);
                relations.push(Relation {
                    id: dep.to_owned(),
                    kind,
                    req,
                    text,
                    client: true,
                });
            }
        }
    }
    let jars = match obj.get("jars") {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|j| j.get("file").and_then(Value::as_str))
            .map(entry_path)
            .filter(|p| !p.is_empty())
            .collect(),
        _ => Vec::new(),
    };
    let environment = json_string(obj.get("environment"));
    Some((
        DeclaredMod {
            id,
            name: json_string(obj.get("name")),
            version: clean_version(json_string(obj.get("version")), None),
            descriptor: DescriptorKind::Fabric,
            loader: LoaderKind::Fabric,
            provides: json_strings(obj.get("provides"))
                .into_iter()
                .filter(|p| valid_id(p))
                .collect(),
            relations,
            client: environment.as_deref() != Some("server"),
            mixin_configs: fabric_mixins(obj.get("mixins")),
            nested_in: nested_in.to_vec(),
        },
        jars,
    ))
}

/// `group:id` → `id`.
fn quilt_id(raw: &str) -> String {
    raw.rsplit(':').next().unwrap_or(raw).trim().to_owned()
}

/// Quilt reads a bare version differently from Fabric; only explicit operators and `*` are
/// judged.
fn quilt_requirement(value: Option<&Value>) -> (VersionReq, Option<String>) {
    let certain = |text: &str| {
        let text = text.trim();
        text == "*" || (text.starts_with(['=', '>', '<', '~', '^']) && !text.contains(' '))
    };
    match value {
        None => (VersionReq::Any, None),
        Some(Value::String(text)) if certain(text) => {
            let req = VersionReq::fabric(text);
            let text = (!req.is_any()).then(|| text.trim().to_owned());
            (req, text)
        }
        Some(Value::Array(items)) => {
            match items.iter().map(Value::as_str).collect::<Option<Vec<_>>>() {
                Some(texts) if !texts.is_empty() && texts.iter().all(|t| certain(t)) => {
                    let req = VersionReq::fabric_any(texts.iter().copied());
                    let text = (!req.is_any()).then(|| texts.join(" || "));
                    (req, text)
                }
                _ => (VersionReq::Unknown, None),
            }
        }
        _ => (VersionReq::Unknown, None),
    }
}

fn quilt_relation(item: &Value, kind: RelationKind) -> Option<Relation> {
    match item {
        Value::String(id) => Some(Relation {
            id: quilt_id(id),
            kind,
            req: VersionReq::Any,
            text: None,
            client: true,
        })
        .filter(|r| valid_id(&r.id)),
        Value::Object(obj) => {
            let id = quilt_id(obj.get("id")?.as_str()?);
            if !valid_id(&id) {
                return None;
            }
            // `unless` makes the relation conditional; such relations are never judged.
            let conditional = obj.contains_key("unless");
            let optional = obj.get("optional").and_then(Value::as_bool) == Some(true);
            let kind = match kind {
                RelationKind::Depends if optional || conditional => RelationKind::Optional,
                RelationKind::Breaks if conditional => return None,
                other => other,
            };
            let (req, text) = quilt_requirement(obj.get("versions"));
            Some(Relation {
                id,
                kind,
                req,
                text,
                client: true,
            })
        }
        // An array lists alternatives (any one satisfies it): not judged.
        _ => None,
    }
}

pub(crate) fn parse_quilt_json(
    text: &str,
    nested_in: &[String],
) -> Option<(DeclaredMod, Vec<String>)> {
    let value = lenient_json::parse(text)?;
    let loader = value.get("quilt_loader")?.as_object()?;
    let id = json_string(loader.get("id")).filter(|id| valid_id(id))?;
    let metadata = loader.get("metadata").and_then(Value::as_object);
    let provides = match loader.get("provides") {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|p| match p {
                Value::Object(obj) => obj.get("id").and_then(Value::as_str),
                other => other.as_str(),
            })
            .map(quilt_id)
            .filter(|p| valid_id(p))
            .collect(),
        _ => Vec::new(),
    };
    let mut relations = Vec::new();
    for (key, kind) in [
        ("depends", RelationKind::Depends),
        ("breaks", RelationKind::Breaks),
    ] {
        if let Some(Value::Array(items)) = loader.get(key) {
            relations.extend(items.iter().filter_map(|item| quilt_relation(item, kind)));
        }
    }
    let jars = json_strings(loader.get("jars"))
        .iter()
        .map(|p| entry_path(p))
        .filter(|p| !p.is_empty())
        .collect();
    let environment = value
        .get("minecraft")
        .and_then(|m| m.get("environment"))
        .and_then(Value::as_str);
    Some((
        DeclaredMod {
            id,
            name: metadata.and_then(|m| json_string(m.get("name"))),
            version: clean_version(json_string(loader.get("version")), None),
            descriptor: DescriptorKind::Quilt,
            loader: LoaderKind::Quilt,
            provides,
            relations,
            client: environment != Some("dedicated_server"),
            mixin_configs: json_strings(value.get("mixin")),
            nested_in: nested_in.to_vec(),
        },
        jars,
    ))
}

fn toml_string(table: &toml::Table, key: &str) -> Option<String> {
    table
        .get(key)?
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn toml_relation(dep: &toml::Table) -> Option<Relation> {
    let id = toml_string(dep, "modId").filter(|id| valid_id(id))?;
    let kind = match (
        dep.get("type").and_then(toml::Value::as_str),
        dep.get("mandatory").and_then(toml::Value::as_bool),
    ) {
        (Some(kind), _) => match kind.trim().to_ascii_lowercase().as_str() {
            "required" => RelationKind::Depends,
            "incompatible" => RelationKind::Breaks,
            "discouraged" => RelationKind::Conflicts,
            _ => RelationKind::Optional,
        },
        (None, Some(true)) => RelationKind::Depends,
        // `mandatory = false`, or neither field (FML refuses such files anyway).
        _ => RelationKind::Optional,
    };
    let range = toml_string(dep, "versionRange");
    let req = VersionReq::maven(range.as_deref());
    let text = range.filter(|_| !req.is_any());
    let client = !dep
        .get("side")
        .and_then(toml::Value::as_str)
        .is_some_and(|side| side.trim().eq_ignore_ascii_case("server"));
    Some(Relation {
        id,
        kind,
        req,
        text,
        client,
    })
}

pub(crate) fn parse_mods_toml(
    text: &str,
    descriptor: DescriptorKind,
    jar_version: Option<&str>,
    nested_in: &[String],
) -> Option<Vec<DeclaredMod>> {
    let default_loader = if descriptor == DescriptorKind::NeoForgeToml {
        LoaderKind::NeoForge
    } else {
        LoaderKind::Forge
    };
    let Ok(table) = toml::from_str::<toml::Table>(text) else {
        // Strict TOML rejects it but FML's reader may not: keep the first mod's id only.
        let found = parse_mods_toml_fallback(text, default_loader, jar_version)?;
        let id = found.meta.id.filter(|id| valid_id(id))?;
        return Some(vec![DeclaredMod {
            id,
            name: found.meta.name,
            version: found.meta.version,
            descriptor,
            loader: default_loader,
            provides: Vec::new(),
            relations: Vec::new(),
            client: true,
            mixin_configs: Vec::new(),
            nested_in: nested_in.to_vec(),
        }]);
    };
    let deps = table.get("dependencies").and_then(toml::Value::as_table);
    let relations_of = |owner: &str| -> Vec<Relation> {
        deps.and_then(|d| d.get(owner))
            .and_then(toml::Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(toml::Value::as_table)
                    .filter_map(toml_relation)
                    .collect()
            })
            .unwrap_or_default()
    };
    let mixins: Vec<String> = table
        .get("mixins")
        .and_then(toml::Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(toml::Value::as_table)
                .filter_map(|m| toml_string(m, "config"))
                .collect()
        })
        .unwrap_or_default();
    let mut out: Vec<DeclaredMod> = Vec::new();
    for entry in table
        .get("mods")?
        .as_array()?
        .iter()
        .filter_map(toml::Value::as_table)
    {
        let Some(id) = toml_string(entry, "modId").filter(|id| valid_id(id)) else {
            continue;
        };
        let relations = relations_of(&id);
        // NeoForge mods for 1.20.2–1.20.4 still used `mods.toml`; they require `neoforge`.
        let loader = if relations
            .iter()
            .any(|r| r.kind == RelationKind::Depends && r.id.eq_ignore_ascii_case("neoforge"))
        {
            LoaderKind::NeoForge
        } else {
            default_loader
        };
        out.push(DeclaredMod {
            name: toml_string(entry, "displayName"),
            version: clean_version(toml_string(entry, "version"), jar_version),
            descriptor,
            loader,
            provides: Vec::new(),
            relations,
            client: true,
            mixin_configs: Vec::new(),
            nested_in: nested_in.to_vec(),
            id,
        });
    }
    if let Some(first) = out.first_mut() {
        first.mixin_configs = mixins;
    }
    (!out.is_empty()).then_some(out)
}

fn jarjar_paths(text: &str) -> Vec<String> {
    lenient_json::parse(text)
        .and_then(|value| {
            value.get("jars").and_then(Value::as_array).map(|jars| {
                jars.iter()
                    .filter_map(|j| j.get("path").and_then(Value::as_str))
                    .map(entry_path)
                    .filter(|p| !p.is_empty())
                    .collect()
            })
        })
        .unwrap_or_default()
}

// --- cached directory scans --------------------------------------------------------------------

struct CacheEntry {
    size: u64,
    modified_ms: Option<i64>,
    contents: Arc<JarContents>,
}

/// Scans mods folders and remembers what each jar contained. Cheap to share; safe to use from
/// several threads at once.
#[derive(Default)]
pub struct ModScanner {
    cache: Mutex<HashMap<PathBuf, CacheEntry>>,
}

static SHARED: LazyLock<ModScanner> = LazyLock::new(ModScanner::new);

impl ModScanner {
    pub fn new() -> Self {
        Self::default()
    }

    /// The process-wide scanner (one cache for every instance).
    pub fn shared() -> &'static Self {
        &SHARED
    }

    /// Forget everything read so far.
    pub fn clear(&self) {
        self.cache.lock().clear();
    }

    /// Number of cached jars.
    pub fn cached(&self) -> usize {
        self.cache.lock().len()
    }

    /// Scan every `*.jar` and `*.jar.disabled` file of `dir` (a missing folder has none).
    /// Blocking: unknown or changed jars are read on a few threads.
    pub fn scan_dir(&self, dir: &Path) -> ModSnapshot {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return ModSnapshot::new(dir, Vec::new());
        };
        struct Found {
            file_name: String,
            path: PathBuf,
            key: PathBuf,
            enabled: bool,
            size: u64,
            modified_ms: Option<i64>,
        }
        let mut found: Vec<Found> = Vec::new();
        for entry in entries.flatten() {
            let file_name = entry.file_name().to_string_lossy().into_owned();
            let Some(enabled) = jar_name_state(&file_name) else {
                continue;
            };
            let path = entry.path();
            // Follows symbolic links, as the loaders do; broken links and folders are skipped.
            let Ok(meta) = std::fs::metadata(&path) else {
                continue;
            };
            if !meta.is_file() {
                continue;
            }
            found.push(Found {
                key: dir.join(strip_disabled(&file_name)),
                file_name,
                path,
                enabled,
                size: meta.len(),
                modified_ms: modified_ms(&meta),
            });
        }

        let mut contents: Vec<Option<Arc<JarContents>>> = {
            let cache = self.cache.lock();
            found
                .iter()
                .map(|f| {
                    cache
                        .get(&f.key)
                        .filter(|c| c.size == f.size && c.modified_ms == f.modified_ms)
                        .map(|c| Arc::clone(&c.contents))
                })
                .collect()
        };
        let misses: Vec<usize> = (0..found.len())
            .filter(|i| contents[*i].is_none())
            .collect();
        if !misses.is_empty() {
            let paths: Vec<&Path> = misses.iter().map(|i| found[*i].path.as_path()).collect();
            let read = scan_many(&paths);
            let mut cache = self.cache.lock();
            if cache.len() + read.len() > MAX_CACHE_ENTRIES {
                cache.clear();
            }
            for (i, jar) in misses.into_iter().zip(read) {
                let f = &found[i];
                if !matches!(jar.state, JarState::Unreadable { .. }) {
                    cache.insert(
                        f.key.clone(),
                        CacheEntry {
                            size: f.size,
                            modified_ms: f.modified_ms,
                            contents: Arc::clone(&jar),
                        },
                    );
                }
                contents[i] = Some(jar);
            }
        }
        // Forget jars of this folder that are gone.
        {
            let keys: HashSet<&Path> = found.iter().map(|f| f.key.as_path()).collect();
            self.cache
                .lock()
                .retain(|key, _| key.parent() != Some(dir) || keys.contains(key.as_path()));
        }

        let jars = found
            .into_iter()
            .zip(contents)
            .map(|(f, contents)| ScannedJar {
                file_name: f.file_name,
                path: f.path,
                enabled: f.enabled,
                size: f.size,
                modified_ms: f.modified_ms,
                contents: contents.unwrap_or_default(),
            })
            .collect();
        ModSnapshot::new(dir, jars)
    }
}

/// Read several jars on up to eight threads, keeping their order.
fn scan_many(paths: &[&Path]) -> Vec<Arc<JarContents>> {
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(1, 8)
        .min(paths.len().div_ceil(4).max(1));
    if threads <= 1 {
        return paths.iter().map(|p| Arc::new(scan_jar_file(p))).collect();
    }
    let chunk = paths.len().div_ceil(threads);
    std::thread::scope(|scope| {
        let handles: Vec<_> = paths
            .chunks(chunk)
            .map(|part| {
                scope.spawn(move || {
                    part.iter()
                        .map(|p| Arc::new(scan_jar_file(p)))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .zip(paths.chunks(chunk))
            .flat_map(|(handle, part)| {
                handle.join().unwrap_or_else(|_| {
                    part.iter()
                        .map(|_| Arc::new(unreadable("reader panicked".to_owned())))
                        .collect()
                })
            })
            .collect()
    })
}

#[cfg(test)]
pub(crate) mod test_jars {
    //! Jar fixtures shared by the scan, check, repair and crash tests.

    use super::super::test_support::zip_bytes;

    pub fn fabric_json(id: &str, version: &str, extra: &str) -> String {
        let extra = if extra.is_empty() {
            String::new()
        } else {
            format!(", {extra}")
        };
        format!(
            r#"{{"schemaVersion": 1, "id": "{id}", "version": "{version}", "name": "{id} name"{extra}}}"#
        )
    }

    /// A Fabric jar with `fabric.mod.json` built from `extra` JSON members.
    pub fn fabric_jar(id: &str, version: &str, extra: &str) -> Vec<u8> {
        zip_bytes(&[(
            "fabric.mod.json",
            fabric_json(id, version, extra).as_bytes(),
        )])
    }

    /// A Fabric jar that nests `inner` jars under `META-INF/jars/`.
    pub fn fabric_jar_with_nested(id: &str, version: &str, inner: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let list: Vec<String> = inner
            .iter()
            .map(|(name, _)| format!(r#"{{"file": "META-INF/jars/{name}"}}"#))
            .collect();
        let json = fabric_json(id, version, &format!(r#""jars": [{}]"#, list.join(",")));
        let paths: Vec<String> = inner
            .iter()
            .map(|(n, _)| format!("META-INF/jars/{n}"))
            .collect();
        let mut entries: Vec<(&str, &[u8])> = vec![("fabric.mod.json", json.as_bytes())];
        for ((_, bytes), path) in inner.iter().zip(&paths) {
            entries.push((path.as_str(), bytes.as_slice()));
        }
        zip_bytes(&entries)
    }

    /// A Forge (`neo = false`) or NeoForge jar with one `[[mods]]` entry and dependency tables.
    pub fn toml_jar(neo: bool, id: &str, version: &str, deps: &str) -> Vec<u8> {
        let text = format!(
            "modLoader=\"javafml\"\nloaderVersion=\"[1,)\"\nlicense=\"MIT\"\n[[mods]]\nmodId=\"{id}\"\nversion=\"{version}\"\ndisplayName=\"{id} name\"\n{deps}"
        );
        let name = if neo {
            "META-INF/neoforge.mods.toml"
        } else {
            "META-INF/mods.toml"
        };
        zip_bytes(&[(name, text.as_bytes())])
    }

    /// One `[[dependencies.<owner>]]` table.
    pub fn toml_dep(owner: &str, id: &str, kind: &str, range: &str) -> String {
        let kind = match kind {
            "true" | "false" => format!("mandatory={kind}"),
            other => format!("type=\"{other}\""),
        };
        format!(
            "[[dependencies.{owner}]]\nmodId=\"{id}\"\n{kind}\nversionRange=\"{range}\"\nside=\"BOTH\"\n"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::zip_bytes;
    use super::test_jars::*;
    use super::*;

    #[test]
    fn fabric_details() {
        let jar = fabric_jar(
            "sodium",
            "0.5.3+mc1.20.1",
            r#""provides": ["rubidium", 7], "environment": "client",
               "depends": {"minecraft": "1.20.x", "fabricloader": ">=0.14", "fabric-api": "*",
                           "odd": ["1.0", "1.1"], "weird": 5},
               "breaks": {"optifabric": "*", "iris": "<1.6"}, "conflicts": {"bad": "*"},
               "recommends": {"rec": "*"},
               "mixins": ["sodium.mixins.json", {"config": "sodium-client.mixins.json", "environment": "client"}]"#,
        );
        let contents = scan_jar_bytes(&jar);
        assert_eq!(contents.state, JarState::Ok);
        assert_eq!(contents.descriptors, vec![DescriptorKind::Fabric]);
        let m = &contents.mods[0];
        assert_eq!(m.id, "sodium");
        assert_eq!(m.version.as_deref(), Some("0.5.3+mc1.20.1"));
        assert_eq!(m.provides, vec!["rubidium", "7"]);
        assert!(m.client);
        assert_eq!(
            m.mixin_configs,
            vec!["sodium.mixins.json", "sodium-client.mixins.json"]
        );
        let rel = |id: &str| m.relations.iter().find(|r| r.id == id).unwrap();
        assert_eq!(rel("minecraft").kind, RelationKind::Depends);
        assert_eq!(rel("minecraft").text.as_deref(), Some("1.20.x"));
        assert!(rel("fabric-api").req.is_any());
        assert_eq!(rel("fabric-api").text, None);
        assert_eq!(rel("odd").text.as_deref(), Some("1.0 || 1.1"));
        assert_eq!(rel("odd").req.matches("1.1"), Some(true));
        assert_eq!(rel("weird").req, VersionReq::Unknown);
        assert_eq!(rel("optifabric").kind, RelationKind::Breaks);
        assert_eq!(rel("iris").req.matches("1.5"), Some(true));
        assert_eq!(rel("bad").kind, RelationKind::Conflicts);
        assert_eq!(rel("rec").kind, RelationKind::Optional);
    }

    #[test]
    fn server_only_and_schema_zero() {
        let server = scan_jar_bytes(&fabric_jar("srv", "1.0", r#""environment": "server""#));
        assert!(!server.mods[0].client);
        let v0 = zip_bytes(&[(
            "fabric.mod.json",
            br#"{"id": "old", "version": "1.0", "requires": {"x": "1"}, "depends": {"y": "*"}, "mixins": {"client": ["old.client.json"], "common": ["old.json"]}}"#,
        )]);
        let v0 = scan_jar_bytes(&v0);
        assert!(v0.mods[0].relations.is_empty());
        assert_eq!(v0.mods[0].mixin_configs.len(), 2);
    }

    #[test]
    fn nested_jars_are_followed() {
        let module = fabric_jar(
            "fabric-api-base",
            "0.4.31",
            r#""mixins": ["fabric-api-base.mixins.json"]"#,
        );
        let deeper = fabric_jar("deep", "2.0", "");
        let middle = fabric_jar_with_nested("middle", "1.0", &[("deep.jar", deeper)]);
        let outer = fabric_jar_with_nested(
            "fabric-api",
            "0.92.0+1.20.1",
            &[
                ("base.jar", module),
                ("middle.jar", middle),
                ("missing.jar", Vec::new()),
            ],
        );
        let contents = scan_jar_bytes(&outer);
        let ids: Vec<&str> = contents.nested.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, vec!["fabric-api-base", "middle", "deep"]);
        assert_eq!(
            contents.nested[2].nested_in,
            vec!["META-INF/jars/middle.jar", "META-INF/jars/deep.jar"]
        );
        assert!(
            contents
                .mixin_configs()
                .any(|c| c == "fabric-api-base.mixins.json")
        );
    }

    #[test]
    fn quilt_details() {
        let json = br#"{
            "schema_version": 1,
            "quilt_loader": {
                "group": "org.example", "id": "qmod", "version": "1.0.0",
                "metadata": {"name": "Q Mod"},
                "provides": ["qalias", {"id": "org.example:other"}],
                "depends": ["qsl", {"id": "minecraft", "versions": ">=1.20"},
                            {"id": "opt", "optional": true}, {"id": "cond", "unless": "x"},
                            {"id": "bare", "versions": "1.0"}, [{"id": "alt1"}, {"id": "alt2"}]],
                "breaks": [{"id": "enemy", "versions": "<2.0"}, {"id": "maybe", "unless": "y"}],
                "jars": ["META-INF/jars/inner.jar"]
            },
            "mixin": ["qmod.mixins.json"],
            "minecraft": {"environment": "*"}
        }"#;
        let jar = zip_bytes(&[("quilt.mod.json", json)]);
        let contents = scan_jar_bytes(&jar);
        let m = &contents.mods[0];
        assert_eq!(m.descriptor, DescriptorKind::Quilt);
        assert_eq!(m.name.as_deref(), Some("Q Mod"));
        assert_eq!(m.provides, vec!["qalias", "other"]);
        assert_eq!(m.mixin_configs, vec!["qmod.mixins.json"]);
        let rel = |id: &str| m.relations.iter().find(|r| r.id == id);
        assert_eq!(rel("qsl").unwrap().kind, RelationKind::Depends);
        assert_eq!(rel("minecraft").unwrap().req.matches("1.20.1"), Some(true));
        assert_eq!(rel("opt").unwrap().kind, RelationKind::Optional);
        assert_eq!(rel("cond").unwrap().kind, RelationKind::Optional);
        assert_eq!(rel("bare").unwrap().req, VersionReq::Unknown);
        assert!(rel("alt1").is_none());
        assert_eq!(rel("enemy").unwrap().kind, RelationKind::Breaks);
        assert!(rel("maybe").is_none());
    }

    #[test]
    fn forge_and_neoforge_tomls() {
        let text = format!(
            "modLoader=\"javafml\"\n[[mods]]\nmodId=\"first\"\nversion=\"${{file.jarVersion}}\"\n[[mods]]\nmodId=\"second\"\nversion=\"2.0\"\n[[mods]]\nmodId=\"${{mod_id}}\"\n{}{}{}[[dependencies.first]]\nmodId=\"serveronly\"\nmandatory=true\nside=\"SERVER\"\n",
            toml_dep("first", "forge", "true", "[47.1,)"),
            toml_dep("first", "jei", "false", "[11,)"),
            toml_dep("second", "minecraft", "true", "[1.20.1,1.20.2)"),
        );
        let jar = zip_bytes(&[
            ("META-INF/MANIFEST.MF", b"Manifest-Version: 1.0\nImplementation-Version: 1.4.2\nMixinConfigs: first.mixins.json, first-client.mixins.json\n"),
            ("META-INF/mods.toml", text.as_bytes()),
            ("com/example/first/First.class", b"\xca\xfe"),
            ("com/example/first/deep/inner/More.class", b"\xca\xfe"),
            ("META-INF/versions/17/com/x/Y.class", b"\xca\xfe"),
        ]);
        let contents = scan_jar_bytes(&jar);
        let ids: Vec<&str> = contents.mods.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, vec!["first", "second"]);
        assert_eq!(contents.mods[0].version.as_deref(), Some("1.4.2"));
        assert_eq!(contents.mods[0].loader, LoaderKind::Forge);
        let first = &contents.mods[0].relations;
        assert_eq!(first.len(), 3);
        assert_eq!(first[0].kind, RelationKind::Depends);
        assert_eq!(first[0].req.matches("47.2.0"), Some(true));
        assert_eq!(first[1].kind, RelationKind::Optional);
        assert!(!first[2].client);
        assert_eq!(contents.mods[1].relations[0].id, "minecraft");
        assert_eq!(
            contents.manifest_mixins,
            vec!["first.mixins.json", "first-client.mixins.json"]
        );
        assert_eq!(
            contents.packages,
            vec!["com/example/first", "com/example/first/deep"]
        );

        let neo = format!(
            "[[mods]]\nmodId=\"neomod\"\nversion=\"3.0\"\n{}{}{}[[mixins]]\nconfig=\"neomod.mixins.json\"\n",
            toml_dep("neomod", "neoforge", "required", "[21.1,)"),
            toml_dep("neomod", "enemy", "incompatible", "[1.0,2.0)"),
            toml_dep("neomod", "meh", "discouraged", ""),
        );
        let jar = zip_bytes(&[("META-INF/neoforge.mods.toml", neo.as_bytes())]);
        let contents = scan_jar_bytes(&jar);
        let m = &contents.mods[0];
        assert_eq!(m.descriptor, DescriptorKind::NeoForgeToml);
        assert_eq!(m.loader, LoaderKind::NeoForge);
        assert_eq!(m.mixin_configs, vec!["neomod.mixins.json"]);
        assert_eq!(m.relations[1].kind, RelationKind::Breaks);
        assert_eq!(m.relations[2].kind, RelationKind::Conflicts);
        assert!(m.relations[2].req.is_any());

        // A 1.20.2–1.20.4 NeoForge mod still ships `mods.toml`.
        let jar = toml_jar(
            false,
            "oldneo",
            "1.0",
            &toml_dep("oldneo", "neoforge", "required", "[20.4,)"),
        );
        assert_eq!(scan_jar_bytes(&jar).mods[0].loader, LoaderKind::NeoForge);
    }

    #[test]
    fn jarjar_nested_forge_mods() {
        let inner = toml_jar(true, "lib", "1.2", "");
        let meta = br#"{"jars": [{"identifier": {"group": "g", "artifact": "lib"}, "version": {"range": "[1,)", "artifactVersion": "1.2"}, "path": "META-INF/jarjar/lib-1.2.jar", "isObfuscated": false}]}"#;
        let outer_toml = b"[[mods]]\nmodId=\"outer\"\nversion=\"1.0\"\n";
        let jar = zip_bytes(&[
            ("META-INF/neoforge.mods.toml", outer_toml),
            ("META-INF/jarjar/metadata.json", meta),
            ("META-INF/jarjar/lib-1.2.jar", &inner),
        ]);
        let contents = scan_jar_bytes(&jar);
        assert_eq!(contents.nested.len(), 1);
        assert_eq!(contents.nested[0].id, "lib");
        assert_eq!(
            contents.nested[0].nested_in,
            vec!["META-INF/jarjar/lib-1.2.jar"]
        );
    }

    #[test]
    fn broken_and_odd_files() {
        let not_zip = scan_jar_bytes(b"<html>404 Not Found</html>");
        assert!(matches!(
            not_zip.state,
            JarState::Corrupt { certain: true, .. }
        ));
        assert!(matches!(
            scan_jar_bytes(&[]).state,
            JarState::Corrupt { certain: true, .. }
        ));
        let mut truncated = fabric_jar("x", "1.0", "");
        truncated.truncate(truncated.len() / 2);
        assert!(matches!(
            scan_jar_bytes(&truncated).state,
            JarState::Corrupt { .. }
        ));

        let library = scan_jar_bytes(&zip_bytes(&[
            (
                "META-INF/MANIFEST.MF",
                b"Manifest-Version: 1.0\nFMLModType: LIBRARY\n",
            ),
            ("org/lib/Thing.class", b"x"),
        ]));
        assert_eq!(library.state, JarState::Ok);
        assert!(library.mods.is_empty() && library.descriptors.is_empty());
        assert_eq!(library.fml_mod_type.as_deref(), Some("LIBRARY"));

        let broken = scan_jar_bytes(&zip_bytes(&[("fabric.mod.json", b"{{{ nope")]));
        assert_eq!(broken.descriptors, vec![DescriptorKind::Fabric]);
        assert_eq!(broken.broken_descriptors, vec![DescriptorKind::Fabric]);
        let no_id = scan_jar_bytes(&zip_bytes(&[("fabric.mod.json", br#"{"version": "1"}"#)]));
        assert_eq!(no_id.broken_descriptors, vec![DescriptorKind::Fabric]);
        let wrong_types = scan_jar_bytes(&zip_bytes(&[(
            "fabric.mod.json",
            br#"{"schemaVersion": 1, "id": "w", "depends": ["a"], "provides": {"x": 1}, "jars": "no", "mixins": 5}"#,
        )]));
        assert_eq!(wrong_types.mods[0].id, "w");
        assert!(wrong_types.mods[0].relations.is_empty());

        // A self-nesting loop stops at the depth limit.
        let mut jar = fabric_jar("loop", "1.0", "");
        for _ in 0..6 {
            jar = fabric_jar_with_nested("loop", "1.0", &[("loop.jar", jar)]);
        }
        assert_eq!(scan_jar_bytes(&jar).nested.len(), MAX_NESTED_DEPTH);
    }

    #[test]
    fn names() {
        assert_eq!(strip_disabled("a.jar.DISABLED"), "a.jar");
        assert_eq!(strip_disabled("a.jar"), "a.jar");
        assert_eq!(file_stem("Sodium-0.5.jar.disabled"), "Sodium-0.5");
        assert_eq!(jar_name_state("x.JAR"), Some(true));
        assert_eq!(jar_name_state("x.jar.disabled"), Some(false));
        assert_eq!(jar_name_state(".hidden.jar"), None);
        assert_eq!(jar_name_state("x.zip"), None);
    }

    #[test]
    fn snapshot_lookups() {
        let forge = zip_bytes(&[
            (
                "META-INF/MANIFEST.MF",
                b"Manifest-Version: 1.0\nMixinConfigs: create.mixins.json\n",
            ),
            (
                "META-INF/mods.toml",
                b"[[mods]]\nmodId=\"create\"\ndisplayName=\"Create\"\nversion=\"0.5.1\"\n",
            ),
            ("com/simibubi/create/Create.class", b"x"),
            ("com/shared/lib/A.class", b"x"),
        ]);
        let other = zip_bytes(&[
            (
                "fabric.mod.json",
                fabric_json("other", "1.0", r#""mixins": ["mixins.other.json"]"#).as_bytes(),
            ),
            ("com/shared/lib/B.class", b"x"),
            ("net/minecraft/client/Hack.class", b"x"),
        ]);
        let snapshot = ModSnapshot::new(
            "mods",
            vec![
                ScannedJar::from_bytes("create.jar", &forge),
                ScannedJar::from_bytes("other.jar.disabled", &other),
            ],
        );
        assert_eq!(
            snapshot.by_file("other.jar").unwrap().file_name,
            "other.jar.disabled"
        );
        assert_eq!(
            snapshot
                .by_mixin_config("create.mixins.json")
                .unwrap()
                .0
                .file_name,
            "create.jar"
        );
        assert_eq!(
            snapshot
                .by_mixin_config("mixins.other.json")
                .unwrap()
                .1
                .unwrap()
                .id,
            "other"
        );
        assert_eq!(snapshot.by_name("create").unwrap().1.id, "create");
        assert_eq!(
            snapshot.by_name("CREATE").unwrap().0.file_name,
            "create.jar"
        );
        assert_eq!(
            snapshot
                .by_class("com.simibubi.create.Create")
                .unwrap()
                .file_name,
            "create.jar"
        );
        assert!(snapshot.by_class("net/minecraft/client/Hack").is_none());
        // Only enabled jars count, so the shared package resolves to the enabled one.
        assert_eq!(
            snapshot.by_class("com/shared/lib/A").unwrap().file_name,
            "create.jar"
        );
        assert_eq!(snapshot.declaring("other").count(), 1);
        assert_eq!(snapshot.disabled().count(), 1);
    }

    #[test]
    fn scanner_caches_by_size_and_time() {
        let dir = tempfile::tempdir().unwrap();
        let mods = dir.path().join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::write(mods.join("a.jar"), fabric_jar("a", "1.0", "")).unwrap();
        std::fs::write(mods.join("b.jar.disabled"), fabric_jar("b", "1.0", "")).unwrap();
        std::fs::write(mods.join("broken.jar"), b"not a zip").unwrap();
        std::fs::write(mods.join("notes.txt"), b"ignored").unwrap();
        std::fs::create_dir_all(mods.join("folder.jar")).unwrap();

        let scanner = ModScanner::new();
        let first = scanner.scan_dir(&mods);
        let names: Vec<&str> = first.jars.iter().map(|j| j.file_name.as_str()).collect();
        assert_eq!(names, vec!["a.jar", "b.jar.disabled", "broken.jar"]);
        assert!(!first.jars[1].enabled);
        assert_eq!(scanner.cached(), 3);

        let second = scanner.scan_dir(&mods);
        assert!(Arc::ptr_eq(
            &first.jars[0].contents,
            &second.jars[0].contents
        ));

        // Enabling keeps the cached contents (same size and time); rewriting drops them.
        std::fs::rename(mods.join("b.jar.disabled"), mods.join("b.jar")).unwrap();
        std::fs::write(
            mods.join("a.jar"),
            fabric_jar("a", "2.0", r#""name": "A two""#),
        )
        .unwrap();
        let third = scanner.scan_dir(&mods);
        assert!(third.jars[1].enabled);
        assert!(Arc::ptr_eq(
            &first.jars[1].contents,
            &third.jars[1].contents
        ));
        assert_eq!(
            third.jars[0].contents.mods[0].version.as_deref(),
            Some("2.0")
        );

        std::fs::remove_file(mods.join("broken.jar")).unwrap();
        let fourth = scanner.scan_dir(&mods);
        assert_eq!(fourth.jars.len(), 2);
        assert_eq!(scanner.cached(), 2);

        assert!(
            scanner
                .scan_dir(&dir.path().join("missing"))
                .jars
                .is_empty()
        );
    }
}

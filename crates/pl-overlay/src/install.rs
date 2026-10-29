//! The overlay's mod in a game folder (OVL-B). The launcher ships one jar per Minecraft version the
//! mod supports (the `overlay/` folder of its resources, described by [`INDEX_FILE`]: a [`Bundle`])
//! and keeps the matching one in every Fabric instance it starts while "Oyun içi katman" is on.
//! [`reconcile`] runs right before a start: after the official pack's sync, before the mod check.
//!
//! - **Ours** is a file our record (`<game dir>/.projects/overlay-mod.json`) lists with the SHA-1 we
//!   wrote, enabled or `.disabled`, or a shipped jar's exact copy under its own name (a lost record).
//!   A file the player changed leaves the record: it is theirs, never touched.
//! - **One copy.** An enabled overlay jar that is not ours (an older launcher's pack copy, one the
//!   player put there) is kept and ours goes: Fabric refuses two copies of a mod. The bridge still
//!   opens, and nothing is said.
//! - **Never a missing dependency.** The wanted jar enters the folder only when the launcher's own
//!   pre-launch mod check ([`pl_mc::mods::check`]) finds nothing about it in the folder as it would
//!   be. When only Fabric API is missing (and the player did not switch it off) the answer is
//!   [`Outcome::NeedsFabricApi`]: the caller installs it the way the launcher installs mods and asks
//!   again with [`Request::ask_fabric_api`] off.
//! - **The switch is the one control**, as it was for the pack's item (OVL2): switched off, not a
//!   Fabric instance, or a Minecraft version without a jar, and ours goes (its disabled copies too).
//! - Version switches park `mods/` with our jar in it; the record keeps those entries (they name
//!   their Minecraft version) and drops only the current version's entries whose file is gone.
//! - **Jars from the signed content** (OVL-AUTO, [`ContentJar`]) cover the Minecraft versions the
//!   launcher's own jars leave out ([`Jars`]: its own jar wins wherever it has one). Only a jar of
//!   this launcher's bridge protocol whose bytes match the signed entry (size, SHA-256 and SHA-1)
//!   goes in; the caller fetches it first ([`Request::fetched`]).

use std::cmp::Ordering;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use pl_content::{OverlayJar, OverlayRelease, compare_minecraft_versions};
use pl_mc::loaders::LoaderKind;
use pl_mc::mods::check::{
    CheckEnvironment, FileOrigins, IssueKind, ModCheckReport, Severity, check_mods,
};
use pl_mc::mods::scan::{ModScanner, ModSnapshot, ScannedJar};
use pl_mc::mods::version::{MinecraftVersion, VersionReq};
use pl_net::HashAlgo;
use serde::{Deserialize, Serialize};

/// The mod's id in its `fabric.mod.json`.
pub const MOD_ID: &str = "projects_overlay";
/// The folder of the launcher's resources that holds the jars.
pub const BUNDLE_DIR: &str = "overlay";
/// What [`Bundle::load`] reads in that folder (written by `scripts/release/overlay-jars.mjs`).
pub const INDEX_FILE: &str = "index.json";
/// Our record, in the game folder's `.projects/` (next to the pack state; never parked).
pub const RECORD_FILE: &str = "overlay-mod.json";
const RECORD_DIR: &str = ".projects";
const INDEX_SCHEMA: u32 = 1;
const MAX_INDEX_BYTES: u64 = 64 * 1024;
const MAX_JAR_BYTES: u64 = 32 * 1024 * 1024;

/// [`INDEX_FILE`]: the jars of a launcher build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleIndex {
    pub schema: u32,
    pub jars: Vec<BundledJar>,
}

/// One shipped jar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BundledJar {
    /// The Minecraft version it is built for (`26.2`).
    pub minecraft: String,
    /// The versions it accepts in Fabric's syntax (`~26.2`), as its `fabric.mod.json` says.
    pub range: String,
    /// Its file name, in the bundle and in `mods/`.
    pub file: String,
    /// The mod's version (`0.1.0+26.2`).
    pub version: String,
    /// Lowercase hex.
    pub sha1: String,
    pub size: u64,
}

/// The jars this launcher ships ([`BUNDLE_DIR`] of its resources).
#[derive(Debug, Clone)]
pub struct Bundle {
    dir: PathBuf,
    jars: Vec<BundledJar>,
}

impl Bundle {
    /// Reads `<dir>/index.json`. `None` without an index, with one that cannot be read, or when
    /// none of its jars is there (a developer build without them): the launcher has no overlay
    /// mod of its own then, and the official pack's item stands in.
    pub fn load(dir: &Path) -> Option<Self> {
        let path = dir.join(INDEX_FILE);
        let meta = std::fs::metadata(&path).ok()?;
        if meta.len() > MAX_INDEX_BYTES {
            tracing::warn!(path = %path.display(), "the overlay's jar index is too large; ignored");
            return None;
        }
        let index: BundleIndex = match std::fs::read(&path)
            .map_err(|error| error.to_string())
            .and_then(|bytes| serde_json::from_slice(&bytes).map_err(|error| error.to_string()))
        {
            Ok(index) => index,
            Err(error) => {
                tracing::warn!(%error, "the overlay's jar index is unreadable; ignored");
                return None;
            }
        };
        if index.schema != INDEX_SCHEMA {
            tracing::warn!(
                schema = index.schema,
                "the overlay's jar index is of another schema"
            );
            return None;
        }
        let jars: Vec<BundledJar> = index
            .jars
            .into_iter()
            .filter(|jar| {
                let fine = valid_jar(jar)
                    && std::fs::metadata(dir.join(&jar.file))
                        .is_ok_and(|meta| meta.is_file() && meta.len() == jar.size);
                if !fine {
                    tracing::warn!(file = %jar.file, "an overlay jar of the index is missing or odd");
                }
                fine
            })
            .collect();
        (!jars.is_empty()).then(|| Self {
            dir: dir.to_path_buf(),
            jars,
        })
    }

    pub fn jars(&self) -> &[BundledJar] {
        &self.jars
    }

    /// The Minecraft versions it has a jar for, in the index's order.
    pub fn versions(&self) -> Vec<String> {
        self.jars.iter().map(|jar| jar.minecraft.clone()).collect()
    }

    /// The jar for an instance on Minecraft `minecraft` (`26.1.3` takes the `~26.1.2` one).
    pub fn jar_for(&self, minecraft: &str) -> Option<&BundledJar> {
        let version = MinecraftVersion::parse(minecraft);
        self.jars
            .iter()
            .find(|jar| VersionReq::fabric(&jar.range).matches_minecraft(&version) == Some(true))
    }

    pub fn path(&self, jar: &BundledJar) -> PathBuf {
        self.dir.join(&jar.file)
    }

    /// A shipped jar's exact copy under its own name.
    fn ships(&self, file: &str, sha1: &str) -> bool {
        self.jars
            .iter()
            .any(|jar| jar.file.eq_ignore_ascii_case(file) && jar.sha1 == sha1)
    }
}

/// A jar of the signed content's `overlay` (OVL-AUTO) this launcher may use: it keeps the format's
/// rules, speaks this launcher's bridge protocol ([`crate::protocol::PROTO`]) and its range, which
/// the launcher can read, accepts the version it was built for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentJar {
    /// The Minecraft version CI built and tested it for (`26.4`).
    pub minecraft: String,
    /// The versions it accepts, in Fabric's syntax (`~26.4`).
    pub range: String,
    /// Its file name in `mods/` (`projects-overlay-0.1.0+26.4.jar`).
    pub file: String,
    /// Lowercase hex.
    pub sha1: String,
    /// Lowercase hex.
    pub sha256: String,
    pub size: u64,
    /// The signed entry, for the download.
    pub entry: OverlayJar,
}

impl ContentJar {
    /// The jars of `release` a launcher of bridge protocol `proto` may use, in the published
    /// order. The others are left out (and logged): another protocol, a broken rule, a range it
    /// cannot read or that does not accept the jar's own version, a file name it would not write.
    pub fn usable(release: &OverlayRelease, proto: u32) -> Vec<Self> {
        let mut usable = Vec::new();
        for entry in release.usable(proto) {
            let (Some(sha1), Some(sha256)) = (entry.sha1(), entry.sha256()) else {
                continue;
            };
            let version = MinecraftVersion::parse(&entry.minecraft);
            let jar = Self {
                minecraft: entry.minecraft.clone(),
                range: entry.range.clone(),
                file: entry.file_name(),
                sha1,
                sha256,
                size: entry.file.size,
                entry: entry.clone(),
            };
            let fits = VersionReq::fabric(&jar.range).matches_minecraft(&version) == Some(true);
            if fits && valid_file_name(&jar.file) {
                usable.push(jar);
            } else {
                tracing::warn!(minecraft = %entry.minecraft, "a published overlay jar's range does not take its own version; left out");
            }
        }
        let refused = release.jars.len() - usable.len();
        if refused > 0 {
            tracing::info!(
                refused,
                "published overlay jars of another bridge protocol or breaking a rule were left out"
            );
        }
        usable
    }
}

/// Every jar a start may get: the launcher's own first, then the signed content's for the
/// versions its own leave out (OVL-AUTO).
#[derive(Debug, Clone, Copy, Default)]
pub struct Jars<'a> {
    pub bundle: Option<&'a Bundle>,
    pub content: &'a [ContentJar],
}

/// The jar a start of one Minecraft version gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick<'a> {
    /// The launcher's own.
    Bundled(&'a BundledJar),
    /// The signed content's; the caller fetches it before the start ([`Request::fetched`]).
    Content(&'a ContentJar),
}

impl Pick<'_> {
    /// Its file name in `mods/`.
    pub fn file(&self) -> &str {
        match self {
            Self::Bundled(jar) => &jar.file,
            Self::Content(jar) => &jar.file,
        }
    }
}

impl<'a> Jars<'a> {
    /// The launcher's own jars alone.
    pub fn bundled(bundle: Option<&'a Bundle>) -> Self {
        Self {
            bundle,
            content: &[],
        }
    }

    /// The jar for an instance on Minecraft `minecraft`: the launcher's own when it has one; else,
    /// of the content's jars whose range accepts it, the one built for the highest version not
    /// above it.
    pub fn jar_for(&self, minecraft: &str) -> Option<Pick<'a>> {
        if let Some(jar) = self.bundle.and_then(|bundle| bundle.jar_for(minecraft)) {
            return Some(Pick::Bundled(jar));
        }
        let version = MinecraftVersion::parse(minecraft);
        self.content
            .iter()
            .filter(|jar| VersionReq::fabric(&jar.range).matches_minecraft(&version) == Some(true))
            .filter(|jar| {
                compare_minecraft_versions(&jar.minecraft, minecraft) != Ordering::Greater
            })
            .max_by(|a, b| compare_minecraft_versions(&a.minecraft, &b.minecraft))
            .map(Pick::Content)
    }

    /// The Minecraft versions some jar is for: the launcher's own, then the content's it lacks.
    pub fn versions(&self) -> Vec<String> {
        let mut versions = self.bundle.map(Bundle::versions).unwrap_or_default();
        for jar in self.content {
            if !versions.contains(&jar.minecraft) {
                versions.push(jar.minecraft.clone());
            }
        }
        versions
    }

    /// Whether any jar at all is there (the launcher has an overlay mod of its own).
    pub fn any(&self) -> bool {
        self.bundle.is_some() || !self.content.is_empty()
    }

    /// An exact copy of one of the jars under its own name.
    fn ships(&self, file: &str, sha1: &str) -> bool {
        self.bundle.is_some_and(|bundle| bundle.ships(file, sha1))
            || self
                .content
                .iter()
                .any(|jar| jar.file.eq_ignore_ascii_case(file) && jar.sha1 == sha1)
    }
}

/// A jar name the launcher may write into `mods/`.
fn valid_file_name(file: &str) -> bool {
    !file.is_empty()
        && file.len() <= 128
        && file.to_ascii_lowercase().ends_with(".jar")
        && !file.starts_with('.')
        && file
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '+'))
}

/// A jar name the launcher may write into `mods/` and a well-formed entry.
fn valid_jar(jar: &BundledJar) -> bool {
    valid_file_name(&jar.file)
        && jar.size > 0
        && jar.size <= MAX_JAR_BYTES
        && jar.sha1.len() == 40
        && jar
            .sha1
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        && !jar.minecraft.trim().is_empty()
        && !jar.range.trim().is_empty()
}

/// Our record: the jars we wrote into this game folder's `mods/` (any version's set).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ModRecord {
    /// File name (without `.disabled`) → what we wrote.
    pub files: BTreeMap<String, RecordedJar>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordedJar {
    /// Lowercase hex.
    pub sha1: String,
    /// The version whose `mods/` it went into.
    pub minecraft: String,
}

impl ModRecord {
    pub fn path(game_dir: &Path) -> PathBuf {
        game_dir.join(RECORD_DIR).join(RECORD_FILE)
    }

    /// The record of `game_dir` (empty when there is none or it cannot be read).
    pub fn load_blocking(game_dir: &Path) -> Self {
        std::fs::read(Self::path(game_dir))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    async fn save(&self, game_dir: &Path) {
        let saved = serde_json::to_vec_pretty(self)
            .map_err(|error| error.to_string())
            .map(|bytes| (Self::path(game_dir), bytes));
        let result = match saved {
            Ok((path, bytes)) => pl_mc::paths::write_atomic(&path, &bytes)
                .await
                .map_err(|error| error.to_string()),
            Err(error) => Err(error),
        };
        if let Err(error) = result {
            tracing::warn!(%error, "the overlay mod's record was not saved");
        }
    }
}

/// The overlay jars the launcher put in `game_dir` (any version's set), by lowercase file name
/// without `.disabled`: lists of the player's own mods leave them out, "Bu modu kapat" on one
/// switches the overlay off. Reads one small file.
pub fn owned_files(game_dir: &Path) -> HashSet<String> {
    ModRecord::load_blocking(game_dir)
        .files
        .into_keys()
        .map(|name| name.to_ascii_lowercase())
        .collect()
}

/// Whether `file_name` (with or without `.disabled`) is in `owned` ([`owned_files`]).
pub fn is_owned(owned: &HashSet<String>, file_name: &str) -> bool {
    let base = pl_mc::mods::scan::strip_disabled(file_name);
    owned.contains(&base.to_ascii_lowercase())
}

/// One start's question to [`reconcile`].
#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    pub game_dir: &'a Path,
    /// The jars a start may get: the launcher's own (none in a build without them), then the
    /// signed content's.
    pub jars: Jars<'a>,
    /// The local copy of the content's jar [`Jars::jar_for`] picks for this version, which the
    /// caller fetched; `None` when it could not be fetched now (or the pick is the launcher's own).
    /// It is checked again against the signed entry before it goes in.
    pub fetched: Option<&'a Path>,
    /// The instance as the pre-launch mod check sees it (version, loader, Java).
    pub env: &'a CheckEnvironment,
    /// "Oyun içi katman".
    pub enabled: bool,
    /// [`Outcome::NeedsFabricApi`] may be the answer; off for the second try, once the caller
    /// tried to install Fabric API.
    pub ask_fabric_api: bool,
}

/// What a game start gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// An enabled overlay mod is in the folder (the bridge opens): ours, or a copy someone else
    /// put there.
    Ready { ours: bool },
    /// The jar would miss only Fabric API, which the folder lacks: install it, then ask again.
    /// Nothing changed in the folder.
    NeedsFabricApi,
    /// No overlay for this start; ours is not in the folder.
    Off(OffReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OffReason {
    /// "Oyun içi katman" is off.
    SwitchedOff,
    /// Not a Fabric instance.
    NotFabric,
    /// The launcher has no jar for this Minecraft version (or none at all).
    NoJar,
    /// The signed content's jar for this version could not be fetched now (OVL-AUTO); the next
    /// start tries again.
    NotFetched,
    /// Fabric API is there, switched off by the player (left alone).
    FabricApiOff,
    /// The mod check finds something about the jar in this folder (Fabric API still missing, the
    /// loader, Java, another mod that breaks it).
    Incompatible,
    /// The jar could not be read or written.
    Failed,
}

/// An overlay jar of the folder that is ours.
#[derive(Debug)]
struct OurJar {
    base: String,
    path: PathBuf,
    enabled: bool,
    sha1: String,
}

/// Puts the right overlay jar in place for a start, or takes ours away (module docs). Never
/// fails: a problem means a start without the overlay.
pub async fn reconcile(request: Request<'_>) -> Outcome {
    let Request {
        game_dir,
        jars,
        fetched,
        env,
        enabled,
        ask_fabric_api,
    } = request;
    let mods_dir = game_dir.join("mods");
    let snapshot = scan(&mods_dir).await;
    let mut record = ModRecord::load_blocking(game_dir);
    let before = record.clone();

    // The overlay jars in the folder: ours, and whether an enabled one of someone else's is there.
    let mut ours: Vec<OurJar> = Vec::new();
    let mut theirs: Vec<String> = Vec::new();
    let mut others = false;
    for jar in snapshot.jars.iter().filter(|jar| declares_overlay(jar)) {
        let base = jar.base_name().to_owned();
        let Ok(sha1) = pl_net::hash_file(&jar.path, HashAlgo::Sha1).await else {
            // Unreadable now (another program holds it): left alone, record and all.
            others |= jar.enabled;
            continue;
        };
        let mine = record
            .files
            .get(&base)
            .is_some_and(|entry| entry.sha1 == sha1)
            || jars.ships(&base, &sha1);
        if mine {
            ours.push(OurJar {
                base,
                path: jar.path.clone(),
                enabled: jar.enabled,
                sha1,
            });
        } else {
            theirs.push(base);
            others |= jar.enabled;
        }
    }
    // A name of ours that now holds only someone else's file (changed by the player): theirs.
    for base in &theirs {
        if !ours.iter().any(|our| our.base == *base) {
            record.files.remove(base);
        }
    }
    // This version's entries whose file is gone (deleted, or replaced by a sync of ours).
    record.files.retain(|name, entry| {
        entry.minecraft != env.minecraft || ours.iter().any(|jar| jar.base == *name)
    });

    let wanted = if !enabled {
        Err(OffReason::SwitchedOff)
    } else if env.loader != LoaderKind::Fabric {
        Err(OffReason::NotFabric)
    } else {
        match jars.jar_for(&env.minecraft) {
            None => Err(OffReason::NoJar),
            Some(Pick::Bundled(jar)) => jars
                .bundle
                .map(|bundle| (bundle.path(jar), Expected::Bundled(jar)))
                .ok_or(OffReason::NoJar),
            Some(Pick::Content(jar)) => match fetched {
                Some(path) => Ok((path.to_path_buf(), Expected::Content(jar))),
                // Not fetched now (offline): the copy of ours already in place serves only when it
                // passes the whole check against the signed entry right now; else nothing does.
                None => {
                    let in_place = ours
                        .iter()
                        .find(|our| our.enabled && our.base.eq_ignore_ascii_case(&jar.file));
                    match in_place {
                        Some(our) if file_matches(&our.path, Expected::Content(jar)).await => {
                            Ok((our.path.clone(), Expected::Content(jar)))
                        }
                        _ => Err(OffReason::NotFetched),
                    }
                }
            },
        }
    };
    let outcome = match wanted {
        Err(reason) => {
            remove(&ours, &mut record, |_| true).await;
            // A copy someone else put in a Fabric instance still gets its bridge (an older
            // launcher's pack copy on a version this build has no jar for).
            if enabled && others && env.loader == LoaderKind::Fabric {
                Outcome::Ready { ours: false }
            } else {
                Outcome::Off(reason)
            }
        }
        Ok(_) if others => {
            remove(&ours, &mut record, |_| true).await;
            Outcome::Ready { ours: false }
        }
        Ok((source, expected)) => {
            place(
                &source,
                expected,
                Place {
                    mods_dir: &mods_dir,
                    snapshot: &snapshot,
                    env,
                    ask_fabric_api,
                },
                &ours,
                &mut record,
            )
            .await
        }
    };
    if record != before {
        record.save(game_dir).await;
    }
    tracing::info!(?outcome, minecraft = %env.minecraft, "in-game overlay mod");
    outcome
}

struct Place<'a> {
    mods_dir: &'a Path,
    snapshot: &'a ModSnapshot,
    env: &'a CheckEnvironment,
    ask_fabric_api: bool,
}

/// What the wanted jar's bytes must be.
#[derive(Debug, Clone, Copy)]
enum Expected<'a> {
    /// The launcher's own: its size and SHA-1 in the shipped index.
    Bundled(&'a BundledJar),
    /// The signed content's: its size, SHA-256 and SHA-1 in the signed index.
    Content(&'a ContentJar),
}

impl Expected<'_> {
    fn file(&self) -> &str {
        match self {
            Self::Bundled(jar) => &jar.file,
            Self::Content(jar) => &jar.file,
        }
    }

    fn sha1(&self) -> &str {
        match self {
            Self::Bundled(jar) => &jar.sha1,
            Self::Content(jar) => &jar.sha1,
        }
    }

    /// Whether `bytes` are the jar, every hash the index has for it included.
    fn matches(&self, bytes: &[u8]) -> bool {
        let sha1 = || pl_net::hash_bytes(bytes, HashAlgo::Sha1);
        match self {
            Self::Bundled(jar) => bytes.len() as u64 == jar.size && sha1() == jar.sha1,
            Self::Content(jar) => {
                bytes.len() as u64 == jar.size
                    && pl_net::hash_bytes(bytes, HashAlgo::Sha256) == jar.sha256
                    && sha1() == jar.sha1
            }
        }
    }
}

/// Whether the file at `path` is the jar `expected` describes (every hash its index has).
async fn file_matches(path: &Path, expected: Expected<'_>) -> bool {
    tokio::fs::read(path)
        .await
        .is_ok_and(|bytes| expected.matches(&bytes))
}

/// The wanted jar (its bytes at `source`): checked against its index, then in the folder as it
/// would be, then written (unless it is there).
async fn place(
    source: &Path,
    jar: Expected<'_>,
    at: Place<'_>,
    ours: &[OurJar],
    record: &mut ModRecord,
) -> Outcome {
    let file = jar.file().to_owned();
    let bytes = match tokio::fs::read(source).await {
        Ok(bytes) if jar.matches(&bytes) => bytes,
        Ok(_) => {
            tracing::warn!(%file, "an overlay jar does not match its index; not used");
            remove(ours, record, |_| true).await;
            return Outcome::Off(OffReason::Failed);
        }
        Err(error) => {
            tracing::warn!(%error, %file, "an overlay jar could not be read");
            remove(ours, record, |_| true).await;
            return Outcome::Off(OffReason::Failed);
        }
    };
    let candidate = ScannedJar::from_bytes(&file, &bytes);
    let mut jars: Vec<ScannedJar> = at
        .snapshot
        .jars
        .iter()
        .filter(|scanned| !ours.iter().any(|our| our.path == scanned.path))
        .cloned()
        .collect();
    // A file of someone else's with the jar's very name is never overwritten.
    if jars
        .iter()
        .any(|scanned| scanned.file_name.eq_ignore_ascii_case(&file))
    {
        tracing::warn!(%file, "a file of the overlay jar's name is in the way");
        remove(ours, record, |_| true).await;
        return Outcome::Off(OffReason::Failed);
    }
    jars.push(candidate);
    let report = check_mods(
        &ModSnapshot::new(at.mods_dir, jars),
        at.env,
        &FileOrigins::default(),
    );
    match verdict(&report, &file) {
        Verdict::Clean => {}
        Verdict::NeedsFabricApi if at.ask_fabric_api => return Outcome::NeedsFabricApi,
        Verdict::NeedsFabricApi | Verdict::Blocked => {
            remove(ours, record, |_| true).await;
            return Outcome::Off(OffReason::Incompatible);
        }
        Verdict::FabricApiOff => {
            remove(ours, record, |_| true).await;
            return Outcome::Off(OffReason::FabricApiOff);
        }
    }

    // The enabled file of ours under the jar's name is the jar (or is overwritten with it).
    let in_place = |our: &OurJar| our.enabled && our.base.eq_ignore_ascii_case(&file);
    // A copy already in place stays only when it is the jar: for the signed content's, its size and
    // SHA-256 too, not its SHA-1 alone (else the checked bytes are written over it).
    let mut kept = false;
    if let Some(our) = ours
        .iter()
        .find(|our| in_place(our) && our.sha1 == jar.sha1())
    {
        kept = match jar {
            Expected::Bundled(_) => true,
            Expected::Content(_) => file_matches(&our.path, jar).await,
        };
    }
    if !kept {
        let target = at.mods_dir.join(&file);
        if let Err(error) = pl_mc::paths::write_atomic(&target, &bytes).await {
            tracing::warn!(%error, "the overlay jar could not be written");
            // A copy of ours under the name is an older jar then: everything of ours goes.
            remove(ours, record, |_| true).await;
            return Outcome::Off(OffReason::Failed);
        }
    }
    remove(ours, record, |our| !in_place(our)).await;
    record.files.insert(
        file,
        RecordedJar {
            sha1: jar.sha1().to_owned(),
            minecraft: at.env.minecraft.clone(),
        },
    );
    Outcome::Ready { ours: true }
}

/// What the mod check says about the jar named `file`.
#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    Clean,
    /// Only Fabric API modules are missing, and no disabled copy has them.
    NeedsFabricApi,
    /// A Fabric API module the jar needs is in a jar the player switched off.
    FabricApiOff,
    Blocked,
}

fn verdict(report: &ModCheckReport, file: &str) -> Verdict {
    let mut needs_api = false;
    for issue in &report.issues {
        let about = issue.mods.iter().any(|named| named.file == file)
            || issue
                .target
                .as_ref()
                .is_some_and(|target| target.file.as_deref() == Some(file));
        if !about || issue.severity == Severity::Info {
            continue;
        }
        if issue.kind != IssueKind::MissingDependency {
            return Verdict::Blocked;
        }
        let Some(target) = &issue.target else {
            return Verdict::Blocked;
        };
        if !is_fabric_api_module(&target.id) {
            return Verdict::Blocked;
        }
        if target.file.is_some() {
            // The check names the disabled jar that has it.
            return Verdict::FabricApiOff;
        }
        needs_api = true;
    }
    if needs_api {
        Verdict::NeedsFabricApi
    } else {
        Verdict::Clean
    }
}

/// Fabric API itself or one of its modules (`fabric-key-mapping-api-v1`, `fabric-api-base`).
fn is_fabric_api_module(id: &str) -> bool {
    id == "fabric-api" || (id.starts_with("fabric-") && id != "fabric-loader")
}

fn declares_overlay(jar: &ScannedJar) -> bool {
    jar.contents.mods.iter().any(|m| m.id == MOD_ID)
}

/// Deletes the jars of ours `which` picks (enabled or disabled copies) and forgets them. A file
/// that cannot be deleted stays recorded; the next start tries again.
async fn remove(ours: &[OurJar], record: &mut ModRecord, which: impl Fn(&OurJar) -> bool) {
    for our in ours.iter().filter(|our| which(our)) {
        let gone = match tokio::fs::remove_file(&our.path).await {
            Ok(()) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
            Err(error) => {
                tracing::warn!(%error, path = %our.path.display(), "an overlay jar could not be removed");
                false
            }
        };
        // The entry goes with the last copy it describes.
        let kept = ours
            .iter()
            .any(|other| other.base == our.base && other.path != our.path && !which(other));
        if gone
            && !kept
            && record
                .files
                .get(&our.base)
                .is_some_and(|entry| entry.sha1 == our.sha1)
        {
            record.files.remove(&our.base);
        }
    }
}

async fn scan(mods_dir: &Path) -> ModSnapshot {
    let dir = mods_dir.to_path_buf();
    tokio::task::spawn_blocking(move || ModScanner::shared().scan_dir(&dir))
        .await
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;

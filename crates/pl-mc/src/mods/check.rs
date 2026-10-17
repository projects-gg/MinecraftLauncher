//! Pre-launch mod check. Reads the jars of a mods folder (through the cached [`ModScanner`])
//! and reports, before the game starts, what would stop it or keep a mod from loading:
//!
//! - the same mod twice (two jars with one id, counting provided ids; a separate copy older
//!   than the copy another jar bundles),
//! - missing required dependencies, and dependencies installed in a version the requiring mod
//!   does not accept,
//! - mods made for another Minecraft version, another mod loader version or a newer Java,
//! - mods made for another loader (a Forge jar in a Fabric instance and the other way round),
//! - mods that declare themselves incompatible with another installed mod,
//! - files that are not readable jars, jars whose mod description cannot be read, and jars
//!   without any mod description (information only).
//!
//! The check is conservative on purpose: a version or requirement that cannot be read with
//! certainty is never reported, and only what the loader certainly refuses is
//! [`Severity::Blocking`]. Each issue names the mods by display name and file and carries
//! machine-readable fixes ([`ModFix`]); [`super::repair`] turns them into a repair plan.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::scan::{
    DeclaredMod, DescriptorKind, JarContents, JarState, ModScanner, ModSnapshot, Relation,
    RelationKind, ScannedJar,
};
use super::version::{
    FabricVersion, MavenVersion, MinecraftVersion, Mismatch, VersionReq, minimum_java,
};
use crate::error::Result;
use crate::gamefiles::fsops::blocking;
use crate::loaders::LoaderKind;

/// The instance the mods are checked for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CheckEnvironment {
    /// Minecraft version id (`1.21.1`, `26.1`, `1.21-pre1`).
    pub minecraft: String,
    pub loader: LoaderKind,
    /// Installed loader version (`0.16.10`, `47.4.10`, `21.1.77`), when known.
    pub loader_version: Option<String>,
    /// Major version of the Java that will run the game, when known.
    pub java_major: Option<u32>,
}

/// Where a mod file came from. Files without an entry were added by hand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FileOrigin {
    /// Installed by the official pack sync; `path` is the pack path (`mods/sodium.jar`).
    Pack { path: String },
    /// Installed from Modrinth by the player.
    Modrinth {
        project_id: String,
        version_id: Option<String>,
    },
}

/// File name (without `.disabled`) → origin.
pub type FileOrigins = HashMap<String, FileOrigin>;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    /// The game will certainly fail to start.
    Blocking,
    /// The game starts, but something will not work (a mod will not load).
    Warning,
    /// Worth knowing; nothing has to be done.
    Info,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "camelCase")]
pub enum IssueKind {
    /// The file is not a readable jar (an interrupted download, an empty file).
    Corrupt,
    /// The file could not be opened (locked by another program, no permission).
    Unreadable,
    /// A mod uses an id the loader itself provides (`minecraft`, `fabricloader`, ...).
    ProvidedByLoader,
    /// The same mod is installed more than once.
    Duplicate,
    /// A mod needs another mod that is not installed.
    MissingDependency,
    /// A mod needs another mod in a version other than the installed one.
    DependencyVersion,
    /// A mod does not support this Minecraft version.
    IncompatibleMinecraft,
    /// A mod needs another mod loader version.
    IncompatibleLoader,
    /// A mod needs a newer Java.
    JavaVersion,
    /// A mod is made for another mod loader.
    WrongLoader,
    /// A mod declares itself incompatible with another installed mod.
    Breaks,
    /// A mod advises against another installed mod (the loader only warns).
    Conflicts,
    /// The jar's mod description cannot be read.
    BrokenMetadata,
    /// A jar without a mod description (a library, or not a mod for this loader).
    NoMetadata,
}

/// A mod named by an issue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModRef {
    /// File name in the mods folder (the outer jar for bundled mods).
    pub file: String,
    pub mod_id: Option<String>,
    /// The mod's own name, else its id, else the file name without extension.
    pub name: String,
    pub version: Option<String>,
    /// Declared inside another jar (jar-in-jar).
    pub bundled: bool,
}

/// The other side of an issue: the dependency, the platform, the incompatible mod.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct IssueTarget {
    /// A mod id, `minecraft`, `java`, a loader id (`fabricloader`, `forge`, `neoforge`), or for
    /// [`IssueKind::WrongLoader`] the loader the jar is made for (`forge`, `fabric`, ...).
    pub id: String,
    pub name: Option<String>,
    /// What is required, as written (`>=0.5.0`, `[1.20.1,1.21)`).
    pub requirement: Option<String>,
    /// What is installed (a version, or the instance's loader).
    pub found: Option<String>,
    /// The file that holds it, when installed.
    pub file: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModIssue {
    pub severity: Severity,
    pub kind: IssueKind,
    /// The mods the issue is about (every copy for duplicates, the requiring mod(s) for
    /// dependency problems, both sides for incompatibilities).
    pub mods: Vec<ModRef>,
    pub target: Option<IssueTarget>,
    /// Ways to fix it, preferred first. Empty when the player has to act (close a program).
    pub fixes: Vec<ModFix>,
}

/// Why a file is turned off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum DisableReason {
    Duplicate,
    WrongLoader,
    IncompatibleMinecraft,
    Breaks,
    Corrupt,
    ProvidedByLoader,
    /// Blamed for a crash.
    Crash,
}

/// A machine-readable fix. Phase two resolves the network parts (Modrinth) and runs it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ModFix {
    /// Download a mod another mod needs.
    FetchDependency {
        mod_id: String,
        /// Accepted versions in the loader's syntax (`>=0.5`, `[1,2)`); `None` = any.
        version_req: Option<String>,
        loader: LoaderKind,
        minecraft: String,
        /// Ids of the mods that need it.
        required_by: Vec<String>,
    },
    /// Turn a disabled jar that provides a needed mod back on.
    EnableFile { file: String, mod_id: String },
    /// Replace the file with a version of the same mod that fits this Minecraft version, the
    /// loader and `version_req`. When none exists and `disable_if_unavailable` is set, turn the
    /// file off instead.
    UpdateToCompatible {
        file: String,
        mod_id: String,
        version_req: Option<String>,
        disable_if_unavailable: bool,
    },
    /// Rename `x.jar` to `x.jar.disabled`.
    DisableFile { file: String, reason: DisableReason },
    /// Download the official pack's copy of the file again (`path` is the pack path).
    RedownloadPackFile { path: String },
    /// Download the Modrinth version the file came from again.
    RedownloadModrinthFile {
        file: String,
        project_id: String,
        version_id: Option<String>,
    },
    /// Install a loader version that accepts `version_req`.
    UpdateLoader {
        loader: LoaderKind,
        version_req: String,
    },
    /// Run the game with at least this Java version.
    UseJava { major: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModCheckReport {
    /// Most severe first.
    pub issues: Vec<ModIssue>,
    /// Enabled jars looked at.
    pub checked_files: u32,
}

impl ModCheckReport {
    pub fn has_blocking(&self) -> bool {
        self.issues.iter().any(|i| i.severity == Severity::Blocking)
    }

    pub fn blocking(&self) -> impl Iterator<Item = &ModIssue> {
        self.issues
            .iter()
            .filter(|i| i.severity == Severity::Blocking)
    }

    /// The most severe issue's severity.
    pub fn worst(&self) -> Option<Severity> {
        self.issues.iter().map(|i| i.severity).min()
    }
}

/// Scan `dir` with `scanner` and check it. Blocking; fast when the jars are cached.
pub fn check_mods_dir(
    scanner: &ModScanner,
    dir: &Path,
    env: &CheckEnvironment,
    origins: &FileOrigins,
) -> (ModSnapshot, ModCheckReport) {
    let snapshot = scanner.scan_dir(dir);
    let report = check_mods(&snapshot, env, origins);
    (snapshot, report)
}

/// [`check_mods_dir`] with the shared scanner, on the blocking thread pool.
pub async fn check_mods_dir_async(
    dir: PathBuf,
    env: CheckEnvironment,
    origins: FileOrigins,
) -> Result<(ModSnapshot, ModCheckReport)> {
    blocking(move || Ok(check_mods_dir(ModScanner::shared(), &dir, &env, &origins))).await
}

// --- loader profile ----------------------------------------------------------------------------

/// Whether the instance's loader reads a descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reads {
    Yes,
    No,
    /// Unknown (old Forge decides by annotations, unknown game versions).
    Unsure,
}

/// What the loader itself provides under an id.
enum Builtin {
    Minecraft,
    Java,
    /// The loader, with its version when known.
    Loader(Option<String>),
    /// Present, version not judged.
    Other,
    /// The id of another loader, which this one does not provide; `certain` when the loader
    /// certainly refuses a mod that requires it.
    Foreign {
        certain: bool,
    },
}

struct Profile<'a> {
    env: &'a CheckEnvironment,
    minecraft: MinecraftVersion,
    loader_version: Option<String>,
    /// Sinytra Connector: Forge/NeoForge also load Fabric mods.
    connector: bool,
    /// Kilt: Fabric also loads Forge mods.
    kilt: bool,
}

impl<'a> Profile<'a> {
    fn new(env: &'a CheckEnvironment, snapshot: &ModSnapshot) -> Self {
        let has = |id: &str| {
            snapshot
                .enabled()
                .any(|jar| jar.contents.mods.iter().any(|m| m.id == id))
        };
        let loader_version = env.loader_version.as_deref().map(|v| {
            let v = v.trim();
            v.strip_prefix(&format!("{}-", env.minecraft))
                .unwrap_or(v)
                .to_owned()
        });
        Self {
            env,
            minecraft: MinecraftVersion::parse(&env.minecraft),
            loader_version: loader_version.filter(|v| !v.is_empty()),
            connector: has("connectormod"),
            kilt: has("kilt"),
        }
    }

    fn loader(&self) -> LoaderKind {
        self.env.loader
    }

    fn fabric_like(&self) -> bool {
        matches!(self.loader(), LoaderKind::Fabric | LoaderKind::Quilt)
    }

    fn reads(&self, descriptor: DescriptorKind, jar: &JarContents) -> Reads {
        use DescriptorKind as D;
        match self.loader() {
            LoaderKind::Fabric => match descriptor {
                D::Fabric => Reads::Yes,
                D::ModsToml | D::NeoForgeToml if self.kilt => Reads::Unsure,
                _ => Reads::No,
            },
            LoaderKind::Quilt => match descriptor {
                D::Quilt => Reads::Yes,
                D::Fabric if !jar.descriptors.contains(&D::Quilt) => Reads::Yes,
                _ => Reads::No,
            },
            LoaderKind::Forge => match self.minecraft.at_least(1, 13, 0) {
                Some(true) => match descriptor {
                    D::ModsToml => Reads::Yes,
                    D::Fabric | D::Quilt if self.connector => Reads::Unsure,
                    _ => Reads::No,
                },
                _ => Reads::Unsure,
            },
            LoaderKind::NeoForge => match (self.minecraft.at_least(1, 20, 5), descriptor) {
                (Some(true), D::NeoForgeToml) | (Some(false), D::ModsToml) => Reads::Yes,
                // NeoForge 1.20.5+ is not known to read the old `mods.toml` at all, but that is
                // not certain enough to call its mods missing.
                (Some(true), D::ModsToml) => Reads::Unsure,
                (Some(_), D::Fabric | D::Quilt) if self.connector => Reads::Unsure,
                (Some(_), _) => Reads::No,
                (None, _) => Reads::Unsure,
            },
            LoaderKind::Vanilla => Reads::No,
        }
    }

    fn builtin(&self, id: &str) -> Option<Builtin> {
        let loader_version = || Builtin::Loader(self.loader_version.clone());
        match (self.loader(), id) {
            (_, "minecraft") => Some(Builtin::Minecraft),
            (_, "mixinextras") => Some(Builtin::Other),
            (LoaderKind::Fabric | LoaderKind::Quilt, "java") => Some(Builtin::Java),
            (LoaderKind::Fabric, "fabricloader") => Some(loader_version()),
            (LoaderKind::Fabric, "fabric-loader") => Some(Builtin::Other),
            (LoaderKind::Quilt, "quilt_loader") => Some(loader_version()),
            (LoaderKind::Quilt, "fabricloader" | "fabric-loader") => Some(Builtin::Other),
            (LoaderKind::Fabric | LoaderKind::Quilt, "forge" | "neoforge") => {
                Some(Builtin::Foreign { certain: true })
            }
            (LoaderKind::Fabric, "quilt_loader") => Some(Builtin::Foreign { certain: true }),
            (LoaderKind::Forge, "forge") => Some(loader_version()),
            (LoaderKind::Forge, "neoforge") => Some(Builtin::Foreign { certain: true }),
            (LoaderKind::NeoForge, "neoforge") => Some(loader_version()),
            // NeoForge for 1.20.1 is a Forge fork that still answers to `forge`.
            (LoaderKind::NeoForge, "forge") => Some(if self.env.minecraft == "1.20.1" {
                Builtin::Other
            } else {
                Builtin::Foreign { certain: false }
            }),
            (
                LoaderKind::Forge | LoaderKind::NeoForge,
                "fabricloader" | "fabric-loader" | "quilt_loader",
            ) => Some(Builtin::Foreign { certain: false }),
            (
                LoaderKind::Forge | LoaderKind::NeoForge,
                "java" | "fml" | "javafml" | "lowcodefml" | "mcp",
            ) => Some(Builtin::Other),
            _ => None,
        }
    }

    /// Whether a mod with this id in the mods folder clashes with the loader's own.
    fn is_reserved(&self, id: &str) -> bool {
        matches!(
            self.builtin(id),
            Some(Builtin::Minecraft | Builtin::Java | Builtin::Loader(_))
        )
    }

    /// The loader skips this descriptor (certainly, or as far as is known: NeoForge 1.20.5+
    /// and the old `mods.toml`). Used for the "made for another loader" warning only.
    fn ignores(&self, descriptor: DescriptorKind, jar: &JarContents) -> bool {
        match self.reads(descriptor, jar) {
            Reads::No => true,
            Reads::Yes => false,
            Reads::Unsure => {
                self.loader() == LoaderKind::NeoForge
                    && descriptor == DescriptorKind::ModsToml
                    && self.minecraft.at_least(1, 20, 5) == Some(true)
            }
        }
    }

    /// Loader name for messages (`fabric`, `neoforge`).
    fn loader_name(loader: LoaderKind) -> String {
        loader.modrinth_name().unwrap_or("vanilla").to_owned()
    }

    /// Compare two versions in the syntax the loader uses; `None` when unsure.
    fn compare(&self, a: &str, b: &str) -> Option<std::cmp::Ordering> {
        if self.fabric_like() {
            Some(FabricVersion::parse(a)?.cmp(&FabricVersion::parse(b)?))
        } else {
            Some(MavenVersion::parse(a)?.cmp(&MavenVersion::parse(b)?))
        }
    }
}

// --- the check ---------------------------------------------------------------------------------

/// A mod present in an enabled jar.
#[derive(Clone, Copy)]
struct Present<'a> {
    jar: &'a ScannedJar,
    m: &'a DeclaredMod,
    top: bool,
    reads: Reads,
}

fn mod_ref(jar: &ScannedJar, m: Option<&DeclaredMod>) -> ModRef {
    ModRef {
        file: jar.file_name.clone(),
        mod_id: m.map(|m| m.id.clone()),
        name: m.map_or_else(|| jar.readable_name(), |m| m.display_name().to_owned()),
        version: m.and_then(|m| m.version.clone()),
        bundled: m.is_some_and(|m| !m.nested_in.is_empty()),
    }
}

fn redownload_fix(jar: &ScannedJar, origins: &FileOrigins) -> Option<ModFix> {
    match origins.get(jar.base_name())? {
        FileOrigin::Pack { path } => Some(ModFix::RedownloadPackFile { path: path.clone() }),
        FileOrigin::Modrinth {
            project_id,
            version_id,
        } => Some(ModFix::RedownloadModrinthFile {
            file: jar.file_name.clone(),
            project_id: project_id.clone(),
            version_id: version_id.clone(),
        }),
    }
}

fn disable(jar: &ScannedJar, reason: DisableReason) -> ModFix {
    ModFix::DisableFile {
        file: jar.file_name.clone(),
        reason,
    }
}

fn update(jar: &ScannedJar, mod_id: &str, version_req: Option<String>, or_disable: bool) -> ModFix {
    ModFix::UpdateToCompatible {
        file: jar.file_name.clone(),
        mod_id: mod_id.to_owned(),
        version_req,
        disable_if_unavailable: or_disable,
    }
}

/// Check the enabled jars of a scanned mods folder for `env`. `origins` turns fixes for pack
/// and Modrinth files into re-downloads. Pure and fast: no file access.
pub fn check_mods(
    snapshot: &ModSnapshot,
    env: &CheckEnvironment,
    origins: &FileOrigins,
) -> ModCheckReport {
    let profile = Profile::new(env, snapshot);
    let mut checker = Checker {
        profile: &profile,
        snapshot,
        origins,
        issues: Vec::new(),
    };
    let enabled: Vec<&ScannedJar> = snapshot.enabled().collect();
    for jar in &enabled {
        checker.file_state(jar);
    }

    // Everything present, by id and provided id.
    let mut present: HashMap<&str, Vec<Present>> = HashMap::new();
    for jar in enabled.iter().filter(|j| j.contents.state == JarState::Ok) {
        let tops = jar.contents.mods.iter().map(|m| (m, true));
        let nested = jar.contents.nested.iter().map(|m| (m, false));
        for (m, top) in tops.chain(nested) {
            let reads = profile.reads(m.descriptor, &jar.contents);
            if reads == Reads::No {
                continue;
            }
            let entry = Present { jar, m, top, reads };
            for id in std::iter::once(&m.id).chain(&m.provides) {
                let list = present.entry(id.as_str()).or_default();
                if !list.iter().any(|p| std::ptr::eq(p.m, m)) {
                    list.push(entry);
                }
            }
        }
    }
    // Mods whose own requirements are judged: top level, read for certain, loaded on clients.
    let mut active: Vec<Present> = Vec::new();
    for p in present.values().flatten() {
        if p.top
            && p.reads == Reads::Yes
            && p.m.client
            && !active.iter().any(|q| std::ptr::eq(q.m, p.m))
        {
            active.push(*p);
        }
    }
    active.sort_by(|a, b| (&a.jar.file_name, &a.m.id).cmp(&(&b.jar.file_name, &b.m.id)));

    for jar in &enabled {
        checker.wrong_loader(jar);
    }
    checker.reserved_ids(&active);
    checker.duplicates(&active, &present);
    checker.relations(&active, &present);

    let mut issues = checker.issues;
    issues.sort_by(|a, b| {
        let key = |i: &ModIssue| {
            (
                i.severity,
                i.kind,
                i.mods.first().map(|m| m.file.to_lowercase()),
                i.target.as_ref().map(|t| t.id.clone()),
            )
        };
        key(a).cmp(&key(b))
    });
    ModCheckReport {
        issues,
        checked_files: u32::try_from(enabled.len()).unwrap_or(u32::MAX),
    }
}

struct Checker<'a> {
    profile: &'a Profile<'a>,
    snapshot: &'a ModSnapshot,
    origins: &'a FileOrigins,
    issues: Vec<ModIssue>,
}

impl Checker<'_> {
    fn push(
        &mut self,
        severity: Severity,
        kind: IssueKind,
        mods: Vec<ModRef>,
        target: Option<IssueTarget>,
        fixes: Vec<ModFix>,
    ) {
        self.issues.push(ModIssue {
            severity,
            kind,
            mods,
            target,
            fixes,
        });
    }

    /// Unreadable, corrupt and descriptor-less files.
    fn file_state(&mut self, jar: &ScannedJar) {
        let contents = &jar.contents;
        match &contents.state {
            JarState::Unreadable { .. } => {
                self.push(
                    Severity::Warning,
                    IssueKind::Unreadable,
                    vec![mod_ref(jar, None)],
                    None,
                    Vec::new(),
                );
            }
            JarState::Corrupt { certain, .. } => {
                // Fabric and Quilt open every jar of the folder and stop on a broken one.
                let severity = if *certain && self.profile.fabric_like() {
                    Severity::Blocking
                } else {
                    Severity::Warning
                };
                let fixes = redownload_fix(jar, self.origins)
                    .into_iter()
                    .chain([disable(jar, DisableReason::Corrupt)])
                    .collect();
                self.push(
                    severity,
                    IssueKind::Corrupt,
                    vec![mod_ref(jar, None)],
                    None,
                    fixes,
                );
            }
            JarState::Ok => {
                let read = |d: &DescriptorKind| self.profile.reads(*d, contents) == Reads::Yes;
                let broken_read = contents.broken_descriptors.iter().any(read);
                let parsed_read = contents
                    .mods
                    .iter()
                    .any(|m| self.profile.reads(m.descriptor, contents) == Reads::Yes);
                if broken_read && !parsed_read {
                    let fixes = redownload_fix(jar, self.origins)
                        .into_iter()
                        .chain([disable(jar, DisableReason::Corrupt)])
                        .collect();
                    self.push(
                        Severity::Warning,
                        IssueKind::BrokenMetadata,
                        vec![mod_ref(jar, None)],
                        None,
                        fixes,
                    );
                }
                let library = contents.fml_mod_type.is_some()
                    && matches!(
                        self.profile.loader(),
                        LoaderKind::Forge | LoaderKind::NeoForge
                    );
                let legacy_forge = self.profile.loader() == LoaderKind::Forge
                    && self.profile.minecraft.at_least(1, 13, 0) != Some(true);
                if contents.descriptors.is_empty() && !library && !legacy_forge {
                    self.push(
                        Severity::Info,
                        IssueKind::NoMetadata,
                        vec![mod_ref(jar, None)],
                        None,
                        Vec::new(),
                    );
                }
            }
        }
    }

    /// Jars none of whose descriptors the loader reads.
    fn wrong_loader(&mut self, jar: &ScannedJar) {
        let contents = &jar.contents;
        if contents.state != JarState::Ok || contents.mods.is_empty() {
            return;
        }
        if contents
            .mods
            .iter()
            .any(|m| !self.profile.ignores(m.descriptor, contents))
        {
            return;
        }
        let first = &contents.mods[0];
        let legacy = contents
            .mods
            .iter()
            .all(|m| m.descriptor == DescriptorKind::McmodInfo);
        if legacy
            && matches!(
                self.profile.loader(),
                LoaderKind::Forge | LoaderKind::NeoForge
            )
        {
            // A Forge mod for 1.12.2 or older in a modern Forge/NeoForge instance.
            self.push(
                Severity::Warning,
                IssueKind::IncompatibleMinecraft,
                vec![mod_ref(jar, Some(first))],
                Some(IssueTarget {
                    id: "minecraft".to_owned(),
                    name: None,
                    requirement: None,
                    found: Some(self.profile.env.minecraft.clone()),
                    file: None,
                }),
                vec![
                    update(jar, &first.id, None, true),
                    disable(jar, DisableReason::IncompatibleMinecraft),
                ],
            );
            return;
        }
        self.push(
            Severity::Warning,
            IssueKind::WrongLoader,
            vec![mod_ref(jar, Some(first))],
            Some(IssueTarget {
                id: Profile::loader_name(first.loader),
                name: None,
                requirement: None,
                found: Some(Profile::loader_name(self.profile.loader())),
                file: None,
            }),
            vec![
                update(jar, &first.id, None, true),
                disable(jar, DisableReason::WrongLoader),
            ],
        );
    }

    /// Mods that use the id of the game, Java or the loader itself.
    fn reserved_ids(&mut self, active: &[Present]) {
        for p in active {
            let own = self.profile.is_reserved(&p.m.id);
            let provided = p.m.provides.iter().any(|id| self.profile.is_reserved(id));
            if !own && !provided {
                continue;
            }
            // Fabric and Quilt refuse mods sharing an id with a built-in mod.
            let severity = if own && self.profile.fabric_like() {
                Severity::Blocking
            } else {
                Severity::Warning
            };
            let id = if own {
                p.m.id.clone()
            } else {
                p.m.provides
                    .iter()
                    .find(|id| self.profile.is_reserved(id))
                    .cloned()
                    .unwrap_or_default()
            };
            self.push(
                severity,
                IssueKind::ProvidedByLoader,
                vec![mod_ref(p.jar, Some(p.m))],
                Some(IssueTarget {
                    id,
                    name: None,
                    requirement: None,
                    found: None,
                    file: None,
                }),
                vec![disable(p.jar, DisableReason::ProvidedByLoader)],
            );
        }
    }

    /// The copy to keep among several: the newest version, else the newest file.
    fn newest<'b>(&self, copies: &[Present<'b>]) -> Present<'b> {
        let mut best = copies[0];
        for c in &copies[1..] {
            let by_version = match (&c.m.version, &best.m.version) {
                (Some(a), Some(b)) => self.profile.compare(a, b),
                _ => None,
            };
            let newer = match by_version {
                Some(order) if order != std::cmp::Ordering::Equal => order.is_gt(),
                _ => {
                    let time = c.jar.modified_ms.cmp(&best.jar.modified_ms);
                    if time != std::cmp::Ordering::Equal {
                        time.is_gt()
                    } else {
                        // "sodium.jar" over "sodium (1).jar".
                        (c.jar.file_name.len(), &c.jar.file_name)
                            < (best.jar.file_name.len(), &best.jar.file_name)
                    }
                }
            };
            if newer {
                best = *c;
            }
        }
        best
    }

    fn duplicates(&mut self, active: &[Present], present: &HashMap<&str, Vec<Present>>) {
        let mut by_id: HashMap<&str, Vec<(Present, bool)>> = HashMap::new();
        for p in active {
            by_id.entry(p.m.id.as_str()).or_default().push((*p, false));
            for id in &p.m.provides {
                by_id.entry(id.as_str()).or_default().push((*p, true));
            }
        }
        let mut ids: Vec<&&str> = by_id.keys().collect();
        ids.sort();
        let mut reported: HashSet<Vec<String>> = HashSet::new();
        for id in ids {
            let entries = &by_id[*id];
            let mut files: Vec<String> = entries
                .iter()
                .map(|(p, _)| p.jar.file_name.clone())
                .collect();
            files.sort();
            files.dedup();
            if files.len() < 2 || !reported.insert(files) {
                continue;
            }
            // One entry per jar.
            let mut copies: Vec<Present> = Vec::new();
            let mut via_provides = false;
            for (p, provided) in entries {
                via_provides |= *provided;
                if !copies.iter().any(|c| std::ptr::eq(c.jar, p.jar)) {
                    copies.push(*p);
                }
            }
            let identical = copies
                .windows(2)
                .all(|w| w[0].m.version == w[1].m.version && w[0].jar.size == w[1].jar.size);
            let severity = if self.profile.fabric_like() && !via_provides && !identical {
                Severity::Blocking
            } else {
                Severity::Warning
            };
            let keep = self.newest(&copies);
            let fixes = copies
                .iter()
                .filter(|c| !std::ptr::eq(c.jar, keep.jar))
                .map(|c| disable(c.jar, DisableReason::Duplicate))
                .collect();
            self.push(
                severity,
                IssueKind::Duplicate,
                copies.iter().map(|c| mod_ref(c.jar, Some(c.m))).collect(),
                Some(IssueTarget {
                    id: (*id).to_owned(),
                    name: None,
                    requirement: None,
                    found: None,
                    file: None,
                }),
                fixes,
            );
        }

        // Fabric and Quilt always load a separately installed mod instead of the copy another
        // mod bundles, even when the separate one is older.
        if !self.profile.fabric_like() {
            return;
        }
        for p in active {
            let Some(version) = &p.m.version else {
                continue;
            };
            let newer_bundled = present.get(p.m.id.as_str()).and_then(|list| {
                list.iter().find(|b| {
                    !b.top
                        && !std::ptr::eq(b.jar, p.jar)
                        && b.m.id == p.m.id
                        && b.m.version.as_deref().is_some_and(|v| {
                            self.profile.compare(v, version) == Some(std::cmp::Ordering::Greater)
                        })
                })
            });
            if let Some(bundled) = newer_bundled {
                self.push(
                    Severity::Warning,
                    IssueKind::Duplicate,
                    vec![
                        mod_ref(p.jar, Some(p.m)),
                        mod_ref(bundled.jar, Some(bundled.m)),
                    ],
                    Some(IssueTarget {
                        id: p.m.id.clone(),
                        name: Some(bundled.m.display_name().to_owned()),
                        requirement: None,
                        found: bundled.m.version.clone(),
                        file: Some(bundled.jar.file_name.clone()),
                    }),
                    vec![disable(p.jar, DisableReason::Duplicate)],
                );
            }
        }
    }

    fn relations(&mut self, active: &[Present], present: &HashMap<&str, Vec<Present>>) {
        // Missing dependencies grouped by (id, requirement) so one issue names every mod that
        // needs it.
        let mut missing: Vec<(String, Option<String>, Vec<Present>)> = Vec::new();
        for p in active {
            for r in p.m.relations.iter().filter(|r| r.client) {
                match r.kind {
                    RelationKind::Optional => {}
                    RelationKind::Depends => {
                        if let Some(builtin) = self.profile.builtin(&r.id) {
                            self.platform_requirement(p, r, builtin);
                            continue;
                        }
                        let candidates = present
                            .get(r.id.as_str())
                            .map(Vec::as_slice)
                            .unwrap_or_default();
                        if candidates.is_empty() {
                            match missing
                                .iter_mut()
                                .find(|(id, text, _)| *id == r.id && *text == r.text)
                            {
                                Some((_, _, needers)) => needers.push(*p),
                                None => missing.push((r.id.clone(), r.text.clone(), vec![*p])),
                            }
                            continue;
                        }
                        self.dependency_version(p, r, candidates);
                    }
                    RelationKind::Breaks | RelationKind::Conflicts => {
                        if r.id == "minecraft" {
                            if r.kind == RelationKind::Breaks
                                && r.req.matches_minecraft(&self.profile.minecraft) == Some(true)
                            {
                                self.minecraft_issue(p, r.text.clone());
                            }
                            continue;
                        }
                        if self.profile.builtin(&r.id).is_some() {
                            continue;
                        }
                        self.incompatibility(p, r, active);
                    }
                }
            }
        }
        for (id, text, needers) in missing {
            self.missing_dependency(&id, text, &needers);
        }
    }

    fn platform_requirement(&mut self, p: &Present, r: &Relation, builtin: Builtin) {
        let env = self.profile.env;
        match builtin {
            Builtin::Minecraft => {
                if r.req.matches_minecraft(&self.profile.minecraft) == Some(false) {
                    self.minecraft_issue(p, r.text.clone());
                }
            }
            Builtin::Java => {
                let Some(major) = env.java_major else {
                    return;
                };
                if r.req.matches(&major.to_string()) == Some(false) {
                    let fixes = minimum_java(&r.req)
                        .filter(|need| *need > major)
                        .map(|major| ModFix::UseJava { major })
                        .into_iter()
                        .collect();
                    self.push(
                        Severity::Blocking,
                        IssueKind::JavaVersion,
                        vec![mod_ref(p.jar, Some(p.m))],
                        Some(IssueTarget {
                            id: "java".to_owned(),
                            name: None,
                            requirement: r.text.clone(),
                            found: Some(major.to_string()),
                            file: None,
                        }),
                        fixes,
                    );
                }
            }
            Builtin::Loader(version) => {
                let Some(version) = version else {
                    return;
                };
                if r.req.matches(&version) != Some(false) {
                    return;
                }
                let fixes = match (r.req.mismatch(&version), &r.text) {
                    (Mismatch::TooOld, Some(text)) => vec![ModFix::UpdateLoader {
                        loader: env.loader,
                        version_req: text.clone(),
                    }],
                    _ => vec![update(p.jar, &p.m.id, None, false)],
                };
                self.push(
                    Severity::Blocking,
                    IssueKind::IncompatibleLoader,
                    vec![mod_ref(p.jar, Some(p.m))],
                    Some(IssueTarget {
                        id: r.id.clone(),
                        name: None,
                        requirement: r.text.clone(),
                        found: Some(version),
                        file: None,
                    }),
                    fixes,
                );
            }
            Builtin::Foreign { certain } => {
                self.push(
                    if certain {
                        Severity::Blocking
                    } else {
                        Severity::Warning
                    },
                    IssueKind::WrongLoader,
                    vec![mod_ref(p.jar, Some(p.m))],
                    Some(IssueTarget {
                        id: Profile::loader_name(p.m.loader),
                        name: None,
                        requirement: r.text.clone(),
                        found: Some(Profile::loader_name(env.loader)),
                        file: None,
                    }),
                    vec![
                        update(p.jar, &p.m.id, None, true),
                        disable(p.jar, DisableReason::WrongLoader),
                    ],
                );
            }
            Builtin::Other => {}
        }
    }

    fn minecraft_issue(&mut self, p: &Present, requirement: Option<String>) {
        self.push(
            Severity::Blocking,
            IssueKind::IncompatibleMinecraft,
            vec![mod_ref(p.jar, Some(p.m))],
            Some(IssueTarget {
                id: "minecraft".to_owned(),
                name: None,
                requirement,
                found: Some(self.profile.env.minecraft.clone()),
                file: None,
            }),
            vec![
                update(p.jar, &p.m.id, None, true),
                disable(p.jar, DisableReason::IncompatibleMinecraft),
            ],
        );
    }

    fn dependency_version(&mut self, p: &Present, r: &Relation, candidates: &[Present]) {
        if r.req.is_any() {
            return;
        }
        // Reported only when every copy certainly fails. Fabric relaxes dependencies on mods
        // it skips on the client (server-only), so those are never judged.
        let mut failing: Vec<&Present> = Vec::new();
        for c in candidates {
            let Some(version) = &c.m.version else {
                return;
            };
            if c.reads != Reads::Yes || !c.m.client || r.req.matches(version) != Some(false) {
                return;
            }
            failing.push(c);
        }
        let Some(best) = failing.iter().find(|c| c.top).or(failing.first()).copied() else {
            return;
        };
        let version = best.m.version.clone().unwrap_or_default();
        let fixes = match r.req.mismatch(&version) {
            Mismatch::TooOld if best.top => vec![update(best.jar, &r.id, r.text.clone(), false)],
            // A separate newer copy is loaded instead of a bundled one.
            Mismatch::TooOld => vec![self.fetch(&r.id, r.text.clone(), vec![p.m.id.clone()])],
            Mismatch::TooNew | Mismatch::Unclear => vec![update(p.jar, &p.m.id, None, false)],
        };
        let found: Vec<String> = failing.iter().filter_map(|c| c.m.version.clone()).collect();
        self.push(
            Severity::Blocking,
            IssueKind::DependencyVersion,
            vec![mod_ref(p.jar, Some(p.m))],
            Some(IssueTarget {
                id: r.id.clone(),
                name: Some(best.m.display_name().to_owned()),
                requirement: r.text.clone(),
                found: Some(found.join(", ")),
                file: Some(best.jar.file_name.clone()),
            }),
            fixes,
        );
    }

    fn fetch(&self, id: &str, version_req: Option<String>, required_by: Vec<String>) -> ModFix {
        ModFix::FetchDependency {
            mod_id: id.to_owned(),
            version_req,
            loader: self.profile.loader(),
            minecraft: self.profile.env.minecraft.clone(),
            required_by,
        }
    }

    fn missing_dependency(&mut self, id: &str, text: Option<String>, needers: &[Present]) {
        let req = needers
            .first()
            .and_then(|p| p.m.relations.iter().find(|r| r.id == id && r.text == text))
            .map(|r| r.req.clone())
            .unwrap_or(VersionReq::Any);
        // A disabled jar that provides it (and fits) is the quickest fix.
        let disabled_copy = self.snapshot.disabled().find_map(|jar| {
            let m = jar.contents.all_mods().find(|m| {
                m.answers_to(id)
                    && self.profile.reads(m.descriptor, &jar.contents) == Reads::Yes
                    && m.version
                        .as_deref()
                        .map_or(req.is_any(), |v| req.matches(v) != Some(false))
            })?;
            Some((jar, m))
        });
        let required_by: Vec<String> = needers.iter().map(|p| p.m.id.clone()).collect();
        let fixes = disabled_copy
            .map(|(jar, m)| ModFix::EnableFile {
                file: jar.file_name.clone(),
                mod_id: m.id.clone(),
            })
            .into_iter()
            .chain([self.fetch(id, text.clone(), required_by)])
            .collect();
        self.push(
            Severity::Blocking,
            IssueKind::MissingDependency,
            needers.iter().map(|p| mod_ref(p.jar, Some(p.m))).collect(),
            Some(IssueTarget {
                id: id.to_owned(),
                name: disabled_copy.map(|(_, m)| m.display_name().to_owned()),
                requirement: text,
                found: None,
                // Where a turned-off copy is.
                file: disabled_copy.map(|(jar, _)| jar.file_name.clone()),
            }),
            fixes,
        );
    }

    fn incompatibility(&mut self, p: &Present, r: &Relation, active: &[Present]) {
        let breaks = r.kind == RelationKind::Breaks;
        for other in active
            .iter()
            .filter(|o| !std::ptr::eq(o.jar, p.jar) && o.m.answers_to(&r.id))
        {
            let matched = match &other.m.version {
                Some(version) => r.req.matches(version),
                None => r.req.is_any().then_some(true),
            };
            if matched != Some(true) {
                continue;
            }
            let mut fixes = Vec::new();
            if r.req.has_upper_bound() {
                fixes.push(update(other.jar, &other.m.id, None, false));
            }
            fixes.push(disable(other.jar, DisableReason::Breaks));
            fixes.push(disable(p.jar, DisableReason::Breaks));
            self.push(
                if breaks {
                    Severity::Blocking
                } else {
                    Severity::Warning
                },
                if breaks {
                    IssueKind::Breaks
                } else {
                    IssueKind::Conflicts
                },
                vec![mod_ref(p.jar, Some(p.m)), mod_ref(other.jar, Some(other.m))],
                Some(IssueTarget {
                    id: r.id.clone(),
                    name: Some(other.m.display_name().to_owned()),
                    requirement: r.text.clone(),
                    found: other.m.version.clone(),
                    file: Some(other.jar.file_name.clone()),
                }),
                fixes,
            );
        }
    }
}

/// The older copies [`check_mods`] would turn off among jars declaring `id` (enabled, top
/// level), for crash attribution of duplicate-mod errors.
pub(crate) fn older_copies<'a>(
    snapshot: &'a ModSnapshot,
    env: &CheckEnvironment,
    id: &str,
) -> Vec<&'a ScannedJar> {
    let profile = Profile::new(env, snapshot);
    let copies: Vec<Present> = snapshot
        .enabled()
        .filter_map(|jar| {
            let m = jar.contents.mods.iter().find(|m| m.answers_to(id))?;
            Some(Present {
                jar,
                m,
                top: true,
                reads: Reads::Yes,
            })
        })
        .collect();
    if copies.len() < 2 {
        return Vec::new();
    }
    let checker = Checker {
        profile: &profile,
        snapshot,
        origins: &FileOrigins::new(),
        issues: Vec::new(),
    };
    let keep = checker.newest(&copies);
    copies
        .into_iter()
        .filter(|c| !std::ptr::eq(c.jar, keep.jar))
        .map(|c| c.jar)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Instant;

    use super::super::scan::test_jars::*;
    use super::super::test_support::zip_bytes;
    use super::*;

    fn env(loader: LoaderKind, minecraft: &str) -> CheckEnvironment {
        CheckEnvironment {
            minecraft: minecraft.to_owned(),
            loader,
            loader_version: Some(
                match loader {
                    LoaderKind::Fabric => "0.16.10",
                    LoaderKind::Quilt => "0.27.1",
                    LoaderKind::Forge => "47.2.0",
                    LoaderKind::NeoForge => "21.1.77",
                    LoaderKind::Vanilla => "",
                }
                .to_owned(),
            ),
            java_major: Some(21),
        }
    }

    fn fabric() -> CheckEnvironment {
        env(LoaderKind::Fabric, "1.21.1")
    }

    fn jar(name: &str, bytes: Vec<u8>) -> ScannedJar {
        ScannedJar::from_bytes(name, &bytes)
    }

    fn check(env: &CheckEnvironment, jars: Vec<ScannedJar>) -> ModCheckReport {
        check_mods(&ModSnapshot::new("mods", jars), env, &FileOrigins::new())
    }

    fn kinds(report: &ModCheckReport) -> Vec<(Severity, IssueKind)> {
        report.issues.iter().map(|i| (i.severity, i.kind)).collect()
    }

    #[test]
    fn clean_set_has_no_issues() {
        let report = check(
            &fabric(),
            vec![
                jar(
                    "fabric-api.jar",
                    fabric_jar(
                        "fabric-api",
                        "0.102.0+1.21.1",
                        r#""provides": ["fabric"], "depends": {"fabricloader": ">=0.15", "minecraft": "~1.21.1", "java": ">=21"}"#,
                    ),
                ),
                jar(
                    "modmenu.jar",
                    fabric_jar(
                        "modmenu",
                        "11.0.1",
                        r#""depends": {"fabric": "*", "minecraft": ">=1.21", "mixinextras": "*", "fabric-loader": "*"}, "recommends": {"absent": "*"}"#,
                    ),
                ),
            ],
        );
        assert!(report.issues.is_empty(), "{:#?}", report.issues);
        assert_eq!(report.checked_files, 2);
        assert!(!report.has_blocking());
    }

    #[test]
    fn duplicates_keep_the_newest() {
        let report = check(
            &fabric(),
            vec![
                jar("sodium-0.5.jar", fabric_jar("sodium", "0.5.8+mc1.21", "")),
                jar("sodium-0.6.jar", fabric_jar("sodium", "0.6.0+mc1.21.1", "")),
            ],
        );
        assert_eq!(
            kinds(&report),
            vec![(Severity::Blocking, IssueKind::Duplicate)]
        );
        let issue = &report.issues[0];
        assert_eq!(issue.mods.len(), 2);
        assert_eq!(issue.mods[0].name, "sodium name");
        assert_eq!(
            issue.fixes,
            vec![ModFix::DisableFile {
                file: "sodium-0.5.jar".into(),
                reason: DisableReason::Duplicate
            }]
        );

        // Identical copies only warn; the copy-named file goes.
        let bytes = fabric_jar("lithium", "0.13.0", "");
        let report = check(
            &fabric(),
            vec![
                jar("lithium (1).jar", bytes.clone()),
                jar("lithium.jar", bytes),
            ],
        );
        assert_eq!(
            kinds(&report),
            vec![(Severity::Warning, IssueKind::Duplicate)]
        );
        assert_eq!(
            report.issues[0].fixes,
            vec![ModFix::DisableFile {
                file: "lithium (1).jar".into(),
                reason: DisableReason::Duplicate
            }]
        );

        // An id that another jar provides: a warning.
        let report = check(
            &fabric(),
            vec![
                jar(
                    "qfapi.jar",
                    fabric_jar("quilted_fabric_api", "7.0", r#""provides": ["fabric-api"]"#),
                ),
                jar("fabric-api.jar", fabric_jar("fabric-api", "0.100", "")),
            ],
        );
        assert_eq!(
            kinds(&report),
            vec![(Severity::Warning, IssueKind::Duplicate)]
        );

        // Forge picks the newest copy itself: a warning there too.
        let forge = env(LoaderKind::Forge, "1.20.1");
        let report = check(
            &forge,
            vec![
                jar("jei-1.jar", toml_jar(false, "jei", "15.2.0.27", "")),
                jar("jei-2.jar", toml_jar(false, "jei", "15.3.0.4", "")),
            ],
        );
        assert_eq!(
            kinds(&report),
            vec![(Severity::Warning, IssueKind::Duplicate)]
        );
        assert_eq!(
            report.issues[0].fixes,
            vec![ModFix::DisableFile {
                file: "jei-1.jar".into(),
                reason: DisableReason::Duplicate
            }]
        );
    }

    #[test]
    fn older_separate_copy_of_a_bundled_mod() {
        let bundled = fabric_jar("cloth-config", "15.0.140", "");
        let report = check(
            &fabric(),
            vec![
                jar(
                    "cloth-config-old.jar",
                    fabric_jar("cloth-config", "15.0.127", ""),
                ),
                jar(
                    "rei.jar",
                    fabric_jar_with_nested(
                        "roughlyenoughitems",
                        "16.0.729",
                        &[("cloth.jar", bundled.clone())],
                    ),
                ),
            ],
        );
        assert_eq!(
            kinds(&report),
            vec![(Severity::Warning, IssueKind::Duplicate)]
        );
        assert_eq!(report.issues[0].mods[1].file, "rei.jar");
        // A newer separate copy is normal.
        let report = check(
            &fabric(),
            vec![
                jar(
                    "cloth-config-new.jar",
                    fabric_jar("cloth-config", "15.0.200", ""),
                ),
                jar(
                    "rei.jar",
                    fabric_jar_with_nested(
                        "roughlyenoughitems",
                        "16.0.729",
                        &[("cloth.jar", bundled)],
                    ),
                ),
            ],
        );
        assert!(report.issues.is_empty(), "{:#?}", report.issues);
    }

    #[test]
    fn missing_dependencies() {
        let report = check(
            &fabric(),
            vec![
                jar(
                    "modmenu.jar",
                    fabric_jar("modmenu", "11.0", r#""depends": {"fabric-api": "*"}"#),
                ),
                jar(
                    "other.jar",
                    fabric_jar(
                        "other",
                        "1.0",
                        r#""depends": {"fabric-api": "*", "cloth-config": ">=15"}"#,
                    ),
                ),
            ],
        );
        assert_eq!(
            kinds(&report),
            vec![
                (Severity::Blocking, IssueKind::MissingDependency),
                (Severity::Blocking, IssueKind::MissingDependency)
            ]
        );
        let fapi = report
            .issues
            .iter()
            .find(|i| i.target.as_ref().unwrap().id == "fabric-api")
            .unwrap();
        assert_eq!(
            fapi.mods.len(),
            2,
            "one issue names every mod that needs it"
        );
        assert_eq!(
            fapi.fixes,
            vec![ModFix::FetchDependency {
                mod_id: "fabric-api".into(),
                version_req: None,
                loader: LoaderKind::Fabric,
                minecraft: "1.21.1".into(),
                required_by: vec!["modmenu".into(), "other".into()]
            }]
        );
        let cloth = report
            .issues
            .iter()
            .find(|i| i.target.as_ref().unwrap().id == "cloth-config")
            .unwrap();
        assert_eq!(
            cloth.target.as_ref().unwrap().requirement.as_deref(),
            Some(">=15")
        );

        // Provided ids, bundled mods and the loader's own ids satisfy dependencies.
        let report = check(
            &fabric(),
            vec![
                jar(
                    "a.jar",
                    fabric_jar(
                        "a",
                        "1.0",
                        r#""depends": {"fabric": "*", "inner-lib": ">=2", "fabricloader": "*", "minecraft": "*", "java": "*"}"#,
                    ),
                ),
                jar(
                    "fapi.jar",
                    fabric_jar("fabric-api", "0.100", r#""provides": ["fabric"]"#),
                ),
                jar(
                    "bundle.jar",
                    fabric_jar_with_nested(
                        "bundle",
                        "1.0",
                        &[("lib.jar", fabric_jar("inner-lib", "2.1", ""))],
                    ),
                ),
            ],
        );
        assert!(report.issues.is_empty(), "{:#?}", report.issues);
    }

    #[test]
    fn disabled_provider_is_enabled_first() {
        let report = check(
            &fabric(),
            vec![
                jar(
                    "a.jar",
                    fabric_jar("a", "1.0", r#""depends": {"lib": ">=1.0"}"#),
                ),
                jar("lib.jar.disabled", fabric_jar("lib", "1.2", "")),
            ],
        );
        assert_eq!(
            kinds(&report),
            vec![(Severity::Blocking, IssueKind::MissingDependency)]
        );
        assert_eq!(
            report.issues[0].fixes[0],
            ModFix::EnableFile {
                file: "lib.jar.disabled".into(),
                mod_id: "lib".into()
            }
        );
        assert!(matches!(
            report.issues[0].fixes[1],
            ModFix::FetchDependency { .. }
        ));

        // A disabled copy that is too old is not offered.
        let report = check(
            &fabric(),
            vec![
                jar(
                    "a.jar",
                    fabric_jar("a", "1.0", r#""depends": {"lib": ">=2.0"}"#),
                ),
                jar("lib.jar.disabled", fabric_jar("lib", "1.2", "")),
            ],
        );
        assert_eq!(report.issues[0].fixes.len(), 1);
    }

    #[test]
    fn dependency_versions() {
        let report = check(
            &fabric(),
            vec![
                jar(
                    "iris.jar",
                    fabric_jar("iris", "1.8.0", r#""depends": {"sodium": ">=0.6.0"}"#),
                ),
                jar("sodium.jar", fabric_jar("sodium", "0.5.8+mc1.21", "")),
            ],
        );
        assert_eq!(
            kinds(&report),
            vec![(Severity::Blocking, IssueKind::DependencyVersion)]
        );
        let issue = &report.issues[0];
        let target = issue.target.as_ref().unwrap();
        assert_eq!(target.found.as_deref(), Some("0.5.8+mc1.21"));
        assert_eq!(target.file.as_deref(), Some("sodium.jar"));
        assert_eq!(
            issue.fixes,
            vec![ModFix::UpdateToCompatible {
                file: "sodium.jar".into(),
                mod_id: "sodium".into(),
                version_req: Some(">=0.6.0".into()),
                disable_if_unavailable: false
            }]
        );

        // Too new: the requiring mod is the one to update.
        let report = check(
            &fabric(),
            vec![
                jar(
                    "addon.jar",
                    fabric_jar("addon", "1.0", r#""depends": {"sodium": "<0.6"}"#),
                ),
                jar("sodium.jar", fabric_jar("sodium", "0.6.0", "")),
            ],
        );
        assert_eq!(
            report.issues[0].fixes,
            vec![ModFix::UpdateToCompatible {
                file: "addon.jar".into(),
                mod_id: "addon".into(),
                version_req: None,
                disable_if_unavailable: false
            }]
        );

        // Not judged: a version or requirement that does not parse with certainty.
        let report = check(
            &fabric(),
            vec![
                jar(
                    "a.jar",
                    fabric_jar("a", "1.0", r#""depends": {"b": ">=2.0", "c": ">=1.x"}"#),
                ),
                jar("b.jar", fabric_jar("b", "1.0_beta", "")),
                jar("c.jar", fabric_jar("c", "0.1", "")),
            ],
        );
        assert!(report.issues.is_empty(), "{:#?}", report.issues);
    }

    #[test]
    fn minecraft_loader_and_java() {
        let report = check(
            &fabric(),
            vec![jar(
                "old.jar",
                fabric_jar(
                    "old",
                    "1.0",
                    r#""depends": {"minecraft": "1.20.x", "fabricloader": ">=0.17", "java": ">=25"}"#,
                ),
            )],
        );
        assert_eq!(
            kinds(&report),
            vec![
                (Severity::Blocking, IssueKind::IncompatibleMinecraft),
                (Severity::Blocking, IssueKind::IncompatibleLoader),
                (Severity::Blocking, IssueKind::JavaVersion),
            ]
        );
        assert_eq!(
            report.issues[0].fixes[0],
            ModFix::UpdateToCompatible {
                file: "old.jar".into(),
                mod_id: "old".into(),
                version_req: None,
                disable_if_unavailable: true
            }
        );
        assert_eq!(
            report.issues[1].fixes,
            vec![ModFix::UpdateLoader {
                loader: LoaderKind::Fabric,
                version_req: ">=0.17".into()
            }]
        );
        assert_eq!(report.issues[2].fixes, vec![ModFix::UseJava { major: 25 }]);

        // Unknown Java or loader versions are not judged; pre-releases only when certain.
        let mut unknown = env(LoaderKind::Fabric, "1.21-pre1");
        unknown.java_major = None;
        unknown.loader_version = None;
        let report = check(
            &unknown,
            vec![
                jar(
                    "a.jar",
                    fabric_jar(
                        "a",
                        "1.0",
                        r#""depends": {"minecraft": ">=1.21-beta.2", "fabricloader": ">=9", "java": ">=99"}"#,
                    ),
                ),
                jar(
                    "b.jar",
                    fabric_jar("b", "1.0", r#""depends": {"minecraft": "~1.20"}"#),
                ),
                jar(
                    "c.jar",
                    fabric_jar("c", "1.0", r#""depends": {"minecraft": "1.21.x"}"#),
                ),
            ],
        );
        assert_eq!(
            kinds(&report),
            vec![(Severity::Blocking, IssueKind::IncompatibleMinecraft)]
        );
        assert_eq!(report.issues[0].mods[0].file, "b.jar");

        // Forge ranges, and the instance loader version given with a game version prefix.
        let mut forge = env(LoaderKind::Forge, "1.21.1");
        forge.loader_version = Some("1.21.1-52.0.1".into());
        let deps = format!(
            "{}{}",
            toml_dep("jei", "minecraft", "true", "[1.20.1,1.20.2)"),
            toml_dep("jei", "forge", "true", "[52.0.2,)")
        );
        let report = check(
            &forge,
            vec![jar("jei.jar", toml_jar(false, "jei", "15.2.0", &deps))],
        );
        assert_eq!(
            kinds(&report),
            vec![
                (Severity::Blocking, IssueKind::IncompatibleMinecraft),
                (Severity::Blocking, IssueKind::IncompatibleLoader),
            ]
        );
        assert_eq!(
            report.issues[1].target.as_ref().unwrap().found.as_deref(),
            Some("52.0.1")
        );
    }

    #[test]
    fn wrong_loaders() {
        let forge_mod = toml_jar(
            false,
            "create",
            "0.5.1",
            &toml_dep("create", "forge", "true", "[47,)"),
        );
        let neo_mod = toml_jar(true, "neomod", "1.0", "");
        let quilt_only = zip_bytes(&[(
            "quilt.mod.json",
            br#"{"schema_version": 1, "quilt_loader": {"id": "qonly", "version": "1.0"}}"#,
        )]);
        let report = check(
            &fabric(),
            vec![
                jar("create.jar", forge_mod.clone()),
                jar("neomod.jar", neo_mod.clone()),
                jar("qonly.jar", quilt_only.clone()),
            ],
        );
        assert_eq!(
            kinds(&report),
            vec![
                (Severity::Warning, IssueKind::WrongLoader),
                (Severity::Warning, IssueKind::WrongLoader),
                (Severity::Warning, IssueKind::WrongLoader),
            ]
        );
        let create = &report.issues[0];
        assert_eq!(create.target.as_ref().unwrap().id, "forge");
        assert_eq!(
            create.target.as_ref().unwrap().found.as_deref(),
            Some("fabric")
        );
        assert_eq!(
            create.fixes,
            vec![
                ModFix::UpdateToCompatible {
                    file: "create.jar".into(),
                    mod_id: "create".into(),
                    version_req: None,
                    disable_if_unavailable: true
                },
                ModFix::DisableFile {
                    file: "create.jar".into(),
                    reason: DisableReason::WrongLoader
                }
            ]
        );

        // Quilt loads Fabric mods; a Fabric mod needing a Forge-only jar is still missing it.
        let quilt = env(LoaderKind::Quilt, "1.21.1");
        let report = check(
            &quilt,
            vec![
                jar("qonly.jar", quilt_only),
                jar(
                    "fab.jar",
                    fabric_jar("fab", "1.0", r#""depends": {"create": "*", "qonly": "*"}"#),
                ),
                jar("create.jar", forge_mod.clone()),
            ],
        );
        assert_eq!(
            kinds(&report),
            vec![
                (Severity::Blocking, IssueKind::MissingDependency),
                (Severity::Warning, IssueKind::WrongLoader),
            ]
        );

        // Connector lets Forge/NeoForge load Fabric mods.
        let neo = env(LoaderKind::NeoForge, "1.21.1");
        let connector = toml_jar(true, "connectormod", "1.0", "");
        let report = check(
            &neo,
            vec![
                jar("connector.jar", connector),
                jar("sodium.jar", fabric_jar("sodium", "0.6.0", "")),
            ],
        );
        assert!(report.issues.is_empty(), "{:#?}", report.issues);

        // A `mods.toml` requiring NeoForge in a Forge instance: Forge refuses it.
        let forge = env(LoaderKind::Forge, "1.20.4");
        let report = check(
            &forge,
            vec![jar(
                "oldneo.jar",
                toml_jar(
                    false,
                    "oldneo",
                    "1.0",
                    &toml_dep("oldneo", "neoforge", "required", "[20.4,)"),
                ),
            )],
        );
        assert_eq!(
            kinds(&report),
            vec![(Severity::Blocking, IssueKind::WrongLoader)]
        );

        // NeoForge 1.21 does not read `mods.toml`: a Forge mod warns, and a NeoForge mod
        // needing it is not called broken (not certain enough).
        let report = check(
            &neo,
            vec![
                jar("create.jar", forge_mod),
                jar(
                    "addon.jar",
                    toml_jar(
                        true,
                        "addon",
                        "1.0",
                        &toml_dep("addon", "create", "required", "[0.5,)"),
                    ),
                ),
            ],
        );
        assert_eq!(
            kinds(&report),
            vec![(Severity::Warning, IssueKind::WrongLoader)]
        );
        let report = check(&neo, vec![jar("neomod.jar", neo_mod)]);
        assert!(report.issues.is_empty(), "{:#?}", report.issues);
    }

    #[test]
    fn breaks_and_conflicts() {
        let report = check(
            &fabric(),
            vec![
                jar(
                    "iris.jar",
                    fabric_jar(
                        "iris",
                        "1.8",
                        r#""breaks": {"sodium": "<0.6", "optifabric": "*"}, "conflicts": {"oldmod": "*"}"#,
                    ),
                ),
                jar("sodium.jar", fabric_jar("sodium", "0.5.8", "")),
                jar("optifabric.jar", fabric_jar("optifabric", "1.14", "")),
                jar("oldmod.jar", fabric_jar("oldmod", "1.0", "")),
            ],
        );
        assert_eq!(
            kinds(&report),
            vec![
                (Severity::Blocking, IssueKind::Breaks),
                (Severity::Blocking, IssueKind::Breaks),
                (Severity::Warning, IssueKind::Conflicts),
            ]
        );
        let sodium = report
            .issues
            .iter()
            .find(|i| i.target.as_ref().unwrap().id == "sodium")
            .unwrap();
        assert_eq!(
            sodium.fixes[0],
            ModFix::UpdateToCompatible {
                file: "sodium.jar".into(),
                mod_id: "sodium".into(),
                version_req: None,
                disable_if_unavailable: false
            }
        );
        let optifabric = report
            .issues
            .iter()
            .find(|i| i.target.as_ref().unwrap().id == "optifabric")
            .unwrap();
        assert_eq!(
            optifabric.fixes[0],
            ModFix::DisableFile {
                file: "optifabric.jar".into(),
                reason: DisableReason::Breaks
            }
        );

        let report = check(
            &fabric(),
            vec![
                jar(
                    "iris.jar",
                    fabric_jar("iris", "1.8", r#""breaks": {"sodium": "<0.6"}"#),
                ),
                jar("sodium.jar", fabric_jar("sodium", "0.6.0", "")),
            ],
        );
        assert!(report.issues.is_empty());

        let neo = env(LoaderKind::NeoForge, "1.21.1");
        let deps = format!(
            "{}{}",
            toml_dep("a", "b", "incompatible", "[1.0,2.0)"),
            toml_dep("a", "c", "discouraged", "")
        );
        let report = check(
            &neo,
            vec![
                jar("a.jar", toml_jar(true, "a", "1.0", &deps)),
                jar("b.jar", toml_jar(true, "b", "1.5", "")),
                jar("c.jar", toml_jar(true, "c", "3.0", "")),
            ],
        );
        assert_eq!(
            kinds(&report),
            vec![
                (Severity::Blocking, IssueKind::Breaks),
                (Severity::Warning, IssueKind::Conflicts)
            ]
        );
    }

    #[test]
    fn files_that_are_not_mods() {
        let mut origins = FileOrigins::new();
        origins.insert(
            "pack-mod.jar".into(),
            FileOrigin::Pack {
                path: "mods/pack-mod.jar".into(),
            },
        );
        let snapshot = ModSnapshot::new(
            "mods",
            vec![
                jar("pack-mod.jar", b"<html>".to_vec()),
                jar("manual.jar", Vec::new()),
                jar("library.jar", zip_bytes(&[("org/lib/A.class", b"x")])),
                jar("broken-json.jar", zip_bytes(&[("fabric.mod.json", b"{{{")])),
                jar(
                    "fabric-loader.jar",
                    fabric_jar("fabricloader", "0.16.10", ""),
                ),
            ],
        );
        let report = check_mods(&snapshot, &fabric(), &origins);
        assert_eq!(
            kinds(&report),
            vec![
                (Severity::Blocking, IssueKind::Corrupt),
                (Severity::Blocking, IssueKind::Corrupt),
                (Severity::Blocking, IssueKind::ProvidedByLoader),
                (Severity::Warning, IssueKind::BrokenMetadata),
                (Severity::Info, IssueKind::NoMetadata),
            ]
        );
        let pack = report
            .issues
            .iter()
            .find(|i| i.mods[0].file == "pack-mod.jar")
            .unwrap();
        assert_eq!(
            pack.fixes[0],
            ModFix::RedownloadPackFile {
                path: "mods/pack-mod.jar".into()
            }
        );

        // The same corrupt file only warns in Forge (FML may skip it).
        let forge = env(LoaderKind::Forge, "1.20.1");
        let report = check(&forge, vec![jar("x.jar", b"nope".to_vec())]);
        assert_eq!(
            kinds(&report),
            vec![(Severity::Warning, IssueKind::Corrupt)]
        );
        // Libraries marked for FML are normal there.
        let lib = zip_bytes(&[(
            "META-INF/MANIFEST.MF",
            b"Manifest-Version: 1.0\nFMLModType: GAMELIBRARY\n",
        )]);
        assert!(check(&forge, vec![jar("lib.jar", lib)]).issues.is_empty());
    }

    #[test]
    fn server_side_and_optional_relations_are_ignored() {
        let report = check(
            &fabric(),
            vec![jar(
                "srv.jar",
                fabric_jar(
                    "srv",
                    "1.0",
                    r#""environment": "server", "depends": {"missing": "*", "minecraft": "1.12"}"#,
                ),
            )],
        );
        assert!(report.issues.is_empty());
        let forge = env(LoaderKind::Forge, "1.20.1");
        let deps = format!(
            "{}{}[[dependencies.m]]\nmodId=\"serveronly\"\nmandatory=true\nside=\"SERVER\"\n",
            toml_dep("m", "optional", "false", "[1,)"),
            toml_dep("m", "minecraft", "true", "[1.20.1,1.20.2)")
        );
        let report = check(
            &forge,
            vec![jar("m.jar", toml_jar(false, "m", "1.0", &deps))],
        );
        assert!(report.issues.is_empty(), "{:#?}", report.issues);
    }

    #[test]
    fn odd_input_never_panics() {
        let weird = [
            fabric_jar("x", "", r#""depends": null, "breaks": [1, 2], "provides": "y""#),
            zip_bytes(&[("fabric.mod.json", br#"[]"#)]),
            zip_bytes(&[("fabric.mod.json", br#"{"schemaVersion": 1, "id": "", "depends": {"": "*"}}"#)]),
            zip_bytes(&[("META-INF/mods.toml", b"[[mods]]\nmodId=5\n[[dependencies.x]]\nmodId=7\n")]),
            zip_bytes(&[("META-INF/mods.toml", b"mods = \"nope\"\ndependencies = 3\n")]),
            zip_bytes(&[("quilt.mod.json", br#"{"quilt_loader": {"id": "q", "depends": [{"id": 5}, null, {"versions": {"any": []}}]}}"#)]),
            vec![0u8; 3],
        ];
        for loader in [
            LoaderKind::Fabric,
            LoaderKind::Quilt,
            LoaderKind::Forge,
            LoaderKind::NeoForge,
            LoaderKind::Vanilla,
        ] {
            for minecraft in ["1.21.1", "24w14a", "", "26.1-snapshot-1"] {
                let jars = weird
                    .iter()
                    .enumerate()
                    .map(|(i, b)| jar(&format!("w{i}.jar"), b.clone()))
                    .collect();
                let _ = check(&env(loader, minecraft), jars);
            }
        }
    }

    #[test]
    fn older_copies_for_crash_attribution() {
        let snapshot = ModSnapshot::new(
            "mods",
            vec![
                jar("jei-new.jar", toml_jar(false, "jei", "15.3.0", "")),
                jar("jei-old.jar", toml_jar(false, "jei", "15.2.0", "")),
            ],
        );
        let older = older_copies(&snapshot, &env(LoaderKind::Forge, "1.20.1"), "jei");
        assert_eq!(older.len(), 1);
        assert_eq!(older[0].file_name, "jei-old.jar");
    }

    /// A warm check of 200 mods (cached jars) must stay far below 200 ms.
    #[test]
    fn warm_check_of_200_mods_is_fast() {
        let dir = tempfile::tempdir().unwrap();
        let mods = dir.path().join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        let module = fabric_jar("lib-module", "1.0.0", "");
        for i in 0..200 {
            let deps = format!(
                r#""depends": {{"fabricloader": ">=0.15", "minecraft": "~1.21", "mod{}": ">=1.0", "lib-module": "*"}}, "breaks": {{"enemy": "<1"}}"#,
                (i + 1) % 200
            );
            let bytes = if i % 20 == 0 {
                let json = fabric_json(
                    &format!("mod{i}"),
                    "1.0.0",
                    &format!(r#"{deps}, "jars": [{{"file": "META-INF/jars/m.jar"}}]"#),
                );
                zip_bytes(&[
                    ("fabric.mod.json", json.as_bytes()),
                    ("META-INF/jars/m.jar", &module),
                ])
            } else {
                fabric_jar(&format!("mod{i}"), "1.0.0", &deps)
            };
            std::fs::write(mods.join(format!("mod{i}.jar")), bytes).unwrap();
        }
        let scanner = ModScanner::new();
        let env = fabric();
        let origins = FileOrigins::new();

        let start = Instant::now();
        let (_, cold) = check_mods_dir(&scanner, &mods, &env, &origins);
        let cold_time = start.elapsed();
        assert!(cold.issues.is_empty(), "{:#?}", cold.issues);
        assert_eq!(cold.checked_files, 200);

        let mut warm_times = Vec::new();
        for _ in 0..5 {
            let start = Instant::now();
            let (snapshot, report) = check_mods_dir(&scanner, &mods, &env, &origins);
            warm_times.push(start.elapsed());
            assert_eq!(report.checked_files, 200);
            assert!(
                Arc::strong_count(&snapshot.jars[0].contents) >= 2,
                "served from the cache"
            );
        }
        let warm = warm_times.iter().max().unwrap();
        println!("mod check, 200 jars: cold {cold_time:?}, warm max {warm:?} ({warm_times:?})");
        assert!(warm.as_millis() < 200, "warm check took {warm:?}");
    }
}

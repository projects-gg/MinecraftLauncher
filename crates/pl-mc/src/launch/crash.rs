//! Crash analysis: turns the end of the game output and the crash report into a diagnosis with
//! a plain Turkish explanation and one-click suggestions. Ported from the legacy launcher's
//! `GameProcessWatcher` heuristics (memory, Java version, graphics driver, mod errors, access
//! denied, silent exits) and extended for mod loader dependency problems and Java mismatches.
//!
//! [`analyze_crash`] also lists the mods the logs point at ([`CrashSuspect`]): Fabric/Quilt
//! dependency resolution errors, Forge/NeoForge dependency and mod loading errors, duplicate
//! mods, mixin failures, the crash report's "Suspected Mods" section, classes compiled for a
//! newer Java and, as a last resort, mod frames of the stack trace. Memory, Java, graphics
//! driver and similar failures name no mods. [`attribute_crash`] then finds the suspects' jars
//! in the mods folder (by file name, mixin config, mod id, display name or class package),
//! adds display names and versions, and turns them into the pre-launch check's fixes
//! ([`ModFix`]).

use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::{Duration, SystemTime};

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::loaders::LoaderKind;
use crate::mods::check::{CheckEnvironment, DisableReason, ModFix, older_copies};
use crate::mods::scan::{DeclaredMod, ModSnapshot, ScannedJar};
use crate::mods::version::{Mismatch, VersionReq, minimum_java};

/// Exits sooner than this count as failed when a crash report was written, even with code 0.
pub const SHORT_RUN: Duration = Duration::from_secs(20);
/// Crash reports older than the launch minus this margin belong to an earlier run.
const CRASH_SCAN_MARGIN: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum CrashKind {
    /// The JVM could not reserve the requested heap.
    MemoryReserve,
    OutOfMemory,
    /// A user-supplied JVM option was rejected.
    InvalidJvmArgument,
    JavaTooOld,
    JavaTooNew,
    GraphicsDriver,
    IncompatibleMods,
    MissingDependency,
    DuplicateMods,
    /// Mixin failures and other mod crashes.
    ModError,
    CorruptedFiles,
    AccessDenied,
    /// Exited without printing anything (often security software).
    NoOutput,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SuggestedAction {
    LowerMaxMemory {
        suggested_mb: Option<u32>,
    },
    IncreaseMaxMemory {
        suggested_mb: Option<u32>,
    },
    ResetJvmArguments,
    /// Switch the instance to automatic Java (downloads the right runtime).
    UseAutomaticJava {
        required_major: Option<u32>,
    },
    UpdateGraphicsDriver,
    TryWithoutMods,
    InstallMods {
        mods: Vec<String>,
    },
    UpdateMods {
        mods: Vec<String>,
    },
    RemoveMods {
        mods: Vec<String>,
    },
    RemoveDuplicateMods {
        mods: Vec<String>,
    },
    AddAntivirusException,
    RepairInstallation,
    OpenCrashReport,
    OpenLogs,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CrashDiagnosis {
    pub kind: CrashKind,
    pub title_tr: String,
    pub hint_tr: String,
    pub suggested_actions: Vec<SuggestedAction>,
    /// Mods named by the error (display names when the loader gives them).
    pub mods: Vec<String>,
    pub exit_code: Option<i32>,
    pub runtime_secs: u64,
    /// The game's crash report (or the JVM's `hs_err_pid*.log`) written during this run.
    pub crash_report: Option<PathBuf>,
    /// Captured output of this run.
    pub log_file: Option<PathBuf>,
    /// Mods the output and the crash report point at, strongest evidence first. Empty when the
    /// failure is not a mod problem (memory, Java, graphics driver, ...). [`attribute_crash`]
    /// adds file names, display names and versions from the jars.
    #[serde(default)]
    pub suspects: Vec<CrashSuspect>,
    /// Repairs in the pre-launch check's vocabulary; filled by [`attribute_crash`].
    #[serde(default)]
    pub mod_fixes: Vec<ModFix>,
}

/// What the logs say a suspected mod did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SuspectReason {
    /// Needs a mod that is not installed (`related` names it).
    RequiresMissing,
    /// Not installed, but another mod (`related`) needs it.
    Missing,
    /// Needs another version of a mod (`related`).
    RequiresOtherVersion,
    /// Installed in a version another mod (`related`) does not accept.
    WrongVersion,
    /// Does not support this Minecraft version.
    IncompatibleMinecraft,
    /// Needs another mod loader version.
    NeedsNewerLoader,
    /// Compiled for, or asking for, a newer Java than the one that ran the game.
    NeedsNewerJava,
    /// Declares itself incompatible with an installed mod (`related`).
    DeclaresIncompatible,
    /// Installed although another mod (`related`) declares itself incompatible with it.
    Incompatible,
    /// Installed more than once.
    Duplicate,
    /// One of its mixins failed to apply or clashed with another mod's mixin.
    MixinFailed,
    /// The mod loader reported that it failed while loading.
    LoadingFailed,
    /// The crash report's own "Suspected Mods" guess.
    NamedInReport,
    /// Its code is in the crashing stack trace (weak evidence, used only when nothing else is
    /// found).
    InStackTrace,
    /// A jar the loader could not open.
    CorruptFile,
}

/// A mod the crash points at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CrashSuspect {
    pub mod_id: Option<String>,
    /// Display name (from the log, replaced by the jar's own name once attributed).
    pub name: Option<String>,
    /// File in the mods folder, when the log names it or [`attribute_crash`] found it.
    pub file: Option<String>,
    pub version: Option<String>,
    pub reason: SuspectReason,
    /// The other mod involved (see [`SuspectReason`]).
    pub related: Option<String>,
    /// The requirement the error quotes, in the loader's syntax when it could be converted
    /// (`>=0.5.0`, `[9.1.12,)`).
    pub requirement: Option<String>,
    /// Mixin config the error names (`sodium.mixins.json`).
    pub mixin_config: Option<String>,
    /// Class the error names (`me.jellysquid.mods.sodium.client.SodiumClientMod`).
    pub class_name: Option<String>,
}

impl CrashSuspect {
    fn new(reason: SuspectReason) -> Self {
        Self {
            mod_id: None,
            name: None,
            file: None,
            version: None,
            reason,
            related: None,
            requirement: None,
            mixin_config: None,
            class_name: None,
        }
    }

    fn id(mut self, id: &str) -> Self {
        self.mod_id = Some(id.trim().to_owned()).filter(|s| !s.is_empty());
        self
    }

    fn named(mut self, name: Option<&str>) -> Self {
        self.name = name.map(|n| n.trim().to_owned()).filter(|s| !s.is_empty());
        self
    }

    fn version(mut self, version: Option<&str>) -> Self {
        self.version = version.map(str::to_owned);
        self
    }

    fn related(mut self, related: &str) -> Self {
        self.related = Some(related.to_owned());
        self
    }

    fn requirement(mut self, requirement: Option<String>) -> Self {
        self.requirement = requirement;
        self
    }

    /// True when both describe the same mod.
    fn same_mod(&self, other: &Self) -> bool {
        let eq = |a: &Option<String>, b: &Option<String>| matches!((a, b), (Some(a), Some(b)) if a.eq_ignore_ascii_case(b));
        eq(&self.mod_id, &other.mod_id)
            || ((self.mod_id.is_none() || other.mod_id.is_none()) && eq(&self.name, &other.name))
            || eq(&self.file, &other.file)
            || eq(&self.mixin_config, &other.mixin_config)
            || eq(&self.class_name, &other.class_name)
    }

    /// Fill what `self` lacks from `other`.
    fn absorb(&mut self, other: Self) {
        let fill = |slot: &mut Option<String>, value: Option<String>| {
            if slot.is_none() {
                *slot = value;
            }
        };
        fill(&mut self.mod_id, other.mod_id);
        fill(&mut self.name, other.name);
        fill(&mut self.file, other.file);
        fill(&mut self.version, other.version);
        fill(&mut self.related, other.related);
        fill(&mut self.requirement, other.requirement);
        fill(&mut self.mixin_config, other.mixin_config);
        fill(&mut self.class_name, other.class_name);
    }
}

/// What the analyzer looks at.
#[derive(Debug, Clone, Default)]
pub struct CrashInput<'a> {
    /// The last lines of the game output.
    pub output: &'a str,
    /// Crash report / JVM error log text, when one was written.
    pub crash_report: Option<&'a str>,
    pub exit_code: Option<i32>,
    pub runtime: Duration,
    /// Major version of the Java that ran the game.
    pub java_major: Option<u32>,
    /// Major version the Minecraft version asks for.
    pub required_java: Option<u32>,
    pub max_memory_mb: Option<u32>,
    /// The JVM arguments the player typed (`LaunchCommand::player_jvm_options`): a JVM that
    /// did not start because of one of them is fixed by resetting them, not by changing the
    /// launcher's memory.
    pub player_jvm_args: &'a [String],
}

/// The legacy rule: a non-zero exit, or a short run that left a crash report.
pub fn is_failure(exit_code: Option<i32>, runtime: Duration, has_crash_report: bool) -> bool {
    exit_code != Some(0) || (runtime < SHORT_RUN && has_crash_report)
}

/// Newest `crash-reports/crash-*.txt` written since `since` (minus a small margin).
pub fn find_crash_report(game_dir: &Path, since: SystemTime) -> Option<PathBuf> {
    newest_file(&game_dir.join("crash-reports"), since, |name| {
        name.starts_with("crash-") && name.ends_with(".txt")
    })
}

/// Newest JVM fatal error log (`hs_err_pid*.log`) written in the game directory since `since`.
pub fn find_jvm_error_log(game_dir: &Path, since: SystemTime) -> Option<PathBuf> {
    newest_file(game_dir, since, |name| {
        name.starts_with("hs_err_pid") && name.ends_with(".log")
    })
}

fn newest_file(dir: &Path, since: SystemTime, accept: impl Fn(&str) -> bool) -> Option<PathBuf> {
    let from = since.checked_sub(CRASH_SCAN_MARGIN).unwrap_or(since);
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|entry| accept(&entry.file_name().to_string_lossy()))
        .filter_map(|entry| {
            let modified = entry.metadata().ok()?.modified().ok()?;
            (modified >= from).then(|| (modified, entry.path()))
        })
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path)
}

static CLASS_FILE_VERSION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"class file version (\d+)").expect("valid regex"));
/// Fabric: "requires version 21 or later of java"; Quilt: "requires version [21, ∞) of java".
static FABRIC_JAVA: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)requires version [\[(]?(\d+)\b[^\n]*? of (?:mod 'Java' \()?java[),! ]")
        .expect("valid regex")
});
/// Minecraft's formatting codes, which Forge before 1.17 leaves in its messages
/// (`Mod §eabc§r requires §6xyz§r`).
static FORMATTING_CODE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"§[0-9a-fk-orA-FK-OR]").expect("valid regex"));
/// The JVM names a refused `-XX:` option without its prefix: "Unrecognized VM option 'UseFoo'",
/// "Improperly specified VM option 'MaxGCPauseMillis'", "Missing +/- setting for VM option
/// 'UseG1GC'", "VM option 'UseZGC' is experimental and must be enabled via ...".
static VM_OPTION_NAMED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"VM option '([^'\r\n]+)'").expect("valid regex"));
/// Options the Java launcher or the JVM quotes whole: "Unrecognized option: -foo", "Invalid
/// maximum heap size: -Xmx4gb", "Invalid thread stack size: -Xss1x. The specified size ...".
static OPTION_QUOTED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:Unrecognized option|Invalid (?:maximum heap|initial heap|initial young generation|maximum young generation|new generation|thread stack|maximum direct memory) size): (\S+)")
        .expect("valid regex")
});
/// A word the Java launcher took for the main class: "Error: Could not find or load main class
/// 2G" (a value typed without its option, an unquoted path with a space).
static MAIN_CLASS_MISSING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Could not find or load main class (\S+)").expect("valid regex"));
/// "Could not reserve enough space for object heap" (old JVMs) and "... for 2097152KB object
/// heap".
static HEAP_RESERVE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)could not reserve enough space for (?:\d+\s*[kmg]?b )?object heap")
        .expect("valid regex")
});
/// The loader's Mixin cannot read the running Java's own classes: "Error loading class:
/// java/util/concurrent/CompletableFuture (java.lang.IllegalArgumentException: Unsupported class
/// file major version 68)".
static JAVA_CLASS_TOO_NEW: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:java|javax|jdk|sun)/[\w/$]+ \(java\.lang\.IllegalArgumentException: Unsupported class file major version \d+")
        .expect("valid regex")
});
/// A native crash inside a graphics driver (`[nvoglv64.dll+0xd5c3a]`). The bare file names also
/// appear in harmless lines (Sodium's graphics adapter line, the crash report's module list).
static GPU_DRIVER_FRAME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\[(?:nvoglv64|nvoglv32|atio6axx|atioglxx|atig6txx|ig9icd64|ig9icd32|ig75icd64|igxelpicd64)\.dll\+0x[0-9a-f]+\]")
        .expect("valid regex")
});
/// A mod declaring an incompatibility (Fabric, NeoForge); not other uses of the words, such as
/// "because mod \"polytone\" is incompatible with it".
static INCOMPATIBLE_DECLARATION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\bMod (?:'[^'\n]+'(?: \([\w.\-]+\))?(?: \S+)?|[\w.\-]+) is incompatible with ")
        .expect("valid regex")
});
/// Lines where Mixin reports a failure. Warnings that merely mention a mixin ("@Mixin target ...
/// was not found", "Method overwrite conflict ... Skipping method") do not count.
static MIXIN_FAILURE_LINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"Mixin (?:apply|prepare) (?:for mod [\w.\-]+ )?failed|Mixin(?:Apply|Prepare)Error|FAILED during [A-Z]|Critical injection failure|Invalid(?:Injection|Mixin|InterfaceMixin|AccessorMixin)Exception|Mixin application of \S+ from .+ has failed")
        .expect("valid regex")
});
/// Quilt Loader: "Indium requires version [3.2.0, ∞) of fabric-renderer-api-v1, which is
/// missing!" (the name is the mod's display name; the jars follow as `- <mods>/x.jar` lines).
static QUILT_REQUIRES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^([^\s\-\[][^\n]*?) (?:transitively )?requires (any version|version [^\n]+?) of ([\w.\-]+), (which is missing|but only [^\n]*? present)")
        .expect("valid regex")
});
/// Quilt Loader: "Duplicate mandatory mod ids [qkl]" and the `- <mods>/x.jar` lines below it.
static QUILT_DUPLICATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^Duplicate mandatory mod ids \[([^\]\n]*)\]((?:\n[ \t]*- [^\n]*)*)")
        .expect("valid regex")
});
static FABRIC_MOD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i:mod) '([^']+)' \(([\w.\-]+)\)").expect("valid regex"));
static FABRIC_MISSING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)requires (?:any version|version [^,]+?) of (?:mod '([^']+)' \(([\w.\-]+)\)|([\w.\-]+)),? which is missing")
        .expect("valid regex")
});
static FABRIC_INSTALL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^\s*- Install ([\w.\-]+),").expect("valid regex"));
static FORGE_DEPENDENCY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"Mod ID: '([^']+)', Requested by: '([^']+)'(?:.*?Actual version: '([^']+)')?")
        .expect("valid regex")
});
/// Forge 1.12: "Mod hahp (Hero Aviation Helicopter Pack (HA/HP)) requires [heroaviation@...]".
static FML_MISSING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"Mod ([\w.\-]+) \((.+?)\) requires \[([\w.\-]+)").expect("valid regex")
});
static DUPLICATE_ID: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:Mod ID: '([^']+)' from mod files|duplicate mod(?: id)?:? '?([\w.\-]+)|same id:? '?([\w.\-]+))")
        .expect("valid regex")
});
/// Forge and NeoForge: "Mod rei requires architectury 13.0.6 or above" + "Currently,
/// architectury is not installed" (or "... is 12.1.3"); optional dependencies say "only
/// supports".
static NEO_REQUIRES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)Mod ([\w.\-]+) (?:requires|only supports) ([\w.\-]+) ([^\n]*?)[ \t]*\n\s*Currently, ([\w.\-]+) is (not installed|\S+)")
        .expect("valid regex")
});
static NEO_INCOMPATIBLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"Mod ([\w.\-]+) is incompatible with ([\w.\-]+)").expect("valid regex")
});

// --- suspect extraction ---

/// Fabric/Quilt: "Mod 'Iris' (iris) 1.8.0 requires version 0.6.0 or later of mod 'Sodium'
/// (sodium), but only the wrong version is present: 0.5.8!" and the "missing" and
/// "incompatible" forms.
/// Built-in targets have no "mod" prefix (Fabric 0.19: "requires any 1.20.x version of
/// 'Minecraft' (minecraft), but only the wrong version is present: 1.21.1!"). Fabric 0.19.3+
/// says "yet a conflicting version is present" where older versions said "but a matching
/// version is present".
static FABRIC_RELATION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"Mod '([^'\n]+)' \(([\w.\-]+)\)(?: (\S+))? (requires|is incompatible with) ([^\n]+?) of (?:(?:mod )?'([^'\n]+)' \(([\w.\-]+)\)|([\w.\-]+)),? (which is missing|but only the wrong version is present: ([^\s!]+)|but only the wrong versions are present: ([^\n!]+)|(?:but a matching|yet a conflicting) version is present: ([^\s!]+))")
        .expect("valid regex")
});
static FABRIC_PROSE_RANGE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\S+) or later,? (?:and|but) (?:earlier than|before|below) (\S+)$")
        .expect("valid regex")
});
static FABRIC_INSTALL_LINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^\s*- Install ([\w.\-]+), ([^\n]+?)\.?$").expect("valid regex")
});
static FABRIC_REPLACE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^\s*- Replace mod '([^'\n]+)' \(([\w.\-]+)\) (\S+) with ([^\n]+?)\.?$")
        .expect("valid regex")
});
/// Fabric Loader before 0.12: "Could not find required mod: modmenu requires {fabric @ [*]}".
static LEGACY_FABRIC_MISSING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)could not find required mod: ([\w.\-]+) requires \{([\w.\-]+) @ \[([^\]]*)\]\}",
    )
    .expect("valid regex")
});
static FORGE_DEPENDENCY_FULL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"Mod ID: '([^']+)', Requested by: '([^']+)', Expected range: '([^']*)', Actual version: '([^']*)'")
        .expect("valid regex")
});
static FORGE_DUPLICATE_FILES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:Mod ID: '([\w.\-]+)' from mod files|Mod ([\w.\-]+) is present in multiple files): ([^\n]+)")
        .expect("valid regex")
});
static CLASS_VERSION_CLASS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"([\w/$.]+) has been compiled by a more recent version of the Java Runtime \(class file version (\d+)")
        .expect("valid regex")
});
/// `sodium.mixins.json:core.MinecraftMixin from mod sodium` (the mod part is added by Fabric and
/// NeoForge; `from mod fluxnautics->@Inject` must not take the arrow's hyphen).
static MIXIN_REF: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"([\w\-.]+\.json):([\w.$]+)(?: from mod ([\w.]+(?:-[\w.]+)*))?")
        .expect("valid regex")
});
/// NeoForge: "Mixin application of com.x.FooMixin from Create Bits 'n' Bobs (bits_n_bobs) has
/// failed".
static MIXIN_APPLICATION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"Mixin application of ([\w.$]+) from (.+?) \(([\w.\-]+)\) has failed")
        .expect("valid regex")
});
static MIXIN_CONFIG_REF: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"in config \[([\w\-.]+\.json)\]").expect("valid regex"));
static MIXIN_APPLY_FOR_MOD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Mixin apply for mod ([\w\-.]+) failed").expect("valid regex"));
static MOD_SECTION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^-- (?:MOD|Mod loading issue for:) ([\w.\-]+) --$").expect("valid regex")
});
static MOD_FILE_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?mi)^\s*Mod file: ([^\n]+?)\s*$").expect("valid regex"));
static CAUGHT_FROM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"Caught exception from ([^(\n]+?) \(([\w.\-]+)\)").expect("valid regex")
});
/// Forge/NeoForge: "Create (create) encountered an error during the common_setup event phase",
/// "Nox Lights (noxlights) encountered an error while dispatching the ... event", NeoForge's
/// "- Amaranth (amaranth) has failed to load correctly" (without the list's dash).
static FAILED_TO_LOAD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"([^\s(:\-][^\n(:]*?) \(([\w.\-]+)\) (?:has failed to load correctly|has class loading errors|encountered an error (?:during|while dispatching) the)")
        .expect("valid regex")
});
/// Fabric: "Could not execute entrypoint stage 'main' due to errors, provided by 'abc'!".
static ENTRYPOINT_FAILED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"Could not execute entrypoint stage '[^'\n]*' due to errors, provided by '([\w.\-]+)'",
    )
    .expect("valid regex")
});
/// The first Java error named in a mod loading section.
static THROWABLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b\w+(?:Exception|Error)\b").expect("valid regex"));
static BROKEN_JAR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:error analyzing \[?([^\]\n]+?\.jar)|file ([^\s'\x22\[\]]+?\.jar) is (?:not a valid|not a jar|corrupt))")
        .expect("valid regex")
});
/// Forge/NeoForge refused a jar: "File mods\x.jar is not a valid mod file" (paths may contain
/// spaces). "is a Fabric mod and cannot be loaded" is only a warning and does not count.
static INVALID_MOD_FILE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"File (\S[^\n]*?\.jar) is (?:not a valid mod file|not a jar file)")
        .expect("valid regex")
});
static SUSPECTED_BLOCK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^[ \t]*Suspected Mods?:[ \t]*\n((?:[ \t]+[^\n]*(?:\n|$))*)")
        .expect("valid regex")
});
static SUSPECTED_LINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^[ \t]+([^\n]+?) \(([\w.\-]+)\), Version: (\S+)").expect("valid regex")
});
/// The one-line form of the system details: "Suspected Mods: NeoForge (neoforge), Rechiseled
/// (rechiseled)".
static SUSPECTED_INLINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^[ \t]*Suspected Mods?:[ \t]*(\S[^\n]*)$").expect("valid regex")
});
static SUSPECTED_ITEM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([^,(\n]+?) \(([\w.\-]+)\)").expect("valid regex"));
/// FML stack frames of mods: `at TRANSFORMER/create@0.5.1.f/com.simibubi...`. The loader's own
/// layers (`MC-BOOTSTRAP/fml_loader@...`, `SECURE-BOOTSTRAP/...`) are not mods.
static FML_FRAME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\bat TRANSFORMER/([a-z][a-z0-9_]{1,63})@[^/\s]+/").expect("valid regex")
});
/// Fabric mixin handler methods carry the mod id: `handler$zza000$sodium$onRender`.
static MIXIN_HANDLER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\$[a-z]{3}\d{3}\$([a-z][a-z0-9_\-]{1,63})\$").expect("valid regex")
});

/// Ids of the game, Java and the loaders: never suspects.
const PLATFORM_IDS: [&str; 12] = [
    "minecraft",
    "java",
    "forge",
    "neoforge",
    "fml",
    "javafml",
    "lowcodefml",
    "mcp",
    "fabricloader",
    "fabric-loader",
    "quilt_loader",
    "mixinextras",
];

/// Classes of the game, the loaders and Java: an error in them names no mod.
const PLATFORM_CLASSES: [&str; 11] = [
    "net.minecraft.",
    "com.mojang.",
    "java.",
    "javax.",
    "jdk.",
    "sun.",
    "net.fabricmc.loader.",
    "org.quiltmc.loader.",
    "net.minecraftforge.",
    "net.neoforged.",
    "cpw.mods.",
];

/// Classify a failed run.
pub fn analyze_crash(input: &CrashInput<'_>) -> CrashDiagnosis {
    let mut text = String::with_capacity(
        input.output.len() + input.crash_report.map(str::len).unwrap_or_default() + 1,
    );
    text.push_str(input.output);
    text.push('\n');
    if let Some(report) = input.crash_report {
        text.push_str(report);
    }
    if text.contains('§') {
        text = FORMATTING_CODE.replace_all(&text, "").into_owned();
    }
    let lower = text.to_lowercase();
    let has = |needle: &str| lower.contains(needle);
    let any = |needles: &[&str]| needles.iter().any(|n| lower.contains(n));

    let (kind, mods, required) = if player_argument_failed(&text, &lower, input.player_jvm_args) {
        (CrashKind::InvalidJvmArgument, Vec::new(), None)
    } else if any(&[
        "unsupportedclassversionerror",
        "has been compiled by a more recent version of the java",
        "of java, but only the wrong version is present",
    ]) || (has("unrecognized option: --")
        && input.java_major.is_none_or(|major| major < 9))
        // The version's own options of a newer Java (26.1's `--sun-misc-unsafe-memory-access`
        // on Java 17): not the player's (checked above), and the Java is older than required.
        || (any(&["unrecognized option:", "unrecognized vm option"])
            && input
                .java_major
                .zip(input.required_java)
                .is_some_and(|(have, need)| have < need))
        || FABRIC_JAVA.is_match(&text)
    {
        let required = CLASS_FILE_VERSION
            .captures(&text)
            .and_then(|c| c[1].parse::<u32>().ok())
            .filter(|v| *v > 44)
            .map(|v| v - 44)
            .or_else(|| FABRIC_JAVA.captures(&text).and_then(|c| c[1].parse().ok()))
            .or(input.required_java);
        (CrashKind::JavaTooOld, Vec::new(), required)
    } else if any(&[
        "cannot be cast to class java.net.urlclassloader",
        "cannot be cast to java.net.urlclassloader",
        "the security manager is deprecated",
        "does not \"opens java.",
        "module java.base does not \"opens",
    ]) || JAVA_CLASS_TOO_NEW.is_match(&text)
    {
        (CrashKind::JavaTooNew, Vec::new(), input.required_java)
    } else if any(&[
        "unrecognized vm option",
        "unrecognized option:",
        "improperly specified vm option",
        "invalid initial heap size",
        "could not find agent library",
    ]) {
        (CrashKind::InvalidJvmArgument, Vec::new(), None)
    } else if HEAP_RESERVE.is_match(&text)
        || any(&[
            "invalid maximum heap size",
            "initial heap size set to a larger value than the maximum heap size",
            "insufficient memory for the java runtime environment",
            "the specified size exceeds the maximum representable size",
        ])
    {
        (CrashKind::MemoryReserve, Vec::new(), None)
    } else if has("outofmemoryerror") {
        (CrashKind::OutOfMemory, Vec::new(), None)
    } else if any(&[
        "found duplicate mods",
        "duplicatemodsfoundexception",
        "duplicate mods found",
        "duplicate mod id",
        "duplicate mandatory mod ids",
        "mods with the same id",
        "is duplicated",
    ]) {
        let mut mods: Vec<String> = DUPLICATE_ID
            .captures_iter(&text)
            .filter_map(|c| c.get(1).or_else(|| c.get(2)).or_else(|| c.get(3)))
            .map(|m| m.as_str().to_owned())
            .collect();
        for c in QUILT_DUPLICATE.captures_iter(&text) {
            mods.extend(c[1].split(',').map(|id| id.trim().to_owned()));
        }
        if mods.is_empty() {
            mods = fabric_mod_names(&text);
        }
        (CrashKind::DuplicateMods, mods, None)
    } else if let Some(missing) = missing_dependencies(&text, &lower) {
        (CrashKind::MissingDependency, missing, None)
    } else if any(&[
        "incompatible mods found",
        "incompatible mod set",
        "some of your mods are incompatible",
        "missing or unsupported mandatory dependencies",
        "missingmodsexception",
    ]) || text
        .lines()
        .any(|l| l.contains("but only the wrong version") && !fabric_warning(l))
        || INCOMPATIBLE_DECLARATION.is_match(&text)
        || NEO_REQUIRES.is_match(&text)
        || QUILT_REQUIRES.is_match(&text)
    {
        let mut mods = fabric_mod_names(&text);
        mods.extend(
            FORGE_DEPENDENCY
                .captures_iter(&text)
                .filter_map(|c| c.get(2).map(|m| m.as_str().to_owned())),
        );
        mods.extend(NEO_REQUIRES.captures_iter(&text).map(|c| c[1].to_owned()));
        mods.extend(
            NEO_INCOMPATIBLE
                .captures_iter(&text)
                .map(|c| c[1].to_owned()),
        );
        mods.extend(
            QUILT_REQUIRES
                .captures_iter(&text)
                .filter(|c| !c[1].starts_with("Mod '"))
                .map(|c| c[1].to_owned()),
        );
        (CrashKind::IncompatibleMods, mods, None)
    } else if any(&[
        "does not support required extensions",
        "does not have required feature",
        "failed to create window",
        "pixel format not accelerated",
        "couldn't set pixel format",
        "no opengl context found",
        "the driver does not appear to support opengl",
        "glfw error 65542",
        "glfw error 65543",
        "glfw error 65545",
        "exception_access_violation",
    ]) || GPU_DRIVER_FRAME.is_match(&text)
        || graphics_device_missing(&lower)
        || input.exit_code == Some(-1073741819)
    {
        (CrashKind::GraphicsDriver, Vec::new(), None)
    } else if any(&[
        "mixin apply failed",
        "mixinapplyerror",
        "mixin prepare failed",
        "mixintransformererror",
        "invalidinjectionexception",
        "net.fabricmc.loader.impl.formattedexception",
        "modloadingexception",
        "loadingfailedexception",
        "caught exception from",
        "mod loading has failed",
        "has failed to load correctly",
        "encountered an error during the",
        "could not execute entrypoint stage",
    ]) || MIXIN_FAILURE_LINE.is_match(&text)
    {
        // Named from the suspects below: the mods whose code failed.
        (CrashKind::ModError, Vec::new(), None)
    } else if any(&[
        "java.util.zip.zipexception",
        "zip end header not found",
        "invalid loc header",
        "error in opening zip file",
        "could not find or load main class",
        "classnotfoundexception: net.minecraft.client",
    ]) {
        (CrashKind::CorruptedFiles, Vec::new(), None)
    } else if any(&[
        "access is denied",
        "accessdeniedexception",
        "unauthorizedaccessexception",
        "erişim engellendi",
        "operation not permitted",
    ]) {
        (CrashKind::AccessDenied, Vec::new(), None)
    } else if any(&[
        "error occurred during initialization of vm",
        "could not create the java virtual machine",
    ]) {
        (CrashKind::MemoryReserve, Vec::new(), None)
    } else if input.output.trim().is_empty() && input.crash_report.is_none() {
        (CrashKind::NoOutput, Vec::new(), None)
    } else {
        (CrashKind::Unknown, Vec::new(), None)
    };

    let suspects = collect_suspects(&text, kind);
    let mods = if kind == CrashKind::ModError {
        mod_error_names(&suspects)
    } else {
        mods
    };
    let mods = dedup_names(mods);
    let (title_tr, hint_tr, mut suggested_actions) = describe(kind, &mods, required, input);
    if input.crash_report.is_some() {
        suggested_actions.push(SuggestedAction::OpenCrashReport);
    }
    suggested_actions.push(SuggestedAction::OpenLogs);
    CrashDiagnosis {
        kind,
        title_tr,
        hint_tr,
        suggested_actions,
        mods,
        exit_code: input.exit_code,
        runtime_secs: input.runtime.as_secs(),
        crash_report: None,
        log_file: None,
        suspects,
        mod_fixes: Vec::new(),
    }
}

/// The JVM did not start because of one of the player's own JVM arguments: the option it
/// refuses is theirs, a stray word of theirs became the main class, or the heap, agent or
/// collector options that failed are theirs. Resetting the arguments is the fix; a lower memory
/// setting would not help (the player's options come after the launcher's and win).
fn player_argument_failed(text: &str, lower: &str, player: &[String]) -> bool {
    if player.is_empty() {
        return false;
    }
    let theirs = |quoted: &str| {
        let quoted = quoted.trim_end_matches(['.', ',', ';']);
        player.iter().any(|arg| arg == quoted)
    };
    // `-XX:+UseFoo`, `-XX:-UseFoo` and `-XX:Foo=1` are named `UseFoo` / `Foo`.
    let vm_option = |arg: &str| -> Option<String> {
        let rest = arg.strip_prefix("-XX:")?;
        let rest = rest.strip_prefix(['+', '-']).unwrap_or(rest);
        rest.split('=').next().map(str::to_owned)
    };
    let named: Vec<String> = player.iter().filter_map(|arg| vm_option(arg)).collect();
    if VM_OPTION_NAMED
        .captures_iter(text)
        .any(|c| named.iter().any(|name| *name == c[1]))
        || OPTION_QUOTED.captures_iter(text).any(|c| theirs(&c[1]))
        || MAIN_CLASS_MISSING
            .captures_iter(text)
            .any(|c| theirs(&c[1]))
    {
        return true;
    }
    let sets = |prefixes: &[&str]| {
        player
            .iter()
            .any(|arg| prefixes.iter().any(|prefix| arg.starts_with(prefix)))
    };
    let max_heap = sets(&["-Xmx", "-XX:MaxHeapSize", "-XX:MaxRAM"]);
    let heap = max_heap
        || sets(&[
            "-Xms",
            "-Xmn",
            "-XX:InitialHeapSize",
            "-XX:MinHeapSize",
            "-XX:NewSize",
            "-XX:MaxNewSize",
            "-XX:InitialRAM",
            "-XX:MinRAM",
        ]);
    let any = |needles: &[&str]| needles.iter().any(|n| lower.contains(n));
    (max_heap
        && (HEAP_RESERVE.is_match(text)
            || any(&["the specified size exceeds the maximum representable size"])))
        || (heap
            && any(&[
                "initial heap size set to a larger value than the maximum heap size",
                "incompatible minimum and maximum heap sizes",
                "too small initial heap",
                "too small maximum heap",
            ]))
        || (sets(&["-javaagent:", "-agentlib:", "-agentpath:"])
            && any(&[
                "agent library failed to init",
                "could not find agent library",
                "error opening zip file or jar manifest missing",
            ]))
        || (player
            .iter()
            .any(|arg| arg.starts_with("-XX:+Use") && arg.ends_with("GC"))
            && any(&["conflicting collector combinations"]))
}

/// Fabric's startup warnings ("recommends", "conflicts with"): the game keeps loading after them.
fn fabric_warning(line: &str) -> bool {
    line.contains(" recommends ")
        || line.contains(" conflicts with ")
        || line.contains("You should")
}

fn fabric_mod_names(text: &str) -> Vec<String> {
    text.lines()
        .filter(|line| !fabric_warning(line))
        .flat_map(|line| FABRIC_MOD.captures_iter(line))
        .map(|c| c[1].to_owned())
        .filter(|name| {
            !name.eq_ignore_ascii_case("minecraft") && !name.eq_ignore_ascii_case("java")
        })
        .collect()
}

/// The game, Java or a mod loader.
fn is_platform_id(id: &str) -> bool {
    PLATFORM_IDS.iter().any(|p| p.eq_ignore_ascii_case(id))
}

/// "Can't getDevice() before it was initialized" means the game could not set up the graphics
/// device, except right after "Failed to get system info for Render Extensions": Minecraft
/// 1.21.5+ logs that harmless warning when a crash report is written before the window exists.
fn graphics_device_missing(lower: &str) -> bool {
    const NEEDLE: &str = "can't getdevice() before it was initialized";
    if !lower.contains(NEEDLE) {
        return false;
    }
    let mut previous = "";
    for line in lower.lines() {
        if line.contains(NEEDLE)
            && !line.contains("render extensions")
            && !previous.contains("failed to get system info for")
        {
            return true;
        }
        if !line.trim().is_empty() {
            previous = line;
        }
    }
    false
}

/// `sodium.mixins.json` → `sodium` (the usual `<mod id>.mixins.json` naming).
fn config_mod_name(config: &str) -> Option<String> {
    config
        .strip_suffix(".mixins.json")
        .filter(|stem| {
            !stem.is_empty()
                && stem
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        })
        .map(str::to_owned)
}

/// Names for a mod error's texts: the mods whose code failed, else whatever the logs pointed at.
fn mod_error_names(suspects: &[CrashSuspect]) -> Vec<String> {
    let label = |s: &CrashSuspect| {
        s.name
            .clone()
            .or_else(|| s.mod_id.clone())
            .or_else(|| s.mixin_config.as_deref().and_then(config_mod_name))
    };
    let failed: Vec<String> = suspects
        .iter()
        .filter(|s| {
            matches!(
                s.reason,
                SuspectReason::MixinFailed
                    | SuspectReason::LoadingFailed
                    | SuspectReason::CorruptFile
            )
        })
        .filter_map(label)
        .collect();
    if failed.is_empty() {
        suspects.iter().filter_map(label).collect()
    } else {
        failed
    }
}

/// Mods reported as missing by Fabric/Quilt, Forge 1.13+ or legacy FML. A missing game, Java or
/// loader version is an incompatibility, not a mod to install.
fn missing_dependencies(text: &str, lower: &str) -> Option<Vec<String>> {
    let mut missing: Vec<String> = Vec::new();
    for c in FABRIC_MISSING.captures_iter(text) {
        if let Some(name) = c.get(1).or_else(|| c.get(3)) {
            missing.push(name.as_str().to_owned());
        }
    }
    for c in QUILT_REQUIRES.captures_iter(text) {
        if &c[4] == "which is missing" && !is_platform_id(&c[3]) && !c[1].starts_with("Mod '") {
            missing.push(c[3].to_owned());
        }
    }
    missing.extend(FABRIC_INSTALL.captures_iter(text).map(|c| c[1].to_owned()));
    for c in FORGE_DEPENDENCY.captures_iter(text) {
        if c.get(3)
            .is_some_and(|actual| actual.as_str() == "[MISSING]")
        {
            missing.push(c[1].to_owned());
        }
    }
    if lower.contains("missingmodsexception") || lower.contains("missing mods") {
        missing.extend(FML_MISSING.captures_iter(text).map(|c| c[3].to_owned()));
    }
    missing.extend(
        NEO_REQUIRES
            .captures_iter(text)
            .filter(|c| &c[5] == "not installed")
            .map(|c| c[2].to_owned()),
    );
    // Fabric's "recommends ..., which is missing!" warnings do not stop the game.
    let requires_missing = || {
        text.lines().any(|line| {
            line.contains("which is missing")
                && line.contains(" requires ")
                && !QUILT_REQUIRES
                    .captures(line)
                    .is_some_and(|c| is_platform_id(&c[3]))
        })
    };
    let found =
        !missing.is_empty() || lower.contains("could not find required mod") || requires_missing();
    found.then_some(missing)
}

fn dedup_names(names: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for name in names {
        let name = name.trim().trim_matches('\'').to_owned();
        if !name.is_empty() && !out.iter().any(|n| n.eq_ignore_ascii_case(&name)) {
            out.push(name);
        }
        if out.len() >= 8 {
            break;
        }
    }
    out
}

fn list(mods: &[String]) -> String {
    mods.join(", ")
}

fn round_mb(value: u32) -> u32 {
    (value / 256).max(4) * 256
}

/// Turkish title, hint and actions for a kind.
fn describe(
    kind: CrashKind,
    mods: &[String],
    required: Option<u32>,
    input: &CrashInput<'_>,
) -> (String, String, Vec<SuggestedAction>) {
    let s = |t: &str| t.to_owned();
    match kind {
        CrashKind::MemoryReserve => (
            s("Ayrılan bellek çok yüksek"),
            s(
                "Oyuna ayrılan en yüksek bellek, bilgisayarınızın şu anda verebileceğinden fazla. \
               Kurulum ayarlarından en yüksek bellek değerini düşürüp yeniden deneyin.",
            ),
            vec![SuggestedAction::LowerMaxMemory {
                suggested_mb: input.max_memory_mb.map(|m| round_mb(m / 2).max(1024)),
            }],
        ),
        CrashKind::OutOfMemory => (
            s("Oyunun belleği yetmedi"),
            s(
                "Oyun, kendisine ayrılan bellek dolduğu için kapandı. Kurulum ayarlarından ayrılan \
               belleği artırın ya da arka planda açık uygulamaları kapatıp yeniden deneyin.",
            ),
            vec![SuggestedAction::IncreaseMaxMemory {
                suggested_mb: input.max_memory_mb.map(|m| round_mb(m + m / 2)),
            }],
        ),
        CrashKind::InvalidJvmArgument => (
            s("Java ayarlarında geçersiz bir değer var"),
            s(
                "Kurulumun Java argümanlarından biri oyunun açılmasını engelledi. Java \
               argümanlarını varsayılana döndürüp yeniden deneyin.",
            ),
            vec![SuggestedAction::ResetJvmArguments],
        ),
        CrashKind::JavaTooOld => {
            let need = match required {
                Some(major) => format!("Bu sürüm Java {major} veya daha yenisini gerektiriyor. "),
                None => s("Bu sürüm daha yeni bir Java gerektiriyor. "),
            };
            (
                s("Java sürümü bu oyun için eski"),
                format!(
                    "{need}Kurulum ayarlarında Java seçimini otomatik yapın; uygun Java sizin \
                     için indirilir."
                ),
                vec![SuggestedAction::UseAutomaticJava {
                    required_major: required,
                }],
            )
        }
        CrashKind::JavaTooNew => {
            let need = match (required, input.java_major) {
                (Some(need), Some(have)) if have > need => format!(
                    "Bu sürüm Java {need} ile çalışıyor, ancak oyun Java {have} ile başlatıldı. "
                ),
                _ => s("Bu sürüm ya da modları daha eski bir Java ile çalışıyor. "),
            };
            (
                s("Java sürümü bu oyun için çok yeni"),
                format!(
                    "{need}Kurulum ayarlarında Java seçimini otomatik yapın; uygun Java sizin \
                     için indirilir."
                ),
                vec![SuggestedAction::UseAutomaticJava {
                    required_major: required,
                }],
            )
        }
        CrashKind::GraphicsDriver => (
            s("Ekran kartı hazırlanamadı"),
            s(
                "Oyun ekran kartınızı hazırlayamadan kapandı. Bu genellikle eski bir ekran kartı \
               sürücüsünden kaynaklanır: sürücünüzü güncelleyip yeniden deneyin. Sorun sürerse \
               modsuz (vanilla) bir kurulumla deneyin.",
            ),
            vec![
                SuggestedAction::UpdateGraphicsDriver,
                SuggestedAction::TryWithoutMods,
            ],
        ),
        CrashKind::IncompatibleMods => {
            let hint = if mods.is_empty() {
                s(
                    "Kurulu modlardan bazıları bu Minecraft sürümüyle ya da birbirleriyle uyumlu \
                   değil. Modlarınızı güncelleyin ya da sorunlu olanları kaldırın; oyun modsuz \
                   açılıyorsa sorun modlardadır.",
                )
            } else {
                format!(
                    "Şu modlar bu Minecraft sürümüyle ya da birbirleriyle uyumlu değil: {}. Bu \
                     modları güncelleyin ya da kaldırın, ardından yeniden deneyin.",
                    list(mods)
                )
            };
            let mut actions = Vec::new();
            if !mods.is_empty() {
                actions.push(SuggestedAction::UpdateMods {
                    mods: mods.to_vec(),
                });
                actions.push(SuggestedAction::RemoveMods {
                    mods: mods.to_vec(),
                });
            }
            actions.push(SuggestedAction::TryWithoutMods);
            (s("Bazı modlar uyumsuz"), hint, actions)
        }
        CrashKind::MissingDependency => {
            let hint = if mods.is_empty() {
                s(
                    "Bazı modların çalışması için gereken başka modlar eksik. Eksik modları kurup \
                   yeniden deneyin.",
                )
            } else {
                format!(
                    "Bazı modların çalışması için başka modlar gerekiyor. Eksik olanlar: {}. \
                     Bunları kurup yeniden deneyin.",
                    list(mods)
                )
            };
            (
                s("Gerekli bir mod eksik"),
                hint,
                vec![SuggestedAction::InstallMods {
                    mods: mods.to_vec(),
                }],
            )
        }
        CrashKind::DuplicateMods => {
            let hint = if mods.is_empty() {
                s(
                    "Mod klasöründe aynı modun birden fazla kopyası var. Fazla kopyaları kaldırıp \
                   yeniden deneyin.",
                )
            } else {
                format!(
                    "Mod klasöründe aynı modun birden fazla kopyası var: {}. Fazla kopyaları \
                     kaldırıp yeniden deneyin.",
                    list(mods)
                )
            };
            (
                s("Aynı mod birden fazla kez yüklü"),
                hint,
                vec![SuggestedAction::RemoveDuplicateMods {
                    mods: mods.to_vec(),
                }],
            )
        }
        CrashKind::ModError => {
            let hint = match mods.len() {
                0 => s(
                    "Kurulu modlardan biri bu sürümle uyuşmuyor ya da hatalı çalışıyor. \
                        Modlarınızı güncelleyin; oyun modsuz (vanilla) açılıyorsa sorun mod \
                        tarafındadır.",
                ),
                1 => format!(
                    "Şu mod oyunun açılmasını engelledi: {}. Modu güncelleyin ya da kaldırıp \
                     yeniden deneyin.",
                    mods[0]
                ),
                _ => format!(
                    "Şu modlar oyunun açılmasını engelledi: {}. Bu modları güncelleyin ya da \
                     kaldırıp yeniden deneyin.",
                    list(mods)
                ),
            };
            let mut actions = Vec::new();
            if !mods.is_empty() {
                actions.push(SuggestedAction::UpdateMods {
                    mods: mods.to_vec(),
                });
                actions.push(SuggestedAction::RemoveMods {
                    mods: mods.to_vec(),
                });
            }
            actions.push(SuggestedAction::TryWithoutMods);
            (s("Bir mod oyunun açılmasını engelledi"), hint, actions)
        }
        CrashKind::CorruptedFiles => (
            s("Oyun dosyaları bozuk görünüyor"),
            s("Oyunun bazı dosyaları eksik ya da bozuk. Kurulumu onarıp yeniden deneyin."),
            vec![SuggestedAction::RepairInstallation],
        ),
        CrashKind::AccessDenied => (
            s("Oyun dosyalarına erişilemedi"),
            s(
                "Oyun bazı dosyalara erişemediği için kapandı. Güvenlik yazılımınız başlatıcıyı ya \
               da Java'yı engelliyor olabilir; başlatıcıyı güvenlik yazılımının istisnalarına \
               ekleyip yeniden deneyin.",
            ),
            vec![SuggestedAction::AddAntivirusException],
        ),
        CrashKind::NoOutput => (
            s("Oyun hiç açılmadan kapandı"),
            s(
                "Oyun hiçbir çıktı üretmeden kapandı. Güvenlik yazılımınız Java'yı engelliyor \
               olabilir; başlatıcıyı istisnalara ekleyip yeniden deneyin. Sorun sürerse kurulumu \
               onarın.",
            ),
            vec![
                SuggestedAction::AddAntivirusException,
                SuggestedAction::RepairInstallation,
            ],
        ),
        CrashKind::Unknown => (
            s("Oyun beklenmedik şekilde kapandı"),
            s(
                "Oyun bir hata nedeniyle kapandı. Ayrıntılar çökme raporunda ve oyun kaydında yer \
               alıyor; sorun sürerse kaydı destek ekibiyle paylaşabilirsiniz.",
            ),
            Vec::new(),
        ),
    }
}

// --- suspects ----------------------------------------------------------------------------------

/// Collected suspects, merged by mod and reason, in the order found.
#[derive(Default)]
struct Suspects(Vec<CrashSuspect>);

impl Suspects {
    fn push(&mut self, suspect: CrashSuspect) {
        let platform = |id: &Option<String>| {
            id.as_deref()
                .is_some_and(|id| PLATFORM_IDS.iter().any(|p| p.eq_ignore_ascii_case(id)))
        };
        if platform(&suspect.mod_id) {
            return;
        }
        match self
            .0
            .iter_mut()
            .find(|s| s.reason == suspect.reason && s.same_mod(&suspect))
        {
            Some(existing) => existing.absorb(suspect),
            None => self.0.push(suspect),
        }
    }

    fn has(&self, reason: SuspectReason) -> bool {
        self.0.iter().any(|s| s.reason == reason)
    }
}

/// Failures that are not mod problems name no mods.
fn names_mods(kind: CrashKind) -> bool {
    !matches!(
        kind,
        CrashKind::MemoryReserve
            | CrashKind::OutOfMemory
            | CrashKind::InvalidJvmArgument
            | CrashKind::JavaTooNew
            | CrashKind::GraphicsDriver
            | CrashKind::AccessDenied
            | CrashKind::NoOutput
    )
}

fn collect_suspects(text: &str, kind: CrashKind) -> Vec<CrashSuspect> {
    if !names_mods(kind) {
        return Vec::new();
    }
    let text = text.replace("\r\n", "\n");
    let mut found = Suspects::default();
    fabric_resolution(&text, &mut found);
    quilt_resolution(&text, &mut found);
    forge_dependencies(&text, &mut found);
    neoforge_dependencies(&text, &mut found);
    duplicate_mods(&text, kind, &mut found);
    if kind == CrashKind::JavaTooOld {
        class_versions(&text, &mut found);
    }
    if matches!(kind, CrashKind::ModError | CrashKind::Unknown) {
        mixin_failures(&text, &mut found);
    }
    let knock_on = loading_failures(&text, &mut found);
    invalid_mod_files(&text, &mut found);
    if kind == CrashKind::CorruptedFiles {
        broken_jars(&text, &mut found);
    }
    // Mods that only failed because a class broken by another mod could not load: named only
    // when nothing failed first-hand.
    if !found.has(SuspectReason::MixinFailed) && !found.has(SuspectReason::LoadingFailed) {
        for suspect in knock_on {
            found.push(suspect);
        }
    }
    suspected_section(&text, &mut found);
    if found.0.is_empty() && matches!(kind, CrashKind::ModError | CrashKind::Unknown) {
        stack_frames(&text, &mut found);
    }
    found.0.truncate(16);
    found.0
}

/// Fabric's wording of a requirement → a Fabric predicate (`version 0.6.0 or later` →
/// `>=0.6.0`). `None` for "any version" and for wordings not understood.
fn fabric_prose_requirement(prose: &str) -> Option<String> {
    let prose = prose.trim().trim_end_matches(['.', '!', ',']).trim();
    let rest = prose.strip_prefix("version ")?;
    let req = if let Some(c) = FABRIC_PROSE_RANGE.captures(rest) {
        format!(">={} <{}", &c[1], &c[2])
    } else if let Some(v) = rest.strip_suffix(" or later") {
        format!(">={v}")
    } else if let Some(v) = rest.strip_suffix(" or earlier") {
        format!("<={v}")
    } else if let Some(v) = rest.strip_prefix("above ") {
        format!(">{v}")
    } else if let Some(v) = rest.strip_prefix("below ") {
        format!("<{v}")
    } else if !rest.contains(' ') {
        rest.to_owned()
    } else {
        return None;
    };
    (VersionReq::fabric(&req) != VersionReq::Unknown).then_some(req)
}

/// Quilt's wording (`version [3.2.0, ∞)`) → a Fabric predicate (`>=3.2.0`).
fn quilt_prose_requirement(prose: &str) -> Option<String> {
    let rest = prose.trim().strip_prefix("version ")?.trim();
    let open = rest.chars().next()?;
    let close = rest.chars().next_back()?;
    if !matches!(open, '[' | '(') || !matches!(close, ']' | ')') || rest.len() < 2 {
        return fabric_prose_requirement(prose);
    }
    let (low, high) = rest[1..rest.len() - 1].split_once(',')?;
    let bound = |v: &str| {
        let v = v.trim();
        (!v.is_empty() && !v.contains('∞')).then(|| v.to_owned())
    };
    let mut parts = Vec::new();
    if let Some(v) = bound(low) {
        parts.push(format!("{}{v}", if open == '[' { ">=" } else { ">" }));
    }
    if let Some(v) = bound(high) {
        parts.push(format!("{}{v}", if close == ']' { "<=" } else { "<" }));
    }
    let req = parts.join(" ");
    (!req.is_empty() && VersionReq::fabric(&req) != VersionReq::Unknown).then_some(req)
}

/// NeoForge's wording of a version range → a Maven range: `13.0.6 or above` → `[13.0.6,)`,
/// `1.0 or above, and below 2.0` → `[1.0,2.0)`, `between 1 and 2 (inclusive)` → `[1,2]`, an
/// exact version `21.1.132` → `[21.1.132]`.
fn neoforge_prose_requirement(prose: &str) -> Option<String> {
    let prose = prose.trim().trim_end_matches('.').trim();
    let above_and = |rest: &str| -> Option<(String, String)> {
        let (low, high) = rest.split_once(", and ")?;
        Some((low.to_owned(), high.strip_suffix(" or below")?.to_owned()))
    };
    let req = if let Some((low, high)) = prose.split_once(" or above, and below ") {
        format!("[{low},{high})")
    } else if let Some((low, high)) = prose.strip_prefix("above ").and_then(above_and) {
        format!("({low},{high}]")
    } else if let Some(rest) = prose.strip_prefix("between ") {
        let (range, open, close) = match rest.strip_suffix(" (inclusive)") {
            Some(range) => (range, '[', ']'),
            None => (rest.strip_suffix(" (exclusive)")?, '(', ')'),
        };
        let (low, high) = range.split_once(" and ")?;
        format!("{open}{low},{high}{close}")
    } else if let Some(v) = prose.strip_suffix(" or above") {
        format!("[{v},)")
    } else if let Some(v) = prose.strip_suffix(" or below") {
        format!("(,{v}]")
    } else if let Some(v) = prose.strip_prefix("above ") {
        format!("({v},)")
    } else if let Some(v) = prose.strip_prefix("below ") {
        format!("(,{v})")
    } else if !prose.is_empty() && prose != "any" && !prose.contains(char::is_whitespace) {
        format!("[{prose}]")
    } else {
        return None;
    };
    (VersionReq::maven(Some(&req)) != VersionReq::Unknown).then_some(req)
}

/// FML prints an exact version (`[1.0]`) as `[1.0,1.0]`, which Maven ranges do not allow.
fn exact_maven_range(range: &str) -> String {
    let same = range
        .strip_prefix('[')
        .and_then(|r| r.strip_suffix(']'))
        .and_then(|r| r.split_once(','))
        .filter(|(low, high)| !low.trim().is_empty() && low.trim() == high.trim());
    match same {
        Some((version, _)) => format!("[{}]", version.trim()),
        None => range.to_owned(),
    }
}

/// True when `id` names the mod loader `loader` (as a dependency target).
fn is_loader_of(loader: LoaderKind, id: &str) -> bool {
    matches!(
        (loader, id),
        (LoaderKind::Forge, "forge")
            | (LoaderKind::NeoForge, "neoforge")
            | (LoaderKind::Fabric, "fabricloader" | "fabric-loader")
            | (LoaderKind::Quilt, "quilt_loader")
    )
}

/// True when `found` (a loader version) is certainly below what `req` accepts.
fn needs_newer_loader(req: VersionReq, found: &str) -> bool {
    req.mismatch(found) == Mismatch::TooOld
}

fn fabric_resolution(text: &str, found: &mut Suspects) {
    for c in FABRIC_RELATION.captures_iter(text) {
        let (name, id) = (&c[1], &c[2]);
        let version = c.get(3).map(|m| m.as_str());
        let target_name = c.get(6).map(|m| m.as_str());
        let Some(target) = c.get(7).or(c.get(8)).map(|m| m.as_str()) else {
            continue;
        };
        let requirement = fabric_prose_requirement(&c[5]);
        let requirer = |reason| {
            CrashSuspect::new(reason)
                .id(id)
                .named(Some(name))
                .version(version)
                .related(target)
                .requirement(requirement.clone())
        };
        let outcome = &c[9];
        let wrong_version = c.get(10).or(c.get(11)).map(|m| m.as_str().trim());
        if &c[4] == "is incompatible with" {
            if let Some(present) = c.get(12) {
                found.push(requirer(SuspectReason::DeclaresIncompatible));
                found.push(
                    CrashSuspect::new(SuspectReason::Incompatible)
                        .id(target)
                        .named(target_name)
                        .version(Some(present.as_str()))
                        .related(id)
                        .requirement(requirement.clone()),
                );
            }
            continue;
        }
        match target {
            "minecraft" => found.push(requirer(SuspectReason::IncompatibleMinecraft)),
            "java" => found.push(requirer(SuspectReason::NeedsNewerJava)),
            "fabricloader" | "fabric-loader" | "quilt_loader" => {
                // Without a readable requirement the direction is unknown.
                let older = match (wrong_version, requirement.as_deref()) {
                    (None, _) => true,
                    (Some(v), Some(r)) => needs_newer_loader(VersionReq::fabric(r), v),
                    (Some(_), None) => false,
                };
                found.push(requirer(if older {
                    SuspectReason::NeedsNewerLoader
                } else {
                    SuspectReason::RequiresOtherVersion
                }));
            }
            _ if outcome == "which is missing" => {
                found.push(requirer(SuspectReason::RequiresMissing));
                found.push(
                    CrashSuspect::new(SuspectReason::Missing)
                        .id(target)
                        .named(target_name)
                        .related(id)
                        .requirement(requirement.clone()),
                );
            }
            _ => {
                found.push(requirer(SuspectReason::RequiresOtherVersion));
                found.push(
                    CrashSuspect::new(SuspectReason::WrongVersion)
                        .id(target)
                        .named(target_name)
                        .version(wrong_version)
                        .related(id)
                        .requirement(requirement.clone()),
                );
            }
        }
    }
    for c in LEGACY_FABRIC_MISSING.captures_iter(text) {
        let requirement = Some(c[3].trim())
            .filter(|r| !r.is_empty() && *r != "*")
            .map(str::to_owned)
            .filter(|r| VersionReq::fabric(r) != VersionReq::Unknown);
        found.push(
            CrashSuspect::new(SuspectReason::RequiresMissing)
                .id(&c[1])
                .related(&c[2]),
        );
        found.push(
            CrashSuspect::new(SuspectReason::Missing)
                .id(&c[2])
                .related(&c[1])
                .requirement(requirement),
        );
    }
    for c in FABRIC_INSTALL_LINE.captures_iter(text) {
        found.push(
            CrashSuspect::new(SuspectReason::Missing)
                .id(&c[1])
                .requirement(fabric_prose_requirement(&c[2])),
        );
    }
    for c in FABRIC_REPLACE.captures_iter(text) {
        let suspect = CrashSuspect::new(SuspectReason::WrongVersion)
            .id(&c[2])
            .named(Some(&c[1]))
            .version(Some(&c[3]))
            .requirement(fabric_prose_requirement(&c[4]));
        // The "More details" lines say why more precisely (another Minecraft version, a
        // newer loader); the suggested replacement only fills in what they left out.
        match found.0.iter_mut().find(|s| s.same_mod(&suspect)) {
            Some(existing) => existing.absorb(suspect),
            None => found.push(suspect),
        }
    }
}

/// The first jar of the `- <mods>/x.jar` list Quilt prints under an error (`after` starts on the
/// error's line).
fn quilt_listed_jar(after: &str) -> Option<String> {
    after
        .lines()
        .skip(1)
        .map(str::trim)
        .skip_while(|line| line.is_empty())
        .take_while(|line| line.starts_with("- "))
        .filter_map(|line| line.strip_prefix("- "))
        .find(|path| path.to_ascii_lowercase().ends_with(".jar"))
        .map(file_name_of)
}

/// Quilt Loader's own wording; its errors name mods by display name and list their jars.
fn quilt_resolution(text: &str, found: &mut Suspects) {
    for c in QUILT_REQUIRES.captures_iter(text) {
        let name = c[1].trim();
        if name.starts_with("Mod '") {
            continue;
        }
        let target = &c[3];
        let missing = &c[4] == "which is missing";
        let file = c.get(0).and_then(|m| quilt_listed_jar(&text[m.end()..]));
        let requirement = quilt_prose_requirement(&c[2]);
        let requirer = |reason, requirement: Option<String>| {
            let mut suspect = CrashSuspect::new(reason)
                .named(Some(name))
                .related(target)
                .requirement(requirement);
            suspect.file = file.clone();
            suspect
        };
        match target {
            "minecraft" => found.push(requirer(SuspectReason::IncompatibleMinecraft, requirement)),
            "java" => found.push(requirer(SuspectReason::NeedsNewerJava, requirement)),
            // Quilt provides the Fabric loader under its own version numbers: nothing to compare.
            "fabricloader" | "fabric-loader" => {
                found.push(requirer(SuspectReason::NeedsNewerLoader, None));
            }
            "quilt_loader" => found.push(requirer(SuspectReason::NeedsNewerLoader, requirement)),
            _ if missing => {
                found.push(requirer(
                    SuspectReason::RequiresMissing,
                    requirement.clone(),
                ));
                found.push(
                    CrashSuspect::new(SuspectReason::Missing)
                        .id(target)
                        .related(name)
                        .requirement(requirement),
                );
            }
            _ => {
                found.push(requirer(
                    SuspectReason::RequiresOtherVersion,
                    requirement.clone(),
                ));
                found.push(
                    CrashSuspect::new(SuspectReason::WrongVersion)
                        .id(target)
                        .related(name)
                        .requirement(requirement),
                );
            }
        }
    }
}

/// One "requested by" relation of Forge or NeoForge.
fn forge_like_relation(
    found: &mut Suspects,
    requester: &str,
    dep: &str,
    requirement: Option<String>,
    actual: Option<&str>,
) {
    let requirer = |reason| {
        CrashSuspect::new(reason)
            .id(requester)
            .related(dep)
            .requirement(requirement.clone())
    };
    match (dep, actual) {
        ("minecraft", _) => found.push(requirer(SuspectReason::IncompatibleMinecraft)),
        ("forge" | "neoforge", actual) => {
            // Without a readable requirement the direction is unknown; a loader newer than the
            // range (an exact pin of an older build) is "another version", not "newer".
            let older = match (actual, requirement.as_deref()) {
                (None, _) => true,
                (Some(v), Some(r)) => needs_newer_loader(VersionReq::maven(Some(r)), v),
                (Some(_), None) => false,
            };
            found.push(requirer(if older {
                SuspectReason::NeedsNewerLoader
            } else {
                SuspectReason::RequiresOtherVersion
            }));
        }
        (_, None) => {
            found.push(requirer(SuspectReason::RequiresMissing));
            found.push(
                CrashSuspect::new(SuspectReason::Missing)
                    .id(dep)
                    .related(requester)
                    .requirement(requirement.clone()),
            );
        }
        (_, Some(version)) => {
            found.push(requirer(SuspectReason::RequiresOtherVersion));
            found.push(
                CrashSuspect::new(SuspectReason::WrongVersion)
                    .id(dep)
                    .version(Some(version))
                    .related(requester)
                    .requirement(requirement.clone()),
            );
        }
    }
}

fn forge_dependencies(text: &str, found: &mut Suspects) {
    for c in FORGE_DEPENDENCY_FULL.captures_iter(text) {
        let range = exact_maven_range(c[3].trim());
        let requirement = (!range.is_empty() && range != "*").then_some(range);
        let actual = Some(c[4].trim()).filter(|a| *a != "[MISSING]" && !a.is_empty());
        forge_like_relation(found, &c[2], &c[1], requirement, actual);
    }
}

fn neoforge_dependencies(text: &str, found: &mut Suspects) {
    for c in NEO_REQUIRES.captures_iter(text) {
        let actual = Some(&c[5]).filter(|a| *a != "not installed");
        forge_like_relation(
            found,
            &c[1],
            &c[2],
            neoforge_prose_requirement(&c[3]),
            actual,
        );
    }
    for c in NEO_INCOMPATIBLE.captures_iter(text) {
        found.push(
            CrashSuspect::new(SuspectReason::DeclaresIncompatible)
                .id(&c[1])
                .related(&c[2]),
        );
        found.push(
            CrashSuspect::new(SuspectReason::Incompatible)
                .id(&c[2])
                .related(&c[1]),
        );
    }
}

/// `C:\x\mods\a.jar` → `a.jar`.
fn file_name_of(path: &str) -> String {
    let path = path.trim().trim_matches(['\'', '"', '[', ']']);
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_owned()
}

fn duplicate_mods(text: &str, kind: CrashKind, found: &mut Suspects) {
    for c in FORGE_DUPLICATE_FILES.captures_iter(text) {
        let Some(id) = c.get(1).or(c.get(2)).map(|m| m.as_str()) else {
            continue;
        };
        for file in c[3].split(',').map(file_name_of).filter(|f| !f.is_empty()) {
            let mut suspect = CrashSuspect::new(SuspectReason::Duplicate).id(id);
            suspect.file = Some(file);
            // One entry per file: `same_mod` would merge them by id.
            found.0.push(suspect);
        }
    }
    for c in QUILT_DUPLICATE.captures_iter(text) {
        let ids: Vec<&str> = c[1]
            .split(',')
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .collect();
        let files: Vec<String> = c[2]
            .lines()
            .filter_map(|line| line.trim().strip_prefix("- "))
            .filter(|path| path.to_ascii_lowercase().ends_with(".jar"))
            .map(file_name_of)
            .collect();
        match ids.as_slice() {
            // One id: the listed jars are its copies.
            [id] if !files.is_empty() => {
                for file in files {
                    let mut suspect = CrashSuspect::new(SuspectReason::Duplicate).id(id);
                    suspect.file = Some(file);
                    found.0.push(suspect);
                }
            }
            _ => {
                for id in ids {
                    found.push(CrashSuspect::new(SuspectReason::Duplicate).id(id));
                }
            }
        }
    }
    if kind != CrashKind::DuplicateMods || found.has(SuspectReason::Duplicate) {
        return;
    }
    for c in DUPLICATE_ID.captures_iter(text) {
        if let Some(id) = c.get(1).or(c.get(2)).or(c.get(3)) {
            found.push(CrashSuspect::new(SuspectReason::Duplicate).id(id.as_str()));
        }
    }
    for c in FABRIC_MOD.captures_iter(text) {
        found.push(
            CrashSuspect::new(SuspectReason::Duplicate)
                .id(&c[2])
                .named(Some(&c[1])),
        );
    }
}

fn class_versions(text: &str, found: &mut Suspects) {
    for c in CLASS_VERSION_CLASS.captures_iter(text) {
        let class = c[1].replace('/', ".");
        if PLATFORM_CLASSES.iter().any(|p| class.starts_with(p)) {
            continue;
        }
        let major = c[2].parse::<u32>().ok().filter(|v| *v > 44).map(|v| v - 44);
        let mut suspect = CrashSuspect::new(SuspectReason::NeedsNewerJava)
            .requirement(major.map(|m| format!(">={m}")));
        suspect.class_name = Some(class);
        found.push(suspect);
    }
}

/// Mixin configs and mod ids on the lines where Mixin reports a failure.
fn mixin_failures(text: &str, found: &mut Suspects) {
    for line in text.lines().filter(|l| MIXIN_FAILURE_LINE.is_match(l)) {
        for c in MIXIN_REF.captures_iter(line) {
            let mut suspect = CrashSuspect::new(SuspectReason::MixinFailed);
            if let Some(id) = c.get(3) {
                suspect = suspect.id(id.as_str());
            }
            suspect.mixin_config = Some(c[1].to_owned());
            found.push(suspect);
        }
        for c in MIXIN_CONFIG_REF.captures_iter(line) {
            let mut suspect = CrashSuspect::new(SuspectReason::MixinFailed);
            suspect.mixin_config = Some(c[1].to_owned());
            found.push(suspect);
        }
        for c in MIXIN_APPLY_FOR_MOD.captures_iter(line) {
            found.push(CrashSuspect::new(SuspectReason::MixinFailed).id(&c[1]));
        }
        for c in MIXIN_APPLICATION.captures_iter(line) {
            let mut suspect = CrashSuspect::new(SuspectReason::MixinFailed)
                .id(&c[3])
                .named(Some(&c[2]));
            suspect.class_name = Some(c[1].to_owned());
            found.push(suspect);
        }
    }
}

/// True when a mod loading section's first error is "Could not initialize class": that class
/// failed earlier, usually because of another mod.
fn knock_on_failure(section: &str) -> bool {
    section
        .lines()
        .find(|line| THROWABLE.is_match(line))
        .is_some_and(|line| line.contains("NoClassDefFoundError: Could not initialize class"))
}

/// Mods the loader reported as failed. Returns, instead of adding them, the ones that only failed
/// as a knock-on effect ([`knock_on_failure`]).
fn loading_failures(text: &str, found: &mut Suspects) -> Vec<CrashSuspect> {
    let mut knock_on: Vec<CrashSuspect> = Vec::new();
    for m in MOD_SECTION.captures_iter(text) {
        let Some(whole) = m.get(0) else {
            continue;
        };
        // The section's own lines, up to the next section.
        let rest = &text[whole.end()..];
        let section = &rest[..rest.find("\n-- ").unwrap_or(rest.len())];
        let file = MOD_FILE_LINE
            .captures(section)
            .map(|c| file_name_of(&c[1]))
            .filter(|f| f.to_ascii_lowercase().ends_with(".jar"));
        let mut suspect = CrashSuspect::new(SuspectReason::LoadingFailed).id(&m[1]);
        suspect.file = file;
        if knock_on_failure(section) {
            knock_on.push(suspect);
        } else {
            found.push(suspect);
        }
    }
    for c in CAUGHT_FROM
        .captures_iter(text)
        .chain(FAILED_TO_LOAD.captures_iter(text))
    {
        let suspect = CrashSuspect::new(SuspectReason::LoadingFailed)
            .id(&c[2])
            .named(Some(&c[1]));
        // A knock-on section's failure message names its own mod again.
        match knock_on.iter_mut().find(|s| s.same_mod(&suspect)) {
            Some(existing) => existing.absorb(suspect),
            None => found.push(suspect),
        }
    }
    for c in ENTRYPOINT_FAILED.captures_iter(text) {
        found.push(CrashSuspect::new(SuspectReason::LoadingFailed).id(&c[1]));
    }
    knock_on
}

/// Jars Forge/NeoForge refused as mod files.
fn invalid_mod_files(text: &str, found: &mut Suspects) {
    for c in INVALID_MOD_FILE.captures_iter(text) {
        let mut suspect = CrashSuspect::new(SuspectReason::CorruptFile);
        suspect.file = Some(file_name_of(&c[1]));
        found.push(suspect);
    }
}

fn broken_jars(text: &str, found: &mut Suspects) {
    for c in BROKEN_JAR.captures_iter(text) {
        if let Some(path) = c.get(1).or(c.get(2)) {
            let mut suspect = CrashSuspect::new(SuspectReason::CorruptFile);
            suspect.file = Some(file_name_of(path.as_str()));
            found.push(suspect);
        }
    }
}

fn suspected_section(text: &str, found: &mut Suspects) {
    for block in SUSPECTED_BLOCK.captures_iter(text) {
        for c in SUSPECTED_LINE.captures_iter(&block[1]) {
            found.push(
                CrashSuspect::new(SuspectReason::NamedInReport)
                    .id(&c[2])
                    .named(Some(&c[1]))
                    .version(Some(&c[3])),
            );
        }
    }
    for line in SUSPECTED_INLINE.captures_iter(text) {
        for c in SUSPECTED_ITEM.captures_iter(&line[1]) {
            found.push(
                CrashSuspect::new(SuspectReason::NamedInReport)
                    .id(&c[2])
                    .named(Some(&c[1])),
            );
        }
    }
}

/// Mod frames among the first lines of the stack trace (FML module names, Fabric mixin
/// handler names). Weak evidence: only used when nothing else was found.
fn stack_frames(text: &str, found: &mut Suspects) {
    let frames = text
        .lines()
        .filter(|l| l.trim_start().starts_with("at "))
        .take(60);
    for line in frames {
        for c in FML_FRAME
            .captures_iter(line)
            .chain(MIXIN_HANDLER.captures_iter(line))
        {
            if found.0.len() >= 3 {
                return;
            }
            found.push(CrashSuspect::new(SuspectReason::InStackTrace).id(&c[1]));
        }
    }
}

// --- attribution -------------------------------------------------------------------------------

/// The jar (and declared mod) a suspect points at. Missing mods are looked for among disabled
/// jars only (to turn them back on); everything else among enabled jars.
fn locate<'a>(
    suspect: &CrashSuspect,
    mods: &'a ModSnapshot,
) -> Option<(&'a ScannedJar, Option<&'a DeclaredMod>)> {
    let missing = suspect.reason == SuspectReason::Missing;
    let usable = |jar: &ScannedJar| jar.enabled != missing;
    let declared = |jar: &'a ScannedJar| {
        suspect
            .mod_id
            .as_deref()
            .and_then(|id| jar.contents.all_mods().find(|m| m.answers_to(id)))
            .or(jar.contents.mods.first())
    };
    if let Some(jar) = suspect
        .file
        .as_deref()
        .and_then(|f| mods.by_file(f))
        .filter(|&j| usable(j))
    {
        return Some((jar, declared(jar)));
    }
    if let Some(found) = suspect
        .mixin_config
        .as_deref()
        .and_then(|c| mods.by_mixin_config(c))
        .filter(|&(j, _)| usable(j))
    {
        return Some(found);
    }
    if let Some((jar, m)) = suspect
        .mod_id
        .as_deref()
        .and_then(|id| mods.declaring(id).find(|&(j, _)| usable(j)))
    {
        return Some((jar, Some(m)));
    }
    if let Some((jar, m)) = suspect
        .name
        .as_deref()
        .and_then(|n| mods.by_name(n))
        .filter(|&(j, _)| usable(j))
    {
        return Some((jar, Some(m)));
    }
    suspect
        .class_name
        .as_deref()
        .filter(|_| !missing)
        .and_then(|c| mods.by_class(c))
        .map(|jar| (jar, jar.contents.mods.first()))
}

/// Find the suspects' jars in `mods` (the mods folder the crashed game used): file names,
/// display names and versions come from the jars. Then fill [`CrashDiagnosis::mod_fixes`]
/// with the pre-launch check's fixes (download what is missing, update what is too old or
/// made for another game version, turn off duplicates and incompatible mods, a newer loader or
/// Java) and, for mod errors, rewrite the Turkish texts with the mods' display names.
pub fn attribute_crash(diagnosis: &mut CrashDiagnosis, mods: &ModSnapshot, env: &CheckEnvironment) {
    for suspect in &mut diagnosis.suspects {
        let Some((jar, declared)) = locate(suspect, mods) else {
            continue;
        };
        suspect.file = Some(jar.file_name.clone());
        match declared {
            Some(m) => {
                if suspect.mod_id.is_none() {
                    suspect.mod_id = Some(m.id.clone());
                }
                suspect.name = Some(m.display_name().to_owned());
                if suspect.version.is_none() {
                    suspect.version = m.version.clone();
                }
            }
            None if suspect.name.is_none() => suspect.name = Some(jar.readable_name()),
            None => {}
        }
    }
    // Clues that turned out to name the same jar for the same reason become one suspect.
    let mut merged: Vec<CrashSuspect> = Vec::new();
    for suspect in std::mem::take(&mut diagnosis.suspects) {
        match merged
            .iter_mut()
            .find(|m| m.reason == suspect.reason && m.file.is_some() && m.file == suspect.file)
        {
            Some(existing) => existing.absorb(suspect),
            None => merged.push(suspect),
        }
    }
    diagnosis.suspects = merged;
    diagnosis.mod_fixes = crash_fixes(diagnosis, mods, env);
    refresh_texts(diagnosis);
}

fn crash_fixes(d: &CrashDiagnosis, mods: &ModSnapshot, env: &CheckEnvironment) -> Vec<ModFix> {
    let mut fixes: Vec<ModFix> = Vec::new();
    let mut add = |fix: ModFix| {
        if !fixes.contains(&fix) {
            fixes.push(fix);
        }
    };
    let required_java = d.suggested_actions.iter().find_map(|a| match a {
        SuggestedAction::UseAutomaticJava { required_major } => *required_major,
        _ => None,
    });
    let enabled_file = |s: &CrashSuspect| {
        s.file
            .as_deref()
            .filter(|f| mods.by_file(f).is_some_and(|j| j.enabled))
            .map(str::to_owned)
    };
    for s in &d.suspects {
        let id = s.mod_id.clone();
        match s.reason {
            SuspectReason::Missing => {
                let Some(id) = id else {
                    continue;
                };
                let disabled = s
                    .file
                    .as_deref()
                    .and_then(|f| mods.by_file(f))
                    .filter(|j| !j.enabled);
                match disabled {
                    Some(jar) => add(ModFix::EnableFile {
                        file: jar.file_name.clone(),
                        mod_id: id,
                    }),
                    None => add(ModFix::FetchDependency {
                        version_req: s.requirement.clone(),
                        loader: env.loader,
                        minecraft: env.minecraft.clone(),
                        required_by: d
                            .suspects
                            .iter()
                            .filter(|r| {
                                r.reason == SuspectReason::RequiresMissing
                                    && r.related.as_deref() == Some(id.as_str())
                            })
                            .filter_map(|r| r.mod_id.clone())
                            .collect(),
                        mod_id: id,
                    }),
                }
            }
            SuspectReason::WrongVersion => {
                if let (Some(file), Some(id)) = (enabled_file(s), id) {
                    add(ModFix::UpdateToCompatible {
                        file,
                        mod_id: id,
                        version_req: s.requirement.clone(),
                        disable_if_unavailable: false,
                    });
                }
            }
            SuspectReason::IncompatibleMinecraft => {
                if let (Some(file), Some(id)) = (enabled_file(s), id) {
                    add(ModFix::UpdateToCompatible {
                        file,
                        mod_id: id,
                        version_req: None,
                        disable_if_unavailable: true,
                    });
                }
            }
            SuspectReason::NeedsNewerLoader => {
                let too_old = match (&env.loader_version, &s.requirement) {
                    (Some(v), Some(r)) => {
                        needs_newer_loader(VersionReq::parse_for(env.loader, r), v)
                    }
                    (None, Some(_)) => true,
                    _ => false,
                };
                match (&s.requirement, enabled_file(s), id) {
                    (Some(req), _, _) if too_old => add(ModFix::UpdateLoader {
                        loader: env.loader,
                        version_req: req.clone(),
                    }),
                    // The installed loader is fine by the requirement: the mod wants a build of its
                    // own for it. Without a requirement nothing is certain.
                    (Some(_), Some(file), Some(id)) => add(ModFix::UpdateToCompatible {
                        file,
                        mod_id: id,
                        version_req: None,
                        disable_if_unavailable: false,
                    }),
                    _ => {}
                }
            }
            SuspectReason::RequiresOtherVersion => {
                // Made for an older build of this loader (often an exact pin): a newer release of
                // the mod may accept the installed one.
                let loader_too_new = s
                    .related
                    .as_deref()
                    .is_some_and(|related| is_loader_of(env.loader, related))
                    && matches!(
                        (&env.loader_version, &s.requirement),
                        (Some(v), Some(r)) if VersionReq::parse_for(env.loader, r).mismatch(v) == Mismatch::TooNew
                    );
                if loader_too_new && let (Some(file), Some(id)) = (enabled_file(s), id) {
                    add(ModFix::UpdateToCompatible {
                        file,
                        mod_id: id,
                        version_req: None,
                        disable_if_unavailable: false,
                    });
                }
            }
            SuspectReason::NeedsNewerJava => {
                let major = s
                    .requirement
                    .as_deref()
                    .and_then(|r| minimum_java(&VersionReq::fabric(r)))
                    .or(required_java);
                if let Some(major) = major {
                    add(ModFix::UseJava { major });
                }
            }
            SuspectReason::Incompatible => {
                // Only some versions are declared incompatible: a newer one may be fine (as in
                // the pre-launch check).
                let bounded = s
                    .requirement
                    .as_deref()
                    .is_some_and(|r| VersionReq::parse_for(env.loader, r).has_upper_bound());
                match (enabled_file(s), id) {
                    (Some(file), Some(id)) if bounded => add(ModFix::UpdateToCompatible {
                        file,
                        mod_id: id,
                        version_req: None,
                        disable_if_unavailable: true,
                    }),
                    (Some(file), _) => add(ModFix::DisableFile {
                        file,
                        reason: DisableReason::Breaks,
                    }),
                    _ => {}
                }
            }
            SuspectReason::CorruptFile => {
                if let Some(file) = enabled_file(s) {
                    add(ModFix::DisableFile {
                        file,
                        reason: DisableReason::Corrupt,
                    });
                }
            }
            SuspectReason::Duplicate => {
                if let Some(id) = id {
                    for jar in older_copies(mods, env, &id) {
                        add(ModFix::DisableFile {
                            file: jar.file_name.clone(),
                            reason: DisableReason::Duplicate,
                        });
                    }
                }
            }
            // For dependency and duplicate errors the specific fixes above are the repair.
            SuspectReason::MixinFailed
            | SuspectReason::LoadingFailed
            | SuspectReason::NamedInReport
                if !matches!(
                    d.kind,
                    CrashKind::MissingDependency
                        | CrashKind::IncompatibleMods
                        | CrashKind::DuplicateMods
                ) =>
            {
                if let (Some(file), Some(id)) = (enabled_file(s), id) {
                    add(ModFix::UpdateToCompatible {
                        file,
                        mod_id: id,
                        version_req: None,
                        disable_if_unavailable: false,
                    });
                }
            }
            SuspectReason::RequiresMissing
            | SuspectReason::DeclaresIncompatible
            | SuspectReason::MixinFailed
            | SuspectReason::LoadingFailed
            | SuspectReason::NamedInReport
            | SuspectReason::InStackTrace => {}
        }
    }
    if d.kind == CrashKind::JavaTooOld
        && let Some(major) = required_java
    {
        add(ModFix::UseJava { major });
    }
    fixes
}

/// For mod errors, show the mods by the names their jars give.
fn refresh_texts(d: &mut CrashDiagnosis) {
    if !matches!(
        d.kind,
        CrashKind::IncompatibleMods
            | CrashKind::MissingDependency
            | CrashKind::DuplicateMods
            | CrashKind::ModError
    ) {
        return;
    }
    let display = |entry: &str| {
        d.suspects
            .iter()
            .filter(|s| s.file.is_some())
            .find(|s| {
                s.mod_id
                    .as_deref()
                    .is_some_and(|id| id.eq_ignore_ascii_case(entry))
                    || s.name
                        .as_deref()
                        .is_some_and(|n| n.eq_ignore_ascii_case(entry))
            })
            .and_then(|s| s.name.clone())
            .unwrap_or_else(|| entry.to_owned())
    };
    let mut mods: Vec<String> = d.mods.iter().map(|m| display(m.as_str())).collect();
    let culprits = d.suspects.iter().filter(|s| {
        s.file.is_some()
            && match d.kind {
                // The mods whose code failed, also when the log only named their mixin config,
                // and jars the loader refused.
                CrashKind::ModError => {
                    matches!(
                        s.reason,
                        SuspectReason::MixinFailed
                            | SuspectReason::LoadingFailed
                            | SuspectReason::CorruptFile
                    )
                }
                _ => mods.is_empty() && s.reason != SuspectReason::InStackTrace,
            }
    });
    let extra: Vec<String> = culprits.filter_map(|s| s.name.clone()).collect();
    mods.extend(extra);
    let mods = dedup_names(mods);
    if mods == d.mods {
        return;
    }
    let had_report = d
        .suggested_actions
        .contains(&SuggestedAction::OpenCrashReport);
    let (title, hint, mut actions) = describe(d.kind, &mods, None, &CrashInput::default());
    if had_report {
        actions.push(SuggestedAction::OpenCrashReport);
    }
    actions.push(SuggestedAction::OpenLogs);
    d.title_tr = title;
    d.hint_tr = hint;
    d.suggested_actions = actions;
    d.mods = mods;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn analyze(output: &str) -> CrashDiagnosis {
        analyze_crash(&CrashInput {
            output,
            exit_code: Some(1),
            runtime: Duration::from_secs(3),
            max_memory_mb: Some(8192),
            required_java: Some(21),
            java_major: Some(25),
            ..CrashInput::default()
        })
    }

    #[test]
    fn jvm_startup_failures() {
        let d = analyze(
            "Error occurred during initialization of VM\nCould not reserve enough space for object heap",
        );
        assert_eq!(d.kind, CrashKind::MemoryReserve);
        assert_eq!(
            d.suggested_actions[0],
            SuggestedAction::LowerMaxMemory {
                suggested_mb: Some(4096)
            }
        );
        let d = analyze(
            "Unrecognized VM option 'UseFancyThing'\nError: Could not create the Java Virtual Machine.",
        );
        assert_eq!(d.kind, CrashKind::InvalidJvmArgument);
        let java9_options =
            "Unrecognized option: --add-exports\nError: Could not create the Java Virtual Machine.";
        let d = analyze_crash(&CrashInput {
            output: java9_options,
            exit_code: Some(1),
            java_major: Some(8),
            required_java: Some(21),
            ..CrashInput::default()
        });
        assert_eq!(d.kind, CrashKind::JavaTooOld);
        // On Java 9+ the same message means a mistyped user argument.
        assert_eq!(analyze(java9_options).kind, CrashKind::InvalidJvmArgument);
        let d = analyze("Error: Could not create the Java Virtual Machine.");
        assert_eq!(d.kind, CrashKind::MemoryReserve, "legacy mapping");
        // Newer JVMs give the size.
        let d = analyze(
            "Error occurred during initialization of VM\r\nCould not reserve enough space for 2097152KB object heap\r\n",
        );
        assert_eq!(d.kind, CrashKind::MemoryReserve);
    }

    /// What `java.exe` prints when the JVM cannot start, with the player's own JVM arguments.
    fn analyze_with(output: &str, player: &[&str], java_major: u32) -> CrashDiagnosis {
        let player: Vec<String> = player.iter().map(|s| (*s).to_owned()).collect();
        analyze_crash(&CrashInput {
            output,
            exit_code: Some(1),
            runtime: Duration::from_millis(300),
            max_memory_mb: Some(4096),
            required_java: Some(21),
            java_major: Some(java_major),
            player_jvm_args: &player,
            ..CrashInput::default()
        })
    }

    #[test]
    fn jvm_start_failures_caused_by_the_players_arguments() {
        let fatal = "Error: Could not create the Java Virtual Machine.\nError: A fatal exception has occurred. Program will exit.";
        let reset = |d: &CrashDiagnosis| {
            assert_eq!(d.kind, CrashKind::InvalidJvmArgument, "{}", d.title_tr);
            assert_eq!(d.suggested_actions[0], SuggestedAction::ResetJvmArguments);
            assert!(d.suspects.is_empty() && d.mods.is_empty());
        };
        // A mistyped -XX option, named without its prefix.
        reset(&analyze_with(
            &format!(
                "Unrecognized VM option 'UseFancyThing'\nDid you mean '(+/-)UseFancyThings'?\n{fatal}"
            ),
            &["-XX:+UseG1GC", "-XX:+UseFancyThing"],
            21,
        ));
        reset(&analyze_with(
            &format!("Improperly specified VM option 'MaxGCPauseMillis'\n{fatal}"),
            &["-XX:MaxGCPauseMillis"],
            21,
        ));
        reset(&analyze_with(
            &format!(
                "VM option 'UseZGC' is experimental and must be enabled via -XX:+UnlockExperimentalVMOptions.\n{fatal}"
            ),
            &["-XX:+UseZGC"],
            11,
        ));
        // A Java 9+ option of the player's on Java 8 is theirs to remove, not "Java too old".
        reset(&analyze_with(
            &format!("Unrecognized option: --add-opens\n{fatal}"),
            &["--add-opens", "java.base/java.lang=ALL-UNNAMED"],
            8,
        ));
        // A malformed memory value of the player's.
        reset(&analyze_with(
            &format!("Invalid maximum heap size: -Xmx4gb\n{fatal}"),
            &["-Xmx4gb"],
            21,
        ));
        reset(&analyze_with(
            &format!(
                "Invalid thread stack size: -Xss1x. The specified size exceeds the maximum representable size.\n{fatal}"
            ),
            &["-Xss1x"],
            21,
        ));
        // A value typed without its option became the main class (not "corrupted files").
        reset(&analyze_with(
            "Error: Could not find or load main class 2G\nCaused by: java.lang.ClassNotFoundException: 2G",
            &["-Xmx4G", "2G"],
            21,
        ));
        // The player's own heap is what failed: lowering the launcher's memory would not help.
        reset(&analyze_with(
            &format!(
                "Error occurred during initialization of VM\nCould not reserve enough space for 67108864KB object heap\n{fatal}"
            ),
            &["-Xmx64G"],
            8,
        ));
        reset(&analyze_with(
            &format!(
                "Error occurred during initialization of VM\nInitial heap size set to a larger value than the maximum heap size\n{fatal}"
            ),
            &["-Xms8G"],
            21,
        ));
        // A Java agent that cannot be found, conflicting collectors.
        reset(&analyze_with(
            &format!(
                "Error opening zip file or JAR manifest missing : C:\\tools\\agent.jar\nError occurred during initialization of VM\nagent library failed to init: instrument\n{fatal}"
            ),
            &["-javaagent:C:\\tools\\agent.jar"],
            21,
        ));
        reset(&analyze_with(
            &format!(
                "Conflicting collector combinations in option list; please refer to the release notes for the combinations allowed\n{fatal}"
            ),
            &["-XX:+UseG1GC", "-XX:+UseParallelGC"],
            21,
        ));
    }

    #[test]
    fn jvm_start_failures_of_the_launchers_own_options() {
        let fatal = "Error: Could not create the Java Virtual Machine.";
        // Without player arguments the launcher's memory setting is what failed.
        let d = analyze_with(
            &format!(
                "Error occurred during initialization of VM\nCould not reserve enough space for 4194304KB object heap\n{fatal}"
            ),
            &[],
            8,
        );
        assert_eq!(d.kind, CrashKind::MemoryReserve);
        assert_eq!(
            d.suggested_actions[0],
            SuggestedAction::LowerMaxMemory {
                suggested_mb: Some(2048)
            }
        );
        // Player arguments that do not touch the heap do not take the blame.
        let d = analyze_with(
            &format!(
                "Error occurred during initialization of VM\nCould not reserve enough space for 4194304KB object heap\n{fatal}"
            ),
            &["-XX:+UseG1GC", "-Dsun.java2d.d3d=false"],
            8,
        );
        assert_eq!(d.kind, CrashKind::MemoryReserve);
        // The launcher's own -Xmx too big for a 32-bit JVM.
        let d = analyze_with(
            &format!(
                "Invalid maximum heap size: -Xmx4096M\nThe specified size exceeds the maximum representable size.\n{fatal}"
            ),
            &["-XX:+UseG1GC"],
            8,
        );
        assert_eq!(d.kind, CrashKind::MemoryReserve);
        // A main class missing for another reason stays a damaged installation.
        let d = analyze_with(
            "Error: Could not find or load main class net.minecraft.client.main.Main\nCaused by: java.lang.ClassNotFoundException: net.minecraft.client.main.Main",
            &["-XX:+UseG1GC"],
            21,
        );
        assert_eq!(d.kind, CrashKind::CorruptedFiles);
        // Java 9+ options the version needs on Java 8 still mean "Java too old".
        let d = analyze_with(
            &format!("Unrecognized option: --add-exports\n{fatal}"),
            &["-XX:+UseG1GC"],
            8,
        );
        assert_eq!(d.kind, CrashKind::JavaTooOld);
        // So do a newer Java's options in the version's own arguments (26.1 on Java 17, seen in
        // the real app), and a newer Java's VM option the version passes.
        let d = analyze_with(
            &format!("Unrecognized option: --sun-misc-unsafe-memory-access=allow\n{fatal}"),
            &["-XX:+UseG1GC"],
            17,
        );
        assert_eq!(d.kind, CrashKind::JavaTooOld);
        assert_eq!(
            d.suggested_actions[0],
            SuggestedAction::UseAutomaticJava {
                required_major: Some(21)
            }
        );
        let d = analyze_with(
            &format!("Unrecognized VM option 'UseCompactObjectHeaders'\n{fatal}"),
            &[],
            17,
        );
        assert_eq!(d.kind, CrashKind::JavaTooOld);
        // With a Java that is new enough, an unknown option stays an argument problem.
        let d = analyze_with(
            &format!("Unrecognized option: --sun-misc-unsafe-memory-access=allow\n{fatal}"),
            &[],
            21,
        );
        assert_eq!(d.kind, CrashKind::InvalidJvmArgument);
        // A crash after the game started keeps its own kind.
        let d = analyze_with(
            "java.lang.OutOfMemoryError: Java heap space",
            &["-Xmx2G"],
            21,
        );
        assert_eq!(d.kind, CrashKind::OutOfMemory);
    }

    #[test]
    fn java_versions() {
        let d = analyze(
            "Exception in thread \"main\" java.lang.UnsupportedClassVersionError: net/minecraft/client/main/Main has been compiled by a more recent version of the Java Runtime (class file version 65.0), this version of the Java Runtime only recognizes class file versions up to 52.0",
        );
        assert_eq!(d.kind, CrashKind::JavaTooOld);
        assert!(d.hint_tr.contains("Java 21"));
        assert_eq!(
            d.suggested_actions[0],
            SuggestedAction::UseAutomaticJava {
                required_major: Some(21)
            }
        );
        let d = analyze(
            "java.lang.ClassCastException: class jdk.internal.loader.ClassLoaders$AppClassLoader cannot be cast to class java.net.URLClassLoader",
        );
        assert_eq!(d.kind, CrashKind::JavaTooNew);
        assert!(d.hint_tr.contains("Java 25"));
    }

    #[test]
    fn out_of_memory_and_graphics() {
        let d = analyze("java.lang.OutOfMemoryError: Java heap space");
        assert_eq!(d.kind, CrashKind::OutOfMemory);
        assert_eq!(
            d.suggested_actions[0],
            SuggestedAction::IncreaseMaxMemory {
                suggested_mb: Some(12288)
            }
        );
        let d = analyze(
            "[12:00:00] [Render thread/ERROR]: GLFW error 65542: WGL: The driver does not appear to support OpenGL",
        );
        assert_eq!(d.kind, CrashKind::GraphicsDriver);
        let d = analyze_crash(&CrashInput {
            output: "some output",
            exit_code: Some(-1073741819),
            ..CrashInput::default()
        });
        assert_eq!(d.kind, CrashKind::GraphicsDriver);
    }

    #[test]
    fn fabric_missing_dependency() {
        let d = analyze(
            "net.fabricmc.loader.impl.FormattedException: Some of your mods are incompatible with the game or each other!\n\
             A potential solution has been determined, this may resolve your problem:\n\
             \t - Install fabric-api, any version.\n\
             More details:\n\
             \t - Mod 'Mod Menu' (modmenu) 7.2.2 requires any version of fabric-api, which is missing!",
        );
        assert_eq!(d.kind, CrashKind::MissingDependency);
        assert_eq!(d.mods, ["fabric-api"]);
        assert!(d.hint_tr.contains("fabric-api"));
        assert_eq!(
            d.suggested_actions[0],
            SuggestedAction::InstallMods {
                mods: vec!["fabric-api".into()]
            }
        );
    }

    #[test]
    fn fabric_incompatible_mods() {
        let d = analyze(
            "Incompatible mods found!\n\
             \t - Mod 'Iris' (iris) 1.6.4 requires version 0.5.0 or later of mod 'Sodium' (sodium), but only the wrong version is present: 0.4.10!\n\
             \t - Replace mod 'Sodium' (sodium) 0.4.10 with version 0.5.0 or later.",
        );
        assert_eq!(d.kind, CrashKind::IncompatibleMods);
        assert_eq!(d.mods, ["Iris", "Sodium"]);
        assert!(d.hint_tr.contains("Iris, Sodium"));
    }

    #[test]
    fn forge_missing_mandatory_dependencies() {
        let d = analyze(
            "Missing or unsupported mandatory dependencies:\n\
             \tMod ID: 'architectury', Requested by: 'rei', Expected range: '[9.1.12,)', Actual version: '[MISSING]'\n\
             \tMod ID: 'cloth_config', Requested by: 'rei', Expected range: '[11,)', Actual version: '[MISSING]'",
        );
        assert_eq!(d.kind, CrashKind::MissingDependency);
        assert_eq!(d.mods, ["architectury", "cloth_config"]);

        let d = analyze(
            "Missing or unsupported mandatory dependencies:\n\
             \tMod ID: 'forge', Requested by: 'jei', Expected range: '[47.1,)', Actual version: '47.0.3'",
        );
        assert_eq!(d.kind, CrashKind::IncompatibleMods);
        assert_eq!(d.mods, ["jei"]);
    }

    #[test]
    fn duplicates_mixins_and_misc() {
        let d =
            analyze("Found duplicate mods:\n\tMod ID: 'jei' from mod files: jei-1.jar, jei-2.jar");
        assert_eq!(d.kind, CrashKind::DuplicateMods);
        assert_eq!(d.mods, ["jei"]);

        let d = analyze(
            "org.spongepowered.asm.mixin.transformer.throwables.MixinTransformerError: An unexpected critical error was encountered\nCaused by: org.spongepowered.asm.mixin.throwables.MixinApplyError: Mixin [sodium.mixins.json:core.MinecraftMixin] from phase [DEFAULT] in config [sodium.mixins.json] FAILED during APPLY",
        );
        assert_eq!(d.kind, CrashKind::ModError);
        assert_eq!(d.mods, ["sodium"]);
        assert!(
            d.hint_tr
                .starts_with("Şu mod oyunun açılmasını engelledi: sodium.")
        );

        let d = analyze("java.nio.file.AccessDeniedException: C:\\game\\options.txt");
        assert_eq!(d.kind, CrashKind::AccessDenied);
        let d = analyze("java.util.zip.ZipException: zip END header not found");
        assert_eq!(d.kind, CrashKind::CorruptedFiles);
        let d = analyze_crash(&CrashInput::default());
        assert_eq!(d.kind, CrashKind::NoOutput);
        assert!(
            d.suggested_actions
                .contains(&SuggestedAction::AddAntivirusException)
        );
        let d = analyze("[12:00:00] [Render thread/ERROR]: Something odd");
        assert_eq!(d.kind, CrashKind::Unknown);
        assert_eq!(d.suggested_actions, [SuggestedAction::OpenLogs]);
    }

    #[test]
    fn turkish_texts_have_no_codes() {
        for output in [
            "Could not reserve enough space for object heap",
            "java.lang.OutOfMemoryError",
            "EXCEPTION_ACCESS_VIOLATION",
            "Access is denied",
            "",
        ] {
            let d = analyze(output);
            for text in [&d.title_tr, &d.hint_tr] {
                assert!(!text.contains("0x"), "{text}");
                assert!(!text.contains("Exception"), "{text}");
                assert!(!text.contains("-1073741819"), "{text}");
            }
        }
    }

    #[test]
    fn failure_rule_and_report_lookup() {
        assert!(is_failure(Some(1), Duration::from_secs(600), false));
        assert!(is_failure(None, Duration::from_secs(600), false));
        assert!(is_failure(Some(0), Duration::from_secs(5), true));
        assert!(!is_failure(Some(0), Duration::from_secs(5), false));
        assert!(!is_failure(Some(0), Duration::from_secs(60), true));

        let tmp = tempfile::tempdir().unwrap();
        let start = SystemTime::now();
        assert!(find_crash_report(tmp.path(), start).is_none());
        let reports = tmp.path().join("crash-reports");
        std::fs::create_dir_all(&reports).unwrap();
        std::fs::write(reports.join("crash-2026-10-15_12.00.00-client.txt"), "x").unwrap();
        std::fs::write(reports.join("notes.txt"), "x").unwrap();
        let found = find_crash_report(tmp.path(), start).unwrap();
        assert!(found.ends_with("crash-2026-10-15_12.00.00-client.txt"));
        let later = start + Duration::from_secs(3600);
        assert!(find_crash_report(tmp.path(), later).is_none());
        std::fs::write(tmp.path().join("hs_err_pid123.log"), "x").unwrap();
        assert!(find_jvm_error_log(tmp.path(), start).is_some());
    }

    // --- suspects and attribution ---

    use crate::loaders::LoaderKind;
    use crate::mods::check::CheckEnvironment;
    use crate::mods::scan::test_jars::{fabric_jar, fabric_json, toml_jar};
    use crate::mods::scan::{ModSnapshot, ScannedJar};
    use crate::mods::test_support::zip_bytes;

    fn env(loader: LoaderKind, minecraft: &str, loader_version: &str) -> CheckEnvironment {
        CheckEnvironment {
            minecraft: minecraft.to_owned(),
            loader,
            loader_version: Some(loader_version.to_owned()),
            java_major: Some(21),
        }
    }

    fn fabric_env() -> CheckEnvironment {
        env(LoaderKind::Fabric, "1.21.1", "0.16.10")
    }

    fn snapshot(jars: Vec<(&str, Vec<u8>)>) -> ModSnapshot {
        ModSnapshot::new(
            "mods",
            jars.into_iter()
                .map(|(name, bytes)| ScannedJar::from_bytes(name, &bytes))
                .collect(),
        )
    }

    fn reasons(d: &CrashDiagnosis) -> Vec<(Option<&str>, SuspectReason)> {
        d.suspects
            .iter()
            .map(|s| (s.mod_id.as_deref(), s.reason))
            .collect()
    }

    /// Fabric Loader 0.15+ resolution failure.
    const FABRIC_RESOLUTION: &str = "[12:00:01] [main/ERROR]: Incompatible mods found!\n\
net.fabricmc.loader.impl.FormattedException: Some of your mods are incompatible with the game or each other!\n\
A potential solution has been determined, this may resolve your problem:\n\
\t - Install fabric-api, any version.\n\
\t - Replace mod 'Sodium' (sodium) 0.5.8+mc1.21 with version 0.6.0 or later.\n\
More details:\n\
\t - Mod 'Mod Menu' (modmenu) 11.0.1 requires any version of fabric-api, which is missing!\n\
\t - Mod 'Iris' (iris) 1.8.0 requires version 0.6.0 or later of mod 'Sodium' (sodium), but only the wrong version is present: 0.5.8+mc1.21!\n\
\t - Mod 'Old Thing' (oldthing) 1.0 requires version 1.20.1 of mod 'Minecraft' (minecraft), but only the wrong version is present: 1.21.1!\n\
\t - Mod 'Shiny' (shiny) 2.0 requires version 0.17.0 or later of fabricloader, but only the wrong version is present: 0.16.10!\n\
\t - Mod 'Shaders' (shaders) 1.0 is incompatible with any version of mod 'OptiFabric' (optifabric), but a matching version is present: 1.14.3!\n";

    #[test]
    fn fabric_resolution_suspects_and_fixes() {
        let mut d = analyze(FABRIC_RESOLUTION);
        assert_eq!(d.kind, CrashKind::MissingDependency);
        assert_eq!(d.mods, ["fabric-api"]);
        let found = reasons(&d);
        for expected in [
            (Some("modmenu"), SuspectReason::RequiresMissing),
            (Some("fabric-api"), SuspectReason::Missing),
            (Some("iris"), SuspectReason::RequiresOtherVersion),
            (Some("sodium"), SuspectReason::WrongVersion),
            (Some("oldthing"), SuspectReason::IncompatibleMinecraft),
            (Some("shiny"), SuspectReason::NeedsNewerLoader),
            (Some("shaders"), SuspectReason::DeclaresIncompatible),
            (Some("optifabric"), SuspectReason::Incompatible),
        ] {
            assert!(found.contains(&expected), "{expected:?} in {found:?}");
        }
        assert_eq!(
            found.len(),
            8,
            "solution lines merge into the same suspects"
        );
        let sodium = d
            .suspects
            .iter()
            .find(|s| s.reason == SuspectReason::WrongVersion)
            .unwrap();
        assert_eq!(sodium.requirement.as_deref(), Some(">=0.6.0"));
        assert_eq!(sodium.version.as_deref(), Some("0.5.8+mc1.21"));
        assert_eq!(sodium.related.as_deref(), Some("iris"));
        assert!(d.mod_fixes.is_empty(), "fixes need the mods folder");

        let mods = snapshot(vec![
            ("modmenu.jar", fabric_jar("modmenu", "11.0.1", "")),
            ("iris.jar", fabric_jar("iris", "1.8.0", "")),
            (
                "sodium-fabric.jar",
                fabric_jar("sodium", "0.5.8+mc1.21", r#""name": "Sodium""#),
            ),
            ("oldthing.jar", fabric_jar("oldthing", "1.0", "")),
            ("shiny.jar", fabric_jar("shiny", "2.0", "")),
            ("shaders.jar", fabric_jar("shaders", "1.0", "")),
            ("optifabric.jar", fabric_jar("optifabric", "1.14.3", "")),
            (
                "fabric-api.jar.disabled",
                fabric_jar("fabric-api", "0.102.0", r#""name": "Fabric API""#),
            ),
        ]);
        attribute_crash(&mut d, &mods, &fabric_env());
        assert_eq!(
            d.mod_fixes,
            vec![
                ModFix::EnableFile {
                    file: "fabric-api.jar.disabled".into(),
                    mod_id: "fabric-api".into()
                },
                ModFix::UpdateToCompatible {
                    file: "sodium-fabric.jar".into(),
                    mod_id: "sodium".into(),
                    version_req: Some(">=0.6.0".into()),
                    disable_if_unavailable: false
                },
                ModFix::UpdateToCompatible {
                    file: "oldthing.jar".into(),
                    mod_id: "oldthing".into(),
                    version_req: None,
                    disable_if_unavailable: true
                },
                ModFix::UpdateLoader {
                    loader: LoaderKind::Fabric,
                    version_req: ">=0.17.0".into()
                },
                ModFix::DisableFile {
                    file: "optifabric.jar".into(),
                    reason: DisableReason::Breaks
                },
            ]
        );
        let sodium = d
            .suspects
            .iter()
            .find(|s| s.reason == SuspectReason::WrongVersion)
            .unwrap();
        assert_eq!(sodium.file.as_deref(), Some("sodium-fabric.jar"));
        assert_eq!(sodium.name.as_deref(), Some("Sodium"));
        // The missing mod has a disabled copy, shown by its own name.
        assert_eq!(d.mods, ["Fabric API"]);
        assert!(d.hint_tr.contains("Fabric API"), "{}", d.hint_tr);
        assert_eq!(d.suggested_actions.last(), Some(&SuggestedAction::OpenLogs));

        // Without a disabled copy the dependency is downloaded for the mods that need it; fixes
        // that need a jar are left out, the loader update is not.
        let mut d = analyze(FABRIC_RESOLUTION);
        attribute_crash(&mut d, &snapshot(Vec::new()), &fabric_env());
        assert_eq!(
            d.mod_fixes,
            vec![
                ModFix::FetchDependency {
                    mod_id: "fabric-api".into(),
                    version_req: None,
                    loader: LoaderKind::Fabric,
                    minecraft: "1.21.1".into(),
                    required_by: vec!["modmenu".into()]
                },
                ModFix::UpdateLoader {
                    loader: LoaderKind::Fabric,
                    version_req: ">=0.17.0".into()
                },
            ]
        );
        assert_eq!(d.mods, ["fabric-api"]);
    }

    /// Fabric Loader 0.19.5 with a mod made for 1.20.x in a 1.21.1 instance (from a real run):
    /// the built-in target has no "mod" prefix and the suggested replacement names the same mod.
    const FABRIC_019_OTHER_MINECRAFT: &str = "[08:11:54] [main/ERROR] [FabricLoader]: Incompatible mods found!\n\
net.fabricmc.loader.impl.FormattedException: Some of your mods are incompatible with the game or each other!\n\
A potential solution has been determined, this may resolve your problem:\n\
\t - Replace mod 'M6 Eski Sürüm' (m6eskisurum) 1.0.0 with any version that is compatible with:\n\
\t\t - minecraft 1.21.1\n\
More details:\n\
\t - Mod 'M6 Eski Sürüm' (m6eskisurum) 1.0.0 requires any 1.20.x version of 'Minecraft' (minecraft), but only the wrong version is present: 1.21.1!\n\
\tat net.fabricmc.loader.impl.FormattedException.ofLocalized(FormattedException.java:51)\n";

    #[test]
    fn real_fabric_019_minecraft_mismatch() {
        let mut d = analyze(FABRIC_019_OTHER_MINECRAFT);
        assert_eq!(d.kind, CrashKind::IncompatibleMods);
        assert_eq!(
            reasons(&d),
            vec![(Some("m6eskisurum"), SuspectReason::IncompatibleMinecraft)]
        );
        let mods = snapshot(vec![(
            "m6-eski-surum.jar",
            fabric_jar(
                "m6eskisurum",
                "1.0.0",
                r#""depends": {"minecraft": "1.20.x"}"#,
            ),
        )]);
        attribute_crash(&mut d, &mods, &fabric_env());
        assert_eq!(d.suspects[0].file.as_deref(), Some("m6-eski-surum.jar"));
        assert_eq!(
            d.mod_fixes,
            vec![ModFix::UpdateToCompatible {
                file: "m6-eski-surum.jar".into(),
                mod_id: "m6eskisurum".into(),
                version_req: None,
                disable_if_unavailable: true,
            }]
        );
    }

    #[test]
    fn mixin_failures_map_configs_to_jars() {
        let log = "[12:00:05] [Render thread/ERROR]: Mixin apply for mod foomod failed mixins.foomod.json:client.HudMixin from mod foomod -> net.minecraft.class_329: org.spongepowered.asm.mixin.injection.throwables.InvalidInjectionException Critical injection failure: @Inject annotation on renderHud could not find any targets matching 'render' in net.minecraft.class_329. Using refmap foomod-refmap.json [PREINJECT Applicator Phase -> mixins.foomod.json:client.HudMixin from mod foomod -> Prepare Injections -> handler$zza000$foomod$renderHud(Lnet/minecraft/class_332;)V -> Parse]\n\
org.spongepowered.asm.mixin.transformer.throwables.MixinTransformerError: An unexpected critical error was encountered\n\
Caused by: org.spongepowered.asm.mixin.throwables.MixinApplyError: Mixin [barmod.client.json:BarMixin] from phase [DEFAULT] in config [barmod.client.json] FAILED during APPLY\n";
        let mut d = analyze(log);
        assert_eq!(d.kind, CrashKind::ModError);
        assert_eq!(d.mods, ["foomod"]);
        assert_eq!(d.suspects.len(), 2, "{:#?}", d.suspects);
        assert_eq!(
            d.suspects[1].mixin_config.as_deref(),
            Some("barmod.client.json")
        );

        let mods = snapshot(vec![
            (
                "foo.jar",
                fabric_jar(
                    "foomod",
                    "1.0",
                    r#""name": "Foo Mod", "mixins": ["mixins.foomod.json"]"#,
                ),
            ),
            (
                "bar.jar",
                fabric_jar(
                    "barmod",
                    "2.0",
                    r#""name": "Bar Mod", "mixins": [{"config": "barmod.client.json", "environment": "client"}]"#,
                ),
            ),
            (
                "other.jar",
                fabric_jar("other", "1.0", r#""mixins": ["other.mixins.json"]"#),
            ),
        ]);
        attribute_crash(&mut d, &mods, &fabric_env());
        let files: Vec<&str> = d
            .suspects
            .iter()
            .filter_map(|s| s.file.as_deref())
            .collect();
        assert_eq!(files, ["foo.jar", "bar.jar"]);
        assert_eq!(d.suspects[1].mod_id.as_deref(), Some("barmod"));
        assert_eq!(d.mods, ["Foo Mod", "Bar Mod"]);
        assert!(
            d.hint_tr
                .starts_with("Şu modlar oyunun açılmasını engelledi: Foo Mod, Bar Mod."),
            "{}",
            d.hint_tr
        );
        assert_eq!(
            d.suggested_actions[0],
            SuggestedAction::UpdateMods {
                mods: vec!["Foo Mod".into(), "Bar Mod".into()]
            }
        );
        assert_eq!(d.mod_fixes.len(), 2);
        assert!(
            matches!(&d.mod_fixes[1], ModFix::UpdateToCompatible { file, .. } if file == "bar.jar")
        );
    }

    #[test]
    fn forge_crash_report_suspects() {
        let report = "---- Minecraft Crash Report ----\n// Who set us up the TNT?\n\nTime: 2026-10-16 12:00:00\nDescription: Ticking entity\n\n\
java.lang.NullPointerException: Cannot invoke \"Object.toString()\" because \"x\" is null\n\
\tat TRANSFORMER/create@0.5.1.f/com.simibubi.create.content.Foo.tick(Foo.java:42) ~[create-1.20.1-0.5.1.f.jar%23183!/:0.5.1.f] {re:classloading}\n\
\tat TRANSFORMER/minecraft@1.20.1/net.minecraft.world.level.Level.tick(Level.java:100) ~[client-1.20.1-srg.jar%23178!/:?]\n\n\
A detailed walkthrough of the error, its code path and all known details is as follows:\n\
---------------------------------------------------------------------------------------\n\n\
-- Head --\nThread: Server thread\nSuspected Mod: \n\
\tCreate (create), Version: 0.5.1.f\n\
\t\tIssue tracker URL: https://github.com/Creators-of-Create/Create/issues\n\
\t\tat TRANSFORMER/create@0.5.1.f/com.simibubi.create.content.Foo.tick(Foo.java:42)\n\
Stacktrace:\n\tat TRANSFORMER/create@0.5.1.f/com.simibubi.create.content.Foo.tick(Foo.java:42)\n";
        let mut d = analyze_crash(&CrashInput {
            output: "",
            crash_report: Some(report),
            exit_code: Some(1),
            ..CrashInput::default()
        });
        assert_eq!(d.kind, CrashKind::Unknown);
        assert_eq!(
            reasons(&d),
            [(Some("create"), SuspectReason::NamedInReport)]
        );
        assert_eq!(d.suspects[0].version.as_deref(), Some("0.5.1.f"));
        let title = d.title_tr.clone();
        let mods = snapshot(vec![
            (
                "create-1.20.1-0.5.1.f.jar",
                toml_jar(false, "create", "0.5.1.f", ""),
            ),
            ("jei.jar", toml_jar(false, "jei", "15.2.0", "")),
        ]);
        attribute_crash(&mut d, &mods, &env(LoaderKind::Forge, "1.20.1", "47.2.0"));
        assert_eq!(
            d.suspects[0].file.as_deref(),
            Some("create-1.20.1-0.5.1.f.jar")
        );
        assert_eq!(
            d.mod_fixes,
            vec![ModFix::UpdateToCompatible {
                file: "create-1.20.1-0.5.1.f.jar".into(),
                mod_id: "create".into(),
                version_req: None,
                disable_if_unavailable: false
            }]
        );
        assert_eq!(
            d.title_tr, title,
            "texts of unknown crashes stay as they are"
        );

        // "Suspected Mods: NONE" names nobody; FML frames are the last resort.
        let d = analyze(
            "Suspected Mods: NONE\njava.lang.IllegalStateException: boom\n\tat TRANSFORMER/create@0.5.1.f/com.simibubi.create.Foo.bar(Foo.java:1)\n\tat TRANSFORMER/minecraft@1.20.1/net.minecraft.A.b(A.java:1)",
        );
        assert_eq!(reasons(&d), [(Some("create"), SuspectReason::InStackTrace)]);
    }

    #[test]
    fn fabric_mixin_frames_are_weak_suspects() {
        let mut d = analyze(
            "java.lang.IllegalStateException: boom\n\tat net.minecraft.class_310.handler$zza000$sodium$onRender(class_310.java:1)\n\tat net.minecraft.class_310.method_1(class_310.java:2)",
        );
        assert_eq!(d.kind, CrashKind::Unknown);
        assert_eq!(reasons(&d), [(Some("sodium"), SuspectReason::InStackTrace)]);
        let mods = snapshot(vec![(
            "sodium.jar",
            fabric_jar("sodium", "0.6.0", r#""name": "Sodium""#),
        )]);
        attribute_crash(&mut d, &mods, &fabric_env());
        assert_eq!(d.suspects[0].name.as_deref(), Some("Sodium"));
        assert!(
            d.mod_fixes.is_empty(),
            "weak evidence is shown, never repaired"
        );
    }

    #[test]
    fn duplicate_files_turn_off_the_older_copy() {
        let mut d = analyze(
            "Found duplicate mods:\n\tMod ID: 'jei' from mod files: jei-1.20.1-forge-15.2.0.27.jar, jei-1.20.1-forge-15.3.0.4.jar",
        );
        assert_eq!(d.kind, CrashKind::DuplicateMods);
        assert_eq!(d.suspects.len(), 2);
        let mods = snapshot(vec![
            (
                "jei-1.20.1-forge-15.2.0.27.jar",
                toml_jar(false, "jei", "15.2.0.27", ""),
            ),
            (
                "jei-1.20.1-forge-15.3.0.4.jar",
                toml_jar(false, "jei", "15.3.0.4", ""),
            ),
        ]);
        attribute_crash(&mut d, &mods, &env(LoaderKind::Forge, "1.20.1", "47.2.0"));
        assert_eq!(
            d.mod_fixes,
            vec![ModFix::DisableFile {
                file: "jei-1.20.1-forge-15.2.0.27.jar".into(),
                reason: DisableReason::Duplicate
            }]
        );
        assert_eq!(d.mods, ["jei name"]);

        // A log that names the mod only: the copies are found in the folder.
        let mut d = analyze("Duplicate mods found:\n\t - Mod 'Sodium' (sodium)\n");
        assert_eq!(d.kind, CrashKind::DuplicateMods);
        assert_eq!(reasons(&d), [(Some("sodium"), SuspectReason::Duplicate)]);
        let mods = snapshot(vec![
            ("sodium-a.jar", fabric_jar("sodium", "0.5.0", "")),
            ("sodium-b.jar", fabric_jar("sodium", "0.6.0", "")),
        ]);
        attribute_crash(&mut d, &mods, &fabric_env());
        assert_eq!(
            d.mod_fixes,
            vec![ModFix::DisableFile {
                file: "sodium-a.jar".into(),
                reason: DisableReason::Duplicate
            }]
        );
    }

    #[test]
    fn neoforge_loading_issues() {
        let log = "-- Mod loading issue for: rei --\nDetails:\n\tMod file: /C:/Users/x/AppData/Roaming/projects/instances/a/mods/RoughlyEnoughItems-16.0.729.jar\n\tFailure message: Mod rei requires architectury 13.0.6 or above\n\t\tCurrently, architectury is not installed\n\tMod version: 16.0.729\n";
        let mut d = analyze(log);
        assert_eq!(d.kind, CrashKind::MissingDependency);
        assert_eq!(d.mods, ["architectury"]);
        let rei = d
            .suspects
            .iter()
            .find(|s| s.reason == SuspectReason::LoadingFailed)
            .unwrap();
        assert_eq!(rei.file.as_deref(), Some("RoughlyEnoughItems-16.0.729.jar"));
        let neo = env(LoaderKind::NeoForge, "1.21.1", "21.1.77");
        let mods = snapshot(vec![(
            "RoughlyEnoughItems-16.0.729.jar",
            toml_jar(true, "rei", "16.0.729", ""),
        )]);
        attribute_crash(&mut d, &mods, &neo);
        assert_eq!(
            d.mod_fixes,
            vec![ModFix::FetchDependency {
                mod_id: "architectury".into(),
                version_req: Some("[13.0.6,)".into()),
                loader: LoaderKind::NeoForge,
                minecraft: "1.21.1".into(),
                required_by: vec!["rei".into()]
            }]
        );

        // A wrong version and a game version mismatch.
        let log = "Mod jade requires minecraft 1.21.2 or above\n\tCurrently, minecraft is 1.21.1\nMod emi requires jei 19.0 or above\n\tCurrently, jei is 18.2.1\n";
        let mut d = analyze(log);
        assert_eq!(d.kind, CrashKind::IncompatibleMods);
        assert_eq!(d.mods, ["jade", "emi"]);
        let mods = snapshot(vec![
            ("jade.jar", toml_jar(true, "jade", "15.0", "")),
            ("emi.jar", toml_jar(true, "emi", "1.1", "")),
            ("jei.jar", toml_jar(true, "jei", "18.2.1", "")),
        ]);
        attribute_crash(&mut d, &mods, &neo);
        assert_eq!(
            d.mod_fixes,
            vec![
                ModFix::UpdateToCompatible {
                    file: "jade.jar".into(),
                    mod_id: "jade".into(),
                    version_req: None,
                    disable_if_unavailable: true
                },
                ModFix::UpdateToCompatible {
                    file: "jei.jar".into(),
                    mod_id: "jei".into(),
                    version_req: Some("[19.0,)".into()),
                    disable_if_unavailable: false
                },
            ]
        );
    }

    #[test]
    fn forge_mod_loading_section() {
        let report = "---- Minecraft Crash Report ----\nDescription: Mod loading error has occurred\n\njava.lang.Exception: Mod Loading has failed\n\n\
-- MOD create --\nDetails:\n\tCaught exception: java.lang.NullPointerException\n\tMod File: /C:/game/mods/create-1.20.1-0.5.1.f.jar\n\
\tFailure message: Create (create) encountered an error during the common_setup event phase\n\t\tjava.lang.NullPointerException: null\n\tMod Version: 0.5.1.f\n";
        let d = analyze_crash(&CrashInput {
            output: "",
            crash_report: Some(report),
            exit_code: Some(1),
            ..CrashInput::default()
        });
        assert_eq!(d.kind, CrashKind::ModError);
        assert_eq!(d.mods, ["Create"]);
        assert_eq!(
            reasons(&d),
            [(Some("create"), SuspectReason::LoadingFailed)]
        );
        assert_eq!(
            d.suspects[0].file.as_deref(),
            Some("create-1.20.1-0.5.1.f.jar")
        );
    }

    #[test]
    fn class_compiled_for_newer_java() {
        let mut d = analyze(
            "Exception in thread \"main\" java.lang.UnsupportedClassVersionError: com/example/newmod/NewMod has been compiled by a more recent version of the Java Runtime (class file version 69.0), this version of the Java Runtime only recognizes class file versions up to 65.0",
        );
        assert_eq!(d.kind, CrashKind::JavaTooOld);
        assert_eq!(
            d.suspects[0].class_name.as_deref(),
            Some("com.example.newmod.NewMod")
        );
        let newmod = zip_bytes(&[
            (
                "fabric.mod.json",
                fabric_json("newmod", "1.0", r#""name": "New Mod""#).as_bytes(),
            ),
            ("com/example/newmod/NewMod.class", b"\xca\xfe"),
        ]);
        let mods = snapshot(vec![("newmod.jar", newmod)]);
        attribute_crash(&mut d, &mods, &fabric_env());
        assert_eq!(d.suspects[0].name.as_deref(), Some("New Mod"));
        assert_eq!(d.suspects[0].file.as_deref(), Some("newmod.jar"));
        assert_eq!(d.mod_fixes, vec![ModFix::UseJava { major: 25 }]);

        // The game's own classes name no mod.
        let d = analyze(
            "java.lang.UnsupportedClassVersionError: net/minecraft/client/main/Main has been compiled by a more recent version of the Java Runtime (class file version 65.0)",
        );
        assert!(d.suspects.is_empty());
    }

    #[test]
    fn corrupt_jar_named_by_the_loader() {
        let mut d = analyze(
            "net.fabricmc.loader.impl.discovery.ModResolutionException: Mod discovery failed!\nCaused by: java.lang.RuntimeException: Error analyzing [C:\\game\\mods\\broken.jar]: java.util.zip.ZipException: zip END header not found",
        );
        assert_eq!(d.kind, CrashKind::CorruptedFiles);
        assert_eq!(d.suspects[0].file.as_deref(), Some("broken.jar"));
        let mods = snapshot(vec![("broken.jar", b"<html>".to_vec())]);
        attribute_crash(&mut d, &mods, &fabric_env());
        assert_eq!(
            d.mod_fixes,
            vec![ModFix::DisableFile {
                file: "broken.jar".into(),
                reason: DisableReason::Corrupt
            }]
        );
    }

    #[test]
    fn other_failures_name_no_mods() {
        let mods = snapshot(vec![
            ("create.jar", toml_jar(false, "create", "0.5.1", "")),
            ("sodium.jar", fabric_jar("sodium", "0.6.0", "")),
        ]);
        for log in [
            "java.lang.OutOfMemoryError: Java heap space\n\tat TRANSFORMER/create@0.5.1/com.simibubi.create.Foo.bar(Foo.java:1)",
            "#  EXCEPTION_ACCESS_VIOLATION (0xc0000005)\n# C  [nvoglv64.dll+0xd5c3a]\n\tat net.minecraft.class_310.handler$zza000$sodium$onRender(class_310.java:1)",
            "Could not reserve enough space for object heap",
        ] {
            let mut d = analyze(log);
            assert!(d.suspects.is_empty(), "{log}");
            attribute_crash(&mut d, &mods, &fabric_env());
            assert!(d.mod_fixes.is_empty(), "{log}");
        }
    }

    #[test]
    fn odd_text_never_panics() {
        let mods = snapshot(vec![
            ("x.jar", b"nope".to_vec()),
            ("y.jar.disabled", fabric_jar("y", "1", "")),
        ]);
        for log in [
            "",
            "Mod '' () requires  of , which is missing!",
            "Mod ID: '', Requested by: '', Expected range: '', Actual version: ''",
            "-- MOD  --\n-- MOD x --",
            "Suspected Mods:\n\t\n\t ( ), Version: \n",
            "Found duplicate mods:\n\tMod ID: 'a' from mod files: , ,",
            "Mixin [.json:] in config [.json] FAILED during APPLY",
            "UnsupportedClassVersionError: a has been compiled by a more recent version of the Java Runtime (class file version 99999999999999999999)",
            "Mod a requires b \n Currently, b is not installed\r\nMod c is incompatible with d",
            "\u{feff}\r\n\r\n\t at TRANSFORMER/\u{e7}@/",
            "Duplicate mandatory mod ids []\n- \n- <mods>/\n",
            "x requires version [ of y, which is missing!\n- <mods>/.jar",
            "x requires version [∞ of y, but only wrong versions are present:",
            "Mod a only supports b between and (inclusive)\nCurrently, b is 1",
            "Suspected Mods: (), ( ), x (y",
            "-- Mod loading issue for: a --\nCaused by 0: java.lang.NoClassDefFoundError: Could not initialize class X",
            "Mixin application of  from  () has failed",
            "§§r§\u{a7}",
            "File .jar is not a valid mod file",
        ] {
            let mut d = analyze(log);
            attribute_crash(&mut d, &mods, &fabric_env());
            attribute_crash(&mut d, &ModSnapshot::default(), &fabric_env());
        }
    }

    // --- real logs and crash reports ---
    // Trimmed, anonymized copies of public reports in `testdata/crash`; each file's first lines
    // name its source.

    /// A sample without its `# source:` / `# loader:` / `# note:` lines.
    fn sample(text: &str) -> String {
        text.lines()
            .skip_while(|line| line.starts_with("# "))
            .collect::<Vec<_>>()
            .join("\n")
    }

    macro_rules! sample {
        ($name:literal) => {
            sample(include_str!(concat!("../../testdata/crash/", $name)))
        };
    }

    fn analyze_report(report: &str) -> CrashDiagnosis {
        analyze_crash(&CrashInput {
            output: "",
            crash_report: Some(report),
            exit_code: Some(1),
            runtime: Duration::from_secs(3),
            java_major: Some(21),
            required_java: Some(21),
            ..CrashInput::default()
        })
    }

    #[test]
    fn real_graphics_lines_are_not_driver_crashes() {
        // Sodium's graphics adapter line names the driver's file (alyti/fluxnautics#7).
        let d = analyze(&sample!(
            "neoforge-1.21.1-sodium-adapter-line-latest-log.txt"
        ));
        assert_eq!(d.kind, CrashKind::ModError);
        assert_eq!(d.mods, ["fluxnautics"]);
        // So does the crash report's module list (AGM-Studio/NeoBlock#7).
        let d = analyze_report(&sample!(
            "forge-1.20.1-init-crash-modules-list-crash-report.txt"
        ));
        assert_eq!(d.kind, CrashKind::ModError);
        assert_eq!(d.mods, ["neoblock"]);
        assert_eq!(
            d.suspects[0].mixin_config.as_deref(),
            Some("neoblock.mixins.json")
        );
        // Minecraft 1.21.5+ logs "Can't getDevice() before it was initialized" whenever a crash
        // report is written before the window exists.
        let d = analyze(&sample!(
            "fabric-1.21.11-render-extensions-warning-latest-log.txt"
        ));
        assert_eq!(d.kind, CrashKind::Unknown);
        assert!(d.suspects.is_empty(), "{:?}", d.suspects);
        // Crashes in the driver itself still are driver crashes.
        let d = analyze("# Problematic frame:\n# C  [atio6axx.dll+0x1b8a0f]\n");
        assert_eq!(d.kind, CrashKind::GraphicsDriver);
        let d = analyze(
            "[Render thread/ERROR]: Unreported exception thrown!\njava.lang.IllegalStateException: Can't getDevice() before it was initialized\n",
        );
        assert_eq!(d.kind, CrashKind::GraphicsDriver);
    }

    #[test]
    fn real_mod_errors_name_only_the_failing_mod() {
        // "Method overwrite conflict ... from mod lithium" and BadOptimizations' "mod \"polytone\"
        // is incompatible with it" are not the problem.
        let d = analyze(&sample!("neoforge-1.21.1-mixin-apply-latest-log.txt"));
        assert_eq!(d.kind, CrashKind::ModError);
        assert_eq!(d.mods, ["ji_afk_cinematic"]);
        assert_eq!(
            reasons(&d),
            [(Some("ji_afk_cinematic"), SuspectReason::MixinFailed)]
        );
        // Mixin 0.8.5 on Forge names the config only.
        let d = analyze(&sample!("forge-1.20.1-mixin-apply-failed-latest-log.txt"));
        assert_eq!(d.mods, ["ae2"]);
        assert_eq!(reasons(&d), [(None, SuspectReason::MixinFailed)]);

        // NeoForge's "Mixin application of ... has failed". The mods that failed only because
        // Create's classes could not load after it are left out.
        let mut d = analyze_report(&sample!(
            "neoforge-1.21.1-mixin-application-crash-report.txt"
        ));
        assert_eq!(d.kind, CrashKind::ModError);
        assert_eq!(
            reasons(&d),
            [(Some("bits_n_bobs"), SuspectReason::MixinFailed)]
        );
        assert_eq!(d.mods, ["Create Bits 'n' Bobs"]);
        let mods = snapshot(vec![
            (
                "bits_n_bobs-0.0.34.jar",
                toml_jar(true, "bits_n_bobs", "0.0.34", ""),
            ),
            ("create-6.0.6.jar", toml_jar(true, "create", "6.0.6", "")),
        ]);
        attribute_crash(
            &mut d,
            &mods,
            &env(LoaderKind::NeoForge, "1.21.1", "21.1.248"),
        );
        assert_eq!(
            d.mod_fixes,
            vec![ModFix::UpdateToCompatible {
                file: "bits_n_bobs-0.0.34.jar".into(),
                mod_id: "bits_n_bobs".into(),
                version_req: None,
                disable_if_unavailable: false
            }]
        );

        // Stack frames tagged "from mod veil" did not fail.
        let d = analyze_report(&sample!("neoforge-1.21.1-event-dispatch-crash-report.txt"));
        assert_eq!(d.kind, CrashKind::ModError);
        assert_eq!(d.mods, ["Nox Lights"]);
        assert_eq!(
            reasons(&d),
            [(Some("noxlights"), SuspectReason::LoadingFailed)]
        );
        assert_eq!(d.suspects[0].file.as_deref(), Some("noxlights-1.0.0.jar"));
        let d = analyze(&sample!("neoforge-1.21.1-failed-to-load-latest-log.txt"));
        assert_eq!(d.mods, ["Amaranth"]);
        assert_eq!(
            reasons(&d),
            [(Some("amaranth"), SuspectReason::LoadingFailed)]
        );

        // The one-line "Suspected Mods" of the system details; the loader itself is no suspect.
        let d = analyze_report(&sample!(
            "neoforge-1.21.1-suspected-mods-inline-crash-report.txt"
        ));
        assert_eq!(
            reasons(&d),
            [(Some("rechiseled"), SuspectReason::NamedInReport)]
        );

        // Fabric names the mod whose entrypoint failed; "recommends ..., which is missing!"
        // warnings are no missing mods.
        let d = analyze(&sample!(
            "fabric-1.20.1-recommends-warnings-false-missing.txt"
        ));
        assert_eq!(d.kind, CrashKind::ModError);
        assert_eq!(
            reasons(&d),
            [(Some("generations_core"), SuspectReason::LoadingFailed)]
        );
    }

    #[test]
    fn real_dependency_errors() {
        let d = analyze_report(&sample!("forge-1.20.1-missing-dependency-crash-report.txt"));
        assert_eq!(d.kind, CrashKind::MissingDependency);
        assert_eq!(d.mods, ["architectury"]);
        let found = reasons(&d);
        for expected in [
            (Some("perception"), SuspectReason::RequiresMissing),
            (Some("octolib"), SuspectReason::RequiresMissing),
            (Some("architectury"), SuspectReason::Missing),
        ] {
            assert!(found.contains(&expected), "{expected:?} in {found:?}");
        }
        assert!(d.suspects.iter().any(|s| {
            s.file
                .as_deref()
                .is_some_and(|f| f.ends_with(" Perception-FORGE-0.1.2.1+1.20.1.jar"))
        }));

        // Forge 1.16 keeps the formatting codes in its message.
        let d = analyze(&sample!(
            "forge-1.16.5-missing-dependency-color-codes-latest-log.txt"
        ));
        assert_eq!(d.kind, CrashKind::MissingDependency);
        assert_eq!(d.mods, ["collective"]);
        let collective = d
            .suspects
            .iter()
            .find(|s| s.reason == SuspectReason::Missing)
            .unwrap();
        assert_eq!(collective.requirement.as_deref(), Some("[2.27,)"));

        let d = analyze(&sample!("neoforge-1.21.1-wrong-version-latest-log.txt"));
        assert_eq!(d.kind, CrashKind::IncompatibleMods);
        assert_eq!(d.mods, ["properproxyprotocol"]);
        let architectury = d
            .suspects
            .iter()
            .find(|s| s.reason == SuspectReason::WrongVersion)
            .unwrap();
        assert_eq!(architectury.version.as_deref(), Some("13.0.8"));
        assert_eq!(architectury.requirement.as_deref(), Some("[19.0.0,)"));

        // An optional dependency ("only supports") and a jar NeoForge refused, in one report.
        let mut d = analyze_report(&sample!(
            "neoforge-1.21.1-only-supports-and-invalid-file-crash-report.txt"
        ));
        assert_eq!(d.kind, CrashKind::IncompatibleMods);
        let mods = snapshot(vec![
            (
                "controllable-neoforge-1.21.1-0.25.4.jar",
                toml_jar(true, "controllable", "0.25.4", ""),
            ),
            (
                "jei-1.21.1-neoforge-19.25.1.334.jar",
                toml_jar(true, "jei", "19.25.1.334", ""),
            ),
            (
                "GrandTeleport-NeoForge-1.21.1-neoforge-1.0.1.jar",
                toml_jar(true, "grandteleport", "1.0.1", ""),
            ),
        ]);
        attribute_crash(
            &mut d,
            &mods,
            &env(LoaderKind::NeoForge, "1.21.1", "21.1.235"),
        );
        assert_eq!(
            d.mod_fixes,
            vec![
                ModFix::UpdateToCompatible {
                    file: "jei-1.21.1-neoforge-19.25.1.334.jar".into(),
                    mod_id: "jei".into(),
                    version_req: Some("[19.27.0.336,)".into()),
                    disable_if_unavailable: false
                },
                ModFix::DisableFile {
                    file: "GrandTeleport-NeoForge-1.21.1-neoforge-1.0.1.jar".into(),
                    reason: DisableReason::Corrupt
                },
            ]
        );

        // A mod pinned to an older NeoForge build: the mod has to change, not the loader.
        let mut d = analyze_report(&sample!(
            "neoforge-1.21.1-exact-loader-pin-crash-report.txt"
        ));
        assert_eq!(d.kind, CrashKind::IncompatibleMods);
        let pin = d
            .suspects
            .iter()
            .find(|s| s.reason == SuspectReason::RequiresOtherVersion)
            .unwrap();
        assert_eq!(pin.mod_id.as_deref(), Some("sepals"));
        assert_eq!(pin.related.as_deref(), Some("neoforge"));
        assert_eq!(pin.requirement.as_deref(), Some("[21.1.132]"));
        let mods = snapshot(vec![(
            "Sepals-neoforge-1.0.15.jar",
            toml_jar(true, "sepals", "1.0.15", ""),
        )]);
        attribute_crash(
            &mut d,
            &mods,
            &env(LoaderKind::NeoForge, "1.21.1", "21.1.214"),
        );
        assert_eq!(
            d.mod_fixes,
            vec![ModFix::UpdateToCompatible {
                file: "Sepals-neoforge-1.0.15.jar".into(),
                mod_id: "sepals".into(),
                version_req: None,
                disable_if_unavailable: false
            }]
        );
        // The same pin in the log's dependency list (cao-awa/Sepals#56).
        let d = analyze(
            "Missing or unsupported mandatory dependencies:\n\tMod ID: 'neoforge', Requested by: 'sepals', Expected range: '[21.1.132,21.1.132]', Actual version: '21.1.230'\n",
        );
        assert_eq!(
            reasons(&d),
            [(Some("sepals"), SuspectReason::RequiresOtherVersion)]
        );
        assert_eq!(d.suspects[0].requirement.as_deref(), Some("[21.1.132]"));

        let d = analyze(&sample!("neoforge-1.21.1-incompatible-latest-log.txt"));
        assert_eq!(d.kind, CrashKind::IncompatibleMods);
        assert_eq!(
            reasons(&d),
            [
                (
                    Some("reeses_sodium_options"),
                    SuspectReason::DeclaresIncompatible
                ),
                (Some("sodiumoptionsapi"), SuspectReason::Incompatible),
            ]
        );

        // Forge 1.12 display names may contain parentheses.
        let d = analyze(
            "net.minecraftforge.fml.common.MissingModsException: Mod pack (Hero Aviation Helicopter Pack (HA/HP)) requires [heroaviation@[1.0,)]",
        );
        assert_eq!(d.mods, ["heroaviation"]);
    }

    #[test]
    fn real_quilt_errors() {
        // A range in Quilt's notation; the requiring mod is named by its display name.
        let mut d = analyze_report(&sample!("quilt-1.20.1-missing-dependency-crash-report.txt"));
        assert_eq!(d.kind, CrashKind::MissingDependency);
        assert_eq!(d.mods, ["fabric-renderer-api-v1"]);
        let mods = snapshot(vec![(
            "indium-1.0.27+mc1.20.1.jar",
            fabric_jar("indium", "1.0.27+mc1.20.1", r#""name": "Indium""#),
        )]);
        attribute_crash(&mut d, &mods, &env(LoaderKind::Quilt, "1.20.1", "0.21.0"));
        assert_eq!(
            d.mod_fixes,
            vec![ModFix::FetchDependency {
                mod_id: "fabric-renderer-api-v1".into(),
                version_req: Some(">=3.2.0".into()),
                loader: LoaderKind::Quilt,
                minecraft: "1.20.1".into(),
                required_by: vec!["indium".into()]
            }]
        );

        // A Fabric Loader version Quilt does not provide: no mod to install, and no loader
        // version to pick from the Fabric numbers.
        let mut d = analyze(&sample!("quilt-1.21.1-missing-dependency-latest-log.txt"));
        assert_eq!(d.kind, CrashKind::IncompatibleMods);
        assert_eq!(d.mods, ["Bookshelf"]);
        assert_eq!(reasons(&d), [(None, SuspectReason::NeedsNewerLoader)]);
        assert_eq!(
            d.suspects[0].file.as_deref(),
            Some("bookshelf-fabric-1.21.1-21.1.3.jar")
        );
        let mods = snapshot(vec![(
            "bookshelf-fabric-1.21.1-21.1.3.jar",
            fabric_jar("bookshelf", "21.1.3", ""),
        )]);
        attribute_crash(&mut d, &mods, &env(LoaderKind::Quilt, "1.21.1", "0.26.4"));
        assert!(d.mod_fixes.is_empty(), "{:?}", d.mod_fixes);

        let mut d = analyze(&sample!("quilt-1.19.2-duplicate-mod-ids.txt"));
        assert_eq!(d.kind, CrashKind::DuplicateMods);
        assert_eq!(d.mods, ["qkl"]);
        assert_eq!(d.suspects.len(), 2);
        // (Versions shortened: the real ones carry two "+" parts, which no version scheme reads.)
        let mods = snapshot(vec![
            (
                "quilt-kotlin-libraries-4.0.0+kt.1.9.23+flk.1.10.19.jar",
                fabric_jar("qkl", "4.0.0", ""),
            ),
            (
                "quilt-kotlin-libraries-1.0.2+kt.1.8.0+flk.1.9.0.jar",
                fabric_jar("qkl", "1.0.2", ""),
            ),
        ]);
        attribute_crash(&mut d, &mods, &env(LoaderKind::Quilt, "1.19.2", "0.26.0"));
        assert_eq!(
            d.mod_fixes,
            vec![ModFix::DisableFile {
                file: "quilt-kotlin-libraries-1.0.2+kt.1.8.0+flk.1.9.0.jar".into(),
                reason: DisableReason::Duplicate
            }]
        );
    }

    #[test]
    fn real_fabric_breaks_wording() {
        // Fabric Loader 0.19.3+: "yet a conflicting version is present". Only old versions of
        // Iris are declared incompatible, so Iris is updated (or turned off if that cannot help).
        let mut d = analyze(&sample!("fabric-1.21.11-incompatible-yet-conflicting.txt"));
        assert_eq!(d.kind, CrashKind::IncompatibleMods);
        assert_eq!(d.mods, ["Sodium", "Iris"]);
        assert_eq!(
            reasons(&d),
            [
                (Some("sodium"), SuspectReason::DeclaresIncompatible),
                (Some("iris"), SuspectReason::Incompatible),
            ]
        );
        let mods = snapshot(vec![
            ("sodium.jar", fabric_jar("sodium", "0.8.13+mc1.21.11", "")),
            ("iris.jar", fabric_jar("iris", "1.10.7+mc1.21.11", "")),
        ]);
        attribute_crash(&mut d, &mods, &env(LoaderKind::Fabric, "1.21.11", "0.19.3"));
        assert_eq!(
            d.mod_fixes,
            vec![ModFix::UpdateToCompatible {
                file: "iris.jar".into(),
                mod_id: "iris".into(),
                version_req: None,
                disable_if_unavailable: true
            }]
        );
        // The plural of the wrong-version wording.
        let d = analyze(
            "Incompatible mods found!\n\t - Mod 'Zoomify' (zoomify) 2.14.0 requires version 3.6.0 or later of mod 'YetAnotherConfigLib' (yet_another_config_lib_v3), but only the wrong versions are present: 3.5.0/3.4.4!\n",
        );
        let yacl = d
            .suspects
            .iter()
            .find(|s| s.reason == SuspectReason::WrongVersion)
            .unwrap();
        assert_eq!(yacl.mod_id.as_deref(), Some("yet_another_config_lib_v3"));
        assert_eq!(yacl.requirement.as_deref(), Some(">=3.6.0"));
    }

    #[test]
    fn real_duplicates_broken_files_and_java() {
        let d = analyze(&sample!("forge-1.20.1-duplicate-mods-latest-log.txt"));
        assert_eq!(d.kind, CrashKind::DuplicateMods);
        assert_eq!(d.mods, ["rubidium"]);
        let files: Vec<&str> = d
            .suspects
            .iter()
            .filter_map(|s| s.file.as_deref())
            .collect();
        assert_eq!(
            files,
            [
                "rubidium-mc1.20.1-0.7.1.jar",
                "embeddium-0.3.31+mc1.20.1.jar"
            ]
        );

        // A jar NeoForge refused is named although its section has no mod id; warnings about
        // other mods' mixins do not count.
        let mut d = analyze(&sample!(
            "neoforge-1.21.1-not-a-valid-mod-file-latest-log.txt"
        ));
        assert_eq!(d.kind, CrashKind::ModError);
        assert_eq!(reasons(&d), [(None, SuspectReason::CorruptFile)]);
        let structures = zip_bytes(&[("data/structory_towers/x.json", b"{}".as_slice())]);
        let mods = snapshot(vec![("Structory_Towers_26.2_v1.0.17.jar", structures)]);
        attribute_crash(
            &mut d,
            &mods,
            &env(LoaderKind::NeoForge, "1.21.1", "21.1.235"),
        );
        assert_eq!(
            d.mod_fixes,
            vec![ModFix::DisableFile {
                file: "Structory_Towers_26.2_v1.0.17.jar".into(),
                reason: DisableReason::Corrupt
            }]
        );
        assert_eq!(d.mods.len(), 1, "named by its jar: {:?}", d.mods);

        let d = analyze(&sample!("forge-1.20.1-zip-end-header-server-log.txt"));
        assert_eq!(d.kind, CrashKind::CorruptedFiles);
        assert!(d.suspects.is_empty());

        // Forge 1.20.1 on Java 24: its Mixin cannot read Java's own classes.
        let log = sample!("forge-1.20.1-java24-class-file-68-server-log.txt");
        let d = analyze_crash(&CrashInput {
            output: &log,
            exit_code: Some(1),
            java_major: Some(24),
            required_java: Some(17),
            ..CrashInput::default()
        });
        assert_eq!(d.kind, CrashKind::JavaTooNew);
        assert!(
            d.hint_tr.contains("Java 17") && d.hint_tr.contains("Java 24"),
            "{}",
            d.hint_tr
        );
        assert_eq!(
            d.suggested_actions[0],
            SuggestedAction::UseAutomaticJava {
                required_major: Some(17)
            }
        );
        assert!(d.suspects.is_empty());
    }

    #[test]
    fn version_wordings() {
        for (prose, maven) in [
            ("13.0.6 or above", Some("[13.0.6,)")),
            ("2.0 or below", Some("(,2.0]")),
            ("above 1.0", Some("(1.0,)")),
            ("below 3", Some("(,3)")),
            ("21.1.132", Some("[21.1.132]")),
            ("1.0 or above, and below 2.0", Some("[1.0,2.0)")),
            ("above 1.0, and 2.0 or below", Some("(1.0,2.0]")),
            ("between 1.0 and 2.0 (inclusive)", Some("[1.0,2.0]")),
            ("between 1.0 and 2.0 (exclusive)", Some("(1.0,2.0)")),
            ("between 1.0 and 2.0", None),
            ("any", None),
            ("", None),
        ] {
            assert_eq!(
                neoforge_prose_requirement(prose).as_deref(),
                maven,
                "{prose}"
            );
        }
        for (prose, fabric) in [
            ("version [3.2.0, ∞)", Some(">=3.2.0")),
            ("version (1.0, 2.0]", Some(">1.0 <=2.0")),
            ("version [1.0, 1.1)", Some(">=1.0 <1.1")),
            ("version 1.2.3", Some("1.2.3")),
            ("any version", None),
            ("version [, ∞)", None),
        ] {
            assert_eq!(quilt_prose_requirement(prose).as_deref(), fabric, "{prose}");
        }
        assert_eq!(exact_maven_range("[21.1.132,21.1.132]"), "[21.1.132]");
        assert_eq!(exact_maven_range("[1.0,2.0)"), "[1.0,2.0)");
    }
}

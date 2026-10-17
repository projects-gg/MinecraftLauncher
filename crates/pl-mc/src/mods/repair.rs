//! Repair plans for one Minecraft version's mod set. The pre-launch check's issues and a
//! crash's findings become one ordered, de-duplicated list of abstract steps ("Onar"); the last
//! resort, reinstalling the version's whole set ("Yeniden yükle"), is described separately by
//! [`plan_reinstall`]. Nothing here uses the network: [`super::apply`] resolves Modrinth
//! versions ([`ModrinthLookup`]) and carries the steps out.
//!
//! Files of the official pack are never upgraded by a repair: a pack file that would need
//! another version goes back to the version chosen in Studio ([`RepairAction::RevertPackFile`],
//! done by the pack sync). Only the player's own mods are updated to compatible versions.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::check::{
    CheckEnvironment, DisableReason, FileOrigin, FileOrigins, IssueKind, ModCheckReport, ModFix,
    ModIssue, Severity,
};
use super::scan::{ModSnapshot, file_stem, strip_disabled};
use crate::launch::crash::CrashDiagnosis;
use crate::loaders::LoaderKind;

/// How phase two finds the right file on Modrinth.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthLookup {
    /// The project, when known (the file was installed from Modrinth).
    pub project_id: Option<String>,
    /// Project slugs to try when the project is not known, best first
    /// ([`modrinth_slug_candidates`]).
    pub slugs: Vec<String>,
    pub loader: LoaderKind,
    pub minecraft: String,
    /// Every requirement the chosen version must meet, in the loader's syntax.
    pub version_reqs: Vec<String>,
}

/// One step of a repair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RepairAction {
    /// Run the official pack sync for this version, so the pack's files match its manifest
    /// (the player's own choices and changes are kept).
    VerifyPackFiles,
    /// Download the pack's copy of a broken pack file again (`path` is the pack path).
    RedownloadPackFile {
        path: String,
    },
    /// Put the version chosen in Studio back in place of a pack file that needs another
    /// version (pack files are never upgraded by a repair).
    RevertPackFile {
        path: String,
        mod_id: String,
    },
    RedownloadModrinthFile {
        file: String,
        project_id: String,
        version_id: Option<String>,
    },
    EnableFile {
        file: String,
        mod_id: String,
    },
    DisableFile {
        file: String,
        reason: DisableReason,
    },
    UpdateLoader {
        loader: LoaderKind,
        version_reqs: Vec<String>,
    },
    UseJava {
        major: u32,
    },
    /// Replace the file with a fitting version of the same mod; when there is none and
    /// `disable_if_unavailable` is set, turn the file off instead.
    UpdateToCompatible {
        file: String,
        mod_id: String,
        disable_if_unavailable: bool,
        lookup: ModrinthLookup,
    },
    FetchDependency {
        mod_id: String,
        /// Ids of the mods that need it.
        required_by: Vec<String>,
        lookup: ModrinthLookup,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RepairStep {
    pub action: RepairAction,
    /// The mod concerned, for the summary shown before repairing (display name when known,
    /// else the mod id or file name).
    pub name: String,
    /// The check's issue kinds this step answers.
    pub because: Vec<IssueKind>,
    /// Asked for by the crash analysis.
    pub from_crash: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RepairPlan {
    pub minecraft: String,
    pub loader: LoaderKind,
    /// In the order to run them: pack files, re-enables, disables, loader and Java, updates,
    /// downloads.
    pub steps: Vec<RepairStep>,
    /// Problems no step answers (a locked file); the UI explains them or offers a reinstall.
    pub unresolved: Vec<ModIssue>,
    /// Some step downloads something.
    pub needs_network: bool,
}

/// What the planner knows about the version's set.
#[derive(Debug, Clone, Copy)]
pub struct RepairContext<'a> {
    pub env: &'a CheckEnvironment,
    pub origins: &'a FileOrigins,
    /// The set includes the official pack (its files can be verified by a sync).
    pub has_pack: bool,
}

/// Issue kinds whose fixes "Onar" applies even though they are only warnings.
fn repairs_warning(kind: IssueKind) -> bool {
    matches!(
        kind,
        IssueKind::Corrupt
            | IssueKind::Duplicate
            | IssueKind::WrongLoader
            | IssueKind::BrokenMetadata
            | IssueKind::IncompatibleMinecraft
    )
}

/// Which reason wins when one file is disabled for several.
fn reason_rank(reason: DisableReason) -> u8 {
    match reason {
        DisableReason::Corrupt => 0,
        DisableReason::ProvidedByLoader => 1,
        DisableReason::Duplicate => 2,
        DisableReason::WrongLoader => 3,
        DisableReason::IncompatibleMinecraft => 4,
        DisableReason::Breaks => 5,
        DisableReason::Crash => 6,
    }
}

fn action_rank(action: &RepairAction) -> u8 {
    match action {
        RepairAction::VerifyPackFiles => 0,
        RepairAction::RedownloadPackFile { .. } | RepairAction::RevertPackFile { .. } => 1,
        RepairAction::RedownloadModrinthFile { .. } => 2,
        RepairAction::EnableFile { .. } => 3,
        RepairAction::DisableFile { .. } => 4,
        RepairAction::UpdateLoader { .. } => 5,
        RepairAction::UseJava { .. } => 6,
        RepairAction::UpdateToCompatible { .. } => 7,
        RepairAction::FetchDependency { .. } => 8,
    }
}

/// The file name of a pack path (`mods/sodium.jar` → `sodium.jar`).
pub(crate) fn pack_file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

fn push_unique(list: &mut Vec<String>, value: Option<String>) {
    if let Some(value) = value
        && !list.contains(&value)
    {
        list.push(value);
    }
}

/// Turn the check's issues and a crash's fixes into one plan. Each issue contributes its
/// preferred fix; fixes for the same file, mod or loader are merged; steps made pointless by
/// another (updating a file that is turned off, downloading a mod only a disabled mod needs,
/// downloading a mod that a re-enabled or replaced file provides) are dropped.
pub fn plan_repair(
    ctx: &RepairContext<'_>,
    report: Option<&ModCheckReport>,
    crash: Option<&CrashDiagnosis>,
) -> RepairPlan {
    let mut wanted: Vec<(ModFix, Option<IssueKind>)> = Vec::new();
    let mut unresolved = Vec::new();
    // File → display name, and file → mod ids, from everything the inputs name.
    let mut names: HashMap<String, String> = HashMap::new();
    let mut ids_in_file: HashMap<String, HashSet<String>> = HashMap::new();
    let mut note = |file: &str, name: Option<&str>, id: Option<&str>| {
        if let Some(name) = name {
            names
                .entry(file.to_owned())
                .or_insert_with(|| name.to_owned());
        }
        if let Some(id) = id {
            ids_in_file
                .entry(file.to_owned())
                .or_default()
                .insert(id.to_owned());
        }
    };

    for issue in report.map(|r| r.issues.as_slice()).unwrap_or_default() {
        for m in &issue.mods {
            note(&m.file, Some(&m.name), m.mod_id.as_deref());
        }
        if let Some(t) = &issue.target
            && let Some(file) = &t.file
        {
            note(file, t.name.as_deref(), None);
        }
        let wanted_here = issue.severity == Severity::Blocking
            || (issue.severity == Severity::Warning && repairs_warning(issue.kind));
        if !wanted_here {
            if issue.severity != Severity::Info && issue.fixes.is_empty() {
                unresolved.push(issue.clone());
            }
            continue;
        }
        match issue.fixes.first() {
            // Every older copy of a duplicate goes, not only the first one.
            Some(ModFix::DisableFile { .. }) if issue.kind == IssueKind::Duplicate => {
                for fix in &issue.fixes {
                    if matches!(fix, ModFix::DisableFile { .. }) {
                        wanted.push((fix.clone(), Some(issue.kind)));
                    }
                }
            }
            Some(fix) => wanted.push((fix.clone(), Some(issue.kind))),
            None => unresolved.push(issue.clone()),
        }
    }
    if let Some(crash) = crash {
        for s in &crash.suspects {
            if let Some(file) = &s.file {
                note(
                    file,
                    s.name.as_deref().or(s.mod_id.as_deref()),
                    s.mod_id.as_deref(),
                );
            }
        }
        wanted.extend(crash.mod_fixes.iter().map(|fix| (fix.clone(), None)));
    }

    let mut steps: Vec<RepairStep> = Vec::new();
    for (fix, kind) in wanted {
        let action = action_for(with_origin(fix, ctx.origins), ctx);
        merge(&mut steps, action, kind);
    }

    // Drop steps another step makes pointless.
    let disabled: HashSet<String> = steps
        .iter()
        .filter_map(|s| match &s.action {
            RepairAction::DisableFile { file, .. } => Some(file.clone()),
            _ => None,
        })
        .collect();
    let disabled_ids: HashSet<&String> = disabled
        .iter()
        .filter_map(|file| ids_in_file.get(file))
        .flatten()
        .collect();
    let provided: HashSet<String> = steps
        .iter()
        .filter_map(|s| match &s.action {
            RepairAction::EnableFile { mod_id, .. }
            | RepairAction::UpdateToCompatible { mod_id, .. }
            | RepairAction::RevertPackFile { mod_id, .. } => Some(mod_id.clone()),
            _ => None,
        })
        .collect();
    steps.retain_mut(|step| match &mut step.action {
        RepairAction::UpdateToCompatible { file, .. } | RepairAction::EnableFile { file, .. } => {
            !disabled.contains(file)
        }
        RepairAction::RevertPackFile { path, .. } => !disabled.contains(pack_file_name(path)),
        RepairAction::FetchDependency {
            mod_id,
            required_by,
            ..
        } => {
            if provided.contains(mod_id) {
                return false;
            }
            let had = !required_by.is_empty();
            required_by.retain(|id| !disabled_ids.contains(id));
            // Kept unless every mod that needed it is being turned off.
            !had || !required_by.is_empty()
        }
        _ => true,
    });
    if ctx.has_pack {
        // The pack sync brings the pack's files in line first. Broken and replaced pack files
        // stay listed: the sync puts exactly those back (other changes the player made stay).
        steps.insert(
            0,
            RepairStep {
                action: RepairAction::VerifyPackFiles,
                name: String::new(),
                because: Vec::new(),
                from_crash: false,
            },
        );
    }
    steps.sort_by_key(|s| action_rank(&s.action));
    for step in &mut steps {
        step.name = step_name(&step.action, &names);
    }
    let needs_network = steps.iter().any(|s| {
        !matches!(
            s.action,
            RepairAction::EnableFile { .. } | RepairAction::DisableFile { .. }
        )
    });
    RepairPlan {
        minecraft: ctx.env.minecraft.clone(),
        loader: ctx.env.loader,
        steps,
        unresolved,
        needs_network,
    }
}

/// The pack path of a file installed by the official pack.
fn pack_path(origins: &FileOrigins, file: &str) -> Option<String> {
    match origins.get(strip_disabled(file))? {
        FileOrigin::Pack { path } => Some(path.clone()),
        FileOrigin::Modrinth { .. } => None,
    }
}

/// A broken file with a known source is downloaded again rather than turned off.
pub(crate) fn with_origin(fix: ModFix, origins: &FileOrigins) -> ModFix {
    let ModFix::DisableFile {
        file,
        reason: DisableReason::Corrupt,
    } = &fix
    else {
        return fix;
    };
    match origins.get(strip_disabled(file)) {
        Some(FileOrigin::Pack { path }) => ModFix::RedownloadPackFile { path: path.clone() },
        Some(FileOrigin::Modrinth {
            project_id,
            version_id,
        }) => ModFix::RedownloadModrinthFile {
            file: file.clone(),
            project_id: project_id.clone(),
            version_id: version_id.clone(),
        },
        None => fix,
    }
}

fn lookup(
    ctx: &RepairContext<'_>,
    file: Option<&str>,
    mod_id: &str,
    version_req: Option<String>,
) -> ModrinthLookup {
    let project_id = file
        .and_then(|f| ctx.origins.get(strip_disabled(f)))
        .and_then(|origin| match origin {
            FileOrigin::Modrinth { project_id, .. } => Some(project_id.clone()),
            FileOrigin::Pack { .. } => None,
        });
    ModrinthLookup {
        project_id,
        slugs: modrinth_slug_candidates(mod_id),
        loader: ctx.env.loader,
        minecraft: ctx.env.minecraft.clone(),
        version_reqs: version_req.into_iter().collect(),
    }
}

/// The step a fix becomes for this set. A pack file that needs another version goes back to
/// the pack's version: pack files are never upgraded here and get no Modrinth lookup.
pub(crate) fn action_for(fix: ModFix, ctx: &RepairContext<'_>) -> RepairAction {
    match fix {
        ModFix::FetchDependency {
            mod_id,
            version_req,
            required_by,
            ..
        } => RepairAction::FetchDependency {
            lookup: lookup(ctx, None, &mod_id, version_req),
            mod_id,
            required_by,
        },
        ModFix::EnableFile { file, mod_id } => RepairAction::EnableFile { file, mod_id },
        ModFix::UpdateToCompatible {
            file,
            mod_id,
            version_req,
            disable_if_unavailable,
        } => match pack_path(ctx.origins, &file) {
            Some(path) => RepairAction::RevertPackFile { path, mod_id },
            None => RepairAction::UpdateToCompatible {
                lookup: lookup(ctx, Some(&file), &mod_id, version_req),
                file,
                mod_id,
                disable_if_unavailable,
            },
        },
        ModFix::DisableFile { file, reason } => RepairAction::DisableFile { file, reason },
        ModFix::RedownloadPackFile { path } => RepairAction::RedownloadPackFile { path },
        ModFix::RedownloadModrinthFile {
            file,
            project_id,
            version_id,
        } => RepairAction::RedownloadModrinthFile {
            file,
            project_id,
            version_id,
        },
        ModFix::UpdateLoader {
            loader,
            version_req,
        } => RepairAction::UpdateLoader {
            loader,
            version_reqs: vec![version_req],
        },
        ModFix::UseJava { major } => RepairAction::UseJava { major },
    }
}

/// Add `action` to the plan, or fold it into the step that already acts on the same file, mod
/// or loader (the stronger disable reason wins, requirements add up, the higher Java wins).
fn merge(steps: &mut Vec<RepairStep>, action: RepairAction, kind: Option<IssueKind>) {
    let found = steps.iter_mut().find(|s| same_target(&s.action, &action));
    let Some(step) = found else {
        steps.push(RepairStep {
            action,
            name: String::new(),
            because: kind.into_iter().collect(),
            from_crash: kind.is_none(),
        });
        return;
    };
    if let Some(kind) = kind
        && !step.because.contains(&kind)
    {
        step.because.push(kind);
    }
    step.from_crash |= kind.is_none();
    match (&mut step.action, action) {
        (
            RepairAction::DisableFile { reason, .. },
            RepairAction::DisableFile { reason: new, .. },
        ) => {
            if reason_rank(new) < reason_rank(*reason) {
                *reason = new;
            }
        }
        (
            RepairAction::UpdateToCompatible {
                disable_if_unavailable,
                lookup,
                ..
            },
            RepairAction::UpdateToCompatible {
                disable_if_unavailable: or_disable,
                lookup: more,
                ..
            },
        ) => {
            *disable_if_unavailable |= or_disable;
            for req in more.version_reqs {
                push_unique(&mut lookup.version_reqs, Some(req));
            }
        }
        (
            RepairAction::FetchDependency {
                mod_id,
                required_by,
                lookup,
            },
            RepairAction::FetchDependency {
                mod_id: other_id,
                required_by: needers,
                lookup: more,
            },
        ) => {
            for id in needers {
                push_unique(required_by, Some(id));
            }
            if *mod_id == other_id {
                for req in more.version_reqs {
                    push_unique(&mut lookup.version_reqs, Some(req));
                }
            } else if is_project_id(&other_id) && !is_project_id(mod_id) {
                // One project asked for by its own id and by one of its parts (Fabric API and
                // `fabric-events-interaction-v0`): one download of the project, by its own id.
                // A part's version requirements are about the part, so they do not carry over.
                *mod_id = other_id;
                *lookup = more;
            }
        }
        (
            RepairAction::UpdateLoader { version_reqs, .. },
            RepairAction::UpdateLoader {
                version_reqs: more, ..
            },
        ) => {
            for req in more {
                push_unique(version_reqs, Some(req));
            }
        }
        (RepairAction::UseJava { major }, RepairAction::UseJava { major: other }) => {
            *major = (*major).max(other);
        }
        _ => {}
    }
}

/// Two steps that act on the same file, mod or loader.
fn same_target(a: &RepairAction, b: &RepairAction) -> bool {
    use RepairAction as A;
    match (a, b) {
        (A::DisableFile { file: x, .. }, A::DisableFile { file: y, .. })
        | (A::EnableFile { file: x, .. }, A::EnableFile { file: y, .. })
        | (A::UpdateToCompatible { file: x, .. }, A::UpdateToCompatible { file: y, .. })
        | (A::RedownloadModrinthFile { file: x, .. }, A::RedownloadModrinthFile { file: y, .. })
        | (A::RedownloadPackFile { path: x }, A::RedownloadPackFile { path: y })
        | (A::RevertPackFile { path: x, .. }, A::RevertPackFile { path: y, .. }) => x == y,
        (A::FetchDependency { mod_id: x, .. }, A::FetchDependency { mod_id: y, .. }) => {
            x == y || dependency_project(x) == dependency_project(y)
        }
        (A::UpdateLoader { loader: x, .. }, A::UpdateLoader { loader: y, .. }) => x == y,
        (A::UseJava { .. }, A::UseJava { .. }) | (A::VerifyPackFiles, A::VerifyPackFiles) => true,
        _ => false,
    }
}

fn step_name(action: &RepairAction, names: &HashMap<String, String>) -> String {
    let for_file = |file: &str| {
        names
            .get(file)
            .cloned()
            .unwrap_or_else(|| display_name_for_file(file))
    };
    match action {
        RepairAction::VerifyPackFiles => String::new(),
        RepairAction::RedownloadPackFile { path } | RepairAction::RevertPackFile { path, .. } => {
            for_file(pack_file_name(path))
        }
        RepairAction::RedownloadModrinthFile { file, .. }
        | RepairAction::EnableFile { file, .. }
        | RepairAction::DisableFile { file, .. }
        | RepairAction::UpdateToCompatible { file, .. } => for_file(file),
        RepairAction::FetchDependency { mod_id, .. } => display_name_for_id(mod_id),
        RepairAction::UpdateLoader { loader, .. } => loader_display_name(*loader).to_owned(),
        RepairAction::UseJava { major } => format!("Java {major}"),
    }
}

/// The name players know a mod loader by.
pub fn loader_display_name(loader: LoaderKind) -> &'static str {
    match loader {
        LoaderKind::Vanilla => "Minecraft",
        LoaderKind::Fabric => "Fabric",
        LoaderKind::Quilt => "Quilt",
        LoaderKind::Forge => "Forge",
        LoaderKind::NeoForge => "NeoForge",
    }
}

/// Mod ids whose display name a heuristic would get wrong, sorted by id.
const NAMES: [(&str, &str); 20] = [
    ("architectury", "Architectury API"),
    ("cloth-config", "Cloth Config API"),
    ("cloth-config2", "Cloth Config API"),
    ("cloth_config", "Cloth Config API"),
    ("fabric", "Fabric API"),
    ("fabric-api", "Fabric API"),
    ("fabric-language-kotlin", "Fabric Language Kotlin"),
    ("fabricloader", "Fabric Loader"),
    ("forge", "Forge"),
    ("geckolib", "GeckoLib"),
    ("iris", "Iris"),
    ("java", "Java"),
    ("minecraft", "Minecraft"),
    ("modmenu", "Mod Menu"),
    ("neoforge", "NeoForge"),
    ("owo", "oωo"),
    ("quilt_loader", "Quilt Loader"),
    ("quilted_fabric_api", "Quilted Fabric API"),
    ("rei", "Roughly Enough Items"),
    ("yet_another_config_lib_v3", "YetAnotherConfigLib"),
];

/// A readable name for a mod known only by its id (a missing dependency): a known name, else
/// the id's words capitalized (`fabric-api` → `Fabric API`, `cloth_config` → `Cloth Config`).
pub fn display_name_for_id(mod_id: &str) -> String {
    let id = mod_id.trim();
    let lower = id.to_ascii_lowercase();
    if let Ok(i) = NAMES.binary_search_by(|(known, _)| (*known).cmp(lower.as_str())) {
        return NAMES[i].1.to_owned();
    }
    if is_fabric_api_module(&lower) {
        return "Fabric API".to_owned();
    }
    const ACRONYMS: [&str; 8] = ["api", "emi", "fps", "gui", "hud", "jei", "rei", "ui"];
    let words: Vec<String> = id
        .split(['-', '_', ' ', '.'])
        .filter(|w| !w.is_empty())
        .map(|word| {
            if ACRONYMS.contains(&word.to_ascii_lowercase().as_str()) {
                return word.to_ascii_uppercase();
            }
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect(),
                None => String::new(),
            }
        })
        .collect();
    if words.is_empty() {
        id.to_owned()
    } else {
        words.join(" ")
    }
}

/// A readable name read from a jar's file name, for a jar that does not say its own (a broken
/// download, no mod description): the words before the first version-like part, without the
/// loader names after the first word, named like [`display_name_for_id`]
/// (`sodium-fabric-0.6.13+mc1.21.4.jar` → `Sodium`, `fabric-api-0.92.0+1.20.1.jar` →
/// `Fabric API`, `EntityCulling-Fabric-1.6.2-mc1.20.1.jar.disabled` → `EntityCulling`). Falls back
/// to the file name without its extension.
pub fn display_name_for_file(file_name: &str) -> String {
    const NOISE: [&str; 6] = ["fabric", "forge", "neoforge", "quilt", "mod", "universal"];
    let stem = file_stem(file_name);
    let versioned = |word: &str| {
        let lower = word.to_ascii_lowercase();
        let digit_after = |prefix: &str| {
            lower
                .strip_prefix(prefix)
                .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
        };
        word.starts_with(|c: char| c.is_ascii_digit()) || digit_after("v") || digit_after("mc")
    };
    let mut words: Vec<&str> = Vec::new();
    for word in stem.split(['-', '_', ' ', '+']).filter(|w| !w.is_empty()) {
        if versioned(word) {
            break;
        }
        if !words.is_empty() && NOISE.contains(&word.to_ascii_lowercase().as_str()) {
            continue;
        }
        words.push(word);
    }
    if words.is_empty() {
        stem.to_owned()
    } else {
        display_name_for_id(&words.join("-"))
    }
}

// --- reinstalling a version's set --------------------------------------------------------------

/// The version set to reinstall and where things go.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ReinstallTarget {
    pub minecraft: String,
    pub loader: LoaderKind,
    /// The version's own `mods/` and `config/` folders.
    pub mods_dir: PathBuf,
    pub config_dir: PathBuf,
    /// Where jars without a known source are put aside (outside `mods/`).
    pub keep_dir: PathBuf,
    /// The set includes the official pack.
    pub has_pack: bool,
}

/// A Modrinth mod to install again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthReinstall {
    pub file: String,
    pub name: String,
    pub project_id: String,
    /// The version it had; the newest version for this game version and loader is preferred.
    pub version_id: Option<String>,
    /// Put back turned off when it was off.
    pub enabled: bool,
}

/// A jar with no known source, copied aside so it is never lost.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct KeptJar {
    pub file: String,
    pub name: String,
    pub from: PathBuf,
    /// Suggested place; when taken, phase two picks a free name next to it.
    pub to: PathBuf,
    pub enabled: bool,
}

/// "Yeniden yükle": what reinstalling one version's mod set does, in order. Phase two shows
/// `resets_settings` before asking for confirmation, copies `keep_aside`, moves
/// `move_to_trash` to the launcher trash (recoverable), then runs the pack sync and the
/// Modrinth installs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ReinstallPlan {
    pub minecraft: String,
    pub loader: LoaderKind,
    /// Jars without a known source, copied to the keep folder first.
    pub keep_aside: Vec<KeptJar>,
    /// Folders moved to the launcher trash (the set's `mods/` and `config/`, when present).
    pub move_to_trash: Vec<PathBuf>,
    /// `config/` has files: mod settings go back to their defaults.
    pub resets_settings: bool,
    /// Run the pack sync afterwards; it reinstalls the player's saved selection.
    pub resync_pack: bool,
    /// Pack files present now (restored by the sync), for the summary.
    pub pack_files: Vec<String>,
    /// The player's Modrinth mods, installed again by project.
    pub reinstall_from_modrinth: Vec<ModrinthReinstall>,
}

/// Describe reinstalling the set whose `mods/` folder `snapshot` scanned. Looks at the disk
/// only to see which of the two folders exist and whether `config/` has files.
pub fn plan_reinstall(
    target: &ReinstallTarget,
    snapshot: &ModSnapshot,
    origins: &FileOrigins,
) -> ReinstallPlan {
    let mut plan = ReinstallPlan {
        minecraft: target.minecraft.clone(),
        loader: target.loader,
        keep_aside: Vec::new(),
        move_to_trash: Vec::new(),
        resets_settings: std::fs::read_dir(&target.config_dir)
            .is_ok_and(|mut entries| entries.next().is_some()),
        resync_pack: target.has_pack,
        pack_files: Vec::new(),
        reinstall_from_modrinth: Vec::new(),
    };
    for dir in [&target.mods_dir, &target.config_dir] {
        if std::fs::symlink_metadata(dir).is_ok() {
            plan.move_to_trash.push(dir.clone());
        }
    }
    for jar in &snapshot.jars {
        let base = jar.base_name();
        match origins.get(base) {
            Some(FileOrigin::Pack { .. }) => plan.pack_files.push(jar.file_name.clone()),
            Some(FileOrigin::Modrinth {
                project_id,
                version_id,
            }) => plan.reinstall_from_modrinth.push(ModrinthReinstall {
                file: base.to_owned(),
                name: jar.readable_name(),
                project_id: project_id.clone(),
                version_id: version_id.clone(),
                enabled: jar.enabled,
            }),
            None => plan.keep_aside.push(KeptJar {
                file: jar.file_name.clone(),
                name: jar.readable_name(),
                from: jar.path.clone(),
                to: target.keep_dir.join(&jar.file_name),
                enabled: jar.enabled,
            }),
        }
    }
    plan
}

// --- Modrinth slugs ----------------------------------------------------------------------------

/// Mod ids whose Modrinth slug differs from the id, sorted by id.
const SLUGS: [(&str, &str); 31] = [
    ("architectury", "architectury-api"),
    ("bookshelf", "bookshelf-lib"),
    ("cardinal-components", "cardinal-components-api"),
    ("cardinal-components-base", "cardinal-components-api"),
    ("citresewn", "cit-resewn"),
    ("cloth-config2", "cloth-config"),
    ("cloth_config", "cloth-config"),
    ("connectormod", "connector"),
    ("cristellib", "cristel-lib"),
    ("entity_model_features", "entity-model-features"),
    ("entity_texture_features", "entitytexturefeatures"),
    ("fabric", "fabric-api"),
    ("ferritecore", "ferrite-core"),
    ("forgeconfigapiport", "forge-config-api-port"),
    ("fzzy_config", "fzzy-config"),
    ("geckolib3", "geckolib"),
    ("kotlinforforge", "kotlin-for-forge"),
    ("lambdynlights", "lambdynamiclights"),
    ("owo", "owo-lib"),
    ("puzzleslib", "puzzles-lib"),
    ("quilted_fabric_api", "qsl"),
    ("resourcefullib", "resourceful-lib"),
    ("roughlyenoughitems", "rei"),
    ("supermartijn642configlib", "supermartijn642s-config-lib"),
    ("supermartijn642corelib", "supermartijn642s-core-lib"),
    ("voicechat", "simple-voice-chat"),
    ("xaerominimap", "xaeros-minimap"),
    ("xaeroworldmap", "xaeros-world-map"),
    ("yet-another-config-lib", "yacl"),
    ("yet_another_config_lib", "yacl"),
    ("yet_another_config_lib_v3", "yacl"),
];

/// Modrinth slugs to try for a mod id, best first: a known mapping (`cloth-config2` →
/// `cloth-config`, Fabric API modules → `fabric-api`, Quilt standard library modules → `qsl`),
/// else the id itself (and its `_` → `-` form).
pub fn modrinth_slug_candidates(mod_id: &str) -> Vec<String> {
    let id = mod_id.trim().to_ascii_lowercase();
    if let Ok(i) = SLUGS.binary_search_by(|(known, _)| (*known).cmp(id.as_str())) {
        return vec![SLUGS[i].1.to_owned()];
    }
    if is_fabric_api_module(&id) {
        return vec!["fabric-api".to_owned()];
    }
    if id.starts_with("quilt_") && id != "quilt_loader" {
        return vec!["qsl".to_owned()];
    }
    let dashed = id.replace('_', "-");
    if dashed == id || id.is_empty() {
        vec![id]
    } else {
        vec![id, dashed]
    }
}

/// `fabric-networking-api-v1`, `fabric-api-base`, `fabric-renderer-indigo`.
/// The project a dependency id stands for: a known id's Modrinth project, Fabric API for its
/// parts (`fabric-events-interaction-v0`), the Quilt standard libraries for theirs, else the id
/// itself. Two ids of one project are one dependency: one issue, one download.
pub fn dependency_project(mod_id: &str) -> String {
    modrinth_slug_candidates(mod_id)
        .into_iter()
        .next()
        .unwrap_or_default()
}

/// The id is its project's own (`fabric-api`), not an alias or one of its parts.
fn is_project_id(mod_id: &str) -> bool {
    dependency_project(mod_id) == mod_id.trim().to_ascii_lowercase()
}

fn is_fabric_api_module(id: &str) -> bool {
    let Some(rest) = id.strip_prefix("fabric-") else {
        return false;
    };
    let versioned = rest.rsplit_once("-v").is_some_and(|(name, n)| {
        !name.is_empty() && !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())
    });
    versioned || matches!(rest, "api-base" | "renderer-indigo")
}

#[cfg(test)]
mod tests {
    use super::super::check::{CheckEnvironment, check_mods};
    use super::super::scan::test_jars::*;
    use super::super::scan::{ModSnapshot, ScannedJar};
    use super::*;

    fn env() -> CheckEnvironment {
        CheckEnvironment {
            minecraft: "1.21.1".into(),
            loader: LoaderKind::Fabric,
            loader_version: Some("0.16.10".into()),
            java_major: Some(21),
        }
    }

    fn jar(name: &str, bytes: Vec<u8>) -> ScannedJar {
        ScannedJar::from_bytes(name, &bytes)
    }

    fn kinds(plan: &RepairPlan) -> Vec<&'static str> {
        plan.steps
            .iter()
            .map(|s| match s.action {
                RepairAction::VerifyPackFiles => "verify",
                RepairAction::RedownloadPackFile { .. } => "redownload-pack",
                RepairAction::RevertPackFile { .. } => "revert-pack",
                RepairAction::RedownloadModrinthFile { .. } => "redownload-modrinth",
                RepairAction::EnableFile { .. } => "enable",
                RepairAction::DisableFile { .. } => "disable",
                RepairAction::UpdateLoader { .. } => "loader",
                RepairAction::UseJava { .. } => "java",
                RepairAction::UpdateToCompatible { .. } => "update",
                RepairAction::FetchDependency { .. } => "fetch",
            })
            .collect()
    }

    fn scenario() -> (ModSnapshot, FileOrigins) {
        let snapshot = ModSnapshot::new(
            "mods",
            vec![
                jar("broken-pack.jar", b"<html>".to_vec()),
                jar("broken-modrinth.jar", Vec::new()),
                jar("sodium-old.jar", fabric_jar("sodium", "0.5.0", "")),
                jar("sodium-new.jar", fabric_jar("sodium", "0.6.0", "")),
                jar("create.jar", toml_jar(false, "create", "0.5.1", "")),
                jar(
                    "a.jar",
                    fabric_jar(
                        "a",
                        "1.0",
                        r#""depends": {"fabric-api": "*", "lib": "*", "fabricloader": ">=0.17", "java": ">=25"}"#,
                    ),
                ),
                jar(
                    "b.jar",
                    fabric_jar("b", "1.0", r#""depends": {"fabric-api": ">=0.100"}"#),
                ),
                jar("lib.jar.disabled", fabric_jar("lib", "1.0", "")),
                jar(
                    "oldmc.jar",
                    fabric_jar("oldmc", "1.0", r#""depends": {"minecraft": "1.20.x"}"#),
                ),
                jar(
                    "conflicting.jar",
                    fabric_jar("conflicting", "1.0", r#""conflicts": {"a": "*"}"#),
                ),
            ],
        );
        let mut origins = FileOrigins::new();
        origins.insert(
            "broken-pack.jar".into(),
            FileOrigin::Pack {
                path: "mods/broken-pack.jar".into(),
            },
        );
        origins.insert(
            "broken-modrinth.jar".into(),
            FileOrigin::Modrinth {
                project_id: "P1".into(),
                version_id: Some("V1".into()),
            },
        );
        origins.insert(
            "oldmc.jar".into(),
            FileOrigin::Modrinth {
                project_id: "OLDMC".into(),
                version_id: None,
            },
        );
        (snapshot, origins)
    }

    #[test]
    fn plan_orders_and_merges() {
        let (snapshot, origins) = scenario();
        let env = env();
        let report = check_mods(&snapshot, &env, &origins);
        let ctx = RepairContext {
            env: &env,
            origins: &origins,
            has_pack: false,
        };
        let plan = plan_repair(&ctx, Some(&report), None);
        assert_eq!(
            kinds(&plan),
            vec![
                "redownload-pack",
                "redownload-modrinth",
                "enable",
                "disable",
                "loader",
                "java",
                "update",
                "update",
                "fetch"
            ],
            "{:#?}",
            plan.steps
        );
        assert!(plan.needs_network);
        let step = |pred: &dyn Fn(&RepairAction) -> bool| {
            plan.steps.iter().find(|s| pred(&s.action)).unwrap().clone()
        };
        let fetch = step(&|a| matches!(a, RepairAction::FetchDependency { .. }));
        let RepairAction::FetchDependency {
            mod_id,
            required_by,
            lookup,
        } = &fetch.action
        else {
            unreachable!()
        };
        assert_eq!(mod_id, "fabric-api");
        assert_eq!(required_by, &vec!["a".to_owned(), "b".to_owned()]);
        assert_eq!(lookup.version_reqs, vec![">=0.100".to_owned()]);
        assert_eq!(lookup.slugs, vec!["fabric-api".to_owned()]);

        let enable = step(&|a| matches!(a, RepairAction::EnableFile { .. }));
        assert_eq!(enable.name, "lib name");
        let duplicate = step(&|a| {
            matches!(
                a,
                RepairAction::DisableFile {
                    reason: DisableReason::Duplicate,
                    ..
                }
            )
        });
        assert_eq!(
            duplicate.action,
            RepairAction::DisableFile {
                file: "sodium-old.jar".into(),
                reason: DisableReason::Duplicate
            }
        );
        let update = step(
            &|a| matches!(a, RepairAction::UpdateToCompatible { file, .. } if file == "oldmc.jar"),
        );
        let RepairAction::UpdateToCompatible {
            disable_if_unavailable,
            lookup,
            ..
        } = &update.action
        else {
            unreachable!()
        };
        assert!(*disable_if_unavailable);
        assert_eq!(lookup.project_id.as_deref(), Some("OLDMC"));
        assert_eq!(update.because, vec![IssueKind::IncompatibleMinecraft]);
        // The wrong-loader Forge jar is replaced or turned off.
        assert!(plan.steps.iter().any(|s| matches!(&s.action, RepairAction::UpdateToCompatible { file, .. } if file == "create.jar")));
        // The conflict only warns: not part of the repair, not unresolved either.
        assert!(!plan.steps.iter().any(|s| s.name.contains("conflicting")));
        assert!(plan.unresolved.is_empty());

        // With the official pack in the set, the sync runs first and puts the broken pack file
        // back.
        let ctx = RepairContext {
            has_pack: true,
            ..ctx
        };
        let plan = plan_repair(&ctx, Some(&report), None);
        assert_eq!(
            kinds(&plan)[..3],
            ["verify", "redownload-pack", "redownload-modrinth"]
        );
    }

    #[test]
    fn pack_files_go_back_to_the_pack_instead_of_updating() {
        let env = env();
        let snapshot = ModSnapshot::new(
            "mods",
            vec![
                jar(
                    "pack-old.jar",
                    fabric_jar("packold", "1.0", r#""depends": {"minecraft": "1.20.x"}"#),
                ),
                jar(
                    "own-old.jar",
                    fabric_jar("ownold", "1.0", r#""depends": {"minecraft": "1.20.x"}"#),
                ),
                jar(
                    "needs-new-lib.jar",
                    fabric_jar("needy", "1.0", r#""depends": {"packlib": ">=2.0"}"#),
                ),
                jar("pack-lib.jar", fabric_jar("packlib", "1.5", "")),
            ],
        );
        let mut origins = FileOrigins::new();
        for (file, path) in [
            ("pack-old.jar", "mods/pack-old.jar"),
            ("pack-lib.jar", "mods/pack-lib.jar"),
        ] {
            origins.insert(file.into(), FileOrigin::Pack { path: path.into() });
        }
        let report = check_mods(&snapshot, &env, &origins);
        let ctx = RepairContext {
            env: &env,
            origins: &origins,
            has_pack: true,
        };
        let plan = plan_repair(&ctx, Some(&report), None);
        assert_eq!(
            kinds(&plan),
            vec!["verify", "revert-pack", "revert-pack", "update"],
            "{:#?}",
            plan.steps
        );
        // The pack mod made for another Minecraft version and the pack library that is too old
        // for an own mod go back to the pack's versions: no Modrinth lookup, never turned off.
        let reverted: Vec<&str> = plan
            .steps
            .iter()
            .filter_map(|s| match &s.action {
                RepairAction::RevertPackFile { path, .. } => Some(path.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(reverted, ["mods/pack-lib.jar", "mods/pack-old.jar"]);
        // The player's own mod is updated (or turned off when nothing fits).
        let update = plan
            .steps
            .iter()
            .find_map(|s| match &s.action {
                RepairAction::UpdateToCompatible {
                    file,
                    disable_if_unavailable,
                    ..
                } => Some((file.as_str(), *disable_if_unavailable)),
                _ => None,
            })
            .unwrap();
        assert_eq!(update, ("own-old.jar", true));
        assert!(plan.needs_network);

        // A crash that blames a pack mod's file reverts it as well.
        let crash = CrashDiagnosis {
            mod_fixes: vec![ModFix::UpdateToCompatible {
                file: "pack-old.jar".into(),
                mod_id: "packold".into(),
                version_req: None,
                disable_if_unavailable: false,
            }],
            ..test_diagnosis()
        };
        let plan = plan_repair(&ctx, None, Some(&crash));
        assert_eq!(kinds(&plan), vec!["verify", "revert-pack"]);
        assert!(plan.steps[1].from_crash);
    }

    #[test]
    fn every_older_duplicate_is_turned_off() {
        let env = env();
        let origins = FileOrigins::new();
        let mut a = jar("sodium-a.jar", fabric_jar("sodium", "0.5.0", ""));
        let mut b = jar("sodium-b.jar", fabric_jar("sodium", "0.5.5", ""));
        let c = jar("sodium-c.jar", fabric_jar("sodium", "0.6.0", ""));
        a.modified_ms = Some(1);
        b.modified_ms = Some(2);
        let snapshot = ModSnapshot::new("mods", vec![a, b, c]);
        let report = check_mods(&snapshot, &env, &origins);
        let ctx = RepairContext {
            env: &env,
            origins: &origins,
            has_pack: false,
        };
        let plan = plan_repair(&ctx, Some(&report), None);
        let disabled: Vec<&str> = plan
            .steps
            .iter()
            .filter_map(|s| match &s.action {
                RepairAction::DisableFile { file, .. } => Some(file.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(disabled, ["sodium-a.jar", "sodium-b.jar"]);
    }

    #[test]
    fn display_names_for_ids() {
        assert!(
            NAMES.windows(2).all(|w| w[0].0 < w[1].0),
            "table must stay sorted"
        );
        assert_eq!(display_name_for_id("fabric-api"), "Fabric API");
        assert_eq!(
            display_name_for_id("fabric-networking-api-v1"),
            "Fabric API"
        );
        assert_eq!(display_name_for_id("cloth-config2"), "Cloth Config API");
        assert_eq!(display_name_for_id("sodium"), "Sodium");
        assert_eq!(
            display_name_for_id("entity_model_features"),
            "Entity Model Features"
        );
        assert_eq!(display_name_for_id("better-hud"), "Better HUD");
        assert_eq!(display_name_for_id(""), "");
        assert_eq!(loader_display_name(LoaderKind::NeoForge), "NeoForge");
    }

    #[test]
    fn one_download_per_dependency_project() {
        // Fabric API went missing: one mod asks for one of its parts, two ask for it by its own
        // id (one with a requirement). One download, by the project's own id and requirement.
        let snapshot = ModSnapshot::new(
            "mods",
            vec![
                jar(
                    "a.jar",
                    fabric_jar(
                        "a",
                        "1.0",
                        r#""depends": {"fabric-events-interaction-v0": "*"}"#,
                    ),
                ),
                jar(
                    "b.jar",
                    fabric_jar("b", "1.0", r#""depends": {"fabric-api": ">=0.92.0"}"#),
                ),
                jar(
                    "c.jar",
                    fabric_jar("c", "1.0", r#""depends": {"fabric-api": "*"}"#),
                ),
            ],
        );
        let env = env();
        let origins = FileOrigins::new();
        let report = check_mods(&snapshot, &env, &origins);
        let ctx = RepairContext {
            env: &env,
            origins: &origins,
            has_pack: false,
        };
        let plan = plan_repair(&ctx, Some(&report), None);
        let fetches: Vec<(&RepairStep, &str, &Vec<String>, &Vec<String>)> = plan
            .steps
            .iter()
            .filter_map(|s| match &s.action {
                RepairAction::FetchDependency {
                    mod_id,
                    required_by,
                    lookup,
                } => Some((s, mod_id.as_str(), required_by, &lookup.version_reqs)),
                _ => None,
            })
            .collect();
        assert_eq!(fetches.len(), 1, "{fetches:?}");
        let (step, mod_id, required_by, reqs) = fetches[0];
        assert_eq!(mod_id, "fabric-api");
        assert_eq!(required_by, &["a", "b", "c"]);
        assert_eq!(reqs, &[">=0.92.0"]);
        assert_eq!(step.name, "Fabric API");
        assert_eq!(dependency_project("fabric"), "fabric-api");
        assert_eq!(dependency_project("Sodium"), "sodium");
    }

    #[test]
    fn display_names_for_files() {
        assert_eq!(
            display_name_for_file("sodium-fabric-0.6.13+mc1.21.4.jar"),
            "Sodium"
        );
        assert_eq!(
            display_name_for_file("fabric-api-0.92.0+1.20.1.jar"),
            "Fabric API"
        );
        assert_eq!(
            display_name_for_file("EntityCulling-Fabric-1.6.2-mc1.20.1.jar.disabled"),
            "EntityCulling"
        );
        assert_eq!(display_name_for_file("iris-mc1.21.4-1.8.2.jar"), "Iris");
        assert_eq!(display_name_for_file("modmenu-13.0.3.jar"), "Mod Menu");
        assert_eq!(
            display_name_for_file("cloth-config-17.0.144-fabric.jar"),
            "Cloth Config API"
        );
        assert_eq!(display_name_for_file("better_hud-v2.1.jar"), "Better HUD");
        // Nothing readable before the version: the file name stays.
        assert_eq!(display_name_for_file("1.21-pack.jar"), "1.21-pack");
    }

    #[test]
    fn pointless_steps_are_dropped() {
        let env = env();
        let origins = FileOrigins::new();
        let snapshot = ModSnapshot::new(
            "mods",
            vec![jar(
                "needy.jar",
                fabric_jar("needy", "1.0", r#""depends": {"gone": "*"}"#),
            )],
        );
        let report = check_mods(&snapshot, &env, &origins);
        let crash = CrashDiagnosis {
            mod_fixes: vec![ModFix::DisableFile {
                file: "needy.jar".into(),
                reason: DisableReason::Crash,
            }],
            suspects: vec![crate::launch::crash::CrashSuspect {
                mod_id: Some("needy".into()),
                name: Some("Needy".into()),
                file: Some("needy.jar".into()),
                version: None,
                reason: crate::launch::crash::SuspectReason::LoadingFailed,
                related: None,
                requirement: None,
                mixin_config: None,
                class_name: None,
            }],
            ..test_diagnosis()
        };
        let ctx = RepairContext {
            env: &env,
            origins: &origins,
            has_pack: false,
        };
        let plan = plan_repair(&ctx, Some(&report), Some(&crash));
        assert_eq!(kinds(&plan), vec!["disable"], "{:#?}", plan.steps);
        assert!(plan.steps[0].from_crash);
        assert_eq!(plan.steps[0].name, "needy name");
        assert!(!plan.needs_network);

        // Fixes for one file merge, the stronger reason wins and requirements add up.
        let mut steps = Vec::new();
        for fix in [
            ModFix::DisableFile {
                file: "x.jar".into(),
                reason: DisableReason::Crash,
            },
            ModFix::DisableFile {
                file: "x.jar".into(),
                reason: DisableReason::Duplicate,
            },
            ModFix::UpdateToCompatible {
                file: "y.jar".into(),
                mod_id: "y".into(),
                version_req: Some(">=1".into()),
                disable_if_unavailable: false,
            },
            ModFix::UpdateToCompatible {
                file: "y.jar".into(),
                mod_id: "y".into(),
                version_req: Some("<3".into()),
                disable_if_unavailable: true,
            },
            ModFix::UseJava { major: 17 },
            ModFix::UseJava { major: 21 },
        ] {
            merge(
                &mut steps,
                action_for(fix, &ctx),
                Some(IssueKind::Duplicate),
            );
        }
        assert_eq!(steps.len(), 3);
        assert_eq!(
            steps[0].action,
            RepairAction::DisableFile {
                file: "x.jar".into(),
                reason: DisableReason::Duplicate
            }
        );
        let RepairAction::UpdateToCompatible {
            disable_if_unavailable,
            lookup,
            ..
        } = &steps[1].action
        else {
            unreachable!()
        };
        assert!(*disable_if_unavailable);
        assert_eq!(lookup.version_reqs, vec![">=1".to_owned(), "<3".to_owned()]);
        assert_eq!(steps[2].action, RepairAction::UseJava { major: 21 });
    }

    #[test]
    fn unreadable_files_are_unresolved() {
        let env = env();
        let origins = FileOrigins::new();
        let report = ModCheckReport {
            issues: vec![ModIssue {
                severity: Severity::Warning,
                kind: IssueKind::Unreadable,
                mods: Vec::new(),
                target: None,
                fixes: Vec::new(),
            }],
            checked_files: 1,
        };
        let ctx = RepairContext {
            env: &env,
            origins: &origins,
            has_pack: false,
        };
        let plan = plan_repair(&ctx, Some(&report), None);
        assert!(plan.steps.is_empty());
        assert_eq!(plan.unresolved.len(), 1);
    }

    #[test]
    fn reinstall_keeps_unknown_jars() {
        let dir = tempfile::tempdir().unwrap();
        let mods = dir.path().join("mods");
        let config = dir.path().join("config");
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::create_dir_all(&config).unwrap();
        std::fs::write(config.join("sodium-options.json"), b"{}").unwrap();
        std::fs::write(mods.join("pack.jar"), fabric_jar("packmod", "1.0", "")).unwrap();
        std::fs::write(mods.join("mr.jar.disabled"), fabric_jar("mrmod", "1.0", "")).unwrap();
        std::fs::write(
            mods.join("manual.jar"),
            fabric_jar("manual", "1.0", r#""name": "Manual Mod""#),
        )
        .unwrap();
        let snapshot = super::super::scan::ModScanner::new().scan_dir(&mods);
        let mut origins = FileOrigins::new();
        origins.insert(
            "pack.jar".into(),
            FileOrigin::Pack {
                path: "mods/pack.jar".into(),
            },
        );
        origins.insert(
            "mr.jar".into(),
            FileOrigin::Modrinth {
                project_id: "MR".into(),
                version_id: Some("V".into()),
            },
        );
        let target = ReinstallTarget {
            minecraft: "1.21.1".into(),
            loader: LoaderKind::Fabric,
            mods_dir: mods.clone(),
            config_dir: config.clone(),
            keep_dir: dir.path().join("kept"),
            has_pack: true,
        };
        let plan = plan_reinstall(&target, &snapshot, &origins);
        assert_eq!(plan.move_to_trash, vec![mods.clone(), config]);
        assert!(plan.resets_settings);
        assert!(plan.resync_pack);
        assert_eq!(plan.pack_files, vec!["pack.jar"]);
        assert_eq!(plan.reinstall_from_modrinth.len(), 1);
        assert_eq!(plan.reinstall_from_modrinth[0].file, "mr.jar");
        assert!(!plan.reinstall_from_modrinth[0].enabled);
        assert_eq!(plan.keep_aside.len(), 1);
        assert_eq!(plan.keep_aside[0].name, "Manual Mod");
        assert_eq!(
            plan.keep_aside[0].to,
            dir.path().join("kept").join("manual.jar")
        );

        // No config folder: nothing to reset.
        let target = ReinstallTarget {
            config_dir: dir.path().join("missing"),
            ..target
        };
        let plan = plan_reinstall(&target, &snapshot, &origins);
        assert!(!plan.resets_settings);
        assert_eq!(plan.move_to_trash, vec![mods]);
    }

    #[test]
    fn slugs() {
        assert!(
            SLUGS.windows(2).all(|w| w[0].0 < w[1].0),
            "table must stay sorted"
        );
        assert_eq!(modrinth_slug_candidates("cloth-config2"), ["cloth-config"]);
        assert_eq!(
            modrinth_slug_candidates("Architectury"),
            ["architectury-api"]
        );
        assert_eq!(
            modrinth_slug_candidates("fabric-networking-api-v1"),
            ["fabric-api"]
        );
        assert_eq!(modrinth_slug_candidates("fabric-api-base"), ["fabric-api"]);
        assert_eq!(
            modrinth_slug_candidates("fabric-language-kotlin"),
            ["fabric-language-kotlin"]
        );
        assert_eq!(modrinth_slug_candidates("quilt_base"), ["qsl"]);
        assert_eq!(modrinth_slug_candidates("sodium"), ["sodium"]);
        assert_eq!(
            modrinth_slug_candidates("some_mod"),
            ["some_mod", "some-mod"]
        );
    }

    fn test_diagnosis() -> CrashDiagnosis {
        crate::launch::crash::analyze_crash(&crate::launch::crash::CrashInput::default())
    }
}

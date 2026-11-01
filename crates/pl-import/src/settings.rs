//! "Oyun ayarları" (IMPORT-SET): the player's own settings from another launcher into the Projects
//! instance, as much of them as can safely come, with one "Geri al" that puts back exactly what
//! was there.
//!
//! From the game folder the settings come from (another launcher's `.minecraft`, or one of its
//! instances' own folders):
//!
//! - `options.txt`, merged into the instance's own: the player's values win
//!   ([`crate::options::merge_into`]). A graphics preset picked before the first launch goes with
//!   it: the first launch would apply it over their video settings;
//! - `hotbar.nbt`, the creative mode's saved hotbars (the game upgrades an older file; one it
//!   cannot read leaves those hotbars empty, nothing else);
//! - the shader choice, from Iris's `config/iris.properties` or OptiFine's `optionsshaders.txt`:
//!   which pack and whether it is on, only when that pack is in the instance's `shaderpacks/`
//!   (brought by the same run, an earlier one, or already there);
//! - settings files of the pack's mods that hold the player's own choices and that every version
//!   of the mod reads leniently ([`MOD_SETTINGS`]): Simple Voice Chat's microphone, speaker,
//!   volumes and push-to-talk mode.
//!
//! Not brought: OptiFine's `optionsof.txt` (no instance here runs OptiFine; what the game itself
//! reads of the video settings is in `options.txt`, and OptiFine's zoom key moves with it), Sodium's
//! `sodium-options.json` (its fields change type between versions, and a file Sodium cannot read
//! leaves its settings read-only behind a red warning in the game; Sodium's defaults are good), and
//! any file the launcher writes for fairness or for the server (`pl_mc::instance::mod_configs`:
//! Litematica's easy placement, Chest Tracker's container names). A mod settings file the pack
//! installs itself (`.projects/pack-state.json`) stays the pack's.
//!
//! Every file is copied before it changes into `<instance>/.launcher/import-backups/<time>/`, out
//! of the game's and the packs' reach, and listed in the instance's journal ([`SettingsRecord`])
//! before it is written: "Geri al" ([`undo_settings`]) puts each back byte for byte and deletes
//! what the import made. The newest [`KEPT_BACKUPS`] folders stay.

use std::collections::{BTreeMap, HashSet};
use std::path::{Component, Path, PathBuf};

use pl_mc::instance::{
    IRIS_PROPERTIES, KeyBinding, mod_configs, picked_preset_file, read_shader_choice,
    with_shader_choice,
};
use pl_mc::mods::scan::ModScanner;

use crate::error::{Error, Result};
use crate::journal::{Journal, SettingsRecord, path_key};
use crate::model::{ImportSource, SettingsOrigin};
use crate::options::{merge_into, same_settings};

/// The folder of the copies, in the instance's launcher folder.
pub(crate) const BACKUPS_DIR: &str = "import-backups";
/// Settings imports whose copies stay per instance.
pub(crate) const KEPT_BACKUPS: usize = 3;
const MAX_OPTIONS: u64 = 2 * 1024 * 1024;
const MAX_FILE: u64 = 1024 * 1024;
/// OptiFine's shader choice (`shaderPack=<pack>`; `OFF` and `(internal)` choose none).
const OPTIFINE_SHADERS: &str = "optionsshaders.txt";
/// The pack's record of the files it installed.
const PACK_STATE: &str = ".projects/pack-state.json";

/// Settings files of the official pack's mods the player's copy may replace: (the mod's id, the
/// file under the game folder). Each is the player's own choices and read leniently by every
/// version of the mod (a value it cannot read falls back to its default).
pub(crate) const MOD_SETTINGS: [(&str, &str); 1] =
    [("voicechat", "config/voicechat/voicechat-client.properties")];

/// Where settings go: an instance, and what may be written there.
#[derive(Debug, Clone)]
pub(crate) struct SettingsTarget {
    pub instance_id: String,
    pub instance_dir: PathBuf,
    pub game_dir: PathBuf,
    /// The official pack's key table (empty when the instance does not follow the pack).
    pub pack: Vec<KeyBinding>,
    /// It follows the official pack: its mods' settings may come before the pack is in place
    /// (the first run makes the instance; the pack arrives at its first start).
    pub official_pack: bool,
}

/// A shader choice of the player's whose pack is not in the instance yet: the run's shader packs
/// may bring it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShaderChoice {
    /// The pack in the folder the settings came from.
    pub pack: PathBuf,
    pub on: bool,
}

/// What [`bring_settings`] did.
#[derive(Debug, Default)]
pub(crate) struct Brought {
    /// The folder had any of the settings.
    pub found: bool,
    /// Files changed.
    pub changed: u32,
    /// A shader choice waiting for its pack.
    pub shader: Option<ShaderChoice>,
}

/// One settings import's changes to an instance, recorded as they happen.
#[derive(Debug)]
pub(crate) struct SettingsTx {
    instance_id: String,
    instance_dir: PathBuf,
    record: SettingsRecord,
    /// The record is in the journal (a file changed).
    saved: bool,
}

impl SettingsTx {
    /// A new settings import from the game folder `from`; nothing is written until a file
    /// changes.
    pub(crate) fn begin(target: &SettingsTarget, from: &Path) -> Self {
        Self {
            instance_id: target.instance_id.clone(),
            instance_dir: target.instance_dir.clone(),
            record: SettingsRecord {
                backup: new_backup_name(&backups_root(&target.instance_dir)),
                from: path_key(from),
                files: BTreeMap::new(),
            },
            saved: false,
        }
    }

    /// What "Geri al" restores, once a file changed.
    pub(crate) fn change(&self) -> Option<crate::model::SettingsChange> {
        self.saved.then(|| crate::model::SettingsChange {
            instance_id: self.instance_id.clone(),
            backup: self.record.backup.clone(),
        })
    }

    fn relative(&self, path: &Path) -> Result<String> {
        let rel = path
            .strip_prefix(&self.instance_dir)
            .map_err(|_| Error::Invalid(format!("{} is outside the instance", path.display())))?;
        Ok(rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/"))
    }

    /// Puts `bytes` at `path` (inside the instance's folder), keeping what was there first. False
    /// when the file holds exactly that already.
    pub(crate) fn write(&mut self, path: &Path, bytes: &[u8]) -> Result<bool> {
        let rel = self.relative(path)?;
        let real = through_link(path);
        let current = read_optional(&real, u64::MAX)?;
        if current.as_deref() == Some(bytes) {
            return Ok(false);
        }
        self.keep(&rel, current.as_deref())?;
        pl_mc::paths::write_durable_blocking(&real, bytes)?;
        Ok(true)
    }

    /// Removes the file at `path`, keeping it first. False when there is none.
    pub(crate) fn remove(&mut self, path: &Path) -> Result<bool> {
        let rel = self.relative(path)?;
        let Some(current) = read_optional(path, u64::MAX)? else {
            return Ok(false);
        };
        self.keep(&rel, Some(&current))?;
        std::fs::remove_file(path).map_err(|e| Error::io(path, e))?;
        Ok(true)
    }

    /// Copies what `rel` holds now (nothing: the import makes it) and records the file, before
    /// it changes: a run that stops in between leaves a record "Geri al" still reads right.
    fn keep(&mut self, rel: &str, current: Option<&[u8]>) -> Result<()> {
        if self.record.files.contains_key(rel) {
            return Ok(());
        }
        let root = backups_root(&self.instance_dir);
        if let Some(bytes) = current {
            let copy = root.join(&self.record.backup).join(rel);
            pl_mc::paths::write_durable_blocking(&copy, bytes)?;
        }
        self.record.files.insert(rel.to_owned(), current.is_some());
        let mut journal = Journal::read(&self.instance_dir);
        journal.settings = Some(self.record.clone());
        journal.write(&self.instance_dir)?;
        if !self.saved {
            self.saved = true;
            prune_backups(&root, &self.record.backup);
        }
        Ok(())
    }
}

/// `<instance>/.launcher/import-backups/`.
fn backups_root(instance_dir: &Path) -> PathBuf {
    instance_dir.join(".launcher").join(BACKUPS_DIR)
}

/// The time stamp and counter of a backup folder's name (`20261101-120500`, `…-2`): `None` for any
/// other name, so a name from elsewhere never reaches a path.
fn backup_order(name: &str) -> Option<(&str, u32)> {
    let digits =
        |part: &str, len: usize| part.len() == len && part.bytes().all(|b| b.is_ascii_digit());
    let mut parts = name.splitn(3, '-');
    let (day, time) = (parts.next()?, parts.next()?);
    if !(digits(day, 8) && digits(time, 6)) {
        return None;
    }
    let count = match parts.next() {
        None => 1,
        Some(n) if (1..=3).contains(&n.len()) && n.bytes().all(|b| b.is_ascii_digit()) => {
            n.parse().ok()?
        }
        Some(_) => return None,
    };
    Some((&name[..15], count))
}

/// The backup folders under `root`, oldest first.
fn backup_names(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(root)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|name| backup_order(name).is_some())
                .collect()
        })
        .unwrap_or_default();
    names.sort_by(|a, b| backup_order(a).cmp(&backup_order(b)));
    names
}

/// A new folder name under `root`: the time (UTC, so names sort by time) and a counter above any
/// folder of the same second.
fn new_backup_name(root: &Path) -> String {
    let stamp = jiff::Timestamp::now().strftime("%Y%m%d-%H%M%S").to_string();
    let last = backup_names(root)
        .iter()
        .filter_map(|name| backup_order(name).filter(|(at, _)| *at == stamp))
        .map(|(_, count)| count)
        .max();
    match last {
        None => stamp,
        Some(count) => format!("{stamp}-{}", count + 1),
    }
}

/// Deletes the oldest folders under `root` until [`KEPT_BACKUPS`] are left, never `current`.
fn prune_backups(root: &Path, current: &str) {
    let names = backup_names(root);
    let mut extra = names.len().saturating_sub(KEPT_BACKUPS);
    for name in names.iter().filter(|name| *name != current) {
        if extra == 0 {
            break;
        }
        extra -= 1;
        if let Err(error) = std::fs::remove_dir_all(root.join(name)) {
            tracing::warn!(%error, backup = %name, "could not remove an old settings backup");
        }
    }
}

/// A file's bytes, `None` when it does not exist; larger than `max` is refused.
fn read_optional(path: &Path, max: u64) -> Result<Option<Vec<u8>>> {
    match std::fs::metadata(path) {
        Ok(meta) if !meta.is_file() => return Ok(None),
        Ok(meta) if meta.len() > max => {
            return Err(Error::Invalid(format!("{} is too large", path.display())));
        }
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(Error::io(path, e)),
    }
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::io(path, e)),
    }
}

/// The file a link at `path` points to (a settings file shared between instances stays shared:
/// the file it points to changes); `path` itself otherwise.
fn through_link(path: &Path) -> PathBuf {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
        }
        _ => path.to_path_buf(),
    }
}

/// The files the pack installed in `game_dir` (its record), lower-case where the disk ignores case.
fn pack_files(game_dir: &Path) -> HashSet<String> {
    let Ok(Some(bytes)) = read_optional(&game_dir.join(PACK_STATE), 16 * 1024 * 1024) else {
        return HashSet::new();
    };
    let Ok(state) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return HashSet::new();
    };
    state
        .get("files")
        .and_then(serde_json::Value::as_object)
        .map(|files| files.keys().map(|path| fold(path)).collect())
        .unwrap_or_default()
}

fn fold(path: &str) -> String {
    let path = path.replace('\\', "/");
    if cfg!(windows) {
        path.to_lowercase()
    } else {
        path
    }
}

/// Whether the mod `id` is in the instance: an enabled jar of `mods/` says so; an official pack
/// instance whose mods are not in place yet has its pack's (Simple Voice Chat, Iris).
fn has_mod(game_dir: &Path, official_pack: bool, id: &str) -> bool {
    let snapshot = ModScanner::shared().scan_dir(&game_dir.join("mods"));
    if snapshot.jars.is_empty() {
        return official_pack;
    }
    snapshot.declaring(id).any(|(jar, _)| jar.enabled)
}

/// Brings the settings of the game folder `from` into `target` (see the module), recording every
/// change in `tx`. Blocking.
pub(crate) fn bring_settings(
    from: &Path,
    target: &SettingsTarget,
    tx: &mut SettingsTx,
) -> Result<Brought> {
    let mut brought = Brought::default();
    let mut changed = 0u32;
    let mut count = |done: bool| changed += u32::from(done);

    // options.txt: theirs merged into the instance's own.
    if let Some(theirs) = read_optional(&from.join("options.txt"), MAX_OPTIONS)? {
        brought.found = true;
        let theirs = String::from_utf8_lossy(&theirs).into_owned();
        let dest = target.game_dir.join("options.txt");
        let ours = read_optional(&through_link(&dest), MAX_OPTIONS)?
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned());
        let merged = merge_into(ours.as_deref(), &theirs, &target.pack);
        let same = ours
            .as_deref()
            .is_some_and(|ours| same_settings(ours, &merged.text));
        if !same {
            count(tx.write(&dest, merged.text.as_bytes())?);
            tracing::info!(
                added = merged.added,
                unbound = merged.unbound,
                merged = ours.is_some(),
                "game settings brought over"
            );
            // Their video settings, not a preset picked before the first launch.
            if let Some(picked) = picked_preset_file(&target.game_dir) {
                count(tx.remove(&picked)?);
            }
        }
    }

    // The creative mode's saved hotbars.
    if let Some(hotbars) = read_optional(&from.join("hotbar.nbt"), MAX_FILE)? {
        brought.found = true;
        count(tx.write(&target.game_dir.join("hotbar.nbt"), &hotbars)?);
    }

    // The pack's mods' settings files, where the pack and the launcher leave them to the player.
    let owned = pack_files(&target.game_dir);
    let launchers: HashSet<String> = mod_configs().iter().map(|c| fold(&c.path)).collect();
    for (id, path) in MOD_SETTINGS {
        let Some(bytes) = read_optional(&from.join(path), MAX_FILE)? else {
            continue;
        };
        brought.found = true;
        if owned.contains(&fold(path))
            || launchers.contains(&fold(path))
            || !has_mod(&target.game_dir, target.official_pack, id)
        {
            continue;
        }
        count(tx.write(&target.game_dir.join(path), &bytes)?);
    }

    // The shader choice, once its pack is here.
    if let Some(choice) = shader_choice(from)? {
        brought.found = true;
        match place_shader(target, tx, &choice)? {
            Some(done) => count(done),
            None => brought.shader = Some(choice),
        }
    }
    brought.changed = changed;
    Ok(brought)
}

/// The shader the folder's game drew: Iris's choice, else OptiFine's. `None` when it drew none.
fn shader_choice(from: &Path) -> Result<Option<ShaderChoice>> {
    let packs = from.join("shaderpacks");
    if let Some(iris) = read_optional(&from.join(IRIS_PROPERTIES), MAX_FILE)?
        && let (Some(pack), on) = read_shader_choice(&iris)
    {
        return Ok(plain_name(&pack).map(|name| ShaderChoice {
            pack: packs.join(name),
            on,
        }));
    }
    if let Some(optifine) = read_optional(&from.join(OPTIFINE_SHADERS), MAX_FILE)?
        && let (Some(pack), _) = read_shader_choice(&optifine)
        && pack != "OFF"
        && pack != "(internal)"
    {
        return Ok(plain_name(&pack).map(|name| ShaderChoice {
            pack: packs.join(name),
            on: true,
        }));
    }
    Ok(None)
}

/// A pack's name as a single plain file name (the value comes from the player's file).
fn plain_name(name: &str) -> Option<&str> {
    let mut parts = Path::new(name).components();
    match (parts.next(), parts.next()) {
        (Some(Component::Normal(only)), None) if only.to_str() == Some(name) => Some(name),
        _ => None,
    }
}

/// Chooses `choice`'s pack in the instance's Iris, under the name it has there: the copy an
/// import made of it, or a pack of the same name already there. `Some(changed)` once placed;
/// `None` while the pack is not in the instance. Nothing happens without Iris or when the pack
/// installs Iris's settings itself.
pub(crate) fn place_shader(
    target: &SettingsTarget,
    tx: &mut SettingsTx,
    choice: &ShaderChoice,
) -> Result<Option<bool>> {
    let packs = target.game_dir.join("shaderpacks");
    let journal = Journal::read(&target.instance_dir);
    let name = journal
        .copied_to(&choice.pack, &target.game_dir)
        .and_then(|dest| dest.strip_prefix("shaderpacks/").map(str::to_owned))
        .or_else(|| {
            let name = choice.pack.file_name()?.to_string_lossy().into_owned();
            packs.join(&name).exists().then_some(name)
        });
    let Some(name) = name else {
        return Ok(None);
    };
    if pack_files(&target.game_dir).contains(&fold(IRIS_PROPERTIES))
        || !has_mod(&target.game_dir, target.official_pack, "iris")
    {
        return Ok(Some(false));
    }
    let dest = target.game_dir.join(IRIS_PROPERTIES);
    let current = read_optional(&through_link(&dest), MAX_FILE)?;
    match with_shader_choice(current.as_deref(), &name, choice.on) {
        Some(updated) => tx.write(&dest, &updated).map(Some),
        None => Ok(Some(false)),
    }
}

/// Puts back what the settings import `backup` changed in the instance at `instance_dir`: every
/// file it replaced byte for byte, every file it made deleted. `Ok(false)` when that import is no
/// longer the instance's last one, or its copies are gone: nothing changes then. Blocking; the
/// game must not be running (it writes its settings when it closes).
pub fn undo_settings(instance_dir: &Path, backup: &str) -> Result<bool> {
    if backup_order(backup).is_none() {
        return Err(Error::Invalid(format!("not a settings backup: {backup}")));
    }
    let mut journal = Journal::read(instance_dir);
    let Some(record) = journal.settings.clone().filter(|r| r.backup == backup) else {
        return Ok(false);
    };
    let copies = backups_root(instance_dir).join(&record.backup);
    // Every copy is read first: half an undo would leave a mix of both.
    let mut restore: Vec<(PathBuf, Option<Vec<u8>>)> = Vec::new();
    for (rel, existed) in &record.files {
        let Some(rel_path) = safe_relative(rel) else {
            return Err(Error::Invalid(format!("not a settings file: {rel}")));
        };
        let saved = if *existed {
            match read_optional(&copies.join(&rel_path), u64::MAX)? {
                Some(bytes) => Some(bytes),
                None => return Ok(false),
            }
        } else {
            None
        };
        restore.push((instance_dir.join(rel_path), saved));
    }
    for (path, saved) in restore {
        let real = through_link(&path);
        match saved {
            Some(bytes) => pl_mc::paths::write_durable_blocking(&real, &bytes)?,
            None => match std::fs::remove_file(&real) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(Error::io(&real, e)),
            },
        }
    }
    journal.settings = None;
    journal.write(instance_dir)?;
    // An import that only made new files kept no copies.
    match std::fs::remove_dir_all(&copies) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => tracing::warn!(%error, backup, "could not remove a used settings backup"),
    }
    tracing::info!(backup, files = record.files.len(), "settings import undone");
    Ok(true)
}

/// `rel` as a relative path of plain parts (`/`-separated, no `..`, no root or drive).
fn safe_relative(rel: &str) -> Option<PathBuf> {
    if rel.is_empty() || rel.contains(['\\', ':']) {
        return None;
    }
    let mut out = PathBuf::new();
    for part in rel.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return None;
        }
        out.push(part);
    }
    Some(out)
}

/// Which of `sources` an earlier settings import into the instance at `instance_dir` came from,
/// while it was not undone: the screens then leave "Oyun ayarları" unticked (bringing them again
/// would put the old values over what the player changed since). Blocking (one small file).
pub fn settings_came_from(sources: &[ImportSource], instance_dir: &Path) -> Option<SettingsOrigin> {
    let from = Journal::read(instance_dir).settings?.from;
    sources.iter().find_map(|source| {
        if source.shared.as_ref().is_some_and(|s| s.options) && path_key(&source.root) == from {
            return Some(SettingsOrigin {
                source_id: source.id.clone(),
                profile_id: None,
            });
        }
        source
            .profiles
            .iter()
            .find(|p| p.has_options && path_key(&p.game_dir) == from)
            .map(|p| SettingsOrigin {
                source_id: source.id.clone(),
                profile_id: Some(p.id.clone()),
            })
    })
}

/// The settings target of the instance `id` in `store`.
pub(crate) fn target_of(
    store: &pl_mc::instance::InstanceStore,
    id: &str,
    official_pack: bool,
    minecraft: &str,
) -> Result<SettingsTarget> {
    let pack = if official_pack && !pl_mc::instance::official_pack_key_lines(minecraft).is_empty() {
        pl_mc::instance::official_pack_bindings().to_vec()
    } else {
        Vec::new()
    };
    Ok(SettingsTarget {
        instance_id: id.to_owned(),
        instance_dir: store.instance_dir(id)?,
        game_dir: store.game_dir(id)?,
        pack,
        official_pack,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_names_are_checked() {
        assert_eq!(
            backup_order("20261101-120500"),
            Some(("20261101-120500", 1))
        );
        assert_eq!(
            backup_order("20261101-120500-12"),
            Some(("20261101-120500", 12))
        );
        for bad in [
            "",
            "../x",
            "20261101-1205",
            "20261101-120500-",
            "20261101-120500-a",
            "x20261101-120500",
        ] {
            assert_eq!(backup_order(bad), None, "{bad}");
        }
        assert_eq!(
            safe_relative("minecraft/options.txt"),
            Some(PathBuf::from("minecraft").join("options.txt"))
        );
        for bad in ["", "../x", "minecraft/../x", "/x", "C:/x", "a\\b", "a//b"] {
            assert_eq!(safe_relative(bad), None, "{bad}");
        }
        assert_eq!(plain_name("BSL.zip"), Some("BSL.zip"));
        for bad in ["../BSL.zip", "a/BSL.zip", "", ".."] {
            assert_eq!(plain_name(bad), None, "{bad}");
        }
    }

    #[test]
    fn only_the_newest_copies_stay() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for name in [
            "20261101-100000",
            "20261101-110000",
            "20261101-120000",
            "20261101-120000-2",
            "other",
        ] {
            std::fs::create_dir_all(root.join(name)).unwrap();
        }
        prune_backups(root, "20261101-100000");
        let mut left: Vec<String> = std::fs::read_dir(root)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        // The current one stays even when it is the oldest; folders of other names are not ours.
        assert_eq!(
            left,
            [
                "20261101-100000",
                "20261101-120000",
                "20261101-120000-2",
                "other"
            ]
        );
    }

    #[test]
    fn the_import_table_leaves_the_launchers_own_files_alone() {
        let launchers: HashSet<String> = mod_configs().iter().map(|c| fold(&c.path)).collect();
        for (_, path) in MOD_SETTINGS {
            assert!(path.starts_with("config/"), "{path}");
            assert!(!launchers.contains(&fold(path)), "{path}");
        }
    }
}

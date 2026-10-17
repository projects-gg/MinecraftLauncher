//! Skin history of Microsoft accounts ("Önceki skinler" in the appearance panel): the skins the
//! launcher saw on a profile or put on it, so the player can go back to an old one. It is kept
//! only on this computer, one file per profile, and never sent anywhere. Mojang keeps no history
//! of past skins, and the sites that show one offer no API that other apps may use, so a history
//! starts with the first profile the launcher reads.
//!
//! Layout of the history folder (the app passes `<data>/skins`):
//! - `history/<profile uuid>.json`: the entries of one profile, written durably;
//! - `textures/<texture id>.png`: skin textures from Mojang's public texture host (downloaded
//!   without credentials); content-addressed, so profiles share them;
//! - `thumbs/<texture id>-<classic|slim>.png`: front views drawn once from them.
//!
//! Recording ([`SkinHistory::record`]): default skins (Steve, Alex and the others) and textures
//! that are not on Mojang's host are skipped; one texture is one entry (worn again, its
//! `last_worn` moves and its arm model follows); at most [`MAX_KEPT`] entries that are not
//! favourites stay, the least recently worn go first and never the skin worn now; favourites
//! are never pruned. A damaged file never breaks the panel: unreadable entries are skipped, and
//! a file that is not a history at all is moved aside (`<uuid>.json.corrupt`).

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use jiff::{SignedDuration, Timestamp};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::avatar::{front_png, texture_id};
use crate::error::{AuthError, AuthStep, Result};
use crate::model::{Skin, SkinVariant};
use crate::skins::validate_skin_png;

/// Entries that are not favourites kept per profile (six pages of ten in the panel).
pub const MAX_KEPT: usize = 60;
/// How long [`SkinHistory::restore`] can bring a removed entry back.
pub const HISTORY_UNDO_WINDOW: Duration = Duration::from_secs(5 * 60);
/// The history of a profile no longer signed in on this computer stays this long after its last
/// change ([`SkinHistory::tidy`]): signing in again within it brings the history back.
pub const FORGOTTEN_PROFILE_KEPT: Duration = Duration::from_secs(180 * DAY_SECS);

const DAY_SECS: u64 = 24 * 60 * 60;
const FILE_VERSION: u32 = 1;
/// A history this launcher writes stays far below this; anything bigger is not one.
const MAX_FILE_BYTES: u64 = 1024 * 1024;
/// Entries read from one file at most (a damaged file cannot make the panel huge).
const MAX_READ_ENTRIES: usize = 500;
const MAX_PICTURE_BYTES: u64 = 2 * 1024 * 1024;
/// The skin worn now gets its `last_worn` rewritten at most this often; every profile read
/// would otherwise write the file.
const LAST_WORN_STEP: SignedDuration = SignedDuration::from_hours(1);
/// Pictures younger than this are never collected (a download may have just written them).
const YOUNG: Duration = Duration::from_secs(60 * 60);
/// Left-over temporary files are removed after a day, files moved aside after a month.
const STRAY_AFTER: Duration = Duration::from_secs(DAY_SECS);
const DAMAGED_KEPT: Duration = Duration::from_secs(30 * DAY_SECS);

/// How a skin entered the history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SkinSource {
    /// Uploaded or worn through the launcher.
    Launcher,
    /// Seen on the profile when the launcher read it (changed elsewhere, or worn before the
    /// history began).
    Profile,
}

/// One skin of a profile's history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    /// The texture's id on Mojang's host (lowercase hex): the entry's key.
    pub texture_id: String,
    /// The arm model it was last worn with.
    pub variant: SkinVariant,
    pub first_seen: Timestamp,
    /// The last time the launcher saw it worn or put it on.
    pub last_worn: Timestamp,
    pub source: SkinSource,
    /// Kept by the player: listed first, never pruned.
    #[serde(default)]
    pub favorite: bool,
}

#[derive(Serialize)]
struct HistoryFile<'a> {
    version: u32,
    /// For whoever opens the folder; the file name is what counts.
    profile: String,
    entries: &'a [HistoryEntry],
}

/// What [`SkinHistory::tidy`] removed.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct HistoryTidy {
    /// Histories of profiles no longer signed in here.
    pub histories: usize,
    /// Pictures nothing refers to any more, left-over temporary files, old damaged files.
    pub files: usize,
    pub bytes: u64,
}

/// The skin histories of every profile on this computer (see the module documentation).
#[derive(Debug)]
pub struct SkinHistory {
    dir: PathBuf,
    /// Serializes every read-modify-write of the history files.
    lock: Mutex<()>,
    /// Entries removed moments ago, for undo.
    removed: Mutex<HashMap<(Uuid, String), (HistoryEntry, Instant)>>,
}

impl SkinHistory {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            lock: Mutex::new(()),
            removed: Mutex::new(HashMap::new()),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn file(&self, profile: Uuid) -> PathBuf {
        self.dir
            .join("history")
            .join(format!("{}.json", profile.simple()))
    }

    fn texture_path(&self, texture_id: &str) -> PathBuf {
        self.dir.join("textures").join(format!("{texture_id}.png"))
    }

    fn thumbnail_path(&self, texture_id: &str, variant: SkinVariant) -> PathBuf {
        self.dir
            .join("thumbs")
            .join(format!("{texture_id}-{}.png", variant.api_name()))
    }

    /// The entries of `profile`: favourites first, then the most recently worn. A missing,
    /// unreadable or damaged file reads as an empty history.
    pub fn entries(&self, profile: Uuid) -> Vec<HistoryEntry> {
        let _guard = self.lock.lock();
        let mut entries = self.load(profile).unwrap_or_else(|error| {
            tracing::warn!(code = %error.code(), %error, "skin history not readable");
            Vec::new()
        });
        sort_for_display(&mut entries);
        entries
    }

    /// The entry of `texture_id`, if the history of `profile` has it.
    pub fn entry(&self, profile: Uuid, texture_id: &str) -> Option<HistoryEntry> {
        let _guard = self.lock.lock();
        self.load(profile)
            .ok()?
            .into_iter()
            .find(|entry| entry.texture_id == texture_id)
    }

    /// Records `skin` as worn by `profile` at `now`. Returns whether the skin is in the history
    /// (default skins and textures that are not on Mojang's host are never recorded).
    pub fn record(
        &self,
        profile: Uuid,
        skin: &Skin,
        source: SkinSource,
        now: Timestamp,
    ) -> Result<bool> {
        if skin.is_default() {
            return Ok(false);
        }
        let Some(id) = texture_id(&skin.url) else {
            return Ok(false);
        };
        let _guard = self.lock.lock();
        let mut entries = self.load(profile)?;
        let latest = entries
            .iter()
            .max_by_key(|entry| entry.last_worn)
            .map(|entry| entry.texture_id.clone());
        let changed = match entries.iter_mut().find(|entry| entry.texture_id == id) {
            Some(entry) => {
                let worn_last = latest.as_deref() == Some(id.as_str());
                let due = now.duration_since(entry.last_worn) >= LAST_WORN_STEP;
                let variant = entry.variant != skin.variant;
                entry.variant = skin.variant;
                if !worn_last || due {
                    entry.last_worn = entry.last_worn.max(now);
                }
                variant || !worn_last || due
            }
            None => {
                entries.push(HistoryEntry {
                    texture_id: id.clone(),
                    variant: skin.variant,
                    first_seen: now,
                    last_worn: now,
                    source,
                    favorite: false,
                });
                true
            }
        };
        if changed {
            prune(&mut entries, &id);
            self.save(profile, &entries)?;
        }
        Ok(true)
    }

    /// Marks or unmarks an entry as a favourite. False when the history has no such entry.
    pub fn set_favorite(&self, profile: Uuid, texture_id: &str, favorite: bool) -> Result<bool> {
        let _guard = self.lock.lock();
        let mut entries = self.load(profile)?;
        let Some(entry) = entries
            .iter_mut()
            .find(|entry| entry.texture_id == texture_id)
        else {
            return Ok(false);
        };
        if entry.favorite != favorite {
            entry.favorite = favorite;
            self.save(profile, &entries)?;
        }
        Ok(true)
    }

    /// Removes an entry; [`Self::restore`] can bring it back for [`HISTORY_UNDO_WINDOW`]. Its
    /// pictures stay until the next [`Self::tidy`]. False when the history has no such entry.
    pub fn remove(&self, profile: Uuid, texture_id: &str) -> Result<bool> {
        let _guard = self.lock.lock();
        let mut entries = self.load(profile)?;
        let Some(index) = entries
            .iter()
            .position(|entry| entry.texture_id == texture_id)
        else {
            return Ok(false);
        };
        let entry = entries.remove(index);
        self.save(profile, &entries)?;
        let mut removed = self.removed.lock();
        removed.retain(|_, (_, at)| at.elapsed() < HISTORY_UNDO_WINDOW);
        removed.insert((profile, entry.texture_id.clone()), (entry, Instant::now()));
        Ok(true)
    }

    /// Puts back an entry removed within [`HISTORY_UNDO_WINDOW`], as it was. When the skin was
    /// recorded again meanwhile, that entry stays and gets the favourite mark back. False when
    /// there is nothing to put back.
    pub fn restore(&self, profile: Uuid, texture_id: &str) -> Result<bool> {
        let _guard = self.lock.lock();
        let key = (profile, texture_id.to_owned());
        let Some((entry, at)) = self.removed.lock().remove(&key) else {
            return Ok(false);
        };
        if at.elapsed() >= HISTORY_UNDO_WINDOW {
            return Ok(false);
        }
        let result = self.load(profile).and_then(|mut entries| {
            match entries
                .iter_mut()
                .find(|existing| existing.texture_id == entry.texture_id)
            {
                Some(existing) => {
                    existing.favorite |= entry.favorite;
                    existing.first_seen = existing.first_seen.min(entry.first_seen);
                    existing.source = entry.source;
                }
                None => entries.push(entry.clone()),
            }
            self.save(profile, &entries)
        });
        if let Err(error) = result {
            // Keep it for another try.
            self.removed.lock().insert(key, (entry, at));
            return Err(error);
        }
        Ok(true)
    }

    /// Mojang filed a skin the launcher put on under another texture id than the one asked for:
    /// the entry moves to the new id, keeping its favourite mark and first sighting.
    pub fn retarget(&self, profile: Uuid, from: &str, to: &str) -> Result<()> {
        if from == to || !is_texture_id(to) {
            return Ok(());
        }
        let _guard = self.lock.lock();
        let mut entries = self.load(profile)?;
        let Some(index) = entries.iter().position(|entry| entry.texture_id == from) else {
            return Ok(());
        };
        let old = entries.remove(index);
        match entries.iter_mut().find(|entry| entry.texture_id == to) {
            Some(new) => {
                new.favorite |= old.favorite;
                new.first_seen = new.first_seen.min(old.first_seen);
                new.source = old.source;
            }
            None => entries.push(HistoryEntry {
                texture_id: to.to_owned(),
                ..old
            }),
        }
        self.save(profile, &entries)
    }

    /// The cached texture `texture_id`, when there is a valid one (a 64×64 or 64×32 PNG).
    pub fn texture(&self, texture_id: &str) -> Option<Vec<u8>> {
        if !is_texture_id(texture_id) {
            return None;
        }
        let bytes = read_capped(&self.texture_path(texture_id), MAX_PICTURE_BYTES).ok()??;
        validate_skin_png(&bytes).is_ok().then_some(bytes)
    }

    /// Caches a texture downloaded from Mojang's host.
    pub fn store_texture(&self, texture_id: &str, png: &[u8]) -> Result<()> {
        if !is_texture_id(texture_id) {
            return Err(AuthError::unexpected(
                AuthStep::Texture,
                "invalid texture id",
            ));
        }
        let path = self.texture_path(texture_id);
        write_file(&path, png, false).map_err(|error| AuthError::storage("write", &path, error))
    }

    /// The front view of an entry, when it was drawn already.
    pub fn thumbnail(&self, texture_id: &str, variant: SkinVariant) -> Option<Vec<u8>> {
        if !is_texture_id(texture_id) {
            return None;
        }
        let bytes =
            read_capped(&self.thumbnail_path(texture_id, variant), MAX_PICTURE_BYTES).ok()??;
        bytes.starts_with(b"\x89PNG\r\n\x1a\n").then_some(bytes)
    }

    /// Draws the front view from the cached texture and stores it. `None` without a usable
    /// texture; a texture that cannot be drawn is deleted, so the next download replaces it.
    pub fn make_thumbnail(&self, texture_id: &str, variant: SkinVariant) -> Option<Vec<u8>> {
        let texture = self.texture(texture_id)?;
        match front_png(&texture, variant) {
            Ok(png) => {
                if let Err(error) =
                    write_file(&self.thumbnail_path(texture_id, variant), &png, false)
                {
                    tracing::debug!(%error, "skin thumbnail not cached");
                }
                Some(png)
            }
            Err(error) => {
                tracing::debug!(%error, "cached skin texture not drawable, dropping it");
                let _ = fs::remove_file(self.texture_path(texture_id));
                None
            }
        }
    }

    /// Housekeeping, for the launcher's daily tidy-up: the histories of profiles not in
    /// `signed_in` and unchanged for [`FORGOTTEN_PROFILE_KEPT`], pictures no history (nor an
    /// entry waiting for undo) refers to, left-over temporary files and old damaged files. When
    /// a history cannot be read, no picture is removed.
    pub fn tidy(&self, signed_in: &[Uuid], now: SystemTime) -> HistoryTidy {
        let _guard = self.lock.lock();
        let mut report = HistoryTidy::default();
        let older = |meta: &fs::Metadata, age: Duration| {
            meta.modified()
                .ok()
                .and_then(|modified| now.duration_since(modified).ok())
                .is_some_and(|elapsed| elapsed >= age)
        };
        let remove = |path: &Path, meta: &fs::Metadata, report: &mut HistoryTidy| {
            if fs::remove_file(path).is_ok() {
                report.files += 1;
                report.bytes += meta.len();
                true
            } else {
                false
            }
        };

        let mut textures = HashSet::new();
        let mut thumbnails = HashSet::new();
        let mut complete = true;
        for (path, meta) in files_in(&self.dir.join("history")) {
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            if name.ends_with(".tmp") {
                if older(&meta, STRAY_AFTER) {
                    remove(&path, &meta, &mut report);
                }
                continue;
            }
            if name.ends_with(".corrupt") {
                if older(&meta, DAMAGED_KEPT) {
                    remove(&path, &meta, &mut report);
                }
                continue;
            }
            let Some(profile) = name
                .strip_suffix(".json")
                .and_then(|stem| Uuid::parse_str(stem).ok())
            else {
                continue;
            };
            if !signed_in.contains(&profile) && older(&meta, FORGOTTEN_PROFILE_KEPT) {
                if remove(&path, &meta, &mut report) {
                    report.files -= 1;
                    report.histories += 1;
                }
                continue;
            }
            match self.load(profile) {
                Ok(entries) => {
                    for entry in entries {
                        thumbnails.insert(thumbnail_key(&entry));
                        textures.insert(entry.texture_id);
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, "skin history not readable, pictures kept");
                    complete = false;
                }
            }
        }
        for (entry, _) in self.removed.lock().values() {
            thumbnails.insert(thumbnail_key(entry));
            textures.insert(entry.texture_id.clone());
        }

        for (folder, keep) in [("textures", &textures), ("thumbs", &thumbnails)] {
            for (path, meta) in files_in(&self.dir.join(folder)) {
                let name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let stray = name.ends_with(".tmp") && older(&meta, STRAY_AFTER);
                let orphan = complete
                    && name
                        .strip_suffix(".png")
                        .is_some_and(|stem| !keep.contains(stem))
                    && older(&meta, YOUNG);
                if stray || orphan {
                    remove(&path, &meta, &mut report);
                }
            }
        }
        report
    }

    /// Reads a profile's entries. A missing file is an empty history; a damaged one is moved
    /// aside and reads as empty; one that cannot be read at all (locked, no permission) is an
    /// error, so that nothing overwrites it.
    fn load(&self, profile: Uuid) -> Result<Vec<HistoryEntry>> {
        let path = self.file(profile);
        let bytes = match read_capped(&path, MAX_FILE_BYTES) {
            Ok(Some(bytes)) => bytes,
            Ok(None) => {
                set_aside(&path);
                return Ok(Vec::new());
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(AuthError::storage("read", &path, error)),
        };
        match parse_entries(&bytes) {
            Some(entries) => Ok(entries),
            None => {
                set_aside(&path);
                Ok(Vec::new())
            }
        }
    }

    fn save(&self, profile: Uuid, entries: &[HistoryEntry]) -> Result<()> {
        let path = self.file(profile);
        let file = HistoryFile {
            version: FILE_VERSION,
            profile: profile.simple().to_string(),
            entries,
        };
        let json = serde_json::to_vec_pretty(&file)
            .map_err(|error| AuthError::storage("write", &path, io::Error::other(error)))?;
        write_file(&path, &json, true).map_err(|error| AuthError::storage("write", &path, error))
    }
}

/// A texture id as [`texture_id`] gives it: lowercase hex, at most 128 characters. Also safe as
/// a file name.
fn is_texture_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn thumbnail_key(entry: &HistoryEntry) -> String {
    format!("{}-{}", entry.texture_id, entry.variant.api_name())
}

/// The entries of a history file, skipping the ones that do not read (unknown fields are
/// ignored, duplicates keep their first copy). `None` when it is not a history file at all.
fn parse_entries(bytes: &[u8]) -> Option<Vec<HistoryEntry>> {
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let list = value.get("entries")?.as_array()?;
    let mut seen = HashSet::new();
    let mut entries = Vec::new();
    for item in list.iter().take(MAX_READ_ENTRIES) {
        let Ok(mut entry) = HistoryEntry::deserialize(item) else {
            continue;
        };
        entry.texture_id.make_ascii_lowercase();
        if is_texture_id(&entry.texture_id) && seen.insert(entry.texture_id.clone()) {
            entries.push(entry);
        }
    }
    Some(entries)
}

fn sort_for_display(entries: &mut [HistoryEntry]) {
    entries.sort_by(|a, b| {
        b.favorite
            .cmp(&a.favorite)
            .then(b.last_worn.cmp(&a.last_worn))
            .then(b.first_seen.cmp(&a.first_seen))
            .then(a.texture_id.cmp(&b.texture_id))
    });
}

/// Drops the least recently worn entries that are not favourites beyond [`MAX_KEPT`]. `keep`
/// (the skin worn now) never goes.
fn prune(entries: &mut Vec<HistoryEntry>, keep: &str) {
    let others = entries.iter().filter(|entry| !entry.favorite).count();
    if others <= MAX_KEPT {
        return;
    }
    let mut candidates: Vec<(Timestamp, Timestamp, String)> = entries
        .iter()
        .filter(|entry| !entry.favorite && entry.texture_id != keep)
        .map(|entry| (entry.last_worn, entry.first_seen, entry.texture_id.clone()))
        .collect();
    candidates.sort();
    let gone: HashSet<String> = candidates
        .into_iter()
        .take(others - MAX_KEPT)
        .map(|(_, _, id)| id)
        .collect();
    entries.retain(|entry| !gone.contains(&entry.texture_id));
}

/// The file's bytes, or `None` when it is larger than `cap` (read no further than that).
fn read_capped(path: &Path, cap: u64) -> io::Result<Option<Vec<u8>>> {
    let file = fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take(cap + 1).read_to_end(&mut bytes)?;
    Ok((bytes.len() as u64 <= cap).then_some(bytes))
}

/// Writes `<path>.tmp` and renames it over `path`; `durable` flushes it to disk first (files
/// the launcher cannot rebuild).
fn write_file(path: &Path, bytes: &[u8], durable: bool) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let result = (|| {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        if durable {
            file.sync_all()?;
        }
        drop(file);
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Moves a damaged history aside (`<file>.corrupt`, replacing an older one), so a fresh one can
/// start and the old bytes stay for a while.
fn set_aside(path: &Path) {
    let mut aside = path.as_os_str().to_owned();
    aside.push(".corrupt");
    let aside = PathBuf::from(aside);
    tracing::warn!(file = %path.display(), "skin history damaged, starting a new one");
    if let Err(error) = fs::rename(path, &aside) {
        tracing::warn!(%error, "could not move the damaged skin history aside");
    }
}

/// The plain files of a folder (none when it does not exist).
fn files_in(dir: &Path) -> Vec<(PathBuf, fs::Metadata)> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?;
            meta.is_file().then(|| (entry.path(), meta))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::avatar::tests::texture;

    const T: [&str; 4] = ["aa01", "bb02", "cc03", "dd04"];

    fn skin(id: &str, variant: SkinVariant) -> Skin {
        Skin {
            url: format!("https://textures.minecraft.net/texture/{id}"),
            variant,
            alias: None,
        }
    }

    fn at(minutes: i64) -> Timestamp {
        Timestamp::from_second(1_800_000_000 + minutes * 60).unwrap()
    }

    fn profile() -> Uuid {
        Uuid::parse_str("069a79f444e94726a5befca90e38aaf5").unwrap()
    }

    fn ids(entries: &[HistoryEntry]) -> Vec<&str> {
        entries
            .iter()
            .map(|entry| entry.texture_id.as_str())
            .collect()
    }

    #[test]
    fn records_are_deduplicated_by_texture() {
        let dir = tempfile::tempdir().unwrap();
        let history = SkinHistory::new(dir.path());
        let p = profile();
        assert!(history.entries(p).is_empty());

        let first = skin(T[0], SkinVariant::Classic);
        assert!(
            history
                .record(p, &first, SkinSource::Profile, at(0))
                .unwrap()
        );
        assert!(
            history
                .record(
                    p,
                    &skin(T[1], SkinVariant::Slim),
                    SkinSource::Launcher,
                    at(10)
                )
                .unwrap()
        );
        // The first skin again, now with slim arms and uppercase in its URL.
        let again = Skin {
            url: format!(
                "http://textures.minecraft.net/texture/{}",
                T[0].to_uppercase()
            ),
            variant: SkinVariant::Slim,
            alias: None,
        };
        assert!(
            history
                .record(p, &again, SkinSource::Launcher, at(20))
                .unwrap()
        );

        let entries = history.entries(p);
        assert_eq!(ids(&entries), [T[0], T[1]]);
        let worn = &entries[0];
        assert_eq!(worn.variant, SkinVariant::Slim);
        assert_eq!(worn.first_seen, at(0));
        assert_eq!(worn.last_worn, at(20));
        // How it first came in stays.
        assert_eq!(worn.source, SkinSource::Profile);
        assert_eq!(entries[1].source, SkinSource::Launcher);
        assert!(history.file(p).is_file());
    }

    #[test]
    fn default_and_foreign_skins_are_not_recorded() {
        let dir = tempfile::tempdir().unwrap();
        let history = SkinHistory::new(dir.path());
        let p = profile();
        let mut steve = skin(T[0], SkinVariant::Classic);
        steve.alias = Some("STEVE".into());
        assert!(
            !history
                .record(p, &steve, SkinSource::Profile, at(0))
                .unwrap()
        );
        let foreign = Skin {
            url: "https://example.com/texture/aa01".into(),
            variant: SkinVariant::Classic,
            alias: None,
        };
        assert!(
            !history
                .record(p, &foreign, SkinSource::Profile, at(0))
                .unwrap()
        );
        assert!(history.entries(p).is_empty());
        assert!(!history.file(p).exists());
    }

    #[test]
    fn the_worn_skin_is_not_rewritten_on_every_read() {
        let dir = tempfile::tempdir().unwrap();
        let history = SkinHistory::new(dir.path());
        let p = profile();
        let worn = skin(T[0], SkinVariant::Classic);
        history
            .record(p, &worn, SkinSource::Profile, at(0))
            .unwrap();
        history
            .record(p, &worn, SkinSource::Profile, at(30))
            .unwrap();
        assert_eq!(history.entries(p)[0].last_worn, at(0));
        history
            .record(p, &worn, SkinSource::Profile, at(61))
            .unwrap();
        assert_eq!(history.entries(p)[0].last_worn, at(61));

        // Back to an older skin: always written, so the order follows at once.
        history
            .record(
                p,
                &skin(T[1], SkinVariant::Classic),
                SkinSource::Profile,
                at(62),
            )
            .unwrap();
        history
            .record(p, &worn, SkinSource::Profile, at(63))
            .unwrap();
        assert_eq!(ids(&history.entries(p)), [T[0], T[1]]);
        assert_eq!(history.entries(p)[0].last_worn, at(63));
    }

    #[test]
    fn favourites_come_first_and_are_never_pruned() {
        let dir = tempfile::tempdir().unwrap();
        let history = SkinHistory::new(dir.path());
        let p = profile();
        let id = |n: usize| format!("{n:04x}");
        for n in 0..MAX_KEPT {
            history
                .record(
                    p,
                    &skin(&id(n), SkinVariant::Classic),
                    SkinSource::Profile,
                    at(n as i64),
                )
                .unwrap();
        }
        // The two oldest become favourites.
        assert!(history.set_favorite(p, &id(0), true).unwrap());
        assert!(history.set_favorite(p, &id(1), true).unwrap());
        assert!(!history.set_favorite(p, "ffff", true).unwrap());
        let entries = history.entries(p);
        assert_eq!(entries.len(), MAX_KEPT);
        assert_eq!(ids(&entries[..3]), [id(1), id(0), id(MAX_KEPT - 1)]);

        // Favourites do not count: 58 others, three more make 61, and the least recently worn
        // one that is not a favourite goes.
        for n in MAX_KEPT..MAX_KEPT + 3 {
            history
                .record(
                    p,
                    &skin(&id(n), SkinVariant::Classic),
                    SkinSource::Profile,
                    at(n as i64),
                )
                .unwrap();
        }
        let entries = history.entries(p);
        assert_eq!(entries.len(), MAX_KEPT + 2);
        let kept: HashSet<&str> = ids(&entries).into_iter().collect();
        assert!(!kept.contains(id(2).as_str()));
        for stays in [0, 1, 3, MAX_KEPT + 2] {
            assert!(kept.contains(id(stays).as_str()), "{stays}");
        }
    }

    #[test]
    fn the_worn_skin_survives_pruning_even_when_oldest() {
        let mut entries: Vec<HistoryEntry> = (0..=MAX_KEPT)
            .map(|n| HistoryEntry {
                texture_id: format!("{n:04x}"),
                variant: SkinVariant::Classic,
                first_seen: at(n as i64),
                last_worn: at(n as i64),
                source: SkinSource::Profile,
                favorite: false,
            })
            .collect();
        prune(&mut entries, "0000");
        assert_eq!(entries.len(), MAX_KEPT);
        assert!(entries.iter().any(|entry| entry.texture_id == "0000"));
        assert!(!entries.iter().any(|entry| entry.texture_id == "0001"));
    }

    #[test]
    fn removals_can_be_undone() {
        let dir = tempfile::tempdir().unwrap();
        let history = SkinHistory::new(dir.path());
        let p = profile();
        history
            .record(
                p,
                &skin(T[0], SkinVariant::Classic),
                SkinSource::Profile,
                at(0),
            )
            .unwrap();
        history
            .record(
                p,
                &skin(T[1], SkinVariant::Slim),
                SkinSource::Launcher,
                at(5),
            )
            .unwrap();
        history.set_favorite(p, T[0], true).unwrap();

        assert!(history.remove(p, T[0]).unwrap());
        assert!(!history.remove(p, T[0]).unwrap());
        assert_eq!(ids(&history.entries(p)), [T[1]]);
        assert!(history.restore(p, T[0]).unwrap());
        let entries = history.entries(p);
        assert_eq!(ids(&entries), [T[0], T[1]]);
        assert!(entries[0].favorite);
        assert_eq!(entries[0].first_seen, at(0));
        // Nothing left to put back.
        assert!(!history.restore(p, T[0]).unwrap());
        assert!(!history.restore(p, T[2]).unwrap());

        // Seen again before the undo: one entry, favourite back.
        history.remove(p, T[0]).unwrap();
        history
            .record(
                p,
                &skin(T[0], SkinVariant::Classic),
                SkinSource::Profile,
                at(9),
            )
            .unwrap();
        assert!(history.restore(p, T[0]).unwrap());
        let entries = history.entries(p);
        assert_eq!(entries.len(), 2);
        assert!(entries[0].favorite);
        assert_eq!(entries[0].first_seen, at(0));
        assert_eq!(entries[0].last_worn, at(9));
    }

    #[test]
    fn retargeting_keeps_the_favourite() {
        let dir = tempfile::tempdir().unwrap();
        let history = SkinHistory::new(dir.path());
        let p = profile();
        history
            .record(
                p,
                &skin(T[0], SkinVariant::Classic),
                SkinSource::Profile,
                at(0),
            )
            .unwrap();
        history.set_favorite(p, T[0], true).unwrap();
        // The wear recorded the new id first.
        history
            .record(
                p,
                &skin(T[1], SkinVariant::Classic),
                SkinSource::Launcher,
                at(5),
            )
            .unwrap();
        history.retarget(p, T[0], T[1]).unwrap();
        let entries = history.entries(p);
        assert_eq!(ids(&entries), [T[1]]);
        assert!(entries[0].favorite);
        assert_eq!(entries[0].first_seen, at(0));
        assert_eq!(entries[0].last_worn, at(5));

        // Without an entry for the new id, the old one is renamed.
        history.retarget(p, T[1], T[2]).unwrap();
        assert_eq!(ids(&history.entries(p)), [T[2]]);
        history.retarget(p, T[2], "not hex").unwrap();
        assert_eq!(ids(&history.entries(p)), [T[2]]);
    }

    #[test]
    fn damaged_files_never_break_the_history() {
        let dir = tempfile::tempdir().unwrap();
        let history = SkinHistory::new(dir.path());
        let p = profile();
        let file = history.file(p);
        fs::create_dir_all(file.parent().unwrap()).unwrap();

        // Some entries damaged: the readable ones stay.
        fs::write(
            &file,
            serde_json::json!({
                "version": 1,
                "entries": [
                    {"textureId": "AA01", "variant": "slim", "firstSeen": "2026-10-01T10:00:00Z",
                     "lastWorn": "2026-10-02T10:00:00Z", "source": "profile", "extra": 1},
                    {"textureId": "../../evil", "variant": "classic", "firstSeen": "2026-10-01T10:00:00Z",
                     "lastWorn": "2026-10-01T10:00:00Z", "source": "profile"},
                    {"textureId": "bb02", "variant": "triangle"},
                    "nonsense",
                    {"textureId": "aa01", "variant": "classic", "firstSeen": "2026-10-01T10:00:00Z",
                     "lastWorn": "2026-10-01T10:00:00Z", "source": "launcher"}
                ]
            })
            .to_string(),
        )
        .unwrap();
        let entries = history.entries(p);
        assert_eq!(ids(&entries), ["aa01"]);
        assert_eq!(entries[0].variant, SkinVariant::Slim);
        assert!(!entries[0].favorite);

        // Not JSON at all: moved aside, a new history starts.
        fs::write(&file, b"\x00\x01 not json").unwrap();
        assert!(history.entries(p).is_empty());
        let mut aside = file.clone().into_os_string();
        aside.push(".corrupt");
        assert!(PathBuf::from(aside).is_file());
        history
            .record(
                p,
                &skin(T[3], SkinVariant::Classic),
                SkinSource::Profile,
                at(0),
            )
            .unwrap();
        assert_eq!(ids(&history.entries(p)), [T[3]]);

        // A huge file is not a history.
        fs::write(&file, vec![b' '; (MAX_FILE_BYTES + 1) as usize]).unwrap();
        assert!(history.entries(p).is_empty());
    }

    #[test]
    fn pictures_are_validated_and_drawn_once() {
        let dir = tempfile::tempdir().unwrap();
        let history = SkinHistory::new(dir.path());
        let png = texture(64, 64, |x, y| [x as u8, y as u8, 7, 255]);
        assert_eq!(history.texture(T[0]), None);
        assert_eq!(history.make_thumbnail(T[0], SkinVariant::Classic), None);
        history.store_texture(T[0], &png).unwrap();
        assert_eq!(history.texture(T[0]), Some(png.clone()));
        assert!(history.store_texture("../x", &png).is_err());
        assert_eq!(history.texture("../x"), None);

        assert_eq!(history.thumbnail(T[0], SkinVariant::Slim), None);
        let drawn = history.make_thumbnail(T[0], SkinVariant::Slim).unwrap();
        assert_eq!(history.thumbnail(T[0], SkinVariant::Slim), Some(drawn));
        assert_eq!(history.thumbnail(T[0], SkinVariant::Classic), None);

        // A damaged texture is dropped, so the next download replaces it.
        fs::write(history.texture_path(T[1]), b"\x89PNG\r\n\x1a\n broken").unwrap();
        assert_eq!(history.texture(T[1]), None);
        fs::write(history.texture_path(T[2]), texture(32, 32, |_, _| [0; 4])).unwrap();
        assert_eq!(history.make_thumbnail(T[2], SkinVariant::Classic), None);
    }

    #[test]
    fn tidy_keeps_what_histories_use() {
        let dir = tempfile::tempdir().unwrap();
        let history = SkinHistory::new(dir.path());
        let png = texture(64, 64, |_, _| [9, 9, 9, 255]);
        let p = profile();
        let gone = Uuid::from_u128(7);
        history
            .record(
                p,
                &skin(T[0], SkinVariant::Classic),
                SkinSource::Profile,
                at(0),
            )
            .unwrap();
        history
            .record(
                gone,
                &skin(T[1], SkinVariant::Slim),
                SkinSource::Profile,
                at(0),
            )
            .unwrap();
        history
            .record(
                p,
                &skin(T[2], SkinVariant::Classic),
                SkinSource::Profile,
                at(1),
            )
            .unwrap();
        for id in T {
            history.store_texture(id, &png).unwrap();
        }
        history.make_thumbnail(T[0], SkinVariant::Classic).unwrap();
        history.make_thumbnail(T[0], SkinVariant::Slim).unwrap();
        history.make_thumbnail(T[1], SkinVariant::Slim).unwrap();
        // T[2] waits for undo.
        history.remove(p, T[2]).unwrap();

        // Everything is young: nothing goes.
        let now = SystemTime::now();
        assert_eq!(history.tidy(&[p], now), HistoryTidy::default());

        // A day later: the unused texture and thumbnail go; the other profile's history stays.
        let report = history.tidy(&[p], now + Duration::from_secs(DAY_SECS));
        assert_eq!(report.histories, 0);
        assert_eq!(report.files, 2);
        assert!(history.texture(T[3]).is_none());
        assert!(history.thumbnail(T[0], SkinVariant::Slim).is_none());
        assert!(history.thumbnail(T[0], SkinVariant::Classic).is_some());
        assert!(history.texture(T[1]).is_some());
        assert!(history.texture(T[2]).is_some());

        // Half a year later the other profile's history and its pictures go too.
        let later = now + FORGOTTEN_PROFILE_KEPT + Duration::from_secs(DAY_SECS);
        let report = history.tidy(&[p], later);
        assert_eq!(report.histories, 1);
        assert!(!history.file(gone).exists());
        assert!(history.texture(T[1]).is_none());
        assert!(history.thumbnail(T[1], SkinVariant::Slim).is_none());
        assert!(history.texture(T[0]).is_some());
        assert_eq!(ids(&history.entries(p)), [T[0]]);
    }
}

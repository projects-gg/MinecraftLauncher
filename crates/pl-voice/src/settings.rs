//! The player's voice settings (Settings → Sesli sohbet, design §5.2) and their per-person choices
//! (volume, "Sustur (yalnız sen)", §6.4), kept in the launcher's data folder (`voice/`).
//!
//! The device choices stay on this computer; everything else may travel in the settings backup
//! ([`VoiceBackup`], the page's `BACKUP_PREFS` list names `voice`). Files are written whole through
//! a temporary file, so a crash never leaves half a file; an unreadable file reads as the defaults.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The folder under the data root.
pub const FOLDER: &str = "voice";
const SETTINGS_FILE: &str = "settings.json";
const PEOPLE_FILE: &str = "people.json";
/// People whose choices are kept at most (the oldest change goes first).
pub const PEOPLE_MAX: usize = 500;
/// People a backup carries at most, the latest changed (VOICE-INT: the friends service keeps a
/// backup of at most 16 KB, and its friends part alone reaches ~11 KB; 50 people are ~2.6 KB).
pub const BACKUP_PEOPLE_MAX: usize = 50;
/// A person's volume tops out here (percent; an `<audio>` engine stops at 100).
pub const VOLUME_MAX: u16 = 200;
/// The microphone's own volume (when automatic volume is off) tops out here.
pub const MIC_VOLUME_MAX: u16 = 200;

/// When the microphone sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum VoiceTalkMode {
    /// "Konuşunca": when the player speaks (no setup).
    #[default]
    Voice,
    /// "Bas-konuş": while the chosen key is held.
    PushToTalk,
}

/// A mouse button a shortcut may use (the left and right buttons never).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum VoiceMouseButton {
    Middle,
    /// The back side button.
    Back,
    /// The forward side button.
    Forward,
}

/// A shortcut's key: a keyboard key by its place on the keyboard (the page's
/// `KeyboardEvent.code`, `KeyV`, `CapsLock`, `F8`: the same key whatever the layout), or a mouse
/// button. `label` is what the page showed when it was picked (the layout's letter), kept to show
/// it again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum VoiceKey {
    Keyboard {
        code: String,
        #[serde(default)]
        label: String,
    },
    Mouse {
        button: VoiceMouseButton,
    },
}

impl VoiceKey {
    /// The same key, whatever its label says.
    pub fn same_key(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Keyboard { code: a, .. }, Self::Keyboard { code: b, .. }) => a == b,
            (Self::Mouse { button: a }, Self::Mouse { button: b }) => a == b,
            _ => false,
        }
    }
}

/// Settings → Sesli sohbet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct VoiceSettings {
    /// "Sesli sohbet": off, no "Ara", no ring, and nobody can call the player.
    pub enabled: bool,
    /// The microphone (none: the system's default). This computer only.
    pub input_device: Option<String>,
    /// Where voices play (none: the system's default). This computer only.
    pub output_device: Option<String>,
    pub mode: VoiceTalkMode,
    /// "Bas-konuş" key (needed for push-to-talk).
    pub push_to_talk_key: Option<VoiceKey>,
    /// "Mikrofonu kapat/aç" shortcut (none by default).
    pub mute_key: Option<VoiceKey>,
    /// "Sağırlaştır" shortcut (none by default).
    pub deafen_key: Option<VoiceKey>,
    /// "Gürültü engelleme".
    pub noise_suppression: bool,
    /// "Yankı önleme" (Gelişmiş).
    pub echo_cancellation: bool,
    /// "Otomatik ses seviyesi" (Gelişmiş).
    pub auto_gain: bool,
    /// "Hassasiyet": none is "Otomatik"; else the threshold, 0 (everything) to 100.
    pub sensitivity: Option<u8>,
    /// "Mikrofon sesi" while automatic volume is off (percent, 0–200).
    pub mic_volume: u16,
    /// "Katılma ve ayrılma sesleri".
    pub sounds: bool,
}

impl Default for VoiceSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            input_device: None,
            output_device: None,
            mode: VoiceTalkMode::Voice,
            push_to_talk_key: None,
            mute_key: None,
            deafen_key: None,
            noise_suppression: true,
            echo_cancellation: true,
            auto_gain: true,
            sensitivity: None,
            mic_volume: 100,
            sounds: true,
        }
    }
}

impl VoiceSettings {
    /// Values held to their ranges; a key the launcher cannot watch is dropped, and a key used by
    /// two shortcuts stays with the first (push-to-talk, then mute, then deafen).
    pub fn normalized(mut self) -> Self {
        self.sensitivity = self.sensitivity.map(|value| value.min(100));
        self.mic_volume = self.mic_volume.min(MIC_VOLUME_MAX);
        self.input_device = self.input_device.filter(|id| valid_device(id));
        self.output_device = self.output_device.filter(|id| valid_device(id));
        let usable = |key: Option<VoiceKey>| key.filter(crate::keys::usable);
        self.push_to_talk_key = usable(self.push_to_talk_key.take());
        self.mute_key = usable(self.mute_key.take());
        self.deafen_key = usable(self.deafen_key.take());
        if let (Some(ptt), Some(mute)) = (&self.push_to_talk_key, &self.mute_key)
            && ptt.same_key(mute)
        {
            self.mute_key = None;
        }
        for earlier in [&self.push_to_talk_key, &self.mute_key] {
            if let (Some(earlier), Some(deafen)) = (earlier, &self.deafen_key)
                && earlier.same_key(deafen)
            {
                self.deafen_key = None;
            }
        }
        if let Some(VoiceKey::Keyboard { label, .. }) = &mut self.push_to_talk_key {
            clip_label(label);
        }
        if let Some(VoiceKey::Keyboard { label, .. }) = &mut self.mute_key {
            clip_label(label);
        }
        if let Some(VoiceKey::Keyboard { label, .. }) = &mut self.deafen_key {
            clip_label(label);
        }
        self
    }

    /// Push-to-talk is on and has its key.
    pub fn push_to_talk(&self) -> Option<&VoiceKey> {
        match self.mode {
            VoiceTalkMode::PushToTalk => self.push_to_talk_key.as_ref(),
            VoiceTalkMode::Voice => None,
        }
    }

    /// The keys a room needs watched: push-to-talk (when on) and the shortcuts.
    pub fn watched_keys(&self) -> Vec<(crate::view::VoiceShortcut, VoiceKey)> {
        use crate::view::VoiceShortcut;
        let mut keys = Vec::new();
        if let Some(key) = self.push_to_talk() {
            keys.push((VoiceShortcut::PushToTalk, key.clone()));
        }
        if let Some(key) = &self.mute_key {
            keys.push((VoiceShortcut::Mute, key.clone()));
        }
        if let Some(key) = &self.deafen_key {
            keys.push((VoiceShortcut::Deafen, key.clone()));
        }
        keys
    }
}

/// A device id as the page names it (WebRTC's: up to 128 printable characters).
fn valid_device(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128 && id.chars().all(|c| !c.is_control())
}

fn clip_label(label: &mut String) {
    if label.chars().count() > 16 {
        *label = label.chars().take(16).collect();
    }
}

/// One person's choices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct VoicePersonPrefs {
    /// Percent, 0–200.
    pub volume: u16,
    /// "Sustur (yalnız sen)".
    pub muted: bool,
}

impl Default for VoicePersonPrefs {
    fn default() -> Self {
        Self {
            volume: 100,
            muted: false,
        }
    }
}

impl VoicePersonPrefs {
    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// What the settings backup carries of voice chat (the page builds the backup): the settings
/// without devices, and the per-person choices.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct VoiceBackup {
    pub settings: VoiceSettings,
    /// User id → choices (at most [`BACKUP_PEOPLE_MAX`]).
    pub people: BTreeMap<String, VoicePersonPrefs>,
}

/// The people file: choices in the order they were last changed (the oldest go first).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PeopleFile {
    #[serde(default)]
    people: Vec<(String, VoicePersonPrefs)>,
}

/// The settings and choices on disk, held in memory.
#[derive(Debug)]
pub struct Store {
    folder: PathBuf,
    settings: VoiceSettings,
    /// Last changed last.
    people: Vec<(String, VoicePersonPrefs)>,
    /// Grows with every change the backup carries (`VoiceState.backup_rev`).
    revision: u32,
}

impl Store {
    /// Reads `<root>/voice/`; missing or unreadable files read as the defaults.
    pub fn open(root: &Path) -> Self {
        let folder = root.join(FOLDER);
        let settings = std::fs::read(folder.join(SETTINGS_FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<VoiceSettings>(&bytes).ok())
            .map(VoiceSettings::normalized)
            .unwrap_or_default();
        let people = std::fs::read(folder.join(PEOPLE_FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<PeopleFile>(&bytes).ok())
            .map(|file| {
                file.people
                    .into_iter()
                    .filter(|(id, _)| valid_person(id))
                    .map(|(id, prefs)| (id, clean_prefs(prefs)))
                    .collect()
            })
            .unwrap_or_default();
        Self {
            folder,
            settings,
            people,
            revision: 0,
        }
    }

    /// A store that never touches the disk (tests).
    pub fn in_memory() -> Self {
        Self {
            folder: PathBuf::new(),
            settings: VoiceSettings::default(),
            people: Vec::new(),
            revision: 0,
        }
    }

    pub fn settings(&self) -> &VoiceSettings {
        &self.settings
    }

    /// Grows with every change the settings backup carries (a device choice too: cheap, and the
    /// page's fingerprint skips an upload that would change nothing).
    pub fn revision(&self) -> u32 {
        self.revision
    }

    /// Saves new settings (normalized); returns them as kept.
    pub fn set_settings(&mut self, settings: VoiceSettings) -> VoiceSettings {
        let settings = settings.normalized();
        if settings != self.settings {
            self.revision = self.revision.wrapping_add(1);
        }
        self.settings = settings;
        self.write_settings();
        self.settings.clone()
    }

    pub fn person(&self, id: &str) -> VoicePersonPrefs {
        self.people
            .iter()
            .find(|(person, _)| person == id)
            .map(|(_, prefs)| *prefs)
            .unwrap_or_default()
    }

    /// Changes one person's choices; true when anything changed.
    pub fn set_person(&mut self, id: &str, change: impl FnOnce(&mut VoicePersonPrefs)) -> bool {
        if !valid_person(id) {
            return false;
        }
        let before = self.person(id);
        let mut prefs = before;
        change(&mut prefs);
        let prefs = clean_prefs(prefs);
        if prefs == before {
            return false;
        }
        self.people.retain(|(person, _)| person != id);
        if !prefs.is_default() {
            self.people.push((id.to_owned(), prefs));
        }
        if self.people.len() > PEOPLE_MAX {
            let extra = self.people.len() - PEOPLE_MAX;
            self.people.drain(..extra);
        }
        self.revision = self.revision.wrapping_add(1);
        self.write_people();
        true
    }

    /// What the settings backup carries.
    pub fn backup(&self) -> VoiceBackup {
        let mut settings = self.settings.clone();
        settings.input_device = None;
        settings.output_device = None;
        let people = self
            .people
            .iter()
            .rev()
            .take(BACKUP_PEOPLE_MAX)
            .map(|(id, prefs)| (id.clone(), *prefs))
            .collect();
        VoiceBackup { settings, people }
    }

    /// A downloaded backup: its settings (this computer's devices stay) and its people's choices
    /// over the ones here.
    pub fn restore(&mut self, backup: VoiceBackup) {
        let mut settings = backup.settings;
        settings.input_device = self.settings.input_device.clone();
        settings.output_device = self.settings.output_device.clone();
        self.settings = settings.normalized();
        for (id, prefs) in backup.people.into_iter().take(BACKUP_PEOPLE_MAX) {
            if valid_person(&id) {
                let prefs = clean_prefs(prefs);
                self.people.retain(|(person, _)| person != &id);
                if !prefs.is_default() {
                    self.people.push((id, prefs));
                }
            }
        }
        if self.people.len() > PEOPLE_MAX {
            let extra = self.people.len() - PEOPLE_MAX;
            self.people.drain(..extra);
        }
        self.revision = self.revision.wrapping_add(1);
        self.write_settings();
        self.write_people();
    }

    fn write_settings(&self) {
        if let Ok(bytes) = serde_json::to_vec_pretty(&self.settings) {
            self.write(SETTINGS_FILE, &bytes);
        }
    }

    fn write_people(&self) {
        let file = PeopleFile {
            people: self.people.clone(),
        };
        if let Ok(bytes) = serde_json::to_vec(&file) {
            self.write(PEOPLE_FILE, &bytes);
        }
    }

    fn write(&self, name: &str, bytes: &[u8]) {
        if self.folder.as_os_str().is_empty() {
            return;
        }
        let result = std::fs::create_dir_all(&self.folder).and_then(|()| {
            let path = self.folder.join(name);
            let temporary = self.folder.join(format!("{name}.tmp"));
            std::fs::write(&temporary, bytes)?;
            std::fs::rename(&temporary, &path)
        });
        if let Err(error) = result {
            tracing::warn!(%error, file = name, "voice settings not saved");
        }
    }
}

/// A user id as the friends service gives them (digits).
pub fn valid_person(id: &str) -> bool {
    !id.is_empty() && id.len() <= 32 && id.bytes().all(|b| b.is_ascii_alphanumeric())
}

fn clean_prefs(prefs: VoicePersonPrefs) -> VoicePersonPrefs {
    VoicePersonPrefs {
        volume: prefs.volume.min(VOLUME_MAX),
        muted: prefs.muted,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn key(code: &str) -> VoiceKey {
        VoiceKey::Keyboard {
            code: code.into(),
            label: code.into(),
        }
    }

    #[test]
    fn defaults_need_no_setup() {
        let settings = VoiceSettings::default();
        assert!(settings.enabled && settings.noise_suppression && settings.sounds);
        assert_eq!(settings.mode, VoiceTalkMode::Voice);
        assert!(settings.watched_keys().is_empty());
        // An older or empty file reads as the defaults.
        let read: VoiceSettings = serde_json::from_value(json!({})).unwrap();
        assert_eq!(read, settings);
    }

    #[test]
    fn keys_are_checked_and_shared_once() {
        let settings = VoiceSettings {
            mode: VoiceTalkMode::PushToTalk,
            push_to_talk_key: Some(key("KeyB")),
            mute_key: Some(key("KeyB")),
            deafen_key: Some(VoiceKey::Mouse {
                button: VoiceMouseButton::Back,
            }),
            sensitivity: Some(250),
            mic_volume: 900,
            ..VoiceSettings::default()
        }
        .normalized();
        assert_eq!(settings.mute_key, None);
        assert_eq!(settings.sensitivity, Some(100));
        assert_eq!(settings.mic_volume, MIC_VOLUME_MAX);
        let watched = settings.watched_keys();
        assert_eq!(watched.len(), 2);
        // A key the launcher does not know is dropped.
        let unknown = VoiceSettings {
            push_to_talk_key: Some(key("Hyper")),
            ..VoiceSettings::default()
        }
        .normalized();
        assert_eq!(unknown.push_to_talk_key, None);
        // Push-to-talk's key is watched only in push-to-talk.
        let voice = VoiceSettings {
            push_to_talk_key: Some(key("KeyB")),
            ..VoiceSettings::default()
        };
        assert!(voice.watched_keys().is_empty());
    }

    #[test]
    fn keys_read_as_the_page_writes_them() {
        assert_eq!(
            serde_json::from_value::<VoiceKey>(
                json!({"kind":"keyboard","code":"CapsLock","label":"Caps Lock"})
            )
            .unwrap(),
            VoiceKey::Keyboard {
                code: "CapsLock".into(),
                label: "Caps Lock".into()
            }
        );
        assert_eq!(
            serde_json::to_value(VoiceKey::Mouse {
                button: VoiceMouseButton::Forward
            })
            .unwrap(),
            json!({"kind":"mouse","button":"forward"})
        );
    }

    #[test]
    fn people_are_remembered_and_saved() {
        let root = tempfile::tempdir().unwrap();
        let mut store = Store::open(root.path());
        assert_eq!(store.person("42"), VoicePersonPrefs::default());
        assert!(store.set_person("42", |prefs| prefs.volume = 250));
        assert_eq!(store.person("42").volume, VOLUME_MAX);
        assert!(!store.set_person("42", |prefs| prefs.volume = 200));
        assert!(store.set_person("7", |prefs| prefs.muted = true));
        assert!(!store.set_person("not an id!", |prefs| prefs.muted = true));
        let settings = store.set_settings(VoiceSettings {
            input_device: Some("mic-1".into()),
            sounds: false,
            ..VoiceSettings::default()
        });
        assert!(!settings.sounds);
        let again = Store::open(root.path());
        assert_eq!(again.person("42").volume, 200);
        assert!(again.person("7").muted);
        assert_eq!(again.settings().input_device.as_deref(), Some("mic-1"));
        // Back to the default: forgotten.
        let mut store = again;
        assert!(store.set_person("7", |prefs| prefs.muted = false));
        assert_eq!(Store::open(root.path()).people.len(), 1);
    }

    #[test]
    fn a_backup_leaves_this_computers_devices() {
        let mut here = Store::in_memory();
        here.set_settings(VoiceSettings {
            input_device: Some("mic-here".into()),
            ..VoiceSettings::default()
        });
        here.set_person("1", |prefs| prefs.volume = 150);
        let backup = here.backup();
        assert_eq!(backup.settings.input_device, None);
        assert_eq!(backup.people.get("1").map(|p| p.volume), Some(150));
        let mut other = Store::in_memory();
        other.set_settings(VoiceSettings {
            output_device: Some("speakers-there".into()),
            ..VoiceSettings::default()
        });
        let mut carried = backup;
        carried.settings.sounds = false;
        other.restore(carried);
        assert!(!other.settings().sounds);
        assert_eq!(
            other.settings().output_device.as_deref(),
            Some("speakers-there")
        );
        assert_eq!(other.person("1").volume, 150);
    }

    #[test]
    fn the_people_list_stays_bounded() {
        let mut store = Store::in_memory();
        for id in 0..(PEOPLE_MAX + 20) {
            store.set_person(&id.to_string(), |prefs| prefs.muted = true);
        }
        assert_eq!(store.people.len(), PEOPLE_MAX);
        // The oldest changes went first.
        assert!(!store.person("0").muted);
        assert!(store.person(&(PEOPLE_MAX + 19).to_string()).muted);
        let backup = store.backup();
        assert_eq!(backup.people.len(), BACKUP_PEOPLE_MAX);
        // The latest changes travel.
        assert!(backup.people.contains_key(&(PEOPLE_MAX + 19).to_string()));
        assert!(!backup.people.contains_key("100"));
    }

    #[test]
    fn the_backup_fits_the_services_share() {
        // The longest ids the service gives, every choice away from the default, every key set:
        // well under the ~5 KB the friends service's 16 KB backup leaves beside the friends part.
        let mut store = Store::in_memory();
        let long = |n: usize| format!("{:020}", 10_000_000_000_000_000_000_u128 + n as u128);
        for n in 0..(BACKUP_PEOPLE_MAX + 10) {
            store.set_person(&long(n), |prefs| {
                prefs.volume = 150;
                prefs.muted = true;
            });
        }
        store.set_settings(VoiceSettings {
            mode: VoiceTalkMode::PushToTalk,
            push_to_talk_key: Some(key("CapsLock")),
            mute_key: Some(key("KeyM")),
            deafen_key: Some(key("KeyN")),
            sensitivity: Some(40),
            ..VoiceSettings::default()
        });
        let bytes = serde_json::to_vec(&store.backup()).unwrap().len();
        assert!(bytes < 4_000, "{bytes} bytes");
    }

    #[test]
    fn every_change_the_backup_carries_counts() {
        let mut store = Store::in_memory();
        let start = store.revision();
        store.set_settings(VoiceSettings::default());
        assert_eq!(store.revision(), start, "the same settings change nothing");
        store.set_settings(VoiceSettings {
            sounds: false,
            ..VoiceSettings::default()
        });
        assert_eq!(store.revision(), start + 1);
        assert!(store.set_person("1", |prefs| prefs.volume = 80));
        assert_eq!(store.revision(), start + 2);
        assert!(!store.set_person("1", |prefs| prefs.volume = 80));
        assert_eq!(store.revision(), start + 2);
        store.restore(VoiceBackup::default());
        assert_eq!(store.revision(), start + 3);
    }
}

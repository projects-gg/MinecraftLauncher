//! What the launcher's page sees of voice chat and what it asks (the VOICE-L1 API; voice.md
//! "L1 API (VOICE-L3 için)").
//!
//! - [`VoiceState`]: the whole state, as `voice-state` events after every change and from
//!   `voice_state` at the page's start. Speaking rings change too often for that: they come as
//!   `voice-speaking` ([`VoiceSpeaking`]) while the launcher's window is on screen.
//! - [`VoiceNotice`]: moments worth a toast or a notification (`voice-notice`).
//! - [`VoiceAnswer`]: what an action got; refusals are answers, never errors.
//!
//! Every type is named `Voice…`: the page's bindings hold the whole launcher's types in one file.

use serde::{Deserialize, Serialize};

use crate::settings::VoiceKey;

/// Voice chat now, whole.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoiceState {
    /// Voice chat may be offered at all: the signed config, Studio's switch and the friends
    /// service agree, and the player is signed in. False: no voice chat anywhere (not even the
    /// settings page).
    pub available: bool,
    /// The player's own "Sesli sohbet" switch (Settings → Sesli sohbet). Off: no "Ara", no ring,
    /// nobody can call them.
    pub enabled: bool,
    /// The room the player is in, or is joining.
    pub room: Option<VoiceRoomView>,
    /// A call the player started that still rings at the friend (the title bar's "Aranıyor…" pill
    /// with "İptal"); the player already waits in the room.
    pub calling: Option<VoiceCalling>,
    /// Calls and invites ringing for the player now (the incoming card: "Katıl" / "Reddet"),
    /// oldest first. Do-not-disturb rings never show here: they are missed calls at once.
    pub rings: Vec<VoiceRing>,
    /// Missed calls ("Cevapsız arama"): "Katıl" while `joinable`, else "Geri ara". Newest first,
    /// at most 10; gone after "Geri ara", "Katıl" or `voice_dismiss_missed`.
    pub missed: Vec<VoiceMissed>,
    /// The live rooms of the player's crews (the crew page's "Sesli"); a crew without any has no
    /// entry (show only "Genel" with "Katıl").
    pub crews: Vec<VoiceCrewRooms>,
    /// How many rooms a crew may have, "Genel" included ("Yeni oda" hides when a crew has them).
    pub crew_rooms_max: u16,
    /// The microphone as the voice engine last saw it.
    pub mic: VoiceMic,
    /// Input and output devices (only known while the voice engine runs: in a room, or while the
    /// settings page is open).
    pub devices: VoiceDevices,
    /// "Mikrofonu dene" runs: the player hears themselves.
    pub testing: bool,
    /// People in the room whom the room named with their card (VOICE-S2): the page names someone
    /// its friend list and crews do not know from here (a friend's friend), never "Bir oyuncu".
    pub profiles: Vec<VoiceProfile>,
    /// Crewmates in a live room of the player's crews (`in`), from the crews' room lists, so a
    /// crewmate who is no friend shows "Sesli sohbette" too. Friends' own (`FriendView.voice`)
    /// come first; the player is never here.
    pub presence: std::collections::BTreeMap<String, pl_social::PresenceVoice>,
    /// The person volume slider's top (percent): 200 while the voice page plays through WebAudio
    /// (and before it said), 100 through `<audio>` elements (`pl_voice::gain`).
    pub volume_max: u16,
    /// Grows whenever something the settings backup carries changes (the settings, a person's
    /// volume or "Benim için sustur", here or from the in-game overlay): the page uploads the
    /// backup again a while after it moves (`voice_backup`).
    pub backup_rev: u32,
    // VOICE-CALL ---------------------------------------------------------------------------------
    /// "Aramadan önce sor": "Sesli arama başlat" asks "{name} aransın mı?" first (the page and the
    /// in-game overlay alike).
    pub ask_before_call: bool,
    /// When someone else first shared the room with the player in this stay (Unix seconds, this
    /// computer's clock): a call's start, for its timer. None while the player has only been
    /// alone in it, and with no room; it holds through reconnects and people coming and going.
    pub together_since: Option<i64>,
}

impl VoiceState {
    /// Nothing at all: before the friends service and the signed config said anything.
    pub fn off() -> Self {
        Self {
            available: false,
            enabled: true,
            room: None,
            calling: None,
            rings: Vec::new(),
            missed: Vec::new(),
            crews: Vec::new(),
            crew_rooms_max: 3,
            mic: VoiceMic::Unknown,
            devices: VoiceDevices::default(),
            testing: false,
            profiles: Vec::new(),
            presence: std::collections::BTreeMap::new(),
            volume_max: crate::gain::Playout::WebAudio.max_percent(),
            backup_rev: 0,
            ask_before_call: true,
            together_since: None,
        }
    }
}

/// Someone in the room as their card names them (VOICE-S2's `card`, the friends list's own).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoiceProfile {
    pub id: String,
    /// `displayName`, else `username`.
    pub name: String,
    /// The picture choice, as the friends list's (`discord`, `preset:…`).
    pub avatar: String,
    /// Discord's picture when `avatar` is `discord`.
    pub avatar_url: Option<String>,
    /// Their profile badges (BADGES), the most important first.
    pub marks: Vec<String>,
}

impl VoiceProfile {
    /// `id` is the room's (the card says the same).
    pub fn of(id: &str, card: &pl_social::PublicProfile) -> Self {
        Self {
            id: id.to_owned(),
            name: card.name().to_owned(),
            avatar: card.avatar.clone(),
            avatar_url: card.discord_avatar_url(128),
            marks: card.shown_marks(),
        }
    }
}

/// A friend room (`friend`: a call that grows with invites) or a crew room (`crew`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum VoiceRoomKind {
    Friend,
    Crew,
}

/// Where the player's stay is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum VoicePhase {
    /// Getting in (the room's answer, the audio connection).
    Joining,
    /// In the room, audio flowing.
    Live,
    /// The connection dropped for more than 2 seconds: "Yeniden bağlanıyor…".
    Reconnecting,
}

/// A role in a room. Crew rooms follow the crew's roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum VoiceRole {
    Leader,
    Moderator,
    Member,
}

/// A connection's quality; the page shows a mark only when it is not `good`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum VoiceQuality {
    Good,
    Weak,
    Bad,
}

impl VoiceQuality {
    pub fn from_wire(q: u8) -> Self {
        match q {
            0 => Self::Good,
            1 => Self::Weak,
            _ => Self::Bad,
        }
    }

    pub fn wire(self) -> u8 {
        match self {
            Self::Good => 0,
            Self::Weak => 1,
            Self::Bad => 2,
        }
    }
}

/// The room the player is in.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoiceRoomView {
    pub id: String,
    pub kind: VoiceRoomKind,
    /// Crew rooms: the crew.
    pub crew: Option<String>,
    /// Crew rooms: always. Friend rooms: none until the leader names it; show the people's names
    /// ("Kaan_07, Ece_x").
    pub name: Option<String>,
    /// The crew's "Genel": never renamed, never closed.
    pub general: bool,
    /// People at most ("Oda dolu.").
    pub max: u16,
    pub phase: VoicePhase,
    pub me: VoiceMe,
    /// Everyone else, longest present first. People the player is blocked with never show.
    pub people: Vec<VoicePerson>,
    /// The player's own invites still ringing (the panel shows them with "İptal").
    pub ringing: Vec<VoiceOutRing>,
    /// What the player may do to the room itself.
    pub can_rename: bool,
    pub can_close: bool,
}

impl VoiceRoomView {
    /// The room's name as the tray and the in-game overlay say it (the page's own is `rules.ts`
    /// `shownRoomName`): the crew's "Genel" and the friends service's "Oda 2" in the player's
    /// language (`english`), a name its leader typed as typed; none for a friend room nobody named
    /// (the people's names stand in).
    pub fn shown_name(&self, english: bool) -> Option<String> {
        if self.general {
            return Some(if english { "General" } else { "Genel" }.to_owned());
        }
        self.name
            .as_deref()
            .map(|name| shown_room_name(name, english))
    }
}

/// The number of a crew's room as the friends service names a new one ("Oda 2", "Oda 3"; social
/// `voice-hub.ts` `crewNew`), or none for any other name: its leader's own words.
pub fn numbered_room(name: &str) -> Option<u16> {
    let digits = name.strip_prefix("Oda ")?;
    if digits.is_empty() || digits.len() > 3 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// A room's own name in the player's language: the service's "Oda 2" is "Room 2" in English, any
/// other name stays as named.
pub fn shown_room_name(name: &str, english: bool) -> String {
    match numbered_room(name) {
        Some(n) if english => format!("Room {n}"),
        Some(n) => format!("Oda {n}"),
        None => name.to_owned(),
    }
}

/// The player in the room.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoiceMe {
    pub id: String,
    pub role: VoiceRole,
    /// "Mikrofonu kapat".
    pub mute: bool,
    /// "Sağırlaştır" (also keeps the microphone closed).
    pub deaf: bool,
    /// A moderator (or Projects' staff) muted the player for everyone ("Bir moderatör seni
    /// susturdu.").
    pub muted_by_moderator: bool,
    /// The microphone sends now (push-to-talk held, or speech in "Konuşunca"; false while muted).
    pub talking: bool,
    pub quality: VoiceQuality,
}

/// Someone else in the room.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoicePerson {
    pub id: String,
    pub role: VoiceRole,
    pub mute: bool,
    pub deaf: bool,
    /// Muted for everyone by a leader or moderator: nobody hears them.
    pub muted_by_moderator: bool,
    pub quality: VoiceQuality,
    /// Speaks now (as of this state; `voice-speaking` brings the changes).
    pub speaking: bool,
    /// Their volume for the player (percent, 0–200, 100 as they arrive; an engine that plays
    /// through `<audio>` stops at 100, `pl_voice::gain`); remembered on this computer.
    pub volume: u16,
    /// "Sustur (yalnız sen)": the player does not hear them; remembered on this computer.
    pub muted_for_me: bool,
    /// When they came in (Unix seconds).
    pub joined_at: i64,
    /// What the player may do to them (the person menu shows only these).
    pub can: VoicePowers,
}

/// A leader's or moderator's actions on one person.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoicePowers {
    /// "Herkes için sustur" / its undo.
    pub mute: bool,
    /// "Odadan çıkar".
    pub kick: bool,
    /// "Taşı" (crew rooms with another live room).
    pub move_to: bool,
    /// "Moderatör yap" / "Moderatörlüğü al" (friend rooms, the leader).
    pub moderator: bool,
}

/// The player's invite that still rings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoiceOutRing {
    pub to: String,
    /// Unix seconds.
    pub until: i64,
}

/// The call the player started.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoiceCalling {
    pub friend: String,
    /// Unix seconds: the ring stops here ("Cevap yok").
    pub until: i64,
}

/// A call or invite ringing for the player.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoiceRing {
    pub room: String,
    pub from: String,
    /// Others in the room (at most 4): the card's small faces.
    pub with: Vec<String>,
    /// Unix seconds.
    pub until: i64,
}

/// A missed call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoiceMissed {
    pub room: String,
    pub from: String,
    /// Unix seconds.
    pub at: i64,
    /// The room still lives and its ticket holds: "Katıl"; otherwise "Geri ara".
    pub joinable: bool,
}

/// One crew's live rooms.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoiceCrewRooms {
    pub crew: String,
    pub rooms: Vec<VoiceCrewRoom>,
    /// "Yeni oda" may show (fewer rooms than the crew may have).
    pub can_add: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoiceCrewRoom {
    pub id: String,
    pub name: String,
    pub general: bool,
    /// Who is in it now (blocked people left out).
    pub people: Vec<String>,
}

/// The microphone as the voice engine saw it last.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum VoiceMic {
    /// The engine has not looked yet.
    Unknown,
    Ok,
    /// Windows or the player refused it: "Mikrofona izin verilmedi." with "Ayarları aç"
    /// (`voice_open_mic_settings`); the player listens only.
    Denied,
    /// No microphone: "Mikrofon bulunamadı."; the player listens only.
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoiceDevices {
    pub inputs: Vec<VoiceDevice>,
    pub outputs: Vec<VoiceDevice>,
}

/// An audio device; `id` is what the settings keep (none there: the system's default).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoiceDevice {
    pub id: String,
    pub label: String,
}

/// Who speaks now (`voice-speaking`; at most ten a second, only while the window is on screen).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoiceSpeaking {
    /// People in the room who speak now.
    pub ids: Vec<String>,
    /// The player's microphone sends now.
    pub me: bool,
}

/// The microphone's level for the settings page's bar (`voice-level`, while it is open).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoiceLevel {
    /// 0 to 1.
    pub level: f32,
    /// Above the sensitivity: this would be sent.
    pub open: bool,
}

/// The live meters (`voice-levels`, VOICE-CALL): the player's microphone beside its button and
/// how loud each person speaks, on the settings bar's 0–1 scale. Only while the page asks
/// (`voice_levels`) and the main window is on screen, at most sixteen a second, only on change.
#[derive(Debug, Clone, PartialEq, Default, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoiceLevels {
    /// The player's microphone (0 while it is off).
    pub me: f32,
    /// What it picks up goes out now; else it is heard but not sent (under the sensitivity, alone
    /// in the room, push-to-talk up).
    pub open: bool,
    /// People in the room who make sound now (the quiet are left out).
    pub people: Vec<VoicePersonLevel>,
}

/// One person's level in [`VoiceLevels`].
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoicePersonLevel {
    pub id: String,
    /// 0 to 1.
    pub level: f32,
}

/// Why the player's stay ended (in [`VoiceNotice::Left`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum VoiceLeftReason {
    /// The leader closed the room.
    Closed,
    /// Nobody else was in it for a while: "Odada kimse kalmadı."
    Alone,
    /// The friend the player called did not join: "Cevap yok" with "Tekrar ara".
    NoAnswer,
    /// The player joined a room on another device: "Başka bir cihazda katıldın."
    Elsewhere,
    /// "Odadan çıkarıldın."
    Kicked,
    /// The player is no longer in that crew.
    Crew,
    /// No input and no speech for a long while: "Uzun süre hareketsiz kaldığın için ayrıldın."
    Idle,
    /// Voice chat was switched off, the month's share is used up, Projects' moderators closed the
    /// room, or the account cannot use voice chat now: "Sesli sohbet şu an kullanılamıyor."
    Unavailable,
    /// The connection did not come back: "Bağlanamadı." with "Tekrar dene" (`voice_retry`).
    Failed,
}

/// A moment worth a toast (`voice-notice`). The launcher shows the Windows notification itself
/// for rings, missed calls and removals while its window is away; the page shows the rest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum VoiceNotice {
    /// The stay ended for a reason the player did not choose.
    Left {
        reason: VoiceLeftReason,
        /// `noAnswer`: the friend who did not join (for "Tekrar ara").
        friend: Option<String>,
    },
    /// A leader or moderator moved the player to another room of the crew.
    Moved { room: String, name: String },
    /// "Bir moderatör seni susturdu."
    MutedByModerator,
    /// A missed call ("Cevapsız arama").
    Missed { from: String, room: String },
    /// The microphone changed (unplugged): "Mikrofon değişti."
    MicChanged,
    /// The microphone cannot be used: "Mikrofona izin verilmedi." / "Mikrofon bulunamadı.".
    Mic { mic: VoiceMic },
}

/// Why an action was refused. The page maps each to its own words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum VoiceRefusal {
    /// Voice chat is off (Studio, the month's share, not signed in): "Sesli sohbet şu an
    /// kullanılamıyor."
    Off,
    /// The friend cannot be called now (offline, voice chat off, or anything they need not tell).
    Unreachable,
    /// Too many calls or invites (`retryAfter`, `until`).
    RateLimited,
    /// A limited (new) account's smaller quota.
    AccountLimited,
    /// A staff mute runs (`until`): no calls, invites or renames.
    StaffMuted,
    /// "Oda dolu."
    RoomFull,
    /// The crew has as many rooms as it may.
    RoomsFull,
    /// That room ended, or the ring's ticket expired.
    RoomGone,
    /// Removed from that room and not allowed back yet.
    Kicked,
    /// Not a member of that crew (any more).
    NotMember,
    /// The player's role may not do that (or not to that person).
    NotAllowed,
    /// That person is not in the room.
    NotFound,
    /// A room name of 3–24 characters.
    InvalidName,
    /// The name filter refused it.
    NameRejected,
    /// The voice service did not answer: "Bağlanamadı." with "Tekrar dene".
    Failed,
    /// Nothing to do that on (not in a room, no such ring).
    Nothing,
}

/// What an action got: `refused` empty means it went through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoiceAnswer {
    pub refused: Option<VoiceRefusal>,
    /// Seconds to wait (`rateLimited`, `accountLimited`).
    pub retry_after: Option<u64>,
    /// Unix seconds (`accountLimited`, `staffMuted`).
    pub until: Option<i64>,
}

impl VoiceAnswer {
    pub const OK: Self = Self {
        refused: None,
        retry_after: None,
        until: None,
    };

    pub fn refused(why: VoiceRefusal) -> Self {
        Self {
            refused: Some(why),
            ..Self::OK
        }
    }

    pub fn is_ok(&self) -> bool {
        self.refused.is_none()
    }

    /// The service's error code as a refusal (`voice_off`, `friend_not_found`, …).
    pub fn from_code(code: &str, retry_after: Option<u64>, until: Option<i64>) -> Self {
        let why = match code {
            "voice_off" | "unauthorized" | "account_restricted" => VoiceRefusal::Off,
            "friend_not_found" => VoiceRefusal::Unreachable,
            "rate_limited" => VoiceRefusal::RateLimited,
            "account_limited" => VoiceRefusal::AccountLimited,
            "voice_muted" => VoiceRefusal::StaffMuted,
            "room_full" => VoiceRefusal::RoomFull,
            "voice_rooms_full" => VoiceRefusal::RoomsFull,
            "room_not_found" | "room_closed" | "invalid_ticket" => VoiceRefusal::RoomGone,
            "voice_kicked" => VoiceRefusal::Kicked,
            "team_not_found" | "not_member" => VoiceRefusal::NotMember,
            "not_allowed" => VoiceRefusal::NotAllowed,
            "not_found" => VoiceRefusal::NotFound,
            "invalid_name" => VoiceRefusal::InvalidName,
            "name_rejected" => VoiceRefusal::NameRejected,
            _ => VoiceRefusal::Failed,
        };
        Self {
            refused: Some(why),
            retry_after,
            until,
        }
    }
}

/// A leader's or moderator's action (`voice_moderate`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "act", rename_all = "camelCase")]
pub enum VoiceModAction {
    /// "Herkes için sustur".
    Mute {
        person: String,
    },
    Unmute {
        person: String,
    },
    /// "Odadan çıkar" (no question; the toast's "Geri al" is `unkick`).
    Kick {
        person: String,
    },
    Unkick {
        person: String,
    },
    /// "Taşı": to another live room of the crew.
    Move {
        person: String,
        room: String,
    },
    /// "Moderatör yap" (friend rooms, the leader).
    Mod {
        person: String,
    },
    Unmod {
        person: String,
    },
    /// The room's new name (3–24 characters).
    Rename {
        name: String,
    },
    /// "Odayı kapat" (the leader; the page asks first: "Herkes odadan çıkar.").
    Close,
}

/// What a key would clash with (`voice_check_key`), for the key picker's single line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum VoiceKeyClash {
    /// Simple Voice Chat in the game uses it (Caps Lock: its push-to-talk; V: its menu):
    /// "Bu tuşu oyundaki sesli sohbet de kullanıyor."
    GameVoiceChat,
    /// The in-game overlay's keys (F7 panel, F9 emojis) and the pack's mods (F6 free look, C zoom,
    /// X gamma).
    Overlay,
    /// One of the game's own keys (moving, jumping, the inventory, the chat, the hotbar…). The
    /// key still reaches the game.
    Game,
    /// Left Alt: Alt+Shift switches the keyboard's language.
    LanguageSwitch,
    /// Another voice shortcut has it already.
    Taken,
    /// Mute or deafen on a key that writes (a letter, a digit, Space…) without Ctrl or Alt: it
    /// fires while the player types too (VOICE-KEYS; never refused).
    Typing,
}

/// The answer to `voice_check_key`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoiceKeyCheck {
    /// The key can be watched (a keyboard key the launcher knows, or a mouse button).
    pub usable: bool,
    pub clash: Option<VoiceKeyClash>,
}

/// Which shortcut a key is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum VoiceShortcut {
    /// "Bas-konuş".
    PushToTalk,
    /// "Mikrofonu kapat/aç".
    Mute,
    /// "Sağırlaştır".
    Deafen,
}

/// The input that a key check is about: the key and the shortcut it would be.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoiceKeyAsk {
    pub key: VoiceKey,
    pub shortcut: VoiceShortcut,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn answers_map_the_service_codes() {
        assert_eq!(
            VoiceAnswer::from_code("friend_not_found", None, None).refused,
            Some(VoiceRefusal::Unreachable)
        );
        let limited = VoiceAnswer::from_code("account_limited", Some(60), Some(1_700_000_000));
        assert_eq!(limited.refused, Some(VoiceRefusal::AccountLimited));
        assert_eq!(limited.retry_after, Some(60));
        assert_eq!(limited.until, Some(1_700_000_000));
        assert_eq!(
            VoiceAnswer::from_code("sfu_failed", None, None).refused,
            Some(VoiceRefusal::Failed)
        );
        assert_eq!(
            VoiceAnswer::from_code("something_new", None, None).refused,
            Some(VoiceRefusal::Failed)
        );
        assert!(VoiceAnswer::OK.is_ok());
    }

    #[test]
    fn written_as_the_page_reads_them() {
        assert_eq!(
            serde_json::to_value(VoiceNotice::Left {
                reason: VoiceLeftReason::NoAnswer,
                friend: Some("u1".into())
            })
            .unwrap(),
            json!({"kind":"left","reason":"noAnswer","friend":"u1"})
        );
        assert_eq!(
            serde_json::to_value(VoiceNotice::MutedByModerator).unwrap(),
            json!({"kind":"mutedByModerator"})
        );
        assert_eq!(
            serde_json::to_value(VoiceAnswer::refused(VoiceRefusal::RoomFull)).unwrap(),
            json!({"refused":"roomFull","retryAfter":null,"until":null})
        );
        let action: VoiceModAction =
            serde_json::from_value(json!({"act":"move","person":"u2","room":"c0123456789abcdef"}))
                .unwrap();
        assert_eq!(
            action,
            VoiceModAction::Move {
                person: "u2".into(),
                room: "c0123456789abcdef".into()
            }
        );
        let close: VoiceModAction = serde_json::from_value(json!({"act":"close"})).unwrap();
        assert_eq!(close, VoiceModAction::Close);
        assert_eq!(VoiceQuality::from_wire(1), VoiceQuality::Weak);
        assert_eq!(VoiceQuality::from_wire(9), VoiceQuality::Bad);
    }

    #[test]
    fn rooms_are_named_in_the_players_language() {
        assert_eq!(numbered_room("Oda 2"), Some(2));
        assert_eq!(numbered_room("Oda 12"), Some(12));
        for typed in [
            "Oda",
            "Oda ",
            "Oda 2 ekibi",
            "Oda +2",
            "Oda 1234",
            "oda 2",
            "Taş Ocağı",
        ] {
            assert_eq!(numbered_room(typed), None, "{typed}");
        }
        assert_eq!(shown_room_name("Oda 3", true), "Room 3");
        assert_eq!(shown_room_name("Oda 3", false), "Oda 3");
        assert_eq!(shown_room_name("Taş Ocağı", true), "Taş Ocağı");

        let room = |name: Option<&str>, general: bool| VoiceRoomView {
            id: "c0123456789abcdef".into(),
            kind: VoiceRoomKind::Crew,
            crew: Some("t1".into()),
            name: name.map(str::to_owned),
            general,
            max: 10,
            phase: VoicePhase::Live,
            me: VoiceMe {
                id: "u1".into(),
                role: VoiceRole::Member,
                mute: false,
                deaf: false,
                muted_by_moderator: false,
                talking: false,
                quality: VoiceQuality::Good,
            },
            people: Vec::new(),
            ringing: Vec::new(),
            can_rename: false,
            can_close: false,
        };
        assert_eq!(
            room(Some("Genel"), true).shown_name(true).as_deref(),
            Some("General")
        );
        assert_eq!(
            room(Some("Genel"), true).shown_name(false).as_deref(),
            Some("Genel")
        );
        assert_eq!(
            room(Some("Oda 2"), false).shown_name(true).as_deref(),
            Some("Room 2")
        );
        assert_eq!(
            room(Some("Gece Ekibi"), false).shown_name(true).as_deref(),
            Some("Gece Ekibi")
        );
        assert_eq!(room(None, false).shown_name(true), None);
    }
}

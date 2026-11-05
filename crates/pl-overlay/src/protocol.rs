//! Bridge frames, version 1. One JSON object per line; `t` names the frame. Both sides ignore unknown
//! frames and fields, so additions keep the version and only breaking changes bump [`PROTO`]. Every
//! addition is recorded in `docs/tasks/ovl2-protocol.md` (shared with the mod's side).

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

/// Protocol version this launcher speaks.
pub const PROTO: u32 = 1;
/// Longest frame the mod may send (player actions), in bytes without the newline.
pub const MAX_GAME_FRAME: usize = 16 * 1024;
/// Longest frame the launcher sends (a snapshot or a conversation history).
pub const MAX_LAUNCHER_FRAME: usize = 256 * 1024;
/// Longest chat message, the social service's rule.
pub const MAX_MESSAGE_CHARS: usize = 300;
/// Messages a `history` frame carries at most (the newest).
pub const MAX_HISTORY: usize = 100;

/// A value that never shows up in `Debug` output or logs.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

/// Frames from the mod.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "t", rename_all = "kebab-case")]
pub enum FromGame {
    /// First frame of every connection.
    Hello {
        proto: u32,
        token: Secret,
        #[serde(rename = "mod", default)]
        mod_version: String,
        #[serde(default)]
        mc: String,
        #[serde(default)]
        lang: String,
        /// What this mod can do beyond protocol 1 (`connect`: it switches servers on [`ToGame::Connect`]).
        #[serde(default)]
        features: Vec<String>,
    },
    /// The player opened or closed the panel. A mod also says `false` when the open panel leaves
    /// its chat page (no conversation is on screen; nothing it gets counts as read) until the next
    /// `open-conv` (GAME-BATCH).
    Panel { open: bool },
    /// The panel shows this conversation: send its history and count it as read.
    OpenConv { conv: String },
    /// The player sent a chat message; answer with `sent` or `send-failed` for `cid`.
    Send {
        cid: String,
        conv: String,
        /// Empty for a sticker.
        #[serde(default)]
        text: String,
        /// CHAT2: the message it answers, by its `sid`.
        #[serde(default)]
        reply: Option<i64>,
        /// CHAT2: a Projects sticker (`pl1/gg`) instead of text.
        #[serde(default)]
        sticker: Option<String>,
    },
    /// CHAT2: the player gives (`on`) or takes back their reaction `e` on message `sid`. The message
    /// comes back with its reactions; a refusal is `react-failed`.
    React {
        conv: String,
        sid: i64,
        e: String,
        on: bool,
    },
    /// CHAT2: the player writes in the panel's field ("yazıyor…"; the mod sends it at most every
    /// 4 s, only while the field has text).
    Typing { conv: String },
    /// The player answered a crew call.
    Answer { call: String, answer: CallAnswer },
    /// The player pressed "Join" on a friend (the mod connects after its own confirmation).
    Join { friend: String },
    /// The answer to [`ToGame::Connect`]: the game is on its way to the server, or why not (`busy`,
    /// `refused`, `unsupported`, …).
    ConnectResult {
        id: String,
        ok: bool,
        #[serde(default)]
        reason: String,
    },
    /// HOST1 (H3; from a mod whose hello lists `host`): the player entered (`open`) or left a
    /// singleplayer world. `folder` is its folder in `saves/`, `mode` the world's game mode, `lan`
    /// whether vanilla "LAN'a Aç" is on.
    HostWorld {
        open: bool,
        #[serde(default)]
        folder: String,
        #[serde(default)]
        mode: String,
        #[serde(default)]
        lan: bool,
    },
    /// HOST1 (H4): "Davet et" in the game: the mode guests play in, whether they may use commands,
    /// the friends to invite.
    HostRequest {
        #[serde(default)]
        mode: String,
        #[serde(default)]
        commands: bool,
        #[serde(default)]
        invite: Vec<String>,
    },
    /// HOST1 (H6): the answer to [`ToGame::HostPublish`]: the world's port on `127.0.0.1` and the
    /// game's protocol number, or why not (`busy`, `lan`, `not-world`, `error`).
    HostPublished {
        id: String,
        ok: bool,
        #[serde(default)]
        port: u16,
        #[serde(default)]
        proto: i32,
        #[serde(default)]
        reason: String,
    },
    /// HOST1 (H7): a guest got in (`in`), left (`out`) or was turned away at the gate (`refused`).
    HostGuest {
        #[serde(default)]
        conn: u64,
        #[serde(default)]
        guest: String,
        #[serde(default)]
        name: String,
        state: String,
        #[serde(default)]
        reason: String,
    },
    /// HOST1 (H9): more invites from the open world.
    HostInvite {
        #[serde(default)]
        invite: Vec<String>,
    },
    /// HOST1 (H10): the player pressed "Dünyadan çıkar" in the game.
    HostRemove { guest: String },
    /// HOST1 (H11): the player stopped sharing, or is leaving the world.
    HostStop {
        #[serde(default)]
        reason: String,
    },
    /// HOST1-M (H17): "Projects'te buluşun" in the game: these friends get a "Birlikte oyna" invite once the
    /// player is on Projects (the game goes there itself, after the player's confirmation in the game).
    HostMeet {
        #[serde(default)]
        invite: Vec<String>,
    },
    /// SKIN1-O (K3; from a mod whose hello lists `link`): a Projects server asked the game on
    /// `projects:link` whether its launcher vouches for the player; `name` is the game's own
    /// account name. The launcher answers with `link-offer` (a permit for that account) or
    /// `link-clear` (none).
    LinkWant {
        #[serde(default)]
        name: String,
    },
    /// OVL-UX (P3; the friend menu): "Birlikte oyna" to this friend. Answered with
    /// `friend-result`.
    Invite { friend: String },
    /// OVL-UX (P4): "Discord'dan mesaj at": the launcher opens the friend's Discord profile (in
    /// the app when it is installed, else on the web). Answered only when it could not.
    Discord { friend: String },
    /// OVL-UX (P5): "Arkadaşlıktan çıkar", after the mod's one-sentence question.
    Unfriend { friend: String },
    /// OVL-UX (P6): "Engelle", after the mod's question.
    Block { friend: String },
    /// OVL-UX (P7): "Rapor et": one of a player's reasons (`name`, `profile`, `harassment`,
    /// `spam`, `scam`, `other`), a note for moderators (200 characters at most, may be empty) and
    /// whether to block them too.
    Report {
        friend: String,
        reason: String,
        #[serde(default)]
        note: String,
        #[serde(default)]
        block: bool,
    },
    /// OVL-UX2 (Q8; from a mod whose hello lists `people2`): "Favorilere ekle" (`on`) or
    /// "Favorilerden çıkar". The friend's next frame carries it (`fav`).
    Favorite { friend: String, on: bool },
    /// OVL-UX2 (Q9): "Çevrimiçi olunca bildir" on or off for this friend (`alert`).
    Alert { friend: String, on: bool },
    /// OVL-UX2 (Q10): "Kategoriye taşı": one of the player's categories by id, empty for the
    /// implicit "Arkadaşlar". Answered with `friend-result`. OVL-UX3 (U5; from a mod whose hello
    /// lists `people3`): `group` empty and a `name`: "Yeni kategori…", a new category of that name
    /// at the end of the list, with the friend in it.
    Move {
        friend: String,
        #[serde(default)]
        group: String,
        #[serde(default)]
        name: String,
    },
    /// OVL-UX2 (Q11): "Ekibe davet et": to one of the crews `lists` names. Answered with
    /// `friend-result`.
    CrewInvite { friend: String, crew: String },
    /// OVL-UX2 (Q12): "Arkadaş ekle" (or "İsteği kabul et" when they asked first) on a crewmate who
    /// is not a friend yet. Answered with `friend-result` (`queued`: the request waits for them).
    AddFriend { friend: String },
    /// OVL-UX3 (U2; from a mod whose hello lists `people3`): "Moderatör yap" (`on`) or
    /// "Moderatörlükten al" on a crewmate of a crew the player leads. Answered with
    /// `friend-result`.
    CrewMod {
        crew: String,
        friend: String,
        on: bool,
    },
    /// OVL-UX3 (U3): "Liderliği devret" to a crewmate of a crew the player leads.
    CrewLead { crew: String, friend: String },
    /// OVL-UX3 (U4): "Ekipten çıkar": anyone out of a crew the player leads, or a plain member
    /// out of a crew whose moderators may remove members, by one of them.
    CrewRemove { crew: String, friend: String },
    /// OVL-LANG (G4; from a mod whose hello lists `lang`): the player picked the overlay's language
    /// in its settings ("Dil": `tr` or `en`). The launcher keeps it for every game from now on and
    /// answers with a new `welcome` in that language; anything else is ignored.
    Lang {
        #[serde(default)]
        lang: String,
    },
    /// CHAT-EMOJI (E4; from a mod whose hello lists `emoji`): the player switched the chat's emoji
    /// picker in the overlay's settings. The launcher keeps it ("Sohbette emoji seçici", the same
    /// switch as its own Settings) and answers with `emojis`.
    Emoji { on: bool },
    /// VOICE-O (V6; from a mod whose hello lists `voice`): "Mikrofon" (`mute`) or "Sağırlaştır"
    /// (`deaf`) in the panel, the one pressed. Opening the microphone ends deafening too (the
    /// launcher's rule); the next `voice` says how it stands.
    VoiceSet {
        #[serde(default)]
        mute: Option<bool>,
        #[serde(default)]
        deaf: Option<bool>,
    },
    /// VOICE-O (V7): "Ayrıl", and "İptal" while the player's call rings.
    VoiceLeave {},
    /// VOICE-O (V8): "Katıl" (`join`) or "Reddet" on a ring the last `voice` listed.
    VoiceAnswer {
        #[serde(default)]
        room: String,
        #[serde(default)]
        join: bool,
    },
    /// VOICE-O (V9): "Ses seviyesi" of someone in the player's room, in percent (0–200; the
    /// launcher holds a value outside to it).
    VoiceVol {
        #[serde(default)]
        person: String,
        #[serde(default)]
        percent: i64,
    },
    /// VOICE-O (V10): "Benim için sustur" (`on`) or its undo, for someone in the player's room.
    VoiceLmute {
        #[serde(default)]
        person: String,
        #[serde(default)]
        on: bool,
    },
    /// VOICE-O (V11): a leader's or moderator's row on someone in the room: `mute` / `unmute`
    /// ("Herkes için sustur"), `kick` ("Odadan çıkar"), `unkick` (its "Geri al"), `mod` / `unmod`
    /// ("Moderatör yap", friend rooms). The room decides whether the player's role may.
    VoiceMod {
        #[serde(default)]
        person: String,
        #[serde(default)]
        act: String,
    },
    /// VOICE-O (V12): "Ara" on a friend.
    VoiceCall {
        #[serde(default)]
        friend: String,
    },
    /// VOICE-O (V13): "Odaya davet et": a friend into the player's friend room.
    VoiceInvite {
        #[serde(default)]
        friend: String,
    },
    /// OVL-SOC (V22; from a mod whose hello lists `voice` and `voice-rooms`): "Katıl" on one of the
    /// player's crew rooms in crew chat (`room`: its id from the last `voice-crews`, the crew's
    /// "Genel" by its own id too), or "Yeni oda" (`new`: the player opens one and joins it).
    VoiceJoin {
        #[serde(default)]
        crew: String,
        #[serde(default)]
        room: String,
        #[serde(default)]
        new: bool,
    },
    /// OVL-SOC 2 (from a mod whose hello lists `people4`): the status switcher in the panel's
    /// header picked `status` (`online`, `dnd` or `invisible`); with SOC-NEXT on (`next` among
    /// the welcome's features) `dnd` and `invisible` come `for` one of the crew mute's durations
    /// (`hour`, `eightHours`, `morning`, `forever`). The launcher's page changes it as its own
    /// menu does; the next `dnd` frame says how it stands.
    Status {
        #[serde(default)]
        status: String,
        #[serde(rename = "for", default)]
        preset: Option<String>,
    },
    /// OVL-SOC 2 (`people4`): "Kabul et" (`accept`) or "Reddet" on a friend request the last
    /// `lists` named. Answered with `friend-result` (`accept` or `decline`).
    Request {
        #[serde(default)]
        id: String,
        #[serde(default)]
        accept: bool,
    },
    /// OVL-AVATARS (from a mod whose hello lists `avatars`): the people whose faces it draws now and holds no
    /// picture for (first seen, let go of, or one that could not be had a while ago), by user id (at most 32 a
    /// frame; `@me` is the player). Each is answered with `avatar`; a change comes by itself later.
    AvatarWant {
        #[serde(default)]
        ids: Vec<String>,
    },
    // --- OVL-SOC2: social management in game (`docs/tasks/ovl2-protocol.md` → "OVL-SOC2") ---------
    /// `people5`: "İsteği geri çek" on a friend request the player sent (`lists`' `outgoing`).
    /// Answered with `done` (`cancel-request`).
    CancelRequest {
        #[serde(default)]
        id: String,
    },
    /// `people5`: "Engeli kaldır" on someone in `lists`' `blocked`. Answered with `done`
    /// (`unblock`).
    Unblock {
        #[serde(default)]
        id: String,
    },
    /// `people5` or `crews2`: "Bul" in the panel's "Ekle": a friend code, a crew code or a crew
    /// invite link, as typed (the launcher tells which). Answered with `found`.
    Find {
        #[serde(default)]
        code: String,
    },
    /// `people5`: "İstek gönder" to the friend code a `found` named. Answered with `done`
    /// (`add-code`; `queued` while the request waits for them).
    AddCode {
        #[serde(default)]
        code: String,
    },
    /// `crews2`: "Katıl" (`accept`) or "Reddet" on an invite to a crew (`lists`' `invites`).
    /// Answered with `done` (`crew-answer`).
    CrewAnswer {
        #[serde(default)]
        crew: String,
        #[serde(default)]
        accept: bool,
    },
    /// `crews2`: "Kabul et" (`accept`) or "Reddet" on someone's request to join one of the
    /// player's crews (`lists`' `asks`). Answered with `done` (`crew-ask`).
    CrewAsk {
        #[serde(default)]
        crew: String,
        #[serde(default)]
        id: String,
        #[serde(default)]
        accept: bool,
    },
    /// `crews2`: "Ekibe katıl" or "Katılma isteği gönder" to the crew code or invite link a
    /// `found` named. Answered with `done` (`crew-join`; `queued` while the request waits).
    CrewJoin {
        #[serde(default)]
        code: String,
    },
    /// `crews2`: "Ekibi çağır" for one of `lists`' `callers`. Answered with `done` (`crew-call`).
    CrewCall {
        #[serde(default)]
        crew: String,
    },
    /// `plans`: "Geliyorum" (`yes`), "Belki" (`maybe`) or "Gelemem" (`no`) to a plan of `lists`'
    /// `plans`. Answered with `done` (`plan-answer`).
    PlanAnswer {
        #[serde(default)]
        plan: String,
        #[serde(default)]
        answer: String,
    },
    /// `plans`: a new plan in one of `lists`' `planners` at `at` (Unix seconds, a whole quarter
    /// of an hour, 5 minutes to 7 days ahead) with its note (at most 80 characters; only while
    /// `lists`' `notes`). Answered with `done` (`plan-make`).
    PlanMake {
        #[serde(default)]
        crew: String,
        #[serde(default)]
        at: i64,
        #[serde(default)]
        note: String,
    },
    /// `chat3`: the player edits their own message `sid` (15 minutes; never a sticker). Answered
    /// with `done` (`edit`); the new text comes as the message itself.
    Edit {
        #[serde(default)]
        conv: String,
        #[serde(default)]
        sid: i64,
        #[serde(default)]
        text: String,
    },
    /// `chat3`: the player deletes their own message `sid` for everyone (7 days). Answered with
    /// `done` (`delete`); the tombstone comes as the message itself.
    Delete {
        #[serde(default)]
        conv: String,
        #[serde(default)]
        sid: i64,
    },
    /// `cards2`: the card kinds the player turned off in the overlay's settings (`message`,
    /// `call`, `invite`, `request`, `online`, `plan`, `missed`): the launcher sends none of them
    /// to this game and keeps its own notifications for them. After the hello and on every change.
    Cards {
        #[serde(default)]
        off: Vec<String>,
    },
    /// `cards2`: one of the launcher's notification switches changed in the overlay's settings
    /// (`favoritesOnline`, `favoritesProjects`, `crewCalls`, `crewChat`, `joinRequests`). The
    /// launcher's page keeps it, as its own switch would; the next `prefs` says how it stands.
    NotifySet {
        #[serde(default)]
        key: String,
        #[serde(default)]
        on: bool,
    },
    /// Anything newer than this launcher.
    #[serde(other)]
    Unknown,
}

/// The `hello` feature of a mod that switches servers on [`ToGame::Connect`].
pub const FEATURE_CONNECT: &str = "connect";
/// The `hello` feature of a mod that knows CHAT2's frames and fields (`docs/tasks/ovl2-protocol.md`,
/// CHAT2-O): it hears "yazıyor…" and the precise reasons, and its panel asks for link cards.
pub const FEATURE_CHAT2: &str = "chat2";

/// The `hello` feature of a mod that hosts worlds (HOST1, H1), and the `welcome` feature that says
/// this player may open their world now (H2).
pub const FEATURE_HOST: &str = "host";

/// The `hello` feature of a mod that draws crew tags (TEAM6-O, T1: it reads `badge` and hears the
/// `badges` and `note` frames), and the `welcome` feature that says crew tags are on now (T2: the
/// service offers them and Studio has them on).
pub const FEATURE_TAGS: &str = "tags";

/// The `hello` feature of a mod that draws profile badges (BADGES-UI): it hears the `marks` frame
/// (the badge catalogue: each badge's key, look and names in both languages) and draws the first
/// badge it knows of a friend's, a sender's or a toast's `marks` right of the name, its tooltip
/// naming them all.
pub const FEATURE_MARKS: &str = "marks";

/// The `hello` feature of a mod that carries the launcher's codeless linking permit to a Projects
/// server and draws its question (SKIN1-O, K1), and the `welcome` feature that says this launcher
/// can vouch for the player now (K2: signed in to friends, the service offers codeless linking,
/// the game was started from here).
pub const FEATURE_LINK: &str = "link";

/// The `hello` feature of a mod with the friend menu (OVL-UX, P1: it hears `friend-result`), and
/// the `welcome` feature that says this launcher takes the menu's frames (`invite`, `discord`,
/// `unfriend`, `block`, `report`; P2: signed in to friends).
pub const FEATURE_PEOPLE: &str = "people";

/// The `welcome` feature that says "Birlikte oyna" can go now (OVL-UX, P2): the player's game is
/// on Projects, or on a server they show their friends.
pub const FEATURE_INVITE: &str = "invite";

/// The `hello` feature of a mod with the launcher's whole person menu (OVL-UX2, Q1: it reads the
/// friends' `fav`, `alert`, `group` and `crews`, hears `lists` and sends `favorite`, `alert`,
/// `move`, `crew-invite` and `add-friend`), and the `welcome` feature that says this launcher takes
/// those frames (signed in to friends).
pub const FEATURE_PEOPLE2: &str = "people2";

/// The `hello` feature of a mod with OVL-UX3's rows (U1: it reads `roles` in `lists`, sends
/// `crew-mod`, `crew-lead`, `crew-remove`, and `move` with a new category's `name`), and the
/// `welcome` feature that says this launcher takes those frames (signed in to friends).
pub const FEATURE_PEOPLE3: &str = "people3";

/// The `hello` feature of a mod whose words follow the launcher (OVL-LANG, G1): it reads `lang` in
/// `welcome`, shows the "Dil" row in its settings and sends `lang` when the player picks there.
pub const FEATURE_LANG: &str = "lang";

/// The `hello` feature of a mod with the chat's emoji picker (CHAT-EMOJI, E1): it hears `emojis`
/// and sends `emoji` when the player switches the picker in its settings.
pub const FEATURE_EMOJI: &str = "emoji";

/// The `hello` feature of a mod that runs the emoji picker alone (CHAT-EMOJI, E2: started with
/// `-Dprojects.overlay.panel=off`): it has no panel and shows no toasts, so the launcher does not
/// count it as connected (its own notifications stay) and sends it only `welcome` and `emojis`.
pub const FEATURE_PANEL_OFF: &str = "panel-off";

/// The `hello` feature of a mod that knows crew chat's slow mode (CHAT-CALM): it reads `slow` on
/// conversations, counts the wait down in its composer and hears `slow` as a reason a message
/// did not go (an older mod hears `rate`).
pub const FEATURE_SLOW: &str = "slow";

/// The `hello` feature of a mod with voice chat's in-game parts (VOICE-O, V1): it hears `voice`,
/// `voice-speaking`, `voice-notice` and `voice-result`, reads friends' `voice`, and sends the
/// `voice-*` presses. A mod without it hears none of them and its `voice-*` frames are dropped.
pub const FEATURE_VOICE: &str = "voice";

/// The `hello` feature of a mod that shows the player's crew voice rooms in crew chat (OVL-SOC,
/// V21: beside `voice` it hears `voice-crews` and sends `voice-join`). A mod without it hears no
/// `voice-crews` and its `voice-join` is dropped.
pub const FEATURE_VOICE_ROOMS: &str = "voice-rooms";

/// OVL-SOC 2: the `hello` feature of a mod with the friend requests (`lists`' `requests`, the
/// `request` frame), the status switcher (`status`; `status` and `until` in `welcome` and `dnd`)
/// and SOC-NEXT's marks (a friend's `real`, a message's `ignored` and `mention`); the `welcome`
/// feature that says this launcher takes `request` and `status` (signed in to friends).
pub const FEATURE_PEOPLE4: &str = "people4";

/// OVL-AVATARS: the `hello` feature of a mod that draws people's pictures on their faces: it sends `avatar-want`
/// for the people it draws and hears `avatar`.
pub const FEATURE_AVATARS: &str = "avatars";

/// OVL-SOC 2: the `welcome` feature that says SOC-NEXT is on (Studio's `features.socNext`): the
/// status switcher offers its durations. Only to a mod that said `people4`.
pub const FEATURE_NEXT: &str = "next";

/// OVL-SOC 2: the `welcome` feature that says streamer mode is on (SOC-NEXT §3.4): the mod shows
/// no server address (it still knows them, to join). Only to a mod that said `people4`.
pub const FEATURE_STREAMER: &str = "streamer";

/// OVL-SOC2: the `hello` feature of a mod with the friends page's whole management: the requests
/// the player sent (`lists`' `outgoing`, `cancel-request`), "Ekle" with a code (`find`, `found`,
/// `add-code`), the player's own code (`lists`' `code`) and the blocked (`lists`' `blocked`,
/// `unblock`); the `welcome` feature that says this launcher takes those frames (signed in).
pub const FEATURE_PEOPLE5: &str = "people5";
/// OVL-SOC2: the `hello` feature of a mod with crews in game: invites and join requests answered
/// (`lists`' `invites` and `asks`, `crew-answer`, `crew-ask`), joining by code or link (`find`,
/// `crew-join`) and "Ekibi çağır" (`lists`' `call` and `callers`, `crew-call`); the `welcome`
/// feature that says this launcher takes them (signed in, crews on).
pub const FEATURE_CREWS2: &str = "crews2";
/// OVL-SOC2: the `hello` feature of a mod that shows crew plans (`lists`' `plans`, `planners`,
/// `notes`; `plan-answer`, `plan-make`); the `welcome` feature that says the service has plans now.
pub const FEATURE_PLANS: &str = "plans";
/// OVL-SOC2: the `hello` feature of a mod that edits and deletes the player's own messages (`edit`,
/// `delete`); the `welcome` feature that says chat is on.
pub const FEATURE_CHAT3: &str = "chat3";
/// OVL-SOC2: the `hello` feature of a mod with the new cards (`request`, `friend`, `online`,
/// `projects`, `crew-invite`, `crew-ask`, `crew-in`, `plan`, `missed`), the player's card kinds
/// (`cards`) and the launcher's notification switches (`prefs`, `notify-set`); the `welcome`
/// feature that says this launcher takes `cards` and `notify-set` (signed in).
pub const FEATURE_CARDS2: &str = "cards2";

/// The card kinds the player may turn off in game (`cards`), and the toast kinds each covers.
pub const CARD_GROUPS: [(&str, &[ToastKind]); 7] = [
    ("message", &[ToastKind::Message]),
    ("call", &[ToastKind::Call]),
    (
        "invite",
        &[ToastKind::Invite, ToastKind::World, ToastKind::CrewInvite],
    ),
    (
        "request",
        &[
            ToastKind::Request,
            ToastKind::Friend,
            ToastKind::CrewAsk,
            ToastKind::CrewIn,
        ],
    ),
    ("online", &[ToastKind::Online, ToastKind::Projects]),
    ("plan", &[ToastKind::Plan]),
    ("missed", &[ToastKind::Missed]),
];

/// The group of a toast kind the player may turn off (`cards`); none for one that always shows.
pub fn card_group(kind: ToastKind) -> Option<&'static str> {
    CARD_GROUPS
        .iter()
        .find(|(_, kinds)| kinds.contains(&kind))
        .map(|(group, _)| *group)
}

impl FromGame {
    /// One of VOICE-O's presses: they go to the launcher's voice chat, never to the friends
    /// feature.
    pub fn is_voice(&self) -> bool {
        matches!(
            self,
            Self::VoiceSet { .. }
                | Self::VoiceLeave {}
                | Self::VoiceAnswer { .. }
                | Self::VoiceVol { .. }
                | Self::VoiceLmute { .. }
                | Self::VoiceMod { .. }
                | Self::VoiceCall { .. }
                | Self::VoiceInvite { .. }
                | Self::VoiceJoin { .. }
        )
    }

    /// One of HOST1's frames: they go to the launcher's world sharing, never to the friends
    /// feature.
    pub fn is_host(&self) -> bool {
        matches!(
            self,
            Self::HostWorld { .. }
                | Self::HostRequest { .. }
                | Self::HostPublished { .. }
                | Self::HostGuest { .. }
                | Self::HostInvite { .. }
                | Self::HostRemove { .. }
                | Self::HostStop { .. }
                | Self::HostMeet { .. }
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CallAnswer {
    Coming,
    Soon,
    Cant,
}

/// Frames to the mod.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "t", rename_all = "kebab-case")]
pub enum ToGame {
    /// The handshake's answer. Comes again (followed by a `snapshot`) when the account, the
    /// features, the reduced-motion choice or the overlay's language change mid-session.
    Welcome {
        proto: u32,
        launcher: String,
        me: Me,
        dnd: bool,
        #[serde(rename = "reducedMotion")]
        reduced_motion: bool,
        /// What works now: `friends` (signed in), `chat`, `calls`. Empty: not signed in.
        features: Vec<String>,
        /// OVL-LANG (G3): the language the overlay shows (`tr`, `en`): the player's pick in its
        /// settings, else the launcher's own. Everything the launcher words for this mod follows
        /// it. Only to a mod that said `lang`.
        #[serde(skip_serializing_if = "Option::is_none")]
        lang: Option<pl_lang::Lang>,
        /// OVL-SOC 2: the player's status when it is not "Çevrimiçi" (`dnd`, `invisible`), and
        /// with SOC-NEXT when a timed one ends (Unix ms). Only to a mod that said `people4`.
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<&'static str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        until: Option<i64>,
    },
    Snapshot {
        friends: Vec<FriendView>,
        convs: Vec<ConvView>,
        calls: Vec<CallView>,
        dnd: bool,
    },
    Friend(FriendView),
    FriendGone {
        id: String,
    },
    Conv(ConvView),
    /// A new message, or a known one again (same `id`) after an edit. (Boxed: a message with
    /// CHAT2's fields is the largest frame by far.)
    Msg(Box<MsgView>),
    /// A message left the conversation (deleted for everyone, or dropped here).
    MsgGone {
        conv: String,
        id: String,
    },
    /// The newest messages of a conversation, oldest first: the answer to `open-conv`, or an
    /// empty list after its history was cleared on this computer.
    History {
        conv: String,
        msgs: Vec<MsgView>,
    },
    Sent {
        cid: String,
        id: String,
        /// CHAT2: the service's id of the message (replies and reactions name it).
        #[serde(skip_serializing_if = "Option::is_none")]
        sid: Option<i64>,
    },
    SendFailed {
        cid: String,
        reason: SendFailure,
        /// CHAT2, with `link`: the link rule that refused it, when the service said.
        #[serde(skip_serializing_if = "Option::is_none")]
        rule: Option<String>,
    },
    /// CHAT2: someone writes in a conversation now (`by` an opaque person key, `name` as the lists
    /// show it). Shown for 6 s after the last; a message of theirs ends it. Sent only while the panel
    /// is open, and only to a mod that said `chat2`.
    Typing {
        conv: String,
        by: String,
        name: String,
    },
    /// CHAT2: the player's reaction did not go (the message comes back as it was).
    ReactFailed {
        conv: String,
        sid: i64,
        e: String,
        reason: ReactFailure,
    },
    Call(CallView),
    CallEnd {
        id: String,
    },
    Toast(ToastView),
    /// "Rahatsız etmeyin" on or off; OVL-SOC 2 (to a mod that said `people4`): the status it
    /// is now when not "Çevrimiçi" and when a timed one ends, as in `welcome`.
    Dnd {
        on: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<&'static str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        until: Option<i64>,
    },
    /// TEAM6-O (T3): the cards of the crew tags friends and crewmates wear, by crew id; friends,
    /// messages and toasts name a crew with `badge`. With `all` the list is the whole set (it
    /// replaces what the mod holds); otherwise it changes only the crews it names, `null` taking
    /// one away (hidden by its leader, worn by nobody the player sees, tags switched off). Only to
    /// a mod that said `tags`.
    Badges {
        all: bool,
        cards: BTreeMap<String, Option<BadgeView>>,
    },
    /// BADGES-UI: the profile badge catalogue, in order (the most important first): each badge's
    /// key, colour, glyph, shape and name in both languages (the mod says it in its own). It
    /// replaces what the mod holds; friends, messages and toasts name badges by key. Only to a mod
    /// that said `marks`, before the snapshot, and again when Studio's owner changes it.
    Marks {
        defs: Vec<pl_social::MarkDef>,
    },
    /// TEAM6-O (T7): a quiet line in a crew's chat ("Ece ekibe katıldı"; the mod words it). Lines
    /// are not kept anywhere: whoever came in while this launcher ran. Only to a mod that said
    /// `tags`.
    Note {
        conv: String,
        id: String,
        kind: NoteKind,
        name: String,
        /// The player themself came in ("Ekibe katıldın").
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        mine: bool,
        /// Unix milliseconds, this computer's clock.
        at: i64,
    },
    /// The player chose "Bağlan" in the launcher (a call, an invite, a plan) and confirmed it:
    /// the game joins this server now. The one command the launcher gives the game; sent only
    /// right after that choice, only for Projects or an address a friend or crew shared, one at a
    /// time, and only to a mod that said `connect` in its hello. The mod answers with
    /// `connect-result`.
    Connect {
        id: String,
        host: String,
        port: u16,
        /// HOST1 (H12): what the mod shows instead of the address ("Arkadaş dünyası: Ayşe"); with
        /// it `host` is `127.0.0.1`, the launcher's own way to a friend's world. Only to a mod that
        /// said `host`.
        #[serde(skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    /// HOST1 (H5): open the world on `127.0.0.1` only (no LAN announcement), its gate for this
    /// publish id. Only right after the player's "Davet et" or "Dünyayı aç".
    HostPublish {
        id: String,
        mode: String,
        commands: bool,
        max: u8,
        /// The name binding key's id (PROTOCOL.md §9).
        binding: String,
    },
    /// HOST1 (H8): the session as the mod's screens show it, whole, on every change.
    HostState {
        open: bool,
        max: u8,
        guests: Vec<HostGuestView>,
        /// `unavailable` or `limit`, only when the world did not open.
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// HOST1 (H10): take this guest out now (`removed`, `blocked`, `restricted`; the mod says
    /// "Bu dünyadan çıkarıldın." for all).
    HostRemove {
        guest: String,
        reason: String,
    },
    /// HOST1 (H11): stop sharing (`player`, `left-world`, `restricted`, `launcher-exit`, `error`).
    HostStop {
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// SKIN1-O (K4): the answer to `link-want`: the codeless linking permit for game account
    /// `name`, valid until `expiresAt` (Unix seconds). The mod keeps it in memory only and hands
    /// it only to a Projects server that asks, while the game plays as `name`. Only to a mod that
    /// said `link`.
    LinkOffer {
        nonce: Secret,
        name: String,
        #[serde(rename = "expiresAt")]
        expires_at: i64,
    },
    /// SKIN1-O (K5): there is no permit (the answer to `link-want`), or the one the mod holds is
    /// no longer good (another friends account, codeless linking switched off): the mod forgets
    /// it and tells a server that asks that it has none. Only to a mod that said `link`.
    LinkClear,
    /// OVL-UX2 (Q7): what the person menu's rows choose from, whole, after the greeting and on
    /// every change. Only to a mod that said `people2`.
    Lists {
        /// The player's friend categories, in their order ("Kategoriye taşı"; empty while there
        /// are none or the service is older).
        groups: Vec<GroupItem>,
        /// The crews the player may invite friends to, in the launcher's order ("Ekibe davet et";
        /// empty while crews are off).
        crews: Vec<CrewItem>,
        /// Crewmates may ask each other to be friends here (the service offers it): "Arkadaş
        /// ekle" on a crewmate.
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        crewmates: bool,
        /// Crewmates who asked the player to be friends ("İsteği kabul et").
        #[serde(skip_serializing_if = "Vec::is_empty")]
        asked: Vec<String>,
        /// Crewmates the player asked ("İstek gönderildi").
        #[serde(skip_serializing_if = "Vec::is_empty")]
        asking: Vec<String>,
        /// Q15: crewmates who are not friends and play on Projects now ("Katıl" in their menu;
        /// never a blocked one).
        #[serde(skip_serializing_if = "Vec::is_empty")]
        projects: Vec<String>,
        /// Q15: crewmates who are not friends, whose Discord profile is known ("Discord'dan mesaj
        /// at" in their menu).
        #[serde(skip_serializing_if = "Vec::is_empty")]
        discord: Vec<String>,
        /// OVL-UX3 (U1): the player's rights over crewmates, by crew id: only crews the player
        /// leads or may remove members from (no entry: no crew rows there). Only to a mod that
        /// said `people3`.
        #[serde(skip_serializing_if = "BTreeMap::is_empty")]
        roles: BTreeMap<String, CrewRoles>,
        /// OVL-SOC 2: who asked the player to be friends, the newest first ("İstekler": "Kabul
        /// et", "Reddet"; never someone blocked). Only to a mod that said `people4`.
        #[serde(skip_serializing_if = "Vec::is_empty")]
        requests: Vec<RequestItem>,
        /// OVL-SOC2: the management lists of `people5`, `crews2` and `plans` (each only to a mod
        /// that said it).
        #[serde(flatten)]
        extra: ListsExtra,
    },
    /// OVL-UX (P8): how a friend menu action went. `queued`: the invite waits for the friend
    /// (offline or on do-not-disturb); `reason` when it did not go. Only to a mod that said
    /// `people`.
    FriendResult {
        friend: String,
        action: FriendAction,
        ok: bool,
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        queued: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<FriendRefusal>,
    },
    /// CHAT-EMOJI (E3): the chat's emoji picker: whether it is on ("Sohbette emoji seçici") and
    /// the server's emojis (Studio's list, else the launcher's built-in one) with the hosts they
    /// belong to (`pl_content::ChatEmojis`, `docs/content-format.md` "Chat emojis"). After the
    /// greeting and whenever the switch or the list changes. Only to a mod that said `emoji`.
    Emojis {
        on: bool,
        hosts: Vec<String>,
        emojis: Vec<pl_content::ChatEmoji>,
    },
    /// VOICE-O (V2): voice chat as the game shows it, whole: whether it may be used now, the
    /// player's room with everyone in it, the call that rings at a friend, the rings for the
    /// player. After the greeting and whenever what it shows changes. Only to a mod that said
    /// `voice`.
    Voice(Box<VoiceView>),
    /// VOICE-O (V3): who speaks in the player's room now (`ids`) and whether the player's own
    /// microphone sends (`me`): only when that changed, at most ten times a second (the voice page
    /// measures and the launcher passes changes alone), only while the player is in a room. Only
    /// to a mod that said `voice`.
    VoiceSpeaking {
        ids: Vec<String>,
        me: bool,
    },
    /// VOICE-O (V4): a moment of voice chat worth a note in the game (the mod words it). Only to a
    /// mod that said `voice`.
    VoiceNotice(VoiceNoticeView),
    /// VOICE-O (V5): how one of the player's presses went (`join`, `call`, `invite`, `mute`,
    /// `unmute`, `kick`, `unkick`, `mod`, `unmod`), whom it was about (`who`, an id), and why not
    /// (`reason`). Only to a mod that said `voice`.
    VoiceResult {
        act: &'static str,
        ok: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<&'static str>,
        #[serde(skip_serializing_if = "String::is_empty")]
        who: String,
    },
    /// OVL-SOC (V21): the voice rooms of the player's crews, whole, for crew chat's "Sesli": every
    /// crew in the launcher's order with its rooms in the launcher's order and who is in each.
    /// After the greeting and whenever it changes, only while voice chat may be used (an empty
    /// list once it may not). Only to a mod that said `voice` and `voice-rooms`.
    VoiceCrews {
        crews: Vec<VoiceCrewShown>,
    },
    /// OVL-AVATARS: a person's picture for their face, the answer to `avatar-want` and again when it changes:
    /// `v` its version (the mod keeps a picture by person and this; `-` for none), `n` this connection's count of
    /// them (pictures are prepared side by side: an older one after a newer one is dropped), `png` the picture
    /// (64 px square on the launcher's face colour, the player head's corners, base64), absent for the letter
    /// (no picture, or none to be had now). Only to a mod that said `avatars`.
    Avatar {
        id: String,
        v: String,
        n: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        png: Option<String>,
    },
    /// OVL-SOC2: what a `find` found, by the `code` the mod sent. Only to a mod that said
    /// `people5` or `crews2`.
    Found(Box<FoundView>),
    /// OVL-SOC2: how one of the new presses went (`act`: `cancel-request`, `unblock`, `add-code`,
    /// `crew-answer`, `crew-ask`, `crew-join`, `crew-call`, `plan-answer`, `plan-make`, `edit`,
    /// `delete`, `notify-set`), by what it named (`id`: the person, the code, the crew, the plan,
    /// the message's `sid`, the switch). `queued`: a request or a join request waits for its
    /// answer. `wait`: seconds until the next crew call may go. `reason` when it did not go (the
    /// mod says a general "Olmadı." for one it does not know).
    Done {
        act: &'static str,
        id: String,
        ok: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<&'static str>,
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        queued: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        wait: Option<u32>,
    },
    /// OVL-SOC2: the launcher's notification switches as they stand (its Settings → Arkadaşlar →
    /// Bildirimler), after the greeting and on every change. Only to a mod that said `cards2`.
    Prefs(PrefsView),
    Bye {
        reason: ByeReason,
    },
}

/// OVL-SOC2: what a code typed in "Ekle" leads to (`found`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct FoundView {
    /// As the mod sent it.
    pub code: String,
    /// `friend`, `crew`, or `none` (nothing found, or it could not be asked: `reason`).
    pub kind: &'static str,
    /// The person's or the crew's id (their face, a crew's look is not drawn).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// A person: `none`, `friend`, `outgoing`, `incoming`, `blocked`, `self`. A crew: `none`,
    /// `member`, `elsewhere`, `blocked`, `full`, `limit`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relation: Option<&'static str>,
    /// A crew: how many are in it, its tag, and whether it approves newcomers ("Katılma isteği
    /// gönder") or the player's request to it waits already.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub members: Option<u32>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub tag: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub approval: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub requested: bool,
    /// With `none`: `invalid` (not a code), `not-found`, `rate` (too many lookups), `offline`,
    /// `failed`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<&'static str>,
}

/// OVL-SOC2: the launcher's notification switches the overlay's settings show (`prefs`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrefsView {
    /// "Favori arkadaşların: Çevrimiçi olunca".
    pub favorites_online: bool,
    /// "Favori arkadaşların: Projects'e girince".
    pub favorites_projects: bool,
    /// "Ekibin: Çağrılar" (the service's switch where it keeps it).
    pub crew_calls: bool,
    /// "Ekip sohbeti bildirimleri".
    pub crew_chat: bool,
    /// "Katılma istekleri" (only while some crew of the player's may get them).
    pub join_requests: bool,
}

/// OVL-SOC2: what `lists` adds for the friends page's, crews' and plans' management.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ListsExtra {
    /// `people5`: the friend requests the player sent that wait ("Gönderilenler", "İsteği geri
    /// çek"), the people they blocked ("Engellenenler", "Engeli kaldır") and their own friend code
    /// ("Kodun", "Kopyala").
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub outgoing: Vec<RequestItem>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub blocked: Vec<RequestItem>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub code: String,
    /// `crews2`: crew invites waiting for the player, and the requests to join their crews they may
    /// answer.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub invites: Vec<CrewInviteItem>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub asks: Vec<CrewAskItem>,
    /// `crews2`: where "Ekibi çağır" stands (`ok`, `needs-server`, `server-hidden`) while crew calls
    /// are on, and the crews the player may call.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call: Option<&'static str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub callers: Vec<String>,
    /// `plans`: the crews' plans not over yet, soonest first; the crews the player may plan in;
    /// whether a plan may carry a note.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub plans: Vec<PlanItem>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub planners: Vec<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub notes: bool,
}

/// OVL-SOC2: a crew invite waiting for the player (`lists`' `invites`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CrewInviteItem {
    pub crew: String,
    pub name: String,
    /// Who invited, as the lists show them, and their id (their face).
    pub from: String,
    pub by: String,
    pub members: u32,
}

/// OVL-SOC2: someone's request to join one of the player's crews (`lists`' `asks`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CrewAskItem {
    pub crew: String,
    /// The crew's name.
    #[serde(rename = "crewName")]
    pub crew_name: String,
    pub id: String,
    pub name: String,
}

/// OVL-SOC2: a crew plan still ahead or under way (`lists`' `plans`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanItem {
    pub id: String,
    pub crew: String,
    /// The plan's time (Unix ms) to show; it starts and ends at `start` and `end` on this
    /// computer's clock (gone after `end`, "Başladı" from `start`).
    pub at: i64,
    pub start: i64,
    pub end: i64,
    /// Who made it, as the crew shows them (empty: someone not in the list).
    pub by: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub mine: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// The player's answer: `yes`, `maybe`, `no`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub my: Option<&'static str>,
    pub yes: u32,
    pub maybe: u32,
    pub no: u32,
}

/// A friend menu action, as `friend-result` names it (OVL-UX).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FriendAction {
    Invite,
    Discord,
    Unfriend,
    Block,
    Report,
    /// OVL-UX2: these go only to a mod that said `people2`.
    Favorite,
    Alert,
    Move,
    CrewInvite,
    AddFriend,
    /// OVL-UX3: these go only to a mod that said `people3`.
    CrewMod,
    CrewLead,
    CrewRemove,
    /// OVL-SOC 2: a friend request answered; only to a mod that said `people4`.
    Accept,
    Decline,
}

/// Why a friend menu action did not go (OVL-UX).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FriendRefusal {
    /// "Birlikte oyna" to them waits for its five minutes (or the day's invites are used up).
    Wait,
    /// Too many actions from the game in a short time (a stuck key or a bad mod).
    Rate,
    /// The friends service cannot be reached now.
    Offline,
    /// Anything else: not a friend any more, the service refused, nothing to open.
    Failed,
}

/// A friend in the player's world session, for the mod's screens (H8).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HostGuestView {
    /// The guest id (`g1` …; empty until the friend's launcher connected).
    pub guest: String,
    /// The friend's user id.
    pub friend: String,
    /// The friend's shown name.
    pub label: String,
    /// The in-game name, once known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// `invited`, `joining` or `in`.
    pub state: &'static str,
}

/// A friend who invited the player to their open world (H13): shown as `guests + 1` of `max + 1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct WorldMark {
    pub guests: u8,
    pub max: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Me {
    pub name: String,
    /// OVL-UX (P9): the crew whose tag the player wears (its card comes in `badges`, to a mod that
    /// said `tags`), only while crew tags are on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub badge: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FriendView {
    pub id: String,
    pub name: String,
    #[serde(rename = "where")]
    pub place: Place,
    /// "Katıl" makes sense: on Projects, or on another server whose address they share.
    pub joinable: bool,
    /// The other server's address, only when they share it (`where: server`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    /// HOST1 (H13): they invited the player to their open world (then `joinable`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub world: Option<WorldMark>,
    /// TEAM6-O (T4): the crew whose tag they wear (its card came in `badges`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub badge: Option<String>,
    /// OVL-UX (P10): their direct conversation with the player, while chat is on: the panel opens
    /// it (a new one too) and messages go there.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dm: Option<String>,
    /// OVL-UX (P11): their Discord profile is known ("Discord'dan mesaj at").
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub discord: bool,
    /// OVL-UX2 (Q2): when an offline friend was last seen, as roughly as the launcher's list says
    /// it (`today`, `yesterday`, `week`, `long`): only what the service gives, which is nothing for
    /// a friend who keeps it to themselves. Never a time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seen: Option<pl_social::LastSeen>,
    /// OVL-UX2 (Q3): one of the player's favourites (the list's star; they lead their section).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub fav: bool,
    /// OVL-UX2 (Q3): "Çevrimiçi olunca bildir" is on for them (their own choice, else the
    /// favourites' setting).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub alert: bool,
    /// OVL-UX2 (Q4): their category's id (none: the implicit "Arkadaşlar").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// OVL-UX2 (Q5): where they stand in the crews `lists` names: `member` or `invited` (the
    /// others may invite them, unless full).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub crews: BTreeMap<String, CrewPlace>,
    /// BADGES-UI: their profile badges, keys of the `marks` frame's catalogue, the most important
    /// first (the first shows right of the name).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub marks: Vec<String>,
    /// VOICE-O (V14): their voice chat: `ready` (they may be called), `in` (in a room; a call still
    /// rings); absent: no "Ara".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice: Option<pl_social::PresenceVoice>,
    /// OVL-SOC (V23): an online friend on "Rahatsız etmeyin" (the launcher's gold dot with its bar);
    /// never for an offline one.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub dnd: bool,
    /// OVL-SOC 2 (SOC-NEXT §3.6): the name the friends service knows, when `name` is the
    /// player's nickname for them (their menu says it; mentions use it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub real: Option<String>,
}

/// Someone who asked the player to be friends (OVL-SOC 2, `lists`' `requests`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RequestItem {
    pub id: String,
    pub name: String,
}

/// A friend in one of the crews the player may invite to (OVL-UX2, Q5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CrewPlace {
    Member,
    Invited,
}

/// One of the player's friend categories (OVL-UX2, Q7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GroupItem {
    pub id: String,
    pub name: String,
}

/// A crew the player may invite friends to (OVL-UX2, Q7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CrewItem {
    pub id: String,
    pub name: String,
    /// Every place is taken: nobody more can be invited.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub full: bool,
}

/// The player's rights in one crew, for the crew rows of a crewmate's menu (OVL-UX3, U1), by the
/// launcher's own rules (`teams.ts` `canRemove`, `person-menu.tsx`), so the mod shows only rows
/// that would go.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrewRoles {
    /// The player leads the crew: "Liderliği devret", and "Ekipten çıkar" for anyone.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub lead: bool,
    /// The leader names moderators here (the service offers roles): "Moderatör yap" and
    /// "Moderatörlükten al".
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub roles: bool,
    /// The crew has as many moderators as it may: "Moderatör yap" stays dimmed.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub mods_full: bool,
    /// The crew's conversation (crew chat's `c:` id), whose senders' menus carry the rows; none
    /// while chat is off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conv: Option<String>,
    /// The crewmates the rows may be about, by user id.
    pub mates: BTreeMap<String, MateRights>,
}

/// One crewmate in [`CrewRoles`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct MateRights {
    /// They are a moderator ("Moderatörlükten al" instead of "Moderatör yap").
    #[serde(rename = "mod", skip_serializing_if = "std::ops::Not::not")]
    pub moderator: bool,
    /// The player may remove them ("Ekipten çıkar").
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub kick: bool,
}

/// A crew tag's card as the overlay shows it (TEAM6-O, T3): the chip (`tag` on the crew's colour)
/// and the bubble it opens (the mark, the name, the motto, "Kuruluş Eylül 2026 · 12 üye"). It never
/// names the crew's members, leader, code or link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BadgeView {
    pub tag: String,
    /// A key of the crew palette (`gold`, `navy`, …; an unknown one draws gold).
    pub color: String,
    /// `cut`, `shield`, `banner` or `hex`.
    pub frame: String,
    /// `sword`, `shield`, … (an unknown one draws a shield).
    pub emblem: String,
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub motto: String,
    /// `YYYY-MM`: the month the crew was founded (the mod says it in its own words).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub since: String,
    pub members: u32,
    /// The player is in this crew ("Sen de bu ekiptesin").
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub mine: bool,
}

/// What a quiet line in a crew's chat says (TEAM6-O, T7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NoteKind {
    /// "Ece ekibe katıldı" (the player's own: "Ekibe katıldın").
    Joined,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Place {
    Projects,
    Launcher,
    Single,
    Server,
    /// In game, where is not known (the game's menus).
    Game,
    Offline,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConvView {
    pub id: String,
    pub kind: ConvKind,
    pub title: String,
    pub unread: u32,
    /// Muted on this computer, or its crew is muted: it never pops a toast.
    pub muted: bool,
    /// The player may write here now (still friends, still in the crew, chat on).
    pub writable: bool,
    /// CHAT2, direct conversations: the friend has seen the messages up to this `sid` ("Görüldü";
    /// only while both keep "Görüldü bilgisi" on).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seen: Option<i64>,
    /// CHAT-CALM, crew conversations: the crew's slow mode in seconds, for a player who waits
    /// between messages (never its leader or a moderator). The mod counts down from the player's
    /// last message there; the service decides anyway.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slow: Option<u32>,
    /// OVL-SOC2, crew conversations: the crew's id (crew chat's own line: "Ekibi çağır", its plans).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crew: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConvKind {
    Dm,
    Crew,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct MsgView {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conv: Option<String>,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cid: Option<String>,
    pub from: String,
    /// A sticker's words ("[Çıkartma: GG]", in the overlay's language) for a mod that cannot draw it.
    pub text: String,
    /// Unix time in milliseconds.
    pub at: i64,
    pub mine: bool,
    /// "düzenlendi".
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub edited: bool,
    /// Deleted for everyone: `text` is empty and the mod shows a quiet "Bu mesaj silindi" line.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub deleted: bool,
    /// CHAT2: the service's id (replies and reactions name a message by it; "Görüldü" compares it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sid: Option<i64>,
    /// CHAT2: a Projects sticker (`pl1/gg`); the mod draws it from its own atlas.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sticker: Option<String>,
    /// CHAT2: the message it answers, as this computer holds it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply: Option<ReplyView>,
    /// CHAT2: its reactions, kinds in the order first given.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub reactions: Vec<ReactionView>,
    /// CHAT2: the links of its text the player may press now (they pass the rule table and
    /// Studio's lists); any other stays plain text. Only `https`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<LinkView>,
    /// CHAT2: its first link's card, made by the launcher ("YouTube · <başlık>").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card: Option<CardView>,
    /// TEAM6-O (T5): the crew tag its sender wears, on someone else's message in a crew's chat
    /// (the chip before the name over their group).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub badge: Option<String>,
    /// OVL-UX2 (Q6): its sender's user id, on someone else's message in a crew's chat (their
    /// name opens their person menu).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
    /// BADGES-UI: its sender's profile badges, on someone else's message in a crew's chat (right
    /// of the name over their group).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub marks: Vec<String>,
    /// OVL-SOC 2 (SOC-NEXT §3.7): someone the player ignores wrote it (crew chat folds their
    /// lines into one with "Göster").
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub ignored: bool,
    /// OVL-SOC 2 (SOC-NEXT §3.1): it mentions the player ("@Kaan_07" in crew chat; its bubble
    /// has the quiet gold ring).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub mention: bool,
}

/// The message a reply answers (CHAT2): the service keeps only the link, the words come from this
/// computer's own history.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ReplyView {
    /// Its `id` in the conversation, when this computer holds it (the mod scrolls to it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub sid: i64,
    /// Its sender as the lists show them (empty when `missing`).
    pub from: String,
    /// The player's own message.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub mine: bool,
    /// What it said, at most [`MAX_QUOTE_CHARS`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sticker: Option<String>,
    /// Deleted for everyone: "Mesaj silindi".
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub deleted: bool,
    /// Not on this computer, or from someone no longer seen: "Önceki bir mesaj".
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub missing: bool,
}

/// Characters of a quoted message the bridge carries (the mod shows one line of it).
pub const MAX_QUOTE_CHARS: usize = 120;

/// One kind of reaction on a message (CHAT2).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ReactionView {
    /// One emoji, as the service keeps it (send it back as it is to give or take the same one).
    pub e: String,
    /// How many people gave it.
    pub n: u32,
    /// The player is one of them.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub mine: bool,
    /// The first few of the others, by name (the mod's "Ayşe, Mert ve 2 kişi daha").
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub who: Vec<String>,
}

/// A link the player may press (CHAT2).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct LinkView {
    /// As written in the text.
    pub url: String,
    /// A site Studio trusts.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub trusted: bool,
}

/// A link's card as the overlay shows it: one quiet line (CHAT2; no picture in game).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CardView {
    /// "YouTube", else the domain.
    pub site: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CallView {
    pub id: String,
    /// The crew's name.
    pub crew: String,
    /// The caller's name.
    pub from: String,
    /// When the call started, Unix milliseconds.
    pub since: i64,
    /// When it ends by itself, Unix milliseconds (no frame says so then).
    pub until: i64,
    pub answers: AnswerCounts,
    pub mine: Option<CallAnswer>,
    /// Where the caller plays: `projects`, or `server` with `host` when they share it. Absent for
    /// an answer-only call.
    #[serde(rename = "where", skip_serializing_if = "Option::is_none")]
    pub place: Option<Place>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    /// The player's own call (nothing to answer).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub own: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct AnswerCounts {
    pub coming: u32,
    pub soon: u32,
    pub cant: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToastView {
    pub kind: ToastKind,
    /// Toasts with the same key update in place (one conversation, one call).
    pub key: String,
    pub title: String,
    pub body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conv: Option<String>,
    /// The crew of a crew chat message (`title` is the sender).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crew: Option<String>,
    /// CHAT2: a sticker message (`body` says "Çıkartma · GG"); the mod may draw a small picture.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sticker: Option<String>,
    /// TEAM6-O (T6): a message's sender wears this crew's tag (the chip before `title`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub badge: Option<String>,
    /// BADGES-UI: the profile badges of the person `title` names (a message's sender, the friend
    /// of an invite), right of the title.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub marks: Vec<String>,
    /// OVL-AVATARS: the person `title` names (a message's sender, an invite's friend), for their picture.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
    /// OVL-SOC2: a plan's time (Unix ms; `plan` cards: "{title}, {when} için plan yaptı").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ToastKind {
    /// `title` is the sender, `body` the text (the mod hides it when the player turned message
    /// text off in its settings).
    Message,
    /// A crew call: `title` is the crew, `body` the caller; the mod words it in its own language.
    Call,
    /// "Play together": `title` is the friend; the mod words it.
    Invite,
    Info,
    /// HOST1 (H14): a world invite, `title` the friend ("%s seni dünyasına çağırıyor"). Only to a
    /// mod that said `host`.
    World,
    // OVL-SOC2: the cards below go only to a mod that said `cards2`; the mod words each one, `title`
    // the person (`by` their id), `crew` the crew's name where one is named.
    /// "{title} seni eklemek istiyor".
    Request,
    /// "{title} ile artık arkadaşsınız".
    Friend,
    /// "{title} çevrimiçi".
    Online,
    /// "{title} Projects'e girdi".
    Projects,
    /// "{title} seni {crew} ekibine davet etti".
    CrewInvite,
    /// "{title}, {crew} ekibine katılmak istiyor".
    CrewAsk,
    /// "{crew} ekibine katıldın" (a join request of the player's was taken; `conv` the crew's chat).
    CrewIn,
    /// "{title}, {when} için plan yaptı" (`at`; `body` the plan's note, `conv` the crew's chat).
    Plan,
    /// "Cevapsız arama", `title` the caller.
    Missed,
}

/// Why a message did not go. The mod shows a general "could not send" for a reason it does not
/// know.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SendFailure {
    /// A link ("Bağlantı gönderilemez.").
    Link,
    /// A phone number.
    Phone,
    /// A word the service does not let through (it never says which).
    Words,
    /// Too many messages in a row.
    Rate,
    /// The day's messages are used up.
    DailyLimit,
    /// Empty, or too long after cleaning.
    TooLong,
    Blocked,
    /// No connection to the friends service: it may still go once the launcher reconnects.
    Offline,
    /// A moderator muted the player.
    Muted,
    /// Chat is switched off.
    Off,
    /// No longer friends, or no longer in the crew.
    NotAllowed,
    /// CHAT2: the day's stickers are used up.
    Stickers,
    /// CHAT2: the day's messages with links are used up.
    Links,
    /// CHAT2: a new Discord account sends no links yet.
    LimitedLinks,
    /// CHAT2: Studio switched this extra (stickers, replies) off.
    FeatureOff,
    /// CHAT-CALM: the crew chat's slow mode; only to a mod that said `slow`.
    Slow,
}

/// Why a reaction did not go (CHAT2). The mod says something general for one it does not know.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReactFailure {
    /// The player has 5 reactions on that message already.
    Full,
    /// The message carries 10 kinds already.
    Kinds,
    /// The message is older than a week (or no longer here).
    Old,
    /// That emoji is never a reaction (🖕).
    Words,
    Rate,
    Muted,
    /// Reactions are switched off.
    Off,
    /// No longer friends, or no longer in the crew.
    NotAllowed,
    /// No connection to the friends service.
    Offline,
}

/// VOICE-O (V2): voice chat as the game shows it. Everything absent is false, empty or none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VoiceView {
    /// Voice chat may be used now (Studio, the friends service and the player's own switch agree):
    /// "Ara" and "Odaya davet et" may show.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub on: bool,
    /// The room the player is in, or is getting into.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room: Option<VoiceRoomShown>,
    /// The friend the player's call rings at ("Aranıyor…"); the player waits in the room.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub calling: Option<VoicePersonRef>,
    /// Calls and invites ringing for the player, oldest first (the card with "Katıl"/"Reddet").
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rings: Vec<VoiceRingShown>,
    /// The mod rings for them: the launcher's window is away, so nothing else does (in front, the
    /// launcher's own page rings and the mod stays quiet).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub sound: bool,
    /// A person's volume at most (percent) while the voice page cannot go past it (V20: 100 through
    /// `<audio>` elements); absent: [`crate::voice::MAX_VOLUME`].
    #[serde(rename = "volumeMax", skip_serializing_if = "Option::is_none")]
    pub volume_max: Option<u16>,
}

impl VoiceView {
    /// Nothing to show: voice chat is off and the player is in no room, called by no one.
    pub fn is_empty(&self) -> bool {
        !self.on && self.room.is_none() && self.calling.is_none() && self.rings.is_empty()
    }
}

/// The player's room (V2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VoiceRoomShown {
    pub id: String,
    /// `friend` or `crew`.
    pub kind: pl_voice::view::VoiceRoomKind,
    /// Its name; none for the crew's "Genel" (`general`, the mod says it in its language) and for a
    /// friend room nobody named (the mod names it after the people in it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub general: bool,
    /// A crew room's crew, by name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crew: Option<String>,
    /// `joining`, `live` or `reconnecting` ("Yeniden bağlanıyor…").
    pub phase: pl_voice::view::VoicePhase,
    /// People at most, the player included.
    pub max: u16,
    pub me: VoiceMeShown,
    /// Everyone else, longest present first.
    pub people: Vec<VoicePersonShown>,
    /// The player's invites still ringing ("Aranıyor…").
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ringing: Vec<VoicePersonRef>,
}

/// The player in the room (V2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceMeShown {
    pub role: pl_voice::view::VoiceRole,
    /// "Mikrofonu kapat".
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub mute: bool,
    /// "Sağırlaştır".
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub deaf: bool,
    /// A moderator muted the player for everyone.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub muted_by_moderator: bool,
    /// The microphone sends now (as of this frame; `voice-speaking` brings the changes).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub talking: bool,
    /// `weak` or `bad`; absent while good.
    #[serde(skip_serializing_if = "is_good")]
    pub quality: pl_voice::view::VoiceQuality,
}

/// Someone else in the room (V2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoicePersonShown {
    pub id: String,
    /// As the friend lists show them; absent for someone who is neither a friend nor a crewmate
    /// (the mod says "Bir oyuncu").
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    pub role: pl_voice::view::VoiceRole,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub mute: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub deaf: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub muted_by_moderator: bool,
    #[serde(skip_serializing_if = "is_good")]
    pub quality: pl_voice::view::VoiceQuality,
    /// Speaks now (as of this frame).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub speaking: bool,
    /// Their volume for the player, in percent (100 as they come).
    pub volume: u16,
    /// "Benim için sustur".
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub muted_for_me: bool,
    /// What the player's role may do to them (the person menu shows only these rows).
    #[serde(skip_serializing_if = "VoiceCan::is_none")]
    pub can: VoiceCan,
}

/// A leader's or moderator's rows on someone (V2). "Taşı" stays in the launcher.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct VoiceCan {
    /// "Herkes için sustur" / its undo.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub mute: bool,
    /// "Odadan çıkar".
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub kick: bool,
    /// "Moderatör yap" / "Moderatörlükten al".
    #[serde(rename = "mod", skip_serializing_if = "std::ops::Not::not")]
    pub moderator: bool,
}

impl VoiceCan {
    pub fn is_none(&self) -> bool {
        !self.mute && !self.kick && !self.moderator
    }
}

/// Someone named in V2 (a call's friend, an invite that rings).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VoicePersonRef {
    pub id: String,
    /// Absent for someone the lists do not know.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
}

/// A call or invite ringing for the player (V2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VoiceRingShown {
    /// What `voice-answer` names.
    pub room: String,
    /// The caller, by id, and their name (absent when the lists do not know them).
    pub from: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// The others in the room, by name (an unknown one empty), at most four.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub with: Vec<String>,
    /// Unix milliseconds: it stops ringing then (no frame says so).
    pub until: i64,
}

/// One of the player's crews in `voice-crews` (V21).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VoiceCrewShown {
    /// The crew, by id (what `voice-join` names).
    pub crew: String,
    /// Its chat (`c:<crew>`): the mod shows the rooms there.
    pub conv: String,
    /// Its rooms in the launcher's order; with nobody in any, its "Genel" alone.
    pub rooms: Vec<VoiceCrewRoomShown>,
    /// "Yeni oda" may show: the crew has fewer rooms than it may.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub add: bool,
}

/// A crew's room in `voice-crews` (V21).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VoiceCrewRoomShown {
    /// What `voice-join` names (the crew's "Genel" with nobody in it: the service's `g` + crew id).
    pub id: String,
    /// Its name in the overlay's language; none for the crew's "Genel" (`general`: the mod says
    /// it in its language).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub general: bool,
    /// Who is in it, as voice chat lists them (the player too, when there); a name the lists do
    /// not know is absent.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub people: Vec<VoicePersonRef>,
    /// OVL-SOC 2 (VOICE-ROOMS): people at most, when the crew gave the room a limit below the
    /// crew room maximum (the pill says "3/5").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<u16>,
    /// OVL-SOC 2: nobody more fits ("Oda dolu.": a press says so and asks nothing).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub full: bool,
}

/// A voice chat moment for a note in the game (V4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VoiceNoticeView {
    /// `left` (with `reason`), `moved` (a moderator moved the player to another room of the
    /// crew), `muted` (a moderator muted the player), `mic` (with `reason`: `denied`, `missing`),
    /// `mic-changed` (the microphone was unplugged; the default one took over).
    pub kind: &'static str,
    /// `left`: `closed`, `alone`, `no-answer`, `elsewhere`, `kicked`, `crew`, `idle`,
    /// `unavailable`, `failed`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<&'static str>,
    /// `left` / `no-answer`: the friend who did not join (absent when unknown).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
}

fn is_good(quality: &pl_voice::view::VoiceQuality) -> bool {
    *quality == pl_voice::view::VoiceQuality::Good
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ByeReason {
    /// The launcher is quitting.
    LauncherExit,
    /// The launcher closes because "on game start: close" is chosen.
    GameStartClose,
    /// The player turned the overlay off.
    Disabled,
    /// A newer connection from the same game took over.
    Replaced,
    /// Wrong token.
    Auth,
    /// No common protocol version.
    Proto,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_parses_and_hides_the_token() {
        let hello: FromGame = serde_json::from_str(
            r#"{"t":"hello","proto":1,"token":"abc","mod":"0.1.0","mc":"26.1.2","lang":"tr_tr","extra":1}"#,
        )
        .unwrap();
        let FromGame::Hello {
            proto,
            token,
            mod_version,
            ..
        } = &hello
        else {
            panic!("not a hello: {hello:?}");
        };
        assert_eq!(*proto, 1);
        assert_eq!(token.expose(), "abc");
        assert_eq!(mod_version, "0.1.0");
        assert!(!format!("{hello:?}").contains("abc"));
    }

    #[test]
    fn avatar_frames() {
        let want: FromGame =
            serde_json::from_str(r#"{"t":"avatar-want","ids":["f-ayse","u-cem"]}"#).unwrap();
        assert_eq!(
            want,
            FromGame::AvatarWant {
                ids: vec!["f-ayse".into(), "u-cem".into()]
            }
        );
        let none: FromGame = serde_json::from_str(r#"{"t":"avatar-want"}"#).unwrap();
        assert_eq!(none, FromGame::AvatarWant { ids: Vec::new() });
        let picture = ToGame::Avatar {
            id: "f-ayse".into(),
            v: "0123456789abcdef".into(),
            n: 2,
            png: Some("iVBO".into()),
        };
        assert_eq!(
            serde_json::to_value(&picture).unwrap(),
            serde_json::json!({"t":"avatar","id":"f-ayse","v":"0123456789abcdef","n":2,"png":"iVBO"})
        );
        let letter = ToGame::Avatar {
            id: "f-can".into(),
            v: "-".into(),
            n: 3,
            png: None,
        };
        assert_eq!(
            serde_json::to_value(&letter).unwrap(),
            serde_json::json!({"t":"avatar","id":"f-can","v":"-","n":3})
        );
    }

    #[test]
    fn unknown_frames_parse_as_unknown() {
        let frame: FromGame = serde_json::from_str(r#"{"t":"screenshot-share","x":1}"#).unwrap();
        assert_eq!(frame, FromGame::Unknown);
    }

    #[test]
    fn player_actions_parse() {
        let send: FromGame =
            serde_json::from_str(r#"{"t":"send","cid":"c1","conv":"dm:1","text":"selam"}"#)
                .unwrap();
        assert!(matches!(send, FromGame::Send { ref text, .. } if text == "selam"));
        let answer: FromGame =
            serde_json::from_str(r#"{"t":"answer","call":"c1","answer":"cant"}"#).unwrap();
        assert!(matches!(
            answer,
            FromGame::Answer {
                answer: CallAnswer::Cant,
                ..
            }
        ));
        assert!(
            serde_json::from_str::<FromGame>(r#"{"t":"answer","call":"c1","answer":"x"}"#).is_err()
        );
        let join: FromGame = serde_json::from_str(r#"{"t":"join","friend":"f1"}"#).unwrap();
        assert_eq!(
            join,
            FromGame::Join {
                friend: "f1".into()
            }
        );
        let done: FromGame =
            serde_json::from_str(r#"{"t":"connect-result","id":"3","ok":true}"#).unwrap();
        assert_eq!(
            done,
            FromGame::ConnectResult {
                id: "3".into(),
                ok: true,
                reason: String::new()
            }
        );
        let busy: FromGame =
            serde_json::from_str(r#"{"t":"connect-result","id":"4","ok":false,"reason":"busy"}"#)
                .unwrap();
        assert!(
            matches!(busy, FromGame::ConnectResult { ok: false, ref reason, .. } if reason == "busy")
        );
    }

    #[test]
    fn hello_says_what_the_mod_can_do() {
        let old: FromGame = serde_json::from_str(r#"{"t":"hello","proto":1,"token":"t"}"#).unwrap();
        assert!(matches!(old, FromGame::Hello { ref features, .. } if features.is_empty()));
        let new: FromGame = serde_json::from_str(
            r#"{"t":"hello","proto":1,"token":"t","features":["connect","later"]}"#,
        )
        .unwrap();
        assert!(
            matches!(new, FromGame::Hello { ref features, .. } if features.iter().any(|f| f == FEATURE_CONNECT))
        );
        let connect = serde_json::to_value(ToGame::Connect {
            id: "1".into(),
            host: "play.projects.gg".into(),
            port: 25565,
            label: None,
        })
        .unwrap();
        assert_eq!(
            connect,
            serde_json::json!({"t":"connect","id":"1","host":"play.projects.gg","port":25565})
        );
    }

    #[test]
    fn the_overlays_language_reads_and_writes_as_the_log_says() {
        // G4: the player's pick from the game.
        let pick: FromGame = serde_json::from_str(r#"{"t":"lang","lang":"en"}"#).unwrap();
        assert_eq!(pick, FromGame::Lang { lang: "en".into() });
        let bare: FromGame = serde_json::from_str(r#"{"t":"lang"}"#).unwrap();
        assert_eq!(
            bare,
            FromGame::Lang {
                lang: String::new()
            }
        );
        // G3: the welcome names the language only when there is one to name (a mod that said
        // `lang`); an older mod's welcome stays exactly as it was.
        let welcome = |lang| ToGame::Welcome {
            proto: PROTO,
            launcher: "t".into(),
            me: Me {
                name: "Ece".into(),
                badge: None,
            },
            dnd: false,
            reduced_motion: false,
            features: Vec::new(),
            lang,
            status: None,
            until: None,
        };
        assert_eq!(
            serde_json::to_value(welcome(Some(pl_lang::Lang::En))).unwrap()["lang"],
            "en"
        );
        assert_eq!(
            serde_json::to_value(welcome(Some(pl_lang::Lang::Tr))).unwrap()["lang"],
            "tr"
        );
        assert_eq!(
            serde_json::to_value(welcome(None)).unwrap(),
            serde_json::json!({"t":"welcome","proto":1,"launcher":"t","me":{"name":"Ece"},"dnd":false,
                "reducedMotion":false,"features":[]})
        );
        assert_eq!(FEATURE_LANG, "lang");
    }

    #[test]
    fn the_emoji_pickers_frames_read_and_write_as_the_log_says() {
        // E4: the player's switch from the game; anything but a bool is not this frame.
        let off: FromGame = serde_json::from_str(r#"{"t":"emoji","on":false}"#).unwrap();
        assert_eq!(off, FromGame::Emoji { on: false });
        assert!(serde_json::from_str::<FromGame>(r#"{"t":"emoji","on":"no"}"#).is_err());
        // E3: the switch and the list as the content document has them.
        let list = pl_content::ChatEmojis::builtin();
        let kalp = list.emojis[0].clone();
        let frame = serde_json::to_value(ToGame::Emojis {
            on: true,
            hosts: list.hosts.clone(),
            emojis: vec![kalp],
        })
        .unwrap();
        assert_eq!(
            frame,
            serde_json::json!({"t":"emojis","on":true,"hosts":["projects.gg"],"emojis":[
                {"code":":kalp:","aliases":["<3","♥"],"look":"&c❤","kind":"symbol","rank":"VIP"}]})
        );
        // The whole built-in list fits a frame many times over.
        let whole = serde_json::to_vec(&ToGame::Emojis {
            on: true,
            hosts: list.hosts,
            emojis: list.emojis,
        })
        .unwrap();
        assert!(
            whole.len() < MAX_LAUNCHER_FRAME / 8,
            "{} bytes",
            whole.len()
        );
        assert_eq!((FEATURE_EMOJI, FEATURE_PANEL_OFF), ("emoji", "panel-off"));
    }

    #[test]
    fn host_frames_read_and_write_as_the_log_says() {
        let world: FromGame = serde_json::from_str(
            r#"{"t":"host-world","open":true,"folder":"Yeni Dünya","mode":"survival","lan":false}"#,
        )
        .unwrap();
        assert!(world.is_host());
        assert!(
            matches!(world, FromGame::HostWorld { open: true, ref folder, .. } if folder == "Yeni Dünya")
        );
        let left: FromGame = serde_json::from_str(r#"{"t":"host-world","open":false}"#).unwrap();
        assert!(matches!(left, FromGame::HostWorld { open: false, .. }));
        let request: FromGame = serde_json::from_str(
            r#"{"t":"host-request","mode":"creative","commands":true,"invite":["f1","f2"]}"#,
        )
        .unwrap();
        assert!(
            matches!(request, FromGame::HostRequest { commands: true, ref invite, .. } if invite.len() == 2)
        );
        let published: FromGame = serde_json::from_str(
            r#"{"t":"host-published","id":"p1","ok":true,"port":51234,"proto":774}"#,
        )
        .unwrap();
        assert_eq!(
            published,
            FromGame::HostPublished {
                id: "p1".into(),
                ok: true,
                port: 51234,
                proto: 774,
                reason: String::new()
            }
        );
        let refused: FromGame =
            serde_json::from_str(r#"{"t":"host-published","id":"p1","ok":false,"reason":"lan"}"#)
                .unwrap();
        assert!(
            matches!(refused, FromGame::HostPublished { ok: false, ref reason, .. } if reason == "lan")
        );
        let guest: FromGame = serde_json::from_str(
            r#"{"t":"host-guest","conn":3,"guest":"g1","name":"Notch","state":"in"}"#,
        )
        .unwrap();
        assert!(matches!(guest, FromGame::HostGuest { conn: 3, ref state, .. } if state == "in"));
        let more: FromGame =
            serde_json::from_str(r#"{"t":"host-invite","invite":["f3"]}"#).unwrap();
        assert!(matches!(more, FromGame::HostInvite { ref invite } if invite == &["f3"]));
        let remove: FromGame = serde_json::from_str(r#"{"t":"host-remove","guest":"g2"}"#).unwrap();
        assert_eq!(remove, FromGame::HostRemove { guest: "g2".into() });
        let stop: FromGame = serde_json::from_str(r#"{"t":"host-stop"}"#).unwrap();
        assert_eq!(
            stop,
            FromGame::HostStop {
                reason: String::new()
            }
        );
        let meet: FromGame =
            serde_json::from_str(r#"{"t":"host-meet","invite":["f1","f2"]}"#).unwrap();
        assert!(meet.is_host());
        assert!(matches!(meet, FromGame::HostMeet { ref invite } if invite == &["f1", "f2"]));
        let join: FromGame = serde_json::from_str(r#"{"t":"join","friend":"f1"}"#).unwrap();
        assert!(!join.is_host());

        let publish = serde_json::to_value(ToGame::HostPublish {
            id: "p1".into(),
            mode: "survival".into(),
            commands: false,
            max: 7,
            binding: "sW9hzU3c".into(),
        })
        .unwrap();
        assert_eq!(
            publish,
            serde_json::json!({"t":"host-publish","id":"p1","mode":"survival","commands":false,"max":7,"binding":"sW9hzU3c"})
        );
        let state = serde_json::to_value(ToGame::HostState {
            open: true,
            max: 7,
            guests: vec![HostGuestView {
                guest: "g1".into(),
                friend: "f1".into(),
                label: "Mert".into(),
                name: Some("Notch".into()),
                state: "in",
            }],
            error: None,
        })
        .unwrap();
        assert_eq!(state["t"], "host-state");
        assert_eq!(state["guests"][0]["label"], "Mert");
        assert!(state.get("error").is_none());
        let failed = serde_json::to_value(ToGame::HostState {
            open: false,
            max: 7,
            guests: Vec::new(),
            error: Some("limit".into()),
        })
        .unwrap();
        assert_eq!(failed["error"], "limit");
        assert_eq!(
            serde_json::to_value(ToGame::HostRemove {
                guest: "g1".into(),
                reason: "removed".into()
            })
            .unwrap(),
            serde_json::json!({"t":"host-remove","guest":"g1","reason":"removed"})
        );
        assert_eq!(
            serde_json::to_value(ToGame::HostStop {
                reason: Some("launcher-exit".into())
            })
            .unwrap(),
            serde_json::json!({"t":"host-stop","reason":"launcher-exit"})
        );
        let connect = serde_json::to_value(ToGame::Connect {
            id: "2".into(),
            host: "127.0.0.1".into(),
            port: 50123,
            label: Some("Arkadaş dünyası: Ayşe".into()),
        })
        .unwrap();
        assert_eq!(connect["label"], "Arkadaş dünyası: Ayşe");
        let toast = serde_json::to_value(ToGame::Toast(ToastView {
            kind: ToastKind::World,
            key: "world:s".into(),
            title: "Ayşe".into(),
            body: String::new(),
            conv: None,
            crew: None,
            sticker: None,
            badge: None,
            marks: Vec::new(),
            by: None,
            at: None,
        }))
        .unwrap();
        assert_eq!(toast["kind"], "world");
    }

    #[test]
    fn friend_menu_frames_read_and_write_as_the_log_says() {
        let invite: FromGame = serde_json::from_str(r#"{"t":"invite","friend":"f1"}"#).unwrap();
        assert_eq!(
            invite,
            FromGame::Invite {
                friend: "f1".into()
            }
        );
        let discord: FromGame = serde_json::from_str(r#"{"t":"discord","friend":"f1"}"#).unwrap();
        assert!(matches!(discord, FromGame::Discord { ref friend } if friend == "f1"));
        let unfriend: FromGame = serde_json::from_str(r#"{"t":"unfriend","friend":"f2"}"#).unwrap();
        assert!(matches!(unfriend, FromGame::Unfriend { ref friend } if friend == "f2"));
        let block: FromGame = serde_json::from_str(r#"{"t":"block","friend":"f3"}"#).unwrap();
        assert!(matches!(block, FromGame::Block { ref friend } if friend == "f3"));
        let report: FromGame = serde_json::from_str(
            r#"{"t":"report","friend":"f4","reason":"harassment","note":"sürekli","block":true}"#,
        )
        .unwrap();
        assert_eq!(
            report,
            FromGame::Report {
                friend: "f4".into(),
                reason: "harassment".into(),
                note: "sürekli".into(),
                block: true
            }
        );
        let bare: FromGame =
            serde_json::from_str(r#"{"t":"report","friend":"f4","reason":"spam"}"#).unwrap();
        assert!(matches!(bare, FromGame::Report { ref note, block: false, .. } if note.is_empty()));
        // A frame without its friend is not one of them.
        assert!(serde_json::from_str::<FromGame>(r#"{"t":"block"}"#).is_err());
        assert!(!invite.is_host());

        let sent = serde_json::to_value(ToGame::FriendResult {
            friend: "f1".into(),
            action: FriendAction::Invite,
            ok: true,
            queued: true,
            reason: None,
        })
        .unwrap();
        assert_eq!(
            sent,
            serde_json::json!({"t":"friend-result","friend":"f1","action":"invite","ok":true,"queued":true})
        );
        let refused = serde_json::to_value(ToGame::FriendResult {
            friend: "f2".into(),
            action: FriendAction::Unfriend,
            ok: false,
            queued: false,
            reason: Some(FriendRefusal::Offline),
        })
        .unwrap();
        assert_eq!(
            refused,
            serde_json::json!({"t":"friend-result","friend":"f2","action":"unfriend","ok":false,"reason":"offline"})
        );
        let me = serde_json::to_value(Me {
            name: "Ece".into(),
            badge: Some("t1".into()),
        })
        .unwrap();
        assert_eq!(me, serde_json::json!({"name":"Ece","badge":"t1"}));
        let plain = serde_json::to_value(Me {
            name: "Ece".into(),
            badge: None,
        })
        .unwrap();
        assert_eq!(plain, serde_json::json!({"name":"Ece"}));
    }

    #[test]
    fn the_whole_person_menus_frames_read_and_write_as_the_log_says() {
        let favorite: FromGame =
            serde_json::from_str(r#"{"t":"favorite","friend":"f1","on":true}"#).unwrap();
        assert_eq!(
            favorite,
            FromGame::Favorite {
                friend: "f1".into(),
                on: true
            }
        );
        let alert: FromGame =
            serde_json::from_str(r#"{"t":"alert","friend":"f1","on":false}"#).unwrap();
        assert!(matches!(alert, FromGame::Alert { ref friend, on: false } if friend == "f1"));
        let moved: FromGame = serde_json::from_str(r#"{"t":"move","friend":"f2"}"#).unwrap();
        assert!(matches!(moved, FromGame::Move { ref group, .. } if group.is_empty()));
        let invited: FromGame =
            serde_json::from_str(r#"{"t":"crew-invite","friend":"f3","crew":"t1"}"#).unwrap();
        assert!(
            matches!(invited, FromGame::CrewInvite { ref friend, ref crew } if friend == "f3" && crew == "t1")
        );
        let added: FromGame =
            serde_json::from_str(r#"{"t":"add-friend","friend":"u-cem"}"#).unwrap();
        assert!(matches!(added, FromGame::AddFriend { ref friend } if friend == "u-cem"));
        // A mark without its state is not one.
        assert!(serde_json::from_str::<FromGame>(r#"{"t":"favorite","friend":"f1"}"#).is_err());

        let lists = serde_json::to_value(ToGame::Lists {
            groups: vec![GroupItem {
                id: "g1".into(),
                name: "Okul".into(),
            }],
            crews: vec![
                CrewItem {
                    id: "t1".into(),
                    name: "Gece Kuşları".into(),
                    full: false,
                },
                CrewItem {
                    id: "t2".into(),
                    name: "Akıncılar".into(),
                    full: true,
                },
            ],
            crewmates: false,
            asked: Vec::new(),
            asking: Vec::new(),
            projects: Vec::new(),
            discord: Vec::new(),
            roles: BTreeMap::new(),
            requests: Vec::new(),
            extra: ListsExtra::default(),
        })
        .unwrap();
        assert_eq!(
            lists,
            serde_json::json!({"t":"lists","groups":[{"id":"g1","name":"Okul"}],
                "crews":[{"id":"t1","name":"Gece Kuşları"},{"id":"t2","name":"Akıncılar","full":true}]})
        );
        // Q15: crewmates on Projects and with a Discord profile, only when there are any.
        let mates = serde_json::to_value(ToGame::Lists {
            groups: Vec::new(),
            crews: Vec::new(),
            crewmates: true,
            asked: Vec::new(),
            asking: Vec::new(),
            projects: vec!["u-cem".into()],
            discord: vec!["u-cem".into(), "u-ali".into()],
            roles: BTreeMap::new(),
            requests: Vec::new(),
            extra: ListsExtra::default(),
        })
        .unwrap();
        assert_eq!(
            mates,
            serde_json::json!({"t":"lists","groups":[],"crews":[],"crewmates":true,
                "projects":["u-cem"],"discord":["u-cem","u-ali"]})
        );
        // OVL-UX3 (U1): the crews the player has rights in, and the crewmates the rows are about.
        let roles = serde_json::to_value(ToGame::Lists {
            groups: Vec::new(),
            crews: Vec::new(),
            crewmates: false,
            asked: Vec::new(),
            asking: Vec::new(),
            projects: Vec::new(),
            discord: Vec::new(),
            roles: BTreeMap::from([
                (
                    "t1".to_owned(),
                    CrewRoles {
                        lead: true,
                        roles: true,
                        mods_full: true,
                        conv: Some("c:t1".into()),
                        mates: BTreeMap::from([
                            (
                                "u-cem".to_owned(),
                                MateRights {
                                    moderator: true,
                                    kick: true,
                                },
                            ),
                            (
                                "u-ali".to_owned(),
                                MateRights {
                                    moderator: false,
                                    kick: true,
                                },
                            ),
                        ]),
                    },
                ),
                (
                    "t2".to_owned(),
                    CrewRoles {
                        mates: BTreeMap::from([(
                            "u-ece".to_owned(),
                            MateRights {
                                moderator: false,
                                kick: true,
                            },
                        )]),
                        ..CrewRoles::default()
                    },
                ),
            ]),
            requests: Vec::new(),
            extra: ListsExtra::default(),
        })
        .unwrap();
        assert_eq!(
            roles,
            serde_json::json!({"t":"lists","groups":[],"crews":[],"roles":{
                "t1":{"lead":true,"roles":true,"modsFull":true,"conv":"c:t1",
                    "mates":{"u-ali":{"kick":true},"u-cem":{"mod":true,"kick":true}}},
                "t2":{"mates":{"u-ece":{"kick":true}}}}})
        );
        let made: FromGame =
            serde_json::from_str(r#"{"t":"crew-mod","crew":"t1","friend":"u-ali","on":true}"#)
                .unwrap();
        assert_eq!(
            made,
            FromGame::CrewMod {
                crew: "t1".into(),
                friend: "u-ali".into(),
                on: true
            }
        );
        let handed: FromGame =
            serde_json::from_str(r#"{"t":"crew-lead","crew":"t1","friend":"u-cem"}"#).unwrap();
        assert_eq!(
            handed,
            FromGame::CrewLead {
                crew: "t1".into(),
                friend: "u-cem".into()
            }
        );
        let removed: FromGame =
            serde_json::from_str(r#"{"t":"crew-remove","crew":"t2","friend":"u-ece"}"#).unwrap();
        assert_eq!(
            removed,
            FromGame::CrewRemove {
                crew: "t2".into(),
                friend: "u-ece".into()
            }
        );
        // "Yeni kategori…": a move with a name and no category.
        let fresh: FromGame =
            serde_json::from_str(r#"{"t":"move","friend":"f1","name":"Okul arkadaşları"}"#)
                .unwrap();
        assert!(
            matches!(fresh, FromGame::Move { ref group, ref name, .. } if group.is_empty() && name == "Okul arkadaşları")
        );
        // Without its state a moderator row is not one.
        assert!(
            serde_json::from_str::<FromGame>(r#"{"t":"crew-mod","crew":"t1","friend":"u-ali"}"#)
                .is_err()
        );
        let result = serde_json::to_value(ToGame::FriendResult {
            friend: "u-cem".into(),
            action: FriendAction::CrewLead,
            ok: true,
            queued: false,
            reason: None,
        })
        .unwrap();
        assert_eq!(
            result,
            serde_json::json!({"t":"friend-result","friend":"u-cem","action":"crew-lead","ok":true})
        );
        let marked = serde_json::to_value(FriendView {
            id: "f1".into(),
            name: "Ayşe".into(),
            place: Place::Offline,
            joinable: false,
            host: None,
            world: None,
            badge: None,
            dm: None,
            discord: false,
            seen: Some(pl_social::LastSeen::Today),
            fav: true,
            alert: true,
            group: Some("g1".into()),
            crews: BTreeMap::from([("t1".to_owned(), CrewPlace::Invited)]),
            marks: vec!["admin".into(), "premium".into()],
            voice: None,
            dnd: true,
            real: None,
        })
        .unwrap();
        assert_eq!(marked["marks"], serde_json::json!(["admin", "premium"]));
        assert_eq!(marked["dnd"], true, "OVL-SOC: the gold dot with its bar");
        assert_eq!(
            (
                marked["seen"].clone(),
                marked["fav"].clone(),
                marked["alert"].clone(),
                marked["group"].clone(),
                marked["crews"].clone()
            ),
            (
                "today".into(),
                true.into(),
                true.into(),
                "g1".into(),
                serde_json::json!({"t1":"invited"})
            )
        );
        let result = serde_json::to_value(ToGame::FriendResult {
            friend: "u-cem".into(),
            action: FriendAction::AddFriend,
            ok: true,
            queued: true,
            reason: None,
        })
        .unwrap();
        assert_eq!(result["action"], "add-friend");
        let by = serde_json::to_value(MsgView {
            id: "i:1".into(),
            from: "Cem".into(),
            by: Some("u-cem".into()),
            ..MsgView::default()
        })
        .unwrap();
        assert_eq!(by["by"], "u-cem");
    }

    #[test]
    fn frames_to_the_game_use_the_wire_names() {
        let toast = ToGame::Toast(ToastView {
            kind: ToastKind::Call,
            key: "call:c1".into(),
            title: "Gece Kuşları".into(),
            body: "Mert".into(),
            conv: None,
            crew: None,
            sticker: None,
            badge: None,
            marks: Vec::new(),
            by: None,
            at: None,
        });
        let json = serde_json::to_value(&toast).unwrap();
        assert_eq!(json["t"], "toast");
        assert_eq!(json["kind"], "call");
        assert!(json.get("conv").is_none() && json.get("crew").is_none());
        assert!(json.get("sticker").is_none() && json.get("badge").is_none());

        let bye = serde_json::to_value(ToGame::Bye {
            reason: ByeReason::GameStartClose,
        })
        .unwrap();
        assert_eq!(
            bye,
            serde_json::json!({"t":"bye","reason":"game-start-close"})
        );

        let friend = serde_json::to_value(ToGame::Friend(FriendView {
            id: "f1".into(),
            name: "Ayşe".into(),
            place: Place::Projects,
            joinable: true,
            host: None,
            world: None,
            badge: None,
            dm: None,
            discord: false,
            seen: None,
            fav: false,
            alert: false,
            group: None,
            crews: BTreeMap::new(),
            marks: Vec::new(),
            voice: None,
            dnd: false,
            real: None,
        }))
        .unwrap();
        assert_eq!(friend["where"], "projects");
        assert_eq!(friend["t"], "friend");
        assert!(friend.get("host").is_none() && friend.get("world").is_none());
        assert!(friend.get("dnd").is_none(), "only while true");
        assert!(friend.get("badge").is_none() && friend.get("marks").is_none());
        assert!(friend.get("dm").is_none() && friend.get("discord").is_none());
        // BADGES-UI: the badge catalogue, names in both languages.
        let catalogue = serde_json::to_value(ToGame::Marks {
            defs: vec![pl_social::MarkDef {
                key: "admin".into(),
                name: pl_social::MarkName {
                    tr: "Projects yöneticisi".into(),
                    en: "Projects admin".into(),
                },
                color: "red".into(),
                glyph: "check".into(),
                shape: "seal".into(),
            }],
        })
        .unwrap();
        assert_eq!(
            catalogue,
            serde_json::json!({"t":"marks","defs":[{"key":"admin",
                "name":{"tr":"Projects yöneticisi","en":"Projects admin"},
                "color":"red","glyph":"check","shape":"seal"}]})
        );
        let inviting = serde_json::to_value(ToGame::Friend(FriendView {
            id: "f2".into(),
            name: "Mert".into(),
            place: Place::Single,
            joinable: true,
            host: None,
            world: Some(WorldMark { guests: 1, max: 7 }),
            badge: None,
            dm: Some("d:f2:me".into()),
            discord: true,
            seen: None,
            fav: false,
            alert: false,
            group: None,
            crews: BTreeMap::new(),
            marks: Vec::new(),
            voice: None,
            dnd: false,
            real: None,
        }))
        .unwrap();
        assert_eq!(inviting["world"], serde_json::json!({"guests":1,"max":7}));
        assert_eq!(inviting["dm"], "d:f2:me");
        assert_eq!(inviting["discord"], true);

        let gone = serde_json::to_value(ToGame::MsgGone {
            conv: "d:1:2".into(),
            id: "i:5".into(),
        })
        .unwrap();
        assert_eq!(
            gone,
            serde_json::json!({"t":"msg-gone","conv":"d:1:2","id":"i:5"})
        );
        let failed = serde_json::to_value(ToGame::SendFailed {
            cid: "c".into(),
            reason: SendFailure::DailyLimit,
            rule: None,
        })
        .unwrap();
        assert_eq!(failed["reason"], "daily-limit");
        assert!(failed.get("rule").is_none());
        let call = serde_json::to_value(ToGame::Call(CallView {
            id: "k".into(),
            crew: "Gece Kuşları".into(),
            from: "Mert".into(),
            since: 1,
            until: 2,
            answers: AnswerCounts::default(),
            mine: Some(CallAnswer::Coming),
            place: Some(Place::Server),
            host: Some("mc.example.net".into()),
            own: false,
        }))
        .unwrap();
        assert_eq!(call["where"], "server");
        assert!(call.get("own").is_none());
        assert_eq!(call["mine"], "coming");
        assert_eq!(call["answers"]["cant"], 0);
        let msg = serde_json::to_value(ToGame::Msg(Box::new(MsgView {
            conv: Some("d:1:2".into()),
            id: "i:1".into(),
            cid: None,
            from: "Ayşe".into(),
            text: "selam".into(),
            at: 5,
            mine: false,
            edited: false,
            deleted: false,
            ..MsgView::default()
        })))
        .unwrap();
        assert!(msg.get("edited").is_none() && msg.get("cid").is_none());
        assert!(msg.get("deleted").is_none());
        // An OVL2 message carries none of CHAT2's fields (nor TEAM6-O's).
        for field in [
            "sid",
            "sticker",
            "reply",
            "reactions",
            "links",
            "card",
            "badge",
        ] {
            assert!(msg.get(field).is_none(), "{field}");
        }
    }

    #[test]
    fn team6_frames_to_the_game_use_the_wire_names() {
        let card = BadgeView {
            tag: "GK".into(),
            color: "navy".into(),
            frame: "shield".into(),
            emblem: "moon".into(),
            name: "Gece Kartalları".into(),
            motto: String::new(),
            since: "2026-09".into(),
            members: 12,
            mine: false,
        };
        let badges = serde_json::to_value(ToGame::Badges {
            all: false,
            cards: BTreeMap::from([("t1".to_owned(), Some(card)), ("t2".to_owned(), None)]),
        })
        .unwrap();
        assert_eq!(
            badges,
            serde_json::json!({"t":"badges","all":false,"cards":{
                "t1":{"tag":"GK","color":"navy","frame":"shield","emblem":"moon",
                      "name":"Gece Kartalları","since":"2026-09","members":12},
                "t2":null}})
        );
        let note = serde_json::to_value(ToGame::Note {
            conv: "c:t1".into(),
            id: "j:u-ece:1800000000".into(),
            kind: NoteKind::Joined,
            name: "Ece".into(),
            mine: false,
            at: 1_800_000_000_000,
        })
        .unwrap();
        assert_eq!(
            note,
            serde_json::json!({"t":"note","conv":"c:t1","id":"j:u-ece:1800000000","kind":"joined",
                "name":"Ece","at":1_800_000_000_000_i64})
        );
        let msg = serde_json::to_value(ToGame::Msg(Box::new(MsgView {
            conv: Some("c:t1".into()),
            id: "i:9".into(),
            from: "Cem".into(),
            badge: Some("t1".into()),
            ..MsgView::default()
        })))
        .unwrap();
        assert_eq!(msg["badge"], "t1");
    }

    #[test]
    fn chat2_actions_of_the_panel_parse() {
        // An older mod's send still reads (no reply, no sticker).
        let plain: FromGame =
            serde_json::from_str(r#"{"t":"send","cid":"c1","conv":"d:1","text":"selam"}"#).unwrap();
        assert!(
            matches!(plain, FromGame::Send { reply: None, sticker: None, ref text, .. } if text == "selam")
        );
        let sticker: FromGame = serde_json::from_str(
            r#"{"t":"send","cid":"c2","conv":"d:1","sticker":"pl1/gg","reply":1800000000123}"#,
        )
        .unwrap();
        assert!(matches!(
            sticker,
            FromGame::Send { ref text, reply: Some(1_800_000_000_123), sticker: Some(ref s), .. }
                if text.is_empty() && s == "pl1/gg"
        ));
        let react: FromGame =
            serde_json::from_str(r#"{"t":"react","conv":"c:t1","sid":42,"e":"👍","on":true}"#)
                .unwrap();
        assert_eq!(
            react,
            FromGame::React {
                conv: "c:t1".into(),
                sid: 42,
                e: "👍".into(),
                on: true
            }
        );
        let typing: FromGame = serde_json::from_str(r#"{"t":"typing","conv":"c:t1"}"#).unwrap();
        assert_eq!(
            typing,
            FromGame::Typing {
                conv: "c:t1".into()
            }
        );
        // A reaction without its emoji is not one.
        assert!(
            serde_json::from_str::<FromGame>(r#"{"t":"react","conv":"c","sid":1,"on":true}"#)
                .is_err()
        );
    }

    #[test]
    fn chat2_frames_to_the_game_use_the_wire_names() {
        let typing = serde_json::to_value(ToGame::Typing {
            conv: "c:t1".into(),
            by: "u-cem".into(),
            name: "Cem".into(),
        })
        .unwrap();
        assert_eq!(
            typing,
            serde_json::json!({"t":"typing","conv":"c:t1","by":"u-cem","name":"Cem"})
        );
        let failed = serde_json::to_value(ToGame::ReactFailed {
            conv: "c:t1".into(),
            sid: 7,
            e: "👍".into(),
            reason: ReactFailure::NotAllowed,
        })
        .unwrap();
        assert_eq!(failed["t"], "react-failed");
        assert_eq!(failed["reason"], "not-allowed");
        let sent = serde_json::to_value(ToGame::Sent {
            cid: "c".into(),
            id: "n:x".into(),
            sid: Some(9),
        })
        .unwrap();
        assert_eq!(
            sent,
            serde_json::json!({"t":"sent","cid":"c","id":"n:x","sid":9})
        );
        let refused = serde_json::to_value(ToGame::SendFailed {
            cid: "c".into(),
            reason: SendFailure::Link,
            rule: Some("shortener".into()),
        })
        .unwrap();
        assert_eq!(refused["rule"], "shortener");
        assert_eq!(
            serde_json::to_value(SendFailure::LimitedLinks).unwrap(),
            "limited-links"
        );
        let msg = serde_json::to_value(ToGame::Msg(Box::new(MsgView {
            conv: Some("c:t1".into()),
            id: "i:8".into(),
            from: "Cem".into(),
            text: "bak https://youtu.be/x".into(),
            at: 8,
            sid: Some(8),
            reply: Some(ReplyView {
                id: Some("i:7".into()),
                sid: 7,
                from: "Ece".into(),
                mine: true,
                text: Some("geliyor musun".into()),
                ..ReplyView::default()
            }),
            reactions: vec![ReactionView {
                e: "👍".into(),
                n: 2,
                mine: true,
                who: vec!["Ayşe".into()],
            }],
            links: vec![LinkView {
                url: "https://youtu.be/x".into(),
                trusted: true,
            }],
            card: Some(CardView {
                site: "YouTube".into(),
                title: "Rekor".into(),
            }),
            ..MsgView::default()
        })))
        .unwrap();
        assert_eq!(msg["sid"], 8);
        assert_eq!(
            msg["reply"],
            serde_json::json!({"id":"i:7","sid":7,"from":"Ece","mine":true,"text":"geliyor musun"})
        );
        assert_eq!(
            msg["reactions"][0],
            serde_json::json!({"e":"👍","n":2,"mine":true,"who":["Ayşe"]})
        );
        assert_eq!(
            msg["links"][0],
            serde_json::json!({"url":"https://youtu.be/x","trusted":true})
        );
        assert_eq!(
            msg["card"],
            serde_json::json!({"site":"YouTube","title":"Rekor"})
        );
    }
}

//! What the launcher's page shows of world sharing (`host-changed`, `host-notice`) and what it
//! asks with: field for field the interface's `apps/launcher/src/lib/api/host-types.ts`. Nothing
//! here carries a path, a relay, an address, a key or a code the player would see (host1.md §1.1).
//!
//! Every optional field is left out when absent (never `null`), as the interface's types expect.

use serde::{Deserialize, Serialize};

/// The game mode guests play in (`survival`, `creative`, `adventure`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum WorldGameMode {
    Survival,
    Creative,
    Adventure,
}

impl WorldGameMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Survival => "survival",
            Self::Creative => "creative",
            Self::Adventure => "adventure",
        }
    }

    /// A mode as the mod names it; anything else (hardcore, spectator, unknown) plays survival.
    pub fn from_wire(mode: &str) -> Self {
        match mode {
            "creative" => Self::Creative,
            "adventure" => Self::Adventure,
            _ => Self::Survival,
        }
    }
}

/// Whether an instance's worlds offer "Arkadaşlarınla oyna".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct HostPlace {
    pub shown: bool,
    /// The game runs this instance with this world (its folder) open and the overlay connected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playing: Option<String>,
}

/// A friend in the player's world session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum WorldGuestState {
    /// The opening has not sent its invites yet.
    Pending,
    Invited,
    /// On their way in (their launcher connected, or their game is logging in).
    Joining,
    In,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorldGuest {
    pub friend: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    /// The in-game name they came with, once they reached the world.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub game_name: Option<String>,
    pub state: WorldGuestState,
}

/// Why the player's world did not open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum WorldOpenFailure {
    /// Today's opens are used up.
    Limit,
    /// No way works now ("Dünyan şu an açılamıyor.").
    Unavailable,
}

impl WorldOpenFailure {
    /// The bridge's `host-state.error` (H8).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Limit => "limit",
            Self::Unavailable => "unavailable",
        }
    }
}

/// Where an opening from the launcher is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum WorldOpeningStep {
    /// A backup first (the last one is older than a day).
    Backup,
    /// The game starts into the world.
    Starting,
    /// The world loaded: it opens on this computer, a way is prepared, the invites go out.
    Publishing,
}

/// The player's own world session (`HostSession` in the interface): the common fields and its
/// phase, written flat with `phase` naming the variant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "phase", rename_all = "camelCase")]
pub enum HostSessionView {
    #[serde(rename_all = "camelCase")]
    Opening {
        instance_id: String,
        folder: String,
        world_name: String,
        guests: Vec<WorldGuest>,
        max: u8,
        step: WorldOpeningStep,
        /// The backup's progress (0–100) while `step` is `backup`.
        #[serde(skip_serializing_if = "Option::is_none")]
        percent: Option<u8>,
    },
    #[serde(rename_all = "camelCase")]
    Open {
        instance_id: String,
        folder: String,
        world_name: String,
        guests: Vec<WorldGuest>,
        max: u8,
        mode: WorldGameMode,
        commands: bool,
    },
    /// It did not open; kept until "Tekrar dene", "Projects'te buluşun" or "Kapat".
    #[serde(rename_all = "camelCase")]
    Failed {
        instance_id: String,
        folder: String,
        world_name: String,
        guests: Vec<WorldGuest>,
        max: u8,
        reason: WorldOpenFailure,
    },
}

/// What a join's window asks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WorldJoinAsk {
    /// A new install with exactly the world's mods ("Kopyala ve katıl").
    Copy { mods: u32, bytes: u64, unknown: u32 },
    /// No install of the world's Minecraft version: the official pack is installed for it.
    Install { mc: String },
    /// The account's in-game name cannot come in.
    Name {
        verdict: WorldNameVerdict,
        name: String,
    },
}

/// Why a name cannot come in: the host's own (or not a valid name), or bound to another friend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum WorldNameVerdict {
    Own,
    Taken,
}

/// The answer to a join's question.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WorldJoinAnswer {
    /// "Kopyala ve katıl": the new install's name in the player's language.
    Copy { name: String },
    /// "Kur ve katıl".
    Install,
    /// Join with another of the player's accounts (it becomes the active one).
    #[serde(rename_all = "camelCase")]
    Account { account_id: String },
}

/// How a "Katıl" ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum WorldJoinFailure {
    Full,
    Closed,
    Removed,
    Unreachable,
    HostPackOld,
    Mods,
}

/// Where a "Katıl" is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WorldJoinStage {
    Preparing {
        #[serde(skip_serializing_if = "Option::is_none")]
        percent: Option<u8>,
    },
    Ask {
        ask: WorldJoinAsk,
    },
    /// The game starts with `instance_id` straight into the world through `127.0.0.1:port`.
    #[serde(rename_all = "camelCase")]
    Ready {
        instance_id: String,
        port: u16,
        missing: u32,
    },
    Connecting,
    In,
    /// The way broke and did not come back: "Yeniden katıl".
    Lost,
    Failed {
        failure: WorldJoinFailure,
    },
}

/// A friend's world invite.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorldInviteView {
    /// The world session; one invite per session.
    pub session: String,
    pub from: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    pub mc: String,
    pub guests: u8,
    pub max: u8,
    /// Unix seconds.
    pub at: i64,
    pub quiet: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub join: Option<WorldJoinStage>,
}

/// Everything the page shows; sent whole on every change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct HostView {
    /// Grows with every view.
    pub seq: u64,
    /// Signed content, service and sign-in all allow it: false shows nothing.
    pub enabled: bool,
    /// Guest limit of a new session (Studio, 1–7).
    pub max_guests: u8,
    pub session: Option<HostSessionView>,
    /// Newest first.
    pub invites: Vec<WorldInviteView>,
}

/// Why invites did not go out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum WorldInviteRefusal {
    /// Today's invites are used up.
    Daily,
    /// The session's live invites (20).
    Session,
    /// A limited account's daily ones (10).
    Limited,
}

/// The answer of `host_invite`: `sent`, or why not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum WorldInviteOutcome {
    Sent,
    Daily,
    Session,
    Limited,
}

impl From<WorldInviteRefusal> for WorldInviteOutcome {
    fn from(refusal: WorldInviteRefusal) -> Self {
        match refusal {
            WorldInviteRefusal::Daily => Self::Daily,
            WorldInviteRefusal::Session => Self::Session,
            WorldInviteRefusal::Limited => Self::Limited,
        }
    }
}

/// The answer of `host_open`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum WorldOpenOutcome {
    /// The caller starts the game into the world now (quick play).
    Start,
    /// The game runs with the world open: it opens in place.
    Running,
    /// "Vazgeç" during the backup.
    Cancelled,
}

/// Moments worth a toast or a notification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum HostNotice {
    /// A world invite arrived live (never a quiet one).
    Invite { invite: WorldInviteView },
    /// The player's world opened.
    #[serde(rename_all = "camelCase")]
    Opened { world_name: String },
    #[serde(rename = "openFailed")]
    OpenFailed { reason: WorldOpenFailure },
    #[serde(rename = "inviteRefused")]
    InviteRefused { why: WorldInviteRefusal },
    #[serde(rename = "joinFailed")]
    JoinFailed {
        session: String,
        name: String,
        failure: WorldJoinFailure,
    },
}

/// "Dünyayı aç" from the launcher.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorldOpenRequest {
    pub instance_id: String,
    pub folder: String,
    pub mode: WorldGameMode,
    pub commands: bool,
    /// Friend ids, at most the guest limit.
    pub invite: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn guest(state: WorldGuestState) -> WorldGuest {
        WorldGuest {
            friend: "f1".into(),
            name: "Mert".into(),
            avatar: None,
            avatar_url: None,
            game_name: None,
            state,
        }
    }

    #[test]
    fn views_read_like_the_interfaces_types() {
        let view = HostView {
            seq: 3,
            enabled: true,
            max_guests: 7,
            session: Some(HostSessionView::Opening {
                instance_id: "projects".into(),
                folder: "Yeni Dünya".into(),
                world_name: "Yeni Dünya".into(),
                guests: vec![guest(WorldGuestState::Pending)],
                max: 7,
                step: WorldOpeningStep::Backup,
                percent: Some(38),
            }),
            invites: vec![WorldInviteView {
                session: "s".into(),
                from: "f2".into(),
                name: "Ayşe".into(),
                avatar: Some("a3".into()),
                avatar_url: None,
                mc: "26.1.2".into(),
                guests: 1,
                max: 7,
                at: 5,
                quiet: false,
                join: Some(WorldJoinStage::Ready {
                    instance_id: "projects".into(),
                    port: 50123,
                    missing: 0,
                }),
            }],
        };
        assert_eq!(
            serde_json::to_value(&view).unwrap(),
            json!({
                "seq": 3, "enabled": true, "maxGuests": 7,
                "session": {
                    "phase": "opening", "instanceId": "projects", "folder": "Yeni Dünya",
                    "worldName": "Yeni Dünya", "max": 7, "step": "backup", "percent": 38,
                    "guests": [{"friend": "f1", "name": "Mert", "state": "pending"}]
                },
                "invites": [{
                    "session": "s", "from": "f2", "name": "Ayşe", "avatar": "a3", "mc": "26.1.2",
                    "guests": 1, "max": 7, "at": 5, "quiet": false,
                    "join": {"kind": "ready", "instanceId": "projects", "port": 50123, "missing": 0}
                }]
            })
        );
        let open = HostSessionView::Open {
            instance_id: "i".into(),
            folder: "w".into(),
            world_name: "W".into(),
            guests: vec![],
            max: 3,
            mode: WorldGameMode::Creative,
            commands: true,
        };
        assert_eq!(
            serde_json::to_value(&open).unwrap(),
            json!({"phase": "open", "instanceId": "i", "folder": "w", "worldName": "W",
                   "guests": [], "max": 3, "mode": "creative", "commands": true})
        );
        let failed = HostSessionView::Failed {
            instance_id: "i".into(),
            folder: "w".into(),
            world_name: "W".into(),
            guests: vec![],
            max: 3,
            reason: WorldOpenFailure::Limit,
        };
        assert_eq!(serde_json::to_value(&failed).unwrap()["reason"], "limit");
        let stages = [
            (
                WorldJoinStage::Preparing { percent: None },
                json!({"kind": "preparing"}),
            ),
            (
                WorldJoinStage::Ask {
                    ask: WorldJoinAsk::Copy {
                        mods: 3,
                        bytes: 1024,
                        unknown: 1,
                    },
                },
                json!({"kind": "ask", "ask": {"kind": "copy", "mods": 3, "bytes": 1024, "unknown": 1}}),
            ),
            (
                WorldJoinStage::Ask {
                    ask: WorldJoinAsk::Name {
                        verdict: WorldNameVerdict::Taken,
                        name: "Notch".into(),
                    },
                },
                json!({"kind": "ask", "ask": {"kind": "name", "verdict": "taken", "name": "Notch"}}),
            ),
            (WorldJoinStage::Lost, json!({"kind": "lost"})),
            (
                WorldJoinStage::Failed {
                    failure: WorldJoinFailure::HostPackOld,
                },
                json!({"kind": "failed", "failure": "hostPackOld"}),
            ),
        ];
        for (stage, wire) in stages {
            assert_eq!(serde_json::to_value(&stage).unwrap(), wire);
        }
        let notices = [
            (
                HostNotice::Opened {
                    world_name: "W".into(),
                },
                json!({"kind": "opened", "worldName": "W"}),
            ),
            (
                HostNotice::OpenFailed {
                    reason: WorldOpenFailure::Unavailable,
                },
                json!({"kind": "openFailed", "reason": "unavailable"}),
            ),
            (
                HostNotice::InviteRefused {
                    why: WorldInviteRefusal::Limited,
                },
                json!({"kind": "inviteRefused", "why": "limited"}),
            ),
            (
                HostNotice::JoinFailed {
                    session: "s".into(),
                    name: "Ayşe".into(),
                    failure: WorldJoinFailure::Removed,
                },
                json!({"kind": "joinFailed", "session": "s", "name": "Ayşe", "failure": "removed"}),
            ),
        ];
        for (notice, wire) in notices {
            assert_eq!(serde_json::to_value(&notice).unwrap(), wire);
        }
        assert_eq!(
            serde_json::to_value(WorldOpenOutcome::Running).unwrap(),
            "running"
        );
        assert_eq!(
            serde_json::to_value(WorldInviteOutcome::from(WorldInviteRefusal::Daily)).unwrap(),
            "daily"
        );
    }

    #[test]
    fn requests_read_what_the_interface_sends() {
        let open: WorldOpenRequest = serde_json::from_value(json!({
            "instanceId": "projects", "folder": "Yeni Dünya", "mode": "adventure",
            "commands": false, "invite": ["f1", "f2"]
        }))
        .unwrap();
        assert_eq!(open.mode, WorldGameMode::Adventure);
        assert_eq!(open.invite.len(), 2);
        let answers = [
            (
                json!({"kind": "copy", "name": "Arkadaş dünyası: Ayşe"}),
                WorldJoinAnswer::Copy {
                    name: "Arkadaş dünyası: Ayşe".into(),
                },
            ),
            (json!({"kind": "install"}), WorldJoinAnswer::Install),
            (
                json!({"kind": "account", "accountId": "acc-2"}),
                WorldJoinAnswer::Account {
                    account_id: "acc-2".into(),
                },
            ),
        ];
        for (wire, answer) in answers {
            assert_eq!(
                serde_json::from_value::<WorldJoinAnswer>(wire).unwrap(),
                answer
            );
        }
        assert_eq!(WorldGameMode::from_wire("hardcore"), WorldGameMode::Survival);
        assert_eq!(WorldGameMode::from_wire("creative").as_str(), "creative");
    }
}

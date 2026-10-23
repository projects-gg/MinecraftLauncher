//! Which social events become in-game toasts.
//!
//! The launcher's own rules live in TypeScript (`features/social/model.ts` `wantsNotice`,
//! `features/social/chat/notify.ts` `chatNoticeWay`), but the web view may rest while the window
//! is hidden, which is exactly when the game runs. So the overlay applies the same rules here, from
//! the social view (Rahatsız etmeyin, the service's crew settings and mutes) and the choices this
//! computer keeps, which the page pushes to Rust whenever they change ([`NotifyPrefs`]).
//!
//! The two sides must not drift: both run one table of cases, `tests/notify-rules.json` (this
//! module's tests and the launcher's `features/overlay/notify-rules.test.ts`). A rule changed on
//! one side only fails the other side's tests.
//!
//! The parts of the TypeScript rules that look at the launcher's window (in front, the
//! conversation on screen, a game running) have no meaning in game: the mod skips a toast for the
//! conversation its panel shows. Own messages and blocked people never get here (`pl-social` drops
//! them); the rules say no to them anyway.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::protocol::{ToastKind, ToastView};

/// The notification choices this computer keeps (Settings → Arkadaşlar and the chat's "Sessize
/// al"), mirrored from the launcher's page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct NotifyPrefs {
    /// "Ekip çağrıları" (on unless turned off). Used only while the service does not keep the
    /// crew settings itself.
    pub crew_calls: bool,
    /// "Ekip sohbeti bildirimleri" (off by default: the panel counts unread messages instead).
    pub crew_chat: bool,
    /// Conversations muted on this computer: until when (Unix milliseconds), or null until the
    /// player unmutes.
    pub chat_mutes: BTreeMap<String, Option<i64>>,
}

impl Default for NotifyPrefs {
    fn default() -> Self {
        Self {
            crew_calls: true,
            crew_chat: false,
            chat_mutes: BTreeMap::new(),
        }
    }
}

impl NotifyPrefs {
    /// The conversation is muted here at `now_ms`.
    pub fn conversation_muted(&self, conv: &str, now_ms: i64) -> bool {
        self.chat_mutes
            .get(conv)
            .is_some_and(|until| until.is_none_or(|until| until > now_ms))
    }
}

/// "Sessize al" as the service keeps it: on until `until` (Unix seconds), or until the player
/// unmutes (`until` None). A timed mute ends by itself.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mute {
    pub muted: bool,
    pub until: Option<i64>,
}

impl Mute {
    pub fn on_at(&self, now_ms: i64) -> bool {
        self.muted && self.until.is_none_or(|until| until * 1000 > now_ms)
    }
}

/// The settings the service keeps for all of the player's crews (`crewNotifications`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CrewSettings {
    /// Crew call notifications are on.
    pub calls: bool,
    /// Every crew at once.
    pub mute: Mute,
}

/// One of the player's crews, in the player's order, with its own mute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrewMute {
    pub id: String,
    pub mute: Mute,
}

/// What the rules look at besides the event.
#[derive(Debug, Clone, Copy)]
pub struct Situation<'a> {
    /// "Rahatsız etmeyin".
    pub dnd: bool,
    /// None: the service does not keep crew settings, so this computer's "Ekip çağrıları" decides.
    pub service: Option<CrewSettings>,
    pub crews: &'a [CrewMute],
    pub prefs: &'a NotifyPrefs,
    /// Unix milliseconds.
    pub now_ms: i64,
}

/// A social event that may deserve a toast, with what the toast would show.
#[derive(Debug, Clone, Copy)]
pub enum Notice<'a> {
    /// A chat message: direct when `crew` is None, else in that crew's chat (`crew` id and name).
    Message {
        conv: &'a str,
        crew: Option<(&'a str, &'a str)>,
        from: &'a str,
        text: &'a str,
        /// The player's own message (another device of theirs).
        mine: bool,
        /// From someone the player blocked.
        blocked: bool,
    },
    /// A teammate called; `crews` are the crew ids the call names (none: the first crew).
    Call {
        id: &'a str,
        crews: &'a [String],
        crew: &'a str,
        from: &'a str,
    },
    /// "Birlikte oyna". A quiet one waited (offline, do-not-disturb) and is never a toast.
    Invite {
        id: &'a str,
        from: &'a str,
        quiet: bool,
    },
}

/// Whether `notice` reaches the player in game.
pub fn heard(notice: &Notice<'_>, s: &Situation<'_>) -> bool {
    if s.dnd {
        return false;
    }
    match *notice {
        Notice::Message {
            conv,
            crew,
            mine,
            blocked,
            ..
        } => {
            if mine || blocked || s.prefs.conversation_muted(conv, s.now_ms) {
                return false;
            }
            let Some((crew, _)) = crew else {
                return true;
            };
            // A crew the list does not have (yet) is never silent, as on the page.
            s.prefs.crew_chat
                && s.crews
                    .iter()
                    .find(|one| one.id == crew)
                    .is_none_or(|one| !(all_muted(s) || one.mute.on_at(s.now_ms)))
        }
        Notice::Call { crews, .. } => calls_heard(s) && call_heard_in(crews, s),
        Notice::Invite { quiet, .. } => !quiet,
    }
}

/// Every crew is muted at once (where the service keeps it).
fn all_muted(s: &Situation<'_>) -> bool {
    s.service.is_some_and(|all| all.mute.on_at(s.now_ms))
}

/// The service's switch and no mute of all crews where it keeps them, else this computer's.
fn calls_heard(s: &Situation<'_>) -> bool {
    match s.service {
        None => s.prefs.crew_calls,
        Some(all) => all.calls && !all.mute.on_at(s.now_ms),
    }
}

/// A call is heard when at least one crew it names is not muted on its own. A call that names no
/// crew is the first crew's; one that names only crews the list does not have is heard.
fn call_heard_in(named: &[String], s: &Situation<'_>) -> bool {
    let crews: Vec<&CrewMute> = if named.is_empty() {
        s.crews.iter().take(1).collect()
    } else {
        s.crews
            .iter()
            .filter(|crew| named.contains(&crew.id))
            .collect()
    };
    crews.is_empty() || crews.iter().any(|crew| !crew.mute.on_at(s.now_ms))
}

/// The toast for `notice`, or None when the rules say it stays quiet. The mod words calls and
/// invites itself, in the game's language: for those `title` and `body` carry names only.
pub fn toast_for(notice: &Notice<'_>, s: &Situation<'_>) -> Option<ToastView> {
    if !heard(notice, s) {
        return None;
    }
    Some(match *notice {
        Notice::Message {
            conv,
            crew,
            from,
            text,
            ..
        } => ToastView {
            kind: ToastKind::Message,
            key: conv.to_owned(),
            title: from.to_owned(),
            body: text.to_owned(),
            conv: Some(conv.to_owned()),
            crew: crew.map(|(_, name)| name.to_owned()),
            sticker: None,
            badge: None,
        },
        Notice::Call { id, crew, from, .. } => ToastView {
            kind: ToastKind::Call,
            key: format!("call:{id}"),
            title: crew.to_owned(),
            body: from.to_owned(),
            conv: None,
            crew: None,
            sticker: None,
            badge: None,
        },
        Notice::Invite { id, from, .. } => ToastView {
            kind: ToastKind::Invite,
            key: format!("invite:{id}"),
            title: from.to_owned(),
            body: String::new(),
            conv: None,
            crew: None,
            sticker: None,
            badge: None,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shared table (`tests/notify-rules.json`), as both sides read it.
    #[derive(Deserialize)]
    struct Table {
        now: i64,
        cases: Vec<Case>,
    }

    #[derive(Deserialize)]
    struct Case {
        name: String,
        event: Event,
        #[serde(default)]
        situation: TableSituation,
        heard: bool,
    }

    #[derive(Deserialize)]
    #[serde(tag = "kind", rename_all = "camelCase")]
    enum Event {
        Direct {
            conv: String,
            #[serde(default)]
            mine: bool,
            #[serde(default)]
            blocked: bool,
        },
        Crew {
            conv: String,
            crew: String,
            #[serde(default)]
            mine: bool,
            #[serde(default)]
            blocked: bool,
        },
        Call {
            #[serde(default)]
            crews: Vec<String>,
        },
        Invite {
            #[serde(default)]
            quiet: bool,
        },
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", default)]
    struct TableSituation {
        status: String,
        crew_calls: bool,
        crew_chat: bool,
        chat_mutes: BTreeMap<String, Option<i64>>,
        service: Option<TableService>,
        crews: Vec<TableCrew>,
    }

    impl Default for TableSituation {
        fn default() -> Self {
            Self {
                status: "online".into(),
                crew_calls: true,
                crew_chat: false,
                chat_mutes: BTreeMap::new(),
                service: None,
                crews: Vec::new(),
            }
        }
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct TableService {
        calls: bool,
        muted: bool,
        mute_until: Option<i64>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct TableCrew {
        id: String,
        #[serde(default)]
        muted: bool,
        #[serde(default)]
        mute_until: Option<i64>,
    }

    #[test]
    fn the_shared_table_holds() {
        let table: Table =
            serde_json::from_str(include_str!("../tests/notify-rules.json")).unwrap();
        assert!(table.cases.len() >= 30, "the table lost cases");
        for case in &table.cases {
            let t = &case.situation;
            let prefs = NotifyPrefs {
                crew_calls: t.crew_calls,
                crew_chat: t.crew_chat,
                chat_mutes: t.chat_mutes.clone(),
            };
            let crews: Vec<CrewMute> = t
                .crews
                .iter()
                .map(|crew| CrewMute {
                    id: crew.id.clone(),
                    mute: Mute {
                        muted: crew.muted,
                        until: crew.mute_until,
                    },
                })
                .collect();
            let situation = Situation {
                dnd: t.status == "dnd",
                service: t.service.as_ref().map(|all| CrewSettings {
                    calls: all.calls,
                    mute: Mute {
                        muted: all.muted,
                        until: all.mute_until,
                    },
                }),
                crews: &crews,
                prefs: &prefs,
                now_ms: table.now,
            };
            let notice = match &case.event {
                Event::Direct {
                    conv,
                    mine,
                    blocked,
                } => Notice::Message {
                    conv,
                    crew: None,
                    from: "Ayşe",
                    text: "selam",
                    mine: *mine,
                    blocked: *blocked,
                },
                Event::Crew {
                    conv,
                    crew,
                    mine,
                    blocked,
                } => Notice::Message {
                    conv,
                    crew: Some((crew, "Gece Kuşları")),
                    from: "Mert",
                    text: "toplanın",
                    mine: *mine,
                    blocked: *blocked,
                },
                Event::Call { crews } => Notice::Call {
                    id: "k1",
                    crews,
                    crew: "Gece Kuşları",
                    from: "Mert",
                },
                Event::Invite { quiet } => Notice::Invite {
                    id: "i1",
                    from: "Can",
                    quiet: *quiet,
                },
            };
            assert_eq!(heard(&notice, &situation), case.heard, "{}", case.name);
            assert_eq!(
                toast_for(&notice, &situation).is_some(),
                case.heard,
                "{}",
                case.name
            );
        }
    }

    #[test]
    fn toasts_carry_names_and_keys() {
        let prefs = NotifyPrefs {
            crew_chat: true,
            ..NotifyPrefs::default()
        };
        let s = Situation {
            dnd: false,
            service: None,
            crews: &[],
            prefs: &prefs,
            now_ms: 0,
        };
        let dm = toast_for(
            &Notice::Message {
                conv: "d:1:2",
                crew: None,
                from: "Ayşe",
                text: "Akşam bedwars?",
                mine: false,
                blocked: false,
            },
            &s,
        )
        .unwrap();
        assert_eq!(
            (
                dm.kind,
                dm.key.as_str(),
                dm.title.as_str(),
                dm.body.as_str()
            ),
            (ToastKind::Message, "d:1:2", "Ayşe", "Akşam bedwars?")
        );
        assert_eq!(dm.conv.as_deref(), Some("d:1:2"));
        assert!(dm.crew.is_none());

        let crew = toast_for(
            &Notice::Message {
                conv: "c:t1",
                crew: Some(("t1", "Gece Kuşları")),
                from: "Mert",
                text: "Toplanın",
                mine: false,
                blocked: false,
            },
            &s,
        )
        .unwrap();
        assert_eq!(crew.title, "Mert");
        assert_eq!(crew.crew.as_deref(), Some("Gece Kuşları"));

        let call = toast_for(
            &Notice::Call {
                id: "k1",
                crews: &[],
                crew: "Gece Kuşları",
                from: "Mert",
            },
            &s,
        )
        .unwrap();
        assert_eq!(
            (
                call.kind,
                call.key.as_str(),
                call.title.as_str(),
                call.body.as_str()
            ),
            (ToastKind::Call, "call:k1", "Gece Kuşları", "Mert")
        );

        let invite = toast_for(
            &Notice::Invite {
                id: "i1",
                from: "Can",
                quiet: false,
            },
            &s,
        )
        .unwrap();
        assert_eq!(
            (invite.kind, invite.key.as_str(), invite.title.as_str()),
            (ToastKind::Invite, "invite:i1", "Can")
        );
        assert!(invite.body.is_empty());
    }

    #[test]
    fn prefs_read_the_pages_json() {
        let prefs: NotifyPrefs = serde_json::from_str(
            r#"{"crewCalls":false,"crewChat":true,"chatMutes":{"d:1:2":null,"c:t1":1700000000000}}"#,
        )
        .unwrap();
        assert!(!prefs.crew_calls && prefs.crew_chat);
        assert!(prefs.conversation_muted("d:1:2", i64::MAX));
        assert!(prefs.conversation_muted("c:t1", 1_699_999_999_999));
        assert!(!prefs.conversation_muted("c:t1", 1_700_000_000_000));
        assert_eq!(
            serde_json::from_str::<NotifyPrefs>("{}").unwrap(),
            NotifyPrefs::default()
        );
    }
}

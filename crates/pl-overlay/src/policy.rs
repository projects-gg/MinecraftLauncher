//! Which social events become in-game toasts.
//!
//! The launcher's own notification rules live in TypeScript (`features/social/model.ts` `wantsNotice`,
//! `features/social/chat/notify.ts` `chatNoticeWay`), but the web view sleeps while the window is hidden,
//! which is exactly when the game is running. So the overlay applies the same rules here, from preferences
//! the web view pushes to Rust whenever they change. Blocked users never reach this point: `pl-social`
//! already drops their messages.

use std::collections::HashSet;

use crate::protocol::{ToastKind, ToastView};

/// The player's notification choices, mirrored from the launcher's settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotifyPrefs {
    /// "Rahatsız etmeyin": nothing pops up; the panel still shows everything.
    pub dnd: bool,
    /// Crew call notifications ("Ekip çağrıları").
    pub crew_calls: bool,
    /// Crew chat notifications; off by default, the panel shows unread counts instead.
    pub crew_chat: bool,
    /// Show message text in toasts; off for streaming ("only the sender").
    pub show_text: bool,
    pub muted_crews: HashSet<String>,
    pub muted_conversations: HashSet<String>,
}

impl Default for NotifyPrefs {
    fn default() -> Self {
        Self {
            dnd: false,
            crew_calls: true,
            crew_chat: false,
            show_text: true,
            muted_crews: HashSet::new(),
            muted_conversations: HashSet::new(),
        }
    }
}

/// A social event that may deserve a toast.
#[derive(Debug, Clone, Copy)]
pub enum SocialEvent<'a> {
    DirectMessage {
        conv: &'a str,
        from: &'a str,
        text: &'a str,
    },
    CrewMessage {
        conv: &'a str,
        crew_id: &'a str,
        crew: &'a str,
        from: &'a str,
        text: &'a str,
    },
    CrewCall {
        call: &'a str,
        crew_id: &'a str,
        crew: &'a str,
        from: &'a str,
    },
    PlayInvite {
        invite: &'a str,
        from: &'a str,
    },
}

/// The toast for `event`, or None when the player's choices say it should stay quiet. The mod words
/// calls and invites itself, in the game's language: for those `body` carries the sender's name.
pub fn toast_for(event: &SocialEvent<'_>, prefs: &NotifyPrefs) -> Option<ToastView> {
    if prefs.dnd {
        return None;
    }
    let text = |t: &str| {
        if prefs.show_text {
            t.to_owned()
        } else {
            String::new()
        }
    };
    match *event {
        SocialEvent::DirectMessage {
            conv,
            from,
            text: body,
        } => (!prefs.muted_conversations.contains(conv)).then(|| ToastView {
            kind: ToastKind::Message,
            key: conv.to_owned(),
            title: from.to_owned(),
            body: text(body),
            conv: Some(conv.to_owned()),
        }),
        SocialEvent::CrewMessage {
            conv,
            crew_id,
            crew,
            from,
            text: body,
        } => (prefs.crew_chat
            && !prefs.muted_crews.contains(crew_id)
            && !prefs.muted_conversations.contains(conv))
        .then(|| ToastView {
            kind: ToastKind::Message,
            key: conv.to_owned(),
            title: format!("{crew} · {from}"),
            body: text(body),
            conv: Some(conv.to_owned()),
        }),
        SocialEvent::CrewCall {
            call,
            crew_id,
            crew,
            from,
        } => (prefs.crew_calls && !prefs.muted_crews.contains(crew_id)).then(|| ToastView {
            kind: ToastKind::Call,
            key: format!("call:{call}"),
            title: crew.to_owned(),
            body: from.to_owned(),
            conv: None,
        }),
        SocialEvent::PlayInvite { invite, from } => Some(ToastView {
            kind: ToastKind::Invite,
            key: format!("invite:{invite}"),
            title: from.to_owned(),
            body: String::new(),
            conv: None,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DM: SocialEvent<'static> = SocialEvent::DirectMessage {
        conv: "dm:1",
        from: "Ayşe",
        text: "Akşam bedwars?",
    };
    const CREW_MSG: SocialEvent<'static> = SocialEvent::CrewMessage {
        conv: "crew:7",
        crew_id: "7",
        crew: "Gece Kuşları",
        from: "Mert",
        text: "Toplanın",
    };
    const CALL: SocialEvent<'static> = SocialEvent::CrewCall {
        call: "c1",
        crew_id: "7",
        crew: "Gece Kuşları",
        from: "Mert",
    };

    #[test]
    fn do_not_disturb_silences_everything() {
        let prefs = NotifyPrefs {
            dnd: true,
            crew_chat: true,
            ..NotifyPrefs::default()
        };
        for event in [
            DM,
            CREW_MSG,
            CALL,
            SocialEvent::PlayInvite {
                invite: "i",
                from: "Can",
            },
        ] {
            assert_eq!(toast_for(&event, &prefs), None);
        }
    }

    #[test]
    fn direct_messages_pop_unless_the_conversation_is_muted() {
        let mut prefs = NotifyPrefs::default();
        let toast = toast_for(&DM, &prefs).unwrap();
        assert_eq!(
            (toast.kind, toast.title.as_str(), toast.body.as_str()),
            (ToastKind::Message, "Ayşe", "Akşam bedwars?")
        );
        prefs.muted_conversations.insert("dm:1".into());
        assert_eq!(toast_for(&DM, &prefs), None);
    }

    #[test]
    fn crew_chat_stays_quiet_by_default() {
        let mut prefs = NotifyPrefs::default();
        assert_eq!(toast_for(&CREW_MSG, &prefs), None);
        prefs.crew_chat = true;
        assert_eq!(
            toast_for(&CREW_MSG, &prefs).unwrap().title,
            "Gece Kuşları · Mert"
        );
        prefs.muted_crews.insert("7".into());
        assert_eq!(toast_for(&CREW_MSG, &prefs), None);
    }

    #[test]
    fn crew_calls_respect_the_crew_mute_and_the_switch() {
        let mut prefs = NotifyPrefs::default();
        let toast = toast_for(&CALL, &prefs).unwrap();
        assert_eq!(
            (toast.kind, toast.key.as_str(), toast.body.as_str()),
            (ToastKind::Call, "call:c1", "Mert")
        );
        prefs.muted_crews.insert("7".into());
        assert_eq!(toast_for(&CALL, &prefs), None);
        prefs.muted_crews.clear();
        prefs.crew_calls = false;
        assert_eq!(toast_for(&CALL, &prefs), None);
    }

    #[test]
    fn hidden_text_keeps_only_the_sender() {
        let prefs = NotifyPrefs {
            show_text: false,
            ..NotifyPrefs::default()
        };
        let toast = toast_for(&DM, &prefs).unwrap();
        assert_eq!(toast.title, "Ayşe");
        assert!(toast.body.is_empty());
    }
}

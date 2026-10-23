//! Which streams the host admits (PROTOCOL.md §6.8), as plain rules over a snapshot of the session.

use super::binding::is_valid_name;
use crate::tunnel::control::Refusal;

/// What the host knows about a game `hello` when it decides.
#[derive(Debug, Clone)]
pub(crate) struct GameAsk<'a> {
    pub slot_removed: bool,
    pub session_open: bool,
    /// The game's protocol number from the hello (0: not known).
    pub proto: i32,
    pub host_proto: i32,
    pub name: &'a str,
    pub host_name: &'a str,
    /// The name is in the world already from another stream.
    pub name_in_world: bool,
    /// The name is bound to another friend in this world.
    pub name_taken: bool,
    /// Guests in the world other than this slot.
    pub guests_besides: usize,
    pub max: usize,
    pub busy: bool,
}

/// Steps 1–6 of a game `hello`, in order (step 7, connecting to the world, comes after).
pub(crate) fn admit_game(ask: &GameAsk<'_>) -> Result<(), Refusal> {
    if ask.slot_removed {
        return Err(Refusal::Removed);
    }
    if !ask.session_open {
        return Err(Refusal::Closed);
    }
    if ask.proto > 0 && ask.proto != ask.host_proto {
        return Err(Refusal::Version);
    }
    if !is_valid_name(ask.name) || ask.name.eq_ignore_ascii_case(ask.host_name) || ask.name_in_world
    {
        return Err(Refusal::Name);
    }
    if ask.name_taken {
        return Err(Refusal::NameTaken);
    }
    if ask.guests_besides >= ask.max {
        return Err(Refusal::Full);
    }
    if ask.busy {
        return Err(Refusal::Busy);
    }
    Ok(())
}

/// A control `hello`: refused only when the slot is removed, the session closed or the stream
/// limits are reached; the name gets a verdict instead.
pub(crate) fn admit_control(
    slot_removed: bool,
    session_open: bool,
    busy: bool,
) -> Result<(), Refusal> {
    if slot_removed {
        Err(Refusal::Removed)
    } else if !session_open {
        Err(Refusal::Closed)
    } else if busy {
        Err(Refusal::Busy)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ask() -> GameAsk<'static> {
        GameAsk {
            slot_removed: false,
            session_open: true,
            proto: 9999,
            host_proto: 9999,
            name: "Notch",
            host_name: "Ayse",
            name_in_world: false,
            name_taken: false,
            guests_besides: 0,
            max: 7,
            busy: false,
        }
    }

    #[test]
    fn steps_apply_in_order() {
        assert_eq!(admit_game(&ask()), Ok(()));
        // Each case breaks one rule and every later one; the earliest wins.
        let all_bad = GameAsk {
            slot_removed: true,
            session_open: false,
            proto: 1,
            name: "Ayse",
            name_taken: true,
            guests_besides: 7,
            busy: true,
            ..ask()
        };
        assert_eq!(admit_game(&all_bad), Err(Refusal::Removed));
        let all_bad = GameAsk {
            slot_removed: false,
            ..all_bad
        };
        assert_eq!(admit_game(&all_bad), Err(Refusal::Closed));
        let all_bad = GameAsk {
            session_open: true,
            ..all_bad
        };
        assert_eq!(admit_game(&all_bad), Err(Refusal::Version));
        let all_bad = GameAsk {
            proto: 0,
            ..all_bad
        };
        assert_eq!(admit_game(&all_bad), Err(Refusal::Name));
        let all_bad = GameAsk {
            name: "Notch",
            ..all_bad
        };
        assert_eq!(admit_game(&all_bad), Err(Refusal::NameTaken));
        let all_bad = GameAsk {
            name_taken: false,
            ..all_bad
        };
        assert_eq!(admit_game(&all_bad), Err(Refusal::Full));
        let all_bad = GameAsk {
            guests_besides: 6,
            ..all_bad
        };
        assert_eq!(admit_game(&all_bad), Err(Refusal::Busy));
    }

    #[test]
    fn names() {
        for (name, ok) in [
            ("AYSE", false),
            ("No", false),
            ("Ayşe", false),
            ("jeb_", true),
        ] {
            assert_eq!(admit_game(&GameAsk { name, ..ask() }).is_ok(), ok, "{name}");
        }
        assert_eq!(
            admit_game(&GameAsk {
                name_in_world: true,
                ..ask()
            }),
            Err(Refusal::Name)
        );
    }

    #[test]
    fn control_streams() {
        assert_eq!(admit_control(false, true, false), Ok(()));
        assert_eq!(admit_control(true, false, true), Err(Refusal::Removed));
        assert_eq!(admit_control(false, false, true), Err(Refusal::Closed));
        assert_eq!(admit_control(false, true, true), Err(Refusal::Busy));
    }
}

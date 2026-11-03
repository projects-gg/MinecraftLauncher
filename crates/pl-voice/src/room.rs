//! One room as the player sees it: the room's facts, the people in it with their states, and the
//! player's own invites that ring, kept from `hello` and the messages after it.

use std::collections::BTreeMap;

use crate::wire::{FromRoom, PersonWire, Role, RoomKind, RoomWire};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Person {
    pub u: String,
    pub j: i64,
    pub mute: bool,
    pub deaf: bool,
    pub sm: bool,
    pub q: u8,
    /// Their card (VOICE-S2), when the room sent one: names a friend's friend.
    pub card: Option<Box<pl_social::PublicProfile>>,
}

impl From<PersonWire> for Person {
    fn from(wire: PersonWire) -> Self {
        Self {
            u: wire.u,
            j: wire.j,
            mute: wire.mute,
            deaf: wire.deaf,
            sm: wire.sm,
            q: wire.q,
            card: wire.card,
        }
    }
}

/// What a message changed for the player.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Applied {
    /// Someone came in (not the player).
    pub joined: Option<String>,
    /// Someone left (not the player).
    pub left: Option<String>,
    /// The player was muted for everyone just now.
    pub muted_me: bool,
    /// A ring of the player's ended with `joined` (that person came).
    pub ring_joined: Option<String>,
    /// Anything the page shows changed.
    pub changed: bool,
    /// People's playback changed (who is in, who is muted for everyone).
    pub playback: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Room {
    pub id: String,
    pub kind: RoomKind,
    pub crew: Option<String>,
    pub name: Option<String>,
    pub leader: Option<String>,
    pub mods: Vec<String>,
    pub max: u16,
    pub me: String,
    pub my_role: Role,
    /// Everyone but the player, by id.
    pub people: BTreeMap<String, Person>,
    /// The player's own state as the room knows it (from `hello` and `state`).
    pub my_sm: bool,
    /// The player's own invites that ring: person → until (Unix seconds).
    pub ringing: BTreeMap<String, i64>,
}

impl Room {
    /// The room from its `hello` (the player is `me`).
    pub fn from_hello(
        me: &str,
        room: RoomWire,
        role: Role,
        people: Vec<PersonWire>,
        rings: Vec<crate::wire::RingWire>,
    ) -> Self {
        let mut my_sm = false;
        let mut others = BTreeMap::new();
        for person in people {
            if person.u == me {
                my_sm = person.sm;
            } else {
                others.insert(person.u.clone(), Person::from(person));
            }
        }
        Self {
            kind: room.kind,
            crew: room.crew,
            name: room.name,
            leader: room.leader,
            mods: room.mods,
            max: room.max,
            id: room.id,
            me: me.to_owned(),
            my_role: role,
            people: others,
            my_sm,
            ringing: rings
                .into_iter()
                .map(|ring| (ring.to, ring.until))
                .collect(),
        }
    }

    /// Someone's role as the room says it (crew rooms: the crew's).
    pub fn role_of(&self, u: &str) -> Role {
        if u == self.me {
            self.my_role
        } else if self.leader.as_deref() == Some(u) {
            Role::Leader
        } else if self.mods.iter().any(|m| m == u) {
            Role::Moderator
        } else {
            Role::Member
        }
    }

    /// People, longest present first.
    pub fn people_in_order(&self) -> Vec<&Person> {
        let mut people: Vec<&Person> = self.people.values().collect();
        people.sort_by(|a, b| a.j.cmp(&b.j).then_with(|| a.u.cmp(&b.u)));
        people
    }

    /// Applies one room message (`hello` and the stay's ends are the caller's).
    pub fn apply(&mut self, message: &FromRoom) -> Applied {
        let mut applied = Applied::default();
        match message {
            FromRoom::Joined { person } => {
                if person.u != self.me {
                    let fresh = !self.people.contains_key(&person.u);
                    self.people
                        .insert(person.u.clone(), Person::from(person.clone()));
                    if fresh {
                        applied.joined = Some(person.u.clone());
                    }
                    applied.changed = true;
                    applied.playback = true;
                }
            }
            FromRoom::Left { u, .. } => {
                if self.people.remove(u).is_some() {
                    applied.left = Some(u.clone());
                    applied.changed = true;
                    applied.playback = true;
                }
            }
            FromRoom::State {
                u,
                mute,
                deaf,
                sm,
                q,
            } => {
                if *u == self.me {
                    if let Some(sm) = sm {
                        applied.muted_me = *sm && !self.my_sm;
                        applied.changed |= *sm != self.my_sm;
                        self.my_sm = *sm;
                    }
                } else if let Some(person) = self.people.get_mut(u) {
                    let before = person.clone();
                    if let Some(mute) = mute {
                        person.mute = *mute;
                    }
                    if let Some(deaf) = deaf {
                        person.deaf = *deaf;
                    }
                    if let Some(sm) = sm {
                        person.sm = *sm;
                    }
                    if let Some(q) = q {
                        person.q = *q;
                    }
                    applied.changed = *person != before;
                    applied.playback = person.sm != before.sm;
                }
            }
            FromRoom::Room {
                name,
                leader,
                mods,
                max,
            } => {
                self.name = name.clone();
                self.leader = leader.clone();
                self.mods = mods.clone();
                // VOICE-ROOMS: the crew changed the room's limit ("Oda dolu." follows it).
                if let Some(max) = max {
                    self.max = *max;
                }
                applied.changed = true;
            }
            FromRoom::Me { role } => {
                applied.changed = *role != self.my_role;
                self.my_role = *role;
            }
            FromRoom::Ring { to, until } => {
                self.ringing.insert(to.clone(), *until);
                applied.changed = true;
            }
            FromRoom::RingEnd { to, why } => {
                if self.ringing.remove(to).is_some() {
                    applied.changed = true;
                }
                if *why == crate::wire::RingEndWhy::Joined {
                    applied.ring_joined = Some(to.clone());
                }
            }
            _ => {}
        }
        applied
    }

    /// Rings past their time (the room ends them too; this keeps the panel honest meanwhile).
    pub fn expire_rings(&mut self, now_unix: i64) -> bool {
        let before = self.ringing.len();
        self.ringing.retain(|_, until| *until > now_unix);
        self.ringing.len() != before
    }

    /// The crew's "Genel".
    pub fn is_general(&self) -> bool {
        self.id.starts_with('g')
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::{LeftWhy, RingEndWhy, RingWire};

    fn person(u: &str, j: i64) -> PersonWire {
        PersonWire {
            u: u.into(),
            j,
            mute: false,
            deaf: false,
            sm: false,
            q: 0,
            card: None,
        }
    }

    fn room() -> Room {
        Room::from_hello(
            "me",
            RoomWire {
                id: "f0123456789abcdef".into(),
                kind: RoomKind::Friend,
                crew: None,
                name: None,
                leader: Some("me".into()),
                mods: vec!["b".into()],
                max: 10,
            },
            Role::Leader,
            vec![person("me", 1), person("b", 3), person("a", 2)],
            vec![RingWire {
                to: "c".into(),
                until: 100,
            }],
        )
    }

    #[test]
    fn hello_keeps_others_and_the_players_own_state() {
        let room = room();
        assert_eq!(room.people.len(), 2);
        assert!(!room.people.contains_key("me"));
        let order: Vec<&str> = room
            .people_in_order()
            .iter()
            .map(|p| p.u.as_str())
            .collect();
        assert_eq!(order, ["a", "b"]);
        assert_eq!(room.role_of("me"), Role::Leader);
        assert_eq!(room.role_of("b"), Role::Moderator);
        assert_eq!(room.role_of("a"), Role::Member);
        assert_eq!(room.ringing.get("c"), Some(&100));
    }

    #[test]
    fn messages_change_the_room() {
        let mut room = room();
        let applied = room.apply(&FromRoom::Joined {
            person: person("c", 9),
        });
        assert_eq!(applied.joined.as_deref(), Some("c"));
        assert!(applied.playback);
        let applied = room.apply(&FromRoom::RingEnd {
            to: "c".into(),
            why: RingEndWhy::Joined,
        });
        assert_eq!(applied.ring_joined.as_deref(), Some("c"));
        assert!(room.ringing.is_empty());
        let applied = room.apply(&FromRoom::State {
            u: "a".into(),
            mute: Some(true),
            deaf: None,
            sm: None,
            q: Some(1),
        });
        assert!(applied.changed && !applied.playback);
        assert!(room.people["a"].mute);
        assert_eq!(room.people["a"].q, 1);
        let applied = room.apply(&FromRoom::State {
            u: "a".into(),
            mute: None,
            deaf: None,
            sm: Some(true),
            q: None,
        });
        assert!(applied.playback);
        // The player muted for everyone.
        let applied = room.apply(&FromRoom::State {
            u: "me".into(),
            mute: None,
            deaf: None,
            sm: Some(true),
            q: None,
        });
        assert!(applied.muted_me && room.my_sm);
        let again = room.apply(&FromRoom::State {
            u: "me".into(),
            mute: None,
            deaf: None,
            sm: Some(true),
            q: None,
        });
        assert!(!again.muted_me);
        let applied = room.apply(&FromRoom::Left {
            u: "b".into(),
            why: LeftWhy::Gone,
        });
        assert_eq!(applied.left.as_deref(), Some("b"));
        // A stranger's state changes nothing.
        assert!(
            !room
                .apply(&FromRoom::State {
                    u: "zz".into(),
                    mute: Some(true),
                    deaf: None,
                    sm: None,
                    q: None
                })
                .changed
        );
        room.apply(&FromRoom::Me { role: Role::Member });
        assert_eq!(room.role_of("me"), Role::Member);
        let max = room.max;
        room.apply(&FromRoom::Room {
            name: Some("Gece".into()),
            leader: Some("a".into()),
            mods: vec![],
            max: None,
        });
        assert_eq!(room.role_of("a"), Role::Leader);
        assert_eq!(room.name.as_deref(), Some("Gece"));
        assert_eq!(room.max, max, "an older service's message keeps the limit");
        // VOICE-ROOMS: the crew changed the room's limit.
        room.apply(&FromRoom::Room {
            name: Some("Gece".into()),
            leader: Some("a".into()),
            mods: vec![],
            max: Some(4),
        });
        assert_eq!(room.max, 4);
    }

    #[test]
    fn rings_expire() {
        let mut room = room();
        assert!(!room.expire_rings(99));
        assert!(room.expire_rings(100));
        assert!(room.ringing.is_empty());
    }
}

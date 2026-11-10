//! OVL-RPC: what the activity may say about where the game is, under the player's choices in
//! Settings → Discord. Projects' own servers are public and always named; another server's
//! address is the player's to share (off by default: Discord shows it to everyone who sees the
//! player, not only friends), and a picture never names a server: Projects' logo on Projects, a
//! generic block anywhere else in the game.

/// Where the game is, as the launcher read it from the game's log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GamePlace<'a> {
    /// The game's menus, or a server whose address is not known (a LAN world, a local address).
    Menus,
    Singleplayer,
    /// One of Projects' own servers, `address` as the game joined it.
    Projects {
        address: &'a str,
    },
    /// Another server.
    Server {
        address: &'a str,
    },
}

/// The player's choices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlacePrivacy {
    /// "Bağlandığım Sunucuyu Göster": where the game is (Projects, singleplayer, a server).
    pub show_place: bool,
    /// "Sunucu Adresini Göster": another server's address too.
    pub show_address: bool,
}

/// What the activity says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShownPlace<'a> {
    /// Only that the game is being played.
    Minecraft,
    Singleplayer,
    /// Projects, with its address (and its player count).
    Projects {
        address: &'a str,
    },
    /// Another server, with its address only when the player shares it.
    Server {
        address: Option<&'a str>,
    },
}

impl ShownPlace<'_> {
    /// The generic block picture instead of Projects' logo: singleplayer and other servers.
    pub fn generic_picture(&self) -> bool {
        matches!(self, Self::Singleplayer | Self::Server { .. })
    }
}

pub fn shown_place(place: GamePlace<'_>, privacy: PlacePrivacy) -> ShownPlace<'_> {
    if !privacy.show_place {
        return ShownPlace::Minecraft;
    }
    match place {
        GamePlace::Menus => ShownPlace::Minecraft,
        GamePlace::Singleplayer => ShownPlace::Singleplayer,
        GamePlace::Projects { address } => ShownPlace::Projects { address },
        GamePlace::Server { address } => ShownPlace::Server {
            address: privacy.show_address.then_some(address),
        },
    }
}

/// RPC-PLUS: the crew the player plays with now (the friends service's count; no crewmate's name).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CrewNow<'a> {
    /// The crew's own name, when the player wears its tag.
    pub name: Option<&'a str>,
    /// Members in the game together, the player included.
    pub together: u32,
    /// The crew's members.
    pub size: u32,
}

/// The player's crew choices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CrewPrivacy {
    /// "Ekibimle Oynadığımı Göster".
    pub show_crew: bool,
    /// "Kaç Kişi Olduğumuzu Göster": together / the crew's size as Discord's party.
    pub show_count: bool,
    /// "Ekibimin Adını Göster": the worn crew's name.
    pub show_name: bool,
}

/// What the crew line says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShownCrew<'a> {
    pub name: Option<&'a str>,
    /// Discord's party size: together, of the crew's size.
    pub party: Option<(u32, u32)>,
}

/// The crew line, only while at least one crewmate plays with the player (alone is never "with the
/// crew").
pub fn shown_crew(crew: Option<CrewNow<'_>>, privacy: CrewPrivacy) -> Option<ShownCrew<'_>> {
    let crew = crew.filter(|crew| privacy.show_crew && crew.together >= 2)?;
    Some(ShownCrew {
        name: crew.name.filter(|_| privacy.show_name),
        party: privacy
            .show_count
            .then_some((crew.together, crew.size.max(crew.together))),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CREW_ALL: CrewPrivacy = CrewPrivacy {
        show_crew: true,
        show_count: true,
        show_name: true,
    };

    #[test]
    fn the_crew_line_needs_a_crewmate_and_the_players_choice() {
        let three = CrewNow {
            name: Some("Ejderler"),
            together: 3,
            size: 5,
        };
        assert_eq!(
            shown_crew(Some(three), CREW_ALL),
            Some(ShownCrew {
                name: Some("Ejderler"),
                party: Some((3, 5))
            })
        );
        let alone = CrewNow {
            together: 1,
            ..three
        };
        assert_eq!(shown_crew(Some(alone), CREW_ALL), None);
        assert_eq!(shown_crew(None, CREW_ALL), None);
        let off = CrewPrivacy {
            show_crew: false,
            ..CREW_ALL
        };
        assert_eq!(shown_crew(Some(three), off), None);
        let plain = CrewPrivacy {
            show_crew: true,
            show_count: false,
            show_name: false,
        };
        assert_eq!(
            shown_crew(Some(three), plain),
            Some(ShownCrew {
                name: None,
                party: None
            })
        );
    }

    const ALL: PlacePrivacy = PlacePrivacy {
        show_place: true,
        show_address: true,
    };
    const DEFAULT: PlacePrivacy = PlacePrivacy {
        show_place: true,
        show_address: false,
    };
    const NOTHING: PlacePrivacy = PlacePrivacy {
        show_place: false,
        show_address: true,
    };

    #[test]
    fn another_servers_address_only_when_shared() {
        let other = GamePlace::Server {
            address: "mc.example.net",
        };
        assert_eq!(
            shown_place(other, DEFAULT),
            ShownPlace::Server { address: None }
        );
        assert_eq!(
            shown_place(other, ALL),
            ShownPlace::Server {
                address: Some("mc.example.net")
            }
        );
        assert_eq!(shown_place(other, NOTHING), ShownPlace::Minecraft);
    }

    #[test]
    fn projects_is_named_while_the_place_shows() {
        let projects = GamePlace::Projects {
            address: "play.projects.gg",
        };
        let named = ShownPlace::Projects {
            address: "play.projects.gg",
        };
        assert_eq!(shown_place(projects, DEFAULT), named);
        assert_eq!(shown_place(projects, ALL), named);
        assert_eq!(shown_place(projects, NOTHING), ShownPlace::Minecraft);
        assert!(!named.generic_picture());
    }

    #[test]
    fn singleplayer_and_menus() {
        assert_eq!(
            shown_place(GamePlace::Singleplayer, DEFAULT),
            ShownPlace::Singleplayer
        );
        assert_eq!(
            shown_place(GamePlace::Singleplayer, NOTHING),
            ShownPlace::Minecraft
        );
        assert_eq!(shown_place(GamePlace::Menus, ALL), ShownPlace::Minecraft);
        assert!(ShownPlace::Singleplayer.generic_picture());
        assert!(ShownPlace::Server { address: None }.generic_picture());
        assert!(!ShownPlace::Minecraft.generic_picture());
    }
}

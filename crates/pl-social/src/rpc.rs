//! RPC-PLUS: the crew line of the player's Discord activity ("Ekibiyle oynuyor", 3 / 5).
//!
//! Only what is true and harmless: the player is on Projects and at least one crewmate plays
//! there too (as "Birlikte oynanan süre" counts it; a crewmate's other server never reaches the
//! launcher), so a player alone never reads as playing with their crew. No crewmate's name: the
//! count, the crew's size, and the crew's own name only for the crew whose tag the player wears
//! (they show it publicly already; a hidden crew's never).

use crate::protocol::{GameLocation, PresenceState, is_projects_host};
use crate::view::{ActivityView, SocialView, TeamView};

/// One crew as the activity may show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrewTogether {
    /// The crew's name, only when the player wears its tag and the crew is not hidden.
    pub name: Option<String>,
    /// Members in the game with the player now, the player included (at least 2).
    pub together: u32,
    /// The crew's members.
    pub size: u32,
}

impl SocialView {
    /// The crew the player plays with now, if any (see the module).
    pub fn crew_together(&self) -> Option<CrewTogether> {
        let worn = self
            .me
            .as_ref()
            .and_then(|me| me.badge.as_ref())
            .map(|badge| badge.crew.as_str());
        crew_together(&self.activity, &self.crews, worn)
    }
}

/// [`SocialView::crew_together`] from its parts: `worn` is the crew whose tag the player wears.
pub fn crew_together(
    activity: &ActivityView,
    crews: &[TeamView],
    worn: Option<&str>,
) -> Option<CrewTogether> {
    let on_projects =
        matches!(activity, ActivityView::Playing { host: Some(host) } if is_projects_host(host));
    if !on_projects {
        return None;
    }
    let mut best: Option<(&TeamView, u32)> = None;
    for crew in crews {
        let mates = crew
            .members
            .iter()
            .filter(|member| {
                !member.me
                    && !member.blocked
                    && member.state == PresenceState::Playing
                    && member
                        .game
                        .as_ref()
                        .is_some_and(|game| game.location == GameLocation::Projects)
            })
            .count();
        let mates = u32::try_from(mates).unwrap_or(u32::MAX);
        if mates == 0 {
            continue;
        }
        let better = match best {
            None => true,
            Some((_, most)) if mates > most => true,
            // A tie goes to the crew whose tag the player wears.
            Some((_, most)) => mates == most && worn == Some(crew.id.as_str()),
        };
        if better {
            best = Some((crew, mates));
        }
    }
    let (crew, mates) = best?;
    let size = u32::try_from(crew.members.len()).unwrap_or(u32::MAX);
    Some(CrewTogether {
        name: (worn == Some(crew.id.as_str()) && !crew.hidden).then(|| crew.name.clone()),
        together: mates.saturating_add(1),
        size: size.max(mates.saturating_add(1)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Game;
    use crate::view::TeamMemberView;
    use crate::{CrewMute, JoinMode, TeamPermissions, TeamRole};

    fn member(id: &str, state: PresenceState, location: Option<GameLocation>) -> TeamMemberView {
        TeamMemberView {
            id: id.into(),
            name: id.into(),
            username: id.into(),
            avatar: "discord".into(),
            avatar_url: None,
            role: TeamRole::Member,
            moderator: false,
            me: id == "me",
            friend: false,
            blocked: false,
            state,
            dnd: false,
            game: location.map(|location| Game {
                location,
                host: None,
                ver: None,
            }),
            discord_url: String::new(),
            discord_id: None,
            joined_at: 0,
            together_month: 0,
            badge: None,
            marks: Vec::new(),
        }
    }

    fn playing(id: &str) -> TeamMemberView {
        member(id, PresenceState::Playing, Some(GameLocation::Projects))
    }

    fn crew(id: &str, name: &str, members: Vec<TeamMemberView>) -> TeamView {
        TeamView {
            id: id.into(),
            name: name.into(),
            tag: "GK".into(),
            emblem: "sword".into(),
            color: "gold".into(),
            frame: "cut".into(),
            motto: String::new(),
            code: "EKP-AAAA-BBBB".into(),
            created_on: "2026-10-20".into(),
            role: TeamRole::Member,
            moderator: false,
            permissions: TeamPermissions::default(),
            members,
            invited: Vec::new(),
            limit: 30,
            notifications: CrewMute::default(),
            text_locked_until: None,
            join: JoinMode::Open,
            link: None,
            link_url: None,
            requests: Vec::new(),
            together_week: 0,
            hidden: false,
            badge_requests: true,
            wearers: 0,
            slow: 0,
        }
    }

    fn on_projects() -> ActivityView {
        ActivityView::Playing {
            host: Some("play.projects.gg".into()),
        }
    }

    #[test]
    fn alone_is_not_with_the_crew() {
        let crews = [crew(
            "c1",
            "Ejderler",
            vec![
                playing("me"),
                member("a", PresenceState::Launcher, None),
                member(
                    "b",
                    PresenceState::Playing,
                    Some(GameLocation::Singleplayer),
                ),
                member("c", PresenceState::Playing, Some(GameLocation::Other)),
                member("d", PresenceState::Offline, None),
            ],
        )];
        assert_eq!(crew_together(&on_projects(), &crews, None), None);
    }

    #[test]
    fn counts_crewmates_on_projects_without_names() {
        let mut blocked = playing("x");
        blocked.blocked = true;
        let crews = [crew(
            "c1",
            "Ejderler",
            vec![
                playing("me"),
                playing("a"),
                playing("b"),
                blocked,
                member("d", PresenceState::Launcher, None),
            ],
        )];
        assert_eq!(
            crew_together(&on_projects(), &crews, None),
            Some(CrewTogether {
                name: None,
                together: 3,
                size: 5
            })
        );
    }

    #[test]
    fn only_on_projects() {
        let crews = [crew("c1", "Ejderler", vec![playing("me"), playing("a")])];
        for activity in [
            ActivityView::Launcher,
            ActivityView::Singleplayer,
            ActivityView::Playing { host: None },
            ActivityView::Playing {
                host: Some("mc.example.net".into()),
            },
        ] {
            assert_eq!(crew_together(&activity, &crews, Some("c1")), None);
        }
    }

    #[test]
    fn the_worn_crew_names_itself_unless_hidden() {
        let mut worn = crew("c2", "Kurtlar", vec![playing("me"), playing("a")]);
        let other = crew("c1", "Ejderler", vec![playing("me"), playing("b")]);
        // A tie goes to the worn crew, which names itself.
        let crews = [other.clone(), worn.clone()];
        assert_eq!(
            crew_together(&on_projects(), &crews, Some("c2")),
            Some(CrewTogether {
                name: Some("Kurtlar".into()),
                together: 2,
                size: 2
            })
        );
        // Not worn: no name.
        assert_eq!(
            crew_together(&on_projects(), &crews, None).and_then(|c| c.name),
            None
        );
        worn.hidden = true;
        assert_eq!(
            crew_together(&on_projects(), &[other, worn], Some("c2")).and_then(|c| c.name),
            None
        );
    }

    #[test]
    fn the_crew_with_most_crewmates_there_wins() {
        let small = crew("c1", "Ejderler", vec![playing("me"), playing("a")]);
        let big = crew(
            "c2",
            "Kurtlar",
            vec![playing("me"), playing("b"), playing("c"), playing("d")],
        );
        let shown = crew_together(&on_projects(), &[small, big], Some("c1")).unwrap();
        assert_eq!((shown.together, shown.size, shown.name), (4, 4, None));
    }
}

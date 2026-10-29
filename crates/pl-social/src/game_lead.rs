//! The friends' "in game" status with several games at once (PLAY-AGAIN). One person has one
//! status, so it follows one game: the oldest whose window is on screen. When that one ends while
//! another is on screen, the status moves to it, starting from what that game's own log said last
//! (its server, its world, the menus). Pure: the launcher hands each step to the service
//! ([`crate::SocialService::game_started`], `game_line`, `game_stopped`).

use crate::game::may_signal;

/// What the friends feature hears next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresenceStep {
    /// A game leads now: "in game", joining `quick_play` (a server label) when it started into one.
    Started { quick_play: Option<String> },
    /// A line of the leading game's log that may change where it is.
    Line(String),
    /// No game is on screen any more: back in the launcher.
    Stopped,
}

#[derive(Debug)]
struct Game {
    id: u64,
    quick_play: Option<String>,
    /// SOC9: the Minecraft version it plays, as friends see it.
    version: Option<String>,
    shown: bool,
    /// Its log's last line that told where it is.
    last_signal: Option<String>,
}

/// The games the status may follow, in the order they started, and the one it follows.
#[derive(Debug, Default)]
pub struct GameLead {
    games: Vec<Game>,
    lead: Option<u64>,
}

impl GameLead {
    pub fn new() -> Self {
        Self::default()
    }

    /// A game started; its window is not on screen yet (the player is still in the launcher).
    pub fn started(&mut self, id: u64, quick_play: Option<String>) {
        if self.games.iter().any(|game| game.id == id) {
            return;
        }
        self.games.push(Game {
            id,
            quick_play,
            version: None,
            shown: false,
            last_signal: None,
        });
    }

    /// SOC9: the Minecraft version game `id` plays (its instance's version).
    pub fn set_version(&mut self, id: u64, version: Option<String>) {
        if let Some(game) = self.games.iter_mut().find(|game| game.id == id) {
            game.version = version;
        }
    }

    /// The version of the game the status follows, if any.
    pub fn lead_version(&self) -> Option<String> {
        let lead = self.lead?;
        self.games
            .iter()
            .find(|game| game.id == lead)
            .and_then(|game| game.version.clone())
    }

    /// The game's window is on screen: it leads when no other game does.
    pub fn shown(&mut self, id: u64) -> Vec<PresenceStep> {
        let Some(game) = self.games.iter_mut().find(|game| game.id == id) else {
            return Vec::new();
        };
        game.shown = true;
        if self.lead.is_some() {
            return Vec::new();
        }
        self.lead = Some(id);
        vec![PresenceStep::Started {
            quick_play: game.quick_play.clone(),
        }]
    }

    /// A line of the game's log: kept when it tells where the game is, passed on while it leads.
    pub fn line(&mut self, id: u64, message: &str) -> Vec<PresenceStep> {
        if !may_signal(message) {
            return Vec::new();
        }
        let Some(game) = self.games.iter_mut().find(|game| game.id == id) else {
            return Vec::new();
        };
        game.last_signal = Some(message.to_owned());
        if self.lead == Some(id) {
            vec![PresenceStep::Line(message.to_owned())]
        } else {
            Vec::new()
        }
    }

    /// The game ended. When it led, the oldest other game on screen leads from where its log
    /// left it; with none, the player is back in the launcher.
    pub fn exited(&mut self, id: u64) -> Vec<PresenceStep> {
        self.games.retain(|game| game.id != id);
        if self.lead != Some(id) {
            return Vec::new();
        }
        let Some(next) = self.games.iter().find(|game| game.shown) else {
            self.lead = None;
            return vec![PresenceStep::Stopped];
        };
        self.lead = Some(next.id);
        let mut steps = vec![PresenceStep::Started {
            quick_play: next.quick_play.clone(),
        }];
        if let Some(line) = &next.last_signal {
            steps.push(PresenceStep::Line(line.clone()));
        }
        steps
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn started(quick_play: Option<&str>) -> PresenceStep {
        PresenceStep::Started {
            quick_play: quick_play.map(str::to_owned),
        }
    }

    #[test]
    fn one_game_reads_as_it_always_did() {
        let mut lead = GameLead::new();
        lead.started(1, Some("play.projects.gg".into()));
        // Lines while it opens are kept, not passed on (the player is still in the launcher).
        assert!(
            lead.line(1, "Connecting to play.projects.gg, 25565")
                .is_empty()
        );
        assert_eq!(lead.shown(1), [started(Some("play.projects.gg"))]);
        assert!(lead.line(1, "Loaded 12 advancements").is_empty());
        assert_eq!(
            lead.line(1, "Starting integrated minecraft server version 26.1.2"),
            [PresenceStep::Line(
                "Starting integrated minecraft server version 26.1.2".into()
            )]
        );
        assert_eq!(lead.exited(1), [PresenceStep::Stopped]);
        assert!(lead.exited(1).is_empty(), "told once");
    }

    #[test]
    fn the_version_is_the_leading_games() {
        let mut lead = GameLead::new();
        lead.started(1, None);
        lead.set_version(1, Some("1.21.4".into()));
        lead.started(2, None);
        lead.set_version(2, Some("26.2".into()));
        assert_eq!(lead.lead_version(), None, "no game on screen yet");
        lead.shown(2);
        lead.shown(1);
        assert_eq!(lead.lead_version().as_deref(), Some("26.2"));
        lead.exited(2);
        assert_eq!(lead.lead_version().as_deref(), Some("1.21.4"));
        lead.exited(1);
        assert_eq!(lead.lead_version(), None);
    }

    #[test]
    fn the_status_follows_the_oldest_game_on_screen_and_moves_when_it_ends() {
        let mut lead = GameLead::new();
        lead.started(1, None);
        assert_eq!(lead.shown(1), [started(None)]);
        assert!(
            !lead
                .line(1, "Connecting to play.projects.gg, 25565")
                .is_empty()
        );
        // A second game: its lines are kept for later, the status stays with the first.
        lead.started(2, None);
        assert!(lead.shown(2).is_empty());
        assert!(
            lead.line(2, "Connecting to mc.example.net, 25565")
                .is_empty()
        );
        assert!(lead.line(2, "Stopping server").is_empty());
        assert!(
            lead.line(2, "Starting integrated minecraft server version 26.1.2")
                .is_empty()
        );
        // The second ends first: nothing changes.
        assert!(lead.exited(2).is_empty());
        // A third, then the first ends: the third leads from where its log left it.
        lead.started(3, Some("play.projects.gg".into()));
        lead.shown(3);
        lead.line(3, "Connecting to mc.example.net, 25565");
        assert_eq!(
            lead.exited(1),
            [
                started(Some("play.projects.gg")),
                PresenceStep::Line("Connecting to mc.example.net, 25565".into())
            ]
        );
        assert_eq!(
            lead.line(3, "Stopping server"),
            [PresenceStep::Line("Stopping server".into())]
        );
        assert_eq!(lead.exited(3), [PresenceStep::Stopped]);
    }

    #[test]
    fn a_game_still_opening_never_leads() {
        let mut lead = GameLead::new();
        lead.started(1, None);
        lead.shown(1);
        lead.started(2, None);
        // The leading game ends while the other still opens: back in the launcher, until the
        // other one's window shows.
        assert_eq!(lead.exited(1), [PresenceStep::Stopped]);
        assert_eq!(lead.shown(2), [started(None)]);
        // A game that ended while it opened says nothing.
        lead.started(3, None);
        assert!(lead.exited(3).is_empty());
        // Unknown games change nothing.
        assert!(lead.shown(9).is_empty() && lead.line(9, "Stopping server").is_empty());
    }
}

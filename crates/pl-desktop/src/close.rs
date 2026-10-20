//! What the launcher does when its window is closed: the title bar's close button, Alt+F4 or the
//! taskbar's "Close window" (TRAY1; owner, 20 Oct 2026: with no game open, closing quits instead
//! of going to the tray).
//!
//! - A game runs, opens or has its files prepared, or AFK accounts are at work: the window goes
//!   to the tray and the launcher keeps running, as before (the game is followed to its end and
//!   the window comes back after it; AFK accounts keep playing).
//! - The player chose "Tepside çalışmaya devam etsin" (friends' notifications with the window
//!   closed): the same.
//! - A job the player started runs (installs, modpacks, version switches, repairs, imports, a
//!   copy of an instance, world backups and restores, cleanups, a data folder move): the window
//!   goes away at once, the job finishes in the tray and the launcher then quits by itself
//!   ([`QuitWhenDone`]), unless the player comes back or a game starts first. Stopping a job
//!   halfway could leave an instance or a world half written, while finishing it needs no
//!   question.
//! - Nothing of that: the launcher quits.
//!
//! macOS keeps its own convention: the red button only hides the window and the app stays in the
//! Dock (⌘Q quits, the Dock icon brings the window back).

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// The page tells the shell the player's choice under this event name: `{ "choice": "quit" }`.
pub const CHOICE_EVENT: &str = "close-choice";

/// The shell tells the page it put the window away under this event name ([`Closed`]).
pub const CLOSED_EVENT: &str = "window-closed";

/// How long nothing may run before a launcher waiting for its jobs quits: the page starts a job's
/// next step when the previous one answers, and that step gets the time to begin.
pub const SETTLE: Duration = Duration::from_secs(2);

/// How often a launcher waiting for its jobs looks again (only then: nothing else polls).
pub const CHECK_EVERY: Duration = Duration::from_secs(1);

/// Settings → Genel → "Pencereyi kapatınca launcher" (no such row on macOS).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CloseChoice {
    /// "Oyun açık değilse kapansın": the default.
    #[default]
    Quit,
    /// "Tepside çalışmaya devam etsin".
    Tray,
}

#[derive(Deserialize)]
struct ChoiceMessage {
    choice: CloseChoice,
}

/// The page's message under [`CHOICE_EVENT`]; anything else is ignored.
pub fn parse_choice(payload: &str) -> Option<CloseChoice> {
    serde_json::from_str::<ChoiceMessage>(payload)
        .ok()
        .map(|message| message.choice)
}

/// The desktop the launcher runs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Windows,
    MacOs,
    Linux,
}

impl Platform {
    pub const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::Linux
        }
    }
}

/// What keeps the launcher busy at the moment.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Activity {
    /// A game runs, opens or has its files prepared (OYNA was pressed).
    pub game: bool,
    /// AFK accounts are in game, connecting or waiting to connect again.
    pub afk: bool,
    /// A job the player started runs (see the module).
    pub jobs: bool,
}

/// The one-time notice a launcher that stays in the tray explains itself with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TrayHint {
    /// "Launcher tepside çalışmaya devam ediyor."
    Tray,
    /// "AFK hesapların açık, launcher tepside çalışıyor."
    Afk,
}

/// What a close does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseAction {
    /// Nothing keeps the launcher: it quits now.
    Quit,
    /// The window goes to the tray and the launcher keeps running. `hint`: what the one-time
    /// notice says (none on macOS, where a closed window never quits an app).
    Hide { hint: Option<TrayHint> },
    /// The window goes to the tray and the launcher quits once its jobs are done
    /// ([`QuitWhenDone`]).
    HideUntilDone,
}

impl CloseAction {
    /// What the page hears under [`CLOSED_EVENT`]; nothing when the launcher quits.
    pub fn closed(self) -> Option<Closed> {
        match self {
            Self::Quit => None,
            Self::Hide { hint } => Some(Closed { hint }),
            Self::HideUntilDone => Some(Closed { hint: None }),
        }
    }
}

/// The window went away: the page stops its ambient motion and shows the hint the first time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Closed {
    pub hint: Option<TrayHint>,
}

/// What closing the window does now.
pub fn decide(platform: Platform, choice: CloseChoice, activity: Activity) -> CloseAction {
    if platform == Platform::MacOs {
        return CloseAction::Hide { hint: None };
    }
    if activity.afk {
        return CloseAction::Hide {
            hint: Some(TrayHint::Afk),
        };
    }
    if activity.game || choice == CloseChoice::Tray {
        return CloseAction::Hide {
            hint: Some(TrayHint::Tray),
        };
    }
    if activity.jobs {
        return CloseAction::HideUntilDone;
    }
    CloseAction::Quit
}

/// One look of a launcher waiting for its jobs ([`QuitWhenDone::check`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Waiting {
    /// A job still runs, or the last one ended less than [`SETTLE`] ago.
    Wait,
    /// Nothing ran for [`SETTLE`]: quit now.
    Quit,
    /// The player came back (the window is on screen again) or a game or an AFK account started:
    /// the launcher stays and waits no more.
    Stay,
}

/// A launcher that quits once its jobs are done ([`CloseAction::HideUntilDone`]).
#[derive(Debug, Default)]
pub struct QuitWhenDone {
    idle_since: Option<Instant>,
}

impl QuitWhenDone {
    /// Looks at the launcher (every [`CHECK_EVERY`]): what runs and whether the window is shown.
    pub fn check(&mut self, now: Instant, activity: Activity, window_shown: bool) -> Waiting {
        if window_shown || activity.game || activity.afk {
            return Waiting::Stay;
        }
        if activity.jobs {
            self.idle_since = None;
            return Waiting::Wait;
        }
        let since = *self.idle_since.get_or_insert(now);
        if now.saturating_duration_since(since) >= SETTLE {
            Waiting::Quit
        } else {
            Waiting::Wait
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDLE: Activity = Activity {
        game: false,
        afk: false,
        jobs: false,
    };
    const GAME: Activity = Activity { game: true, ..IDLE };
    const AFK: Activity = Activity { afk: true, ..IDLE };
    const JOBS: Activity = Activity { jobs: true, ..IDLE };

    const TRAY_HINT: CloseAction = CloseAction::Hide {
        hint: Some(TrayHint::Tray),
    };
    const AFK_HINT: CloseAction = CloseAction::Hide {
        hint: Some(TrayHint::Afk),
    };

    fn every_activity() -> Vec<Activity> {
        let mut all = Vec::new();
        for bits in 0..8u8 {
            all.push(Activity {
                game: bits & 1 != 0,
                afk: bits & 2 != 0,
                jobs: bits & 4 != 0,
            });
        }
        all
    }

    #[test]
    fn nothing_running_quits_on_windows_and_linux() {
        for platform in [Platform::Windows, Platform::Linux] {
            assert_eq!(decide(platform, CloseChoice::Quit, IDLE), CloseAction::Quit);
        }
    }

    #[test]
    fn a_game_keeps_the_launcher_in_the_tray() {
        assert_eq!(
            decide(Platform::Windows, CloseChoice::Quit, GAME),
            TRAY_HINT
        );
        // Its jobs do not make it quit later: the game is followed to its end.
        let game_and_jobs = Activity { jobs: true, ..GAME };
        assert_eq!(
            decide(Platform::Linux, CloseChoice::Quit, game_and_jobs),
            TRAY_HINT
        );
    }

    #[test]
    fn afk_accounts_keep_the_launcher_and_name_themselves() {
        assert_eq!(decide(Platform::Windows, CloseChoice::Quit, AFK), AFK_HINT);
        // With a game or the tray choice too, the notice still names the AFK accounts.
        for activity in [
            Activity { game: true, ..AFK },
            Activity { jobs: true, ..AFK },
        ] {
            assert_eq!(
                decide(Platform::Windows, CloseChoice::Quit, activity),
                AFK_HINT
            );
            assert_eq!(
                decide(Platform::Windows, CloseChoice::Tray, activity),
                AFK_HINT
            );
        }
    }

    #[test]
    fn the_tray_choice_always_keeps_the_launcher() {
        for activity in [IDLE, GAME, JOBS] {
            assert_eq!(
                decide(Platform::Windows, CloseChoice::Tray, activity),
                TRAY_HINT,
                "{activity:?}"
            );
        }
    }

    #[test]
    fn jobs_finish_before_the_launcher_quits() {
        for platform in [Platform::Windows, Platform::Linux] {
            assert_eq!(
                decide(platform, CloseChoice::Quit, JOBS),
                CloseAction::HideUntilDone
            );
        }
    }

    #[test]
    fn macos_only_hides_the_window() {
        for choice in [CloseChoice::Quit, CloseChoice::Tray] {
            for activity in every_activity() {
                assert_eq!(
                    decide(Platform::MacOs, choice, activity),
                    CloseAction::Hide { hint: None },
                    "{choice:?} {activity:?}"
                );
            }
        }
    }

    #[test]
    fn the_page_hears_every_close_that_keeps_the_process() {
        assert_eq!(CloseAction::Quit.closed(), None);
        assert_eq!(
            TRAY_HINT.closed(),
            Some(Closed {
                hint: Some(TrayHint::Tray)
            })
        );
        assert_eq!(
            CloseAction::HideUntilDone.closed(),
            Some(Closed { hint: None })
        );
        let json = |closed: Option<Closed>| serde_json::to_string(&closed.unwrap()).unwrap();
        assert_eq!(json(AFK_HINT.closed()), r#"{"hint":"afk"}"#);
        assert_eq!(json(TRAY_HINT.closed()), r#"{"hint":"tray"}"#);
        assert_eq!(
            json(CloseAction::HideUntilDone.closed()),
            r#"{"hint":null}"#
        );
    }

    #[test]
    fn the_page_names_the_choice() {
        assert_eq!(
            parse_choice(r#"{"choice":"quit"}"#),
            Some(CloseChoice::Quit)
        );
        assert_eq!(
            parse_choice(r#"{"choice":"tray"}"#),
            Some(CloseChoice::Tray)
        );
        for wrong in [
            r#"{"choice":"exit"}"#,
            r#"{"choice":true}"#,
            "{}",
            "null",
            "tray",
            "",
        ] {
            assert_eq!(parse_choice(wrong), None, "{wrong}");
        }
        assert_eq!(CloseChoice::default(), CloseChoice::Quit);
    }

    #[test]
    fn the_platform_is_this_build() {
        let expected = if cfg!(target_os = "macos") {
            Platform::MacOs
        } else if cfg!(windows) {
            Platform::Windows
        } else {
            Platform::Linux
        };
        assert_eq!(Platform::current(), expected);
    }

    #[test]
    fn waiting_quits_once_the_jobs_have_been_done_for_a_moment() {
        let start = Instant::now();
        let at = |secs: u64| start + Duration::from_secs(secs);
        let mut wait = QuitWhenDone::default();
        assert_eq!(wait.check(at(0), JOBS, false), Waiting::Wait);
        assert_eq!(wait.check(at(30), JOBS, false), Waiting::Wait);
        // The jobs ended: a moment for a next step, then quit.
        assert_eq!(wait.check(at(31), IDLE, false), Waiting::Wait);
        assert_eq!(wait.check(at(32), IDLE, false), Waiting::Wait);
        assert_eq!(wait.check(at(33), IDLE, false), Waiting::Quit);
    }

    #[test]
    fn a_next_step_starts_the_moment_again() {
        let start = Instant::now();
        let at = |secs: u64| start + Duration::from_secs(secs);
        let mut wait = QuitWhenDone::default();
        assert_eq!(wait.check(at(0), IDLE, false), Waiting::Wait);
        assert_eq!(wait.check(at(1), JOBS, false), Waiting::Wait);
        assert_eq!(wait.check(at(2), IDLE, false), Waiting::Wait);
        assert_eq!(wait.check(at(3), IDLE, false), Waiting::Wait);
        assert_eq!(wait.check(at(4), IDLE, false), Waiting::Quit);
    }

    #[test]
    fn the_player_coming_back_calls_the_quit_off() {
        let now = Instant::now();
        for activity in every_activity() {
            assert_eq!(
                QuitWhenDone::default().check(now, activity, true),
                Waiting::Stay,
                "{activity:?}"
            );
        }
    }

    #[test]
    fn a_game_or_an_afk_account_started_meanwhile_keeps_the_launcher() {
        let now = Instant::now();
        for activity in [GAME, AFK, Activity { jobs: true, ..GAME }] {
            assert_eq!(
                QuitWhenDone::default().check(now, activity, false),
                Waiting::Stay,
                "{activity:?}"
            );
        }
    }

    #[test]
    fn a_clock_that_goes_back_never_quits_early() {
        let start = Instant::now() + Duration::from_secs(10);
        let mut wait = QuitWhenDone::default();
        assert_eq!(wait.check(start, IDLE, false), Waiting::Wait);
        assert_eq!(
            wait.check(start - Duration::from_secs(5), IDLE, false),
            Waiting::Wait
        );
    }
}

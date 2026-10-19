//! When the game's window is on screen (PLAY1). From the moment the game process starts until its
//! window shows, the launcher says "Açılıyor"; "Oyunda", the presence (Discord, friends) and the
//! launcher's own window (minimize, tray) follow the moment it shows. Before, the launcher said
//! "Oyunda" as soon as the process started, while the window came 10 to 40 seconds later.
//!
//! What tells, strongest first:
//! - a probe of the desktop's windows ([`WindowProbe`]; on Windows a visible GLFW or LWJGL window of
//!   the game's process or of one it started, `pl_desktop::game_window`), asked every
//!   [`probe_interval`] while the game opens and never again once the window showed;
//! - lines the game logs only once its window exists ([`LogMarker::Shown`]);
//! - without a probe (macOS, Linux), also the lines logged right before the window is created
//!   ([`LogMarker::Imminent`]): the window follows within a second;
//! - after [`GIVE_UP_AFTER`] without any of these, a game that still runs counts as shown (a window
//!   or a log this does not know), so presence and the launcher's window do not wait for ever.
//!
//! A game that exits first never counts as shown: its exit and crash take over.

use std::time::Duration;

/// Whether a window of the game started as this process id is on screen now. Cheap (no message
/// is sent to any window) and asked only while the game opens.
pub type WindowProbe = fn(u32) -> bool;

/// A game still running after this long without any sign of its window counts as shown.
pub const GIVE_UP_AFTER: Duration = Duration::from_secs(300);

/// What a log line says about the game's window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogMarker {
    /// Logged right before the window is created; it shows within a second.
    Imminent,
    /// Logged only once the window exists.
    Shown,
}

impl LogMarker {
    /// Whether the marker means the window is on screen. With a probe only [`LogMarker::Shown`]
    /// does: the probe sees the window itself, and the lines before it come too early (Forge's
    /// and NeoForge's loading window shows long before the game's own lines).
    pub fn counts(self, probing: bool) -> bool {
        !probing || self == LogMarker::Shown
    }
}

/// The window marker in a log message, from LWJGL 2 era versions (1.12 and older) to today.
pub fn log_marker(message: &str) -> Option<LogMarker> {
    let message = message.trim_start();
    // Minecraft 1.13 and later log the backend right before creating the window, 1.12 and older
    // LWJGL's version; Forge's and NeoForge's early loading window logs its OpenGL context.
    if message.starts_with("Backend library: LWJGL")
        || message.starts_with("LWJGL Version")
        || message.starts_with("Requested GL version")
    {
        return Some(LogMarker::Imminent);
    }
    // Resources load, sounds start and texture atlases are stitched once the window exists.
    if message.starts_with("Reloading ResourceManager")
        || message.starts_with("Sound engine started")
        || message.starts_with("OpenAL initialized")
        || (message.starts_with("Created: ") && message.contains("atlas"))
    {
        return Some(LogMarker::Shown);
    }
    None
}

/// How long the probe waits before it looks again: four times a second in the first minute,
/// when nearly every window shows, then once a second.
pub fn probe_interval(elapsed: Duration) -> Duration {
    if elapsed < Duration::from_secs(60) {
        Duration::from_millis(250)
    } else {
        Duration::from_secs(1)
    }
}

/// What made the window count as shown (logged with the time it took).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShownBy {
    Probe,
    Log,
    GaveUp,
}

/// The instance's active version set as last recorded (`pl_content::SetInfo`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayedSet<'a> {
    pub minecraft: &'a str,
    /// The loader's Modrinth name, empty for vanilla.
    pub loader: &'a str,
    /// The game started with this set before.
    pub played: bool,
}

/// Whether this start is the first with the instance's version and loader. It takes longer: the
/// loader prepares its caches (Fabric remaps the mods) and the computer reads every new file once,
/// so the opening state tells the player after a while. Without a recorded set (instances made
/// before sets were recorded) the instance's own play record decides.
pub fn first_start(
    set: Option<PlayedSet<'_>>,
    minecraft: &str,
    loader: &str,
    instance_played: bool,
) -> bool {
    match set {
        Some(set) => !(set.played && set.minecraft == minecraft && set.loader == loader),
        None => !instance_played,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_before_the_window() {
        for line in [
            "Backend library: LWJGL version 3.3.3+5",
            "Backend library: LWJGL version 3.2.2 build 10",
            "LWJGL Version: 2.9.4",
            "Requested GL version 4.6 got version 4.6",
        ] {
            assert_eq!(log_marker(line), Some(LogMarker::Imminent), "{line}");
        }
    }

    #[test]
    fn lines_once_the_window_exists() {
        for line in [
            "Reloading ResourceManager: vanilla, fabric, fabric-api, sodium",
            "Reloading ResourceManager: Default, Mod Resources",
            "Reloading ResourceManager: Default",
            "Sound engine started",
            "OpenAL initialized on device OpenAL Soft on Speakers (Realtek(R) Audio)",
            "Created: 1024x512x4 minecraft:textures/atlas/blocks.png-atlas",
            "Created: 512x512 textures-atlas",
            "  Sound engine started",
        ] {
            assert_eq!(log_marker(line), Some(LogMarker::Shown), "{line}");
        }
    }

    #[test]
    fn other_lines_say_nothing() {
        for line in [
            "Setting user: Steve",
            "Loading Minecraft 1.21.1 with Fabric Loader 0.16.5",
            "Loading 57 mods:",
            "Created: new world",
            "Environment: Environment[sessionHost=https://sessionserver.mojang.com]",
            "Narrator library for x64 successfully loaded",
            "",
        ] {
            assert_eq!(log_marker(line), None, "{line}");
        }
    }

    /// A probe sees the window itself: the lines logged before it do not count then.
    #[test]
    fn a_probe_waits_for_the_window_itself() {
        assert!(LogMarker::Shown.counts(true));
        assert!(!LogMarker::Imminent.counts(true));
        assert!(LogMarker::Shown.counts(false));
        assert!(LogMarker::Imminent.counts(false));
    }

    #[test]
    fn the_first_start_of_a_version_and_loader() {
        let set = |minecraft, loader, played| {
            Some(PlayedSet {
                minecraft,
                loader,
                played,
            })
        };
        assert!(!first_start(
            set("1.21.1", "fabric", true),
            "1.21.1",
            "fabric",
            true
        ));
        // A new version, another loader, a set never started (a switch recorded it).
        assert!(first_start(
            set("1.20.1", "fabric", true),
            "1.21.1",
            "fabric",
            true
        ));
        assert!(first_start(
            set("1.21.1", "fabric", true),
            "1.21.1",
            "neoforge",
            true
        ));
        assert!(first_start(set("1.21.1", "", false), "1.21.1", "", true));
        // No recorded set: the instance's record decides.
        assert!(first_start(None, "1.21.1", "", false));
        assert!(!first_start(None, "1.21.1", "", true));
    }

    #[test]
    fn the_probe_looks_often_at_first() {
        assert_eq!(probe_interval(Duration::ZERO), Duration::from_millis(250));
        assert_eq!(
            probe_interval(Duration::from_secs(59)),
            Duration::from_millis(250)
        );
        assert_eq!(
            probe_interval(Duration::from_secs(60)),
            Duration::from_secs(1)
        );
        assert_eq!(
            probe_interval(Duration::from_secs(280)),
            Duration::from_secs(1)
        );
        // Four looks a second for a minute, then one: about 480 looks before it gives up.
        let mut looks = 0;
        let mut elapsed = Duration::ZERO;
        while elapsed < GIVE_UP_AFTER {
            elapsed += probe_interval(elapsed);
            looks += 1;
        }
        assert!(looks < 500, "{looks}");
    }
}

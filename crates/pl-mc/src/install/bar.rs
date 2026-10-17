//! One progress bar for a whole launch (the OYNA button, the command bar, the taskbar). The parts
//! of the preparation each fill their share of it, the game's install stages keep their spans
//! inside the game part, and the bar never moves back: a part that reports from zero, or the
//! game stages the loader's installer runs on its own (its Java and client jar), hold it where
//! it is instead of making it jump down (42% → 4% before).

use parking_lot::Mutex;

use super::InstallStage;

/// A part of a launch's preparation, in the order they run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LaunchPart {
    /// A Java the launch installs first ("Kur ve oyna" for mods that need a newer Java).
    Java,
    /// The mod loader's own steps (profile, installer, libraries, processors).
    Loader,
    /// The game files: version JSON, Java runtime, libraries, assets, client, natives.
    Game,
    /// The official pack's files (mods, configs, resource packs).
    Pack,
}

impl LaunchPart {
    /// Share of the bar against the other parts (the game files are the bulk of a first install).
    fn weight(self) -> f64 {
        match self {
            Self::Java => 10.0,
            Self::Loader => 12.0,
            Self::Game => 70.0,
            Self::Pack => 18.0,
        }
    }
}

/// Span of a game install stage inside the game part, in percent of the part.
pub fn game_stage_span(stage: InstallStage) -> (f64, f64) {
    match stage {
        InstallStage::Metadata => (0.0, 4.0),
        InstallStage::Java => (4.0, 22.0),
        InstallStage::Libraries => (22.0, 42.0),
        InstallStage::Assets => (42.0, 86.0),
        InstallStage::Client => (86.0, 93.0),
        InstallStage::Natives => (93.0, 99.0),
        InstallStage::Done => (100.0, 100.0),
        // Not a game stage: holds the bar (see `LaunchBar::stage_percent`).
        InstallStage::Loader => (0.0, 0.0),
    }
}

#[derive(Debug)]
struct State {
    /// The parts this launch runs, with their span of the bar (start, end), in order.
    parts: Vec<(LaunchPart, f64, f64)>,
    part: Option<LaunchPart>,
    shown: f64,
}

/// The bar of one launch. Shared by the progress callbacks (`&self` methods, cheap lock).
#[derive(Debug)]
pub struct LaunchBar {
    state: Mutex<State>,
}

impl LaunchBar {
    /// A bar over `parts` (in the order they run), each with its share.
    pub fn new(parts: &[LaunchPart]) -> Self {
        let total: f64 = parts.iter().map(|p| p.weight()).sum();
        let mut start = 0.0;
        let spans = parts
            .iter()
            .map(|&part| {
                let end = if total > 0.0 {
                    start + part.weight() / total * 100.0
                } else {
                    100.0
                };
                let span = (part, start, end);
                start = end;
                span
            })
            .collect();
        Self {
            state: Mutex::new(State {
                parts: spans,
                part: parts.first().copied(),
                shown: 0.0,
            }),
        }
    }

    /// The preparation moved on to `part`; the bar is at least at the part's start.
    pub fn enter(&self, part: LaunchPart) {
        let mut state = self.state.lock();
        state.part = Some(part);
        if let Some(&(_, start, _)) = state.parts.iter().find(|(p, _, _)| *p == part) {
            state.shown = state.shown.max(start);
        }
    }

    /// The bar for a report of `stage` at `fraction` (0–1) of that stage's own work. In the game
    /// part the stage's span counts; in the Java and loader parts only their own stage moves the
    /// bar (`Java`, `Loader`); anything else holds it.
    pub fn stage_percent(&self, stage: InstallStage, fraction: f64) -> f64 {
        let mut state = self.state.lock();
        let fraction = fraction.clamp(0.0, 1.0);
        let part_fraction = match state.part {
            Some(LaunchPart::Game) if stage != InstallStage::Loader => {
                let (from, to) = game_stage_span(stage);
                Some((from + (to - from) * fraction) / 100.0)
            }
            Some(LaunchPart::Java) if stage == InstallStage::Java => Some(fraction),
            Some(LaunchPart::Loader) if stage == InstallStage::Loader => Some(fraction),
            _ => None,
        };
        match part_fraction {
            Some(f) => state.advance(f),
            None => state.shown,
        }
    }

    /// The bar for `fraction` (0–1) of the current part as a whole (the pack sync).
    pub fn part_percent(&self, fraction: f64) -> f64 {
        self.state.lock().advance(fraction.clamp(0.0, 1.0))
    }
}

impl State {
    fn advance(&mut self, fraction: f64) -> f64 {
        let span = self
            .part
            .and_then(|part| self.parts.iter().find(|(p, _, _)| *p == part));
        if let Some(&(_, start, end)) = span {
            self.shown = self
                .shown
                .max((start + (end - start) * fraction).min(100.0));
        }
        self.shown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn a_vanilla_launch_is_the_game_part_alone() {
        let bar = LaunchBar::new(&[LaunchPart::Game]);
        assert!(close(bar.stage_percent(InstallStage::Metadata, 1.0), 4.0));
        assert!(close(bar.stage_percent(InstallStage::Assets, 0.5), 64.0));
        assert!(close(bar.stage_percent(InstallStage::Natives, 1.0), 99.0));
    }

    #[test]
    fn parts_share_the_bar_in_order() {
        let bar = LaunchBar::new(&[LaunchPart::Loader, LaunchPart::Game, LaunchPart::Pack]);
        assert!(close(bar.stage_percent(InstallStage::Loader, 1.0), 12.0));
        bar.enter(LaunchPart::Game);
        assert!(close(bar.stage_percent(InstallStage::Metadata, 0.0), 12.0));
        assert!(close(
            bar.stage_percent(InstallStage::Natives, 1.0),
            12.0 + 70.0 * 0.99
        ));
        bar.enter(LaunchPart::Pack);
        assert!(close(bar.part_percent(0.0), 82.0));
        assert!(close(bar.part_percent(1.0), 100.0));
    }

    #[test]
    fn the_bar_never_moves_back() {
        let bar = LaunchBar::new(&[LaunchPart::Loader, LaunchPart::Game]);
        let loader = bar.stage_percent(InstallStage::Loader, 0.5);
        // The loader's installer downloads the game's Java and client jar on its own: they
        // hold the bar instead of moving it.
        assert!(close(bar.stage_percent(InstallStage::Client, 0.9), loader));
        assert!(close(bar.stage_percent(InstallStage::Java, 0.1), loader));
        bar.enter(LaunchPart::Game);
        let libraries = bar.stage_percent(InstallStage::Libraries, 1.0);
        // A stage that starts over (metadata again) does not pull the bar down.
        assert!(close(
            bar.stage_percent(InstallStage::Metadata, 0.0),
            libraries
        ));
        assert!(bar.stage_percent(InstallStage::Assets, 0.2) >= libraries);
    }

    #[test]
    fn entering_a_part_moves_the_bar_to_its_start() {
        let bar = LaunchBar::new(&[LaunchPart::Java, LaunchPart::Game, LaunchPart::Pack]);
        // Nothing to install for Java: the game part starts where its share begins.
        bar.enter(LaunchPart::Game);
        assert!(close(
            bar.stage_percent(InstallStage::Metadata, 0.0),
            10.0 / 98.0 * 100.0
        ));
        // The game stages' own reports never touch the pack's share early.
        assert!(bar.stage_percent(InstallStage::Natives, 1.0) < 100.0 * 80.0 / 98.0 + 0.01);
    }
}

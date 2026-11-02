//! A person's volume (design §6.4).
//!
//! How the voice page plays people decides the slider's top, and VOICE-0's echo test decides the
//! path: WebRTC's echo canceller is sure to hear what `<audio>` elements play, where volume stops
//! at 100 %; WebAudio gain nodes go to 200 % (behind a limiter) but only if the canceller hears
//! WebAudio's output too. A remembered value from the other path (a settings backup restored on
//! another launcher) is held to this path's top.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Playout {
    /// One `<audio>` element per person.
    Element,
    /// One WebAudio gain node per person, then a limiter.
    WebAudio,
}

impl Playout {
    /// The slider's top, in percent.
    pub const fn max_percent(self) -> u16 {
        match self {
            Self::Element => 100,
            Self::WebAudio => 200,
        }
    }
}

/// A slider value held to the path's top.
pub fn clamp(percent: u16, playout: Playout) -> u16 {
    percent.min(playout.max_percent())
}

/// The gain for a slider value (1.0 plays a person as they arrive).
pub fn gain(percent: u16, playout: Playout) -> f32 {
    f32::from(clamp(percent, playout)) / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_top_depends_on_the_path() {
        assert_eq!(gain(150, Playout::WebAudio), 1.5);
        assert_eq!(gain(150, Playout::Element), 1.0);
        assert_eq!(gain(250, Playout::WebAudio), 2.0);
        assert_eq!(gain(0, Playout::Element), 0.0);
        assert_eq!(clamp(180, Playout::Element), 100);
        assert_eq!(clamp(80, Playout::Element), 80);
    }

    #[test]
    fn names_as_the_page_and_the_probe_write_them() {
        assert_eq!(
            serde_json::to_string(&Playout::WebAudio).unwrap(),
            "\"webAudio\""
        );
        assert_eq!(
            serde_json::from_str::<Playout>("\"element\"").unwrap(),
            Playout::Element
        );
    }
}

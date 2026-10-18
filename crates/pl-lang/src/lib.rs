//! The language players read Projects Launcher in.
//!
//! The owner's rule (18 Oct 2026): projects.gg is a Turkish server; English is there so players
//! from abroad can use the launcher. The language is, in this order:
//!
//! 1. the player's own pick in Settings → Genel → Dil;
//! 2. Turkish when the computer is in Turkey (its time zone, or Windows' home location);
//! 3. Turkish when the system's display language is Turkish;
//! 4. English otherwise.
//!
//! Steps 2 to 4 are "Otomatik" ([`Signals::automatic`]); [`resolve`] puts the pick in front.
//! Nothing asks the network. The UI resolves the language and tells the backend
//! ([`Lang::set_current`]); the texts the backend writes for players (command errors, the tray
//! menu, the Discord presence, the browser sign-in page) follow it through [`pick`], and so does
//! the game's first language. Nothing sets it in Studio, which stays Turkish: [`Lang::Tr`] is the
//! default.
//!
//! Functions whose texts are tested take a [`Lang`] instead of reading the current one, so tests
//! that run side by side never see each other's language.

use std::sync::atomic::{AtomicU8, Ordering};

use serde::{Deserialize, Serialize};

/// A language of the launcher's own texts.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    /// Turkish: the launcher's first language, and the backend's until the UI says otherwise.
    #[default]
    Tr,
    En,
}

/// The language set last ([`Lang::set_current`]): 0 Turkish, 1 English.
static CURRENT: AtomicU8 = AtomicU8::new(0);

impl Lang {
    /// The language of a locale tag: Turkish for `tr`, `tr-TR`, `tr_TR.UTF-8`, `tr-Latn-TR` and
    /// the like, English for every other tag (an empty one, `C` and `POSIX` included).
    pub fn from_locale(tag: &str) -> Self {
        let primary = tag
            .trim()
            .split(['-', '_', '.', '@'])
            .next()
            .unwrap_or_default();
        if primary.eq_ignore_ascii_case("tr") {
            Self::Tr
        } else {
            Self::En
        }
    }

    /// The language the UI set last ([`Lang::set_current`]); Turkish before that.
    pub fn current() -> Self {
        match CURRENT.load(Ordering::Relaxed) {
            1 => Self::En,
            _ => Self::Tr,
        }
    }

    /// Makes this the language of the backend's texts from now on.
    pub fn set_current(self) {
        let value = match self {
            Self::Tr => 0,
            Self::En => 1,
        };
        CURRENT.store(value, Ordering::Relaxed);
    }

    /// `tr` or `en` (what the UI calls the language too).
    pub fn code(self) -> &'static str {
        match self {
            Self::Tr => "tr",
            Self::En => "en",
        }
    }

    /// This language's text out of a Turkish/English pair.
    pub fn pick<T>(self, tr: T, en: T) -> T {
        match self {
            Self::Tr => tr,
            Self::En => en,
        }
    }
}

/// The current language's text out of a Turkish/English pair ([`Lang::current`]).
pub fn pick<T>(tr: T, en: T) -> T {
    Lang::current().pick(tr, en)
}

/// Time zones that count as Turkey: `Europe/Istanbul` with its old names, and `Asia/Famagusta`,
/// Northern Cyprus, where Turkish is the language people speak.
pub const TURKEY_TIME_ZONES: [&str; 4] = [
    "Europe/Istanbul",
    "Asia/Istanbul",
    "Turkey",
    "Asia/Famagusta",
];

/// What the computer says about the player, for the automatic language. The UI gets the same
/// signals before its first script runs, so both sides resolve alike.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Signals {
    /// The system's display language, a locale tag: on Windows the first of the user's
    /// preferred UI languages (not the regional format), on macOS the first preferred language,
    /// on Linux `LANGUAGE`, `LC_ALL`, `LC_MESSAGES` or `LANG`.
    pub language: Option<String>,
    /// The device's IANA time zone (`Europe/Istanbul`).
    pub time_zone: Option<String>,
    /// A home country the system names (ISO 3166-1 code). Only Windows' home location (Settings
    /// → Time & language → Region), and only when it is Turkey: nothing else needs a country.
    pub region: Option<String>,
}

impl Signals {
    /// Reads the signals of this computer (no network).
    pub fn system() -> Self {
        Self {
            language: sys_locale::get_locale(),
            time_zone: jiff::tz::TimeZone::try_system()
                .ok()
                .and_then(|zone| zone.iana_name().map(str::to_owned)),
            region: home_region(),
        }
    }

    /// True when the computer is in Turkey: a Turkish time zone ([`TURKEY_TIME_ZONES`]) or
    /// Turkey as Windows' home location.
    pub fn in_turkey(&self) -> bool {
        let zone = self.time_zone.as_deref().map(str::trim).unwrap_or_default();
        TURKEY_TIME_ZONES
            .iter()
            .any(|turkey| turkey.eq_ignore_ascii_case(zone))
            || self
                .region
                .as_deref()
                .is_some_and(|region| region.trim().eq_ignore_ascii_case("TR"))
    }

    /// "Otomatik": Turkish in Turkey or on a Turkish system, English otherwise.
    pub fn automatic(&self) -> Lang {
        if self.in_turkey() {
            Lang::Tr
        } else {
            self.language.as_deref().map_or(Lang::En, Lang::from_locale)
        }
    }
}

/// The language to show: the player's pick, else the automatic one.
pub fn resolve(choice: Option<Lang>, signals: &Signals) -> Lang {
    choice.unwrap_or_else(|| signals.automatic())
}

/// Windows' home location when it is Turkey (`GetUserGeoID`, there since Windows XP: a newer
/// call would stop the launcher from starting on an older Windows 10). GeoID 235 is Turkey.
#[cfg(windows)]
fn home_region() -> Option<String> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        safe fn GetUserGeoID(geo_class: u32) -> i32;
    }
    const GEOCLASS_NATION: u32 = 16;
    const TURKEY: i32 = 235;
    (GetUserGeoID(GEOCLASS_NATION) == TURKEY).then(|| "TR".to_owned())
}

/// Only Windows names a home location here; elsewhere the time zone and the language decide.
#[cfg(not(windows))]
fn home_region() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signals(language: &str, time_zone: &str) -> Signals {
        Signals {
            language: Some(language.to_owned()).filter(|s| !s.is_empty()),
            time_zone: Some(time_zone.to_owned()).filter(|s| !s.is_empty()),
            region: None,
        }
    }

    #[test]
    fn turkish_tags_only_give_turkish() {
        for tag in [
            "tr",
            "TR",
            "tr-TR",
            "tr_TR",
            "tr_TR.UTF-8",
            "tr-Latn-TR",
            "tr@euro",
            " tr-CY ",
        ] {
            assert_eq!(Lang::from_locale(tag), Lang::Tr, "{tag}");
        }
        for tag in [
            "en-US",
            "en_GB",
            "de-DE",
            "az-Latn-AZ",
            "trv",
            "ku-TR",
            "C",
            "POSIX",
            "",
            "-",
        ] {
            assert_eq!(Lang::from_locale(tag), Lang::En, "{tag}");
        }
    }

    #[test]
    fn turkey_comes_before_the_system_language() {
        // An English system in Istanbul reads Turkish; the same system in Berlin, English.
        assert_eq!(signals("en-US", "Europe/Istanbul").automatic(), Lang::Tr);
        assert_eq!(signals("en-US", "Europe/Berlin").automatic(), Lang::En);
        // A Turkish system reads Turkish anywhere.
        assert_eq!(signals("tr-TR", "Europe/Berlin").automatic(), Lang::Tr);
        assert_eq!(signals("tr-TR", "").automatic(), Lang::Tr);
        // Every other system outside Turkey reads English, as does a computer that says nothing.
        assert_eq!(signals("de-DE", "Europe/Berlin").automatic(), Lang::En);
        assert_eq!(signals("", "").automatic(), Lang::En);
        assert_eq!(Signals::default().automatic(), Lang::En);
    }

    #[test]
    fn turkeys_time_zones_and_home_location() {
        for zone in [
            "Europe/Istanbul",
            "Asia/Istanbul",
            "Turkey",
            "Asia/Famagusta",
            " europe/istanbul ",
        ] {
            assert!(signals("en-US", zone).in_turkey(), "{zone}");
        }
        for zone in [
            "Europe/Nicosia",
            "Asia/Nicosia",
            "Europe/Athens",
            "Asia/Baku",
            "UTC",
            "",
        ] {
            assert!(!signals("en-US", zone).in_turkey(), "{zone}");
        }
        // Windows' home location set to Turkey counts, wherever the clock is set.
        let home = Signals {
            region: Some("TR".to_owned()),
            ..signals("en-GB", "Europe/London")
        };
        assert!(home.in_turkey());
        assert_eq!(home.automatic(), Lang::Tr);
    }

    #[test]
    fn the_players_pick_comes_first() {
        let istanbul = signals("en-US", "Europe/Istanbul");
        let berlin = signals("en-US", "Europe/Berlin");
        assert_eq!(resolve(Some(Lang::En), &istanbul), Lang::En);
        assert_eq!(resolve(Some(Lang::Tr), &berlin), Lang::Tr);
        assert_eq!(resolve(None, &istanbul), Lang::Tr);
        assert_eq!(resolve(None, &berlin), Lang::En);
    }

    #[test]
    fn the_pair_follows_the_language() {
        assert_eq!(Lang::Tr.pick("Oyna", "Play"), "Oyna");
        assert_eq!(Lang::En.pick("Oyna", "Play"), "Play");
        assert_eq!(Lang::Tr.code(), "tr");
        assert_eq!(Lang::En.code(), "en");
    }

    // The only test that touches the process-wide language.
    #[test]
    fn the_current_language_is_turkish_until_set() {
        assert_eq!(Lang::current(), Lang::Tr);
        assert_eq!(pick("Oyna", "Play"), "Oyna");
        Lang::En.set_current();
        assert_eq!(Lang::current(), Lang::En);
        assert_eq!(pick("Oyna", "Play"), "Play");
        Lang::Tr.set_current();
        assert_eq!(pick("Oyna", "Play"), "Oyna");
    }

    #[test]
    fn the_ui_gets_codes_and_signals_as_json() {
        assert_eq!(serde_json::to_string(&Lang::En).unwrap(), "\"en\"");
        assert_eq!(serde_json::from_str::<Lang>("\"tr\"").unwrap(), Lang::Tr);
        assert_eq!(
            serde_json::to_string(&signals("en-US", "Europe/Istanbul")).unwrap(),
            r#"{"language":"en-US","timeZone":"Europe/Istanbul","region":null}"#
        );
    }

    #[test]
    fn this_computer_resolves_without_trouble() {
        // Whatever this machine says, reading it works and gives one of the two.
        let signals = Signals::system();
        assert!(matches!(signals.automatic(), Lang::Tr | Lang::En));
    }
}

//! The game versions an AFK account can join: the version picker and how a stored choice resolves.
//!
//! The list belongs to the AFK client release: its CI reads MCC's own protocol table and publishes
//! it with the release (`afk.minecraftVersions` in the signed content index), and Studio adds the
//! version the Projects server needs (`afk.recommendedMinecraftVersion`). Without a usable list the
//! built-in table is used: the same CI list, newest first, as of client 4.0.0. `auto` ("ask the
//! server") is never part of a list; the launcher offers it itself, first in the picker.
//!
//! An account stores one of:
//! - `""`: follow the recommended version (new accounts, and legacy records that never stored a
//!   version: the legacy launcher mapped those to its default too);
//! - [`AUTO_MINECRAFT_VERSION`];
//! - a listed value (pinned).
//!
//! A stored value the current list does not know is never rewritten on disk: it resolves to the
//! recommended version (the settings sheet says so) until the player saves another choice.

use crate::model::{AUTO_MINECRAFT_VERSION, DEFAULT_MINECRAFT_VERSION, VersionOption};
use crate::util::eq_ignore_case;

/// The client CI's list (`minecraftVersions` of the 16 October 2026 run, MCC protocol table):
/// versions sharing a protocol are one entry, `value` is the lowest of them, newest first.
const VERSION_TABLE: &[(&str, &str)] = &[
    ("26.3", "26.3"),
    ("26.2", "26.2"),
    ("26.1", "26.1"),
    ("1.21.11", "1.21.11"),
    ("1.21.9", "1.21.9 – 1.21.10"),
    ("1.21.7", "1.21.7 – 1.21.8"),
    ("1.21.6", "1.21.6"),
    ("1.21.5", "1.21.5"),
    ("1.21.4", "1.21.4"),
    ("1.21.2", "1.21.2 – 1.21.3"),
    ("1.21", "1.21 – 1.21.1"),
    ("1.20.5", "1.20.5 – 1.20.6"),
    ("1.20.3", "1.20.3 – 1.20.4"),
    ("1.20.2", "1.20.2"),
    ("1.20", "1.20 – 1.20.1"),
    ("1.19.4", "1.19.4"),
    ("1.19.3", "1.19.3"),
    ("1.19.1", "1.19.1 – 1.19.2"),
    ("1.19", "1.19"),
    ("1.18.2", "1.18.2"),
    ("1.18", "1.18 – 1.18.1"),
    ("1.17.1", "1.17.1"),
    ("1.17", "1.17"),
    ("1.16.4", "1.16.4 – 1.16.5"),
    ("1.16.3", "1.16.3"),
    ("1.16.2", "1.16.2"),
    ("1.16.1", "1.16.1"),
    ("1.16", "1.16"),
    ("1.15.2", "1.15.2"),
    ("1.15.1", "1.15.1"),
    ("1.15", "1.15"),
    ("1.14.4", "1.14.4"),
    ("1.14.3", "1.14.3"),
    ("1.14.2", "1.14.2"),
    ("1.14.1", "1.14.1"),
    ("1.14", "1.14"),
    ("1.13.2", "1.13.2"),
    ("1.13.1", "1.13.1"),
    ("1.13", "1.13"),
    ("1.12.2", "1.12.2"),
    ("1.12.1", "1.12.1"),
    ("1.12", "1.12"),
    ("1.11.1", "1.11.1 – 1.11.2"),
    ("1.11", "1.11"),
    ("1.10", "1.10 – 1.10.2"),
    ("1.9.3", "1.9.3 – 1.9.4"),
    ("1.9.2", "1.9.2"),
    ("1.9.1", "1.9.1"),
    ("1.9", "1.9"),
    ("1.8", "1.8 – 1.8.9"),
    ("1.7.6", "1.7.6 – 1.7.10"),
    ("1.7.2", "1.7.2 – 1.7.5"),
    ("1.6.4", "1.6.4"),
    ("1.6.2", "1.6.2"),
    ("1.6.1", "1.6.1"),
    ("1.6", "1.6"),
    ("1.5.2", "1.5.2"),
    ("1.5", "1.5 – 1.5.1"),
    ("1.4.6", "1.4.6 – 1.4.7"),
];

/// Most entries a published list may have (the content format's limit).
pub const MAX_LISTED_VERSIONS: usize = 256;
/// Longest version value, in bytes.
const MAX_VALUE_BYTES: usize = 32;
/// Longest version label, in characters.
const MAX_LABEL_CHARS: usize = 64;

/// One listed version: what MCC's `MinecraftVersion` takes and what the picker shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionEntry {
    pub value: String,
    pub label: String,
}

/// The versions accounts can choose from now, and the recommended one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionCatalog {
    entries: Vec<VersionEntry>,
    /// A listed value or [`AUTO_MINECRAFT_VERSION`].
    recommended: String,
}

/// A stored choice resolved against a [`VersionCatalog`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedVersion {
    /// What the client gets as `MinecraftVersion` and the picker shows selected: a listed value or
    /// [`AUTO_MINECRAFT_VERSION`].
    pub value: String,
    /// `value` is the recommended version.
    pub recommended: bool,
    /// The stored value when the list does not know it (`value` is the recommended one then).
    pub unknown: Option<String>,
}

impl Default for VersionCatalog {
    fn default() -> Self {
        Self::built_in()
    }
}

impl VersionCatalog {
    /// The built-in table, recommending [`DEFAULT_MINECRAFT_VERSION`].
    pub fn built_in() -> Self {
        Self {
            entries: built_in_entries(),
            recommended: DEFAULT_MINECRAFT_VERSION.to_owned(),
        }
    }

    /// The list and recommendation published with the AFK client release (`afk` in the content
    /// index). An empty list means the built-in one (with the published recommendation). `None`
    /// when either breaks a rule of the content format (empty or overlong values, whitespace or
    /// control characters, `auto` listed, empty or overlong labels, duplicates, more than
    /// [`MAX_LISTED_VERSIONS`] entries, a recommendation that is neither listed nor `auto`): the
    /// caller then keeps the built-in catalog. Without a recommendation the built-in default is
    /// recommended when listed, else the newest entry.
    pub fn from_release<V, L>(
        versions: impl IntoIterator<Item = (V, L)>,
        recommended: Option<&str>,
    ) -> Option<Self>
    where
        V: Into<String>,
        L: Into<String>,
    {
        let mut entries: Vec<VersionEntry> = Vec::new();
        for (value, label) in versions {
            let entry = VersionEntry {
                value: value.into(),
                label: label.into(),
            };
            if entries.len() == MAX_LISTED_VERSIONS
                || !is_valid_entry(&entry)
                || entries
                    .iter()
                    .any(|e| eq_ignore_case(&e.value, &entry.value))
            {
                return None;
            }
            entries.push(entry);
        }
        if entries.is_empty() {
            entries = built_in_entries();
        }
        let recommended = match recommended {
            Some(wanted) if eq_ignore_case(wanted, AUTO_MINECRAFT_VERSION) => {
                AUTO_MINECRAFT_VERSION.to_owned()
            }
            Some(wanted) => entries
                .iter()
                .find(|e| e.value == wanted)
                .map(|e| e.value.clone())?,
            None => entries
                .iter()
                .find(|e| e.value == DEFAULT_MINECRAFT_VERSION)
                .unwrap_or(&entries[0])
                .value
                .clone(),
        };
        Some(Self {
            entries,
            recommended,
        })
    }

    /// The recommended value: a listed one or [`AUTO_MINECRAFT_VERSION`].
    pub fn recommended(&self) -> &str {
        &self.recommended
    }

    /// The listed versions, newest first (without `auto`).
    pub fn entries(&self) -> &[VersionEntry] {
        &self.entries
    }

    /// The picker: `auto` first, then the listed versions newest first; the recommended one is
    /// flagged. `auto`'s label is the value itself (the UI words it).
    pub fn options(&self) -> Vec<VersionOption> {
        let auto = VersionOption {
            value: AUTO_MINECRAFT_VERSION.to_owned(),
            label: AUTO_MINECRAFT_VERSION.to_owned(),
            recommended: self.recommended == AUTO_MINECRAFT_VERSION,
            auto: true,
        };
        std::iter::once(auto)
            .chain(self.entries.iter().map(|e| VersionOption {
                value: e.value.clone(),
                label: e.label.clone(),
                recommended: e.value == self.recommended,
                auto: false,
            }))
            .collect()
    }

    /// The listed entry for `value` (trimmed, case-insensitive).
    pub fn find(&self, value: &str) -> Option<&VersionEntry> {
        let value = value.trim();
        self.entries
            .iter()
            .find(|e| eq_ignore_case(&e.value, value))
    }

    /// What a stored choice means now: follow (`""`) and unknown values give the recommended
    /// version, `auto` and listed values (case-insensitive) themselves.
    pub fn resolve(&self, stored: &str) -> ResolvedVersion {
        let stored = stored.trim();
        let recommended = || ResolvedVersion {
            value: self.recommended.clone(),
            recommended: true,
            unknown: None,
        };
        if stored.is_empty() {
            return recommended();
        }
        if eq_ignore_case(stored, AUTO_MINECRAFT_VERSION) {
            return ResolvedVersion {
                value: AUTO_MINECRAFT_VERSION.to_owned(),
                recommended: self.recommended == AUTO_MINECRAFT_VERSION,
                unknown: None,
            };
        }
        match self.find(stored) {
            Some(entry) => ResolvedVersion {
                recommended: entry.value == self.recommended,
                value: entry.value.clone(),
                unknown: None,
            },
            None => ResolvedVersion {
                unknown: Some(stored.to_owned()),
                ..recommended()
            },
        }
    }

    /// What to store for a choice of the picker: the recommended entry means "follow the
    /// recommendation" (`""`, so the account moves along when Studio recommends another version);
    /// `auto` and listed values are kept (canonical spelling); anything else follows the
    /// recommendation too.
    pub fn to_stored(&self, chosen: &str) -> String {
        let chosen = chosen.trim();
        if chosen.is_empty() || eq_ignore_case(chosen, &self.recommended) {
            return String::new();
        }
        if eq_ignore_case(chosen, AUTO_MINECRAFT_VERSION) {
            return AUTO_MINECRAFT_VERSION.to_owned();
        }
        self.find(chosen)
            .map(|entry| entry.value.clone())
            .unwrap_or_default()
    }
}

fn built_in_entries() -> Vec<VersionEntry> {
    VERSION_TABLE
        .iter()
        .map(|(value, label)| VersionEntry {
            value: (*value).to_owned(),
            label: (*label).to_owned(),
        })
        .collect()
}

/// The content format's entry rules (`AfkRelease::minecraft_version_problems` in pl-content).
fn is_valid_entry(entry: &VersionEntry) -> bool {
    let value = &entry.value;
    let label = &entry.label;
    !value.is_empty()
        && value.len() <= MAX_VALUE_BYTES
        && !eq_ignore_case(value, AUTO_MINECRAFT_VERSION)
        && !value.chars().any(|c| c.is_whitespace() || c.is_control())
        && !label.trim().is_empty()
        && label.chars().count() <= MAX_LABEL_CHARS
        && !label.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(entries: &[(&str, &str)], recommended: Option<&str>) -> Option<VersionCatalog> {
        VersionCatalog::from_release(entries.iter().copied(), recommended)
    }

    #[test]
    fn built_in_table_is_the_client_ci_list() {
        let catalog = VersionCatalog::built_in();
        let entries = catalog.entries();
        assert_eq!(entries.len(), 59);
        assert_eq!(entries[0].value, "26.3");
        assert_eq!(entries[4].label, "1.21.9 – 1.21.10");
        assert_eq!(
            entries.last().map(|e| e.label.as_str()),
            Some("1.4.6 – 1.4.7")
        );
        // The 23 entries the legacy table lacked.
        for value in [
            "1.15.1", "1.15", "1.14.3", "1.14.2", "1.14.1", "1.14", "1.13.1", "1.13", "1.12.1",
            "1.12", "1.11", "1.9.2", "1.9.1", "1.9", "1.7.6", "1.7.2", "1.6.4", "1.6.2", "1.6.1",
            "1.6", "1.5.2", "1.5", "1.4.6",
        ] {
            assert!(catalog.find(value).is_some(), "{value}");
        }
        // Newest first, unique, and every entry follows the content format rules.
        for pair in entries.windows(2) {
            assert!(pl_version_order(&pair[0].value, &pair[1].value), "{pair:?}");
        }
        assert!(entries.iter().all(is_valid_entry));
        assert_eq!(catalog.recommended(), DEFAULT_MINECRAFT_VERSION);
    }

    /// `a` is a newer Minecraft version than `b` (numeric components).
    fn pl_version_order(a: &str, b: &str) -> bool {
        let parts = |v: &str| -> Vec<u32> { v.split('.').map(|p| p.parse().unwrap()).collect() };
        parts(a) > parts(b)
    }

    #[test]
    fn picker_offers_auto_first_and_flags_the_recommendation() {
        let options = VersionCatalog::built_in().options();
        assert_eq!(options.len(), 60);
        assert!(options[0].auto && !options[0].recommended);
        assert_eq!(options[0].value, AUTO_MINECRAFT_VERSION);
        assert!(options[1].recommended && options[1].value == "26.3");
        assert_eq!(options.iter().filter(|o| o.recommended).count(), 1);

        let auto = index(&[("26.4", "26.4"), ("26.3", "26.3")], Some("AUTO")).unwrap();
        assert_eq!(auto.recommended(), AUTO_MINECRAFT_VERSION);
        let options = auto.options();
        assert!(options[0].auto && options[0].recommended);
        assert!(options[1..].iter().all(|o| !o.recommended));
    }

    #[test]
    fn index_list_replaces_the_table() {
        let catalog = index(
            &[
                ("26.4", "26.4"),
                ("26.3", "26.3"),
                ("1.21.9", "1.21.9 – 1.21.10"),
            ],
            Some("26.4"),
        )
        .unwrap();
        assert_eq!(catalog.entries().len(), 3);
        assert_eq!(catalog.recommended(), "26.4");
        assert_eq!(catalog.resolve("26.4").value, "26.4");
        // Without a recommendation: the built-in default when listed, else the newest entry.
        assert_eq!(
            index(&[("26.4", "26.4"), ("26.3", "26.3")], None)
                .unwrap()
                .recommended(),
            "26.3"
        );
        assert_eq!(
            index(&[("27.1", "27.1"), ("27.0", "27.0")], None)
                .unwrap()
                .recommended(),
            "27.1"
        );
        // An empty list keeps the built-in versions with the published recommendation.
        let empty = index(&[], Some("1.21.9")).unwrap();
        assert_eq!(empty.entries().len(), 59);
        assert_eq!(empty.recommended(), "1.21.9");
        assert_eq!(index(&[], None).unwrap(), VersionCatalog::built_in());
    }

    #[test]
    fn broken_index_lists_are_refused() {
        let ok = ("26.3", "26.3");
        for broken in [
            vec![ok, ("26.3", "again")],
            vec![ok, ("1.20-pre1", "x"), ("1.20-PRE1", "case")],
            vec![ok, ("", "empty")],
            vec![ok, ("1.21 4", "space")],
            vec![ok, ("1.20", " ")],
            vec![ok, ("auto", "Otomatik")],
            vec![ok, ("1.20", "line\nbreak")],
        ] {
            assert_eq!(index(&broken, None), None, "{broken:?}");
        }
        assert_eq!(index(&[ok], Some("1.2.5")), None, "unknown recommendation");
        let long = "9".repeat(33);
        assert_eq!(index(&[ok, (long.as_str(), "long")], None), None);
        let many: Vec<(String, String)> = (0..=MAX_LISTED_VERSIONS)
            .map(|i| (format!("1.{i}"), "x".to_owned()))
            .collect();
        assert_eq!(VersionCatalog::from_release(many, None), None);
    }

    #[test]
    fn stored_values_resolve_against_the_current_list() {
        let catalog = VersionCatalog::built_in();
        let follow = catalog.resolve("");
        assert_eq!(
            (follow.value.as_str(), follow.recommended, follow.unknown),
            ("26.3", true, None)
        );
        let auto = catalog.resolve("  AUTO ");
        assert_eq!((auto.value.as_str(), auto.recommended), ("auto", false));
        let pinned = catalog.resolve(" 1.21.9 ");
        assert_eq!(
            (pinned.value.as_str(), pinned.recommended),
            ("1.21.9", false)
        );
        assert_eq!(catalog.resolve("26.3").value, "26.3");
        assert!(catalog.resolve("26.3").recommended);
        // Unknown: the recommended version, and the stored value is reported.
        let unknown = catalog.resolve("1.2.3");
        assert_eq!(unknown.value, "26.3");
        assert!(unknown.recommended);
        assert_eq!(unknown.unknown.as_deref(), Some("1.2.3"));

        // A version only the published list knows is valid with that list.
        let newer = index(&[("26.4", "26.4"), ("26.3", "26.3")], Some("26.3")).unwrap();
        assert_eq!(newer.resolve("26.4").value, "26.4");
        assert!(newer.resolve("26.4").unknown.is_none());
        assert_eq!(catalog.resolve("26.4").unknown.as_deref(), Some("26.4"));
    }

    #[test]
    fn choices_store_follow_auto_or_a_pinned_value() {
        let catalog = VersionCatalog::built_in();
        assert_eq!(
            catalog.to_stored("26.3"),
            "",
            "the recommended entry follows"
        );
        assert_eq!(catalog.to_stored(""), "");
        assert_eq!(catalog.to_stored(" Auto "), "auto");
        assert_eq!(catalog.to_stored("1.21.9"), "1.21.9");
        assert_eq!(catalog.to_stored("1.2.3"), "", "unknown choices follow");

        let auto = index(&[("26.3", "26.3")], Some("auto")).unwrap();
        assert_eq!(
            auto.to_stored("auto"),
            "",
            "auto is the recommendation here"
        );
        assert_eq!(auto.to_stored("26.3"), "26.3");
        assert_eq!(auto.resolve("").value, "auto");
    }
}

//! Profile badges (BADGES; the service's README "Profile badges", SOC-VERIFY's single mark grown
//! into a catalogue): what Projects gives an account, right of its name.
//!
//! - Cards carry the keys of the badges they show (`marks`, in the catalogue's order: the first is
//!   the most important) beside SOC-VERIFY's `mark` for launchers from before badges. A service
//!   from before badges sends only `mark`: it reads as a list of one, since the built-in
//!   catalogue knows both of its keys.
//! - The catalogue (`GET /v1/marks`, no session) names each badge and says how it looks. The
//!   launcher carries the built-in one, so a first view draws right before (or without) the
//!   network, and reads the service's again when its revision moves (`marksRev`, `markdefs`) or a
//!   card names a key it does not know.
//! - Colours, glyphs and shapes are fixed sets that `@pl/ui` draws for each pixel grid; a badge
//!   with a look this version does not know comes from a newer catalogue and is left out.

use serde::{Deserialize, Serialize};

use crate::model::ProfileMark;

/// The colours this version draws (`--color-mark-<name>`).
pub const MARK_COLORS: [&str; 9] = [
    "blue", "red", "purple", "green", "orange", "teal", "indigo", "pink", "slate",
];
/// The glyphs this version draws.
pub const MARK_GLYPHS: [&str; 9] = [
    "check", "star", "cloud", "bolt", "crown", "heart", "flask", "play", "trophy",
];
/// The outlines this version draws.
pub const MARK_SHAPES: [&str; 3] = ["seal", "round", "shield"];

/// Badges a catalogue or a card holds at most (one bit each of the service's 31-bit set).
pub const MAX_MARKS: usize = 31;
/// A badge's key: `admin`, `verified`, … for the built-in ones, `m<id>` for those Studio adds.
const MAX_KEY_CHARS: usize = 24;
/// A badge's name, in either language.
const MAX_NAME_CHARS: usize = 32;

/// A badge's name in both of the launcher's languages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct MarkName {
    pub tr: String,
    pub en: String,
}

/// One badge of the catalogue: its key (what cards carry), its name and its look.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct MarkDef {
    pub key: String,
    pub name: MarkName,
    /// One of [`MARK_COLORS`].
    pub color: String,
    /// One of [`MARK_GLYPHS`].
    pub glyph: String,
    /// One of [`MARK_SHAPES`].
    pub shape: String,
}

impl MarkDef {
    /// The name in English or Turkish.
    pub fn name_in(&self, english: bool) -> &str {
        if english {
            &self.name.en
        } else {
            &self.name.tr
        }
    }
}

/// The badges that show, in order, and the catalogue's revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkCatalog {
    pub rev: i64,
    pub marks: Vec<MarkDef>,
}

impl Default for MarkCatalog {
    fn default() -> Self {
        Self::builtin()
    }
}

fn builtin(key: &str, tr: &str, en: &str, color: &str, glyph: &str) -> MarkDef {
    MarkDef {
        key: key.to_owned(),
        name: MarkName {
            tr: tr.to_owned(),
            en: en.to_owned(),
        },
        color: color.to_owned(),
        glyph: glyph.to_owned(),
        shape: "seal".to_owned(),
    }
}

impl MarkCatalog {
    /// The service's catalogue until Studio's owner first saves one (revision 0): the badges that
    /// show of its built-in ones, in its order (the four extras ship switched off).
    pub fn builtin() -> Self {
        Self {
            rev: 0,
            marks: vec![
                builtin(
                    "admin",
                    "Projects yöneticisi",
                    "Projects admin",
                    "red",
                    "check",
                ),
                builtin(
                    "moderator",
                    "Projects moderatörü",
                    "Projects moderator",
                    "purple",
                    "check",
                ),
                builtin(
                    "staff",
                    "Projects personeli",
                    "Projects staff",
                    "green",
                    "check",
                ),
                builtin(
                    "verified",
                    "Doğrulanmış hesap",
                    "Verified account",
                    "blue",
                    "check",
                ),
                builtin(
                    "premium",
                    "Premium Club üyesi",
                    "Premium Club member",
                    "orange",
                    "star",
                ),
            ],
        }
    }

    /// The badge with this key, when this catalogue shows it.
    pub fn get(&self, key: &str) -> Option<&MarkDef> {
        self.marks.iter().find(|def| def.key == key)
    }

    /// Whether every key names a badge of this catalogue (a card naming another one was made
    /// with a newer catalogue).
    pub fn knows_all<'a>(&self, keys: impl IntoIterator<Item = &'a String>) -> bool {
        keys.into_iter().all(|key| self.get(key).is_some())
    }

    /// `keys` in this catalogue's order, without repeats; keys it does not know go last, in
    /// their own order (they show again once the catalogue knows them).
    pub fn order(&self, keys: impl IntoIterator<Item = String>) -> Vec<String> {
        let mut known: Vec<(usize, String)> = Vec::new();
        let mut unknown: Vec<String> = Vec::new();
        for key in keys {
            if known.iter().any(|(_, k)| *k == key) || unknown.contains(&key) {
                continue;
            }
            match self.marks.iter().position(|def| def.key == key) {
                Some(at) => known.push((at, key)),
                None => unknown.push(key),
            }
        }
        known.sort_by_key(|(at, _)| *at);
        known
            .into_iter()
            .map(|(_, key)| key)
            .chain(unknown)
            .collect()
    }

    /// `GET /v1/marks` → `{"rev": 3, "marks": [{key, name: {tr, en}, color, glyph, shape}]}`.
    /// Badges this version cannot draw (a newer colour, glyph or shape), with a malformed key or
    /// name, or named twice are left out; None for anything that is not a catalogue.
    pub fn parse(body: &[u8]) -> Option<Self> {
        #[derive(Deserialize)]
        struct Wire {
            rev: i64,
            marks: Vec<serde_json::Value>,
        }
        let wire: Wire = serde_json::from_slice(body).ok()?;
        let mut marks: Vec<MarkDef> = Vec::new();
        for value in wire.marks.into_iter().take(MAX_MARKS) {
            let Ok(def) = serde_json::from_value::<MarkDef>(value) else {
                continue;
            };
            let drawable = MARK_COLORS.contains(&def.color.as_str())
                && MARK_GLYPHS.contains(&def.glyph.as_str())
                && MARK_SHAPES.contains(&def.shape.as_str());
            let named = readable_name(&def.name.tr) && readable_name(&def.name.en);
            if drawable && named && is_mark_key(&def.key) && !marks.iter().any(|d| d.key == def.key)
            {
                marks.push(def);
            }
        }
        Some(Self {
            rev: wire.rev,
            marks,
        })
    }
}

fn readable_name(name: &str) -> bool {
    let count = name.chars().count();
    (1..=MAX_NAME_CHARS).contains(&count) && !name.chars().any(char::is_control)
}

/// `admin`, `verified`, `m12`: lower-case letters, digits, `-` and `_`.
pub fn is_mark_key(key: &str) -> bool {
    (1..=MAX_KEY_CHARS).contains(&key.len())
        && key
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

impl ProfileMark {
    /// The badge key SOC-VERIFY's mark stands for (the built-in catalogue has both).
    pub fn key(self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::Admin => "admin",
        }
    }
}

/// The badges a card shows: its `marks`, or SOC-VERIFY's `mark` from a service without badges.
pub fn shown_keys(marks: &[String], mark: Option<ProfileMark>) -> Vec<String> {
    if marks.is_empty() {
        mark.map(|mark| vec![mark.key().to_owned()])
            .unwrap_or_default()
    } else {
        marks.to_vec()
    }
}

/// SOC-VERIFY's `mark` of a set of shown badges: the admin one before the verified one.
pub fn legacy_mark(keys: &[String]) -> Option<ProfileMark> {
    if keys.iter().any(|key| key == "admin") {
        Some(ProfileMark::Admin)
    } else if keys.iter().any(|key| key == "verified") {
        Some(ProfileMark::Verified)
    } else {
        None
    }
}

/// A list of badge keys as a card or a frame carries it. Anything this version cannot read (not a
/// list, a key that is not text or not a key) reads as no badge: the card around it still reads.
pub(crate) fn readable_keys<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<String>, D::Error> {
    let raw = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(keys_of(raw))
}

/// [`readable_keys`] for a field whose absence says something (`#[serde(default)]` makes an
/// absent one None): present, it is a list, an unreadable one empty.
pub(crate) fn readable_keys_opt<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Vec<String>>, D::Error> {
    let raw = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(Some(keys_of(raw)))
}

fn keys_of(raw: Option<serde_json::Value>) -> Vec<String> {
    let Some(serde_json::Value::Array(items)) = raw else {
        return Vec::new();
    };
    let mut keys: Vec<String> = Vec::new();
    for item in items.into_iter().take(MAX_MARKS) {
        if let serde_json::Value::String(key) = item
            && is_mark_key(&key)
            && !keys.contains(&key)
        {
            keys.push(key);
        }
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(list: &[&str]) -> Vec<String> {
        list.iter().map(|key| (*key).to_owned()).collect()
    }

    #[test]
    fn the_builtin_catalogue_is_the_services_shown_one_in_its_order() {
        let catalog = MarkCatalog::builtin();
        assert_eq!(catalog.rev, 0);
        let order: Vec<&str> = catalog.marks.iter().map(|def| def.key.as_str()).collect();
        assert_eq!(
            order,
            ["admin", "moderator", "staff", "verified", "premium"]
        );
        let premium = catalog.get("premium").unwrap();
        assert_eq!(
            (premium.color.as_str(), premium.glyph.as_str()),
            ("orange", "star")
        );
        assert_eq!(premium.name_in(false), "Premium Club üyesi");
        assert_eq!(premium.name_in(true), "Premium Club member");
        // Every built-in look is one this version draws.
        for def in &catalog.marks {
            assert!(MARK_COLORS.contains(&def.color.as_str()));
            assert!(MARK_GLYPHS.contains(&def.glyph.as_str()));
            assert!(MARK_SHAPES.contains(&def.shape.as_str()));
        }
    }

    #[test]
    fn a_catalogue_reads_and_leaves_out_what_this_version_cannot_draw() {
        let body = br#"{"rev":3,"marks":[
            {"key":"admin","name":{"tr":"Projects yöneticisi","en":"Projects admin"},"color":"red","glyph":"check","shape":"seal"},
            {"key":"m10","name":{"tr":"Yeni","en":"New"},"color":"gold","glyph":"check","shape":"seal"},
            {"key":"m11","name":{"tr":"Kalkan","en":"Shield"},"color":"teal","glyph":"rocket","shape":"seal"},
            {"key":"m12","name":{"tr":"Biçim","en":"Shape"},"color":"teal","glyph":"star","shape":"hex"},
            {"key":"Bad Key","name":{"tr":"a","en":"a"},"color":"teal","glyph":"star","shape":"round"},
            {"key":"m13","name":{"tr":"","en":"x"},"color":"teal","glyph":"star","shape":"round"},
            {"key":"admin","name":{"tr":"İkinci","en":"Second"},"color":"blue","glyph":"star","shape":"round"},
            {"key":"cloud","name":{"tr":"Bulut AFK kullanıcısı","en":"Cloud AFK user"},"color":"teal","glyph":"cloud","shape":"round","extra":1},
            "not a badge"
        ]}"#;
        let catalog = MarkCatalog::parse(body).unwrap();
        assert_eq!(catalog.rev, 3);
        let order: Vec<&str> = catalog.marks.iter().map(|def| def.key.as_str()).collect();
        assert_eq!(order, ["admin", "cloud"]);
        assert_eq!(catalog.get("admin").unwrap().color, "red");
        assert!(MarkCatalog::parse(b"{}").is_none());
        assert!(MarkCatalog::parse(b"not json").is_none());
        assert!(MarkCatalog::parse(br#"{"rev":1,"marks":[]}"#).is_some());
    }

    #[test]
    fn soc_verifys_mark_reads_as_a_list_of_one_and_back() {
        assert_eq!(
            shown_keys(&[], Some(ProfileMark::Admin)),
            keys(&["admin"])
        );
        assert_eq!(
            shown_keys(&[], Some(ProfileMark::Verified)),
            keys(&["verified"])
        );
        assert!(shown_keys(&[], None).is_empty());
        // A list wins over the older field.
        assert_eq!(
            shown_keys(&keys(&["moderator", "premium"]), Some(ProfileMark::Verified)),
            keys(&["moderator", "premium"])
        );
        assert_eq!(
            legacy_mark(&keys(&["moderator", "verified", "admin"])),
            Some(ProfileMark::Admin)
        );
        assert_eq!(
            legacy_mark(&keys(&["staff", "verified"])),
            Some(ProfileMark::Verified)
        );
        assert_eq!(legacy_mark(&keys(&["premium"])), None);
    }

    #[test]
    fn keys_go_in_the_catalogues_order_and_unknown_ones_last() {
        let catalog = MarkCatalog::builtin();
        assert_eq!(
            catalog.order(keys(&["premium", "m20", "admin", "premium", "verified"])),
            keys(&["admin", "verified", "premium", "m20"])
        );
        assert!(catalog.knows_all(&keys(&["admin", "premium"])));
        assert!(!catalog.knows_all(&keys(&["admin", "cloud"])));
    }

    #[test]
    fn a_cards_list_reads_leniently() {
        #[derive(Deserialize)]
        struct Card {
            #[serde(default, deserialize_with = "readable_keys")]
            marks: Vec<String>,
        }
        let read = |json: &str| serde_json::from_str::<Card>(json).unwrap().marks;
        assert_eq!(
            read(r#"{"marks":["admin","premium"]}"#),
            keys(&["admin", "premium"])
        );
        assert!(read(r#"{}"#).is_empty());
        assert!(read(r#"{"marks":null}"#).is_empty());
        assert!(read(r#"{"marks":"admin"}"#).is_empty());
        assert_eq!(
            read(r#"{"marks":["admin",7,"Not A Key","admin","m3"]}"#),
            keys(&["admin", "m3"])
        );
    }
}

//! Item art (IA-ART): the looks a Minecraft server gives its own items and the pictures its menu
//! titles draw, for the cloud inventory views (site, launcher, Android). Studio imports them from
//! the server's ItemsAdder folder and publishes them as `itemart/items.<sha256>.json` with PNG
//! atlases; clients use them only for accounts on one of the document's `hosts` (see
//! `docs/content-format.md`, "Item art").
//!
//! An inventory item names its look by its type plus `custom_model_data` (the first float since
//! Minecraft 1.21.4, a whole number before) or by an `item_model` component. A client draws an
//! item with the entry of the same `model`, else the entry of the same type and `cmd`, else with
//! its ordinary sprite ([`ItemArt::lookup`]).

use std::collections::{HashMap, HashSet};
use std::fmt;

use serde::{Deserialize, Serialize};

use super::{DocRef, check_schema, default_schema};
use crate::error::{Error, Result};

/// Highest [`ItemArt::schema`] this build understands.
pub const ITEM_ART_SCHEMA: u32 = 1;
/// The hosts Studio starts a document with: the Projects server and its subdomains.
pub const DEFAULT_ITEM_ART_HOSTS: [&str; 1] = ["projects.gg"];
/// Most entries of `hosts`.
pub const MAX_ITEM_ART_HOSTS: usize = 16;
/// Most items a document may hold.
pub const MAX_ITEM_ART_ITEMS: usize = 8192;
/// Most glyphs a document may hold.
pub const MAX_ITEM_ART_GLYPHS: usize = 4096;
/// Most cells per atlas row.
pub const MAX_ITEM_ART_ATLAS_COLUMNS: u32 = 128;
/// Longest item or glyph name, in characters.
pub const MAX_ITEM_ART_NAME_CHARS: usize = 64;
/// Longest server id (`namespace:id`) or pack name, in bytes.
pub const MAX_ITEM_ART_ID_LEN: usize = 128;
/// Largest glyph picture edge, in GUI pixels.
pub const MAX_ITEM_ART_GLYPH_EDGE: u32 = 1024;
/// Largest glyph advance or ascent, in GUI pixels (either sign).
pub const MAX_ITEM_ART_GLYPH_OFFSET: i32 = 4096;

/// `itemart/items.<sha256>.json`: a server's own item looks and menu glyphs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ItemArt {
    #[serde(default = "default_schema")]
    pub schema: u32,
    /// The server hosts the art belongs to. An account's server matches when its host equals an
    /// entry or is a subdomain of it, ignoring the port and case ([`item_art_host_matches`]).
    /// Empty: the art applies nowhere.
    #[serde(default)]
    pub hosts: Vec<String>,
    /// The item icons, one atlas per icon size; every atlas has the same cells.
    #[serde(default)]
    pub atlases: Vec<ItemArtAtlas>,
    #[serde(default)]
    pub items: Vec<ItemArtEntry>,
    /// Characters of the server's default font that its menu titles draw (backgrounds, bars,
    /// icons) or move by (offsets).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub glyphs: Vec<ItemArtGlyph>,
}

impl Default for ItemArt {
    fn default() -> Self {
        Self {
            schema: ITEM_ART_SCHEMA,
            hosts: DEFAULT_ITEM_ART_HOSTS.map(str::to_owned).to_vec(),
            atlases: Vec::new(),
            items: Vec::new(),
            glyphs: Vec::new(),
        }
    }
}

/// One PNG of square icon cells, left to right and top to bottom.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ItemArtAtlas {
    /// Edge of a cell in pixels: 16 or 32 today (a multiple of 8 from 8 to 128).
    pub size: u32,
    /// Cells per row: the image is `columns × size` pixels wide.
    pub columns: u32,
    /// Number of cells: the image has `ceil(cells / columns)` rows.
    pub cells: u32,
    /// `itemart/atlas-<size>.<sha256>.png`.
    pub file: DocRef,
}

impl ItemArtAtlas {
    /// The pixel rectangle `(x, y, size)` of a cell.
    pub fn cell(&self, icon: u32) -> Option<(u32, u32, u32)> {
        if icon >= self.cells || self.columns == 0 {
            return None;
        }
        Some((
            (icon % self.columns) * self.size,
            (icon / self.columns) * self.size,
            self.size,
        ))
    }
}

/// One custom item look.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ItemArtEntry {
    /// The item type the look is set on (`minecraft:paper`); with `cmd`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
    /// The `custom_model_data` that picks the look (the first float since 1.21.4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cmd: Option<i32>,
    /// The `item_model` component that picks the look (`projects.gg:kanli_ay/eskisayfa1`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Cell in every atlas. Entries may share a cell.
    pub icon: u32,
    /// The server's own id for the item (ItemsAdder's `namespace:id`), for Studio and support.
    pub id: String,
    /// The item's name as the server's configuration writes it, as plain text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The folder the item came from (ItemsAdder's `contents/<pack>`), so third-party packs show.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pack: Option<String>,
}

/// A character of the server's default font that menu titles use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ItemArtGlyph {
    /// The character: one code point of a private use area (U+E000–U+F8FF, planes 15 and 16).
    #[serde(rename = "char")]
    pub ch: String,
    /// How far the title moves on after it, in GUI pixels; negative moves left.
    pub advance: i32,
    /// The picture; absent for a pure offset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<GlyphImage>,
}

/// The picture of a glyph, drawn in the menu at the glyph's place in the title.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GlyphImage {
    /// `itemart/glyph.<sha256>.png`.
    pub file: DocRef,
    /// Drawn size in GUI pixels (the file may be larger: it is scaled to this).
    pub width: u32,
    pub height: u32,
    /// The font's `ascent`: the picture's top is `7 − ascent` GUI pixels below the top of the
    /// title's line.
    pub ascent: i32,
    /// The server's own id for the picture (ItemsAdder's `namespace:id`).
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pack: Option<String>,
}

/// Why a document breaks a rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemArtProblem {
    TooManyHosts,
    BadHost(String),
    DuplicateHost(String),
    BadAtlas { size: u32, reason: &'static str },
    DuplicateAtlas(u32),
    CellMismatch,
    NoAtlas,
    TooManyItems,
    BadItem { id: String, reason: &'static str },
    DuplicateKey(String),
    TooManyGlyphs,
    BadGlyph { ch: String, reason: &'static str },
    DuplicateGlyph(String),
}

impl fmt::Display for ItemArtProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyHosts => write!(f, "more than {MAX_ITEM_ART_HOSTS} hosts"),
            Self::BadHost(host) => write!(f, "host {host:?} is not a host name or IPv4 address"),
            Self::DuplicateHost(host) => write!(f, "host {host:?} is listed twice"),
            Self::BadAtlas { size, reason } => write!(f, "atlas {size}: {reason}"),
            Self::DuplicateAtlas(size) => write!(f, "two atlases of size {size}"),
            Self::CellMismatch => write!(f, "the atlases hold different numbers of cells"),
            Self::NoAtlas => write!(f, "items without an atlas"),
            Self::TooManyItems => write!(f, "more than {MAX_ITEM_ART_ITEMS} items"),
            Self::BadItem { id, reason } => write!(f, "item {id:?}: {reason}"),
            Self::DuplicateKey(key) => write!(f, "two items for {key}"),
            Self::TooManyGlyphs => write!(f, "more than {MAX_ITEM_ART_GLYPHS} glyphs"),
            Self::BadGlyph { ch, reason } => {
                write!(f, "glyph {}: {reason}", glyph_label(ch))
            }
            Self::DuplicateGlyph(ch) => write!(f, "glyph {} is listed twice", glyph_label(ch)),
        }
    }
}

/// `U+E04E` for a one-character glyph, the escaped text otherwise.
fn glyph_label(ch: &str) -> String {
    let mut chars = ch.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => format!("U+{:04X}", u32::from(c)),
        _ => format!("{ch:?}"),
    }
}

impl ItemArt {
    pub fn check_schema(&self) -> Result<()> {
        check_schema("item art", self.schema, ITEM_ART_SCHEMA)
    }

    /// The schema and every rule; the first problem is the error.
    pub fn validate(&self) -> Result<()> {
        self.check_schema()?;
        match self.problems().into_iter().next() {
            Some(problem) => Err(Error::Invalid(format!("item art: {problem}"))),
            None => Ok(()),
        }
    }

    /// Every rule the document breaks, in document order.
    pub fn problems(&self) -> Vec<ItemArtProblem> {
        let mut problems = Vec::new();
        if self.hosts.len() > MAX_ITEM_ART_HOSTS {
            problems.push(ItemArtProblem::TooManyHosts);
        }
        let mut hosts = HashSet::new();
        for host in &self.hosts {
            if !is_valid_item_art_host(host) {
                problems.push(ItemArtProblem::BadHost(host.clone()));
            } else if !hosts.insert(host.to_ascii_lowercase()) {
                problems.push(ItemArtProblem::DuplicateHost(host.clone()));
            }
        }

        let mut sizes = HashSet::new();
        for atlas in &self.atlases {
            let reason = if atlas.size < 8 || atlas.size > 128 || atlas.size % 8 != 0 {
                Some("the cell size is not a multiple of 8 from 8 to 128")
            } else if atlas.columns == 0 || atlas.columns > MAX_ITEM_ART_ATLAS_COLUMNS {
                Some("bad number of columns")
            } else if atlas.file.validate().is_err() {
                Some("bad file reference")
            } else {
                None
            };
            if let Some(reason) = reason {
                problems.push(ItemArtProblem::BadAtlas {
                    size: atlas.size,
                    reason,
                });
            }
            if !sizes.insert(atlas.size) {
                problems.push(ItemArtProblem::DuplicateAtlas(atlas.size));
            }
        }
        let cells = self.atlases.first().map(|atlas| atlas.cells);
        if self.atlases.iter().any(|atlas| Some(atlas.cells) != cells) {
            problems.push(ItemArtProblem::CellMismatch);
        }
        if !self.items.is_empty() && self.atlases.is_empty() {
            problems.push(ItemArtProblem::NoAtlas);
        }

        if self.items.len() > MAX_ITEM_ART_ITEMS {
            problems.push(ItemArtProblem::TooManyItems);
        }
        let cells = self.atlases.iter().map(|atlas| atlas.cells).min();
        let mut keys = HashSet::new();
        for entry in &self.items {
            if let Some(reason) = entry_problem(entry, cells) {
                problems.push(ItemArtProblem::BadItem {
                    id: entry.id.clone(),
                    reason,
                });
                continue;
            }
            for key in entry_keys(entry) {
                if !keys.insert(key.clone()) {
                    problems.push(ItemArtProblem::DuplicateKey(key));
                }
            }
        }

        if self.glyphs.len() > MAX_ITEM_ART_GLYPHS {
            problems.push(ItemArtProblem::TooManyGlyphs);
        }
        let mut glyphs = HashSet::new();
        for glyph in &self.glyphs {
            if let Some(reason) = glyph_problem(glyph) {
                problems.push(ItemArtProblem::BadGlyph {
                    ch: glyph.ch.clone(),
                    reason,
                });
            } else if !glyphs.insert(glyph.ch.as_str()) {
                problems.push(ItemArtProblem::DuplicateGlyph(glyph.ch.clone()));
            }
        }
        problems
    }

    /// Whether the art belongs to the server at `host` (`play.projects.gg`, with or without a
    /// port).
    pub fn applies_to(&self, host: &str) -> bool {
        item_art_host_matches(host, &self.hosts)
    }

    /// Maps for drawing many items: built once per document.
    pub fn lookup(&self) -> ItemArtLookup<'_> {
        let mut by_model = HashMap::new();
        let mut by_cmd = HashMap::new();
        for entry in &self.items {
            if let Some(model) = &entry.model {
                by_model.entry(model.to_ascii_lowercase()).or_insert(entry);
            }
            if let (Some(item), Some(cmd)) = (&entry.item, entry.cmd) {
                by_cmd
                    .entry((normalize_item_id(item), cmd))
                    .or_insert(entry);
            }
        }
        ItemArtLookup { by_model, by_cmd }
    }

    /// Every file the document references: the atlases, then the glyph pictures.
    pub fn files(&self) -> impl Iterator<Item = &DocRef> {
        self.atlases.iter().map(|atlas| &atlas.file).chain(
            self.glyphs
                .iter()
                .filter_map(|glyph| glyph.image.as_ref().map(|image| &image.file)),
        )
    }

    /// The glyph of a character, if the document has one.
    pub fn glyph(&self, ch: char) -> Option<&ItemArtGlyph> {
        let mut buffer = [0u8; 4];
        let text: &str = ch.encode_utf8(&mut buffer);
        self.glyphs.iter().find(|glyph| glyph.ch == text)
    }
}

/// [`ItemArt::lookup`].
#[derive(Debug, Clone)]
pub struct ItemArtLookup<'a> {
    by_model: HashMap<String, &'a ItemArtEntry>,
    by_cmd: HashMap<(String, i32), &'a ItemArtEntry>,
}

impl<'a> ItemArtLookup<'a> {
    /// The look of an inventory item: the entry of its `model`, else of its type (`item`, with or
    /// without the `minecraft:` namespace) and `cmd`; `None` draws its ordinary sprite.
    pub fn entry(
        &self,
        item: &str,
        cmd: Option<i64>,
        model: Option<&str>,
    ) -> Option<&'a ItemArtEntry> {
        if let Some(entry) = model.and_then(|model| self.by_model.get(&model.to_ascii_lowercase()))
        {
            return Some(entry);
        }
        let cmd = i32::try_from(cmd?).ok()?;
        self.by_cmd.get(&(normalize_item_id(item), cmd)).copied()
    }
}

/// The lookup keys an entry claims: `model:<id>` and `<item>#<cmd>`.
fn entry_keys(entry: &ItemArtEntry) -> Vec<String> {
    let mut keys = Vec::new();
    if let Some(model) = &entry.model {
        keys.push(format!("model:{}", model.to_ascii_lowercase()));
    }
    if let (Some(item), Some(cmd)) = (&entry.item, entry.cmd) {
        keys.push(format!("{}#{cmd}", normalize_item_id(item)));
    }
    keys
}

fn entry_problem(entry: &ItemArtEntry, cells: Option<u32>) -> Option<&'static str> {
    if entry.id.is_empty()
        || entry.id.len() > MAX_ITEM_ART_ID_LEN
        || entry.id.chars().any(char::is_control)
    {
        return Some("bad id");
    }
    match (&entry.item, entry.cmd, &entry.model) {
        (None, None, None) => return Some("neither item and cmd nor model"),
        (Some(_), None, _) => return Some("item without cmd"),
        (None, Some(_), _) => return Some("cmd without item"),
        _ => {}
    }
    if entry
        .item
        .as_deref()
        .is_some_and(|item| !is_resource_location(item))
    {
        return Some("bad item type");
    }
    if entry
        .model
        .as_deref()
        .is_some_and(|model| !is_resource_location(model))
    {
        return Some("bad model id");
    }
    if cells.is_some_and(|cells| entry.icon >= cells) {
        return Some("icon outside the atlas");
    }
    if entry
        .name
        .as_deref()
        .is_some_and(|name| !is_plain_name(name))
    {
        return Some("bad name");
    }
    if entry.pack.as_deref().is_some_and(|pack| !is_plain_id(pack)) {
        return Some("bad pack");
    }
    None
}

fn glyph_problem(glyph: &ItemArtGlyph) -> Option<&'static str> {
    let mut chars = glyph.ch.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if is_private_use(c) => {}
        _ => return Some("not one private use character"),
    }
    if glyph.advance.abs() > MAX_ITEM_ART_GLYPH_OFFSET {
        return Some("advance out of range");
    }
    let Some(image) = &glyph.image else {
        return None;
    };
    if image.width == 0
        || image.height == 0
        || image.width > MAX_ITEM_ART_GLYPH_EDGE
        || image.height > MAX_ITEM_ART_GLYPH_EDGE
    {
        return Some("bad picture size");
    }
    if image.ascent.abs() > MAX_ITEM_ART_GLYPH_OFFSET {
        return Some("ascent out of range");
    }
    if image.file.validate().is_err() {
        return Some("bad file reference");
    }
    if !is_plain_id(&image.id) {
        return Some("bad id");
    }
    if image.pack.as_deref().is_some_and(|pack| !is_plain_id(pack)) {
        return Some("bad pack");
    }
    None
}

/// The private use areas a server font assigns its pictures and offsets to.
pub fn is_private_use(c: char) -> bool {
    matches!(u32::from(c), 0xE000..=0xF8FF | 0xF_0000..=0xF_FFFD | 0x10_0000..=0x10_FFFD)
}

/// `minecraft:paper` for `paper`; ids are compared in lowercase.
pub fn normalize_item_id(item: &str) -> String {
    let item = item.trim().to_ascii_lowercase();
    if item.contains(':') {
        item
    } else {
        format!("minecraft:{item}")
    }
}

/// A namespaced id as Minecraft writes them: `namespace:path`, the namespace from `[a-z0-9_.-]`,
/// the path from `[a-z0-9_.-/]`, at most 256 bytes.
pub fn is_resource_location(id: &str) -> bool {
    let Some((namespace, path)) = id.split_once(':') else {
        return false;
    };
    id.len() <= 256
        && !namespace.is_empty()
        && !path.is_empty()
        && namespace.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'.' | b'-')
        })
        && path.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'.' | b'-' | b'/')
        })
}

fn is_plain_name(name: &str) -> bool {
    let count = name.chars().count();
    (1..=MAX_ITEM_ART_NAME_CHARS).contains(&count)
        && !name.chars().any(|c| c.is_control() || is_private_use(c))
        && name.trim() == name
}

fn is_plain_id(id: &str) -> bool {
    (1..=MAX_ITEM_ART_ID_LEN).contains(&id.len())
        && !id
            .chars()
            .any(|c| c.is_control() || (c.is_whitespace() && c != ' '))
        && id.trim() == id
}

/// A `hosts` entry: a lowercase or mixed-case DNS name (labels of `[A-Za-z0-9-]`, 1–63 bytes,
/// not starting or ending with `-`, at most 253 bytes, at least two labels) or an IPv4 address.
/// No port, scheme or trailing dot.
pub fn is_valid_item_art_host(host: &str) -> bool {
    if host.parse::<std::net::Ipv4Addr>().is_ok() {
        return true;
    }
    let labels: Vec<&str> = host.split('.').collect();
    host.len() <= 253
        && labels.len() >= 2
        && labels.iter().all(|label| {
            (1..=63).contains(&label.len())
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
        && labels
            .last()
            .is_some_and(|tld| tld.bytes().any(|b| b.is_ascii_alphabetic()))
}

/// Whether a server address (`play.projects.gg`, `PLAY.projects.gg.:25565`) is one of `hosts` or
/// a subdomain of one. The port and a trailing dot are ignored, case does not matter; an IPv4
/// entry matches only itself.
pub fn item_art_host_matches(host: &str, hosts: &[String]) -> bool {
    let name = host_name(host);
    if name.is_empty() {
        return false;
    }
    hosts.iter().any(|entry| {
        let entry = entry.trim().trim_end_matches('.').to_ascii_lowercase();
        if entry.is_empty() {
            return false;
        }
        if entry.parse::<std::net::Ipv4Addr>().is_ok() {
            return name == entry;
        }
        name == entry
            || name
                .strip_suffix(entry.as_str())
                .is_some_and(|rest| rest.ends_with('.'))
    })
}

/// The host part of a server address, lowercase, without the port or a trailing dot.
fn host_name(host: &str) -> String {
    let host = host.trim();
    let name = match host.rsplit_once(':') {
        Some((name, port)) if port.bytes().all(|b| b.is_ascii_digit()) && !name.contains(':') => {
            name
        }
        _ => host,
    };
    name.trim_end_matches('.').to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc_ref(path: &str, byte: char) -> DocRef {
        DocRef {
            path: path.into(),
            sha256: byte.to_string().repeat(64),
            size: 100,
        }
    }

    fn entry(id: &str, item: &str, cmd: i32, icon: u32) -> ItemArtEntry {
        ItemArtEntry {
            item: Some(item.into()),
            cmd: Some(cmd),
            model: None,
            icon,
            id: id.into(),
            name: Some("Eski Sayfa".into()),
            pack: Some("pro-items-special".into()),
        }
    }

    fn sample() -> ItemArt {
        ItemArt {
            atlases: vec![
                ItemArtAtlas {
                    size: 16,
                    columns: 32,
                    cells: 3,
                    file: doc_ref("itemart/atlas-16.png", 'a'),
                },
                ItemArtAtlas {
                    size: 32,
                    columns: 32,
                    cells: 3,
                    file: doc_ref("itemart/atlas-32.png", 'b'),
                },
            ],
            items: vec![
                entry("pro-items-special:eskisayfa1", "minecraft:paper", 35, 0),
                entry(
                    "violetset:violet_sword",
                    "minecraft:netherite_sword",
                    110_006,
                    1,
                ),
                ItemArtEntry {
                    item: None,
                    cmd: None,
                    model: Some("projects.gg:kanli_ay/eskisayfa2".into()),
                    icon: 2,
                    id: "pro-items-special:eskisayfa2".into(),
                    name: None,
                    pack: None,
                },
            ],
            glyphs: vec![
                ItemArtGlyph {
                    ch: "\u{E052}".into(),
                    advance: 257,
                    image: Some(GlyphImage {
                        file: doc_ref("itemart/glyph.png", 'c'),
                        width: 256,
                        height: 256,
                        ascent: 36,
                        id: "pro-guis:market".into(),
                        pack: Some("pro-guis".into()),
                    }),
                },
                ItemArtGlyph {
                    ch: "\u{F808}".into(),
                    advance: -8,
                    image: None,
                },
            ],
            ..ItemArt::default()
        }
    }

    #[test]
    fn a_good_document_has_no_problems() {
        let art = sample();
        assert_eq!(art.problems(), Vec::new());
        art.validate().unwrap();
        assert_eq!(art.hosts, ["projects.gg"]);
        let json = serde_json::to_string(&art).unwrap();
        let again: ItemArt = serde_json::from_str(&json).unwrap();
        assert_eq!(again, art);
        assert!(json.contains("\"char\":"), "{json}");
    }

    #[test]
    fn lookup_takes_the_model_first_then_type_and_cmd() {
        let art = sample();
        let lookup = art.lookup();
        let id = |entry: Option<&ItemArtEntry>| entry.map(|e| e.id.clone());
        assert_eq!(
            id(lookup.entry("minecraft:paper", Some(35), None)),
            Some("pro-items-special:eskisayfa1".into())
        );
        assert_eq!(
            id(lookup.entry("PAPER", Some(35), None)),
            Some("pro-items-special:eskisayfa1".into()),
            "bare and upper-case types are normalized"
        );
        assert_eq!(lookup.entry("minecraft:paper", Some(36), None), None);
        assert_eq!(lookup.entry("minecraft:paper", None, None), None);
        assert_eq!(lookup.entry("minecraft:book", Some(35), None), None);
        assert_eq!(
            id(lookup.entry(
                "minecraft:paper",
                Some(35),
                Some("projects.gg:kanli_ay/eskisayfa2")
            )),
            Some("pro-items-special:eskisayfa2".into()),
            "the model wins"
        );
        assert_eq!(
            id(lookup.entry("minecraft:paper", Some(35), Some("other:model"))),
            Some("pro-items-special:eskisayfa1".into()),
            "an unknown model falls back to the type and cmd"
        );
        assert_eq!(
            lookup.entry("minecraft:paper", Some(i64::from(i32::MAX) + 1), None),
            None
        );
    }

    #[test]
    fn hosts_match_themselves_and_their_subdomains() {
        let hosts = vec!["projects.gg".to_owned(), "95.0.0.1".to_owned()];
        for host in [
            "projects.gg",
            "play.projects.gg",
            "PLAY.Projects.GG.",
            "play.projects.gg:25565",
            "  mc.projects.gg ",
            "95.0.0.1",
            "95.0.0.1:25565",
        ] {
            assert!(item_art_host_matches(host, &hosts), "{host}");
        }
        for host in [
            "notprojects.gg",
            "projects.gg.example.com",
            "mc.hypixel.net",
            "projects.gg:abc",
            "195.0.0.1",
            "a.95.0.0.1",
            "",
            ":25565",
        ] {
            assert!(!item_art_host_matches(host, &hosts), "{host}");
        }
        assert!(!item_art_host_matches("play.projects.gg", &[]));
        let art = sample();
        assert!(art.applies_to("play.projects.gg"));
        assert!(!art.applies_to("play.example.com"));
    }

    #[test]
    fn host_entries_are_names_or_ipv4() {
        for host in [
            "projects.gg",
            "Play.Projects.gg",
            "95.0.0.1",
            "a-b.example.com",
        ] {
            assert!(is_valid_item_art_host(host), "{host}");
        }
        for host in [
            "projects",
            "projects.gg.",
            "projects.gg:25565",
            "https://projects.gg",
            "-a.projects.gg",
            "a_b.projects.gg",
            "1.2.3",
            "",
        ] {
            assert!(!is_valid_item_art_host(host), "{host}");
        }
    }

    #[test]
    fn broken_documents_name_their_problem() {
        let mut art = sample();
        art.hosts.push("PROJECTS.gg".into());
        art.hosts.push("bad host".into());
        art.atlases[1].cells = 4;
        art.items.push(entry("dup", "minecraft:paper", 35, 0));
        art.items.push(ItemArtEntry {
            cmd: None,
            ..entry("nocmd", "minecraft:paper", 1, 0)
        });
        art.items.push(entry("far", "minecraft:paper", 40, 9));
        art.items.push(entry("Bad Type", "Minecraft:Paper", 41, 0));
        art.glyphs.push(ItemArtGlyph {
            ch: "A".into(),
            advance: 1,
            image: None,
        });
        art.glyphs.push(art.glyphs[1].clone());
        let problems: Vec<String> = art.problems().iter().map(ToString::to_string).collect();
        assert_eq!(
            problems,
            [
                "host \"PROJECTS.gg\" is listed twice",
                "host \"bad host\" is not a host name or IPv4 address",
                "the atlases hold different numbers of cells",
                "two items for minecraft:paper#35",
                "item \"nocmd\": item without cmd",
                "item \"far\": icon outside the atlas",
                "item \"Bad Type\": bad item type",
                "glyph U+0041: not one private use character",
                "glyph U+F808 is listed twice",
            ]
        );
        assert!(art.validate().is_err());
    }

    #[test]
    fn atlases_and_files() {
        let art = sample();
        assert_eq!(art.atlases[0].cell(0), Some((0, 0, 16)));
        assert_eq!(art.atlases[1].cell(2), Some((64, 0, 32)));
        let wide = ItemArtAtlas {
            columns: 2,
            ..art.atlases[0].clone()
        };
        assert_eq!(wide.cell(2), Some((0, 16, 16)));
        assert_eq!(wide.cell(3), None);
        let files: Vec<&str> = art.files().map(|file| file.path.as_str()).collect();
        assert_eq!(
            files,
            [
                "itemart/atlas-16.png",
                "itemart/atlas-32.png",
                "itemart/glyph.png"
            ]
        );
        assert_eq!(art.glyph('\u{F808}').map(|g| g.advance), Some(-8));
        assert!(art.glyph('\u{E000}').is_none());
        assert!(is_private_use('\u{E000}'));
        assert!(is_private_use('\u{F8FF}'));
        assert!(is_private_use('\u{F8F26}'));
        assert!(!is_private_use('A'));
    }

    #[test]
    fn newer_atlas_sizes_and_unknown_fields_stay_readable() {
        let json = r#"{
            "schema": 1,
            "hosts": ["projects.gg"],
            "atlases": [{"size": 64, "columns": 8, "cells": 1, "file": {"path": "itemart/a.png", "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "size": 1}, "future": true}],
            "items": [{"item": "minecraft:paper", "cmd": 1, "icon": 0, "id": "x:y", "tint": "white"}],
            "later": []
        }"#;
        let art: ItemArt = serde_json::from_str(json).unwrap();
        art.validate().unwrap();
        let newer: ItemArt =
            serde_json::from_str(r#"{"schema": 2, "hosts": [], "atlases": [], "items": []}"#)
                .unwrap();
        assert!(matches!(
            newer.validate(),
            Err(Error::UnsupportedSchema { .. })
        ));
    }
}

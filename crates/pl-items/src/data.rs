//! Vanilla item data for AFK+ orders (afkplus-free.md §1.4): every item of one Minecraft version
//! with its stack size, durability, Turkish and English names (and the grey line the game shows
//! under the few names that repeat: music discs, smithing templates, banner patterns), the aliases
//! players type, and the enchantments that fit it. Built from the game's own files by
//! `examples/build_item_data.rs` (the server's data generator report, its data pack and English
//! language file, the Turkish language file from the version's asset index), served from our CDN
//! under content-addressed names, never committed. The AFK+ agent, the launcher, the site and the
//! phone read the same file; `@pl/ui/items`' `item-data.ts` is the TypeScript reader.
//!
//! The file (format 1): compact JSON, one row per line, keys in a fixed order, the same bytes for
//! the same inputs.
//!
//! ```text
//! {"format":1,"mc":"26.3","src":{"en":"<sha1>","server":"<sha1>","tr":"<sha1>"},
//! "items":[
//! ["minecraft:stone",64,0,"Taş","Stone",-1],
//! ["minecraft:elytra",1,432,"Elitra","Elytra",2,{"a":["elytira","kanat","kanatlar"]}],
//! ["minecraft:music_disc_cat",1,0,"Müzik Diski","Music Disc",-1,{"d":["C418 - cat","C418 - cat"]}]
//! ],
//! "ench":[
//! ["minecraft:binding_curse",1,"Bağlanma Laneti","Curse of Binding",{"a":["binding","baglanma"],"c":1}],
//! ["minecraft:sharpness",5,"Keskinlik","Sharpness",{"a":["sharp","keskin"],"x":[1,4,6,15,35]}]
//! ],
//! "groups":[
//! [2,9,22,37,41]
//! ]}
//! ```
//!
//! - `src`: the SHA-1s of what it was built from, the ones Mojang publishes: `server` (the server
//!   download), `tr` (`tr_tr.json`, the asset index's hash), `en` (`en_us.json` in the jar); for
//!   1.20.5 and 1.20.6, whose enchantments are built into the game, also `enchFrom` (the version
//!   whose enchantment definitions were taken, 1.21) and `ench` (that server download's SHA-1).
//! - An item row is `[id, stack, maxDamage, tr, en, group]`, then an object of extras when it has
//!   any: `a` our aliases (folded, see `fold`; with the word swaps' phrases), `d` the description
//!   line `[tr, en]`. Items are in the game's registry order; `minecraft:air` is left out. Names are
//!   the item's own (`item_name`), Turkish falling back to English as in the game; never a name an
//!   anvil gave.
//! - `id` is the order's item reference (`minecraft:diamond_block`, `^minecraft:[a-z0-9_]{1,64}$`)
//!   and the icon's key in ITEMART's atlas index (`AtlasIndex::icons`). `maxDamage` 0: the item
//!   takes no damage. `group` indexes `groups` (-1: no enchantment fits); a group lists `ench`
//!   indexes, ascending. An enchanted book fits every enchantment.
//! - An enchantment row is `[id, maxLevel, tr, en]`, then extras: `a` aliases, `c` 1 for a curse,
//!   `x` the enchantments it cannot share an item with (both ways: the game refuses a pair either
//!   one's exclusive set names). Enchantments are sorted by id.
//! - Readers ignore extras keys they do not know (format 1 grows only there); a newer `format` is
//!   rejected (`DataError::Newer`).
//!
//! Next to the files, `manifest.json` (`DataManifest`) names each version's file with its SHA-256,
//! which a reader checks before parsing: `{"format": 1, "default": "26.3", "versions": {"26.3":
//! {"file": "26.3/items.<first 12 hex of the SHA-256>.json", "sha256": "<64 hex>", "bytes": N}}}`,
//! `file` relative to the manifest. Our CDN serves the tree at `DATA_BASE` (files for a year,
//! immutable; the manifest for five minutes). `DataManifest::pick` chooses a version's file.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde_json::Value;

use crate::source::{AssetSource, split_id};

/// The item data file's format; readers reject a newer one.
pub const DATA_FORMAT: u32 = 1;

/// The manifest's format.
pub const MANIFEST_FORMAT: u32 = 1;

/// The aliases file's format (`crates/pl-items/data/aliases.json`).
pub const ALIASES_FORMAT: u32 = 1;

/// Where the item data lives on our CDN: `manifest.json` and one folder per version (the same as
/// `@pl/ui/items`' `ITEM_DATA_BASE`). The format is in the path: a later one gets its own tree and
/// readers of this one keep working.
pub const DATA_BASE: &str = "https://cdn.projects.gg/itemdata/v1/";

/// The largest item data file a reader takes (a version's file is about 150 KB).
pub const MAX_DATA_BYTES: usize = 4 * 1024 * 1024;

/// The most aliases one item or enchantment carries (ours and the word swaps').
const MAX_ALIASES: usize = 24;

/// The most word-swap variants one name makes.
const MAX_VARIANTS: usize = 8;

#[derive(Debug, thiserror::Error)]
pub enum DataError {
    #[error("item data: {0}")]
    Invalid(String),
    #[error("item data format {0} is newer than this reader knows")]
    Newer(u64),
    #[error("item data build: {0}")]
    Build(String),
}

fn invalid(message: impl Into<String>) -> DataError {
    DataError::Invalid(message.into())
}

fn build_error(message: impl Into<String>) -> DataError {
    DataError::Build(message.into())
}

/// Search folding for Turkish players, the same as `@pl/ui/search`'s `fold` on Latin text: lower
/// case the Turkish way ("I", "İ" and "ı" all become "i") and marks dropped ("ş" → "s", "ğ" → "g",
/// "â" → "a"). Names and aliases are matched in this form; aliases are stored in it.
pub fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c, 'I' | 'İ' | 'ı') {
            out.push('i');
            continue;
        }
        for lower in c.to_lowercase() {
            if !is_mark(lower) {
                out.push(unmarked(lower));
            }
        }
    }
    out
}

/// Combining marks: what canonical decomposition splits off a letter.
fn is_mark(c: char) -> bool {
    matches!(
        c as u32,
        0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF | 0xFE20..=0xFE2F
    )
}

/// A lower-case Latin letter with a mark (Latin-1 and Latin Extended-A) as what canonical
/// decomposition leaves without the mark. Letters that do not decompose (`æ`, `ø`, `ł`, `đ`) stay.
fn unmarked(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => 'a',
        'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => 'c',
        'ď' => 'd',
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => 'e',
        'ĝ' | 'ğ' | 'ġ' | 'ģ' => 'g',
        'ĥ' => 'h',
        'ì' | 'í' | 'î' | 'ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' => 'i',
        'ĵ' => 'j',
        'ķ' => 'k',
        'ĺ' | 'ļ' | 'ľ' => 'l',
        'ñ' | 'ń' | 'ņ' | 'ň' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ō' | 'ŏ' | 'ő' => 'o',
        'ŕ' | 'ŗ' | 'ř' => 'r',
        'ś' | 'ŝ' | 'ş' | 'š' => 's',
        'ţ' | 'ť' => 't',
        'ù' | 'ú' | 'û' | 'ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => 'u',
        'ŵ' => 'w',
        'ý' | 'ÿ' | 'ŷ' => 'y',
        'ź' | 'ż' | 'ž' => 'z',
        other => other,
    }
}

/// A resource location: `namespace:path`, lower case, as the game writes them.
fn resource_id(id: &str) -> bool {
    let Some((ns, path)) = id.split_once(':') else {
        return false;
    };
    let ok = |s: &str, extra: &[u8]| {
        !s.is_empty()
            && s.bytes().all(|b| {
                b.is_ascii_lowercase()
                    || b.is_ascii_digit()
                    || b"_.-".contains(&b)
                    || extra.contains(&b)
            })
    };
    id.len() <= 128 && ok(ns, b"") && ok(path, b"/")
}

/// A vanilla item or enchantment id an order may name: `^minecraft:[a-z0-9_]{1,64}$` (the
/// Worker's `ItemRef` rule).
pub fn vanilla_id(id: &str) -> bool {
    id.strip_prefix("minecraft:").is_some_and(|path| {
        (1..=64).contains(&path.len())
            && path
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    })
}

/// An alias as stored: folded words of `[a-z0-9]` with single spaces, 2–40 characters.
pub fn alias_ok(alias: &str) -> bool {
    (2..=40).contains(&alias.len())
        && alias.split(' ').all(|w| {
            !w.is_empty()
                && w.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        })
}

fn name_ok(name: &str) -> bool {
    let n = name.chars().count();
    (1..=120).contains(&n) && !name.chars().any(char::is_control) && name.trim() == name
}

/// A name in both languages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Names {
    pub tr: String,
    pub en: String,
}

impl Names {
    /// The name in `lang` ("en"; anything else is Turkish).
    pub fn get(&self, lang: &str) -> &str {
        if lang == "en" { &self.en } else { &self.tr }
    }
}

/// One item of a version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    /// `minecraft:diamond_sword`: the order's item reference and the icon's key in ITEMART's atlas.
    pub id: String,
    /// The most one stack holds (1, 16, 64).
    pub stack: u32,
    /// Durability when new; 0 when the item takes no damage.
    pub max_damage: u32,
    pub name: Names,
    /// The grey line the game shows under a name that repeats ("Music Disc": "C418 - cat").
    pub desc: Option<Names>,
    /// Our aliases, folded ("elitra", "kanat", "dia kilic").
    pub aliases: Vec<String>,
    /// Index into `ItemData::groups`: the enchantments that fit; None when none does.
    pub group: Option<usize>,
}

impl Item {
    pub fn name(&self, lang: &str) -> &str {
        self.name.get(lang)
    }

    pub fn damageable(&self) -> bool {
        self.max_damage > 0
    }
}

/// One enchantment of a version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Enchantment {
    pub id: String,
    pub max_level: u32,
    pub name: Names,
    /// Binding and vanishing (`#minecraft:curse`): an order says whether a cursed item may pass.
    pub curse: bool,
    /// The enchantments it cannot share an item with (indexes into `ItemData::enchantments`).
    pub excludes: Vec<usize>,
    pub aliases: Vec<String>,
}

/// One version's item data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemData {
    pub minecraft: String,
    /// What it was built from: the SHA-1s of the game's files (the ones Mojang publishes).
    pub sources: BTreeMap<String, String>,
    pub items: Vec<Item>,
    pub enchantments: Vec<Enchantment>,
    /// Sets of enchantments (ascending indexes into `enchantments`) items share.
    pub groups: Vec<Vec<usize>>,
    by_id: HashMap<String, usize>,
    enchantment_by_id: HashMap<String, usize>,
}

impl ItemData {
    /// Checks and indexes the parts.
    pub fn new(
        minecraft: String,
        sources: BTreeMap<String, String>,
        items: Vec<Item>,
        enchantments: Vec<Enchantment>,
        groups: Vec<Vec<usize>>,
    ) -> Result<Self, DataError> {
        if minecraft.is_empty() || minecraft.len() > 32 || !name_ok(&minecraft) {
            return Err(invalid("mc"));
        }
        if items.is_empty() {
            return Err(invalid("no items"));
        }
        let mut enchantment_by_id = HashMap::new();
        for (i, e) in enchantments.iter().enumerate() {
            let at = |what: &str| invalid(format!("ench[{i}] {what}"));
            if !resource_id(&e.id) {
                return Err(at("id"));
            }
            if enchantment_by_id.insert(e.id.clone(), i).is_some() {
                return Err(at("id twice"));
            }
            if !(1..=255).contains(&e.max_level) {
                return Err(at("max level"));
            }
            if !name_ok(&e.name.tr) || !name_ok(&e.name.en) {
                return Err(at("name"));
            }
            if !ascending(&e.excludes, enchantments.len()) || e.excludes.contains(&i) {
                return Err(at("excludes"));
            }
            check_aliases(&e.aliases).map_err(at)?;
        }
        for (i, group) in groups.iter().enumerate() {
            if group.is_empty() || !ascending(group, enchantments.len()) {
                return Err(invalid(format!("groups[{i}]")));
            }
        }
        let mut by_id = HashMap::new();
        for (i, item) in items.iter().enumerate() {
            let at = |what: &str| invalid(format!("items[{i}] {what}"));
            if !resource_id(&item.id) {
                return Err(at("id"));
            }
            if by_id.insert(item.id.clone(), i).is_some() {
                return Err(at("id twice"));
            }
            if !(1..=99).contains(&item.stack) {
                return Err(at("stack"));
            }
            if !name_ok(&item.name.tr) || !name_ok(&item.name.en) {
                return Err(at("name"));
            }
            if let Some(desc) = &item.desc
                && (!name_ok(&desc.tr) || !name_ok(&desc.en))
            {
                return Err(at("desc"));
            }
            if item.group.is_some_and(|g| g >= groups.len()) {
                return Err(at("group"));
            }
            check_aliases(&item.aliases).map_err(at)?;
        }
        Ok(Self {
            minecraft,
            sources,
            items,
            enchantments,
            groups,
            by_id,
            enchantment_by_id,
        })
    }

    /// Reads a file (format 1), checking every row.
    pub fn parse(bytes: &[u8]) -> Result<Self, DataError> {
        if bytes.len() > MAX_DATA_BYTES {
            return Err(invalid("too large"));
        }
        let doc: Value =
            serde_json::from_slice(bytes).map_err(|e| invalid(format!("not JSON ({e})")))?;
        let format = doc
            .get("format")
            .and_then(Value::as_u64)
            .ok_or_else(|| invalid("no format"))?;
        if format > u64::from(DATA_FORMAT) {
            return Err(DataError::Newer(format));
        }
        if format != u64::from(DATA_FORMAT) {
            return Err(invalid(format!("format {format}")));
        }
        let minecraft = doc
            .get("mc")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("mc"))?
            .to_owned();
        let mut sources = BTreeMap::new();
        if let Some(src) = doc.get("src") {
            let src = src.as_object().ok_or_else(|| invalid("src"))?;
            for (key, value) in src {
                let value = value
                    .as_str()
                    .ok_or_else(|| invalid(format!("src.{key}")))?;
                sources.insert(key.clone(), value.to_owned());
            }
        }
        let rows = |key: &str| {
            doc.get(key)
                .and_then(Value::as_array)
                .ok_or_else(|| invalid(format!("no {key}")))
        };
        let enchantments = rows("ench")?
            .iter()
            .enumerate()
            .map(|(i, row)| parse_enchantment(i, row))
            .collect::<Result<Vec<_>, _>>()?;
        let groups = rows("groups")?
            .iter()
            .enumerate()
            .map(|(i, row)| {
                row.as_array()
                    .and_then(|list| list.iter().map(index).collect::<Option<Vec<_>>>())
                    .ok_or_else(|| invalid(format!("groups[{i}]")))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let items = rows("items")?
            .iter()
            .enumerate()
            .map(|(i, row)| parse_item(i, row))
            .collect::<Result<Vec<_>, _>>()?;
        Self::new(minecraft, sources, items, enchantments, groups)
    }

    /// The file: compact JSON, one row per line, keys in a fixed order (the same bytes every
    /// time for the same data).
    pub fn to_json(&self) -> Vec<u8> {
        let mut out = String::with_capacity(64 * 1024 + self.items.len() * 96);
        out.push_str("{\"format\":");
        out.push_str(&DATA_FORMAT.to_string());
        out.push_str(",\"mc\":");
        push_json_str(&mut out, &self.minecraft);
        out.push_str(",\"src\":{");
        for (i, (key, value)) in self.sources.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            push_json_str(&mut out, key);
            out.push(':');
            push_json_str(&mut out, value);
        }
        out.push_str("},\n\"items\":[");
        for (i, item) in self.items.iter().enumerate() {
            out.push_str(if i == 0 { "\n" } else { ",\n" });
            push_item(&mut out, item);
        }
        out.push_str("\n],\n\"ench\":[");
        for (i, enchantment) in self.enchantments.iter().enumerate() {
            out.push_str(if i == 0 { "\n" } else { ",\n" });
            push_enchantment(&mut out, enchantment);
        }
        out.push_str("\n],\n\"groups\":[");
        for (i, group) in self.groups.iter().enumerate() {
            out.push_str(if i == 0 { "\n" } else { ",\n" });
            push_numbers(&mut out, group);
        }
        out.push_str("\n]}\n");
        out.into_bytes()
    }

    pub fn item(&self, id: &str) -> Option<&Item> {
        self.by_id.get(id).map(|&i| &self.items[i])
    }

    pub fn enchantment(&self, id: &str) -> Option<&Enchantment> {
        self.enchantment_by_id
            .get(id)
            .map(|&i| &self.enchantments[i])
    }

    /// The enchantments that fit an item (an anvil puts them on it; any on an enchanted book).
    pub fn enchantments_for<'a>(&'a self, item: &Item) -> impl Iterator<Item = &'a Enchantment> {
        item.group
            .map(|g| self.groups[g].as_slice())
            .unwrap_or_default()
            .iter()
            .map(|&i| &self.enchantments[i])
    }

    /// Whether an enchantment fits an item (both by id).
    pub fn fits(&self, item: &str, enchantment: &str) -> bool {
        let (Some(item), Some(&e)) = (self.item(item), self.enchantment_by_id.get(enchantment))
        else {
            return false;
        };
        item.group
            .is_some_and(|g| self.groups[g].binary_search(&e).is_ok())
    }

    /// Whether two enchantments (by id) can be on one item: both known, not the same, and
    /// neither excludes the other (sharpness and smite cannot).
    pub fn compatible(&self, a: &str, b: &str) -> bool {
        match (self.enchantment_by_id.get(a), self.enchantment_by_id.get(b)) {
            (Some(&ia), Some(&ib)) => {
                ia != ib
                    && !self.enchantments[ia].excludes.contains(&ib)
                    && !self.enchantments[ib].excludes.contains(&ia)
            }
            _ => false,
        }
    }
}

/// Folded terms, each once.
fn terms<'a>(names: impl IntoIterator<Item = &'a str>, aliases: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for term in names.into_iter().map(fold).chain(aliases.iter().cloned()) {
        if !term.is_empty() && !out.contains(&term) {
            out.push(term);
        }
    }
    out
}

impl Item {
    /// What a text may call the item, folded: both names, both description lines, our aliases;
    /// each once. A matcher over these finds it ("elmas kilic", "nobetci zirh suslemesi",
    /// "dia kilic").
    pub fn terms(&self) -> Vec<String> {
        let desc = self
            .desc
            .iter()
            .flat_map(|d| [d.tr.as_str(), d.en.as_str()]);
        terms(
            [self.name.tr.as_str(), self.name.en.as_str()]
                .into_iter()
                .chain(desc),
            &self.aliases,
        )
    }
}

impl Enchantment {
    /// What a text may call the enchantment, folded: both names and our aliases, each once.
    pub fn terms(&self) -> Vec<String> {
        terms(
            [self.name.tr.as_str(), self.name.en.as_str()],
            &self.aliases,
        )
    }
}

/// Whether `list` is strictly ascending with every index below `len`.
fn ascending(list: &[usize], len: usize) -> bool {
    list.iter().all(|&i| i < len) && list.windows(2).all(|w| w[0] < w[1])
}

fn check_aliases(aliases: &[String]) -> Result<(), &'static str> {
    if aliases.len() > MAX_ALIASES {
        return Err("too many aliases");
    }
    for (i, alias) in aliases.iter().enumerate() {
        if !alias_ok(alias) {
            return Err("alias");
        }
        if aliases[..i].contains(alias) {
            return Err("alias twice");
        }
    }
    Ok(())
}

fn index(value: &Value) -> Option<usize> {
    value.as_u64().and_then(|v| usize::try_from(v).ok())
}

fn small(value: &Value) -> Option<u32> {
    value.as_u64().and_then(|v| u32::try_from(v).ok())
}

fn strings(value: Option<&Value>) -> Option<Vec<String>> {
    match value {
        None => Some(Vec::new()),
        Some(v) => v
            .as_array()?
            .iter()
            .map(|s| s.as_str().map(str::to_owned))
            .collect(),
    }
}

fn pair(value: &Value) -> Option<Names> {
    match value.as_array()?.as_slice() {
        [tr, en] => Some(Names {
            tr: tr.as_str()?.to_owned(),
            en: en.as_str()?.to_owned(),
        }),
        _ => None,
    }
}

fn parse_item(i: usize, row: &Value) -> Result<Item, DataError> {
    let at = |what: &str| invalid(format!("items[{i}] {what}"));
    let row = row.as_array().ok_or_else(|| at("is not a row"))?;
    if !(6..=7).contains(&row.len()) {
        return Err(at("length"));
    }
    let text = |v: &Value, what: &str| v.as_str().map(str::to_owned).ok_or_else(|| at(what));
    let group = match row[5].as_i64() {
        Some(-1) => None,
        Some(g) => Some(usize::try_from(g).map_err(|_| at("group"))?),
        None => return Err(at("group")),
    };
    let (aliases, desc) = match row.get(6) {
        None => (Vec::new(), None),
        Some(extra) => {
            let extra = extra.as_object().ok_or_else(|| at("extras"))?;
            let aliases = strings(extra.get("a")).ok_or_else(|| at("aliases"))?;
            let desc = match extra.get("d") {
                None => None,
                Some(d) => Some(pair(d).ok_or_else(|| at("desc"))?),
            };
            (aliases, desc)
        }
    };
    Ok(Item {
        id: text(&row[0], "id")?,
        stack: small(&row[1]).ok_or_else(|| at("stack"))?,
        max_damage: small(&row[2]).ok_or_else(|| at("max damage"))?,
        name: Names {
            tr: text(&row[3], "tr")?,
            en: text(&row[4], "en")?,
        },
        desc,
        aliases,
        group,
    })
}

fn parse_enchantment(i: usize, row: &Value) -> Result<Enchantment, DataError> {
    let at = |what: &str| invalid(format!("ench[{i}] {what}"));
    let row = row.as_array().ok_or_else(|| at("is not a row"))?;
    if !(4..=5).contains(&row.len()) {
        return Err(at("length"));
    }
    let text = |v: &Value, what: &str| v.as_str().map(str::to_owned).ok_or_else(|| at(what));
    let (aliases, curse, excludes) = match row.get(4) {
        None => (Vec::new(), false, Vec::new()),
        Some(extra) => {
            let extra = extra.as_object().ok_or_else(|| at("extras"))?;
            let aliases = strings(extra.get("a")).ok_or_else(|| at("aliases"))?;
            let curse = match extra.get("c") {
                None => false,
                Some(c) => c.as_u64().ok_or_else(|| at("curse"))? == 1,
            };
            let excludes = match extra.get("x") {
                None => Vec::new(),
                Some(x) => x
                    .as_array()
                    .and_then(|list| list.iter().map(index).collect::<Option<Vec<_>>>())
                    .ok_or_else(|| at("excludes"))?,
            };
            (aliases, curse, excludes)
        }
    };
    Ok(Enchantment {
        id: text(&row[0], "id")?,
        max_level: small(&row[1]).ok_or_else(|| at("max level"))?,
        name: Names {
            tr: text(&row[2], "tr")?,
            en: text(&row[3], "en")?,
        },
        curse,
        excludes,
        aliases,
    })
}

fn push_json_str(out: &mut String, text: &str) {
    out.push_str(&serde_json::to_string(text).expect("a string serialises"));
}

fn push_numbers(out: &mut String, list: &[usize]) {
    out.push('[');
    for (i, n) in list.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&n.to_string());
    }
    out.push(']');
}

fn push_strings(out: &mut String, list: &[String]) {
    out.push('[');
    for (i, s) in list.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        push_json_str(out, s);
    }
    out.push(']');
}

fn push_item(out: &mut String, item: &Item) {
    out.push('[');
    push_json_str(out, &item.id);
    out.push_str(&format!(",{},{},", item.stack, item.max_damage));
    push_json_str(out, &item.name.tr);
    out.push(',');
    push_json_str(out, &item.name.en);
    match item.group {
        Some(g) => out.push_str(&format!(",{g}")),
        None => out.push_str(",-1"),
    }
    if !item.aliases.is_empty() || item.desc.is_some() {
        out.push_str(",{");
        let mut first = true;
        if !item.aliases.is_empty() {
            out.push_str("\"a\":");
            push_strings(out, &item.aliases);
            first = false;
        }
        if let Some(desc) = &item.desc {
            if !first {
                out.push(',');
            }
            out.push_str("\"d\":");
            push_strings(out, &[desc.tr.clone(), desc.en.clone()]);
        }
        out.push('}');
    }
    out.push(']');
}

fn push_enchantment(out: &mut String, e: &Enchantment) {
    out.push('[');
    push_json_str(out, &e.id);
    out.push_str(&format!(",{},", e.max_level));
    push_json_str(out, &e.name.tr);
    out.push(',');
    push_json_str(out, &e.name.en);
    let mut extras: Vec<String> = Vec::new();
    if !e.aliases.is_empty() {
        let mut a = String::from("\"a\":");
        push_strings(&mut a, &e.aliases);
        extras.push(a);
    }
    if e.curse {
        extras.push("\"c\":1".into());
    }
    if !e.excludes.is_empty() {
        let mut x = String::from("\"x\":");
        push_numbers(&mut x, &e.excludes);
        extras.push(x);
    }
    if !extras.is_empty() {
        out.push_str(",{");
        out.push_str(&extras.join(","));
        out.push('}');
    }
    out.push(']');
}

/// `26.1.2` → `[26, 1, 2]`; each part's leading digits (none: 0), as `@pl/ui/items` compares
/// versions.
fn version_parts(version: &str) -> Vec<u64> {
    version
        .split('.')
        .map(|part| {
            let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
            digits.parse().unwrap_or(0)
        })
        .collect()
}

/// Orders Minecraft versions by their numbers (`1.21.10` after `1.21.9`).
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    let (pa, pb) = (version_parts(a), version_parts(b));
    for i in 0..pa.len().max(pb.len()) {
        let order = pa.get(i).unwrap_or(&0).cmp(pb.get(i).unwrap_or(&0));
        if order != Ordering::Equal {
            return order;
        }
    }
    Ordering::Equal
}

/// One version's file in the manifest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataFile {
    /// Relative to the manifest: `26.3/items.<first 12 hex of its SHA-256>.json`.
    pub file: String,
    /// Its SHA-256 (hex): a reader checks the bytes before parsing them.
    pub sha256: String,
    pub bytes: u64,
}

/// `manifest.json`: each version's file and the version to use when the account's is unknown.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DataManifest {
    pub default: String,
    pub versions: BTreeMap<String, DataFile>,
}

fn file_ok(file: &str) -> bool {
    let Some((version, name)) = file.split_once('/') else {
        return false;
    };
    let hash = name
        .strip_prefix("items.")
        .and_then(|rest| rest.strip_suffix(".json"));
    !version.is_empty()
        && version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        && !version.starts_with('.')
        && hash.is_some_and(|h| {
            h.len() == 12
                && h.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
}

impl DataManifest {
    pub fn parse(bytes: &[u8]) -> Result<Self, DataError> {
        let doc: Value = serde_json::from_slice(bytes)
            .map_err(|e| invalid(format!("manifest: not JSON ({e})")))?;
        let format = doc
            .get("format")
            .and_then(Value::as_u64)
            .ok_or_else(|| invalid("manifest: no format"))?;
        if format > u64::from(MANIFEST_FORMAT) {
            return Err(DataError::Newer(format));
        }
        let default = doc
            .get("default")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let mut versions = BTreeMap::new();
        let list = doc
            .get("versions")
            .and_then(Value::as_object)
            .ok_or_else(|| invalid("manifest: no versions"))?;
        for (version, entry) in list {
            let at = || invalid(format!("manifest: {version}"));
            let file = entry.get("file").and_then(Value::as_str).ok_or_else(at)?;
            let sha256 = entry.get("sha256").and_then(Value::as_str).ok_or_else(at)?;
            let bytes = entry.get("bytes").and_then(Value::as_u64).ok_or_else(at)?;
            if !file_ok(file)
                || sha256.len() != 64
                || !sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                || !file.contains(&sha256[..12])
                || bytes == 0
            {
                return Err(at());
            }
            versions.insert(
                version.clone(),
                DataFile {
                    file: file.to_owned(),
                    sha256: sha256.to_owned(),
                    bytes,
                },
            );
        }
        Ok(Self { default, versions })
    }

    /// The manifest, versions oldest first.
    pub fn to_json(&self) -> Vec<u8> {
        let mut versions: Vec<(&String, &DataFile)> = self.versions.iter().collect();
        versions.sort_by(|a, b| compare_versions(a.0, b.0).then_with(|| a.0.cmp(b.0)));
        let mut out = String::from("{\n  \"format\": ");
        out.push_str(&MANIFEST_FORMAT.to_string());
        out.push_str(",\n  \"default\": ");
        push_json_str(&mut out, &self.default);
        out.push_str(",\n  \"versions\": {");
        for (i, (version, file)) in versions.iter().enumerate() {
            out.push_str(if i == 0 { "\n    " } else { ",\n    " });
            push_json_str(&mut out, version);
            out.push_str(": { \"file\": ");
            push_json_str(&mut out, &file.file);
            out.push_str(", \"sha256\": ");
            push_json_str(&mut out, &file.sha256);
            out.push_str(&format!(", \"bytes\": {} }}", file.bytes));
        }
        out.push_str("\n  }\n}\n");
        out.into_bytes()
    }

    /// The version whose file to read for a game version: its own; else the newest older one (the
    /// items it has); else, for a version older than every file, the oldest (the nearest newer);
    /// with no version asked, the default (else the newest).
    pub fn pick(&self, minecraft: Option<&str>) -> Option<&str> {
        let mut versions: Vec<&str> = self.versions.keys().map(String::as_str).collect();
        versions.sort_by(|a, b| compare_versions(a, b));
        if let Some(wanted) = minecraft {
            if let Some((version, _)) = self.versions.get_key_value(wanted) {
                return Some(version.as_str());
            }
            let older = versions
                .iter()
                .rev()
                .find(|v| compare_versions(v, wanted) != Ordering::Greater);
            return older.or(versions.first()).copied();
        }
        match self.versions.get_key_value(&self.default) {
            Some((version, _)) => Some(version.as_str()),
            None => versions.last().copied(),
        }
    }
}

/// Our aliases (`crates/pl-items/data/aliases.json`): what players type for an item or an
/// enchantment besides its names (slang, short forms, a word of the other language), and word
/// swaps tried on every item name ("elmas" → "dia": "Elmas Kılıç" is also "dia kilic").
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Aliases {
    pub items: BTreeMap<String, Vec<String>>,
    pub enchantments: BTreeMap<String, Vec<String>>,
    pub words: BTreeMap<String, Vec<String>>,
}

impl Aliases {
    /// Reads and checks the file: ids as the game's, aliases folded, no alias for two things.
    pub fn parse(bytes: &[u8]) -> Result<Self, DataError> {
        let doc: Value = serde_json::from_slice(bytes)
            .map_err(|e| invalid(format!("aliases: not JSON ({e})")))?;
        if doc.get("format").and_then(Value::as_u64) != Some(u64::from(ALIASES_FORMAT)) {
            return Err(invalid("aliases: format"));
        }
        let section = |key: &str| -> Result<BTreeMap<String, Vec<String>>, DataError> {
            let mut out = BTreeMap::new();
            let Some(map) = doc.get(key) else {
                return Ok(out);
            };
            let map = map
                .as_object()
                .ok_or_else(|| invalid(format!("aliases: {key}")))?;
            for (name, list) in map {
                let list =
                    strings(Some(list)).ok_or_else(|| invalid(format!("aliases: {key}.{name}")))?;
                out.insert(name.clone(), list);
            }
            Ok(out)
        };
        let aliases = Self {
            items: section("items")?,
            enchantments: section("ench")?,
            words: section("words")?,
        };
        for (what, map) in [("items", &aliases.items), ("ench", &aliases.enchantments)] {
            let mut owner: HashMap<&str, &str> = HashMap::new();
            for (id, list) in map {
                if !vanilla_id(id) {
                    return Err(invalid(format!(
                        "aliases: {what}: {id} is not a vanilla id"
                    )));
                }
                check_aliases(list)
                    .map_err(|why| invalid(format!("aliases: {what}.{id}: {why}")))?;
                for alias in list {
                    if fold(alias) != *alias {
                        return Err(invalid(format!(
                            "aliases: {what}.{id}: {alias} is not folded"
                        )));
                    }
                    if let Some(other) = owner.insert(alias, id) {
                        return Err(invalid(format!(
                            "aliases: {what}: {alias} names both {other} and {id}"
                        )));
                    }
                }
            }
        }
        let word = |w: &str| {
            !w.is_empty()
                && w.len() <= 24
                && w.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        };
        for (from, to) in &aliases.words {
            if !word(from) || to.is_empty() || to.iter().any(|t| !word(t) || t == from) {
                return Err(invalid(format!("aliases: words.{from}")));
            }
        }
        Ok(aliases)
    }
}

/// What the server's data generator reports
/// (`java -DbundlerMainClass=net.minecraft.data.Main -jar server.jar --reports`).
#[derive(Clone, Debug, Default)]
pub struct Report {
    /// Every item in the game's registry order, with its default components.
    pub items: Vec<(String, Value)>,
    /// The enchantment registry, when the report lists it: before 1.21 enchantments were built
    /// into the game (no data files); since, they are data and the registry is not reported.
    pub enchantments: Vec<String>,
}

fn registry_ids(registries: &Value, name: &str) -> Option<Vec<String>> {
    let entries = registries.get(name)?.get("entries")?.as_object()?;
    let mut ids: Vec<(i64, &String)> = entries
        .iter()
        .map(|(id, entry)| {
            let order = entry
                .get("protocol_id")
                .and_then(Value::as_i64)
                .unwrap_or(i64::MAX);
            (order, id)
        })
        .collect();
    ids.sort();
    Some(ids.into_iter().map(|(_, id)| id.clone()).collect())
}

/// An item's default components as one map: reports before 1.21.2 list them as
/// `[{"type": …, "value": …}]`.
fn component_map(components: Value) -> Option<Value> {
    match components {
        Value::Object(_) => Some(components),
        Value::Array(list) => {
            let mut map = serde_json::Map::new();
            for entry in list {
                let kind = entry.get("type")?.as_str()?.to_owned();
                map.insert(kind, entry.get("value").cloned().unwrap_or(Value::Null));
            }
            Some(Value::Object(map))
        }
        _ => None,
    }
}

impl Report {
    /// Reads a report folder through `read` (a file by its path in the folder): `registries.json`
    /// and the item components, either one `items.json` (components as a list in 1.20.5–1.21.1,
    /// a map later) or one file per item (`minecraft/components/item/<path>.json`, 26.x).
    pub fn read(read: &mut dyn FnMut(&str) -> Option<Vec<u8>>) -> Result<Self, DataError> {
        let registries: Value = read("registries.json")
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .ok_or_else(|| build_error("report: no registries.json"))?;
        let ids = registry_ids(&registries, "minecraft:item")
            .ok_or_else(|| build_error("report: registries.json has no item registry"))?;
        let enchantments = registry_ids(&registries, "minecraft:enchantment").unwrap_or_default();
        let all: Option<Value> =
            read("items.json").and_then(|bytes| serde_json::from_slice(&bytes).ok());
        let mut items = Vec::with_capacity(ids.len());
        for id in ids {
            let entry = match &all {
                Some(all) => all.get(&id).cloned(),
                None => {
                    let (ns, path) = split_id(&id);
                    read(&format!("{ns}/components/item/{path}.json"))
                        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                }
            };
            let components = entry
                .and_then(|e| e.get("components").cloned())
                .and_then(component_map)
                .ok_or_else(|| build_error(format!("report: no components for {id}")))?;
            items.push((id, components));
        }
        Ok(Self {
            items,
            enchantments,
        })
    }
}

/// What one version's item data is built from.
pub struct DataInputs<'a> {
    pub minecraft: &'a str,
    /// The version's server jar (the inner jar of Mojang's bundle) or client jar: the English
    /// language file, the enchantments and the tags.
    pub jar: &'a mut dyn AssetSource,
    /// A newer version's jar for the enchantments' definitions when this one has none (before
    /// 1.21): only the enchantments the report lists are taken; tags are read from `jar` first.
    pub donor: Option<&'a mut dyn AssetSource>,
    pub report: &'a Report,
    /// The Turkish language file (`tr_tr.json` from the version's asset index); a missing name
    /// falls back to English, as in the game.
    pub tr: &'a HashMap<String, String>,
    pub aliases: &'a Aliases,
    /// The input files' SHA-1s, written into the file.
    pub sources: BTreeMap<String, String>,
}

/// A built version and what the build noticed (fallbacks, repeated names, dropped aliases).
pub struct DataBuild {
    pub data: ItemData,
    pub notes: Vec<String>,
}

/// The jar, then the donor.
struct Files<'s, 'a> {
    jar: &'s mut dyn AssetSource,
    donor: Option<&'s mut (dyn AssetSource + 'a)>,
}

impl Files<'_, '_> {
    fn json(&mut self, paths: &[String]) -> Option<Value> {
        let parse = |bytes: Vec<u8>| serde_json::from_slice::<Value>(&bytes).ok();
        for path in paths {
            if let Some(v) = self.jar.read(path).and_then(parse) {
                return Some(v);
            }
        }
        let donor = self.donor.as_mut()?;
        paths
            .iter()
            .find_map(|path| donor.read(path).and_then(parse))
    }

    /// A tag's own entries (`data/<ns>/tags/<dir>/<path>.json`, each dir tried).
    fn tag(&mut self, dirs: &[&str], id: &str) -> Option<Vec<Value>> {
        let (ns, path) = split_id(id);
        let paths: Vec<String> = dirs
            .iter()
            .map(|dir| format!("data/{ns}/tags/{dir}/{path}.json"))
            .collect();
        let doc = self.json(&paths)?;
        doc.get("values").and_then(Value::as_array).cloned()
    }

    /// Everything a tag holds, nested tags followed; optional entries may be missing.
    fn resolve(
        &mut self,
        dirs: &[&str],
        id: &str,
        out: &mut BTreeSet<String>,
        seen: &mut BTreeSet<String>,
    ) -> Result<(), DataError> {
        if !seen.insert(id.to_owned()) {
            return Ok(());
        }
        let Some(values) = self.tag(dirs, id) else {
            return Err(build_error(format!("no tag #{id}")));
        };
        for value in values {
            let (entry, required) = match &value {
                Value::String(s) => (s.as_str(), true),
                Value::Object(o) => (
                    o.get("id").and_then(Value::as_str).unwrap_or_default(),
                    o.get("required").and_then(Value::as_bool).unwrap_or(true),
                ),
                _ => return Err(build_error(format!("tag #{id}: odd entry"))),
            };
            match entry.strip_prefix('#') {
                Some(tag) => match self.resolve(dirs, &full(tag), out, seen) {
                    Err(_) if !required => {}
                    other => other?,
                },
                None if !entry.is_empty() => {
                    out.insert(full(entry));
                }
                None => return Err(build_error(format!("tag #{id}: odd entry"))),
            }
        }
        Ok(())
    }

    /// A holder set as data files write it: `#tag`, an id, or a list of either.
    fn holders(&mut self, dirs: &[&str], value: &Value) -> Result<BTreeSet<String>, DataError> {
        let mut out = BTreeSet::new();
        let list: Vec<&str> = match value {
            Value::String(s) => vec![s.as_str()],
            Value::Array(list) => list.iter().filter_map(Value::as_str).collect(),
            _ => return Err(build_error("odd holder set")),
        };
        for entry in list {
            match entry.strip_prefix('#') {
                Some(tag) => self.resolve(dirs, &full(tag), &mut out, &mut BTreeSet::new())?,
                None => {
                    out.insert(full(entry));
                }
            }
        }
        Ok(out)
    }
}

/// `stone` → `minecraft:stone`.
fn full(id: &str) -> String {
    let (ns, path) = split_id(id);
    format!("{ns}:{path}")
}

const ITEM_TAGS: &[&str] = &["item", "items"];
const ENCHANTMENT_TAGS: &[&str] = &["enchantment", "enchantments"];

/// A text component's names: a translation (English required; Turkish falls back to it) or
/// plain text. Also the translation key. Reports of 1.21.2–1.21.11 write components as JSON text
/// (`"{\"translate\":\"item.minecraft.stone\"}"`).
fn text_names(
    component: &Value,
    en: &HashMap<String, String>,
    tr: &HashMap<String, String>,
) -> Option<(Names, Option<String>)> {
    match component {
        Value::String(text) => {
            if let Ok(inner) = serde_json::from_str::<Value>(text)
                && (inner.is_object() || inner.is_string())
            {
                return text_names(&inner, en, tr);
            }
            Some((
                Names {
                    tr: text.clone(),
                    en: text.clone(),
                },
                None,
            ))
        }
        Value::Object(o) => {
            if let Some(key) = o.get("translate").and_then(Value::as_str) {
                let english = en
                    .get(key)
                    .cloned()
                    .or_else(|| o.get("fallback").and_then(Value::as_str).map(str::to_owned))?;
                let turkish = tr.get(key).cloned().unwrap_or_else(|| english.clone());
                return Some((
                    Names {
                        tr: turkish,
                        en: english,
                    },
                    Some(key.to_owned()),
                ));
            }
            let text = o.get("text").and_then(Value::as_str)?;
            Some((
                Names {
                    tr: text.to_owned(),
                    en: text.to_owned(),
                },
                None,
            ))
        }
        _ => None,
    }
}

fn lang_names(
    key: &str,
    en: &HashMap<String, String>,
    tr: &HashMap<String, String>,
) -> Option<Names> {
    let english = en.get(key)?;
    Some(Names {
        tr: tr.get(key).unwrap_or(english).clone(),
        en: english.clone(),
    })
}

/// The grey line under a name the game repeats: `<name key>.desc` (music discs, banner patterns),
/// a trim template's pattern, an upgrade template's upgrade.
fn description(
    path: &str,
    key: Option<&str>,
    en: &HashMap<String, String>,
    tr: &HashMap<String, String>,
) -> Option<Names> {
    let mut keys: Vec<String> = Vec::new();
    if let Some(key) = key {
        keys.push(format!("{key}.desc"));
    }
    if let Some(pattern) = path.strip_suffix("_armor_trim_smithing_template") {
        keys.push(format!("trim_pattern.minecraft.{pattern}"));
    } else if let Some(upgrade) = path.strip_suffix("_smithing_template") {
        keys.push(format!("upgrade.minecraft.{upgrade}"));
    }
    keys.iter().find_map(|k| lang_names(k, en, tr))
}

/// Every phrase a name becomes with the word swaps (the name itself left out), at most
/// `MAX_VARIANTS`.
fn variants(name: &str, words: &BTreeMap<String, Vec<String>>) -> Vec<String> {
    let mut phrases: Vec<Vec<&str>> = vec![Vec::new()];
    for word in name.split(' ') {
        let mut choices: Vec<&str> = vec![word];
        if let Some(swaps) = words.get(word) {
            choices.extend(swaps.iter().map(String::as_str));
        }
        let mut next = Vec::new();
        for phrase in &phrases {
            for choice in &choices {
                let mut longer = phrase.clone();
                longer.push(choice);
                next.push(longer);
            }
        }
        next.truncate(MAX_VARIANTS + 1);
        phrases = next;
    }
    phrases
        .into_iter()
        .map(|p| p.join(" "))
        .filter(|p| p != name)
        .take(MAX_VARIANTS)
        .collect()
}

/// Builds one version's item data from the game's files and our aliases.
pub fn build_item_data(inputs: DataInputs<'_>) -> Result<DataBuild, DataError> {
    let DataInputs {
        minecraft,
        jar,
        donor,
        report,
        tr,
        aliases,
        sources,
    } = inputs;
    let mut notes: Vec<String> = Vec::new();
    let en: HashMap<String, String> = jar
        .read("assets/minecraft/lang/en_us.json")
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .ok_or_else(|| build_error("the jar has no assets/minecraft/lang/en_us.json"))?;

    // Items, in the registry's order.
    let mut items: Vec<Item> = Vec::new();
    let mut turkish_fallbacks: Vec<String> = Vec::new();
    for (id, components) in &report.items {
        if id == "minecraft:air" {
            continue;
        }
        if !vanilla_id(id) {
            return Err(build_error(format!("unexpected item id {id}")));
        }
        let (_, path) = split_id(id);
        let stack = match components
            .get("minecraft:max_stack_size")
            .and_then(Value::as_u64)
        {
            Some(n) => u32::try_from(n).map_err(|_| build_error(format!("{id}: stack")))?,
            None => {
                notes.push(format!("{id}: no max_stack_size, 64"));
                64
            }
        };
        let max_damage = components
            .get("minecraft:max_damage")
            .and_then(Value::as_u64)
            .map_or(Ok(0), u32::try_from)
            .map_err(|_| build_error(format!("{id}: max damage")))?;
        let (name, key) = match components.get("minecraft:item_name") {
            Some(component) => text_names(component, &en, tr),
            None => [
                format!("item.minecraft.{path}"),
                format!("block.minecraft.{path}"),
            ]
            .into_iter()
            .find_map(|key| lang_names(&key, &en, tr).map(|names| (names, Some(key)))),
        }
        .ok_or_else(|| build_error(format!("{id}: no English name")))?;
        if let Some(key) = &key
            && !tr.contains_key(key)
        {
            turkish_fallbacks.push(id.clone());
        }
        let desc = description(path, key.as_deref(), &en, tr);
        items.push(Item {
            id: id.clone(),
            stack,
            max_damage,
            name,
            desc,
            aliases: Vec::new(),
            group: None,
        });
    }
    if !turkish_fallbacks.is_empty() {
        notes.push(format!(
            "{} names not in tr_tr.json, English used: {}",
            turkish_fallbacks.len(),
            turkish_fallbacks.join(", ")
        ));
    }

    // Enchantments: the version's own data, else the donor's for the registry's enchantments.
    let mut files = Files { jar, donor };
    let own = files.jar.list("data/minecraft/enchantment", ".json");
    let (listed, from_donor): (Vec<String>, bool) = if own.is_empty() {
        let Some(donor) = files.donor.as_mut() else {
            return Err(build_error(
                "the jar has no enchantment data (before 1.21): give a newer jar as the donor",
            ));
        };
        (donor.list("data/minecraft/enchantment", ".json"), true)
    } else {
        (own, false)
    };
    let mut definitions: BTreeMap<String, Value> = BTreeMap::new();
    for path in &listed {
        let Some(name) = path
            .strip_prefix("data/minecraft/enchantment/")
            .and_then(|p| p.strip_suffix(".json"))
        else {
            continue;
        };
        let id = format!("minecraft:{name}");
        if !report.enchantments.is_empty() && !report.enchantments.contains(&id) {
            continue;
        }
        let json = files
            .json(std::slice::from_ref(path))
            .ok_or_else(|| build_error(format!("{path}: not JSON")))?;
        definitions.insert(id, json);
    }
    for id in &report.enchantments {
        if !definitions.contains_key(id) {
            return Err(build_error(format!(
                "no definition for the enchantment {id}"
            )));
        }
    }
    if from_donor {
        notes.push(format!(
            "{} enchantments defined by the donor jar",
            definitions.len()
        ));
    }
    let curses = {
        let mut out = BTreeSet::new();
        match files.resolve(
            ENCHANTMENT_TAGS,
            "minecraft:curse",
            &mut out,
            &mut BTreeSet::new(),
        ) {
            Ok(()) => out,
            Err(_) => {
                notes.push("no #minecraft:curse tag".into());
                BTreeSet::new()
            }
        }
    };
    let ids: Vec<String> = definitions.keys().cloned().collect();
    let mut enchantments: Vec<Enchantment> = Vec::new();
    let mut supported: Vec<BTreeSet<String>> = Vec::new();
    for (id, def) in &definitions {
        let max_level = def
            .get("max_level")
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| build_error(format!("{id}: max_level")))?;
        let (name, _) = def
            .get("description")
            .and_then(|d| text_names(d, &en, tr))
            .ok_or_else(|| build_error(format!("{id}: no English name")))?;
        let items_value = def
            .get("supported_items")
            .ok_or_else(|| build_error(format!("{id}: supported_items")))?;
        supported.push(
            files
                .holders(ITEM_TAGS, items_value)
                .map_err(|e| build_error(format!("{id}: {e}")))?,
        );
        let excluded = match def.get("exclusive_set") {
            Some(set) => files
                .holders(ENCHANTMENT_TAGS, set)
                .map_err(|e| build_error(format!("{id}: {e}")))?,
            None => BTreeSet::new(),
        };
        let excludes: Vec<usize> = excluded
            .iter()
            .filter(|other| *other != id)
            .filter_map(|other| ids.binary_search(other).ok())
            .collect();
        enchantments.push(Enchantment {
            id: id.clone(),
            max_level,
            name,
            curse: curses.contains(id),
            excludes,
            aliases: Vec::new(),
        });
    }
    // The game checks both enchantments' sets: a pair either one excludes cannot share an item.
    let pairs: Vec<(usize, usize)> = enchantments
        .iter()
        .enumerate()
        .flat_map(|(i, e)| e.excludes.iter().map(move |&j| (j, i)))
        .collect();
    for (i, j) in pairs {
        if !enchantments[i].excludes.contains(&j) {
            enchantments[i].excludes.push(j);
            enchantments[i].excludes.sort_unstable();
        }
    }

    // Groups: the enchantments that fit each item; any on an enchanted book.
    let all: Vec<usize> = (0..enchantments.len()).collect();
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut group_of: HashMap<Vec<usize>, usize> = HashMap::new();
    for item in &mut items {
        let fitting: Vec<usize> = if item.id == "minecraft:enchanted_book" {
            all.clone()
        } else {
            supported
                .iter()
                .enumerate()
                .filter(|(_, set)| set.contains(&item.id))
                .map(|(i, _)| i)
                .collect()
        };
        if fitting.is_empty() {
            continue;
        }
        let next = groups.len();
        let g = *group_of.entry(fitting.clone()).or_insert(next);
        if g == next {
            groups.push(fitting);
        }
        item.group = Some(g);
    }

    apply_item_aliases(&mut items, aliases, &mut notes)?;
    apply_enchantment_aliases(&mut enchantments, aliases, &mut notes)?;
    note_repeated_names(&items, &mut notes);

    let data = ItemData::new(minecraft.to_owned(), sources, items, enchantments, groups)?;
    Ok(DataBuild { data, notes })
}

/// Our aliases onto the items: each alias is the item's alone (never another's name or alias);
/// the word swaps' phrases only when no other item has the same.
fn apply_item_aliases(
    items: &mut [Item],
    aliases: &Aliases,
    notes: &mut Vec<String>,
) -> Result<(), DataError> {
    // Folded names → the items that carry them.
    let mut names: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, item) in items.iter().enumerate() {
        for name in [fold(&item.name.tr), fold(&item.name.en)] {
            let owners = names.entry(name).or_default();
            if !owners.contains(&i) {
                owners.push(i);
            }
        }
    }
    let index: HashMap<&str, usize> = items
        .iter()
        .enumerate()
        .map(|(i, item)| (item.id.as_str(), i))
        .collect();
    let mut taken: HashMap<String, usize> = HashMap::new();
    let mut explicit: Vec<Vec<String>> = vec![Vec::new(); items.len()];
    let mut absent: Vec<&str> = Vec::new();
    let mut redundant: Vec<String> = Vec::new();
    for (id, list) in &aliases.items {
        let Some(&i) = index.get(id.as_str()) else {
            absent.push(id);
            continue;
        };
        for alias in list {
            match names.get(alias) {
                Some(owners) if owners.contains(&i) => {
                    redundant.push(alias.clone());
                    continue;
                }
                Some(owners) => {
                    return Err(build_error(format!(
                        "alias {alias} of {id} is the name of {}",
                        items[owners[0]].id
                    )));
                }
                None => {}
            }
            taken.insert(alias.clone(), i);
            explicit[i].push(alias.clone());
        }
    }
    if !absent.is_empty() {
        notes.push(format!(
            "aliases for items not in this version: {}",
            absent.join(", ")
        ));
    }
    if !redundant.is_empty() {
        notes.push(format!(
            "aliases equal to their own name, left out: {}",
            redundant.join(", ")
        ));
    }
    // Word swaps: phrases no name, alias or other item's phrase has.
    let mut phrases: HashMap<String, Vec<usize>> = HashMap::new();
    if !aliases.words.is_empty() {
        for (i, item) in items.iter().enumerate() {
            for name in [fold(&item.name.tr), fold(&item.name.en)] {
                for phrase in variants(&name, &aliases.words) {
                    if names.contains_key(&phrase) || taken.contains_key(&phrase) {
                        continue;
                    }
                    let owners = phrases.entry(phrase).or_default();
                    if !owners.contains(&i) {
                        owners.push(i);
                    }
                }
            }
        }
    }
    let mut swapped: Vec<Vec<String>> = vec![Vec::new(); items.len()];
    let mut shared = 0;
    for (phrase, owners) in phrases {
        match owners.as_slice() {
            [i] => swapped[*i].push(phrase),
            _ => shared += 1,
        }
    }
    let mut made = 0;
    for (i, item) in items.iter_mut().enumerate() {
        let mut list = std::mem::take(&mut explicit[i]);
        swapped[i].sort();
        for phrase in swapped[i].drain(..) {
            if list.len() < MAX_ALIASES {
                list.push(phrase);
                made += 1;
            }
        }
        list.truncate(MAX_ALIASES);
        item.aliases = list;
    }
    if made > 0 || shared > 0 {
        notes.push(format!(
            "word swaps: {made} aliases made, {shared} left out (more than one item)"
        ));
    }
    Ok(())
}

fn apply_enchantment_aliases(
    enchantments: &mut [Enchantment],
    aliases: &Aliases,
    notes: &mut Vec<String>,
) -> Result<(), DataError> {
    let names: HashMap<String, usize> = enchantments
        .iter()
        .enumerate()
        .flat_map(|(i, e)| [(fold(&e.name.tr), i), (fold(&e.name.en), i)])
        .collect();
    let mut absent: Vec<&str> = Vec::new();
    for (id, list) in &aliases.enchantments {
        let Some(i) = enchantments.iter().position(|e| &e.id == id) else {
            absent.push(id);
            continue;
        };
        for alias in list {
            match names.get(alias) {
                Some(&owner) if owner == i => continue,
                Some(&owner) => {
                    return Err(build_error(format!(
                        "alias {alias} of {id} is the name of {}",
                        enchantments[owner].id
                    )));
                }
                None => enchantments[i].aliases.push(alias.clone()),
            }
        }
    }
    if !absent.is_empty() {
        notes.push(format!(
            "aliases for enchantments not in this version: {}",
            absent.join(", ")
        ));
    }
    Ok(())
}

/// Notes the names two items share (with their description lines): a picker shows the icon or
/// the other language beside them, the assistant asks which one.
fn note_repeated_names(items: &[Item], notes: &mut Vec<String>) {
    for lang in ["en", "tr"] {
        let mut seen: BTreeMap<(String, String), Vec<&str>> = BTreeMap::new();
        for item in items {
            let desc = item
                .desc
                .as_ref()
                .map(|d| d.get(lang).to_owned())
                .unwrap_or_default();
            seen.entry((item.name(lang).to_owned(), desc))
                .or_default()
                .push(&item.id);
        }
        let repeated: Vec<String> = seen
            .into_iter()
            .filter(|(_, ids)| ids.len() > 1)
            .map(|((name, _), ids)| format!("{name} ({})", ids.join(", ")))
            .collect();
        if !repeated.is_empty() {
            notes.push(format!("{lang} names shared: {}", repeated.join("; ")));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::MemSource;

    fn map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn json_bytes(value: Value) -> Vec<u8> {
        serde_json::to_vec(&value).unwrap()
    }

    const EN: &[(&str, &str)] = &[
        ("block.minecraft.stone", "Stone"),
        ("block.minecraft.obsidian", "Obsidian"),
        ("item.minecraft.diamond", "Diamond"),
        ("item.minecraft.diamond_sword", "Diamond Sword"),
        ("item.minecraft.iron_axe", "Iron Axe"),
        ("item.minecraft.diamond_helmet", "Diamond Helmet"),
        ("item.minecraft.elytra", "Elytra"),
        ("item.minecraft.enchanted_book", "Enchanted Book"),
        ("item.minecraft.music_disc_cat", "Music Disc"),
        ("item.minecraft.music_disc_cat.desc", "C418 - cat"),
        ("item.minecraft.music_disc_13", "Music Disc"),
        ("item.minecraft.music_disc_13.desc", "C418 - 13"),
        (
            "item.minecraft.sentry_armor_trim_smithing_template",
            "Smithing Template",
        ),
        ("trim_pattern.minecraft.sentry", "Sentry Armor Trim"),
        (
            "item.minecraft.netherite_upgrade_smithing_template",
            "Smithing Template",
        ),
        ("upgrade.minecraft.netherite_upgrade", "Netherite Upgrade"),
        ("item.minecraft.beacon_like", "Beacon"),
        ("block.minecraft.lantern", "Lantern"),
        ("enchantment.minecraft.sharpness", "Sharpness"),
        ("enchantment.minecraft.smite", "Smite"),
        ("enchantment.minecraft.mending", "Mending"),
        ("enchantment.minecraft.binding_curse", "Curse of Binding"),
        ("enchantment.minecraft.protection", "Protection"),
    ];

    const TR: &[(&str, &str)] = &[
        ("block.minecraft.stone", "Taş"),
        ("block.minecraft.obsidian", "Obsidyen"),
        ("item.minecraft.diamond", "Elmas"),
        ("item.minecraft.diamond_sword", "Elmas Kılıç"),
        ("item.minecraft.iron_axe", "Demir Balta"),
        ("item.minecraft.diamond_helmet", "Elmas Miğfer"),
        ("item.minecraft.enchanted_book", "Büyülü Kitap"),
        ("item.minecraft.music_disc_cat", "Müzik Diski"),
        ("item.minecraft.music_disc_13", "Müzik Diski"),
        (
            "item.minecraft.sentry_armor_trim_smithing_template",
            "Demirci Şablonu",
        ),
        ("trim_pattern.minecraft.sentry", "Nöbetçi Zırh Süslemesi"),
        (
            "item.minecraft.netherite_upgrade_smithing_template",
            "Demirci Şablonu",
        ),
        (
            "upgrade.minecraft.netherite_upgrade",
            "Netherit Yükseltmesi",
        ),
        ("item.minecraft.beacon_like", "Fener"),
        ("block.minecraft.lantern", "Fener"),
        ("enchantment.minecraft.sharpness", "Keskinlik"),
        ("enchantment.minecraft.smite", "Çarpma"),
        ("enchantment.minecraft.mending", "Onarım"),
        ("enchantment.minecraft.binding_curse", "Bağlanma Laneti"),
        ("enchantment.minecraft.protection", "Koruma"),
    ];

    /// (id, stack, max damage, item_name key) in registry order; stone has no item_name (as in
    /// old reports) and falls back to its block key.
    const ITEMS: &[(&str, u64, u64, Option<&str>)] = &[
        ("minecraft:air", 64, 0, Some("block.minecraft.air")),
        ("minecraft:stone", 64, 0, None),
        (
            "minecraft:obsidian",
            64,
            0,
            Some("block.minecraft.obsidian"),
        ),
        ("minecraft:diamond", 64, 0, Some("item.minecraft.diamond")),
        (
            "minecraft:diamond_sword",
            1,
            1561,
            Some("item.minecraft.diamond_sword"),
        ),
        (
            "minecraft:iron_axe",
            1,
            250,
            Some("item.minecraft.iron_axe"),
        ),
        (
            "minecraft:diamond_helmet",
            1,
            363,
            Some("item.minecraft.diamond_helmet"),
        ),
        ("minecraft:elytra", 1, 432, Some("item.minecraft.elytra")),
        (
            "minecraft:enchanted_book",
            1,
            0,
            Some("item.minecraft.enchanted_book"),
        ),
        (
            "minecraft:music_disc_cat",
            1,
            0,
            Some("item.minecraft.music_disc_cat"),
        ),
        (
            "minecraft:music_disc_13",
            1,
            0,
            Some("item.minecraft.music_disc_13"),
        ),
        (
            "minecraft:sentry_armor_trim_smithing_template",
            64,
            0,
            Some("item.minecraft.sentry_armor_trim_smithing_template"),
        ),
        (
            "minecraft:netherite_upgrade_smithing_template",
            64,
            0,
            Some("item.minecraft.netherite_upgrade_smithing_template"),
        ),
        (
            "minecraft:beacon_like",
            64,
            0,
            Some("item.minecraft.beacon_like"),
        ),
        ("minecraft:lantern", 64, 0, Some("block.minecraft.lantern")),
    ];

    fn components(stack: u64, damage: u64, key: Option<&str>) -> Value {
        let mut c =
            serde_json::json!({ "minecraft:max_stack_size": stack, "minecraft:rarity": "common" });
        if damage > 0 {
            c["minecraft:max_damage"] = damage.into();
        }
        if let Some(key) = key {
            c["minecraft:item_name"] = serde_json::json!({ "translate": key });
        }
        c
    }

    /// A report folder: per-item files (26.x) or one items.json; enchantments listed or not.
    fn report_files(single: bool, enchantments: &[&str]) -> BTreeMap<String, Vec<u8>> {
        let mut files = BTreeMap::new();
        let mut registries = serde_json::json!({
            "minecraft:item": { "default": "minecraft:air", "protocol_id": 7, "entries": {} }
        });
        let mut all = serde_json::Map::new();
        // Protocol ids reversed against the insertion order: the report's order is the ids'.
        for (n, (id, stack, damage, key)) in ITEMS.iter().enumerate() {
            registries["minecraft:item"]["entries"][*id] = serde_json::json!({ "protocol_id": n });
            let map = components(*stack, *damage, *key);
            let entry = serde_json::json!({ "components": map });
            if single {
                // The older layouts: components as a type/value list, names as JSON text.
                let list: Vec<Value> = map
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(kind, value)| {
                        let value = if kind == "minecraft:item_name" {
                            Value::String(value.to_string())
                        } else {
                            value.clone()
                        };
                        serde_json::json!({ "type": kind, "value": value })
                    })
                    .collect();
                all.insert(id.to_string(), serde_json::json!({ "components": list }));
            } else {
                let (_, path) = split_id(id);
                files.insert(
                    format!("minecraft/components/item/{path}.json"),
                    json_bytes(entry),
                );
            }
        }
        if !enchantments.is_empty() {
            let mut entries = serde_json::Map::new();
            for (n, id) in enchantments.iter().enumerate() {
                entries.insert(id.to_string(), serde_json::json!({ "protocol_id": n }));
            }
            registries["minecraft:enchantment"] = serde_json::json!({ "entries": entries });
        }
        if single {
            files.insert("items.json".into(), json_bytes(Value::Object(all)));
        }
        files.insert("registries.json".into(), json_bytes(registries));
        files
    }

    fn report(single: bool, enchantments: &[&str]) -> Report {
        let files = report_files(single, enchantments);
        Report::read(&mut |path| files.get(path).cloned()).unwrap()
    }

    fn enchantment(
        source: &mut MemSource,
        name: &str,
        max: u32,
        items: &str,
        exclusive: Option<&str>,
    ) {
        let mut def = serde_json::json!({
            "description": { "translate": format!("enchantment.minecraft.{name}") },
            "max_level": max,
            "supported_items": items,
            "weight": 1
        });
        if let Some(set) = exclusive {
            def["exclusive_set"] = set.into();
        }
        source.insert(
            format!("data/minecraft/enchantment/{name}.json"),
            json_bytes(def),
        );
    }

    fn tag(source: &mut MemSource, dir: &str, path: &str, values: Value) {
        source.insert(
            format!("data/minecraft/tags/{dir}/{path}.json"),
            json_bytes(serde_json::json!({ "values": values })),
        );
    }

    /// The item tags enchantments refer to (in `dir`: `item`, or `items` before 1.21).
    fn item_tags(source: &mut MemSource, dir: &str) {
        tag(
            source,
            dir,
            "swords",
            serde_json::json!(["minecraft:diamond_sword"]),
        );
        tag(
            source,
            dir,
            "axes",
            serde_json::json!(["minecraft:iron_axe"]),
        );
        tag(
            source,
            dir,
            "enchantable/sharp_weapon",
            serde_json::json!(["#minecraft:swords", "#minecraft:axes"]),
        );
        tag(
            source,
            dir,
            "enchantable/durability",
            serde_json::json!([
                "#minecraft:enchantable/sharp_weapon",
                "minecraft:diamond_helmet",
                "minecraft:elytra",
                { "id": "#minecraft:not_in_this_version", "required": false }
            ]),
        );
        tag(
            source,
            dir,
            "enchantable/armor",
            serde_json::json!(["minecraft:diamond_helmet"]),
        );
    }

    /// A jar with 1.21-style data: English, five enchantments, their tags.
    fn jar() -> MemSource {
        let mut s = MemSource::default();
        s.insert(
            "assets/minecraft/lang/en_us.json",
            json_bytes(serde_json::to_value(map(EN)).unwrap()),
        );
        enchantment(
            &mut s,
            "sharpness",
            5,
            "#minecraft:enchantable/sharp_weapon",
            Some("#minecraft:exclusive_set/damage"),
        );
        enchantment(
            &mut s,
            "smite",
            5,
            "#minecraft:enchantable/sharp_weapon",
            Some("#minecraft:exclusive_set/damage"),
        );
        enchantment(
            &mut s,
            "mending",
            1,
            "#minecraft:enchantable/durability",
            None,
        );
        enchantment(
            &mut s,
            "binding_curse",
            1,
            "#minecraft:enchantable/armor",
            None,
        );
        // Only protection names the pair (as one id, not a tag): the data excludes both ways.
        enchantment(
            &mut s,
            "protection",
            4,
            "#minecraft:enchantable/armor",
            Some("minecraft:mending"),
        );
        item_tags(&mut s, "item");
        tag(
            &mut s,
            "enchantment",
            "exclusive_set/damage",
            serde_json::json!(["minecraft:sharpness", "minecraft:smite"]),
        );
        tag(
            &mut s,
            "enchantment",
            "curse",
            serde_json::json!(["minecraft:binding_curse"]),
        );
        s
    }

    fn aliases() -> Aliases {
        Aliases::parse(
            br#"{"format":1,
              "words":{"elmas":["dia"],"diamond":["dia"],"migfer":["kask"]},
              "items":{"minecraft:elytra":["elitra","kanat"],"minecraft:obsidian":["obsi","obsidyen"],
                       "minecraft:copper_golem_statue":["golem"]},
              "ench":{"minecraft:sharpness":["sharp","keskin"],"minecraft:fortune":["fort"]}}"#,
        )
        .unwrap()
    }

    fn build_with(
        jar: &mut MemSource,
        donor: Option<&mut MemSource>,
        report: &Report,
    ) -> DataBuild {
        let tr = map(TR);
        let aliases = aliases();
        build_item_data(DataInputs {
            minecraft: "26.3",
            jar,
            donor: donor.map(|d| d as &mut dyn AssetSource),
            report,
            tr: &tr,
            aliases: &aliases,
            sources: BTreeMap::from([("server".to_string(), "ab".repeat(20))]),
        })
        .unwrap()
    }

    fn built() -> DataBuild {
        build_with(&mut jar(), None, &report(false, &[]))
    }

    #[test]
    fn folds_like_the_interface() {
        for (text, folded) in [
            ("Elmas Kılıç", "elmas kilic"),
            ("IŞIK", "isik"),
            ("İnci", "inci"),
            ("Ölümsüzlük Totemi", "olumsuzluk totemi"),
            ("Rüzgâr Patlaması", "ruzgar patlamasi"),
            ("ÇĞİÖŞÜ çğıöşü", "cgiosu cgiosu"),
            ("Iron Ingot", "iron ingot"),
            ("Café crème", "cafe creme"),
            ("s\u{0327}apka", "sapka"),
            ("Jack o\u{2019}Lantern", "jack o\u{2019}lantern"),
            ("æøł", "æøł"),
        ] {
            assert_eq!(fold(text), folded, "{text}");
        }
    }

    #[test]
    fn builds_items_in_registry_order_with_names_and_numbers() {
        let data = built().data;
        let ids: Vec<&str> = data.items.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids[0], "minecraft:stone", "air is left out");
        assert_eq!(ids.len(), ITEMS.len() - 1);
        let sword = data.item("minecraft:diamond_sword").unwrap();
        assert_eq!((sword.stack, sword.max_damage), (1, 1561));
        assert_eq!(
            sword.name,
            Names {
                tr: "Elmas Kılıç".into(),
                en: "Diamond Sword".into()
            }
        );
        assert!(sword.damageable());
        let stone = data.item("minecraft:stone").unwrap();
        assert_eq!(stone.name.tr, "Taş", "no item_name: the block's key");
        let elytra = data.item("minecraft:elytra").unwrap();
        assert_eq!(
            elytra.name.tr, "Elytra",
            "a name missing in Turkish falls back to English"
        );
        assert_eq!(elytra.aliases, vec!["elitra", "kanat"]);
    }

    #[test]
    fn describes_names_the_game_repeats() {
        let data = built().data;
        let desc = |id: &str| data.item(id).unwrap().desc.clone();
        assert_eq!(desc("minecraft:music_disc_cat").unwrap().en, "C418 - cat");
        assert_eq!(
            desc("minecraft:sentry_armor_trim_smithing_template").unwrap(),
            Names {
                tr: "Nöbetçi Zırh Süslemesi".into(),
                en: "Sentry Armor Trim".into()
            }
        );
        assert_eq!(
            desc("minecraft:netherite_upgrade_smithing_template")
                .unwrap()
                .tr,
            "Netherit Yükseltmesi"
        );
        assert_eq!(desc("minecraft:diamond"), None);
        let notes = built().notes.join("\n");
        assert!(
            notes.contains("tr names shared: Fener (minecraft:beacon_like, minecraft:lantern)"),
            "{notes}"
        );
        assert!(
            !notes.contains("Music Disc ("),
            "the description lines tell the discs apart: {notes}"
        );
    }

    #[test]
    fn groups_the_enchantments_that_fit() {
        let data = built().data;
        let names = |id: &str| {
            let item = data.item(id).unwrap();
            data.enchantments_for(item)
                .map(|e| e.id.as_str())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            names("minecraft:diamond_sword"),
            vec![
                "minecraft:mending",
                "minecraft:sharpness",
                "minecraft:smite"
            ]
        );
        assert_eq!(
            names("minecraft:iron_axe"),
            names("minecraft:diamond_sword")
        );
        assert_eq!(
            data.item("minecraft:iron_axe").unwrap().group,
            data.item("minecraft:diamond_sword").unwrap().group,
            "one group for the same set"
        );
        assert_eq!(
            names("minecraft:diamond_helmet"),
            vec![
                "minecraft:binding_curse",
                "minecraft:mending",
                "minecraft:protection"
            ]
        );
        assert_eq!(names("minecraft:elytra"), vec!["minecraft:mending"]);
        assert_eq!(
            names("minecraft:enchanted_book").len(),
            5,
            "any enchantment on a book"
        );
        assert!(names("minecraft:diamond").is_empty());
        assert!(data.fits("minecraft:diamond_sword", "minecraft:sharpness"));
        assert!(!data.fits("minecraft:elytra", "minecraft:sharpness"));
        let sharpness = data.enchantment("minecraft:sharpness").unwrap();
        assert_eq!(sharpness.max_level, 5);
        assert_eq!(sharpness.name.tr, "Keskinlik");
        let smite = data
            .enchantments
            .iter()
            .position(|e| e.id == "minecraft:smite")
            .unwrap();
        assert_eq!(sharpness.excludes, vec![smite]);
        assert!(data.enchantment("minecraft:binding_curse").unwrap().curse);
        assert!(!sharpness.curse);
        assert_eq!(sharpness.aliases, vec!["sharp", "keskin"]);
        let at = |id: &str| data.enchantments.iter().position(|e| e.id == id).unwrap();
        assert_eq!(
            data.enchantment("minecraft:protection").unwrap().excludes,
            vec![at("minecraft:mending")]
        );
        assert_eq!(
            data.enchantment("minecraft:mending").unwrap().excludes,
            vec![at("minecraft:protection")]
        );
    }

    #[test]
    fn swaps_words_into_aliases_only_one_item_has() {
        let data = built().data;
        let aliases = |id: &str| data.item(id).unwrap().aliases.clone();
        assert_eq!(aliases("minecraft:diamond"), vec!["dia"]);
        assert_eq!(
            aliases("minecraft:diamond_sword"),
            vec!["dia kilic", "dia sword"]
        );
        assert_eq!(
            aliases("minecraft:diamond_helmet"),
            vec!["dia helmet", "dia kask", "dia migfer", "elmas kask"]
        );
        assert_eq!(
            aliases("minecraft:obsidian"),
            vec!["obsi"],
            "an alias equal to the name is left out"
        );
        let notes = built().notes.join("\n");
        assert!(
            notes.contains("aliases for items not in this version: minecraft:copper_golem_statue"),
            "{notes}"
        );
        assert!(
            notes.contains("aliases for enchantments not in this version: minecraft:fortune"),
            "{notes}"
        );
        let template = data
            .item("minecraft:sentry_armor_trim_smithing_template")
            .unwrap();
        assert_eq!(
            template.terms(),
            vec![
                "demirci sablonu",
                "smithing template",
                "nobetci zirh suslemesi",
                "sentry armor trim"
            ]
        );
        let sharpness = data.enchantment("minecraft:sharpness").unwrap();
        assert_eq!(
            sharpness.terms(),
            vec!["keskinlik", "sharpness", "sharp", "keskin"]
        );
        assert!(!data.compatible("minecraft:sharpness", "minecraft:smite"));
        assert!(
            !data.compatible("minecraft:mending", "minecraft:protection"),
            "excluded one way, refused both"
        );
        assert!(data.compatible("minecraft:sharpness", "minecraft:mending"));
        assert!(!data.compatible("minecraft:sharpness", "minecraft:sharpness"));
        assert!(
            !data.compatible("minecraft:sharpness", "minecraft:fortune"),
            "unknown"
        );
    }

    #[test]
    fn refuses_an_alias_that_is_another_items_name() {
        let tr = map(TR);
        let bad = Aliases::parse(
            br#"{"format":1,"items":{"minecraft:beacon_like":["fener","lantern"]}}"#,
        )
        .unwrap();
        let result = build_item_data(DataInputs {
            minecraft: "26.3",
            jar: &mut jar(),
            donor: None,
            report: &report(false, &[]),
            tr: &tr,
            aliases: &bad,
            sources: BTreeMap::new(),
        });
        let error = result.err().unwrap().to_string();
        assert!(
            error.contains(
                "alias lantern of minecraft:beacon_like is the name of minecraft:lantern"
            ),
            "{error}"
        );
    }

    #[test]
    fn writes_the_same_bytes_and_reads_them_back() {
        let a = built().data.to_json();
        let b = build_with(&mut jar(), None, &report(true, &[]))
            .data
            .to_json();
        assert_eq!(a, b, "both report layouts, same file");
        let text = String::from_utf8(a.clone()).unwrap();
        assert!(
            text.starts_with("{\"format\":1,\"mc\":\"26.3\",\"src\":{\"server\":\""),
            "{text}"
        );
        assert!(text.contains("\n[\"minecraft:diamond_sword\",1,1561,\"Elmas Kılıç\",\"Diamond Sword\",0,{\"a\":[\"dia kilic\",\"dia sword\"]}],\n"), "{text}");
        assert!(text.contains("\n[\"minecraft:music_disc_cat\",1,0,\"Müzik Diski\",\"Music Disc\",-1,{\"d\":[\"C418 - cat\",\"C418 - cat\"]}],\n"), "{text}");
        assert!(
            text.contains(
                "[\"minecraft:binding_curse\",1,\"Bağlanma Laneti\",\"Curse of Binding\",{\"c\":1}]"
            ),
            "{text}"
        );
        let read = ItemData::parse(&a).unwrap();
        assert_eq!(read, built().data);
        assert_eq!(read.to_json(), a);
    }

    #[test]
    fn takes_enchantments_from_a_donor_before_1_21() {
        // A 1.20.5 jar: English and `tags/items/`, no enchantment data; the registry lists four.
        let mut old = MemSource::default();
        old.insert(
            "assets/minecraft/lang/en_us.json",
            json_bytes(serde_json::to_value(map(EN)).unwrap()),
        );
        item_tags(&mut old, "items");
        let mut donor = jar();
        let listed = [
            "minecraft:sharpness",
            "minecraft:smite",
            "minecraft:mending",
            "minecraft:binding_curse",
        ];
        let build = build_with(&mut old, Some(&mut donor), &report(false, &listed));
        let ids: Vec<&str> = build
            .data
            .enchantments
            .iter()
            .map(|e| e.id.as_str())
            .collect();
        assert_eq!(
            ids,
            vec![
                "minecraft:binding_curse",
                "minecraft:mending",
                "minecraft:sharpness",
                "minecraft:smite"
            ]
        );
        assert!(
            build
                .notes
                .iter()
                .any(|n| n == "4 enchantments defined by the donor jar")
        );
        let data = build.data;
        assert!(
            data.fits("minecraft:iron_axe", "minecraft:smite"),
            "the old jar's own item tags"
        );
        assert!(
            data.enchantment("minecraft:binding_curse").unwrap().curse,
            "the donor's curse tag"
        );
        // Without a donor the build says what is missing.
        let tr = map(TR);
        let none = Aliases::default();
        let mut old = MemSource::default();
        old.insert(
            "assets/minecraft/lang/en_us.json",
            json_bytes(serde_json::to_value(map(EN)).unwrap()),
        );
        let error = build_item_data(DataInputs {
            minecraft: "1.20.5",
            jar: &mut old,
            donor: None,
            report: &report(false, &listed),
            tr: &tr,
            aliases: &none,
            sources: BTreeMap::new(),
        })
        .err()
        .unwrap()
        .to_string();
        assert!(error.contains("no enchantment data"), "{error}");
    }

    #[test]
    fn parse_refuses_broken_or_newer_files() {
        let good = String::from_utf8(built().data.to_json()).unwrap();
        assert!(matches!(
            ItemData::parse(good.replacen("\"format\":1", "\"format\":2", 1).as_bytes()),
            Err(DataError::Newer(2))
        ));
        for (from, to) in [
            (",0,{\"a\":[\"dia kilic\"", ",99,{\"a\":[\"dia kilic\""),
            ("\"dia kilic\",\"dia sword\"", "\"dia kilic\",\"dia kilic\""),
            ("\"dia kilic\"", "\"Dia Kılıç\""),
            ("\"minecraft:iron_axe\"", "\"minecraft:diamond_sword\""),
            ("[\"minecraft:stone\",64,", "[\"minecraft:stone\",0,"),
            ("{\"c\":1}", "{\"c\":1,\"x\":[0]}"),
        ] {
            assert!(good.contains(from), "{from}");
            let broken = good.replacen(from, to, 1);
            assert!(ItemData::parse(broken.as_bytes()).is_err(), "{to}");
        }
        // Unknown extras are a newer writer's: read past them.
        let extended = good.replacen("{\"c\":1}", "{\"c\":1,\"z\":true}", 1);
        assert!(ItemData::parse(extended.as_bytes()).is_ok());
    }

    #[test]
    fn manifest_picks_the_nearest_version() {
        let entry = |v: &str| DataFile {
            file: format!("{v}/items.0123456789ab.json"),
            sha256: format!("0123456789ab{}", "0".repeat(52)),
            bytes: 1,
        };
        let manifest = DataManifest {
            default: "26.3".into(),
            versions: ["1.20.5", "1.21.4", "1.21.10", "26.1.2", "26.3"]
                .into_iter()
                .map(|v| (v.to_string(), entry(v)))
                .collect(),
        };
        assert_eq!(manifest.pick(Some("26.1.2")), Some("26.1.2"));
        assert_eq!(
            manifest.pick(Some("26.2")),
            Some("26.1.2"),
            "the newest older file"
        );
        assert_eq!(
            manifest.pick(Some("1.21.11")),
            Some("1.21.10"),
            "1.21.10 is after 1.21.9"
        );
        assert_eq!(manifest.pick(Some("26.4-snapshot-5")), Some("26.3"));
        assert_eq!(
            manifest.pick(Some("1.8.9")),
            Some("1.20.5"),
            "older than all: the nearest newer"
        );
        assert_eq!(manifest.pick(None), Some("26.3"));
        let read = DataManifest::parse(&manifest.to_json()).unwrap();
        assert_eq!(read, manifest);
        let text = String::from_utf8(manifest.to_json()).unwrap();
        assert!(
            text.find("\"1.21.4\"").unwrap() < text.find("\"1.21.10\"").unwrap(),
            "{text}"
        );
        assert!(
            DataManifest::parse(text.replace("items.0123", "items.9999").as_bytes()).is_err(),
            "name and hash disagree"
        );
        assert!(DataManifest::parse(text.replace("26.3/items", "../items").as_bytes()).is_err());
        assert!(matches!(
            DataManifest::parse(text.replace("\"format\": 1", "\"format\": 2").as_bytes()),
            Err(DataError::Newer(2))
        ));
    }

    #[test]
    fn our_alias_file_is_sound() {
        let aliases = Aliases::parse(include_bytes!("../data/aliases.json")).unwrap();
        assert!(aliases.items.len() >= 40, "{}", aliases.items.len());
        assert!(
            aliases.enchantments.len() >= 20,
            "{}",
            aliases.enchantments.len()
        );
        for (id, list) in aliases.items.iter().chain(&aliases.enchantments) {
            assert!(!list.is_empty(), "{id}");
        }
        assert!(
            Aliases::parse(br#"{"format":1,"items":{"minecraft:elytra":["kanat"],"minecraft:feather":["kanat"]}}"#).is_err(),
            "one alias, two items"
        );
        assert!(
            Aliases::parse(br#"{"format":1,"items":{"minecraft:elytra":["Elitra"]}}"#).is_err(),
            "not folded"
        );
        assert!(
            Aliases::parse(br#"{"format":1,"items":{"elytra":["elitra"]}}"#).is_err(),
            "not a full id"
        );
        assert!(Aliases::parse(br#"{"format":1,"words":{"elmas":["elmas"]}}"#).is_err());
    }
}

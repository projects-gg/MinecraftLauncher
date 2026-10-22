//! The Projects chat stickers this launcher brings (CHAT2 §8): the sets of `packages/stickers`
//! (`pl1`), read from their manifests. A message carries only a sticker's reference (`pl1/gg`); the
//! pictures come with the launcher (the UI draws them), the names are here for whatever shows a
//! sticker as words (the in-game overlay's older mods, "[Çıkartma: GG]").

use std::sync::OnceLock;

use serde::Deserialize;

/// The manifests of the sets this launcher has, in the order they came.
const MANIFESTS: [&str; 1] = [include_str!("../../../packages/stickers/pl1/manifest.json")];

#[derive(Deserialize)]
struct Manifest {
    set: String,
    stickers: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    #[serde(rename = "ref")]
    reference: String,
    name: Names,
}

#[derive(Deserialize)]
struct Names {
    tr: String,
    en: String,
}

/// One sticker: its reference and names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sticker {
    /// `pl1/gg`.
    pub reference: String,
    pub name_tr: String,
    pub name_en: String,
}

struct Sets {
    sets: Vec<String>,
    stickers: Vec<Sticker>,
}

fn sets() -> &'static Sets {
    static SETS: OnceLock<Sets> = OnceLock::new();
    SETS.get_or_init(|| {
        let mut sets = Vec::new();
        let mut stickers = Vec::new();
        for text in MANIFESTS {
            // The manifests are tested to parse (below); one that does not is simply absent.
            let Ok(manifest) = serde_json::from_str::<Manifest>(text) else {
                continue;
            };
            stickers.extend(manifest.stickers.into_iter().map(|entry| Sticker {
                reference: entry.reference,
                name_tr: entry.name.tr,
                name_en: entry.name.en,
            }));
            sets.push(manifest.set);
        }
        Sets { sets, stickers }
    })
}

/// The sets this launcher has (`pl1`).
pub fn sticker_sets() -> &'static [String] {
    &sets().sets
}

/// A sticker this launcher has, by its reference.
pub fn sticker(reference: &str) -> Option<&'static Sticker> {
    sets().stickers.iter().find(|s| s.reference == reference)
}

/// A sticker as words, for whatever cannot draw it ("[Çıkartma: GG]" / "[Sticker: GG]"; one of a set
/// this launcher does not have: "[Çıkartma]").
pub fn sticker_text(reference: &str, english: bool) -> String {
    let word = if english { "Sticker" } else { "Çıkartma" };
    match sticker(reference) {
        Some(found) => {
            let name = if english {
                &found.name_en
            } else {
                &found.name_tr
            };
            format!("[{word}: {name}]")
        }
        None => format!("[{word}]"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_set_comes_with_its_names() {
        assert_eq!(sticker_sets(), ["pl1"]);
        assert_eq!(sets().stickers.len(), 32);
        let gg = sticker("pl1/gg").unwrap();
        assert_eq!((gg.name_tr.as_str(), gg.name_en.as_str()), ("GG", "GG"));
        assert_eq!(sticker_text("pl1/lol", false), "[Çıkartma: Kahkaha]");
        assert_eq!(sticker_text("pl1/lol", true), "[Sticker: LOL]");
        assert_eq!(sticker_text("pl9/new", false), "[Çıkartma]");
        assert!(sticker("pl1/../x").is_none());
    }
}

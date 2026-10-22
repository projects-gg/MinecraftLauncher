//! Link cards kept on this device (CHAT2 §9.3). The friends service's hub makes a link's card (this
//! computer never fetches anything from a linked site); the launcher keeps what it said with the
//! chat history, one small file per link under `<history>/cards/`, at most [`CARDS_MAX_BYTES`]
//! together (the oldest go first), so a message's card is asked for once. What could not be asked
//! (the hub busy, the day's cards used up, no connection) is kept for this run only, as nothing to
//! show. Nothing here talks to the service.

use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::model::{LinkCard, LinkRule};

/// The cards on disk together, at most.
pub const CARDS_MAX_BYTES: u64 = 20 * 1024 * 1024;
/// Cards kept read in memory.
const LOADED_MAX: usize = 48;
/// The longest picture taken (a `data:` URI of a 64 KB picture is about 87,400 characters).
const IMAGE_MAX: usize = 90_000;
const FORMAT: u32 = 1;

/// What the hub said about a link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum StoredCard {
    /// Its card.
    Ready { card: Box<LinkCard> },
    /// Nothing to show: the plain card (an icon and the domain).
    Plain,
    /// The link leads somewhere the rules refuse (a redirect to a shortener, say): plain text.
    Refused { rule: Option<LinkRule> },
}

#[derive(Serialize, Deserialize)]
struct CardFile {
    v: u32,
    url: String,
    #[serde(flatten)]
    card: StoredCard,
}

/// A card as it may be shown: texts as long as the hub sends them at most, a picture only as a
/// `data:` URI of the three kinds the hub sends.
fn tidy(mut card: LinkCard) -> LinkCard {
    let cut = |text: Option<String>, max: usize| {
        text.map(|text| text.trim().chars().take(max).collect::<String>())
            .filter(|text| !text.is_empty())
    };
    card.title = cut(card.title, 120);
    card.author = cut(card.author, 60);
    card.desc = cut(card.desc, 200);
    card.site = card.site.trim().chars().take(60).collect();
    card.image = card.image.filter(|image| {
        image.len() <= IMAGE_MAX
            && ["jpeg", "png", "webp"]
                .iter()
                .any(|kind| image.starts_with(&format!("data:image/{kind};base64,")))
    });
    if card.image.is_none() {
        card.w = None;
        card.h = None;
    }
    card
}

/// A link's file name: a digest of the link as written.
fn key_of(url: &str) -> String {
    Sha256::digest(url.as_bytes())
        .iter()
        .take(16)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The link cards of one account on this device.
#[derive(Debug)]
pub struct CardStore {
    /// None: kept in memory only.
    dir: Option<PathBuf>,
    /// The files on disk (key → bytes, order of writing), read when first needed.
    index: Option<HashMap<String, (u64, u64)>>,
    total: u64,
    next: u64,
    /// Cards read or made lately, newest last.
    loaded: VecDeque<(String, StoredCard)>,
    /// What could not be asked in this run (by key): nothing to show, not kept.
    passing: HashMap<String, ()>,
    /// The files together, at most (bytes).
    max: u64,
}

impl CardStore {
    /// The cards under `dir` (None: in memory only).
    pub fn open(dir: Option<PathBuf>) -> Self {
        Self::with_limit(dir, CARDS_MAX_BYTES)
    }

    fn with_limit(dir: Option<PathBuf>, max: u64) -> Self {
        Self {
            dir,
            index: None,
            total: 0,
            next: 0,
            loaded: VecDeque::new(),
            passing: HashMap::new(),
            max,
        }
    }

    /// What is known about a link: kept, or found out in this run.
    pub fn get(&mut self, url: &str) -> Option<StoredCard> {
        let key = key_of(url);
        if let Some(at) = self.loaded.iter().position(|(k, _)| *k == key) {
            let entry = self.loaded.remove(at)?;
            let card = entry.1.clone();
            self.loaded.push_back(entry);
            return Some(card);
        }
        if self.passing.contains_key(&key) {
            return Some(StoredCard::Plain);
        }
        let card = self.read(&key, url)?;
        self.remember(key, card.clone());
        Some(card)
    }

    /// Keeps what the hub said about a link (on disk, the oldest going past the limit).
    pub fn put(&mut self, url: &str, card: StoredCard) {
        let key = key_of(url);
        let card = match card {
            StoredCard::Ready { card } => StoredCard::Ready {
                card: Box::new(tidy(*card)),
            },
            other => other,
        };
        self.passing.remove(&key);
        self.write(&key, url, &card);
        self.remember(key, card);
    }

    /// Nothing to show for a link in this run (it could not be asked); never kept.
    pub fn put_passing(&mut self, url: &str) {
        if self.passing.len() > 1_000 {
            self.passing.clear();
        }
        self.passing.insert(key_of(url), ());
    }

    /// Forgets every card ("Verilerimi sil"; the history's folder goes with them).
    pub fn erase(&mut self) {
        self.index = Some(HashMap::new());
        self.total = 0;
        self.loaded.clear();
        self.passing.clear();
        if let Some(dir) = &self.dir {
            let _ = fs::remove_dir_all(dir);
        }
    }

    fn remember(&mut self, key: String, card: StoredCard) {
        self.loaded.retain(|(k, _)| *k != key);
        self.loaded.push_back((key, card));
        while self.loaded.len() > LOADED_MAX {
            self.loaded.pop_front();
        }
    }

    fn path(&self, key: &str) -> Option<PathBuf> {
        Some(self.dir.as_ref()?.join(format!("{key}.json")))
    }

    fn read(&mut self, key: &str, url: &str) -> Option<StoredCard> {
        self.load_index();
        if !self.index.as_ref()?.contains_key(key) {
            return None;
        }
        let bytes = fs::read(self.path(key)?).ok()?;
        let file: CardFile = serde_json::from_slice(&bytes).ok()?;
        (file.v == FORMAT && file.url == url).then_some(file.card)
    }

    fn write(&mut self, key: &str, url: &str, card: &StoredCard) {
        let Some(path) = self.path(key) else {
            return;
        };
        let file = CardFile {
            v: FORMAT,
            url: url.to_owned(),
            card: card.clone(),
        };
        let Ok(bytes) = serde_json::to_vec(&file) else {
            return;
        };
        self.load_index();
        let written = self
            .dir
            .as_deref()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| replace(&path, &bytes));
        if let Err(error) = written {
            tracing::info!(%error, "a link card is kept for this run only");
            return;
        }
        let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        self.next += 1;
        let order = self.next;
        if let Some(index) = self.index.as_mut()
            && let Some((before, _)) = index.insert(key.to_owned(), (size, order))
        {
            self.total = self.total.saturating_sub(before);
        }
        self.total = self.total.saturating_add(size);
        self.trim();
    }

    /// The files on disk, oldest first by when they were written.
    fn load_index(&mut self) {
        if self.index.is_some() {
            return;
        }
        let mut files: Vec<(String, u64, std::time::SystemTime)> = Vec::new();
        if let Some(dir) = &self.dir
            && let Ok(entries) = fs::read_dir(dir)
        {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                let Some(key) = name.strip_suffix(".json") else {
                    continue;
                };
                if let Ok(meta) = entry.metadata() {
                    let written = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
                    files.push((key.to_owned(), meta.len(), written));
                }
            }
        }
        files.sort_by_key(|(_, _, written)| *written);
        let mut index = HashMap::new();
        for (key, size, _) in files {
            self.next += 1;
            self.total = self.total.saturating_add(size);
            index.insert(key, (size, self.next));
        }
        self.index = Some(index);
        self.trim();
    }

    /// The oldest files go while the cards take more than the limit.
    fn trim(&mut self) {
        while self.total > self.max {
            let Some(index) = self.index.as_mut() else {
                return;
            };
            let Some(oldest) = index
                .iter()
                .min_by_key(|(_, (_, order))| *order)
                .map(|(key, _)| key.clone())
            else {
                return;
            };
            if let Some((size, _)) = index.remove(&oldest) {
                self.total = self.total.saturating_sub(size);
            }
            self.loaded.retain(|(key, _)| *key != oldest);
            if let Some(path) = self.path(&oldest) {
                let _ = fs::remove_file(path);
            }
        }
    }
}

/// Writes through a temporary file and a rename, so a crash never leaves half a file.
fn replace(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, bytes)?;
    fs::rename(&temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(url: &str, title: &str) -> LinkCard {
        LinkCard {
            url: url.to_owned(),
            site: "YouTube".into(),
            icon: "youtube".into(),
            kind: "video".into(),
            title: Some(title.to_owned()),
            author: None,
            desc: None,
            image: Some("data:image/jpeg;base64,AAAA".into()),
            w: Some(320),
            h: Some(180),
        }
    }

    #[test]
    fn a_card_is_kept_once_asked_and_survives_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let url = "https://youtu.be/dQw4w9WgXcQ";
        {
            let mut cards = CardStore::open(Some(dir.path().join("cards")));
            assert_eq!(cards.get(url), None);
            cards.put(
                url,
                StoredCard::Ready {
                    card: Box::new(card(url, "  Bir video ")),
                },
            );
            cards.put(
                "https://grabify.link/x",
                StoredCard::Refused {
                    rule: Some(LinkRule::Grabber),
                },
            );
            // Nothing to show this run, never kept.
            cards.put_passing("https://twitch.tv/x");
            assert_eq!(cards.get("https://twitch.tv/x"), Some(StoredCard::Plain));
        }
        let mut cards = CardStore::open(Some(dir.path().join("cards")));
        match cards.get(url) {
            Some(StoredCard::Ready { card }) => {
                assert_eq!(card.title.as_deref(), Some("Bir video"));
                assert_eq!(card.w, Some(320));
            }
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(
            cards.get("https://grabify.link/x"),
            Some(StoredCard::Refused {
                rule: Some(LinkRule::Grabber)
            })
        );
        assert_eq!(cards.get("https://twitch.tv/x"), None, "this run's only");
        cards.erase();
        assert_eq!(cards.get(url), None);
    }

    #[test]
    fn only_pictures_the_hub_sends_are_taken_and_texts_keep_their_length() {
        let mut cards = CardStore::open(None);
        let url = "https://example.com/";
        let odd = LinkCard {
            image: Some("javascript:alert(1)".into()),
            title: Some("x".repeat(500)),
            ..card(url, "")
        };
        cards.put(
            url,
            StoredCard::Ready {
                card: Box::new(odd),
            },
        );
        match cards.get(url) {
            Some(StoredCard::Ready { card }) => {
                assert_eq!(card.image, None);
                assert_eq!((card.w, card.h), (None, None));
                assert_eq!(card.title.map(|t| t.chars().count()), Some(120));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn the_oldest_cards_go_past_the_limit() {
        let dir = tempfile::tempdir().unwrap();
        let limit = 1_000_000;
        let mut cards = CardStore::with_limit(Some(dir.path().to_path_buf()), limit);
        let big = |n: usize| LinkCard {
            image: Some(format!("data:image/png;base64,{}", "A".repeat(80_000))),
            ..card(&format!("https://example.com/{n}"), "x")
        };
        // About 80 KB each: well past the limit after 30.
        for n in 0..30 {
            let url = format!("https://example.com/{n}");
            cards.put(
                &url,
                StoredCard::Ready {
                    card: Box::new(big(n)),
                },
            );
        }
        assert!(cards.total <= limit);
        let on_disk: u64 = fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|entry| entry.metadata().unwrap().len())
            .sum();
        assert!(on_disk <= limit, "{on_disk}");
        assert!(
            cards.get("https://example.com/29").is_some(),
            "the newest stays"
        );
        let mut fresh = CardStore::with_limit(Some(dir.path().to_path_buf()), limit);
        assert_eq!(fresh.get("https://example.com/0"), None, "the oldest went");
        assert!(fresh.get("https://example.com/29").is_some());
    }
}

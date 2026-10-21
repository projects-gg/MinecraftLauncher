//! The chat history on this device (CHAT1). The service keeps only a short mailbox (the last 200
//! messages of the last 7 days per conversation) for devices that were away; the history belongs
//! to the devices. Per account: one small index (`index.json`: what was read, what was cleared
//! here, the newest message seen, the messages still waiting to go) and one file per conversation,
//! one message per line, at most [`HISTORY_LIMIT`] messages. A few conversations stay in memory;
//! the others are read when they are needed.
//!
//! Every change is written at once (an append for a new message, a rewrite for an edit or a
//! delete, both through a temporary file and a rename for rewrites), so a crash loses nothing that
//! was shown. Nothing here talks to the service.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::model::ChatMessage;
use crate::view::SendFailure;

/// Messages kept per conversation on this device; the oldest go first.
pub const HISTORY_LIMIT: usize = 5_000;
/// Conversations kept in memory (the others stay on disk until they are needed).
const LOADED_MAX: usize = 8;
/// A sync starts this much before the newest message held (the service's advice: duplicates are
/// dropped by conversation and id).
pub const SYNC_OVERLAP_MS: i64 = 60_000;
const INDEX_FILE: &str = "index.json";

/// What a conversation id names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Conversation {
    /// `c:<team id>`: a crew's chat.
    Crew(String),
    /// `d:<id>:<id>`: two friends, the smaller id first.
    Direct(String, String),
}

fn is_user_id(id: &str) -> bool {
    (17..=20).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_digit())
}

fn is_team_id(id: &str) -> bool {
    id.len() == 16
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The smaller of two user ids comes first: a shorter id is smaller, ids of the same length
/// compare as text (the service's order).
fn id_order(a: &str, b: &str) -> std::cmp::Ordering {
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

impl Conversation {
    /// A conversation id as the service writes it, or None for anything else (which never
    /// becomes a file name).
    pub fn parse(conv: &str) -> Option<Self> {
        if let Some(team) = conv.strip_prefix("c:") {
            return is_team_id(team).then(|| Self::Crew(team.to_owned()));
        }
        let (a, b) = conv.strip_prefix("d:")?.split_once(':')?;
        (is_user_id(a) && is_user_id(b) && id_order(a, b).is_lt())
            .then(|| Self::Direct(a.to_owned(), b.to_owned()))
    }

    /// The other person of a direct conversation, seen by `me`.
    pub fn peer(&self, me: &str) -> Option<&str> {
        match self {
            Self::Direct(a, b) if a == me => Some(b),
            Self::Direct(a, b) if b == me => Some(a),
            _ => None,
        }
    }
}

/// The direct conversation of two players (`d:<smaller>:<larger>`).
pub fn direct_conversation(a: &str, b: &str) -> String {
    if id_order(a, b).is_le() {
        format!("d:{a}:{b}")
    } else {
        format!("d:{b}:{a}")
    }
}

/// A crew's conversation.
pub fn crew_conversation(team: &str) -> String {
    format!("c:{team}")
}

/// One message on disk (short keys: the file holds up to 5,000 of them).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Line {
    i: i64,
    f: String,
    t: String,
    m: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    e: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    n: Option<String>,
}

impl Line {
    fn of(message: &ChatMessage) -> Self {
        Self {
            i: message.id,
            f: message.from.clone(),
            t: message.text.clone(),
            m: message.mac.clone(),
            e: message.edited_at,
            n: message.nonce.clone().filter(|nonce| !nonce.is_empty()),
        }
    }

    fn message(&self, conv: &str) -> ChatMessage {
        ChatMessage {
            conv: conv.to_owned(),
            id: self.i,
            from: self.f.clone(),
            text: self.t.clone(),
            mac: self.m.clone(),
            edited_at: self.e,
            nonce: self.n.clone(),
        }
    }
}

/// The key a message keeps on screen from the moment it is typed: its nonce when it has one (the
/// player's own messages), else its id.
pub fn message_key(message: &ChatMessage) -> String {
    match message.nonce.as_deref().filter(|nonce| !nonce.is_empty()) {
        Some(nonce) => format!("n:{nonce}"),
        None => format!("i:{}", message.id),
    }
}

/// A message the player sent from this device that the service has not taken yet: on its way
/// (`failed` None), or turned away and kept for "Tekrar dene".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outgoing {
    pub conv: String,
    pub nonce: String,
    pub text: String,
    /// Unix milliseconds on this computer when the player sent it.
    pub at: i64,
    #[serde(default)]
    pub failed: Option<SendFailure>,
}

impl Outgoing {
    pub fn key(&self) -> String {
        format!("n:{}", self.nonce)
    }
}

/// One entry of a conversation as shown: a message the service took, or one on its way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    Sent(ChatMessage),
    Outgoing(Outgoing),
}

impl Entry {
    pub fn key(&self) -> String {
        match self {
            Self::Sent(message) => message_key(message),
            Self::Outgoing(outgoing) => outgoing.key(),
        }
    }
}

/// What a change did to one conversation, for the screen: entries to add or replace (by key),
/// keys to take away, or everything gone (`reset`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Change {
    pub conv: String,
    pub upsert: Vec<Entry>,
    pub remove: Vec<String>,
    pub reset: bool,
}

impl Change {
    fn new(conv: &str) -> Self {
        Self {
            conv: conv.to_owned(),
            ..Self::default()
        }
    }
}

/// The newest message of a conversation, for the list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Last {
    pub i: i64,
    pub f: String,
    pub t: String,
}

/// What the index keeps about one conversation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationMeta {
    /// Messages up to this id are read.
    #[serde(default)]
    pub read: i64,
    /// Messages up to this id were cleared here ("Geçmişi temizle"): they never come back.
    #[serde(default)]
    pub cleared: i64,
    /// Messages from others after `read`.
    #[serde(default)]
    pub unread: u32,
    #[serde(default)]
    pub last: Option<Last>,
    #[serde(default)]
    pub count: u32,
    /// Who or what it is with, as last known (a friend's or a crew's name), for when they are
    /// gone from the lists.
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Index {
    #[serde(default)]
    v: u32,
    /// The newest message id seen in any conversation: the next sync starts a minute before it.
    #[serde(default)]
    newest: i64,
    #[serde(default)]
    convs: BTreeMap<String, ConversationMeta>,
    #[serde(default)]
    outgoing: Vec<Outgoing>,
}

/// The chat history of one account on this device.
#[derive(Debug)]
pub struct History {
    /// None: kept in memory only (no data folder).
    dir: Option<PathBuf>,
    me: String,
    index: Index,
    loaded: HashMap<String, Vec<Line>>,
    recent: VecDeque<String>,
}

impl History {
    /// The history of `me` under `root` (`<root>/<me>/`), or in memory only without a root.
    pub fn open(root: Option<&Path>, me: &str) -> Self {
        let dir = root.filter(|_| is_user_id(me)).map(|root| root.join(me));
        let index = dir
            .as_ref()
            .and_then(|dir| fs::read(dir.join(INDEX_FILE)).ok())
            .and_then(|bytes| serde_json::from_slice::<Index>(&bytes).ok())
            .unwrap_or_default();
        Self {
            dir,
            me: me.to_owned(),
            index,
            loaded: HashMap::new(),
            recent: VecDeque::new(),
        }
    }

    pub fn me(&self) -> &str {
        &self.me
    }

    /// Where the next sync starts (ms on the hub's clock): the newest message seen minus a
    /// minute, 0 on a new device.
    pub fn sync_since(&self) -> i64 {
        (self.index.newest - SYNC_OVERLAP_MS).max(0)
    }

    /// Every conversation this device knows, with what the list shows.
    pub fn conversations(&self) -> impl Iterator<Item = (&str, &ConversationMeta)> {
        self.index
            .convs
            .iter()
            .map(|(conv, meta)| (conv.as_str(), meta))
    }

    #[cfg(test)]
    pub fn meta(&self, conv: &str) -> Option<&ConversationMeta> {
        self.index.convs.get(conv)
    }

    /// The messages waiting to go, oldest first.
    pub fn outgoing(&self) -> &[Outgoing] {
        &self.index.outgoing
    }

    /// Remembers who a conversation is with (shown when they are gone from the lists).
    pub fn set_title(&mut self, conv: &str, title: &str) {
        if Conversation::parse(conv).is_none() {
            return;
        }
        let meta = self.index.convs.entry(conv.to_owned()).or_default();
        if meta.title.as_deref() != Some(title) {
            meta.title = Some(title.to_owned());
            self.save_index();
        }
    }

    /// A message the service sent (live, in a sync, or as it stored the player's own): added, or
    /// put in place of the version held. One of the player's own that waited here is settled by
    /// its nonce. None when nothing changed (a duplicate, or a message cleared here).
    pub fn put(&mut self, message: &ChatMessage) -> Option<Change> {
        let conv = message.conv.clone();
        Conversation::parse(&conv)?;
        let mut change = Change::new(&conv);
        let mine = message.from == self.me;
        // The player's own message from this device: the one waiting goes, the same key stays.
        let settled = mine
            && message.nonce.as_deref().is_some_and(|nonce| {
                let before = self.index.outgoing.len();
                self.index
                    .outgoing
                    .retain(|o| !(o.conv == conv && o.nonce == nonce));
                self.index.outgoing.len() != before
            });
        let cleared = self.index.convs.get(&conv).map_or(0, |meta| meta.cleared);
        if message.id <= cleared {
            if settled {
                // Sent, then cleared before its answer came: it simply goes.
                change.remove.push(message_key(message));
                self.save_index();
                return Some(change);
            }
            return None;
        }
        self.index.newest = self.index.newest.max(message.id);
        let line = Line::of(message);
        let read = self.index.convs.get(&conv).map_or(0, |meta| meta.read);
        let lines = self.lines(&conv);
        let (inserted, appended) = match lines.binary_search_by_key(&line.i, |l| l.i) {
            Ok(at) => {
                if lines[at] == line {
                    if !settled {
                        return None;
                    }
                    (false, true)
                } else {
                    // A newer version (an edit), or the same message with its nonce now known.
                    let nonce = line.n.clone().or_else(|| lines[at].n.clone());
                    lines[at] = Line { n: nonce, ..line };
                    (false, false)
                }
            }
            Err(at) => {
                let appended = at == lines.len();
                lines.insert(at, line);
                (true, appended)
            }
        };
        let mut dropped = Vec::new();
        while lines.len() > HISTORY_LIMIT {
            dropped.push(lines.remove(0));
        }
        let stored = lines
            .binary_search_by_key(&message.id, |l| l.i)
            .ok()
            .map(|at| lines[at].message(&conv));
        let last = lines.last().map(|l| Last {
            i: l.i,
            f: l.f.clone(),
            t: l.t.clone(),
        });
        let count = u32::try_from(lines.len()).unwrap_or(u32::MAX);
        if let Some(stored) = stored {
            change.upsert.push(Entry::Sent(stored));
        }
        for line in &dropped {
            change.remove.push(message_key(&line.message(&conv)));
        }
        let meta = self.index.convs.entry(conv.clone()).or_default();
        if inserted && !mine && message.id > read {
            meta.unread = meta.unread.saturating_add(1);
        }
        for line in &dropped {
            if line.f != self.me && line.i > meta.read {
                meta.unread = meta.unread.saturating_sub(1);
            }
        }
        meta.last = last;
        meta.count = count;
        if inserted && appended && dropped.is_empty() {
            self.append_line(&conv, message);
        } else if inserted || !appended || !dropped.is_empty() {
            self.write_lines(&conv);
        }
        self.save_index();
        Some(change)
    }

    /// A message was deleted for everyone: it goes from the history. None when it was not held.
    pub fn remove(&mut self, conv: &str, id: i64) -> Option<Change> {
        Conversation::parse(conv)?;
        let lines = self.lines(conv);
        let at = lines.binary_search_by_key(&id, |l| l.i).ok()?;
        let line = lines.remove(at);
        let last = lines.last().map(|l| Last {
            i: l.i,
            f: l.f.clone(),
            t: l.t.clone(),
        });
        let count = u32::try_from(lines.len()).unwrap_or(u32::MAX);
        let mut change = Change::new(conv);
        change.remove.push(message_key(&line.message(conv)));
        let meta = self.index.convs.entry(conv.to_owned()).or_default();
        if line.f != self.me && line.i > meta.read {
            meta.unread = meta.unread.saturating_sub(1);
        }
        meta.last = last;
        meta.count = count;
        self.write_lines(conv);
        self.save_index();
        Some(change)
    }

    /// The player sent a message from here: it waits, shown at once, until the service takes it.
    pub fn add_outgoing(&mut self, conv: &str, text: &str, nonce: &str, at: i64) -> Option<Change> {
        Conversation::parse(conv)?;
        let outgoing = Outgoing {
            conv: conv.to_owned(),
            nonce: nonce.to_owned(),
            text: text.to_owned(),
            at,
            failed: None,
        };
        self.index.outgoing.push(outgoing.clone());
        self.index.convs.entry(conv.to_owned()).or_default();
        self.save_index();
        let mut change = Change::new(conv);
        change.upsert.push(Entry::Outgoing(outgoing));
        Some(change)
    }

    /// A message on its way did not go (`failure`), or goes again (None).
    pub fn set_outgoing(&mut self, nonce: &str, failure: Option<SendFailure>) -> Option<Change> {
        let outgoing = self
            .index
            .outgoing
            .iter_mut()
            .find(|outgoing| outgoing.nonce == nonce)?;
        if outgoing.failed == failure {
            return None;
        }
        outgoing.failed = failure;
        let mut change = Change::new(&outgoing.conv);
        change.upsert.push(Entry::Outgoing(outgoing.clone()));
        self.save_index();
        Some(change)
    }

    pub fn outgoing_of(&self, nonce: &str) -> Option<&Outgoing> {
        self.index.outgoing.iter().find(|o| o.nonce == nonce)
    }

    /// "Tekrar dene": the message counts as sent now (it moves to the end, with a fresh time).
    pub fn restart_outgoing(&mut self, nonce: &str, at: i64) {
        let Some(position) = self.index.outgoing.iter().position(|o| o.nonce == nonce) else {
            return;
        };
        let mut outgoing = self.index.outgoing.remove(position);
        outgoing.at = at;
        self.index.outgoing.push(outgoing);
        self.save_index();
    }

    /// A message that did not go is dropped ("Sil" on a failed message).
    pub fn discard_outgoing(&mut self, nonce: &str) -> Option<Change> {
        let at = self.index.outgoing.iter().position(|o| o.nonce == nonce)?;
        let outgoing = self.index.outgoing.remove(at);
        self.save_index();
        let mut change = Change::new(&outgoing.conv);
        change.remove.push(outgoing.key());
        Some(change)
    }

    /// The newest `limit` entries before `before` (an id; None: the newest of all, with the
    /// messages still on their way after them), oldest first, and whether older ones remain.
    pub fn page(&mut self, conv: &str, before: Option<i64>, limit: usize) -> (Vec<Entry>, bool) {
        if Conversation::parse(conv).is_none() {
            return (Vec::new(), false);
        }
        let lines = self.lines(conv);
        let end = match before {
            Some(id) => lines.partition_point(|l| l.i < id),
            None => lines.len(),
        };
        let start = end.saturating_sub(limit);
        let mut entries: Vec<Entry> = lines[start..end]
            .iter()
            .map(|line| Entry::Sent(line.message(conv)))
            .collect();
        if before.is_none() {
            entries.extend(
                self.index
                    .outgoing
                    .iter()
                    .filter(|o| o.conv == conv)
                    .cloned()
                    .map(Entry::Outgoing),
            );
        }
        (entries, start > 0)
    }

    /// One message as held (for an edit, a delete or a report).
    pub fn message(&mut self, conv: &str, id: i64) -> Option<ChatMessage> {
        Conversation::parse(conv)?;
        let lines = self.lines(conv);
        let at = lines.binary_search_by_key(&id, |l| l.i).ok()?;
        Some(lines[at].message(conv))
    }

    /// Up to `count` messages before `id`, oldest first (a report's context).
    pub fn before(&mut self, conv: &str, id: i64, count: usize) -> Vec<ChatMessage> {
        if Conversation::parse(conv).is_none() {
            return Vec::new();
        }
        let lines = self.lines(conv);
        let end = lines.partition_point(|l| l.i < id);
        lines[end.saturating_sub(count)..end]
            .iter()
            .map(|line| line.message(conv))
            .collect()
    }

    /// Everything in a conversation is read now. False when it already was.
    pub fn mark_read(&mut self, conv: &str) -> bool {
        let Some(meta) = self.index.convs.get_mut(conv) else {
            return false;
        };
        let newest = meta.last.as_ref().map_or(meta.read, |last| last.i);
        if meta.unread == 0 && meta.read >= newest {
            return false;
        }
        meta.read = meta.read.max(newest);
        meta.unread = 0;
        self.save_index();
        true
    }

    /// "Geçmişi temizle": the conversation's messages leave this device, and never come back
    /// (a sync may list them again; they are dropped). Messages still on their way stay.
    pub fn clear(&mut self, conv: &str) -> Option<Change> {
        Conversation::parse(conv)?;
        let newest = {
            let lines = self.lines(conv);
            let newest = lines.last().map_or(0, |l| l.i);
            lines.clear();
            newest
        };
        let meta = self.index.convs.entry(conv.to_owned()).or_default();
        meta.cleared = meta
            .cleared
            .max(newest)
            .max(meta.last.as_ref().map_or(0, |l| l.i));
        meta.read = meta.read.max(meta.cleared);
        meta.unread = 0;
        meta.last = None;
        meta.count = 0;
        if let Some(path) = self.conversation_path(conv) {
            let _ = fs::remove_file(path);
        }
        self.save_index();
        let mut change = Change::new(conv);
        change.reset = true;
        change.upsert = self
            .index
            .outgoing
            .iter()
            .filter(|o| o.conv == conv)
            .cloned()
            .map(Entry::Outgoing)
            .collect();
        Some(change)
    }

    // --- files ------------------------------------------------------------------------------

    fn conversation_path(&self, conv: &str) -> Option<PathBuf> {
        Conversation::parse(conv)?;
        Some(
            self.dir
                .as_ref()?
                .join(format!("{}.jsonl", conv.replace(':', "_"))),
        )
    }

    /// The conversation's lines, read from disk the first time (or after it left memory).
    fn lines(&mut self, conv: &str) -> &mut Vec<Line> {
        if let Some(at) = self.recent.iter().position(|c| c == conv) {
            self.recent.remove(at);
        }
        self.recent.push_back(conv.to_owned());
        if !self.loaded.contains_key(conv) {
            let lines = self.read_lines(conv);
            self.loaded.insert(conv.to_owned(), lines);
            while self.recent.len() > LOADED_MAX {
                if let Some(old) = self.recent.pop_front() {
                    self.loaded.remove(&old);
                }
            }
        }
        self.loaded.entry(conv.to_owned()).or_default()
    }

    fn read_lines(&self, conv: &str) -> Vec<Line> {
        let Some(path) = self.conversation_path(conv) else {
            return Vec::new();
        };
        let Ok(text) = fs::read_to_string(path) else {
            return Vec::new();
        };
        let mut lines: Vec<Line> = text
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        lines.sort_by_key(|l| l.i);
        // A later line of the same id is its newer version.
        let mut kept: Vec<Line> = Vec::with_capacity(lines.len());
        for line in lines {
            match kept.last_mut() {
                Some(last) if last.i == line.i => *last = line,
                _ => kept.push(line),
            }
        }
        let extra = kept.len().saturating_sub(HISTORY_LIMIT);
        kept.drain(..extra);
        kept
    }

    fn append_line(&self, conv: &str, message: &ChatMessage) {
        let Some(path) = self.conversation_path(conv) else {
            return;
        };
        let Ok(mut text) = serde_json::to_string(&Line::of(message)) else {
            return;
        };
        text.push('\n');
        let written = self.ensure_dir().and_then(|()| {
            fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .and_then(|mut file| file.write_all(text.as_bytes()))
        });
        if let Err(error) = written {
            tracing::info!(%error, "a chat message is kept for this run only");
        }
    }

    fn write_lines(&self, conv: &str) {
        let Some(path) = self.conversation_path(conv) else {
            return;
        };
        let Some(lines) = self.loaded.get(conv) else {
            return;
        };
        if lines.is_empty() {
            let _ = fs::remove_file(path);
            return;
        }
        let mut text = String::new();
        for line in lines {
            if let Ok(json) = serde_json::to_string(line) {
                text.push_str(&json);
                text.push('\n');
            }
        }
        if let Err(error) = self
            .ensure_dir()
            .and_then(|()| replace(&path, text.as_bytes()))
        {
            tracing::info!(%error, "the chat history could not be saved");
        }
    }

    fn save_index(&self) {
        let Some(dir) = &self.dir else {
            return;
        };
        let Ok(bytes) = serde_json::to_vec(&self.index) else {
            return;
        };
        if let Err(error) = self
            .ensure_dir()
            .and_then(|()| replace(&dir.join(INDEX_FILE), &bytes))
        {
            tracing::info!(%error, "the chat index could not be saved");
        }
    }

    fn ensure_dir(&self) -> std::io::Result<()> {
        match &self.dir {
            Some(dir) => fs::create_dir_all(dir),
            None => Ok(()),
        }
    }

    /// Forgets the whole history of this account on this device ("Verilerimi sil").
    pub fn erase(&mut self) {
        self.index = Index::default();
        self.loaded.clear();
        self.recent.clear();
        if let Some(dir) = &self.dir {
            let _ = fs::remove_dir_all(dir);
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

    const ME: &str = "100000000000000001";
    const AYSE: &str = "100000000000000002";
    const CREW: &str = "c:0123456789abcdef";

    fn dm() -> String {
        direct_conversation(ME, AYSE)
    }

    fn message(conv: &str, id: i64, from: &str, text: &str) -> ChatMessage {
        ChatMessage {
            conv: conv.to_owned(),
            id,
            from: from.to_owned(),
            text: text.to_owned(),
            mac: format!("mac{id}"),
            edited_at: None,
            nonce: None,
        }
    }

    #[test]
    fn conversation_ids_are_checked_before_they_name_a_file() {
        assert_eq!(
            Conversation::parse(CREW),
            Some(Conversation::Crew("0123456789abcdef".into()))
        );
        assert_eq!(direct_conversation(AYSE, ME), format!("d:{ME}:{AYSE}"));
        // A shorter id is the smaller one, whatever its digits.
        assert_eq!(
            direct_conversation("900000000000000000", "10000000000000000"),
            "d:10000000000000000:900000000000000000"
        );
        assert!(Conversation::parse(&dm()).is_some());
        for bad in [
            "c:../../etc",
            "c:0123456789ABCDEF",
            "d:1:2",
            &format!("d:{AYSE}:{ME}"),
            "x:1",
            "",
        ] {
            assert_eq!(Conversation::parse(bad), None, "{bad}");
        }
        assert_eq!(Conversation::parse(&dm()).unwrap().peer(ME), Some(AYSE));
    }

    #[test]
    fn messages_arrive_once_in_order_and_count_as_unread_until_read() {
        let mut history = History::open(None, ME);
        let conv = dm();
        assert!(history.put(&message(&conv, 20, AYSE, "selam")).is_some());
        assert!(history.put(&message(&conv, 10, AYSE, "önce")).is_some());
        // The minute of overlap of a sync brings it again: nothing changes.
        assert!(history.put(&message(&conv, 20, AYSE, "selam")).is_none());
        assert!(history.put(&message(&conv, 30, ME, "naber")).is_some());
        let meta = history.meta(&conv).unwrap();
        assert_eq!(meta.unread, 2, "the player's own message is never unread");
        assert_eq!(meta.count, 3);
        assert_eq!(meta.last.as_ref().unwrap().t, "naber");
        let (entries, more) = history.page(&conv, None, 50);
        assert!(!more);
        let ids: Vec<i64> = entries
            .iter()
            .map(|e| match e {
                Entry::Sent(m) => m.id,
                Entry::Outgoing(_) => -1,
            })
            .collect();
        assert_eq!(ids, [10, 20, 30]);
        assert!(history.mark_read(&conv));
        assert_eq!(history.meta(&conv).unwrap().unread, 0);
        assert!(!history.mark_read(&conv), "nothing new to read");
        assert_eq!(history.sync_since(), 0, "30 ms is less than a minute");
        history.put(&message(&conv, 1_792_521_000_000, AYSE, "yeni"));
        assert_eq!(history.sync_since(), 1_792_521_000_000 - SYNC_OVERLAP_MS);
        assert_eq!(history.meta(&conv).unwrap().unread, 1);
    }

    #[test]
    fn an_edit_replaces_the_text_and_a_delete_takes_it_away() {
        let mut history = History::open(None, ME);
        let conv = CREW.to_owned();
        history.put(&message(&conv, 10, AYSE, "Geliyrum"));
        let edited = ChatMessage {
            text: "Geliyorum".into(),
            mac: "new".into(),
            edited_at: Some(15),
            ..message(&conv, 10, AYSE, "")
        };
        let change = history.put(&edited).unwrap();
        assert_eq!(change.upsert, [Entry::Sent(edited.clone())]);
        assert_eq!(
            history.meta(&conv).unwrap().unread,
            1,
            "an edit is no new message"
        );
        assert_eq!(history.message(&conv, 10), Some(edited));
        let change = history.remove(&conv, 10).unwrap();
        assert_eq!(change.remove, ["i:10"]);
        assert_eq!(history.meta(&conv).unwrap().unread, 0);
        assert!(history.meta(&conv).unwrap().last.is_none());
        assert!(
            history.remove(&conv, 10).is_none(),
            "deleting again changes nothing"
        );
    }

    #[test]
    fn the_players_own_message_keeps_its_key_from_typing_to_sent() {
        let mut history = History::open(None, ME);
        let conv = dm();
        let change = history
            .add_outgoing(&conv, "selam", "q8Zr3kT0aWbc", 5)
            .unwrap();
        assert_eq!(change.upsert[0].key(), "n:q8Zr3kT0aWbc");
        assert_eq!(history.outgoing().len(), 1);
        // It did not go; then it goes again.
        let failed = history
            .set_outgoing("q8Zr3kT0aWbc", Some(SendFailure::Connection))
            .unwrap();
        assert!(matches!(&failed.upsert[0], Entry::Outgoing(o) if o.failed.is_some()));
        assert!(history.set_outgoing("q8Zr3kT0aWbc", None).is_some());
        // The service took it: the same key, now sent, and nothing waits any more.
        let taken = ChatMessage {
            nonce: Some("q8Zr3kT0aWbc".into()),
            ..message(&conv, 40, ME, "selam")
        };
        let change = history.put(&taken).unwrap();
        assert_eq!(change.upsert[0].key(), "n:q8Zr3kT0aWbc");
        assert!(history.outgoing().is_empty());
        // The same message again (an echo of the answer) changes nothing.
        assert!(history.put(&taken).is_none());
        // A message that failed for good may be dropped.
        history.add_outgoing(&conv, "link", "zz", 6);
        let gone = history.discard_outgoing("zz").unwrap();
        assert_eq!(gone.remove, ["n:zz"]);
        assert!(history.outgoing().is_empty());
    }

    #[test]
    fn cleared_messages_never_come_back() {
        let mut history = History::open(None, ME);
        let conv = dm();
        history.put(&message(&conv, 10, AYSE, "a"));
        history.put(&message(&conv, 20, AYSE, "b"));
        history.add_outgoing(&conv, "yolda", "n1", 7);
        let change = history.clear(&conv).unwrap();
        assert!(change.reset);
        assert_eq!(change.upsert.len(), 1, "a message on its way stays");
        assert_eq!(history.meta(&conv).unwrap().count, 0);
        // A sync lists them again: they stay away; a newer one comes.
        assert!(history.put(&message(&conv, 20, AYSE, "b")).is_none());
        assert!(history.put(&message(&conv, 25, AYSE, "c")).is_some());
        assert_eq!(history.page(&conv, None, 10).0.len(), 2);
    }

    #[test]
    fn pages_go_back_in_time_and_the_history_stays_within_its_limit() {
        let mut history = History::open(None, ME);
        let conv = CREW.to_owned();
        for id in 1..=(HISTORY_LIMIT as i64 + 3) {
            history.put(&message(&conv, id, AYSE, "x"));
        }
        let meta = history.meta(&conv).unwrap();
        assert_eq!(meta.count as usize, HISTORY_LIMIT);
        let (newest, more) = history.page(&conv, None, 60);
        assert!(more);
        assert_eq!(newest.len(), 60);
        let first = match &newest[0] {
            Entry::Sent(m) => m.id,
            Entry::Outgoing(_) => unreachable!(),
        };
        let (older, _) = history.page(&conv, Some(first), 60);
        assert_eq!(older.len(), 60);
        assert!(matches!(&older[59], Entry::Sent(m) if m.id == first - 1));
        // The three oldest went to keep the limit.
        assert!(history.message(&conv, 3).is_none());
        assert!(history.message(&conv, 4).is_some());
        let before = history.before(&conv, 100, 20);
        assert_eq!(before.len(), 20);
        assert_eq!(before[0].id, 80);
        assert_eq!(before[19].id, 99);
    }

    #[test]
    fn the_history_survives_a_restart_and_another_account_is_apart() {
        let dir = tempfile::tempdir().unwrap();
        let conv = dm();
        {
            let mut history = History::open(Some(dir.path()), ME);
            history.put(&message(&conv, 10, AYSE, "selam"));
            history.put(&message(&conv, 20, AYSE, "nasılsın"));
            let edited = ChatMessage {
                text: "nasılsın?".into(),
                edited_at: Some(25),
                ..message(&conv, 20, AYSE, "")
            };
            history.put(&edited);
            history.add_outgoing(&conv, "iyiyim", "n9", 30);
            history.set_title(&conv, "Ayşe");
            history.put(&message(CREW, 5, AYSE, "ekip"));
            history.clear(CREW);
        }
        let mut history = History::open(Some(dir.path()), ME);
        let (entries, _) = history.page(&conv, None, 50);
        assert_eq!(entries.len(), 3);
        assert!(
            matches!(&entries[1], Entry::Sent(m) if m.text == "nasılsın?" && m.edited_at == Some(25))
        );
        assert!(matches!(&entries[2], Entry::Outgoing(o) if o.text == "iyiyim"));
        assert_eq!(history.meta(&conv).unwrap().title.as_deref(), Some("Ayşe"));
        assert_eq!(history.meta(&conv).unwrap().unread, 2);
        assert!(
            history.put(&message(CREW, 5, AYSE, "ekip")).is_none(),
            "still cleared"
        );
        // Another account on the same computer sees none of it.
        let other = History::open(Some(dir.path()), "100000000000000009");
        assert_eq!(other.conversations().count(), 0);
        history.erase();
        let again = History::open(Some(dir.path()), ME);
        assert_eq!(again.conversations().count(), 0);
    }
}

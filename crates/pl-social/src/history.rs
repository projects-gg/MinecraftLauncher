//! The chat history on this device (CHAT1, CHAT2). The service keeps only a short mailbox (the
//! last 200 messages of the last 7 days per conversation) for devices that were away; the history
//! belongs to the devices. Per account: one small index (`index.json`: what was read, what was
//! cleared here, the newest message seen, the messages still waiting to go, the read marks) and
//! one file per conversation, one message per line, at most [`HISTORY_LIMIT`] messages. A few
//! conversations stay in memory; the others are read when they are needed. Link cards live beside
//! them (`cards/`, see [`crate::cards`]).
//!
//! Every change is written at once (an append for a new message or a reaction, a rewrite for an
//! edit or a delete, both through a temporary file and a rename for rewrites), so a crash loses
//! nothing that was shown. A reaction appends the message's newer version (a later line of the
//! same id wins when the file is read); the file is written anew now and then, and by every
//! rewrite. Nothing here talks to the service.
//!
//! A message deleted for everyone stays in its place as a tombstone (FIX2): who sent it and when,
//! who deleted it. Its text, sticker, reply link, reactions and seal are erased, in memory and in
//! the file, and never come back (a sync or a late event may still list the message). Only a
//! message this device held becomes one; tombstones count towards the history's limit like any
//! message, never as unread.
//!
//! CHAT2 adds to a message its sticker, the link to the message it answers (with that link's
//! seal) and its reactions; to a conversation the player's read mark as the service knows it and
//! a friend's shared one ("Görüldü"); to the index whether the device caught up with the extras
//! once. Older files read as they are; an older launcher ignores what it does not know.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::cards::CardStore;
use crate::model::{ChatBody, ChatMessage, ChatReplyRef, LinkRule, Reaction};
use crate::view::SendFailure;

/// Messages kept per conversation on this device; the oldest go first.
pub const HISTORY_LIMIT: usize = 5_000;
/// Conversations kept in memory (the others stay on disk until they are needed).
const LOADED_MAX: usize = 8;
/// A sync starts this much before the newest message held (the service's advice: duplicates are
/// dropped by conversation and id).
pub const SYNC_OVERLAP_MS: i64 = 60_000;
/// The first sync with chat extras reaches back this far (CHAT2: the mailbox's week), once.
pub const CATCH_UP_MS: i64 = 7 * 86_400_000;
const INDEX_FILE: &str = "index.json";
/// The index's `extras` once this device caught up with CHAT2's extras.
const EXTRAS_CAUGHT_UP: u32 = 1;
/// Newer versions of messages appended to a conversation's file before it is written anew.
const APPENDED_MAX: usize = 256;

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

/// Two emoji are the same reaction (❤ and ❤️: the service adds a missing VS16; a skin tone makes
/// another).
pub fn same_emoji(a: &str, b: &str) -> bool {
    a.chars()
        .filter(|c| *c != '\u{FE0F}')
        .eq(b.chars().filter(|c| *c != '\u{FE0F}'))
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
    /// Deleted for everyone: a tombstone, its text and seal erased.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    d: Option<Gone>,
    /// A sticker in place of the text (CHAT2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    s: Option<String>,
    /// The message it answers, and that link's seal (CHAT2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    p: Option<Parent>,
    /// Its reactions (CHAT2).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    x: Vec<Reacts>,
}

/// Who deleted a message for everyone (`b`: a user id, `staff`, or empty when not known) and when
/// (`a`: the hub's clock in ms, 0 when not known).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Gone {
    #[serde(default)]
    b: String,
    #[serde(default)]
    a: i64,
}

/// The message a reply answers (`i`, `f`) and the seal of that link (`m`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Parent {
    i: i64,
    f: String,
    #[serde(default)]
    m: String,
}

/// One kind of reaction: the emoji and who gave it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Reacts {
    e: String,
    #[serde(default)]
    b: Vec<String>,
}

/// Overwrites a text before it is dropped: what a deletion erases should not linger in memory.
fn erase(text: &mut String) {
    let mut bytes = std::mem::take(text).into_bytes();
    bytes.fill(0);
}

fn reacts_of(reactions: &[Reaction]) -> Vec<Reacts> {
    reactions
        .iter()
        .filter(|reaction| !reaction.by.is_empty())
        .map(|reaction| Reacts {
            e: reaction.e.clone(),
            b: reaction.by.clone(),
        })
        .collect()
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
            d: None,
            s: message.sticker.clone(),
            p: message.reply.as_ref().map(|reply| Parent {
                i: reply.id,
                f: reply.from.clone(),
                m: message.rmac.clone().unwrap_or_default(),
            }),
            x: reacts_of(&message.reactions),
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
            sticker: self.s.clone(),
            reply: self.p.as_ref().map(|parent| ChatReplyRef {
                id: parent.i,
                from: parent.f.clone(),
            }),
            rmac: self
                .p
                .as_ref()
                .map(|parent| parent.m.clone())
                .filter(|mac| !mac.is_empty()),
            reactions: self
                .x
                .iter()
                .map(|reaction| Reaction {
                    e: reaction.e.clone(),
                    by: reaction.b.clone(),
                })
                .collect(),
        }
    }

    fn key(&self) -> String {
        key_of(self.n.as_deref(), self.i)
    }

    /// The line as shown: a message, or the tombstone of one.
    fn entry(&self, conv: &str) -> Entry {
        match &self.d {
            Some(gone) => Entry::Deleted(Tombstone {
                conv: conv.to_owned(),
                id: self.i,
                from: self.f.clone(),
                nonce: self.n.clone(),
                by: gone.b.clone(),
                at: gone.a,
            }),
            None => Entry::Sent(self.message(conv)),
        }
    }

    fn last(&self) -> Last {
        Last {
            i: self.i,
            f: self.f.clone(),
            t: self.t.clone(),
            d: self.d.as_ref().map(|gone| gone.b.clone()),
            s: self.s.clone(),
        }
    }
}

fn key_of(nonce: Option<&str>, id: i64) -> String {
    match nonce.filter(|nonce| !nonce.is_empty()) {
        Some(nonce) => format!("n:{nonce}"),
        None => format!("i:{id}"),
    }
}

/// The key a message keeps on screen from the moment it is typed: its nonce when it has one (the
/// player's own messages), else its id. Its tombstone keeps the same key.
pub fn message_key(message: &ChatMessage) -> String {
    key_of(message.nonce.as_deref(), message.id)
}

/// Messages from others after `read` that count as unread (tombstones never do).
fn unread_after(lines: &[Line], read: i64, me: &str) -> u32 {
    let unread = lines
        .iter()
        .filter(|line| line.i > read && line.d.is_none() && line.f != me)
        .count();
    u32::try_from(unread).unwrap_or(u32::MAX)
}

/// Gives (`on`) or takes back `by`'s reaction `e`; whether anything changed. A new kind goes last,
/// a kind nobody keeps goes.
fn apply_reaction(reactions: &mut Vec<Reacts>, by: &str, e: &str, on: bool) -> bool {
    let kind = reactions
        .iter()
        .position(|reaction| same_emoji(&reaction.e, e));
    match (kind, on) {
        (Some(at), true) => {
            let people = &mut reactions[at].b;
            if people.iter().any(|someone| someone == by) {
                return false;
            }
            people.push(by.to_owned());
            true
        }
        (None, true) => {
            reactions.push(Reacts {
                e: e.to_owned(),
                b: vec![by.to_owned()],
            });
            true
        }
        (Some(at), false) => {
            let people = &mut reactions[at].b;
            let before = people.len();
            people.retain(|someone| someone != by);
            let changed = people.len() != before;
            if people.is_empty() {
                reactions.remove(at);
            }
            changed
        }
        (None, false) => false,
    }
}

/// A message deleted for everyone, as this device keeps it in its place: who sent it and when
/// (`id`, the hub's clock), who deleted it and when. Its text and seal are gone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tombstone {
    pub conv: String,
    pub id: i64,
    pub from: String,
    /// The sender's own key (the player's messages): the tombstone keeps the message's key.
    pub nonce: Option<String>,
    /// A user id (the sender, or a crew's leader or moderator), `staff` for Projects' moderators,
    /// or empty when not known (that account is gone, or the service did not say).
    pub by: String,
    /// The hub's clock in ms when it was deleted (0 when not known).
    pub at: i64,
}

impl Tombstone {
    pub fn key(&self) -> String {
        key_of(self.nonce.as_deref(), self.id)
    }
}

/// A message the player sent from this device that the service has not taken yet: on its way
/// (`failed` None), or turned away and kept for "Tekrar dene".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outgoing {
    pub conv: String,
    pub nonce: String,
    /// The text; empty for a sticker.
    pub text: String,
    /// A sticker in place of the text (CHAT2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sticker: Option<String>,
    /// The message it answers: its id in the same conversation (CHAT2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply: Option<i64>,
    /// Unix milliseconds on this computer when the player sent it.
    pub at: i64,
    #[serde(default)]
    pub failed: Option<SendFailure>,
    /// With `failed` `link`: the link rule that refused it, when the service said (CHAT2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<LinkRule>,
}

impl Outgoing {
    pub fn key(&self) -> String {
        format!("n:{}", self.nonce)
    }

    /// What it carries.
    pub fn body(&self) -> ChatBody {
        match &self.sticker {
            Some(sticker) => ChatBody::Sticker(sticker.clone()),
            None => ChatBody::Text(self.text.clone()),
        }
    }
}

/// One entry of a conversation as shown: a message the service took, the tombstone of one
/// deleted for everyone, or one on its way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    Sent(ChatMessage),
    Deleted(Tombstone),
    Outgoing(Outgoing),
}

impl Entry {
    pub fn key(&self) -> String {
        match self {
            Self::Sent(message) => message_key(message),
            Self::Deleted(tombstone) => tombstone.key(),
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

    /// One entry of `conv`, as it is now.
    pub fn of(conv: &str, entry: Entry) -> Self {
        Self {
            conv: conv.to_owned(),
            upsert: vec![entry],
            ..Self::default()
        }
    }
}

/// The newest message of a conversation, for the list (`d`: it was deleted for everyone, by whom;
/// its text is empty then; `s`: a sticker, CHAT2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Last {
    pub i: i64,
    pub f: String,
    pub t: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub d: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub s: Option<String>,
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
    /// The player's read mark as the service knows it (CHAT2): sent from here, or heard from
    /// another device. Marks go only forward.
    #[serde(default)]
    pub mark: i64,
    /// A direct conversation's friend has read up to this message (CHAT2, "Görüldü"; 0: not
    /// known). Only marks the friend shared arrive.
    #[serde(default)]
    pub peer_read: i64,
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
    /// [`EXTRAS_CAUGHT_UP`] once this device synced back a week with CHAT2's extras.
    #[serde(default)]
    extras: u32,
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
    /// Newer versions appended to each loaded conversation's file since it was last written.
    appended: HashMap<String, usize>,
    cards: CardStore,
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
        let cards = CardStore::open(dir.as_ref().map(|dir| dir.join("cards")));
        Self {
            dir,
            me: me.to_owned(),
            index,
            loaded: HashMap::new(),
            recent: VecDeque::new(),
            appended: HashMap::new(),
            cards,
        }
    }

    pub fn me(&self) -> &str {
        &self.me
    }

    /// The link cards kept with this history (CHAT2).
    pub fn cards(&mut self) -> &mut CardStore {
        &mut self.cards
    }

    /// Where the next sync starts (ms on the hub's clock): the newest message seen minus a
    /// minute, 0 on a new device.
    pub fn sync_since(&self) -> i64 {
        (self.index.newest - SYNC_OVERLAP_MS).max(0)
    }

    /// Where the first sync with chat extras starts (CHAT2): the newest message seen minus a
    /// week; negative once this device caught up.
    pub fn catch_up_since(&self) -> i64 {
        if self.index.extras >= EXTRAS_CAUGHT_UP {
            -1
        } else {
            (self.index.newest - CATCH_UP_MS).max(0)
        }
    }

    /// The week-long sync with chat extras is done (CHAT2): later syncs start at the newest.
    pub fn set_caught_up(&mut self) {
        if self.index.extras < EXTRAS_CAUGHT_UP {
            self.index.extras = EXTRAS_CAUGHT_UP;
            self.save_index();
        }
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
    /// its nonce, and reads everything before it (written on another device too). None when
    /// nothing changed (a duplicate, or a message cleared here).
    pub fn put(&mut self, message: &ChatMessage) -> Option<Change> {
        let conv = message.conv.clone();
        Conversation::parse(&conv)?;
        let me = self.me.clone();
        let mut change = Change::new(&conv);
        let mine = message.from == me;
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
        let mut line = Line::of(message);
        let read = self.index.convs.get(&conv).map_or(0, |meta| meta.read);
        let lines = self.lines(&conv);
        let (inserted, appended) = match lines.binary_search_by_key(&line.i, |l| l.i) {
            // Deleted for everyone here: it never comes back, whatever lists it again.
            Ok(at) if lines[at].d.is_some() => {
                let entry = lines[at].entry(&conv);
                if !settled {
                    return None;
                }
                // The answer to the player's own message came after its tombstone: nothing of
                // it waits any more.
                change.upsert.push(entry);
                self.save_index();
                return Some(change);
            }
            Ok(at) => {
                // What a newer version leaves out stays: the sender's own key, a reply's link
                // (an edit from a device that did not send it), a sticker.
                let held = &lines[at];
                if line.n.is_none() {
                    line.n.clone_from(&held.n);
                }
                if line.p.is_none() {
                    line.p.clone_from(&held.p);
                }
                if line.s.is_none() {
                    line.s.clone_from(&held.s);
                }
                if *held == line {
                    if !settled {
                        return None;
                    }
                    (false, true)
                } else {
                    lines[at] = line;
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
        let last = lines.last().map(Line::last);
        let count = u32::try_from(lines.len()).unwrap_or(u32::MAX);
        // The player's own message reads everything before it.
        let recount = (mine && message.id > read).then(|| unread_after(lines, message.id, &me));
        if let Some(stored) = stored {
            change.upsert.push(Entry::Sent(stored));
        }
        for line in &dropped {
            change.remove.push(line.key());
        }
        let meta = self.index.convs.entry(conv.clone()).or_default();
        if inserted && !mine && message.id > read {
            meta.unread = meta.unread.saturating_add(1);
        }
        for line in &dropped {
            // A tombstone stopped counting when its message was deleted.
            if line.d.is_none() && line.f != me && line.i > meta.read {
                meta.unread = meta.unread.saturating_sub(1);
            }
        }
        if let Some(unread) = recount {
            meta.read = message.id;
            meta.unread = unread;
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

    /// A message was deleted for everyone (`by`: a user id, `staff`, or empty when not known;
    /// `at`: the hub's clock in ms, 0 when not known). The device keeps a tombstone in its place:
    /// the same key, who sent it and when, who deleted it. Its text, sticker, reply link,
    /// reactions and seal are erased here and in the file. None when it was not held (no
    /// tombstone for a message this device never had), or when its tombstone learns nothing new
    /// (a deleter not known before is learnt).
    pub fn delete(&mut self, conv: &str, id: i64, by: &str, at: i64) -> Option<Change> {
        Conversation::parse(conv)?;
        let me = self.me.clone();
        let lines = self.lines(conv);
        let index = lines.binary_search_by_key(&id, |l| l.i).ok()?;
        let line = &mut lines[index];
        let was_message = line.d.is_none();
        match &mut line.d {
            Some(gone) => {
                if !gone.b.is_empty() || by.is_empty() {
                    return None;
                }
                gone.b = by.to_owned();
                if at > 0 {
                    gone.a = at;
                }
            }
            None => {
                erase(&mut line.t);
                erase(&mut line.m);
                line.e = None;
                line.s = None;
                line.p = None;
                line.x.clear();
                line.d = Some(Gone {
                    b: by.to_owned(),
                    a: at.max(0),
                });
            }
        }
        let entry = line.entry(conv);
        let from_someone_else = line.f != me;
        let last = lines.last().map(Line::last);
        let mut change = Change::new(conv);
        change.upsert.push(entry);
        let meta = self.index.convs.entry(conv.to_owned()).or_default();
        // There is nothing left to read in it.
        if was_message && from_someone_else && id > meta.read {
            meta.unread = meta.unread.saturating_sub(1);
        }
        meta.last = last;
        self.write_lines(conv);
        self.save_index();
        Some(change)
    }

    // --- reactions (CHAT2) -------------------------------------------------------------------

    /// `by` gave (`on`) or took back reaction `e` on message `id` (an event, or the player's own
    /// at once). None when it changed nothing, or the message is not held (or deleted).
    pub fn react(&mut self, conv: &str, id: i64, by: &str, e: &str, on: bool) -> Option<Change> {
        Conversation::parse(conv)?;
        let lines = self.lines(conv);
        let at = lines.binary_search_by_key(&id, |l| l.i).ok()?;
        let line = &mut lines[at];
        if line.d.is_some() || !apply_reaction(&mut line.x, by, e, on) {
            return None;
        }
        let entry = line.entry(conv);
        self.append_version(conv, at);
        Some(Change::of(conv, entry))
    }

    /// A message's reactions as the service says they are now (an answer, a sync). None when
    /// nothing changed, or the message is not held (or deleted).
    pub fn set_reactions(&mut self, conv: &str, id: i64, reactions: &[Reaction]) -> Option<Change> {
        Conversation::parse(conv)?;
        let next = reacts_of(reactions);
        let lines = self.lines(conv);
        let at = lines.binary_search_by_key(&id, |l| l.i).ok()?;
        let line = &mut lines[at];
        if line.d.is_some() || line.x == next {
            return None;
        }
        line.x = next;
        let entry = line.entry(conv);
        self.append_version(conv, at);
        Some(Change::of(conv, entry))
    }

    // --- read marks (CHAT2) ------------------------------------------------------------------

    /// The player read the conversation up to message `id` on another device: what came before
    /// it is read here too (the unread count follows). Whether the read point moved.
    pub fn read_up_to(&mut self, conv: &str, id: i64) -> bool {
        if Conversation::parse(conv).is_none() {
            return false;
        }
        let me = self.me.clone();
        let read = self.index.convs.get(conv).map_or(0, |meta| meta.read);
        let recount = (id > read).then(|| unread_after(self.lines(conv), id, &me));
        let meta = self.index.convs.entry(conv.to_owned()).or_default();
        let marked = id > meta.mark;
        meta.mark = meta.mark.max(id);
        if let Some(unread) = recount {
            meta.read = id;
            meta.unread = unread;
        }
        if marked || recount.is_some() {
            self.save_index();
        }
        recount.is_some()
    }

    /// A friend's shared read mark in a direct conversation ("Görüldü"): only forward. Whether it
    /// moved.
    pub fn set_peer_read(&mut self, conv: &str, id: i64) -> bool {
        if !matches!(Conversation::parse(conv), Some(Conversation::Direct(..))) {
            return false;
        }
        let meta = self.index.convs.entry(conv.to_owned()).or_default();
        if id <= meta.peer_read {
            return false;
        }
        meta.peer_read = id;
        self.save_index();
        true
    }

    /// The player's read mark as the service knows it (0: none).
    pub fn mark(&self, conv: &str) -> i64 {
        self.index.convs.get(conv).map_or(0, |meta| meta.mark)
    }

    /// The player's read mark went to the service (only forward).
    pub fn set_mark(&mut self, conv: &str, id: i64) {
        if Conversation::parse(conv).is_none() {
            return;
        }
        let meta = self.index.convs.entry(conv.to_owned()).or_default();
        if id > meta.mark {
            meta.mark = id;
            self.save_index();
        }
    }

    /// The message a read mark names now: someone else's newest message, when it is the newest of
    /// all (after the player's own message there is nothing to mark: it reads what came before).
    pub fn read_target(&mut self, conv: &str) -> Option<i64> {
        Conversation::parse(conv)?;
        let me = self.me.clone();
        let newest = self
            .lines(conv)
            .iter()
            .rev()
            .find(|line| line.d.is_none())?;
        (newest.f != me).then_some(newest.i)
    }

    // --- looking things up -------------------------------------------------------------------

    /// One entry by its id: the message, or its tombstone.
    pub fn entry(&mut self, conv: &str, id: i64) -> Option<Entry> {
        Conversation::parse(conv)?;
        let lines = self.lines(conv);
        let at = lines.binary_search_by_key(&id, |l| l.i).ok()?;
        Some(lines[at].entry(conv))
    }

    /// One entry by the key it has on screen (`i:<id>`, or `n:<nonce>` for the player's own,
    /// sent or still on its way).
    pub fn entry_by_key(&mut self, conv: &str, key: &str) -> Option<Entry> {
        if let Some(id) = key.strip_prefix("i:").and_then(|id| id.parse().ok()) {
            return self.entry(conv, id);
        }
        let nonce = key.strip_prefix("n:").filter(|nonce| !nonce.is_empty())?;
        Conversation::parse(conv)?;
        let found = self
            .lines(conv)
            .iter()
            .rev()
            .find(|line| line.n.as_deref() == Some(nonce))
            .map(|line| line.entry(conv));
        found.or_else(|| {
            self.index
                .outgoing
                .iter()
                .find(|o| o.conv == conv && o.nonce == nonce)
                .cloned()
                .map(Entry::Outgoing)
        })
    }

    /// The entries that answer message `id` (they show it as their quote), held or on their way.
    pub fn replies_to(&mut self, conv: &str, id: i64) -> Vec<Entry> {
        if Conversation::parse(conv).is_none() {
            return Vec::new();
        }
        let mut entries: Vec<Entry> = self
            .lines(conv)
            .iter()
            .filter(|line| line.d.is_none() && line.p.as_ref().is_some_and(|p| p.i == id))
            .map(|line| line.entry(conv))
            .collect();
        entries.extend(
            self.index
                .outgoing
                .iter()
                .filter(|o| o.conv == conv && o.reply == Some(id))
                .cloned()
                .map(Entry::Outgoing),
        );
        entries
    }

    // --- messages on their way -------------------------------------------------------------

    /// The player sent a message from here (a text or a sticker, maybe answering message
    /// `reply`): it waits, shown at once, until the service takes it.
    pub fn add_outgoing(
        &mut self,
        conv: &str,
        body: &ChatBody,
        reply: Option<i64>,
        nonce: &str,
        at: i64,
    ) -> Option<Change> {
        Conversation::parse(conv)?;
        let (text, sticker) = match body {
            ChatBody::Text(text) => (text.clone(), None),
            ChatBody::Sticker(sticker) => (String::new(), Some(sticker.clone())),
        };
        let outgoing = Outgoing {
            conv: conv.to_owned(),
            nonce: nonce.to_owned(),
            text,
            sticker,
            reply,
            at,
            failed: None,
            rule: None,
        };
        self.index.outgoing.push(outgoing.clone());
        self.index.convs.entry(conv.to_owned()).or_default();
        self.save_index();
        Some(Change::of(conv, Entry::Outgoing(outgoing)))
    }

    /// A message on its way did not go (`failure`, with the link rule that refused it), or goes
    /// again (None).
    pub fn set_outgoing(
        &mut self,
        nonce: &str,
        failure: Option<SendFailure>,
        rule: Option<LinkRule>,
    ) -> Option<Change> {
        let outgoing = self
            .index
            .outgoing
            .iter_mut()
            .find(|outgoing| outgoing.nonce == nonce)?;
        let rule = rule.filter(|_| failure.is_some());
        if outgoing.failed == failure && outgoing.rule == rule {
            return None;
        }
        outgoing.failed = failure;
        outgoing.rule = rule;
        let change = Change::of(&outgoing.conv, Entry::Outgoing(outgoing.clone()));
        self.save_index();
        Some(change)
    }

    /// A message on its way goes without its reply link (the message it answered is gone, or
    /// Studio switched replies off): the text still goes.
    pub fn unlink_outgoing(&mut self, nonce: &str) -> Option<Change> {
        let outgoing = self
            .index
            .outgoing
            .iter_mut()
            .find(|outgoing| outgoing.nonce == nonce && outgoing.reply.is_some())?;
        outgoing.reply = None;
        let change = Change::of(&outgoing.conv, Entry::Outgoing(outgoing.clone()));
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
            .map(|line| line.entry(conv))
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

    /// One message as held (for an edit, a delete, a report or a reply); a tombstone has none.
    pub fn message(&mut self, conv: &str, id: i64) -> Option<ChatMessage> {
        Conversation::parse(conv)?;
        let lines = self.lines(conv);
        let at = lines.binary_search_by_key(&id, |l| l.i).ok()?;
        let line = &lines[at];
        line.d.is_none().then(|| line.message(conv))
    }

    /// Up to `count` messages before `id`, oldest first (a report's context; tombstones carry
    /// nothing to show).
    pub fn before(&mut self, conv: &str, id: i64, count: usize) -> Vec<ChatMessage> {
        if Conversation::parse(conv).is_none() {
            return Vec::new();
        }
        let lines = self.lines(conv);
        let end = lines.partition_point(|l| l.i < id);
        let mut before: Vec<ChatMessage> = lines[..end]
            .iter()
            .rev()
            .filter(|line| line.d.is_none())
            .take(count)
            .map(|line| line.message(conv))
            .collect();
        before.reverse();
        before
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
        self.appended.remove(conv);
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
            let (lines, versions) = self.read_lines(conv);
            self.loaded.insert(conv.to_owned(), lines);
            self.appended.insert(conv.to_owned(), versions);
            while self.recent.len() > LOADED_MAX {
                if let Some(old) = self.recent.pop_front() {
                    self.loaded.remove(&old);
                    self.appended.remove(&old);
                }
            }
        }
        self.loaded.entry(conv.to_owned()).or_default()
    }

    /// The file's messages, and how many older versions it holds besides them.
    fn read_lines(&self, conv: &str) -> (Vec<Line>, usize) {
        let Some(path) = self.conversation_path(conv) else {
            return (Vec::new(), 0);
        };
        let Ok(text) = fs::read_to_string(path) else {
            return (Vec::new(), 0);
        };
        let mut lines: Vec<Line> = text
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        let read = lines.len();
        lines.sort_by_key(|l| l.i);
        // A later line of the same id is its newer version; a tombstone is never undone.
        let mut kept: Vec<Line> = Vec::with_capacity(lines.len());
        for line in lines {
            match kept.last_mut() {
                Some(last) if last.i == line.i => {
                    if last.d.is_none() || line.d.is_some() {
                        *last = line;
                    }
                }
                _ => kept.push(line),
            }
        }
        let versions = read - kept.len();
        let extra = kept.len().saturating_sub(HISTORY_LIMIT);
        kept.drain(..extra);
        (kept, versions)
    }

    fn append_line(&self, conv: &str, message: &ChatMessage) {
        self.append_raw(conv, &Line::of(message));
    }

    fn append_raw(&self, conv: &str, line: &Line) {
        let Some(path) = self.conversation_path(conv) else {
            return;
        };
        let Ok(mut text) = serde_json::to_string(line) else {
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

    /// The newer version of line `at` goes to the end of the file (it wins when the file is
    /// read); past [`APPENDED_MAX`] such versions the file is written anew.
    fn append_version(&mut self, conv: &str, at: usize) {
        let count = self.loaded.get(conv).map_or(0, Vec::len);
        let versions = self.appended.entry(conv.to_owned()).or_default();
        *versions += 1;
        if *versions > APPENDED_MAX.max(count / 2) {
            self.appended.insert(conv.to_owned(), 0);
            self.write_lines(conv);
            return;
        }
        if let Some(line) = self.loaded.get(conv).and_then(|lines| lines.get(at)) {
            self.append_raw(conv, line);
        }
    }

    fn write_lines(&mut self, conv: &str) {
        let Some(path) = self.conversation_path(conv) else {
            return;
        };
        let Some(lines) = self.loaded.get(conv) else {
            return;
        };
        self.appended.insert(conv.to_owned(), 0);
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
        self.appended.clear();
        self.cards.erase();
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
            ..ChatMessage::default()
        }
    }

    fn text(body: &str) -> ChatBody {
        ChatBody::Text(body.to_owned())
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
        assert_eq!(history.meta(&conv).unwrap().unread, 2);
        assert!(history.put(&message(&conv, 30, ME, "naber")).is_some());
        let meta = history.meta(&conv).unwrap();
        assert_eq!(
            meta.unread, 0,
            "the player's own message reads everything before it"
        );
        assert_eq!(meta.count, 3);
        assert_eq!(meta.last.as_ref().unwrap().t, "naber");
        let (entries, more) = history.page(&conv, None, 50);
        assert!(!more);
        let ids: Vec<i64> = entries
            .iter()
            .map(|e| match e {
                Entry::Sent(m) => m.id,
                Entry::Deleted(t) => t.id,
                Entry::Outgoing(_) => -1,
            })
            .collect();
        assert_eq!(ids, [10, 20, 30]);
        history.put(&message(&conv, 40, AYSE, "iyi"));
        assert_eq!(history.meta(&conv).unwrap().unread, 1);
        assert!(history.mark_read(&conv));
        assert_eq!(history.meta(&conv).unwrap().unread, 0);
        assert!(!history.mark_read(&conv), "nothing new to read");
        assert_eq!(history.sync_since(), 0, "40 ms is less than a minute");
        history.put(&message(&conv, 1_792_521_000_000, AYSE, "yeni"));
        assert_eq!(history.sync_since(), 1_792_521_000_000 - SYNC_OVERLAP_MS);
        assert_eq!(history.meta(&conv).unwrap().unread, 1);
    }

    #[test]
    fn an_edit_replaces_the_text_and_a_delete_leaves_a_tombstone() {
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
        assert_eq!(history.message(&conv, 10), Some(edited.clone()));
        let change = history.delete(&conv, 10, AYSE, 20).unwrap();
        let tombstone = Tombstone {
            conv: conv.clone(),
            id: 10,
            from: AYSE.into(),
            nonce: None,
            by: AYSE.into(),
            at: 20,
        };
        // The same key: the bubble turns into its tombstone in place.
        assert_eq!(change.upsert, [Entry::Deleted(tombstone.clone())]);
        assert!(change.remove.is_empty());
        let meta = history.meta(&conv).unwrap();
        assert_eq!(meta.unread, 0, "nothing left to read in it");
        assert_eq!(meta.count, 1, "a tombstone keeps its place");
        let last = meta.last.as_ref().unwrap();
        assert_eq!((last.t.as_str(), last.d.as_deref()), ("", Some(AYSE)));
        assert_eq!(history.page(&conv, None, 10).0, [Entry::Deleted(tombstone)]);
        // Nothing to edit, delete or report any more.
        assert_eq!(history.message(&conv, 10), None);
        assert!(
            history.delete(&conv, 10, AYSE, 21).is_none(),
            "deleting again changes nothing"
        );
        // A late event or a sync that lists it again never brings the text back.
        assert!(history.put(&edited).is_none());
        assert!(history.put(&message(&conv, 10, AYSE, "Geliyrum")).is_none());
        assert_eq!(history.message(&conv, 10), None);
    }

    #[test]
    fn only_messages_held_here_leave_a_tombstone() {
        let mut history = History::open(None, ME);
        let conv = dm();
        assert!(history.delete(&conv, 10, AYSE, 20).is_none());
        assert!(history.meta(&conv).is_none(), "nothing was made for it");
        history.put(&message(&conv, 10, AYSE, "selam"));
        history.put(&message(&conv, 20, AYSE, "naber"));
        // Who deleted it is learnt later (an edit found it gone before the event came).
        let first = history.delete(&conv, 20, "", 0).unwrap();
        assert!(matches!(&first.upsert[0], Entry::Deleted(t) if t.by.is_empty()));
        let learnt = history.delete(&conv, 20, "staff", 30).unwrap();
        assert!(matches!(&learnt.upsert[0], Entry::Deleted(t) if t.by == "staff" && t.at == 30));
        assert!(
            history.delete(&conv, 20, AYSE, 31).is_none(),
            "known already"
        );
        assert_eq!(
            history.meta(&conv).unwrap().unread,
            1,
            "the first is unread"
        );
        // The player's own message keeps its nonce key; the report context skips tombstones.
        let mine = ChatMessage {
            nonce: Some("k3y".into()),
            ..message(&conv, 30, ME, "benim")
        };
        history.put(&mine);
        let change = history.delete(&conv, 30, ME, 40).unwrap();
        assert_eq!(change.upsert[0].key(), "n:k3y");
        history.put(&message(&conv, 50, AYSE, "son"));
        let context: Vec<i64> = history.before(&conv, 50, 20).iter().map(|m| m.id).collect();
        assert_eq!(context, [10]);
    }

    #[test]
    fn a_tombstone_keeps_nothing_of_the_text_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let conv = dm();
        {
            let mut history = History::open(Some(dir.path()), ME);
            history.put(&message(&conv, 10, AYSE, "gizli sözcük"));
            history.put(&message(&conv, 20, AYSE, "kalan"));
            // A reaction appends a newer version of the line: the deletion erases that too.
            history.react(&conv, 10, ME, "😮", true);
            history.delete(&conv, 10, AYSE, 15);
        }
        let file = dir
            .path()
            .join(ME)
            .join(format!("{}.jsonl", conv.replace(':', "_")));
        let written = fs::read_to_string(&file).unwrap();
        assert!(!written.contains("gizli"), "{written}");
        assert!(!written.contains("mac10"), "the seal goes too: {written}");
        assert!(!written.contains("😮"), "and the reactions: {written}");
        assert!(written.contains("kalan"));
        let index = fs::read_to_string(dir.path().join(ME).join(INDEX_FILE)).unwrap();
        assert!(!index.contains("gizli"), "{index}");
        // After a restart it is still the tombstone, and it stays one.
        let mut history = History::open(Some(dir.path()), ME);
        let (entries, _) = history.page(&conv, None, 10);
        assert!(matches!(&entries[0], Entry::Deleted(t) if t.id == 10 && t.by == AYSE));
        assert!(
            history
                .put(&message(&conv, 10, AYSE, "gizli sözcük"))
                .is_none()
        );
        // The newest one deleted: the list shows that, never its text.
        history.delete(&conv, 20, AYSE, 25);
        let last = history.meta(&conv).unwrap().last.clone().unwrap();
        assert_eq!(
            (last.i, last.t.as_str(), last.d.as_deref()),
            (20, "", Some(AYSE))
        );
        let index = fs::read_to_string(dir.path().join(ME).join(INDEX_FILE)).unwrap();
        assert!(!index.contains("kalan"), "{index}");
    }

    #[test]
    fn the_players_own_message_keeps_its_key_from_typing_to_sent() {
        let mut history = History::open(None, ME);
        let conv = dm();
        let change = history
            .add_outgoing(&conv, &text("selam"), None, "q8Zr3kT0aWbc", 5)
            .unwrap();
        assert_eq!(change.upsert[0].key(), "n:q8Zr3kT0aWbc");
        assert_eq!(history.outgoing().len(), 1);
        // It did not go; then it goes again.
        let failed = history
            .set_outgoing("q8Zr3kT0aWbc", Some(SendFailure::Connection), None)
            .unwrap();
        assert!(matches!(&failed.upsert[0], Entry::Outgoing(o) if o.failed.is_some()));
        assert!(history.set_outgoing("q8Zr3kT0aWbc", None, None).is_some());
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
        history.add_outgoing(&conv, &text("link"), None, "zz", 6);
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
        history.add_outgoing(&conv, &text("yolda"), None, "n1", 7);
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
            Entry::Deleted(_) | Entry::Outgoing(_) => unreachable!(),
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
            history.add_outgoing(&conv, &text("iyiyim"), None, "n9", 30);
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

    #[test]
    fn stickers_replies_and_reactions_are_kept_and_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let conv = dm();
        let reply = ChatMessage {
            reply: Some(ChatReplyRef {
                id: 10,
                from: AYSE.into(),
            }),
            rmac: Some("rmac20".into()),
            reactions: vec![Reaction {
                e: "👍".into(),
                by: vec![AYSE.into()],
            }],
            nonce: Some("own".into()),
            ..message(&conv, 20, ME, "evet")
        };
        let sticker = ChatMessage {
            text: String::new(),
            sticker: Some("pl1/gg".into()),
            ..message(&conv, 30, AYSE, "")
        };
        {
            let mut history = History::open(Some(dir.path()), ME);
            history.put(&message(&conv, 10, AYSE, "Geliyor musun?"));
            history.put(&reply);
            history.put(&sticker);
            let last = history.meta(&conv).unwrap().last.clone().unwrap();
            assert_eq!(last.s.as_deref(), Some("pl1/gg"));
        }
        let mut history = History::open(Some(dir.path()), ME);
        assert_eq!(history.message(&conv, 20), Some(reply.clone()));
        assert_eq!(history.message(&conv, 30), Some(sticker.clone()));
        // What goes back to the service: the reply with its link and seal, the sticker in place
        // of the text.
        let sealed = history.message(&conv, 20).unwrap().sealed();
        assert_eq!(sealed["rmac"], "rmac20");
        assert_eq!(
            history.message(&conv, 30).unwrap().sealed()["sticker"],
            "pl1/gg"
        );
        // An edit from a device that did not send the link keeps the link held here.
        let edited = ChatMessage {
            text: "evet, 9'da".into(),
            mac: "new".into(),
            edited_at: Some(25),
            reply: None,
            rmac: None,
            nonce: None,
            ..reply.clone()
        };
        history.put(&edited).unwrap();
        let held = history.message(&conv, 20).unwrap();
        assert_eq!(held.text, "evet, 9'da");
        assert_eq!(held.reply.as_ref().map(|r| r.id), Some(10));
        assert_eq!(held.rmac.as_deref(), Some("rmac20"));
        assert_eq!(held.nonce.as_deref(), Some("own"), "and its key");
        assert_eq!(history.replies_to(&conv, 10).len(), 1);
        assert_eq!(
            history.entry_by_key(&conv, "n:own").map(|e| e.key()),
            Some("n:own".into())
        );
        assert!(matches!(history.entry_by_key(&conv, "i:30"), Some(Entry::Sent(m)) if m.id == 30));
        assert!(history.entry_by_key(&conv, "i:99").is_none());
        // An older file reads as it is (no CHAT2 fields).
        let old = r#"{"i":40,"f":"100000000000000002","t":"eski","m":"x"}"#;
        let line: Line = serde_json::from_str(old).unwrap();
        assert_eq!(line.message(&conv).sticker, None);
        assert!(line.message(&conv).reactions.is_empty());
    }

    #[test]
    fn reactions_change_one_by_one_and_as_the_service_says() {
        let dir = tempfile::tempdir().unwrap();
        let conv = CREW.to_owned();
        {
            let mut history = History::open(Some(dir.path()), ME);
            history.put(&message(&conv, 10, AYSE, "gg"));
            let change = history.react(&conv, 10, AYSE, "❤", true).unwrap();
            assert!(matches!(&change.upsert[0], Entry::Sent(m) if m.reactions.len() == 1));
            // ❤ and ❤️ are one; giving it again changes nothing.
            assert!(history.react(&conv, 10, ME, "❤️", true).is_some());
            assert!(history.react(&conv, 10, ME, "❤️", true).is_none());
            assert!(history.react(&conv, 10, ME, "😂", true).is_some());
            let held = history.message(&conv, 10).unwrap();
            assert_eq!(held.reactions[0].by, [AYSE, ME]);
            assert_eq!(held.reactions[1].e, "😂");
            // Taking back the last of a kind takes the kind.
            assert!(history.react(&conv, 10, ME, "😂", false).is_some());
            assert!(history.react(&conv, 10, ME, "😂", false).is_none());
            assert_eq!(history.message(&conv, 10).unwrap().reactions.len(), 1);
            // The service's word replaces what is held; the same again changes nothing.
            let said = [Reaction {
                e: "🔥".into(),
                by: vec![AYSE.into()],
            }];
            assert!(history.set_reactions(&conv, 10, &said).is_some());
            assert!(history.set_reactions(&conv, 10, &said).is_none());
            // Reactions are never news.
            assert_eq!(history.meta(&conv).unwrap().unread, 1);
            assert!(
                history.react(&conv, 99, ME, "👍", true).is_none(),
                "not held"
            );
        }
        // The newest version wins after a restart.
        let mut history = History::open(Some(dir.path()), ME);
        let held = history.message(&conv, 10).unwrap();
        assert_eq!(held.reactions.len(), 1);
        assert_eq!(held.reactions[0].e, "🔥");
    }

    #[test]
    fn appended_versions_are_folded_into_the_file_now_and_then() {
        let dir = tempfile::tempdir().unwrap();
        let conv = CREW.to_owned();
        let file = dir
            .path()
            .join(ME)
            .join(format!("{}.jsonl", conv.replace(':', "_")));
        let mut history = History::open(Some(dir.path()), ME);
        history.put(&message(&conv, 10, AYSE, "gg"));
        for n in 0..(APPENDED_MAX + 5) {
            history.react(&conv, 10, ME, "👍", n % 2 == 0);
        }
        let lines = fs::read_to_string(&file).unwrap().lines().count();
        assert!(lines < APPENDED_MAX, "{lines} lines");
        let held = history.message(&conv, 10).unwrap();
        let again = History::open(Some(dir.path()), ME)
            .message(&conv, 10)
            .unwrap();
        assert_eq!(held, again);
    }

    #[test]
    fn read_marks_keep_devices_in_step_and_say_what_the_friend_saw() {
        let mut history = History::open(None, ME);
        let conv = dm();
        history.put(&message(&conv, 10, AYSE, "a"));
        history.put(&message(&conv, 20, AYSE, "b"));
        history.put(&message(&conv, 30, AYSE, "c"));
        assert_eq!(history.read_target(&conv), Some(30));
        // Read up to 20 on the phone: one unread left here.
        assert!(history.read_up_to(&conv, 20));
        let meta = history.meta(&conv).unwrap();
        assert_eq!((meta.read, meta.unread, meta.mark), (20, 1, 20));
        assert!(!history.read_up_to(&conv, 15), "only forward");
        history.set_mark(&conv, 30);
        assert_eq!(history.mark(&conv), 30);
        history.set_mark(&conv, 25);
        assert_eq!(history.mark(&conv), 30, "a mark never goes back");
        // The friend's shared mark, only forward.
        assert!(history.set_peer_read(&conv, 25));
        assert!(!history.set_peer_read(&conv, 20));
        assert_eq!(history.meta(&conv).unwrap().peer_read, 25);
        assert!(!history.set_peer_read(CREW, 5), "crews have no Görüldü");
        // After the player's own message there is nothing to mark.
        history.put(&message(&conv, 40, ME, "tamam"));
        assert_eq!(history.read_target(&conv), None);
        assert_eq!(history.meta(&conv).unwrap().unread, 0);
    }

    #[test]
    fn a_device_catches_up_with_the_extras_once() {
        let dir = tempfile::tempdir().unwrap();
        let conv = dm();
        {
            let mut history = History::open(Some(dir.path()), ME);
            assert_eq!(history.catch_up_since(), 0, "a new device: everything");
            history.put(&message(&conv, 1_792_521_000_000, AYSE, "selam"));
            assert_eq!(history.catch_up_since(), 1_792_521_000_000 - CATCH_UP_MS);
            history.set_caught_up();
            assert!(history.catch_up_since() < 0);
        }
        let history = History::open(Some(dir.path()), ME);
        assert!(history.catch_up_since() < 0, "kept in the index");
    }

    #[test]
    fn stickers_and_replies_wait_like_any_message() {
        let dir = tempfile::tempdir().unwrap();
        let conv = dm();
        {
            let mut history = History::open(Some(dir.path()), ME);
            history.put(&message(&conv, 10, AYSE, "Geliyor musun?"));
            history.add_outgoing(
                &conv,
                &ChatBody::Sticker("pl1/gg".into()),
                Some(10),
                "s1",
                5,
            );
            history.add_outgoing(&conv, &text("evet"), Some(10), "t1", 6);
            history.set_outgoing("t1", Some(SendFailure::Link), Some(LinkRule::Shortener));
        }
        let mut history = History::open(Some(dir.path()), ME);
        let waiting = history.outgoing().to_vec();
        assert_eq!(waiting[0].body(), ChatBody::Sticker("pl1/gg".into()));
        assert_eq!(waiting[0].reply, Some(10));
        assert_eq!(waiting[1].rule, Some(LinkRule::Shortener));
        assert_eq!(history.replies_to(&conv, 10).len(), 2);
        // The answered message is gone: the text goes without the link.
        let change = history.unlink_outgoing("t1").unwrap();
        assert!(matches!(&change.upsert[0], Entry::Outgoing(o) if o.reply.is_none()));
        assert!(history.unlink_outgoing("t1").is_none());
        // A rule goes only with a failure.
        history.set_outgoing("s1", None, Some(LinkRule::Form));
        assert_eq!(history.outgoing_of("s1").unwrap().rule, None);
    }
}

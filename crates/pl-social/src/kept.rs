//! SOC-DISK: the signed-in account's profile and lists, kept on this computer with the relations
//! version that describes them (SCALE-SOC), so a launcher start shows them without reading them
//! again (`GET /v1/friends?rel=1`) while nothing they show changed.
//!
//! The file holds one read's answer exactly as the service sent it (`me`, the lists and their
//! version), so reading it back is reading that answer again, also for a newer launcher. Only a
//! read whose version the launcher keeps writes it, and it goes as soon as the copy in memory no
//! longer matches that version (an event or the player's own change forgets the version too), at
//! a sign-out and with "Verilerimi sil". A start uses it only for the account it names and only
//! when it can still be current (`SocialService` checks the day and the profile); the presence
//! `hello` then says whether it is. It holds what the friends panel shows, never a token or a
//! secret. A file this version cannot read (cut short, broken, of another format or request, too
//! large) is no copy: the start reads the lists as before and writes a good one over it.

use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::client::{FRIENDS_TARGET, MAX_RESPONSE_BYTES};
use crate::model::{FriendsOverview, Me};
use crate::presence::is_rel_version;

/// The file's format. A new one makes every kept copy be read again once.
const FORMAT: u32 = 1;

/// An answer is at most `MAX_RESPONSE_BYTES`: a file far larger is not one of ours.
const MAX_BYTES: u64 = 2 * MAX_RESPONSE_BYTES as u64;

/// What one read brought, as kept.
#[derive(Debug, Clone, PartialEq)]
pub struct KeptCopy {
    /// When it was read (Unix seconds, this computer's clock).
    pub at: i64,
    /// The relations version of `me` and `lists` (16 lowercase hex digits).
    pub rel: String,
    /// The profile the read brought, moderators' notes included.
    pub me: Me,
    /// The lists the read brought, `me` and `rel` taken out.
    pub lists: FriendsOverview,
}

/// The file: whose answer, when it was read, the request it answered, and the answer.
#[derive(Serialize, Deserialize)]
struct Document {
    v: u32,
    account: String,
    at: i64,
    from: String,
    answer: serde_json::Value,
}

/// Where the signed-in account's copy lives (`<data>/social/lists.json`): one account's at a
/// time, as one account is signed in at a time.
#[derive(Debug, Clone)]
pub struct KeptLists {
    path: PathBuf,
}

impl KeptLists {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The copy kept for `account`, when there is a whole one of this format.
    pub fn load(&self, account: &str) -> Option<KeptCopy> {
        if std::fs::metadata(&self.path).ok()?.len() > MAX_BYTES {
            return None;
        }
        let bytes = std::fs::read(&self.path).ok()?;
        let document: Document = serde_json::from_slice(&bytes).ok()?;
        if document.v != FORMAT || document.account != account || document.from != FRIENDS_TARGET {
            return None;
        }
        let mut lists: FriendsOverview = serde_json::from_value(document.answer).ok()?;
        let me = lists.me.take().filter(|me| me.profile.id == account)?;
        let rel = lists.rel.take().filter(|rel| is_rel_version(rel))?;
        Some(KeptCopy {
            at: document.at,
            rel,
            me,
            lists,
        })
    }

    /// Keeps `account`'s answer to `GET /v1/friends?rel=1` (read at `at`, Unix seconds) in place
    /// of the copy before. Writes through a temporary file, so a crash never leaves half a copy.
    pub fn save(&self, account: &str, at: i64, answer: &[u8]) -> io::Result<()> {
        let bytes = serde_json::to_vec(&Document {
            v: FORMAT,
            account: account.to_owned(),
            at,
            from: FRIENDS_TARGET.to_owned(),
            answer: serde_json::from_slice(answer)?,
        })?;
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let temporary = self.temporary();
        std::fs::write(&temporary, bytes)?;
        std::fs::rename(&temporary, &self.path)
    }

    /// Forgets the copy; forgetting nothing is fine.
    pub fn clear(&self) {
        for path in [&self.path, &self.temporary()] {
            if let Err(error) = std::fs::remove_file(path)
                && error.kind() != io::ErrorKind::NotFound
            {
                tracing::warn!(%error, "could not delete the friend list kept on this computer");
            }
        }
    }

    fn temporary(&self) -> PathBuf {
        self.path.with_extension("json.tmp")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACCOUNT: &str = "123456789012345678";
    const AT: i64 = 1_792_521_000;

    /// One answer with what this launcher reads loosely (a notice kind, roles, permissions and a
    /// join setting on the wire's words, a mark it does not know, a crew feature it does not
    /// know): kept as it came, it reads back exactly as the read did.
    const ANSWER: &str = r#"{"friends":[{"id":"223456789012345678","username":"craftyfox",
        "displayName":"CraftyFox","avatar":"preset:fox","avatarHash":null,"since":"2026-10-01",
        "lastSeen":"today","badge":{"crew":"0123456789abcdef","tag":"GKR","name":"Gece Kartalları",
        "emblem":"sword"},"mark":"admin"},{"id":"323456789012345678","username":"pixel_rider",
        "displayName":null,"avatar":"discord","avatarHash":"c3","since":"2026-10-02",
        "lastSeen":null,"mark":"someday"}],
        "incoming":[{"id":"423456789012345678","username":"lavasurfer9","displayName":"LavaSurfer9",
        "avatar":"preset:cat","avatarHash":null,"sentAt":1760000100}],"outgoing":[],
        "blocked":[{"id":"523456789012345678","username":"grief_bot","displayName":null,
        "avatar":"discord","avatarHash":null,"since":"2026-10-03"}],
        "limits":{"friends":200,"outgoingRequests":50,"teamMembers":30,"groups":20},
        "groups":{"version":2,"groups":[{"id":"oyun","name":"Oyun"}],
        "members":{"223456789012345678":"oyun"}},
        "team":{"id":"0123456789abcdef","name":"Gece Kartalları","tag":"GKR","emblem":"sword",
          "code":"TKM-ABCD-EFGH","createdOn":"2026-10-19","join":"approval",
          "permissions":{"invite":"members","call":"moderators","kick":"leader","edit":"leader",
            "admit":"moderators"},
          "members":[{"id":"123456789012345678","username":"nightowl_42",
          "displayName":"NightOwl_42","avatar":"discord","avatarHash":null,"role":"founder",
          "joinedAt":5},{"id":"223456789012345678","username":"craftyfox",
          "displayName":"CraftyFox","avatar":"preset:fox","avatarHash":null,"role":"member",
          "moderator":true,"joinedAt":6}]},
        "teamInvited":["323456789012345678"],"teamInvites":[],
        "teamFeatures":["look","calls","roles","mute","requests","link","futureThing"],
        "teamNotifications":{"calls":true,"muted":false},"features":["push","chat"],
        "me":{"id":"123456789012345678","username":"nightowl_42","displayName":"NightOwl_42",
          "avatar":"discord","avatarHash":null,"discordAvatarHash":"a1b2",
          "friendCode":"PRJ-ABCD-EFGH","status":"online","privacy":{"shareServer":false,"shareLastSeen":true,"shareRead":true},
          "createdOn":"2026-10-16","settings":{"version":3,"updatedAt":1760000000,
          "device":"Laptop","size":64},
          "notices":[{"id":"n1","kind":"profile_reset","at":1760000000,"reason":"name"}]},
        "rel":"0123456789abcdef"}"#;

    /// The answer as the client reads it.
    fn read(answer: &str) -> FriendsOverview {
        serde_json::from_str(answer).unwrap()
    }

    fn answer_with(change: impl Fn(&mut serde_json::Value)) -> Vec<u8> {
        let mut value: serde_json::Value = serde_json::from_str(ANSWER).unwrap();
        change(&mut value);
        serde_json::to_vec(&value).unwrap()
    }

    fn file(dir: &tempfile::TempDir) -> KeptLists {
        KeptLists::new(dir.path().join("social").join("lists.json"))
    }

    #[test]
    fn a_kept_copy_reads_back_as_the_read_brought_it() {
        let dir = tempfile::tempdir().unwrap();
        let kept = file(&dir);
        // Nothing kept yet (no folder either).
        assert_eq!(kept.load(ACCOUNT), None);
        kept.save(ACCOUNT, AT, ANSWER.as_bytes()).unwrap();
        // Another launcher run reads the same profile, lists and version back.
        let mut lists = read(ANSWER);
        let me = lists.me.take().unwrap();
        let rel = lists.rel.take().unwrap();
        let again = KeptLists::new(kept.path.clone());
        let copy = again.load(ACCOUNT).expect("a whole copy");
        assert_eq!(
            copy,
            KeptCopy {
                at: AT,
                rel,
                me,
                lists,
            }
        );
        assert_eq!(
            copy.me.notices[0].kind,
            crate::model::NoticeKind::ProfileReset
        );
        // Never another account's.
        assert_eq!(again.load("223456789012345678"), None);
        // A newer read replaces it.
        let newer = answer_with(|value| value["rel"] = "fedcba9876543210".into());
        again.save(ACCOUNT, AT + 60, &newer).unwrap();
        let copy = kept.load(ACCOUNT).unwrap();
        assert_eq!((copy.at, copy.rel.as_str()), (AT + 60, "fedcba9876543210"));
        assert!(!kept.temporary().exists());
    }

    #[test]
    fn no_session_or_secret_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let kept = file(&dir);
        kept.save(ACCOUNT, AT, ANSWER.as_bytes()).unwrap();
        let text = std::fs::read_to_string(&kept.path).unwrap();
        // The profile and the lists the panel shows, whose and when: nothing of the session.
        assert!(!text.contains("pls_"));
        assert!(!text.to_lowercase().contains("token"));
        let document: serde_json::Value = serde_json::from_str(&text).unwrap();
        let mut keys: Vec<_> = document.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        assert_eq!(keys, ["account", "answer", "at", "from", "v"]);
    }

    #[test]
    fn clearing_forgets_the_copy() {
        let dir = tempfile::tempdir().unwrap();
        let kept = file(&dir);
        // Nothing to forget is fine.
        kept.clear();
        kept.save(ACCOUNT, AT, ANSWER.as_bytes()).unwrap();
        std::fs::write(kept.temporary(), b"half").unwrap();
        kept.clear();
        assert!(!kept.path.exists());
        assert!(!kept.temporary().exists());
        assert_eq!(kept.load(ACCOUNT), None);
    }

    #[test]
    fn a_file_this_version_cannot_read_is_no_copy() {
        let dir = tempfile::tempdir().unwrap();
        let kept = file(&dir);
        let write = |bytes: &[u8]| {
            std::fs::create_dir_all(kept.path.parent().unwrap()).unwrap();
            std::fs::write(&kept.path, bytes).unwrap();
        };
        kept.save(ACCOUNT, AT, ANSWER.as_bytes()).unwrap();
        let good = std::fs::read(&kept.path).unwrap();
        let changed = |change: &dyn Fn(&mut serde_json::Value)| {
            let mut value: serde_json::Value = serde_json::from_slice(&good).unwrap();
            change(&mut value);
            serde_json::to_vec(&value).unwrap()
        };
        // Broken, cut short, empty.
        for bytes in [&b"{not json"[..], &good[..good.len() / 2], &b""[..]] {
            write(bytes);
            assert_eq!(kept.load(ACCOUNT), None);
        }
        // Another format; the answer to another request; lists that do not read; no version or
        // one that is not the service's; a profile of someone else.
        let changes: [&dyn Fn(&mut serde_json::Value); 6] = [
            &|value: &mut serde_json::Value| value["v"] = 2.into(),
            &|value: &mut serde_json::Value| value["from"] = "v1/friends?crews=lite&rel=1".into(),
            &|value: &mut serde_json::Value| value["answer"]["friends"] = "none".into(),
            &|value: &mut serde_json::Value| value["answer"]["rel"] = serde_json::Value::Null,
            &|value: &mut serde_json::Value| value["answer"]["rel"] = "0123456789ABCDEF".into(),
            &|value: &mut serde_json::Value| {
                value["answer"]["me"]["id"] = "223456789012345678".into();
            },
        ];
        for change in changes {
            write(&changed(change));
            assert_eq!(kept.load(ACCOUNT), None);
        }
        // Far too large to be one.
        let large = std::fs::File::create(&kept.path).unwrap();
        large.set_len(MAX_BYTES + 1).unwrap();
        drop(large);
        assert_eq!(kept.load(ACCOUNT), None);
        // The good one reads, and the next read writes over a broken one.
        write(&good);
        assert!(kept.load(ACCOUNT).is_some());
        write(b"{not json");
        kept.save(ACCOUNT, AT, ANSWER.as_bytes()).unwrap();
        assert!(kept.load(ACCOUNT).is_some());
        // An answer that is not JSON keeps nothing new.
        assert!(kept.save(ACCOUNT, AT, b"<html>").is_err());
    }
}

//! "Birlikte oyna" invites the player answered or closed, kept on this computer until they
//! expire.
//!
//! The service lists the invites of the last hour again on every connection (`hello`), also
//! after the launcher restarts. [`SocialState`](crate::SocialState) keeps the closed ones closed
//! while it runs; this small file carries them over a restart. Per account and friend it holds
//! the time of the newest invite the player answered or closed (the same rule as in memory).
//! Entries older than [`INVITE_LIFETIME_SECS`] are dropped on every read and write, so the file
//! never holds more than the last hour, and it is removed once nothing is left.

use std::collections::HashMap;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::state::INVITE_LIFETIME_SECS;

/// Per friend id, the time (Unix seconds) of the newest invite the player answered or closed.
pub type ClosedInvites = HashMap<String, i64>;

/// Where the closed invites of every account on this computer outlive a restart.
#[derive(Debug, Clone)]
pub struct ClosedInvitesFile {
    path: PathBuf,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Document {
    /// Per account (Discord user id).
    #[serde(default)]
    accounts: HashMap<String, ClosedInvites>,
}

impl ClosedInvitesFile {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The account's closed invites that the service may still send (`now` in Unix seconds). A
    /// missing or unreadable file is an empty list; expired entries leave the file.
    pub fn load(&self, account: &str, now: i64) -> ClosedInvites {
        let mut document = self.read();
        let before = entries(&document);
        prune(&mut document, now);
        if entries(&document) != before
            && let Err(error) = self.write(&document)
        {
            tracing::debug!(%error, "could not drop expired closed invites");
        }
        document.accounts.remove(account).unwrap_or_default()
    }

    /// Replaces the account's closed invites; expired entries of every account are dropped.
    pub fn save(&self, account: &str, closed: &ClosedInvites, now: i64) -> io::Result<()> {
        let mut document = self.read();
        document.accounts.insert(account.to_owned(), closed.clone());
        prune(&mut document, now);
        self.write(&document)
    }

    fn read(&self) -> Document {
        std::fs::read(&self.path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Writes through a temporary file (a crash never leaves half a document); nothing left
    /// removes the file.
    fn write(&self, document: &Document) -> io::Result<()> {
        if document.accounts.is_empty() {
            return match std::fs::remove_file(&self.path) {
                Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
                _ => Ok(()),
            };
        }
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let temporary = self.path.with_extension("json.tmp");
        std::fs::write(&temporary, serde_json::to_vec(document)?)?;
        std::fs::rename(&temporary, &self.path)
    }
}

fn prune(document: &mut Document, now: i64) {
    for closed in document.accounts.values_mut() {
        closed.retain(|_, at| now - *at < INVITE_LIFETIME_SECS);
    }
    document.accounts.retain(|_, closed| !closed.is_empty());
}

fn entries(document: &Document) -> usize {
    document.accounts.values().map(HashMap::len).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_792_521_000;

    /// A file of its own under the system temp folder, removed when the test ends.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("pl-social-closed-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            Self(dir)
        }

        fn file(&self) -> ClosedInvitesFile {
            ClosedInvitesFile::new(self.0.join("social").join("closed-invites.json"))
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn closed(entries: &[(&str, i64)]) -> ClosedInvites {
        entries
            .iter()
            .map(|(from, at)| ((*from).to_owned(), *at))
            .collect()
    }

    #[test]
    fn keeps_each_accounts_closed_invites_until_they_expire() {
        let scratch = Scratch::new("accounts");
        let file = scratch.file();
        // Nothing saved yet (no folder either): nothing closed.
        assert!(file.load("ayse", NOW).is_empty());
        file.save(
            "ayse",
            &closed(&[("bora", NOW - 60), ("cem", NOW - 3_000)]),
            NOW,
        )
        .unwrap();
        file.save("deniz", &closed(&[("bora", NOW - 10)]), NOW)
            .unwrap();
        // Another launcher run reads them back, per account.
        let again = ClosedInvitesFile::new(file.path.clone());
        assert_eq!(
            again.load("ayse", NOW + 60),
            closed(&[("bora", NOW - 60), ("cem", NOW - 3_000)])
        );
        assert_eq!(again.load("deniz", NOW + 60), closed(&[("bora", NOW - 10)]));
        // Cem's invite is an hour old ten minutes later: it can no longer come back.
        assert_eq!(again.load("ayse", NOW + 600), closed(&[("bora", NOW - 60)]));
        // Saving an account's list replaces it and keeps the others.
        again.save("ayse", &closed(&[]), NOW + 600).unwrap();
        assert!(again.load("ayse", NOW + 600).is_empty());
        assert_eq!(
            again.load("deniz", NOW + 600),
            closed(&[("bora", NOW - 10)])
        );
        // Once everything has expired the file goes away.
        assert!(again.load("deniz", NOW + 3_600).is_empty());
        assert!(!file.path.exists());
    }

    #[test]
    fn an_unreadable_file_counts_as_nothing_closed() {
        let scratch = Scratch::new("broken");
        let file = scratch.file();
        std::fs::create_dir_all(file.path.parent().unwrap()).unwrap();
        std::fs::write(&file.path, b"{not json").unwrap();
        assert!(file.load("ayse", NOW).is_empty());
        // The next save writes a good document over it.
        file.save("ayse", &closed(&[("bora", NOW)]), NOW).unwrap();
        assert_eq!(file.load("ayse", NOW), closed(&[("bora", NOW)]));
    }
}

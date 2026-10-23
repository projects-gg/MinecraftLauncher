//! Each username account's Projects skin as this computer saved it (SKIN1, skin1.md §5.6): the
//! current and the previous source (for "Geri al" and the preview) and the change on its way. Only
//! on this computer; nothing here is sent anywhere but back to the friends service as a change.
//!
//! `<dir>/<account id>/state.json` with `current.png`, `previous.png` and `pending.png` beside it
//! (the clean PNG of a `png` source, the texture of a `premium` one). A copy goes when its account
//! goes or is renamed (the new name is another account on Projects), and when it was not used for
//! 180 days.

use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use pl_auth::SkinVariant;
use serde::{Deserialize, Serialize};

use super::wire::{McPremium, McSource};

/// Copies of accounts not opened for this long go.
pub const KEPT_UNUSED: Duration = Duration::from_secs(180 * 24 * 3_600);
const STATE_FILE: &str = "state.json";
const VERSION: u32 = 1;

/// One source of a Projects skin, as kept here. The pictures live in the files beside it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum KeptSource {
    Png {
        variant: SkinVariant,
        sha256: String,
        /// The texture Mojang made of it, once applied.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        texture: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Premium {
        name: String,
        id: String,
        texture: String,
        variant: SkinVariant,
        value: String,
        signature: String,
    },
    Default,
}

impl KeptSource {
    /// The texture id the service will name for this skin, when known.
    pub fn texture(&self) -> Option<&str> {
        match self {
            Self::Png { texture, .. } => texture.as_deref(),
            Self::Premium { texture, .. } => Some(texture),
            Self::Default => None,
        }
    }

    pub fn variant(&self) -> Option<SkinVariant> {
        match self {
            Self::Png { variant, .. } | Self::Premium { variant, .. } => Some(*variant),
            Self::Default => None,
        }
    }

    pub fn has_picture(&self) -> bool {
        !matches!(self, Self::Default)
    }

    /// What `PUT /v1/mc/skin` sends for it again; None when its picture is missing.
    pub fn to_source(&self, png: Option<Vec<u8>>) -> Option<McSource> {
        Some(match self {
            Self::Png {
                variant, sha256, ..
            } => McSource::Png {
                variant: *variant,
                png: png?,
                sha256: sha256.clone(),
            },
            Self::Premium {
                name,
                id,
                texture,
                variant,
                value,
                signature,
            } => McSource::Premium(McPremium {
                name: name.clone(),
                id: id.clone(),
                texture: texture.clone(),
                variant: *variant,
                value: value.clone(),
                signature: signature.clone(),
            }),
            Self::Default => McSource::Default,
        })
    }
}

/// A change sent from here and not closed yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeptPending {
    pub job: String,
    pub source: KeptSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Kept {
    #[serde(default)]
    pub v: u32,
    /// The game account's name the copy belongs to.
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub current: Option<KeptSource>,
    #[serde(default)]
    pub previous: Option<KeptSource>,
    #[serde(default)]
    pub pending: Option<KeptPending>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Current,
    Previous,
    Pending,
}

impl Slot {
    fn file(self) -> &'static str {
        match self {
            Self::Current => "current.png",
            Self::Previous => "previous.png",
            Self::Pending => "pending.png",
        }
    }
}

/// The copies of all accounts. Blocking file work, small (a few KiB per account).
#[derive(Debug, Clone)]
pub struct SkinStore {
    dir: PathBuf,
}

fn valid_account(account: &str) -> bool {
    !account.is_empty()
        && account.len() <= 64
        && account
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

impl SkinStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn folder(&self, account: &str) -> Option<PathBuf> {
        valid_account(account).then(|| self.dir.join(account))
    }

    /// The copy of `account` for the game account `name`. A copy made for another name goes
    /// first (the account was renamed: the new name is someone else on Projects).
    pub fn read(&self, account: &str, name: &str) -> Kept {
        let Some(folder) = self.folder(account) else {
            return Kept::default();
        };
        let kept = fs::read(folder.join(STATE_FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Kept>(&bytes).ok());
        match kept {
            Some(kept) if kept.name.eq_ignore_ascii_case(name) => {
                touch(&folder);
                kept
            }
            Some(_) => {
                let _ = fs::remove_dir_all(&folder);
                Kept::default()
            }
            None => Kept::default(),
        }
    }

    pub fn picture(&self, account: &str, slot: Slot) -> Option<Vec<u8>> {
        let folder = self.folder(account)?;
        fs::read(folder.join(slot.file()))
            .ok()
            .filter(|bytes| bytes.starts_with(b"\x89PNG\r\n\x1a\n"))
    }

    fn write(&self, account: &str, kept: &Kept) -> io::Result<()> {
        let folder = self
            .folder(account)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "not an account id"))?;
        fs::create_dir_all(&folder)?;
        let bytes = serde_json::to_vec_pretty(kept).map_err(io::Error::other)?;
        write_atomic(&folder.join(STATE_FILE), &bytes)
    }

    /// A change of `name`'s skin went to the service as `job`: kept until it closes.
    pub fn set_pending(
        &self,
        account: &str,
        name: &str,
        job: &str,
        source: &KeptSource,
        picture: Option<&[u8]>,
    ) -> io::Result<()> {
        let mut kept = self.read(account, name);
        kept.v = VERSION;
        kept.name = name.to_owned();
        if let (Some(folder), Some(picture)) = (self.folder(account), picture) {
            fs::create_dir_all(&folder)?;
            write_atomic(&folder.join(Slot::Pending.file()), picture)?;
        }
        kept.pending = Some(KeptPending {
            job: job.to_owned(),
            source: source.clone(),
        });
        self.write(account, &kept)
    }

    /// The change `job` was applied (`texture`: what Mojang made of a `png`): it becomes the
    /// current skin and the one before it the previous one. False when it was not this copy's.
    pub fn applied(
        &self,
        account: &str,
        name: &str,
        job: &str,
        texture: Option<&str>,
    ) -> io::Result<bool> {
        let mut kept = self.read(account, name);
        let Some(pending) = kept.pending.take().filter(|pending| pending.job == job) else {
            return Ok(false);
        };
        let folder = self
            .folder(account)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "not an account id"))?;
        let mut source = pending.source;
        if let (KeptSource::Png { texture: kept, .. }, Some(texture)) = (&mut source, texture) {
            *kept = Some(texture.to_ascii_lowercase());
        }
        // current → previous, pending → current (pictures first, then the state).
        let _ = fs::remove_file(folder.join(Slot::Previous.file()));
        if kept.current.as_ref().is_some_and(KeptSource::has_picture) {
            let _ = fs::rename(
                folder.join(Slot::Current.file()),
                folder.join(Slot::Previous.file()),
            );
        }
        let _ = fs::remove_file(folder.join(Slot::Current.file()));
        if source.has_picture() {
            // Without its picture the skin still counts; the preview then asks Mojang for it.
            let _ = fs::rename(
                folder.join(Slot::Pending.file()),
                folder.join(Slot::Current.file()),
            );
        } else {
            let _ = fs::remove_file(folder.join(Slot::Pending.file()));
        }
        kept.previous = kept.current.take();
        kept.current = Some(source);
        self.write(account, &kept)?;
        Ok(true)
    }

    /// The change `job` ended without being applied (or was cancelled): forgotten.
    pub fn drop_pending(&self, account: &str, name: &str, job: &str) {
        let mut kept = self.read(account, name);
        if kept
            .pending
            .as_ref()
            .is_none_or(|pending| pending.job != job)
        {
            return;
        }
        kept.pending = None;
        if let Some(folder) = self.folder(account) {
            let _ = fs::remove_file(folder.join(Slot::Pending.file()));
        }
        if let Err(error) = self.write(account, &kept) {
            tracing::debug!(%error, "could not forget a Projects skin change");
        }
    }

    /// The source "Geri al" puts back, with its picture; None without one (or without the
    /// picture it needs).
    pub fn previous(&self, account: &str, name: &str) -> Option<(KeptSource, Option<Vec<u8>>)> {
        let previous = self.read(account, name).previous?;
        let picture = if previous.has_picture() {
            Some(self.picture(account, Slot::Previous)?)
        } else {
            None
        };
        Some((previous, picture))
    }

    /// Removes the copies of accounts that are gone (`accounts`: id and name of every account),
    /// of renamed ones, and those not opened for [`KEPT_UNUSED`].
    pub fn prune(&self, accounts: &[(String, String)], now: SystemTime) {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let id = entry.file_name().to_string_lossy().into_owned();
            let name = fs::read(path.join(STATE_FILE))
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Kept>(&bytes).ok())
                .map(|kept| kept.name);
            let known = accounts.iter().any(|(account, account_name)| {
                *account == id
                    && name
                        .as_deref()
                        .is_some_and(|name| name.eq_ignore_ascii_case(account_name))
            });
            let stale = fs::metadata(path.join(STATE_FILE))
                .and_then(|meta| meta.modified())
                .ok()
                .and_then(|modified| now.duration_since(modified).ok())
                .is_none_or(|age| age > KEPT_UNUSED);
            if !known || stale {
                let _ = fs::remove_dir_all(&path);
            }
        }
    }
}

/// Marks a copy as used now (the 180 days count from the last use).
fn touch(folder: &Path) {
    let path = folder.join(STATE_FILE);
    if let Ok(file) = fs::File::options().append(true).open(&path) {
        let _ = file.set_modified(SystemTime::now());
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    let result = (|| {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(tag: u8) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        bytes.push(tag);
        bytes
    }

    /// What "Geri al" would send for acc1.
    fn again(store: &SkinStore) -> Option<McSource> {
        let (previous, picture) = store.previous("acc1", "Ahmet")?;
        previous.to_source(picture)
    }

    fn png_source(sha: char) -> KeptSource {
        KeptSource::Png {
            variant: SkinVariant::Slim,
            sha256: sha.to_string().repeat(64),
            texture: None,
        }
    }

    #[test]
    fn a_change_becomes_current_and_the_one_before_previous() {
        let dir = tempfile::tempdir().unwrap();
        let store = SkinStore::new(dir.path());
        assert_eq!(store.read("acc1", "Ahmet"), Kept::default());
        store
            .set_pending("acc1", "Ahmet", "j1", &png_source('a'), Some(&png(1)))
            .unwrap();
        // Not this job: nothing changes.
        assert!(!store.applied("acc1", "Ahmet", "j9", None).unwrap());
        assert!(store.applied("acc1", "Ahmet", "j1", Some("ABCD")).unwrap());
        let kept = store.read("acc1", "ahmet");
        assert_eq!(
            kept.current,
            Some(KeptSource::Png {
                variant: SkinVariant::Slim,
                sha256: "a".repeat(64),
                texture: Some("abcd".into())
            })
        );
        assert_eq!(kept.previous, None);
        assert_eq!(store.picture("acc1", Slot::Current), Some(png(1)));
        // Nothing to put back after the first change.
        assert_eq!(again(&store), None);

        store
            .set_pending("acc1", "Ahmet", "j2", &png_source('b'), Some(&png(2)))
            .unwrap();
        assert!(store.applied("acc1", "Ahmet", "j2", Some("ef")).unwrap());
        assert_eq!(store.picture("acc1", Slot::Current), Some(png(2)));
        assert_eq!(store.picture("acc1", Slot::Previous), Some(png(1)));
        let Some(McSource::Png {
            png: back, sha256, ..
        }) = again(&store)
        else {
            panic!("the first skin can be put back");
        };
        assert_eq!((back, sha256), (png(1), "a".repeat(64)));

        // "Varsayılana dön": no picture of its own; "Geri al" then brings the PNG back.
        store
            .set_pending("acc1", "Ahmet", "j3", &KeptSource::Default, None)
            .unwrap();
        assert!(store.applied("acc1", "Ahmet", "j3", None).unwrap());
        assert_eq!(store.picture("acc1", Slot::Current), None);
        assert_eq!(store.picture("acc1", Slot::Previous), Some(png(2)));
        assert!(matches!(again(&store), Some(McSource::Png { .. })));
    }

    #[test]
    fn premium_sources_keep_their_signed_texture() {
        let dir = tempfile::tempdir().unwrap();
        let store = SkinStore::new(dir.path());
        let premium = KeptSource::Premium {
            name: "Dream".into(),
            id: "0".repeat(32),
            texture: "f".repeat(64),
            variant: SkinVariant::Classic,
            value: "dmFsdWU=".into(),
            signature: "c2ln".into(),
        };
        store
            .set_pending("acc1", "Ahmet", "j1", &premium, Some(&png(7)))
            .unwrap();
        store.applied("acc1", "Ahmet", "j1", None).unwrap();
        store
            .set_pending("acc1", "Ahmet", "j2", &KeptSource::Default, None)
            .unwrap();
        store.applied("acc1", "Ahmet", "j2", None).unwrap();
        let Some(McSource::Premium(back)) = again(&store) else {
            panic!("the premium skin comes back");
        };
        assert_eq!(
            (back.name.as_str(), back.value.as_str()),
            ("Dream", "dmFsdWU=")
        );
    }

    #[test]
    fn failures_and_cancels_forget_the_change() {
        let dir = tempfile::tempdir().unwrap();
        let store = SkinStore::new(dir.path());
        store
            .set_pending("acc1", "Ahmet", "j1", &png_source('a'), Some(&png(1)))
            .unwrap();
        store.drop_pending("acc1", "Ahmet", "j0");
        assert!(store.read("acc1", "Ahmet").pending.is_some());
        store.drop_pending("acc1", "Ahmet", "j1");
        let kept = store.read("acc1", "Ahmet");
        assert_eq!((kept.pending, kept.current), (None, None));
        assert_eq!(store.picture("acc1", Slot::Pending), None);
    }

    #[test]
    fn a_renamed_or_removed_account_loses_its_copy() {
        let dir = tempfile::tempdir().unwrap();
        let store = SkinStore::new(dir.path());
        for (account, name) in [("acc1", "Ahmet"), ("acc2", "Mehmet"), ("acc3", "Ali")] {
            store
                .set_pending(account, name, "j1", &png_source('a'), Some(&png(1)))
                .unwrap();
            store.applied(account, name, "j1", None).unwrap();
        }
        // Read under another name: the copy goes.
        assert_eq!(store.read("acc1", "Veli"), Kept::default());
        assert!(!dir.path().join("acc1").exists());
        // acc2 is gone, acc3 stays.
        store.prune(&[("acc3".into(), "ali".into())], SystemTime::now());
        assert!(!dir.path().join("acc2").exists());
        assert!(dir.path().join("acc3").exists());
        // Not opened for 180 days.
        store.prune(
            &[("acc3".into(), "Ali".into())],
            SystemTime::now() + KEPT_UNUSED + Duration::from_secs(60),
        );
        assert!(!dir.path().join("acc3").exists());
        // Ids that are not ids are never paths.
        assert_eq!(store.read("../x", "Ahmet"), Kept::default());
        assert!(
            store
                .set_pending("..", "Ahmet", "j1", &KeptSource::Default, None)
                .is_err()
        );
    }
}

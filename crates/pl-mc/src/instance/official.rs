//! The official "Projects" instance and the player's choice about it (LIB1).
//!
//! Every instance can be deleted, the official one and its copies too. Once the player has deleted
//! the last official instance, nothing brings it back by itself: not the next start, the pack
//! watcher, the welcome flow, the tray's "Oyna", "Katıl" or an import. Only an explicit request
//! ("Resmî paketi kur") does, or putting the instance back from the launcher trash. It works like
//! the Projects server in a new server list: offered once, never forced back after the player
//! removed it ([`super::defaults`]).
//!
//! The memory is one small file in the data folder ([`OFFICIAL_CHOICE_FILE`]), so it moves with the
//! data folder and outlives a reset of the page's own settings (a reinstall that cleared the web
//! view's storage shows the welcome flow again, which then respects it).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::model::Instance;
use crate::error::Result;
use crate::paths::write_durable;

/// The memory's file in the data folder.
pub const OFFICIAL_CHOICE_FILE: &str = "official.json";
/// The official instance's name (the launcher creates it under this name).
pub const OFFICIAL_INSTANCE_NAME: &str = "Projects";
/// A file larger than this is not ours; it reads as "never deleted".
const MAX_FILE_BYTES: u64 = 64 * 1024;

/// What the player decided about the official instance.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficialChoice {
    /// When the player deleted the last official instance; `None` while they have not (or since it
    /// came back).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removed_at: Option<jiff::Timestamp>,
}

/// What a look for the official instance may lead to ([`OfficialChoice::decide`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfficialEnsure {
    /// An official instance is there: it is used as it is.
    Existing,
    /// None is there and one may be made: the player never deleted it, or asked for it now.
    Create,
    /// None is there and the player deleted it: nothing is made; the caller offers
    /// "Resmî paketi kur" instead.
    Offer,
}

impl OfficialChoice {
    /// The file in the data folder `root`.
    pub fn path(root: &Path) -> PathBuf {
        root.join(OFFICIAL_CHOICE_FILE)
    }

    /// The choice stored in the data folder `root`. A missing, oversized or unreadable file reads
    /// as "never deleted" (a first start makes the instance as before).
    pub async fn load(root: &Path) -> Self {
        let path = Self::path(root);
        match tokio::fs::metadata(&path).await {
            Ok(meta) if meta.len() <= MAX_FILE_BYTES => {}
            _ => return Self::default(),
        }
        match tokio::fs::read(&path).await {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|error| {
                tracing::warn!(%error, path = %path.display(), "official choice unreadable; read as never deleted");
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    /// Stores the choice in the data folder `root` (flushed to disk before it replaces the old
    /// file).
    pub async fn save(&self, root: &Path) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(self).unwrap_or_default();
        write_durable(&Self::path(root), &bytes).await
    }

    /// The player deleted the official instance and has not brought it back.
    pub fn removed(&self) -> bool {
        self.removed_at.is_some()
    }

    /// The rule every look for the official instance follows: the one there is used; without one
    /// a new one is made unless the player deleted it, and `asked` (the player's own
    /// "Resmî paketi kur") always makes one.
    pub fn decide(&self, official_there: bool, asked: bool) -> OfficialEnsure {
        if official_there {
            OfficialEnsure::Existing
        } else if asked || !self.removed() {
            OfficialEnsure::Create
        } else {
            OfficialEnsure::Offer
        }
    }

    /// After the player deleted an instance: when an official instance was there before and none
    /// is left (a copy of the pack still counts as one), the removal is remembered. Returns whether
    /// the choice changed (it needs saving).
    pub fn record_deletion(
        &mut self,
        official_before: bool,
        official_after: bool,
        now: jiff::Timestamp,
    ) -> bool {
        if !official_before || official_after || self.removed() {
            return false;
        }
        self.removed_at = Some(now);
        true
    }

    /// An official instance is there again (installed on request, put back from the trash): the
    /// removal is forgotten. Returns whether the choice changed.
    pub fn forget(&mut self) -> bool {
        self.removed_at.take().is_some()
    }
}

/// The official instance among `instances`. With a published pack, `follows` tells which instances
/// follow its family (the first one is it; its copies follow too). Without one (nothing published
/// yet, or no content at hand), the instance named "Projects", else one that follows a pack.
pub fn find_official<'a>(
    instances: &'a [Instance],
    follows: Option<&dyn Fn(&Instance) -> bool>,
) -> Option<&'a Instance> {
    match follows {
        Some(follows) => instances.iter().find(|instance| follows(instance)),
        None => instances
            .iter()
            .find(|instance| {
                instance
                    .name
                    .trim()
                    .eq_ignore_ascii_case(OFFICIAL_INSTANCE_NAME)
            })
            .or_else(|| {
                instances
                    .iter()
                    .find(|instance| instance.managed_pack.is_some())
            }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instance::model::ManagedPack;

    fn instance(id: &str, name: &str, pack: Option<&str>) -> Instance {
        let mut instance = Instance::new(id, name, "26.1.2");
        instance.managed_pack = pack.map(|pack_id| ManagedPack {
            pack_id: pack_id.to_owned(),
            version: String::new(),
        });
        instance
    }

    fn now() -> jiff::Timestamp {
        jiff::Timestamp::from_second(1_792_000_000).unwrap()
    }

    #[tokio::test]
    async fn the_choice_survives_a_restart_and_a_broken_file_reads_as_never_deleted() {
        let dir = tempfile::tempdir().unwrap();
        // No file yet: a first start makes the instance.
        let fresh = OfficialChoice::load(dir.path()).await;
        assert!(!fresh.removed());

        let mut choice = fresh.clone();
        assert!(choice.record_deletion(true, false, now()));
        choice.save(dir.path()).await.unwrap();
        let again = OfficialChoice::load(dir.path()).await;
        assert!(again.removed());
        assert_eq!(again, choice);
        // Written as plain JSON with a readable time (support can see when it happened).
        let text = std::fs::read_to_string(OfficialChoice::path(dir.path())).unwrap();
        assert!(text.contains("removedAt"), "{text}");

        std::fs::write(OfficialChoice::path(dir.path()), b"{not json").unwrap();
        assert!(!OfficialChoice::load(dir.path()).await.removed());
        std::fs::write(
            OfficialChoice::path(dir.path()),
            vec![b' '; (MAX_FILE_BYTES + 1) as usize],
        )
        .unwrap();
        assert!(!OfficialChoice::load(dir.path()).await.removed());

        // Brought back: the file says so.
        let mut back = again;
        assert!(back.forget());
        back.save(dir.path()).await.unwrap();
        assert!(!OfficialChoice::load(dir.path()).await.removed());
        assert!(!back.forget(), "nothing left to forget");
    }

    #[test]
    fn nothing_brings_a_deleted_official_instance_back_unless_the_player_asks() {
        let never = OfficialChoice::default();
        assert_eq!(never.decide(true, false), OfficialEnsure::Existing);
        // Never deleted (a first start, a pack published later): made as before.
        assert_eq!(never.decide(false, false), OfficialEnsure::Create);

        let mut deleted = OfficialChoice::default();
        deleted.record_deletion(true, false, now());
        // Start-up, the pack watcher, the welcome flow, "Katıl", the tray, an import: an offer.
        assert_eq!(deleted.decide(false, false), OfficialEnsure::Offer);
        // "Resmî paketi kur".
        assert_eq!(deleted.decide(false, true), OfficialEnsure::Create);
        // One is there again (put back from the trash): used, whatever the file says.
        assert_eq!(deleted.decide(true, false), OfficialEnsure::Existing);
    }

    #[test]
    fn only_deleting_the_last_official_instance_is_remembered() {
        let mut choice = OfficialChoice::default();
        // A custom instance went; the official one stays.
        assert!(!choice.record_deletion(true, true, now()));
        // A copy of the pack is left: the player still has the official pack.
        assert!(!choice.record_deletion(true, true, now()));
        // A custom instance went while there was no official one.
        assert!(!choice.record_deletion(false, false, now()));
        assert!(!choice.removed());
        // The last one went.
        assert!(choice.record_deletion(true, false, now()));
        assert!(choice.removed());
        // Already remembered: the first time stays.
        let later = now() + jiff::SignedDuration::from_hours(1);
        assert!(!choice.record_deletion(true, false, later));
        assert_eq!(choice.removed_at, Some(now()));
    }

    #[test]
    fn the_official_instance_is_the_one_following_the_pack_or_named_projects() {
        let projects = instance("projects", "Projects", Some("projects-26-1"));
        let copy = instance("projects-kopya", "Projects (kopya)", Some("projects-26-1"));
        let vanilla = instance("vanilla", "Vanilla", None);
        let follows = |i: &Instance| {
            i.managed_pack
                .as_ref()
                .is_some_and(|m| m.pack_id.starts_with("projects"))
        };

        let all = vec![vanilla.clone(), projects.clone(), copy.clone()];
        assert_eq!(
            find_official(&all, Some(&follows)).map(|i| i.id.as_str()),
            Some("projects")
        );
        // The original deleted: its copy is still an official instance.
        let without_original = vec![vanilla.clone(), copy.clone()];
        assert_eq!(
            find_official(&without_original, Some(&follows)).map(|i| i.id.as_str()),
            Some("projects-kopya")
        );
        // Both deleted: none, so the deletion is remembered.
        let only_vanilla = vec![vanilla.clone()];
        assert!(find_official(&only_vanilla, Some(&follows)).is_none());
        let mut choice = OfficialChoice::default();
        assert!(choice.record_deletion(
            find_official(&without_original, Some(&follows)).is_some(),
            find_official(&only_vanilla, Some(&follows)).is_some(),
            now(),
        ));

        // Before any pack (or without content): the instance named "Projects", else one that
        // follows a pack under another name.
        let vanilla_projects = instance("projects", "Projects", None);
        let renamed = instance("benim", "Benim Projects", Some("projects-26-1"));
        assert_eq!(
            find_official(&[vanilla.clone(), vanilla_projects], None).map(|i| i.id.as_str()),
            Some("projects")
        );
        assert_eq!(
            find_official(&[vanilla.clone(), renamed], None).map(|i| i.id.as_str()),
            Some("benim")
        );
        assert!(find_official(&[vanilla], None).is_none());
    }
}

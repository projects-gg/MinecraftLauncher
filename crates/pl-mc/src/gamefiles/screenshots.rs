//! Screenshots (`screenshots/*.png`), and the ones a game session added.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::fsops::{blocking, modified_ms, read_dir_or_empty};
use crate::error::Result;

/// The screenshots present before a game session, to find the ones it added afterwards.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScreenshotSnapshot {
    names: HashSet<String>,
}

impl ScreenshotSnapshot {
    /// Remember the screenshots of `game_dir` now (a missing folder is an empty snapshot).
    pub async fn take(game_dir: &Path) -> Result<Self> {
        Ok(Self::of(&list_screenshots(game_dir).await?))
    }

    pub fn of(shots: &[ScreenshotInfo]) -> Self {
        Self {
            names: shots.iter().map(|s| s.file_name.clone()).collect(),
        }
    }

    /// Screenshots in `now` that were not there when the snapshot was taken, newest first (the
    /// order of [`list_screenshots`] is kept).
    pub fn added(&self, now: Vec<ScreenshotInfo>) -> Vec<ScreenshotInfo> {
        now.into_iter()
            .filter(|shot| !self.names.contains(&shot.file_name))
            .collect()
    }

    /// The screenshots `game_dir` has now that the snapshot does not, newest first.
    pub async fn new_since(&self, game_dir: &Path) -> Result<Vec<ScreenshotInfo>> {
        Ok(self.added(list_screenshots(game_dir).await?))
    }
}

/// The screenshots already announced after a game ended, per game folder, while other games of
/// that folder run (PLAY-AGAIN: two games of one instance share its `screenshots/`, and each one's
/// end would otherwise announce the other's pictures again).
#[derive(Debug, Default)]
pub struct AnnouncedShots {
    folders: HashMap<PathBuf, HashSet<String>>,
}

impl AnnouncedShots {
    pub fn new() -> Self {
        Self::default()
    }

    /// The pictures of `added` (a game of `game_dir` just ended) not announced yet. While
    /// `others_run` (games of the folder still run) they count as announced; once none runs, the
    /// folder is forgotten.
    pub fn fresh(
        &mut self,
        game_dir: &Path,
        added: Vec<ScreenshotInfo>,
        others_run: bool,
    ) -> Vec<ScreenshotInfo> {
        let told = self.folders.entry(game_dir.to_path_buf()).or_default();
        let fresh: Vec<ScreenshotInfo> = added
            .into_iter()
            .filter(|shot| !told.contains(&shot.file_name))
            .collect();
        if others_run {
            told.extend(fresh.iter().map(|shot| shot.file_name.clone()));
        } else {
            self.folders.remove(game_dir);
        }
        fresh
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotInfo {
    pub file_name: String,
    pub path: PathBuf,
    pub size: u64,
    pub modified_ms: Option<i64>,
}

/// PNG screenshots in `<game_dir>/screenshots`, newest first.
pub async fn list_screenshots(game_dir: &Path) -> Result<Vec<ScreenshotInfo>> {
    let dir = game_dir.join("screenshots");
    blocking(move || {
        let mut shots: Vec<ScreenshotInfo> = read_dir_or_empty(&dir)?
            .into_iter()
            .filter_map(|entry| {
                let file_name = entry.file_name().to_string_lossy().into_owned();
                if !file_name.to_ascii_lowercase().ends_with(".png") || file_name.starts_with('.') {
                    return None;
                }
                let meta = entry.metadata().ok().filter(|m| m.is_file())?;
                Some(ScreenshotInfo {
                    path: entry.path(),
                    size: meta.len(),
                    modified_ms: modified_ms(&meta),
                    file_name,
                })
            })
            .collect();
        // Newest first; the game's names (yyyy-MM-dd_HH.mm.ss) break ties.
        shots.sort_by(|a, b| {
            b.modified_ms
                .cmp(&a.modified_ms)
                .then_with(|| b.file_name.cmp(&a.file_name))
        });
        Ok(shots)
    })
    .await
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use super::*;

    fn shot(name: &str) -> ScreenshotInfo {
        ScreenshotInfo {
            file_name: name.to_owned(),
            path: PathBuf::from("game/screenshots").join(name),
            size: 1,
            modified_ms: None,
        }
    }

    fn names(shots: &[ScreenshotInfo]) -> Vec<&str> {
        shots.iter().map(|s| s.file_name.as_str()).collect()
    }

    #[test]
    fn two_games_of_a_folder_announce_each_picture_once() {
        let mut told = AnnouncedShots::new();
        let dir = Path::new("game");
        // The second game ends first (the first still runs): its pictures, and the first's so far.
        let first_end = told.fresh(dir, vec![shot("b.png"), shot("a.png")], true);
        assert_eq!(names(&first_end), ["b.png", "a.png"]);
        // The first ends after it: only what came since (the folder is forgotten after).
        let second_end = told.fresh(
            dir,
            vec![shot("c.png"), shot("b.png"), shot("a.png")],
            false,
        );
        assert_eq!(names(&second_end), ["c.png"]);
        // A later game alone announces everything new again.
        assert_eq!(
            names(&told.fresh(dir, vec![shot("a.png")], false)),
            ["a.png"]
        );
        // Other folders are their own.
        told.fresh(dir, vec![shot("x.png")], true);
        assert_eq!(
            names(&told.fresh(Path::new("other"), vec![shot("x.png")], false)),
            ["x.png"]
        );
    }

    #[tokio::test]
    async fn newest_first_png_only() {
        let dir = tempfile::tempdir().unwrap();
        let shots = dir.path().join("screenshots");
        std::fs::create_dir_all(&shots).unwrap();
        let now = SystemTime::now();
        for (name, age_secs) in [("old.png", 300), ("new.PNG", 10), ("mid.png", 100)] {
            let path = shots.join(name);
            std::fs::write(&path, b"png").unwrap();
            let file = std::fs::File::options().write(true).open(&path).unwrap();
            file.set_modified(now - Duration::from_secs(age_secs))
                .unwrap();
        }
        std::fs::write(shots.join("notes.txt"), b"x").unwrap();
        let list = list_screenshots(dir.path()).await.unwrap();
        let names: Vec<&str> = list.iter().map(|s| s.file_name.as_str()).collect();
        assert_eq!(names, vec!["new.PNG", "mid.png", "old.png"]);
        assert_eq!(list[0].size, 3);
        assert!(
            list_screenshots(&dir.path().join("none"))
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn screenshots_a_session_added() {
        let dir = tempfile::tempdir().unwrap();
        // No folder yet: the game creates it with the first screenshot.
        let before = ScreenshotSnapshot::take(dir.path()).await.unwrap();
        let shots = dir.path().join("screenshots");
        std::fs::create_dir_all(&shots).unwrap();
        std::fs::write(shots.join("2026-10-16_12.00.00.png"), b"a").unwrap();
        let added = before.new_since(dir.path()).await.unwrap();
        assert_eq!(added.len(), 1);

        // The next session only reports its own pictures; deleting old ones changes nothing.
        let before = ScreenshotSnapshot::take(dir.path()).await.unwrap();
        let now = SystemTime::now();
        for (name, age_secs) in [
            ("2026-10-16_20.00.00.png", 30),
            ("2026-10-16_20.05.00.png", 5),
            ("2026-10-16_20.05.00_1.png", 5),
        ] {
            let path = shots.join(name);
            std::fs::write(&path, b"png").unwrap();
            let file = std::fs::File::options().write(true).open(&path).unwrap();
            file.set_modified(now - Duration::from_secs(age_secs))
                .unwrap();
        }
        std::fs::remove_file(shots.join("2026-10-16_12.00.00.png")).unwrap();
        std::fs::write(shots.join("notes.txt"), b"x").unwrap();
        let added = before.new_since(dir.path()).await.unwrap();
        let names: Vec<&str> = added.iter().map(|s| s.file_name.as_str()).collect();
        assert_eq!(
            names,
            [
                "2026-10-16_20.05.00_1.png",
                "2026-10-16_20.05.00.png",
                "2026-10-16_20.00.00.png"
            ]
        );
        let unchanged = ScreenshotSnapshot::take(dir.path()).await.unwrap();
        assert!(unchanged.new_since(dir.path()).await.unwrap().is_empty());
    }
}

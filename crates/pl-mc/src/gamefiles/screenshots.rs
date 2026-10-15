//! Screenshots (`screenshots/*.png`).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::fsops::{blocking, modified_ms, read_dir_or_empty};
use crate::error::Result;

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
}

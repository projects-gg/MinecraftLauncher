//! Small previews of screenshots for the gallery, cached in `cache/thumbs/<instance>/`.
//!
//! A thumbnail's file name is derived from its picture's name, size and modification time
//! ([`thumbnail_name`]), so a changed picture simply gets a new one; [`prune_thumbnails`] removes
//! those no picture needs any more. Thumbnails are made on demand, two at a time (decoding a big
//! PNG takes a core and some memory), and written with a plain atomic write: they are a cache.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use tokio::sync::Semaphore;

use super::fsops::{blocking, read_dir_or_empty};
use super::screenshots::ScreenshotInfo;
use crate::error::{Error, Result};

/// Longest side of a thumbnail in pixels (sharp on a 2× display at the gallery's tile size).
pub const THUMB_SIZE: u32 = 512;
const JPEG_QUALITY: u8 = 82;
/// Pictures larger than this are not decoded for a thumbnail (the gallery shows the original).
const MAX_SOURCE_BYTES: u64 = 64 * 1024 * 1024;
/// Part of every name: changing the size or the format makes new thumbnails.
const VERSION: &[u8] = b"thumb-v1";

static WORKERS: LazyLock<Semaphore> = LazyLock::new(|| Semaphore::new(2));

/// Folder of an instance's thumbnails below the data folder's `cache/`.
pub fn thumbnails_dir(cache_dir: &Path, instance_id: &str) -> PathBuf {
    cache_dir
        .join("thumbs")
        .join(super::logs::game_log_id(instance_id))
}

/// File name of the thumbnail of `shot`: FNV-1a over its name, size and modification time.
pub fn thumbnail_name(shot: &ScreenshotInfo) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut feed = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    };
    feed(VERSION);
    feed(shot.file_name.as_bytes());
    feed(&[0]);
    feed(&shot.size.to_le_bytes());
    feed(&shot.modified_ms.unwrap_or(0).to_le_bytes());
    format!("{hash:016x}.jpg")
}

/// The thumbnail of `shot` in `dir`, if it was made already.
pub fn existing_thumbnail(dir: &Path, shot: &ScreenshotInfo) -> Option<PathBuf> {
    let path = dir.join(thumbnail_name(shot));
    path.is_file().then_some(path)
}

/// The thumbnail of `shot` in `dir`, made now when missing.
pub async fn ensure_thumbnail(dir: &Path, shot: &ScreenshotInfo) -> Result<PathBuf> {
    let path = dir.join(thumbnail_name(shot));
    if path.is_file() {
        return Ok(path);
    }
    let _permit = WORKERS.acquire().await.map_err(|_| Error::Cancelled)?;
    let source = shot.path.clone();
    let target = path.clone();
    blocking(move || {
        // Another request may have made it while this one waited.
        if target.is_file() {
            return Ok(());
        }
        let bytes = make_thumbnail(&source)?;
        crate::paths::write_atomic_blocking(&target, &bytes)
    })
    .await?;
    Ok(path)
}

/// JPEG bytes of the picture at `source`, scaled to fit [`THUMB_SIZE`] × [`THUMB_SIZE`].
pub fn make_thumbnail(source: &Path) -> Result<Vec<u8>> {
    let size = std::fs::metadata(source)
        .map_err(|e| Error::io(source, e))?
        .len();
    if size > MAX_SOURCE_BYTES {
        return Err(Error::Invalid(format!(
            "{} is too large for a thumbnail ({size} bytes)",
            source.display()
        )));
    }
    let picture = image::ImageReader::open(source)
        .map_err(|e| Error::io(source, e))?
        .with_guessed_format()
        .map_err(|e| Error::io(source, e))?
        .decode()
        .map_err(|e| Error::Invalid(format!("cannot decode {}: {e}", source.display())))?;
    let small = if picture.width() <= THUMB_SIZE && picture.height() <= THUMB_SIZE {
        picture.into_rgb8()
    } else {
        picture.thumbnail(THUMB_SIZE, THUMB_SIZE).into_rgb8()
    };
    let mut out = Vec::with_capacity(64 * 1024);
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY)
        .encode_image(&small)
        .map_err(|e| Error::Invalid(format!("cannot encode a thumbnail: {e}")))?;
    Ok(out)
}

/// Removes the files in `dir` that none of `shots` needs (pictures that changed or are gone).
/// Returns how many went.
pub async fn prune_thumbnails(dir: &Path, shots: &[ScreenshotInfo]) -> Result<u64> {
    let keep: HashSet<String> = shots.iter().map(thumbnail_name).collect();
    let dir = dir.to_path_buf();
    blocking(move || {
        let mut removed = 0;
        for entry in read_dir_or_empty(&dir)? {
            let name = entry.file_name().to_string_lossy().into_owned();
            if keep.contains(&name) || !entry.file_type().is_ok_and(|t| t.is_file()) {
                continue;
            }
            if std::fs::remove_file(entry.path()).is_ok() {
                removed += 1;
            }
        }
        Ok(removed)
    })
    .await
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use super::*;
    use crate::gamefiles::list_screenshots;

    fn write_png(path: &Path, width: u32, height: u32) {
        let picture = image::RgbImage::from_fn(width, height, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, 128])
        });
        picture.save(path).unwrap();
    }

    #[tokio::test]
    async fn makes_reuses_and_prunes_thumbnails() {
        let game = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        let shots_dir = game.path().join("screenshots");
        std::fs::create_dir_all(&shots_dir).unwrap();
        write_png(&shots_dir.join("2026-10-16_20.00.00.png"), 1920, 1080);
        write_png(&shots_dir.join("tall.png"), 300, 900);
        let dir = thumbnails_dir(cache.path(), "projects");
        assert!(dir.ends_with("thumbs/projects"));

        let shots = list_screenshots(game.path()).await.unwrap();
        let wide = shots
            .iter()
            .find(|s| s.file_name.starts_with("2026"))
            .unwrap();
        assert!(existing_thumbnail(&dir, wide).is_none());
        let path = ensure_thumbnail(&dir, wide).await.unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.starts_with(&[0xFF, 0xD8]), "a JPEG");
        let thumb = image::load_from_memory(&bytes).unwrap();
        assert_eq!((thumb.width(), thumb.height()), (512, 288));
        assert_eq!(existing_thumbnail(&dir, wide), Some(path.clone()));

        // Made once: asking again does not write it again.
        let written = std::fs::metadata(&path).unwrap().modified().unwrap();
        assert_eq!(ensure_thumbnail(&dir, wide).await.unwrap(), path);
        assert_eq!(
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            written
        );

        let tall = shots.iter().find(|s| s.file_name == "tall.png").unwrap();
        let tall_thumb = image::open(ensure_thumbnail(&dir, tall).await.unwrap()).unwrap();
        assert_eq!((tall_thumb.width(), tall_thumb.height()), (171, 512));

        // The picture changed: it gets a new name, and the old thumbnail is pruned.
        write_png(&shots_dir.join("2026-10-16_20.00.00.png"), 640, 360);
        let file = std::fs::File::options()
            .write(true)
            .open(shots_dir.join("2026-10-16_20.00.00.png"))
            .unwrap();
        file.set_modified(SystemTime::now() + Duration::from_secs(5))
            .unwrap();
        let shots = list_screenshots(game.path()).await.unwrap();
        let changed = shots
            .iter()
            .find(|s| s.file_name.starts_with("2026"))
            .unwrap();
        assert_ne!(thumbnail_name(changed), thumbnail_name(wide));
        assert!(existing_thumbnail(&dir, changed).is_none());
        std::fs::write(dir.join("stray.tmp"), b"x").unwrap();
        assert_eq!(prune_thumbnails(&dir, &shots).await.unwrap(), 2);
        assert!(!path.exists());
        assert!(existing_thumbnail(&dir, tall).is_some(), "still needed");
        assert_eq!(
            prune_thumbnails(&cache.path().join("none"), &shots)
                .await
                .unwrap(),
            0
        );
    }

    #[tokio::test]
    async fn unreadable_pictures_have_no_thumbnail() {
        let game = tempfile::tempdir().unwrap();
        let shots_dir = game.path().join("screenshots");
        std::fs::create_dir_all(&shots_dir).unwrap();
        std::fs::write(shots_dir.join("broken.png"), b"\x89PNG\r\n\x1a\nnot really").unwrap();
        let shots = list_screenshots(game.path()).await.unwrap();
        let dir = game.path().join("thumbs");
        assert!(ensure_thumbnail(&dir, &shots[0]).await.is_err());
        assert!(
            read_dir_or_empty(&dir).unwrap().is_empty(),
            "nothing written"
        );
    }
}

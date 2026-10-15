use std::path::PathBuf;

use pl_net::{Checksum, DownloadItem};
use serde::{Deserialize, Serialize};
use url::Url;

use super::is_http_url;
use crate::error::{Error, Result};
use crate::fsutil::normalize_hex;
use crate::path::check_mirror_path;

/// A JSON document referenced by the root index, verified by SHA-256 after download.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DocRef {
    /// Path relative to the mirror base, e.g. `config/launcher.<sha256>.json`.
    pub path: String,
    /// Lowercase hex SHA-256 of the file.
    pub sha256: String,
    /// Size in bytes.
    pub size: u64,
}

impl DocRef {
    /// Checks the path alphabet and the digest format.
    pub fn validate(&self) -> Result<()> {
        check_mirror_path(&self.path)?;
        normalize_hex(&self.sha256, 64)
            .map(|_| ())
            .ok_or_else(|| Error::Invalid(format!("bad sha256 for {}", self.path)))
    }
}

/// A downloadable binary (AFK client build, hosted file). Either a mirror-relative `path`, a
/// list of absolute `urls`, or both (mirror copies are tried first).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FileRef {
    /// Path relative to every mirror base.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Absolute alternate URLs (e.g. GitHub release assets), tried after the mirror copies.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub urls: Vec<String>,
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha1: Option<String>,
    pub size: u64,
}

impl FileRef {
    /// Download candidates in order: the mirror copy on each mirror, then the alternates.
    /// Invalid paths and non-HTTP URLs are skipped.
    pub fn candidate_urls(&self, mirrors: &[Url]) -> Vec<String> {
        let mut urls = Vec::new();
        if let Some(path) = &self.path
            && check_mirror_path(path).is_ok()
        {
            urls.extend(
                mirrors
                    .iter()
                    .filter_map(|base| base.join(path).ok())
                    .map(String::from),
            );
        }
        urls.extend(self.urls.iter().filter(|url| is_http_url(url)).cloned());
        urls
    }

    /// A SHA-256 verified [`DownloadItem`] for `dest`.
    pub fn download_item(&self, mirrors: &[Url], dest: impl Into<PathBuf>) -> Result<DownloadItem> {
        let sha256 = normalize_hex(&self.sha256, 64)
            .ok_or_else(|| Error::Invalid("file reference without a valid sha256".into()))?;
        let urls = self.candidate_urls(mirrors);
        if urls.is_empty() {
            return Err(Error::Invalid("file reference without a usable URL".into()));
        }
        Ok(DownloadItem::with_urls(urls, dest)
            .checksum(Some(Checksum::sha256(sha256)))
            .size(Some(self.size)))
    }
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "camelCase")]
pub enum MediaKind {
    #[default]
    Image,
    Video,
    /// A kind added by a newer Studio; show the poster or nothing.
    #[serde(other)]
    Unknown,
}

/// An image or video stored at `media/<sha256>.<ext>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MediaRef {
    /// Path relative to the mirror base, e.g. `media/<sha256>.webp`.
    pub path: String,
    pub sha256: String,
    pub size: u64,
    #[serde(default)]
    pub kind: MediaKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    /// BlurHash placeholder shown while the file loads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blurhash: Option<String>,
    /// Still image for videos (first frame / fallback in performance mode).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poster: Option<Box<MediaRef>>,
    /// Accessible description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alt: Option<String>,
}

impl MediaRef {
    /// Lowercase file extension taken from `path` (alphanumeric, at most 8 characters), or
    /// `bin` when there is none.
    pub fn extension(&self) -> String {
        let name = self.path.rsplit('/').next().unwrap_or_default();
        match name.rsplit_once('.') {
            Some((stem, ext))
                if !stem.is_empty()
                    && (1..=8).contains(&ext.len())
                    && ext.bytes().all(|b| b.is_ascii_alphanumeric()) =>
            {
                ext.to_ascii_lowercase()
            }
            _ => "bin".to_owned(),
        }
    }

    /// Name of the cached copy: `<sha256>.<ext>`.
    pub fn cache_file_name(&self) -> String {
        format!(
            "{}.{}",
            self.sha256.trim().to_ascii_lowercase(),
            self.extension()
        )
    }

    /// This media followed by its poster chain (bounded, posters of posters are unusual).
    pub fn with_posters(&self) -> impl Iterator<Item = &MediaRef> {
        std::iter::successors(Some(self), |media| media.poster.as_deref()).take(4)
    }

    /// Checks the path alphabet and the digest format.
    pub fn validate(&self) -> Result<()> {
        check_mirror_path(&self.path)?;
        normalize_hex(&self.sha256, 64)
            .map(|_| ())
            .ok_or_else(|| Error::Invalid(format!("bad sha256 for {}", self.path)))
    }
}

/// Call to action: opens `url` in the system browser or the news article `newsId`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Cta {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub news_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn media(path: &str) -> MediaRef {
        MediaRef {
            path: path.into(),
            sha256: "AB".repeat(32),
            size: 1,
            kind: MediaKind::Image,
            width: None,
            height: None,
            blurhash: None,
            poster: None,
            alt: None,
        }
    }

    #[test]
    fn media_extension_and_cache_name() {
        assert_eq!(media("media/x.WEBP").extension(), "webp");
        assert_eq!(media("media/x").extension(), "bin");
        assert_eq!(media("media/.webp").extension(), "bin");
        assert_eq!(media("media/x.toolongext").extension(), "bin");
        assert_eq!(media("media/x.we-p").extension(), "bin");
        assert_eq!(
            media("media/x.mp4").cache_file_name(),
            format!("{}.mp4", "ab".repeat(32))
        );
    }

    #[test]
    fn poster_chain_is_walked() {
        let mut video = media("media/v.mp4");
        video.kind = MediaKind::Video;
        video.poster = Some(Box::new(media("media/p.webp")));
        let all: Vec<_> = video.with_posters().map(|m| m.path.as_str()).collect();
        assert_eq!(all, ["media/v.mp4", "media/p.webp"]);
    }

    #[test]
    fn unknown_media_kind_is_tolerated() {
        let parsed: MediaKind = serde_json::from_str("\"hologram\"").unwrap();
        assert_eq!(parsed, MediaKind::Unknown);
    }

    #[test]
    fn file_ref_candidates() {
        let mirrors = [
            Url::parse("https://cdn.example/v1/").unwrap(),
            Url::parse("https://mirror.example/content/v1/").unwrap(),
        ];
        let file = FileRef {
            path: Some("afk/1.0.0/windows-x86_64.zip".into()),
            urls: vec!["https://github.com/x/y.zip".into(), "ftp://nope".into()],
            sha256: "00".repeat(32),
            sha1: None,
            size: 10,
        };
        assert_eq!(
            file.candidate_urls(&mirrors),
            [
                "https://cdn.example/v1/afk/1.0.0/windows-x86_64.zip",
                "https://mirror.example/content/v1/afk/1.0.0/windows-x86_64.zip",
                "https://github.com/x/y.zip",
            ]
        );
        let item = file.download_item(&mirrors, "x.zip").unwrap();
        assert_eq!(item.urls.len(), 3);
        assert_eq!(item.size, Some(10));

        let unsafe_path = FileRef {
            path: Some("../../etc/passwd".into()),
            urls: Vec::new(),
            ..file
        };
        assert!(unsafe_path.candidate_urls(&mirrors).is_empty());
        assert!(unsafe_path.download_item(&mirrors, "x").is_err());
    }
}

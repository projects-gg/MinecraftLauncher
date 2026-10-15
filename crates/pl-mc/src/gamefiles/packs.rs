//! Resource packs (`resourcepacks/`) and shader packs (`shaderpacks/`).

use std::fs::DirEntry;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::fsops::{
    blocking, dir_size, modified_ms, read_dir_or_empty, read_png_file, read_text_file,
};
use crate::error::Result;
use crate::mods::lenient_json;
use crate::server::chat;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ResourcePackInfo {
    pub file_name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
    pub modified_ms: Option<i64>,
    /// `pack.mcmeta` description as plain text (formatting codes removed).
    pub description: Option<String>,
    pub pack_format: Option<i64>,
    /// `pack.png` (at most 256 KB).
    pub icon_png: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ShaderPackInfo {
    pub file_name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
    pub modified_ms: Option<i64>,
}

fn is_hidden(name: &str) -> bool {
    name.starts_with('.')
}

fn has_zip_extension(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".zip")
}

/// `(description, pack_format)` from `pack.mcmeta` text.
fn parse_mcmeta(text: &str) -> (Option<String>, Option<i64>) {
    let Some(value) = lenient_json::parse(text) else {
        return (None, None);
    };
    let Some(pack) = value.get("pack") else {
        return (None, None);
    };
    let description = pack
        .get("description")
        .map(|d| chat::to_plain(d).trim().to_owned())
        .filter(|d| !d.is_empty());
    let pack_format = pack.get("pack_format").and_then(Value::as_i64);
    (description, pack_format)
}

fn read_resource_pack(entry: &DirEntry) -> Option<ResourcePackInfo> {
    let file_name = entry.file_name().to_string_lossy().into_owned();
    if is_hidden(&file_name) {
        return None;
    }
    let path = entry.path();
    let meta = entry.metadata().ok()?;
    if meta.is_dir() {
        let mcmeta = read_text_file(&path.join("pack.mcmeta"))?;
        let (description, pack_format) = parse_mcmeta(&mcmeta);
        return Some(ResourcePackInfo {
            size: dir_size(&path),
            modified_ms: modified_ms(&meta),
            icon_png: read_png_file(&path.join("pack.png")),
            file_name,
            path,
            is_dir: true,
            description,
            pack_format,
        });
    }
    if !meta.is_file() || !has_zip_extension(&file_name) {
        return None;
    }
    let mut info = ResourcePackInfo {
        file_name,
        path: path.clone(),
        is_dir: false,
        size: meta.len(),
        modified_ms: modified_ms(&meta),
        description: None,
        pack_format: None,
        icon_png: None,
    };
    // Broken zips are still listed (so they can be removed), just without details.
    let read = std::panic::catch_unwind(|| {
        let file = std::fs::File::open(&path).ok()?;
        let mut archive = zip::ZipArchive::new(BufReader::new(file)).ok()?;
        let mcmeta = crate::mods::read_zip_text(&mut archive, "pack.mcmeta");
        let icon = crate::mods::read_zip_png(&mut archive, "pack.png");
        Some((mcmeta, icon))
    });
    if let Ok(Some((mcmeta, icon))) = read {
        if let Some(text) = mcmeta {
            (info.description, info.pack_format) = parse_mcmeta(&text);
        }
        info.icon_png = icon;
    }
    Some(info)
}

fn read_shader_pack(entry: &DirEntry) -> Option<ShaderPackInfo> {
    let file_name = entry.file_name().to_string_lossy().into_owned();
    if is_hidden(&file_name) {
        return None;
    }
    let path = entry.path();
    let meta = entry.metadata().ok()?;
    let (is_dir, size) = if meta.is_dir() {
        (true, dir_size(&path))
    } else if meta.is_file() && has_zip_extension(&file_name) {
        (false, meta.len())
    } else {
        return None; // e.g. Iris' `<pack>.zip.txt` settings files
    };
    Some(ShaderPackInfo {
        modified_ms: modified_ms(&meta),
        file_name,
        path,
        is_dir,
        size,
    })
}

async fn list_parallel<T, F>(dir: PathBuf, read: F) -> Result<Vec<T>>
where
    T: Send + 'static,
    F: Fn(&DirEntry) -> Option<T> + Send + Sync + Copy + 'static,
{
    let entries = blocking(move || read_dir_or_empty(&dir)).await?;
    let parallelism = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(2, 8);
    let items = futures::stream::iter(entries)
        .map(|entry| blocking(move || Ok(read(&entry))))
        .buffer_unordered(parallelism)
        .filter_map(|result| async move { result.ok().flatten() })
        .collect()
        .await;
    Ok(items)
}

/// Resource packs in `<game_dir>/resourcepacks`: zip files and folders with a `pack.mcmeta`.
/// Sorted by file name.
pub async fn list_resource_packs(game_dir: &Path) -> Result<Vec<ResourcePackInfo>> {
    let mut packs = list_parallel(game_dir.join("resourcepacks"), read_resource_pack).await?;
    packs.sort_by_cached_key(|p| p.file_name.to_lowercase());
    Ok(packs)
}

/// Shader packs in `<game_dir>/shaderpacks`: zip files and folders. Sorted by file name.
pub async fn list_shader_packs(game_dir: &Path) -> Result<Vec<ShaderPackInfo>> {
    let mut packs = list_parallel(game_dir.join("shaderpacks"), read_shader_pack).await?;
    packs.sort_by_cached_key(|p| p.file_name.to_lowercase());
    Ok(packs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mods::test_support::{png, zip_bytes};

    #[test]
    fn mcmeta_description_forms() {
        let (d, f) = parse_mcmeta(r#"{"pack":{"pack_format":15,"description":"§6Gold §rpack"}}"#);
        assert_eq!(d.as_deref(), Some("Gold pack"));
        assert_eq!(f, Some(15));
        let (d, _) = parse_mcmeta(
            r#"{"pack":{"pack_format":34,"description":[{"text":"A","color":"red"},{"text":" B"}]}}"#,
        );
        assert_eq!(d.as_deref(), Some("A B"));
        let (d, f) =
            parse_mcmeta(r#"{"pack":{"description":{"translate":"x","fallback":"Fallback"},}}"#);
        assert_eq!(d.as_deref(), Some("Fallback"));
        assert_eq!(f, None);
        assert_eq!(parse_mcmeta("garbage"), (None, None));
    }

    #[tokio::test]
    async fn lists_zip_and_folder_packs() {
        let dir = tempfile::tempdir().unwrap();
        let rp = dir.path().join("resourcepacks");
        std::fs::create_dir_all(rp.join("Folder Pack")).unwrap();
        std::fs::write(
            rp.join("Folder Pack").join("pack.mcmeta"),
            br#"{"pack":{"pack_format":15,"description":"From a folder"}}"#,
        )
        .unwrap();
        std::fs::create_dir_all(rp.join("not a pack")).unwrap();
        let icon = png(40);
        std::fs::write(
            rp.join("Zipped.zip"),
            zip_bytes(&[
                (
                    "pack.mcmeta",
                    br#"{"pack":{"pack_format":18,"description":"Zipped!"}}"#,
                ),
                ("pack.png", &icon),
            ]),
        )
        .unwrap();
        std::fs::write(rp.join("broken.zip"), b"nope").unwrap();
        std::fs::write(rp.join("readme.txt"), b"x").unwrap();

        let packs = list_resource_packs(dir.path()).await.unwrap();
        let names: Vec<&str> = packs.iter().map(|p| p.file_name.as_str()).collect();
        assert_eq!(names, vec!["broken.zip", "Folder Pack", "Zipped.zip"]);
        assert_eq!(packs[0].description, None);
        assert!(packs[1].is_dir);
        assert_eq!(packs[1].description.as_deref(), Some("From a folder"));
        assert_eq!(packs[2].pack_format, Some(18));
        assert_eq!(packs[2].icon_png.as_deref(), Some(icon.as_slice()));

        let sp = dir.path().join("shaderpacks");
        std::fs::create_dir_all(sp.join("BSL_v8")).unwrap();
        std::fs::write(sp.join("BSL_v8").join("a.txt"), b"12345").unwrap();
        std::fs::write(sp.join("Complementary.zip"), b"zip").unwrap();
        std::fs::write(sp.join("Complementary.zip.txt"), b"settings").unwrap();
        let shaders = list_shader_packs(dir.path()).await.unwrap();
        assert_eq!(shaders.len(), 2);
        assert_eq!(shaders[0].file_name, "BSL_v8");
        assert_eq!(shaders[0].size, 5);
        assert_eq!(shaders[1].size, 3);

        assert!(
            list_shader_packs(&dir.path().join("missing"))
                .await
                .unwrap()
                .is_empty()
        );
    }
}

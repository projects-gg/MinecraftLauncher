//! Modpacks: Modrinth `.mrpack` read/write, Prism Launcher / MultiMC and CurseForge zip import.
//!
//! Every archive path goes through [`safe_path`]: nothing is written outside the target game
//! directory, and names that cannot exist on Windows are refused on every platform.

mod curseforge;
mod extract;
mod mrpack;
mod preview;
mod prism;
pub mod safe_path;
mod update;

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use pl_net::CancellationToken;
use serde::{Deserialize, Serialize};
use zip::ZipArchive;

pub use curseforge::{
    CurseForgeFileRef, CurseForgeManifest, CurseForgeModLink, import_curseforge,
    parse_curseforge_manifest,
};
pub use mrpack::{
    MRPACK_INDEX, MrEnv, MrEnvSupport, MrFile, MrHashes, MrIndex, MrpackExport, MrpackHostedFiles,
    MrpackWriteSummary, identify_hosted_files, import_mrpack, mrpack_download_items,
    read_mrpack_index, write_mrpack,
};
pub use preview::{PackOptionalFile, PackPreview, preview_pack};
pub use prism::import_prism;
pub use update::{
    HashCache, KeptReason, MODPACK_STATE_FILE, ModpackState, OnDisk, PackContents, PackFile,
    PackFileSource, UpdatePlan, disk_state, pack_update_candidate, plan_update, read_pack_contents,
    remove_dropped, update_download_items, write_bundled,
};

use crate::error::{Error, Result};
use crate::gamefiles::fsops::blocking;
use crate::loaders::LoaderKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ModpackFormat {
    /// Modrinth `.mrpack`.
    Mrpack,
    /// Prism Launcher / MultiMC instance export.
    Prism,
    /// CurseForge modpack zip.
    CurseForge,
}

/// Files an import could not put in place by itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ImportPendingFiles {
    None,
    /// Modrinth-hosted files from the index; download with [`mrpack_download_items`].
    Modrinth {
        files: Vec<MrFile>,
    },
    /// CurseForge mod files. They can only be downloaded through the CurseForge API with an API
    /// key, which this launcher does not have: show them to the user (`links` come from the
    /// pack's `modlist.html`).
    CurseForgeNeedsApiKey {
        files: Vec<CurseForgeFileRef>,
        links: Vec<CurseForgeModLink>,
    },
}

/// What an imported pack describes, after its bundled files were extracted into the game
/// directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportedInstance {
    pub format: ModpackFormat,
    pub name: String,
    pub pack_version: Option<String>,
    pub summary: Option<String>,
    pub author: Option<String>,
    pub mc_version: String,
    pub loader: LoaderKind,
    pub loader_version: Option<String>,
    pub icon_png: Option<Vec<u8>>,
    pub notes: Option<String>,
    pub jvm_args: Option<String>,
    pub min_memory_mb: Option<u32>,
    pub max_memory_mb: Option<u32>,
    pub total_played_secs: Option<u64>,
    /// Files written into the game directory.
    pub extracted_files: u64,
    pub extracted_bytes: u64,
    /// Archive entries left out (unsafe paths, links).
    pub skipped_entries: Vec<String>,
    /// Parts of the source instance this launcher cannot reproduce (Prism jar mods, custom
    /// components, launch commands).
    pub unsupported: Vec<String>,
    pub pending: ImportPendingFiles,
}

impl ImportedInstance {
    pub(crate) fn new(format: ModpackFormat, name: String, mc_version: String) -> Self {
        Self {
            format,
            name,
            pack_version: None,
            summary: None,
            author: None,
            mc_version,
            loader: LoaderKind::Vanilla,
            loader_version: None,
            icon_png: None,
            notes: None,
            jvm_args: None,
            min_memory_mb: None,
            max_memory_mb: None,
            total_played_secs: None,
            extracted_files: 0,
            extracted_bytes: 0,
            skipped_entries: Vec::new(),
            unsupported: Vec::new(),
            pending: ImportPendingFiles::None,
        }
    }

    pub(crate) fn apply_report(&mut self, report: extract::ExtractReport) {
        self.extracted_files += report.files;
        self.extracted_bytes += report.bytes;
        self.skipped_entries.extend(report.skipped);
    }
}

/// The pack format of an open archive (also used by drag-and-drop to recognize packs).
pub(crate) fn detect_in<R: std::io::Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
) -> Option<ModpackFormat> {
    let names = extract::entry_names(archive);
    if names.iter().any(|n| n == MRPACK_INDEX) {
        return Some(ModpackFormat::Mrpack);
    }
    if prism::find_root(&names, "instance.cfg").is_some() {
        return Some(ModpackFormat::Prism);
    }
    let root = prism::find_root(&names, "manifest.json")?;
    let text = extract::read_entry_string(archive, &format!("{root}manifest.json"), 16 << 20)?;
    parse_curseforge_manifest(&text)
        .is_ok()
        .then_some(ModpackFormat::CurseForge)
}

/// Recognize a pack archive by its contents (not its extension). `None` = not a known pack.
pub async fn detect_pack_format(path: &Path) -> Result<Option<ModpackFormat>> {
    let path = path.to_path_buf();
    blocking(move || {
        let file = File::open(&path).map_err(|e| Error::io(&path, e))?;
        match ZipArchive::new(BufReader::new(file)) {
            Ok(mut archive) => Ok(detect_in(&mut archive)),
            Err(_) => Ok(None),
        }
    })
    .await
}

/// Import any supported pack archive into `game_dir` (format detected from its contents).
pub async fn import_pack(
    path: &Path,
    game_dir: &Path,
    cancel: CancellationToken,
) -> Result<ImportedInstance> {
    match detect_pack_format(path).await? {
        Some(ModpackFormat::Mrpack) => import_mrpack(path, game_dir, cancel).await,
        Some(ModpackFormat::Prism) => import_prism(path, game_dir, cancel).await,
        Some(ModpackFormat::CurseForge) => import_curseforge(path, game_dir, cancel).await,
        None => Err(Error::Unsupported(format!(
            "{} is not a Modrinth, Prism/MultiMC or CurseForge pack",
            path.display()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mods::test_support::zip_bytes;

    #[tokio::test]
    async fn detection_by_content() {
        let dir = tempfile::tempdir().unwrap();
        let cases: [(&str, Vec<u8>, Option<ModpackFormat>); 5] = [
            (
                "a.zip",
                zip_bytes(&[(MRPACK_INDEX, b"{}")]),
                Some(ModpackFormat::Mrpack),
            ),
            (
                "b.zip",
                zip_bytes(&[
                    ("Inst/instance.cfg", b"name=x"),
                    ("Inst/mmc-pack.json", b"{}"),
                ]),
                Some(ModpackFormat::Prism),
            ),
            (
                "c.zip",
                zip_bytes(&[(
                    "manifest.json",
                    br#"{"minecraft":{"version":"1.20.1"},"manifestType":"minecraftModpack"}"#,
                )]),
                Some(ModpackFormat::CurseForge),
            ),
            (
                "d.zip",
                zip_bytes(&[("manifest.json", br#"{"name":"not a pack"}"#)]),
                None,
            ),
            ("e.zip", b"not a zip".to_vec(), None),
        ];
        for (name, bytes, expected) in cases {
            let path = dir.path().join(name);
            std::fs::write(&path, bytes).unwrap();
            assert_eq!(detect_pack_format(&path).await.unwrap(), expected, "{name}");
        }
        let unknown = import_pack(
            &dir.path().join("e.zip"),
            dir.path(),
            CancellationToken::new(),
        )
        .await;
        assert!(matches!(unknown, Err(Error::Unsupported(_))));
    }

    /// Tauri async commands need `Send` futures; check the public async APIs of the
    /// game-file modules at compile time.
    #[test]
    fn public_futures_are_send() {
        fn send<T: Send>(_: &T) {}
        let p = Path::new("x");
        let token = CancellationToken::new;
        send(&crate::mods::list_mods(p, true));
        send(&crate::mods::read_mod(p, true));
        send(&crate::mods::set_enabled(p, true));
        send(&crate::mods::remove_mod(p, false));
        send(&crate::gamefiles::list_resource_packs(p));
        send(&crate::gamefiles::list_shader_packs(p));
        send(&crate::gamefiles::list_worlds(p));
        send(&crate::gamefiles::list_screenshots(p));
        send(&crate::gamefiles::backup_world(p, p, token(), None));
        send(&crate::gamefiles::delete_world(p, false));
        send(&crate::gamefiles::import_into(
            p,
            crate::gamefiles::ImportKind::Mod,
            p,
        ));
        send(&detect_pack_format(p));
        send(&import_pack(p, p, token()));
        send(&read_mrpack_index(p));
        let export = MrpackExport {
            name: String::new(),
            version_id: String::new(),
            summary: None,
            mc_version: String::new(),
            loader: LoaderKind::Vanilla,
            loader_version: None,
            files: Vec::new(),
            overrides: Vec::new(),
        };
        send(&write_mrpack(p, &export, p, token()));
        let http = pl_net::Http::new(pl_net::HttpConfig::new("send-check")).unwrap();
        let client = crate::modrinth::ModrinthClient::new(http);
        send(&identify_hosted_files(&client, p, &[]));
    }

    #[test]
    fn pending_files_serialize_with_a_kind_tag() {
        let pending = ImportPendingFiles::CurseForgeNeedsApiKey {
            files: vec![CurseForgeFileRef {
                project_id: 1,
                file_id: 2,
                required: true,
            }],
            links: Vec::new(),
        };
        let json = serde_json::to_value(&pending).unwrap();
        assert_eq!(json["kind"], "curseForgeNeedsApiKey");
        assert_eq!(json["files"][0]["projectId"], 1);
        let none = serde_json::to_value(ImportPendingFiles::None).unwrap();
        assert_eq!(none, serde_json::json!({"kind": "none"}));
    }
}

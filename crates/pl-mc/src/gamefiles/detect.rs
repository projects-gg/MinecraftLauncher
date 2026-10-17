//! What a file dropped onto the launcher is, read from its contents (a name alone never decides):
//! a mod, a resource pack, a shader pack or a modpack. Drag-and-drop uses it to send every file to
//! the right folder, and to say where it will go before the file is released.

use std::io::{BufReader, Read, Seek};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use zip::ZipArchive;

use super::fsops::{ImportKind, blocking, import_file_named};
use crate::error::{Error, Result};

/// Archives with more entries than this are not inspected (nothing real comes close).
const MAX_ENTRIES: usize = 500_000;

/// Descriptors a mod carries at the root of its jar.
const MOD_DESCRIPTORS: [&str; 6] = [
    "fabric.mod.json",
    "quilt.mod.json",
    "META-INF/mods.toml",
    "META-INF/neoforge.mods.toml",
    "mcmod.info",
    "litemod.json",
];

/// Entries that mark a jar as an installer or as the game itself, never a mod.
const NOT_A_MOD: [&str; 6] = [
    "install_profile.json",
    "net/fabricmc/installer/",
    "org/quiltmc/installer/",
    "net/minecraft/client/main/Main.class",
    "net/minecraft/server/Main.class",
    "net/minecraft/bundler/Main.class",
];

/// What a dropped file turned out to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum DroppedKind {
    Mod,
    ResourcePack,
    ShaderPack,
    /// A modpack the import flow reads (Modrinth `.mrpack`, Prism/MultiMC or CurseForge zip).
    Modpack,
    /// Folders, data packs, installers, archives without a known layout, other files.
    Unknown,
}

impl DroppedKind {
    /// The game folder this kind is copied into (modpacks are imported, not copied).
    pub fn import_kind(self) -> Option<ImportKind> {
        match self {
            Self::Mod => Some(ImportKind::Mod),
            Self::ResourcePack => Some(ImportKind::ResourcePack),
            Self::ShaderPack => Some(ImportKind::ShaderPack),
            Self::Modpack | Self::Unknown => None,
        }
    }
}

/// One dropped path and what it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DroppedFile {
    pub path: PathBuf,
    pub file_name: String,
    pub kind: DroppedKind,
}

fn extension(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default()
}

/// Entry names as the game reads them: forward slashes, no leading `./` or `/`.
fn normalize(name: &str) -> String {
    let name = name.replace('\\', "/");
    let mut name = name.as_str();
    loop {
        if let Some(rest) = name.strip_prefix("./") {
            name = rest;
        } else if let Some(rest) = name.strip_prefix('/') {
            name = rest;
        } else {
            break;
        }
    }
    name.to_owned()
}

/// `shaders/` at the root, or inside the one folder everything else is in (`BSL_v8/shaders/`):
/// Iris and OptiFine read both.
fn has_shaders_folder(names: &[String]) -> bool {
    if names.iter().any(|n| n.starts_with("shaders/")) {
        return true;
    }
    let mut top = names
        .iter()
        .filter(|n| !n.is_empty())
        .map(|n| n.split_once('/').map_or(n.as_str(), |(first, _)| first));
    let Some(first) = top.next() else {
        return false;
    };
    let single_folder = top.all(|t| t == first);
    single_folder
        && names.iter().any(|n| {
            n.strip_prefix(first)
                .is_some_and(|rest| rest.starts_with("/shaders/"))
        })
}

/// Decide from the entry names (and the file extension) of an archive that is not a modpack.
pub(crate) fn classify_entries(names: &[String], file_name: &str) -> DroppedKind {
    let has = |entry: &str| names.iter().any(|n| n == entry);
    let has_prefix = |prefix: &str| names.iter().any(|n| n.starts_with(prefix));

    // A mod's own descriptor wins: many mods also carry `pack.mcmeta` and `assets/`.
    if MOD_DESCRIPTORS.iter().any(|d| has(d)) {
        return DroppedKind::Mod;
    }
    if has("pack.mcmeta") {
        // Data packs (`data/` without `assets/`) belong in a world, not in `resourcepacks/`.
        return if !has_prefix("assets/") && has_prefix("data/") {
            DroppedKind::Unknown
        } else {
            DroppedKind::ResourcePack
        };
    }
    if has_shaders_folder(names) {
        return DroppedKind::ShaderPack;
    }
    // Old mods without a descriptor are plain jars of classes; installers and the game's own
    // jars are not mods.
    let jar = matches!(extension(file_name).as_str(), "jar" | "litemod");
    if jar
        && names.iter().any(|n| n.ends_with(".class"))
        && !NOT_A_MOD.iter().any(|marker| {
            if marker.ends_with('/') {
                has_prefix(marker)
            } else {
                has(marker)
            }
        })
    {
        return DroppedKind::Mod;
    }
    DroppedKind::Unknown
}

fn classify_archive<R: Read + Seek>(mut archive: ZipArchive<R>, file_name: &str) -> DroppedKind {
    if archive.len() > MAX_ENTRIES {
        return DroppedKind::Unknown;
    }
    if crate::modpack::detect_in(&mut archive).is_some() {
        return DroppedKind::Modpack;
    }
    let names: Vec<String> = archive
        .file_names()
        .filter_map(|name| name.ok())
        .map(|name| normalize(&name))
        // Finder's resource forks say nothing about the layout.
        .filter(|n| !n.is_empty() && !n.starts_with("__MACOSX/"))
        .collect();
    classify_entries(&names, file_name)
}

/// What one path is. Unreadable files, folders and missing paths are [`DroppedKind::Unknown`].
pub fn classify_path(path: &Path) -> DroppedFile {
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let kind = match std::fs::metadata(path) {
        Ok(meta) if meta.is_file() => {
            // Zip parsing of untrusted files: a panic inside the zip reader only makes the file
            // unknown.
            std::panic::catch_unwind(|| {
                let file = std::fs::File::open(path).ok()?;
                let archive = ZipArchive::new(BufReader::new(file)).ok()?;
                Some(classify_archive(archive, &file_name))
            })
            .ok()
            .flatten()
            .unwrap_or(DroppedKind::Unknown)
        }
        _ => DroppedKind::Unknown,
    };
    DroppedFile {
        path: path.to_path_buf(),
        file_name,
        kind,
    }
}

/// [`classify_path`] for several paths, off the async runtime. Order is kept.
pub async fn classify_files(paths: Vec<PathBuf>) -> Vec<DroppedFile> {
    let fallback = paths.clone();
    blocking(move || Ok(paths.iter().map(|p| classify_path(p)).collect()))
        .await
        .unwrap_or_else(|_| {
            fallback
                .into_iter()
                .map(|path| DroppedFile {
                    file_name: path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    path,
                    kind: DroppedKind::Unknown,
                })
                .collect()
        })
}

/// The file name a dropped file gets in its folder: the game only reads resource and shader
/// packs ending in `.zip` and mods ending in `.jar`/`.zip`/`.litemod`, so a missing extension is
/// added.
pub fn import_name(file_name: &str, kind: ImportKind) -> String {
    if kind.accepts(file_name) {
        return file_name.to_owned();
    }
    let ext = match kind {
        ImportKind::Mod => ".jar",
        ImportKind::ResourcePack | ImportKind::ShaderPack => ".zip",
    };
    format!("{file_name}{ext}")
}

/// Copy a classified file into its folder of `game_dir` (the active set's `mods/` for mods).
/// Never overwrites: a file of the same name gives [`Error::AlreadyExists`].
pub async fn import_dropped(game_dir: &Path, file: &DroppedFile) -> Result<PathBuf> {
    let Some(kind) = file.kind.import_kind() else {
        return Err(Error::Invalid(format!(
            "{} cannot be copied into a game folder",
            file.file_name
        )));
    };
    let name = import_name(&file.file_name, kind);
    import_file_named(&file.path, &game_dir.join(kind.folder()), &name).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mods::test_support::zip_bytes;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|n| normalize(n)).collect()
    }

    fn write(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn mods_by_descriptor_or_classes() {
        for descriptor in MOD_DESCRIPTORS {
            assert_eq!(
                classify_entries(&names(&[descriptor, "a/B.class"]), "x.zip"),
                DroppedKind::Mod,
                "{descriptor}"
            );
        }
        // Mods often ship their own resources; the descriptor decides.
        assert_eq!(
            classify_entries(
                &names(&["pack.mcmeta", "assets/x/lang/en_us.json", "fabric.mod.json"]),
                "sodium.jar"
            ),
            DroppedKind::Mod
        );
        // A plain jar of classes is an old mod; a zip of classes is not.
        let classes = names(&["net/example/Mod.class", "META-INF/MANIFEST.MF"]);
        assert_eq!(classify_entries(&classes, "OldMod.jar"), DroppedKind::Mod);
        assert_eq!(
            classify_entries(&classes, "OldMod.litemod"),
            DroppedKind::Mod
        );
        assert_eq!(
            classify_entries(&classes, "classes.zip"),
            DroppedKind::Unknown
        );
    }

    #[test]
    fn installers_and_game_jars_are_not_mods() {
        for marker in [
            vec![
                "install_profile.json",
                "net/minecraftforge/installer/A.class",
            ],
            vec!["net/fabricmc/installer/Main.class"],
            vec!["net/minecraft/client/main/Main.class", "version.json"],
            vec!["net/minecraft/bundler/Main.class"],
        ] {
            assert_eq!(
                classify_entries(&names(&marker), "forge-installer.jar"),
                DroppedKind::Unknown,
                "{marker:?}"
            );
        }
    }

    #[test]
    fn packs_by_layout() {
        assert_eq!(
            classify_entries(
                &names(&["pack.mcmeta", "pack.png", "assets/minecraft/textures/a.png"]),
                "Faithful.zip"
            ),
            DroppedKind::ResourcePack
        );
        // A bare pack.mcmeta (overlays only, or an empty pack) is still a resource pack.
        assert_eq!(
            classify_entries(&names(&["pack.mcmeta"]), "empty.zip"),
            DroppedKind::ResourcePack
        );
        // Data packs go into a world.
        assert_eq!(
            classify_entries(
                &names(&["pack.mcmeta", "data/x/function/a.mcfunction"]),
                "dp.zip"
            ),
            DroppedKind::Unknown
        );
        assert_eq!(
            classify_entries(
                &names(&["shaders/composite.fsh", "shaders/lang/en_US.lang"]),
                "a.zip"
            ),
            DroppedKind::ShaderPack
        );
        // Shader packs wrapped in one folder, with Windows separators.
        assert_eq!(
            classify_entries(
                &names(&["BSL_v8.2/shaders\\final.fsh", "BSL_v8.2\\License.txt"]),
                "BSL.zip"
            ),
            DroppedKind::ShaderPack
        );
        // Two folders: not a shader pack layout any loader reads.
        assert_eq!(
            classify_entries(&names(&["a/shaders/final.fsh", "b/readme.txt"]), "x.zip"),
            DroppedKind::Unknown
        );
        // A resource pack zipped together with its folder is not readable by the game.
        assert_eq!(
            classify_entries(&names(&["Pack/pack.mcmeta", "Pack/assets/a.png"]), "x.zip"),
            DroppedKind::Unknown
        );
        assert_eq!(
            classify_entries(&names(&["readme.txt"]), "notes.zip"),
            DroppedKind::Unknown
        );
    }

    #[tokio::test]
    async fn files_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let mrpack = write(
            dir.path(),
            "Pack.mrpack",
            &zip_bytes(&[(
                "modrinth.index.json",
                br#"{"formatVersion":1,"game":"minecraft","versionId":"1","name":"P","files":[],"dependencies":{"minecraft":"1.21.1"}}"#,
            )]),
        );
        // The extension does not matter: a renamed .mrpack is still a modpack.
        let renamed = write(dir.path(), "Pack.zip", &std::fs::read(&mrpack).unwrap());
        let jar = write(
            dir.path(),
            "sodium.jar",
            &zip_bytes(&[("fabric.mod.json", br#"{"id":"sodium"}"#)]),
        );
        let pack = write(
            dir.path(),
            "Faithful",
            &zip_bytes(&[("pack.mcmeta", br#"{"pack":{"pack_format":34}}"#)]),
        );
        let shader = write(
            dir.path(),
            "Complementary.zip",
            &zip_bytes(&[("shaders/final.fsh", b"void main(){}")]),
        );
        let fake_jar = write(dir.path(), "not-a-jar.jar", b"plain text");
        let folder = dir.path().join("Folder Pack");
        std::fs::create_dir_all(&folder).unwrap();
        let missing = dir.path().join("gone.zip");

        let files = classify_files(vec![
            mrpack, renamed, jar, pack, shader, fake_jar, folder, missing,
        ])
        .await;
        let kinds: Vec<DroppedKind> = files.iter().map(|f| f.kind).collect();
        assert_eq!(
            kinds,
            [
                DroppedKind::Modpack,
                DroppedKind::Modpack,
                DroppedKind::Mod,
                DroppedKind::ResourcePack,
                DroppedKind::ShaderPack,
                DroppedKind::Unknown,
                DroppedKind::Unknown,
                DroppedKind::Unknown,
            ]
        );
        assert_eq!(files[3].file_name, "Faithful");
    }

    #[tokio::test]
    async fn imports_into_the_right_folder() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path().join("game");
        let pack = write(
            dir.path(),
            "Faithful",
            &zip_bytes(&[("pack.mcmeta", br#"{"pack":{"pack_format":34}}"#)]),
        );
        let jar = write(
            dir.path(),
            "sodium.jar",
            &zip_bytes(&[("fabric.mod.json", br#"{"id":"sodium"}"#)]),
        );
        let shader = write(
            dir.path(),
            "BSL.zip",
            &zip_bytes(&[("shaders/final.fsh", b"x")]),
        );

        let pack = classify_path(&pack);
        let dest = import_dropped(&game, &pack).await.unwrap();
        // The game only lists zip resource packs that end in ".zip".
        assert_eq!(dest, game.join("resourcepacks").join("Faithful.zip"));
        assert!(matches!(
            import_dropped(&game, &pack).await,
            Err(Error::AlreadyExists(_))
        ));

        let jar = classify_path(&jar);
        assert_eq!(
            import_dropped(&game, &jar).await.unwrap(),
            game.join("mods").join("sodium.jar")
        );
        let shader = classify_path(&shader);
        assert_eq!(
            import_dropped(&game, &shader).await.unwrap(),
            game.join("shaderpacks").join("BSL.zip")
        );

        let unknown = DroppedFile {
            path: dir.path().join("x.txt"),
            file_name: "x.txt".into(),
            kind: DroppedKind::Unknown,
        };
        assert!(import_dropped(&game, &unknown).await.is_err());
    }

    #[test]
    fn import_names() {
        assert_eq!(import_name("a.jar", ImportKind::Mod), "a.jar");
        assert_eq!(import_name("a.ZIP", ImportKind::ResourcePack), "a.ZIP");
        assert_eq!(
            import_name("Faithful", ImportKind::ResourcePack),
            "Faithful.zip"
        );
        assert_eq!(import_name("OldMod", ImportKind::Mod), "OldMod.jar");
        assert_eq!(
            import_name("BSL.rar", ImportKind::ShaderPack),
            "BSL.rar.zip"
        );
    }
}

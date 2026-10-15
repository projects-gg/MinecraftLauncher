//! Data directory layout.
//!
//! Every path the launcher touches on disk comes from [`DataDirs`]. The root is user-relocatable
//! (Settings → Data), so nothing here is global: a `DataDirs` value is created once by the app and
//! passed explicitly.
//!
//! ```text
//! meta/                cached manifests (Mojang version manifest, loader metadata, Java manifests)
//! libraries/           shared Maven-layout libraries
//! assets/              indexes/, objects/, virtual/<index>/, log_configs/
//! versions/<id>/       <id>.json, <id>.jar
//! runtimes/<component>/<platform>/   Mojang Java runtimes
//! natives/<key>/       extracted natives (per version)
//! instances/<id>/      instance.json + minecraft/ (the game directory)
//! cache/content/       signed content index, news, media
//! afk/                 AFK client builds and accounts
//! logs/                launcher and game logs
//! settings.json, accounts.json
//! ```

use std::path::{Component, Path, PathBuf};

use crate::error::{Error, Result};

/// Folder name of the data root inside the per-OS application data directory.
pub const ROOT_DIR_NAME: &str = "ProjectsLauncher";

/// The launcher's data directory layout. Cheap to clone.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DataDirs {
    root: PathBuf,
}

impl DataDirs {
    /// Use `root` as the data root (it does not need to exist yet; see [`DataDirs::ensure`]).
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Default root: Windows `%APPDATA%\ProjectsLauncher`, macOS
    /// `~/Library/Application Support/ProjectsLauncher`, Linux `$XDG_DATA_HOME/ProjectsLauncher`
    /// (`~/.local/share/ProjectsLauncher`). `None` when the OS reports no home directory.
    pub fn default_root() -> Option<PathBuf> {
        dirs::data_dir().map(|dir| dir.join(ROOT_DIR_NAME))
    }

    /// [`DataDirs`] at [`DataDirs::default_root`].
    pub fn with_default_root() -> Result<Self> {
        Self::default_root()
            .map(Self::new)
            .ok_or_else(|| Error::NotFound("user data directory".into()))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Cached manifests and metadata (`meta/`).
    pub fn meta(&self) -> PathBuf {
        self.root.join("meta")
    }

    /// Shared Maven-layout libraries (`libraries/`).
    pub fn libraries(&self) -> PathBuf {
        self.root.join("libraries")
    }

    /// A library file from its Maven-layout relative path (`org/ow2/asm/asm/9.7/asm-9.7.jar`).
    /// Fails on absolute paths or `..` so a hostile version JSON cannot escape the folder.
    pub fn library_file(&self, maven_path: &str) -> Result<PathBuf> {
        safe_join(&self.libraries(), maven_path)
    }

    /// `assets/`, passed to the game as `${assets_root}`.
    pub fn assets(&self) -> PathBuf {
        self.root.join("assets")
    }

    /// `assets/indexes/`.
    pub fn asset_indexes(&self) -> PathBuf {
        self.assets().join("indexes")
    }

    /// `assets/indexes/<id>.json`.
    pub fn asset_index_file(&self, index_id: &str) -> Result<PathBuf> {
        Ok(self
            .asset_indexes()
            .join(format!("{}.json", safe_name(index_id)?)))
    }

    /// `assets/objects/`.
    pub fn asset_objects(&self) -> PathBuf {
        self.assets().join("objects")
    }

    /// `assets/objects/<first two hex chars>/<hash>`.
    pub fn asset_object(&self, hash: &str) -> Result<PathBuf> {
        if hash.len() < 2 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(Error::Invalid(format!("asset hash `{hash}`")));
        }
        Ok(self.asset_objects().join(&hash[..2]).join(hash))
    }

    /// `assets/virtual/` (pre-1.7 "virtual" asset layouts).
    pub fn asset_virtual(&self) -> PathBuf {
        self.assets().join("virtual")
    }

    /// `assets/virtual/<index id>/` (e.g. `virtual/legacy`).
    pub fn asset_virtual_dir(&self, index_id: &str) -> Result<PathBuf> {
        Ok(self.asset_virtual().join(safe_name(index_id)?))
    }

    /// `assets/log_configs/` (log4j configuration files from version JSONs).
    pub fn log_configs(&self) -> PathBuf {
        self.assets().join("log_configs")
    }

    /// `versions/`.
    pub fn versions(&self) -> PathBuf {
        self.root.join("versions")
    }

    /// `versions/<id>/`.
    pub fn version_dir(&self, id: &str) -> Result<PathBuf> {
        Ok(self.versions().join(safe_name(id)?))
    }

    /// `versions/<id>/<id>.json`.
    pub fn version_json(&self, id: &str) -> Result<PathBuf> {
        Ok(self.version_dir(id)?.join(format!("{id}.json")))
    }

    /// `versions/<id>/<id>.jar`.
    pub fn version_jar(&self, id: &str) -> Result<PathBuf> {
        Ok(self.version_dir(id)?.join(format!("{id}.jar")))
    }

    /// `runtimes/`.
    pub fn runtimes(&self) -> PathBuf {
        self.root.join("runtimes")
    }

    /// `runtimes/<component>/<platform>/` (e.g. `runtimes/java-runtime-delta/windows-x64`).
    pub fn runtime_dir(&self, component: &str, platform_key: &str) -> Result<PathBuf> {
        Ok(self
            .runtimes()
            .join(safe_name(component)?)
            .join(safe_name(platform_key)?))
    }

    /// `natives/`.
    pub fn natives(&self) -> PathBuf {
        self.root.join("natives")
    }

    /// `natives/<key>/`.
    pub fn natives_dir(&self, key: &str) -> Result<PathBuf> {
        Ok(self.natives().join(safe_name(key)?))
    }

    /// `instances/`.
    pub fn instances(&self) -> PathBuf {
        self.root.join("instances")
    }

    /// `instances/<id>/`.
    pub fn instance_dir(&self, id: &str) -> Result<PathBuf> {
        Ok(self.instances().join(safe_name(id)?))
    }

    /// `cache/`.
    pub fn cache(&self) -> PathBuf {
        self.root.join("cache")
    }

    /// `cache/content/` (signed content index, news, media).
    pub fn content_cache(&self) -> PathBuf {
        self.cache().join("content")
    }

    /// `afk/`.
    pub fn afk(&self) -> PathBuf {
        self.root.join("afk")
    }

    /// `logs/` (launcher logs and captured game output).
    pub fn logs(&self) -> PathBuf {
        self.root.join("logs")
    }

    /// `settings.json`.
    pub fn settings_file(&self) -> PathBuf {
        self.root.join("settings.json")
    }

    /// `accounts.json` (account list only; tokens live in the OS keychain).
    pub fn accounts_file(&self) -> PathBuf {
        self.root.join("accounts.json")
    }

    /// Create the root and every top-level folder. Idempotent.
    pub fn ensure(&self) -> Result<()> {
        let dirs = [
            self.root.clone(),
            self.meta(),
            self.libraries(),
            self.asset_indexes(),
            self.asset_objects(),
            self.asset_virtual(),
            self.log_configs(),
            self.versions(),
            self.runtimes(),
            self.natives(),
            self.instances(),
            self.content_cache(),
            self.afk(),
            self.logs(),
        ];
        for dir in dirs {
            std::fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
        }
        Ok(())
    }
}

/// Write a file atomically: write a uniquely named temporary file next to it, then rename it over
/// the destination. Parent folders are created as needed.
pub async fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| Error::io(parent, e))?;
    }
    let tmp = temp_sibling(path);
    tokio::fs::write(&tmp, bytes)
        .await
        .map_err(|e| Error::io(&tmp, e))?;
    if let Err(error) = tokio::fs::rename(&tmp, path).await {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(Error::io(path, error));
    }
    Ok(())
}

/// Blocking variant of [`write_atomic`] for code already running on a blocking thread.
pub fn write_atomic_blocking(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    let tmp = temp_sibling(path);
    std::fs::write(&tmp, bytes).map_err(|e| Error::io(&tmp, e))?;
    if let Err(error) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(Error::io(path, error));
    }
    Ok(())
}

fn temp_sibling(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(format!(".{}.tmp", uuid::Uuid::new_v4().simple()));
    path.with_file_name(name)
}

/// Validate a single path component that comes from external data (version ids, runtime
/// components, instance ids): no separators, no `..`, not empty.
pub fn safe_name(name: &str) -> Result<&str> {
    let bad = name.is_empty()
        || name == "."
        || name == ".."
        || name.contains(['/', '\\', '\0'])
        || (cfg!(windows) && name.contains(':'));
    if bad {
        return Err(Error::Invalid(format!("unsafe path component `{name}`")));
    }
    Ok(name)
}

/// Join a `/`-separated relative path from external data onto `base`, rejecting absolute paths,
/// drive prefixes and `..` so the result always stays inside `base`.
pub fn safe_join(base: &Path, relative: &str) -> Result<PathBuf> {
    let invalid = || Error::Invalid(format!("unsafe relative path `{relative}`"));
    if relative.is_empty() || relative.contains('\0') || relative.starts_with(['/', '\\']) {
        return Err(invalid());
    }
    let mut out = base.to_path_buf();
    for part in relative.split(['/', '\\']) {
        if part.is_empty() || part == "." {
            continue;
        }
        let mut components = Path::new(part).components();
        match (components.next(), components.next()) {
            (Some(Component::Normal(normal)), None) => out.push(normal),
            _ => return Err(invalid()),
        }
    }
    if out == base {
        return Err(invalid());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_matches_architecture() {
        let dirs = DataDirs::new("/data");
        assert_eq!(dirs.meta(), Path::new("/data/meta"));
        assert_eq!(
            dirs.asset_object("ab12cd").unwrap(),
            Path::new("/data/assets/objects/ab/ab12cd")
        );
        assert_eq!(
            dirs.version_json("1.21.1").unwrap(),
            Path::new("/data/versions/1.21.1/1.21.1.json")
        );
        assert_eq!(
            dirs.runtime_dir("jre-legacy", "windows-x64").unwrap(),
            Path::new("/data/runtimes/jre-legacy/windows-x64")
        );
        assert_eq!(
            dirs.asset_virtual_dir("legacy").unwrap(),
            Path::new("/data/assets/virtual/legacy")
        );
        assert_eq!(
            dirs.library_file("org/ow2/asm/asm/9.7/asm-9.7.jar")
                .unwrap(),
            Path::new("/data/libraries/org/ow2/asm/asm/9.7/asm-9.7.jar")
        );
    }

    #[test]
    fn rejects_escaping_paths() {
        let dirs = DataDirs::new("/data");
        assert!(dirs.version_json("../evil").is_err());
        assert!(dirs.version_json("a/b").is_err());
        assert!(dirs.library_file("../../etc/passwd").is_err());
        assert!(dirs.library_file("/etc/passwd").is_err());
        assert!(dirs.library_file("C:\\Windows\\x.dll").is_err() || !cfg!(windows));
        assert!(dirs.asset_object("zz").is_err());
        assert!(dirs.instance_dir("").is_err());
        assert!(safe_join(Path::new("/data"), "a/./b").is_ok());
        assert!(safe_join(Path::new("/data"), "").is_err());
    }

    #[tokio::test]
    async fn atomic_writes_replace_files() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("a").join("b.json");
        write_atomic(&file, b"one").await.unwrap();
        write_atomic(&file, b"two").await.unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"two");
        write_atomic_blocking(&file, b"three").unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"three");
        let leftovers = std::fs::read_dir(file.parent().unwrap()).unwrap().count();
        assert_eq!(leftovers, 1, "no temporary files are left behind");
    }

    #[test]
    fn ensure_creates_tree() {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = DataDirs::new(tmp.path().join("root"));
        dirs.ensure().unwrap();
        assert!(dirs.asset_indexes().is_dir());
        assert!(dirs.content_cache().is_dir());
        assert!(dirs.logs().is_dir());
        dirs.ensure().unwrap();
    }
}

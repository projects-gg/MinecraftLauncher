//! Instances on disk: `instances/<id>/instance.json` plus `instances/<id>/minecraft/` (the game
//! directory). Writes are atomic; broken entries are skipped when listing.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::error::{Error, Result};
use crate::instance::memory::{recommended_memory, system_memory_mb};
use crate::instance::model::{Instance, NewInstance};
use crate::paths::{DataDirs, safe_name, write_atomic};

pub const INSTANCE_FILE: &str = "instance.json";
/// Game directory inside an instance folder.
pub const GAME_DIR_NAME: &str = "minecraft";
const MAX_SLUG_LEN: usize = 40;

#[derive(Debug, Clone)]
pub struct InstanceStore {
    root: PathBuf,
    use_trash: bool,
}

impl InstanceStore {
    /// Store at `dirs.instances()`; deleted instances go to the OS trash.
    pub fn new(dirs: &DataDirs) -> Self {
        Self::with_root(dirs.instances())
    }

    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            use_trash: true,
        }
    }

    /// Delete permanently instead of moving to the trash (tests, headless systems).
    pub fn without_trash(mut self) -> Self {
        self.use_trash = false;
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn instance_dir(&self, id: &str) -> Result<PathBuf> {
        Ok(self.root.join(safe_name(id)?))
    }

    /// The game directory (`${game_directory}`).
    pub fn game_dir(&self, id: &str) -> Result<PathBuf> {
        Ok(self.instance_dir(id)?.join(GAME_DIR_NAME))
    }

    fn file(&self, id: &str) -> Result<PathBuf> {
        Ok(self.instance_dir(id)?.join(INSTANCE_FILE))
    }

    /// Every readable instance, sorted by name. Broken folders are skipped with a warning.
    pub async fn list(&self) -> Result<Vec<Instance>> {
        let mut entries = match tokio::fs::read_dir(&self.root).await {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(Error::io(&self.root, e)),
        };
        let mut out = Vec::new();
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| Error::io(&self.root, e))?
        {
            let Ok(kind) = entry.file_type().await else {
                continue;
            };
            if !kind.is_dir() {
                continue;
            }
            let id = entry.file_name().to_string_lossy().into_owned();
            match self.get(&id).await {
                Ok(instance) => out.push(instance),
                Err(error) => {
                    tracing::warn!(%id, %error, "skipping unreadable instance");
                }
            }
        }
        out.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(out)
    }

    pub async fn get(&self, id: &str) -> Result<Instance> {
        let path = self.file(id)?;
        let bytes = match tokio::fs::read(&path).await {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(Error::NotFound(format!("instance {id}")));
            }
            Err(e) => return Err(Error::io(&path, e)),
        };
        let mut instance: Instance = serde_json::from_slice(&bytes)
            .map_err(|e| Error::json(path.display().to_string(), e))?;
        // The folder name is authoritative (folders may have been renamed or copied by hand).
        if instance.id != id {
            instance.id = id.to_owned();
        }
        Ok(instance)
    }

    /// Create an instance with a unique slug id derived from its name.
    pub async fn create(&self, new: NewInstance) -> Result<Instance> {
        let name = new.name.trim().to_owned();
        if name.is_empty() {
            return Err(Error::Invalid("instance name is empty".into()));
        }
        if new.mc_version.trim().is_empty() {
            return Err(Error::Invalid("instance version is empty".into()));
        }
        let id = self.claim_id(&slugify(&name)).await?;
        let mut instance = Instance::new(&id, name, new.mc_version.trim());
        instance.loader = new.loader;
        if let Some(icon) = new.icon {
            instance.icon = icon;
        }
        instance.group = new.group.filter(|g| !g.trim().is_empty());
        instance.memory = new
            .memory
            .unwrap_or_else(|| recommended_memory(system_memory_mb(), instance.is_modded()));
        instance.managed_pack = new.managed_pack;
        let game_dir = self.game_dir(&id)?;
        if let Err(error) = async {
            tokio::fs::create_dir_all(&game_dir)
                .await
                .map_err(|e| Error::io(&game_dir, e))?;
            self.write(&instance).await
        }
        .await
        {
            let _ = tokio::fs::remove_dir_all(self.instance_dir(&id)?).await;
            return Err(error);
        }
        Ok(instance)
    }

    /// Save changes to an existing instance (atomic).
    pub async fn update(&self, instance: &Instance) -> Result<()> {
        if !self.file(&instance.id)?.is_file() {
            return Err(Error::NotFound(format!("instance {}", instance.id)));
        }
        self.write(instance).await
    }

    async fn write(&self, instance: &Instance) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(instance)
            .map_err(|e| Error::json(format!("instance {}", instance.id), e))?;
        write_atomic(&self.file(&instance.id)?, &bytes).await
    }

    /// Remove an instance: into the OS trash when possible, deleted otherwise.
    pub async fn delete(&self, id: &str) -> Result<()> {
        let dir = self.instance_dir(id)?;
        if !dir.is_dir() {
            return Err(Error::NotFound(format!("instance {id}")));
        }
        let use_trash = self.use_trash;
        let dir_for_error = dir.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            if use_trash {
                match trash::delete(&dir) {
                    Ok(()) => return Ok(()),
                    Err(error) => {
                        tracing::warn!(%error, path = %dir.display(), "moving to trash failed, deleting");
                    }
                }
            }
            std::fs::remove_dir_all(&dir).map_err(|e| Error::io(&dir, e))
        })
        .await
        .map_err(|e| Error::io(dir_for_error, std::io::Error::other(e.to_string())))?
    }

    /// Copy an instance (settings and game directory) under a new name.
    pub async fn duplicate(&self, id: &str, new_name: &str) -> Result<Instance> {
        let source = self.get(id).await?;
        let name = match new_name.trim() {
            "" => format!("{} (kopya)", source.name),
            name => name.to_owned(),
        };
        let new_id = self.claim_id(&slugify(&name)).await?;
        let from = self.instance_dir(id)?;
        let to = self.instance_dir(&new_id)?;
        let copy_to = to.clone();
        let copied = tokio::task::spawn_blocking(move || copy_tree(&from, &copy_to))
            .await
            .map_err(|e| Error::io(&to, std::io::Error::other(e.to_string())))
            .and_then(|r| r);
        let result = match copied {
            Ok(()) => {
                let mut copy = source;
                copy.id = new_id.clone();
                copy.name = name;
                copy.created_at = jiff::Timestamp::now();
                copy.last_played_at = None;
                copy.play_time_secs = 0;
                self.write(&copy).await.map(|()| copy)
            }
            Err(error) => Err(error),
        };
        if result.is_err() {
            let _ = tokio::fs::remove_dir_all(&to).await;
        }
        result
    }

    /// Add a finished session to the play time and set "last played".
    pub async fn record_session(&self, id: &str, played: Duration) -> Result<Instance> {
        let mut instance = self.get(id).await?;
        instance.play_time_secs = instance.play_time_secs.saturating_add(played.as_secs());
        instance.last_played_at = Some(jiff::Timestamp::now());
        self.write(&instance).await?;
        Ok(instance)
    }

    /// Reserve a free folder for `base` (`base`, `base-2`, `base-3`...). Creating the folder is
    /// the reservation, so concurrent creates never collide.
    async fn claim_id(&self, base: &str) -> Result<String> {
        tokio::fs::create_dir_all(&self.root)
            .await
            .map_err(|e| Error::io(&self.root, e))?;
        for n in 1..10_000u32 {
            let candidate = if n == 1 {
                base.to_owned()
            } else {
                format!("{base}-{n}")
            };
            let dir = self.root.join(&candidate);
            match tokio::fs::create_dir(&dir).await {
                Ok(()) => return Ok(candidate),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(Error::io(&dir, e)),
            }
        }
        Err(Error::Invalid(format!("no free instance id for {base}")))
    }
}

/// Folder-safe id from a display name (Turkish letters transliterated): `"Çılgın Köy"` →
/// `cilgin-koy`.
pub fn slugify(name: &str) -> String {
    let mut slug = String::with_capacity(name.len());
    let mut pending_dash = false;
    for c in name.chars() {
        let mapped = match c {
            'ç' | 'Ç' => Some('c'),
            'ğ' | 'Ğ' => Some('g'),
            'ı' | 'İ' => Some('i'),
            'ö' | 'Ö' => Some('o'),
            'ş' | 'Ş' => Some('s'),
            'ü' | 'Ü' => Some('u'),
            'â' | 'Â' | 'á' | 'à' | 'ä' => Some('a'),
            'é' | 'è' | 'ê' | 'ë' => Some('e'),
            'î' | 'í' | 'ï' => Some('i'),
            'ô' | 'ó' => Some('o'),
            'û' | 'ú' => Some('u'),
            c if c.is_ascii_alphanumeric() => Some(c.to_ascii_lowercase()),
            _ => None,
        };
        match mapped {
            Some(c) => {
                if pending_dash && !slug.is_empty() {
                    slug.push('-');
                }
                pending_dash = false;
                slug.push(c);
            }
            None => pending_dash = true,
        }
        if slug.len() >= MAX_SLUG_LEN {
            break;
        }
    }
    let slug = slug.trim_matches('-').to_owned();
    if slug.is_empty() {
        return "instance".to_owned();
    }
    const RESERVED: [&str; 4] = ["con", "prn", "aux", "nul"];
    let reserved_device = (slug.starts_with("com") || slug.starts_with("lpt"))
        && slug.len() == 4
        && slug.as_bytes()[3].is_ascii_digit();
    if RESERVED.contains(&slug.as_str()) || reserved_device {
        return format!("{slug}-instance");
    }
    slug
}

/// Recursive copy (directories and regular files; symlinks are skipped).
fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    for entry in walkdir::WalkDir::new(from).follow_links(false) {
        let entry = entry.map_err(|e| {
            let path = e
                .path()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| from.to_path_buf());
            Error::io(path, std::io::Error::other(e.to_string()))
        })?;
        let relative = entry.path().strip_prefix(from).unwrap_or(entry.path());
        let target = to.join(relative);
        let kind = entry.file_type();
        if kind.is_dir() {
            std::fs::create_dir_all(&target).map_err(|e| Error::io(&target, e))?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), &target).map_err(|e| Error::io(&target, e))?;
        } else {
            tracing::debug!(path = %entry.path().display(), "skipping symlink while duplicating");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instance::model::{InstanceLoader, MemorySettings};
    use crate::loaders::LoaderKind;

    fn store() -> (tempfile::TempDir, InstanceStore) {
        let tmp = tempfile::tempdir().unwrap();
        let store = InstanceStore::with_root(tmp.path().join("instances")).without_trash();
        (tmp, store)
    }

    fn new(name: &str) -> NewInstance {
        NewInstance {
            name: name.into(),
            mc_version: "1.21.1".into(),
            memory: Some(MemorySettings {
                min_mb: 1024,
                max_mb: 4096,
            }),
            ..NewInstance::default()
        }
    }

    #[test]
    fn slugs() {
        assert_eq!(slugify("Çılgın Köy Şövalyeleri"), "cilgin-koy-sovalyeleri");
        assert_eq!(slugify("  Fabric 1.21.1 !! "), "fabric-1-21-1");
        assert_eq!(slugify("İğne"), "igne");
        assert_eq!(slugify("///"), "instance");
        assert_eq!(slugify("CON"), "con-instance");
        assert_eq!(slugify("com1"), "com1-instance");
        assert_eq!(slugify("community"), "community");
        assert!(slugify(&"a".repeat(100)).len() <= MAX_SLUG_LEN);
    }

    #[tokio::test]
    async fn crud_round_trip() {
        let (_tmp, store) = store();
        assert!(store.list().await.unwrap().is_empty());

        let a = store.create(new("Survival")).await.unwrap();
        assert_eq!(a.id, "survival");
        assert!(store.game_dir("survival").unwrap().is_dir());
        let b = store.create(new("Survival")).await.unwrap();
        assert_eq!(b.id, "survival-2");
        let mut fabric = new("Modlu Dünya");
        fabric.loader = InstanceLoader {
            kind: LoaderKind::Fabric,
            version: Some("0.16.5".into()),
        };
        let c = store.create(fabric).await.unwrap();
        assert_eq!(c.id, "modlu-dunya");

        let ids: Vec<String> = store
            .list()
            .await
            .unwrap()
            .into_iter()
            .map(|i| i.id)
            .collect();
        assert_eq!(ids, ["modlu-dunya", "survival", "survival-2"]);

        let mut changed = store.get("survival").await.unwrap();
        changed.notes = "notlar".into();
        changed.jvm_args = "-XX:+UseZGC".into();
        store.update(&changed).await.unwrap();
        assert_eq!(store.get("survival").await.unwrap().notes, "notlar");

        let played = store
            .record_session("survival", Duration::from_secs(125))
            .await
            .unwrap();
        assert_eq!(played.play_time_secs, 125);
        assert!(played.last_played_at.is_some());
        let played = store
            .record_session("survival", Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(played.play_time_secs, 130);

        store.delete("survival-2").await.unwrap();
        assert!(matches!(
            store.get("survival-2").await,
            Err(Error::NotFound(_))
        ));
        assert!(matches!(
            store.delete("survival-2").await,
            Err(Error::NotFound(_))
        ));
        let mut ghost = changed.clone();
        ghost.id = "ghost".into();
        assert!(matches!(
            store.update(&ghost).await,
            Err(Error::NotFound(_))
        ));
        assert!(store.get("../escape").await.is_err());
        assert!(store.create(new("   ")).await.is_err());
    }

    #[tokio::test]
    async fn duplicate_copies_game_files() {
        let (_tmp, store) = store();
        let original = store.create(new("Ana")).await.unwrap();
        let game = store.game_dir(&original.id).unwrap();
        std::fs::create_dir_all(game.join("saves/World")).unwrap();
        std::fs::write(game.join("saves/World/level.dat"), b"nbt").unwrap();
        std::fs::write(game.join("options.txt"), b"fov:70").unwrap();
        store
            .record_session(&original.id, Duration::from_secs(60))
            .await
            .unwrap();

        let copy = store.duplicate(&original.id, "").await.unwrap();
        assert_eq!(copy.id, "ana-kopya");
        assert_eq!(copy.name, "Ana (kopya)");
        assert_eq!(copy.play_time_secs, 0);
        let copied = store.game_dir(&copy.id).unwrap();
        assert_eq!(
            std::fs::read(copied.join("saves/World/level.dat")).unwrap(),
            b"nbt"
        );
        assert_eq!(
            std::fs::read(copied.join("options.txt")).unwrap(),
            b"fov:70"
        );
        assert_eq!(store.get(&copy.id).await.unwrap().name, "Ana (kopya)");
        assert_eq!(store.get(&original.id).await.unwrap().play_time_secs, 60);
    }

    #[tokio::test]
    async fn broken_entries_are_skipped() {
        let (_tmp, store) = store();
        store.create(new("Good")).await.unwrap();
        let broken = store.root().join("broken");
        std::fs::create_dir_all(&broken).unwrap();
        std::fs::write(broken.join(INSTANCE_FILE), b"{not json").unwrap();
        std::fs::create_dir_all(store.root().join("empty")).unwrap();
        std::fs::write(store.root().join("stray.txt"), b"x").unwrap();
        let all = store.list().await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, "good");

        // A copied folder whose JSON still has the old id is reported under its folder name.
        let moved = store.root().join("moved");
        std::fs::create_dir_all(&moved).unwrap();
        std::fs::copy(
            store.root().join("good").join(INSTANCE_FILE),
            moved.join(INSTANCE_FILE),
        )
        .unwrap();
        assert_eq!(store.get("moved").await.unwrap().id, "moved");
    }
}

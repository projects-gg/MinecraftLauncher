//! What the Modrinth App's database (`app.db`, next to its settings) says about the player's
//! profiles: where the profile folders are (`settings.custom_dir`, the app folder the player
//! chose in its settings) and each profile's name, Minecraft version, loader and times.
//!
//! The layout changed over the app's versions: `profiles` (2024–2026) became `instances`, whose
//! version and loader moved into `instance_content_sets` (the set `applied_content_set_id`
//! names). Both are read; columns are looked up by name, so a column added or dropped later
//! leaves the others readable. Times are Unix seconds. Accounts and their sign-ins (other tables)
//! are never read.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::types::ValueRef;
use rusqlite::{Connection, Row};

/// More profiles than any player keeps: a damaged table is not read forever.
const MAX_ROWS: usize = 10_000;

#[derive(Debug, Default)]
pub(super) struct AppDb {
    /// The folder the app keeps its profiles in when the player moved it (`<custom_dir>/profiles`).
    pub profiles_root: Option<PathBuf>,
    pub profiles: Vec<DbProfile>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct DbProfile {
    /// The profile's folder under `profiles/` (a single folder name).
    pub path: String,
    pub name: Option<String>,
    pub game_version: Option<String>,
    /// `vanilla`, `fabric`, `quilt`, `forge`, `neoforge`.
    pub loader: Option<String>,
    /// As the app keeps it (Forge's carry the Minecraft version: `1.20.1-47.3.0`).
    pub loader_version: Option<String>,
    pub created_ms: Option<i64>,
    pub last_played_ms: Option<i64>,
}

/// The app's database in `app_dir`, when there is one that can be read.
pub(super) fn read(app_dir: &Path) -> Option<AppDb> {
    let db = crate::sqlite::open(&app_dir.join("app.db"))?;
    let profiles = if has_table(&db, "instances") {
        instances(&db)
    } else if has_table(&db, "profiles") {
        legacy_profiles(&db)
    } else {
        Vec::new()
    };
    Some(AppDb {
        profiles_root: custom_dir(&db).map(|dir| dir.join("profiles")),
        profiles,
    })
}

fn has_table(db: &Connection, name: &str) -> bool {
    db.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [name],
        |_| Ok(()),
    )
    .is_ok()
}

/// The app folder the player chose; `None` when it is the default one or not set.
fn custom_dir(db: &Connection) -> Option<PathBuf> {
    let dir: Option<String> = db
        .query_row("SELECT custom_dir FROM settings LIMIT 1", [], |row| {
            row.get(0)
        })
        .ok()?;
    let dir = dir?;
    let dir = dir.trim();
    // A path Windows canonicalized (`\\?\D:\Modrinth`) reads the same without its prefix.
    let dir = match dir.strip_prefix(r"\\?\UNC\") {
        Some(share) => format!(r"\\{share}"),
        None => dir.strip_prefix(r"\\?\").unwrap_or(dir).to_owned(),
    };
    let path = PathBuf::from(dir);
    path.is_absolute().then_some(path)
}

/// Every row of `table` that `map` turns into something.
fn rows<T>(db: &Connection, table: &str, map: impl Fn(&Row<'_>) -> Option<T>) -> Vec<T> {
    let read = || -> rusqlite::Result<Vec<T>> {
        // `table` is one of this file's own names, never the database's.
        let mut statement = db.prepare(&format!("SELECT * FROM \"{table}\""))?;
        let mut rows = statement.query([])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            if let Some(item) = map(row) {
                out.push(item);
            }
            if out.len() >= MAX_ROWS {
                break;
            }
        }
        Ok(out)
    };
    read().unwrap_or_else(|error| {
        tracing::debug!(%error, table, "could not read the Modrinth App's table");
        Vec::new()
    })
}

/// A text column (`None` when missing, empty or not text).
fn text(row: &Row<'_>, column: &str) -> Option<String> {
    let value = match row.get_ref(column).ok()? {
        ValueRef::Text(bytes) => String::from_utf8_lossy(bytes).trim().to_owned(),
        ValueRef::Integer(number) => number.to_string(),
        _ => return None,
    };
    (!value.is_empty()).then_some(value)
}

/// A time column in milliseconds since 1970. The app writes Unix seconds; a value already in
/// milliseconds (no app wrote one, but it would be unmistakable) is taken as it is.
fn time_ms(row: &Row<'_>, column: &str) -> Option<i64> {
    let value = match row.get_ref(column).ok()? {
        ValueRef::Integer(number) => number,
        ValueRef::Real(number) if number.is_finite() => number as i64,
        ValueRef::Text(bytes) => std::str::from_utf8(bytes).ok()?.trim().parse().ok()?,
        _ => return None,
    };
    match value {
        ..=0 => None,
        100_000_000_000.. => Some(value),
        seconds => seconds.checked_mul(1000),
    }
}

/// A profile folder name the app could have made: one plain name, nothing that leaves
/// `profiles/`.
fn folder_name(path: String) -> Option<String> {
    let plain = !path.is_empty()
        && path != "."
        && path != ".."
        && !path.contains(['/', '\\'])
        && !path.contains(':');
    plain.then_some(path)
}

/// The current layout: `instances`, each playing the content set it names.
fn instances(db: &Connection) -> Vec<DbProfile> {
    struct Set {
        instance: Option<String>,
        game_version: Option<String>,
        loader: Option<String>,
        loader_version: Option<String>,
    }
    let sets: HashMap<String, Set> = if has_table(db, "instance_content_sets") {
        rows(db, "instance_content_sets", |row| {
            Some((
                text(row, "id")?,
                Set {
                    instance: text(row, "instance_id"),
                    game_version: text(row, "game_version"),
                    loader: text(row, "loader"),
                    loader_version: text(row, "loader_version"),
                },
            ))
        })
        .into_iter()
        .collect()
    } else {
        HashMap::new()
    };
    rows(db, "instances", |row| {
        let path = folder_name(text(row, "path")?)?;
        let id = text(row, "id");
        // The set it plays; else any set of it (one is always applied, but a half-written
        // database may say otherwise).
        let set = text(row, "applied_content_set_id")
            .and_then(|applied| sets.get(&applied))
            .or_else(|| {
                let id = id.as_deref()?;
                sets.values()
                    .find(|set| set.instance.as_deref() == Some(id))
            });
        Some(DbProfile {
            path,
            name: text(row, "name"),
            game_version: set.and_then(|s| s.game_version.clone()),
            loader: set.and_then(|s| s.loader.clone()),
            loader_version: set.and_then(|s| s.loader_version.clone()),
            created_ms: time_ms(row, "created"),
            last_played_ms: time_ms(row, "last_played"),
        })
    })
}

/// The 2024–2026 layout: one `profiles` row per profile.
fn legacy_profiles(db: &Connection) -> Vec<DbProfile> {
    rows(db, "profiles", |row| {
        Some(DbProfile {
            path: folder_name(text(row, "path")?)?,
            name: text(row, "name"),
            game_version: text(row, "game_version"),
            loader: text(row, "mod_loader"),
            loader_version: text(row, "mod_loader_version"),
            created_ms: time_ms(row, "created"),
            last_played_ms: time_ms(row, "last_played"),
        })
    })
}

#[cfg(test)]
pub(crate) mod fixtures {
    //! Modrinth App databases as its migrations leave them (the columns this module reads, and
    //! enough of the rest to look like the real thing).

    use std::path::Path;

    use rusqlite::Connection;

    /// A profile row: folder, name, Minecraft version, loader, loader version, last played
    /// (Unix seconds).
    pub type Profile<'a> = (
        &'a str,
        &'a str,
        &'a str,
        &'a str,
        Option<&'a str>,
        Option<i64>,
    );

    /// The tables of the 2024 layout, `profiles` with a profile per `(path, name, version,
    /// loader, loader version, last played)`.
    pub fn legacy(path: &Path, custom_dir: Option<&str>, profiles: &[Profile<'_>]) {
        let db = Connection::open(path).unwrap();
        db.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE settings (
                 id INTEGER NOT NULL CHECK (id = 0),
                 theme TEXT NOT NULL DEFAULT 'dark',
                 custom_dir TEXT NULL,
                 prev_custom_dir TEXT NULL,
                 migrated INTEGER NOT NULL DEFAULT FALSE,
                 PRIMARY KEY (id)
             );
             INSERT INTO settings (id) VALUES (0);
             CREATE TABLE minecraft_users (
                 uuid TEXT NOT NULL, active INTEGER NOT NULL DEFAULT FALSE,
                 username TEXT NOT NULL, access_token TEXT NOT NULL, refresh_token TEXT NOT NULL,
                 expires INTEGER NOT NULL, PRIMARY KEY (uuid)
             );
             INSERT INTO minecraft_users VALUES ('u', 1, 'Ejder', 'secret-token', 'secret-refresh', 0);
             CREATE TABLE profiles (
                 path TEXT NOT NULL,
                 install_stage TEXT NOT NULL,
                 name TEXT NOT NULL,
                 icon_path TEXT NULL,
                 game_version TEXT NOT NULL,
                 mod_loader TEXT NOT NULL,
                 mod_loader_version TEXT NULL,
                 -- array of strings
                 groups JSONB NOT NULL,
                 linked_project_id TEXT NULL,
                 linked_version_id TEXT NULL,
                 locked INTEGER NULL,
                 created INTEGER NOT NULL,
                 modified INTEGER NOT NULL,
                 last_played INTEGER NULL,
                 submitted_time_played INTEGER NOT NULL DEFAULT 0,
                 recent_time_played INTEGER NOT NULL DEFAULT 0,
                 PRIMARY KEY (path)
             );
             ALTER TABLE profiles ADD COLUMN protocol_version INTEGER NULL;",
        )
        .unwrap();
        db.execute("UPDATE settings SET custom_dir = ?1", [custom_dir])
            .unwrap();
        for (path, name, version, loader, loader_version, last_played) in profiles {
            db.execute(
                "INSERT INTO profiles (path, install_stage, name, game_version, mod_loader,
                     mod_loader_version, groups, created, modified, last_played)
                 VALUES (?1, 'installed', ?2, ?3, ?4, ?5, jsonb_array(), 1714564800, 1714564800, ?6)",
                rusqlite::params![path, name, version, loader, loader_version, last_played],
            )
            .unwrap();
        }
    }

    /// The 2026 layout: `instances` and their `instance_content_sets`.
    pub fn current(path: &Path, custom_dir: Option<&str>, profiles: &[Profile<'_>]) -> Connection {
        let db = Connection::open(path).unwrap();
        db.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE settings (
                 id INTEGER NOT NULL CHECK (id = 0),
                 theme TEXT NOT NULL DEFAULT 'dark',
                 custom_dir TEXT NULL,
                 prev_custom_dir TEXT NULL,
                 migrated INTEGER NOT NULL DEFAULT FALSE,
                 PRIMARY KEY (id)
             );
             INSERT INTO settings (id) VALUES (0);
             CREATE TABLE instances (
                 id TEXT NOT NULL,
                 path TEXT NOT NULL,
                 applied_content_set_id TEXT NULL,
                 install_stage TEXT NOT NULL,
                 launcher_feature_version TEXT NOT NULL,
                 update_channel TEXT NOT NULL DEFAULT 'release',
                 name TEXT NOT NULL,
                 icon_path TEXT NULL,
                 created INTEGER NOT NULL,
                 modified INTEGER NOT NULL,
                 last_played INTEGER NULL,
                 submitted_time_played INTEGER NOT NULL DEFAULT 0,
                 recent_time_played INTEGER NOT NULL DEFAULT 0,
                 PRIMARY KEY (id),
                 UNIQUE (path)
             );
             CREATE TABLE instance_content_sets (
                 id TEXT NOT NULL,
                 instance_id TEXT NOT NULL,
                 name TEXT NOT NULL,
                 source_kind TEXT NOT NULL,
                 status TEXT NOT NULL,
                 game_version TEXT NOT NULL,
                 protocol_version INTEGER NULL,
                 loader TEXT NOT NULL,
                 loader_version TEXT NULL,
                 created INTEGER NOT NULL,
                 modified INTEGER NOT NULL,
                 PRIMARY KEY (id),
                 FOREIGN KEY (instance_id) REFERENCES instances(id) ON DELETE CASCADE
             );",
        )
        .unwrap();
        db.execute("UPDATE settings SET custom_dir = ?1", [custom_dir])
            .unwrap();
        for (path, name, version, loader, loader_version, last_played) in profiles {
            let id = format!("legacy:{path}");
            let set = format!("{id}:default");
            db.execute(
                "INSERT INTO instances (id, path, applied_content_set_id, install_stage,
                     launcher_feature_version, name, created, modified, last_played)
                 VALUES (?1, ?2, ?3, 'installed', 'none', ?4, 1714564800, 1714564800, ?5)",
                rusqlite::params![id, path, set, name, last_played],
            )
            .unwrap();
            db.execute(
                "INSERT INTO instance_content_sets (id, instance_id, name, source_kind, status,
                     game_version, loader, loader_version, created, modified)
                 VALUES (?1, ?2, 'Default', 'local', 'available', ?3, ?4, ?5, 1714564800, 1714564800)",
                rusqlite::params![set, id, version, loader, loader_version],
            )
            .unwrap();
        }
        db
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROFILES: &[fixtures::Profile<'static>] = &[
        (
            "Fabulously Optimized",
            "Fabulously Optimized",
            "1.21.1",
            "fabric",
            Some("0.16.5"),
            Some(1_790_000_000),
        ),
        (
            "Create",
            "Create: Above and Beyond",
            "1.20.1",
            "forge",
            Some("1.20.1-47.3.0"),
            None,
        ),
        (
            "../outside",
            "Not a folder",
            "1.21.1",
            "vanilla",
            None,
            None,
        ),
    ];

    fn expected() -> Vec<DbProfile> {
        vec![
            DbProfile {
                path: "Fabulously Optimized".into(),
                name: Some("Fabulously Optimized".into()),
                game_version: Some("1.21.1".into()),
                loader: Some("fabric".into()),
                loader_version: Some("0.16.5".into()),
                created_ms: Some(1_714_564_800_000),
                last_played_ms: Some(1_790_000_000_000),
            },
            DbProfile {
                path: "Create".into(),
                name: Some("Create: Above and Beyond".into()),
                game_version: Some("1.20.1".into()),
                loader: Some("forge".into()),
                loader_version: Some("1.20.1-47.3.0".into()),
                created_ms: Some(1_714_564_800_000),
                last_played_ms: None,
            },
        ]
    }

    #[test]
    fn both_layouts_name_the_same_profiles() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("old");
        std::fs::create_dir_all(&old).unwrap();
        fixtures::legacy(&old.join("app.db"), None, PROFILES);
        let read = read(&old).unwrap();
        assert_eq!(read.profiles, expected());
        assert_eq!(read.profiles_root, None);

        let new = dir.path().join("new");
        std::fs::create_dir_all(&new).unwrap();
        let moved = dir.path().join("D").join("Modrinth");
        let live = fixtures::current(&new.join("app.db"), Some(moved.to_str().unwrap()), PROFILES);
        // Read while the app still runs and keeps it all in its log.
        let read = super::read(&new).unwrap();
        assert_eq!(read.profiles, expected());
        assert_eq!(read.profiles_root, Some(moved.join("profiles")));
        drop(live);
    }

    #[test]
    fn a_database_without_profiles_still_names_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        {
            let db = Connection::open(dir.path().join("app.db")).unwrap();
            db.execute_batch(
                r"CREATE TABLE settings (id INTEGER PRIMARY KEY, custom_dir TEXT NULL);
                  INSERT INTO settings VALUES (0, '\\?\D:\Oyunlar\Modrinth');",
            )
            .unwrap();
        }
        let read = read(dir.path()).unwrap();
        assert!(read.profiles.is_empty());
        if cfg!(windows) {
            assert_eq!(
                read.profiles_root,
                Some(PathBuf::from(r"D:\Oyunlar\Modrinth\profiles"))
            );
        }
        assert!(super::read(&dir.path().join("missing")).is_none());
    }

    #[test]
    fn folder_names_never_leave_the_profiles_folder() {
        assert_eq!(folder_name("Create".into()).as_deref(), Some("Create"));
        for bad in ["", ".", "..", "../x", r"a\b", "C:x"] {
            assert_eq!(folder_name(bad.into()), None, "{bad}");
        }
    }
}

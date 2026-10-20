//! Launchers the player moved: the Modrinth App's profiles in the folder its database names (the
//! app still open, its last changes only in the database's write-ahead log), CurseForge's
//! instances where its own instance list puts them. Both are found, come over with the versions
//! the launchers recorded, and stay exactly as they were.

use std::collections::BTreeMap;
use std::io::{Cursor, Write};
use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;

use pl_import::{
    CancellationToken, ImportRequest, ImportSource, Locations, Os, RunContext, SourceKind,
    SourceRequest,
};
use pl_mc::instance::InstanceStore;
use pl_mc::loaders::LoaderKind;
use rusqlite::Connection;
use zip::write::SimpleFileOptions;

fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

fn forge_jar(id: &str, range: &str) -> Vec<u8> {
    let toml = format!(
        "modLoader=\"javafml\"\nloaderVersion=\"[1,)\"\nlicense=\"MIT\"\n[[mods]]\nmodId=\"{id}\"\nversion=\"1.0.0\"\n[[dependencies.{id}]]\nmodId=\"minecraft\"\nmandatory=true\nversionRange=\"{range}\"\nside=\"BOTH\"\n"
    );
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file("META-INF/mods.toml", SimpleFileOptions::default())
        .unwrap();
    writer.write_all(toml.as_bytes()).unwrap();
    writer.finish().unwrap().into_inner()
}

/// Every file under `root` with its size, modification time and content.
fn snapshot(root: &Path) -> BTreeMap<String, (u64, SystemTime, Vec<u8>)> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
        .map(|e| {
            let meta = e.metadata().unwrap();
            (
                e.path().strip_prefix(root).unwrap().display().to_string(),
                (
                    meta.len(),
                    meta.modified().unwrap(),
                    std::fs::read(e.path()).unwrap(),
                ),
            )
        })
        .collect()
}

/// The Modrinth App, open: its database (the 2026 layout) names a moved app folder and two
/// profiles; an account with its sign-in sits in the same database.
fn modrinth_app(home: &Path, moved: &Path) -> Connection {
    let app = home.join("AppData/Roaming/ModrinthApp");
    std::fs::create_dir_all(&app).unwrap();
    let db = Connection::open(app.join("app.db")).unwrap();
    db.execute_batch(
        "PRAGMA journal_mode = WAL;
         CREATE TABLE settings (
             id INTEGER NOT NULL CHECK (id = 0), theme TEXT NOT NULL DEFAULT 'dark',
             custom_dir TEXT NULL, prev_custom_dir TEXT NULL, PRIMARY KEY (id)
         );
         INSERT INTO settings (id) VALUES (0);
         CREATE TABLE minecraft_users (
             uuid TEXT NOT NULL, active INTEGER NOT NULL DEFAULT FALSE, username TEXT NOT NULL,
             access_token TEXT NOT NULL, refresh_token TEXT NOT NULL, expires INTEGER NOT NULL,
             PRIMARY KEY (uuid)
         );
         INSERT INTO minecraft_users VALUES ('u', 1, 'Ejder', 'secret-token', 'secret-refresh', 0);
         CREATE TABLE instances (
             id TEXT NOT NULL, path TEXT NOT NULL, applied_content_set_id TEXT NULL,
             install_stage TEXT NOT NULL, launcher_feature_version TEXT NOT NULL,
             update_channel TEXT NOT NULL DEFAULT 'release', name TEXT NOT NULL,
             icon_path TEXT NULL, created INTEGER NOT NULL, modified INTEGER NOT NULL,
             last_played INTEGER NULL, submitted_time_played INTEGER NOT NULL DEFAULT 0,
             recent_time_played INTEGER NOT NULL DEFAULT 0, PRIMARY KEY (id), UNIQUE (path)
         );
         CREATE TABLE instance_content_sets (
             id TEXT NOT NULL, instance_id TEXT NOT NULL, name TEXT NOT NULL,
             source_kind TEXT NOT NULL, status TEXT NOT NULL, game_version TEXT NOT NULL,
             protocol_version INTEGER NULL, loader TEXT NOT NULL, loader_version TEXT NULL,
             created INTEGER NOT NULL, modified INTEGER NOT NULL, PRIMARY KEY (id)
         );
         INSERT INTO instances (id, path, applied_content_set_id, install_stage,
             launcher_feature_version, name, created, modified, last_played)
         VALUES ('a1', 'Create', 's1', 'installed', 'none', 'Create: Above and Beyond',
             1714564800, 1790000000, 1790000000),
            ('a2', 'Bos', 's2', 'not_installed', 'none', 'Boş profil', 1714564800, 1714564800, NULL);
         INSERT INTO instance_content_sets (id, instance_id, name, source_kind, status,
             game_version, loader, loader_version, created, modified)
         VALUES ('s1', 'a1', 'Default', 'local', 'available', '1.20.1', 'forge', '1.20.1-47.3.0',
             1714564800, 1714564800),
            ('s2', 'a2', 'Default', 'local', 'available', '1.21.1', 'fabric', '0.16.5',
             1714564800, 1714564800);",
    )
    .unwrap();
    db.execute(
        "UPDATE settings SET custom_dir = ?1",
        [moved.to_str().unwrap()],
    )
    .unwrap();
    db
}

fn source<'a>(sources: &'a [ImportSource], id: &str) -> &'a ImportSource {
    sources
        .iter()
        .find(|s| s.id == id)
        .unwrap_or_else(|| panic!("no source {id}"))
}

#[tokio::test]
async fn moved_launchers_are_found_brought_over_and_left_as_they_were() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");

    // Modrinth: profiles on another drive, the app open while the launcher looks.
    let moved = dir.path().join("D/Oyunlar/Modrinth");
    let create = moved.join("profiles/Create");
    write(
        &create.join("mods/create.jar"),
        forge_jar("create", "[1.20.1,1.21)"),
    );
    write(&create.join("config/create-common.toml"), "x=1");
    write(&create.join("options.txt"), "lang:tr_tr\n");
    write(&create.join("logs/latest.log"), "log");
    std::fs::create_dir_all(moved.join("profiles/Bos")).unwrap();
    let live = modrinth_app(&home, &moved);

    // CurseForge: its Minecraft folder moved; the app's own list says where.
    let instances = dir.path().join("E/CurseForge/Instances");
    let atm = instances.join("ATM9");
    write(
        &atm.join("minecraftinstance.json"),
        r#"{"name":"All the Mods 9","gameVersion":"1.20.1","baseModLoader":{"name":"forge-47.2.0","minecraftVersion":"1.20.1"},"lastPlayed":"2026-09-01T10:00:00Z"}"#,
    );
    write(&atm.join("mods/atm.jar"), forge_jar("atm", "[1.20.1,1.21)"));
    write(
        &home.join("AppData/Roaming/CurseForge/agent/GameInstances/MinecraftGameInstance.json"),
        serde_json::json!([{"name": "All the Mods 9", "installPath": format!("{}\\", atm.display())}])
            .to_string(),
    );

    let before = snapshot(dir.path());
    assert!(
        before.keys().any(|name| name.ends_with("app.db-wal")),
        "the open app keeps its changes in the log"
    );

    let locations = Locations::under(&home, Os::Windows);
    let sources = pl_import::scan(&locations, &[]);
    let modrinth = source(&sources, "modrinth");
    assert_eq!(modrinth.kind, SourceKind::Modrinth);
    let rows: Vec<(&str, Option<&str>, LoaderKind, Option<&str>)> = modrinth
        .profiles
        .iter()
        .map(|p| {
            (
                p.name.as_str(),
                p.minecraft.as_deref(),
                p.loader,
                p.loader_version.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            (
                "Create: Above and Beyond",
                Some("1.20.1"),
                LoaderKind::Forge,
                Some("47.3.0")
            ),
            (
                "Boş profil",
                Some("1.21.1"),
                LoaderKind::Fabric,
                Some("0.16.5")
            ),
        ]
    );
    let curseforge = source(&sources, "curseforge");
    assert_eq!(curseforge.profiles[0].name, "All the Mods 9");
    // Nothing of the accounts' sign-ins is in what the scan returns.
    let json = serde_json::to_string(&sources).unwrap();
    assert!(!json.contains("secret"));

    let data = tempfile::tempdir().unwrap();
    let store = InstanceStore::with_root(data.path().join("instances")).without_trash();
    let request = ImportRequest {
        sources: vec![
            SourceRequest {
                source_id: "modrinth".into(),
                shared: vec![],
                profiles: vec!["instance:Create".into()],
            },
            SourceRequest {
                source_id: "curseforge".into(),
                shared: vec![],
                profiles: vec!["instance:ATM9".into()],
            },
        ],
    };
    let report = pl_import::run_import(
        sources,
        request,
        RunContext {
            store: store.clone(),
            official: None,
            locker: Arc::new(|_: &str| Arc::new(tokio::sync::Mutex::new(()))),
            cancel: CancellationToken::new(),
            progress: Arc::new(|_| {}),
            reserve_bytes: 0,
        },
    )
    .await;
    assert!(!report.cancelled);
    assert_eq!(report.created.len(), 2, "{report:#?}");
    let made = store.list().await.unwrap();
    let create_instance = made
        .iter()
        .find(|i| i.name == "Create: Above and Beyond")
        .expect("Create came over");
    assert_eq!(create_instance.mc_version, "1.20.1");
    assert_eq!(create_instance.loader.kind, LoaderKind::Forge);
    assert_eq!(create_instance.loader.version.as_deref(), Some("47.3.0"));
    let game = store.game_dir(&create_instance.id).unwrap();
    assert!(game.join("mods/create.jar").is_file());
    assert!(game.join("config/create-common.toml").is_file());
    assert!(!game.join("logs").exists(), "logs stay behind");
    let atm_instance = made.iter().find(|i| i.name == "All the Mods 9").unwrap();
    assert_eq!(atm_instance.loader.version.as_deref(), Some("47.2.0"));
    assert!(
        store
            .game_dir(&atm_instance.id)
            .unwrap()
            .join("mods/atm.jar")
            .is_file()
    );

    // Every file of both launchers (the open database, its log and shared memory included) is
    // exactly as it was; nothing was added next to them.
    assert_eq!(snapshot(dir.path()), before);

    // Scanned again, both are known as brought over.
    let imported = pl_import::imported_profiles(store.root());
    let again = pl_import::scan(&locations, &imported);
    let create_row = source(&again, "modrinth")
        .profiles
        .iter()
        .find(|p| p.id == "instance:Create")
        .unwrap();
    assert_eq!(
        create_row.imported_as.as_deref(),
        Some(create_instance.id.as_str())
    );
    assert!(
        source(&again, "curseforge").profiles[0]
            .imported_as
            .is_some()
    );
    drop(live);
}

//! The whole import against a pretend home folder: finding the launchers, measuring, copying
//! into a pretend data folder, running again, cancelling. The other launchers' folders must
//! come out of it exactly as they went in.

use std::collections::BTreeMap;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use pl_import::{
    CancellationToken, ImportProgress, ImportReport, ImportRequest, ImportShared, ImportSource,
    ItemRef, Locations, OfficialTarget, Os, OutcomeReason, OutcomeStatus, RunContext, SourceKind,
    SourceRequest, Unsupported,
};
use pl_mc::instance::{InstanceLoader, InstanceStore, ManagedPack, NewInstance};
use pl_mc::loaders::LoaderKind;
use pl_mc::server::{ServerList, ServerListEntry};
use zip::write::SimpleFileOptions;

fn zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        writer
            .start_file(*name, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn fabric_jar(id: &str, minecraft: Option<&str>) -> Vec<u8> {
    let depends = minecraft
        .map(|mc| format!(r#", "depends": {{"minecraft": "{mc}"}}"#))
        .unwrap_or_default();
    let json = format!(r#"{{"schemaVersion": 1, "id": "{id}", "version": "1.0.0"{depends}}}"#);
    zip_bytes(&[("fabric.mod.json", json.as_bytes())])
}

fn forge_jar(id: &str, range: &str) -> Vec<u8> {
    let toml = format!(
        "modLoader=\"javafml\"\nloaderVersion=\"[1,)\"\nlicense=\"MIT\"\n[[mods]]\nmodId=\"{id}\"\nversion=\"1.0.0\"\n[[dependencies.{id}]]\nmodId=\"minecraft\"\nmandatory=true\nversionRange=\"{range}\"\nside=\"BOTH\"\n"
    );
    zip_bytes(&[("META-INF/mods.toml", toml.as_bytes())])
}

fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

fn level_dat(version: Option<&str>, brands: &[&str]) -> Vec<u8> {
    use fastnbt::Value;
    use std::collections::HashMap;
    let mut data = HashMap::new();
    if let Some(version) = version {
        let mut v = HashMap::new();
        v.insert("Name".to_owned(), Value::String(version.to_owned()));
        data.insert("Version".to_owned(), Value::Compound(v));
    }
    data.insert(
        "ServerBrands".to_owned(),
        Value::List(
            brands
                .iter()
                .map(|b| Value::String((*b).to_owned()))
                .collect(),
        ),
    );
    data.insert(
        "WasModded".to_owned(),
        Value::Byte(i8::from(brands.iter().any(|b| *b != "vanilla"))),
    );
    let mut root = HashMap::new();
    root.insert("Data".to_owned(), Value::Compound(data));
    let nbt = fastnbt::to_bytes(&Value::Compound(root)).unwrap();
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    gz.write_all(&nbt).unwrap();
    gz.finish().unwrap()
}

fn world(saves: &Path, name: &str, version: Option<&str>, brands: &[&str]) {
    write(
        &saves.join(name).join("level.dat"),
        level_dat(version, brands),
    );
    write(
        &saves.join(name).join("region/r.0.0.mca"),
        vec![3u8; 64 * 1024],
    );
    write(&saves.join(name).join("session.lock"), "lock");
}

fn servers(entries: &[(&str, &str)]) -> Vec<u8> {
    let mut list = ServerList::default();
    for (name, ip) in entries {
        list.servers.push(ServerListEntry::new(*name, *ip));
    }
    list.to_nbt().unwrap()
}

fn version_json(dir: &Path, id: &str, json: &str) {
    write(
        &dir.join("versions").join(id).join(format!("{id}.json")),
        json,
    );
}

/// A TLauncher `.minecraft`, a CurseForge instance, a Prism instance and a Modrinth profile.
fn home(root: &Path) -> PathBuf {
    let mc = root.join("AppData/Roaming/.minecraft");
    write(
        &mc.join("options.txt"),
        "version:3955\nlang:tr_tr\nguiScale:3\nkey_key.sprint:key.keyboard.caps.lock\nkey_key.drop:key.keyboard.q\n",
    );
    write(
        &mc.join("servers.dat"),
        servers(&[
            ("Hypixel", "mc.hypixel.net"),
            ("Arkadaşlar", "192.168.1.20"),
        ]),
    );
    write(
        &mc.join("TlauncherProfiles.json"),
        r#"{"accounts":{"u1":{"username":"Ejder_42","type":"FREE","accessToken":"secret-token"}},"selectedAccountUUID":"u1"}"#,
    );
    write(
        &root.join("AppData/Roaming/.tlauncher/tlauncher-2.0.properties"),
        "login.version.game=1.8.9\nlogin.account=Ejder_42\nlogin.account.type=FREE\n",
    );
    write(
        &mc.join("launcher_profiles.json"),
        r#"{"profiles":{
            "abc":{"name":"Benim modlarım","type":"custom","lastVersionId":"fabric-loader-0.16.9-1.21.4","lastUsed":"2026-10-01T10:00:00.000Z"},
            "latest":{"name":"","type":"latest-release","lastVersionId":"latest-release"}
        },"authenticationDatabase":{"x":{"accessToken":"secret-token"}}}"#,
    );
    version_json(
        &mc,
        "1.21.4",
        r#"{"id":"1.21.4","mainClass":"net.minecraft.client.main.Main"}"#,
    );
    version_json(
        &mc,
        "fabric-loader-0.16.9-1.21.4",
        r#"{"id":"fabric-loader-0.16.9-1.21.4","inheritsFrom":"1.21.4",
            "mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient",
            "libraries":[{"name":"net.fabricmc:fabric-loader:0.16.9"},{"name":"net.fabricmc:intermediary:1.21.4"}]}"#,
    );
    version_json(
        &mc,
        "1.20.1",
        r#"{"id":"1.20.1","mainClass":"net.minecraft.client.main.Main"}"#,
    );
    version_json(
        &mc,
        "1.20.1-forge-47.3.0",
        r#"{"id":"1.20.1-forge-47.3.0","inheritsFrom":"1.20.1",
            "mainClass":"cpw.mods.bootstraplauncher.BootstrapLauncher",
            "arguments":{"game":["--fml.forgeVersion","47.3.0","--fml.mcVersion","1.20.1"]},
            "libraries":[{"name":"net.minecraftforge:fmlloader:1.20.1-47.3.0"}]}"#,
    );
    version_json(
        &mc,
        "1.8.9",
        r#"{"id":"1.8.9","mainClass":"net.minecraft.client.main.Main"}"#,
    );
    version_json(
        &mc,
        "1.12.2-LiteLoader1.12.2",
        r#"{"id":"1.12.2-LiteLoader1.12.2","inheritsFrom":"1.12.2",
            "mainClass":"net.minecraft.launchwrapper.Launch",
            "libraries":[{"name":"com.mumfrey:liteloader:1.12.2-SNAPSHOT"}]}"#,
    );
    write(
        &mc.join("mods/sodium.jar"),
        fabric_jar("sodium", Some("~1.21.4")),
    );
    write(
        &mc.join("mods/fabric-api.jar"),
        fabric_jar("fabric-api", None),
    );
    write(&mc.join("mods/old.jar"), fabric_jar("old", Some("1.20.1")));
    write(
        &mc.join("mods/create.jar"),
        forge_jar("create", "[1.20.1,1.20.2)"),
    );
    write(&mc.join("mods/broken.jar"), "not a zip");
    write(&mc.join("config/sodium-options.json"), "{}");
    world(&mc.join("saves"), "Survival", Some("1.21.4"), &["vanilla"]);
    world(
        &mc.join("saves"),
        "Create World",
        Some("1.20.1"),
        &["forge"],
    );
    world(&mc.join("saves"), "Old", None, &[]);
    write(&mc.join("resourcepacks/Faithful.zip"), vec![1u8; 2048]);
    write(&mc.join("resourcepacks/MyPack/pack.mcmeta"), "{}");
    write(&mc.join("shaderpacks/BSL.zip"), vec![2u8; 4096]);
    write(&mc.join("shaderpacks/BSL.zip.txt"), "shadowDistance=8");
    write(
        &mc.join("screenshots/2026-01-01_12.00.00.png"),
        b"\x89PNG one",
    );
    write(
        &mc.join("screenshots/2026-01-02_12.00.00.png"),
        b"\x89PNG two",
    );

    let cf = root.join("curseforge/minecraft/Instances/ATM9");
    write(
        &cf.join("minecraftinstance.json"),
        r#"{"name":"All the Mods 9","gameVersion":"1.20.1","baseModLoader":{"name":"forge-47.2.0","minecraftVersion":"1.20.1"},"lastPlayed":"2026-09-01T10:00:00Z","installedAddons":[{"addonID":1}]}"#,
    );
    write(&cf.join("mods/atm.jar"), forge_jar("atm", "[1.20.1,1.21)"));
    write(&cf.join("config/atm.toml"), "x=1");
    write(&cf.join("options.txt"), "lang:en_us\n");
    write(
        &cf.join("servers.dat"),
        servers(&[("ATM", "atm.example.org")]),
    );
    write(&cf.join("logs/latest.log"), "log");
    world(&cf.join("saves"), "ATM World", Some("1.20.1"), &["forge"]);

    let prism = root.join("AppData/Roaming/PrismLauncher/instances/FO");
    write(
        &prism.join("instance.cfg"),
        "[General]\nname=Fabulously Optimized\nlastLaunchTime=1790000000000\n",
    );
    write(
        &prism.join("mmc-pack.json"),
        r#"{"components":[{"uid":"net.minecraft","version":"1.21.1"},{"uid":"net.fabricmc.fabric-loader","version":"0.16.5"}]}"#,
    );
    write(
        &prism.join(".minecraft/mods/fo.jar"),
        fabric_jar("fo", Some("1.21.1")),
    );
    write(&prism.join(".minecraft/options.txt"), "lang:tr_tr\n");

    let modrinth = root.join("AppData/Roaming/ModrinthApp/profiles/Vanilla Plus");
    write(
        &modrinth.join("logs/latest.log"),
        "[12:00:00] [main/INFO]: Loading Minecraft 1.21.1 with Fabric Loader 0.16.5\n",
    );
    write(
        &modrinth.join("mods/vp.jar"),
        fabric_jar("vp", Some("1.21.1")),
    );
    root.to_path_buf()
}

/// Every file under `root` with its size and modification time.
fn snapshot(root: &Path) -> BTreeMap<String, (u64, SystemTime)> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
        .map(|e| {
            let meta = e.metadata().unwrap();
            (
                e.path().strip_prefix(root).unwrap().display().to_string(),
                (meta.len(), meta.modified().unwrap()),
            )
        })
        .collect()
}

fn source<'a>(sources: &'a [ImportSource], id: &str) -> &'a ImportSource {
    sources.iter().find(|s| s.id == id).unwrap()
}

fn profile_id(source: &ImportSource, name: &str) -> String {
    source
        .profiles
        .iter()
        .find(|p| p.name == name)
        .unwrap_or_else(|| panic!("no profile {name}"))
        .id
        .clone()
}

struct Data {
    _dir: tempfile::TempDir,
    store: InstanceStore,
    official: OfficialTarget,
}

async fn data() -> Data {
    let dir = tempfile::tempdir().unwrap();
    let store = InstanceStore::with_root(dir.path().join("instances")).without_trash();
    let projects = store
        .create(NewInstance {
            name: "Projects".into(),
            mc_version: "26.1.2".into(),
            loader: InstanceLoader {
                kind: LoaderKind::Fabric,
                version: None,
            },
            managed_pack: Some(ManagedPack {
                pack_id: "projects".into(),
                version: String::new(),
            }),
            ..NewInstance::default()
        })
        .await
        .unwrap();
    Data {
        _dir: dir,
        official: OfficialTarget {
            id: projects.id,
            name: projects.name,
            minecraft: "26.1.2".into(),
            official_pack: true,
        },
        store,
    }
}

fn context(
    data: &Data,
    cancel: CancellationToken,
    on: impl Fn(ImportProgress) + Send + Sync + 'static,
) -> RunContext {
    RunContext {
        store: data.store.clone(),
        official: Some(data.official.clone()),
        locker: Arc::new(|_: &str| Arc::new(tokio::sync::Mutex::new(()))),
        cancel,
        progress: Arc::new(on),
        reserve_bytes: 0,
    }
}

fn everything(sources: &[ImportSource]) -> ImportRequest {
    let mc = source(sources, "minecraft");
    ImportRequest {
        sources: vec![
            SourceRequest {
                source_id: "minecraft".into(),
                shared: vec![
                    ImportShared::Options,
                    ImportShared::Servers,
                    ImportShared::Worlds,
                    ImportShared::ResourcePacks,
                    ImportShared::ShaderPacks,
                    ImportShared::Screenshots,
                ],
                profiles: vec![
                    profile_id(mc, "Benim modlarım"),
                    profile_id(mc, "Forge 1.20.1"),
                    profile_id(mc, "Minecraft 1.8.9"),
                    profile_id(mc, "LiteLoader 1.12.2"),
                ],
            },
            SourceRequest {
                source_id: "curseforge".into(),
                shared: vec![],
                profiles: vec!["instance:ATM9".into()],
            },
            SourceRequest {
                source_id: "prism".into(),
                shared: vec![],
                profiles: vec!["instance:FO".into()],
            },
        ],
    }
}

fn outcome<'a>(report: &'a ImportReport, item: &ItemRef) -> &'a pl_import::ItemOutcome {
    report
        .outcomes
        .iter()
        .find(|o| &o.item == item)
        .unwrap_or_else(|| panic!("no outcome for {item:?}"))
}

fn shared(kind: ImportShared) -> ItemRef {
    ItemRef::Shared { kind }
}

async fn instance_named(store: &InstanceStore, name: &str) -> pl_mc::instance::Instance {
    store
        .list()
        .await
        .unwrap()
        .into_iter()
        .find(|i| i.name == name)
        .unwrap_or_else(|| panic!("no instance {name}"))
}

fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|e| {
            e.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

#[test]
fn every_launcher_is_found_without_reading_anything_big() {
    let dir = tempfile::tempdir().unwrap();
    let root = home(dir.path());
    let sources = pl_import::scan(&Locations::under(&root, Os::Windows), &[]);
    let ids: Vec<&str> = sources.iter().map(|s| s.id.as_str()).collect();
    // The .minecraft folder first, then the instance launchers by last use.
    assert_eq!(ids[0], "minecraft");
    assert_eq!(ids.len(), 4, "{ids:?}");
    assert!(ids.contains(&"curseforge") && ids.contains(&"prism") && ids.contains(&"modrinth"));

    let mc = source(&sources, "minecraft");
    assert_eq!(mc.kind, SourceKind::TLauncher);
    assert_eq!(mc.offline_name.as_deref(), Some("Ejder_42"));
    let shared = mc.shared.as_ref().unwrap();
    assert!(shared.options);
    assert_eq!(
        (
            shared.servers,
            shared.worlds,
            shared.resource_packs,
            shared.shader_packs,
            shared.screenshots
        ),
        (2, 3, 2, 1, 2)
    );
    let rows: Vec<(&str, Option<&str>, LoaderKind)> = mc
        .profiles
        .iter()
        .map(|p| (p.name.as_str(), p.minecraft.as_deref(), p.loader))
        .collect();
    // The player's launcher profile keeps its name; the vanilla versions loaders stand on are
    // not listed; the version TLauncher started last is.
    assert!(rows.contains(&("Benim modlarım", Some("1.21.4"), LoaderKind::Fabric)));
    assert!(rows.contains(&("Forge 1.20.1", Some("1.20.1"), LoaderKind::Forge)));
    assert!(rows.contains(&("Minecraft 1.8.9", Some("1.8.9"), LoaderKind::Vanilla)));
    assert!(
        !rows
            .iter()
            .any(|r| r.0 == "Minecraft 1.21.4" || r.0 == "Minecraft 1.20.1")
    );
    let lite = mc
        .profiles
        .iter()
        .find(|p| p.name == "LiteLoader 1.12.2")
        .unwrap();
    assert_eq!(
        lite.unsupported,
        Some(Unsupported::Loader {
            name: "LiteLoader".into()
        })
    );
    // TLauncher started 1.8.9 last; the player's own launcher profile was played before; what
    // cannot come over is listed last.
    assert_eq!(mc.profiles[0].name, "Minecraft 1.8.9");
    assert_eq!(mc.profiles[1].name, "Benim modlarım");
    assert_eq!(mc.profiles.last().unwrap().name, "LiteLoader 1.12.2");

    let cf = source(&sources, "curseforge");
    assert_eq!(cf.profiles[0].name, "All the Mods 9");
    assert_eq!(cf.profiles[0].loader_version.as_deref(), Some("47.2.0"));
    assert_eq!(cf.profiles[0].mod_files, 1);
    let modrinth = source(&sources, "modrinth");
    assert_eq!(modrinth.profiles[0].minecraft.as_deref(), Some("1.21.1"));
    assert_eq!(modrinth.profiles[0].loader, LoaderKind::Fabric);

    // Nothing of the accounts' sign-ins is in what the scan returns.
    let json = serde_json::to_string(&sources).unwrap();
    assert!(!json.contains("secret-token"));
    assert!(
        !json.contains("AppData\\\\Roaming\\\\.minecraft\\\\mods"),
        "paths stay out of the rows"
    );

    // A home without any launcher has nothing to offer.
    let empty = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(empty.path().join("AppData/Roaming/.minecraft")).unwrap();
    write(
        &empty
            .path()
            .join("AppData/Roaming/.minecraft/launcher_profiles.json"),
        "{}",
    );
    assert!(pl_import::scan(&Locations::under(empty.path(), Os::Windows), &[]).is_empty());
}

#[test]
fn measuring_counts_the_mods_that_fit() {
    let dir = tempfile::tempdir().unwrap();
    let root = home(dir.path());
    let sources = pl_import::scan(&Locations::under(&root, Os::Windows), &[]);
    let mc = source(&sources, "minecraft");
    let measured = pl_import::measure(mc);
    let sizes = measured.shared.unwrap();
    assert!(sizes.worlds >= 3 * 64 * 1024);
    assert_eq!(sizes.shader_packs, 4096 + 16);
    let by_name = |name: &str| {
        let id = profile_id(mc, name);
        measured
            .profiles
            .iter()
            .find(|p| p.id == id)
            .unwrap()
            .clone()
    };
    let fabric = by_name("Benim modlarım");
    // sodium and fabric-api fit; the Forge mod, the 1.20.1 mod and the broken file stay.
    assert_eq!((fabric.mods, fabric.left_mods), (2, 3));
    let forge = by_name("Forge 1.20.1");
    assert_eq!((forge.mods, forge.left_mods), (1, 4));
    let vanilla = by_name("Minecraft 1.8.9");
    assert_eq!((vanilla.mods, vanilla.left_mods, vanilla.bytes), (0, 0, 0));
    let cf = pl_import::measure(source(&sources, "curseforge"));
    assert_eq!((cf.profiles[0].mods, cf.profiles[0].worlds), (1, 1));
}

#[tokio::test]
async fn everything_comes_over_once_and_the_sources_stay_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let root = home(dir.path());
    let before = snapshot(&root);
    let locations = Locations::under(&root, Os::Windows);
    let data = data().await;
    let sources = pl_import::scan(&locations, &[]);
    let request = everything(&sources);
    let progress: Arc<Mutex<Vec<ImportProgress>>> = Arc::default();
    let seen = progress.clone();
    let report = pl_import::run_import(
        sources,
        request.clone(),
        context(&data, CancellationToken::new(), move |p| {
            seen.lock().unwrap().push(p)
        }),
    )
    .await;
    assert!(!report.cancelled);

    // Game settings: the player's lines, plus the pack keys they lack; push to talk would share
    // their sprint key (Caps Lock) and stays unbound.
    let projects_game = data.store.game_dir(&data.official.id).unwrap();
    let options = std::fs::read_to_string(projects_game.join("options.txt")).unwrap();
    assert!(options.starts_with("version:3955\nlang:tr_tr\nguiScale:3\n"));
    assert!(options.contains("key_key.wi_zoom.zoom:key.keyboard.c\n"));
    assert!(options.contains("key_key.push_to_talk:key.keyboard.unknown\n"));
    assert_eq!(
        outcome(&report, &shared(ImportShared::Options)).status,
        OutcomeStatus::Done
    );

    // Servers: Projects first, then theirs.
    let list =
        ServerList::from_nbt(&std::fs::read(projects_game.join("servers.dat")).unwrap()).unwrap();
    let ips: Vec<&str> = list.servers.iter().map(|s| s.ip.as_str()).collect();
    assert_eq!(ips, ["play.projects.gg", "mc.hypixel.net", "192.168.1.20"]);
    assert_eq!(outcome(&report, &shared(ImportShared::Servers)).copied, 2);

    // Profiles became instances with the mods that fit.
    let fabric = instance_named(&data.store, "Benim modlarım").await;
    assert_eq!(
        (fabric.mc_version.as_str(), fabric.loader.kind),
        ("1.21.4", LoaderKind::Fabric)
    );
    assert_eq!(fabric.loader.version.as_deref(), Some("0.16.9"));
    assert!(fabric.default_server_seeded);
    let fabric_game = data.store.game_dir(&fabric.id).unwrap();
    assert_eq!(
        names_in(&fabric_game.join("mods")),
        ["fabric-api.jar", "sodium.jar"]
    );
    assert!(fabric_game.join("config/sodium-options.json").is_file());
    assert!(fabric_game.join("options.txt").is_file());
    let fabric_outcome = report
        .outcomes
        .iter()
        .find(|o| o.label == "Benim modlarım")
        .unwrap();
    assert_eq!(fabric_outcome.status, OutcomeStatus::Partial);
    assert_eq!(fabric_outcome.reason, Some(OutcomeReason::IncompatibleMods));
    assert_eq!((fabric_outcome.copied, fabric_outcome.left), (2, 3));

    let forge = instance_named(&data.store, "Forge 1.20.1").await;
    let forge_game = data.store.game_dir(&forge.id).unwrap();
    assert_eq!(names_in(&forge_game.join("mods")), ["create.jar"]);
    let vanilla = instance_named(&data.store, "Minecraft 1.8.9").await;
    assert_eq!(vanilla.loader.kind, LoaderKind::Vanilla);
    let lite = report
        .outcomes
        .iter()
        .find(|o| o.label == "LiteLoader 1.12.2")
        .unwrap();
    assert_eq!(
        (lite.status, lite.reason),
        (OutcomeStatus::Skipped, Some(OutcomeReason::Unsupported))
    );

    // Worlds follow the instance they were played with; the others go to Projects.
    assert_eq!(names_in(&forge_game.join("saves")), ["Create World"]);
    assert_eq!(names_in(&projects_game.join("saves")), ["Old", "Survival"]);
    assert!(!projects_game.join("saves/Survival/session.lock").exists());
    let worlds = outcome(&report, &shared(ImportShared::Worlds));
    assert_eq!((worlds.copied, worlds.left), (3, 0));
    assert_eq!(worlds.destinations.len(), 2);

    // Packs, shader settings and screenshots go to Projects.
    assert_eq!(
        names_in(&projects_game.join("resourcepacks")),
        ["Faithful.zip", "MyPack"]
    );
    assert_eq!(
        names_in(&projects_game.join("shaderpacks")),
        ["BSL.zip", "BSL.zip.txt"]
    );
    assert_eq!(names_in(&projects_game.join("screenshots")).len(), 2);

    // A CurseForge instance comes as a whole, without its logs; its servers get Projects.
    let atm = instance_named(&data.store, "All the Mods 9").await;
    assert_eq!(atm.loader.version.as_deref(), Some("47.2.0"));
    let atm_game = data.store.game_dir(&atm.id).unwrap();
    assert!(atm_game.join("mods/atm.jar").is_file());
    assert!(atm_game.join("config/atm.toml").is_file());
    assert!(atm_game.join("saves/ATM World/level.dat").is_file());
    assert!(!atm_game.join("logs").exists());
    assert!(!atm_game.join("minecraftinstance.json").exists());
    let atm_servers =
        ServerList::from_nbt(&std::fs::read(atm_game.join("servers.dat")).unwrap()).unwrap();
    let atm_ips: Vec<&str> = atm_servers.servers.iter().map(|s| s.ip.as_str()).collect();
    assert_eq!(atm_ips, ["play.projects.gg", "atm.example.org"]);
    let fo = instance_named(&data.store, "Fabulously Optimized").await;
    assert_eq!(fo.mc_version, "1.21.1");
    assert_eq!(report.created.len(), 5);

    // Progress went from preparing to finishing and reached every item.
    {
        let seen = progress.lock().unwrap();
        assert_eq!(
            seen.first().unwrap().stage,
            pl_import::ImportStage::Preparing
        );
        let last = seen.last().unwrap();
        assert_eq!(last.stage, pl_import::ImportStage::Finishing);
        assert_eq!(last.done_items, last.total_items);
        assert!(last.done_bytes >= last.total_bytes);
    }

    // The other launchers' folders are exactly as they were.
    assert_eq!(snapshot(&root), before);

    // Running it again copies nothing and makes nothing twice.
    let instances_before = data.store.list().await.unwrap().len();
    let imported = pl_import::imported_profiles(data.store.root());
    let again_sources = pl_import::scan(&locations, &imported);
    let again = pl_import::run_import(
        again_sources.clone(),
        everything(&again_sources),
        context(&data, CancellationToken::new(), |_| {}),
    )
    .await;
    assert_eq!(data.store.list().await.unwrap().len(), instances_before);
    for o in &again.outcomes {
        assert_eq!(o.status, OutcomeStatus::Skipped, "{o:?}");
        assert!(
            matches!(
                o.reason,
                Some(OutcomeReason::AlreadyThere | OutcomeReason::Unsupported)
            ),
            "{o:?}"
        );
    }
    assert_eq!(names_in(&projects_game.join("saves")), ["Old", "Survival"]);
    assert_eq!(names_in(&projects_game.join("screenshots")).len(), 2);
    assert_eq!(snapshot(&root), before);
}

#[tokio::test]
async fn the_projects_instances_own_settings_stay_and_nothing_is_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let root = home(dir.path());
    let data = data().await;
    let projects_game = data.store.game_dir(&data.official.id).unwrap();
    write(&projects_game.join("options.txt"), "lang:en_us\n");
    write(
        &projects_game.join("saves/Survival/level.dat"),
        "a different world",
    );
    let sources = pl_import::scan(&Locations::under(&root, Os::Windows), &[]);
    let report = pl_import::run_import(
        sources,
        ImportRequest {
            sources: vec![SourceRequest {
                source_id: "minecraft".into(),
                shared: vec![ImportShared::Options, ImportShared::Worlds],
                profiles: vec![],
            }],
        },
        context(&data, CancellationToken::new(), |_| {}),
    )
    .await;
    let options = outcome(&report, &shared(ImportShared::Options));
    assert_eq!(
        (options.status, options.reason),
        (OutcomeStatus::Skipped, Some(OutcomeReason::KeptExisting))
    );
    assert_eq!(
        std::fs::read_to_string(projects_game.join("options.txt")).unwrap(),
        "lang:en_us\n"
    );
    // Their own "Survival" stays; the imported one gets a free name. With no Forge instance
    // made, the Forge world goes to Projects too.
    assert_eq!(
        std::fs::read(projects_game.join("saves/Survival/level.dat")).unwrap(),
        b"a different world"
    );
    assert_eq!(
        names_in(&projects_game.join("saves")),
        ["Create World", "Old", "Survival", "Survival (2)"]
    );
}

#[tokio::test]
async fn a_cancelled_import_leaves_only_whole_copies() {
    let dir = tempfile::tempdir().unwrap();
    let root = home(dir.path());
    let before = snapshot(&root);
    let data = data().await;
    let sources = pl_import::scan(&Locations::under(&root, Os::Windows), &[]);
    let request = everything(&sources);
    let cancel = CancellationToken::new();
    let stop = cancel.clone();
    // Stop as soon as the first bytes move.
    let report = pl_import::run_import(
        sources,
        request,
        context(&data, cancel, move |p| {
            if p.done_bytes > 0 {
                stop.cancel();
            }
        }),
    )
    .await;
    assert!(report.cancelled);
    assert!(
        report
            .outcomes
            .iter()
            .any(|o| o.reason == Some(OutcomeReason::Cancelled))
    );
    // No temporary leftovers, and every world that is there is whole.
    for entry in walkdir::WalkDir::new(data.store.root())
        .into_iter()
        .flatten()
    {
        let name = entry.file_name().to_string_lossy();
        assert!(
            !name.contains(".pl-import-"),
            "leftover {}",
            entry.path().display()
        );
    }
    for instance in data.store.list().await.unwrap() {
        let saves = data.store.game_dir(&instance.id).unwrap().join("saves");
        for world in names_in(&saves) {
            assert!(
                saves.join(&world).join("level.dat").is_file(),
                "{world} is not whole"
            );
        }
    }
    // A cancelled profile leaves no half instance: every instance made is in the report.
    let made: Vec<String> = report.created.iter().map(|c| c.id.clone()).collect();
    for instance in data.store.list().await.unwrap() {
        assert!(
            instance.id == data.official.id || made.contains(&instance.id),
            "{}",
            instance.id
        );
    }
    assert_eq!(snapshot(&root), before);
}

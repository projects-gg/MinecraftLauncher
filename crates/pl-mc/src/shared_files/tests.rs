use std::fs;
use std::time::{Duration, SystemTime};

use super::*;

const DAY: Duration = Duration::from_secs(24 * 60 * 60);

fn write(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

/// Scans "two days from now", so everything the fixture just wrote counts as old.
fn later() -> ScanOptions {
    ScanOptions {
        now: SystemTime::now() + 2 * DAY,
        min_age: DAY,
        protected: Vec::new(),
    }
}

fn fabric(mc: &str, loader: &str) -> GameRef {
    GameRef::new(mc, Some(LoaderKind::Fabric), Some(loader.to_owned()))
}

struct Fixture {
    _tmp: tempfile::TempDir,
    dirs: DataDirs,
}

impl Fixture {
    fn lib(&self, rel: &str) -> PathBuf {
        self.dirs.libraries().join(rel)
    }

    fn version(&self, id: &str) -> PathBuf {
        self.dirs.versions().join(id)
    }
}

/// A data folder with two game versions, three loader profiles on them, Java runtimes,
/// libraries, assets and natives.
fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = DataDirs::new(tmp.path().join("root"));
    dirs.ensure().unwrap();
    let json = |id: &str, body: &str| {
        write(
            &dirs.version_json(id).unwrap(),
            format!(r#"{{"id":"{id}",{body}}}"#).as_bytes(),
        );
    };
    json(
        "1.21.1",
        r#""mainClass":"net.minecraft.client.main.Main","assetIndex":{"id":"17"},
        "javaVersion":{"component":"java-runtime-delta","majorVersion":21},
        "logging":{"client":{"argument":"-Dlog4j.configurationFile=${path}","file":{"id":"client-1.12.xml"}}},
        "libraries":[
          {"name":"com.example:shared:1.0","downloads":{"artifact":{"path":"com/example/shared/1.0/shared-1.0.jar","url":"https://x/shared.jar"}}},
          {"name":"org.lwjgl:lwjgl:3.3.3:natives-windows","downloads":{"artifact":{"path":"org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar","url":"https://x/l.jar"}},"rules":[{"action":"allow","os":{"name":"windows"}}]}
        ]"#,
    );
    json(
        "1.20.1",
        r#""mainClass":"net.minecraft.client.main.Main","assetIndex":{"id":"5"},
        "javaVersion":{"component":"java-runtime-gamma","majorVersion":17},
        "libraries":[
          {"name":"com.example:shared:1.0","downloads":{"artifact":{"path":"com/example/shared/1.0/shared-1.0.jar","url":"https://x/shared.jar"}}},
          {"name":"org.lwjgl.lwjgl:lwjgl-platform:2.9.4","natives":{"windows":"natives-windows-${arch}"}}
        ]"#,
    );
    json(
        "fabric-loader-0.16.5-1.21.1",
        r#""inheritsFrom":"1.21.1","mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient",
        "libraries":[{"name":"net.fabricmc:fabric-loader:0.16.5","url":"https://maven.fabricmc.net/"}]"#,
    );
    json(
        "fabric-loader-0.15.0-1.20.1",
        r#""inheritsFrom":"1.20.1","mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient",
        "libraries":[{"name":"net.fabricmc:fabric-loader:0.15.0","url":"https://maven.fabricmc.net/"}]"#,
    );
    json(
        "1.20.1-forge-47.2.0",
        r#""inheritsFrom":"1.20.1","mainClass":"cpw.mods.bootstraplauncher.BootstrapLauncher",
        "libraries":[{"name":"net.minecraftforge:forge:1.20.1-47.2.0:client","downloads":{"artifact":{"path":"net/minecraftforge/forge/1.20.1-47.2.0/forge-1.20.1-47.2.0-client.jar","url":""}}}]"#,
    );
    write(
        &dirs
            .version_dir("1.20.1-forge-47.2.0")
            .unwrap()
            .join("projects-loader.json"),
        br#"{"format":1,"kind":"forge","minecraft":"1.20.1","loader":"47.2.0","files":[
          {"path":"libraries/net/minecraft/client/1.20.1-srg/client-1.20.1-srg.jar","sha1":"x","size":3}]}"#,
    );
    // Game jars: profiles hold hard links to their game version's jar.
    let jar_121 = dirs.version_jar("1.21.1").unwrap();
    write(&jar_121, &[1u8; 1000]);
    fs::hard_link(
        &jar_121,
        dirs.version_jar("fabric-loader-0.16.5-1.21.1").unwrap(),
    )
    .unwrap();
    let jar_120 = dirs.version_jar("1.20.1").unwrap();
    write(&jar_120, &[2u8; 800]);
    for profile in ["fabric-loader-0.15.0-1.20.1", "1.20.1-forge-47.2.0"] {
        fs::hard_link(&jar_120, dirs.version_jar(profile).unwrap()).unwrap();
    }

    for (rel, size) in [
        ("com/example/shared/1.0/shared-1.0.jar", 10),
        ("org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar", 11),
        (
            "org/lwjgl/lwjgl/lwjgl-platform/2.9.4/lwjgl-platform-2.9.4-natives-windows-64.jar",
            12,
        ),
        (
            "net/fabricmc/fabric-loader/0.16.5/fabric-loader-0.16.5.jar",
            13,
        ),
        (
            "net/fabricmc/fabric-loader/0.15.0/fabric-loader-0.15.0.jar",
            14,
        ),
        (
            "net/minecraftforge/forge/1.20.1-47.2.0/forge-1.20.1-47.2.0-client.jar",
            15,
        ),
        ("net/minecraft/client/1.20.1-srg/client-1.20.1-srg.jar", 16),
        ("com/example/orphan/1.0/orphan-1.0.jar", 17),
    ] {
        write(&dirs.libraries().join(rel), &vec![0u8; size]);
    }

    write(
        &dirs.asset_index_file("17").unwrap(),
        br#"{"objects":{"a.ogg":{"hash":"aa11","size":3},"s.ogg":{"hash":"cc33","size":4}}}"#,
    );
    write(
        &dirs.asset_index_file("5").unwrap(),
        br#"{"objects":{"b.ogg":{"hash":"bb22","size":5},"s.ogg":{"hash":"cc33","size":4}}}"#,
    );
    for (hash, size) in [("aa11", 3), ("bb22", 5), ("cc33", 4), ("dd44", 6)] {
        write(&dirs.asset_object(hash).unwrap(), &vec![9u8; size]);
    }
    write(&dirs.log_configs().join("client-1.12.xml"), b"<xml/>");
    write(&dirs.log_configs().join("client-1.7.xml"), b"<old/>");

    for (component, version) in [
        ("java-runtime-delta", "21.0.3"),
        ("java-runtime-gamma", "17.0.8"),
        ("jre-legacy", "1.8.0_51"),
    ] {
        let root = dirs.runtime_dir(component, "windows-x64").unwrap();
        write(&root.join("bin").join("javaw.exe"), &[0u8; 100]);
        write(
            &root.join(".projects-runtime.json"),
            format!(r#"{{"version":"{version}","manifestSha1":"x"}}"#).as_bytes(),
        );
    }

    for key in [
        "1.21.1-0123456789",
        "fabric-loader-0.16.5-1.21.1-abcdefabcd",
        "fabric-loader-0.15.0-1.20.1-1111111111",
        "gone-2222222222",
        "weird",
    ] {
        write(&dirs.natives().join(key).join("x.dll"), &[0u8; 20]);
    }
    Fixture { _tmp: tmp, dirs }
}

fn ids(scan: &SharedScan) -> Vec<&str> {
    scan.items.iter().map(|item| item.id.as_str()).collect()
}

fn item<'a>(scan: &'a SharedScan, id: &str) -> &'a SharedItem {
    scan.items
        .iter()
        .find(|item| item.id == id)
        .unwrap_or_else(|| panic!("{id} missing from {:?}", ids(scan)))
}

/// The files an item would remove.
fn removal_names(fx: &Fixture, refs: &[GameRef], id: &str) -> Vec<String> {
    let plan = plan(&fx.dirs, refs, &later()).unwrap();
    let mut names: Vec<String> = plan
        .removals
        .get(id)
        .into_iter()
        .flatten()
        .map(|r| r.path.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn one_fabric_instance_frees_the_other_game_version() {
    let fx = fixture();
    let refs = [fabric("1.21.1", "0.16.5")];
    let scan = scan(&fx.dirs, &refs, &later()).unwrap();
    assert!(scan.complete);
    assert_eq!(
        ids(&scan),
        [
            "version:1.20.1",
            "version:1.20.1-forge-47.2.0",
            "version:fabric-loader-0.15.0-1.20.1",
            "runtime:java-runtime-gamma",
            "runtime:jre-legacy",
            "libraries",
            "assets",
        ]
    );

    let game = item(&scan, "version:1.20.1");
    assert_eq!(game.kind, SharedKind::GameVersion);
    assert_eq!(game.minecraft.as_deref(), Some("1.20.1"));
    let forge = item(&scan, "version:1.20.1-forge-47.2.0");
    assert_eq!(forge.kind, SharedKind::LoaderProfile);
    assert_eq!(forge.loader, Some(LoaderKind::Forge));
    assert_eq!(forge.loader_version.as_deref(), Some("47.2.0"));
    assert_eq!(forge.minecraft.as_deref(), Some("1.20.1"));
    let old_fabric = item(&scan, "version:fabric-loader-0.15.0-1.20.1");
    assert_eq!(old_fabric.loader, Some(LoaderKind::Fabric));
    assert_eq!(old_fabric.loader_version.as_deref(), Some("0.15.0"));
    // Its natives go with it; the shared game jar is in a group of its three holders.
    assert_eq!(
        removal_names(&fx, &refs, "version:fabric-loader-0.15.0-1.20.1"),
        [
            "fabric-loader-0.15.0-1.20.1",
            "fabric-loader-0.15.0-1.20.1-1111111111"
        ]
    );
    assert_eq!(scan.groups.len(), 1);
    assert_eq!(scan.groups[0].bytes, 800);
    assert_eq!(
        scan.groups[0].items,
        [
            "version:1.20.1",
            "version:1.20.1-forge-47.2.0",
            "version:fabric-loader-0.15.0-1.20.1"
        ]
    );
    assert!(game.bytes < 800, "the linked jar is not counted per holder");

    let gamma = item(&scan, "runtime:java-runtime-gamma");
    assert_eq!(gamma.java_major, Some(17));
    assert_eq!(item(&scan, "runtime:jre-legacy").java_major, Some(8));

    assert_eq!(
        removal_names(&fx, &refs, "libraries"),
        [
            "client-1.20.1-srg.jar",
            "fabric-loader-0.15.0.jar",
            "forge-1.20.1-47.2.0-client.jar",
            "gone-2222222222",
            "lwjgl-platform-2.9.4-natives-windows-64.jar",
            "orphan-1.0.jar",
        ]
    );
    let libraries = item(&scan, "libraries");
    assert_eq!(libraries.bytes, 12 + 14 + 15 + 16 + 17 + 20);
    assert_eq!(libraries.files, 6);
    assert_eq!(
        removal_names(&fx, &refs, "assets"),
        ["5.json", "bb22", "client-1.7.xml", "dd44"]
    );

    // Choosing everything frees the items and the group.
    let all: Vec<String> = scan.items.iter().map(|i| i.id.clone()).collect();
    assert_eq!(
        scan.bytes_for(&all),
        scan.items.iter().map(|i| i.bytes).sum::<u64>() + 800
    );
    assert_eq!(scan.total_bytes(), scan.bytes_for(&all));
    // Without every holder, the shared jar stays.
    let some: Vec<String> = vec!["version:1.20.1".into()];
    assert_eq!(scan.bytes_for(&some), game.bytes);
}

#[test]
fn a_parked_forge_set_keeps_its_profile_and_installer_outputs() {
    let fx = fixture();
    let refs = [
        fabric("1.21.1", "0.16.5"),
        // Parked sets record Forge versions in Maven form.
        GameRef::new(
            "1.20.1",
            Some(LoaderKind::Forge),
            Some("1.20.1-47.2.0".into()),
        ),
    ];
    let scan = scan(&fx.dirs, &refs, &later()).unwrap();
    assert_eq!(
        ids(&scan),
        [
            "version:fabric-loader-0.15.0-1.20.1",
            "runtime:jre-legacy",
            "libraries",
            "assets",
        ]
    );
    // The jar it links stays with the used versions: nothing to free there, no group.
    assert!(scan.groups.is_empty());
    assert_eq!(
        removal_names(&fx, &refs, "libraries"),
        [
            "fabric-loader-0.15.0.jar",
            "gone-2222222222",
            "orphan-1.0.jar"
        ]
    );
    assert_eq!(
        removal_names(&fx, &refs, "assets"),
        ["client-1.7.xml", "dd44"]
    );
}

#[test]
fn an_unknown_loader_version_keeps_every_profile_of_that_game_version() {
    let fx = fixture();
    let refs = [GameRef::new("1.20.1", Some(LoaderKind::Fabric), None)];
    let scan = scan(&fx.dirs, &refs, &later()).unwrap();
    let versions: Vec<&str> = ids(&scan)
        .into_iter()
        .filter(|id| id.starts_with("version:"))
        .collect();
    assert_eq!(
        versions,
        ["version:1.21.1", "version:fabric-loader-0.16.5-1.21.1"]
    );
    // Both holders of the 1.21.1 jar are unused: one group.
    assert_eq!(scan.groups.len(), 1);
    assert_eq!(scan.groups[0].bytes, 1000);

    // An unknown loader keeps them as well; a vanilla instance keeps only the game version.
    let unknown = [GameRef::new("1.20.1", None, None)];
    let scan = super::scan(&fx.dirs, &unknown, &later()).unwrap();
    assert!(!ids(&scan).contains(&"version:1.20.1-forge-47.2.0"));
    let vanilla = [GameRef::new("1.20.1", Some(LoaderKind::Vanilla), None)];
    let scan = super::scan(&fx.dirs, &vanilla, &later()).unwrap();
    assert!(ids(&scan).contains(&"version:1.20.1-forge-47.2.0"));
    assert!(!ids(&scan).contains(&"version:1.20.1"));
}

#[test]
fn unreadable_references_keep_libraries_assets_and_java() {
    let fx = fixture();
    // A used Forge profile whose installer record is gone: its generated files are unknown.
    fs::remove_file(
        fx.version("1.20.1-forge-47.2.0")
            .join("projects-loader.json"),
    )
    .unwrap();
    let refs = [GameRef::new(
        "1.20.1",
        Some(LoaderKind::Forge),
        Some("47.2.0".into()),
    )];
    let scan = scan(&fx.dirs, &refs, &later()).unwrap();
    assert!(!scan.complete);
    assert!(
        scan.items.iter().all(|item| matches!(
            item.kind,
            SharedKind::GameVersion | SharedKind::LoaderProfile
        )),
        "{:?}",
        ids(&scan)
    );
    assert!(ids(&scan).contains(&"version:1.21.1"));

    // A used version folder with a broken JSON does the same.
    let fx = fixture();
    fs::write(fx.dirs.version_json("1.21.1").unwrap(), b"{ not json").unwrap();
    let scan = super::scan(&fx.dirs, &[fabric("1.21.1", "0.16.5")], &later()).unwrap();
    assert!(!scan.complete);
    assert!(!ids(&scan).contains(&"libraries"));

    // A used asset index that cannot be read keeps the assets only.
    let fx = fixture();
    fs::write(fx.dirs.asset_index_file("17").unwrap(), b"garbage").unwrap();
    let scan = super::scan(&fx.dirs, &[fabric("1.21.1", "0.16.5")], &later()).unwrap();
    assert!(scan.complete);
    assert!(ids(&scan).contains(&"libraries"));
    assert!(!ids(&scan).contains(&"assets"));
}

#[test]
fn fresh_files_and_protected_runtimes_stay() {
    let fx = fixture();
    let options = later();
    // A library written "just now" (relative to the scan) may belong to a running download.
    fs::File::options()
        .write(true)
        .open(fx.lib("com/example/orphan/1.0/orphan-1.0.jar"))
        .unwrap()
        .set_modified(options.now - Duration::from_secs(60))
        .unwrap();
    let custom_java = fx
        .dirs
        .runtime_dir("jre-legacy", "windows-x64")
        .unwrap()
        .join("bin")
        .join("javaw.exe");
    let options = ScanOptions {
        protected: vec![custom_java],
        ..options
    };
    let refs = [fabric("1.21.1", "0.16.5")];
    let plan = plan(&fx.dirs, &refs, &options).unwrap();
    assert!(!ids(&plan.scan).contains(&"runtime:jre-legacy"));
    let libraries: Vec<String> = plan.removals["libraries"]
        .iter()
        .map(|r| r.path.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert!(!libraries.contains(&"orphan-1.0.jar".to_owned()));
    // Nothing is offered when everything is fresh.
    let now = ScanOptions::default();
    assert!(scan(&fx.dirs, &refs, &now).unwrap().items.is_empty());
}

#[test]
fn delete_removes_only_what_is_still_unused() {
    let fx = fixture();
    let before = [fabric("1.21.1", "0.16.5")];
    let chosen: Vec<String> = [
        "version:fabric-loader-0.15.0-1.20.1",
        "runtime:jre-legacy",
        "libraries",
        "assets",
    ]
    .map(str::to_owned)
    .to_vec();
    // Meanwhile a parked set started using the old Fabric profile again.
    let after = [fabric("1.21.1", "0.16.5"), fabric("1.20.1", "0.15.0")];
    let expected = scan(&fx.dirs, &after, &later()).unwrap().bytes_for(&chosen);
    assert!(
        scan(&fx.dirs, &before, &later())
            .unwrap()
            .bytes_for(&chosen)
            > expected
    );

    let progress = std::sync::Mutex::new(Vec::new());
    let report = delete(
        &fx.dirs,
        &after,
        &later(),
        &chosen,
        &CancellationToken::new(),
        &|done, total| progress.lock().unwrap().push((done, total)),
    )
    .unwrap();
    assert_eq!(report.freed_bytes, expected);
    assert_eq!(report.failed_files, 0);
    let steps = progress.into_inner().unwrap();
    assert_eq!(steps.first().map(|s| s.0), Some(0));
    assert!(steps.last().is_some_and(|(done, total)| done == total));

    // Still used: kept.
    assert!(fx.version("fabric-loader-0.15.0-1.20.1").is_dir());
    assert!(
        fx.lib("net/fabricmc/fabric-loader/0.15.0/fabric-loader-0.15.0.jar")
            .is_file()
    );
    assert!(fx.dirs.asset_object("bb22").unwrap().is_file());
    assert!(fx.lib("com/example/shared/1.0/shared-1.0.jar").is_file());
    // Unused: gone, and their empty folders with them.
    assert!(!fx.dirs.runtimes().join("jre-legacy").exists());
    assert!(!fx.lib("com/example/orphan").exists());
    assert!(!fx.dirs.asset_object("dd44").unwrap().exists());
    assert!(!fx.dirs.asset_objects().join("dd").exists());
    assert!(!fx.dirs.natives().join("gone-2222222222").exists());
    assert!(fx.dirs.natives().join("weird").exists());

    // A cancelled delete stops before touching anything.
    let fx = fixture();
    let cancel = CancellationToken::new();
    cancel.cancel();
    let result = delete(
        &fx.dirs,
        &before,
        &later(),
        &["version:1.20.1".to_owned()],
        &cancel,
        &|_, _| {},
    );
    assert!(matches!(result, Err(Error::Cancelled)));
    assert!(fx.version("1.20.1").is_dir());
}

#[test]
fn names_and_ordering_helpers() {
    assert_eq!(natives_version("1.21.1-0123456789"), Some("1.21.1"));
    assert_eq!(
        natives_version("fabric-loader-0.15.0-1.20.1-abcdefabcd"),
        Some("fabric-loader-0.15.0-1.20.1")
    );
    assert_eq!(natives_version("weird"), None);
    assert_eq!(natives_version("x-12345"), None);
    assert_eq!(
        guess_loader("neoforge-21.1.77", Some("1.21.1")),
        Some((
            LoaderKind::NeoForge,
            Some("21.1.77".into()),
            Some("1.21.1".into())
        ))
    );
    assert_eq!(
        guess_loader("1.20.1-neoforge-47.1.106", None),
        Some((
            LoaderKind::NeoForge,
            Some("47.1.106".into()),
            Some("1.20.1".into())
        ))
    );
    assert_eq!(
        guess_loader("quilt-loader-0.26.0-1.21", None),
        Some((
            LoaderKind::Quilt,
            Some("0.26.0".into()),
            Some("1.21".into())
        ))
    );
    assert_eq!(guess_loader("24w14a", None), None);
    assert_eq!(
        natural_cmp("1.21.10", "1.21.9"),
        std::cmp::Ordering::Greater
    );
    assert_eq!(norm("Com\\Example/X.jar"), {
        if cfg!(any(windows, target_os = "macos")) {
            "com/example/x.jar"
        } else {
            "Com/Example/X.jar"
        }
    });
}

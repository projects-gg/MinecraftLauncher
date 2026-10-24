//! The overlay's mod in a game folder: real folders, real jars (zips with a `fabric.mod.json`), the
//! launcher's real mod check.

use std::io::Write;
use std::path::Path;

use pl_mc::loaders::LoaderKind;
use pl_mc::mods::check::CheckEnvironment;

use super::*;

/// The six Fabric API modules the mod needs (its `fabric.mod.json`).
const MODULES: [&str; 6] = [
    "fabric-key-mapping-api-v1",
    "fabric-lifecycle-events-v1",
    "fabric-networking-api-v1",
    "fabric-rendering-v1",
    "fabric-resource-loader-v1",
    "fabric-screen-api-v1",
];

fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut out);
        let options = zip::write::SimpleFileOptions::default();
        for (name, content) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(content).unwrap();
        }
        writer.finish().unwrap();
    }
    out.into_inner()
}

/// The overlay's jar as the build makes it; `build` tells two builds of one version apart.
fn overlay_jar(version: &str, range: &str, build: &str) -> Vec<u8> {
    let depends: Vec<String> = MODULES.iter().map(|m| format!("\"{m}\": \"*\"")).collect();
    let json = format!(
        r#"{{"schemaVersion": 1, "id": "projects_overlay", "version": "{version}",
            "environment": "client",
            "depends": {{"fabricloader": ">=0.19.5", "minecraft": "{range}", "java": ">=25", {}}}}}"#,
        depends.join(", ")
    );
    zip(&[
        ("fabric.mod.json", json.as_bytes()),
        ("build.txt", build.as_bytes()),
    ])
}

/// Fabric API as one jar that provides its modules.
fn fabric_api_jar(version: &str) -> Vec<u8> {
    let provides: Vec<String> = MODULES.iter().map(|m| format!("\"{m}\"")).collect();
    let json = format!(
        r#"{{"schemaVersion": 1, "id": "fabric-api", "version": "{version}",
            "provides": [{}]}}"#,
        provides.join(", ")
    );
    zip(&[("fabric.mod.json", json.as_bytes())])
}

fn sha1(bytes: &[u8]) -> String {
    pl_net::hash_bytes(bytes, HashAlgo::Sha1)
}

/// A launcher build's jars for `versions` (Minecraft version, range), all of build `build`.
fn bundle(dir: &Path, versions: &[(&str, &str)], build: &str) -> Bundle {
    std::fs::create_dir_all(dir).unwrap();
    let jars: Vec<BundledJar> = versions
        .iter()
        .map(|(minecraft, range)| {
            let version = format!("0.1.0+{minecraft}");
            let bytes = overlay_jar(&version, range, build);
            let file = format!("projects-overlay-{version}.jar");
            std::fs::write(dir.join(&file), &bytes).unwrap();
            BundledJar {
                minecraft: (*minecraft).to_owned(),
                range: (*range).to_owned(),
                file,
                version,
                sha1: sha1(&bytes),
                size: bytes.len() as u64,
            }
        })
        .collect();
    let index = BundleIndex { schema: 1, jars };
    std::fs::write(
        dir.join(INDEX_FILE),
        serde_json::to_vec_pretty(&index).unwrap(),
    )
    .unwrap();
    Bundle::load(dir).expect("a bundle")
}

fn three(dir: &Path, build: &str) -> Bundle {
    bundle(
        dir,
        &[("26.1.2", "~26.1.2"), ("26.2", "~26.2"), ("26.3", "~26.3")],
        build,
    )
}

fn env(minecraft: &str) -> CheckEnvironment {
    CheckEnvironment {
        minecraft: minecraft.to_owned(),
        loader: LoaderKind::Fabric,
        loader_version: Some("0.19.5".to_owned()),
        java_major: Some(25),
    }
}

fn put(game: &Path, name: &str, bytes: &[u8]) {
    let mods = game.join("mods");
    std::fs::create_dir_all(&mods).unwrap();
    std::fs::write(mods.join(name), bytes).unwrap();
}

fn mods(game: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(game.join("mods"))
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

async fn run(game: &Path, bundle: Option<&Bundle>, env: &CheckEnvironment, on: bool) -> Outcome {
    reconcile(Request {
        game_dir: game,
        bundle,
        env,
        enabled: on,
        ask_fabric_api: true,
    })
    .await
}

const JAR_262: &str = "projects-overlay-0.1.0+26.2.jar";
const API_262: &str = "fabric-api-0.161.0+26.2.jar";

#[tokio::test]
async fn puts_the_versions_jar_in_place_and_records_it() {
    let dir = tempfile::tempdir().unwrap();
    let game = dir.path().join("game");
    let bundle = three(&dir.path().join("bundle"), "b1");
    put(&game, API_262, &fabric_api_jar("0.161.0+26.2"));

    let outcome = run(&game, Some(&bundle), &env("26.2"), true).await;
    assert_eq!(outcome, Outcome::Ready { ours: true });
    assert_eq!(mods(&game), [API_262, JAR_262]);
    let written = std::fs::read(game.join("mods").join(JAR_262)).unwrap();
    assert_eq!(sha1(&written), bundle.jar_for("26.2").unwrap().sha1);
    let record = ModRecord::load_blocking(&game);
    assert_eq!(record.files[JAR_262].minecraft, "26.2");
    assert!(is_owned(&owned_files(&game), JAR_262));
    assert!(is_owned(
        &owned_files(&game),
        &format!("{JAR_262}.disabled")
    ));
    assert!(!is_owned(&owned_files(&game), API_262));

    // The next start finds it in place: nothing changes.
    let modified = std::fs::metadata(game.join("mods").join(JAR_262))
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(
        run(&game, Some(&bundle), &env("26.2"), true).await,
        Outcome::Ready { ours: true }
    );
    let again = std::fs::metadata(game.join("mods").join(JAR_262))
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(modified, again);
}

#[tokio::test]
async fn asks_for_fabric_api_before_anything_moves() {
    let dir = tempfile::tempdir().unwrap();
    let game = dir.path().join("game");
    let bundle = three(&dir.path().join("bundle"), "b1");
    std::fs::create_dir_all(game.join("mods")).unwrap();

    // No Fabric API: the caller is asked to install it, the folder stays as it is.
    assert_eq!(
        run(&game, Some(&bundle), &env("26.2"), true).await,
        Outcome::NeedsFabricApi
    );
    assert!(mods(&game).is_empty());
    // Still none on the second try (Modrinth unreachable): no overlay, never a missing mod.
    let second = reconcile(Request {
        game_dir: &game,
        bundle: Some(&bundle),
        env: &env("26.2"),
        enabled: true,
        ask_fabric_api: false,
    })
    .await;
    assert_eq!(second, Outcome::Off(OffReason::Incompatible));
    assert!(mods(&game).is_empty());

    // Fabric API came: the jar goes in.
    put(&game, API_262, &fabric_api_jar("0.161.0+26.2"));
    assert_eq!(
        run(&game, Some(&bundle), &env("26.2"), true).await,
        Outcome::Ready { ours: true }
    );

    // The player deleted Fabric API later: the jar waits for it again, and goes without it.
    std::fs::remove_file(game.join("mods").join(API_262)).unwrap();
    assert_eq!(
        run(&game, Some(&bundle), &env("26.2"), true).await,
        Outcome::NeedsFabricApi
    );
    let without = reconcile(Request {
        game_dir: &game,
        bundle: Some(&bundle),
        env: &env("26.2"),
        enabled: true,
        ask_fabric_api: false,
    })
    .await;
    assert_eq!(without, Outcome::Off(OffReason::Incompatible));
    assert!(mods(&game).is_empty(), "{:?}", mods(&game));
    assert!(ModRecord::load_blocking(&game).files.is_empty());
}

#[tokio::test]
async fn fabric_api_the_player_switched_off_is_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let game = dir.path().join("game");
    let bundle = three(&dir.path().join("bundle"), "b1");
    let disabled = format!("{API_262}.disabled");
    put(&game, &disabled, &fabric_api_jar("0.161.0+26.2"));

    assert_eq!(
        run(&game, Some(&bundle), &env("26.2"), true).await,
        Outcome::Off(OffReason::FabricApiOff)
    );
    assert_eq!(mods(&game), [disabled]);
}

#[tokio::test]
async fn switched_off_takes_ours_away_and_leaves_the_rest() {
    let dir = tempfile::tempdir().unwrap();
    let game = dir.path().join("game");
    let bundle = three(&dir.path().join("bundle"), "b1");
    put(&game, API_262, &fabric_api_jar("0.161.0+26.2"));
    run(&game, Some(&bundle), &env("26.2"), true).await;

    assert_eq!(
        run(&game, Some(&bundle), &env("26.2"), false).await,
        Outcome::Off(OffReason::SwitchedOff)
    );
    assert_eq!(
        mods(&game),
        [API_262],
        "Fabric API stays: other mods may need it"
    );
    assert!(ModRecord::load_blocking(&game).files.is_empty());

    // On again: back.
    assert_eq!(
        run(&game, Some(&bundle), &env("26.2"), true).await,
        Outcome::Ready { ours: true }
    );
    assert_eq!(mods(&game), [API_262, JAR_262]);
}

#[tokio::test]
async fn a_copy_someone_else_put_there_wins_quietly() {
    let dir = tempfile::tempdir().unwrap();
    let game = dir.path().join("game");
    let bundle = three(&dir.path().join("bundle"), "b1");
    put(&game, API_262, &fabric_api_jar("0.161.0+26.2"));
    run(&game, Some(&bundle), &env("26.2"), true).await;
    // The player adds a copy of their own (another build, another name).
    let theirs = "projects-overlay-dev.jar";
    put(&game, theirs, &overlay_jar("0.1.0+26.2", "~26.2", "dev"));

    assert_eq!(
        run(&game, Some(&bundle), &env("26.2"), true).await,
        Outcome::Ready { ours: false }
    );
    assert_eq!(mods(&game), [API_262, theirs], "one copy: theirs");
    assert!(ModRecord::load_blocking(&game).files.is_empty());

    // Switched off: their copy is theirs, it stays (and no bridge).
    assert_eq!(
        run(&game, Some(&bundle), &env("26.2"), false).await,
        Outcome::Off(OffReason::SwitchedOff)
    );
    assert_eq!(mods(&game), [API_262, theirs]);

    // A disabled copy of theirs does not count: ours comes back.
    std::fs::rename(
        game.join("mods").join(theirs),
        game.join("mods").join(format!("{theirs}.disabled")),
    )
    .unwrap();
    assert_eq!(
        run(&game, Some(&bundle), &env("26.2"), true).await,
        Outcome::Ready { ours: true }
    );
}

#[tokio::test]
async fn a_jar_the_player_changed_becomes_theirs() {
    let dir = tempfile::tempdir().unwrap();
    let game = dir.path().join("game");
    let bundle = three(&dir.path().join("bundle"), "b1");
    put(&game, API_262, &fabric_api_jar("0.161.0+26.2"));
    run(&game, Some(&bundle), &env("26.2"), true).await;
    // Replaced by hand with another build under our name.
    let theirs = overlay_jar("0.1.0+26.2", "~26.2", "a much longer custom build");
    put(&game, JAR_262, &theirs);

    assert_eq!(
        run(&game, Some(&bundle), &env("26.2"), true).await,
        Outcome::Ready { ours: false }
    );
    assert_eq!(
        std::fs::read(game.join("mods").join(JAR_262)).unwrap(),
        theirs,
        "never overwritten"
    );
    assert!(ModRecord::load_blocking(&game).files.is_empty());
    assert_eq!(
        run(&game, Some(&bundle), &env("26.2"), false).await,
        Outcome::Off(OffReason::SwitchedOff)
    );
    assert!(game.join("mods").join(JAR_262).exists(), "theirs stays");
}

#[tokio::test]
async fn a_new_launcher_build_replaces_the_jar_and_our_disabled_copies() {
    let dir = tempfile::tempdir().unwrap();
    let game = dir.path().join("game");
    let old = three(&dir.path().join("old"), "b1");
    put(&game, API_262, &fabric_api_jar("0.161.0+26.2"));
    run(&game, Some(&old), &env("26.2"), true).await;
    // The player disabled ours by hand: the switch is the one control, it is not adopted.
    std::fs::rename(
        game.join("mods").join(JAR_262),
        game.join("mods").join(format!("{JAR_262}.disabled")),
    )
    .unwrap();

    let new = three(&dir.path().join("new"), "build two");
    assert_eq!(
        run(&game, Some(&new), &env("26.2"), true).await,
        Outcome::Ready { ours: true }
    );
    assert_eq!(mods(&game), [API_262, JAR_262]);
    let written = std::fs::read(game.join("mods").join(JAR_262)).unwrap();
    assert_eq!(sha1(&written), new.jar_for("26.2").unwrap().sha1);
    assert_eq!(
        ModRecord::load_blocking(&game).files[JAR_262].sha1,
        new.jar_for("26.2").unwrap().sha1
    );

    // Same name, other bytes, enabled: overwritten in place.
    let newer = three(&dir.path().join("newer"), "build three, longer");
    assert_eq!(
        run(&game, Some(&newer), &env("26.2"), true).await,
        Outcome::Ready { ours: true }
    );
    assert_eq!(mods(&game), [API_262, JAR_262]);
    let written = std::fs::read(game.join("mods").join(JAR_262)).unwrap();
    assert_eq!(sha1(&written), newer.jar_for("26.2").unwrap().sha1);
}

#[tokio::test]
async fn parked_versions_keep_their_entries() {
    let dir = tempfile::tempdir().unwrap();
    let game = dir.path().join("game");
    let bundle = three(&dir.path().join("bundle"), "b1");
    put(
        &game,
        "fabric-api-0.152.1+26.1.2.jar",
        &fabric_api_jar("0.152.1+26.1.2"),
    );
    run(&game, Some(&bundle), &env("26.1.2"), true).await;
    // A version switch parks mods/ and starts 26.2 with an empty one.
    std::fs::rename(game.join("mods"), dir.path().join("parked-26.1.2")).unwrap();
    put(&game, API_262, &fabric_api_jar("0.161.0+26.2"));
    assert_eq!(
        run(&game, Some(&bundle), &env("26.2"), true).await,
        Outcome::Ready { ours: true }
    );
    let record = ModRecord::load_blocking(&game);
    assert_eq!(
        record.files.keys().collect::<Vec<_>>(),
        ["projects-overlay-0.1.0+26.1.2.jar", JAR_262]
    );

    // Back to 26.1.2: its parked jar is ours, still current.
    std::fs::remove_dir_all(game.join("mods")).unwrap();
    std::fs::rename(dir.path().join("parked-26.1.2"), game.join("mods")).unwrap();
    assert_eq!(
        run(&game, Some(&bundle), &env("26.1.2"), true).await,
        Outcome::Ready { ours: true }
    );
    assert_eq!(ModRecord::load_blocking(&game).files.len(), 2);

    // This version's entry whose file is gone (deleted by hand) is forgotten; the parked one stays.
    std::fs::remove_file(game.join("mods").join("projects-overlay-0.1.0+26.1.2.jar")).unwrap();
    run(&game, Some(&bundle), &env("26.1.2"), false).await;
    let record = ModRecord::load_blocking(&game);
    assert_eq!(record.files.keys().collect::<Vec<_>>(), [JAR_262]);
}

#[tokio::test]
async fn other_loaders_and_versions_without_a_jar_get_none() {
    let dir = tempfile::tempdir().unwrap();
    let game = dir.path().join("game");
    let bundle = three(&dir.path().join("bundle"), "b1");
    put(&game, API_262, &fabric_api_jar("0.161.0+26.2"));
    run(&game, Some(&bundle), &env("26.2"), true).await;

    let quilt = CheckEnvironment {
        loader: LoaderKind::Quilt,
        ..env("26.2")
    };
    assert_eq!(
        run(&game, Some(&bundle), &quilt, true).await,
        Outcome::Off(OffReason::NotFabric)
    );
    assert_eq!(mods(&game), [API_262]);

    for minecraft in ["26.4", "26.1", "26.3-pre1", "1.21.11"] {
        assert_eq!(
            run(&game, Some(&bundle), &env(minecraft), true).await,
            Outcome::Off(OffReason::NoJar),
            "{minecraft}"
        );
    }
    // A build without jars: none of ours either.
    run(&game, Some(&bundle), &env("26.2"), true).await;
    assert_eq!(
        run(&game, None, &env("26.2"), true).await,
        Outcome::Off(OffReason::NoJar)
    );
    assert_eq!(mods(&game), [API_262]);

    // ... but an older launcher's pack copy keeps its bridge there.
    let pack_copy = "projects-overlay-0.1.0+mc26.2.jar";
    put(
        &game,
        pack_copy,
        &overlay_jar("0.1.0+26.2", "~26.2", "pack"),
    );
    assert_eq!(
        run(&game, None, &env("26.2"), true).await,
        Outcome::Ready { ours: false }
    );
    assert_eq!(
        run(&game, None, &quilt, true).await,
        Outcome::Off(OffReason::NotFabric)
    );
}

#[tokio::test]
async fn a_loader_or_java_it_does_not_take_means_no_overlay() {
    let dir = tempfile::tempdir().unwrap();
    let game = dir.path().join("game");
    let bundle = three(&dir.path().join("bundle"), "b1");
    put(&game, API_262, &fabric_api_jar("0.161.0+26.2"));

    let old_loader = CheckEnvironment {
        loader_version: Some("0.18.4".to_owned()),
        ..env("26.2")
    };
    assert_eq!(
        run(&game, Some(&bundle), &old_loader, true).await,
        Outcome::Off(OffReason::Incompatible)
    );
    let old_java = CheckEnvironment {
        java_major: Some(21),
        ..env("26.2")
    };
    assert_eq!(
        run(&game, Some(&bundle), &old_java, true).await,
        Outcome::Off(OffReason::Incompatible)
    );
    assert_eq!(mods(&game), [API_262]);

    // A mod that breaks the overlay keeps it out too.
    let breaker = r#"{"schemaVersion": 1, "id": "grumpy", "version": "1.0.0",
        "breaks": {"projects_overlay": "*"}}"#;
    put(
        &game,
        "grumpy.jar",
        &zip(&[("fabric.mod.json", breaker.as_bytes())]),
    );
    assert_eq!(
        run(&game, Some(&bundle), &env("26.2"), true).await,
        Outcome::Off(OffReason::Incompatible)
    );
}

#[tokio::test]
async fn a_lost_record_still_knows_a_shipped_jar() {
    let dir = tempfile::tempdir().unwrap();
    let game = dir.path().join("game");
    let bundle = three(&dir.path().join("bundle"), "b1");
    put(&game, API_262, &fabric_api_jar("0.161.0+26.2"));
    run(&game, Some(&bundle), &env("26.2"), true).await;
    std::fs::remove_file(ModRecord::path(&game)).unwrap();

    assert_eq!(
        run(&game, Some(&bundle), &env("26.2"), false).await,
        Outcome::Off(OffReason::SwitchedOff)
    );
    assert_eq!(mods(&game), [API_262], "its exact copy is ours");
}

#[test]
fn the_bundle_reads_only_sound_jars() {
    let dir = tempfile::tempdir().unwrap();
    let good = overlay_jar("0.1.0+26.2", "~26.2", "b1");
    std::fs::write(dir.path().join(JAR_262), &good).unwrap();
    let entry = |file: &str, sha1: &str, size: u64| BundledJar {
        minecraft: "26.2".into(),
        range: "~26.2".into(),
        file: file.into(),
        version: "0.1.0+26.2".into(),
        sha1: sha1.into(),
        size,
    };
    let good_sha = sha1(&good);
    let index = BundleIndex {
        schema: 1,
        jars: vec![
            entry("../escape.jar", &good_sha, good.len() as u64),
            entry("missing.jar", &good_sha, 10),
            entry(JAR_262, &good_sha, good.len() as u64 + 1),
            entry(JAR_262, &good_sha.to_uppercase(), good.len() as u64),
            entry(JAR_262, &good_sha, good.len() as u64),
        ],
    };
    std::fs::write(
        dir.path().join(INDEX_FILE),
        serde_json::to_vec(&index).unwrap(),
    )
    .unwrap();
    let bundle = Bundle::load(dir.path()).unwrap();
    assert_eq!(bundle.jars().len(), 1);
    assert_eq!(bundle.versions(), ["26.2"]);
    assert_eq!(bundle.jar_for("26.2.1").unwrap().file, JAR_262);
    assert!(bundle.jar_for("26.3").is_none());

    // Another schema, or no index at all: no bundle.
    std::fs::write(dir.path().join(INDEX_FILE), br#"{"schema": 2, "jars": []}"#).unwrap();
    assert!(Bundle::load(dir.path()).is_none());
    std::fs::remove_file(dir.path().join(INDEX_FILE)).unwrap();
    assert!(Bundle::load(dir.path()).is_none());
}

#[test]
fn versions_pick_their_jar() {
    let dir = tempfile::tempdir().unwrap();
    let bundle = three(dir.path(), "b1");
    let pick = |minecraft: &str| bundle.jar_for(minecraft).map(|jar| jar.minecraft.as_str());
    assert_eq!(pick("26.1.2"), Some("26.1.2"));
    assert_eq!(pick("26.1.3"), Some("26.1.2"));
    assert_eq!(pick("26.2"), Some("26.2"));
    assert_eq!(pick("26.3"), Some("26.3"));
    assert_eq!(pick("26.1.1"), None);
    assert_eq!(pick("26.4"), None);
}

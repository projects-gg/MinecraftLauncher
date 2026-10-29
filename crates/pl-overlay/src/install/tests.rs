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
        jars: Jars::bundled(bundle),
        fetched: None,
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
        jars: Jars::bundled(Some(&bundle)),
        fetched: None,
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
        jars: Jars::bundled(Some(&bundle)),
        fetched: None,
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

// --- OVL-AUTO: jars from the signed content ------------------------------------------------------

const JAR_264: &str = "projects-overlay-0.1.0+26.4.jar";
const API_264: &str = "fabric-api-0.170.0+26.4.jar";

/// The signed index's entry for `bytes`, built for `minecraft` (range `~<minecraft>`).
fn entry(bytes: &[u8], minecraft: &str) -> OverlayJar {
    OverlayJar {
        minecraft: minecraft.into(),
        range: format!("~{minecraft}"),
        version: format!("0.1.0+{minecraft}"),
        proto: crate::protocol::PROTO,
        file: pl_content::studio::overlay_file_ref(bytes),
        fabric_api: None,
        commit: None,
        run: None,
        tested_at: None,
    }
}

fn usable(entries: Vec<OverlayJar>) -> Vec<ContentJar> {
    ContentJar::usable(&OverlayRelease { jars: entries }, crate::protocol::PROTO)
}

/// What the caller fetched: `bytes` in a folder of their own.
fn fetched(dir: &Path, bytes: &[u8]) -> PathBuf {
    let path = dir.join("fetched").join(format!("{}.jar", sha1(bytes)));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, bytes).unwrap();
    path
}

async fn run_with(
    game: &Path,
    jars: Jars<'_>,
    fetched: Option<&Path>,
    env: &CheckEnvironment,
) -> Outcome {
    reconcile(Request {
        game_dir: game,
        jars,
        fetched,
        env,
        enabled: true,
        ask_fabric_api: true,
    })
    .await
}

#[tokio::test]
async fn a_published_jar_covers_a_version_the_launcher_has_none_for() {
    let dir = tempfile::tempdir().unwrap();
    let game = dir.path().join("game");
    let bundle = three(&dir.path().join("bundle"), "b1");
    let bytes = overlay_jar("0.1.0+26.4", "~26.4", "ci");
    let content = usable(vec![entry(&bytes, "26.4")]);
    let copy = fetched(dir.path(), &bytes);
    put(&game, API_264, &fabric_api_jar("0.170.0+26.4"));
    let jars = Jars {
        bundle: Some(&bundle),
        content: &content,
    };

    assert_eq!(
        run_with(&game, jars, Some(&copy), &env("26.4")).await,
        Outcome::Ready { ours: true }
    );
    assert_eq!(mods(&game), [API_264, JAR_264]);
    assert_eq!(
        std::fs::read(game.join("mods").join(JAR_264)).unwrap(),
        bytes
    );
    let record = ModRecord::load_blocking(&game);
    assert_eq!(record.files[JAR_264].sha1, sha1(&bytes));
    assert_eq!(record.files[JAR_264].minecraft, "26.4");

    // Offline at the next start: the copy in place passes the same check and serves.
    assert_eq!(
        run_with(&game, jars, None, &env("26.4")).await,
        Outcome::Ready { ours: true }
    );
    assert_eq!(mods(&game), [API_264, JAR_264]);

    // Taken out of the index: ours goes at the next start.
    assert_eq!(
        run_with(&game, Jars::bundled(Some(&bundle)), None, &env("26.4")).await,
        Outcome::Off(OffReason::NoJar)
    );
    assert_eq!(mods(&game), [API_264]);
    assert!(ModRecord::load_blocking(&game).files.is_empty());
}

#[tokio::test]
async fn the_launchers_own_jar_wins_over_a_published_one() {
    let dir = tempfile::tempdir().unwrap();
    let game = dir.path().join("game");
    let bundle = three(&dir.path().join("bundle"), "b1");
    let published = overlay_jar("0.1.0+26.2", "~26.2", "ci");
    let content = usable(vec![entry(&published, "26.2")]);
    let copy = fetched(dir.path(), &published);
    put(&game, API_262, &fabric_api_jar("0.161.0+26.2"));
    let jars = Jars {
        bundle: Some(&bundle),
        content: &content,
    };
    assert!(matches!(jars.jar_for("26.2"), Some(Pick::Bundled(_))));
    assert_eq!(
        run_with(&game, jars, Some(&copy), &env("26.2")).await,
        Outcome::Ready { ours: true }
    );
    let written = std::fs::read(game.join("mods").join(JAR_262)).unwrap();
    assert_eq!(sha1(&written), bundle.jar_for("26.2").unwrap().sha1);
}

/// A published jar for 26.4 whose fetched copy is `copy`: refused, nothing goes in.
async fn refused(entry: OverlayJar, copy: &[u8]) {
    let dir = tempfile::tempdir().unwrap();
    let game = dir.path().join("game");
    put(&game, API_264, &fabric_api_jar("0.170.0+26.4"));
    let content = usable(vec![entry]);
    let path = fetched(dir.path(), copy);
    let outcome = run_with(
        &game,
        Jars {
            bundle: None,
            content: &content,
        },
        Some(&path),
        &env("26.4"),
    )
    .await;
    assert!(
        matches!(outcome, Outcome::Off(OffReason::Failed | OffReason::NoJar)),
        "{outcome:?}"
    );
    assert_eq!(mods(&game), [API_264]);
    assert!(ModRecord::load_blocking(&game).files.is_empty());
}

#[tokio::test]
async fn a_published_jar_of_another_bridge_protocol_is_refused() {
    let bytes = overlay_jar("0.1.0+26.4", "~26.4", "ci");
    let mut other = entry(&bytes, "26.4");
    other.proto = crate::protocol::PROTO + 1;
    assert!(usable(vec![other.clone()]).is_empty());
    refused(other, &bytes).await;
}

#[tokio::test]
async fn a_fetched_jar_of_another_size_is_refused() {
    let bytes = overlay_jar("0.1.0+26.4", "~26.4", "ci");
    let mut signed = entry(&bytes, "26.4");
    signed.file.size += 1;
    refused(signed, &bytes).await;
}

#[tokio::test]
async fn a_fetched_jar_with_another_sha256_is_refused() {
    let bytes = overlay_jar("0.1.0+26.4", "~26.4", "ci");
    let other = overlay_jar("0.1.0+26.4", "~26.4", "cj");
    assert_eq!(bytes.len(), other.len());
    // Size and SHA-1 of the copy, SHA-256 of the signed bytes.
    let mut signed = entry(&bytes, "26.4");
    signed.file.sha1 = Some(sha1(&other));
    refused(signed, &other).await;
}

#[tokio::test]
async fn a_fetched_jar_with_another_sha1_is_refused() {
    let bytes = overlay_jar("0.1.0+26.4", "~26.4", "ci");
    // Size and SHA-256 of the copy, another SHA-1.
    let mut signed = entry(&bytes, "26.4");
    signed.file.sha1 = Some(sha1(b"something else"));
    refused(signed, &bytes).await;
}

#[tokio::test]
async fn a_published_jar_without_sha1_or_whose_range_misses_its_version_is_refused() {
    let bytes = overlay_jar("0.1.0+26.4", "~26.4", "ci");
    let mut unhashed = entry(&bytes, "26.4");
    unhashed.file.sha1 = None;
    assert!(usable(vec![unhashed.clone()]).is_empty());
    refused(unhashed, &bytes).await;
    let mut elsewhere = entry(&bytes, "26.4");
    elsewhere.range = "~26.5".into();
    assert!(usable(vec![elsewhere.clone()]).is_empty());
    refused(elsewhere, &bytes).await;
}

#[tokio::test]
async fn offline_a_copy_in_place_that_fails_the_check_is_refused_and_forgotten() {
    let dir = tempfile::tempdir().unwrap();
    let game = dir.path().join("game");
    put(&game, API_264, &fabric_api_jar("0.170.0+26.4"));
    let bytes = overlay_jar("0.1.0+26.4", "~26.4", "ci");
    let signed = entry(&bytes, "26.4");
    let copy = fetched(dir.path(), &bytes);
    let content = usable(vec![signed.clone()]);
    let jars = Jars {
        bundle: None,
        content: &content,
    };
    assert_eq!(
        run_with(&game, jars, Some(&copy), &env("26.4")).await,
        Outcome::Ready { ours: true }
    );

    // The copy in place changes but keeps the SHA-1 our record and the entry name (as a SHA-1
    // collision would): its size is the same, its SHA-256 is not the signed one.
    let tampered = overlay_jar("0.1.0+26.4", "~26.4", "cj");
    assert_eq!(tampered.len(), bytes.len());
    std::fs::write(game.join("mods").join(JAR_264), &tampered).unwrap();
    let mut record = ModRecord::load_blocking(&game);
    record.files.get_mut(JAR_264).unwrap().sha1 = sha1(&tampered);
    std::fs::write(ModRecord::path(&game), serde_json::to_vec(&record).unwrap()).unwrap();
    let mut collided = signed;
    collided.file.sha1 = Some(sha1(&tampered));
    let content = usable(vec![collided]);
    let jars = Jars {
        bundle: None,
        content: &content,
    };

    // Offline: it serves nothing, goes, and our record forgets it.
    assert_eq!(
        run_with(&game, jars, None, &env("26.4")).await,
        Outcome::Off(OffReason::NotFetched)
    );
    assert_eq!(mods(&game), [API_264]);
    assert!(ModRecord::load_blocking(&game).files.is_empty());
}

#[tokio::test]
async fn a_jar_that_could_not_be_fetched_means_no_overlay_this_time() {
    let dir = tempfile::tempdir().unwrap();
    let game = dir.path().join("game");
    put(&game, API_264, &fabric_api_jar("0.170.0+26.4"));
    let bytes = overlay_jar("0.1.0+26.4", "~26.4", "ci");
    let content = usable(vec![entry(&bytes, "26.4")]);
    assert_eq!(
        run_with(
            &game,
            Jars {
                bundle: None,
                content: &content
            },
            None,
            &env("26.4")
        )
        .await,
        Outcome::Off(OffReason::NotFetched)
    );
    assert_eq!(mods(&game), [API_264]);
}

#[test]
fn published_jars_fill_the_versions_the_launcher_leaves_out() {
    let dir = tempfile::tempdir().unwrap();
    let bundle = three(dir.path(), "b1");
    let mut patch = entry(b"jar for 26.4.1", "26.4.1");
    patch.version = "0.1.0+26.4.1".into();
    let content = usable(vec![
        entry(b"jar for 26.3", "26.3"),
        entry(b"jar for 26.4", "26.4"),
        patch,
        entry(b"jar for 26.5", "26.5"),
    ]);
    assert_eq!(content.len(), 4);
    let jars = Jars {
        bundle: Some(&bundle),
        content: &content,
    };
    let pick = |minecraft: &str| match jars.jar_for(minecraft) {
        Some(Pick::Bundled(jar)) => format!("own {}", jar.minecraft),
        Some(Pick::Content(jar)) => format!("published {}", jar.minecraft),
        None => "none".to_owned(),
    };
    assert_eq!(pick("26.3"), "own 26.3");
    assert_eq!(pick("26.3.1"), "own 26.3");
    assert_eq!(pick("26.4"), "published 26.4");
    // Both 26.4 jars take 26.4.2: the one built for the newer version not above it.
    assert_eq!(pick("26.4.2"), "published 26.4.1");
    assert_eq!(pick("26.5"), "published 26.5");
    assert_eq!(pick("26.6"), "none");
    assert_eq!(
        jars.versions(),
        ["26.1.2", "26.2", "26.3", "26.4", "26.4.1", "26.5"]
    );
    assert!(jars.any() && Jars::bundled(None).versions().is_empty());
    assert_eq!(
        jars.jar_for("26.4").map(|pick| pick.file().to_owned()),
        Some(JAR_264.to_owned())
    );
}

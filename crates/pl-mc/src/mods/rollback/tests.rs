use std::sync::Arc;

use pl_net::{HashAlgo, hash_bytes};
use serde_json::{Value, json};

use super::*;
use crate::addons::ADDON_INDEX_FILE;
use crate::launch::crash::{CrashInput, analyze_crash, attribute_crash};
use crate::loaders::LoaderKind;
use crate::mods::apply::check_environment;
use crate::mods::apply::tests::{client, downloader, json_body, serve};
use crate::mods::check::{
    CheckEnvironment, FileOrigin, FileOrigins, IssueKind, ModFix, check_mods,
};
use crate::mods::repair::{RepairAction, RepairContext, plan_repair};
use crate::mods::scan::test_jars::{fabric_jar, fabric_json};
use crate::mods::test_support::zip_bytes;

const MC: &str = "26.3";
const NEW_FILE: &str = "EuphoriaPatcher-1.11.2-r5.10-fabric.jar";
const OLD_FILE: &str = "EuphoriaPatcher-1.10.5-r5.9.3-fabric.jar";

fn env() -> CheckEnvironment {
    check_environment(MC, LoaderKind::Fabric, Some("0.17.3"), Some(25))
}

fn sha(bytes: &[u8]) -> String {
    hash_bytes(bytes, HashAlgo::Sha1)
}

/// A Fabric jar of Euphoria Patches with one class of its own (so a frame can name its jar).
fn euphoria(version: &str, extra: &str) -> Vec<u8> {
    let json = fabric_json("euphoria_patcher", version, extra);
    zip_bytes(&[
        ("fabric.mod.json", json.as_bytes()),
        ("de/euphoria/patcher/TickHandler.class", b"\xca\xfe\xba\xbe"),
    ])
}

fn snapshot(files: &[(&str, &[u8])]) -> ModSnapshot {
    ModSnapshot::new(
        "mods",
        files
            .iter()
            .map(|(name, bytes)| ScannedJar::from_bytes(name, bytes))
            .collect(),
    )
}

fn known(files: &[(&str, &[u8])]) -> KnownFiles {
    files
        .iter()
        .map(|(name, bytes)| {
            (
                file_key(name),
                KnownFile {
                    sha1: Some(sha(bytes)),
                    ..KnownFile::default()
                },
            )
        })
        .collect()
}

fn clean(file: &str, bytes: &[u8], ids: &[&str], version_id: Option<&str>) -> CleanMod {
    CleanMod {
        file: file.into(),
        size: bytes.len() as u64,
        modified_ms: None,
        sha1: sha(bytes),
        ids: ids.iter().map(|id| (*id).to_owned()).collect(),
        version: None,
        project_id: Some("EUPH".into()),
        version_id: version_id.map(str::to_owned),
        pack_item: None,
    }
}

/// The player's report: in game, a tick of Euphoria Patches' code failed (a Fabric crash report:
/// no "Suspected Mods", the mod only in the stack trace and its mixin handler).
const TICK_CRASH: &str = "---- Minecraft Crash Report ----\n// Why did you do that?\n\n\
Time: 2026-11-24 21:14:02\nDescription: Ticking player\n\n\
java.lang.NullPointerException: Cannot invoke \"java.util.Map.get(Object)\" because \"this.cache\" is null\n\
\tat de.euphoria.patcher.TickHandler.onTick(TickHandler.java:88)\n\
\tat net.minecraft.class_746.handler$zza000$euphoria_patcher$afterTick(class_746.java:2310)\n\
\tat net.minecraft.class_746.method_5773(class_746.java:251)\n\
\tat net.minecraft.class_638.method_18646(class_638.java:310)\n\n\n\
A detailed walkthrough of the error, its code path and all known details is as follows:\n\
---------------------------------------------------------------------------------------\n\n\
-- Head --\nThread: Render thread\nStacktrace:\n\
\tat de.euphoria.patcher.TickHandler.onTick(TickHandler.java:88)\n";

fn tick_crash(mods: &ModSnapshot) -> CrashDiagnosis {
    let mut d = analyze_crash(&CrashInput {
        output: "[Render thread/INFO]: Connecting to play.projects.gg, 25565\n",
        crash_report: Some(TICK_CRASH),
        exit_code: Some(-1),
        runtime: std::time::Duration::from_secs(340),
        java_major: Some(25),
        required_java: Some(25),
        window_shown: true,
        ..CrashInput::default()
    });
    attribute_crash(&mut d, mods, &env());
    d
}

// --- the crash's candidate ---------------------------------------------------------------------

#[test]
fn a_tick_crash_of_a_mod_the_version_switch_updated_is_a_candidate() {
    let new = euphoria("1.11.2-r5.10", "");
    let old = euphoria("1.10.5-r5.9.3", "");
    let sodium = fabric_jar("sodium", "0.8.0", "");
    let mods = snapshot(&[(NEW_FILE, &new), ("sodium.jar", &sodium)]);
    let d = tick_crash(&mods);
    assert_eq!(d.kind, CrashKind::ModCrash, "{d:#?}");
    assert!(
        d.suspects
            .iter()
            .all(|s| s.reason == SuspectReason::InStackTrace),
        "a Fabric tick crash only has stack trace clues: {:?}",
        d.suspects
    );
    assert_eq!(d.suspects[0].file.as_deref(), Some(NEW_FILE));

    // The last clean game was on 26.1.2 (the switch to 26.3 is the version's first visit).
    let mut history = ModHistory::default();
    history.record_clean(
        "26.1.2",
        CleanGame {
            at_ms: 10,
            loader: "fabric".into(),
            mods: vec![
                clean(OLD_FILE, &old, &["euphoria_patcher"], Some("vE105")),
                clean("sodium.jar", &sodium, &["sodium"], None),
            ],
        },
    );
    let baseline = history.baseline(MC).expect("another version's record");
    let files = known(&[(NEW_FILE, &new), ("sodium.jar", &sodium)]);
    let c = rollback_candidate(&d, &mods, baseline, &files, &HashSet::new()).expect("changed");
    assert_eq!(c.mod_id, "euphoria_patcher");
    assert_eq!(c.file, NEW_FILE);
    assert_eq!(c.version.as_deref(), Some("1.11.2-r5.10"));
    assert_eq!(c.reason, SuspectReason::InStackTrace);
    assert_eq!(c.sha1.as_deref(), Some(sha(&new).as_str()));
    assert_eq!(
        c.before.as_ref().and_then(|b| b.version_id.as_deref()),
        Some("vE105")
    );

    // A jar a ready-made set owns is the set's to update.
    let skip: HashSet<String> = [file_key(NEW_FILE)].into();
    assert!(rollback_candidate(&d, &mods, baseline, &files, &skip).is_none());

    // The same jar as in the last clean game: nothing to go back to.
    let same = CleanGame {
        at_ms: 20,
        loader: "fabric".into(),
        mods: vec![clean(NEW_FILE, &new, &["euphoria_patcher"], None)],
    };
    assert!(rollback_candidate(&d, &mods, &same, &files, &HashSet::new()).is_none());

    // Added since the last clean game: a candidate without a version that worked.
    let without = CleanGame {
        at_ms: 20,
        loader: "fabric".into(),
        mods: vec![clean("sodium.jar", &sodium, &["sodium"], None)],
    };
    let added = rollback_candidate(&d, &mods, &without, &files, &HashSet::new()).unwrap();
    assert!(added.before.is_none());
}

#[test]
fn stack_trace_clues_count_only_without_a_stronger_one() {
    let new = euphoria("1.11.2-r5.10", "");
    let other = fabric_jar("othermod", "2.0.0", "");
    let mods = snapshot(&[(NEW_FILE, &new), ("othermod.jar", &other)]);
    let mut d = tick_crash(&mods);
    // The report also names an unchanged mod outright.
    d.suspects.insert(
        0,
        crate::launch::crash::CrashSuspect {
            mod_id: Some("othermod".into()),
            name: Some("Other".into()),
            file: Some("othermod.jar".into()),
            version: None,
            reason: SuspectReason::MixinFailed,
            related: None,
            requirement: None,
            mixin_config: None,
            class_name: None,
        },
    );
    let baseline = CleanGame {
        at_ms: 1,
        loader: "fabric".into(),
        mods: vec![
            clean("othermod.jar", &other, &["othermod"], None),
            clean(
                OLD_FILE,
                &euphoria("1.10.5", ""),
                &["euphoria_patcher"],
                None,
            ),
        ],
    };
    let files = known(&[(NEW_FILE, &new), ("othermod.jar", &other)]);
    assert!(
        rollback_candidate(&d, &mods, &baseline, &files, &HashSet::new()).is_none(),
        "the changed jar is only in the stack trace while an unchanged one failed outright"
    );
}

#[test]
fn dependency_errors_and_non_mod_crashes_are_not_candidates() {
    let new = euphoria("1.11.2-r5.10", "");
    let mods = snapshot(&[(NEW_FILE, &new)]);
    let baseline = CleanGame::default();
    let files = known(&[(NEW_FILE, &new)]);
    for kind in [
        CrashKind::MissingDependency,
        CrashKind::IncompatibleMods,
        CrashKind::GraphicsDriver,
        CrashKind::OutOfMemory,
    ] {
        let mut d = tick_crash(&mods);
        d.kind = kind;
        assert!(rollback_candidate(&d, &mods, &baseline, &files, &HashSet::new()).is_none());
    }
}

/// The real-app test's crashing jar: a "newer" Lithium whose entrypoint class is missing. Fabric
/// names the mod, so the crash is a candidate when the last clean game had the real one.
#[test]
fn a_missing_entrypoint_names_the_updated_mod() {
    let fake = zip_bytes(&[(
        "fabric.mod.json",
        br#"{"schemaVersion":1,"id":"lithium","version":"99.0.0","name":"Lithium","entrypoints":{"main":["e2e.repairrb.Crash"]}}"#,
    )]);
    let real = fabric_jar("lithium", "0.18.1+mc26.1.2", "");
    let mods = snapshot(&[("lithium-fabric-99.0.0.jar", &fake)]);
    let output = "[main/INFO] (FabricLoader/GameProvider): Loading Minecraft 26.1.2 with Fabric Loader 0.20.1\n\
Exception in thread \"main\" java.lang.RuntimeException: Could not execute entrypoint stage 'main' due to errors, provided by 'lithium' at 'e2e.repairrb.Crash'!\n\
\tat net.fabricmc.loader.impl.FabricLoaderImpl.lambda$invokeEntrypoints$2(FabricLoaderImpl.java:403)\n\
Caused by: net.fabricmc.loader.api.EntrypointException: Exception while loading entries for entrypoint 'main' provided by 'lithium'\n\
Caused by: java.lang.ClassNotFoundException: e2e.repairrb.Crash\n";
    let mut d = analyze_crash(&CrashInput {
        output,
        exit_code: Some(1),
        runtime: std::time::Duration::from_secs(9),
        java_major: Some(25),
        required_java: Some(25),
        ..CrashInput::default()
    });
    attribute_crash(
        &mut d,
        &mods,
        &check_environment("26.1.2", LoaderKind::Fabric, Some("0.20.1"), Some(25)),
    );
    assert!(
        d.suspects
            .iter()
            .any(|s| s.reason == SuspectReason::LoadingFailed
                && s.file.as_deref() == Some("lithium-fabric-99.0.0.jar")),
        "{d:#?}"
    );
    let baseline = game_of(
        1,
        vec![clean(
            "lithium-fabric-0.18.1+mc26.1.2.jar",
            &real,
            &["lithium"],
            Some("lith-v"),
        )],
    );
    let files = known(&[("lithium-fabric-99.0.0.jar", &fake)]);
    let c = rollback_candidate(&d, &mods, &baseline, &files, &HashSet::new()).expect("a candidate");
    assert_eq!(c.reason, SuspectReason::LoadingFailed);
    assert_eq!(c.mod_id, "lithium");
    assert_eq!(
        c.before.and_then(|b| b.version_id).as_deref(),
        Some("lith-v")
    );
}

// --- the version to go back to -----------------------------------------------------------------

fn mr_version(host: &str, id: &str, number: &str, games: &[&str], kind: &str, date: &str) -> Value {
    let content = format!("{id} bytes");
    json!({
        "id": id, "project_id": "EUPH", "author_id": "a", "name": number,
        "version_number": number, "changelog": null, "dependencies": [],
        "game_versions": games, "version_type": kind, "loaders": ["fabric", "neoforge"],
        "featured": false, "status": "listed", "date_published": date, "downloads": 1,
        "files": [{
            "hashes": {
                "sha1": sha(content.as_bytes()),
                "sha512": hash_bytes(content.as_bytes(), HashAlgo::Sha512)
            },
            "url": format!("{host}/files/{id}.jar"), "filename": format!("EuphoriaPatcher-{number}-fabric.jar"),
            "primary": true, "size": content.len(), "file_type": null
        }]
    })
}

/// Euphoria Patches on Modrinth as the report saw it, newest first: its 26.3 versions are
/// releases from 18–20 November, and 1.10.5-r5.9.3 also lists 26.3.
fn euphoria_versions(host: &str) -> Vec<Value> {
    vec![
        mr_version(
            host,
            "v1112",
            "1.11.2-r5.10",
            &["26.3"],
            "release",
            "2026-11-20T10:00:00Z",
        ),
        mr_version(
            host,
            "v1111",
            "1.11.1-r5.10",
            &["26.3"],
            "release",
            "2026-11-19T10:00:00Z",
        ),
        mr_version(
            host,
            "v1110",
            "1.11.0-r5.10",
            &["26.3"],
            "release",
            "2026-11-18T10:00:00Z",
        ),
        mr_version(
            host,
            "v1105",
            "1.10.5-r5.9.3",
            &["26.1.2", "26.3"],
            "release",
            "2026-10-30T10:00:00Z",
        ),
        mr_version(
            host,
            "v1104",
            "1.10.4-r5.9.2",
            &["26.1.2"],
            "release",
            "2026-10-12T10:00:00Z",
        ),
    ]
}

/// Modrinth's JSON (snake_case keys) as the launcher's model (camelCase keys).
fn camel(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, value)| {
                    let mut out = String::new();
                    let mut upper = false;
                    for c in key.chars() {
                        if c == '_' {
                            upper = true;
                        } else if upper {
                            out.extend(c.to_uppercase());
                            upper = false;
                        } else {
                            out.push(c);
                        }
                    }
                    (out, camel(value))
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.into_iter().map(camel).collect()),
        other => other,
    }
}

fn parsed(values: Vec<Value>) -> Vec<ModrinthVersion> {
    values
        .into_iter()
        .map(|v| serde_json::from_value(camel(v)).unwrap())
        .collect()
}

fn before_of(version: &ModrinthVersion) -> CleanMod {
    CleanMod {
        file: version.files[0].filename.clone(),
        sha1: version.files[0].hashes.sha1.clone().unwrap(),
        ids: vec!["euphoria_patcher".into()],
        version_id: Some(version.id.clone()),
        ..CleanMod::default()
    }
}

#[test]
fn the_last_clean_version_wins_when_it_runs_here_else_the_newest_older_one() {
    let versions = parsed(euphoria_versions("http://mock"));
    let target = ModrinthResolveTarget::for_loader(LoaderKind::Fabric, MC);
    let by_id = |id: &str| versions.iter().find(|v| v.id == id).unwrap();
    let current = by_id("v1112");
    let pick = |before: Option<&CleanMod>, current: Option<&ModrinthVersion>| {
        pick_rollback(&versions, &target, before, current, None).map(|v| v.id.as_str())
    };

    // The report: the last clean game had 1.10.5-r5.9.3, which lists 26.3.
    assert_eq!(
        pick(Some(&before_of(by_id("v1105"))), Some(current)),
        Some("v1105")
    );
    // Found by its hash when the record had no Modrinth version.
    let mut hashed = before_of(by_id("v1105"));
    hashed.version_id = None;
    assert_eq!(pick(Some(&hashed), Some(current)), Some("v1105"));
    // A last clean version made only for 26.1.2: the newest release before the crashing one.
    assert_eq!(
        pick(Some(&before_of(by_id("v1104"))), Some(current)),
        Some("v1111")
    );
    // Added since the last clean game: the same.
    assert_eq!(pick(None, Some(current)), Some("v1111"));
    // The oldest 26.3 build crashed: back to the one before it that also runs on 26.3.
    assert_eq!(
        pick(Some(&before_of(by_id("v1104"))), Some(by_id("v1110"))),
        Some("v1105")
    );
    // Without the crashing version's date nothing is "older".
    assert_eq!(pick(Some(&before_of(by_id("v1104"))), None), None);
    // The last clean version is never the crashing one itself.
    assert_eq!(
        pick_rollback(
            &versions,
            &target,
            Some(&before_of(current)),
            Some(current),
            current.files[0].hashes.sha1.as_deref(),
        )
        .map(|v| v.id.as_str()),
        Some("v1111")
    );
    // Another loader's versions never fit.
    let neo = ModrinthResolveTarget::for_loader(LoaderKind::Forge, MC);
    assert!(pick_rollback(&versions, &neo, None, Some(current), None).is_none());
}

#[test]
fn releases_come_before_betas_and_alphas() {
    let host = "http://mock";
    let versions = parsed(vec![
        mr_version(
            host,
            "beta2",
            "2.0.0-beta.2",
            &["26.3"],
            "beta",
            "2026-11-25T10:00:00Z",
        ),
        mr_version(
            host,
            "rel",
            "1.9.0",
            &["26.3"],
            "release",
            "2026-11-20T10:00:00Z",
        ),
        mr_version(
            host,
            "alpha",
            "1.9.0-alpha",
            &["26.3"],
            "alpha",
            "2026-11-15T10:00:00Z",
        ),
        mr_version(
            host,
            "beta1",
            "1.8.0-beta.1",
            &["26.3"],
            "beta",
            "2026-11-10T10:00:00Z",
        ),
    ]);
    let target = ModrinthResolveTarget::for_loader(LoaderKind::Fabric, MC);
    let current = &versions[1];
    // Only older versions: a beta before an alpha, even an older one; the newer beta never.
    assert_eq!(
        pick_rollback(&versions, &target, None, Some(current), None).map(|v| v.id.as_str()),
        Some("beta1")
    );
}

/// A fake Modrinth for one project: `version_files`, the project's versions and their files.
async fn fake_modrinth(
    files: Vec<(String, Vec<u8>)>,
) -> (String, Arc<std::sync::Mutex<Vec<String>>>) {
    let seen: Arc<std::sync::Mutex<Vec<String>>> = Arc::default();
    let log = Arc::clone(&seen);
    let files = Arc::new(files);
    let host = serve(Arc::new(move |method, url, host| {
        log.lock().unwrap().push(format!("{method} {}", url.path()));
        let versions = euphoria_versions_with_files(host, &files);
        match (method, url.path()) {
            ("POST", "/v2/version_files") => {
                let map: serde_json::Map<String, Value> = versions
                    .iter()
                    .map(|v| {
                        (
                            v["files"][0]["hashes"]["sha1"].as_str().unwrap().to_owned(),
                            v.clone(),
                        )
                    })
                    .collect();
                json_body(Value::Object(map))
            }
            ("GET", "/v2/project/EUPH/version") => json_body(json!(versions)),
            ("GET", path) if path.starts_with("/files/") => {
                let name = &path["/files/".len()..];
                files
                    .iter()
                    .find(|(n, _)| n == name)
                    .map_or((404, Vec::new()), |(_, bytes)| (200, bytes.clone()))
            }
            _ => (404, b"{}".to_vec()),
        }
    }))
    .await;
    (host, seen)
}

/// [`euphoria_versions`] whose files are `files` (by the version id's jar name), so downloads
/// match their hashes.
fn euphoria_versions_with_files(host: &str, files: &[(String, Vec<u8>)]) -> Vec<Value> {
    euphoria_versions(host)
        .into_iter()
        .map(|mut v| {
            let id = v["id"].as_str().unwrap().to_owned();
            if let Some((_, bytes)) = files.iter().find(|(n, _)| *n == format!("{id}.jar")) {
                v["files"][0]["hashes"]["sha1"] = json!(sha(bytes));
                v["files"][0]["hashes"]["sha512"] = json!(hash_bytes(bytes, HashAlgo::Sha512));
                v["files"][0]["size"] = json!(bytes.len());
            }
            v
        })
        .collect()
}

#[tokio::test]
async fn resolving_finds_the_project_by_hash_once_and_asks_for_its_versions_once() {
    let new = euphoria("1.11.2-r5.10", "");
    let old = euphoria("1.10.5-r5.9.3", "");
    let (host, seen) = fake_modrinth(vec![
        ("v1112.jar".into(), new.clone()),
        ("v1105.jar".into(), old.clone()),
    ])
    .await;
    let candidate = RollbackCandidate {
        mod_id: "euphoria_patcher".into(),
        file: NEW_FILE.into(),
        name: "Euphoria Patches".into(),
        version: Some("1.11.2-r5.10".into()),
        reason: SuspectReason::InStackTrace,
        before: Some(CleanMod {
            project_id: None,
            ..clean(OLD_FILE, &old, &["euphoria_patcher"], None)
        }),
        sha1: Some(sha(&new)),
    };
    let target = ModrinthResolveTarget::for_loader(LoaderKind::Fabric, MC);
    let found = resolve_rollback(&client(&host), &candidate, None, &target)
        .await
        .unwrap();
    let RollbackLookup::Found(plan) = found else {
        panic!("{found:?}");
    };
    assert_eq!(plan.target.id, "v1105");
    assert_eq!(plan.project_id, "EUPH");
    assert_eq!(plan.from_version(), "1.11.2-r5.10");
    assert_eq!(
        *seen.lock().unwrap(),
        ["POST /v2/version_files", "GET /v2/project/EUPH/version"]
    );

    // With the project known, only the version list is asked for.
    seen.lock().unwrap().clear();
    let found = resolve_rollback(&client(&host), &candidate, Some("EUPH"), &target)
        .await
        .unwrap();
    assert!(matches!(found, RollbackLookup::Found(_)));
    assert_eq!(*seen.lock().unwrap(), ["GET /v2/project/EUPH/version"]);

    // A mod Modrinth does not know.
    let stranger = RollbackCandidate {
        before: None,
        sha1: Some(sha(b"nobody")),
        ..candidate.clone()
    };
    let unknown = resolve_rollback(&client(&host), &stranger, None, &target)
        .await
        .unwrap();
    assert_eq!(unknown, RollbackLookup::Unknown);
}

// --- a mod of the player's ---------------------------------------------------------------------

#[tokio::test]
async fn a_players_mod_swaps_jars_through_the_trash_and_back() {
    let new = euphoria("1.11.2-r5.10", "");
    let old = euphoria("1.10.5-r5.9.3", "");
    let (host, _) = fake_modrinth(vec![
        ("v1112.jar".into(), new.clone()),
        ("v1105.jar".into(), old.clone()),
    ])
    .await;
    let dir = tempfile::tempdir().unwrap();
    let game = dir.path().join("game");
    let mods = game.join("mods");
    std::fs::create_dir_all(&mods).unwrap();
    std::fs::write(mods.join(NEW_FILE), &new).unwrap();
    std::fs::write(mods.join("sodium.jar"), fabric_jar("sodium", "0.8.0", "")).unwrap();
    let addons =
        Addons::new(&game, dir.path().join(".launcher").join(ADDON_INDEX_FILE)).without_trash();
    let trash = LauncherTrash::new(dir.path().join("trash"));
    let target = ModrinthResolveTarget::for_loader(LoaderKind::Fabric, MC);
    let versions = parsed(euphoria_versions_with_files(
        &host,
        &[
            ("v1112.jar".into(), new.clone()),
            ("v1105.jar".into(), old.clone()),
        ],
    ));
    let plan = RollbackPlan {
        candidate: RollbackCandidate {
            mod_id: "euphoria_patcher".into(),
            file: NEW_FILE.into(),
            name: "Euphoria Patches".into(),
            version: Some("1.11.2-r5.10".into()),
            reason: SuspectReason::InStackTrace,
            before: None,
            sha1: Some(sha(&new)),
        },
        project_id: "EUPH".into(),
        current: versions.iter().find(|v| v.id == "v1112").cloned(),
        target: versions.iter().find(|v| v.id == "v1105").cloned().unwrap(),
    };
    let modrinth = client(&host);
    let loader = downloader();
    let job = OwnRollbackJob {
        addons: &addons,
        client: &modrinth,
        downloader: &loader,
        trash: &trash,
        instance_id: "test",
        target,
        cancel: CancellationToken::new(),
    };
    let hold = roll_back_own(&job, &plan, "Euphoria Patches", 42)
        .await
        .unwrap();
    assert_eq!(hold.file, OLD_FILE);
    assert_eq!(hold.version, "1.10.5-r5.9.3");
    assert_eq!(hold.from_version, "1.11.2-r5.10");
    assert!(!mods.join(NEW_FILE).exists(), "the crashing jar left");
    assert_eq!(std::fs::read(mods.join(OLD_FILE)).unwrap(), old);
    assert!(mods.join("sodium.jar").exists(), "nothing else changed");
    let trashed = trash.list(Some("test")).await.unwrap();
    assert_eq!(trashed.len(), 1);
    assert_eq!(trashed[0].kind, TrashKind::Mod);
    assert_eq!(trashed[0].name, "Euphoria Patches 1.11.2-r5.10");
    assert_eq!(hold.trash_id.as_deref(), Some(trashed[0].id.as_str()));

    // "Geri Al": the crashing jar comes back, the held one goes to the trash.
    undo_own(&mods, &trash, "test", &hold).await.unwrap();
    assert_eq!(std::fs::read(mods.join(NEW_FILE)).unwrap(), new);
    assert!(!mods.join(OLD_FILE).exists());
    let left = trash.list(Some("test")).await.unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].name, "Euphoria Patches 1.10.5-r5.9.3");
}

#[tokio::test]
async fn a_failed_download_puts_the_crashing_jar_back() {
    let new = euphoria("1.11.2-r5.10", "");
    // Modrinth lists the version, but its file is not there.
    let (host, _) = fake_modrinth(vec![("v1112.jar".into(), new.clone())]).await;
    let dir = tempfile::tempdir().unwrap();
    let game = dir.path().join("game");
    let mods = game.join("mods");
    std::fs::create_dir_all(&mods).unwrap();
    std::fs::write(mods.join(NEW_FILE), &new).unwrap();
    let addons = Addons::new(&game, dir.path().join(ADDON_INDEX_FILE)).without_trash();
    let trash = LauncherTrash::new(dir.path().join("trash"));
    let versions = parsed(euphoria_versions(&host));
    let plan = RollbackPlan {
        candidate: RollbackCandidate {
            mod_id: "euphoria_patcher".into(),
            file: NEW_FILE.into(),
            name: "Euphoria Patches".into(),
            version: None,
            reason: SuspectReason::InStackTrace,
            before: None,
            sha1: None,
        },
        project_id: "EUPH".into(),
        current: None,
        target: versions.iter().find(|v| v.id == "v1105").cloned().unwrap(),
    };
    let modrinth = client(&host);
    let loader = downloader();
    let job = OwnRollbackJob {
        addons: &addons,
        client: &modrinth,
        downloader: &loader,
        trash: &trash,
        instance_id: "test",
        target: ModrinthResolveTarget::for_loader(LoaderKind::Fabric, MC),
        cancel: CancellationToken::new(),
    };
    assert!(
        roll_back_own(&job, &plan, "Euphoria Patches", 1)
            .await
            .is_err()
    );
    assert_eq!(std::fs::read(mods.join(NEW_FILE)).unwrap(), new);
    assert!(trash.list(None).await.unwrap().is_empty());
}

// --- the history -------------------------------------------------------------------------------

fn game_of(at_ms: i64, mods: Vec<CleanMod>) -> CleanGame {
    CleanGame {
        at_ms,
        loader: "fabric".into(),
        mods,
    }
}

#[test]
fn the_baseline_is_the_versions_own_record_else_the_newest_other() {
    let mut history = ModHistory::default();
    assert!(history.baseline(MC).is_none());
    history.record_clean(
        "26.1.2",
        game_of(10, vec![clean("a.jar", b"a", &["a"], None)]),
    );
    history.record_clean(
        "26.2",
        game_of(20, vec![clean("b.jar", b"b", &["b"], None)]),
    );
    assert_eq!(
        history.baseline(MC).unwrap().at_ms,
        20,
        "first visit: the newest"
    );
    history.record_clean(MC, game_of(5, vec![clean("c.jar", b"c", &["c"], None)]));
    assert_eq!(history.baseline(MC).unwrap().at_ms, 5, "the version's own");
    assert_eq!(history.baseline("26.1.2").unwrap().at_ms, 10);
}

#[test]
fn records_are_kept_for_four_versions_and_unchanged_ones_are_not_written() {
    let mut history = ModHistory::default();
    for (at, minecraft) in [
        (1, "1.21.1"),
        (2, "26.1"),
        (3, "26.1.2"),
        (4, "26.2"),
        (5, "26.3"),
    ] {
        assert!(history.record_clean(
            minecraft,
            game_of(at, vec![clean("a.jar", b"a", &["a"], None)])
        ));
    }
    assert_eq!(history.clean.len(), MAX_CLEAN_SETS);
    assert!(!history.clean.contains_key("1.21.1"), "the oldest went");
    // The same jars again, the version's record already the newest: nothing to write.
    assert!(!history.record_clean("26.3", game_of(6, vec![clean("a.jar", b"a", &["a"], None)])));
    // Another version played since: written, so it is the newest again.
    assert!(history.record_clean("26.2", game_of(7, vec![clean("a.jar", b"a", &["a"], None)])));
    assert!(history.record_clean("26.3", game_of(8, vec![clean("a.jar", b"a", &["a"], None)])));
    // Other jars: written.
    assert!(history.record_clean("26.3", game_of(9, vec![clean("b.jar", b"b", &["b"], None)])));
    let text = serde_json::to_string(&history).unwrap();
    let back: ModHistory = serde_json::from_str(&text).unwrap();
    assert_eq!(back, history);
    assert!(!text.contains('\n'), "compact");
}

fn hold(mod_id: &str, file: &str) -> ModHold {
    ModHold {
        mod_id: mod_id.into(),
        name: mod_id.into(),
        file: file.into(),
        version: "1.0".into(),
        from_version: "2.0".into(),
        ..ModHold::default()
    }
}

#[test]
fn a_clean_game_proves_held_versions_and_ends_failed_marks() {
    let mut history = ModHistory::default();
    history.hold(MC, hold("euphoria_patcher", OLD_FILE));
    let mut failed = hold("othermod", "othermod-1.jar");
    failed.failed = true;
    history.hold(MC, failed);
    assert!(history.record_clean(
        MC,
        game_of(
            1,
            vec![clean(OLD_FILE, b"old", &["euphoria_patcher"], None)]
        )
    ));
    let holds = history.holds_of(MC);
    assert_eq!(holds.len(), 1);
    assert!(holds[0].proven);
    // Proven once: the same game again changes nothing.
    assert!(!history.record_clean(
        MC,
        game_of(
            1,
            vec![clean(OLD_FILE, b"old", &["euphoria_patcher"], None)]
        )
    ));
}

#[test]
fn holds_follow_the_mods_folder_and_the_pack_overrides() {
    let mut history = ModHistory::default();
    history.hold(MC, hold("euphoria_patcher", OLD_FILE));
    let mut pack = hold("sodium", "sodium-0.7.jar");
    pack.pack_item = Some("sodium".into());
    history.hold(MC, pack);
    let old = euphoria("1.10.5", "");
    let with_jar = snapshot(&[(OLD_FILE, &old)]);
    let rollback_items: HashSet<String> = ["sodium".to_owned()].into();
    assert!(!history.reconcile(MC, &with_jar, &rollback_items));
    assert_eq!(history.holds_of(MC).len(), 2);
    // A disabled held jar stays held.
    let disabled = snapshot(&[(&format!("{OLD_FILE}.disabled"), &old)]);
    assert!(!history.reconcile(MC, &disabled, &rollback_items));
    // "Paketin Sürümüne Dön" ended the pack mod's; the player updated their own.
    assert!(history.reconcile(MC, &snapshot(&[]), &HashSet::new()));
    assert!(history.holds_of(MC).is_empty());
    assert!(history.holds.is_empty());
}

#[test]
fn a_crash_with_the_held_mod_before_a_clean_game_is_a_repeat() {
    let old = euphoria("1.10.5-r5.9.3", "");
    let mods = snapshot(&[(OLD_FILE, &old)]);
    let d = tick_crash(&mods);
    let mut history = ModHistory::default();
    history.hold(MC, hold("euphoria_patcher", OLD_FILE));
    let repeated = repeated_hold(history.holds_of(MC), &d).expect("a repeat");
    assert_eq!(repeated.mod_id, "euphoria_patcher");
    assert!(history.mark_failed(MC, "euphoria_patcher"));
    assert!(!history.mark_failed(MC, "euphoria_patcher"), "once");
    assert!(repeated_hold(history.holds_of(MC), &d).is_none());
    assert!(failed_hold(history.holds_of(MC), &d).is_some());
    assert!(
        HeldFiles::of(history.holds_of(MC)).is_empty(),
        "no longer held"
    );
    // A proven hold: a later crash is a new one.
    let mut proven = hold("euphoria_patcher", OLD_FILE);
    proven.proven = true;
    assert!(repeated_hold(&[proven], &d).is_none());
}

#[test]
fn clean_games_hash_only_new_or_changed_jars() {
    let a = fabric_jar("amod", "1.0", "");
    let b = fabric_jar("bmod", "1.0", "");
    let c = fabric_jar("cmod", "1.0", "");
    let mods = snapshot(&[
        ("a.jar", &a),
        ("b.jar", &b),
        ("c.jar", &c),
        ("off.jar.disabled", &c),
    ]);
    let mut known = known(&[("a.jar", &a)]);
    known.get_mut("a.jar").unwrap().project_id = Some("AAA".into());
    known.get_mut("a.jar").unwrap().version_id = Some("a1".into());
    let previous = game_of(1, vec![clean("b.jar", &b, &["bmod"], None)]);
    let mut hashed = Vec::new();
    let game = clean_game(&mods, "fabric", 9, &known, Some(&previous), |path| {
        hashed.push(path.to_path_buf());
        Some(sha(&c))
    });
    assert_eq!(
        hashed,
        [std::path::PathBuf::from("c.jar")],
        "a known, b carried, the disabled jar left out"
    );
    assert_eq!(game.mods.len(), 3);
    assert_eq!(game.at_ms, 9);
    let a_mod = &game.mods[0];
    assert_eq!(a_mod.sha1, sha(&a));
    assert_eq!(a_mod.ids, ["amod"]);
    assert_eq!(a_mod.version.as_deref(), Some("1.0"));
    assert_eq!(a_mod.project_id.as_deref(), Some("AAA"));
    assert_eq!(a_mod.version_id.as_deref(), Some("a1"));
    assert_eq!(game.mods[1].sha1, sha(&b));
    assert_eq!(game.mods[1].project_id.as_deref(), Some("EUPH"), "carried");
}

// --- repairs leave held jars alone -------------------------------------------------------------

#[test]
fn repairs_do_not_move_held_jars_to_another_version() {
    // The held version declares only 26.1.2 although Modrinth lists 26.3 for it: the check warns,
    // and "Onar" would update it.
    let old = euphoria("1.10.5-r5.9.3", r#""depends": {"minecraft": "26.1.2"}"#);
    let mods = snapshot(&[(OLD_FILE, &old)]);
    let env = env();
    let origins = FileOrigins::new();
    let report = check_mods(&mods, &env, &origins);
    assert!(
        report
            .issues
            .iter()
            .any(|i| i.kind == IssueKind::IncompatibleMinecraft),
        "{report:#?}"
    );
    let plan_with = |held: &HeldFiles, origins: &FileOrigins| {
        let ctx = RepairContext {
            env: &env,
            origins,
            has_pack: false,
            pack_upgrades: false,
            held,
        };
        plan_repair(&ctx, Some(&report), None)
    };
    let free = plan_with(HeldFiles::none(), &origins);
    assert!(
        free.steps
            .iter()
            .any(|s| matches!(&s.action, RepairAction::UpdateToCompatible { file, .. } if file == OLD_FILE)),
        "{free:#?}"
    );
    let held = HeldFiles::of(&[hold("euphoria_patcher", OLD_FILE)]);
    let plan = plan_with(&held, &origins);
    assert!(
        !plan.steps.iter().any(|s| matches!(
            &s.action,
            RepairAction::UpdateToCompatible { .. } | RepairAction::RevertPackFile { .. }
        )),
        "{plan:#?}"
    );
    assert!(
        plan.unresolved
            .iter()
            .any(|i| i.kind == IssueKind::IncompatibleMinecraft),
        "the issue stays, unresolved: {plan:#?}"
    );

    // A pack mod held by its rollback override: not put back to the pack's version either.
    let mut pack_origins = FileOrigins::new();
    pack_origins.insert(
        OLD_FILE.into(),
        FileOrigin::Pack {
            path: format!("mods/{OLD_FILE}"),
        },
    );
    let plan = plan_with(&held, &pack_origins);
    assert!(
        !plan.steps.iter().any(|s| matches!(
            &s.action,
            RepairAction::RevertPackFile { .. } | RepairAction::UpgradePackFile { .. }
        )),
        "{plan:#?}"
    );

    // A crash's own fix for the held jar is dropped as well.
    let mut d = tick_crash(&mods);
    d.mod_fixes = vec![ModFix::UpdateToCompatible {
        file: OLD_FILE.into(),
        mod_id: "euphoria_patcher".into(),
        version_req: None,
        disable_if_unavailable: false,
    }];
    let ctx = RepairContext {
        env: &env,
        origins: &origins,
        has_pack: false,
        pack_upgrades: false,
        held: &held,
    };
    let plan = plan_repair(&ctx, None, Some(&d));
    assert!(plan.steps.is_empty(), "{plan:#?}");
}

#[test]
fn bulk_updates_skip_held_jars_but_one_named_alone_goes() {
    let holds = [hold("a", "Mod-1.0.jar")];
    let crashed = update_to("v2", "2.0", "2026-11-01T00:00:00Z");
    assert!(
        left_out_of_update(&holds, "mod-1.0.jar.disabled", 0, &crashed),
        "every update"
    );
    assert!(
        left_out_of_update(&holds, "Mod-1.0.jar", 2, &crashed),
        "a list: Tümünü Güncelle"
    );
    assert!(
        !left_out_of_update(&holds, "Mod-1.0.jar", 1, &crashed),
        "named on its own: the way out"
    );
    assert!(!left_out_of_update(&holds, "Other-2.0.jar", 0, &crashed));
    let mut failed = hold("a", "Mod-1.0.jar");
    failed.failed = true;
    assert!(
        !left_out_of_update(&[failed], "Mod-1.0.jar", 0, &crashed),
        "a hold that did not help holds nothing"
    );
}

fn update_to(id: &str, number: &str, published: &str) -> AddonUpdate {
    AddonUpdate {
        version_id: id.into(),
        version_number: number.into(),
        version_type: crate::modrinth::ModrinthVersionType::Release,
        date_published: published.into(),
    }
}

/// A hold protects against the version that crashed, not against the fix: a newer version is
/// offered by the usual update prompts again.
#[test]
fn a_hold_hides_the_crashed_version_and_older_ones_only() {
    let mut held = hold("a", "Mod-1.0.jar");
    held.from_version_id = Some("v2".into());
    held.from_published = Some("2026-11-01T00:00:00Z".into());
    let holds = [held.clone()];
    // The version that crashed, and an older one Modrinth offers instead: still held.
    let crashed = update_to("v2", "2.0", "2026-11-01T00:00:00Z");
    assert!(held.hides(&crashed));
    assert!(held.hides(&update_to("v1b", "1.5", "2026-10-01T00:00:00Z")));
    assert_eq!(
        hold_hiding(&holds, "Mod-1.0.jar", &crashed).map(|h| h.from_version.as_str()),
        Some("2.0")
    );
    // A newer version: offered as usual, also by "Tümünü Güncelle".
    let fix = update_to("v3", "3.0", "2026-12-01T00:00:00Z");
    assert!(!held.hides(&fix));
    assert!(hold_hiding(&holds, "Mod-1.0.jar", &fix).is_none());
    assert!(!left_out_of_update(&holds, "Mod-1.0.jar", 0, &fix));
    assert!(!left_out_of_update(&holds, "Mod-1.0.jar", 3, &fix));
    // Without dates only the crashed version itself, by its number.
    let undated = hold("b", "Other-1.0.jar");
    assert!(undated.hides(&update_to("x", "2.0", "")));
    assert!(!undated.hides(&update_to("y", "2.1", "")));
    // A hold that did not help hides nothing.
    held.failed = true;
    assert!(!held.hides(&crashed));
}

#[test]
fn a_failed_pack_hold_lets_its_override_go_but_stays_to_say_so() {
    let mut history = ModHistory::default();
    let mut sodium = hold("sodium", "sodium-0.7.jar");
    sodium.pack_item = Some("sodium".into());
    history.hold(MC, sodium);
    let mut iris = hold("iris", "iris-1.jar");
    iris.pack_item = Some("iris".into());
    history.hold(MC, iris);
    let both: HashSet<String> = ["sodium".to_owned(), "iris".to_owned()].into();
    assert_eq!(history.held_pack_items(MC), both);
    assert!(history.mark_failed(MC, "sodium"));
    // Its override ends (the sync puts the pack's file back)...
    let iris_only: HashSet<String> = ["iris".to_owned()].into();
    assert_eq!(history.held_pack_items(MC), iris_only);
    // ...and the hold stays, failed, until a clean game says the mods work.
    assert!(!history.reconcile(MC, &snapshot(&[]), &iris_only));
    assert_eq!(history.holds_of(MC).len(), 2);
    assert!(history.held_pack_items("1.20.1").is_empty());
}

#[test]
fn a_named_update_ends_the_hold_even_under_the_same_file_name() {
    let mut history = ModHistory::default();
    history.hold(MC, hold("a", "Mod.jar"));
    let mut pack = hold("p", "Pack-1.0.jar");
    pack.pack_item = Some("item-p".into());
    history.hold(MC, pack);
    assert!(!history.release_files(MC, &["Other.jar".to_owned()]));
    assert!(history.release_files(MC, &["mod.jar.disabled".to_owned()]));
    let left: Vec<&str> = history
        .holds_of(MC)
        .iter()
        .map(|h| h.mod_id.as_str())
        .collect();
    assert_eq!(left, ["p"]);
    assert!(
        !history.release_files(MC, &["Pack-1.0.jar".to_owned()]),
        "a pack hold ends with its override"
    );
    assert!(!history.release_files("1.20.1", &["Mod.jar".to_owned()]));
}

#[test]
fn held_files_ignore_case_and_disabled_names() {
    let held = HeldFiles::of(&[hold("a", "Mod-1.0.jar")]);
    assert!(held.contains("mod-1.0.jar"));
    assert!(held.contains("Mod-1.0.jar.disabled"));
    assert!(!held.contains("Mod-2.0.jar"));
    assert!(!HeldFiles::none().contains("Mod-1.0.jar"));
    assert_eq!(
        hold_of(&[hold("a", "Mod-1.0.jar")], "MOD-1.0.JAR").map(|h| h.mod_id.as_str()),
        Some("a")
    );
}

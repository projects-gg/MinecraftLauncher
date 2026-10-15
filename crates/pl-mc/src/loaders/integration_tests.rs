//! End-to-end loader installs against the real loader servers: Fabric (1.21.1 and the newest
//! year-based release it supports), Quilt (1.21.1), Forge (1.20.1, 1.12.2, 1.7.10; recommended
//! builds) and NeoForge (1.21.1 and the newest year-based release with a stable build). Downloads
//! several hundred MB and runs the Forge/NeoForge installer processors, so ignored by default.
//! Run with (data root override: `PL_MC_TEST_ROOT`):
//!
//! ```text
//! CARGO_TARGET_DIR=D:/ProLauncher-build/target cargo test -p pl-mc --lib -- --ignored install_loaders --nocapture
//! ```
//!
//! The game is never started: after each install the launch command is built offline from the
//! installed files and every classpath / module path entry is checked.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use parking_lot::Mutex;
use pl_net::{Http, HttpConfig, VerifyMode};

use crate::install::{InstallOptions, InstallProgressFn, InstallStage, McContext, install_version};
use crate::launch::{LaunchIdentity, LaunchOptions, build_launch_command};
use crate::loaders::{LoaderKind, LoaderVersion, install_loader, list_loader_versions};
use crate::paths::DataDirs;
use crate::version::{VersionType, load_manifest, resolve_version};

fn test_root() -> PathBuf {
    std::env::var_os("PL_MC_TEST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("D:/ProLauncher-build/mc-test"))
}

fn is_year_version(id: &str) -> bool {
    id.split('.')
        .next()
        .and_then(|major| major.parse::<u32>().ok())
        .is_some_and(|major| major >= 20)
}

/// The version to install: the recommended one, else the newest stable one.
fn pick(versions: &[LoaderVersion]) -> Option<LoaderVersion> {
    versions
        .iter()
        .find(|v| v.recommended)
        .or_else(|| versions.iter().find(|v| v.stable))
        .cloned()
}

struct Case {
    kind: LoaderKind,
    mc: String,
    loader: LoaderVersion,
    main_class: &'static str,
    list_secs: f64,
    listed: usize,
}

fn expected_main_class(kind: LoaderKind, mc: &str) -> &'static str {
    match kind {
        LoaderKind::Fabric => "net.fabricmc.loader.impl.launch.knot.KnotClient",
        LoaderKind::Quilt => "org.quiltmc.loader.impl.launch.knot.KnotClient",
        LoaderKind::Forge if mc == "1.20.1" => "cpw.mods.bootstraplauncher.BootstrapLauncher",
        LoaderKind::Forge => "net.minecraft.launchwrapper.Launch",
        LoaderKind::NeoForge if is_year_version(mc) => "net.neoforged.fml.startup.Client",
        LoaderKind::NeoForge => "cpw.mods.bootstraplauncher.BootstrapLauncher",
        LoaderKind::Vanilla => "net.minecraft.client.main.Main",
    }
}

/// Entries of the JVM option following `flag` (`-cp`, `-p`), split on the classpath separator.
fn option_entries(args: &[String], flag: &str, separator: &str) -> Vec<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(|value| value.split(separator).map(str::to_owned).collect())
        .unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "downloads loaders, Minecraft versions and Java runtimes; runs installer processors"]
async fn install_loaders() {
    let root = test_root();
    let dirs = DataDirs::new(&root);
    dirs.ensure().unwrap();
    let http = Http::new(HttpConfig::new("ProjectsLauncher-tests/2.0")).unwrap();
    let ctx = McContext::new(http, dirs.clone());
    println!("platform: {}, data root: {}", ctx.platform, root.display());
    let manifest = load_manifest(&ctx, false).await.unwrap();
    let year_releases: Vec<String> = manifest
        .of_type(VersionType::Release)
        .filter(|v| is_year_version(&v.id))
        .map(|v| v.id.clone())
        .collect();

    // Pick the versions to install.
    let mut cases: Vec<Case> = Vec::new();
    let mut plan: Vec<(LoaderKind, String)> = vec![(LoaderKind::Fabric, "1.21.1".into())];
    for mc in &year_releases {
        let started = Instant::now();
        let versions = list_loader_versions(&ctx, LoaderKind::Fabric, mc)
            .await
            .unwrap();
        if versions.iter().any(|v| v.stable) {
            println!(
                "newest year-based release with Fabric: {mc} ({:.2}s)",
                started.elapsed().as_secs_f64()
            );
            plan.push((LoaderKind::Fabric, mc.clone()));
            break;
        }
    }
    plan.extend([
        (LoaderKind::Quilt, "1.21.1".into()),
        (LoaderKind::Forge, "1.20.1".into()),
        (LoaderKind::Forge, "1.12.2".into()),
        (LoaderKind::Forge, "1.7.10".into()),
        (LoaderKind::NeoForge, "1.21.1".into()),
    ]);
    for mc in &year_releases {
        let versions = list_loader_versions(&ctx, LoaderKind::NeoForge, mc)
            .await
            .unwrap();
        if versions.iter().any(|v| v.stable) {
            println!("newest year-based release with a stable NeoForge: {mc}");
            plan.push((LoaderKind::NeoForge, mc.clone()));
            break;
        }
    }
    for (kind, mc) in plan {
        let started = Instant::now();
        let versions = list_loader_versions(&ctx, kind, &mc).await.unwrap();
        let list_secs = started.elapsed().as_secs_f64();
        let loader = pick(&versions)
            .unwrap_or_else(|| panic!("no stable {kind:?} version for {mc}: {versions:?}"));
        assert!(
            versions.iter().filter(|v| v.recommended).count() <= 1,
            "{kind:?} {mc}: at most one recommended version"
        );
        cases.push(Case {
            kind,
            main_class: expected_main_class(kind, &mc),
            mc,
            loader,
            list_secs,
            listed: versions.len(),
        });
    }

    let mut report = Vec::new();
    for case in &cases {
        let label = format!(
            "{} {} on {}",
            case.kind.display_name(),
            case.loader.version,
            case.mc
        );
        println!(
            "\n== {label} (listed {} versions in {:.2}s)",
            case.listed, case.list_secs
        );

        let stages: Arc<Mutex<Option<InstallStage>>> = Arc::new(Mutex::new(None));
        let stage_log = Arc::clone(&stages);
        let clock = Instant::now();
        let progress: InstallProgressFn = Arc::new(move |p| {
            let mut last = stage_log.lock();
            if *last != Some(p.stage) {
                println!("  [{:>6.1}s] {:?}", clock.elapsed().as_secs_f64(), p.stage);
                *last = Some(p.stage);
            }
        });
        let game_dir = root
            .join("instances")
            .join(format!("test-{:?}-{}", case.kind, case.mc).to_lowercase())
            .join("minecraft");

        // Full install (verifies every existing file).
        let started = Instant::now();
        let id = install_loader(
            &ctx,
            &case.mc,
            case.kind,
            &case.loader.version,
            InstallOptions {
                game_dir: Some(game_dir.clone()),
                progress: Some(progress),
                ..InstallOptions::default()
            },
        )
        .await
        .unwrap_or_else(|e| panic!("{label}: install failed: {e}"));
        let install_secs = started.elapsed().as_secs_f64();
        assert!(dirs.version_json(&id).unwrap().is_file());

        // Pre-launch check (sizes only) and an offline re-install.
        let started = Instant::now();
        let again = install_loader(
            &ctx,
            &case.mc,
            case.kind,
            &case.loader.version,
            InstallOptions {
                verify: VerifyMode::Size,
                game_dir: Some(game_dir.clone()),
                ..InstallOptions::default()
            },
        )
        .await
        .unwrap_or_else(|e| panic!("{label}: size check failed: {e}"));
        let quick_secs = started.elapsed().as_secs_f64();
        assert_eq!(again, id);
        let started = Instant::now();
        let offline = install_loader(
            &ctx,
            &case.mc,
            case.kind,
            &case.loader.version,
            InstallOptions {
                offline: true,
                game_dir: Some(game_dir.clone()),
                ..InstallOptions::default()
            },
        )
        .await
        .unwrap_or_else(|e| panic!("{label}: offline install failed: {e}"));
        let offline_secs = started.elapsed().as_secs_f64();
        assert_eq!(offline, id);

        // Launch command built offline from the installed files.
        let resolved = resolve_version(&ctx, &id, None).await.unwrap();
        assert_eq!(resolved.main_class, case.main_class, "{label}: main class");
        assert_eq!(
            resolved.chain.last().map(String::as_str),
            Some(case.mc.as_str())
        );
        let installed = install_version(
            &ctx,
            &resolved,
            InstallOptions {
                offline: true,
                game_dir: Some(game_dir.clone()),
                ..InstallOptions::default()
            },
        )
        .await
        .unwrap_or_else(|e| panic!("{label}: offline install_version failed: {e}"));
        let mut options = LaunchOptions::new(installed.java.path.clone(), game_dir.clone());
        options.max_memory_mb = Some(2048);
        let command = build_launch_command(
            &dirs,
            &resolved,
            &installed,
            &LaunchIdentity::offline("Tester"),
            &options,
        )
        .unwrap();
        let separator = installed.platform.classpath_separator();
        let classpath = option_entries(&command.args, "-cp", separator);
        assert!(!classpath.is_empty(), "{label}: no -cp");
        for entry in &classpath {
            assert!(
                Path::new(entry).is_file(),
                "{label}: classpath entry missing: {entry}"
            );
        }
        let module_path = option_entries(&command.args, "-p", separator);
        for entry in &module_path {
            assert!(
                Path::new(entry).is_file(),
                "{label}: module path entry missing: {entry}"
            );
        }
        assert!(command.args.contains(&resolved.main_class));
        assert!(
            command.args.iter().all(|a| !a.contains("${")),
            "{label}: unsubstituted placeholder in {:?}",
            command.redacted_args()
        );

        // Everything the installer step produced is still there.
        let marker = dirs.version_dir(&id).unwrap().join("projects-loader.json");
        let mut produced = 0;
        if matches!(case.kind, LoaderKind::Forge | LoaderKind::NeoForge) {
            let value: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&marker).unwrap()).unwrap();
            for file in value["files"].as_array().unwrap() {
                let path = root.join(file["path"].as_str().unwrap());
                assert!(
                    path.is_file(),
                    "{label}: installer output missing: {}",
                    path.display()
                );
                produced += 1;
            }
        }

        let line = format!(
            "{label}: id {id} | install {install_secs:.1}s | size check {quick_secs:.2}s | offline {offline_secs:.2}s | java {} | {} classpath + {} module path entries | {produced} installer files",
            installed.java.info.major,
            classpath.len(),
            module_path.len(),
        );
        println!("{line}");
        report.push(line);
    }
    println!("\n==== summary ====\n{}", report.join("\n"));
}

//! End-to-end install against Mojang's servers: latest release, a year-based release, 1.21.1,
//! 1.12.2, 1.8.9 and 1.7.10 plus their Java runtimes. Several GB of downloads, so ignored by
//! default. Run with (data root override: `PL_MC_TEST_ROOT`):
//!
//! ```text
//! CARGO_TARGET_DIR=D:/ProLauncher-build/target cargo test -p pl-mc --lib -- --ignored install_vanilla --nocapture
//! ```
//!
//! The game itself is never started: the test checks files, the launch command and that the
//! provisioned Java answers `-version`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use parking_lot::Mutex;
use pl_net::{Http, HttpConfig, VerifyMode};

use crate::install::{InstallOptions, InstallProgressFn, InstallStage, McContext, install_version};
use crate::java::probe_java;
use crate::launch::{LaunchIdentity, LaunchOptions, build_launch_command};
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

fn count_native_files(dir: &Path) -> usize {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
        .filter(|e| crate::install::is_native_library(&e.file_name().to_string_lossy()))
        .count()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "downloads Minecraft versions and Java runtimes (several GB)"]
async fn install_vanilla_versions() {
    let root = test_root();
    let dirs = DataDirs::new(&root);
    dirs.ensure().unwrap();
    let http = Http::new(HttpConfig::new("ProjectsLauncher-tests/2.0")).unwrap();
    let ctx = McContext::new(http, dirs.clone());
    println!("platform: {}", ctx.platform);

    let manifest = load_manifest(&ctx, false).await.unwrap();
    let latest = manifest.latest.release.clone();
    let mut ids = vec![latest.clone()];
    let year_release = manifest
        .of_type(VersionType::Release)
        .filter(|v| v.id != latest && is_year_version(&v.id))
        .max_by_key(|v| (v.id.matches('.').count() == 2, v.release_timestamp()))
        .map(|v| v.id.clone());
    if let Some(year) = year_release {
        ids.push(year);
    }
    for id in ["1.21.1", "1.12.2", "1.8.9", "1.7.10"] {
        if !ids.iter().any(|i| i == id) {
            ids.push(id.to_owned());
        }
    }
    println!("versions: {ids:?}");

    let mut report = Vec::new();
    for id in &ids {
        let started = Instant::now();
        let resolved = resolve_version(&ctx, id, Some(&manifest)).await.unwrap();
        assert!(dirs.version_json(id).unwrap().is_file());
        let game_dir = root
            .join("instances")
            .join(format!("test-{id}"))
            .join("minecraft");

        let last_stage: Arc<Mutex<Option<InstallStage>>> = Arc::new(Mutex::new(None));
        let stage_log = Arc::clone(&last_stage);
        let stage_clock = Instant::now();
        let progress: InstallProgressFn = Arc::new(move |p| {
            let mut last = stage_log.lock();
            if *last != Some(p.stage) {
                println!(
                    "  [{id_secs:>6.1}s] {:?}",
                    p.stage,
                    id_secs = stage_clock.elapsed().as_secs_f64()
                );
                *last = Some(p.stage);
            }
        });
        let installed = install_version(
            &ctx,
            &resolved,
            InstallOptions {
                game_dir: Some(game_dir.clone()),
                progress: Some(progress),
                ..InstallOptions::default()
            },
        )
        .await
        .unwrap_or_else(|e| panic!("{id}: install failed: {e}"));
        let install_secs = started.elapsed().as_secs_f64();
        assert_eq!(*last_stage.lock(), Some(InstallStage::Done));

        // Files.
        for entry in &installed.classpath {
            assert!(
                entry.is_file(),
                "{id}: missing classpath entry {}",
                entry.display()
            );
        }
        assert!(installed.client_jar.is_file());
        let natives = count_native_files(&installed.natives_dir);
        assert!(natives > 0, "{id}: natives directory is empty");
        let index = dirs.asset_index_file(resolved.assets_id()).unwrap();
        assert!(index.is_file(), "{id}: asset index missing");
        if resolved.logging.is_some() {
            assert!(
                installed
                    .logging_config
                    .as_ref()
                    .is_some_and(|p| p.is_file())
            );
        }

        // Java.
        let java_started = Instant::now();
        let info = probe_java(&installed.java.path).await.unwrap();
        let java_secs = java_started.elapsed().as_secs_f64();
        assert_eq!(info.major, resolved.java_major(), "{id}: wrong Java major");

        // Launch command (offline player); never executed.
        let mut options = LaunchOptions::new(installed.java.path.clone(), game_dir.clone());
        options.min_memory_mb = Some(1024);
        options.max_memory_mb = Some(2048);
        let command = build_launch_command(
            &dirs,
            &resolved,
            &installed,
            &LaunchIdentity::offline("Tester"),
            &options,
        )
        .unwrap();
        let cp = command
            .args
            .iter()
            .position(|a| a == "-cp")
            .map(|i| command.args[i + 1].clone())
            .unwrap_or_else(|| panic!("{id}: no -cp"));
        for entry in cp.split(installed.platform.classpath_separator()) {
            assert!(
                Path::new(entry).is_file(),
                "{id}: classpath entry missing: {entry}"
            );
        }
        assert!(
            command.args.iter().all(|a| !a.contains("${")),
            "{id}: {:?}",
            command.args
        );
        assert!(command.args.contains(&resolved.main_class));

        // Routine pre-launch check: online, sizes only (what the app does before every launch).
        let quick_started = Instant::now();
        let quick = install_version(
            &ctx,
            &resolved,
            InstallOptions {
                verify: VerifyMode::Size,
                game_dir: Some(game_dir.clone()),
                ..InstallOptions::default()
            },
        )
        .await
        .unwrap_or_else(|e| panic!("{id}: quick check failed: {e}"));
        let quick_secs = quick_started.elapsed().as_secs_f64();
        assert_eq!(quick.classpath, installed.classpath);

        // Repeat offline: no network, everything already there.
        let offline_started = Instant::now();
        let offline = install_version(
            &ctx,
            &resolved,
            InstallOptions {
                offline: true,
                game_dir: Some(game_dir.clone()),
                ..InstallOptions::default()
            },
        )
        .await
        .unwrap_or_else(|e| panic!("{id}: offline check failed: {e}"));
        let offline_secs = offline_started.elapsed().as_secs_f64();
        assert_eq!(offline.classpath, installed.classpath);
        assert_eq!(offline.java.path, installed.java.path);

        let line = format!(
            "{id}: install/full verify {install_secs:.1}s, size check {quick_secs:.2}s, offline check {offline_secs:.2}s, java {} [{}] ({java_secs:.2}s probe), {} classpath entries, {} natives, assets {}",
            info.version,
            resolved.java_component(),
            installed.classpath.len(),
            natives,
            resolved.assets_id(),
        );
        println!("{line}");
        report.push(line);
    }
    println!("\n==== summary ====\n{}", report.join("\n"));
}

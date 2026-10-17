//! Live end-to-end check of the content features against the real Modrinth API (no game
//! launch): installs into a throwaway Fabric 1.21.1 instance, updates, removes, imports a small
//! `.mrpack`, exports one, then deletes the throwaway instances.
//!
//! ```text
//! PROJECTS_LAUNCHER_DATA_DIR=D:/ProLauncher-build/mc-test \
//!   cargo run -p pl-mc --example content_live -- D:/ProLauncher-build/tmp
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;

use pl_mc::addons::{ADDON_INDEX_FILE, AddonInstallOptions, AddonProgress, Addons};
use pl_mc::gamefiles::ImportKind;
use pl_mc::instance::{InstanceLoader, InstanceStore, NewInstance};
use pl_mc::loaders::LoaderKind;
use pl_mc::modpack::{
    ImportPendingFiles, MrpackExport, identify_hosted_files, import_pack, mrpack_download_items,
    preview_pack, read_mrpack_index, write_mrpack,
};
use pl_mc::modrinth::{
    ModrinthClient, ModrinthProjectType, ModrinthResolveTarget, ModrinthSearchQuery,
    ModrinthSortIndex,
};
use pl_mc::paths::DataDirs;
use pl_net::{CancellationToken, DownloadOptions, Downloader, HashAlgo, Http, HttpConfig};

type Res<T> = Result<T, Box<dyn std::error::Error>>;

fn check(ok: bool, what: &str) -> Res<()> {
    if ok {
        println!("  ok  {what}");
        Ok(())
    } else {
        Err(format!("FAILED: {what}").into())
    }
}

async fn sha(path: &Path, algo: HashAlgo) -> Res<String> {
    Ok(pl_net::hash_file(path, algo).await?)
}

fn addons_for(store: &InstanceStore, root: &Path, id: &str) -> Res<Addons> {
    let instance_dir = store.instance_dir(id)?;
    Ok(Addons::new(
        store.game_dir(id)?,
        instance_dir.join(".launcher").join(ADDON_INDEX_FILE),
    )
    .with_icon_dir(root.join("cache").join("modrinth").join("icons"))
    .without_trash())
}

/// Every installed file must match the hashes Modrinth publishes for its version.
async fn verify_against_modrinth(client: &ModrinthClient, addons: &Addons) -> Res<()> {
    let scan = addons.identify(client, ImportKind::Mod).await?;
    check(!scan.offline, "Modrinth reachable for identification")?;
    for entry in &scan.entries {
        let source = entry
            .source
            .as_ref()
            .ok_or_else(|| format!("{} not identified", entry.file_name))?;
        let version = client.version(&source.version_id).await?;
        let file = version
            .files
            .iter()
            .find(|f| f.hashes.sha1.as_deref() == Some(entry.sha1.as_str()))
            .ok_or_else(|| format!("{} not among the files of {}", entry.file_name, version.id))?;
        let path = addons.game_dir().join("mods").join(&entry.file_name);
        let sha512 = sha(&path, HashAlgo::Sha512).await?;
        check(
            file.hashes.sha512.as_deref() == Some(sha512.as_str()),
            &format!(
                "{} sha512 matches {} {}",
                entry.file_name,
                source.title.as_deref().unwrap_or("?"),
                version.version_number
            ),
        )?;
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Res<()> {
    let scratch = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "D:/ProLauncher-build/tmp".into()),
    );
    let root = PathBuf::from(
        std::env::var("PROJECTS_LAUNCHER_DATA_DIR")
            .unwrap_or_else(|_| "D:/ProLauncher-build/mc-test".into()),
    );
    let dirs = DataDirs::new(&root);
    let store = InstanceStore::new(&dirs).without_trash();
    let http = Http::new(HttpConfig::new(
        "ProjectsLauncher/2.0.0-dev (+https://projects.gg)",
    ))?;
    let client = ModrinthClient::new(http.clone());
    let downloader = Downloader::new(http.clone());
    let mut cleanup: Vec<String> = Vec::new();

    let result = run(&store, &root, &scratch, &client, &downloader, &mut cleanup).await;
    for id in cleanup {
        match store.delete(&id).await {
            Ok(()) => println!("deleted throwaway instance {id}"),
            Err(error) => println!("could not delete {id}: {error}"),
        }
    }
    result
}

async fn run(
    store: &InstanceStore,
    root: &Path,
    scratch: &Path,
    client: &ModrinthClient,
    downloader: &Downloader,
    cleanup: &mut Vec<String>,
) -> Res<()> {
    println!("== mods into a Fabric 1.21.1 instance");
    let instance = store
        .create(NewInstance {
            name: "content live test".into(),
            mc_version: "1.21.1".into(),
            loader: InstanceLoader {
                kind: LoaderKind::Fabric,
                version: Some("0.16.14".into()),
            },
            ..NewInstance::default()
        })
        .await?;
    cleanup.push(instance.id.clone());
    let addons = addons_for(store, root, &instance.id)?;
    let target = ModrinthResolveTarget::for_loader(LoaderKind::Fabric, "1.21.1");
    let progress_seen = Arc::new(parking_lot::Mutex::new(Vec::<AddonProgress>::new()));
    let seen = Arc::clone(&progress_seen);
    let options = AddonInstallOptions {
        progress: Some(Arc::new(move |p: AddonProgress| seen.lock().push(p))),
        ..AddonInstallOptions::default()
    };

    // An older Sodium first, so an update exists.
    let sodium_versions = client
        .project_versions("sodium", &target.loaders, &target.game_versions)
        .await?;
    let releases: Vec<_> = sodium_versions
        .iter()
        .filter(|v| v.version_type == pl_mc::modrinth::ModrinthVersionType::Release)
        .collect();
    let older = releases
        .get(1)
        .copied()
        .ok_or("Sodium has fewer than two 1.21.1 releases")?
        .clone();
    let report = addons
        .install(
            client,
            downloader,
            ImportKind::Mod,
            vec![older.clone()],
            &target,
            options.clone(),
        )
        .await?;
    check(
        report.installed.len() == 1 && report.installed[0].project_id == "AANobbMI",
        &format!("installed older Sodium {}", older.version_number),
    )?;
    {
        let progress = progress_seen.lock();
        check(
            progress
                .iter()
                .any(|p| matches!(p, AddonProgress::Downloading { .. })),
            "download progress reported",
        )?;
    }

    // Mod Menu needs Fabric API (and Placeholder API): the resolver brings them along.
    let modmenu = client
        .best_version("modmenu", &target)
        .await?
        .ok_or("no Mod Menu for 1.21.1")?;
    let report = addons
        .install(
            client,
            downloader,
            ImportKind::Mod,
            vec![modmenu],
            &target,
            options.clone(),
        )
        .await?;
    let names: Vec<String> = report
        .installed
        .iter()
        .map(|i| {
            format!(
                "{}{}",
                i.title.clone().unwrap_or_default(),
                if i.dependency { " (dependency)" } else { "" }
            )
        })
        .collect();
    println!("  installed: {}", names.join(", "));
    check(
        report
            .installed
            .iter()
            .any(|i| i.project_id == "P7dR8mSH" && i.dependency),
        "Fabric API pulled in as a dependency",
    )?;
    check(report.unresolved.is_empty(), "no unresolved dependencies")?;
    verify_against_modrinth(client, &addons).await?;

    println!("== update check");
    let scan = addons
        .check_updates(client, ImportKind::Mod, &target)
        .await?;
    let sodium = scan
        .entries
        .iter()
        .find(|e| {
            e.source
                .as_ref()
                .is_some_and(|s| s.project_id == "AANobbMI")
        })
        .ok_or("Sodium missing from the scan")?;
    let update = sodium.update.clone().ok_or("no Sodium update offered")?;
    check(
        update.version_id != older.id,
        &format!("Sodium update {} offered", update.version_number),
    )?;
    check(
        scan.entries
            .iter()
            .filter(|e| {
                e.source
                    .as_ref()
                    .is_some_and(|s| s.project_id != "AANobbMI")
            })
            .all(|e| e.update.is_none()),
        "freshly installed mods have no update",
    )?;
    let latest = client.version(&update.version_id).await?;
    let report = addons
        .install(
            client,
            downloader,
            ImportKind::Mod,
            vec![latest],
            &target,
            options.clone(),
        )
        .await?;
    check(
        report.replaced.len() == 1,
        &format!("old Sodium replaced: {:?}", report.replaced),
    )?;
    let mods_dir = addons.game_dir().join("mods");
    let sodium_files = std::fs::read_dir(&mods_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains("sodium"))
        .count();
    check(sodium_files == 1, "exactly one Sodium jar left")?;
    let scan = addons
        .check_updates(client, ImportKind::Mod, &target)
        .await?;
    check(
        scan.entries.iter().all(|e| e.update.is_none()),
        "no updates after updating",
    )?;
    verify_against_modrinth(client, &addons).await?;

    println!("== resource pack and shader");
    let rp_target = ModrinthResolveTarget {
        loaders: Vec::new(),
        game_versions: vec!["1.21.1".into()],
    };
    for (slug, kind) in [
        ("fast-better-grass", ImportKind::ResourcePack),
        ("complementary-reimagined", ImportKind::ShaderPack),
    ] {
        let version = client
            .best_version(slug, &rp_target)
            .await?
            .ok_or_else(|| format!("no 1.21.1 version of {slug}"))?;
        let report = addons
            .install(
                client,
                downloader,
                kind,
                vec![version],
                &rp_target,
                AddonInstallOptions {
                    with_dependencies: false,
                    ..AddonInstallOptions::default()
                },
            )
            .await?;
        let file = &report.installed[0].file_name;
        check(
            addons.game_dir().join(kind.folder()).join(file).is_file(),
            &format!("{slug} in {}/", kind.folder()),
        )?;
    }

    println!("== remove");
    let scan = addons.identify(client, ImportKind::Mod).await?;
    let modmenu_file = scan
        .entries
        .iter()
        .find(|e| {
            e.source
                .as_ref()
                .is_some_and(|s| s.project_id == "mOgUt4GM")
        })
        .ok_or("Mod Menu missing")?
        .file_name
        .clone();
    addons.remove(ImportKind::Mod, &modmenu_file).await?;
    check(!mods_dir.join(&modmenu_file).exists(), "Mod Menu removed")?;
    let icons = root.join("cache").join("modrinth").join("icons");
    check(
        pl_mc::addons::cached_icon(&icons, "AANobbMI").is_some(),
        "Sodium icon cached",
    )?;

    println!("== export as .mrpack");
    let game_dir = addons.game_dir().to_path_buf();
    let candidates: Vec<String> = std::fs::read_dir(&mods_dir)?
        .filter_map(|e| e.ok())
        .map(|e| format!("mods/{}", e.file_name().to_string_lossy()))
        .collect();
    let hosted = identify_hosted_files(client, &game_dir, &candidates).await?;
    check(
        hosted.unmatched.is_empty() && hosted.files.len() == candidates.len(),
        "every mod is hosted on Modrinth",
    )?;
    std::fs::create_dir_all(game_dir.join("config"))?;
    std::fs::write(game_dir.join("config").join("example.toml"), "a = 1")?;
    let dest = scratch.join("content-live-export.mrpack");
    let summary = write_mrpack(
        &game_dir,
        &MrpackExport {
            name: "Content live export".into(),
            version_id: "1.0.0".into(),
            summary: None,
            mc_version: "1.21.1".into(),
            loader: LoaderKind::Fabric,
            loader_version: Some("0.16.14".into()),
            files: hosted.files,
            overrides: vec!["mods".into(), "config".into()],
        },
        &dest,
        CancellationToken::new(),
    )
    .await?;
    let index = read_mrpack_index(&dest).await?;
    check(
        summary.override_files == 1 && index.files.len() == candidates.len(),
        &format!(
            "export lists {} hosted files and bundles {} file(s)",
            index.files.len(),
            summary.override_files
        ),
    )?;

    println!("== import a small .mrpack from Modrinth");
    let packs = client
        .search(&ModrinthSearchQuery {
            project_type: Some(ModrinthProjectType::Modpack),
            loaders: vec!["fabric".into()],
            game_versions: vec!["1.21.1".into()],
            index: ModrinthSortIndex::Downloads,
            limit: 30,
            ..ModrinthSearchQuery::default()
        })
        .await?;
    // A pack with few, small downloads keeps the check quick.
    let mut chosen = None;
    for hit in &packs.hits {
        let versions = client
            .project_versions(&hit.project_id, &[], &["1.21.1".to_owned()])
            .await?;
        let Some(version) = versions.into_iter().next() else {
            continue;
        };
        let Some(file) = version
            .files
            .iter()
            .find(|f| f.filename.ends_with(".mrpack") && f.size < 2_000_000)
            .cloned()
        else {
            continue;
        };
        let pack_path = scratch.join(&file.filename);
        downloader
            .download(
                pl_net::DownloadItem::new(file.url.clone(), &pack_path)
                    .sha1(file.hashes.sha1.clone().unwrap_or_default())
                    .size(Some(file.size)),
                &DownloadOptions::default(),
            )
            .await?;
        let preview = preview_pack(&pack_path).await?;
        if preview.download_files > 0
            && preview.download_files <= 40
            && preview.download_bytes <= 80_000_000
        {
            chosen = Some((hit.title.clone(), version, pack_path, preview));
            break;
        }
        let _ = std::fs::remove_file(&pack_path);
    }
    let (title, version, pack_path, preview) =
        chosen.ok_or("no small 1.21.1 Fabric modpack found")?;
    println!(
        "  {title} {}: {} {} {:?}, {} downloads ({} bytes), {} bundled",
        version.version_number,
        preview.name,
        preview.mc_version,
        preview.loader,
        preview.download_files,
        preview.download_bytes,
        preview.bundled_files
    );
    check(preview.download_files > 0, "preview lists downloads")?;
    let imported_instance = store
        .create(NewInstance {
            name: preview.name.clone(),
            mc_version: preview.mc_version.clone(),
            loader: InstanceLoader {
                kind: preview.loader,
                version: preview.loader_version.clone(),
            },
            ..NewInstance::default()
        })
        .await?;
    cleanup.push(imported_instance.id.clone());
    let pack_addons = addons_for(store, root, &imported_instance.id)?;
    let imported =
        import_pack(&pack_path, pack_addons.game_dir(), CancellationToken::new()).await?;
    let ImportPendingFiles::Modrinth { files } = &imported.pending else {
        return Err("mrpack import should leave Modrinth files to download".into());
    };
    let items = mrpack_download_items(pack_addons.game_dir(), files, true)?;
    let count = items.len();
    downloader
        .download_all(items, &DownloadOptions::default())
        .await?;
    pack_addons.record_pack_files(files).await?;
    for file in files {
        let path = pack_addons.game_dir().join(&file.path);
        if !path.exists() {
            continue; // client-unsupported files are skipped on purpose
        }
        let sha1 = sha(&path, HashAlgo::Sha1).await?;
        check(sha1 == file.hashes.sha1, &format!("{} verified", file.path))?;
    }
    println!(
        "  {count} files downloaded, {} extracted",
        imported.extracted_files
    );
    let scan = pack_addons.identify(client, ImportKind::Mod).await?;
    check(
        scan.entries.iter().all(|e| e.source.is_some()),
        "every pack mod identified on Modrinth",
    )?;
    let _ = std::fs::remove_file(&pack_path);
    let _ = std::fs::remove_file(&dest);
    println!("all checks passed");
    Ok(())
}

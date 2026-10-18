//! Import from the v1 launcher (`%APPDATA%\.projects`, Windows only): premium session, offline
//! nickname, AFK accounts + client, and game data into a new "Projects" instance. The legacy
//! folder is only read, never modified, so the old launcher keeps working during the transition.

use std::path::{Path, PathBuf};

use pl_mc::instance::{InstanceIcon, InstanceLoader, NewInstance};
use pl_mc::loaders::LoaderKind;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::{CmdError, CmdResult};
use crate::state::AppState;
use pl_lang::pick;

fn legacy_root() -> Option<PathBuf> {
    if !cfg!(windows) {
        return None;
    }
    let root = dirs::data_dir()?.join(".projects");
    root.is_dir().then_some(root)
}

fn dir_stats(dir: &Path) -> (u32, u64) {
    let mut count = 0u32;
    let mut bytes = 0u64;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            count += 1;
            bytes += tree_size(&entry.path());
        }
    }
    (count, bytes)
}

fn tree_size(path: &Path) -> u64 {
    match std::fs::metadata(path) {
        Ok(meta) if meta.is_file() => meta.len(),
        Ok(meta) if meta.is_dir() => std::fs::read_dir(path)
            .map(|entries| entries.flatten().map(|e| tree_size(&e.path())).sum())
            .unwrap_or(0),
        _ => 0,
    }
}

/// The v1 launcher kept the nickname in a .NET `user.config` under LocalAppData.
fn legacy_nickname() -> Option<String> {
    let local = dirs::data_local_dir()?;
    let re = regex::Regex::new(r#"(?s)<setting name="NickNames"[^>]*>\s*<value>([^<]*)</value>"#)
        .ok()?;
    let mut best: Option<(std::time::SystemTime, String)> = None;
    for root in [
        local.join("Apps").join("2.0").join("Data"),
        local.join("Projects"),
    ] {
        for entry in walkdir::WalkDir::new(&root)
            .max_depth(8)
            .into_iter()
            .flatten()
        {
            if entry.file_name() != "user.config" {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(entry.path()) else {
                continue;
            };
            let Some(name) = re
                .captures(&text)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().trim().to_owned())
            else {
                continue;
            };
            if !pl_auth::validate_offline_name(&name).is_ok() {
                continue;
            }
            let modified = entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .unwrap_or(std::time::UNIX_EPOCH);
            if best.as_ref().is_none_or(|(t, _)| modified > *t) {
                best = Some((modified, name));
            }
        }
    }
    best.map(|(_, name)| name)
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LegacyScan {
    pub found: bool,
    pub has_premium: bool,
    pub nickname: Option<String>,
    pub afk_accounts: u32,
    pub has_afk_client: bool,
    pub worlds: u32,
    pub worlds_bytes: u64,
    pub screenshots: u32,
    pub resource_packs: u32,
    pub shader_packs: u32,
    pub free_bytes: Option<u64>,
}

#[tauri::command]
#[specta::specta]
pub async fn legacy_scan(state: State<'_, AppState>) -> CmdResult<LegacyScan> {
    let root_for_space = state.root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(root) = legacy_root() else {
            return LegacyScan {
                found: false,
                has_premium: false,
                nickname: None,
                afk_accounts: 0,
                has_afk_client: false,
                worlds: 0,
                worlds_bytes: 0,
                screenshots: 0,
                resource_packs: 0,
                shader_packs: 0,
                free_bytes: None,
            };
        };
        let afk_accounts = std::fs::read_to_string(root.join("afk").join("accounts.json"))
            .ok()
            .and_then(|json| pl_afk::legacy::parse_accounts(&json).ok())
            .map(|list| list.len() as u32)
            .unwrap_or(0);
        let (worlds, worlds_bytes) = dir_stats(&root.join("saves"));
        LegacyScan {
            found: true,
            has_premium: root.join("premium.dat").is_file(),
            nickname: legacy_nickname(),
            afk_accounts,
            has_afk_client: root
                .join("afk")
                .join("bin")
                .join(pl_afk::CLIENT_EXE_NAME)
                .is_file(),
            worlds,
            worlds_bytes,
            screenshots: dir_stats(&root.join("screenshots")).0,
            resource_packs: dir_stats(&root.join("resourcepacks")).0,
            shader_packs: dir_stats(&root.join("shaderpacks")).0,
            free_bytes: fs4::available_space(&root_for_space).ok(),
        }
    })
    .await
    .map_err(CmdError::internal)
}

#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LegacyImportRequest {
    // Read on Windows only, where the premium session can be decrypted.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub premium: bool,
    pub nickname: bool,
    pub afk: bool,
    pub game_data: bool,
    pub worlds: bool,
}

#[derive(Debug, Clone, Default, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LegacyImportReport {
    pub premium_imported: bool,
    pub nickname: Option<String>,
    pub afk_accounts: u32,
    pub afk_client: bool,
    pub instance_id: Option<String>,
    pub copied_files: u32,
    pub linked_files: u32,
    /// Turkish notes for the summary screen (things skipped and why).
    pub notes: Vec<String>,
}

fn copy_tree(from: &Path, to: &Path, copied: &mut u32) -> std::io::Result<()> {
    if from.is_file() {
        if !to.exists() {
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(from, to)?;
            *copied += 1;
        }
        return Ok(());
    }
    if !from.is_dir() {
        return Ok(());
    }
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)?.flatten() {
        copy_tree(&entry.path(), &to.join(entry.file_name()), copied)?;
    }
    Ok(())
}

/// Hard-links shared game files (same disk: no extra space, no re-download). Files that cannot be
/// linked are skipped; the installer downloads them when needed.
fn link_tree(from: &Path, to: &Path, linked: &mut u32) {
    for entry in walkdir::WalkDir::new(from).into_iter().flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let Ok(rel) = entry.path().strip_prefix(from) else {
            continue;
        };
        let target = to.join(rel);
        if target.exists() {
            continue;
        }
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if std::fs::hard_link(entry.path(), &target).is_ok() {
            *linked += 1;
        }
    }
}

#[tauri::command]
#[specta::specta]
pub async fn legacy_import(
    state: State<'_, AppState>,
    request: LegacyImportRequest,
) -> CmdResult<LegacyImportReport> {
    let root = legacy_root().ok_or_else(|| {
        CmdError::new(
            "ESKI-YOK",
            pick(
                "Eski Projects Launcher verisi bulunamadı.",
                "No data from the old Projects Launcher was found.",
            ),
            "no %APPDATA%\\.projects",
        )
    })?;
    let mut report = LegacyImportReport::default();

    // Premium session (DPAPI-encrypted on Windows). Never in an end-to-end test run: it is a
    // real Microsoft account.
    #[cfg(windows)]
    if request.premium && root.join("premium.dat").is_file() {
        match crate::commands::accounts::refuse_microsoft_in_tests() {
            Err(refused) => report.notes.push(refused.message),
            Ok(()) => match state.auth.import_legacy_premium().await {
                Ok(pl_auth::LegacyImport::Imported { .. }) => report.premium_imported = true,
                Ok(_) => report.notes.push(
                    pick("Premium oturum okunamadı; Ayarlar > Hesaplar'dan tekrar giriş yapabilirsin.", "Couldn't read the premium session; you can sign in again in Settings > Accounts.")
                        .into(),
                ),
                Err(error) => {
                    tracing::warn!(%error, "legacy premium import failed");
                    report.notes.push(
                        pick("Premium oturum aktarılamadı; Microsoft ile yeniden giriş yap.", "Couldn't bring over the premium session; sign in with Microsoft again.").into(),
                    );
                }
            },
        }
    }

    // Offline nickname.
    if request.nickname
        && let Some(name) = legacy_nickname()
    {
        let exists = state
            .auth
            .accounts()
            .iter()
            .any(|a| a.username.eq_ignore_ascii_case(&name));
        if !exists && state.auth.add_offline(&name).await.is_ok() {
            report.nickname = Some(name);
        }
    }

    // AFK accounts (login commands move into the OS keychain) and the installed client.
    if request.afk {
        if let Ok(json) = std::fs::read_to_string(root.join("afk").join("accounts.json"))
            && let Ok(list) = pl_afk::legacy::parse_accounts(&json)
        {
            let store = state.afk.store();
            for legacy in list {
                // Whatever does not come over is said in the summary, never dropped silently.
                let nickname = legacy.account.nickname.clone();
                let mut created = match store.add_account(&nickname) {
                    Ok(created) => created,
                    // Imported before, or added by hand: it is there already.
                    Err(pl_afk::AfkError::DuplicateNickname) => continue,
                    Err(error) => {
                        tracing::warn!(%error, "legacy AFK account not imported");
                        report.notes.push(pick(
                            format!("{nickname} AFK hesabı aktarılamadı."),
                            format!("The AFK account {nickname} couldn't be brought over."),
                        ));
                        continue;
                    }
                };
                let id = created.id.clone();
                created = pl_afk::AfkAccount {
                    id: id.clone(),
                    nickname: created.nickname,
                    ..legacy.account
                };
                if let Err(error) = store.update_account(created) {
                    tracing::warn!(%error, "legacy AFK account settings not imported");
                    report.notes.push(pick(
                        format!(
                            "{nickname} AFK hesabının ayarları aktarılamadı; AFK sayfasından kontrol et."
                        ),
                        format!(
                            "The settings of the AFK account {nickname} couldn't be brought over; check them on the AFK page."
                        ),
                    ));
                }
                if !legacy.login_commands.is_empty()
                    && let Err(error) = store.set_login_commands(&id, &legacy.login_commands)
                {
                    tracing::warn!(%error, "legacy AFK login commands not imported");
                    report.notes.push(pick(
                        format!(
                            "{nickname} AFK hesabının giriş komutları aktarılamadı; AFK ayarlarından yeniden gir."
                        ),
                        format!(
                            "The login commands of the AFK account {nickname} couldn't be brought over; enter them again in its AFK settings."
                        ),
                    ));
                }
                report.afk_accounts += 1;
            }
        }
        let bin = root.join("afk").join("bin");
        let version = std::fs::read_to_string(bin.join("version.txt"))
            .map(|v| v.trim().to_owned())
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "legacy".to_owned());
        let runtime = state.afk.runtime().clone();
        let adopted =
            tauri::async_runtime::spawn_blocking(move || runtime.adopt_folder(&version, &bin))
                .await
                .map_err(CmdError::internal)?;
        report.afk_client = adopted.is_ok();
    }

    // Game data into a "Projects" instance.
    if request.game_data {
        let instance = match state
            .instances
            .list()
            .await
            .unwrap_or_default()
            .into_iter()
            .find(|i| i.name == "Projects")
        {
            Some(existing) => existing,
            None => state
                .instances
                .create(NewInstance {
                    name: "Projects".to_owned(),
                    mc_version: "26.1.2".to_owned(),
                    loader: InstanceLoader {
                        kind: LoaderKind::Fabric,
                        version: None,
                    },
                    icon: Some(InstanceIcon::Builtin {
                        key: "dragon-egg".to_owned(),
                    }),
                    group: None,
                    memory: None,
                    managed_pack: None,
                })
                .await
                .map_err(CmdError::from)?,
        };
        let game_dir = state
            .instances
            .game_dir(&instance.id)
            .map_err(CmdError::from)?;
        let data_root = state.root.clone();
        let copy_worlds = request.worlds;
        let (copied, linked, skipped_worlds) = tauri::async_runtime::spawn_blocking(move || {
            let mut copied = 0u32;
            let mut linked = 0u32;
            for item in [
                "options.txt",
                "optionsof.txt",
                "servers.dat",
                "resourcepacks",
                "shaderpacks",
                "screenshots",
            ] {
                let _ = copy_tree(&root.join(item), &game_dir.join(item), &mut copied);
            }
            let saves_bytes = tree_size(&root.join("saves"));
            let free = fs4::available_space(&game_dir).unwrap_or(0);
            let skipped = copy_worlds && saves_bytes > free.saturating_sub(2 * 1024 * 1024 * 1024);
            if copy_worlds && !skipped {
                let _ = copy_tree(&root.join("saves"), &game_dir.join("saves"), &mut copied);
            }
            // Shared files: link instead of downloading again.
            link_tree(&root.join("assets"), &data_root.join("assets"), &mut linked);
            link_tree(
                &root.join("libraries"),
                &data_root.join("libraries"),
                &mut linked,
            );
            (copied, linked, skipped)
        })
        .await
        .map_err(CmdError::internal)?;
        report.copied_files = copied;
        report.linked_files = linked;
        // The old launcher's server list wins; an empty one gets the Projects server.
        let mut instance = instance;
        crate::commands::instances::seed_default_server_once(&state, &mut instance).await;
        report.instance_id = Some(instance.id);
        if skipped_worlds {
            report.notes.push(
                pick("Dünyalar diskte yeterli yer olmadığı için kopyalanmadı. Eski launcher'daki dünyaların yerinde duruyor.", "The worlds weren't copied because the disk is too full. Your worlds are still in the old launcher.").into(),
            );
        }
    }

    Ok(report)
}

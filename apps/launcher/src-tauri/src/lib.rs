//! Projects Launcher application shell: window, plugins and the typed command bridge.

mod commands;
mod error;
mod host;
mod identity;
mod notify;
mod overlay;
mod prepare;
mod shell;
mod shot_copy;
#[cfg(target_os = "macos")]
mod single_instance_macos;
mod state;
mod webview_rest;

use tauri::Manager;

pub fn run() {
    // Debug builds run under an identity of their own (identifier, keychain service, data
    // folder): they never forward to, read from or write over an installed launcher.
    identity::check();
    init_logging();
    // Windows: windows and notifications under the identity's AppUserModelID, the one the
    // installer's shortcuts carry, before any window exists (notify).
    notify::adopt_app_id();

    let specta = specta_builder();
    #[cfg(debug_assertions)]
    export_bindings(&specta);

    // Until the UI names the language, the backend speaks the player's last pick (kept by
    // `app_set_language`), else the automatic one: Turkish in Turkey or on a Turkish system,
    // English otherwise (pl_lang). AFK accounts that start automatically connect before the page
    // has run, so they need it now.
    let signals = pl_lang::Signals::system();
    let pick = pl_lang::read_pick(&state::default_root().join(pl_lang::PICK_FILE));
    pl_lang::resolve(pick, &signals).set_current();

    let mut builder = tauri::Builder::default();

    #[cfg(desktop)]
    {
        use tauri_plugin_window_state::StateFlags;
        // Single instance must be the first plugin so a second launch only focuses us.
        builder = builder
            .plugin(single_instance())
            .plugin(
                tauri_plugin_window_state::Builder::new()
                    .with_state_flags(
                        StateFlags::SIZE | StateFlags::POSITION | StateFlags::MAXIMIZED,
                    )
                    .with_filename(shell::window_state_file())
                    .build(),
            )
            .plugin(tauri_plugin_updater::Builder::new().build())
            .plugin(shell::autostart_plugin())
            // The close button, Alt+F4 and the taskbar's "Close window" never close the main
            // window itself: the shell quits, goes to the tray, or goes to the tray and quits once
            // the player's jobs are done (shell::close_requested).
            .on_window_event(|window, event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event
                    && window.label() == "main"
                {
                    api.prevent_close();
                    shell::close_requested(window.app_handle());
                }
            });
    }

    builder
        .plugin(language_plugin(&signals))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .invoke_handler(specta.invoke_handler())
        .setup(move |app| {
            // A second copy on macOS hands over to the running launcher now that AppKit has
            // finished launching it, without touching the data folder (single_instance_macos).
            #[cfg(target_os = "macos")]
            if single_instance_macos::is_secondary() {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.destroy();
                }
                single_instance_macos::hand_over(app.handle());
                return Ok(());
            }
            specta.mount_events(app);
            let version = app.package_info().version.to_string();
            let state = state::AppState::init(app.handle(), &version)?;
            // Cached news art and instance files are shown through the asset protocol.
            app.asset_protocol_scope()
                .allow_directory(state.root.join("cache"), true)?;
            app.manage(state);
            // A game the last start left running when it quit ("Kapansın") is followed again, or
            // its session counted, before anything else could touch its files.
            commands::launch::adopt_detached(app.handle());
            // A data folder move finishes once this start succeeded: the old folder goes.
            commands::data::finish_pending_move(app.handle());
            shell::setup_deep_links(app);
            // The page's choice for the close button (Settings → Genel).
            shell::setup_close(app);
            #[cfg(desktop)]
            shell::setup_tray(app)?;
            // First start: a window size that suits the screen; later starts: the player's own
            // size, set exactly (shell::fit_main_window).
            #[cfg(desktop)]
            shell::fit_main_window(app);
            // While the window is out of sight the page rests: hidden for WebView2, little
            // memory, suspended when it has nothing to do (webview_rest).
            webview_rest::install(app);
            // Windows shows the launcher's notifications with its name and picture (notify).
            notify::register(app.handle());
            // After an update restart, the AFK accounts it stopped come back (commands::updates).
            let resumed_afk = commands::updates::take_restart_note(app.handle());
            start_auto_afk(app.handle().clone(), resumed_afk);
            // A version switch a crash interrupted is finished before anything uses the folders.
            commands::versions::recover_unfinished_switches(app.handle().clone());
            // Trash limits and leftovers: shortly after start, then daily (pl_data::housekeeping).
            commands::cleanup::start_housekeeping(app.handle());
            // The launcher's Java runtimes: newer builds staged and swapped in while no game runs.
            commands::java::start_java_updates(app.handle());
            // The chosen instance's files, downloaded gently while the launcher is idle (prepare).
            prepare::start(app.handle());
            // Started at login: stay in the tray until the user opens the launcher (not after an
            // update the player installed from the open window).
            if !commands::updates::start_hidden() {
                reveal_window_fallback(app.handle().clone());
            }
            Ok(())
        })
        .build(identity::context())
        .expect("error while running Projects Launcher")
        .run(|_app, _event| {
            #[cfg(target_os = "macos")]
            single_instance_macos::on_run_event(_app, &_event, shell::focus_main_window);
        });
}

/// A second launch only focuses us; `projects://` links it carried reach the deep-link handler
/// through the plugin's deep-link feature.
#[cfg(all(desktop, not(target_os = "macos")))]
fn single_instance() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_single_instance::init(|app, _args, _cwd| {
        shell::focus_main_window(app);
    })
}

/// On macOS a second copy hands over only after its launch finished (single_instance_macos);
/// links arrive there as Apple events, not as arguments.
#[cfg(target_os = "macos")]
fn single_instance() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    single_instance_macos::init(shell::focus_main_window)
}

/// Hands the page the computer's language signals before its first script runs
/// (`window.__PL_SYSTEM__`, read by `lib/i18n.ts`), so it resolves the language exactly like the
/// backend and its first paint is already in it. The OS display language matters here: WebView2's
/// own language can differ from it.
fn language_plugin(signals: &pl_lang::Signals) -> tauri::plugin::TauriPlugin<tauri::Wry> {
    let json = serde_json::to_string(signals).unwrap_or_else(|_| "{}".to_owned());
    tauri::plugin::Builder::new("pl-language")
        .js_init_script(format!("window.__PL_SYSTEM__ = {json};"))
        .build()
}

fn specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new()
        // Byte counts, timestamps and play time are u64 but stay far below 2^53.
        .dangerously_cast_bigints_to_number()
        // Emitted as plain Tauri events ("afk-event"), so export the payload type explicitly.
        .typ::<pl_afk::AfkEvent>()
        // Emitted as the plain "deep-link" event (tray actions and projects:// links).
        .typ::<shell::DeepLink>()
        // Emitted as the plain "game-log" event (live log of the running game).
        .typ::<commands::launch::GameLogEvent>()
        // Emitted as the plain "adopted-game-exited" event (a game an earlier start left running).
        .typ::<commands::launch::AdoptedGameExit>()
        // Emitted as the plain "java-progress" event (a Java runtime update downloading).
        .typ::<commands::java::JavaProgressEvent>()
        // Emitted as the plain "prepare-status" event (background preparation).
        .typ::<prepare::PrepareStatusView>()
        // Emitted as the plain "social-view" / "social-notice" events (friends feature).
        .typ::<pl_social::SocialView>()
        .typ::<pl_social::SocialNotice>()
        .typ::<pl_social::ChatChangeView>()
        // "social-chat-typing" (someone writes in a conversation, CHAT2).
        .typ::<pl_social::ChatTypingView>()
        // "social-tag-off" (the crew's leader hid the crew whose tag the player wore, TEAM6).
        .typ::<pl_social::TagOffView>()
        // "projects-skin-changed" (SKIN1: a job closed, a link made elsewhere, read again).
        .typ::<pl_social::ProjectsSkinChange>()
        // Emitted as the plain "overlay-status" event (the in-game overlay's mod connected).
        .typ::<pl_overlay::OverlayStatus>()
        // "overlay-people" (a favourite or an online notice changed from the game, OVL-UX2).
        .typ::<pl_overlay::PeopleChange>()
        // Emitted as the plain "cloud-afk-live" event (a watched 7/24 Bulut console).
        .typ::<pl_cloud_afk::CloudLiveEvent>()
        // "host-changed" / "host-notice" (world sharing, HOST1).
        .typ::<pl_worlds::HostView>()
        .typ::<pl_worlds::HostNotice>()
        .commands(tauri_specta::collect_commands![
            commands::app::app_info,
            commands::app::app_set_language,
            commands::app::app_tray_in_game,
            commands::accounts::accounts_list,
            commands::accounts::account_add_offline,
            commands::accounts::account_login_microsoft,
            commands::accounts::account_cancel_login,
            commands::accounts::account_set_active,
            commands::accounts::account_remove,
            commands::accounts::account_undo_remove,
            commands::accounts::account_forget_removed,
            commands::accounts::account_rename,
            commands::accounts::account_set_pinned,
            commands::accounts::accounts_reorder_pinned,
            commands::accounts::account_set_avatar,
            commands::accounts::account_check_session,
            commands::accounts::accounts_ensure_heads,
            commands::accounts::account_import_legacy,
            commands::content::content_home,
            commands::content::content_status,
            commands::content::content_recheck,
            commands::discord::discord_update,
            commands::discord::discord_set_enabled,
            commands::instances::instances_list,
            commands::instances::instance_create,
            commands::instances::instance_delete,
            commands::instances::instance_restore,
            commands::instances::instance_duplicate,
            commands::instances::instance_open_folder,
            commands::instances::instance_detail,
            commands::instances::instance_update_settings,
            commands::instances::instance_graphics,
            commands::instances::instance_set_graphics,
            commands::instances::instance_switch_graphics,
            commands::instances::instance_undo_graphics,
            commands::instances::instance_shader,
            commands::instances::instance_set_shader,
            commands::instances::mod_set_enabled,
            commands::instances::instance_servers_edit,
            commands::instances::minecraft_versions,
            commands::instances::loader_versions,
            commands::launch::launch_instance,
            commands::launch::launch_cancel,
            commands::launch::game_kill,
            commands::launch::game_close,
            commands::launch::game_running,
            commands::launch::game_adopted,
            commands::launch::game_copy_screenshots,
            prepare::prepare_configure,
            prepare::prepare_status,
            // the in-game overlay (Settings → Genel "Oyun içi katman")
            overlay::overlay_configure,
            overlay::overlay_state,
            overlay::overlay_connect_in_game,
            // world sharing (HOST1; shown only while the signed config and the service offer it)
            commands::host::host_state,
            commands::host::host_place,
            commands::host::host_open,
            commands::host::host_cancel_open,
            commands::host::host_invite,
            commands::host::host_remove,
            commands::host::host_stop,
            commands::host::host_dismiss_failure,
            commands::host::host_meet_on_projects,
            commands::host::host_join,
            commands::host::host_answer,
            commands::host::host_cancel_join,
            commands::host::host_dismiss_invite,
            commands::host::host_connect_in_game,
            commands::server::server_status,
            commands::troubleshoot::troubleshoot_run,
            commands::troubleshoot::troubleshoot_crash,
            commands::troubleshoot::troubleshoot_open_clock,
            commands::afk::afk_overview,
            commands::afk::afk_add,
            commands::afk::afk_remove,
            commands::afk::afk_connect,
            commands::afk::afk_disconnect,
            commands::afk::afk_connect_all,
            commands::afk::afk_stop_all,
            commands::afk::afk_send,
            commands::afk::afk_console,
            commands::afk::afk_any_active,
            commands::afk::afk_reconnect,
            commands::afk::afk_settings,
            commands::afk::afk_save_settings,
            commands::afk::afk_check_pattern,
            commands::afk::afk_global_settings,
            commands::afk::afk_set_show_script_commands,
            commands::afk::afk_open_folder,
            // 7/24 Bulut (cloud AFK accounts; shown only while the service answers)
            commands::cloud_afk::cloud_afk_overview,
            commands::cloud_afk::cloud_afk_start,
            commands::cloud_afk::cloud_afk_stop,
            commands::cloud_afk::cloud_afk_restart,
            commands::cloud_afk::cloud_afk_send,
            commands::cloud_afk::cloud_afk_logs,
            commands::cloud_afk::cloud_afk_watch,
            commands::cloud_afk::cloud_afk_unwatch,
            commands::cloud_afk::cloud_afk_edit,
            commands::cloud_afk::cloud_afk_save,
            commands::cloud_afk::cloud_afk_reveal,
            commands::cloud_afk::cloud_afk_move_plan,
            commands::cloud_afk::cloud_afk_move,
            commands::cloud_afk::cloud_afk_connect_here,
            commands::cloud_afk::cloud_afk_open,
            commands::cloud_afk_feat::cloud_afk_inventory_watch,
            commands::cloud_afk_feat::cloud_afk_inventory_unwatch,
            commands::cloud_afk_feat::cloud_afk_inv_op,
            commands::cloud_afk_feat::cloud_afk_set_inventory,
            commands::cloud_afk_feat::cloud_afk_try_reply,
            commands::cloud_afk_feat::cloud_afk_console_download,
            commands::cloud_afk_feat::cloud_afk_show_download,
            commands::cloud_afk_feat::cloud_afk_copy_source,
            commands::cloud_afk_feat::cloud_afk_copy_settings,
            commands::cloud_afk_plus::cloud_afk_plus_panel,
            commands::cloud_afk_plus::cloud_afk_plus_settings,
            commands::cloud_afk_plus::cloud_afk_plus_catalog,
            commands::cloud_afk_plus::cloud_afk_plus_save,
            commands::cloud_afk_plus::cloud_afk_plus_restore,
            commands::cloud_afk_plus::cloud_afk_plus_control,
            commands::cloud_afk_plus::cloud_afk_plus_feed,
            commands::cloud_afk_plus::cloud_afk_plus_deals,
            commands::cloud_afk_plus::cloud_afk_plus_approvals,
            commands::cloud_afk_plus::cloud_afk_plus_answer,
            commands::cloud_afk_plus::cloud_afk_plus_quota,
            commands::cloud_afk_plus::cloud_afk_plus_inbox,
            commands::cloud_afk_plus::cloud_afk_plus_thread,
            commands::cloud_afk_plus::cloud_afk_plus_reply,
            commands::cloud_afk_plus::cloud_afk_plus_read,
            commands::cloud_afk_plus::cloud_afk_plus_hand_back,
            commands::cloud_afk_plus::cloud_afk_plus_parse,
            commands::cloud_afk_plus::cloud_afk_plus_history,
            commands::cloud_afk_plus::cloud_afk_plus_summary,
            commands::cloud_afk_plus::cloud_afk_plus_report,
            commands::cloud_afk_plus::cloud_afk_plus_export,
            commands::cloud_afk_plus::cloud_afk_plus_show_export,
            commands::cloud_afk_plus::cloud_afk_notify_prefs,
            commands::cloud_afk_plus::cloud_afk_set_notify_prefs,
            commands::cloud_afk_plus::cloud_afk_notice_seen,
            commands::cloud_afk_plus::cloud_afk_plus_watch,
            commands::cloud_afk_plus::cloud_afk_plus_unwatch,
            commands::items::items_atlas,
            commands::items::items_overlay,
            commands::migrate::legacy_scan,
            commands::migrate::legacy_import,
            commands::import::import_scan,
            commands::import::import_measure,
            commands::import::import_start,
            commands::updates::update_check,
            commands::updates::update_download,
            commands::updates::update_apply,
            commands::updates::app_restart,
            commands::updates::update_restart_info,
            // official pack & desktop shell
            commands::official::official_status,
            commands::official::official_instance_ensure,
            shell::deep_link_pending,
            shell::app_quit,
            shell::launcher_leave_for_game,
            shell::window_bring_forward,
            notify::desktop_notify,
            // small conveniences: dropped files, "Devam et", desktop shortcuts
            commands::drop::files_classify,
            commands::drop::files_drop,
            commands::resume::resume_target,
            commands::shortcut::instance_shortcut_create,
            // mods & modpacks
            commands::modrinth::modrinth_search,
            commands::modrinth::modrinth_categories,
            commands::modrinth::modrinth_project,
            commands::modrinth::modrinth_versions,
            commands::modrinth::content_scan,
            commands::modrinth::content_install,
            commands::modrinth::content_update,
            commands::modrinth::content_remove,
            commands::modrinth::content_add_files,
            commands::modrinth::content_cancel,
            commands::modpack::modpack_preview,
            commands::modpack::modpack_import,
            commands::modpack::modpack_install,
            commands::modpack::modpack_export,
            commands::modpack_update::modpack_info,
            commands::modpack_update::modpack_update_check,
            commands::modpack_update::modpack_update_plan,
            commands::modpack_update::modpack_update_apply,
            commands::modpack_update::modrinth_project_briefs,
            commands::datapacks::datapacks_list,
            commands::datapacks::datapack_install,
            commands::datapacks::datapack_remove,
            // mod center and per-version sets
            commands::modcenter::mod_center,
            commands::modcenter::mod_center_preview,
            commands::modcenter::mod_center_apply,
            commands::modcenter::mod_center_mark_seen,
            commands::modsets::mod_sets_catalog,
            commands::modsets::mod_sets_instance,
            commands::modsets::mod_set_preview,
            commands::modsets::mod_set_apply,
            commands::modsets::mod_set_remove,
            commands::modsets::mod_set_mark_seen,
            commands::modsets::instance_create_from_set,
            commands::versions::instance_versions,
            commands::versions::instance_switch_preview,
            commands::versions::instance_switch_version,
            commands::versions::instance_restore_trashed_set,
            commands::versions::mods_migration_plan,
            commands::versions::mods_migration_check,
            commands::versions::mods_migrate,
            // mod health: pre-launch check, repairs, set reinstall
            commands::modhealth::mods_check,
            commands::modhealth::mods_repair,
            commands::modhealth::mods_reinstall_preview,
            commands::modhealth::mods_reinstall,
            commands::modhealth::instance_set_max_memory,
            commands::modhealth::instance_set_jvm_args,
            commands::modhealth::mods_fix_keys,
            commands::modhealth::mods_undo_keys,
            // Java: Settings → Java, runtime updates (the pre-launch check is part of the launch)
            commands::java::java_overview,
            commands::java::java_runtime_update,
            commands::java::java_runtime_remove,
            commands::java::java_use_auto,
            commands::java::java_open_folder,
            commands::java::java_check,
            // skins
            commands::skins::skin_appearance,
            commands::skins::skin_texture,
            commands::skins::skin_file_preview,
            commands::skins::skin_upload,
            commands::skins::skin_set_variant,
            commands::skins::skin_set_cape,
            commands::skins::skin_reset,
            commands::skins::skin_history,
            commands::skins::skin_history_prepare,
            commands::skins::skin_history_wear,
            commands::skins::skin_history_set_favorite,
            commands::skins::skin_history_remove,
            commands::skins::skin_history_restore,
            // data folder (Settings → Veri)
            commands::data::data_overview,
            commands::data::data_open_folder,
            commands::data::data_support_bundle,
            commands::data::data_move_plan,
            commands::data::data_move_start,
            commands::data::data_move_cancel,
            // cleanup (Settings → Veri → Temizlik)
            commands::cleanup::cleanup_status,
            commands::cleanup::cleanup_old_sets,
            commands::cleanup::cleanup_remove_sets,
            commands::cleanup::cleanup_restore_sets,
            commands::cleanup::cleanup_shared_files,
            commands::cleanup::cleanup_delete_shared,
            commands::cleanup::cleanup_leftovers,
            commands::cleanup::cleanup_clean_leftovers,
            commands::cleanup::cleanup_trash,
            commands::cleanup::cleanup_empty_trash,
            commands::cleanup::cleanup_delete_trashed_set,
            commands::cleanup::cleanup_summary,
            // instance page: logs, worlds, screenshots; library groups
            commands::library::game_logs,
            commands::library::game_log_read,
            commands::library::game_log_share,
            commands::library::game_log_reveal,
            commands::library::worlds_list,
            commands::library::world_backups,
            commands::library::world_backup,
            commands::library::world_restore,
            commands::library::world_delete,
            commands::library::world_backup_delete,
            commands::library::library_cancel,
            commands::library::trash_list,
            commands::library::trash_restore,
            commands::library::trash_delete,
            commands::library::screenshots_list,
            commands::library::screenshot_thumbnail,
            commands::library::screenshot_copy,
            commands::library::screenshot_open,
            commands::library::screenshot_delete,
            commands::library::instance_reveal_file,
            commands::library::instance_set_group,
            commands::library::instance_disk_usage,
            commands::library::instance_disk_usage_cancel,
            // friends (optional feature, off until the signed config turns it on)
            commands::social::social_state,
            commands::social::social_sign_in,
            commands::social::social_sign_in_cancel,
            commands::social::social_sign_out,
            commands::social::social_refresh,
            commands::social::social_lookup,
            commands::social::social_send_request,
            commands::social::social_request_crewmate,
            commands::social::social_accept,
            commands::social::social_decline,
            commands::social::social_cancel_request,
            commands::social::social_remove_friend,
            commands::social::social_block,
            commands::social::social_unblock,
            commands::social::social_rotate_code,
            commands::social::social_set_status,
            commands::social::social_set_privacy,
            commands::social::social_set_avatar,
            commands::social::social_invite,
            commands::social::social_dismiss_invite,
            commands::social::social_open_in_discord,
            commands::social::social_sessions,
            commands::social::social_revoke_session,
            commands::social::social_revoke_other_sessions,
            commands::social::social_delete_account,
            commands::social::social_backup_upload,
            commands::social::social_backup_download,
            commands::social::social_backup_delete,
            commands::social::social_groups_save,
            commands::social::social_team_create,
            commands::social::social_team_edit,
            commands::social::social_team_moderator,
            commands::social::social_team_notifications,
            commands::social::social_team_mute,
            commands::social::social_team_order,
            commands::social::social_team_call,
            commands::social::social_team_call_answer,
            commands::social::social_team_call_end,
            commands::social::social_team_disband,
            commands::social::social_team_leave,
            commands::social::social_team_rotate_code,
            commands::social::social_team_invite,
            commands::social::social_team_remove,
            commands::social::social_team_transfer,
            commands::social::social_team_lookup,
            commands::social::social_team_join,
            commands::social::social_team_accept,
            commands::social::social_team_decline,
            commands::social::social_team_lookup_link,
            commands::social::social_team_join_link,
            commands::social::social_team_request_withdraw,
            commands::social::social_team_request_answer,
            commands::social::social_team_link,
            commands::social::social_team_join_mode,
            commands::social::social_plan_make,
            commands::social::social_plan_answer,
            commands::social::social_plan_cancel,
            commands::social::social_plan_note,
            commands::social::social_together,
            commands::social::social_together_clear,
            commands::social::social_chat_page,
            commands::social::social_chat_send,
            commands::social::social_chat_retry,
            commands::social::social_chat_discard,
            commands::social::social_chat_edit,
            commands::social::social_chat_delete,
            commands::social::social_chat_read,
            commands::social::social_chat_clear,
            commands::social::social_chat_reply,
            commands::social::social_chat_sticker,
            commands::social::social_chat_react,
            commands::social::social_chat_typing,
            commands::social::social_chat_unfurl,
            commands::social::social_chat_link_card,
            commands::social::social_set_chat_privacy,
            commands::social::social_set_badge,
            commands::social::social_team_hidden,
            commands::social::social_team_badge_requests,
            commands::social::social_request_from_badge,
            commands::social::social_crew_greet,
            commands::social::social_report,
            commands::social::social_notices_seen,
            // SKIN1: Projects skins of username accounts and the game account link
            commands::projects_skin::projects_skin_overview,
            commands::projects_skin::projects_skin_state,
            commands::projects_skin::projects_skin_default,
            commands::projects_skin::projects_skin_pick_file,
            commands::projects_skin::projects_skin_lookup,
            commands::projects_skin::projects_skin_save,
            commands::projects_skin::projects_skin_cancel,
            commands::projects_skin::projects_link,
            commands::projects_skin::projects_unlink,
            commands::projects_skin::projects_link_check,
        ])
}

/// Writes the typed command/event bindings used by the React app. Runs automatically in debug
/// builds; `cargo run -p projects-launcher --features export-bindings --bin export-bindings` does
/// it without a window.
pub fn export_typescript_bindings(path: &std::path::Path) -> Result<(), String> {
    specta_builder()
        .export(specta_typescript::Typescript::default(), path)
        .map_err(|error| error.to_string())
}

pub fn default_bindings_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/bindings.ts")
}

#[cfg(debug_assertions)]
fn export_bindings(_builder: &tauri_specta::Builder<tauri::Wry>) {
    if let Err(error) = export_typescript_bindings(&default_bindings_path()) {
        tracing::error!(%error, "failed to export TypeScript bindings");
    }
}

/// Logs go to daily files in `<data root>/logs` (`launcher.<local date>.log`, the last 7 kept;
/// support bundles include them, see `pl_data::logfile`) and, in debug builds, to the terminal.
fn init_logging() {
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::{EnvFilter, fmt};

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,projects_launcher_lib=debug,pl_=debug"));
    // Written synchronously: the last lines before a crash are the ones support needs.
    let file = pl_data::logfile::DailyLog::new(state::default_root().join("logs"))
        .map_err(|error| eprintln!("launcher log file unavailable: {error}"))
        .ok()
        .map(|log| {
            fmt::layer()
                .with_ansi(false)
                .with_timer(LocalTime)
                .with_writer(std::sync::Mutex::new(log))
        });
    let stdout = cfg!(debug_assertions).then(fmt::layer);
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(file)
        .with(stdout)
        .try_init();

    // Panics reach the log file too (the default hook only prints to stderr).
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!(panic = %info, "the launcher panicked");
        default_hook(info);
    }));
}

/// Local time in log files: what players and support talk in.
struct LocalTime;

impl tracing_subscriber::fmt::time::FormatTime for LocalTime {
    fn format_time(&self, w: &mut tracing_subscriber::fmt::format::Writer<'_>) -> std::fmt::Result {
        write!(
            w,
            "{}",
            jiff::Zoned::now().strftime("%Y-%m-%d %H:%M:%S%.3f")
        )
    }
}

/// AFK accounts marked "start automatically" connect when the launcher starts (staggered), with
/// the client build and game versions of the cached content index (the UI may not have asked for
/// content yet, for example when the launcher starts minimized). `resumed`: the accounts an update
/// restart stopped, which join the same queue.
fn start_auto_afk(app: tauri::AppHandle, resumed: Vec<String>) {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<state::AppState>();
        if let Ok(cached) = state.content.load_cached().await {
            commands::content::apply_snapshot(&state, &cached);
        }
        if let Err(error) = state.afk.auto_start_with(&resumed).await {
            tracing::warn!(%error, "AFK auto-start failed");
        }
    });
}

/// The window starts hidden and the frontend reveals it after first paint.
/// If the frontend fails to load, show it anyway so the user is never left with nothing.
fn reveal_window_fallback(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(4)).await;
        if let Some(window) = app.get_webview_window("main")
            && !window.is_visible().unwrap_or(true)
        {
            let _ = window.show();
        }
    });
}

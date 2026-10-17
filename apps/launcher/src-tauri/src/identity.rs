//! This process's identity (`pl_desktop::identity`): the installed launcher's in release builds,
//! one of its own in debug builds (`PROJECTS_LAUNCHER_INSTANCE`, default `dev`), so a test build
//! runs next to the installed launcher without forwarding to it or touching its data folder,
//! keychain entries, WebView2 profile, window state or login item. Resolved once, first thing in
//! `run`.

use std::path::PathBuf;
use std::sync::OnceLock;

use pl_desktop::identity::{INSTANCE_ENV, Identity, PRODUCTION_IDENTIFIER};

static CURRENT: OnceLock<Identity> = OnceLock::new();

/// The identity of this process.
pub fn current() -> &'static Identity {
    CURRENT.get_or_init(|| {
        Identity::resolve(
            cfg!(debug_assertions),
            std::env::var(INSTANCE_ENV).ok().as_deref(),
        )
        .unwrap_or_else(|error| refuse(&error))
    })
}

/// Settles the identity before anything opens a folder, and stops the process when a debug
/// build would use the installed launcher's data folder (its default folder or the one its
/// "Taşı" pointer names).
pub fn check() {
    let identity = current();
    if !identity.is_isolated() {
        return;
    }
    let mut production: Vec<PathBuf> = pl_mc::paths::DataDirs::default_root().into_iter().collect();
    if let Some(moved) = crate::commands::data::location_file_of(PRODUCTION_IDENTIFIER)
        .and_then(|file| pl_data::relocate::pointed_root(&file))
    {
        production.push(moved);
    }
    if let Err(error) = identity.check_data_root(&crate::state::default_root(), &production) {
        refuse(&error);
    }
}

fn refuse(error: &str) -> ! {
    eprintln!("Projects Launcher: {error}");
    std::process::exit(2);
}

/// The app's Tauri context under this identity: a debug build takes its own identifier (its own
/// single-instance lock, window state and WebView2 profile) and names its instance in the window
/// title.
pub fn context() -> tauri::Context<tauri::Wry> {
    let mut context = tauri::generate_context!();
    let identity = current();
    if identity.is_isolated() {
        context
            .config_mut()
            .identifier
            .clone_from(&identity.identifier);
        for window in &mut context.config_mut().app.windows {
            window.title = identity.label(&window.title);
        }
        tracing::info!(
            identifier = %identity.identifier,
            data = %crate::state::default_root().display(),
            "debug launcher runs isolated from the installed one"
        );
    }
    context
}

/// Debug builds register `projects://` for themselves only when asked
/// (`PROJECTS_LAUNCHER_REGISTER_LINKS=1`, e.g. to test the friends sign-in page): the registration
/// would take the scheme away from the installed launcher. A link reaches a running test build
/// anyway when its exe is started again with the link as argument (single-instance plugin).
#[cfg(debug_assertions)]
pub fn register_links() -> bool {
    std::env::var("PROJECTS_LAUNCHER_REGISTER_LINKS").is_ok_and(|value| value.trim() == "1")
}

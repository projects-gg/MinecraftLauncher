//! The single instance on macOS (Windows and Linux keep tauri-plugin-single-instance).
//!
//! tauri-plugin-single-instance 2.5 ends a second copy with `process::exit(0)` in its plugin
//! setup, before AppKit has finished launching that copy. Finder is still waiting for the launch
//! and reports "“Projects Launcher” can't be opened". Players met it after installing a new copy
//! beside one still running in the menu bar (Finder named the new one "Projects Launcher 2").
//!
//! Here the plugin setup only marks a second copy. Once AppKit has finished launching it (the
//! app's setup hook runs then), the copy hands over without touching the data folder:
//! - normally it brings the running copy forward and leaves (the plugin's socket and message, so
//!   a running 2.0.x launcher answers as well);
//! - when the running copy is an older version installed elsewhere, or does not answer, it asks
//!   whether to close that one and open this one.

use std::{
    ffi::{OsString, c_int, c_void},
    io::{ErrorKind, Read, Write},
    os::unix::{
        ffi::OsStringExt,
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use tauri::{
    AppHandle, RunEvent, Wry,
    plugin::{Builder, TauriPlugin},
};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

/// Set in the plugin setup when another copy of the app is already running.
static SECONDARY: AtomicBool = AtomicBool::new(false);

const SIGKILL: c_int = 9;
const SIGTERM: c_int = 15;
const PROC_PIDPATHINFO_MAXSIZE: usize = 4096;

// libproc and kill(2), both part of libSystem.
unsafe extern "C" {
    fn proc_listallpids(buffer: *mut c_void, buffersize: c_int) -> c_int;
    fn proc_pidpath(pid: c_int, buffer: *mut c_void, buffersize: u32) -> c_int;
    fn kill(pid: c_int, sig: c_int) -> c_int;
}

/// Whether this process is a second copy that is handing over (see the module docs).
pub fn is_secondary() -> bool {
    SECONDARY.load(Ordering::SeqCst)
}

/// The plugin: the first copy listens on the socket, a second one is only marked.
pub fn init(on_second_launch: fn(&AppHandle)) -> TauriPlugin<Wry> {
    Builder::new("projects-instance")
        .setup(move |app, _api| {
            let identifier = app.config().identifier.clone();
            if !other_copies(&identifier).is_empty() {
                SECONDARY.store(true, Ordering::SeqCst);
                return Ok(());
            }
            let socket = socket_path(&identifier);
            match UnixStream::connect(&socket) {
                // Someone answers although no other app bundle runs (a development build): the
                // empty message only brings it forward.
                Ok(_) => SECONDARY.store(true, Ordering::SeqCst),
                Err(error)
                    if matches!(
                        error.kind(),
                        ErrorKind::NotFound | ErrorKind::ConnectionRefused
                    ) =>
                {
                    let _ = std::fs::remove_file(&socket);
                    listen(socket, app.clone(), on_second_launch);
                }
                Err(error) => {
                    tracing::debug!(%error, "single instance: socket unusable, launching normally");
                }
            }
            Ok(())
        })
        .on_event(|app, event| {
            if let RunEvent::Exit = event
                && !is_secondary()
            {
                let _ = std::fs::remove_file(socket_path(&app.config().identifier));
            }
        })
        .build()
}

/// Run-loop events on macOS: a second copy keeps running while its question is open, and the
/// Dock icon (or the app opened again from Finder) brings back a window that went to the menu bar.
pub fn on_run_event(app: &AppHandle, event: &RunEvent, show_main: fn(&AppHandle)) {
    match event {
        RunEvent::ExitRequested { api, .. } if is_secondary() => api.prevent_exit(),
        RunEvent::Reopen {
            has_visible_windows: false,
            ..
        } if !is_secondary() => show_main(app),
        _ => {}
    }
}

/// Called from the app's setup hook in a second copy, after AppKit has finished launching it.
pub fn hand_over(app: &AppHandle) {
    let identifier = app.config().identifier.clone();
    let socket = socket_path(&identifier);
    let own_bundle = std::env::current_exe().ok().and_then(|exe| bundle_of(&exe));
    let own_version = semver::Version::parse(&app.package_info().version.to_string()).ok();
    let running = other_copies(&identifier);

    // An older version installed somewhere else: offer to replace it.
    let stale = running
        .iter()
        .find(|copy| {
            let elsewhere = own_bundle.as_deref() != Some(copy.bundle.as_path());
            let older = match (&own_version, copy.version.as_deref()) {
                (Some(own), Some(theirs)) => {
                    semver::Version::parse(theirs).is_ok_and(|theirs| theirs < *own)
                }
                _ => false,
            };
            elsewhere && older
        })
        .cloned();
    if stale.is_none() && notify(&socket).is_ok() {
        std::process::exit(0);
    }
    match stale.or_else(|| running.first().cloned()) {
        Some(target) => ask_to_replace(app, target, socket, own_bundle),
        // The other copy quit meanwhile: start again as the only one.
        None => {
            relaunch(own_bundle.as_deref());
            std::process::exit(0);
        }
    }
}

#[derive(Clone)]
struct RunningCopy {
    pid: c_int,
    bundle: PathBuf,
    version: Option<String>,
}

fn ask_to_replace(
    app: &AppHandle,
    target: RunningCopy,
    socket: PathBuf,
    own_bundle: Option<PathBuf>,
) {
    // The automatic language (pl_lang): this copy's UI never ran, so no pick reached it.
    let turkish = matches!(pl_lang::Lang::current(), pl_lang::Lang::Tr);
    let texts = Texts::new(turkish, target.version.as_deref());
    app.dialog()
        .message(texts.message)
        .title(texts.title)
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            texts.replace.to_owned(),
            texts.cancel.to_owned(),
        ))
        .show(move |replace| {
            // Waiting for the old copy to close must not block the main thread.
            std::thread::spawn(move || {
                if replace {
                    terminate(target.pid);
                    let _ = std::fs::remove_file(&socket);
                    relaunch(own_bundle.as_deref());
                } else {
                    let _ = notify(&socket);
                }
                std::process::exit(0);
            });
        });
}

struct Texts {
    title: &'static str,
    message: String,
    replace: &'static str,
    cancel: &'static str,
}

impl Texts {
    /// `older`: the running copy's version when it is older; none when it does not answer.
    fn new(turkish: bool, older: Option<&str>) -> Self {
        if turkish {
            Self {
                title: "Projects Launcher zaten açık",
                message: match older {
                    Some(version) => format!(
                        "Projects Launcher'ın eski bir sürümü ({version}) açık. Kapatıp bu sürümü açayım mı?"
                    ),
                    None => {
                        "Açık olan Projects Launcher yanıt vermiyor. Kapatıp yeniden açayım mı?"
                            .to_owned()
                    }
                },
                replace: "Kapat ve aç",
                cancel: "Vazgeç",
            }
        } else {
            Self {
                title: "Projects Launcher is already open",
                message: match older {
                    Some(version) => format!(
                        "An older version ({version}) of Projects Launcher is open. Close it and open this one?"
                    ),
                    None => {
                        "The open Projects Launcher isn't responding. Close it and open it again?"
                            .to_owned()
                    }
                },
                replace: "Close and open",
                cancel: "Cancel",
            }
        }
    }
}

/// The plugin's socket, so copies of either kind find each other.
fn socket_path(identifier: &str) -> PathBuf {
    let identifier = identifier.replace(['.', '-'], "_");
    // Socket paths must stay under about 100 bytes, hence /tmp.
    PathBuf::from(format!("/tmp/{identifier}_si.sock"))
}

/// The plugin's message (working folder, then the arguments): the receiver brings its window
/// forward.
fn notify(socket: &Path) -> std::io::Result<()> {
    let mut stream = UnixStream::connect(socket)?;
    let cwd = std::env::current_dir()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let args = std::env::args_os()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<String>>()
        .join("\0");
    stream.write_all(format!("{cwd}\0\0{args}").as_bytes())?;
    stream.flush()
}

fn listen(socket: PathBuf, app: AppHandle, on_second_launch: fn(&AppHandle)) {
    let spawned = std::thread::Builder::new()
        .name("single-instance".into())
        .spawn(move || {
            let listener = match UnixListener::bind(&socket) {
                Ok(listener) => listener,
                Err(error) => {
                    tracing::error!(%error, "single instance: cannot listen for other copies");
                    return;
                }
            };
            // Blocked in accept while nothing happens: no wake-ups.
            for stream in listener.incoming() {
                match stream {
                    Ok(mut stream) => {
                        // Links reach macOS apps as Apple events, so the message is not needed.
                        let mut message = Vec::new();
                        let _ = stream.read_to_end(&mut message);
                        on_second_launch(&app);
                    }
                    Err(error) => tracing::debug!(%error, "single instance: accept failed"),
                }
            }
        });
    if let Err(error) = spawned {
        tracing::error!(%error, "single instance: cannot start the listener");
    }
}

/// Other running copies of this app (the same bundle identifier), wherever they are installed.
fn other_copies(identifier: &str) -> Vec<RunningCopy> {
    let own_pid = std::process::id() as c_int;
    let own_name = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.file_name().map(|name| name.to_owned()));
    let mut pids: Vec<c_int> = vec![0; 8192];
    let size = (pids.len() * std::mem::size_of::<c_int>()) as c_int;
    // SAFETY: the buffer holds `size` bytes; unused entries stay 0.
    if unsafe { proc_listallpids(pids.as_mut_ptr().cast(), size) } <= 0 {
        return Vec::new();
    }
    pids.into_iter()
        .filter(|&pid| pid > 0 && pid != own_pid)
        .filter_map(|pid| {
            let exe = pid_path(pid)?;
            if own_name.is_some() && exe.file_name() != own_name.as_deref() {
                return None;
            }
            let bundle = bundle_of(&exe)?;
            let plist = std::fs::read_to_string(bundle.join("Contents/Info.plist")).ok()?;
            if plist_string(&plist, "CFBundleIdentifier").as_deref() != Some(identifier) {
                return None;
            }
            Some(RunningCopy {
                pid,
                version: plist_string(&plist, "CFBundleShortVersionString"),
                bundle,
            })
        })
        .collect()
}

fn pid_path(pid: c_int) -> Option<PathBuf> {
    let mut buffer = vec![0u8; PROC_PIDPATHINFO_MAXSIZE];
    // SAFETY: the buffer holds PROC_PIDPATHINFO_MAXSIZE bytes.
    let length = unsafe { proc_pidpath(pid, buffer.as_mut_ptr().cast(), buffer.len() as u32) };
    if length <= 0 {
        return None;
    }
    buffer.truncate(length as usize);
    Some(PathBuf::from(OsString::from_vec(buffer)))
}

/// `…/Name.app` for `…/Name.app/Contents/MacOS/<executable>`.
fn bundle_of(exe: &Path) -> Option<PathBuf> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let bundle = contents.parent()?;
    let inside = macos.file_name()? == "MacOS" && contents.file_name()? == "Contents";
    (inside && bundle.extension()? == "app").then(|| bundle.to_path_buf())
}

/// A `<string>` value of an XML Info.plist (the bundler writes XML).
fn plist_string(plist: &str, key: &str) -> Option<String> {
    let rest = &plist[plist.find(&format!("<key>{key}</key>"))?..];
    let start = rest.find("<string>")? + "<string>".len();
    let end = start + rest[start..].find("</string>")?;
    Some(rest[start..end].trim().to_owned())
}

/// Asks the old copy to quit, then insists.
fn terminate(pid: c_int) {
    // SAFETY: plain signals to a process of this user.
    unsafe { kill(pid, SIGTERM) };
    if !wait_gone(pid, Duration::from_secs(5)) {
        unsafe { kill(pid, SIGKILL) };
        wait_gone(pid, Duration::from_secs(2));
    }
}

fn wait_gone(pid: c_int, limit: Duration) -> bool {
    let start = Instant::now();
    loop {
        // SAFETY: signal 0 only checks that the process exists.
        if unsafe { kill(pid, 0) } != 0 {
            return true;
        }
        if start.elapsed() >= limit {
            return false;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Opens this bundle again once this process has gone (a copy still closing would count as a
/// running one).
fn relaunch(bundle: Option<&Path>) {
    let Some(bundle) = bundle else { return };
    let _ = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg("while kill -0 \"$0\" 2>/dev/null; do sleep 0.1; done; exec /usr/bin/open \"$1\"")
        .arg(std::process::id().to_string())
        .arg(bundle)
        .spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLIST: &str = r#"<dict>
	<key>CFBundleIdentifier</key>
	<string>gg.projects.launcher</string>
	<key>CFBundleShortVersionString</key>
	<string>2.0.1-beta.1</string>
</dict>"#;

    #[test]
    fn reads_plist_strings() {
        assert_eq!(
            plist_string(PLIST, "CFBundleIdentifier").as_deref(),
            Some("gg.projects.launcher")
        );
        assert_eq!(
            plist_string(PLIST, "CFBundleShortVersionString").as_deref(),
            Some("2.0.1-beta.1")
        );
        assert_eq!(plist_string(PLIST, "CFBundleName"), None);
    }

    #[test]
    fn finds_the_bundle_of_an_executable() {
        assert_eq!(
            bundle_of(Path::new(
                "/Applications/Projects Launcher 2.app/Contents/MacOS/projects-launcher"
            )),
            Some(PathBuf::from("/Applications/Projects Launcher 2.app"))
        );
        assert_eq!(
            bundle_of(Path::new("/usr/local/bin/projects-launcher")),
            None
        );
    }

    #[test]
    fn uses_the_plugins_socket() {
        assert_eq!(
            socket_path("gg.projects.launcher"),
            PathBuf::from("/tmp/gg_projects_launcher_si.sock")
        );
    }
}

//! The Discord desktop app, for "Discord'dan mesaj at": a friend's profile opens in the app when
//! it is installed (Message is one click away there), otherwise the caller opens the web profile.
//!
//! The one link handed to the app is `discord://-/users/<id>`. The app registers the `discord:`
//! scheme (on Windows `Software\Classes\discord\shell\open\command` is
//! `"…\Discord.exe" --url -- "%1"`), takes the link's path and shows it as discord.com would: the
//! path `/users/<id>` is the same profile as `https://discord.com/users/<id>`.
//!
//! Whether the app is installed is asked of the system without starting anything visible:
//! - Windows: the command registered for `discord:` links (the player's choice, then the
//!   per-user and machine-wide classes), whose program must exist on disk.
//! - macOS: the application LaunchServices picks for `discord:` links, which must exist on disk.
//! - Linux: `xdg-mime query default x-scheme-handler/discord`, whose desktop entry must be
//!   installed.
//!
//! The answer is kept for a minute, and only asked for when the player presses the button.

#[cfg(any(windows, target_os = "linux", test))]
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

/// How long a detection result is reused: a press soon after installing Discord finds it.
const FRESH_FOR: Duration = Duration::from_secs(60);

/// The last answer and when it was found.
static RECENT: Mutex<Option<(Instant, bool)>> = Mutex::new(None);

/// A Discord user id: a snowflake, a positive 64-bit number written in plain decimal digits
/// (no sign, no leading zero, nothing else).
pub fn is_user_id(id: &str) -> bool {
    (1..=20).contains(&id.len())
        && id.bytes().all(|b| b.is_ascii_digit())
        && !id.starts_with('0')
        && id.parse::<u64>().is_ok()
}

/// The link that shows `user_id`'s profile in the Discord app; `None` when it is not a user id.
pub fn app_profile_link(user_id: &str) -> Option<String> {
    is_user_id(user_id).then(|| format!("discord://-/users/{user_id}"))
}

/// Whether a Discord app is installed to take `discord:` links (asked again after a minute).
pub fn app_installed() -> bool {
    remembered(&RECENT, Instant::now(), platform::handler_installed)
}

/// Opens `user_id`'s profile in the Discord app with `open` (the system opener; true when it
/// took the link). False when the id is not a Discord user id, no app takes `discord:` links or
/// the opener failed: the caller opens the web profile instead.
pub fn open_profile_in_app(user_id: &str, open: impl FnOnce(&str) -> bool) -> bool {
    open_profile_with(user_id, app_installed, open)
}

fn open_profile_with(
    user_id: &str,
    installed: impl FnOnce() -> bool,
    open: impl FnOnce(&str) -> bool,
) -> bool {
    let Some(link) = app_profile_link(user_id) else {
        return false;
    };
    installed() && open(&link)
}

/// The answer in `slot` while it is fresh, otherwise `detect`'s (kept). The lock is held while
/// detecting, so presses at the same moment ask the system once.
fn remembered(
    slot: &Mutex<Option<(Instant, bool)>>,
    now: Instant,
    detect: impl FnOnce() -> bool,
) -> bool {
    let mut slot = slot.lock().unwrap_or_else(PoisonError::into_inner);
    match *slot {
        Some((at, installed)) if now.saturating_duration_since(at) < FRESH_FOR => installed,
        _ => {
            let installed = detect();
            *slot = Some((now, installed));
            installed
        }
    }
}

/// The program a registered Windows command starts, as `CreateProcess` reads it: the quoted path,
/// or else the shortest part up to a space that is a file (`C:\Program Files\x.exe --url`).
/// `None` when that program is not on disk (an app folder an update removed, for example).
#[cfg(any(windows, test))]
fn command_program(command: &str, is_file: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    let command = command.trim_start();
    if let Some(rest) = command.strip_prefix('"') {
        let path = Path::new(&rest[..rest.find('"')?]);
        return (!path.as_os_str().is_empty() && is_file(path)).then(|| path.to_path_buf());
    }
    command
        .char_indices()
        .filter(|(_, c)| *c == ' ')
        .map(|(at, _)| &command[..at])
        .chain(std::iter::once(command.trim_end()))
        .map(Path::new)
        .find(|path| !path.as_os_str().is_empty() && is_file(path))
        .map(Path::to_path_buf)
}

/// The desktop entry id `xdg-mime query default …` printed (`discord.desktop`), if any.
#[cfg(any(target_os = "linux", test))]
fn desktop_entry_id(output: &str) -> Option<&str> {
    let id = output.lines().next()?.trim();
    (id.len() > ".desktop".len() && id.ends_with(".desktop") && !id.contains(['/', '\\']))
        .then_some(id)
}

/// Where a desktop entry id may live under `applications/`: `foo-bar.desktop` is
/// `foo-bar.desktop` or `foo/bar.desktop` (each `-` may stand for a folder). Bounded: an id with
/// many dashes is only looked up as it is.
#[cfg(any(target_os = "linux", test))]
fn desktop_entry_paths(id: &str) -> Vec<PathBuf> {
    let parts: Vec<&str> = id.split('-').collect();
    if parts.len() > 6 {
        return vec![PathBuf::from(id)];
    }
    (0..1u32 << (parts.len() - 1))
        .map(|folders| {
            let mut path = PathBuf::new();
            let mut name = parts[0].to_owned();
            for (at, part) in parts.iter().enumerate().skip(1) {
                if folders & (1 << (at - 1)) != 0 {
                    path.push(&name);
                    name = (*part).to_owned();
                } else {
                    name.push('-');
                    name.push_str(part);
                }
            }
            path.push(name);
            path
        })
        .collect()
}

/// Whether the desktop entry `id` is installed under one of `data_dirs`' `applications/`.
#[cfg(any(target_os = "linux", test))]
fn desktop_entry_installed(id: &str, data_dirs: &[PathBuf]) -> bool {
    let paths = desktop_entry_paths(id);
    data_dirs.iter().any(|dir| {
        let applications = dir.join("applications");
        paths.iter().any(|path| applications.join(path).is_file())
    })
}

/// The XDG data folders, the player's first: `$XDG_DATA_HOME` (else `~/.local/share`), then
/// `$XDG_DATA_DIRS` (else `/usr/local/share:/usr/share`), then where Flatpak and Snap put their
/// apps' entries, which a session without their profile scripts does not list.
#[cfg(any(target_os = "linux", test))]
fn xdg_data_dirs(
    home: Option<&Path>,
    data_home: Option<&str>,
    data_dirs: Option<&str>,
) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    match data_home.filter(|value| !value.is_empty()) {
        Some(value) => dirs.push(PathBuf::from(value)),
        None => dirs.extend(home.map(|home| home.join(".local/share"))),
    }
    let system = data_dirs
        .filter(|value| !value.is_empty())
        .unwrap_or("/usr/local/share:/usr/share");
    dirs.extend(
        system
            .split(':')
            .filter(|dir| !dir.is_empty())
            .map(PathBuf::from),
    );
    dirs.extend(home.map(|home| home.join(".local/share/flatpak/exports/share")));
    dirs.push(PathBuf::from("/var/lib/flatpak/exports/share"));
    dirs.push(PathBuf::from("/var/lib/snapd/desktop"));
    dirs
}

#[cfg(windows)]
mod platform {
    use windows::Win32::UI::Shell::{
        ASSOCF_IS_PROTOCOL, ASSOCF_NOTRUNCATE, ASSOCSTR_COMMAND, AssocQueryStringW,
    };
    use windows::core::{PWSTR, w};

    /// The command Windows runs for `discord:` links: the player's choice for the scheme, then
    /// `Software\Classes\discord\shell\open\command` (per user, then machine-wide).
    fn open_command() -> Option<String> {
        let mut len = 0u32;
        // SAFETY: a sizing call: no output buffer, `len` receives the length with the terminator.
        let sized = unsafe {
            AssocQueryStringW(
                ASSOCF_IS_PROTOCOL,
                ASSOCSTR_COMMAND,
                w!("discord"),
                w!("open"),
                None,
                &mut len,
            )
        };
        if sized.is_err() || len == 0 || len > 32_768 {
            return None;
        }
        let mut buffer = vec![0u16; len as usize];
        // SAFETY: `buffer` holds `len` characters, the size the call is given.
        let read = unsafe {
            AssocQueryStringW(
                ASSOCF_IS_PROTOCOL | ASSOCF_NOTRUNCATE,
                ASSOCSTR_COMMAND,
                w!("discord"),
                w!("open"),
                Some(PWSTR(buffer.as_mut_ptr())),
                &mut len,
            )
        };
        if read.is_err() {
            return None;
        }
        let end = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
        Some(String::from_utf16_lossy(&buffer[..end]))
    }

    pub fn handler_installed() -> bool {
        // The shell's association lookup may use COM: an apartment of its own (`crate::com`).
        crate::com::in_apartment(open_command)
            .flatten()
            .is_some_and(|command| super::command_program(&command, |p| p.is_file()).is_some())
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::ffi::c_void;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    type CFTypeRef = *const c_void;
    type CFIndex = isize;
    const UTF8: u32 = 0x0800_0100;
    const ALL_ROLES: u32 = 0xFFFF_FFFF;

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFURLCreateWithBytes(
            allocator: CFTypeRef,
            bytes: *const u8,
            length: CFIndex,
            encoding: u32,
            base: CFTypeRef,
        ) -> CFTypeRef;
        fn CFURLGetFileSystemRepresentation(
            url: CFTypeRef,
            resolve_against_base: u8,
            buffer: *mut u8,
            max_len: CFIndex,
        ) -> u8;
        fn CFRelease(value: CFTypeRef);
    }

    #[link(name = "CoreServices", kind = "framework")]
    unsafe extern "C" {
        fn LSCopyDefaultApplicationURLForURL(
            url: CFTypeRef,
            roles: u32,
            error: *mut CFTypeRef,
        ) -> CFTypeRef;
    }

    pub fn handler_installed() -> bool {
        let link = b"discord://";
        // SAFETY: Core Foundation calls on objects created here, each released once; the path
        // buffer outlives the call that fills it and is NUL-terminated by it on success.
        unsafe {
            let url = CFURLCreateWithBytes(
                std::ptr::null(),
                link.as_ptr(),
                link.len() as CFIndex,
                UTF8,
                std::ptr::null(),
            );
            if url.is_null() {
                return false;
            }
            let app = LSCopyDefaultApplicationURLForURL(url, ALL_ROLES, std::ptr::null_mut());
            CFRelease(url);
            if app.is_null() {
                return false;
            }
            let mut buffer = [0u8; 4096];
            let found = CFURLGetFileSystemRepresentation(
                app,
                1,
                buffer.as_mut_ptr(),
                buffer.len() as CFIndex,
            ) != 0;
            CFRelease(app);
            if !found {
                return false;
            }
            let end = buffer.iter().position(|b| *b == 0).unwrap_or(buffer.len());
            Path::new(std::ffi::OsStr::from_bytes(&buffer[..end])).exists()
        }
    }
}

/// Asks `program` (`xdg-mime`) which app takes `discord:` links, and whether its desktop entry is
/// installed. False without an answer: no `xdg-mime`, no handler, or an app removed since. Built
/// on every platform in tests, so the Linux path is checked everywhere.
#[cfg(any(target_os = "linux", test))]
fn xdg_handler_installed(program: &str) -> bool {
    use std::process::{Command, Stdio};

    let Ok(output) = Command::new(program)
        .args(["query", "default", "x-scheme-handler/discord"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
    else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let output = String::from_utf8_lossy(&output.stdout);
    let Some(id) = desktop_entry_id(&output) else {
        return false;
    };
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let data_home = std::env::var("XDG_DATA_HOME").ok();
    let data_dirs = std::env::var("XDG_DATA_DIRS").ok();
    let dirs = xdg_data_dirs(home.as_deref(), data_home.as_deref(), data_dirs.as_deref());
    desktop_entry_installed(id, &dirs)
}

#[cfg(target_os = "linux")]
mod platform {
    pub fn handler_installed() -> bool {
        super::xdg_handler_installed("xdg-mime")
    }
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
mod platform {
    pub fn handler_installed() -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use url::Url;

    use super::*;

    #[test]
    fn user_ids_are_plain_snowflakes() {
        assert!(is_user_id("80351110224678912"));
        assert!(is_user_id("1234567890123456789"));
        assert!(is_user_id("18446744073709551615")); // u64::MAX
        assert!(is_user_id("1"));
        for bad in [
            "",
            "0",
            "0123",
            "18446744073709551616", // over u64
            "123456789012345678901",
            "+123",
            "-1",
            " 123",
            "123 ",
            "12a3",
            "1e5",
            "١٢٣", // Arabic-Indic digits
            "123/../../x",
            "123?x=1",
            "123#x",
            "123%2F",
        ] {
            assert!(!is_user_id(bad), "{bad:?} is not a user id");
            assert_eq!(app_profile_link(bad), None, "{bad:?} builds no link");
        }
    }

    #[test]
    fn the_app_link_shows_the_same_profile_as_the_web() {
        let link = app_profile_link("80351110224678912").unwrap();
        assert_eq!(link, "discord://-/users/80351110224678912");
        // Discord reads the link with the WHATWG URL parser and shows its path as discord.com.
        let url = Url::parse(&link).unwrap();
        assert_eq!(url.scheme(), "discord");
        assert_eq!(url.host_str(), Some("-"));
        assert_eq!(url.path(), "/users/80351110224678912");
        assert_eq!(url.query(), None);
        assert_eq!(url.fragment(), None);
        assert_eq!(
            Url::parse("https://discord.com")
                .unwrap()
                .join(url.path())
                .unwrap()
                .as_str(),
            "https://discord.com/users/80351110224678912"
        );
    }

    #[test]
    fn opens_only_a_valid_id_and_only_when_the_app_is_there() {
        let asked = Cell::new(0);
        let opened = std::cell::RefCell::new(Vec::<String>::new());
        let installed = |answer: bool| {
            let asked = &asked;
            move || {
                asked.set(asked.get() + 1);
                answer
            }
        };
        let open = |works: bool| {
            let opened = &opened;
            move |link: &str| {
                opened.borrow_mut().push(link.to_owned());
                works
            }
        };

        // Not a user id: nothing is asked or opened.
        assert!(!open_profile_with("12a", installed(true), open(true)));
        assert_eq!(asked.get(), 0);
        assert!(opened.borrow().is_empty());

        // No app: the web profile is the caller's.
        assert!(!open_profile_with("42", installed(false), open(true)));
        assert_eq!(asked.get(), 1);
        assert!(opened.borrow().is_empty());

        // The app takes the link.
        assert!(open_profile_with("42", installed(true), open(true)));
        assert_eq!(*opened.borrow(), ["discord://-/users/42"]);

        // The opener failed: the caller falls back to the web profile.
        assert!(!open_profile_with("42", installed(true), open(false)));
        assert_eq!(opened.borrow().len(), 2);
    }

    #[test]
    fn the_answer_is_kept_for_a_minute() {
        let slot = Mutex::new(None);
        let start = Instant::now();
        let asked = Cell::new(0);
        let detect = |answer: bool| {
            let asked = &asked;
            move || {
                asked.set(asked.get() + 1);
                answer
            }
        };
        assert!(remembered(&slot, start, detect(true)));
        assert!(remembered(
            &slot,
            start + Duration::from_secs(59),
            detect(false)
        ));
        assert_eq!(asked.get(), 1, "a fresh answer is reused");
        assert!(!remembered(&slot, start + FRESH_FOR, detect(false)));
        assert_eq!(asked.get(), 2, "then asked again");
        assert!(!remembered(
            &slot,
            start + FRESH_FOR + Duration::from_secs(1),
            detect(true)
        ));
        assert_eq!(asked.get(), 2);
    }

    #[test]
    fn windows_commands_name_their_program() {
        let discord = r"C:\Users\oyuncu\AppData\Local\Discord\app-1.0.9264\Discord.exe";
        let registered = format!(r#""{discord}" --url -- "%1""#);
        let only = |wanted: &'static str| move |path: &Path| path == Path::new(wanted);

        // Discord's own registration.
        let found = command_program(&registered, |p: &Path| p == Path::new(discord));
        assert_eq!(found.as_deref(), Some(Path::new(discord)));
        // The registered app folder is gone (removed by an update): no app.
        assert_eq!(command_program(&registered, |_: &Path| false), None);

        // Unquoted, with spaces in the path: the shortest existing program.
        assert_eq!(
            command_program(
                r#"C:\Program Files\Discord\Discord.exe --url -- "%1""#,
                only(r"C:\Program Files\Discord\Discord.exe"),
            )
            .as_deref(),
            Some(Path::new(r"C:\Program Files\Discord\Discord.exe"))
        );
        assert_eq!(
            command_program(r"C:\Discord\Discord.exe", only(r"C:\Discord\Discord.exe")).as_deref(),
            Some(Path::new(r"C:\Discord\Discord.exe"))
        );
        assert_eq!(
            command_program(
                "  C:\\Discord\\Discord.exe  ",
                only(r"C:\Discord\Discord.exe")
            )
            .as_deref(),
            Some(Path::new(r"C:\Discord\Discord.exe"))
        );

        // Broken or empty registrations.
        assert_eq!(
            command_program(r#""C:\Discord\Discord.exe --url"#, |_: &Path| true),
            None
        );
        assert_eq!(
            command_program(r#""" --url -- "%1""#, |_: &Path| true),
            None
        );
        assert_eq!(command_program("", |_: &Path| true), None);
        assert_eq!(command_program("   ", |_: &Path| true), None);
    }

    #[test]
    fn xdg_mime_answers() {
        assert_eq!(
            desktop_entry_id("discord.desktop\n"),
            Some("discord.desktop")
        );
        assert_eq!(
            desktop_entry_id("com.discordapp.Discord.desktop\n"),
            Some("com.discordapp.Discord.desktop")
        );
        assert_eq!(
            desktop_entry_id("discord_discord.desktop"),
            Some("discord_discord.desktop")
        );
        for none in [
            "",
            "\n",
            ".desktop",
            "discord",
            "../discord.desktop",
            "a/b.desktop",
        ] {
            assert_eq!(desktop_entry_id(none), None, "{none:?}");
        }
    }

    #[test]
    fn desktop_entries_may_sit_in_folders() {
        assert_eq!(
            desktop_entry_paths("discord.desktop"),
            [PathBuf::from("discord.desktop")]
        );
        let paths = desktop_entry_paths("discord-canary.desktop");
        assert_eq!(paths.len(), 2);
        assert!(paths.contains(&PathBuf::from("discord-canary.desktop")));
        assert!(paths.contains(&Path::new("discord").join("canary.desktop")));
        assert_eq!(desktop_entry_paths("a-b-c-d-e-f-g.desktop").len(), 1);
    }

    #[test]
    fn desktop_entries_are_found_where_they_are_installed() {
        let root = tempfile::tempdir().unwrap();
        let user = root.path().join("home/.local/share");
        let system = root.path().join("usr/share");
        std::fs::create_dir_all(user.join("applications")).unwrap();
        std::fs::create_dir_all(system.join("applications/discord")).unwrap();
        std::fs::write(user.join("applications/vesktop.desktop"), "").unwrap();
        std::fs::write(system.join("applications/discord/canary.desktop"), "").unwrap();
        let dirs = [user, system];

        assert!(desktop_entry_installed("vesktop.desktop", &dirs));
        assert!(desktop_entry_installed("discord-canary.desktop", &dirs));
        // A default left behind by an uninstalled app.
        assert!(!desktop_entry_installed("discord.desktop", &dirs));
    }

    #[test]
    fn no_xdg_mime_means_no_app() {
        assert!(!xdg_handler_installed("pl-desktop-test-no-such-program"));
    }

    #[test]
    fn xdg_data_dirs_follow_the_spec() {
        let home = Path::new("/home/oyuncu");
        let dirs = xdg_data_dirs(Some(home), None, None);
        assert_eq!(dirs[0], Path::new("/home/oyuncu/.local/share"));
        assert_eq!(dirs[1], Path::new("/usr/local/share"));
        assert_eq!(dirs[2], Path::new("/usr/share"));
        assert!(dirs.contains(&PathBuf::from("/var/lib/flatpak/exports/share")));
        assert!(dirs.contains(&PathBuf::from("/var/lib/snapd/desktop")));

        let dirs = xdg_data_dirs(Some(home), Some("/data"), Some("/opt/share::/usr/share"));
        assert_eq!(
            dirs[..3],
            [
                PathBuf::from("/data"),
                PathBuf::from("/opt/share"),
                PathBuf::from("/usr/share")
            ]
        );
        // Empty values count as unset.
        let dirs = xdg_data_dirs(Some(home), Some(""), Some(""));
        assert_eq!(dirs[0], Path::new("/home/oyuncu/.local/share"));
        assert_eq!(dirs[1], Path::new("/usr/local/share"));
    }
}

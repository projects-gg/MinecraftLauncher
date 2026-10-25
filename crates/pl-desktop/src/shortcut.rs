//! Desktop shortcuts that start one instance through a `projects://play/<instance>` link.
//!
//! - Windows: a shell link (`.lnk`, written with `IShellLinkW`) to the launcher executable with
//!   the link as its argument; the single-instance handler forwards it when the launcher runs.
//! - Linux: a `.desktop` launcher with the same command (desktops may ask to trust it once).
//! - macOS: a `.webloc` file with the link; Finder opens it with the app registered for the
//!   scheme.
//!
//! An earlier shortcut of the same link is replaced. Another file with the same name (the
//! player's own shortcut, another instance's of the same name) is never touched: the new shortcut
//! gets a numbered name (`Fabric (2).lnk`).

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// Longest file name stem (keeps desktop paths short).
const MAX_STEM_CHARS: usize = 80;

/// What a shortcut starts and how it looks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutSpec {
    /// Shown name (the instance name); the file name is derived from it.
    pub name: String,
    /// The launcher executable (Windows, Linux).
    pub target: PathBuf,
    /// The link it opens (`projects://play/<instance>`).
    pub link: String,
    /// Picture: an `.ico` on Windows, a `.png` on Linux. `None` = the executable's own icon.
    pub icon: Option<PathBuf>,
    /// Tooltip (Windows) or comment (Linux).
    pub description: String,
}

const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// A file name for the shortcut of `name` that every desktop accepts: characters Windows
/// refuses become spaces, runs of spaces collapse, trailing dots and spaces go, device names
/// get a suffix and an empty result becomes "Projects".
pub fn shortcut_file_name(name: &str, extension: &str) -> String {
    let mut stem = String::with_capacity(name.len());
    let mut space = false;
    for c in name.chars() {
        let c = if c.is_control()
            || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
        {
            ' '
        } else {
            c
        };
        if c.is_whitespace() {
            space = true;
            continue;
        }
        if space && !stem.is_empty() {
            stem.push(' ');
        }
        space = false;
        stem.push(c);
    }
    let mut stem: String = stem.chars().take(MAX_STEM_CHARS).collect();
    while stem.ends_with(['.', ' ']) {
        stem.pop();
    }
    if stem.is_empty() {
        stem = "Projects".to_owned();
    }
    let device = stem
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    if RESERVED.contains(&device.as_str()) {
        stem.push('_');
    }
    format!("{stem}.{extension}")
}

/// A Desktop Entry string value: one line, backslashes and control characters escaped.
fn entry_value(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

/// One `Exec` argument: quoted, with the characters the spec reserves escaped, `%` doubled
/// (field codes). The result is still a string value, so [`entry_value`] runs on the whole line.
fn exec_argument(arg: &str) -> String {
    let mut out = String::from("\"");
    for c in arg.chars() {
        match c {
            '"' | '`' | '$' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            '%' => out.push_str("%%"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The `.desktop` launcher for Linux desktops.
pub fn desktop_entry(spec: &ShortcutSpec) -> String {
    let exec = format!(
        "{} {}",
        exec_argument(&spec.target.to_string_lossy()),
        exec_argument(&spec.link)
    );
    let mut entry = String::from("[Desktop Entry]\nType=Application\nVersion=1.0\n");
    entry.push_str(&format!("Name={}\n", entry_value(&spec.name)));
    if !spec.description.is_empty() {
        entry.push_str(&format!("Comment={}\n", entry_value(&spec.description)));
    }
    entry.push_str(&format!("Exec={}\n", entry_value(&exec)));
    if let Some(icon) = &spec.icon {
        entry.push_str(&format!("Icon={}\n", entry_value(&icon.to_string_lossy())));
    }
    entry.push_str("Terminal=false\nCategories=Game;\nStartupNotify=true\n");
    entry
}

fn xml_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The `.webloc` file for macOS (a property list with the link).
pub fn webloc(link: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n<dict>\n\t<key>URL</key>\n\t<string>{}</string>\n</dict>\n</plist>\n",
        xml_text(link)
    )
}

/// Create (or replace) the shortcut of `spec` in `dir` (usually the desktop). Returns its path.
/// Only a shortcut of the same link is replaced; see [`shortcut_path`].
pub fn create_shortcut(dir: &Path, spec: &ShortcutSpec) -> Result<PathBuf> {
    if spec.link.trim().is_empty() {
        return Err(Error::Invalid("the shortcut has no link".into()));
    }
    std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    let path = shortcut_path(dir, spec)?;
    platform::create(&path, spec)?;
    Ok(path)
}

/// Where the shortcut of `spec` goes in `dir`: its name, else the first numbered name
/// (`Name (2)`, `Name (3)`, …), that is free or already holds a shortcut of the same link.
fn shortcut_path(dir: &Path, spec: &ShortcutSpec) -> Result<PathBuf> {
    let extension = platform::EXTENSION;
    let name = shortcut_file_name(&spec.name, extension);
    let stem = name
        .strip_suffix(&format!(".{extension}"))
        .unwrap_or(&name)
        .to_owned();
    let usable =
        |path: &Path| std::fs::symlink_metadata(path).is_err() || platform::opens(path, &spec.link);
    std::iter::once(dir.join(&name))
        .chain((2..100).map(|n| dir.join(format!("{stem} ({n}).{extension}"))))
        .find(|path| usable(path))
        .ok_or_else(|| Error::Invalid(format!("no free shortcut name for {name}")))
}

#[cfg(windows)]
mod platform {
    use std::path::Path;

    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, CoCreateInstance, IPersistFile, STGM_READ,
    };
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
    use windows::core::{HSTRING, Interface};

    use super::ShortcutSpec;
    use crate::error::{Error, Result};

    pub const EXTENSION: &str = "lnk";

    fn write_link(path: &Path, spec: &ShortcutSpec) -> windows::core::Result<()> {
        // SAFETY: plain COM calls on interfaces this function created; every string outlives
        // the call that reads it.
        unsafe {
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
            link.SetPath(&HSTRING::from(spec.target.as_os_str()))?;
            link.SetArguments(&HSTRING::from(spec.link.as_str()))?;
            if let Some(dir) = spec.target.parent() {
                link.SetWorkingDirectory(&HSTRING::from(dir.as_os_str()))?;
            }
            if !spec.description.is_empty() {
                link.SetDescription(&HSTRING::from(spec.description.as_str()))?;
            }
            let icon = spec.icon.as_deref().unwrap_or(&spec.target);
            link.SetIconLocation(&HSTRING::from(icon.as_os_str()), 0)?;
            let file: IPersistFile = link.cast()?;
            file.Save(&HSTRING::from(path.as_os_str()), true)
        }
    }

    /// The arguments of the shell link at `path`.
    fn read_arguments(path: &Path) -> windows::core::Result<String> {
        // SAFETY: plain COM calls on interfaces this function created; the buffer outlives the
        // call that fills it.
        unsafe {
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
            let file: IPersistFile = link.cast()?;
            file.Load(&HSTRING::from(path.as_os_str()), STGM_READ)?;
            let mut buffer = [0u16; 2048];
            link.GetArguments(&mut buffer)?;
            let end = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
            Ok(String::from_utf16_lossy(&buffer[..end]))
        }
    }

    /// Runs `job` with COM, in an apartment of its own (`crate::com`).
    fn on_com_thread<T: Send + 'static>(
        job: impl FnOnce() -> windows::core::Result<T> + Send + 'static,
    ) -> Result<T> {
        crate::com::in_apartment(job)
            .ok_or_else(|| Error::Platform("the shortcut thread stopped".into()))?
            .map_err(|e| Error::Platform(e.message()))
    }

    pub fn create(path: &Path, spec: &ShortcutSpec) -> Result<()> {
        let path = path.to_path_buf();
        let spec = spec.clone();
        on_com_thread(move || write_link(&path, &spec))
    }

    /// Whether `path` is a shell link that opens `link` (a shortcut made for it earlier).
    pub fn opens(path: &Path, link: &str) -> bool {
        let path = path.to_path_buf();
        on_com_thread(move || read_arguments(&path))
            .is_ok_and(|arguments| arguments.trim() == link.trim())
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use std::path::Path;

    use super::{ShortcutSpec, desktop_entry, entry_value, exec_argument};
    use crate::error::{Error, Result};

    pub const EXTENSION: &str = "desktop";

    pub fn create(path: &Path, spec: &ShortcutSpec) -> Result<()> {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(path, desktop_entry(spec)).map_err(|e| Error::io(path, e))?;
        // Desktops only start launchers marked executable.
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| Error::io(path, e))
    }

    /// Whether `path` is a launcher whose command opens `link`.
    pub fn opens(path: &Path, link: &str) -> bool {
        std::fs::read_to_string(path)
            .is_ok_and(|text| text.contains(&entry_value(&exec_argument(link))))
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::path::Path;

    use super::{ShortcutSpec, webloc, xml_text};
    use crate::error::{Error, Result};

    pub const EXTENSION: &str = "webloc";

    pub fn create(path: &Path, spec: &ShortcutSpec) -> Result<()> {
        std::fs::write(path, webloc(&spec.link)).map_err(|e| Error::io(path, e))
    }

    /// Whether `path` is a `.webloc` of `link`.
    pub fn opens(path: &Path, link: &str) -> bool {
        std::fs::read_to_string(path)
            .is_ok_and(|text| text.contains(&format!("<string>{}</string>", xml_text(link))))
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
mod platform {
    use std::path::Path;

    use super::ShortcutSpec;
    use crate::error::{Error, Result};

    pub const EXTENSION: &str = "lnk";

    pub fn create(_path: &Path, _spec: &ShortcutSpec) -> Result<()> {
        Err(Error::Platform(
            "desktop shortcuts are not supported here".into(),
        ))
    }

    pub fn opens(_path: &Path, _link: &str) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(target: &str) -> ShortcutSpec {
        ShortcutSpec {
            name: "Fabric 1.21".into(),
            target: PathBuf::from(target),
            link: "projects://play/fabric-1-21".into(),
            icon: None,
            description: "Fabric 1.21 kurulumunu başlatır.".into(),
        }
    }

    #[test]
    fn file_names() {
        assert_eq!(shortcut_file_name("Projects", "lnk"), "Projects.lnk");
        assert_eq!(
            shortcut_file_name("Fabric: 1.21 / test?", "lnk"),
            "Fabric 1.21 test.lnk"
        );
        assert_eq!(
            shortcut_file_name("  Yeni   Dünya  ", "lnk"),
            "Yeni Dünya.lnk"
        );
        assert_eq!(shortcut_file_name("Sonu nokta...", "lnk"), "Sonu nokta.lnk");
        assert_eq!(shortcut_file_name("con", "lnk"), "con_.lnk");
        assert_eq!(shortcut_file_name("LPT1.backup", "lnk"), "LPT1.backup_.lnk");
        assert_eq!(shortcut_file_name("\u{7}*?", "desktop"), "Projects.desktop");
        let long = shortcut_file_name(&"ş".repeat(300), "lnk");
        assert_eq!(long.chars().count(), MAX_STEM_CHARS + 4);
    }

    #[test]
    fn linux_entry() {
        let mut spec = spec("/opt/Projects Launcher/projects-launcher");
        spec.icon = Some(PathBuf::from("/home/oyuncu/.local/share/icons/fabric.png"));
        let entry = desktop_entry(&spec);
        assert!(entry.starts_with("[Desktop Entry]\nType=Application\n"));
        assert!(entry.contains("Name=Fabric 1.21\n"));
        assert!(entry.contains("Comment=Fabric 1.21 kurulumunu başlatır.\n"));
        assert!(entry.contains(
            "Exec=\"/opt/Projects Launcher/projects-launcher\" \"projects://play/fabric-1-21\"\n"
        ));
        assert!(entry.contains("Icon=/home/oyuncu/.local/share/icons/fabric.png\n"));
        // Reserved characters in the command survive both escaping layers.
        assert_eq!(exec_argument("a\"b$c%d"), "\"a\\\"b\\$c%%d\"");
        assert_eq!(entry_value("a\\b\nc"), "a\\\\b\\nc");
    }

    #[test]
    fn mac_webloc() {
        let text = webloc("projects://play/x&y");
        assert!(text.contains("<key>URL</key>"));
        assert!(text.contains("<string>projects://play/x&amp;y</string>"));
        assert!(text.starts_with("<?xml"));
    }

    /// A file with the shortcut's name that is not a shortcut of this link (the player's own
    /// shortcut, another instance's of the same name) is never replaced.
    #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
    #[test]
    fn someone_elses_shortcut_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("projects-launcher.exe");
        std::fs::write(&target, b"MZ").unwrap();
        let spec = spec(&target.to_string_lossy());
        let theirs = dir
            .path()
            .join(shortcut_file_name(&spec.name, platform::EXTENSION));
        std::fs::write(&theirs, b"the player's own shortcut").unwrap();

        let path = create_shortcut(dir.path(), &spec).unwrap();
        assert_ne!(path, theirs);
        assert_eq!(
            std::fs::read(&theirs).unwrap(),
            b"the player's own shortcut"
        );
        // The numbered one is this link's: made again, it is replaced in place.
        assert_eq!(create_shortcut(dir.path(), &spec).unwrap(), path);
        // Another instance with the same name gets a name of its own.
        let mut other = spec.clone();
        other.link = "projects://play/fabric-1-21-2".into();
        let third = create_shortcut(dir.path(), &other).unwrap();
        assert!(third != path && third != theirs, "{third:?}");
    }

    #[test]
    fn refuses_an_empty_link() {
        let dir = tempfile::tempdir().unwrap();
        let mut spec = spec("/x");
        spec.link = " ".into();
        assert!(create_shortcut(dir.path(), &spec).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn windows_shell_link_round_trip() {
        use windows::Win32::System::Com::{
            CLSCTX_INPROC_SERVER, CoCreateInstance, IPersistFile, STGM_READ,
        };
        use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
        use windows::core::{HSTRING, Interface};

        let dir = tempfile::tempdir().unwrap();
        let target = dir
            .path()
            .join("Projects Launcher")
            .join("projects-launcher.exe");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, b"MZ").unwrap();
        let mut spec = spec(&target.to_string_lossy());
        spec.name = "Yeni Dünya: Ada".into();
        let path = create_shortcut(dir.path(), &spec).unwrap();
        assert_eq!(path, dir.path().join("Yeni Dünya Ada.lnk"));
        // Creating it again replaces it.
        assert_eq!(create_shortcut(dir.path(), &spec).unwrap(), path);

        fn read(buffer: &[u16]) -> String {
            let end = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
            String::from_utf16_lossy(&buffer[..end])
        }
        // In an apartment like the launcher's own lookups: one at a time in this process.
        let (arguments, working_dir) = crate::com::in_apartment(move || {
            // SAFETY: test-only COM calls inside the apartment `in_apartment` set up.
            unsafe {
                let link: IShellLinkW =
                    CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).unwrap();
                let file: IPersistFile = link.cast().unwrap();
                file.Load(&HSTRING::from(path.as_os_str()), STGM_READ)
                    .unwrap();
                let mut arguments = [0u16; 1024];
                link.GetArguments(&mut arguments).unwrap();
                let mut working_dir = [0u16; 1024];
                link.GetWorkingDirectory(&mut working_dir).unwrap();
                (read(&arguments), read(&working_dir))
            }
        })
        .expect("the shell link was read back");
        assert_eq!(arguments, "projects://play/fabric-1-21");
        assert_eq!(
            PathBuf::from(working_dir),
            target.parent().unwrap().to_path_buf()
        );
    }
}

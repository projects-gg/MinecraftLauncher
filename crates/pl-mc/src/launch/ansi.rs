//! Windows starts Java with a command line its launcher reads in the system's ANSI code page
//! (`GetACP`): a path with a letter that code page lacks (`ş` on a Windows whose language for
//! non-Unicode programs is English, as in an account named "Ayşe") reaches Java mangled, and the
//! game cannot start: its Java is not found ("could not find java.dll"), its class path or game
//! folder is broken. Such paths go in their 8.3 short form (`C:\Users\AYE~1`, plain ASCII); a path
//! with no short form (8.3 names off on its disk) stops the launch with a clear error instead
//! (CRASH-DIAG).

use std::borrow::Cow;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// `program` and `args` with every absolute path the code page cannot carry in its short form:
/// a whole argument, the value of `-Dkey=`, `--key=` or `-javaagent:`, an `@` argument file,
/// each entry of a `;` list. Other text (a world's name) stays as it is.
/// [`Error::UnreadablePath`] when such a path has no short form.
pub fn ansi_safe(program: &Path, args: &[String]) -> Result<(PathBuf, Vec<String>)> {
    let program = match program.to_str() {
        Some(text) if foreign(text) => PathBuf::from(argument(text)?.into_owned()),
        _ => program.to_path_buf(),
    };
    let args = args
        .iter()
        .map(|arg| argument(arg).map(Cow::into_owned))
        .collect::<Result<Vec<_>>>()?;
    Ok((program, args))
}

/// Whether the system's ANSI code page lacks a character of `text` (never elsewhere than on
/// Windows, nor with UTF-8 as that code page).
pub fn foreign(text: &str) -> bool {
    !text.is_ascii() && imp::foreign(text)
}

fn argument(arg: &str) -> Result<Cow<'_, str>> {
    if !foreign(arg) {
        return Ok(Cow::Borrowed(arg));
    }
    let (prefix, value) = split_option(arg);
    let parts = value
        .split(';')
        .map(|part| {
            if foreign(part) && is_absolute(part) {
                short(part)
            } else {
                Ok(part.to_owned())
            }
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Cow::Owned(format!("{prefix}{}", parts.join(";"))))
}

/// An option's name and its value: `-Djava.library.path=` and the path, `-javaagent:` and the
/// agent, `@` and an argument file; else no name.
fn split_option(arg: &str) -> (&str, &str) {
    for prefix in ["-javaagent:", "-agentpath:", "@"] {
        if let Some(value) = arg.strip_prefix(prefix) {
            return (prefix, value);
        }
    }
    match arg.find('=') {
        Some(at) if arg.starts_with('-') => arg.split_at(at + 1),
        _ => ("", arg),
    }
}

/// `C:\...`, `C:/...` or `\\server\share\...`.
fn is_absolute(text: &str) -> bool {
    let bytes = text.as_bytes();
    (bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/'))
        || text.starts_with("\\\\")
}

/// `path` with its longest existing part in its short form and the rest (a file still to be
/// written) after it.
fn short(path: &str) -> Result<String> {
    let unreadable = || Error::UnreadablePath(PathBuf::from(path));
    let full = Path::new(path);
    let mut existing = full;
    let mut rest: Vec<&OsStr> = Vec::new();
    while !existing.exists() {
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name);
                existing = parent;
            }
            _ => return Err(unreadable()),
        }
    }
    let mut out = imp::short_name(existing).ok_or_else(unreadable)?;
    for name in rest.iter().rev() {
        out.push(name);
    }
    match out.to_str() {
        Some(text) if !foreign(text) => Ok(text.to_owned()),
        _ => Err(unreadable()),
    }
}

#[cfg(windows)]
mod imp {
    use std::ffi::OsString;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::path::{Path, PathBuf};

    use windows_sys::Win32::Globalization::{
        CP_ACP, CP_UTF8, GetACP, WC_NO_BEST_FIT_CHARS, WideCharToMultiByte,
    };
    use windows_sys::Win32::Storage::FileSystem::GetShortPathNameW;

    pub fn foreign(text: &str) -> bool {
        // SAFETY: no arguments, no state.
        if unsafe { GetACP() } == CP_UTF8 {
            return false;
        }
        let wide: Vec<u16> = text.encode_utf16().collect();
        let Ok(len) = i32::try_from(wide.len()) else {
            return true;
        };
        let mut used_default = 0;
        // SAFETY: `wide` holds `len` UTF-16 units; with no output buffer the call only measures,
        // and `used_default` outlives it.
        let written = unsafe {
            WideCharToMultiByte(
                CP_ACP,
                WC_NO_BEST_FIT_CHARS,
                wide.as_ptr(),
                len,
                std::ptr::null_mut(),
                0,
                std::ptr::null(),
                &mut used_default,
            )
        };
        written == 0 || used_default != 0
    }

    pub fn short_name(path: &Path) -> Option<PathBuf> {
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
        // SAFETY: `wide` is NUL-terminated; with no buffer the call returns the size it needs.
        let needed = unsafe { GetShortPathNameW(wide.as_ptr(), std::ptr::null_mut(), 0) };
        if needed == 0 {
            return None;
        }
        let mut buffer = vec![0u16; usize::try_from(needed).ok()?];
        // SAFETY: `buffer` holds `needed` units, the size the call asked for.
        let written = unsafe { GetShortPathNameW(wide.as_ptr(), buffer.as_mut_ptr(), needed) };
        if written == 0 || written >= needed {
            return None;
        }
        buffer.truncate(usize::try_from(written).ok()?);
        Some(PathBuf::from(OsString::from_wide(&buffer)))
    }
}

#[cfg(not(windows))]
mod imp {
    use std::path::{Path, PathBuf};

    pub fn foreign(_text: &str) -> bool {
        false
    }

    pub fn short_name(_path: &Path) -> Option<PathBuf> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_arguments_stay_as_they_are() {
        let args = vec![
            "-Xmx4096M".to_owned(),
            "-Djava.library.path=C:\\Games\\natives".to_owned(),
            "--username".to_owned(),
            "KorAtesi".to_owned(),
        ];
        let (program, out) = ansi_safe(Path::new("C:\\Games\\java.exe"), &args).unwrap();
        assert_eq!(program, Path::new("C:\\Games\\java.exe"));
        assert_eq!(out, args);
        assert_eq!(
            split_option("-Djava.library.path=C:\\x"),
            ("-Djava.library.path=", "C:\\x")
        );
        assert_eq!(
            split_option("-javaagent:C:\\a.jar=x"),
            ("-javaagent:", "C:\\a.jar=x")
        );
        assert_eq!(split_option("@C:\\a.args"), ("@", "C:\\a.args"));
        assert_eq!(split_option("--gameDir"), ("", "--gameDir"));
        assert!(is_absolute("C:\\x") && is_absolute("d:/x") && is_absolute("\\\\nas\\x"));
        assert!(!is_absolute("x\\y") && !is_absolute("Dünya"));
    }

    /// Only where the code page lacks the letter (an English Windows here); elsewhere the paths
    /// stay as they are, which Java reads.
    #[test]
    fn paths_with_letters_the_code_page_lacks_go_short() {
        let tmp = tempfile::tempdir().unwrap();
        let folder = tmp.path().join("Ayşe Ğ");
        std::fs::create_dir_all(folder.join("libraries")).unwrap();
        let jar = folder.join("libraries").join("a.jar");
        std::fs::write(&jar, b"").unwrap();
        let java = folder.join("java.exe");
        std::fs::write(&java, b"").unwrap();
        let text = |path: &Path| path.to_str().unwrap().to_owned();
        let args = vec![
            format!("-Djava.library.path={}", text(&folder.join("natives"))),
            "-cp".to_owned(),
            format!("{};{}", text(&jar), "C:\\plain\\b.jar"),
            "--gameDir".to_owned(),
            text(&folder),
            "--quickPlaySingleplayer".to_owned(),
            "Şehir".to_owned(),
        ];
        let (program, out) = ansi_safe(&java, &args).unwrap();
        if !foreign("ş") {
            assert_eq!(out, args);
            return;
        }
        let shortened = |path: &Path| out.iter().any(|arg| arg.contains(&text(path)));
        // With 8.3 names off on this disk the short form is the long one: nothing to check.
        let Some(short) = imp::short_name(&folder).filter(|short| short != &folder) else {
            return;
        };
        assert!(!shortened(&folder));
        assert!(program.to_str().unwrap().is_ascii());
        assert!(out[0].is_ascii() && out[0].starts_with("-Djava.library.path="));
        assert!(out[0].ends_with("\\natives"), "{}", out[0]);
        assert!(
            out[2].is_ascii() && out[2].ends_with(";C:\\plain\\b.jar"),
            "{}",
            out[2]
        );
        assert_eq!(out[4], text(&short));
        // A world's name is no path: it stays.
        assert_eq!(out[6], "Şehir");
        // Each short path is the same file.
        let first = out[2].split(';').next().unwrap();
        assert!(Path::new(first).is_file());
    }
}

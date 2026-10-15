//! Strict validation of relative paths that come from untrusted archives and manifests
//! (`.mrpack` indexes, zip entries, imported file names).
//!
//! A valid path uses `/` separators and every segment must be a portable file name: no `.`/`..`,
//! no empty segments, no drive letters or other `:` uses, no backslashes, no characters Windows
//! forbids, no trailing dot/space, and no Windows device names (`CON`, `nul.txt`, `COM1.jar`, …).
//! Rejecting these everywhere means a pack that installs on Linux also installs on Windows, and no
//! entry can escape the target directory.

use std::path::PathBuf;

use crate::error::{Error, Result};

/// Device names Windows reserves for any extension (`aux.json` is as bad as `AUX`).
const RESERVED_NAMES: [&str; 6] = ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"];
const RESERVED_NUMBERED: [&str; 2] = ["COM", "LPT"];

/// Why a path segment was rejected.
fn segment_problem(segment: &str) -> Option<&'static str> {
    if segment.is_empty() {
        return Some("empty path segment");
    }
    if segment == "." || segment == ".." {
        return Some("relative path segment");
    }
    if segment.chars().any(|c| {
        c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*' | '\\' | '/')
    }) {
        return Some("character not allowed in file names");
    }
    if segment.ends_with('.') || segment.ends_with(' ') {
        return Some("file name ends with a dot or space");
    }
    // Windows matches device names on the part before the first dot, ignoring trailing spaces.
    let stem = segment.split('.').next().unwrap_or(segment).trim_end();
    if is_reserved_device(stem) {
        return Some("reserved device name");
    }
    None
}

fn is_reserved_device(stem: &str) -> bool {
    if RESERVED_NAMES.iter().any(|r| r.eq_ignore_ascii_case(stem)) {
        return true;
    }
    let mut chars = stem.chars();
    let prefix: String = chars.by_ref().take(3).collect();
    if !RESERVED_NUMBERED
        .iter()
        .any(|r| r.eq_ignore_ascii_case(&prefix))
    {
        return false;
    }
    let rest: Vec<char> = chars.collect();
    matches!(rest.as_slice(), [c] if c.is_ascii_digit() || matches!(c, '¹' | '²' | '³'))
}

fn unsafe_path(path: &str, reason: &str) -> Error {
    Error::Invalid(format!("unsafe path {path:?}: {reason}"))
}

/// Check a single file name (no separators allowed).
pub fn validate_file_name(name: &str) -> Result<()> {
    match segment_problem(name) {
        Some(reason) => Err(unsafe_path(name, reason)),
        None => Ok(()),
    }
}

/// Validate a `/`-separated relative path and convert it to a platform path.
pub fn validate_relative_path(path: &str) -> Result<PathBuf> {
    if path.is_empty() {
        return Err(unsafe_path(path, "empty path"));
    }
    if path.starts_with('/') {
        return Err(unsafe_path(path, "absolute path"));
    }
    if path.contains('\\') {
        return Err(unsafe_path(path, "backslash in path"));
    }
    let mut out = PathBuf::new();
    for segment in path.split('/') {
        if let Some(reason) = segment_problem(segment) {
            return Err(unsafe_path(path, reason));
        }
        out.push(segment);
    }
    Ok(out)
}

/// True if [`validate_relative_path`] accepts `path`.
pub fn is_safe_relative_path(path: &str) -> bool {
    validate_relative_path(path).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_normal_paths() {
        for ok in [
            "mods/sodium-fabric-0.5.8.jar",
            "config/iris.properties",
            "resourcepacks/My Pack v2.zip",
            "a/b/c/d.txt",
            "COM10.txt",
            "my con.txt",
            "con_backup.json",
            "Türkçe ğüşıöç.txt",
            ".hidden/file",
        ] {
            assert!(is_safe_relative_path(ok), "{ok:?} should be accepted");
        }
        assert_eq!(
            validate_relative_path("mods/a.jar").unwrap(),
            PathBuf::from("mods").join("a.jar")
        );
    }

    #[test]
    fn rejects_traversal_absolute_and_drive_paths() {
        for bad in [
            "",
            "/etc/passwd",
            "../outside.jar",
            "mods/../../outside.jar",
            "mods/./a.jar",
            "a//b",
            "mods/",
            "C:/Windows/system32/x.dll",
            "C:relative.txt",
            "mods\\..\\..\\x.jar",
            "\\\\server\\share\\x",
            "mods/a:stream",
            "mods/a\0b",
            "mods/what?.jar",
        ] {
            assert!(!is_safe_relative_path(bad), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn rejects_windows_reserved_names() {
        for bad in [
            "CON",
            "mods/con.jar",
            "config/NUL.cfg",
            "aux.json",
            "prn",
            "COM1.txt",
            "lpt9",
            "com¹.txt",
            "CONIN$",
            "NUL .txt",
            "mods/x.jar.",
            "mods/x.jar ",
        ] {
            assert!(!is_safe_relative_path(bad), "{bad:?} should be rejected");
        }
        assert!(validate_file_name("pack.zip").is_ok());
        assert!(validate_file_name("a/b.zip").is_err());
        assert!(validate_file_name("..").is_err());
    }
}

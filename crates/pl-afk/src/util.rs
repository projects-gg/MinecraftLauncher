//! Small text and file helpers shared by the crate.
//!
//! The legacy launcher compared strings with .NET's `StringComparison.OrdinalIgnoreCase`, which
//! folds every UTF-16 unit through `ToUpperInvariant`. [`fold`] reproduces that per character
//! (multi-character uppercase expansions such as `ß` stay unchanged, exactly like .NET).

use std::io::Write;
use std::path::{Path, PathBuf};

/// `.NET char.ToUpperInvariant` equivalent for a single character.
pub(crate) fn fold(c: char) -> char {
    let mut upper = c.to_uppercase();
    match (upper.next(), upper.next()) {
        (Some(u), None) => u,
        _ => c,
    }
}

/// `string.Equals(a, b, StringComparison.OrdinalIgnoreCase)`.
pub(crate) fn eq_ignore_case(a: &str, b: &str) -> bool {
    a.chars().map(fold).eq(b.chars().map(fold))
}

/// Byte range of the first case-insensitive occurrence of `needle` in `haystack` at or after `from`.
pub(crate) fn find_ignore_case(
    haystack: &str,
    needle: &str,
    from: usize,
) -> Option<(usize, usize)> {
    if needle.is_empty() {
        return None;
    }
    let needle: Vec<char> = needle.chars().map(fold).collect();
    let mut starts = haystack
        .char_indices()
        .map(|(i, _)| i)
        .filter(|&i| i >= from);
    starts.find_map(|start| {
        let mut chars = haystack[start..].char_indices();
        for &wanted in &needle {
            match chars.next() {
                Some((_, c)) if fold(c) == wanted => {}
                _ => return None,
            }
        }
        let end = chars.next().map_or(haystack.len(), |(i, _)| start + i);
        Some((start, end))
    })
}

/// `haystack.IndexOf(needle, StringComparison.OrdinalIgnoreCase) >= 0`.
pub(crate) fn contains_ignore_case(haystack: &str, needle: &str) -> bool {
    find_ignore_case(haystack, needle, 0).is_some()
}

/// MCC's `ChatBot.GetVerbatim`: drops Minecraft `§x` formatting codes.
pub(crate) fn strip_color_codes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '§' {
            chars.next();
        } else {
            out.push(c);
        }
    }
    out
}

/// Collapses line breaks and trims, then caps the length (in characters) with an ellipsis.
pub(crate) fn one_line(text: &str, max_chars: usize) -> String {
    let flat: String = text
        .chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect();
    let flat = flat.trim();
    if flat.chars().count() <= max_chars {
        return flat.to_owned();
    }
    let mut cut: String = flat.chars().take(max_chars.saturating_sub(1)).collect();
    cut.push('…');
    cut
}

pub(crate) fn tmp_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(".tmp");
    path.with_file_name(name)
}

/// Writes `bytes` to `path.tmp` and renames it over `path`, so a crash never leaves a half-written
/// file. `private` restricts the file to its owner on Unix (used for files holding secrets).
pub(crate) fn write_atomic(path: &Path, bytes: &[u8], private: bool) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = tmp_path(path);
    let result = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        if private {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        #[cfg(not(unix))]
        let _ = private;
        let mut file = options.open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Removes a file, treating "already gone" as success.
pub(crate) fn remove_file_if_exists(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinal_ignore_case_matches_dotnet_folding() {
        assert!(eq_ignore_case("AUTO", "auto"));
        // ToUpperInvariant('ı') == 'I' == ToUpperInvariant('i'), so .NET treats them as equal.
        assert!(eq_ignore_case("/gırıs", "/GIRIS"));
        assert!(!eq_ignore_case("auto", "autos"));
        assert!(eq_ignore_case("straße", "STRAßE"));
    }

    #[test]
    fn finds_case_insensitive_ranges_on_char_boundaries() {
        let text = "[Script] send /LOGIN şifre";
        let (start, end) = find_ignore_case(text, "/login şİfre", 0).unwrap_or((0, 0));
        assert_eq!((start, end), (0, 0), "İ folds to İ, not to I");
        let (start, end) = find_ignore_case(text, "/login ŞIFRE", 0).unwrap();
        assert_eq!(&text[start..end], "/LOGIN şifre");
        assert!(find_ignore_case(text, "", 0).is_none());
        assert!(contains_ignore_case(
            "Server is RESTARTING now",
            "restarting"
        ));
    }

    #[test]
    fn strips_formatting_codes() {
        assert_eq!(strip_color_codes("§cKicked: §lspam"), "Kicked: spam");
        assert_eq!(strip_color_codes("trailing §"), "trailing ");
    }

    #[test]
    fn one_line_flattens_and_caps() {
        assert_eq!(one_line("  a\nb\r\nc  ", 10), "a b  c");
        assert_eq!(one_line("abcdef", 4), "abc…");
    }

    #[test]
    fn atomic_write_replaces_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("f.txt");
        write_atomic(&path, b"one", false).unwrap();
        write_atomic(&path, b"two", true).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"two");
        assert!(!tmp_path(&path).exists());
        remove_file_if_exists(&path).unwrap();
        remove_file_if_exists(&path).unwrap();
    }
}

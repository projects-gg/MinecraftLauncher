//! Safe extraction of a sub-tree of a zip archive into a directory.

use std::fs::File;
use std::io::{BufWriter, Read, Seek, Write};
use std::path::Path;

use pl_net::CancellationToken;
use zip::ZipArchive;

use super::safe_path::validate_relative_path;
use crate::error::{Error, Result};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct ExtractReport {
    pub files: u64,
    pub bytes: u64,
    /// Entries left out: unsafe paths, symlinks, unreadable names.
    pub skipped: Vec<String>,
}

/// Entry names of the archive with `\` normalized to `/` (some Windows tools write them).
pub(crate) fn entry_names<R: Read + Seek>(archive: &ZipArchive<R>) -> Vec<String> {
    archive
        .file_names()
        .filter_map(|name| name.ok())
        .map(|name| name.replace('\\', "/"))
        .collect()
}

/// Read a small text entry (manifests, configs). `None` if missing or larger than `limit`.
pub(crate) fn read_entry_string<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
    limit: u64,
) -> Option<String> {
    let index = archive.index_for_name(name).or_else(|| {
        // Fall back to a backslash spelling of the same name.
        archive.index_for_name(&name.replace('/', "\\"))
    })?;
    let entry = archive.by_index(index).ok()?;
    if entry.size() > limit {
        return None;
    }
    let mut bytes = Vec::new();
    entry.take(limit + 1).read_to_end(&mut bytes).ok()?;
    Some(
        String::from_utf8_lossy(&bytes)
            .trim_start_matches('\u{feff}')
            .to_owned(),
    )
}

/// Extract every entry below `prefix` (e.g. `"overrides/"`, or `""` for everything) into
/// `dest`. Each path is validated with [`validate_relative_path`]; unsafe entries and links
/// are skipped and listed in the report. Existing files are overwritten.
pub(crate) fn extract_prefix<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    archive_path: &Path,
    prefix: &str,
    dest: &Path,
    cancel: &CancellationToken,
    report: &mut ExtractReport,
) -> Result<()> {
    for index in 0..archive.len() {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let mut entry = match archive.by_index(index) {
            Ok(entry) => entry,
            Err(e) => {
                report.skipped.push(format!("#{index}: {e}"));
                continue;
            }
        };
        let name = match entry.name() {
            Ok(name) => name.replace('\\', "/"),
            Err(e) => {
                report.skipped.push(format!("#{index}: {e}"));
                continue;
            }
        };
        let Some(rel) = name.strip_prefix(prefix) else {
            continue;
        };
        let is_dir = entry.is_dir() || rel.ends_with('/');
        let rel = rel.trim_end_matches('/');
        if rel.is_empty() {
            continue;
        }
        let Ok(rel_path) = validate_relative_path(rel) else {
            report.skipped.push(name);
            continue;
        };
        let target = dest.join(rel_path);
        if is_dir {
            std::fs::create_dir_all(&target).map_err(|e| Error::io(&target, e))?;
            continue;
        }
        if entry.is_symlink() {
            report.skipped.push(name);
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        let file = File::create(&target).map_err(|e| Error::io(&target, e))?;
        let mut out = BufWriter::new(file);
        let copied = std::io::copy(&mut entry, &mut out).map_err(|e| {
            if e.kind() == std::io::ErrorKind::InvalidData {
                Error::archive(archive_path, format!("{name}: {e}"))
            } else {
                Error::io(&target, e)
            }
        })?;
        out.flush().map_err(|e| Error::io(&target, e))?;
        report.files += 1;
        report.bytes += copied;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::mods::test_support::zip_bytes;

    #[test]
    fn extracts_only_safe_entries_below_prefix() {
        let bytes = zip_bytes(&[
            ("overrides/config/a.cfg", b"a"),
            ("overrides/mods/b.jar", b"bb"),
            ("overrides/../escape.txt", b"x"),
            ("overrides/CON.txt", b"x"),
            ("overrides\\config\\win.cfg", b"w"),
            ("other/ignored.txt", b"i"),
            ("overrides/", b""),
        ]);
        let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let mut report = ExtractReport::default();
        extract_prefix(
            &mut archive,
            Path::new("test.zip"),
            "overrides/",
            dir.path(),
            &CancellationToken::new(),
            &mut report,
        )
        .unwrap();
        assert_eq!(report.files, 3);
        assert_eq!(report.bytes, 4);
        assert_eq!(
            report.skipped,
            vec!["overrides/../escape.txt", "overrides/CON.txt"]
        );
        assert_eq!(
            std::fs::read(dir.path().join("config").join("win.cfg")).unwrap(),
            b"w"
        );
        assert!(!dir.path().join("ignored.txt").exists());
        assert!(!dir.path().parent().unwrap().join("escape.txt").exists());
    }

    #[test]
    fn cancellation_stops_extraction() {
        let bytes = zip_bytes(&[("a.txt", b"a")]);
        let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let token = CancellationToken::new();
        token.cancel();
        let result = extract_prefix(
            &mut archive,
            Path::new("t.zip"),
            "",
            dir.path(),
            &token,
            &mut ExtractReport::default(),
        );
        assert!(matches!(result, Err(Error::Cancelled)));
    }
}

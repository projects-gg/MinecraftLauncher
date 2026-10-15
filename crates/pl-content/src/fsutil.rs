//! Small file and hashing helpers shared by the client, the sync engine and Studio helpers.

use std::path::{Path, PathBuf};

use pl_net::HashAlgo;
use rand::Rng;

use crate::error::{Error, Result};

/// Lowercase hex SHA-256 of a buffer.
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    pl_net::hash_bytes(bytes, HashAlgo::Sha256)
}

/// Normalizes a hex digest of `len` characters to lowercase, or `None` when it is malformed.
pub(crate) fn normalize_hex(value: &str, len: usize) -> Option<String> {
    let value = value.trim();
    (value.len() == len && value.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| value.to_ascii_lowercase())
}

/// Writes `bytes` to `path` atomically: a uniquely named temporary file next to the target is
/// written first and then renamed over it, so readers never observe a partial file.
pub(crate) async fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| Error::io(parent, e))?;
    }
    let tmp = tmp_path(path);
    if let Err(e) = tokio::fs::write(&tmp, bytes).await {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(Error::io(&tmp, e));
    }
    if let Err(e) = tokio::fs::rename(&tmp, path).await {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(Error::io(path, e));
    }
    Ok(())
}

fn tmp_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(format!(".{:016x}.tmp", rand::rng().next_u64()));
    path.with_file_name(name)
}

/// Reads a file, mapping "not found" to `Ok(None)`.
pub(crate) async fn read_optional(path: &Path) -> Result<Option<Vec<u8>>> {
    match tokio::fs::read(path).await {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::io(path, e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_normalization() {
        assert_eq!(normalize_hex(" ABcd ", 4).as_deref(), Some("abcd"));
        assert_eq!(normalize_hex("abc", 4), None);
        assert_eq!(normalize_hex("abcg", 4), None);
    }

    #[tokio::test]
    async fn atomic_write_replaces_and_leaves_no_temp_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("file.json");
        write_atomic(&path, b"one").await.unwrap();
        write_atomic(&path, b"two").await.unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"two");
        let entries: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(
            entries.len(),
            1,
            "temporary files must be renamed away: {entries:?}"
        );
        assert_eq!(
            read_optional(&dir.path().join("missing")).await.unwrap(),
            None
        );
    }
}

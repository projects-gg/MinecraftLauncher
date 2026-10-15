use std::io::Read;
use std::path::Path;

use sha1::Digest;

use crate::download::VerifyMode;
use crate::error::{NetError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HashAlgo {
    Sha1,
    Sha256,
    Sha512,
}

/// An expected digest, always stored as lowercase hex.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Checksum {
    pub algo: HashAlgo,
    pub hex: String,
}

impl Checksum {
    pub fn new(algo: HashAlgo, hex: impl AsRef<str>) -> Self {
        Self {
            algo,
            hex: hex.as_ref().trim().to_ascii_lowercase(),
        }
    }

    pub fn sha1(hex: impl AsRef<str>) -> Self {
        Self::new(HashAlgo::Sha1, hex)
    }

    pub fn sha256(hex: impl AsRef<str>) -> Self {
        Self::new(HashAlgo::Sha256, hex)
    }

    pub fn sha512(hex: impl AsRef<str>) -> Self {
        Self::new(HashAlgo::Sha512, hex)
    }
}

pub(crate) enum Hasher {
    Sha1(sha1::Sha1),
    Sha256(sha2::Sha256),
    Sha512(sha2::Sha512),
}

impl Hasher {
    pub(crate) fn new(algo: HashAlgo) -> Self {
        match algo {
            HashAlgo::Sha1 => Self::Sha1(sha1::Sha1::new()),
            HashAlgo::Sha256 => Self::Sha256(sha2::Sha256::new()),
            HashAlgo::Sha512 => Self::Sha512(sha2::Sha512::new()),
        }
    }

    pub(crate) fn update(&mut self, data: &[u8]) {
        match self {
            Self::Sha1(h) => h.update(data),
            Self::Sha256(h) => h.update(data),
            Self::Sha512(h) => h.update(data),
        }
    }

    pub(crate) fn finalize_hex(self) -> String {
        match self {
            Self::Sha1(h) => hex::encode(h.finalize()),
            Self::Sha256(h) => hex::encode(h.finalize()),
            Self::Sha512(h) => hex::encode(h.finalize()),
        }
    }
}

/// Hash an in-memory buffer.
pub fn hash_bytes(data: &[u8], algo: HashAlgo) -> String {
    let mut hasher = Hasher::new(algo);
    hasher.update(data);
    hasher.finalize_hex()
}

/// Stream a file through the hasher on the blocking pool.
pub async fn hash_file(path: &Path, algo: HashAlgo) -> Result<String> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let mut file = std::fs::File::open(&path).map_err(|e| NetError::io(&path, e))?;
        let mut hasher = Hasher::new(algo);
        let mut buf = vec![0u8; 256 * 1024];
        loop {
            let n = file.read(&mut buf).map_err(|e| NetError::io(&path, e))?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        Ok(hasher.finalize_hex())
    })
    .await
    .map_err(|e| NetError::Setup(format!("hash task panicked: {e}")))?
}

/// Check whether an existing file already satisfies the expectations.
/// Returns `false` (never an error) when the file is missing or unreadable.
pub async fn verify_file(
    path: &Path,
    checksum: Option<&Checksum>,
    size: Option<u64>,
    mode: VerifyMode,
) -> bool {
    let Ok(meta) = tokio::fs::metadata(path).await else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    if mode == VerifyMode::Exists {
        return true;
    }
    if let Some(expected) = size
        && meta.len() != expected
    {
        return false;
    }
    if mode == VerifyMode::Size {
        return true;
    }
    match checksum {
        Some(sum) => hash_file(path, sum.algo)
            .await
            .map(|actual| actual == sum.hex)
            .unwrap_or(false),
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_digests() {
        assert_eq!(
            hash_bytes(b"abc", HashAlgo::Sha1),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(
            hash_bytes(b"abc", HashAlgo::Sha256),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(hash_bytes(b"abc", HashAlgo::Sha512).starts_with("ddaf35a193617aba"));
    }

    #[test]
    fn checksum_normalizes_case() {
        assert_eq!(Checksum::sha1(" ABCDEF ").hex, "abcdef");
    }

    #[tokio::test]
    async fn verify_modes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.txt");
        std::fs::write(&path, b"abc").unwrap();
        let good = Checksum::sha1("a9993e364706816aba3e25717850c26c9cd0d89d");
        let bad = Checksum::sha1("0000");
        assert!(verify_file(&path, Some(&good), Some(3), VerifyMode::Full).await);
        assert!(!verify_file(&path, Some(&bad), Some(3), VerifyMode::Full).await);
        assert!(verify_file(&path, Some(&bad), Some(3), VerifyMode::Size).await);
        assert!(!verify_file(&path, None, Some(4), VerifyMode::Size).await);
        assert!(verify_file(&path, Some(&bad), Some(4), VerifyMode::Exists).await);
        assert!(!verify_file(&dir.path().join("missing"), None, None, VerifyMode::Exists).await);
    }
}

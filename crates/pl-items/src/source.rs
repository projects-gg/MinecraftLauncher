//! Where the game's assets come from: a client jar on this computer, or files in memory (tests,
//! resource-pack overlays). Paths are the jar's own: `assets/minecraft/items/stone.json`.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::path::Path;

use crate::Error;

/// The largest single asset read (textures and models are a few KiB; this stops a broken jar).
const MAX_ASSET_BYTES: u64 = 8 * 1024 * 1024;

pub trait AssetSource {
    /// A file's bytes, or None when it is not there.
    fn read(&mut self, path: &str) -> Option<Vec<u8>>;
    /// Every file path under `dir` (no trailing slash) that ends with `suffix`, sorted.
    fn list(&mut self, dir: &str, suffix: &str) -> Vec<String>;
}

/// A client jar (any zip with the game's `assets/` tree).
pub struct JarSource<R: Read + Seek> {
    archive: zip::ZipArchive<R>,
}

impl JarSource<BufReader<File>> {
    pub fn open(path: &Path) -> Result<Self, Error> {
        let file = File::open(path).map_err(|e| Error::Jar(format!("{}: {e}", path.display())))?;
        Self::new(BufReader::new(file))
    }
}

impl<R: Read + Seek> JarSource<R> {
    pub fn new(reader: R) -> Result<Self, Error> {
        let archive = zip::ZipArchive::new(reader).map_err(|e| Error::Jar(e.to_string()))?;
        Ok(Self { archive })
    }
}

impl<R: Read + Seek> AssetSource for JarSource<R> {
    fn read(&mut self, path: &str) -> Option<Vec<u8>> {
        let file = self.archive.by_name(path).ok()?;
        if file.size() > MAX_ASSET_BYTES {
            return None;
        }
        let mut bytes = Vec::with_capacity(file.size() as usize);
        file.take(MAX_ASSET_BYTES).read_to_end(&mut bytes).ok()?;
        Some(bytes)
    }

    fn list(&mut self, dir: &str, suffix: &str) -> Vec<String> {
        let prefix = format!("{dir}/");
        let mut out: Vec<String> = self
            .archive
            .file_names()
            .filter_map(|name| name.ok())
            .filter(|name| name.starts_with(&prefix) && name.ends_with(suffix))
            .map(|name| name.into_owned())
            .collect();
        out.sort();
        out
    }
}

/// Files in memory.
#[derive(Default)]
pub struct MemSource {
    pub files: BTreeMap<String, Vec<u8>>,
}

impl MemSource {
    pub fn insert(&mut self, path: impl Into<String>, bytes: impl Into<Vec<u8>>) {
        self.files.insert(path.into(), bytes.into());
    }
}

impl AssetSource for MemSource {
    fn read(&mut self, path: &str) -> Option<Vec<u8>> {
        self.files.get(path).cloned()
    }

    fn list(&mut self, dir: &str, suffix: &str) -> Vec<String> {
        let prefix = format!("{dir}/");
        self.files
            .keys()
            .filter(|name| name.starts_with(&prefix) && name.ends_with(suffix))
            .cloned()
            .collect()
    }
}

/// A resource location (`minecraft:block/stone`, `block/stone`) split into namespace and path.
pub fn split_id(id: &str) -> (&str, &str) {
    match id.split_once(':') {
        Some((ns, path)) => (ns, path),
        None => ("minecraft", id),
    }
}

/// `minecraft:block/stone` with the namespace spelled out.
pub fn full_id(id: &str) -> String {
    let (ns, path) = split_id(id);
    format!("{ns}:{path}")
}

pub fn model_path(id: &str) -> String {
    let (ns, path) = split_id(id);
    format!("assets/{ns}/models/{path}.json")
}

pub fn texture_path(id: &str) -> String {
    let (ns, path) = split_id(id);
    format!("assets/{ns}/textures/{path}.png")
}

pub fn item_definition_path(id: &str) -> String {
    let (ns, path) = split_id(id);
    format!("assets/{ns}/items/{path}.json")
}

//! Publishing helpers for Projects Studio.
//!
//! [`IndexBuilder`] turns documents into content-addressed files, computes their
//! [`DocRef`]s and assembles the next [`RootIndex`] with a higher `sequence`.
//! Publish order: upload [`BuiltIndex::documents`] (and any media/files) first, then
//! `index.json.sig`, then `index.json` last; the index is the commit point.

use std::collections::BTreeMap;

use jiff::Timestamp;

use crate::error::{Error, Result};
use crate::fsutil::sha256_hex;
use crate::json::to_canonical_json;
use crate::model::{
    AfkRelease, DocRef, LauncherConfig, MediaKind, MediaRef, ModSetCatalog, NewsFeed, PackHashes,
    PackManifest, PackRef, ROOT_INDEX_SCHEMA, RootIndex,
};
use crate::path::check_mirror_path;
use crate::signing::{SignatureFile, SigningKey, sign_index};

/// A file to upload, with its content-derived reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedDoc {
    /// Path relative to the mirror base.
    pub path: String,
    pub sha256: String,
    pub bytes: Vec<u8>,
}

impl PublishedDoc {
    fn new(path: String, bytes: Vec<u8>) -> Self {
        Self {
            sha256: sha256_hex(&bytes),
            path,
            bytes,
        }
    }

    pub fn doc_ref(&self) -> DocRef {
        DocRef {
            path: self.path.clone(),
            sha256: self.sha256.clone(),
            size: self.bytes.len() as u64,
        }
    }
}

/// Result of [`IndexBuilder::build`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltIndex {
    pub index: RootIndex,
    /// Exact bytes of `index.json` (canonical JSON); these are what gets signed.
    pub index_bytes: Vec<u8>,
    /// Immutable documents to upload before the index, in path order.
    pub documents: Vec<PublishedDoc>,
}

impl BuiltIndex {
    /// Signs `index_bytes`; returns the signature and the bytes of `index.json.sig`.
    pub fn sign(&self, key: &SigningKey) -> Result<(SignatureFile, Vec<u8>)> {
        let signature = sign_index(&self.index_bytes, key);
        let bytes = to_canonical_json(&signature)?;
        Ok((signature, bytes))
    }
}

/// Assembles the next root index. Start from the live index with
/// [`IndexBuilder::from_previous`] so unchanged parts keep their references.
#[derive(Debug, Clone, Default)]
pub struct IndexBuilder {
    previous_sequence: u64,
    sequence: Option<u64>,
    config: Option<DocRef>,
    news: Option<DocRef>,
    packs: Vec<PackRef>,
    afk: Option<AfkRelease>,
    mod_sets: Option<DocRef>,
    documents: BTreeMap<String, PublishedDoc>,
}

impl IndexBuilder {
    /// An empty builder (first publish).
    pub fn new() -> Self {
        Self::default()
    }

    /// Keeps every reference of `previous`; the new sequence will be higher.
    pub fn from_previous(previous: &RootIndex) -> Self {
        Self {
            previous_sequence: previous.sequence,
            sequence: None,
            config: Some(previous.config.clone()),
            news: Some(previous.news.clone()),
            packs: previous.packs.clone(),
            afk: previous.afk.clone(),
            mod_sets: previous.mod_sets.clone(),
            documents: BTreeMap::new(),
        }
    }

    /// Serializes the ready-made mod sets to `modsets/modsets.<sha256>.json`; `None` or a
    /// catalog without sets drops the reference. The catalog must be valid
    /// ([`ModSetCatalog::validate`]).
    pub fn set_mod_sets(&mut self, catalog: Option<&ModSetCatalog>) -> Result<Option<DocRef>> {
        let Some(catalog) = catalog.filter(|catalog| !catalog.sets.is_empty()) else {
            if let Some(previous) = self.mod_sets.take() {
                self.documents.remove(&previous.path);
            }
            return Ok(None);
        };
        catalog.validate()?;
        let doc = PublishedDoc::new(String::new(), to_canonical_json(catalog)?);
        let doc = PublishedDoc {
            path: mod_sets_path(&doc.sha256),
            ..doc
        };
        let doc_ref = self.add_document(doc, self.mod_sets.clone());
        self.mod_sets = Some(doc_ref.clone());
        Ok(Some(doc_ref))
    }

    /// Serializes the launcher config to `config/launcher.<sha256>.json`.
    pub fn set_config(&mut self, config: &LauncherConfig) -> Result<DocRef> {
        let bytes = to_canonical_json(config)?;
        let doc = PublishedDoc::new(String::new(), bytes);
        let doc = PublishedDoc {
            path: config_path(&doc.sha256),
            ..doc
        };
        let doc_ref = self.add_document(doc, self.config.clone());
        self.config = Some(doc_ref.clone());
        Ok(doc_ref)
    }

    /// Serializes the news feed to `news/news.<sha256>.json`.
    pub fn set_news(&mut self, news: &NewsFeed) -> Result<DocRef> {
        let bytes = to_canonical_json(news)?;
        let doc = PublishedDoc::new(String::new(), bytes);
        let doc = PublishedDoc {
            path: news_path(&doc.sha256),
            ..doc
        };
        let doc_ref = self.add_document(doc, self.news.clone());
        self.news = Some(doc_ref.clone());
        Ok(doc_ref)
    }

    /// Adds or replaces a pack (id = `manifest.projects.packId`) with its manifest at
    /// `packs/<id>/<versionId>.json`. Manifests are immutable per version: publish changes
    /// under a new `versionId`. The reference copies the manifest's family (omitted when it is
    /// the pack id) and Minecraft version; a replaced pack keeps its `default` flag.
    pub fn set_pack(
        &mut self,
        name: impl Into<String>,
        manifest: &PackManifest,
    ) -> Result<PackRef> {
        manifest.validate()?;
        let id = manifest.projects.pack_id.clone();
        let path = pack_manifest_path(&id, &manifest.version_id)?;
        let doc = PublishedDoc::new(path, to_canonical_json(manifest)?);
        let existing = self.packs.iter().find(|p| p.id == id);
        let previous = existing.map(|p| p.manifest.clone());
        let default = existing.is_some_and(|p| p.default);
        let family = manifest.projects.family_id();
        let minecraft = manifest.dependencies.minecraft.trim();
        let pack = PackRef {
            id: id.clone(),
            name: name.into(),
            latest: manifest.version_id.clone(),
            manifest: self.add_document(doc, previous),
            family: (family != id).then(|| family.to_owned()),
            minecraft: (!minecraft.is_empty()).then(|| minecraft.to_owned()),
            default,
        };
        match self.packs.iter_mut().find(|p| p.id == id) {
            Some(existing) => *existing = pack.clone(),
            None => self.packs.push(pack.clone()),
        }
        Ok(pack)
    }

    /// Makes `id` the member of its family that new installs use (and clears the flag on the
    /// other members of that family).
    pub fn set_family_default(&mut self, id: &str) -> Result<()> {
        let family = self
            .packs
            .iter()
            .find(|pack| pack.id == id)
            .map(|pack| pack.family_id().to_owned())
            .ok_or_else(|| Error::Invalid(format!("pack {id} is not in the index")))?;
        for pack in &mut self.packs {
            if pack.family_id() == family {
                pack.default = pack.id == id;
            }
        }
        Ok(())
    }

    /// Removes a pack from the index; returns whether it was listed.
    pub fn remove_pack(&mut self, id: &str) -> bool {
        let before = self.packs.len();
        self.packs.retain(|pack| pack.id != id);
        before != self.packs.len()
    }

    pub fn set_afk(&mut self, release: Option<AfkRelease>) {
        self.afk = release;
    }

    /// Forces a specific sequence (recovery or tests). It must exceed the previous one.
    pub fn set_sequence(&mut self, sequence: u64) {
        self.sequence = Some(sequence);
    }

    /// Builds the index. The sequence defaults to `max(previous + 1, unix seconds of
    /// generated_at)`, so it keeps increasing even when Studio lost track of the last one.
    pub fn build(self, generated_at: Timestamp) -> Result<BuiltIndex> {
        let config = self
            .config
            .ok_or_else(|| Error::Invalid("the index needs a launcher config".into()))?;
        let news = self
            .news
            .ok_or_else(|| Error::Invalid("the index needs a news feed".into()))?;
        let next = self.previous_sequence.saturating_add(1);
        let sequence = self
            .sequence
            .unwrap_or_else(|| next.max(u64::try_from(generated_at.as_second()).unwrap_or(0)));
        if sequence <= self.previous_sequence {
            return Err(Error::Invalid(format!(
                "sequence {sequence} must be greater than {}",
                self.previous_sequence
            )));
        }
        let index = RootIndex {
            schema: ROOT_INDEX_SCHEMA,
            sequence,
            generated_at,
            config,
            news,
            packs: self.packs,
            afk: self.afk,
            mod_sets: self.mod_sets,
        };
        index.validate()?;
        let index_bytes = to_canonical_json(&index)?;
        Ok(BuiltIndex {
            index,
            index_bytes,
            documents: self.documents.into_values().collect(),
        })
    }

    /// Records `doc` for upload and drops the document it replaces if this builder added it.
    fn add_document(&mut self, doc: PublishedDoc, replaced: Option<DocRef>) -> DocRef {
        if let Some(replaced) = replaced
            && replaced.path != doc.path
        {
            self.documents.remove(&replaced.path);
        }
        let doc_ref = doc.doc_ref();
        self.documents.insert(doc.path.clone(), doc);
        doc_ref
    }
}

/// `config/launcher.<sha256>.json`
pub fn config_path(sha256: &str) -> String {
    format!("config/launcher.{sha256}.json")
}

/// `news/news.<sha256>.json`
pub fn news_path(sha256: &str) -> String {
    format!("news/news.{sha256}.json")
}

/// `modsets/modsets.<sha256>.json`
pub fn mod_sets_path(sha256: &str) -> String {
    format!("modsets/modsets.{sha256}.json")
}

/// `packs/<id>/<version>.json`; both parts must be single `[A-Za-z0-9._-]` components.
pub fn pack_manifest_path(id: &str, version: &str) -> Result<String> {
    for part in [id, version] {
        if part.is_empty() || part.contains('/') {
            return Err(Error::Invalid(format!("bad pack id or version {part:?}")));
        }
    }
    let path = format!("packs/{id}/{version}.json");
    check_mirror_path(&path)?;
    Ok(path)
}

/// `media/<sha256>.<ext>`
pub fn media_path(sha256: &str, extension: &str) -> String {
    format!("media/{sha256}.{}", extension.to_ascii_lowercase())
}

/// `files/<sha1>`: a file we host for packs.
pub fn hosted_file_path(sha1: &str) -> String {
    format!("files/{sha1}")
}

/// `afk/<version>/<platform>.zip`
pub fn afk_build_path(version: &str, platform: &str) -> String {
    format!("afk/{version}/{platform}.zip")
}

/// A [`MediaRef`] for uploaded bytes (dimensions, blurhash and poster are filled by Studio).
pub fn media_ref(bytes: &[u8], extension: &str, kind: MediaKind) -> MediaRef {
    let sha256 = sha256_hex(bytes);
    MediaRef {
        path: media_path(&sha256, extension),
        sha256,
        size: bytes.len() as u64,
        kind,
        width: None,
        height: None,
        blurhash: None,
        poster: None,
        alt: None,
    }
}

/// `.mrpack` hashes (SHA-1 + SHA-512) of a file.
pub fn pack_hashes(bytes: &[u8]) -> PackHashes {
    PackHashes {
        sha1: pl_net::hash_bytes(bytes, pl_net::HashAlgo::Sha1),
        sha512: Some(pl_net::hash_bytes(bytes, pl_net::HashAlgo::Sha512)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signing::verify_index;
    use crate::testutil::{sample_config, sample_news};

    fn at(text: &str) -> Timestamp {
        text.parse().unwrap()
    }

    #[test]
    fn builds_content_addressed_documents() {
        let mut builder = IndexBuilder::new();
        let config = builder.set_config(&sample_config(None)).unwrap();
        let news = builder.set_news(&sample_news(None)).unwrap();
        assert!(config.path.starts_with("config/launcher.") && config.path.ends_with(".json"));
        assert!(news.path.contains(&news.sha256));
        let built = builder.build(at("2026-10-15T12:00:00Z")).unwrap();
        assert_eq!(built.documents.len(), 2);
        for doc in &built.documents {
            assert_eq!(sha256_hex(&doc.bytes), doc.sha256);
        }
        let unix_seconds = u64::try_from(at("2026-10-15T12:00:00Z").as_second()).unwrap();
        assert_eq!(built.index.sequence, unix_seconds, "unix time floor");
        assert_eq!(
            serde_json::from_slice::<RootIndex>(&built.index_bytes).unwrap(),
            built.index
        );

        let key = SigningKey::from_bytes(&[5; 32]);
        let (signature, bytes) = built.sign(&key).unwrap();
        verify_index(&built.index_bytes, &signature, &[key.verifying_key()]).unwrap();
        assert_eq!(
            serde_json::from_slice::<SignatureFile>(&bytes).unwrap(),
            signature
        );
    }

    #[test]
    fn output_is_deterministic() {
        let build = || {
            let mut builder = IndexBuilder::new();
            builder.set_config(&sample_config(None)).unwrap();
            builder.set_news(&sample_news(None)).unwrap();
            builder.build(at("2026-10-15T12:00:00Z")).unwrap()
        };
        assert_eq!(build(), build());
    }

    #[test]
    fn sequence_increases_from_previous() {
        let mut first = IndexBuilder::new();
        first.set_config(&sample_config(None)).unwrap();
        first.set_news(&sample_news(None)).unwrap();
        first.set_sequence(u64::MAX - 5);
        let first = first.build(at("2026-10-15T12:00:00Z")).unwrap();

        let mut next = IndexBuilder::from_previous(&first.index);
        let mut news = sample_news(None);
        news.items[0].title = "Güncel".into();
        let news_ref = next.set_news(&news).unwrap();
        let next = next.build(at("2026-10-15T12:01:00Z")).unwrap();
        assert_eq!(next.index.sequence, u64::MAX - 4);
        assert_eq!(
            next.index.config, first.index.config,
            "unchanged parts are kept"
        );
        assert_eq!(next.index.news, news_ref);
        assert_eq!(
            next.documents.len(),
            1,
            "only the changed document is uploaded"
        );

        let mut stale = IndexBuilder::from_previous(&next.index);
        stale.set_sequence(3);
        assert!(stale.build(at("2026-10-15T12:02:00Z")).is_err());
        assert!(
            IndexBuilder::new()
                .build(at("2026-10-15T12:02:00Z"))
                .is_err()
        );
    }

    #[test]
    fn mod_sets_are_published_as_a_document() {
        use crate::testutil::{sample_mod_sets, sample_variant};
        let catalog = sample_mod_sets(sample_variant(None, "26.3", &[("mods/zoom.jar", b"z")]));
        let mut builder = IndexBuilder::new();
        builder.set_config(&sample_config(None)).unwrap();
        builder.set_news(&sample_news(None)).unwrap();
        let doc = builder.set_mod_sets(Some(&catalog)).unwrap().unwrap();
        assert!(doc.path.starts_with("modsets/modsets.") && doc.path.contains(&doc.sha256));
        let built = builder.build(at("2026-10-15T12:00:00Z")).unwrap();
        assert_eq!(built.index.mod_sets.as_ref(), Some(&doc));
        let uploaded = built
            .documents
            .iter()
            .find(|published| published.path == doc.path)
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<ModSetCatalog>(&uploaded.bytes).unwrap(),
            catalog
        );

        // Kept by the next publish, dropped when the team removes every set.
        let next = IndexBuilder::from_previous(&built.index);
        let kept = next.build(at("2026-10-15T12:01:00Z")).unwrap();
        assert_eq!(kept.index.mod_sets, built.index.mod_sets);
        assert!(kept.documents.is_empty());
        let mut cleared = IndexBuilder::from_previous(&built.index);
        assert_eq!(
            cleared
                .set_mod_sets(Some(&ModSetCatalog::default()))
                .unwrap(),
            None
        );
        assert_eq!(
            cleared
                .build(at("2026-10-15T12:02:00Z"))
                .unwrap()
                .index
                .mod_sets,
            None
        );

        // An invalid catalog is refused.
        let mut broken = catalog.clone();
        broken.sets[0].name = String::new();
        let mut refused = IndexBuilder::new();
        assert!(refused.set_mod_sets(Some(&broken)).is_err());
    }

    #[test]
    fn replacing_a_document_drops_the_superseded_upload() {
        let mut builder = IndexBuilder::new();
        builder.set_config(&sample_config(None)).unwrap();
        let mut changed = sample_config(None);
        changed.hero.title = "Yeni".into();
        let current = builder.set_config(&changed).unwrap();
        builder.set_news(&sample_news(None)).unwrap();
        let built = builder.build(at("2026-10-15T12:00:00Z")).unwrap();
        let paths: Vec<_> = built.documents.iter().map(|d| d.path.clone()).collect();
        assert_eq!(paths.len(), 2);
        assert!(paths.contains(&current.path));
    }

    #[test]
    fn pack_families() {
        let member = |id: &str, family: Option<&str>, minecraft: &str| {
            let mut manifest = sample_manifest_offline(id);
            manifest.projects.family = family.map(str::to_owned);
            manifest.dependencies.minecraft = minecraft.into();
            manifest
        };
        let mut builder = IndexBuilder::new();
        builder.set_config(&sample_config(None)).unwrap();
        builder.set_news(&sample_news(None)).unwrap();
        let first = builder
            .set_pack("Projects", &member("projects", None, "1.21.1"))
            .unwrap();
        assert_eq!(first.family, None, "a pack is its own family");
        assert_eq!(first.minecraft.as_deref(), Some("1.21.1"));
        let second = builder
            .set_pack(
                "Projects",
                &member("projects-1.21.4", Some("projects"), "1.21.4"),
            )
            .unwrap();
        assert_eq!(second.family.as_deref(), Some("projects"));
        builder.set_family_default("projects").unwrap();
        assert!(builder.set_family_default("missing").is_err());
        // A new version of the default member stays the default.
        let mut update = member("projects", Some("projects"), "1.21.1");
        update.version_id = "1.0.1".into();
        let updated = builder.set_pack("Projects", &update).unwrap();
        assert!(updated.default && updated.family.is_none());

        let built = builder.build(at("2026-10-15T12:00:00Z")).unwrap();
        let index = &built.index;
        assert_eq!(index.family_default("projects").unwrap().id, "projects");
        assert_eq!(
            index.family_member("projects", "1.21.4").unwrap().id,
            "projects-1.21.4"
        );

        let mut next = IndexBuilder::from_previous(index);
        next.set_family_default("projects-1.21.4").unwrap();
        let next = next.build(at("2026-10-15T12:01:00Z")).unwrap();
        assert_eq!(
            next.index.family_default("projects").unwrap().id,
            "projects-1.21.4"
        );
        assert_eq!(next.index.packs.iter().filter(|p| p.default).count(), 1);

        // Two members on the same Minecraft version are refused at build time.
        let mut clash = IndexBuilder::from_previous(&next.index);
        clash
            .set_pack(
                "Projects",
                &member("projects-copy", Some("projects"), "1.21.4"),
            )
            .unwrap();
        assert!(clash.build(at("2026-10-15T12:02:00Z")).is_err());
    }

    /// A small valid manifest for `id` (files are never downloaded here).
    fn sample_manifest_offline(id: &str) -> PackManifest {
        serde_json::from_value(serde_json::json!({
            "formatVersion": 1,
            "versionId": "1.0.0",
            "name": "Projects",
            "files": [{
                "path": "mods/a.jar",
                "hashes": { "sha1": "a".repeat(40) },
                "downloads": ["https://cdn.modrinth.com/data/x/versions/y/a.jar"],
                "fileSize": 1
            }],
            "dependencies": { "minecraft": "1.21.1" },
            "projects": { "packId": id }
        }))
        .unwrap()
    }

    #[test]
    fn paths() {
        assert_eq!(
            pack_manifest_path("projects", "1.2.0").unwrap(),
            "packs/projects/1.2.0.json"
        );
        assert!(pack_manifest_path("../x", "1").is_err());
        assert!(pack_manifest_path("a/b", "1").is_err());
        assert!(pack_manifest_path("p", "1 0").is_err());
        assert_eq!(media_path("ab", "WEBP"), "media/ab.webp");
        assert_eq!(hosted_file_path("cd"), "files/cd");
        assert_eq!(
            afk_build_path("1.0.0", "linux-x86_64"),
            "afk/1.0.0/linux-x86_64.zip"
        );
        let media = media_ref(b"img", "png", MediaKind::Image);
        assert_eq!(media.size, 3);
        media.validate().unwrap();
        let hashes = pack_hashes(b"abc");
        assert_eq!(hashes.sha1, "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(hashes.sha512.unwrap().len(), 128);
    }
}

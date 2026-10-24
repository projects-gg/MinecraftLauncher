//! Publishing helpers for Projects Studio.
//!
//! [`IndexBuilder`] turns documents into content-addressed files, computes their
//! [`DocRef`]s and assembles the next [`RootIndex`] with a higher `sequence`.
//! Publish order: upload [`BuiltIndex::documents`] (and any media/files) first, then
//! `index.json.sig`, then `index.json` last; the index is the commit point.
//!
//! The `…_with` setters and [`IndexBuilder::build_with`] let Studio edit a document's JSON before
//! its bytes are made: it puts back what its model does not know (fields a newer Studio wrote),
//! so a republish keeps them. The reference and the signature cover the edited bytes, and the
//! edited document must still read as its type.

use std::collections::BTreeMap;

use jiff::Timestamp;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::error::{Error, Result};
use crate::fsutil::sha256_hex;
use crate::json::to_canonical_json;
use crate::model::{
    AfkRelease, DocRef, ItemArt, LauncherConfig, MediaKind, MediaRef, ModSetCatalog, NewsFeed,
    PackHashes, PackManifest, PackRef, ROOT_INDEX_SCHEMA, RootIndex,
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
    item_art: Option<DocRef>,
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
            item_art: previous.item_art.clone(),
            documents: BTreeMap::new(),
        }
    }

    /// Serializes the ready-made mod sets to `modsets/modsets.<sha256>.json`; `None` or a
    /// catalog without sets drops the reference. The catalog must be valid
    /// ([`ModSetCatalog::validate`]).
    pub fn set_mod_sets(&mut self, catalog: Option<&ModSetCatalog>) -> Result<Option<DocRef>> {
        self.set_mod_sets_with(catalog, |_| {})
    }

    /// [`Self::set_mod_sets`] with `edit` applied to the catalog's JSON (see the module
    /// documentation).
    pub fn set_mod_sets_with(
        &mut self,
        catalog: Option<&ModSetCatalog>,
        edit: impl FnOnce(&mut Value),
    ) -> Result<Option<DocRef>> {
        let Some(catalog) = catalog.filter(|catalog| !catalog.sets.is_empty()) else {
            if let Some(previous) = self.mod_sets.take() {
                self.documents.remove(&previous.path);
            }
            return Ok(None);
        };
        catalog.validate()?;
        let doc = PublishedDoc::new(String::new(), edited_json(catalog, edit)?);
        let doc = PublishedDoc {
            path: mod_sets_path(&doc.sha256),
            ..doc
        };
        let doc_ref = self.add_document(doc, self.mod_sets.clone());
        self.mod_sets = Some(doc_ref.clone());
        Ok(Some(doc_ref))
    }

    /// Serializes the item art to `itemart/items.<sha256>.json`; `None` or a document without
    /// items and glyphs drops the reference. The document must be valid ([`ItemArt::validate`]);
    /// its PNG files are uploaded by the caller (they are not documents).
    pub fn set_item_art(&mut self, art: Option<&ItemArt>) -> Result<Option<DocRef>> {
        self.set_item_art_with(art, |_| {})
    }

    /// [`Self::set_item_art`] with `edit` applied to the document's JSON (see the module
    /// documentation).
    pub fn set_item_art_with(
        &mut self,
        art: Option<&ItemArt>,
        edit: impl FnOnce(&mut Value),
    ) -> Result<Option<DocRef>> {
        let Some(art) = art.filter(|art| !art.items.is_empty() || !art.glyphs.is_empty()) else {
            if let Some(previous) = self.item_art.take() {
                self.documents.remove(&previous.path);
            }
            return Ok(None);
        };
        art.validate()?;
        let doc = PublishedDoc::new(String::new(), edited_json(art, edit)?);
        let doc = PublishedDoc {
            path: item_art_path(&doc.sha256),
            ..doc
        };
        let doc_ref = self.add_document(doc, self.item_art.clone());
        self.item_art = Some(doc_ref.clone());
        Ok(Some(doc_ref))
    }

    /// Serializes the launcher config to `config/launcher.<sha256>.json`.
    pub fn set_config(&mut self, config: &LauncherConfig) -> Result<DocRef> {
        self.set_config_with(config, |_| {})
    }

    /// [`Self::set_config`] with `edit` applied to the config's JSON (see the module
    /// documentation).
    pub fn set_config_with(
        &mut self,
        config: &LauncherConfig,
        edit: impl FnOnce(&mut Value),
    ) -> Result<DocRef> {
        let bytes = edited_json(config, edit)?;
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
        self.set_news_with(news, |_| {})
    }

    /// [`Self::set_news`] with `edit` applied to the feed's JSON (see the module documentation).
    pub fn set_news_with(
        &mut self,
        news: &NewsFeed,
        edit: impl FnOnce(&mut Value),
    ) -> Result<DocRef> {
        let bytes = edited_json(news, edit)?;
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
        self.set_pack_with(name, manifest, |_| {})
    }

    /// [`Self::set_pack`] with `edit` applied to the manifest's JSON (see the module
    /// documentation).
    pub fn set_pack_with(
        &mut self,
        name: impl Into<String>,
        manifest: &PackManifest,
        edit: impl FnOnce(&mut Value),
    ) -> Result<PackRef> {
        manifest.validate()?;
        let id = manifest.projects.pack_id.clone();
        let path = pack_manifest_path(&id, &manifest.version_id)?;
        let doc = PublishedDoc::new(path, edited_json(manifest, edit)?);
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
        self.build_with(generated_at, |_| {})
    }

    /// [`Self::build`] with `edit` applied to the index's JSON before it is signed (see the
    /// module documentation). [`BuiltIndex::index`] is the typed index without the edit.
    pub fn build_with(
        self,
        generated_at: Timestamp,
        edit: impl FnOnce(&mut Value),
    ) -> Result<BuiltIndex> {
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
            item_art: self.item_art,
        };
        index.validate()?;
        let index_bytes = edited_json(&index, edit)?;
        let edited: RootIndex =
            serde_json::from_slice(&index_bytes).map_err(|e| Error::json("edited index", e))?;
        edited.validate()?;
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

/// Canonical bytes of `document` after `edit` changed its JSON. The result must still read as
/// `T`: an edit can add what `T` does not know, never break what it does.
fn edited_json<T: Serialize + DeserializeOwned>(
    document: &T,
    edit: impl FnOnce(&mut Value),
) -> Result<Vec<u8>> {
    let mut value = serde_json::to_value(document).map_err(|e| Error::json("document", e))?;
    edit(&mut value);
    let bytes = to_canonical_json(&value)?;
    serde_json::from_slice::<T>(&bytes).map_err(|e| Error::json("edited document", e))?;
    Ok(bytes)
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

/// `itemart/items.<sha256>.json`
pub fn item_art_path(sha256: &str) -> String {
    format!("itemart/items.{sha256}.json")
}

/// `itemart/atlas-<size>.<sha256>.png`: an icon atlas of the item art.
pub fn item_art_atlas_path(size: u32, sha256: &str) -> String {
    format!("itemart/atlas-{size}.{sha256}.png")
}

/// `itemart/glyph.<sha256>.png`: a menu glyph's picture.
pub fn item_art_glyph_path(sha256: &str) -> String {
    format!("itemart/glyph.{sha256}.png")
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
    fn item_art_is_published_as_a_document() {
        let (art, _) = crate::testutil::sample_item_art();
        let mut builder = IndexBuilder::new();
        builder.set_config(&sample_config(None)).unwrap();
        builder.set_news(&sample_news(None)).unwrap();
        let doc = builder.set_item_art(Some(&art)).unwrap().unwrap();
        assert_eq!(doc.path, item_art_path(&doc.sha256));
        let built = builder.build(at("2026-10-15T12:00:00Z")).unwrap();
        assert_eq!(built.index.item_art.as_ref(), Some(&doc));
        let uploaded = built
            .documents
            .iter()
            .find(|published| published.path == doc.path)
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<ItemArt>(&uploaded.bytes).unwrap(),
            art
        );

        // Kept by the next publish, dropped when nothing is left to show.
        let kept = IndexBuilder::from_previous(&built.index)
            .build(at("2026-10-15T12:01:00Z"))
            .unwrap();
        assert_eq!(kept.index.item_art, built.index.item_art);
        let mut cleared = IndexBuilder::from_previous(&built.index);
        assert_eq!(
            cleared.set_item_art(Some(&ItemArt::default())).unwrap(),
            None
        );
        let cleared = cleared.build(at("2026-10-15T12:02:00Z")).unwrap();
        assert_eq!(cleared.index.item_art, None);
        assert!(cleared.documents.is_empty());

        // An invalid document is refused.
        let mut broken = art.clone();
        broken.hosts = vec!["projects.gg:25565".into()];
        assert!(IndexBuilder::new().set_item_art(Some(&broken)).is_err());
        assert_eq!(item_art_atlas_path(16, "ab"), "itemart/atlas-16.ab.png");
        assert_eq!(item_art_glyph_path("ab"), "itemart/glyph.ab.png");
    }

    #[test]
    fn edits_reach_the_bytes_the_reference_and_the_signature() {
        let mark = |key: &'static str| move |json: &mut Value| json[key] = serde_json::json!(true);
        let mut builder = IndexBuilder::new();
        let config = builder
            .set_config_with(&sample_config(None), mark("futureConfig"))
            .unwrap();
        let news = builder
            .set_news_with(&sample_news(None), mark("futureNews"))
            .unwrap();
        let pack = builder
            .set_pack_with("Projects", &sample_manifest_offline("projects"), |json| {
                json["projects"]["futurePack"] = serde_json::json!(1);
            })
            .unwrap();
        let built = builder
            .build_with(at("2026-10-15T12:00:00Z"), mark("futureIndex"))
            .unwrap();
        let document = |path: &str| -> Value {
            let doc = built.documents.iter().find(|d| d.path == path).unwrap();
            assert_eq!(sha256_hex(&doc.bytes), doc.sha256);
            serde_json::from_slice(&doc.bytes).unwrap()
        };
        assert_eq!(document(&config.path)["futureConfig"], true);
        assert_eq!(document(&news.path)["futureNews"], true);
        assert_eq!(document(&pack.manifest.path)["projects"]["futurePack"], 1);
        let config_doc = built
            .documents
            .iter()
            .find(|d| d.path == config.path)
            .unwrap();
        assert_eq!(config.size, config_doc.bytes.len() as u64);

        // The signed bytes are the edited index, canonical like any other.
        let index: Value = serde_json::from_slice(&built.index_bytes).unwrap();
        assert_eq!(index["futureIndex"], true);
        assert_eq!(to_canonical_json(&index).unwrap(), built.index_bytes);
        let key = SigningKey::from_bytes(&[7; 32]);
        let (signature, _) = built.sign(&key).unwrap();
        verify_index(&built.index_bytes, &signature, &[key.verifying_key()]).unwrap();

        // An edit that breaks what the type knows is refused.
        let mut broken = IndexBuilder::new();
        assert!(
            broken
                .set_config_with(&sample_config(None), |json| json["hero"] = Value::Null)
                .is_err()
        );
        let mut broken = IndexBuilder::new();
        broken.set_config(&sample_config(None)).unwrap();
        broken.set_news(&sample_news(None)).unwrap();
        assert!(
            broken
                .build_with(at("2026-10-15T12:00:00Z"), |json| json["schema"] =
                    99.into())
                .is_err()
        );
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

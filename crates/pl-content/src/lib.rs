//! Signed content index, news and home configuration models, modpack sync.
//!
//! Shared by the launcher, Projects Studio and (through the JSON formats specified in
//! `docs/content-format.md`) the Android app.
//!
//! - [`model`]: wire formats (root index, launcher config, news, pack manifest, release
//!   channel) with forward-compatible defaults.
//! - [`signing`]: Ed25519 keys, `index.json.sig` signing and verification, and the trusted key
//!   list ([`CONTENT_PUBLIC_KEYS`]) the launcher and Studio share.
//! - [`ContentClient`]: mirror fallback, signature + rollback checks, document and media
//!   cache, offline snapshots, cache garbage collection.
//! - [`items`]: the mod center's item selection ([`resolve_items`]): defaults, new items,
//!   `requires` closure, libraries and adopted manual changes.
//! - [`sync`]: modpack diff ([`plan_sync`]) and application ([`apply_sync`]) with ownership
//!   tracking, config policies, items, manual changes, optional groups and path safety.
//! - [`sets`]: per-version sets of a game directory ([`VersionSets`]): parking, restoring and
//!   starting `mods/` + `config/` per Minecraft version with a crash-safe switch.
//! - [`modsets`]: ready-made mod sets in a game directory ([`ModSetStates`]): installing,
//!   updating and removing a [`ModSet`] with the sync engine, per version like the pack state.
//! - [`needs`]: mods that need another mod in practice although they declare it optional
//!   ([`hidden_needs`]).
//! - [`studio`]: [`IndexBuilder`] and helpers to publish content; [`json`] gives the
//!   deterministic serialization Studio uploads.
//! - [`feed`]: the Discord bot's news feed ([`FeedClient`]: delegated-key verification,
//!   rollback checks, cache, covers) and the home screen merge rules ([`merge_news`],
//!   [`newest_patch_notes`], [`merge_events`], [`home_status`], [`merge_minecraft`]).

pub mod client;
pub mod error;
pub mod feed;
mod fsutil;
mod glob;
pub mod images;
pub mod items;
pub mod json;
pub mod model;
pub mod modsets;
pub mod needs;
pub mod path;
pub mod sets;
pub mod signing;
pub mod studio;
pub mod sync;
#[cfg(test)]
mod testutil;

pub use client::{
    ContentClient, ContentSnapshot, GITHUB_MIRROR, GcReport, MediaPrefetch, PRIMARY_MIRROR,
};
pub use error::{Error, Result};
pub use feed::{
    FeedClient, FeedMediaReport, FeedSettings, FeedSnapshot, HomeEvent, HomeNewsItem, MirrorProbe,
    NewsOrigin, card_order, feed_age_floor, home_status, merge_events, merge_minecraft, merge_news,
    newest_patch_notes,
};
pub use glob::glob_match;
pub use images::{LAUNCHER_IMAGE_HOSTS, is_launcher_image_url};
pub use items::{
    AdoptedChange, FABRIC_API_PROJECT, ItemResolution, ItemStatus, LauncherItems, ManualChange,
    OVERLAY_ITEM, PackSelection, is_fabric_api, resolve_items, resolve_items_with,
};
pub use model::*;
pub use modsets::{
    AddedSet, MOD_SETS_FILE, ModSetStates, SetApplyReport, SetChange, apply_set, keep_shared_files,
    plan_set, plan_set_removal, provided_projects,
};
pub use needs::{HIDDEN_NEEDS, HiddenNeed, hidden_needs};
pub use sets::{ParkedSet, SetInfo, SwitchJournal, SwitchRequest, SwitchStep, VersionSets};
pub use signing::{
    CONTENT_PUBLIC_KEYS, SignatureFile, SigningKey, TrustedKey, VerifyingKey, content_public_keys,
    decode_signing_key, decode_verifying_key, decode_verifying_keys, encode_signing_key,
    encode_verifying_key, generate_signing_key, is_content_public_key, key_id, sign_index,
    verify_index,
};
pub use studio::{BuiltIndex, IndexBuilder, PublishedDoc};
pub use sync::{
    COPIED_DIRS, CategoryCounts, DownloadAction, DownloadEstimate, FileCategory, InstalledFile,
    KeepReason, KeptFile, PARKED_DIRS, PackState, PlannedDelete, PlannedDownload, PlannedRestore,
    SyncIssue, SyncIssueKind, SyncPhase, SyncPlan, SyncPolicy, SyncProgress, SyncProgressFn,
    SyncReport, SyncSummary, VersionSwitch, apply_sync, apply_sync_into, estimate_downloads,
    plan_sync,
};

#[cfg(test)]
mod doc_examples {
    //! Every JSON example in `docs/content-format.md` preceded by `<!-- example: Type -->`
    //! must deserialize into `Type`, survive a round trip, and contain no field the type does
    //! not know (catches typos in the spec).

    use serde::Serialize;
    use serde::de::DeserializeOwned;
    use serde_json::Value;

    use super::*;

    const SPEC: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/content-format.md"
    ));

    fn examples() -> Vec<(String, String)> {
        let mut found = Vec::new();
        let mut lines = SPEC.lines();
        while let Some(line) = lines.next() {
            let Some(name) = line
                .trim()
                .strip_prefix("<!-- example: ")
                .and_then(|rest| rest.strip_suffix(" -->"))
            else {
                continue;
            };
            let fence = lines.next().unwrap_or_default();
            assert!(
                fence.starts_with("```json"),
                "example {name} must be a json block"
            );
            let body: Vec<&str> = lines
                .by_ref()
                .take_while(|l| !l.starts_with("```"))
                .collect();
            found.push((name.to_owned(), body.join("\n")));
        }
        found
    }

    /// Every key present in `example` must survive serialization (unknown keys are dropped
    /// by serde, so a typo would disappear here).
    fn assert_known_fields(example: &Value, reserialized: &Value, path: &str) {
        match (example, reserialized) {
            (Value::Object(expected), Value::Object(actual)) => {
                for (key, value) in expected {
                    let Some(other) = actual.get(key) else {
                        panic!("{path}.{key} is not a known field");
                    };
                    assert_known_fields(value, other, &format!("{path}.{key}"));
                }
            }
            (Value::Array(expected), Value::Array(actual)) => {
                assert_eq!(expected.len(), actual.len(), "{path}");
                for (i, (a, b)) in expected.iter().zip(actual).enumerate() {
                    assert_known_fields(a, b, &format!("{path}[{i}]"));
                }
            }
            (expected, actual) => assert_eq!(expected, actual, "{path}"),
        }
    }

    fn check<T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug>(
        name: &str,
        json: &str,
    ) {
        let example: Value =
            serde_json::from_str(json).unwrap_or_else(|e| panic!("{name}: invalid JSON: {e}"));
        let parsed: T =
            serde_json::from_value(example.clone()).unwrap_or_else(|e| panic!("{name}: {e}"));
        let reserialized = serde_json::to_value(&parsed).unwrap();
        assert_known_fields(&example, &reserialized, name);
        let again: T = serde_json::from_value(reserialized).unwrap();
        assert_eq!(parsed, again, "{name} round trip");
    }

    #[test]
    fn spec_examples_round_trip() {
        let examples = examples();
        assert!(
            examples.len() >= 8,
            "found only {} examples",
            examples.len()
        );
        for (name, json) in &examples {
            match name.as_str() {
                "RootIndex" => {
                    check::<RootIndex>(name, json);
                    let index: RootIndex = serde_json::from_str(json).unwrap();
                    index.validate().unwrap_or_else(|e| panic!("{name}: {e}"));
                }
                "SignatureFile" => check::<SignatureFile>(name, json),
                "LauncherConfig" => {
                    check::<LauncherConfig>(name, json);
                    let config: LauncherConfig = serde_json::from_str(json).unwrap();
                    if let Some(feed) = &config.news_feed {
                        assert!(feed.problems().is_empty(), "{name}: {:?}", feed.problems());
                    }
                    if let Some(version) = &config.recommended_minecraft_version {
                        assert!(is_valid_minecraft_version(version), "{name}: {version:?}");
                    }
                }
                "NewsFeed" => check::<NewsFeed>(name, json),
                "BotFeed" => {
                    check::<BotFeed>(name, json);
                    let (_, problems) =
                        BotFeed::parse(json.as_bytes()).unwrap_or_else(|e| panic!("{name}: {e}"));
                    assert!(problems.is_empty(), "{name}: {problems:?}");
                }
                "PackManifest" => {
                    check::<PackManifest>(name, json);
                    let manifest: PackManifest = serde_json::from_str(json).unwrap();
                    manifest
                        .validate()
                        .unwrap_or_else(|e| panic!("{name}: {e}"));
                }
                "ReleaseChannel" => check::<ReleaseChannel>(name, json),
                "ModSetCatalog" => {
                    check::<ModSetCatalog>(name, json);
                    let catalog: ModSetCatalog = serde_json::from_str(json).unwrap();
                    catalog.validate().unwrap_or_else(|e| panic!("{name}: {e}"));
                }
                "ItemArt" => {
                    check::<ItemArt>(name, json);
                    let art: ItemArt = serde_json::from_str(json).unwrap();
                    art.validate().unwrap_or_else(|e| panic!("{name}: {e}"));
                }
                "ChatEmojis" => {
                    check::<ChatEmojis>(name, json);
                    let emojis: ChatEmojis = serde_json::from_str(json).unwrap();
                    emojis.validate().unwrap_or_else(|e| panic!("{name}: {e}"));
                }
                "OverlayRelease" => {
                    check::<OverlayRelease>(name, json);
                    let overlay: OverlayRelease = serde_json::from_str(json).unwrap();
                    assert!(
                        overlay.problems().is_empty(),
                        "{name}: {:?}",
                        overlay.problems()
                    );
                }
                "OverlayCiRelease" => {
                    check::<OverlayCiRelease>(name, json);
                    let report: OverlayCiRelease = serde_json::from_str(json).unwrap();
                    assert!(report.jars.iter().all(|jar| jar.tests.passed()), "{name}");
                }
                "ModSetStates" => check::<ModSetStates>(name, json),
                "PackState" => check::<PackState>(name, json),
                "MediaRef" => check::<MediaRef>(name, json),
                other => panic!("unknown example type {other}"),
            }
        }
    }
}

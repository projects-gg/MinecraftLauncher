//! Signed content index, news and home configuration models, modpack sync.
//!
//! Shared by the launcher, Projects Studio and (through the JSON formats specified in
//! `docs/content-format.md`) the Android app.
//!
//! - [`model`]: wire formats (root index, launcher config, news, pack manifest, release
//!   channel) with forward-compatible defaults.
//! - [`signing`]: Ed25519 keys, `index.json.sig` signing and verification.
//! - [`ContentClient`]: mirror fallback, signature + rollback checks, document and media
//!   cache, offline snapshots, cache garbage collection.
//! - [`sync`]: modpack diff ([`plan_sync`]) and application ([`apply_sync`]) with ownership
//!   tracking, config policies, optional groups and path safety.
//! - [`studio`]: [`IndexBuilder`] and helpers to publish content; [`json`] gives the
//!   deterministic serialization Studio uploads.

pub mod client;
pub mod error;
mod fsutil;
mod glob;
pub mod json;
pub mod model;
pub mod path;
pub mod signing;
pub mod studio;
pub mod sync;
#[cfg(test)]
mod testutil;

pub use client::{
    ContentClient, ContentSnapshot, GITHUB_MIRROR, GcReport, MediaPrefetch, PRIMARY_MIRROR,
};
pub use error::{Error, Result};
pub use glob::glob_match;
pub use model::*;
pub use signing::{
    SignatureFile, SigningKey, VerifyingKey, decode_signing_key, decode_verifying_key,
    decode_verifying_keys, encode_signing_key, encode_verifying_key, generate_signing_key, key_id,
    sign_index, verify_index,
};
pub use studio::{BuiltIndex, IndexBuilder, PublishedDoc};
pub use sync::{
    CategoryCounts, DownloadAction, FileCategory, InstalledFile, KeepReason, KeptFile, PackState,
    PlannedDelete, PlannedDownload, SyncPhase, SyncPlan, SyncPolicy, SyncProgress, SyncProgressFn,
    SyncReport, SyncSummary, apply_sync, plan_sync,
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
                "RootIndex" => check::<RootIndex>(name, json),
                "SignatureFile" => check::<SignatureFile>(name, json),
                "LauncherConfig" => check::<LauncherConfig>(name, json),
                "NewsFeed" => check::<NewsFeed>(name, json),
                "PackManifest" => check::<PackManifest>(name, json),
                "ReleaseChannel" => check::<ReleaseChannel>(name, json),
                "PackState" => check::<PackState>(name, json),
                "MediaRef" => check::<MediaRef>(name, json),
                other => panic!("unknown example type {other}"),
            }
        }
    }
}

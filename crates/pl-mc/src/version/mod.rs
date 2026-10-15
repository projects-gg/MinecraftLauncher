//! Minecraft versions: the Mojang version manifest, the version JSON model (vanilla and loader
//! profiles), rule evaluation, Maven coordinates and `inheritsFrom` resolution.
//!
//! Typical flow: [`load_manifest`] → [`resolve_version`] → `install::install_version`.

pub mod manifest;
pub mod maven;
pub mod model;
pub mod resolve;
pub mod rules;

pub use manifest::{
    LatestVersions, MANIFEST_CACHE_FILE, ManifestVersion, VERSION_MANIFEST_URL, VersionManifest,
    VersionType, cached_manifest, compare_newest_first, fetch_manifest, load_manifest,
};
pub use maven::{MAVEN_CENTRAL_URL, MOJANG_LIBRARIES_URL, MavenCoord};
pub use model::{
    Argument, ArgumentValue, Arguments, Artifact, AssetIndexInfo, ExtractRules, JavaVersionInfo,
    Library, LibraryArtifact, LibraryDownloads, LoggingConfig, LoggingFile, LoggingInfo, OsRule,
    Rule, RuleAction, VersionJson, maven_urls, parse_timestamp,
};
pub use resolve::{
    ResolvedVersion, component_major, fetch_version_json, load_version_json, merge_pair,
    read_local_version, resolve_version, resolve_with,
};
pub use rules::{
    Features, evaluate_arguments, os_rule_matches, referenced_features, rule_matches, rules_allow,
};

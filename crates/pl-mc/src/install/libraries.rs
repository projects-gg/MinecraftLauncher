//! Library planning: which jars are downloaded, which go on the classpath and which natives jars
//! are extracted, for one platform.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use pl_net::DownloadItem;

use crate::error::Result;
use crate::paths::DataDirs;
use crate::platform::Platform;
use crate::version::{LibraryArtifact, ResolvedVersion};

/// A natives jar to unpack into the natives directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeJar {
    pub jar: PathBuf,
    /// Entry name prefixes to skip (always contains `META-INF/`).
    pub excludes: Vec<String>,
    /// 1.19+ `:natives-*` jars keep libraries under `<os>/<arch>/...`: only the native library
    /// files are extracted, flat, into the natives directory. Legacy jars keep their layout.
    pub flatten: bool,
}

#[derive(Debug, Clone, Default)]
pub struct LibraryPlan {
    /// Classpath jars in order (the client jar is appended by the installer).
    pub classpath: Vec<PathBuf>,
    pub downloads: Vec<DownloadItem>,
    pub natives: Vec<NativeJar>,
}

/// Work out the libraries of `resolved` for `platform`.
///
/// Like the official launcher, every `:natives-*` jar allowed by the rules stays on the classpath
/// (LWJGL picks the right one at runtime); the one matching the platform architecture is also
/// extracted so `java.library.path` works for code that loads natives directly.
pub fn plan_libraries(
    dirs: &DataDirs,
    resolved: &ResolvedVersion,
    platform: &Platform,
) -> Result<LibraryPlan> {
    let mut plan = LibraryPlan::default();
    let mut on_classpath = HashSet::new();
    let mut extracted = HashSet::new();
    for library in &resolved.libraries {
        if !library.applies_to(platform) {
            continue;
        }
        if let Some(artifact) = library.artifact() {
            let path = dirs.library_file(&artifact.path)?;
            plan.downloads.push(download_item(&artifact, &path));
            if let Some(classifier) = library.natives_classifier_in_name()
                && platform.natives_classifier_matches(&classifier)
                && extracted.insert(path.clone())
            {
                plan.natives.push(NativeJar {
                    jar: path.clone(),
                    excludes: library.extract_excludes(),
                    flatten: true,
                });
            }
            if on_classpath.insert(path.clone()) {
                plan.classpath.push(path);
            }
        }
        if let Some(native) = library.native_artifact(platform) {
            let path = dirs.library_file(&native.path)?;
            plan.downloads.push(download_item(&native, &path));
            if extracted.insert(path.clone()) {
                plan.natives.push(NativeJar {
                    jar: path,
                    excludes: library.extract_excludes(),
                    flatten: false,
                });
            }
        }
    }
    Ok(plan)
}

fn download_item(artifact: &LibraryArtifact, path: &Path) -> DownloadItem {
    DownloadItem::with_urls(artifact.urls.clone(), path)
        .checksum(artifact.checksum.clone())
        .size(artifact.size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{Arch, Os};
    use crate::version::{VersionJson, resolve_with};

    fn resolved(libraries: &str) -> ResolvedVersion {
        let json: VersionJson = serde_json::from_str(&format!(
            r#"{{"id": "t", "mainClass": "M", "libraries": {libraries}}}"#
        ))
        .unwrap();
        resolve_with("t", |_| Ok(json.clone())).unwrap()
    }

    #[test]
    fn modern_natives_stay_on_classpath_and_matching_one_is_extracted() {
        let r = resolved(
            r#"[
            {"name": "org.lwjgl:lwjgl:3.3.3", "downloads": {"artifact": {"path": "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3.jar", "url": "https://l/a.jar", "sha1": "a", "size": 1}}},
            {"name": "org.lwjgl:lwjgl:3.3.3:natives-windows", "rules": [{"action": "allow", "os": {"name": "windows"}}], "downloads": {"artifact": {"path": "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar", "url": "https://l/b.jar", "sha1": "b", "size": 2}}},
            {"name": "org.lwjgl:lwjgl:3.3.3:natives-windows-arm64", "rules": [{"action": "allow", "os": {"name": "windows"}}], "downloads": {"artifact": {"path": "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows-arm64.jar", "url": "https://l/c.jar", "sha1": "c", "size": 3}}},
            {"name": "org.lwjgl:lwjgl:3.3.3:natives-linux", "rules": [{"action": "allow", "os": {"name": "linux"}}], "downloads": {"artifact": {"path": "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-linux.jar", "url": "https://l/d.jar", "sha1": "d", "size": 4}}}
        ]"#,
        );
        let dirs = DataDirs::new("/data");
        let plan =
            plan_libraries(&dirs, &r, &Platform::new(Os::Windows, Arch::X86_64, "10.0")).unwrap();
        assert_eq!(plan.classpath.len(), 3);
        assert_eq!(plan.downloads.len(), 3);
        assert_eq!(plan.natives.len(), 1);
        assert!(plan.natives[0].flatten);
        assert!(
            plan.natives[0]
                .jar
                .ends_with("lwjgl-3.3.3-natives-windows.jar")
        );

        let arm = plan_libraries(
            &dirs,
            &r,
            &Platform::new(Os::Windows, Arch::Aarch64, "10.0"),
        )
        .unwrap();
        assert!(
            arm.natives[0]
                .jar
                .ends_with("lwjgl-3.3.3-natives-windows-arm64.jar")
        );
    }

    #[test]
    fn legacy_natives_are_extracted_not_on_classpath() {
        let r = resolved(
            r#"[
            {"name": "org.lwjgl.lwjgl:lwjgl-platform:2.9.4-nightly-20150209",
             "natives": {"linux": "natives-linux", "osx": "natives-osx", "windows": "natives-windows"},
             "extract": {"exclude": ["META-INF/"]},
             "downloads": {"classifiers": {
                "natives-windows": {"path": "org/lwjgl/lwjgl/lwjgl-platform/2.9.4-nightly-20150209/lwjgl-platform-2.9.4-nightly-20150209-natives-windows.jar", "url": "https://l/w.jar", "sha1": "w", "size": 5},
                "natives-osx": {"path": "org/lwjgl/lwjgl/lwjgl-platform/2.9.4-nightly-20150209/lwjgl-platform-2.9.4-nightly-20150209-natives-osx.jar", "url": "https://l/o.jar", "sha1": "o", "size": 6}}}},
            {"name": "net.minecraft:launchwrapper:1.12"}
        ]"#,
        );
        let dirs = DataDirs::new("/data");
        let plan =
            plan_libraries(&dirs, &r, &Platform::new(Os::Windows, Arch::X86_64, "10.0")).unwrap();
        assert_eq!(
            plan.classpath.len(),
            1,
            "only launchwrapper is a classpath jar"
        );
        assert_eq!(plan.downloads.len(), 2);
        assert_eq!(plan.natives.len(), 1);
        assert!(!plan.natives[0].flatten);
        assert!(
            plan.natives[0]
                .jar
                .ends_with("lwjgl-platform-2.9.4-nightly-20150209-natives-windows.jar")
        );
        assert_eq!(
            plan.downloads[1].urls[0],
            "https://libraries.minecraft.net/net/minecraft/launchwrapper/1.12/launchwrapper-1.12.jar"
        );
    }
}

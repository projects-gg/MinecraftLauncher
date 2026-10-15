//! Natives extraction for both formats:
//! - legacy `natives` + `downloads.classifiers` jars (LWJGL 2, jinput, twitch, text2speech...):
//!   extracted with their layout, honouring `extract.exclude`;
//! - 1.19+ `:natives-<os>[-<arch>]` jars (LWJGL 3.3+, jtracy): the native libraries are
//!   extracted flat, so they can be found through `java.library.path`.
//!
//! `META-INF/` is always skipped. Files already present with the same size and CRC are left
//! alone, which keeps a running game's locked DLLs untouched.

use std::io::Read;
use std::path::{Path, PathBuf};

use pl_net::{CancellationToken, HashAlgo, hash_bytes};

use crate::error::{Error, Result};
use crate::install::libraries::NativeJar;
use crate::paths::{DataDirs, safe_join, safe_name, write_atomic_blocking};
use crate::platform::Platform;
use crate::version::ResolvedVersion;

/// Folder key under `natives/`: the version id plus a short hash of the natives set and
/// architecture, so a changed library list never fights with a running game over locked files.
pub fn natives_key(
    dirs: &DataDirs,
    resolved: &ResolvedVersion,
    platform: &Platform,
    jars: &[NativeJar],
) -> String {
    let mut fingerprint = format!("{:?}|{:?}|", platform.os, platform.arch);
    if let Some(sub) = library_path_subdir(resolved) {
        fingerprint.push_str(&sub);
    }
    let libraries = dirs.libraries();
    for jar in jars {
        let relative = jar.jar.strip_prefix(&libraries).unwrap_or(&jar.jar);
        fingerprint.push('|');
        fingerprint.push_str(&relative.to_string_lossy().replace('\\', "/"));
        fingerprint.push_str(if jar.flatten { ":flat" } else { ":tree" });
    }
    let hash = hash_bytes(fingerprint.as_bytes(), HashAlgo::Sha1);
    format!("{}-{}", resolved.id, &hash[..10])
}

/// Sub-folder of `${natives_directory}` used as `java.library.path` (`java` for 26.x JSONs whose
/// JVM arguments say `-Djava.library.path=${natives_directory}/java`), if any.
pub fn library_path_subdir(resolved: &ResolvedVersion) -> Option<String> {
    resolved
        .jvm_arguments
        .iter()
        .flat_map(|arg| arg.values())
        .find_map(|value| value.strip_prefix("-Djava.library.path=${natives_directory}"))
        .and_then(clean_subdir)
}

/// Every `${natives_directory}/<sub>` folder referenced by JVM arguments (26.x uses `java`,
/// `jna`, `lwjgl`, `netty`); created up front so no library trips over a missing folder.
pub fn natives_subdirs(resolved: &ResolvedVersion) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for value in resolved.jvm_arguments.iter().flat_map(|arg| arg.values()) {
        let mut rest = value;
        while let Some(pos) = rest.find("${natives_directory}") {
            rest = &rest[pos + "${natives_directory}".len()..];
            let end = rest
                .find(|c: char| c.is_whitespace() || c == ';' || c == ':' || c == '"')
                .unwrap_or(rest.len());
            if let Some(sub) = clean_subdir(&rest[..end])
                && !out.contains(&sub)
            {
                out.push(sub);
            }
        }
    }
    out
}

fn clean_subdir(raw: &str) -> Option<String> {
    let sub = raw
        .trim_start_matches(['/', '\\'])
        .trim_end_matches(['/', '\\']);
    let valid = !sub.is_empty()
        && sub
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && safe_name(sub).is_ok();
    valid.then(|| sub.to_owned())
}

/// Native library file names (`.dll`, `.so`, `.so.1`, `.dylib`, `.jnilib`).
pub fn is_native_library(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let versioned_so = lower.rsplit_once(".so.").is_some_and(|(_, version)| {
        !version.is_empty() && version.chars().all(|c| c.is_ascii_digit() || c == '.')
    });
    lower.ends_with(".dll")
        || lower.ends_with(".so")
        || versioned_so
        || lower.ends_with(".dylib")
        || lower.ends_with(".jnilib")
}

/// Extract `jars` into `natives_dir` (or its `library_subdir` when the version uses one) and
/// create the extra sub-folders. Calls `on_jar(done, total)` after each jar.
pub async fn extract_natives(
    jars: Vec<NativeJar>,
    natives_dir: PathBuf,
    library_subdir: Option<String>,
    extra_subdirs: Vec<String>,
    cancel: CancellationToken,
    on_jar: impl Fn(usize, usize) + Send + 'static,
) -> Result<()> {
    tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&natives_dir).map_err(|e| Error::io(&natives_dir, e))?;
        for sub in &extra_subdirs {
            let dir = natives_dir.join(sub);
            std::fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
        }
        let target = match &library_subdir {
            Some(sub) => natives_dir.join(sub),
            None => natives_dir.clone(),
        };
        std::fs::create_dir_all(&target).map_err(|e| Error::io(&target, e))?;
        let total = jars.len();
        for (index, jar) in jars.iter().enumerate() {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            extract_jar(jar, &target)?;
            on_jar(index + 1, total);
        }
        Ok(())
    })
    .await
    .map_err(|e| Error::archive("natives", format!("extraction task failed: {e}")))?
}

fn extract_jar(jar: &NativeJar, target: &Path) -> Result<()> {
    let file = std::fs::File::open(&jar.jar).map_err(|e| Error::io(&jar.jar, e))?;
    let mut archive = zip::ZipArchive::new(std::io::BufReader::new(file))
        .map_err(|e| Error::archive(&jar.jar, e))?;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| Error::archive(&jar.jar, e))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry
            .name()
            .map_err(|e| Error::archive(&jar.jar, e))?
            .replace('\\', "/");
        if jar
            .excludes
            .iter()
            .any(|prefix| !prefix.is_empty() && name.starts_with(prefix.as_str()))
        {
            continue;
        }
        let dest = if jar.flatten {
            if !is_native_library(&name) {
                continue;
            }
            let file_name = name.rsplit('/').next().unwrap_or(&name);
            target.join(safe_name(file_name)?)
        } else {
            safe_join(target, &name)?
        };
        if same_content(&dest, entry.size(), entry.crc32()) {
            continue;
        }
        let mut bytes = Vec::with_capacity(entry.size().min(64 * 1024 * 1024) as usize);
        entry
            .read_to_end(&mut bytes)
            .map_err(|e| Error::archive(&jar.jar, format!("{name}: {e}")))?;
        write_atomic_blocking(&dest, &bytes)?;
    }
    Ok(())
}

fn same_content(path: &Path, size: u64, crc: u32) -> bool {
    match std::fs::metadata(path) {
        Ok(meta) if meta.is_file() && meta.len() == size => {}
        _ => return false,
    }
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    let mut hasher = flate2::Crc::new();
    hasher.update(&bytes);
    hasher.sum() == crc
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;
    use crate::version::{VersionJson, resolve_with};

    fn write_jar(path: &Path, entries: &[(&str, &[u8])]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        for (name, data) in entries {
            if name.ends_with('/') {
                zip.add_directory(name.trim_end_matches('/'), options)
                    .unwrap();
            } else {
                zip.start_file(*name, options).unwrap();
                zip.write_all(data).unwrap();
            }
        }
        zip.finish().unwrap();
    }

    #[tokio::test]
    async fn extracts_both_formats() {
        let tmp = tempfile::tempdir().unwrap();
        let legacy = tmp.path().join("legacy.jar");
        write_jar(
            &legacy,
            &[
                ("META-INF/MANIFEST.MF", b"x"),
                ("lwjgl64.dll", b"legacy-dll"),
                ("skip/me.txt", b"no"),
                ("sub/", b""),
                ("sub/OpenAL64.dll", b"al"),
            ],
        );
        let modern = tmp.path().join("modern.jar");
        write_jar(
            &modern,
            &[
                ("META-INF/windows/x64/org/lwjgl/lwjgl.dll.sha1", b"hash"),
                ("windows/x64/org/lwjgl/lwjgl.dll", b"modern-dll"),
                ("windows/x64/org/lwjgl/readme.txt", b"no"),
                ("Tracy_LICENSE", b"no"),
            ],
        );
        let natives = tmp.path().join("natives");
        let jars = vec![
            NativeJar {
                jar: legacy,
                excludes: vec!["META-INF/".into(), "skip/".into()],
                flatten: false,
            },
            NativeJar {
                jar: modern,
                excludes: vec!["META-INF/".into()],
                flatten: true,
            },
        ];
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = calls.clone();
        extract_natives(
            jars.clone(),
            natives.clone(),
            None,
            vec!["jna".into()],
            CancellationToken::new(),
            move |_, _| {
                counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            },
        )
        .await
        .unwrap();
        assert_eq!(
            std::fs::read(natives.join("lwjgl64.dll")).unwrap(),
            b"legacy-dll"
        );
        assert_eq!(
            std::fs::read(natives.join("sub/OpenAL64.dll")).unwrap(),
            b"al"
        );
        assert_eq!(
            std::fs::read(natives.join("lwjgl.dll")).unwrap(),
            b"modern-dll"
        );
        assert!(!natives.join("META-INF").exists());
        assert!(!natives.join("skip").exists());
        assert!(!natives.join("readme.txt").exists());
        assert!(!natives.join("Tracy_LICENSE").exists());
        assert!(natives.join("jna").is_dir());
        assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 2);

        // Second run is a no-op (same content) and works into a library sub-folder too.
        extract_natives(
            jars,
            natives.clone(),
            Some("java".into()),
            vec![],
            CancellationToken::new(),
            |_, _| {},
        )
        .await
        .unwrap();
        assert_eq!(
            std::fs::read(natives.join("java/lwjgl.dll")).unwrap(),
            b"modern-dll"
        );
    }

    #[test]
    fn subdirs_from_jvm_arguments() {
        let json: VersionJson = serde_json::from_str(
            r#"{"id": "26.3", "mainClass": "M", "arguments": {"jvm": [
                "-Djava.library.path=${natives_directory}/java",
                "-Djna.tmpdir=${natives_directory}/jna",
                "-Dorg.lwjgl.system.SharedLibraryExtractPath=${natives_directory}/lwjgl",
                "-Dio.netty.native.workdir=${natives_directory}/netty",
                "-cp", "${classpath}"]}}"#,
        )
        .unwrap();
        let r = resolve_with("26.3", |_| Ok(json.clone())).unwrap();
        assert_eq!(library_path_subdir(&r).as_deref(), Some("java"));
        assert_eq!(natives_subdirs(&r), ["java", "jna", "lwjgl", "netty"]);

        let old: VersionJson = serde_json::from_str(
            r#"{"id": "1.21.1", "mainClass": "M", "arguments": {"jvm": ["-Djava.library.path=${natives_directory}", "-Dx=${natives_directory}/../evil"]}}"#,
        )
        .unwrap();
        let r = resolve_with("1.21.1", |_| Ok(old.clone())).unwrap();
        assert_eq!(library_path_subdir(&r), None);
        assert!(natives_subdirs(&r).is_empty());

        let dirs = DataDirs::new("/data");
        let p = Platform::new(
            crate::platform::Os::Windows,
            crate::platform::Arch::X86_64,
            "10.0",
        );
        let key = natives_key(&dirs, &r, &p, &[]);
        assert!(key.starts_with("1.21.1-"));
        assert_eq!(key.len(), "1.21.1-".len() + 10);
        assert_ne!(
            key,
            natives_key(&dirs, &r, &p.with_arch(crate::platform::Arch::Aarch64), &[])
        );
    }

    #[test]
    fn native_names() {
        assert!(is_native_library("windows/x64/org/lwjgl/lwjgl.dll"));
        assert!(is_native_library("liblwjgl.so"));
        assert!(is_native_library("libfoo.so.1"));
        assert!(is_native_library("liblwjgl.dylib"));
        assert!(is_native_library("libjcocoa.jnilib"));
        assert!(!is_native_library("lwjgl.dll.sha1"));
        assert!(!is_native_library("libfoo.so.sha1"));
        assert!(!is_native_library("module-info.class"));
    }
}

//! The check before launching with a custom Java ("Özel yol"): does the chosen Java suit this
//! game, and what "Kur ve oyna" switches the instance to when it does not. The launcher asks the
//! player only when the answer is certain: a Java older than the game (or its mods) needs, a
//! Java 9+ for a LaunchWrapper game, or a path that is gone or does not run.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::java::requirement::{JavaNeed, component_for_major};
use crate::java::{JavaInfo, probe_java};

/// What is known about the custom Java.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CustomJava {
    /// The file is gone (moved, uninstalled, a removed drive).
    Missing,
    /// The file exists but does not answer as a Java.
    Broken,
    Found(JavaInfo),
}

/// The check's answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JavaVerdict {
    Fine,
    Missing,
    Broken,
    TooOld { have: u32, need: u32 },
    TooNew { have: u32, max: u32 },
}

impl JavaVerdict {
    pub fn is_fine(self) -> bool {
        self == Self::Fine
    }

    /// A short stable name (logs, session memory).
    pub fn key(self) -> String {
        match self {
            Self::Fine => "fine".to_owned(),
            Self::Missing => "missing".to_owned(),
            Self::Broken => "broken".to_owned(),
            Self::TooOld { have, need } => format!("old-{have}-{need}"),
            Self::TooNew { have, max } => format!("new-{have}-{max}"),
        }
    }
}

/// Judge the custom Java against what the game needs.
pub fn judge(java: &CustomJava, need: &JavaNeed) -> JavaVerdict {
    match java {
        CustomJava::Missing => JavaVerdict::Missing,
        CustomJava::Broken => JavaVerdict::Broken,
        CustomJava::Found(info) if info.major < need.min => JavaVerdict::TooOld {
            have: info.major,
            need: need.min,
        },
        CustomJava::Found(info) => match need.max {
            Some(max) if info.major > max => JavaVerdict::TooNew {
                have: info.major,
                max,
            },
            _ => JavaVerdict::Fine,
        },
    }
}

/// What "Kur ve oyna" switches the instance to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JavaRemedy {
    /// "Otomatik": the runtime of the game version fits.
    Automatic,
    /// A runtime of another Java version, kept current by the launcher (the mods need a newer
    /// Java than the game version's own runtime).
    Managed { component: &'static str, major: u32 },
}

/// The remedy for `need`; `None` when no Mojang runtime fits (mods asking for a Java that does
/// not exist yet).
pub fn remedy(need: &JavaNeed) -> Option<JavaRemedy> {
    if need.accepts(need.automatic_major()) {
        return Some(JavaRemedy::Automatic);
    }
    let (component, major) = component_for_major(need.min)?;
    need.accepts(major)
        .then_some(JavaRemedy::Managed { component, major })
}

/// "Yine de dene" answers given this session: the same question about the same instance, Java
/// and game is not asked again until the launcher restarts.
#[derive(Debug, Default)]
pub struct PromptMemory {
    answered: HashSet<String>,
}

impl PromptMemory {
    fn key(instance_id: &str, java: &Path, verdict: JavaVerdict) -> String {
        format!("{instance_id}\n{}\n{}", java.display(), verdict.key())
    }

    pub fn remember(&mut self, instance_id: &str, java: &Path, verdict: JavaVerdict) {
        self.answered.insert(Self::key(instance_id, java, verdict));
    }

    pub fn answered(&self, instance_id: &str, java: &Path, verdict: JavaVerdict) -> bool {
        self.answered
            .contains(&Self::key(instance_id, java, verdict))
    }
}

/// A file's identity for caching: size and modification time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileStamp {
    pub len: u64,
    pub modified: Option<SystemTime>,
}

impl FileStamp {
    /// `None` when `path` is not a file.
    pub fn of(path: &Path) -> Option<Self> {
        let meta = std::fs::metadata(path)
            .ok()
            .filter(std::fs::Metadata::is_file)?;
        Some(Self {
            len: meta.len(),
            modified: meta.modified().ok(),
        })
    }
}

/// Probes of Java executables, kept while the file stays the same (an update of that Java
/// changes its size or time and is probed again). `None` results (does not run) are kept too.
#[derive(Debug, Default)]
pub struct ProbeCache {
    entries: HashMap<PathBuf, (FileStamp, Option<JavaInfo>)>,
}

impl ProbeCache {
    /// The cached result for `path` with this stamp: `Some(None)` = known not to run.
    pub fn get(&self, path: &Path, stamp: FileStamp) -> Option<Option<JavaInfo>> {
        self.entries
            .get(path)
            .filter(|(cached, _)| *cached == stamp)
            .map(|(_, info)| info.clone())
    }

    pub fn insert(&mut self, path: &Path, stamp: FileStamp, info: Option<JavaInfo>) {
        self.entries.insert(path.to_path_buf(), (stamp, info));
    }
}

/// The custom Java's state: probed once per file version (cached in `cache`).
pub async fn check_custom_java(path: &Path, cache: &parking_lot::Mutex<ProbeCache>) -> CustomJava {
    let owned = path.to_path_buf();
    let stamp = tokio::task::spawn_blocking(move || FileStamp::of(&owned))
        .await
        .ok()
        .flatten();
    let Some(stamp) = stamp else {
        return CustomJava::Missing;
    };
    if let Some(cached) = cache.lock().get(path, stamp) {
        return cached.map_or(CustomJava::Broken, CustomJava::Found);
    }
    let info = match probe_java(path).await {
        Ok(info) => Some(info),
        Err(error) => {
            tracing::info!(path = %path.display(), %error, "custom Java does not run");
            None
        }
    };
    cache.lock().insert(path, stamp, info.clone());
    info.map_or(CustomJava::Broken, CustomJava::Found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Arch;

    fn found(major: u32) -> CustomJava {
        CustomJava::Found(JavaInfo {
            version: major.to_string(),
            major,
            arch: Arch::X86_64,
            vendor: None,
        })
    }

    #[test]
    fn judges_the_custom_java() {
        let modern = JavaNeed::for_game("java-runtime-delta", 21, false);
        assert_eq!(judge(&found(21), &modern), JavaVerdict::Fine);
        assert_eq!(judge(&found(25), &modern), JavaVerdict::Fine);
        assert_eq!(
            judge(&found(17), &modern),
            JavaVerdict::TooOld { have: 17, need: 21 }
        );
        assert_eq!(
            judge(
                &found(8),
                &JavaNeed::for_game("java-runtime-epsilon", 25, false)
            ),
            JavaVerdict::TooOld { have: 8, need: 25 }
        );
        // LaunchWrapper: anything above 8 is too new; plain Java 8 games run on newer Java.
        let legacy = JavaNeed::for_game("jre-legacy", 8, true);
        assert_eq!(judge(&found(8), &legacy), JavaVerdict::Fine);
        assert_eq!(
            judge(&found(17), &legacy),
            JavaVerdict::TooNew { have: 17, max: 8 }
        );
        assert_eq!(
            judge(&found(17), &JavaNeed::for_game("jre-legacy", 8, false)),
            JavaVerdict::Fine
        );
        // The mods' minimum counts.
        let mods = JavaNeed::for_game("java-runtime-gamma", 17, false).with_mods(Some(21));
        assert_eq!(
            judge(&found(17), &mods),
            JavaVerdict::TooOld { have: 17, need: 21 }
        );
        assert_eq!(judge(&CustomJava::Missing, &modern), JavaVerdict::Missing);
        assert_eq!(judge(&CustomJava::Broken, &modern), JavaVerdict::Broken);
    }

    #[test]
    fn remedies() {
        // The game's own runtime fits: "Otomatik".
        let modern = JavaNeed::for_game("java-runtime-delta", 21, false);
        assert_eq!(remedy(&modern), Some(JavaRemedy::Automatic));
        let legacy = JavaNeed::for_game("jre-legacy", 8, true);
        assert_eq!(remedy(&legacy), Some(JavaRemedy::Automatic));
        // Mods need more than the game's runtime: the smallest runtime that fits.
        let mods = JavaNeed::for_game("java-runtime-gamma", 17, false).with_mods(Some(21));
        assert_eq!(
            remedy(&mods),
            Some(JavaRemedy::Managed {
                component: "java-runtime-delta",
                major: 21
            })
        );
        let newest = JavaNeed::for_game("java-runtime-delta", 21, false).with_mods(Some(24));
        assert_eq!(
            remedy(&newest),
            Some(JavaRemedy::Managed {
                component: "java-runtime-epsilon",
                major: 25
            })
        );
        // Nothing fits: a LaunchWrapper game whose mods want Java 17, mods wanting Java 99.
        assert_eq!(remedy(&legacy.clone().with_mods(Some(17))), None);
        assert_eq!(remedy(&modern.with_mods(Some(99))), None);
    }

    #[test]
    fn remembers_answers_for_the_session() {
        let mut memory = PromptMemory::default();
        let java = Path::new("C:/Java/jdk-17/bin/javaw.exe");
        let old = JavaVerdict::TooOld { have: 17, need: 21 };
        assert!(!memory.answered("survival", java, old));
        memory.remember("survival", java, old);
        assert!(memory.answered("survival", java, old));
        // Another instance, another Java or another need is a new question.
        assert!(!memory.answered("creative", java, old));
        assert!(!memory.answered("survival", Path::new("C:/other/java.exe"), old));
        assert!(!memory.answered("survival", java, JavaVerdict::TooOld { have: 17, need: 25 }));
    }

    #[tokio::test]
    async fn probes_are_cached_per_file_version() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = parking_lot::Mutex::new(ProbeCache::default());
        let missing = tmp.path().join("gone.exe");
        assert_eq!(
            check_custom_java(&missing, &cache).await,
            CustomJava::Missing
        );

        // Not a Java: probed once, then answered from the cache while the file is unchanged.
        let fake = tmp.path().join("java.exe");
        std::fs::write(&fake, b"not a program").unwrap();
        assert_eq!(check_custom_java(&fake, &cache).await, CustomJava::Broken);
        let stamp = FileStamp::of(&fake).unwrap();
        assert_eq!(cache.lock().get(&fake, stamp), Some(None));

        // A cached answer for this exact file version wins over probing.
        let info = JavaInfo {
            version: "21.0.7".into(),
            major: 21,
            arch: Arch::X86_64,
            vendor: None,
        };
        cache.lock().insert(&fake, stamp, Some(info.clone()));
        assert_eq!(
            check_custom_java(&fake, &cache).await,
            CustomJava::Found(info)
        );

        // A changed file is probed again.
        std::fs::write(&fake, b"still not a program, but longer").unwrap();
        let changed = FileStamp::of(&fake).unwrap();
        assert_ne!(changed, stamp);
        assert_eq!(cache.lock().get(&fake, changed), None);
        assert_eq!(check_custom_java(&fake, &cache).await, CustomJava::Broken);
    }
}

//! Crash analysis: turns the end of the game output and the crash report into a diagnosis with
//! a plain Turkish explanation and one-click suggestions. Ported from the legacy launcher's
//! `GameProcessWatcher` heuristics (memory, Java version, graphics driver, mod errors, access
//! denied, silent exits) and extended for mod loader dependency problems and Java mismatches.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::{Duration, SystemTime};

use regex::Regex;
use serde::{Deserialize, Serialize};

/// Exits sooner than this count as failed when a crash report was written, even with code 0.
pub const SHORT_RUN: Duration = Duration::from_secs(20);
/// Crash reports older than the launch minus this margin belong to an earlier run.
const CRASH_SCAN_MARGIN: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum CrashKind {
    /// The JVM could not reserve the requested heap.
    MemoryReserve,
    OutOfMemory,
    /// A user-supplied JVM option was rejected.
    InvalidJvmArgument,
    JavaTooOld,
    JavaTooNew,
    GraphicsDriver,
    IncompatibleMods,
    MissingDependency,
    DuplicateMods,
    /// Mixin failures and other mod crashes.
    ModError,
    CorruptedFiles,
    AccessDenied,
    /// Exited without printing anything (often security software).
    NoOutput,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SuggestedAction {
    LowerMaxMemory {
        suggested_mb: Option<u32>,
    },
    IncreaseMaxMemory {
        suggested_mb: Option<u32>,
    },
    ResetJvmArguments,
    /// Switch the instance to automatic Java (downloads the right runtime).
    UseAutomaticJava {
        required_major: Option<u32>,
    },
    UpdateGraphicsDriver,
    TryWithoutMods,
    InstallMods {
        mods: Vec<String>,
    },
    UpdateMods {
        mods: Vec<String>,
    },
    RemoveMods {
        mods: Vec<String>,
    },
    RemoveDuplicateMods {
        mods: Vec<String>,
    },
    AddAntivirusException,
    RepairInstallation,
    OpenCrashReport,
    OpenLogs,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CrashDiagnosis {
    pub kind: CrashKind,
    pub title_tr: String,
    pub hint_tr: String,
    pub suggested_actions: Vec<SuggestedAction>,
    /// Mods named by the error (display names when the loader gives them).
    pub mods: Vec<String>,
    pub exit_code: Option<i32>,
    pub runtime_secs: u64,
    /// The game's crash report (or the JVM's `hs_err_pid*.log`) written during this run.
    pub crash_report: Option<PathBuf>,
    /// Captured output of this run.
    pub log_file: Option<PathBuf>,
}

/// What the analyzer looks at.
#[derive(Debug, Clone, Default)]
pub struct CrashInput<'a> {
    /// The last lines of the game output.
    pub output: &'a str,
    /// Crash report / JVM error log text, when one was written.
    pub crash_report: Option<&'a str>,
    pub exit_code: Option<i32>,
    pub runtime: Duration,
    /// Major version of the Java that ran the game.
    pub java_major: Option<u32>,
    /// Major version the Minecraft version asks for.
    pub required_java: Option<u32>,
    pub max_memory_mb: Option<u32>,
}

/// The legacy rule: a non-zero exit, or a short run that left a crash report.
pub fn is_failure(exit_code: Option<i32>, runtime: Duration, has_crash_report: bool) -> bool {
    exit_code != Some(0) || (runtime < SHORT_RUN && has_crash_report)
}

/// Newest `crash-reports/crash-*.txt` written since `since` (minus a small margin).
pub fn find_crash_report(game_dir: &Path, since: SystemTime) -> Option<PathBuf> {
    newest_file(&game_dir.join("crash-reports"), since, |name| {
        name.starts_with("crash-") && name.ends_with(".txt")
    })
}

/// Newest JVM fatal error log (`hs_err_pid*.log`) written in the game directory since `since`.
pub fn find_jvm_error_log(game_dir: &Path, since: SystemTime) -> Option<PathBuf> {
    newest_file(game_dir, since, |name| {
        name.starts_with("hs_err_pid") && name.ends_with(".log")
    })
}

fn newest_file(dir: &Path, since: SystemTime, accept: impl Fn(&str) -> bool) -> Option<PathBuf> {
    let from = since.checked_sub(CRASH_SCAN_MARGIN).unwrap_or(since);
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|entry| accept(&entry.file_name().to_string_lossy()))
        .filter_map(|entry| {
            let modified = entry.metadata().ok()?.modified().ok()?;
            (modified >= from).then(|| (modified, entry.path()))
        })
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path)
}

static CLASS_FILE_VERSION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"class file version (\d+)").expect("valid regex"));
static FABRIC_JAVA: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)requires version (\d+)(?: or later)? of (?:mod 'Java' \()?java")
        .expect("valid regex")
});
static FABRIC_MOD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i:mod) '([^']+)' \(([\w.\-]+)\)").expect("valid regex"));
static FABRIC_MISSING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)requires (?:any version|version [^,]+?) of (?:mod '([^']+)' \(([\w.\-]+)\)|([\w.\-]+)),? which is missing")
        .expect("valid regex")
});
static FABRIC_INSTALL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^\s*- Install ([\w.\-]+),").expect("valid regex"));
static FORGE_DEPENDENCY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"Mod ID: '([^']+)', Requested by: '([^']+)'(?:.*?Actual version: '([^']+)')?")
        .expect("valid regex")
});
static FML_MISSING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"Mod ([\w.\-]+) \(([^)]+)\) requires \[([\w.\-]+)").expect("valid regex")
});
static DUPLICATE_ID: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:Mod ID: '([^']+)' from mod files|duplicate mod(?: id)?:? '?([\w.\-]+)|same id:? '?([\w.\-]+))")
        .expect("valid regex")
});
static MIXIN_MOD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:Mixin apply for mod ([\w.\-]+) failed|from mod ([\w.\-]+)|Mixin \[([\w\-]+)\.mixins\.json|in config \[([\w\-]+)\.mixins\.json\]|Caught exception from ([^(\n]+?) \()")
        .expect("valid regex")
});

/// Classify a failed run.
pub fn analyze_crash(input: &CrashInput<'_>) -> CrashDiagnosis {
    let mut text = String::with_capacity(
        input.output.len() + input.crash_report.map(str::len).unwrap_or_default() + 1,
    );
    text.push_str(input.output);
    text.push('\n');
    if let Some(report) = input.crash_report {
        text.push_str(report);
    }
    let lower = text.to_lowercase();
    let has = |needle: &str| lower.contains(needle);
    let any = |needles: &[&str]| needles.iter().any(|n| lower.contains(n));

    let (kind, mods, required) = if any(&[
        "unsupportedclassversionerror",
        "has been compiled by a more recent version of the java",
        "of java, but only the wrong version is present",
    ]) || (has("unrecognized option: --")
        && input.java_major.is_none_or(|major| major < 9))
        || FABRIC_JAVA.is_match(&text)
    {
        let required = CLASS_FILE_VERSION
            .captures(&text)
            .and_then(|c| c[1].parse::<u32>().ok())
            .filter(|v| *v > 44)
            .map(|v| v - 44)
            .or_else(|| FABRIC_JAVA.captures(&text).and_then(|c| c[1].parse().ok()))
            .or(input.required_java);
        (CrashKind::JavaTooOld, Vec::new(), required)
    } else if any(&[
        "cannot be cast to class java.net.urlclassloader",
        "cannot be cast to java.net.urlclassloader",
        "the security manager is deprecated",
        "does not \"opens java.",
        "module java.base does not \"opens",
    ]) {
        (CrashKind::JavaTooNew, Vec::new(), input.required_java)
    } else if any(&[
        "unrecognized vm option",
        "unrecognized option:",
        "improperly specified vm option",
        "invalid initial heap size",
        "could not find agent library",
    ]) {
        (CrashKind::InvalidJvmArgument, Vec::new(), None)
    } else if any(&[
        "could not reserve enough space for object heap",
        "invalid maximum heap size",
        "initial heap size set to a larger value than the maximum heap size",
        "insufficient memory for the java runtime environment",
        "the specified size exceeds the maximum representable size",
    ]) {
        (CrashKind::MemoryReserve, Vec::new(), None)
    } else if has("outofmemoryerror") {
        (CrashKind::OutOfMemory, Vec::new(), None)
    } else if any(&[
        "found duplicate mods",
        "duplicatemodsfoundexception",
        "duplicate mods found",
        "duplicate mod id",
        "mods with the same id",
        "is duplicated",
    ]) {
        let mut mods: Vec<String> = DUPLICATE_ID
            .captures_iter(&text)
            .filter_map(|c| c.get(1).or_else(|| c.get(2)).or_else(|| c.get(3)))
            .map(|m| m.as_str().to_owned())
            .collect();
        if mods.is_empty() {
            mods = fabric_mod_names(&text);
        }
        (CrashKind::DuplicateMods, mods, None)
    } else if let Some(missing) = missing_dependencies(&text, &lower) {
        (CrashKind::MissingDependency, missing, None)
    } else if any(&[
        "incompatible mods found",
        "incompatible mod set",
        "some of your mods are incompatible",
        "but only the wrong version is present",
        "is incompatible with",
        "missing or unsupported mandatory dependencies",
        "missingmodsexception",
    ]) {
        let mut mods = fabric_mod_names(&text);
        mods.extend(
            FORGE_DEPENDENCY
                .captures_iter(&text)
                .filter_map(|c| c.get(2).map(|m| m.as_str().to_owned())),
        );
        (CrashKind::IncompatibleMods, mods, None)
    } else if any(&[
        "does not support required extensions",
        "does not have required feature",
        "can't getdevice() before it was initialized",
        "failed to create window",
        "pixel format not accelerated",
        "couldn't set pixel format",
        "no opengl context found",
        "the driver does not appear to support opengl",
        "glfw error 65542",
        "glfw error 65543",
        "glfw error 65545",
        "exception_access_violation",
        "atio6axx.dll",
        "atig6txx.dll",
        "ig9icd64.dll",
        "ig75icd64.dll",
        "nvoglv64.dll",
    ]) || input.exit_code == Some(-1073741819)
    {
        (CrashKind::GraphicsDriver, Vec::new(), None)
    } else if any(&[
        "mixin apply failed",
        "mixinapplyerror",
        "mixin prepare failed",
        "mixintransformererror",
        "invalidinjectionexception",
        "net.fabricmc.loader.impl.formattedexception",
        "modloadingexception",
        "loadingfailedexception",
        "caught exception from",
    ]) {
        let mods = MIXIN_MOD
            .captures_iter(&text)
            .filter_map(|c| (1..=5).find_map(|i| c.get(i)))
            .map(|m| m.as_str().trim().to_owned())
            .collect();
        (CrashKind::ModError, mods, None)
    } else if any(&[
        "java.util.zip.zipexception",
        "zip end header not found",
        "invalid loc header",
        "error in opening zip file",
        "could not find or load main class",
        "classnotfoundexception: net.minecraft.client",
    ]) {
        (CrashKind::CorruptedFiles, Vec::new(), None)
    } else if any(&[
        "access is denied",
        "accessdeniedexception",
        "unauthorizedaccessexception",
        "erişim engellendi",
        "operation not permitted",
    ]) {
        (CrashKind::AccessDenied, Vec::new(), None)
    } else if any(&[
        "error occurred during initialization of vm",
        "could not create the java virtual machine",
    ]) {
        (CrashKind::MemoryReserve, Vec::new(), None)
    } else if input.output.trim().is_empty() && input.crash_report.is_none() {
        (CrashKind::NoOutput, Vec::new(), None)
    } else {
        (CrashKind::Unknown, Vec::new(), None)
    };

    let mods = dedup_names(mods);
    let (title_tr, hint_tr, mut suggested_actions) = describe(kind, &mods, required, input);
    if input.crash_report.is_some() {
        suggested_actions.push(SuggestedAction::OpenCrashReport);
    }
    suggested_actions.push(SuggestedAction::OpenLogs);
    CrashDiagnosis {
        kind,
        title_tr,
        hint_tr,
        suggested_actions,
        mods,
        exit_code: input.exit_code,
        runtime_secs: input.runtime.as_secs(),
        crash_report: None,
        log_file: None,
    }
}

fn fabric_mod_names(text: &str) -> Vec<String> {
    FABRIC_MOD
        .captures_iter(text)
        .map(|c| c[1].to_owned())
        .filter(|name| {
            !name.eq_ignore_ascii_case("minecraft") && !name.eq_ignore_ascii_case("java")
        })
        .collect()
}

/// Mods reported as missing by Fabric/Quilt, Forge 1.13+ or legacy FML.
fn missing_dependencies(text: &str, lower: &str) -> Option<Vec<String>> {
    let mut missing: Vec<String> = Vec::new();
    for c in FABRIC_MISSING.captures_iter(text) {
        if let Some(name) = c.get(1).or_else(|| c.get(3)) {
            missing.push(name.as_str().to_owned());
        }
    }
    missing.extend(FABRIC_INSTALL.captures_iter(text).map(|c| c[1].to_owned()));
    for c in FORGE_DEPENDENCY.captures_iter(text) {
        if c.get(3)
            .is_some_and(|actual| actual.as_str() == "[MISSING]")
        {
            missing.push(c[1].to_owned());
        }
    }
    if lower.contains("missingmodsexception") || lower.contains("missing mods") {
        missing.extend(FML_MISSING.captures_iter(text).map(|c| c[3].to_owned()));
    }
    let found = !missing.is_empty()
        || lower.contains("which is missing")
        || lower.contains("could not find required mod");
    found.then_some(missing)
}

fn dedup_names(names: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for name in names {
        let name = name.trim().trim_matches('\'').to_owned();
        if !name.is_empty() && !out.iter().any(|n| n.eq_ignore_ascii_case(&name)) {
            out.push(name);
        }
        if out.len() >= 8 {
            break;
        }
    }
    out
}

fn list(mods: &[String]) -> String {
    mods.join(", ")
}

fn round_mb(value: u32) -> u32 {
    (value / 256).max(4) * 256
}

/// Turkish title, hint and actions for a kind.
fn describe(
    kind: CrashKind,
    mods: &[String],
    required: Option<u32>,
    input: &CrashInput<'_>,
) -> (String, String, Vec<SuggestedAction>) {
    let s = |t: &str| t.to_owned();
    match kind {
        CrashKind::MemoryReserve => (
            s("Ayrılan bellek çok yüksek"),
            s(
                "Oyuna ayrılan en yüksek bellek, bilgisayarınızın şu anda verebileceğinden fazla. \
               Kurulum ayarlarından en yüksek bellek değerini düşürüp yeniden deneyin.",
            ),
            vec![SuggestedAction::LowerMaxMemory {
                suggested_mb: input.max_memory_mb.map(|m| round_mb(m / 2).max(1024)),
            }],
        ),
        CrashKind::OutOfMemory => (
            s("Oyunun belleği yetmedi"),
            s(
                "Oyun, kendisine ayrılan bellek dolduğu için kapandı. Kurulum ayarlarından ayrılan \
               belleği artırın ya da arka planda açık uygulamaları kapatıp yeniden deneyin.",
            ),
            vec![SuggestedAction::IncreaseMaxMemory {
                suggested_mb: input.max_memory_mb.map(|m| round_mb(m + m / 2)),
            }],
        ),
        CrashKind::InvalidJvmArgument => (
            s("Java ayarlarında geçersiz bir değer var"),
            s(
                "Kurulumun Java argümanlarından biri tanınmadığı için oyun başlatılamadı. Java \
               argümanlarını varsayılana döndürüp yeniden deneyin.",
            ),
            vec![SuggestedAction::ResetJvmArguments],
        ),
        CrashKind::JavaTooOld => {
            let need = match required {
                Some(major) => format!("Bu sürüm Java {major} veya daha yenisini gerektiriyor. "),
                None => s("Bu sürüm daha yeni bir Java gerektiriyor. "),
            };
            (
                s("Java sürümü bu oyun için eski"),
                format!(
                    "{need}Kurulum ayarlarında Java seçimini otomatik yapın; uygun Java sizin \
                     için indirilir."
                ),
                vec![SuggestedAction::UseAutomaticJava {
                    required_major: required,
                }],
            )
        }
        CrashKind::JavaTooNew => {
            let need = match (required, input.java_major) {
                (Some(need), Some(have)) if have > need => format!(
                    "Bu sürüm Java {need} ile çalışıyor, ancak oyun Java {have} ile başlatıldı. "
                ),
                _ => s("Bu sürüm ya da modları daha eski bir Java ile çalışıyor. "),
            };
            (
                s("Java sürümü bu oyun için çok yeni"),
                format!(
                    "{need}Kurulum ayarlarında Java seçimini otomatik yapın; uygun Java sizin \
                     için indirilir."
                ),
                vec![SuggestedAction::UseAutomaticJava {
                    required_major: required,
                }],
            )
        }
        CrashKind::GraphicsDriver => (
            s("Ekran kartı hazırlanamadı"),
            s(
                "Oyun ekran kartınızı hazırlayamadan kapandı. Bu genellikle eski bir ekran kartı \
               sürücüsünden kaynaklanır: sürücünüzü güncelleyip yeniden deneyin. Sorun sürerse \
               modsuz (vanilla) bir kurulumla deneyin.",
            ),
            vec![
                SuggestedAction::UpdateGraphicsDriver,
                SuggestedAction::TryWithoutMods,
            ],
        ),
        CrashKind::IncompatibleMods => {
            let hint = if mods.is_empty() {
                s(
                    "Kurulu modlardan bazıları bu Minecraft sürümüyle ya da birbirleriyle uyumlu \
                   değil. Modlarınızı güncelleyin ya da sorunlu olanları kaldırın; oyun modsuz \
                   açılıyorsa sorun modlardadır.",
                )
            } else {
                format!(
                    "Şu modlar bu Minecraft sürümüyle ya da birbirleriyle uyumlu değil: {}. Bu \
                     modları güncelleyin ya da kaldırın, ardından yeniden deneyin.",
                    list(mods)
                )
            };
            let mut actions = Vec::new();
            if !mods.is_empty() {
                actions.push(SuggestedAction::UpdateMods {
                    mods: mods.to_vec(),
                });
                actions.push(SuggestedAction::RemoveMods {
                    mods: mods.to_vec(),
                });
            }
            actions.push(SuggestedAction::TryWithoutMods);
            (s("Bazı modlar uyumsuz"), hint, actions)
        }
        CrashKind::MissingDependency => {
            let hint = if mods.is_empty() {
                s(
                    "Bazı modların çalışması için gereken başka modlar eksik. Eksik modları kurup \
                   yeniden deneyin.",
                )
            } else {
                format!(
                    "Bazı modların çalışması için başka modlar gerekiyor. Eksik olanlar: {}. \
                     Bunları kurup yeniden deneyin.",
                    list(mods)
                )
            };
            (
                s("Gerekli bir mod eksik"),
                hint,
                vec![SuggestedAction::InstallMods {
                    mods: mods.to_vec(),
                }],
            )
        }
        CrashKind::DuplicateMods => {
            let hint = if mods.is_empty() {
                s(
                    "Mod klasöründe aynı modun birden fazla kopyası var. Fazla kopyaları kaldırıp \
                   yeniden deneyin.",
                )
            } else {
                format!(
                    "Mod klasöründe aynı modun birden fazla kopyası var: {}. Fazla kopyaları \
                     kaldırıp yeniden deneyin.",
                    list(mods)
                )
            };
            (
                s("Aynı mod birden fazla kez yüklü"),
                hint,
                vec![SuggestedAction::RemoveDuplicateMods {
                    mods: mods.to_vec(),
                }],
            )
        }
        CrashKind::ModError => {
            let hint = match mods.len() {
                0 => s(
                    "Kurulu modlardan biri bu sürümle uyuşmuyor ya da hatalı çalışıyor. \
                        Modlarınızı güncelleyin; oyun modsuz (vanilla) açılıyorsa sorun mod \
                        tarafındadır.",
                ),
                1 => format!(
                    "Şu mod oyunun açılmasını engelledi: {}. Modu güncelleyin ya da kaldırıp \
                     yeniden deneyin.",
                    mods[0]
                ),
                _ => format!(
                    "Şu modlar oyunun açılmasını engelledi: {}. Bu modları güncelleyin ya da \
                     kaldırıp yeniden deneyin.",
                    list(mods)
                ),
            };
            let mut actions = Vec::new();
            if !mods.is_empty() {
                actions.push(SuggestedAction::UpdateMods {
                    mods: mods.to_vec(),
                });
                actions.push(SuggestedAction::RemoveMods {
                    mods: mods.to_vec(),
                });
            }
            actions.push(SuggestedAction::TryWithoutMods);
            (s("Bir mod oyunun açılmasını engelledi"), hint, actions)
        }
        CrashKind::CorruptedFiles => (
            s("Oyun dosyaları bozuk görünüyor"),
            s("Oyunun bazı dosyaları eksik ya da bozuk. Kurulumu onarıp yeniden deneyin."),
            vec![SuggestedAction::RepairInstallation],
        ),
        CrashKind::AccessDenied => (
            s("Oyun dosyalarına erişilemedi"),
            s(
                "Oyun bazı dosyalara erişemediği için kapandı. Güvenlik yazılımınız başlatıcıyı ya \
               da Java'yı engelliyor olabilir; başlatıcıyı güvenlik yazılımının istisnalarına \
               ekleyip yeniden deneyin.",
            ),
            vec![SuggestedAction::AddAntivirusException],
        ),
        CrashKind::NoOutput => (
            s("Oyun hiç açılmadan kapandı"),
            s(
                "Oyun hiçbir çıktı üretmeden kapandı. Güvenlik yazılımınız Java'yı engelliyor \
               olabilir; başlatıcıyı istisnalara ekleyip yeniden deneyin. Sorun sürerse kurulumu \
               onarın.",
            ),
            vec![
                SuggestedAction::AddAntivirusException,
                SuggestedAction::RepairInstallation,
            ],
        ),
        CrashKind::Unknown => (
            s("Oyun beklenmedik şekilde kapandı"),
            s(
                "Oyun bir hata nedeniyle kapandı. Ayrıntılar çökme raporunda ve oyun kaydında yer \
               alıyor; sorun sürerse kaydı destek ekibiyle paylaşabilirsiniz.",
            ),
            Vec::new(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn analyze(output: &str) -> CrashDiagnosis {
        analyze_crash(&CrashInput {
            output,
            exit_code: Some(1),
            runtime: Duration::from_secs(3),
            max_memory_mb: Some(8192),
            required_java: Some(21),
            java_major: Some(25),
            ..CrashInput::default()
        })
    }

    #[test]
    fn jvm_startup_failures() {
        let d = analyze(
            "Error occurred during initialization of VM\nCould not reserve enough space for object heap",
        );
        assert_eq!(d.kind, CrashKind::MemoryReserve);
        assert_eq!(
            d.suggested_actions[0],
            SuggestedAction::LowerMaxMemory {
                suggested_mb: Some(4096)
            }
        );
        let d = analyze(
            "Unrecognized VM option 'UseFancyThing'\nError: Could not create the Java Virtual Machine.",
        );
        assert_eq!(d.kind, CrashKind::InvalidJvmArgument);
        let java9_options =
            "Unrecognized option: --add-exports\nError: Could not create the Java Virtual Machine.";
        let d = analyze_crash(&CrashInput {
            output: java9_options,
            exit_code: Some(1),
            java_major: Some(8),
            required_java: Some(21),
            ..CrashInput::default()
        });
        assert_eq!(d.kind, CrashKind::JavaTooOld);
        // On Java 9+ the same message means a mistyped user argument.
        assert_eq!(analyze(java9_options).kind, CrashKind::InvalidJvmArgument);
        let d = analyze("Error: Could not create the Java Virtual Machine.");
        assert_eq!(d.kind, CrashKind::MemoryReserve, "legacy mapping");
    }

    #[test]
    fn java_versions() {
        let d = analyze(
            "Exception in thread \"main\" java.lang.UnsupportedClassVersionError: net/minecraft/client/main/Main has been compiled by a more recent version of the Java Runtime (class file version 65.0), this version of the Java Runtime only recognizes class file versions up to 52.0",
        );
        assert_eq!(d.kind, CrashKind::JavaTooOld);
        assert!(d.hint_tr.contains("Java 21"));
        assert_eq!(
            d.suggested_actions[0],
            SuggestedAction::UseAutomaticJava {
                required_major: Some(21)
            }
        );
        let d = analyze(
            "java.lang.ClassCastException: class jdk.internal.loader.ClassLoaders$AppClassLoader cannot be cast to class java.net.URLClassLoader",
        );
        assert_eq!(d.kind, CrashKind::JavaTooNew);
        assert!(d.hint_tr.contains("Java 25"));
    }

    #[test]
    fn out_of_memory_and_graphics() {
        let d = analyze("java.lang.OutOfMemoryError: Java heap space");
        assert_eq!(d.kind, CrashKind::OutOfMemory);
        assert_eq!(
            d.suggested_actions[0],
            SuggestedAction::IncreaseMaxMemory {
                suggested_mb: Some(12288)
            }
        );
        let d = analyze(
            "[12:00:00] [Render thread/ERROR]: GLFW error 65542: WGL: The driver does not appear to support OpenGL",
        );
        assert_eq!(d.kind, CrashKind::GraphicsDriver);
        let d = analyze_crash(&CrashInput {
            output: "some output",
            exit_code: Some(-1073741819),
            ..CrashInput::default()
        });
        assert_eq!(d.kind, CrashKind::GraphicsDriver);
    }

    #[test]
    fn fabric_missing_dependency() {
        let d = analyze(
            "net.fabricmc.loader.impl.FormattedException: Some of your mods are incompatible with the game or each other!\n\
             A potential solution has been determined, this may resolve your problem:\n\
             \t - Install fabric-api, any version.\n\
             More details:\n\
             \t - Mod 'Mod Menu' (modmenu) 7.2.2 requires any version of fabric-api, which is missing!",
        );
        assert_eq!(d.kind, CrashKind::MissingDependency);
        assert_eq!(d.mods, ["fabric-api"]);
        assert!(d.hint_tr.contains("fabric-api"));
        assert_eq!(
            d.suggested_actions[0],
            SuggestedAction::InstallMods {
                mods: vec!["fabric-api".into()]
            }
        );
    }

    #[test]
    fn fabric_incompatible_mods() {
        let d = analyze(
            "Incompatible mods found!\n\
             \t - Mod 'Iris' (iris) 1.6.4 requires version 0.5.0 or later of mod 'Sodium' (sodium), but only the wrong version is present: 0.4.10!\n\
             \t - Replace mod 'Sodium' (sodium) 0.4.10 with version 0.5.0 or later.",
        );
        assert_eq!(d.kind, CrashKind::IncompatibleMods);
        assert_eq!(d.mods, ["Iris", "Sodium"]);
        assert!(d.hint_tr.contains("Iris, Sodium"));
    }

    #[test]
    fn forge_missing_mandatory_dependencies() {
        let d = analyze(
            "Missing or unsupported mandatory dependencies:\n\
             \tMod ID: 'architectury', Requested by: 'rei', Expected range: '[9.1.12,)', Actual version: '[MISSING]'\n\
             \tMod ID: 'cloth_config', Requested by: 'rei', Expected range: '[11,)', Actual version: '[MISSING]'",
        );
        assert_eq!(d.kind, CrashKind::MissingDependency);
        assert_eq!(d.mods, ["architectury", "cloth_config"]);

        let d = analyze(
            "Missing or unsupported mandatory dependencies:\n\
             \tMod ID: 'forge', Requested by: 'jei', Expected range: '[47.1,)', Actual version: '47.0.3'",
        );
        assert_eq!(d.kind, CrashKind::IncompatibleMods);
        assert_eq!(d.mods, ["jei"]);
    }

    #[test]
    fn duplicates_mixins_and_misc() {
        let d =
            analyze("Found duplicate mods:\n\tMod ID: 'jei' from mod files: jei-1.jar, jei-2.jar");
        assert_eq!(d.kind, CrashKind::DuplicateMods);
        assert_eq!(d.mods, ["jei"]);

        let d = analyze(
            "org.spongepowered.asm.mixin.transformer.throwables.MixinTransformerError: An unexpected critical error was encountered\nCaused by: org.spongepowered.asm.mixin.throwables.MixinApplyError: Mixin [sodium.mixins.json:core.MinecraftMixin] from phase [DEFAULT] in config [sodium.mixins.json] FAILED during APPLY",
        );
        assert_eq!(d.kind, CrashKind::ModError);
        assert_eq!(d.mods, ["sodium"]);
        assert!(
            d.hint_tr
                .starts_with("Şu mod oyunun açılmasını engelledi: sodium.")
        );

        let d = analyze("java.nio.file.AccessDeniedException: C:\\game\\options.txt");
        assert_eq!(d.kind, CrashKind::AccessDenied);
        let d = analyze("java.util.zip.ZipException: zip END header not found");
        assert_eq!(d.kind, CrashKind::CorruptedFiles);
        let d = analyze_crash(&CrashInput::default());
        assert_eq!(d.kind, CrashKind::NoOutput);
        assert!(
            d.suggested_actions
                .contains(&SuggestedAction::AddAntivirusException)
        );
        let d = analyze("[12:00:00] [Render thread/ERROR]: Something odd");
        assert_eq!(d.kind, CrashKind::Unknown);
        assert_eq!(d.suggested_actions, [SuggestedAction::OpenLogs]);
    }

    #[test]
    fn turkish_texts_have_no_codes() {
        for output in [
            "Could not reserve enough space for object heap",
            "java.lang.OutOfMemoryError",
            "EXCEPTION_ACCESS_VIOLATION",
            "Access is denied",
            "",
        ] {
            let d = analyze(output);
            for text in [&d.title_tr, &d.hint_tr] {
                assert!(!text.contains("0x"), "{text}");
                assert!(!text.contains("Exception"), "{text}");
                assert!(!text.contains("-1073741819"), "{text}");
            }
        }
    }

    #[test]
    fn failure_rule_and_report_lookup() {
        assert!(is_failure(Some(1), Duration::from_secs(600), false));
        assert!(is_failure(None, Duration::from_secs(600), false));
        assert!(is_failure(Some(0), Duration::from_secs(5), true));
        assert!(!is_failure(Some(0), Duration::from_secs(5), false));
        assert!(!is_failure(Some(0), Duration::from_secs(60), true));

        let tmp = tempfile::tempdir().unwrap();
        let start = SystemTime::now();
        assert!(find_crash_report(tmp.path(), start).is_none());
        let reports = tmp.path().join("crash-reports");
        std::fs::create_dir_all(&reports).unwrap();
        std::fs::write(reports.join("crash-2026-10-15_12.00.00-client.txt"), "x").unwrap();
        std::fs::write(reports.join("notes.txt"), "x").unwrap();
        let found = find_crash_report(tmp.path(), start).unwrap();
        assert!(found.ends_with("crash-2026-10-15_12.00.00-client.txt"));
        let later = start + Duration::from_secs(3600);
        assert!(find_crash_report(tmp.path(), later).is_none());
        std::fs::write(tmp.path().join("hs_err_pid123.log"), "x").unwrap();
        assert!(find_jvm_error_log(tmp.path(), start).is_some());
    }
}

//! Support bundles: one zip with the launcher's logs, recent game logs and crash reports, plus
//! short system and instance reports. Every entry goes through [`mask_secrets`] on its way in,
//! and nothing from the account store, the AFK folder or the OS keychain is ever read here.

use std::fmt::Write as _;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use zip::write::SimpleFileOptions;

use crate::error::{Error, Result};
use crate::mask::mask_secrets;

/// Larger files keep their first and last part (the start holds the command line and the mod
/// list, the end holds the crash).
pub const MAX_ENTRY_BYTES: u64 = 8 * 1024 * 1024;
const HEAD_BYTES: u64 = 1024 * 1024;

/// Where a bundle is saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundleFolder {
    Downloads,
    Desktop,
    Home,
}

/// The user's Downloads folder, else the Desktop, else the home folder.
pub fn bundle_folder() -> Option<(PathBuf, BundleFolder)> {
    [
        (dirs::download_dir(), BundleFolder::Downloads),
        (dirs::desktop_dir(), BundleFolder::Desktop),
        (dirs::home_dir(), BundleFolder::Home),
    ]
    .into_iter()
    .find_map(|(dir, kind)| dir.filter(|d| d.is_dir()).map(|d| (d, kind)))
}

/// `projects-destek-2026-10-15-1432.zip`.
pub fn bundle_file_name(now: &jiff::Zoned) -> String {
    format!("projects-destek-{}.zip", now.strftime("%Y-%m-%d-%H%M"))
}

/// `dir/name`, or `dir/<stem>-2.<ext>`, `-3`, ... when that name is taken.
pub fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let (stem, ext) = name.rsplit_once('.').unwrap_or((name, ""));
    (2u32..)
        .map(|n| {
            if ext.is_empty() {
                dir.join(format!("{stem}-{n}"))
            } else {
                dir.join(format!("{stem}-{n}.{ext}"))
            }
        })
        .find(|candidate| !candidate.exists())
        .unwrap_or(first)
}

/// One file in the bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleEntry {
    /// Path inside the zip (`/`-separated).
    pub name: String,
    pub source: EntrySource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntrySource {
    Text(String),
    /// A text file on disk (logs, crash reports), read when the bundle is written.
    File(PathBuf),
}

impl BundleEntry {
    pub fn text(name: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            source: EntrySource::Text(text.into()),
        }
    }

    pub fn file(name: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            source: EntrySource::File(path.into()),
        }
    }
}

/// Writes the zip at `dest` (through `<dest>.part`, so a failed run leaves nothing behind).
/// Files that vanished or cannot be read are skipped. Returns how many entries were written.
pub fn write_bundle(dest: &Path, entries: &[BundleEntry]) -> Result<usize> {
    let mut part = dest.as_os_str().to_owned();
    part.push(".part");
    let part = PathBuf::from(part);
    let result = write_zip(&part, entries);
    let written = match result {
        Ok(written) => written,
        Err(error) => {
            let _ = std::fs::remove_file(&part);
            return Err(error);
        }
    };
    if let Err(error) = std::fs::rename(&part, dest) {
        let _ = std::fs::remove_file(&part);
        return Err(Error::io(dest, error));
    }
    Ok(written)
}

fn write_zip(path: &Path, entries: &[BundleEntry]) -> Result<usize> {
    let file = std::fs::File::create(path).map_err(|e| Error::io(path, e))?;
    let mut zip = zip::ZipWriter::new(std::io::BufWriter::new(file));
    let mut written = 0;
    for entry in entries {
        let text = match &entry.source {
            EntrySource::Text(text) => mask_secrets(text).into_owned(),
            EntrySource::File(source) => match read_masked(source, MAX_ENTRY_BYTES) {
                Ok(text) => text,
                Err(error) => {
                    tracing::debug!(path = %source.display(), %error, "skipping a bundle file");
                    continue;
                }
            },
        };
        let mut options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        if let Some(time) = zip_now() {
            options = options.last_modified_time(time);
        }
        zip.start_file(entry.name.as_str(), options)?;
        zip.write_all(text.as_bytes())
            .map_err(|e| Error::io(path, e))?;
        written += 1;
    }
    let mut out = zip.finish()?;
    out.flush().map_err(|e| Error::io(path, e))?;
    Ok(written)
}

fn zip_now() -> Option<zip::DateTime> {
    let now = jiff::Zoned::now();
    zip::DateTime::from_date_and_time(
        u16::try_from(now.year()).ok()?,
        u8::try_from(now.month()).ok()?,
        u8::try_from(now.day()).ok()?,
        u8::try_from(now.hour()).ok()?,
        u8::try_from(now.minute()).ok()?,
        u8::try_from(now.second()).ok()?,
    )
    .ok()
}

/// A text file for the bundle: masked, and for files over `max_bytes` only the first megabyte
/// and the end, cut at line breaks so no secret is split in half.
pub fn read_masked(path: &Path, max_bytes: u64) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let len = file.metadata()?.len();
    let text = if len <= max_bytes {
        let mut bytes = Vec::with_capacity(usize::try_from(len).unwrap_or(0));
        file.read_to_end(&mut bytes)?;
        String::from_utf8_lossy(&bytes).into_owned()
    } else {
        let head_len = HEAD_BYTES.min(max_bytes / 2);
        let tail_len = max_bytes - head_len;
        let mut head = vec![0; usize::try_from(head_len).unwrap_or(0)];
        file.read_exact(&mut head)?;
        file.seek(SeekFrom::Start(len - tail_len))?;
        let mut tail = Vec::new();
        file.read_to_end(&mut tail)?;
        let head = &head[..head.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1)];
        let tail = &tail[tail
            .iter()
            .position(|b| *b == b'\n')
            .map_or(tail.len(), |i| i + 1)..];
        let skipped = len - head.len() as u64 - tail.len() as u64;
        format!(
            "{}\n[... {} kısaltıldı ...]\n\n{}",
            String::from_utf8_lossy(head),
            human_bytes(skipped),
            String::from_utf8_lossy(tail)
        )
    };
    Ok(mask_secrets(&text).into_owned())
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

/// Files directly in `dir` whose names pass `keep`, newest first.
fn newest_files(dir: &Path, keep: impl Fn(&str) -> bool) -> Vec<(PathBuf, SystemTime)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<(PathBuf, SystemTime)> = entries
        .filter_map(Result::ok)
        .filter(|entry| keep(&entry.file_name().to_string_lossy()))
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|entry| Some((entry.path(), modified(&entry.path())?)))
        .collect();
    files.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.cmp(&a.0)));
    files
}

/// Launcher logs (`launcher.<date>.log`) written to within `max_age`, oldest first.
pub fn recent_launcher_logs(logs_dir: &Path, now: SystemTime, max_age: Duration) -> Vec<PathBuf> {
    let mut logs: Vec<PathBuf> = newest_files(logs_dir, |name| {
        name.starts_with("launcher.") && name.ends_with(".log")
    })
    .into_iter()
    .filter(|(_, modified)| {
        now.duration_since(*modified)
            .map_or(true, |age| age <= max_age)
    })
    .map(|(path, _)| path)
    .collect();
    logs.reverse();
    logs
}

/// The `count` newest captured game logs (`game-*.log`), newest first.
pub fn newest_game_logs(logs_dir: &Path, count: usize) -> Vec<PathBuf> {
    newest_files(logs_dir, |name| {
        name.starts_with("game-") && name.ends_with(".log")
    })
    .into_iter()
    .take(count)
    .map(|(path, _)| path)
    .collect()
}

/// Crash reports (`crash-reports/crash-*.txt`) and JVM fatal error logs (`hs_err_pid*.log`) of
/// the given game folders (label, folder), written within `max_age`, newest first.
pub fn recent_crash_reports(
    game_dirs: &[(String, PathBuf)],
    now: SystemTime,
    max_age: Duration,
    max_count: usize,
) -> Vec<(String, PathBuf)> {
    let mut all: Vec<(String, PathBuf, SystemTime)> = Vec::new();
    for (label, game_dir) in game_dirs {
        let reports = newest_files(&game_dir.join("crash-reports"), |name| {
            name.starts_with("crash-") && name.ends_with(".txt")
        });
        let jvm = newest_files(game_dir, |name| {
            name.starts_with("hs_err_pid") && name.ends_with(".log")
        });
        all.extend(
            reports
                .into_iter()
                .chain(jvm)
                .map(|(path, modified)| (label.clone(), path, modified)),
        );
    }
    all.retain(|(_, _, modified)| {
        now.duration_since(*modified)
            .map_or(true, |age| age <= max_age)
    });
    all.sort_by_key(|(_, _, modified)| std::cmp::Reverse(*modified));
    all.into_iter()
        .take(max_count)
        .map(|(label, path, _)| (label, path))
        .collect()
}

/// A safe single path component for names inside the zip (instance names can hold anything).
pub fn zip_component(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ' ' | '(' | ')') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').trim();
    if cleaned.is_empty() {
        "_".to_owned()
    } else {
        cleaned.chars().take(64).collect()
    }
}

/// `12,4 GB` (Turkish decimal comma).
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit]).replace('.', ",")
}

/// The machine, as far as support needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MachineInfo {
    pub os: String,
    pub kernel: String,
    pub arch: String,
    pub cpu: String,
    pub cpu_threads: usize,
    pub cpu_cores: Option<usize>,
    pub memory_total: u64,
    pub memory_available: u64,
}

impl MachineInfo {
    pub fn collect() -> Self {
        use sysinfo::{CpuRefreshKind, System};
        let mut system = System::new();
        system.refresh_memory();
        system.refresh_cpu_list(CpuRefreshKind::nothing());
        let cpu = system
            .cpus()
            .first()
            .map(|cpu| cpu.brand().trim().to_owned())
            .filter(|brand| !brand.is_empty())
            .unwrap_or_else(|| "bilinmiyor".to_owned());
        Self {
            os: System::long_os_version()
                .or_else(System::name)
                .unwrap_or_else(|| std::env::consts::OS.to_owned()),
            kernel: System::kernel_version().unwrap_or_default(),
            arch: std::env::consts::ARCH.to_owned(),
            cpu,
            cpu_threads: system.cpus().len(),
            cpu_cores: System::physical_core_count(),
            memory_total: system.total_memory(),
            memory_available: system.available_memory(),
        }
    }
}

/// Facts for `sistem.txt`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SystemReport {
    pub created_at: String,
    pub launcher_version: String,
    pub debug_build: bool,
    pub machine: MachineInfo,
    pub data_dir: PathBuf,
    pub data_dir_free: Option<u64>,
    /// Account sign-ins survive restarts (the OS keychain works).
    pub secure_storage: bool,
    /// One line per Java the launcher can use.
    pub java: Vec<String>,
}

impl SystemReport {
    pub fn render(&self) -> String {
        let m = &self.machine;
        let mut out = String::new();
        let _ = writeln!(out, "Projects Launcher destek paketi");
        let _ = writeln!(out, "Oluşturulma: {}", self.created_at);
        let _ = writeln!(out);
        let build = if self.debug_build {
            " (geliştirici sürümü)"
        } else {
            ""
        };
        let _ = writeln!(out, "Launcher sürümü: {}{build}", self.launcher_version);
        let _ = writeln!(out, "İşletim sistemi: {} (çekirdek {})", m.os, m.kernel);
        let _ = writeln!(out, "Mimari: {}", m.arch);
        let cores = m
            .cpu_cores
            .map(|cores| format!("{cores} çekirdek, {} iş parçacığı", m.cpu_threads))
            .unwrap_or_else(|| format!("{} iş parçacığı", m.cpu_threads));
        let _ = writeln!(out, "İşlemci: {} ({cores})", m.cpu);
        let _ = writeln!(
            out,
            "Bellek (RAM): {} toplam, {} boş",
            human_bytes(m.memory_total),
            human_bytes(m.memory_available)
        );
        let _ = writeln!(out, "Veri klasörü: {}", self.data_dir.display());
        let free = self
            .data_dir_free
            .map(human_bytes)
            .unwrap_or_else(|| "bilinmiyor".to_owned());
        let _ = writeln!(out, "Veri diskinde boş alan: {free}");
        let ascii = |path: &Path| {
            if path.to_string_lossy().is_ascii() {
                "yok"
            } else {
                "var"
            }
        };
        let _ = writeln!(
            out,
            "Veri klasörü yolunda Türkçe ya da özel karakter: {}",
            ascii(&self.data_dir)
        );
        if let Some(home) = dirs::home_dir() {
            let _ = writeln!(
                out,
                "Kullanıcı klasörü yolunda Türkçe ya da özel karakter: {}",
                ascii(&home)
            );
        }
        let keychain = if self.secure_storage {
            "çalışıyor"
        } else {
            "kullanılamıyor (girişler yalnızca bu oturumda geçerli)"
        };
        let _ = writeln!(out, "Güvenli depolama: {keychain}");
        let _ = writeln!(out);
        let _ = writeln!(out, "Java:");
        if self.java.is_empty() {
            let _ = writeln!(out, "  bulunamadı");
        }
        for line in &self.java {
            let _ = writeln!(out, "  {line}");
        }
        out
    }
}

/// A file in an instance's `mods` folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModFile {
    pub file_name: String,
    pub enabled: bool,
}

/// Mod jars of a game folder (`mods/*.jar`, and `*.jar.disabled` ones), by name.
pub fn mod_files(game_dir: &Path) -> Vec<ModFile> {
    let Ok(entries) = std::fs::read_dir(game_dir.join("mods")) else {
        return Vec::new();
    };
    let mut mods: Vec<ModFile> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let lower = name.to_lowercase();
            if lower.ends_with(".jar") {
                Some(ModFile {
                    file_name: name,
                    enabled: true,
                })
            } else if lower.ends_with(".jar.disabled") {
                Some(ModFile {
                    file_name: name,
                    enabled: false,
                })
            } else {
                None
            }
        })
        .collect();
    mods.sort_by_key(|m| m.file_name.to_lowercase());
    mods
}

/// One instance for `kurulumlar.txt`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InstanceReport {
    pub name: String,
    pub id: String,
    pub minecraft: String,
    /// "Fabric 0.16.5", "Vanilla".
    pub loader: String,
    /// Modpack the launcher keeps up to date ("projects 1.4.2").
    pub pack: Option<String>,
    /// "Otomatik" or the chosen Java's path.
    pub java: String,
    pub memory: String,
    pub jvm_args: String,
    pub mods: Vec<ModFile>,
}

pub fn render_instances(instances: &[InstanceReport]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Kurulumlar ({})", instances.len());
    for instance in instances {
        let _ = writeln!(out);
        let _ = writeln!(out, "{} [{}]", instance.name, instance.id);
        let _ = writeln!(out, "  Minecraft: {}", instance.minecraft);
        let _ = writeln!(out, "  Mod yükleyici: {}", instance.loader);
        if let Some(pack) = &instance.pack {
            let _ = writeln!(out, "  Mod paketi: {pack}");
        }
        let _ = writeln!(out, "  Java: {}", instance.java);
        let _ = writeln!(out, "  Bellek: {}", instance.memory);
        if !instance.jvm_args.trim().is_empty() {
            let _ = writeln!(out, "  JVM argümanları: {}", instance.jvm_args.trim());
        }
        let enabled = instance.mods.iter().filter(|m| m.enabled).count();
        let _ = writeln!(
            out,
            "  Modlar: {} dosya ({enabled} açık)",
            instance.mods.len()
        );
        for m in &instance.mods {
            let state = if m.enabled { "" } else { " (kapalı)" };
            let _ = writeln!(out, "    {}{state}", m.file_name);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn set_age(path: &Path, secs: u64) {
        fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(SystemTime::now() - Duration::from_secs(secs))
            .unwrap();
    }

    fn read_zip(path: &Path) -> Vec<(String, String)> {
        let mut archive = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
        (0..archive.len())
            .map(|i| {
                let mut file = archive.by_index(i).unwrap();
                let mut text = String::new();
                file.read_to_string(&mut text).unwrap();
                (file.name().unwrap().to_string(), text)
            })
            .collect()
    }

    #[test]
    fn names_and_unique_paths() {
        let now: jiff::Zoned = "2026-10-15T14:32:07+03:00[Europe/Istanbul]"
            .parse()
            .unwrap();
        assert_eq!(
            bundle_file_name(&now),
            "projects-destek-2026-10-15-1432.zip"
        );
        let dir = tempfile::tempdir().unwrap();
        let name = "projects-destek-2026-10-15-1432.zip";
        assert_eq!(unique_path(dir.path(), name), dir.path().join(name));
        fs::write(dir.path().join(name), b"x").unwrap();
        assert_eq!(
            unique_path(dir.path(), name),
            dir.path().join("projects-destek-2026-10-15-1432-2.zip")
        );
        assert_eq!(
            zip_component("Projects: Survival/1"),
            "Projects_ Survival_1"
        );
        assert_eq!(zip_component(".."), "_");
        assert_eq!(human_bytes(1536), "1,5 KB");
        assert_eq!(human_bytes(12), "12 B");
    }

    #[test]
    fn picks_recent_logs_and_crash_reports() {
        let dir = tempfile::tempdir().unwrap();
        let logs = dir.path().join("logs");
        let day = 24 * 60 * 60;
        for (name, age) in [
            ("launcher.2026-10-15.log", 60),
            ("launcher.2026-10-14.log", day),
            ("launcher.2026-10-10.log", 5 * day),
            ("game-a-20261015-100000.log", 10),
            ("game-a-20261014-100000.log", day),
            ("game-b-20261013-100000.log", 2 * day),
            ("launch-x.args", 10),
        ] {
            write(&logs.join(name), "x");
            set_age(&logs.join(name), age);
        }
        let now = SystemTime::now();
        let launcher = recent_launcher_logs(&logs, now, Duration::from_secs(3 * day));
        let names: Vec<_> = launcher
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            ["launcher.2026-10-14.log", "launcher.2026-10-15.log"]
        );

        let games = newest_game_logs(&logs, 2);
        assert_eq!(games.len(), 2);
        assert!(games[0].ends_with("game-a-20261015-100000.log"));

        let game_a = dir.path().join("instances/a/minecraft");
        let game_b = dir.path().join("instances/b/minecraft");
        write(
            &game_a.join("crash-reports/crash-2026-10-15_10.00.00-client.txt"),
            "boom",
        );
        write(&game_a.join("crash-reports/old-format.txt"), "ignored");
        write(&game_b.join("hs_err_pid42.log"), "jvm");
        write(
            &game_b.join("crash-reports/crash-2026-09-01_10.00.00-client.txt"),
            "old",
        );
        set_age(
            &game_b.join("crash-reports/crash-2026-09-01_10.00.00-client.txt"),
            40 * day,
        );
        let reports = recent_crash_reports(
            &[
                ("A".into(), game_a),
                ("B".into(), game_b),
                ("C".into(), dir.path().join("none")),
            ],
            now,
            Duration::from_secs(14 * day),
            10,
        );
        let labels: Vec<_> = reports.iter().map(|(label, _)| label.as_str()).collect();
        assert_eq!(reports.len(), 2, "{reports:?}");
        assert!(labels.contains(&"A") && labels.contains(&"B"));
    }

    #[test]
    fn bundle_is_masked_and_complete() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("game.log");
        write(
            &log,
            "# command: java --accessToken eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjMifQ.c2lnbmF0dXJlMTIz\n[Chat] /login hunter2\nok\n",
        );
        let dest = dir.path().join("out/projects-destek.zip");
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        let entries = [
            BundleEntry::text("sistem.txt", "Veri klasörü: C:\\Users\\hazar\\AppData"),
            BundleEntry::file("oyun-kayitlari/game.log", &log),
            BundleEntry::file("oyun-kayitlari/missing.log", dir.path().join("missing.log")),
        ];
        assert_eq!(write_bundle(&dest, &entries).unwrap(), 2);
        assert!(!dest.with_extension("zip.part").exists());
        let files = read_zip(&dest);
        assert_eq!(files.len(), 2);
        let all: String = files.iter().map(|(_, text)| text.as_str()).collect();
        assert!(!all.contains("hunter2") && !all.contains("eyJhbGci") && !all.contains("hazar"));
        assert!(all.contains("--accessToken ********") && all.contains("ok\n"));
        assert_eq!(files[1].0, "oyun-kayitlari/game.log");
    }

    #[test]
    fn big_files_keep_head_and_tail() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.log");
        let mut text = String::from("HEAD password=hunter2\n");
        for i in 0..2000 {
            text.push_str(&format!("line {i} lorem ipsum dolor sit amet\n"));
        }
        text.push_str("TAIL accessToken=abc\n");
        fs::write(&path, &text).unwrap();
        let out = read_masked(&path, 4096).unwrap();
        assert!(
            out.starts_with("HEAD password=********\n"),
            "{}",
            &out[..60]
        );
        assert!(out.ends_with("TAIL accessToken=********\n"));
        assert!(out.contains("kısaltıldı"));
        assert!(out.len() < 4300);
        // Cut at line breaks: every kept line is whole.
        assert!(out.lines().all(|l| l.is_empty()
            || l.starts_with("line ")
            || l.starts_with("HEAD")
            || l.starts_with("TAIL")
            || l.starts_with("[...")));
    }

    #[test]
    fn reports_render() {
        let report = SystemReport {
            created_at: "2026-10-15 14:32".into(),
            launcher_version: "2.0.0".into(),
            machine: MachineInfo {
                os: "Windows 11 Pro".into(),
                cpu: "Ryzen".into(),
                cpu_threads: 16,
                cpu_cores: Some(8),
                memory_total: 32 * 1024 * 1024 * 1024,
                ..MachineInfo::default()
            },
            data_dir: PathBuf::from("D:/Oyunlar/ProjectsLauncher"),
            java: vec!["Mojang java-runtime-delta: 21.0.7".into()],
            ..SystemReport::default()
        };
        let text = report.render();
        assert!(text.contains("Launcher sürümü: 2.0.0\n"));
        assert!(text.contains("İşlemci: Ryzen (8 çekirdek, 16 iş parçacığı)"));
        assert!(text.contains("Bellek (RAM): 32,0 GB toplam"));
        assert!(text.contains("  Mojang java-runtime-delta: 21.0.7"));
        assert!(!MachineInfo::collect().cpu.is_empty());

        let game = tempfile::tempdir().unwrap();
        write(&game.path().join("mods/b.jar"), "");
        write(&game.path().join("mods/a.jar.disabled"), "");
        write(&game.path().join("mods/readme.txt"), "");
        let mods = mod_files(game.path());
        assert_eq!(mods.len(), 2);
        assert_eq!(mods[0].file_name, "a.jar.disabled");
        let text = render_instances(&[InstanceReport {
            name: "Projects".into(),
            id: "projects".into(),
            minecraft: "1.21.1".into(),
            loader: "Fabric 0.16.5".into(),
            pack: Some("projects 1.4.2".into()),
            java: "Otomatik".into(),
            memory: "2048-4096 MB".into(),
            jvm_args: String::new(),
            mods,
        }]);
        assert!(text.starts_with("Kurulumlar (1)\n\nProjects [projects]\n"));
        assert!(
            text.contains("  Modlar: 2 dosya (1 açık)\n    a.jar.disabled (kapalı)\n    b.jar\n")
        );
    }
}

//! The file a crash dialog opens to explain a crash (REPAIR-RB's correction: players were sent to a
//! log that said nothing about the crash). Best first: the game's crash report of the run
//! (`crash-reports/crash-*.txt`), else the JVM's fatal error log (`hs_err_pid*.log`), else the game's
//! own log of the run: the output the launcher captured, else `logs/latest.log` when the game wrote
//! it during the run. Never the launcher's own log.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use super::crash::CrashDiagnosis;

/// What [`crash_file`] found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum CrashFileKind {
    /// The game's crash report.
    CrashReport,
    /// The JVM's fatal error log.
    JvmLog,
    /// The game's own log of the run.
    GameLog,
}

/// The file to open for a crash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrashFile {
    pub kind: CrashFileKind,
    pub path: PathBuf,
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

/// The kind of a report the crash analysis kept ([`CrashDiagnosis::crash_report`]).
fn report_kind(path: &Path) -> Option<CrashFileKind> {
    let name = file_name(path);
    if name.starts_with("hs_err_pid") && name.ends_with(".log") {
        Some(CrashFileKind::JvmLog)
    } else if name.ends_with(".txt") {
        Some(CrashFileKind::CrashReport)
    } else {
        None
    }
}

/// The file that explains `diagnosis` (a crash of a game in `game_dir`), if one is still on disk.
/// `logs/latest.log` counts only when it changed after `run_started` (another game of the
/// folder may have written it since; without a start time it does not count).
pub fn crash_file(
    diagnosis: &CrashDiagnosis,
    game_dir: &Path,
    run_started: Option<SystemTime>,
) -> Option<CrashFile> {
    if let Some(report) = diagnosis.crash_report.as_ref().filter(|p| p.is_file())
        && let Some(kind) = report_kind(report)
    {
        return Some(CrashFile {
            kind,
            path: report.clone(),
        });
    }
    if let Some(log) = diagnosis.log_file.as_ref().filter(|p| p.is_file()) {
        return Some(CrashFile {
            kind: CrashFileKind::GameLog,
            path: log.clone(),
        });
    }
    let latest = game_dir.join("logs").join("latest.log");
    let since = run_started?.checked_sub(Duration::from_secs(5))?;
    let written = std::fs::metadata(&latest)
        .and_then(|meta| meta.modified())
        .is_ok_and(|modified| modified >= since);
    written.then_some(CrashFile {
        kind: CrashFileKind::GameLog,
        path: latest,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launch::crash::{CrashInput, analyze_crash};

    fn diagnosis(report: Option<PathBuf>, log: Option<PathBuf>) -> CrashDiagnosis {
        let mut d = analyze_crash(&CrashInput {
            output: "boom",
            exit_code: Some(1),
            ..CrashInput::default()
        });
        d.crash_report = report;
        d.log_file = log;
        d
    }

    #[test]
    fn the_crash_report_then_the_jvm_log_then_the_games_own_log() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path().join("game");
        let reports = game.join("crash-reports");
        let logs = game.join("logs");
        std::fs::create_dir_all(&reports).unwrap();
        std::fs::create_dir_all(&logs).unwrap();
        let report = reports.join("crash-2026-11-24_21.14.02-client.txt");
        let jvm = game.join("hs_err_pid4242.log");
        let captured = dir.path().join("launcher-logs").join("game-projects-1.log");
        std::fs::create_dir_all(captured.parent().unwrap()).unwrap();
        for path in [&report, &jvm, &captured] {
            std::fs::write(path, "x").unwrap();
        }
        let started = SystemTime::now() - Duration::from_secs(60);

        let found = crash_file(
            &diagnosis(Some(report.clone()), Some(captured.clone())),
            &game,
            Some(started),
        );
        assert_eq!(
            found,
            Some(CrashFile {
                kind: CrashFileKind::CrashReport,
                path: report.clone()
            })
        );
        let found = crash_file(
            &diagnosis(Some(jvm.clone()), Some(captured.clone())),
            &game,
            Some(started),
        );
        assert_eq!(found.map(|f| f.kind), Some(CrashFileKind::JvmLog));
        // The report is gone (an old crash shown again): the run's captured output.
        std::fs::remove_file(&report).unwrap();
        let found = crash_file(
            &diagnosis(Some(report.clone()), Some(captured.clone())),
            &game,
            Some(started),
        );
        assert_eq!(
            found,
            Some(CrashFile {
                kind: CrashFileKind::GameLog,
                path: captured.clone()
            })
        );
        // Nothing captured: the game's latest.log, only when the run wrote it.
        std::fs::remove_file(&captured).unwrap();
        assert_eq!(
            crash_file(
                &diagnosis(None, Some(captured.clone())),
                &game,
                Some(started)
            ),
            None
        );
        std::fs::write(logs.join("latest.log"), "log").unwrap();
        assert_eq!(
            crash_file(&diagnosis(None, None), &game, Some(started)),
            Some(CrashFile {
                kind: CrashFileKind::GameLog,
                path: logs.join("latest.log")
            })
        );
        assert_eq!(
            crash_file(
                &diagnosis(None, None),
                &game,
                Some(SystemTime::now() + Duration::from_secs(600))
            ),
            None,
            "a latest.log older than the run is another game's"
        );
        assert_eq!(crash_file(&diagnosis(None, None), &game, None), None);
    }
}

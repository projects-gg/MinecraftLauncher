//! The config writer must match the legacy C# `AfkConfigWriter` byte for byte.
//!
//! `tests/golden/<case>/` holds what the legacy writer produced (compiled from the legacy sources
//! by a throwaway .NET 10 harness) for the account in `account.json`, which is that account
//! serialized by Newtonsoft.Json with the legacy store settings. Each case is loaded through the
//! legacy reader and rebuilt here; `cases.json` lists the launcher nickname passed to the writer.
//!
//! One addition is deliberate: the `[MCSettings]` locale (the language servers see; the legacy
//! file had none). It is taken out of the rebuilt ini before comparing, so the fixtures stay
//! exactly what the legacy writer produced.

use std::path::{Path, PathBuf};

use pl_afk::{
    INI_FILE_NAME, LOGIN_SCRIPT_FILE_NAME, MATCHES_FILE_NAME, VersionCatalog, build_config,
    write_config,
};

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    name: String,
    launcher_nickname: Option<String>,
}

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
}

fn cases() -> Vec<Case> {
    let text = std::fs::read_to_string(golden_dir().join("cases.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

/// The locale table `build_config` (Turkish) adds right before `[ChatBot]`.
const LOCALE_TABLE: &str = "[MCSettings]\r\nLocale = \"tr_tr\"\r\n\r\n";

/// `file` as the legacy writer would have written it: the ini without its locale table, which
/// must be there exactly once, in its place.
fn as_legacy(case: &str, file: &str, actual: &[u8]) -> Vec<u8> {
    if file != INI_FILE_NAME {
        return actual.to_vec();
    }
    let text = std::str::from_utf8(actual).unwrap();
    let placed = format!("\r\n{LOCALE_TABLE}[ChatBot]\r\n");
    assert_eq!(
        (
            text.matches(LOCALE_TABLE).count(),
            text.matches(&placed).count()
        ),
        (1, 1),
        "{case}/{file}: the locale table, once, right before [ChatBot]"
    );
    text.replacen(LOCALE_TABLE, "", 1).into_bytes()
}

/// Fails with the first differing line, shown with escapes, instead of two walls of bytes.
fn assert_same(case: &str, file: &str, actual: &[u8], expected: &[u8]) {
    if actual == expected {
        return;
    }
    let actual_text = String::from_utf8_lossy(actual);
    let expected_text = String::from_utf8_lossy(expected);
    let mut actual_lines = actual_text.split_inclusive('\n');
    let mut expected_lines = expected_text.split_inclusive('\n');
    for number in 1.. {
        match (actual_lines.next(), expected_lines.next()) {
            (None, None) => break,
            (a, e) if a == e => {}
            (a, e) => panic!(
                "{case}/{file} differs at line {number}:\n  rust:   {:?}\n  legacy: {:?}",
                a.unwrap_or("<end>"),
                e.unwrap_or("<end>")
            ),
        }
    }
    panic!("{case}/{file} differs (same lines, different bytes)");
}

#[test]
fn golden_fixtures_are_intact() {
    // Guards against git line-ending normalization of the fixtures (see tests/golden/.gitattributes).
    for case in cases() {
        for file in [INI_FILE_NAME, MATCHES_FILE_NAME, LOGIN_SCRIPT_FILE_NAME] {
            let bytes = std::fs::read(golden_dir().join(&case.name).join(file)).unwrap();
            assert!(
                !bytes.starts_with(&[0xEF, 0xBB, 0xBF]),
                "{}/{file} has a BOM",
                case.name
            );
            let lf = bytes.iter().filter(|&&b| b == b'\n').count();
            let crlf = bytes.windows(2).filter(|w| w == b"\r\n").count();
            assert_eq!(lf, crlf, "{}/{file} lost its CRLF line endings", case.name);
        }
    }
}

#[test]
fn config_matches_the_legacy_writer_byte_for_byte() {
    let cases = cases();
    assert!(cases.len() >= 6, "expected at least 6 golden cases");
    for case in &cases {
        let dir = golden_dir().join(&case.name);
        let json = std::fs::read_to_string(dir.join("account.json")).unwrap();
        let legacy = pl_afk::legacy::parse_account(&json).unwrap();
        let files = build_config(
            &legacy.account,
            &legacy.login_commands,
            case.launcher_nickname.as_deref().unwrap_or_default(),
            &VersionCatalog::built_in(),
        );
        for (file, actual) in [
            (INI_FILE_NAME, &files.ini),
            (MATCHES_FILE_NAME, &files.matches),
            (LOGIN_SCRIPT_FILE_NAME, &files.login_script),
        ] {
            let expected = std::fs::read(dir.join(file)).unwrap();
            let actual = as_legacy(&case.name, file, actual.as_bytes());
            assert_same(&case.name, file, &actual, &expected);
        }
    }
}

#[test]
fn written_files_are_utf8_without_bom_and_identical() {
    let out = tempfile::tempdir().unwrap();
    for case in cases() {
        let dir = golden_dir().join(&case.name);
        let json = std::fs::read_to_string(dir.join("account.json")).unwrap();
        let legacy = pl_afk::legacy::parse_account(&json).unwrap();
        let files = build_config(
            &legacy.account,
            &legacy.login_commands,
            case.launcher_nickname.as_deref().unwrap_or_default(),
            &VersionCatalog::built_in(),
        );
        let target = out.path().join(&case.name);
        write_config(&target, &files).unwrap();
        for file in [INI_FILE_NAME, MATCHES_FILE_NAME, LOGIN_SCRIPT_FILE_NAME] {
            let written = std::fs::read(target.join(file)).unwrap();
            let expected = std::fs::read(dir.join(file)).unwrap();
            let written = as_legacy(&case.name, file, &written);
            assert_same(&case.name, file, &written, &expected);
        }
    }
}

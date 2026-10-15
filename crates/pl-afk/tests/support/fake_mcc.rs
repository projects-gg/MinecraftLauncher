//! Test-only stand-in for the Projects Console Client (built as `pl-afk-fake-mcc`).
//!
//! Runs in the account folder like the real client and follows `fake-mcc.json` from there:
//! `{"runs": [[step, ...], ...]}` — one step list per launch (the last one repeats). Steps:
//!
//! - `{"out": "text"}` / `{"err": "text"}`: print a line on stdout / stderr
//! - `{"sleepMs": 50}`
//! - `{"join": true}`: like MCC's ScriptScheduler + bridge: read `giris-komutlari.txt`, print the
//!   `joined` event, then echo the script's `send` lines as `[Script] send ...`
//! - `{"echoUntilQuit": true}`: echo stdin lines as `[echo] ...`, exit 0 on `/quit`
//! - `{"waitLine": "text"}`: read stdin until that line arrives
//! - `{"hang": true}`: ignore everything until killed
//! - `{"exit": 3}`
//!
//! Each launch appends a summary to `fake-mcc-report.txt`; stdin lines go to `fake-mcc-stdin.txt`.

use std::fs::OpenOptions;
use std::io::{BufRead, Write};
use std::path::Path;
use std::time::Duration;

use serde_json::Value;

fn append(path: &Path, text: &str) {
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{text}");
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cwd = std::env::current_dir().unwrap_or_default();

    let counter = cwd.join("fake-mcc-runs.txt");
    let run: usize = std::fs::read_to_string(&counter)
        .ok()
        .and_then(|t| t.trim().parse().ok())
        .unwrap_or(0)
        + 1;
    let _ = std::fs::write(&counter, run.to_string());

    let scenario: Value = std::fs::read_to_string(cwd.join("fake-mcc.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(Value::Null);
    let runs = scenario["runs"].as_array().cloned().unwrap_or_default();
    let steps = runs
        .get(run - 1)
        .or(runs.last())
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let ini = args.first().map(String::as_str).unwrap_or_default();
    append(
        &cwd.join("fake-mcc-report.txt"),
        &format!(
            "run={run} ini_name={} ini_exists={} mode={} bridge={} login_script_at_start={}",
            Path::new(ini)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            Path::new(ini).is_file(),
            args.last().map(String::as_str).unwrap_or_default(),
            std::env::var("MCC_LAUNCHER_BRIDGE").unwrap_or_default(),
            cwd.join("giris-komutlari.txt").is_file(),
        ),
    );

    let stdin_log = cwd.join("fake-mcc-stdin.txt");
    let stdin = std::io::stdin();
    let mut input = stdin.lock().lines();
    let mut stdout = std::io::stdout();

    for step in steps {
        if let Some(text) = step["out"].as_str() {
            let _ = writeln!(stdout, "{text}");
            let _ = stdout.flush();
        } else if let Some(text) = step["err"].as_str() {
            eprintln!("{text}");
        } else if let Some(ms) = step["sleepMs"].as_u64() {
            std::thread::sleep(Duration::from_millis(ms));
        } else if step["join"].as_bool() == Some(true) {
            let script = std::fs::read_to_string(cwd.join("giris-komutlari.txt")).ok();
            append(
                &cwd.join("fake-mcc-report.txt"),
                &format!("run={run} login_script_at_join={}", script.is_some()),
            );
            let _ = writeln!(
                stdout,
                r#"@@MCC@@{{"e":"joined","user":"Fake","host":"localhost","port":25565,"protocol":777}}"#
            );
            for line in script.iter().flat_map(|s| s.lines()) {
                if line.starts_with("send ") {
                    let _ = writeln!(stdout, "[Script] {line}");
                }
            }
            let _ = stdout.flush();
        } else if step["echoUntilQuit"].as_bool() == Some(true) {
            for line in input.by_ref().map_while(Result::ok) {
                append(&stdin_log, &line);
                let _ = writeln!(stdout, "[echo] {line}");
                let _ = stdout.flush();
                if line.trim() == "/quit" {
                    std::process::exit(0);
                }
            }
        } else if let Some(wanted) = step["waitLine"].as_str() {
            for line in input.by_ref().map_while(Result::ok) {
                append(&stdin_log, &line);
                if line.trim() == wanted {
                    break;
                }
            }
        } else if step["hang"].as_bool() == Some(true) {
            loop {
                std::thread::sleep(Duration::from_secs(3600));
            }
        } else if let Some(code) = step["exit"].as_i64() {
            std::process::exit(i32::try_from(code).unwrap_or(1));
        }
    }
    std::process::exit(0);
}

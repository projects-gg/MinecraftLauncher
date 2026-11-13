//! Spike 1 (and 3): the whole publisher against the real Cloudflare Realtime SFU, sharing a test
//! window of our own. Calls the SFU's HTTPS API directly (what the room's Durable Object does in
//! production). Secrets come from the file named by `PL_SHARE_SFU_ENV`, never printed; the publisher
//! session id goes only into `--info <file>` for the subscriber script.
//!
//! `cargo run -p pl-share --example sfu_publish -- --secs 90 --info <file> [--pause-at 20]
//!  [--resume-at 70] [--no-keepalive] [--preset low|normal|high] [--fps 30]`
//!
//! SQOL (live changes, between two test windows of our own, the second 4:3): `[--switch-at 15]`
//! (to the second window: new encoders, its size differs) `[--switch-back-at 25]` (a keyframe only)
//! `[--hold-at 30] [--unhold-at 36]` (a blank picture, then a fresh frame) `[--update-at 40]`
//! (`--update-preset` / `--update-fps`, default low / 15) `[--close-at 45]` (the shown window closes:
//! a blank picture and `sourceGone`, the share goes on) `[--preview]` (the sharer's own preview).
//! SQOL-R3: `[--still]`: the second window is painted once (a still source sends one frame: the
//! switch must not lose it; its still picture follows within seconds).

#[cfg(windows)]
#[path = "common/mod.rs"]
mod common;

#[cfg(not(windows))]
fn main() {}

#[cfg(windows)]
#[tokio::main(flavor = "current_thread")]
async fn main() {
    use std::time::{Duration, Instant};

    use pl_share::{
        AudioScope, Preset, ShareCmd, ShareController, ShareEnv, ShareEvent, ShareOpts, SourceId,
        Tuning,
    };
    use serde_json::{Value, json};

    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "pl_share=debug,str0m=warn".into()),
        )
        .init();

    let args: Vec<String> = std::env::args().collect();
    let arg = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    // SQOL-R3: `--window-only WxH [--still]`: this process only shows a test window (another
    // program's window for `--other-process`), says its handle, and ends when the window is gone.
    if let Some(spec) = arg("--window-only") {
        let (w, h) = spec
            .split_once('x')
            .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
            .unwrap_or((1280, 720));
        let hwnd = if args.iter().any(|a| a == "--still") {
            common::still_window(w, h)
        } else {
            common::test_window(w, h)
        };
        println!("hwnd {hwnd}");
        let _ = std::io::Write::flush(&mut std::io::stdout());
        while common::window_alive(hwnd) {
            std::thread::sleep(Duration::from_millis(100));
        }
        return;
    }
    let secs: u64 = arg("--secs").and_then(|s| s.parse().ok()).unwrap_or(60);
    let pause_at: Option<u64> = arg("--pause-at").and_then(|s| s.parse().ok());
    let resume_at: Option<u64> = arg("--resume-at").and_then(|s| s.parse().ok());
    let info_path = arg("--info");
    let fps: u8 = arg("--fps").and_then(|s| s.parse().ok()).unwrap_or(30);
    let preset_of = |name: Option<String>, default: Preset| match name.as_deref() {
        Some("low") => Preset::Low,
        Some("normal") => Preset::Normal,
        Some("high") => Preset::High,
        _ => default,
    };
    let preset = preset_of(arg("--preset"), Preset::Normal);
    // SQOL: the live changes, each once at its second.
    let at = |name: &str| arg(name).and_then(|s| s.parse::<u64>().ok());
    let mut plan: Vec<(u64, &str)> = [
        "--switch-at",
        "--switch-back-at",
        "--hold-at",
        "--unhold-at",
        "--update-at",
        "--close-at",
    ]
    .into_iter()
    .filter_map(|name| at(name).map(|second| (second, name)))
    .collect();
    plan.sort();
    let update_preset = preset_of(arg("--update-preset"), Preset::Low);
    let update_fps: u8 = arg("--update-fps")
        .and_then(|s| s.parse().ok())
        .unwrap_or(15);
    let preview = args.iter().any(|a| a == "--preview");
    // SQOL-R3 (the 2.2.0 crash): `--stress <changes>`: from 4 s on, a live quality or frame-rate
    // change every 0.6-1.8 s (new encoders nearly every time), keyframe requests every 0.1-0.6 s
    // in between (as watchers' PLIs); `--size WxH` the first window (E2E's 1026x800),
    // `--still-main` it painted once (keep-alives and keyframes repeat its last picture).
    let stress: u32 = arg("--stress").and_then(|s| s.parse().ok()).unwrap_or(0);
    let size: (i32, i32) = arg("--size")
        .and_then(|s| {
            let (w, h) = s.split_once('x')?;
            Some((w.parse().ok()?, h.parse().ok()?))
        })
        .unwrap_or((1280, 720));
    let still_main = args.iter().any(|a| a == "--still-main");
    if args.iter().any(|a| a == "--no-keepalive") {
        // SAFETY: set before any other thread starts.
        unsafe { std::env::set_var("PL_SHARE_TEST_NO_KEEPALIVE", "1") };
    }

    // Secrets: read, never printed.
    let env_path = std::env::var("PL_SHARE_SFU_ENV")
        .expect("PL_SHARE_SFU_ENV: a file with SFU_APP_ID= and SFU_APP_SECRET= lines");
    let env_file = std::fs::read_to_string(env_path).expect("SFU env file");
    let value = |key: &str| {
        env_file
            .lines()
            .find_map(|l| l.trim().strip_prefix(&format!("{key}=")))
            .map(|v| v.trim().trim_matches('"').to_string())
            .expect(key)
    };
    let app = value("SFU_APP_ID");
    let secret = value("SFU_APP_SECRET");
    let base = format!("https://rtc.live.cloudflare.com/v1/apps/{app}");
    let http = reqwest::Client::new();
    let call = |method: reqwest::Method, path: String, body: Option<Value>| {
        let request = http
            .request(method, format!("{base}{path}"))
            .bearer_auth(&secret);
        let request = match body {
            Some(b) => request.json(&b),
            None => request,
        };
        async move {
            let response = request.send().await.expect("sfu request");
            let status = response.status();
            let body: Value = response.json().await.unwrap_or(Value::Null);
            if !status.is_success() || body.get("errorCode").is_some() {
                println!(
                    "SFU error {status}: {}",
                    body.get("errorCode").unwrap_or(&Value::Null)
                );
            }
            body
        }
    };

    // SQOL-R3: `--other-process`: the first window belongs to a child process of its own (closing
    // it ends that program, as E2E's WinForms window did).
    let mut child = None;
    let (hwnd, main_pid) = if args.iter().any(|a| a == "--other-process") {
        let mut command = std::process::Command::new(std::env::current_exe().expect("exe"));
        command
            .arg("--window-only")
            .arg(format!("{}x{}", size.0, size.1));
        if still_main {
            command.arg("--still");
        }
        let mut spawned = command
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("the window's process");
        let mut line = String::new();
        let stdout = spawned.stdout.take().expect("its output");
        std::io::BufRead::read_line(&mut std::io::BufReader::new(stdout), &mut line)
            .expect("its window");
        let hwnd: isize = line
            .trim()
            .strip_prefix("hwnd ")
            .and_then(|s| s.parse().ok())
            .expect("a window handle");
        let pid = spawned.id();
        child = Some(spawned);
        (hwnd, pid)
    } else if still_main {
        (common::still_window(size.0, size.1), std::process::id())
    } else {
        (common::test_window(size.0, size.1), std::process::id())
    };
    // SQOL: the second window to switch to (4:3: another picture size).
    let still = args.iter().any(|a| a == "--still");
    let second = (!plan.is_empty()).then(|| {
        if still {
            common::still_window(1024, 768)
        } else {
            common::test_window(1024, 768)
        }
    });
    std::thread::sleep(Duration::from_millis(300));
    let main_window = hwnd;
    let window = |hwnd: isize| SourceId::Window {
        hwnd: hwnd as u32,
        pid: if hwnd == main_window {
            main_pid
        } else {
            std::process::id()
        },
    };
    let (controller, mut events) = ShareController::start(ShareEnv::default()).expect("controller");
    let opts = ShareOpts {
        preset,
        fps,
        tuning: Tuning::Motion,
        cursor: true,
        audio: true,
        audio_scope: AudioScope::App,
    };
    controller.send(ShareCmd::Prepare {
        n: 1,
        source: window(hwnd),
        opts,
    });

    let t0 = Instant::now();
    let mut session = String::new();
    let mut mids: Vec<(String, String)> = Vec::new();
    let mut paused = false;
    let mut stats_lines = 0;
    let mut previews = 0u32;
    let deadline = t0 + Duration::from_secs(secs);
    // SQOL-R3: the stress run's own clock and dice (xorshift: no dependency for an example).
    let mut seed =
        t0.elapsed().as_nanos() as u64 ^ 0x9e37_79b9_7f4a_7c15 ^ u64::from(std::process::id());
    let mut roll = move |lo: u64, hi: u64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        lo + seed % (hi - lo + 1)
    };
    const COMBOS: [(Preset, u8); 6] = [
        (Preset::High, 30),
        (Preset::Normal, 30),
        (Preset::High, 60),
        (Preset::Low, 15),
        (Preset::Normal, 15),
        (Preset::Low, 30),
    ];
    let mut changes = 0u32;
    let mut keys = 0u32;
    let mut next_change = t0 + Duration::from_secs(4);
    let mut next_key = t0 + Duration::from_secs(4);
    while Instant::now() < deadline {
        let tick = tokio::time::sleep(Duration::from_millis(if stress > 0 { 50 } else { 500 }));
        tokio::select! {
            event = events.recv() => {
                let Some(event) = event else { break };
                match event {
                    ShareEvent::Offer { n, sdp } => {
                        println!("offer: {} bytes, simulcast line: {}", sdp.len(),
                            sdp.lines().find(|l| l.starts_with("a=simulcast")).unwrap_or("-"));
                        let mut kind = "";
                        for line in sdp.lines() {
                            if line.starts_with("m=video") { kind = "video" }
                            if line.starts_with("m=audio") { kind = "audio" }
                            if let Some(mid) = line.strip_prefix("a=mid:") {
                                mids.push((mid.to_string(), kind.to_string()));
                            }
                        }
                        let s = call(reqwest::Method::POST, "/sessions/new".into(), None).await;
                        session = s["sessionId"].as_str().unwrap_or_default().to_string();
                        let tracks: Vec<Value> = mids.iter()
                            .map(|(mid, name)| json!({"location": "local", "mid": mid, "trackName": name}))
                            .collect();
                        let r = call(reqwest::Method::POST, format!("/sessions/{session}/tracks/new"),
                            Some(json!({"sessionDescription": {"type": "offer", "sdp": sdp}, "tracks": tracks}))).await;
                        let answer = r["sessionDescription"]["sdp"].as_str().unwrap_or_default().to_string();
                        for t in r["tracks"].as_array().into_iter().flatten() {
                            println!("track {}: {}", t["trackName"], t.get("errorCode").map(|e| e.to_string()).unwrap_or("ok".into()));
                        }
                        println!("answer: {} bytes, codec lines: {:?}", answer.len(),
                            answer.lines().filter(|l| l.starts_with("a=rtpmap")).collect::<Vec<_>>());
                        println!("answer feedback/extensions: {:?}", answer.lines().filter(|l| l.contains("transport-cc") || l.contains("transport-wide") || l.starts_with("a=rtcp-fb") || l.starts_with("a=extmap")).collect::<Vec<_>>());
                        println!("answer simulcast: {:?}", answer.lines().filter(|l| l.starts_with("a=simulcast") || l.starts_with("a=rid")).collect::<Vec<_>>());
                        if let Some(path) = &info_path {
                            let info = json!({"session": session, "tracks": mids.iter().map(|m| m.1.clone()).collect::<Vec<_>>()});
                            std::fs::write(path, info.to_string()).expect("info");
                        }
                        controller.send(ShareCmd::Answer { n, sdp: answer });
                    }
                    ShareEvent::Stats { stats, .. } => {
                        stats_lines += 1;
                        println!("[{:>5.1}s] stats {:?}", t0.elapsed().as_secs_f64(), stats);
                    }
                    ShareEvent::Thumb { n, jpg } => {
                        println!("[{:>5.1}s] thumb {n}: {} bytes", t0.elapsed().as_secs_f64(), jpg.len());
                    }
                    ShareEvent::PreviewFrame { n, jpg } => {
                        previews += 1;
                        if previews % 10 == 1 {
                            println!("[{:>5.1}s] preview {n}: {} bytes ({previews} so far)", t0.elapsed().as_secs_f64(), jpg.len());
                        }
                    }
                    ShareEvent::Started { n } => {
                        println!("[{:>5.1}s] started {n}", t0.elapsed().as_secs_f64());
                        if preview {
                            controller.send(ShareCmd::Preview { n, on: true });
                        }
                    }
                    other => println!("[{:>5.1}s] {other:?}", t0.elapsed().as_secs_f64()),
                }
            }
            _ = tick => {
                let now = Instant::now();
                if changes < stress && now >= next_change {
                    let (preset, fps) = COMBOS[(roll(0, 99) as usize + changes as usize) % COMBOS.len()];
                    changes += 1;
                    println!(
                        "[{:>5.1}s] stress change {changes}: {preset:?} {fps} fps (handles {})",
                        t0.elapsed().as_secs_f64(),
                        common::handles()
                    );
                    controller.send(ShareCmd::Update {
                        n: 1,
                        opts: ShareOpts { preset, fps, ..opts },
                    });
                    next_change = now + Duration::from_millis(roll(600, 1_800));
                }
                if stress > 0 && changes <= stress && now >= next_key {
                    keys += 1;
                    controller.send(ShareCmd::KeyFrame { n: 1 });
                    next_key = now + Duration::from_millis(roll(100, 600));
                }
                let at = t0.elapsed().as_secs();
                // SQOL: the live changes due now.
                while let Some(&(due, name)) = plan.first() && at >= due {
                    plan.remove(0);
                    println!("[{at}s] {name}");
                    let cmd = match name {
                        "--switch-at" => second.map(|other| ShareCmd::Switch { n: 1, source: window(other) }),
                        "--switch-back-at" => Some(ShareCmd::Switch { n: 1, source: window(hwnd) }),
                        "--hold-at" => Some(ShareCmd::Hold { n: 1, on: true }),
                        "--unhold-at" => Some(ShareCmd::Hold { n: 1, on: false }),
                        "--update-at" => Some(ShareCmd::Update {
                            n: 1,
                            opts: ShareOpts { preset: update_preset, fps: update_fps, ..opts },
                        }),
                        "--close-at" => {
                            // The first window closes (shown again after `--switch-back-at`):
                            // the share holds a blank picture.
                            common::close_window(hwnd);
                            None
                        }
                        _ => None,
                    };
                    if let Some(cmd) = cmd {
                        controller.send(cmd);
                    }
                }
                if !paused && pause_at.is_some_and(|p| at >= p) && resume_at.is_none_or(|r| at < r) {
                    paused = true;
                    println!("[{at}s] pause");
                    controller.send(ShareCmd::Pause { n: 1, paused: true });
                }
                if paused && resume_at.is_some_and(|r| at >= r) {
                    paused = false;
                    println!("[{at}s] resume");
                    controller.send(ShareCmd::Pause { n: 1, paused: false });
                }
            }
        }
    }
    if stress > 0 {
        println!(
            "stress done: {changes} changes, {keys} keyframe requests (handles {})",
            common::handles()
        );
    }
    println!("stopping ({stats_lines} stats lines)");
    controller.send(ShareCmd::Stop { n: 1 });
    if !session.is_empty() {
        let tracks: Vec<Value> = mids.iter().map(|(mid, _)| json!({"mid": mid})).collect();
        call(
            reqwest::Method::PUT,
            format!("/sessions/{session}/tracks/close"),
            Some(json!({"tracks": tracks, "force": true})),
        )
        .await;
    }
    tokio::time::sleep(Duration::from_millis(500)).await;
    let _ = hwnd;
    if let Some(mut child) = child {
        let _ = child.kill();
        let _ = child.wait();
    }
}

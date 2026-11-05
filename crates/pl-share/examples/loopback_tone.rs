//! Spike 4: process loopback INCLUDE/EXCLUDE with a quiet test tone from a child process of ours
//! (1 kHz at -46 dBFS, a few seconds). Only the tone's energy is measured; nothing is stored.
//!
//! `cargo run -p pl-share --example loopback_tone`

#[cfg(not(windows))]
fn main() {}

#[cfg(windows)]
fn main() {
    use std::collections::VecDeque;
    use std::time::{Duration, Instant};

    use wasapi::*;

    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("play") {
        let secs: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(6);
        initialize_mta().ok().unwrap();
        let device = DeviceEnumerator::new()
            .unwrap()
            .get_default_device(&Direction::Render)
            .unwrap();
        let mut client = device.get_iaudioclient().unwrap();
        let format = WaveFormat::new(32, 32, &SampleType::Float, 48000, 2, None);
        let (period, _) = client.get_device_period().unwrap();
        client
            .initialize_client(
                &format,
                &Direction::Render,
                &StreamMode::EventsShared {
                    autoconvert: true,
                    buffer_duration_hns: period,
                },
            )
            .unwrap();
        let event = client.set_get_eventhandle().unwrap();
        let render = client.get_audiorenderclient().unwrap();
        let mut t = 0u64;
        let mut write = |frames: usize| {
            let mut data = Vec::with_capacity(frames * 8);
            for _ in 0..frames {
                let s =
                    ((t as f64 / 48000.0 * 1000.0 * std::f64::consts::TAU).sin() * 0.005) as f32;
                t += 1;
                data.extend_from_slice(&s.to_le_bytes());
                data.extend_from_slice(&s.to_le_bytes());
            }
            render.write_to_device(frames, &data, None).unwrap();
        };
        write(client.get_available_space_in_frames().unwrap() as usize);
        client.start_stream().unwrap();
        let end = Instant::now() + Duration::from_secs(secs);
        while Instant::now() < end {
            if event.wait_for_event(1000).is_err() {
                break;
            }
            write(client.get_available_space_in_frames().unwrap() as usize);
        }
        let _ = client.stop_stream();
        return;
    }

    println!("windows build {}", pl_share::win::os_build());
    let exe = std::env::current_exe().unwrap();
    let mut child = std::process::Command::new(exe)
        .args(["play", "9"])
        .spawn()
        .unwrap();
    let pid = child.id();
    std::thread::sleep(Duration::from_millis(800));

    // 1 kHz energy relative to total energy (Goertzel over the left channel).
    let measure = |label: &str, target: u32, include: bool| {
        initialize_mta().ok().unwrap();
        let mut client = match AudioClient::new_application_loopback_client(target, include) {
            Ok(c) => c,
            Err(e) => {
                println!("{label}: activation failed: {e}");
                return;
            }
        };
        let format = WaveFormat::new(16, 16, &SampleType::Int, 48000, 2, None);
        if let Err(e) = client.initialize_client(
            &format,
            &Direction::Capture,
            &StreamMode::EventsShared {
                autoconvert: true,
                buffer_duration_hns: 200_000,
            },
        ) {
            println!("{label}: initialize failed: {e}");
            return;
        }
        let event = client.set_get_eventhandle().unwrap();
        let capture = client.get_audiocaptureclient().unwrap();
        client.start_stream().unwrap();
        let mut pcm: VecDeque<u8> = VecDeque::new();
        let end = Instant::now() + Duration::from_secs(2);
        let mut events = 0;
        while Instant::now() < end {
            if event.wait_for_event(500).is_ok() {
                events += 1;
                let _ = capture.read_from_device_to_deque(&mut pcm);
            }
        }
        let _ = client.stop_stream();
        let bytes: Vec<u8> = pcm.into_iter().collect();
        let left: Vec<f64> = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| f64::from(i16::from_le_bytes([c[0], c[1]])) / 32768.0)
            .collect();
        let n = left.len().max(1) as f64;
        let total: f64 = left.iter().map(|s| s * s).sum::<f64>() / n;
        let k = 2.0 * (std::f64::consts::TAU * 1000.0 / 48000.0).cos();
        let (mut s1, mut s2) = (0.0f64, 0.0f64);
        for &x in &left {
            let s0 = x + k * s1 - s2;
            s2 = s1;
            s1 = s0;
        }
        let power = (s1 * s1 + s2 * s2 - k * s1 * s2) / (n * n) * 2.0;
        let db = |p: f64| {
            if p > 0.0 {
                10.0 * p.log10()
            } else {
                f64::NEG_INFINITY
            }
        };
        println!(
            "{label}: {events} events, {:.2} s of audio, 1 kHz {:.1} dBFS, total {:.1} dBFS",
            left.len() as f64 / 48000.0,
            db(power),
            db(total)
        );
    };
    measure("INCLUDE tone process", pid, true);
    measure("EXCLUDE tone process", pid, false);
    measure("INCLUDE this process (silent)", std::process::id(), true);

    // Our own pipeline: Opus packets from the tone, plus keep-alive when silent.
    let packets = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let p2 = packets.clone();
    let handle = pl_share::win::audio::start(
        Some(pl_share::win::audio::AudioTarget::Include(pid)),
        Instant::now(),
        Box::new(move |_data, _at, _rtp| {
            p2.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }),
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        Box::new(|| println!("pipeline: audio failed")),
    )
    .unwrap();
    std::thread::sleep(Duration::from_secs(2));
    println!(
        "pipeline: {} Opus packets in 2 s (50/s expected while the tone plays)",
        packets.load(std::sync::atomic::Ordering::Relaxed)
    );
    drop(handle);
    let _ = child.wait();
}

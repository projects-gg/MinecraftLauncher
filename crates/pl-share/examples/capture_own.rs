//! Spike 3: Windows.Graphics.Capture on a window of our own (frame rate, conversion, cursor and
//! border switches) and, with `--wda`, whether `WDA_EXCLUDEFROMCAPTURE` hides a small window of
//! ours from a monitor capture (only the pixels under that window are looked at, in memory; nothing
//! is stored).
//!
//! `cargo run -p pl-share --example capture_own [-- --wda]`

#[cfg(windows)]
#[path = "common/mod.rs"]
mod common;

#[cfg(not(windows))]
fn main() {}

#[cfg(windows)]
fn main() {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use pl_share::win::capture::{self, Capture};
    use pl_share::win::convert::Scaler;
    use pl_share::win::d3d::Gpu;
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::Graphics::Direct3D11::{
        D3D11_BOX, D3D11_CPU_ACCESS_READ, D3D11_MAP_READ, D3D11_MAPPED_SUBRESOURCE,
        D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
    };
    use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowRect, HWND_TOPMOST, SWP_NOACTIVATE, SetWindowPos,
    };

    pl_share::win::com_mta();
    println!(
        "build {}, WGC {}, cursor toggle {}, border toggle {}",
        pl_share::win::os_build(),
        capture::supported(),
        capture::cursor_toggle_supported(),
        capture::border_toggle_supported()
    );
    let gpu = Arc::new(Gpu::new().unwrap());
    let hwnd = common::test_window(1280, 720);
    std::thread::sleep(Duration::from_millis(300));

    let item = capture::item_for_window(hwnd).unwrap();
    println!("item size {:?}", capture::item_size(&item));
    let frames = Arc::new(AtomicU32::new(0));
    let converted = Arc::new(AtomicU32::new(0));
    let scaler = Arc::new(Mutex::new(Scaler::new(gpu.clone(), (1280, 720)).unwrap()));
    let (f2, c2, s2) = (frames.clone(), converted.clone(), scaler.clone());
    let cpu0 = common::cpu_seconds();
    let cap = Capture::start(
        &gpu,
        item,
        false,
        Box::new(move |tex, content, _| {
            f2.fetch_add(1, Ordering::Relaxed);
            if s2.lock().unwrap().convert(tex, content).is_ok() {
                c2.fetch_add(1, Ordering::Relaxed);
            }
        }),
        Box::new(|| println!("closed")),
    )
    .unwrap();
    let start = Instant::now();
    std::thread::sleep(Duration::from_secs(5));
    let secs = start.elapsed().as_secs_f64();
    println!(
        "own window: {:.1} fps arrived, {} converted, CPU {:.1}% of one core (window painting included)",
        frames.load(Ordering::Relaxed) as f64 / secs,
        converted.load(Ordering::Relaxed),
        (common::cpu_seconds() - cpu0) / secs * 100.0
    );
    cap.set_cursor(true);
    drop(cap);

    if !std::env::args().any(|a| a == "--wda") {
        return;
    }
    // A small magenta-ish window on top (no focus), then the primary monitor's capture: count how
    // many pixels under it are the window's colour, with and without WDA_EXCLUDEFROMCAPTURE.
    let probe = common::test_window(160, 120);
    std::thread::sleep(Duration::from_millis(200));
    unsafe {
        let _ = SetWindowPos(
            HWND(probe as *mut _),
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            windows::Win32::UI::WindowsAndMessaging::SWP_NOMOVE
                | windows::Win32::UI::WindowsAndMessaging::SWP_NOSIZE
                | SWP_NOACTIVATE,
        );
    }
    std::thread::sleep(Duration::from_millis(300));
    let mut rect = RECT::default();
    unsafe { GetWindowRect(HWND(probe as *mut _), &mut rect).unwrap() };
    let monitor = pl_share::win::sources::monitors()
        .into_iter()
        .find(|m| m.primary)
        .unwrap();
    let region = D3D11_BOX {
        left: (rect.left - monitor.rect.left + 20).max(0) as u32,
        top: (rect.top - monitor.rect.top + 60).max(0) as u32,
        right: (rect.left - monitor.rect.left + 120).max(1) as u32,
        bottom: (rect.top - monitor.rect.top + 100).max(1) as u32,
        front: 0,
        back: 1,
    };
    let sample = |label: &str| {
        let staging = unsafe {
            let desc = D3D11_TEXTURE2D_DESC {
                Width: region.right - region.left,
                Height: region.bottom - region.top,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: D3D11_USAGE_STAGING,
                BindFlags: 0,
                CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                MiscFlags: 0,
            };
            let mut t = None;
            gpu.device
                .CreateTexture2D(&desc, None, Some(&mut t))
                .unwrap();
            t.unwrap()
        };
        let result = Arc::new(Mutex::new(None::<(u32, u32)>));
        let (r2, g2, st) = (result.clone(), gpu.clone(), pl_share::win::SendCom(staging));
        let item = capture::item_for_monitor(monitor.handle).unwrap();
        let cap = Capture::start(
            &gpu,
            item,
            false,
            Box::new(move |tex, _, _| unsafe {
                let st = &st;
                let mut out = r2.lock().unwrap();
                if out.is_some() {
                    return;
                }
                let _l = g2.lock();
                g2.context
                    .CopySubresourceRegion(&st.0, 0, 0, 0, 0, tex, 0, Some(&region));
                let mut map = D3D11_MAPPED_SUBRESOURCE::default();
                g2.context
                    .Map(&st.0, 0, D3D11_MAP_READ, 0, Some(&mut map))
                    .unwrap();
                let (w, h) = (region.right - region.left, region.bottom - region.top);
                let mut ours = 0;
                for y in 0..h {
                    let row = std::slice::from_raw_parts(
                        (map.pData as *const u8).add((y * map.RowPitch) as usize),
                        (w * 4) as usize,
                    );
                    for px in row.as_chunks::<4>().0 {
                        // The test window's background: B 0x30, G 0x20, R 0x20..0x5e.
                        if px[0] == 0x30 && px[1] == 0x20 && (0x20..0x60).contains(&px[2]) {
                            ours += 1;
                        }
                    }
                }
                g2.context.Unmap(&st.0, 0);
                *out = Some((ours, w * h));
            }),
            Box::new(|| {}),
        )
        .unwrap();
        let until = Instant::now() + Duration::from_secs(3);
        while result.lock().unwrap().is_none() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(50));
        }
        drop(cap);
        println!(
            "{label}: {:?} (pixels of our window colour, region size)",
            result.lock().unwrap()
        );
    };
    sample("monitor capture, window visible");
    println!(
        "exclude_from_capture: {}",
        pl_share::exclude_from_capture(probe, true)
    );
    std::thread::sleep(Duration::from_millis(200));
    sample("monitor capture, WDA_EXCLUDEFROMCAPTURE");
}

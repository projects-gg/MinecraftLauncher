//! Spike 2: which H.264 encoders exist, and what our GPU pipeline (scale/convert + MF encode)
//! costs in CPU at 720p30 and 1080p60. Frames are synthetic (a scrolling noise texture made on
//! the GPU), nothing is captured or stored.
//!
//! `cargo run -p pl-share --example mft_bench --release`

#[cfg(windows)]
#[path = "common/mod.rs"]
mod common;

#[cfg(not(windows))]
fn main() {}

#[cfg(windows)]
fn main() {
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use pl_share::preset::Layer;
    use pl_share::transport::H264Profile;
    use pl_share::win::SendCom;
    use pl_share::win::convert::Scaler;
    use pl_share::win::d3d::Gpu;
    use pl_share::win::encoder::{self, EncEvent, EncIn, EncoderConfig};
    use windows::Win32::Graphics::Direct3D11::{
        D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_BOX, D3D11_SUBRESOURCE_DATA,
        D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT,
    };
    use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};

    pl_share::win::com_mta();
    println!("== H.264 encoders (Media Foundation)");
    for e in encoder::list() {
        println!(
            "  {:<45} hardware={} async={} vendor={}",
            e.name, e.hardware, e.is_async, e.vendor_id
        );
    }
    let gpu = Arc::new(Gpu::new().expect("d3d11"));
    println!(
        "== adapter: {} (vendor {:#06x})",
        gpu.adapter, gpu.vendor_id
    );
    println!(
        "== hardware for this adapter: {:?}",
        encoder::hardware_name(gpu.luid)
    );

    // A 2x-wide noise texture; each frame copies a shifted 1920x1080 window of it.
    let (sw, sh) = (3840u32, 1080u32);
    let mut seed = 0x1234_5678u32;
    // Game-like content: blocky colour regions with fine texture (light noise), scrolling.
    let noise: Vec<u8> = (0..sw * sh * 4)
        .map(|i| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let p = i / 4;
            let (x, y, c) = (p % sw, p / sw, i % 4);
            let block = ((x / 24) * 37 + (y / 24) * 91 + c * 53) % 200;
            (block as u8).wrapping_add((seed >> 28) as u8)
        })
        .collect();
    let make = |w: u32, h: u32, data: Option<&[u8]>| unsafe {
        let desc = D3D11_TEXTURE2D_DESC {
            Width: w,
            Height: h,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: (D3D11_BIND_SHADER_RESOURCE.0 | D3D11_BIND_RENDER_TARGET.0) as u32,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };
        let init = data.map(|d| D3D11_SUBRESOURCE_DATA {
            pSysMem: d.as_ptr().cast(),
            SysMemPitch: w * 4,
            SysMemSlicePitch: 0,
        });
        let mut t = None;
        gpu.device
            .CreateTexture2D(&desc, init.as_ref().map(|i| i as *const _), Some(&mut t))
            .unwrap();
        t.unwrap()
    };
    let source = make(sw, sh, Some(&noise));
    let frame = make(1920, 1080, None);

    let cpus = common::cpus();
    for (label, w, h, fps, kbps, software) in [
        ("720p30 hw", 1280u32, 720u32, 30u8, 2500u32, false),
        ("1080p60 hw", 1920, 1080, 60, 6000, false),
        ("720p30 sw", 1280, 720, 30, 2500, true),
    ] {
        let stats = Arc::new(Mutex::new((
            0u64,
            0u32,
            0u32,
            Vec::<(Instant, bool)>::new(),
        )));
        let s2 = stats.clone();
        let epoch = Instant::now();
        let handle = encoder::spawn(
            gpu.clone(),
            EncoderConfig {
                layer: Layer::Full,
                w,
                h,
                fps,
                kbps,
                max_kbps: kbps,
                profile: H264Profile::ConstrainedHigh,
                epoch,
                software,
            },
            Box::new(move |e| {
                if let EncEvent::Frame(f) = e {
                    let mut s = s2.lock().unwrap();
                    s.0 += f.data.len() as u64;
                    s.1 += 1;
                    if f.key {
                        s.2 += 1;
                    }
                    s.3.push((Instant::now(), f.key));
                }
            }),
        );
        let handle = match handle {
            Ok(h) => h,
            Err(e) => {
                println!("== {label}: no encoder ({e})");
                continue;
            }
        };
        println!("== {label}: {} ({})", handle.name, handle.settings);
        let mut scaler = Scaler::new(gpu.clone(), (w, h)).unwrap();
        let interval = Duration::from_micros(1_000_000 / u64::from(fps));
        let run = Duration::from_secs(10);
        let cpu0 = common::cpu_seconds();
        let start = Instant::now();
        let mut next = start;
        let mut i = 0u32;
        let mut key_asked: Option<Instant> = None;
        let mut halved = false;
        let mut bytes_half = 0u64;
        while start.elapsed() < run {
            let offset = (i * 6) % (sw - 1920);
            unsafe {
                let b = D3D11_BOX {
                    left: offset,
                    top: 0,
                    front: 0,
                    right: offset + 1920,
                    bottom: 1080,
                    back: 1,
                };
                let _l = gpu.lock();
                gpu.context
                    .CopySubresourceRegion(&frame, 0, 0, 0, 0, &source, 0, Some(&b));
            }
            let out = match scaler.convert(&frame, (1920, 1080)) {
                Ok(out) => out,
                Err(e) => {
                    println!("convert failed: {e}");
                    break;
                }
            };
            handle.send(EncIn::Frame {
                texture: SendCom(out),
                at: Instant::now(),
            });
            i += 1;
            if i == fps as u32 * 3 {
                key_asked = Some(Instant::now());
                handle.send(EncIn::KeyFrame);
            }
            if !halved && start.elapsed() >= run / 2 {
                halved = true;
                bytes_half = stats.lock().unwrap().0;
                handle.send(EncIn::Rate {
                    kbps: kbps / 2,
                    fps,
                });
            }
            next += interval;
            if let Some(d) = next.checked_duration_since(Instant::now()) {
                std::thread::sleep(d);
            }
        }
        std::thread::sleep(Duration::from_millis(300));
        let cpu = common::cpu_seconds() - cpu0;
        let secs = start.elapsed().as_secs_f64();
        drop(handle);
        let s = stats.lock().unwrap();
        let first_half = bytes_half as f64 * 8.0 / 1000.0 / (secs / 2.0);
        let second_half = (s.0 - bytes_half) as f64 * 8.0 / 1000.0 / (secs / 2.0);
        let key_latency = key_asked.and_then(|t| {
            s.3.iter()
                .find(|(at, key)| *key && *at >= t)
                .map(|(at, _)| at.duration_since(t).as_millis())
        });
        println!(
            "== {label}: in {i} out {} frames, keyframes {}, kbps first half {:.0} (target {kbps}) second half {:.0} (target {}), forced keyframe after {:?} ms, CPU {:.1}% of one core = {:.2}% of {} logical",
            s.1,
            s.2,
            first_half,
            second_half,
            kbps / 2,
            key_latency,
            cpu / secs * 100.0,
            cpu / secs * 100.0 / cpus,
            cpus
        );
    }
}

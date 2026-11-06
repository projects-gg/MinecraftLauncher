//! The taskbar button's overlay badge (Windows): a small mark in the corner of the launcher's
//! taskbar button, with the words a screen reader says for it. SHARE-LIVE uses it while the player
//! shares their screen ("Yayındasın").
//!
//! The mark is drawn here at the taskbar's small-icon size (nothing to scale, sharp at every DPI):
//! a jade disc (the design's `jade-400`, the colour of what is live) with a thin dark rim that keeps
//! it apart from a light taskbar, and the broadcast glyph (a dot between arcs) in the panels' dark
//! `obsidian-900`. No red: the launcher's "Yayında" is calm.

/// `jade-400`.
const JADE: [u8; 3] = [0x5F, 0xCB, 0x8E];
/// `obsidian-900`: the glyph.
const INK: [u8; 3] = [0x12, 0x10, 0x16];
/// `obsidian-950`: the rim.
const RIM: [u8; 3] = [0x0A, 0x09, 0x0C];

/// The live mark's pixels: `size` × `size` RGBA, straight alpha (`size` held to 8–256).
pub fn live_badge(size: u32) -> Vec<u8> {
    let size = size.clamp(8, 256);
    const SAMPLES: u32 = 4;
    // One pixel in the mark's own units (the disc is the square from -1 to 1).
    let pixel = 2.0 / size as f32;
    let outer = 1.0 - pixel * 0.25;
    let rim = pixel.max(0.06);
    let dot = 0.18_f32.max(pixel * 1.1);
    let stroke = 0.13_f32.max(pixel);
    // Two arcs a side where there is room for them, one on the smallest marks.
    let arcs: &[f32] = if size >= 24 { &[0.46, 0.72] } else { &[0.58] };
    let mut out = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let mut colour = [0f32; 3];
            let mut covered = 0u32;
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let u = (x as f32 + (sx as f32 + 0.5) / SAMPLES as f32) * pixel - 1.0;
                    let v = (y as f32 + (sy as f32 + 0.5) / SAMPLES as f32) * pixel - 1.0;
                    let r = (u * u + v * v).sqrt();
                    if r > outer {
                        continue;
                    }
                    // The arcs face left and right: within 45 degrees of the middle line.
                    let arc =
                        v.abs() <= u.abs() && arcs.iter().any(|at| (r - at).abs() <= stroke / 2.0);
                    let paint = if r > outer - rim {
                        RIM
                    } else if r <= dot || arc {
                        INK
                    } else {
                        JADE
                    };
                    for (sum, channel) in colour.iter_mut().zip(paint) {
                        *sum += f32::from(channel);
                    }
                    covered += 1;
                }
            }
            if covered == 0 {
                continue;
            }
            let at = ((y * size + x) * 4) as usize;
            for (channel, sum) in out[at..at + 3].iter_mut().zip(colour) {
                *channel = (sum / covered as f32).round() as u8;
            }
            out[at + 3] = ((covered * 255) as f32 / (SAMPLES * SAMPLES) as f32).round() as u8;
        }
    }
    out
}

/// The taskbar's small-icon size on this computer (16 at 100 %, 24 at 150 %, …).
#[cfg(windows)]
pub fn small_icon_size() -> u32 {
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSMICON};
    // SAFETY: a plain query.
    let size = unsafe { GetSystemMetrics(SM_CXSMICON) };
    u32::try_from(size).unwrap_or(16).clamp(16, 64)
}

/// Sets (`Some((rgba, size))`, a square of straight-alpha pixels) or clears (`None`) the overlay
/// badge on the taskbar button of `hwnd`, with `description` for screen readers. Call it on the
/// window's own thread, in its apartment (the UI thread); the taskbar keeps its own copy.
#[cfg(windows)]
pub fn set_overlay(
    hwnd: isize,
    badge: Option<(&[u8], u32)>,
    description: &str,
) -> windows::core::Result<()> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
    use windows::Win32::UI::Shell::{ITaskbarList3, TaskbarList};
    use windows::Win32::UI::WindowsAndMessaging::{CreateIcon, DestroyIcon, HICON};
    use windows::core::{HSTRING, PCWSTR};

    // SAFETY: COM on the calling thread's apartment; the icon is made from buffers that outlive the
    // call and destroyed after the taskbar copied it.
    unsafe {
        let taskbar: ITaskbarList3 = CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER)?;
        taskbar.HrInit()?;
        let icon = match badge {
            Some((rgba, size)) if rgba.len() == (size * size * 4) as usize => {
                let bgra: Vec<u8> = rgba
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .flat_map(|pixel| [pixel[2], pixel[1], pixel[0], pixel[3]])
                    .collect();
                // The mask's rows are whole 16-bit words; with alpha in the colour it shows all.
                let mask = vec![0u8; size.div_ceil(16) as usize * 2 * size as usize];
                Some(CreateIcon(
                    None,
                    size as i32,
                    size as i32,
                    1,
                    32,
                    mask.as_ptr(),
                    bgra.as_ptr(),
                )?)
            }
            _ => None,
        };
        let text = HSTRING::from(description);
        let result = taskbar.SetOverlayIcon(
            HWND(hwnd as *mut _),
            icon.unwrap_or(HICON::default()),
            if icon.is_some() {
                PCWSTR(text.as_ptr())
            } else {
                PCWSTR::null()
            },
        );
        if let Some(icon) = icon {
            let _ = DestroyIcon(icon);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(badge: &[u8], size: u32, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * size + x) * 4) as usize;
        [badge[at], badge[at + 1], badge[at + 2], badge[at + 3]]
    }

    #[test]
    fn the_live_mark_is_a_jade_disc_with_a_dark_glyph() {
        for size in [16, 20, 24, 32] {
            let badge = live_badge(size);
            assert_eq!(badge.len(), (size * size * 4) as usize);
            // Round: the corners are clear, the middle is the glyph's dot.
            assert_eq!(pixel(&badge, size, 0, 0)[3], 0, "{size}");
            assert_eq!(pixel(&badge, size, size - 1, size - 1)[3], 0, "{size}");
            let middle = pixel(&badge, size, size / 2, size / 2);
            assert_eq!(middle[3], 255);
            assert!(middle[..3].iter().all(|c| *c < 40), "{size}: {middle:?}");
            // Above the dot (no arc there) the disc shows jade.
            let above = pixel(&badge, size, size / 2, size * 3 / 10);
            assert_eq!(above[..3], JADE, "{size}");
            assert_eq!(above[3], 255);
        }
        // An arc beside the dot, on the larger marks.
        let badge = live_badge(32);
        let arc = pixel(&badge, 32, 16 + 7, 16);
        assert!(arc[..3].iter().all(|c| *c < 60), "{arc:?}");
        // Held to its range.
        assert_eq!(live_badge(1).len(), 8 * 8 * 4);
    }
}

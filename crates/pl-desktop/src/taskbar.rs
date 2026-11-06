//! The taskbar button's overlay badge (Windows): a small mark in the corner of the launcher's
//! taskbar button, with the words a screen reader says for it. SHARE-LIVE uses it while the player
//! shares their screen ("Yayındasın"); the mark's art is the caller's.

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

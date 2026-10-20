//! The main window: its default size, chosen from the screen it first opens on, and how it is
//! brought to the front.
//!
//! The launcher opens at about 62% of the screen's width (a calm size next to other windows),
//! never smaller than the layout's minimum and never larger than the old fixed default. After
//! that the player's own size and position are remembered (the window-state plugin).

/// The smallest size the layout is designed for; also the window's minimum size.
pub const MIN_SIZE: (u32, u32) = (1024, 640);

/// The largest default size. Players can still make the window bigger or maximize it.
pub const MAX_SIZE: (u32, u32) = (1280, 760);

/// Share of the work area's width the default window takes.
const WIDTH_SHARE: f64 = 0.625;

/// At most this share of the work area's height (short, wide screens).
const HEIGHT_SHARE_MAX: f64 = 0.92;

/// The default window size, in logical pixels, for a monitor whose work area (the screen without
/// the taskbar or dock) is `work_width` × `work_height` logical pixels.
///
/// Width is 62.5% of the work area's width; height follows the layout's aspect ratio
/// (1280 × 760). Both stay between [`MIN_SIZE`] and [`MAX_SIZE`], and the height stays inside
/// the work area unless that would go below the minimum. Sizes are even numbers.
///
/// A 1920 × 1080 screen at 100% (work area 1920 × 1032) gets 1200 × 712; 1600 × 900 and
/// 1366 × 768 screens get the minimum; 2560 × 1440 gets 1280 × 760.
pub fn default_window_size(work_width: f64, work_height: f64) -> (u32, u32) {
    let known = |value: f64| value.is_finite() && value > 0.0;
    if !known(work_width) || !known(work_height) {
        // No usable monitor information: assume the most common desktop, 1920 × 1080 at 100%.
        return default_window_size(1920.0, 1032.0);
    }
    let (min_w, min_h) = (f64::from(MIN_SIZE.0), f64::from(MIN_SIZE.1));
    let (max_w, max_h) = (f64::from(MAX_SIZE.0), f64::from(MAX_SIZE.1));
    let even = |value: f64| (value / 2.0).round() * 2.0;

    let width = even(work_width * WIDTH_SHARE).clamp(min_w, max_w);
    let height = even(width * max_h / max_w)
        .min(even(work_height * HEIGHT_SHARE_MAX))
        .clamp(min_h, max_h);
    // Both values are clamped to small positive ranges above, so the casts are exact.
    (width as u32, height as u32)
}

/// What bringing the main window to the front came to (Windows).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Raise {
    /// It already was the foreground window: nothing was done.
    AlreadyInFront,
    /// Windows let the launcher take the foreground.
    Raised,
    /// Windows refused (the player is in another app that did not hand the right over): the
    /// taskbar button flashes instead.
    Flashed,
}

/// How many times the taskbar button flashes when Windows refuses the foreground. Windows then
/// keeps the button lit until the player switches to the launcher.
pub const FLASH_COUNT: u32 = 3;

/// Brings a window to the front only the way Windows allows it. The platform calls are passed in:
/// `set_foreground` (`SetForegroundWindow`) runs only when the window is not in front already,
/// `flash` (`FlashWindowEx`) only when Windows refused. Never synthetic input: tao's `set_focus`
/// presses Alt with `SendInput` when refused, so every tray click, second launch, `projects://`
/// link and game end sent a fake key press to the app the player was in (E2E2, 19 Oct 2026).
/// Windows normally allows it where the player asked for the launcher: a second launch hands the
/// running launcher its right first (tauri-plugin-single-instance calls
/// `AllowSetForegroundWindow`), and a click on the tray icon comes from the shell.
pub fn raise_window(
    in_front: bool,
    set_foreground: impl FnOnce() -> bool,
    flash: impl FnOnce(),
) -> Raise {
    if in_front {
        return Raise::AlreadyInFront;
    }
    if set_foreground() {
        return Raise::Raised;
    }
    flash();
    Raise::Flashed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::launcher_sources;

    #[test]
    fn common_screens() {
        // 1920 × 1080 at 100% with a 48 px taskbar: the owner's main screen.
        assert_eq!(default_window_size(1920.0, 1032.0), (1200, 712));
        // 1600 × 900 and 1366 × 768: the minimum.
        assert_eq!(default_window_size(1600.0, 852.0), (1024, 640));
        assert_eq!(default_window_size(1366.0, 728.0), (1024, 640));
        // 1920 × 1080 at 125% is 1536 × 826 logical.
        assert_eq!(default_window_size(1536.0, 826.0), (1024, 640));
        // 1680 × 1050: wider than the minimum, height at the minimum.
        assert_eq!(default_window_size(1680.0, 1002.0), (1050, 640));
        // 2560 × 1440 and 4K at 150%: the old default, never more.
        assert_eq!(default_window_size(2560.0, 1392.0), (1280, 760));
        assert_eq!(default_window_size(2560.0, 1400.0), (1280, 760));
    }

    #[test]
    fn short_or_odd_screens() {
        // Ultrawide 2560 × 1080: width capped, height follows.
        assert_eq!(default_window_size(2560.0, 1032.0), (1280, 760));
        // A short wide work area keeps the window inside it.
        assert_eq!(default_window_size(1920.0, 760.0), (1200, 700));
        // Smaller than the minimum: the minimum (the window enforces it anyway).
        assert_eq!(default_window_size(800.0, 560.0), (1024, 640));
    }

    #[test]
    fn unknown_work_area_assumes_a_1080p_screen() {
        assert_eq!(default_window_size(0.0, 0.0), (1200, 712));
        assert_eq!(default_window_size(f64::NAN, f64::INFINITY), (1200, 712));
        assert_eq!(default_window_size(-5.0, 900.0), (1200, 712));
    }

    /// The launcher's main window starts hidden and without the focus: a start in the tray
    /// (`--minimized`) must not make the invisible window the foreground one (E2E1); showing it
    /// activates it (`apps/launcher/src-tauri/src/shell.rs` → `start_minimized`).
    #[test]
    fn the_launcher_window_starts_hidden_without_the_focus() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../apps/launcher/src-tauri/tauri.conf.json"
        );
        let config = std::fs::read_to_string(path).expect("launcher tauri.conf.json");
        let main = config
            .split('{')
            .find(|object| object.contains("\"label\": \"main\""))
            .expect("the main window in tauri.conf.json");
        let main = &main[..main.find('}').unwrap_or(main.len())];
        assert!(main.contains("\"visible\": false"), "{main}");
        assert!(main.contains("\"focus\": false"), "{main}");
    }

    #[test]
    fn raising_goes_no_further_than_windows_allows() {
        use std::cell::Cell;

        let (asked, flashed) = (Cell::new(0), Cell::new(0));
        let raise = |in_front: bool, allowed: bool| {
            raise_window(
                in_front,
                || {
                    asked.set(asked.get() + 1);
                    allowed
                },
                || flashed.set(flashed.get() + 1),
            )
        };

        // In front already: Windows is not asked and nothing flashes.
        assert_eq!(raise(true, false), Raise::AlreadyInFront);
        assert_eq!((asked.get(), flashed.get()), (0, 0));
        // Allowed: raised, no flash.
        assert_eq!(raise(false, true), Raise::Raised);
        assert_eq!((asked.get(), flashed.get()), (1, 0));
        // Refused: asked once, then the taskbar flashes; nothing else is tried.
        assert_eq!(raise(false, false), Raise::Flashed);
        assert_eq!((asked.get(), flashed.get()), (2, 1));
    }

    /// The launcher brings its window forward only through [`raise_window`] on Windows: no
    /// `setFocus()` from the page, and the shell's `set_focus()` only where Windows is not.
    #[test]
    fn the_launcher_never_forces_the_foreground_on_windows() {
        for (path, lines) in launcher_sources("src", &["ts", "tsx"]) {
            for (number, line) in lines.iter().enumerate() {
                assert!(
                    !line.contains(".setFocus("),
                    "{path}:{}: the page must not call setFocus (tao presses Alt when Windows \
                     refuses); use commands.windowBringForward()",
                    number + 1
                );
            }
        }
        for (path, lines) in launcher_sources("src-tauri/src", &["rs"]) {
            for (number, line) in lines.iter().enumerate() {
                if !line.contains(".set_focus()") || line.trim_start().starts_with("//") {
                    continue;
                }
                let before = &lines[number.saturating_sub(4)..number];
                assert!(
                    before.iter().any(|l| l.contains("cfg(not(windows))")),
                    "{path}:{}: set_focus() outside a cfg(not(windows)) item (tao presses Alt \
                     when Windows refuses the foreground); use pl_desktop::window::raise_window",
                    number + 1
                );
            }
        }
    }
}

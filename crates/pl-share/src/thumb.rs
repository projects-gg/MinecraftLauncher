//! A live share's still picture for people who do not watch it (SHARE-LIVE, the lead's contract
//! `docs/tasks/share-live.md` §5): the room keeps the newest one and hands it to a member list's
//! hover card. About 3 s after the share went live, then once a minute, a JPEG at most [`SIDE`]
//! pixels on its long side (aspect kept) and [`MAX_BYTES`] long, made on the GPU from the picture
//! the encoder has (while encoding rests, from the next captured frame) with one small readback and
//! WIC. It only goes to the room ([`crate::ShareEvent::Thumb`]): nothing is written to disk.
//!
//! A still source sends the same picture again (no GPU work, no encoding): the room hands out none
//! older than a few minutes, and the time of the newest one tells watchers' caches it is current.

use std::time::Duration;

/// The first picture this long after the share went live (the first frames are in by then).
pub const FIRST: Duration = Duration::from_secs(3);
/// Then one a minute (the room takes one per share every 20 s at most).
pub const EVERY: Duration = Duration::from_secs(60);
/// No picture could be made yet (nothing captured, a minimized window): asked again this soon.
pub const RETRY: Duration = Duration::from_secs(10);
/// While encoding rests, a captured frame is awaited this long; then the encoder's last one is used.
pub const FRAME_WAIT: Duration = Duration::from_secs(1);
/// The picture's long side at most, in pixels (a 16:9 share: 320 × 180).
pub const SIDE: u32 = 320;
/// One picture's bytes at most. The room takes up to 24 KB, but at this size its base64 keeps the
/// whole `share_thumb` frame under the room socket's ordinary 16 KB frame limit: a room from before
/// SHARE-LIVE only answers `bad_request` (it would close the socket on a larger frame), and a
/// 320 px picture needs no more.
pub const MAX_BYTES: usize = 12_000;
/// JPEG qualities tried in turn until the picture fits.
pub const QUALITIES: [f32; 4] = [0.70, 0.55, 0.40, 0.25];

/// SQOL: the sharer's own live preview (the panel's card and their own tile on the stage): its long
/// side, at most this many a second (only when a new frame came), and its JPEG.
pub const PREVIEW_SIDE: u32 = 640;
pub const PREVIEW_PER_SECOND: u32 = 2;
pub const PREVIEW_EVERY: Duration = Duration::from_millis(1_000 / PREVIEW_PER_SECOND as u64);
/// A preview goes to the launcher's own page only: a bit larger than the room's picture is fine.
pub const PREVIEW_MAX_BYTES: usize = 60_000;
pub const PREVIEW_QUALITIES: [f32; 3] = [0.60, 0.45, 0.30];

/// The picture's size for content of `w` × `h`: the aspect kept, the long side [`SIDE`] (a smaller
/// source is not enlarged), both sides even and at least 2.
pub fn size(w: u32, h: u32) -> (u32, u32) {
    size_within(w, h, SIDE)
}

/// The size for content of `w` × `h` with the long side at most `side`, as [`size`].
pub fn size_within(w: u32, h: u32, side: u32) -> (u32, u32) {
    let (w, h) = (w.max(2), h.max(2));
    let scale = (f64::from(side.max(2)) / f64::from(w.max(h))).min(1.0);
    let even = |v: u32| ((f64::from(v) * scale).round() as u32).max(2) & !1;
    (even(w), even(h))
}

/// The first of [`QUALITIES`] whose picture fits in [`MAX_BYTES`]; none when no quality fits (or
/// the encoder failed every time).
pub fn fit(encode: impl FnMut(f32) -> Option<Vec<u8>>) -> Option<Vec<u8>> {
    fit_in(&QUALITIES, MAX_BYTES, encode)
}

/// SQOL: the preview's JPEG, as [`fit`] with [`PREVIEW_QUALITIES`] and [`PREVIEW_MAX_BYTES`].
pub fn fit_preview(encode: impl FnMut(f32) -> Option<Vec<u8>>) -> Option<Vec<u8>> {
    fit_in(&PREVIEW_QUALITIES, PREVIEW_MAX_BYTES, encode)
}

fn fit_in(
    qualities: &[f32],
    max: usize,
    mut encode: impl FnMut(f32) -> Option<Vec<u8>>,
) -> Option<Vec<u8>> {
    qualities
        .iter()
        .find_map(|&quality| encode(quality).filter(|jpg| jpg.len() <= max))
}

/// Whether `jpg` looks like a JPEG (the room checks the same: `FF D8 FF`).
pub fn is_jpeg(jpg: &[u8]) -> bool {
    jpg.len() > 3 && jpg[..3] == [0xFF, 0xD8, 0xFF]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_picture_keeps_the_aspect_within_its_box() {
        assert_eq!(size(1920, 1080), (320, 180));
        assert_eq!(size(1280, 720), (320, 180));
        // A 4:3 window, a portrait screen, an ultrawide one.
        assert_eq!(size(1024, 768), (320, 240));
        assert_eq!(size(1080, 1920), (180, 320));
        assert_eq!(size(3440, 1440), (320, 134));
        // A small window is not enlarged; odd sides become even.
        assert_eq!(size(301, 201), (300, 200));
        // Nothing degenerate reaches the GPU.
        assert_eq!(size(0, 0), (2, 2));
        assert_eq!(size(5000, 3), (320, 2));
    }

    #[test]
    fn quality_drops_until_the_picture_fits() {
        let mut asked = Vec::new();
        let jpg = fit(|quality| {
            asked.push(quality);
            // Too big at the first two qualities.
            let len = if quality > 0.5 { MAX_BYTES + 1 } else { 1_000 };
            Some(vec![0; len])
        });
        assert_eq!(jpg.map(|jpg| jpg.len()), Some(1_000));
        assert_eq!(asked, vec![0.70, 0.55, 0.40]);
        // Nothing fits, or the encoder fails: no picture.
        assert_eq!(fit(|_| Some(vec![0; MAX_BYTES + 1])), None);
        assert_eq!(fit(|_| None), None);
        assert!(is_jpeg(&[0xFF, 0xD8, 0xFF, 0xE0]));
        assert!(!is_jpeg(&[0x89, b'P', b'N', b'G']));
    }

    #[test]
    fn the_preview_is_larger_and_twice_a_second() {
        assert_eq!(size_within(1920, 1080, PREVIEW_SIDE), (640, 360));
        assert_eq!(size_within(1080, 1920, PREVIEW_SIDE), (360, 640));
        assert_eq!(size_within(500, 300, PREVIEW_SIDE), (500, 300));
        assert_eq!(PREVIEW_EVERY, Duration::from_millis(500));
        let mut asked = Vec::new();
        let jpg = fit_preview(|quality| {
            asked.push(quality);
            Some(vec![
                0;
                if quality > 0.5 {
                    PREVIEW_MAX_BYTES + 1
                } else {
                    9_000
                }
            ])
        });
        assert_eq!(jpg.map(|jpg| jpg.len()), Some(9_000));
        assert_eq!(asked, vec![0.60, 0.45]);
    }
}

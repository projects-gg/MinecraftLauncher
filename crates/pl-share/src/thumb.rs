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

/// The picture's size for content of `w` × `h`: the aspect kept, the long side [`SIDE`] (a smaller
/// source is not enlarged), both sides even and at least 2.
pub fn size(w: u32, h: u32) -> (u32, u32) {
    let (w, h) = (w.max(2), h.max(2));
    let scale = (f64::from(SIDE) / f64::from(w.max(h))).min(1.0);
    let side = |v: u32| ((f64::from(v) * scale).round() as u32).max(2) & !1;
    (side(w), side(h))
}

/// The first of [`QUALITIES`] whose picture fits in [`MAX_BYTES`]; none when no quality fits (or
/// the encoder failed every time).
pub fn fit(mut encode: impl FnMut(f32) -> Option<Vec<u8>>) -> Option<Vec<u8>> {
    QUALITIES
        .iter()
        .find_map(|&quality| encode(quality).filter(|jpg| jpg.len() <= MAX_BYTES))
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
}

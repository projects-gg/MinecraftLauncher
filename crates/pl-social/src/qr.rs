//! The friend code QR as one SVG path, so the launcher draws it in its own colours.

use std::fmt::Write;

use qrcode::{Color, EcLevel, QrCode};
use serde::Serialize;

/// Dark modules of a QR code. Draw with `viewBox="0 0 size size"` (no quiet zone included:
/// leave at least 2 modules of light margin around it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct QrPath {
    /// Modules per side.
    pub size: u32,
    /// `d` of a single path covering every dark module (one rectangle per horizontal run).
    pub path: String,
}

/// Encodes `data` (error correction M, enough for a phone camera on a screen).
pub fn qr_path(data: &str) -> Option<QrPath> {
    let code = QrCode::with_error_correction_level(data.as_bytes(), EcLevel::M).ok()?;
    let width = code.width();
    let colors = code.to_colors();
    let mut path = String::new();
    for y in 0..width {
        let row = &colors[y * width..(y + 1) * width];
        let mut x = 0;
        while x < width {
            if row[x] != Color::Dark {
                x += 1;
                continue;
            }
            let start = x;
            while x < width && row[x] == Color::Dark {
                x += 1;
            }
            let run = x - start;
            let _ = write!(path, "M{start} {y}h{run}v1h-{run}z");
        }
    }
    Some(QrPath {
        size: u32::try_from(width).ok()?,
        path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_a_share_link() {
        let qr = qr_path("https://social.projects.gg/f/PRJ-ABCD-EFGH").unwrap();
        // A small version (21 + 4n modules): easy to scan from a screen.
        assert!(
            (21..=41).contains(&qr.size) && (qr.size - 21).is_multiple_of(4),
            "{}",
            qr.size
        );
        // The top-left finder pattern starts with a 7-module run.
        assert!(qr.path.starts_with("M0 0h7v1h-7z"), "{}", &qr.path[..40]);
        assert_eq!(
            qr,
            qr_path("https://social.projects.gg/f/PRJ-ABCD-EFGH").unwrap()
        );
        assert_ne!(
            qr,
            qr_path("https://social.projects.gg/f/PRJ-ABCD-EFGJ").unwrap()
        );
    }
}

//! A PNG wrapped as a Windows icon (`.ico` with one PNG image, read by Windows Vista and later).

use crate::error::{Error, Result};

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
/// Larger icons are not worth carrying around (a 256×256 PNG is far smaller).
const MAX_PNG_BYTES: usize = 1024 * 1024;

/// Width and height from a PNG's `IHDR` chunk.
fn png_size(png: &[u8]) -> Option<(u32, u32)> {
    if !png.starts_with(PNG_SIGNATURE) || png.len() < 24 || &png[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes(png[16..20].try_into().ok()?);
    let height = u32::from_be_bytes(png[20..24].try_into().ok()?);
    Some((width, height))
}

/// Wrap `png` (at most 256×256) as a single-image icon.
pub fn png_to_ico(png: &[u8]) -> Result<Vec<u8>> {
    if png.len() > MAX_PNG_BYTES {
        return Err(Error::Invalid("icon image is too large".into()));
    }
    let (width, height) =
        png_size(png).ok_or_else(|| Error::Invalid("icon image is not a PNG".into()))?;
    if !(1..=256).contains(&width) || !(1..=256).contains(&height) {
        return Err(Error::Invalid(format!(
            "icon image is {width}×{height}; at most 256×256 fits an icon"
        )));
    }
    // 0 stands for 256 in the one-byte size fields.
    let side = |v: u32| u8::try_from(v).unwrap_or(0);
    let len = u32::try_from(png.len()).map_err(|_| Error::Invalid("icon too large".into()))?;
    let mut ico = Vec::with_capacity(22 + png.len());
    ico.extend_from_slice(&0u16.to_le_bytes()); // reserved
    ico.extend_from_slice(&1u16.to_le_bytes()); // type: icon
    ico.extend_from_slice(&1u16.to_le_bytes()); // one image
    ico.push(side(width));
    ico.push(side(height));
    ico.push(0); // no palette
    ico.push(0); // reserved
    ico.extend_from_slice(&1u16.to_le_bytes()); // colour planes
    ico.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
    ico.extend_from_slice(&len.to_le_bytes());
    ico.extend_from_slice(&22u32.to_le_bytes()); // image offset: right after this header
    ico.extend_from_slice(png);
    Ok(ico)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = PNG_SIGNATURE.to_vec();
        bytes.extend_from_slice(&13u32.to_be_bytes());
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&[8, 6, 0, 0, 0]);
        bytes.extend_from_slice(&[0; 4]); // CRC (not checked)
        bytes
    }

    #[test]
    fn wraps_a_png() {
        let image = png(256, 256);
        let ico = png_to_ico(&image).unwrap();
        assert_eq!(&ico[0..6], &[0, 0, 1, 0, 1, 0]);
        // 256 is written as 0.
        assert_eq!(&ico[6..10], &[0, 0, 0, 0]);
        assert_eq!(u16::from_le_bytes([ico[12], ico[13]]), 32);
        assert_eq!(
            u32::from_le_bytes(ico[14..18].try_into().unwrap()) as usize,
            image.len()
        );
        assert_eq!(u32::from_le_bytes(ico[18..22].try_into().unwrap()), 22);
        assert_eq!(&ico[22..], image.as_slice());

        let small = png_to_ico(&png(48, 32)).unwrap();
        assert_eq!(&small[6..8], &[48, 32]);
    }

    #[test]
    fn refuses_what_is_not_an_icon() {
        assert!(png_to_ico(b"GIF89a").is_err());
        assert!(png_to_ico(&png(512, 512)).is_err());
        assert!(png_to_ico(&png(0, 16)).is_err());
        assert!(png_to_ico(PNG_SIGNATURE).is_err());
    }
}

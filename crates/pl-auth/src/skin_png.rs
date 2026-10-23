//! Projects skins of username accounts (SKIN1, skin1.md §4.1): a skin PNG the player picked is
//! decoded once, here in memory-safe Rust, checked the way the game draws it and encoded again as
//! the clean picture the friends service, ProBot and Lobi accept: 64×64, 8-bit RGBA, not
//! interlaced, nothing but IHDR, IDAT and IEND. None of them decodes a picture; they only check
//! this structure.
//!
//! The picture that goes to the service keeps the file's own pixels (an old 64×32 skin is turned
//! into 64×64 the way the game does it). The preview's picture draws the base layer opaque, as
//! the game does ("ne görürsen o").

use std::fmt;
use std::io::{Cursor, ErrorKind};
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::avatar::rgba_pixels;
use crate::error::SkinFileProblem;
use crate::skins::{MAX_SKIN_BYTES, read_capped};

/// The largest clean skin the friends service takes (`PUT /v1/mc/skin`). A 64×64 RGBA picture
/// is 16,448 bytes before compression, so a clean skin always fits.
pub const CLEAN_SKIN_MAX_BYTES: usize = 24_576;

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
/// Skins are 64 pixels wide; 64 high, or 32 for the old format.
const WIDTH: u32 = 64;
/// More see-through base layer than this (in %) may show black in game.
const SEE_THROUGH_WARN_PERCENT: usize = 5;

/// Why a file cannot be a Projects skin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SkinPngProblem {
    #[error("not a PNG")]
    NotPng,
    /// "Skin 64×64 ya da 64×32 olmalı." (HD skins included: the game does not draw them.)
    #[error("a skin is 64×64 or 64×32, not {width}×{height}")]
    Dimensions { width: u32, height: u32 },
    /// Over [`MAX_SKIN_BYTES`].
    #[error("the file is larger than 1 MiB")]
    TooBig,
    /// The base layer is see-through everywhere: "Bu skin boş görünüyor."
    #[error("the skin's base layer is empty")]
    Empty,
    /// The file could not be read (missing, a folder, no permission, broken image data).
    #[error("the file could not be read ({0:?})")]
    Unreadable(ErrorKind),
}

/// A skin checked and ready to be saved on Projects.
#[derive(Clone, PartialEq, Eq)]
pub struct CleanSkin {
    /// 64×64 RGBA8 PNG, not interlaced, IHDR + IDAT + IEND only, at most
    /// [`CLEAN_SKIN_MAX_BYTES`]: what the service gets.
    pub png: Vec<u8>,
    /// Lower-case hex SHA-256 of `png`.
    pub sha256: String,
    /// The same skin with its base layer opaque, as the game draws it: for the preview.
    pub preview: Vec<u8>,
    /// The file was an old 64×32 skin (turned into 64×64; no slim arms).
    pub legacy: bool,
    /// More than 5% of the base layer is see-through: those parts may look black in game.
    pub black_parts: bool,
}

impl fmt::Debug for CleanSkin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CleanSkin")
            .field("bytes", &self.png.len())
            .field("sha256", &self.sha256)
            .field("legacy", &self.legacy)
            .field("black_parts", &self.black_parts)
            .finish_non_exhaustive()
    }
}

/// Reads the skin file the player picked (at most [`MAX_SKIN_BYTES`]; blocking) and cleans it.
pub fn read_clean_skin(path: &Path) -> Result<CleanSkin, SkinPngProblem> {
    let bytes = read_capped(path).map_err(|problem| match problem {
        SkinFileProblem::TooLarge => SkinPngProblem::TooBig,
        SkinFileProblem::Unreadable { kind } => SkinPngProblem::Unreadable(kind),
        _ => SkinPngProblem::Unreadable(ErrorKind::InvalidData),
    })?;
    clean_skin(&bytes)
}

/// Decodes a skin PNG of any bit depth and colour type (interlaced too), turns an old 64×32 skin
/// into 64×64 as the game does, checks its base layer and encodes it again.
pub fn clean_skin(bytes: &[u8]) -> Result<CleanSkin, SkinPngProblem> {
    let (pixels, legacy) = decode(bytes)?;
    let (see_through, total) = base_layer_see_through(&pixels);
    if see_through == total {
        return Err(SkinPngProblem::Empty);
    }
    let black_parts = see_through * 100 > total * SEE_THROUGH_WARN_PERCENT;
    let png = encode(&pixels, png::Compression::High)?;
    if png.len() > CLEAN_SKIN_MAX_BYTES {
        // Cannot happen (16,448 bytes uncompressed); refuse rather than send what the service
        // turns away.
        return Err(SkinPngProblem::TooBig);
    }
    let sha256 = Sha256::digest(&png)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let preview = encode(&opaque_base(pixels), png::Compression::Fast)?;
    Ok(CleanSkin {
        png,
        sha256,
        preview,
        legacy,
        black_parts,
    })
}

/// The picture the preview shows for a skin texture (a premium player's from Mojang, or a copy
/// kept on this computer): 64×64 with its base layer opaque, as the game draws it.
pub fn skin_preview(texture: &[u8]) -> Result<Vec<u8>, SkinPngProblem> {
    let (pixels, _) = decode(texture)?;
    encode(&opaque_base(pixels), png::Compression::Fast)
}

/// 64×64 RGBA pixels, and whether the file was an old 64×32 skin.
fn decode(bytes: &[u8]) -> Result<(Vec<u8>, bool), SkinPngProblem> {
    if bytes.len() > MAX_SKIN_BYTES {
        return Err(SkinPngProblem::TooBig);
    }
    if !bytes.starts_with(PNG_SIGNATURE) {
        return Err(SkinPngProblem::NotPng);
    }
    let broken = |_| SkinPngProblem::Unreadable(ErrorKind::InvalidData);
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(|_| SkinPngProblem::NotPng)?;
    let (width, height) = {
        let info = reader.info();
        (info.width, info.height)
    };
    // Checked before anything is allocated for the picture.
    if width != WIDTH || (height != WIDTH && height != WIDTH / 2) {
        return Err(SkinPngProblem::Dimensions { width, height });
    }
    let size = reader
        .output_buffer_size()
        .ok_or(SkinPngProblem::Unreadable(ErrorKind::InvalidData))?;
    let mut buffer = vec![0; size];
    let frame = reader.next_frame(&mut buffer).map_err(broken)?;
    let data = buffer
        .get(..frame.buffer_size())
        .ok_or(SkinPngProblem::Unreadable(ErrorKind::InvalidData))?;
    let pixels = rgba_pixels(data, frame.color_type)
        .filter(|pixels| pixels.len() == (width * height * 4) as usize)
        .ok_or(SkinPngProblem::Unreadable(ErrorKind::InvalidData))?;
    if height == WIDTH {
        Ok((pixels, false))
    } else {
        Ok((from_legacy(&pixels), true))
    }
}

fn at(x: u32, y: u32) -> usize {
    ((y * WIDTH + x) * 4) as usize
}

/// Minecraft's `processLegacySkin`: the old 64×32 skin becomes the top half of a 64×64 one, its
/// right leg and arm mirrored into the left ones' places. The game also hides a hat area without
/// a single see-through pixel (old skins painted it solid; "Notch transparency"); it does that
/// only for 64×32 textures, so the converted skin carries it already.
fn from_legacy(top: &[u8]) -> Vec<u8> {
    let mut image = vec![0u8; (WIDTH * WIDTH * 4) as usize];
    image[..top.len()].copy_from_slice(top);
    // (x, y, dx, dy, width, height), all mirrored horizontally, as the game copies them.
    const COPIES: [(u32, u32, i32, i32, u32, u32); 12] = [
        (4, 16, 16, 32, 4, 4),
        (8, 16, 16, 32, 4, 4),
        (0, 20, 24, 32, 4, 12),
        (4, 20, 16, 32, 4, 12),
        (8, 20, 8, 32, 4, 12),
        (12, 20, 16, 32, 4, 12),
        (44, 16, -8, 32, 4, 4),
        (48, 16, -8, 32, 4, 4),
        (40, 20, 0, 32, 4, 12),
        (44, 20, -8, 32, 4, 12),
        (48, 20, -16, 32, 4, 12),
        (52, 20, -8, 32, 4, 12),
    ];
    for (x, y, dx, dy, width, height) in COPIES {
        for row in 0..height {
            for col in 0..width {
                let from = at(x + col, y + row);
                let to_x = (x as i32 + dx) as u32 + (width - 1 - col);
                let to_y = (y as i32 + dy) as u32 + row;
                let to = at(to_x, to_y);
                let pixel: [u8; 4] = image[from..from + 4].try_into().expect("four bytes");
                image[to..to + 4].copy_from_slice(&pixel);
            }
        }
    }
    // The game decides over x 32..64, y 0..32 and clears all of it; below the hat it makes the
    // pixels opaque again right after, so only the hat area really changes.
    let solid = (0..32)
        .flat_map(|y| (32..64).map(move |x| (x, y)))
        .all(|(x, y)| image[at(x, y) + 3] >= 128);
    if solid {
        for y in 0..16 {
            for x in 32..64 {
                image[at(x, y) + 3] = 0;
            }
        }
    }
    image
}

/// The areas the game draws opaque (`setNoAlpha`): the head's and the body's base layer.
const OPAQUE_AREAS: [(u32, u32, u32, u32); 3] = [(0, 0, 32, 16), (0, 16, 64, 32), (16, 48, 48, 64)];

fn opaque_base(mut pixels: Vec<u8>) -> Vec<u8> {
    for (x0, y0, x1, y1) in OPAQUE_AREAS {
        for y in y0..y1 {
            for x in x0..x1 {
                pixels[at(x, y) + 3] = 255;
            }
        }
    }
    pixels
}

/// The faces of the base layer the game draws (x, y, width, height), with 3-pixel arms: the
/// pixels both arm models draw (a slim skin leaves the classic arms' fourth column empty).
const BASE_FACES: [(u32, u32, u32, u32); 18] = [
    // head: top, bottom, then the four sides
    (8, 0, 8, 8),
    (16, 0, 8, 8),
    (0, 8, 32, 8),
    // right leg
    (4, 16, 4, 4),
    (8, 16, 4, 4),
    (0, 20, 16, 12),
    // body
    (20, 16, 8, 4),
    (28, 16, 8, 4),
    (16, 20, 24, 12),
    // right arm (slim)
    (44, 16, 3, 4),
    (47, 16, 3, 4),
    (40, 20, 14, 12),
    // left leg
    (20, 48, 4, 4),
    (24, 48, 4, 4),
    (16, 52, 16, 12),
    // left arm (slim)
    (36, 48, 3, 4),
    (39, 48, 3, 4),
    (32, 52, 14, 12),
];

/// How many base layer pixels are see-through (alpha below half), of how many.
fn base_layer_see_through(pixels: &[u8]) -> (usize, usize) {
    let mut see_through = 0;
    let mut total = 0;
    for (x0, y0, width, height) in BASE_FACES {
        for y in y0..y0 + height {
            for x in x0..x0 + width {
                total += 1;
                if pixels[at(x, y) + 3] < 128 {
                    see_through += 1;
                }
            }
        }
    }
    (see_through, total)
}

fn encode(pixels: &[u8], compression: png::Compression) -> Result<Vec<u8>, SkinPngProblem> {
    let failed = |_| SkinPngProblem::Unreadable(ErrorKind::InvalidData);
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, WIDTH, WIDTH);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(compression);
    let mut writer = encoder.write_header().map_err(failed)?;
    writer.write_image_data(pixels).map_err(failed)?;
    writer.finish().map_err(failed)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real PNG of `width`×`height`: `paint(x, y)` gives each pixel.
    fn picture(width: u32, height: u32, paint: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
        crate::avatar::tests::texture(width, height, paint)
    }

    /// A plain skin: every base face opaque grey, the outer layer see-through.
    fn plain(x: u32, y: u32) -> [u8; 4] {
        if BASE_FACES
            .iter()
            .any(|&(x0, y0, w, h)| (x0..x0 + w).contains(&x) && (y0..y0 + h).contains(&y))
        {
            [120, 130, 140, 255]
        } else {
            [0, 0, 0, 0]
        }
    }

    /// The chunk types of a PNG, in order, with every CRC checked.
    fn chunks(png: &[u8]) -> Vec<String> {
        assert!(png.starts_with(PNG_SIGNATURE));
        let mut out = Vec::new();
        let mut rest = &png[8..];
        while !rest.is_empty() {
            let length = u32::from_be_bytes(rest[..4].try_into().unwrap()) as usize;
            let kind = String::from_utf8(rest[4..8].to_vec()).unwrap();
            let body = &rest[4..8 + length];
            let crc = u32::from_be_bytes(rest[8 + length..12 + length].try_into().unwrap());
            assert_eq!(crc, crc32(body), "{kind} has a wrong CRC");
            out.push(kind);
            rest = &rest[12 + length..];
        }
        out
    }

    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = 0xffff_ffffu32;
        for &byte in bytes {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xedb8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }

    fn decoded(png: &[u8]) -> (png::OutputInfo, Vec<u8>) {
        let decoder = png::Decoder::new(Cursor::new(png));
        let mut reader = decoder.read_info().unwrap();
        let mut buffer = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut buffer).unwrap();
        (info, buffer)
    }

    #[test]
    fn a_clean_skin_is_64_by_64_rgba_with_only_the_three_chunks() {
        let skin = clean_skin(&picture(64, 64, plain)).unwrap();
        assert_eq!(chunks(&skin.png), ["IHDR", "IDAT", "IEND"]);
        let (info, pixels) = decoded(&skin.png);
        assert_eq!((info.width, info.height), (64, 64));
        assert_eq!(info.color_type, png::ColorType::Rgba);
        assert_eq!(info.bit_depth, png::BitDepth::Eight);
        // Not interlaced (IHDR's last byte).
        assert_eq!(skin.png[8 + 8 + 12], 0);
        assert!(skin.png.len() <= CLEAN_SKIN_MAX_BYTES);
        assert_eq!(skin.sha256.len(), 64);
        let expected: String = Sha256::digest(&skin.png)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(skin.sha256, expected);
        assert!(!skin.legacy && !skin.black_parts);
        // The file's own pixels: the outer layer stays see-through.
        assert_eq!(&pixels[at(40, 8)..at(40, 8) + 4], &[0, 0, 0, 0]);
        assert_eq!(&pixels[at(8, 8)..at(8, 8) + 4], &[120, 130, 140, 255]);
    }

    #[test]
    fn metadata_and_odd_formats_are_dropped_and_widened() {
        // A grayscale PNG with a text chunk and 16-bit depth: decoded and written again as plain
        // 8-bit RGBA without the text.
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, 64, 64);
            encoder.set_color(png::ColorType::Grayscale);
            encoder.set_depth(png::BitDepth::Sixteen);
            encoder
                .add_text_chunk("Comment".into(), "made with love".into())
                .unwrap();
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&vec![0x80; 64 * 64 * 2]).unwrap();
            writer.finish().unwrap();
        }
        assert!(chunks(&out).contains(&"tEXt".to_owned()));
        let skin = clean_skin(&out).unwrap();
        assert_eq!(chunks(&skin.png), ["IHDR", "IDAT", "IEND"]);
        let (info, pixels) = decoded(&skin.png);
        assert_eq!(info.color_type, png::ColorType::Rgba);
        assert_eq!(&pixels[..4], &[0x80, 0x80, 0x80, 255]);
    }

    #[test]
    fn a_palette_and_an_interlaced_file_are_fine() {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, 64, 64);
            encoder.set_color(png::ColorType::Indexed);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.set_palette(vec![10, 20, 30, 200, 100, 50]);
            encoder.set_trns(vec![255, 0]);
            let mut writer = encoder.write_header().unwrap();
            let pixels: Vec<u8> = (0..64 * 64)
                .map(|i| {
                    let (x, y) = (i % 64, i / 64);
                    u8::from(plain(x, y)[3] == 0)
                })
                .collect();
            writer.write_image_data(&pixels).unwrap();
            writer.finish().unwrap();
        }
        let skin = clean_skin(&out).unwrap();
        let (_, pixels) = decoded(&skin.png);
        assert_eq!(&pixels[at(8, 8)..at(8, 8) + 4], &[10, 20, 30, 255]);
        assert_eq!(pixels[at(40, 8) + 3], 0);

        // Adam7 (written by hand: the png crate does not write interlaced files).
        let skin = clean_skin(&interlaced(plain)).unwrap();
        assert_eq!(skin.png[8 + 8 + 12], 0, "written without interlacing");
        let (_, pixels) = decoded(&skin.png);
        assert_eq!(&pixels[at(8, 8)..at(8, 8) + 4], &[120, 130, 140, 255]);
        assert_eq!(pixels[at(40, 8) + 3], 0);
    }

    /// A 64×64 RGBA PNG interlaced with Adam7, its data in stored (uncompressed) deflate blocks.
    fn interlaced(paint: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
        const PASSES: [(u32, u32, u32, u32); 7] = [
            (0, 0, 8, 8),
            (4, 0, 8, 8),
            (0, 4, 4, 8),
            (2, 0, 4, 4),
            (0, 2, 2, 4),
            (1, 0, 2, 2),
            (0, 1, 1, 2),
        ];
        let mut raw = Vec::new();
        for (x0, y0, dx, dy) in PASSES {
            for y in (y0..64).step_by(dy as usize) {
                raw.push(0);
                for x in (x0..64).step_by(dx as usize) {
                    raw.extend_from_slice(&paint(x, y));
                }
            }
        }
        let mut zlib = vec![0x78, 0x01];
        let blocks: Vec<&[u8]> = raw.chunks(65_535).collect();
        for (index, block) in blocks.iter().enumerate() {
            zlib.push(u8::from(index + 1 == blocks.len()));
            let len = block.len() as u16;
            zlib.extend_from_slice(&len.to_le_bytes());
            zlib.extend_from_slice(&(!len).to_le_bytes());
            zlib.extend_from_slice(block);
        }
        let (mut a, mut b) = (1u32, 0u32);
        for &byte in &raw {
            a = (a + u32::from(byte)) % 65_521;
            b = (b + a) % 65_521;
        }
        zlib.extend_from_slice(&((b << 16) | a).to_be_bytes());
        let chunk = |kind: &[u8], body: &[u8]| {
            let mut out = (body.len() as u32).to_be_bytes().to_vec();
            let mut typed = kind.to_vec();
            typed.extend_from_slice(body);
            out.extend_from_slice(&typed);
            out.extend_from_slice(&crc32(&typed).to_be_bytes());
            out
        };
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&64u32.to_be_bytes());
        ihdr.extend_from_slice(&64u32.to_be_bytes());
        ihdr.extend_from_slice(&[8, 6, 0, 0, 1]);
        let mut png = PNG_SIGNATURE.to_vec();
        png.extend(chunk(b"IHDR", &ihdr));
        png.extend(chunk(b"IDAT", &zlib));
        png.extend(chunk(b"IEND", &[]));
        png
    }

    #[test]
    fn wrong_files_are_refused_with_their_reason() {
        assert_eq!(
            clean_skin(b"GIF89a....").unwrap_err(),
            SkinPngProblem::NotPng
        );
        assert_eq!(
            clean_skin(&picture(128, 128, plain)).unwrap_err(),
            SkinPngProblem::Dimensions {
                width: 128,
                height: 128
            }
        );
        assert_eq!(
            clean_skin(&picture(64, 48, plain)).unwrap_err(),
            SkinPngProblem::Dimensions {
                width: 64,
                height: 48
            }
        );
        let mut huge = PNG_SIGNATURE.to_vec();
        huge.resize(MAX_SKIN_BYTES + 1, 0);
        assert_eq!(clean_skin(&huge).unwrap_err(), SkinPngProblem::TooBig);
        assert_eq!(
            clean_skin(&picture(64, 64, |_, _| [0, 0, 0, 0])).unwrap_err(),
            SkinPngProblem::Empty
        );
        // A PNG cut short after its header.
        let whole = picture(64, 64, plain);
        assert_eq!(
            clean_skin(&whole[..60]).unwrap_err(),
            SkinPngProblem::Unreadable(ErrorKind::InvalidData)
        );
    }

    #[test]
    fn see_through_base_layer_warns_but_saves() {
        // The face (8×8 of 1,568 base pixels) is fine; the whole front of the body is not.
        let face = clean_skin(&picture(64, 64, |x, y| {
            if (8..16).contains(&x) && (8..16).contains(&y) {
                [0, 0, 0, 0]
            } else {
                plain(x, y)
            }
        }))
        .unwrap();
        assert!(!face.black_parts);
        let body = clean_skin(&picture(64, 64, |x, y| {
            if (16..40).contains(&x) && (20..32).contains(&y) {
                [0, 0, 0, 0]
            } else {
                plain(x, y)
            }
        }))
        .unwrap();
        assert!(body.black_parts);
        // The preview draws it opaque, in its own colour (black here), as the game does.
        let (_, preview) = decoded(&body.preview);
        assert_eq!(&preview[at(20, 24)..at(20, 24) + 4], &[0, 0, 0, 255]);
        // ...while the outer layer stays see-through there too.
        assert_eq!(preview[at(20, 40) + 3], 0);
    }

    #[test]
    fn slim_arms_are_not_black_parts() {
        // A slim skin leaves the classic arm's fourth column empty: no warning.
        let slim = clean_skin(&picture(64, 64, |x, y| {
            let fourth = ((50..52).contains(&x) && (16..20).contains(&y))
                || ((54..56).contains(&x) && (20..32).contains(&y))
                || ((42..44).contains(&x) && (48..52).contains(&y))
                || ((46..48).contains(&x) && (52..64).contains(&y));
            if fourth { [0, 0, 0, 0] } else { plain(x, y) }
        }))
        .unwrap();
        assert!(!slim.black_parts);
    }

    #[test]
    fn an_old_64_by_32_skin_becomes_64_by_64_like_the_game_does_it() {
        // Right leg front red, right arm front green, the right arm's outer side blue; the hat
        // area painted solid white.
        let legacy = picture(64, 32, |x, y| {
            if (4..8).contains(&x) && (20..32).contains(&y) {
                [200, 0, 0, 255]
            } else if (44..48).contains(&x) && (20..32).contains(&y) {
                [0, 200, 0, 255]
            } else if (40..44).contains(&x) && (20..32).contains(&y) {
                [0, 0, 200, 255]
            } else if (32..64).contains(&x) && y < 16 {
                [255, 255, 255, 255]
            } else {
                [90, 90, 90, 255]
            }
        });
        let skin = clean_skin(&legacy).unwrap();
        assert!(skin.legacy);
        let (info, pixels) = decoded(&skin.png);
        assert_eq!((info.width, info.height), (64, 64));
        let pixel =
            |x: u32, y: u32| -> [u8; 4] { pixels[at(x, y)..at(x, y) + 4].try_into().unwrap() };
        // The left leg's front is the right leg's front, mirrored.
        assert_eq!(pixel(20, 52), [200, 0, 0, 255]);
        assert_eq!(pixel(23, 63), [200, 0, 0, 255]);
        // The left arm's front is the right arm's front; its outer side (now on the left of the
        // texture's arm area) comes from the right arm's outer side.
        assert_eq!(pixel(36, 52), [0, 200, 0, 255]);
        assert_eq!(pixel(40, 52), [0, 0, 200, 255]);
        // The solid hat area was hidden, as the game hides it for 64×32 skins.
        assert_eq!(pixel(40, 8)[3], 0);
        // The face is kept.
        assert_eq!(pixel(8, 8), [90, 90, 90, 255]);

        // A legacy skin with a see-through pixel in its hat area keeps its hat.
        let hatted = picture(64, 32, |x, y| {
            if x == 63 && y == 0 {
                [0, 0, 0, 0]
            } else {
                [90, 90, 90, 255]
            }
        });
        let skin = clean_skin(&hatted).unwrap();
        let (_, pixels) = decoded(&skin.png);
        assert_eq!(pixels[at(40, 8) + 3], 255);
    }

    #[test]
    fn previews_of_textures_are_64_by_64_with_an_opaque_base() {
        let preview = skin_preview(&picture(64, 32, |_, _| [10, 10, 10, 0])).unwrap();
        let (info, pixels) = decoded(&preview);
        assert_eq!((info.width, info.height), (64, 64));
        assert_eq!(pixels[at(8, 8) + 3], 255);
        assert_eq!(pixels[at(20, 52) + 3], 255);
        assert!(matches!(
            skin_preview(&picture(32, 32, plain)),
            Err(SkinPngProblem::Dimensions { .. })
        ));
    }

    #[test]
    fn reads_files_within_the_size_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("skin.png");
        std::fs::write(&path, picture(64, 64, plain)).unwrap();
        assert!(read_clean_skin(&path).is_ok());
        let big = dir.path().join("big.png");
        std::fs::write(&big, vec![0u8; MAX_SKIN_BYTES + 10]).unwrap();
        assert_eq!(read_clean_skin(&big).unwrap_err(), SkinPngProblem::TooBig);
        assert!(matches!(
            read_clean_skin(&dir.path().join("missing.png")),
            Err(SkinPngProblem::Unreadable(_))
        ));
    }
}

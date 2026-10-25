//! A small RGBA image: PNG in and out, frames, alpha compositing, tint and shade.

use std::io::Cursor;

use crate::Error;

/// Straight-alpha RGBA, row by row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Image {
    /// A transparent image.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; (width * height * 4) as usize],
        }
    }

    pub fn get(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.width + x) * 4) as usize;
        [
            self.pixels[i],
            self.pixels[i + 1],
            self.pixels[i + 2],
            self.pixels[i + 3],
        ]
    }

    pub fn put(&mut self, x: u32, y: u32, px: [u8; 4]) {
        let i = ((y * self.width + x) * 4) as usize;
        self.pixels[i..i + 4].copy_from_slice(&px);
    }

    /// Whether every pixel is fully transparent.
    pub fn is_blank(&self) -> bool {
        self.pixels.as_chunks::<4>().0.iter().all(|px| px[3] == 0)
    }

    /// Whether some pixel is neither transparent nor opaque (stained glass, ice...).
    pub fn has_translucency(&self) -> bool {
        self.pixels
            .as_chunks::<4>()
            .0
            .iter()
            .any(|px| px[3] > 0 && px[3] < 255)
    }

    /// The colour at a texel position with bilinear filtering, wrapping at the edges (the game's
    /// blurred, repeating textures); positions in texels, texel centres at `.5`.
    pub fn sample_wrapped(&self, x: f32, y: f32) -> [f32; 3] {
        let (w, h) = (self.width as i64, self.height as i64);
        let (fx, fy) = (x - 0.5, y - 0.5);
        let (x0, y0) = (fx.floor(), fy.floor());
        let (tx, ty) = (fx - x0, fy - y0);
        let mut out = [0.0f32; 3];
        for (dy, wy) in [(0, 1.0 - ty), (1, ty)] {
            for (dx, wx) in [(0, 1.0 - tx), (1, tx)] {
                let sx = (x0 as i64 + dx).rem_euclid(w) as u32;
                let sy = (y0 as i64 + dy).rem_euclid(h) as u32;
                let px = self.get(sx, sy);
                for (c, value) in out.iter_mut().enumerate() {
                    *value += px[c] as f32 / 255.0 * wx * wy;
                }
            }
        }
        out
    }

    /// Decodes a PNG to RGBA (palette, grey and 16-bit images included).
    pub fn decode_png(bytes: &[u8]) -> Result<Self, Error> {
        let decode = |error: png::DecodingError| Error::Image(error.to_string());
        let mut decoder = png::Decoder::new(Cursor::new(bytes));
        decoder.set_transformations(png::Transformations::normalize_to_color8());
        let mut reader = decoder.read_info().map_err(decode)?;
        let size = reader
            .output_buffer_size()
            .ok_or_else(|| Error::Image("image too large".into()))?;
        let mut buffer = vec![0; size];
        let info = reader.next_frame(&mut buffer).map_err(decode)?;
        let data = &buffer[..info.buffer_size()];
        let pixels: Vec<u8> = match info.color_type {
            png::ColorType::Rgba => data.to_vec(),
            png::ColorType::Rgb => data
                .as_chunks::<3>()
                .0
                .iter()
                .flat_map(|c| [c[0], c[1], c[2], 255])
                .collect(),
            png::ColorType::GrayscaleAlpha => data
                .as_chunks::<2>()
                .0
                .iter()
                .flat_map(|c| [c[0], c[0], c[0], c[1]])
                .collect(),
            png::ColorType::Grayscale => data.iter().flat_map(|&g| [g, g, g, 255]).collect(),
            png::ColorType::Indexed => return Err(Error::Image("palette was not expanded".into())),
        };
        Ok(Self {
            width: info.width,
            height: info.height,
            pixels,
        })
    }

    /// Encodes as an RGBA PNG.
    pub fn encode_png(&self) -> Result<Vec<u8>, Error> {
        let encode = |error: png::EncodingError| Error::Image(error.to_string());
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::High);
        let mut writer = encoder.write_header().map_err(encode)?;
        writer.write_image_data(&self.pixels).map_err(encode)?;
        writer.finish().map_err(encode)?;
        Ok(out)
    }

    /// An animated texture's first frame (frames are stacked squares); other images unchanged.
    pub fn first_frame(self) -> Self {
        if self.height > self.width && self.height.is_multiple_of(self.width) {
            self.crop(0, 0, self.width, self.width)
        } else {
            self
        }
    }

    /// A copy of the rectangle (clamped to the image).
    pub fn crop(&self, x: u32, y: u32, width: u32, height: u32) -> Self {
        let mut out = Self::new(width, height);
        for dy in 0..height {
            for dx in 0..width {
                if x + dx < self.width && y + dy < self.height {
                    out.put(dx, dy, self.get(x + dx, y + dy));
                }
            }
        }
        out
    }

    /// Draws `src` over this image with its top-left corner at (x, y).
    pub fn draw(&mut self, src: &Image, x: u32, y: u32) {
        for sy in 0..src.height {
            for sx in 0..src.width {
                let (dx, dy) = (x + sx, y + sy);
                if dx < self.width && dy < self.height {
                    let px = over(self.get(dx, dy), src.get(sx, sy));
                    self.put(dx, dy, px);
                }
            }
        }
    }

    /// Copies `src` into this image at (x, y), replacing what is there.
    pub fn blit(&mut self, src: &Image, x: u32, y: u32) {
        for sy in 0..src.height {
            for sx in 0..src.width {
                let (dx, dy) = (x + sx, y + sy);
                if dx < self.width && dy < self.height {
                    self.put(dx, dy, src.get(sx, sy));
                }
            }
        }
    }

    /// Scales up by a whole factor, nearest neighbour.
    pub fn upscale(&self, factor: u32) -> Self {
        let mut out = Self::new(self.width * factor, self.height * factor);
        for y in 0..out.height {
            for x in 0..out.width {
                out.put(x, y, self.get(x / factor, y / factor));
            }
        }
        out
    }

    /// Multiplies every pixel's colour by `rgb` (0xRRGGBB), the way the game tints a layer.
    pub fn tinted(&self, rgb: u32) -> Self {
        let mut out = self.clone();
        for px in out.pixels.as_chunks_mut::<4>().0 {
            *px = tint(*px, rgb);
        }
        out
    }

    /// How much of the silhouette's edge stands clear of a ground colour: of the opaque pixels
    /// within `radius` pixels of a clear one (or of the border), the share at least `distance`
    /// away from the ground in OKLab. Small when the outline melts into that ground (a netherite
    /// ingot or black wool on a near-black card). None for a picture without opaque pixels.
    pub fn edge_clearance(&self, ground: [u8; 3], radius: u32, distance: f32) -> Option<f32> {
        let (w, h, r) = (self.width as i64, self.height as i64, radius as i64);
        let opaque = |x: i64, y: i64| {
            (0..w).contains(&x) && (0..h).contains(&y) && self.get(x as u32, y as u32)[3] >= 128
        };
        let base = oklab(ground);
        let (mut band, mut clear) = (0u32, 0u32);
        for y in 0..h {
            for x in 0..w {
                if !opaque(x, y) || (-r..=r).all(|dy| (-r..=r).all(|dx| opaque(x + dx, y + dy))) {
                    continue;
                }
                band += 1;
                let px = self.get(x as u32, y as u32);
                let c = oklab([px[0], px[1], px[2]]);
                let d = [c[0] - base[0], c[1] - base[1], c[2] - base[2]];
                if (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() >= distance {
                    clear += 1;
                }
            }
        }
        (band > 0).then(|| clear as f32 / band as f32)
    }

    /// Lays the see-through pixels over a ground colour, as the game shows them on its slot
    /// (stained glass, ice): they become opaque; clear and opaque pixels stay as they are.
    pub fn flatten_translucency(&mut self, ground: [u8; 3]) {
        for px in self.pixels.as_chunks_mut::<4>().0 {
            if px[3] > 0 && px[3] < 255 {
                *px = over([ground[0], ground[1], ground[2], 255], *px);
            }
        }
    }
}

/// An sRGB channel in linear light (0–1).
fn linear(c: u8) -> f32 {
    let c = c as f32 / 255.0;
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// An sRGB colour in OKLab (Björn Ottosson's matrices), where distances match what the eye sees.
fn oklab(rgb: [u8; 3]) -> [f32; 3] {
    let [r, g, b] = rgb.map(linear);
    let l = (0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}

/// `top` over `bottom`, straight alpha.
pub fn over(bottom: [u8; 4], top: [u8; 4]) -> [u8; 4] {
    let ta = top[3] as f32 / 255.0;
    if ta >= 1.0 {
        return top;
    }
    if ta <= 0.0 {
        return bottom;
    }
    let ba = bottom[3] as f32 / 255.0;
    let a = ta + ba * (1.0 - ta);
    if a <= 0.0 {
        return [0, 0, 0, 0];
    }
    let mix = |t: u8, b: u8| {
        ((t as f32 * ta + b as f32 * ba * (1.0 - ta)) / a)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    [
        mix(top[0], bottom[0]),
        mix(top[1], bottom[1]),
        mix(top[2], bottom[2]),
        (a * 255.0).round() as u8,
    ]
}

/// A pixel multiplied by a tint colour (0xRRGGBB).
pub fn tint(px: [u8; 4], rgb: u32) -> [u8; 4] {
    let m = |c: u8, t: u32| ((c as u32 * (t & 255) + 127) / 255) as u8;
    [
        m(px[0], rgb >> 16),
        m(px[1], rgb >> 8),
        m(px[2], rgb),
        px[3],
    ]
}

/// A pixel darkened by a light factor (0-1), alpha kept.
pub fn shade(px: [u8; 4], factor: f32) -> [u8; 4] {
    let s = |c: u8| (c as f32 * factor).round().clamp(0.0, 255.0) as u8;
    [s(px[0]), s(px[1]), s(px[2]), px[3]]
}

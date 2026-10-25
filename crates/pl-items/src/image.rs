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

    /// Whether the pixel is inside the picture and at least half opaque.
    fn solid(&self, x: i64, y: i64) -> bool {
        (0..self.width as i64).contains(&x)
            && (0..self.height as i64).contains(&y)
            && self.get(x as u32, y as u32)[3] >= 128
    }

    /// How light the silhouette's edge looks on a ground colour: the median CIELAB lightness (L*,
    /// 0–100) of the opaque pixels within `radius` pixels of a clear one (or of the border), each
    /// laid over the ground. Low when the outline melts into a near-black card (obsidian, black
    /// wool). None for a picture without opaque pixels.
    pub fn edge_lightness(&self, ground: [u8; 3], radius: u32) -> Option<f32> {
        let r = radius as i64;
        let mut values = Vec::new();
        for y in 0..self.height as i64 {
            for x in 0..self.width as i64 {
                let inner = (-r..=r).all(|dy| (-r..=r).all(|dx| self.solid(x + dx, y + dy)));
                if !self.solid(x, y) || inner {
                    continue;
                }
                let seen = over(
                    [ground[0], ground[1], ground[2], 255],
                    self.get(x as u32, y as u32),
                );
                values.push(lightness([seen[0], seen[1], seen[2]]));
            }
        }
        if values.is_empty() {
            return None;
        }
        values.sort_by(f32::total_cmp);
        Some(values[values.len() / 2])
    }

    /// How much an edge `width` pixels wide drawn around the silhouette would fill in between its
    /// parts (the holes in leaves, a chain's links, the crook of a pickaxe): the clear pixels with an
    /// opaque one within `width` on both sides of a row, a column or a diagonal, per opaque pixel.
    pub fn gap_share(&self, width: u32) -> f32 {
        let reach = |x: i64, y: i64, dx: i64, dy: i64| {
            (1..=width as i64).any(|k| self.solid(x + dx * k, y + dy * k))
        };
        let (mut gaps, mut solid) = (0u32, 0u32);
        for y in 0..self.height as i64 {
            for x in 0..self.width as i64 {
                if self.solid(x, y) {
                    solid += 1;
                } else if [(1, 0), (0, 1), (1, 1), (1, -1)]
                    .iter()
                    .any(|&(dx, dy)| reach(x, y, dx, dy) && reach(x, y, -dx, -dy))
                {
                    gaps += 1;
                }
            }
        }
        if solid == 0 {
            0.0
        } else {
            gaps as f32 / solid as f32
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

/// An sRGB colour's CIELAB lightness L* (0–100).
fn lightness(rgb: [u8; 3]) -> f32 {
    let [r, g, b] = rgb.map(linear);
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    if y > 216.0 / 24389.0 {
        116.0 * y.cbrt() - 16.0
    } else {
        y * 24389.0 / 27.0
    }
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

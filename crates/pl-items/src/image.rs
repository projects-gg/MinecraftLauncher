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

/// How a silhouette's edge looks on a ground colour ([`Image::edge_tone`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeTone {
    /// The median CIELAB lightness L* (0–100).
    pub lightness: f32,
    /// The median OKLab chroma (0 for greys).
    pub chroma: f32,
}

/// How far a whole silhouette stands out from a ground colour ([`Image::contrast`]): luminance
/// contrast ratios (1 for none, 21 for white on black) of its pixels laid over the ground.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contrast {
    /// The median pixel's.
    pub median: f32,
    /// The upper quartile's: how much of it is lit.
    pub upper: f32,
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

    /// Whether the pixel is inside the picture and part of the silhouette: at least a sixteenth
    /// opaque, so see-through glass counts (it shows on the slot, if faintly).
    fn solid(&self, x: i64, y: i64) -> bool {
        (0..self.width as i64).contains(&x)
            && (0..self.height as i64).contains(&y)
            && self.get(x as u32, y as u32)[3] >= 16
    }

    /// How the silhouette's edge looks on a ground colour: the median CIELAB lightness and the
    /// median OKLab chroma of its pixels within `radius` pixels of a clear one (or of the border),
    /// each laid over the ground (see-through ones show the ground through them). Dark and grey when
    /// the outline melts into a near-black card (obsidian, black wool, black glass); a dark red
    /// outline (redstone, netherrack) stays coloured. None for a picture with nothing in it.
    pub fn edge_tone(&self, ground: [u8; 3], radius: u32) -> Option<EdgeTone> {
        let r = radius as i64;
        let (mut lights, mut chromas) = (Vec::new(), Vec::new());
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
                let rgb = [seen[0], seen[1], seen[2]];
                lights.push(lightness(rgb));
                chromas.push(chroma(rgb));
            }
        }
        let median = |mut values: Vec<f32>| {
            values.sort_by(f32::total_cmp);
            values[values.len() / 2]
        };
        (!lights.is_empty()).then(|| EdgeTone {
            lightness: median(lights),
            chroma: median(chromas),
        })
    }

    /// How far the whole silhouette stands out from a ground colour: the luminance contrast of its
    /// pixels, each laid over the ground (see-through ones show the ground through them). An anvil
    /// on our darkest card is about 1.33 at the median and 1.74 at the upper quartile; a hopper,
    /// which reads, 1.64 and 2.02. None for a picture with nothing in it.
    pub fn contrast(&self, ground: [u8; 3]) -> Option<Contrast> {
        let mut lums: Vec<f32> = (0..self.height as i64)
            .flat_map(|y| (0..self.width as i64).map(move |x| (x, y)))
            .filter(|&(x, y)| self.solid(x, y))
            .map(|(x, y)| {
                let seen = over(
                    [ground[0], ground[1], ground[2], 255],
                    self.get(x as u32, y as u32),
                );
                luminance([seen[0], seen[1], seen[2]])
            })
            .collect();
        if lums.is_empty() {
            return None;
        }
        lums.sort_by(f32::total_cmp);
        let n = lums.len();
        let floor = luminance(ground);
        let ratio = |lum: f32| (lum.max(floor) + 0.05) / (lum.min(floor) + 0.05);
        Some(Contrast {
            median: ratio(lums[n / 2]),
            upper: ratio(lums[(n * 3 / 4).min(n - 1)]),
        })
    }

    /// Lays every see-through pixel (neither clear nor opaque) over a ground colour at `strength`
    /// (0–1): over that ground at full strength the pixel looks as before; elsewhere it shows that
    /// much of the ground. Clear and opaque pixels are kept.
    pub fn lay_see_through_over(&mut self, ground: [u8; 3], strength: f32) {
        let alpha = (strength.clamp(0.0, 1.0) * 255.0).round() as u8;
        let under = [ground[0], ground[1], ground[2], alpha];
        for px in self.pixels.as_chunks_mut::<4>().0 {
            if px[3] > 0 && px[3] < 255 {
                *px = over(under, *px);
            }
        }
    }

    /// How much an edge `width` pixels wide drawn around the silhouette would fill in between its
    /// parts (the holes in leaves, a chain's links, the crook of a pickaxe): the clear pixels with a
    /// silhouette one within `width` on both sides of a row, a column or a diagonal, per silhouette
    /// pixel.
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

/// An sRGB colour's relative luminance (0–1, as WCAG's contrast ratio uses it).
fn luminance(rgb: [u8; 3]) -> f32 {
    let [r, g, b] = rgb.map(linear);
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// An sRGB colour's CIELAB lightness L* (0–100).
fn lightness(rgb: [u8; 3]) -> f32 {
    let y = luminance(rgb);
    if y > 216.0 / 24389.0 {
        116.0 * y.cbrt() - 16.0
    } else {
        y * 24389.0 / 27.0
    }
}

/// An sRGB colour's OKLab chroma: 0 for greys, about 0.1 for redstone's dark red, 0.3 at most.
fn chroma(rgb: [u8; 3]) -> f32 {
    let [r, g, b] = rgb.map(linear);
    let l = (0.412_221_5 * r + 0.536_332_5 * g + 0.051_445_99 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_397 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_8 * g + 0.629_978_7 * b).cbrt();
    let a = 1.977_998 * l - 2.428_592 * m + 0.450_593_7 * s;
    let b = 0.025_904_04 * l + 0.782_771_8 * m - 0.808_675_8 * s;
    a.hypot(b)
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

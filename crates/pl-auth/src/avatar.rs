//! Player heads for the account list: the face of a skin texture with its hat layer, the default
//! skin Minecraft gives a profile without one (offline accounts), and a small disk cache. Also
//! the front view of a whole skin ([`front_png`]) for the skin history's thumbnails.
//!
//! Heads are cut from textures the launcher already has: the Mojang texture of a Microsoft
//! profile, or a default skin read from a client jar on this computer. No third-party avatar
//! service is ever asked.

use std::fs;
use std::io::{self, BufReader, Cursor, Read, Write as _};
use std::path::{Path, PathBuf};

use url::Url;
use uuid::Uuid;

use crate::model::{Account, AccountKind, SkinVariant};
use crate::skins::is_mojang_texture_url;

/// Minecraft's default skins, in the client's order (`DefaultPlayerSkin`, 1.19.3 and later):
/// every name exists once with slim arms and once with wide ones.
pub const DEFAULT_SKIN_NAMES: [&str; 9] = [
    "alex", "ari", "efe", "kai", "makena", "noor", "steve", "sunny", "zuri",
];

/// Textures are a few KiB; anything far bigger is not a skin.
const MAX_TEXTURE_BYTES: u64 = 2 * 1024 * 1024;
/// Client jars looked at for default skins (newest first). Any 1.19.3+ client has all of them.
const MAX_JARS: usize = 12;

/// One of Minecraft's default skins in one arm model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DefaultSkin {
    pub name: &'static str,
    pub variant: SkinVariant,
}

impl DefaultSkin {
    /// The default skin the game shows for a profile without a skin of its own:
    /// `DEFAULT_SKINS[Math.floorMod(uuid.hashCode(), 18)]`, the slim models first.
    pub fn for_profile(uuid: Uuid) -> Self {
        let count = DEFAULT_SKIN_NAMES.len();
        // `rem_euclid` of a positive modulus is never negative: the cast cannot wrap.
        let index = java_uuid_hash(uuid).rem_euclid(2 * count as i32) as usize;
        Self {
            name: DEFAULT_SKIN_NAMES[index % count],
            variant: if index < count {
                SkinVariant::Slim
            } else {
                SkinVariant::Classic
            },
        }
    }

    /// Where the texture lives inside a client jar.
    pub fn jar_entry(self) -> String {
        let model = match self.variant {
            SkinVariant::Slim => "slim",
            SkinVariant::Classic => "wide",
        };
        format!(
            "assets/minecraft/textures/entity/player/{model}/{}.png",
            self.name
        )
    }
}

/// Java's `UUID.hashCode()`: the high and low halves of `most ^ least`, xored.
fn java_uuid_hash(uuid: Uuid) -> i32 {
    let (most, least) = uuid.as_u64_pair();
    let hilo = most ^ least;
    ((hilo >> 32) as u32 ^ hilo as u32) as i32
}

/// The id of a texture on Mojang's texture host (`…/texture/<hex>`), lowercased. Texture URLs
/// are content-addressed: one id always means the same picture.
pub fn texture_id(url: &str) -> Option<String> {
    if !is_mojang_texture_url(url) {
        return None;
    }
    let parsed = Url::parse(url).ok()?;
    let mut segments = parsed.path_segments()?;
    if segments.next()? != "texture" {
        return None;
    }
    let id = segments.next()?;
    let valid = !id.is_empty() && id.len() <= 128 && id.bytes().all(|b| b.is_ascii_hexdigit());
    (valid && segments.next().is_none()).then(|| id.to_ascii_lowercase())
}

/// Where the head of an account comes from; also its cache key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HeadSource {
    /// The profile's own skin on Mojang's texture host.
    Texture { url: String, id: String },
    /// A default skin from a local client jar.
    Default(DefaultSkin),
}

impl HeadSource {
    /// Offline accounts wear the default skin of their UUID, as in the game. Microsoft accounts
    /// use their profile's skin; without a known one (never refreshed) there is no head, since
    /// guessing would show someone a face that is not theirs.
    pub fn for_account(account: &Account) -> Option<Self> {
        match account.kind {
            AccountKind::Offline => Some(Self::Default(DefaultSkin::for_profile(account.uuid))),
            AccountKind::Microsoft => {
                let url = &account.skin.as_ref()?.url;
                let id = texture_id(url)?;
                Some(Self::Texture {
                    url: url.clone(),
                    id,
                })
            }
        }
    }

    /// File name stem in the [`HeadCache`].
    pub fn cache_key(&self) -> String {
        match self {
            Self::Texture { id, .. } => format!("skin-{id}"),
            Self::Default(skin) => format!("default-{}-{}", skin.name, skin.variant.api_name()),
        }
    }
}

/// Why a texture gave no head.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HeadError {
    #[error("not a readable PNG: {0}")]
    Decode(String),
    #[error("not a skin texture: {width}x{height}")]
    Size { width: u32, height: u32 },
    #[error("could not encode the head: {0}")]
    Encode(String),
}

/// An image as 8-bit RGBA rows.
struct Rgba {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl Rgba {
    fn at(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.width + x) * 4) as usize;
        [
            self.pixels[i],
            self.pixels[i + 1],
            self.pixels[i + 2],
            self.pixels[i + 3],
        ]
    }
}

fn decode_rgba(bytes: &[u8]) -> Result<Rgba, HeadError> {
    let decode = |error: png::DecodingError| HeadError::Decode(error.to_string());
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(decode)?;
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| HeadError::Decode("image too large".to_owned()))?;
    let mut buffer = vec![0; size];
    let info = reader.next_frame(&mut buffer).map_err(decode)?;
    let data = &buffer[..info.buffer_size()];
    let pixels = rgba_pixels(data, info.color_type)
        .ok_or_else(|| HeadError::Decode("palette was not expanded".to_owned()))?;
    Ok(Rgba {
        width: info.width,
        height: info.height,
        pixels,
    })
}

/// The 8-bit pixels of a frame decoded with `normalize_to_color8` as RGBA. None for a palette
/// that was not expanded.
pub(crate) fn rgba_pixels(data: &[u8], color_type: png::ColorType) -> Option<Vec<u8>> {
    Some(match color_type {
        png::ColorType::Rgba => data.to_vec(),
        png::ColorType::Rgb => data
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|&[r, g, b]| [r, g, b, 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => data
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|&[g, a]| [g, g, g, a])
            .collect(),
        png::ColorType::Grayscale => data.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return None,
    })
}

/// `top` over `bottom` (straight alpha). `bottom` is opaque: the face is drawn without alpha.
fn over(bottom: [u8; 4], top: [u8; 4]) -> [u8; 4] {
    let alpha = u32::from(top[3]);
    if alpha == 0 {
        return bottom;
    }
    let mix =
        |b: u8, t: u8| ((u32::from(t) * alpha + u32::from(b) * (255 - alpha) + 127) / 255) as u8;
    [
        mix(bottom[0], top[0]),
        mix(bottom[1], top[1]),
        mix(bottom[2], top[2]),
        255,
    ]
}

/// Cuts the face of a skin texture (64×64, legacy 64×32, or a multiple of either) and lays the
/// hat layer over it, as the game draws the head from the front. Returns an RGBA PNG of
/// 8×8 pixels (8·scale for high-resolution textures).
///
/// Like the game, the face itself is drawn opaque, and a legacy 64×32 texture whose hat area
/// has no transparent pixel is treated as having no hat (old skins painted it solid).
pub fn head_png(texture: &[u8]) -> Result<Vec<u8>, HeadError> {
    let skin = decode_skin(texture)?;
    let hat = skin.has_hat();
    let SkinTexture { image, scale, .. } = skin;
    let size = 8 * scale;

    let mut pixels = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let [r, g, b, _] = image.at(8 * scale + x, 8 * scale + y);
            let mut pixel = [r, g, b, 255];
            if hat {
                pixel = over(pixel, image.at(40 * scale + x, 8 * scale + y));
            }
            pixels.extend_from_slice(&pixel);
        }
    }
    encode_rgba(size, size, &pixels)
}

/// A decoded skin texture: 64×64, legacy 64×32, or a multiple of either (at most 1024 wide).
struct SkinTexture {
    image: Rgba,
    /// Texture pixels per skin pixel.
    scale: u32,
    legacy: bool,
}

fn decode_skin(texture: &[u8]) -> Result<SkinTexture, HeadError> {
    if texture.len() as u64 > MAX_TEXTURE_BYTES {
        return Err(HeadError::Decode("file too large".to_owned()));
    }
    let image = decode_rgba(texture)?;
    let (width, height) = (image.width, image.height);
    let legacy = height * 2 == width;
    if width < 64 || width % 64 != 0 || width > 1024 || !(height == width || legacy) {
        return Err(HeadError::Size { width, height });
    }
    Ok(SkinTexture {
        scale: width / 64,
        legacy,
        image,
    })
}

impl SkinTexture {
    /// Whether the hat layer is drawn: always for 64×64 textures; for legacy ones only when the
    /// hat region (x 32..64, y 0..16) has a transparent pixel (old skins painted it solid).
    fn has_hat(&self) -> bool {
        let scale = self.scale;
        !self.legacy
            || (0..16 * scale)
                .flat_map(|y| (32 * scale..64 * scale).map(move |x| (x, y)))
                .any(|(x, y)| self.image.at(x, y)[3] < 128)
    }
}

/// One face of a body part: where it is in the texture and where it lands in the front view
/// (both in skin pixels), and whether it is drawn mirrored.
struct Face {
    texture: (u32, u32),
    at: (u32, u32),
    size: (u32, u32),
    mirror: bool,
}

const fn face(texture: (u32, u32), at: (u32, u32), size: (u32, u32)) -> Face {
    Face {
        texture,
        at,
        size,
        mirror: false,
    }
}

/// The front faces of the inner layer (drawn opaque) and of the outer layer (hat, jacket,
/// sleeves, trousers; blended over it), as the game lays them out in a 16×32 front view.
fn front_faces(variant: SkinVariant, legacy: bool) -> (Vec<Face>, Vec<Face>) {
    // Legacy textures predate slim arms.
    let arm = if variant == SkinVariant::Slim && !legacy {
        3
    } else {
        4
    };
    let mirrored = |texture, at, size| Face {
        texture,
        at,
        size,
        mirror: true,
    };
    let inner = vec![
        face((8, 8), (4, 0), (8, 8)),
        face((20, 20), (4, 8), (8, 12)),
        face((44, 20), (4 - arm, 8), (arm, 12)),
        // 64×32 textures reuse the right limbs for the left ones, mirrored.
        if legacy {
            mirrored((44, 20), (12, 8), (arm, 12))
        } else {
            face((36, 52), (12, 8), (arm, 12))
        },
        face((4, 20), (4, 20), (4, 12)),
        if legacy {
            mirrored((4, 20), (8, 20), (4, 12))
        } else {
            face((20, 52), (8, 20), (4, 12))
        },
    ];
    let mut outer = vec![face((40, 8), (4, 0), (8, 8))];
    if !legacy {
        outer.extend([
            face((20, 36), (4, 8), (8, 12)),
            face((44, 36), (4 - arm, 8), (arm, 12)),
            face((52, 52), (12, 8), (arm, 12)),
            face((4, 36), (4, 20), (4, 12)),
            face((4, 52), (8, 20), (4, 12)),
        ]);
    }
    (inner, outer)
}

/// The whole player seen from the front, standing still: head, body, arms and legs with their
/// outer layer, the way the game draws them. Slim skins have 3 px arms (the picture stays 16 px
/// wide; the outermost columns are left empty). Like the game, the inner layer is opaque, and a
/// legacy 64×32 texture whose hat area has no transparent pixel has no hat.
///
/// Returns an RGBA PNG of 16×32 pixels (times the scale of a high-resolution texture) with a
/// transparent background. Drawn once per skin for the skin history's thumbnails.
pub fn front_png(texture: &[u8], variant: SkinVariant) -> Result<Vec<u8>, HeadError> {
    let skin = decode_skin(texture)?;
    let hat = skin.has_hat();
    let SkinTexture {
        image,
        scale,
        legacy,
    } = skin;
    let (width, height) = (16 * scale, 32 * scale);
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    let (inner, outer) = front_faces(variant, legacy);
    let mut draw = |face: &Face, blend: bool| {
        let (w, h) = (face.size.0 * scale, face.size.1 * scale);
        for dy in 0..h {
            for dx in 0..w {
                let sx = if face.mirror { w - 1 - dx } else { dx };
                let top = image.at(face.texture.0 * scale + sx, face.texture.1 * scale + dy);
                let (x, y) = (face.at.0 * scale + dx, face.at.1 * scale + dy);
                let i = ((y * width + x) * 4) as usize;
                let pixel = if blend {
                    let bottom = [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]];
                    over(bottom, top)
                } else {
                    [top[0], top[1], top[2], 255]
                };
                pixels[i..i + 4].copy_from_slice(&pixel);
            }
        }
    };
    for part in &inner {
        draw(part, false);
    }
    for (index, part) in outer.iter().enumerate() {
        if index > 0 || hat {
            draw(part, true);
        }
    }
    encode_rgba(width, height, &pixels)
}

fn encode_rgba(width: u32, height: u32, pixels: &[u8]) -> Result<Vec<u8>, HeadError> {
    let encode = |error: png::EncodingError| HeadError::Encode(error.to_string());
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(encode)?;
    writer.write_image_data(pixels).map_err(encode)?;
    writer.finish().map_err(encode)?;
    Ok(out)
}

/// Client jars under a Minecraft `versions` folder (`<id>/<id>.jar`), most recently written
/// first, at most a dozen. Mod loader profiles without their own jar are skipped.
pub fn client_jars(versions_dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(versions_dir) else {
        return Vec::new();
    };
    let mut jars: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let jar = entry
                .path()
                .join(format!("{}.jar", entry.file_name().to_string_lossy()));
            let meta = fs::metadata(&jar).ok().filter(|meta| meta.is_file())?;
            Some((meta.modified().ok()?, jar))
        })
        .collect();
    jars.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    jars.into_iter()
        .take(MAX_JARS)
        .map(|(_, jar)| jar)
        .collect()
}

/// The texture of `skin` from the first jar in `jars` that has it. Missing, unreadable and older
/// jars (before 1.19.3) are skipped; `None` when no jar has it.
pub fn read_default_skin(jars: &[PathBuf], skin: DefaultSkin) -> Option<Vec<u8>> {
    let entry = skin.jar_entry();
    jars.iter().find_map(|jar| read_jar_entry(jar, &entry))
}

fn read_jar_entry(jar: &Path, entry: &str) -> Option<Vec<u8>> {
    let file = fs::File::open(jar).ok()?;
    let mut archive = zip::ZipArchive::new(BufReader::new(file)).ok()?;
    let file = archive.by_name(entry).ok()?;
    if file.size() > MAX_TEXTURE_BYTES {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(MAX_TEXTURE_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() as u64 <= MAX_TEXTURE_BYTES).then_some(bytes)
}

/// Head PNGs on disk, one file per [`HeadSource::cache_key`]. Texture ids are content
/// addresses and default skins never change, so entries never go stale.
#[derive(Debug, Clone)]
pub struct HeadCache {
    dir: PathBuf,
}

impl HeadCache {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn path(&self, source: &HeadSource) -> PathBuf {
        self.dir.join(format!("{}.png", source.cache_key()))
    }

    /// The cached head of `account`, if it has one (no network, no jar reading).
    pub fn for_account(&self, account: &Account) -> Option<Vec<u8>> {
        self.get(&HeadSource::for_account(account)?)
    }

    /// The cached head, if there is a readable one.
    pub fn get(&self, source: &HeadSource) -> Option<Vec<u8>> {
        let bytes = fs::read(self.path(source)).ok()?;
        bytes.starts_with(b"\x89PNG\r\n\x1a\n").then_some(bytes)
    }

    /// Stores a head (written to a temporary file first, then renamed into place).
    pub fn put(&self, source: &HeadSource, png: &[u8]) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        let path = self.path(source);
        let tmp = path.with_extension("png.tmp");
        let result = (|| {
            let mut file = fs::File::create(&tmp)?;
            file.write_all(png)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&tmp, &path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        result
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::offline::offline_uuid;

    /// A real RGBA PNG: `paint(x, y)` gives each pixel.
    pub(crate) fn texture(width: u32, height: u32, paint: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
        let mut pixels = Vec::new();
        for y in 0..height {
            for x in 0..width {
                pixels.extend_from_slice(&paint(x, y));
            }
        }
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&pixels).unwrap();
        writer.finish().unwrap();
        out
    }

    fn pixels(png: &[u8]) -> (u32, Vec<[u8; 4]>) {
        let image = decode_rgba(png).unwrap();
        assert_eq!(image.width, image.height);
        let list = (0..image.height)
            .flat_map(|y| (0..image.width).map(move |x| (x, y)))
            .map(|(x, y)| image.at(x, y))
            .collect();
        (image.width, list)
    }

    const FACE: [u8; 4] = [200, 150, 100, 255];
    const HAT: [u8; 4] = [10, 20, 30, 255];
    const CLEAR: [u8; 4] = [0, 0, 0, 0];

    fn in_face(x: u32, y: u32) -> bool {
        (8..16).contains(&x) && (8..16).contains(&y)
    }

    fn in_hat(x: u32, y: u32) -> bool {
        (40..48).contains(&x) && (8..16).contains(&y)
    }

    #[test]
    fn default_skins_match_the_game() {
        // Computed with Java's UUID.nameUUIDFromBytes / hashCode / Math.floorMod.
        let cases = [
            ("Notch", -524_802_362, "makena", SkinVariant::Slim),
            ("Steve", 1_277_771_483, "efe", SkinVariant::Classic),
            ("Alex", 223_495_834, "makena", SkinVariant::Slim),
            ("EjderAvcisi", -1_497_001_071, "steve", SkinVariant::Classic),
            ("KorAtesi_", 1_820_912_999, "noor", SkinVariant::Slim),
            ("Madenci07", -840_181_890, "steve", SkinVariant::Slim),
            ("abc", -1_805_618_793, "steve", SkinVariant::Classic),
            (
                "A_1234567890_bcd",
                -1_295_221_410,
                "alex",
                SkinVariant::Slim,
            ),
            ("jeb_", -1_770_600_077, "ari", SkinVariant::Slim),
            ("Dinnerbone", 366_416_444, "efe", SkinVariant::Slim),
        ];
        for (name, hash, skin, variant) in cases {
            let uuid = offline_uuid(name);
            assert_eq!(java_uuid_hash(uuid), hash, "{name}");
            assert_eq!(
                DefaultSkin::for_profile(uuid),
                DefaultSkin {
                    name: skin,
                    variant
                },
                "{name}"
            );
        }
        let notch = Uuid::parse_str("069a79f4-44e9-4726-a5be-fca90e38aaf5").unwrap();
        assert_eq!(java_uuid_hash(notch), -369_792_882);
        assert_eq!(DefaultSkin::for_profile(Uuid::nil()).name, "alex");
        assert_eq!(DefaultSkin::for_profile(Uuid::max()).name, "alex");
        assert_eq!(
            DefaultSkin::for_profile(offline_uuid("Steve")).jar_entry(),
            "assets/minecraft/textures/entity/player/wide/efe.png"
        );
    }

    #[test]
    fn texture_ids_only_come_from_mojang() {
        assert_eq!(
            texture_id("http://textures.minecraft.net/texture/ABC123").as_deref(),
            Some("abc123")
        );
        assert_eq!(
            texture_id("https://textures.minecraft.net/texture/abc").as_deref(),
            Some("abc")
        );
        for bad in [
            "https://evil.example/texture/abc",
            "https://textures.minecraft.net/texture/abc/def",
            "https://textures.minecraft.net/texture/not-hex",
            "https://textures.minecraft.net/skin/abc",
            "https://textures.minecraft.net/texture/",
            "https://textures.minecraft.net/texture/..%2F..",
        ] {
            assert_eq!(texture_id(bad), None, "{bad}");
        }
    }

    #[test]
    fn head_sources_and_keys() {
        let mut offline = Account::new(AccountKind::Offline, "Steve", offline_uuid("Steve"));
        let source = HeadSource::for_account(&offline).unwrap();
        assert_eq!(source.cache_key(), "default-efe-classic");
        // Offline accounts never use a stored skin URL.
        offline.skin = Some(crate::model::Skin {
            url: "https://textures.minecraft.net/texture/abc".into(),
            variant: SkinVariant::Classic,
            alias: None,
        });
        assert_eq!(HeadSource::for_account(&offline), Some(source));

        let mut microsoft = Account::new(AccountKind::Microsoft, "Steve", Uuid::new_v4());
        assert_eq!(HeadSource::for_account(&microsoft), None);
        microsoft.skin = Some(crate::model::Skin {
            url: "http://textures.minecraft.net/texture/F00D".into(),
            variant: SkinVariant::Slim,
            alias: None,
        });
        let source = HeadSource::for_account(&microsoft).unwrap();
        assert_eq!(source.cache_key(), "skin-f00d");
        microsoft.skin.as_mut().unwrap().url = "https://example.com/texture/f00d".into();
        assert_eq!(HeadSource::for_account(&microsoft), None);
    }

    #[test]
    fn the_head_is_the_face_under_the_hat() {
        // Hat covers the top half of the face; the rest of the hat layer is transparent.
        let skin = texture(64, 64, |x, y| {
            if in_face(x, y) {
                FACE
            } else if in_hat(x, y) && y < 12 {
                HAT
            } else {
                CLEAR
            }
        });
        let (size, head) = pixels(&head_png(&skin).unwrap());
        assert_eq!(size, 8);
        assert_eq!(head[0], HAT);
        assert_eq!(head[3 * 8 + 7], HAT);
        assert_eq!(head[4 * 8], FACE);
        assert_eq!(head[63], FACE);
    }

    #[test]
    fn half_transparent_hats_blend_and_faces_are_opaque() {
        let skin = texture(64, 64, |x, y| {
            if in_face(x, y) {
                [100, 100, 100, 0]
            } else if in_hat(x, y) {
                [200, 0, 0, 128]
            } else {
                CLEAR
            }
        });
        let (_, head) = pixels(&head_png(&skin).unwrap());
        assert_eq!(head[0], [150, 50, 50, 255]);
    }

    #[test]
    fn legacy_skins_with_a_solid_hat_area_have_no_hat() {
        let solid = texture(64, 32, |x, y| {
            if in_face(x, y) {
                FACE
            } else if x >= 32 && y < 16 {
                HAT
            } else {
                CLEAR
            }
        });
        let (_, head) = pixels(&head_png(&solid).unwrap());
        assert!(head.iter().all(|p| *p == FACE));

        let with_hat = texture(64, 32, |x, y| {
            if in_face(x, y) {
                FACE
            } else if in_hat(x, y) {
                HAT
            } else {
                CLEAR
            }
        });
        let (_, head) = pixels(&head_png(&with_hat).unwrap());
        assert!(head.iter().all(|p| *p == HAT));
    }

    #[test]
    fn high_resolution_textures_keep_their_detail() {
        let skin = texture(128, 128, |x, y| {
            if (16..32).contains(&x) && (16..32).contains(&y) {
                FACE
            } else {
                CLEAR
            }
        });
        let (size, head) = pixels(&head_png(&skin).unwrap());
        assert_eq!(size, 16);
        assert!(head.iter().all(|p| *p == FACE));
    }

    #[test]
    fn non_skins_are_rejected() {
        assert!(matches!(head_png(b"not a png"), Err(HeadError::Decode(_))));
        assert_eq!(
            head_png(&texture(32, 32, |_, _| FACE)),
            Err(HeadError::Size {
                width: 32,
                height: 32
            })
        );
        assert_eq!(
            head_png(&texture(64, 48, |_, _| FACE)),
            Err(HeadError::Size {
                width: 64,
                height: 48
            })
        );
    }

    fn jar_with(dir: &Path, version: &str, entries: &[(&str, &[u8])]) -> PathBuf {
        let folder = dir.join(version);
        fs::create_dir_all(&folder).unwrap();
        let path = folder.join(format!("{version}.jar"));
        let mut writer = zip::ZipWriter::new(fs::File::create(&path).unwrap());
        for (name, bytes) in entries {
            writer
                .start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap();
        path
    }

    #[test]
    fn default_skins_come_from_local_client_jars() {
        let dir = tempfile::tempdir().unwrap();
        let versions = dir.path().join("versions");
        let skin = DefaultSkin::for_profile(offline_uuid("Steve"));
        let efe = texture(64, 64, |x, y| if in_face(x, y) { FACE } else { CLEAR });
        jar_with(
            &versions,
            "1.18.2",
            &[("assets/minecraft/textures/entity/steve.png", &efe)],
        );
        // A mod loader profile without a jar, and a broken jar.
        fs::create_dir_all(versions.join("1.21.1-fabric")).unwrap();
        fs::create_dir_all(versions.join("broken")).unwrap();
        fs::write(versions.join("broken").join("broken.jar"), b"not a zip").unwrap();
        assert_eq!(read_default_skin(&client_jars(&versions), skin), None);

        jar_with(&versions, "26.3", &[(&skin.jar_entry(), &efe)]);
        let jars = client_jars(&versions);
        assert_eq!(jars.len(), 3);
        assert_eq!(read_default_skin(&jars, skin), Some(efe));
        assert!(client_jars(&dir.path().join("missing")).is_empty());
    }

    #[test]
    fn the_cache_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let cache = HeadCache::new(dir.path().join("heads"));
        let source = HeadSource::Default(DefaultSkin::for_profile(offline_uuid("Alex")));
        assert_eq!(cache.get(&source), None);
        let head = head_png(&texture(64, 64, |_, _| FACE)).unwrap();
        cache.put(&source, &head).unwrap();
        assert_eq!(cache.get(&source), Some(head));
        assert!(
            dir.path()
                .join("heads")
                .join("default-makena-slim.png")
                .is_file()
        );
        // Garbage in the cache is ignored.
        fs::write(
            dir.path().join("heads").join("default-makena-slim.png"),
            b"x",
        )
        .unwrap();
        assert_eq!(cache.get(&source), None);
    }

    /// Paints every front face of a 64×64 skin in its own colour (inner layer) and leaves the
    /// outer layer transparent unless `outer` paints it.
    fn front_skin(outer: impl Fn(u32, u32) -> Option<[u8; 4]>) -> Vec<u8> {
        texture(64, 64, |x, y| {
            if let Some(pixel) = outer(x, y) {
                return pixel;
            }
            let inside = |x0: u32, y0: u32, w: u32, h: u32| {
                (x0..x0 + w).contains(&x) && (y0..y0 + h).contains(&y)
            };
            if inside(8, 8, 8, 8) {
                FACE
            } else if inside(20, 20, 8, 12) {
                [0, 0, 200, 255]
            } else if inside(44, 20, 4, 12) {
                // Right arm: its first column is darker, to see which way it is drawn.
                if x == 44 {
                    [90, 0, 0, 255]
                } else {
                    [200, 0, 0, 255]
                }
            } else if inside(36, 52, 4, 12) {
                [0, 200, 0, 255]
            } else if inside(4, 20, 4, 12) {
                [200, 200, 0, 255]
            } else if inside(20, 52, 4, 12) {
                [0, 200, 200, 255]
            } else {
                CLEAR
            }
        })
    }

    fn front(png: &[u8]) -> (u32, u32, Vec<[u8; 4]>) {
        let image = decode_rgba(png).unwrap();
        let list = (0..image.height)
            .flat_map(|y| (0..image.width).map(move |x| (x, y)))
            .map(|(x, y)| image.at(x, y))
            .collect();
        (image.width, image.height, list)
    }

    #[test]
    fn the_front_view_lays_out_every_part() {
        let (width, height, pixels) =
            front(&front_png(&front_skin(|_, _| None), SkinVariant::Classic).unwrap());
        assert_eq!((width, height), (16, 32));
        let at = |x: u32, y: u32| pixels[(y * 16 + x) as usize];
        assert_eq!(at(4, 0), FACE);
        assert_eq!(at(11, 7), FACE);
        assert_eq!(at(4, 8), [0, 0, 200, 255]);
        // The right arm on the viewer's left, its first texture column outermost.
        assert_eq!(at(0, 8), [90, 0, 0, 255]);
        assert_eq!(at(3, 19), [200, 0, 0, 255]);
        assert_eq!(at(12, 8), [0, 200, 0, 255]);
        assert_eq!(at(15, 19), [0, 200, 0, 255]);
        assert_eq!(at(4, 20), [200, 200, 0, 255]);
        assert_eq!(at(8, 31), [0, 200, 200, 255]);
        // Beside the head and between nothing: transparent.
        assert_eq!(at(0, 0), CLEAR);
        assert_eq!(at(15, 7), CLEAR);
    }

    #[test]
    fn slim_arms_are_three_pixels_wide() {
        let (_, _, pixels) =
            front(&front_png(&front_skin(|_, _| None), SkinVariant::Slim).unwrap());
        let at = |x: u32, y: u32| pixels[(y * 16 + x) as usize];
        assert_eq!(at(0, 10), CLEAR);
        assert_eq!(at(1, 10), [90, 0, 0, 255]);
        assert_eq!(at(14, 10), [0, 200, 0, 255]);
        assert_eq!(at(15, 10), CLEAR);
    }

    #[test]
    fn the_outer_layer_blends_over_the_inner_one() {
        let jacket = [255, 255, 255, 128];
        let skin =
            front_skin(|x, y| ((20..28).contains(&x) && (36..48).contains(&y)).then_some(jacket));
        let (_, _, pixels) = front(&front_png(&skin, SkinVariant::Classic).unwrap());
        // 0,0,200 under half-transparent white.
        assert_eq!(pixels[(8 * 16 + 4) as usize], [128, 128, 228, 255]);
    }

    #[test]
    fn legacy_skins_mirror_the_right_limbs() {
        let skin = texture(64, 32, |x, y| {
            if (44..48).contains(&x) && (20..32).contains(&y) {
                if x == 44 {
                    [90, 0, 0, 255]
                } else {
                    [200, 0, 0, 255]
                }
            } else if (32..64).contains(&x) && y < 16 {
                // A solid hat area: no hat, as in the game.
                HAT
            } else if in_face(x, y) {
                FACE
            } else {
                CLEAR
            }
        });
        // The arm model is ignored: legacy skins have no slim arms.
        let (width, height, pixels) = front(&front_png(&skin, SkinVariant::Slim).unwrap());
        assert_eq!((width, height), (16, 32));
        let at = |x: u32, y: u32| pixels[(y * 16 + x) as usize];
        assert_eq!(at(0, 8), [90, 0, 0, 255]);
        assert_eq!(at(15, 8), [90, 0, 0, 255]);
        assert_eq!(at(12, 8), [200, 0, 0, 255]);
        assert_eq!(at(6, 4), FACE);
    }

    #[test]
    fn high_resolution_front_views_keep_their_detail() {
        let skin = texture(128, 128, |x, y| {
            if (16..32).contains(&x) && (16..32).contains(&y) {
                FACE
            } else {
                CLEAR
            }
        });
        let (width, height, pixels) = front(&front_png(&skin, SkinVariant::Classic).unwrap());
        assert_eq!((width, height), (32, 64));
        assert_eq!(pixels[(2 * 32 + 8) as usize], FACE);
        assert!(matches!(
            front_png(&texture(32, 32, |_, _| FACE), SkinVariant::Classic),
            Err(HeadError::Size { .. })
        ));
        assert!(front_png(b"nope", SkinVariant::Classic).is_err());
    }
}

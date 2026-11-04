//! People's pictures on the overlay's faces (OVL-AVATARS; the owner: "kullanıcının ilk harfi yerine avatarı
//! gözükse daha iyi olur fpsi etkilemeyecekse").
//!
//! The launcher's own rule (`SocialFace` in `apps/launcher/src/features/social/social-face.tsx`): a bundled preset
//! (`preset:<slug>`), the Minecraft head cut from the skin (`minecraft:<texture hash>`), the photo (`avatarUrl`, not
//! Discord's stand-in for an account without one), else the name's letter. The launcher fetches each picture once,
//! only from [`pl_content::LAUNCHER_IMAGE_HOSTS`] (no third-party avatar service), and prepares it as the faces draw
//! it: [`SIZE`] pixels square on the launcher's face colour with the player head's rounded corners, a small PNG the
//! mod decodes off its render thread, uploads once and keeps by person and version. The mod never goes to the
//! network.

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::io::Cursor;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use image::imageops::{self, FilterType};
use image::{ExtendedColorType, ImageEncoder, RgbaImage};
use tokio::sync::{OnceCell, Semaphore};

/// A prepared picture's pixels, square (a face is at most 16 GUI pixels, 64 at GUI 4).
pub const SIZE: u32 = 64;
/// A picture's source bytes at most (a Discord photo at 64 px, a skin and a preset are far below).
pub const MAX_SOURCE: usize = 1 << 20;
/// A source picture's sides at most (a skin is 64, a preset 128, Discord's photo 64): a small file can claim a huge
/// picture, and decoding it would hold the launcher's memory while the game runs.
const MAX_SIDE: u32 = 512;
/// What decoding one picture may allocate at most (512 × 512 at 16 bits a channel is 2 MiB).
const MAX_DECODED: u64 = 8 << 20;
/// One picture's fetch at most, retries included: a stalled one keeps the letter (asked for again later).
const FETCH_FOR: Duration = Duration::from_secs(30);
/// Fetches on their way at once (a panel full of new faces opens), and pictures decoded at once (each holds up to
/// [`MAX_DECODED`] while it is).
const FETCHES: usize = 6;
const DECODES: usize = 2;
/// Prepared pictures kept, by version (a few hundred kilobytes).
const CACHE: usize = 256;
/// A picture that could not be had is not asked for again this long (offline, refused).
const FAILED_FOR: Duration = Duration::from_secs(600);
/// Discord's stand-in for an account without a picture: its logo, not the player's face (`own-photo.ts`).
const DISCORD_STAND_IN: &str = "https://cdn.discordapp.com/embed/avatars/";
/// Discord's pictures of people.
const DISCORD_AVATARS: &str = "https://cdn.discordapp.com/avatars/";
/// What shows through a picture's see-through parts: the launcher's face (`bg-obsidian-800`, the mod's
/// `Theme.OBSIDIAN_800`), so a face reads the same over any panel or scene.
const BEHIND: [u8; 3] = [0x1b, 0x18, 0x20];

/// Where a person's picture comes from.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Picture {
    /// One of the launcher's bundled pictures, by slug.
    Preset(String),
    /// The head of a Minecraft skin, by its texture hash.
    Skin(String),
    /// A photo on one of the launcher's picture hosts.
    Photo(String),
}

impl Picture {
    /// What changes when the picture does (the mod keeps a picture by person and this): a short hash of its source.
    pub fn version(&self) -> String {
        let source = match self {
            Self::Preset(slug) => format!("p:{slug}"),
            Self::Skin(hash) => format!("s:{hash}"),
            Self::Photo(url) => format!("u:{url}"),
        };
        // FNV-1a, 64 bits: stable across runs and builds (the mod compares it, nothing secret).
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in source.bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
        format!("{hash:016x}")
    }

    /// The address to fetch, for the ones not in the launcher.
    fn url(&self) -> Option<String> {
        match self {
            Self::Preset(_) => None,
            Self::Skin(hash) => Some(format!("https://textures.minecraft.net/texture/{hash}")),
            Self::Photo(url) => Some(fetched(url)),
        }
    }
}

/// What is fetched for a photo: Discord's at the faces' size, an animated one as its first frame (the faces are
/// still, and there is no GIF decoder here); any other address as it is.
fn fetched(url: &str) -> String {
    let Some(rest) = url.strip_prefix(DISCORD_AVATARS) else {
        return url.to_owned();
    };
    let path = rest.split(['?', '#']).next().unwrap_or(rest);
    let path = path
        .strip_suffix(".gif")
        .map_or_else(|| path.to_owned(), |stem| format!("{stem}.png"));
    format!("{DISCORD_AVATARS}{path}?size={SIZE}")
}

/// A person's picture by the launcher's rule, or None for the letter: a preset this launcher has, else a skin's
/// head, else the photo. A preset it lacks (a newer launcher's) or a broken texture falls through to the photo, as
/// on the page.
pub fn picture_of(avatar: &str, avatar_url: Option<&str>) -> Option<Picture> {
    let avatar = avatar.trim();
    if let Some(slug) = avatar.strip_prefix("preset:")
        && preset(slug).is_some()
    {
        return Some(Picture::Preset(slug.to_owned()));
    }
    if let Some(hash) = avatar.strip_prefix("minecraft:")
        && (40..=64).contains(&hash.len())
        && hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Some(Picture::Skin(hash.to_owned()));
    }
    avatar_url
        .map(str::trim)
        .filter(|url| !url.starts_with(DISCORD_STAND_IN))
        .filter(|url| pl_content::is_launcher_image_url(url))
        .map(|url| Picture::Photo(url.to_owned()))
}

/// The launcher's bundled pictures (`apps/launcher/src/assets/avatars`, their 128 px files), by slug. The test
/// `the_presets_are_the_launchers` keeps the list the manifest's.
macro_rules! presets {
    ($($slug:literal),* $(,)?) => {
        &[$(($slug, include_bytes!(concat!("../../../apps/launcher/src/assets/avatars/", $slug, "-128.webp")))),*]
    };
}

static PRESETS: &[(&str, &[u8])] = presets![
    "golden-dragon",
    "ninja",
    "van-cat",
    "baby-dragon",
    "fox",
    "knight",
    "panda",
    "frost-dragon",
    "inventor",
    "owl",
    "wizard",
    "potion",
    "robot",
    "axolotl",
    "ember-dragon",
    "pirate",
    "lantern",
    "wolf",
    "crystal",
    "dwarf-miner",
    "compass",
    "bunny",
    "spellbook",
    "elf-ranger",
];

fn preset(slug: &str) -> Option<&'static [u8]> {
    PRESETS
        .iter()
        .find(|(name, _)| *name == slug)
        .map(|(_, bytes)| *bytes)
}

/// A picture's source as the faces draw it: [`SIZE`] square RGBA on the launcher's face colour with the player
/// head's rounded corners, as PNG. A skin gives its head (face and hat layer, pixel-sharp); anything else is cut to
/// its middle square. None for bytes that are no picture (or a skin of another shape).
pub fn prepare(bytes: &[u8], skin: bool) -> Option<Vec<u8>> {
    if bytes.len() > MAX_SOURCE {
        return None;
    }
    let source = decode(bytes)?;
    let mut face = if skin {
        head_of(&source)?
    } else {
        let side = source.width().min(source.height());
        if side == 0 {
            return None;
        }
        let x = (source.width() - side) / 2;
        let y = (source.height() - side) / 2;
        let square = imageops::crop_imm(&source, x, y, side, side).to_image();
        imageops::resize(&square, SIZE, SIZE, FilterType::Triangle)
    };
    flatten(&mut face);
    round_corners(&mut face);
    let mut out = Vec::with_capacity(8 * 1024);
    image::codecs::png::PngEncoder::new(Cursor::new(&mut out))
        .write_image(face.as_raw(), SIZE, SIZE, ExtendedColorType::Rgba8)
        .ok()?;
    Some(out)
}

/// `bytes` decoded within [`MAX_SIDE`] and [`MAX_DECODED`], or None (no picture, or too big to be a face's).
fn decode(bytes: &[u8]) -> Option<RgbaImage> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_DECODED);
    reader.limits(limits);
    Some(reader.decode().ok()?.to_rgba8())
}

/// The head of a 64×64 (or old 64×32) skin: the face at (8, 8) with the hat layer at (40, 8) over it, 8 × 8 each,
/// grown to [`SIZE`] without blurring.
fn head_of(skin: &RgbaImage) -> Option<RgbaImage> {
    if skin.width() != 64 || !(skin.height() == 64 || skin.height() == 32) {
        return None;
    }
    let mut head = imageops::crop_imm(skin, 8, 8, 8, 8).to_image();
    // The face itself is opaque: a skin with holes there would show the panel through it.
    for pixel in head.pixels_mut() {
        pixel.0[3] = 255;
    }
    let hat = imageops::crop_imm(skin, 40, 8, 8, 8).to_image();
    imageops::overlay(&mut head, &hat, 0, 0);
    Some(imageops::resize(&head, SIZE, SIZE, FilterType::Nearest))
}

/// A picture over the launcher's face colour ([`BEHIND`]): opaque everywhere.
fn flatten(face: &mut RgbaImage) {
    for pixel in face.pixels_mut() {
        let [r, g, b, a] = pixel.0;
        if a == 255 {
            continue;
        }
        let a = u16::from(a);
        // Rounded: (c·a + under·(255 − a) + 127) / 255 is at most 255.
        let mix = |c: u8, under: u8| {
            u8::try_from((u16::from(c) * a + u16::from(under) * (255 - a) + 127) / 255)
                .unwrap_or(u8::MAX)
        };
        pixel.0 = [mix(r, BEHIND[0]), mix(g, BEHIND[1]), mix(b, BEHIND[2]), 255];
    }
}

/// The player head's corners (4 px on its 32 px, the face's two GUI pixels on sixteen), smooth at the edge.
fn round_corners(face: &mut RgbaImage) {
    let size = face.width() as f32;
    let radius = size / 8.0;
    for (x, y, pixel) in face.enumerate_pixels_mut() {
        let px = x as f32 + 0.5;
        let py = y as f32 + 0.5;
        let cx = px.clamp(radius, size - radius);
        let cy = py.clamp(radius, size - radius);
        let distance = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
        let coverage = (radius - distance + 0.5).clamp(0.0, 1.0);
        if coverage < 1.0 {
            pixel.0[3] = (f32::from(pixel.0[3]) * coverage).round() as u8;
        }
    }
}

/// Fetches an `https://` picture on the launcher's picture hosts: its bytes, or None (offline, refused, too big).
pub type FetchFn =
    Arc<dyn Fn(String) -> Pin<Box<dyn Future<Output = Option<Vec<u8>>> + Send>> + Send + Sync>;

/// A [`FetchFn`] from an async function of the address.
pub fn fetch_fn<F, Fut>(fetch: F) -> FetchFn
where
    F: Fn(String) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Option<Vec<u8>>> + Send + 'static,
{
    Arc::new(
        move |url: String| -> Pin<Box<dyn Future<Output = Option<Vec<u8>>> + Send>> {
            Box::pin(fetch(url))
        },
    )
}

/// The prepared picture (None: it could not be had), once: whoever asks while it is on its way waits for the same.
type Prepared = Arc<OnceCell<Option<Arc<Vec<u8>>>>>;

/// One picture of the cache, and since when.
struct Slot {
    png: Prepared,
    since: Instant,
}

/// The prepared pictures every game's session shares, by version (a few hundred at most, the oldest go first), and
/// how they are fetched (the launcher's HTTP client; none until it says).
pub struct Pictures {
    fetch: Mutex<Option<FetchFn>>,
    cache: Mutex<(HashMap<String, Slot>, VecDeque<String>)>,
    fetching: Semaphore,
    decoding: Semaphore,
}

impl Default for Pictures {
    fn default() -> Self {
        Self {
            fetch: Mutex::default(),
            cache: Mutex::default(),
            fetching: Semaphore::new(FETCHES),
            decoding: Semaphore::new(DECODES),
        }
    }
}

impl Pictures {
    pub fn set_fetch(&self, fetch: FetchFn) {
        *self.fetch.lock().unwrap_or_else(|e| e.into_inner()) = Some(fetch);
    }

    /// `picture` as the faces draw it: from the cache, else fetched and prepared once (the decoding off the
    /// caller's thread), however many ask at a time. None while it cannot be had.
    pub async fn prepared(&self, picture: &Picture) -> Option<Arc<Vec<u8>>> {
        // Nothing is remembered before the launcher says how to fetch (a picture of its own aside).
        let fetch = match picture {
            Picture::Preset(_) => None,
            _ => Some(
                self.fetch
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone()?,
            ),
        };
        let version = picture.version();
        let png = {
            let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            let (map, order) = &mut *cache;
            let failed_long_ago = map.get(&version).is_some_and(|slot| {
                matches!(slot.png.get(), Some(None)) && slot.since.elapsed() >= FAILED_FOR
            });
            if failed_long_ago {
                map.remove(&version);
                order.retain(|kept| *kept != version);
            }
            match map.get(&version) {
                Some(slot) => Arc::clone(&slot.png),
                None => {
                    let png = Prepared::default();
                    map.insert(
                        version.clone(),
                        Slot {
                            png: Arc::clone(&png),
                            since: Instant::now(),
                        },
                    );
                    order.push_back(version);
                    while order.len() > CACHE {
                        if let Some(old) = order.pop_front() {
                            map.remove(&old);
                        }
                    }
                    png
                }
            }
        };
        png.get_or_init(|| self.load(picture, fetch)).await.clone()
    }

    /// Fetches (only from the launcher's picture hosts, only up to [`MAX_SOURCE`], [`FETCHES`] at a time) and
    /// prepares one picture ([`DECODES`] at a time, off the async threads).
    async fn load(&self, picture: &Picture, fetch: Option<FetchFn>) -> Option<Arc<Vec<u8>>> {
        let source = match picture {
            Picture::Preset(slug) => preset(slug)?.to_vec(),
            _ => {
                let url = picture.url()?;
                if !pl_content::is_launcher_image_url(&url) {
                    return None;
                }
                let fetch = fetch?;
                let _turn = self.fetching.acquire().await.ok()?;
                tokio::time::timeout(FETCH_FOR, fetch(url))
                    .await
                    .ok()
                    .flatten()
                    .filter(|bytes| bytes.len() <= MAX_SOURCE)?
            }
        };
        let skin = matches!(picture, Picture::Skin(_));
        let _turn = self.decoding.acquire().await.ok()?;
        tokio::task::spawn_blocking(move || prepare(&source, skin))
            .await
            .ok()
            .flatten()
            .map(Arc::new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_of(image: &RgbaImage) -> Vec<u8> {
        let mut out = Vec::new();
        image::codecs::png::PngEncoder::new(Cursor::new(&mut out))
            .write_image(
                image.as_raw(),
                image.width(),
                image.height(),
                ExtendedColorType::Rgba8,
            )
            .unwrap();
        out
    }

    #[test]
    fn the_rule_is_the_launchers() {
        assert_eq!(
            picture_of("preset:fox", None),
            Some(Picture::Preset("fox".into()))
        );
        assert_eq!(
            picture_of("preset:unknown", None),
            None,
            "a preset this build lacks"
        );
        let hash = "a".repeat(64);
        assert_eq!(
            picture_of(&format!("minecraft:{hash}"), None),
            Some(Picture::Skin(hash.clone()))
        );
        assert_eq!(picture_of("minecraft:not-a-hash", None), None);
        let photo = "https://cdn.discordapp.com/avatars/1/abc.png?size=128";
        assert_eq!(
            picture_of("discord", Some(photo)),
            Some(Picture::Photo(photo.into()))
        );
        // Discord's stand-in is its logo, not the player: the letter.
        assert_eq!(
            picture_of(
                "discord",
                Some("https://cdn.discordapp.com/embed/avatars/3.png")
            ),
            None
        );
        // No third-party host, no plain http.
        assert_eq!(
            picture_of("discord", Some("https://avatars.example.com/a.png")),
            None
        );
        assert_eq!(
            picture_of("discord", Some("http://cdn.discordapp.com/avatars/1/a.png")),
            None
        );
        assert_eq!(picture_of("discord", None), None);
        // A preset wins over a photo, as on the page.
        assert_eq!(
            picture_of("preset:owl", Some(photo)),
            Some(Picture::Preset("owl".into()))
        );
        // A preset this build lacks (a newer launcher's) or a broken texture: the photo, as on the page.
        for chosen in ["preset:unknown", "minecraft:not-a-hash"] {
            assert_eq!(
                picture_of(chosen, Some(photo)),
                Some(Picture::Photo(photo.into())),
                "{chosen}"
            );
        }
    }

    #[test]
    fn a_discord_photo_is_fetched_small_and_still() {
        assert_eq!(
            Picture::Photo("https://cdn.discordapp.com/avatars/1/abc.png?size=128".into()).url(),
            Some("https://cdn.discordapp.com/avatars/1/abc.png?size=64".into())
        );
        assert_eq!(
            Picture::Photo("https://cdn.discordapp.com/avatars/1/a_abc.gif?size=128".into()).url(),
            Some("https://cdn.discordapp.com/avatars/1/a_abc.png?size=64".into())
        );
        let other = "https://cdn.projects.gg/faces/a.webp";
        assert_eq!(Picture::Photo(other.into()).url(), Some(other.into()));
        assert_eq!(
            Picture::Skin("b".repeat(64)).url(),
            Some(format!(
                "https://textures.minecraft.net/texture/{}",
                "b".repeat(64)
            ))
        );
        assert_eq!(Picture::Preset("fox".into()).url(), None);
    }

    #[test]
    fn a_version_follows_the_source() {
        let a = Picture::Photo("https://cdn.discordapp.com/avatars/1/a.png".into());
        let b = Picture::Photo("https://cdn.discordapp.com/avatars/1/b.png".into());
        assert_eq!(a.version(), a.clone().version());
        assert_ne!(a.version(), b.version());
        assert_eq!(a.version().len(), 16);
    }

    #[test]
    fn the_presets_are_the_launchers() {
        let manifest: serde_json::Value = serde_json::from_str(include_str!(
            "../../../apps/launcher/src/assets/avatars/manifest.json"
        ))
        .unwrap();
        let mut slugs: Vec<&str> = manifest["avatars"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["slug"].as_str().unwrap())
            .collect();
        let mut ours: Vec<&str> = PRESETS.iter().map(|(slug, _)| *slug).collect();
        slugs.sort_unstable();
        ours.sort_unstable();
        assert_eq!(ours, slugs, "a preset was added or removed: update PRESETS");
        // Each one decodes and prepares, small.
        for (slug, bytes) in PRESETS {
            let png = prepare(bytes, false).unwrap_or_else(|| panic!("{slug}"));
            assert!(png.len() < 24 * 1024, "{slug}: {} bytes", png.len());
            let face = image::load_from_memory(&png).unwrap().to_rgba8();
            assert_eq!((face.width(), face.height()), (SIZE, SIZE));
        }
    }

    #[test]
    fn a_photo_is_cut_square_on_the_faces_colour_with_round_corners() {
        // A wide picture: red on the left third, green in the middle, see-through on the right.
        let wide = RgbaImage::from_fn(300, 100, |x, _| match x {
            0..=99 => image::Rgba([255, 0, 0, 255]),
            100..=199 => image::Rgba([0, 255, 0, 255]),
            _ => image::Rgba([0, 0, 255, 0]),
        });
        let png = prepare(&png_of(&wide), false).unwrap();
        let face = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!((face.width(), face.height()), (SIZE, SIZE));
        // Its middle square: green.
        assert_eq!(face.get_pixel(32, 32).0, [0, 255, 0, 255]);
        // The corners are see-through, the edges' middles are not.
        assert_eq!(face.get_pixel(0, 0).0[3], 0);
        assert_eq!(face.get_pixel(SIZE - 1, SIZE - 1).0[3], 0);
        assert_eq!(face.get_pixel(32, 0).0[3], 255);
        assert!(prepare(b"not a picture", false).is_none());
        // A see-through photo shows the launcher's face colour, not the panel.
        let clear = RgbaImage::from_pixel(80, 80, image::Rgba([255, 255, 255, 0]));
        let face = image::load_from_memory(&prepare(&png_of(&clear), false).unwrap())
            .unwrap()
            .to_rgba8();
        assert_eq!(
            face.get_pixel(32, 32).0,
            [BEHIND[0], BEHIND[1], BEHIND[2], 255]
        );
    }

    #[test]
    fn a_picture_too_big_for_a_face_is_none() {
        // A small file can claim a huge picture: past the sides it is not decoded at all.
        let wide = RgbaImage::from_pixel(MAX_SIDE + 1, 1, image::Rgba([9, 9, 9, 255]));
        assert!(prepare(&png_of(&wide), false).is_none());
        let widest = RgbaImage::from_pixel(MAX_SIDE, 2, image::Rgba([9, 9, 9, 255]));
        assert!(prepare(&png_of(&widest), false).is_some());
    }

    #[test]
    fn a_skin_gives_its_head_with_the_hat() {
        let mut skin = RgbaImage::from_pixel(64, 64, image::Rgba([0, 0, 0, 0]));
        // The face: grey; the hat layer covers its top row in gold.
        for y in 8..16 {
            for x in 8..16 {
                skin.put_pixel(x, y, image::Rgba([120, 120, 120, 255]));
            }
        }
        for x in 40..48 {
            skin.put_pixel(x, 8, image::Rgba([250, 200, 60, 255]));
        }
        let png = prepare(&png_of(&skin), true).unwrap();
        let face = image::load_from_memory(&png).unwrap().to_rgba8();
        // Each skin pixel is 8 × 8: the hat's row over the face's top, the face below.
        assert_eq!(face.get_pixel(32, 4).0, [250, 200, 60, 255]);
        assert_eq!(face.get_pixel(32, 40).0, [120, 120, 120, 255]);
        // A picture that is no skin is no head.
        let odd = RgbaImage::from_pixel(32, 32, image::Rgba([1, 2, 3, 255]));
        assert!(prepare(&png_of(&odd), true).is_none());
    }

    fn skin_png() -> Vec<u8> {
        let mut skin = RgbaImage::from_pixel(64, 64, image::Rgba([90, 90, 90, 255]));
        skin.put_pixel(0, 0, image::Rgba([0, 0, 0, 0]));
        png_of(&skin)
    }

    #[tokio::test]
    async fn a_picture_is_fetched_once_and_a_failure_waits() {
        let pictures = Pictures::default();
        // Before the launcher says how to fetch: nothing, and nothing remembered.
        let ok = Picture::Skin("a".repeat(64));
        assert!(pictures.prepared(&ok).await.is_none());
        let calls = Arc::new(Mutex::new(Vec::<String>::new()));
        let seen = Arc::clone(&calls);
        let skin = skin_png();
        pictures.set_fetch(fetch_fn(move |url: String| {
            seen.lock().unwrap().push(url.clone());
            let bytes = skin.clone();
            async move { url.contains("texture/aa").then_some(bytes) }
        }));
        let first = pictures.prepared(&ok).await.unwrap();
        let again = pictures.prepared(&ok).await.unwrap();
        assert!(Arc::ptr_eq(&first, &again), "from the cache");
        let missing = Picture::Skin("b".repeat(64));
        assert!(pictures.prepared(&missing).await.is_none());
        assert!(pictures.prepared(&missing).await.is_none());
        assert_eq!(
            calls.lock().unwrap().len(),
            2,
            "once each, the failure not asked again yet"
        );
        // A preset needs no fetch.
        assert!(
            pictures
                .prepared(&Picture::Preset("fox".into()))
                .await
                .is_some()
        );
        assert_eq!(calls.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn two_asking_at_once_wait_for_one_fetch() {
        let pictures = Arc::new(Pictures::default());
        let calls = Arc::new(Mutex::new(0_u32));
        let seen = Arc::clone(&calls);
        let skin = skin_png();
        pictures.set_fetch(fetch_fn(move |_url: String| {
            *seen.lock().unwrap() += 1;
            let bytes = skin.clone();
            async move {
                tokio::time::sleep(Duration::from_millis(50)).await;
                Some(bytes)
            }
        }));
        let picture = Picture::Skin("c".repeat(64));
        let (a, b) = tokio::join!(pictures.prepared(&picture), pictures.prepared(&picture));
        assert!(Arc::ptr_eq(&a.unwrap(), &b.unwrap()));
        assert_eq!(*calls.lock().unwrap(), 1);
    }
}

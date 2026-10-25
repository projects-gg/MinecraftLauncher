//! Item icons for the cloud inventory and AFK+'s item pickers (ITEMART), rendered from a Minecraft
//! client jar the way the game draws them in an inventory: flat items from their layer textures,
//! blocks and other models as small isometric scenes, chests, shulker boxes, heads and beds from
//! their entity textures. The launcher builds the atlas at run time from the player's own jar
//! (nothing of the game is shipped or committed); the site gets one built ahead of time
//! (`examples/build_atlas.rs`) and served from our CDN.
//!
//! Output: one PNG atlas and an index (`AtlasIndex`, JSON) keyed by the item id
//! (`minecraft:diamond_sword`). A layer whose colour the server can change (a dyed leather piece,
//! a potion) is kept apart with its tint source and default colour, so a client multiplies it.
//! Overlays (Projects' own ItemsAdder items) are a separate document resolved first by the client.

pub mod data;
mod image;
mod itemdef;
mod math;
mod model;
mod pack;
mod render;
mod source;
mod special;

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use image::Image;
pub use source::{AssetSource, JarSource, MemSource};

use itemdef::{Part, Tint};
use math::Mat4;
use model::{Display, Model};
use render::Scene;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("client jar: {0}")]
    Jar(String),
    #[error("image: {0}")]
    Image(String),
    #[error("the jar has no item definitions or item models")]
    NoItems,
}

/// The index's format version; clients reject a newer one.
pub const INDEX_FORMAT: u32 = 1;

/// Bumped whenever icons are drawn differently, so caches of older drawings are redrawn.
/// 2: the game's slot lighting by face normals, beds, the piglin head and skins' hat layer, the
/// static glint frame, dark icons marked, hidden dye layers. 3: the dragon head and the decorated
/// pot. 4: copper golem statues, dark icons by their outline's lightness, 3D icons drawn at 48 px
/// too, cells on even pixels.
pub const ATLAS_REVISION: u32 = 4;

/// The atlas index (`index.json`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AtlasIndex {
    pub format: u32,
    /// The Minecraft version of the jar.
    pub minecraft: String,
    /// The atlas image's file name.
    pub image: String,
    /// The atlas size in pixels.
    pub size: [u32; 2],
    /// The static enchantment glint in the atlas, if the jar has the game's glint texture: an
    /// opaque square the client adds (`screen`) over an enchanted icon, masked by the icon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glint: Option<[u32; 4]>,
    /// The 3D icons drawn again at a larger size, in a second image a client loads only when it
    /// draws them that size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub large: Option<LargeAtlas>,
    /// Icons by item id.
    pub icons: BTreeMap<String, Icon>,
}

/// The second image of 3D icons drawn larger: 48 pixels, as the game draws them at GUI scale 3
/// (the 32 pixel drawing scaled by one and a half shows uneven pixels).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LargeAtlas {
    /// The image's file name.
    pub image: String,
    /// Its size in pixels.
    pub size: [u32; 2],
    /// The side of its icons in pixels.
    pub icon: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Icon {
    pub kind: IconKind,
    /// Drawn in order; each `rect` is 16 (flat) or the block size (3D) square.
    pub layers: Vec<Layer>,
    /// The icon's outline melts into a dark slot (obsidian, black wool, netherite): there the
    /// client draws it a quiet one-texel edge.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dark: bool,
    /// A 3D icon's larger drawing, `[x, y, width, height]` in the large atlas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub large: Option<[u32; 4]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IconKind {
    /// Layer textures, as the game draws handheld and generated items.
    Flat,
    /// A model rendered with its inventory transform.
    Block,
    /// A model the game draws in code (chest, shulker box, head, shield, bed, statue).
    Special,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    /// `[x, y, width, height]` in the atlas.
    pub rect: [u32; 4],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tint: Option<LayerTint>,
}

/// A layer the client multiplies by a colour: `source` is what the server sends
/// (`dye`, `potion`, `firework`, `map_color`, `team`, `custom_model_data`), `default` the colour
/// when it sends none (`#rrggbb`); without a default the layer is not drawn until it sends one
/// (undyed wolf armour has no dye layer).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayerTint {
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
}

#[derive(Clone, Debug)]
pub struct BuildOptions {
    pub minecraft: String,
    /// The pixel size of rendered (3D) icons; flat ones keep their texture size.
    pub block_size: u32,
    /// The atlas image's file name written into the index.
    pub image_name: String,
    /// The atlas width in pixels.
    pub atlas_width: u32,
    /// The size 3D icons are drawn at a second time, into the large atlas (None: no large atlas).
    pub large_size: Option<u32>,
    /// The large atlas image's file name written into the index.
    pub large_image_name: String,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            minecraft: String::new(),
            block_size: 32,
            image_name: "atlas.png".into(),
            atlas_width: 1024,
            large_size: Some(48),
            large_image_name: "atlas-48.png".into(),
        }
    }
}

pub struct Atlas {
    pub png: Vec<u8>,
    /// The large atlas image (`index.large`), when 3D icons were drawn larger.
    pub large_png: Option<Vec<u8>>,
    pub index: AtlasIndex,
    /// Items with a definition but no icon (drawn in code and not here, or broken).
    pub missing: Vec<String>,
}

/// The inventory view of a block model without its own (`block/block`'s).
const DEFAULT_BLOCK_GUI: Display = Display {
    rotation: [30.0, 225.0, 0.0],
    translation: [0.0, 0.0, 0.0],
    scale: [0.625, 0.625, 0.625],
};

/// Our darkest card colour (`--color-obsidian-800`), the ground a dark icon is measured against.
const DARK_GROUND: [u8; 3] = [0x1b, 0x18, 0x20];

/// An icon is marked dark when its outline is near-black on that ground: the median lightness of
/// its silhouette's edge (two texels deep) within about 6 L* of the ground's 8.9 (obsidian, black
/// wool, the ender chest, netherite armour; not a netherite ingot or a hopper, which read). And only
/// when the client's one-texel edge would not fill in between its parts: see-through structure
/// (leaves, sculk veins, spawners, chains) keeps its holes and gets no edge.
const DARK_EDGE_TEXELS: u32 = 2;
const DARK_EDGE_LIGHTNESS: f32 = 15.0;
const DARK_GAP_SHARE: f32 = 0.02;

/// A flat item's layers, each with the tint the client applies.
type FlatLayers = Vec<(Image, Option<LayerTint>)>;

/// A drawn icon before packing.
struct DrawnIcon {
    id: String,
    kind: IconKind,
    /// The layers with their tints.
    layers: FlatLayers,
    dark: bool,
    /// A 3D icon's larger drawing.
    large: Option<Image>,
}

struct Builder<'a> {
    source: &'a mut dyn AssetSource,
    textures: HashMap<String, Option<Image>>,
    models: HashMap<String, Option<Model>>,
    grass: Option<Image>,
    block_size: u32,
    large_size: Option<u32>,
}

/// A rendered (3D) icon: at the block size and, when asked, the large size.
struct Rendered {
    kind: IconKind,
    image: Image,
    large: Option<Image>,
}

/// What one item draws to; `preview` is the whole picture with the default colours.
enum Drawn {
    Flat { layers: FlatLayers, preview: Image },
    Rendered(Rendered),
}

fn hex(rgb: u32) -> String {
    format!("#{:06x}", rgb & 0xFF_FFFF)
}

/// Whether an ARGB tint shows anything (a transparent one hides its layer).
fn visible(argb: u32) -> bool {
    argb >> 24 != 0
}

impl Builder<'_> {
    fn texture(&mut self, id: &str) -> Option<Image> {
        if let Some(hit) = self.textures.get(id) {
            return hit.clone();
        }
        let image = self
            .source
            .read(&source::texture_path(id))
            .and_then(|bytes| Image::decode_png(&bytes).ok())
            .map(Image::first_frame);
        self.textures.insert(id.to_owned(), image.clone());
        image
    }

    fn model(&mut self, id: &str) -> Option<Model> {
        let key = source::full_id(id);
        if let Some(hit) = self.models.get(&key) {
            return hit.clone();
        }
        let model = model::load(self.source, &key);
        self.models.insert(key, model.clone());
        model
    }

    /// The biome grass colour the game shows items in (temperature and downfall).
    fn grass(&mut self, temperature: f32, downfall: f32) -> u32 {
        if self.grass.is_none() {
            self.grass = self.texture("minecraft:colormap/grass");
        }
        let Some(map) = &self.grass else {
            return 0x91BD59;
        };
        let t = temperature.clamp(0.0, 1.0);
        let d = (downfall.clamp(0.0, 1.0)) * t;
        let x = (((1.0 - t) * 255.0) as u32).min(map.width - 1);
        let y = (((1.0 - d) * 255.0) as u32).min(map.height - 1);
        let px = map.get(x, y);
        ((px[0] as u32) << 16) | ((px[1] as u32) << 8) | px[2] as u32
    }

    /// A tint's ARGB colour when drawn into the icon (defaults for what the server could change).
    fn tint_color(&mut self, tint: &Tint) -> Option<u32> {
        Some(match tint {
            Tint::Constant(c)
            | Tint::Dye(c)
            | Tint::Potion(c)
            | Tint::Firework(c)
            | Tint::MapColor(c)
            | Tint::Team(c)
            | Tint::CustomModelData(c) => *c,
            Tint::Grass {
                temperature,
                downfall,
            } => self.grass(*temperature, *downfall) | 0xFF00_0000,
            Tint::None => return None,
        })
    }

    /// A flat item's layers (tinted ones kept apart) and its whole picture.
    fn flat_layers(&mut self, model: &Model, tints: &[Tint]) -> (FlatLayers, Image) {
        let mut layers: FlatLayers = Vec::new();
        let sources: Vec<(i32, Image)> = model
            .layers()
            .into_iter()
            .filter_map(|(i, id)| self.texture(&id).map(|image| (i, image)))
            .collect();
        let size = sources
            .iter()
            .map(|(_, img)| img.width.max(img.height))
            .max()
            .unwrap_or(16);
        let mut preview = Image::new(size, size);
        for (i, image) in sources {
            let image = if image.width < size && size.is_multiple_of(image.width) {
                image.upscale(size / image.width)
            } else {
                image
            };
            let tint = tints.get(i as usize);
            let runtime = tint.and_then(|t| t.runtime_source().map(|s| (s, t.clone())));
            match runtime {
                Some((source, tint)) => {
                    let color = self.tint_color(&tint).unwrap_or(0xFFFF_FFFF);
                    if visible(color) {
                        preview.draw(&image.tinted(color), 0, 0);
                    }
                    layers.push((
                        image,
                        Some(LayerTint {
                            source: source.to_owned(),
                            default: visible(color).then(|| hex(color)),
                        }),
                    ));
                }
                None => {
                    let baked = match tint.and_then(|t| self.tint_color(t)) {
                        Some(argb) if !visible(argb) => continue,
                        Some(argb) => image.tinted(argb),
                        None => image,
                    };
                    preview.draw(&baked, 0, 0);
                    // Untinted layers fold into the one below when it is untinted too.
                    match layers.last_mut() {
                        Some((below, None)) if below.width == baked.width => {
                            below.draw(&baked, 0, 0)
                        }
                        _ => layers.push((baked, None)),
                    }
                }
            }
        }
        (layers, preview)
    }

    fn draw(&mut self, parts: &[Part]) -> Option<Drawn> {
        // A single flat model (most items): its layers.
        if let [Part::Model { id, tints, .. }] = parts {
            let model = self.model(id)?;
            if model.generated {
                let (layers, preview) = self.flat_layers(&model, tints);
                return (!layers.is_empty()).then_some(Drawn::Flat { layers, preview });
            }
        }
        // Otherwise a scene: every 3D part with its placement.
        let mut scene = Scene::default();
        let mut display: Option<Display> = None;
        let mut front = false;
        let mut kind = IconKind::Block;
        let mut flat_fallback: Option<(FlatLayers, Image)> = None;
        for part in parts {
            match part {
                Part::Model {
                    id,
                    tints,
                    transform,
                } => {
                    let Some(model) = self.model(id) else {
                        continue;
                    };
                    if model.generated {
                        if flat_fallback.is_none() {
                            flat_fallback = Some(self.flat_layers(&model, tints));
                        }
                        continue;
                    }
                    let colors: Vec<Option<u32>> =
                        tints.iter().map(|t| self.tint_color(t)).collect();
                    let place = transform.unwrap_or(Mat4::IDENTITY);
                    let face_tint = |slot: i32| colors.get(slot as usize).copied().flatten();
                    render::add_elements(
                        &mut scene,
                        &model,
                        &place,
                        &mut |tex| self.texture(tex),
                        &face_tint,
                    );
                    if display.is_none() {
                        display = model.gui;
                        front = model.front_light;
                    }
                }
                Part::Special {
                    kind: special_kind,
                    args,
                    base,
                    transform,
                } => {
                    let Some(special) = special::special(special_kind, args) else {
                        continue;
                    };
                    let place = transform.unwrap_or(Mat4::IDENTITY);
                    for (i, piece) in special.pieces.iter().enumerate() {
                        let Some(texture) = self.texture(&piece.texture) else {
                            continue;
                        };
                        let index = scene.add_texture(texture);
                        special.add(i, &mut scene, index, &place);
                    }
                    kind = IconKind::Special;
                    if display.is_none()
                        && let Some(base) = self.model(base)
                    {
                        display = base.gui;
                        front = base.front_light;
                    }
                }
            }
        }
        if scene.quads.is_empty() {
            return flat_fallback
                .filter(|(layers, _)| !layers.is_empty())
                .map(|(layers, preview)| Drawn::Flat { layers, preview });
        }
        let display = display.unwrap_or(DEFAULT_BLOCK_GUI);
        let draw = |size: u32| render::rasterize(&scene, &display, size, front);
        let image = draw(self.block_size);
        if image.is_blank() {
            return None;
        }
        Some(Drawn::Rendered(Rendered {
            kind,
            image,
            large: self.large_size.map(draw),
        }))
    }
}

/// The static glint frame: its side in the atlas, the glint texels it spans across, where it
/// starts in the glint texture and how far it is turned (the game's texture matrix turns the
/// glint by π/18), and the game's default glint strength.
const GLINT_FRAME: u32 = 32;
const GLINT_TEXELS: f32 = 14.0;
const GLINT_ORIGIN: [f32; 2] = [40.0, 56.0];
const GLINT_TURN_DEGREES: f32 = 10.0;
const GLINT_STRENGTH: f32 = 0.75;
/// How much of the game's added light stays everywhere, and how strongly the streaks show.
const GLINT_BASE: f32 = 0.3;
const GLINT_STREAK: f32 = 1.43;

/// The enchantment glint as one still frame, drawn from the game's own glint texture. The game
/// adds `(c · strength)²` of a moving texture; still, that reads as a purple wash over the whole
/// item, so the frame keeps only a little of it everywhere and lifts the texture's bright streaks:
/// the item keeps its own colours and a streak or two crosses it, as the moving glint does.
fn glint_frame(texture: &Image) -> Image {
    let added = |c: [f32; 3]| c.map(|v| (v * GLINT_STRENGTH).powi(2));
    let weight = |a: [f32; 3]| 0.3 * a[0] + 0.3 * a[1] + 0.4 * a[2];
    let texel = |x: u32, y: u32| {
        let px = texture.get(x, y);
        [px[0], px[1], px[2]].map(|c| c as f32 / 255.0)
    };
    let mut levels: Vec<(f32, [f32; 3])> = (0..texture.height)
        .flat_map(|y| (0..texture.width).map(move |x| (x, y)))
        .map(|(x, y)| {
            let c = texel(x, y);
            (weight(added(c)), c)
        })
        .collect();
    levels.sort_by(|a, b| a.0.total_cmp(&b.0));
    let threshold = levels[levels.len() * 85 / 100].0;
    let top = levels.last().map(|(level, _)| *level).unwrap_or(1.0);
    // The streaks' own colour, at full brightness and a little towards white.
    let streaks = &levels[levels.len() * 85 / 100..];
    let mut color = [0.0f32; 3];
    for (_, c) in streaks {
        for (sum, v) in color.iter_mut().zip(c) {
            *sum += v;
        }
    }
    let brightest = color.iter().copied().fold(f32::MIN_POSITIVE, f32::max);
    let color = color.map(|c| {
        let c = c / brightest;
        c + (1.0 - c) * 0.2
    });
    let (sin, cos) = GLINT_TURN_DEGREES.to_radians().sin_cos();
    let step = GLINT_TEXELS / GLINT_FRAME as f32;
    let mut frame = Image::new(GLINT_FRAME, GLINT_FRAME);
    for y in 0..GLINT_FRAME {
        for x in 0..GLINT_FRAME {
            let u = (x as f32 + 0.5) * step;
            let v = (y as f32 + 0.5) * step;
            let gx = cos * u - sin * v + GLINT_ORIGIN[0];
            let gy = sin * u + cos * v + GLINT_ORIGIN[1];
            let a = added(texture.sample_wrapped(gx, gy));
            let streak = ((weight(a) - threshold) / (top - threshold).max(1e-6)).clamp(0.0, 1.0);
            let px = [0, 1, 2].map(|i| {
                ((GLINT_BASE * a[i] + GLINT_STREAK * streak * color[i]).clamp(0.0, 1.0) * 255.0)
                    .round() as u8
            });
            frame.put(x, y, [px[0], px[1], px[2], 255]);
        }
    }
    frame
}

/// Every item id the jar defines: its item definitions (1.21.4 and later) or, for older jars,
/// its item models.
fn item_ids(source: &mut dyn AssetSource) -> (Vec<String>, bool) {
    let defs = source.list("assets/minecraft/items", ".json");
    if !defs.is_empty() {
        let ids = defs
            .iter()
            .filter_map(|p| {
                p.strip_prefix("assets/minecraft/items/")?
                    .strip_suffix(".json")
            })
            .map(|id| format!("minecraft:{id}"))
            .collect();
        return (ids, true);
    }
    let models = source.list("assets/minecraft/models/item", ".json");
    let ids = models
        .iter()
        .filter_map(|p| {
            p.strip_prefix("assets/minecraft/models/item/")?
                .strip_suffix(".json")
        })
        .map(|id| format!("minecraft:{id}"))
        .collect();
    (ids, false)
}

/// Whether the source has item definitions (`assets/minecraft/items/`, the game since 1.21.4).
/// Older jars name chests, heads, banners, shields and beds `builtin/entity`, which is not drawn,
/// so a newer jar draws more of an older version's items.
pub fn has_item_definitions(source: &mut dyn AssetSource) -> bool {
    !source.list("assets/minecraft/items", ".json").is_empty()
}

/// Builds the atlas for every item in the source.
pub fn build_atlas(source: &mut dyn AssetSource, options: &BuildOptions) -> Result<Atlas, Error> {
    let (ids, definitions) = item_ids(source);
    if ids.is_empty() {
        return Err(Error::NoItems);
    }
    let mut builder = Builder {
        source,
        textures: HashMap::new(),
        models: HashMap::new(),
        grass: None,
        block_size: options.block_size,
        large_size: options
            .large_size
            .filter(|&size| size != options.block_size),
    };
    let dark = |picture: &Image| {
        let texel = (picture.width / 16).max(1);
        picture
            .edge_lightness(DARK_GROUND, DARK_EDGE_TEXELS * texel)
            .is_some_and(|lightness| lightness <= DARK_EDGE_LIGHTNESS)
            && picture.gap_share(texel) <= DARK_GAP_SHARE
    };
    let mut drawn: Vec<DrawnIcon> = Vec::new();
    let mut missing = Vec::new();
    for id in &ids {
        let parts = if definitions {
            let path = source::item_definition_path(id);
            match builder
                .source
                .read(&path)
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            {
                Some(json) => itemdef::evaluate(&json),
                None => Vec::new(),
            }
        } else {
            let (_, path) = source::split_id(id);
            vec![Part::Model {
                id: format!("minecraft:item/{path}"),
                tints: Vec::new(),
                transform: None,
            }]
        };
        match builder.draw(&parts) {
            Some(Drawn::Flat { layers, preview }) => drawn.push(DrawnIcon {
                id: id.clone(),
                kind: IconKind::Flat,
                dark: dark(&preview),
                layers,
                large: None,
            }),
            Some(Drawn::Rendered(Rendered { kind, image, large })) => drawn.push(DrawnIcon {
                id: id.clone(),
                kind,
                dark: dark(&image),
                layers: vec![(image, None)],
                large,
            }),
            None => missing.push(id.clone()),
        }
    }
    let glint = builder
        .texture("minecraft:misc/enchanted_glint_item")
        .map(|texture| glint_frame(&texture));
    let mut images: Vec<&Image> = drawn
        .iter()
        .flat_map(|icon| icon.layers.iter().map(|(img, _)| img))
        .collect();
    let glint_slot = glint.as_ref().map(|g| {
        images.push(g);
        images.len() - 1
    });
    let (atlas, rects) = pack::pack(&images, options.atlas_width);
    let large_images: Vec<&Image> = drawn
        .iter()
        .filter_map(|icon| icon.large.as_ref())
        .collect();
    let large = (!large_images.is_empty()).then(|| pack::pack(&large_images, options.atlas_width));
    let mut icons = BTreeMap::new();
    let (mut next, mut next_large) = (0, 0);
    for icon in drawn {
        let layers = icon
            .layers
            .into_iter()
            .map(|(_, tint)| {
                let rect = rects[next];
                next += 1;
                Layer { rect, tint }
            })
            .collect();
        let large_rect = match (&icon.large, &large) {
            (Some(_), Some((_, large_rects))) => {
                next_large += 1;
                Some(large_rects[next_large - 1])
            }
            _ => None,
        };
        icons.insert(
            icon.id,
            Icon {
                kind: icon.kind,
                layers,
                dark: icon.dark,
                large: large_rect,
            },
        );
    }
    let index = AtlasIndex {
        format: INDEX_FORMAT,
        minecraft: options.minecraft.clone(),
        image: options.image_name.clone(),
        size: [atlas.width, atlas.height],
        glint: glint_slot.map(|i| rects[i]),
        large: large.as_ref().map(|(image, _)| LargeAtlas {
            image: options.large_image_name.clone(),
            size: [image.width, image.height],
            icon: builder.large_size.unwrap_or(options.block_size),
        }),
        icons,
    };
    Ok(Atlas {
        png: atlas.encode_png()?,
        large_png: large.map(|(image, _)| image.encode_png()).transpose()?,
        index,
        missing,
    })
}

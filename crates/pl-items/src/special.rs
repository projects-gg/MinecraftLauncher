//! Items the game draws in code from entity textures: chests, shulker boxes, mob heads (the dragon's
//! too), beds, the shield, banners, the conduit, the decorated pot and the copper golem statues.
//! Each is built from the game's own model parts (26.1: cuboids in pixels with the entity models'
//! texture layout, part poses, inflation, mirrored and hidden faces), then placed by the item
//! definition's `transformation`.

use serde_json::Value;

use crate::math::{Mat4, Vec3};
use crate::model::Dir;
use crate::render::{Quad, Scene};

/// A model part's pose (`PartPose`): offset in pixels, rotation in degrees (the game turns X, then
/// Y, then Z) and a uniform scale.
#[derive(Clone, Copy)]
struct Pose {
    offset: Vec3,
    rotation: Vec3,
    scale: f32,
}

const REST: Pose = Pose {
    offset: [0.0; 3],
    rotation: [0.0; 3],
    scale: 1.0,
};

impl Pose {
    /// In block units, as `ModelPart.translateAndRotate`: translate, turn, scale.
    fn matrix(&self) -> Mat4 {
        let [x, y, z] = self.offset;
        let [rx, ry, rz] = self.rotation;
        let turned = Mat4::translation([x / 16.0, y / 16.0, z / 16.0])
            .mul(&Mat4::rotation(2, rz))
            .mul(&Mat4::rotation(1, ry))
            .mul(&Mat4::rotation(0, rx));
        if self.scale == 1.0 {
            turned
        } else {
            turned.mul(&Mat4::scale([self.scale; 3]))
        }
    }
}

/// One cuboid of an entity model part.
struct Cuboid {
    /// Texture offset in the model's texture pixels.
    tex: [f32; 2],
    /// Origin and size in pixels, inside the part.
    origin: Vec3,
    size: Vec3,
    /// Grows the box on every side without changing its texture layout (`CubeDeformation`).
    inflate: f32,
    /// A face the model leaves out (the bed's inner faces).
    hidden: Option<Dir>,
    /// The only face the model keeps (the decorated pot's sides).
    only: Option<Dir>,
    /// Mirrored left to right with its texture (`CubeListBuilder.mirror`).
    mirror: bool,
    /// The part's pose, inside its parent part's (the dragon's jaw in its head).
    pose: Pose,
    parent: Pose,
}

const fn cuboid(tex: [f32; 2], origin: Vec3, size: Vec3) -> Cuboid {
    Cuboid {
        tex,
        origin,
        size,
        inflate: 0.0,
        hidden: None,
        only: None,
        mirror: false,
        pose: REST,
        parent: REST,
    }
}

impl Cuboid {
    const fn inflated(mut self, by: f32) -> Self {
        self.inflate = by;
        self
    }

    const fn without(mut self, face: Dir) -> Self {
        self.hidden = Some(face);
        self
    }

    const fn only(mut self, face: Dir) -> Self {
        self.only = Some(face);
        self
    }

    const fn mirrored(mut self) -> Self {
        self.mirror = true;
        self
    }

    const fn posed(mut self, offset: Vec3, rotation: Vec3) -> Self {
        self.pose = Pose {
            offset,
            rotation,
            scale: 1.0,
        };
        self
    }

    const fn in_part(mut self, parent: Pose) -> Self {
        self.parent = parent;
        self
    }

    /// The part's pose in block units, inside its parent's.
    fn pose(&self) -> Mat4 {
        self.parent.matrix().mul(&self.pose.matrix())
    }

    fn shows(&self, face: Dir) -> bool {
        self.hidden != Some(face) && self.only.is_none_or(|only| only == face)
    }
}

/// The quads of a cuboid with the game's `ModelPart.Cube` corner order and texture layout;
/// `uv_scale` maps the model's texture pixels to the loaded texture's.
fn cuboid_quads(
    c: &Cuboid,
    texture: usize,
    place: &Mat4,
    tint: Option<u32>,
    uv_scale: [f32; 2],
) -> Vec<Quad> {
    let g = c.inflate;
    let [x0, y0, z0] = [c.origin[0] - g, c.origin[1] - g, c.origin[2] - g];
    let [w, h, d] = c.size;
    let (x1, y1, z1) = (
        c.origin[0] + w + g,
        c.origin[1] + h + g,
        c.origin[2] + d + g,
    );
    // A mirrored cube swaps its sides (`ModelPart.Cube`); its faces then run the other way round.
    let (x0, x1) = if c.mirror { (x1, x0) } else { (x0, x1) };
    let v7 = [x0, y0, z0];
    let v0 = [x1, y0, z0];
    let v1 = [x1, y1, z0];
    let v2 = [x0, y1, z0];
    let v3 = [x0, y0, z1];
    let v4 = [x1, y0, z1];
    let v5 = [x1, y1, z1];
    let v6 = [x0, y1, z1];
    let [u, v] = c.tex;
    let (u0, u1, u2, u3, u4, u5) = (
        u,
        u + d,
        u + d + w,
        u + d + w + w,
        u + d + w + d,
        u + d + w + d + w,
    );
    let (va, vb, vc) = (v, v + d, v + d + h);
    // (direction, corners, u1, v1, u2, v2) per the game's polygon list.
    let faces: [(Dir, [Vec3; 4], f32, f32, f32, f32); 6] = [
        (Dir::Down, [v4, v3, v7, v0], u1, va, u2, vb),
        (Dir::Up, [v1, v2, v6, v5], u2, vb, u3, va),
        (Dir::West, [v7, v3, v6, v2], u0, vb, u1, vc),
        (Dir::North, [v0, v7, v2, v1], u1, vb, u2, vc),
        (Dir::East, [v4, v0, v1, v5], u2, vb, u4, vc),
        (Dir::South, [v3, v4, v5, v6], u4, vb, u5, vc),
    ];
    let transform = place.mul(&c.pose());
    let [su, sv] = uv_scale;
    faces
        .into_iter()
        .filter(|(dir, ..)| c.shows(*dir))
        .map(|(_, mut corners, ua, vaa, ub, vbb)| {
            let mut uvs = [[ub, vaa], [ua, vaa], [ua, vbb], [ub, vbb]];
            // `ModelPart.Polygon`: a mirrored face keeps each corner's texture point and reverses
            // the corner order, so it still faces out.
            if c.mirror {
                corners.reverse();
                uvs.reverse();
            }
            Quad {
                corners: corners.map(|p| transform.apply([p[0] / 16.0, p[1] / 16.0, p[2] / 16.0])),
                uvs: uvs.map(|[a, b]| [a * su, b * sv]),
                texture,
                tint,
            }
        })
        .collect()
}

/// Cuboids drawn with one entity texture, optionally tinted.
pub struct Piece {
    pub texture: String,
    /// The texture size the model is laid out for (the game's `LayerDefinition`).
    texture_size: [f32; 2],
    cuboids: Vec<Cuboid>,
    tint: Option<u32>,
    /// The model's root part pose, around every part (the statue turns it upside down).
    root: Pose,
}

/// A special model: its pieces, before the item definition's transformation.
pub struct Special {
    pub pieces: Vec<Piece>,
}

impl Special {
    /// Adds a piece's quads (`texture` is its image's index in the scene).
    pub fn add(&self, piece: usize, scene: &mut Scene, texture: usize, transform: &Mat4) {
        let piece = &self.pieces[piece];
        let image = &scene.textures[texture];
        let uv_scale = [
            image.width as f32 / piece.texture_size[0],
            image.height as f32 / piece.texture_size[1],
        ];
        let place = transform.mul(&piece.root.matrix());
        for c in &piece.cuboids {
            scene
                .quads
                .extend(cuboid_quads(c, texture, &place, piece.tint, uv_scale));
        }
    }
}

fn piece(texture: impl Into<String>, size: [f32; 2], cuboids: Vec<Cuboid>) -> Piece {
    Piece {
        texture: texture.into(),
        texture_size: size,
        cuboids,
        tint: None,
        root: REST,
    }
}

fn texture_arg(args: &Value, folder: &str, fallback: &str) -> String {
    let name = args
        .get("texture")
        .and_then(Value::as_str)
        .unwrap_or(fallback)
        .trim_start_matches("minecraft:");
    format!("minecraft:entity/{folder}/{name}")
}

/// A `texture` given as a whole path (`minecraft:textures/entity/copper_golem/copper_golem.png`),
/// as a texture id (`minecraft:entity/copper_golem/copper_golem`).
fn texture_path_arg(args: &Value, fallback: &str) -> String {
    let path = args
        .get("texture")
        .and_then(Value::as_str)
        .unwrap_or(fallback)
        .trim_start_matches("minecraft:");
    let path = path.strip_prefix("textures/").unwrap_or(path);
    let path = path.strip_suffix(".png").unwrap_or(path);
    format!("minecraft:{path}")
}

/// The game's dye colours (`DyeColor` texture colours) for banners.
fn dye(name: &str) -> u32 {
    match name {
        "orange" => 0xF9801D,
        "magenta" => 0xC74EBD,
        "light_blue" => 0x3AB3DA,
        "yellow" => 0xFED83D,
        "lime" => 0x80C71F,
        "pink" => 0xF38BAA,
        "gray" => 0x474F52,
        "light_gray" => 0x9D9D97,
        "cyan" => 0x169C9C,
        "purple" => 0x8932B8,
        "blue" => 0x3C44AA,
        "brown" => 0x835432,
        "green" => 0x5E7C16,
        "red" => 0xB02E26,
        "black" => 0x1D1D21,
        _ => 0xF9FFFE,
    }
}

/// A mob or player head (`SkullModel`): the 8-pixel head, with the skin's hat layer for heads
/// drawn from a 64 x 64 skin.
fn skull(texture: &str, hat: bool) -> Piece {
    let mut cuboids = vec![cuboid([0.0, 0.0], [-4.0, -8.0, -4.0], [8.0, 8.0, 8.0])];
    let mut size = [64.0, 32.0];
    if hat {
        cuboids.push(cuboid([32.0, 0.0], [-4.0, -8.0, -4.0], [8.0, 8.0, 8.0]).inflated(0.25));
        size = [64.0, 64.0];
    }
    piece(texture, size, cuboids)
}

/// The ears' angle at rest (`PiglinHeadModel.setupAnim` at animation 0: ±(cos 0 + 2.5) · 0.2 rad).
const PIGLIN_EAR_DEGREES: f32 = 40.107_04;

/// The jaw's opening at rest (`DragonHeadModel.setupAnim` at animation 0: (sin 0 + 1) · 0.2 rad).
const DRAGON_JAW_DEGREES: f32 = 11.459_156;

/// The dragon head (`DragonHeadModel.createHeadLayer`): the head part, three quarters the size,
/// with its scales and nostrils (the left pair mirrored) and the jaw, a little open.
fn dragon_head() -> Piece {
    let head = Pose {
        offset: [0.0, -7.986_666, 0.0],
        rotation: [0.0; 3],
        scale: 0.75,
    };
    let in_head = |c: Cuboid| c.in_part(head);
    piece(
        "minecraft:entity/enderdragon/dragon",
        [256.0, 256.0],
        vec![
            in_head(cuboid(
                [176.0, 44.0],
                [-6.0, -1.0, -24.0],
                [12.0, 5.0, 16.0],
            )),
            in_head(cuboid(
                [112.0, 30.0],
                [-8.0, -8.0, -10.0],
                [16.0, 16.0, 16.0],
            )),
            in_head(cuboid([0.0, 0.0], [-5.0, -12.0, -4.0], [2.0, 4.0, 6.0]).mirrored()),
            in_head(cuboid([112.0, 0.0], [-5.0, -3.0, -22.0], [2.0, 2.0, 4.0]).mirrored()),
            in_head(cuboid([0.0, 0.0], [3.0, -12.0, -4.0], [2.0, 4.0, 6.0])),
            in_head(cuboid([112.0, 0.0], [3.0, -3.0, -22.0], [2.0, 2.0, 4.0])),
            in_head(
                cuboid([176.0, 65.0], [-6.0, 0.0, -16.0], [12.0, 4.0, 16.0])
                    .posed([0.0, 4.0, -8.0], [DRAGON_JAW_DEGREES, 0.0, 0.0]),
            ),
        ],
    )
}

/// The decorated pot without sherds (`DecoratedPotRenderer`'s base and side layers, as the item
/// draws them): the neck turned upside down, the top and bottom, and four plain sides.
fn decorated_pot() -> Vec<Piece> {
    let flat = || cuboid([-14.0, 13.0], [0.0; 3], [14.0, 0.0, 14.0]);
    let side = |offset: Vec3, rotation: Vec3| {
        cuboid([1.0, 0.0], [0.0; 3], [14.0, 16.0, 0.0])
            .only(Dir::North)
            .posed(offset, rotation)
    };
    vec![
        piece(
            "minecraft:entity/decorated_pot/decorated_pot_base",
            [32.0, 32.0],
            vec![
                cuboid([0.0, 0.0], [4.0, 17.0, 4.0], [8.0, 3.0, 8.0])
                    .inflated(-0.1)
                    .posed([0.0, 37.0, 16.0], [180.0, 0.0, 0.0]),
                cuboid([0.0, 5.0], [5.0, 20.0, 5.0], [6.0, 1.0, 6.0])
                    .inflated(0.2)
                    .posed([0.0, 37.0, 16.0], [180.0, 0.0, 0.0]),
                flat().posed([1.0, 16.0, 1.0], [0.0; 3]),
                flat().posed([1.0, 0.0, 1.0], [0.0; 3]),
            ],
        ),
        piece(
            "minecraft:entity/decorated_pot/decorated_pot_side",
            [16.0, 16.0],
            vec![
                side([15.0, 16.0, 1.0], [0.0, 0.0, 180.0]),
                side([1.0, 16.0, 1.0], [0.0, -90.0, 180.0]),
                side([15.0, 16.0, 15.0], [0.0, 90.0, 180.0]),
                side([1.0, 16.0, 15.0], [180.0, 0.0, 0.0]),
            ],
        ),
    ]
}

/// The copper golem statue (`CopperGolemModel.createBodyLayer`, the standing pose an item without a
/// block state shows): the body with the head (nose, antenna rod and bulb) and both arms in it,
/// and the legs. The statue turns the model's root upside down (`CopperGolemStatueModel.setupAnim`:
/// `y` 0, `zRot` π); the item's display turns it back.
fn copper_golem_statue(texture: String) -> Piece {
    let body = Pose {
        offset: [0.0, -5.0, 0.0],
        rotation: [0.0; 3],
        scale: 1.0,
    };
    let head = |c: Cuboid| c.posed([0.0, -6.0, 0.0], [0.0; 3]).in_part(body);
    let arm = |c: Cuboid, x: f32| c.posed([x, -6.0, 0.0], [0.0; 3]).in_part(body);
    let leg = |c: Cuboid| c.posed([0.0, -5.0, 0.0], [0.0; 3]);
    let mut statue = piece(
        texture,
        [64.0, 64.0],
        vec![
            cuboid([0.0, 15.0], [-4.0, -6.0, -3.0], [8.0, 6.0, 6.0]).posed(body.offset, [0.0; 3]),
            head(cuboid([0.0, 0.0], [-4.0, -5.0, -5.0], [8.0, 5.0, 10.0]).inflated(0.015)),
            head(cuboid([56.0, 0.0], [-1.0, -2.0, -6.0], [2.0, 3.0, 2.0])),
            head(cuboid([37.0, 8.0], [-1.0, -9.0, -1.0], [2.0, 4.0, 2.0]).inflated(-0.015)),
            head(cuboid([37.0, 0.0], [-2.0, -13.0, -2.0], [4.0, 4.0, 4.0]).inflated(-0.015)),
            arm(
                cuboid([36.0, 16.0], [-3.0, -1.0, -2.0], [3.0, 10.0, 4.0]),
                -4.0,
            ),
            arm(
                cuboid([50.0, 16.0], [0.0, -1.0, -2.0], [3.0, 10.0, 4.0]),
                4.0,
            ),
            leg(cuboid([0.0, 27.0], [-4.0, 0.0, -2.0], [4.0, 5.0, 4.0])),
            leg(cuboid([16.0, 27.0], [0.0, 0.0, -2.0], [4.0, 5.0, 4.0])),
        ],
    );
    statue.root = Pose {
        offset: [0.0; 3],
        rotation: [0.0, 0.0, 180.0],
        scale: 1.0,
    };
    statue
}

/// The special model for a definition's `model` object, or None when it is not drawn here (the
/// client then shows a stand-in).
pub fn special(kind: &str, args: &Value) -> Option<Special> {
    let pieces = match kind {
        "chest" => vec![piece(
            texture_arg(args, "chest", "normal"),
            [64.0, 64.0],
            vec![
                cuboid([0.0, 19.0], [1.0, 0.0, 1.0], [14.0, 10.0, 14.0]),
                cuboid([0.0, 0.0], [1.0, 9.0, 1.0], [14.0, 5.0, 14.0]),
                cuboid([0.0, 0.0], [7.0, 7.0, 15.0], [2.0, 4.0, 1.0]),
            ],
        )],
        "shulker_box" => vec![piece(
            texture_arg(args, "shulker", "shulker"),
            [64.0, 64.0],
            vec![
                cuboid([0.0, 28.0], [-8.0, 16.0, -8.0], [16.0, 8.0, 16.0]),
                cuboid([0.0, 0.0], [-8.0, 8.0, -8.0], [16.0, 12.0, 16.0]),
            ],
        )],
        "head" | "player_head" => {
            let kind = match kind {
                "player_head" => "player",
                _ => args.get("kind").and_then(Value::as_str).unwrap_or("player"),
            };
            vec![match kind {
                "skeleton" => skull("minecraft:entity/skeleton/skeleton", false),
                "wither_skeleton" => skull("minecraft:entity/skeleton/wither_skeleton", false),
                "creeper" => skull("minecraft:entity/creeper/creeper", false),
                "zombie" => skull("minecraft:entity/zombie/zombie", true),
                "player" => skull("minecraft:entity/player/wide/steve", true),
                "piglin" => piece(
                    "minecraft:entity/piglin/piglin",
                    [64.0, 64.0],
                    vec![
                        cuboid([0.0, 0.0], [-5.0, -8.0, -4.0], [10.0, 8.0, 8.0]),
                        cuboid([31.0, 1.0], [-2.0, -4.0, -5.0], [4.0, 4.0, 1.0]),
                        cuboid([2.0, 4.0], [2.0, -2.0, -5.0], [1.0, 2.0, 1.0]),
                        cuboid([2.0, 0.0], [-3.0, -2.0, -5.0], [1.0, 2.0, 1.0]),
                        cuboid([51.0, 6.0], [0.0, 0.0, -2.0], [1.0, 5.0, 4.0])
                            .posed([4.5, -6.0, 0.0], [0.0, 0.0, -PIGLIN_EAR_DEGREES]),
                        cuboid([39.0, 6.0], [-1.0, 0.0, -2.0], [1.0, 5.0, 4.0])
                            .posed([-4.5, -6.0, 0.0], [0.0, 0.0, PIGLIN_EAR_DEGREES]),
                    ],
                ),
                "dragon" => dragon_head(),
                _ => return None,
            }]
        }
        "decorated_pot" => decorated_pot(),
        // `BedRenderer.createHeadLayer` / `createFootLayer`: the mattress half and its two legs;
        // the halves leave out the faces where they meet.
        "bed" => {
            let texture = texture_arg(args, "bed", "white");
            let head = args.get("part").and_then(Value::as_str) != Some("foot");
            let cuboids = if head {
                vec![
                    cuboid([0.0, 0.0], [0.0, 0.0, 0.0], [16.0, 16.0, 6.0]).without(Dir::Up),
                    cuboid([50.0, 6.0], [0.0, 6.0, 0.0], [3.0, 3.0, 3.0])
                        .without(Dir::Down)
                        .posed([0.0; 3], [90.0, 0.0, 90.0]),
                    cuboid([50.0, 18.0], [-16.0, 6.0, 0.0], [3.0, 3.0, 3.0])
                        .without(Dir::Down)
                        .posed([0.0; 3], [90.0, 0.0, 180.0]),
                ]
            } else {
                vec![
                    cuboid([0.0, 22.0], [0.0, 0.0, 0.0], [16.0, 16.0, 6.0]).without(Dir::Down),
                    cuboid([50.0, 0.0], [0.0, 6.0, -16.0], [3.0, 3.0, 3.0])
                        .without(Dir::Down)
                        .posed([0.0; 3], [90.0, 0.0, 0.0]),
                    cuboid([50.0, 12.0], [-16.0, 6.0, -16.0], [3.0, 3.0, 3.0])
                        .without(Dir::Down)
                        .posed([0.0; 3], [90.0, 0.0, 270.0]),
                ]
            };
            vec![piece(texture, [64.0, 64.0], cuboids)]
        }
        "shield" => vec![piece(
            "minecraft:entity/shield/shield_base_nopattern",
            [64.0, 64.0],
            vec![
                cuboid([0.0, 0.0], [-6.0, -11.0, -2.0], [12.0, 22.0, 1.0]),
                cuboid([26.0, 0.0], [-1.0, -3.0, -1.0], [2.0, 6.0, 6.0]),
            ],
        )],
        "banner" => {
            let color = dye(args
                .get("color")
                .and_then(Value::as_str)
                .unwrap_or("white")
                .trim_start_matches("minecraft:"));
            vec![
                piece(
                    "minecraft:entity/banner/banner_base",
                    [64.0, 64.0],
                    vec![
                        cuboid([44.0, 0.0], [-1.0, -42.0, -1.0], [2.0, 42.0, 2.0]),
                        cuboid([0.0, 42.0], [-10.0, -44.0, -1.0], [20.0, 2.0, 2.0]),
                    ],
                ),
                Piece {
                    texture: "minecraft:entity/banner/base".into(),
                    texture_size: [64.0, 64.0],
                    cuboids: vec![cuboid([0.0, 0.0], [-10.0, -44.0, -2.0], [20.0, 40.0, 1.0])],
                    tint: Some(color),
                    root: REST,
                },
            ]
        }
        "conduit" => vec![piece(
            "minecraft:entity/conduit/base",
            [32.0, 16.0],
            vec![cuboid([0.0, 0.0], [-3.0, -3.0, -3.0], [6.0, 6.0, 6.0])],
        )],
        "copper_golem_statue" => vec![copper_golem_statue(texture_path_arg(
            args,
            "entity/copper_golem/copper_golem",
        ))],
        _ => return None,
    };
    Some(Special { pieces })
}

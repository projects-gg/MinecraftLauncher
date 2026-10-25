//! Drawing an inventory icon the way the game does: a flat item is its layers; anything with
//! cuboid elements (blocks, stairs, fences, chests...) is projected with its `gui` display
//! transform, lit by the game's two slot lights, depth-tested and sampled nearest-neighbour.

use crate::image::{Image, shade, tint};
use crate::math::{Mat4, Vec3, cross, sub};
use crate::model::{Dir, Display, Element, Model};

/// One textured quad in block units (a full block spans 0..1).
#[derive(Clone, Debug)]
pub struct Quad {
    /// The corners in the game's order for the face (outward normal by the right-hand rule).
    pub corners: [Vec3; 4],
    /// Texture coordinates of the corners, in texture pixels.
    pub uvs: [[f32; 2]; 4],
    pub texture: usize,
    pub tint: Option<u32>,
}

fn normalize(v: Vec3) -> Vec3 {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len == 0.0 {
        return v;
    }
    [v[0] / len, v[1] / len, v[2] / len]
}

fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// The game's two light directions for an item in a slot, where they meet the faces' normals:
/// view space, x right, y up, z towards the viewer. From the game (26.1, `Lighting`): the
/// diffuse lights `(0.2, 1, -0.7)` and `(-0.2, 1, 0.7)`, the `ITEMS_3D` matrix
/// `scaling(1, -1, 1).rotateYXZ(1.0821041, 3.2375858, 0).rotateYXZ(-π/8, 3π/4, 0)` for side-lit
/// models and `ITEMS_FLAT` `rotationY(-π/8).rotateX(3π/4)` for `gui_light: front`, and the slot
/// pose `scale(s, -s, s)` (`GuiItemAtlas`), which flips the normals' y as well. A cube in the
/// usual view gets its top at full light, its left side 0.65 and its right side 0.4.
fn gui_lights(front: bool) -> [Vec3; 2] {
    let flat = Mat4::rotation(1, -22.5).mul(&Mat4::rotation(0, 135.0));
    let side = Mat4::rotation(1, 1.082_104_1_f32.to_degrees())
        .mul(&Mat4::rotation(0, 3.237_585_8_f32.to_degrees()))
        .mul(&flat);
    [normalize([0.2, 1.0, -0.7]), normalize([-0.2, 1.0, 0.7])].map(|light| {
        if front {
            let v = flat.apply(light);
            [v[0], -v[1], v[2]]
        } else {
            side.apply(light)
        }
    })
}

/// The game's light for a face: 0.4 ambient plus 0.6 of each light it faces, at most 1.
fn gui_light(lights: &[Vec3; 2], normal: Vec3) -> f32 {
    let facing = dot(lights[0], normal).max(0.0) + dot(lights[1], normal).max(0.0);
    (0.4 + 0.6 * facing).min(1.0)
}

/// Quads and the textures they sample.
#[derive(Default)]
pub struct Scene {
    pub quads: Vec<Quad>,
    pub textures: Vec<Image>,
}

impl Scene {
    pub fn add_texture(&mut self, image: Image) -> usize {
        self.textures.push(image);
        self.textures.len() - 1
    }
}

/// The face corners of a cuboid from `a` to `b` (block pixels), in the game's per-face order.
pub fn face_corners(dir: Dir, a: Vec3, b: Vec3) -> [Vec3; 4] {
    let ([x0, y0, z0], [x1, y1, z1]) = (a, b);
    match dir {
        Dir::Down => [[x0, y0, z1], [x0, y0, z0], [x1, y0, z0], [x1, y0, z1]],
        Dir::Up => [[x0, y1, z0], [x0, y1, z1], [x1, y1, z1], [x1, y1, z0]],
        Dir::North => [[x1, y1, z0], [x1, y0, z0], [x0, y0, z0], [x0, y1, z0]],
        Dir::South => [[x0, y1, z1], [x0, y0, z1], [x1, y0, z1], [x1, y1, z1]],
        Dir::West => [[x0, y1, z0], [x0, y0, z0], [x0, y0, z1], [x0, y1, z1]],
        Dir::East => [[x1, y1, z1], [x1, y0, z1], [x1, y0, z0], [x1, y1, z0]],
    }
}

/// The game's default face UV from the element's bounds, in sixteenths.
fn default_uv(dir: Dir, from: Vec3, to: Vec3) -> [f32; 4] {
    match dir {
        Dir::Down => [from[0], 16.0 - to[2], to[0], 16.0 - from[2]],
        Dir::Up => [from[0], from[2], to[0], to[2]],
        Dir::North => [16.0 - to[0], 16.0 - to[1], 16.0 - from[0], 16.0 - from[1]],
        Dir::South => [from[0], 16.0 - to[1], to[0], 16.0 - from[1]],
        Dir::West => [from[2], 16.0 - to[1], to[2], 16.0 - from[1]],
        Dir::East => [16.0 - to[2], 16.0 - to[1], 16.0 - from[2], 16.0 - from[1]],
    }
}

/// UV of corner `i` for a face UV and rotation (the game's `BlockFaceUV` shifting).
fn corner_uv(uv: [f32; 4], rotation: i32, i: usize) -> [f32; 2] {
    let shifted = (i + (rotation.rem_euclid(360) / 90) as usize) % 4;
    let u = if shifted == 0 || shifted == 1 {
        uv[0]
    } else {
        uv[2]
    };
    let v = if shifted == 0 || shifted == 3 {
        uv[1]
    } else {
        uv[3]
    };
    [u, v]
}

fn rotate_element(p: Vec3, element: &Element) -> Vec3 {
    let Some(rot) = &element.rotation else {
        return p;
    };
    let o = rot.origin;
    let mut q = Mat4::rotation(rot.axis, rot.angle).apply(sub(p, o));
    if rot.rescale && rot.angle.abs() > 0.0 {
        let k = 1.0 / rot.angle.to_radians().cos();
        for (axis, value) in q.iter_mut().enumerate() {
            if axis != rot.axis {
                *value *= k;
            }
        }
    }
    [q[0] + o[0], q[1] + o[1], q[2] + o[2]]
}

/// Adds a model's elements to the scene. `texture` loads a texture by id (None if missing);
/// `face_tint` gives a tint slot's colour. `transform` places the model (block units).
pub fn add_elements(
    scene: &mut Scene,
    model: &Model,
    transform: &Mat4,
    texture: &mut dyn FnMut(&str) -> Option<Image>,
    face_tint: &dyn Fn(i32) -> Option<u32>,
) -> usize {
    let mut added = 0;
    let Some(elements) = &model.elements else {
        return 0;
    };
    let mut indices: std::collections::HashMap<String, Option<usize>> =
        std::collections::HashMap::new();
    for element in elements {
        for face in &element.faces {
            let Some(texture_id) = model.texture(&face.texture) else {
                continue;
            };
            let index = match indices.get(&texture_id) {
                Some(hit) => *hit,
                None => {
                    let index = texture(&texture_id).map(|image| scene.add_texture(image));
                    indices.insert(texture_id.clone(), index);
                    index
                }
            };
            let Some(index) = index else {
                continue;
            };
            let (tw, th) = {
                let image = &scene.textures[index];
                (image.width as f32, image.height as f32)
            };
            let uv = face
                .uv
                .unwrap_or_else(|| default_uv(face.dir, element.from, element.to));
            let corners = face_corners(face.dir, element.from, element.to).map(|p| {
                let p = rotate_element(p, element);
                transform.apply([p[0] / 16.0, p[1] / 16.0, p[2] / 16.0])
            });
            let uvs = [0, 1, 2, 3].map(|i| {
                let [u, v] = corner_uv(uv, face.rotation, i);
                [u * tw / 16.0, v * th / 16.0]
            });
            scene.quads.push(Quad {
                corners,
                uvs,
                texture: index,
                tint: if face.tintindex >= 0 {
                    face_tint(face.tintindex)
                } else {
                    None
                },
            });
            added += 1;
        }
    }
    added
}

/// The inventory projection: centre the model, apply the display transform, flip to screen.
fn view_matrix(display: &Display) -> Mat4 {
    let t = display.translation;
    Mat4::translation([t[0] / 16.0, t[1] / 16.0, t[2] / 16.0])
        .mul(&Mat4::euler_xyz(display.rotation))
        .mul(&Mat4::scale(display.scale))
        .mul(&Mat4::translation([-0.5, -0.5, -0.5]))
}

fn edge(a: [f32; 2], b: [f32; 2], p: [f32; 2]) -> f32 {
    (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
}

/// Renders a scene into a `size` x `size` icon; `front` lights it as `gui_light: front` models.
pub fn rasterize(scene: &Scene, display: &Display, size: u32, front: bool) -> Image {
    let view = view_matrix(display);
    let lights = gui_lights(front);
    let n = size as f32;
    let mut image = Image::new(size, size);
    let mut depth = vec![f32::NEG_INFINITY; (size * size) as usize];
    // Opaque quads first, then translucent ones over them.
    let translucent: Vec<bool> = scene.textures.iter().map(Image::has_translucency).collect();
    let mut order: Vec<usize> = (0..scene.quads.len()).collect();
    order.sort_by_key(|&i| translucent[scene.quads[i].texture]);
    for &qi in &order {
        let quad = &scene.quads[qi];
        let world = quad.corners.map(|p| view.apply(p));
        let normal = cross(sub(world[1], world[0]), sub(world[2], world[0]));
        if normal[2] <= 1e-6 {
            continue; // facing away
        }
        let light = gui_light(&lights, normalize(normal));
        let screen = world.map(|p| [(p[0] + 0.5) * n, (0.5 - p[1]) * n]);
        let texture = &scene.textures[quad.texture];
        for tri in [[0usize, 1, 2], [0, 2, 3]] {
            let (a, b, c) = (screen[tri[0]], screen[tri[1]], screen[tri[2]]);
            let area = edge(a, b, c);
            if area.abs() < 1e-6 {
                continue;
            }
            let min_x = a[0].min(b[0]).min(c[0]).floor().max(0.0) as u32;
            let max_x = a[0].max(b[0]).max(c[0]).ceil().min(n) as u32;
            let min_y = a[1].min(b[1]).min(c[1]).floor().max(0.0) as u32;
            let max_y = a[1].max(b[1]).max(c[1]).ceil().min(n) as u32;
            for y in min_y..max_y {
                for x in min_x..max_x {
                    let p = [x as f32 + 0.5, y as f32 + 0.5];
                    let w0 = edge(b, c, p) / area;
                    let w1 = edge(c, a, p) / area;
                    let w2 = edge(a, b, p) / area;
                    if w0 < -1e-5 || w1 < -1e-5 || w2 < -1e-5 {
                        continue;
                    }
                    let z = w0 * world[tri[0]][2] + w1 * world[tri[1]][2] + w2 * world[tri[2]][2];
                    let slot = (y * size + x) as usize;
                    if z < depth[slot] - 1e-5 {
                        continue;
                    }
                    let uv0 = quad.uvs[tri[0]];
                    let uv1 = quad.uvs[tri[1]];
                    let uv2 = quad.uvs[tri[2]];
                    let u = w0 * uv0[0] + w1 * uv1[0] + w2 * uv2[0];
                    let v = w0 * uv0[1] + w1 * uv1[1] + w2 * uv2[1];
                    let tx = (u.floor().max(0.0) as u32).min(texture.width - 1);
                    let ty = (v.floor().max(0.0) as u32).min(texture.height - 1);
                    let mut px = texture.get(tx, ty);
                    if px[3] == 0 {
                        continue;
                    }
                    if let Some(rgb) = quad.tint {
                        px = tint(px, rgb);
                    }
                    px = shade(px, light);
                    if px[3] == 255 {
                        image.put(x, y, px);
                        depth[slot] = z;
                    } else {
                        let under = image.get(x, y);
                        image.put(x, y, crate::image::over(under, px));
                    }
                }
            }
        }
    }
    image
}

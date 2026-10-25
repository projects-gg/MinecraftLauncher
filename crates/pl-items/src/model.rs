//! Block and item models (`assets/<ns>/models/**.json`): parents resolved, textures named, cuboid
//! elements and the inventory (`gui`) display transform kept.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::source::{AssetSource, full_id, model_path};

/// The inventory transform from a model's `display.gui`: rotation in degrees (X, then Y, then Z),
/// translation in sixteenths of a block, scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Display {
    pub rotation: [f32; 3],
    pub translation: [f32; 3],
    pub scale: [f32; 3],
}

impl Default for Display {
    fn default() -> Self {
        Self {
            rotation: [0.0; 3],
            translation: [0.0; 3],
            scale: [1.0; 3],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Down,
    Up,
    North,
    South,
    West,
    East,
}

impl Dir {
    pub const ALL: [Dir; 6] = [
        Dir::Down,
        Dir::Up,
        Dir::North,
        Dir::South,
        Dir::West,
        Dir::East,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Dir::Down => "down",
            Dir::Up => "up",
            Dir::North => "north",
            Dir::South => "south",
            Dir::West => "west",
            Dir::East => "east",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Face {
    pub dir: Dir,
    /// A texture reference (`#side`) or id.
    pub texture: String,
    /// `[u1, v1, u2, v2]` in sixteenths of the texture; None means "from the element's bounds".
    pub uv: Option<[f32; 4]>,
    /// 0, 90, 180 or 270.
    pub rotation: i32,
    /// The tint slot, -1 for none.
    pub tintindex: i32,
}

#[derive(Clone, Debug)]
pub struct ElementRotation {
    pub origin: [f32; 3],
    /// 0 x, 1 y, 2 z.
    pub axis: usize,
    pub angle: f32,
    pub rescale: bool,
}

/// A cuboid element. Its `shade` flag is not kept: the game lights items in a slot by their face
/// normals alone (`ItemFeatureRenderer`), shaded or not.
#[derive(Clone, Debug)]
pub struct Element {
    pub from: [f32; 3],
    pub to: [f32; 3],
    pub rotation: Option<ElementRotation>,
    pub faces: Vec<Face>,
}

#[derive(Clone, Debug, Default)]
pub struct Model {
    pub textures: BTreeMap<String, String>,
    pub elements: Option<Vec<Element>>,
    pub gui: Option<Display>,
    /// `gui_light: "front"` (flat lighting, as for 2D items).
    pub front_light: bool,
    /// The parent chain ends in `builtin/generated`: a flat item drawn from `layer0`, `layer1`...
    pub generated: bool,
}

fn vec3(v: &Value) -> Option<[f32; 3]> {
    let a = v.as_array()?;
    Some([
        a.first()?.as_f64()? as f32,
        a.get(1)?.as_f64()? as f32,
        a.get(2)?.as_f64()? as f32,
    ])
}

fn parse_display(v: &Value) -> Option<Display> {
    let mut display = Display::default();
    if let Some(r) = v.get("rotation").and_then(vec3) {
        display.rotation = r;
    }
    if let Some(t) = v.get("translation").and_then(vec3) {
        display.translation = t;
    }
    if let Some(s) = v.get("scale").and_then(vec3) {
        display.scale = s;
    }
    Some(display)
}

fn parse_face(dir: Dir, v: &Value) -> Option<Face> {
    let uv = v.get("uv").and_then(Value::as_array).and_then(|a| {
        Some([
            a.first()?.as_f64()? as f32,
            a.get(1)?.as_f64()? as f32,
            a.get(2)?.as_f64()? as f32,
            a.get(3)?.as_f64()? as f32,
        ])
    });
    Some(Face {
        dir,
        texture: v.get("texture")?.as_str()?.to_owned(),
        uv,
        rotation: v.get("rotation").and_then(Value::as_i64).unwrap_or(0) as i32,
        tintindex: v.get("tintindex").and_then(Value::as_i64).unwrap_or(-1) as i32,
    })
}

fn parse_element(v: &Value) -> Option<Element> {
    let faces_json = v.get("faces")?.as_object()?;
    let faces = Dir::ALL
        .iter()
        .filter_map(|&dir| faces_json.get(dir.name()).and_then(|f| parse_face(dir, f)))
        .collect();
    let rotation = v.get("rotation").and_then(|r| {
        let axis = match r.get("axis")?.as_str()? {
            "x" => 0,
            "y" => 1,
            _ => 2,
        };
        Some(ElementRotation {
            origin: r.get("origin").and_then(vec3).unwrap_or([8.0; 3]),
            axis,
            angle: r.get("angle").and_then(Value::as_f64).unwrap_or(0.0) as f32,
            rescale: r.get("rescale").and_then(Value::as_bool).unwrap_or(false),
        })
    });
    Some(Element {
        from: vec3(v.get("from")?)?,
        to: vec3(v.get("to")?)?,
        rotation,
        faces,
    })
}

/// Loads a model and its parents (child values win). None when the model or a parent is missing.
pub fn load(source: &mut dyn AssetSource, id: &str) -> Option<Model> {
    let mut chain: Vec<Value> = Vec::new();
    let mut generated = false;
    let mut current = Some(full_id(id));
    for _ in 0..24 {
        let Some(id) = current.take() else { break };
        match id.as_str() {
            "minecraft:builtin/generated" => {
                generated = true;
                break;
            }
            "minecraft:builtin/entity" => break,
            _ => {}
        }
        let json: Value = serde_json::from_slice(&source.read(&model_path(&id))?).ok()?;
        current = json.get("parent").and_then(Value::as_str).map(full_id);
        chain.push(json);
    }
    let mut model = Model {
        generated,
        ..Model::default()
    };
    // Textures: the root first, every child overriding.
    for json in chain.iter().rev() {
        if let Some(map) = json.get("textures").and_then(Value::as_object) {
            for (name, value) in map {
                // A plain id or reference, or (since 26.1) an object with a `sprite`.
                let value = value
                    .as_str()
                    .or_else(|| value.get("sprite").and_then(Value::as_str));
                if let Some(value) = value {
                    model.textures.insert(name.clone(), value.to_owned());
                }
            }
        }
    }
    for json in &chain {
        if model.elements.is_none()
            && let Some(elements) = json.get("elements").and_then(Value::as_array)
        {
            model.elements = Some(elements.iter().filter_map(parse_element).collect());
        }
        if model.gui.is_none()
            && let Some(gui) = json.get("display").and_then(|d| d.get("gui"))
        {
            model.gui = parse_display(gui);
        }
    }
    model.front_light = chain
        .iter()
        .find_map(|json| json.get("gui_light").and_then(Value::as_str))
        .map(|light| light == "front")
        .unwrap_or(false);
    Some(model)
}

impl Model {
    /// A texture reference (`#side`, `minecraft:block/stone`) followed to a full texture id.
    pub fn texture(&self, reference: &str) -> Option<String> {
        // A face may name a texture variable without the `#`.
        let mut current = if !reference.starts_with('#') && self.textures.contains_key(reference) {
            format!("#{reference}")
        } else {
            reference.to_owned()
        };
        for _ in 0..16 {
            match current.strip_prefix('#') {
                Some(name) => current = self.textures.get(name)?.clone(),
                None => return Some(full_id(&current)),
            }
        }
        None
    }

    /// A flat item's layers in drawing order: (layer number, texture id).
    pub fn layers(&self) -> Vec<(i32, String)> {
        (0..16)
            .filter_map(|i| {
                let name = format!("layer{i}");
                if !self.textures.contains_key(&name) {
                    return None;
                }
                self.texture(&format!("#{name}"))
                    .map(|texture| (i, texture))
            })
            .collect()
    }
}

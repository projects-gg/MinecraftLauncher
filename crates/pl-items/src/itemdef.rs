//! Item definitions (`assets/<ns>/items/<id>.json`, the game since 1.21.4): which models draw an
//! item, picked for an item lying in an inventory (no use, no charge, no trim, no holiday).

use serde_json::Value;

use crate::math::Mat4;

/// A tint the definition gives a model's layers or faces, by tint slot. Colours are ARGB as the
/// game writes them; a transparent one hides the layer (undyed wolf armour's dye layer).
#[derive(Clone, Debug, PartialEq)]
pub enum Tint {
    Constant(u32),
    /// The player's dye (leather armour); the colour is the undyed default.
    Dye(u32),
    /// A potion's colour; the default is water's.
    Potion(u32),
    Firework(u32),
    MapColor(u32),
    Team(u32),
    CustomModelData(u32),
    /// The biome grass colour at a temperature and downfall (grass blocks, ferns).
    Grass {
        temperature: f32,
        downfall: f32,
    },
    /// Anything else: drawn untinted.
    None,
}

impl Tint {
    /// The kind of colour the server can change at run time, if any.
    pub fn runtime_source(&self) -> Option<&'static str> {
        match self {
            Tint::Dye(_) => Some("dye"),
            Tint::Potion(_) => Some("potion"),
            Tint::Firework(_) => Some("firework"),
            Tint::MapColor(_) => Some("map_color"),
            Tint::Team(_) => Some("team"),
            Tint::CustomModelData(_) => Some("custom_model_data"),
            _ => None,
        }
    }
}

/// An ARGB colour as the game reads it (the int as written, so `-1` is opaque white and `0`
/// transparent).
fn argb(v: Option<&Value>, fallback: u32) -> u32 {
    v.and_then(Value::as_i64)
        .map(|c| c as u32)
        .unwrap_or(fallback)
}

const OPAQUE: u32 = 0xFF00_0000;

fn parse_tint(v: &Value) -> Tint {
    let kind = v.get("type").and_then(Value::as_str).unwrap_or("");
    match kind.trim_start_matches("minecraft:") {
        "constant" => Tint::Constant(argb(v.get("value"), 0xFFFF_FFFF)),
        "dye" => Tint::Dye(argb(v.get("default"), 0xFFFF_FFFF)),
        "potion" => Tint::Potion(argb(v.get("default"), 0xFF38_5DC6)),
        "firework" => Tint::Firework(argb(v.get("default"), 0xFF8A_8A8A)),
        // The game makes a map's colour opaque, its default too.
        "map_color" => Tint::MapColor(argb(v.get("default"), 0x0046_402E) | OPAQUE),
        "team" => Tint::Team(argb(v.get("default"), 0xFFFF_FFFF)),
        "custom_model_data" => Tint::CustomModelData(argb(v.get("default"), 0xFFFF_FFFF)),
        "grass" => Tint::Grass {
            temperature: v.get("temperature").and_then(Value::as_f64).unwrap_or(0.5) as f32,
            downfall: v.get("downfall").and_then(Value::as_f64).unwrap_or(1.0) as f32,
        },
        _ => Tint::None,
    }
}

/// One drawing the definition asks for.
#[derive(Clone, Debug)]
pub enum Part {
    /// A model (`minecraft:item/diamond_sword`, `minecraft:block/stone`) with its tints.
    Model {
        id: String,
        tints: Vec<Tint>,
        transform: Option<Mat4>,
    },
    /// A model the game draws in code (chests, shulker boxes, heads, banners, shields...): `kind`
    /// is its type, `args` its settings; `base` names the model that carries its display transform.
    Special {
        kind: String,
        args: Value,
        base: String,
        transform: Option<Mat4>,
    },
}

fn vec(v: Option<&Value>, n: usize) -> Option<Vec<f32>> {
    let a = v?.as_array()?;
    (a.len() == n).then(|| a.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect())
}

/// A `transformation` (an object of translation, rotations and scale, or 16 matrix numbers) in
/// block units.
fn parse_transform(v: Option<&Value>) -> Option<Mat4> {
    let v = v?;
    if let Some(values) = v.as_array().filter(|a| a.len() == 16) {
        let f: Vec<f32> = values
            .iter()
            .map(|x| x.as_f64().unwrap_or(0.0) as f32)
            .collect();
        // Row-major as the game writes it.
        return Some(Mat4([
            [f[0], f[1], f[2], f[3]],
            [f[4], f[5], f[6], f[7]],
            [f[8], f[9], f[10], f[11]],
            [f[12], f[13], f[14], f[15]],
        ]));
    }
    let t = vec(v.get("translation"), 3).unwrap_or(vec![0.0; 3]);
    let s = vec(v.get("scale"), 3).unwrap_or(vec![1.0; 3]);
    let l = vec(v.get("left_rotation"), 4).unwrap_or(vec![0.0, 0.0, 0.0, 1.0]);
    let r = vec(v.get("right_rotation"), 4).unwrap_or(vec![0.0, 0.0, 0.0, 1.0]);
    Some(
        Mat4::translation([t[0], t[1], t[2]])
            .mul(&Mat4::quaternion([l[0], l[1], l[2], l[3]]))
            .mul(&Mat4::scale([s[0], s[1], s[2]]))
            .mul(&Mat4::quaternion([r[0], r[1], r[2], r[3]])),
    )
}

fn compose(outer: Option<Mat4>, inner: Option<Mat4>) -> Option<Mat4> {
    match (outer, inner) {
        (Some(a), Some(b)) => Some(a.mul(&b)),
        (a, None) => a,
        (None, b) => b,
    }
}

/// The parts of a definition file (`{"model": {...}}`), for an item resting in an inventory.
pub fn evaluate(definition: &Value) -> Vec<Part> {
    let mut out = Vec::new();
    if let Some(node) = definition.get("model") {
        walk(node, None, &mut out, 0);
    }
    out
}

fn walk(node: &Value, transform: Option<Mat4>, out: &mut Vec<Part>, depth: usize) {
    if depth > 32 {
        return;
    }
    let kind = node.get("type").and_then(Value::as_str).unwrap_or("");
    let own = compose(transform, parse_transform(node.get("transformation")));
    match kind.trim_start_matches("minecraft:") {
        "model" => {
            if let Some(id) = node.get("model").and_then(Value::as_str) {
                let tints = node
                    .get("tints")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().map(parse_tint).collect())
                    .unwrap_or_default();
                out.push(Part::Model {
                    id: id.to_owned(),
                    tints,
                    transform: own,
                });
            }
        }
        "composite" => {
            for child in node
                .get("models")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                walk(child, own, out, depth + 1);
            }
        }
        "condition" => {
            // At rest the condition is false (not in use, not broken, no lodestone), except the
            // dye check: a dyed item's model carries the dye layer, which stays hidden until the
            // server sends a colour (wolf armour).
            let property = node.get("property").and_then(Value::as_str).unwrap_or("");
            let component = node.get("component").and_then(Value::as_str).unwrap_or("");
            let dyed = property.trim_start_matches("minecraft:") == "has_component"
                && component.trim_start_matches("minecraft:") == "dyed_color";
            if let Some(child) = node.get(if dyed { "on_true" } else { "on_false" }) {
                walk(child, own, out, depth + 1);
            }
        }
        "select" => {
            let property = node.get("property").and_then(Value::as_str).unwrap_or("");
            let cases = node.get("cases").and_then(Value::as_array);
            let wanted = property.trim_start_matches("minecraft:") == "display_context";
            let hit = cases.and_then(|cases| {
                cases.iter().find(|case| {
                    wanted
                        && match case.get("when") {
                            Some(Value::String(s)) => s == "gui",
                            Some(Value::Array(a)) => a.iter().any(|w| w.as_str() == Some("gui")),
                            _ => false,
                        }
                })
            });
            let child = hit
                .and_then(|case| case.get("model"))
                .or_else(|| node.get("fallback"))
                .or_else(|| {
                    cases
                        .and_then(|c| c.first())
                        .and_then(|case| case.get("model"))
                });
            if let Some(child) = child {
                walk(child, own, out, depth + 1);
            }
        }
        "range_dispatch" => {
            // The property at rest is 0: the highest threshold not above it.
            let entries = node.get("entries").and_then(Value::as_array);
            let hit = entries.and_then(|entries| {
                entries
                    .iter()
                    .filter(|e| e.get("threshold").and_then(Value::as_f64).unwrap_or(0.0) <= 0.0)
                    .max_by(|a, b| {
                        let ta = a.get("threshold").and_then(Value::as_f64).unwrap_or(0.0);
                        let tb = b.get("threshold").and_then(Value::as_f64).unwrap_or(0.0);
                        ta.total_cmp(&tb)
                    })
            });
            let child = hit
                .and_then(|e| e.get("model"))
                .or_else(|| node.get("fallback"))
                .or_else(|| entries.and_then(|e| e.first()).and_then(|e| e.get("model")));
            if let Some(child) = child {
                walk(child, own, out, depth + 1);
            }
        }
        "special" => {
            let args = node.get("model").cloned().unwrap_or(Value::Null);
            let kind = args
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim_start_matches("minecraft:")
                .to_owned();
            if let Some(base) = node.get("base").and_then(Value::as_str) {
                out.push(Part::Special {
                    kind,
                    args,
                    base: base.to_owned(),
                    transform: own,
                });
            }
        }
        // "empty", "bundle/selected_item": nothing to draw at rest.
        _ => {}
    }
}

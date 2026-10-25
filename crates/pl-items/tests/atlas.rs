//! The atlas builder against a small synthetic jar (no game files): flat items, tinted layers,
//! cube blocks lit like the game's slot, the inventory-state choice of item definitions, specials
//! (heads, beds, the copper golem statue), hidden dye layers, dark icons, the glint frame, old jars.

use pl_items::{
    AssetSource, BuildOptions, IconKind, Image, MemSource, build_atlas, has_item_definitions,
};

fn png(width: u32, height: u32, color: [u8; 4]) -> Vec<u8> {
    let mut image = Image::new(width, height);
    for y in 0..height {
        for x in 0..width {
            image.put(x, y, color);
        }
    }
    image.encode_png().unwrap()
}

fn item(source: &mut MemSource, id: &str, json: &str) {
    source.insert(
        format!("assets/minecraft/items/{id}.json"),
        json.as_bytes().to_vec(),
    );
}

fn model(source: &mut MemSource, path: &str, json: &str) {
    source.insert(
        format!("assets/minecraft/models/{path}.json"),
        json.as_bytes().to_vec(),
    );
}

fn texture(source: &mut MemSource, path: &str, width: u32, height: u32, color: [u8; 4]) {
    source.insert(
        format!("assets/minecraft/textures/{path}.png"),
        png(width, height, color),
    );
}

/// A flat item of one colour (`item/<name>`).
fn flat(source: &mut MemSource, name: &str, color: [u8; 4]) {
    texture(source, &format!("item/{name}"), 16, 16, color);
    model(
        source,
        &format!("item/{name}"),
        &format!(r#"{{"parent":"item/generated","textures":{{"layer0":"item/{name}"}}}}"#),
    );
    item(
        source,
        name,
        &format!(r#"{{"model":{{"type":"minecraft:model","model":"minecraft:item/{name}"}}}}"#),
    );
}

/// A tiny jar with the vanilla parents the test items need.
fn jar() -> MemSource {
    let mut s = MemSource::default();
    model(
        &mut s,
        "item/generated",
        r#"{"parent":"builtin/generated","gui_light":"front"}"#,
    );
    model(&mut s, "item/handheld", r#"{"parent":"item/generated"}"#);
    model(
        &mut s,
        "block/block",
        r#"{"gui_light":"side","display":{"gui":{"rotation":[30,225,0],"translation":[0,0,0],"scale":[0.625,0.625,0.625]}}}"#,
    );
    model(
        &mut s,
        "block/cube",
        r##"{"parent":"block/block","elements":[{"from":[0,0,0],"to":[16,16,16],"faces":{
            "down":{"texture":"#down"},"up":{"texture":"#up"},"north":{"texture":"#north"},
            "south":{"texture":"#south"},"west":{"texture":"#west"},"east":{"texture":"#east"}}}]}"##,
    );
    model(
        &mut s,
        "block/cube_all",
        r##"{"parent":"block/cube","textures":{"particle":"#all","down":"#all","up":"#all","north":"#all","east":"#all","south":"#all","west":"#all"}}"##,
    );
    // A sword: one layer.
    texture(&mut s, "item/sword", 16, 16, [200, 200, 220, 255]);
    model(
        &mut s,
        "item/sword",
        r#"{"parent":"item/handheld","textures":{"layer0":"item/sword"}}"#,
    );
    item(
        &mut s,
        "sword",
        r#"{"model":{"type":"minecraft:model","model":"minecraft:item/sword"}}"#,
    );
    // A potion: a tinted liquid layer under the bottle.
    texture(&mut s, "item/liquid", 16, 16, [255, 255, 255, 255]);
    texture(&mut s, "item/bottle", 16, 16, [0, 0, 0, 0]);
    model(
        &mut s,
        "item/potion",
        r#"{"parent":"item/generated","textures":{"layer0":"item/liquid","layer1":"item/bottle"}}"#,
    );
    item(
        &mut s,
        "potion",
        r#"{"model":{"type":"minecraft:model","model":"minecraft:item/potion","tints":[{"type":"minecraft:potion","default":-13083194}]}}"#,
    );
    // A stone block, in the 26.x texture form (an object with a sprite).
    texture(&mut s, "block/stone", 16, 16, [128, 128, 128, 255]);
    model(
        &mut s,
        "block/stone",
        r#"{"parent":"block/cube_all","textures":{"all":{"sprite":"minecraft:block/stone"}}}"#,
    );
    item(
        &mut s,
        "stone",
        r#"{"model":{"type":"minecraft:model","model":"minecraft:block/stone"}}"#,
    );
    // A bow: the condition at rest (not in use).
    texture(&mut s, "item/bow", 16, 16, [120, 80, 40, 255]);
    texture(&mut s, "item/bow_pulling", 16, 16, [255, 0, 0, 255]);
    model(
        &mut s,
        "item/bow",
        r#"{"parent":"item/generated","textures":{"layer0":"item/bow"}}"#,
    );
    model(
        &mut s,
        "item/bow_pulling",
        r#"{"parent":"item/generated","textures":{"layer0":"item/bow_pulling"}}"#,
    );
    item(
        &mut s,
        "bow",
        r#"{"model":{"type":"minecraft:condition","property":"minecraft:using_item",
            "on_false":{"type":"minecraft:model","model":"minecraft:item/bow"},
            "on_true":{"type":"minecraft:model","model":"minecraft:item/bow_pulling"}}}"#,
    );
    // A select on the display context: the inventory ("gui") case wins.
    texture(&mut s, "item/spear_gui", 16, 16, [0, 200, 0, 255]);
    texture(&mut s, "item/spear_hand", 16, 16, [0, 0, 200, 255]);
    model(
        &mut s,
        "item/spear_gui",
        r#"{"parent":"item/generated","textures":{"layer0":"item/spear_gui"}}"#,
    );
    model(
        &mut s,
        "item/spear_hand",
        r#"{"parent":"item/generated","textures":{"layer0":"item/spear_hand"}}"#,
    );
    item(
        &mut s,
        "spear",
        r#"{"model":{"type":"minecraft:select","property":"minecraft:display_context",
            "cases":[{"when":["gui","ground"],"model":{"type":"minecraft:model","model":"minecraft:item/spear_gui"}}],
            "fallback":{"type":"minecraft:model","model":"minecraft:item/spear_hand"}}}"#,
    );
    // A head drawn in code from an entity texture.
    texture(
        &mut s,
        "entity/skeleton/skeleton",
        64,
        32,
        [220, 220, 210, 255],
    );
    model(
        &mut s,
        "item/template_skull",
        r#"{"display":{"gui":{"rotation":[30,45,0],"translation":[0,3,0],"scale":[1,1,1]}}}"#,
    );
    item(
        &mut s,
        "skeleton_skull",
        r#"{"model":{"type":"minecraft:special","base":"minecraft:item/template_skull",
            "model":{"type":"minecraft:head","kind":"skeleton"},
            "transformation":{"left_rotation":[1,0,0,0],"right_rotation":[0,0,0,1],"scale":[1,1,1],"translation":[0.5,0,0.5]}}}"#,
    );
    // A special this crate does not draw.
    item(
        &mut s,
        "mystery_statue",
        r#"{"model":{"type":"minecraft:special","base":"minecraft:item/template_copper_golem_statue","model":{"type":"minecraft:mystery"}}}"#,
    );
    // The copper golem statue, as 26.1 defines it: copper, with the antenna's bulb green and the
    // legs blue in this texture, so the test sees which way up it stands.
    let mut golem = Image::new(64, 64);
    for y in 0..64 {
        for x in 0..64 {
            let color = if (37..53).contains(&x) && y < 8 {
                [60, 220, 90, 255]
            } else if x < 32 && (27..36).contains(&y) {
                [40, 60, 220, 255]
            } else {
                [180, 110, 70, 255]
            };
            golem.put(x, y, color);
        }
    }
    s.insert(
        "assets/minecraft/textures/entity/copper_golem/copper_golem.png",
        golem.encode_png().unwrap(),
    );
    model(
        &mut s,
        "item/template_copper_golem_statue",
        r#"{"display":{"gui":{"rotation":[30,45,180],"translation":[0,2,0],"scale":[0.55,0.55,0.55]}}}"#,
    );
    item(
        &mut s,
        "copper_golem_statue",
        r#"{"model":{"type":"minecraft:select","block_state_property":"copper_golem_pose","property":"minecraft:block_state",
            "cases":[{"when":"sitting","model":{"type":"minecraft:special","base":"minecraft:item/template_copper_golem_statue",
                "model":{"type":"minecraft:copper_golem_statue","pose":"sitting","texture":"minecraft:textures/entity/copper_golem/copper_golem.png"}}}],
            "fallback":{"type":"minecraft:special","base":"minecraft:item/template_copper_golem_statue",
                "model":{"type":"minecraft:copper_golem_statue","pose":"standing","texture":"minecraft:textures/entity/copper_golem/copper_golem.png"}},
            "transformation":{"left_rotation":[0,0,0,1],"right_rotation":[0,0,0,1],"scale":[1,-1,-1],"translation":[0.5,1.5,0.5]}}}"#,
    );
    // Dark grey with a few highlights (a netherite ingot) still reads on a dark slot, as does a
    // saturated dark red; a near-black item full of holes (leaves) keeps its holes, so no edge.
    let mut leaves = Image::new(16, 16);
    for y in 0..16 {
        for x in 0..16 {
            if (x + y) % 2 == 0 {
                leaves.put(x, y, [20, 24, 20, 255]);
            }
        }
    }
    s.insert(
        "assets/minecraft/textures/item/dark_leaves.png",
        leaves.encode_png().unwrap(),
    );
    model(
        &mut s,
        "item/dark_leaves",
        r#"{"parent":"item/generated","textures":{"layer0":"item/dark_leaves"}}"#,
    );
    item(
        &mut s,
        "dark_leaves",
        r#"{"model":{"type":"minecraft:model","model":"minecraft:item/dark_leaves"}}"#,
    );
    let mut ingot = Image::new(16, 16);
    for y in 0..16 {
        for x in 0..16 {
            let lit = (x + y) % 5 == 0;
            ingot.put(
                x,
                y,
                if lit {
                    [150, 140, 148, 255]
                } else {
                    [61, 58, 61, 255]
                },
            );
        }
    }
    s.insert(
        "assets/minecraft/textures/item/netherite_ingot.png",
        ingot.encode_png().unwrap(),
    );
    model(
        &mut s,
        "item/netherite_ingot",
        r#"{"parent":"item/generated","textures":{"layer0":"item/netherite_ingot"}}"#,
    );
    item(
        &mut s,
        "netherite_ingot",
        r#"{"model":{"type":"minecraft:model","model":"minecraft:item/netherite_ingot"}}"#,
    );
    texture(&mut s, "item/red_dye", 16, 16, [142, 32, 32, 255]);
    model(
        &mut s,
        "item/red_dye",
        r#"{"parent":"item/generated","textures":{"layer0":"item/red_dye"}}"#,
    );
    item(
        &mut s,
        "red_dye",
        r#"{"model":{"type":"minecraft:model","model":"minecraft:item/red_dye"}}"#,
    );
    // Near-black, but dark red: redstone's outline reads by its colour.
    flat(&mut s, "redstone", [75, 8, 8, 255]);
    // An armour and tool set whose helmet is near-black and whose sword is not; another set.
    flat(&mut s, "netherite_helmet", [22, 20, 24, 255]);
    flat(&mut s, "netherite_sword", [70, 66, 70, 255]);
    flat(&mut s, "iron_sword", [200, 200, 200, 255]);
    // See-through black glass: the slot shows through it.
    texture(
        &mut s,
        "block/black_stained_glass",
        16,
        16,
        [20, 18, 24, 110],
    );
    model(
        &mut s,
        "block/black_stained_glass",
        r#"{"parent":"block/cube_all","textures":{"all":"minecraft:block/black_stained_glass"}}"#,
    );
    item(
        &mut s,
        "black_stained_glass",
        r#"{"model":{"type":"minecraft:model","model":"minecraft:block/black_stained_glass"}}"#,
    );
    // The dragon head: the dragon's own texture, a skull's placement.
    texture(
        &mut s,
        "entity/enderdragon/dragon",
        256,
        256,
        [40, 30, 50, 255],
    );
    model(
        &mut s,
        "item/dragon_head",
        r#"{"parent":"item/template_skull","display":{"gui":{"translation":[-2,2,0],"rotation":[30,45,0],"scale":[0.6,0.6,0.6]}}}"#,
    );
    item(
        &mut s,
        "dragon_head",
        r#"{"model":{"type":"minecraft:special","base":"minecraft:item/dragon_head",
            "model":{"type":"minecraft:head","kind":"dragon"},
            "transformation":{"left_rotation":[1,0,0,0],"right_rotation":[0,0,0,1],"scale":[1,1,1],"translation":[0.5,0,0.5]}}}"#,
    );
    // The decorated pot: base (neck, top, bottom) and side textures, lit from the front.
    texture(
        &mut s,
        "entity/decorated_pot/decorated_pot_base",
        32,
        32,
        [150, 80, 50, 255],
    );
    texture(
        &mut s,
        "entity/decorated_pot/decorated_pot_side",
        16,
        16,
        [170, 90, 60, 255],
    );
    model(
        &mut s,
        "item/decorated_pot",
        r#"{"gui_light":"front","display":{"gui":{"rotation":[30,45,0],"translation":[0,0,0],"scale":[0.6,0.6,0.6]}}}"#,
    );
    item(
        &mut s,
        "decorated_pot",
        r#"{"model":{"type":"minecraft:special","base":"minecraft:item/decorated_pot","model":{"type":"minecraft:decorated_pot"}}}"#,
    );
    // A bed: two halves drawn from the bed texture, as 26.1's definitions write them.
    texture(&mut s, "entity/bed/red", 64, 64, [160, 40, 40, 255]);
    model(
        &mut s,
        "item/template_bed",
        r#"{"display":{"gui":{"rotation":[30,160,0],"translation":[2,3,0],"scale":[0.5325,0.5325,0.5325]}}}"#,
    );
    model(&mut s, "item/red_bed", r#"{"parent":"item/template_bed"}"#);
    item(
        &mut s,
        "red_bed",
        r#"{"model":{"type":"minecraft:composite","models":[
            {"type":"minecraft:special","base":"minecraft:item/red_bed","model":{"type":"minecraft:bed","part":"head","texture":"minecraft:red"},
             "transformation":{"left_rotation":[0.0,-0.70710677,0.70710677,0.0],"right_rotation":[0,0,0,1],"scale":[1,1,1],"translation":[1,0.5625,1]}},
            {"type":"minecraft:special","base":"minecraft:item/red_bed","model":{"type":"minecraft:bed","part":"foot","texture":"minecraft:red"},
             "transformation":{"left_rotation":[0.0,-0.70710677,0.70710677,0.0],"right_rotation":[0,0,0,1],"scale":[1,1,1],"translation":[1,0.5625,0]}}]}}"#,
    );
    // Wolf armour: the dyed model's dye layer is transparent until the server sends a colour.
    texture(&mut s, "item/wolf_armor", 16, 16, [150, 120, 110, 255]);
    texture(
        &mut s,
        "item/wolf_armor_overlay",
        16,
        16,
        [255, 255, 255, 255],
    );
    model(
        &mut s,
        "item/wolf_armor",
        r#"{"parent":"item/generated","textures":{"layer0":"item/wolf_armor"}}"#,
    );
    model(
        &mut s,
        "item/wolf_armor_dyed",
        r#"{"parent":"item/generated","textures":{"layer0":"item/wolf_armor","layer1":"item/wolf_armor_overlay"}}"#,
    );
    item(
        &mut s,
        "wolf_armor",
        r#"{"model":{"type":"minecraft:condition","property":"minecraft:has_component","component":"minecraft:dyed_color",
            "on_false":{"type":"minecraft:model","model":"minecraft:item/wolf_armor"},
            "on_true":{"type":"minecraft:model","model":"minecraft:item/wolf_armor_dyed","tints":[{"type":"minecraft:constant","value":-1},{"type":"minecraft:dye","default":0}]}}}"#,
    );
    // A dark block.
    texture(&mut s, "block/obsidian", 16, 16, [16, 11, 25, 255]);
    model(
        &mut s,
        "block/obsidian",
        r#"{"parent":"block/cube_all","textures":{"all":"minecraft:block/obsidian"}}"#,
    );
    item(
        &mut s,
        "obsidian",
        r#"{"model":{"type":"minecraft:model","model":"minecraft:block/obsidian"}}"#,
    );
    // A see-through block and a see-through flat item.
    texture(
        &mut s,
        "block/red_stained_glass",
        16,
        16,
        [200, 50, 50, 128],
    );
    model(
        &mut s,
        "block/red_stained_glass",
        r#"{"parent":"block/cube_all","textures":{"all":"minecraft:block/red_stained_glass"}}"#,
    );
    item(
        &mut s,
        "red_stained_glass",
        r#"{"model":{"type":"minecraft:model","model":"minecraft:block/red_stained_glass"}}"#,
    );
    texture(
        &mut s,
        "item/red_stained_glass_pane",
        16,
        16,
        [200, 50, 50, 128],
    );
    model(
        &mut s,
        "item/red_stained_glass_pane",
        r#"{"parent":"item/generated","textures":{"layer0":"item/red_stained_glass_pane"}}"#,
    );
    item(
        &mut s,
        "red_stained_glass_pane",
        r#"{"model":{"type":"minecraft:model","model":"minecraft:item/red_stained_glass_pane"}}"#,
    );
    // The game's glint texture.
    let mut glint = Image::new(128, 128);
    for y in 0..128 {
        for x in 0..128 {
            let streak = if (x + y) % 32 < 3 { 255 } else { 120 };
            glint.put(x, y, [streak / 2, streak / 4, streak, 255]);
        }
    }
    s.insert(
        "assets/minecraft/textures/misc/enchanted_glint_item.png",
        glint.encode_png().unwrap(),
    );
    s
}

fn options() -> BuildOptions {
    BuildOptions {
        minecraft: "test".into(),
        ..BuildOptions::default()
    }
}

#[test]
fn draws_flat_items_from_their_layers() {
    let atlas = build_atlas(&mut jar(), &options()).unwrap();
    let image = Image::decode_png(&atlas.png).unwrap();
    assert_eq!([image.width, image.height], atlas.index.size);
    let sword = &atlas.index.icons["minecraft:sword"];
    assert_eq!(sword.kind, IconKind::Flat);
    assert_eq!(sword.layers.len(), 1);
    let [x, y, w, h] = sword.layers[0].rect;
    assert_eq!([w, h], [16, 16]);
    assert_eq!(image.get(x + 3, y + 3), [200, 200, 220, 255]);
}

#[test]
fn keeps_runtime_tints_apart() {
    let atlas = build_atlas(&mut jar(), &options()).unwrap();
    let potion = &atlas.index.icons["minecraft:potion"];
    assert_eq!(potion.layers.len(), 2);
    let tint = potion.layers[0].tint.as_ref().unwrap();
    assert_eq!(tint.source, "potion");
    assert_eq!(tint.default.as_deref(), Some("#385dc6"));
    assert!(potion.layers[1].tint.is_none());
}

#[test]
fn renders_blocks_lit_like_the_slot() {
    let atlas = build_atlas(&mut jar(), &options()).unwrap();
    let image = Image::decode_png(&atlas.png).unwrap();
    let stone = &atlas.index.icons["minecraft:stone"];
    assert_eq!(stone.kind, IconKind::Block);
    let [x, y, w, h] = stone.layers[0].rect;
    assert_eq!([w, h], [32, 32]);
    // The game's two slot lights: the top at full light, the left side 0.65, the right 0.4.
    let top = image.get(x + 16, y + 7);
    let left = image.get(x + 8, y + 20);
    let right = image.get(x + 24, y + 20);
    assert_eq!(top, [128, 128, 128, 255]);
    assert!(left[3] == 255 && right[3] == 255);
    assert!((left[0] as i32 - 83).abs() <= 1, "left side {left:?}");
    assert!((right[0] as i32 - 51).abs() <= 1, "right side {right:?}");
    // The corners stay empty: it is a cube, not a square.
    assert_eq!(image.get(x, y)[3], 0);
}

#[test]
fn lights_heads_by_their_turned_faces() {
    // The skull's model is upside down until the definition turns it; its top still gets the
    // full light.
    let atlas = build_atlas(&mut jar(), &options()).unwrap();
    let image = Image::decode_png(&atlas.png).unwrap();
    let [x, y, w, h] = atlas.index.icons["minecraft:skeleton_skull"].layers[0].rect;
    let brightest = (y..y + h)
        .flat_map(|py| (x..x + w).map(move |px| (px, py)))
        .map(|(px, py)| image.get(px, py))
        .filter(|p| p[3] == 255)
        .map(|p| p[0])
        .max()
        .unwrap();
    assert_eq!(brightest, 220);
}

#[test]
fn draws_beds_from_their_two_halves() {
    let atlas = build_atlas(&mut jar(), &options()).unwrap();
    let bed = &atlas.index.icons["minecraft:red_bed"];
    assert_eq!(bed.kind, IconKind::Special);
    let image = Image::decode_png(&atlas.png).unwrap();
    let [x, y, w, h] = bed.layers[0].rect;
    let drawn = (y..y + h)
        .flat_map(|py| (x..x + w).map(move |px| (px, py)))
        .filter(|&(px, py)| image.get(px, py)[3] == 255)
        .count();
    // A bed seen from above covers a good part of the icon, wider than a single half.
    assert!(drawn > 250, "{drawn} pixels");
}

#[test]
fn hides_a_dye_layer_until_dyed() {
    let atlas = build_atlas(&mut jar(), &options()).unwrap();
    let armor = &atlas.index.icons["minecraft:wolf_armor"];
    assert_eq!(armor.layers.len(), 2);
    let dye = armor.layers[1].tint.as_ref().unwrap();
    assert_eq!(dye.source, "dye");
    assert_eq!(dye.default, None);
    let json = serde_json::to_string(dye).unwrap();
    assert_eq!(json, r#"{"source":"dye"}"#);
}

#[test]
fn marks_dark_icons() {
    let atlas = build_atlas(&mut jar(), &options()).unwrap();
    // A near-black outline, also see-through.
    assert!(atlas.index.icons["minecraft:obsidian"].dark);
    assert!(atlas.index.icons["minecraft:black_stained_glass"].dark);
    // Outlines that read on a dark slot: dark grey with highlights, a saturated dark red, a lit
    // block, a light item, a near-black red (a grey edge around it would look like a sticker).
    assert!(!atlas.index.icons["minecraft:netherite_ingot"].dark);
    assert!(!atlas.index.icons["minecraft:red_dye"].dark);
    assert!(!atlas.index.icons["minecraft:stone"].dark);
    assert!(!atlas.index.icons["minecraft:sword"].dark);
    assert!(!atlas.index.icons["minecraft:redstone"].dark);
    // Near-black, but its holes would fill in under an edge.
    assert!(!atlas.index.icons["minecraft:dark_leaves"].dark);
    // A set is dark as a whole: the helmet takes the sword of its material along, not another's.
    assert!(atlas.index.icons["minecraft:netherite_helmet"].dark);
    assert!(atlas.index.icons["minecraft:netherite_sword"].dark);
    assert!(!atlas.index.icons["minecraft:iron_sword"].dark);
}

#[test]
fn keeps_a_still_glint_frame() {
    let atlas = build_atlas(&mut jar(), &options()).unwrap();
    let [x, y, w, h] = atlas.index.glint.expect("glint frame");
    assert_eq!([w, h], [32, 32]);
    let image = Image::decode_png(&atlas.png).unwrap();
    let pixels: Vec<[u8; 4]> = (y..y + h)
        .flat_map(|py| (x..x + w).map(move |px| (px, py)))
        .map(|(px, py)| image.get(px, py))
        .collect();
    // Opaque, mostly dark (it is added over the item), with brighter streaks.
    assert!(pixels.iter().all(|p| p[3] == 255));
    let dim = pixels.iter().filter(|p| p[2] < 64).count();
    let bright = pixels.iter().filter(|p| p[2] > 160).count();
    assert!(dim > pixels.len() / 2, "{dim} dim");
    assert!(bright > 0, "no streak");
}

#[test]
fn picks_the_inventory_state() {
    let atlas = build_atlas(&mut jar(), &options()).unwrap();
    let image = Image::decode_png(&atlas.png).unwrap();
    let at = |id: &str| {
        let [x, y, ..] = atlas.index.icons[id].layers[0].rect;
        image.get(x + 4, y + 4)
    };
    assert_eq!(at("minecraft:bow"), [120, 80, 40, 255]);
    assert_eq!(at("minecraft:spear"), [0, 200, 0, 255]);
}

#[test]
fn draws_heads_and_skips_what_it_cannot() {
    let atlas = build_atlas(&mut jar(), &options()).unwrap();
    assert_eq!(
        atlas.index.icons["minecraft:skeleton_skull"].kind,
        IconKind::Special
    );
    assert_eq!(atlas.missing, vec!["minecraft:mystery_statue".to_owned()]);
}

#[test]
fn draws_copper_golem_statues_upright() {
    let atlas = build_atlas(&mut jar(), &options()).unwrap();
    let icon = &atlas.index.icons["minecraft:copper_golem_statue"];
    assert_eq!(icon.kind, IconKind::Special);
    let image = Image::decode_png(&atlas.png).unwrap();
    let [x, y, w, h] = icon.layers[0].rect;
    // The mean height of the antenna's bulb (green) and of the legs (blue) in the icon.
    let mut green = (0.0, 0.0);
    let mut blue = (0.0, 0.0);
    for py in y..y + h {
        for px in x..x + w {
            let [r, g, b, a] = image.get(px, py);
            if a < 255 {
                continue;
            }
            if g > r && g > b {
                green = (green.0 + (py - y) as f32, green.1 + 1.0);
            } else if b > r && b > g {
                blue = (blue.0 + (py - y) as f32, blue.1 + 1.0);
            }
        }
    }
    assert!(green.1 > 0.0 && blue.1 > 0.0, "{green:?} {blue:?}");
    let (bulb, legs) = (green.0 / green.1, blue.0 / blue.1);
    assert!(bulb + 4.0 < legs, "bulb at {bulb}, legs at {legs}");
}

/// Opaque pixels of an icon and their distinct colours.
fn coverage(atlas: &pl_items::Atlas, id: &str) -> (usize, usize) {
    let image = Image::decode_png(&atlas.png).unwrap();
    let [x, y, w, h] = atlas.index.icons[id].layers[0].rect;
    let mut colors = std::collections::HashSet::new();
    let mut drawn = 0;
    for py in y..y + h {
        for px in x..x + w {
            let p = image.get(px, py);
            if p[3] == 255 {
                drawn += 1;
                colors.insert(p);
            }
        }
    }
    (drawn, colors.len())
}

#[test]
fn draws_the_dragon_head_and_the_decorated_pot() {
    let atlas = build_atlas(&mut jar(), &options()).unwrap();
    for id in ["minecraft:dragon_head", "minecraft:decorated_pot"] {
        let icon = &atlas.index.icons[id];
        assert_eq!(icon.kind, IconKind::Special, "{id}");
        let (drawn, shades) = coverage(&atlas, id);
        // A solid model seen from above and the side: a good part of the icon, in more than one
        // light (top and sides).
        assert!(drawn > 150, "{id}: {drawn} pixels");
        assert!(shades >= 2, "{id}: {shades} shades");
    }
    // The pot is lit from the front, like the game's `gui_light: front`: its brightest face (the
    // rim on top, 150) keeps the texture's colour.
    let image = Image::decode_png(&atlas.png).unwrap();
    let [x, y, w, h] = atlas.index.icons["minecraft:decorated_pot"].layers[0].rect;
    let brightest = (y..y + h)
        .flat_map(|py| (x..x + w).map(move |px| (px, py)))
        .map(|(px, py)| image.get(px, py))
        .filter(|p| p[3] == 255)
        .map(|p| p[0])
        .max()
        .unwrap();
    assert!(brightest >= 140, "{brightest}");
}

#[test]
fn draws_3d_icons_again_at_48() {
    let atlas = build_atlas(&mut jar(), &options()).unwrap();
    let large = atlas.index.large.as_ref().expect("large atlas");
    assert_eq!(large.icon, 48);
    let image = Image::decode_png(atlas.large_png.as_ref().unwrap()).unwrap();
    assert_eq!([image.width, image.height], large.size);
    let opaque = |image: &Image, [x, y, w, h]: [u32; 4]| {
        (y..y + h)
            .flat_map(|py| (x..x + w).map(move |px| (px, py)))
            .filter(|&(px, py)| image.get(px, py)[3] == 255)
            .count() as f32
    };
    let small = Image::decode_png(&atlas.png).unwrap();
    for id in [
        "minecraft:stone",
        "minecraft:skeleton_skull",
        "minecraft:red_bed",
    ] {
        let icon = &atlas.index.icons[id];
        let rect = icon.large.expect(id);
        assert_eq!([rect[2], rect[3]], [48, 48], "{id}");
        // The same picture, one and a half times as wide: about 2.25 times the pixels.
        let ratio = opaque(&image, rect) / opaque(&small, icon.layers[0].rect);
        assert!((2.0..2.5).contains(&ratio), "{id}: {ratio}");
    }
    // Flat icons are crisp at every whole multiple of 16 already.
    assert!(atlas.index.icons["minecraft:sword"].large.is_none());
    let without = BuildOptions {
        large_size: None,
        ..options()
    };
    let plain = build_atlas(&mut jar(), &without).unwrap();
    assert!(plain.large_png.is_none() && plain.index.large.is_none());
    assert!(plain.index.icons["minecraft:stone"].large.is_none());
    assert_eq!(plain.png, atlas.png);
}

#[test]
fn places_every_icon_on_even_pixels() {
    // Halved (a 32 px drawing shown 16 px wide), an icon then keeps the same pixels wherever it
    // lies, in every version's atlas.
    let atlas = build_atlas(&mut jar(), &options()).unwrap();
    assert!(atlas.index.large.is_some());
    let even = |[x, y, ..]: [u32; 4]| x.is_multiple_of(2) && y.is_multiple_of(2);
    for (id, icon) in &atlas.index.icons {
        for rect in icon.layers.iter().map(|layer| layer.rect).chain(icon.large) {
            assert!(even(rect), "{id} at {rect:?}");
        }
    }
    assert!(even(atlas.index.glint.unwrap()));
}

#[test]
fn keeps_see_through_pixels_see_through() {
    // Stained glass shows whatever slot it is drawn on, as in the game; laid over a fixed grey it
    // read as concrete on our dark slots.
    let atlas = build_atlas(&mut jar(), &options()).unwrap();
    let image = Image::decode_png(&atlas.png).unwrap();
    for id in [
        "minecraft:red_stained_glass",
        "minecraft:red_stained_glass_pane",
    ] {
        let [x, y, w, h] = atlas.index.icons[id].layers[0].rect;
        let pixels: Vec<[u8; 4]> = (y..y + h)
            .flat_map(|py| (x..x + w).map(move |px| (px, py)))
            .map(|(px, py)| image.get(px, py))
            .filter(|p| p[3] > 0)
            .collect();
        assert!(!pixels.is_empty(), "{id}");
        assert!(pixels.iter().all(|p| p[3] < 255), "{id}: drawn opaque");
    }
}

#[test]
fn is_deterministic() {
    let a = build_atlas(&mut jar(), &options()).unwrap();
    let b = build_atlas(&mut jar(), &options()).unwrap();
    assert_eq!(a.png, b.png);
    assert_eq!(a.large_png, b.large_png);
    assert_eq!(a.index, b.index);
}

#[test]
fn reads_old_jars_by_item_model() {
    let mut s = MemSource::default();
    model(
        &mut s,
        "item/generated",
        r#"{"parent":"builtin/generated"}"#,
    );
    texture(&mut s, "items/apple", 16, 16, [220, 20, 20, 255]);
    model(
        &mut s,
        "item/apple",
        r#"{"parent":"item/generated","textures":{"layer0":"items/apple"}}"#,
    );
    let atlas = build_atlas(&mut s, &options()).unwrap();
    assert!(atlas.index.icons.contains_key("minecraft:apple"));
    assert!(s.read("assets/minecraft/items/apple.json").is_none());
    assert!(!has_item_definitions(&mut s));
    assert!(has_item_definitions(&mut jar()));
}

#[test]
fn refuses_a_jar_without_items() {
    assert!(build_atlas(&mut MemSource::default(), &options()).is_err());
}

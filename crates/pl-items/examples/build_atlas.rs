//! Builds the vanilla item atlas from a client jar, for the site (the launcher builds its own at
//! run time from the player's jar). The output stays outside git; the site serves it from our CDN.
//!
//!   cargo run -p pl-items --release --example build_atlas -- <client.jar> <out dir> [minecraft version] [--cdn] [--block <px>]
//!
//! Writes `<out dir>/atlas.png`, `<out dir>/atlas-48.png` (3D icons drawn at 48 px) and
//! `<out dir>/index.json`, and lists items without an icon.
//!
//! With `--cdn`, writes the CDN layout `loadVanillaAtlas` (`@pl/ui/items`) reads instead, names that
//! never change for the same bytes: `<out dir>/<version>/atlas.<sha256>.png`,
//! `<out dir>/<version>/atlas-48.<sha256>.png`,
//! `<out dir>/<version>/index.<sha256>.json` (its `image` and `large.image` name those) and
//! `<out dir>/manifest.json`, which gains the version (the first version written is the default;
//! edit `default` to change it).

use std::path::{Path, PathBuf};

use pl_items::{BuildOptions, JarSource, build_atlas};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn fail(message: impl std::fmt::Display) -> ! {
    eprintln!("{message}");
    std::process::exit(1);
}

fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn write(path: &Path, bytes: &[u8]) {
    if let Some(dir) = path.parent()
        && let Err(error) = std::fs::create_dir_all(dir)
    {
        fail(format!("{}: {error}", dir.display()));
    }
    if let Err(error) = std::fs::write(path, bytes) {
        fail(format!("{}: {error}", path.display()));
    }
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let cdn = args.iter().any(|a| a == "--cdn");
    args.retain(|a| a != "--cdn");
    // `--block <px>`: the size of rendered (3D) icons, for trying other sizes (default 32).
    let block_at = args.iter().position(|a| a == "--block");
    let block_size = block_at.map(|at| {
        let value = args.get(at + 1).and_then(|v| v.parse::<u32>().ok());
        args.drain(at..(at + 2).min(args.len()));
        value.unwrap_or_else(|| fail("--block needs a size in pixels"))
    });
    let (Some(jar), Some(out)) = (args.first(), args.get(1)) else {
        eprintln!(
            "usage: build_atlas <client.jar> <out dir> [minecraft version] [--cdn] [--block <px>]"
        );
        std::process::exit(2);
    };
    let jar = PathBuf::from(jar);
    let out = PathBuf::from(out);
    let minecraft = args.get(2).cloned().unwrap_or_else(|| {
        jar.file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    });
    let mut source = JarSource::open(&jar).unwrap_or_else(|error| fail(error));
    let mut options = BuildOptions {
        minecraft: minecraft.clone(),
        block_size: block_size.unwrap_or(BuildOptions::default().block_size),
        ..BuildOptions::default()
    };
    let mut atlas = build_atlas(&mut source, &options).unwrap_or_else(|error| fail(error));
    if cdn {
        // The pictures do not depend on their names, so the index takes the hashed names afterwards.
        options.image_name = format!("atlas.{}.png", sha256(&atlas.png));
        atlas.index.image = options.image_name.clone();
        if let (Some(png), Some(large)) = (&atlas.large_png, atlas.index.large.as_mut()) {
            large.image = format!("atlas-{}.{}.png", large.icon, sha256(png));
        }
        let index = serde_json::to_vec(&atlas.index).expect("index serialises");
        let index_name = format!("index.{}.json", sha256(&index));
        let dir = out.join(&minecraft);
        write(&dir.join(&options.image_name), &atlas.png);
        if let (Some(png), Some(large)) = (&atlas.large_png, &atlas.index.large) {
            write(&dir.join(&large.image), png);
            println!("{}/{}", minecraft, large.image);
        }
        write(&dir.join(&index_name), &index);
        let manifest_path = out.join("manifest.json");
        let mut manifest: Value = std::fs::read(&manifest_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_else(|| json!({ "format": 1, "versions": {} }));
        manifest["versions"][&minecraft] = json!(format!("{minecraft}/{index_name}"));
        if manifest.get("default").and_then(Value::as_str).is_none() {
            manifest["default"] = json!(minecraft);
        }
        let text = serde_json::to_vec_pretty(&manifest).expect("manifest serialises");
        write(&manifest_path, &text);
        println!("{}/{}", minecraft, options.image_name);
        println!("{minecraft}/{index_name}");
    } else {
        let index = serde_json::to_vec(&atlas.index).expect("index serialises");
        write(&out.join("atlas.png"), &atlas.png);
        if let (Some(png), Some(large)) = (&atlas.large_png, &atlas.index.large) {
            write(&out.join(&large.image), png);
        }
        write(&out.join("index.json"), &index);
    }
    println!(
        "minecraft {minecraft}: {} icons, atlas {}x{} ({} KiB), {} without an icon",
        atlas.index.icons.len(),
        atlas.index.size[0],
        atlas.index.size[1],
        atlas.png.len() / 1024,
        atlas.missing.len()
    );
    if let (Some(png), Some(large)) = (&atlas.large_png, &atlas.index.large) {
        println!(
            "large atlas ({} px icons): {}x{} ({} KiB)",
            large.icon,
            large.size[0],
            large.size[1],
            png.len() / 1024
        );
    }
    for id in &atlas.missing {
        println!("  no icon: {id}");
    }
}

use std::path::Path;

/// The in-game overlay's jars (OVL-B) are the resource folder `overlay/` (`tauri.conf.json`), filled
/// by `node scripts/release/overlay-jars.mjs build` and empty in a fresh checkout. tauri-build needs
/// the folder to exist; a release build that ships needs the jars in it, so it never goes out
/// with an overlay that silently does nothing.
fn main() {
    let dir = Path::new("overlay");
    if let Err(error) = std::fs::create_dir_all(dir) {
        panic!("could not create the overlay resource folder: {error}");
    }
    println!("cargo:rerun-if-changed=overlay/index.json");
    if shipping_release() {
        check_overlay_jars(dir);
    }
    tauri_build::build()
}

/// The release profile without debug assertions: what installers are made of. `tauri build
/// --debug` and the `e2e-release` measurement profile (debug assertions on) are not.
fn shipping_release() -> bool {
    std::env::var("PROFILE").as_deref() == Ok("release")
        && std::env::var_os("CARGO_CFG_DEBUG_ASSERTIONS").is_none()
}

fn check_overlay_jars(dir: &Path) {
    let fail = |why: &str| -> ! {
        panic!(
            "{why}. A release build ships the in-game overlay's jars: run `node \
             scripts/release/overlay-jars.mjs build` (or `check`) first."
        )
    };
    let index = std::fs::read(dir.join("index.json"))
        .unwrap_or_else(|_| fail("overlay/index.json is missing"));
    let index: serde_json::Value =
        serde_json::from_slice(&index).unwrap_or_else(|_| fail("overlay/index.json is unreadable"));
    let jars = index
        .get("jars")
        .and_then(serde_json::Value::as_array)
        .filter(|jars| !jars.is_empty())
        .unwrap_or_else(|| fail("overlay/index.json lists no jar"));
    for jar in jars {
        let (Some(file), Some(size)) = (
            jar.get("file").and_then(serde_json::Value::as_str),
            jar.get("size").and_then(serde_json::Value::as_u64),
        ) else {
            fail("overlay/index.json has an entry without a file or a size");
        };
        let path = dir.join(file);
        println!("cargo:rerun-if-changed={}", path.display());
        if std::fs::metadata(&path).map(|meta| meta.len()).ok() != Some(size) {
            fail(&format!(
                "overlay/{file} is missing or not the file the index names"
            ));
        }
    }
}

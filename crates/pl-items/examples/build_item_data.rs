//! Builds one Minecraft version's item data for AFK+ orders (`pl_items::data`) from the game's own
//! files, for our CDN. Nothing of the game and none of the output is committed.
//!
//!   java -DbundlerMainClass=net.minecraft.data.Main -jar server.jar --reports --output <gen>
//!   cargo run -p pl-items --release --example build_item_data -- <server.jar> <gen>/reports <tr_tr.json> <out dir> [--ench-from <server.jar>] [--default <version>] [--aliases <aliases.json>]
//!
//! `server.jar` is Mojang's server download for the version (the bundle; its inner jar is read)
//! and `tr_tr.json` the Turkish language file from the version's asset index. Writes
//! `<out dir>/<version>/items.<first 12 hex of its SHA-256>.json` (an older file of that version
//! in the folder is removed) and adds the version to `<out dir>/manifest.json` (the first version
//! written is the default until `--default` names another), then prints what the build noticed and
//! the size. Before 1.21 the game keeps enchantments in code: `--ench-from` names a 1.21+ server
//! jar whose enchantment definitions to take (only the enchantments the version's registry has).

use std::collections::{BTreeMap, HashMap};
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use pl_items::AssetSource;
use pl_items::JarSource;
use pl_items::data::{
    Aliases, DataFile, DataInputs, DataManifest, ItemData, Report, build_item_data,
    compare_versions,
};
use sha1::Sha1;
use sha2::{Digest, Sha256};

fn fail(message: impl std::fmt::Display) -> ! {
    eprintln!("{message}");
    std::process::exit(1);
}

fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|error| fail(format!("{}: {error}", path.display())))
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

fn sha1_hex(bytes: &[u8]) -> String {
    hex::encode(Sha1::digest(bytes))
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// The largest inner jar read into memory (26.3's is 27 MB).
const MAX_INNER_JAR: u64 = 256 * 1024 * 1024;

/// The game's own jar and version: the inner jar of Mojang's server bundle
/// (`META-INF/versions.list`: `<sha256>\t<version>\t<path>`), or the jar itself with its
/// `version.json`.
fn game_jar(path: &Path) -> (JarSource<Cursor<Vec<u8>>>, String) {
    let bytes = read(path);
    let mut outer = zip::ZipArchive::new(Cursor::new(bytes.clone()))
        .unwrap_or_else(|error| fail(format!("{}: {error}", path.display())));
    let listed = outer
        .by_name("META-INF/versions.list")
        .ok()
        .map(|mut file| {
            let mut text = String::new();
            file.read_to_string(&mut text)
                .unwrap_or_else(|error| fail(format!("versions.list: {error}")));
            text
        });
    let (inner, version) = match listed {
        Some(text) => {
            let line = text.lines().next().unwrap_or_default();
            let parts: Vec<&str> = line.split('\t').collect();
            let [hash, version, inner_path] = parts.as_slice() else {
                fail(format!("{}: odd versions.list", path.display()));
            };
            let mut file = outer
                .by_name(&format!("META-INF/versions/{inner_path}"))
                .unwrap_or_else(|error| fail(format!("{inner_path}: {error}")));
            if file.size() > MAX_INNER_JAR {
                fail(format!("{inner_path}: too large"));
            }
            let mut inner = Vec::with_capacity(file.size() as usize);
            file.read_to_end(&mut inner)
                .unwrap_or_else(|error| fail(format!("{inner_path}: {error}")));
            if sha256_hex(&inner) != *hash {
                fail(format!("{inner_path}: SHA-256 differs from versions.list"));
            }
            (inner, version.to_string())
        }
        None => (bytes, String::new()),
    };
    let mut jar = JarSource::new(Cursor::new(inner)).unwrap_or_else(|error| fail(error));
    let own: Option<String> = jar
        .read("version.json")
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .and_then(|v| v.get("id").and_then(|id| id.as_str()).map(str::to_owned));
    let version = match (version.is_empty(), own) {
        (true, Some(own)) => own,
        (false, Some(own)) if own != version => fail(format!(
            "{}: versions.list says {version}, version.json {own}",
            path.display()
        )),
        (false, _) => version,
        (true, None) => fail(format!("{}: no version", path.display())),
    };
    (jar, version)
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut option = |name: &str| -> Option<String> {
        let at = args.iter().position(|a| a == name)?;
        let value = args
            .get(at + 1)
            .cloned()
            .unwrap_or_else(|| fail(format!("{name} needs a value")));
        args.drain(at..at + 2);
        Some(value)
    };
    let donor_path = option("--ench-from").map(PathBuf::from);
    let default = option("--default");
    let aliases_path = option("--aliases")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("data/aliases.json"));
    let [jar_path, reports, tr_path, out] = args.as_slice() else {
        eprintln!(
            "usage: build_item_data <server.jar> <reports dir> <tr_tr.json> <out dir> [--ench-from <server.jar>] [--default <version>] [--aliases <aliases.json>]"
        );
        std::process::exit(2);
    };
    let (jar_path, reports, tr_path, out) = (
        PathBuf::from(jar_path),
        PathBuf::from(reports),
        PathBuf::from(tr_path),
        PathBuf::from(out),
    );

    let (mut jar, minecraft) = game_jar(&jar_path);
    let mut sources = BTreeMap::new();
    sources.insert("server".to_string(), sha1_hex(&read(&jar_path)));
    let en = jar
        .read("assets/minecraft/lang/en_us.json")
        .unwrap_or_else(|| fail("the jar has no en_us.json"));
    sources.insert("en".to_string(), sha1_hex(&en));
    let tr_bytes = read(&tr_path);
    sources.insert("tr".to_string(), sha1_hex(&tr_bytes));
    let tr: HashMap<String, String> = serde_json::from_slice(&tr_bytes)
        .unwrap_or_else(|error| fail(format!("{}: {error}", tr_path.display())));
    let aliases = Aliases::parse(&read(&aliases_path)).unwrap_or_else(|error| fail(error));
    let report = Report::read(&mut |rel| std::fs::read(reports.join(rel)).ok())
        .unwrap_or_else(|error| fail(error));
    let mut donor = donor_path.as_ref().map(|path| {
        let (donor, version) = game_jar(path);
        if compare_versions(&version, &minecraft).is_le() {
            fail(format!("the donor {version} is not newer than {minecraft}"));
        }
        sources.insert("ench".to_string(), sha1_hex(&read(path)));
        sources.insert("enchFrom".to_string(), version);
        donor
    });

    let build = build_item_data(DataInputs {
        minecraft: &minecraft,
        jar: &mut jar,
        donor: donor.as_mut().map(|d| d as &mut dyn AssetSource),
        report: &report,
        tr: &tr,
        aliases: &aliases,
        sources,
    })
    .unwrap_or_else(|error| fail(error));

    let bytes = build.data.to_json();
    // The file reads back as what was built (what every reader gets).
    match ItemData::parse(&bytes) {
        Ok(read) if read == build.data && read.to_json() == bytes => {}
        Ok(_) => fail("the written file reads back differently"),
        Err(error) => fail(format!("the written file does not read back: {error}")),
    }
    let sha256 = sha256_hex(&bytes);
    let name = format!("items.{}.json", &sha256[..12]);
    let dir = out.join(&minecraft);
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let file = entry.file_name().to_string_lossy().into_owned();
            if file != name && file.starts_with("items.") && file.ends_with(".json") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    write(&dir.join(&name), &bytes);
    let manifest_path = out.join("manifest.json");
    let mut manifest = match std::fs::read(&manifest_path) {
        Ok(existing) => DataManifest::parse(&existing).unwrap_or_else(|error| fail(error)),
        Err(_) => DataManifest::default(),
    };
    manifest.versions.insert(
        minecraft.clone(),
        DataFile {
            file: format!("{minecraft}/{name}"),
            sha256,
            bytes: bytes.len() as u64,
        },
    );
    if let Some(default) = default {
        manifest.default = default;
    } else if manifest.default.is_empty() {
        manifest.default = minecraft.clone();
    }
    write(&manifest_path, &manifest.to_json());

    let data = &build.data;
    let aliases_count: usize = data.items.iter().map(|i| i.aliases.len()).sum::<usize>()
        + data
            .enchantments
            .iter()
            .map(|e| e.aliases.len())
            .sum::<usize>();
    println!(
        "minecraft {minecraft}: {} items, {} enchantments, {} groups, {} with a description line, {aliases_count} aliases; {minecraft}/{name} {} B",
        data.items.len(),
        data.enchantments.len(),
        data.groups.len(),
        data.items.iter().filter(|i| i.desc.is_some()).count(),
        bytes.len()
    );
    for note in &build.notes {
        println!("  note: {note}");
    }
}

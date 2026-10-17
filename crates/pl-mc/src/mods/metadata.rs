//! Mod descriptor parsers: `fabric.mod.json`, `quilt.mod.json`, `META-INF/mods.toml`,
//! `META-INF/neoforge.mods.toml` and legacy `mcmod.info`. Every parser is tolerant: missing or
//! odd fields are skipped and whatever could be read is returned.

use std::io::{Read, Seek};

use serde_json::{Map, Value};
use zip::ZipArchive;

use super::ModMetadata;
use super::lenient_json;
use crate::gamefiles::fsops::{MAX_ICON_BYTES, MAX_METADATA_BYTES, read_png_limited};
use crate::loaders::LoaderKind;

/// Ids that name the platform rather than a mod; left out of `depends`.
const PLATFORM_IDS: [&str; 10] = [
    "minecraft",
    "java",
    "fabricloader",
    "fabric-loader",
    "quilt_loader",
    "forge",
    "neoforge",
    "fml",
    "mcp",
    "javafml",
];

/// Parsed descriptor plus the icon paths it points at (best first).
#[derive(Debug, Default)]
pub(crate) struct Descriptor {
    pub meta: ModMetadata,
    pub icons: Vec<String>,
}

/// Read all known descriptors from a jar and merge them (the first found is primary).
pub(crate) fn read_metadata<R: Read + Seek>(archive: &mut ZipArchive<R>) -> Option<ModMetadata> {
    let mut found: Vec<Descriptor> = Vec::new();
    if let Some(text) = read_entry_text(archive, "fabric.mod.json") {
        found.extend(parse_fabric(&text));
    }
    if let Some(text) = read_entry_text(archive, "quilt.mod.json") {
        found.extend(parse_quilt(&text));
    }
    let jar_version = read_entry_text(archive, "META-INF/MANIFEST.MF")
        .and_then(|text| manifest_attribute(&text, "Implementation-Version"));
    if let Some(text) = read_entry_text(archive, "META-INF/neoforge.mods.toml") {
        found.extend(parse_mods_toml(
            &text,
            LoaderKind::NeoForge,
            jar_version.as_deref(),
        ));
    }
    if let Some(text) = read_entry_text(archive, "META-INF/mods.toml") {
        found.extend(parse_mods_toml(
            &text,
            LoaderKind::Forge,
            jar_version.as_deref(),
        ));
    }
    if let Some(text) = read_entry_text(archive, "mcmod.info") {
        found.extend(parse_mcmod_info(&text));
    }

    let mut iter = found.into_iter();
    let mut primary = iter.next()?;
    for other in iter {
        merge(&mut primary, other);
    }
    primary.icons.push("pack.png".to_owned());
    primary.meta.icon_png = primary
        .icons
        .iter()
        .find_map(|path| read_entry_png(archive, path));
    Some(primary.meta)
}

fn merge(primary: &mut Descriptor, other: Descriptor) {
    let (meta, extra) = (&mut primary.meta, other.meta);
    for loader in extra.loaders {
        if !meta.loaders.contains(&loader) {
            meta.loaders.push(loader);
        }
    }
    let fill = |slot: &mut Option<String>, value: Option<String>| {
        if slot.is_none() {
            *slot = value;
        }
    };
    fill(&mut meta.id, extra.id);
    fill(&mut meta.name, extra.name);
    fill(&mut meta.version, extra.version);
    fill(&mut meta.description, extra.description);
    fill(&mut meta.homepage, extra.homepage);
    fill(&mut meta.sources, extra.sources);
    if meta.authors.is_empty() {
        meta.authors = extra.authors;
    }
    if meta.depends.is_empty() {
        meta.depends = extra.depends;
    }
    primary.icons.extend(other.icons);
}

fn entry_path(path: &str) -> &str {
    path.trim_start_matches("./").trim_start_matches('/')
}

pub(crate) fn read_entry_text<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
) -> Option<String> {
    let entry = archive.by_name(entry_path(name)).ok()?;
    if entry.is_dir() || entry.size() > MAX_METADATA_BYTES {
        return None;
    }
    let mut bytes = Vec::new();
    entry
        .take(MAX_METADATA_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

pub(crate) fn read_entry_png<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
) -> Option<Vec<u8>> {
    let entry = archive.by_name(entry_path(name)).ok()?;
    if entry.is_dir() || entry.size() > MAX_ICON_BYTES {
        return None;
    }
    read_png_limited(entry, MAX_ICON_BYTES)
}

// --- helpers ---------------------------------------------------------------------------------

fn text(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(s) => Some(s.trim().to_owned()).filter(|s| !s.is_empty()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn field(obj: &Map<String, Value>, key: &str) -> Option<String> {
    text(obj.get(key))
}

/// `version` with `${file.jarVersion}` resolved; other unresolved placeholders give `None`.
pub(super) fn clean_version(raw: Option<String>, jar_version: Option<&str>) -> Option<String> {
    let mut version = raw?;
    if version.contains("${file.jarVersion}") {
        version = version.replace("${file.jarVersion}", jar_version?);
    }
    let version = version.trim().to_owned();
    (!version.is_empty() && !version.contains("${")).then_some(version)
}

/// Authors as strings or `{name: ...}` objects.
fn people(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| match item {
                Value::Object(obj) => field(obj, "name"),
                other => text(Some(other)),
            })
            .collect(),
        Some(Value::String(s)) => text(Some(&Value::String(s.clone()))).into_iter().collect(),
        Some(Value::Object(map)) => map.keys().cloned().collect(),
        _ => Vec::new(),
    }
}

fn dependency_ids(ids: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for id in ids {
        let id = id.trim().to_owned();
        if id.is_empty()
            || PLATFORM_IDS.iter().any(|p| p.eq_ignore_ascii_case(&id))
            || out.contains(&id)
        {
            continue;
        }
        out.push(id);
    }
    out
}

/// Icon field as a path or a `{size: path}` map → candidate paths, best first: the largest
/// size up to 128 px, then larger ones from small to big.
fn icon_candidates(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::String(path)) if !path.trim().is_empty() => vec![path.trim().to_owned()],
        Some(Value::Object(sizes)) => {
            let mut entries: Vec<(u32, String)> = sizes
                .iter()
                .filter_map(|(size, path)| {
                    Some((size.trim().parse().unwrap_or(u32::MAX), text(Some(path))?))
                })
                .collect();
            entries.sort_by_key(|(size, _)| {
                if *size <= 128 {
                    (0, u32::MAX - size)
                } else {
                    (1, *size)
                }
            });
            entries.into_iter().map(|(_, path)| path).collect()
        }
        _ => Vec::new(),
    }
}

// --- Fabric / Quilt --------------------------------------------------------------------------

pub(crate) fn parse_fabric(text_in: &str) -> Option<Descriptor> {
    let value = lenient_json::parse(text_in)?;
    let obj = value.as_object()?;
    let contact = obj.get("contact").and_then(Value::as_object);
    let depends = match obj.get("depends") {
        Some(Value::Object(deps)) => dependency_ids(deps.keys().cloned()),
        _ => Vec::new(),
    };
    Some(Descriptor {
        meta: ModMetadata {
            id: field(obj, "id"),
            name: field(obj, "name"),
            version: clean_version(field(obj, "version"), None),
            description: field(obj, "description"),
            authors: people(obj.get("authors")),
            loaders: vec![LoaderKind::Fabric],
            icon_png: None,
            homepage: contact.and_then(|c| field(c, "homepage")),
            sources: contact.and_then(|c| field(c, "sources")),
            depends,
        },
        icons: icon_candidates(obj.get("icon")),
    })
}

pub(crate) fn parse_quilt(text_in: &str) -> Option<Descriptor> {
    let value = lenient_json::parse(text_in)?;
    let loader = value.get("quilt_loader")?.as_object()?;
    let metadata = loader.get("metadata").and_then(Value::as_object);
    let contact = metadata
        .and_then(|m| m.get("contact"))
        .and_then(Value::as_object);
    let depends = match loader.get("depends") {
        Some(Value::Array(items)) => dependency_ids(items.iter().filter_map(|item| match item {
            Value::String(id) => Some(id.clone()),
            Value::Object(dep) if dep.get("optional").and_then(Value::as_bool) != Some(true) => {
                field(dep, "id")
            }
            _ => None,
        })),
        _ => Vec::new(),
    };
    Some(Descriptor {
        meta: ModMetadata {
            id: field(loader, "id"),
            name: metadata.and_then(|m| field(m, "name")),
            version: clean_version(field(loader, "version"), None),
            description: metadata.and_then(|m| field(m, "description")),
            authors: people(metadata.and_then(|m| m.get("contributors"))),
            loaders: vec![LoaderKind::Quilt],
            icon_png: None,
            homepage: contact.and_then(|c| field(c, "homepage")),
            sources: contact.and_then(|c| field(c, "sources")),
            depends,
        },
        icons: icon_candidates(metadata.and_then(|m| m.get("icon"))),
    })
}

// --- Forge / NeoForge (mods.toml) ------------------------------------------------------------

pub(crate) fn parse_mods_toml(
    text_in: &str,
    loader: LoaderKind,
    jar_version: Option<&str>,
) -> Option<Descriptor> {
    match toml::from_str::<toml::Table>(text_in) {
        Ok(table) => from_toml_table(&table, loader, jar_version),
        Err(error) => {
            tracing::debug!(%error, "mods.toml is not valid TOML, using the fallback reader");
            parse_mods_toml_fallback(text_in, loader, jar_version)
        }
    }
}

fn toml_text(table: &toml::Table, key: &str) -> Option<String> {
    match table.get(key)? {
        toml::Value::String(s) => Some(s.trim().to_owned()).filter(|s| !s.is_empty()),
        _ => None,
    }
}

fn toml_people(value: Option<&toml::Value>) -> Vec<String> {
    match value {
        Some(toml::Value::String(s)) if !s.trim().is_empty() => vec![s.trim().to_owned()],
        Some(toml::Value::Array(items)) => items
            .iter()
            .filter_map(|item| item.as_str())
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

fn from_toml_table(
    table: &toml::Table,
    loader: LoaderKind,
    jar_version: Option<&str>,
) -> Option<Descriptor> {
    let mods = table.get("mods")?.as_array()?;
    let first = mods.first()?.as_table()?;
    let id = toml_text(first, "modId");

    // Dependencies of the first mod; NeoForge mods on 20.2–20.4 still used `mods.toml`, so a
    // dependency on `neoforge` marks the jar as NeoForge.
    let mut neoforge = false;
    let mut required = Vec::new();
    if let Some(deps) = table.get("dependencies").and_then(toml::Value::as_table) {
        for (owner, list) in deps {
            let Some(list) = list.as_array() else {
                continue;
            };
            for dep in list.iter().filter_map(toml::Value::as_table) {
                let Some(dep_id) = toml_text(dep, "modId") else {
                    continue;
                };
                if dep_id.eq_ignore_ascii_case("neoforge") {
                    neoforge = true;
                }
                let is_required = match (dep.get("type"), dep.get("mandatory")) {
                    (Some(toml::Value::String(kind)), _) => kind.eq_ignore_ascii_case("required"),
                    (_, Some(toml::Value::Boolean(mandatory))) => *mandatory,
                    _ => true,
                };
                if is_required && id.as_deref().is_none_or(|id| id == owner) {
                    required.push(dep_id);
                }
            }
        }
    }
    let loader = if neoforge {
        LoaderKind::NeoForge
    } else {
        loader
    };
    let logo = toml_text(first, "logoFile").or_else(|| toml_text(table, "logoFile"));
    let authors = match toml_people(first.get("authors")) {
        list if list.is_empty() => toml_people(table.get("authors")),
        list => list,
    };
    Some(Descriptor {
        meta: ModMetadata {
            id,
            name: toml_text(first, "displayName"),
            version: clean_version(toml_text(first, "version"), jar_version),
            description: toml_text(first, "description"),
            authors,
            loaders: vec![loader],
            icon_png: None,
            homepage: toml_text(first, "displayURL").or_else(|| toml_text(table, "displayURL")),
            sources: None,
            depends: dependency_ids(required),
        },
        icons: logo.into_iter().collect(),
    })
}

/// Line-based reader for `mods.toml` files strict TOML rejects (duplicate keys, bad escapes):
/// takes the first `[[mods]]` table's simple string keys.
pub(super) fn parse_mods_toml_fallback(
    text_in: &str,
    loader: LoaderKind,
    jar_version: Option<&str>,
) -> Option<Descriptor> {
    let start = text_in.find("[[mods]]")?;
    let body = &text_in[start + "[[mods]]".len()..];
    let mut values: Vec<(String, String)> = Vec::new();
    let mut rest = body;
    loop {
        rest = rest.trim_start();
        if rest.is_empty() || rest.starts_with('[') {
            break;
        }
        if rest.starts_with('#') {
            rest = rest.split_once('\n').map_or("", |(_, r)| r);
            continue;
        }
        let Some((key, after)) = rest.split_once('=') else {
            break;
        };
        let key = key.trim().to_owned();
        if key.contains('\n') {
            // Not a key/value line; skip it.
            rest = rest.split_once('\n').map_or("", |(_, r)| r);
            continue;
        }
        let after = after.trim_start_matches([' ', '\t']);
        let (value, remaining) = toml_fallback_value(after);
        values.push((key, value));
        rest = remaining;
    }
    let get = |key: &str| {
        values
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.trim().to_owned())
            .filter(|v| !v.is_empty())
    };
    Some(Descriptor {
        meta: ModMetadata {
            id: get("modId"),
            name: get("displayName"),
            version: clean_version(get("version"), jar_version),
            description: get("description"),
            authors: get("authors").into_iter().collect(),
            loaders: vec![loader],
            icon_png: None,
            homepage: get("displayURL"),
            sources: None,
            depends: Vec::new(),
        },
        icons: get("logoFile").into_iter().collect(),
    })
}

/// Read one TOML value (string forms only; anything else is taken up to the end of the line).
fn toml_fallback_value(input: &str) -> (String, &str) {
    for quote in ["'''", "\"\"\""] {
        if let Some(body) = input.strip_prefix(quote) {
            return match body.find(quote) {
                Some(end) => (
                    body[..end].trim_start_matches('\n').to_owned(),
                    &body[end + 3..],
                ),
                None => (body.to_owned(), ""),
            };
        }
    }
    if let Some(body) = input.strip_prefix('\'') {
        let end = body.find(['\'', '\n']).unwrap_or(body.len());
        let rest = body.get(end + 1..).unwrap_or("");
        return (body[..end].to_owned(), rest);
    }
    if let Some(body) = input.strip_prefix('"') {
        let mut value = String::new();
        let mut chars = body.char_indices();
        while let Some((i, c)) = chars.next() {
            match c {
                '"' | '\n' => return (value, &body[i + 1..]),
                '\\' => match chars.next() {
                    Some((_, 'n')) => value.push('\n'),
                    Some((_, 't')) => value.push('\t'),
                    Some((_, other)) => value.push(other),
                    None => break,
                },
                c => value.push(c),
            }
        }
        return (value, "");
    }
    let end = input.find('\n').unwrap_or(input.len());
    let line = &input[..end];
    let line = line.split(" #").next().unwrap_or(line);
    (line.trim().to_owned(), &input[end..])
}

// --- mcmod.info ------------------------------------------------------------------------------

pub(crate) fn parse_mcmod_info(text_in: &str) -> Option<Descriptor> {
    let value = lenient_json::parse(text_in)?;
    let list = match &value {
        Value::Array(items) => items,
        Value::Object(obj) => obj.get("modList").and_then(Value::as_array)?,
        _ => return None,
    };
    let first = list.iter().find_map(Value::as_object)?;
    let mut authors = people(first.get("authorList"));
    if authors.is_empty() {
        authors = people(first.get("authors"));
    }
    let depends = ["requiredMods", "dependencies"]
        .iter()
        .find_map(|key| first.get(*key).and_then(Value::as_array))
        .map(|items| {
            dependency_ids(items.iter().filter_map(|item| {
                let id = item.as_str()?;
                // "Forge@[10.13,)" → "Forge"; "required-after:x" style keys are not ids.
                Some(id.split('@').next().unwrap_or(id).to_owned())
            }))
        })
        .unwrap_or_default();
    Some(Descriptor {
        meta: ModMetadata {
            id: field(first, "modid"),
            name: field(first, "name"),
            version: clean_version(field(first, "version"), None),
            description: field(first, "description"),
            authors,
            loaders: vec![LoaderKind::Forge],
            icon_png: None,
            homepage: field(first, "url"),
            sources: None,
            depends,
        },
        icons: field(first, "logoFile").into_iter().collect(),
    })
}

// --- MANIFEST.MF -----------------------------------------------------------------------------

/// Read a main-section attribute from `META-INF/MANIFEST.MF` (handles continuation lines).
pub(crate) fn manifest_attribute(manifest: &str, key: &str) -> Option<String> {
    let mut current: Option<(String, String)> = None;
    let matches = |entry: &Option<(String, String)>| {
        entry
            .as_ref()
            .filter(|(k, v)| k.eq_ignore_ascii_case(key) && !v.trim().is_empty())
            .map(|(_, v)| v.trim().to_owned())
    };
    for line in manifest.lines() {
        if let Some(continued) = line.strip_prefix(' ') {
            if let Some((_, value)) = current.as_mut() {
                value.push_str(continued);
            }
            continue;
        }
        if let Some(found) = matches(&current) {
            return Some(found);
        }
        if line.trim().is_empty() {
            return None; // end of the main section
        }
        current = line.split_once(':').map(|(k, v)| {
            (
                k.trim().to_owned(),
                v.strip_prefix(' ').unwrap_or(v).to_owned(),
            )
        });
    }
    matches(&current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fabric_authors_icons_and_depends() {
        let text = r#"{
            "schemaVersion": 1, "id": "examplemod", "version": "1.2.3", "name": "Example",
            "description": "Does things",
            "authors": ["Alice", {"name": "Bob", "contact": {}}, 5],
            "contact": {"homepage": "https://example.com", "sources": "https://git.example.com"},
            "icon": {"16": "a16.png", "64": "a64.png", "128": "a128.png", "256": "a256.png", "512": "a512.png"},
            "depends": {"fabricloader": ">=0.14", "minecraft": "1.20.x", "fabric-api": "*", "java": ">=17"}
        }"#;
        let d = parse_fabric(text).unwrap();
        assert_eq!(d.meta.id.as_deref(), Some("examplemod"));
        assert_eq!(d.meta.authors, vec!["Alice", "Bob", "5"]);
        assert_eq!(d.meta.depends, vec!["fabric-api"]);
        assert_eq!(d.meta.sources.as_deref(), Some("https://git.example.com"));
        assert_eq!(
            d.icons,
            vec!["a128.png", "a64.png", "a16.png", "a256.png", "a512.png"]
        );
        assert_eq!(d.meta.loaders, vec![LoaderKind::Fabric]);
    }

    #[test]
    fn fabric_string_icon_and_placeholder_version() {
        let d = parse_fabric(r#"{"id":"x","version":"${version}","icon":"assets/x/icon.png"}"#)
            .unwrap();
        assert_eq!(d.meta.version, None);
        assert_eq!(d.icons, vec!["assets/x/icon.png"]);
    }

    #[test]
    fn quilt_contributors_and_depends() {
        let text = r#"{
            "schema_version": 1,
            "quilt_loader": {
                "group": "org.example", "id": "qmod", "version": "0.1.0",
                "metadata": {
                    "name": "Q Mod", "description": "quilty",
                    "contributors": {"Carol": "Owner", "Dave": ["Developer"]},
                    "contact": {"homepage": "https://q.example"},
                    "icon": "assets/qmod/icon.png"
                },
                "depends": ["quilt_loader", {"id": "qsl"}, {"id": "optional_dep", "optional": true}, {"id": "minecraft", "versions": ">=1.20"}]
            }
        }"#;
        let d = parse_quilt(text).unwrap();
        assert_eq!(d.meta.id.as_deref(), Some("qmod"));
        assert_eq!(d.meta.name.as_deref(), Some("Q Mod"));
        assert_eq!(d.meta.authors, vec!["Carol", "Dave"]);
        assert_eq!(d.meta.depends, vec!["qsl"]);
        assert_eq!(d.meta.loaders, vec![LoaderKind::Quilt]);
    }

    #[test]
    fn forge_mods_toml_with_jar_version() {
        let text = r#"
modLoader="javafml"
loaderVersion="[47,)"
license="MIT"
[[mods]]
modId="examplemod"
version="${file.jarVersion}"
displayName="Example Mod"
displayURL="https://example.com"
logoFile="logo.png"
authors="Love, Cheese"
description='''
Multi-line
description.
'''
[[dependencies.examplemod]]
    modId="forge"
    mandatory=true
    versionRange="[47,)"
[[dependencies.examplemod]]
    modId="jei"
    mandatory=false
[[dependencies.examplemod]]
    modId="geckolib"
    mandatory=true
"#;
        let d = parse_mods_toml(text, LoaderKind::Forge, Some("1.0.4")).unwrap();
        assert_eq!(d.meta.id.as_deref(), Some("examplemod"));
        assert_eq!(d.meta.version.as_deref(), Some("1.0.4"));
        assert_eq!(
            d.meta.description.as_deref(),
            Some("Multi-line\ndescription.")
        );
        assert_eq!(d.meta.authors, vec!["Love, Cheese"]);
        assert_eq!(d.meta.depends, vec!["geckolib"]);
        assert_eq!(d.meta.loaders, vec![LoaderKind::Forge]);
        assert_eq!(d.icons, vec!["logo.png"]);

        let unresolved = parse_mods_toml(text, LoaderKind::Forge, None).unwrap();
        assert_eq!(unresolved.meta.version, None);
    }

    #[test]
    fn neoforge_detection_and_type_field() {
        let text = r#"
modLoader="javafml"
[[mods]]
modId="neomod"
version="2.0"
[[dependencies.neomod]]
modId="neoforge"
type="required"
[[dependencies.neomod]]
modId="curios"
type="optional"
[[dependencies.neomod]]
modId="architectury"
type="required"
"#;
        let d = parse_mods_toml(text, LoaderKind::Forge, None).unwrap();
        assert_eq!(d.meta.loaders, vec![LoaderKind::NeoForge]);
        assert_eq!(d.meta.depends, vec!["architectury"]);
    }

    #[test]
    fn broken_mods_toml_uses_fallback() {
        // Duplicate key: invalid TOML, but Forge's parser accepts it.
        let text = "modLoader=\"javafml\"\nmodLoader=\"javafml\"\n[[mods]]\nmodId=\"broken\"\nversion=\"1.0\" # comment\ndisplayName='Broken Mod'\ndescription=\"\"\"\nLong text\n\"\"\"\n[[dependencies.broken]]\nmodId=\"forge\"\n";
        let d = parse_mods_toml(text, LoaderKind::Forge, None).unwrap();
        assert_eq!(d.meta.id.as_deref(), Some("broken"));
        assert_eq!(d.meta.version.as_deref(), Some("1.0"));
        assert_eq!(d.meta.name.as_deref(), Some("Broken Mod"));
        assert_eq!(d.meta.description.as_deref(), Some("Long text"));
    }

    #[test]
    fn mcmod_info_array_and_modlist_forms() {
        let array = r#"[{"modid":"oldmod","name":"Old Mod","version":"${version}","description":"desc
with newline","authorList":["Eve"],"url":"http://old.example","logoFile":"/logo.png","requiredMods":["Forge@[10.13,)","CodeChickenCore"]},]"#;
        let d = parse_mcmod_info(array).unwrap();
        assert_eq!(d.meta.id.as_deref(), Some("oldmod"));
        assert_eq!(d.meta.version, None);
        assert_eq!(d.meta.description.as_deref(), Some("desc\nwith newline"));
        assert_eq!(d.meta.authors, vec!["Eve"]);
        assert_eq!(d.meta.depends, vec!["CodeChickenCore"]);
        assert_eq!(d.icons, vec!["/logo.png"]);

        let list =
            r#"{"modListVersion": 2, "modList": [{"modid":"m2","name":"M2","authors":["Frank"]}]}"#;
        let d = parse_mcmod_info(list).unwrap();
        assert_eq!(d.meta.id.as_deref(), Some("m2"));
        assert_eq!(d.meta.authors, vec!["Frank"]);
        assert!(parse_mcmod_info("[]").is_none());
        assert!(parse_mcmod_info("{{{").is_none());
    }

    #[test]
    fn manifest_attributes() {
        let manifest = "Manifest-Version: 1.0\r\nImplementation-Title: x\r\nImplementation-Version: 1.2\r\n 3.4-long\r\n\r\nName: other\r\nImplementation-Version: wrong\r\n";
        assert_eq!(
            manifest_attribute(manifest, "Implementation-Version").as_deref(),
            Some("1.23.4-long")
        );
        assert_eq!(manifest_attribute(manifest, "Missing"), None);
        assert_eq!(
            manifest_attribute("Implementation-Version: 9", "implementation-version").as_deref(),
            Some("9")
        );
    }
}

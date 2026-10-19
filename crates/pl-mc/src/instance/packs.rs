//! The game's resource pack list (`options.txt` `resourcePacks`) and Iris's shader choice
//! (`config/iris.properties`), for ready-made mod sets: a pack a set installs is turned on once,
//! a shader pack is chosen once when none is; nothing else in the files changes.
//!
//! `resourcePacks` is a JSON list, lowest priority first (the game's "Selected" list read from
//! the bottom): `["vanilla","fabric","file/Faithful.zip"]`. New packs go on top in the order
//! given (the first name ends up topmost), packs already in the list keep their place, and the
//! player's own packs keep their order. A list the launcher cannot read is left alone.

use std::path::Path;

use super::defaults::OPTIONS_FILE;
use super::options::{key_value, split_lines, write_options};
use crate::error::{Error, Result};
use crate::paths::write_durable;

const RESOURCE_PACKS_KEY: &str = "resourcePacks";
/// Iris's settings file, inside the game directory.
pub const IRIS_PROPERTIES: &str = "config/iris.properties";

/// What [`enable_resource_packs`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackListChange {
    /// The packs are in the list (added now, or already there).
    Done,
    /// `options.txt` does not exist yet: the caller tries again once the game's first file is
    /// written.
    NoOptionsFile,
    /// The list could not be read; nothing was changed (the packs count as handled, so the
    /// launcher never fights a file it does not understand).
    Unreadable,
}

/// The id the game gives a pack file in `resourcepacks/`.
fn pack_id(file_name: &str) -> String {
    format!("file/{file_name}")
}

/// `content` (an `options.txt`) with `names` (pack file names, top first) on top of the
/// resource pack list. `Ok(None)`: nothing to change; `Err(())`: the list is unreadable.
fn enable_in(content: &[u8], names: &[String]) -> std::result::Result<Option<Vec<u8>>, ()> {
    edit_pack_list(content, |list| {
        let mut changed = false;
        for name in names.iter().rev() {
            let id = pack_id(name);
            if !list.contains(&id) {
                list.push(id);
                changed = true;
            }
        }
        changed
    })
}

/// `content` without `names` in the resource pack list.
fn disable_in(content: &[u8], names: &[String]) -> std::result::Result<Option<Vec<u8>>, ()> {
    let ids: Vec<String> = names.iter().map(|name| pack_id(name)).collect();
    edit_pack_list(content, |list| {
        let before = list.len();
        list.retain(|id| !ids.contains(id));
        list.len() != before
    })
}

/// Rewrites the `resourcePacks` line through `edit` (which says whether it changed the list).
/// A missing line starts from `["vanilla"]`, as the game writes it. Every other byte stays.
fn edit_pack_list(
    content: &[u8],
    edit: impl FnOnce(&mut Vec<String>) -> bool,
) -> std::result::Result<Option<Vec<u8>>, ()> {
    let lines = split_lines(content);
    let found = lines
        .iter()
        .rposition(|(text, _)| key_value(text).is_some_and(|(key, _)| key == RESOURCE_PACKS_KEY));
    let mut list: Vec<String> = match found {
        Some(index) => {
            let (_, value) = key_value(lines[index].0).ok_or(())?;
            let value = std::str::from_utf8(value).map_err(|_| ())?.trim();
            if value.is_empty() {
                Vec::new()
            } else {
                serde_json::from_str(value).map_err(|_| ())?
            }
        }
        None => vec!["vanilla".to_owned()],
    };
    if !edit(&mut list) {
        return Ok(None);
    }
    let line = format!(
        "{RESOURCE_PACKS_KEY}:{}",
        serde_json::to_string(&list).map_err(|_| ())?
    );
    let crlf = lines.iter().any(|(_, ending)| *ending == b"\r\n");
    let mut out = Vec::with_capacity(content.len() + line.len() + 2);
    for (index, (text, ending)) in lines.iter().enumerate() {
        if Some(index) == found {
            out.extend_from_slice(line.as_bytes());
        } else {
            out.extend_from_slice(text);
        }
        out.extend_from_slice(ending);
    }
    if found.is_none() {
        let line_end: &[u8] = if crlf { b"\r\n" } else { b"\n" };
        if !out.is_empty() && !out.ends_with(b"\n") {
            out.extend_from_slice(line_end);
        }
        out.extend_from_slice(line.as_bytes());
        out.extend_from_slice(line_end);
    }
    Ok(Some(out))
}

/// Turns the packs `names` (file names in `resourcepacks/`, top first) on in
/// `<game_dir>/options.txt`. Packs already in the list stay where they are.
pub async fn enable_resource_packs(game_dir: &Path, names: &[String]) -> Result<PackListChange> {
    let path = game_dir.join(OPTIONS_FILE);
    let content = match tokio::fs::read(&path).await {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(PackListChange::NoOptionsFile);
        }
        Err(error) => return Err(Error::io(&path, error)),
    };
    match enable_in(&content, names) {
        Ok(Some(updated)) => {
            write_options(&path, updated).await?;
            Ok(PackListChange::Done)
        }
        Ok(None) => Ok(PackListChange::Done),
        Err(()) => {
            tracing::warn!(path = %path.display(), "unreadable resource pack list; left as it is");
            Ok(PackListChange::Unreadable)
        }
    }
}

/// Takes the packs `names` out of the list in `<game_dir>/options.txt` (their files are gone).
/// Returns whether the file changed.
pub async fn disable_resource_packs(game_dir: &Path, names: &[String]) -> Result<bool> {
    let path = game_dir.join(OPTIONS_FILE);
    let content = match tokio::fs::read(&path).await {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(Error::io(&path, error)),
    };
    match disable_in(&content, names) {
        Ok(Some(updated)) => {
            write_options(&path, updated).await?;
            Ok(true)
        }
        Ok(None) | Err(()) => Ok(false),
    }
}

/// A Java properties value: backslashes and characters outside ASCII escaped, as
/// `Properties.load` reads them.
fn properties_value(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            c if c.is_ascii() && !c.is_ascii_control() => out.push(c),
            c => {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    out.push_str(&format!("\\u{unit:04x}"));
                }
            }
        }
    }
    out
}

/// `content` (Iris's settings, `None` when the file does not exist) choosing `file_name`, when
/// it chooses no shader pack yet. `None`: a shader is already chosen.
fn choose_shader_in(content: Option<&[u8]>, file_name: &str) -> Option<Vec<u8>> {
    let content = content.unwrap_or_default();
    let lines = split_lines(content);
    let property = |text: &[u8]| -> Option<(String, String)> {
        let text = std::str::from_utf8(text).ok()?.trim_start();
        if text.starts_with('#') || text.starts_with('!') {
            return None;
        }
        let split = text.find(['=', ':'])?;
        Some((
            text[..split].trim().to_owned(),
            text[split + 1..].trim().to_owned(),
        ))
    };
    let chosen = lines.iter().any(|(text, _)| {
        property(text).is_some_and(|(key, value)| key == "shaderPack" && !value.is_empty())
    });
    if chosen {
        return None;
    }
    let crlf = lines.iter().any(|(_, ending)| *ending == b"\r\n");
    let line_end: &str = if crlf { "\r\n" } else { "\n" };
    let mut out = Vec::with_capacity(content.len() + 64);
    for (text, ending) in &lines {
        let replaced =
            property(text).is_some_and(|(key, _)| key == "shaderPack" || key == "enableShaders");
        if !replaced {
            out.extend_from_slice(text);
            out.extend_from_slice(ending);
        }
    }
    if !out.is_empty() && !out.ends_with(b"\n") {
        out.extend_from_slice(line_end.as_bytes());
    }
    out.extend_from_slice(
        format!(
            "enableShaders=true{line_end}shaderPack={}{line_end}",
            properties_value(file_name)
        )
        .as_bytes(),
    );
    Some(out)
}

/// Chooses the shader pack `file_name` in Iris (`config/iris.properties`) when no shader pack is
/// chosen yet. Returns whether it did.
pub async fn choose_shader_once(game_dir: &Path, file_name: &str) -> Result<bool> {
    let path = game_dir.join(IRIS_PROPERTIES);
    let content = match tokio::fs::read(&path).await {
        Ok(content) => Some(content),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(Error::io(&path, error)),
    };
    let Some(updated) = choose_shader_in(content.as_deref(), file_name) else {
        return Ok(false);
    };
    write_durable(&path, &updated).await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| (*name).to_owned()).collect()
    }

    fn text(bytes: Vec<u8>) -> String {
        String::from_utf8(bytes).unwrap()
    }

    #[test]
    fn packs_go_on_top_in_the_set_order() {
        let options = b"version:4325\r\nresourcePacks:[\"vanilla\",\"fabric\",\"file/Faithful.zip\"]\r\nlang:tr_tr\r\n";
        let out = enable_in(options, &names(&["Shield.zip", "PvP Basics 2.3.1.zip"]))
            .unwrap()
            .unwrap();
        assert_eq!(
            text(out),
            "version:4325\r\nresourcePacks:[\"vanilla\",\"fabric\",\"file/Faithful.zip\",\"file/PvP Basics 2.3.1.zip\",\"file/Shield.zip\"]\r\nlang:tr_tr\r\n",
            "the first set pack ends up topmost; the player's order stays"
        );
    }

    #[test]
    fn packs_already_on_keep_their_place() {
        let options = b"resourcePacks:[\"vanilla\",\"file/Shield.zip\",\"file/Faithful.zip\"]\n";
        assert_eq!(enable_in(options, &names(&["Shield.zip"])), Ok(None));
        let out = enable_in(options, &names(&["Shield.zip", "LowFire.zip"]))
            .unwrap()
            .unwrap();
        assert_eq!(
            text(out),
            "resourcePacks:[\"vanilla\",\"file/Shield.zip\",\"file/Faithful.zip\",\"file/LowFire.zip\"]\n"
        );
    }

    #[test]
    fn a_missing_list_starts_from_vanilla() {
        let out = enable_in(b"lang:en_us", &names(&["LowFire.zip"]))
            .unwrap()
            .unwrap();
        assert_eq!(
            text(out),
            "lang:en_us\nresourcePacks:[\"vanilla\",\"file/LowFire.zip\"]\n"
        );
        let out = enable_in(b"", &names(&["LowFire.zip"])).unwrap().unwrap();
        assert_eq!(
            text(out),
            "resourcePacks:[\"vanilla\",\"file/LowFire.zip\"]\n"
        );
    }

    #[test]
    fn an_unreadable_list_is_left_alone() {
        assert_eq!(
            enable_in(b"resourcePacks:[vanilla\n", &names(&["a.zip"])),
            Err(())
        );
        assert_eq!(
            disable_in(b"resourcePacks:{}\n", &names(&["a.zip"])),
            Err(())
        );
    }

    #[test]
    fn removed_packs_leave_the_list() {
        let options = b"resourcePacks:[\"vanilla\",\"file/LowFire.zip\",\"file/Mine.zip\"]\nincompatibleResourcePacks:[]\n";
        let out = disable_in(options, &names(&["LowFire.zip"]))
            .unwrap()
            .unwrap();
        assert_eq!(
            text(out),
            "resourcePacks:[\"vanilla\",\"file/Mine.zip\"]\nincompatibleResourcePacks:[]\n"
        );
        assert_eq!(disable_in(options, &names(&["Other.zip"])), Ok(None));
    }

    #[test]
    fn a_shader_is_chosen_only_when_none_is() {
        let fresh = choose_shader_in(None, "MakeUp-UltraFast-9.4c.zip").unwrap();
        assert_eq!(
            text(fresh),
            "enableShaders=true\nshaderPack=MakeUp-UltraFast-9.4c.zip\n"
        );
        let iris = b"#Iris settings\r\nenableShaders=false\r\nmaxShadowRenderDistance=32\r\nshaderPack=\r\n";
        let out = choose_shader_in(Some(iris), "Soft.zip").unwrap();
        assert_eq!(
            text(out),
            "#Iris settings\r\nmaxShadowRenderDistance=32\r\nenableShaders=true\r\nshaderPack=Soft.zip\r\n"
        );
        let chosen = b"enableShaders=false\nshaderPack=ComplementaryReimagined_r5.4.zip\n";
        assert_eq!(choose_shader_in(Some(chosen), "Soft.zip"), None);
        assert_eq!(
            properties_value("Şık\\paket.zip"),
            "\\u015e\\u0131k\\\\paket.zip"
        );
    }

    #[tokio::test]
    async fn files_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        assert_eq!(
            enable_resource_packs(game, &names(&["LowFire.zip"]))
                .await
                .unwrap(),
            PackListChange::NoOptionsFile
        );
        std::fs::write(game.join(OPTIONS_FILE), b"lang:tr_tr\n").unwrap();
        assert_eq!(
            enable_resource_packs(game, &names(&["LowFire.zip"]))
                .await
                .unwrap(),
            PackListChange::Done
        );
        let written = std::fs::read_to_string(game.join(OPTIONS_FILE)).unwrap();
        assert!(written.ends_with("resourcePacks:[\"vanilla\",\"file/LowFire.zip\"]\n"));
        assert!(
            disable_resource_packs(game, &names(&["LowFire.zip"]))
                .await
                .unwrap()
        );
        assert!(choose_shader_once(game, "Soft.zip").await.unwrap());
        assert!(!choose_shader_once(game, "Other.zip").await.unwrap());
        let iris = std::fs::read_to_string(game.join(IRIS_PROPERTIES)).unwrap();
        assert!(iris.contains("shaderPack=Soft.zip"));
    }
}

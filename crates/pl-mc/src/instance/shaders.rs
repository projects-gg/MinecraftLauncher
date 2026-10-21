//! Iris's shaders, for Settings → Grafik ve FPS (SMALL7): whether the game draws a shader pack,
//! and turning it off (or back on) in one tap. Nothing but `enableShaders` in
//! `config/iris.properties` changes: the chosen pack and every other line stay exactly as the
//! player (or Iris) wrote them. Iris reads a missing `enableShaders` as on.
//!
//! A shader counts only while Iris can draw it: an enabled jar in `mods/` is Iris, a pack is
//! chosen and that pack is in `shaderpacks/`. Anything else means no shader, so the setting does
//! not show at all.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::options::split_lines;
use super::packs::IRIS_PROPERTIES;
use crate::error::{Error, Result};
use crate::mods::scan::ModScanner;
use crate::paths::write_durable;

/// The key that turns Iris's shaders on and off.
const ENABLE_KEY: &str = "enableShaders";
/// The key that names the chosen pack (its file or folder in `shaderpacks/`).
const PACK_KEY: &str = "shaderPack";
/// Iris's mod id.
const IRIS_MOD_ID: &str = "iris";

/// The shader pack Iris draws in an instance, or would draw when turned on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ShaderState {
    /// The chosen pack's file (or folder) name in `shaderpacks/`.
    pub pack: String,
    /// Iris draws it (`enableShaders` is not `false`).
    pub on: bool,
}

/// One `key=value` (or `key:value`) line of a properties file, the value still escaped; comments
/// and blank lines give nothing.
fn property(text: &[u8]) -> Option<(String, String)> {
    // Properties files are Latin-1; Iris escapes everything past ASCII anyway.
    let text: String = text.iter().map(|&byte| char::from(byte)).collect();
    let text = text.trim_start();
    if text.is_empty() || text.starts_with('#') || text.starts_with('!') {
        return None;
    }
    let split = text.find(['=', ':'])?;
    Some((
        text[..split].trim().to_owned(),
        text[split + 1..].trim().to_owned(),
    ))
}

/// A properties value as `Properties.load` reads it: `\uXXXX` (pairs too), `\t`, `\n`, `\r`, `\f`,
/// and any other escaped character as itself.
fn unescape(value: &str) -> String {
    let mut units: Vec<u16> = Vec::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            let mut buffer = [0u16; 2];
            units.extend_from_slice(c.encode_utf16(&mut buffer));
            continue;
        }
        match chars.next() {
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                match u16::from_str_radix(&hex, 16) {
                    Ok(unit) if hex.len() == 4 => units.push(unit),
                    // Not an escape after all: keep what was written.
                    _ => units.extend("\\u".encode_utf16().chain(hex.encode_utf16())),
                }
            }
            Some('t') => units.push(u16::from(b'\t')),
            Some('n') => units.push(u16::from(b'\n')),
            Some('r') => units.push(u16::from(b'\r')),
            Some('f') => units.push(0x0c),
            Some(other) => {
                let mut buffer = [0u16; 2];
                units.extend_from_slice(other.encode_utf16(&mut buffer));
            }
            None => units.push(u16::from(b'\\')),
        }
    }
    String::from_utf16_lossy(&units)
}

/// The choice in Iris's settings (`content`; `None` when the file does not exist): the chosen
/// pack, if any, and whether shaders are on. A key written twice counts with its last value, as
/// in Java.
fn read_choice(content: Option<&[u8]>) -> (Option<String>, bool) {
    let mut pack = None;
    let mut on = true;
    for (text, _) in split_lines(content.unwrap_or_default()) {
        match property(text) {
            Some((key, value)) if key == PACK_KEY => {
                let value = unescape(&value);
                pack = (!value.trim().is_empty()).then(|| value.trim().to_owned());
            }
            // As Iris reads it: anything but `false` is on.
            Some((key, value)) if key == ENABLE_KEY => on = value != "false",
            _ => {}
        }
    }
    (pack, on)
}

/// `content` with shaders turned `on` or off: every `enableShaders` line says so (one is added at
/// the end when there is none), every other byte stays. `None` when nothing changes.
fn with_shaders(content: Option<&[u8]>, on: bool) -> Option<Vec<u8>> {
    let content = content.unwrap_or_default();
    let lines = split_lines(content);
    let line = format!("{ENABLE_KEY}={on}");
    let mut found = false;
    let mut changed = false;
    let mut out = Vec::with_capacity(content.len() + line.len() + 2);
    for (text, ending) in &lines {
        match property(text) {
            Some((key, value)) if key == ENABLE_KEY => {
                found = true;
                if value != on.to_string() {
                    changed = true;
                }
                out.extend_from_slice(line.as_bytes());
            }
            _ => out.extend_from_slice(text),
        }
        out.extend_from_slice(ending);
    }
    if !found {
        let crlf = lines.iter().any(|(_, ending)| *ending == b"\r\n");
        let line_end: &[u8] = if crlf { b"\r\n" } else { b"\n" };
        if !out.is_empty() && !out.ends_with(b"\n") {
            out.extend_from_slice(line_end);
        }
        out.extend_from_slice(line.as_bytes());
        out.extend_from_slice(line_end);
        changed = true;
    }
    changed.then_some(out)
}

async fn read_settings(game_dir: &Path) -> Result<Option<Vec<u8>>> {
    let path = game_dir.join(IRIS_PROPERTIES);
    match tokio::fs::read(&path).await {
        Ok(content) => Ok(Some(content)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(Error::io(&path, error)),
    }
}

/// An enabled jar in `<game_dir>/mods` is Iris (cached scan: only new or changed jars are read).
async fn iris_installed(game_dir: &Path) -> bool {
    let mods = game_dir.join("mods");
    tokio::task::spawn_blocking(move || {
        ModScanner::shared()
            .scan_dir(&mods)
            .declaring(IRIS_MOD_ID)
            .any(|(jar, _)| jar.enabled)
    })
    .await
    .unwrap_or(false)
}

/// The pack Iris draws in the game of `game_dir`, on or off. `None` when no enabled Iris jar is
/// in `mods/`, no pack is chosen, or the chosen pack is not in `shaderpacks/` (Iris then draws
/// none either).
pub async fn shader_state(game_dir: &Path) -> Result<Option<ShaderState>> {
    let content = read_settings(game_dir).await?;
    let (Some(pack), on) = read_choice(content.as_deref()) else {
        return Ok(None);
    };
    // The pack's name comes from the player's file: only a plain name in `shaderpacks/` counts.
    let plain = Path::new(&pack)
        .file_name()
        .is_some_and(|name| name == pack.as_str());
    let present = tokio::fs::try_exists(game_dir.join("shaderpacks").join(&pack))
        .await
        .unwrap_or(false);
    if !plain || !present {
        return Ok(None);
    }
    if !iris_installed(game_dir).await {
        return Ok(None);
    }
    Ok(Some(ShaderState { pack, on }))
}

/// Turns the chosen shader pack on or off for the next start of the game. `Ok(None)`: there is no
/// shader to turn (as [`shader_state`]), and nothing was written.
pub async fn set_shaders(game_dir: &Path, on: bool) -> Result<Option<ShaderState>> {
    let Some(state) = shader_state(game_dir).await? else {
        return Ok(None);
    };
    if state.on != on {
        let content = read_settings(game_dir).await?;
        if let Some(updated) = with_shaders(content.as_deref(), on) {
            write_durable(&game_dir.join(IRIS_PROPERTIES), &updated).await?;
        }
    }
    Ok(Some(ShaderState { on, ..state }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(bytes: Vec<u8>) -> String {
        String::from_utf8(bytes).unwrap()
    }

    /// What Iris itself writes (`Properties.store`): a comment, a date and sorted keys.
    const IRIS: &str = "#This file stores configuration options for Iris, such as the currently active shaderpack\r\n#Tue Oct 21 10:12:44 TRT 2026\r\ncolorSpace=SRGB\r\ndisableUpdateMessage=false\r\nenableDebugOptions=false\r\nenableShaders=true\r\nmaxShadowRenderDistance=32\r\nshaderPack=ComplementaryReimagined_r5.4.zip\r\n";

    #[test]
    fn the_choice_is_read_as_iris_reads_it() {
        assert_eq!(
            read_choice(Some(IRIS.as_bytes())),
            (Some("ComplementaryReimagined_r5.4.zip".to_owned()), true)
        );
        // No file, no pack, or an empty one: no shader. A missing switch means on.
        assert_eq!(read_choice(None), (None, true));
        assert_eq!(read_choice(Some(b"shaderPack=\n")), (None, true));
        assert_eq!(
            read_choice(Some(b"shaderPack=BSL.zip\nenableShaders = false\n")),
            (Some("BSL.zip".to_owned()), false)
        );
        // Escapes as `Properties.store` writes them; the last value wins.
        assert_eq!(
            read_choice(Some(
                b"shaderPack=Old.zip\nshaderPack=\\u015e\\u0131k paket\\:2.zip\n"
            )),
            (Some("Şık paket:2.zip".to_owned()), true)
        );
        assert_eq!(unescape("a\\\\b\\tc\\ud83d\\ude00"), "a\\b\tc😀");
        assert_eq!(unescape("broken\\u12"), "broken\\u12");
    }

    #[test]
    fn only_the_switch_changes() {
        let off = text(with_shaders(Some(IRIS.as_bytes()), false).unwrap());
        assert_eq!(
            off,
            IRIS.replace("enableShaders=true", "enableShaders=false")
        );
        assert_eq!(with_shaders(Some(off.as_bytes()), false), None);
        let back = text(with_shaders(Some(off.as_bytes()), true).unwrap());
        assert_eq!(back, IRIS);
        // No switch yet: one is added with the file's own line ending; a duplicate is set too.
        assert_eq!(
            text(with_shaders(Some(b"shaderPack=BSL.zip"), false).unwrap()),
            "shaderPack=BSL.zip\nenableShaders=false\n"
        );
        assert_eq!(
            text(with_shaders(Some(b"a=1\r\nshaderPack=BSL.zip\r\n"), false).unwrap()),
            "a=1\r\nshaderPack=BSL.zip\r\nenableShaders=false\r\n"
        );
        assert_eq!(
            text(
                with_shaders(
                    Some(b"enableShaders=true\nx=1\nenableShaders = true\n"),
                    false
                )
                .unwrap()
            ),
            "enableShaders=false\nx=1\nenableShaders=false\n"
        );
    }

    /// A jar Fabric would load as `id`.
    fn fabric_jar(id: &str) -> Vec<u8> {
        crate::mods::scan::test_jars::fabric_jar(id, "1.8.0", "")
    }

    #[tokio::test]
    async fn a_shader_counts_only_while_iris_can_draw_it() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        std::fs::create_dir_all(game.join("config")).unwrap();
        std::fs::create_dir_all(game.join("mods")).unwrap();
        std::fs::create_dir_all(game.join("shaderpacks")).unwrap();
        std::fs::write(game.join(IRIS_PROPERTIES), IRIS).unwrap();
        // The pack is chosen but Iris is not installed.
        std::fs::write(
            game.join("shaderpacks/ComplementaryReimagined_r5.4.zip"),
            b"zip",
        )
        .unwrap();
        std::fs::write(game.join("mods/sodium.jar"), fabric_jar("sodium")).unwrap();
        assert_eq!(shader_state(game).await.unwrap(), None);
        assert_eq!(set_shaders(game, false).await.unwrap(), None);
        // Turned off in the mods folder: still nothing.
        std::fs::write(game.join("mods/iris.jar.disabled"), fabric_jar("iris")).unwrap();
        assert_eq!(shader_state(game).await.unwrap(), None);
        std::fs::rename(
            game.join("mods/iris.jar.disabled"),
            game.join("mods/iris.jar"),
        )
        .unwrap();
        let on = ShaderState {
            pack: "ComplementaryReimagined_r5.4.zip".to_owned(),
            on: true,
        };
        assert_eq!(shader_state(game).await.unwrap(), Some(on.clone()));

        // Off, then on again: only the switch moved.
        let off = ShaderState {
            on: false,
            ..on.clone()
        };
        assert_eq!(set_shaders(game, false).await.unwrap(), Some(off.clone()));
        assert_eq!(shader_state(game).await.unwrap(), Some(off));
        assert_eq!(
            std::fs::read_to_string(game.join(IRIS_PROPERTIES)).unwrap(),
            IRIS.replace("enableShaders=true", "enableShaders=false")
        );
        assert_eq!(set_shaders(game, true).await.unwrap(), Some(on));
        assert_eq!(
            std::fs::read_to_string(game.join(IRIS_PROPERTIES)).unwrap(),
            IRIS
        );

        // A pack that is gone, or a name that leaves the folder: no shader.
        std::fs::remove_file(game.join("shaderpacks/ComplementaryReimagined_r5.4.zip")).unwrap();
        assert_eq!(shader_state(game).await.unwrap(), None);
        std::fs::write(game.join("shader.zip"), b"zip").unwrap();
        std::fs::write(game.join(IRIS_PROPERTIES), "shaderPack=../shader.zip\n").unwrap();
        assert_eq!(shader_state(game).await.unwrap(), None);
        // No settings file at all: no shader.
        std::fs::remove_file(game.join(IRIS_PROPERTIES)).unwrap();
        assert_eq!(shader_state(game).await.unwrap(), None);
    }
}

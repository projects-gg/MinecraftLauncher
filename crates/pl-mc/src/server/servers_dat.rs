//! `servers.dat`: the multiplayer server list of a game directory. Uncompressed NBT
//! `{servers: [{name, ip, icon?, acceptTextures?, hidden?}]}`. Unknown tags (root level and per
//! server) are kept and written back unchanged.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use base64::Engine;
use fastnbt::Value;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::gamefiles::fsops::{blocking, write_atomic};

/// One entry of the in-game server list.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ServerListEntry {
    pub name: String,
    pub ip: String,
    /// Base64 PNG without a data-URL prefix, exactly as the game stores it.
    pub icon: Option<String>,
    /// Server resource pack choice: `None` = ask, `Some(true)` = accept, `Some(false)` = decline.
    pub accept_textures: Option<bool>,
    /// Hidden entries (added via "Direct Connection") are kept by the game but not listed.
    pub hidden: bool,
    /// Tags this launcher does not know about; preserved on write.
    #[serde(skip)]
    extra: HashMap<String, Value>,
}

impl ServerListEntry {
    pub fn new(name: impl Into<String>, ip: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ip: ip.into(),
            ..Self::default()
        }
    }

    /// Decoded icon PNG, if the stored base64 is valid.
    pub fn icon_png(&self) -> Option<Vec<u8>> {
        let icon = self.icon.as_deref()?;
        let cleaned: String = icon.chars().filter(|c| !c.is_whitespace()).collect();
        base64::engine::general_purpose::STANDARD
            .decode(cleaned)
            .ok()
    }

    /// Store a PNG as the entry icon (e.g. the favicon from a ping).
    pub fn set_icon_png(&mut self, png: Option<&[u8]>) {
        self.icon = png.map(|bytes| base64::engine::general_purpose::STANDARD.encode(bytes));
    }

    /// True if this entry carries tags the launcher does not model.
    pub fn has_unknown_fields(&self) -> bool {
        !self.extra.is_empty()
    }

    fn from_compound(mut tags: HashMap<String, Value>) -> Self {
        let name = take_string(&mut tags, "name").unwrap_or_default();
        let ip = take_string(&mut tags, "ip").unwrap_or_default();
        let icon = take_string(&mut tags, "icon").filter(|s| !s.is_empty());
        let accept_textures = take_bool(&mut tags, "acceptTextures");
        let hidden = take_bool(&mut tags, "hidden").unwrap_or(false);
        Self {
            name,
            ip,
            icon,
            accept_textures,
            hidden,
            extra: tags,
        }
    }

    fn to_compound(&self) -> Value {
        let mut tags = self.extra.clone();
        tags.insert("name".into(), Value::String(self.name.clone()));
        tags.insert("ip".into(), Value::String(self.ip.clone()));
        match &self.icon {
            Some(icon) => tags.insert("icon".into(), Value::String(icon.clone())),
            None => tags.remove("icon"),
        };
        match self.accept_textures {
            Some(accept) => tags.insert("acceptTextures".into(), Value::Byte(i8::from(accept))),
            None => tags.remove("acceptTextures"),
        };
        tags.insert("hidden".into(), Value::Byte(i8::from(self.hidden)));
        Value::Compound(tags)
    }
}

/// The whole `servers.dat` file.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ServerList {
    pub servers: Vec<ServerListEntry>,
    /// Unknown root-level tags; preserved on write.
    #[serde(skip)]
    extra: HashMap<String, Value>,
}

impl ServerList {
    /// Parse NBT bytes (gzip-compressed input is tolerated, though the game writes it raw).
    pub fn from_nbt(bytes: &[u8]) -> Result<Self> {
        let decompressed;
        let bytes = if bytes.starts_with(&[0x1f, 0x8b]) {
            let mut out = Vec::new();
            flate2::read::GzDecoder::new(bytes)
                .read_to_end(&mut out)
                .map_err(|e| Error::Invalid(format!("servers.dat: bad gzip data: {e}")))?;
            decompressed = out;
            decompressed.as_slice()
        } else {
            bytes
        };
        if bytes.is_empty() {
            return Ok(Self::default());
        }
        let root: Value =
            fastnbt::from_bytes(bytes).map_err(|e| Error::Invalid(format!("servers.dat: {e}")))?;
        let Value::Compound(mut root) = root else {
            return Err(Error::Invalid(
                "servers.dat: root tag is not a compound".into(),
            ));
        };
        let servers = match root.remove("servers") {
            Some(Value::List(items)) => items
                .into_iter()
                .filter_map(|item| match item {
                    Value::Compound(tags) => Some(ServerListEntry::from_compound(tags)),
                    _ => None,
                })
                .collect(),
            Some(other) => {
                // Not a list: keep it untouched rather than losing data.
                root.insert("servers".into(), other);
                Vec::new()
            }
            None => Vec::new(),
        };
        Ok(Self {
            servers,
            extra: root,
        })
    }

    /// Serialize to uncompressed NBT, as the game writes it.
    pub fn to_nbt(&self) -> Result<Vec<u8>> {
        let mut root = self.extra.clone();
        root.insert(
            "servers".into(),
            Value::List(
                self.servers
                    .iter()
                    .map(ServerListEntry::to_compound)
                    .collect(),
            ),
        );
        fastnbt::to_bytes(&Value::Compound(root))
            .map_err(|e| Error::Invalid(format!("servers.dat: cannot encode: {e}")))
    }

    /// Read a `servers.dat`. A missing file is an empty list.
    pub async fn read(path: &Path) -> Result<Self> {
        let path = path.to_path_buf();
        blocking(move || match std::fs::read(&path) {
            Ok(bytes) => Self::from_nbt(&bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(Error::io(&path, e)),
        })
        .await
    }

    /// Write atomically (temporary file + rename).
    pub async fn write(&self, path: &Path) -> Result<()> {
        let bytes = self.to_nbt()?;
        let path = path.to_path_buf();
        blocking(move || write_atomic(&path, &bytes)).await
    }

    /// Carry unknown tags over from `previous` (typically the file on disk) into entries that
    /// came back from the UI without them. Entries are matched by address, then by name.
    pub fn adopt_unknown_fields(&mut self, previous: &ServerList) {
        if self.extra.is_empty() {
            self.extra = previous.extra.clone();
        }
        let mut used = vec![false; previous.servers.len()];
        for entry in self.servers.iter_mut().filter(|e| e.extra.is_empty()) {
            let found = previous
                .servers
                .iter()
                .enumerate()
                .find(|(i, old)| !used[*i] && old.ip.eq_ignore_ascii_case(&entry.ip))
                .or_else(|| {
                    previous
                        .servers
                        .iter()
                        .enumerate()
                        .find(|(i, old)| !used[*i] && old.name == entry.name)
                });
            if let Some((index, old)) = found {
                used[index] = true;
                entry.extra = old.extra.clone();
            }
        }
    }
}

fn take_string(tags: &mut HashMap<String, Value>, key: &str) -> Option<String> {
    match tags.remove(key)? {
        Value::String(s) => Some(s),
        other => {
            tags.insert(key.to_owned(), other);
            None
        }
    }
}

fn take_bool(tags: &mut HashMap<String, Value>, key: &str) -> Option<bool> {
    let value = match tags.remove(key)? {
        Value::Byte(v) => i64::from(v),
        Value::Short(v) => i64::from(v),
        Value::Int(v) => i64::from(v),
        Value::Long(v) => v,
        other => {
            tags.insert(key.to_owned(), other);
            return None;
        }
    };
    Some(value != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_nbt() -> Vec<u8> {
        let mut first = HashMap::new();
        first.insert("name".to_owned(), Value::String("Projects".into()));
        first.insert("ip".to_owned(), Value::String("play.projects.gg".into()));
        first.insert("acceptTextures".to_owned(), Value::Byte(1));
        first.insert("hidden".to_owned(), Value::Byte(0));
        first.insert("futureTag".to_owned(), Value::Int(7));
        let mut second = HashMap::new();
        second.insert("name".to_owned(), Value::String("Hidden".into()));
        second.insert("ip".to_owned(), Value::String("localhost:25570".into()));
        second.insert("hidden".to_owned(), Value::Byte(1));
        second.insert("icon".to_owned(), Value::String("iVBORw0KGgo=".into()));
        let mut root = HashMap::new();
        root.insert(
            "servers".to_owned(),
            Value::List(vec![Value::Compound(first), Value::Compound(second)]),
        );
        root.insert("rootExtra".to_owned(), Value::String("keep me".into()));
        fastnbt::to_bytes(&Value::Compound(root)).unwrap()
    }

    #[test]
    fn reads_known_and_keeps_unknown_fields() {
        let list = ServerList::from_nbt(&sample_nbt()).unwrap();
        assert_eq!(list.servers.len(), 2);
        let first = &list.servers[0];
        assert_eq!(first.name, "Projects");
        assert_eq!(first.ip, "play.projects.gg");
        assert_eq!(first.accept_textures, Some(true));
        assert!(!first.hidden);
        assert_eq!(first.extra.get("futureTag"), Some(&Value::Int(7)));
        let second = &list.servers[1];
        assert!(second.hidden);
        assert_eq!(second.accept_textures, None);
        assert_eq!(second.icon_png().unwrap(), b"\x89PNG\r\n\x1a\n");
        assert_eq!(
            list.extra.get("rootExtra"),
            Some(&Value::String("keep me".into()))
        );
    }

    #[test]
    fn nbt_round_trip_is_lossless() {
        let list = ServerList::from_nbt(&sample_nbt()).unwrap();
        let again = ServerList::from_nbt(&list.to_nbt().unwrap()).unwrap();
        assert_eq!(list, again);
    }

    #[tokio::test]
    async fn file_round_trip_and_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("servers.dat");
        assert!(ServerList::read(&path).await.unwrap().servers.is_empty());

        let mut list = ServerList::from_nbt(&sample_nbt()).unwrap();
        list.servers
            .push(ServerListEntry::new("New", "mc.example.com"));
        list.servers[2].set_icon_png(Some(b"png"));
        list.write(&path).await.unwrap();
        let read = ServerList::read(&path).await.unwrap();
        assert_eq!(read, list);
        assert!(!dir.path().join("servers.dat.tmp").exists());
    }

    #[test]
    fn adopt_unknown_fields_after_ui_round_trip() {
        let on_disk = ServerList::from_nbt(&sample_nbt()).unwrap();
        // The UI only sees the serde view, which drops unknown tags.
        let json = serde_json::to_string(&on_disk).unwrap();
        let mut from_ui: ServerList = serde_json::from_str(&json).unwrap();
        assert!(!from_ui.servers[0].has_unknown_fields());
        from_ui.servers.reverse();
        from_ui.adopt_unknown_fields(&on_disk);
        let projects = from_ui
            .servers
            .iter()
            .find(|s| s.name == "Projects")
            .unwrap();
        assert_eq!(projects.extra.get("futureTag"), Some(&Value::Int(7)));
        assert!(from_ui.extra.contains_key("rootExtra"));
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        assert!(ServerList::from_nbt(b"\x0a\x00\x00\x09garbage").is_err());
        assert!(ServerList::from_nbt(&[0x1f, 0x8b, 0, 1, 2]).is_err());
        assert!(ServerList::from_nbt(&[]).unwrap().servers.is_empty());
    }
}

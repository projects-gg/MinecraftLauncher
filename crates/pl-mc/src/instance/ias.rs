//! In-Game Account Switcher (IAS, mod id [`IAS_MOD_ID`]): the launcher's username accounts join the
//! mod's own account list right before a game starts (SRV-ACC). One way and only adding: a name goes
//! to an install's list once (the names sent are remembered beside the install, so one the player
//! deleted in the game never comes back), nothing is ever removed, and Microsoft accounts never go
//! (the launcher hands the mod no session).
//!
//! The list (IAS 9.0.6, the official pack's; read from its `IASStorage` and account classes, and
//! checked against a list the mod itself saved, `testdata/ias/`) is [`ACCOUNTS_FILE`] under the game
//! folder (Fabric's game folder). On Windows the mod marks the `.hidden` folder hidden and system,
//! not the file. The file is a zlib stream (Java's `DeflaterOutputStream`) of `DataOutputStream`
//! data: a big-endian `u16` count, then per account its type and its content. Every text is Java's
//! modified UTF-8 (`writeUTF`: a `u16` byte count; NUL as `C0 80`; a character past U+FFFF as two
//! three-byte surrogates):
//!
//! - `ias:offline_v1`: the name;
//! - `ias:offline_v2`: the name, a boolean (one byte, not zero is true) and, when true, the skin's
//!   UUID as two big-endian `i64`;
//! - `ias:microsoft_v1`: a boolean, the profile's UUID (two `i64`), the name, a `u16` length and
//!   that many bytes of the encrypted session.
//!
//! The mod reads `count` accounts and ignores whatever follows; a type it does not know fails its
//! whole list (and its next save would drop every account). So only an IAS of the 9 line is
//! touched ([`known_version`]), and a list this module cannot read to its last byte (a newer mod's
//! layout, a damaged file) is left exactly as it is: nothing is written and only the reason comes
//! back, never what the list holds. A list it can read keeps every account byte for byte; the new
//! ones go at its end as `ias:offline_v1`, the name alone, which every IAS 9 reads (9.0.6 makes a
//! username account without a skin of it, as it would of `ias:offline_v2` without one), after a
//! copy of the file is kept beside it ([`BACKUP_SUFFIX`]). The new file is read back before it is
//! written, replaces the old one with a single rename and keeps its attributes. Without a list (the
//! mod saves one the first time it starts) nothing happens; the next launch adds. The game must
//! not be running.

use std::fmt;
use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use flate2::{Compression, Decompress, FlushDecompress, Status};
use serde::{Deserialize, Serialize};

use crate::mods::scan::ModSnapshot;
use crate::{Error, Result};

/// The mod's id (`fabric.mod.json`).
pub const IAS_MOD_ID: &str = "ias";

/// The mod's account list, relative to the game folder.
pub const ACCOUNTS_FILE: &str =
    "_IAS_ACCOUNTS_DO_NOT_SEND_TO_ANYONE/.hidden/accounts_v1.do_not_send_to_anyone";

/// The copy of the list kept beside it before the launcher changes it (`<list>.projects-bak`).
pub const BACKUP_SUFFIX: &str = ".projects-bak";

/// The new list while it is written, beside it; renamed over the list once complete.
const TEMP_SUFFIX: &str = ".projects-tmp";

/// The names already sent to an install's list, in its launcher folder (`<install>/.launcher/`).
const RECORD_FILE: &str = "ias-synced.json";

const OFFLINE_V1: &str = "ias:offline_v1";
const OFFLINE_V2: &str = "ias:offline_v2";
const MICROSOFT_V1: &str = "ias:microsoft_v1";

/// A list file larger than this is not read (the mod's own lists are a few hundred bytes).
const MAX_FILE_BYTES: u64 = 4 << 20;
/// Nor one that inflates past this.
const MAX_PLAIN_BYTES: usize = 16 << 20;

/// File attributes the new list takes over from the old one (Windows): hidden, system, archive,
/// not content indexed.
#[cfg(windows)]
const KEPT_ATTRIBUTES: u32 = 0x2 | 0x4 | 0x20 | 0x2000;

/// Why a list was left as it is. Says nothing about what the list holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IasProblem {
    /// Not a plain file (a folder, a link).
    NotAFile,
    /// Marked read-only: the mod could not save it either.
    ReadOnly,
    /// Larger than this module reads.
    TooLarge,
    /// Not a complete zlib stream, or its checksum does not match.
    Compression,
    /// Bytes after the zlib stream.
    TrailingData,
    /// The accounts end before the count says.
    Truncated,
    /// A text that is not Java's modified UTF-8.
    BadText,
    /// An account type this module does not know (a newer mod).
    UnknownType,
    /// Bytes after the last account.
    TrailingBytes,
    /// The list changed while it was read.
    Changed,
    /// The install's record of the names already sent could not be read.
    Record,
    /// An IAS whose list this module does not know (not the 9 line).
    UnknownVersion,
}

impl IasProblem {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotAFile => "not_a_file",
            Self::ReadOnly => "read_only",
            Self::TooLarge => "too_large",
            Self::Compression => "compression",
            Self::TrailingData => "trailing_data",
            Self::Truncated => "truncated",
            Self::BadText => "bad_text",
            Self::UnknownType => "unknown_type",
            Self::TrailingBytes => "trailing_bytes",
            Self::Changed => "changed",
            Self::Record => "record",
            Self::UnknownVersion => "unknown_version",
        }
    }
}

impl fmt::Display for IasProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What [`sync_names`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IasSync {
    /// No name to send: each went to this install before (or there is none).
    UpToDate,
    /// The mod has not saved its list in this install yet.
    NoList,
    /// How many names went into the list now (the others it had already).
    Added(usize),
    /// The list was left as it is, and why.
    LeftAlone(IasProblem),
}

/// The version of the IAS an enabled jar of the install's mods folder holds (empty when the jar
/// names none), or `None` without one.
pub fn installed(mods: &ModSnapshot) -> Option<String> {
    mods.enabled()
        .flat_map(|jar| jar.contents.all_mods())
        .find(|declared| declared.id == IAS_MOD_ID)
        .map(|declared| declared.version.clone().unwrap_or_default())
}

/// Whether this module knows the list of an IAS of `version`: the 9 line, whose storage
/// (`accounts_v1`) was checked against 9.0.6 and which reads `ias:offline_v1` throughout.
pub fn known_version(version: &str) -> bool {
    version.split(['.', '+', '-']).next() == Some("9")
}

/// Adds `names` (the launcher's username accounts, in its order) to the IAS list of the install
/// whose game folder is `game_dir` and whose IAS is `version` ([`installed`]): each name not sent
/// to this install before and not in the list in any letter case. Names that are not player names
/// (3–16 of `[A-Za-z0-9_]`) are skipped. The names sent, and those the list already had, are
/// remembered beside the install before the list changes (a list that got a name its record did
/// not could bring it back after the player deleted it). Blocking; the game must not be running.
/// An error is a file that could not be read or written: the list is then as it was.
pub fn sync_names(game_dir: &Path, version: &str, names: &[String]) -> Result<IasSync> {
    if !known_version(version) {
        return Ok(IasSync::LeftAlone(IasProblem::UnknownVersion));
    }
    let Some(record_path) = game_dir
        .parent()
        .map(|install| install.join(".launcher").join(RECORD_FILE))
    else {
        return Ok(IasSync::LeftAlone(IasProblem::Record));
    };
    let Ok(mut record) = read_record(&record_path) else {
        return Ok(IasSync::LeftAlone(IasProblem::Record));
    };
    let mut wanted: Vec<String> = Vec::new();
    for name in names {
        let new = sendable(name)
            && !record
                .names
                .iter()
                .any(|sent| sent.eq_ignore_ascii_case(name))
            && !wanted.iter().any(|w| w.eq_ignore_ascii_case(name));
        if new {
            wanted.push(name.clone());
        }
    }
    if wanted.is_empty() {
        return Ok(IasSync::UpToDate);
    }

    let path = game_dir.join(ACCOUNTS_FILE);
    let meta = match fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(IasSync::NoList),
        Err(error) => return Err(Error::io(&path, error)),
    };
    if !meta.is_file() {
        return Ok(IasSync::LeftAlone(IasProblem::NotAFile));
    }
    if meta.permissions().readonly() {
        return Ok(IasSync::LeftAlone(IasProblem::ReadOnly));
    }
    if meta.len() > MAX_FILE_BYTES {
        return Ok(IasSync::LeftAlone(IasProblem::TooLarge));
    }
    let bytes = fs::read(&path).map_err(|error| Error::io(&path, error))?;
    let plan = match plan(&bytes, &wanted) {
        Ok(plan) => plan,
        Err(problem) => return Ok(IasSync::LeftAlone(problem)),
    };
    let before = Record {
        names: record.names.clone(),
    };
    for name in plan.added.iter().chain(&plan.present) {
        record.names.push(name.to_ascii_lowercase());
    }
    record.names.sort();
    record.names.dedup();
    save_record(&record_path, &record)?;
    if let Some(new_list) = &plan.file {
        let written = write_list(&path, &meta, &bytes, new_list);
        if !matches!(written, Ok(None)) {
            // The list kept its names: so does the record (as far as it can be written).
            let _ = save_record(&record_path, &before);
            return written
                .map(|problem| IasSync::LeftAlone(problem.unwrap_or(IasProblem::Changed)));
        }
    }
    Ok(if plan.added.is_empty() {
        IasSync::UpToDate
    } else {
        IasSync::Added(plan.added.len())
    })
}

/// The backup and the new list, only when nothing touched the list since it was read (`bytes`,
/// `meta`); `Some` says why not.
fn write_list(
    path: &Path,
    meta: &fs::Metadata,
    bytes: &[u8],
    new_list: &[u8],
) -> Result<Option<IasProblem>> {
    let now = fs::symlink_metadata(path).map_err(|error| Error::io(path, error))?;
    let same = now.is_file()
        && now.len() == bytes.len() as u64
        && now.modified().ok() == meta.modified().ok();
    if !same {
        return Ok(Some(IasProblem::Changed));
    }
    write_like(&suffixed(path, BACKUP_SUFFIX), meta, bytes)?;
    write_like(path, meta, new_list)?;
    Ok(None)
}

fn save_record(path: &Path, record: &Record) -> Result<()> {
    let text = serde_json::to_vec(record).map_err(|source| Error::Json {
        what: RECORD_FILE.into(),
        source,
    })?;
    crate::paths::write_durable_blocking(path, &text)
}

/// A player name the list may get: 3–16 of `[A-Za-z0-9_]` (the launcher's own rule).
fn sendable(name: &str) -> bool {
    (3..=16).contains(&name.len())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

/// The names already sent to an install (lower case).
#[derive(Debug, Default, Serialize, Deserialize)]
struct Record {
    #[serde(default)]
    names: Vec<String>,
}

/// The install's record; none yet is empty. One that cannot be read is an error: sending again
/// could bring back a name the player deleted.
fn read_record(path: &Path) -> std::result::Result<Record, ()> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(drop),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Record::default()),
        Err(_) => Err(()),
    }
}

/// What a list becomes with `wanted` names.
#[derive(Debug)]
struct Plan {
    /// The new file; `None` when every name is in the list already.
    file: Option<Vec<u8>>,
    /// The names added, in order.
    added: Vec<String>,
    /// The names the list already had (in any letter case).
    present: Vec<String>,
}

/// Reads the list `file` to its last byte and adds each of `wanted` (player names) it does not
/// have in any letter case, after every account it has, byte for byte.
fn plan(file: &[u8], wanted: &[String]) -> std::result::Result<Plan, IasProblem> {
    let plain = inflate(file)?;
    let names = account_names(&plain)?;
    let room = usize::from(u16::MAX) - names.len();
    let mut added: Vec<String> = Vec::new();
    let mut present = Vec::new();
    for name in wanted {
        if names.iter().any(|units| same_name(units, name)) {
            present.push(name.clone());
        } else if added.len() < room && !added.iter().any(|a| a.eq_ignore_ascii_case(name)) {
            added.push(name.clone());
        }
    }
    if added.is_empty() {
        return Ok(Plan {
            file: None,
            added,
            present,
        });
    }
    let count = u16::try_from(names.len() + added.len()).map_err(|_| IasProblem::TooLarge)?;
    let mut out = Vec::with_capacity(plain.len() + added.len() * 40);
    out.extend_from_slice(&count.to_be_bytes());
    // Every account as it was: the bytes after the count are exactly the accounts (checked).
    out.extend_from_slice(&plain[2..]);
    for name in &added {
        out.extend_from_slice(&offline_account(name));
    }
    // Read back as the mod will: every account, the new ones too, and nothing else.
    let file = deflate(&out).ok_or(IasProblem::Compression)?;
    if inflate(&file)? != out || account_names(&out)?.len() != names.len() + added.len() {
        return Err(IasProblem::Compression);
    }
    Ok(Plan {
        file: Some(file),
        added,
        present,
    })
}

/// A new `ias:offline_v1` account: the name alone. `name` is a player name (ASCII).
fn offline_account(name: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(OFFLINE_V1.len() + name.len() + 4);
    for text in [OFFLINE_V1, name] {
        // ASCII only: one byte a character, never more than 65,535 of them.
        let len = u16::try_from(text.len()).unwrap_or(u16::MAX);
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(text.as_bytes());
    }
    out
}

/// The names of every account of a list's data (after inflating), or why it cannot be read to
/// its last byte.
fn account_names(plain: &[u8]) -> std::result::Result<Vec<Vec<u16>>, IasProblem> {
    let mut reader = Reader {
        bytes: plain,
        at: 0,
    };
    let count = reader.u16()?;
    let mut names = Vec::with_capacity(usize::from(count));
    for _ in 0..count {
        let kind = reader.text()?;
        let name = if is(&kind, OFFLINE_V1) {
            reader.text()?
        } else if is(&kind, OFFLINE_V2) {
            let name = reader.text()?;
            if reader.boolean()? {
                reader.take(16)?;
            }
            name
        } else if is(&kind, MICROSOFT_V1) {
            reader.boolean()?;
            reader.take(16)?;
            let name = reader.text()?;
            let data = usize::from(reader.u16()?);
            reader.take(data)?;
            name
        } else {
            return Err(IasProblem::UnknownType);
        };
        names.push(name);
    }
    if reader.at != plain.len() {
        return Err(IasProblem::TrailingBytes);
    }
    Ok(names)
}

/// Whether Java's string `units` is exactly `ascii`.
fn is(units: &[u16], ascii: &str) -> bool {
    units.len() == ascii.len()
        && units
            .iter()
            .zip(ascii.bytes())
            .all(|(unit, byte)| *unit == u16::from(byte))
}

/// Whether a name of the list is `name` (ASCII) in any letter case: servers see one player.
fn same_name(units: &[u16], name: &str) -> bool {
    units.len() == name.len()
        && units.iter().zip(name.bytes()).all(|(unit, byte)| {
            u8::try_from(*unit)
                .is_ok_and(|unit| unit.is_ascii() && unit.eq_ignore_ascii_case(&byte))
        })
}

/// A `DataInputStream` over a list's data.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> std::result::Result<&'a [u8], IasProblem> {
        let end = self
            .at
            .checked_add(len)
            .filter(|end| *end <= self.bytes.len())
            .ok_or(IasProblem::Truncated)?;
        let taken = &self.bytes[self.at..end];
        self.at = end;
        Ok(taken)
    }

    fn u16(&mut self) -> std::result::Result<u16, IasProblem> {
        let bytes = self.take(2)?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    /// `readBoolean`: one byte, not zero is true.
    fn boolean(&mut self) -> std::result::Result<bool, IasProblem> {
        Ok(self.take(1)?[0] != 0)
    }

    /// `readUTF`.
    fn text(&mut self) -> std::result::Result<Vec<u16>, IasProblem> {
        let len = usize::from(self.u16()?);
        java_utf(self.take(len)?)
    }
}

/// Java's `DataInputStream.readUTF` decoding, accepting exactly what Java accepts (a plain NUL
/// byte, overlong forms and unpaired surrogates too), into UTF-16 code units.
fn java_utf(bytes: &[u8]) -> std::result::Result<Vec<u16>, IasProblem> {
    let mut units = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let first = bytes[at];
        match first >> 4 {
            0..=7 => {
                units.push(u16::from(first));
                at += 1;
            }
            12 | 13 => {
                let second = *bytes.get(at + 1).ok_or(IasProblem::BadText)?;
                if second & 0xC0 != 0x80 {
                    return Err(IasProblem::BadText);
                }
                units.push((u16::from(first & 0x1F) << 6) | u16::from(second & 0x3F));
                at += 2;
            }
            14 => {
                let (Some(second), Some(third)) = (bytes.get(at + 1), bytes.get(at + 2)) else {
                    return Err(IasProblem::BadText);
                };
                if second & 0xC0 != 0x80 || third & 0xC0 != 0x80 {
                    return Err(IasProblem::BadText);
                }
                units.push(
                    (u16::from(first & 0x0F) << 12)
                        | (u16::from(second & 0x3F) << 6)
                        | u16::from(third & 0x3F),
                );
                at += 3;
            }
            _ => return Err(IasProblem::BadText),
        }
    }
    Ok(units)
}

/// The data of a list file: one complete zlib stream with nothing after it and a matching checksum.
fn inflate(file: &[u8]) -> std::result::Result<Vec<u8>, IasProblem> {
    let mut stream = Decompress::new(true);
    let mut plain: Vec<u8> = Vec::with_capacity(file.len().saturating_mul(4).clamp(256, 1 << 16));
    loop {
        let used = usize::try_from(stream.total_in()).map_err(|_| IasProblem::Compression)?;
        let before = (stream.total_in(), stream.total_out());
        let status = stream
            .decompress_vec(&file[used..], &mut plain, FlushDecompress::None)
            .map_err(|_| IasProblem::Compression)?;
        if status == Status::StreamEnd {
            break;
        }
        if plain.len() == plain.capacity() {
            if plain.len() >= MAX_PLAIN_BYTES {
                return Err(IasProblem::TooLarge);
            }
            let room = MAX_PLAIN_BYTES - plain.len();
            plain.reserve_exact(plain.len().max(256).min(room));
        } else if (stream.total_in(), stream.total_out()) == before {
            // No room was missing and nothing moved: the stream stops before its end.
            return Err(IasProblem::Compression);
        }
    }
    if plain.len() > MAX_PLAIN_BYTES {
        return Err(IasProblem::TooLarge);
    }
    let used = usize::try_from(stream.total_in()).map_err(|_| IasProblem::Compression)?;
    if used != file.len() {
        return Err(IasProblem::TrailingData);
    }
    // The stream ends with the data's Adler-32 (big-endian), checked here whatever the backend
    // does: Java's inflater refuses a stream whose checksum is wrong.
    if used < 4 || file[used - 4..] != adler32(&plain).to_be_bytes() {
        return Err(IasProblem::Compression);
    }
    Ok(plain)
}

fn adler32(data: &[u8]) -> u32 {
    const MOD: u32 = 65_521;
    let (mut a, mut b) = (1u32, 0u32);
    // 5,552 bytes at most between reductions keep both sums inside a u32.
    for chunk in data.chunks(5_552) {
        for &byte in chunk {
            a += u32::from(byte);
            b += a;
        }
        a %= MOD;
        b %= MOD;
    }
    (b << 16) | a
}

/// A zlib stream of `plain`, as Java's `DeflaterOutputStream` writes one (any level reads back).
fn deflate(plain: &[u8]) -> Option<Vec<u8>> {
    let mut encoder = flate2::write::ZlibEncoder::new(
        Vec::with_capacity(plain.len() / 2 + 64),
        Compression::default(),
    );
    encoder.write_all(plain).ok()?;
    encoder.finish().ok()
}

/// `path` with `suffix` after its file name.
fn suffixed(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(suffix);
    path.with_file_name(name)
}

/// Writes `bytes` to `path`: into a sibling that gets `like`'s attributes (Windows) or permissions
/// (Unix) and is flushed to disk, then renamed over `path` in one step.
fn write_like(path: &Path, like: &fs::Metadata, bytes: &[u8]) -> Result<()> {
    let temp = suffixed(path, TEMP_SUFFIX);
    // One a cut run left behind.
    let _ = fs::remove_file(&temp);
    if let Err(source) = write_then_rename(&temp, path, like, bytes) {
        let _ = fs::remove_file(&temp);
        return Err(Error::io(path, source));
    }
    if let Some(parent) = path.parent() {
        crate::paths::sync_dir(parent);
    }
    Ok(())
}

fn write_then_rename(
    temp: &Path,
    path: &Path,
    like: &fs::Metadata,
    bytes: &[u8],
) -> io::Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::{MetadataExt as _, OpenOptionsExt as _};
        options.attributes(like.file_attributes() & KEPT_ATTRIBUTES);
    }
    let mut file = options.open(temp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    #[cfg(unix)]
    fs::set_permissions(temp, like.permissions())?;
    #[cfg(not(any(windows, unix)))]
    let _ = like;
    fs::rename(temp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mods::scan::ScannedJar;
    use crate::mods::scan::test_jars::fabric_jar;

    /// The list IAS 9.0.6 itself saved (`IASStorage.save`) with four made-up accounts: `KorAtesi`
    /// (offline, no skin), `x7Ender` (Microsoft, 48 made-up session bytes), `Şimşek_😀` (offline,
    /// a skin) and `MavI_Kedi` (Microsoft, insecure, no session bytes).
    const MADE_BY_IAS: &[u8] = include_bytes!("../../testdata/ias/ias-9.0.6.bin");

    /// Java's `writeUTF`.
    fn utf(text: &str) -> Vec<u8> {
        let mut body = Vec::new();
        for unit in text.encode_utf16() {
            match unit {
                1..=0x7F => body.push(unit as u8),
                0 | 0x80..=0x7FF => {
                    body.push(0xC0 | (unit >> 6) as u8);
                    body.push(0x80 | (unit & 0x3F) as u8);
                }
                _ => {
                    body.push(0xE0 | (unit >> 12) as u8);
                    body.push(0x80 | ((unit >> 6) & 0x3F) as u8);
                    body.push(0x80 | (unit & 0x3F) as u8);
                }
            }
        }
        let mut out = (body.len() as u16).to_be_bytes().to_vec();
        out.extend(body);
        out
    }

    fn offline_v1(name: &str) -> Vec<u8> {
        [utf(OFFLINE_V1), utf(name)].concat()
    }

    fn offline_v2(name: &str, skin: Option<[u8; 16]>) -> Vec<u8> {
        let mut out = [utf(OFFLINE_V2), utf(name)].concat();
        match skin {
            Some(uuid) => {
                out.push(1);
                out.extend(uuid);
            }
            None => out.push(0),
        }
        out
    }

    fn microsoft(insecure: bool, name: &str, session: &[u8]) -> Vec<u8> {
        let mut out = utf(MICROSOFT_V1);
        out.push(u8::from(insecure));
        out.extend((0u8..16).map(|i| i.wrapping_mul(29).wrapping_add(7)));
        out.extend(utf(name));
        out.extend((session.len() as u16).to_be_bytes());
        out.extend(session);
        out
    }

    fn data(accounts: &[Vec<u8>]) -> Vec<u8> {
        let mut out = (accounts.len() as u16).to_be_bytes().to_vec();
        for account in accounts {
            out.extend(account);
        }
        out
    }

    /// A list file of `plain` (tests: deflating into memory works).
    fn zlib(plain: &[u8]) -> Vec<u8> {
        deflate(plain).unwrap()
    }

    /// The official pack's IAS.
    const KNOWN: &str = "9.0.6+26.1.2";

    fn names_of(file: &[u8]) -> Vec<String> {
        account_names(&inflate(file).unwrap())
            .unwrap()
            .iter()
            .map(|units| String::from_utf16(units).unwrap())
            .collect()
    }

    fn strings(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| (*n).to_owned()).collect()
    }

    #[test]
    fn reads_the_list_the_mod_saved() {
        assert_eq!(
            names_of(MADE_BY_IAS),
            ["KorAtesi", "x7Ender", "Şimşek_😀", "MavI_Kedi"]
        );
    }

    #[test]
    fn adds_after_every_account_byte_for_byte() {
        let before = inflate(MADE_BY_IAS).unwrap();
        let plan = plan(
            MADE_BY_IAS,
            &strings(&[
                "Yeni_1",
                "korATESI",
                "X7ENDER",
                "mavi_kedi",
                "Yeni_2",
                "yeni_1",
            ]),
        )
        .unwrap();
        assert_eq!(plan.added, ["Yeni_1", "Yeni_2"]);
        assert_eq!(plan.present, ["korATESI", "X7ENDER", "mavi_kedi"]);
        let after = inflate(&plan.file.unwrap()).unwrap();
        assert_eq!(after[..2], [0, 6]);
        // The Microsoft accounts (and every other) are the very bytes the mod wrote.
        assert_eq!(after[2..before.len()], before[2..]);
        assert_eq!(
            after[before.len()..],
            [offline_v1("Yeni_1"), offline_v1("Yeni_2")].concat()
        );
        assert_eq!(
            names_of(&zlib(&after)),
            [
                "KorAtesi",
                "x7Ender",
                "Şimşek_😀",
                "MavI_Kedi",
                "Yeni_1",
                "Yeni_2"
            ]
        );
    }

    #[test]
    fn every_type_round_trips() {
        let accounts = [
            offline_v1("Eski_Ad"),
            offline_v2("Atlas", None),
            offline_v2("Gölge\u{0}Kurt", Some([7; 16])),
            microsoft(false, "Ms_Oyuncu", &[0xAB; 300]),
            microsoft(true, "Bos_Oturum", &[]),
            offline_v2("𝔊ece_ok", None),
        ];
        let plain = data(&accounts);
        let file = zlib(&plain);
        assert_eq!(inflate(&file).unwrap(), plain);
        assert_eq!(
            names_of(&file),
            [
                "Eski_Ad",
                "Atlas",
                "Gölge\u{0}Kurt",
                "Ms_Oyuncu",
                "Bos_Oturum",
                "𝔊ece_ok"
            ]
        );
        let plan = plan(&file, &strings(&["Atlas", "Ms_oyuncu", "Ucuncu"])).unwrap();
        assert_eq!(plan.added, ["Ucuncu"]);
        let after = inflate(&plan.file.unwrap()).unwrap();
        let mut expected = accounts.to_vec();
        expected.push(offline_v1("Ucuncu"));
        assert_eq!(after, data(&expected));
    }

    #[test]
    fn a_list_with_every_name_is_not_written() {
        let plan = plan(MADE_BY_IAS, &strings(&["KORATESI", "x7ender"])).unwrap();
        assert!(plan.file.is_none() && plan.added.is_empty());
        assert_eq!(plan.present, ["KORATESI", "x7ender"]);
    }

    #[test]
    fn lists_it_cannot_read_to_the_end_are_left_alone() {
        let good = data(&[
            offline_v2("Atlas", None),
            microsoft(false, "Ms_Oyuncu", &[1, 2]),
        ]);
        let wanted = strings(&["Yeni"]);
        let problem = |plain: &[u8]| plan(&zlib(plain), &wanted).unwrap_err();

        let unknown = data(&[
            offline_v2("Atlas", None),
            [utf("ias:future_v9"), utf("X")].concat(),
        ]);
        assert_eq!(problem(&unknown), IasProblem::UnknownType);
        let mut extra = good.clone();
        extra.push(0);
        assert_eq!(problem(&extra), IasProblem::TrailingBytes);
        assert_eq!(problem(&good[..good.len() - 1]), IasProblem::Truncated);
        let mut short_count = good.clone();
        short_count[1] = 3;
        assert_eq!(problem(&short_count), IasProblem::Truncated);
        assert_eq!(problem(&[]), IasProblem::Truncated);
        // A continuation byte first, a four-byte form, and a pair cut at the end.
        for bad in [
            &[0x80u8][..],
            &[0xF0, 0x9F, 0x98, 0x80][..],
            &[b'A', 0xC3][..],
        ] {
            let mut name = (bad.len() as u16).to_be_bytes().to_vec();
            name.extend(bad);
            let text = data(&[[utf(OFFLINE_V1), name].concat()]);
            assert_eq!(problem(&text), IasProblem::BadText, "{bad:?}");
        }

        let file = zlib(&good);
        let mut after_end = file.clone();
        after_end.push(0);
        assert_eq!(
            plan(&after_end, &wanted).unwrap_err(),
            IasProblem::TrailingData
        );
        assert_eq!(
            plan(&file[..file.len() - 5], &wanted).unwrap_err(),
            IasProblem::Compression
        );
        let mut checksum = file.clone();
        *checksum.last_mut().unwrap() ^= 1;
        assert_eq!(
            plan(&checksum, &wanted).unwrap_err(),
            IasProblem::Compression
        );
        assert_eq!(plan(&[], &wanted).unwrap_err(), IasProblem::Compression);
        assert_eq!(plan(&good, &wanted).unwrap_err(), IasProblem::Compression);
    }

    #[test]
    fn texts_decode_as_java_reads_them() {
        assert_eq!(java_utf(&[0]).unwrap(), [0]);
        assert_eq!(java_utf(&[0xC0, 0x80]).unwrap(), [0]);
        // Overlong forms are read, as Java reads them.
        assert_eq!(java_utf(&[0xC1, 0x81]).unwrap(), [0x41]);
        assert_eq!(java_utf(&[0xED, 0xA0, 0xBD]).unwrap(), [0xD83D]);
        assert_eq!(java_utf(&[0xE2, 0x82]).unwrap_err(), IasProblem::BadText);
        assert_eq!(java_utf(&[0xC3, 0x41]).unwrap_err(), IasProblem::BadText);
        assert!(same_name(
            &"KorAtesi".encode_utf16().collect::<Vec<_>>(),
            "koratesi"
        ));
        // The Kelvin sign is not a K.
        assert!(!same_name(
            &"\u{212A}orAtesi".encode_utf16().collect::<Vec<_>>(),
            "KorAtesi"
        ));
    }

    #[test]
    fn a_huge_list_is_not_inflated() {
        let file = zlib(&vec![0; MAX_PLAIN_BYTES + 1]);
        assert_eq!(inflate(&file).unwrap_err(), IasProblem::TooLarge);
    }

    /// An install folder with its game folder, the list `file` in it when given.
    fn install(file: Option<&[u8]>) -> (tempfile::TempDir, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let game = root.path().join("minecraft");
        let list = game.join(ACCOUNTS_FILE);
        fs::create_dir_all(list.parent().unwrap()).unwrap();
        if let Some(file) = file {
            fs::write(&list, file).unwrap();
        }
        (root, game)
    }

    fn record_of(game: &Path) -> Vec<String> {
        let text = fs::read(game.parent().unwrap().join(".launcher").join(RECORD_FILE)).unwrap();
        serde_json::from_slice::<Record>(&text).unwrap().names
    }

    #[test]
    fn without_a_list_nothing_is_written() {
        let (_root, game) = install(None);
        assert_eq!(
            sync_names(&game, KNOWN, &strings(&["Atlas"])).unwrap(),
            IasSync::NoList
        );
        assert!(!game.join(ACCOUNTS_FILE).exists());
        assert!(!game.parent().unwrap().join(".launcher").exists());
    }

    #[test]
    fn names_go_once_and_deleted_ones_stay_deleted() {
        let (_root, game) = install(Some(MADE_BY_IAS));
        let list = game.join(ACCOUNTS_FILE);
        let names = strings(&["Atlas", "koratesi", "bad name", "ab", "Ça_1", "Gece_Kusu"]);
        assert_eq!(sync_names(&game, KNOWN, &names).unwrap(), IasSync::Added(2));
        assert_eq!(
            fs::read(suffixed(&list, BACKUP_SUFFIX)).unwrap(),
            MADE_BY_IAS
        );
        assert_eq!(
            names_of(&fs::read(&list).unwrap()),
            [
                "KorAtesi",
                "x7Ender",
                "Şimşek_😀",
                "MavI_Kedi",
                "Atlas",
                "Gece_Kusu"
            ]
        );
        assert_eq!(record_of(&game), ["atlas", "gece_kusu", "koratesi"]);
        assert!(!suffixed(&list, TEMP_SUFFIX).exists());

        // The player deletes Atlas in the game (the mod saves its list again): it stays deleted.
        let without = data(&[offline_v2("KorAtesi", None), offline_v2("Gece_Kusu", None)]);
        fs::write(&list, zlib(&without)).unwrap();
        assert_eq!(sync_names(&game, KNOWN, &names).unwrap(), IasSync::UpToDate);
        assert_eq!(inflate(&fs::read(&list).unwrap()).unwrap(), without);

        // A new launcher account goes; the old copy is replaced by the list before it.
        let more = strings(&["Atlas", "Yeni_Hesap"]);
        assert_eq!(sync_names(&game, KNOWN, &more).unwrap(), IasSync::Added(1));
        assert_eq!(
            inflate(&fs::read(suffixed(&list, BACKUP_SUFFIX)).unwrap()).unwrap(),
            without
        );
        assert_eq!(
            names_of(&fs::read(&list).unwrap()),
            ["KorAtesi", "Gece_Kusu", "Yeni_Hesap"]
        );
    }

    #[test]
    fn names_the_list_has_are_remembered_without_writing() {
        let (_root, game) = install(Some(MADE_BY_IAS));
        let list = game.join(ACCOUNTS_FILE);
        assert_eq!(
            sync_names(&game, KNOWN, &strings(&["KORATESI"])).unwrap(),
            IasSync::UpToDate
        );
        assert_eq!(fs::read(&list).unwrap(), MADE_BY_IAS);
        assert!(!suffixed(&list, BACKUP_SUFFIX).exists());
        assert_eq!(record_of(&game), ["koratesi"]);
    }

    #[test]
    fn an_unreadable_list_or_record_stops_everything() {
        let unknown = zlib(&data(&[[utf("ias:future_v9"), utf("X")].concat()]));
        let (_root, game) = install(Some(&unknown));
        let list = game.join(ACCOUNTS_FILE);
        assert_eq!(
            sync_names(&game, KNOWN, &strings(&["Atlas"])).unwrap(),
            IasSync::LeftAlone(IasProblem::UnknownType)
        );
        assert_eq!(fs::read(&list).unwrap(), unknown);
        assert!(!suffixed(&list, BACKUP_SUFFIX).exists());
        assert!(!game.parent().unwrap().join(".launcher").exists());

        let (_root, game) = install(Some(MADE_BY_IAS));
        let record = game.parent().unwrap().join(".launcher").join(RECORD_FILE);
        fs::create_dir_all(record.parent().unwrap()).unwrap();
        fs::write(&record, b"{not json").unwrap();
        assert_eq!(
            sync_names(&game, KNOWN, &strings(&["Atlas"])).unwrap(),
            IasSync::LeftAlone(IasProblem::Record)
        );
        assert_eq!(fs::read(game.join(ACCOUNTS_FILE)).unwrap(), MADE_BY_IAS);
    }

    #[test]
    fn a_read_only_list_is_left_alone() {
        let (_root, game) = install(Some(MADE_BY_IAS));
        let list = game.join(ACCOUNTS_FILE);
        let mut permissions = fs::metadata(&list).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&list, permissions.clone()).unwrap();
        assert_eq!(
            sync_names(&game, KNOWN, &strings(&["Atlas"])).unwrap(),
            IasSync::LeftAlone(IasProblem::ReadOnly)
        );
        #[allow(clippy::permissions_set_readonly_false)]
        permissions.set_readonly(false);
        fs::set_permissions(&list, permissions).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn a_hidden_list_stays_hidden() {
        use std::os::windows::fs::{MetadataExt as _, OpenOptionsExt as _};
        let (_root, game) = install(None);
        let list = game.join(ACCOUNTS_FILE);
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .attributes(0x2 | 0x4)
            .open(&list)
            .unwrap();
        file.write_all(MADE_BY_IAS).unwrap();
        drop(file);
        assert_eq!(
            sync_names(&game, KNOWN, &strings(&["Atlas"])).unwrap(),
            IasSync::Added(1)
        );
        let attributes = fs::metadata(&list).unwrap().file_attributes();
        assert_eq!(attributes & (0x2 | 0x4), 0x2 | 0x4);
        let backup = fs::metadata(suffixed(&list, BACKUP_SUFFIX)).unwrap();
        assert_eq!(backup.file_attributes() & (0x2 | 0x4), 0x2 | 0x4);
        assert_eq!(names_of(&fs::read(&list).unwrap()).last().unwrap(), "Atlas");
    }

    #[test]
    fn only_an_enabled_ias_counts() {
        let ias = fabric_jar(IAS_MOD_ID, KNOWN, "");
        let other = fabric_jar("sodium", "0.8", "");
        let snapshot = |jars: Vec<ScannedJar>| ModSnapshot::new("mods", jars);
        assert_eq!(
            installed(&snapshot(vec![
                ScannedJar::from_bytes("sodium.jar", &other),
                ScannedJar::from_bytes("IAS-9.0.6+26.1.2-fabric.jar", &ias),
            ]))
            .as_deref(),
            Some(KNOWN)
        );
        assert_eq!(
            installed(&snapshot(vec![ScannedJar::from_bytes(
                "IAS-9.0.6+26.1.2-fabric.jar.disabled",
                &ias
            )])),
            None
        );
        assert_eq!(
            installed(&snapshot(vec![ScannedJar::from_bytes(
                "sodium.jar",
                &other
            )])),
            None
        );
    }

    #[test]
    fn only_the_9_line_is_touched() {
        for known in ["9.0.6+26.1.2", "9.0.6", "9.1.0-beta.1", "9"] {
            assert!(known_version(known), "{known}");
        }
        for unknown in ["8.0.2+1.20.4", "10.0.0", "", "v9.0.6", "90.1"] {
            assert!(!known_version(unknown), "{unknown}");
        }
        let (_root, game) = install(Some(MADE_BY_IAS));
        assert_eq!(
            sync_names(&game, "10.0.0", &strings(&["Atlas"])).unwrap(),
            IasSync::LeftAlone(IasProblem::UnknownVersion)
        );
        assert_eq!(fs::read(game.join(ACCOUNTS_FILE)).unwrap(), MADE_BY_IAS);
        assert!(!game.parent().unwrap().join(".launcher").exists());
    }

    #[test]
    fn a_list_that_cannot_be_written_keeps_its_record() {
        let (_root, game) = install(Some(MADE_BY_IAS));
        let list = game.join(ACCOUNTS_FILE);
        // The new list's place is taken: its write fails after the backup.
        fs::create_dir(suffixed(&list, TEMP_SUFFIX)).unwrap();
        assert!(sync_names(&game, KNOWN, &strings(&["Atlas"])).is_err());
        assert_eq!(fs::read(&list).unwrap(), MADE_BY_IAS);
        assert!(record_of(&game).is_empty());
        // Once it can be written, the name goes.
        fs::remove_dir(suffixed(&list, TEMP_SUFFIX)).unwrap();
        assert_eq!(
            sync_names(&game, KNOWN, &strings(&["Atlas"])).unwrap(),
            IasSync::Added(1)
        );
        assert_eq!(record_of(&game), ["atlas"]);
    }
}

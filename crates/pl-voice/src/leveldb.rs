//! A small read-only LevelDB reader (VOICE-KEYS): the newest value of a few keys in a database
//! folder another program owns and may be writing (Chromium's localStorage, Discord's keybinds).
//!
//! It reads the folder's write-ahead logs (`*.log`) and tables (`*.ldb`, `*.sst`) straight into
//! memory, never writes there and never takes its lock, and ignores the manifest: every version
//! of a wanted key found anywhere counts, the highest sequence number wins (a deletion hides the
//! older values). Table blocks may be Snappy-compressed ([`snappy`]). Checksums are not verified:
//! a damaged file can only make a value unreadable, and every length is bounded, so nothing here
//! can panic or allocate without limit.

use std::collections::HashMap;
use std::path::Path;

/// Files bigger than this are skipped (Chromium's localStorage files are a few megabytes).
const FILE_MAX: u64 = 64 * 1024 * 1024;
/// A block, or a log record, decoded to at most this.
const BLOCK_MAX: usize = 16 * 1024 * 1024;
/// A log file's block.
const LOG_BLOCK: usize = 32 * 1024;
/// A table's footer: two block handles, padding, the magic number.
const FOOTER: usize = 48;
const TABLE_MAGIC: u64 = 0xdb47_7524_8b80_fb57;

/// One version of a key: its sequence number and its value (None: deleted).
type Version = (u64, Option<Vec<u8>>);

/// The newest value of each of `wanted` in the database folder `dir` (missing: absent or deleted).
pub fn newest(dir: &Path, wanted: &[&[u8]]) -> HashMap<Vec<u8>, Vec<u8>> {
    let mut found: HashMap<Vec<u8>, Version> = HashMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return HashMap::new();
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let kind = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase);
        let is_log = kind.as_deref() == Some("log");
        let is_table = matches!(kind.as_deref(), Some("ldb" | "sst"));
        // LOG and LOG.old are LevelDB's own text logs, not write-ahead logs.
        let named_log = path
            .file_stem()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.bytes().all(|b| b.is_ascii_digit()));
        if !(is_table || (is_log && named_log)) {
            continue;
        }
        if !entry.metadata().is_ok_and(|m| m.len() <= FILE_MAX) {
            continue;
        }
        // Read whole (Windows: shared read, the owner may hold it open for writing).
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let mut keep = |key: &[u8], sequence: u64, value: Option<&[u8]>| {
            if !wanted.contains(&key) {
                return;
            }
            let newer = found.get(key).is_none_or(|(seen, _)| sequence >= *seen);
            if newer {
                found.insert(key.to_vec(), (sequence, value.map(<[u8]>::to_vec)));
            }
        };
        if is_table {
            read_table(&bytes, &mut keep);
        } else {
            read_log(&bytes, &mut keep);
        }
    }
    found
        .into_iter()
        .filter_map(|(key, (_, value))| value.map(|value| (key, value)))
        .collect()
}

/// A little-endian varint (up to 64 bits) at `at`; moves `at` past it.
fn varint(bytes: &[u8], at: &mut usize) -> Option<u64> {
    let mut value = 0u64;
    for shift in (0..64).step_by(7) {
        let byte = *bytes.get(*at)?;
        *at += 1;
        value |= u64::from(byte & 0x7F) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

fn take<'a>(bytes: &'a [u8], at: &mut usize, n: usize) -> Option<&'a [u8]> {
    let end = at.checked_add(n)?;
    let slice = bytes.get(*at..end)?;
    *at = end;
    Some(slice)
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn u64_at(bytes: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(bytes.get(at..at + 8)?.try_into().ok()?))
}

/// A write-ahead log: 32 KiB blocks of records (7-byte header: checksum, length, type) that
/// carry write batches whole or in pieces.
fn read_log(bytes: &[u8], keep: &mut impl FnMut(&[u8], u64, Option<&[u8]>)) {
    let mut at = 0;
    let mut batch: Vec<u8> = Vec::new();
    while at < bytes.len() {
        let left_in_block = LOG_BLOCK - at % LOG_BLOCK;
        if left_in_block < 7 {
            at += left_in_block;
            continue;
        }
        let Some(header) = bytes.get(at..at + 7) else {
            return;
        };
        let length = usize::from(u16::from_le_bytes([header[4], header[5]]));
        let kind = header[6];
        let start = at + 7;
        let Some(data) = bytes.get(start..start + length) else {
            return;
        };
        at = start + length;
        match kind {
            // A whole batch.
            1 => {
                read_batch(data, keep);
                batch.clear();
            }
            // First, middle and last pieces.
            2 => {
                batch.clear();
                batch.extend_from_slice(data);
            }
            3 if !batch.is_empty() && batch.len() + data.len() <= BLOCK_MAX => {
                batch.extend_from_slice(data);
            }
            4 if !batch.is_empty() && batch.len() + data.len() <= BLOCK_MAX => {
                batch.extend_from_slice(data);
                read_batch(&batch, keep);
                batch.clear();
            }
            // Zeroed space (preallocated) ends what was written.
            0 => {
                at += LOG_BLOCK - at % LOG_BLOCK;
                batch.clear();
            }
            _ => batch.clear(),
        }
    }
}

/// A write batch: a sequence number, a count, then puts (tag 1, key, value) and deletions (tag 0,
/// key), each one sequence number after the last.
fn read_batch(data: &[u8], keep: &mut impl FnMut(&[u8], u64, Option<&[u8]>)) {
    let (Some(sequence), Some(count)) = (u64_at(data, 0), u32_at(data, 8)) else {
        return;
    };
    let mut at = 12;
    for i in 0..u64::from(count) {
        let Some(&tag) = data.get(at) else {
            return;
        };
        at += 1;
        let Some(key) = varint(data, &mut at).and_then(|n| take(data, &mut at, n as usize)) else {
            return;
        };
        let sequence = sequence.saturating_add(i);
        match tag {
            1 => {
                let Some(value) =
                    varint(data, &mut at).and_then(|n| take(data, &mut at, n as usize))
                else {
                    return;
                };
                keep(key, sequence, Some(value));
            }
            0 => keep(key, sequence, None),
            _ => return,
        }
    }
}

/// A block handle: offset and size, two varints.
fn handle(bytes: &[u8], at: &mut usize) -> Option<(usize, usize)> {
    let offset = usize::try_from(varint(bytes, at)?).ok()?;
    let size = usize::try_from(varint(bytes, at)?).ok()?;
    Some((offset, size))
}

/// A table block's contents, decompressed (its 5-byte trailer: compression type, checksum).
fn block(bytes: &[u8], (offset, size): (usize, usize)) -> Option<Vec<u8>> {
    if size > BLOCK_MAX {
        return None;
    }
    let raw = bytes.get(offset..offset.checked_add(size)?)?;
    match bytes.get(offset + size)? {
        0 => Some(raw.to_vec()),
        1 => snappy::decompress(raw, BLOCK_MAX),
        _ => None,
    }
}

/// A block's entries (keys share their start with the one before), in order.
fn entries(block: &[u8], mut each: impl FnMut(&[u8], &[u8])) {
    let Some(restarts) = block.len().checked_sub(4).and_then(|at| u32_at(block, at)) else {
        return;
    };
    let Some(end) = (restarts as usize)
        .checked_mul(4)
        .and_then(|n| block.len().checked_sub(4 + n))
    else {
        return;
    };
    let data = &block[..end];
    let mut at = 0;
    let mut key: Vec<u8> = Vec::new();
    while at < data.len() {
        let (Some(shared), Some(unshared), Some(length)) = (
            varint(data, &mut at),
            varint(data, &mut at),
            varint(data, &mut at),
        ) else {
            return;
        };
        let Some(delta) = take(data, &mut at, unshared as usize) else {
            return;
        };
        let Some(value) = take(data, &mut at, length as usize) else {
            return;
        };
        if shared as usize > key.len() {
            return;
        }
        key.truncate(shared as usize);
        key.extend_from_slice(delta);
        each(&key, value);
    }
}

/// A table: the footer names the index block, whose values name the data blocks; data keys are
/// internal keys (the user's key, then 8 bytes: sequence << 8 | type, 1 a value, 0 a deletion).
fn read_table(bytes: &[u8], keep: &mut impl FnMut(&[u8], u64, Option<&[u8]>)) {
    let Some(footer_at) = bytes.len().checked_sub(FOOTER) else {
        return;
    };
    if u64_at(bytes, bytes.len() - 8) != Some(TABLE_MAGIC) {
        return;
    }
    let mut at = footer_at;
    let (Some(_meta), Some(index)) = (handle(bytes, &mut at), handle(bytes, &mut at)) else {
        return;
    };
    let Some(index) = block(bytes, index) else {
        return;
    };
    entries(&index, |_, value| {
        let mut at = 0;
        let Some(data) = handle(value, &mut at).and_then(|h| block(bytes, h)) else {
            return;
        };
        entries(&data, |internal, value| {
            let Some(split) = internal.len().checked_sub(8) else {
                return;
            };
            let (key, tag) = internal.split_at(split);
            let Some(tag) = u64_at(tag, 0) else {
                return;
            };
            match tag & 0xFF {
                1 => keep(key, tag >> 8, Some(value)),
                0 => keep(key, tag >> 8, None),
                _ => {}
            }
        });
    });
}

/// Snappy's raw format (Chromium's LevelDB compresses table blocks with it), decoded with every
/// length and offset checked.
pub mod snappy {
    /// `data` decoded, or None when it is damaged or would pass `max` bytes.
    pub fn decompress(data: &[u8], max: usize) -> Option<Vec<u8>> {
        let mut at = 0;
        let length = usize::try_from(super::varint(data, &mut at)?).ok()?;
        if length > max {
            return None;
        }
        let mut out = Vec::with_capacity(length);
        while at < data.len() {
            let tag = data[at];
            at += 1;
            match tag & 0b11 {
                0 => {
                    let mut len = usize::from(tag >> 2);
                    if len >= 60 {
                        let extra = len - 59;
                        let bytes = super::take(data, &mut at, extra)?;
                        len = bytes
                            .iter()
                            .rev()
                            .fold(0usize, |acc, b| (acc << 8) | usize::from(*b));
                    }
                    let literal = super::take(data, &mut at, len.checked_add(1)?)?;
                    out.extend_from_slice(literal);
                }
                kind => {
                    let (len, offset) = match kind {
                        1 => {
                            let low = usize::from(*data.get(at)?);
                            at += 1;
                            (
                                4 + usize::from((tag >> 2) & 0b111),
                                (usize::from(tag >> 5) << 8) | low,
                            )
                        }
                        2 => {
                            let b = super::take(data, &mut at, 2)?;
                            (
                                1 + usize::from(tag >> 2),
                                usize::from(u16::from_le_bytes([b[0], b[1]])),
                            )
                        }
                        _ => {
                            let b = super::take(data, &mut at, 4)?;
                            let offset = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
                            (1 + usize::from(tag >> 2), usize::try_from(offset).ok()?)
                        }
                    };
                    if offset == 0 || offset > out.len() {
                        return None;
                    }
                    // A copy may overlap what it writes (a run).
                    let from = out.len() - offset;
                    for i in 0..len {
                        let byte = out[from + i];
                        out.push(byte);
                    }
                }
            }
            if out.len() > length {
                return None;
            }
        }
        (out.len() == length).then_some(out)
    }
}

#[cfg(test)]
pub(crate) mod fixture {
    //! Builds LevelDB files from made-up entries for the tests (a write-ahead log and a table
    //! with a Snappy-compressed block), as LevelDB lays them out.

    fn varint(mut value: u64, out: &mut Vec<u8>) {
        while value >= 0x80 {
            out.push((value as u8) | 0x80);
            value >>= 7;
        }
        out.push(value as u8);
    }

    /// A log holding one batch of puts (None: a deletion), its first sequence number `sequence`.
    pub fn log(sequence: u64, writes: &[(&[u8], Option<&[u8]>)]) -> Vec<u8> {
        let mut batch = Vec::new();
        batch.extend_from_slice(&sequence.to_le_bytes());
        batch.extend_from_slice(&(writes.len() as u32).to_le_bytes());
        for (key, value) in writes {
            batch.push(u8::from(value.is_some()));
            varint(key.len() as u64, &mut batch);
            batch.extend_from_slice(key);
            if let Some(value) = value {
                varint(value.len() as u64, &mut batch);
                batch.extend_from_slice(value);
            }
        }
        // Split into records so a batch spans a block boundary when it is long (FIRST, MIDDLE,
        // LAST), as LevelDB writes it.
        let mut out = Vec::new();
        let mut rest = &batch[..];
        let mut first = true;
        loop {
            let room = super::LOG_BLOCK - out.len() % super::LOG_BLOCK;
            if room < 7 {
                out.resize(out.len() + room, 0);
                continue;
            }
            let n = rest.len().min(room - 7);
            let last = n == rest.len();
            let kind = match (first, last) {
                (true, true) => 1,
                (true, false) => 2,
                (false, false) => 3,
                (false, true) => 4,
            };
            out.extend_from_slice(&[0, 0, 0, 0]);
            out.extend_from_slice(&(n as u16).to_le_bytes());
            out.push(kind);
            out.extend_from_slice(&rest[..n]);
            rest = &rest[n..];
            first = false;
            if last {
                return out;
            }
        }
    }

    /// Snappy for tests: literals, with a copy wherever the next 4+ bytes repeat earlier ones.
    pub fn snappy(data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        varint(data.len() as u64, &mut out);
        let mut literal: Vec<u8> = Vec::new();
        let flush = |literal: &mut Vec<u8>, out: &mut Vec<u8>| {
            for chunk in literal.chunks(60) {
                out.push(((chunk.len() - 1) as u8) << 2);
                out.extend_from_slice(chunk);
            }
            literal.clear();
        };
        let mut at = 0;
        while at < data.len() {
            let found = (1..=at.min(2000)).find_map(|offset| {
                let len = (0..64.min(data.len() - at))
                    .take_while(|i| data[at + i] == data[at - offset + i])
                    .count();
                (len >= 4).then_some((offset, len))
            });
            if let Some((offset, len)) = found {
                flush(&mut literal, &mut out);
                // A copy with a 2-byte offset.
                out.push((((len - 1) as u8) << 2) | 0b10);
                out.extend_from_slice(&(offset as u16).to_le_bytes());
                at += len;
            } else {
                literal.push(data[at]);
                at += 1;
            }
        }
        flush(&mut literal, &mut out);
        out
    }

    /// One table entry: the user's key, its sequence number, its value (None: a deletion).
    pub type Entry<'a> = (&'a [u8], u64, Option<&'a [u8]>);

    /// A table of one data block (Snappy-compressed) with `entries`, sorted by key as LevelDB
    /// keeps them.
    pub fn table(entries: &[Entry<'_>]) -> Vec<u8> {
        let mut block = Vec::new();
        let mut last: Vec<u8> = Vec::new();
        for (key, sequence, value) in entries {
            let mut internal = key.to_vec();
            let tag = (sequence << 8) | u64::from(value.is_some());
            internal.extend_from_slice(&tag.to_le_bytes());
            let shared = last
                .iter()
                .zip(&internal)
                .take_while(|(a, b)| a == b)
                .count();
            let value = value.unwrap_or(&[]);
            varint(shared as u64, &mut block);
            varint((internal.len() - shared) as u64, &mut block);
            varint(value.len() as u64, &mut block);
            block.extend_from_slice(&internal[shared..]);
            block.extend_from_slice(value);
            last = internal;
        }
        block.extend_from_slice(&0u32.to_le_bytes());
        block.extend_from_slice(&1u32.to_le_bytes());
        let compressed = snappy(&block);
        let mut out = Vec::new();
        out.extend_from_slice(&compressed);
        out.push(1);
        out.extend_from_slice(&[0, 0, 0, 0]);
        // The index: one entry naming the data block.
        let index_at = out.len();
        let mut handle = Vec::new();
        varint(0, &mut handle);
        varint(compressed.len() as u64, &mut handle);
        let mut index = Vec::new();
        varint(0, &mut index);
        varint(last.len() as u64, &mut index);
        varint(handle.len() as u64, &mut index);
        index.extend_from_slice(&last);
        index.extend_from_slice(&handle);
        index.extend_from_slice(&0u32.to_le_bytes());
        index.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&index);
        out.push(0);
        out.extend_from_slice(&[0, 0, 0, 0]);
        // The footer: an empty meta index, the index, padding, the magic number.
        let mut footer = Vec::new();
        varint(index_at as u64, &mut footer);
        varint(0, &mut footer);
        varint(index_at as u64, &mut footer);
        varint(index.len() as u64, &mut footer);
        footer.resize(40, 0);
        footer.extend_from_slice(&super::TABLE_MAGIC.to_le_bytes());
        out.extend_from_slice(&footer);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snappy_round_trips_literals_and_overlapping_copies() {
        let text = b"keybinds keybinds keybinds aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa!".repeat(3);
        let packed = fixture::snappy(&text);
        assert!(packed.len() < text.len());
        assert_eq!(
            snappy::decompress(&packed, 1 << 20).as_deref(),
            Some(&text[..])
        );
        // Damaged: an offset before the start, a length past the limit, a cut stream.
        assert_eq!(snappy::decompress(&[5, 0b0000_0110, 9, 0], 1 << 20), None);
        assert_eq!(snappy::decompress(&packed, 10), None);
        assert_eq!(
            snappy::decompress(&packed[..packed.len() - 3], 1 << 20),
            None
        );
        assert_eq!(snappy::decompress(&[], 1 << 20), None);
    }

    #[test]
    fn the_newest_version_wins_across_logs_and_tables() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path();
        // An old table, a newer log, a log that deletes one key.
        std::fs::write(
            path.join("000005.ldb"),
            fixture::table(&[
                (b"a", 3, Some(b"old a")),
                (b"b", 4, Some(b"old b")),
                (b"c", 5, Some(b"c")),
            ]),
        )
        .unwrap();
        std::fs::write(
            path.join("000007.log"),
            fixture::log(10, &[(b"a", Some(b"new a")), (b"z", Some(b"z"))]),
        )
        .unwrap();
        std::fs::write(path.join("000009.log"), fixture::log(20, &[(b"b", None)])).unwrap();
        // LevelDB's text log and the lock are not data.
        std::fs::write(path.join("LOG"), b"2026/11/02 opened").unwrap();
        std::fs::write(path.join("LOCK"), b"").unwrap();
        let found = newest(path, &[b"a", b"b", b"c", b"missing"]);
        assert_eq!(found.get(&b"a"[..]).map(Vec::as_slice), Some(&b"new a"[..]));
        assert_eq!(found.get(&b"b"[..]), None);
        assert_eq!(found.get(&b"c"[..]).map(Vec::as_slice), Some(&b"c"[..]));
        // Not asked for: not kept.
        assert_eq!(found.get(&b"z"[..]), None);
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn a_batch_split_across_log_blocks_reads_whole() {
        let dir = tempfile::tempdir().unwrap();
        let big = vec![b'x'; 70_000];
        std::fs::write(
            dir.path().join("000003.log"),
            fixture::log(1, &[(b"big", Some(&big)), (b"small", Some(b"s"))]),
        )
        .unwrap();
        let found = newest(dir.path(), &[b"big", b"small"]);
        assert_eq!(found.get(&b"big"[..]).map(Vec::len), Some(70_000));
        assert_eq!(found.get(&b"small"[..]).map(Vec::as_slice), Some(&b"s"[..]));
    }

    #[test]
    fn damaged_files_read_as_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let mut table = fixture::table(&[(b"a", 1, Some(b"1"))]);
        // A table with its block cut short, and garbage of every length.
        let len = table.len();
        table[len - 60] ^= 0xFF;
        std::fs::write(dir.path().join("000001.ldb"), &table).unwrap();
        for n in 0..64 {
            std::fs::write(
                dir.path().join(format!("{:06}.log", n + 10)),
                vec![0x5A; n * 7],
            )
            .unwrap();
        }
        // Nothing panics; whatever survives is only what was written.
        let found = newest(dir.path(), &[b"a"]);
        assert!(found.get(&b"a"[..]).is_none_or(|v| v == b"1"));
        assert!(newest(&dir.path().join("missing"), &[b"a"]).is_empty());
    }
}

//! What the panel saw of an account's audit log ("Akış"), kept on this computer for 7 days (README
//! "AFK+ player API" → Audit log: each device keeps what it saw; the machine keeps the log itself 7
//! days and D1 never). When the machine cannot be asked, the panel shows these. One folder per
//! account under `<data>/cache/afkplus/`: `today.jsonl`, the newest AFK+ day's entries as plain
//! JSON lines (they still grow), and `<day>.jsonl.z` for each closed day, its lines in one raw
//! deflate block (closed days are packed when they close: CLAUDE.md "Veritabanı verimliliği"). At
//! most 2 MB in all: the oldest days go first, then the oldest entries of the newest day.
//!
//! The days are counted from this computer's clock (an entry's day is its sequence's: the AFK+ day,
//! the UTC date of `t − 3600`), so an account no longer looked at ages out too: whatever is older
//! than the window goes at the next keep or read. One writer at a time: a page and a live batch are
//! kept from separate blocking threads, and a read never sees a file half replaced.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use flate2::Compression;
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use parking_lot::Mutex;

use crate::plus::PlusAuditEntry;

/// Days kept: the newest one and the six before it.
pub const AUDIT_KEEP_DAYS: i64 = 7;
/// Bytes kept per account, all files together.
pub const AUDIT_MAX_BYTES: u64 = 2 * 1024 * 1024;
const TODAY: &str = "today.jsonl";
const PACKED: &str = ".jsonl.z";

/// Every account's files, one writer or reader at a time.
static STORE: Mutex<()> = parking_lot::const_mutex(());

/// The AFK+ day of a moment (Unix seconds): days since 1970-01-01 of `t − 3600` (04:00 Turkey time).
pub fn plus_day(unix_seconds: i64) -> i64 {
    (unix_seconds - 3_600).div_euclid(86_400)
}

/// The days kept: the newest (today, or the day after it when this computer's clock is a little
/// behind the machine's) and the six before it. A day further ahead (a clock far off) is not kept
/// and moves nothing.
fn window(today: i64, days: impl Iterator<Item = i64>) -> (i64, i64) {
    let newest = days.filter(|day| *day <= today + 1).fold(today, i64::max);
    (newest, newest - (AUDIT_KEEP_DAYS - 1))
}

/// The account's folder, refusing ids that could leave the cache.
fn folder(root: &Path, acct: &str) -> Option<PathBuf> {
    let safe = !acct.is_empty()
        && acct.len() <= 32
        && acct
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    safe.then(|| root.join(acct))
}

fn to_lines(entries: &[&PlusAuditEntry]) -> Vec<u8> {
    let mut out = Vec::new();
    for entry in entries {
        if let Ok(line) = serde_json::to_vec(entry) {
            out.extend_from_slice(&line);
            out.push(b'\n');
        }
    }
    out
}

fn from_lines(bytes: &[u8]) -> Vec<PlusAuditEntry> {
    bytes
        .split(|b| *b == b'\n')
        .filter(|line| !line.is_empty())
        .filter_map(|line| serde_json::from_slice(line).ok())
        .collect()
}

fn pack(bytes: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(bytes)?;
    encoder.finish()
}

fn unpack(bytes: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut out = Vec::new();
    // A broken block (a crash mid-write) reads as nothing, never as a panic or a huge buffer.
    DeflateDecoder::new(bytes)
        .take(AUDIT_MAX_BYTES * 8)
        .read_to_end(&mut out)?;
    Ok(out)
}

fn read_packed(path: &Path) -> Vec<PlusAuditEntry> {
    std::fs::read(path).map_or_else(
        |_| Vec::new(),
        |bytes| from_lines(&unpack(&bytes).unwrap_or_default()),
    )
}

/// The closed days on disk: day number → file.
fn packed_days(dir: &Path) -> BTreeMap<i64, PathBuf> {
    let mut out = BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(day) = name
            .strip_suffix(PACKED)
            .and_then(|day| day.parse::<i64>().ok())
        {
            out.insert(day, entry.path());
        }
    }
    out
}

fn write_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let temp = path.with_extension("part");
    std::fs::write(&temp, bytes)?;
    std::fs::rename(&temp, path)
}

fn size_of(path: &Path) -> u64 {
    std::fs::metadata(path).map_or(0, |m| m.len())
}

/// Keeps entries the panel received (a page or the live stream): merged by their sequence, the
/// newest day plain, older days packed, then the days and the size trimmed. `today`: this AFK+
/// day ([`plus_day`] of the clock).
pub fn keep(
    root: &Path,
    acct: &str,
    entries: &[PlusAuditEntry],
    today: i64,
) -> std::io::Result<()> {
    let Some(dir) = folder(root, acct) else {
        return Ok(());
    };
    if entries.is_empty() {
        return Ok(());
    }
    let _one = STORE.lock();
    std::fs::create_dir_all(&dir)?;
    let today_path = dir.join(TODAY);
    let current = std::fs::read(&today_path).map_or_else(|_| Vec::new(), |b| from_lines(&b));
    let packed = packed_days(&dir);
    let days = current
        .iter()
        .chain(entries)
        .map(PlusAuditEntry::day)
        .chain(packed.keys().copied());
    let (newest, oldest_kept) = window(today, days);
    let kept = |day: i64| (oldest_kept..=newest).contains(&day);

    let mut by_day: BTreeMap<i64, BTreeMap<i64, PlusAuditEntry>> = BTreeMap::new();
    for entry in current.into_iter().chain(entries.iter().cloned()) {
        if kept(entry.day()) {
            by_day
                .entry(entry.day())
                .or_default()
                .insert(entry.s, entry);
        }
    }
    // The newest day is the plain one: a packed copy of it (its plain file lost, or packed while
    // the clock said another day) joins it before the file goes below.
    if let Some(path) = packed.get(&newest) {
        let lines = by_day.entry(newest).or_default();
        for entry in read_packed(path) {
            lines.entry(entry.s).or_insert(entry);
        }
    }

    // Days other than the newest go into their packed file (with what it held).
    for (day, lines) in &by_day {
        if *day == newest {
            continue;
        }
        let path = dir.join(format!("{day}{PACKED}"));
        let mut merged: BTreeMap<i64, PlusAuditEntry> = BTreeMap::new();
        if let Some(existing) = packed.get(day) {
            for entry in read_packed(existing) {
                merged.insert(entry.s, entry);
            }
        }
        let before = merged.len();
        merged.extend(lines.iter().map(|(s, entry)| (*s, entry.clone())));
        if merged.len() != before || !packed.contains_key(day) {
            let list: Vec<&PlusAuditEntry> = merged.values().collect();
            write_file(&path, &pack(&to_lines(&list))?)?;
        }
    }
    let newest_lines: Vec<&PlusAuditEntry> = by_day
        .get(&newest)
        .map(|lines| lines.values().collect())
        .unwrap_or_default();
    write_file(&today_path, &to_lines(&newest_lines))?;

    // Seven days, then 2 MB: the oldest days first, then the newest day's oldest entries.
    for (day, path) in packed_days(&dir) {
        if !kept(day) || day == newest {
            let _ = std::fs::remove_file(path);
        }
    }
    let mut days = packed_days(&dir);
    let total = |days: &BTreeMap<i64, PathBuf>| {
        size_of(&today_path) + days.values().map(|p| size_of(p)).sum::<u64>()
    };
    while total(&days) > AUDIT_MAX_BYTES {
        let Some((&day, path)) = days.iter().next() else {
            break;
        };
        let _ = std::fs::remove_file(path);
        days.remove(&day);
    }
    if size_of(&today_path) > AUDIT_MAX_BYTES {
        let mut list = newest_lines;
        let mut bytes = to_lines(&list);
        while bytes.len() as u64 > AUDIT_MAX_BYTES && !list.is_empty() {
            let cut = (list.len() / 10).max(1);
            list.drain(..cut);
            bytes = to_lines(&list);
        }
        write_file(&today_path, &bytes)?;
    }
    Ok(())
}

/// Everything kept for an account, newest first; what aged out of the window goes from the disk.
pub fn read(root: &Path, acct: &str, today: i64) -> Vec<PlusAuditEntry> {
    let Some(dir) = folder(root, acct) else {
        return Vec::new();
    };
    let _one = STORE.lock();
    let packed = packed_days(&dir);
    let today_path = dir.join(TODAY);
    let plain = std::fs::read(&today_path).map_or_else(|_| Vec::new(), |b| from_lines(&b));
    let days = plain
        .iter()
        .map(PlusAuditEntry::day)
        .chain(packed.keys().copied());
    let (newest, oldest_kept) = window(today, days);
    let kept = |day: i64| (oldest_kept..=newest).contains(&day);
    let mut all: BTreeMap<i64, PlusAuditEntry> = BTreeMap::new();
    for (day, path) in &packed {
        if !kept(*day) {
            let _ = std::fs::remove_file(path);
            continue;
        }
        for entry in read_packed(path) {
            all.insert(entry.s, entry);
        }
    }
    let had_plain = !plain.is_empty();
    let mut plain_kept = false;
    for entry in plain {
        if kept(entry.day()) {
            plain_kept = true;
            all.insert(entry.s, entry);
        }
    }
    if had_plain && !plain_kept {
        let _ = std::fs::remove_file(&today_path);
    }
    if all.is_empty() {
        // Nothing left: the folder goes too (only when it is empty).
        let _ = std::fs::remove_file(&today_path);
        let _ = std::fs::remove_dir(&dir);
    }
    all.into_values().rev().collect()
}

/// Drops what was kept: one account's (it left AFK+), or every account's but those named (a
/// sign-out keeps none: `keep_only` empty).
pub fn forget(root: &Path, keep_only: &[&str]) {
    let _one = STORE.lock();
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !keep_only.contains(&name.as_str()) && folder(root, &name).is_some() {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// Where the audit logs are kept (`<data>/cache/afkplus`).
pub fn audit_dir(cache_root: &Path) -> PathBuf {
    cache_root.join("afkplus")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plus::AUDIT_DAY;

    fn entry(day: i64, n: i64, kind: &str) -> PlusAuditEntry {
        PlusAuditEntry {
            s: day * AUDIT_DAY + n,
            t: (day * 86_400 + n) * 1000,
            k: kind.into(),
            x: (kind == "pm_in").then(|| "elytra var mı".to_owned()),
            ..PlusAuditEntry::default()
        }
    }

    #[test]
    fn the_afk_plus_day_starts_at_four_in_turkey() {
        // 2026-10-26 00:59:59 UTC is still the 25th's AFK+ day; 01:00 starts the 26th's.
        let day_26 = 20_752;
        assert_eq!(plus_day(day_26 * 86_400 + 3_599), day_26 - 1);
        assert_eq!(plus_day(day_26 * 86_400 + 3_600), day_26);
    }

    #[test]
    fn keeps_merges_and_packs_closed_days() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        keep(
            root,
            "a_1",
            &[entry(100, 1, "pm_in"), entry(100, 2, "reply")],
            100,
        )
        .unwrap();
        // The same entries again (a page read twice) change nothing.
        keep(root, "a_1", &[entry(100, 2, "reply")], 100).unwrap();
        assert_eq!(read(root, "a_1", 100).len(), 2);
        // The next day: the first day is packed, the new one plain.
        keep(root, "a_1", &[entry(101, 1, "offer")], 101).unwrap();
        let account = root.join("a_1");
        assert!(account.join("100.jsonl.z").exists());
        let today = std::fs::read_to_string(account.join(TODAY)).unwrap();
        assert_eq!(today.lines().count(), 1);
        let all = read(root, "a_1", 101);
        assert_eq!(
            all.iter().map(|e| e.s).collect::<Vec<_>>(),
            vec![
                101 * AUDIT_DAY + 1,
                100 * AUDIT_DAY + 2,
                100 * AUDIT_DAY + 1
            ],
            "newest first"
        );
        assert_eq!(all[2].x.as_deref(), Some("elytra var mı"));
        // A late entry of a closed day joins its packed file.
        keep(root, "a_1", &[entry(100, 3, "agree")], 101).unwrap();
        assert_eq!(read(root, "a_1", 101).len(), 4);
        assert_eq!(
            std::fs::read_to_string(account.join(TODAY))
                .unwrap()
                .lines()
                .count(),
            1
        );
    }

    #[test]
    fn seven_days_and_two_megabytes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for day in 100..110 {
            keep(root, "a_1", &[entry(day, 1, "offer")], day).unwrap();
        }
        let days: Vec<i64> = read(root, "a_1", 109)
            .iter()
            .map(PlusAuditEntry::day)
            .collect();
        assert_eq!(days, vec![109, 108, 107, 106, 105, 104, 103]);
        // One day far past the cap: its oldest entries go.
        let big: Vec<PlusAuditEntry> = (0..9_000)
            .map(|n| PlusAuditEntry {
                x: Some("x".repeat(300)),
                ..entry(110, n, "pm_in")
            })
            .collect();
        keep(root, "a_1", &big, 110).unwrap();
        let account = root.join("a_1");
        let total: u64 = std::fs::read_dir(&account)
            .unwrap()
            .map(|e| e.unwrap().metadata().unwrap().len())
            .sum();
        assert!(total <= AUDIT_MAX_BYTES, "{total}");
        let kept = read(root, "a_1", 110);
        assert_eq!(kept[0].s, 110 * AUDIT_DAY + 8_999, "the newest stays");
    }

    #[test]
    fn ages_by_the_clock_and_ignores_a_far_future_entry() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        keep(root, "a_1", &[entry(100, 1, "offer")], 100).unwrap();
        keep(root, "a_1", &[entry(101, 1, "offer")], 101).unwrap();
        // Nothing new for a week: reading on day 107 drops day 100, day 108 everything, and the
        // folder goes with the last file.
        assert_eq!(read(root, "a_1", 107).len(), 1);
        assert!(!root.join("a_1").join("100.jsonl.z").exists());
        assert!(read(root, "a_1", 108).is_empty());
        assert!(!root.join("a_1").exists());
        // An entry dated far ahead (a clock off by days) is not kept and moves nothing.
        keep(root, "a_2", &[entry(200, 1, "offer")], 200).unwrap();
        keep(root, "a_2", &[entry(900, 1, "offer")], 200).unwrap();
        let days: Vec<i64> = read(root, "a_2", 200)
            .iter()
            .map(PlusAuditEntry::day)
            .collect();
        assert_eq!(days, vec![200]);
        // A clock a day behind the machine's still keeps the machine's today plain.
        keep(root, "a_3", &[entry(301, 1, "offer")], 300).unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("a_3").join(TODAY))
                .unwrap()
                .lines()
                .count(),
            1
        );
    }

    #[test]
    fn a_lost_plain_file_never_takes_its_day_with_it() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        keep(root, "a_1", &[entry(100, 1, "offer")], 100).unwrap();
        // The clock went back a day after the entry was packed as a closed day.
        keep(root, "a_1", &[entry(101, 1, "offer")], 101).unwrap();
        std::fs::remove_file(root.join("a_1").join(TODAY)).unwrap();
        std::fs::write(
            root.join("a_1").join("101.jsonl.z"),
            pack(&to_lines(&[&entry(101, 1, "offer")])).unwrap(),
        )
        .unwrap();
        keep(root, "a_1", &[entry(101, 2, "agree")], 101).unwrap();
        let all: Vec<i64> = read(root, "a_1", 101).iter().map(|e| e.s).collect();
        assert_eq!(
            all,
            vec![
                101 * AUDIT_DAY + 2,
                101 * AUDIT_DAY + 1,
                100 * AUDIT_DAY + 1
            ]
        );
    }

    #[test]
    fn two_writers_lose_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let threads: Vec<_> = (0..8)
            .map(|n| {
                let root = root.clone();
                std::thread::spawn(move || {
                    for i in 0..20 {
                        keep(&root, "a_1", &[entry(100, n * 100 + i, "offer")], 100).unwrap();
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        assert_eq!(read(&root, "a_1", 100).len(), 160);
    }

    #[test]
    fn stays_in_its_folder_and_forgets() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        keep(root, "../x", &[entry(1, 1, "offer")], 1).unwrap();
        assert!(!dir.path().parent().unwrap().join("x").exists());
        keep(root, "a_1", &[entry(1, 1, "offer")], 1).unwrap();
        keep(root, "a_2", &[entry(1, 1, "offer")], 1).unwrap();
        forget(root, &["a_2"]);
        assert!(read(root, "a_1", 1).is_empty());
        assert_eq!(read(root, "a_2", 1).len(), 1);
        forget(root, &[]);
        assert!(read(root, "a_2", 1).is_empty());
        // A broken packed file reads as nothing.
        std::fs::create_dir_all(root.join("a_3")).unwrap();
        std::fs::write(root.join("a_3").join("5.jsonl.z"), b"not deflate").unwrap();
        assert!(read(root, "a_3", 5).is_empty());
    }
}

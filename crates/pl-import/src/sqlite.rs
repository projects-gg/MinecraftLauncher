//! Reading another launcher's SQLite database (the Modrinth App's `app.db`) without touching it.
//!
//! The other launcher may be running, so its database is never opened the usual way: a normal
//! SQLite connection, even a read-only one, may create `-wal` and `-shm` files next to it, take
//! locks, or write the log back into the database when it closes. Instead:
//!
//! - Without a write-ahead log next to it (the other launcher is closed), the file is opened as
//!   `immutable`: SQLite reads only the pages it needs, with no locks and no side files.
//! - With a log (the other launcher is open, or it crashed), the database and the log are read as
//!   plain files, the log's committed pages are laid over the database the way SQLite's own
//!   recovery does (`sqlite.org/fileformat2.html`, "The Write-Ahead Log"), and SQLite reads that
//!   snapshot from memory. A log restarted while it was read is read again.
//!
//! A database that cannot be read this way is simply not used: the caller falls back to what the
//! folders say.

use std::io::Read;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};

/// Databases larger than this are not copied into memory (the Modrinth App's is a few megabytes;
/// only its download cache grows).
const MAX_SNAPSHOT_BYTES: u64 = 256 * 1024 * 1024;
/// A log this large is left alone (SQLite keeps it near 4 MB while the other launcher runs).
const MAX_LOG_BYTES: u64 = 128 * 1024 * 1024;
/// Smaller files are not databases (one page at least).
const MIN_DATABASE_BYTES: u64 = 512;
const LOG_HEADER: usize = 32;
const FRAME_HEADER: usize = 24;
const LOG_MAGIC_LITTLE: u32 = 0x377f_0682;
const LOG_MAGIC_BIG: u32 = 0x377f_0683;
const LOG_VERSION: u32 = 3_007_000;
/// A log restarted while it is read is read again this many times.
const ATTEMPTS: usize = 3;

/// A read-only connection to what `database` holds now: its committed transactions, the ones
/// still in its write-ahead log included. `None` when it cannot be read.
pub(crate) fn open(database: &Path) -> Option<Connection> {
    let meta = std::fs::metadata(database).ok()?;
    if !meta.is_file() || meta.len() < MIN_DATABASE_BYTES {
        return None;
    }
    let log = side_file(database, "-wal");
    let log_len = std::fs::metadata(&log).map_or(0, |m| m.len());
    let connection = if log_len > LOG_HEADER as u64 && log_len <= MAX_LOG_BYTES {
        snapshot(database, &log).or_else(|| immutable(database))
    } else {
        immutable(database)
    }?;
    // Its schema is somebody else's: nothing in it runs with side effects, nothing is written.
    let _ = connection.execute_batch("PRAGMA trusted_schema = OFF; PRAGMA query_only = ON;");
    Some(connection)
}

/// `app.db` → `app.db-wal`.
fn side_file(database: &Path, suffix: &str) -> PathBuf {
    let mut name = database.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// Opens the file as read-only media: no locks, no side files, the log ignored.
fn immutable(database: &Path) -> Option<Connection> {
    let uri = immutable_uri(database)?;
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY
        | OpenFlags::SQLITE_OPEN_URI
        | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    match Connection::open_with_flags(&uri, flags) {
        Ok(connection) => Some(connection),
        Err(error) => {
            tracing::debug!(%error, path = %database.display(), "could not open the database");
            None
        }
    }
}

/// `C:\Users\Ada\app.db` → `file:///C:/Users/Ada/app.db?immutable=1`, every byte that is not
/// plainly safe in a URI percent-encoded (spaces, `?`, `#`, `%`, non-ASCII names).
pub(crate) fn immutable_uri(database: &Path) -> Option<String> {
    let path = database.to_str()?.replace('\\', "/");
    let mut uri = String::from("file:");
    // A drive path needs an empty authority before it; Unix and UNC paths start with slashes.
    uri.push_str(if path.starts_with('/') { "//" } else { "///" });
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b':' | b'.' | b'_' | b'-' | b'~' => {
                uri.push(char::from(byte));
            }
            _ => uri.push_str(&format!("%{byte:02X}")),
        }
    }
    uri.push_str("?immutable=1");
    Some(uri)
}

/// The database with its log's committed pages laid over it, read by SQLite from memory.
fn snapshot(database: &Path, log: &Path) -> Option<Connection> {
    for _ in 0..ATTEMPTS {
        let before = log_salt_on_disk(log);
        let image = read_capped(database, MAX_SNAPSHOT_BYTES)?;
        let log_bytes = read_capped(log, MAX_LOG_BYTES);
        let after = log_bytes.as_deref().and_then(log_salt);
        if before != after {
            // The other launcher restarted (or removed) its log meanwhile: what was read of the
            // database may not match it. Read both again.
            continue;
        }
        let image = apply_log(image, log_bytes.as_deref())?;
        return deserialize(image);
    }
    tracing::debug!(path = %database.display(), "the database kept changing while it was read");
    None
}

fn read_capped(path: &Path, max: u64) -> Option<Vec<u8>> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > max {
        return None;
    }
    std::fs::read(path).ok()
}

/// The salt of the log's header: it changes whenever the log starts over.
fn log_salt(log: &[u8]) -> Option<[u8; 8]> {
    log.get(16..24)?.try_into().ok()
}

fn log_salt_on_disk(log: &Path) -> Option<[u8; 8]> {
    let mut header = [0u8; LOG_HEADER];
    std::fs::File::open(log)
        .ok()?
        .read_exact(&mut header)
        .ok()?;
    log_salt(&header)
}

fn be32(bytes: &[u8], at: usize) -> u32 {
    bytes
        .get(at..at + 4)
        .and_then(|b| b.try_into().ok())
        .map_or(0, u32::from_be_bytes)
}

/// The database's page size from its header (`1` stands for 65536).
fn page_size(image: &[u8]) -> Option<usize> {
    if image.get(..16)? != b"SQLite format 3\0" {
        return None;
    }
    let raw = u16::from_be_bytes(image.get(16..18)?.try_into().ok()?);
    let size = if raw == 1 { 65_536 } else { usize::from(raw) };
    (size >= 512 && size.is_power_of_two()).then_some(size)
}

/// SQLite's log checksum over `data` (a multiple of 8 bytes), continuing from `sum`.
fn checksum(little: bool, data: &[u8], (mut s0, mut s1): (u32, u32)) -> (u32, u32) {
    for &[a, b, c, d, e, f, g, h] in data.as_chunks::<8>().0 {
        let (x0, x1) = if little {
            (
                u32::from_le_bytes([a, b, c, d]),
                u32::from_le_bytes([e, f, g, h]),
            )
        } else {
            (
                u32::from_be_bytes([a, b, c, d]),
                u32::from_be_bytes([e, f, g, h]),
            )
        };
        s0 = s0.wrapping_add(x0).wrapping_add(s1);
        s1 = s1.wrapping_add(x1).wrapping_add(s0);
    }
    (s0, s1)
}

/// The pages of the log's committed transactions, in log order (later ones win), and the size of
/// the database in pages after the last commit. Frames after the last commit, or after the first
/// frame that does not check out (a torn write, a leftover of an earlier log), are not used.
fn committed_frames(log: &[u8], page_size: usize) -> Option<(Vec<(u32, usize)>, u32)> {
    let header = log.get(..LOG_HEADER)?;
    let little = match be32(header, 0) {
        LOG_MAGIC_LITTLE => true,
        LOG_MAGIC_BIG => false,
        _ => return None,
    };
    if be32(header, 4) != LOG_VERSION || be32(header, 8) as usize != page_size {
        return None;
    }
    let mut sum = checksum(little, &header[..24], (0, 0));
    if sum != (be32(header, 24), be32(header, 28)) {
        return None;
    }
    let salt = &header[16..24];
    let frame_len = FRAME_HEADER + page_size;
    let mut pending = Vec::new();
    let mut committed = Vec::new();
    let mut pages_after_commit = 0;
    let mut at = LOG_HEADER;
    while let Some(frame) = log.get(at..at + frame_len) {
        let (head, page) = frame.split_at(FRAME_HEADER);
        let number = be32(head, 0);
        if number == 0 || &head[8..16] != salt {
            break;
        }
        sum = checksum(little, &head[..8], sum);
        sum = checksum(little, page, sum);
        if sum != (be32(head, 16), be32(head, 20)) {
            break;
        }
        pending.push((number, at + FRAME_HEADER));
        let commit = be32(head, 4);
        if commit != 0 {
            committed.append(&mut pending);
            pages_after_commit = commit;
        }
        at += frame_len;
    }
    (!committed.is_empty()).then_some((committed, pages_after_commit))
}

/// The database as the log's last commit left it, marked as an ordinary (rollback-journal)
/// database: SQLite reads a database from memory only that way.
fn apply_log(mut image: Vec<u8>, log: Option<&[u8]>) -> Option<Vec<u8>> {
    let page_size = page_size(&image)?;
    if let Some(log) = log
        && let Some((frames, pages)) = committed_frames(log, page_size)
    {
        let size = usize::try_from(pages).ok()?.checked_mul(page_size)?;
        if size as u64 > MAX_SNAPSHOT_BYTES + MAX_LOG_BYTES {
            return None;
        }
        image.resize(size, 0);
        for (number, offset) in frames {
            let Some(start) = usize::try_from(number - 1)
                .ok()
                .and_then(|n| n.checked_mul(page_size))
            else {
                continue;
            };
            // A page past the end was given back by a later transaction of the same log.
            if let (Some(target), Some(page)) = (
                image.get_mut(start..start + page_size),
                log.get(offset..offset + page_size),
            ) {
                target.copy_from_slice(page);
            }
        }
    }
    // Bytes 18 and 19: write and read format, 2 for a database in WAL mode.
    for at in [18, 19] {
        if image.get(at) == Some(&2) {
            image[at] = 1;
        }
    }
    Some(image)
}

fn deserialize(image: Vec<u8>) -> Option<Connection> {
    let mut connection = Connection::open_in_memory().ok()?;
    let len = image.len();
    match connection.deserialize_read_exact(rusqlite::MAIN_DB, image.as_slice(), len, true) {
        Ok(()) => Some(connection),
        Err(error) => {
            tracing::debug!(%error, "could not read the database snapshot");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::SystemTime;

    use rusqlite::config::DbConfig;

    use super::*;

    /// A program that keeps its database open: WAL mode, nothing written back to the database
    /// file, the log left in place when it closes (as after a crash).
    fn writer(path: &Path) -> Connection {
        let db = Connection::open(path).unwrap();
        db.execute_batch(
            "PRAGMA journal_mode = WAL; PRAGMA wal_autocheckpoint = 0;
             CREATE TABLE IF NOT EXISTS items (id INTEGER PRIMARY KEY, name TEXT NOT NULL);",
        )
        .unwrap();
        db.set_db_config(DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true)
            .unwrap();
        db
    }

    fn add(db: &Connection, names: &[&str]) {
        let tx = db.unchecked_transaction().unwrap();
        for name in names {
            tx.execute("INSERT INTO items (name) VALUES (?1)", [name])
                .unwrap();
        }
        tx.commit().unwrap();
    }

    fn names(db: &Connection) -> Vec<String> {
        let mut statement = db.prepare("SELECT name FROM items ORDER BY id").unwrap();
        statement
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    /// Every file in `dir` with its size, modification time and content.
    fn files(dir: &Path) -> BTreeMap<String, (u64, SystemTime, Vec<u8>)> {
        std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| {
                let meta = e.metadata().unwrap();
                (
                    e.file_name().to_string_lossy().into_owned(),
                    (
                        meta.len(),
                        meta.modified().unwrap(),
                        std::fs::read(e.path()).unwrap(),
                    ),
                )
            })
            .collect()
    }

    #[test]
    fn reads_what_a_running_program_still_keeps_in_its_log() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.db");
        let live = writer(&path);
        add(&live, &["Fabulously Optimized", "Create"]);
        add(&live, &["Skyblock"]);
        // Everything is still in the log: the database file holds the empty start.
        let before = files(dir.path());
        assert!(before["app.db-wal"].0 > before["app.db"].0);

        let read = open(&path).expect("readable");
        assert_eq!(names(&read), ["Fabulously Optimized", "Create", "Skyblock"]);
        assert!(read.execute("DELETE FROM items", []).is_err(), "read-only");
        drop(read);
        // The running program's files are exactly as they were; nothing was added next to them.
        assert_eq!(files(dir.path()), before);

        // It goes on writing; the next read sees that too.
        add(&live, &["Vanilla"]);
        assert_eq!(
            open(&path)
                .unwrap()
                .query_row("SELECT count(*) FROM items", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            4
        );
    }

    #[test]
    fn a_closed_database_is_read_without_side_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.db");
        {
            let db = Connection::open(&path).unwrap();
            db.execute_batch(
                "PRAGMA journal_mode = WAL;
                 CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL);
                 INSERT INTO items (name) VALUES ('Fabulously Optimized'), ('Create');",
            )
            .unwrap();
        }
        // A clean close folds the log into the database and removes it.
        let before = files(dir.path());
        assert_eq!(before.keys().collect::<Vec<_>>(), ["app.db"]);
        let read = open(&path).expect("readable");
        assert_eq!(names(&read), ["Fabulously Optimized", "Create"]);
        drop(read);
        assert_eq!(files(dir.path()), before, "no -wal or -shm was created");
    }

    #[test]
    fn a_torn_or_damaged_last_transaction_is_left_out() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.db");
        let log = side_file(&path, "-wal");
        {
            let db = writer(&path);
            add(&db, &["first"]);
            add(&db, &["second"]);
        }
        let whole = std::fs::read(&log).unwrap();
        // Cut into the last frame: the second commit never finished.
        std::fs::write(&log, &whole[..whole.len() - 100]).unwrap();
        assert_eq!(names(&open(&path).unwrap()), ["first"]);
        // One flipped byte in the last frame's page fails its checksum.
        let mut damaged = whole.clone();
        let last = damaged.len() - 10;
        damaged[last] ^= 0xff;
        std::fs::write(&log, &damaged).unwrap();
        assert_eq!(names(&open(&path).unwrap()), ["first"]);
        // A log of some other database (another salt) is not used at all: the database file
        // alone has no table yet.
        let mut foreign = whole.clone();
        foreign[16] ^= 0xff;
        std::fs::write(&log, &foreign).unwrap();
        assert!(
            open(&path)
                .unwrap()
                .prepare("SELECT name FROM items")
                .is_err()
        );
        // Whole again: both.
        std::fs::write(&log, &whole).unwrap();
        assert_eq!(names(&open(&path).unwrap()), ["first", "second"]);
    }

    /// Re-encodes a little-endian log with big-endian checksums (what a big-endian machine
    /// writes).
    fn big_endian(log: &[u8], page_size: usize) -> Vec<u8> {
        let mut out = log.to_vec();
        out[..4].copy_from_slice(&LOG_MAGIC_BIG.to_be_bytes());
        let mut sum = checksum(false, &out[..24], (0, 0));
        out[24..28].copy_from_slice(&sum.0.to_be_bytes());
        out[28..32].copy_from_slice(&sum.1.to_be_bytes());
        let frame_len = FRAME_HEADER + page_size;
        let mut at = LOG_HEADER;
        while at + frame_len <= out.len() {
            sum = checksum(false, &out[at..at + 8], sum);
            sum = checksum(false, &out[at + FRAME_HEADER..at + frame_len], sum);
            out[at + 16..at + 20].copy_from_slice(&sum.0.to_be_bytes());
            out[at + 20..at + 24].copy_from_slice(&sum.1.to_be_bytes());
            at += frame_len;
        }
        out
    }

    #[test]
    fn logs_with_either_byte_order_are_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.db");
        {
            let db = writer(&path);
            add(&db, &["Fabulously Optimized", "Create"]);
        }
        let log = side_file(&path, "-wal");
        let little = std::fs::read(&log).unwrap();
        let page = page_size(&std::fs::read(&path).unwrap()).unwrap();
        let big = big_endian(&little, page);
        std::fs::write(&log, &big).unwrap();
        // SQLite itself accepts the re-encoded log (a copy, opened the ordinary way)...
        let copy = tempfile::tempdir().unwrap();
        let copied = copy.path().join("app.db");
        std::fs::copy(&path, &copied).unwrap();
        std::fs::write(side_file(&copied, "-wal"), &big).unwrap();
        assert_eq!(
            names(&Connection::open(&copied).unwrap()),
            ["Fabulously Optimized", "Create"]
        );
        // ...and so does this reader.
        assert_eq!(
            names(&open(&path).unwrap()),
            ["Fabulously Optimized", "Create"]
        );
    }

    #[test]
    fn what_is_not_a_database_is_not_read() {
        let dir = tempfile::tempdir().unwrap();
        assert!(open(&dir.path().join("missing.db")).is_none());
        let text = dir.path().join("notes.db");
        std::fs::write(&text, vec![b'x'; 4096]).unwrap();
        let read = open(&text);
        assert!(read.is_none_or(|db| db.prepare("SELECT 1 FROM sqlite_master").is_err()));
        let tiny = dir.path().join("tiny.db");
        std::fs::write(&tiny, b"SQLite format 3\0").unwrap();
        assert!(open(&tiny).is_none());
    }

    #[test]
    fn paths_become_uris_sqlite_reads_back() {
        assert_eq!(
            immutable_uri(Path::new(
                r"C:\Users\Kullanıcı\AppData\Roaming\Modrinth App#1\app.db"
            ))
            .unwrap(),
            "file:///C:/Users/Kullan%C4%B1c%C4%B1/AppData/Roaming/Modrinth%20App%231/app.db?immutable=1"
        );
        assert_eq!(
            immutable_uri(Path::new("/home/ada/.local/share/ModrinthApp/app.db")).unwrap(),
            "file:///home/ada/.local/share/ModrinthApp/app.db?immutable=1"
        );
        // A folder name SQLite would otherwise read as a URI's fragment or escape still works.
        let dir = tempfile::tempdir().unwrap();
        let odd = dir.path().join("Ada's #% Modrinth Ü");
        std::fs::create_dir_all(&odd).unwrap();
        let path = odd.join("app.db");
        {
            let db = Connection::open(&path).unwrap();
            db.execute_batch(
                "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL);
                 INSERT INTO items (name) VALUES ('Create');",
            )
            .unwrap();
        }
        assert_eq!(names(&open(&path).unwrap()), ["Create"]);
    }
}

//! Whether a singleplayer world is open in a running game (PLAY-AGAIN). Minecraft (1.16+) holds
//! an operating system lock on `<world>/session.lock` while the world is open, and its world list
//! shows such a world as locked by another running game. A launch straight into a world ("Devam
//! et", a world's "Oyna", opening it to friends) asks first: the game would otherwise load for half
//! a minute and stay on its title screen, saying it could not reach the world.
//!
//! The check takes the lock for a moment, as Minecraft's own `DirectoryLock::isLocked` does, and
//! lets it go at once. Older versions held no lock (the second game took the world over); quick
//! play into worlds needs 1.20+, so the launcher never opens one of those itself.

use std::path::Path;

use fs4::{FileExt, TryLockError};

/// The world's lock file.
pub const SESSION_LOCK: &str = "session.lock";

/// True when another program (a running game) holds `<world_dir>/session.lock`. A world without
/// the file, or one the check cannot open, is not in use as far as anyone can tell.
pub fn world_in_use(world_dir: &Path) -> bool {
    let path = world_dir.join(SESSION_LOCK);
    let Ok(file) = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
    else {
        return false;
    };
    // The trait's methods by name: `std::fs::File` has its own since Rust 1.89, past the MSRV.
    match FileExt::try_lock(&file) {
        Ok(()) => {
            let _ = FileExt::unlock(&file);
            false
        }
        Err(TryLockError::WouldBlock) => true,
        Err(TryLockError::Error(error)) => {
            tracing::debug!(path = %path.display(), %error, "the world's lock could not be checked");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_world_open_in_another_game_is_in_use() {
        let dir = tempfile::tempdir().unwrap();
        let world = dir.path().join("saves").join("Ejderha Adası");
        std::fs::create_dir_all(&world).unwrap();
        // Never opened (or a world from before 1.16): nothing holds it.
        assert!(!world_in_use(&world));
        std::fs::write(world.join(SESSION_LOCK), "☃").unwrap();
        assert!(!world_in_use(&world), "a lock file nobody holds");
        // The game holds it while the world is open (its own handle, as Minecraft's FileChannel).
        let game = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(world.join(SESSION_LOCK))
            .unwrap();
        FileExt::try_lock(&game).unwrap();
        assert!(world_in_use(&world));
        // The check let go of nothing it did not take: the game still holds its lock.
        assert!(world_in_use(&world));
        FileExt::unlock(&game).unwrap();
        assert!(!world_in_use(&world));
        // The check never keeps the lock: the game can take it right after.
        FileExt::try_lock(&game).unwrap();
        FileExt::unlock(&game).unwrap();
    }
}

//! Java's AOT cache for faster Fabric starts (AOT-CACHE; Java 25's JEP 483/514/515).
//!
//! A game started with `-XX:AOTCache=<file>` finds the JDK's and the class path's classes already
//! loaded and linked, with their method profiles: the official pack opens ~15–20% faster (mixin
//! and bootstrap phase). The cache is made in two steps, both out of the player's way:
//!
//! 1. **Training** ([`AotPlan::Train`]): the player's own next start of the instance runs with
//!    `-XX:AOTMode=record`; when the game exits normally the JVM writes what it loaded to
//!    `<instance>.<key>.aotconf` (~10 s after the window closed, unseen).
//! 2. **Creating** ([`create`]): later, while no game runs, a JVM below normal priority turns the
//!    recording into `<instance>.<key>.aot` (~10 s, no window).
//!
//! The training start keeps the client jar off the class path and hands it to Fabric with
//! `-Dfabric.gameJarPath.client`: JDK 25 reads every class path jar's manifest while it records,
//! with a replace that is quadratic in the lines (`StringUtils::replace_no_expand`); the signed
//! client jar's 4.5 MB manifest (124 000 CRLF lines) alone held the JVM ~80 s before `main`. The
//! client jar is the last class path entry and its classes are loaded by Fabric's own class loader
//! (never cached), so the cache made without it is valid for the normal command (the class path of
//! the training is a prefix of the game's).
//!
//! A cache is only ever used for the exact launch it was made for ([`launch_key`]: the Java
//! runtime, the JVM options and every class path jar's size and time); anything else changes the
//! key and the instance's old files go ([`AotStore::plan`]). The JVM's own messages about the
//! cache stay out of the game's log (`-Xlog:aot*=off,cds*=off`): a cache the JVM still refuses
//! just means a normal start. A training whose game crashed is not tried again for that key, nor
//! after [`MAX_TRAININGS`] starts without a recording; a cached start that crashed before it got
//! to play drops its cache for good ([`AotStore::give_up`]).

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, UNIX_EPOCH};

use pl_net::CancellationToken;
use sha1::{Digest, Sha1};

use crate::error::Result;
use crate::launch::command::{LaunchCommand, java_argfile};
use crate::paths::write_atomic_blocking;

/// The caches' folder below the data folder's `cache/`.
pub const DIR_NAME: &str = "aot";
/// The first Java with `-XX:AOTCacheOutput`'s workflow and method profiles in the cache (25).
pub const MIN_JAVA_MAJOR: u32 = 25;
/// Fabric's main class: the only loader whose game jar can leave the class path for a training.
pub const FABRIC_MAIN: &str = "net.fabricmc.loader.impl.launch.knot.KnotClient";
/// Fabric's property naming the client jar when it is not on the class path.
pub const GAME_JAR_PROPERTY: &str = "fabric.gameJarPath.client";
/// Starts that record but never leave a recording (closed while opening, killed) before the
/// training of a key is given up.
pub const MAX_TRAININGS: u32 = 3;
/// Free room the cache folder needs before a training (recording + cache are ~180 MB each).
pub const TRAINING_ROOM: u64 = 2 * 1024 * 1024 * 1024;
/// The create step never takes longer than this (it takes ~10 s for the official pack).
pub const CREATE_TIMEOUT: Duration = Duration::from_secs(10 * 60);
/// The JVM's cache and CDS messages are not the player's business (a refused cache is a normal
/// start).
pub const QUIET_LOG: &str = "-Xlog:aot*=off,cds*=off";

const CACHE_EXT: &str = "aot";
const CONFIG_EXT: &str = "aotconf";
const ARGS_EXT: &str = "args";
/// The Java that recorded (the create step must be the same runtime).
const JAVA_EXT: &str = "java";
const TRIES_EXT: &str = "tries";
/// A key given up for good (its training crashed, or its cache did).
const GIVEN_UP: &str = "x";

/// What one launch does about the cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AotPlan {
    /// A normal start.
    Off,
    /// Start with this cache.
    Use { cache: PathBuf },
    /// Record this start into `config` (the client jar leaves the class path, [`training_jvm_args`]).
    Train { config: PathBuf },
}

/// A launch's cache key, or `None` when this launch cannot have a cache: Java older than
/// [`MIN_JAVA_MAJOR`], not a Fabric start, a wrapper command, a client jar that is not the class
/// path's last entry, a Java agent or the player's own CDS/AOT options. `java_home` is the runtime's folder (its
/// `release` file and module image are part of the key).
pub fn launch_key(
    command: &LaunchCommand,
    java_major: u32,
    java_home: &Path,
    client_jar: &Path,
) -> Option<String> {
    if java_major < MIN_JAVA_MAJOR {
        return None;
    }
    // A wrapper command (`gamemoderun java ...`) starts something else first: left alone.
    let range = command.jvm_option_range();
    if range.start != 0 || command.args.get(range.end).map(String::as_str) != Some(FABRIC_MAIN) {
        return None;
    }
    let jvm = command.jvm_options();
    if jvm.iter().any(|a| foreign_option(a)) {
        return None;
    }
    let classpath = classpath_of(jvm)?;
    let entries = split_classpath(classpath);
    if entries.last().map(Path::new) != Some(client_jar) {
        return None;
    }
    let mut hasher = Sha1::new();
    let mut line = |text: &str| {
        hasher.update(text.as_bytes());
        hasher.update(b"\n");
    };
    line(&command.program.to_string_lossy());
    let release = std::fs::read_to_string(java_home.join("release")).unwrap_or_default();
    line(&release);
    line(&stamp(&java_home.join("lib").join("modules")));
    for option in jvm.iter().filter(|a| !volatile_option(a)) {
        line(option);
    }
    for entry in entries {
        line(&stamp(Path::new(entry)));
    }
    let digest = hasher.finalize();
    let mut key = String::with_capacity(16);
    for byte in &digest[..8] {
        let _ = write!(key, "{byte:02x}");
    }
    Some(key)
}

/// JVM options that change from start to start without changing what the cache holds: the
/// launcher's own properties (the overlay's port and token, its version), the heap's size.
fn volatile_option(arg: &str) -> bool {
    arg.starts_with("-Dprojects.")
        || arg.starts_with("-Dminecraft.launcher.version=")
        || arg.starts_with("-Xms")
        || arg.starts_with("-Xmx")
}

/// Options a cache does not go with: agents (the JVM records nothing with them), the player's own
/// CDS or AOT setup.
fn foreign_option(arg: &str) -> bool {
    arg.starts_with("-javaagent")
        || arg.starts_with("-agentlib")
        || arg.starts_with("-agentpath")
        || arg.starts_with("-XX:AOT")
        || arg.starts_with("-XX:SharedArchiveFile")
        || arg.starts_with("-XX:ArchiveClassesAtExit")
        || arg.starts_with("-XX:+AutoCreateSharedArchive")
        || arg.starts_with("-Xshare")
        || arg.starts_with("-Djava.system.class.loader")
        || arg.starts_with(&format!("-D{GAME_JAR_PROPERTY}="))
}

fn classpath_of(jvm: &[String]) -> Option<&str> {
    let at = jvm
        .iter()
        .position(|a| a == "-cp" || a == "-classpath" || a == "--class-path")?;
    jvm.get(at + 1).map(String::as_str)
}

fn split_classpath(classpath: &str) -> Vec<&str> {
    let separator = if cfg!(windows) { ';' } else { ':' };
    classpath
        .split(separator)
        .filter(|e| !e.is_empty())
        .collect()
}

/// `<path> <size> <modified ns>` (`-` when it cannot be read).
fn stamp(path: &Path) -> String {
    match std::fs::metadata(path) {
        Ok(meta) => {
            let modified = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_nanos());
            format!("{} {} {modified}", path.display(), meta.len())
        }
        Err(_) => format!("{} -", path.display()),
    }
}

/// JVM options that start with `cache`.
pub fn use_jvm_args(cache: &Path) -> Vec<String> {
    vec![
        format!("-XX:AOTCache={}", cache.display()),
        QUIET_LOG.to_owned(),
    ]
}

/// JVM options of a training start that records into `config`; the client jar goes to Fabric by
/// [`GAME_JAR_PROPERTY`] (the caller takes it off the class path).
pub fn training_jvm_args(config: &Path, client_jar: &Path) -> Vec<String> {
    vec![
        "-XX:AOTMode=record".to_owned(),
        format!("-XX:AOTConfiguration={}", config.display()),
        QUIET_LOG.to_owned(),
        format!("-D{GAME_JAR_PROPERTY}={}", client_jar.display()),
    ]
}

/// The JVM options the create step repeats from a training start: everything but the recording's
/// own options, the launcher's per-start properties (the overlay's token among them) and `-Xms`.
pub fn create_jvm_options(training: &[String]) -> Vec<String> {
    training
        .iter()
        .filter(|a| {
            !(a.starts_with("-XX:AOTMode=")
                || a.starts_with("-XX:AOTConfiguration=")
                || a.as_str() == QUIET_LOG
                || a.starts_with("-Dprojects.")
                || a.starts_with("-Xms"))
        })
        .cloned()
        .collect()
}

/// The instance id as it appears in file names.
fn file_id(instance: &str) -> String {
    instance
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// A recording that waits for its create step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingCache {
    pub instance: String,
    pub key: String,
    pub config: PathBuf,
    pub args: PathBuf,
    pub cache: PathBuf,
    /// The Java that recorded it.
    pub java: PathBuf,
}

/// The cache folder (`<data>/cache/aot`): per instance at most one key's files.
#[derive(Debug, Clone)]
pub struct AotStore {
    dir: PathBuf,
}

impl AotStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn file(&self, instance: &str, key: &str, ext: &str) -> PathBuf {
        self.dir.join(format!("{}.{key}.{ext}", file_id(instance)))
    }

    /// The files of `instance` with their key (`<instance>.<key>.<ext>[.part]`).
    fn files_of(&self, instance: &str) -> Vec<(PathBuf, String)> {
        let prefix = format!("{}.", file_id(instance));
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        entries
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                let rest = name.strip_prefix(&prefix)?;
                let key = rest.split('.').next()?.to_owned();
                // Another instance whose id starts with this one's and a dot is not this one's.
                (key.len() == 16 && key.bytes().all(|b| b.is_ascii_hexdigit()))
                    .then(|| (entry.path(), key))
            })
            .collect()
    }

    /// What this start of `instance` does: its cache when there is one, else a training when the
    /// key has not been given up and `room` says the disk has [`TRAINING_ROOM`]; a recording
    /// waiting for its create step starts normally. The instance's files of other keys go first
    /// (the Java, the class path or the options changed). Files that cannot be removed (a running
    /// game maps its cache) stay until the next time.
    pub fn plan(&self, instance: &str, key: &str, room: bool) -> AotPlan {
        for (path, other) in self.files_of(instance) {
            if other != key {
                match std::fs::remove_file(&path) {
                    Ok(()) => {
                        tracing::info!(path = %path.display(), "removed an outdated AOT cache file")
                    }
                    Err(error) => {
                        tracing::debug!(%error, path = %path.display(), "outdated AOT cache file kept for now")
                    }
                }
            }
        }
        let cache = self.file(instance, key, CACHE_EXT);
        if cache.is_file() {
            return AotPlan::Use { cache };
        }
        let config = self.file(instance, key, CONFIG_EXT);
        if config.is_file() {
            return AotPlan::Off;
        }
        if !room || self.tries(instance, key).is_none_or(|n| n >= MAX_TRAININGS) {
            return AotPlan::Off;
        }
        AotPlan::Train { config }
    }

    /// Starts recorded so far for `key`; `None` once given up.
    fn tries(&self, instance: &str, key: &str) -> Option<u32> {
        match std::fs::read_to_string(self.file(instance, key, TRIES_EXT)) {
            Ok(text) if text.trim() == GIVEN_UP => None,
            Ok(text) => Some(text.trim().parse().unwrap_or(MAX_TRAININGS)),
            Err(_) => Some(0),
        }
    }

    /// Before a training start: counts it and keeps what the create step repeats: the Java that
    /// records and the JVM options ([`create_jvm_options`] of `training_jvm`, as a Java argument
    /// file).
    pub fn begin_training(
        &self,
        instance: &str,
        key: &str,
        java: &Path,
        training_jvm: &[String],
    ) -> Result<()> {
        let tries = self.tries(instance, key).unwrap_or(MAX_TRAININGS);
        write_atomic_blocking(
            &self.file(instance, key, TRIES_EXT),
            (tries + 1).to_string().as_bytes(),
        )?;
        write_atomic_blocking(
            &self.file(instance, key, JAVA_EXT),
            java.to_string_lossy().as_bytes(),
        )?;
        let options = create_jvm_options(training_jvm);
        write_atomic_blocking(
            &self.file(instance, key, ARGS_EXT),
            java_argfile(&options).as_bytes(),
        )
    }

    /// After a training start ended: a crashed game gives the key up (the recording, if any,
    /// goes); a clean exit leaves its recording for [`AotStore::pending`].
    pub fn training_ended(&self, instance: &str, key: &str, crashed: bool) {
        if crashed {
            self.give_up(instance, key);
        }
    }

    /// Never again for this key: its cache (when the JVM lets go of it), recording and argument
    /// file go, and a marker stays.
    pub fn give_up(&self, instance: &str, key: &str) {
        for ext in [CACHE_EXT, CONFIG_EXT, ARGS_EXT, JAVA_EXT] {
            let _ = std::fs::remove_file(self.file(instance, key, ext));
        }
        if let Err(error) =
            write_atomic_blocking(&self.file(instance, key, TRIES_EXT), GIVEN_UP.as_bytes())
        {
            tracing::debug!(%error, "AOT give-up marker not written");
        }
    }

    /// Recordings that wait for their create step.
    pub fn pending(&self) -> Vec<PendingCache> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(stem) = name.strip_suffix(&format!(".{CONFIG_EXT}")) else {
                continue;
            };
            let Some((instance, key)) = stem.rsplit_once('.') else {
                continue;
            };
            let Ok(java) = std::fs::read_to_string(self.dir.join(format!("{stem}.{JAVA_EXT}")))
            else {
                continue;
            };
            let pending = PendingCache {
                java: PathBuf::from(java.trim()),
                instance: instance.to_owned(),
                key: key.to_owned(),
                config: entry.path(),
                args: self.dir.join(format!("{stem}.{ARGS_EXT}")),
                cache: self.dir.join(format!("{stem}.{CACHE_EXT}")),
            };
            if pending.args.is_file() && pending.java.is_file() && !pending.cache.exists() {
                out.push(pending);
            }
        }
        out
    }

    /// Every file of the folder with its size, for the cleanup.
    pub fn files(&self) -> Vec<(PathBuf, u64)> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        entries
            .flatten()
            .filter_map(|e| {
                let meta = e.metadata().ok()?;
                meta.is_file().then(|| (e.path(), meta.len()))
            })
            .collect()
    }
}

/// How a create step ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateOutcome {
    /// The cache is in place (its size).
    Created(u64),
    /// Stopped by `cancel` (a game started): the recording stays for another time.
    Cancelled,
    /// The JVM could not make a cache from it: the key is given up.
    Failed,
}

/// Turns `pending`'s recording into its cache with the Java that recorded it, below normal
/// priority and without a window; the recording and its side files go once the cache is in place.
/// The output is not kept (the JVM prints a line per class it skips).
pub async fn create(
    store: &AotStore,
    pending: &PendingCache,
    cancel: &CancellationToken,
) -> CreateOutcome {
    let java = pending.java.as_path();
    let part = pending.cache.with_extension("aot.part");
    let _ = tokio::fs::remove_file(&part).await;
    let started = std::time::Instant::now();
    let mut command = tokio::process::Command::new(crate::launch::process::launch_program(java));
    command
        .arg("-XX:AOTMode=create")
        .arg(format!("-XX:AOTConfiguration={}", pending.config.display()))
        .arg(format!("-XX:AOTCache={}", part.display()))
        .arg(format!("@{}", pending.args.display()))
        .current_dir(store.dir())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(crate::java::detect::CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS);
    let status = match command.spawn() {
        Ok(mut child) => tokio::select! {
            status = child.wait() => status.ok(),
            _ = cancel.cancelled() => {
                let _ = child.kill().await;
                let _ = tokio::fs::remove_file(&part).await;
                tracing::info!(instance = %pending.instance, "AOT cache creation paused for a game");
                return CreateOutcome::Cancelled;
            }
            _ = tokio::time::sleep(CREATE_TIMEOUT) => {
                let _ = child.kill().await;
                None
            }
        },
        Err(error) => {
            tracing::warn!(%error, java = %java.display(), "could not start the AOT cache creation");
            None
        }
    };
    let size = tokio::fs::metadata(&part).await.map(|m| m.len()).ok();
    let created = match (status, size) {
        (Some(status), Some(size)) if status.success() && size > 0 => {
            tokio::fs::rename(&part, &pending.cache)
                .await
                .ok()
                .map(|()| size)
        }
        _ => None,
    };
    let elapsed_ms = started.elapsed().as_millis();
    match created {
        Some(size) => {
            let _ = tokio::fs::remove_file(&pending.config).await;
            let _ = tokio::fs::remove_file(&pending.args).await;
            let _ = tokio::fs::remove_file(pending.args.with_extension(JAVA_EXT)).await;
            tracing::info!(instance = %pending.instance, size, elapsed_ms, "AOT cache created");
            CreateOutcome::Created(size)
        }
        None => {
            let _ = tokio::fs::remove_file(&part).await;
            let (store, instance, key) =
                (store.clone(), pending.instance.clone(), pending.key.clone());
            let _ = tokio::task::spawn_blocking(move || store.give_up(&instance, &key)).await;
            tracing::warn!(instance = %pending.instance, ?status, elapsed_ms, "AOT cache creation failed; this setup starts without one");
            CreateOutcome::Failed
        }
    }
}

/// Windows' `BELOW_NORMAL_PRIORITY_CLASS`: the create step leaves the processor to the player.
#[cfg(windows)]
const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x0000_4000;

#[cfg(test)]
mod tests {
    use super::*;

    const SEP: &str = if cfg!(windows) { ";" } else { ":" };

    fn command(dir: &Path, jvm: Vec<String>, main: &str) -> LaunchCommand {
        let mut args = jvm.clone();
        args.push(main.to_owned());
        args.push("--username".to_owned());
        LaunchCommand::for_tests(dir.join("java"), args, dir.to_path_buf())
            .with_jvm_options(0..jvm.len())
    }

    fn setup() -> (tempfile::TempDir, PathBuf, PathBuf, Vec<String>) {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("jre");
        std::fs::create_dir_all(home.join("lib")).unwrap();
        std::fs::write(home.join("release"), "JAVA_VERSION=\"25.0.1\"").unwrap();
        std::fs::write(home.join("lib").join("modules"), b"modules").unwrap();
        let lib = dir.path().join("lib.jar");
        let client = dir.path().join("client.jar");
        std::fs::write(&lib, b"lib").unwrap();
        std::fs::write(&client, b"client").unwrap();
        let jvm = vec![
            "-Xms1024M".to_owned(),
            "-Xmx4096M".to_owned(),
            "-cp".to_owned(),
            format!("{}{SEP}{}", lib.display(), client.display()),
            "-Dprojects.overlay.token=abcdef".to_owned(),
        ];
        (dir, home, client, jvm)
    }

    #[test]
    fn the_key_follows_the_setup_and_ignores_per_start_options() {
        let (dir, home, client, jvm) = setup();
        let key = launch_key(
            &command(dir.path(), jvm.clone(), FABRIC_MAIN),
            25,
            &home,
            &client,
        )
        .unwrap();
        assert_eq!(key.len(), 16);
        // The overlay's token, the heap and the launcher's version change every start.
        let mut other = jvm.clone();
        other[1] = "-Xmx6144M".to_owned();
        other[4] = "-Dprojects.overlay.token=zzzzzz".to_owned();
        other.push("-Dminecraft.launcher.version=2.1.4".to_owned());
        assert_eq!(
            launch_key(&command(dir.path(), other, FABRIC_MAIN), 25, &home, &client).as_deref(),
            Some(key.as_str())
        );
        // A player's option, a changed jar, another runtime: another key.
        let mut typed = jvm.clone();
        typed.push("-XX:+UseZGC".to_owned());
        assert_ne!(
            launch_key(&command(dir.path(), typed, FABRIC_MAIN), 25, &home, &client).unwrap(),
            key
        );
        std::fs::write(dir.path().join("lib.jar"), b"lib, newer").unwrap();
        let changed = launch_key(
            &command(dir.path(), jvm.clone(), FABRIC_MAIN),
            25,
            &home,
            &client,
        )
        .unwrap();
        assert_ne!(changed, key);
        std::fs::write(home.join("release"), "JAVA_VERSION=\"25.0.2\"").unwrap();
        assert_ne!(
            launch_key(&command(dir.path(), jvm, FABRIC_MAIN), 25, &home, &client).unwrap(),
            changed
        );
    }

    #[test]
    fn only_fabric_on_java_25_with_the_client_jar_last_and_no_agents() {
        let (dir, home, client, jvm) = setup();
        let ok = |jvm: Vec<String>, main: &str, major: u32| {
            launch_key(&command(dir.path(), jvm, main), major, &home, &client).is_some()
        };
        assert!(ok(jvm.clone(), FABRIC_MAIN, 25));
        assert!(ok(jvm.clone(), FABRIC_MAIN, 26));
        assert!(!ok(jvm.clone(), FABRIC_MAIN, 21));
        assert!(!ok(jvm.clone(), "net.minecraft.client.main.Main", 25));
        for foreign in [
            "-javaagent:x.jar",
            "-agentlib:jdwp=transport=dt_socket",
            "-XX:AOTCache=mine.aot",
            "-XX:SharedArchiveFile=a.jsa",
            "-Xshare:off",
        ] {
            let mut with = jvm.clone();
            with.push(foreign.to_owned());
            assert!(!ok(with, FABRIC_MAIN, 25), "{foreign}");
        }
        let mut first = jvm.clone();
        first[3] = format!(
            "{}{SEP}{}",
            client.display(),
            dir.path().join("lib.jar").display()
        );
        assert!(!ok(first, FABRIC_MAIN, 25), "client jar not last");
    }

    #[test]
    fn create_options_leave_out_the_recording_the_token_and_the_initial_heap() {
        let config = Path::new("x.aotconf");
        let client = Path::new("client.jar");
        let mut training = vec!["-Xms1024M".to_owned(), "-Xmx4096M".to_owned()];
        training.extend(training_jvm_args(config, client));
        training.push("-Dprojects.overlay.token=secret".to_owned());
        training.extend(["-cp".to_owned(), "lib.jar".to_owned()]);
        let options = create_jvm_options(&training);
        assert_eq!(
            options,
            vec![
                "-Xmx4096M".to_owned(),
                format!("-D{GAME_JAR_PROPERTY}=client.jar"),
                "-cp".to_owned(),
                "lib.jar".to_owned()
            ]
        );
        assert!(training.iter().any(|a| a == QUIET_LOG));
        assert!(
            use_jvm_args(Path::new("c.aot"))
                .iter()
                .any(|a| a == QUIET_LOG)
        );
    }

    #[test]
    fn plan_trains_then_waits_then_uses_and_drops_other_keys() {
        let dir = tempfile::tempdir().unwrap();
        let store = AotStore::new(dir.path());
        let key = "0123456789abcdef";
        let java = dir.path().join("java.exe");
        std::fs::write(&java, b"").unwrap();
        let AotPlan::Train { config } = store.plan("projects", key, true) else {
            panic!("first start trains")
        };
        assert_eq!(
            store.plan("projects", key, false),
            AotPlan::Off,
            "no room, no training"
        );
        store
            .begin_training(
                "projects",
                key,
                &java,
                &["-cp".to_owned(), "a.jar".to_owned()],
            )
            .unwrap();
        assert!(store.pending().is_empty(), "nothing recorded yet");
        std::fs::write(&config, b"recorded").unwrap();
        assert_eq!(
            store.plan("projects", key, true),
            AotPlan::Off,
            "recording waits"
        );
        let pending = store.pending();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].instance, "projects");
        assert_eq!(pending[0].key, key);
        assert_eq!(pending[0].java, java);
        std::fs::write(&pending[0].cache, b"cache").unwrap();
        std::fs::remove_file(&pending[0].config).unwrap();
        assert_eq!(
            store.plan("projects", key, true),
            AotPlan::Use {
                cache: pending[0].cache.clone()
            }
        );
        // An instance whose id only starts the same keeps its files.
        let other = "fedcba9876543210";
        std::fs::write(dir.path().join(format!("projects-2.{other}.aot")), b"x").unwrap();
        // A new key: the old files go.
        let new_key = "1111111111111111";
        assert!(matches!(
            store.plan("projects", new_key, true),
            AotPlan::Train { .. }
        ));
        assert!(!pending[0].cache.exists());
        assert!(dir.path().join(format!("projects-2.{other}.aot")).exists());
    }

    #[test]
    fn trainings_stop_after_a_crash_or_too_many_tries() {
        let dir = tempfile::tempdir().unwrap();
        let store = AotStore::new(dir.path());
        let key = "0123456789abcdef";
        for _ in 0..MAX_TRAININGS {
            assert!(matches!(store.plan("a", key, true), AotPlan::Train { .. }));
            store
                .begin_training("a", key, Path::new("java"), &[])
                .unwrap();
            store.training_ended("a", key, false);
        }
        assert_eq!(store.plan("a", key, true), AotPlan::Off);
        assert!(matches!(store.plan("b", key, true), AotPlan::Train { .. }));
        store
            .begin_training("b", key, Path::new("java"), &[])
            .unwrap();
        store.training_ended("b", key, true);
        assert_eq!(store.plan("b", key, true), AotPlan::Off);
        // A cache that crashed its game is not used again.
        std::fs::write(dir.path().join(format!("c.{key}.aot")), b"cache").unwrap();
        assert!(matches!(store.plan("c", key, true), AotPlan::Use { .. }));
        store.give_up("c", key);
        assert_eq!(store.plan("c", key, true), AotPlan::Off);
    }

    #[test]
    fn file_ids_stay_plain() {
        assert_eq!(file_id("projects"), "projects");
        assert_eq!(file_id("a.b c/ğ"), "a_b_c__");
    }
}

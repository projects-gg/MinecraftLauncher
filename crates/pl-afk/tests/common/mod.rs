//! Shared helpers for the session tests: a temporary AFK root and an in-memory client spawner.
#![allow(dead_code)]

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use pl_afk::{
    AccountStore, AfkAccount, AfkEvent, AfkEventKind, AfkManager, AfkPaths, AfkRuntime,
    CLIENT_EXE_NAME, ClientProcess, ClientSpawner, ConsoleLine, EventSink, ExitFuture,
    LOGIN_SCRIPT_FILE_NAME, LaunchSpec, ManagerConfig, ManagerOptions, MemorySecretStore,
    ProcessExit, SecretStore, SessionState,
};
use pl_net::CancellationToken;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream};

pub struct TestEnv {
    pub dir: tempfile::TempDir,
    pub paths: AfkPaths,
    pub secrets: Arc<MemorySecretStore>,
    pub store: Arc<AccountStore>,
    pub runtime: Arc<AfkRuntime>,
    pub events: Arc<Mutex<Vec<AfkEvent>>>,
}

impl TestEnv {
    /// A fresh AFK root whose "client" is `client` in a dev-override folder ([`shared_client`];
    /// a dummy file when `None`, for the in-memory spawner).
    pub fn new(client: Option<&Path>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let paths = AfkPaths::new(dir.path().join("afk"));
        let dev = match client {
            Some(source) => shared_client(source),
            None => {
                let dev = dir.path().join("dev");
                std::fs::create_dir_all(&dev).unwrap();
                std::fs::write(dev.join(CLIENT_EXE_NAME), b"in-memory client").unwrap();
                dev
            }
        };
        let secrets = Arc::new(MemorySecretStore::new());
        let dyn_secrets: Arc<dyn SecretStore> = secrets.clone();
        let (store, _) = AccountStore::open(paths.clone(), dyn_secrets).unwrap();
        let runtime = AfkRuntime::new(paths.bin_dir()).with_dev_dir(Some(dev));
        Self {
            dir,
            paths,
            secrets,
            store: Arc::new(store),
            runtime: Arc::new(runtime),
            events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn sink(&self) -> EventSink {
        let events = self.events.clone();
        Arc::new(move |event| events.lock().push(event))
    }

    pub fn manager(
        &self,
        spawner: Option<Arc<dyn ClientSpawner>>,
        config: ManagerConfig,
    ) -> AfkManager {
        AfkManager::new(
            self.store.clone(),
            self.runtime.clone(),
            self.sink(),
            ManagerOptions {
                config,
                spawner,
                launcher_nickname: "Hazar".into(),
                ..ManagerOptions::default()
            },
        )
    }

    pub fn add(&self, nickname: &str) -> AfkAccount {
        self.store.add_account(nickname).unwrap()
    }

    pub fn update(&self, id: &str, change: impl FnOnce(&mut AfkAccount)) {
        let mut account = self.store.account(id).unwrap();
        change(&mut account);
        self.store.update_account(account).unwrap();
    }

    pub fn account_dir(&self, id: &str) -> PathBuf {
        self.paths.account_dir(id)
    }

    pub fn login_script_exists(&self, id: &str) -> bool {
        self.account_dir(id).join(LOGIN_SCRIPT_FILE_NAME).exists()
    }

    pub fn states(&self, id: &str) -> Vec<SessionState> {
        self.events
            .lock()
            .iter()
            .filter(|e| e.account_id == id)
            .filter_map(|e| match &e.kind {
                AfkEventKind::State(state) => Some(state.clone()),
                _ => None,
            })
            .collect()
    }

    pub fn last_state(&self, id: &str) -> Option<SessionState> {
        self.states(id).pop()
    }

    pub fn lines(&self, id: &str) -> Vec<ConsoleLine> {
        self.events
            .lock()
            .iter()
            .filter(|e| e.account_id == id)
            .filter_map(|e| match &e.kind {
                AfkEventKind::Line(line) => Some(line.clone()),
                _ => None,
            })
            .collect()
    }

    pub fn countdowns(&self, id: &str) -> Vec<u32> {
        self.events
            .lock()
            .iter()
            .filter(|e| e.account_id == id)
            .filter_map(|e| match e.kind {
                AfkEventKind::Countdown { secs } => Some(secs),
                _ => None,
            })
            .collect()
    }

    /// Every serialized event, for "the secret never leaves" checks.
    pub fn all_event_json(&self) -> String {
        self.events
            .lock()
            .iter()
            .map(|e| serde_json::to_string(e).unwrap())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// A folder holding `source` as the client executable, shared by every test and every later run
/// of this build of it: each fresh copy is a new program to the virus scanner, which reads it
/// before its first start (seconds on a loaded machine). The runtime only reads the folder.
pub fn shared_client(source: &Path) -> PathBuf {
    let meta = std::fs::metadata(source).unwrap();
    let stamp = meta
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("pl-afk-fake-client-{}-{stamp}", meta.len()));
    let exe = dir.join(CLIENT_EXE_NAME);
    if !exe.is_file() {
        std::fs::create_dir_all(&dir).unwrap();
        // Copied aside and moved in whole: a test running meanwhile never sees half a file, and
        // when another one got there first, its copy stays.
        let mut part = tempfile::NamedTempFile::new_in(&dir).unwrap();
        std::io::copy(
            &mut std::fs::File::open(source).unwrap(),
            part.as_file_mut(),
        )
        .unwrap();
        let _ = part.persist_noclobber(&exe);
    }
    dir
}

/// Waits for `condition` in real time, looking less often the longer it takes, and fails after
/// `max` (for real processes; [`until`] is the paused-clock one).
pub async fn eventually(what: &str, max: Duration, mut condition: impl FnMut() -> bool) {
    let deadline = std::time::Instant::now() + max;
    let mut pause = Duration::from_millis(5);
    while !condition() {
        if std::time::Instant::now() >= deadline {
            panic!("timed out after {max:?} waiting for: {what}");
        }
        tokio::time::sleep(pause).await;
        pause = (pause * 2).min(Duration::from_millis(200));
    }
}

/// Polls `condition` (cheap under paused time) and fails after `max`.
pub async fn until(what: &str, max: Duration, mut condition: impl FnMut() -> bool) {
    let deadline = tokio::time::Instant::now() + max;
    while !condition() {
        if tokio::time::Instant::now() >= deadline {
            panic!("timed out waiting for: {what}");
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

pub fn is_error(state: &Option<SessionState>) -> bool {
    matches!(state, Some(SessionState::Error { .. }))
}

// --- In-memory client ------------------------------------------------------------------------------

/// The "process side" a script drives.
pub struct FakeProc {
    out: DuplexStream,
    err: DuplexStream,
    input: tokio::io::Lines<BufReader<DuplexStream>>,
    received: Arc<Mutex<Vec<String>>>,
}

impl FakeProc {
    pub async fn out(&mut self, line: &str) {
        let _ = self.out.write_all(format!("{line}\n").as_bytes()).await;
    }

    pub async fn err(&mut self, line: &str) {
        let _ = self.err.write_all(format!("{line}\r\n").as_bytes()).await;
    }

    pub async fn event(&mut self, json: &str) {
        self.out(&format!("@@MCC@@{json}")).await;
    }

    /// Next stdin line (recorded); `None` once the session closed stdin.
    pub async fn read(&mut self) -> Option<String> {
        let line = self.input.next_line().await.ok().flatten()?;
        self.received.lock().push(line.clone());
        Some(line)
    }

    /// Like the real client: exit 0 on `/quit`.
    pub async fn until_quit(&mut self) -> Option<i32> {
        while let Some(line) = self.read().await {
            if line == "/quit" {
                return Some(0);
            }
        }
        None
    }

    /// Ignores everything until killed.
    pub async fn hang(&mut self) -> Option<i32> {
        while self.read().await.is_some() {}
        std::future::pending().await
    }
}

pub type ScriptFuture = Pin<Box<dyn Future<Output = Option<i32>> + Send>>;
pub type Script = Arc<dyn Fn(usize, FakeProc) -> ScriptFuture + Send + Sync>;

/// What the session handed to one launch.
#[derive(Debug, Clone)]
pub struct Launch {
    pub at: tokio::time::Instant,
    pub spec: LaunchSpec,
    /// `MinecraftClient.ini` as it was when the client started.
    pub ini: String,
    /// The login script if it existed when the client started.
    pub login_script: Option<String>,
}

pub struct ScriptedSpawner {
    script: Script,
    pub launches: Mutex<Vec<Launch>>,
    pub received: Arc<Mutex<Vec<String>>>,
}

impl ScriptedSpawner {
    /// `script(run_index, process)` plays one launch and returns its exit code.
    pub fn new<F, Fut>(script: F) -> Arc<Self>
    where
        F: Fn(usize, FakeProc) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Option<i32>> + Send + 'static,
    {
        Arc::new(Self {
            script: Arc::new(move |run, proc| Box::pin(script(run, proc))),
            launches: Mutex::new(Vec::new()),
            received: Arc::new(Mutex::new(Vec::new())),
        })
    }

    pub fn launch_count(&self) -> usize {
        self.launches.lock().len()
    }

    pub fn launch(&self, index: usize) -> Launch {
        self.launches.lock()[index].clone()
    }
}

impl ClientSpawner for ScriptedSpawner {
    fn spawn(&self, spec: &LaunchSpec) -> std::io::Result<ClientProcess> {
        let run = {
            let mut launches = self.launches.lock();
            launches.push(Launch {
                at: tokio::time::Instant::now(),
                spec: spec.clone(),
                ini: std::fs::read_to_string(&spec.ini).unwrap_or_default(),
                login_script: std::fs::read_to_string(spec.cwd.join(LOGIN_SCRIPT_FILE_NAME)).ok(),
            });
            launches.len() - 1
        };
        let (client_stdin, proc_stdin) = tokio::io::duplex(64 * 1024);
        let (proc_stdout, client_stdout) = tokio::io::duplex(64 * 1024);
        let (proc_stderr, client_stderr) = tokio::io::duplex(64 * 1024);
        let proc = FakeProc {
            out: proc_stdout,
            err: proc_stderr,
            input: BufReader::new(proc_stdin).lines(),
            received: self.received.clone(),
        };
        let (exit_tx, exit_rx) = tokio::sync::oneshot::channel();
        let future = (self.script)(run, proc);
        let task = tokio::spawn(async move {
            let code = future.await;
            let _ = exit_tx.send(code);
        });
        Ok(ClientProcess {
            stdin: Box::new(client_stdin),
            stdout: Box::new(client_stdout),
            stderr: Box::new(client_stderr),
            exit: Box::new(FakeExit { exit_rx, task }),
        })
    }
}

struct FakeExit {
    exit_rx: tokio::sync::oneshot::Receiver<Option<i32>>,
    task: tokio::task::JoinHandle<()>,
}

impl ProcessExit for FakeExit {
    fn wait(self: Box<Self>, kill: CancellationToken) -> ExitFuture {
        let FakeExit { exit_rx, task } = *self;
        Box::pin(async move {
            tokio::select! {
                code = exit_rx => code.unwrap_or(None),
                () = kill.cancelled() => {
                    task.abort();
                    let _ = task.await;
                    None
                }
            }
        })
    }
}

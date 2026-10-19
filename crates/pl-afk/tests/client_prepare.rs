//! A connect that finds no AFK client: the manager refreshes the content index once (through the
//! app's release source), installs what it lists and goes on connecting, every waiting account
//! showing the step; a client that cannot be had leaves each of them a problem to show. Builds are
//! served over loopback HTTP and verified by sha256 through pl-net; the "client" is in memory.

mod common;

use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use common::{ScriptedSpawner, until};
use futures::FutureExt;
use parking_lot::Mutex;
use pl_afk::{
    AccountStore, AfkClientBuild, AfkClientRelease, AfkError, AfkEvent, AfkEventKind, AfkManager,
    AfkPaths, AfkRuntime, CLIENT_EXE_NAME, ClientProblem, ClientStep, ManagerConfig,
    ManagerOptions, MemorySecretStore, ReleaseSource, SecretStore,
};
use pl_net::{Downloader, HashAlgo, Http, HttpConfig, hash_bytes};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Semaphore;

const WAIT: Duration = Duration::from_secs(20);

/// A build like the client's CI makes: the executable at the zip root.
fn package(exe: &[u8]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.start_file(CLIENT_EXE_NAME, options).unwrap();
    zip.write_all(exe).unwrap();
    zip.start_file("libSystem.IO.Ports.Native.so", options)
        .unwrap();
    zip.write_all(b"native").unwrap();
    zip.finish().unwrap().into_inner()
}

#[derive(Clone)]
enum Route {
    Body(Arc<Vec<u8>>),
    /// Accepts the request and never answers.
    Stall,
    /// Answers once the gate has a permit.
    Gate(Arc<Vec<u8>>, Arc<Semaphore>),
}

/// Loopback HTTP: `GET /<path>` answers with its route, anything else is a 404; hits are counted.
struct Server {
    base: String,
    hits: Arc<Mutex<HashMap<String, usize>>>,
}

impl Server {
    async fn start(routes: Vec<(&'static str, Route)>) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let routes: Arc<HashMap<String, Route>> = Arc::new(
            routes
                .into_iter()
                .map(|(path, route)| (path.to_owned(), route))
                .collect(),
        );
        let hits = Arc::new(Mutex::new(HashMap::new()));
        let counted = hits.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let routes = routes.clone();
                let hits = counted.clone();
                tokio::spawn(async move {
                    let mut request = Vec::new();
                    let mut buf = [0u8; 1024];
                    while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                        match socket.read(&mut buf).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => request.extend_from_slice(&buf[..n]),
                        }
                    }
                    let text = String::from_utf8_lossy(&request);
                    let path = text
                        .split_whitespace()
                        .nth(1)
                        .unwrap_or("/")
                        .trim_start_matches('/')
                        .to_owned();
                    *hits.lock().entry(path.clone()).or_default() += 1;
                    let body = match routes.get(&path).cloned() {
                        Some(Route::Body(body)) => Some(body),
                        Some(Route::Gate(body, gate)) => {
                            let _permit = gate.acquire().await.unwrap();
                            Some(body)
                        }
                        Some(Route::Stall) => {
                            std::future::pending::<()>().await;
                            None
                        }
                        None => None,
                    };
                    let response = match body {
                        Some(body) => {
                            let mut r = format!(
                                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/zip\r\nConnection: close\r\n\r\n",
                                body.len()
                            )
                            .into_bytes();
                            r.extend_from_slice(&body);
                            r
                        }
                        None => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            .to_vec(),
                    };
                    let _ = socket.write_all(&response).await;
                    let _ = socket.shutdown().await;
                });
            }
        });
        Self { base, hits }
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{path}", self.base)
    }

    fn hits(&self, path: &str) -> usize {
        self.hits.lock().get(path).copied().unwrap_or(0)
    }
}

fn release(version: &str, url: String, zip: &[u8]) -> AfkClientRelease {
    AfkClientRelease {
        version: version.to_owned(),
        build: AfkClientBuild {
            urls: vec![url],
            sha256: hash_bytes(zip, HashAlgo::Sha256),
            size: zip.len() as u64,
        },
    }
}

/// What the scripted index refresh answers.
type Answer = Box<dyn Fn() -> pl_afk::Result<Option<AfkClientRelease>> + Send>;

/// The app's index refresh, scripted: counts its calls and answers with `answer`.
struct Source {
    calls: Arc<AtomicUsize>,
    answer: Arc<Mutex<Answer>>,
}

impl Source {
    fn new(answer: impl Fn() -> pl_afk::Result<Option<AfkClientRelease>> + Send + 'static) -> Self {
        Self {
            calls: Arc::new(AtomicUsize::new(0)),
            answer: Arc::new(Mutex::new(Box::new(answer))),
        }
    }

    fn answer(
        &self,
        answer: impl Fn() -> pl_afk::Result<Option<AfkClientRelease>> + Send + 'static,
    ) {
        *self.answer.lock() = Box::new(answer);
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    fn release_source(&self) -> ReleaseSource {
        let calls = self.calls.clone();
        let answer = self.answer.clone();
        Arc::new(move || {
            calls.fetch_add(1, Ordering::SeqCst);
            let result = (answer.lock())();
            async move { result }.boxed()
        })
    }
}

/// A fresh AFK root with nothing installed (no development client either).
struct Env {
    _dir: tempfile::TempDir,
    bin: PathBuf,
    store: Arc<AccountStore>,
    runtime: Arc<AfkRuntime>,
    events: Arc<Mutex<Vec<AfkEvent>>>,
    spawner: Arc<ScriptedSpawner>,
}

impl Env {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let paths = AfkPaths::new(dir.path().join("afk"));
        let secrets: Arc<dyn SecretStore> = Arc::new(MemorySecretStore::new());
        let (store, _) = AccountStore::open(paths.clone(), secrets).unwrap();
        let bin = paths.bin_dir();
        Self {
            runtime: Arc::new(AfkRuntime::new(&bin)),
            bin,
            _dir: dir,
            store: Arc::new(store),
            events: Arc::new(Mutex::new(Vec::new())),
            // In game until told to quit.
            spawner: ScriptedSpawner::new(|_, mut proc| async move { proc.until_quit().await }),
        }
    }

    fn manager(&self, source: Option<&Source>, known: Option<AfkClientRelease>) -> AfkManager {
        self.manager_with(source, known, Duration::from_millis(60))
    }

    fn manager_with(
        &self,
        source: Option<&Source>,
        known: Option<AfkClientRelease>,
        stagger: Duration,
    ) -> AfkManager {
        let events = self.events.clone();
        let mut config = HttpConfig::new("pl-afk-test");
        config.retries = 0;
        let manager = AfkManager::new(
            self.store.clone(),
            self.runtime.clone(),
            Arc::new(move |event| events.lock().push(event)),
            ManagerOptions {
                config: ManagerConfig {
                    stagger,
                    ..ManagerConfig::default()
                },
                downloader: Some(Downloader::new(Http::new(config).unwrap())),
                release_source: source.map(Source::release_source),
                spawner: Some(self.spawner.clone()),
                launcher_nickname: "Hazar".into(),
                ..ManagerOptions::default()
            },
        );
        manager.set_client_release(known);
        manager
    }

    fn add(&self, nickname: &str) -> String {
        self.store.add_account(nickname).unwrap().id
    }

    /// The client steps the account's card was told, in order.
    fn steps(&self, id: &str) -> Vec<Option<ClientStep>> {
        self.events
            .lock()
            .iter()
            .filter(|e| e.account_id == id)
            .filter_map(|e| match &e.kind {
                AfkEventKind::Client(step) => Some(*step),
                _ => None,
            })
            .collect()
    }

    fn launched_exe(&self, index: usize) -> PathBuf {
        self.spawner.launch(index).spec.exe
    }
}

fn is_problem(result: &pl_afk::Result<()>, expected: ClientProblem) -> bool {
    matches!(result, Err(AfkError::ClientUnavailable { problem, .. }) if *problem == expected)
}

#[tokio::test]
async fn a_connect_before_the_index_knew_the_client_refreshes_it_and_installs() {
    // The owner's case: the launcher read the index before AFK 4.0.0 was published, so it knows
    // no build, and nothing is installed.
    let env = Env::new();
    let zip = package(b"client 4.0.0");
    let server = Server::start(vec![(
        "v1/afk/4.0.0/windows-x86_64.zip",
        Route::Body(Arc::new(zip.clone())),
    )])
    .await;
    let published = release("4.0.0", server.url("v1/afk/4.0.0/windows-x86_64.zip"), &zip);
    let source = Source::new({
        let published = published.clone();
        move || Ok(Some(published.clone()))
    });
    let manager = env.manager(Some(&source), None);
    let id = env.add("Alpha");

    manager.connect(&id).await.unwrap();

    assert_eq!(source.calls(), 1, "the index is refreshed once");
    assert_eq!(env.runtime.installed_versions(), vec!["4.0.0"]);
    until("the client is launched", WAIT, || {
        env.spawner.launch_count() == 1
    })
    .await;
    assert_eq!(
        env.launched_exe(0),
        env.bin.join("4.0.0").join(CLIENT_EXE_NAME)
    );
    assert_eq!(server.hits("v1/afk/4.0.0/windows-x86_64.zip"), 1);
    // The card said what was going on, then let the session speak.
    let steps = env.steps(&id);
    assert_eq!(steps.first(), Some(&Some(ClientStep::Checking)));
    assert!(steps.contains(&Some(ClientStep::Installing)), "{steps:?}");
    assert_eq!(steps.last(), Some(&None));
    assert_eq!(manager.client_step(&id), None);
    assert!(manager.any_active());
    manager.shutdown().await;
}

#[tokio::test]
async fn an_index_without_the_client_says_it_is_not_published() {
    let env = Env::new();
    let source = Source::new(|| Ok(None));
    let manager = env.manager(Some(&source), None);
    let id = env.add("Alpha");

    let result = manager.connect(&id).await;

    assert!(
        is_problem(&result, ClientProblem::NotPublished),
        "{result:?}"
    );
    assert_eq!(source.calls(), 1);
    assert_eq!(
        manager.client_step(&id),
        Some(ClientStep::Failed {
            problem: ClientProblem::NotPublished
        })
    );
    assert_eq!(
        env.steps(&id).last(),
        Some(&Some(ClientStep::Failed {
            problem: ClientProblem::NotPublished
        }))
    );
    assert_eq!(env.spawner.launch_count(), 0);
    assert!(!manager.any_active());

    // Published since: "Tekrar dene" connects, and the card forgets the failure.
    let zip = package(b"client");
    let server = Server::start(vec![("afk.zip", Route::Body(Arc::new(zip.clone())))]).await;
    let published = release("4.0.0", server.url("afk.zip"), &zip);
    source.answer(move || Ok(Some(published.clone())));
    manager.connect(&id).await.unwrap();
    assert_eq!(source.calls(), 2);
    assert_eq!(manager.client_step(&id), None);
    until("the client is launched", WAIT, || {
        env.spawner.launch_count() == 1
    })
    .await;
    manager.shutdown().await;
}

#[tokio::test]
async fn an_unreachable_index_says_offline() {
    let env = Env::new();
    let source = Source::new(|| Err(AfkError::ContentUnavailable("no mirror answered".into())));
    let manager = env.manager(Some(&source), None);
    let id = env.add("Alpha");

    let result = manager.connect(&id).await;

    assert!(is_problem(&result, ClientProblem::Offline), "{result:?}");
    assert!(result.unwrap_err().is_network());
    assert_eq!(
        manager.client_step(&id),
        Some(ClientStep::Failed {
            problem: ClientProblem::Offline
        })
    );
    // Stopping the account clears it.
    manager.disconnect(&id);
    assert_eq!(manager.client_step(&id), None);
    assert_eq!(env.steps(&id).last(), Some(&None));
    manager.shutdown().await;
}

#[tokio::test]
async fn an_installed_client_connects_at_once_without_looking() {
    let env = Env::new();
    let zip = package(b"client 4.0.0");
    let server = Server::start(vec![("afk.zip", Route::Body(Arc::new(zip.clone())))]).await;
    let installed = release("4.0.0", server.url("afk.zip"), &zip);
    let downloader = Downloader::new(Http::new(HttpConfig::new("pl-afk-test")).unwrap());
    env.runtime
        .install(&downloader, &installed, &pl_net::DownloadOptions::default())
        .await
        .unwrap();
    // Offline start: the launcher knows no build, the installed one is used as it is.
    let source = Source::new(|| panic!("no refresh is needed"));
    let manager = env.manager(Some(&source), None);
    let id = env.add("Alpha");

    manager.connect(&id).await.unwrap();

    assert_eq!(source.calls(), 0);
    until("the client is launched", WAIT, || {
        env.spawner.launch_count() == 1
    })
    .await;
    // Nothing about the client flashed on the card.
    assert!(env.steps(&id).is_empty(), "{:?}", env.steps(&id));
    manager.shutdown().await;
}

#[tokio::test]
async fn a_known_build_installs_without_refreshing() {
    let env = Env::new();
    let zip = package(b"client 4.0.0");
    let server = Server::start(vec![("afk.zip", Route::Body(Arc::new(zip.clone())))]).await;
    let source = Source::new(|| panic!("the known build needs no refresh"));
    let manager = env.manager(
        Some(&source),
        Some(release("4.0.0", server.url("afk.zip"), &zip)),
    );
    let id = env.add("Alpha");

    manager.connect(&id).await.unwrap();

    assert_eq!(source.calls(), 0);
    assert_eq!(env.runtime.installed_versions(), vec!["4.0.0"]);
    until("the client is launched", WAIT, || {
        env.spawner.launch_count() == 1
    })
    .await;
    assert!(!env.steps(&id).contains(&Some(ClientStep::Checking)));
    manager.shutdown().await;
}

#[tokio::test]
async fn a_known_build_gone_from_the_mirrors_gets_one_more_look() {
    let env = Env::new();
    let old = package(b"client 4.0.0");
    let new = package(b"client 4.0.1");
    // 4.0.0 is no longer served (404); the index now lists 4.0.1.
    let server = Server::start(vec![("afk-4.0.1.zip", Route::Body(Arc::new(new.clone())))]).await;
    let newer = release("4.0.1", server.url("afk-4.0.1.zip"), &new);
    let source = Source::new(move || Ok(Some(newer.clone())));
    let manager = env.manager(
        Some(&source),
        Some(release("4.0.0", server.url("afk-4.0.0.zip"), &old)),
    );
    let id = env.add("Alpha");

    manager.connect(&id).await.unwrap();

    assert_eq!(source.calls(), 1);
    assert_eq!(env.runtime.installed_versions(), vec!["4.0.1"]);
    until("the client is launched", WAIT, || {
        env.spawner.launch_count() == 1
    })
    .await;
    assert_eq!(
        env.launched_exe(0),
        env.bin.join("4.0.1").join(CLIENT_EXE_NAME)
    );
    manager.shutdown().await;
}

#[tokio::test]
async fn an_update_that_cannot_download_keeps_the_installed_client() {
    let env = Env::new();
    let zip = package(b"client 4.0.0");
    let server = Server::start(vec![("afk.zip", Route::Body(Arc::new(zip.clone())))]).await;
    let downloader = Downloader::new(Http::new(HttpConfig::new("pl-afk-test")).unwrap());
    env.runtime
        .install(
            &downloader,
            &release("4.0.0", server.url("afk.zip"), &zip),
            &pl_net::DownloadOptions::default(),
        )
        .await
        .unwrap();
    // 4.0.1 is known but its mirror cannot be reached: the installed client still connects.
    let source = Source::new(|| panic!("an installed client needs no refresh"));
    let unreachable = release("4.0.1", "http://127.0.0.1:9/afk.zip".into(), b"zip");
    let manager = env.manager(Some(&source), Some(unreachable));
    let id = env.add("Alpha");

    manager.connect(&id).await.unwrap();

    until("the client is launched", WAIT, || {
        env.spawner.launch_count() == 1
    })
    .await;
    assert_eq!(
        env.launched_exe(0),
        env.bin.join("4.0.0").join(CLIENT_EXE_NAME)
    );
    assert_eq!(source.calls(), 0);
    manager.shutdown().await;
}

#[tokio::test]
async fn stopping_a_waiting_account_cancels_its_download() {
    let env = Env::new();
    let server = Server::start(vec![("afk.zip", Route::Stall)]).await;
    let manager = env.manager(
        None,
        Some(release("4.0.0", server.url("afk.zip"), &package(b"x"))),
    );
    let id = env.add("Alpha");

    let connect = tokio::spawn({
        let manager = manager.clone();
        let id = id.clone();
        async move { manager.connect(&id).await }
    });
    until("the download shows on the card", WAIT, || {
        matches!(
            manager.client_step(&id),
            Some(ClientStep::Downloading { .. })
        )
    })
    .await;
    assert!(
        manager.any_active(),
        "a wait for the client keeps the launcher up"
    );
    assert_eq!(manager.activity().connecting, 1);

    manager.disconnect(&id);

    let result = tokio::time::timeout(WAIT, connect).await.unwrap().unwrap();
    assert!(matches!(result, Err(AfkError::Cancelled)), "{result:?}");
    assert_eq!(manager.client_step(&id), None);
    until("the preparation ends", WAIT, || !manager.any_active()).await;
    assert_eq!(env.spawner.launch_count(), 0);
    assert!(env.runtime.installed_versions().is_empty());
    manager.shutdown().await;
}

#[tokio::test]
async fn waiting_accounts_share_one_download_and_one_may_leave() {
    let env = Env::new();
    let zip = package(b"client 4.0.0");
    let gate = Arc::new(Semaphore::new(0));
    let server = Server::start(vec![(
        "afk.zip",
        Route::Gate(Arc::new(zip.clone()), gate.clone()),
    )])
    .await;
    let manager = env.manager(None, Some(release("4.0.0", server.url("afk.zip"), &zip)));
    let alpha = env.add("Alpha");
    let beta = env.add("Beta");

    let connect = |id: &String| {
        let manager = manager.clone();
        let id = id.clone();
        tokio::spawn(async move { manager.connect(&id).await })
    };
    let first = connect(&alpha);
    until("the download starts", WAIT, || server.hits("afk.zip") == 1).await;
    let second = connect(&beta);
    until("both cards show the download", WAIT, || {
        manager
            .client_step(&alpha)
            .is_some_and(|s| s.is_preparing())
            && manager.client_step(&beta).is_some_and(|s| s.is_preparing())
    })
    .await;

    // Alpha is stopped; the download goes on for Beta.
    manager.disconnect(&alpha);
    let first = tokio::time::timeout(WAIT, first).await.unwrap().unwrap();
    assert!(matches!(first, Err(AfkError::Cancelled)), "{first:?}");
    gate.add_permits(10);
    tokio::time::timeout(WAIT, second)
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    until("Beta is launched", WAIT, || env.spawner.launch_count() == 1).await;
    assert_eq!(server.hits("afk.zip"), 1, "one download for both");
    assert!(manager.snapshot(&beta).state.is_active());
    assert!(!manager.snapshot(&alpha).state.is_active());
    assert_eq!(manager.client_step(&beta), None);
    manager.shutdown().await;
}

#[tokio::test]
async fn connect_all_waits_for_the_client_together() {
    let env = Env::new();
    let zip = package(b"client 4.0.0");
    let server = Server::start(vec![("afk.zip", Route::Body(Arc::new(zip.clone())))]).await;
    let published = release("4.0.0", server.url("afk.zip"), &zip);
    let source = Source::new(move || Ok(Some(published.clone())));
    let manager = env.manager(Some(&source), None);
    let alpha = env.add("Alpha");
    let beta = env.add("Beta");

    let outcome = manager.connect_all().await.unwrap();

    assert_eq!(outcome.started, vec![alpha.clone(), beta.clone()]);
    assert_eq!(source.calls(), 1);
    assert_eq!(server.hits("afk.zip"), 1);
    until("both are launched", WAIT, || {
        env.spawner.launch_count() == 2
    })
    .await;
    // Both cards showed the shared preparation.
    for id in [&alpha, &beta] {
        assert_eq!(
            env.steps(id).first(),
            Some(&Some(ClientStep::Checking)),
            "{id}"
        );
    }
    manager.shutdown().await;
}

#[tokio::test]
async fn connect_all_without_a_client_tells_every_account() {
    let env = Env::new();
    let source = Source::new(|| Ok(None));
    let manager = env.manager(Some(&source), None);
    let alpha = env.add("Alpha");
    let beta = env.add("Beta");

    let outcome = manager.connect_all().await.unwrap();

    assert_eq!(outcome.failed, vec![alpha.clone(), beta.clone()]);
    assert!(outcome.started.is_empty());
    assert_eq!(source.calls(), 1, "one look for the whole run");
    for id in [&alpha, &beta] {
        assert_eq!(
            manager.client_step(id),
            Some(ClientStep::Failed {
                problem: ClientProblem::NotPublished
            })
        );
    }
    assert_eq!(env.spawner.launch_count(), 0);
    assert!(!manager.any_active());
    manager.shutdown().await;
}

#[tokio::test]
async fn stopping_a_queued_account_takes_it_out_of_the_run() {
    let env = Env::new();
    let zip = package(b"client 4.0.0");
    let server = Server::start(vec![("afk.zip", Route::Body(Arc::new(zip.clone())))]).await;
    // Long enough a gap to stop Beta while it waits for its turn.
    let manager = env.manager_with(
        None,
        Some(release("4.0.0", server.url("afk.zip"), &zip)),
        Duration::from_secs(2),
    );
    let alpha = env.add("Alpha");
    let beta = env.add("Beta");
    let gamma = env.add("Gamma");

    let run = tokio::spawn({
        let manager = manager.clone();
        async move { manager.connect_all().await }
    });
    until("Alpha is launched", WAIT, || {
        env.spawner.launch_count() == 1
    })
    .await;
    // Beta waits for its turn: the player stops it.
    manager.disconnect(&beta);

    let outcome = tokio::time::timeout(WAIT, run)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(outcome.started, vec![alpha.clone(), gamma.clone()]);
    assert!(!outcome.failed.contains(&beta) && !outcome.skipped.contains(&beta));
    until("Gamma is launched", WAIT, || {
        env.spawner.launch_count() == 2
    })
    .await;
    assert!(!manager.snapshot(&beta).state.is_active());
    manager.shutdown().await;
}

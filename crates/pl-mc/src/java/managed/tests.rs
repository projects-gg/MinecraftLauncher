use std::collections::HashMap;
use std::sync::Arc;

use pl_net::{Http, HttpConfig};
use sha1::{Digest, Sha1};

use super::*;
use crate::java::runtime::{RemoteFile, RuntimeVersion};
use crate::platform::Platform;

fn sha1_hex(bytes: &[u8]) -> String {
    hex::encode(Sha1::digest(bytes))
}

fn write(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

fn marker_json(component: &str, platform: &str, version: &str, sha1: &str) -> String {
    format!(
        r#"{{"component": "{component}", "platform": "{platform}", "version": "{version}", "manifestSha1": "{sha1}"}}"#
    )
}

/// A runtime folder with a marker and an executable (Linux layout).
fn fake_runtime(root: &Path, component: &str, version: &str, sha1: &str) {
    write(&root.join("bin").join("java"), b"java");
    let platform = root.file_name().unwrap().to_str().unwrap();
    write(
        &root.join(MARKER_FILE),
        marker_json(component, platform, version, sha1).as_bytes(),
    );
}

fn entry(sha1: &str, version: &str, url: &str, size: u64) -> RuntimeEntry {
    RuntimeEntry {
        manifest: RemoteFile {
            sha1: sha1.to_owned(),
            size,
            url: url.to_owned(),
        },
        version: RuntimeVersion {
            name: version.to_owned(),
            released: None,
        },
    }
}

fn index_with(platform: &str, component: &str, entry: RuntimeEntry) -> RuntimeIndex {
    let mut components = BTreeMap::new();
    components.insert(component.to_owned(), vec![entry]);
    let mut index = RuntimeIndex::new();
    index.insert(platform.to_owned(), components);
    index
}

/// A local HTTP server answering GET requests from `files` (path → body), counting requests.
async fn file_server(
    files: HashMap<String, Vec<u8>>,
) -> (String, Arc<parking_lot::Mutex<HashMap<String, usize>>>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let files = Arc::new(files);
    let hits = Arc::new(parking_lot::Mutex::new(HashMap::<String, usize>::new()));
    let counter = Arc::clone(&hits);
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let files = Arc::clone(&files);
            let counter = Arc::clone(&counter);
            tokio::spawn(async move {
                let mut head = Vec::new();
                let mut buf = [0u8; 1024];
                while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                    match socket.read(&mut buf).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => head.extend_from_slice(&buf[..n]),
                    }
                }
                let text = String::from_utf8_lossy(&head);
                let path = text
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("/")
                    .to_owned();
                *counter.lock().entry(path.clone()).or_default() += 1;
                let response = match files.get(&path) {
                    Some(body) => {
                        let mut out = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        )
                        .into_bytes();
                        out.extend_from_slice(body);
                        out
                    }
                    None => {
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            .to_vec()
                    }
                };
                let _ = socket.write_all(&response).await;
                let _ = socket.shutdown().await;
            });
        }
    });
    (format!("http://{address}"), hits)
}

fn manifest_json(files: &[(&str, &[u8], &str)]) -> String {
    let mut entries = vec![
        r#""bin": {"type": "directory"}"#.to_owned(),
        r#""lib": {"type": "directory"}"#.to_owned(),
    ];
    for (name, body, url) in files {
        entries.push(format!(
            r#""{name}": {{"type": "file", "executable": {}, "downloads": {{"raw": {{"sha1": "{}", "size": {}, "url": "{url}"}}}}}}"#,
            name.starts_with("bin/"),
            sha1_hex(body),
            body.len()
        ));
    }
    format!(r#"{{"files": {{{}}}}}"#, entries.join(", "))
}

/// Only changed files are downloaded into `.next`; the runtime in use is not touched until the
/// swap; after it, the new build is in place and the folders next to it are gone.
#[tokio::test]
async fn stages_only_what_changed_then_swaps() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = DataDirs::new(tmp.path());
    let root = dirs.runtime_dir("java-runtime-delta", "linux").unwrap();

    // The build in use: java, a.txt and b.txt.
    let old_files: Vec<(&str, &[u8], &str)> = vec![
        ("bin/java", b"old-java", "http://unused/java"),
        ("lib/a.txt", b"same", "http://unused/a"),
        ("lib/b.txt", b"old-b", "http://unused/b"),
    ];
    for (name, body, _) in &old_files {
        write(&root.join(name), body);
    }
    write(
        &root.join(MANIFEST_FILE),
        manifest_json(&old_files).as_bytes(),
    );
    write(
        &root.join(MARKER_FILE),
        marker_json("java-runtime-delta", "linux", "21.0.7", "old").as_bytes(),
    );

    // The new build: java changed, a.txt the same, b.txt gone, c.txt new.
    let (base, hits) = {
        let new_java: &[u8] = b"new-java";
        let new_c: &[u8] = b"new-c";
        let mut served = HashMap::new();
        served.insert("/java".to_owned(), new_java.to_vec());
        served.insert("/a".to_owned(), b"same".to_vec());
        served.insert("/c".to_owned(), new_c.to_vec());
        file_server(served).await
    };
    let new_manifest = manifest_json(&[
        ("bin/java", b"new-java", &format!("{base}/java")),
        ("lib/a.txt", b"same", &format!("{base}/a")),
        ("lib/c.txt", b"new-c", &format!("{base}/c")),
    ]);
    // The manifest itself is served too.
    let (manifest_base, manifest_hits) = {
        let mut served = HashMap::new();
        served.insert(
            "/manifest.json".to_owned(),
            new_manifest.clone().into_bytes(),
        );
        file_server(served).await
    };
    let update = entry(
        &sha1_hex(new_manifest.as_bytes()),
        "21.0.8",
        &format!("{manifest_base}/manifest.json"),
        new_manifest.len() as u64,
    );

    let ctx = McContext::new(
        Http::new(HttpConfig::new("pl-mc-test")).unwrap(),
        dirs.clone(),
    )
    .with_platform(Platform::new(Os::Linux, Arch::X86_64, "6.1"));
    let staged = stage_update(
        &ctx,
        "java-runtime-delta",
        "linux",
        &update,
        &CancellationToken::new(),
        None,
    )
    .await
    .unwrap();
    assert_eq!(staged.version, "21.0.8");

    let next = staged_dir(&root);
    assert_eq!(std::fs::read(next.join("bin/java")).unwrap(), b"new-java");
    assert_eq!(std::fs::read(next.join("lib/a.txt")).unwrap(), b"same");
    assert_eq!(std::fs::read(next.join("lib/c.txt")).unwrap(), b"new-c");
    assert!(!next.join("lib/b.txt").exists());
    // The unchanged file came from the build in use.
    assert_eq!(hits.lock().get("/a"), None);
    assert_eq!(hits.lock().get("/java"), Some(&1));
    assert_eq!(manifest_hits.lock().get("/manifest.json"), Some(&1));
    // The runtime in use is untouched.
    assert_eq!(std::fs::read(root.join("bin/java")).unwrap(), b"old-java");

    // Listed with its staged update; nothing more to stage for that build.
    let listed = list_managed(&dirs, Os::Linux);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].version, "21.0.7");
    assert_eq!(listed[0].major, 21);
    assert_eq!(
        listed[0].staged.as_ref().map(|s| s.version.as_str()),
        Some("21.0.8")
    );
    let index = index_with("linux", "java-runtime-delta", update.clone());
    assert!(runtimes_to_stage(&listed, &index, |_| true).is_empty());

    // Staging the same build again does nothing.
    stage_update(
        &ctx,
        "java-runtime-delta",
        "linux",
        &update,
        &CancellationToken::new(),
        None,
    )
    .await
    .unwrap();
    assert_eq!(manifest_hits.lock().get("/manifest.json"), Some(&1));

    assert_eq!(
        swap_in_staged(&root).unwrap(),
        SwapOutcome::Swapped {
            version: "21.0.8".into()
        }
    );
    assert_eq!(std::fs::read(root.join("bin/java")).unwrap(), b"new-java");
    assert_eq!(std::fs::read(root.join("lib/a.txt")).unwrap(), b"same");
    assert!(!root.join("lib/b.txt").exists());
    assert!(!staged_dir(&root).exists());
    assert!(!old_dir(&root).exists());
    let listed = list_managed(&dirs, Os::Linux);
    assert_eq!(listed[0].version, "21.0.8");
    assert_eq!(listed[0].staged, None);
    assert!(update_for(&index, &listed[0]).is_none());
    assert_eq!(swap_in_staged(&root).unwrap(), SwapOutcome::NothingStaged);
}

/// A staged folder without its marker is not complete: never swapped in.
#[test]
fn incomplete_stages_are_ignored() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("runtimes").join("jre-legacy").join("linux");
    fake_runtime(&root, "jre-legacy", "8u51", "a");
    write(&staged_dir(&root).join("bin").join("java"), b"half");
    assert_eq!(swap_in_staged(&root).unwrap(), SwapOutcome::NothingStaged);
    assert_eq!(std::fs::read(root.join("bin/java")).unwrap(), b"java");
}

#[test]
fn recovers_interrupted_swaps_and_removals() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().join("runtimes").join("java-runtime-delta");

    // Stopped after the old build moved away: the complete new build takes its place.
    let root = base.join("linux");
    fake_runtime(&old_dir(&root), "java-runtime-delta", "21.0.7", "a");
    fake_runtime(&staged_dir(&root), "java-runtime-delta", "21.0.8", "b");
    recover(&root).unwrap();
    assert!(
        std::fs::read_to_string(root.join(MARKER_FILE))
            .unwrap()
            .contains("21.0.8")
    );
    assert!(!old_dir(&root).exists() && !staged_dir(&root).exists());

    // Stopped with no complete new build: the old one comes back.
    let root = base.join("windows-x64");
    fake_runtime(&old_dir(&root), "java-runtime-delta", "21.0.7", "a");
    write(&staged_dir(&root).join("bin").join("java"), b"half");
    recover(&root).unwrap();
    assert!(
        std::fs::read_to_string(root.join(MARKER_FILE))
            .unwrap()
            .contains("21.0.7")
    );

    // A removal that stopped half-way is finished, never undone.
    let root = base.join("mac-os");
    fake_runtime(&removed_dir(&root), "java-runtime-delta", "21.0.7", "a");
    recover(&root).unwrap();
    assert!(!root.exists());
    assert!(!removed_dir(&root).exists());
}

#[test]
fn removes_runtimes_and_reports_the_space() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = DataDirs::new(tmp.path());
    let legacy = dirs.runtime_dir("jre-legacy", "linux").unwrap();
    fake_runtime(&legacy, "jre-legacy", "8u51", "a");
    write(&legacy.join("lib").join("rt.jar"), &[0u8; 1000]);
    write(&staged_dir(&legacy).join("lib").join("rt.jar"), &[0u8; 500]);
    let marker_len = std::fs::metadata(legacy.join(MARKER_FILE)).unwrap().len();
    let freed = remove_runtime(&legacy).unwrap();
    assert_eq!(freed, 4 + 1000 + marker_len + 500);
    assert!(!legacy.exists() && !staged_dir(&legacy).exists());
    // The component folder went with its last runtime.
    assert!(!legacy.parent().unwrap().exists());

    // Another platform of the same component keeps the component folder.
    let delta = dirs.runtime_dir("java-runtime-delta", "linux").unwrap();
    let delta_x86 = dirs
        .runtime_dir("java-runtime-delta", "linux-i386")
        .unwrap();
    fake_runtime(&delta, "java-runtime-delta", "21.0.7", "a");
    fake_runtime(&delta_x86, "java-runtime-delta", "21.0.7", "b");
    remove_runtime(&delta).unwrap();
    assert!(delta_x86.is_dir());
    let listed = list_managed(&dirs, Os::Linux);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].platform, "linux-i386");
    assert_eq!(listed[0].arch, Arch::X86);
}

#[test]
fn lists_only_complete_runtimes() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = DataDirs::new(tmp.path());
    fake_runtime(
        &dirs.runtime_dir("java-runtime-gamma", "linux").unwrap(),
        "java-runtime-gamma",
        "17.0.15",
        "a",
    );
    fake_runtime(
        &dirs.runtime_dir("jre-legacy", "linux").unwrap(),
        "jre-legacy",
        "8u51-cacert462b08",
        "b",
    );
    // No executable, no marker: not listed.
    write(
        &dirs
            .runtime_dir("java-runtime-delta", "linux")
            .unwrap()
            .join(MARKER_FILE),
        marker_json("java-runtime-delta", "linux", "21.0.7", "c").as_bytes(),
    );
    write(
        &dirs
            .runtime_dir("java-runtime-epsilon", "linux")
            .unwrap()
            .join("bin")
            .join("java"),
        b"java",
    );
    let listed = list_managed(&dirs, Os::Linux);
    let names: Vec<(&str, u32)> = listed
        .iter()
        .map(|r| (r.component.as_str(), r.major))
        .collect();
    assert_eq!(names, [("java-runtime-gamma", 17), ("jre-legacy", 8)]);
    assert_eq!(
        listed[1].executable,
        dirs.runtime_dir("jre-legacy", "linux")
            .unwrap()
            .join("bin")
            .join("java")
    );
}

#[test]
fn finds_the_runtime_of_an_executable() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = DataDirs::new(tmp.path());
    let root = dirs
        .runtime_dir("java-runtime-delta", "windows-x64")
        .unwrap();
    fake_runtime(&root, "java-runtime-delta", "21.0.7", "a");
    assert_eq!(
        managed_runtime_of(&dirs, &root.join("bin").join("javaw.exe")),
        Some(("java-runtime-delta".to_owned(), "windows-x64".to_owned()))
    );
    assert_eq!(
        managed_runtime_of(&dirs, &dirs.runtimes().join("java")),
        None
    );
    assert_eq!(
        managed_runtime_of(&dirs, &staged_dir(&root).join("bin").join("java")),
        None
    );
    assert_eq!(
        managed_runtime_of(&dirs, Path::new("/usr/lib/jvm/java-21/bin/java")),
        None
    );
}

#[test]
fn picks_the_runtimes_to_stage() {
    let runtime = |component: &str, sha1: &str, staged: Option<&str>| ManagedRuntime {
        component: component.to_owned(),
        platform: "windows-x64".to_owned(),
        root: PathBuf::from(format!("/rt/{component}")),
        version: "1".to_owned(),
        manifest_sha1: sha1.to_owned(),
        major: 21,
        executable: PathBuf::from("/rt/bin/javaw.exe"),
        arch: Arch::X86_64,
        staged: staged.map(|sha1| StagedRuntime {
            version: "2".to_owned(),
            manifest_sha1: sha1.to_owned(),
        }),
    };
    let installed = vec![
        runtime("java-runtime-delta", "old", None),
        runtime("java-runtime-gamma", "current", None),
        runtime("jre-legacy", "old", Some("new")),
        runtime("java-runtime-epsilon", "old", None),
    ];
    let mut index = index_with(
        "windows-x64",
        "java-runtime-delta",
        entry("new", "21.0.8", "https://x/d.json", 1),
    );
    let components = index.get_mut("windows-x64").unwrap();
    components.insert(
        "java-runtime-gamma".into(),
        vec![entry("current", "17.0.15", "https://x/g.json", 1)],
    );
    components.insert(
        "jre-legacy".into(),
        vec![entry("new", "8u52", "https://x/l.json", 1)],
    );
    components.insert(
        "java-runtime-epsilon".into(),
        vec![entry("new", "25.0.2", "https://x/e.json", 1)],
    );
    // Delta has an update; gamma is current; legacy's update is staged already; epsilon is not
    // wanted (no instance uses it).
    let picked: Vec<&str> = runtimes_to_stage(&installed, &index, |r| {
        r.component != "java-runtime-epsilon"
    })
    .into_iter()
    .map(|(runtime, _)| runtime.component.as_str())
    .collect();
    assert_eq!(picked, ["java-runtime-delta"]);
    assert_eq!(
        update_for(&index, &installed[2]).map(|e| e.version.name.as_str()),
        Some("8u52")
    );
    assert!(update_for(&index, &installed[1]).is_none());
    // A platform or component Mojang does not list has no update.
    let mut other = installed[0].clone();
    other.platform = "linux".into();
    assert!(update_for(&index, &other).is_none());
}

#[test]
fn update_checks_are_daily() {
    let hour = 60 * 60 * 1000;
    let now = 1_800_000_000_000;
    let mut schedule = UpdateSchedule::default();
    assert!(schedule.due(now));
    schedule.record(now, true);
    assert!(!schedule.due(now + 23 * hour));
    assert!(schedule.due(now + 25 * hour));
    // A failure waits an hour before the next try.
    schedule.record(now + 25 * hour, false);
    assert!(!schedule.due(now + 25 * hour + hour / 2));
    assert!(schedule.due(now + 26 * hour + 1));
    // A clock that jumped back a long way does not stop the checks.
    assert!(schedule.due(now - 400 * 24 * hour));

    let tmp = tempfile::tempdir().unwrap();
    let dirs = DataDirs::new(tmp.path());
    assert_eq!(UpdateSchedule::load(&dirs), UpdateSchedule::default());
    std::fs::create_dir_all(dirs.meta()).unwrap();
    schedule.save(&dirs).unwrap();
    assert_eq!(UpdateSchedule::load(&dirs), schedule);
}

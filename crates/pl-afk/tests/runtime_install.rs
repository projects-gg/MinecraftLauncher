//! Client installs from a local zip served over loopback HTTP, verified by sha256 through pl-net.

use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use pl_afk::{AfkClientBuild, AfkClientRelease, AfkError, AfkRuntime, CLIENT_EXE_NAME};
use pl_net::{DownloadOptions, Downloader, HashAlgo, Http, HttpConfig, hash_bytes};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// A zip like the publish script makes: the executable and a native library at the root, plus a
/// zip-slip entry that must never be extracted.
fn package(exe_content: &[u8]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.start_file(CLIENT_EXE_NAME, options).unwrap();
    zip.write_all(exe_content).unwrap();
    zip.start_file("libMagick.Native-Q16-x64.dll", options)
        .unwrap();
    zip.write_all(b"native").unwrap();
    zip.add_directory("runtimes/", options).unwrap();
    zip.start_file("runtimes/extra.txt", options).unwrap();
    zip.write_all(b"extra").unwrap();
    zip.start_file("../escape.txt", options).unwrap();
    zip.write_all(b"evil").unwrap();
    zip.finish().unwrap().into_inner()
}

/// Serves `body` for `GET /<path>` on 127.0.0.1; any other path is a 404.
async fn serve(path: &'static str, body: Vec<u8>) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let body = Arc::new(body);
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let body = body.clone();
            tokio::spawn(async move {
                let mut request = Vec::new();
                let mut buf = [0u8; 1024];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    match socket.read(&mut buf).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => request.extend_from_slice(&buf[..n]),
                    }
                }
                let wanted = format!("GET /{path} ");
                let response = if request.starts_with(wanted.as_bytes()) {
                    let mut r = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/zip\r\nConnection: close\r\n\r\n",
                        body.len()
                    )
                    .into_bytes();
                    r.extend_from_slice(&body);
                    r
                } else {
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        .to_vec()
                };
                let _ = socket.write_all(&response).await;
                let _ = socket.shutdown().await;
            });
        }
    });
    format!("http://{addr}/{path}")
}

fn downloader() -> Downloader {
    let mut config = HttpConfig::new("pl-afk-test");
    config.retries = 0;
    Downloader::new(Http::new(config).unwrap())
}

fn release(version: &str, urls: Vec<String>, zip: &[u8]) -> AfkClientRelease {
    AfkClientRelease {
        version: version.to_owned(),
        build: AfkClientBuild {
            urls,
            sha256: hash_bytes(zip, HashAlgo::Sha256).to_uppercase(),
            size: zip.len() as u64,
        },
    }
}

#[tokio::test]
async fn installs_verifies_and_selects_a_release() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    let zip = package(b"client v1");
    // A dead mirror first: the download falls through to the working one.
    let url = serve("afk/v1/windows-x64.zip", zip.clone()).await;
    let dead = url.replace("windows-x64.zip", "missing.zip");
    let runtime = AfkRuntime::new(&bin);

    let client = runtime
        .install(
            &downloader(),
            &release("v1", vec![dead, url], &zip),
            &DownloadOptions::default(),
        )
        .await
        .unwrap();
    assert_eq!(client.version, "v1");
    assert_eq!(client.path, bin.join("v1").join(CLIENT_EXE_NAME));
    assert_eq!(std::fs::read(&client.path).unwrap(), b"client v1");
    assert!(bin.join("v1").join("runtimes").join("extra.txt").is_file());
    assert!(!root.path().join("escape.txt").exists() && !bin.join("escape.txt").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&client.path)
            .unwrap()
            .permissions()
            .mode();
        assert_ne!(mode & 0o111, 0, "the client must be executable");
    }
    assert_eq!(runtime.installed_versions(), vec!["v1"]);
    assert_eq!(runtime.current_version().as_deref(), Some("v1"));
    assert_eq!(runtime.resolve(), Some(client.clone()));
    assert!(
        !bin.join(".staging").exists()
            || std::fs::read_dir(bin.join(".staging"))
                .unwrap()
                .next()
                .is_none(),
        "no staging leftovers"
    );

    // Installing again only reselects.
    let again = runtime
        .install(
            &downloader(),
            &release("v1", vec!["http://127.0.0.1:9/x".into()], &zip),
            &DownloadOptions::default(),
        )
        .await
        .unwrap();
    assert_eq!(again, client);
}

#[tokio::test]
async fn checksum_mismatch_keeps_the_previous_version() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    let runtime = AfkRuntime::new(&bin);
    let good = package(b"client v1");
    let url_v1 = serve("v1.zip", good.clone()).await;
    runtime
        .install(
            &downloader(),
            &release("v1", vec![url_v1], &good),
            &DownloadOptions::default(),
        )
        .await
        .unwrap();

    let tampered = package(b"tampered v2");
    let url_v2 = serve("v2.zip", tampered).await;
    let mut wrong = release("v2", vec![url_v2], &good);
    wrong.build.size = good.len() as u64;
    let error = runtime
        .install(&downloader(), &wrong, &DownloadOptions::default())
        .await
        .unwrap_err();
    assert!(matches!(error, AfkError::Net(_)), "{error}");
    assert!(!bin.join("v2").exists());
    assert_eq!(runtime.resolve().unwrap().version, "v1");

    // `ensure` falls back to the installed build when the update cannot be installed.
    let client = runtime
        .ensure(
            Some(&downloader()),
            Some(&wrong),
            &DownloadOptions::default(),
        )
        .await
        .unwrap();
    assert_eq!(client.version, "v1");
}

#[tokio::test]
async fn offline_uses_the_newest_installed_version() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    let runtime = AfkRuntime::new(&bin);

    // Nothing installed and nothing reachable.
    let unreachable = release("v9", vec!["http://127.0.0.1:9/v9.zip".into()], b"zip");
    let error = runtime
        .ensure(
            Some(&downloader()),
            Some(&unreachable),
            &DownloadOptions::default(),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, AfkError::Net(_)));
    assert!(matches!(
        runtime
            .ensure(None, None, &DownloadOptions::default())
            .await,
        Err(AfkError::ClientNotInstalled)
    ));

    for version in ["v2", "v10"] {
        let zip = package(version.as_bytes());
        let url = serve("c.zip", zip.clone()).await;
        runtime
            .install(
                &downloader(),
                &release(version, vec![url], &zip),
                &DownloadOptions::default(),
            )
            .await
            .unwrap();
    }
    assert_eq!(runtime.installed_versions(), vec!["v10", "v2"]);
    std::fs::remove_file(bin.join("current.json")).unwrap();
    let client = runtime
        .ensure(
            Some(&downloader()),
            Some(&unreachable),
            &DownloadOptions::default(),
        )
        .await
        .unwrap();
    assert_eq!(client.version, "v10", "natural order: v10 is newer than v2");
}

#[tokio::test]
async fn rejects_unsafe_releases() {
    let root = tempfile::tempdir().unwrap();
    let runtime = AfkRuntime::new(root.path().join("bin"));
    for version in ["../escape", "", "a/b"] {
        let bad = release(version, vec!["http://127.0.0.1:9/x".into()], b"x");
        assert!(matches!(
            runtime
                .install(&downloader(), &bad, &DownloadOptions::default())
                .await,
            Err(AfkError::InvalidRelease(_))
        ));
    }
    let mut no_hash = release("v1", vec!["http://127.0.0.1:9/x".into()], b"x");
    no_hash.build.sha256.clear();
    assert!(matches!(
        runtime
            .install(&downloader(), &no_hash, &DownloadOptions::default())
            .await,
        Err(AfkError::InvalidRelease(_))
    ));
}

#[tokio::test]
async fn package_without_the_client_is_rejected() {
    let root = tempfile::tempdir().unwrap();
    let runtime = AfkRuntime::new(root.path().join("bin"));
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file("readme.txt", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"no client here").unwrap();
    let zip = zip.finish().unwrap().into_inner();
    let url = serve("empty.zip", zip.clone()).await;
    let error = runtime
        .install(
            &downloader(),
            &release("v1", vec![url], &zip),
            &DownloadOptions::default(),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, AfkError::InvalidRelease(_)), "{error}");
    assert!(runtime.installed_versions().is_empty());
}

#[tokio::test]
async fn dev_override_and_pruning() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    let runtime = AfkRuntime::new(&bin);
    for version in ["v1", "v2", "v3", "v4"] {
        let zip = package(version.as_bytes());
        let url = serve("p.zip", zip.clone()).await;
        runtime
            .install(
                &downloader(),
                &release(version, vec![url], &zip),
                &DownloadOptions::default(),
            )
            .await
            .unwrap();
    }
    // v4 is selected, v1 runs somewhere: keep v4, v3 (newest two) and v1.
    let removed = runtime.prune(&["v1".to_owned()]);
    assert_eq!(removed, vec!["v2"]);
    assert_eq!(runtime.installed_versions(), vec!["v4", "v3", "v1"]);

    let dev_dir = root.path().join("dev");
    std::fs::create_dir_all(&dev_dir).unwrap();
    let with_dev = runtime.clone().with_dev_dir(Some(dev_dir.clone()));
    assert_eq!(
        with_dev.resolve().unwrap().version,
        "v4",
        "empty dev folder is ignored"
    );
    std::fs::write(dev_dir.join(CLIENT_EXE_NAME), b"dev").unwrap();
    let dev = with_dev.resolve().unwrap();
    assert!(dev.dev);
    assert_eq!(dev.path, dev_dir.join(CLIENT_EXE_NAME));
    let ensured = with_dev
        .ensure(None, None, &DownloadOptions::default())
        .await
        .unwrap();
    assert!(ensured.dev);
    assert!(Path::new(&ensured.path).is_file());
}

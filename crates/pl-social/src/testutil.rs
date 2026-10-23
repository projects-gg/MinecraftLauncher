//! Test helpers: a tiny HTTP/1.1 server on 127.0.0.1 with a canned-reply handler.

use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[derive(Debug, Clone)]
pub(crate) struct Recorded {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Recorded {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    pub fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body).expect("request body is JSON")
    }

    pub fn body_text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

pub(crate) struct MockReply {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl MockReply {
    pub fn json(status: u16, body: &str) -> Self {
        Self {
            status,
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: body.as_bytes().to_vec(),
        }
    }

    /// A reply without a body (`204`).
    pub fn empty(status: u16) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    /// Any bytes (SKIN1's textures).
    pub fn bytes(status: u16, content_type: &str, body: Vec<u8>) -> Self {
        Self {
            status,
            headers: vec![("Content-Type".into(), content_type.to_owned())],
            body,
        }
    }

    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }
}

type Handler = Arc<dyn Fn(&Recorded) -> MockReply + Send + Sync>;

pub(crate) struct MockServer {
    base: String,
    log: Arc<Mutex<Vec<Recorded>>>,
    task: tokio::task::JoinHandle<()>,
}

impl MockServer {
    pub async fn start(handler: impl Fn(&Recorded) -> MockReply + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/", listener.local_addr().unwrap());
        let log = Arc::new(Mutex::new(Vec::new()));
        let handler: Handler = Arc::new(handler);
        let task = tokio::spawn({
            let log = log.clone();
            async move {
                while let Ok((stream, _)) = listener.accept().await {
                    let log = log.clone();
                    let handler = handler.clone();
                    tokio::spawn(async move {
                        let _ = serve(stream, &log, &handler).await;
                    });
                }
            }
        });
        Self { base, log, task }
    }

    pub fn base(&self) -> String {
        self.base.clone()
    }

    pub fn requests(&self) -> Vec<Recorded> {
        self.log.lock().unwrap().clone()
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn serve(
    mut stream: TcpStream,
    log: &Mutex<Vec<Recorded>>,
    handler: &Handler,
) -> std::io::Result<()> {
    loop {
        let mut buffer = Vec::new();
        let mut chunk = [0u8; 4096];
        let header_end = loop {
            let read = stream.read(&mut chunk).await?;
            if read == 0 {
                return Ok(());
            }
            buffer.extend_from_slice(&chunk[..read]);
            if let Some(position) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
                break position;
            }
        };
        let head = String::from_utf8_lossy(&buffer[..header_end]).into_owned();
        let mut lines = head.split("\r\n");
        let request_line = lines.next().unwrap_or_default();
        let mut parts = request_line.split(' ');
        let method = parts.next().unwrap_or_default().to_owned();
        let path = parts.next().unwrap_or_default().to_owned();
        let headers: Vec<(String, String)> = lines
            .filter_map(|line| line.split_once(':'))
            .map(|(key, value)| (key.trim().to_owned(), value.trim().to_owned()))
            .collect();
        let length = headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case("content-length"))
            .and_then(|(_, value)| value.parse::<usize>().ok())
            .unwrap_or(0);
        let mut body = buffer[header_end + 4..].to_vec();
        while body.len() < length {
            let read = stream.read(&mut chunk).await?;
            if read == 0 {
                break;
            }
            body.extend_from_slice(&chunk[..read]);
        }
        let recorded = Recorded {
            method,
            path,
            headers,
            body,
        };
        let reply = handler(&recorded);
        log.lock().unwrap().push(recorded);
        let mut response = format!("HTTP/1.1 {} X\r\n", reply.status);
        for (name, value) in &reply.headers {
            response.push_str(&format!("{name}: {value}\r\n"));
        }
        response.push_str(&format!("Content-Length: {}\r\n\r\n", reply.body.len()));
        let mut bytes = response.into_bytes();
        bytes.extend_from_slice(&reply.body);
        stream.write_all(&bytes).await?;
    }
}

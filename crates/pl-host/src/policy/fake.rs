//! In-memory paths for the tests: host paths that accept the streams guest paths open, with a
//! switch that kills a path and every stream on it at once (a relay going away).

use std::borrow::Cow;
use std::io;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

use tokio::io::{AsyncRead, AsyncWrite, DuplexStream, ReadBuf};
use tokio::sync::{Notify, mpsc};

use super::env::TurnAllocation;
use crate::transport::{
    BoxFuture, BoxStream, GuestPath, HostPath, PathAddress, PathError, PathKind, Step,
};

/// A switch that kills a path and its streams.
#[derive(Default)]
pub(crate) struct Kill {
    dead: AtomicBool,
    notify: Notify,
    wakers: Mutex<Vec<Waker>>,
}

impl Kill {
    pub fn kill(&self) {
        self.dead.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
        for waker in self.wakers.lock().unwrap().drain(..) {
            waker.wake();
        }
    }

    pub fn is_dead(&self) -> bool {
        self.dead.load(Ordering::SeqCst)
    }

    fn watch(&self, waker: &Waker) {
        let mut wakers = self.wakers.lock().unwrap();
        if !wakers.iter().any(|w| w.will_wake(waker)) {
            wakers.push(waker.clone());
        }
    }

    pub async fn dead(&self) {
        loop {
            let notified = self.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.is_dead() {
                return;
            }
            notified.await;
        }
    }
}

/// One end of an in-memory stream that fails once its path is killed.
pub(crate) struct KillStream {
    inner: DuplexStream,
    kill: Arc<Kill>,
    rx: Option<Arc<AtomicU64>>,
}

fn reset() -> io::Error {
    io::Error::new(io::ErrorKind::ConnectionReset, "path killed")
}

impl AsyncRead for KillStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.kill.is_dead() {
            return Poll::Ready(Err(reset()));
        }
        self.kill.watch(cx.waker());
        let before = buf.filled().len();
        let polled = Pin::new(&mut self.inner).poll_read(cx, buf);
        if let (Poll::Ready(Ok(())), Some(rx)) = (&polled, &self.rx) {
            rx.fetch_add((buf.filled().len() - before) as u64, Ordering::Relaxed);
        }
        polled
    }
}

impl AsyncWrite for KillStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        if self.kill.is_dead() {
            return Poll::Ready(Err(reset()));
        }
        self.kill.watch(cx.waker());
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if self.kill.is_dead() {
            return Poll::Ready(Err(reset()));
        }
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

/// The host's end of a fake relay path.
pub(crate) struct FakeHostPath {
    pub kind: PathKind,
    pub address: PathAddress,
    tx: mpsc::UnboundedSender<BoxStream>,
    rx: tokio::sync::Mutex<mpsc::UnboundedReceiver<BoxStream>>,
    pub kill: Arc<Kill>,
    pub admitted: Mutex<Vec<SocketAddrV4>>,
    pub received: Arc<AtomicU64>,
    pub opened: AtomicU32,
}

impl FakeHostPath {
    fn new(kind: PathKind, address: PathAddress) -> Arc<Self> {
        let (tx, rx) = mpsc::unbounded_channel();
        Arc::new(Self {
            kind,
            address,
            tx,
            rx: tokio::sync::Mutex::new(rx),
            kill: Arc::new(Kill::default()),
            admitted: Mutex::new(Vec::new()),
            received: Arc::new(AtomicU64::new(0)),
            opened: AtomicU32::new(0),
        })
    }

    /// A stream from a guest: the guest's end, with the host's end queued for `accept`.
    pub fn connect(&self) -> Result<BoxStream, PathError> {
        let (step, code) = match self.kind {
            PathKind::Turn => (Step::Quic, "turn_quic_timeout"),
            PathKind::E4mc => (Step::Tcp, "e4mc_tcp_refused"),
        };
        if self.kill.is_dead() {
            return Err(PathError {
                step,
                code: Cow::Borrowed(code),
            });
        }
        let (guest, host) = tokio::io::duplex(1 << 20);
        let host = KillStream {
            inner: host,
            kill: self.kill.clone(),
            rx: Some(self.received.clone()),
        };
        self.opened.fetch_add(1, Ordering::Relaxed);
        self.tx.send(Box::new(host)).map_err(|_| PathError {
            step,
            code: Cow::Borrowed(code),
        })?;
        Ok(Box::new(KillStream {
            inner: guest,
            kill: self.kill.clone(),
            rx: None,
        }))
    }

    /// A stream from someone who is not a guest: raw bytes, then whatever the host does.
    pub fn connect_raw(&self) -> DuplexStream {
        let (near, far) = tokio::io::duplex(1 << 16);
        let far = KillStream {
            inner: far,
            kill: self.kill.clone(),
            rx: None,
        };
        let _ = self.tx.send(Box::new(far));
        near
    }
}

impl HostPath for FakeHostPath {
    fn kind(&self) -> PathKind {
        self.kind
    }

    fn address(&self) -> PathAddress {
        self.address.clone()
    }

    fn admit_peer(&self, relay: SocketAddrV4) -> BoxFuture<'_, Result<(), PathError>> {
        self.admitted.lock().unwrap().push(relay);
        Box::pin(async { Ok(()) })
    }

    fn forget_peer(&self, relay: SocketAddrV4) {
        self.admitted.lock().unwrap().retain(|r| *r != relay);
    }

    fn accept(&self) -> BoxFuture<'_, Result<BoxStream, PathError>> {
        Box::pin(async move {
            let lost = PathError {
                step: Step::Domain,
                code: Cow::Borrowed("e4mc_closed"),
            };
            let mut rx = self.rx.lock().await;
            tokio::select! {
                stream = rx.recv() => stream.ok_or(lost),
                () = self.kill.dead() => Err(lost),
            }
        })
    }

    fn closed(&self) -> BoxFuture<'_, PathError> {
        Box::pin(async move {
            self.kill.dead().await;
            PathError {
                step: Step::Domain,
                code: Cow::Borrowed("e4mc_closed"),
            }
        })
    }

    fn rx_bytes(&self) -> u64 {
        self.received.load(Ordering::Relaxed)
    }

    fn close(&self) -> BoxFuture<'_, ()> {
        self.kill.kill();
        Box::pin(async {})
    }
}

/// The guest's end of a fake path to one host path.
pub(crate) struct FakeGuestPath {
    pub host: Arc<FakeHostPath>,
}

impl GuestPath for FakeGuestPath {
    fn kind(&self) -> PathKind {
        self.host.kind
    }

    fn open(&self) -> BoxFuture<'_, Result<BoxStream, PathError>> {
        Box::pin(async move { self.host.connect() })
    }

    fn explain_reply(&self, _packet: &[u8]) -> Option<Cow<'static, str>> {
        None
    }

    fn closed(&self) -> BoxFuture<'_, PathError> {
        Box::pin(async move {
            self.host.kill.dead().await;
            PathError {
                step: Step::Tcp,
                code: Cow::Borrowed("e4mc_closed"),
            }
        })
    }

    fn rx_bytes(&self) -> u64 {
        0
    }

    fn close(&self) -> BoxFuture<'_, ()> {
        Box::pin(async {})
    }
}

/// Every fake host path, found by its address.
#[derive(Default)]
pub(crate) struct FakeNet {
    paths: Mutex<Vec<Arc<FakeHostPath>>>,
    next: AtomicU32,
}

impl FakeNet {
    pub fn host_path(&self, kind: PathKind) -> Arc<FakeHostPath> {
        let n = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let address = match kind {
            PathKind::Turn => PathAddress::Turn {
                relay: SocketAddrV4::new(Ipv4Addr::new(162, 159, 207, 10), 50_000 + n as u16),
                pin: [n as u8; 32],
            },
            PathKind::E4mc => PathAddress::E4mc {
                domain: format!("world-{n}.eu.e4mc.link"),
            },
        };
        let path = FakeHostPath::new(kind, address);
        self.paths.lock().unwrap().push(path.clone());
        path
    }

    pub fn find(&self, address: &PathAddress) -> Option<Arc<FakeHostPath>> {
        self.paths
            .lock()
            .unwrap()
            .iter()
            .find(|p| p.address == *address)
            .cloned()
    }

    pub fn all(&self) -> Vec<Arc<FakeHostPath>> {
        self.paths.lock().unwrap().clone()
    }

    /// A guest path to whatever host path has this address.
    pub fn guest_path(&self, address: &PathAddress) -> Option<Arc<dyn GuestPath>> {
        let host = self.find(address)?;
        (!host.kill.is_dead()).then(|| Arc::new(FakeGuestPath { host }) as Arc<dyn GuestPath>)
    }
}

/// A guest's fake TURN allocation.
pub(crate) struct FakeAllocation {
    pub net: Arc<FakeNet>,
    pub relay: SocketAddrV4,
}

impl TurnAllocation for FakeAllocation {
    fn relay(&self) -> SocketAddrV4 {
        self.relay
    }

    fn connect(
        &self,
        host_relay: SocketAddrV4,
        pin: [u8; 32],
    ) -> BoxFuture<'_, Result<Arc<dyn GuestPath>, PathError>> {
        Box::pin(async move {
            self.net
                .guest_path(&PathAddress::Turn {
                    relay: host_relay,
                    pin,
                })
                .ok_or(PathError {
                    step: Step::Quic,
                    code: Cow::Borrowed("turn_quic_timeout"),
                })
        })
    }
}

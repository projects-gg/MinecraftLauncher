//! QUIC pieces both paths use: the stream type, a UDP socket that never logs, the transport
//! settings and the certificate pin.

use std::fmt;
use std::future::Future;
use std::io::{self, IoSliceMut};
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll, ready};
use std::time::Duration;

use quinn::udp::{RecvMeta, Transmit};
use quinn::{
    AsyncUdpSocket, ConnectionError, IdleTimeout, RecvStream, SendStream, TransportConfig,
    TransportErrorCode, UdpPoller, VarInt,
};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::WebPkiSupportedAlgorithms;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{CertificateError, DigitallySignedStruct, SignatureScheme};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// One bidirectional QUIC stream as a byte stream. Shutting down the write half finishes the stream.
pub(crate) struct QuicStream {
    send: SendStream,
    recv: RecvStream,
    finished: bool,
}

impl QuicStream {
    pub(crate) fn new(send: SendStream, recv: RecvStream) -> Self {
        Self {
            send,
            recv,
            finished: false,
        }
    }
}

impl AsyncRead for QuicStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        AsyncRead::poll_read(Pin::new(&mut self.get_mut().recv), cx, buf)
    }
}

impl AsyncWrite for QuicStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        AsyncWrite::poll_write(Pin::new(&mut self.get_mut().send), cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if !this.finished {
            this.finished = true;
            // A stream the peer already stopped or reset is over either way.
            let _ = this.send.finish();
        }
        Poll::Ready(Ok(()))
    }
}

type Writable = Pin<Box<dyn Future<Output = io::Result<()>> + Send + Sync>>;

/// Waits for a socket to become writable; one per QUIC connection (each keeps its own waker).
pub(crate) struct WritablePoller {
    make: Box<dyn Fn() -> Writable + Send + Sync>,
    pending: Option<Writable>,
}

impl WritablePoller {
    pub(crate) fn new(make: impl Fn() -> Writable + Send + Sync + 'static) -> Self {
        Self {
            make: Box::new(make),
            pending: None,
        }
    }
}

impl fmt::Debug for WritablePoller {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WritablePoller")
    }
}

impl UdpPoller for WritablePoller {
    fn poll_writable(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let pending = this.pending.get_or_insert_with(|| (this.make)());
        let result = pending.as_mut().poll(cx);
        if result.is_ready() {
            this.pending = None;
        }
        result
    }
}

/// A poller for sockets that never push back (TURN over TCP queues or drops).
#[derive(Debug)]
pub(crate) struct AlwaysWritable;

impl UdpPoller for AlwaysWritable {
    fn poll_writable(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

pub(crate) fn recv_meta(addr: SocketAddr, len: usize) -> RecvMeta {
    RecvMeta {
        addr,
        len,
        stride: len,
        ecn: None,
        dst_ip: None,
    }
}

/// A plain UDP socket for QUIC to the e4mc relay. Unlike quinn's own socket it never logs: a failed
/// send is dropped like a lost datagram (quinn's would log the relay's address).
#[derive(Debug)]
pub(crate) struct PlainUdp {
    io: Arc<tokio::net::UdpSocket>,
}

impl PlainUdp {
    /// A socket to talk to `remote`.
    pub(crate) fn bind(remote: SocketAddr) -> io::Result<Self> {
        let socket = std::net::UdpSocket::bind(super::net::local_for(remote))?;
        socket.set_nonblocking(true)?;
        Ok(Self {
            io: Arc::new(tokio::net::UdpSocket::from_std(socket)?),
        })
    }
}

impl AsyncUdpSocket for PlainUdp {
    fn create_io_poller(self: Arc<Self>) -> Pin<Box<dyn UdpPoller>> {
        let io = self.io.clone();
        Box::pin(WritablePoller::new(move || {
            let io = io.clone();
            Box::pin(async move { io.writable().await })
        }))
    }

    fn try_send(&self, transmit: &Transmit<'_>) -> io::Result<()> {
        match self.io.try_send_to(transmit.contents, transmit.destination) {
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Err(error),
            _ => Ok(()),
        }
    }

    fn poll_recv(
        &self,
        cx: &mut Context<'_>,
        bufs: &mut [IoSliceMut<'_>],
        meta: &mut [RecvMeta],
    ) -> Poll<io::Result<usize>> {
        let (Some(first), Some(first_meta)) = (bufs.first_mut(), meta.first_mut()) else {
            return Poll::Ready(Ok(0));
        };
        let mut buf = ReadBuf::new(first);
        let from = ready!(self.io.poll_recv_from(cx, &mut buf))?;
        *first_meta = recv_meta(from, buf.filled().len());
        Poll::Ready(Ok(1))
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.io.local_addr()
    }
}

/// QUIC settings of one connection.
pub(crate) struct Settings {
    pub(crate) idle_timeout: Duration,
    pub(crate) keep_alive: Duration,
    /// Bidirectional streams the peer may have open at once.
    pub(crate) peer_bidi_streams: u32,
    pub(crate) initial_mtu: u16,
}

/// No datagrams, no unidirectional streams, no path MTU discovery (the MTU stays at its initial
/// value, which fits inside TURN's framing).
pub(crate) fn transport(settings: &Settings) -> TransportConfig {
    let mut transport = TransportConfig::default();
    transport
        .max_idle_timeout(IdleTimeout::try_from(settings.idle_timeout).ok())
        .keep_alive_interval(Some(settings.keep_alive))
        .max_concurrent_bidi_streams(VarInt::from_u32(settings.peer_bidi_streams))
        .max_concurrent_uni_streams(VarInt::from_u32(0))
        .datagram_receive_buffer_size(None)
        .initial_mtu(settings.initial_mtu)
        .mtu_discovery_config(None);
    transport
}

/// Whether a connection failed because the server refused our ALPN (TLS alert 120).
pub(crate) fn alpn_refused(error: &ConnectionError) -> bool {
    const NO_APPLICATION_PROTOCOL: u8 = 120;
    let code = TransportErrorCode::crypto(NO_APPLICATION_PROTOCOL);
    match error {
        ConnectionError::TransportError(error) => error.code == code,
        ConnectionError::ConnectionClosed(close) => close.error_code == code,
        _ => false,
    }
}

/// SHA-256 of a certificate's DER: the pin a guest checks (PROTOCOL.md §2, §5.6).
pub(crate) fn pin_of(certificate_der: &[u8]) -> [u8; 32] {
    let digest = Sha256::digest(certificate_der);
    let mut pin = [0u8; 32];
    pin.copy_from_slice(&digest);
    pin
}

/// Accepts exactly the certificate whose SHA-256 is the pin, and checks that the server holds its
/// key; nothing else (no names, no dates, no roots).
pub(crate) struct PinVerifier {
    pin: [u8; 32],
    algorithms: WebPkiSupportedAlgorithms,
    mismatch: Arc<AtomicBool>,
}

impl PinVerifier {
    pub(crate) fn new(pin: [u8; 32], mismatch: Arc<AtomicBool>) -> Self {
        Self {
            pin,
            algorithms: super::net::provider().signature_verification_algorithms,
            mismatch,
        }
    }
}

impl fmt::Debug for PinVerifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PinVerifier")
    }
}

impl ServerCertVerifier for PinVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if pin_of(end_entity) == self.pin {
            Ok(ServerCertVerified::assertion())
        } else {
            self.mismatch.store(true, Ordering::Release);
            Err(rustls::Error::InvalidCertificate(
                CertificateError::ApplicationVerificationFailure,
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        // QUIC is TLS 1.3 only.
        Err(rustls::Error::General("TLS 1.2 is not used".into()))
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

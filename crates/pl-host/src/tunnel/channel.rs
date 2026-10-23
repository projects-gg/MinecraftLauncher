//! One PLJ1 stream in transport mode (PROTOCOL.md §6.3, §6.6).
//!
//! A reader task decrypts frames into [`In`] items and a writer task encrypts [`Out`] items into
//! frames, so the owner only deals with two queues and can wait on them in `select!`. The tasks
//! also keep the stream alive: a side pings after 1 s in which it sent nothing (one ping at a
//! time), answers pings at once, and a stream that received nothing for 3.5 s is broken.

use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::sync::{Notify, mpsc};
use tokio::task::AbortHandle;
use tokio::time::{Instant, sleep_until};

use super::control::{Control, ControlError};
use super::ids::random;
use super::noise::Cipher;
use super::record::{ControlJoiner, Record, frame, frame_len_ok, split_control};
use crate::policy::Policy;
use crate::report::RttHistogram;
use crate::transport::BoxStream;

/// What the reader hands its owner.
#[derive(Debug)]
pub(crate) enum In {
    /// A whole control message, or why it could not be read (unknown `t`, malformed fields).
    Control(Result<Control, ControlError>),
    Data {
        offset: u64,
        bytes: Vec<u8>,
    },
    Ack(u64),
    /// The stream is over: nothing more comes.
    Ended(Ended),
}

/// Why a stream ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ended {
    /// The peer ended its write side.
    Eof,
    /// The transport failed.
    Io,
    /// Nothing arrived for the silence limit.
    Silent,
    /// A frame did not decrypt or a record broke the rules.
    Protocol,
}

/// What the owner gives the writer.
#[derive(Debug)]
pub(crate) enum Out {
    Control(Control),
    Data {
        offset: u64,
        bytes: Vec<u8>,
    },
    Ack(u64),
    /// End the write side after everything queued before it.
    Finish,
}

/// Keepalive timing.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Keepalive {
    pub ping_after_idle: Duration,
    pub dead_after_silence: Duration,
}

impl Keepalive {
    pub fn from_policy(policy: &Policy) -> Self {
        Self {
            ping_after_idle: Duration::from_millis(policy.plj.ping_after_idle_ms),
            dead_after_silence: Duration::from_millis(policy.plj.dead_after_silence_ms),
        }
    }
}

struct Shared {
    /// The ping in flight and when it went.
    ping: Mutex<Option<([u8; 8], Instant)>>,
    /// The ping was answered: the writer may ping again.
    pong_seen: Notify,
    write_failed: Notify,
    rtt: Option<Arc<Mutex<RttHistogram>>>,
}

/// Queue depth towards the writer (records).
const OUT_QUEUE: usize = 64;
/// Queue depth towards the owner (records).
const IN_QUEUE: usize = 64;

/// A PLJ1 stream after the handshake.
pub(crate) struct Channel {
    pub tx: mpsc::Sender<Out>,
    pub rx: mpsc::Receiver<In>,
    reader: AbortHandle,
}

impl Channel {
    /// Starts the reader and writer tasks on a stream whose handshake is done. `rtt` collects the
    /// ping round trips (the session report's median).
    pub fn spawn(
        stream: BoxStream,
        cipher: Cipher,
        keepalive: Keepalive,
        rtt: Option<Arc<Mutex<RttHistogram>>>,
    ) -> Self {
        let (reader, writer) = tokio::io::split(stream);
        let cipher = Arc::new(cipher);
        let shared = Arc::new(Shared {
            ping: Mutex::new(None),
            pong_seen: Notify::new(),
            write_failed: Notify::new(),
            rtt,
        });
        let (out_tx, out_rx) = mpsc::channel(OUT_QUEUE);
        let (in_tx, in_rx) = mpsc::channel(IN_QUEUE);
        let (pong_tx, pong_rx) = mpsc::channel(4);
        let reader_task = tokio::spawn(read_loop(
            reader,
            cipher.clone(),
            in_tx,
            pong_tx,
            shared.clone(),
            keepalive,
        ));
        tokio::spawn(write_loop(
            writer, cipher, out_rx, pong_rx, shared, keepalive,
        ));
        Self {
            tx: out_tx,
            rx: in_rx,
            reader: reader_task.abort_handle(),
        }
    }

    /// Sends a control message (waits for room in the queue).
    pub async fn send(&self, message: Control) -> bool {
        self.tx.send(Out::Control(message)).await.is_ok()
    }

    /// Ends the write side once everything queued went out; the stream then ends for the peer.
    pub async fn finish(self) {
        let _ = self.tx.send(Out::Finish).await;
    }

    /// The next item, `None` once the reader is gone.
    pub async fn recv(&mut self) -> Option<In> {
        self.rx.recv().await
    }
}

impl Drop for Channel {
    fn drop(&mut self) {
        // The writer ends on its own when its queue closes (flushing what is queued).
        self.reader.abort();
    }
}

async fn read_frame(reader: &mut ReadHalf<BoxStream>) -> io::Result<Option<Vec<u8>>> {
    let mut len = [0u8; 2];
    match reader.read_exact(&mut len).await {
        Ok(_) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = usize::from(u16::from_be_bytes(len));
    if !frame_len_ok(len) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame length out of range",
        ));
    }
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).await?;
    Ok(Some(buf))
}

async fn read_loop(
    mut reader: ReadHalf<BoxStream>,
    cipher: Arc<Cipher>,
    to_owner: mpsc::Sender<In>,
    pongs: mpsc::Sender<[u8; 8]>,
    shared: Arc<Shared>,
    keepalive: Keepalive,
) {
    let mut nonce: u64 = 0;
    let mut joiner = ControlJoiner::default();
    let mut last_rx = Instant::now();
    let end = loop {
        let frame = tokio::select! {
            frame = read_frame(&mut reader) => frame,
            () = sleep_until(last_rx + keepalive.dead_after_silence) => break Ended::Silent,
            () = shared.write_failed.notified() => break Ended::Io,
        };
        let ciphertext = match frame {
            Ok(Some(ciphertext)) => ciphertext,
            Ok(None) => break Ended::Eof,
            Err(e) if e.kind() == io::ErrorKind::InvalidData => break Ended::Protocol,
            Err(_) => break Ended::Io,
        };
        if nonce == u64::MAX {
            break Ended::Protocol;
        }
        let Some(plain) = cipher.open(nonce, &ciphertext) else {
            break Ended::Protocol;
        };
        nonce += 1;
        last_rx = Instant::now();
        let item = match Record::decode(&plain) {
            Err(_) => break Ended::Protocol,
            Ok(Record::Ctrl(body)) => match joiner.push(true, &body) {
                Ok(Some(json)) => In::Control(Control::parse(&json)),
                Ok(None) => continue,
                Err(_) => break Ended::Protocol,
            },
            Ok(Record::CtrlPart(body)) => match joiner.push(false, &body) {
                Ok(_) => continue,
                Err(_) => break Ended::Protocol,
            },
            Ok(Record::Data { offset, bytes }) => In::Data { offset, bytes },
            Ok(Record::Ack(next)) => In::Ack(next),
            Ok(Record::Ping(body)) => {
                // Answered at once; if the writer is gone the stream is over anyway.
                let _ = pongs.try_send(body);
                continue;
            }
            Ok(Record::Pong(body)) => {
                let mut ping = shared.ping.lock().expect("ping lock");
                if let Some((sent, at)) = *ping
                    && sent == body
                {
                    *ping = None;
                    shared.pong_seen.notify_one();
                    if let Some(rtt) = &shared.rtt {
                        rtt.lock().expect("rtt lock").record(at.elapsed());
                    }
                }
                continue;
            }
            Ok(Record::Unknown(_)) => continue,
        };
        if to_owner.send(item).await.is_err() {
            return;
        }
        // Time spent waiting for our own owner is not the peer's silence.
        last_rx = Instant::now();
    };
    let _ = to_owner.send(In::Ended(end)).await;
}

fn seal_into(cipher: &Cipher, nonce: &mut u64, record: &Record, out: &mut Vec<u8>) -> bool {
    if *nonce == u64::MAX {
        return false;
    }
    let Some(ciphertext) = cipher.seal(*nonce, &record.encode()) else {
        return false;
    };
    *nonce += 1;
    out.extend_from_slice(&frame(&ciphertext));
    true
}

async fn write_loop(
    mut writer: WriteHalf<BoxStream>,
    cipher: Arc<Cipher>,
    mut from_owner: mpsc::Receiver<Out>,
    mut pongs: mpsc::Receiver<[u8; 8]>,
    shared: Arc<Shared>,
    keepalive: Keepalive,
) {
    let mut nonce: u64 = 0;
    let mut last_tx = Instant::now();
    let mut buf = Vec::new();
    loop {
        let ping_idle = shared.ping.lock().expect("ping lock").is_none();
        let records: Vec<Record> = tokio::select! {
            biased;
            Some(body) = pongs.recv() => vec![Record::Pong(body)],
            item = from_owner.recv() => match item {
                None | Some(Out::Finish) => break,
                Some(Out::Control(message)) => match split_control(&message.to_json()) {
                    Ok(records) => records,
                    // A message over 64 KiB is a bug on this side; it is not sent.
                    Err(_) => continue,
                },
                Some(Out::Data { offset, bytes }) => vec![Record::Data { offset, bytes }],
                Some(Out::Ack(next)) => vec![Record::Ack(next)],
            },
            () = sleep_until(last_tx + keepalive.ping_after_idle), if ping_idle => {
                let body: [u8; 8] = random();
                *shared.ping.lock().expect("ping lock") = Some((body, Instant::now()));
                vec![Record::Ping(body)]
            }
            // The ping in flight was answered: look at the idle timer again.
            () = shared.pong_seen.notified(), if !ping_idle => continue,
        };
        if records.is_empty() {
            continue;
        }
        buf.clear();
        if !records
            .iter()
            .all(|record| seal_into(&cipher, &mut nonce, record, &mut buf))
        {
            shared.write_failed.notify_one();
            return;
        }
        if writer.write_all(&buf).await.is_err() || writer.flush().await.is_err() {
            shared.write_failed.notify_one();
            return;
        }
        last_tx = Instant::now();
    }
    let _ = writer.shutdown().await;
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::tunnel::control::{ByeReason, Hello};
    use crate::tunnel::ids::{HostKey, Psk, SessionId, Slot};
    use crate::tunnel::noise::{Initiator, parse_opening, respond};

    /// A connected pair of channels over an in-memory stream.
    pub(crate) fn pair(keepalive: Keepalive) -> (Channel, Channel) {
        let host = HostKey::generate();
        let (psk, slot, sid) = (Psk::generate(), Slot::generate(), SessionId::generate());
        let (initiator, opening) = Initiator::start(host.public(), &psk, &slot, &sid).unwrap();
        let (_, msg1) = parse_opening(&opening).unwrap();
        let (host_cipher, reply) = respond(&host, &psk, &slot, &sid, &msg1).unwrap();
        let guest_cipher = initiator.finish(reply[1..].try_into().unwrap()).unwrap();
        let (a, b) = tokio::io::duplex(1 << 20);
        (
            Channel::spawn(Box::new(a), guest_cipher, keepalive, None),
            Channel::spawn(Box::new(b), host_cipher, keepalive, None),
        )
    }

    fn keepalive() -> Keepalive {
        Keepalive::from_policy(&Policy::default())
    }

    #[tokio::test(start_paused = true)]
    async fn messages_and_long_control_messages_cross() {
        let (guest, mut host) = pair(keepalive());
        let hello = Control::Hello(Hello::Control {
            name: "Notch".into(),
            premium: false,
            launcher: "x".repeat(40_000),
        });
        assert!(guest.send(hello.clone()).await);
        assert!(
            guest
                .tx
                .send(Out::Data {
                    offset: 0,
                    bytes: vec![1, 2, 3]
                })
                .await
                .is_ok()
        );
        match host.recv().await {
            Some(In::Control(Ok(message))) => assert_eq!(message, hello),
            other => panic!("{other:?}"),
        }
        match host.recv().await {
            Some(In::Data { offset: 0, bytes }) => assert_eq!(bytes, vec![1, 2, 3]),
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn idle_streams_stay_alive_and_measure_round_trips() {
        let rtt = Arc::new(Mutex::new(RttHistogram::default()));
        let host = HostKey::generate();
        let (psk, slot, sid) = (Psk::generate(), Slot::generate(), SessionId::generate());
        let (initiator, opening) = Initiator::start(host.public(), &psk, &slot, &sid).unwrap();
        let (_, msg1) = parse_opening(&opening).unwrap();
        let (host_cipher, reply) = respond(&host, &psk, &slot, &sid, &msg1).unwrap();
        let guest_cipher = initiator.finish(reply[1..].try_into().unwrap()).unwrap();
        let (a, b) = tokio::io::duplex(1 << 16);
        let guest = Channel::spawn(Box::new(a), guest_cipher, keepalive(), Some(rtt.clone()));
        let mut host = Channel::spawn(Box::new(b), host_cipher, keepalive(), None);
        tokio::time::sleep(Duration::from_secs(30)).await;
        assert!(rtt.lock().unwrap().samples() > 10);
        assert!(guest.send(Control::Bye(ByeReason::Leave)).await);
        match host.recv().await {
            Some(In::Control(Ok(Control::Bye(ByeReason::Leave)))) => {}
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_silent_peer_breaks_the_stream() {
        let host = HostKey::generate();
        let (psk, slot, sid) = (Psk::generate(), Slot::generate(), SessionId::generate());
        let (initiator, opening) = Initiator::start(host.public(), &psk, &slot, &sid).unwrap();
        let (_, msg1) = parse_opening(&opening).unwrap();
        let (_host_cipher, reply) = respond(&host, &psk, &slot, &sid, &msg1).unwrap();
        let guest_cipher = initiator.finish(reply[1..].try_into().unwrap()).unwrap();
        // The other end is held open but never writes.
        let (a, _b) = tokio::io::duplex(1 << 16);
        let mut guest = Channel::spawn(Box::new(a), guest_cipher, keepalive(), None);
        let start = Instant::now();
        match guest.recv().await {
            Some(In::Ended(Ended::Silent)) => {}
            other => panic!("{other:?}"),
        }
        assert_eq!(start.elapsed(), Duration::from_millis(3_500));
    }

    #[tokio::test(start_paused = true)]
    async fn finishing_ends_the_stream_for_the_peer() {
        let (guest, mut host) = pair(keepalive());
        guest.finish().await;
        match host.recv().await {
            Some(In::Ended(Ended::Eof)) => {}
            other => panic!("{other:?}"),
        }
    }
}

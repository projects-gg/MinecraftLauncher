//! A game connection relayed over PLJ1 game streams (PROTOCOL.md §6.5, §6.7).
//!
//! The local connection (the guest's game, or the host launcher's connection to the integrated
//! server) outlives the streams: when a stream breaks the link is suspended, keeps its unacknowledged
//! bytes (up to 8 MiB) and continues on whatever stream is attached next, resending from what the
//! other side says it received. Bytes carry their offset, so duplicates are dropped by one rule.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::{Instant, sleep_until, timeout};

use super::channel::{Channel, In, Out};
use super::control::{Accept, CloseReason, Control};
use super::ids::StreamId;
use super::record::MAX_DATA;
use crate::policy::Policy;

/// The link's timers and limits.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LinkConfig {
    /// How long a broken link waits for a stream to be attached.
    pub suspend: Duration,
    pub ack_every: u64,
    pub ack_delay: Duration,
    pub max_unacked: u64,
}

impl LinkConfig {
    /// The host keeps a broken game stream for its resume window (15 s).
    pub fn host(policy: &Policy) -> Self {
        Self::with_suspend(policy, policy.host.resume_window_ms)
    }

    /// The guest tries for 12 s, then gives up.
    pub fn guest(policy: &Policy) -> Self {
        Self::with_suspend(policy, policy.guest.resume_try_ms)
    }

    fn with_suspend(policy: &Policy, suspend_ms: u64) -> Self {
        Self {
            suspend: Duration::from_millis(suspend_ms),
            ack_every: policy.plj.ack_every_bytes,
            ack_delay: Duration::from_millis(policy.plj.ack_delay_ms),
            max_unacked: policy.plj.max_unacked_bytes,
        }
    }
}

/// Where a link is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LinkState {
    Live,
    /// The stream broke at this time; the link waits for another one.
    Broken(Instant),
    Ended(LinkEnd),
}

/// How a link ended. The local connection is closed in every case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LinkEnd {
    /// The game connection ended and both sides agreed (`close`).
    Closed,
    /// No stream was attached within the suspension time.
    Expired,
    /// The session ended it.
    Aborted,
}

pub(crate) enum LinkCmd {
    /// Continue on `channel`. The peer received `peer_recv` bytes of this side's direction (or
    /// more, when it acknowledged more on the old stream meanwhile). With `resume`, the link first
    /// sends `ok{stream, recv}` (the host's answer to a resume). The reply is what this side
    /// received, or the channel back when `peer_recv` is impossible (beyond what was sent).
    Attach {
        channel: Channel,
        peer_recv: u64,
        resume: Option<StreamId>,
        reply: oneshot::Sender<Result<u64, Option<Channel>>>,
    },
    /// End the game connection in order: stop reading it, `close` after the last byte, and end
    /// once the peer has everything (the guest leaving with its game connected).
    Close,
    Abort,
}

/// The owner's handle on a running link.
#[derive(Clone)]
pub(crate) struct LinkHandle {
    cmd: mpsc::Sender<LinkCmd>,
    state: watch::Receiver<LinkState>,
    received: Arc<AtomicU64>,
}

impl LinkHandle {
    pub fn state(&self) -> LinkState {
        *self.state.borrow()
    }

    /// Waits until the state differs from `seen` and returns the new one.
    pub async fn changed_from(&mut self, seen: LinkState) -> LinkState {
        loop {
            let now = *self.state.borrow_and_update();
            if now != seen {
                return now;
            }
            if self.state.changed().await.is_err() {
                return *self.state.borrow();
            }
        }
    }

    /// Bytes of the other side's direction received in order.
    pub fn received(&self) -> u64 {
        self.received.load(Ordering::Acquire)
    }

    /// Hands the link a new stream (see [`LinkCmd::Attach`]). On refusal the channel comes back
    /// when it is still usable (to answer `no{resume}` on it).
    pub async fn attach(
        &self,
        channel: Channel,
        peer_recv: u64,
        resume: Option<StreamId>,
    ) -> Result<u64, Option<Channel>> {
        let (reply, answer) = oneshot::channel();
        let cmd = LinkCmd::Attach {
            channel,
            peer_recv,
            resume,
            reply,
        };
        if let Err(mpsc::error::SendError(cmd)) = self.cmd.send(cmd).await {
            return match cmd {
                LinkCmd::Attach { channel, .. } => Err(Some(channel)),
                LinkCmd::Close | LinkCmd::Abort => Err(None),
            };
        }
        answer.await.unwrap_or(Err(None))
    }

    /// Ends the link at once (the session closed, the guest was removed).
    pub async fn abort(&self) {
        let _ = self.cmd.send(LinkCmd::Abort).await;
    }

    /// Ends the game connection in order (see [`LinkCmd::Close`]); waits at most `limit` for the
    /// peer, then aborts.
    pub async fn close(&self, limit: Duration) {
        if self.cmd.send(LinkCmd::Close).await.is_err() {
            return;
        }
        let mut state = self.state.clone();
        let ended = timeout(limit, async {
            while !matches!(*state.borrow_and_update(), LinkState::Ended(_)) {
                if state.changed().await.is_err() {
                    return;
                }
            }
        })
        .await;
        if ended.is_err() {
            self.abort().await;
        }
    }

    pub fn is_ended(&self) -> bool {
        matches!(self.state(), LinkState::Ended(_))
    }
}

/// Starts relaying `local` over `channel`. `prefix`: bytes already read from the local connection
/// (the guest's kept handshake), sent first from offset 0.
pub(crate) fn spawn<L>(
    local: L,
    prefix: Vec<u8>,
    channel: Channel,
    config: LinkConfig,
) -> LinkHandle
where
    L: AsyncRead + AsyncWrite + Send + 'static,
{
    let (cmd, cmds) = mpsc::channel(4);
    let (state_tx, state) = watch::channel(LinkState::Live);
    let received = Arc::new(AtomicU64::new(0));
    tokio::spawn(run(
        local,
        prefix,
        channel,
        config,
        cmds,
        state_tx,
        received.clone(),
    ));
    LinkHandle {
        cmd,
        state,
        received,
    }
}

/// What this side sent and the peer has not acknowledged.
struct Outbound {
    /// Offset of `data[0]`: the peer acknowledged everything before it.
    base: u64,
    data: VecDeque<u8>,
}

impl Outbound {
    fn end(&self) -> u64 {
        self.base + self.data.len() as u64
    }

    fn acknowledge(&mut self, next: u64) {
        let n = usize::try_from(next - self.base)
            .unwrap_or(usize::MAX)
            .min(self.data.len());
        self.data.drain(..n);
        self.base = next;
    }

    fn chunk(&self, from: u64) -> Vec<u8> {
        let start = usize::try_from(from - self.base)
            .unwrap_or(usize::MAX)
            .min(self.data.len());
        let len = (self.data.len() - start).min(MAX_DATA);
        let (a, b) = self.data.as_slices();
        let mut out = Vec::with_capacity(len);
        if start < a.len() {
            let take = (a.len() - start).min(len);
            out.extend_from_slice(&a[start..start + take]);
            out.extend_from_slice(&b[..len - take]);
        } else {
            let at = start - a.len();
            out.extend_from_slice(&b[at..at + len]);
        }
        out
    }
}

enum LocalOut {
    Bytes(Vec<u8>),
}

async fn write_local<W: AsyncWrite + Unpin>(
    mut writer: W,
    mut items: mpsc::Receiver<LocalOut>,
    failed: oneshot::Sender<()>,
) {
    while let Some(LocalOut::Bytes(bytes)) = items.recv().await {
        if writer.write_all(&bytes).await.is_err() {
            let _ = failed.send(());
            return;
        }
    }
    let _ = writer.shutdown().await;
}

async fn recv_from(channel: &mut Option<Channel>) -> Option<In> {
    match channel {
        Some(channel) => channel.rx.recv().await,
        None => std::future::pending().await,
    }
}

async fn reserve(tx: &Option<mpsc::Sender<Out>>) -> Option<mpsc::Permit<'_, Out>> {
    match tx {
        Some(tx) => tx.reserve().await.ok(),
        None => std::future::pending().await,
    }
}

fn far() -> Instant {
    Instant::now() + Duration::from_secs(86_400 * 365)
}

async fn run<L>(
    local: L,
    prefix: Vec<u8>,
    channel: Channel,
    config: LinkConfig,
    mut cmds: mpsc::Receiver<LinkCmd>,
    state: watch::Sender<LinkState>,
    received: Arc<AtomicU64>,
) where
    L: AsyncRead + AsyncWrite + Send + 'static,
{
    let (mut local_read, local_write) = tokio::io::split(local);
    let (to_local, local_items) = mpsc::channel::<LocalOut>(64);
    let (local_fail_tx, mut local_fail) = oneshot::channel();
    tokio::spawn(write_local(local_write, local_items, local_fail_tx));

    let mut out = Outbound {
        base: 0,
        data: VecDeque::from(prefix),
    };
    // Next offset to send on the current stream.
    let mut sent: u64 = 0;
    // What arrived in order from the peer.
    let mut in_next: u64 = 0;
    let mut unacked: u64 = 0;
    let mut ack_at: Option<Instant> = None;
    let mut ack_due = false;
    // The local connection stopped giving bytes (and why).
    let mut local_end: Option<CloseReason> = None;
    let mut local_dead = false;
    let mut close_sent = false;
    let mut peer_close: Option<u64> = None;
    let mut channel = Some(channel);
    let mut broken_at: Option<Instant> = None;
    let mut buf = vec![0u8; MAX_DATA];

    let end = loop {
        let can_read = local_end.is_none() && (out.data.len() as u64) < config.max_unacked;
        let tx = channel.as_ref().map(|c| c.tx.clone());
        let wants_send =
            tx.is_some() && (ack_due || sent < out.end() || (local_end.is_some() && !close_sent));
        let mut broke = false;
        tokio::select! {
            read = local_read.read(&mut buf), if can_read => match read {
                Ok(0) => local_end = Some(CloseReason::Eof),
                Ok(n) => out.data.extend(&buf[..n]),
                Err(_) => local_end = Some(CloseReason::Error),
            },
            item = recv_from(&mut channel) => match item {
                Some(In::Data { offset, bytes }) => {
                    let data_end = offset.saturating_add(bytes.len() as u64);
                    if offset > in_next || data_end == u64::MAX {
                        // A gap (or an impossible offset): the peer broke the rules on this stream.
                        broke = true;
                    } else if data_end > in_next {
                        let fresh = bytes[usize::try_from(in_next - offset).unwrap_or(0)..].to_vec();
                        in_next = data_end;
                        received.store(in_next, Ordering::Release);
                        unacked += fresh.len() as u64;
                        if unacked >= config.ack_every {
                            ack_due = true;
                        } else if ack_at.is_none() {
                            ack_at = Some(Instant::now() + config.ack_delay);
                        }
                        if !local_dead && to_local.send(LocalOut::Bytes(fresh)).await.is_err() {
                            local_dead = true;
                        }
                        if peer_close.is_some_and(|at| in_next >= at) {
                            break LinkEnd::Closed;
                        }
                    }
                }
                Some(In::Ack(next)) => {
                    if next > out.end() {
                        broke = true;
                    } else if next > out.base {
                        out.acknowledge(next);
                        sent = sent.max(next);
                    }
                    if close_sent && out.data.is_empty() {
                        break LinkEnd::Closed;
                    }
                }
                Some(In::Control(Ok(Control::Close { at, .. }))) => {
                    if at < in_next {
                        broke = true;
                    } else {
                        peer_close = Some(at);
                        // Nothing more is read from the local connection: the other end is gone.
                        local_end.get_or_insert(CloseReason::Eof);
                        if in_next >= at {
                            break LinkEnd::Closed;
                        }
                    }
                }
                // Other control messages have no meaning on a game stream.
                Some(In::Control(_)) => {}
                Some(In::Ended(why)) => {
                    tracing::debug!(?why, "game stream ended");
                    broke = true;
                }
                None => broke = true,
            },
            permit = reserve(&tx), if wants_send => match permit {
                Some(permit) => {
                    if ack_due {
                        permit.send(Out::Ack(in_next));
                        ack_due = false;
                        unacked = 0;
                        ack_at = None;
                    } else if sent < out.end() {
                        let bytes = out.chunk(sent);
                        let offset = sent;
                        sent += bytes.len() as u64;
                        permit.send(Out::Data { offset, bytes });
                    } else if let Some(reason) = local_end {
                        permit.send(Out::Control(Control::Close { reason, at: out.end() }));
                        close_sent = true;
                        if out.data.is_empty() && peer_close.is_some() {
                            break LinkEnd::Closed;
                        }
                    }
                }
                None => broke = true,
            },
            () = sleep_until(ack_at.unwrap_or_else(far)), if ack_at.is_some() && !ack_due => {
                ack_due = true;
            }
            () = sleep_until(broken_at.map_or_else(far, |t| t + config.suspend)), if broken_at.is_some() => {
                break LinkEnd::Expired;
            }
            cmd = cmds.recv() => match cmd {
                Some(LinkCmd::Attach { channel: new, peer_recv, resume, reply }) => {
                    if peer_recv > out.end() {
                        let _ = reply.send(Err(Some(new)));
                        continue;
                    }
                    // In a planned move the old stream still carries acknowledgements while the
                    // resume is under way: the peer may have acknowledged more than it said.
                    let from = peer_recv.max(out.base);
                    out.acknowledge(from);
                    sent = from;
                    close_sent = false;
                    ack_due = false;
                    unacked = 0;
                    ack_at = None;
                    if let Some(stream) = resume
                        && !new.send(Control::Ok(Accept::resume(stream, in_next))).await
                    {
                        let _ = reply.send(Err(None));
                        continue;
                    }
                    // The old stream, if any, closes right away.
                    channel = Some(new);
                    broken_at = None;
                    state.send_replace(LinkState::Live);
                    let _ = reply.send(Ok(in_next));
                }
                Some(LinkCmd::Close) => {
                    local_end.get_or_insert(CloseReason::Eof);
                    if channel.is_none() {
                        break LinkEnd::Aborted;
                    }
                }
                Some(LinkCmd::Abort) | None => break LinkEnd::Aborted,
            },
            _ = &mut local_fail, if !local_dead => {
                local_dead = true;
                local_end.get_or_insert(CloseReason::Error);
            }
        }
        if broke {
            channel = None;
            if broken_at.is_none() {
                let now = Instant::now();
                broken_at = Some(now);
                state.send_replace(LinkState::Broken(now));
            }
        }
    };

    if end == LinkEnd::Closed
        && let Some(channel) = channel.take()
    {
        // The peer learns everything arrived, then the stream ends.
        let _ = timeout(Duration::from_secs(5), async {
            if channel.tx.send(Out::Ack(in_next)).await.is_ok() {
                channel.finish().await;
            }
        })
        .await;
    }
    drop(channel);
    drop(local_read);
    drop(to_local);
    state.send_replace(LinkState::Ended(end));
}

#[cfg(test)]
mod tests {
    use tokio::io::duplex;

    use super::*;
    use crate::tunnel::channel::Keepalive;
    use crate::tunnel::channel::tests::pair;

    fn config(policy: &Policy) -> LinkConfig {
        LinkConfig::guest(policy)
    }

    #[test]
    fn chunks_cross_the_ring_buffer_seam() {
        let mut out = Outbound {
            base: 0,
            data: VecDeque::with_capacity(8),
        };
        out.data.extend(0u8..6);
        out.acknowledge(4);
        out.data.extend(6u8..12);
        assert_eq!(out.chunk(4), (4u8..12).collect::<Vec<_>>());
        assert_eq!(out.chunk(9), vec![9, 10, 11]);
        assert_eq!(out.end(), 12);
    }

    /// Two links joined by a channel pair; returns both local ends.
    async fn joined() -> (
        tokio::io::DuplexStream,
        LinkHandle,
        tokio::io::DuplexStream,
        LinkHandle,
    ) {
        let policy = Policy::default();
        let (a_chan, b_chan) = pair(Keepalive::from_policy(&policy));
        let (game_a, local_a) = duplex(1 << 20);
        let (game_b, local_b) = duplex(1 << 20);
        let a = spawn(local_a, Vec::new(), a_chan, config(&policy));
        let b = spawn(local_b, Vec::new(), b_chan, config(&policy));
        (game_a, a, game_b, b)
    }

    #[tokio::test(start_paused = true)]
    async fn bytes_cross_both_ways_and_close_in_order() {
        let (mut game_a, a, mut game_b, b) = joined().await;
        let big: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        game_a.write_all(&big).await.unwrap();
        game_b.write_all(b"pong").await.unwrap();
        let mut got = vec![0u8; big.len()];
        game_b.read_exact(&mut got).await.unwrap();
        assert_eq!(got, big);
        let mut small = [0u8; 4];
        game_a.read_exact(&mut small).await.unwrap();
        assert_eq!(&small, b"pong");
        assert_eq!(b.received(), big.len() as u64);

        drop(game_a);
        let mut rest = Vec::new();
        game_b.read_to_end(&mut rest).await.unwrap();
        assert!(rest.is_empty());
        let (mut a, mut b) = (a, b);
        assert_eq!(
            a.changed_from(LinkState::Live).await,
            LinkState::Ended(LinkEnd::Closed)
        );
        assert_eq!(
            b.changed_from(LinkState::Live).await,
            LinkState::Ended(LinkEnd::Closed)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_broken_stream_resumes_without_loss_or_duplicates() {
        let policy = Policy::default();
        let keepalive = Keepalive::from_policy(&policy);
        let (a_chan, b_chan) = pair(keepalive);
        let (mut game_a, local_a) = duplex(1 << 22);
        let (mut game_b, local_b) = duplex(1 << 22);
        let mut a = spawn(
            local_a,
            b"hello".to_vec(),
            a_chan,
            LinkConfig::guest(&policy),
        );
        let mut b = spawn(local_b, Vec::new(), b_chan, LinkConfig::host(&policy));

        let first: Vec<u8> = (0..50_000u32).map(|i| (i % 7) as u8).collect();
        game_a.write_all(&first).await.unwrap();
        let mut got = vec![0u8; 5 + first.len()];
        game_b.read_exact(&mut got).await.unwrap();
        assert_eq!(&got[..5], b"hello");

        // Break the stream: both sides drop their channels by attaching nothing; simulate by
        // aborting the transport through a fresh pair attached on both sides.
        let (a_new, b_new) = pair(keepalive);
        // The host learns the guest's count from the resume hello; the guest the host's from ok.
        let guest_recv = a.received();
        let Ok(host_recv) = b.attach(b_new, guest_recv, None).await else {
            panic!("attach")
        };
        assert!(matches!(a.attach(a_new, host_recv, None).await, Ok(r) if r == guest_recv));

        let second: Vec<u8> = (0..70_000u32).map(|i| (i % 13) as u8).collect();
        game_a.write_all(&second).await.unwrap();
        game_b.write_all(b"back").await.unwrap();
        let mut got2 = vec![0u8; second.len()];
        game_b.read_exact(&mut got2).await.unwrap();
        assert_eq!(got2, second);
        let mut back = [0u8; 4];
        game_a.read_exact(&mut back).await.unwrap();
        assert_eq!(&back, b"back");
        assert_eq!(a.state(), LinkState::Live);
        assert_eq!(b.state(), LinkState::Live);
        let _ = (&mut a, &mut b);
    }

    #[tokio::test(start_paused = true)]
    async fn a_link_left_broken_expires_and_closes_the_game_connection() {
        let policy = Policy::default();
        let (a_chan, b_chan) = pair(Keepalive::from_policy(&policy));
        let (mut game_a, local_a) = duplex(1 << 16);
        let mut a = spawn(local_a, Vec::new(), a_chan, LinkConfig::host(&policy));
        drop(b_chan);
        let broken = a.changed_from(LinkState::Live).await;
        let LinkState::Broken(since) = broken else {
            panic!("{broken:?}")
        };
        assert_eq!(
            a.changed_from(broken).await,
            LinkState::Ended(LinkEnd::Expired)
        );
        assert_eq!(since.elapsed(), Duration::from_secs(15));
        let mut rest = Vec::new();
        game_a.read_to_end(&mut rest).await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn impossible_counts_are_refused() {
        let policy = Policy::default();
        let (a_chan, _b_chan) = pair(Keepalive::from_policy(&policy));
        let (_game_a, local_a) = duplex(1 << 16);
        let a = spawn(local_a, b"abc".to_vec(), a_chan, LinkConfig::host(&policy));
        let (c, _d) = pair(Keepalive::from_policy(&policy));
        assert!(matches!(a.attach(c, 4, None).await, Err(Some(_))));
    }
}

//! One share's connection to the Realtime SFU (str0m, sans-IO): the offer, the answer, and a thread
//! that owns the UDP socket and the `Rtc`. It wakes only for a packet, a frame to send, or one of
//! str0m's / the keyframe policy's deadlines; nothing polls.
//!
//! Each share is its own SFU session (share.md §4): one video m-line with simulcast send `f`/`q`
//! (H.264, RTX, TWCC) and, with sound, one Opus m-line.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use str0m::bwe::{Bitrate, BweKind};
use str0m::change::{SdpAnswer, SdpPendingOffer};
use str0m::format::{Codec, FormatParams};
use str0m::media::{
    Direction, Frequency, KeyframeRequest, MediaKind, MediaTime, Mid, Pt, Rid, Simulcast,
    SimulcastLayer,
};
use str0m::net::{Protocol, Receive};
use str0m::{Candidate, Event, IceConnectionState, Input, Output, Rtc};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use crate::api::{LostWhy, Tuning};
use crate::keyframe::KeyframePolicy;
use crate::preset::{AUDIO_KBPS, Layer, LayerSpec};
use crate::rate::{RateController, RatePlan};

/// Without a connection this long after the answer, UDP is taken as blocked.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// A dropped connection that does not come back within this ends the share.
const RECONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const STATS_EVERY: Duration = Duration::from_secs(5);

const PT_H264_HIGH: u8 = 112;
const RTX_H264_HIGH: u8 = 113;
const PT_H264_BASE: u8 = 108;
const RTX_H264_BASE: u8 = 109;
const PT_OPUS: u8 = 111;
/// Constrained High, level 3.1 in SDP (level-asymmetry-allowed lets us send higher levels).
const PROFILE_HIGH: u32 = 0x640c1f;
/// Constrained Baseline, level 3.1.
const PROFILE_BASE: u32 = 0x42e01f;

/// The H.264 profile the SFU accepted; the encoders are configured with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum H264Profile {
    ConstrainedHigh,
    ConstrainedBaseline,
}

#[derive(Debug)]
pub enum TransportError {
    Io(std::io::Error),
    Sdp(String),
    /// The answer has no usable H.264.
    NoCodec,
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportError::Io(e) => write!(f, "io: {e}"),
            TransportError::Sdp(e) => write!(f, "sdp: {e}"),
            TransportError::NoCodec => write!(f, "no usable H.264 in the answer"),
        }
    }
}

impl std::error::Error for TransportError {}

/// From the media pipeline to the transport thread.
#[derive(Debug)]
pub enum ToTransport {
    /// An encoded frame (Annex B). `at` is the capture instant, `rtp` 90 kHz ticks since the start;
    /// `generation` is its encoders' (SQOL: frames of replaced encoders are dropped, [`Self::Floor`]).
    Video {
        layer: Layer,
        data: Vec<u8>,
        at: Instant,
        rtp: u64,
        key: bool,
        generation: u32,
    },
    /// SQOL: encoders of this generation run now; frames of older ones still in flight are stale.
    Floor(u32),
    /// One Opus packet; `rtp` in 48 kHz samples since the start.
    Audio {
        data: Vec<u8>,
        at: Instant,
        rtp: u64,
    },
    Paused(bool),
    /// A watcher came: keyframes on every layer (through the policy).
    KeyframeAll,
    Reconfigure {
        full: LayerSpec,
        small: LayerSpec,
        audio: bool,
        tuning: Tuning,
    },
    Stop,
}

/// From the transport thread up to the session.
#[derive(Debug, Clone, PartialEq)]
pub enum MediaCtl {
    /// Force a keyframe on this layer now.
    KeyFrame(Layer),
    /// New encoder bitrates and the full layer's frame-rate ceiling.
    Rate(RatePlan),
    /// ICE and DTLS are up: media flows from here.
    Connected,
    Stats {
        kbps: u32,
        full_fps: u8,
        bwe_kbps: u32,
    },
    /// The transport ended on its own.
    Lost(LostWhy),
    /// The thread ended (after `Stop` or `Lost`).
    Closed,
}

/// The `Rtc` between offer and answer.
pub struct Prepared {
    rtc: Rtc,
    pending: Option<SdpPendingOffer>,
    socket: UdpSocket,
    local: SocketAddr,
    video: Mid,
    audio: Option<Mid>,
}

/// After the answer: what the session needs to start encoders, and the thread's inputs.
pub struct Negotiated {
    pub profile: H264Profile,
    prepared: Prepared,
    h264: Pt,
    opus: Option<Pt>,
}

/// The address the default route leaves from (no packet is sent): the host candidate. Without a
/// route (offline) the loopback address keeps the offer valid; ICE then fails and says so.
fn local_ip() -> IpAddr {
    let routed = || -> std::io::Result<IpAddr> {
        let probe = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
        probe.connect((Ipv4Addr::new(1, 1, 1, 1), 53))?;
        Ok(probe.local_addr()?.ip())
    };
    routed().unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST))
}

/// A new session's offer. `initial_bps` seeds the bandwidth estimator.
pub fn prepare(audio: bool, initial_bps: u64) -> Result<(Prepared, String), TransportError> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).map_err(TransportError::Io)?;
    let port = socket.local_addr().map_err(TransportError::Io)?.port();
    let local = SocketAddr::new(local_ip(), port);

    let mut config = Rtc::builder()
        .clear_codecs()
        .enable_bwe(Some(Bitrate::bps(initial_bps)))
        // Peer stats carry the estimate and RTT too (a fallback when no estimate event comes).
        .set_stats_interval(Some(Duration::from_secs(2)));
    let codecs = config.codec_config();
    codecs.add_h264(
        PT_H264_HIGH.into(),
        Some(RTX_H264_HIGH.into()),
        true,
        PROFILE_HIGH,
    );
    codecs.add_h264(
        PT_H264_BASE.into(),
        Some(RTX_H264_BASE.into()),
        true,
        PROFILE_BASE,
    );
    if audio {
        codecs.add_config(
            PT_OPUS.into(),
            None,
            Codec::Opus,
            Frequency::FORTY_EIGHT_KHZ,
            Some(2),
            FormatParams {
                min_p_time: Some(10),
                use_inband_fec: Some(true),
                stereo: Some(true),
                sprop_stereo: Some(true),
                ..Default::default()
            },
        );
    }
    let mut rtc = config.build(Instant::now());
    let candidate =
        Candidate::host(local, "udp").map_err(|e| TransportError::Sdp(e.to_string()))?;
    rtc.add_local_candidate(candidate);

    let mut api = rtc.sdp_api();
    let mut simulcast = Simulcast::new();
    for layer in Layer::ALL {
        simulcast.add_send_layer(SimulcastLayer::new(layer.rid()));
    }
    let video = api.add_media(
        MediaKind::Video,
        Direction::SendOnly,
        Some("share".into()),
        Some("video".into()),
        Some(simulcast),
    );
    let audio_mid = audio.then(|| {
        api.add_media(
            MediaKind::Audio,
            Direction::SendOnly,
            Some("share".into()),
            Some("audio".into()),
            None,
        )
    });
    let (offer, pending) = api
        .apply()
        .ok_or_else(|| TransportError::Sdp("empty offer".into()))?;
    Ok((
        Prepared {
            rtc,
            pending: Some(pending),
            socket,
            local,
            video,
            audio: audio_mid,
        },
        offer.to_sdp_string(),
    ))
}

impl Prepared {
    /// Applies the SFU's answer.
    pub fn accept(mut self, answer: &str) -> Result<Negotiated, TransportError> {
        let answer =
            SdpAnswer::from_sdp_string(answer).map_err(|e| TransportError::Sdp(e.to_string()))?;
        let pending = self
            .pending
            .take()
            .ok_or_else(|| TransportError::Sdp("answered twice".into()))?;
        self.rtc
            .sdp_api()
            .accept_answer(pending, answer)
            .map_err(|e| TransportError::Sdp(e.to_string()))?;
        let writer = self.rtc.writer(self.video).ok_or(TransportError::NoCodec)?;
        let (h264, profile) = writer
            .payload_params()
            .filter(|p| p.spec().codec == Codec::H264)
            .filter(|p| p.spec().format.packetization_mode == Some(1))
            .map(|p| {
                let high = p
                    .spec()
                    .format
                    .profile_level_id
                    .is_some_and(|id| id >> 16 == 0x64);
                let profile = if high {
                    H264Profile::ConstrainedHigh
                } else {
                    H264Profile::ConstrainedBaseline
                };
                (p.pt(), profile)
            })
            .next()
            .ok_or(TransportError::NoCodec)?;
        let opus = match self.audio {
            Some(mid) => self.rtc.writer(mid).and_then(|w| {
                w.payload_params()
                    .find(|p| p.spec().codec == Codec::Opus)
                    .map(|p| p.pt())
            }),
            None => None,
        };
        Ok(Negotiated {
            profile,
            prepared: self,
            h264,
            opus,
        })
    }
}

/// The transport's input queue (made before the thread, so encoders can hold it first).
#[derive(Clone)]
pub struct TransportHandle {
    tx: mpsc::UnboundedSender<ToTransport>,
}

/// The receiving end, handed to [`Negotiated::start`].
pub struct TransportInbox(mpsc::UnboundedReceiver<ToTransport>);

pub fn channel() -> (TransportHandle, TransportInbox) {
    let (tx, rx) = mpsc::unbounded_channel();
    (TransportHandle { tx }, TransportInbox(rx))
}

impl TransportHandle {
    pub fn send(&self, msg: ToTransport) -> bool {
        self.tx.send(msg).is_ok()
    }
}

impl Negotiated {
    pub fn has_audio(&self) -> bool {
        self.opus.is_some()
    }

    /// Starts the thread. `ctl` is called on that thread for every [`MediaCtl`].
    pub fn start(
        self,
        inbox: TransportInbox,
        full: LayerSpec,
        small: LayerSpec,
        tuning: Tuning,
        ctl: Box<dyn FnMut(MediaCtl) + Send>,
    ) -> std::io::Result<()> {
        let rx = inbox.0;
        let audio_kbps = if self.opus.is_some() { AUDIO_KBPS } else { 0 };
        let rate = RateController::new(full, small, audio_kbps, tuning);
        std::thread::Builder::new()
            .name("pl-share-transport".into())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_io()
                    .enable_time()
                    .build()
                {
                    Ok(rt) => rt,
                    Err(e) => {
                        warn!("share transport runtime: {e}");
                        let mut ctl = ctl;
                        ctl(MediaCtl::Lost(LostWhy::Internal));
                        ctl(MediaCtl::Closed);
                        return;
                    }
                };
                let mut ctl = ctl;
                let why = runtime.block_on(run(self, rate, rx, &mut ctl));
                if let Some(why) = why {
                    ctl(MediaCtl::Lost(why));
                }
                ctl(MediaCtl::Closed);
            })?;
        Ok(())
    }
}

struct Counters {
    bytes: u64,
    full_frames: u32,
    since: Instant,
}

/// The loop; returns why it ended on its own (`None` after `Stop`).
async fn run(
    negotiated: Negotiated,
    mut rate: RateController,
    mut rx: mpsc::UnboundedReceiver<ToTransport>,
    ctl: &mut Box<dyn FnMut(MediaCtl) + Send>,
) -> Option<LostWhy> {
    let Negotiated {
        prepared,
        h264,
        opus,
        ..
    } = negotiated;
    let Prepared {
        mut rtc,
        socket,
        local,
        video,
        audio,
        ..
    } = prepared;
    if let Err(e) = socket.set_nonblocking(true) {
        warn!("share socket: {e}");
        return Some(LostWhy::Internal);
    }
    let socket = match tokio::net::UdpSocket::from_std(socket) {
        Ok(s) => s,
        Err(e) => {
            warn!("share socket: {e}");
            return Some(LostWhy::Internal);
        }
    };

    let started = Instant::now();
    let mut connected = false;
    let mut connect_deadline = Some(started + CONNECT_TIMEOUT);
    let mut lost_at: Option<Instant> = None;
    let mut paused = false;
    let mut floor = crate::switch::GenFloor::default();
    let mut policies = [KeyframePolicy::new(), KeyframePolicy::new()];
    let mut bwe_kbps = 0u32;
    let mut twcc_seen = false;
    let mut stats_at = started + STATS_EVERY;
    let mut counters = Counters {
        bytes: 0,
        full_frames: 0,
        since: started,
    };
    rtc.bwe().set_bitrate(
        Bitrate::bps(rate.minimum_bps()),
        Bitrate::bps(rate.desired_bps()),
    );
    let plan = rate.initial(started);
    ctl(MediaCtl::Rate(plan));

    let mut buf = vec![0u8; 2000];
    loop {
        // Drain to the next timeout (str0m's single-mutation invariant).
        let timeout = loop {
            match rtc.poll_output() {
                Ok(Output::Timeout(t)) => break t,
                Ok(Output::Transmit(t)) => {
                    // A full socket buffer drops the datagram, like the network would.
                    let _ = socket.try_send_to(&t.contents, t.destination);
                }
                Ok(Output::Event(event)) => match event {
                    Event::IceConnectionStateChange(state) => {
                        debug!("share ice: {state:?}");
                        match state {
                            IceConnectionState::Disconnected => {
                                lost_at.get_or_insert(Instant::now());
                            }
                            IceConnectionState::Connected | IceConnectionState::Completed => {
                                lost_at = None;
                            }
                            _ => {}
                        }
                    }
                    Event::Connected => {
                        if !connected {
                            connected = true;
                            connect_deadline = None;
                            info!("share connected in {} ms", started.elapsed().as_millis());
                            ctl(MediaCtl::Connected);
                            // The first frames must be decodable.
                            let now = Instant::now();
                            for layer in Layer::ALL {
                                if policies[layer.index()].request(now) {
                                    ctl(MediaCtl::KeyFrame(layer));
                                }
                            }
                        }
                    }
                    Event::KeyframeRequest(KeyframeRequest { rid, kind, .. }) => {
                        debug!("share keyframe request {kind:?} rid {rid:?}");
                        let now = Instant::now();
                        let layers: Vec<Layer> = match rid.as_deref().and_then(Layer::from_rid) {
                            Some(layer) => vec![layer],
                            None => Layer::ALL.to_vec(),
                        };
                        for layer in layers {
                            if policies[layer.index()].request(now) {
                                ctl(MediaCtl::KeyFrame(layer));
                            }
                        }
                    }
                    Event::PeerStats(stats) => {
                        debug!(
                            "share peer: bwe {:?} kbps, loss {:?}, rtt {:?}",
                            stats.bwe_tx.map(|b| b.as_u64() / 1000),
                            stats.egress_loss_fraction,
                            stats.rtt
                        );
                        if !twcc_seen && let Some(bwe) = stats.bwe_tx {
                            bwe_kbps = (bwe.as_u64() / 1000) as u32;
                            if !paused
                                && let Some(plan) = rate.on_estimate(bwe_kbps, Instant::now())
                            {
                                ctl(MediaCtl::Rate(plan));
                            }
                        }
                    }
                    Event::EgressBitrateEstimate(BweKind::Remb { estimate, .. }) => {
                        debug!("share remb {} kbps", estimate.as_u64() / 1000);
                    }
                    Event::EgressBitrateEstimate(BweKind::Twcc { estimate, .. }) => {
                        twcc_seen = true;
                        bwe_kbps = (estimate.as_u64() / 1000) as u32;
                        if !paused && let Some(plan) = rate.on_estimate(bwe_kbps, Instant::now()) {
                            ctl(MediaCtl::Rate(plan));
                        }
                    }
                    _ => {}
                },
                Err(e) => {
                    warn!("share rtc: {e}");
                    return Some(if connected {
                        LostWhy::Network
                    } else {
                        LostWhy::Negotiation
                    });
                }
            }
        };

        let now = Instant::now();
        if !rtc.is_alive() {
            return Some(LostWhy::Network);
        }
        if connect_deadline.is_some_and(|d| now >= d) {
            return Some(LostWhy::Connect);
        }
        if lost_at.is_some_and(|t| now.duration_since(t) >= RECONNECT_TIMEOUT) {
            return Some(LostWhy::Network);
        }
        if connected {
            for layer in Layer::ALL {
                if policies[layer.index()].poll(now, !paused) {
                    ctl(MediaCtl::KeyFrame(layer));
                }
            }
        }
        if now >= stats_at {
            let secs = now.duration_since(counters.since).as_secs_f64().max(0.001);
            if connected {
                ctl(MediaCtl::Stats {
                    kbps: (counters.bytes as f64 * 8.0 / 1000.0 / secs) as u32,
                    full_fps: (f64::from(counters.full_frames) / secs).round() as u8,
                    bwe_kbps,
                });
            }
            counters = Counters {
                bytes: 0,
                full_frames: 0,
                since: now,
            };
            stats_at = now + STATS_EVERY;
        }
        if timeout <= now {
            if rtc.handle_input(Input::Timeout(now)).is_err() {
                return Some(LostWhy::Network);
            }
            continue;
        }

        let mut deadline = timeout.min(stats_at);
        for d in [connect_deadline, lost_at.map(|t| t + RECONNECT_TIMEOUT)]
            .into_iter()
            .flatten()
        {
            deadline = deadline.min(d);
        }
        if connected {
            for p in &policies {
                if let Some(d) = p.deadline(!paused) {
                    deadline = deadline.min(d);
                }
            }
        }

        tokio::select! {
            received = socket.recv_from(&mut buf) => {
                // Windows reports ICMP "port unreachable" as a receive error: ignore it.
                if let Ok((n, source)) = received
                    && let Ok(receive) = Receive::new(Protocol::Udp, source, local, &buf[..n])
                    && let Err(e) = rtc.handle_input(Input::Receive(Instant::now(), receive))
                {
                    debug!("share input: {e}");
                }
            }
            msg = rx.recv() => {
                let msg = msg?;
                match msg {
                    ToTransport::Stop => {
                        rtc.disconnect();
                        // Flush the DTLS close notify.
                        while let Ok(out) = rtc.poll_output() {
                            match out {
                                Output::Transmit(t) => {
                                    let _ = socket.try_send_to(&t.contents, t.destination);
                                }
                                Output::Timeout(_) => break,
                                Output::Event(_) => {}
                            }
                        }
                        return None;
                    }
                    ToTransport::Floor(generation) => floor.raise(generation),
                    ToTransport::Video { generation, .. } if !floor.admits(generation) => {
                        // A replaced encoder's last frames: the new stream began with its keyframe.
                    }
                    ToTransport::Video { layer, data, at, rtp, key, .. } => {
                        if key {
                            policies[layer.index()].produced(Instant::now());
                        }
                        if layer == Layer::Full {
                            counters.full_frames += 1;
                        }
                        counters.bytes += data.len() as u64;
                        if connected && let Some(writer) = rtc.writer(video) {
                            let result = writer
                                .rid(Rid::from(layer.rid()))
                                .write(h264, at, MediaTime::from_90khz(rtp), data);
                            if let Err(e) = result {
                                debug!("share video write: {e}");
                            }
                        }
                    }
                    ToTransport::Audio { data, at, rtp } => {
                        counters.bytes += data.len() as u64;
                        if connected
                            && let (Some(mid), Some(pt)) = (audio, opus)
                            && let Some(writer) = rtc.writer(mid)
                        {
                            let time = MediaTime::new(rtp, Frequency::FORTY_EIGHT_KHZ);
                            if let Err(e) = writer.write(pt, at, time, data) {
                                debug!("share audio write: {e}");
                            }
                        }
                    }
                    ToTransport::Paused(p) => {
                        paused = p;
                        // While paused only the 1 fps keep-alive flows: no padding to fill.
                        let minimum = if p { 0 } else { rate.minimum_bps() };
                        rtc.bwe().set_bitrate(Bitrate::bps(minimum), Bitrate::bps(rate.desired_bps()));
                        if !p {
                            let now = Instant::now();
                            for layer in Layer::ALL {
                                if policies[layer.index()].request(now) {
                                    ctl(MediaCtl::KeyFrame(layer));
                                }
                            }
                        }
                    }
                    ToTransport::KeyframeAll => {
                        let now = Instant::now();
                        for layer in Layer::ALL {
                            if policies[layer.index()].request(now) {
                                ctl(MediaCtl::KeyFrame(layer));
                            }
                        }
                    }
                    ToTransport::Reconfigure { full, small, audio: with_audio, tuning } => {
                        let audio_kbps = if with_audio && opus.is_some() { AUDIO_KBPS } else { 0 };
                        let known = (bwe_kbps > 0).then_some(bwe_kbps);
                        let plan = rate.reconfigure(full, small, audio_kbps, tuning, known, Instant::now());
                        rtc.bwe().set_bitrate(Bitrate::bps(rate.minimum_bps()), Bitrate::bps(rate.desired_bps()));
                        ctl(MediaCtl::Rate(plan));
                    }
                }
            }
            _ = tokio::time::sleep_until(deadline.into()) => {
                if rtc.handle_input(Input::Timeout(Instant::now())).is_err() {
                    return Some(LostWhy::Network);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offer_has_simulcast_h264_and_stereo_opus() {
        let (_prepared, sdp) = prepare(true, 3_000_000).unwrap();
        assert!(sdp.contains("a=simulcast:send f;q"), "{sdp}");
        assert!(
            sdp.contains("a=rid:f send") && sdp.contains("a=rid:q send"),
            "{sdp}"
        );
        assert!(sdp.contains("profile-level-id=640c1f"), "{sdp}");
        assert!(sdp.contains("profile-level-id=42e01f"), "{sdp}");
        assert!(sdp.contains("transport-cc"), "TWCC: {sdp}");
        assert!(
            sdp.contains("opus/48000/2") && sdp.contains("stereo=1"),
            "{sdp}"
        );
        assert!(sdp.contains("a=sendonly"), "{sdp}");
        assert!(sdp.lines().filter(|l| l.starts_with("m=")).count() == 2);
    }

    #[test]
    fn offer_without_audio_has_one_m_line() {
        let (_prepared, sdp) = prepare(false, 1_000_000).unwrap();
        assert_eq!(sdp.lines().filter(|l| l.starts_with("m=")).count(), 1);
        assert!(!sdp.contains("opus"));
    }
}

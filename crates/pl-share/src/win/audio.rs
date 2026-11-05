//! The stream's sound: WASAPI process loopback → Opus 48 kHz stereo, 20 ms packets.
//!
//! - Game or window: INCLUDE the shared program's process tree (Minecraft is our child, so
//!   "everything but us" would mute it; only its own tree is right).
//! - Monitor or "whole computer": EXCLUDE the voice window's WebView2 browser process tree, which
//!   plays the voice chat, so the room never hears itself.
//!
//! Loopback keeps delivering (silent) buffers every 10 ms while the target plays nothing, and the
//! encoder's unconstrained VBR makes those packets tiny. If a device stops delivering, one silent
//! packet per second keeps the SFU track alive (30 s rule); paused or muted shares send only that.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::{Duration, Instant};

use tracing::{info, warn};
use wasapi::{AudioClient, Direction, SampleType, StreamMode, WaveFormat};

use crate::opus::{FRAME_LEN, FRAME_SAMPLES, Opus, SAMPLE_RATE};

const KEEPALIVE: Duration = Duration::from_secs(1);
const BYTES_PER_FRAME: usize = FRAME_LEN * 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioTarget {
    /// This process and its children.
    Include(u32),
    /// Everything but this process and its children.
    Exclude(u32),
}

/// Encoded audio: the packet, its capture instant and 48 kHz time since the epoch.
pub type AudioOut = Box<dyn FnMut(Vec<u8>, Instant, u64) + Send>;

enum Ctl {
    /// `None`: send only the keep-alive (muted or paused).
    Target(Option<AudioTarget>),
    Stop,
}

pub struct AudioHandle {
    tx: Sender<Ctl>,
}

impl AudioHandle {
    /// Changes what is captured; `None` mutes (keep-alive only).
    pub fn set(&self, target: Option<AudioTarget>) {
        let _ = self.tx.send(Ctl::Target(target));
    }
}

impl Drop for AudioHandle {
    fn drop(&mut self) {
        let _ = self.tx.send(Ctl::Stop);
    }
}

/// Starts the audio thread. `failed` is raised when capture cannot start (the share keeps going
/// without sound; the session reports `AudioUnavailable`).
pub fn start(
    target: Option<AudioTarget>,
    epoch: Instant,
    out: AudioOut,
    failed: Arc<AtomicBool>,
    on_failed: Box<dyn FnMut() + Send>,
) -> std::io::Result<AudioHandle> {
    let (tx, rx) = channel();
    std::thread::Builder::new()
        .name("pl-share-audio".into())
        .spawn(move || {
            let _ = wasapi::initialize_mta();
            let Ok(opus) = Opus::new(96_000) else {
                warn!("share audio: opus init failed");
                return;
            };
            let mut worker = Worker {
                opus,
                epoch,
                samples: 0,
                out,
                failed,
                on_failed,
                pcm: VecDeque::new(),
            };
            worker.run(target, rx);
        })?;
    Ok(AudioHandle { tx })
}

struct Live {
    client: AudioClient,
    capture: wasapi::AudioCaptureClient,
    event: wasapi::Handle,
}

struct Worker {
    opus: Opus,
    epoch: Instant,
    /// 48 kHz samples sent so far (kept in step with the wall clock across gaps).
    samples: u64,
    out: AudioOut,
    failed: Arc<AtomicBool>,
    on_failed: Box<dyn FnMut() + Send>,
    pcm: VecDeque<u8>,
}

impl Worker {
    fn run(&mut self, mut target: Option<AudioTarget>, rx: Receiver<Ctl>) {
        let mut live = target.and_then(|t| self.open(t));
        let mut last_sent = Instant::now();
        loop {
            // Control messages first (they are rare).
            loop {
                match rx.try_recv() {
                    Ok(Ctl::Target(next)) => {
                        if next != target {
                            if let Some(l) = live.take() {
                                let _ = l.client.stop_stream();
                            }
                            target = next;
                            self.pcm.clear();
                            live = target.and_then(|t| self.open(t));
                        }
                    }
                    Ok(Ctl::Stop) | Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        if let Some(l) = live.take() {
                            let _ = l.client.stop_stream();
                        }
                        return;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                }
            }
            match &live {
                Some(l) => {
                    if l.event.wait_for_event(KEEPALIVE.as_millis() as u32).is_ok() {
                        let before = self.pcm.len();
                        match l.capture.read_from_device_to_deque(&mut self.pcm) {
                            Ok(info) if info.flags.silent => {
                                for b in self.pcm.iter_mut().skip(before) {
                                    *b = 0;
                                }
                            }
                            Ok(_) => {}
                            Err(e) => {
                                warn!("share audio read: {e}");
                                self.fail();
                                live = None;
                                continue;
                            }
                        }
                        while self.pcm.len() >= BYTES_PER_FRAME {
                            let mut frame = [0i16; FRAME_LEN];
                            for s in frame.iter_mut() {
                                let lo = self.pcm.pop_front().unwrap_or(0);
                                let hi = self.pcm.pop_front().unwrap_or(0);
                                *s = i16::from_le_bytes([lo, hi]);
                            }
                            self.send(&frame);
                            last_sent = Instant::now();
                        }
                    }
                    if last_sent.elapsed() >= KEEPALIVE {
                        self.send(&[0; FRAME_LEN]);
                        last_sent = Instant::now();
                    }
                }
                None => match rx.recv_timeout(KEEPALIVE.saturating_sub(last_sent.elapsed())) {
                    Ok(Ctl::Target(next)) => {
                        target = next;
                        self.pcm.clear();
                        live = target.and_then(|t| self.open(t));
                    }
                    Ok(Ctl::Stop) | Err(RecvTimeoutError::Disconnected) => return,
                    Err(RecvTimeoutError::Timeout) => {
                        self.send(&[0; FRAME_LEN]);
                        last_sent = Instant::now();
                    }
                },
            }
        }
    }

    fn fail(&mut self) {
        if !self.failed.swap(true, Ordering::SeqCst) {
            (self.on_failed)();
        }
    }

    fn open(&mut self, target: AudioTarget) -> Option<Live> {
        let (pid, include) = match target {
            AudioTarget::Include(pid) => (pid, true),
            AudioTarget::Exclude(pid) => (pid, false),
        };
        let result = (|| -> Result<Live, Box<dyn std::error::Error>> {
            let mut client = AudioClient::new_application_loopback_client(pid, include)?;
            let format = WaveFormat::new(16, 16, &SampleType::Int, SAMPLE_RATE as usize, 2, None);
            let mode = StreamMode::EventsShared {
                autoconvert: true,
                buffer_duration_hns: 200_000,
            };
            client.initialize_client(&format, &Direction::Capture, &mode)?;
            let event = client.set_get_eventhandle()?;
            let capture = client.get_audiocaptureclient()?;
            client.start_stream()?;
            Ok(Live {
                client,
                capture,
                event,
            })
        })();
        match result {
            Ok(live) => {
                info!(
                    "share audio: loopback {} started",
                    if include { "include" } else { "exclude" }
                );
                Some(live)
            }
            Err(e) => {
                warn!("share audio: {e}");
                self.fail();
                None
            }
        }
    }

    fn send(&mut self, frame: &[i16; FRAME_LEN]) {
        if crate::test_no_keepalive() && frame.iter().all(|&s| s == 0) {
            return;
        }
        let now = Instant::now();
        // After a gap (silence, pause) the RTP clock jumps to the wall clock, so sound stays in
        // step with the picture.
        let wall = (now.saturating_duration_since(self.epoch).as_micros() as u64)
            * u64::from(SAMPLE_RATE)
            / 1_000_000;
        if wall > self.samples + u64::from(SAMPLE_RATE) / 10 {
            self.samples = wall;
        }
        if let Some(packet) = self.opus.encode(frame) {
            (self.out)(packet, now, self.samples);
        }
        self.samples += FRAME_SAMPLES as u64;
    }
}

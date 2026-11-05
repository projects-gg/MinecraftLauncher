//! The native publisher (SHARE-N's `pl-share`) behind voice chat's share commands: the machine's
//! `ShareCmd`s become the controller's, its events come back as the machine's `ShareEvent`s. Window
//! ids stay on this computer (`<hwnd>:<pid>` in the picker's ids), titles only in the picker.
//!
//! The controller's thread waits on its queue and costs nothing until a share or a picker asks.
//! Its environment: the launcher's own process (its windows are never offered) and the WebView2
//! browser process (its tree, the voice chat's sound, is left out of "Bütün bilgisayar").

use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pl_share::{
    AudioScope, LostWhy, Preset, ShareCaps, ShareController, ShareEnv, SourceId, SourceInfo,
    SourceKind, Tuning,
};
use pl_voice::Voice;
use pl_voice::share::{
    ShareCmd, ShareEvent, ShareLostWhy, ShareOpts, SharePreset, ShareSource, ShareSourceInfo,
    ShareSources,
};
use tauri::{AppHandle, Manager};

use super::share::Publisher;

/// How long the picker waits for its list (pictures included).
const SOURCES_WAIT: Duration = Duration::from_secs(8);

pub(super) struct Native {
    controller: ShareController,
    voice: Voice,
    /// The picker's request waiting for its list.
    waiting: Mutex<Option<mpsc::Sender<(Vec<SourceInfo>, ShareCaps)>>>,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl Native {
    /// Starts the controller and the pump of its events; asks once what this computer can do.
    pub(super) fn start(app: &AppHandle, voice: &Voice) -> Option<Arc<Self>> {
        let env = ShareEnv {
            own_pids: vec![std::process::id()],
            voice_browser_pid: None,
        };
        let (controller, mut events) = match ShareController::start(env) {
            Ok(started) => started,
            Err(error) => {
                tracing::warn!(%error, "share: the publisher did not start");
                return None;
            }
        };
        let native = Arc::new(Self {
            controller,
            voice: voice.clone(),
            waiting: Mutex::new(None),
        });
        let pump = native.clone();
        tauri::async_runtime::spawn(async move {
            while let Some(event) = events.recv().await {
                pump.on_event(event);
            }
        });
        // What this computer can do (sound, a hardware encoder): one quick look, no pictures.
        native.controller.send(pl_share::ShareCmd::Sources {
            game_pid: None,
            thumbs: false,
        });
        native.learn_browser(app);
        Some(native)
    }

    /// The WebView2 browser process (the main window's and the voice window's are one): its tree
    /// is the voice chat's sound, left out of "Bütün bilgisayar"; its windows are ours.
    fn learn_browser(self: &Arc<Self>, app: &AppHandle) {
        #[cfg(windows)]
        {
            let Some(window) = app.get_webview_window("main") else {
                return;
            };
            let native = self.clone();
            let _ = window.with_webview(move |webview| {
                let mut pid = 0u32;
                // SAFETY: COM calls on the WebView2 objects Tauri hands over, on their thread.
                let found = unsafe {
                    webview
                        .controller()
                        .CoreWebView2()
                        .and_then(|core| core.BrowserProcessId(&mut pid))
                };
                if found.is_ok() && pid != 0 {
                    native.controller.send(pl_share::ShareCmd::Env {
                        env: ShareEnv {
                            own_pids: vec![std::process::id(), pid],
                            voice_browser_pid: Some(pid),
                        },
                    });
                }
            });
        }
        #[cfg(not(windows))]
        let _ = app;
    }

    fn on_event(&self, event: pl_share::ShareEvent) {
        use pl_share::ShareEvent as E;
        match event {
            E::Sources { list, caps } => {
                self.voice
                    .set_share_publisher(true, caps.audio, caps.hardware_encoder.is_some());
                if let Some(waiting) = lock(&self.waiting).take() {
                    let _ = waiting.send((list, caps));
                }
            }
            E::Offer { n, sdp } => self.voice.share_event(ShareEvent::Offer {
                n,
                sdp: pl_voice::wire::Sdp::new(sdp),
            }),
            E::Started { n } => self.voice.share_event(ShareEvent::Started { n }),
            E::Lost { n, why } => {
                tracing::info!(n, ?why, "share: the publisher lost a share");
                self.voice.share_event(ShareEvent::Lost {
                    n,
                    why: if why == LostWhy::Connect {
                        ShareLostWhy::Connect
                    } else {
                        ShareLostWhy::Other
                    },
                });
            }
            E::SourceGone { n } => self.voice.share_event(ShareEvent::SourceGone { n }),
            E::AudioUnavailable { n, why } => {
                tracing::debug!(n, ?why, "share: no sound for a share");
                self.voice.share_event(ShareEvent::AudioUnavailable { n });
            }
            // The panel shows the room's state, not the encoder's numbers; a stop is the machine's.
            E::Stats { .. } | E::Stopped { .. } => {}
        }
    }
}

fn preset(preset: SharePreset) -> Preset {
    match preset {
        SharePreset::Low => Preset::Low,
        SharePreset::Normal => Preset::Normal,
        SharePreset::High => Preset::High,
    }
}

fn opts(opts: ShareOpts) -> pl_share::ShareOpts {
    pl_share::ShareOpts {
        preset: preset(opts.preset),
        fps: opts.fps,
        tuning: if opts.motion {
            Tuning::Motion
        } else {
            Tuning::Clarity
        },
        cursor: opts.cursor,
        audio: opts.audio,
        audio_scope: if opts.audio_system {
            AudioScope::System
        } else {
            AudioScope::App
        },
    }
}

/// The picker's id of a source (window ids carry the process, against a reused handle).
fn source_of(id: &SourceId) -> ShareSource {
    match id {
        SourceId::Game { .. } => ShareSource::Game,
        SourceId::Window { hwnd, pid } => ShareSource::Window {
            id: format!("{hwnd}:{pid}"),
        },
        SourceId::Monitor { device } => ShareSource::Screen { id: device.clone() },
    }
}

/// The publisher's id of what the player picked (`game_pid`: the game the launcher runs).
fn source_id(source: &ShareSource, game_pid: Option<u32>) -> Option<SourceId> {
    match source {
        ShareSource::Game => game_pid.map(|pid| SourceId::Game { pid }),
        ShareSource::Window { id } => {
            let (hwnd, pid) = id.split_once(':')?;
            Some(SourceId::Window {
                hwnd: hwnd.parse().ok()?,
                pid: pid.parse().ok()?,
            })
        }
        ShareSource::Screen { id } => Some(SourceId::Monitor { device: id.clone() }),
    }
}

/// The publisher's list as the picker shows it: the game first, then windows, then screens.
fn sources_of(list: Vec<SourceInfo>, caps: &ShareCaps) -> ShareSources {
    let mut sources = ShareSources {
        audio: caps.audio,
        ..ShareSources::default()
    };
    for info in list {
        let shown = ShareSourceInfo {
            source: source_of(&info.id),
            label: info.title,
            app: (!info.app.is_empty()).then_some(info.app),
            thumb: info
                .thumb
                .map(|jpeg| format!("data:image/jpeg;base64,{jpeg}")),
            primary: info.primary,
            width: info.w,
            height: info.h,
        };
        match info.kind {
            SourceKind::Game => sources.game = Some(shown),
            SourceKind::Window => sources.windows.push(shown),
            SourceKind::Monitor => sources.screens.push(shown),
        }
    }
    sources
}

impl Publisher for Native {
    fn caps(&self) -> (bool, bool, bool) {
        // Until the first look answers: it can share, without sound or "Yüksek".
        (true, false, false)
    }

    fn sources(&self, game_pid: Option<u32>) -> ShareSources {
        let (sender, receiver) = mpsc::channel();
        *lock(&self.waiting) = Some(sender);
        self.controller.send(pl_share::ShareCmd::Sources {
            game_pid,
            thumbs: true,
        });
        match receiver.recv_timeout(SOURCES_WAIT) {
            Ok((list, caps)) => sources_of(list, &caps),
            Err(_) => {
                lock(&self.waiting).take();
                ShareSources::default()
            }
        }
    }

    fn command(&self, cmd: ShareCmd, game_pid: Option<u32>) {
        use pl_share::ShareCmd as C;
        let cmd = match cmd {
            ShareCmd::Prepare {
                n,
                source,
                opts: chosen,
            } => match source_id(&source, game_pid) {
                Some(source) => C::Prepare {
                    n,
                    source,
                    opts: opts(chosen),
                },
                None => {
                    // The game ended before it started, or an id that is no window's.
                    self.voice.share_event(ShareEvent::SourceGone { n });
                    return;
                }
            },
            ShareCmd::Answer { n, sdp } => C::Answer {
                n,
                sdp: sdp.into_inner(),
            },
            ShareCmd::Pause { n, paused } => C::Pause { n, paused },
            ShareCmd::KeyFrame { n } => C::KeyFrame { n },
            ShareCmd::Update { n, opts: chosen } => C::Update {
                n,
                opts: opts(chosen),
            },
            ShareCmd::Stop { n } => C::Stop { n },
        };
        self.controller.send(cmd);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_ids_go_both_ways_and_odd_ones_are_none() {
        let id = SourceId::Window {
            hwnd: 0x2a01,
            pid: 4321,
        };
        let shown = source_of(&id);
        assert_eq!(source_id(&shown, None), Some(id));
        assert_eq!(
            source_id(&ShareSource::Window { id: "x:1".into() }, None),
            None
        );
        assert_eq!(source_id(&ShareSource::Game, None), None);
        assert_eq!(
            source_id(&ShareSource::Game, Some(7)),
            Some(SourceId::Game { pid: 7 })
        );
        let monitor = SourceId::Monitor {
            device: r"\\.\DISPLAY1".into(),
        };
        assert_eq!(source_id(&source_of(&monitor), None), Some(monitor));
    }
}

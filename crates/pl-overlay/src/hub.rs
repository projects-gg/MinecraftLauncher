//! The launcher's one handle on the overlay ([`Overlay`]): the player's choices, and the session of
//! the game that runs now.
//!
//! The friends feature's sink hands every update to [`Overlay::on_social`]; with no game session
//! it is dropped at once, so the overlay costs nothing between games. A launch calls
//! [`Overlay::start`] for a fresh bridge and passes the [`Endpoint`] to the game; the game's exit,
//! the launcher quitting or the switch turning off end it ([`Overlay::stop`]).

use std::io;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use pl_social::SocialUpdate;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};

use crate::policy::NotifyPrefs;
use crate::protocol::ByeReason;
use crate::server::{Bridge, Endpoint};
use crate::session::{CONNECT_WAIT, ConnectOutcome, ConnectedFn, Msg, Session, SocialActions};

/// How long [`Overlay::stop`] waits for the goodbye to leave.
const STOP_WAIT: Duration = Duration::from_secs(1);

/// The player's choices that reach the overlay (Settings → Genel "Oyun içi katman", the friends
/// notification choices, Hareketi azalt). The page pushes them at its start and on every change;
/// the launcher keeps them in its data folder for the starts before the page asks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct OverlayConfig {
    /// "Oyun içi katman" (on by default): the official pack's overlay item is installed and each
    /// game start gets a bridge.
    pub enabled: bool,
    pub notify: NotifyPrefs,
    /// Performance mode or the system's reduced motion: toasts appear in place.
    pub reduced_motion: bool,
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            notify: NotifyPrefs::default(),
            reduced_motion: false,
        }
    }
}

/// Whether the mod in the running game is connected (`overlay-status`): while it is, its toasts
/// stand in for the launcher's system notifications of the same events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OverlayStatus {
    pub connected: bool,
}

type StatusFn = Arc<dyn Fn(OverlayStatus) + Send + Sync>;

struct Running {
    id: u64,
    /// The bridge's port: names the session for [`Overlay::stop_for`].
    port: u16,
    msgs: mpsc::UnboundedSender<Msg>,
}

struct Inner {
    config: OverlayConfig,
    running: Option<Running>,
}

struct Shared {
    inner: Mutex<Inner>,
    next: AtomicU64,
    connected: AtomicBool,
    status: Mutex<Option<StatusFn>>,
}

/// Cheap to clone; the launcher keeps one.
#[derive(Clone)]
pub struct Overlay {
    shared: Arc<Shared>,
}

impl std::fmt::Debug for Overlay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Overlay")
            .field("connected", &self.connected())
            .finish_non_exhaustive()
    }
}

impl Overlay {
    pub fn new(config: OverlayConfig) -> Self {
        Self {
            shared: Arc::new(Shared {
                inner: Mutex::new(Inner {
                    config,
                    running: None,
                }),
                next: AtomicU64::new(1),
                connected: AtomicBool::new(false),
                status: Mutex::new(None),
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.shared
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn config(&self) -> OverlayConfig {
        self.lock().config.clone()
    }

    /// "Oyun içi katman" is on.
    pub fn enabled(&self) -> bool {
        self.lock().config.enabled
    }

    /// The page's choices. A session running now follows them; turning the overlay off ends it
    /// (`bye{disabled}`). Returns whether anything changed.
    pub fn configure(&self, config: OverlayConfig) -> bool {
        let ended = {
            let mut inner = self.lock();
            if inner.config == config {
                return false;
            }
            inner.config = config.clone();
            match &inner.running {
                Some(running) if config.enabled => {
                    let _ = running.msgs.send(Msg::Config(config));
                    None
                }
                Some(_) => inner.running.take(),
                None => None,
            }
        };
        if let Some(running) = ended {
            tracing::info!("the in-game overlay was turned off during the game");
            let _ = running.msgs.send(Msg::Stop {
                reason: ByeReason::Disabled,
                done: None,
            });
            self.report(running.id, false, true);
        }
        true
    }

    /// Hears whether the mod is connected (the launcher tells its page).
    pub fn on_status(&self, sink: impl Fn(OverlayStatus) + Send + Sync + 'static) {
        *self
            .shared
            .status
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Arc::new(sink));
    }

    /// The mod in the running game is connected now.
    pub fn connected(&self) -> bool {
        self.shared.connected.load(Ordering::SeqCst)
    }

    /// A game session has a bridge now.
    pub fn active(&self) -> bool {
        self.lock().running.is_some()
    }

    /// Everything the friends feature tells the launcher's page goes here too. Without a game
    /// session it is dropped at once.
    pub fn on_social(&self, update: SocialUpdate) {
        let msgs = match &self.lock().running {
            Some(running) => running.msgs.clone(),
            None => return,
        };
        let msg = match update {
            SocialUpdate::View(view) => Msg::View(Arc::from(view)),
            SocialUpdate::Notice(notice) => Msg::Notice(notice),
            SocialUpdate::Chat(change) => Msg::Chat(change),
            // "Yazıyor…" (CHAT2): the session passes it on while the panel is open.
            SocialUpdate::Typing(typing) => Msg::Typing(typing),
            // A crew tag taken off by its crew's leader (TEAM6) does not reach the game yet.
            SocialUpdate::TagOff(_) => return,
        };
        let _ = msgs.send(msg);
    }

    /// A fresh bridge for a game about to start (a session still running from an earlier start
    /// ends first). The endpoint's two JVM arguments go on the game's command line.
    pub async fn start<S: SocialActions>(
        &self,
        social: S,
        launcher: impl Into<String>,
    ) -> io::Result<Endpoint> {
        self.stop(ByeReason::LauncherExit).await;
        let (bridge, endpoint) = Bridge::bind().await?;
        let id = self.shared.next.fetch_add(1, Ordering::Relaxed);
        let (control, control_rx) = mpsc::unbounded_channel();
        let (events_tx, events) = mpsc::unbounded_channel();
        let (msgs, msgs_rx) = mpsc::unbounded_channel();
        let config = {
            let mut inner = self.lock();
            inner.running = Some(Running {
                id,
                port: endpoint.port(),
                msgs: msgs.clone(),
            });
            inner.config.clone()
        };
        let hub = self.clone();
        let connected: ConnectedFn = Arc::new(move |on| hub.report(id, on, false));
        let session = Session::new(social, launcher.into(), config, control, connected);
        tokio::spawn(bridge.serve(control_rx, events_tx));
        tokio::spawn(session.run(msgs_rx, events));
        tracing::info!(port = endpoint.port(), "in-game overlay bridge ready");
        Ok(endpoint)
    }

    /// "Bağlan" (a crew call, an invite, a plan), which the player chose and confirmed in the
    /// launcher: the running game joins `target` (`host[:port]`) through its overlay instead of
    /// starting again. Never call it on the launcher's own initiative (a reminder, a timer).
    ///
    /// Only Projects or an address a friend or crew shares right now, one at a time, and only with
    /// a mod that can (`connect` in its hello). [`ConnectOutcome::Unavailable`] when any of that
    /// fails or the game does not answer within [`CONNECT_WAIT`]: the caller then starts the game
    /// again as before.
    pub async fn connect_in_game(&self, target: &str) -> ConnectOutcome {
        let msgs = match &self.lock().running {
            Some(running) => running.msgs.clone(),
            None => return ConnectOutcome::unavailable("no-overlay"),
        };
        let (reply, answer) = oneshot::channel();
        let asked = Msg::Connect {
            target: target.to_owned(),
            reply,
        };
        if msgs.send(asked).is_err() {
            return ConnectOutcome::unavailable("no-overlay");
        }
        match tokio::time::timeout(CONNECT_WAIT, answer).await {
            Ok(Ok(outcome)) => outcome,
            Ok(Err(_)) => ConnectOutcome::unavailable("no-overlay"),
            Err(_) => ConnectOutcome::unavailable("timeout"),
        }
    }

    /// Ends the running session, if any: the mod hears `bye{reason}` (when it is still there) and
    /// the bridge closes. Waits for that, at most [`STOP_WAIT`].
    pub async fn stop(&self, reason: ByeReason) {
        self.end(|_| true, reason).await;
    }

    /// [`Overlay::stop`] for the session of the bridge on `port` only (its game ended, its launch
    /// stopped): a newer game's session is left alone.
    pub async fn stop_for(&self, port: u16, reason: ByeReason) {
        self.end(|running| running.port == port, reason).await;
    }

    async fn end(&self, which: impl FnOnce(&Running) -> bool, reason: ByeReason) {
        let running = {
            let mut inner = self.lock();
            match inner.running.as_ref() {
                Some(running) if which(running) => inner.running.take(),
                _ => None,
            }
        };
        let Some(running) = running else {
            return;
        };
        self.report(running.id, false, true);
        let (done, closed) = oneshot::channel();
        if running
            .msgs
            .send(Msg::Stop {
                reason,
                done: Some(done),
            })
            .is_ok()
        {
            let _ = tokio::time::timeout(STOP_WAIT, closed).await;
        }
    }

    /// A session says whether its mod is connected; only the running one counts (`ended`: the
    /// hub itself, for a session it just took away).
    fn report(&self, session: u64, connected: bool, ended: bool) {
        let current = self.lock().running.as_ref().map(|running| running.id) == Some(session);
        if !current && !ended {
            return;
        }
        if self.shared.connected.swap(connected, Ordering::SeqCst) == connected {
            return;
        }
        let sink = self
            .shared
            .status
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        if let Some(sink) = sink {
            sink(OverlayStatus { connected });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_config_reads_the_pages_json_and_defaults_to_on() {
        let config: OverlayConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(config, OverlayConfig::default());
        assert!(config.enabled && !config.notify.crew_chat && config.notify.crew_calls);
        let config: OverlayConfig = serde_json::from_str(
            r#"{"enabled":false,"reducedMotion":true,"notify":{"crewChat":true},"later":1}"#,
        )
        .unwrap();
        assert!(!config.enabled && config.reduced_motion && config.notify.crew_chat);
    }

    #[test]
    fn configure_says_whether_anything_changed() {
        let overlay = Overlay::new(OverlayConfig::default());
        assert!(!overlay.configure(OverlayConfig::default()));
        let off = OverlayConfig {
            enabled: false,
            ..OverlayConfig::default()
        };
        assert!(overlay.configure(off.clone()));
        assert!(!overlay.enabled());
        assert_eq!(overlay.config(), off);
        // Without a game nothing runs, and updates are dropped.
        assert!(!overlay.active() && !overlay.connected());
        overlay.on_social(SocialUpdate::Notice(
            pl_social::SocialNotice::SignedOutRemotely,
        ));
    }
}

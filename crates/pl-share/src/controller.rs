//! The controller: one thread that owns every share and answers [`ShareCmd`]s with
//! [`ShareEvent`]s. Media work runs on each share's own threads; this one only coordinates, so a
//! slow command never stalls a running share.

use std::sync::mpsc::{Receiver, Sender, channel};

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tracing::warn;

use crate::api::{ShareCmd, ShareEnv, ShareEvent, ShareNo};

pub type EventTx = UnboundedSender<ShareEvent>;

/// Messages to the controller thread.
pub enum Ctl {
    Cmd(ShareCmd),
    /// A share ended on its own (lost, source gone, encoder failure); `generation` tells an old
    /// share from a newer one with the same number.
    Ended {
        n: ShareNo,
        generation: u64,
    },
    Quit,
}

#[cfg(windows)]
use crate::win::session::Backend;

#[cfg(not(windows))]
use self::stub::Backend;

/// The launcher's handle. Dropping it stops every share.
pub struct ShareController {
    tx: Sender<Ctl>,
}

impl ShareController {
    /// Starts the controller thread; events arrive on the returned receiver.
    pub fn start(env: ShareEnv) -> std::io::Result<(Self, UnboundedReceiver<ShareEvent>)> {
        let (events, rx_events) = unbounded_channel();
        let (tx, rx) = channel();
        let internal = tx.clone();
        std::thread::Builder::new()
            .name("pl-share".into())
            .spawn(move || run(env, rx, internal, events))?;
        Ok((Self { tx }, rx_events))
    }

    pub fn send(&self, cmd: ShareCmd) {
        if self.tx.send(Ctl::Cmd(cmd)).is_err() {
            warn!("share controller is gone");
        }
    }
}

impl Drop for ShareController {
    fn drop(&mut self) {
        let _ = self.tx.send(Ctl::Cmd(ShareCmd::StopAll));
        let _ = self.tx.send(Ctl::Quit);
    }
}

fn run(env: ShareEnv, rx: Receiver<Ctl>, internal: Sender<Ctl>, events: EventTx) {
    let mut backend = Backend::new(env, events, internal);
    while let Ok(msg) = rx.recv() {
        match msg {
            Ctl::Cmd(cmd) => backend.handle(cmd),
            Ctl::Ended { n, generation } => backend.ended(n, generation),
            Ctl::Quit => break,
        }
    }
    backend.handle(ShareCmd::StopAll);
}

#[cfg(not(windows))]
mod stub {
    //! Other systems: nothing to share yet (the picker shows nothing, a start fails).
    use std::sync::mpsc::Sender;

    use super::{Ctl, EventTx};
    use crate::api::{LostWhy, ShareCaps, ShareCmd, ShareEnv, ShareEvent, ShareNo};

    pub struct Backend {
        events: EventTx,
    }

    impl Backend {
        pub fn new(_env: ShareEnv, events: EventTx, _internal: Sender<Ctl>) -> Self {
            Self { events }
        }

        pub fn handle(&mut self, cmd: ShareCmd) {
            let event = match cmd {
                ShareCmd::Sources { .. } => ShareEvent::Sources {
                    list: Vec::new(),
                    caps: ShareCaps {
                        audio: false,
                        border_off: false,
                        cursor_toggle: false,
                        hardware_encoder: None,
                    },
                },
                ShareCmd::Prepare { n, .. } => ShareEvent::Lost {
                    n,
                    why: LostWhy::Capture,
                },
                ShareCmd::Stop { n } => ShareEvent::Stopped { n },
                _ => return,
            };
            let _ = self.events.send(event);
        }

        pub fn ended(&mut self, _n: ShareNo, _generation: u64) {}
    }
}

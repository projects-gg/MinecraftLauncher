//! Screen sharing's side of [`Voice`] (SHARE): the page's share commands and the publisher's and
//! the watching page's news, each one step of the machine (a child of [`crate::manager`]).

use super::{Voice, lock};
use crate::share::{ShareEvent, ShareLayer, ShareRefusal, ShareSource, ShareSurface, WatchReport};
use crate::view::VoiceAnswer;

impl Voice {
    /// Whether this computer can share (the publisher works) and share an app's own sound.
    pub fn set_share_publisher(&self, available: bool, audio: bool) {
        self.effects(|machine, _, _| machine.set_share_publisher(available, audio));
    }

    /// "Paylaş" in the picker (`label`: the window's title or the screen's name, this computer
    /// only). The room's answer shows in the state ("Yayında") or as a `share-notice`.
    pub fn share_start(&self, source: ShareSource, label: String) -> Result<(), ShareRefusal> {
        self.step(
            |machine, now, unix| match machine.share_start(source, &label, now, unix) {
                Ok(effects) => (Ok(()), effects),
                Err(refusal) => (Err(refusal), Vec::new()),
            },
        )
    }

    /// "Durdur" on one of the player's shares.
    pub fn share_stop(&self, n: u8) {
        self.effects(|machine, now, _| machine.share_stop(n, now));
    }

    /// Every share of the player's stops (the tray's "Paylaşımı durdur").
    pub fn share_stop_all(&self) {
        self.effects(|machine, now, _| {
            let mut out = Vec::new();
            for n in machine.sharing() {
                out.extend(machine.share_stop(n, now));
            }
            out
        });
    }

    /// The player's shares now.
    pub fn sharing(&self) -> Vec<u8> {
        lock(&self.inner.machine).sharing()
    }

    /// The publisher's news (its own threads).
    pub fn share_event(&self, event: ShareEvent) {
        self.effects(|machine, now, _| machine.share_event(event, now));
    }

    /// "İzle".
    pub fn share_watch(&self, person: String, n: u8) -> Result<(), ShareRefusal> {
        self.step(
            |machine, now, _| match machine.share_watch(&person, n, now) {
                Ok(effects) => (Ok(()), effects),
                Err(refusal) => (Err(refusal), Vec::new()),
            },
        )
    }

    /// "İzlemeyi bırak".
    pub fn share_unwatch(&self, person: String, n: u8) {
        self.effects(|machine, now, _| machine.share_unwatch(&person, n, now));
    }

    /// The tile of a watched share grew or shrank.
    pub fn share_layer(&self, person: String, n: u8, layer: ShareLayer) {
        self.effects(|machine, _, _| machine.share_layer(&person, n, layer));
    }

    /// A page that watches attached its channel.
    pub fn share_attach(&self, surface: ShareSurface) {
        self.effects(|machine, _, _| machine.share_attach(surface));
    }

    /// A page that watched went away.
    pub fn share_detach(&self, surface: ShareSurface) {
        self.effects(|machine, now, _| machine.share_detach(surface, now));
    }

    /// "Ayrı pencerede aç" (`open`), or back into the launcher's window.
    pub fn share_popout(&self, open: bool) {
        self.effects(|machine, now, _| machine.share_popout(open, now));
    }

    /// The page that watches went out of sight or came back.
    pub fn share_visible(&self, surface: ShareSurface, visible: bool) {
        self.effects(|machine, now, _| machine.share_visible(surface, visible, now));
    }

    /// The watching page's report.
    pub fn share_watch_report(&self, surface: ShareSurface, report: WatchReport) {
        self.effects(|machine, now, _| machine.share_watch_report(surface, report, now));
    }

    /// "Evet" on "Hâlâ izliyor musun?".
    pub fn share_still(&self) {
        self.effects(|machine, now, _| machine.share_still(now));
    }

    /// A sharer's stream sound for the player.
    pub fn set_stream_volume(&self, person: String, volume: u16, muted: bool) {
        self.effects(|machine, _, _| machine.set_stream_volume(&person, volume, muted));
    }

    /// What the service allows of screen sharing (`hello.voice.share`; none: off).
    pub fn set_share_service(&self, service: Option<crate::share::ServiceShare>) {
        self.effects(|machine, now, _| machine.set_share_service(service, now));
    }

    /// "Kim paylaşabilir" in a friend room (its leader).
    pub async fn share_set_sharers(&self, who: crate::share::ShareBy) -> VoiceAnswer {
        self.request(|machine| machine.share_set_sharers(who)).await
    }

    /// "Paylaşımı durdur" on someone else's share (a leader or moderator).
    pub async fn share_unshare(&self, person: String, n: u8) -> VoiceAnswer {
        self.request(|machine| machine.share_unshare(&person, n))
            .await
    }
}

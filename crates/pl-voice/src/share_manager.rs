//! Screen sharing's side of [`Voice`] (SHARE): the page's share commands and the publisher's and
//! the watching page's news, each one step of the machine (a child of [`crate::manager`]).

use super::{Voice, lock};
use crate::share::{
    ShareChange, ShareEvent, ShareLayer, ShareRefusal, ShareSource, ShareSurface, ShareViewPrefs,
    WatchReport,
};
use crate::view::VoiceAnswer;

impl Voice {
    /// Whether this computer can share (the publisher works), share an app's own sound, and
    /// encode "Yüksek" in hardware.
    pub fn set_share_publisher(&self, available: bool, audio: bool, high: bool) {
        self.effects(|machine, _, _| machine.set_share_publisher(available, audio, high));
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
        self.effects(|machine, now, _| machine.share_attach(surface, now));
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

    // SQOL ---------------------------------------------------------------------------------------

    /// "Ekranı Değiştir" on a live share (`label`: the window's title or the screen's name, this
    /// computer only). The result shows in the state and as a `share-notice`.
    pub fn share_switch(
        &self,
        n: u8,
        source: ShareSource,
        label: String,
    ) -> Result<(), ShareRefusal> {
        self.step(
            |machine, _, _| match machine.share_switch(n, source, &label) {
                Ok(effects) => (Ok(()), effects),
                Err(refusal) => (Err(refusal), Vec::new()),
            },
        )
    }

    /// A live change of a share (quality, frame rate, "Görüntü", cursor, sound, still picture).
    pub fn share_update(&self, n: u8, change: ShareChange) -> Result<(), ShareRefusal> {
        self.step(
            |machine, now, _| match machine.share_update(n, change, now) {
                Ok(effects) => (Ok(()), effects),
                Err(refusal) => (Err(refusal), Vec::new()),
            },
        )
    }

    /// "Duraklat" (`on`) / "Sürdür".
    pub fn share_hold(&self, n: u8, on: bool) -> Result<(), ShareRefusal> {
        self.step(|machine, _, _| match machine.share_hold(n, on) {
            Ok(effects) => (Ok(()), effects),
            Err(refusal) => (Err(refusal), Vec::new()),
        })
    }

    /// The player's own small live preview of a share (its card on screen).
    pub fn share_preview(&self, n: u8, on: bool) {
        self.effects(|machine, _, _| machine.share_preview(n, on));
    }

    /// The launcher started a game: a share whose game closed takes it.
    pub fn share_game_started(&self) {
        self.effects(|machine, _, _| machine.share_game_started());
    }

    /// The watching choices ("Yayın Kalitesi", the stream's ducking) from either window.
    pub fn share_view_prefs(&self, prefs: ShareViewPrefs) {
        let mut settings = self.settings();
        let before = settings.share.clone();
        prefs.apply(&mut settings.share);
        if settings.share != before {
            self.save_settings(settings);
        }
    }
}

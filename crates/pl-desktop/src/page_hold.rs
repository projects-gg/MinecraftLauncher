//! What a sleeping page is told once it wakes (LIGHT; the launcher's `webview_rest`). A suspended
//! WebView2 page runs no script, so whatever is sent to it would wait inside WebView2, every event
//! of it, for as long as the game runs, and run all at once when the player comes back. Events
//! offered here instead wait as their kind allows: a state only as its newest, a stream in order
//! (up to a bound), a passing one not at all, and a moment worth telling wakes the page.

/// How an event reaches a page that may be asleep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    /// A whole state (a friends view): only the newest waits.
    Latest,
    /// Each one counts, in order (a chat's changes).
    InOrder,
    /// The page has to act now (a notification to show): it wakes first.
    Moment,
    /// Means nothing later (someone typing): dropped while the page sleeps.
    Passing,
}

/// What became of an offered event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Offer {
    /// The page is awake: the caller sends it now.
    Send,
    /// It waits for the page to wake.
    Kept,
    /// It waits, and the page should wake now to take it (a moment, or too many waiting).
    Wake,
    /// Dropped (a passing event while the page sleeps).
    Dropped,
}

/// The events kept for a sleeping page, in the order they came.
#[derive(Debug)]
pub struct PageHold<T> {
    asleep: bool,
    events: Vec<(&'static str, T)>,
    max: usize,
}

impl<T> PageHold<T> {
    /// `max`: past this many waiting events the page is woken to take them.
    pub const fn new(max: usize) -> Self {
        Self {
            asleep: false,
            events: Vec::new(),
            max,
        }
    }

    pub fn asleep(&self) -> bool {
        self.asleep
    }

    /// The page was suspended: events wait from now on.
    pub fn fell_asleep(&mut self) {
        self.asleep = true;
    }

    /// Offers an event; `payload` makes what is kept (`None`: it cannot be kept, and the page is
    /// woken so nothing is lost).
    pub fn offer(
        &mut self,
        event: &'static str,
        delivery: Delivery,
        payload: impl FnOnce() -> Option<T>,
    ) -> Offer {
        if !self.asleep {
            return Offer::Send;
        }
        if delivery == Delivery::Passing {
            return Offer::Dropped;
        }
        let Some(payload) = payload() else {
            return Offer::Wake;
        };
        if delivery == Delivery::Latest {
            self.events.retain(|(name, _)| *name != event);
        }
        self.events.push((event, payload));
        if delivery == Delivery::Moment || self.events.len() > self.max {
            Offer::Wake
        } else {
            Offer::Kept
        }
    }

    /// The page woke: the next events to send, in order. `None` once nothing waits; the page then
    /// counts as awake, so what comes while a batch is sent waits for the next call.
    pub fn take(&mut self) -> Option<Vec<(&'static str, T)>> {
        if self.events.is_empty() {
            self.asleep = false;
            return None;
        }
        Some(std::mem::take(&mut self.events))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offered(hold: &mut PageHold<u32>, event: &'static str, delivery: Delivery, n: u32) -> Offer {
        hold.offer(event, delivery, || Some(n))
    }

    #[test]
    fn an_awake_page_gets_every_event_at_once() {
        let mut hold = PageHold::new(10);
        for delivery in [
            Delivery::Latest,
            Delivery::InOrder,
            Delivery::Moment,
            Delivery::Passing,
        ] {
            assert_eq!(offered(&mut hold, "e", delivery, 1), Offer::Send);
        }
        assert!(hold.take().is_none());
    }

    #[test]
    fn a_sleeping_page_keeps_the_newest_state_streams_in_order_and_no_passing_events() {
        let mut hold = PageHold::new(10);
        hold.fell_asleep();
        assert_eq!(offered(&mut hold, "view", Delivery::Latest, 1), Offer::Kept);
        assert_eq!(
            offered(&mut hold, "chat", Delivery::InOrder, 2),
            Offer::Kept
        );
        assert_eq!(
            offered(&mut hold, "typing", Delivery::Passing, 3),
            Offer::Dropped
        );
        assert_eq!(
            offered(&mut hold, "chat", Delivery::InOrder, 4),
            Offer::Kept
        );
        // A newer view replaces the older one and goes after what came before it.
        assert_eq!(offered(&mut hold, "view", Delivery::Latest, 5), Offer::Kept);
        assert!(hold.asleep());
        assert_eq!(
            hold.take(),
            Some(vec![("chat", 2), ("chat", 4), ("view", 5)])
        );
        assert!(hold.asleep(), "awake only once nothing waits");
        assert!(hold.take().is_none());
        assert!(!hold.asleep());
        assert_eq!(offered(&mut hold, "view", Delivery::Latest, 6), Offer::Send);
    }

    #[test]
    fn a_moment_wakes_the_page_after_what_waits() {
        let mut hold = PageHold::new(10);
        hold.fell_asleep();
        assert_eq!(offered(&mut hold, "view", Delivery::Latest, 1), Offer::Kept);
        assert_eq!(
            offered(&mut hold, "notice", Delivery::Moment, 2),
            Offer::Wake
        );
        // What comes while the page wakes waits behind the moment.
        assert_eq!(offered(&mut hold, "view", Delivery::Latest, 3), Offer::Kept);
        assert_eq!(
            hold.take(),
            Some(vec![("notice", 2), ("view", 3)]),
            "the newer view replaced the older one"
        );
        assert!(hold.take().is_none());
    }

    #[test]
    fn too_many_waiting_events_wake_the_page_and_none_is_lost() {
        let mut hold = PageHold::new(3);
        hold.fell_asleep();
        for n in 0..3 {
            assert_eq!(
                offered(&mut hold, "chat", Delivery::InOrder, n),
                Offer::Kept
            );
        }
        assert_eq!(
            offered(&mut hold, "chat", Delivery::InOrder, 3),
            Offer::Wake
        );
        assert_eq!(hold.take().map(|events| events.len()), Some(4));
    }

    #[test]
    fn an_event_that_cannot_be_kept_wakes_the_page() {
        let mut hold: PageHold<u32> = PageHold::new(3);
        hold.fell_asleep();
        assert_eq!(hold.offer("odd", Delivery::Latest, || None), Offer::Wake);
        assert!(hold.take().is_none());
    }
}

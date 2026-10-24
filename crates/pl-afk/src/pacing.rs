//! Connection pacing per destination (owner, 24 Oct 2026: a server must never throttle the
//! player's address). Proxies count logins per client address: Velocity's `login-ratelimit`
//! turns away a second login within 3000 ms, BungeeCord's `connection_throttle` within 4000 ms,
//! and such a refusal is a failure that backs the account off. A player's AFK accounts share one
//! address, so their connection attempts to one server are kept [`MIN_CONNECT_SPACING`] apart and
//! go in the order they asked: after a server restart the accounts come back one by one, the one
//! that has waited longest first. The cloud machine (`afk-host`) keeps the same spacing per
//! resolved target address.

use std::collections::HashMap;
use std::time::Duration;

use parking_lot::Mutex;
use tokio::time::Instant;

/// Least time between two connection attempts to one destination: above BungeeCord's 4 s, with
/// room for the client's own start-up time varying between attempts.
pub const MIN_CONNECT_SPACING: Duration = Duration::from_millis(4_500);

/// Hands out connection slots per destination.
#[derive(Debug, Default)]
pub struct ConnectPacer {
    /// Destination → the latest slot handed out.
    last: Mutex<HashMap<String, Instant>>,
}

impl ConnectPacer {
    /// Takes the next slot for a connection to `destination` asked for at `now`: `now` itself
    /// when the latest slot there is at least `spacing` earlier, else `spacing` after that slot.
    /// Slots go out in the order they are asked for and stay taken, so a caller waits for its own
    /// slot instead of asking again.
    pub fn reserve(&self, destination: &str, spacing: Duration, now: Instant) -> Instant {
        let mut last = self.last.lock();
        // Destinations whose next slot is free anyway (slots still ahead stay).
        last.retain(|_, at| *at + spacing > now);
        let slot = last
            .get(destination)
            .map_or(now, |at| (*at + spacing).max(now));
        last.insert(destination.to_owned(), slot);
        slot
    }
}

/// The destination of an account's connections: its server's host name as the client dials it
/// (lower case, no trailing dot). The port is left out on purpose: servers on one host sit behind
/// one address, and waiting a few seconds too many costs less than a refused login.
pub fn connect_destination(host: &str) -> String {
    host.trim().trim_end_matches('.').to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPACING: Duration = MIN_CONNECT_SPACING;

    fn ms(from: Instant, to: Instant) -> u128 {
        to.duration_since(from).as_millis()
    }

    #[test]
    fn attempts_to_one_destination_go_one_by_one_in_the_order_asked() {
        let pacer = ConnectPacer::default();
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        // Three accounts back from a server restart within half a second: 0, 4.5 and 9 s.
        let first = pacer.reserve("play.projects.gg", SPACING, at(0));
        let second = pacer.reserve("play.projects.gg", SPACING, at(200));
        let third = pacer.reserve("play.projects.gg", SPACING, at(500));
        assert_eq!(
            [ms(start, first), ms(start, second), ms(start, third)],
            [0, 4_500, 9_000]
        );
        // Another server is not held up.
        assert_eq!(pacer.reserve("mc.example.org", SPACING, at(600)), at(600));
        // A late one queues behind the slots already taken, not behind the clock.
        assert_eq!(
            ms(start, pacer.reserve("play.projects.gg", SPACING, at(1_000))),
            13_500
        );
        // Once the last slot is a spacing in the past, the next attempt goes at once.
        assert_eq!(
            pacer.reserve("play.projects.gg", SPACING, at(20_000)),
            at(20_000)
        );
        assert_eq!(
            ms(
                start,
                pacer.reserve("play.projects.gg", SPACING, at(20_001))
            ),
            24_500
        );
    }

    #[test]
    fn slots_are_never_closer_than_the_spacing() {
        let pacer = ConnectPacer::default();
        let start = Instant::now();
        let mut slots = Vec::new();
        // Twenty accounts asking at uneven moments over ten seconds.
        for i in 0..20u64 {
            let asked = start + Duration::from_millis(i * i * 25);
            let slot = pacer.reserve("play.projects.gg", SPACING, asked);
            assert!(slot >= asked, "{i}");
            slots.push(slot);
        }
        for pair in slots.windows(2) {
            assert!(pair[1].duration_since(pair[0]) >= SPACING, "{pair:?}");
        }
    }

    #[test]
    fn no_spacing_never_waits_and_old_destinations_are_forgotten() {
        let pacer = ConnectPacer::default();
        let start = Instant::now();
        assert_eq!(pacer.reserve("a", Duration::ZERO, start), start);
        assert_eq!(pacer.reserve("a", Duration::ZERO, start), start);
        pacer.reserve("b", SPACING, start);
        pacer.reserve("c", SPACING, start + Duration::from_secs(60));
        assert_eq!(
            pacer.last.lock().len(),
            1,
            "only the recent destination is kept"
        );
    }

    #[test]
    fn destinations_ignore_case_spaces_and_the_root_dot() {
        assert_eq!(
            connect_destination(" Play.Projects.GG. "),
            "play.projects.gg"
        );
        assert_eq!(connect_destination("mc.example.org"), "mc.example.org");
    }
}

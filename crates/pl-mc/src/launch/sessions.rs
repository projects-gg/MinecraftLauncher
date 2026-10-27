//! Play time of games that ran next to each other (PLAY-AGAIN). Each game's session counts toward
//! its instance's play time when it ends, as it always did; two games of one instance at the same
//! time are one player's time, so a session adds only what no session of that instance counted
//! before covers ([`SessionLedger`]).

use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use crate::launch::detached::LONGEST_SESSION;

/// The sessions counted this launcher run, per instance.
#[derive(Debug, Default)]
pub struct SessionLedger {
    counted: HashMap<String, Vec<(SystemTime, SystemTime)>>,
}

impl SessionLedger {
    pub fn new() -> Self {
        Self::default()
    }

    /// Counts the run `start..end` of `instance` and answers the part of it no session counted
    /// before covers: what it adds to the instance's play time. Sessions that ended a long while
    /// before this one began are forgotten (no game runs that long).
    pub fn count(&mut self, instance: &str, start: SystemTime, end: SystemTime) -> Duration {
        if end <= start {
            return Duration::ZERO;
        }
        let list = self.counted.entry(instance.to_owned()).or_default();
        list.retain(|&(_, ended)| ended + LONGEST_SESSION >= start);
        let fresh = uncovered(start, end, list);
        list.push((start, end));
        fresh
    }
}

/// The part of `start..end` that none of `counted` covers.
pub fn uncovered(
    start: SystemTime,
    end: SystemTime,
    counted: &[(SystemTime, SystemTime)],
) -> Duration {
    let whole = end.duration_since(start).unwrap_or_default();
    let mut parts: Vec<(SystemTime, SystemTime)> = counted
        .iter()
        .filter_map(|&(from, to)| {
            let (from, to) = (from.max(start), to.min(end));
            (from < to).then_some((from, to))
        })
        .collect();
    parts.sort();
    let mut covered = Duration::ZERO;
    let mut reached = start;
    for (from, to) in parts {
        let from = from.max(reached);
        if to > from {
            covered += to.duration_since(from).unwrap_or_default();
            reached = to;
        }
    }
    whole.saturating_sub(covered)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(minutes: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_792_000_000 + minutes * 60)
    }

    fn mins(n: u64) -> Duration {
        Duration::from_secs(n * 60)
    }

    #[test]
    fn games_of_one_instance_side_by_side_count_once() {
        let mut ledger = SessionLedger::new();
        // A second game opened inside the first one's hour and closed first: nothing new.
        assert_eq!(ledger.count("projects", at(10), at(20)), mins(10));
        assert_eq!(ledger.count("projects", at(0), at(60)), mins(50));
        // One that went on after both: only its own stretch.
        assert_eq!(ledger.count("projects", at(50), at(90)), mins(30));
        // Another instance is another player's time on another game: counted whole.
        assert_eq!(ledger.count("hardcore", at(0), at(60)), mins(60));
        // A later session alone counts whole, and a run that is not one counts nothing.
        assert_eq!(ledger.count("projects", at(120), at(150)), mins(30));
        assert_eq!(ledger.count("projects", at(200), at(200)), Duration::ZERO);
        assert_eq!(ledger.count("projects", at(210), at(200)), Duration::ZERO);
    }

    #[test]
    fn overlaps_are_merged_before_they_are_taken_away() {
        let counted = [(at(0), at(30)), (at(10), at(40)), (at(50), at(55))];
        assert_eq!(uncovered(at(0), at(60), &counted), mins(15));
        assert_eq!(uncovered(at(40), at(50), &counted), mins(10));
        assert_eq!(uncovered(at(0), at(60), &[]), mins(60));
    }

    #[test]
    fn old_sessions_are_forgotten() {
        let mut ledger = SessionLedger::new();
        ledger.count("projects", at(0), at(60));
        let day_later = 60 + LONGEST_SESSION.as_secs() / 60 + 1;
        assert_eq!(
            ledger.count("projects", at(day_later), at(day_later + 10)),
            mins(10)
        );
        assert_eq!(ledger.counted["projects"].len(), 1, "the old one went");
    }
}

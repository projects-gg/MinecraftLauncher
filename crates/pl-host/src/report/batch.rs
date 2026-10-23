//! Sending telemetry (PROTOCOL.md §12.5): events wait at most 60 s and go 20 to a frame; a
//! session's end and every `u` event flush at once; at most 20 frames a minute. While the presence
//! socket is down events are dropped, except TURN bytes, which are carried to the next frame.
//!
//! [`Reporter`] is a plain state machine: the caller gives it the time and sends the frames it
//! returns, and wakes it at [`Reporter::next_deadline`].

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use super::{Event, MAX_EVENTS, Role, frame};
use crate::tunnel::SessionId;

/// How long an event waits for others.
pub const FLUSH_AFTER: Duration = Duration::from_secs(60);
/// Frames allowed per minute.
pub const MAX_FRAMES_PER_MINUTE: usize = 20;
/// Events kept while frames are held back by the rate; older ones go first.
const MAX_QUEUED: usize = MAX_EVENTS * MAX_FRAMES_PER_MINUTE;

struct Queue {
    mc: String,
    events: Vec<Event>,
    since: Instant,
    urgent: bool,
}

/// Groups events into `world_report` frames.
pub struct Reporter {
    launcher_version: String,
    online: bool,
    queues: Vec<Queue>,
    sent: VecDeque<Instant>,
    /// TURN bytes of `u` events that could not go, per session and role.
    carried: Vec<(SessionId, Role, u64, String)>,
}

impl Reporter {
    /// `launcher_version`: the frames' `lv`.
    pub fn new(launcher_version: impl Into<String>) -> Self {
        Self {
            launcher_version: launcher_version.into(),
            online: true,
            queues: Vec::new(),
            sent: VecDeque::new(),
            carried: Vec::new(),
        }
    }

    /// The presence socket went down or came back. Going down drops what waits (TURN bytes are
    /// carried); coming back sends carried TURN bytes at once.
    pub fn set_online(&mut self, online: bool, now: Instant) -> Vec<String> {
        if self.online == online {
            return Vec::new();
        }
        self.online = online;
        if !online {
            for queue in std::mem::take(&mut self.queues) {
                for event in queue.events {
                    self.carry(&queue.mc, &event);
                }
            }
            return Vec::new();
        }
        for (session, role, turn_rx, mc) in std::mem::take(&mut self.carried) {
            self.enqueue(
                &mc,
                Event::Usage {
                    session,
                    role,
                    turn_rx,
                },
                now,
                true,
            );
        }
        self.poll(now)
    }

    /// Queues an event for a session on Minecraft `mc`; returns the frames to send now.
    pub fn push(&mut self, mc: &str, event: Event, now: Instant) -> Vec<String> {
        if !event.is_valid() {
            return Vec::new();
        }
        if !self.online {
            self.carry(mc, &event);
            return Vec::new();
        }
        let event = match event {
            Event::Usage {
                session,
                role,
                turn_rx,
            } => {
                let extra = self.take_carried(session, role);
                Event::Usage {
                    session,
                    role,
                    turn_rx: turn_rx.saturating_add(extra),
                }
            }
            other => other,
        };
        let urgent = matches!(event, Event::Session { .. } | Event::Usage { .. });
        self.enqueue(mc, event, now, urgent);
        self.poll(now)
    }

    /// The frames due at `now`.
    pub fn poll(&mut self, now: Instant) -> Vec<String> {
        while self
            .sent
            .front()
            .is_some_and(|t| now.duration_since(*t) >= Duration::from_secs(60))
        {
            self.sent.pop_front();
        }
        let mut frames = Vec::new();
        let mut i = 0;
        while i < self.queues.len() {
            let queue = &mut self.queues[i];
            let due = queue.urgent
                || queue.events.len() >= MAX_EVENTS
                || now.duration_since(queue.since) >= FLUSH_AFTER;
            if !due || queue.events.is_empty() {
                i += 1;
                continue;
            }
            if self.sent.len() >= MAX_FRAMES_PER_MINUTE {
                break;
            }
            let take = queue.events.len().min(MAX_EVENTS);
            let batch: Vec<Event> = queue.events.drain(..take).collect();
            frames.push(frame(&queue.mc, &self.launcher_version, &batch));
            self.sent.push_back(now);
            if queue.events.is_empty() {
                self.queues.remove(i);
            } else {
                queue.since = now;
            }
        }
        frames
    }

    /// When [`Reporter::poll`] next has something to send.
    pub fn next_deadline(&self) -> Option<Instant> {
        let rate_free = if self.sent.len() >= MAX_FRAMES_PER_MINUTE {
            self.sent.front().map(|t| *t + Duration::from_secs(60))
        } else {
            None
        };
        let due = self
            .queues
            .iter()
            .filter(|q| !q.events.is_empty())
            .map(|q| {
                if q.urgent || q.events.len() >= MAX_EVENTS {
                    q.since
                } else {
                    q.since + FLUSH_AFTER
                }
            })
            .min()?;
        Some(rate_free.map_or(due, |free| free.max(due)))
    }

    fn enqueue(&mut self, mc: &str, event: Event, now: Instant, urgent: bool) {
        let queue = match self.queues.iter_mut().position(|q| q.mc == mc) {
            Some(i) => &mut self.queues[i],
            None => {
                self.queues.push(Queue {
                    mc: mc.to_owned(),
                    events: Vec::new(),
                    since: now,
                    urgent: false,
                });
                self.queues.last_mut().expect("just pushed")
            }
        };
        if queue.events.is_empty() {
            queue.since = now;
        }
        queue.events.push(event);
        queue.urgent |= urgent;
        let total: usize = self.queues.iter().map(|q| q.events.len()).sum();
        if total > MAX_QUEUED {
            // Held back by the rate for long: the oldest event that is not TURN usage goes.
            let not_usage = |e: &Event| !matches!(e, Event::Usage { .. });
            if let Some(queue) = self
                .queues
                .iter_mut()
                .find(|q| q.events.iter().any(not_usage))
                && let Some(i) = queue.events.iter().position(not_usage)
            {
                queue.events.remove(i);
            }
        }
    }

    fn carry(&mut self, mc: &str, event: &Event) {
        if let Event::Usage {
            session,
            role,
            turn_rx,
        } = event
        {
            match self
                .carried
                .iter_mut()
                .find(|(s, r, _, _)| s == session && r == role)
            {
                Some(entry) => entry.2 = entry.2.saturating_add(*turn_rx),
                None => self
                    .carried
                    .push((*session, *role, *turn_rx, mc.to_owned())),
            }
        }
    }

    fn take_carried(&mut self, session: SessionId, role: Role) -> u64 {
        match self
            .carried
            .iter()
            .position(|(s, r, _, _)| *s == session && *r == role)
        {
            Some(i) => self.carried.remove(i).2,
            None => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;
    use crate::transport::{PathKind, Step};

    fn attempt() -> Event {
        Event::attempt(
            PathKind::Turn,
            Role::Guest,
            Step::Plj,
            "ok",
            Duration::from_millis(120),
            None,
        )
        .unwrap()
    }

    fn events_of(frame: &str) -> Vec<Value> {
        serde_json::from_str::<Value>(frame).unwrap()["r"]
            .as_array()
            .unwrap()
            .clone()
    }

    #[test]
    fn events_wait_a_minute_or_for_twenty() {
        let start = Instant::now();
        let mut reporter = Reporter::new("2.1.0");
        assert!(reporter.push("26.1.2", attempt(), start).is_empty());
        assert_eq!(reporter.next_deadline(), Some(start + FLUSH_AFTER));
        assert!(reporter.poll(start + Duration::from_secs(59)).is_empty());
        let frames = reporter.poll(start + FLUSH_AFTER);
        assert_eq!(frames.len(), 1);
        assert_eq!(events_of(&frames[0]).len(), 1);

        for i in 0..19 {
            assert!(
                reporter
                    .push("26.1.2", attempt(), start + Duration::from_secs(61 + i))
                    .is_empty()
            );
        }
        let frames = reporter.push("26.1.2", attempt(), start + Duration::from_secs(80));
        assert_eq!(frames.len(), 1);
        assert_eq!(events_of(&frames[0]).len(), 20);
        assert!(reporter.next_deadline().is_none());
    }

    #[test]
    fn session_ends_and_usage_flush_at_once() {
        let now = Instant::now();
        let mut reporter = Reporter::new("2.1.0");
        reporter.push("26.1.2", attempt(), now);
        let session = SessionId::generate();
        let frames = reporter.push(
            "26.2",
            Event::Usage {
                session,
                role: Role::Host,
                turn_rx: 5,
            },
            now,
        );
        assert_eq!(frames.len(), 1);
        assert_eq!(
            serde_json::from_str::<Value>(&frames[0]).unwrap()["mc"],
            "26.2"
        );
        let summary = Event::Session {
            role: Role::Host,
            min: 5,
            guests: 1,
            turn_rx: 0,
            e4mc_rx: 0,
            moves: 0,
            rtt_turn: 0,
            rtt_e4mc: 0,
        };
        let frames = reporter.push("26.1.2", summary, now);
        assert_eq!(frames.len(), 1);
        assert_eq!(events_of(&frames[0]).len(), 2);
    }

    #[test]
    fn offline_drops_events_but_carries_turn_bytes() {
        let now = Instant::now();
        let session = SessionId::generate();
        let mut reporter = Reporter::new("2.1.0");
        reporter.push("26.1.2", attempt(), now);
        assert!(reporter.set_online(false, now).is_empty());
        assert!(
            reporter
                .push(
                    "26.1.2",
                    Event::Usage {
                        session,
                        role: Role::Guest,
                        turn_rx: 100
                    },
                    now
                )
                .is_empty()
        );
        assert!(reporter.push("26.1.2", attempt(), now).is_empty());
        let frames = reporter.set_online(true, now);
        assert_eq!(frames.len(), 1);
        let events = events_of(&frames[0]);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["turnRx"], 100);
        // Nothing else was kept.
        assert!(reporter.next_deadline().is_none());
    }

    #[test]
    fn carried_bytes_join_the_next_usage_event() {
        let now = Instant::now();
        let session = SessionId::generate();
        let mut reporter = Reporter::new("2.1.0");
        reporter.set_online(false, now);
        reporter.push(
            "26.1.2",
            Event::Usage {
                session,
                role: Role::Host,
                turn_rx: 7,
            },
            now,
        );
        reporter.online = true;
        let frames = reporter.push(
            "26.1.2",
            Event::Usage {
                session,
                role: Role::Host,
                turn_rx: 3,
            },
            now,
        );
        assert_eq!(events_of(&frames[0])[0]["turnRx"], 10);
    }

    #[test]
    fn twenty_frames_a_minute() {
        let now = Instant::now();
        let session = SessionId::generate();
        let mut reporter = Reporter::new("2.1.0");
        let mut sent = 0;
        for _ in 0..25 {
            sent += reporter
                .push(
                    "26.1.2",
                    Event::Usage {
                        session,
                        role: Role::Host,
                        turn_rx: 1,
                    },
                    now,
                )
                .len();
        }
        assert_eq!(sent, MAX_FRAMES_PER_MINUTE);
        assert_eq!(
            reporter.next_deadline(),
            Some(now + Duration::from_secs(60))
        );
        let later = reporter.poll(now + Duration::from_secs(60));
        assert_eq!(later.len(), 1);
        assert_eq!(events_of(&later[0]).len(), 5);
    }
}

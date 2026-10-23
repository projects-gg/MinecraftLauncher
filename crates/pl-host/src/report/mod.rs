//! Health telemetry (PROTOCOL.md §12): anonymous events about path attempts, stream moves, session
//! summaries and TURN usage, sent on the presence socket as `world_report` frames.
//!
//! Anonymous by construction: an event holds only the fields its kind lists, codes come from a
//! closed list and numbers are bucketed. No name, address, port, domain, slot, key or credential
//! can be expressed in one. [`batch::Reporter`] groups events into frames (§12.5).

pub mod batch;

use std::borrow::Cow;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};
use serde_json::Value;

use crate::transport::{PathKind, Step};
use crate::tunnel::SessionId;

/// Most events in one frame.
pub const MAX_EVENTS: usize = 20;
/// `ms` buckets.
pub const MS_BUCKETS: [u32; 9] = [0, 50, 100, 200, 400, 800, 1600, 3200, 6400];
/// `min` buckets.
pub const MINUTE_BUCKETS: [u32; 8] = [0, 5, 15, 30, 60, 120, 240, 480];
/// `rttTurn` / `rttE4mc` buckets.
pub const RTT_BUCKETS: [u32; 12] = [0, 10, 20, 40, 60, 80, 100, 150, 200, 300, 500, 1000];

/// Success codes.
pub const SUCCESS_CODES: [&str; 2] = ["ok", "turn_tcp_only"];
/// e4mc codes (besides `e4mc_broker_http_<3-digit status>`).
pub const E4MC_CODES: [&str; 12] = [
    "e4mc_broker_timeout",
    "e4mc_broker_bad",
    "e4mc_quic_timeout",
    "e4mc_quic_refused",
    "e4mc_domain_timeout",
    "e4mc_closed",
    "e4mc_rate_limited",
    "e4mc_unknown_domain",
    "e4mc_dns_failed",
    "e4mc_tcp_refused",
    "e4mc_tcp_timeout",
    "e4mc_protocol_changed",
];
/// TURN codes.
pub const TURN_CODES: [&str; 14] = [
    "turn_denied_budget",
    "turn_denied_day",
    "turn_denied_session",
    "turn_denied_player",
    "turn_denied_full",
    "turn_denied_off",
    "turn_creds_error",
    "turn_alloc_unauthorized",
    "turn_alloc_timeout",
    "turn_alloc_error",
    "turn_permission_failed",
    "turn_peer_timeout",
    "turn_quic_timeout",
    "turn_quic_pin",
];
/// PLJ1 codes.
pub const PLJ_CODES: [&str; 9] = [
    "plj_auth_failed",
    "plj_timeout",
    "plj_full",
    "plj_closed",
    "plj_removed",
    "plj_name",
    "plj_version",
    "plj_busy",
    "plj_error",
];
/// Move outcomes.
pub const MOVE_CODES: [&str; 2] = ["resume_ok", "resume_failed"];

/// Whether `code` is one of §12.2's attempt codes.
pub fn is_attempt_code(code: &str) -> bool {
    SUCCESS_CODES.contains(&code)
        || E4MC_CODES.contains(&code)
        || TURN_CODES.contains(&code)
        || PLJ_CODES.contains(&code)
        || code
            .strip_prefix("e4mc_broker_http_")
            .is_some_and(|s| s.len() == 3 && s.bytes().all(|b| b.is_ascii_digit()))
}

/// Whether `step` belongs to `path` (§12.1).
pub fn step_belongs(path: PathKind, step: Step) -> bool {
    match path {
        PathKind::Turn => {
            matches!(
                step,
                Step::Creds | Step::Alloc | Step::Peer | Step::Quic | Step::Plj
            )
        }
        PathKind::E4mc => {
            matches!(
                step,
                Step::Broker | Step::Quic | Step::Domain | Step::Tcp | Step::Plj
            )
        }
    }
}

/// The largest bucket not above `value`.
pub fn bucket(value: u64, buckets: &[u32]) -> u32 {
    buckets
        .iter()
        .copied()
        .take_while(|b| u64::from(*b) <= value)
        .last()
        .unwrap_or(0)
}

/// A duration in `ms` buckets.
pub fn bucket_ms(elapsed: Duration) -> u32 {
    bucket(
        u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX),
        &MS_BUCKETS,
    )
}

/// A duration in `min` buckets.
pub fn bucket_minutes(elapsed: Duration) -> u32 {
    bucket(elapsed.as_secs() / 60, &MINUTE_BUCKETS)
}

/// A round trip in `rtt…` buckets.
pub fn bucket_rtt(rtt: Duration) -> u32 {
    bucket(
        u64::try_from(rtt.as_millis()).unwrap_or(u64::MAX),
        &RTT_BUCKETS,
    )
}

/// Ping round trips of one path, counted per `rtt…` bucket; the report gives the median.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RttHistogram {
    counts: [u32; RTT_BUCKETS.len()],
}

impl RttHistogram {
    pub fn record(&mut self, rtt: Duration) {
        let value = bucket_rtt(rtt);
        if let Some(i) = RTT_BUCKETS.iter().position(|b| *b == value) {
            self.counts[i] = self.counts[i].saturating_add(1);
        }
    }

    /// How many round trips were recorded.
    pub fn samples(&self) -> u32 {
        self.counts
            .iter()
            .fold(0u32, |sum, c| sum.saturating_add(*c))
    }

    /// The median round trip's bucket; 0 when nothing was recorded.
    pub fn median(&self) -> u32 {
        let total = self.samples();
        if total == 0 {
            return 0;
        }
        let half = total.div_ceil(2);
        let mut seen = 0u32;
        for (count, bucket) in self.counts.iter().zip(RTT_BUCKETS) {
            seen = seen.saturating_add(*count);
            if seen >= half {
                return bucket;
            }
        }
        0
    }

    /// Adds another histogram's counts.
    pub fn merge(&mut self, other: &Self) {
        for (mine, theirs) in self.counts.iter_mut().zip(other.counts) {
            *mine = mine.saturating_add(theirs);
        }
    }
}

/// Round-trip histograms per path kind.
#[derive(Clone, Default)]
pub(crate) struct RttSinks {
    pub turn: Arc<Mutex<RttHistogram>>,
    pub e4mc: Arc<Mutex<RttHistogram>>,
}

impl RttSinks {
    pub fn for_kind(&self, kind: PathKind) -> Arc<Mutex<RttHistogram>> {
        match kind {
            PathKind::Turn => self.turn.clone(),
            PathKind::E4mc => self.e4mc.clone(),
        }
    }

    /// The median round trip of each path, in buckets.
    pub fn medians(&self) -> (u32, u32) {
        let median = |h: &Arc<Mutex<RttHistogram>>| h.lock().expect("rtt lock").median();
        (median(&self.turn), median(&self.e4mc))
    }
}

/// Which side of a session a launcher is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    Host,
    Guest,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Host => "host",
            Self::Guest => "guest",
        }
    }
}

/// Why streams moved (`m.why`): `move.why`'s reasons, and `host` when the host asked for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MoveCause {
    Budget,
    Cap,
    Renew,
    Failure,
    Off,
    /// The guest moved because the host's `move` asked it to.
    Host,
}

impl MoveCause {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Budget => "budget",
            Self::Cap => "cap",
            Self::Renew => "renew",
            Self::Failure => "failure",
            Self::Off => "off",
            Self::Host => "host",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "budget" => Self::Budget,
            "cap" => Self::Cap,
            "renew" => Self::Renew,
            "failure" => Self::Failure,
            "off" => Self::Off,
            "host" => Self::Host,
            _ => return None,
        })
    }
}

impl From<crate::tunnel::control::MoveWhy> for MoveCause {
    fn from(why: crate::tunnel::control::MoveWhy) -> Self {
        use crate::tunnel::control::MoveWhy;
        match why {
            MoveWhy::Budget => Self::Budget,
            MoveWhy::Cap => Self::Cap,
            MoveWhy::Renew => Self::Renew,
            MoveWhy::Failure => Self::Failure,
            MoveWhy::Off => Self::Off,
        }
    }
}

/// One telemetry event. Build them with the constructors, which bucket the numbers and refuse
/// anything the service would drop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// `a`: one per path attempt.
    Attempt {
        path: PathKind,
        role: Role,
        step: Step,
        code: Cow<'static, str>,
        ms: u32,
        /// The e4mc relay's id (`eu`), host side.
        region: Option<String>,
    },
    /// `m`: one per stream move.
    Move {
        from: PathKind,
        to: PathKind,
        why: MoveCause,
        ok: bool,
        ms: u32,
    },
    /// `s`: when the launcher's part of a session ends.
    Session {
        role: Role,
        min: u32,
        /// Most guests at once.
        guests: u32,
        turn_rx: u64,
        e4mc_rx: u64,
        moves: u32,
        rtt_turn: u32,
        rtt_e4mc: u32,
    },
    /// `u`: TURN bytes since the previous one.
    Usage {
        session: SessionId,
        role: Role,
        turn_rx: u64,
    },
}

impl Event {
    /// An attempt event; `None` when the step does not belong to the path or the code is not one
    /// of §12.2's (such an event would be dropped, and the code might not be fit for a log).
    pub fn attempt(
        path: PathKind,
        role: Role,
        step: Step,
        code: impl Into<Cow<'static, str>>,
        elapsed: Duration,
        region: Option<&str>,
    ) -> Option<Self> {
        let event = Self::Attempt {
            path,
            role,
            step,
            code: code.into(),
            ms: bucket_ms(elapsed),
            region: region.map(str::to_owned),
        };
        event.is_valid().then_some(event)
    }

    /// A move event.
    pub fn moved(
        from: PathKind,
        to: PathKind,
        why: MoveCause,
        ok: bool,
        elapsed: Duration,
    ) -> Self {
        Self::Move {
            from,
            to,
            why,
            ok,
            ms: bucket_ms(elapsed),
        }
    }

    /// Whether the event passes the service's checks (§12.1).
    pub fn is_valid(&self) -> bool {
        match self {
            Self::Attempt {
                path,
                step,
                code,
                ms,
                region,
                ..
            } => {
                step_belongs(*path, *step)
                    && is_attempt_code(code)
                    && MS_BUCKETS.contains(ms)
                    && region.as_deref().is_none_or(|r| {
                        *path == PathKind::E4mc
                            && (1..=16).contains(&r.len())
                            && r.bytes()
                                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                    })
            }
            Self::Move { ms, .. } => MS_BUCKETS.contains(ms),
            Self::Session {
                min,
                rtt_turn,
                rtt_e4mc,
                ..
            } => {
                MINUTE_BUCKETS.contains(min)
                    && RTT_BUCKETS.contains(rtt_turn)
                    && RTT_BUCKETS.contains(rtt_e4mc)
            }
            Self::Usage { .. } => true,
        }
    }

    /// The event's JSON (tests and diagnostics; frames serialize the events directly).
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).expect("events serialize")
    }

    /// Reads an event the way the service checks it: `None` when it would be dropped (unknown kind,
    /// a field outside its list or missing, a step that does not belong to its path, a code
    /// outside §12.2, an unbucketed number).
    pub fn from_value(value: &Value) -> Option<Self> {
        let object = value.as_object()?;
        let fields = |allowed: &[&str], optional: &[&str]| {
            object
                .keys()
                .all(|k| allowed.contains(&k.as_str()) || optional.contains(&k.as_str()))
                && allowed.iter().all(|k| object.contains_key(*k))
        };
        let text = |k: &str| object.get(k).and_then(Value::as_str);
        let number = |k: &str| object.get(k).and_then(Value::as_u64);
        let small = |k: &str| number(k).and_then(|n| u32::try_from(n).ok());
        let path = |k: &str| match text(k)? {
            "turn" => Some(PathKind::Turn),
            "e4mc" => Some(PathKind::E4mc),
            _ => None,
        };
        let role = || match text("role")? {
            "host" => Some(Role::Host),
            "guest" => Some(Role::Guest),
            _ => None,
        };
        let event = match text("k")? {
            "a" => {
                if !fields(&["k", "path", "role", "step", "code", "ms"], &["region"]) {
                    return None;
                }
                let step = parse_step(text("step")?)?;
                let code = text("code")?;
                if !is_attempt_code(code) {
                    return None;
                }
                let region = match object.get("region") {
                    Some(r) => Some(r.as_str()?.to_owned()),
                    None => None,
                };
                Self::Attempt {
                    path: path("path")?,
                    role: role()?,
                    step,
                    code: Cow::Owned(code.to_owned()),
                    ms: small("ms")?,
                    region,
                }
            }
            "m" => {
                if !fields(&["k", "from", "to", "why", "code", "ms"], &[]) {
                    return None;
                }
                let ok = match text("code")? {
                    "resume_ok" => true,
                    "resume_failed" => false,
                    _ => return None,
                };
                Self::Move {
                    from: path("from")?,
                    to: path("to")?,
                    why: MoveCause::parse(text("why")?)?,
                    ok,
                    ms: small("ms")?,
                }
            }
            "s" => {
                let list = [
                    "k", "role", "min", "guests", "turnRx", "e4mcRx", "moves", "rttTurn", "rttE4mc",
                ];
                if !fields(&list, &[]) {
                    return None;
                }
                Self::Session {
                    role: role()?,
                    min: small("min")?,
                    guests: small("guests")?,
                    turn_rx: number("turnRx")?,
                    e4mc_rx: number("e4mcRx")?,
                    moves: small("moves")?,
                    rtt_turn: small("rttTurn")?,
                    rtt_e4mc: small("rttE4mc")?,
                }
            }
            "u" => {
                if !fields(&["k", "session", "role", "turnRx"], &[]) {
                    return None;
                }
                Self::Usage {
                    session: SessionId::parse(text("session")?)?,
                    role: role()?,
                    turn_rx: number("turnRx")?,
                }
            }
            _ => return None,
        };
        event.is_valid().then_some(event)
    }
}

impl Serialize for Event {
    /// Fields in protocol order (§12.1).
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Attempt {
                path,
                role,
                step,
                code,
                ms,
                region,
            } => {
                let mut map = serializer.serialize_map(Some(6 + usize::from(region.is_some())))?;
                map.serialize_entry("k", "a")?;
                map.serialize_entry("path", path.as_str())?;
                map.serialize_entry("role", role.as_str())?;
                map.serialize_entry("step", step.as_str())?;
                map.serialize_entry("code", code.as_ref())?;
                map.serialize_entry("ms", ms)?;
                if let Some(region) = region {
                    map.serialize_entry("region", region)?;
                }
                map.end()
            }
            Self::Move {
                from,
                to,
                why,
                ok,
                ms,
            } => {
                let mut map = serializer.serialize_map(Some(6))?;
                map.serialize_entry("k", "m")?;
                map.serialize_entry("from", from.as_str())?;
                map.serialize_entry("to", to.as_str())?;
                map.serialize_entry("why", why.as_str())?;
                map.serialize_entry("code", if *ok { "resume_ok" } else { "resume_failed" })?;
                map.serialize_entry("ms", ms)?;
                map.end()
            }
            Self::Session {
                role,
                min,
                guests,
                turn_rx,
                e4mc_rx,
                moves,
                rtt_turn,
                rtt_e4mc,
            } => {
                let mut map = serializer.serialize_map(Some(9))?;
                map.serialize_entry("k", "s")?;
                map.serialize_entry("role", role.as_str())?;
                map.serialize_entry("min", min)?;
                map.serialize_entry("guests", guests)?;
                map.serialize_entry("turnRx", turn_rx)?;
                map.serialize_entry("e4mcRx", e4mc_rx)?;
                map.serialize_entry("moves", moves)?;
                map.serialize_entry("rttTurn", rtt_turn)?;
                map.serialize_entry("rttE4mc", rtt_e4mc)?;
                map.end()
            }
            Self::Usage {
                session,
                role,
                turn_rx,
            } => {
                let mut map = serializer.serialize_map(Some(4))?;
                map.serialize_entry("k", "u")?;
                map.serialize_entry("session", session)?;
                map.serialize_entry("role", role.as_str())?;
                map.serialize_entry("turnRx", turn_rx)?;
                map.end()
            }
        }
    }
}

fn parse_step(text: &str) -> Option<Step> {
    Some(match text {
        "creds" => Step::Creds,
        "alloc" => Step::Alloc,
        "peer" => Step::Peer,
        "broker" => Step::Broker,
        "quic" => Step::Quic,
        "domain" => Step::Domain,
        "tcp" => Step::Tcp,
        "plj" => Step::Plj,
        _ => return None,
    })
}

#[derive(Serialize)]
struct FrameOut<'a> {
    t: &'static str,
    mc: &'a str,
    lv: &'a str,
    r: &'a [Event],
}

/// A `world_report` frame as the presence socket sends it (compact JSON, fields in protocol order).
pub fn frame(mc: &str, launcher_version: &str, events: &[Event]) -> String {
    serde_json::to_string(&FrameOut {
        t: "world_report",
        mc,
        lv: launcher_version,
        r: events,
    })
    .expect("frames serialize")
}

/// A frame the service drops whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FrameError {
    #[error("not a world_report frame")]
    NotReport,
    #[error("a frame carries 1 to {MAX_EVENTS} events")]
    EventCount,
}

/// Reads a frame the way the service does: the events it keeps.
pub fn check_frame(value: &Value) -> Result<Vec<Event>, FrameError> {
    let object = value.as_object().ok_or(FrameError::NotReport)?;
    let short_text = |k: &str| {
        object
            .get(k)
            .and_then(Value::as_str)
            .is_some_and(|s| (1..=32).contains(&s.len()))
    };
    if object.get("t").and_then(Value::as_str) != Some("world_report")
        || !short_text("mc")
        || !short_text("lv")
    {
        return Err(FrameError::NotReport);
    }
    let events = object
        .get("r")
        .and_then(Value::as_array)
        .ok_or(FrameError::NotReport)?;
    if events.is_empty() || events.len() > MAX_EVENTS {
        return Err(FrameError::EventCount);
    }
    Ok(events.iter().filter_map(Event::from_value).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdata;

    #[test]
    fn vector_frames() {
        let file = testdata::json("report/world-report.json");
        for frame_value in file["valid"].as_array().unwrap() {
            let events = check_frame(frame_value).unwrap();
            assert_eq!(events.len(), frame_value["r"].as_array().unwrap().len());
            // What this launcher sends for them is the same JSON.
            let again = frame(
                frame_value["mc"].as_str().unwrap(),
                frame_value["lv"].as_str().unwrap(),
                &events,
            );
            assert_eq!(&serde_json::from_str::<Value>(&again).unwrap(), frame_value);
            assert!(
                again.starts_with(r#"{"t":"world_report","mc":""#),
                "{again}"
            );
        }
        for case in file["invalid"].as_array().unwrap() {
            let name = case["case"].as_str().unwrap();
            let result = check_frame(&case["frame"]);
            match case["expect"].as_str().unwrap() {
                "drop the frame" => assert!(result.is_err(), "{name}"),
                _ => assert_eq!(result, Ok(vec![]), "{name}"),
            }
        }
    }

    #[test]
    fn rtt_median() {
        let mut rtt = RttHistogram::default();
        assert_eq!(rtt.median(), 0);
        for ms in [12, 45, 47, 300, 41] {
            rtt.record(Duration::from_millis(ms));
        }
        assert_eq!(rtt.samples(), 5);
        assert_eq!(rtt.median(), 40);
    }

    #[test]
    fn buckets() {
        assert_eq!(bucket_ms(Duration::from_millis(187)), 100);
        assert_eq!(bucket_ms(Duration::from_millis(200)), 200);
        assert_eq!(bucket_ms(Duration::from_secs(60)), 6400);
        assert_eq!(bucket_minutes(Duration::from_secs(59 * 60)), 30);
        assert_eq!(bucket_rtt(Duration::from_millis(45)), 40);
        assert_eq!(bucket_rtt(Duration::from_secs(3)), 1000);
    }

    #[test]
    fn constructors_refuse_what_the_service_drops() {
        let ms = Duration::from_millis(250);
        assert!(Event::attempt(PathKind::E4mc, Role::Guest, Step::Alloc, "ok", ms, None).is_none());
        assert!(
            Event::attempt(
                PathKind::E4mc,
                Role::Guest,
                Step::Tcp,
                "able-acid.eu.e4mc.link",
                ms,
                None
            )
            .is_none()
        );
        assert!(
            Event::attempt(
                PathKind::Turn,
                Role::Host,
                Step::Alloc,
                "ok",
                ms,
                Some("eu")
            )
            .is_none()
        );
        let broker = Event::attempt(
            PathKind::E4mc,
            Role::Host,
            Step::Broker,
            "e4mc_broker_http_503",
            ms,
            None,
        )
        .unwrap();
        assert_eq!(broker.to_value()["ms"], 200);
        assert!(
            Event::attempt(
                PathKind::E4mc,
                Role::Host,
                Step::Broker,
                "e4mc_broker_http_50x",
                ms,
                None
            )
            .is_none()
        );
    }
}

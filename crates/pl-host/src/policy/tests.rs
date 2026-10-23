//! The path policy scenarios (`testdata/policy/guest-connect.json`, `host-open.json`) run against
//! scripted environments: each step of a path does what the scenario says.

use std::borrow::Cow;
use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::Policy;
use super::env::{
    Creds, CredsAnswer, CredsError, CredsReason, E4mcOpened, GuestPaths, HostPaths, TurnAllocation,
};
use super::guest::{Ctx, JoinFailure, join};
use crate::host::gate::GateKey;
use crate::host::{GameEndpoint, HostConfig, HostEnv, HostEvent, HostSession};
use crate::manifest::{Loader, Manifest};
use crate::report::{Event, RttSinks};
use crate::testdata;
use crate::transport::{
    BoxFuture, BoxStream, GuestPath, HostPath, PathAddress, PathError, PathKind, Step,
};
use crate::tunnel::channel::{Channel, In, Keepalive};
use crate::tunnel::control::{Accept, Control, Hello, NameVerdict, Refusal};
use crate::tunnel::noise::{OPENING_LEN, parse_opening, respond};
use crate::tunnel::{GuestId, HostKey, Join, Paths, Psk, SessionId, Slot, StreamId, TurnAddress};

fn kind(text: &str) -> PathKind {
    match text {
        "turn" => PathKind::Turn,
        "e4mc" => PathKind::E4mc,
        other => panic!("unknown path {other}"),
    }
}

fn step_name(step: Step) -> &'static str {
    step.as_str()
}

/// `script`: per path, `step:outcome` in order.
fn outcomes(scenario: &Value) -> HashMap<PathKind, Vec<(String, String)>> {
    let mut out = HashMap::new();
    for (path, steps) in scenario["script"].as_object().unwrap() {
        let list = steps
            .as_array()
            .unwrap()
            .iter()
            .map(|s| {
                let (step, outcome) = s.as_str().unwrap().split_once(':').unwrap();
                (step.to_owned(), outcome.to_owned())
            })
            .collect();
        out.insert(kind(path), list);
    }
    out
}

fn outcome(
    script: &HashMap<PathKind, Vec<(String, String)>>,
    path: PathKind,
    step: &str,
) -> String {
    script
        .get(&path)
        .and_then(|steps| steps.iter().find(|(s, _)| s == step))
        .map(|(_, o)| o.clone())
        .unwrap_or_else(|| panic!("the scenario has no {}:{step}", path.as_str()))
}

fn events_of(events: &[Event]) -> Vec<Vec<String>> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::Attempt {
                path, step, code, ..
            } => Some(vec![
                path.as_str().to_owned(),
                step_name(*step).to_owned(),
                code.to_string(),
            ]),
            _ => None,
        })
        .collect()
}

fn expected_events(expect: &Value) -> Vec<Vec<String>> {
    expect["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            e.as_array()
                .unwrap()
                .iter()
                .map(|s| s.as_str().unwrap().to_owned())
                .collect()
        })
        .collect()
}

fn strings(value: &Value) -> Vec<String> {
    value.as_array().map_or_else(Vec::new, |a| {
        a.iter().map(|s| s.as_str().unwrap().to_owned()).collect()
    })
}

/// The host the scripted guest paths reach: real PLJ1, answers from the script.
struct MiniHost {
    key: HostKey,
    psk: Psk,
    slot: Slot,
    session: SessionId,
    kept: Mutex<Vec<Channel>>,
}

impl MiniHost {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            key: HostKey::generate(),
            psk: Psk::generate(),
            slot: Slot::generate(),
            session: SessionId::generate(),
            kept: Mutex::new(Vec::new()),
        })
    }

    fn join(&self, paths: Paths) -> Join {
        Join {
            session: self.session,
            key: *self.key.public(),
            slot: self.slot,
            psk: self.psk.clone(),
            mc: "26.1.2".into(),
            proto: 9999,
            pack: Some("projects".into()),
            paths,
        }
    }

    /// Answers one stream the way `plj` says.
    async fn serve(self: Arc<Self>, mut stream: tokio::io::DuplexStream, plj: String) {
        let mut opening = [0u8; OPENING_LEN];
        if stream.read_exact(&mut opening).await.is_err() {
            return;
        }
        match plj.as_str() {
            "plj_auth_failed" => (),
            "e4mc_rate_limited" | "e4mc_unknown_domain" => {
                // A relay's refusal packet: id 0x00 and a string.
                let text = if plj == "e4mc_rate_limited" {
                    "too fast"
                } else {
                    "unknown"
                };
                let mut packet = vec![0x00, text.len() as u8];
                packet.extend_from_slice(text.as_bytes());
                let mut out = vec![packet.len() as u8];
                out.extend_from_slice(&packet);
                let _ = stream.write_all(&out).await;
            }
            _ => {
                let (_, msg1) = parse_opening(&opening).unwrap();
                let (cipher, reply) =
                    respond(&self.key, &self.psk, &self.slot, &self.session, &msg1).unwrap();
                stream.write_all(&reply).await.unwrap();
                let policy = Policy::default();
                let mut channel = Channel::spawn(
                    Box::new(stream),
                    cipher,
                    Keepalive::from_policy(&policy),
                    None,
                );
                let Some(In::Control(Ok(Control::Hello(Hello::Control { .. })))) =
                    channel.recv().await
                else {
                    panic!("expected a control hello");
                };
                let answer = match plj.strip_prefix("no:") {
                    Some(reason) => Control::No(Refusal::from_wire(reason)),
                    None => Control::Ok(Accept::control(
                        GuestId::new(1).unwrap(),
                        StreamId::generate(),
                        NameVerdict::Ok,
                    )),
                };
                channel.send(answer).await;
                self.kept.lock().unwrap().push(channel);
            }
        }
    }
}

struct ScriptedPath {
    kind: PathKind,
    plj: String,
    host: Arc<MiniHost>,
}

impl GuestPath for ScriptedPath {
    fn kind(&self) -> PathKind {
        self.kind
    }

    fn open(&self) -> BoxFuture<'_, Result<BoxStream, PathError>> {
        let (guest, host) = tokio::io::duplex(1 << 16);
        tokio::spawn(self.host.clone().serve(host, self.plj.clone()));
        Box::pin(async move { Ok(Box::new(guest) as BoxStream) })
    }

    fn explain_reply(&self, _packet: &[u8]) -> Option<Cow<'static, str>> {
        self.plj
            .starts_with("e4mc_")
            .then(|| Cow::Owned(self.plj.clone()))
    }

    fn closed(&self) -> BoxFuture<'_, PathError> {
        Box::pin(std::future::pending())
    }

    fn rx_bytes(&self) -> u64 {
        0
    }

    fn close(&self) -> BoxFuture<'_, ()> {
        Box::pin(async {})
    }
}

struct ScriptedAllocation {
    script: HashMap<PathKind, Vec<(String, String)>>,
    host: Arc<MiniHost>,
}

impl TurnAllocation for ScriptedAllocation {
    fn relay(&self) -> SocketAddrV4 {
        SocketAddrV4::new(Ipv4Addr::new(162, 159, 207, 11), 40_001)
    }

    fn connect(
        &self,
        _host_relay: SocketAddrV4,
        _pin: [u8; 32],
    ) -> BoxFuture<'_, Result<Arc<dyn GuestPath>, PathError>> {
        Box::pin(async move {
            match outcome(&self.script, PathKind::Turn, "quic").as_str() {
                "ok" => Ok(Arc::new(ScriptedPath {
                    kind: PathKind::Turn,
                    plj: outcome(&self.script, PathKind::Turn, "plj"),
                    host: self.host.clone(),
                }) as Arc<dyn GuestPath>),
                code => Err(PathError {
                    step: Step::Quic,
                    code: Cow::Owned(code.to_owned()),
                }),
            }
        })
    }
}

fn host_paths() -> Paths {
    Paths {
        turn: Some(TurnAddress {
            relay: SocketAddrV4::new(Ipv4Addr::new(162, 159, 207, 10), 51_234),
            pin: [7; 32],
        }),
        e4mc: Some("able-acid.eu.e4mc.link".into()),
    }
}

struct ScriptedGuest {
    order: Vec<PathKind>,
    ice: bool,
    script: HashMap<PathKind, Vec<(String, String)>>,
    host: Arc<MiniHost>,
    messages: Mutex<Vec<String>>,
    creds: AtomicU64,
}

impl ScriptedGuest {
    fn creds(&self) -> Creds {
        Creds {
            id: self.creds.fetch_add(1, Ordering::Relaxed),
            ttl: Duration::from_secs(7_200),
        }
    }

    fn say(&self, message: &str) {
        self.messages.lock().unwrap().push(message.to_owned());
    }
}

impl GuestPaths for ScriptedGuest {
    fn turn_creds(&self, reason: CredsReason) -> BoxFuture<'_, Result<CredsAnswer, CredsError>> {
        Box::pin(async move {
            self.say(&format!("turn_creds/{}", reason.as_str()));
            match reason {
                CredsReason::Primary => Ok(CredsAnswer {
                    order: self.order.clone(),
                    creds: self.ice.then(|| self.creds()),
                }),
                _ => match outcome(&self.script, PathKind::Turn, "creds").as_str() {
                    "ok" => Ok(CredsAnswer {
                        order: vec![PathKind::Turn],
                        creds: Some(self.creds()),
                    }),
                    code => Err(CredsError::Denied(
                        code.strip_prefix("turn_denied_").unwrap_or(code).to_owned(),
                    )),
                },
            }
        })
    }

    fn turn_release(&self, _creds: Creds) {
        self.say("turn_release");
    }

    fn world_peer(
        &self,
        path: PathKind,
        _relay: Option<SocketAddrV4>,
    ) -> BoxFuture<'_, Option<Paths>> {
        Box::pin(async move {
            self.say(&format!("world_peer/{}", path.as_str()));
            (outcome(&self.script, path, "peer") == "ok").then(host_paths)
        })
    }

    fn turn_allocate(
        &self,
        _creds: Creds,
    ) -> BoxFuture<'_, Result<Arc<dyn TurnAllocation>, PathError>> {
        Box::pin(async move {
            match outcome(&self.script, PathKind::Turn, "alloc").as_str() {
                "ok" => Ok(Arc::new(ScriptedAllocation {
                    script: self.script.clone(),
                    host: self.host.clone(),
                }) as Arc<dyn TurnAllocation>),
                code => Err(PathError {
                    step: Step::Alloc,
                    code: Cow::Owned(code.to_owned()),
                }),
            }
        })
    }

    fn e4mc_connect(
        &self,
        _domain: &str,
        _proto: i32,
    ) -> BoxFuture<'_, Result<Arc<dyn GuestPath>, PathError>> {
        Box::pin(async move {
            match outcome(&self.script, PathKind::E4mc, "tcp").as_str() {
                "ok" => Ok(Arc::new(ScriptedPath {
                    kind: PathKind::E4mc,
                    plj: outcome(&self.script, PathKind::E4mc, "plj"),
                    host: self.host.clone(),
                }) as Arc<dyn GuestPath>),
                code => Err(PathError {
                    step: Step::Tcp,
                    code: Cow::Owned(code.to_owned()),
                }),
            }
        })
    }
}

#[tokio::test(start_paused = true)]
async fn guest_connect_scenarios() {
    let file = testdata::json("policy/guest-connect.json");
    let scenarios = file["scenarios"].as_array().unwrap();
    assert_eq!(scenarios.len(), 9);
    for scenario in scenarios {
        let case = scenario["case"].as_str().unwrap();
        let host = MiniHost::new();
        let mut invite_paths = Paths::default();
        for path in strings(&scenario["invitePaths"]) {
            if let Some(address) = host_paths().address(kind(&path)) {
                invite_paths.set(address);
            }
        }
        let env = ScriptedGuest {
            order: strings(&scenario["order"])
                .iter()
                .map(|s| kind(s))
                .collect(),
            ice: scenario["ice"].as_bool().unwrap(),
            script: outcomes(scenario),
            host: host.clone(),
            messages: Mutex::new(Vec::new()),
            creds: AtomicU64::new(1),
        };
        let join_object = host.join(invite_paths.clone());
        let policy = Policy::default();
        let rtt = RttSinks::default();
        let ctx = Ctx {
            env: &env,
            join: &join_object,
            policy: &policy,
            rtt: &rtt,
        };
        let events = Mutex::new(Vec::new());
        let report = |event| events.lock().unwrap().push(event);
        let hello = Hello::Control {
            name: "Notch".into(),
            premium: false,
            launcher: "2.1.0".into(),
        };
        let mut paths = invite_paths;
        let result = join(&ctx, hello, &mut paths, &report).await;
        let expect = &scenario["expect"];
        let got = match &result {
            Ok(joined) => format!("connected:{}", joined.route.kind.as_str()),
            Err(JoinFailure::NoPath) => "failed".to_owned(),
            Err(JoinFailure::Refused(reason)) => format!("refused:{}", reason.as_str()),
        };
        assert_eq!(got, expect["result"].as_str().unwrap(), "{case}");
        assert_eq!(
            *env.messages.lock().unwrap(),
            strings(&expect["messages"]),
            "{case}"
        );
        assert_eq!(
            events_of(&events.lock().unwrap()),
            expected_events(expect),
            "{case}"
        );
    }
}

// ------------------------------------------------------------------------------------------------
// Host side

struct ScriptedHost {
    order: Vec<PathKind>,
    ice: bool,
    script: HashMap<PathKind, Vec<(String, String)>>,
    messages: Mutex<Vec<String>>,
    events: Mutex<Vec<Event>>,
    creds: AtomicU64,
}

struct NullHostPath(PathKind);

impl HostPath for NullHostPath {
    fn kind(&self) -> PathKind {
        self.0
    }

    fn address(&self) -> PathAddress {
        match self.0 {
            PathKind::Turn => PathAddress::Turn {
                relay: SocketAddrV4::new(Ipv4Addr::new(162, 159, 207, 10), 51_234),
                pin: [7; 32],
            },
            PathKind::E4mc => PathAddress::E4mc {
                domain: "able-acid.eu.e4mc.link".into(),
            },
        }
    }

    fn admit_peer(&self, _relay: SocketAddrV4) -> BoxFuture<'_, Result<(), PathError>> {
        Box::pin(async { Ok(()) })
    }

    fn forget_peer(&self, _relay: SocketAddrV4) {}

    fn accept(&self) -> BoxFuture<'_, Result<BoxStream, PathError>> {
        Box::pin(std::future::pending())
    }

    fn closed(&self) -> BoxFuture<'_, PathError> {
        Box::pin(std::future::pending())
    }

    fn rx_bytes(&self) -> u64 {
        0
    }

    fn close(&self) -> BoxFuture<'_, ()> {
        Box::pin(async {})
    }
}

impl HostPaths for ScriptedHost {
    fn turn_creds(&self, reason: CredsReason) -> BoxFuture<'_, Result<CredsAnswer, CredsError>> {
        Box::pin(async move {
            let creds = Creds {
                id: self.creds.fetch_add(1, Ordering::Relaxed),
                ttl: Duration::from_secs(7_200),
            };
            match reason {
                CredsReason::Primary => Ok(CredsAnswer {
                    order: self.order.clone(),
                    creds: self.ice.then_some(creds),
                }),
                _ => {
                    self.messages
                        .lock()
                        .unwrap()
                        .push(format!("turn_creds/{}", reason.as_str()));
                    match outcome(&self.script, PathKind::Turn, "creds").as_str() {
                        "ok" => Ok(CredsAnswer {
                            order: vec![PathKind::Turn],
                            creds: Some(creds),
                        }),
                        code => Err(CredsError::Denied(
                            code.strip_prefix("turn_denied_").unwrap_or(code).to_owned(),
                        )),
                    }
                }
            }
        })
    }

    fn turn_release(&self, _creds: Creds) {
        self.messages.lock().unwrap().push("turn_release".into());
    }

    fn turn_open(&self, _creds: Creds) -> BoxFuture<'_, Result<Arc<dyn HostPath>, PathError>> {
        Box::pin(async move {
            match outcome(&self.script, PathKind::Turn, "alloc").as_str() {
                "ok" => Ok(Arc::new(NullHostPath(PathKind::Turn)) as Arc<dyn HostPath>),
                code => Err(PathError {
                    step: Step::Alloc,
                    code: Cow::Owned(code.to_owned()),
                }),
            }
        })
    }

    fn e4mc_open(&self) -> BoxFuture<'_, E4mcOpened> {
        Box::pin(async move {
            let steps = self
                .script
                .get(&PathKind::E4mc)
                .cloned()
                .unwrap_or_default();
            let mut broker_failure = None;
            for (step, result) in steps {
                if result == "ok" {
                    continue;
                }
                if step == "broker" {
                    broker_failure = Some(Cow::Owned(result));
                    continue;
                }
                let step = if step == "quic" {
                    Step::Quic
                } else {
                    Step::Domain
                };
                return E4mcOpened {
                    result: Err(PathError {
                        step,
                        code: Cow::Owned(result),
                    }),
                    region: None,
                    broker_failure,
                };
            }
            E4mcOpened {
                result: Ok(Arc::new(NullHostPath(PathKind::E4mc)) as Arc<dyn HostPath>),
                region: Some("eu".into()),
                broker_failure,
            }
        })
    }
}

impl HostEnv for ScriptedHost {
    fn event(&self, event: HostEvent) {
        match event {
            HostEvent::Report(event) => self.events.lock().unwrap().push(event),
            HostEvent::Closed => self
                .messages
                .lock()
                .unwrap()
                .push("world/open:false".into()),
            HostEvent::StopGame => self.messages.lock().unwrap().push("host-stop".into()),
            // Step 6's announcement follows a ready path; the scenarios list path preparation.
            _ => {}
        }
    }
}

pub(crate) fn test_manifest() -> Manifest {
    Manifest::new(
        "26.1.2",
        Loader {
            kind: "fabric".into(),
            version: "0.19.5".into(),
        },
        None,
        Vec::new(),
        Vec::new(),
        0,
    )
}

pub(crate) fn host_config(world_dir: &std::path::Path, port: u16) -> HostConfig {
    HostConfig {
        session: SessionId::generate(),
        key: HostKey::generate(),
        max_guests: 7,
        host_name: "Ayse".into(),
        mc: "26.1.2".into(),
        pack: Some("projects".into()),
        manifest: test_manifest(),
        game: GameEndpoint {
            port,
            proto: 9999,
            publish_id: "p1".into(),
            gate: GateKey::from_bridge_token("WTFgKcgDIYOrciBNaoRi9vSxyjWBCjU9sg1HQiqdK-8"),
        },
        world_dir: world_dir.to_path_buf(),
        binding: crate::host::binding::BindingKey::from_bytes([3; 32]),
        policy: Policy::default(),
    }
}

#[tokio::test(start_paused = true)]
async fn host_open_scenarios() {
    let file = testdata::json("policy/host-open.json");
    let scenarios = file["scenarios"].as_array().unwrap();
    assert_eq!(scenarios.len(), 5);
    let dir = tempfile::tempdir().unwrap();
    for scenario in scenarios {
        let case = scenario["case"].as_str().unwrap();
        let env = Arc::new(ScriptedHost {
            order: strings(&scenario["order"])
                .iter()
                .map(|s| kind(s))
                .collect(),
            ice: scenario["ice"].as_bool().unwrap(),
            script: outcomes(scenario),
            messages: Mutex::new(Vec::new()),
            events: Mutex::new(Vec::new()),
            creds: AtomicU64::new(1),
        });
        let opened = HostSession::open(host_config(dir.path(), 1), env.clone()).await;
        let expect = &scenario["expect"];
        let paths: Vec<String> = match &opened {
            Ok(session) => session
                .paths()
                .kinds()
                .iter()
                .map(|k| k.as_str().to_owned())
                .collect(),
            Err(_) => Vec::new(),
        };
        assert_eq!(paths, strings(&expect["paths"]), "{case}");
        assert_eq!(
            *env.messages.lock().unwrap(),
            strings(&expect["messages"]),
            "{case}"
        );
        assert_eq!(
            events_of(&env.events.lock().unwrap()),
            expected_events(expect),
            "{case}"
        );
        if let Ok(session) = opened {
            let joined = session.invite("234567890123456789").await.unwrap();
            assert_eq!(
                joined.paths,
                session.paths(),
                "{case}: invites carry the ready path"
            );
            session.close().await;
        }
    }
}

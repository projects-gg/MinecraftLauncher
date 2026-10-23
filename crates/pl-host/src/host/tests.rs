//! A host and guests over in-memory paths: joining, admission, the PLG1 header, relaying, and
//! moving streams between paths without losing or repeating a byte.

use std::borrow::Cow;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};
use tokio::sync::mpsc;
use tokio::time::timeout;

use super::gate::{GateKey, Identity, verify};
use super::session::memory_world;
use super::{GuestStatus, HostConfig, HostEnv, HostEvent, HostSession};
use crate::guest::local::{GameHandshake, parse_handshake};
use crate::guest::{GuestConfig, GuestEnd, GuestEnv, GuestEvent, GuestSession};
use crate::policy::Policy;
use crate::policy::env::{
    Creds, CredsAnswer, CredsError, CredsReason, E4mcOpened, GuestPaths, HostPaths, TurnAllocation,
};
use crate::policy::fake::{FakeAllocation, FakeNet};
use crate::policy::guest::JoinFailure;
use crate::report::Event;
use crate::testdata;
use crate::transport::{BoxFuture, GuestPath, HostPath, PathAddress, PathError, PathKind, Step};
use crate::tunnel::control::{NameVerdict, Refusal};
use crate::tunnel::{Join, Paths, Slot};

const FRIEND_A: &str = "123456789012345678";
const FRIEND_B: &str = "234567890123456789";
const TOKEN: &str = "WTFgKcgDIYOrciBNaoRi9vSxyjWBCjU9sg1HQiqdK-8";

/// The launcher's side of the host, recording what the session asks for.
struct HostSide {
    net: Arc<FakeNet>,
    order: Mutex<Vec<PathKind>>,
    turn_ok: AtomicBool,
    ttl: Duration,
    creds: AtomicU64,
    log: Mutex<Vec<String>>,
    events: mpsc::UnboundedSender<HostEvent>,
}

impl HostSide {
    fn say(&self, line: impl Into<String>) {
        self.log.lock().unwrap().push(line.into());
    }

    fn creds(&self) -> Creds {
        Creds {
            id: self.creds.fetch_add(1, Ordering::Relaxed),
            ttl: self.ttl,
        }
    }
}

impl HostPaths for HostSide {
    fn turn_creds(&self, reason: CredsReason) -> BoxFuture<'_, Result<CredsAnswer, CredsError>> {
        Box::pin(async move {
            self.say(format!("turn_creds/{}", reason.as_str()));
            let turn_ok = self.turn_ok.load(Ordering::SeqCst);
            match reason {
                CredsReason::Primary => {
                    let order: Vec<PathKind> = self
                        .order
                        .lock()
                        .unwrap()
                        .iter()
                        .copied()
                        .filter(|k| turn_ok || *k != PathKind::Turn)
                        .collect();
                    let creds = (order.first() == Some(&PathKind::Turn)).then(|| self.creds());
                    Ok(CredsAnswer { order, creds })
                }
                _ if turn_ok => Ok(CredsAnswer {
                    order: vec![PathKind::Turn],
                    creds: Some(self.creds()),
                }),
                _ => Err(CredsError::Denied("budget".into())),
            }
        })
    }

    fn turn_release(&self, creds: Creds) {
        self.say(format!("turn_release/{}", creds.id));
    }

    fn turn_open(&self, _creds: Creds) -> BoxFuture<'_, Result<Arc<dyn HostPath>, PathError>> {
        Box::pin(async move { Ok(self.net.host_path(PathKind::Turn) as Arc<dyn HostPath>) })
    }

    fn e4mc_open(&self) -> BoxFuture<'_, E4mcOpened> {
        Box::pin(async move {
            E4mcOpened {
                result: Ok(self.net.host_path(PathKind::E4mc) as Arc<dyn HostPath>),
                region: Some("eu".into()),
                broker_failure: None,
            }
        })
    }
}

impl HostEnv for HostSide {
    fn event(&self, event: HostEvent) {
        let _ = self.events.send(event);
    }
}

/// The launcher's side of a guest; its `world_peer` goes straight to the host (the service's job).
struct GuestSide {
    net: Arc<FakeNet>,
    host: HostSession,
    slot: Slot,
    order: Mutex<Vec<PathKind>>,
    creds: AtomicU64,
    relays: AtomicU32,
    log: Mutex<Vec<String>>,
    events: mpsc::UnboundedSender<GuestEvent>,
}

impl GuestSide {
    fn say(&self, line: impl Into<String>) {
        self.log.lock().unwrap().push(line.into());
    }
}

impl GuestPaths for GuestSide {
    fn turn_creds(&self, reason: CredsReason) -> BoxFuture<'_, Result<CredsAnswer, CredsError>> {
        Box::pin(async move {
            self.say(format!("turn_creds/{}", reason.as_str()));
            let order = self.order.lock().unwrap().clone();
            let creds = Creds {
                id: self.creds.fetch_add(1, Ordering::Relaxed),
                ttl: Duration::from_secs(7_200),
            };
            let first_turn = order.first() == Some(&PathKind::Turn);
            match reason {
                CredsReason::Primary => Ok(CredsAnswer {
                    order,
                    creds: first_turn.then_some(creds),
                }),
                _ => Ok(CredsAnswer {
                    order: vec![PathKind::Turn],
                    creds: Some(creds),
                }),
            }
        })
    }

    fn turn_release(&self, creds: Creds) {
        self.say(format!("turn_release/{}", creds.id));
    }

    fn world_peer(
        &self,
        path: PathKind,
        relay: Option<SocketAddrV4>,
    ) -> BoxFuture<'_, Option<Paths>> {
        Box::pin(async move {
            self.say(format!("world_peer/{}", path.as_str()));
            self.host.world_peer(&self.slot, path, relay).await
        })
    }

    fn turn_allocate(
        &self,
        _creds: Creds,
    ) -> BoxFuture<'_, Result<Arc<dyn TurnAllocation>, PathError>> {
        Box::pin(async move {
            let n = self.relays.fetch_add(1, Ordering::Relaxed);
            let relay = SocketAddrV4::new(Ipv4Addr::new(104, 30, 1, 2), 30_000 + n as u16);
            Ok(Arc::new(FakeAllocation {
                net: self.net.clone(),
                relay,
            }) as Arc<dyn TurnAllocation>)
        })
    }

    fn e4mc_connect(
        &self,
        domain: &str,
        _proto: i32,
    ) -> BoxFuture<'_, Result<Arc<dyn GuestPath>, PathError>> {
        let address = PathAddress::E4mc {
            domain: domain.to_owned(),
        };
        Box::pin(async move {
            self.net.guest_path(&address).ok_or(PathError {
                step: Step::Tcp,
                code: Cow::Borrowed("e4mc_unknown_domain"),
            })
        })
    }
}

impl GuestEnv for GuestSide {
    fn event(&self, event: GuestEvent) {
        let _ = self.events.send(event);
    }
}

/// A host session over a fake net, its world, and its events.
struct World {
    net: Arc<FakeNet>,
    side: Arc<HostSide>,
    session: HostSession,
    events: mpsc::UnboundedReceiver<HostEvent>,
    connections: mpsc::UnboundedReceiver<DuplexStream>,
    _dir: tempfile::TempDir,
}

async fn open_world(order: &[PathKind], max: u8) -> World {
    open_world_with(order, max, Duration::from_secs(7_200), |_| {}).await
}

async fn open_world_with(
    order: &[PathKind],
    max: u8,
    ttl: Duration,
    prepare: impl FnOnce(&std::path::Path),
) -> World {
    let dir = tempfile::tempdir().unwrap();
    prepare(dir.path());
    let (port, connections) = memory_world::listen();
    let mut config: HostConfig = crate::policy::tests::host_config(dir.path(), port);
    config.max_guests = max;
    let net = Arc::new(FakeNet::default());
    let (tx, events) = mpsc::unbounded_channel();
    let side = Arc::new(HostSide {
        net: net.clone(),
        order: Mutex::new(order.to_vec()),
        turn_ok: AtomicBool::new(true),
        ttl,
        creds: AtomicU64::new(1),
        log: Mutex::new(Vec::new()),
        events: tx,
    });
    let session = HostSession::open(config, side.clone()).await.unwrap();
    World {
        net,
        side,
        session,
        events,
        connections,
        _dir: dir,
    }
}

struct Guest {
    side: Arc<GuestSide>,
    session: GuestSession,
    events: mpsc::UnboundedReceiver<GuestEvent>,
}

async fn try_join(
    world: &World,
    friend: &str,
    name: &str,
    order: &[PathKind],
) -> (
    Result<GuestSession, JoinFailure>,
    Arc<GuestSide>,
    mpsc::UnboundedReceiver<GuestEvent>,
) {
    let join: Join = world.session.invite(friend).await.unwrap();
    let (tx, events) = mpsc::unbounded_channel();
    let side = Arc::new(GuestSide {
        net: world.net.clone(),
        host: world.session.clone(),
        slot: join.slot,
        order: Mutex::new(order.to_vec()),
        creds: AtomicU64::new(100),
        relays: AtomicU32::new(0),
        log: Mutex::new(Vec::new()),
        events: tx,
    });
    let config = GuestConfig {
        join,
        name: name.into(),
        premium: false,
        launcher: "2.1.0".into(),
        policy: Policy::default(),
    };
    let result = GuestSession::join(config, side.clone()).await;
    (result, side, events)
}

async fn join_world(world: &World, friend: &str, name: &str, order: &[PathKind]) -> Guest {
    let (result, side, events) = try_join(world, friend, name, order).await;
    Guest {
        side,
        session: result.expect("joins"),
        events,
    }
}

fn game_bytes() -> Vec<u8> {
    let file = testdata::json("noise/nkpsk0-game.json");
    hex::decode(file["gameBytesGuestToHostHex"].as_str().unwrap()).unwrap()
}

fn handshake_with(proto: i32) -> (GameHandshake, Vec<u8>) {
    let mut bytes = game_bytes();
    // The vector's protocol number is 9999 (`8f4e`); rewrite it when another is wanted.
    if proto != 9999 {
        let mut packet = vec![0x00];
        crate::tunnel::varint::write(&mut packet, proto);
        packet.extend_from_slice(&bytes[4..17]);
        let mut out = Vec::new();
        crate::tunnel::varint::write(&mut out, packet.len() as i32);
        out.extend_from_slice(&packet);
        out.extend_from_slice(&bytes[17..]);
        bytes = out;
    }
    let crate::guest::local::Parsed::Done { proto, next_state } = parse_handshake(&bytes) else {
        panic!("handshake");
    };
    (
        GameHandshake {
            proto,
            next_state,
            kept: bytes.clone(),
        },
        bytes,
    )
}

/// The game connects to the guest's local endpoint; returns the game's end.
async fn connect_game(guest: &Guest, proto: i32) -> (DuplexStream, Vec<u8>) {
    let (game, local) = tokio::io::duplex(1 << 22);
    let (handshake, bytes) = handshake_with(proto);
    guest
        .session
        .game_connected(Box::new(local), handshake, "26.1.2")
        .await;
    (game, bytes)
}

/// The world's side of one connection: the PLG1 header checked and returned.
async fn world_accepts(world: &mut World) -> (DuplexStream, Identity) {
    let mut conn = timeout(Duration::from_secs(30), world.connections.recv())
        .await
        .unwrap()
        .unwrap();
    let mut head = vec![0u8; 6];
    conn.read_exact(&mut head).await.unwrap();
    let len = u16::from_be_bytes([head[4], head[5]]) as usize;
    let mut rest = vec![0u8; len + 32];
    conn.read_exact(&mut rest).await.unwrap();
    head.extend_from_slice(&rest);
    let (identity, used) = verify(&GateKey::from_bridge_token(TOKEN), &head).unwrap();
    assert_eq!(used, head.len());
    (conn, identity)
}

async fn next_guest_event(guest: &mut Guest, wanted: impl Fn(&GuestEvent) -> bool) -> GuestEvent {
    let mut seen = Vec::new();
    loop {
        let Ok(event) = timeout(Duration::from_secs(300), guest.events.recv()).await else {
            panic!("no such guest event in time; seen: {seen:?}");
        };
        let event = event.expect("events");
        if wanted(&event) {
            return event;
        }
        seen.push(event);
    }
}

async fn next_host_event(world: &mut World, wanted: impl Fn(&HostEvent) -> bool) -> HostEvent {
    let mut seen = Vec::new();
    loop {
        let Ok(event) = timeout(Duration::from_secs(300), world.events.recv()).await else {
            panic!("no such host event in time; seen: {seen:?}");
        };
        let event = event.expect("events");
        if wanted(&event) {
            return event;
        }
        seen.push(event);
    }
}

fn pattern(len: usize, seed: u8) -> Vec<u8> {
    (0..len)
        .map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed))
        .collect()
}

/// Pushes bytes through both directions at once and checks they arrive exactly.
async fn exchange(game: &mut DuplexStream, world: &mut DuplexStream, len: usize, seed: u8) {
    let up = pattern(len, seed);
    let down = pattern(len, seed.wrapping_add(1));
    let (mut got_up, mut got_down) = (vec![0u8; len], vec![0u8; len]);
    let (mut game_read, mut game_write) = tokio::io::split(&mut *game);
    let (mut world_read, mut world_write) = tokio::io::split(&mut *world);
    let moved = timeout(Duration::from_secs(60), async {
        tokio::try_join!(
            game_write.write_all(&up),
            world_write.write_all(&down),
            world_read.read_exact(&mut got_up),
            game_read.read_exact(&mut got_down),
        )
    })
    .await;
    moved
        .expect("the bytes arrive in time")
        .expect("no I/O error");
    assert!(got_up == up, "game → world bytes differ");
    assert!(got_down == down, "world → game bytes differ");
}

#[tokio::test(start_paused = true)]
async fn a_guest_joins_plays_and_leaves() {
    let mut world = open_world(&[PathKind::Turn, PathKind::E4mc], 7).await;
    let mut guest = join_world(&world, FRIEND_A, "Notch", &[PathKind::Turn, PathKind::E4mc]).await;
    assert_eq!(guest.session.name_verdict(), NameVerdict::Ok);
    assert_eq!(
        guest.session.manifest(),
        crate::policy::tests::test_manifest()
    );
    assert_eq!(guest.session.world(), (0, 7));
    assert_eq!(
        *guest.side.log.lock().unwrap(),
        ["turn_creds/primary", "world_peer/turn"],
        "TURN first: credentials with the order, then the host permits the relay"
    );

    let (mut game, bytes) = connect_game(&guest, 9999).await;
    let (mut conn, identity) = world_accepts(&mut world).await;
    assert_eq!(identity.v, 1);
    assert_eq!(identity.id, "p1");
    assert_eq!(identity.conn, 1);
    assert_eq!(identity.guest.to_string(), "g1");
    assert_eq!(identity.name, "Notch");
    assert!(!identity.premium);
    assert_eq!(
        identity.friend,
        crate::host::binding::BindingKey::from_bytes([3; 32]).digest(FRIEND_A)
    );
    // The game's first bytes, then everything else, both ways.
    let mut first = vec![0u8; bytes.len()];
    conn.read_exact(&mut first).await.unwrap();
    assert_eq!(first, bytes);
    exchange(&mut game, &mut conn, 300_000, 7).await;

    next_guest_event(&mut guest, |e| {
        matches!(e, GuestEvent::State { guests: 1, max: 7 })
    })
    .await;
    next_host_event(
        &mut world,
        |e| matches!(e, HostEvent::Guest(g) if g.status == GuestStatus::InWorld),
    )
    .await;

    // The player leaves the world: the game closes its connection.
    drop(game);
    let mut rest = Vec::new();
    timeout(Duration::from_secs(30), conn.read_to_end(&mut rest))
        .await
        .unwrap()
        .unwrap();
    assert!(rest.is_empty());
    next_guest_event(&mut guest, |e| matches!(e, GuestEvent::GameEnded)).await;
    next_host_event(
        &mut world,
        |e| matches!(e, HostEvent::Guest(g) if g.status == GuestStatus::OutOfWorld),
    )
    .await;

    // Two minutes without a game stream: bye{leave}.
    next_guest_event(&mut guest, |e| {
        matches!(e, GuestEvent::Ended(GuestEnd::Left))
    })
    .await;
    next_host_event(
        &mut world,
        |e| matches!(e, HostEvent::Guest(g) if g.status == GuestStatus::Left),
    )
    .await;
    world.session.close().await;
    next_host_event(&mut world, |e| matches!(e, HostEvent::Closed)).await;
}

#[tokio::test(start_paused = true)]
async fn a_dead_path_moves_the_game_to_the_other_one() {
    let mut world = open_world(&[PathKind::Turn, PathKind::E4mc], 7).await;
    // Both paths are open before the guest comes (a guest asked for e4mc).
    let first = join_world(&world, FRIEND_B, "jeb_", &[PathKind::E4mc]).await;
    assert!(world.session.paths().has(PathKind::Turn) && world.session.paths().has(PathKind::E4mc));
    let mut guest = join_world(&world, FRIEND_A, "Notch", &[PathKind::Turn, PathKind::E4mc]).await;
    let (mut game, bytes) = connect_game(&guest, 9999).await;
    let (mut conn, _) = world_accepts(&mut world).await;
    let mut kept = vec![0u8; bytes.len()];
    conn.read_exact(&mut kept).await.unwrap();
    exchange(&mut game, &mut conn, 100_000, 1).await;

    // TURN goes away under the game.
    let turn = world
        .net
        .all()
        .into_iter()
        .find(|p| p.kind == PathKind::Turn)
        .unwrap();
    turn.kill.kill();
    // Bytes written while the path is down arrive after the move, once and in order.
    exchange(&mut game, &mut conn, 200_000, 2).await;
    exchange(&mut game, &mut conn, 50_000, 3).await;
    let moved = next_guest_event(&mut guest, |e| {
        matches!(e, GuestEvent::Report(Event::Move { .. }))
    })
    .await;
    let GuestEvent::Report(Event::Move { from, to, ok, .. }) = moved else {
        unreachable!()
    };
    assert_eq!((from, to, ok), (PathKind::Turn, PathKind::E4mc, true));
    drop(first);
}

#[tokio::test(start_paused = true)]
async fn turn_move_takes_the_game_to_e4mc_and_turn_closes() {
    let mut world = open_world(&[PathKind::Turn, PathKind::E4mc], 7).await;
    let mut guest = join_world(&world, FRIEND_A, "Notch", &[PathKind::Turn, PathKind::E4mc]).await;
    let (mut game, bytes) = connect_game(&guest, 9999).await;
    let (mut conn, _) = world_accepts(&mut world).await;
    let mut kept = vec![0u8; bytes.len()];
    conn.read_exact(&mut kept).await.unwrap();

    world.side.turn_ok.store(false, Ordering::SeqCst);
    world.session.turn_move("budget").await;
    exchange(&mut game, &mut conn, 150_000, 5).await;
    let moved = next_guest_event(&mut guest, |e| {
        matches!(e, GuestEvent::Report(Event::Move { .. }))
    })
    .await;
    let GuestEvent::Report(Event::Move {
        from, to, why, ok, ..
    }) = moved
    else {
        unreachable!()
    };
    assert_eq!((from, to, ok), (PathKind::Turn, PathKind::E4mc, true));
    assert_eq!(why, crate::report::MoveCause::Host);
    // The host reports its move too, and TURN closes 30 s after its last guest left it.
    next_host_event(&mut world, |e| {
        matches!(e, HostEvent::Report(Event::Move { ok: true, .. }))
    })
    .await;
    tokio::time::sleep(Duration::from_secs(31)).await;
    let turn = world
        .net
        .all()
        .into_iter()
        .find(|p| p.kind == PathKind::Turn)
        .unwrap();
    assert!(turn.kill.is_dead(), "the old TURN path is closed");
    assert!(
        world
            .side
            .log
            .lock()
            .unwrap()
            .iter()
            .any(|l| l.starts_with("turn_release/"))
    );
    assert!(!world.session.paths().has(PathKind::Turn));
    exchange(&mut game, &mut conn, 10_000, 6).await;
}

#[tokio::test(start_paused = true)]
async fn refusals() {
    // One guest at most.
    let mut world = open_world(&[PathKind::E4mc], 1).await;
    let mut a = join_world(&world, FRIEND_A, "Notch", &[PathKind::E4mc]).await;
    let (_game_a, _) = connect_game(&a, 9999).await;
    let (_conn_a, _) = world_accepts(&mut world).await;
    next_guest_event(&mut a, |e| matches!(e, GuestEvent::GameConnected)).await;

    let mut b = join_world(&world, FRIEND_B, "jeb_", &[PathKind::E4mc]).await;
    let (_game_b, _) = connect_game(&b, 9999).await;
    let refused = next_guest_event(&mut b, |e| matches!(e, GuestEvent::GameRefused(_))).await;
    assert!(matches!(refused, GuestEvent::GameRefused(Refusal::Full)));

    // Another Minecraft version.
    let (_game_b2, _) = connect_game(&b, 1234).await;
    let refused = next_guest_event(&mut b, |e| matches!(e, GuestEvent::GameRefused(_))).await;
    assert!(matches!(refused, GuestEvent::GameRefused(Refusal::Version)));

    // The host's own name: the control stream's verdict, then the game's refusal.
    let mut c = join_world(&world, "345678901234567890", "ayse", &[PathKind::E4mc]).await;
    assert_eq!(c.session.name_verdict(), NameVerdict::Name);
    let (_game_c, _) = connect_game(&c, 9999).await;
    let refused = next_guest_event(&mut c, |e| matches!(e, GuestEvent::GameRefused(_))).await;
    assert!(matches!(refused, GuestEvent::GameRefused(Refusal::Name)));
    // "Hesabı değiştir": a new name on a new control stream.
    assert_eq!(
        c.session.rename("Can_1".into(), false).await,
        Some(NameVerdict::Ok)
    );

    // Removed by the host.
    world.session.remove(FRIEND_B, true).await;
    next_guest_event(&mut b, |e| {
        matches!(e, GuestEvent::Ended(GuestEnd::Removed))
    })
    .await;
    next_host_event(
        &mut world,
        |e| matches!(e, HostEvent::Revoke { to } if to == FRIEND_B),
    )
    .await;

    // The session closes.
    world.session.close().await;
    next_guest_event(&mut a, |e| {
        matches!(e, GuestEvent::Ended(GuestEnd::HostClosed))
    })
    .await;
    next_guest_event(&mut c, |e| {
        matches!(e, GuestEvent::Ended(GuestEnd::HostClosed))
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_name_bound_to_another_friend() {
    let key = crate::host::binding::BindingKey::from_bytes([3; 32]);
    let file = format!(
        r#"{{"v":1,"key":"{}","names":[{{"name":"Notch","friend":"{}","last":1792521000}}]}}"#,
        key.key_id(),
        key.digest(FRIEND_B)
    );
    let mut world = open_world_with(&[PathKind::E4mc], 7, Duration::from_secs(7_200), |dir| {
        std::fs::write(dir.join(crate::host::binding::FILE_NAME), &file).unwrap();
    })
    .await;
    let mut guest = join_world(&world, FRIEND_A, "notch", &[PathKind::E4mc]).await;
    assert_eq!(guest.session.name_verdict(), NameVerdict::NameTaken);
    let (_game, _) = connect_game(&guest, 9999).await;
    let refused = next_guest_event(&mut guest, |e| matches!(e, GuestEvent::GameRefused(_))).await;
    assert!(matches!(
        refused,
        GuestEvent::GameRefused(Refusal::NameTaken)
    ));
    // The friend the name belongs to gets in.
    let mut owner = join_world(&world, FRIEND_B, "Notch", &[PathKind::E4mc]).await;
    assert_eq!(owner.session.name_verdict(), NameVerdict::Ok);
    let (_owner_game, _) = connect_game(&owner, 9999).await;
    let (_conn, identity) = world_accepts(&mut world).await;
    assert_eq!(identity.friend, key.digest(FRIEND_B));
    next_guest_event(&mut owner, |e| matches!(e, GuestEvent::GameConnected)).await;
}

#[tokio::test(start_paused = true)]
async fn failed_openings_make_the_host_busy() {
    let world = open_world(&[PathKind::E4mc], 7).await;
    let path = world
        .net
        .all()
        .into_iter()
        .find(|p| p.kind == PathKind::E4mc)
        .unwrap();
    for _ in 0..21 {
        let mut junk = path.connect_raw();
        junk.write_all(&[0x44; 69]).await.unwrap();
        let mut answer = Vec::new();
        // Closed without a word.
        timeout(Duration::from_secs(10), junk.read_to_end(&mut answer))
            .await
            .unwrap()
            .unwrap();
        assert!(answer.is_empty());
    }
    // A real guest is turned away too while the window lasts.
    let (result, _, _) = try_join(&world, FRIEND_A, "Notch", &[PathKind::E4mc]).await;
    assert_eq!(result.err(), Some(JoinFailure::NoPath));
    tokio::time::sleep(Duration::from_secs(61)).await;
    let (result, _, _) = try_join(&world, FRIEND_A, "Notch", &[PathKind::E4mc]).await;
    assert!(result.is_ok());
}

#[tokio::test(start_paused = true)]
async fn the_hosts_turn_renewal_moves_its_guests_to_a_new_allocation() {
    // Credentials of 25 minutes: renewal is due after 5.
    let mut world =
        open_world_with(&[PathKind::Turn], 7, Duration::from_secs(25 * 60), |_| {}).await;
    let mut guest = join_world(&world, FRIEND_A, "Notch", &[PathKind::Turn]).await;
    let (mut game, bytes) = connect_game(&guest, 9999).await;
    let (mut conn, _) = world_accepts(&mut world).await;
    let mut kept = vec![0u8; bytes.len()];
    conn.read_exact(&mut kept).await.unwrap();
    let before = world.session.paths().turn.unwrap();

    // Keep the game busy while time passes.
    for round in 0..6u8 {
        tokio::time::sleep(Duration::from_secs(60)).await;
        exchange(&mut game, &mut conn, 20_000, round).await;
    }
    let after = world.session.paths().turn.unwrap();
    assert_ne!(before.relay, after.relay, "a new allocation");
    let moved = next_guest_event(&mut guest, |e| {
        matches!(e, GuestEvent::Report(Event::Move { .. }))
    })
    .await;
    let GuestEvent::Report(Event::Move { from, to, ok, .. }) = moved else {
        unreachable!()
    };
    assert_eq!((from, to, ok), (PathKind::Turn, PathKind::Turn, true));
    tokio::time::sleep(Duration::from_secs(31)).await;
    assert!(
        world
            .side
            .log
            .lock()
            .unwrap()
            .iter()
            .any(|l| l == "turn_creds/renew")
    );
    assert!(
        world
            .side
            .log
            .lock()
            .unwrap()
            .iter()
            .any(|l| l == "turn_release/1"),
        "{:?}",
        world.side.log.lock().unwrap()
    );
    exchange(&mut game, &mut conn, 20_000, 9).await;
}

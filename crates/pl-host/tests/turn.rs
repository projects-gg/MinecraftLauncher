//! The TURN path against a local TURN server (loopback only): allocations over UDP, TCP and TLS,
//! QUIC between two allocations with the certificate pin, credentials, refreshes and renewal.

mod support;

use std::time::Duration;

use pl_host::transport::net::Trust;
use pl_host::transport::turn::{
    self, IceServers, QuicIdentity, Secret, TurnConfig, TurnGuest, TurnHostPath,
};
use pl_host::transport::{GuestPath, HostPath, PathAddress, Step};
use support::certs::TestCa;
use support::turn_server::TurnServer;
use support::within;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const HOST: (&str, &str) = ("w1.0123456789ab.sid.h", "host-password");
const GUEST: (&str, &str) = ("w1.ba9876543210.sid.g", "guest-password");

fn ice(server: &TurnServer, user: (&str, &str), kinds: &[&str]) -> IceServers {
    let urls = kinds
        .iter()
        .map(|kind| match *kind {
            "udp" => format!("turn:127.0.0.1:{}?transport=udp", server.udp.port()),
            "tcp" => format!("turn:127.0.0.1:{}?transport=tcp", server.tcp.port()),
            "tls" => format!("turns:127.0.0.1:{}?transport=tcp", server.tls.port()),
            other => other.to_owned(),
        })
        .collect();
    IceServers::new(urls, Secret::new(user.0), Secret::new(user.1), 7200)
}

fn config(ca: &TestCa) -> TurnConfig {
    TurnConfig {
        url_preference: [
            "turn:*:*?transport=udp",
            "turns:*:*?transport=tcp",
            "turn:*:*?transport=tcp",
        ]
        .map(String::from)
        .to_vec(),
        udp_head_start: Duration::from_millis(300),
        alloc_timeout: Duration::from_secs(3),
        request_timeout: Duration::from_secs(2),
        quic_connect_timeout: Duration::from_secs(3),
        trust: Trust::Roots(vec![ca.root.clone()]),
        public_relays_only: false,
        ..TurnConfig::default()
    }
}

struct Pair {
    host: TurnHostPath,
    guest: TurnGuest,
    identity: QuicIdentity,
}

async fn pair(
    server: &TurnServer,
    config: &TurnConfig,
    host_kinds: &[&str],
    guest_kinds: &[&str],
) -> Pair {
    let identity = QuicIdentity::generate().unwrap();
    let host = within(
        10,
        turn::open_host(&ice(server, HOST, host_kinds), &identity, config),
    )
    .await
    .unwrap();
    let guest = within(10, turn::allocate(&ice(server, GUEST, guest_kinds), config))
        .await
        .unwrap();
    within(5, host.admit_peer(guest.relay())).await.unwrap();
    Pair {
        host,
        guest,
        identity,
    }
}

/// The guest sends `size` bytes on a new stream and ends it; the host reads them all and sends them
/// back on the same stream.
async fn exchange(host: &TurnHostPath, path: &impl GuestPath, size: usize) {
    let data: Vec<u8> = (0..size as u32).map(|i| (i % 253) as u8).collect();
    let stream = within(5, path.open()).await.unwrap();
    let (mut guest_read, mut guest_write) = tokio::io::split(stream);
    let sending = {
        let data = data.clone();
        tokio::spawn(async move {
            guest_write.write_all(&data).await.unwrap();
            guest_write.shutdown().await.unwrap();
        })
    };
    let mut accepted = within(5, host.accept()).await.unwrap();
    let mut got = Vec::new();
    within(60, accepted.read_to_end(&mut got)).await.unwrap();
    assert!(got == data, "the host got the guest's bytes in order");
    accepted.write_all(&got).await.unwrap();
    accepted.shutdown().await.unwrap();
    let mut back = Vec::new();
    within(60, guest_read.read_to_end(&mut back)).await.unwrap();
    assert!(back == data, "the guest got them back");
    sending.await.unwrap();
}

async fn quic_between_allocations(kind: &str, code: &str) {
    let ca = TestCa::new();
    let server = TurnServer::start(&ca, &[HOST, GUEST]).await;
    let config = config(&ca);
    let Pair {
        host,
        guest,
        identity,
    } = pair(&server, &config, &[kind], &[kind]).await;
    assert_eq!(host.attempt_code(), code);
    assert_eq!(guest.attempt_code(), code);
    assert_eq!(host.tcp_only(), code == "turn_tcp_only");
    assert_eq!(guest.tcp_only(), code == "turn_tcp_only");
    assert_eq!(
        host.address(),
        PathAddress::Turn {
            relay: host.relay(),
            pin: identity.pin()
        }
    );
    let path = within(10, guest.connect(host.relay(), identity.pin()))
        .await
        .unwrap();
    exchange(&host, &path, 300_000).await;
    exchange(&host, &path, 10).await;
    assert!(
        host.rx_bytes() > 300_000 && path.rx_bytes() > 300_000,
        "both count what the server sent them"
    );
    assert!(server.bytes_to_clients() >= host.rx_bytes().min(path.rx_bytes()));
    assert_eq!(server.authenticated("allocate", HOST.0), 1);
    within(5, path.close()).await;
    within(5, host.close()).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn quic_runs_between_two_allocations_over_udp() {
    quic_between_allocations("udp", "ok").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn quic_runs_between_two_allocations_over_tcp() {
    quic_between_allocations("tcp", "turn_tcp_only").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn quic_runs_between_two_allocations_over_tls() {
    quic_between_allocations("tls", "turn_tcp_only").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn sides_on_different_transports_meet() {
    let ca = TestCa::new();
    let server = TurnServer::start(&ca, &[HOST, GUEST]).await;
    let config = config(&ca);
    let Pair {
        host,
        guest,
        identity,
    } = pair(&server, &config, &["tls"], &["udp"]).await;
    let path = within(10, guest.connect(host.relay(), identity.pin()))
        .await
        .unwrap();
    exchange(&host, &path, 50_000).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn udp_that_never_answers_falls_back_to_tls() {
    let ca = TestCa::new();
    let server = TurnServer::start(&ca, &[HOST, GUEST]).await;
    server.drop_udp(true);
    let config = config(&ca);
    let started = std::time::Instant::now();
    let Pair {
        host,
        guest,
        identity,
    } = pair(&server, &config, &["udp", "tls"], &["udp", "tcp"]).await;
    assert_eq!(host.attempt_code(), "turn_tcp_only");
    assert_eq!(guest.attempt_code(), "turn_tcp_only");
    assert!(
        started.elapsed() >= config.udp_head_start,
        "TLS waited for UDP's head start"
    );
    let path = within(10, guest.connect(host.relay(), identity.pin()))
        .await
        .unwrap();
    exchange(&host, &path, 20_000).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stale_nonce_is_answered_again() {
    let ca = TestCa::new();
    let server = TurnServer::start(&ca, &[HOST, GUEST]).await;
    server.stale_nonce_once();
    let config = config(&ca);
    let Pair {
        host,
        guest,
        identity,
    } = pair(&server, &config, &["udp"], &["udp"]).await;
    let path = within(10, guest.connect(host.relay(), identity.pin()))
        .await
        .unwrap();
    exchange(&host, &path, 1_000).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn wrong_credentials_are_unauthorized() {
    let ca = TestCa::new();
    let server = TurnServer::start(&ca, &[HOST]).await;
    let config = config(&ca);
    for kinds in [&["udp"][..], &["tls"][..]] {
        let error = within(
            10,
            turn::allocate(&ice(&server, (HOST.0, "wrong"), kinds), &config),
        )
        .await
        .err()
        .unwrap();
        assert_eq!(
            (error.step, error.code.as_ref()),
            (Step::Alloc, "turn_alloc_unauthorized")
        );
    }
    assert_eq!(server.allocations(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn nothing_usable_or_nothing_answering() {
    let ca = TestCa::new();
    let server = TurnServer::start(&ca, &[HOST]).await;
    let config = TurnConfig {
        alloc_timeout: Duration::from_millis(1500),
        ..config(&ca)
    };
    let error = within(
        10,
        turn::allocate(&ice(&server, HOST, &["stun:127.0.0.1:3478"]), &config),
    )
    .await
    .err()
    .unwrap();
    assert_eq!(
        (error.step, error.code.as_ref()),
        (Step::Alloc, "turn_alloc_error")
    );
    server.drop_udp(true);
    let error = within(10, turn::allocate(&ice(&server, HOST, &["udp"]), &config))
        .await
        .err()
        .unwrap();
    assert_eq!(
        (error.step, error.code.as_ref()),
        (Step::Alloc, "turn_alloc_timeout")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn only_the_pinned_certificate_is_accepted() {
    let ca = TestCa::new();
    let server = TurnServer::start(&ca, &[HOST, GUEST]).await;
    let config = config(&ca);
    let Pair { host, guest, .. } = pair(&server, &config, &["udp"], &["udp"]).await;
    let other = QuicIdentity::generate().unwrap();
    let error = within(10, guest.connect(host.relay(), other.pin()))
        .await
        .err()
        .unwrap();
    assert_eq!(
        (error.step, error.code.as_ref()),
        (Step::Quic, "turn_quic_pin")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn private_relay_addresses_are_refused_unless_testing() {
    let ca = TestCa::new();
    let server = TurnServer::start(&ca, &[HOST, GUEST]).await;
    let config = TurnConfig {
        public_relays_only: true,
        ..config(&ca)
    };
    let identity = QuicIdentity::generate().unwrap();
    let host = within(
        10,
        turn::open_host(&ice(&server, HOST, &["udp"]), &identity, &config),
    )
    .await
    .unwrap();
    let guest = within(10, turn::allocate(&ice(&server, GUEST, &["udp"]), &config))
        .await
        .unwrap();
    let error = within(5, host.admit_peer(guest.relay()))
        .await
        .err()
        .unwrap();
    assert_eq!(
        (error.step, error.code.as_ref()),
        (Step::Peer, "turn_permission_failed")
    );
    let error = within(5, guest.connect(host.relay(), identity.pin()))
        .await
        .err()
        .unwrap();
    assert_eq!(
        (error.step, error.code.as_ref()),
        (Step::Peer, "turn_permission_failed")
    );
    assert_eq!(
        server.authenticated("permission", HOST.0),
        0,
        "nothing was asked of the server"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn refreshes_keep_it_alive_and_revocation_ends_it() {
    let ca = TestCa::new();
    let server = TurnServer::start(&ca, &[HOST, GUEST]).await;
    // A refresh every second, and an allocation that outlives a machine stalled for some seconds
    // (with 3 s, a stall of 2 let it expire); the wait below stays past the lifetime.
    let config = TurnConfig {
        allocation_lifetime: Duration::from_secs(6),
        refresh_when_left: Duration::from_secs(5),
        permission_refresh: Duration::from_secs(1),
        channel_refresh: Duration::from_secs(1),
        ..config(&ca)
    };
    let Pair {
        host,
        guest,
        identity,
    } = pair(&server, &config, &["udp"], &["tcp"]).await;
    let path = within(10, guest.connect(host.relay(), identity.pin()))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(6500)).await;
    exchange(&host, &path, 5_000).await;
    assert!(
        server.authenticated("refresh", HOST.0) >= 2,
        "the allocation was refreshed"
    );
    assert!(
        server.authenticated("permission", HOST.0) >= 3,
        "the guest's permission was kept"
    );
    assert!(
        server.authenticated("channel", GUEST.0) >= 3,
        "the guest's channel was kept"
    );

    server.revoke(HOST.0);
    let lost = within(10, host.closed()).await;
    assert_eq!(
        (lost.step, lost.code.as_ref()),
        (Step::Alloc, "turn_alloc_unauthorized")
    );
    // A lost path fails at once rather than after QUIC's idle timeout.
    let Err(error) = within(2, host.accept()).await else {
        panic!("a lost path accepts nothing");
    };
    assert_eq!(error.code, "turn_alloc_unauthorized");
    let lost = within(10, async {
        server.revoke(GUEST.0);
        path.closed().await
    })
    .await;
    assert_eq!(lost.code, "turn_alloc_unauthorized");
    assert!(within(2, path.open()).await.is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn renewal_moves_to_a_new_allocation_with_the_same_pin() {
    let ca = TestCa::new();
    let renewed_user = ("w1.0123456789ab.sid.h2", "renewed-password");
    let server = TurnServer::start(&ca, &[HOST, GUEST, renewed_user]).await;
    let config = config(&ca);
    let Pair {
        host,
        guest,
        identity,
    } = pair(&server, &config, &["udp"], &["udp"]).await;
    let old_path = within(10, guest.connect(host.relay(), identity.pin()))
        .await
        .unwrap();
    exchange(&host, &old_path, 1_000).await;

    let renewed = within(10, host.renew(&ice(&server, renewed_user, &["udp"])))
        .await
        .unwrap();
    assert_ne!(renewed.relay(), host.relay());
    assert_eq!(
        renewed.address(),
        PathAddress::Turn {
            relay: renewed.relay(),
            pin: identity.pin()
        }
    );
    // The guest was admitted on the new allocation before anyone moved.
    assert_eq!(server.authenticated("permission", renewed_user.0), 1);
    let new_path = within(10, guest.connect(renewed.relay(), identity.pin()))
        .await
        .unwrap();
    exchange(&renewed, &new_path, 1_000).await;

    let deleted = server.deleted();
    within(5, old_path.close()).await;
    within(5, host.close()).await;
    assert_eq!(
        server.deleted(),
        deleted + 1,
        "the old allocation was deleted"
    );
    assert_eq!(server.allocations_of(HOST.0), 0);
    exchange(&renewed, &new_path, 1_000).await;
    within(5, new_path.close()).await;
    assert_eq!(
        server.allocations_of(GUEST.0),
        1,
        "the guest's allocation lives while the guest holds it"
    );
    drop(guest);
    within(5, renewed.close()).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_last_holder_deletes_the_guests_allocation() {
    let ca = TestCa::new();
    let server = TurnServer::start(&ca, &[HOST, GUEST]).await;
    let config = config(&ca);
    let Pair {
        host,
        guest,
        identity,
    } = pair(&server, &config, &["udp"], &["tcp"]).await;
    let path = within(10, guest.connect(host.relay(), identity.pin()))
        .await
        .unwrap();
    exchange(&host, &path, 1_000).await;
    drop(guest);
    assert_eq!(server.allocations_of(GUEST.0), 1);
    within(5, path.close()).await;
    assert_eq!(server.allocations_of(GUEST.0), 0);
    assert_eq!(within(5, path.open()).await.err().unwrap().step, Step::Quic);
    within(5, host.close()).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_forgotten_guest_is_cut_off() {
    let ca = TestCa::new();
    let server = TurnServer::start(&ca, &[HOST, GUEST]).await;
    let config = TurnConfig {
        quic_connect_timeout: Duration::from_millis(1500),
        ..config(&ca)
    };
    let Pair {
        host,
        guest,
        identity,
    } = pair(&server, &config, &["udp"], &["udp"]).await;
    let path = within(10, guest.connect(host.relay(), identity.pin()))
        .await
        .unwrap();
    exchange(&host, &path, 1_000).await;
    host.forget_peer(guest.relay());
    // QUIC's close still reaches it; after that nothing it sends gets through.
    tokio::time::sleep(Duration::from_millis(500)).await;
    let error = within(10, path.open()).await.err().unwrap();
    assert_eq!(
        (error.step, error.code.as_ref()),
        (Step::Quic, "turn_quic_timeout")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_guest_can_reconnect_on_the_same_allocation() {
    let ca = TestCa::new();
    let server = TurnServer::start(&ca, &[HOST, GUEST]).await;
    let config = config(&ca);
    let Pair {
        host,
        guest,
        identity,
    } = pair(&server, &config, &["udp"], &["udp"]).await;
    let first = within(10, guest.connect(host.relay(), identity.pin()))
        .await
        .unwrap();
    exchange(&host, &first, 1_000).await;
    within(5, first.close()).await;
    let second = within(10, guest.connect(host.relay(), identity.pin()))
        .await
        .unwrap();
    exchange(&host, &second, 1_000).await;
    assert_eq!(
        server.authenticated("allocate", GUEST.0),
        1,
        "one allocation served both"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn closing_one_of_two_paths_to_a_host_keeps_the_other() {
    let ca = TestCa::new();
    let server = TurnServer::start(&ca, &[HOST, GUEST]).await;
    let config = config(&ca);
    let Pair {
        host,
        guest,
        identity,
    } = pair(&server, &config, &["udp"], &["udp"]).await;
    let first = within(10, guest.connect(host.relay(), identity.pin()))
        .await
        .unwrap();
    let second = within(10, guest.connect(host.relay(), identity.pin()))
        .await
        .unwrap();
    within(5, first.close()).await;
    within(5, first.close()).await;
    // Past the forgotten-peer linger: the permission still holds for the second path.
    tokio::time::sleep(Duration::from_millis(500)).await;
    exchange(&host, &second, 5_000).await;
}

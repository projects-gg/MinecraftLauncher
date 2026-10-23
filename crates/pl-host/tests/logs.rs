//! PROTOCOL.md §13: no IP address, port of a relay address, TURN username or credential, e4mc
//! domain (other than as `*.e4mc.link`) or relay broadcast text in any log line. Runs both paths
//! through success and failure with every log line captured, then searches them.

mod support;

use std::io;
use std::net::SocketAddr;
use std::time::Duration;

use pl_host::transport::e4mc::{self, E4mcConfig};
use pl_host::transport::net::{Resolver, Trust};
use pl_host::transport::turn::{self, IceServers, QuicIdentity, Secret, TurnConfig};
use pl_host::transport::{GuestPath, HostPath};
use support::certs::TestCa;
use support::quiclime::{Broker, BrokerReply, FakeRelay, RelayOptions};
use support::turn_server::TurnServer;
use support::within;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const BROADCAST: &str = "operator note 4f1c: maintenance at 03:00";

#[tokio::test(flavor = "multi_thread")]
async fn nothing_secret_reaches_the_logs() {
    support::logs::install();
    let ca = TestCa::new();
    let mut forbidden: Vec<String> = vec![
        "127.0.0.1".into(),
        "::1".into(),
        BROADCAST.into(),
        "maintenance".into(),
    ];

    // TURN, both sides, UDP and TLS, then a revocation and a wrong password.
    let host_user = ("w1.1a2b3c4d5e6f.c2lk.h", "cred-host-9d8f7e6a");
    let guest_user = ("w1.6f5e4d3c2b1a.c2lk.g", "cred-guest-1a2b3c4d");
    forbidden
        .extend([host_user.0, host_user.1, guest_user.0, guest_user.1, "c2lk"].map(String::from));
    let server = TurnServer::start(&ca, &[host_user, guest_user]).await;
    let config = TurnConfig {
        url_preference: ["turn:*:*?transport=udp", "turns:*:*?transport=tcp"]
            .map(String::from)
            .to_vec(),
        udp_head_start: Duration::from_millis(200),
        allocation_lifetime: Duration::from_secs(3),
        refresh_when_left: Duration::from_secs(2),
        trust: Trust::Roots(vec![ca.root.clone()]),
        public_relays_only: false,
        ..TurnConfig::default()
    };
    let urls = |user: (&str, &str)| {
        IceServers::new(
            vec![
                format!("turn:127.0.0.1:{}?transport=udp", server.udp.port()),
                format!("turns:127.0.0.1:{}?transport=tcp", server.tls.port()),
            ],
            Secret::new(user.0),
            Secret::new(user.1),
            7200,
        )
    };
    server.drop_udp(true);
    let identity = QuicIdentity::generate().unwrap();
    let host = within(10, turn::open_host(&urls(host_user), &identity, &config))
        .await
        .unwrap();
    server.drop_udp(false);
    let guest = within(10, turn::allocate(&urls(guest_user), &config))
        .await
        .unwrap();
    for relay in [host.relay(), guest.relay()] {
        forbidden.push(format!(":{}", relay.port()));
        forbidden.push(format!("={}", relay.port()));
    }
    for port in [server.udp.port(), server.tcp.port(), server.tls.port()] {
        forbidden.push(format!(":{port}"));
    }
    within(5, host.admit_peer(guest.relay())).await.unwrap();
    let path = within(10, guest.connect(host.relay(), identity.pin()))
        .await
        .unwrap();
    let mut stream = within(5, path.open()).await.unwrap();
    stream.write_all(b"ping").await.unwrap();
    let mut accepted = within(5, host.accept()).await.unwrap();
    let mut ping = [0u8; 4];
    accepted.read_exact(&mut ping).await.unwrap();
    let wrong = IceServers::new(
        urls(guest_user).urls,
        Secret::new(guest_user.0),
        Secret::new("not-it"),
        7200,
    );
    assert!(within(10, turn::allocate(&wrong, &config)).await.is_err());
    let other = QuicIdentity::generate().unwrap();
    assert!(
        within(10, guest.connect(host.relay(), other.pin()))
            .await
            .is_err()
    );
    server.revoke(host_user.0);
    within(10, host.closed()).await;
    within(5, path.close()).await;

    // e4mc: a failing broker, a relay with a broadcast, a guest through it, a refusal.
    let relay = FakeRelay::start(
        &ca,
        RelayOptions {
            broadcast: Some(BROADCAST.into()),
            ..RelayOptions::default()
        },
    )
    .await;
    let broker = Broker::start(BrokerReply::Status(502)).await;
    let mut e4mc_config = E4mcConfig::new("2.0.0-test");
    e4mc_config.broker_url = broker.url();
    e4mc_config.relay_port = relay.quic.port();
    e4mc_config.player_port = relay.minecraft.port();
    e4mc_config.trust = Trust::Roots(vec![ca.root.clone()]);
    e4mc_config.resolver = Resolver::custom(|host, port| {
        Box::pin(async move {
            if host.ends_with(".e4mc.link") {
                Ok(vec![SocketAddr::from(([127, 0, 0, 1], port))])
            } else {
                Err(io::ErrorKind::NotFound.into())
            }
        })
    });
    forbidden.extend(["able-acid", "nope-nope"].map(String::from));
    for port in [
        relay.quic.port(),
        relay.minecraft.port(),
        broker.address.port(),
    ] {
        forbidden.push(format!(":{port}"));
    }
    let opening = within(10, e4mc::open_host(&e4mc_config, "auto")).await;
    assert!(opening.broker_error.is_some());
    let e4mc_host = opening.result.unwrap();
    let e4mc_guest = e4mc::guest(e4mc_host.domain(), 9999, &e4mc_config).unwrap();
    let mut stream = within(5, e4mc_guest.open()).await.unwrap();
    stream.write_all(b"hello").await.unwrap();
    stream.flush().await.unwrap();
    let mut accepted = within(5, e4mc_host.accept()).await.unwrap();
    let mut hello = [0u8; 5];
    accepted.read_exact(&mut hello).await.unwrap();
    let refused = e4mc::guest("nope-nope.eu.e4mc.link", 9999, &e4mc_config).unwrap();
    let mut stream = within(5, refused.open()).await.unwrap();
    stream.write_all(&[0x01, 0x00]).await.unwrap();
    stream.flush().await.unwrap();
    let mut reply = Vec::new();
    let _ = within(5, stream.read_to_end(&mut reply)).await;
    relay.close_hosts();
    within(5, e4mc_host.closed()).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    let lines = support::logs::lines();
    assert!(
        lines.iter().any(|line| line.contains("pl_host")),
        "the capture works: {lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|line| line.contains("e4mc_broker_http_502"))
    );
    assert!(
        lines
            .iter()
            .any(|line| line.contains("TURN allocation lost"))
    );
    for line in &lines {
        // Our relay and server stand-ins use quinn's own socket, which can log the peer it failed
        // to send to; the launcher's paths never use that socket.
        if line.contains("quinn_udp") {
            continue;
        }
        for secret in &forbidden {
            assert!(
                !line.contains(secret.as_str()),
                "{secret:?} in log line {line:?}"
            );
        }
    }
}

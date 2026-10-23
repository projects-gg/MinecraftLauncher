//! The e4mc path against a local QUIClime relay and broker (loopback only).

mod support;

use std::io;
use std::net::SocketAddr;
use std::time::Duration;

use pl_host::transport::e4mc::{self, E4mcConfig, HostOpening};
use pl_host::transport::net::{Resolver, Trust};
use pl_host::transport::{BoxStream, GuestPath, HostPath, PathAddress, PathError, Step};
use support::certs::TestCa;
use support::quiclime::{Broker, BrokerReply, DomainReply, FakeRelay, RelayOptions};
use support::{hex_field, vectors, within};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Every `*.e4mc.link` name is the local relay; nothing else resolves.
fn local_names() -> Resolver {
    Resolver::custom(|host, port| {
        Box::pin(async move {
            if host.ends_with(".e4mc.link") {
                Ok(vec![SocketAddr::from(([127, 0, 0, 1], port))])
            } else {
                Err(io::ErrorKind::NotFound.into())
            }
        })
    })
}

fn config(ca: &TestCa, relay: &FakeRelay, broker: Option<&Broker>) -> E4mcConfig {
    let mut config = E4mcConfig::new("2.0.0-test");
    // An unused loopback port: the broker refuses at once.
    config.broker_url =
        broker.map_or_else(|| "http://127.0.0.1:9/getBestRelay".to_owned(), Broker::url);
    config.relay_port = relay.quic.port();
    config.player_port = relay.minecraft.port();
    config.trust = Trust::Roots(vec![ca.root.clone()]);
    config.resolver = local_names();
    config.broker_timeout = Duration::from_millis(600);
    config.domain_timeout = Duration::from_millis(600);
    config
}

fn broker_answer(relay: &FakeRelay, id: &str) -> BrokerReply {
    BrokerReply::Json(format!(
        r#"{{"id":"{id}","host":"{id}.e4mc.link","port":{}}}"#,
        relay.quic.port()
    ))
}

struct Opened {
    broker_error: Option<PathError>,
    region: Option<String>,
}

async fn open(config: &E4mcConfig, region: &str) -> (Opened, e4mc::E4mcHost) {
    let HostOpening {
        broker_error,
        region,
        result,
    } = within(10, e4mc::open_host(config, region)).await;
    (
        Opened {
            broker_error,
            region,
        },
        result.expect("the e4mc path opens"),
    )
}

/// What a guest's tunnel does first: the opening, then the reply's VarInt length and bytes.
async fn opening_reply(stream: &mut BoxStream, opening: &[u8]) -> (u32, Vec<u8>) {
    stream.write_all(opening).await.unwrap();
    stream.flush().await.unwrap();
    let mut len = 0u32;
    for shift in 0..5 {
        let byte = stream.read_u8().await.unwrap();
        len |= u32::from(byte & 0x7F) << (7 * shift);
        if byte & 0x80 == 0 {
            break;
        }
    }
    let mut packet = vec![0u8; len as usize];
    stream.read_exact(&mut packet).await.unwrap();
    (len, packet)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_guest_reaches_the_host_through_the_relay() {
    let wire = vectors("e4mc/wire.json");
    let opening_bytes = hex_field(&wire["guestFirstWrite"]["openingHex"]);
    let ca = TestCa::new();
    let relay = FakeRelay::start(&ca, RelayOptions::default()).await;
    let broker = Broker::start(broker_answer(&relay, "eu")).await;
    let config = config(&ca, &relay, Some(&broker));
    let (opening, host) = open(&config, "auto").await;
    assert!(opening.broker_error.is_none());
    assert_eq!(opening.region.as_deref(), Some("eu"));
    assert_eq!(host.region(), "eu");
    let domain = wire["control"]["domain"].as_str().unwrap();
    assert_eq!(host.domain(), domain);
    assert_eq!(
        host.address(),
        PathAddress::E4mc {
            domain: domain.to_owned()
        }
    );
    assert_eq!(
        broker.user_agents(),
        vec!["ProjectsLauncher/2.0.0-test (+https://projects.gg)".to_owned()]
    );

    // The control stream stays ours after the domain: the relay still routes a while later.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let guest = e4mc::guest(domain, 9999, &config).unwrap();
    let mut stream = within(5, guest.open()).await.unwrap();
    stream.write_all(&opening_bytes).await.unwrap();
    stream.flush().await.unwrap();

    let mut accepted = within(5, host.accept()).await.unwrap();
    let mut seen = vec![0u8; opening_bytes.len()];
    accepted.read_exact(&mut seen).await.unwrap();
    assert_eq!(
        seen, opening_bytes,
        "the host's stream starts at the PLJ1 opening"
    );
    // One TCP write carried the handshake and the opening; the relay read them as the vectors say.
    assert_eq!(
        relay.guest_bytes(0),
        hex_field(&wire["guestFirstWrite"]["bothHex"])
    );

    let mut reply = vec![0x30];
    reply.extend_from_slice(&[7u8; 48]);
    accepted.write_all(&reply).await.unwrap();
    let mut got = vec![0u8; 49];
    stream.read_exact(&mut got).await.unwrap();
    assert_eq!(got, reply);

    // Bulk both ways.
    let bulk: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
    let (mut guest_read, mut guest_write) = tokio::io::split(stream);
    let sending = {
        let bulk = bulk.clone();
        tokio::spawn(async move {
            guest_write.write_all(&bulk).await.unwrap();
            guest_write
        })
    };
    let mut echoed = vec![0u8; bulk.len()];
    accepted.read_exact(&mut echoed).await.unwrap();
    assert_eq!(echoed, bulk);
    accepted.write_all(&bulk).await.unwrap();
    let mut back = vec![0u8; bulk.len()];
    guest_read.read_exact(&mut back).await.unwrap();
    assert_eq!(back, bulk);
    let _guest_write = sending.await.unwrap();
    assert!(host.rx_bytes() > bulk.len() as u64);
    assert!(guest.rx_bytes() >= (bulk.len() + 49) as u64);
    host.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_relays_refusals_are_named() {
    let wire = vectors("e4mc/wire.json");
    let opening = hex_field(&wire["guestFirstWrite"]["openingHex"]);
    let refusals = wire["relayRefusals"].as_array().unwrap();
    let ca = TestCa::new();

    let relay = FakeRelay::start(&ca, RelayOptions::default()).await;
    let config = config(&ca, &relay, None);
    let guest = e4mc::guest("nope-nope.eu.e4mc.link", 9999, &config).unwrap();
    let mut stream = within(5, guest.open()).await.unwrap();
    let (len, packet) = within(5, opening_reply(&mut stream, &opening)).await;
    assert_ne!(len, 48);
    let mut whole = vec![len as u8];
    whole.extend_from_slice(&packet);
    assert_eq!(whole, hex_field(&refusals[0]["hex"]));
    assert_eq!(
        guest.explain_reply(&packet).as_deref(),
        Some("e4mc_unknown_domain")
    );

    let limited = FakeRelay::start(
        &ca,
        RelayOptions {
            limit_per_minute: 0,
            ..RelayOptions::default()
        },
    )
    .await;
    let config = self::config(&ca, &limited, None);
    let guest = e4mc::guest("able-acid.eu.e4mc.link", 9999, &config).unwrap();
    let mut stream = within(5, guest.open()).await.unwrap();
    let (len, packet) = within(5, opening_reply(&mut stream, &opening)).await;
    let mut whole = vec![len as u8];
    whole.extend_from_slice(&packet);
    assert_eq!(whole, hex_field(&refusals[1]["hex"]));
    assert_eq!(
        guest.explain_reply(&packet).as_deref(),
        Some("e4mc_rate_limited")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn broker_failures_fall_back_to_the_known_relays() {
    let ca = TestCa::new();
    let relay = FakeRelay::start(&ca, RelayOptions::default()).await;
    let cases = [
        (BrokerReply::Status(503), "e4mc_broker_http_503"),
        (BrokerReply::Json("not json".into()), "e4mc_broker_bad"),
        (
            BrokerReply::Json(r#"{"id":"eu","host":"relay.evil.example","port":25575}"#.into()),
            "e4mc_broker_bad",
        ),
        (
            BrokerReply::Json(r#"{"id":"eu","host":"eu.e4mc.link","port":0}"#.into()),
            "e4mc_broker_bad",
        ),
        (BrokerReply::Hang, "e4mc_broker_timeout"),
    ];
    for (reply, code) in cases {
        let broker = Broker::start(reply).await;
        let config = config(&ca, &relay, Some(&broker));
        let (opening, host) = open(&config, "auto").await;
        let error = opening.broker_error.expect("the broker failed");
        assert_eq!((error.step, error.code.as_ref()), (Step::Broker, code));
        assert_eq!(
            opening.region.as_deref(),
            Some("eu"),
            "the first fallback relay"
        );
        host.close().await;
    }
    // Nothing listening for the broker at all.
    let config = config(&ca, &relay, None);
    let (opening, host) = open(&config, "auto").await;
    assert_eq!(opening.broker_error.unwrap().code, "e4mc_broker_timeout");
    host.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_named_region_goes_straight_to_its_relay() {
    let ca = TestCa::new();
    let relay = FakeRelay::start(
        &ca,
        RelayOptions {
            base: "de.e4mc.link",
            ..RelayOptions::default()
        },
    )
    .await;
    let broker = Broker::start(BrokerReply::Status(500)).await;
    let config = config(&ca, &relay, Some(&broker));
    let (opening, host) = open(&config, "de").await;
    assert!(opening.broker_error.is_none());
    assert!(broker.user_agents().is_empty(), "the broker was not asked");
    assert_eq!(opening.region.as_deref(), Some("de"));
    assert_eq!(host.domain(), "able-acid.de.e4mc.link");
    host.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn relay_changes_read_as_protocol_changes() {
    let ca = TestCa::new();
    let cases = [
        (
            RelayOptions {
                alpn: b"quiclime-2",
                ..RelayOptions::default()
            },
            Step::Quic,
            "e4mc_protocol_changed",
        ),
        (
            RelayOptions {
                domain: DomainReply::Unknown("ticket_required"),
                ..RelayOptions::default()
            },
            Step::Domain,
            "e4mc_protocol_changed",
        ),
        (
            RelayOptions {
                domain: DomainReply::Never,
                ..RelayOptions::default()
            },
            Step::Domain,
            "e4mc_domain_timeout",
        ),
    ];
    for (options, step, code) in cases {
        let relay = FakeRelay::start(&ca, options).await;
        let broker = Broker::start(broker_answer(&relay, "eu")).await;
        let config = config(&ca, &relay, Some(&broker));
        let opening = within(10, e4mc::open_host(&config, "auto")).await;
        let error = opening
            .result
            .expect_err("the relay is not one we speak to");
        assert_eq!((error.step, error.code.as_ref()), (step, code));
    }
    // A capability answer before the domain is skipped.
    let relay = FakeRelay::start(
        &ca,
        RelayOptions {
            domain: DomainReply::CapabilitiesFirst,
            ..RelayOptions::default()
        },
    )
    .await;
    let broker = Broker::start(broker_answer(&relay, "eu")).await;
    let (_, host) = open(&config(&ca, &relay, Some(&broker)), "auto").await;
    assert_eq!(host.domain(), "able-acid.eu.e4mc.link");
    host.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_untrusted_relay_is_refused() {
    let ca = TestCa::new();
    let relay = FakeRelay::start(&ca, RelayOptions::default()).await;
    let broker = Broker::start(broker_answer(&relay, "eu")).await;
    let mut config = config(&ca, &relay, Some(&broker));
    config.trust = Trust::Roots(vec![TestCa::new().root]);
    let opening = within(10, e4mc::open_host(&config, "auto")).await;
    let error = opening.result.expect_err("another CA signed the relay");
    assert_eq!(
        (error.step, error.code.as_ref()),
        (Step::Quic, "e4mc_quic_refused")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_relay_going_away_ends_the_path() {
    let ca = TestCa::new();
    let relay = FakeRelay::start(&ca, RelayOptions::default()).await;
    let broker = Broker::start(broker_answer(&relay, "eu")).await;
    let (_, host) = open(&config(&ca, &relay, Some(&broker)), "auto").await;
    relay.close_hosts();
    let error = within(5, host.closed()).await;
    assert_eq!(
        (error.step, error.code.as_ref()),
        (Step::Quic, "e4mc_closed")
    );
    let Err(error) = within(5, host.accept()).await else {
        panic!("no more streams");
    };
    assert_eq!(error.code, "e4mc_closed");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stream_without_our_handshake_is_closed() {
    let wire = vectors("e4mc/wire.json");
    let ca = TestCa::new();
    let relay = FakeRelay::start(
        &ca,
        RelayOptions {
            next_state_to_host: Some(1),
            ..RelayOptions::default()
        },
    )
    .await;
    let broker = Broker::start(broker_answer(&relay, "eu")).await;
    let config = config(&ca, &relay, Some(&broker));
    let (_, host) = open(&config, "auto").await;
    let guest = e4mc::guest(host.domain(), 9999, &config).unwrap();
    let mut stream = within(5, guest.open()).await.unwrap();
    stream
        .write_all(&hex_field(&wire["guestFirstWrite"]["openingHex"]))
        .await
        .unwrap();
    stream.flush().await.unwrap();
    let mut buf = [0u8; 16];
    let read = within(5, stream.read(&mut buf)).await;
    assert!(
        matches!(read, Ok(0) | Err(_)),
        "the relay closed the guest's connection"
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(300), host.accept())
            .await
            .is_err(),
        "nothing reached accept"
    );
    host.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn closing_the_guest_path_ends_its_streams() {
    let ca = TestCa::new();
    let relay = FakeRelay::start(&ca, RelayOptions::default()).await;
    let broker = Broker::start(broker_answer(&relay, "eu")).await;
    let config = config(&ca, &relay, Some(&broker));
    let (_, host) = open(&config, "auto").await;
    let guest = e4mc::guest(host.domain(), 9999, &config).unwrap();
    let mut stream = within(5, guest.open()).await.unwrap();
    stream.write_all(b"hello").await.unwrap();
    stream.flush().await.unwrap();
    let mut accepted = within(5, host.accept()).await.unwrap();
    let mut hello = [0u8; 5];
    accepted.read_exact(&mut hello).await.unwrap();
    within(5, guest.close()).await;
    let mut buf = [0u8; 8];
    let read = within(5, stream.read(&mut buf)).await;
    assert!(matches!(read, Ok(0) | Err(_)));
    assert_eq!(
        within(5, guest.open()).await.err().unwrap().code,
        "e4mc_closed"
    );
    assert_eq!(within(5, guest.closed()).await.code, "e4mc_closed");
    host.close().await;
}

#[tokio::test]
async fn a_guest_connects_to_e4mc_names_only() {
    let config = E4mcConfig::new("2.0.0-test");
    for domain in [
        "evil.example",
        "able-acid.eu.e4mc.link.evil.example",
        "eu.e4mc.link",
        "ABLE-ACID.EU.E4MC.LINK",
    ] {
        let error = e4mc::guest(domain, 9999, &config).expect_err(domain);
        assert_eq!(
            (error.step, error.code.as_ref()),
            (Step::Tcp, "e4mc_unknown_domain")
        );
    }
    let mut config = config;
    config.resolver =
        Resolver::custom(|_, _| Box::pin(async { Err(io::ErrorKind::NotFound.into()) }));
    let guest = e4mc::guest("able-acid.eu.e4mc.link", 9999, &config).unwrap();
    let error = within(5, guest.open()).await.err().unwrap();
    assert_eq!(
        (error.step, error.code.as_ref()),
        (Step::Tcp, "e4mc_dns_failed")
    );
}

//! The PLJ1 test vectors (`testdata/noise/*.json`, `testdata/plj1/records.json`,
//! `testdata/invite/join.json`), reproduced byte for byte.

use serde_json::Value;
use sha2::{Digest, Sha256};

use super::control::Control;
use super::ids::{HostKey, Psk, SessionId, Slot, b64u};
use super::invite::{Join, parse_relay};
use super::noise::{Cipher, Initiator, parse_opening, prologue, respond_with};
use super::record::{ControlJoiner, Record, frame, split_control};
use super::varint;
use crate::manifest::{Loader, Manifest, ModRef, PackRef};
use crate::testdata;

fn bytes(value: &Value) -> Vec<u8> {
    hex::decode(value.as_str().expect("hex string")).expect("hex")
}

fn array<const N: usize>(value: &Value) -> [u8; N] {
    bytes(value).try_into().expect("length")
}

/// The record a vector's `record` field describes, built with this crate's types.
fn record_of(description: &Value) -> Record {
    match description["type"].as_str().unwrap() {
        "ctrl" => {
            let json = serde_json::to_vec(&description["json"]).unwrap();
            let message = Control::parse(&json).expect("vector control messages parse");
            Record::Ctrl(message.to_json())
        }
        "data" => Record::Data {
            offset: description["offset"].as_u64().unwrap(),
            bytes: bytes(&description["payloadHex"]),
        },
        "ack" => Record::Ack(description["next"].as_u64().unwrap()),
        "ping" => Record::Ping(array(&description["bodyHex"])),
        "pong" => Record::Pong(array(&description["bodyHex"])),
        other => panic!("unknown record type {other}"),
    }
}

struct Keys {
    host: HostKey,
    psk: Psk,
    slot: Slot,
    session: SessionId,
}

fn keys(file: &Value) -> Keys {
    let host = HostKey::from_secret(array(&file["hostStaticPrivateHex"]));
    assert_eq!(
        hex::encode(host.public()),
        file["hostStaticPublicHex"].as_str().unwrap()
    );
    assert_eq!(
        b64u(host.public()),
        file["hostStaticPublicB64u"].as_str().unwrap()
    );
    let psk = Psk::from_bytes(array(&file["pskHex"]));
    assert_eq!(b64u(psk.expose()), file["pskB64u"].as_str().unwrap());
    let slot = Slot::from_bytes(array(&file["slotHex"]));
    assert_eq!(slot.to_b64u(), file["slotB64u"].as_str().unwrap());
    let session = SessionId::from_bytes(array(&file["sessionHex"]));
    assert_eq!(session.to_string(), file["sessionB64u"].as_str().unwrap());
    Keys {
        host,
        psk,
        slot,
        session,
    }
}

/// Runs a full handshake with the file's fixed ephemerals; returns (guest, host) ciphers.
fn handshake(file: &Value) -> (Cipher, Cipher) {
    let k = keys(file);
    assert_eq!(
        hex::encode(prologue(&k.slot, &k.session)),
        file["prologueHex"].as_str().unwrap()
    );
    let guest_e: [u8; 32] = array(&file["guestEphemeralPrivateHex"]);
    let host_e: [u8; 32] = array(&file["hostEphemeralPrivateHex"]);
    let (initiator, opening) =
        Initiator::start_with(k.host.public(), &k.psk, &k.slot, &k.session, Some(&guest_e))
            .unwrap();
    assert_eq!(hex::encode(opening), file["openingHex"].as_str().unwrap());
    let (slot, msg1) = parse_opening(&opening).unwrap();
    assert_eq!(slot, k.slot);
    assert_eq!(hex::encode(msg1), file["msg1Hex"].as_str().unwrap());
    let (host_cipher, reply) =
        respond_with(&k.host, &k.psk, &k.slot, &k.session, &msg1, Some(&host_e)).unwrap();
    assert_eq!(hex::encode(reply), file["replyHex"].as_str().unwrap());
    assert_eq!(hex::encode(&reply[1..]), file["msg2Hex"].as_str().unwrap());
    let guest_cipher = initiator.finish(reply[1..].try_into().unwrap()).unwrap();
    let hash = file["handshakeHashHex"].as_str().unwrap();
    assert_eq!(hex::encode(guest_cipher.handshake_hash()), hash);
    assert_eq!(hex::encode(host_cipher.handshake_hash()), hash);
    (guest_cipher, host_cipher)
}

fn check_transport_file(name: &str) {
    let file = testdata::json(name);
    let (guest, host) = handshake(&file);
    let transport = file["transport"].as_array().unwrap();
    assert!(!transport.is_empty());
    for (i, entry) in transport.iter().enumerate() {
        let (sender, receiver) = match entry["dir"].as_str().unwrap() {
            "guest->host" => (&guest, &host),
            "host->guest" => (&host, &guest),
            other => panic!("unknown direction {other}"),
        };
        let nonce = entry["nonce"].as_u64().unwrap();
        let record = record_of(&entry["record"]);
        let plain = record.encode();
        assert_eq!(
            hex::encode(&plain),
            entry["plaintextHex"].as_str().unwrap(),
            "{name} #{i}"
        );
        let ciphertext = sender.seal(nonce, &plain).unwrap();
        assert_eq!(
            hex::encode(&ciphertext),
            entry["ciphertextHex"].as_str().unwrap(),
            "{name} #{i}"
        );
        assert_eq!(
            hex::encode(frame(&ciphertext)),
            entry["frameHex"].as_str().unwrap(),
            "{name} #{i}"
        );
        let opened = receiver.open(nonce, &ciphertext).unwrap();
        assert_eq!(Record::decode(&opened).unwrap(), record, "{name} #{i}");
        // The wrong nonce does not open it.
        assert!(receiver.open(nonce + 1, &ciphertext).is_none());
    }
}

#[test]
fn control_stream() {
    check_transport_file("noise/nkpsk0-control.json");
}

#[test]
fn game_stream() {
    check_transport_file("noise/nkpsk0-game.json");
    let file = testdata::json("noise/nkpsk0-game.json");
    // The first data record carries the game's first bytes.
    assert_eq!(
        file["gameBytesGuestToHostHex"],
        file["transport"][2]["record"]["payloadHex"]
    );
}

#[test]
fn resume_stream() {
    check_transport_file("noise/nkpsk0-resume.json");
}

#[test]
fn negative_cases_are_refused() {
    let base = keys(&testdata::json("noise/nkpsk0-control.json"));
    let file = testdata::json("noise/nkpsk0-negative.json");
    for case in file["cases"].as_array().unwrap() {
        let name = case["case"].as_str().unwrap();
        if let Some(opening) = case.get("openingHex") {
            let opening = bytes(opening);
            // The host reads exactly 69 bytes.
            let Ok(head) =
                <[u8; super::noise::OPENING_LEN]>::try_from(&opening[..super::noise::OPENING_LEN])
            else {
                panic!("{name}: short opening")
            };
            let refused = match parse_opening(&head) {
                Err(_) => true,
                // Only the known slot has a key; no other key is tried.
                Ok((slot, _)) if slot != base.slot => true,
                Ok((_, msg1)) => {
                    super::noise::respond(&base.host, &base.psk, &base.slot, &base.session, &msg1)
                        .is_err()
                }
            };
            assert!(refused, "{name}");
        } else {
            let msg1: [u8; 48] = array(&case["msg1Hex"]);
            let host_e: [u8; 32] = array(&case["hostEphemeralPrivateHex"]);
            let (host_cipher, reply) = respond_with(
                &base.host,
                &base.psk,
                &base.slot,
                &base.session,
                &msg1,
                Some(&host_e),
            )
            .unwrap();
            assert_eq!(
                hex::encode(&reply[1..]),
                case["msg2Hex"].as_str().unwrap(),
                "{name}"
            );
            let frame = bytes(&case["frameHex"]);
            assert!(host_cipher.open(0, &frame[2..]).is_none(), "{name}");
        }
    }
}

#[test]
fn varints_and_plain_records() {
    let file = testdata::json("plj1/records.json");
    for case in file["varints"].as_array().unwrap() {
        let value = i32::try_from(case["value"].as_i64().unwrap()).unwrap();
        let hex = case["hex"].as_str().unwrap();
        assert_eq!(hex::encode(varint::encode(value)), hex);
        let encoded = hex::decode(hex).unwrap();
        assert_eq!(
            varint::decode(&encoded),
            varint::Decoded::Value(value, encoded.len())
        );
    }
    for case in file["records"].as_array().unwrap() {
        let record = record_of(&case["record"]);
        let hex = case["hex"].as_str().unwrap();
        assert_eq!(hex::encode(record.encode()), hex);
        assert_eq!(Record::decode(&hex::decode(hex).unwrap()).unwrap(), record);
    }
}

#[test]
fn a_long_control_message_is_split_and_joined() {
    let file = testdata::json("plj1/records.json");
    let split = &file["splitControl"];
    let mods = (0..320).map(|i| ModRef::new(format!("p{i:07}"), format!("v{i:07}")));
    let manifest = Manifest::new(
        "26.1.2",
        Loader {
            kind: "fabric".into(),
            version: "0.19.5".into(),
        },
        Some(PackRef {
            family: "projects".into(),
            id: "projects-26.1.2".into(),
            version: "2.3.0".into(),
        }),
        vec!["pvp".into(), "performance".into()],
        mods,
        0,
    );
    let json = Control::Manifest(Box::new(manifest.clone())).to_json();
    assert_eq!(json.len() as u64, split["jsonLength"].as_u64().unwrap());
    assert_eq!(
        hex::encode(Sha256::digest(&json)),
        split["jsonSha256"].as_str().unwrap()
    );
    let records = split_control(&json).unwrap();
    let pieces = split["pieces"].as_array().unwrap();
    assert_eq!(records.len(), pieces.len());
    let mut joiner = ControlJoiner::default();
    let mut joined = None;
    for (record, piece) in records.iter().zip(pieces) {
        let plain = record.encode();
        assert_eq!(plain.len() as u64, piece["length"].as_u64().unwrap());
        assert_eq!(
            hex::encode(Sha256::digest(&plain)),
            piece["sha256"].as_str().unwrap()
        );
        assert!(hex::encode(&plain).starts_with(piece["headHex"].as_str().unwrap()));
        let (last, body) = match record {
            Record::Ctrl(body) => (true, body),
            Record::CtrlPart(body) => (false, body),
            other => panic!("{other:?}"),
        };
        joined = joiner.push(last, body).unwrap();
    }
    let joined = joined.expect("the final piece completes the message");
    assert_eq!(
        Control::parse(&joined).unwrap(),
        Control::Manifest(Box::new(manifest))
    );
}

#[test]
fn invite_join() {
    let file = testdata::json("invite/join.json");
    let join = Join::parse(&file["join"]).unwrap();
    let text = join.to_json();
    assert_eq!(text.len() as u64, file["joinBytes"].as_u64().unwrap());
    assert!(
        text.starts_with(r#"{"v":1,"session":"stuiX9ATk0i2-2_h8YikXQ","key":"#),
        "{text}"
    );
    assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), file["join"]);
    assert_eq!(file["frame"]["join"], file["join"]);
    // The vector's keys are the Noise files' keys.
    let noise = keys(&testdata::json("noise/nkpsk0-control.json"));
    assert_eq!(&join.key, noise.host.public());
    assert_eq!(join.slot, noise.slot);
    assert_eq!(join.psk, noise.psk);
    assert_eq!(join.session, noise.session);
    // Secrets stay out of Debug.
    let shown = format!("{join:?}");
    assert!(
        !shown.contains("HKmlsLjSJco76g9M5OxXnFMqzzT_Ifoll1olG7xpyPA"),
        "{shown}"
    );
    assert!(!shown.contains("162.159"), "{shown}");

    for good in file["relayAddresses"]["valid"].as_array().unwrap() {
        assert!(parse_relay(good.as_str().unwrap()).is_some(), "{good}");
    }
    for bad in file["relayAddresses"]["invalid"].as_array().unwrap() {
        assert!(parse_relay(bad.as_str().unwrap()).is_none(), "{bad}");
        let mut altered = file["join"].clone();
        altered["paths"]["turn"]["relay"] = bad.clone();
        assert!(Join::parse(&altered).is_err(), "{bad}");
    }
}

#[test]
fn invites_of_other_versions_or_without_paths_are_dropped() {
    let file = testdata::json("invite/join.json");
    let mut v2 = file["join"].clone();
    v2["v"] = 2.into();
    assert_eq!(Join::parse(&v2), Err(super::JoinError::UnknownVersion));
    let mut no_paths = file["join"].clone();
    no_paths["paths"] = serde_json::json!({});
    assert_eq!(Join::parse(&no_paths), Err(super::JoinError::Malformed));
    let mut other_domain = file["join"].clone();
    other_domain["paths"]["e4mc"]["domain"] = "able-acid.eu.e4mc.link.evil.example".into();
    assert_eq!(Join::parse(&other_domain), Err(super::JoinError::Malformed));
}

#[test]
fn e4mc_domains() {
    use super::invite::is_e4mc_domain;
    let file = testdata::json("e4mc/wire.json");
    if let Some(domains) = file.get("domains") {
        for good in domains["valid"].as_array().into_iter().flatten() {
            assert!(is_e4mc_domain(good.as_str().unwrap()), "{good}");
        }
        for bad in domains["invalid"].as_array().into_iter().flatten() {
            assert!(!is_e4mc_domain(bad.as_str().unwrap()), "{bad}");
        }
    }
    assert!(is_e4mc_domain("able-acid.eu.e4mc.link"));
    assert!(!is_e4mc_domain("Able-acid.eu.e4mc.link"));
    assert!(!is_e4mc_domain("eu.e4mc.link"));
    assert!(!is_e4mc_domain("a.b.c.e4mc.link"));
}

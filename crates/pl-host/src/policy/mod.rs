//! Path policy (PROTOCOL.md §5.7): the client's timers and limits ([`Policy`]), and the state
//! machines that work through the service's `order`: a guest joining ([`guest`]) and a host
//! preparing its first path ([`host`]).
//!
//! The numbers are data: `testdata/policy/client-policy.json`, which [`Policy::default`] must
//! equal (a test compares them).

pub mod env;
#[cfg(test)]
pub(crate) mod fake;
pub mod guest;
pub mod host;
#[cfg(test)]
pub(crate) mod tests;

use serde::{Deserialize, Serialize};

/// Every client timer and limit of the protocol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Policy {
    pub v: u32,
    pub guest: GuestPolicy,
    pub host: HostPolicy,
    pub plj: PljPolicy,
    pub turn: TurnPolicy,
    pub e4mc: E4mcPolicy,
    pub quic: QuicPolicy,
    pub report: ReportPolicy,
}

/// The guest's side.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GuestPolicy {
    /// One path attempt, from its first step to `ok` on the control stream.
    pub path_attempt_ms: u64,
    /// How long the local endpoint accepts the game's connection.
    pub local_accept_window_ms: u64,
    /// How long the control stream stays without a game stream before `bye{leave}`.
    pub control_linger_ms: u64,
    /// How long a broken game stream is tried again before the guest gives up.
    pub resume_try_ms: u64,
    /// New streams a guest opens per minute per session.
    pub new_streams_per_minute: u32,
}

/// The host's side.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostPolicy {
    /// The 69-byte opening must arrive within this.
    pub opening_timeout_ms: u64,
    /// Failed openings per minute above which the host is busy.
    pub failed_openings_per_minute: u32,
    /// How long the busy window lasts.
    pub busy_ms: u64,
    /// Streams a guest keeps open (control and game); one more may be opening.
    pub max_streams_per_guest: u32,
    pub max_new_streams_per_guest_per_minute: u32,
    /// How long a broken game stream stays suspended for a resume.
    pub resume_window_ms: u64,
    /// How long an old path stays open after its last guest moved.
    pub old_path_linger_ms: u64,
    /// The guest count goes to the service at most this often.
    pub world_count_min_interval_ms: u64,
    pub max_live_invites: u32,
}

/// PLJ1's streams.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PljPolicy {
    pub max_record_plaintext: u32,
    pub max_control_message: u32,
    pub ping_after_idle_ms: u64,
    pub dead_after_silence_ms: u64,
    pub ack_every_bytes: u64,
    pub ack_delay_ms: u64,
    pub max_unacked_bytes: u64,
}

/// TURN (HOST1-R2's paths; credential renewal here).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TurnPolicy {
    pub credential_ttl_s: u64,
    pub renew_when_left_s: u64,
    pub allocation_lifetime_s: u64,
    pub refresh_when_left_s: u64,
    pub permission_refresh_s: u64,
    pub channel_refresh_s: u64,
    pub udp_head_start_ms: u64,
    pub url_preference: Vec<String>,
    /// A host with no guest on TURN releases its allocation after this.
    pub host_release_idle_ms: u64,
}

/// The e4mc relay (HOST1-R2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct E4mcPolicy {
    pub broker_url: String,
    pub fallback_relays: Vec<String>,
    pub relay_port: u16,
    pub player_port: u16,
    pub alpn: String,
    pub broker_timeout_ms: u64,
    pub quic_connect_timeout_ms: u64,
    pub domain_timeout_ms: u64,
    pub keep_alive_ms: u64,
    pub idle_timeout_ms: u64,
    pub user_agent: String,
}

/// QUIC over TURN (HOST1-R2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QuicPolicy {
    pub alpn: String,
    pub server_name: String,
    pub idle_timeout_ms: u64,
    pub keep_alive_ms: u64,
    pub max_bidi_streams_per_connection: u32,
    pub datagrams: bool,
    pub initial_mtu: u16,
}

/// Telemetry (§12.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReportPolicy {
    pub max_events_per_frame: u32,
    pub flush_after_ms: u64,
    pub usage_every_ms: u64,
    pub max_frames_per_minute: u32,
    pub ms_buckets: Vec<u32>,
    pub minute_buckets: Vec<u32>,
    pub rtt_buckets: Vec<u32>,
}

fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            v: 1,
            guest: GuestPolicy {
                path_attempt_ms: 4_000,
                local_accept_window_ms: 60_000,
                control_linger_ms: 120_000,
                resume_try_ms: 12_000,
                new_streams_per_minute: 5,
            },
            host: HostPolicy {
                opening_timeout_ms: 5_000,
                failed_openings_per_minute: 20,
                busy_ms: 60_000,
                max_streams_per_guest: 2,
                max_new_streams_per_guest_per_minute: 5,
                resume_window_ms: 15_000,
                old_path_linger_ms: 30_000,
                world_count_min_interval_ms: 10_000,
                max_live_invites: 20,
            },
            plj: PljPolicy {
                max_record_plaintext: 16_384,
                max_control_message: 65_536,
                ping_after_idle_ms: 1_000,
                dead_after_silence_ms: 3_500,
                ack_every_bytes: 65_536,
                ack_delay_ms: 250,
                max_unacked_bytes: 8 * 1024 * 1024,
            },
            turn: TurnPolicy {
                credential_ttl_s: 7_200,
                renew_when_left_s: 1_200,
                allocation_lifetime_s: 600,
                refresh_when_left_s: 120,
                permission_refresh_s: 240,
                channel_refresh_s: 480,
                udp_head_start_ms: 1_000,
                url_preference: strings(&[
                    "turn:*:3478?transport=udp",
                    "turn:*:443?transport=udp",
                    "turns:*:443?transport=tcp",
                    "turn:*:80?transport=tcp",
                    "turns:*:5349?transport=tcp",
                    "turn:*:3478?transport=tcp",
                ]),
                host_release_idle_ms: 300_000,
            },
            e4mc: E4mcPolicy {
                broker_url: "https://broker.e4mc.link/getBestRelay".to_owned(),
                fallback_relays: strings(&["eu.e4mc.link", "de.e4mc.link", "uk.e4mc.link"]),
                relay_port: 25_575,
                player_port: 25_565,
                alpn: "quiclime".to_owned(),
                broker_timeout_ms: 1_500,
                quic_connect_timeout_ms: 2_500,
                domain_timeout_ms: 2_000,
                keep_alive_ms: 5_000,
                idle_timeout_ms: 15_000,
                user_agent: "ProjectsLauncher/<version> (+https://projects.gg)".to_owned(),
            },
            quic: QuicPolicy {
                alpn: "plj1".to_owned(),
                server_name: "plj.invalid".to_owned(),
                idle_timeout_ms: 10_000,
                keep_alive_ms: 2_000,
                max_bidi_streams_per_connection: 4,
                datagrams: false,
                initial_mtu: 1_200,
            },
            report: ReportPolicy {
                max_events_per_frame: 20,
                flush_after_ms: 60_000,
                usage_every_ms: 300_000,
                max_frames_per_minute: 20,
                ms_buckets: crate::report::MS_BUCKETS.to_vec(),
                minute_buckets: crate::report::MINUTE_BUCKETS.to_vec(),
                rtt_buckets: crate::report::RTT_BUCKETS.to_vec(),
            },
        }
    }
}

#[cfg(test)]
mod numbers {
    use std::time::Duration;

    use super::*;
    use crate::testdata;

    #[test]
    fn default_equals_the_data() {
        let data: Policy =
            serde_json::from_value(testdata::json("policy/client-policy.json")).unwrap();
        assert_eq!(Policy::default(), data);
        assert_eq!(
            serde_json::to_value(Policy::default()).unwrap(),
            testdata::json("policy/client-policy.json")
        );
    }

    #[test]
    fn the_crate_constants_follow_the_policy() {
        let policy = Policy::default();
        assert_eq!(
            policy.plj.max_record_plaintext as usize,
            crate::tunnel::record::MAX_RECORD
        );
        assert_eq!(
            policy.plj.max_control_message as usize,
            crate::tunnel::record::MAX_CONTROL_MESSAGE
        );
        assert_eq!(
            policy.report.max_events_per_frame as usize,
            crate::report::MAX_EVENTS
        );
        assert_eq!(
            policy.report.max_frames_per_minute as usize,
            crate::report::batch::MAX_FRAMES_PER_MINUTE
        );
        assert_eq!(
            Duration::from_millis(policy.report.flush_after_ms),
            crate::report::batch::FLUSH_AFTER
        );
    }
}

//! The rules the desktop AFK and the phone's share (PHONE-AFK): `tests/shared/afk-rules.json`
//! holds their cases, this test checks them against `restart.rs`, `pacing.rs` and `console.rs`, and
//! the phone's JVM tests (projects-mobile `gg.projects.mobile.afk.core`) read their copy of the same
//! file. A rule changed here without the file fails this test; the file changed without the phone's
//! copy fails the phone's.

use std::time::Duration;

use pl_afk::bridge::DisconnectReason;
use pl_afk::{
    AutoRelogOptions, ConnectPacer, ConsoleLineKind, ExitDecision, Failure, FailureReason,
    KickCause, LineFilter, RestartSchedule, connect_destination, decide_failure,
    is_sensitive_command, kick_cause,
};
use serde_json::Value;

fn rules() -> Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/shared/afk-rules.json");
    let text = std::fs::read_to_string(path).expect("afk-rules.json");
    serde_json::from_str(&text).expect("afk-rules.json is JSON")
}

fn cases<'a>(rules: &'a Value, key: &str) -> &'a [Value] {
    rules[key]
        .as_array()
        .unwrap_or_else(|| panic!("{key} is a list"))
}

fn f64_of(v: &Value) -> f64 {
    v.as_f64().expect("a number")
}

fn u32_of(v: &Value) -> u32 {
    u32::try_from(v.as_u64().expect("a whole number")).expect("fits u32")
}

fn str_of(v: &Value) -> &str {
    v.as_str().expect("a string")
}

fn relog(min: f64, max: f64) -> AutoRelogOptions {
    AutoRelogOptions {
        delay_min: min,
        delay_max: max,
        ..AutoRelogOptions::default()
    }
}

fn cause_of(name: &Value) -> Option<KickCause> {
    if name.is_null() {
        return None;
    }
    Some(serde_json::from_value(name.clone()).expect("a kick cause"))
}

fn reason_of(name: &str) -> Option<DisconnectReason> {
    match name {
        "inGameKick" => Some(DisconnectReason::InGameKick),
        "loginRejected" => Some(DisconnectReason::LoginRejected),
        "connectionLost" => Some(DisconnectReason::ConnectionLost),
        "unsupportedVersion" => None,
        other => panic!("unknown reason {other}"),
    }
}

#[test]
fn restart_delays_back_off_alike() {
    let rules = rules();
    let schedule = RestartSchedule::default();
    for case in cases(&rules, "delays") {
        let relog = relog(f64_of(&case["min"]), f64_of(&case["max"]));
        for (i, want) in cases(case, "seconds").iter().enumerate() {
            let attempt = u32::try_from(i + 1).expect("few");
            let got = schedule.delay(&relog, attempt).as_secs_f64();
            assert!(
                (got - f64_of(want)).abs() < 1e-9,
                "{case} attempt {attempt}: {got}"
            );
        }
    }
    for case in cases(&rules, "delayAt") {
        let relog = relog(f64_of(&case["min"]), f64_of(&case["max"]));
        let got = schedule
            .delay(&relog, u32_of(&case["attempt"]))
            .as_secs_f64();
        assert!(
            (got - f64_of(&case["seconds"])).abs() < 1e-9,
            "{case}: {got}"
        );
    }
    for case in cases(&rules, "spread") {
        let relog = relog(f64_of(&case["min"]), f64_of(&case["max"]));
        let got = schedule
            .spread_delay(&relog, u32_of(&case["attempt"]), f64_of(&case["spread"]))
            .as_millis();
        assert_eq!(
            got,
            u128::from(case["millis"].as_u64().expect("ms")),
            "{case}"
        );
    }
    for case in cases(&rules, "lasting") {
        let in_game = Duration::from_secs(case["inGameSeconds"].as_u64().expect("s"));
        assert_eq!(
            schedule.lasting(in_game, cause_of(&case["cause"])),
            case["lasting"].as_bool().expect("bool"),
            "{case}"
        );
    }
}

#[test]
fn kick_messages_name_the_same_causes() {
    let rules = rules();
    for case in cases(&rules, "kickCauses") {
        let failure = Failure::new(
            reason_of(str_of(&case["reason"])),
            Some(str_of(&case["message"])),
            false,
        );
        assert_eq!(
            kick_cause(failure.reason.as_ref(), failure.message.as_deref()),
            cause_of(&case["cause"]),
            "{case}"
        );
    }
}

#[test]
fn drops_end_in_the_same_decisions() {
    let rules = rules();
    // The phone's fixed settings: the desktop's defaults (on, 5-10 s, unlimited, any kick message).
    let relog = AutoRelogOptions::default();
    for case in cases(&rules, "decisions") {
        let name = str_of(&case["reason"]);
        let failure = Failure::new(
            reason_of(name),
            Some(str_of(&case["message"])),
            name == "unsupportedVersion",
        );
        let decision = decide_failure(
            &relog,
            failure.reason.as_ref(),
            failure.to_reason(),
            u32_of(&case["attempts"]),
            u32_of(&case["repeats"]),
        );
        let got = match decision {
            ExitDecision::Restart { attempt, .. } => format!("restart {attempt}"),
            ExitDecision::Fail(FailureReason::Refused { cause, .. }) => {
                format!(
                    "refused {}",
                    serde_json::to_value(cause)
                        .expect("cause")
                        .as_str()
                        .expect("name")
                )
            }
            ExitDecision::Fail(FailureReason::UnsupportedVersion { .. }) => {
                "fail unsupportedVersion".to_owned()
            }
            other => format!("{other:?}"),
        };
        assert_eq!(got, str_of(&case["expect"]), "{case}");
    }
}

#[test]
fn connections_are_paced_alike() {
    let rules = rules();
    for case in cases(&rules, "pacing") {
        let pacer = ConnectPacer::default();
        let start = tokio::time::Instant::now();
        let spacing = Duration::from_millis(case["spacingMs"].as_u64().expect("ms"));
        for step in cases(case, "steps") {
            let at = start + Duration::from_millis(step["atMs"].as_u64().expect("ms"));
            let slot = pacer.reserve(str_of(&step["destination"]), spacing, at);
            assert_eq!(
                slot.duration_since(start).as_millis(),
                u128::from(step["slotMs"].as_u64().expect("ms")),
                "{step}"
            );
        }
    }
    for case in cases(&rules, "destinations") {
        assert_eq!(
            connect_destination(str_of(&case["host"])),
            str_of(&case["destination"]),
            "{case}"
        );
    }
}

#[test]
fn secrets_are_masked_alike() {
    let rules = rules();
    for case in cases(&rules, "sensitiveCommands") {
        assert_eq!(
            is_sensitive_command(str_of(&case["command"])),
            case["sensitive"].as_bool().expect("bool"),
            "{case}"
        );
    }
    for case in cases(&rules, "masking") {
        let commands: Vec<&str> = cases(case, "loginCommands").iter().map(str_of).collect();
        let mut filter = LineFilter::new(&commands);
        for typed in cases(case, "userCommands") {
            filter.add_user_command(str_of(typed));
        }
        for line in cases(case, "lines") {
            let raw = str_of(&line["raw"]);
            assert_eq!(filter.mask(raw), str_of(&line["masked"]), "{line}");
            let kind = match filter.classify(raw) {
                ConsoleLineKind::Normal => "normal",
                ConsoleLineKind::Script => "script",
                ConsoleLineKind::Sensitive => "sensitive",
            };
            assert_eq!(kind, str_of(&line["kind"]), "{line}");
        }
    }
}

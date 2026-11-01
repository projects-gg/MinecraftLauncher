use jiff::civil::{Date, DateTime, date};
use jiff::tz::{TimeZone, offset};

use super::*;
use crate::model::SeasonRule;

/// A moment in Turkey (UTC+3 all year).
fn at(text: &str) -> Zoned {
    let local: DateTime = text.parse().unwrap();
    local.to_zoned(TimeZone::fixed(offset(3))).unwrap()
}

/// The moment a local day begins in Turkey.
fn midnight_of(day: &str) -> Timestamp {
    at(&format!("{day}T00:00")).timestamp()
}

fn calendar() -> SeasonCalendar {
    SeasonCalendar::default()
}

fn themed(calendar: SeasonCalendar) -> Theme {
    Theme {
        calendar: Some(calendar),
        ..Theme::default()
    }
}

fn look(calendar: &SeasonCalendar, moment: &str) -> Season {
    calendar.season_at(&at(moment)).season
}

fn range(from: &str, to: &str) -> SeasonRule {
    SeasonRule {
        from: Some(from.into()),
        to: Some(to.into()),
        ..SeasonRule::default()
    }
}

#[test]
fn without_a_calendar_the_season_is_a_fixed_pick() {
    let theme = Theme {
        season: Season::Winter,
        ..Theme::default()
    };
    let now = theme.season_at(&at("2026-07-01T12:00"));
    assert_eq!(now.season, Season::Winter);
    assert_eq!(now.until, None);
    let outlook = theme.season_outlook(&at("2026-07-01T12:00"));
    assert!(!outlook.automatic && outlook.picked && outlook.next.is_none());
}

#[test]
fn automatic_dates_and_the_next_change() {
    let calendar = calendar();
    // Halloween: 20 October - 2 November.
    let now = calendar.season_at(&at("2026-10-25T15:00"));
    assert_eq!(now.season, Season::Halloween);
    assert_eq!(now.until, Some(midnight_of("2026-11-03")));
    assert_eq!(look(&calendar, "2026-10-19T23:59"), Season::None);
    assert_eq!(look(&calendar, "2026-10-20T00:00"), Season::Halloween);
    // Nothing until winter.
    let quiet = calendar.season_at(&at("2026-11-05T09:00"));
    assert_eq!(quiet.season, Season::None);
    assert_eq!(quiet.until, Some(midnight_of("2026-12-15")));
    // Winter crosses the new year: 15 December - 6 January.
    let winter = calendar.season_at(&at("2027-01-03T20:00"));
    assert_eq!(winter.season, Season::Winter);
    assert_eq!(winter.until, Some(midnight_of("2027-01-07")));
    assert_eq!(look(&calendar, "2026-12-31T23:00"), Season::Winter);
    assert_eq!(look(&calendar, "2027-01-07T00:00"), Season::None);
    // Ramazan Bayramı 2026 (20-22 March): from two days before the eve through its last day.
    assert_eq!(look(&calendar, "2026-03-16T12:00"), Season::None);
    let feast = calendar.season_at(&at("2026-03-17T00:00"));
    assert_eq!(feast.season, Season::Holiday);
    assert_eq!(feast.until, Some(midnight_of("2026-03-23")));
    // Kurban Bayramı 2026 (27-30 May).
    assert_eq!(look(&calendar, "2026-05-24T08:00"), Season::Holiday);
    assert_eq!(look(&calendar, "2026-05-30T23:00"), Season::Holiday);
    assert_eq!(look(&calendar, "2026-05-31T00:00"), Season::None);
    // `event` has no automatic dates.
    assert!(
        calendar
            .spans(2026..=2026)
            .iter()
            .all(|span| span.occasion != Occasion::Event)
    );
}

#[test]
fn a_studio_range_repeats_every_year() {
    let calendar = SeasonCalendar {
        halloween: range("10-10", "11-09"),
        ..calendar()
    };
    // Years whose feasts stay clear of October and early November.
    for year in [2026, 2031, 2036] {
        assert_eq!(
            look(&calendar, &format!("{year}-10-09T23:00")),
            Season::None
        );
        assert_eq!(
            look(&calendar, &format!("{year}-10-10T00:00")),
            Season::Halloween
        );
        assert_eq!(
            look(&calendar, &format!("{year}-11-09T23:59")),
            Season::Halloween
        );
        assert_eq!(
            look(&calendar, &format!("{year}-11-10T00:00")),
            Season::None
        );
    }
    // A one-day range and one that crosses the new year.
    let calendar = SeasonCalendar {
        event: range("06-15", "06-15"),
        winter: range("12-01", "02-28"),
        ..calendar
    };
    assert_eq!(look(&calendar, "2027-06-15T12:00"), Season::Event);
    assert_eq!(look(&calendar, "2027-06-16T00:00"), Season::None);
    assert_eq!(look(&calendar, "2026-12-01T00:00"), Season::Winter);
    assert_eq!(look(&calendar, "2027-02-28T12:00"), Season::Winter);
    assert_eq!(look(&calendar, "2027-03-01T00:00"), Season::None);
}

#[test]
fn leap_day_ranges_fall_on_the_28th_in_other_years() {
    let calendar = SeasonCalendar {
        event: range("02-29", "03-02"),
        // Ramazan Bayramı 2028 begins on 26 February.
        ramadan_feast: SeasonRule {
            off: true,
            ..SeasonRule::default()
        },
        ..calendar()
    };
    assert_eq!(look(&calendar, "2027-02-28T12:00"), Season::Event);
    assert_eq!(look(&calendar, "2028-02-28T12:00"), Season::None);
    assert_eq!(look(&calendar, "2028-02-29T12:00"), Season::Event);
}

#[test]
fn off_occasions_never_show() {
    let calendar = SeasonCalendar {
        winter: SeasonRule {
            off: true,
            ..SeasonRule::default()
        },
        ramadan_feast: SeasonRule {
            off: true,
            ..SeasonRule::default()
        },
        ..calendar()
    };
    assert_eq!(look(&calendar, "2026-12-31T12:00"), Season::None);
    assert_eq!(look(&calendar, "2026-03-21T12:00"), Season::None);
    assert_eq!(look(&calendar, "2026-05-28T12:00"), Season::Holiday);
    // Off with a range kept: still off.
    let calendar = SeasonCalendar {
        halloween: SeasonRule {
            off: true,
            ..range("10-01", "10-31")
        },
        ..calendar
    };
    assert_eq!(look(&calendar, "2026-10-25T12:00"), Season::None);
}

#[test]
fn a_feast_wins_over_a_fixed_range_and_the_range_comes_back() {
    // Ramazan Bayramı 2033 begins on 23 December, inside winter (15 December - 6 January).
    let calendar = calendar();
    assert_eq!(look(&calendar, "2033-12-19T12:00"), Season::Winter);
    let feast = calendar.season_at(&at("2033-12-20T00:00"));
    assert_eq!(feast.season, Season::Holiday);
    assert_eq!(feast.until, Some(midnight_of("2033-12-26")));
    assert_eq!(look(&calendar, "2033-12-25T23:00"), Season::Holiday);
    assert_eq!(look(&calendar, "2033-12-26T00:00"), Season::Winter);
    let before = calendar.season_at(&at("2033-12-19T12:00"));
    assert_eq!(before.until, Some(midnight_of("2033-12-20")));
}

#[test]
fn the_range_that_started_last_wins_then_event_halloween_winter() {
    let calendar = SeasonCalendar {
        event: range("10-25", "10-27"),
        winter: range("11-01", "02-28"),
        ..calendar()
    };
    assert_eq!(look(&calendar, "2026-10-24T12:00"), Season::Halloween);
    assert_eq!(look(&calendar, "2026-10-26T12:00"), Season::Event);
    assert_eq!(look(&calendar, "2026-10-28T12:00"), Season::Halloween);
    assert_eq!(look(&calendar, "2026-11-01T12:00"), Season::Winter);
    // Same first day: event, then Halloween, then winter.
    let tied = SeasonCalendar {
        event: range("10-20", "10-22"),
        winter: range("10-20", "01-06"),
        ..calendar
    };
    assert_eq!(look(&tied, "2026-10-21T12:00"), Season::Event);
    assert_eq!(look(&tied, "2026-10-23T12:00"), Season::Halloween);
    assert_eq!(look(&tied, "2026-11-03T12:00"), Season::Winter);
}

#[test]
fn a_manual_pick_wins_while_it_runs() {
    let mut calendar = SeasonCalendar {
        pick: Some(Season::Event),
        pick_from: Some(midnight_of("2026-11-10")),
        pick_until: Some(midnight_of("2026-11-20")),
        ..calendar()
    };
    let before = calendar.season_at(&at("2026-11-05T12:00"));
    assert_eq!(before.season, Season::None);
    assert_eq!(before.until, Some(midnight_of("2026-11-10")));
    let during = calendar.season_at(&at("2026-11-12T12:00"));
    assert_eq!(during.season, Season::Event);
    assert_eq!(during.until, Some(midnight_of("2026-11-20")));
    let after = calendar.season_at(&at("2026-11-21T12:00"));
    assert_eq!(after.season, Season::None);
    assert_eq!(after.until, Some(midnight_of("2026-12-15")));
    // Inside Halloween the pick still wins, and Halloween is back when it ends.
    calendar.pick_from = Some(midnight_of("2026-10-22"));
    calendar.pick_until = Some(midnight_of("2026-10-24"));
    assert_eq!(look(&calendar, "2026-10-23T12:00"), Season::Event);
    assert_eq!(look(&calendar, "2026-10-24T12:00"), Season::Halloween);
    // "none" with no end: no season whatever the dates say.
    let off = SeasonCalendar {
        pick: Some(Season::None),
        ..SeasonCalendar::default()
    };
    let now = off.season_at(&at("2026-10-25T12:00"));
    assert_eq!(
        now,
        SeasonNow {
            season: Season::None,
            until: None
        }
    );
    // A window that ends before it starts is ignored.
    let broken = SeasonCalendar {
        pick: Some(Season::Winter),
        pick_from: Some(midnight_of("2026-11-20")),
        pick_until: Some(midnight_of("2026-11-10")),
        ..SeasonCalendar::default()
    };
    assert_eq!(look(&broken, "2026-11-15T12:00"), Season::None);
}

#[test]
fn a_newer_studios_season_shows_the_default_look() {
    let calendar: SeasonCalendar = serde_json::from_str(r#"{"pick":"spring"}"#).unwrap();
    assert_eq!(look(&calendar, "2026-10-25T12:00"), Season::Unknown);
    let theme: Theme =
        serde_json::from_str(r#"{"season":"aurora","calendar":{"halloween":{"off":true}}}"#)
            .unwrap();
    assert_eq!(theme.season, Season::Unknown);
    assert_eq!(
        theme.season_at(&at("2026-10-25T12:00")).season,
        Season::None
    );
}

#[test]
fn broken_rules_fall_back_to_the_automatic_dates() {
    let calendar = SeasonCalendar {
        halloween: range("13-01", "11-09"),
        ramadan_feast: SeasonRule {
            days_before: Some(31),
            days_after: Some(1),
            ..SeasonRule::default()
        },
        ..calendar()
    };
    assert_eq!(look(&calendar, "2026-10-15T12:00"), Season::None);
    assert_eq!(look(&calendar, "2026-10-21T12:00"), Season::Halloween);
    // 31 days is too many: the automatic 3 before; the 1 after is kept.
    assert_eq!(look(&calendar, "2026-03-16T12:00"), Season::None);
    assert_eq!(look(&calendar, "2026-03-17T12:00"), Season::Holiday);
    assert_eq!(look(&calendar, "2026-03-23T12:00"), Season::Holiday);
    assert_eq!(look(&calendar, "2026-03-24T12:00"), Season::None);
}

#[test]
fn feast_margins_move_both_ends() {
    let calendar = SeasonCalendar {
        sacrifice_feast: SeasonRule {
            days_before: Some(0),
            days_after: Some(2),
            ..SeasonRule::default()
        },
        ..calendar()
    };
    // Kurban Bayramı 2027: 16-19 May.
    let spans = calendar.spans(2027..=2027);
    let span = spans
        .iter()
        .find(|span| span.occasion == Occasion::SacrificeFeast)
        .unwrap();
    assert_eq!(
        (span.first, span.last),
        (date(2027, 5, 16), date(2027, 5, 21))
    );
}

#[test]
fn studio_sees_now_next_and_the_feasts() {
    let theme = themed(calendar());
    let outlook = theme.season_outlook(&at("2026-11-01T10:00"));
    assert_eq!(outlook.season, Season::Halloween);
    assert!(outlook.automatic && !outlook.picked);
    assert_eq!(outlook.until, Some(midnight_of("2026-11-03")));
    let current = outlook.current.unwrap();
    assert_eq!(
        (current.first, current.last),
        (date(2026, 10, 20), date(2026, 11, 2))
    );
    let next = outlook.next.unwrap();
    assert_eq!(next.occasion, Occasion::Winter);
    assert_eq!(
        (next.first, next.last),
        (date(2026, 12, 15), date(2027, 1, 6))
    );
    let feasts: Vec<(Occasion, Date, Date)> = outlook
        .feasts
        .iter()
        .map(|f| (f.occasion, f.first, f.last))
        .collect();
    assert_eq!(
        feasts,
        [
            (Occasion::RamadanFeast, date(2026, 3, 20), date(2026, 3, 22)),
            (
                Occasion::SacrificeFeast,
                date(2026, 5, 27),
                date(2026, 5, 30)
            ),
            (Occasion::RamadanFeast, date(2027, 3, 9), date(2027, 3, 11)),
            (
                Occasion::SacrificeFeast,
                date(2027, 5, 16),
                date(2027, 5, 19)
            ),
        ]
    );
    // While a pick runs the outlook says so and still knows the dates.
    let picked = themed(SeasonCalendar {
        pick: Some(Season::Event),
        ..calendar()
    });
    let outlook = picked.season_outlook(&at("2026-11-01T10:00"));
    assert_eq!(outlook.season, Season::Event);
    assert!(outlook.picked);
    assert_eq!(outlook.until, None);
    assert_eq!(
        outlook.current.map(|s| s.occasion),
        Some(Occasion::Halloween)
    );
}

#[test]
fn day_numbers_round_trip() {
    for (text, number) in [("2000-01-01", 2_451_545), ("1970-01-01", 2_440_588)] {
        let day: Date = text.parse().unwrap();
        assert_eq!(day_of_date(day), number);
        assert_eq!(date_of_day(number), day);
    }
    let mut day = date(1999, 12, 25);
    for _ in 0..800 {
        let next = add_days(day, 1);
        assert_eq!(next, day.tomorrow().unwrap());
        day = next;
    }
}

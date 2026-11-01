//! Season themes that start and end by themselves (SEASON-AUTO): `theme.calendar` in the launcher
//! config. Each occasion follows its automatic dates, a yearly range Studio set, or is off; a
//! manual pick shows one look instead of the dates (for a while, or until changed). Launchers
//! resolve it on the player's local date ([`crate::season`]). See `docs/content-format.md`,
//! "Season calendar".

use std::fmt;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use super::Season;

/// Most days a feast's theme may start before its first day or end after its last one.
pub const MAX_FEAST_MARGIN_DAYS: u8 = 30;

/// When the season looks come by themselves. Missing from a config: `theme.season` is a fixed
/// pick, as before SEASON-AUTO. Every field is optional; rules left at their automatic dates are
/// left out, so a calendar that only switches the dates on is `{}`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct SeasonCalendar {
    /// A look shown instead of the dates while it runs (`none` shows no season). Missing: the
    /// dates decide.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pick: Option<Season>,
    /// The pick starts at this moment (missing: at once).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pick_from: Option<Timestamp>,
    /// The pick ends at this moment and the dates decide again (missing: until it is changed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pick_until: Option<Timestamp>,
    #[serde(skip_serializing_if = "SeasonRule::is_automatic")]
    pub halloween: SeasonRule,
    #[serde(skip_serializing_if = "SeasonRule::is_automatic")]
    pub winter: SeasonRule,
    #[serde(skip_serializing_if = "SeasonRule::is_automatic")]
    pub ramadan_feast: SeasonRule,
    #[serde(skip_serializing_if = "SeasonRule::is_automatic")]
    pub sacrifice_feast: SeasonRule,
    #[serde(skip_serializing_if = "SeasonRule::is_automatic")]
    pub event: SeasonRule,
}

/// How one occasion comes each year. The default is its automatic dates.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct SeasonRule {
    /// Never shown.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub off: bool,
    /// First day of a yearly range of the player's local dates, `MM-DD` (both days included; it
    /// may cross the new year). Fixed-date occasions only; `from` and `to` come together.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// Last day of the yearly range, `MM-DD`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    /// Feasts only: the theme starts this many days before the feast's first day (automatic: 3,
    /// two days before the eve).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub days_before: Option<u8>,
    /// Feasts only: the theme ends this many days after the feast's last day (automatic: 0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub days_after: Option<u8>,
}

impl SeasonRule {
    /// Left at the occasion's automatic dates (nothing set).
    pub fn is_automatic(&self) -> bool {
        *self == Self::default()
    }

    /// The yearly range when both ends are valid `MM-DD` days.
    pub fn range(&self) -> Option<(MonthDay, MonthDay)> {
        Some((
            MonthDay::parse(self.from.as_deref()?)?,
            MonthDay::parse(self.to.as_deref()?)?,
        ))
    }
}

/// The calendar's occasions, in the order Studio lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Occasion {
    /// Cadılar Bayramı: automatically 20 October – 2 November.
    Halloween,
    /// Kış, the new year: automatically 15 December – 6 January.
    Winter,
    /// Ramazan Bayramı (1–3 Şevval, Diyanet's dates): automatically from two days before the eve
    /// through its last day.
    RamadanFeast,
    /// Kurban Bayramı (10–13 Zilhicce): as Ramazan Bayramı.
    SacrificeFeast,
    /// Etkinlik: no automatic dates, only a yearly range.
    Event,
}

impl Occasion {
    pub const ALL: [Occasion; 5] = [
        Occasion::Halloween,
        Occasion::Winter,
        Occasion::RamadanFeast,
        Occasion::SacrificeFeast,
        Occasion::Event,
    ];

    /// The look the launcher wears during it.
    pub fn season(self) -> Season {
        match self {
            Occasion::Halloween => Season::Halloween,
            Occasion::Winter => Season::Winter,
            Occasion::RamadanFeast | Occasion::SacrificeFeast => Season::Holiday,
            Occasion::Event => Season::Event,
        }
    }

    /// A feast that follows the lunar (Hijri) calendar: its rule takes day margins, not a range.
    pub fn is_feast(self) -> bool {
        matches!(self, Occasion::RamadanFeast | Occasion::SacrificeFeast)
    }

    /// The automatic yearly range of a fixed-date occasion (`None` for feasts and `event`).
    pub fn automatic_range(self) -> Option<(MonthDay, MonthDay)> {
        match self {
            Occasion::Halloween => Some((MonthDay::new(10, 20), MonthDay::new(11, 2))),
            Occasion::Winter => Some((MonthDay::new(12, 15), MonthDay::new(1, 6))),
            _ => None,
        }
    }
}

/// Default of [`SeasonRule::days_before`] for feasts.
pub const FEAST_DAYS_BEFORE: u8 = 3;
/// Default of [`SeasonRule::days_after`] for feasts.
pub const FEAST_DAYS_AFTER: u8 = 0;

impl SeasonCalendar {
    pub fn rule(&self, occasion: Occasion) -> &SeasonRule {
        match occasion {
            Occasion::Halloween => &self.halloween,
            Occasion::Winter => &self.winter,
            Occasion::RamadanFeast => &self.ramadan_feast,
            Occasion::SacrificeFeast => &self.sacrifice_feast,
            Occasion::Event => &self.event,
        }
    }

    pub fn rule_mut(&mut self, occasion: Occasion) -> &mut SeasonRule {
        match occasion {
            Occasion::Halloween => &mut self.halloween,
            Occasion::Winter => &mut self.winter,
            Occasion::RamadanFeast => &mut self.ramadan_feast,
            Occasion::SacrificeFeast => &mut self.sacrifice_feast,
            Occasion::Event => &mut self.event,
        }
    }

    /// What readers of `theme.season` alone (launchers and Studios before SEASON-AUTO) should
    /// keep showing: the pick when it has no window (it stays until changed), else `none`.
    pub fn lasting_pick(&self) -> Season {
        match self.pick {
            Some(season) if self.pick_from.is_none() && self.pick_until.is_none() => season,
            _ => Season::None,
        }
    }

    /// What breaks the format's rules. Launchers read such a rule as its automatic dates (a bad
    /// range or margin) and ignore a pick window that ends before it starts; Studio refuses to
    /// publish them.
    pub fn problems(&self) -> Vec<SeasonProblem> {
        let mut problems = Vec::new();
        for occasion in Occasion::ALL {
            let rule = self.rule(occasion);
            let has_range = rule.from.is_some() || rule.to.is_some();
            if occasion.is_feast() {
                if has_range {
                    problems.push(SeasonProblem::RangeOnFeast(occasion));
                }
                let margins = [rule.days_before, rule.days_after];
                if margins.iter().flatten().any(|d| *d > MAX_FEAST_MARGIN_DAYS) {
                    problems.push(SeasonProblem::MarginTooLong(occasion));
                }
            } else {
                if rule.days_before.is_some() || rule.days_after.is_some() {
                    problems.push(SeasonProblem::MarginOnFixed(occasion));
                }
                if has_range && rule.range().is_none() {
                    problems.push(SeasonProblem::BadRange(occasion));
                }
            }
        }
        if let (Some(from), Some(until)) = (self.pick_from, self.pick_until)
            && until <= from
        {
            problems.push(SeasonProblem::PickEndsBeforeStart);
        }
        problems
    }
}

/// What is wrong with a season calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeasonProblem {
    /// `from`/`to` missing one end or not a real `MM-DD` day.
    BadRange(Occasion),
    /// A feast follows the lunar calendar: a fixed range does not apply to it.
    RangeOnFeast(Occasion),
    /// Day margins apply to feasts only.
    MarginOnFixed(Occasion),
    /// A margin above [`MAX_FEAST_MARGIN_DAYS`].
    MarginTooLong(Occasion),
    /// `pickUntil` is not after `pickFrom`.
    PickEndsBeforeStart,
}

impl fmt::Display for SeasonProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadRange(o) => write!(f, "{o:?}: the range is not two MM-DD days"),
            Self::RangeOnFeast(o) => write!(f, "{o:?}: a feast takes day margins, not a range"),
            Self::MarginOnFixed(o) => write!(f, "{o:?}: day margins are for feasts only"),
            Self::MarginTooLong(o) => {
                write!(f, "{o:?}: a margin is over {MAX_FEAST_MARGIN_DAYS} days")
            }
            Self::PickEndsBeforeStart => f.write_str("the pick ends before it starts"),
        }
    }
}

/// A day of the year without a year, as `MM-DD` writes it. `02-29` is allowed and falls on
/// 28 February in other years.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MonthDay {
    pub month: i8,
    pub day: i8,
}

impl MonthDay {
    pub const fn new(month: i8, day: i8) -> Self {
        Self { month, day }
    }

    /// `MM-DD` with a real day of a leap year, else `None`.
    pub fn parse(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        if bytes.len() != 5 || bytes[2] != b'-' {
            return None;
        }
        let digits = |pair: &[u8]| -> Option<i8> {
            pair.iter().try_fold(0i8, |acc, b| {
                b.is_ascii_digit().then(|| acc * 10 + (b - b'0') as i8)
            })
        };
        let month = digits(&bytes[..2])?;
        let day = digits(&bytes[3..])?;
        ((1..=12).contains(&month) && day >= 1 && day <= days_in_month(month, true))
            .then_some(Self { month, day })
    }

    /// `MM-DD`.
    pub fn text(self) -> String {
        format!("{:02}-{:02}", self.month, self.day)
    }

    /// This day in `year` (`02-29` falls on the 28th outside leap years).
    pub fn in_year(self, year: i16) -> jiff::civil::Date {
        let day = self.day.min(days_in_month(self.month, is_leap(year)));
        jiff::civil::date(year, self.month, day)
    }
}

fn is_leap(year: i16) -> bool {
    let y = i32::from(year);
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn days_in_month(month: i8, leap: bool) -> i8 {
    match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn month_days_parse_strictly() {
        assert_eq!(MonthDay::parse("10-10"), Some(MonthDay::new(10, 10)));
        assert_eq!(MonthDay::parse("02-29"), Some(MonthDay::new(2, 29)));
        for bad in [
            "2-29", "13-01", "00-10", "04-31", "02-30", "10/10", "1010", " 10-10", "aa-bb",
        ] {
            assert_eq!(MonthDay::parse(bad), None, "{bad}");
        }
        assert_eq!(
            MonthDay::new(2, 29).in_year(2027),
            jiff::civil::date(2027, 2, 28)
        );
        assert_eq!(
            MonthDay::new(2, 29).in_year(2028),
            jiff::civil::date(2028, 2, 29)
        );
        assert_eq!(MonthDay::new(1, 6).text(), "01-06");
    }

    #[test]
    fn automatic_rules_are_left_out_and_older_readers_skip_the_calendar() {
        let calendar = SeasonCalendar::default();
        assert_eq!(serde_json::to_string(&calendar).unwrap(), "{}");
        let custom = SeasonCalendar {
            halloween: SeasonRule {
                from: Some("10-10".into()),
                to: Some("11-09".into()),
                ..SeasonRule::default()
            },
            winter: SeasonRule {
                off: true,
                ..SeasonRule::default()
            },
            ..SeasonCalendar::default()
        };
        let text = serde_json::to_string(&custom).unwrap();
        assert_eq!(
            text,
            r#"{"halloween":{"from":"10-10","to":"11-09"},"winter":{"off":true}}"#
        );
        assert_eq!(
            serde_json::from_str::<SeasonCalendar>(&text).unwrap(),
            custom
        );
        // A newer Studio's occasion and fields are skipped, the rest is read.
        let newer: SeasonCalendar = serde_json::from_str(
            r#"{"spring":{"from":"04-01","to":"04-30"},"halloween":{"off":true,"glow":2},"pick":"spring"}"#,
        )
        .unwrap();
        assert!(newer.halloween.off);
        assert_eq!(newer.pick, Some(Season::Unknown));
    }

    #[test]
    fn problems_name_what_launchers_would_ignore() {
        let mut calendar = SeasonCalendar::default();
        assert!(calendar.problems().is_empty());
        calendar.halloween.from = Some("10-10".into());
        calendar.winter.from = Some("12-01".into());
        calendar.winter.to = Some("02-30".into());
        calendar.event.days_before = Some(1);
        calendar.ramadan_feast.from = Some("03-01".into());
        calendar.ramadan_feast.to = Some("03-05".into());
        calendar.sacrifice_feast.days_after = Some(31);
        calendar.pick = Some(Season::Event);
        calendar.pick_from = Some("2026-11-10T00:00:00Z".parse().unwrap());
        calendar.pick_until = Some("2026-11-10T00:00:00Z".parse().unwrap());
        assert_eq!(
            calendar.problems(),
            vec![
                SeasonProblem::BadRange(Occasion::Halloween),
                SeasonProblem::BadRange(Occasion::Winter),
                SeasonProblem::RangeOnFeast(Occasion::RamadanFeast),
                SeasonProblem::MarginTooLong(Occasion::SacrificeFeast),
                SeasonProblem::MarginOnFixed(Occasion::Event),
                SeasonProblem::PickEndsBeforeStart,
            ]
        );
        calendar.sacrifice_feast.days_after = Some(MAX_FEAST_MARGIN_DAYS);
        assert!(
            !calendar
                .problems()
                .contains(&SeasonProblem::MarginTooLong(Occasion::SacrificeFeast))
        );
    }
}

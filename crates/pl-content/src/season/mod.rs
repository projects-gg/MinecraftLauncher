//! Season themes by date (SEASON-AUTO): which look ([`Season`]) the launcher wears on the player's
//! local date, from the config's [`SeasonCalendar`]. Fixed-date occasions follow a yearly range
//! (Studio's, else their automatic one); Ramazan and Kurban Bayramı follow Diyanet's lunar
//! calendar ([`feast_first_days`]). When occasions overlap, a feast wins, then the one that
//! started last (a short range inside a long one shows during its days), then `event`, Halloween,
//! winter. A manual pick wins over all of them while it runs. See `docs/content-format.md`,
//! "Season calendar".

mod astro;
mod hijri;

use jiff::civil::Date;
use jiff::tz::TimeZone;
use jiff::{Timestamp, Zoned};
use serde::Serialize;

pub use hijri::{Feast, feast_first_days};

use crate::model::{
    FEAST_DAYS_AFTER, FEAST_DAYS_BEFORE, MAX_FEAST_MARGIN_DAYS, Occasion, Season, SeasonCalendar,
    Theme,
};

/// How far ahead the next change is looked for.
const LOOKAHEAD_DAYS: usize = 400;

/// One run of an occasion's look, both days included (the player's local dates).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeasonSpan {
    pub occasion: Occasion,
    pub season: Season,
    pub first: Date,
    pub last: Date,
}

/// The look at one moment and when it may change next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeasonNow {
    pub season: Season,
    /// The next moment the look may change (a local midnight, or the manual pick's start or end);
    /// `None` while nothing ahead changes it.
    pub until: Option<Timestamp>,
}

/// A feast's days in one year (the eve not counted).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeastDays {
    pub occasion: Occasion,
    pub first: Date,
    pub last: Date,
}

/// What Studio shows beside the calendar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeasonOutlook {
    /// The look now.
    pub season: Season,
    /// It comes from the manual pick, not the dates.
    pub picked: bool,
    /// When the look may change next.
    pub until: Option<Timestamp>,
    /// The dates' run today (also while a pick covers it).
    pub current: Option<SeasonSpan>,
    /// The next run by the dates that starts after today.
    pub next: Option<SeasonSpan>,
    /// Both feasts' days this year and next year, in order.
    pub feasts: Vec<FeastDays>,
    /// Without a calendar the config's `season` is a fixed pick.
    pub automatic: bool,
}

impl Theme {
    /// The look at `now`: the calendar's when there is one, else the fixed `season`.
    pub fn season_at(&self, now: &Zoned) -> SeasonNow {
        match &self.calendar {
            Some(calendar) => calendar.season_at(now),
            None => SeasonNow {
                season: self.season,
                until: None,
            },
        }
    }

    /// Studio's outlook at `now`.
    pub fn season_outlook(&self, now: &Zoned) -> SeasonOutlook {
        let current = self.season_at(now);
        let year = now.date().year();
        let mut feasts: Vec<FeastDays> = [year, year.saturating_add(1)]
            .into_iter()
            .flat_map(|year| {
                [Feast::Ramadan, Feast::Sacrifice]
                    .into_iter()
                    .flat_map(move |feast| {
                        feast_first_days(feast, year)
                            .into_iter()
                            .map(move |first| FeastDays {
                                occasion: feast.occasion(),
                                first,
                                last: add_days(first, feast.days() - 1),
                            })
                    })
            })
            .collect();
        feasts.sort_by_key(|f| f.first);
        let Some(calendar) = &self.calendar else {
            return SeasonOutlook {
                season: current.season,
                picked: true,
                until: None,
                current: None,
                next: None,
                feasts,
                automatic: false,
            };
        };
        let today = now.date();
        let spans = calendar.spans(year.saturating_sub(1)..=year.saturating_add(2));
        let running = look_on(&spans, today).copied();
        // The next run to start; on a shared first day the one `look_on` would show.
        let next = spans
            .iter()
            .filter(|span| span.first > today && Some(**span) != running)
            .min_by_key(|span| {
                let wins = (span.occasion.is_feast(), rank(span));
                (span.first, std::cmp::Reverse(wins))
            })
            .copied();
        SeasonOutlook {
            season: current.season,
            picked: calendar.pick_running(now.timestamp()).is_some(),
            until: current.until,
            current: running,
            next,
            feasts,
            automatic: true,
        }
    }
}

impl SeasonCalendar {
    /// The look at `now` and when it may change.
    pub fn season_at(&self, now: &Zoned) -> SeasonNow {
        let instant = now.timestamp();
        if let Some(season) = self.pick_running(instant) {
            return SeasonNow {
                season,
                until: self.pick_until,
            };
        }
        let (season, change) = self.by_date(now.date());
        let date_change = change.and_then(|day| midnight(day, now.time_zone()));
        let pick_change = self
            .pick_window()
            .and_then(|(from, until)| [from, until].into_iter().flatten().find(|t| *t > instant));
        SeasonNow {
            season,
            until: [date_change, pick_change].into_iter().flatten().min(),
        }
    }

    /// The runs of every occasion that is on, by their start year.
    pub fn spans(&self, years: std::ops::RangeInclusive<i16>) -> Vec<SeasonSpan> {
        let mut spans = Vec::new();
        for occasion in Occasion::ALL {
            let rule = self.rule(occasion);
            if rule.off {
                continue;
            }
            if let Some(feast) = Feast::of(occasion) {
                let margin = |value: Option<u8>, default: u8| {
                    i64::from(
                        value
                            .filter(|d| *d <= MAX_FEAST_MARGIN_DAYS)
                            .unwrap_or(default),
                    )
                };
                let before = margin(rule.days_before, FEAST_DAYS_BEFORE);
                let after = margin(rule.days_after, FEAST_DAYS_AFTER);
                for year in years.clone() {
                    for first in feast_first_days(feast, year) {
                        spans.push(SeasonSpan {
                            occasion,
                            season: occasion.season(),
                            first: add_days(first, -before),
                            last: add_days(first, feast.days() - 1 + after),
                        });
                    }
                }
            } else if let Some((from, to)) = rule.range().or_else(|| occasion.automatic_range()) {
                for year in years.clone() {
                    let end_year = if to < from {
                        year.saturating_add(1)
                    } else {
                        year
                    };
                    spans.push(SeasonSpan {
                        occasion,
                        season: occasion.season(),
                        first: from.in_year(year),
                        last: to.in_year(end_year),
                    });
                }
            }
        }
        spans
    }

    /// The manual pick when it covers `instant`.
    fn pick_running(&self, instant: Timestamp) -> Option<Season> {
        let (from, until) = self.pick_window()?;
        (from.is_none_or(|from| from <= instant) && until.is_none_or(|until| instant < until))
            .then_some(self.pick)
            .flatten()
    }

    /// The pick's window, `None` without a pick or with one that ends before it starts.
    fn pick_window(&self) -> Option<(Option<Timestamp>, Option<Timestamp>)> {
        self.pick?;
        match (self.pick_from, self.pick_until) {
            (Some(from), Some(until)) if until <= from => None,
            window => Some(window),
        }
    }

    /// The look by the dates on `today` and the first later day it changes.
    fn by_date(&self, today: Date) -> (Season, Option<Date>) {
        let year = today.year();
        let spans = self.spans(year.saturating_sub(1)..=year.saturating_add(2));
        let look = |day: Date| look_on(&spans, day).map_or(Season::None, |span| span.season);
        let now = look(today);
        let mut day = today;
        for _ in 0..LOOKAHEAD_DAYS {
            day = add_days(day, 1);
            if look(day) != now {
                return (now, Some(day));
            }
        }
        (now, None)
    }
}

/// The run that sets the look on `day`: a feast first, then the one that started last, then by
/// occasion.
fn look_on(spans: &[SeasonSpan], day: Date) -> Option<&SeasonSpan> {
    spans
        .iter()
        .filter(|span| span.first <= day && day <= span.last)
        .max_by_key(|span| (span.occasion.is_feast(), span.first, rank(span)))
}

/// Ties between runs that start on the same day: `event`, then Halloween, then winter.
fn rank(span: &SeasonSpan) -> u8 {
    match span.occasion {
        Occasion::Event => 3,
        Occasion::Halloween => 2,
        Occasion::Winter => 1,
        Occasion::RamadanFeast | Occasion::SacrificeFeast => 0,
    }
}

/// The moment `day` begins in `tz`.
fn midnight(day: Date, tz: &TimeZone) -> Option<Timestamp> {
    day.to_zoned(tz.clone()).ok().map(|zoned| zoned.timestamp())
}

/// Julian day number of a date (proleptic Gregorian).
pub(crate) fn day_of_date(date: Date) -> i64 {
    let (y, m, d) = (
        i64::from(date.year()),
        i64::from(date.month()),
        i64::from(date.day()),
    );
    let a = (14 - m) / 12;
    let y = y + 4800 - a;
    let m = m + 12 * a - 3;
    d + (153 * m + 2) / 5 + 365 * y + y / 4 - y / 100 + y / 400 - 32045
}

/// The date of a Julian day number.
pub(crate) fn date_of_day(day: i64) -> Date {
    let a = day + 32044;
    let b = (4 * a + 3) / 146_097;
    let c = a - 146_097 * b / 4;
    let d = (4 * c + 3) / 1461;
    let e = c - 1461 * d / 4;
    let m = (5 * e + 2) / 153;
    let day = e - (153 * m + 2) / 5 + 1;
    let month = m + 3 - 12 * (m / 10);
    let year = 100 * b + d - 4800 + m / 10;
    Date::new(year as i16, month as i8, day as i8).unwrap_or(Date::MAX)
}

fn add_days(date: Date, days: i64) -> Date {
    date_of_day(day_of_date(date) + days)
}

#[cfg(test)]
mod tests;

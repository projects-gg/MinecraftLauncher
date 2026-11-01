//! Turkey's lunar feasts by Diyanet's calendar. Diyanet starts each Hijri month by calculation,
//! not by a sighting: the month begins on the day after the evening when, anywhere on Earth before
//! 24:00 UT, the Moon stands at least 5° above the horizon at sunset and at least 8° from the Sun
//! (the 1978 Istanbul criterion). From 1438 (October 2016) the 2016 Istanbul congress's unified
//! calendar adds: when that happens only after 24:00 UT, on land in the Americas, the month still
//! begins the next day if the new moon came before dawn in New Zealand. Checked against Diyanet's
//! own dates for 1980–2035 (`tests`).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use jiff::civil::Date;

use super::astro::{self, SUNSET_ALTITUDE, Sky};
use super::date_of_day;
use crate::model::Occasion;

/// Meeus' lunation 0, the new moon of 6 January 2000, began Şevval 1420: Hijri month index
/// `(year - 1) * 12 + (month - 1)` minus this is the lunation.
const LUNATION_OFFSET: i64 = 17_037;
/// The first Hijri year of the 2016 congress's rule.
const UNIFIED_RULE_YEAR: i64 = 1438;
const MIN_ELONGATION: f64 = 8.0;
const MIN_ALTITUDE: f64 = 5.0;
/// Sunsets nearer the poles than this are left out (the criterion is about inhabited places).
const MAX_LATITUDE: f64 = 60.0;
/// Auckland, where New Zealand's dawn is taken.
const NEW_ZEALAND: (f64, f64) = (-36.85, 174.76);
/// The Sun's altitude at dawn (imsak) in Diyanet's tables.
const DAWN_ALTITUDE: f64 = -18.0;
/// Years the feasts are worked out for; other years have none.
const YEARS: std::ops::RangeInclusive<i16> = 1900..=2500;

/// The westernmost land of the Americas by latitude, east longitude: the latest sunsets there.
#[rustfmt::skip]
const AMERICAS_WEST_COAST: [(f64, f64); 24] = [
    (60.0, -165.0), (55.0, -162.0), (50.0, -128.0), (45.0, -124.0), (40.0, -124.4),
    (35.0, -121.0), (30.0, -116.0), (25.0, -112.5), (20.0, -105.7), (15.0, -93.0),
    (10.0, -85.8), (5.0, -77.5), (0.0, -80.1), (-5.0, -81.3), (-10.0, -78.2),
    (-15.0, -75.5), (-20.0, -70.2), (-25.0, -70.6), (-30.0, -71.7), (-35.0, -72.2),
    (-40.0, -73.8), (-45.0, -75.5), (-50.0, -75.5), (-55.0, -72.0),
];

/// One of the two feasts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Feast {
    /// Ramazan Bayramı: 1–3 Şevval.
    Ramadan,
    /// Kurban Bayramı: 10–13 Zilhicce.
    Sacrifice,
}

impl Feast {
    /// Its days, the eve not counted.
    pub const fn days(self) -> i64 {
        match self {
            Feast::Ramadan => 3,
            Feast::Sacrifice => 4,
        }
    }

    /// The calendar occasion it is.
    pub const fn occasion(self) -> Occasion {
        match self {
            Feast::Ramadan => Occasion::RamadanFeast,
            Feast::Sacrifice => Occasion::SacrificeFeast,
        }
    }

    pub const fn of(occasion: Occasion) -> Option<Feast> {
        match occasion {
            Occasion::RamadanFeast => Some(Feast::Ramadan),
            Occasion::SacrificeFeast => Some(Feast::Sacrifice),
            _ => None,
        }
    }

    /// The Hijri month it falls in (1-based) and its first day's place in that month.
    const fn month_and_day(self) -> (i64, i64) {
        match self {
            Feast::Ramadan => (10, 1),
            Feast::Sacrifice => (12, 10),
        }
    }
}

/// The first days of `feast` in the Gregorian `year`, in order: one, or two in the years a lunar
/// year fits twice (Ramazan Bayramı in 2000 and 2033). Empty outside 1900–2500.
pub fn feast_first_days(feast: Feast, year: i16) -> Vec<Date> {
    if !YEARS.contains(&year) {
        return Vec::new();
    }
    static CACHE: OnceLock<Mutex<HashMap<(Feast, i16), Vec<Date>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(days) = cache
        .lock()
        .ok()
        .and_then(|c| c.get(&(feast, year)).cloned())
    {
        return days;
    }
    let (month, day) = feast.month_and_day();
    // The Hijri year a Gregorian year mostly overlaps; the feast falls in it or a neighbour.
    let near = (i64::from(year) - 622) * 33 / 32;
    let mut days: Vec<Date> = (near - 1..=near + 2)
        .map(|hijri_year| {
            let lunation = (hijri_year - 1) * 12 + (month - 1) - LUNATION_OFFSET;
            date_of_day(month_start(lunation) + day - 1)
        })
        .filter(|date| date.year() == year)
        .collect();
    days.sort();
    if let Ok(mut cache) = cache.lock() {
        cache.insert((feast, year), days.clone());
    }
    days
}

/// The day (Julian day number) the Hijri month of `lunation` begins in Turkey.
fn month_start(lunation: i64) -> i64 {
    let conjunction = astro::new_moon(lunation);
    let hijri_year = (lunation + LUNATION_OFFSET).div_euclid(12) + 1;
    let unified = hijri_year >= UNIFIED_RULE_YEAR;
    // 0h UT of the conjunction's day; each later midnight closes one evening to check.
    let day_start = (conjunction - 0.5).floor() + 0.5;
    for n in 1..=3 {
        let midnight = day_start + f64::from(n);
        let begins = seen_before(midnight)
            || (unified && conjunction < new_zealand_dawn(midnight) && seen_in_americas(midnight));
        if begins {
            return day_of_jd(midnight);
        }
    }
    // The crescent always qualifies within two evenings; this is only a bound.
    day_of_jd(day_start + 4.0)
}

/// The civil day that begins at the UT midnight `jd` (a Julian day ending in .5).
fn day_of_jd(jd: f64) -> i64 {
    (jd + 0.5).round() as i64
}

/// Whether the criterion is met at some sunset before the UT midnight `midnight`: checked along
/// the line where the Sun sets at that moment, where the Moon is oldest and highest.
fn seen_before(midnight: f64) -> bool {
    let sky = Sky::at(midnight);
    if sky.elongation < MIN_ELONGATION {
        return false;
    }
    (0..=480).any(|step| {
        let lat = -MAX_LATITUDE + f64::from(step) * 0.25;
        sky.sun_hour_angle(lat, SUNSET_ALTITUDE)
            .is_some_and(|hour_angle| {
                let lon = sky.longitude_at_sun_hour_angle(hour_angle);
                sky.moon_altitude(lat, lon) >= MIN_ALTITUDE
            })
    })
}

/// Whether the criterion is met at a sunset on the Americas' west coast in the evening that ends
/// after the UT midnight `midnight`.
fn seen_in_americas(midnight: f64) -> bool {
    AMERICAS_WEST_COAST.iter().any(|&(lat, lon)| {
        let guess = midnight - lon / 360.0 - 0.25;
        astro::sun_crossing(lat, lon, SUNSET_ALTITUDE, true, guess).is_some_and(|sunset| {
            let sky = Sky::at(sunset);
            sky.elongation >= MIN_ELONGATION && sky.moon_altitude(lat, lon) >= MIN_ALTITUDE
        })
    })
}

/// Dawn in New Zealand on the day that begins at the UT midnight `midnight` (local morning).
fn new_zealand_dawn(midnight: f64) -> f64 {
    let (lat, lon) = NEW_ZEALAND;
    let guess = midnight + 5.0 / 24.0 - lon / 360.0;
    astro::sun_crossing(lat, lon, DAWN_ALTITUDE, false, guess).unwrap_or(f64::INFINITY)
}

#[cfg(test)]
mod tests {
    use super::super::day_of_date;
    use super::*;

    fn day(date: &str) -> i64 {
        day_of_date(date.parse().unwrap())
    }

    /// First days of Ramazan Bayramı from Diyanet's "Dini Günler" calculator
    /// (vakithesaplama.diyanet.gov.tr/dini_gunler.php, read 1 November 2026).
    const DIYANET_RAMADAN: [&str; 58] = [
        "1980-08-12",
        "1981-08-01",
        "1982-07-22",
        "1983-07-12",
        "1984-06-30",
        "1985-06-20",
        "1986-06-09",
        "1987-05-29",
        "1988-05-17",
        "1989-05-06",
        "1990-04-26",
        "1991-04-16",
        "1992-04-04",
        "1993-03-24",
        "1994-03-13",
        "1995-03-03",
        "1996-02-20",
        "1997-02-09",
        "1998-01-29",
        "1999-01-19",
        "2000-01-08",
        "2000-12-27",
        "2001-12-16",
        "2002-12-05",
        "2003-11-25",
        "2004-11-14",
        "2005-11-03",
        "2006-10-23",
        "2007-10-12",
        "2008-09-30",
        "2009-09-20",
        "2010-09-09",
        "2011-08-30",
        "2012-08-19",
        "2013-08-08",
        "2014-07-28",
        "2015-07-17",
        "2016-07-05",
        "2017-06-25",
        "2018-06-15",
        "2019-06-04",
        "2020-05-24",
        "2021-05-13",
        "2022-05-02",
        "2023-04-21",
        "2024-04-10",
        "2025-03-30",
        "2026-03-20",
        "2027-03-09",
        "2028-02-26",
        "2029-02-14",
        "2030-02-04",
        "2031-01-24",
        "2032-01-14",
        "2033-01-02",
        "2033-12-23",
        "2034-12-12",
        "2035-12-01",
    ];

    /// First days of Kurban Bayramı, same source.
    const DIYANET_SACRIFICE: [&str; 57] = [
        "1980-10-19",
        "1981-10-08",
        "1982-09-27",
        "1983-09-17",
        "1984-09-06",
        "1985-08-26",
        "1986-08-16",
        "1987-08-05",
        "1988-07-24",
        "1989-07-13",
        "1990-07-03",
        "1991-06-23",
        "1992-06-11",
        "1993-06-01",
        "1994-05-21",
        "1995-05-10",
        "1996-04-28",
        "1997-04-18",
        "1998-04-07",
        "1999-03-28",
        "2000-03-16",
        "2001-03-05",
        "2002-02-22",
        "2003-02-11",
        "2004-02-01",
        "2005-01-20",
        "2006-01-10",
        "2006-12-31",
        "2007-12-20",
        "2008-12-08",
        "2009-11-27",
        "2010-11-16",
        "2011-11-06",
        "2012-10-25",
        "2013-10-15",
        "2014-10-04",
        "2015-09-24",
        "2016-09-12",
        "2017-09-01",
        "2018-08-21",
        "2019-08-11",
        "2020-07-31",
        "2021-07-20",
        "2022-07-09",
        "2023-06-28",
        "2024-06-16",
        "2025-06-06",
        "2026-05-27",
        "2027-05-16",
        "2028-05-05",
        "2029-04-24",
        "2030-04-13",
        "2031-04-02",
        "2032-03-22",
        "2033-03-11",
        "2034-03-01",
        "2035-02-18",
    ];

    fn computed(feast: Feast) -> Vec<String> {
        (1980..=2035)
            .flat_map(|year| feast_first_days(feast, year))
            .map(|date| date.to_string())
            .collect()
    }

    fn differences(feast: Feast, official: &[&str]) -> Vec<String> {
        let ours = computed(feast);
        let mut out: Vec<String> = official
            .iter()
            .filter(|d| !ours.iter().any(|o| o == *d))
            .map(|d| format!("missing {d}"))
            .collect();
        out.extend(
            ours.iter()
                .filter(|o| !official.contains(&o.as_str()))
                .map(|o| format!("extra {o}")),
        );
        out
    }

    #[test]
    fn ramadan_feast_matches_diyanet_1980_to_2035() {
        assert_eq!(
            differences(Feast::Ramadan, &DIYANET_RAMADAN),
            Vec::<String>::new()
        );
    }

    /// One difference in 56 years, before the current rule: Diyanet began Zilhicce 1433 a day
    /// earlier than either criterion gives (that evening the Moon reached 8° only after 24:00 UT,
    /// over the Pacific).
    #[test]
    fn sacrifice_feast_matches_diyanet_1980_to_2035_but_2012() {
        assert_eq!(
            differences(Feast::Sacrifice, &DIYANET_SACRIFICE),
            ["missing 2012-10-25", "extra 2012-10-26"]
        );
    }

    #[test]
    fn a_lunar_year_can_fit_twice() {
        assert_eq!(feast_first_days(Feast::Ramadan, 2033).len(), 2);
        assert_eq!(feast_first_days(Feast::Sacrifice, 2006).len(), 2);
        assert_eq!(feast_first_days(Feast::Ramadan, 2034).len(), 1);
        assert!(feast_first_days(Feast::Ramadan, 1899).is_empty());
        assert!(feast_first_days(Feast::Ramadan, 2501).is_empty());
    }

    #[test]
    fn later_years_keep_moving_eleven_days_a_year() {
        // No official dates past 2035: the computed ones still come one lunar year (twelve
        // months of 29 or 30 days: 353-356 days) after the last and never skip a year.
        let mut last = day("2035-12-01");
        for year in 2036..=2100 {
            for first in feast_first_days(Feast::Ramadan, year) {
                let step = day_of_date(first) - last;
                assert!((353..=356).contains(&step), "{first}: {step}");
                last = day_of_date(first);
            }
        }
        assert!(last >= day("2100-01-01"));
    }
}

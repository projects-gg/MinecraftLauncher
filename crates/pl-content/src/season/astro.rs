//! The little astronomy the lunar feasts need, after Jean Meeus, "Astronomical Algorithms"
//! (2nd ed.): the Moon's position (chapter 47: the ELP-2000/82 terms Meeus prints, about 10" in
//! longitude), the Sun's (chapter 25, about 0.01°), nutation (chapter 22, the short series),
//! sidereal time (chapter 12) and ΔT (the Espenak–Meeus polynomials). Angles are degrees, times
//! Julian days: UT unless the name says `jde` (Terrestrial Time).

const D2R: f64 = std::f64::consts::PI / 180.0;
const R2D: f64 = 180.0 / std::f64::consts::PI;

/// Julian day of J2000.0.
const J2000: f64 = 2_451_545.0;

/// Altitude of the Sun's centre at sunset: refraction (34') and its radius (16').
pub(super) const SUNSET_ALTITUDE: f64 = -0.8333;

pub(super) fn norm(x: f64) -> f64 {
    x.rem_euclid(360.0)
}

pub(super) fn wrap180(x: f64) -> f64 {
    let x = norm(x);
    if x > 180.0 { x - 360.0 } else { x }
}

fn sin_d(x: f64) -> f64 {
    (x * D2R).sin()
}

fn cos_d(x: f64) -> f64 {
    (x * D2R).cos()
}

/// ΔT = TT − UT in seconds for a decimal year.
fn delta_t(year: f64) -> f64 {
    if year < 2005.0 {
        let t = year - 2000.0;
        63.86 + 0.3345 * t - 0.060374 * t.powi(2)
            + 0.0017275 * t.powi(3)
            + 0.000651814 * t.powi(4)
            + 0.00002373599 * t.powi(5)
    } else if year < 2050.0 {
        let t = year - 2000.0;
        62.92 + 0.32217 * t + 0.005589 * t * t
    } else if year < 2150.0 {
        -20.0 + 32.0 * ((year - 1820.0) / 100.0).powi(2) - 0.5628 * (2150.0 - year)
    } else {
        -20.0 + 32.0 * ((year - 1820.0) / 100.0).powi(2)
    }
}

fn delta_t_days(jd: f64) -> f64 {
    delta_t(2000.0 + (jd - J2000) / 365.25) / 86_400.0
}

/// Periodic terms of the Moon's longitude and distance: multiples of D, M, M', F, then the sine
/// coefficient of Σl (10⁻⁶ degree) and the cosine coefficient of Σr (10⁻³ km). Meeus table 47.A.
#[rustfmt::skip]
const MOON_LR: [(i8, i8, i8, i8, i32, i32); 60] = [
    (0, 0, 1, 0, 6288774, -20905355), (2, 0, -1, 0, 1274027, -3699111),
    (2, 0, 0, 0, 658314, -2955968), (0, 0, 2, 0, 213618, -569925),
    (0, 1, 0, 0, -185116, 48888), (0, 0, 0, 2, -114332, -3149),
    (2, 0, -2, 0, 58793, 246158), (2, -1, -1, 0, 57066, -152138),
    (2, 0, 1, 0, 53322, -170733), (2, -1, 0, 0, 45758, -204586),
    (0, 1, -1, 0, -40923, -129620), (1, 0, 0, 0, -34720, 108743),
    (0, 1, 1, 0, -30383, 104755), (2, 0, 0, -2, 15327, 10321),
    (0, 0, 1, 2, -12528, 0), (0, 0, 1, -2, 10980, 79661),
    (4, 0, -1, 0, 10675, -34782), (0, 0, 3, 0, 10034, -23210),
    (4, 0, -2, 0, 8548, -21636), (2, 1, -1, 0, -7888, 24208),
    (2, 1, 0, 0, -6766, 30824), (1, 0, -1, 0, -5163, -8379),
    (1, 1, 0, 0, 4987, -16675), (2, -1, 1, 0, 4036, -12831),
    (2, 0, 2, 0, 3994, -10445), (4, 0, 0, 0, 3861, -11650),
    (2, 0, -3, 0, 3665, 14403), (0, 1, -2, 0, -2689, -7003),
    (2, 0, -1, 2, -2602, 0), (2, -1, -2, 0, 2390, 10056),
    (1, 0, 1, 0, -2348, 6322), (2, -2, 0, 0, 2236, -9884),
    (0, 1, 2, 0, -2120, 5751), (0, 2, 0, 0, -2069, 0),
    (2, -2, -1, 0, 2048, -4950), (2, 0, 1, -2, -1773, 4130),
    (2, 0, 0, 2, -1595, 0), (4, -1, -1, 0, 1215, -3958),
    (0, 0, 2, 2, -1110, 0), (3, 0, -1, 0, -892, 3258),
    (2, 1, 1, 0, -810, 2616), (4, -1, -2, 0, 759, -1897),
    (0, 2, -1, 0, -713, -2117), (2, 2, -1, 0, -700, 2354),
    (2, 1, -2, 0, 691, 0), (2, -1, 0, -2, 596, 0),
    (4, 0, 1, 0, 549, -1423), (0, 0, 4, 0, 537, -1117),
    (4, -1, 0, 0, 520, -1571), (1, 0, -2, 0, -487, -1739),
    (2, 1, 0, -2, -399, 0), (0, 0, 2, -2, -381, -4421),
    (1, 1, 1, 0, 351, 0), (3, 0, -2, 0, -340, 0),
    (4, 0, -3, 0, 330, 0), (2, -1, 2, 0, 327, 0),
    (0, 2, 1, 0, -323, 1165), (1, 1, -1, 0, 299, 0),
    (2, 0, 3, 0, 294, 0), (2, 0, -1, -2, 0, 8752),
];

/// Periodic terms of the Moon's latitude: multiples of D, M, M', F and the sine coefficient of
/// Σb (10⁻⁶ degree). Meeus table 47.B.
#[rustfmt::skip]
const MOON_B: [(i8, i8, i8, i8, i32); 60] = [
    (0, 0, 0, 1, 5128122), (0, 0, 1, 1, 280602), (0, 0, 1, -1, 277693), (2, 0, 0, -1, 173237),
    (2, 0, -1, 1, 55413), (2, 0, -1, -1, 46271), (2, 0, 0, 1, 32573), (0, 0, 2, 1, 17198),
    (2, 0, 1, -1, 9266), (0, 0, 2, -1, 8822), (2, -1, 0, -1, 8216), (2, 0, -2, -1, 4324),
    (2, 0, 1, 1, 4200), (2, 1, 0, -1, -3359), (2, -1, -1, 1, 2463), (2, -1, 0, 1, 2211),
    (2, -1, -1, -1, 2065), (0, 1, -1, -1, -1870), (4, 0, -1, -1, 1828), (0, 1, 0, 1, -1794),
    (0, 0, 0, 3, -1749), (0, 1, -1, 1, -1565), (1, 0, 0, 1, -1491), (0, 1, 1, 1, -1475),
    (0, 1, 1, -1, -1410), (0, 1, 0, -1, -1344), (1, 0, 0, -1, -1335), (0, 0, 3, 1, 1107),
    (4, 0, 0, -1, 1021), (4, 0, -1, 1, 833), (0, 0, 1, -3, 777), (4, 0, -2, 1, 671),
    (2, 0, 0, -3, 607), (2, 0, 2, -1, 596), (2, -1, 1, -1, 491), (2, 0, -2, 1, -451),
    (0, 0, 3, -1, 439), (2, 0, 2, 1, 422), (2, 0, -3, -1, 421), (2, 1, -1, 1, -366),
    (2, 1, 0, 1, -351), (4, 0, 0, 1, 331), (2, -1, 1, 1, 315), (2, -2, 0, -1, 302),
    (0, 0, 1, 3, -283), (2, 1, 1, -1, -229), (1, 1, 0, -1, 223), (1, 1, 0, 1, 223),
    (0, 1, -2, -1, -220), (2, 1, -1, -1, -220), (1, 0, 1, 1, -185), (2, -1, -2, -1, 181),
    (0, 1, 2, 1, -177), (4, 0, -2, -1, 176), (4, -1, -1, -1, 166), (1, 0, 1, -1, -164),
    (4, 0, 1, -1, 132), (1, 0, -1, -1, -119), (4, -1, 0, -1, 115), (2, -2, 0, 1, 107),
];

struct MoonPosition {
    /// Geometric ecliptic longitude, mean equinox of date.
    lon: f64,
    lat: f64,
    /// Distance between the centres of the Earth and the Moon, km.
    distance: f64,
}

fn moon_position(jde: f64) -> MoonPosition {
    let t = (jde - J2000) / 36_525.0;
    let (t2, t3, t4) = (t * t, t * t * t, t * t * t * t);
    let l1 = 218.3164477 + 481267.88123421 * t - 0.0015786 * t2 + t3 / 538841.0 - t4 / 65194000.0;
    let d = 297.8501921 + 445267.1114034 * t - 0.0018819 * t2 + t3 / 545868.0 - t4 / 113065000.0;
    let m = 357.5291092 + 35999.0502909 * t - 0.0001536 * t2 + t3 / 24490000.0;
    let m1 = 134.9633964 + 477198.8675055 * t + 0.0087414 * t2 + t3 / 69699.0 - t4 / 14712000.0;
    let f = 93.272095 + 483202.0175233 * t - 0.0036539 * t2 - t3 / 3526000.0 + t4 / 863310000.0;
    let a1 = 119.75 + 131.849 * t;
    let a2 = 53.09 + 479264.29 * t;
    let a3 = 313.45 + 481266.484 * t;
    let e = 1.0 - 0.002516 * t - 0.0000074 * t2;
    let eccentricity = |m_multiple: i8| match m_multiple.abs() {
        1 => e,
        2 => e * e,
        _ => 1.0,
    };
    let argument = |dm: i8, mm: i8, m1m: i8, fm: i8| {
        f64::from(dm) * d + f64::from(mm) * m + f64::from(m1m) * m1 + f64::from(fm) * f
    };
    let (mut sl, mut sr, mut sb) = (0.0, 0.0, 0.0);
    for &(dm, mm, m1m, fm, l, r) in &MOON_LR {
        let arg = argument(dm, mm, m1m, fm);
        let k = eccentricity(mm);
        sl += f64::from(l) * k * sin_d(arg);
        sr += f64::from(r) * k * cos_d(arg);
    }
    for &(dm, mm, m1m, fm, b) in &MOON_B {
        sb += f64::from(b) * eccentricity(mm) * sin_d(argument(dm, mm, m1m, fm));
    }
    sl += 3958.0 * sin_d(a1) + 1962.0 * sin_d(l1 - f) + 318.0 * sin_d(a2);
    sb += -2235.0 * sin_d(l1)
        + 382.0 * sin_d(a3)
        + 175.0 * sin_d(a1 - f)
        + 175.0 * sin_d(a1 + f)
        + 127.0 * sin_d(l1 - m1)
        - 115.0 * sin_d(l1 + m1);
    MoonPosition {
        lon: norm(l1 + sl / 1e6),
        lat: sb / 1e6,
        distance: 385_000.56 + sr / 1e3,
    }
}

struct Nutation {
    /// Nutation in longitude.
    psi: f64,
    /// True obliquity of the ecliptic.
    obliquity: f64,
}

fn nutation(jde: f64) -> Nutation {
    let t = (jde - J2000) / 36_525.0;
    let omega = 125.04452 - 1934.136261 * t;
    let l = 280.4665 + 36000.7698 * t;
    let l1 = 218.3165 + 481267.8813 * t;
    let psi = (-17.20 * sin_d(omega) - 1.32 * sin_d(2.0 * l) - 0.23 * sin_d(2.0 * l1)
        + 0.21 * sin_d(2.0 * omega))
        / 3600.0;
    let eps = (9.20 * cos_d(omega) + 0.57 * cos_d(2.0 * l) + 0.10 * cos_d(2.0 * l1)
        - 0.09 * cos_d(2.0 * omega))
        / 3600.0;
    let eps0 = 23.0 + 26.0 / 60.0 + 21.448 / 3600.0
        - (46.8150 * t + 0.00059 * t * t - 0.001813 * t * t * t) / 3600.0;
    Nutation {
        psi,
        obliquity: eps0 + eps,
    }
}

/// The Sun's apparent longitude (nutation and aberration included).
fn sun_longitude(jde: f64) -> f64 {
    let t = (jde - J2000) / 36_525.0;
    let l0 = 280.46646 + 36000.76983 * t + 0.0003032 * t * t;
    let m = 357.52911 + 35999.05029 * t - 0.0001537 * t * t;
    let c = (1.914602 - 0.004817 * t - 0.000014 * t * t) * sin_d(m)
        + (0.019993 - 0.000101 * t) * sin_d(2.0 * m)
        + 0.000289 * sin_d(3.0 * m);
    let omega = 125.04 - 1934.136 * t;
    norm(l0 + c - 0.00569 - 0.00478 * sin_d(omega))
}

/// Right ascension and declination from ecliptic coordinates.
fn equatorial(lon: f64, lat: f64, obliquity: f64) -> (f64, f64) {
    let ra = (sin_d(lon) * cos_d(obliquity) - (lat * D2R).tan() * sin_d(obliquity))
        .atan2(cos_d(lon))
        * R2D;
    let dec =
        (sin_d(lat) * cos_d(obliquity) + cos_d(lat) * sin_d(obliquity) * sin_d(lon)).asin() * R2D;
    (norm(ra), dec)
}

/// Greenwich mean sidereal time at a UT Julian day.
fn mean_sidereal(jd: f64) -> f64 {
    let t = (jd - J2000) / 36_525.0;
    norm(
        280.46061837 + 360.98564736629 * (jd - J2000) + 0.000387933 * t * t
            - t * t * t / 38_710_000.0,
    )
}

/// The Sun and the Moon as seen from the Earth's centre at one moment.
pub(super) struct Sky {
    /// Greenwich apparent sidereal time.
    sidereal: f64,
    sun_ra: f64,
    sun_dec: f64,
    moon_ra: f64,
    moon_dec: f64,
    /// The Moon's equatorial horizontal parallax.
    moon_parallax: f64,
    /// Geocentric angle between the Sun's and the Moon's centres.
    pub(super) elongation: f64,
}

impl Sky {
    pub(super) fn at(jd: f64) -> Self {
        let jde = jd + delta_t_days(jd);
        let nut = nutation(jde);
        let moon = moon_position(jde);
        let moon_lon = moon.lon + nut.psi;
        let sun_lon = sun_longitude(jde);
        let (moon_ra, moon_dec) = equatorial(moon_lon, moon.lat, nut.obliquity);
        let (sun_ra, sun_dec) = equatorial(sun_lon, 0.0, nut.obliquity);
        let elongation = (cos_d(moon.lat) * cos_d(moon_lon - sun_lon)).acos() * R2D;
        Self {
            sidereal: norm(mean_sidereal(jd) + nut.psi * cos_d(nut.obliquity)),
            sun_ra,
            sun_dec,
            moon_ra,
            moon_dec,
            moon_parallax: (6378.14 / moon.distance).asin() * R2D,
            elongation,
        }
    }

    /// The Sun's hour angle at `altitude` for latitude `lat` (positive: afternoon), `None` while
    /// it never gets there that day.
    pub(super) fn sun_hour_angle(&self, lat: f64, altitude: f64) -> Option<f64> {
        let cos_h = (sin_d(altitude) - sin_d(lat) * sin_d(self.sun_dec))
            / (cos_d(lat) * cos_d(self.sun_dec));
        (-1.0..=1.0).contains(&cos_h).then(|| cos_h.acos() * R2D)
    }

    /// The Sun's local hour angle at east longitude `lon`, −180..180.
    pub(super) fn sun_local_hour_angle(&self, lon: f64) -> f64 {
        wrap180(self.sidereal + lon - self.sun_ra)
    }

    /// The east longitude where the Sun's local hour angle is `hour_angle`.
    pub(super) fn longitude_at_sun_hour_angle(&self, hour_angle: f64) -> f64 {
        wrap180(hour_angle - self.sidereal + self.sun_ra)
    }

    /// The Moon's topocentric altitude (no refraction) at `lat`, east longitude `lon`.
    pub(super) fn moon_altitude(&self, lat: f64, lon: f64) -> f64 {
        let hour_angle = self.sidereal + lon - self.moon_ra;
        let geocentric = (sin_d(lat) * sin_d(self.moon_dec)
            + cos_d(lat) * cos_d(self.moon_dec) * cos_d(hour_angle))
        .asin()
            * R2D;
        geocentric - self.moon_parallax * cos_d(geocentric)
    }
}

/// The UT Julian day when the Sun reaches `altitude` at (`lat`, east `lon`) near `guess`, rising
/// (`setting == false`) or setting; `None` where it never gets there.
pub(super) fn sun_crossing(
    lat: f64,
    lon: f64,
    altitude: f64,
    setting: bool,
    guess: f64,
) -> Option<f64> {
    let mut jd = guess;
    for _ in 0..8 {
        let sky = Sky::at(jd);
        let target = sky.sun_hour_angle(lat, altitude)?;
        let target = if setting { target } else { -target };
        let step = wrap180(target - sky.sun_local_hour_angle(lon)) / 360.985_6;
        jd += step;
        if step.abs() < 1e-6 {
            break;
        }
    }
    Some(jd)
}

/// The UT Julian day of the true new moon (geocentric conjunction in apparent longitude) of
/// lunation `k` (Meeus: 0 is the new moon of 6 January 2000).
pub(super) fn new_moon(k: i64) -> f64 {
    let mut jde = 2_451_550.097_66 + 29.530_588_861 * k as f64;
    for _ in 0..20 {
        let nut = nutation(jde);
        let gap = wrap180(moon_position(jde).lon + nut.psi - sun_longitude(jde));
        // The Moon gains about 12.19° a day on the Sun.
        jde -= gap / 12.19;
        if gap.abs() < 1e-7 {
            break;
        }
    }
    jde - delta_t_days(jde)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Meeus example 47.a: 12 April 1992, 0h TD.
    #[test]
    fn moon_matches_meeus_example() {
        let moon = moon_position(2_448_724.5);
        assert!((moon.lon - 133.162655).abs() < 1e-5, "{}", moon.lon);
        assert!((moon.lat - -3.229126).abs() < 1e-5, "{}", moon.lat);
        assert!((moon.distance - 368_409.7).abs() < 0.1, "{}", moon.distance);
    }

    /// Meeus example 25.a: 13 October 1992, 0h TD, apparent longitude 199.90895° by this
    /// low-accuracy method (199.90606° by the full VSOP87 theory).
    #[test]
    fn sun_matches_meeus_example() {
        assert!((sun_longitude(2_448_908.5) - 199.90895).abs() < 2e-4);
    }

    /// The new moon of 6 January 2000 was at 18:14 UT; of 29 March 2025 at 10:58 UT.
    #[test]
    fn new_moons_land_on_the_minute() {
        let minutes = |jd: f64, expected: f64| ((jd - expected) * 1440.0).abs();
        assert!(minutes(new_moon(0), 2_451_550.259_722) < 2.0);
        assert!(minutes(new_moon(312), 2_460_763.956_944) < 2.0);
    }
}

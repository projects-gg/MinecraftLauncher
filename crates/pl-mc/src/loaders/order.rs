//! Ordering of loader version strings (`0.16.10`, `0.20.0-beta.9`, `47.4.10`,
//! `26.1.0.0-alpha.10+snapshot-6`, `0.4.8+build.155`).

use std::cmp::Ordering;

/// Compare two loader versions: the dotted release part numerically, then the pre-release part
/// (`-beta.3`; a version without one is newer), then build metadata (`+build.12`), then the raw
/// text so the order is total.
pub(crate) fn compare_versions(a: &str, b: &str) -> Ordering {
    let (a_main, a_build) = split(a.trim(), '+');
    let (b_main, b_build) = split(b.trim(), '+');
    let (a_release, a_pre) = split(a_main, '-');
    let (b_release, b_pre) = split(b_main, '-');
    compare_dotted(a_release, b_release, true)
        .then_with(|| match (a_pre, b_pre) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(x), Some(y)) => compare_dotted(x, y, false),
        })
        .then_with(|| match (a_build, b_build) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Less,
            (Some(_), None) => Ordering::Greater,
            (Some(x), Some(y)) => compare_dotted(x, y, false),
        })
        .then_with(|| a.cmp(b))
}

/// Sort newest first.
pub(crate) fn sort_newest_first<T>(items: &mut [T], version: impl Fn(&T) -> &str) {
    items.sort_by(|x, y| compare_versions(version(y), version(x)));
}

fn split(text: &str, separator: char) -> (&str, Option<&str>) {
    match text.split_once(separator) {
        Some((head, tail)) => (head, Some(tail)),
        None => (text, None),
    }
}

/// Compare `.`/`-` separated components. In release parts a missing component counts as `0`
/// (`1.0` == `1.0.0`); elsewhere the shorter list is older (`beta` < `beta.1`).
fn compare_dotted(a: &str, b: &str, pad_with_zero: bool) -> Ordering {
    let mut left = a.split(['.', '-']);
    let mut right = b.split(['.', '-']);
    loop {
        let ordering = match (left.next(), right.next()) {
            (None, None) => return Ordering::Equal,
            (Some(x), Some(y)) => compare_component(x, y),
            (Some(x), None) if pad_with_zero => compare_component(x, "0"),
            (None, Some(y)) if pad_with_zero => compare_component("0", y),
            (Some(_), None) => Ordering::Greater,
            (None, Some(_)) => Ordering::Less,
        };
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
}

/// Compare one component by its runs of digits and non-digits (`25w14craftmine`).
fn compare_component(a: &str, b: &str) -> Ordering {
    let mut left = runs(a).into_iter();
    let mut right = runs(b).into_iter();
    loop {
        let ordering = match (left.next(), right.next()) {
            (None, None) => return Ordering::Equal,
            (Some(_), None) => Ordering::Greater,
            (None, Some(_)) => Ordering::Less,
            (Some(x), Some(y)) => compare_run(x, y),
        };
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
}

fn runs(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let bytes = text.as_bytes();
    for index in 1..bytes.len() {
        if bytes[index].is_ascii_digit() != bytes[index - 1].is_ascii_digit() {
            out.push(&text[start..index]);
            start = index;
        }
    }
    if start < text.len() {
        out.push(&text[start..]);
    }
    out
}

fn compare_run(a: &str, b: &str) -> Ordering {
    let a_digits = a.bytes().all(|c| c.is_ascii_digit());
    let b_digits = b.bytes().all(|c| c.is_ascii_digit());
    match (a_digits, b_digits) {
        (true, true) => {
            // Compare as arbitrarily large numbers: strip leading zeros, then length, then text.
            let x = a.trim_start_matches('0');
            let y = b.trim_start_matches('0');
            x.len().cmp(&y.len()).then_with(|| x.cmp(y))
        }
        // Numeric identifiers are older than textual ones (semver).
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (false, false) => qualifier_rank(a)
            .cmp(&qualifier_rank(b))
            .then_with(|| a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase())),
    }
}

fn qualifier_rank(text: &str) -> u8 {
    match text.to_ascii_lowercase().as_str() {
        "snapshot" => 0,
        "alpha" | "a" => 1,
        "beta" | "b" => 2,
        "pre" | "preview" => 3,
        "rc" => 4,
        _ => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn newer(a: &str, b: &str) {
        assert_eq!(compare_versions(a, b), Ordering::Greater, "{a} > {b}");
        assert_eq!(compare_versions(b, a), Ordering::Less, "{b} < {a}");
    }

    #[test]
    fn numeric_and_prerelease_ordering() {
        newer("0.16.10", "0.16.9");
        newer("0.20.0", "0.20.0-beta.9");
        newer("0.20.0-beta.10", "0.20.0-beta.9");
        newer("0.30.1", "0.29.2");
        newer("21.1.262", "21.1.259");
        newer("47.4.10", "47.4.9");
        newer("10.13.4.1614", "10.13.4.1558");
        newer("26.1.0.0-alpha.10+snapshot-6", "26.1.0.0-alpha.9+snapshot-6");
        newer("26.1.0.1-beta", "26.1.0.0-alpha.15+pre-3");
        newer("26.1.2.121", "26.1.2.120");
        newer("21.0.1-beta", "21.0.0-beta");
        newer("0.4.8+build.155", "0.4.8+build.154");
        newer("1.0-rc.1", "1.0-beta.2");
        newer("1.0-beta", "1.0-alpha.5");
        assert_eq!(compare_versions("1.0", "1.0"), Ordering::Equal);
        assert_ne!(compare_versions("1.0", "1.0.0"), Ordering::Equal, "total order");
    }

    #[test]
    fn sorts_newest_first() {
        let mut versions = vec![
            "0.20.0-beta.9",
            "0.20.0-beta.7",
            "0.24.0",
            "0.20.0-beta.10",
            "0.29.2",
            "0.30.1",
        ];
        sort_newest_first(&mut versions, |v| v);
        assert_eq!(
            versions,
            [
                "0.30.1",
                "0.29.2",
                "0.24.0",
                "0.20.0-beta.10",
                "0.20.0-beta.9",
                "0.20.0-beta.7"
            ]
        );
    }
}

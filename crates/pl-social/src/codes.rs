//! Friend codes (`PRJ-XXXX-XXXX`) and crew codes, both Crockford base32, share links and deep
//! links. Crews ("Ekip") show their code as `EKP-XXXX-XXXX`; the service still sends `TKM-…`
//! (what launchers before crews accept back) and takes `EKP-`, `TKM-` or the 8 symbols alone,
//! so the launcher sends the symbols alone: every service version takes them.

use url::Url;

const ALPHABET: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const LENGTH: usize = 8;
const PREFIX: &str = "PRJ";
/// The crew code prefix players see.
pub const CREW_CODE_PREFIX: &str = "EKP";
/// Prefixes a pasted crew code may carry: today's and the one from before crews ("Takım").
const TEAM_PREFIXES: [&str; 2] = [CREW_CODE_PREFIX, "TKM"];

/// The deep link the QR landing page opens: `projects://friend/<code>`.
pub const FRIEND_DEEP_LINK_PREFIX: &str = "projects://friend/";

/// A crew invite link's symbols (TEAM5): 12, longer than a code on purpose (its page names the
/// crew without signing in), with no prefix (the length tells them apart).
const LINK_LENGTH: usize = 12;

/// Normalises a crew invite link (`ABCD-EFGH-JKMN`, `abcd efgh jkmn`, `ABCDEFGHJKMN`, the same
/// alphabet and mapping as codes) into `XXXX-XXXX-XXXX`, or None when it cannot be one. A code
/// (8 symbols) is never a link.
pub fn normalize_crew_link(input: &str) -> Option<String> {
    if input.len() > 40 {
        return None;
    }
    let raw: String = input
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-' && *c != '_')
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if raw.chars().count() != LINK_LENGTH {
        return None;
    }
    let mut out = String::with_capacity(LINK_LENGTH + 2);
    for (index, c) in raw.chars().enumerate() {
        if index > 0 && index % 4 == 0 {
            out.push('-');
        }
        let c = match c {
            'O' => '0',
            'I' | 'L' => '1',
            other => other,
        };
        if !c.is_ascii() || !ALPHABET.contains(&(c as u8)) {
            return None;
        }
        out.push(c);
    }
    Some(out)
}

/// The invite link's page: `<service>/e/XXXX-XXXX-XXXX` (what players share; it opens
/// `projects://crew/<link>`).
pub fn crew_link_url(base: &Url, link: &str) -> Option<Url> {
    let link = normalize_crew_link(link)?;
    base.join(&format!("e/{link}")).ok()
}

/// Normalises what a player typed or scanned into `PRJ-XXXX-XXXX`, or None when it cannot be
/// a code. Accepts `prj abcd efgh`, `ABCDEFGH`, `abcd-efgh`; maps I/L to 1 and O to 0.
pub fn normalize_friend_code(input: &str) -> Option<String> {
    let symbols = symbols(input, &[PREFIX])?;
    Some(format!("{PREFIX}-{symbols}"))
}

/// Normalises a crew code (`EKP-…`, `TKM-…` or the 8 symbols alone, same rules as friend codes)
/// into `XXXX-XXXX`, the form every service version takes.
pub fn normalize_team_code(input: &str) -> Option<String> {
    symbols(input, &TEAM_PREFIXES)
}

/// A crew code as players see it: `EKP-XXXX-XXXX` (the service sends `TKM-XXXX-XXXX`). Anything
/// that is not a code stays as it is.
pub fn crew_code(code: &str) -> String {
    normalize_team_code(code).map_or_else(
        || code.to_owned(),
        |symbols| format!("{CREW_CODE_PREFIX}-{symbols}"),
    )
}

/// The 8 symbols as `XXXX-XXXX`, without one of `prefixes`, or None.
fn symbols(input: &str, prefixes: &[&str]) -> Option<String> {
    if input.len() > 32 {
        return None;
    }
    let mut code: String = input
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-' && *c != '_')
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if let Some(prefix) = prefixes
        .iter()
        .find(|prefix| code.len() == prefix.len() + LENGTH && code.starts_with(**prefix))
    {
        code.drain(..prefix.len());
    }
    if code.len() != LENGTH {
        return None;
    }
    let mut out = String::with_capacity(LENGTH + 1);
    for (index, c) in code.chars().enumerate() {
        if index == 4 {
            out.push('-');
        }
        let c = match c {
            'O' => '0',
            'I' | 'L' => '1',
            other => other,
        };
        if !c.is_ascii() || !ALPHABET.contains(&(c as u8)) {
            return None;
        }
        out.push(c);
    }
    Some(out)
}

/// The friend code inside `projects://friend/<code>`, normalised.
pub fn friend_code_from_deep_link(link: &str) -> Option<String> {
    let rest = link.strip_prefix(FRIEND_DEEP_LINK_PREFIX)?;
    normalize_friend_code(rest.trim_end_matches('/'))
}

/// `projects://friend/PRJ-XXXX-XXXX`
pub fn friend_deep_link(code: &str) -> Option<String> {
    normalize_friend_code(code).map(|code| format!("{FRIEND_DEEP_LINK_PREFIX}{code}"))
}

/// The share link (what the QR encodes): `<service>/f/PRJ-XXXX-XXXX`.
pub fn friend_share_link(base: &Url, code: &str) -> Option<Url> {
    let code = normalize_friend_code(code)?;
    base.join(&format!("f/{code}")).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_like_the_service() {
        assert_eq!(
            normalize_friend_code("PRJ-ABCD-EFGH").as_deref(),
            Some("PRJ-ABCD-EFGH")
        );
        assert_eq!(
            normalize_friend_code("prj abcd efgh").as_deref(),
            Some("PRJ-ABCD-EFGH")
        );
        assert_eq!(
            normalize_friend_code("abcdefgh").as_deref(),
            Some("PRJ-ABCD-EFGH")
        );
        assert_eq!(
            normalize_friend_code("O0IL-1234").as_deref(),
            Some("PRJ-0011-1234")
        );
        assert_eq!(normalize_friend_code("ABCD-EFGU"), None);
        assert_eq!(normalize_friend_code("ABC"), None);
        assert_eq!(normalize_friend_code("ABCD-EFGŞ"), None);
        assert_eq!(normalize_friend_code(&"x".repeat(40)), None);
    }

    #[test]
    fn crew_codes_take_both_prefixes_and_go_out_without_one() {
        assert_eq!(
            normalize_team_code("tkm abcd efgh").as_deref(),
            Some("ABCD-EFGH")
        );
        assert_eq!(
            normalize_team_code("EKP-ABCD-EFGH").as_deref(),
            Some("ABCD-EFGH")
        );
        assert_eq!(
            normalize_team_code("ekp_abcd_efgh").as_deref(),
            Some("ABCD-EFGH")
        );
        assert_eq!(
            normalize_team_code("abcdefgh").as_deref(),
            Some("ABCD-EFGH")
        );
        // E, K, P and T, K, M are code symbols too: 8 symbols that start like a prefix stay.
        assert_eq!(
            normalize_team_code("EKPA1234").as_deref(),
            Some("EKPA-1234")
        );
        assert_eq!(
            normalize_team_code("o0il-1234").as_deref(),
            Some("0011-1234")
        );
        // A friend code is not a crew code, nor the other way round.
        assert_eq!(normalize_team_code("PRJ-ABCD-EFGH"), None);
        assert_eq!(normalize_friend_code("TKM-ABCD-EFGH"), None);
        assert_eq!(normalize_friend_code("EKP-ABCD-EFGH"), None);
        // Shown with the crew prefix whatever the service sent.
        assert_eq!(crew_code("TKM-7Q2X-9KDA"), "EKP-7Q2X-9KDA");
        assert_eq!(crew_code("EKP-7Q2X-9KDA"), "EKP-7Q2X-9KDA");
        assert_eq!(crew_code("PRJ-ABCD-EFGH"), "PRJ-ABCD-EFGH");
    }

    #[test]
    fn links() {
        let base = Url::parse("https://social.projects.gg/").unwrap();
        assert_eq!(
            friend_share_link(&base, "abcd efgh").unwrap().as_str(),
            "https://social.projects.gg/f/PRJ-ABCD-EFGH"
        );
        assert_eq!(
            friend_deep_link("abcdefgh").as_deref(),
            Some("projects://friend/PRJ-ABCD-EFGH")
        );
        assert_eq!(
            friend_code_from_deep_link("projects://friend/prj-abcd-efgh/").as_deref(),
            Some("PRJ-ABCD-EFGH")
        );
        assert_eq!(friend_code_from_deep_link("projects://news/1"), None);
    }

    #[test]
    fn crew_links_are_twelve_symbols_and_never_a_code() {
        assert_eq!(
            normalize_crew_link("abcd efgh jkmn").as_deref(),
            Some("ABCD-EFGH-JKMN")
        );
        assert_eq!(
            normalize_crew_link("ABCDEFGHJKMN").as_deref(),
            Some("ABCD-EFGH-JKMN")
        );
        assert_eq!(
            normalize_crew_link("o0il-1234-5678").as_deref(),
            Some("0011-1234-5678")
        );
        // A code, a friend code or a broken link is not a link.
        assert_eq!(normalize_crew_link("ABCD-EFGH"), None);
        assert_eq!(normalize_crew_link("PRJ-ABCD-EFGH"), None);
        assert_eq!(normalize_crew_link("ABCD-EFGH-JKMU"), None);
        assert_eq!(normalize_crew_link("ABCD-EFGH-JKMŞ"), None);
        assert_eq!(normalize_crew_link(&"A".repeat(41)), None);
        let base = Url::parse("https://social.projects.gg/").unwrap();
        assert_eq!(
            crew_link_url(&base, "abcd efgh jkmn").unwrap().as_str(),
            "https://social.projects.gg/e/ABCD-EFGH-JKMN"
        );
        let local = Url::parse("http://127.0.0.1:8797/").unwrap();
        assert_eq!(
            crew_link_url(&local, "ABCDEFGHJKMN").unwrap().as_str(),
            "http://127.0.0.1:8797/e/ABCD-EFGH-JKMN"
        );
        assert!(crew_link_url(&base, "nope").is_none());
    }
}

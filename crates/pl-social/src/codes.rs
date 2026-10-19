//! Friend codes (`PRJ-XXXX-XXXX`) and team codes (`TKM-XXXX-XXXX`), both Crockford base32,
//! share links and deep links.

use url::Url;

const ALPHABET: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const LENGTH: usize = 8;
const PREFIX: &str = "PRJ";
const TEAM_PREFIX: &str = "TKM";

/// The deep link the QR landing page opens: `projects://friend/<code>`.
pub const FRIEND_DEEP_LINK_PREFIX: &str = "projects://friend/";

/// Normalises what a player typed or scanned into `PRJ-XXXX-XXXX`, or None when it cannot be
/// a code. Accepts `prj abcd efgh`, `ABCDEFGH`, `abcd-efgh`; maps I/L to 1 and O to 0.
pub fn normalize_friend_code(input: &str) -> Option<String> {
    normalize(input, PREFIX)
}

/// Normalises a team code into `TKM-XXXX-XXXX` (same rules as friend codes, own prefix).
pub fn normalize_team_code(input: &str) -> Option<String> {
    normalize(input, TEAM_PREFIX)
}

fn normalize(input: &str, prefix: &str) -> Option<String> {
    if input.len() > 32 {
        return None;
    }
    let mut code: String = input
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-' && *c != '_')
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if code.len() == prefix.len() + LENGTH && code.starts_with(prefix) {
        code.drain(..prefix.len());
    }
    if code.len() != LENGTH {
        return None;
    }
    let mut out = String::with_capacity(13);
    out.push_str(prefix);
    for (index, c) in code.chars().enumerate() {
        if index % 4 == 0 {
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
    fn team_codes_have_their_own_prefix() {
        assert_eq!(
            normalize_team_code("tkm abcd efgh").as_deref(),
            Some("TKM-ABCD-EFGH")
        );
        assert_eq!(
            normalize_team_code("abcdefgh").as_deref(),
            Some("TKM-ABCD-EFGH")
        );
        // A friend code is not a team code.
        assert_eq!(normalize_team_code("PRJ-ABCD-EFGH"), None);
        assert_eq!(normalize_friend_code("TKM-ABCD-EFGH"), None);
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
}

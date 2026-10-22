//! The plain links of a chat message as the friends service finds them (CHAT2; its README, "How
//! links are checked", step 1): `http://` or `https://` in any case, not right after a letter or a
//! digit, then URL characters (ASCII letters and digits, `- . _ ~ : / ? # [ ] @ ! $ & ' ( ) * + , ;
//! = %`, and letters, marks and digits of any script), without what only follows a link in a
//! sentence (`. , ; : ! ? ' *`, a `)` or `]` without its opening one, a Turkish case ending such as
//! `'i`). Only finding: whether a link may go or be pressed is the launcher's composer check
//! (`text.ts`, the shared rule table) and the service's; here a message's first link names its card.

use std::sync::OnceLock;

use regex::Regex;

/// URL characters after the scheme.
fn url_chars() -> &'static Regex {
    static CHARS: OnceLock<Regex> = OnceLock::new();
    CHARS.get_or_init(|| {
        Regex::new(r"\A[\p{L}\p{M}\p{N}\-._~:/?#\[\]@!$&'()*+,;=%]*")
            .expect("a fixed pattern compiles")
    })
}

/// A letter or a digit of any script (a link never starts right after one).
fn letter_or_digit(ch: char) -> bool {
    static CLASS: OnceLock<Regex> = OnceLock::new();
    let class =
        CLASS.get_or_init(|| Regex::new(r"\A[\p{L}\p{N}]\z").expect("a fixed pattern compiles"));
    let mut buffer = [0_u8; 4];
    class.is_match(ch.encode_utf8(&mut buffer))
}

/// A case ending after an apostrophe: letters only, to the end.
fn case_ending(url: &str) -> Option<usize> {
    let at = url.rfind('\'')?;
    let tail = &url[at + 1..];
    (!tail.is_empty() && tail.chars().all(char::is_alphabetic)).then_some(at)
}

/// A link as written, without what only follows it in a sentence.
fn trim(raw: &str) -> &str {
    let mut url = raw;
    loop {
        let scheme = url.find("://").map_or(url.len(), |at| at + 3);
        if url.len() <= scheme {
            return url;
        }
        if let Some(at) = case_ending(url).filter(|at| *at > scheme) {
            url = &url[..at];
        } else if url.ends_with(['.', ',', ';', ':', '!', '?', '\'', '*'])
            || (url.ends_with(')') && url.matches('(').count() < url.matches(')').count())
            || (url.ends_with(']') && url.matches('[').count() < url.matches(']').count())
        {
            url = &url[..url.len() - 1];
        } else {
            return url;
        }
    }
}

/// The scheme at `at` (`http://` or `https://`, any case), as its length.
fn scheme_at(text: &str, at: usize) -> Option<usize> {
    let rest = text.get(at..)?;
    let lower = |n: usize| rest.get(..n).map(str::to_ascii_lowercase);
    if lower(8).as_deref() == Some("https://") {
        Some(8)
    } else if lower(7).as_deref() == Some("http://") {
        Some(7)
    } else {
        None
    }
}

/// The plain links of a text, in order, as written.
pub fn find_links(text: &str) -> Vec<&str> {
    let mut links = Vec::new();
    let mut from = 0;
    while let Some(offset) = text.get(from..).and_then(|rest| rest.find(['h', 'H'])) {
        let at = from + offset;
        let after_letter = text[..at].chars().next_back().is_some_and(letter_or_digit);
        match scheme_at(text, at).filter(|_| !after_letter) {
            Some(scheme) => {
                let tail = &text[at + scheme..];
                let length = url_chars().find(tail).map_or(0, |found| found.end());
                let url = trim(&text[at..at + scheme + length]);
                links.push(url);
                from = at + scheme + length;
            }
            None => from = at + 1,
        }
    }
    links
}

/// A message's first plain `https` link, the one a card is made for.
pub fn first_link(text: &str) -> Option<&str> {
    find_links(text)
        .into_iter()
        .next()
        .filter(|url| url.len() > 8 && url[..8].eq_ignore_ascii_case("https://"))
}

/// A link's host, lower case, without a port or a trailing dot (`https://WWW.YouTube.com./x` →
/// `www.youtube.com`); empty when it has none.
pub fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let authority = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let host = match authority.rsplit_once(':') {
        Some((host, port)) if port.bytes().all(|b| b.is_ascii_digit()) => host,
        _ => authority,
    };
    host.trim_end_matches('.').to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_are_found_as_the_service_finds_them() {
        assert_eq!(
            find_links("şuna bak https://www.youtube.com/watch?v=dQw4w9WgXcQ, harika"),
            ["https://www.youtube.com/watch?v=dQw4w9WgXcQ"]
        );
        // Sentence endings, unbalanced brackets and a Turkish case ending are left out.
        assert_eq!(
            find_links("(https://youtu.be/x'i izle) ve https://projects.gg/wiki."),
            ["https://youtu.be/x", "https://projects.gg/wiki"]
        );
        assert_eq!(
            find_links("https://tr.wikipedia.org/wiki/Minecraft_(oyun)"),
            ["https://tr.wikipedia.org/wiki/Minecraft_(oyun)"]
        );
        // Any case; never right after a letter or a digit; an emoji ends it.
        assert_eq!(
            find_links("HTTPS://Modrinth.com/mod/sodium🔥"),
            ["HTTPS://Modrinth.com/mod/sodium"]
        );
        assert!(find_links("xhttps://a.com").is_empty());
        assert_eq!(find_links("1 http://a.com"), ["http://a.com"]);
        // Letters of any script are URL characters.
        assert_eq!(
            find_links("https://örnek.com/şey yok"),
            ["https://örnek.com/şey"]
        );
        assert!(find_links("discord.gg/abc ve youtube . com").is_empty());
    }

    #[test]
    fn the_first_https_link_names_the_card() {
        assert_eq!(
            first_link("bak: https://youtu.be/dQw4w9WgXcQ ve https://twitch.tv/x"),
            Some("https://youtu.be/dQw4w9WgXcQ")
        );
        assert_eq!(first_link("http://youtu.be/x"), None);
        assert_eq!(first_link("https://"), None);
        assert_eq!(first_link("selam"), None);
        assert_eq!(
            host_of("https://WWW.YouTube.com./watch?v=1"),
            "www.youtube.com"
        );
        assert_eq!(host_of("https://user@example.com:8080/x"), "example.com");
        assert_eq!(host_of("https://youtu.be"), "youtu.be");
    }
}

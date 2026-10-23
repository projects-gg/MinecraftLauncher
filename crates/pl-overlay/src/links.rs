//! Which links of a chat message the player may press in game (CHAT2 §9): the friends service's
//! rule table (`src/chat-links.json`, copied beside the launcher's composer check as
//! `apps/launcher/src/features/social/chat/chat-links.json`) and Studio's lists, as they are now.
//!
//! The launcher's page applies the same table in TypeScript (`text.ts`, `messageLinks`), but the
//! page rests while a game runs, so the bridge decides here and tells the mod only the links that
//! pass; the mod opens nothing else. These are the per-link rules of `linkProblem`: a message on
//! screen already went through the service's whole check when it was sent, and a link Studio blocks
//! later (or a newer table refuses) turns back into plain text in it.

use std::collections::HashSet;
use std::sync::OnceLock;

use serde::Deserialize;

/// The rule table, one source for every client.
const TABLE: &str = include_str!("../../../apps/launcher/src/features/social/chat/chat-links.json");

/// Studio's lists, as `hello.chat.links` brings them: hosts opened without a warning, hosts and
/// pages no link may lead to, whether Discord invites may be linked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LinkLists {
    pub trusted: Vec<String>,
    pub blocked: Vec<String>,
    pub invites: bool,
}

/// A link the player may press now, as written in the message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pressable {
    pub url: String,
    /// A site Studio trusts (the launcher opens it without its warning).
    pub trusted: bool,
}

/// Why a link may not go or be pressed: the service's `rule` names (`phone`: it carries a phone
/// number).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    Form,
    Count,
    Http,
    Credentials,
    Port,
    Ip,
    Long,
    Shortener,
    Grabber,
    Adult,
    Lookalike,
    Scam,
    File,
    Invite,
    Blocked,
    Phone,
}

impl Problem {
    /// The rule's name as the service and the composer say it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Form => "form",
            Self::Count => "count",
            Self::Http => "http",
            Self::Credentials => "credentials",
            Self::Port => "port",
            Self::Ip => "ip",
            Self::Long => "long",
            Self::Shortener => "shortener",
            Self::Grabber => "grabber",
            Self::Adult => "adult",
            Self::Lookalike => "lookalike",
            Self::Scam => "scam",
            Self::File => "file",
            Self::Invite => "invite",
            Self::Blocked => "blocked",
            Self::Phone => "phone",
        }
    }
}

#[derive(Deserialize)]
struct Table {
    version: u32,
    limits: Limits,
    shorteners: Vec<String>,
    grabbers: Vec<String>,
    messengers: HostList,
    files: FileList,
    adult: AdultList,
    lookalike: LookalikeList,
    scams: ScamList,
    invites: InviteList,
    studio: StudioList,
}

#[derive(Deserialize)]
struct Limits {
    links: usize,
    length: usize,
}

#[derive(Deserialize)]
struct HostList {
    hosts: Vec<String>,
}

#[derive(Deserialize)]
struct FileList {
    extensions: Vec<String>,
}

#[derive(Deserialize)]
struct AdultList {
    tlds: Vec<String>,
}

#[derive(Deserialize)]
struct LookalikeList {
    confusables: Vec<(String, String)>,
    #[serde(rename = "secondLevel")]
    second_level: Vec<String>,
    brands: Vec<BrandEntry>,
    lures: Vec<String>,
}

#[derive(Deserialize)]
struct BrandEntry {
    keys: Vec<String>,
    official: Vec<String>,
}

#[derive(Deserialize)]
struct ScamList {
    phrases: Vec<String>,
}

#[derive(Deserialize)]
struct InviteList {
    hosts: Vec<String>,
    paths: Vec<String>,
}

#[derive(Deserialize)]
struct StudioList {
    trusted: Vec<String>,
    blocked: Vec<String>,
}

/// The table, ready to apply.
struct Rules {
    version: u32,
    max_links: usize,
    max_length: usize,
    shorteners: Vec<String>,
    grabbers: Vec<String>,
    messengers: Vec<String>,
    files: HashSet<String>,
    adult: HashSet<String>,
    confusables: Vec<(String, String)>,
    second_level: HashSet<String>,
    /// Each brand's names as the eye reads them, and its own domains.
    brands: Vec<(Vec<String>, Vec<String>)>,
    official: Vec<String>,
    lures: HashSet<String>,
    lure_max: usize,
    scams: Vec<String>,
    invite_hosts: Vec<String>,
    invite_paths: Vec<String>,
    studio: LinkLists,
}

fn rules() -> &'static Rules {
    static RULES: OnceLock<Rules> = OnceLock::new();
    RULES.get_or_init(|| {
        // The table is tested to parse (below); a broken copy would refuse every link.
        let table: Table = serde_json::from_str(TABLE).unwrap_or_else(|error| {
            tracing::warn!(%error, "the chat link rules did not parse; no link can be pressed");
            Table {
                version: 0,
                limits: Limits {
                    links: 0,
                    length: 0,
                },
                shorteners: Vec::new(),
                grabbers: Vec::new(),
                messengers: HostList { hosts: Vec::new() },
                files: FileList {
                    extensions: Vec::new(),
                },
                adult: AdultList { tlds: Vec::new() },
                lookalike: LookalikeList {
                    confusables: Vec::new(),
                    second_level: Vec::new(),
                    brands: Vec::new(),
                    lures: Vec::new(),
                },
                scams: ScamList {
                    phrases: Vec::new(),
                },
                invites: InviteList {
                    hosts: Vec::new(),
                    paths: Vec::new(),
                },
                studio: StudioList {
                    trusted: Vec::new(),
                    blocked: Vec::new(),
                },
            }
        });
        let confusables = table.lookalike.confusables;
        let skeleton_of = |name: &str| skeleton_with(&confusables, name);
        let brands: Vec<(Vec<String>, Vec<String>)> = table
            .lookalike
            .brands
            .iter()
            .map(|brand| {
                (
                    brand.keys.iter().map(|key| skeleton_of(key)).collect(),
                    brand.official.clone(),
                )
            })
            .collect();
        let official = brands
            .iter()
            .flat_map(|(_, official)| official.iter().cloned())
            .collect();
        let lures: HashSet<String> = table
            .lookalike
            .lures
            .iter()
            .map(|lure| skeleton_of(lure))
            .collect();
        let lure_max = lures.iter().map(String::len).max().unwrap_or(0);
        Rules {
            version: table.version,
            max_links: table.limits.links,
            max_length: table.limits.length,
            shorteners: table.shorteners,
            grabbers: table.grabbers,
            messengers: table.messengers.hosts,
            files: table.files.extensions.into_iter().collect(),
            adult: table.adult.tlds.into_iter().collect(),
            second_level: table.lookalike.second_level.into_iter().collect(),
            brands,
            official,
            lures,
            lure_max,
            scams: table.scams.phrases,
            invite_hosts: table.invites.hosts,
            invite_paths: table.invites.paths,
            studio: LinkLists {
                trusted: table.studio.trusted,
                blocked: table.studio.blocked,
                invites: true,
            },
            confusables,
        }
    })
}

/// The rule table's version.
pub fn version() -> u32 {
    rules().version
}

/// What the service starts with while Studio has never set its lists.
pub fn default_lists() -> LinkLists {
    rules().studio.clone()
}

/// The links of a message's text the player may press now, in order: plain `https` links that pass
/// the rules and Studio's lists (at most the first three count, as in a message that may go).
pub fn pressable(text: &str, lists: &LinkLists) -> Vec<Pressable> {
    let rules = rules();
    pl_social::find_links(text)
        .into_iter()
        .take(rules.max_links)
        .filter(|url| problem(url, lists).is_none())
        .map(|url| Pressable {
            url: url.to_owned(),
            trusted: trusted(&host(&parts(url).1), lists),
        })
        .collect()
}

/// A link as written: its scheme (lower case), what lies between `//` and the first `/`, `?` or
/// `#`, and the rest (path, query, fragment).
fn parts(url: &str) -> (String, String, String) {
    let (scheme, tail) = url.split_once("://").unwrap_or(("", url));
    let cut = tail.find(['/', '?', '#']).unwrap_or(tail.len());
    (
        scheme.to_ascii_lowercase(),
        tail[..cut].to_owned(),
        tail[cut..].to_owned(),
    )
}

/// A link's host as the rules read it: the authority in lower case, without a trailing dot.
fn host(authority: &str) -> String {
    let lower = authority.to_lowercase();
    lower.strip_suffix('.').map_or(lower.clone(), str::to_owned)
}

fn under(host: &str, domain: &str) -> bool {
    host == domain
        || host
            .strip_suffix(domain)
            .is_some_and(|before| before.ends_with('.'))
}

fn any_under(host: &str, domains: &[String]) -> bool {
    domains.iter().any(|domain| under(host, domain))
}

/// A trusted site (its subdomains too).
fn trusted(host: &str, lists: &LinkLists) -> bool {
    any_under(host, &lists.trusted)
}

/// `^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$`
fn label_ok(label: &str) -> bool {
    let bytes = label.as_bytes();
    let alnum = |b: &u8| b.is_ascii_lowercase() || b.is_ascii_digit();
    !bytes.is_empty()
        && bytes.len() <= 63
        && bytes.first().is_some_and(alnum)
        && bytes.last().is_some_and(alnum)
        && bytes.iter().all(|b| alnum(b) || *b == b'-')
}

/// `^[a-z]{2,24}$`
fn tld_ok(label: &str) -> bool {
    (2..=24).contains(&label.len()) && label.bytes().all(|b| b.is_ascii_lowercase())
}

/// Why one plain link may not be pressed (the composer's `linkProblem`), None when it may.
pub fn problem(url: &str, lists: &LinkLists) -> Option<Problem> {
    let rules = rules();
    let (scheme, authority, rest) = parts(url);
    if scheme != "https" {
        return Some(Problem::Http);
    }
    if authority.contains('@') {
        return Some(Problem::Credentials);
    }
    // An address with or without a port is an address first.
    let named = authority
        .rfind(':')
        .map_or(authority.as_str(), |at| &authority[..at]);
    if authority.starts_with('[')
        || (!named.is_empty() && named.bytes().all(|b| b.is_ascii_digit() || b == b'.'))
    {
        return Some(Problem::Ip);
    }
    if authority.contains(':') {
        return Some(Problem::Port);
    }
    let host = host(&authority);
    let labels: Vec<&str> = host.split('.').collect();
    if host.chars().any(|c| !(' '..='~').contains(&c))
        || labels.iter().any(|label| label.starts_with("xn--"))
    {
        return Some(Problem::Lookalike);
    }
    if host.len() > 253
        || labels.len() < 2
        || !labels.iter().all(|label| label_ok(label))
        || !labels.last().is_some_and(|last| tld_ok(last))
    {
        return Some(Problem::Form);
    }
    if url.chars().count() > rules.max_length {
        return Some(Problem::Long);
    }
    if any_under(&host, &rules.shorteners) {
        return Some(Problem::Shortener);
    }
    if any_under(&host, &rules.grabbers) {
        return Some(Problem::Grabber);
    }
    if labels.last().is_some_and(|tld| rules.adult.contains(*tld)) {
        return Some(Problem::Adult);
    }
    if !trusted(&host, lists) && lookalike(&host) {
        return Some(Problem::Lookalike);
    }
    if any_under(&host, &rules.messengers) && phone_in(&decoded(&rest)) {
        return Some(Problem::Phone);
    }
    if scammy(&format!("{host} {}", decoded(&rest))) {
        return Some(Problem::Scam);
    }
    let path = decoded(rest.split(['?', '#']).next().unwrap_or_default()).to_lowercase();
    for segment in path.split('/') {
        if let Some(dot) = segment.rfind('.')
            && rules.files.contains(&segment[dot + 1..])
        {
            return Some(Problem::File);
        }
    }
    if !lists.invites && discord_invite(&host, &rest) {
        return Some(Problem::Invite);
    }
    if lists
        .blocked
        .iter()
        .any(|entry| entry_names(entry, &host, &rest))
    {
        return Some(Problem::Blocked);
    }
    None
}

/// A whole message's plain links against the rules, as the service checks one that is sent: at most
/// three, each allowed, no scam phrase beside them. (The service then reads the rest of the text with
/// CHAT1's checks, which a shown message passed already.)
pub fn check(text: &str, lists: &LinkLists) -> Result<Vec<Pressable>, Problem> {
    let rules = rules();
    let found = pl_social::find_links(text);
    if found.len() > rules.max_links {
        return Err(Problem::Count);
    }
    let mut links = Vec::new();
    for url in &found {
        if let Some(problem) = problem(url, lists) {
            return Err(problem);
        }
        links.push(Pressable {
            url: (*url).to_owned(),
            trusted: trusted(&host(&parts(url).1), lists),
        });
    }
    if !found.is_empty() && scammy(&without(text, &found)) {
        return Err(Problem::Scam);
    }
    Ok(links)
}

/// `text` with the found links (slices of it) blanked out.
fn without(text: &str, links: &[&str]) -> String {
    let base = text.as_ptr() as usize;
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for link in links {
        let start = (link.as_ptr() as usize).saturating_sub(base);
        if start < at || start + link.len() > text.len() {
            continue;
        }
        out.push_str(&text[at..start]);
        out.extend(std::iter::repeat_n(' ', link.chars().count()));
        at = start + link.len();
    }
    out.push_str(&text[at..]);
    out
}

/// A list entry names a link: a host (with its subdomains), or a host and the start of its path in
/// whole segments, without regard to case (`youtube.com/@kanal` names `/@kanal` and
/// `/@kanal/videos`, not `/@kanallar`).
fn entry_names(entry: &str, host: &str, rest: &str) -> bool {
    let Some(slash) = entry.find('/') else {
        return under(host, entry);
    };
    let prefix = entry[slash..].to_lowercase();
    let path = rest.to_lowercase();
    if !under(host, &entry[..slash]) || !path.starts_with(&prefix) {
        return false;
    }
    let next = path[prefix.len()..].chars().next();
    prefix.ends_with('/') || matches!(next, None | Some('/' | '?' | '#'))
}

fn discord_invite(host: &str, rest: &str) -> bool {
    let rules = rules();
    any_under(host, &rules.invite_hosts)
        || rules
            .invite_paths
            .iter()
            .any(|entry| entry_names(entry, host, rest))
}

/// A run of 10 to 15 digits (a phone number in a messenger link).
fn phone_in(text: &str) -> bool {
    text.split(|c: char| !c.is_ascii_digit())
        .any(|run| (10..=15).contains(&run.len()))
}

/// `decodeURIComponent`: the text as it was before percent-encoding, or as it is when that is not
/// well formed.
fn decoded(text: &str) -> String {
    if !text.contains('%') {
        return text.to_owned();
    }
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = text
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok());
            match hex {
                Some(byte)
                    if bytes[i + 1].is_ascii_hexdigit() && bytes[i + 2].is_ascii_hexdigit() =>
                {
                    out.push(byte);
                    i += 3;
                }
                _ => return text.to_owned(),
            }
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| text.to_owned())
}

/// A name as the eye reads it: no hyphens, look-alike letters and digits as one letter.
fn skeleton_with(confusables: &[(String, String)], name: &str) -> String {
    let mut out = name.replace('-', "");
    for (from, to) in confusables {
        out = out.replace(from.as_str(), to);
    }
    out
}

fn skeleton(name: &str) -> String {
    skeleton_with(&rules().confusables, name)
}

/// Edit distance with swaps of two neighbours (optimal string alignment), on ASCII names.
fn typo_distance(a: &str, b: &str) -> usize {
    let a = a.as_bytes();
    let b = b.as_bytes();
    let mut d = vec![vec![0_usize; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut best = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                best = best.min(d[i - 2][j - 2] + 1);
            }
            d[i][j] = best;
        }
    }
    d[a.len()][b.len()]
}

/// Whether `text` is made of lure words only (or is empty).
fn lures(text: &str) -> bool {
    let rules = rules();
    let n = text.len();
    let mut can = vec![false; n + 1];
    can[0] = true;
    for end in 1..=n {
        let mut start = end.saturating_sub(rules.lure_max);
        while start < end && !can[end] {
            can[end] = can[start]
                && text
                    .get(start..end)
                    .is_some_and(|word| rules.lures.contains(word));
            start += 1;
        }
    }
    can[n]
}

/// The label before the public suffix (`example` of `www.example.com.tr`).
fn main_label<'a>(labels: &[&'a str]) -> &'a str {
    let n = labels.len();
    let last = labels.get(n.wrapping_sub(1)).copied().unwrap_or_default();
    let second = labels.get(n.wrapping_sub(2)).copied().unwrap_or_default();
    if n >= 3 && last.len() == 2 && rules().second_level.contains(second) {
        labels.get(n - 3).copied().unwrap_or_default()
    } else {
        second
    }
}

/// Whether a (valid, lower-case) host borrows a brand's name without being the brand's.
fn lookalike(host: &str) -> bool {
    let rules = rules();
    if any_under(host, &rules.official) {
        return false;
    }
    let labels: Vec<&str> = host.split('.').collect();
    // An official domain as whole labels inside the host, more labels after it.
    for domain in &rules.official {
        let parts: Vec<&str> = domain.split('.').collect();
        let mut i = 0;
        while i + parts.len() < labels.len() {
            if parts
                .iter()
                .enumerate()
                .all(|(k, part)| labels[i + k] == *part)
            {
                return true;
            }
            i += 1;
        }
    }
    let name = skeleton(main_label(&labels));
    for (keys, _) in &rules.brands {
        for key in keys {
            if name == *key {
                return true;
            }
            if key.len() >= 6
                && name.len().abs_diff(key.len()) <= 1
                && typo_distance(&name, key) == 1
            {
                return true;
            }
            if key.len() >= 5 {
                let mut from = 0;
                while let Some(found) = name.get(from..).and_then(|rest| rest.find(key.as_str())) {
                    let at = from + found;
                    let before = &name[..at];
                    let after = &name[at + key.len()..];
                    if (!before.is_empty() || !after.is_empty()) && lures(before) && lures(after) {
                        return true;
                    }
                    from = at + 1;
                }
            }
        }
    }
    false
}

/// A letter as the scam check reads it: lower case (`I` and `İ` both read `i`), accents and
/// full-width forms as plain Latin letters; None for anything else.
fn plain_letter(c: char) -> Option<char> {
    let c = match c {
        'I' | 'İ' | 'ı' => return Some('i'),
        'Ａ'..='Ｚ' => char::from_u32(c as u32 - 'Ａ' as u32 + 'a' as u32)?,
        'ａ'..='ｚ' => char::from_u32(c as u32 - 'ａ' as u32 + 'a' as u32)?,
        _ => c.to_lowercase().next().unwrap_or(c),
    };
    let base = match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => 'a',
        'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => 'c',
        'ď' => 'd',
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => 'e',
        'ĝ' | 'ğ' | 'ġ' | 'ģ' => 'g',
        'ĥ' => 'h',
        'ì' | 'í' | 'î' | 'ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' => 'i',
        'ĵ' => 'j',
        'ķ' => 'k',
        'ĺ' | 'ļ' | 'ľ' => 'l',
        'ñ' | 'ń' | 'ņ' | 'ň' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ō' | 'ŏ' | 'ő' => 'o',
        'ŕ' | 'ŗ' | 'ř' => 'r',
        'ś' | 'ŝ' | 'ş' | 'š' => 's',
        'ţ' | 'ť' => 't',
        'ù' | 'ú' | 'û' | 'ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => 'u',
        'ŵ' => 'w',
        'ý' | 'ÿ' | 'ŷ' => 'y',
        'ź' | 'ż' | 'ž' => 'z',
        '0' => 'o',
        '1' | '!' => 'i',
        '3' => 'e',
        '4' | '@' => 'a',
        '5' | '$' => 's',
        '7' => 't',
        other => other,
    };
    base.is_ascii_lowercase().then_some(base)
}

/// Words apart, as the scam phrases are matched: ` free nitro `.
fn scam_form(text: &str) -> String {
    let mut out = String::from(" ");
    for c in text.chars() {
        // Combining marks leave the letter they sit on as it is.
        if ('\u{300}'..='\u{36f}').contains(&c) {
            continue;
        }
        match plain_letter(c) {
            Some(letter) => out.push(letter),
            None if !out.ends_with(' ') => out.push(' '),
            None => {}
        }
    }
    if !out.ends_with(' ') {
        out.push(' ');
    }
    out
}

fn scammy(text: &str) -> bool {
    let form = scam_form(text);
    rules()
        .scams
        .iter()
        .any(|phrase| form.contains(&format!(" {phrase} ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    const VECTORS: &str =
        include_str!("../../../apps/launcher/src/features/social/chat/text-vectors.json");

    #[derive(Deserialize)]
    struct Shared {
        links: LinkCases,
    }

    #[derive(Deserialize)]
    struct LinkCases {
        about: String,
        lists: ListsCase,
        vectors: Vec<LinkVector>,
    }

    #[derive(Deserialize)]
    struct ListsCase {
        trusted: Vec<String>,
        blocked: Vec<String>,
        invites: bool,
    }

    #[derive(Deserialize)]
    struct LinkVector {
        name: String,
        text: String,
        reason: Option<String>,
        rule: Option<String>,
        #[serde(default)]
        links: Vec<LinkCase>,
        #[serde(default)]
        off: bool,
        invites: Option<bool>,
    }

    #[derive(Deserialize)]
    struct LinkCase {
        url: String,
        trusted: bool,
    }

    /// The cases the service's whole-text check turns away for what lies outside the links (CHAT1's
    /// checks of the rest of the text, or links switched off): a shown message passed them already.
    const OUTSIDE_THE_LINKS: [&str; 9] = [
        "links switched off",
        "a hidden link while links are off",
        "a bare domain beside a link",
        "a bare domain",
        "a hidden dot",
        "another scheme",
        "a phone number beside a link",
        "the CHAT1 vectors still hold: discord.gg in regional indicators",
        "the CHAT1 vectors still hold: emoji between digit groups",
    ];

    #[test]
    fn the_table_parses_and_is_the_one_the_vectors_were_made_for() {
        let shared: Shared = serde_json::from_str(VECTORS).unwrap();
        assert!(version() >= 2);
        assert!(
            shared
                .links
                .about
                .contains(&format!("rule table version {}", version()))
        );
        assert!(shared.links.vectors.len() > 100);
        assert!(default_lists().trusted.iter().any(|t| t == "youtube.com"));
    }

    #[test]
    fn every_shared_case_the_link_rules_decide() {
        let shared: Shared = serde_json::from_str(VECTORS).unwrap();
        let mut failures = Vec::new();
        let mut outside = Vec::new();
        for vector in &shared.links.vectors {
            let lists = LinkLists {
                trusted: shared.links.lists.trusted.clone(),
                blocked: shared.links.lists.blocked.clone(),
                invites: vector.invites.unwrap_or(shared.links.lists.invites),
            };
            let got = check(&vector.text, &lists);
            let want_reason = vector.reason.as_deref();
            match (&got, want_reason) {
                (Ok(links), None) => {
                    let got: Vec<(&str, bool)> =
                        links.iter().map(|l| (l.url.as_str(), l.trusted)).collect();
                    let want: Vec<(&str, bool)> = vector
                        .links
                        .iter()
                        .map(|l| (l.url.as_str(), l.trusted))
                        .collect();
                    if got != want && !vector.off {
                        failures.push(format!("{}: links {got:?}, want {want:?}", vector.name));
                    }
                    // What may be pressed is what may go.
                    if pressable(&vector.text, &lists).len() != want.len() {
                        failures.push(format!("{}: pressable count", vector.name));
                    }
                }
                (Ok(_), Some(_)) => outside.push(vector.name.clone()),
                (Err(problem), Some("phone")) if *problem == Problem::Phone => {}
                (Err(problem), Some("link")) if Some(problem.name()) == vector.rule.as_deref() => {
                    // A link that may not go may not be pressed either.
                    if pressable(&vector.text, &lists).len()
                        == pl_social::find_links(&vector.text).len()
                        && *problem != Problem::Count
                        && *problem != Problem::Scam
                    {
                        failures.push(format!("{}: pressable though refused", vector.name));
                    }
                }
                (got, want) => failures.push(format!(
                    "{}: got {got:?}, want {want:?} {:?}",
                    vector.name, vector.rule
                )),
            }
        }
        assert_eq!(failures, Vec::<String>::new());
        outside.sort();
        let mut known: Vec<String> = OUTSIDE_THE_LINKS.iter().map(|s| (*s).to_owned()).collect();
        known.sort();
        assert_eq!(outside, known);
    }

    #[test]
    fn links_may_be_pressed_only_while_they_pass_the_rules_as_they_are_now() {
        let text = "https://youtu.be/a https://www.planetminecraft.com/x https://bit.ly/y";
        let lists = default_lists();
        let pressed = pressable(text, &lists);
        assert_eq!(
            pressed,
            [
                Pressable {
                    url: "https://youtu.be/a".into(),
                    trusted: true
                },
                Pressable {
                    url: "https://www.planetminecraft.com/x".into(),
                    trusted: false
                },
            ]
        );
        assert_eq!(
            problem("https://bit.ly/y", &lists),
            Some(Problem::Shortener)
        );
        // A site Studio blocks later turns back into plain text in older messages.
        let blocked = LinkLists {
            blocked: vec!["youtu.be".into()],
            ..default_lists()
        };
        assert_eq!(pressable(text, &blocked).len(), 1);
        // Only the first three links count; http never opens.
        let four = "https://a.com/1 https://b.com/2 https://c.com/3 https://d.com/4";
        assert_eq!(pressable(four, &lists).len(), 3);
        assert!(pressable("http://youtu.be/a", &lists).is_empty());
    }

    #[test]
    fn brand_names_read_as_the_eye_reads_them() {
        assert_eq!(skeleton("d1scord"), skeleton("discord"));
        assert_eq!(typo_distance("dslcord", "dlscord"), 1);
        assert!(lookalike("discordapp.xyz"));
        assert!(!lookalike("cdn.discordapp.com"));
        assert!(scammy("Bedava Robux burada"));
        assert!(scammy("FR33 N1TR0"));
        assert!(!scammy("selam nasılsın"));
        assert_eq!(decoded("a%20b"), "a b");
        assert_eq!(decoded("%E0%A4%A"), "%E0%A4%A");
        assert!(phone_in("send?phone=905321234567&text=x"));
        assert!(!phone_in("watch?v=123456789"));
    }
}

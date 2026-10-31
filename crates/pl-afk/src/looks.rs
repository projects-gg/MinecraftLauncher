//! A console line's look on this computer (AFK-UX: CONSOLE-COLOR's looks in the desktop console).
//!
//! A client 4.2.4 started with the console kinds may give a line event its look: `st`, the parts
//! of the plain `text` in the game's colours and flags, and `ev`, what a part shows on hover or
//! offers on a click (the contract is 7/24 Bulut's: the client fork's `docs/agent-bridge.md`,
//! "Client 4.2.4: console line looks"). It is read strictly here ([`read`]), every click checked
//! against the contract's one rule list ([`click_fits`]); the account's secrets come next: a line
//! the mask changes has no look (the session's rule), an event with a string the mask changes is
//! dropped ([`unmask`]). A line the console cuts is cut with its look ([`cut`]), and one line's
//! look stays within [`MAX_LINE_BYTES`] ([`fit`]). The view checks all of it again
//! (`@pl/ui/console`); `text` stays the line for search, copy, the log and everything else.
//!
//! This computer has no item tables: an item's hover is dropped (its part keeps its colour), and
//! an entity's hover keeps its own name only.

use serde_json::{Map, Value, json};

/// Most parts of a line's look.
pub const MAX_SPANS: usize = 64;
/// Most events of a line.
pub const MAX_EVENTS: usize = 8;
/// Longest click value, in UTF-16 code units (a longer one is never cut: its event goes).
pub const MAX_VALUE_UNITS: usize = 256;
/// Longest hover text, in UTF-16 code units, and its most lines and parts.
pub const MAX_HOVER_UNITS: usize = 256;
pub const MAX_HOVER_LINES: usize = 8;
pub const MAX_HOVER_SPANS: usize = 16;
/// Longest entity name, in UTF-16 code units.
pub const MAX_NAME_UNITS: usize = 64;
/// Largest look of one line, in bytes of the JSON of `st` and `ev`.
pub const MAX_LINE_BYTES: usize = 4096;

/// The click actions a look may carry; the client never writes another one.
const ACTIONS: [&str; 4] = [
    "run_command",
    "suggest_command",
    "open_url",
    "copy_to_clipboard",
];

/// One part of a look: `units` UTF-16 code units of the text in `code`'s colour and flags (empty:
/// the line's own), pointing at event `ev`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub units: usize,
    pub code: String,
    pub ev: Option<usize>,
}

/// A part's click: `a` one of the four actions, `v` its command, address or text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Click {
    pub a: String,
    pub v: String,
}

/// What pointing at a part shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hover {
    /// The server's text (its own look, no events: depth 1).
    Text { text: String, st: Option<Vec<Span>> },
    /// An entity's own name (this computer knows no type names).
    Entity { name: String, st: Option<Vec<Span>> },
}

/// What a part does: a click, a hover, or both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LookEvent {
    pub click: Option<Click>,
    pub hover: Option<Hover>,
}

/// A line's look: the parts covering its text exactly, and the events they point at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineLooks {
    pub st: Vec<Span>,
    pub ev: Vec<LookEvent>,
}

/// UTF-16 code units of `text` (the contract's lengths).
pub fn units(text: &str) -> usize {
    text.chars().map(char::len_utf16).sum()
}

/// The contract's one list of characters no click value, card text or hover carries: controls (C0,
/// DEL, C1), the soft hyphen, the combining grapheme joiner, the Arabic letter mark, the Hangul and
/// Mongolian fillers, the zero-width spaces, joiners and direction marks, the separators,
/// embeddings and overrides, the invisible operators, the isolates, U+FEFF, the halfwidth Hangul
/// filler and the tag characters.
pub fn is_forbidden(c: char) -> bool {
    let u = u32::from(c);
    u <= 0x1f
        || (0x7f..=0x9f).contains(&u)
        || matches!(
            u,
            0xad | 0x34f | 0x61c | 0x115f | 0x1160 | 0x180e | 0x3164 | 0xfeff | 0xffa0
        )
        || (0x200b..=0x200f).contains(&u)
        || (0x2028..=0x202e).contains(&u)
        || (0x2060..=0x2064).contains(&u)
        || (0x2066..=0x2069).contains(&u)
        || (0xe0000..=0xe007f).contains(&u)
}

/// A text of 1 to `max` units without the rule list's characters.
fn plain_value(value: &str, max: usize) -> bool {
    let n = units(value);
    (1..=max).contains(&n) && !value.chars().any(is_forbidden)
}

/// `^(?:[0-9a-f]|#[0-9a-f]{6})?k?l?m?n?o?$`: a colour (the game's 16 or a hex one) and flags.
pub fn is_style_code(code: &str) -> bool {
    let bytes = code.as_bytes();
    let hex = |b: &u8| b.is_ascii_digit() || (b'a'..=b'f').contains(b);
    let mut at = 0;
    if bytes.first() == Some(&b'#') {
        if bytes.len() < 7 || !bytes[1..7].iter().all(hex) {
            return false;
        }
        at = 7;
    } else if bytes.first().is_some_and(hex) {
        at = 1;
    }
    let mut next = b'k';
    for &flag in &bytes[at..] {
        if !(next..=b'o').contains(&flag) {
            return false;
        }
        next = flag + 1;
    }
    true
}

/// `^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$`, any letter case.
fn is_label(label: &str) -> bool {
    let bytes = label.as_bytes();
    let edge = |b: Option<&u8>| b.is_some_and(u8::is_ascii_alphanumeric);
    bytes.len() <= 63
        && edge(bytes.first())
        && edge(bytes.last())
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'-')
}

/// An address the console may open: `https://`, no white space and none of the rule list's
/// characters, no sign-in before the host (`@`), a port of 1–5 digits, and a host of at least two
/// plain ASCII labels: no IDN (Unicode or `xn--`), no IP address (v4, v6 or a number), no
/// `localhost`; and a URL parser reads the same host (`@pl/ui` `urlParts`, the Worker and the
/// client have the same rule).
pub fn is_https_url(value: &str) -> bool {
    let Some(tail) = value.strip_prefix("https://") else {
        return false;
    };
    if value
        .chars()
        .any(|c| c.is_whitespace() || c == '\u{85}' || is_forbidden(c))
    {
        return false;
    }
    let end = tail.find(['/', '?', '#']).unwrap_or(tail.len());
    let authority = &tail[..end];
    if authority.contains('@') || authority.starts_with('[') {
        return false;
    }
    let (host, port) = match authority.split_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (authority, None),
    };
    if let Some(port) = port
        && (!(1..=5).contains(&port.len()) || !port.bytes().all(|b| b.is_ascii_digit()))
    {
        return false;
    }
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() < 2 || !labels.iter().all(|label| is_label(label)) {
        return false;
    }
    if labels
        .iter()
        .any(|label| label.len() >= 4 && label[..4].eq_ignore_ascii_case("xn--"))
    {
        return false;
    }
    if labels
        .last()
        .is_some_and(|label| label.bytes().all(|b| b.is_ascii_digit()))
    {
        return false;
    }
    let lower = host.to_ascii_lowercase();
    if lower == "localhost" || lower.ends_with(".localhost") {
        return false;
    }
    match url::Url::parse(value) {
        Ok(url) => {
            url.scheme() == "https"
                && url.host_str() == Some(lower.as_str())
                && url.username().is_empty()
                && url.password().is_none()
        }
        Err(_) => false,
    }
}

/// A click the contract allows: one of the four actions with a value of 1–256 units outside the
/// rule list; a command starts with `/` (and has more than it), an address is [`is_https_url`].
pub fn click_fits(action: &str, value: &str) -> bool {
    ACTIONS.contains(&action)
        && plain_value(value, MAX_VALUE_UNITS)
        && match action {
            "run_command" => value.starts_with('/') && units(value) >= 2,
            "open_url" => is_https_url(value),
            _ => true,
        }
}

/// The parts of `text` as `st` gives them: 1 or more units each with a code of the contract,
/// covering the text exactly, never ending between a character's halves, at most `max`; an event
/// index only below `events`. None when any of that fails.
fn read_spans(value: &Value, text: &str, max: usize, events: usize) -> Option<Vec<Span>> {
    let parts = value.as_array()?;
    if parts.is_empty() || parts.len() > max {
        return None;
    }
    // Where each character starts, in units: a part may only end at one of them (or the end).
    let mut starts = Vec::with_capacity(text.len());
    let mut total = 0;
    for c in text.chars() {
        starts.push(total);
        total += c.len_utf16();
    }
    let mut spans = Vec::with_capacity(parts.len());
    let mut at = 0usize;
    for part in parts {
        let part = part.as_array()?;
        if !(2..=3).contains(&part.len()) {
            return None;
        }
        let n = usize::try_from(part[0].as_u64()?).ok()?;
        let code = part[1].as_str()?;
        if n == 0 || !is_style_code(code) {
            return None;
        }
        let ev = match part.get(2) {
            None => None,
            Some(index) => {
                let index = usize::try_from(index.as_u64()?).ok()?;
                if index >= events {
                    return None;
                }
                Some(index)
            }
        };
        at = at.checked_add(n)?;
        if at > total || (at < total && starts.binary_search(&at).is_err()) {
            return None;
        }
        spans.push(Span {
            units: n,
            code: code.to_owned(),
            ev,
        });
    }
    (at == total).then_some(spans)
}

fn read_click(value: &Value) -> Option<Click> {
    let object = value.as_object()?;
    let a = object.get("a")?.as_str()?;
    let v = object.get("v")?.as_str()?;
    click_fits(a, v).then(|| Click {
        a: a.to_owned(),
        v: v.to_owned(),
    })
}

fn read_hover(value: &Value) -> Option<Hover> {
    let object = value.as_object()?;
    if let Some(text) = object.get("text") {
        let text = text.as_str()?;
        let n = units(text);
        if n == 0
            || n > MAX_HOVER_UNITS
            || text.split('\n').count() > MAX_HOVER_LINES
            || text.chars().any(|c| c != '\n' && is_forbidden(c))
        {
            return None;
        }
        let st = object
            .get("st")
            .and_then(|st| read_spans(st, text, MAX_HOVER_SPANS, 0));
        return Some(Hover::Text {
            text: text.to_owned(),
            st,
        });
    }
    let entity = object.get("entity")?.as_object()?;
    // Without its name there is nothing to show here: the type's name needs tables.
    let name = entity.get("name")?.as_str()?;
    if !plain_value(name, MAX_NAME_UNITS) {
        return None;
    }
    let st = entity
        .get("st")
        .and_then(|st| read_spans(st, name, MAX_HOVER_SPANS, 0));
    Some(Hover::Entity {
        name: name.to_owned(),
        st,
    })
}

fn read_event(value: &Value) -> Option<LookEvent> {
    let object = value.as_object()?;
    let click = object.get("click").and_then(read_click);
    let hover = object.get("hover").and_then(read_hover);
    (click.is_some() || hover.is_some()).then_some(LookEvent { click, hover })
}

/// Keeps the events that stand, in their order, and points the parts at their new places; a part
/// whose event went keeps its look without one, an event no part points at goes.
fn renumber(spans: Vec<Span>, events: Vec<Option<LookEvent>>) -> LineLooks {
    let mut used = vec![false; events.len()];
    for span in &spans {
        if let Some(index) = span.ev
            && events.get(index).is_some_and(Option::is_some)
        {
            used[index] = true;
        }
    }
    let mut map = vec![None; events.len()];
    let mut kept = Vec::new();
    for (index, event) in events.into_iter().enumerate() {
        if let Some(event) = event
            && used[index]
        {
            map[index] = Some(kept.len());
            kept.push(event);
        }
    }
    let st = spans
        .into_iter()
        .map(|span| Span {
            ev: span.ev.and_then(|index| map.get(index).copied().flatten()),
            ..span
        })
        .collect();
    LineLooks { st, ev: kept }
}

/// The look a line event gives, read strictly: parts that do not read, or do not cover `text`
/// exactly, leave the line without a look; an event that does not read is dropped (its parts keep
/// their colours), and so are all of them when `ev` is not a list of at most [`MAX_EVENTS`].
pub fn read(st: Option<&Value>, ev: Option<&Value>, text: &str) -> Option<LineLooks> {
    let spans = read_spans(st?, text, MAX_SPANS, MAX_EVENTS)?;
    let events = match ev {
        Some(Value::Array(events)) if events.len() <= MAX_EVENTS => {
            events.iter().map(read_event).collect()
        }
        _ => Vec::new(),
    };
    Some(renumber(spans, events))
}

/// The look with the account's secrets kept out: an event whose click value, hover text or name
/// the mask changes is dropped. (A line whose text the mask changes gets no look at all: the
/// session's rule.)
pub fn unmask(looks: LineLooks, mask: &dyn Fn(&str) -> String) -> LineLooks {
    let same = |text: &str| mask(text) == text;
    let events = looks
        .ev
        .into_iter()
        .map(|event| {
            let clean = event.click.as_ref().is_none_or(|click| same(&click.v))
                && match &event.hover {
                    None => true,
                    Some(Hover::Text { text, .. }) => same(text),
                    Some(Hover::Entity { name, .. }) => same(name),
                };
            clean.then_some(event)
        })
        .collect();
    renumber(looks.st, events)
}

/// The look of a line cut after `kept` units of its text with "…" after them (the console's cut):
/// the parts up to there, the part across the cut shortened, then "…" in the line's own look. A
/// part that lost text loses its event, and an event of a dropped part goes.
pub fn cut(looks: LineLooks, kept: usize) -> LineLooks {
    let mut st = Vec::new();
    let mut at = 0;
    for span in looks.st {
        if at >= kept {
            break;
        }
        let end = at + span.units;
        if end <= kept {
            st.push(span);
        } else {
            st.push(Span {
                units: kept - at,
                ev: None,
                ..span
            });
        }
        at = end;
    }
    st.push(Span {
        units: 1,
        code: String::new(),
        ev: None,
    });
    let events = looks.ev.into_iter().map(Some).collect();
    renumber(st, events)
}

/// `st` as the contract writes it.
pub fn spans_json(spans: &[Span]) -> Value {
    Value::Array(
        spans
            .iter()
            .map(|span| match span.ev {
                Some(index) => json!([span.units, span.code, index]),
                None => json!([span.units, span.code]),
            })
            .collect(),
    )
}

/// `ev` as the contract writes it (an entity without its type's name).
pub fn events_json(events: &[LookEvent]) -> Value {
    Value::Array(
        events
            .iter()
            .map(|event| {
                let mut out = Map::new();
                if let Some(click) = &event.click {
                    out.insert("click".into(), json!({ "a": click.a, "v": click.v }));
                }
                if let Some(hover) = &event.hover {
                    let hover = match hover {
                        Hover::Text { text, st } => {
                            let mut text = json!({ "text": text });
                            if let Some(st) = st {
                                text["st"] = spans_json(st);
                            }
                            text
                        }
                        Hover::Entity { name, st } => {
                            let mut entity = json!({ "name": name });
                            if let Some(st) = st {
                                entity["st"] = spans_json(st);
                            }
                            json!({ "entity": entity })
                        }
                    };
                    out.insert("hover".into(), hover);
                }
                Value::Object(out)
            })
            .collect(),
    )
}

/// Bytes of the look's JSON (`st`, and `ev` when it has events).
pub fn json_bytes(looks: &LineLooks) -> usize {
    let st = spans_json(&looks.st).to_string().len();
    if looks.ev.is_empty() {
        st
    } else {
        st + events_json(&looks.ev).to_string().len()
    }
}

/// One line's look within [`MAX_LINE_BYTES`] (its hovers go first, then its events, then the look),
/// and only when it shows something: parts that are all the line's own look and do nothing are
/// no look.
pub fn fit(looks: LineLooks) -> Option<LineLooks> {
    let meaningful = |looks: LineLooks| {
        looks
            .st
            .iter()
            .any(|span| !span.code.is_empty() || span.ev.is_some())
            .then_some(looks)
    };
    if json_bytes(&looks) <= MAX_LINE_BYTES {
        return meaningful(looks);
    }
    let events = looks
        .ev
        .into_iter()
        .map(|event| {
            event.click.map(|click| LookEvent {
                click: Some(click),
                hover: None,
            })
        })
        .collect();
    let looks = renumber(looks.st, events);
    if json_bytes(&looks) <= MAX_LINE_BYTES {
        return meaningful(looks);
    }
    let looks = LineLooks {
        st: looks
            .st
            .into_iter()
            .map(|span| Span { ev: None, ..span })
            .collect(),
        ev: Vec::new(),
    };
    if json_bytes(&looks) <= MAX_LINE_BYTES {
        meaningful(looks)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn looks(st: Value, ev: Value, text: &str) -> Option<LineLooks> {
        read(Some(&st), Some(&ev), text)
    }

    #[test]
    fn style_codes_are_the_contract_s() {
        for good in [
            "", "a", "0", "f", "#a1b2c3", "l", "klmno", "ckn", "#000000o",
        ] {
            assert!(is_style_code(good), "{good}");
        }
        for bad in [
            "g", "A", "#A1B2C3", "#12345", "lk", "ll", "x", "#1234567", "aa", "kp",
        ] {
            assert!(!is_style_code(bad), "{bad}");
        }
    }

    #[test]
    fn addresses_follow_the_one_rule() {
        for good in [
            "https://projects.gg",
            "https://forum.projects.gg/threads/1?x=y#top",
            "https://shop.example.com.tr:8443/a",
        ] {
            assert!(is_https_url(good), "{good}");
            assert!(click_fits("open_url", good), "{good}");
        }
        for bad in [
            "http://projects.gg",
            "https://projects",
            "https://user@projects.gg",
            "https://projects.gg:80800",
            "https://127.0.0.1",
            "https://[::1]/",
            "https://xn--prjects-1ya.gg",
            "https://proj\u{435}cts.gg",
            "https://a.localhost",
            "https://projects.gg/a b",
            "https://projects.gg/\u{202e}x",
            "https://-a.gg",
        ] {
            assert!(!is_https_url(bad), "{bad}");
        }
    }

    #[test]
    fn clicks_keep_to_their_action_and_the_rule_list() {
        assert!(click_fits("run_command", "/spawn"));
        assert!(!click_fits("run_command", "spawn"));
        assert!(!click_fits("run_command", "/"));
        assert!(click_fits("suggest_command", "/msg Kaan_07 "));
        assert!(click_fits("copy_to_clipboard", "kod-42"));
        assert!(!click_fits("copy_to_clipboard", "kod\u{200b}42"));
        assert!(!click_fits("copy_to_clipboard", ""));
        assert!(!click_fits("change_page", "2"));
        assert!(!click_fits("run_command", &format!("/{}", "a".repeat(256))));
        assert!(click_fits("run_command", &format!("/{}", "a".repeat(255))));
        // A tag character is forbidden too (outside the BMP).
        assert!(!click_fits("copy_to_clipboard", "a\u{e0041}"));
    }

    #[test]
    fn a_look_reads_strictly() {
        let text = "[VIP] Kaan_07: selam";
        let read = looks(
            json!([[5, "6l"], [1, ""], [7, "e", 0], [7, ""]]),
            json!([{ "click": { "a": "suggest_command", "v": "/msg Kaan_07 " } }]),
            text,
        )
        .unwrap();
        assert_eq!(read.st.len(), 4);
        assert_eq!(read.st[2].ev, Some(0));
        assert_eq!(read.ev.len(), 1);

        // Not covering the text, a bad code, an empty part, a missing look: no look.
        for st in [
            json!([[5, ""]]),
            json!([[5, "z"], [15, ""]]),
            json!([[0, ""], [20, ""]]),
            json!([]),
            json!("6l"),
        ] {
            assert_eq!(looks(st, json!([]), text), None);
        }
        assert_eq!(
            read_spans(&json!([[20, ""]]), text, MAX_SPANS, 0)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(super::read(None, None, text), None);

        // Never between a character's halves.
        let emoji = "a😀b";
        assert_eq!(units(emoji), 4);
        assert!(looks(json!([[2, "c"], [2, ""]]), json!([]), emoji).is_none());
        assert!(looks(json!([[3, "c"], [1, ""]]), json!([]), emoji).is_some());
    }

    #[test]
    fn odd_events_drop_and_their_parts_keep_their_colours() {
        let text = "Tikla veya git";
        let read = looks(
            json!([[5, "a", 0], [6, ""], [3, "b", 1]]),
            json!([
                { "click": { "a": "run_command", "v": "spawn" } },
                { "click": { "a": "open_url", "v": "https://projects.gg" } }
            ]),
            text,
        )
        .unwrap();
        assert_eq!(read.ev.len(), 1);
        assert_eq!(read.st[0].ev, None);
        assert_eq!(read.st[0].code, "a");
        assert_eq!(read.st[2].ev, Some(0));

        // Too many events, or none readable: the parts stay, without events.
        let many: Vec<Value> = (0..9)
            .map(|_| json!({ "click": { "a": "copy_to_clipboard", "v": "x" } }))
            .collect();
        let read = looks(json!([[14, "c", 0]]), Value::Array(many), text).unwrap();
        assert!(read.ev.is_empty());
        assert_eq!(read.st[0].ev, None);

        // An item's hover: dropped here; an entity keeps its name.
        let read = looks(
            json!([[5, "", 0], [9, "", 1]]),
            json!([
                { "hover": { "item": { "id": "minecraft:diamond_sword" } } },
                { "hover": { "entity": { "id": "minecraft:zombie", "name": "Zombi", "st": [[5, "c"]] } } }
            ]),
            text,
        )
        .unwrap();
        assert_eq!(read.ev.len(), 1);
        assert_eq!(
            read.ev[0].hover,
            Some(Hover::Entity {
                name: "Zombi".into(),
                st: Some(vec![Span {
                    units: 5,
                    code: "c".into(),
                    ev: None
                }])
            })
        );
        assert_eq!(read.st[0].ev, None);
        assert_eq!(read.st[1].ev, Some(0));

        // A hover's parts carry no events (depth 1); a long hover is not shown.
        let read = looks(
            json!([[14, "", 0]]),
            json!([{ "hover": { "text": "Merhaba\nDünya", "st": [[7, "a", 0], [6, ""]] } }]),
            text,
        )
        .unwrap();
        assert_eq!(
            read.ev[0].hover,
            Some(Hover::Text {
                text: "Merhaba\nDünya".into(),
                st: None
            })
        );
        let long = "a".repeat(MAX_HOVER_UNITS + 1);
        assert!(
            looks(
                json!([[14, "", 0]]),
                json!([{ "hover": { "text": long } }]),
                text
            )
            .unwrap()
            .ev
            .is_empty()
        );
    }

    #[test]
    fn the_mask_drops_an_event_it_changes() {
        let text = "Kodun: tikla";
        let read = looks(
            json!([[7, ""], [5, "a", 0]]),
            json!([{ "click": { "a": "copy_to_clipboard", "v": "/login gizli123" } }]),
            text,
        )
        .unwrap();
        let mask = |s: &str| s.replace("gizli123", "******");
        let unmasked = unmask(read.clone(), &mask);
        assert!(unmasked.ev.is_empty());
        assert_eq!(unmasked.st[1].ev, None);
        assert_eq!(unmasked.st[1].code, "a");
        assert_eq!(unmask(read.clone(), &|s: &str| s.to_owned()), read);
    }

    #[test]
    fn a_cut_line_keeps_the_parts_before_the_cut() {
        let looks = LineLooks {
            st: vec![
                Span {
                    units: 3,
                    code: "a".into(),
                    ev: Some(0),
                },
                Span {
                    units: 4,
                    code: "b".into(),
                    ev: Some(1),
                },
                Span {
                    units: 3,
                    code: "c".into(),
                    ev: None,
                },
            ],
            ev: vec![
                LookEvent {
                    click: Some(Click {
                        a: "copy_to_clipboard".into(),
                        v: "x".into(),
                    }),
                    hover: None,
                },
                LookEvent {
                    click: Some(Click {
                        a: "copy_to_clipboard".into(),
                        v: "y".into(),
                    }),
                    hover: None,
                },
            ],
        };
        let cut = cut(looks, 5);
        assert_eq!(
            cut.st,
            vec![
                Span {
                    units: 3,
                    code: "a".into(),
                    ev: Some(0)
                },
                Span {
                    units: 2,
                    code: "b".into(),
                    ev: None
                },
                Span {
                    units: 1,
                    code: String::new(),
                    ev: None
                },
            ]
        );
        assert_eq!(cut.ev.len(), 1);
        assert_eq!(cut.ev[0].click.as_ref().unwrap().v, "x");
    }

    #[test]
    fn a_large_look_sheds_hovers_then_events_then_itself() {
        let text = "x".repeat(64);
        let hover = "h".repeat(MAX_HOVER_UNITS);
        let st: Vec<Value> = (0..8).map(|i| json!([8, "a", i])).collect();
        let ev: Vec<Value> = (0..8)
            .map(|_| {
                json!({
                    "click": { "a": "copy_to_clipboard", "v": "v".repeat(200) },
                    "hover": { "text": hover }
                })
            })
            .collect();
        let read = looks(Value::Array(st), Value::Array(ev), &text).unwrap();
        assert!(json_bytes(&read) > MAX_LINE_BYTES);
        let fitted = fit(read).unwrap();
        assert!(json_bytes(&fitted) <= MAX_LINE_BYTES);
        assert_eq!(fitted.ev.len(), 8);
        assert!(fitted.ev.iter().all(|event| event.hover.is_none()));

        // Parts that are all the line's own look and do nothing: no look.
        let plain = looks(json!([[3, ""], [2, ""]]), json!([]), "hello").unwrap();
        assert_eq!(fit(plain), None);
    }

    #[test]
    fn the_json_is_the_contract_s() {
        let looks = looks(
            json!([[2, "a", 0], [3, ""]]),
            json!([{
                "click": { "a": "run_command", "v": "/spawn" },
                "hover": { "entity": { "name": "Kaan_07" } }
            }]),
            "hello",
        )
        .unwrap();
        assert_eq!(spans_json(&looks.st), json!([[2, "a", 0], [3, ""]]));
        assert_eq!(
            events_json(&looks.ev),
            json!([{
                "click": { "a": "run_command", "v": "/spawn" },
                "hover": { "entity": { "name": "Kaan_07" } }
            }])
        );
    }
}

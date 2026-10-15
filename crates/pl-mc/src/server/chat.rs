//! Minecraft formatted text: JSON chat components and legacy `§` formatting codes, turned into
//! plain text and styled spans the UI can render in color.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// A run of text sharing one style. `color` is `#rrggbb` (lowercase) when set.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatSpan {
    pub text: String,
    pub color: Option<String>,
    pub bold: bool,
    pub italic: bool,
    pub underlined: bool,
    pub strikethrough: bool,
    pub obfuscated: bool,
}

/// Formatted game text in three shapes: plain (codes removed), styled spans, and the raw source
/// (a JSON string possibly containing `§` codes, or a chat component).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatText {
    pub plain: String,
    pub spans: Vec<ChatSpan>,
    pub raw: Value,
}

/// Colors of the legacy codes `§0`..`§f`, also used for named JSON colors.
const LEGACY_COLORS: [(&str, &str); 16] = [
    ("black", "#000000"),
    ("dark_blue", "#0000aa"),
    ("dark_green", "#00aa00"),
    ("dark_aqua", "#00aaaa"),
    ("dark_red", "#aa0000"),
    ("dark_purple", "#aa00aa"),
    ("gold", "#ffaa00"),
    ("gray", "#aaaaaa"),
    ("dark_gray", "#555555"),
    ("blue", "#5555ff"),
    ("green", "#55ff55"),
    ("aqua", "#55ffff"),
    ("red", "#ff5555"),
    ("light_purple", "#ff55ff"),
    ("yellow", "#ffff55"),
    ("white", "#ffffff"),
];

/// Deep component trees are legal but pointless; this bounds recursion on hostile input.
const MAX_DEPTH: usize = 48;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Style {
    color: Option<String>,
    bold: bool,
    italic: bool,
    underlined: bool,
    strikethrough: bool,
    obfuscated: bool,
}

impl Style {
    fn span(&self, text: String) -> ChatSpan {
        ChatSpan {
            text,
            color: self.color.clone(),
            bold: self.bold,
            italic: self.italic,
            underlined: self.underlined,
            strikethrough: self.strikethrough,
            obfuscated: self.obfuscated,
        }
    }

    fn same_as(&self, span: &ChatSpan) -> bool {
        self.color == span.color
            && self.bold == span.bold
            && self.italic == span.italic
            && self.underlined == span.underlined
            && self.strikethrough == span.strikethrough
            && self.obfuscated == span.obfuscated
    }
}

/// Parse any chat value: a string (legacy codes allowed), a component object, or an array.
pub fn parse_chat(value: &Value) -> ChatText {
    let mut spans = Vec::new();
    walk(value, &Style::default(), &mut spans, 0);
    finish(spans, value.clone())
}

/// Parse a plain string that may contain legacy `§` codes.
pub fn parse_legacy(text: &str) -> ChatText {
    let mut spans = Vec::new();
    push_legacy(text, &Style::default(), &mut spans);
    finish(spans, Value::String(text.to_owned()))
}

/// Plain text of a chat value with every formatting code removed.
pub fn to_plain(value: &Value) -> String {
    parse_chat(value).plain
}

/// Remove legacy `§x` formatting codes from a string.
pub fn strip_formatting(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '§' {
            chars.next();
        } else {
            out.push(c);
        }
    }
    out
}

fn finish(spans: Vec<ChatSpan>, raw: Value) -> ChatText {
    let plain = spans.iter().map(|s| s.text.as_str()).collect();
    ChatText { plain, spans, raw }
}

fn walk(value: &Value, parent: &Style, out: &mut Vec<ChatSpan>, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    match value {
        Value::Null => {}
        Value::String(s) => push_legacy(s, parent, out),
        Value::Bool(b) => push_legacy(&b.to_string(), parent, out),
        Value::Number(n) => push_legacy(&n.to_string(), parent, out),
        Value::Array(items) => {
            // The first element is the parent of the others, which inherit its style.
            let mut iter = items.iter();
            let Some(first) = iter.next() else { return };
            let style = match first {
                Value::Object(obj) => apply_style(parent, obj),
                _ => parent.clone(),
            };
            walk(first, parent, out, depth + 1);
            for item in iter {
                walk(item, &style, out, depth + 1);
            }
        }
        Value::Object(obj) => {
            let style = apply_style(parent, obj);
            if let Some(text) = component_text(obj, depth) {
                push_legacy(&text, &style, out);
            }
            if let Some(Value::Array(extra)) = obj.get("extra") {
                for item in extra {
                    walk(item, &style, out, depth + 1);
                }
            }
        }
    }
}

/// The literal content of one component (without its children).
fn component_text(obj: &Map<String, Value>, depth: usize) -> Option<String> {
    if let Some(text) = obj.get("text") {
        return scalar_string(text);
    }
    if let Some(key) = obj.get("translate").and_then(Value::as_str) {
        if let Some(fallback) = obj.get("fallback").and_then(Value::as_str) {
            return Some(fallback.to_owned());
        }
        // Translations are not available here. Substitute `%s` / `%1$s` with the plain
        // arguments so format-only keys still read correctly; other keys show as-is (like the game).
        let args: Vec<String> = match obj.get("with") {
            Some(Value::Array(args)) => args
                .iter()
                .map(|arg| {
                    let mut spans = Vec::new();
                    walk(arg, &Style::default(), &mut spans, depth + 1);
                    spans.into_iter().map(|s| s.text).collect()
                })
                .collect(),
            _ => Vec::new(),
        };
        return Some(substitute_args(key, &args));
    }
    if let Some(key) = obj.get("keybind").and_then(Value::as_str) {
        return Some(key.to_owned());
    }
    if let Some(selector) = obj.get("selector").and_then(Value::as_str) {
        return Some(selector.to_owned());
    }
    if let Some(Value::Object(score)) = obj.get("score") {
        return score.get("value").and_then(scalar_string);
    }
    None
}

fn scalar_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn substitute_args(format: &str, args: &[String]) -> String {
    if args.is_empty() || !format.contains('%') {
        return format.to_owned();
    }
    let mut out = String::new();
    let mut next = 0usize;
    let mut rest = format;
    while let Some(pos) = rest.find('%') {
        out.push_str(&rest[..pos]);
        let tail = &rest[pos + 1..];
        if let Some(after) = tail.strip_prefix('s') {
            out.push_str(args.get(next).map(String::as_str).unwrap_or(""));
            next += 1;
            rest = after;
        } else if let Some(after) = tail.strip_prefix('%') {
            out.push('%');
            rest = after;
        } else if let Some(dollar) = tail.find("$s") {
            match tail[..dollar].parse::<usize>() {
                Ok(index) if index >= 1 => {
                    out.push_str(args.get(index - 1).map(String::as_str).unwrap_or(""));
                    rest = &tail[dollar + 2..];
                }
                _ => {
                    out.push('%');
                    rest = tail;
                }
            }
        } else {
            out.push('%');
            rest = tail;
        }
    }
    out.push_str(rest);
    out
}

fn apply_style(parent: &Style, obj: &Map<String, Value>) -> Style {
    let mut style = parent.clone();
    if let Some(color) = obj.get("color").and_then(Value::as_str) {
        if color.eq_ignore_ascii_case("reset") {
            style.color = None;
        } else if let Some(hex) = parse_color(color) {
            style.color = Some(hex);
        }
    }
    let flags: [(&str, &mut bool); 5] = [
        ("bold", &mut style.bold),
        ("italic", &mut style.italic),
        ("underlined", &mut style.underlined),
        ("strikethrough", &mut style.strikethrough),
        ("obfuscated", &mut style.obfuscated),
    ];
    for (key, flag) in flags {
        if let Some(value) = obj.get(key).and_then(bool_like) {
            *flag = value;
        }
    }
    style
}

fn bool_like(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(b) => Some(*b),
        Value::Number(n) => n.as_i64().map(|n| n != 0),
        Value::String(s) if s.eq_ignore_ascii_case("true") => Some(true),
        Value::String(s) if s.eq_ignore_ascii_case("false") => Some(false),
        _ => None,
    }
}

/// Named color (`gold`) or `#rrggbb` → `#rrggbb`.
fn parse_color(color: &str) -> Option<String> {
    let color = color.trim();
    if let Some(hex) = color.strip_prefix('#') {
        return (hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()))
            .then(|| format!("#{}", hex.to_ascii_lowercase()));
    }
    LEGACY_COLORS
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(color))
        .map(|(_, hex)| (*hex).to_owned())
}

fn legacy_color(code: char) -> Option<&'static str> {
    let index = code.to_digit(16)? as usize;
    LEGACY_COLORS.get(index).map(|(_, hex)| *hex)
}

/// Append `text` (which may contain `§` codes) with `base` as its starting style. Codes only
/// affect this text run; `§r` returns to `base`, a color code clears bold/italic/etc. like the game.
fn push_legacy(text: &str, base: &Style, out: &mut Vec<ChatSpan>) {
    let mut style = base.clone();
    let mut buf = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '§' {
            buf.push(c);
            continue;
        }
        let Some(code) = chars.next() else { break };
        flush(&mut buf, &style, out);
        let code = code.to_ascii_lowercase();
        match code {
            'k' => style.obfuscated = true,
            'l' => style.bold = true,
            'm' => style.strikethrough = true,
            'n' => style.underlined = true,
            'o' => style.italic = true,
            'r' => style = base.clone(),
            'x' => {
                // BungeeCord hex color: §x§R§R§G§G§B§B
                let mut probe = chars.clone();
                let mut hex = String::with_capacity(6);
                while hex.len() < 6 {
                    match (probe.next(), probe.next()) {
                        (Some('§'), Some(d)) if d.is_ascii_hexdigit() => {
                            hex.push(d.to_ascii_lowercase())
                        }
                        _ => break,
                    }
                }
                if hex.len() == 6 {
                    chars = probe;
                    style = Style {
                        color: Some(format!("#{hex}")),
                        ..Style::default()
                    };
                }
            }
            other => {
                if let Some(hex) = legacy_color(other) {
                    style = Style {
                        color: Some(hex.to_owned()),
                        ..Style::default()
                    };
                }
            }
        }
    }
    flush(&mut buf, &style, out);
}

fn flush(buf: &mut String, style: &Style, out: &mut Vec<ChatSpan>) {
    if buf.is_empty() {
        return;
    }
    let text = std::mem::take(buf);
    match out.last_mut() {
        Some(last) if style.same_as(last) => last.text.push_str(&text),
        _ => out.push(style.span(text)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn plain_string_and_legacy_codes() {
        let text = parse_chat(&json!("§aGreen §lBold§r plain"));
        assert_eq!(text.plain, "Green Bold plain");
        assert_eq!(text.spans.len(), 3);
        assert_eq!(text.spans[0].color.as_deref(), Some("#55ff55"));
        assert!(!text.spans[0].bold);
        assert!(text.spans[1].bold);
        assert_eq!(text.spans[1].color.as_deref(), Some("#55ff55"));
        assert_eq!(text.spans[2].color, None);
        assert!(!text.spans[2].bold);
    }

    #[test]
    fn color_code_resets_formatting() {
        let text = parse_legacy("§l§nA§cB");
        assert!(text.spans[0].bold && text.spans[0].underlined);
        assert!(!text.spans[1].bold && !text.spans[1].underlined);
        assert_eq!(text.spans[1].color.as_deref(), Some("#ff5555"));
    }

    #[test]
    fn component_tree_flattens_with_inheritance() {
        let value = json!({
            "text": "",
            "extra": [
                {"text": "Hello ", "color": "gold", "bold": true},
                {"text": "World", "color": "#12AbEf", "extra": [{"text": "!", "bold": false}]},
                "\n§7Second line"
            ]
        });
        let text = parse_chat(&value);
        assert_eq!(text.plain, "Hello World!\nSecond line");
        let hello = &text.spans[0];
        assert_eq!(hello.text, "Hello ");
        assert_eq!(hello.color.as_deref(), Some("#ffaa00"));
        assert!(hello.bold);
        let world = &text.spans[1];
        assert_eq!(world.color.as_deref(), Some("#12abef"));
        assert!(!world.bold);
        assert_eq!(text.raw, value);
    }

    #[test]
    fn arrays_share_first_element_style() {
        let value = json!([{"text": "A", "color": "red"}, "B", {"text": "C", "color": "blue"}]);
        let text = parse_chat(&value);
        assert_eq!(text.plain, "ABC");
        assert_eq!(text.spans[0].text, "AB");
        assert_eq!(text.spans[0].color.as_deref(), Some("#ff5555"));
        assert_eq!(text.spans[1].color.as_deref(), Some("#5555ff"));
    }

    #[test]
    fn translate_and_odd_values() {
        let value = json!({"translate": "%s joined (%2$s)", "with": ["Steve", {"text": "x"}]});
        assert_eq!(to_plain(&value), "Steve joined (x)");
        assert_eq!(
            to_plain(&json!({"translate": "k", "fallback": "Fallback"})),
            "Fallback"
        );
        assert_eq!(to_plain(&json!({"text": 42})), "42");
        assert_eq!(to_plain(&json!(null)), "");
        assert_eq!(to_plain(&json!({"keybind": "key.jump"})), "key.jump");
    }

    #[test]
    fn bungee_hex_colors_and_trailing_section_sign() {
        let text = parse_legacy("§x§f§f§0§0§a§aHex§");
        assert_eq!(text.plain, "Hex");
        assert_eq!(text.spans[0].color.as_deref(), Some("#ff00aa"));
        assert_eq!(strip_formatting("§cRed §lbold§"), "Red bold");
    }

    #[test]
    fn deep_nesting_is_bounded() {
        let mut value = json!({"text": "x"});
        for _ in 0..200 {
            value = json!({"text": "", "extra": [value]});
        }
        // Must not overflow the stack; deep content is simply dropped.
        let _ = parse_chat(&value);
    }
}

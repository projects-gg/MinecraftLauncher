//! Tolerant JSON reading for hand-written metadata files (`fabric.mod.json`, `mcmod.info`,
//! `pack.mcmeta`). Many mods ship files the game's lenient parsers accept but strict JSON does
//! not: comments, trailing commas, raw newlines/tabs inside strings, stray backslashes, a BOM.

use serde_json::Value;

/// Parse strictly first; on failure, repair the common mistakes and try again.
pub(crate) fn parse(text: &str) -> Option<Value> {
    let text = text.trim_start_matches('\u{feff}');
    if let Ok(value) = serde_json::from_str(text) {
        return Some(value);
    }
    serde_json::from_str(&repair(text)).ok()
}

/// Remove comments and trailing commas, escape control characters inside strings and double
/// backslashes that do not start a valid escape.
fn repair(text: &str) -> String {
    drop_trailing_commas(&strip_comments_fix_strings(text))
}

/// Remove commas directly followed (after whitespace) by `}` or `]`. Expects valid strings.
fn drop_trailing_commas(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut in_string = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            out.push(c);
            if c == '\\' {
                if let Some(&next) = chars.get(i + 1) {
                    out.push(next);
                    i += 1;
                }
            } else if c == '"' {
                in_string = false;
            }
        } else if c == ',' {
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            if !matches!(chars.get(j), Some('}') | Some(']')) {
                out.push(c);
            }
        } else {
            in_string = c == '"';
            out.push(c);
        }
        i += 1;
    }
    out
}

fn strip_comments_fix_strings(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    let mut in_string = false;
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            match c {
                '"' => {
                    in_string = false;
                    out.push(c);
                }
                '\\' => match chars.get(i + 1) {
                    Some(&next)
                        if matches!(next, '"' | '\\' | '/' | 'b' | 'f' | 'n' | 'r' | 't') =>
                    {
                        out.push('\\');
                        out.push(next);
                        i += 1;
                    }
                    Some('u')
                        if chars.len() > i + 5
                            && chars[i + 2..i + 6].iter().all(char::is_ascii_hexdigit) =>
                    {
                        out.push('\\');
                    }
                    _ => out.push_str("\\\\"),
                },
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                c => out.push(c),
            }
            i += 1;
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                out.push(c);
            }
            '/' if chars.get(i + 1) == Some(&'/') => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                continue;
            }
            '/' if chars.get(i + 1) == Some(&'*') => {
                i += 2;
                while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                    i += 1;
                }
                i += 2;
                continue;
            }
            '#' if out.trim_end().ends_with(['{', ',', '[']) || out.trim().is_empty() => {
                // Hash comments (seen in a few mcmod.info files) at the start of a line.
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                continue;
            }
            c => out.push(c),
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn strict_json_passes_through() {
        assert_eq!(parse(r#"{"a": [1, 2]}"#), Some(json!({"a": [1, 2]})));
        assert_eq!(parse("\u{feff}{\"a\":1}"), Some(json!({"a": 1})));
    }

    #[test]
    fn repairs_common_mistakes() {
        let text = "{\n  // comment\n  \"id\": \"x\", /* block */\n  \"description\": \"line one\nline two\ttab\",\n  \"path\": \"C:\\Users\\me\",\n  \"list\": [1, 2,],\n}";
        let value = parse(text).unwrap();
        assert_eq!(value["id"], "x");
        assert_eq!(value["description"], "line one\nline two\ttab");
        assert_eq!(value["path"], "C:\\Users\\me");
        assert_eq!(value["list"], json!([1, 2]));
    }

    #[test]
    fn trailing_comma_before_comment() {
        let value = parse("{\"a\": [1, 2, // last\n], \"b\": \"x,]\",}").unwrap();
        assert_eq!(value["a"], json!([1, 2]));
        assert_eq!(value["b"], "x,]");
    }

    #[test]
    fn keeps_url_slashes_and_unicode_escapes() {
        let value = parse("{\"url\": \"https://example.com//x\", \"u\": \"\\u00e7\",}").unwrap();
        assert_eq!(value["url"], "https://example.com//x");
        assert_eq!(value["u"], "ç");
    }

    #[test]
    fn hopeless_input_is_none() {
        assert_eq!(parse("not json at all"), None);
        assert_eq!(parse(""), None);
    }
}

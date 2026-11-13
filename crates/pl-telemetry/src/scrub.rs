//! Crash and page-error summaries with nothing of the player in them.
//!
//! A panic's message and a page error's text are written by our code, but they can carry what the
//! code had in hand: a file path (with the Windows user name), a file or world name, a server
//! address, a link, an account name, a token. Only the shape of the message goes out, and what is
//! not known to be safe goes: quoted text becomes `…` (only Rust's code in backticks and a page
//! error's property name in single quotes stay; double quotes never); paths, links, host names,
//! e-mail and IP addresses become placeholders; a word with a dot (a file or a host), an
//! underscore or letters mixed with digits (a user name, a code) becomes `…`, as does a word with
//! letters outside ASCII (a Turkish name); long numbers become `#`, ids `<id>`; the whole is one
//! line of at most [`MESSAGE_MAX`] characters. Plain words and Rust paths (`feed::add::run`) stay.
//! Locations keep only
//! the part of a path our build put there (`crates/pl-mc/src/launch.rs:412`,
//! `tokio-1.53.0/src/…`), never the folders of the machine that built it. The service checks
//! every field again: it trusts nothing a launcher sends (the launcher is open source).

/// The longest message a summary keeps, in characters.
pub const MESSAGE_MAX: usize = 120;
/// The longest location (its file part, before `:line`).
pub const LOCATION_MAX: usize = 100;
/// The longest failure category.
pub const CATEGORY_MAX: usize = 32;

/// Top-level domains a one-dot word is named a host with (`aternos.me`: `<host>`); any other word
/// with a dot (`oyna.pw`, `x.json`, `e.map`) goes as `…` all the same.
const TLDS: &[&str] = &[
    "app", "biz", "br", "cc", "cf", "club", "co", "com", "de", "dev", "es", "eu", "fr", "fun",
    "ga", "games", "gg", "gq", "host", "info", "io", "it", "link", "live", "lol", "me", "ml",
    "net", "network", "nl", "online", "org", "pl", "pro", "ru", "site", "space", "store", "tk",
    "top", "tr", "tv", "uk", "us", "world", "ws", "xyz",
];

/// Rust's own words a panic message puts in backticks (`` `None` ``).
const RUST_WORDS: &[&str] = &[
    "None", "Some", "Ok", "Err", "Option", "Result", "Vec", "String", "Box", "Rc", "Arc",
    "RefCell", "Mutex",
];

/// Quoted text that stays: in backticks, Rust's code (`Option::unwrap()`, `` `None` ``); in
/// single quotes, a page error's property or method name (`'length'`, `'getContext'`). Text in
/// double quotes never stays: Rust prints a string that way (`name: "kaan"`), names and file names
/// included. Anything else in quotes may be the player's: a name, a path, a server's answer.
fn is_code_word(text: &str, quote: char) -> bool {
    let bytes = text.as_bytes();
    let Some(&first) = bytes.first() else {
        return false;
    };
    if bytes.len() > 40 {
        return false;
    }
    match quote {
        '\'' => first.is_ascii_lowercase() && bytes.iter().all(u8::is_ascii_alphabetic),
        '`' => {
            RUST_WORDS.contains(&text)
                || ((first.is_ascii_alphabetic() || first == b'_')
                    && bytes
                        .iter()
                        .all(|b| b.is_ascii_alphanumeric() || b"_:<>&()".contains(b))
                    && (text.contains("::") || text.contains('(')))
        }
        _ => false,
    }
}

/// Where a quote may open: at the start or after a space or an opening mark, so the apostrophe in
/// "Couldn't" is a letter, not a quote.
fn opens_quote(previous: Option<char>) -> bool {
    previous.is_none_or(|c| c.is_whitespace() || "([{=:,".contains(c))
}

/// One word outside quotes, as it may go out (the punctuation around it stays).
fn word(raw: &str) -> String {
    let start = raw
        .find(|c: char| !"([{<,;".contains(c))
        .unwrap_or(raw.len());
    let end = raw
        .rfind(|c: char| !")]}>,;.!?:".contains(c))
        .map_or(start, |i| {
            i + raw[i..].chars().next().map_or(1, char::len_utf8)
        });
    if start >= end {
        return raw.to_owned();
    }
    format!(
        "{}{}{}",
        &raw[..start],
        word_body(&raw[start..end]),
        &raw[end..]
    )
}

fn word_body(body: &str) -> String {
    if !body.is_ascii() {
        return "…".to_owned();
    }
    let has_letter = body.bytes().any(|b| b.is_ascii_alphabetic());
    let has_digit = body.bytes().any(|b| b.is_ascii_digit());
    let placeholder = if body.contains("://") {
        "<url>"
    } else if is_email(body) {
        "<email>"
    } else if is_ipv4(body) || is_ipv6(body) {
        "<ip>"
    } else if is_path(body) {
        "<path>"
    } else if is_token(body) {
        "<id>"
    } else if is_rust_path(body) {
        return body.to_owned();
    } else if body.contains('.') && has_letter {
        // A file, a world, a host or minified code (`e.map`): only a version (`1.20.1`) stays.
        if is_host(body) { "<host>" } else { "…" }
    } else if body.contains('_') || (has_letter && has_digit) {
        // `Steve_42`, `kaan07`, `AB12-CD34`: a name or a code, never a word.
        "…"
    } else {
        return runs(body);
    };
    placeholder.to_owned()
}

fn is_email(body: &str) -> bool {
    body.split_once('@')
        .is_some_and(|(user, host)| !user.is_empty() && host.contains('.'))
}

/// Any slash: absolute and relative paths (`saves/MyWorld/level.dat`), shares, links without a
/// scheme (`discord.gg/abc`); `~` starts a home folder.
fn is_path(body: &str) -> bool {
    body.contains('/') || body.contains('\\') || body.starts_with('~')
}

/// `core::option::unwrap_failed`, `Option::unwrap()`: our code's own words.
fn is_rust_path(body: &str) -> bool {
    body.contains("::")
        && body
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && body
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_:<>&()".contains(&b))
}

fn is_ipv4(body: &str) -> bool {
    let host = body.split(':').next().unwrap_or(body);
    let parts: Vec<&str> = host.split('.').collect();
    parts.len() == 4
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.len() <= 3 && p.bytes().all(|b| b.is_ascii_digit()))
}

fn is_ipv6(body: &str) -> bool {
    let trimmed = body.trim_start_matches('[');
    let trimmed = trimmed.split(']').next().unwrap_or(trimmed);
    trimmed.matches(':').count() >= 3
        && trimmed.bytes().any(|b| b.is_ascii_digit())
        && trimmed.bytes().all(|b| b.is_ascii_hexdigit() || b == b':')
}

/// `play.example.net`, `example.com:25565`, `aternos.me`: a server the player may play on.
fn is_host(body: &str) -> bool {
    let (host, port) = match body.rsplit_once(':') {
        Some((host, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => {
            (host, true)
        }
        _ => (body, false),
    };
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() < 2
        || labels.iter().any(|label| {
            label.is_empty()
                || !label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
    {
        return false;
    }
    let tld = labels[labels.len() - 1].to_ascii_lowercase();
    if tld.len() < 2 || !tld.bytes().all(|b| b.is_ascii_alphabetic()) {
        return false;
    }
    labels.len() >= 3 || port || TLDS.contains(&tld.as_str())
}

/// UUIDs, keys and other ids: long and mixed, or one long run of id characters.
fn is_token(body: &str) -> bool {
    let hex = body.bytes().filter(u8::is_ascii_hexdigit).count();
    if body.matches('-').count() >= 3 && hex >= 16 {
        return true;
    }
    if body.len() >= 32
        && body
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"+/=_-".contains(&b))
    {
        return true;
    }
    let digits = body.bytes().filter(u8::is_ascii_digit).count();
    body.len() >= 24
        && digits >= 3
        && body.bytes().any(|b| b.is_ascii_uppercase())
        && body.bytes().any(|b| b.is_ascii_lowercase())
        && body
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"+/=_-.".contains(&b))
}

/// Five or more digits in a row, or twelve or more hex characters with a digit among them, become
/// `#` (sizes, ports, timestamps, hashes); shorter numbers stay ("error 5", "line 42").
fn runs(body: &str) -> String {
    let bytes = body.as_bytes();
    let mut out = String::with_capacity(body.len());
    let mut i = 0;
    while i < bytes.len() {
        if !bytes[i].is_ascii_hexdigit() {
            out.push(char::from(bytes[i]));
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_hexdigit() {
            i += 1;
        }
        let run = &body[start..i];
        let digits = run.bytes().filter(u8::is_ascii_digit).count();
        if (digits == run.len() && run.len() >= 5) || (run.len() >= 12 && digits > 0) {
            out.push('#');
        } else {
            // Digit runs inside a mixed run still count: "abc123456" keeps "abc".
            out.push_str(&digit_runs(run));
        }
    }
    out
}

fn digit_runs(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut digits = String::new();
    for c in text.chars() {
        if c.is_ascii_digit() {
            digits.push(c);
            continue;
        }
        flush_digits(&mut out, &mut digits);
        out.push(c);
    }
    flush_digits(&mut out, &mut digits);
    out
}

fn flush_digits(out: &mut String, digits: &mut String) {
    if digits.len() >= 5 {
        out.push('#');
    } else {
        out.push_str(digits);
    }
    digits.clear();
}

/// Cuts at `max` characters with `…` (on a character boundary).
fn cap(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// A panic's or a page error's message, safe to send (see the module).
pub fn message(raw: &str) -> String {
    let line = raw.lines().next().unwrap_or_default();
    let chars: Vec<char> = line.chars().filter(|c| !c.is_control()).collect();
    let mut out = String::new();
    let mut plain = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let previous = i.checked_sub(1).map(|p| chars[p]);
        let quote = matches!(c, '"' | '`') || (c == '\'' && opens_quote(previous));
        if !quote {
            plain.push(c);
            i += 1;
            continue;
        }
        // The quoted text, up to the same quote (a backslash escapes it inside "…").
        let mut j = i + 1;
        let mut inner = String::new();
        let mut closed = false;
        while j < chars.len() {
            let d = chars[j];
            if d == '\\' && c == '"' && j + 1 < chars.len() {
                inner.push(chars[j + 1]);
                j += 2;
                continue;
            }
            if d == c {
                closed = true;
                break;
            }
            inner.push(d);
            j += 1;
        }
        push_plain(&mut out, &mut plain);
        out.push(c);
        out.push_str(if is_code_word(&inner, c) {
            &inner
        } else {
            "…"
        });
        if !closed {
            // Nothing after an unclosed quote is kept.
            break;
        }
        out.push(c);
        i = j + 1;
    }
    push_plain(&mut out, &mut plain);
    let collapsed = out.split_whitespace().collect::<Vec<_>>().join(" ");
    cap(&collapsed, MESSAGE_MAX)
}

fn push_plain(out: &mut String, plain: &mut String) {
    for (index, piece) in plain.split(' ').enumerate() {
        if index > 0 {
            out.push(' ');
        }
        out.push_str(&word(piece));
    }
    plain.clear();
}

/// The part of a source path our build put there, `/`-separated (see the module).
fn source_path(file: &str) -> String {
    let file = file.replace('\\', "/");
    let tail = if let Some((_, rest)) = file.split_once("/registry/src/") {
        // `index.crates.io-<hash>/tokio-1.53.0/src/…`: the crate and its path.
        rest.split_once('/')
            .map_or(rest, |(_, crate_path)| crate_path)
            .to_owned()
    } else if let Some((_, rest)) = file.split_once("/git/checkouts/") {
        // `<repo>-<hash>/<rev>/src/…`.
        rest.splitn(3, '/').nth(2).unwrap_or(rest).to_owned()
    } else if let Some(rest) = file.strip_prefix("/rustc/") {
        // `/rustc/<commit>/library/core/src/…`.
        let library = rest.split_once('/').map_or(rest, |(_, path)| path);
        format!("rustc/{library}")
    } else if file.starts_with('/') || file.as_bytes().get(1) == Some(&b':') {
        // Another absolute path: the build machine's folders are nobody's business.
        let parts: Vec<&str> = file.split('/').filter(|p| !p.is_empty()).collect();
        parts[parts.len().saturating_sub(3)..].join("/")
    } else {
        file
    };
    let clean: String = tail
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || "_./-".contains(*c))
        .collect();
    let clean = clean.trim_start_matches(['.', '/']);
    // The end of a long path says the most.
    let count = clean.chars().count();
    clean
        .chars()
        .skip(count.saturating_sub(LOCATION_MAX))
        .collect()
}

/// A panic's location: `crates/pl-mc/src/launch/mod.rs:412`.
pub fn location(file: &str, line: u32) -> String {
    let path = source_path(file);
    if path.is_empty() {
        return format!("?:{line}");
    }
    format!("{path}:{line}")
}

/// A page error's place in the launcher's own scripts: `assets/index-B9x2.js:1:2345` (the scheme
/// and host of `http://tauri.localhost/…` and `tauri://localhost/…` go; scripts that are not
/// files say `inline`).
pub fn page_source(url: &str, line: u32, column: u32) -> String {
    let path = match url.split_once("://") {
        Some(("http" | "https" | "tauri" | "asset", rest)) => rest
            .split_once('/')
            .map_or("", |(_, path)| path)
            .split(['?', '#'])
            .next()
            .unwrap_or_default()
            .to_owned(),
        _ => String::new(),
    };
    let path = Some(source_path(&path))
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| "inline".to_owned());
    match (line, column) {
        (0, _) => path,
        (line, 0) => format!("{path}:{line}"),
        (line, column) => format!("{path}:{line}:{column}"),
    }
}

/// A failure category (a command error's code, `crash.<kind>`), as it may go out.
pub fn category(raw: &str) -> String {
    let clean: String = raw
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || ".:_-".contains(*c))
        .take(CATEGORY_MAX)
        .collect();
    if clean.is_empty() {
        "other".to_owned()
    } else {
        clean
    }
}

/// A game crash's trace (`native/gpu-amd`, `java/NullPointerException/sodium`, `exit/-1`), kept
/// only in that shape: a source, then one or two parts of at most 40 letters, digits, `.`, `_` or
/// `-`. The analysis builds it from closed lists; anything else is not sent.
pub fn trace(raw: &str) -> Option<String> {
    let mut parts = raw.split('/');
    let source = parts.next()?;
    if !matches!(source, "native" | "exit" | "java") {
        return None;
    }
    let rest: Vec<&str> = parts.collect();
    let part_ok = |part: &&str| {
        !part.is_empty()
            && part.len() <= 40
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || ".-_".contains(c))
    };
    (matches!(rest.len(), 1 | 2) && rest.iter().all(part_ok)).then(|| raw.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn std_unwrap_messages_keep_their_code_words() {
        assert_eq!(
            message("called `Option::unwrap()` on a `None` value"),
            "called `Option::unwrap()` on a `None` value"
        );
        assert_eq!(
            message("index out of bounds: the len is 3 but the index is 7"),
            "index out of bounds: the len is 3 but the index is 7"
        );
    }

    #[test]
    fn windows_paths_and_user_names_go() {
        let raw = r#"called `Result::unwrap()` on an `Err` value: Os { code: 2, kind: NotFound, message: "Sistem belirtilen dosyayı bulamıyor." } at C:\Users\Ahmet\AppData\Roaming\x.json"#;
        assert_eq!(
            message(raw),
            r#"called `Result::unwrap()` on an `Err` value: Os { code: 2, kind: NotFound, message: "…" } at <path>"#
        );
    }

    #[test]
    fn paths_go_in_every_form() {
        assert_eq!(
            message(r#"failed to open "C:\\Users\\Zeynep\\Desktop\\mods" (code 5)"#),
            r#"failed to open "…" (code 5)"#
        );
        assert_eq!(
            message("no such file /home/kaan/.local/share/x and /Users/ece/Library"),
            "no such file <path> and <path>"
        );
        assert_eq!(message("bad dir ~/minecraft/mods"), "bad dir <path>");
        assert_eq!(
            message(r"share \\server\games\mc failed"),
            "share <path> failed"
        );
        assert_eq!(message("drive D:/games/mc full"), "drive <path> full");
    }

    #[test]
    fn names_in_quotes_and_non_ascii_words_go() {
        assert_eq!(
            message("account 'Steve_42' not found"),
            "account '…' not found"
        );
        assert_eq!(message("account \"Kaan_07\" left"), "account \"…\" left");
        assert_eq!(message("oyuncu Ayşe katılamadı"), "oyuncu … …");
        // The apostrophe in a word is no quote.
        assert_eq!(
            message("Couldn't find callback id 12"),
            "Couldn't find callback id 12"
        );
        // A property name in a page error is a code word; a capital word only in backticks.
        assert_eq!(
            message("TypeError: Cannot read properties of undefined (reading 'length')"),
            "TypeError: Cannot read properties of undefined (reading 'length')"
        );
        assert_eq!(message("got 'None' here"), "got '…' here");
        // A host in quotes is no code word.
        assert_eq!(message("server 'play.example.net' down"), "server '…' down");
        // Double quotes never keep anything (Rust prints strings so); backticks only code.
        assert_eq!(
            message(r#"Player { name: "kaan" }"#),
            r#"Player { name: "…" }"#
        );
        assert_eq!(message("open 'MyWorld.zip' failed"), "open '…' failed");
        assert_eq!(message("out of bounds of `kaan`"), "out of bounds of `…`");
        assert_eq!(
            message("called `Result::unwrap()` on an `Err` value"),
            "called `Result::unwrap()` on an `Err` value"
        );
    }

    #[test]
    fn names_files_and_codes_outside_quotes_go() {
        assert_eq!(message("account kaan_07 not found"), "account … not found");
        assert_eq!(message("user kaan07 left"), "user … left");
        assert_eq!(message("code AB12-CD34 used"), "code … used");
        assert_eq!(message("read saves/MyWorld/level.dat"), "read <path>");
        assert_eq!(message("join discord.gg/abc now"), "join <path> now");
        assert_eq!(message("ping oyna.pw and craft.ovh"), "ping … and …");
        assert_eq!(message("bad options.txt line"), "bad … line");
        assert_eq!(
            message("TypeError: e.map is not a function"),
            "TypeError: … is not a function"
        );
        // Plain words, small numbers, versions and Rust paths stay.
        assert_eq!(
            message("Minecraft 1.20.1 needs Java 17, got 8."),
            "Minecraft 1.20.1 needs Java 17, got 8."
        );
        assert_eq!(
            message("in core::option::unwrap_failed"),
            "in core::option::unwrap_failed"
        );
    }

    #[test]
    fn addresses_links_and_ids_go() {
        assert_eq!(
            message("connect to play.example.net:25565 at 192.168.1.20:25565 failed"),
            "connect to <host> at <ip> failed"
        );
        assert_eq!(message("ping aternos.me failed"), "ping <host> failed");
        assert_eq!(
            message("fetch https://x.test/a?token=abc failed"),
            "fetch <url> failed"
        );
        assert_eq!(
            message("mail kaan@example.com bounced"),
            "mail <email> bounced"
        );
        assert_eq!(
            message("session 550e8400-e29b-41d4-a716-446655440000 expired"),
            "session <id> expired"
        );
        assert_eq!(
            message("token eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9 refused"),
            "token <id> refused"
        );
        assert_eq!(message("peer 2001:db8::1 left"), "peer <ip> left");
        assert_eq!(
            message("memory allocation of 1073741824 bytes failed"),
            "memory allocation of # bytes failed"
        );
        assert_eq!(message("hash deadbeefcafe1234 mismatch"), "hash … mismatch");
        assert_eq!(message("got 4294967296 back"), "got # back");
        // Code is neither a host nor an address.
        assert_eq!(message("in feed::add::run"), "in feed::add::run");
        assert_eq!(
            message("Minecraft 1.20.1 needs Java 17"),
            "Minecraft 1.20.1 needs Java 17"
        );
    }

    #[test]
    fn messages_are_one_short_line() {
        let out = message(&format!("first line {}\nsecond line", "word ".repeat(80)));
        assert!(out.chars().count() <= MESSAGE_MAX);
        assert!(out.ends_with('…'));
        // One long run of id characters is an id.
        assert_eq!(message(&format!("key {}", "x".repeat(400))), "key <id>");
        assert!(!out.contains("second"));
        assert_eq!(message("  spaced \t out  "), "spaced out");
        assert_eq!(message("unclosed \"quote C:\\Users\\x"), "unclosed \"…");
        assert_eq!(message(""), "");
    }

    #[test]
    fn locations_keep_only_the_build_s_part() {
        assert_eq!(
            location(r"crates\pl-mc\src\launch\mod.rs", 412),
            "crates/pl-mc/src/launch/mod.rs:412"
        );
        assert_eq!(
            location(
                r"C:\Users\runneradmin\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\tokio-1.53.0\src\runtime\park.rs",
                7
            ),
            "tokio-1.53.0/src/runtime/park.rs:7"
        );
        assert_eq!(
            location(
                "/home/runner/.cargo/git/checkouts/tauri-abc123/9f1e2d3/crates/tauri/src/app.rs",
                9
            ),
            "crates/tauri/src/app.rs:9"
        );
        assert_eq!(
            location(
                "/rustc/4d91de4e48198da2e33413efdcd9cd2cc0c46688/library/core/src/option.rs",
                2
            ),
            "rustc/library/core/src/option.rs:2"
        );
        assert_eq!(
            location(r"C:\Users\hazar\Desktop\Dev Tools\x\src\lib.rs", 3),
            "x/src/lib.rs:3"
        );
        assert_eq!(location("", 1), "?:1");
    }

    #[test]
    fn page_sources_lose_scheme_and_host() {
        assert_eq!(
            page_source("http://tauri.localhost/assets/index-B9x2.js", 1, 2345),
            "assets/index-B9x2.js:1:2345"
        );
        assert_eq!(
            page_source("tauri://localhost/assets/index-B9x2.js?v=1", 3, 0),
            "assets/index-B9x2.js:3"
        );
        assert_eq!(page_source("", 0, 0), "inline");
        assert_eq!(
            page_source("blob:http://tauri.localhost/1-2", 4, 5),
            "inline:4:5"
        );
    }

    #[test]
    fn categories_are_short_codes() {
        assert_eq!(category("DISK-DOLU"), "DISK-DOLU");
        assert_eq!(category("crash.modError"), "crash.modError");
        assert_eq!(category("a b/c\\d"), "abcd");
        assert_eq!(category(""), "other");
        assert_eq!(category(&"X".repeat(80)).len(), CATEGORY_MAX);
    }
}

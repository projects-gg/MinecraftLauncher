//! The launcher's system notifications on Windows: toasts under the launcher's own
//! AppUserModelID, which Windows shows with the launcher's name and picture.
//!
//! Windows finds the name and the picture of a toast's ID in the Start menu (a shortcut that
//! carries the ID, as the installer makes) or under `HKCU\Software\Classes\AppUserModelId\<id>`
//! (`DisplayName`, `IconUri`). A toast under an ID it finds in neither place goes to the history
//! without a banner, so nobody sees it (tried on Windows 11, 22 Oct 2026). The notification plugin
//! got around that in development by sending as Windows PowerShell from every exe under a
//! `target\debug` or `target\release` folder; that is how the chat of test launchers reached the
//! owner as "Windows PowerShell" (NOTIF1). The launcher now sends every toast under its own ID (the
//! installed launcher's `gg.projects.launcher`, a debug build's own) and names that ID itself at
//! every start, so neither the folder the exe runs from nor a shortcut matters.
//!
//! The toast's text is built here (tested); the Windows calls are in the launcher
//! (`apps/launcher/src-tauri/src/notify.rs`).
//!
//! CHAT-CALM: a chat's notification carries a tag (its conversation) and a data-bound text, so one
//! conversation has one notification: a new one takes the old one's place, and between pop-ups the
//! one on screen or in the notification center changes its words in place ("Kaan: 5 yeni mesaj").

/// Where the AppUserModelIDs of apps without a package are named, under `HKEY_CURRENT_USER`: one
/// key per ID with its `DisplayName` and `IconUri`.
pub const APP_IDS_KEY: &str = r"Software\Classes\AppUserModelId";

/// The picture's file name, in the app's per-machine folder (`<local data>/<app identifier>/`).
pub const ICON_FILE: &str = "notification-icon.png";

/// Longest title kept, in characters: Windows shows two lines of it.
pub const TITLE_CHARS: usize = 200;
/// Longest body kept, in characters: Windows shows about four lines, and a toast stays small.
pub const BODY_CHARS: usize = 600;

/// The group of the launcher's chat notifications (with a tag per conversation).
pub const CHAT_GROUP: &str = "chat";

/// A tagged toast's text, bound to the values `title` and `body` of its notification data: the
/// words can change in place later, and nothing a player wrote is ever read as XML.
pub const BOUND_TOAST_XML: &str = concat!(
    r#"<toast><visual><binding template="ToastGeneric"><text>{title}</text><text>{body}</text>"#,
    r#"</binding></visual><audio silent="true"/></toast>"#
);

/// The tag of a conversation's notification: 16 hexadecimal characters of the key's FNV-1a hash,
/// within the 16 characters Windows took before version 1703 (a conversation id is longer).
pub fn toast_tag(key: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in key.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// A value of a tagged toast's data: the characters XML can carry, cut at `max` with "…".
pub fn toast_value(text: &str, max: usize) -> String {
    let kept: Vec<char> = text.chars().filter(|&c| xml_char(c)).collect();
    if kept.len() <= max {
        return kept.into_iter().collect();
    }
    let mut cut: String = kept.into_iter().take(max - 1).collect();
    cut.push('…');
    cut
}

/// An AppUserModelID Windows takes and a registry key can be named after: 1–128 printable ASCII
/// characters, no spaces, no backslashes.
pub fn valid_app_id(id: &str) -> bool {
    (1..=128).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_graphic() && b != b'\\')
}

/// The toast: the title and, when there is one, the body, without a sound (the launcher's
/// notifications never made one). The text is escaped and loses the characters XML cannot carry,
/// so nothing a player wrote (a chat message, a crew name) can add to the toast or break it; very
/// long text is cut with "…".
pub fn toast_xml(title: &str, body: Option<&str>) -> String {
    let mut xml = String::from(r#"<toast><visual><binding template="ToastGeneric">"#);
    push_text(&mut xml, title, TITLE_CHARS);
    if let Some(body) = body.filter(|body| !body.trim().is_empty()) {
        push_text(&mut xml, body, BODY_CHARS);
    }
    xml.push_str(r#"</binding></visual><audio silent="true"/></toast>"#);
    xml
}

fn push_text(xml: &mut String, text: &str, max: usize) {
    let kept: Vec<char> = text.chars().filter(|&c| xml_char(c)).collect();
    let cut = kept.len() > max;
    xml.push_str("<text>");
    for &c in kept.iter().take(if cut { max - 1 } else { max }) {
        match c {
            '&' => xml.push_str("&amp;"),
            '<' => xml.push_str("&lt;"),
            '>' => xml.push_str("&gt;"),
            '"' => xml.push_str("&quot;"),
            '\'' => xml.push_str("&apos;"),
            c => xml.push(c),
        }
    }
    if cut {
        xml.push('…');
    }
    xml.push_str("</text>");
}

/// The characters XML 1.0 carries: tabs and line breaks, not the other control characters or the
/// non-characters U+FFFE and U+FFFF.
fn xml_char(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::Identity;

    #[test]
    fn a_toast_carries_the_title_and_the_body_without_a_sound() {
        assert_eq!(
            toast_xml("Ayşe", Some("Akşam 9'da Bedwars? Çıkartma: GG")),
            "<toast><visual><binding template=\"ToastGeneric\"><text>Ayşe</text>\
             <text>Akşam 9&apos;da Bedwars? Çıkartma: GG</text></binding></visual>\
             <audio silent=\"true\"/></toast>"
        );
        let only_title = "<toast><visual><binding template=\"ToastGeneric\"><text>Kurulum bitti</text>\
                          </binding></visual><audio silent=\"true\"/></toast>";
        assert_eq!(toast_xml("Kurulum bitti", None), only_title);
        assert_eq!(toast_xml("Kurulum bitti", Some(" \n ")), only_title);
    }

    /// A chat message is someone else's text: whatever it holds stays text in one `<text>`.
    #[test]
    fn text_cannot_add_to_the_toast() {
        let attack = r#"</text></binding></visual><actions><action activationType="protocol" arguments="https://example.com" content="Aç"/></actions><visual><binding template="ToastGeneric"><text>"#;
        let xml = toast_xml("Ece", Some(attack));
        assert_eq!(xml.matches("<text>").count(), 2, "{xml}");
        assert_eq!(xml.matches("</text>").count(), 2, "{xml}");
        assert!(!xml.contains("<action"), "{xml}");
        assert!(xml.contains("&lt;/text&gt;&lt;/binding&gt;"), "{xml}");
        assert!(xml.contains("activationType=&quot;protocol&quot;"), "{xml}");
        // A name can be odd too.
        let xml = toast_xml("<Ekip & Co>", None);
        assert!(xml.contains("<text>&lt;Ekip &amp; Co&gt;</text>"), "{xml}");
    }

    #[test]
    fn characters_xml_cannot_carry_are_dropped() {
        let xml = toast_xml(
            "a\u{0}b\u{1b}c",
            Some("satır\u{b}\u{c}\u{fffe}\u{ffff}\nikinci\tsekme 🎮"),
        );
        assert!(xml.contains("<text>abc</text>"), "{xml}");
        assert!(
            xml.contains("<text>satır\nikinci\tsekme 🎮</text>"),
            "{xml}"
        );
    }

    #[test]
    fn long_text_is_cut_with_an_ellipsis() {
        let body = "ğ".repeat(1_000);
        let xml = toast_xml(&"Ş".repeat(TITLE_CHARS), Some(&body));
        // A title of exactly the limit stays whole.
        assert!(
            xml.contains(&format!("<text>{}</text>", "Ş".repeat(TITLE_CHARS))),
            "{xml}"
        );
        assert!(
            xml.contains(&format!("<text>{}…</text>", "ğ".repeat(BODY_CHARS - 1))),
            "{xml}"
        );
        // Escapes count as the one character they stand for.
        let xml = toast_xml(&"&".repeat(TITLE_CHARS + 1), None);
        assert!(
            xml.contains(&format!(
                "<text>{}…</text>",
                "&amp;".repeat(TITLE_CHARS - 1)
            )),
            "{xml}"
        );
    }

    #[test]
    fn a_conversation_has_one_short_tag() {
        let tag = toast_tag("d:123456789012345678:223456789012345678");
        assert_eq!(tag.len(), 16);
        assert!(tag.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_eq!(
            tag,
            toast_tag("d:123456789012345678:223456789012345678"),
            "stable"
        );
        assert_ne!(tag, toast_tag("c:0123456789abcdef"));
        assert_eq!(toast_tag(""), "cbf29ce484222325");
    }

    #[test]
    fn a_bound_toast_says_its_values_and_nothing_else() {
        assert!(BOUND_TOAST_XML.contains("<text>{title}</text><text>{body}</text>"));
        assert!(BOUND_TOAST_XML.contains(r#"<audio silent="true"/>"#));
        // A value is never read as XML: the characters XML cannot carry go, the rest stays.
        assert_eq!(toast_value("<b>&</b>\u{0}", 50), "<b>&</b>");
        assert_eq!(toast_value(&"ğ".repeat(10), 5), "ğğğğ…");
        assert_eq!(toast_value("12 yeni mesaj", BODY_CHARS), "12 yeni mesaj");
    }

    #[test]
    fn the_launchers_identities_are_valid_app_ids() {
        let longest = "x".repeat(32);
        for identity in [
            Identity::production(),
            Identity::resolve(true, None).unwrap(),
            Identity::resolve(true, Some("e2e-7")).unwrap(),
            Identity::resolve(true, Some(&longest)).unwrap(),
        ] {
            assert!(
                valid_app_id(&identity.identifier),
                "{}",
                identity.identifier
            );
        }
        for bad in [
            "",
            "Projects Launcher",
            r"gg\projects",
            "gg.projekt.ş",
            &"x".repeat(129),
        ] {
            assert!(!valid_app_id(bad), "{bad:?}");
        }
    }
}

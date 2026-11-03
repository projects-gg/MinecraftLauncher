//! Who a crew chat message mentions (OVL-SOC 2, SOC-NEXT §3.1): the launcher's rule
//! (`apps/launcher/src/features/social/next/mentions.ts`, `mentionsIn` and `mentionOf`) for the
//! overlay's cards and the quiet gold ring on its bubbles. A message goes as plain text: whoever
//! reads "@Kaan_07" is mentioned when it names them, matched without case or Turkish marks and at a
//! word's edge, the longest member name first (names may have spaces). A message that mentions
//! more than five members tells nobody (its ring stays).

/// A message that mentions more members than this tells none of them (`MENTION_NOTIFY_MAX`).
pub const MENTION_NOTIFY_MAX: usize = 5;

/// How a message is heard by the player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mention {
    /// It does not mention them.
    None,
    /// It mentions them, and few enough others: heard as a reply to them is.
    Me,
    /// It mentions them among more than [`MENTION_NOTIFY_MAX`] members: the ring only.
    Crowd,
}

/// Search folding for Turkish players, `@pl/ui/search`'s `fold` on Latin text (the same as
/// `pl_items::fold`): lower case the Turkish way ("I", "İ" and "ı" all become "i") and marks
/// dropped ("ş" → "s", "ğ" → "g").
pub fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        fold_into(c, &mut out);
    }
    out
}

fn fold_into(c: char, out: &mut String) {
    if matches!(c, 'I' | 'İ' | 'ı') {
        out.push('i');
        return;
    }
    for lower in c.to_lowercase() {
        if !is_mark(lower) {
            out.push(unmarked(lower));
        }
    }
}

/// Combining marks: what canonical decomposition splits off a letter.
fn is_mark(c: char) -> bool {
    matches!(
        c as u32,
        0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF | 0xFE20..=0xFE2F
    )
}

/// A lower-case Latin letter with a mark (Latin-1 and Latin Extended-A) without it.
fn unmarked(c: char) -> char {
    match c {
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
        other => other,
    }
}

/// A letter, digit or "_" (a name goes on): anything else is a word's edge.
fn wordy(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// A crew's members as a message is matched against them: (id, folded name), the longest name
/// first (so "@Kaan Efe" is Kaan Efe, not Kaan).
pub fn roster<'a>(members: impl IntoIterator<Item = (&'a str, &'a str)>) -> Vec<(String, String)> {
    let mut names: Vec<(String, String)> = members
        .into_iter()
        .map(|(id, name)| (id.to_owned(), fold(name)))
        .filter(|(_, folded)| !folded.is_empty())
        .collect();
    names.sort_by_key(|(_, folded)| std::cmp::Reverse(folded.len()));
    names
}

/// Who `text` mentions, each once, in the order first mentioned: every "@" at a word's edge
/// followed by a member's name (the longest that fits) that ends at a word's edge.
pub fn mentions_in(text: &str, roster: &[(String, String)]) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    if !text.contains('@') || roster.is_empty() {
        return found;
    }
    let chars: Vec<char> = text.chars().collect();
    let mut from = 0;
    while let Some(offset) = chars[from..].iter().position(|c| *c == '@') {
        let at = from + offset;
        from = at + 1;
        if at > 0 && wordy(chars[at - 1]) {
            continue;
        }
        for (id, folded) in roster {
            let Some(end) = match_end(&chars, at + 1, folded) else {
                continue;
            };
            if chars.get(end).copied().is_some_and(wordy) {
                continue;
            }
            if !found.contains(id) {
                found.push(id.clone());
            }
            from = end;
            break;
        }
    }
    found
}

/// Where `folded` ends when the text from `start` folds to it, or None.
fn match_end(chars: &[char], start: usize, folded: &str) -> Option<usize> {
    let mut built = String::new();
    let mut index = start;
    for c in &chars[start..] {
        if built.len() >= folded.len() {
            break;
        }
        fold_into(*c, &mut built);
        index += 1;
        if !folded.starts_with(built.as_str()) {
            return None;
        }
    }
    (built == folded).then_some(index)
}

/// How `text` is heard by `me` (`mentionOf`).
pub fn mention_of(text: &str, me: &str, roster: &[(String, String)]) -> Mention {
    if me.is_empty() {
        return Mention::None;
    }
    let ids = mentions_in(text, roster);
    if !ids.iter().any(|id| id == me) {
        Mention::None
    } else if ids.len() > MENTION_NOTIFY_MAX {
        Mention::Crowd
    } else {
        Mention::Me
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crew() -> Vec<(String, String)> {
        roster([
            ("u-kaan", "Kaan_07"),
            ("u-ece", "Ece"),
            ("u-kaanefe", "Kaan Efe"),
            ("u-ilker", "İlker"),
            ("u-sule", "Şule"),
        ])
    }

    #[test]
    fn a_name_after_an_at_sign_at_word_edges_mentions_them() {
        let crew = crew();
        assert_eq!(mentions_in("selam @Kaan_07 gel", &crew), ["u-kaan"]);
        assert_eq!(mentions_in("@ece naber", &crew), ["u-ece"]);
        // Folded the Turkish way, marks dropped.
        assert_eq!(mentions_in("@ilker @SULE", &crew), ["u-ilker", "u-sule"]);
        // Each once, in the order first mentioned.
        assert_eq!(
            mentions_in("@Ece @Kaan_07 @ece", &crew),
            ["u-ece", "u-kaan"]
        );
        // The longest name that fits wins (a name with a space).
        assert_eq!(mentions_in("@Kaan Efe geldi", &crew), ["u-kaanefe"]);
        // Punctuation is an edge.
        assert_eq!(mentions_in("(@Ece), bak", &crew), ["u-ece"]);
    }

    #[test]
    fn an_address_a_longer_word_or_no_member_is_no_mention() {
        let crew = crew();
        assert!(mentions_in("ece@mail.com", &crew).is_empty());
        assert!(mentions_in("@Ecem gel", &crew).is_empty());
        assert!(mentions_in("@Kaan gel", &crew).is_empty());
        assert!(mentions_in("selam herkes", &crew).is_empty());
        assert!(mentions_in("@Ece", &[]).is_empty());
    }

    #[test]
    fn more_than_five_members_tell_nobody() {
        let crew = roster([
            ("a", "Aa1"),
            ("b", "Bb1"),
            ("c", "Cc1"),
            ("d", "Dd1"),
            ("e", "Ee1"),
            ("f", "Ff1"),
        ]);
        assert_eq!(mention_of("@Aa1 @Bb1", "a", &crew), Mention::Me);
        assert_eq!(mention_of("@Bb1", "a", &crew), Mention::None);
        assert_eq!(
            mention_of("@Aa1 @Bb1 @Cc1 @Dd1 @Ee1 @Ff1", "a", &crew),
            Mention::Crowd
        );
        assert_eq!(mention_of("@Aa1", "", &crew), Mention::None);
    }
}

//! Guards the voice of the Turkish texts players read (account errors, the browser sign-in
//! page): the launcher speaks to the player as "sen" and says "Tekrar dene", never with the
//! formal "siz" forms or "Lütfen".

use std::path::Path;

/// Fragments only the formal voice uses, lowercase ("Lütfen", "yeniden deneyin",
/// "dönebilirsiniz", "oynayabilirsiniz").
const FORMAL: [&str; 4] = ["lütfen", "deneyin", "ebilirsiniz", "abilirsiniz"];

/// Collects every non-comment line of the crate's sources that uses the formal voice.
fn formal_lines(dir: &Path, found: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).expect("read the source folder") {
        let path = entry.expect("read a source entry").path();
        if path.is_dir() {
            formal_lines(&path, found);
            continue;
        }
        if path.extension().is_none_or(|ext| ext != "rs") || path.ends_with("wording.rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("read a source file");
        for (index, line) in text.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            // Lowercasing "İ" leaves a combining dot behind.
            let lower = line.to_lowercase().replace('\u{307}', "");
            if FORMAL.iter().any(|fragment| lower.contains(fragment)) {
                found.push(format!("{}:{}: {}", path.display(), index + 1, line.trim()));
            }
        }
    }
}

#[test]
fn player_texts_use_the_informal_voice() {
    let mut found = Vec::new();
    formal_lines(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut found,
    );
    assert!(
        found.is_empty(),
        "formal Turkish in player texts (write \"sen\" and \"Tekrar dene\"):\n{}",
        found.join("\n")
    );
}

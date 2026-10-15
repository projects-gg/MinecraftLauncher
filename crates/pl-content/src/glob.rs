//! Minimal glob matcher for config policy rules.
//!
//! Semantics (documented in `docs/content-format.md`, mirrored by the Android app):
//! - patterns match the whole `/`-separated relative path, case-sensitively;
//! - `*` matches any run of characters except `/`;
//! - `**` matches any run of characters including `/`; `**/` also matches zero folders;
//! - `?` matches exactly one character except `/`;
//! - every other character matches itself.

/// Returns true when `path` matches `pattern`.
pub fn glob_match(pattern: &str, path: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let path: Vec<char> = path.chars().collect();
    matches(&pattern, &path)
}

fn matches(pattern: &[char], path: &[char]) -> bool {
    match pattern.first() {
        None => path.is_empty(),
        Some('*') if pattern.get(1) == Some(&'*') => {
            let rest = &pattern[2..];
            // `**/` may stand for zero folders: "config/**/x" matches "config/x".
            if rest.first() == Some(&'/') && matches(&rest[1..], path) {
                return true;
            }
            (0..=path.len()).any(|skip| matches(rest, &path[skip..]))
        }
        Some('*') => {
            let rest = &pattern[1..];
            for skip in 0..=path.len() {
                if matches(rest, &path[skip..]) {
                    return true;
                }
                if path.get(skip) == Some(&'/') {
                    break;
                }
            }
            false
        }
        Some('?') => path.first().is_some_and(|c| *c != '/') && matches(&pattern[1..], &path[1..]),
        Some(literal) => path.first() == Some(literal) && matches(&pattern[1..], &path[1..]),
    }
}

#[cfg(test)]
mod tests {
    use super::glob_match;

    #[test]
    fn single_star_stays_in_one_folder() {
        assert!(glob_match("*.txt", "options.txt"));
        assert!(!glob_match("*.txt", "config/options.txt"));
        assert!(glob_match("config/*.json", "config/a.json"));
        assert!(!glob_match("config/*.json", "config/sub/a.json"));
        assert!(glob_match("config/*", "config/a.json"));
        assert!(glob_match("mods/sodium-*.jar", "mods/sodium-0.6.0.jar"));
    }

    #[test]
    fn double_star_crosses_folders() {
        assert!(glob_match("config/**", "config/a.json"));
        assert!(glob_match("config/**", "config/a/b/c.toml"));
        assert!(!glob_match("config/**", "configs/a.json"));
        assert!(!glob_match("config/**", "config"));
        assert!(glob_match("config/**/x.json", "config/x.json"));
        assert!(glob_match("config/**/x.json", "config/a/b/x.json"));
        assert!(glob_match("**/*.json", "a.json"));
        assert!(glob_match("**/*.json", "config/a/b.json"));
        assert!(glob_match("**", "anything/at/all"));
    }

    #[test]
    fn question_mark_and_literals() {
        assert!(glob_match("option?.txt", "options.txt"));
        assert!(!glob_match("a?b", "a/b"));
        assert!(glob_match("options.txt", "options.txt"));
        assert!(!glob_match("options.txt", "Options.txt"));
        assert!(!glob_match("", "x"));
        assert!(glob_match("", ""));
    }
}

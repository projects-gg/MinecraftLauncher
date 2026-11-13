//! SRV-ACC: the player names of the player's accounts on the Projects server's site
//! (mc.projects.gg), for the account center's "Sunucu Hesapların". The friends service asks the
//! site (`GET /v1/me/server-accounts`, services/social README "Server accounts"): the site
//! accounts linked to this Projects account and every account of their proven email addresses,
//! the primary first. This keeps only what the launcher can add as username accounts.

/// The most names the launcher takes (the site sends at most 50).
pub const SERVER_ACCOUNTS_MAX: usize = 50;

/// The names in `values` the launcher can add, in order: player names (3–16 of `[A-Za-z0-9_]`),
/// each once in any letter case, at most [`SERVER_ACCOUNTS_MAX`]. Anything else is left out.
pub fn player_names(values: &[serde_json::Value]) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for value in values {
        let Some(name) = value.as_str() else {
            continue;
        };
        let valid = (3..=16).contains(&name.len())
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
        if valid && !names.iter().any(|kept| kept.eq_ignore_ascii_case(name)) {
            names.push(name.to_owned());
            if names.len() == SERVER_ACCOUNTS_MAX {
                break;
            }
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn keeps_player_names_once() {
        let values = [
            json!("KorAtesi"),
            json!("koratesi"),
            json!("x7Ender"),
            json!("ab"),
            json!("çok_uzun_bir_ad"),
            json!("has space"),
            json!("A_very_long_name_17"),
            json!(42),
            json!(null),
            json!("Gece_Kusu"),
        ];
        assert_eq!(player_names(&values), ["KorAtesi", "x7Ender", "Gece_Kusu"]);
    }

    #[test]
    fn takes_fifty_at_most() {
        let values: Vec<_> = (0..80).map(|i| json!(format!("Oyuncu_{i}"))).collect();
        let names = player_names(&values);
        assert_eq!(names.len(), SERVER_ACCOUNTS_MAX);
        assert_eq!(names[49], "Oyuncu_49");
    }
}

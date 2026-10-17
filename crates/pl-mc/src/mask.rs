//! Masking secrets in text that is shown, copied or leaves the computer: game logs in the Logs
//! tab, logs shared on mclo.gs and support bundles (`pl_data::bundle`). This is the launcher's only
//! masking code.
//!
//! Logs can hold things that must never be shared: Minecraft access tokens (game arguments,
//! old games print the session), Microsoft refresh tokens, `Authorization` headers, passwords in
//! server login commands, signing keys. [`mask_secrets`] replaces each of them with [`MASK`],
//! keeping the surrounding text readable. The OS account name in home folder paths is masked too
//! (logs end up in public support channels).

use std::borrow::Cow;
use std::sync::LazyLock;

use regex::Regex;

/// What a masked value is replaced with.
pub const MASK: &str = "********";

struct Rule {
    pattern: Regex,
    replacement: &'static str,
}

fn rule(pattern: &str, replacement: &'static str) -> Rule {
    Rule {
        pattern: Regex::new(pattern).expect("valid masking pattern"),
        replacement,
    }
}

/// Applied in order: specific shapes first, then the catch-all ones.
static RULES: LazyLock<Vec<Rule>> = LazyLock::new(|| {
    vec![
        // PEM private keys of any algorithm, across lines.
        rule(
            r"(?s)-----BEGIN ([A-Z0-9 ]*)PRIVATE KEY-----.*?-----END [A-Z0-9 ]*PRIVATE KEY-----",
            "-----BEGIN ${1}PRIVATE KEY-----********-----END ${1}PRIVATE KEY-----",
        ),
        // Game arguments that carry the session: `--accessToken <x>`, `--session <x>`, ...
        rule(
            r"(?i)(--(?:accessToken|session|sessionId|clientToken|clientId|xuid))(\s+|=)(\S+)",
            "${1}${2}********",
        ),
        // HTTP authorization values: `Bearer <x>`, Xbox `XBL3.0 x=<uhs>;<token>`.
        rule(r"(?i)\b(bearer)\s+[A-Za-z0-9._~+/=-]{6,}", "${1} ********"),
        rule(r"(?i)\b(XBL3\.0\s+x=)[^\s,'\x22]+", "${1}********"),
        // Key/value pairs in any common spelling: `accessToken=…`, `"refresh_token": "…"`,
        // `password: …`, `token:<x>:<y>` (old games print "Session ID is token:…"),
        // `PROJECTS_SIGNING_KEY=…`.
        rule(
            r#"(?i)((?:(?:access|refresh|client|session|auth|id|xbl|xsts|user|device|api)[_-]?)?token|(?:client[_-]?)?secret|password|passwd|pwd|passphrase|(?:client|signing|private|secret)[_-]?key|session[_-]?id|xuid)("?\s*[:=]\s*"?)([^\s"',;&})\]]+)"#,
            "${1}${2}********",
        ),
        // JWTs (Minecraft access tokens, Microsoft id tokens) wherever they appear.
        rule(
            r"\beyJ[A-Za-z0-9_-]{6,}\.[A-Za-z0-9_-]{6,}\.[A-Za-z0-9_-]{6,}",
            "********",
        ),
        // Microsoft refresh tokens (`M.C5xx_BAY.0.U.-…`).
        rule(
            r"\bM\.[A-Za-z0-9]{1,8}_[A-Za-z0-9]{2,8}\.[A-Za-z0-9!*$._-]{16,}",
            "********",
        ),
        // Server login commands typed by players or AFK scripts: everything after the command.
        rule(
            r"(?i)(^|[\s\x22'>:\]])(/(?:login|log|l|register|reg|giris|giriş|kayit|kayıt|changepassword|changepass|cp|unregister))[ \t]+[^\r\n]*",
            "${1}${2} ********",
        ),
        // A bare 32-byte key in base64 (the content signing key has this shape).
        rule(
            r"(^|[^A-Za-z0-9+/=])[A-Za-z0-9+/]{43}=($|[^A-Za-z0-9+/=])",
            "${1}********${2}",
        ),
        // The OS account name: home folder paths and Java's `user.name`.
        rule(
            r#"(?i)\b([A-Z]:[\\/]+(?:Users|Documents and Settings)[\\/]+)[^\\/\r\n"'<>|:*?]+"#,
            "${1}********",
        ),
        rule(
            r#"(^|[^\w.~/-])(/(?:home|Users)/)[^/\s"'<>|:*?]+"#,
            "${1}${2}********",
        ),
        rule(r"(?i)\b(user\.name\s*[=:]\s*)\S+", "${1}********"),
    ]
});

/// `text` with every secret this module knows replaced by [`MASK`]. Borrows when nothing matched.
pub fn mask_secrets(text: &str) -> Cow<'_, str> {
    let mut out = Cow::Borrowed(text);
    for rule in RULES.iter() {
        // Most text (every live game log line comes through here) matches no rule, and a plain
        // match test is several times cheaper than a replacement that finds nothing.
        if !rule.pattern.is_match(&out) {
            continue;
        }
        if let Cow::Owned(masked) = rule.pattern.replace_all(&out, rule.replacement) {
            out = Cow::Owned(masked);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn masked(text: &str) -> String {
        mask_secrets(text).into_owned()
    }

    // ---- From the support bundle's masking (formerly `pl_data::mask`).

    #[test]
    fn game_arguments_lose_the_session() {
        let line = "java -cp x net.minecraft.client.main.Main --username Hazar --accessToken eyJhbGciOi.payload --uuid 1234 --clientId Y2xpZW50 --xuid 2535";
        let out = masked(line);
        assert!(out.contains("--username Hazar"), "{out}");
        assert!(out.contains("--accessToken ********"), "{out}");
        assert!(out.contains("--clientId ********"), "{out}");
        assert!(out.contains("--xuid ********"), "{out}");
        assert!(!out.contains("eyJhbGciOi"), "{out}");
        assert!(masked("--session=token:abc:def").contains("--session=********"));
    }

    #[test]
    fn tokens_in_key_value_and_json_forms() {
        let cases = [
            ("accessToken=abc123def", "abc123def"),
            ("refresh_token=M.C507_BAY.0.U.xyz", "M.C507"),
            (
                r#"{"access_token": "secret-value", "expires_in": 3600}"#,
                "secret-value",
            ),
            (r#""refreshToken":"rt-value""#, "rt-value"),
            ("url?client_secret=s3cr3t&x=1", "s3cr3t"),
            ("password: hunter2", "hunter2"),
            (
                "Session ID is token:0123456789abcdef:fedcba9876543210",
                "0123456789abcdef",
            ),
            ("sessionId=aaaabbbbccccdddd", "aaaabbbbccccdddd"),
            ("PROJECTS_SIGNING_KEY=c2VjcmV0", "c2VjcmV0"),
            (
                "Authorization: XBL3.0 x=1234567890;eyJtokenpart",
                "1234567890",
            ),
        ];
        for (input, secret) in cases {
            let out = masked(input);
            assert!(!out.contains(secret), "{input} -> {out}");
            assert!(out.contains(MASK), "{input} -> {out}");
        }
        // The non-secret neighbour survives.
        assert!(masked(r#"{"access_token": "x1", "expires_in": 3600}"#).contains("expires_in"));
        assert!(masked("url?client_secret=s3cr3t&x=1").ends_with("&x=1"));
    }

    #[test]
    fn bearer_jwt_and_refresh_tokens_anywhere() {
        let jwt = "eyJraWQiOiJhYzg0YSIsImFsZyI6IkhTMjU2In0.eyJ4dWlkIjoiMjUzNTQwMTk2In0.c2lnbmF0dXJlX3BhcnQ";
        let out = masked(&format!("POST /x Authorization: Bearer {jwt} done"));
        assert!(!out.contains("eyJ4dWlk"), "{out}");
        assert!(out.ends_with("done"), "{out}");
        let out = masked(&format!("got profile with {jwt} for player"));
        assert_eq!(out, "got profile with ******** for player");
        let out = masked("stored M.C534_BAY.0.U.-CgYGrUtd*Mx3Bq!Rj0JkL7 in keychain");
        assert_eq!(out, "stored ******** in keychain");
    }

    #[test]
    fn login_commands_hide_their_arguments() {
        assert_eq!(masked("/login hunter2"), "/login ********");
        assert_eq!(masked("> /register pass1 pass1"), "> /register ********");
        assert_eq!(
            masked("[Chat] sent: /l sifre123"),
            "[Chat] sent: /l ********"
        );
        assert_eq!(masked("/giriş şifrem"), "/giriş ********");
        // Paths and other slash words are left alone.
        assert_eq!(
            masked("C:/Program Files/lib/x.jar"),
            "C:/Program Files/lib/x.jar"
        );
        assert_eq!(
            masked("https://login.live.com/oauth20"),
            "https://login.live.com/oauth20"
        );
    }

    #[test]
    fn signing_keys_are_masked() {
        let key = "muj3CSu3m9jpB7fUP5TbkRSnISGEH13XbTt6agmIK68=";
        assert_eq!(masked(&format!("key {key} loaded")), "key ******** loaded");
        let pem =
            "a\n-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEI\n-----END PRIVATE KEY-----\nb";
        let out = masked(pem);
        assert!(!out.contains("MC4CAQAw"), "{out}");
        assert!(out.starts_with("a\n-----BEGIN PRIVATE KEY-----") && out.ends_with("\nb"));
    }

    #[test]
    fn home_folder_names_are_hidden() {
        assert_eq!(
            masked(r"C:\Users\Hazar Yılmaz\AppData\Roaming\ProjectsLauncher"),
            r"C:\Users\********\AppData\Roaming\ProjectsLauncher"
        );
        assert_eq!(
            masked(r#"{"path":"C:\\Users\\hazar\\x"}"#),
            r#"{"path":"C:\\Users\\********\\x"}"#
        );
        assert_eq!(
            masked("/home/hazar/.local/share"),
            "/home/********/.local/share"
        );
        assert_eq!(masked("user.name = hazar"), "user.name = ********");
    }

    #[test]
    fn ordinary_log_lines_are_untouched() {
        let lines = [
            "[12:00:01] [Render thread/INFO]: Setting user: Hazar",
            "[12:00:02] [main/INFO]: Loading 54 mods: fabric-api 0.100.0+1.21.1",
            "Downloaded 3 files (12.4 MB) from https://cdn.projects.gg/v1/index.json",
            "sha256 9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08",
            "Exception in thread \"main\" java.lang.NullPointerException: tokens were empty",
        ];
        for line in lines {
            assert!(
                matches!(mask_secrets(line), Cow::Borrowed(_)),
                "changed: {line}"
            );
        }
    }

    // ---- From the game log viewer's masking (formerly `pl_mc::gamefiles::logs`).

    #[test]
    fn masks_tokens_and_home_folders_in_game_logs() {
        let jwt = "eyJraWQiOiJhYmMiLCJhbGciOiJSUzI1NiJ9.eyJ4dWlkIjoiMjUzNSJ9.c2lnbmF0dXJlX3BhcnQ";
        let text = format!(
            "# command: java -cp x net.minecraft.client.main.Main --username Steve --accessToken {jwt} --uuid 1234 --xuid 2535405290 --clientId Y2xpZW50 --session abc123\n\
             (Session ID is token:0123456789abcdef0123456789abcdef:4566e69fc90748ee8d71d7ba5aa00d20)\n\
             Authorization: Bearer EwBIA+l3BAAUtokenvalue123\n\
             {{\"accessToken\":\"secretvalue\",\"refresh_token\": \"r3fr3sh\"}}\n\
             url?access_token=abc&client=1 password=hunter2\n\
             stray {jwt} here\n\
             C:\\Users\\Ali Veli\\AppData\\Roaming\\ProjectsLauncher and C:/Users/hazar/x\n\
             -Djava.library.path=/home/steve/.local/share and /Users/steve/Library\n\
             Loaded 5 tokens: fine, Setting user: Steve, tokenizer=on\n"
        );
        let masked = mask_secrets(&text);
        for secret in [
            jwt,
            "2535405290",
            "Y2xpZW50",
            "abc123",
            "0123456789abcdef",
            "EwBIA",
            "secretvalue",
            "r3fr3sh",
            "access_token=abc",
            "hunter2",
            "Ali Veli",
            "hazar",
            "/home/steve",
            "/Users/steve",
        ] {
            assert!(!masked.contains(secret), "{secret} leaked:\n{masked}");
        }
        for kept in [
            "--username Steve",
            "--uuid 1234",
            "--accessToken ********",
            "Bearer ********",
            "\"accessToken\":\"********\"",
            "C:\\Users\\********\\AppData",
            "Loaded 5 tokens: fine",
            "Setting user: Steve",
            "tokenizer=on",
        ] {
            assert!(masked.contains(kept), "{kept} missing:\n{masked}");
        }
        // Nothing to mask: no copy.
        assert!(matches!(
            mask_secrets("[12:00:00] [main/INFO]: hello"),
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn already_masked_text_stays_masked() {
        let once = masked("--accessToken abc token=xyz /login pw");
        assert_eq!(masked(&once), once);
        assert!(!once.contains("abc") && !once.contains("xyz") && !once.contains(" pw"));
    }
}

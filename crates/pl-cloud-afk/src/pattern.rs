//! "ProBot AI ile kalıp kur" (the service's README "Pattern helper"): the player says in their own
//! words which chat messages an auto-reply rule should answer, and the service answers with a
//! trigger and one plain sentence about it, as on afk.projects.gg. Free for players; the service
//! counts the calls that reach its model per account (3 a minute, 20 a day), so nothing here repeats
//! a request by itself. `/offer` → `ai.patterns` says whether to offer the helper at all.

use serde::{Deserialize, Serialize};

use crate::error::{CloudError, Result};

/// A description, in characters (the service's limit).
pub const PATTERN_TEXT_MAX: usize = 200;
/// What an answer may hold before the launcher calls it broken: above the service's own limits
/// (sentence 160, example 80, trigger 160 characters), so a small change there breaks nothing.
const SUMMARY_MAX: usize = 300;
const EXAMPLE_MAX: usize = 160;

/// `POST /accounts/:id/rules/pattern` → `suggestion`: a trigger made from the player's words. Every
/// field is the contract's (no defaults: one shape for both directions in the bindings).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PatternSuggestion {
    /// False: the message contains `match` (any letter case); true: a pattern that starts with
    /// `(?i)`.
    pub regex: bool,
    /// The rule's trigger, ready to save.
    #[serde(rename = "match")]
    pub pattern: String,
    /// One sentence a 13-year-old understands: when the rule answers.
    pub summary: String,
    /// One to three messages it answers.
    pub matches: Vec<String>,
    /// One or two similar messages it leaves alone.
    pub misses: Vec<String>,
    /// `plain` (the words themselves, no model), `ai` or `cache`: "Tekrar dene" only after the
    /// model.
    pub source: String,
}

impl PatternSuggestion {
    /// The answer as the launcher uses it: a trigger the rule can hold (pl-afk's own check, as the
    /// machine runs it), a sentence and short examples. Anything else breaks the contract.
    pub(crate) fn checked(mut self) -> Result<Self> {
        let pattern = self.pattern.trim();
        let line = |text: &String| {
            let count = text.trim().chars().count();
            count > 0 && count <= EXAMPLE_MAX && !text.chars().any(char::is_control)
        };
        let summary = self.summary.trim().chars().count();
        let usable = !pattern.is_empty()
            && pl_afk::check_pattern(pattern, self.regex).is_none()
            && summary > 0
            && summary <= SUMMARY_MAX
            && !self.summary.chars().any(char::is_control)
            && (1..=3).contains(&self.matches.len())
            && self.misses.len() <= 2
            && self.matches.iter().chain(&self.misses).all(line);
        if !usable {
            return Err(CloudError::Protocol(
                "pattern suggestion out of contract".into(),
            ));
        }
        self.pattern = pattern.to_owned();
        self.summary = self.summary.trim().to_owned();
        if !matches!(self.source.as_str(), "plain" | "ai" | "cache") {
            // A source this version does not know: offer "Tekrar dene" as after the model.
            self.source = "ai".to_owned();
        }
        Ok(self)
    }
}

/// The description as the service reads it: spaces folded, 1–200 characters, no control
/// character. None: nothing to send.
pub fn pattern_text(text: &str) -> Option<String> {
    let folded = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let count = folded.chars().count();
    (count > 0 && count <= PATTERN_TEXT_MAX && !folded.chars().any(char::is_control))
        .then_some(folded)
}

/// How the helper's refusals read to the player (the site's words; the launcher's command words
/// them).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternTrouble {
    /// `422 pattern_failed`: no usable trigger from these words ("Başka sözlerle anlat").
    Unclear,
    /// `503 ai_unavailable` `busy`: the model is rate-limited or overloaded for a moment.
    Busy,
    /// `503 ai_unavailable` `off` or `budget`: the helper cannot answer now (a plain phrase or a
    /// pattern written by hand still work).
    Off,
}

impl PatternTrouble {
    pub fn of(error: &CloudError) -> Option<Self> {
        match error {
            CloudError::Refused { code, .. } if code == "pattern_failed" => Some(Self::Unclear),
            CloudError::Refused { code, .. } if code == "ai_unavailable" => {
                Some(if error.reason() == Some("busy") {
                    Self::Busy
                } else {
                    Self::Off
                })
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Map, Value, json};

    use super::*;

    fn suggestion(value: Value) -> PatternSuggestion {
        serde_json::from_value(value).expect("a suggestion")
    }

    #[test]
    fn reads_the_contract_and_keeps_what_the_rule_can_hold() {
        let made = suggestion(json!({
            "regex": true,
            "match": " (?i)\\b(tp|[ıi][şs][ıi]nla)\\b ",
            "summary": "Mesajda \"tp\" ya da \"ışınla\" geçerse yanıt verir. ",
            "matches": ["tp atar mısın", "beni ışınla"],
            "misses": ["tpa geldim"],
            "source": "ai"
        }))
        .checked()
        .expect("usable");
        assert!(made.regex);
        assert_eq!(made.pattern, "(?i)\\b(tp|[ıi][şs][ıi]nla)\\b");
        assert_eq!(
            made.summary,
            "Mesajda \"tp\" ya da \"ışınla\" geçerse yanıt verir."
        );
        assert_eq!(made.source, "ai");
        let plain = suggestion(json!({
            "regex": false,
            "match": "selam",
            "summary": "Mesajda \"selam\" geçerse yanıt verir.",
            "matches": ["selam", "Selam"],
            "misses": ["merhaba"],
            "source": "plain"
        }))
        .checked()
        .expect("usable");
        assert_eq!(plain.source, "plain");
        let unknown = suggestion(json!({
            "regex": false,
            "match": "selam",
            "summary": "S.",
            "matches": ["selam"],
            "misses": [],
            "source": "new"
        }))
        .checked()
        .expect("usable");
        assert_eq!(unknown.source, "ai");
    }

    #[test]
    fn refuses_an_answer_out_of_contract() {
        let base = json!({
            "regex": true,
            "match": "(?i)^!davet \\w+$",
            "summary": "Mesaj \"!davet\" ile başlarsa yanıt verir.",
            "matches": ["!davet Kaan"],
            "misses": ["davet et"],
            "source": "ai"
        });
        assert!(suggestion(base.clone()).checked().is_ok());
        let broken = |key: &str, value: Value| {
            let mut changed = base.clone();
            changed[key] = value;
            suggestion(changed).checked().is_err()
        };
        assert!(broken("match", json!("")));
        assert!(
            broken("match", json!("(?i)(a+)+$")),
            "too complex for the machine"
        );
        assert!(broken("match", json!("(?i)(unclosed")), "not a pattern");
        assert!(broken("summary", json!(" ")));
        assert!(broken("summary", json!("x".repeat(301))));
        assert!(broken("matches", json!([])));
        assert!(broken("matches", json!(["a", "b", "c", "d"])));
        assert!(broken("misses", json!(["a", "b", "c"])));
        assert!(broken("matches", json!(["line\u{0007}"])));
        assert!(broken("misses", json!(["x".repeat(161)])));
    }

    #[test]
    fn a_missing_field_breaks_the_contract() {
        let partial =
            json!({ "regex": false, "match": "selam", "summary": "S.", "matches": ["selam"] });
        assert!(serde_json::from_value::<PatternSuggestion>(partial).is_err());
    }

    #[test]
    fn folds_the_description_as_the_service_does() {
        assert_eq!(
            pattern_text("  biri  tp\tya da\nışınla yazınca ").as_deref(),
            Some("biri tp ya da ışınla yazınca")
        );
        assert_eq!(pattern_text("   "), None);
        assert_eq!(
            pattern_text(&"ş".repeat(200)).map(|t| t.chars().count()),
            Some(200)
        );
        assert_eq!(pattern_text(&"ş".repeat(201)), None);
        assert_eq!(pattern_text("tp\u{0000}"), None);
    }

    #[test]
    fn words_the_helpers_refusals() {
        let refused = |status: u16, code: &str, extra: Value| CloudError::Refused {
            status,
            code: code.to_owned(),
            extra: match extra {
                Value::Object(map) => map,
                _ => Map::new(),
            },
        };
        assert_eq!(
            PatternTrouble::of(&refused(
                422,
                "pattern_failed",
                json!({"reason": "unclear"})
            )),
            Some(PatternTrouble::Unclear)
        );
        assert_eq!(
            PatternTrouble::of(&refused(503, "ai_unavailable", json!({"reason": "busy"}))),
            Some(PatternTrouble::Busy)
        );
        for reason in ["off", "budget"] {
            assert_eq!(
                PatternTrouble::of(&refused(503, "ai_unavailable", json!({"reason": reason}))),
                Some(PatternTrouble::Off)
            );
        }
        assert_eq!(
            PatternTrouble::of(&refused(409, "rev_conflict", json!({}))),
            None
        );
        assert_eq!(
            PatternTrouble::of(&CloudError::RateLimited {
                retry_after: Some(20),
                reason: None
            }),
            None
        );
    }

    #[test]
    fn the_services_envelope_becomes_a_trouble() {
        let error = crate::error::from_response(
            503,
            Some("30"),
            br#"{"error": {"code": "ai_unavailable", "message": "x", "reason": "busy"}}"#,
        );
        assert_eq!(PatternTrouble::of(&error), Some(PatternTrouble::Busy));
        let error = crate::error::from_response(
            422,
            None,
            br#"{"error": {"code": "pattern_failed", "message": "x", "reason": "unclear"}}"#,
        );
        assert_eq!(PatternTrouble::of(&error), Some(PatternTrouble::Unclear));
    }
}

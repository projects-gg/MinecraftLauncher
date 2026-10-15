//! `rules[]` evaluation for libraries and arguments, and argument flattening.
//!
//! Semantics follow the official launcher: no rules means allowed; otherwise start disallowed and
//! let every matching rule set the outcome (the last match wins). A rule matches when its `os`
//! block (name, version regex, arch) and every `features` flag match.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::platform::Platform;
use crate::version::model::{Argument, OsRule, Rule, RuleAction};

/// Launcher feature flags referenced by argument rules.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "camelCase")]
pub struct Features {
    pub is_demo_user: bool,
    pub has_custom_resolution: bool,
    pub has_quick_plays_support: bool,
    pub is_quick_play_singleplayer: bool,
    pub is_quick_play_multiplayer: bool,
    pub is_quick_play_realms: bool,
}

impl Features {
    /// Value of a feature by its JSON name; unknown features are `false`.
    pub fn get(&self, name: &str) -> bool {
        match name {
            "is_demo_user" => self.is_demo_user,
            "has_custom_resolution" => self.has_custom_resolution,
            "has_quick_plays_support" => self.has_quick_plays_support,
            "is_quick_play_singleplayer" => self.is_quick_play_singleplayer,
            "is_quick_play_multiplayer" => self.is_quick_play_multiplayer,
            "is_quick_play_realms" => self.is_quick_play_realms,
            _ => false,
        }
    }
}

pub fn os_rule_matches(os: &OsRule, platform: &Platform) -> bool {
    if let Some(name) = &os.name
        && !platform.os.matches_name(name)
    {
        return false;
    }
    if let Some(pattern) = &os.version {
        match regex::Regex::new(pattern) {
            Ok(re) if re.is_match(&platform.os_version) => {}
            Ok(_) => return false,
            Err(error) => {
                tracing::warn!(%pattern, %error, "invalid OS version regex in rule");
                return false;
            }
        }
    }
    if let Some(arch) = &os.arch
        && !platform.arch.matches_rule(arch)
    {
        return false;
    }
    true
}

pub fn rule_matches(rule: &Rule, platform: &Platform, features: &Features) -> bool {
    if let Some(os) = &rule.os
        && !os_rule_matches(os, platform)
    {
        return false;
    }
    if let Some(wanted) = &rule.features
        && !wanted
            .iter()
            .all(|(name, value)| features.get(name) == *value)
    {
        return false;
    }
    true
}

pub fn rules_allow(rules: &[Rule], platform: &Platform, features: &Features) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in rules {
        if rule_matches(rule, platform, features) {
            allowed = rule.action == RuleAction::Allow;
        }
    }
    allowed
}

/// Flatten arguments for `platform`/`features` (placeholders are left untouched).
pub fn evaluate_arguments(
    args: &[Argument],
    platform: &Platform,
    features: &Features,
) -> Vec<String> {
    let mut out = Vec::with_capacity(args.len());
    for arg in args {
        match arg {
            Argument::Plain(value) => out.push(value.clone()),
            Argument::Conditional { rules, value } => {
                if rules_allow(rules, platform, features) {
                    out.extend(value.values().into_iter().map(str::to_owned));
                }
            }
        }
    }
    out
}

/// Feature names referenced by argument rules (e.g. to detect quick play support).
pub fn referenced_features(args: &[Argument]) -> BTreeSet<String> {
    args.iter()
        .filter_map(|arg| match arg {
            Argument::Conditional { rules, .. } => Some(rules),
            Argument::Plain(_) => None,
        })
        .flatten()
        .filter_map(|rule| rule.features.as_ref())
        .flat_map(|features| features.keys().cloned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{Arch, Os};

    fn rules(json: &str) -> Vec<Rule> {
        serde_json::from_str(json).unwrap()
    }

    fn platform(os: Os, arch: Arch, version: &str) -> Platform {
        Platform::new(os, arch, version)
    }

    #[test]
    fn empty_rules_allow() {
        let p = platform(Os::Linux, Arch::X86_64, "6.1");
        assert!(rules_allow(&[], &p, &Features::default()));
    }

    #[test]
    fn allow_then_disallow_os() {
        let r = rules(r#"[{"action": "allow"}, {"action": "disallow", "os": {"name": "osx"}}]"#);
        let f = Features::default();
        assert!(rules_allow(
            &r,
            &platform(Os::Windows, Arch::X86_64, "10.0"),
            &f
        ));
        assert!(rules_allow(
            &r,
            &platform(Os::Linux, Arch::X86_64, "6.1"),
            &f
        ));
        assert!(!rules_allow(
            &r,
            &platform(Os::MacOs, Arch::Aarch64, "14.5"),
            &f
        ));
    }

    #[test]
    fn allow_only_os() {
        let r = rules(r#"[{"action": "allow", "os": {"name": "osx"}}]"#);
        let f = Features::default();
        assert!(rules_allow(
            &r,
            &platform(Os::MacOs, Arch::X86_64, "13.0"),
            &f
        ));
        assert!(!rules_allow(
            &r,
            &platform(Os::Windows, Arch::X86_64, "10.0"),
            &f
        ));
    }

    #[test]
    fn os_version_regex() {
        let leopard = rules(
            r#"[{"action": "allow"}, {"action": "disallow", "os": {"name": "osx", "version": "^10\\.5\\.\\d$"}}]"#,
        );
        let f = Features::default();
        assert!(!rules_allow(
            &leopard,
            &platform(Os::MacOs, Arch::X86, "10.5.8"),
            &f
        ));
        assert!(rules_allow(
            &leopard,
            &platform(Os::MacOs, Arch::X86_64, "10.15.7"),
            &f
        ));
        let win10 =
            rules(r#"[{"action": "allow", "os": {"name": "windows", "version": "^10\\."}}]"#);
        assert!(rules_allow(
            &win10,
            &platform(Os::Windows, Arch::X86_64, "10.0.26100"),
            &f
        ));
        assert!(!rules_allow(
            &win10,
            &platform(Os::Windows, Arch::X86_64, "6.1.7601"),
            &f
        ));
        let broken = rules(r#"[{"action": "allow", "os": {"version": "(["}}]"#);
        assert!(!rules_allow(
            &broken,
            &platform(Os::Windows, Arch::X86_64, "10.0"),
            &f
        ));
    }

    #[test]
    fn arch_rule() {
        let r = rules(r#"[{"action": "allow", "os": {"arch": "x86"}}]"#);
        let f = Features::default();
        assert!(rules_allow(
            &r,
            &platform(Os::Windows, Arch::X86, "10.0"),
            &f
        ));
        assert!(!rules_allow(
            &r,
            &platform(Os::Windows, Arch::X86_64, "10.0"),
            &f
        ));
        assert!(!rules_allow(
            &r,
            &platform(Os::MacOs, Arch::Aarch64, "14.0"),
            &f
        ));
    }

    #[test]
    fn feature_rules_and_arguments() {
        let args: Vec<Argument> = serde_json::from_str(
            r#"["--username", "${auth_player_name}",
                {"rules": [{"action": "allow", "features": {"is_demo_user": true}}], "value": "--demo"},
                {"rules": [{"action": "allow", "features": {"has_custom_resolution": true}}],
                 "value": ["--width", "${resolution_width}", "--height", "${resolution_height}"]},
                {"rules": [{"action": "allow", "features": {"is_quick_play_multiplayer": true}}],
                 "value": ["--quickPlayMultiplayer", "${quickPlayMultiplayer}"]},
                {"rules": [{"action": "allow", "features": {"some_future_flag": true}}], "value": "--future"},
                {"rules": [{"action": "allow", "os": {"name": "osx"}}], "value": ["-XstartOnFirstThread"]}]"#,
        )
        .unwrap();
        let win = platform(Os::Windows, Arch::X86_64, "10.0");
        let plain = evaluate_arguments(&args, &win, &Features::default());
        assert_eq!(plain, ["--username", "${auth_player_name}"]);

        let features = Features {
            has_custom_resolution: true,
            is_quick_play_multiplayer: true,
            ..Features::default()
        };
        let with = evaluate_arguments(&args, &win, &features);
        assert_eq!(
            with,
            [
                "--username",
                "${auth_player_name}",
                "--width",
                "${resolution_width}",
                "--height",
                "${resolution_height}",
                "--quickPlayMultiplayer",
                "${quickPlayMultiplayer}"
            ]
        );
        let mac = platform(Os::MacOs, Arch::Aarch64, "14.0");
        assert!(
            evaluate_arguments(&args, &mac, &Features::default())
                .contains(&"-XstartOnFirstThread".to_owned())
        );

        let referenced = referenced_features(&args);
        assert!(referenced.contains("is_quick_play_multiplayer"));
        assert!(referenced.contains("is_demo_user"));
    }

    #[test]
    fn disallow_feature_rule() {
        let r = rules(r#"[{"action": "disallow", "features": {"is_demo_user": true}}]"#);
        let p = platform(Os::Linux, Arch::X86_64, "6.1");
        // Only a disallow rule: nothing ever allows it.
        assert!(!rules_allow(&r, &p, &Features::default()));
    }
}

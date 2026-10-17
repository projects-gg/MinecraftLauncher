//! The settings page as the UI sees it.
//!
//! [`AccountSettings`] carries the login commands verbatim, and they may contain passwords. Those
//! never leave the keychain: [`SettingsPage`] lists a stored login/register/password command that
//! has arguments only by its name ([`LoginCommandLine::Stored`], shown as "kayıtlı"). Sending such a
//! line back unchanged keeps the stored command, so the page can be saved, reordered or trimmed
//! without the UI ever holding the password; replacing it means typing a new line.
//!
//! Which commands count as secret is the console's rule ([`is_sensitive_command`]), so a command
//! the console masks is also masked here.

use serde::{Deserialize, Serialize};

use crate::console::is_sensitive_command;
use crate::error::{AfkError, Result};
use crate::form::{AccountSettings, AutoRespondSettings, ChatLogSettings, LoginCommandsSettings};
use crate::model::{
    AlertsOptions, AntiAfkOptions, AutoEatOptions, AutoRelogOptions, SchedulerOptions,
};
use crate::util::eq_ignore_case;

/// One line of the login commands list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LoginCommandLine {
    /// Shown and saved as typed (commands without secrets, and every new or edited line).
    Text { text: String },
    /// A stored command that carries a secret (`/login …`): only `command` (`/login`) is shown.
    /// Sent back as it is, it keeps stored command number `index` of the list the page was built
    /// from.
    Stored { index: u32, command: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct LoginCommandsPage {
    pub enabled: bool,
    pub only_first_login: bool,
    pub delay_seconds: f64,
    pub commands: Vec<LoginCommandLine>,
}

impl Default for LoginCommandsPage {
    fn default() -> Self {
        let defaults = LoginCommandsSettings::default();
        Self {
            enabled: defaults.enabled,
            only_first_login: defaults.only_first_login,
            delay_seconds: defaults.delay_seconds,
            commands: Vec::new(),
        }
    }
}

/// [`AccountSettings`] with the login commands' secrets left in the keychain. Same fields, same
/// meaning; see the field docs there.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct SettingsPage {
    pub server_host: String,
    pub server_port: i32,
    pub minecraft_version: String,
    pub auto_start: bool,
    pub allow_resource_packs: bool,
    pub login_commands: LoginCommandsPage,
    pub scheduler: SchedulerOptions,
    pub anti_afk: AntiAfkOptions,
    pub auto_relog: AutoRelogOptions,
    pub auto_eat: AutoEatOptions,
    pub auto_respond: AutoRespondSettings,
    pub chat_log: ChatLogSettings,
    pub alerts: AlertsOptions,
}

impl Default for SettingsPage {
    fn default() -> Self {
        Self::from_settings(AccountSettings::default())
    }
}

/// The name shown for a command that carries a secret, or `None` when nothing needs hiding (not a
/// login/register/password command, or one without arguments).
pub fn secret_command_name(command: &str) -> Option<&str> {
    let trimmed = command.trim();
    let name = trimmed.split([' ', '\t']).next().unwrap_or(trimmed);
    (is_sensitive_command(trimmed) && !trimmed[name.len()..].trim().is_empty()).then_some(name)
}

impl SettingsPage {
    /// Builds the page from the form. Blank stored lines are left out (saving drops them anyway);
    /// `Stored` indices still point into the full stored list.
    pub fn from_settings(settings: AccountSettings) -> Self {
        let AccountSettings {
            server_host,
            server_port,
            minecraft_version,
            auto_start,
            allow_resource_packs,
            login_commands,
            scheduler,
            anti_afk,
            auto_relog,
            auto_eat,
            auto_respond,
            chat_log,
            alerts,
        } = settings;
        let commands = login_commands
            .commands
            .iter()
            .enumerate()
            .filter(|(_, command)| !command.trim().is_empty())
            .map(|(index, command)| match secret_command_name(command) {
                Some(name) => LoginCommandLine::Stored {
                    index: u32::try_from(index).unwrap_or(u32::MAX),
                    command: name.to_owned(),
                },
                None => LoginCommandLine::Text {
                    text: command.clone(),
                },
            })
            .collect();
        Self {
            server_host,
            server_port,
            minecraft_version,
            auto_start,
            allow_resource_packs,
            login_commands: LoginCommandsPage {
                enabled: login_commands.enabled,
                only_first_login: login_commands.only_first_login,
                delay_seconds: login_commands.delay_seconds,
                commands,
            },
            scheduler,
            anti_afk,
            auto_relog,
            auto_eat,
            auto_respond,
            chat_log,
            alerts,
        }
    }

    /// The form to validate and save. `Stored` lines are resolved against `stored`, the commands in
    /// the keychain now; [`AfkError::LoginCommandsChanged`] when one no longer matches (the list
    /// changed after the page was built), so a secret is never swapped for another one.
    pub fn into_settings(self, stored: &[String]) -> Result<AccountSettings> {
        let Self {
            server_host,
            server_port,
            minecraft_version,
            auto_start,
            allow_resource_packs,
            login_commands,
            scheduler,
            anti_afk,
            auto_relog,
            auto_eat,
            auto_respond,
            chat_log,
            alerts,
        } = self;
        let mut commands = Vec::with_capacity(login_commands.commands.len());
        for line in login_commands.commands {
            match line {
                LoginCommandLine::Text { text } => commands.push(text),
                LoginCommandLine::Stored { index, command } => {
                    let current = usize::try_from(index)
                        .ok()
                        .and_then(|i| stored.get(i))
                        .filter(|current| {
                            secret_command_name(current)
                                .is_some_and(|name| eq_ignore_case(name, command.trim()))
                        })
                        .ok_or(AfkError::LoginCommandsChanged)?;
                    commands.push(current.clone());
                }
            }
        }
        Ok(AccountSettings {
            server_host,
            server_port,
            minecraft_version,
            auto_start,
            allow_resource_packs,
            login_commands: LoginCommandsSettings {
                enabled: login_commands.enabled,
                only_first_login: login_commands.only_first_login,
                delay_seconds: login_commands.delay_seconds,
                commands,
            },
            scheduler,
            anti_afk,
            auto_relog,
            auto_eat,
            auto_respond,
            chat_log,
            alerts,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::AfkAccount;
    use crate::versions::VersionCatalog;

    fn page_with(commands: &[&str]) -> (SettingsPage, Vec<String>) {
        let stored: Vec<String> = commands.iter().map(|c| (*c).to_owned()).collect();
        let settings = AccountSettings::from_account(
            &AfkAccount::new("Test"),
            &stored,
            &VersionCatalog::built_in(),
        );
        (SettingsPage::from_settings(settings), stored)
    }

    #[test]
    fn secrets_are_listed_by_name_only() {
        let (page, _) = page_with(&[
            "/login hunter22",
            "  ",
            "/warp afk",
            "/LOGIN",
            "merhaba millet",
            "/register gizli gizli",
        ]);
        assert_eq!(
            page.login_commands.commands,
            vec![
                LoginCommandLine::Stored {
                    index: 0,
                    command: "/login".into()
                },
                LoginCommandLine::Text {
                    text: "/warp afk".into()
                },
                LoginCommandLine::Text {
                    text: "/LOGIN".into()
                },
                LoginCommandLine::Text {
                    text: "merhaba millet".into()
                },
                LoginCommandLine::Stored {
                    index: 5,
                    command: "/register".into()
                },
            ]
        );
        let json = serde_json::to_string(&page).unwrap();
        assert!(
            !json.contains("hunter22") && !json.contains("gizli"),
            "{json}"
        );
        assert!(json.contains(r#"{"kind":"stored","index":0,"command":"/login"}"#));
        assert!(json.contains(r#""loginCommands":{"enabled":false"#));
    }

    #[test]
    fn stored_lines_resolve_to_the_secret_and_edits_are_kept() {
        let (mut page, stored) = page_with(&["/login hunter22", "/warp afk"]);
        page.login_commands.enabled = true;
        // Reordered, one plain line edited, one added.
        page.login_commands.commands = vec![
            LoginCommandLine::Text {
                text: "/warp spawn".into(),
            },
            LoginCommandLine::Stored {
                index: 0,
                command: "/login".into(),
            },
            LoginCommandLine::Text {
                text: "/kit".into(),
            },
        ];
        let settings = page.into_settings(&stored).unwrap();
        assert!(settings.login_commands.enabled);
        assert_eq!(
            settings.login_commands.commands,
            vec!["/warp spawn", "/login hunter22", "/kit"]
        );
    }

    #[test]
    fn a_changed_list_is_never_resolved_to_another_secret() {
        let (page, _) = page_with(&["/login eski", "/kit"]);
        // The keychain now holds a different list.
        for stored in [
            vec!["/kit".to_owned()],
            vec!["/register yeni".to_owned()],
            vec!["/login".to_owned()],
            Vec::new(),
        ] {
            assert!(matches!(
                page.clone().into_settings(&stored),
                Err(AfkError::LoginCommandsChanged)
            ));
        }
        // Same command name, new password: the stored one is kept (it is what "keep" means).
        let settings = page
            .into_settings(&["/LOGIN yeni".to_owned(), "/kit".to_owned()])
            .unwrap();
        assert_eq!(
            settings.login_commands.commands,
            vec!["/LOGIN yeni", "/kit"]
        );
    }

    #[test]
    fn everything_else_round_trips_unchanged() {
        let mut account = AfkAccount::new("Test");
        account.server_host = "mc.example.org".into();
        account.anti_afk.command = "/ping".into();
        account.alerts.matches = vec!["elmas".into()];
        account.scheduler.enabled = true;
        let settings = AccountSettings::from_account(
            &account,
            &["/kit".to_owned()],
            &VersionCatalog::built_in(),
        );
        let page = SettingsPage::from_settings(settings.clone());
        assert_eq!(page.into_settings(&["/kit".to_owned()]).unwrap(), settings);
        assert_eq!(
            SettingsPage::default().into_settings(&[]).unwrap(),
            AccountSettings::default()
        );
    }
}

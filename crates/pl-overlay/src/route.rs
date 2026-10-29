//! AUTOJOIN: whether a launch hands the overlay mod the Projects sub-server the player picked
//! beside "Doğrudan sunucuya bağlan" ([`crate::ROUTE_ARG`], [`crate::route_jvm_arg`]). The mod
//! sends the sub-server's command once, after the lobby's `ready` on `projects:route`; this decides
//! only what the game starts with, from the signed config (the page names an id, never a command).

use pl_content::{LauncherConfig, SubServer};

/// What the launch knows when it decides.
#[derive(Debug, Clone, Copy)]
pub struct RouteLaunch<'a> {
    /// The signed launcher config (Studio's sub-server lists and `features.subServers`).
    pub config: &'a LauncherConfig,
    /// The instance's own server ("Doğrudan sunucuya bağlan"'s address, `host[:port]`).
    pub instance_server: Option<&'a str>,
    /// The game joins that server through the switch: OYNA with "Doğrudan sunucuya bağlan" on,
    /// no target of its own ("Devam et", an invite's or an event's "Katıl", a crew call).
    pub joins_own_server: bool,
    /// This launcher's own overlay mod is in place for the game (not a pack's older copy).
    pub our_mod: bool,
    /// The player's pick for this instance (a sub-server id); none: the lobby.
    pub pick: Option<&'a str>,
}

/// The sub-server the game starts with, or none: only for OYNA to the instance's own Projects
/// server, with this launcher's mod, `features.subServers` on and a pick that is one of the
/// server's sub-servers now. A pick marked [`SubServer::testers_only`] is the caller's to check
/// against the beta program before passing it.
pub fn route_pick(launch: RouteLaunch<'_>) -> Option<&SubServer> {
    if !launch.joins_own_server || !launch.our_mod {
        return None;
    }
    launch
        .config
        .sub_server(launch.instance_server?, launch.pick?)
}

#[cfg(test)]
mod tests {
    use pl_content::ServerEntry;

    use super::*;

    fn sub(id: &str, name: &str, command: &str) -> SubServer {
        SubServer {
            id: id.into(),
            name: name.into(),
            command: command.into(),
            testers_only: false,
        }
    }

    fn config(on: bool) -> LauncherConfig {
        let mut config = LauncherConfig {
            servers: vec![ServerEntry {
                id: "main".into(),
                name: "Projects".into(),
                address: "play.projects.gg".into(),
                port: None,
                featured: true,
                description: None,
                icon: None,
                sub_servers: vec![
                    sub("skyblock", "SkyBlock", "/aurora"),
                    sub("broken", "Broken", "aurora"),
                ],
            }],
            ..LauncherConfig::default()
        };
        config.features.sub_servers = on;
        config
    }

    fn launch(config: &LauncherConfig) -> RouteLaunch<'_> {
        RouteLaunch {
            config,
            instance_server: Some("play.projects.gg"),
            joins_own_server: true,
            our_mod: true,
            pick: Some("skyblock"),
        }
    }

    #[test]
    fn only_oyna_to_the_own_projects_server_with_our_mod_gets_the_pick() {
        let on = config(true);
        assert_eq!(
            route_pick(launch(&on)).map(|sub| sub.command.as_str()),
            Some("/aurora")
        );
        // "Devam et", an invite or an event: their own target.
        let own_target = RouteLaunch {
            joins_own_server: false,
            ..launch(&on)
        };
        assert!(route_pick(own_target).is_none());
        // A pack's older overlay jar, or none at all.
        let other_mod = RouteLaunch {
            our_mod: false,
            ..launch(&on)
        };
        assert!(route_pick(other_mod).is_none());
        // The lobby (no pick), a pick Studio no longer lists, a row that breaks a rule.
        for pick in [None, Some("smp"), Some("broken")] {
            assert!(
                route_pick(RouteLaunch {
                    pick,
                    ..launch(&on)
                })
                .is_none(),
                "{pick:?}"
            );
        }
        // Another server, or none.
        for instance_server in [None, Some("mc.example.net")] {
            let elsewhere = RouteLaunch {
                instance_server,
                ..launch(&on)
            };
            assert!(route_pick(elsewhere).is_none(), "{instance_server:?}");
        }
        // The switch off: nothing, whatever the pick.
        let off = config(false);
        assert!(route_pick(launch(&off)).is_none());
    }
}

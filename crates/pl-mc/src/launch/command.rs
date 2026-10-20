//! Launch command construction: JVM arguments (version JSON or legacy defaults), memory, user
//! arguments, log4j configuration, main class and game arguments with every placeholder
//! substituted.

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::ops::Range;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::install::InstalledVersion;
use crate::launch::identity::{LaunchIdentity, UserType};
use crate::paths::DataDirs;
use crate::version::{
    Argument, ArgumentValue, Features, OsRule, ResolvedVersion, Rule, RuleAction,
    evaluate_arguments,
};

/// `${launcher_name}`.
pub const LAUNCHER_NAME: &str = "Projects Launcher";
/// Default `${launcher_version}` (the crate version; the app passes its own).
pub const LAUNCHER_VERSION: &str = env!("CARGO_PKG_VERSION");

const REDACTED: &str = "********";

/// Join a world or server directly after start.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum QuickPlay {
    /// Uses 1.20+ quick play when supported, else `--server`/`--port`.
    Multiplayer { host: String, port: Option<u16> },
    /// 1.20+ only (world folder name).
    Singleplayer { world: String },
    /// 1.20+ only.
    Realms { realm_id: String },
}

/// What the game joins right after start. The launch's own target ("Devam et", an invite's
/// "Katıl") comes first: a server joins on every version (`--server`/`--port` before 1.20), a
/// world or a realm only with the game's quick play (`supported`). Otherwise the instance's own
/// server, when auto-join is on.
pub fn pick_quick_play(
    requested: Option<&QuickPlay>,
    instance_server: Option<(&str, Option<u16>)>,
    auto_join: bool,
    supported: bool,
) -> Option<QuickPlay> {
    match (requested, instance_server, auto_join) {
        (Some(target @ QuickPlay::Multiplayer { .. }), _, _) => Some(target.clone()),
        (Some(target), _, _) if supported => Some(target.clone()),
        (_, Some((host, port)), true) => Some(QuickPlay::Multiplayer {
            host: host.to_owned(),
            port,
        }),
        _ => None,
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WindowOptions {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fullscreen: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchOptions {
    /// Java executable (usually `InstalledVersion::java.path`).
    pub java_path: PathBuf,
    /// Game directory (`${game_directory}`, process working directory).
    pub game_dir: PathBuf,
    pub min_memory_mb: Option<u32>,
    pub max_memory_mb: Option<u32>,
    /// Extra JVM arguments as typed by the user (split with quote handling).
    pub jvm_args: String,
    /// Extra game arguments as typed by the user (split with quote handling).
    pub game_args: String,
    pub window: WindowOptions,
    pub quick_play: Option<QuickPlay>,
    pub demo: bool,
    pub launcher_name: String,
    pub launcher_version: String,
    pub env: BTreeMap<String, String>,
    /// Command the game is wrapped in, e.g. `gamemoderun`, `prime-run`, `mangohud --dlsym`.
    pub wrapper: Option<String>,
}

impl LaunchOptions {
    pub fn new(java_path: impl Into<PathBuf>, game_dir: impl Into<PathBuf>) -> Self {
        Self {
            java_path: java_path.into(),
            game_dir: game_dir.into(),
            min_memory_mb: None,
            max_memory_mb: None,
            jvm_args: String::new(),
            game_args: String::new(),
            window: WindowOptions::default(),
            quick_play: None,
            demo: false,
            launcher_name: LAUNCHER_NAME.to_owned(),
            launcher_version: LAUNCHER_VERSION.to_owned(),
            env: BTreeMap::new(),
            wrapper: None,
        }
    }
}

/// A ready-to-spawn command. `Debug` and [`LaunchCommand::display_redacted`] mask the access
/// token; never log `args` directly.
#[derive(Clone, PartialEq, Eq)]
pub struct LaunchCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: BTreeMap<String, String>,
    /// Indices in `args` of the JVM options (after the Java executable, before the main class).
    jvm_options: Range<usize>,
    /// Indices in `args` of the JVM arguments the player typed ([`LaunchOptions::jvm_args`]).
    player_jvm_options: Range<usize>,
    secrets: Vec<String>,
}

impl LaunchCommand {
    /// The JVM options (memory, system properties, classpath...).
    pub fn jvm_options(&self) -> &[String] {
        &self.args[self.jvm_options.clone()]
    }

    /// The JVM arguments the player typed, as split into the command (crash analysis tells a
    /// rejected argument of the player's from one of the launcher's).
    pub fn player_jvm_options(&self) -> &[String] {
        self.args
            .get(self.player_jvm_options.clone())
            .unwrap_or_default()
    }

    pub(crate) fn jvm_option_range(&self) -> Range<usize> {
        self.jvm_options.clone()
    }

    /// Length of the command line as Windows sees it (limit: 32 767 characters).
    pub fn command_line_len(&self) -> usize {
        self.program.as_os_str().len() + self.args.iter().map(|a| a.len() + 3).sum::<usize>()
    }

    /// Arguments with secrets replaced (the value after `--accessToken`/`--session` and any
    /// occurrence of the token).
    pub fn redacted_args(&self) -> Vec<String> {
        let mut out = Vec::with_capacity(self.args.len());
        let mut mask_next = false;
        for arg in &self.args {
            if mask_next {
                out.push(REDACTED.to_owned());
                mask_next = false;
                continue;
            }
            let mut shown = arg.clone();
            for secret in &self.secrets {
                shown = shown.replace(secret.as_str(), REDACTED);
            }
            mask_next = matches!(arg.as_str(), "--accessToken" | "--session");
            out.push(shown);
        }
        out
    }

    /// Shell-like one-line rendering for logs and "copy details", with secrets masked.
    pub fn display_redacted(&self) -> String {
        self.display_redacted_as(&self.program)
    }

    /// [`Self::display_redacted`] with the executable that was really started.
    pub(crate) fn display_redacted_as(&self, program: &Path) -> String {
        std::iter::once(program.display().to_string())
            .chain(self.redacted_args())
            .map(|arg| quote_for_display(&arg))
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[cfg(test)]
    pub(crate) fn for_tests(program: PathBuf, args: Vec<String>, cwd: PathBuf) -> Self {
        Self {
            program,
            args,
            cwd,
            env: BTreeMap::new(),
            jvm_options: 0..0,
            player_jvm_options: 0..0,
            secrets: Vec::new(),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_jvm_options(mut self, range: Range<usize>) -> Self {
        self.jvm_options = range;
        self
    }

    #[cfg(test)]
    pub(crate) fn with_player_jvm_options(mut self, range: Range<usize>) -> Self {
        self.player_jvm_options = range;
        self
    }
}

impl fmt::Debug for LaunchCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LaunchCommand")
            .field("program", &self.program)
            .field("args", &self.redacted_args())
            .field("cwd", &self.cwd)
            .field("env", &self.env.keys().collect::<Vec<_>>())
            .finish()
    }
}

fn quote_for_display(arg: &str) -> String {
    if arg.is_empty() || arg.contains(char::is_whitespace) || arg.contains('"') {
        format!("\"{}\"", arg.replace('"', "\\\""))
    } else {
        arg.to_owned()
    }
}

/// Split a user-typed argument string: whitespace separates, `"..."` and `'...'` group, `\"`
/// is a literal quote. Other backslashes are kept (Windows paths).
pub fn split_args(input: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_token = false;
    let mut quote: Option<char> = None;
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        match quote {
            Some(q) if c == q => quote = None,
            Some('"') if c == '\\' && chars.peek() == Some(&'"') => {
                current.push('"');
                chars.next();
            }
            Some(_) => current.push(c),
            None => match c {
                '"' | '\'' => {
                    quote = Some(c);
                    in_token = true;
                }
                '\\' if chars.peek() == Some(&'"') => {
                    current.push('"');
                    chars.next();
                    in_token = true;
                }
                c if c.is_whitespace() => {
                    if in_token {
                        args.push(std::mem::take(&mut current));
                        in_token = false;
                    }
                }
                c => {
                    current.push(c);
                    in_token = true;
                }
            },
        }
    }
    if in_token {
        args.push(current);
    }
    args
}

/// Replace every known `${name}`; unknown placeholders are left as they are.
pub fn substitute(template: &str, vars: &HashMap<&str, String>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find('}') {
            Some(end) => {
                let key = &after[..end];
                match vars.get(key) {
                    Some(value) => out.push_str(value),
                    None => {
                        out.push_str("${");
                        out.push_str(key);
                        out.push('}');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

fn allow_os(name: Option<&str>, arch: Option<&str>) -> Vec<Rule> {
    vec![Rule {
        action: RuleAction::Allow,
        os: Some(OsRule {
            name: name.map(str::to_owned),
            version: None,
            arch: arch.map(str::to_owned),
        }),
        features: None,
    }]
}

/// JVM arguments the official launcher uses for versions without `arguments.jvm`.
pub fn legacy_jvm_arguments() -> Vec<Argument> {
    vec![
        Argument::Conditional {
            rules: allow_os(Some("windows"), None),
            value: ArgumentValue::Single(
                "-XX:HeapDumpPath=MojangTricksIntelDriversForPerformance_javaw.exe_minecraft.exe.heapdump"
                    .to_owned(),
            ),
        },
        Argument::Conditional {
            rules: allow_os(None, Some("x86")),
            value: ArgumentValue::Single("-Xss1M".to_owned()),
        },
        Argument::plain("-Djava.library.path=${natives_directory}"),
        Argument::plain("-Dminecraft.launcher.brand=${launcher_name}"),
        Argument::plain("-Dminecraft.launcher.version=${launcher_version}"),
        Argument::plain("-cp"),
        Argument::plain("${classpath}"),
    ]
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// `host:port` for quick play (IPv6 literals in brackets).
fn server_address(host: &str, port: Option<u16>) -> String {
    let host = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_owned()
    };
    match port {
        Some(port) => format!("{host}:{port}"),
        None => host,
    }
}

/// Build the full command line for `resolved` as installed in `installed`.
pub fn build_launch_command(
    dirs: &DataDirs,
    resolved: &ResolvedVersion,
    installed: &InstalledVersion,
    identity: &LaunchIdentity,
    options: &LaunchOptions,
) -> Result<LaunchCommand> {
    if identity.name.trim().is_empty() {
        return Err(Error::Invalid("player name is empty".into()));
    }
    let platform = &installed.platform;
    let legacy = resolved.uses_legacy_arguments();
    let quick_play_supported = resolved.supports_quick_play();
    let window = &options.window;
    let resolution = match (window.width, window.height) {
        (Some(w), Some(h)) if !window.fullscreen && w > 0 && h > 0 => Some((w, h)),
        _ => None,
    };

    let mut features = Features {
        is_demo_user: options.demo,
        has_custom_resolution: resolution.is_some(),
        // The game then writes the world or server it joined (read for "Devam et").
        has_quick_plays_support: quick_play_supported,
        ..Features::default()
    };
    if quick_play_supported {
        match &options.quick_play {
            Some(QuickPlay::Multiplayer { .. }) => features.is_quick_play_multiplayer = true,
            Some(QuickPlay::Singleplayer { .. }) => features.is_quick_play_singleplayer = true,
            Some(QuickPlay::Realms { .. }) => features.is_quick_play_realms = true,
            None => {}
        }
    }

    let separator = platform.classpath_separator();
    let classpath = installed
        .classpath
        .iter()
        .map(|p| path_string(p))
        .collect::<Vec<_>>()
        .join(separator);
    let uuid = identity.uuid.simple().to_string();
    let session = if identity.user_type == UserType::Msa && !identity.access_token.is_empty() {
        format!("token:{}:{uuid}", identity.access_token)
    } else {
        "-".to_owned()
    };
    let (width, height) = resolution.unwrap_or((854, 480));

    let mut vars: HashMap<&str, String> = HashMap::new();
    vars.insert("auth_player_name", identity.name.clone());
    vars.insert("version_name", resolved.id.clone());
    vars.insert("game_directory", path_string(&options.game_dir));
    vars.insert("assets_root", path_string(&installed.assets.root));
    vars.insert(
        "game_assets",
        path_string(&installed.assets.game_assets(&options.game_dir)),
    );
    vars.insert("assets_index_name", installed.assets.index_id.clone());
    vars.insert("auth_uuid", uuid);
    vars.insert("auth_access_token", identity.access_token.clone());
    vars.insert("auth_session", session);
    vars.insert("clientid", identity.client_id.clone().unwrap_or_default());
    vars.insert("auth_xuid", identity.xuid.clone().unwrap_or_default());
    vars.insert("user_type", identity.user_type.as_str().to_owned());
    vars.insert("version_type", resolved.kind.clone());
    vars.insert("resolution_width", width.to_string());
    vars.insert("resolution_height", height.to_string());
    vars.insert("natives_directory", path_string(&installed.natives_dir));
    vars.insert("launcher_name", options.launcher_name.clone());
    vars.insert("launcher_version", options.launcher_version.clone());
    vars.insert("classpath", classpath);
    vars.insert("classpath_separator", separator.to_owned());
    vars.insert("library_directory", path_string(&dirs.libraries()));
    vars.insert("user_properties", "{}".to_owned());
    vars.insert(
        "quickPlayPath",
        path_string(&crate::launch::resume::quick_play_log_path(
            &options.game_dir,
        )),
    );
    if let Some(quick_play) = &options.quick_play {
        match quick_play {
            QuickPlay::Multiplayer { host, port } => {
                vars.insert("quickPlayMultiplayer", server_address(host, *port));
            }
            QuickPlay::Singleplayer { world } => {
                vars.insert("quickPlaySingleplayer", world.clone());
            }
            QuickPlay::Realms { realm_id } => {
                vars.insert("quickPlayRealms", realm_id.clone());
            }
        }
    }

    // JVM arguments: the version's own, or the launcher defaults for legacy versions (and for
    // odd profiles that forgot the classpath).
    let mut jvm = evaluate_arguments(&resolved.jvm_arguments, platform, &features);
    if legacy || !jvm.iter().any(|a| a.contains("${classpath}")) {
        let mut defaults = evaluate_arguments(&legacy_jvm_arguments(), platform, &features);
        defaults.append(&mut jvm);
        jvm = defaults;
    }
    let mut args: Vec<String> = jvm.iter().map(|a| substitute(a, &vars)).collect();

    // UTF-8 console output on every Java version (Turkish text in logs stays readable).
    for property in [
        "-Dstdout.encoding=UTF-8",
        "-Dstderr.encoding=UTF-8",
        "-Dsun.stdout.encoding=UTF-8",
        "-Dsun.stderr.encoding=UTF-8",
    ] {
        args.push(property.to_owned());
    }
    if let Some(min) = options.min_memory_mb.filter(|m| *m > 0) {
        args.push(format!("-Xms{min}M"));
    }
    if let Some(max) = options.max_memory_mb.filter(|m| *m > 0) {
        args.push(format!("-Xmx{max}M"));
    }
    let player_start = args.len();
    args.extend(split_args(&options.jvm_args));
    let player_options = player_start..args.len();
    if let (Some(config), Some(path)) = (&resolved.logging, &installed.logging_config)
        && let Some(argument) = config.argument.as_deref()
    {
        args.push(argument.replace("${path}", &path_string(path)));
    }

    let jvm_option_count = args.len();
    args.push(resolved.main_class.clone());

    // Game arguments.
    let mut game: Vec<String> = resolved
        .minecraft_arguments
        .as_deref()
        .map(|s| s.split_whitespace().map(str::to_owned).collect())
        .unwrap_or_default();
    game.extend(evaluate_arguments(
        &resolved.game_arguments,
        platform,
        &features,
    ));
    let mut game: Vec<String> = game.iter().map(|a| substitute(a, &vars)).collect();
    if legacy {
        if let Some((w, h)) = resolution
            && !game.iter().any(|a| a == "--width")
        {
            game.extend(["--width".to_owned(), w.to_string()]);
            game.extend(["--height".to_owned(), h.to_string()]);
        }
        if options.demo && !game.iter().any(|a| a == "--demo") {
            game.push("--demo".to_owned());
        }
    }
    if window.fullscreen && !game.iter().any(|a| a == "--fullscreen") {
        game.push("--fullscreen".to_owned());
    }
    if let Some(quick_play) = &options.quick_play
        && !quick_play_supported
    {
        match quick_play {
            QuickPlay::Multiplayer { host, port } => {
                game.extend([
                    "--server".to_owned(),
                    host.clone(),
                    "--port".to_owned(),
                    port.unwrap_or(25565).to_string(),
                ]);
            }
            other => {
                tracing::warn!(version = %resolved.id, ?other, "quick play is not supported by this version");
            }
        }
    }
    game.extend(split_args(&options.game_args));
    args.extend(game);

    let java = path_string(&options.java_path);
    let wrapper = options
        .wrapper
        .as_deref()
        .map(split_args)
        .filter(|w| !w.is_empty());
    let (program, args, offset) = match wrapper {
        Some(mut wrapped) => {
            let program = PathBuf::from(wrapped.remove(0));
            wrapped.push(java);
            let start = wrapped.len();
            wrapped.extend(args);
            (program, wrapped, start)
        }
        None => (options.java_path.clone(), args, 0),
    };
    let jvm_options = offset..offset + jvm_option_count;
    let player_jvm_options = offset + player_options.start..offset + player_options.end;

    let secrets = [identity.access_token.as_str()]
        .into_iter()
        .filter(|s| s.len() >= 6)
        .map(str::to_owned)
        .collect();
    Ok(LaunchCommand {
        program,
        args,
        cwd: options.game_dir.clone(),
        env: options.env.clone(),
        jvm_options,
        player_jvm_options,
        secrets,
    })
}

/// Java argument file content (`java @file`, Java 9+) for `options`: one quoted argument per
/// line, with `\` and `"` escaped as the launcher expects inside quotes.
pub fn java_argfile(options: &[String]) -> String {
    let mut out = String::new();
    for arg in options {
        out.push('"');
        for c in arg.chars() {
            if c == '\\' || c == '"' {
                out.push('\\');
            }
            out.push(c);
        }
        out.push_str("\"\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install::{AssetLayout, AssetsInfo};
    use crate::java::{JavaInfo, JavaInstallation, JavaSource};
    use crate::platform::{Arch, Os, Platform};
    use crate::version::{VersionJson, resolve_with};

    fn resolve(json: &str) -> ResolvedVersion {
        let v: VersionJson = serde_json::from_str(json).unwrap();
        let id = v.id.clone();
        resolve_with(&id, |_| Ok(v.clone())).unwrap()
    }

    #[test]
    fn quick_play_choice() {
        let invite = QuickPlay::Multiplayer {
            host: "mc.example.net".into(),
            port: Some(25570),
        };
        let world = QuickPlay::Singleplayer {
            world: "Yeni Dünya".into(),
        };
        let projects = Some(("play.projects.gg", None));
        let pick = pick_quick_play;

        // A requested server wins over the instance's own, on old versions too (`--server`).
        assert_eq!(
            pick(Some(&invite), projects, true, true),
            Some(invite.clone())
        );
        assert_eq!(
            pick(Some(&invite), projects, true, false),
            Some(invite.clone())
        );
        assert_eq!(
            pick(Some(&invite), None, false, false),
            Some(invite.clone())
        );
        // A world needs quick play; without it the instance's server (or the title screen).
        assert_eq!(
            pick(Some(&world), projects, false, true),
            Some(world.clone())
        );
        assert_eq!(
            pick(Some(&world), projects, true, false),
            Some(QuickPlay::Multiplayer {
                host: "play.projects.gg".into(),
                port: None
            })
        );
        assert_eq!(pick(Some(&world), projects, false, false), None);
        // Nothing requested: auto-join decides.
        assert_eq!(
            pick(None, Some(("play.projects.gg", Some(25566))), true, true),
            Some(QuickPlay::Multiplayer {
                host: "play.projects.gg".into(),
                port: Some(25566)
            })
        );
        assert_eq!(pick(None, projects, false, true), None);
        assert_eq!(pick(None, None, true, true), None);
    }

    fn installed(platform: Platform, layout: AssetLayout, index: &str) -> InstalledVersion {
        let root = PathBuf::from("/data");
        InstalledVersion {
            java: JavaInstallation {
                path: PathBuf::from("/java/bin/java"),
                info: JavaInfo {
                    version: "21.0.7".into(),
                    major: 21,
                    arch: platform.arch,
                    vendor: None,
                },
                source: JavaSource::System,
            },
            classpath: vec![root.join("libraries/a.jar"), root.join("versions/v/v.jar")],
            client_jar: root.join("versions/v/v.jar"),
            natives_dir: root.join("natives/v-123"),
            assets: AssetsInfo {
                index_id: index.into(),
                layout,
                root: root.join("assets"),
                virtual_dir: Some(root.join("assets/virtual").join(index)),
            },
            logging_config: Some(root.join("assets/log_configs/client-1.12.xml")),
            platform,
        }
    }

    const MODERN: &str = r#"{
        "id": "1.21.1", "type": "release", "mainClass": "net.minecraft.client.main.Main",
        "assetIndex": {"id": "17"},
        "logging": {"client": {"argument": "-Dlog4j.configurationFile=${path}", "file": {"id": "client-1.12.xml"}, "type": "log4j2-xml"}},
        "arguments": {
            "game": ["--username", "${auth_player_name}", "--version", "${version_name}", "--gameDir", "${game_directory}",
                "--assetsDir", "${assets_root}", "--assetIndex", "${assets_index_name}", "--uuid", "${auth_uuid}",
                "--accessToken", "${auth_access_token}", "--clientId", "${clientid}", "--xuid", "${auth_xuid}",
                "--userType", "${user_type}", "--versionType", "${version_type}",
                {"rules": [{"action": "allow", "features": {"is_demo_user": true}}], "value": "--demo"},
                {"rules": [{"action": "allow", "features": {"has_custom_resolution": true}}], "value": ["--width", "${resolution_width}", "--height", "${resolution_height}"]},
                {"rules": [{"action": "allow", "features": {"has_quick_plays_support": true}}], "value": ["--quickPlayPath", "${quickPlayPath}"]},
                {"rules": [{"action": "allow", "features": {"is_quick_play_multiplayer": true}}], "value": ["--quickPlayMultiplayer", "${quickPlayMultiplayer}"]}],
            "jvm": [{"rules": [{"action": "allow", "os": {"name": "osx"}}], "value": ["-XstartOnFirstThread"]},
                {"rules": [{"action": "allow", "os": {"name": "windows"}}], "value": "-XX:HeapDumpPath=MojangTricksIntelDriversForPerformance_javaw.exe_minecraft.exe.heapdump"},
                {"rules": [{"action": "allow", "os": {"arch": "x86"}}], "value": "-Xss1M"},
                "-Djava.library.path=${natives_directory}", "-Dminecraft.launcher.brand=${launcher_name}",
                "-Dminecraft.launcher.version=${launcher_version}", "-cp", "${classpath}"]
        }
    }"#;

    #[test]
    fn modern_arguments() {
        let r = resolve(MODERN);
        let win = Platform::new(Os::Windows, Arch::X86_64, "10.0.26100");
        let inst = installed(win, AssetLayout::Standard, "17");
        let mut identity = LaunchIdentity::offline("Notch");
        identity.access_token = "secret-token-value".into();
        identity.user_type = UserType::Msa;
        let mut options = LaunchOptions::new("/java/bin/javaw.exe", "/games/one");
        options.min_memory_mb = Some(1024);
        options.max_memory_mb = Some(4096);
        options.jvm_args = r#"-XX:+UseG1GC "-Dmy.prop=a b""#.into();
        options.window = WindowOptions {
            width: Some(1280),
            height: Some(720),
            fullscreen: false,
        };
        options.quick_play = Some(QuickPlay::Multiplayer {
            host: "play.projects.gg".into(),
            port: Some(25565),
        });
        let cmd =
            build_launch_command(&DataDirs::new("/data"), &r, &inst, &identity, &options).unwrap();
        let a = &cmd.args;
        let pos = |s: &str| {
            a.iter()
                .position(|x| x == s)
                .unwrap_or_else(|| panic!("{s} missing in {a:?}"))
        };
        assert_eq!(cmd.program, PathBuf::from("/java/bin/javaw.exe"));
        assert_eq!(cmd.cwd, PathBuf::from("/games/one"));
        assert!(a.iter().any(|x| x.starts_with("-XX:HeapDumpPath=")));
        assert!(!a.contains(&"-XstartOnFirstThread".to_owned()));
        assert!(!a.contains(&"-Xss1M".to_owned()));
        assert!(a.contains(&format!(
            "-Djava.library.path={}",
            path_string(&inst.natives_dir)
        )));
        assert!(a.contains(&"-Dminecraft.launcher.brand=Projects Launcher".to_owned()));
        let cp = &a[pos("-cp") + 1];
        assert_eq!(
            cp,
            &format!(
                "{};{}",
                path_string(&inst.classpath[0]),
                path_string(&inst.classpath[1])
            )
        );
        assert!(pos("-Xmx4096M") > pos("-cp"));
        assert!(pos("-Dmy.prop=a b") > pos("-Xmx4096M"));
        assert!(
            a.iter()
                .any(|x| x.starts_with("-Dlog4j.configurationFile="))
        );
        let main = pos("net.minecraft.client.main.Main");
        assert!(main > pos("-Dmy.prop=a b"));
        assert_eq!(a[pos("--username") + 1], "Notch");
        assert_eq!(a[pos("--uuid") + 1], "b50ad385829d3141a2167e7d7539ba7f");
        assert_eq!(a[pos("--userType") + 1], "msa");
        assert_eq!(a[pos("--versionType") + 1], "release");
        assert_eq!(a[pos("--assetIndex") + 1], "17");
        assert_eq!(a[pos("--width") + 1], "1280");
        assert_eq!(
            a[pos("--quickPlayMultiplayer") + 1],
            "play.projects.gg:25565"
        );
        assert!(!a.contains(&"--server".to_owned()));
        // The game logs what it joined there ("Devam et" reads it).
        assert_eq!(
            PathBuf::from(&a[pos("--quickPlayPath") + 1]),
            PathBuf::from("/games/one")
                .join("quickPlay")
                .join("log.json")
        );
        assert!(!a.contains(&"--demo".to_owned()));
        assert!(a.iter().all(|x| !x.contains("${")), "unsubstituted: {a:?}");

        // Secrets never show up in the printable forms.
        let shown = cmd.display_redacted();
        assert!(!shown.contains("secret-token-value"));
        assert!(shown.contains("--accessToken ********"));
        assert!(shown.contains("\"-Dminecraft.launcher.brand=Projects Launcher\""));
        assert!(!format!("{cmd:?}").contains("secret-token-value"));

        // JVM options end right before the main class and never contain the token.
        let jvm = cmd.jvm_options();
        assert_eq!(cmd.args[jvm.len()], "net.minecraft.client.main.Main");
        assert!(jvm.iter().any(|x| x == "-cp"));
        assert!(jvm.iter().all(|x| !x.contains("secret-token-value")));
        assert!(cmd.command_line_len() > cp.len());
        // The player's own arguments are known (crash analysis), the launcher's are not theirs.
        assert_eq!(cmd.player_jvm_options(), ["-XX:+UseG1GC", "-Dmy.prop=a b"]);
        assert_eq!(
            cmd.display_redacted_as(Path::new("/java/bin/java.exe"))
                .split(' ')
                .next(),
            Some("/java/bin/java.exe")
        );
    }

    #[test]
    fn argfile_quoting() {
        let file = java_argfile(&[
            "-cp".to_owned(),
            r"C:\Program Files\a.jar;D:\b.jar".to_owned(),
            "-Dx=\"q\"".to_owned(),
        ]);
        assert_eq!(
            file,
            "\"-cp\"\n\"C:\\\\Program Files\\\\a.jar;D:\\\\b.jar\"\n\"-Dx=\\\"q\\\"\"\n"
        );
    }

    #[test]
    fn modern_on_macos_arm64_and_wrapper() {
        let r = resolve(MODERN);
        let mac = Platform::new(Os::MacOs, Arch::Aarch64, "14.5");
        let inst = installed(mac, AssetLayout::Standard, "17");
        let mut options = LaunchOptions::new("/java/bin/java", "/games/one");
        options.wrapper = Some("mangohud --dlsym".into());
        options.window.fullscreen = true;
        options.demo = true;
        options.jvm_args = "-Xss2M".into();
        let cmd = build_launch_command(
            &DataDirs::new("/data"),
            &r,
            &inst,
            &LaunchIdentity::offline("Alex"),
            &options,
        )
        .unwrap();
        assert_eq!(cmd.program, PathBuf::from("mangohud"));
        assert_eq!(cmd.args[0], "--dlsym");
        assert_eq!(cmd.args[1], "/java/bin/java");
        assert_eq!(cmd.jvm_options()[0], "-XstartOnFirstThread");
        assert_eq!(cmd.player_jvm_options(), ["-Xss2M"]);
        assert!(cmd.args.contains(&"-XstartOnFirstThread".to_owned()));
        assert!(!cmd.args.iter().any(|x| x.starts_with("-XX:HeapDumpPath")));
        let cp = cmd.args.iter().position(|x| x == "-cp").unwrap();
        assert!(cmd.args[cp + 1].contains(':'));
        assert!(cmd.args.contains(&"--fullscreen".to_owned()));
        assert!(cmd.args.contains(&"--demo".to_owned()));
        assert!(!cmd.args.contains(&"--width".to_owned()));
        let user_type = cmd.args.iter().position(|x| x == "--userType").unwrap();
        assert_eq!(cmd.args[user_type + 1], "legacy");
    }

    #[test]
    fn legacy_arguments_1_8_9() {
        let r = resolve(
            r#"{"id": "1.8.9", "type": "release", "mainClass": "net.minecraft.client.main.Main",
                "assetIndex": {"id": "1.8"}, "assets": "1.8",
                "minecraftArguments": "--username ${auth_player_name} --version ${version_name} --gameDir ${game_directory} --assetsDir ${assets_root} --assetIndex ${assets_index_name} --uuid ${auth_uuid} --accessToken ${auth_access_token} --userProperties ${user_properties} --userType ${user_type}"}"#,
        );
        let win = Platform::new(Os::Windows, Arch::X86_64, "10.0.26100");
        let inst = installed(win, AssetLayout::Standard, "1.8");
        let mut options = LaunchOptions::new("/java/bin/javaw.exe", "/games/old");
        options.window = WindowOptions {
            width: Some(1024),
            height: Some(768),
            fullscreen: false,
        };
        options.quick_play = Some(QuickPlay::Multiplayer {
            host: "play.projects.gg".into(),
            port: None,
        });
        let cmd = build_launch_command(
            &DataDirs::new("/data"),
            &r,
            &inst,
            &LaunchIdentity::offline("Steve"),
            &options,
        )
        .unwrap();
        let a = &cmd.args;
        let pos = |s: &str| a.iter().position(|x| x == s).unwrap();
        assert!(pos("-cp") < pos("net.minecraft.client.main.Main"));
        assert!(a.iter().any(|x| x.starts_with("-Djava.library.path=")));
        assert!(a.iter().any(|x| x.starts_with("-XX:HeapDumpPath=")));
        assert_eq!(a[pos("--userProperties") + 1], "{}");
        assert_eq!(a[pos("--accessToken") + 1], "0");
        assert_eq!(a[pos("--width") + 1], "1024");
        assert_eq!(a[pos("--height") + 1], "768");
        assert_eq!(a[pos("--server") + 1], "play.projects.gg");
        assert_eq!(a[pos("--port") + 1], "25565");
        assert!(a.iter().all(|x| !x.contains("${")));
    }

    #[test]
    fn legacy_alpha_uses_game_assets_and_session() {
        let r = resolve(
            r#"{"id": "a1.0.4", "type": "old_alpha", "mainClass": "net.minecraft.launchwrapper.Launch",
                "assetIndex": {"id": "pre-1.6"}, "assets": "pre-1.6",
                "minecraftArguments": "${auth_player_name} ${auth_session} --gameDir ${game_directory} --assetsDir ${game_assets} --tweakClass net.minecraft.launchwrapper.AlphaVanillaTweaker"}"#,
        );
        let linux = Platform::new(Os::Linux, Arch::X86, "6.1");
        let inst = installed(linux, AssetLayout::Resources, "pre-1.6");
        let cmd = build_launch_command(
            &DataDirs::new("/data"),
            &r,
            &inst,
            &LaunchIdentity::offline("Steve"),
            &LaunchOptions::new("/java/bin/java", "/games/alpha"),
        )
        .unwrap();
        let a = &cmd.args;
        let main = a
            .iter()
            .position(|x| x == "net.minecraft.launchwrapper.Launch")
            .unwrap();
        assert_eq!(a[main + 1], "Steve");
        assert_eq!(a[main + 2], "-");
        let assets = a.iter().position(|x| x == "--assetsDir").unwrap();
        assert_eq!(
            a[assets + 1],
            path_string(&Path::new("/games/alpha").join("resources"))
        );
        assert!(a.contains(&"-Xss1M".to_owned()), "32-bit JVM gets -Xss1M");
        let cp = a.iter().position(|x| x == "-cp").unwrap();
        assert!(a[cp + 1].contains(':'));
    }

    #[test]
    fn substitution_and_splitting() {
        let vars = HashMap::from([("a", "1".to_owned()), ("b", "two".to_owned())]);
        assert_eq!(substitute("x${a}y${b}z", &vars), "x1ytwoz");
        assert_eq!(substitute("${unknown}-${a}", &vars), "${unknown}-1");
        assert_eq!(substitute("open ${a", &vars), "open ${a");
        assert_eq!(substitute("plain", &vars), "plain");

        assert_eq!(
            split_args(r#"-Xmx2G  "-Dfoo=bar baz" -Dx='a b' C:\Program\x \"q\" "" 'it"s'"#),
            [
                "-Xmx2G",
                "-Dfoo=bar baz",
                "-Dx=a b",
                r"C:\Program\x",
                "\"q\"",
                "",
                "it\"s"
            ]
        );
        assert!(split_args("   ").is_empty());
        assert_eq!(server_address("::1", Some(25565)), "[::1]:25565");
        assert_eq!(server_address("mc.example.com", None), "mc.example.com");
    }
}

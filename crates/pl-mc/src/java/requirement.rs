//! Which Java a game needs: the version JSON's `javaVersion` (the Mojang runtime component and
//! its major version), the mods' minimum, and a ceiling for games that start through
//! LaunchWrapper (Java 8 only). When the version JSON is not at hand, a table by version id
//! stands in.

use crate::loaders::LoaderKind;
use crate::mods::version::{MinecraftKind, MinecraftVersion};
use crate::paths::DataDirs;
use crate::version::{VersionJson, component_major};

/// LaunchWrapper's main class (Minecraft 1.5.2 and older, Forge before 1.13, OptiFine
/// standalone). It casts the system class loader to `URLClassLoader`, which fails on Java 9+.
pub const LAUNCHWRAPPER_MAIN: &str = "net.minecraft.launchwrapper.Launch";

/// Mojang's runtime components, oldest first: (component, Java major).
const COMPONENTS: &[(&str, u32)] = &[
    ("jre-legacy", 8),
    ("java-runtime-alpha", 16),
    ("java-runtime-gamma", 17),
    ("java-runtime-delta", 21),
    ("java-runtime-epsilon", 25),
];

/// The Java a launch needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaNeed {
    /// Lowest Java major the game and its mods accept.
    pub min: u32,
    /// Highest Java major known to work (`Some(8)` for LaunchWrapper games); `None` when no
    /// ceiling is known for certain.
    pub max: Option<u32>,
    /// Mojang runtime component of the game version ("Otomatik" installs it).
    pub component: String,
    /// The Java major the game version itself asks for.
    pub game: u32,
    /// The mods' minimum, when it is above the game's.
    pub mods: Option<u32>,
}

impl JavaNeed {
    /// The game's own need: `javaVersion` of its version JSON (component and major) and whether
    /// it starts through LaunchWrapper.
    pub fn for_game(component: &str, major: u32, launchwrapper: bool) -> Self {
        Self {
            min: major,
            max: (launchwrapper && major <= 8).then_some(8),
            component: component.to_owned(),
            game: major,
            mods: None,
        }
    }

    /// Adds what the mods ask for (the highest `java` minimum among them).
    pub fn with_mods(mut self, mods_min: Option<u32>) -> Self {
        if let Some(mods) = mods_min.filter(|mods| *mods > self.min) {
            self.min = mods;
            self.mods = Some(mods);
        }
        self
    }

    /// Whether a Java of this major should run the game.
    pub fn accepts(&self, major: u32) -> bool {
        major >= self.min && self.max.is_none_or(|max| major <= max)
    }

    /// The Java major of the runtime "Otomatik" gives the game.
    pub fn automatic_major(&self) -> u32 {
        component_major(&self.component).unwrap_or(self.game)
    }
}

/// Whether the game starts through LaunchWrapper: the vanilla main class says so (1.5.2 and
/// older), or Forge before 1.13.
pub fn uses_launchwrapper(vanilla_main_class: &str, loader: LoaderKind, minecraft: &str) -> bool {
    vanilla_main_class.trim() == LAUNCHWRAPPER_MAIN
        || (loader == LoaderKind::Forge
            && MinecraftVersion::parse(minecraft).at_least(1, 13, 0) == Some(false))
}

/// The smallest Mojang runtime with at least Java `major`: (component, its major).
pub fn component_for_major(major: u32) -> Option<(&'static str, u32)> {
    COMPONENTS
        .iter()
        .find(|(_, provided)| *provided >= major)
        .copied()
}

/// The Mojang runtime (component, Java major) a Minecraft version uses, from its id alone. For
/// when the version JSON is not on disk; `None` for ids it cannot place.
pub fn fallback_for_minecraft(id: &str) -> Option<(&'static str, u32)> {
    let version = MinecraftVersion::parse(id);
    let release = |major, minor, patch| version.at_least(major, minor, patch);
    let pick = |component: &'static str| component_major(component).map(|m| (component, m));
    match version.kind {
        MinecraftKind::Release
        | MinecraftKind::PreRelease
        | MinecraftKind::ReleaseCandidate
        | MinecraftKind::Snapshot
            if version.release.is_some() =>
        {
            if release(26, 0, 0)? {
                pick("java-runtime-epsilon")
            } else if release(1, 20, 5)? {
                pick("java-runtime-delta")
            } else if release(1, 19, 0)? {
                pick("java-runtime-gamma")
            } else if release(1, 18, 0)? {
                pick("java-runtime-beta")
            } else if release(1, 17, 0)? {
                pick("java-runtime-alpha")
            } else {
                pick("jre-legacy")
            }
        }
        MinecraftKind::Snapshot => {
            // Week snapshots (`24w14a`): the first that needed each Java.
            let year: u32 = id.get(0..2)?.parse().ok()?;
            let week: u32 = id.get(3..5)?.parse().ok()?;
            let at = (year, week);
            if at >= (24, 14) {
                pick("java-runtime-delta")
            } else if at >= (21, 37) {
                pick("java-runtime-beta")
            } else if at >= (21, 19) {
                pick("java-runtime-alpha")
            } else {
                pick("jre-legacy")
            }
        }
        _ => {
            // Alpha, beta, classic, pre-classic and Infdev ids.
            let old = ["a1.", "b1.", "c0.", "rd-", "inf-"];
            old.iter()
                .any(|prefix| id.starts_with(prefix))
                .then(|| pick("jre-legacy"))
                .flatten()
        }
    }
}

/// The game's own need from its vanilla version JSON: `javaVersion` (Java 8 and `jre-legacy`
/// for JSONs that predate it) and whether it starts through LaunchWrapper.
pub fn need_from_version_json(json: &VersionJson, loader: LoaderKind, minecraft: &str) -> JavaNeed {
    let java = json.java_version.as_ref();
    let component = java
        .and_then(|j| j.component.as_deref())
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .unwrap_or("jre-legacy");
    let major = java
        .and_then(|j| j.major_version)
        .or_else(|| component_major(component))
        .unwrap_or(8);
    let main_class = json.main_class.as_deref().unwrap_or_default();
    JavaNeed::for_game(
        component,
        major,
        uses_launchwrapper(main_class, loader, minecraft),
    )
}

/// The game's own need from its installed vanilla version JSON, else from the table (then
/// LaunchWrapper is judged by the id: Minecraft before 1.6, Forge before 1.13). `None` for ids
/// the table cannot place. Blocking (reads one file).
pub fn game_need(dirs: &DataDirs, minecraft: &str, loader: LoaderKind) -> Option<JavaNeed> {
    if let Some(json) = installed_vanilla_json(dirs, minecraft) {
        return Some(need_from_version_json(&json, loader, minecraft));
    }
    let (component, major) = fallback_for_minecraft(minecraft)?;
    let version = MinecraftVersion::parse(minecraft);
    let before_1_6 = match version.kind {
        MinecraftKind::Other => true,
        _ => version.at_least(1, 6, 0) == Some(false),
    };
    let launchwrapper = before_1_6 || uses_launchwrapper("", loader, minecraft);
    Some(JavaNeed::for_game(component, major, launchwrapper))
}

/// The runtime component "Otomatik" uses for a game version. Blocking.
pub fn game_component(dirs: &DataDirs, minecraft: &str) -> Option<String> {
    game_need(dirs, minecraft, LoaderKind::Vanilla).map(|need| need.component)
}

fn installed_vanilla_json(dirs: &DataDirs, minecraft: &str) -> Option<VersionJson> {
    let path = dirs.version_json(minecraft).ok()?;
    let bytes = std::fs::read(&path).ok()?;
    let json = VersionJson::from_slice(&bytes, &path.display().to_string()).ok()?;
    // A loader profile under a vanilla id would say nothing about Java.
    json.inherits_from
        .as_deref()
        .is_none_or(|parent| parent.trim().is_empty())
        .then_some(json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needs_from_the_game_and_its_mods() {
        let modern = JavaNeed::for_game("java-runtime-delta", 21, false);
        assert_eq!((modern.min, modern.max), (21, None));
        assert!(modern.accepts(21) && modern.accepts(25) && !modern.accepts(17));
        assert_eq!(modern.automatic_major(), 21);

        // LaunchWrapper games: Java 8 only.
        let legacy = JavaNeed::for_game("jre-legacy", 8, true);
        assert_eq!((legacy.min, legacy.max), (8, Some(8)));
        assert!(legacy.accepts(8) && !legacy.accepts(17));
        // Java 8 games without LaunchWrapper have no known ceiling.
        assert_eq!(JavaNeed::for_game("jre-legacy", 8, false).max, None);

        // Mods raise the minimum; a lower mod minimum changes nothing.
        let with_mods = JavaNeed::for_game("java-runtime-gamma", 17, false).with_mods(Some(21));
        assert_eq!(
            (with_mods.min, with_mods.mods, with_mods.game),
            (21, Some(21), 17)
        );
        assert_eq!(with_mods.automatic_major(), 17);
        let lower = JavaNeed::for_game("java-runtime-delta", 21, false).with_mods(Some(17));
        assert_eq!((lower.min, lower.mods), (21, None));

        // An unknown future component falls back to the JSON's major.
        assert_eq!(
            JavaNeed::for_game("java-runtime-zeta", 29, false).automatic_major(),
            29
        );
    }

    #[test]
    fn launchwrapper_games() {
        assert!(uses_launchwrapper(
            LAUNCHWRAPPER_MAIN,
            LoaderKind::Vanilla,
            "1.5.2"
        ));
        assert!(uses_launchwrapper(
            "net.minecraft.client.main.Main",
            LoaderKind::Forge,
            "1.12.2"
        ));
        assert!(uses_launchwrapper(
            "net.minecraft.client.main.Main",
            LoaderKind::Forge,
            "1.7.10"
        ));
        assert!(!uses_launchwrapper(
            "net.minecraft.client.main.Main",
            LoaderKind::Forge,
            "1.16.5"
        ));
        assert!(!uses_launchwrapper(
            "net.minecraft.client.main.Main",
            LoaderKind::Vanilla,
            "1.12.2"
        ));
        assert!(!uses_launchwrapper(
            "net.minecraft.client.main.Main",
            LoaderKind::Fabric,
            "1.8.9"
        ));
    }

    #[test]
    fn smallest_runtime_for_a_major() {
        assert_eq!(component_for_major(8), Some(("jre-legacy", 8)));
        assert_eq!(component_for_major(11), Some(("java-runtime-alpha", 16)));
        assert_eq!(component_for_major(17), Some(("java-runtime-gamma", 17)));
        assert_eq!(component_for_major(18), Some(("java-runtime-delta", 21)));
        assert_eq!(component_for_major(21), Some(("java-runtime-delta", 21)));
        assert_eq!(component_for_major(22), Some(("java-runtime-epsilon", 25)));
        assert_eq!(component_for_major(26), None);
    }

    #[test]
    fn runtimes_by_version_id() {
        let major = |id: &str| fallback_for_minecraft(id).map(|(_, major)| major);
        let component = |id: &str| fallback_for_minecraft(id).map(|(component, _)| component);
        assert_eq!(major("1.8.9"), Some(8));
        assert_eq!(major("1.12.2"), Some(8));
        assert_eq!(major("1.16.5"), Some(8));
        assert_eq!(major("1.17"), Some(16));
        assert_eq!(major("1.17.1"), Some(16));
        assert_eq!(component("1.18.2"), Some("java-runtime-beta"));
        assert_eq!(major("1.18.2"), Some(17));
        assert_eq!(component("1.20.1"), Some("java-runtime-gamma"));
        assert_eq!(major("1.20.4"), Some(17));
        assert_eq!(major("1.20.5"), Some(21));
        assert_eq!(major("1.21.1"), Some(21));
        assert_eq!(major("1.21.11"), Some(21));
        assert_eq!(major("26.1"), Some(25));
        assert_eq!(major("26.1.2"), Some(25));
        // Pre-releases, release candidates and year-based snapshots follow their release.
        assert_eq!(major("1.20.5-pre1"), Some(21));
        assert_eq!(major("1.18-rc3"), Some(17));
        assert_eq!(major("26.1-snapshot-2"), Some(25));
        // Week snapshots.
        assert_eq!(major("21w18a"), Some(8));
        assert_eq!(major("21w19a"), Some(16));
        assert_eq!(major("21w37a"), Some(17));
        assert_eq!(major("24w13a"), Some(17));
        assert_eq!(major("24w14a"), Some(21));
        assert_eq!(major("25w45a"), Some(21));
        // Old versions; unknown ids.
        assert_eq!(major("b1.7.3"), Some(8));
        assert_eq!(major("rd-132211"), Some(8));
        assert_eq!(major("3D Shareware v1.34"), None);
        assert_eq!(major("my-custom-build"), None);
    }

    #[test]
    fn components_from_installed_version_files() {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = DataDirs::new(tmp.path());
        let write = |id: &str, json: &str| {
            let path = dirs.version_json(id).unwrap();
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, json).unwrap();
        };
        write(
            "1.21.1",
            r#"{"id": "1.21.1", "javaVersion": {"component": "java-runtime-delta", "majorVersion": 21}}"#,
        );
        write("1.12.2", r#"{"id": "1.12.2", "mainClass": "x"}"#);
        write(
            "fabric-loader-0.16-1.21.1",
            r#"{"id": "fabric-loader-0.16-1.21.1", "inheritsFrom": "1.21.1"}"#,
        );
        assert_eq!(
            game_component(&dirs, "1.21.1").as_deref(),
            Some("java-runtime-delta")
        );
        assert_eq!(
            game_component(&dirs, "1.12.2").as_deref(),
            Some("jre-legacy")
        );
        // Not installed: the table.
        assert_eq!(
            game_component(&dirs, "1.20.1").as_deref(),
            Some("java-runtime-gamma")
        );
        assert_eq!(game_component(&dirs, "my-custom-build"), None);
    }

    #[test]
    fn game_needs_from_json_or_table() {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = DataDirs::new(tmp.path());
        let write = |id: &str, json: &str| {
            let path = dirs.version_json(id).unwrap();
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, json).unwrap();
        };
        write(
            "1.5.2",
            r#"{"id": "1.5.2", "mainClass": "net.minecraft.launchwrapper.Launch"}"#,
        );
        write(
            "1.12.2",
            r#"{"id": "1.12.2", "mainClass": "net.minecraft.client.main.Main", "javaVersion": {"component": "jre-legacy", "majorVersion": 8}}"#,
        );
        write(
            "26.1.2",
            r#"{"id": "26.1.2", "mainClass": "net.minecraft.client.main.Main", "javaVersion": {"component": "java-runtime-epsilon", "majorVersion": 25}}"#,
        );
        let need = |id: &str, loader| game_need(&dirs, id, loader).unwrap();
        assert_eq!(need("1.5.2", LoaderKind::Vanilla).max, Some(8));
        assert_eq!(need("1.12.2", LoaderKind::Vanilla).max, None);
        assert_eq!(need("1.12.2", LoaderKind::Forge).max, Some(8));
        let modern = need("26.1.2", LoaderKind::Fabric);
        assert_eq!(
            (modern.min, modern.component.as_str()),
            (25, "java-runtime-epsilon")
        );
        // Not installed: the table, LaunchWrapper by id.
        assert_eq!(need("1.4.7", LoaderKind::Vanilla).max, Some(8));
        assert_eq!(need("b1.7.3", LoaderKind::Vanilla).max, Some(8));
        assert_eq!(need("1.7.10", LoaderKind::Vanilla).max, None);
        assert_eq!(need("1.7.10", LoaderKind::Forge).max, Some(8));
        assert_eq!(need("1.21.1", LoaderKind::NeoForge).min, 21);
        assert!(game_need(&dirs, "my-custom-build", LoaderKind::Vanilla).is_none());
    }
}

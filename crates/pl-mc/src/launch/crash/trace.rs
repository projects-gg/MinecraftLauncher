//! A crash's coarse trace for usage data (TELEM-FIX): what kind of code failed, and where, built
//! only from closed lists. It never carries a path, a file name, a player or world name, a server
//! or a stack: an exception class outside the platform's packages, a module or a mod outside the
//! lists below becomes `other` (`mod` for a mod).
//!
//! - `native/<family>`: the JVM died in native code (its `hs_err_pid*.log`): the family of the
//!   module of its problematic frame (`gpu-amd`, `jvm`, `overlay-rtss`, `java-code`).
//! - `java/<Exception>/<culprit>`: the game's own crash report: the root exception's simple name
//!   (only the classes of Java, the game, the mod loaders and their libraries), and the known mod
//!   its trace names (`sodium`), `mod` for another mod, `none` for none.
//! - `exit/<name>`: neither: Windows' name for the exit code (`hung`), a few common numbers, else
//!   `other`.
//!
//! [`CrashTrace::phase`] is the crash report's description when the game itself writes it
//! (`Rendering overlay`).

use std::sync::LazyLock;

use regex::Regex;

use super::{CrashKind, CrashSuspect, PLATFORM_CLASSES, SuspectReason};

/// The coarse trace of one crash (see the module).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CrashTrace {
    /// `native/gpu-amd`, `java/NullPointerException/sodium`, `exit/hung`.
    pub sig: String,
    /// The crash report's description, when it is one of [`PHASES`].
    pub phase: Option<&'static str>,
}

/// Windows ends a program that stopped responding with this code (`0xCFFFFFFF`): the player
/// closed the frozen window ("Close the program") or the system did.
pub const NOT_RESPONDING_EXIT: i32 = -805_306_369;

/// The first line of the game's own crash report.
pub(super) const REPORT_HEADER: &str = "---- Minecraft Crash Report ----";

/// `# Problematic frame:` and the frame below it: its type (`C` native, `V` the JVM, `J`/`j`
/// Java code) and, for a native one, its module (`[atio6axx.dll+0x1b8a0f]`).
static PROBLEMATIC_FRAME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^#\s*Problematic frame:[ \t]*\r?\n#[ \t]+([A-Za-z])[ \t]+(?:\[([^\]\s+]+)\+0x[0-9a-fA-F]+\])?")
        .expect("valid regex")
});
/// A thrown class at the start of a line of the crash report's stack (`java.lang.Foo: text`,
/// `Caused by: java.lang.Foo`).
static THROWN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^(Caused by: )?((?:[A-Za-z_$][\w$]*\.)+[A-Za-z_$][\w$]*)(?::|[ \t]*\r?$)")
        .expect("valid regex")
});
static DESCRIPTION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^Description: ([^\r\n]{1,80})\r?$").expect("valid regex"));
/// A Java stack frame: `at net.caffeinemc.mods.sodium.client.X.method(X.java:12)` (the module
/// prefixes of FML, `TRANSFORMER/sodium@0.6/`, are skipped).
static FRAME_CLASS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^\s*at (?:[\w.\-]+(?:@[^/\s]*)?/)*((?:[a-z_][\w$]*\.)+[A-Za-z_$][\w$]*)\.[\w$<>]+\(",
    )
    .expect("valid regex")
});

/// Module file name prefixes (lower case) and their family. Programs that draw over games come
/// first: their hooks sit in the graphics calls, above the driver.
const NATIVE_FAMILIES: &[(&str, &str)] = &[
    ("rtsshooks", "overlay-rtss"),
    ("discordhook", "overlay-discord"),
    ("gameoverlayrenderer", "overlay-steam"),
    ("nvspcap", "overlay-nvidia"),
    ("graphics-hook", "overlay-obs"),
    ("obs-vulkan", "overlay-obs"),
    ("ow-graphics", "overlay-overwolf"),
    ("owclient", "overlay-overwolf"),
    ("eosovh", "overlay-epic"),
    ("fraps", "overlay-fraps"),
    ("nvoglv", "gpu-nvidia"),
    ("nvwgf2um", "gpu-nvidia"),
    ("nvd3dum", "gpu-nvidia"),
    ("nvgpucomp", "gpu-nvidia"),
    ("libnvidia-", "gpu-nvidia"),
    ("atio6axx", "gpu-amd"),
    ("atioglxx", "gpu-amd"),
    ("atig6txx", "gpu-amd"),
    ("atig6pxx", "gpu-amd"),
    ("atiumd", "gpu-amd"),
    ("atidxx", "gpu-amd"),
    ("amdxx", "gpu-amd"),
    ("amdxc", "gpu-amd"),
    ("amdvlk", "gpu-amd"),
    ("radeonsi", "gpu-amd"),
    ("libdrm_amdgpu", "gpu-amd"),
    ("ig9icd", "gpu-intel"),
    ("ig8icd", "gpu-intel"),
    ("ig75icd", "gpu-intel"),
    ("ig7icd", "gpu-intel"),
    ("ig4icd", "gpu-intel"),
    ("igxelpicd", "gpu-intel"),
    ("igdumdim", "gpu-intel"),
    ("igd10iumd", "gpu-intel"),
    ("igd12umd", "gpu-intel"),
    ("igvk", "gpu-intel"),
    ("igc64", "gpu-intel"),
    ("iris_dri", "gpu-intel"),
    ("i965_dri", "gpu-intel"),
    ("opengl32", "opengl"),
    ("jvm.", "jvm"),
    ("libjvm", "jvm"),
    ("lwjgl", "lwjgl"),
    ("liblwjgl", "lwjgl"),
    ("glfw", "lwjgl"),
    ("libglfw", "lwjgl"),
    ("openal", "openal"),
    ("soft_oal", "openal"),
    ("libopenal", "openal"),
    ("freetype", "freetype"),
    ("libfreetype", "freetype"),
    ("ntdll", "system"),
    ("kernelbase", "system"),
    ("kernel32", "system"),
    ("ucrtbase", "system"),
    ("msvcrt", "system"),
    ("msvcp", "system"),
    ("vcruntime", "system"),
    ("user32", "system"),
    ("win32u", "system"),
    ("gdi32", "system"),
    ("combase", "system"),
    ("libc.", "system"),
    ("libc-", "system"),
    ("libpthread", "system"),
    ("libsystem_", "system"),
];

/// Packages whose exception classes may be named: Java's, the game's, the loaders' and the
/// libraries they bring (a mod's own exception is `other`).
const NAMED_EXCEPTION_PACKAGES: &[&str] = &[
    "java.",
    "javax.",
    "jdk.",
    "sun.",
    "com.sun.",
    "net.minecraft.",
    "com.mojang.",
    "net.fabricmc.",
    "org.quiltmc.",
    "net.minecraftforge.",
    "net.neoforged.",
    "cpw.mods.",
    "org.spongepowered.",
    "com.llamalad7.mixinextras.",
    "org.lwjgl.",
    "io.netty.",
    "com.google.",
    "it.unimi.",
    "org.joml.",
    "org.apache.",
];

/// Libraries the game and the loaders bring: a frame of theirs points at no mod.
const LIBRARY_CLASSES: &[&str] = &[
    "com.google.",
    "org.apache.",
    "io.netty.",
    "it.unimi.",
    "org.lwjgl.",
    "org.joml.",
    "org.slf4j.",
    "org.spongepowered.",
    "com.llamalad7.",
    "org.objectweb.asm.",
    "kotlin.",
    "kotlinx.",
    "oshi.",
    "com.sun.",
    "com.ibm.icu.",
    "org.quiltmc.",
];

/// Mods a trace may name (public mods, the official pack's and the most played): the name in
/// the trace, the mod ids they declare and the packages of their code.
const KNOWN_MODS: &[(&str, &[&str], &[&str])] = &[
    (
        "projects-overlay",
        &["projects_overlay"],
        &["gg.projects.overlay."],
    ),
    (
        "sodium",
        &["sodium"],
        &["net.caffeinemc.mods.sodium.", "me.jellysquid.mods.sodium."],
    ),
    (
        "iris",
        &["iris"],
        &["net.irisshaders.iris.", "net.coderbot.iris."],
    ),
    (
        "lithium",
        &["lithium"],
        &[
            "net.caffeinemc.mods.lithium.",
            "me.jellysquid.mods.lithium.",
        ],
    ),
    (
        "sodium-extra",
        &["sodium-extra"],
        &["me.flashyreese.mods.sodiumextra."],
    ),
    (
        "reeses-sodium-options",
        &["reeses-sodium-options"],
        &["me.flashyreese.mods.reeses_sodium_options."],
    ),
    ("indium", &["indium"], &["link.infra.indium."]),
    (
        "fabric-api",
        &["fabric-api", "fabric"],
        &["net.fabricmc.fabric."],
    ),
    ("voicechat", &["voicechat"], &["de.maxhenkel.voicechat."]),
    (
        "xaero",
        &[
            "xaerominimap",
            "xaeroworldmap",
            "xaerobetterpvp",
            "xaeroplus",
            "xaerolib",
        ],
        &["xaero."],
    ),
    ("litematica", &["litematica"], &["fi.dy.masa.litematica."]),
    ("malilib", &["malilib"], &["fi.dy.masa.malilib."]),
    (
        "chesttracker",
        &["chesttracker"],
        &["red.jackf.chesttracker."],
    ),
    ("wi-zoom", &["wi_zoom"], &[]),
    ("logical-zoom", &["logical_zoom"], &[]),
    ("gamma-utils", &["gammautils"], &[]),
    ("modmenu", &["modmenu"], &["com.terraformersmc.modmenu."]),
    (
        "cloth-config",
        &["cloth-config", "cloth-config2"],
        &["me.shedaniel.clothconfig2.", "me.shedaniel.autoconfig."],
    ),
    (
        "yacl",
        &["yet_another_config_lib_v3", "yet-another-config-lib"],
        &["dev.isxander.yacl3."],
    ),
    (
        "entityculling",
        &["entityculling"],
        &["dev.tr7zw.entityculling."],
    ),
    (
        "skinlayers3d",
        &["skinlayers3d"],
        &["dev.tr7zw.skinlayers."],
    ),
    (
        "notenoughanimations",
        &["notenoughanimations"],
        &["dev.tr7zw.notenoughanimations."],
    ),
    ("ferritecore", &["ferritecore"], &["malte0811.ferritecore."]),
    (
        "immediatelyfast",
        &["immediatelyfast"],
        &["net.raphimc.immediatelyfast."],
    ),
    ("moreculling", &["moreculling"], &["ca.fxco.moreculling."]),
    ("modernfix", &["modernfix"], &["org.embeddedt.modernfix."]),
    ("c2me", &["c2me"], &["com.ishland.c2me."]),
    ("krypton", &["krypton"], &["me.steinborn.krypton."]),
    ("dynamic-fps", &["dynamic_fps"], &["dynamic_fps.impl."]),
    (
        "continuity",
        &["continuity"],
        &["me.pepperbell.continuity."],
    ),
    (
        "entity-texture-features",
        &["entity_texture_features"],
        &["traben.entity_texture_features."],
    ),
    (
        "entity-model-features",
        &["entity_model_features"],
        &["traben.entity_model_features."],
    ),
    ("appleskin", &["appleskin"], &["squeek.appleskin."]),
    ("jade", &["jade"], &["snownee.jade."]),
    ("emi", &["emi"], &["dev.emi.emi."]),
    ("rei", &["roughlyenoughitems"], &["me.shedaniel.rei."]),
    ("jei", &["jei"], &["mezz.jei."]),
    ("journeymap", &["journeymap"], &["journeymap."]),
    ("zoomify", &["zoomify"], &["dev.isxander.zoomify."]),
    (
        "shulkerboxtooltip",
        &["shulkerboxtooltip"],
        &["com.misterpemodder.shulkerboxtooltip."],
    ),
    ("architectury", &["architectury"], &["dev.architectury."]),
    ("geckolib", &["geckolib"], &["software.bernie.geckolib."]),
    ("owo", &["owo"], &["io.wispforest.owo."]),
    (
        "optifine",
        &["optifabric", "optifine"],
        &["net.optifine.", "me.modmuss50.optifabric."],
    ),
    ("create", &["create"], &["com.simibubi.create."]),
];

/// Descriptions the game writes in its crash reports (`CrashReport.forThrowable`): one of them
/// says where the game was when it failed.
const PHASES: &[&str] = &[
    "Unexpected error",
    "Initializing game",
    "Rendering overlay",
    "Rendering screen",
    "Rendering entity in world",
    "Rendering Block Entity",
    "Rendering item",
    "Ticking entity",
    "Ticking block entity",
    "Ticking player",
    "Exception in server tick loop",
    "Exception ticking world",
    "Tesselating block model",
    "Tesselating liquid in world",
    "Updating screen events",
    "mouseClicked event handler",
    "keyPressed event handler",
    "charTyped event handler",
    "Loading entity NBT",
    "Saving entity NBT",
    "Exception generating new chunk",
    "Feature placement",
    "Registering texture",
    "Batching chunks",
    "Watching Server",
    "Manually triggered debug crash",
];

/// The problematic frame of a JVM fatal error log: its type and, for a native frame, its module.
fn problematic_frame(text: &str) -> Option<(char, Option<&str>)> {
    let c = PROBLEMATIC_FRAME.captures(text)?;
    let kind = c.get(1)?.as_str().chars().next()?;
    Some((kind, c.get(2).map(|m| m.as_str())))
}

/// The family of the module a native crash happened in (see [`NATIVE_FAMILIES`]).
fn native_family(frame: char, module: Option<&str>) -> &'static str {
    let Some(module) = module else {
        // `J 1234 c2 net.minecraft...`, `j  ...`: compiled or interpreted Java code.
        return if matches!(frame, 'J' | 'j' | 'A') {
            "java-code"
        } else {
            "other"
        };
    };
    let lower = module.to_ascii_lowercase();
    NATIVE_FAMILIES
        .iter()
        .find(|(prefix, _)| lower.starts_with(*prefix))
        .map_or("other", |&(_, family)| family)
}

/// The program drawing over the game whose hook crashed it (`overlay-rtss`), from the JVM's
/// fatal error log.
pub(super) fn overlay_crash(text: &str) -> Option<&'static str> {
    let (frame, module) = problematic_frame(text)?;
    Some(native_family(frame, module)).filter(|family| family.starts_with("overlay-"))
}

/// Windows' name for an exit code, a few common numbers, else `other`.
fn exit_name(code: Option<i32>) -> &'static str {
    match code {
        None => "none",
        Some(NOT_RESPONDING_EXIT) => "hung",
        // 0xC0000005, 0xC00000FD, 0xC0000374, 0xC0000409, 0xC000001D, 0xC0000135, 0xC000013A
        Some(-1_073_741_819) => "access-violation",
        Some(-1_073_741_571) => "stack-overflow",
        Some(-1_073_740_940) => "heap-corruption",
        Some(-1_073_740_791) => "buffer-overrun",
        Some(-1_073_741_795) => "illegal-instruction",
        Some(-1_073_741_515) => "dll-missing",
        Some(-1_073_741_510) => "console-closed",
        Some(-1) => "-1",
        Some(0) => "0",
        Some(1) => "1",
        Some(2) => "2",
        Some(130) => "130",
        Some(134) => "134",
        Some(137) => "137",
        Some(139) => "139",
        Some(143) => "143",
        Some(255) => "255",
        Some(_) => "other",
    }
}

fn is_named_exception(class: &str) -> bool {
    NAMED_EXCEPTION_PACKAGES
        .iter()
        .any(|package| class.starts_with(package))
}

/// The root exception of the game's crash report, as the trace names it: its simple name for a
/// platform class (`NullPointerException`), else `other`.
fn root_exception(report: &str) -> Option<String> {
    // Only the report's own stack (its sections and system details below quote other things).
    let start = report.find("\nDescription: ").map_or(0, |at| at + 1);
    let head = &report[start..];
    let end = ["\nA detailed walkthrough of the error", "\n-- Head --"]
        .iter()
        .filter_map(|marker| head.find(marker))
        .min()
        .unwrap_or(head.len());
    let stack = &head[..end];
    let mut thrown = THROWN
        .captures_iter(stack)
        .filter(|c| !c[2].starts_with("Description"));
    let first = thrown.next()?;
    let root = thrown
        .filter(|c| c.get(1).is_some())
        .last()
        .unwrap_or(first);
    let class = &root[2];
    let simple = class.rsplit('.').next().unwrap_or(class);
    let named = is_named_exception(class)
        && simple.len() <= 60
        && simple.starts_with(|c: char| c.is_ascii_uppercase())
        && simple.chars().all(|c| c.is_ascii_alphanumeric());
    Some(if named {
        simple.to_owned()
    } else {
        "other".to_owned()
    })
}

/// The known mod an id or a class belongs to.
fn known_mod_by_id(id: &str) -> Option<&'static str> {
    let id = id.to_ascii_lowercase();
    if id.starts_with("fabric-") && id != "fabric-language-kotlin" {
        // Fabric API's modules (`fabric-rendering-v1`).
        return Some("fabric-api");
    }
    KNOWN_MODS
        .iter()
        .find(|(_, ids, _)| ids.contains(&id.as_str()))
        .map(|(key, _, _)| *key)
}

fn known_mod_by_class(class: &str) -> Option<&'static str> {
    KNOWN_MODS
        .iter()
        .find(|(_, _, packages)| packages.iter().any(|package| class.starts_with(package)))
        .map(|(key, _, _)| *key)
}

/// A class of the game, Java, a loader or a library they bring.
fn is_platform_class(class: &str) -> bool {
    PLATFORM_CLASSES
        .iter()
        .chain(LIBRARY_CLASSES)
        .any(|package| class.starts_with(package))
}

/// The top frames of the crash report's stack that belong to a mod's own code (not the game's,
/// Java's, a loader's or a library's), the first ones first.
pub(super) fn mod_frames(report: &str) -> Vec<String> {
    let start = report.find("\nDescription: ").map_or(0, |at| at + 1);
    report[start..]
        .lines()
        .filter(|line| line.trim_start().starts_with("at "))
        .take(8)
        .filter_map(|line| FRAME_CLASS.captures(line).map(|c| c[1].to_owned()))
        .filter(|class| !is_platform_class(class))
        .collect()
}

/// The mod the crash points at, for the trace: a known one by name, `mod` for another, `none`.
fn culprit(suspects: &[CrashSuspect], frames: &[String]) -> &'static str {
    let known = suspects.iter().find_map(|suspect| {
        suspect
            .mod_id
            .as_deref()
            .and_then(known_mod_by_id)
            .or_else(|| suspect.class_name.as_deref().and_then(known_mod_by_class))
    });
    if let Some(key) = known.or_else(|| frames.iter().find_map(|class| known_mod_by_class(class))) {
        return key;
    }
    if suspects.is_empty() && frames.is_empty() {
        "none"
    } else {
        "mod"
    }
}

/// Where the game was: the crash report's description, when it is one the game writes.
fn phase(report: &str) -> Option<&'static str> {
    let said = DESCRIPTION.captures(report)?;
    let said = said[1].trim();
    PHASES
        .iter()
        .find(|phase| phase.eq_ignore_ascii_case(said))
        .copied()
}

/// The trace of a crash the analyzer classified as `kind` (see the module). `report` is the
/// crash report or JVM fatal error log of the run, `text` all it read.
pub(super) fn trace(
    text: &str,
    report: Option<&str>,
    exit_code: Option<i32>,
    kind: CrashKind,
    suspects: &[CrashSuspect],
) -> CrashTrace {
    let game_report = report.filter(|report| report.contains(REPORT_HEADER));
    if game_report.is_none()
        && let Some((frame, module)) = problematic_frame(text)
    {
        return CrashTrace {
            sig: format!("native/{}", native_family(frame, module)),
            phase: None,
        };
    }
    if let Some(report) = game_report {
        let exception = root_exception(report).unwrap_or_else(|| "other".to_owned());
        // A mod failing the loader's checks names the mods it broke: the trace names the mod
        // whose code failed (the analyzer's suspects), never the frames of a healthy one.
        let frames = if matches!(kind, CrashKind::ModCrash | CrashKind::Unknown) {
            mod_frames(report)
        } else {
            Vec::new()
        };
        let named: Vec<CrashSuspect> = suspects
            .iter()
            .filter(|suspect| suspect.reason != SuspectReason::Missing)
            .cloned()
            .collect();
        return CrashTrace {
            sig: format!("java/{exception}/{}", culprit(&named, &frames)),
            phase: phase(report),
        };
    }
    CrashTrace {
        sig: format!("exit/{}", exit_name(exit_code)),
        phase: None,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::super::{CrashInput, analyze_crash};
    use super::*;

    fn shown(
        output: &str,
        report: Option<&str>,
        exit_code: Option<i32>,
    ) -> super::super::CrashDiagnosis {
        analyze_crash(&CrashInput {
            output,
            crash_report: report,
            exit_code,
            runtime: Duration::from_secs(1800),
            java_major: Some(25),
            required_java: Some(25),
            window_shown: true,
            ..CrashInput::default()
        })
    }

    fn hs_err(frame: &str) -> String {
        format!(
            "#\n# A fatal error has been detected by the Java Runtime Environment:\n#\n#  EXCEPTION_ACCESS_VIOLATION (0xc0000005) at pc=0x00007ffe1b2c3d4e, pid=4242, tid=1717\n#\n# JRE version: OpenJDK Runtime Environment (25.0.1+8) (build 25.0.1+8-LTS)\n# Problematic frame:\n# {frame}\n#\n# Core dump will be written. Default location: C:\\Users\\KorAtesi\\AppData\\Roaming\\x\\hs_err_pid4242.mdmp\n"
        )
    }

    const PLAYING: &str = "[18:02:11] [Render thread/INFO]: Loaded 1290 advancements\n";

    fn report(description: &str, stack: &str) -> String {
        format!(
            "---- Minecraft Crash Report ----\n// Shall we play a game?\n\nTime: 2026-11-12 18:31:03\nDescription: {description}\n\n{stack}\n\nA detailed walkthrough of the error, its code path and all known details is as follows:\n---------------------------------------------------------------------------------------\n\n-- Head --\nThread: Render thread\nStacktrace:\n\tat java.base/java.lang.Thread.run(Thread.java:1583)\n\n-- System Details --\nDetails:\n\tMinecraft Version: 26.1.2\n\tOperating System: Windows 11 (amd64) version 10.0\n\tJava Version: 25.0.1, Microsoft\n\tLaunched Version: fabric-loader-0.18.1-26.1.2\n\tBackend Library: LWJGL version 3.3.3\n\tUser: KorAtesi\n"
        )
    }

    #[test]
    fn native_crashes_name_the_family_of_their_module() {
        for (frame, sig) in [
            ("C  [atio6axx.dll+0x1b8a0f]", "native/gpu-amd"),
            ("C  [nvoglv64.dll+0xd5c3a]", "native/gpu-nvidia"),
            ("C  [ig9icd64.dll+0x2a1b3]", "native/gpu-intel"),
            ("V  [jvm.dll+0x3e1f20]", "native/jvm"),
            (
                "J 41234 c2 net.minecraft.client.renderer.LevelRenderer.renderLevel(Lnet/minecraft/client/DeltaTracker;)V (812 bytes) @ 0x00000201",
                "native/java-code",
            ),
            ("C  [lwjgl_opengl.dll+0x12ab]", "native/lwjgl"),
            ("C  [OpenAL.dll+0x9a1c]", "native/openal"),
            ("C  [freetype.dll+0x1f2e3]", "native/freetype"),
            ("C  [ntdll.dll+0xa1b2c]", "native/system"),
            ("C  [RTSSHooks64.dll+0x1b8a0f]", "native/overlay-rtss"),
            ("C  [DiscordHook64.dll+0x3c4d]", "native/overlay-discord"),
            ("C  [KorAtesi_tools.dll+0x10]", "native/other"),
            ("C  0x00007ffe1b2c3d4e", "native/other"),
        ] {
            let d = shown(PLAYING, Some(&hs_err(frame)), Some(1));
            assert_eq!(d.trace.sig, sig, "{frame}");
            assert!(d.trace.phase.is_none());
            assert!(!d.trace.sig.contains("KorAtesi"), "{frame}");
        }
    }

    #[test]
    fn overlay_hooks_are_another_program_not_the_driver() {
        for frame in [
            "C  [RTSSHooks64.dll+0x1b8a0f]",
            "C  [gameoverlayrenderer64.dll+0x8c2f1]",
            "C  [nvspcap64.dll+0x4e21]",
            "C  [graphics-hook64.dll+0x21f0]",
        ] {
            for window in [false, true] {
                let d = analyze_crash(&CrashInput {
                    output: PLAYING,
                    crash_report: Some(&hs_err(frame)),
                    exit_code: Some(1),
                    window_shown: window,
                    ..CrashInput::default()
                });
                assert_eq!(d.kind, CrashKind::OtherProgram, "{frame} window={window}");
                assert!(d.suspects.is_empty());
                assert!(d.mods.is_empty());
            }
        }
        // The driver's own frame stays a driver crash.
        let d = shown(PLAYING, Some(&hs_err("C  [amdvlk64.dll+0x51a2b]")), Some(1));
        assert_eq!(d.kind, CrashKind::GraphicsDriver);
        assert_eq!(d.trace.sig, "native/gpu-amd");
    }

    #[test]
    fn a_frozen_game_windows_closed_is_not_responding() {
        let d = shown(PLAYING, None, Some(NOT_RESPONDING_EXIT));
        assert_eq!(d.kind, CrashKind::NotResponding);
        assert_eq!(d.trace.sig, "exit/hung");
        assert!(d.suspects.is_empty());
        for text in [&d.title_tr, &d.hint_tr] {
            assert!(
                !text.contains("0x") && !text.contains("805306369"),
                "{text}"
            );
        }
        // A cause the output names still wins (the game ran out of memory, then hung).
        let d = shown(
            "[18:02:11] [Render thread/ERROR]: java.lang.OutOfMemoryError: Java heap space\n",
            None,
            Some(NOT_RESPONDING_EXIT),
        );
        assert_eq!(d.kind, CrashKind::OutOfMemory);
        assert_eq!(d.trace.sig, "exit/hung");
    }

    #[test]
    fn exit_codes_are_named_from_a_closed_list() {
        for (code, sig) in [
            (Some(-1_073_741_819), "exit/access-violation"),
            (Some(-1_073_740_791), "exit/buffer-overrun"),
            (Some(1), "exit/1"),
            (Some(-1), "exit/-1"),
            (Some(31_337), "exit/other"),
            (None, "exit/none"),
        ] {
            let d = shown(PLAYING, None, code);
            assert_eq!(d.trace.sig, sig, "{code:?}");
        }
    }

    #[test]
    fn an_in_game_crash_report_with_a_mod_in_its_stack_is_a_mod_crash() {
        let stack = "java.lang.NullPointerException: Cannot invoke \"Object.toString()\" because \"<local4>\" is null\n\
            \tat net.minecraft.client.gui.Gui.handler$zza000$xaerominimap$onRender(Gui.java:412)\n\
            \tat net.minecraft.client.gui.Gui.render(Gui.java:200)\n\
            \tat net.minecraft.client.renderer.GameRenderer.render(GameRenderer.java:999)";
        let crash = report("Rendering overlay", stack);
        let d = shown(PLAYING, Some(&crash), Some(-1));
        assert_eq!(d.kind, CrashKind::ModCrash);
        assert_eq!(d.mods, ["xaerominimap"]);
        assert_eq!(d.trace.sig, "java/NullPointerException/xaero");
        assert_eq!(d.trace.phase, Some("Rendering overlay"));
        // The player's name in the report never reaches the trace.
        assert!(!d.trace.sig.contains("KorAtesi"));
    }

    #[test]
    fn a_mod_class_at_the_top_of_the_stack_names_its_mod() {
        let stack = "java.lang.IllegalStateException: Buffer is not mapped\n\
            \tat net.caffeinemc.mods.sodium.client.render.chunk.RenderSection.upload(RenderSection.java:88)\n\
            \tat net.minecraft.client.renderer.LevelRenderer.renderLevel(LevelRenderer.java:1100)";
        let d = shown(PLAYING, Some(&report("Unexpected error", stack)), Some(-1));
        assert_eq!(d.kind, CrashKind::ModCrash);
        assert_eq!(
            d.suspects[0].class_name.as_deref(),
            Some("net.caffeinemc.mods.sodium.client.render.chunk.RenderSection")
        );
        assert_eq!(d.trace.sig, "java/IllegalStateException/sodium");
        // A mod outside the list is only `mod`: its package may name its author.
        let stack = "java.lang.IllegalStateException: boom\n\
            \tat com.korates.secretmod.Thing.tick(Thing.java:3)\n\
            \tat net.minecraft.world.entity.Entity.tick(Entity.java:10)";
        let d = shown(PLAYING, Some(&report("Ticking entity", stack)), Some(-1));
        assert_eq!(d.kind, CrashKind::ModCrash);
        assert_eq!(d.trace.sig, "java/IllegalStateException/mod");
        assert_eq!(d.trace.phase, Some("Ticking entity"));
        // A mod's own exception class is `other`; the root cause wins over its wrapper.
        let stack = "net.minecraft.ReportedException: Ticking entity\n\
            \tat net.minecraft.server.MinecraftServer.tickChildren(MinecraftServer.java:1)\n\
            Caused by: com.korates.secretmod.SecretException: no\n\
            \tat com.korates.secretmod.Thing.tick(Thing.java:3)";
        let d = shown(PLAYING, Some(&report("Ticking entity", stack)), Some(-1));
        assert_eq!(d.trace.sig, "java/other/mod");
    }

    #[test]
    fn a_vanilla_stack_names_no_mod_and_stays_unknown() {
        let stack = "java.lang.ArrayIndexOutOfBoundsException: Index 5 out of bounds for length 4\n\
            \tat net.minecraft.world.level.chunk.PalettedContainer.get(PalettedContainer.java:1)\n\
            \tat net.minecraft.client.renderer.chunk.SectionCompiler.compile(SectionCompiler.java:2)";
        let d = shown(PLAYING, Some(&report("Batching chunks", stack)), Some(-1));
        assert_eq!(d.kind, CrashKind::Unknown);
        assert_eq!(d.trace.sig, "java/ArrayIndexOutOfBoundsException/none");
        assert_eq!(d.trace.phase, Some("Batching chunks"));
        // A description the game does not write is not kept.
        let d = shown(
            PLAYING,
            Some(&report("KorAtesi's world exploded", stack)),
            Some(-1),
        );
        assert_eq!(d.trace.phase, None);
    }

    #[test]
    fn before_the_window_a_report_with_mods_stays_what_it_was() {
        let stack = "java.lang.IllegalStateException: boom\n\
            \tat net.caffeinemc.mods.sodium.client.SodiumClientMod.onInitialize(SodiumClientMod.java:1)";
        let d = analyze_crash(&CrashInput {
            output: "[18:02:11] [main/INFO]: Loading Minecraft 26.1.2 with Fabric Loader 0.18.1",
            crash_report: Some(&report("Initializing game", stack)),
            exit_code: Some(-1),
            ..CrashInput::default()
        });
        assert_ne!(d.kind, CrashKind::ModCrash);
        assert_eq!(d.trace.sig, "java/IllegalStateException/sodium");
        assert_eq!(d.trace.phase, Some("Initializing game"));
    }

    #[test]
    fn known_mods_by_id_and_by_package() {
        assert_eq!(known_mod_by_id("fabric-rendering-v1"), Some("fabric-api"));
        assert_eq!(
            known_mod_by_id("projects_overlay"),
            Some("projects-overlay")
        );
        assert_eq!(known_mod_by_id("Sodium"), Some("sodium"));
        assert_eq!(known_mod_by_id("korates_mod"), None);
        assert_eq!(
            known_mod_by_class("gg.projects.overlay.voice.VoiceHud"),
            Some("projects-overlay")
        );
        assert_eq!(known_mod_by_class("com.korates.Thing"), None);
        // Every name a trace may carry passes the service's location pattern.
        let ok = |s: &str| {
            !s.is_empty()
                && s.len() <= 40
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        };
        for &(key, _, _) in KNOWN_MODS {
            assert!(ok(key), "{key}");
        }
        for &(_, family) in NATIVE_FAMILIES {
            assert!(ok(family), "{family}");
        }
    }
}

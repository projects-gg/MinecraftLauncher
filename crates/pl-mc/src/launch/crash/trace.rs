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
//! (`Rendering overlay`). [`CrashTrace::opening`] says whether the game's window had shown, and a
//! JVM fatal error's [`NativeFault`] what the JVM reported and what kind of thread it hit, from the
//! closed lists [`FAULT_ERRORS`] and [`FAULT_THREADS`] (TELEM-JVM): never a thread's own name.

use std::sync::LazyLock;

use regex::Regex;

use super::{
    CrashKind, CrashSuspect, PLATFORM_CLASSES, PLATFORM_IDS, SuspectReason, config_mod_name,
};

/// The coarse trace of one crash (see the module).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CrashTrace {
    /// `native/gpu-amd`, `java/NullPointerException/sodium`, `exit/hung`.
    pub sig: String,
    /// The crash report's description, when it is one of [`PHASES`].
    pub phase: Option<&'static str>,
    /// The game's window had not shown yet: it failed while it opened.
    pub opening: bool,
    /// What a JVM fatal error log says about the failure (`native/…` traces only).
    pub fault: Option<NativeFault>,
}

impl CrashTrace {
    /// The JVM itself or the code it compiled failed (`native/jvm`, `native/java-code`), not a
    /// driver, a library or another program, nor memory the computer could not give: what Java's
    /// AOT cache could be behind.
    pub fn in_jvm(&self) -> bool {
        matches!(self.sig.as_str(), "native/jvm" | "native/java-code")
            && self.fault.is_none_or(|fault| fault.error() != "oom")
    }
}

/// A JVM fatal error's error and the kind of thread it hit, from [`FAULT_ERRORS`] and
/// [`FAULT_THREADS`] (kept as their places in the lists: a crash diagnosis travels in events).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeFault {
    error: u8,
    thread: u8,
}

impl NativeFault {
    fn new(error: &str, thread: &str) -> Self {
        let place = |list: &[&str], token: &str| {
            let at = list.iter().position(|known| *known == token);
            u8::try_from(at.unwrap_or(list.len() - 1)).unwrap_or(u8::MAX)
        };
        Self {
            error: place(FAULT_ERRORS, error),
            thread: place(FAULT_THREADS, thread),
        }
    }

    /// One of [`FAULT_ERRORS`].
    pub fn error(self) -> &'static str {
        FAULT_ERRORS
            .get(usize::from(self.error))
            .copied()
            .unwrap_or("other")
    }

    /// One of [`FAULT_THREADS`].
    pub fn thread(self) -> &'static str {
        FAULT_THREADS
            .get(usize::from(self.thread))
            .copied()
            .unwrap_or("other")
    }
}

/// What a JVM fatal error log reports (keep in step with `pl_telemetry::detail`): an access
/// violation (`av`; `av-null` at an address below 64 KiB, a missing null check rather than
/// memory that was overwritten), a stack overflow, an illegal instruction, a page the system
/// could not read in (`inpage`: the disk or the page file), the JVM's own check that failed
/// (`internal`), memory it could not get (`oom`).
pub const FAULT_ERRORS: &[&str] = &[
    "av", "av-null", "stack", "ill", "inpage", "internal", "oom", "other",
];
/// The kind of thread a JVM fatal error hit: the JIT compiler, the garbage collector, the VM's
/// own, the game's render, server and worker threads, another Java thread, a thread outside Java
/// (`native`: a driver's or a library's).
pub const FAULT_THREADS: &[&str] = &[
    "jit", "gc", "vm", "render", "server", "worker", "java", "native", "other",
];

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
/// What follows `ExceptionInInitializerError: ` when the JVM rethrows a class's earlier static
/// initializer failure (JDK 17+): `Exception java.lang.NullPointerException: text [in thread "x"]`.
static EARLIER_INIT_FAILURE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*Exception ((?:[A-Za-z_$][\w$]*\.)+[A-Za-z_$][\w$]*)").expect("valid regex")
});
static DESCRIPTION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^Description: ([^\r\n]{1,80})\r?$").expect("valid regex"));
/// A Java stack frame: `at net.caffeinemc.mods.sodium.client.X.method(X.java:12)`, its class and
/// method. Class loader and module prefixes are skipped: FML's `TRANSFORMER/sodium@0.6/`,
/// `java.base/` and Fabric's `knot//` (its class loader, in no module), which every frame of a
/// Fabric game's code carries.
static FRAME_CLASS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^\s*at (?:[\w.\-]*(?:@[^/\s]*)?/)*((?:[a-z_][\w$]*\.)+[A-Za-z_$][\w$]*)\.([\w$<>]+)\(",
    )
    .expect("valid regex")
});
/// A Mixin handler merged into the game's code is named after its mod:
/// `handler$zza000$sodium$onRender`, `wrapMethod$bkn000$dynamic_fps_common$renderFrame`.
static HANDLER_METHOD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[A-Za-z]+\$[a-z]{3}\d{3}\$([a-z][a-z0-9_\-]{1,63})\$").expect("valid regex")
});
/// FML names the mod of a frame's module: `at TRANSFORMER/create@0.5.1.f/com.simibubi...`.
static FML_MODULE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*at TRANSFORMER/([a-z][a-z0-9_]{1,63})@[^/\s]+/").expect("valid regex")
});
/// The lines of a crash report's "Mixins in Stacktrace" section (a mod adds one): a class of the
/// stack (`\tnet.minecraft.client.renderer.LevelRenderer:`) and each mixin mods applied to it, with
/// its config (`\t\tnet.irisshaders.iris.mixin.MixinLevelRenderer (mixins.iris.json)`).
static MIXINS_CLASS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s+([\w.$]+):\s*$").expect("valid regex"));
static MIXINS_ENTRY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s+([\w.$]+) \(([\w\-.]+\.json)\)\s*$").expect("valid regex"));

/// The error line of a JVM fatal error log's header (`#  EXCEPTION_ACCESS_VIOLATION (0xc0000005)
/// at pc=…`, `#  SIGSEGV (0xb) at pc=…`, `#  Internal Error (g1…cpp:12), pid=…`,
/// `#  Out of Memory Error (os_windows.cpp:3600), pid=…`).
static FATAL_ERROR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^#\s+(EXCEPTION_[A-Z_]+|SIG[A-Z]+|Internal Error|Out of Memory Error)\b")
        .expect("valid regex")
});
/// The header of a JVM fatal error, and of one where it could not get memory (that log has no
/// problematic frame).
static FATAL_HEADER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^#\s*A fatal error has been detected by the Java Runtime Environment")
        .expect("valid regex")
});
static NATIVE_OOM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^#\s*There is insufficient memory for the Java Runtime Environment")
        .expect("valid regex")
});
/// The address an access violation touched: `siginfo: EXCEPTION_ACCESS_VIOLATION (0xc0000005),
/// reading address 0x0000000000000008`, `si_addr: 0x0000000000000008`.
static FAULT_ADDRESS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^siginfo:[^\r\n]*?(?:address|si_addr:)\s*0x([0-9a-fA-F]{1,16})")
        .expect("valid regex")
});
/// The thread a JVM fatal error hit: `Current thread (0x…):  JavaThread "Render thread" daemon
/// [_thread_in_native, …]`, `Current thread (0x…):  GCTaskThread "GC Thread#3" […]`, or `Current
/// thread is native thread`. Its name is only compared, never kept.
static CURRENT_THREAD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?m)^Current thread (?:is (native) thread|\(0x[0-9a-fA-F]+\):[ \t]+([A-Za-z0-9_]+)(?:[ \t]+"([^"\r\n]{0,120})")?)"#,
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
    ("wi-zoom", &["wi_zoom"], &["net.wimods.zoom."]),
    ("logical-zoom", &["logical_zoom"], &[]),
    (
        "gamma-utils",
        &["gammautils"],
        &["io.github.sjouwer.gammautils."],
    ),
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
    // The rest of the official pack (a trace names our own mods, not only `mod`).
    (
        "badoptimizations",
        &["badoptimizations"],
        &["me.thosea.badoptimizations."],
    ),
    (
        "betteradvancements",
        &["betteradvancements"],
        &["betteradvancements."],
    ),
    // No longer in the pack's jars here, so by its id only.
    ("betterf3", &["betterf3"], &[]),
    (
        "bettermounthud",
        &["bettermounthud"],
        &["me.lortseam.bettermounthud."],
    ),
    (
        "betterstats",
        &["betterstats"],
        &["com.thecsdev.betterstats."],
    ),
    (
        "tcdcommons",
        &["tcdcommons"],
        &["com.thecsdev.common.", "com.thecsdev.commonmc."],
    ),
    ("boatiview", &["boatiview"], &["me.soapsuds.boatiview."]),
    ("capes", &["capes"], &["me.cael.capes."]),
    ("clumps", &["clumps"], &["com.blamejared.clumps."]),
    ("fpsreducer", &["fpsreducer"], &["bre2el.fpsreducer."]),
    ("ias", &["ias"], &["ru.vidtu.ias."]),
    ("mousetweaks", &["mousetweaks"], &["yalter.mousetweaks."]),
    (
        "resourcify",
        &["resourcify"],
        &["dev.dediamondpro.resourcify."],
    ),
    (
        "sodium-extra-information",
        &["sodiumextrainformation"],
        &["net.anvian.sodiumextrainformation."],
    ),
    ("anvianslib", &["anvianslib"], &["net.anvian.anvianslib."]),
    (
        "dynamiclights",
        &["dynamiclights"],
        &["net.tschipcraft.dynamiclights."],
    ),
    (
        "entity-sound-features",
        &["entity_sound_features"],
        &["traben.entity_sound_features."],
    ),
    (
        "fabric-language-kotlin",
        &["fabric-language-kotlin"],
        &["net.fabricmc.language.kotlin."],
    ),
    (
        "language-reload",
        &["languagereload"],
        &["jerozgen.languagereload."],
    ),
    (
        "morechathistory",
        &["morechathistory"],
        &["red.jackf.morechathistory."],
    ),
    ("optigui", &["optigui"], &["opekope2.optigui."]),
    (
        "placeholder-api",
        &["placeholder-api"],
        &["eu.pb4.placeholders."],
    ),
    ("quick-pack", &["quick-pack"], &["me.drex.quickpack."]),
    ("replaymod", &["replaymod"], &["com.replaymod."]),
    ("rrls", &["rrls"], &["org.redlance.dima_dencep.mods.rrls."]),
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

/// The family of the module a JVM fatal error happened in (its problematic frame's). Without that
/// frame: the JVM's own (`jvm`) when it could not get memory, `other` for a log cut short; `None`
/// when the JVM did not die of a fatal error.
fn fatal_family(text: &str) -> Option<&'static str> {
    if let Some((frame, module)) = problematic_frame(text) {
        Some(native_family(frame, module))
    } else if NATIVE_OOM.is_match(text) {
        Some("jvm")
    } else if FATAL_HEADER.is_match(text) {
        Some("other")
    } else {
        None
    }
}

/// The program drawing over the game whose hook crashed it (`overlay-rtss`), from the JVM's
/// fatal error log.
pub(super) fn overlay_crash(text: &str) -> Option<&'static str> {
    let (frame, module) = problematic_frame(text)?;
    Some(native_family(frame, module)).filter(|family| family.starts_with("overlay-"))
}

/// What a JVM fatal error log reports, as one of [`FAULT_ERRORS`].
fn fault_error(log: &str) -> &'static str {
    let Some(error) = FATAL_ERROR.captures(log) else {
        return if NATIVE_OOM.is_match(log) {
            "oom"
        } else {
            "other"
        };
    };
    match &error[1] {
        "EXCEPTION_ACCESS_VIOLATION" | "SIGSEGV" => {
            let near_null = FAULT_ADDRESS
                .captures(log)
                .and_then(|address| u64::from_str_radix(&address[1], 16).ok())
                .is_some_and(|address| address < 0x1_0000);
            if near_null { "av-null" } else { "av" }
        }
        "EXCEPTION_STACK_OVERFLOW" => "stack",
        "EXCEPTION_ILLEGAL_INSTRUCTION" | "EXCEPTION_PRIV_INSTRUCTION" | "SIGILL" => "ill",
        "EXCEPTION_IN_PAGE_ERROR" | "SIGBUS" => "inpage",
        "Internal Error" => "internal",
        "Out of Memory Error" => "oom",
        _ => "other",
    }
}

/// The kind of thread a JVM fatal error hit, as one of [`FAULT_THREADS`].
fn fault_thread(log: &str) -> &'static str {
    let Some(current) = CURRENT_THREAD.captures(log) else {
        return "other";
    };
    if current.get(1).is_some() {
        return "native";
    }
    let kind = current.get(2).map_or("", |m| m.as_str());
    let name = current.get(3).map_or("", |m| m.as_str());
    if kind == "CompilerThread" || name.contains("CompilerThread") {
        "jit"
    } else if kind == "JavaThread" {
        match name {
            "Render thread" => "render",
            "Server thread" => "server",
            _ if name.starts_with("Worker-Main") => "worker",
            _ => "java",
        }
    } else if matches!(kind, "VMThread" | "WatcherThread") {
        "vm"
    } else if kind.contains("GC")
        || kind.starts_with("G1")
        || kind.starts_with('Z')
        || kind.starts_with("Shenandoah")
        || kind == "WorkerThread"
        || name.starts_with("GC Thread")
        || name.starts_with("G1 ")
    {
        "gc"
    } else {
        "other"
    }
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
    let mut class = &root[2];
    // A class whose static initializer failed earlier throws `NoClassDefFoundError: Could not
    // initialize class X` from then on, caused by a stackless `ExceptionInInitializerError` that
    // names the original failure on its line: that one is the root.
    if class == "java.lang.ExceptionInInitializerError" {
        let rest = stack[root.get(0).map_or(0, |m| m.end())..]
            .lines()
            .next()
            .unwrap_or("");
        if let Some(earlier) = EARLIER_INIT_FAILURE.captures(rest).and_then(|c| c.get(1)) {
            class = earlier.as_str();
        }
    }
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

/// Frames looked at from the top of each exception of the crash report's stack.
const TOP_FRAMES: usize = 12;

/// The game's main loops (Mojang's names, and Fabric's intermediary names of the client's):
/// every crash of a thread passes through them, and so do the mods that wrap them (a frame rate
/// limiter wraps `renderFrame`). Their frames and the ones below say nothing about the failure.
const MAIN_LOOP: &[(&str, &[&str])] = &[
    (
        "net.minecraft.client.Minecraft",
        &["run", "runTick", "renderFrame", "tick"],
    ),
    (
        "net.minecraft.class_310",
        &["method_1514", "method_1523", "method_1574"],
    ),
    ("net.minecraft.client.renderer.GameRenderer", &["render"]),
    (
        "net.minecraft.server.MinecraftServer",
        &["runServer", "tickServer", "tickChildren"],
    ),
    (
        "net.minecraft.client.server.IntegratedServer",
        &["tickServer"],
    ),
    ("net.minecraft.client.main.Main", &["main"]),
];

/// A frame of a main loop (`renderFrame$mixinextras$wrapped$426` is the original `renderFrame`
/// a mod wrapped).
fn main_loop(class: &str, method: &str) -> bool {
    let method = method.split('$').next().unwrap_or(method);
    MAIN_LOOP
        .iter()
        .any(|(owner, methods)| *owner == class && methods.contains(&method))
}

/// Mods that carry other mods' code: Fabric API's events and hooks, the Kotlin language adapter,
/// config screen libraries and multi-loader layers. A failure in their frames is mostly the code
/// of the mod they called; they are the culprit only when no other mod is named.
const CARRIERS: &[&str] = &[
    "fabric-api",
    "fabric-language-kotlin",
    "cloth-config",
    "yacl",
    "architectury",
    "owo",
];

/// One of [`CARRIERS`] (by its name in a trace).
fn is_carrier(key: &str) -> bool {
    CARRIERS.contains(&key)
}

/// A mod a frame of the crash report's stack points at: a class of its own code, or its id (a
/// Mixin handler it merged into the game's code, FML's module of the frame).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum StackMod {
    Class(String),
    Id(String),
}

impl StackMod {
    /// The mod's name in a trace, when it is one of [`KNOWN_MODS`].
    fn known(&self) -> Option<&'static str> {
        match self {
            Self::Class(class) => known_mod_by_class(class),
            Self::Id(id) => known_mod_by_id(id),
        }
    }

    /// A mod that carries other mods' code ([`CARRIERS`]).
    pub(super) fn carrier(&self) -> bool {
        self.known().is_some_and(is_carrier)
    }

    /// What tells two frames of one mod apart from two mods: its name, its id, else the first
    /// three parts of its package.
    fn owner(&self) -> String {
        if let Some(key) = self.known() {
            return key.to_owned();
        }
        match self {
            Self::Id(id) => id.clone(),
            Self::Class(class) => class.split('.').take(3).collect::<Vec<_>>().join("."),
        }
    }
}

/// The mod a stack frame points at: FML's module of a mod, a Mixin handler's mod, else the class of
/// a mod's own code (not the game's, Java's, a loader's or a library's).
fn frame_mod(line: &str) -> Option<StackMod> {
    let platform = |id: &str| PLATFORM_IDS.iter().any(|p| p.eq_ignore_ascii_case(id));
    if let Some(module) = FML_MODULE.captures(line)
        && !platform(&module[1])
    {
        return Some(StackMod::Id(module[1].to_owned()));
    }
    let frame = FRAME_CLASS.captures(line)?;
    if let Some(handler) = HANDLER_METHOD.captures(&frame[2]) {
        let id = &handler[1];
        return (!platform(id)).then(|| StackMod::Id(id.to_owned()));
    }
    let class = &frame[1];
    (!is_platform_class(class)).then(|| StackMod::Class(class.to_owned()))
}

/// The mods in the top frames of the crash report's own stack, one entry per mod: the exception
/// that caused the others first (where the failure began), then the ones it caused.
pub(super) fn stack_mods(report: &str) -> Vec<StackMod> {
    let start = report.find("\nDescription: ").map_or(0, |at| at + 1);
    let head = &report[start..];
    let end = head
        .find("\nA detailed walkthrough of the error")
        .unwrap_or(head.len());
    // The thrown exception, then each `Caused by:`.
    let mut blocks: Vec<Vec<&str>> = vec![Vec::new()];
    for line in head[..end].lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("Caused by:") {
            blocks.push(Vec::new());
        } else if trimmed.starts_with("at ")
            && let Some(block) = blocks.last_mut()
        {
            block.push(line);
        }
    }
    let mut mods: Vec<StackMod> = Vec::new();
    for block in blocks.iter().rev() {
        for line in block.iter().take(TOP_FRAMES) {
            if FRAME_CLASS
                .captures(line)
                .is_some_and(|frame| main_loop(&frame[1], &frame[2]))
            {
                break;
            }
            if let Some(found) = frame_mod(line)
                && !mods.iter().any(|m| m.owner() == found.owner())
            {
                mods.push(found);
            }
        }
    }
    mods
}

/// The mixins mods applied to the class the crash report's exception was thrown in (its top
/// frame's), from the report's "Mixins in Stacktrace" section: `(mixin class, mixin config)`.
/// Empty without that section.
pub(super) fn thrower_mixins(report: &str) -> Vec<(String, String)> {
    let start = report.find("\nDescription: ").map_or(0, |at| at + 1);
    let Some(thrower) = report[start..]
        .lines()
        .find(|line| line.trim_start().starts_with("at "))
        .and_then(|line| FRAME_CLASS.captures(line))
        .map(|frame| frame[1].to_owned())
    else {
        return Vec::new();
    };
    let Some(at) = report.find("Mixins in Stacktrace:") else {
        return Vec::new();
    };
    let mut mixins = Vec::new();
    let mut thrower_listed = false;
    for line in report[at..].lines().skip(1) {
        if let Some(class) = MIXINS_CLASS.captures(line) {
            if thrower_listed {
                break;
            }
            thrower_listed = class[1] == thrower;
        } else if let Some(mixin) = MIXINS_ENTRY.captures(line) {
            if thrower_listed {
                mixins.push((mixin[1].to_owned(), mixin[2].to_owned()));
            }
        } else {
            break;
        }
    }
    mixins
}

/// The mod the crash points at, for the trace: the first suspect, else the first frame, that is
/// not a carrier ([`CARRIERS`]: one is named only when nothing else is), by name when it is a
/// known one, `mod` for another; `none` for none.
fn culprit(suspects: &[CrashSuspect], frames: &[StackMod]) -> &'static str {
    let named: Vec<Option<&'static str>> = suspects
        .iter()
        .map(|suspect| {
            suspect
                .mod_id
                .as_deref()
                .and_then(known_mod_by_id)
                .or_else(|| suspect.class_name.as_deref().and_then(known_mod_by_class))
                .or_else(|| {
                    suspect
                        .mixin_config
                        .as_deref()
                        .and_then(config_mod_name)
                        .and_then(|id| known_mod_by_id(&id))
                })
        })
        .chain(frames.iter().map(StackMod::known))
        .collect();
    named
        .iter()
        .find(|key| !key.is_some_and(is_carrier))
        .or_else(|| named.first())
        .map_or("none", |key| key.unwrap_or("mod"))
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
/// crash report or JVM fatal error log of the run, `text` all it read; `opening` when the game's
/// window had not shown.
pub(super) fn trace(
    text: &str,
    report: Option<&str>,
    exit_code: Option<i32>,
    kind: CrashKind,
    suspects: &[CrashSuspect],
    opening: bool,
) -> CrashTrace {
    let game_report = report.filter(|report| report.contains(REPORT_HEADER));
    if game_report.is_none()
        && let Some(family) = fatal_family(text)
    {
        let log = report.unwrap_or(text);
        return CrashTrace {
            sig: format!("native/{family}"),
            phase: None,
            opening,
            fault: Some(NativeFault::new(fault_error(log), fault_thread(log))),
        };
    }
    if let Some(report) = game_report {
        let exception = root_exception(report).unwrap_or_else(|| "other".to_owned());
        // A mod failing the loader's checks names the mods it broke: the trace names the mod
        // whose code failed (the analyzer's suspects), never the frames of a healthy one.
        let frames = if matches!(kind, CrashKind::ModCrash | CrashKind::Unknown) {
            stack_mods(report)
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
            opening,
            fault: None,
        };
    }
    CrashTrace {
        sig: format!("exit/{}", exit_name(exit_code)),
        phase: None,
        opening,
        fault: None,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::super::{CrashInput, SuggestedAction, analyze_crash};
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
    fn a_static_initializer_that_failed_earlier_names_its_first_exception() {
        // JDK 17+: the class's first failure, rethrown on every later use without a stack.
        let stack = "java.lang.NoClassDefFoundError: Could not initialize class xaero.common.misc.Misc\n\
            \tat xaero.common.HudMod.tick(HudMod.java:12)\n\
            \tat net.minecraft.client.Minecraft.tick(Minecraft.java:40)\n\
            Caused by: java.lang.ExceptionInInitializerError: Exception java.lang.NullPointerException: Cannot invoke \"Object.getClass()\" [in thread \"Render thread\"]\n\
            \tat xaero.common.misc.Misc.<clinit>(Misc.java:3)";
        let d = shown(PLAYING, Some(&report("Unexpected error", stack)), Some(-1));
        assert_eq!(d.trace.sig, "java/NullPointerException/xaero");
        // A mod's own exception there is `other`, as anywhere.
        let stack = "java.lang.NoClassDefFoundError: Could not initialize class xaero.common.misc.Misc\n\
            \tat xaero.common.HudMod.tick(HudMod.java:12)\n\
            Caused by: java.lang.ExceptionInInitializerError: Exception com.korates.secret.Boom [in thread \"Render thread\"]";
        let d = shown(PLAYING, Some(&report("Unexpected error", stack)), Some(-1));
        assert_eq!(d.trace.sig, "java/other/xaero");
        // A first failure keeps its cause, as before.
        let stack = "java.lang.ExceptionInInitializerError\n\
            \tat xaero.common.HudMod.tick(HudMod.java:12)\n\
            Caused by: java.lang.IllegalStateException: not ready\n\
            \tat xaero.common.misc.Misc.<clinit>(Misc.java:3)";
        let d = shown(PLAYING, Some(&report("Unexpected error", stack)), Some(-1));
        assert_eq!(d.trace.sig, "java/IllegalStateException/xaero");
    }

    #[test]
    fn the_official_packs_mods_are_named() {
        assert_eq!(known_mod_by_id("replaymod"), Some("replaymod"));
        assert_eq!(
            known_mod_by_class("org.redlance.dima_dencep.mods.rrls.Rrls"),
            Some("rrls")
        );
        assert_eq!(
            known_mod_by_class("com.thecsdev.commonmc.api.Thing"),
            Some("tcdcommons")
        );
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

    /// A JVM fatal error log with its thread part and a process part that quotes exceptions the
    /// game caught before.
    fn fatal(error: &str, siginfo: &str, thread: &str, frame: &str) -> String {
        format!(
            "#\n# A fatal error has been detected by the Java Runtime Environment:\n#\n#  {error}\n#\n# JRE version: OpenJDK Runtime Environment Microsoft-12574223 (25.0.1+8) (build 25.0.1+8-LTS)\n# Problematic frame:\n# {frame}\n#\n\n---------------  S U M M A R Y ------------\n\nHost: AMD Ryzen 7 5800X 8-Core Processor, 16 cores, 31G,  Windows 11 , 64 bit Build 26100\n\n---------------  T H R E A D  ---------------\n\n{thread}\n\nStack: [0x0000003a29800000,0x0000003a29900000]\n\n{siginfo}\n\n---------------  P R O C E S S  ---------------\n\nExceptions (2 events):\nEvent: 8601.2 Thread 0x000001b8d0638980 Exception <a 'java/nio/file/AccessDeniedException'{{0x00000007}}: C:\\Users\\KorAtesi\\x> (0x00000007) thrown\nEvent: 8602.0 Thread 0x000001b8d0638980 Exception <a 'java/lang/OutOfMemoryError'{{0x00000008}}> (0x00000008) thrown\n\n---------------  S Y S T E M  ---------------\n\nOS:\n Windows 11 , 64 bit Build 26100\n"
        )
    }

    const AV: &str =
        "EXCEPTION_ACCESS_VIOLATION (0xc0000005) at pc=0x00007ffe1b2c3d4e, pid=4242, tid=1717";
    const WILD: &str =
        "siginfo: EXCEPTION_ACCESS_VIOLATION (0xc0000005), reading address 0x00000473000007d4";
    const GC: &str = "Current thread (0x000001b8d0638980):  GCTaskThread \"GC Thread#3\" [id=1717]";

    #[test]
    fn a_fatal_error_names_its_error_and_the_kind_of_thread_it_hit() {
        let null =
            "siginfo: EXCEPTION_ACCESS_VIOLATION (0xc0000005), reading address 0x0000000000000010";
        let thread = |kind: &str, name: &str| {
            format!(
                "Current thread (0x000001b8d0638980):  {kind} \"{name}\" daemon [_thread_in_native, id=1717]"
            )
        };
        for (error, siginfo, current, want) in [
            (AV, WILD, GC.to_owned(), ("av", "gc")),
            (
                AV,
                null,
                thread("JavaThread", "C2 CompilerThread0"),
                ("av-null", "jit"),
            ),
            (
                AV,
                WILD,
                thread("CompilerThread", "C1 CompilerThread0"),
                ("av", "jit"),
            ),
            (
                AV,
                WILD,
                thread("JavaThread", "Render thread"),
                ("av", "render"),
            ),
            (
                AV,
                WILD,
                thread("JavaThread", "Server thread"),
                ("av", "server"),
            ),
            (
                AV,
                WILD,
                thread("JavaThread", "Worker-Main-125"),
                ("av", "worker"),
            ),
            (
                AV,
                WILD,
                thread("JavaThread", "KorAtesi's thread"),
                ("av", "java"),
            ),
            (AV, WILD, thread("VMThread", "VM Thread"), ("av", "vm")),
            (
                AV,
                WILD,
                thread("ConcurrentGCThread", "G1 Conc#0"),
                ("av", "gc"),
            ),
            (
                AV,
                WILD,
                thread("WorkerThread", "GC Thread#0"),
                ("av", "gc"),
            ),
            (
                AV,
                WILD,
                "Current thread is native thread".to_owned(),
                ("av", "native"),
            ),
            (
                "EXCEPTION_STACK_OVERFLOW (0xc00000fd) at pc=0x00007ffe1b2c3d4e, pid=4242, tid=1717",
                "",
                thread("JavaThread", "Render thread"),
                ("stack", "render"),
            ),
            (
                "EXCEPTION_IN_PAGE_ERROR (0xc0000006) at pc=0x00007ffe1b2c3d4e, pid=4242, tid=1717",
                "",
                String::new(),
                ("inpage", "other"),
            ),
            (
                "EXCEPTION_ILLEGAL_INSTRUCTION (0xc000001d) at pc=0x00007ffe1b2c3d4e, pid=4242",
                "",
                String::new(),
                ("ill", "other"),
            ),
            (
                "Internal Error (g1ConcurrentMark.cpp:1234), pid=4242, tid=1717",
                "",
                thread("ConcurrentGCThread", "G1 Conc#0"),
                ("internal", "gc"),
            ),
            (
                "SIGSEGV (0xb) at pc=0x00007f1b2c3d4e5f, pid=4242, tid=1717",
                "siginfo: si_signo: 11 (SIGSEGV), si_code: 1 (SEGV_MAPERR), si_addr: 0x0000000000000008",
                thread("JavaThread", "Render thread"),
                ("av-null", "render"),
            ),
        ] {
            let log = fatal(error, siginfo, &current, "V  [jvm.dll+0x3e1f20]");
            let d = shown(PLAYING, Some(&log), Some(1));
            assert_eq!(d.trace.sig, "native/jvm", "{error} {current}");
            let fault = d.trace.fault.expect("a fatal error's fault");
            assert_eq!((fault.error(), fault.thread()), want, "{error} {current}");
            assert!(FAULT_ERRORS.contains(&fault.error()));
            assert!(FAULT_THREADS.contains(&fault.thread()));
            assert!(!d.trace.opening);
            assert!(d.trace.in_jvm());
        }
        // While the window opened, in a driver.
        let log = fatal(
            AV,
            WILD,
            "Current thread is native thread",
            "C  [atio6axx.dll+0x1b8a0f]",
        );
        let d = analyze_crash(&CrashInput {
            output: PLAYING,
            crash_report: Some(&log),
            exit_code: Some(1),
            ..CrashInput::default()
        });
        assert_eq!(d.trace.sig, "native/gpu-amd");
        assert!(d.trace.opening);
        assert!(!d.trace.in_jvm());
        // Only a fatal error has a fault.
        assert_eq!(shown(PLAYING, None, Some(1)).trace.fault, None);
    }

    #[test]
    fn a_fatal_error_without_a_frame_is_still_the_jvms() {
        // The JVM could not get memory: its log has no problematic frame.
        let oom = "#\n# There is insufficient memory for the Java Runtime Environment to continue.\n# Native memory allocation (mmap) failed to map 65536 bytes. Error detail: committing reserved memory.\n# An error report file with more information is saved as:\n# C:\\Users\\KorAtesi\\hs_err_pid4242.log\n#\n#  Out of Memory Error (os_windows.cpp:3825), pid=4242, tid=1717\n#\n# JRE version: OpenJDK Runtime Environment (25.0.1+8) (build 25.0.1+8-LTS)\n";
        let d = shown(PLAYING, Some(oom), Some(1));
        assert_eq!(d.trace.sig, "native/jvm");
        assert_eq!(d.trace.fault.map(NativeFault::error), Some("oom"));
        // Memory the computer could not give is no reason to suspect an AOT cache.
        assert!(!d.trace.in_jvm());
        // A log cut short before its frame names no module.
        let cut = "#\n# A fatal error has been detected by the Java Runtime Environment:\n#\n#  EXCEPTION_ACCESS_VIOLATION (0xc0000005) at pc=0x00007ffe1b2c3d4e, pid=4242, tid=1717\n#\n";
        let d = shown(PLAYING, Some(cut), Some(1));
        assert_eq!(d.trace.sig, "native/other");
        assert_eq!(d.trace.fault.map(NativeFault::error), Some("av"));
        // Nothing fatal: the exit code.
        assert_eq!(shown(PLAYING, None, Some(1)).trace.sig, "exit/1");
    }

    #[test]
    fn what_a_fatal_errors_process_part_quotes_is_no_reason() {
        // The exceptions the game caught before (the log's events) are not why the JVM died.
        let log = fatal(AV, WILD, GC, "V  [jvm.dll+0x3e1f20]");
        let d = shown(PLAYING, Some(&log), Some(1));
        assert_eq!(d.kind, CrashKind::Unknown);
        assert_eq!(d.trace.sig, "native/jvm");
        assert!(
            d.suggested_actions
                .contains(&SuggestedAction::OpenCrashReport)
        );
    }

    #[test]
    fn a_crash_report_before_the_hang_says_what_happened() {
        let stack = "java.util.ConcurrentModificationException\n\
            \tat java.base/java.util.ArrayList$Itr.checkForComodification(ArrayList.java:1096)\n\
            \tat java.base/java.util.ArrayList$Itr.next(ArrayList.java:1050)\n\
            \tat net.minecraft.client.multiplayer.ClientLevel.tickEntities(ClientLevel.java:300)";
        let crash = report("Unexpected error", stack);
        let d = shown(PLAYING, Some(&crash), Some(NOT_RESPONDING_EXIT));
        assert_eq!(d.kind, CrashKind::Unknown);
        assert_eq!(d.trace.sig, "java/ConcurrentModificationException/none");
        assert_eq!(d.trace.phase, Some("Unexpected error"));
        // A mod in the report's stack is named, as without the hang.
        let stack = "java.util.ConcurrentModificationException\n\
            \tat java.base/java.util.ArrayList$Itr.checkForComodification(ArrayList.java:1096)\n\
            \tat net.caffeinemc.mods.sodium.client.render.SodiumWorldRenderer.tick(SodiumWorldRenderer.java:88)";
        let crash = report("Unexpected error", stack);
        let d = shown(PLAYING, Some(&crash), Some(NOT_RESPONDING_EXIT));
        assert_eq!(d.kind, CrashKind::ModCrash);
        assert_eq!(d.trace.sig, "java/ConcurrentModificationException/sodium");
        // The server's watchdog reports a freeze: that stays one.
        let stack = "java.lang.Error: Watchdog\n\
            \tat net.minecraft.server.MinecraftServer.tickServer(MinecraftServer.java:1)";
        let crash = report("Watching Server", stack);
        let d = shown(PLAYING, Some(&crash), Some(NOT_RESPONDING_EXIT));
        assert_eq!(d.kind, CrashKind::NotResponding);
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

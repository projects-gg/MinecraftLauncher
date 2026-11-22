//! A game crash trace's detail (TELEM-JVM): when the game failed (while it opened or while it was
//! played), whether Java's AOT cache was in its start (AOT-CACHE) and, for a JVM fatal error, what
//! the JVM reported and the kind of thread it hit. Closed lists only, as the trace's last part:
//! `native/jvm/play.aot.av.gc`, `exit/hung/play.std`. A `java/…` trace has no room left (its
//! exception and its mod) and keeps its shape. The service checks the same lists
//! (`services/studio-team`'s `wire.ts`).

/// Java's AOT cache in a game's start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AotUse {
    /// A normal start.
    #[default]
    Off,
    /// The start used a cache.
    Use,
    /// The start recorded one.
    Train,
}

impl AotUse {
    pub fn key(self) -> &'static str {
        match self {
            Self::Off => "std",
            Self::Use => "aot",
            Self::Train => "rec",
        }
    }
}

/// What a JVM fatal error log reports (`pl_mc`'s `FAULT_ERRORS`).
pub const ERRORS: &[&str] = &[
    "av", "av-null", "stack", "ill", "inpage", "internal", "oom", "other",
];
/// The kind of thread a JVM fatal error hit (`pl_mc`'s `FAULT_THREADS`).
pub const THREADS: &[&str] = &[
    "jit", "gc", "vm", "render", "server", "worker", "java", "native", "other",
];

/// `sig` with its detail (see the module): `opening` when the game's window had not shown, `fault`
/// a JVM fatal error's error and thread. Anything outside the lists is `other`.
pub fn detailed(sig: &str, opening: bool, aot: AotUse, fault: Option<(&str, &str)>) -> String {
    let mut parts = sig.split('/');
    let source = parts.next().unwrap_or_default();
    if parts.count() != 1 || !matches!(source, "native" | "exit") {
        return sig.to_owned();
    }
    let when = if opening { "open" } else { "play" };
    let mut out = format!("{sig}/{when}.{}", aot.key());
    if source == "native" {
        let (error, thread) = fault.unwrap_or(("other", "other"));
        let known = |list: &[&'static str], token: &str| -> &'static str {
            list.iter()
                .find(|known| **known == token)
                .copied()
                .unwrap_or("other")
        };
        out.push('.');
        out.push_str(known(ERRORS, error));
        out.push('.');
        out.push_str(known(THREADS, thread));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_and_exit_traces_carry_their_detail() {
        assert_eq!(
            detailed("native/jvm", false, AotUse::Use, Some(("av", "gc"))),
            "native/jvm/play.aot.av.gc"
        );
        assert_eq!(
            detailed(
                "native/gpu-amd",
                true,
                AotUse::Off,
                Some(("av-null", "native"))
            ),
            "native/gpu-amd/open.std.av-null.native"
        );
        assert_eq!(
            detailed("native/other", false, AotUse::Train, None),
            "native/other/play.rec.other.other"
        );
        assert_eq!(
            detailed("exit/hung", false, AotUse::Off, None),
            "exit/hung/play.std"
        );
        assert_eq!(
            detailed("exit/2", true, AotUse::Use, None),
            "exit/2/open.aot"
        );
    }

    #[test]
    fn a_java_trace_and_other_shapes_stay_as_they_are() {
        for sig in [
            "java/NullPointerException/sodium",
            "native/jvm/play.aot.av.gc",
            "C:/Users/KorAtesi/x.dll",
            "other/x",
            "native",
        ] {
            assert_eq!(detailed(sig, false, AotUse::Use, Some(("av", "gc"))), sig);
        }
    }

    #[test]
    fn tokens_outside_the_lists_are_other() {
        assert_eq!(
            detailed(
                "native/jvm",
                false,
                AotUse::Off,
                Some(("KorAtesi", "Render thread"))
            ),
            "native/jvm/play.std.other.other"
        );
    }

    #[test]
    fn every_detail_passes_the_trace_shape() {
        for error in ERRORS {
            for thread in THREADS {
                for aot in [AotUse::Off, AotUse::Use, AotUse::Train] {
                    for opening in [false, true] {
                        let sig = detailed(
                            "native/overlay-overwolf",
                            opening,
                            aot,
                            Some((error, thread)),
                        );
                        assert_eq!(
                            crate::scrub::trace(&sig).as_deref(),
                            Some(sig.as_str()),
                            "{sig}"
                        );
                    }
                }
            }
        }
    }
}

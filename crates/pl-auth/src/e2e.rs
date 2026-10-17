//! A guard for end-to-end test runs on a developer's machine: with
//! `PROJECTS_E2E_NO_MICROSOFT=1` a debug build never signs a real Microsoft account in (no
//! browser sign-in, no import of the old launcher's premium session), whatever an automated flow
//! clicks. Release builds never read the variable.

/// The environment variable that turns the guard on (value `1`).
pub const NO_MICROSOFT_ENV: &str = "PROJECTS_E2E_NO_MICROSOFT";

/// True when Microsoft sign-ins must be refused: a debug build with `PROJECTS_E2E_NO_MICROSOFT=1`.
/// Always false in release builds.
pub fn microsoft_blocked() -> bool {
    #[cfg(debug_assertions)]
    {
        blocked_by(std::env::var(NO_MICROSOFT_ENV).ok().as_deref())
    }
    #[cfg(not(debug_assertions))]
    {
        false
    }
}

/// Whether a value of the variable turns the guard on.
#[cfg(any(debug_assertions, test))]
fn blocked_by(value: Option<&str>) -> bool {
    value.is_some_and(|value| value.trim() == "1")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_one_turns_the_guard_on() {
        assert!(blocked_by(Some("1")));
        assert!(blocked_by(Some(" 1 ")));
        assert!(!blocked_by(None));
        assert!(!blocked_by(Some("")));
        assert!(!blocked_by(Some("0")));
        assert!(!blocked_by(Some("true")));
    }

    #[test]
    fn the_name_is_stable() {
        // The e2e protocol (docs) and run scripts set this exact name.
        assert_eq!(NO_MICROSOFT_ENV, "PROJECTS_E2E_NO_MICROSOFT");
    }
}

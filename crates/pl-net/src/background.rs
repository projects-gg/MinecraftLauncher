//! When downloads nobody asked for yet may run: the launcher's background preparation.
//!
//! Background downloads use only an unmetered connection, wait while battery saver is on, while a
//! full-screen app is in front and while anything the player started runs, and come back by
//! themselves. The rules are plain data so they are tested here: [`Conditions::hold`] says whether a
//! run may start (or has to stop) now and [`Schedule`] says when the next one is due. The system's
//! readings ([`network_hint`], [`power_saver`], [`full_screen`]) and change notifications
//! ([`watch_network`], [`watch_power`]) come from Windows; what cannot be read counts against
//! running: a connection of unknown cost is a metered one.
//!
//! Nothing here polls. A run is due only after an event ([`Trigger`]) or, for a hold that ends
//! without one (another job, a full-screen app), after a check-again delay that grows each time.

use std::time::{Duration, Instant};

/// How far the connection reaches (Windows' connectivity level hint).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Connectivity {
    /// The system does not say (its connectivity check is turned off): the cost decides.
    Unknown,
    /// No network.
    None,
    /// A local network without internet.
    Local,
    /// Internet access.
    Internet,
    /// Internet behind a sign-in page (a café or hotel network): nothing would download.
    Constrained,
}

/// What the connection costs (Windows' connection cost hint).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cost {
    /// The system does not say: counts as metered.
    Unknown,
    /// No limit (home broadband, most Wi-Fi).
    Unrestricted,
    /// A data allowance (a network set as metered, a phone's hotspot).
    Fixed,
    /// Paid by the byte (mobile data).
    Variable,
}

/// The system's view of the current connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetworkHint {
    pub connectivity: Connectivity,
    pub cost: Cost,
    pub roaming: bool,
    pub approaching_limit: bool,
    pub over_limit: bool,
}

impl NetworkHint {
    /// From the fields of Windows' `NL_NETWORK_CONNECTIVITY_HINT`: the level and the cost numbered
    /// as in `NL_NETWORK_CONNECTIVITY_LEVEL_HINT` and `NL_NETWORK_CONNECTIVITY_COST_HINT`.
    pub fn from_windows(
        level: i32,
        cost: i32,
        approaching_limit: bool,
        over_limit: bool,
        roaming: bool,
    ) -> Self {
        let connectivity = match level {
            1 => Connectivity::None,
            2 => Connectivity::Local,
            3 => Connectivity::Internet,
            4 => Connectivity::Constrained,
            // 0 unknown, 5 hidden.
            _ => Connectivity::Unknown,
        };
        let cost = match cost {
            1 => Cost::Unrestricted,
            2 => Cost::Fixed,
            3 => Cost::Variable,
            _ => Cost::Unknown,
        };
        Self {
            connectivity,
            cost,
            roaming,
            approaching_limit,
            over_limit,
        }
    }

    /// An unmetered connection to the internet, as debug builds stand it in for tests.
    pub fn unmetered_internet() -> Self {
        Self::from_windows(3, 1, false, false, false)
    }

    /// The internet is (probably) reachable.
    pub fn online(&self) -> bool {
        matches!(
            self.connectivity,
            Connectivity::Internet | Connectivity::Unknown
        )
    }

    /// No data allowance, no roaming and no limit in sight.
    pub fn unmetered(&self) -> bool {
        self.cost == Cost::Unrestricted
            && !self.roaming
            && !self.approaching_limit
            && !self.over_limit
    }
}

/// The environment variable debug builds read instead of the connection's cost: `1` metered, `0`
/// unmetered (also where the system cannot tell).
pub const METERED_ENV: &str = "PROJECTS_E2E_METERED";

/// The connection debug builds use instead of the real one ([`METERED_ENV`]). Always `None` in
/// release builds.
pub fn network_override() -> Option<NetworkHint> {
    #[cfg(debug_assertions)]
    {
        parse_metered(std::env::var(METERED_ENV).ok().as_deref())
    }
    #[cfg(not(debug_assertions))]
    {
        None
    }
}

#[cfg(any(debug_assertions, test))]
fn parse_metered(value: Option<&str>) -> Option<NetworkHint> {
    match value?.trim() {
        "1" => Some(NetworkHint {
            cost: Cost::Fixed,
            ..NetworkHint::unmetered_internet()
        }),
        "0" => Some(NetworkHint::unmetered_internet()),
        _ => None,
    }
}

/// Why background downloads wait.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hold {
    /// Turned off in the settings, or nothing is chosen to prepare.
    Off,
    /// A game runs or is being started.
    Game,
    /// Something the player started runs: an install, a backup, a data move, a Java update.
    Busy,
    /// A full-screen app (a game, a video, a presentation) is in front.
    FullScreen,
    /// Battery saver is on.
    PowerSaver,
    /// No internet.
    Offline,
    /// A metered connection, or one whose cost the system does not tell.
    Metered,
    /// The disk would get too full.
    Disk,
}

impl Hold {
    /// Holds that end without an event the launcher hears about: checked again after a while.
    pub fn checks_again(self) -> bool {
        matches!(self, Hold::Busy | Hold::FullScreen)
    }
}

/// What the launcher and the system say right now.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Conditions {
    /// The setting is on and an instance is chosen.
    pub enabled: bool,
    /// A game runs or a launch is being prepared.
    pub game: bool,
    /// Another job runs (installs, backups, data moves, Java updates).
    pub busy: bool,
    /// A full-screen app is in front (`false` when unknown).
    pub full_screen: bool,
    /// Battery saver; `None` when the system does not tell (nothing waits for it then).
    pub power_saver: Option<bool>,
    /// The connection; `None` when the system does not tell (counts as metered).
    pub network: Option<NetworkHint>,
}

impl Conditions {
    /// The first reason to wait, or `None` when a background run may go ahead.
    pub fn hold(&self) -> Option<Hold> {
        if !self.enabled {
            return Some(Hold::Off);
        }
        if self.game {
            return Some(Hold::Game);
        }
        if self.busy {
            return Some(Hold::Busy);
        }
        if self.full_screen {
            return Some(Hold::FullScreen);
        }
        if self.power_saver == Some(true) {
            return Some(Hold::PowerSaver);
        }
        match self.network {
            None => Some(Hold::Metered),
            Some(network) if !network.online() => Some(Hold::Offline),
            Some(network) if !network.unmetered() => Some(Hold::Metered),
            Some(_) => None,
        }
    }
}

/// What makes a background run due.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// The launcher started: once things settled down.
    Started,
    /// Another instance was chosen, its version changed or the setting was turned on.
    Chosen,
    /// New content arrived (a pack update).
    Content,
    /// A launch ended without a game (an error, a cancel, a question to the player).
    LaunchEnded,
    /// The game closed.
    GameExited,
    /// The connection or battery saver changed.
    System,
}

impl Trigger {
    /// How long after the event the run starts (the player may still be busy with the launcher).
    pub fn delay(self) -> Duration {
        match self {
            Trigger::Started => Duration::from_secs(45),
            Trigger::Chosen => Duration::from_secs(3),
            Trigger::Content => Duration::from_secs(5),
            Trigger::LaunchEnded => Duration::from_secs(30),
            Trigger::GameExited => Duration::from_secs(60),
            Trigger::System => Duration::from_secs(10),
        }
    }
}

/// How a run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Everything the next launch needs is in place.
    Ready,
    /// It waited, or stopped half-way, for this reason (finished files stay).
    Held(Hold),
    /// Stopped for the player's own launch or a change of the setting: what comes next is
    /// scheduled by that event.
    Stopped,
    /// Something went wrong (a server, a file): tried again a few times, then left until the next
    /// event.
    Failed,
}

/// The first check-again delay for a hold without an event; it doubles each time...
pub const CHECK_AGAIN_FIRST: Duration = Duration::from_secs(60);
/// ...up to this.
pub const CHECK_AGAIN_MAX: Duration = Duration::from_secs(15 * 60);
/// The first retry after a failed run; it doubles each time...
pub const RETRY_FIRST: Duration = Duration::from_secs(5 * 60);
/// ...this many times, then the next event decides.
pub const RETRIES: u32 = 3;

/// When the next background run is due. Without anything due there is no timer at all: the
/// launcher waits for an event.
#[derive(Debug, Clone, Default)]
pub struct Schedule {
    due: Option<Instant>,
    checks: u32,
    failures: u32,
}

impl Schedule {
    /// The run that is due next, if any.
    pub fn due(&self) -> Option<Instant> {
        self.due
    }

    /// An event: a run becomes due after its delay (an earlier due run stays earlier). The
    /// player's own choices and new content start the retry count again.
    pub fn trigger(&mut self, trigger: Trigger, now: Instant) {
        if matches!(
            trigger,
            Trigger::Started | Trigger::Chosen | Trigger::Content
        ) {
            self.failures = 0;
        }
        self.due_at(now + trigger.delay());
    }

    /// The due run starts now.
    pub fn start(&mut self) {
        self.due = None;
    }

    /// A run ended: what follows from how.
    pub fn finish(&mut self, outcome: Outcome, now: Instant) {
        match outcome {
            Outcome::Ready => {
                self.checks = 0;
                self.failures = 0;
            }
            Outcome::Held(hold) if hold.checks_again() => {
                let wait = doubled(CHECK_AGAIN_FIRST, self.checks).min(CHECK_AGAIN_MAX);
                self.checks = self.checks.saturating_add(1);
                self.due_at(now + wait);
            }
            Outcome::Held(_) | Outcome::Stopped => self.checks = 0,
            Outcome::Failed => {
                self.checks = 0;
                if self.failures < RETRIES {
                    let wait = doubled(RETRY_FIRST, self.failures);
                    self.failures += 1;
                    self.due_at(now + wait);
                }
            }
        }
    }

    fn due_at(&mut self, at: Instant) {
        self.due = Some(self.due.map_or(at, |due| due.min(at)));
    }
}

fn doubled(first: Duration, times: u32) -> Duration {
    first.saturating_mul(1u32.checked_shl(times.min(16)).unwrap_or(u32::MAX))
}

// --- what the system says ------------------------------------------------------------------------

/// The current connection; `None` when the system cannot tell (Windows before 10 version 2004,
/// other systems), which counts as metered. Debug builds take [`network_override`] first.
pub fn network_hint() -> Option<NetworkHint> {
    network_override().or_else(system::network_hint)
}

/// Calls `on_change` from a system thread whenever the connection changes, for the life of the
/// process (one watcher per process; it must not block or panic). `false` when the system cannot
/// tell.
pub fn watch_network(on_change: impl Fn() + Send + Sync + 'static) -> bool {
    system::watch_network(Box::new(on_change))
}

/// Battery saver (energy saver); `None` when the system does not tell.
pub fn power_saver() -> Option<bool> {
    system::power_saver()
}

/// Calls `on_change` from a system thread whenever battery saver turns on or off (and once right
/// away), for the life of the process (one watcher per process; it must not block or panic).
/// `false` when the system cannot tell.
pub fn watch_power(on_change: impl Fn() + Send + Sync + 'static) -> bool {
    system::watch_power(Box::new(on_change))
}

/// A full-screen app (a game, a video, a presentation) is in front; `false` when unknown.
pub fn full_screen() -> bool {
    system::full_screen()
}

type Watcher = Box<dyn Fn() + Send + Sync>;

#[cfg(windows)]
mod system {
    use std::ffi::c_void;
    use std::sync::OnceLock;

    use windows::Win32::Foundation::{ERROR_SUCCESS, HANDLE, HMODULE};
    use windows::Win32::System::LibraryLoader::{
        GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
    };
    use windows::Win32::System::Power::{
        DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS, GetSystemPowerStatus, PowerSettingRegisterNotification,
        SYSTEM_POWER_STATUS,
    };
    use windows::Win32::System::SystemServices::GUID_POWER_SAVING_STATUS;
    use windows::Win32::UI::Shell::{
        QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN,
        SHQueryUserNotificationState,
    };
    use windows::Win32::UI::WindowsAndMessaging::{DEVICE_NOTIFY_CALLBACK, PBT_POWERSETTINGCHANGE};
    use windows::core::{s, w};

    use super::{NetworkHint, Watcher};

    /// `NL_NETWORK_CONNECTIVITY_HINT` (netioapi.h).
    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct ConnectivityHint {
        level: i32,
        cost: i32,
        approaching_data_limit: u8,
        over_data_limit: u8,
        roaming: u8,
    }

    impl ConnectivityHint {
        fn read(self) -> NetworkHint {
            NetworkHint::from_windows(
                self.level,
                self.cost,
                self.approaching_data_limit != 0,
                self.over_data_limit != 0,
                self.roaming != 0,
            )
        }
    }

    /// What `GetProcAddress` hands out.
    type Proc = unsafe extern "system" fn() -> isize;
    type GetHint = unsafe extern "system" fn(*mut ConnectivityHint) -> u32;
    type HintChanged = unsafe extern "system" fn(*const c_void, ConnectivityHint);
    type NotifyHint = unsafe extern "system" fn(HintChanged, *const c_void, u8, *mut HANDLE) -> u32;

    /// iphlpapi.dll from System32, loaded once. Its connectivity hint functions exist since
    /// Windows 10 version 2004, so they are looked up rather than linked (an older Windows would
    /// not start the launcher otherwise).
    fn iphlpapi() -> Option<HMODULE> {
        static MODULE: OnceLock<Option<usize>> = OnceLock::new();
        let module = MODULE.get_or_init(|| {
            // SAFETY: a plain library load from System32 by a constant name.
            unsafe { LoadLibraryExW(w!("iphlpapi.dll"), None, LOAD_LIBRARY_SEARCH_SYSTEM32) }
                .ok()
                .map(|module| module.0 as usize)
        });
        module.map(|module| HMODULE(module as *mut c_void))
    }

    pub(super) fn network_hint() -> Option<NetworkHint> {
        let module = iphlpapi()?;
        // SAFETY: a lookup by a constant name in a loaded module.
        let proc: Proc = unsafe { GetProcAddress(module, s!("GetNetworkConnectivityHint")) }?;
        // SAFETY: the symbol has the documented `GetNetworkConnectivityHint` signature.
        let get = unsafe { std::mem::transmute::<Proc, GetHint>(proc) };
        let mut hint = ConnectivityHint::default();
        // SAFETY: `hint` is a valid `NL_NETWORK_CONNECTIVITY_HINT` to write into.
        (unsafe { get(&mut hint) } == 0).then(|| hint.read())
    }

    static NETWORK_CHANGED: OnceLock<Watcher> = OnceLock::new();

    unsafe extern "system" fn network_changed(_context: *const c_void, _hint: ConnectivityHint) {
        if let Some(on_change) = NETWORK_CHANGED.get() {
            on_change();
        }
    }

    pub(super) fn watch_network(on_change: Watcher) -> bool {
        let Some(module) = iphlpapi() else {
            return false;
        };
        // SAFETY: a lookup by a constant name in a loaded module.
        let Some(proc) =
            (unsafe { GetProcAddress(module, s!("NotifyNetworkConnectivityHintChange")) })
        else {
            return false;
        };
        if NETWORK_CHANGED.set(on_change).is_err() {
            return false;
        }
        // SAFETY: the symbol has the documented `NotifyNetworkConnectivityHintChange` signature.
        let notify = unsafe { std::mem::transmute::<Proc, NotifyHint>(proc) };
        let mut handle = HANDLE::default();
        // Registered for the life of the process (never cancelled); no first call right away.
        // SAFETY: `network_changed` matches `PNETWORK_CONNECTIVITY_HINT_CHANGE_CALLBACK`.
        unsafe { notify(network_changed, std::ptr::null(), 0, &mut handle) == 0 }
    }

    pub(super) fn power_saver() -> Option<bool> {
        let mut status = SYSTEM_POWER_STATUS::default();
        // SAFETY: `status` is a valid `SYSTEM_POWER_STATUS` to write into.
        unsafe { GetSystemPowerStatus(&mut status) }.ok()?;
        // `SystemStatusFlag` 1: battery saver is on.
        Some(status.SystemStatusFlag == 1)
    }

    static POWER_CHANGED: OnceLock<Watcher> = OnceLock::new();

    unsafe extern "system" fn power_changed(
        _context: *const c_void,
        kind: u32,
        _setting: *const c_void,
    ) -> u32 {
        if kind == PBT_POWERSETTINGCHANGE
            && let Some(on_change) = POWER_CHANGED.get()
        {
            on_change();
        }
        0
    }

    pub(super) fn watch_power(on_change: Watcher) -> bool {
        if POWER_CHANGED.set(on_change).is_err() {
            return false;
        }
        // The parameters live as long as the registration: the whole process.
        let params: &'static DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS =
            Box::leak(Box::new(DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS {
                Callback: Some(power_changed),
                Context: std::ptr::null_mut(),
            }));
        let mut registration = std::ptr::null_mut();
        // SAFETY: with `DEVICE_NOTIFY_CALLBACK` the recipient is a pointer to the (static)
        // subscription parameters.
        let result = unsafe {
            PowerSettingRegisterNotification(
                &GUID_POWER_SAVING_STATUS,
                DEVICE_NOTIFY_CALLBACK,
                HANDLE(std::ptr::from_ref(params).cast_mut().cast()),
                &mut registration,
            )
        };
        result == ERROR_SUCCESS
    }

    pub(super) fn full_screen() -> bool {
        // SAFETY: no arguments; the state is returned.
        matches!(
            unsafe { SHQueryUserNotificationState() },
            Ok(state) if state == QUNS_BUSY
                || state == QUNS_RUNNING_D3D_FULL_SCREEN
                || state == QUNS_PRESENTATION_MODE
        )
    }
}

#[cfg(not(windows))]
mod system {
    use super::{NetworkHint, Watcher};

    pub(super) fn network_hint() -> Option<NetworkHint> {
        None
    }

    pub(super) fn watch_network(_on_change: Watcher) -> bool {
        false
    }

    pub(super) fn power_saver() -> Option<bool> {
        None
    }

    pub(super) fn watch_power(_on_change: Watcher) -> bool {
        false
    }

    pub(super) fn full_screen() -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clear() -> Conditions {
        Conditions {
            enabled: true,
            network: Some(NetworkHint::unmetered_internet()),
            ..Conditions::default()
        }
    }

    #[test]
    fn windows_hints_map_to_levels_and_costs() {
        let hint = NetworkHint::from_windows(3, 1, false, false, false);
        assert_eq!(hint.connectivity, Connectivity::Internet);
        assert_eq!(hint.cost, Cost::Unrestricted);
        assert!(hint.online() && hint.unmetered());
        assert_eq!(
            NetworkHint::from_windows(5, 1, false, false, false).connectivity,
            Connectivity::Unknown,
            "a hidden level tells nothing"
        );
        assert_eq!(
            NetworkHint::from_windows(4, 1, false, false, false).connectivity,
            Connectivity::Constrained
        );
        assert_eq!(
            NetworkHint::from_windows(3, 2, false, false, false).cost,
            Cost::Fixed
        );
        assert_eq!(
            NetworkHint::from_windows(3, 3, false, false, false).cost,
            Cost::Variable
        );
        assert_eq!(
            NetworkHint::from_windows(3, 9, false, false, false).cost,
            Cost::Unknown
        );
    }

    #[test]
    fn only_an_unrestricted_connection_without_limits_is_unmetered() {
        let base = NetworkHint::unmetered_internet();
        assert!(base.unmetered());
        for cost in [Cost::Unknown, Cost::Fixed, Cost::Variable] {
            assert!(!NetworkHint { cost, ..base }.unmetered(), "{cost:?}");
        }
        assert!(
            !NetworkHint {
                roaming: true,
                ..base
            }
            .unmetered()
        );
        assert!(
            !NetworkHint {
                approaching_limit: true,
                ..base
            }
            .unmetered()
        );
        assert!(
            !NetworkHint {
                over_limit: true,
                ..base
            }
            .unmetered()
        );
    }

    #[test]
    fn offline_and_sign_in_networks_are_not_online() {
        for level in [1, 2, 4] {
            assert!(!NetworkHint::from_windows(level, 1, false, false, false).online());
        }
        assert!(NetworkHint::from_windows(0, 1, false, false, false).online());
    }

    #[test]
    fn a_clear_moment_lets_a_run_go() {
        assert_eq!(clear().hold(), None);
        // Battery saver the system cannot tell about does not stop anything.
        assert_eq!(
            Conditions {
                power_saver: Some(false),
                ..clear()
            }
            .hold(),
            None
        );
    }

    #[test]
    fn an_unknown_connection_counts_as_metered() {
        let unknown = Conditions {
            network: None,
            ..clear()
        };
        assert_eq!(unknown.hold(), Some(Hold::Metered));
        let unknown_cost = Conditions {
            network: Some(NetworkHint {
                cost: Cost::Unknown,
                ..NetworkHint::unmetered_internet()
            }),
            ..clear()
        };
        assert_eq!(unknown_cost.hold(), Some(Hold::Metered));
        let mobile = Conditions {
            network: Some(NetworkHint::from_windows(3, 3, false, false, false)),
            ..clear()
        };
        assert_eq!(mobile.hold(), Some(Hold::Metered));
        let offline = Conditions {
            network: Some(NetworkHint::from_windows(1, 0, false, false, false)),
            ..clear()
        };
        assert_eq!(offline.hold(), Some(Hold::Offline));
    }

    #[test]
    fn holds_come_in_order() {
        let everything = Conditions {
            enabled: false,
            game: true,
            busy: true,
            full_screen: true,
            power_saver: Some(true),
            network: None,
        };
        assert_eq!(everything.hold(), Some(Hold::Off));
        let on = Conditions {
            enabled: true,
            ..everything
        };
        assert_eq!(on.hold(), Some(Hold::Game));
        assert_eq!(Conditions { game: false, ..on }.hold(), Some(Hold::Busy));
        assert_eq!(
            Conditions {
                game: false,
                busy: false,
                ..on
            }
            .hold(),
            Some(Hold::FullScreen)
        );
        assert_eq!(
            Conditions {
                game: false,
                busy: false,
                full_screen: false,
                ..on
            }
            .hold(),
            Some(Hold::PowerSaver)
        );
    }

    #[test]
    fn debug_builds_stand_in_a_connection() {
        assert_eq!(parse_metered(None), None);
        assert_eq!(parse_metered(Some("yes")), None);
        assert!(parse_metered(Some("0")).is_some_and(|hint| hint.unmetered() && hint.online()));
        assert!(parse_metered(Some(" 1 ")).is_some_and(|hint| !hint.unmetered() && hint.online()));
    }

    #[test]
    fn nothing_is_due_without_an_event() {
        let schedule = Schedule::default();
        assert_eq!(schedule.due(), None);
    }

    #[test]
    fn an_event_makes_a_run_due_after_its_delay() {
        let now = Instant::now();
        let mut schedule = Schedule::default();
        schedule.trigger(Trigger::Started, now);
        assert_eq!(schedule.due(), Some(now + Duration::from_secs(45)));
        // A sooner event wins; a later one does not push the run back.
        schedule.trigger(Trigger::Chosen, now);
        assert_eq!(schedule.due(), Some(now + Duration::from_secs(3)));
        schedule.trigger(Trigger::GameExited, now);
        assert_eq!(schedule.due(), Some(now + Duration::from_secs(3)));
        schedule.start();
        assert_eq!(schedule.due(), None);
    }

    #[test]
    fn a_ready_run_leaves_nothing_due() {
        let now = Instant::now();
        let mut schedule = Schedule::default();
        schedule.trigger(Trigger::Chosen, now);
        schedule.start();
        schedule.finish(Outcome::Ready, now);
        assert_eq!(schedule.due(), None);
    }

    #[test]
    fn holds_without_an_event_are_checked_again_less_and_less_often() {
        let now = Instant::now();
        let mut schedule = Schedule::default();
        let mut waits = Vec::new();
        for _ in 0..7 {
            schedule.start();
            schedule.finish(Outcome::Held(Hold::Busy), now);
            waits.push(schedule.due().unwrap() - now);
        }
        let minutes: Vec<u64> = waits.iter().map(|wait| wait.as_secs() / 60).collect();
        assert_eq!(minutes, [1, 2, 4, 8, 15, 15, 15]);
        // Once the run went ahead the delays start small again.
        schedule.start();
        schedule.finish(Outcome::Ready, now);
        schedule.start();
        schedule.finish(Outcome::Held(Hold::FullScreen), now);
        assert_eq!(schedule.due(), Some(now + CHECK_AGAIN_FIRST));
    }

    #[test]
    fn holds_with_an_event_wait_for_it() {
        let now = Instant::now();
        for hold in [
            Hold::Off,
            Hold::Game,
            Hold::PowerSaver,
            Hold::Offline,
            Hold::Metered,
            Hold::Disk,
        ] {
            let mut schedule = Schedule::default();
            schedule.finish(Outcome::Held(hold), now);
            assert_eq!(schedule.due(), None, "{hold:?}");
        }
        let mut schedule = Schedule::default();
        schedule.finish(Outcome::Stopped, now);
        assert_eq!(schedule.due(), None);
    }

    #[test]
    fn a_failed_run_is_tried_again_a_few_times() {
        let now = Instant::now();
        let mut schedule = Schedule::default();
        let mut waits = Vec::new();
        for _ in 0..5 {
            schedule.start();
            schedule.finish(Outcome::Failed, now);
            waits.push(schedule.due().map(|due| (due - now).as_secs() / 60));
        }
        assert_eq!(waits, [Some(5), Some(10), Some(20), None, None]);
        // The player's next choice starts over.
        schedule.trigger(Trigger::Chosen, now);
        schedule.start();
        schedule.finish(Outcome::Failed, now);
        assert_eq!(schedule.due(), Some(now + RETRY_FIRST));
    }

    #[test]
    fn a_game_closing_does_not_reset_the_retries() {
        let now = Instant::now();
        let mut schedule = Schedule::default();
        for _ in 0..RETRIES {
            schedule.start();
            schedule.finish(Outcome::Failed, now);
        }
        schedule.trigger(Trigger::GameExited, now);
        schedule.start();
        schedule.finish(Outcome::Failed, now);
        assert_eq!(schedule.due(), None);
    }
}

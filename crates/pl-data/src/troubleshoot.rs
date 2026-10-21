//! The troubleshooter (Settings → Sorun yardımcısı, HELP1): for "the game won't start" and "I can't
//! connect", what each check found, as a level, a code for support and, where the launcher can do
//! it, one fix.
//!
//! The rules live here. The launcher gathers the facts (short probes, the server ping, free
//! space, Java, the mod check, the last crash) and feeds them in as they come ([`Assembler`]), so
//! every row settles on its own and none flips back and forth: a failure the internet could be
//! behind waits until the internet's own check is in, then reads either as its own problem or as
//! "needs the internet".
//!
//! Nothing here carries personal data. Findings hold levels, codes, numbers, versions and mod
//! names, never account names, ids, paths, addresses or tokens, so the support summary built from
//! them alone cannot leak any ([`safe_name`] keeps a mod name that looks like a path out).

use std::collections::BTreeMap;
use std::time::Duration;

use jiff::{SignedDuration, Timestamp};
use pl_mc::instance::{MemoryIssue, MemoryValidation};
use pl_mc::launch::CrashKind;
use pl_mc::server::PingFailure;
use pl_net::background::Connectivity;
use pl_net::{Probe, Reach};
use serde::{Deserialize, Serialize};

/// One row of the troubleshooter, in the order the rows are shown.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "camelCase")]
pub enum Check {
    Internet,
    /// The Projects Minecraft server (server list ping).
    Server,
    /// The signed content (packs, news, the launcher's settings from Studio).
    Content,
    /// The friends service, while the feature is on.
    Friends,
    /// Microsoft's sign-in and Minecraft services, when a Microsoft account is added.
    Microsoft,
    /// Discord (friends sign-in, the support channel), while the friends feature is on.
    Discord,
    /// This computer's clock; a row only when it is wrong.
    Clock,
    /// Free space on the data folder's disk.
    Disk,
    /// The Java the instance Play starts would run with.
    Java,
    /// That instance's memory; a row only when it is too high.
    Memory,
    /// That instance's mod check.
    Mods,
    /// That instance's last crash.
    Crash,
}

/// How a check went. The calm states first: only `Warning` and `Problem` count as findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Level {
    Ok,
    /// Worth knowing, nothing to fix (a planned maintenance).
    Note,
    Warning,
    Problem,
    /// Could not be judged (no internet for it, no instance, the check failed).
    Skipped,
}

impl Level {
    /// Higher is worse; the headline leads with the worst.
    pub fn severity(self) -> u8 {
        match self {
            Self::Problem => 4,
            Self::Warning => 3,
            Self::Note => 2,
            Self::Ok => 1,
            Self::Skipped => 0,
        }
    }
}

/// What a check found, stable for support (the summary writes it as `SERVER-UNREACHABLE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Code {
    InternetOk,
    InternetSlow,
    /// The system has no network at all.
    InternetOffline,
    /// A network without internet (a router that lost its line).
    InternetLocal,
    /// A network behind a sign-in page (a café, a school, a hotel).
    InternetSignIn,
    /// No address resolves (DNS).
    InternetNoDns,
    /// Secure connections fail (the clock, security software or a proxy that reads HTTPS).
    InternetInsecure,
    InternetDown,
    ServerOk,
    ServerSlow,
    /// The server refused the connection.
    ServerDown,
    /// The server's name does not resolve.
    ServerNotFound,
    ServerUnreachable,
    ServerNeedsInternet,
    ContentOk,
    /// Out of reach; the launcher goes on with what it saved.
    ContentUnreachable,
    /// Out of reach with nothing saved yet.
    ContentMissing,
    ContentNeedsInternet,
    FriendsOk,
    /// The service answered with an error.
    FriendsDown,
    FriendsUnreachable,
    FriendsNeedsInternet,
    MicrosoftOk,
    MicrosoftDown,
    MicrosoftUnreachable,
    MicrosoftNeedsInternet,
    DiscordOk,
    DiscordUnreachable,
    DiscordNeedsInternet,
    ClockWrong,
    DiskOk,
    DiskLow,
    DiskFull,
    DiskUnknown,
    /// "Otomatik": the launcher's runtime is installed.
    JavaReady,
    /// "Otomatik": the runtime comes with the first start.
    JavaDownloads,
    JavaCustomOk,
    /// A 32-bit Java on a 64-bit computer: the game gets little memory.
    JavaX86,
    JavaMissing,
    JavaBroken,
    JavaTooOld,
    JavaTooNew,
    JavaUnknown,
    MemoryTooHigh,
    MemoryHigh,
    ModsOk,
    ModsWarning,
    ModsBlocking,
    ModsUnknown,
    CrashNone,
    /// The game ran fine after its last crash.
    CrashResolved,
    CrashRecent,
    /// The check could not run (an error, or its time ran out).
    CheckFailed,
}

/// The one-tap fix a row offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Fix {
    /// The cleanup (Settings → Veri).
    MakeSpace,
    /// Switch the instance to a Java that fits and play (the Java prompt's "Kur ve oyna").
    InstallJava,
    /// The instance's settings tab.
    InstanceSettings,
    /// The mod repair ("Onar").
    Repair,
    /// The mod check's findings.
    ShowMods,
    /// The crash dialog.
    CrashDetails,
    /// The instance's memory back on "Otomatik".
    MemoryAuto,
    /// The system's date and time settings.
    ClockSettings,
    /// The status page of an announcement.
    StatusPage,
}

/// Numbers and names a row's words use. Never personal data.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Facts {
    pub latency_ms: Option<u32>,
    pub players: Option<u32>,
    pub free_bytes: Option<u64>,
    /// A Java major version (the one there is).
    pub major: Option<u32>,
    /// The Java the game needs (the lowest, or the highest for a Java that is too new).
    pub need: Option<u32>,
    /// Mod issues, or mods.
    pub count: Option<u32>,
    /// How far the clock is off (positive: ahead).
    pub minutes: Option<i64>,
    /// When the last crash happened.
    pub at: Option<Timestamp>,
    pub crash: Option<CrashKind>,
    /// The mod a crash points at (its display name).
    pub name: Option<String>,
}

/// What one check found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub check: Check,
    pub level: Level,
    pub code: Code,
    pub fix: Option<Fix>,
    pub facts: Facts,
}

impl Finding {
    pub fn new(check: Check, level: Level, code: Code) -> Self {
        Self {
            check,
            level,
            code,
            fix: None,
            facts: Facts::default(),
        }
    }

    fn fix(mut self, fix: Fix) -> Self {
        self.fix = Some(fix);
        self
    }

    fn facts(mut self, fill: impl FnOnce(&mut Facts)) -> Self {
        fill(&mut self.facts);
        self
    }
}

/// The quickest well-known address answering slower than this: a slow connection.
pub const SLOW_INTERNET: Duration = Duration::from_secs(2);
/// A server ping from this many milliseconds on is high: Minecraft's bars drop to three.
pub const HIGH_PING_MS: u32 = 300;
/// Below this much free space a download, a first start or a world save may not fit.
pub const DISK_FULL_BELOW: u64 = 1024 * 1024 * 1024;
/// Below this much free space the launcher stops preparing games in the background.
pub const DISK_LOW_BELOW: u64 = pl_mc::install::space::BACKGROUND_RESERVE;
/// A clock this far off breaks secure connections and Microsoft sign-in.
pub const CLOCK_TOLERANCE_SECS: i64 = 5 * 60;
/// A crash older than this is history.
pub const CRASH_WINDOW: SignedDuration = SignedDuration::from_secs(7 * 24 * 60 * 60);

/// What the internet's check decided, for the checks it may be behind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Uplink {
    /// Still checking: a failure waits.
    Pending,
    Up,
    Down,
    /// The internet's check did not come to a result: every check judges by itself.
    Unknown,
}

/// A failure of a check that needs the internet: its own finding, or "needs the internet" while
/// the internet is down, or nothing yet while the internet is being checked.
fn dependent(own: Finding, needs_internet: Code, uplink: Uplink) -> Option<Finding> {
    match uplink {
        Uplink::Pending => None,
        Uplink::Down => Some(Finding::new(own.check, Level::Skipped, needs_internet)),
        Uplink::Up | Uplink::Unknown => Some(own),
    }
}

fn millis(duration: Duration) -> u32 {
    u32::try_from(duration.as_millis()).unwrap_or(u32::MAX)
}

/// The internet: any of the well-known secure addresses answering is enough. When none does,
/// the system's own view of the network says why; otherwise the way the requests failed.
/// `clock_wrong`: a secure connection that failed is the clock's fault and its fix applies.
pub fn internet(os: Option<Connectivity>, secure: &[Probe], clock_wrong: bool) -> Finding {
    let fastest = secure
        .iter()
        .filter(|p| p.reach.answered())
        .map(|p| p.elapsed)
        .min();
    if let Some(fastest) = fastest {
        let (level, code) = if fastest > SLOW_INTERNET {
            (Level::Warning, Code::InternetSlow)
        } else {
            (Level::Ok, Code::InternetOk)
        };
        return Finding::new(Check::Internet, level, code)
            .facts(|f| f.latency_ms = Some(millis(fastest)));
    }
    let all = |reach: Reach| !secure.is_empty() && secure.iter().all(|p| p.reach == reach);
    let code = match os {
        Some(Connectivity::None) => Code::InternetOffline,
        Some(Connectivity::Local) => Code::InternetLocal,
        Some(Connectivity::Constrained) => Code::InternetSignIn,
        _ if all(Reach::UnknownHost) => Code::InternetNoDns,
        _ if secure.iter().any(|p| p.reach == Reach::Insecure) => Code::InternetInsecure,
        _ => Code::InternetDown,
    };
    let finding = Finding::new(Check::Internet, Level::Problem, code);
    if code == Code::InternetInsecure && clock_wrong {
        finding.fix(Fix::ClockSettings)
    } else {
        finding
    }
}

/// The clock, from the dates servers sent (the middle one, so one odd server does not decide):
/// a finding only when it is off by [`CLOCK_TOLERANCE_SECS`] or more.
pub fn clock(offsets_secs: &[i64]) -> Option<Finding> {
    let mut sorted = offsets_secs.to_vec();
    sorted.sort_unstable();
    let middle = *sorted.get(sorted.len() / 2)?;
    (middle.abs() >= CLOCK_TOLERANCE_SECS).then(|| {
        Finding::new(Check::Clock, Level::Problem, Code::ClockWrong)
            .fix(Fix::ClockSettings)
            .facts(|f| f.minutes = Some(middle / 60))
    })
}

/// How the server list ping went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerProbe {
    Online {
        latency_ms: u32,
        players: Option<u32>,
    },
    Offline(PingFailure),
}

pub fn server(probe: &ServerProbe, uplink: Uplink) -> Option<Finding> {
    match *probe {
        ServerProbe::Online {
            latency_ms,
            players,
        } => {
            let (level, code) = if latency_ms >= HIGH_PING_MS {
                (Level::Warning, Code::ServerSlow)
            } else {
                (Level::Ok, Code::ServerOk)
            };
            Some(Finding::new(Check::Server, level, code).facts(|f| {
                f.latency_ms = Some(latency_ms);
                f.players = players;
            }))
        }
        ServerProbe::Offline(failure) => {
            let code = match failure {
                PingFailure::UnknownHost | PingFailure::InvalidAddress => Code::ServerNotFound,
                PingFailure::Refused => Code::ServerDown,
                PingFailure::Timeout | PingFailure::Protocol | PingFailure::Io => {
                    Code::ServerUnreachable
                }
            };
            dependent(
                Finding::new(Check::Server, Level::Problem, code),
                Code::ServerNeedsInternet,
                uplink,
            )
        }
    }
}

/// How the content mirrors answered, and whether anything is saved from before.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentProbe {
    pub mirrors: Vec<Reach>,
    pub cached: bool,
}

pub fn content(probe: &ContentProbe, uplink: Uplink) -> Option<Finding> {
    if probe.mirrors.iter().any(|r| r.succeeded()) {
        return Some(Finding::new(Check::Content, Level::Ok, Code::ContentOk));
    }
    let own = if probe.cached {
        Finding::new(Check::Content, Level::Warning, Code::ContentUnreachable)
    } else {
        Finding::new(Check::Content, Level::Problem, Code::ContentMissing)
    };
    dependent(own, Code::ContentNeedsInternet, uplink)
}

/// The friends service's health check: it answers 2xx without touching its database.
pub fn friends(probes: &[Reach], uplink: Uplink) -> Option<Finding> {
    if !probes.is_empty() && probes.iter().all(|r| r.succeeded()) {
        return Some(Finding::new(Check::Friends, Level::Ok, Code::FriendsOk));
    }
    if probes.iter().any(|r| r.answered()) {
        return Some(Finding::new(
            Check::Friends,
            Level::Warning,
            Code::FriendsDown,
        ));
    }
    dependent(
        Finding::new(Check::Friends, Level::Warning, Code::FriendsUnreachable),
        Code::FriendsNeedsInternet,
        uplink,
    )
}

/// Microsoft's sign-in chain (Microsoft, Xbox Live, Minecraft services): each has to answer
/// without an error of its own (a 404 for a bare address is still an answer).
pub fn microsoft(probes: &[Reach], uplink: Uplink) -> Option<Finding> {
    if !probes.is_empty() && probes.iter().all(|r| r.healthy()) {
        return Some(Finding::new(Check::Microsoft, Level::Ok, Code::MicrosoftOk));
    }
    if probes.iter().any(|r| r.answered() && !r.healthy()) {
        return Some(Finding::new(
            Check::Microsoft,
            Level::Warning,
            Code::MicrosoftDown,
        ));
    }
    dependent(
        Finding::new(Check::Microsoft, Level::Warning, Code::MicrosoftUnreachable),
        Code::MicrosoftNeedsInternet,
        uplink,
    )
}

pub fn discord(probes: &[Reach], uplink: Uplink) -> Option<Finding> {
    if probes.iter().any(|r| r.healthy()) {
        return Some(Finding::new(Check::Discord, Level::Ok, Code::DiscordOk));
    }
    dependent(
        Finding::new(Check::Discord, Level::Warning, Code::DiscordUnreachable),
        Code::DiscordNeedsInternet,
        uplink,
    )
}

/// Free space on the data folder's disk.
pub fn disk(free: Option<u64>) -> Finding {
    let Some(free) = free else {
        return Finding::new(Check::Disk, Level::Skipped, Code::DiskUnknown);
    };
    let finding = if free < DISK_FULL_BELOW {
        Finding::new(Check::Disk, Level::Problem, Code::DiskFull).fix(Fix::MakeSpace)
    } else if free < DISK_LOW_BELOW {
        Finding::new(Check::Disk, Level::Warning, Code::DiskLow).fix(Fix::MakeSpace)
    } else {
        Finding::new(Check::Disk, Level::Ok, Code::DiskOk)
    };
    finding.facts(|f| f.free_bytes = Some(free))
}

/// The Java the instance would run with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JavaState {
    /// "Otomatik": the launcher's runtime for the game, installed (its major version) or not yet.
    Automatic {
        installed: Option<u32>,
    },
    /// A custom Java that is gone.
    Missing,
    /// A custom Java that does not run.
    Broken,
    /// A custom Java that fits the game and its mods.
    Fits {
        major: u32,
        bits32: bool,
    },
    TooOld {
        have: u32,
        need: u32,
    },
    TooNew {
        have: u32,
        max: u32,
    },
    /// The game's needs are unknown (its version is not installed yet), or there is no instance.
    Unknown,
}

/// `fixable`: a launcher runtime fits, so "Kur ve oyna" can switch to it.
pub fn java(state: JavaState, fixable: bool) -> Finding {
    let fix = if fixable {
        Fix::InstallJava
    } else {
        Fix::InstanceSettings
    };
    let problem = |code| Finding::new(Check::Java, Level::Problem, code).fix(fix);
    match state {
        JavaState::Automatic {
            installed: Some(major),
        } => Finding::new(Check::Java, Level::Ok, Code::JavaReady).facts(|f| f.major = Some(major)),
        JavaState::Automatic { installed: None } => {
            Finding::new(Check::Java, Level::Ok, Code::JavaDownloads)
        }
        JavaState::Missing => problem(Code::JavaMissing),
        JavaState::Broken => problem(Code::JavaBroken),
        JavaState::TooOld { have, need } => problem(Code::JavaTooOld).facts(|f| {
            f.major = Some(have);
            f.need = Some(need);
        }),
        JavaState::TooNew { have, max } => problem(Code::JavaTooNew).facts(|f| {
            f.major = Some(have);
            f.need = Some(max);
        }),
        JavaState::Fits {
            major,
            bits32: true,
        } => Finding::new(Check::Java, Level::Warning, Code::JavaX86)
            .fix(fix)
            .facts(|f| f.major = Some(major)),
        JavaState::Fits {
            major,
            bits32: false,
        } => Finding::new(Check::Java, Level::Ok, Code::JavaCustomOk)
            .facts(|f| f.major = Some(major)),
        JavaState::Unknown => Finding::new(Check::Java, Level::Skipped, Code::JavaUnknown),
    }
}

/// The instance's memory against this computer's (the settings' own rules,
/// `pl_mc::instance::validate_memory`): a finding only when it is more than the computer can
/// give, or close to it. "Otomatik" always fits.
pub fn memory(automatic: bool, validation: &MemoryValidation) -> Option<Finding> {
    if automatic {
        return None;
    }
    if validation
        .errors
        .iter()
        .any(|issue| matches!(issue, MemoryIssue::MaxTooHigh { .. }))
    {
        return Some(
            Finding::new(Check::Memory, Level::Problem, Code::MemoryTooHigh).fix(Fix::MemoryAuto),
        );
    }
    validation
        .warnings
        .iter()
        .any(|issue| matches!(issue, MemoryIssue::HighUsage { .. }))
        .then(|| Finding::new(Check::Memory, Level::Warning, Code::MemoryHigh).fix(Fix::MemoryAuto))
}

/// What the mod check of the instance found (its "info" notes do not count).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModsSummary {
    pub blocking: u32,
    pub warnings: u32,
    /// "Onar" has something to do.
    pub repairable: bool,
    /// Mod files in the instance.
    pub mods: u32,
}

/// `None`: the check could not read the mods.
pub fn mods(summary: Option<&ModsSummary>) -> Finding {
    let Some(summary) = summary else {
        return Finding::new(Check::Mods, Level::Skipped, Code::ModsUnknown);
    };
    let fix = if summary.repairable {
        Fix::Repair
    } else {
        Fix::ShowMods
    };
    if summary.blocking > 0 {
        Finding::new(Check::Mods, Level::Problem, Code::ModsBlocking)
            .fix(fix)
            .facts(|f| f.count = Some(summary.blocking))
    } else if summary.warnings > 0 {
        Finding::new(Check::Mods, Level::Warning, Code::ModsWarning)
            .fix(fix)
            .facts(|f| f.count = Some(summary.warnings))
    } else {
        Finding::new(Check::Mods, Level::Ok, Code::ModsOk).facts(|f| f.count = Some(summary.mods))
    }
}

/// The instance's last crash the launcher saw.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastCrash {
    pub at: Timestamp,
    pub kind: CrashKind,
    /// The mod it points at most strongly (display name).
    pub suspect: Option<String>,
    /// A later start reached the game and ended without a crash.
    pub resolved: bool,
}

pub fn crash(last: Option<&LastCrash>, now: Timestamp) -> Finding {
    match last {
        Some(last) if now.duration_since(last.at) <= CRASH_WINDOW => {
            if last.resolved {
                Finding::new(Check::Crash, Level::Ok, Code::CrashResolved)
            } else {
                Finding::new(Check::Crash, Level::Problem, Code::CrashRecent)
                    .fix(Fix::CrashDetails)
                    .facts(|f| {
                        f.at = Some(last.at);
                        f.crash = Some(last.kind);
                        f.name = last.suspect.as_deref().and_then(safe_name);
                    })
            }
        }
        _ => Finding::new(Check::Crash, Level::Ok, Code::CrashNone),
    }
}

/// The longest mod name a row (and the summary) carries.
const NAME_MAX_CHARS: usize = 48;

/// A mod's display name as findings may carry it: trimmed, no control characters, at most
/// [`NAME_MAX_CHARS`]. Nothing that looks like a path, an address or a mail address: a loader that
/// names a jar by its full path would otherwise put the player's folder in the summary.
pub fn safe_name(name: &str) -> Option<String> {
    let name: String = name.trim().chars().filter(|c| !c.is_control()).collect();
    if name.is_empty() || name.contains(['/', '\\', '@']) || has_address(&name) {
        return None;
    }
    Some(name.chars().take(NAME_MAX_CHARS).collect())
}

/// A drive letter (`C:`), a port (`host:25565`), an IPv6 address or an IPv4 one; a colon in a
/// title ("Create: Steam 'n' Rails") is fine.
fn has_address(name: &str) -> bool {
    let bytes = name.as_bytes();
    let drive = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    let port = bytes
        .windows(2)
        .any(|pair| pair[0] == b':' && pair[1].is_ascii_digit());
    let ipv6 = name.matches(':').count() >= 2;
    let ipv4 = name
        .split(|c: char| !c.is_ascii_digit() && c != '.')
        .any(|run| {
            let parts: Vec<&str> = run.split('.').collect();
            parts.len() == 4
                && parts
                    .iter()
                    .all(|p| (1..=3).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_digit()))
        });
    drive || port || ipv6 || ipv4
}

/// What one check found out, fed to the [`Assembler`] as it comes.
#[derive(Debug, Clone)]
pub enum Observation {
    Internet {
        /// The system's view of the network (Windows), when it has one.
        os: Option<Connectivity>,
        /// Well-known secure addresses.
        secure: Vec<Probe>,
        /// Plain HTTP addresses, only for the clock (they answer while secure connections fail).
        plain: Vec<Probe>,
    },
    Server(ServerProbe),
    Content(ContentProbe),
    Friends(Vec<Reach>),
    Microsoft(Vec<Reach>),
    Discord(Vec<Reach>),
    Disk(Option<u64>),
    Java {
        state: JavaState,
        fixable: bool,
    },
    Memory {
        automatic: bool,
        validation: MemoryValidation,
    },
    Mods(Option<ModsSummary>),
    Crash {
        last: Option<LastCrash>,
        now: Timestamp,
    },
    /// The check could not run: an error, or its time ran out.
    Failed(Check),
}

/// Puts the findings together while the checks come in, in any order. Each call returns only the
/// findings that are new or changed, in row order, so the launcher sends each row as it settles.
#[derive(Debug, Clone)]
pub struct Assembler {
    planned: Vec<Check>,
    uplink: Uplink,
    clock: Option<Finding>,
    server: Option<ServerProbe>,
    content: Option<ContentProbe>,
    friends: Option<Vec<Reach>>,
    microsoft: Option<Vec<Reach>>,
    discord: Option<Vec<Reach>>,
    settled: BTreeMap<Check, Finding>,
    shown: BTreeMap<Check, Finding>,
    finished: bool,
}

impl Assembler {
    /// `planned`: the rows shown while they are checked (the conditional ones, memory and the
    /// clock, appear only with a finding).
    pub fn new(planned: impl IntoIterator<Item = Check>) -> Self {
        let mut planned: Vec<Check> = planned.into_iter().collect();
        planned.sort_unstable();
        planned.dedup();
        Self {
            planned,
            uplink: Uplink::Pending,
            clock: None,
            server: None,
            content: None,
            friends: None,
            microsoft: None,
            discord: None,
            settled: BTreeMap::new(),
            shown: BTreeMap::new(),
            finished: false,
        }
    }

    pub fn planned(&self) -> &[Check] {
        &self.planned
    }

    pub fn observe(&mut self, observation: Observation) -> Vec<Finding> {
        match observation {
            Observation::Internet { os, secure, plain } => {
                let offsets: Vec<i64> = secure
                    .iter()
                    .chain(plain.iter())
                    .filter_map(Probe::clock_offset_secs)
                    .collect();
                self.clock = clock(&offsets);
                let finding = internet(os, &secure, self.clock.is_some());
                self.uplink = if finding.level == Level::Problem {
                    Uplink::Down
                } else {
                    Uplink::Up
                };
                self.settled.insert(Check::Internet, finding);
            }
            Observation::Server(probe) => self.server = Some(probe),
            Observation::Content(probe) => self.content = Some(probe),
            Observation::Friends(probes) => self.friends = Some(probes),
            Observation::Microsoft(probes) => self.microsoft = Some(probes),
            Observation::Discord(probes) => self.discord = Some(probes),
            Observation::Disk(free) => {
                self.settled.insert(Check::Disk, disk(free));
            }
            Observation::Java { state, fixable } => {
                self.settled.insert(Check::Java, java(state, fixable));
            }
            Observation::Memory {
                automatic,
                validation,
            } => match memory(automatic, &validation) {
                Some(finding) => {
                    self.settled.insert(Check::Memory, finding);
                }
                None => {
                    self.settled.remove(&Check::Memory);
                }
            },
            Observation::Mods(summary) => {
                self.settled.insert(Check::Mods, mods(summary.as_ref()));
            }
            Observation::Crash { last, now } => {
                self.settled.insert(Check::Crash, crash(last.as_ref(), now));
            }
            Observation::Failed(check) => {
                if check == Check::Internet {
                    self.uplink = Uplink::Unknown;
                }
                self.settled.insert(
                    check,
                    Finding::new(check, Level::Skipped, Code::CheckFailed),
                );
            }
        }
        self.refresh()
    }

    /// Every check is in (or its time ran out): failures that waited for the internet settle, and
    /// a planned row with nothing found says the check could not run.
    pub fn finish(&mut self) -> Vec<Finding> {
        self.finished = true;
        self.refresh()
    }

    /// Everything found so far, in row order.
    pub fn findings(&self) -> Vec<Finding> {
        self.shown.values().cloned().collect()
    }

    fn current(&self) -> BTreeMap<Check, Finding> {
        let mut all = self.settled.clone();
        let uplink = if self.finished && self.uplink == Uplink::Pending {
            Uplink::Unknown
        } else {
            self.uplink
        };
        let mut put = |finding: Option<Finding>| {
            if let Some(finding) = finding {
                all.insert(finding.check, finding);
            }
        };
        put(self.server.as_ref().and_then(|p| server(p, uplink)));
        put(self.content.as_ref().and_then(|p| content(p, uplink)));
        put(self.friends.as_deref().and_then(|p| friends(p, uplink)));
        put(self.microsoft.as_deref().and_then(|p| microsoft(p, uplink)));
        put(self.discord.as_deref().and_then(|p| discord(p, uplink)));
        put(self.clock.clone());
        if self.finished {
            for check in &self.planned {
                all.entry(*check)
                    .or_insert_with(|| Finding::new(*check, Level::Skipped, Code::CheckFailed));
            }
        }
        all
    }

    fn refresh(&mut self) -> Vec<Finding> {
        let current = self.current();
        let changed: Vec<Finding> = current
            .values()
            .filter(|finding| self.shown.get(&finding.check) != Some(*finding))
            .cloned()
            .collect();
        self.shown = current;
        changed
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;

    fn probe(reach: Reach, ms: u64) -> Probe {
        Probe {
            reach,
            elapsed: Duration::from_millis(ms),
            server_date: None,
            received_at: SystemTime::now(),
        }
    }

    fn dated(reach: Reach, offset_secs: i64) -> Probe {
        let server = UNIX_EPOCH + Duration::from_secs(1_800_000_000);
        let received_at = if offset_secs >= 0 {
            server + Duration::from_secs(offset_secs.unsigned_abs())
        } else {
            server - Duration::from_secs(offset_secs.unsigned_abs())
        };
        Probe {
            reach,
            elapsed: Duration::from_millis(80),
            server_date: Some(server),
            received_at,
        }
    }

    const OK: Reach = Reach::Answered { status: 200 };

    #[test]
    fn the_internet_is_up_when_any_address_answers() {
        let probes = [probe(Reach::TimedOut, 4000), probe(OK, 120)];
        let finding = internet(Some(Connectivity::Local), &probes, false);
        assert_eq!((finding.level, finding.code), (Level::Ok, Code::InternetOk));
        assert_eq!(finding.facts.latency_ms, Some(120));
        // A 404 is an answer too.
        let not_found = [probe(Reach::Answered { status: 404 }, 90)];
        assert_eq!(internet(None, &not_found, false).code, Code::InternetOk);
        let slow = [probe(OK, 2600)];
        assert_eq!(internet(None, &slow, false).code, Code::InternetSlow);
    }

    #[test]
    fn says_why_the_internet_is_down() {
        let failed = [probe(Reach::Failed, 10), probe(Reach::Failed, 10)];
        let code = |os, probes: &[Probe]| internet(os, probes, false).code;
        assert_eq!(
            code(Some(Connectivity::None), &failed),
            Code::InternetOffline
        );
        assert_eq!(
            code(Some(Connectivity::Local), &failed),
            Code::InternetLocal
        );
        assert_eq!(
            code(Some(Connectivity::Constrained), &failed),
            Code::InternetSignIn
        );
        let dns = [probe(Reach::UnknownHost, 5), probe(Reach::UnknownHost, 5)];
        assert_eq!(
            code(Some(Connectivity::Internet), &dns),
            Code::InternetNoDns
        );
        assert_eq!(code(None, &dns), Code::InternetNoDns);
        let insecure = [probe(Reach::Insecure, 30), probe(Reach::TimedOut, 4000)];
        assert_eq!(code(None, &insecure), Code::InternetInsecure);
        assert_eq!(
            code(Some(Connectivity::Unknown), &failed),
            Code::InternetDown
        );
        let finding = internet(None, &failed, false);
        assert_eq!(finding.level, Level::Problem);
        assert_eq!(finding.fix, None);
        // A wrong clock is why secure connections fail: its fix comes along.
        assert_eq!(
            internet(None, &insecure, true).fix,
            Some(Fix::ClockSettings)
        );
        assert_eq!(internet(None, &failed, true).fix, None);
    }

    #[test]
    fn a_wrong_clock_is_a_row_of_its_own() {
        assert_eq!(clock(&[]), None);
        assert_eq!(clock(&[30, -2, 1]), None);
        let off = clock(&[3_600, 3_590, 3_610]).unwrap();
        assert_eq!((off.level, off.code), (Level::Problem, Code::ClockWrong));
        assert_eq!(off.facts.minutes, Some(60));
        assert_eq!(off.fix, Some(Fix::ClockSettings));
        assert_eq!(clock(&[-86_400]).unwrap().facts.minutes, Some(-1_440));
        // One odd server does not decide.
        assert_eq!(clock(&[2, 1, 99_999]), None);
    }

    #[test]
    fn judges_the_server_ping() {
        let online = |latency_ms, players| ServerProbe::Online {
            latency_ms,
            players,
        };
        let finding = server(&online(42, Some(1_204)), Uplink::Pending).unwrap();
        assert_eq!((finding.level, finding.code), (Level::Ok, Code::ServerOk));
        assert_eq!(finding.facts.players, Some(1_204));
        assert_eq!(finding.facts.latency_ms, Some(42));
        assert_eq!(
            server(&online(300, None), Uplink::Up).unwrap().code,
            Code::ServerSlow
        );
        let offline = |failure| {
            server(&ServerProbe::Offline(failure), Uplink::Up)
                .unwrap()
                .code
        };
        assert_eq!(offline(PingFailure::Refused), Code::ServerDown);
        assert_eq!(offline(PingFailure::UnknownHost), Code::ServerNotFound);
        assert_eq!(offline(PingFailure::InvalidAddress), Code::ServerNotFound);
        assert_eq!(offline(PingFailure::Timeout), Code::ServerUnreachable);
        assert_eq!(offline(PingFailure::Protocol), Code::ServerUnreachable);
        // A failure waits for the internet, then says what the internet decided.
        let down = ServerProbe::Offline(PingFailure::Timeout);
        assert_eq!(server(&down, Uplink::Pending), None);
        let skipped = server(&down, Uplink::Down).unwrap();
        assert_eq!(
            (skipped.level, skipped.code),
            (Level::Skipped, Code::ServerNeedsInternet)
        );
        assert_eq!(
            server(&down, Uplink::Unknown).unwrap().code,
            Code::ServerUnreachable
        );
    }

    #[test]
    fn judges_the_content_and_the_services() {
        let saved = ContentProbe {
            mirrors: vec![Reach::TimedOut, OK],
            cached: true,
        };
        assert_eq!(
            content(&saved, Uplink::Pending).unwrap().code,
            Code::ContentOk
        );
        let gone = ContentProbe {
            mirrors: vec![Reach::TimedOut, Reach::Answered { status: 404 }],
            cached: true,
        };
        assert_eq!(content(&gone, Uplink::Pending), None);
        assert_eq!(
            content(&gone, Uplink::Up).unwrap().code,
            Code::ContentUnreachable
        );
        let first = ContentProbe {
            cached: false,
            ..gone.clone()
        };
        let missing = content(&first, Uplink::Up).unwrap();
        assert_eq!(
            (missing.level, missing.code),
            (Level::Problem, Code::ContentMissing)
        );
        assert_eq!(
            content(&first, Uplink::Down).unwrap().code,
            Code::ContentNeedsInternet
        );

        assert_eq!(
            friends(&[OK], Uplink::Pending).unwrap().code,
            Code::FriendsOk
        );
        assert_eq!(
            friends(&[Reach::Answered { status: 503 }], Uplink::Pending)
                .unwrap()
                .code,
            Code::FriendsDown
        );
        assert_eq!(friends(&[Reach::TimedOut], Uplink::Pending), None);
        assert_eq!(
            friends(&[Reach::TimedOut], Uplink::Up).unwrap().code,
            Code::FriendsUnreachable
        );
        assert_eq!(
            friends(&[], Uplink::Up).unwrap().code,
            Code::FriendsUnreachable
        );

        let not_found = Reach::Answered { status: 404 };
        assert_eq!(
            microsoft(&[OK, not_found, OK], Uplink::Pending)
                .unwrap()
                .code,
            Code::MicrosoftOk
        );
        assert_eq!(
            microsoft(&[OK, Reach::Answered { status: 502 }, OK], Uplink::Pending)
                .unwrap()
                .code,
            Code::MicrosoftDown
        );
        assert_eq!(microsoft(&[OK, Reach::TimedOut, OK], Uplink::Pending), None);
        assert_eq!(
            microsoft(&[OK, Reach::TimedOut, OK], Uplink::Up)
                .unwrap()
                .code,
            Code::MicrosoftUnreachable
        );
        assert_eq!(
            microsoft(&[Reach::TimedOut], Uplink::Down).unwrap().code,
            Code::MicrosoftNeedsInternet
        );

        assert_eq!(
            discord(&[not_found], Uplink::Pending).unwrap().code,
            Code::DiscordOk
        );
        let unreachable = discord(&[Reach::UnknownHost], Uplink::Up).unwrap();
        assert_eq!(
            (unreachable.level, unreachable.code),
            (Level::Warning, Code::DiscordUnreachable)
        );
    }

    #[test]
    fn judges_free_space() {
        let unknown = disk(None);
        assert_eq!(
            (unknown.level, unknown.code),
            (Level::Skipped, Code::DiskUnknown)
        );
        let full = disk(Some(800 * 1024 * 1024));
        assert_eq!(
            (full.level, full.code, full.fix),
            (Level::Problem, Code::DiskFull, Some(Fix::MakeSpace))
        );
        assert_eq!(full.facts.free_bytes, Some(800 * 1024 * 1024));
        let low = disk(Some(GIB + GIB / 2));
        assert_eq!((low.level, low.code), (Level::Warning, Code::DiskLow));
        let roomy = disk(Some(2 * GIB));
        assert_eq!(
            (roomy.level, roomy.code, roomy.fix),
            (Level::Ok, Code::DiskOk, None)
        );
    }

    #[test]
    fn judges_java() {
        let ready = java(
            JavaState::Automatic {
                installed: Some(21),
            },
            true,
        );
        assert_eq!(
            (ready.level, ready.code, ready.facts.major),
            (Level::Ok, Code::JavaReady, Some(21))
        );
        assert_eq!(
            java(JavaState::Automatic { installed: None }, true).code,
            Code::JavaDownloads
        );
        let missing = java(JavaState::Missing, true);
        assert_eq!(
            (missing.level, missing.fix),
            (Level::Problem, Some(Fix::InstallJava))
        );
        assert_eq!(
            java(JavaState::Broken, false).fix,
            Some(Fix::InstanceSettings)
        );
        let old = java(JavaState::TooOld { have: 8, need: 21 }, true);
        assert_eq!(
            (old.code, old.facts.major, old.facts.need),
            (Code::JavaTooOld, Some(8), Some(21))
        );
        let new = java(JavaState::TooNew { have: 25, max: 17 }, true);
        assert_eq!((new.code, new.facts.need), (Code::JavaTooNew, Some(17)));
        let x86 = java(
            JavaState::Fits {
                major: 21,
                bits32: true,
            },
            true,
        );
        assert_eq!(
            (x86.level, x86.code, x86.fix),
            (Level::Warning, Code::JavaX86, Some(Fix::InstallJava))
        );
        let fits = java(
            JavaState::Fits {
                major: 21,
                bits32: false,
            },
            true,
        );
        assert_eq!(
            (fits.level, fits.code, fits.fix),
            (Level::Ok, Code::JavaCustomOk, None)
        );
        assert_eq!(java(JavaState::Unknown, true).level, Level::Skipped);
    }

    #[test]
    fn judges_memory_by_the_settings_rules() {
        let total = 8 * 1024;
        let check = |max_mb| {
            let settings = pl_mc::instance::MemorySettings {
                min_mb: 1024,
                max_mb,
            };
            memory(false, &pl_mc::instance::validate_memory(&settings, total))
        };
        assert_eq!(check(4096), None);
        let high = check(7 * 1024 - 256).unwrap();
        assert_eq!(
            (high.level, high.code, high.fix),
            (Level::Warning, Code::MemoryHigh, Some(Fix::MemoryAuto))
        );
        let over = check(8 * 1024).unwrap();
        assert_eq!(
            (over.level, over.code),
            (Level::Problem, Code::MemoryTooHigh)
        );
        // "Otomatik" always fits.
        let settings = pl_mc::instance::MemorySettings {
            min_mb: 1024,
            max_mb: 8 * 1024,
        };
        assert_eq!(
            memory(true, &pl_mc::instance::validate_memory(&settings, total)),
            None
        );
    }

    #[test]
    fn judges_the_mod_check() {
        assert_eq!(mods(None).code, Code::ModsUnknown);
        let summary = |blocking, warnings, repairable| ModsSummary {
            blocking,
            warnings,
            repairable,
            mods: 48,
        };
        let blocked = mods(Some(&summary(2, 1, true)));
        assert_eq!(
            (blocked.level, blocked.code, blocked.fix),
            (Level::Problem, Code::ModsBlocking, Some(Fix::Repair))
        );
        assert_eq!(blocked.facts.count, Some(2));
        let warned = mods(Some(&summary(0, 3, false)));
        assert_eq!(
            (warned.level, warned.code, warned.fix),
            (Level::Warning, Code::ModsWarning, Some(Fix::ShowMods))
        );
        let fine = mods(Some(&summary(0, 0, false)));
        assert_eq!(
            (fine.level, fine.code, fine.facts.count),
            (Level::Ok, Code::ModsOk, Some(48))
        );
    }

    #[test]
    fn a_crash_counts_for_a_week_until_the_game_runs_again() {
        let now: Timestamp = "2026-10-21T18:00:00Z".parse().unwrap();
        let at: Timestamp = "2026-10-20T21:30:00Z".parse().unwrap();
        let last = LastCrash {
            at,
            kind: CrashKind::OutOfMemory,
            suspect: Some("Sodium".into()),
            resolved: false,
        };
        let recent = crash(Some(&last), now);
        assert_eq!(
            (recent.level, recent.code, recent.fix),
            (Level::Problem, Code::CrashRecent, Some(Fix::CrashDetails))
        );
        assert_eq!(recent.facts.at, Some(at));
        assert_eq!(recent.facts.crash, Some(CrashKind::OutOfMemory));
        assert_eq!(recent.facts.name.as_deref(), Some("Sodium"));
        let resolved = LastCrash {
            resolved: true,
            ..last.clone()
        };
        assert_eq!(crash(Some(&resolved), now).code, Code::CrashResolved);
        let old = LastCrash {
            at: "2026-10-10T10:00:00Z".parse().unwrap(),
            ..last
        };
        assert_eq!(crash(Some(&old), now).code, Code::CrashNone);
        assert_eq!(crash(None, now).code, Code::CrashNone);
    }

    #[test]
    fn mod_names_never_carry_a_path() {
        assert_eq!(safe_name("  Sodium "), Some("Sodium".into()));
        assert_eq!(safe_name("Fabric API"), Some("Fabric API".into()));
        assert_eq!(safe_name("Xaero's Minimap"), Some("Xaero's Minimap".into()));
        assert_eq!(
            safe_name("sodium-fabric-0.6.0.jar"),
            Some("sodium-fabric-0.6.0.jar".into())
        );
        assert_eq!(
            safe_name("Create: Steam 'n' Rails"),
            Some("Create: Steam 'n' Rails".into())
        );
        for bad in [
            "",
            "   ",
            r"C:\Users\Emir\AppData\Roaming\ProjectsLauncher\mods\x.jar",
            "/home/emir/.minecraft/mods/x.jar",
            "emir@example.com",
            "192.168.1.20",
            "near 10.0.0.7 here",
            "play.projects.gg:25565",
            "fe80::1",
            "D:Mods",
        ] {
            assert_eq!(safe_name(bad), None, "{bad}");
        }
        let long = "A".repeat(100);
        assert_eq!(safe_name(&long).unwrap().chars().count(), 48);
        assert_eq!(safe_name("Iris\u{7}\n"), Some("Iris".into()));
    }

    fn all_observations() -> Vec<Observation> {
        let now: Timestamp = "2026-10-21T18:00:00Z".parse().unwrap();
        vec![
            Observation::Internet {
                os: Some(Connectivity::Internet),
                secure: vec![probe(OK, 90), probe(Reach::TimedOut, 4000)],
                plain: vec![dated(Reach::Answered { status: 301 }, 4)],
            },
            Observation::Server(ServerProbe::Offline(PingFailure::Timeout)),
            Observation::Content(ContentProbe {
                mirrors: vec![OK],
                cached: true,
            }),
            Observation::Friends(vec![Reach::TimedOut]),
            Observation::Disk(Some(800 * 1024 * 1024)),
            Observation::Java {
                state: JavaState::Automatic {
                    installed: Some(21),
                },
                fixable: true,
            },
            Observation::Memory {
                automatic: true,
                validation: MemoryValidation::default(),
            },
            Observation::Mods(Some(ModsSummary {
                blocking: 0,
                warnings: 0,
                repairable: false,
                mods: 40,
            })),
            Observation::Crash { last: None, now },
        ]
    }

    const PLAN: [Check; 7] = [
        Check::Internet,
        Check::Server,
        Check::Content,
        Check::Friends,
        Check::Disk,
        Check::Java,
        Check::Mods,
    ];

    fn assemble(observations: Vec<Observation>) -> Vec<Finding> {
        let mut assembler = Assembler::new(PLAN.into_iter().chain([Check::Crash]));
        for observation in observations {
            assembler.observe(observation);
        }
        assembler.finish();
        assembler.findings()
    }

    #[test]
    fn the_order_the_checks_come_in_does_not_matter() {
        let forward = assemble(all_observations());
        let mut reversed = all_observations();
        reversed.reverse();
        assert_eq!(assemble(reversed), forward);
        let mut rotated = all_observations();
        rotated.rotate_left(4);
        assert_eq!(assemble(rotated), forward);
        let codes: Vec<(Check, Code)> = forward.iter().map(|f| (f.check, f.code)).collect();
        assert_eq!(
            codes,
            [
                (Check::Internet, Code::InternetOk),
                (Check::Server, Code::ServerUnreachable),
                (Check::Content, Code::ContentOk),
                (Check::Friends, Code::FriendsUnreachable),
                (Check::Disk, Code::DiskFull),
                (Check::Java, Code::JavaReady),
                (Check::Mods, Code::ModsOk),
                (Check::Crash, Code::CrashNone),
            ]
        );
    }

    #[test]
    fn a_failure_waits_for_the_internet_and_never_flips() {
        let mut assembler = Assembler::new(PLAN);
        // The ping fails before the internet is known: nothing yet.
        assert!(
            assembler
                .observe(Observation::Server(ServerProbe::Offline(
                    PingFailure::Timeout
                )))
                .is_empty()
        );
        // A success does not wait.
        let content = assembler.observe(Observation::Content(ContentProbe {
            mirrors: vec![OK],
            cached: true,
        }));
        assert_eq!(content.len(), 1);
        assert_eq!(content[0].code, Code::ContentOk);
        // The internet is down: the server needs it, and both come in one go, in row order.
        let changed = assembler.observe(Observation::Internet {
            os: Some(Connectivity::None),
            secure: vec![probe(Reach::Failed, 5)],
            plain: vec![],
        });
        let codes: Vec<Code> = changed.iter().map(|f| f.code).collect();
        assert_eq!(codes, [Code::InternetOffline, Code::ServerNeedsInternet]);
        // The same news again changes nothing.
        assert!(
            assembler
                .observe(Observation::Server(ServerProbe::Offline(
                    PingFailure::Timeout
                )))
                .is_empty()
        );
    }

    #[test]
    fn finishing_settles_what_still_waits() {
        let mut assembler = Assembler::new(PLAN);
        assembler.observe(Observation::Friends(vec![Reach::TimedOut]));
        assembler.observe(Observation::Failed(Check::Java));
        let changed = assembler.finish();
        assert!(
            changed.iter().all(|f| f.check != Check::Java),
            "shown before"
        );
        let settled = assembler.findings();
        let get = |check| settled.iter().find(|f| f.check == check).cloned().unwrap();
        // No internet result came: the friends service judges by itself.
        assert_eq!(get(Check::Friends).code, Code::FriendsUnreachable);
        // Rows that found nothing say the check could not run.
        for check in [
            Check::Internet,
            Check::Server,
            Check::Content,
            Check::Disk,
            Check::Mods,
        ] {
            let finding = get(check);
            assert_eq!(
                (finding.level, finding.code),
                (Level::Skipped, Code::CheckFailed),
                "{check:?}"
            );
        }
        assert_eq!(get(Check::Java).code, Code::CheckFailed);
        // Memory and the clock are not planned rows: nothing about them.
        assert!(
            settled
                .iter()
                .all(|f| f.check != Check::Memory && f.check != Check::Clock)
        );
    }

    #[test]
    fn the_internet_failing_to_check_lets_the_others_judge() {
        let mut assembler = Assembler::new(PLAN);
        assembler.observe(Observation::Server(ServerProbe::Offline(
            PingFailure::Refused,
        )));
        let changed = assembler.observe(Observation::Failed(Check::Internet));
        let codes: Vec<Code> = changed.iter().map(|f| f.code).collect();
        assert_eq!(codes, [Code::CheckFailed, Code::ServerDown]);
    }

    #[test]
    fn a_wrong_clock_adds_its_row_and_its_fix() {
        let mut assembler = Assembler::new(PLAN);
        let changed = assembler.observe(Observation::Internet {
            os: Some(Connectivity::Internet),
            secure: vec![probe(Reach::Insecure, 40), probe(Reach::Insecure, 40)],
            plain: vec![dated(Reach::Answered { status: 301 }, -3 * 24 * 3600)],
        });
        let internet = &changed[0];
        assert_eq!(
            (internet.code, internet.fix),
            (Code::InternetInsecure, Some(Fix::ClockSettings))
        );
        let clock = changed.iter().find(|f| f.check == Check::Clock).unwrap();
        assert_eq!(clock.facts.minutes, Some(-3 * 24 * 60));
        // A clock that is right adds no row.
        let mut fine = Assembler::new(PLAN);
        let changed = fine.observe(Observation::Internet {
            os: None,
            secure: vec![dated(OK, 1)],
            plain: vec![],
        });
        assert!(changed.iter().all(|f| f.check != Check::Clock));
    }

    #[test]
    fn memory_appears_only_when_it_is_too_high() {
        let mut assembler = Assembler::new(PLAN);
        let settings = pl_mc::instance::MemorySettings {
            min_mb: 1024,
            max_mb: 16 * 1024,
        };
        let changed = assembler.observe(Observation::Memory {
            automatic: false,
            validation: pl_mc::instance::validate_memory(&settings, 8 * 1024),
        });
        assert_eq!(changed[0].code, Code::MemoryTooHigh);
        // Back on "Otomatik": the row goes.
        assembler.observe(Observation::Memory {
            automatic: true,
            validation: MemoryValidation::default(),
        });
        assert!(
            assembler
                .findings()
                .iter()
                .all(|f| f.check != Check::Memory)
        );
    }

    #[test]
    fn findings_carry_no_personal_data() {
        let mut assembler = Assembler::new(PLAN);
        for observation in all_observations() {
            assembler.observe(observation);
        }
        assembler.observe(Observation::Crash {
            last: Some(LastCrash {
                at: "2026-10-21T17:00:00Z".parse().unwrap(),
                kind: CrashKind::ModError,
                suspect: Some(
                    r"C:\Users\Emir Can\AppData\Roaming\ProjectsLauncher\mods\a.jar".into(),
                ),
                resolved: false,
            }),
            now: "2026-10-21T18:00:00Z".parse().unwrap(),
        });
        assembler.finish();
        let json = serde_json::to_string(&assembler.findings()).unwrap();
        for needle in ["Emir", "Users", "\\\\", "AppData", "@"] {
            assert!(!json.contains(needle), "{needle} in {json}");
        }
        assert!(json.contains("\"code\":\"crashRecent\""));
    }

    #[test]
    fn levels_rank_problems_first() {
        let mut levels = [
            Level::Ok,
            Level::Skipped,
            Level::Problem,
            Level::Note,
            Level::Warning,
        ];
        levels.sort_by_key(|l| std::cmp::Reverse(l.severity()));
        assert_eq!(
            levels,
            [
                Level::Problem,
                Level::Warning,
                Level::Note,
                Level::Ok,
                Level::Skipped
            ]
        );
    }

    #[test]
    fn codes_and_checks_speak_camel_case() {
        assert_eq!(
            serde_json::to_string(&Code::JavaX86).unwrap(),
            "\"javaX86\""
        );
        assert_eq!(
            serde_json::to_string(&Code::ServerNeedsInternet).unwrap(),
            "\"serverNeedsInternet\""
        );
        assert_eq!(
            serde_json::to_string(&Check::Microsoft).unwrap(),
            "\"microsoft\""
        );
        assert_eq!(
            serde_json::to_string(&Fix::MakeSpace).unwrap(),
            "\"makeSpace\""
        );
    }
}

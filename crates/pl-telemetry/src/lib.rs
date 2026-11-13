//! Anonymous usage data of Projects Launcher (TELEMETRY).
//!
//! The owner wanted to see, in Studio, what helps improve the launcher and find problems, without
//! loading the servers and without anything a security product could mistake for spying. So:
//!
//! - **Off until Studio turns it on** (the signed config's `features.telemetry`), and off for good
//!   with the player's switch (Settings → Veri → "Kullanım verisi gönder", on by default). Off
//!   forgets whatever was not sent yet.
//! - **Counted on the computer, sent once a day**: one small gzipped HTTPS request to our own
//!   service (`studio-team.projects.gg`), like the launcher's other requests; a crash summary may
//!   go soon after a panic, at most once an hour ([`batch`]). Nothing wakes the launcher for it:
//!   the daily send rides on the daily tidy-up, the crash send on the crash.
//! - **Anonymous**: no account, no name, no install id, no IP address (the service keeps none),
//!   no chat, no server, no file paths, no list of installed mods. What a batch holds is in
//!   [`batch`] and [`facts`]; crash summaries are scrubbed ([`scrub`]). A game crash's trace
//!   (TELEM-FIX) names only things from closed lists: a Java or game exception class, the family
//!   of a native module, a known public mod, an exit code's name ([`ledger::GameCrash`]).
//! - **Nothing that looks like spying**: the counts come from what the launcher does anyway (its
//!   own commands, its launches, its voice state, its own panics and page errors). No other
//!   program, window, key press or file is looked at.
//!
//! This crate holds the rules and the file; the launcher's shell wires it (`src-tauri`'s
//! `telemetry.rs`) and sends the requests.

pub mod batch;
pub mod facts;
pub mod feature;
pub mod ledger;
pub mod recorder;
pub mod scrub;
pub mod voice;

pub use batch::{Carried, Outgoing};
pub use facts::AppFacts;
pub use feature::Feature;
pub use ledger::{CrashKind, CrashNote, GameCrash};
pub use recorder::{Answer, GameTrace, Launch, Recorder};
pub use voice::{Phase, VoiceSeen};

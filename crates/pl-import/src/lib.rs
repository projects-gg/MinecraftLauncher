//! Bringing a player over from another Minecraft launcher (IMPORT1).
//!
//! - [`scan`] finds what other launchers left on this computer: Mojang's Minecraft Launcher,
//!   TLauncher and SKlauncher (`.minecraft`), CurseForge, the Modrinth App, Prism Launcher, PolyMC
//!   and MultiMC. It is cheap enough for the first-run screen: it reads small settings files and
//!   lists folders, never walks worlds or opens mod jars.
//! - [`measure`] adds what the screen shows next, on demand: folder sizes and which mods fit the
//!   version they would run with (the mod health check's own rules, `pl_mc::mods::check`).
//! - [`run_import`] copies what the player picked: game settings, the server list, worlds,
//!   resource packs, shader packs and screenshots into the Projects instance, and every chosen
//!   profile or instance into an instance of its own (its mods become "Kendi modların").
//!   Installed versions are not copied: the instances download them through the launcher.
//!
//! Rules every part keeps:
//!
//! - Other launchers' folders are only read. Nothing there is written, renamed or locked.
//! - Accounts and sign-in tokens never come over; only an offline player name may be suggested.
//! - Every copy is atomic (a temporary file or folder next to the target, renamed at the end);
//!   nothing the player already has is replaced. A cancelled or failed item leaves nothing
//!   half-copied behind.
//! - Running it again copies only what is not there yet ([`journal`] remembers what came from
//!   where), so nothing is doubled.

mod copy;
mod detect;
mod error;
mod journal;
mod locate;
mod measure;
pub mod model;
pub mod options;
mod run;
pub mod servers;
mod version_json;
mod worlds;

pub use detect::{scan, scan_source};
pub use error::{Error, Result};
pub use journal::{ImportedFrom, imported_profiles};
pub use locate::{Locations, Os};
pub use measure::measure;
pub use model::*;
pub use run::{InstanceLocker, OfficialTarget, ProgressFn, RunContext, run_import};
pub use tokio_util::sync::CancellationToken;

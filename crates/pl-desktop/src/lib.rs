//! Desktop integration of Projects Launcher that needs no window:
//!
//! - [`link`]: `projects://` links (tray actions, the website, desktop shortcuts) parsed into a
//!   [`DeepLink`]. Unknown or malformed links are ignored, never guessed.
//! - [`shortcut`]: desktop shortcuts that start one instance (`projects://play/<instance>`):
//!   a shell link on Windows, a `.desktop` launcher on Linux, a `.webloc` on macOS.
//! - [`ico`]: a PNG wrapped as a Windows icon (the shortcut's picture).
//! - [`window`]: the main window's default size for the screen it first opens on.
//! - [`close`]: what closing the main window does (quit, the tray, or quit once the jobs end).
//! - [`game_window`]: whether the game's own window is on screen yet (Windows), so the launcher
//!   says "Açılıyor" until it is and "Oyunda" from then on.
//! - [`game_close`]: asking the game to close the way its own close button does (it saves and
//!   leaves a server as always), before it restarts into another server ("Bağlan").
//! - [`identity`]: the identity a process runs under (debug builds never share the installed
//!   launcher's identifier, keychain service or data folder).
//! - [`discord`]: whether the Discord app is installed, and the one link handed to it (a
//!   friend's profile, `discord://-/users/<id>`), for "Discord'dan mesaj at".
//! - [`process`]: a process the launcher left running (the game it quit beside): who it is, and
//!   waiting for its end.

pub mod close;
pub mod discord;
mod error;
pub mod game_close;
pub mod game_window;
pub mod ico;
pub mod identity;
pub mod link;
pub mod process;
pub mod shortcut;
#[cfg(test)]
mod test_support;
pub mod window;

pub use error::{Error, Result};
pub use ico::png_to_ico;
pub use link::{DeepLink, parse_link, play_link, valid_id};
pub use shortcut::{ShortcutSpec, create_shortcut, desktop_entry, shortcut_file_name, webloc};
pub use window::default_window_size;

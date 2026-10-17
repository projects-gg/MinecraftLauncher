//! Desktop integration of Projects Launcher that needs no window:
//!
//! - [`link`]: `projects://` links (tray actions, the website, desktop shortcuts) parsed into a
//!   [`DeepLink`]. Unknown or malformed links are ignored, never guessed.
//! - [`shortcut`]: desktop shortcuts that start one instance (`projects://play/<instance>`):
//!   a shell link on Windows, a `.desktop` launcher on Linux, a `.webloc` on macOS.
//! - [`ico`]: a PNG wrapped as a Windows icon (the shortcut's picture).
//! - [`window`]: the main window's default size for the screen it first opens on.
//! - [`identity`]: the identity a process runs under (debug builds never share the installed
//!   launcher's identifier, keychain service or data folder).

mod error;
pub mod ico;
pub mod identity;
pub mod link;
pub mod shortcut;
pub mod window;

pub use error::{Error, Result};
pub use ico::png_to_ico;
pub use link::{DeepLink, parse_link, play_link, valid_id};
pub use shortcut::{ShortcutSpec, create_shortcut, desktop_entry, shortcut_file_name, webloc};
pub use window::default_window_size;

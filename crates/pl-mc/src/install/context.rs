use pl_net::{Downloader, Http};

use crate::paths::DataDirs;
use crate::platform::Platform;

/// Files downloaded in parallel while preparing in the background (asset objects use twice as
/// many): gentle on a home connection someone else is using.
pub const BACKGROUND_CONCURRENCY: usize = 2;

/// Everything the version/install/launch pipeline needs. Cheap to clone (the HTTP client is
/// reference counted).
#[derive(Debug, Clone)]
pub struct McContext {
    pub http: Http,
    pub downloader: Downloader,
    pub dirs: DataDirs,
    /// Platform of the launcher process. Install narrows it to the architecture of the chosen
    /// Java (e.g. x64 under emulation) for natives and rules.
    pub platform: Platform,
    /// Preparing in the background ([`McContext::for_background`]): few downloads at a time,
    /// interrupted files keep what they got, and installer steps run at a low priority.
    pub background: bool,
}

impl McContext {
    /// Context for the current platform.
    pub fn new(http: Http, dirs: DataDirs) -> Self {
        Self {
            downloader: Downloader::new(http.clone()),
            http,
            dirs,
            platform: Platform::current(),
            background: false,
        }
    }

    /// Override the platform (tests, or preparing files for another OS/architecture).
    pub fn with_platform(mut self, platform: Platform) -> Self {
        self.platform = platform;
        self
    }

    /// The same context for preparing in the background, before the player asked for anything.
    pub fn for_background(&self) -> Self {
        Self {
            downloader: self.downloader.clone().keeping_partial_files(),
            background: true,
            ..self.clone()
        }
    }

    /// Parallel downloads: `wanted`, or [`BACKGROUND_CONCURRENCY`] in the background.
    pub fn concurrency(&self, wanted: usize) -> usize {
        if self.background {
            BACKGROUND_CONCURRENCY
        } else {
            wanted
        }
    }
}

#[cfg(test)]
mod tests {
    use pl_net::HttpConfig;

    use super::*;

    #[test]
    fn a_background_context_downloads_gently() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = McContext::new(
            Http::new(HttpConfig::new("pl-mc-test")).unwrap(),
            DataDirs::new(dir.path()),
        );
        assert!(!ctx.background);
        assert_eq!(ctx.concurrency(16), 16);
        let background = ctx.for_background();
        assert!(background.background);
        assert_eq!(background.concurrency(16), BACKGROUND_CONCURRENCY);
        assert_eq!(background.dirs.root(), ctx.dirs.root());
    }
}

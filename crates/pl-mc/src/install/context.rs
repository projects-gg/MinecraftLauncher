use pl_net::{Downloader, Http};

use crate::paths::DataDirs;
use crate::platform::Platform;

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
}

impl McContext {
    /// Context for the current platform.
    pub fn new(http: Http, dirs: DataDirs) -> Self {
        Self {
            downloader: Downloader::new(http.clone()),
            http,
            dirs,
            platform: Platform::current(),
        }
    }

    /// Override the platform (tests, or preparing files for another OS/architecture).
    pub fn with_platform(mut self, platform: Platform) -> Self {
        self.platform = platform;
        self
    }
}

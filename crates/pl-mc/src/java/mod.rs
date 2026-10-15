//! Java for the game: Mojang's runtimes ([`runtime`]) and Java installed on the system
//! ([`detect`]), chosen per instance with [`JavaSetting`].

pub mod detect;
pub mod runtime;

use std::path::PathBuf;

use pl_net::{CancellationToken, ProgressFn, VerifyMode};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::install::McContext;
use crate::platform::Arch;
use crate::version::ResolvedVersion;

pub use detect::{
    detect_system_javas, find_system_java, parse_java_properties, parse_major, probe_java,
};
pub use runtime::{
    JAVA_RUNTIME_MANIFEST_URL, RuntimeEntry, RuntimeIndex, install_runtime, installed_runtime,
    runtime_index, runtime_java_executable, select_runtime,
};

/// Per-instance Java choice.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum JavaSetting {
    /// Mojang's runtime for the version (downloaded on demand), else a matching system Java.
    #[default]
    Auto,
    /// A specific `java`/`javaw` executable chosen by the user.
    Custom { path: PathBuf },
}

/// What `java -XshowSettings:properties -version` reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct JavaInfo {
    /// `java.version` (`21.0.7`, `1.8.0_51`, `25`).
    pub version: String,
    pub major: u32,
    /// `os.arch` of the JVM (decides which natives are used).
    pub arch: Arch,
    pub vendor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum JavaSource {
    /// A Mojang runtime under `runtimes/<component>/<platform>/`.
    Mojang { component: String, platform: String },
    /// Found on the system (JAVA_HOME, PATH, common install folders).
    System,
    /// Chosen by the user.
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct JavaInstallation {
    /// Executable used to start the game (`javaw.exe` on Windows when available).
    pub path: PathBuf,
    pub info: JavaInfo,
    pub source: JavaSource,
}

impl JavaInstallation {
    pub fn arch(&self) -> Arch {
        self.info.arch
    }
}

/// Options for [`ensure_java`].
#[derive(Clone, Default)]
pub struct EnsureJavaOptions {
    /// No network: only installed runtimes and system Java are considered.
    pub offline: bool,
    /// How much to trust existing runtime files. With [`VerifyMode::Full`] every file of an
    /// already installed runtime is re-hashed (repair); otherwise an installed runtime whose
    /// manifest is current is used as is.
    pub verify: VerifyMode,
    pub cancel: CancellationToken,
    pub progress: Option<ProgressFn>,
}

impl std::fmt::Debug for EnsureJavaOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EnsureJavaOptions")
            .field("offline", &self.offline)
            .field("verify", &self.verify)
            .field("has_progress", &self.progress.is_some())
            .finish()
    }
}

/// Pick (and if needed download) the Java for `resolved`.
///
/// - [`JavaSetting::Custom`]: the given executable, probed for version/architecture.
/// - [`JavaSetting::Auto`]: the Mojang runtime named by `javaVersion.component` (`jre-legacy`
///   when missing), with the macOS arm64 → x64 (Rosetta) and Windows arm64 → x64 fallbacks. When
///   Mojang publishes nothing for the platform, or offline without an installed runtime, a system
///   Java with the right major version is used instead.
pub async fn ensure_java(
    ctx: &McContext,
    resolved: &ResolvedVersion,
    setting: &JavaSetting,
    options: &EnsureJavaOptions,
) -> Result<JavaInstallation> {
    let component = resolved.java_component();
    let major = resolved.java_major();
    match setting {
        JavaSetting::Custom { path } => {
            let info = probe_java(path).await?;
            if info.major < major {
                tracing::warn!(
                    path = %path.display(),
                    have = info.major,
                    need = major,
                    "custom Java is older than the version requires"
                );
            }
            Ok(JavaInstallation {
                path: path.clone(),
                info,
                source: JavaSource::Custom,
            })
        }
        JavaSetting::Auto if options.offline => {
            if let Some(installed) = installed_runtime(ctx, component).await {
                return Ok(installed);
            }
            if let Some(system) = find_system_java(major).await {
                tracing::info!(path = %system.path.display(), "offline: using system Java");
                return Ok(system);
            }
            tracing::warn!(%component, "offline and no installed Java runtime");
            Err(Error::Offline)
        }
        JavaSetting::Auto => match install_runtime(ctx, component, options).await {
            Ok(installed) => Ok(installed),
            Err(error) if error.is_cancelled() => Err(Error::Cancelled),
            Err(error) => {
                if let Some(installed) = installed_runtime(ctx, component).await {
                    tracing::warn!(%error, "runtime update failed, using the installed copy");
                    return Ok(installed);
                }
                if let Some(system) = find_system_java(major).await {
                    tracing::warn!(%error, path = %system.path.display(), "using system Java instead");
                    return Ok(system);
                }
                Err(match error {
                    Error::Unsupported(message) => Error::Java(format!(
                        "{message}; install Java {major} and choose it in the instance settings"
                    )),
                    other => other,
                })
            }
        },
    }
}

//! Java installed on the system: discovery (JAVA_HOME, PATH, common install folders per OS) and
//! probing with `java -XshowSettings:properties -version`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use crate::error::{Error, Result};
use crate::java::{JavaInfo, JavaInstallation, JavaSource};
use crate::platform::{Arch, Os};

const PROBE_TIMEOUT: Duration = Duration::from_secs(15);

/// Windows: do not flash a console window for child processes.
#[cfg(windows)]
pub(crate) const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Run `java -XshowSettings:properties -version` and parse version, major, `os.arch` and vendor.
pub async fn probe_java(path: &Path) -> Result<JavaInfo> {
    let exe = console_java(path);
    let mut command = tokio::process::Command::new(&exe);
    command
        .arg("-XshowSettings:properties")
        .arg("-version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);

    let output = tokio::time::timeout(PROBE_TIMEOUT, command.output())
        .await
        .map_err(|_| Error::Java(format!("{} did not answer in time", exe.display())))?
        .map_err(|e| Error::Java(format!("cannot run {}: {e}", exe.display())))?;
    let mut text = String::from_utf8_lossy(&output.stderr).into_owned();
    text.push('\n');
    text.push_str(&String::from_utf8_lossy(&output.stdout));
    parse_java_properties(&text)
        .ok_or_else(|| Error::Java(format!("{} is not a working Java", exe.display())))
}

/// Parse the output of `java -XshowSettings:properties -version` (falls back to the
/// `version "..."` banner for JVMs that ignore `-XshowSettings`).
pub fn parse_java_properties(text: &str) -> Option<JavaInfo> {
    let mut props: HashMap<&str, &str> = HashMap::new();
    for line in text.lines() {
        if let Some((key, value)) = line.trim().split_once(" = ") {
            props.entry(key.trim()).or_insert(value.trim());
        }
    }
    let version = props
        .get("java.version")
        .map(|v| (*v).to_owned())
        .or_else(|| banner_version(text))?;
    let major = parse_major(&version)?;
    let arch = props
        .get("os.arch")
        .map(|a| Arch::from_name(a))
        .unwrap_or(Arch::Other);
    let vendor = props
        .get("java.vendor")
        .or_else(|| props.get("java.vm.vendor"))
        .map(|v| (*v).to_owned());
    Some(JavaInfo {
        version,
        major,
        arch,
        vendor,
    })
}

fn banner_version(text: &str) -> Option<String> {
    let start = text.find("version \"")? + "version \"".len();
    let end = text[start..].find('"')? + start;
    Some(text[start..end].to_owned())
}

/// Java major version from a version string: `1.8.0_402` → 8, `17.0.2` → 17, `25-ea` → 25,
/// Mojang runtime names like `8u51` → 8.
pub fn parse_major(version: &str) -> Option<u32> {
    let v = version.trim().trim_matches('"');
    let leading = |s: &str| -> Option<u32> {
        let digits: String = s.chars().take_while(char::is_ascii_digit).collect();
        digits.parse().ok()
    };
    match v.strip_prefix("1.") {
        Some(rest) => leading(rest),
        None => leading(v),
    }
    .filter(|m| *m > 0)
}

/// On Windows `javaw.exe` prints nothing useful without a console; probe the sibling `java.exe`.
fn console_java(path: &Path) -> PathBuf {
    if path
        .file_name()
        .is_some_and(|n| n.eq_ignore_ascii_case("javaw.exe"))
    {
        let sibling = path.with_file_name("java.exe");
        if sibling.is_file() {
            return sibling;
        }
    }
    path.to_path_buf()
}

/// Executable to start the game with: `javaw.exe` next to `java.exe` when present (no console).
pub fn preferred_launch_executable(java: &Path) -> PathBuf {
    if java
        .file_name()
        .is_some_and(|n| n.eq_ignore_ascii_case("java.exe"))
    {
        let javaw = java.with_file_name("javaw.exe");
        if javaw.is_file() {
            return javaw;
        }
    }
    java.to_path_buf()
}

/// Find and probe every Java on the system. Duplicates (same canonical path) are removed and
/// broken installs are skipped.
pub async fn detect_system_javas() -> Vec<JavaInstallation> {
    let candidates = tokio::task::spawn_blocking(candidate_executables)
        .await
        .unwrap_or_default();
    let probes = candidates.into_iter().map(|path| async move {
        match probe_java(&path).await {
            Ok(info) => Some(JavaInstallation {
                path: preferred_launch_executable(&path),
                info,
                source: JavaSource::System,
            }),
            Err(error) => {
                tracing::debug!(path = %path.display(), %error, "skipping Java candidate");
                None
            }
        }
    });
    futures::future::join_all(probes)
        .await
        .into_iter()
        .flatten()
        .collect()
}

/// Best system Java for `major`: the same major version, else (for Java 17+ requirements) the
/// closest newer one. Java 8 games are never given a newer Java (LaunchWrapper breaks on 9+).
pub async fn find_system_java(major: u32) -> Option<JavaInstallation> {
    let all = detect_system_javas().await;
    pick_for_major(all, major)
}

pub(crate) fn pick_for_major(all: Vec<JavaInstallation>, major: u32) -> Option<JavaInstallation> {
    let native_first = |j: &JavaInstallation| j.info.arch != Arch::current();
    let mut exact: Vec<JavaInstallation> = all
        .iter()
        .filter(|j| j.info.major == major)
        .cloned()
        .collect();
    exact.sort_by_key(native_first);
    if let Some(found) = exact.into_iter().next() {
        return Some(found);
    }
    if major < 17 {
        return None;
    }
    let mut newer: Vec<JavaInstallation> =
        all.into_iter().filter(|j| j.info.major > major).collect();
    newer.sort_by_key(|j| (j.info.major, native_first(j)));
    newer.into_iter().next()
}

fn exe_name() -> &'static str {
    if Os::current() == Os::Windows {
        "java.exe"
    } else {
        "java"
    }
}

/// Sub-directories of `dir` (empty when it does not exist).
fn children(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect()
        })
        .unwrap_or_default()
}

/// Candidate `java` executables, deduplicated by canonical path. Blocking (directory listing).
fn candidate_executables() -> Vec<PathBuf> {
    let exe = exe_name();
    let mut out: Vec<PathBuf> = Vec::new();
    let push_home = |home: &Path, out: &mut Vec<PathBuf>| {
        out.push(home.join("bin").join(exe));
        out.push(home.join("jre").join("bin").join(exe));
        out.push(home.join("Contents").join("Home").join("bin").join(exe));
    };

    if let Some(home) = std::env::var_os("JAVA_HOME") {
        push_home(Path::new(&home), &mut out);
    }
    if let Ok(found) = which::which_all_global("java") {
        out.extend(found);
    }

    let user_home = dirs::home_dir();
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(home) = &user_home {
        roots.push(home.join(".jdks"));
        roots.push(home.join(".sdkman").join("candidates").join("java"));
    }
    match Os::current() {
        Os::Windows => {
            let mut program_dirs: Vec<PathBuf> =
                ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"]
                    .iter()
                    .filter_map(std::env::var_os)
                    .map(PathBuf::from)
                    .collect();
            if let Some(local) = std::env::var_os("LOCALAPPDATA") {
                program_dirs.push(PathBuf::from(local).join("Programs"));
            }
            let vendors = [
                "Java",
                "Eclipse Adoptium",
                "Eclipse Foundation",
                "AdoptOpenJDK",
                "Microsoft",
                "Zulu",
                "BellSoft",
                "Amazon Corretto",
                "Semeru",
                "IBM",
                "OpenJDK",
                "RedHat",
                "ojdkbuild",
            ];
            for base in &program_dirs {
                for vendor in vendors {
                    roots.push(base.join(vendor));
                }
            }
            // Runtimes downloaded by the official Minecraft launcher.
            let mut mc_runtime_roots = Vec::new();
            if let Some(appdata) = std::env::var_os("APPDATA") {
                mc_runtime_roots.push(PathBuf::from(appdata).join(".minecraft").join("runtime"));
            }
            if let Some(local) = std::env::var_os("LOCALAPPDATA") {
                mc_runtime_roots.push(
                    PathBuf::from(local)
                        .join("Packages")
                        .join("Microsoft.4297127D64EC6_8wekyb3d8bbwe")
                        .join("LocalCache")
                        .join("Local")
                        .join("runtime"),
                );
            }
            for base in &program_dirs {
                mc_runtime_roots.push(base.join("Minecraft Launcher").join("runtime"));
            }
            for root in mc_runtime_roots {
                // runtime/<component>/<platform>/<component>/bin/java.exe
                for component in children(&root) {
                    for platform in children(&component) {
                        for home in children(&platform) {
                            push_home(&home, &mut out);
                        }
                    }
                }
            }
        }
        Os::MacOs => {
            roots.push(PathBuf::from("/Library/Java/JavaVirtualMachines"));
            if let Some(home) = &user_home {
                roots.push(
                    home.join("Library")
                        .join("Java")
                        .join("JavaVirtualMachines"),
                );
            }
            for brew in ["/opt/homebrew/opt", "/usr/local/opt"] {
                for dir in children(Path::new(brew)) {
                    let name = dir.file_name().map(|n| n.to_string_lossy().into_owned());
                    if name.is_some_and(|n| n.starts_with("openjdk")) {
                        push_home(&dir, &mut out);
                        push_home(&dir.join("libexec").join("openjdk.jdk"), &mut out);
                    }
                }
            }
        }
        Os::Linux | Os::Other => {
            for dir in [
                "/usr/lib/jvm",
                "/usr/lib64/jvm",
                "/usr/java",
                "/opt/java",
                "/opt/jdk",
            ] {
                roots.push(PathBuf::from(dir));
            }
            for dir in children(Path::new("/opt")) {
                let name = dir.file_name().map(|n| n.to_string_lossy().to_lowercase());
                if name
                    .is_some_and(|n| n.contains("jdk") || n.contains("jre") || n.contains("java"))
                {
                    push_home(&dir, &mut out);
                }
            }
        }
    }
    for root in &roots {
        for home in children(root) {
            push_home(&home, &mut out);
        }
    }

    let mut seen = HashSet::new();
    out.into_iter()
        .filter(|p| p.is_file())
        // The macOS `/usr/bin/java` stub opens an "install Java" dialog when no JDK exists.
        .filter(|p| !(Os::current() == Os::MacOs && p == Path::new("/usr/bin/java")))
        .filter(|p| seen.insert(std::fs::canonicalize(p).unwrap_or_else(|_| p.clone())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_properties_output() {
        let text = "Property settings:\n    file.encoding = UTF-8\n    java.home = C:\\Program Files\\Java\\jdk-25\n    java.library.path = C:\\a\n        C:\\b\n    java.vendor = Oracle Corporation\n    java.version = 25\n    os.arch = amd64\n\njava version \"25\" 2025-09-16 LTS\n";
        let info = parse_java_properties(text).unwrap();
        assert_eq!(info.version, "25");
        assert_eq!(info.major, 25);
        assert_eq!(info.arch, Arch::X86_64);
        assert_eq!(info.vendor.as_deref(), Some("Oracle Corporation"));

        let java8 = "    java.version = 1.8.0_51\n    os.arch = x86\n    java.vendor = Oracle Corporation\n";
        let info = parse_java_properties(java8).unwrap();
        assert_eq!(info.major, 8);
        assert_eq!(info.arch, Arch::X86);

        let banner_only = "openjdk version \"17.0.2\" 2022-01-18\nOpenJDK Runtime Environment";
        assert_eq!(parse_java_properties(banner_only).unwrap().major, 17);
        assert!(parse_java_properties("command not found").is_none());
    }

    #[test]
    fn major_versions() {
        assert_eq!(parse_major("1.8.0_402"), Some(8));
        assert_eq!(parse_major("17.0.15"), Some(17));
        assert_eq!(parse_major("21"), Some(21));
        assert_eq!(parse_major("25-ea"), Some(25));
        assert_eq!(parse_major("8u51-cacert462b08"), Some(8));
        assert_eq!(parse_major("16.0.1.9.1"), Some(16));
        assert_eq!(parse_major("abc"), None);
    }

    fn inst(major: u32, arch: Arch) -> JavaInstallation {
        JavaInstallation {
            path: PathBuf::from(format!("/j{major}")),
            info: JavaInfo {
                version: major.to_string(),
                major,
                arch,
                vendor: None,
            },
            source: JavaSource::System,
        }
    }

    #[test]
    fn picks_matching_major() {
        let native = Arch::current();
        let all = vec![inst(8, native), inst(21, native), inst(25, native)];
        assert_eq!(pick_for_major(all.clone(), 8).unwrap().info.major, 8);
        assert_eq!(pick_for_major(all.clone(), 21).unwrap().info.major, 21);
        assert_eq!(pick_for_major(all.clone(), 17).unwrap().info.major, 21);
        assert!(pick_for_major(vec![inst(21, native)], 8).is_none());
        assert!(pick_for_major(all, 26).is_none());
    }

    #[tokio::test]
    async fn probing_a_missing_java_fails_cleanly() {
        let err = probe_java(Path::new("/definitely/not/java"))
            .await
            .unwrap_err();
        assert!(matches!(err, Error::Java(_)));
    }
}

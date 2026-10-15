//! Install processors (Forge 1.13+, NeoForge): the client data map, token substitution exactly
//! like the official installer (`{KEY}`, `'literal'`, `\` escapes, `[maven:coordinate]`) and
//! running each processor as `java -cp <jar + classpath> <Main-Class> <args>`.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use pl_net::CancellationToken;

use crate::error::{Error, Result};
use crate::loaders::forge::installer::{Processor, SidedValue};
use crate::paths::safe_join;
use crate::version::MavenCoord;

/// One processor may run for a while on slow disks (binary patching, remapping), but not forever.
const PROCESSOR_TIMEOUT: Duration = Duration::from_secs(30 * 60);
/// Lines of processor output kept in error messages.
const TAIL_LINES: usize = 25;
const TAIL_CHARS: usize = 4000;

/// A `data` value for the client side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DataValue {
    /// `[group:artifact:version[:classifier][@ext]]` → file in `libraries/`.
    Artifact(PathBuf),
    /// `'text'` → the text.
    Literal(String),
    /// Anything else names a file inside the installer (`/data/client.lzma`); the value is the
    /// entry name without the leading slash.
    InstallerFile(String),
}

/// The `[coordinate]` inside brackets, if `value` is one.
fn bracketed(value: &str) -> Option<&str> {
    value.strip_prefix('[')?.strip_suffix(']')
}

/// Path of a Maven coordinate in `libraries`.
pub(crate) fn artifact_path(coordinate: &str, libraries: &Path) -> Result<PathBuf> {
    let coord = MavenCoord::parse(coordinate)?;
    safe_join(libraries, &coord.path())
}

pub(crate) fn classify_data_value(value: &str, libraries: &Path) -> Result<DataValue> {
    if let Some(coordinate) = bracketed(value) {
        return Ok(DataValue::Artifact(artifact_path(coordinate, libraries)?));
    }
    if value.len() >= 2 && value.starts_with('\'') && value.ends_with('\'') {
        return Ok(DataValue::Literal(value[1..value.len() - 1].to_owned()));
    }
    let entry = value.trim_start_matches('/');
    if entry.is_empty() {
        return Err(Error::Invalid(format!("installer data value `{value}`")));
    }
    Ok(DataValue::InstallerFile(entry.to_owned()))
}

/// The client side of the `data` map, classified (keys without a client value are skipped).
pub(crate) fn client_data(
    data: &BTreeMap<String, SidedValue>,
    libraries: &Path,
) -> Result<Vec<(String, DataValue)>> {
    data.iter()
        .filter_map(|(key, value)| value.client.as_deref().map(|client| (key, client)))
        .map(|(key, client)| Ok((key.clone(), classify_data_value(client, libraries)?)))
        .collect()
}

/// Fixed values the official installer adds to the data map.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Environment<'a> {
    /// Data root (`{ROOT}`, the launcher's `.minecraft`).
    pub root: &'a Path,
    pub libraries: &'a Path,
    pub installer: &'a Path,
    /// Vanilla client jar (`versions/<mc>/<mc>.jar`).
    pub minecraft_jar: &'a Path,
    pub minecraft_version: &'a str,
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Token map for `{KEY}` substitution. `files` maps installer entry names to where they were
/// extracted.
pub(crate) fn build_tokens(
    data: &[(String, DataValue)],
    files: &HashMap<String, PathBuf>,
    env: &Environment<'_>,
) -> Result<HashMap<String, String>> {
    let mut tokens = HashMap::with_capacity(data.len() + 6);
    for (key, value) in data {
        let resolved = match value {
            DataValue::Artifact(path) => path_string(path),
            DataValue::Literal(text) => text.clone(),
            DataValue::InstallerFile(entry) => {
                let path = files
                    .get(entry)
                    .ok_or_else(|| Error::Invalid(format!("installer file `{entry}` missing")))?;
                path_string(path)
            }
        };
        tokens.insert(key.clone(), resolved);
    }
    tokens.insert("SIDE".into(), "client".into());
    tokens.insert("MINECRAFT_JAR".into(), path_string(env.minecraft_jar));
    tokens.insert("MINECRAFT_VERSION".into(), env.minecraft_version.to_owned());
    tokens.insert("ROOT".into(), path_string(env.root));
    tokens.insert("INSTALLER".into(), path_string(env.installer));
    tokens.insert("LIBRARY_DIR".into(), path_string(env.libraries));
    Ok(tokens)
}

/// `{KEY}` → token, `'text'` → text, `\x` → `x` (the installer's `Util.replaceTokens`).
pub(crate) fn replace_tokens(tokens: &HashMap<String, String>, value: &str) -> Result<String> {
    let invalid = |why: &str| Error::Invalid(format!("installer pattern `{value}`: {why}"));
    let chars: Vec<char> = value.chars().collect();
    let mut out = String::with_capacity(value.len());
    let mut index = 0;
    while index < chars.len() {
        let c = chars[index];
        if c == '\\' {
            let next = chars.get(index + 1).ok_or_else(|| invalid("bad escape"))?;
            out.push(*next);
            index += 2;
            continue;
        }
        if c == '{' || c == '\'' {
            let close = if c == '{' { '}' } else { '\'' };
            let mut key = String::new();
            let mut cursor = index + 1;
            let mut closed = false;
            while cursor < chars.len() {
                let d = chars[cursor];
                if d == '\\' {
                    let next = chars.get(cursor + 1).ok_or_else(|| invalid("bad escape"))?;
                    key.push(*next);
                    cursor += 2;
                    continue;
                }
                if d == close {
                    closed = true;
                    break;
                }
                key.push(d);
                cursor += 1;
            }
            if !closed {
                return Err(invalid(&format!("unclosed {c}")));
            }
            if c == '\'' {
                out.push_str(&key);
            } else {
                let token = tokens
                    .get(&key)
                    .ok_or_else(|| invalid(&format!("unknown key {key}")))?;
                out.push_str(token);
            }
            index = cursor + 1;
            continue;
        }
        out.push(c);
        index += 1;
    }
    Ok(out)
}

/// One processor ready to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Invocation {
    pub name: String,
    pub jar: PathBuf,
    /// Processor jar first, then its classpath (duplicates removed).
    pub classpath: Vec<PathBuf>,
    pub args: Vec<String>,
    /// Declared outputs and their expected SHA-1 (lowercase).
    pub outputs: Vec<(PathBuf, String)>,
}

/// Substitute a processor's jar, classpath, arguments and outputs.
pub(crate) fn plan_invocation(
    processor: &Processor,
    tokens: &HashMap<String, String>,
    libraries: &Path,
) -> Result<Invocation> {
    let jar = artifact_path(&processor.jar, libraries)?;
    let mut classpath = vec![jar.clone()];
    for entry in &processor.classpath {
        let path = artifact_path(entry, libraries)?;
        if !classpath.contains(&path) {
            classpath.push(path);
        }
    }
    let args = processor
        .args
        .iter()
        .map(|arg| match bracketed(arg) {
            Some(coordinate) => artifact_path(coordinate, libraries).map(|p| path_string(&p)),
            None => replace_tokens(tokens, arg),
        })
        .collect::<Result<Vec<_>>>()?;
    let outputs = processor
        .outputs
        .iter()
        .map(|(key, value)| {
            let path = match bracketed(key) {
                Some(coordinate) => artifact_path(coordinate, libraries)?,
                None => PathBuf::from(replace_tokens(tokens, key)?),
            };
            let sha1 = replace_tokens(tokens, value)?.trim().to_ascii_lowercase();
            Ok((path, sha1))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Invocation {
        name: processor.display_name(),
        jar,
        classpath,
        args,
        outputs,
    })
}

/// Last lines of the processor output (stderr, or stdout when stderr is empty).
pub(crate) fn output_tail(stderr: &[u8], stdout: &[u8]) -> String {
    let err = String::from_utf8_lossy(stderr);
    let text = if err.trim().is_empty() {
        String::from_utf8_lossy(stdout)
    } else {
        err
    };
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let tail = lines[lines.len().saturating_sub(TAIL_LINES)..].join("\n");
    let count = tail.chars().count();
    if count > TAIL_CHARS {
        tail.chars().skip(count - TAIL_CHARS).collect()
    } else {
        tail
    }
}

/// Run one processor with `java`; fails with [`Error::Process`] (including the output tail) on a
/// non-zero exit, a timeout or when Java cannot start.
pub(crate) async fn run_invocation(
    java: &Path,
    main_class: &str,
    invocation: &Invocation,
    separator: &str,
    cwd: &Path,
    cancel: &CancellationToken,
) -> Result<()> {
    let classpath = invocation
        .classpath
        .iter()
        .map(|p| path_string(p))
        .collect::<Vec<_>>()
        .join(separator);
    let mut command = tokio::process::Command::new(java);
    command
        .arg("-cp")
        .arg(&classpath)
        .arg(main_class)
        .args(&invocation.args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(crate::java::detect::CREATE_NO_WINDOW);
    tracing::debug!(
        processor = %invocation.name,
        java = %java.display(),
        %main_class,
        args = ?invocation.args,
        "running installer processor"
    );
    let child = command.spawn().map_err(|e| {
        Error::Process(format!(
            "cannot start {} for the installer step {}: {e}",
            java.display(),
            invocation.name
        ))
    })?;
    // Dropping the wait future (cancel/timeout) kills the process (`kill_on_drop`).
    let output = tokio::select! {
        _ = cancel.cancelled() => return Err(Error::Cancelled),
        result = tokio::time::timeout(PROCESSOR_TIMEOUT, child.wait_with_output()) => match result {
            Ok(Ok(output)) => output,
            Ok(Err(error)) => {
                return Err(Error::Process(format!(
                    "installer step {} could not be waited for: {error}",
                    invocation.name
                )));
            }
            Err(_) => {
                return Err(Error::Process(format!(
                    "installer step {} did not finish within {} minutes",
                    invocation.name,
                    PROCESSOR_TIMEOUT.as_secs() / 60
                )));
            }
        },
    };
    if !output.status.success() {
        let tail = output_tail(&output.stderr, &output.stdout);
        tracing::warn!(processor = %invocation.name, status = %output.status, %tail, "installer processor failed");
        return Err(Error::Process(format!(
            "installer step {} failed ({}):\n{tail}",
            invocation.name, output.status
        )));
    }
    tracing::debug!(
        processor = %invocation.name,
        output = %output_tail(&[], &output.stdout),
        "installer processor finished"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens() -> HashMap<String, String> {
        HashMap::from([
            ("MC_SLIM".to_owned(), "/lib/slim.jar".to_owned()),
            ("MC_SLIM_SHA".to_owned(), "DE86B035".to_owned()),
            ("SIDE".to_owned(), "client".to_owned()),
        ])
    }

    #[test]
    fn token_substitution_matches_the_installer() {
        let t = tokens();
        assert_eq!(replace_tokens(&t, "{MC_SLIM}").unwrap(), "/lib/slim.jar");
        assert_eq!(
            replace_tokens(&t, "--side={SIDE}/x").unwrap(),
            "--side=client/x"
        );
        assert_eq!(replace_tokens(&t, "'{SIDE}'").unwrap(), "{SIDE}");
        assert_eq!(replace_tokens(&t, r"a\{b\}").unwrap(), "a{b}");
        assert_eq!(replace_tokens(&t, r"{MC\_SLIM}").unwrap(), "/lib/slim.jar");
        assert_eq!(replace_tokens(&t, "plain").unwrap(), "plain");
        assert!(matches!(
            replace_tokens(&t, "{NOPE}"),
            Err(Error::Invalid(_))
        ));
        assert!(replace_tokens(&t, "{SIDE").is_err());
        assert!(replace_tokens(&t, "'open").is_err());
        assert!(replace_tokens(&t, "trailing\\").is_err());
    }

    #[test]
    fn data_values_and_tokens() {
        let libraries = Path::new("/data/libraries");
        assert_eq!(
            classify_data_value(
                "[net.minecraft:client:1.20.1-20230612.114412:mappings@txt]",
                libraries
            )
            .unwrap(),
            DataValue::Artifact(libraries.join(
                "net/minecraft/client/1.20.1-20230612.114412/client-1.20.1-20230612.114412-mappings.txt"
            ))
        );
        assert_eq!(
            classify_data_value("'20230612.114412'", libraries).unwrap(),
            DataValue::Literal("20230612.114412".into())
        );
        assert_eq!(
            classify_data_value("/data/client.lzma", libraries).unwrap(),
            DataValue::InstallerFile("data/client.lzma".into())
        );
        assert!(classify_data_value("[../evil:x:1]", libraries).is_err());
        assert!(classify_data_value("/", libraries).is_err());

        let data: BTreeMap<String, SidedValue> = serde_json::from_str(
            r#"{"BINPATCH": {"client": "/data/client.lzma", "server": "/data/server.lzma"},
                "MCP_VERSION": {"client": "'20230612.114412'", "server": "'20230612.114412'"},
                "SERVER_ONLY": {"server": "'x'"},
                "PATCHED": {"client": "[net.minecraftforge:forge:1.20.1-47.4.10:client]",
                            "server": "[net.minecraftforge:forge:1.20.1-47.4.10:server]"}}"#,
        )
        .unwrap();
        let client = client_data(&data, libraries).unwrap();
        assert_eq!(client.len(), 3);
        let files = HashMap::from([(
            "data/client.lzma".to_owned(),
            PathBuf::from("/work/data/client.lzma"),
        )]);
        let env = Environment {
            root: Path::new("/data"),
            libraries,
            installer: Path::new("/data/libraries/forge-installer.jar"),
            minecraft_jar: Path::new("/data/versions/1.20.1/1.20.1.jar"),
            minecraft_version: "1.20.1",
        };
        let tokens = build_tokens(&client, &files, &env).unwrap();
        assert_eq!(
            tokens["BINPATCH"],
            path_string(Path::new("/work/data/client.lzma"))
        );
        assert_eq!(tokens["MCP_VERSION"], "20230612.114412");
        assert_eq!(
            PathBuf::from(&tokens["PATCHED"]),
            libraries
                .join("net/minecraftforge/forge/1.20.1-47.4.10/forge-1.20.1-47.4.10-client.jar")
        );
        assert_eq!(tokens["SIDE"], "client");
        assert_eq!(tokens["MINECRAFT_VERSION"], "1.20.1");
        assert_eq!(tokens["ROOT"], path_string(Path::new("/data")));
        assert!(!tokens.contains_key("SERVER_ONLY"));
        assert!(build_tokens(&client, &HashMap::new(), &env).is_err());
    }

    #[test]
    fn processor_invocation() {
        let libraries = Path::new("/data/libraries");
        let processor: Processor = serde_json::from_str(
            r#"{"jar": "net.minecraftforge:jarsplitter:1.1.4",
                "classpath": ["net.sf.jopt-simple:jopt-simple:5.0.4", "net.minecraftforge:jarsplitter:1.1.4", "net.minecraftforge:srgutils:0.4.3"],
                "args": ["--input", "{MC_SLIM}", "--srg", "[de.oceanlabs.mcp:mcp_config:1.20.1-20230612.114412@zip]", "--side", "{SIDE}", "'{literal}'"],
                "outputs": {"{MC_SLIM}": "{MC_SLIM_SHA}", "[net.minecraft:client:1.20.1:extra]": "'ABCDEF'"}}"#,
        )
        .unwrap();
        let invocation = plan_invocation(&processor, &tokens(), libraries).unwrap();
        assert_eq!(invocation.name, "jarsplitter");
        assert_eq!(
            invocation.jar,
            libraries.join("net/minecraftforge/jarsplitter/1.1.4/jarsplitter-1.1.4.jar")
        );
        assert_eq!(invocation.classpath.len(), 3, "the jar is listed once");
        assert_eq!(invocation.args.len(), 7);
        assert_eq!(invocation.args[..3], ["--input", "/lib/slim.jar", "--srg"]);
        assert_eq!(
            PathBuf::from(&invocation.args[3]),
            libraries.join(
                "de/oceanlabs/mcp/mcp_config/1.20.1-20230612.114412/mcp_config-1.20.1-20230612.114412.zip"
            )
        );
        assert_eq!(invocation.args[4..], ["--side", "client", "{literal}"]);
        // Outputs come in key order (`[` sorts before `{`).
        assert_eq!(
            invocation.outputs,
            [
                (
                    libraries.join("net/minecraft/client/1.20.1/client-1.20.1-extra.jar"),
                    "abcdef".to_owned()
                ),
                (PathBuf::from("/lib/slim.jar"), "de86b035".to_owned()),
            ]
        );
        let unknown: Processor =
            serde_json::from_str(r#"{"jar": "a:b:1", "args": ["{MISSING}"]}"#).unwrap();
        assert!(plan_invocation(&unknown, &tokens(), libraries).is_err());
    }

    #[test]
    fn output_tail_prefers_stderr() {
        let stdout = b"line 1\nline 2\n";
        assert_eq!(output_tail(b"", stdout), "line 1\nline 2");
        assert_eq!(output_tail(b"boom\n", stdout), "boom");
        let long: String = (0..100).map(|i| format!("l{i}\n")).collect();
        let tail = output_tail(long.as_bytes(), b"");
        assert_eq!(tail.lines().count(), TAIL_LINES);
        assert!(tail.ends_with("l99"));
    }
}

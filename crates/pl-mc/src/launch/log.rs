//! Game output parsing: the log4j2 XML events Mojang's logging configs print to stdout
//! (`<log4j:Event ...>` spanning several lines, CDATA message and throwable) and plain console
//! lines (`[12:34:56] [Render thread/INFO]: ...`, Forge/Fabric/NeoForge variants, JVM errors).

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

/// Events larger than this are given up on and emitted as plain text.
const MAX_EVENT_BYTES: usize = 4 * 1024 * 1024;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "camelCase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

impl LogLevel {
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_uppercase().as_str() {
            "TRACE" | "FINEST" | "FINER" => Some(Self::Trace),
            "DEBUG" | "FINE" | "CONFIG" => Some(Self::Debug),
            "INFO" => Some(Self::Info),
            "WARN" | "WARNING" => Some(Self::Warn),
            "ERROR" | "SEVERE" => Some(Self::Error),
            "FATAL" => Some(Self::Fatal),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Trace => "TRACE",
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
            Self::Fatal => "FATAL",
        }
    }
}

/// One parsed log entry (a stack trace stays inside `message`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    /// Local wall-clock time `HH:MM:SS` when known.
    pub time: Option<String>,
    pub level: LogLevel,
    pub thread: Option<String>,
    pub logger: Option<String>,
    pub message: String,
}

impl LogLine {
    /// Plain text line with a guessed level.
    pub fn plain(text: &str) -> Self {
        parse_plain_line(text)
    }

    /// `[time] [thread/LEVEL] [logger]: message` (plain lines keep their original text).
    pub fn format(&self) -> String {
        let Some(thread) = &self.thread else {
            return match &self.time {
                Some(time) => format!("[{time}] [{}]: {}", self.level.as_str(), self.message),
                None => self.message.clone(),
            };
        };
        let mut out = String::with_capacity(self.message.len() + 48);
        if let Some(time) = &self.time {
            out.push('[');
            out.push_str(time);
            out.push_str("] ");
        }
        out.push('[');
        out.push_str(thread);
        out.push('/');
        out.push_str(self.level.as_str());
        out.push(']');
        if let Some(logger) = &self.logger {
            out.push_str(" [");
            out.push_str(logger);
            out.push(']');
        }
        out.push_str(": ");
        out.push_str(&self.message);
        out
    }
}

/// Incremental parser: feed raw output lines, get finished entries back.
#[derive(Debug, Default)]
pub struct Log4jParser {
    buffer: String,
    in_event: bool,
}

impl Log4jParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one line (without the trailing newline).
    pub fn push_line(&mut self, line: &str) -> Vec<LogLine> {
        if self.in_event {
            self.buffer.push_str(line);
            self.buffer.push('\n');
            if line.contains("</log4j:Event>") {
                self.in_event = false;
                return self.take_event();
            }
            if self.buffer.len() > MAX_EVENT_BYTES {
                self.in_event = false;
                return self.take_as_plain();
            }
            return Vec::new();
        }
        let trimmed = line.trim_start();
        if trimmed.starts_with("<log4j:Event") {
            self.buffer.clear();
            self.buffer.push_str(trimmed);
            self.buffer.push('\n');
            if trimmed.contains("</log4j:Event>") {
                return self.take_event();
            }
            self.in_event = true;
            return Vec::new();
        }
        if line.trim().is_empty() {
            return Vec::new();
        }
        vec![parse_plain_line(line)]
    }

    /// Flush a half-finished event when the stream ends.
    pub fn finish(&mut self) -> Vec<LogLine> {
        if self.in_event {
            self.in_event = false;
            return self.take_as_plain();
        }
        Vec::new()
    }

    fn take_event(&mut self) -> Vec<LogLine> {
        let xml = std::mem::take(&mut self.buffer);
        match parse_event(&xml) {
            Some(line) => vec![line],
            None => xml
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(parse_plain_line)
                .collect(),
        }
    }

    fn take_as_plain(&mut self) -> Vec<LogLine> {
        let text = std::mem::take(&mut self.buffer);
        text.lines()
            .filter(|l| !l.trim().is_empty())
            .map(parse_plain_line)
            .collect()
    }
}

/// Parse one complete `<log4j:Event ...>...</log4j:Event>` block.
pub fn parse_event(xml: &str) -> Option<LogLine> {
    let start = xml.find("<log4j:Event")?;
    let head_end = start + xml[start..].find('>')?;
    let attributes = parse_attributes(&xml[start..head_end]);
    let message = element_text(xml, "log4j:Message").unwrap_or_default();
    let throwable = element_text(xml, "log4j:Throwable").filter(|t| !t.trim().is_empty());
    let level = attributes
        .get("level")
        .and_then(|l| LogLevel::parse(l))
        .unwrap_or(LogLevel::Info);
    let time = attributes
        .get("timestamp")
        .and_then(|t| t.trim().parse::<i64>().ok())
        .and_then(format_millis);
    let mut message = message.trim_end_matches(['\r', '\n']).to_owned();
    if let Some(throwable) = throwable {
        if !message.is_empty() {
            message.push('\n');
        }
        message.push_str(throwable.trim_end_matches(['\r', '\n']));
    }
    Some(LogLine {
        time,
        level,
        thread: attributes.get("thread").cloned(),
        logger: attributes.get("logger").cloned().filter(|l| !l.is_empty()),
        message,
    })
}

fn parse_attributes(head: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut rest = head;
    while let Some(eq) = rest.find("=\"") {
        let name = rest[..eq]
            .rsplit(char::is_whitespace)
            .next()
            .unwrap_or_default()
            .trim()
            .to_owned();
        let after = &rest[eq + 2..];
        let Some(end) = after.find('"') else {
            break;
        };
        out.insert(name, unescape_xml(&after[..end]));
        rest = &after[end + 1..];
    }
    out
}

/// Text of `<tag>...</tag>`: CDATA sections verbatim, other text unescaped. A literal
/// `</tag>` inside CDATA does not end the element.
fn element_text(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut rest = &xml[xml.find(&open)? + open.len()..];
    let mut out = String::new();
    let mut text = String::new();
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("<![CDATA[") {
            out.push_str(&unescape_xml(&std::mem::take(&mut text)));
            match after.find("]]>") {
                Some(end) => {
                    out.push_str(&after[..end]);
                    rest = &after[end + 3..];
                }
                None => {
                    out.push_str(after);
                    rest = "";
                }
            }
        } else if rest.starts_with(close.as_str()) {
            break;
        } else {
            match rest[1..].find('<') {
                Some(next) => {
                    text.push_str(&rest[..next + 1]);
                    rest = &rest[next + 1..];
                }
                None => {
                    text.push_str(rest);
                    rest = "";
                }
            }
        }
    }
    out.push_str(&unescape_xml(&text));
    Some(out)
}

fn unescape_xml(text: &str) -> String {
    if !text.contains('&') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp + 1..];
        let Some(semi) = after.find(';').filter(|s| *s <= 10) else {
            out.push('&');
            rest = after;
            continue;
        };
        let entity = &after[..semi];
        let decoded = match entity {
            "lt" => Some('<'),
            "gt" => Some('>'),
            "amp" => Some('&'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|d| d.parse().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &after[semi + 1..];
            }
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn format_millis(millis: i64) -> Option<String> {
    let ts = jiff::Timestamp::from_millisecond(millis).ok()?;
    Some(
        ts.to_zoned(jiff::tz::TimeZone::system())
            .strftime("%H:%M:%S")
            .to_string(),
    )
}

/// `[12:34:56] [Render thread/INFO]: msg`, `[12:34:56] [main/INFO] [cpw.mods/MODLAUNCHER]: msg`,
/// `[12:34:56] [main/INFO] (FabricLoader) msg`, `[15Oct2026 12:34:56.789] [main/INFO] [x/]: msg`.
static BRACKETED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^\[(?:[^\]]*?\s)?(?P<time>\d{1,2}:\d{2}:\d{2})(?:[.,]\d+)?\] \[(?P<thread>[^\]]*?)/(?P<level>[A-Za-z]+)\](?: \[(?P<logger>[^\]]*)\]| \((?P<logger2>[^)]*)\))?:? ?(?P<msg>.*)$",
    )
    .expect("valid regex")
});

/// `[12:34:56 INFO]: msg` (server style) and `2013-07-01 12:00:00 [INFO] msg` (1.6 and older).
static SIMPLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(?:\[(?P<t1>\d{1,2}:\d{2}:\d{2}) (?P<l1>[A-Z]+)\]:?|(?:\d{4}-\d{2}-\d{2} )(?P<t2>\d{2}:\d{2}:\d{2}) \[(?P<l2>[A-Z]+)\]) ?(?P<msg>.*)$",
    )
    .expect("valid regex")
});

/// Parse a console line; unknown formats keep the whole text with a guessed level.
pub fn parse_plain_line(line: &str) -> LogLine {
    let line = line.trim_end_matches(['\r', '\n']);
    if let Some(c) = BRACKETED.captures(line)
        && let Some(level) = c.name("level").and_then(|l| LogLevel::parse(l.as_str()))
    {
        let logger = c
            .name("logger")
            .or_else(|| c.name("logger2"))
            .map(|l| l.as_str().to_owned())
            .filter(|l| !l.is_empty());
        return LogLine {
            time: c.name("time").map(|t| t.as_str().to_owned()),
            level,
            thread: c.name("thread").map(|t| t.as_str().to_owned()),
            logger,
            message: c
                .name("msg")
                .map(|m| m.as_str().to_owned())
                .unwrap_or_default(),
        };
    }
    if let Some(c) = SIMPLE.captures(line)
        && let Some(level) = c
            .name("l1")
            .or_else(|| c.name("l2"))
            .and_then(|l| LogLevel::parse(l.as_str()))
    {
        return LogLine {
            time: c
                .name("t1")
                .or_else(|| c.name("t2"))
                .map(|t| t.as_str().to_owned()),
            level,
            thread: None,
            logger: None,
            message: c
                .name("msg")
                .map(|m| m.as_str().to_owned())
                .unwrap_or_default(),
        };
    }
    LogLine {
        time: None,
        level: guess_level(line),
        thread: None,
        logger: None,
        message: line.to_owned(),
    }
}

/// Level of an unstructured line (JVM errors, stack traces, `System.out` prints).
pub fn guess_level(line: &str) -> LogLevel {
    let trimmed = line.trim_start();
    let upper_words = |word: &str| line.contains(word);
    if upper_words("FATAL") || line.contains("#@!@#") {
        LogLevel::Fatal
    } else if upper_words("ERROR")
        || trimmed.starts_with("at ")
        || trimmed.starts_with("Caused by:")
        || trimmed.starts_with("Exception in thread")
        || trimmed.starts_with("Error:")
        || trimmed.starts_with("... ")
        || line.contains("Exception:")
        || line.contains("Error occurred")
    {
        LogLevel::Error
    } else if upper_words("WARN") || trimmed.starts_with("Warning:") {
        LogLevel::Warn
    } else if upper_words("DEBUG") {
        LogLevel::Debug
    } else {
        LogLevel::Info
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multi_line_events_with_throwable() {
        let mut parser = Log4jParser::new();
        let lines = [
            r#"<log4j:Event logger="net.minecraft.client.Minecraft" timestamp="1697040000000" level="INFO" thread="Render thread">"#,
            r#"  <log4j:Message><![CDATA[Setting user: Steve]]></log4j:Message>"#,
            r#"</log4j:Event>"#,
            r#"<log4j:Event logger="xyz" timestamp="1697040001000" level="ERROR" thread="Worker &quot;1&quot;">"#,
            r#"  <log4j:Message><![CDATA[Something failed: a < b & </log4j:Message> stays]]></log4j:Message>"#,
            r#"  <log4j:Throwable><![CDATA[java.lang.RuntimeException: boom"#,
            "\tat a.b.C.d(C.java:1)",
            r#"]]></log4j:Throwable>"#,
            r#"</log4j:Event>"#,
        ];
        let mut out = Vec::new();
        for line in lines {
            out.extend(parser.push_line(line));
        }
        out.extend(parser.finish());
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].level, LogLevel::Info);
        assert_eq!(out[0].message, "Setting user: Steve");
        assert_eq!(out[0].thread.as_deref(), Some("Render thread"));
        assert_eq!(
            out[0].logger.as_deref(),
            Some("net.minecraft.client.Minecraft")
        );
        assert_eq!(out[0].time.as_deref().map(str::len), Some(8));
        assert_eq!(out[1].level, LogLevel::Error);
        assert_eq!(out[1].thread.as_deref(), Some("Worker \"1\""));
        assert!(
            out[1]
                .message
                .starts_with("Something failed: a < b & </log4j:Message> stays\n")
        );
        assert!(
            out[1]
                .message
                .contains("java.lang.RuntimeException: boom\n\tat a.b.C.d")
        );
    }

    #[test]
    fn plain_lines_interleaved() {
        let mut parser = Log4jParser::new();
        let mut out = parser.push_line("Picked up _JAVA_OPTIONS: -Xmx1G");
        out.extend(parser.push_line(r#"<log4j:Event logger="a" timestamp="1" level="WARN" thread="main"><log4j:Message><![CDATA[one line]]></log4j:Message></log4j:Event>"#));
        out.extend(parser.push_line(""));
        out.extend(
            parser.push_line("Exception in thread \"main\" java.lang.NoClassDefFoundError: x"),
        );
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].level, LogLevel::Info);
        assert_eq!(out[1].level, LogLevel::Warn);
        assert_eq!(out[1].message, "one line");
        assert_eq!(out[2].level, LogLevel::Error);
    }

    #[test]
    fn unterminated_event_is_flushed_as_text() {
        let mut parser = Log4jParser::new();
        assert!(
            parser
                .push_line(r#"<log4j:Event logger="a" level="INFO" thread="t">"#)
                .is_empty()
        );
        assert!(parser.push_line("<log4j:Message><![CDATA[cut").is_empty());
        let out = parser.finish();
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn console_formats() {
        let vanilla = parse_plain_line(
            "[12:34:56] [Render thread/INFO]: Backend library: LWJGL version 3.3.3",
        );
        assert_eq!(vanilla.time.as_deref(), Some("12:34:56"));
        assert_eq!(vanilla.thread.as_deref(), Some("Render thread"));
        assert_eq!(vanilla.level, LogLevel::Info);
        assert_eq!(vanilla.message, "Backend library: LWJGL version 3.3.3");
        assert_eq!(
            vanilla.format(),
            "[12:34:56] [Render thread/INFO]: Backend library: LWJGL version 3.3.3"
        );

        let forge = parse_plain_line(
            "[12:34:56] [main/WARN] [cpw.mods.modlauncher.Launcher/MODLAUNCHER]: Hello",
        );
        assert_eq!(forge.level, LogLevel::Warn);
        assert_eq!(
            forge.logger.as_deref(),
            Some("cpw.mods.modlauncher.Launcher/MODLAUNCHER")
        );
        assert_eq!(forge.message, "Hello");

        let fabric = parse_plain_line("[12:34:56] [main/INFO] (FabricLoader) Loading 42 mods:");
        assert_eq!(fabric.logger.as_deref(), Some("FabricLoader"));
        assert_eq!(fabric.message, "Loading 42 mods:");

        let neo = parse_plain_line(
            "[15Oct2026 12:34:56.789] [main/ERROR] [net.neoforged.fml.loading.FMLLoader/CORE]: Bad",
        );
        assert_eq!(neo.time.as_deref(), Some("12:34:56"));
        assert_eq!(neo.level, LogLevel::Error);

        let old = parse_plain_line("2013-07-01 12:00:00 [SEVERE] Unable to launch");
        assert_eq!(old.level, LogLevel::Error);
        assert_eq!(old.message, "Unable to launch");

        let jvm = parse_plain_line("Error: Could not create the Java Virtual Machine.");
        assert_eq!(jvm.level, LogLevel::Error);
        assert_eq!(
            parse_plain_line("\tat net.minecraft.Main.main(Main.java:1)").level,
            LogLevel::Error
        );
        assert_eq!(parse_plain_line("hello world").level, LogLevel::Info);
        assert_eq!(parse_plain_line("[WARNING] careful").level, LogLevel::Warn);
    }

    #[test]
    fn entity_unescaping() {
        assert_eq!(
            unescape_xml("a &lt;b&gt; &amp; &#65;&#x42; &bogus; & x"),
            "a <b> & AB &bogus; & x"
        );
    }
}

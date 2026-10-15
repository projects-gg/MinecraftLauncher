//! Maven coordinates (`group:artifact:version[:classifier][@extension]`) and their repository paths.

use std::fmt;
use std::str::FromStr;

use crate::error::{Error, Result};

/// Mojang's library repository, also the fallback for loader libraries.
pub const MOJANG_LIBRARIES_URL: &str = "https://libraries.minecraft.net/";
/// Last-resort repository for loader libraries that moved off their original host.
pub const MAVEN_CENTRAL_URL: &str = "https://repo1.maven.org/maven2/";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MavenCoord {
    pub group: String,
    pub artifact: String,
    pub version: String,
    pub classifier: Option<String>,
    /// File extension without the dot; `jar` unless the coordinate says `@zip` etc.
    pub extension: String,
}

impl MavenCoord {
    pub fn parse(coord: &str) -> Result<Self> {
        let invalid = || Error::Invalid(format!("maven coordinate `{coord}`"));
        let trimmed = coord.trim();
        let (main, extension) = match trimmed.rsplit_once('@') {
            Some((main, ext)) if !ext.is_empty() => (main, ext),
            Some(_) => return Err(invalid()),
            None => (trimmed, "jar"),
        };
        let parts: Vec<&str> = main.split(':').collect();
        let (group, artifact, version, classifier) = match parts.as_slice() {
            [g, a, v] => (*g, *a, *v, None),
            [g, a, v, c] => (*g, *a, *v, Some(*c)),
            _ => return Err(invalid()),
        };
        let segment_ok =
            |s: &str| !s.is_empty() && s != "." && s != ".." && !s.contains(['/', '\\', '\0', ' ']);
        let group_ok = group.split('.').all(segment_ok);
        let rest_ok = [artifact, version, extension]
            .into_iter()
            .chain(classifier)
            .all(segment_ok);
        if !group_ok || !rest_ok {
            return Err(invalid());
        }
        Ok(Self {
            group: group.to_owned(),
            artifact: artifact.to_owned(),
            version: version.to_owned(),
            classifier: classifier.map(str::to_owned),
            extension: extension.to_owned(),
        })
    }

    /// `artifact-version[-classifier].ext`
    pub fn file_name(&self) -> String {
        match &self.classifier {
            Some(c) => format!(
                "{}-{}-{}.{}",
                self.artifact, self.version, c, self.extension
            ),
            None => format!("{}-{}.{}", self.artifact, self.version, self.extension),
        }
    }

    /// Repository-relative path with `/` separators:
    /// `org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar`.
    pub fn path(&self) -> String {
        format!(
            "{}/{}/{}/{}",
            self.group.replace('.', "/"),
            self.artifact,
            self.version,
            self.file_name()
        )
    }

    /// Full URL in a Maven repository (`base` with or without a trailing slash).
    pub fn url(&self, base: &str) -> String {
        format!("{}/{}", base.trim_end_matches('/'), self.path())
    }

    /// Identity used to deduplicate libraries across `inheritsFrom` levels:
    /// `group:artifact[:classifier]` (the version is deliberately left out).
    pub fn dedup_key(&self) -> String {
        match &self.classifier {
            Some(c) => format!("{}:{}:{}", self.group, self.artifact, c),
            None => format!("{}:{}", self.group, self.artifact),
        }
    }

    pub fn with_classifier(&self, classifier: &str) -> Self {
        Self {
            classifier: Some(classifier.to_owned()),
            ..self.clone()
        }
    }
}

impl FromStr for MavenCoord {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

impl fmt::Display for MavenCoord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.group, self.artifact, self.version)?;
        if let Some(c) = &self.classifier {
            write!(f, ":{c}")?;
        }
        if self.extension != "jar" {
            write!(f, "@{}", self.extension)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_coordinate() {
        let c = MavenCoord::parse("org.ow2.asm:asm:9.7").unwrap();
        assert_eq!(c.path(), "org/ow2/asm/asm/9.7/asm-9.7.jar");
        assert_eq!(c.dedup_key(), "org.ow2.asm:asm");
        assert_eq!(c.to_string(), "org.ow2.asm:asm:9.7");
        assert_eq!(
            c.url("https://maven.fabricmc.net/"),
            "https://maven.fabricmc.net/org/ow2/asm/asm/9.7/asm-9.7.jar"
        );
        assert_eq!(
            c.url("https://libraries.minecraft.net"),
            "https://libraries.minecraft.net/org/ow2/asm/asm/9.7/asm-9.7.jar"
        );
    }

    #[test]
    fn classifier_and_extension() {
        let c = MavenCoord::parse("org.lwjgl:lwjgl:3.3.3:natives-windows").unwrap();
        assert_eq!(
            c.path(),
            "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar"
        );
        assert_eq!(c.dedup_key(), "org.lwjgl:lwjgl:natives-windows");

        let z =
            MavenCoord::parse("de.oceanlabs.mcp:mcp_config:1.20.1-20230612.114412@zip").unwrap();
        assert_eq!(
            z.path(),
            "de/oceanlabs/mcp/mcp_config/1.20.1-20230612.114412/mcp_config-1.20.1-20230612.114412.zip"
        );
        assert_eq!(
            z.to_string(),
            "de.oceanlabs.mcp:mcp_config:1.20.1-20230612.114412@zip"
        );

        let both =
            MavenCoord::parse("net.minecraft:client:1.20.1-20230612.114412:srg@txt").unwrap();
        assert_eq!(both.classifier.as_deref(), Some("srg"));
        assert_eq!(both.extension, "txt");
        assert_eq!(both.file_name(), "client-1.20.1-20230612.114412-srg.txt");

        let legacy = MavenCoord::parse("tv.twitch:twitch-platform:5.16")
            .unwrap()
            .with_classifier("natives-windows-64");
        assert_eq!(
            legacy.path(),
            "tv/twitch/twitch-platform/5.16/twitch-platform-5.16-natives-windows-64.jar"
        );
        let plus = MavenCoord::parse("net.fabricmc.fabric-api:fabric-api:0.92.2+1.20.1").unwrap();
        assert_eq!(plus.file_name(), "fabric-api-0.92.2+1.20.1.jar");
    }

    #[test]
    fn rejects_bad_coordinates() {
        for bad in [
            "",
            "a:b",
            "a:b:c:d:e",
            "a::c",
            "a:b:c@",
            "../x:y:z",
            "a:b:../../c",
            "a..b:c:d",
            "a:b:c/d",
        ] {
            assert!(MavenCoord::parse(bad).is_err(), "{bad} should be rejected");
        }
    }
}

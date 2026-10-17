//! Whether the friends feature is on, and which service it talks to.
//!
//! The feature is optional and off by default: the signed launcher config turns it on
//! (`features.friends`). Development builds may point it at a local `wrangler dev` instead
//! (the launcher reads `PROJECTS_SOCIAL_URL` in debug builds only and passes it here).

use std::net::IpAddr;

use url::{Host, Url};

use crate::client::PRODUCTION_URL;

/// The service to use, or None while the feature is off.
///
/// A valid `override_url` forces the feature on and replaces the production service; `http`
/// is accepted only for loopback hosts, and URLs with credentials, a query or a fragment are
/// ignored (then the remote flag decides).
pub fn service_url(remote_flag: bool, override_url: Option<&str>) -> Option<Url> {
    if let Some(url) = override_url.and_then(parse_override) {
        return Some(url);
    }
    remote_flag.then(|| Url::parse(PRODUCTION_URL).expect("the production URL is valid"))
}

fn parse_override(raw: &str) -> Option<Url> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let mut url = Url::parse(raw).ok()?;
    let loopback = match url.host()? {
        Host::Domain(name) => name.eq_ignore_ascii_case("localhost"),
        Host::Ipv4(ip) => IpAddr::V4(ip).is_loopback(),
        Host::Ipv6(ip) => IpAddr::V6(ip).is_loopback(),
    };
    let allowed = match url.scheme() {
        "https" => true,
        "http" => loopback,
        _ => false,
    };
    if !allowed
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }
    Some(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_unless_the_config_turns_it_on() {
        assert_eq!(service_url(false, None), None);
        assert_eq!(
            service_url(true, None).unwrap().as_str(),
            "https://social.projects.gg/"
        );
    }

    #[test]
    fn a_development_override_forces_it_on() {
        assert_eq!(
            service_url(false, Some("http://127.0.0.1:8797"))
                .unwrap()
                .as_str(),
            "http://127.0.0.1:8797/"
        );
        assert_eq!(
            service_url(true, Some(" http://localhost:8797/ "))
                .unwrap()
                .as_str(),
            "http://localhost:8797/"
        );
        assert_eq!(
            service_url(false, Some("https://social.example.test/api"))
                .unwrap()
                .as_str(),
            "https://social.example.test/api/"
        );
    }

    #[test]
    fn unsafe_or_broken_overrides_are_ignored() {
        for bad in [
            "",
            "   ",
            "not a url",
            "http://social.example.test/",
            "ftp://127.0.0.1/",
            "http://user:pw@127.0.0.1:8797/",
            "http://127.0.0.1:8797/?x=1",
            "http://127.0.0.1:8797/#x",
        ] {
            assert_eq!(service_url(false, Some(bad)), None, "{bad}");
            // The remote flag still decides when the override is unusable.
            assert_eq!(
                service_url(true, Some(bad)).unwrap().as_str(),
                "https://social.projects.gg/",
                "{bad}"
            );
        }
        assert!(service_url(false, Some("http://[::1]:8797/")).is_some());
    }
}

//! Server addresses as the service takes them (the contract's "Server address"): sent already
//! normalized (lowercase ASCII, IDNs as punycode, no trailing dot), because the host is part of
//! every seal's AAD and the service never rewrites it. Private and reserved addresses and
//! sensitive ports are refused here before the service refuses them.

use std::net::Ipv4Addr;

use serde::{Deserialize, Serialize};

use crate::model::CloudServer;

/// Why an address cannot be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum CloudAddressProblem {
    Empty,
    /// Not a host name or an IPv4 address (IPv6 included: the machines have none).
    Format,
    /// A host name without a dot (`localhost` aside, which is `Private`).
    Dot,
    /// A private, local or reserved address.
    Private,
    /// A port the machines never connect to.
    Port,
}

const PRIVATE_SUFFIXES: &[&str] = &[
    ".localhost",
    ".local",
    ".lan",
    ".internal",
    ".home.arpa",
    ".invalid",
    ".test",
    ".example",
    ".onion",
];

const REFUSED_PORTS: &[u16] = &[
    1433, 1521, 2375, 2376, 3306, 3389, 5432, 5900, 5984, 6379, 6443, 8086, 9042, 9092, 9200, 9300,
    11211, 27017,
];

/// The host the service stores for `input` (trimmed, lowercase, punycode, no trailing dot), or why
/// it cannot be one.
pub fn normalize_host(input: &str) -> Result<String, CloudAddressProblem> {
    let raw = input.trim();
    if raw.is_empty() {
        return Err(CloudAddressProblem::Empty);
    }
    if raw.contains(['/', '@', '?', '#', ' ', '[', ']', ':', '\\']) {
        return Err(CloudAddressProblem::Format);
    }
    let host = match url::Host::parse(raw) {
        Ok(url::Host::Domain(name)) => name,
        Ok(url::Host::Ipv4(ip)) => ip.to_string(),
        Ok(url::Host::Ipv6(_)) | Err(_) => return Err(CloudAddressProblem::Format),
    };
    let host = host.strip_suffix('.').unwrap_or(&host).to_owned();
    if host.is_empty() || host.len() > 253 {
        return Err(CloudAddressProblem::Format);
    }
    if !host
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'-' | b'_'))
    {
        return Err(CloudAddressProblem::Format);
    }
    if host.starts_with(['.', '-'])
        || host
            .split('.')
            .any(|label| label.is_empty() || label.len() > 63)
    {
        return Err(CloudAddressProblem::Format);
    }
    if let Some(ip) = ipv4_literal(&host) {
        return if is_private_ipv4(ip) {
            Err(CloudAddressProblem::Private)
        } else {
            Ok(host)
        };
    }
    if host == "localhost" || PRIVATE_SUFFIXES.iter().any(|suffix| host.ends_with(suffix)) {
        return Err(CloudAddressProblem::Private);
    }
    let last = host.rsplit('.').next().unwrap_or(&host);
    if !host.contains('.') {
        return Err(CloudAddressProblem::Dot);
    }
    if last.bytes().all(|b| b.is_ascii_digit()) {
        return Err(CloudAddressProblem::Format);
    }
    Ok(host)
}

/// Four decimal numbers 0-255 without leading zeros.
fn ipv4_literal(host: &str) -> Option<Ipv4Addr> {
    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() != 4 {
        return None;
    }
    let mut octets = [0u8; 4];
    for (octet, part) in octets.iter_mut().zip(&parts) {
        if part.is_empty()
            || part.len() > 3
            || !part.bytes().all(|b| b.is_ascii_digit())
            || (part.len() > 1 && part.starts_with('0'))
        {
            return None;
        }
        *octet = part.parse().ok()?;
    }
    Some(Ipv4Addr::from(octets))
}

fn is_private_ipv4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    matches!(
        (a, b, c),
        (0, _, _)
            | (10, _, _)
            | (127, _, _)
            | (169, 254, _)
            | (192, 168, _)
            | (192, 0, 0)
            | (192, 0, 2)
            | (192, 88, 99)
            | (198, 51, 100)
            | (203, 0, 113)
    ) || (a == 100 && (64..=127).contains(&b))
        || (a == 172 && (16..=31).contains(&b))
        || (a == 198 && (18..=19).contains(&b))
        || a >= 224
}

/// Whether the machines would connect to `port` (0: the client finds it).
pub fn check_port(port: u16) -> Result<u16, CloudAddressProblem> {
    if (1..=1023).contains(&port) || REFUSED_PORTS.contains(&port) {
        Err(CloudAddressProblem::Port)
    } else {
        Ok(port)
    }
}

/// The address the service stores for a host and port as typed.
pub fn normalize(host: &str, port: u16) -> Result<CloudServer, CloudAddressProblem> {
    Ok(CloudServer::new(normalize_host(host)?, check_port(port)?))
}

/// Two addresses reach the same server: the same normalized host, and the same port where 0 (the
/// SRV record, then 25565) and 25565 count as one. Two clients with one name there keep kicking
/// each other.
pub fn same_server(a: &CloudServer, b: &CloudServer) -> bool {
    let port = |p: u16| if p == 0 { 25565 } else { p };
    let host = |h: &str| normalize_host(h).unwrap_or_else(|_| h.trim().to_ascii_lowercase());
    port(a.port) == port(b.port) && host(&a.host) == host(&b.host)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_hosts() {
        assert_eq!(
            normalize_host(" Play.Projects.GG. ").as_deref(),
            Ok("play.projects.gg")
        );
        assert_eq!(
            normalize_host("Örnek-Sunucu.com").as_deref(),
            Ok("xn--rnek-sunucu-qfb.com")
        );
        assert_eq!(
            normalize_host("93.184.216.34").as_deref(),
            Ok("93.184.216.34")
        );
        assert_eq!(
            normalize_host("mc_1.example.org").as_deref(),
            Ok("mc_1.example.org")
        );
    }

    #[test]
    fn refuses_what_the_service_refuses() {
        assert_eq!(normalize_host(""), Err(CloudAddressProblem::Empty));
        assert_eq!(
            normalize_host("play.projects.gg:25565"),
            Err(CloudAddressProblem::Format)
        );
        assert_eq!(
            normalize_host("http://play.projects.gg"),
            Err(CloudAddressProblem::Format)
        );
        assert_eq!(normalize_host("[::1]"), Err(CloudAddressProblem::Format));
        assert_eq!(normalize_host("sunucum"), Err(CloudAddressProblem::Dot));
        // The browser's IPv4 parser fills short forms in (the site sends the same).
        assert_eq!(normalize_host("1.2.3").as_deref(), Ok("1.2.0.3"));
        for private in [
            "localhost",
            "oyun.local",
            "x.lan",
            "127.0.0.1",
            "10.1.2.3",
            "192.168.1.20",
            "172.20.0.5",
            "100.64.0.1",
            "169.254.1.1",
            "224.0.0.1",
            "0.0.0.0",
        ] {
            assert_eq!(
                normalize_host(private),
                Err(CloudAddressProblem::Private),
                "{private}"
            );
        }
        // The browser's IPv4 parser: hex and short forms are IPv4 too.
        assert_eq!(normalize_host("0x7f.1"), Err(CloudAddressProblem::Private));
        assert_eq!(normalize_host("172.32.0.1").as_deref(), Ok("172.32.0.1"));
    }

    #[test]
    fn ports() {
        assert_eq!(check_port(0), Ok(0));
        assert_eq!(check_port(25565), Ok(25565));
        assert_eq!(check_port(22), Err(CloudAddressProblem::Port));
        assert_eq!(check_port(3306), Err(CloudAddressProblem::Port));
    }

    #[test]
    fn the_same_server() {
        let a = CloudServer::new("play.projects.gg", 0);
        assert!(same_server(
            &a,
            &CloudServer::new("Play.Projects.gg", 25565)
        ));
        assert!(!same_server(
            &a,
            &CloudServer::new("play.projects.gg", 25566)
        ));
        assert!(!same_server(&a, &CloudServer::new("mc.projects.gg", 0)));
    }
}

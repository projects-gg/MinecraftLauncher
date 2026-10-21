//! One short request to a web address, to tell how it went: the troubleshooter's network checks
//! (HELP1). An answer (with its HTTP status and the server's clock), a name that does not resolve,
//! no answer in time, a refused connection, a secure connection that failed, or anything else that
//! kept the address out of reach. No retries, and nothing is downloaded: a `HEAD` reads the
//! headers only, a `GET` stops after them.

use std::error::Error as StdError;
use std::io;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::Http;

/// How a probe ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Reach {
    /// The server answered, with this HTTP status.
    Answered { status: u16 },
    /// The name did not resolve (DNS).
    UnknownHost,
    /// No answer in time.
    TimedOut,
    /// The computer at the address refused the connection.
    Refused,
    /// The secure connection failed: a certificate the system does not trust (a wrong clock,
    /// security software or a proxy that reads HTTPS, a network that redirects).
    Insecure,
    /// Anything else: a reset, an unreachable network, a malformed address.
    Failed,
}

impl Reach {
    /// The server answered at all.
    pub fn answered(self) -> bool {
        matches!(self, Self::Answered { .. })
    }

    /// The server answered without an error of its own (any status below 500).
    pub fn healthy(self) -> bool {
        matches!(self, Self::Answered { status } if status < 500)
    }

    /// The server answered with success (2xx, or a redirect).
    pub fn succeeded(self) -> bool {
        matches!(self, Self::Answered { status } if (200..400).contains(&status))
    }
}

/// What one probe found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Probe {
    pub reach: Reach,
    /// From the request until the answer's headers (or the failure).
    pub elapsed: Duration,
    /// The server's clock (`Date`), when the answer carried one.
    pub server_date: Option<SystemTime>,
    /// This computer's clock when the answer (or the failure) came.
    pub received_at: SystemTime,
}

impl Probe {
    /// How far this computer's clock is from the server's, in seconds (positive: this computer is
    /// ahead), when the answer carried a date. Good to a second or two: the date has whole seconds.
    pub fn clock_offset_secs(&self) -> Option<i64> {
        let server = self.server_date?;
        Some(match self.received_at.duration_since(server) {
            Ok(ahead) => i64::try_from(ahead.as_secs()).unwrap_or(i64::MAX),
            Err(behind) => -i64::try_from(behind.duration().as_secs()).unwrap_or(i64::MAX),
        })
    }
}

/// The request a probe makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeMethod {
    /// Headers only: for a page or a file whose content does not matter.
    Head,
    /// For an endpoint that only answers `GET` (a service's health check); the body is not read.
    Get,
}

impl Http {
    /// One request to `url`, at most `timeout` long, without the client's retries.
    pub async fn probe(&self, method: ProbeMethod, url: &str, timeout: Duration) -> Probe {
        let request = match method {
            ProbeMethod::Head => self.client().head(url),
            ProbeMethod::Get => self.client().get(url),
        };
        let started = Instant::now();
        let result = request.timeout(timeout).send().await;
        let elapsed = started.elapsed();
        let received_at = SystemTime::now();
        match result {
            Ok(response) => Probe {
                reach: Reach::Answered {
                    status: response.status().as_u16(),
                },
                elapsed,
                server_date: response
                    .headers()
                    .get(reqwest::header::DATE)
                    .and_then(|value| value.to_str().ok())
                    .and_then(parse_http_date),
                received_at,
            },
            Err(error) => Probe {
                reach: classify(&error),
                elapsed,
                server_date: None,
                received_at,
            },
        }
    }
}

/// One error of a chain, as [`classify_chain`] reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorLink {
    /// Its message, lower case.
    pub text: String,
    /// The I/O error kind, when it is one.
    pub kind: Option<io::ErrorKind>,
    /// The operating system's error number, when it is one.
    pub os: Option<i32>,
}

/// What kept a request from an answer.
pub fn classify(error: &reqwest::Error) -> Reach {
    let mut links = Vec::new();
    let mut next: Option<&(dyn StdError + 'static)> = Some(error);
    while let Some(current) = next {
        let io = current.downcast_ref::<io::Error>();
        links.push(ErrorLink {
            text: current.to_string().to_lowercase(),
            kind: io.map(io::Error::kind),
            os: io.and_then(io::Error::raw_os_error),
        });
        next = current.source();
    }
    classify_chain(error.is_timeout(), &links)
}

/// The operating systems' "connection refused": Windows (`WSAECONNREFUSED`), Linux, macOS.
const REFUSED_OS: [i32; 3] = [10061, 111, 61];
/// Windows' "no such host" (`WSAHOST_NOT_FOUND`) and "no data record" (`WSANO_DATA`).
const UNKNOWN_HOST_OS: [i32; 2] = [11001, 11004];

/// What the systems' resolvers and hyper's connector say when a name does not resolve.
const UNKNOWN_HOST_WORDS: [&str; 7] = [
    "dns error",
    "failed to lookup address",
    "no such host is known",
    "nodename nor servname",
    "name or service not known",
    "temporary failure in name resolution",
    "no address associated with",
];

/// What rustls, Schannel and OpenSSL say when a secure connection fails.
const INSECURE_WORDS: [&str; 6] = ["certificate", "tls", "ssl", "handshake", "schannel", "x509"];

/// [`classify`] on an error chain read beforehand (testable without a network).
pub fn classify_chain(timeout: bool, links: &[ErrorLink]) -> Reach {
    if timeout
        || links
            .iter()
            .any(|l| l.kind == Some(io::ErrorKind::TimedOut))
    {
        return Reach::TimedOut;
    }
    if links.iter().any(|l| {
        l.kind == Some(io::ErrorKind::ConnectionRefused)
            || l.os.is_some_and(|code| REFUSED_OS.contains(&code))
    }) {
        return Reach::Refused;
    }
    if links.iter().any(|l| {
        l.os.is_some_and(|code| UNKNOWN_HOST_OS.contains(&code))
            || UNKNOWN_HOST_WORDS.iter().any(|word| l.text.contains(word))
    }) {
        return Reach::UnknownHost;
    }
    if links
        .iter()
        .any(|l| INSECURE_WORDS.iter().any(|word| l.text.contains(word)))
    {
        return Reach::Insecure;
    }
    Reach::Failed
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// An HTTP date in the form every server sends (`Tue, 21 Oct 2026 18:42:07 GMT`, RFC 9110's
/// IMF-fixdate). Older forms are not read: they only mean no clock check.
pub fn parse_http_date(text: &str) -> Option<SystemTime> {
    let mut parts = text.split_ascii_whitespace();
    let weekday = parts.next()?.strip_suffix(',')?;
    if weekday.len() != 3 {
        return None;
    }
    let day: u32 = parts.next()?.parse().ok()?;
    let month_name = parts.next()?;
    let month = MONTHS
        .iter()
        .position(|m| m.eq_ignore_ascii_case(month_name))?;
    let month = u32::try_from(month).ok()? + 1;
    let year: i64 = parts.next()?.parse().ok()?;
    let mut clock = parts.next()?.split(':');
    let hour: u64 = clock.next()?.parse().ok()?;
    let minute: u64 = clock.next()?.parse().ok()?;
    let second: u64 = clock.next()?.parse().ok()?;
    if clock.next().is_some() || parts.next()? != "GMT" || parts.next().is_some() {
        return None;
    }
    if !(1970..=9999).contains(&year)
        || day == 0
        || day > days_in_month(year, month)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    let days = u64::try_from(days_from_civil(year, month, day)).ok()?;
    let secs = days * 86_400 + hour * 3_600 + minute * 60 + second;
    UNIX_EPOCH.checked_add(Duration::from_secs(secs))
}

fn leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        2 if leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Days from 1970-01-01 to a civil date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_from_march = i64::from((month + 9) % 12);
    let day_of_year = (153 * month_from_march + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;
    use crate::HttpConfig;

    fn secs(time: SystemTime) -> u64 {
        time.duration_since(UNIX_EPOCH).unwrap().as_secs()
    }

    #[test]
    fn reads_http_dates() {
        let date = |text| parse_http_date(text).map(secs);
        assert_eq!(date("Thu, 01 Jan 1970 00:00:00 GMT"), Some(0));
        assert_eq!(date("Sun, 06 Nov 1994 08:49:37 GMT"), Some(784_111_777));
        assert_eq!(date("Wed, 21 Oct 2026 18:42:07 GMT"), Some(1_792_608_127));
        assert_eq!(date("Thu, 29 Feb 2024 23:59:59 GMT"), Some(1_709_251_199));
        // Lower case months and extra spaces still read.
        assert_eq!(date("  Sun,  06 nov 1994 08:49:37 GMT "), Some(784_111_777));
    }

    #[test]
    fn ignores_dates_it_cannot_trust() {
        for text in [
            "",
            "yesterday",
            "Sunday, 06-Nov-94 08:49:37 GMT",
            "Sun Nov  6 08:49:37 1994",
            "Sun, 06 Nov 1994 08:49:37 UTC",
            "Sun, 06 Nov 1994 08:49:37 GMT extra",
            "Sun, 31 Feb 2026 08:49:37 GMT",
            "Sun, 29 Feb 2025 08:49:37 GMT",
            "Sun, 06 Nov 1994 24:00:00 GMT",
            "Sun, 06 Foo 1994 08:49:37 GMT",
            "Sun, 06 Nov 1969 08:49:37 GMT",
        ] {
            assert_eq!(parse_http_date(text), None, "{text}");
        }
    }

    #[test]
    fn measures_how_far_the_clock_is() {
        let server = UNIX_EPOCH + Duration::from_secs(1_000_000);
        let probe = |received: SystemTime| Probe {
            reach: Reach::Answered { status: 200 },
            elapsed: Duration::from_millis(40),
            server_date: Some(server),
            received_at: received,
        };
        assert_eq!(probe(server).clock_offset_secs(), Some(0));
        assert_eq!(
            probe(server + Duration::from_secs(600)).clock_offset_secs(),
            Some(600)
        );
        assert_eq!(
            probe(server - Duration::from_secs(7_200)).clock_offset_secs(),
            Some(-7_200)
        );
        let undated = Probe {
            server_date: None,
            ..probe(server)
        };
        assert_eq!(undated.clock_offset_secs(), None);
    }

    fn link(text: &str) -> ErrorLink {
        ErrorLink {
            text: text.to_lowercase(),
            kind: None,
            os: None,
        }
    }

    fn io_link(text: &str, kind: io::ErrorKind, os: Option<i32>) -> ErrorLink {
        ErrorLink {
            kind: Some(kind),
            os,
            ..link(text)
        }
    }

    #[test]
    fn names_what_kept_a_request_from_an_answer() {
        let request = link("error sending request for url (https://cdn.example/)");
        let connect = link("client error (Connect)");
        // Windows' resolver, Linux's and macOS's.
        let windows_dns = [
            request.clone(),
            connect.clone(),
            link("dns error"),
            io_link(
                "No such host is known. (os error 11001)",
                io::ErrorKind::Other,
                Some(11001),
            ),
        ];
        assert_eq!(classify_chain(false, &windows_dns), Reach::UnknownHost);
        let linux_dns = [
            request.clone(),
            link("failed to lookup address information: Name or service not known"),
        ];
        assert_eq!(classify_chain(false, &linux_dns), Reach::UnknownHost);
        let mac_dns = [link(
            "failed to lookup address information: nodename nor servname provided, or not known",
        )];
        assert_eq!(classify_chain(false, &mac_dns), Reach::UnknownHost);

        let refused = [
            request.clone(),
            connect.clone(),
            io_link(
                "No connection could be made because the target machine actively refused it. (os error 10061)",
                io::ErrorKind::ConnectionRefused,
                Some(10061),
            ),
        ];
        assert_eq!(classify_chain(false, &refused), Reach::Refused);
        assert_eq!(
            classify_chain(
                false,
                &[io_link("refused", io::ErrorKind::Other, Some(111))]
            ),
            Reach::Refused
        );

        // rustls and Schannel.
        let rustls = [
            request.clone(),
            connect.clone(),
            link("invalid peer certificate: UnknownIssuer"),
        ];
        assert_eq!(classify_chain(false, &rustls), Reach::Insecure);
        let schannel = [link(
            "The certificate chain was issued by an authority that is not trusted. (os error -2146762487)",
        )];
        assert_eq!(classify_chain(false, &schannel), Reach::Insecure);

        // A timeout wins over everything it may have cut short.
        assert_eq!(classify_chain(true, &windows_dns), Reach::TimedOut);
        assert_eq!(
            classify_chain(
                false,
                &[io_link("timed out", io::ErrorKind::TimedOut, None)]
            ),
            Reach::TimedOut
        );

        let reset = [
            request,
            io_link(
                "An existing connection was forcibly closed by the remote host. (os error 10054)",
                io::ErrorKind::ConnectionReset,
                Some(10054),
            ),
        ];
        assert_eq!(classify_chain(false, &reset), Reach::Failed);
        assert_eq!(classify_chain(false, &[]), Reach::Failed);
    }

    #[test]
    fn tells_answers_apart() {
        let answered = |status| Reach::Answered { status };
        assert!(answered(204).succeeded() && answered(301).succeeded());
        assert!(!answered(404).succeeded() && answered(404).healthy());
        assert!(!answered(503).healthy() && answered(503).answered());
        assert!(!Reach::TimedOut.answered() && !Reach::Refused.healthy());
    }

    fn http() -> Http {
        Http::new(HttpConfig::new("pl-net tests")).unwrap()
    }

    /// A local server that answers every request with `head` (status line and headers) and
    /// closes the connection.
    async fn answering(head: &'static str) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut request = Vec::new();
                    let mut buf = [0u8; 1024];
                    while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                        match socket.read(&mut buf).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => request.extend_from_slice(&buf[..n]),
                        }
                    }
                    let _ = socket.write_all(head.as_bytes()).await;
                    let _ = socket.shutdown().await;
                });
            }
        });
        format!("http://{address}/")
    }

    #[tokio::test]
    async fn reads_an_answer_and_its_clock() {
        let url = answering(
            "HTTP/1.1 204 No Content\r\nDate: Wed, 21 Oct 2026 18:42:07 GMT\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        )
        .await;
        let probe = http()
            .probe(ProbeMethod::Head, &url, Duration::from_secs(5))
            .await;
        assert_eq!(probe.reach, Reach::Answered { status: 204 });
        assert_eq!(probe.server_date.map(secs), Some(1_792_608_127));
        let get = http()
            .probe(ProbeMethod::Get, &url, Duration::from_secs(5))
            .await;
        assert_eq!(get.reach, Reach::Answered { status: 204 });
    }

    #[tokio::test]
    async fn a_server_error_is_still_an_answer() {
        let url = answering(
            "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        )
        .await;
        let probe = http()
            .probe(ProbeMethod::Get, &url, Duration::from_secs(5))
            .await;
        assert_eq!(probe.reach, Reach::Answered { status: 503 });
        assert!(!probe.reach.healthy());
        assert_eq!(probe.server_date, None);
    }

    #[tokio::test]
    async fn a_closed_port_refuses() {
        let port = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap().port()
        };
        let probe = http()
            .probe(
                ProbeMethod::Head,
                &format!("http://127.0.0.1:{port}/"),
                Duration::from_secs(10),
            )
            .await;
        assert_eq!(probe.reach, Reach::Refused);
    }

    #[tokio::test]
    async fn a_silent_server_times_out() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((socket, _)) = listener.accept().await {
                held.push(socket);
            }
        });
        let probe = http()
            .probe(
                ProbeMethod::Head,
                &format!("http://{address}/"),
                Duration::from_millis(300),
            )
            .await;
        assert_eq!(probe.reach, Reach::TimedOut);
        assert!(probe.elapsed < Duration::from_secs(5));
    }
}

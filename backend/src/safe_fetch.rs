//! SSRF-guarded fetcher for remote images (og:image covers).
//!
//! Every connection, including each redirect hop, is checked: hostnames through the
//! resolver (so there is no check/connect gap for DNS rebinding), IP literals explicitly
//! (the connector never consults the resolver for them).

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use bytes::{Bytes, BytesMut};
use futures::StreamExt;
use reqwest::Url;
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use reqwest::redirect::{Attempt, Policy};

use crate::images::MAX_INPUT_BYTES;

const MAX_REDIRECTS: usize = 3;
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);
const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/146.0.0.0 Safari/537.36";

/// Coarse on purpose: nothing here may reveal internal addresses to API clients.
#[derive(Debug, PartialEq, Eq)]
pub enum FetchError {
    BadUrl,
    Forbidden,
    TooLarge,
    Http(String),
}

/// Marker raised by the resolver and redirect policy; found via the error `source()` chain.
#[derive(Debug)]
struct Blocked;

impl std::fmt::Display for Blocked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("blocked destination")
    }
}

impl std::error::Error for Blocked {}

fn forbidden_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_documentation()
        || a == 0
        || (a == 100 && (64..128).contains(&b)) // CGNAT 100.64/10
        || (a == 192 && b == 0 && c == 0) // IETF protocol assignments 192.0.0/24
        || (a == 192 && b == 88 && c == 99) // deprecated 6to4 relay anycast
        || (a == 198 && (b == 18 || b == 19)) // benchmarking 198.18/15
        || a >= 240 // reserved 240/4
}

fn embedded_v4(hi: u16, lo: u16) -> Ipv4Addr {
    let [a, b] = hi.to_be_bytes();
    let [c, d] = lo.to_be_bytes();
    Ipv4Addr::new(a, b, c, d)
}

fn forbidden_v6(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return forbidden_v4(v4);
    }
    let s = ip.segments();
    // ::ffff:0:0:0/96 (SIIT)
    if s[..4].iter().all(|&x| x == 0) && s[4] == 0xffff && s[5] == 0 {
        return forbidden_v4(embedded_v4(s[6], s[7]));
    }
    let embedded = embedded_v4;
    if ip.is_unspecified() || ip.is_loopback() || ip.is_multicast() {
        return true;
    }
    // ::/96 (IPv4-compatible)
    if s[..6].iter().all(|&x| x == 0) {
        return forbidden_v4(embedded(s[6], s[7]));
    }
    // 64:ff9b::/96 (NAT64)
    if s[0] == 0x64 && s[1] == 0xff9b && s[2..6].iter().all(|&x| x == 0) {
        return forbidden_v4(embedded(s[6], s[7]));
    }
    // 64:ff9b:1::/48 (local-use NAT64)
    if s[0] == 0x64 && s[1] == 0xff9b && s[2] == 1 {
        return true;
    }
    // 2002::/16 (6to4) embedding a forbidden v4
    if s[0] == 0x2002 {
        return forbidden_v4(embedded(s[1], s[2]));
    }
    // 2001::/32 Teredo, 2001:db8::/32 documentation, 100::/64 discard-only
    if s[0] == 0x2001 && (s[1] == 0 || s[1] == 0x0db8) {
        return true;
    }
    if s[0] == 0x3fff && s[1] < 0x1000 {
        return true; // 3fff::/20 documentation
    }
    if s[0] == 0x0100 && s[1..4].iter().all(|&x| x == 0) {
        return true;
    }
    let first = s[0];
    (first & 0xfe00) == 0xfc00 // fc00::/7 unique-local
        || (first & 0xffc0) == 0xfe80 // fe80::/10 link-local
        || (first & 0xffc0) == 0xfec0 // fec0::/10 deprecated site-local
}

/// True for any address a server-side fetch must never connect to.
pub fn is_forbidden_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => forbidden_v4(v4),
        IpAddr::V6(v6) => forbidden_v6(v6),
    }
}

fn url_host_forbidden(url: &Url) -> bool {
    match url.host() {
        Some(url::Host::Ipv4(ip)) => forbidden_v4(ip),
        Some(url::Host::Ipv6(ip)) => forbidden_v6(ip),
        _ => false,
    }
}

fn scheme_ok(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https")
}

/// Redirect policy decision: `true` = follow. Hostnames are guarded by the resolver.
fn check_redirect(url: &Url, prior_hops: usize, allow_private: bool) -> bool {
    if prior_hops >= MAX_REDIRECTS || !scheme_ok(url) || url.host().is_none() {
        return false;
    }
    allow_private || !url_host_forbidden(url)
}

fn filter_addrs(addrs: impl Iterator<Item = SocketAddr>, allow_private: bool) -> Vec<SocketAddr> {
    addrs
        .filter(|a| allow_private || !is_forbidden_ip(a.ip()))
        .collect()
}

struct GuardedResolver {
    allow_private: bool,
}

impl Resolve for GuardedResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let allow_private = self.allow_private;
        Box::pin(async move {
            let looked_up = tokio::net::lookup_host((name.as_str(), 0)).await?;
            let addrs = filter_addrs(looked_up, allow_private);
            if addrs.is_empty() {
                return Err(Box::new(Blocked) as Box<dyn std::error::Error + Send + Sync>);
            }
            Ok(Box::new(addrs.into_iter()) as Addrs)
        })
    }
}

#[derive(Clone)]
pub struct ImageFetcher {
    client: reqwest::Client,
    allow_private: bool,
}

impl ImageFetcher {
    /// `allow_private` disables only the IP checks; production must always pass `false`.
    pub fn new(allow_private: bool) -> anyhow::Result<Self> {
        Self::build(allow_private, allow_private)
    }

    /// Resolver and redirect guards are separate so tests can reach a loopback first hop
    /// while keeping the redirect policy enforcing.
    fn build(resolver_allow_private: bool, redirect_allow_private: bool) -> anyhow::Result<Self> {
        let policy = Policy::custom(move |attempt: Attempt| {
            // `previous()` includes the initial URL.
            let followed = attempt.previous().len().saturating_sub(1);
            if check_redirect(attempt.url(), followed, redirect_allow_private) {
                attempt.follow()
            } else if followed >= MAX_REDIRECTS {
                attempt.error("too many redirects")
            } else {
                attempt.error(Blocked)
            }
        });
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .no_proxy()
            .dns_resolver(Arc::new(GuardedResolver {
                allow_private: resolver_allow_private,
            }))
            .redirect(policy)
            .timeout(FETCH_TIMEOUT)
            .build()?;
        Ok(Self {
            client,
            allow_private: resolver_allow_private,
        })
    }

    pub async fn fetch_image(&self, url: &str) -> Result<Bytes, FetchError> {
        let url = Url::parse(url).map_err(|_| FetchError::BadUrl)?;
        if !scheme_ok(&url) || url.host().is_none() {
            return Err(FetchError::BadUrl);
        }
        if !self.allow_private && url_host_forbidden(&url) {
            return Err(FetchError::Forbidden);
        }

        let resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| map_error(&e))?;
        if !resp.status().is_success() {
            return Err(FetchError::Http(format!(
                "status {}",
                resp.status().as_u16()
            )));
        }
        if resp
            .content_length()
            .is_some_and(|len| len > MAX_INPUT_BYTES as u64)
        {
            return Err(FetchError::TooLarge);
        }

        let mut body = BytesMut::new();
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| map_error(&e))?;
            if body.len() + chunk.len() > MAX_INPUT_BYTES {
                return Err(FetchError::TooLarge);
            }
            body.extend_from_slice(&chunk);
        }
        Ok(body.freeze())
    }
}

/// Maps to a coarse error; never includes the URL or the underlying message.
fn map_error(err: &reqwest::Error) -> FetchError {
    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(err);
    while let Some(e) = source {
        if e.is::<Blocked>() {
            return FetchError::Forbidden;
        }
        source = e.source();
    }
    let kind = if err.is_timeout() {
        "timeout"
    } else if err.is_redirect() {
        "redirect"
    } else if err.is_connect() {
        "connect"
    } else {
        "request failed"
    };
    FetchError::Http(kind.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn forbidden_ip_table() {
        let forbidden = [
            "127.0.0.1",
            "10.1.2.3",
            "172.31.0.1",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "100.127.255.255",
            "0.0.0.0",
            "0.1.2.3",
            "255.255.255.255",
            "224.0.0.1",
            "192.0.2.1",
            "198.51.100.1",
            "203.0.113.1",
            "198.18.0.1",
            "198.19.255.255",
            "240.0.0.1",
            "192.0.0.1",
            "::",
            "::1",
            "fe80::1",
            "fd00::1",
            "fc00::1",
            "ff02::1",
            "::ffff:10.0.0.1",
            "::ffff:127.0.0.1",
            "::ffff:169.254.169.254",
            "::127.0.0.1",
            "::10.0.0.1",
            "64:ff9b::a00:1",
            "64:ff9b::7f00:1",
            "64:ff9b:1::1",
            "2002:7f00:1::1",
            "2002:a9fe:a9fe::1",
            "2002:c0a8:101::",
            "2001::1",
            "2001:db8::1",
            "fec0::1",
            "100::1",
            "::ffff:0:7f00:1",
            "::ffff:0:a00:1",
            "3fff::1",
            "3fff:fff::1",
            "192.88.99.1",
        ];
        for s in forbidden {
            assert!(is_forbidden_ip(ip(s)), "{s} must be forbidden");
        }
        let allowed = [
            "1.1.1.1",
            "8.8.8.8",
            "172.32.0.1",
            "172.15.255.255",
            "100.63.255.255",
            "100.128.0.1",
            "198.17.0.1",
            "198.20.0.1",
            "223.255.255.255",
            "2606:4700::1111",
            "64:ff9b::808:808",
            "2002:808:808::1",
            "::ffff:8.8.8.8",
            "::ffff:0:808:808",
            "3fff:1000::1",
            "192.88.98.1",
        ];
        for s in allowed {
            assert!(!is_forbidden_ip(ip(s)), "{s} must be allowed");
        }
    }

    #[test]
    fn redirect_checks() {
        let u = |s: &str| Url::parse(s).unwrap();
        for bad in [
            "http://169.254.169.254/",
            "http://[::1]/",
            "http://2130706433/",
            "http://0x7f.1/",
            "http://127.1/",
            "http://[::ffff:10.0.0.1]/",
            "ftp://example.com/",
            "file:///etc/passwd",
        ] {
            assert!(!check_redirect(&u(bad), 0, false), "{bad}");
        }
        assert!(check_redirect(&u("http://example.com/"), 0, false));
        assert!(check_redirect(&u("https://1.1.1.1/"), 2, false));
        assert!(!check_redirect(&u("http://example.com/"), 3, false));
        assert!(check_redirect(&u("http://127.0.0.1/"), 0, true));
    }

    #[test]
    fn resolver_filters_forbidden_addresses() {
        let addrs = [
            "127.0.0.1:0",
            "[::1]:0",
            "10.0.0.1:0",
            "8.8.8.8:0",
            "[2606:4700::1111]:0",
        ]
        .map(|s| s.parse::<SocketAddr>().unwrap());
        let kept = filter_addrs(addrs.into_iter(), false);
        assert_eq!(kept.len(), 2);
        assert!(kept.iter().all(|a| !is_forbidden_ip(a.ip())));
        assert_eq!(filter_addrs(addrs.into_iter(), true).len(), 5);
    }

    #[tokio::test]
    async fn resolver_blocks_localhost() {
        let r = GuardedResolver {
            allow_private: false,
        };
        let name: Name = "localhost".parse().unwrap();
        let err = r.resolve(name).await.err().expect("must be blocked");
        assert!(err.is::<Blocked>());
    }

    #[tokio::test]
    async fn rejects_non_http_scheme() {
        let f = ImageFetcher::new(false).unwrap();
        for url in ["file:///etc/passwd", "ftp://x", "gopher://x/", "not a url"] {
            assert_eq!(f.fetch_image(url).await, Err(FetchError::BadUrl), "{url}");
        }
    }

    #[tokio::test]
    async fn rejects_ip_literals_before_connecting() {
        let f = ImageFetcher::new(false).unwrap();
        for url in [
            "http://127.0.0.1:1/x.jpg",
            "http://[::1]:1/x.jpg",
            "http://2130706433/x.jpg",
            "http://[::ffff:127.0.0.1]/x.jpg",
        ] {
            assert_eq!(
                f.fetch_image(url).await,
                Err(FetchError::Forbidden),
                "{url}"
            );
        }
    }

    #[tokio::test]
    async fn redirect_hop_to_private_literal_is_blocked() {
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(302).insert_header("Location", "http://169.254.169.254/x"),
            )
            .mount(&server)
            .await;
        // First hop (loopback wiremock) allowed, redirect policy enforcing.
        let f = ImageFetcher::build(true, false).unwrap();
        let res = f.fetch_image(&format!("{}/a", server.uri())).await;
        assert_eq!(res, Err(FetchError::Forbidden));
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }
}

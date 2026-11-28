use parking_lot::Mutex;
use std::collections::HashMap;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_millis(1500);
const TTL: Duration = Duration::from_secs(300);
/// Budget a probe may take before the answer counts as reachable.
const BUDGET: Duration = Duration::from_millis(2000);

/// Probes a host before navigating, so an unreachable one shows our page.
#[derive(Default)]
pub struct Reachability {
    seen: Mutex<HashMap<String, (bool, Instant)>>,
}

impl Reachability {
    /// Cached answer, or a fresh probe. Only http(s) urls are probeable.
    pub fn check(&self, url: &str) -> bool {
        let Some((host, port)) = host_and_port(url) else {
            return true;
        };
        let key = format!("{host}:{port}");
        {
            let seen = self.seen.lock();
            if let Some((ok, at)) = seen.get(&key) {
                if at.elapsed() < TTL {
                    return *ok;
                }
            }
        }

        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(probe(&host, port));
        });
        let Ok(ok) = receiver.recv_timeout(BUDGET) else {
            crate::logging::warn(&format!("reachability probe timed out for {key}"));
            return true;
        };
        let mut seen = self.seen.lock();
        seen.insert(key, (ok, Instant::now()));
        ok
    }

    #[cfg(test)]
    pub fn remember(&self, url: &str, ok: bool) {
        if let Some((host, port)) = host_and_port(url) {
            let mut seen = self.seen.lock();
            seen.insert(format!("{host}:{port}"), (ok, Instant::now()));
        }
    }
}

fn probe(host: &str, port: u16) -> bool {
    let Ok(addrs) = (host, port).to_socket_addrs() else {
        return false;
    };
    for addr in addrs {
        if TcpStream::connect_timeout(&addr, TIMEOUT).is_ok() {
            return true;
        }
    }
    false
}

/// Whether an address on the public internet is what this url names.
///
/// Used before the app fetches something on a page's behalf - a favicon is the case - because
/// that fetch comes from this process, which is inside the reader's network. A page that
/// names `http://192.168.1.1/` or `http://127.0.0.1:8080/` would otherwise have the browser
/// probe the reader's own machines for it, and the answer (reachable or not, fast or slow)
/// is visible to the page.
///
/// The host is resolved, not just pattern-matched: a public name can point at a private
/// address, so `evil.example` resolving to `127.0.0.1` has to be caught too. Every address
/// the name resolves to has to be public - one private answer is enough to refuse, since
/// there is no way to know which one the fetch would use.
pub fn is_public_web(url: &str) -> bool {
    let Some((host, port)) = host_and_port(url) else {
        return false;
    };
    // A literal address can be judged without a lookup, which also keeps the loopback
    // spellings that a resolver would not normalise.
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return !is_private_ip(&ip);
    }
    let Ok(addresses) = (host.as_str(), port).to_socket_addrs() else {
        return false;
    };
    let mut any = false;
    for address in addresses {
        any = true;
        if is_private_ip(&address.ip()) {
            return false;
        }
    }
    any
}

/// Loopback, link-local, private ranges and the other ways to name a machine nearby.
fn is_private_ip(ip: &std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                // 100.64/10 is carrier-grade NAT, 0.0.0.0/8 is "this network".
                || v4.octets()[0] == 0
                || (v4.octets()[0] == 100 && (64..128).contains(&v4.octets()[1]))
        }
        std::net::IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                // ::ffff:a.b.c.d is an IPv4 address wearing an IPv6 hat.
                || v6.to_ipv4_mapped().map(|v4| is_private_ip(&v4.into())).unwrap_or(false)
                // fc00::/7 unique local, fe80::/10 link local.
                || (v6.segments()[0] & 0xfe00) == 0xfc00
                || (v6.segments()[0] & 0xffc0) == 0xfe80
        }
    }
}

/// "https://example.com/a" -> ("example.com", 443)
pub fn host_and_port(url: &str) -> Option<(String, u16)> {
    let (scheme, rest) = url.split_once("://")?;
    let port = match scheme.to_ascii_lowercase().as_str() {
        "http" => 80,
        "https" => 443,
        _ => return None,
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let authority = authority
        .rsplit_once('@')
        .map(|(_, tail)| tail)
        .unwrap_or(authority);

    let (host, port) = match authority.rsplit_once(':') {
        Some((host, maybe_port)) if maybe_port.parse::<u16>().is_ok() => {
            (host.to_string(), maybe_port.parse().ok()?)
        }
        _ => (authority.to_string(), port),
    };
    if host.is_empty() {
        return None;
    }
    Some((host, port))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_and_port_handles_the_common_forms() {
        assert_eq!(
            host_and_port("https://example.com/a"),
            Some(("example.com".into(), 443))
        );
        assert_eq!(
            host_and_port("http://example.com"),
            Some(("example.com".into(), 80))
        );
        assert_eq!(
            host_and_port("http://127.0.0.1:8765/x"),
            Some(("127.0.0.1".into(), 8765))
        );
        assert_eq!(
            host_and_port("https://user:pw@a.test/"),
            Some(("a.test".into(), 443))
        );
        assert_eq!(host_and_port("about:blank"), None);
    }

    #[test]
    fn cache_answers_repeat_questions() {
        let reach = Reachability::default();
        reach.remember("https://a.test/", false);
        assert!(!reach.check("https://a.test/other"));
        reach.remember("https://a.test/", true);
        assert!(reach.check("https://a.test/"));
    }

    #[test]
    fn a_local_port_without_a_listener_is_unreachable() {
        assert!(!probe("127.0.0.1", 1));
    }

    #[test]
    fn the_reader_s_own_machines_are_not_the_public_web() {
        // A favicon url comes from a page. Fetching it is this process reaching out from
        // inside the reader's network, so a page must not be able to point it inward.
        for url in [
            "http://127.0.0.1/",
            "http://127.0.0.1:8080/favicon.ico",
            "http://localhost/x.ico",
            "https://192.168.1.1/",
            "http://10.0.0.5/",
            "http://[::1]/",
            "http://[fe80::1]/",
            "http://0.0.0.0/",
            "http://169.254.1.1/",
            // 100.64/10, carrier-grade NAT.
            "http://100.64.0.1/",
            // An IPv4 address wearing an IPv6 hat is the same address.
            "http://[::ffff:127.0.0.1]/",
            // Not a url this can judge, so not something to fetch on a page's word.
            "file:///C:/Windows/win.ini",
            "about:blank",
        ] {
            assert!(!is_public_web(url), "should be refused: {url}");
        }
    }

    #[test]
    fn an_ordinary_address_is_the_public_web() {
        // The check must not refuse what it is meant to allow, or every favicon disappears.
        assert!(is_public_web("https://example.com/favicon.ico"));
        assert!(is_public_web("http://i0.hdslb.com/bfs/face/x.jpg"));
    }

    #[test]
    fn the_private_ranges_are_the_ones_that_matter() {
        use std::net::IpAddr;
        let private = [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.0.1",
            "169.254.0.1",
            "100.64.0.1",
            // The documentation range: nothing real is served from it, and a page naming it
            // is not asking for an icon.
            "203.0.113.9",
        ];
        for ip in private {
            assert!(
                is_private_ip(&ip.parse::<IpAddr>().expect("ip")),
                "should be private: {ip}"
            );
        }
        for ip in ["8.8.8.8", "1.1.1.1"] {
            assert!(
                !is_private_ip(&ip.parse::<IpAddr>().expect("ip")),
                "should be public: {ip}"
            );
        }
    }
}

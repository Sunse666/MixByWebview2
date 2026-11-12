use std::collections::HashMap;
use std::net::{TcpStream, ToSocketAddrs};
use parking_lot::Mutex;
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

/// "https://example.com/a" -> ("example.com", 443)
pub fn host_and_port(url: &str) -> Option<(String, u16)> {
    let (scheme, rest) = url.split_once("://")?;
    let port = match scheme.to_ascii_lowercase().as_str() {
        "http" => 80,
        "https" => 443,
        _ => return None,
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let authority = authority.rsplit_once('@').map(|(_, tail)| tail).unwrap_or(authority);

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
        assert_eq!(host_and_port("https://example.com/a"), Some(("example.com".into(), 443)));
        assert_eq!(host_and_port("http://example.com"), Some(("example.com".into(), 80)));
        assert_eq!(host_and_port("http://127.0.0.1:8765/x"), Some(("127.0.0.1".into(), 8765)));
        assert_eq!(host_and_port("https://user:pw@a.test/"), Some(("a.test".into(), 443)));
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
}

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Default TCP port for the orishu client API.
pub const DEFAULT_PORT: u16 = 6680;

/// Returns the default Unix socket path for the orishu worker.
///
/// Uses `$XDG_RUNTIME_DIR/orishu/worker.sock` when the environment variable is
/// set (the common case on systemd-based Linux, where it resolves to
/// `/run/user/<uid>/orishu/worker.sock`).
///
/// Falls back to `/tmp/orishu-<uid>/worker.sock` when `XDG_RUNTIME_DIR` is not
/// available, keeping the "no special permissions required" guarantee.
pub fn default_socket_path() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") {
        PathBuf::from(dir).join("orishu/worker.sock")
    } else {
        // Fallback: per-user directory under /tmp when XDG_RUNTIME_DIR is not set.
        let user = std::env::var("USER").unwrap_or_else(|_| "unknown".to_string());
        PathBuf::from(format!("/tmp/orishu-{user}/worker.sock"))
    }
}

/// Where to connect when constructing a
/// [`HttpClusterClient`](super::http_client::HttpClusterClient).
///
/// A `ClusterAddress` is parsed from a CLI string with [`ClusterAddress::parse`] or via the
/// [`std::str::FromStr`] impl, which lets clap derive it automatically for `--host`-style
/// arguments.
///
/// # Accepted string formats
///
/// | Input | Variant | Notes |
/// |---|---|---|
/// | `/run/user/1000/orishu/worker.sock` | `UnixSocket` | Absolute path |
/// | `./custom.sock` | `UnixSocket` | Relative path |
/// | `unix:/run/user/1000/orishu/worker.sock` | `UnixSocket` | Explicit scheme |
/// | `192.0.2.1:6680` | `Ip` | IPv4 + port |
/// | `[2001:db8::1]:6680` | `Ip` | IPv6 + port (bracket notation) |
/// | `cluster.orishu.local:6680` | `Host` | Hostname + explicit port |
/// | `cluster.orishu.local` | `Host` | Hostname, default port (6680) |
///
/// Bare IPv6 addresses without brackets are not supported because the colon
/// is ambiguous with the port separator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClusterAddress {
    /// Local Unix domain socket. Qualifies for Tier 1 (unauthenticated, read-only) access
    /// when the node runs under the same OS user as the client.
    UnixSocket(PathBuf),

    /// A single, already-resolved IP address + port. Connects directly — no DNS lookup.
    /// Useful when the caller already holds a concrete address (e.g. from a node list).
    Ip(std::net::SocketAddr),

    /// A hostname + port. DNS is resolved at connection time and may yield multiple
    /// candidate addresses; the client tries them in order until one succeeds.
    Host { host: String, port: u16 },
}

impl ClusterAddress {
    /// Parse a CLI string into a `ClusterAddress`. Equivalent to `s.parse()`.
    pub fn parse(s: &str) -> Result<Self, ClusterAddressParseError> {
        s.parse()
    }
}

impl Default for ClusterAddress {
    /// The default address is the local node's Unix domain socket.
    /// Matches the behavior of running `orishu-ctl` or `orishu-monitor` without `--host`.
    fn default() -> Self {
        Self::UnixSocket(default_socket_path())
    }
}

impl std::fmt::Display for ClusterAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnixSocket(path) => write!(f, "{}", path.display()),
            Self::Ip(addr) => write!(f, "{addr}"),
            Self::Host { host, port } => write!(f, "{host}:{port}"),
        }
    }
}

impl std::str::FromStr for ClusterAddress {
    type Err = ClusterAddressParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // ── Unix socket ───────────────────────────────────────────────────────
        if s.starts_with('/') || s.starts_with("./") {
            return Ok(Self::UnixSocket(PathBuf::from(s)));
        }
        if let Some(path) = s.strip_prefix("unix:") {
            return Ok(Self::UnixSocket(PathBuf::from(path)));
        }

        // ── Direct IP + port ─────────────────────────────────────────────────
        // `SocketAddr::parse` handles both `1.2.3.4:port` and `[::1]:port`.
        if let Ok(addr) = s.parse::<std::net::SocketAddr>() {
            return Ok(Self::Ip(addr));
        }

        // ── Hostname with optional port ───────────────────────────────────────
        // Split on the *last* colon. Bare IPv6 without brackets hits the SocketAddr
        // branch above if valid, otherwise we reject it here (ambiguous separator).
        match s.rfind(':') {
            Some(colon) => {
                let host = &s[..colon];
                let port_str = &s[colon + 1..];
                match port_str.parse::<u16>() {
                    Ok(port) if !host.is_empty() => Ok(Self::Host {
                        host: host.to_string(),
                        port,
                    }),
                    _ => Err(ClusterAddressParseError(s.to_string())),
                }
            }
            // No colon at all — treat as hostname with default port.
            None if !s.is_empty() => Ok(Self::Host {
                host: s.to_string(),
                port: DEFAULT_PORT,
            }),
            _ => Err(ClusterAddressParseError(s.to_string())),
        }
    }
}

// ── Parse error ───────────────────────────────────────────────────────────────

/// Returned when a string cannot be interpreted as a [`ClusterAddress`].
#[derive(Debug, Clone)]
pub struct ClusterAddressParseError(String);

impl std::fmt::Display for ClusterAddressParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "invalid cluster address {:?}: expected a Unix socket path, \
             IP:port, or hostname[:port]",
            self.0
        )
    }
}

impl std::error::Error for ClusterAddressParseError {}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

    fn parse(s: &str) -> ClusterAddress {
        s.parse()
            .unwrap_or_else(|e| panic!("parse({s:?}) failed: {e}"))
    }

    #[test]
    fn unix_absolute_path() {
        assert_eq!(
            parse("/run/orishu/worker.sock"),
            ClusterAddress::UnixSocket("/run/orishu/worker.sock".into()),
        );
    }

    #[test]
    fn unix_relative_path() {
        assert_eq!(
            parse("./custom.sock"),
            ClusterAddress::UnixSocket("./custom.sock".into()),
        );
    }

    #[test]
    fn unix_explicit_scheme() {
        assert_eq!(
            parse("unix:/var/run/orishu.sock"),
            ClusterAddress::UnixSocket("/var/run/orishu.sock".into()),
        );
    }

    #[test]
    fn ipv4_with_port() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)), 6680);
        assert_eq!(parse("192.0.2.1:6680"), ClusterAddress::Ip(addr));
    }

    #[test]
    fn ipv6_with_port() {
        let addr = SocketAddr::new(
            IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1)),
            6680,
        );
        assert_eq!(parse("[2001:db8::1]:6680"), ClusterAddress::Ip(addr));
    }

    #[test]
    fn hostname_with_port() {
        assert_eq!(
            parse("cluster.orishu.local:9000"),
            ClusterAddress::Host {
                host: "cluster.orishu.local".into(),
                port: 9000
            },
        );
    }

    #[test]
    fn hostname_without_port_uses_default() {
        assert_eq!(
            parse("cluster.orishu.local"),
            ClusterAddress::Host {
                host: "cluster.orishu.local".into(),
                port: DEFAULT_PORT
            },
        );
    }

    #[test]
    fn empty_string_is_error() {
        assert!("".parse::<ClusterAddress>().is_err());
    }

    #[test]
    fn display_roundtrips_ip() {
        let addr = "192.0.2.1:6680";
        let ca: ClusterAddress = addr.parse().unwrap();
        assert_eq!(ca.to_string(), addr);
    }

    #[test]
    fn display_roundtrips_host() {
        let ca: ClusterAddress = "cluster.orishu.local:9000".parse().unwrap();
        assert_eq!(ca.to_string(), "cluster.orishu.local:9000");
    }
}

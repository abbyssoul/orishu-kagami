//! Explicit per-role network placement: bind a role's sockets to one named
//! interface so both ingress and egress follow the operator's selection.
//!
//! Address selection alone does not place traffic. A server bound to an
//! Ethernet address still replies over Wi-Fi when the route table prefers it,
//! which is what the physical cluster observed before this module existed. The
//! enforcement here is `SO_BINDTODEVICE` on every socket the placed role
//! creates. See [ADR 0026](../../../docs/adr/0026-worker-network-interface-placement.md).
//!
//! Enforcement is Linux-only. On other platforms an unplaced socket behaves
//! exactly as before and a placed one is a startup error, never a silent
//! fallback to an unconstrained socket.

use std::net::SocketAddr;

/// Kernel `IFNAMSIZ` is 16 including the terminating NUL.
const MAX_INTERFACE_NAME_LEN: usize = 15;

/// A validated network interface name, not an arbitrary operator string.
///
/// The grammar matches the one the Pi lab inventory validator already enforces
/// (`[A-Za-z0-9][A-Za-z0-9_.-]{0,14}`) so the two surfaces cannot disagree about
/// what an interface name is.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InterfaceName(String);

impl InterfaceName {
    /// The validated name, suitable for `SO_BINDTODEVICE` and diagnostics.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Resolve to the kernel interface index, which doubles as the existence
    /// check. An unknown interface is a configuration error, never a warning.
    #[cfg(target_os = "linux")]
    pub fn resolve(&self) -> Result<u32, PlacementError> {
        // A datagram socket is only the handle the `SIOCGIFINDEX` ioctl needs;
        // it is never bound and carries no traffic.
        let probe = std::net::UdpSocket::bind(("127.0.0.1", 0))
            .map_err(|source| PlacementError::Resolve { source })?;
        rustix::net::netdevice::name_to_index(&probe, &self.0).map_err(|_| {
            PlacementError::UnknownInterface {
                name: self.0.clone(),
            }
        })
    }

    /// Non-Linux builds cannot enforce placement, so they never resolve a name.
    #[cfg(not(target_os = "linux"))]
    pub fn resolve(&self) -> Result<u32, PlacementError> {
        Err(PlacementError::UnsupportedPlatform)
    }
}

impl std::fmt::Display for InterfaceName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::str::FromStr for InterfaceName {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let invalid = || {
            format!(
                "invalid interface name {value:?}: expected 1-{MAX_INTERFACE_NAME_LEN} characters \
                 starting with a letter or digit, then letters, digits, '_', '.' or '-'"
            )
        };
        if value.is_empty() || value.len() > MAX_INTERFACE_NAME_LEN {
            return Err(invalid());
        }
        let mut bytes = value.bytes();
        let first = bytes.next().ok_or_else(invalid)?;
        if !first.is_ascii_alphanumeric() {
            return Err(invalid());
        }
        if !bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || b == b'-') {
            return Err(invalid());
        }
        Ok(Self(value.to_owned()))
    }
}

impl TryFrom<String> for InterfaceName {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl<'de> serde::Deserialize<'de> for InterfaceName {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        raw.parse().map_err(serde::de::Error::custom)
    }
}

/// Why a placed socket could not be created. Kept structured so callers report
/// which interface and which role failed rather than a flattened string.
#[derive(Debug, thiserror::Error)]
pub enum PlacementError {
    /// The configured interface does not exist in this network namespace.
    #[error("unknown network interface {name:?}; it must exist in the worker's network namespace")]
    UnknownInterface {
        /// The rejected interface name.
        name: String,
    },
    /// This build cannot enforce device-constrained placement.
    #[error(
        "network interface placement is only enforceable on Linux; \
         remove the interface selection or deploy on Linux"
    )]
    UnsupportedPlatform,
    /// The interface index could not be looked up at all.
    #[error("cannot inspect network interfaces: {source}")]
    Resolve {
        /// The underlying failure.
        source: std::io::Error,
    },
    /// Binding the socket to the device failed.
    #[error("cannot bind socket to interface {name:?}: {source}")]
    DeviceBind {
        /// The interface that could not be selected.
        name: String,
        /// The underlying failure.
        source: std::io::Error,
    },
    /// The socket could not be bound to its address.
    #[error("cannot bind {address}: {source}")]
    Bind {
        /// The address that could not be bound.
        address: SocketAddr,
        /// The underlying failure.
        source: std::io::Error,
    },
}

/// Apply the device constraint, or refuse to pretend it was applied.
#[cfg(target_os = "linux")]
fn bind_device(
    socket: &socket2::Socket,
    device: Option<&InterfaceName>,
) -> Result<(), PlacementError> {
    let Some(device) = device else {
        return Ok(());
    };
    socket
        .bind_device(Some(device.as_str().as_bytes()))
        .map_err(|source| PlacementError::DeviceBind {
            name: device.0.clone(),
            source,
        })
}

/// Non-Linux placement never silently degrades to an unconstrained socket.
#[cfg(not(target_os = "linux"))]
fn bind_device(
    _socket: &socket2::Socket,
    device: Option<&InterfaceName>,
) -> Result<(), PlacementError> {
    match device {
        None => Ok(()),
        Some(_) => Err(PlacementError::UnsupportedPlatform),
    }
}

/// Whether an IPv6 socket should also accept IPv4-mapped traffic.
///
/// The two quinn constructors this module replaces differ here:
/// `Endpoint::client` asks for dual-stack, `Endpoint::server` leaves the
/// platform default. Carrying the distinction explicitly keeps each unplaced
/// call site exactly as it was instead of quietly widening a listener on
/// platforms where `bindv6only` defaults on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DualStack {
    /// Best-effort `IPV6_V6ONLY=0`, matching `quinn::Endpoint::client`.
    Request,
    /// Leave the platform default, matching `quinn::Endpoint::server`.
    PlatformDefault,
}

/// A UDP socket for QUIC, optionally constrained to one interface.
///
/// Apart from the device binding and the caller's `dual_stack` choice, the
/// option set is what the quinn constructor for that call site already used, so
/// an unplaced socket is unchanged. Quinn still applies its own GSO/GRO/ECN
/// options and non-blocking mode when it wraps the socket.
pub fn placed_udp_socket(
    address: SocketAddr,
    device: Option<&InterfaceName>,
    dual_stack: DualStack,
) -> Result<std::net::UdpSocket, PlacementError> {
    let socket = socket2::Socket::new(
        socket2::Domain::for_address(address),
        socket2::Type::DGRAM,
        Some(socket2::Protocol::UDP),
    )
    .map_err(|source| PlacementError::Bind { address, source })?;
    if address.is_ipv6() && dual_stack == DualStack::Request {
        // Matches quinn: best-effort, never fatal.
        let _ = socket.set_only_v6(false);
    }
    // The device must constrain the socket before it is bound and reachable.
    bind_device(&socket, device)?;
    socket
        .bind(&address.into())
        .map_err(|source| PlacementError::Bind { address, source })?;
    Ok(socket.into())
}

/// A listening TCP socket for client traffic, optionally constrained to one
/// interface. Accepted connections inherit the device binding, so replies leave
/// through the selected interface rather than whichever one the route prefers.
pub fn placed_tcp_listener(
    address: SocketAddr,
    device: Option<&InterfaceName>,
    backlog: i32,
) -> Result<std::net::TcpListener, PlacementError> {
    let socket = socket2::Socket::new(
        socket2::Domain::for_address(address),
        socket2::Type::STREAM,
        Some(socket2::Protocol::TCP),
    )
    .map_err(|source| PlacementError::Bind { address, source })?;
    // Parity with tokio's listener, which sets this before binding.
    socket
        .set_reuse_address(true)
        .map_err(|source| PlacementError::Bind { address, source })?;
    bind_device(&socket, device)?;
    socket
        .bind(&address.into())
        .map_err(|source| PlacementError::Bind { address, source })?;
    socket
        .listen(backlog)
        .map_err(|source| PlacementError::Bind { address, source })?;
    socket
        .set_nonblocking(true)
        .map_err(|source| PlacementError::Bind { address, source })?;
    Ok(socket.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(value: &str) -> InterfaceName {
        value.parse().expect("valid interface name")
    }

    #[test]
    fn interface_names_reject_hostile_and_oversized_values() {
        for rejected in [
            "",
            "-eth0",
            "_eth0",
            ".",
            "..",
            "eth0/../x",
            "eth 0",
            "eth\u{0}0",
            "ethernet01234567",
            "ünicode",
        ] {
            assert!(
                rejected.parse::<InterfaceName>().is_err(),
                "expected {rejected:?} to be rejected"
            );
        }
        for accepted in ["lo", "eth0", "wlan0", "enx001122334455", "br-0.1", "v_0-1"] {
            assert_eq!(name(accepted).as_str(), accepted);
        }
        // Exactly IFNAMSIZ - 1 is the longest the kernel accepts.
        assert_eq!(
            name("abcdefghijklmno").as_str().len(),
            MAX_INTERFACE_NAME_LEN
        );
        assert!("abcdefghijklmnop".parse::<InterfaceName>().is_err());
    }

    #[test]
    fn interface_names_deserialize_through_the_same_validation() {
        let ok: InterfaceName = serde_yaml::from_str("lo").expect("valid name");
        assert_eq!(ok.as_str(), "lo");
        assert!(serde_yaml::from_str::<InterfaceName>("\"eth 0\"").is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn loopback_resolves_and_unknown_interfaces_are_named_in_the_error() {
        assert!(name("lo").resolve().is_ok());
        let error = name("orishunodev0")
            .resolve()
            .expect_err("no such interface");
        assert!(
            error.to_string().contains("orishunodev0"),
            "error must name the interface: {error}"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn placed_sockets_bind_to_loopback_and_report_their_address() {
        let device = name("lo");
        let udp = placed_udp_socket(
            "127.0.0.1:0".parse().unwrap(),
            Some(&device),
            DualStack::PlatformDefault,
        )
        .expect("placed udp socket");
        assert!(udp.local_addr().expect("bound").port() > 0);

        let tcp = placed_tcp_listener("127.0.0.1:0".parse().unwrap(), Some(&device), 128)
            .expect("placed tcp listener");
        assert!(tcp.local_addr().expect("bound").port() > 0);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn unplaced_sockets_keep_their_previous_unconstrained_behaviour() {
        let udp = placed_udp_socket("127.0.0.1:0".parse().unwrap(), None, DualStack::Request)
            .expect("udp socket");
        assert!(udp.local_addr().expect("bound").port() > 0);
        let tcp =
            placed_tcp_listener("127.0.0.1:0".parse().unwrap(), None, 128).expect("tcp listener");
        assert!(tcp.local_addr().expect("bound").port() > 0);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn placing_on_a_missing_interface_fails_instead_of_falling_back() {
        let device = name("orishunodev0");
        let error = placed_udp_socket(
            "127.0.0.1:0".parse().unwrap(),
            Some(&device),
            DualStack::PlatformDefault,
        )
        .expect_err("must not fall back to an unconstrained socket");
        assert!(matches!(error, PlacementError::DeviceBind { .. }));
    }
}

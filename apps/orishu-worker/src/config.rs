use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::ListenAddress;
use orishu_worker::net_placement::InterfaceName;
mod tracing;
pub use tracing::TracingConfig;
mod logging;
pub use logging::LoggingConfig;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub logging: LoggingConfig,
    pub tracing: TracingConfig,
    pub observability: ObservabilityConfig,
    pub state_dir: Option<PathBuf>,
    pub worker_name: Option<orishu_membership::WorkerName>,
    pub cluster_name: Option<orishu_membership::ClusterName>,
    pub listen: Vec<ListenAddress>,
    pub peer_listen: Option<std::net::SocketAddr>,
    pub peer_advertise: Option<std::net::SocketAddr>,
    /// Interface every peer socket is bound to, including outbound join dials.
    pub peer_interface: Option<InterfaceName>,
    /// Interface every TCP client listener and its replies are bound to.
    pub client_interface: Option<InterfaceName>,
    pub accepts_peers: Option<bool>,
    pub tls_cert: Option<PathBuf>,
    pub tls_key: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservabilityConfig {
    pub enabled: Option<bool>,
    pub bind: Option<std::net::SocketAddr>,
    pub metrics: Option<bool>,
    pub probes: Option<bool>,
}

impl ObservabilityConfig {
    /// Route selection never enables the listener itself.
    pub fn metrics_enabled(&self) -> bool {
        self.metrics.unwrap_or(true)
    }

    /// The three process probes share one route-group switch.
    pub fn probes_enabled(&self) -> bool {
        self.probes.unwrap_or(true)
    }
}

impl RuntimeConfig {
    pub fn from_file(path: &Path) -> Result<Self, String> {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| format!("failed to read config file {}: {e}", path.display()))?;
        parse_config_str(&raw, path)
    }

    pub fn apply_cli(
        &mut self,
        listen: &[ListenAddress],
        tls_cert: Option<&Path>,
        tls_key: Option<&Path>,
    ) {
        if !listen.is_empty() {
            self.listen = listen.to_vec();
        }
        if let Some(path) = tls_cert {
            self.tls_cert = Some(path.to_path_buf());
        }
        if let Some(path) = tls_key {
            self.tls_key = Some(path.to_path_buf());
        }
    }

    pub fn finalize(mut self) -> Result<Self, String> {
        self.logging.validate()?;
        self.tracing.validate()?;
        if self.observability.enabled == Some(true) {
            if !cfg!(feature = "observability") {
                return Err("diagnostics require the observability build feature".to_owned());
            }
            if !self.observability.metrics_enabled() && !self.observability.probes_enabled() {
                return Err("enabled diagnostics require metrics or probes; disable the listener with observability.enabled=false".to_owned());
            }
            let bind = self
                .observability
                .bind
                .get_or_insert_with(|| "127.0.0.1:9168".parse().unwrap());
            if !bind.ip().is_loopback() {
                return Err("diagnostics currently require a loopback bind; secured remote metrics are not implemented".to_owned());
            }
        }
        if self.accepts_peers == Some(true) && self.peer_listen.is_none() {
            return Err("peer admission requires an explicit peer listener".to_owned());
        }
        if self.peer_advertise.is_some() && self.peer_listen.is_none() {
            return Err("peer advertisement requires a peer listener".to_owned());
        }
        if let Some(bind) = self.peer_listen {
            let advertised = self.peer_advertise.unwrap_or(bind);
            if advertised.ip().is_unspecified()
                || advertised.ip().is_multicast()
                || (self.peer_advertise.is_some() && advertised.port() == 0)
            {
                return Err("peer advertisement requires a unicast IP and explicit nonzero port; wildcard binds need --advertise.peers".to_owned());
            }
        }
        if self.state_dir.is_none() {
            self.state_dir = Some(default_state_dir()?);
        }
        if self
            .state_dir
            .as_ref()
            .is_some_and(|path| !path.is_absolute())
        {
            return Err("worker state directory must be an absolute path".to_owned());
        }
        if self.listen.is_empty() {
            self.listen
                .push(ListenAddress::Unix(orishu::client::default_socket_path()));
        }

        if self
            .listen
            .iter()
            .any(|address| matches!(address, ListenAddress::Tcp(_)))
            && self.tls_cert.is_none()
            && self.tls_key.is_none()
        {
            return Err("TCP client listeners require a TLS certificate and key".to_owned());
        }

        self.validate_placement()?;

        match (&self.tls_cert, &self.tls_key) {
            (None, None) | (Some(_), Some(_)) => Ok(self),
            (Some(_), None) => {
                Err("TLS certificate configured without a matching TLS private key".to_string())
            }
            (None, Some(_)) => {
                Err("TLS private key configured without a matching TLS certificate".to_string())
            }
        }
    }

    /// Reject every placement that cannot be enforced, before any listener is
    /// bound. An unenforceable selection must fail startup rather than degrade
    /// to an unconstrained socket, so there is no warning path here.
    ///
    /// Sharing one interface between both roles is deliberate and allowed;
    /// only unenforceable combinations are refused. See
    /// [ADR 0026](../../../docs/adr/0026-worker-network-interface-placement.md).
    fn validate_placement(&self) -> Result<(), String> {
        if let Some(interface) = &self.peer_interface {
            let Some(bind) = self.peer_listen else {
                return Err(
                    "peer interface placement requires an explicit peer listener; \
                     set --listen.peers or spec.listen.peers"
                        .to_owned(),
                );
            };
            if !bind.ip().is_unspecified() {
                return Err(format!(
                    "peer interface placement requires a wildcard peer bind; \
                     replace {bind} with 0.0.0.0:{port} or [::]:{port} and keep \
                     --advertise.peers for the reachable address",
                    port = bind.port()
                ));
            }
            interface
                .resolve()
                .map_err(|error| format!("peer interface placement is unavailable: {error}"))?;
        }

        if let Some(interface) = &self.client_interface {
            let mut placed_any = false;
            for address in &self.listen {
                let ListenAddress::Tcp(bind) = address else {
                    continue;
                };
                placed_any = true;
                if !bind.ip().is_unspecified() {
                    return Err(format!(
                        "client interface placement requires wildcard TCP client binds; \
                         replace {bind} with 0.0.0.0:{port} or [::]:{port}",
                        port = bind.port()
                    ));
                }
            }
            if !placed_any {
                return Err(
                    "client interface placement requires a TCP client listener; \
                     Unix client sockets have no interface to select"
                        .to_owned(),
                );
            }
            interface
                .resolve()
                .map_err(|error| format!("client interface placement is unavailable: {error}"))?;
        }

        Ok(())
    }
}

#[derive(Debug, Default, Deserialize)]
struct WorkerConfigFile {
    name: Option<orishu_membership::WorkerName>,
    #[serde(default)]
    cluster: ClusterConfig,
    #[serde(default)]
    spec: WorkerSpec,
}

#[derive(Debug, Default, Deserialize)]
struct ClusterConfig {
    name: Option<orishu_membership::ClusterName>,
}

#[derive(Debug, Default, Deserialize)]
struct WorkerSpec {
    #[serde(default)]
    logging: LoggingConfig,
    #[serde(default)]
    tracing: TracingConfig,
    #[serde(default)]
    observability: ObservabilityConfig,
    #[serde(default)]
    accepts: WorkerAccepts,
    #[serde(rename = "stateDir")]
    state_dir: Option<PathBuf>,
    #[serde(default)]
    listen: WorkerListen,
    #[serde(default)]
    advertise: WorkerAdvertise,
    #[serde(default)]
    interface: WorkerInterface,
    #[serde(default)]
    tls: WorkerTls,
}

/// Per-role interface placement. Strict about its own keys: a silently ignored
/// typo here would leave traffic unplaced while the operator believed it was
/// isolated, which is the failure mode ADR 0026 exists to remove.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkerInterface {
    peers: Option<InterfaceName>,
    clients: Option<InterfaceName>,
}

#[derive(Debug, Default, Deserialize)]
struct WorkerAccepts {
    peers: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
struct WorkerListen {
    #[serde(default)]
    clients: Vec<String>,
    #[serde(default)]
    peers: Vec<std::net::SocketAddr>,
}

#[derive(Debug, Default, Deserialize)]
struct WorkerAdvertise {
    #[serde(default)]
    peers: Vec<std::net::SocketAddr>,
}

#[derive(Debug, Default, Deserialize)]
struct WorkerTls {
    cert: Option<PathBuf>,
    key: Option<PathBuf>,
}

fn parse_config_str(contents: &str, source: &Path) -> Result<RuntimeConfig, String> {
    let parsed: WorkerConfigFile = serde_yaml::from_str(contents)
        .map_err(|e| format!("failed to parse config file {}: {e}", source.display()))?;
    if parsed.spec.listen.peers.len() > 1 || parsed.spec.advertise.peers.len() > 1 {
        return Err("formation PoC currently supports one peer listener/advertisement".to_owned());
    }

    let mut listen = Vec::new();
    for value in parsed.spec.listen.clients {
        let addr = value.parse::<ListenAddress>().map_err(|e| {
            format!(
                "invalid listen.clients entry {value:?} in {}: {e}",
                source.display()
            )
        })?;
        listen.push(addr);
    }

    Ok(RuntimeConfig {
        logging: parsed.spec.logging,
        tracing: parsed.spec.tracing,
        observability: parsed.spec.observability,
        state_dir: parsed.spec.state_dir,
        worker_name: parsed.name,
        cluster_name: parsed.cluster.name,
        listen,
        peer_listen: parsed.spec.listen.peers.first().copied(),
        peer_advertise: parsed.spec.advertise.peers.first().copied(),
        peer_interface: parsed.spec.interface.peers,
        client_interface: parsed.spec.interface.clients,
        accepts_peers: parsed.spec.accepts.peers,
        tls_cert: parsed.spec.tls.cert,
        tls_key: parsed.spec.tls.key,
    })
}

fn default_state_dir() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("XDG_STATE_HOME").filter(|value| !value.is_empty()) {
        let path = PathBuf::from(path);
        if !path.is_absolute() {
            return Err("XDG_STATE_HOME must be an absolute path".to_owned());
        }
        return Ok(path.join("orishu/worker"));
    }
    std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(|path| PathBuf::from(path).join(".local/state/orishu/worker"))
        .ok_or_else(|| {
            "configure --state-dir when HOME and XDG_STATE_HOME are unavailable".to_owned()
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn parse(contents: &str) -> RuntimeConfig {
        parse_config_str(contents, Path::new("/test/orishu-worker.conf")).unwrap()
    }

    #[test]
    fn diagnostics_are_opt_in_feature_checked_and_loopback_only() {
        assert_ne!(
            RuntimeConfig::default()
                .finalize()
                .unwrap()
                .observability
                .enabled,
            Some(true)
        );
        let config = parse("spec:\n  observability:\n    enabled: true\n");
        if cfg!(feature = "observability") {
            assert_eq!(
                config.finalize().unwrap().observability.bind.unwrap(),
                "127.0.0.1:9168".parse().unwrap()
            );
            for bind in ["0.0.0.0:9168", "192.0.2.1:9168", "[::]:9168"] {
                assert!(
                    parse(&format!(
                        "spec:\n  observability:\n    enabled: true\n    bind: '{bind}'\n"
                    ))
                    .finalize()
                    .is_err()
                );
            }
        } else {
            assert!(config.finalize().unwrap_err().contains("build feature"));
        }
        assert!(
            parse_config_str(
                "spec:\n  observability:\n    enabld: true\n",
                Path::new("test")
            )
            .is_err()
        );
        let mut disabled =
            parse("spec:\n  observability:\n    enabled: true\n    bind: '127.0.0.1:9999'\n");
        disabled.observability.enabled = Some(false);
        assert_eq!(
            disabled.finalize().unwrap().observability.enabled,
            Some(false)
        );
    }

    #[test]
    fn diagnostics_route_groups_are_explicit_and_cannot_enable_an_empty_listener() {
        for metrics in [false, true] {
            for probes in [false, true] {
                let mut config = parse(&format!(
                    "spec:\n  observability:\n    metrics: {metrics}\n    probes: {probes}\n"
                ));
                assert_eq!(config.observability.metrics_enabled(), metrics);
                assert_eq!(config.observability.probes_enabled(), probes);
                assert_ne!(
                    config.clone().finalize().unwrap().observability.enabled,
                    Some(true)
                );
                config.observability.enabled = Some(true);
                if cfg!(feature = "observability") && (metrics || probes) {
                    assert!(config.finalize().is_ok());
                } else {
                    assert!(config.finalize().is_err());
                }
            }
        }
        let defaults = ObservabilityConfig::default();
        assert!(defaults.metrics_enabled());
        assert!(defaults.probes_enabled());
    }

    #[test]
    fn admission_is_explicit_and_requires_listener() {
        assert_eq!(
            RuntimeConfig::default().finalize().unwrap().accepts_peers,
            None
        );
        assert!(
            parse("spec:\n  accepts:\n    peers: true\n")
                .finalize()
                .is_err()
        );
        let mut config =
            parse("spec:\n  accepts:\n    peers: true\n  listen:\n    peers: ['127.0.0.1:0']\n")
                .finalize()
                .unwrap();
        assert_eq!(config.accepts_peers, Some(true));
        config.accepts_peers = Some(false);
        assert_eq!(config.finalize().unwrap().accepts_peers, Some(false));
        assert!(
            parse_config_str("spec:\n  accepts:\n    peers: invalid\n", Path::new("test")).is_err()
        );
    }

    /// Every placement that cannot be enforced must be refused before a
    /// listener exists. There is no warn-and-continue path: a worker that
    /// starts unplaced while the operator believes otherwise is the exact
    /// failure ADR 0026 exists to remove.
    #[test]
    fn unenforceable_interface_placement_is_rejected_before_binding() {
        let interface = |name: &str| Some(name.parse().expect("valid interface name"));

        // A peer interface with no peer listener has no socket to place.
        assert!(
            RuntimeConfig {
                peer_interface: interface("lo"),
                ..Default::default()
            }
            .finalize()
            .is_err()
        );

        // A concrete peer bind cannot be reconciled with a device: the kernel
        // accepts an address belonging to another interface and then matches
        // no ingress at all.
        assert!(
            RuntimeConfig {
                peer_listen: Some("127.0.0.1:6681".parse().unwrap()),
                peer_advertise: Some("127.0.0.1:6681".parse().unwrap()),
                peer_interface: interface("lo"),
                ..Default::default()
            }
            .finalize()
            .is_err()
        );

        // An interface that cannot be used is always rejected. Only Linux can
        // say *why* by name; elsewhere the reason is the unsupported platform,
        // so assert the naming only where resolution actually happens.
        let error = RuntimeConfig {
            peer_listen: Some("0.0.0.0:6681".parse().unwrap()),
            peer_advertise: Some("127.0.0.1:6681".parse().unwrap()),
            peer_interface: interface("orishunodev0"),
            ..Default::default()
        }
        .finalize()
        .expect_err("unusable interface must fail startup");
        if cfg!(target_os = "linux") {
            assert!(error.contains("orishunodev0"), "{error}");
        } else {
            assert!(error.contains("Linux"), "{error}");
        }

        // A client interface needs a TCP listener; Unix sockets have none.
        assert!(
            RuntimeConfig {
                listen: vec![ListenAddress::Unix("/run/orishu/worker.sock".into())],
                client_interface: interface("lo"),
                ..Default::default()
            }
            .finalize()
            .is_err()
        );

        // Concrete client binds are refused for the same reason as peers.
        assert!(
            RuntimeConfig {
                listen: vec![ListenAddress::Tcp("127.0.0.1:6680".parse().unwrap())],
                client_interface: interface("lo"),
                tls_cert: Some(PathBuf::from("/tmp/cert.pem")),
                tls_key: Some(PathBuf::from("/tmp/key.pem")),
                ..Default::default()
            }
            .finalize()
            .is_err()
        );
    }

    /// Wildcard binds plus a device are the supported form, and naming one
    /// interface for both roles is a deliberate operator choice, not a
    /// fallback, so it must be accepted.
    ///
    /// Linux-only: every assertion here expects an interface to resolve, which
    /// is exactly what other platforms cannot do.
    #[cfg(target_os = "linux")]
    #[test]
    fn wildcard_placement_is_accepted_and_roles_may_share_one_interface() {
        let config = RuntimeConfig {
            listen: vec![ListenAddress::Tcp("0.0.0.0:6680".parse().unwrap())],
            peer_listen: Some("0.0.0.0:6681".parse().unwrap()),
            peer_advertise: Some("127.0.0.1:6681".parse().unwrap()),
            peer_interface: Some("lo".parse().unwrap()),
            client_interface: Some("lo".parse().unwrap()),
            tls_cert: Some(PathBuf::from("/tmp/cert.pem")),
            tls_key: Some(PathBuf::from("/tmp/key.pem")),
            ..Default::default()
        }
        .finalize()
        .expect("wildcard placement on an existing interface is enforceable");
        assert_eq!(config.peer_interface, config.client_interface);

        // Independent selection remains possible; placement is per role.
        let split = RuntimeConfig {
            listen: vec![ListenAddress::Tcp("[::]:6680".parse().unwrap())],
            peer_listen: Some("0.0.0.0:6681".parse().unwrap()),
            peer_advertise: Some("127.0.0.1:6681".parse().unwrap()),
            peer_interface: Some("lo".parse().unwrap()),
            client_interface: None,
            tls_cert: Some(PathBuf::from("/tmp/cert.pem")),
            tls_key: Some(PathBuf::from("/tmp/key.pem")),
            ..Default::default()
        }
        .finalize()
        .expect("one placed role and one unplaced role is valid");
        assert!(split.client_interface.is_none());
    }

    /// Where placement cannot be enforced it must be refused, never accepted
    /// and quietly ignored. An unplaced worker that believes it is isolated is
    /// worse than one that refuses to start.
    #[cfg(not(target_os = "linux"))]
    #[test]
    fn placement_is_rejected_as_unsupported_off_linux() {
        for (peer, client) in [(Some("lo"), None), (None, Some("lo"))] {
            let error = RuntimeConfig {
                listen: vec![ListenAddress::Tcp("0.0.0.0:6680".parse().unwrap())],
                peer_listen: Some("0.0.0.0:6681".parse().unwrap()),
                peer_advertise: Some("127.0.0.1:6681".parse().unwrap()),
                peer_interface: peer.map(|name| name.parse().unwrap()),
                client_interface: client.map(|name| name.parse().unwrap()),
                tls_cert: Some(PathBuf::from("/tmp/cert.pem")),
                tls_key: Some(PathBuf::from("/tmp/key.pem")),
                ..Default::default()
            }
            .finalize()
            .expect_err("placement is only enforceable on Linux");
            assert!(error.contains("Linux"), "{error}");
        }

        // An unconfigured worker is unaffected on every platform.
        assert!(
            RuntimeConfig {
                peer_listen: Some("127.0.0.1:6681".parse().unwrap()),
                ..Default::default()
            }
            .finalize()
            .is_ok()
        );
    }

    /// The YAML block is strict about its own keys, and placement survives the
    /// file/environment/command-line precedence chain unchanged.
    #[test]
    fn yaml_interface_placement_parses_and_rejects_unknown_keys() {
        let config = parse(
            "spec:\n  interface:\n    peers: eth0\n    clients: eth1\n  listen:\n    peers: ['0.0.0.0:6681']\n",
        );
        assert_eq!(config.peer_interface.unwrap().as_str(), "eth0");
        assert_eq!(config.client_interface.unwrap().as_str(), "eth1");

        assert!(
            parse_config_str("spec:\n  interface:\n    peer: eth0\n", Path::new("test")).is_err(),
            "a misspelled key inside the placement block must not be ignored"
        );
        assert!(
            parse_config_str(
                "spec:\n  interface:\n    peers: 'eth 0'\n",
                Path::new("test")
            )
            .is_err(),
            "interface names go through the same validation as the CLI"
        );
        assert!(
            parse_config_str(
                "spec:\n  interface:\n    peers: ethernet01234567\n",
                Path::new("test")
            )
            .is_err(),
            "names longer than IFNAMSIZ - 1 cannot reach a socket option"
        );
    }

    #[test]
    fn peer_listener_requires_explicit_usable_advertisement() {
        let config = parse("spec:\n  listen:\n    peers: ['127.0.0.1:0']\n")
            .finalize()
            .unwrap();
        assert_eq!(config.peer_listen.unwrap().port(), 0);
        assert!(config.peer_advertise.is_none());
        for (listen, advertise) in [
            (None, Some("127.0.0.1:6681")),
            (Some("0.0.0.0:6681"), None),
            (Some("127.0.0.1:0"), Some("127.0.0.1:0")),
            (Some("127.0.0.1:6681"), Some("224.0.0.1:6681")),
        ] {
            assert!(
                RuntimeConfig {
                    peer_listen: listen.map(|s| s.parse().unwrap()),
                    peer_advertise: advertise.map(|s| s.parse().unwrap()),
                    ..Default::default()
                }
                .finalize()
                .is_err()
            );
        }
        assert!(
            parse_config_str(
                "spec:\n  listen:\n    peers: ['127.0.0.1:1', '127.0.0.1:2']\n",
                Path::new("test")
            )
            .is_err()
        );
        let config = parse("spec:\n  listen:\n    peers: ['0.0.0.0:6681']\n  advertise:\n    peers: ['192.0.2.1:6681']\n").finalize().unwrap();
        assert_eq!(config.peer_advertise.unwrap().to_string(), "192.0.2.1:6681");
    }

    #[test]
    fn parses_identity_labels_and_private_state_location() {
        let config = parse(
            "name: same-worker\ncluster:\n  name: same-cluster\nspec:\n  stateDir: /private/worker-a\n",
        );
        assert_eq!(config.worker_name.unwrap().as_str(), "same-worker");
        assert_eq!(config.cluster_name.unwrap().as_str(), "same-cluster");
        assert_eq!(config.state_dir, Some(PathBuf::from("/private/worker-a")));
        assert!(
            RuntimeConfig {
                state_dir: Some(PathBuf::from("relative")),
                ..Default::default()
            }
            .finalize()
            .is_err()
        );
    }

    #[test]
    fn parses_systemd_friendly_worker_config() {
        let cfg = parse(
            r#"
spec:
  listen:
    clients:
      - /run/orishu/worker.sock
  tls:
    cert: /etc/orishu/pki/worker.crt
    key: /etc/orishu/pki/worker.key
"#,
        );

        assert_eq!(
            cfg.listen,
            vec![ListenAddress::Unix(PathBuf::from(
                "/run/orishu/worker.sock"
            ))]
        );
        assert_eq!(
            cfg.tls_cert,
            Some(PathBuf::from("/etc/orishu/pki/worker.crt"))
        );
        assert_eq!(
            cfg.tls_key,
            Some(PathBuf::from("/etc/orishu/pki/worker.key"))
        );
    }

    #[test]
    fn cli_values_override_config_file_values() {
        let mut cfg = parse(
            r#"
spec:
  listen:
    clients:
      - /run/orishu/worker.sock
  tls:
    cert: /etc/orishu/pki/worker.crt
    key: /etc/orishu/pki/worker.key
"#,
        );

        cfg.apply_cli(
            &[ListenAddress::Tcp("127.0.0.1:6680".parse().unwrap())],
            Some(Path::new("/run/credentials/orishu-worker/worker.crt")),
            Some(Path::new("/run/credentials/orishu-worker/worker.key")),
        );

        assert_eq!(
            cfg.listen,
            vec![ListenAddress::Tcp("127.0.0.1:6680".parse().unwrap())]
        );
        assert_eq!(
            cfg.tls_cert,
            Some(PathBuf::from("/run/credentials/orishu-worker/worker.crt"))
        );
        assert_eq!(
            cfg.tls_key,
            Some(PathBuf::from("/run/credentials/orishu-worker/worker.key"))
        );
    }

    #[test]
    fn cli_cert_can_pair_with_config_key() {
        let mut cfg = parse(
            r#"
spec:
  listen:
    clients:
      - /run/orishu/worker.sock
  tls:
    key: /etc/orishu/pki/worker.key
"#,
        );

        cfg.apply_cli(
            &[],
            Some(Path::new("/run/credentials/orishu-worker/worker.crt")),
            None,
        );

        let cfg = cfg.finalize().unwrap();
        assert_eq!(
            cfg.tls_cert,
            Some(PathBuf::from("/run/credentials/orishu-worker/worker.crt"))
        );
        assert_eq!(
            cfg.tls_key,
            Some(PathBuf::from("/etc/orishu/pki/worker.key"))
        );
    }

    #[test]
    fn cli_key_can_pair_with_config_cert() {
        let mut cfg = parse(
            r#"
spec:
  listen:
    clients:
      - /run/orishu/worker.sock
  tls:
    cert: /etc/orishu/pki/worker.crt
"#,
        );

        cfg.apply_cli(
            &[],
            None,
            Some(Path::new("/run/credentials/orishu-worker/worker.key")),
        );

        let cfg = cfg.finalize().unwrap();
        assert_eq!(
            cfg.tls_cert,
            Some(PathBuf::from("/etc/orishu/pki/worker.crt"))
        );
        assert_eq!(
            cfg.tls_key,
            Some(PathBuf::from("/run/credentials/orishu-worker/worker.key"))
        );
    }

    #[test]
    fn finalize_uses_default_socket_when_no_listener_is_configured() {
        let cfg = RuntimeConfig::default().finalize().unwrap();

        assert_eq!(
            cfg.listen,
            vec![ListenAddress::Unix(orishu::client::default_socket_path())]
        );
    }

    #[test]
    fn finalize_rejects_partial_tls_configuration() {
        let err = RuntimeConfig {
            listen: vec![ListenAddress::Unix(PathBuf::from(
                "/run/orishu/worker.sock",
            ))],
            tls_cert: Some(PathBuf::from("/etc/orishu/pki/worker.crt")),
            tls_key: None,
            ..RuntimeConfig::default()
        }
        .finalize()
        .unwrap_err();

        assert!(err.contains("TLS certificate configured without a matching TLS private key"));
    }
}

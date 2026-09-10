use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::ListenAddress;
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
    tls: WorkerTls,
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

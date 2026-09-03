use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::ListenAddress;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub listen: Vec<ListenAddress>,
    pub tls_cert: Option<PathBuf>,
    pub tls_key: Option<PathBuf>,
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
        if self.listen.is_empty() {
            self.listen
                .push(ListenAddress::Unix(orishu::client::default_socket_path()));
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
    #[serde(default)]
    spec: WorkerSpec,
}

#[derive(Debug, Default, Deserialize)]
struct WorkerSpec {
    #[serde(default)]
    listen: WorkerListen,
    #[serde(default)]
    tls: WorkerTls,
}

#[derive(Debug, Default, Deserialize)]
struct WorkerListen {
    #[serde(default)]
    clients: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct WorkerTls {
    cert: Option<PathBuf>,
    key: Option<PathBuf>,
}

fn parse_config_str(contents: &str, source: &Path) -> Result<RuntimeConfig, String> {
    let parsed: WorkerConfigFile = serde_yaml::from_str(contents)
        .map_err(|e| format!("failed to parse config file {}: {e}", source.display()))?;

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
        listen,
        tls_cert: parsed.spec.tls.cert,
        tls_key: parsed.spec.tls.key,
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
        }
        .finalize()
        .unwrap_err();

        assert!(err.contains("TLS certificate configured without a matching TLS private key"));
    }
}

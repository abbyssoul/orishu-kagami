//! Typed startup contract for optional bounded local OTLP delivery.
use clap::Args;
use serde::Deserialize;
use std::path::PathBuf;

pub use orishu_worker::trace_destination::TraceEndpoint;

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Args)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TracingConfig {
    /// Enable bounded local operation export; requires the otlp-tracing feature.
    #[arg(long = "tracing.enabled", env = "ORISHU_TRACING_ENABLED", action = clap::ArgAction::Set)]
    pub enabled: Option<bool>,
    /// Explicit OTLP/HTTP trace URL, including its path; no implicit destination.
    #[arg(long = "tracing.endpoint", env = "ORISHU_TRACING_ENDPOINT")]
    pub endpoint: Option<TraceEndpoint>,
    /// Explicit PEM collector trust roots; never the worker peer CA.
    #[arg(long = "tracing.ca-file", env = "ORISHU_TRACING_CA_FILE")]
    pub ca_file: Option<PathBuf>,
    /// PEM collector-client certificate chain; requires a matching key setting.
    #[arg(
        long = "tracing.client-cert-file",
        env = "ORISHU_TRACING_CLIENT_CERT_FILE"
    )]
    pub client_cert_file: Option<PathBuf>,
    /// Private PEM collector-client key; never the worker identity key.
    #[arg(
        long = "tracing.client-key-file",
        env = "ORISHU_TRACING_CLIENT_KEY_FILE"
    )]
    pub client_key_file: Option<PathBuf>,
    /// Private file containing a collector-only bearer token; HTTPS required.
    #[arg(
        long = "tracing.bearer-token-file",
        env = "ORISHU_TRACING_BEARER_TOKEN_FILE"
    )]
    pub bearer_token_file: Option<PathBuf>,
    /// Head-sampling probability in parts per million (default: 1000).
    #[arg(long = "tracing.sample-ppm", env = "ORISHU_TRACING_SAMPLE_PPM")]
    pub sample_ppm: Option<u32>,
    /// Maximum queued completed spans (default: 1024; maximum: 4096).
    #[arg(long = "tracing.queue-capacity", env = "ORISHU_TRACING_QUEUE_CAPACITY")]
    pub queue_capacity: Option<u32>,
    /// Maximum spans per export batch (default: 128; maximum: 256).
    #[arg(long = "tracing.batch-size", env = "ORISHU_TRACING_BATCH_SIZE")]
    pub batch_size: Option<u32>,
    /// Maximum concurrently active sampled spans (default: 128; maximum: 4096).
    #[arg(
        long = "tracing.active-span-capacity",
        env = "ORISHU_TRACING_ACTIVE_SPAN_CAPACITY"
    )]
    pub active_span_capacity: Option<u32>,
    /// Maximum encoded bytes per export request (default: 1048576; maximum: 4194304).
    #[arg(
        long = "tracing.export-max-bytes",
        env = "ORISHU_TRACING_EXPORT_MAX_BYTES"
    )]
    pub export_max_bytes: Option<u32>,
    /// Maximum collector response bytes consumed (default: 16384; maximum: 65536).
    #[arg(
        long = "tracing.response-max-bytes",
        env = "ORISHU_TRACING_RESPONSE_MAX_BYTES"
    )]
    pub response_max_bytes: Option<u32>,
    /// Batch flush interval in milliseconds (default: 1000).
    #[arg(
        long = "tracing.flush-interval-ms",
        env = "ORISHU_TRACING_FLUSH_INTERVAL_MS"
    )]
    pub flush_interval_ms: Option<u32>,
    /// Whole export-attempt timeout in milliseconds (default: 2000).
    #[arg(
        long = "tracing.export-timeout-ms",
        env = "ORISHU_TRACING_EXPORT_TIMEOUT_MS"
    )]
    pub export_timeout_ms: Option<u32>,
    /// Whole shutdown flush budget in milliseconds (default: 3000).
    #[arg(
        long = "tracing.shutdown-timeout-ms",
        env = "ORISHU_TRACING_SHUTDOWN_TIMEOUT_MS"
    )]
    pub shutdown_timeout_ms: Option<u32>,
}

impl TracingConfig {
    /// Apply only explicitly supplied CLI/environment values over file values.
    pub fn overlay(&mut self, overrides: &Self) {
        if let Some(endpoint) = &overrides.endpoint {
            self.endpoint = Some(endpoint.clone());
        }
        for (current, supplied) in [
            (&mut self.ca_file, &overrides.ca_file),
            (&mut self.client_cert_file, &overrides.client_cert_file),
            (&mut self.client_key_file, &overrides.client_key_file),
            (&mut self.bearer_token_file, &overrides.bearer_token_file),
        ] {
            if let Some(path) = supplied {
                *current = Some(path.clone());
            }
        }
        macro_rules! overlay {
            ($($field:ident),+) => { $(if overrides.$field.is_some() {
                self.$field = overrides.$field;
            })+ };
        }
        overlay!(
            enabled,
            sample_ppm,
            queue_capacity,
            batch_size,
            active_span_capacity,
            export_max_bytes,
            response_max_bytes,
            flush_interval_ms,
            export_timeout_ms,
            shutdown_timeout_ms
        );
    }

    /// Validate even disabled settings, so impossible budgets do not lurk in
    /// a staged configuration. No file access, DNS, collector or task startup.
    pub fn validate(&self) -> Result<(), String> {
        if let Some(endpoint) = &self.endpoint {
            endpoint.validate()?;
        }
        let tls_files = [
            ("tracing.ca-file", &self.ca_file),
            ("tracing.client-cert-file", &self.client_cert_file),
            ("tracing.client-key-file", &self.client_key_file),
            ("tracing.bearer-token-file", &self.bearer_token_file),
        ];
        for (name, path) in tls_files {
            if path.as_ref().is_some_and(|path| {
                path.as_os_str().is_empty() || path.as_os_str().as_encoded_bytes().len() > 4096
            }) {
                return Err(format!(
                    "{name} requires a nonempty path of at most 4096 bytes"
                ));
            }
        }
        if self.client_cert_file.is_some() != self.client_key_file.is_some() {
            return Err("tracing client certificate and key must be configured together".into());
        }
        if tls_files.iter().any(|(_, path)| path.is_some())
            && !self.endpoint.as_ref().is_some_and(|endpoint| {
                endpoint
                    .validated_uri()
                    .is_ok_and(|uri| uri.scheme_str() == Some("https"))
            })
        {
            return Err(
                "tracing TLS and credential files require an explicit HTTPS endpoint".into(),
            );
        }
        if self.sample_ppm.unwrap_or(1000) > 1_000_000 {
            return Err("tracing.sample-ppm must be in 0..=1000000".into());
        }
        let queue = self.queue_capacity.unwrap_or(1024);
        let batch = self.batch_size.unwrap_or(128);
        if !(1..=4096).contains(&queue) {
            return Err("tracing.queue-capacity must be in 1..=4096".into());
        }
        if !(1..=256).contains(&batch) || batch > queue {
            return Err(
                "tracing.batch-size must be in 1..=256 and no larger than queue capacity".into(),
            );
        }
        if !(1..=4096).contains(&self.active_span_capacity.unwrap_or(128)) {
            return Err("tracing.active-span-capacity must be in 1..=4096".into());
        }
        if !(1024..=4_194_304).contains(&self.export_max_bytes.unwrap_or(1_048_576)) {
            return Err("tracing.export-max-bytes must be in 1024..=4194304".into());
        }
        if !(1024..=65_536).contains(&self.response_max_bytes.unwrap_or(16_384)) {
            return Err("tracing.response-max-bytes must be in 1024..=65536".into());
        }
        if !(100..=10_000).contains(&self.flush_interval_ms.unwrap_or(1000)) {
            return Err("tracing.flush-interval-ms must be in 100..=10000".into());
        }
        if !(100..=10_000).contains(&self.export_timeout_ms.unwrap_or(2000)) {
            return Err("tracing.export-timeout-ms must be in 100..=10000".into());
        }
        if !(100..=10_000).contains(&self.shutdown_timeout_ms.unwrap_or(3000)) {
            return Err("tracing.shutdown-timeout-ms must be in 100..=10000".into());
        }
        if self.enabled == Some(true) {
            if !cfg!(feature = "otlp-tracing") {
                return Err("OTLP tracing is not available in this build".into());
            }
            if self.endpoint.is_none() {
                return Err("OTLP tracing is not available without an explicit endpoint".into());
            }
        }
        Ok(())
    }

    #[cfg(feature = "otlp-tracing")]
    pub fn prepare(
        &self,
    ) -> Result<
        Option<(
            orishu_worker::trace_export::SpanQueue,
            orishu_worker::trace_export::ExportLoop,
        )>,
        String,
    > {
        use orishu_worker::trace_export::{BatchLimits, CollectorFiles, ExportLoop, SpanQueue};
        use std::time::Duration;
        self.validate()?;
        if self.enabled != Some(true) {
            return Ok(None);
        }
        let invalid =
            || "invalid trace exporter configuration or collector credential files".to_owned();
        let delivery = CollectorFiles {
            ca: self.ca_file.as_deref(),
            certificate: self.client_cert_file.as_deref(),
            key: self.client_key_file.as_deref(),
            token: self.bearer_token_file.as_deref(),
        }
        .prepare(
            self.endpoint.as_ref().ok_or_else(invalid)?,
            Duration::from_millis(self.export_timeout_ms.unwrap_or(2000).into()),
            self.response_max_bytes.unwrap_or(16384) as usize,
        )
        .map_err(|_| invalid())?;
        let (queue, receiver) = SpanQueue::new(
            self.sample_ppm.unwrap_or(1000),
            self.active_span_capacity.unwrap_or(128) as usize,
            self.queue_capacity.unwrap_or(1024) as usize,
        )
        .map_err(|_| invalid())?;
        let limits = BatchLimits::new(
            self.batch_size.unwrap_or(128) as usize,
            self.export_max_bytes.unwrap_or(1048576) as usize,
        )
        .map_err(|_| invalid())?;
        let exporter = ExportLoop::new(
            receiver,
            delivery,
            limits,
            Duration::from_millis(self.flush_interval_ms.unwrap_or(1000).into()),
            Duration::from_millis(self.shutdown_timeout_ms.unwrap_or(3000).into()),
        )
        .map_err(|_| invalid())?;
        Ok(Some((queue, exporter)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collector_credentials_are_explicit_paired_and_https_only() {
        let mut config: TracingConfig = serde_yaml::from_str(
            "endpoint: https://collector.invalid/v1/traces\ncaFile: nonexistent-ca.pem\nclientCertFile: nonexistent-client.pem\nclientKeyFile: nonexistent-key.pem\nbearerTokenFile: nonexistent-token\n",
        ).unwrap();
        // Disabled settings validate structure without opening credential files.
        assert!(config.validate().is_ok());
        config.overlay(&TracingConfig {
            ca_file: Some("replacement-ca.pem".into()),
            ..Default::default()
        });
        assert_eq!(config.ca_file, Some("replacement-ca.pem".into()));
        assert_eq!(config.client_key_file, Some("nonexistent-key.pem".into()));
        for endpoint in [
            None,
            Some("http://127.0.0.1:4318/v1/traces".parse().unwrap()),
        ] {
            let mut invalid = config.clone();
            invalid.endpoint = endpoint;
            assert!(invalid.validate().unwrap_err().contains("HTTPS"));
        }
        for (cert, key) in [
            (Some("cert.pem".into()), None),
            (None, Some("key.pem".into())),
        ] {
            let mut invalid = config.clone();
            invalid.client_cert_file = cert;
            invalid.client_key_file = key;
            assert!(invalid.validate().unwrap_err().contains("together"));
        }
        for path in [PathBuf::new(), PathBuf::from("x".repeat(4097))] {
            let mut invalid = config.clone();
            invalid.bearer_token_file = Some(path);
            assert!(
                invalid
                    .validate()
                    .unwrap_err()
                    .contains("tracing.bearer-token-file")
            );
        }
        config.bearer_token_file = Some("x".repeat(4096).into());
        assert!(config.validate().is_ok());
        config.enabled = Some(true);
        #[cfg(feature = "otlp-tracing")]
        {
            assert!(config.validate().is_ok());
            assert!(config.prepare().is_err()); // Invalid/missing files fail before startup.
        }
        #[cfg(not(feature = "otlp-tracing"))]
        assert!(
            config
                .validate()
                .unwrap_err()
                .contains("OTLP tracing is not available")
        );
    }

    #[test]
    fn destinations_are_explicit_bounded_and_never_expose_rejected_input() {
        for address in [
            "https://collector.example/v1/traces",
            "https://collector.example:4318/otlp/v1/traces",
            "http://127.0.0.1:4318/v1/traces",
            "http://[::1]:4318/v1/traces",
            "https://collector.example:1/v1/traces",
            "https://collector.example:65535/v1/traces",
        ] {
            let endpoint: TraceEndpoint = address.parse().unwrap();
            assert!(endpoint.validate().is_ok(), "{address}");
        }
        for address in [
            "",
            "/v1/traces",
            "http://collector.example/v1/traces",
            "http://localhost:4318/v1/traces",
            "http://192.0.2.1:4318/v1/traces",
            "https://collector.example",
            "https://collector.example:0/v1/traces",
            "https://collector.example:65536/v1/traces",
            "https://collector.example:/v1/traces",
            "https://collector.example:abc/v1/traces",
            "https://user:secret@collector.example/v1/traces",
            "https://collector.example/v1/traces?secret=seed",
            "https://collector.example/v1/traces#secret",
            "https://collector.example/a/../v1/traces",
            "https://collector.example/%2e/v1/traces",
            " https://collector.example/v1/traces",
        ] {
            let endpoint: TraceEndpoint = address.parse().unwrap();
            let error = endpoint.validate().expect_err(address);
            assert!(!error.contains("secret"));
            assert_eq!(format!("{endpoint:?}"), "TraceEndpoint([redacted])");
        }
        let prefix = "https://collector.example/";
        let address = format!("{prefix}{}", "a".repeat(2048 - prefix.len()));
        assert!(address.parse::<TraceEndpoint>().unwrap().validate().is_ok());
        assert!(
            format!("{address}a")
                .parse::<TraceEndpoint>()
                .unwrap()
                .validate()
                .is_err()
        );
    }

    #[test]
    fn active_span_and_exchange_byte_budgets_have_exact_boundaries() {
        for (field, minimum, maximum) in [
            ("activeSpanCapacity", 1, 4096),
            ("exportMaxBytes", 1024, 4_194_304),
            ("responseMaxBytes", 1024, 65_536),
        ] {
            for (value, valid) in [
                (minimum - 1, false),
                (minimum, true),
                (maximum, true),
                (maximum + 1, false),
            ] {
                let config: TracingConfig =
                    serde_yaml::from_str(&format!("{field}: {value}")).unwrap();
                assert_eq!(config.validate().is_ok(), valid, "{field}: {value}");
            }
        }
        let mut file: TracingConfig = serde_yaml::from_str(
            "activeSpanCapacity: 12\nexportMaxBytes: 2048\nresponseMaxBytes: 4096\n",
        )
        .unwrap();
        file.overlay(&TracingConfig {
            response_max_bytes: Some(8192),
            ..Default::default()
        });
        assert_eq!(file.active_span_capacity, Some(12));
        assert_eq!(file.export_max_bytes, Some(2048));
        assert_eq!(file.response_max_bytes, Some(8192));
        assert!(file.validate().is_ok());
    }

    #[test]
    fn file_values_are_overlaid_only_by_explicit_settings() {
        use clap::Parser;
        #[derive(Parser)]
        struct Cli {
            #[command(flatten)]
            tracing: TracingConfig,
        }
        let mut file: TracingConfig = serde_yaml::from_str(
            "enabled: true\nsamplePpm: 123\nqueueCapacity: 32\nbatchSize: 16\n",
        )
        .unwrap();
        let cli = Cli::try_parse_from([
            "worker",
            "--tracing.enabled",
            "false",
            "--tracing.batch-size",
            "8",
        ])
        .unwrap();
        file.overlay(&cli.tracing);
        assert_eq!(file.sample_ppm, Some(123));
        assert_eq!(file.queue_capacity, Some(32));
        assert_eq!(file.batch_size, Some(8));
        assert_eq!(file.enabled, Some(false));
        assert!(file.validate().is_ok());
        for text in [
            "enabld: false",
            "enabled: maybe",
            "samplePpm: -1",
            "queueCapacity: 1.5",
        ] {
            assert!(serde_yaml::from_str::<TracingConfig>(text).is_err());
        }
        assert!(Cli::try_parse_from(["worker", "--tracing.sample-ppm", "-1"]).is_err());
    }

    #[test]
    fn limits_are_finite_and_enabling_is_never_a_silent_noop() {
        assert!(TracingConfig::default().validate().is_ok());
        for value in [0, 1_000_000] {
            assert!(
                TracingConfig {
                    sample_ppm: Some(value),
                    ..Default::default()
                }
                .validate()
                .is_ok()
            );
        }
        for value in [100, 10_000] {
            assert!(
                TracingConfig {
                    flush_interval_ms: Some(value),
                    export_timeout_ms: Some(value),
                    shutdown_timeout_ms: Some(value),
                    ..Default::default()
                }
                .validate()
                .is_ok()
            );
        }
        for invalid in [
            TracingConfig {
                sample_ppm: Some(1_000_001),
                ..Default::default()
            },
            TracingConfig {
                queue_capacity: Some(0),
                ..Default::default()
            },
            TracingConfig {
                queue_capacity: Some(4097),
                ..Default::default()
            },
            TracingConfig {
                batch_size: Some(0),
                ..Default::default()
            },
            TracingConfig {
                batch_size: Some(257),
                ..Default::default()
            },
            TracingConfig {
                queue_capacity: Some(1),
                batch_size: Some(2),
                ..Default::default()
            },
            TracingConfig {
                flush_interval_ms: Some(99),
                ..Default::default()
            },
            TracingConfig {
                export_timeout_ms: Some(10_001),
                ..Default::default()
            },
            TracingConfig {
                shutdown_timeout_ms: Some(0),
                ..Default::default()
            },
            TracingConfig {
                enabled: Some(true),
                ..Default::default()
            },
        ] {
            assert!(invalid.validate().is_err());
        }
    }
}

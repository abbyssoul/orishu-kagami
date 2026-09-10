//! Explicit, independent standard-output logging configuration.
use clap::Args;
use orishu_worker::operational_log::{
    DEFAULT_QUEUE_RECORDS, DEFAULT_SHUTDOWN, MAX_QUEUE_RECORDS, MAX_SHUTDOWN,
};
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Args)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoggingConfig {
    /// Enable bounded structured stdout logs, independently of metrics/tracing.
    #[arg(id = "logging_enabled", long = "logging.enabled", env = "ORISHU_LOGGING_ENABLED", action = clap::ArgAction::Set)]
    pub enabled: Option<bool>,
    /// Preallocated record slots (default: 256; range: 1..=4096).
    #[arg(long = "logging.queue-records", env = "ORISHU_LOGGING_QUEUE_RECORDS")]
    pub queue_records: Option<u32>,
    /// Shutdown drain milliseconds (default: 250; range: 0..=2000).
    #[arg(long = "logging.shutdown-ms", env = "ORISHU_LOGGING_SHUTDOWN_MS")]
    pub shutdown_ms: Option<u32>,
}

impl LoggingConfig {
    pub fn overlay(&mut self, supplied: &Self) {
        if supplied.enabled.is_some() {
            self.enabled = supplied.enabled;
        }
        if supplied.queue_records.is_some() {
            self.queue_records = supplied.queue_records;
        }
        if supplied.shutdown_ms.is_some() {
            self.shutdown_ms = supplied.shutdown_ms;
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=MAX_QUEUE_RECORDS).contains(&self.queued()) {
            return Err("logging.queue-records must be in 1..=4096".into());
        }
        if self.shutdown() > MAX_SHUTDOWN {
            return Err("logging.shutdown-ms must be in 0..=2000".into());
        }
        if self.enabled == Some(true) && !cfg!(unix) {
            return Err("bounded stdout logging currently requires Unix".into());
        }
        Ok(())
    }
    pub fn queued(&self) -> usize {
        self.queue_records
            .map_or(DEFAULT_QUEUE_RECORDS, |value| value as usize)
    }
    pub fn shutdown(&self) -> Duration {
        self.shutdown_ms.map_or(DEFAULT_SHUTDOWN, |value| {
            Duration::from_millis(value.into())
        })
    }
    pub fn prepare(
        &self,
    ) -> Result<
        Option<(
            orishu_worker::operational_log::Log,
            orishu_worker::operational_log::Output,
        )>,
        String,
    > {
        self.validate()?;
        if self.enabled != Some(true) {
            return Ok(None);
        }
        #[cfg(unix)]
        {
            orishu_worker::operational_log::Output::stdout(self.queued(), self.shutdown())
                .map(Some)
                .map_err(|error| error.to_string())
        }
        #[cfg(not(unix))]
        {
            Err("bounded stdout logging currently requires Unix".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flattened_worker_cli_has_unique_argument_ids() {
        use clap::CommandFactory;
        crate::Cli::command().debug_assert();
    }
    #[test]
    fn defaults_are_inert_and_even_disabled_budgets_are_validated() {
        let config = LoggingConfig::default();
        assert!(config.prepare().unwrap().is_none());
        assert_eq!(config.queued(), 256);
        assert_eq!(config.shutdown(), Duration::from_millis(250));
        for count in [0, 4097, u32::MAX] {
            assert!(
                LoggingConfig {
                    queue_records: Some(count),
                    ..Default::default()
                }
                .validate()
                .is_err()
            );
        }
        for millis in [2001, u32::MAX] {
            assert!(
                LoggingConfig {
                    shutdown_ms: Some(millis),
                    ..Default::default()
                }
                .validate()
                .is_err()
            );
        }
        for millis in [0, 2000] {
            assert!(
                LoggingConfig {
                    shutdown_ms: Some(millis),
                    ..Default::default()
                }
                .validate()
                .is_ok()
            );
        }
        assert!(serde_yaml::from_str::<LoggingConfig>("sink: vendor").is_err());
    }
    #[test]
    fn explicit_false_and_zero_overlay_without_resetting_other_file_settings() {
        let mut file: LoggingConfig =
            serde_yaml::from_str("enabled: true\nqueueRecords: 64\nshutdownMs: 100").unwrap();
        file.overlay(&LoggingConfig {
            enabled: Some(false),
            shutdown_ms: Some(0),
            ..Default::default()
        });
        assert_eq!(file.enabled, Some(false));
        assert_eq!(file.queued(), 64);
        assert_eq!(file.shutdown(), Duration::ZERO);
    }
}

//! Worker IO adapters. Domain membership remains in `orishu-membership`.
#[cfg(all(feature = "formation-fault-test", not(debug_assertions)))]
compile_error!("formation-fault-test is forbidden in release builds");
pub mod credentials;
pub mod driver;
pub mod formation_metrics;
pub mod health;
pub mod join_operations;
mod membership_view;
pub mod operational_log;
pub mod peer;
pub mod runtime;
pub mod trace_context;
pub mod trace_destination;
#[cfg(feature = "otlp-tracing")]
pub mod trace_export;
#[cfg(unix)]
pub mod unix_socket;

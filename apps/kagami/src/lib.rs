//! Kagami's window: the imperative shell over the document authority.
//!
//! A library as well as a binary so the parts that are *not* pixels can be
//! tested without one. The message → envelope mapping, the split between the
//! authoritative and client-local queues, and the document lifecycle are all
//! ordinary functions over values; only `view` needs a window.

#[cfg(unix)]
pub mod catalog_form;
#[cfg(unix)]
pub mod component_form;
pub mod document;
pub mod export;
pub mod launch;
pub mod mcp;
pub mod message;
pub mod model;
#[cfg(unix)]
pub mod physics_form;
pub mod plugins;
pub mod run;
#[cfg(unix)]
pub mod scientific;
#[cfg(unix)]
pub mod scientific_effect;
pub mod subscription;
pub mod update;
pub mod view;
pub mod viewport;
#[cfg(unix)]
pub mod workload;
pub mod workload_cli;
#[cfg(unix)]
pub mod workload_preparation;

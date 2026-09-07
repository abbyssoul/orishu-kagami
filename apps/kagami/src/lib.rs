//! Kagami's window: the imperative shell over the document authority.
//!
//! A library as well as a binary so the parts that are *not* pixels can be
//! tested without one. The message → envelope mapping, the split between the
//! authoritative and client-local queues, and the document lifecycle are all
//! ordinary functions over values; only `view` needs a window.

pub mod document;
pub mod launch;
pub mod message;
pub mod model;
pub mod subscription;
pub mod update;
pub mod view;
pub mod viewport;

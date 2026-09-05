use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::model::{
    audit::AuditEvent,
    blocklist::BlocklistAddResult,
    cluster::{ClusterEvent, LeaveResult, LogLine},
    node::DiagnosticResult,
    tombstones::TombstoneRecord,
};

pub mod audit;
pub mod blocklist;
pub mod checkpoint;
pub mod cluster;
pub mod manifest;
pub mod node;
pub mod quantity;
pub mod result;
pub mod storage;
pub mod tombstones;
pub mod workload;

/// New type to define server API route.
/// All existing API routes for clients are listed in the protocol-client.md
#[derive(Debug, Clone)]
pub struct ApiRoute {
    path: Cow<'static, str>,
    query: Option<Cow<'static, str>>,
}

impl ApiRoute {
    /// Create a route from a static string. Usable in `const` contexts.
    pub const fn from_static(v: &'static str) -> Self {
        Self {
            path: Cow::Borrowed(v),
            query: None,
        }
    }

    /// Construct a sub-resource path: `{route}/{id}`.
    pub fn with_id(&self, id: impl std::fmt::Display) -> ApiRoute {
        ApiRoute {
            path: Cow::Owned(format!("{}/{}", self.path, id)),
            query: self.query.clone(),
        }
    }

    /// Set query after path: `{route}?{query}`.
    /// If `q` is empty, the route is returned unchanged (no trailing `?`).
    pub fn with_query(&self, q: impl std::fmt::Display) -> ApiRoute {
        let q_str = q.to_string();
        ApiRoute {
            path: self.path.clone(),
            query: if q_str.is_empty() {
                self.query.clone()
            } else {
                Some(Cow::Owned(q_str))
            },
        }
    }

    /// Set query after path: `{route}?{query}`.
    /// If `q` is empty, the route is returned unchanged (no trailing `?`).
    pub fn with_query_param(&self, q: impl QuerySet) -> ApiRoute {
        let q_str = q.to_query();
        ApiRoute {
            path: self.path.clone(),
            query: if q_str.is_empty() {
                self.query.clone()
            } else {
                Some(Cow::Owned(q_str))
            },
        }
    }
}

impl std::fmt::Display for ApiRoute {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.query {
            Some(q) => write!(f, "{}?{}", self.path, q),
            None => f.write_str(&self.path),
        }
    }
}

// ── Cluster resource ──────────────────────────────────────────────────────────
pub const API_ROUTE_SELF_MEMBERSHIP: ApiRoute = ApiRoute::from_static("membership");

pub const API_ROUTE_CLUSTER: ApiRoute = ApiRoute::from_static("cluster");
pub const API_ROUTE_CLUSTER_LOCK: ApiRoute = ApiRoute::from_static("cluster/lock");
pub const API_ROUTE_CLUSTER_EVENTS: ApiRoute = ApiRoute::from_static("cluster/events");
pub const API_ROUTE_CLUSTER_LOGS: ApiRoute = ApiRoute::from_static("cluster/logs");
pub const API_ROUTE_CLUSTER_AUDIT: ApiRoute = ApiRoute::from_static("cluster/audit-log");
pub const API_ROUTE_CLUSTER_TOKEN: ApiRoute = ApiRoute::from_static("cluster/token");

// ── Node resource ─────────────────────────────────────────────────────────────
pub const API_ROUTE_CLUSTER_NODES: ApiRoute = ApiRoute::from_static("cluster/nodes");
pub const API_ROUTE_CLUSTER_TOMBSTONES: ApiRoute = ApiRoute::from_static("cluster/tombstones");

// ── Blocklist resource ────────────────────────────────────────────────────────
pub const API_ROUTE_CLUSTER_BLOCKLIST: ApiRoute = ApiRoute::from_static("cluster/blocklist");

// ── Workload resource ─────────────────────────────────────────────────────────
pub const API_ROUTE_CLUSTER_WORKLOAD: ApiRoute = ApiRoute::from_static("cluster/workload");
pub const API_ROUTE_CLUSTER_WORKLOAD_CHECK: ApiRoute =
    ApiRoute::from_static("cluster/workload/check");
pub const API_ROUTE_CLUSTER_WORKLOAD_CHECKPOINT: ApiRoute =
    ApiRoute::from_static("cluster/workload/checkpoints");
pub const API_ROUTE_CLUSTER_WORKLOAD_STREAM: ApiRoute =
    ApiRoute::from_static("cluster/workload/stream");

// ── Results resource ──────────────────────────────────────────────────────────
pub const API_ROUTE_CLUSTER_RESULTS: ApiRoute = ApiRoute::from_static("cluster/results");

// ── Query string serialization ────────────────────────────────────────────────

/// Trait for types that can be serialized as a single URL query parameter value.
/// Returns `Some(value)` when the parameter should be included, `None` to skip it.
pub trait ToQueryParam {
    fn to_query_param(&self) -> Option<String>;
}

impl<T: std::fmt::Display> ToQueryParam for Option<T> {
    fn to_query_param(&self) -> Option<String> {
        self.as_ref().map(|v| v.to_string())
    }
}

impl ToQueryParam for Vec<String> {
    fn to_query_param(&self) -> Option<String> {
        if self.is_empty() {
            None
        } else {
            Some(self.join(","))
        }
    }
}

impl ToQueryParam for String {
    fn to_query_param(&self) -> Option<String> {
        if self.is_empty() {
            None
        } else {
            Some(self.clone())
        }
    }
}

/// Trait for types that can be serialized as a single URL query parameter value.
/// Returns `Some(value)` when the parameter should be included, `None` to skip it.
pub trait QuerySet {
    fn to_query(&self) -> String;
}

/// Defines a filter struct and generates a `to_query()` method that serializes
/// non-empty fields into a URL query string.
///
/// Each field uses `as "param_name"` to specify the query parameter key, which
/// may differ from the Rust field name.
///
/// # Example
///
/// ```ignore
/// query_filter! {
///     #[derive(Debug, Clone, Default)]
///     pub struct MyFilter {
///         /// Inclusive lower bound.
///         pub from as "since": Option<DateTime<Utc>>,
///         /// Restrict to these event types.
///         pub event_types as "type": Vec<String>,
///     }
/// }
///
/// let f = MyFilter { from: None, event_types: vec!["A".into()] };
/// assert_eq!(f.to_query(), "type=A");
/// ```
#[macro_export]
macro_rules! query_filter {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident {
            $(
                $(#[$field_meta:meta])*
                $field_vis:vis $field:ident as $param:literal : $type:ty
            ),* $(,)?
        }
    ) => {
        $(#[$meta])*
        $vis struct $name {
            $(
                $(#[$field_meta])*
                $field_vis $field: $type,
            )*
        }

        impl $crate::model::QuerySet for &$name {
            /// Serialize non-empty fields into a URL query string.
            /// Returns an empty string when no parameters are set.
            fn to_query(&self) -> String {
                let mut parts: Vec<String> = Vec::new();
                $(
                    if let Some(v) = $crate::model::ToQueryParam::to_query_param(&self.$field) {
                        parts.push(format!("{}={}", $param, v));
                    }
                )*
                parts.join("&")
            }
        }
    };
}

/// Common API response to data purge requests
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntriesRemoved {
    pub count: u32,
}

/// Opaque pagination data
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaginationCursor(String);

/// Defines a response enum and generates a `TryFrom` impl for each variant,
/// so callers can extract the expected type from a generic response wrapper.
///
/// Each variant `Name(Type)` produces:
/// - the enum variant itself
/// - `impl TryFrom<EnumName> for Type` that matches `Name` and rejects everything else
macro_rules! response_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $( $variant:ident($type:ty) ),* $(,)?
        }
    ) => {
        $(#[$meta])*
        $vis enum $name {
            $( $variant($type) ),*
        }

        $(
            impl TryFrom<$name> for $type {
                type Error = String;
                fn try_from(data: $name) -> std::result::Result<Self, Self::Error> {
                    match data {
                        $name::$variant(v) => Ok(v),
                        _ => Err(concat!("expected ", stringify!($variant), " response").to_string()),
                    }
                }
            }
        )*
    };
}

response_enum! {
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub enum ResponseData {
        ClusterManifest(cluster::Manifest),
        NodeManifest(node::Manifest),
        WorkloadManifest(Option<workload::Manifest>),
        BlocklistEntry(blocklist::Entry),
        ResultRemoved(EntriesRemoved),
        WorkloadCompatibilityReport(cluster::WorkloadCompatibilityReport),

        JoinToken(cluster::JoinToken),
        LockIntent(cluster::LockIntent),
        BlocklistAddResult(BlocklistAddResult),
        WorkloadAccepted(workload::Accepted),
        JoinRequestAccepted(cluster::JoinRequestAccepted),
        LeaveResult(LeaveResult),
        ResultRecord(result::Record),
        CheckpointRecord(checkpoint::Record),
        DiagnosticResult(DiagnosticResult),
        TombstoneRecord(TombstoneRecord),
    }
}

response_enum! {
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub enum ResponseCollection {
        BlocklistEntry(blocklist::Entry),
        ClusterEvent(ClusterEvent),
        LogLine(LogLine),
        AuditEvent(AuditEvent),
        NodeManifest(node::Manifest),
        ResultRecord(result::Record),
        CheckpointRecord(checkpoint::Record),
        TombstoneRecord(TombstoneRecord),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum ApiResponse {
    /// Valid Server response indicating an error
    /// see "Error responses" section in the docs.
    Error {
        /// Machine-readable error code
        code: String,
        /// human-readable description
        message: String,
    },
    Ok {
        data: Option<ResponseData>,
    },
    OkCollection {
        data: Vec<ResponseCollection>,
        next_cursor: Option<PaginationCursor>,
    },
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use pretty_assertions::assert_eq;

    #[allow(clippy::cmp_owned)]
    impl PartialEq<str> for ApiRoute {
        fn eq(&self, other: &str) -> bool {
            self.to_string() == other
        }
    }

    // ── ApiRoute helpers ──────────────────────────────────────────────────

    #[test]
    fn test_api_route_display() {
        assert_eq!(API_ROUTE_CLUSTER_NODES.to_string(), "cluster/nodes");
    }

    #[test]
    fn test_api_route_display_with_query() {
        let route = API_ROUTE_CLUSTER_WORKLOAD.with_query("force");
        assert_eq!(route.to_string(), "cluster/workload?force");
    }

    #[test]
    fn test_api_route_with_id_str() {
        let route = API_ROUTE_CLUSTER_NODES.with_id("abc-123");
        assert_eq!(route, *"cluster/nodes/abc-123");
    }

    #[test]
    fn test_api_route_with_id_display_type() {
        use crate::model::node::NodeId;
        let id = NodeId::from_str("node-42").unwrap();
        let route = API_ROUTE_CLUSTER_NODES.with_id(&id);
        assert_eq!(route, *"cluster/nodes/node-42");
    }

    #[test]
    fn test_api_route_with_id_numeric() {
        let route = API_ROUTE_CLUSTER_RESULTS.with_id(7u64);
        assert_eq!(route, *"cluster/results/7");
    }

    #[test]
    fn test_api_route_with_query_empty_string_is_noop() {
        let route = API_ROUTE_CLUSTER_AUDIT.with_query("");
        assert_eq!(route, *"cluster/audit-log");
    }

    // ── query_filter! / to_query ─────────────────────────────────────────

    #[test]
    fn test_audit_filter_to_query_empty() {
        use crate::model::audit::AuditFilter;
        let filter = &AuditFilter::default();
        assert_eq!(filter.to_query(), "");
    }

    #[test]
    fn test_audit_filter_to_query_single_field() {
        use crate::model::audit::AuditFilter;
        let filter = &AuditFilter {
            event_types: vec!["NodeJoined".into()],
            ..Default::default()
        };
        assert_eq!(filter.to_query(), "type=NodeJoined");
    }

    #[test]
    fn test_audit_filter_to_query_multiple_event_types() {
        use crate::model::audit::AuditFilter;
        let filter = &AuditFilter {
            event_types: vec!["NodeJoined".into(), "WorkloadStarted".into()],
            ..Default::default()
        };
        assert_eq!(filter.to_query(), "type=NodeJoined,WorkloadStarted");
    }

    #[test]
    fn test_audit_filter_to_query_all_fields() {
        use crate::model::audit::AuditFilter;
        use chrono::{TimeZone, Utc};
        let filter = &AuditFilter {
            after: Some(Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap()),
            before: Some(Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap()),
            event_types: vec!["TokenRotated".into()],
        };
        let query = filter.to_query();
        assert!(query.contains("after=2025-01-01"));
        assert!(query.contains("before=2025-06-01"));
        assert!(query.contains("type=TokenRotated"));
    }

    #[test]
    fn test_audit_filter_to_query_with_route() {
        use crate::model::audit::AuditFilter;
        let filter = &AuditFilter {
            event_types: vec!["NodeJoined".into()],
            ..Default::default()
        };
        let route = API_ROUTE_CLUSTER_AUDIT.with_query(filter.to_query());
        assert_eq!(route, *"cluster/audit-log?type=NodeJoined");
    }

    #[test]
    fn test_audit_filter_empty_with_route_no_trailing_question_mark() {
        use crate::model::audit::AuditFilter;
        let filter = &AuditFilter::default();
        let route = API_ROUTE_CLUSTER_AUDIT.with_query(filter.to_query());
        assert_eq!(route, *"cluster/audit-log");
    }
}

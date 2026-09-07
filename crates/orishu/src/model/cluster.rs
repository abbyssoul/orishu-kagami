use crate::{
    client::ClusterAddress,
    model::{manifest::Manifest as DefManifest, node::NodeId, workload::SimulationState},
};
use chrono::{DateTime, Utc};
pub use orishu_identity::CertFingerprint;
pub use orishu_identity::FormationId;
use serde::{Deserialize, Serialize};

use super::workload::Manifest as WorkloadManifest;

/// Version-1 synthetic formation projection, never an authored cluster manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Summary {
    /// Resource schema version, independent of peer ALPN.
    #[serde(deserialize_with = "summary_version")]
    pub schema_version: u32,
    /// Immutable identity of the locally observed formation.
    pub formation_id: orishu_identity::FormationId,
    /// Reusable human label; not identity.
    pub cluster_name: orishu_identity::ClusterName,
    /// Worker that produced this local projection.
    pub source_node_id: NodeId,
    /// Includes all retained member records, not only live members.
    pub member_count: usize,
    /// Number of locally observed live members.
    pub alive_count: usize,
    /// Locally accepted replicated lock; not a synchronous cluster fence.
    pub membership_locked: bool,
    /// Local participation state; never inferred from member count.
    pub participation: Participation,
    /// Whether the local worker can currently enforce every admission gate.
    pub introducer_ready: bool,
    /// Explicit no-workload state for the formation PoC.
    pub workload: FormationWorkload,
    /// Where this projection came from and what freshness it promises.
    pub view: SummaryView,
}

/// Bounded caller-chosen retry identity, scoped to one worker and formation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct OperationId(String);

impl std::str::FromStr for OperationId {
    type Err = &'static str;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty()
            || value.len() > 64
            || !value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err("operation ID must be 1–64 ASCII letters, digits, underscores or hyphens");
        }
        Ok(Self(value.to_owned()))
    }
}

impl TryFrom<String> for OperationId {
    type Error = &'static str;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<OperationId> for String {
    fn from(value: OperationId) -> Self {
        value.0
    }
}

/// Identified lock intent addressed directly to one worker in one formation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LockRequest {
    /// Resource schema version; the current decoder accepts only 1.
    #[serde(deserialize_with = "summary_version")]
    pub schema_version: u32,
    /// Caller-selected identity reused unchanged for retries.
    pub operation_id: OperationId,
    /// Required current formation of the directly addressed worker.
    pub formation_id: orishu_identity::FormationId,
    /// Desired locally accepted policy state.
    pub locked: bool,
}

/// Historical local acceptance receipt. Replaying it does not report current
/// policy or prove convergence; read Summary for the current entry-node view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LockReceipt {
    /// Resource schema version; the current decoder accepts only 1.
    #[serde(deserialize_with = "summary_version")]
    pub schema_version: u32,
    /// Identity of the request whose outcome this records.
    pub operation_id: OperationId,
    /// Formation in which the command was accepted.
    pub formation_id: orishu_identity::FormationId,
    /// Worker whose owner accepted the command, not a permanent coordinator.
    pub source_node_id: NodeId,
    /// State accepted for this operation, not necessarily current policy.
    pub locked: bool,
    /// None for an unchanged initial unlocked policy, otherwise its accepted version.
    pub policy_version: Option<orishu_identity::VersionTuple>,
}

/// Identified voluntary departure addressed to one worker, never a cluster relay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LeaveRequest {
    /// Current resource schema version is 1.
    #[serde(deserialize_with = "summary_version")]
    pub schema_version: u32,
    /// Reuse unchanged to recover a historical acceptance after a lost reply.
    pub operation_id: OperationId,
    /// Formation the directly addressed worker must still occupy.
    pub formation_id: FormationId,
}

/// Historical local departure result, not proof of remote departure visibility.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LeaveReceipt {
    /// Current resource schema version is 1.
    #[serde(deserialize_with = "summary_version")]
    pub schema_version: u32,
    /// Accepted request identity.
    pub operation_id: OperationId,
    /// Formation occupied before acceptance.
    pub previous_formation_id: FormationId,
    /// Local node identity before acceptance.
    pub previous_node_id: NodeId,
    /// False for an already standalone formation of one.
    pub changed: bool,
    /// Local state at acceptance, not a fresh view when the receipt is replayed.
    pub current: Summary,
}

/// The worker's participation lifecycle, separate from membership liveness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Participation {
    /// Fresh formation, eligible for an explicit join.
    Standalone,
    /// An explicit join is in progress.
    Joining,
    /// Admission may have happened remotely, but local adoption was not proven.
    /// No automatic retry/new join is permitted until explicit recovery.
    JoinUnresolved,
    /// Admitted but not ready to introduce other workers.
    CatchingUp,
    /// Joined, even if every other member is currently dead.
    Joined,
    /// Removed from the formation; no further participation is allowed.
    Ejected,
    /// Draining and closing the process.
    Stopping,
}

/// Only the formation workload state currently supported by the PoC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FormationWorkload {
    None,
}

/// A fresh read of the entry worker's local model, not global convergence proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SummaryView {
    LocalAtRequest,
}

fn summary_version<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<u32, D::Error> {
    let version = u32::deserialize(deserializer)?;
    if version != 1 {
        return Err(serde::de::Error::custom(
            "unsupported formation summary version",
        ));
    }
    Ok(version)
}

/// High-level summary of the cluster, returned by `GET /cluster`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClusterSpec {
    pub membership_locked: bool,
    pub workload: Option<WorkloadManifest>,
}

/// Cluster state version
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub epoch: u64,
    pub counter: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClusterStatus {
    pub version: Version,
    pub node_count: u32,
    pub simulation_status: Option<SimulationState>,
}

/// Cluster manifest kind.
pub const CLUSTER_MANIFEST_KIND: &str = "Cluster";

/// A synthetic runtime projection of current formation state.
///
/// It shares the resource envelope with every other Orishu resource, and that
/// is all it shares. It is not a durable operator-authored `cluster.yaml`: it
/// has no create, update, or delete lifecycle, no parsing entry point, and no
/// authored form a client could submit. `metadata.name` is a reusable human
/// label; formation identity is [`orishu_identity::FormationId`] under ADR
/// 0013, never this resource's metadata.
pub type Manifest = DefManifest<ClusterSpec, ClusterStatus>;

/// Project current cluster state as a resource.
///
/// Deliberately the only constructor: there is no `parse` counterpart, so a
/// cluster resource can be rendered for a client but never read back in as
/// configuration.
pub fn manifest(name: &str, spec: ClusterSpec) -> Manifest {
    crate::model::manifest::resource(CLUSTER_MANIFEST_KIND, name, spec)
}

/// A structured cluster event: node joins/leaves, workload lifecycle transitions,
/// administrative actions. See `AuditEvent` for privileged-operation records.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterEvent {
    pub timestamp: DateTime<Utc>,
    pub event_type: String,
    /// Node ID, workload ID, or other affected resource identifier.
    pub target_id: String,
    pub details: String,
}

crate::query_filter! {

/// Criteria for filtering `GET /cluster/events` results.
#[derive(Debug, Clone, Default)]
pub struct EventFilter {
    /// Inclusive lower bound.
    pub after as "after": Option<DateTime<Utc>>,
    /// Inclusive upper bound.
    pub before as "before": Option<DateTime<Utc>>,
    /// Restrict to these event type names. Empty means all.
    pub event_types as "type": Vec<String>,
}
}
/// A single log line from a cluster node.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,
    pub component: String,
    pub node_id: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Trace => f.write_str("trace"),
            Self::Debug => f.write_str("debug"),
            Self::Info => f.write_str("info"),
            Self::Warn => f.write_str("warn"),
            Self::Error => f.write_str("error"),
        }
    }
}

crate::query_filter! {
/// Criteria for filtering `GET /cluster/logs` results.
#[derive(Debug, Clone, Default)]
pub struct LogFilter {
    /// Inclusive lower bound.
    pub after  as "after": Option<DateTime<Utc>>,
    /// Inclusive upper bound.
    pub before as "before": Option<DateTime<Utc>>,
    pub level as "level": Option<LogLevel>,
    pub component as "component": Option<String>,
}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkloadCompatibilityReport {
    /// Number of nodes currently in the online
    /// that match workload requirements
    pub nodes_compatible: u32,
}

/// Compatibility read projection returned by `MembershipApi::is_lock`.
/// Mutations require `LockRequest`, never this unconditioned boolean.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockIntent {
    /// Current locally observed lock state.
    pub locked: bool,
}

crate::query_filter! {
/// Filter for members, when listing cluster participants.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MembersSelector {
    /// Filter by membership state: `Alive`, `Suspected`, `Dead`, `Removed`.
    pub member_state as "memberState": Option<String>,
    /// Filter by node name.
    pub name as "name": Option<String>,
    /// Filter by capability role: `introducer` or `worker`.
    pub role as "role": Option<String>,
}
}

/// A newly issued join token. The token value is shown exactly once.
/// Imported rotation DTO; the formation PoC retrieves `JoinMaterial` instead.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinToken {
    pub token: String,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Admission secret for explicit encrypted transfer; never ordinary status.
#[derive(Clone, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct AdmissionToken(String);

impl AdmissionToken {
    /// Explicit secret access for the encrypted wire or operator export path.
    pub fn expose(&self) -> &str {
        &self.0
    }
}
impl std::fmt::Debug for AdmissionToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AdmissionToken([REDACTED])")
    }
}
impl TryFrom<String> for AdmissionToken {
    type Error = &'static str;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.len() != 64
            || !value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("invalid admission token format");
        }
        Ok(Self(value))
    }
}
impl From<AdmissionToken> for String {
    fn from(token: AdmissionToken) -> Self {
        token.0
    }
}

#[cfg(test)]
mod admission_token_tests {
    use super::AdmissionToken;
    #[test]
    fn admission_secret_is_bounded_and_redacted_except_explicit_export() {
        let value = "a".repeat(64);
        let token = AdmissionToken::try_from(value.clone()).unwrap();
        assert!(!format!("{token:?}").contains(&value));
        assert_eq!(
            serde_json::to_string(&token).unwrap(),
            format!("\"{value}\"")
        );
        for invalid in [
            "a".repeat(63),
            "a".repeat(65),
            "A".repeat(64),
            "g".repeat(64),
        ] {
            assert!(AdmissionToken::try_from(invalid).is_err());
        }
    }
}

/// Privileged bootstrap material bound to an introducer and formation.
/// Endpoint hints never override the certificate pin or formation guard.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JoinMaterial {
    /// Resource version, currently 1.
    #[serde(deserialize_with = "summary_version")]
    pub schema_version: u32,
    /// Target formation; never inferred from a cluster label.
    pub formation_id: FormationId,
    /// Introducer's assigned identity at retrieval.
    pub introducer_node_id: NodeId,
    /// Required mTLS server pin before disclosing the token.
    pub introducer_fingerprint: orishu_identity::CertFingerprint,
    /// Public peer address candidates, not trust authorities.
    pub peer_endpoints: Vec<String>,
    /// Whether introduction is ready now; retrieval does not grant admission.
    pub introducer_ready: bool,
    /// Secret usable for admission only, never client API authorization.
    pub token: AdmissionToken,
}

impl JoinMaterial {
    /// Validate the bounded literal-address PoC profile before any network IO.
    /// Readiness is advisory; the introducer must re-check admission itself.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != 1
            || self.peer_endpoints.is_empty()
            || self.peer_endpoints.len() > 8
        {
            return Err("unsupported or oversized join material");
        }
        for endpoint in &self.peer_endpoints {
            let address: std::net::SocketAddr = endpoint
                .parse()
                .map_err(|_| "join material requires literal peer socket addresses")?;
            if endpoint.len() > 128
                || address.port() == 0
                || address.ip().is_unspecified()
                || address.ip().is_multicast()
            {
                return Err("join material contains an unusable peer endpoint");
            }
        }
        Ok(())
    }
}

/// Identified intent to move the directly addressed standalone worker. This
/// envelope does not authorize automatic leave or imply command acceptance.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JoinRequest {
    /// Request version, currently 1.
    #[serde(deserialize_with = "summary_version")]
    pub schema_version: u32,
    /// Stable caller-selected retry identity.
    pub operation_id: OperationId,
    /// Expected current source formation, not the target in join material.
    pub formation_id: FormationId,
    /// Privileged target binding and admission secret.
    pub material: JoinMaterial,
}

/// Current state of an identified local join operation, never an admission ACK.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JoinOperation {
    /// Resource version, currently 2.
    #[serde(deserialize_with = "join_operation_version")]
    pub schema_version: u32,
    /// Caller-selected request identity.
    pub operation_id: OperationId,
    /// Formation/identity of the directly addressed worker when it accepted work.
    pub source_formation_id: FormationId,
    /// Source identity may differ from the newly assigned target identity.
    pub source_node_id: NodeId,
    /// Target identity pinned by the accepted request.
    pub target_formation_id: FormationId,
    /// Secret-free correlation, retained after transient join IO is discarded.
    /// Null means no reference was bound, not proof of remote non-admission.
    #[serde(deserialize_with = "required_recovery_reference")]
    pub recovery_reference: Option<JoinRecoveryReference>,
    /// Processing/adoption state; successful transport is not this decision.
    pub state: JoinOperationState,
}

/// Correlates one peer admission attempt with its original authenticated issuer.
/// This is neither an admission credential nor authorization to retry or recover.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JoinRecoveryReference {
    /// Public attempt nonce; never the formation-assigned member identity.
    pub attempt_id: NodeId,
    /// Applicant identity taken from the worker's own certificate.
    pub applicant_fingerprint: CertFingerprint,
    /// Original introducer identity from the pinned join material.
    pub introducer_node_id: NodeId,
    /// Original introducer certificate pin, not a mutable endpoint hint.
    pub introducer_fingerprint: CertFingerprint,
}

fn join_operation_version<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    let version = u32::deserialize(d)?;
    if version != 2 {
        return Err(serde::de::Error::custom(
            "unsupported join operation version",
        ));
    }
    Ok(version)
}

/// Authenticated read-only query to the original admission issuer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdmissionInspectionRequest {
    /// Inspection schema version, currently 1.
    #[serde(deserialize_with = "summary_version")]
    pub schema_version: u32,
    /// Original target formation, not the source's standalone formation.
    pub formation_id: FormationId,
    /// Immutable correlation retained by the source operation.
    pub reference: JoinRecoveryReference,
}

/// One issuer's local-at-request evidence; never permission for new admission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdmissionInspection {
    /// Inspection schema version, currently 1.
    #[serde(deserialize_with = "summary_version")]
    pub schema_version: u32,
    /// Echoed query for response correlation.
    pub request: AdmissionInspectionRequest,
    /// Actual formation of the responding process.
    pub source_formation_id: FormationId,
    /// Actual assigned identity of the responding process.
    pub source_node_id: NodeId,
    /// Bounded local evidence, not a global exactly-once assertion.
    pub outcome: AdmissionInspectionOutcome,
}

/// Retained issuer evidence cannot prove non-insertion when history is absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum AdmissionInspectionOutcome {
    /// The addressed formation/node/certificate is not the original issuer.
    WrongIssuer,
    /// History is absent; acceptance remains unknown.
    RecordUnavailable,
    /// Assignment passes current local member checks, not all replay gates.
    CurrentMember { node_id: NodeId },
    /// Assignment is absent, dead, identity-mismatched or locally excluded.
    RetiredOrRestricted { node_id: NodeId },
}

// A missing field must not silently become evidence that no attempt was bound.
fn required_recovery_reference<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<JoinRecoveryReference>, D::Error> {
    Option::deserialize(d)
}

/// Explicit operation stages preserve the boundary between pre-admission failure,
/// uncertain remote insertion, validated adoption and completed catch-up.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "phase",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum JoinOperationState {
    Connecting,
    Admitting,
    CatchingUp { node_id: NodeId },
    Joined { node_id: NodeId },
    FailedBeforeAdmission,
    Unresolved,
    CatchUpFailed { node_id: NodeId },
}

/// An intent for the recipient of this message to join a given cluster.
/// Imported legacy DTO; not the identified formation join request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinIntent {
    pub addresses: Vec<ClusterAddress>,
    pub token: JoinToken,
}

/// Result of a node voluntarily leaving its cluster.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LeaveResult {
    /// Cluster name the node was previously a member of, if any.
    pub previous_cluster: Option<String>,
    /// Node ID the node held in the previous cluster, if any.
    pub previous_node_id: Option<String>,
}

/// Cluster response when a new node is accepted
/// It provides node with a new NodeId assigned by the cluster,
/// ID of the node that introduced request into the cluster and basic cluster info as [Manifest].
/// Note that Node-ID header in this case is the ID of the node that has joined the cluster, not the introducer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinRequestAccepted {
    /// ID assigned to the joining node by the cluster.
    pub node_id: NodeId,
    /// ID of the cluster member that admitted the node.
    pub admitted_by: NodeId,
    /// Cluster manifest, providing the joining node with basic cluster information.
    pub cluster: Manifest,
}

#[cfg(test)]
mod formation_tests {
    use super::*;

    #[test]
    fn lock_request_contract_requires_identity_version_and_bounded_operation_id() {
        let golden = serde_json::json!({"schemaVersion":1,"operationId":"lock-001","formationId":"formation-a","locked":true});
        let request: LockRequest = serde_json::from_value(golden.clone()).unwrap();
        assert_eq!(serde_json::to_value(request).unwrap(), golden);
        for field in ["schemaVersion", "operationId", "formationId", "locked"] {
            let mut invalid = golden.clone();
            invalid.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<LockRequest>(invalid).is_err());
        }
        for value in [
            "".to_owned(),
            "x".repeat(65),
            "bad/id".to_owned(),
            "with space".to_owned(),
        ] {
            assert!(value.parse::<OperationId>().is_err());
        }
        let mut invalid = golden.clone();
        invalid["schemaVersion"] = serde_json::json!(2);
        assert!(serde_json::from_value::<LockRequest>(invalid).is_err());
        let mut invalid = golden;
        invalid["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<LockRequest>(invalid).is_err());
    }

    #[test]
    fn summary_contract_preserves_identity_and_rejects_unknown_versions() {
        let golden = serde_json::json!({
            "schemaVersion": 1, "formationId": "formation-a", "clusterName": "shared label",
            "sourceNodeId": "node-a", "memberCount": 1, "aliveCount": 1,
            "membershipLocked": false, "participation": "joined", "introducerReady": false,
            "workload": "none", "view": "localAtRequest"
        });
        let summary: Summary = serde_json::from_value(golden.clone()).unwrap();
        assert_eq!(summary.participation, Participation::Joined);
        assert_eq!(serde_json::to_value(&summary).unwrap(), golden);
        let mut invalid = golden.clone();
        invalid["schemaVersion"] = serde_json::json!(2);
        assert!(serde_json::from_value::<Summary>(invalid).is_err());
        let mut invalid = golden.clone();
        invalid["formationId"] = serde_json::json!("not an identity");
        assert!(serde_json::from_value::<Summary>(invalid).is_err());
        let mut invalid = golden;
        invalid["extra"] = serde_json::json!(true);
        assert!(serde_json::from_value::<Summary>(invalid).is_err());
    }
}

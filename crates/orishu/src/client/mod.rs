pub mod address;
// TODO: Should be a feature disabled by default
pub mod http_client;

mod bearer_auth;
mod cbor_middleware;
mod netrc_auth;

pub use address::{ClusterAddress, ClusterAddressParseError, DEFAULT_PORT, default_socket_path};
use async_trait::async_trait;

use crate::model::{
    EntriesRemoved,
    audit::{AuditEvent, AuditFilter},
    blocklist::{self, BlocklistAddRequest, BlocklistAddResult},
    checkpoint::{self, CheckpointId},
    cluster::{
        self, ClusterEvent, EventFilter, JoinToken, LockIntent, LogFilter, LogLine,
        MembersSelector, WorkloadCompatibilityReport,
    },
    node::{self, DiagnosticResult, InspectSource, NodeId, RemoveMode},
    result::{self, ResultId},
    tombstones::{TombstoneFilter, TombstoneRecord},
    workload::{self, SimulationStateTransitionMode, SimulationStream},
};

// ── Credentials ───────────────────────────────────────────────────────────────

/// Authentication credentials presented to the node.
///
/// - [`Credentials::Token`] carries a signed operator token in the HTTP `Authorization` header.
/// - [`Credentials::Mtls`] uses a client certificate for mutual TLS — planned, not yet implemented.
#[derive(Debug, Clone)]
pub enum Credentials {
    /// Signed operator token. Grants Tier 2 privileged access.
    Token(String),
    /// mTLS client certificate (DER-encoded key pair). Planned; not yet implemented.
    Mtls { cert_der: Vec<u8>, key_der: Vec<u8> },
}

// ── ClientError ───────────────────────────────────────────────────────────────

/// Errors returned by Orishu client operations.
#[derive(Debug)]
pub enum ClientError {
    /// Failed to serialized request data
    DataSerialization(String),
    /// Could not establish a connection to the address.
    ConnectionFailed(String),
    /// A transport-level error (I/O, TLS, DNS resolution failure, etc.).
    TransportError(String),
    /// The operation requires authentication but no credentials were provided.
    AuthenticationRequired,
    /// Credentials were provided but the server rejected them.
    AuthorizationDenied,
    /// The referenced resource does not exist in the cluster.
    ResourceNotFound(String),
    /// Unexpected response: what server returned is not the kind of response client expected
    /// Example: for a request expecting no data, a response object, which is not of ApiResponse::Error variant, was returned by the server.
    /// If this error is encountered it hints to one of two options:
    ///  - Either a server is non-compliant - it replies with the wrong message type and does not implement the protocol spec.
    ///  - Or: the client implementation is incorrect.
    ///
    /// To resolved which case is it, first check with the latest spec version, what type of reply is expected for the message passed.
    ///    Then check what client version is implemented by the server and client.
    ///    It is an implementation bug, only if client and server implement same protocol version.
    UnexpectedResponseType(String),

    /// The server returned a structured error response.
    ApiError { status: u16, message: String },
    /// The operation is not valid in the current cluster state
    /// (e.g. starting a workload when none is loaded).
    InvalidState(String),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DataSerialization(msg) => write!(f, "serialization error: {msg}"),
            Self::ConnectionFailed(msg) => write!(f, "connection failed: {msg}"),
            Self::AuthenticationRequired => write!(f, "authentication required"),
            Self::AuthorizationDenied => write!(f, "authorization denied"),
            Self::ResourceNotFound(id) => write!(f, "resource not found: {id}"),
            Self::TransportError(msg) => write!(f, "transport error: {msg}"),
            Self::UnexpectedResponseType(msg) => write!(f, "unexpected response type error: {msg}"),
            Self::ApiError { status, message } => write!(f, "API error {status}: {message}"),
            Self::InvalidState(msg) => write!(f, "invalid cluster state: {msg}"),
        }
    }
}

impl std::error::Error for ClientError {}

/// CRUD APIs for the cluster as a whole.
/// Only R(retrieve) is available.
/// Cluster is a logical construct. Creating or Deleting one can not be performed via API.
#[async_trait]
pub trait ClusterApi {
    /// Get cluster info. Return information about cluster in aggregate.
    /// For the list of individual member, see [`MembershipApi`]
    async fn summary(&self) -> Result<cluster::Summary, ClientError>;

    /// GET a list of events happening in the cluster.
    async fn events(&self, filter: &EventFilter) -> Result<Vec<ClusterEvent>, ClientError>;

    /// GET a logs.
    async fn logs(&self, filter: &LogFilter) -> Result<Vec<LogLine>, ClientError>;

    /// GET cluster auditable events: A list of admin actions.
    async fn audit_log(&self, filter: &AuditFilter) -> Result<Vec<AuditEvent>, ClientError>;
}

/// CRUD APIs managing cluster-wide blocklist resource.
#[async_trait]
pub trait BlocklistApi {
    async fn add(&self, req: &BlocklistAddRequest) -> Result<BlocklistAddResult, ClientError>;
    async fn list(&self, filter: &blocklist::Filter) -> Result<Vec<blocklist::Entry>, ClientError>;
    async fn remove(&self, entry_id: &str) -> Result<(), ClientError>;
}

/// CRUD APIs managing the cluster graveyard (retained removals).
#[async_trait]
pub trait GraveyardApi {
    /// List retained tombstones for nodes that were removed from the cluster.
    async fn list(&self, filter: &TombstoneFilter) -> Result<Vec<TombstoneRecord>, ClientError>;

    /// Clear a tombstone, allowing a previously removed node to attempt re-joining.
    /// This removes the exclusion barrier but does not automatically re-admit the node.
    async fn clear(&self, id: &NodeId) -> Result<(), ClientError>;
}

// ── Results resource ──────────────────────────────────────────────────────
#[async_trait]
pub trait ResultsApi {
    /// List results, optionally filtered.
    async fn list(&self, filter: &result::Filter) -> Result<Vec<result::Record>, ClientError>;
    /// Get metadata for a specific result.
    async fn get(&self, id: &ResultId) -> Result<result::Record, ClientError>;
    /// Download result data, streaming it into the provided writer.
    async fn download(
        &self,
        id: &ResultId,
        writer: &mut (dyn tokio::io::AsyncWrite + Unpin + Send),
    ) -> Result<(), ClientError>;
    /// Delete given result data by ID
    async fn delete(&self, id: &ResultId) -> Result<(), ClientError>;
    /// Bulk-remove all results matching filter
    async fn purge(&self, filter: &result::Filter) -> Result<EntriesRemoved, ClientError>;
}

// ── Checkpoint resource ───────────────────────────────────────────────────
#[async_trait]
pub trait CheckpointApi {
    /// List checkpoints, optionally filtered.
    async fn list(
        &self,
        filter: &checkpoint::Filter,
    ) -> Result<Vec<checkpoint::Record>, ClientError>;
    /// Get metadata for a specific checkpoint.
    async fn get(&self, id: &CheckpointId) -> Result<checkpoint::Record, ClientError>;
    /// Download checkpoint data, streaming it into the provided writer.
    async fn download(
        &self,
        id: &CheckpointId,
        writer: &mut (dyn tokio::io::AsyncWrite + Unpin + Send),
    ) -> Result<(), ClientError>;
    /// Delete given checkpoint data by ID
    async fn delete(&self, id: &CheckpointId) -> Result<(), ClientError>;
    /// Bulk-remove all checkpoints matching filter
    async fn purge(&self, filter: &checkpoint::Filter) -> Result<EntriesRemoved, ClientError>;
}

// ── Workload resource ─────────────────────────────────────────────────────
#[async_trait]
pub trait WorkloadApi {
    async fn load(&self, manifest: &workload::Manifest) -> Result<workload::Accepted, ClientError>;
    async fn get(&self) -> Result<Option<workload::Manifest>, ClientError>;
    async fn unload(&self, mode: &SimulationStateTransitionMode) -> Result<(), ClientError>;

    /// Check compatibility of the given workload with the cluster
    async fn check(
        &self,
        manifest: &workload::Manifest,
    ) -> Result<WorkloadCompatibilityReport, ClientError>;

    /// Workload playback control: Start simulation
    /// Errors, if no workload is loaded. No-op if a simulation is already running.
    async fn start(
        &self,
        mode: &SimulationStateTransitionMode,
        checkpoint: Option<u64>,
    ) -> Result<(), ClientError>;
    /// Workload playback control: Reset the state to a checkpoint
    /// Errors, if no workload is loaded or if simulation is already running.
    async fn reset(&self, checkpoint: CheckpointId) -> Result<(), ClientError>;
    /// Workload playback control: Advance simulation by a number of steps
    /// Errors, if no workload is loaded or if simulation is already running.
    async fn step(&self, steps: u128, checkpoint: Option<u64>) -> Result<(), ClientError>;
    /// Workload playback control: Stop a running simulation.
    /// Errors, if no workload is loaded. No-op if a simulation is already stopped.
    async fn stop(&self, mode: &SimulationStateTransitionMode) -> Result<(), ClientError>;

    /// Stream the workload simulation results as they being computed
    async fn stream(&self) -> Result<SimulationStream, ClientError>;
}

/// CRUD APIs to mange cluster membership.
#[async_trait]
pub trait MembershipApi {
    /// Set the formation lock with an explicit retry identity and precondition.
    /// The receipt is local acceptance, not a synchronous cluster-wide fence.
    async fn set_lock(
        &self,
        request: &cluster::LockRequest,
    ) -> Result<cluster::LockReceipt, ClientError>;

    /// Get membership lock state.
    async fn is_lock(&self) -> Result<LockIntent, ClientError>;

    /// Submit an identified join to the directly addressed worker. The returned
    /// operation is processing state, not a successful admission assertion.
    async fn join(&self, req: &cluster::JoinRequest)
    -> Result<cluster::JoinOperation, ClientError>;
    /// Poll retained local operation state using the same worker and credential.
    async fn join_status(
        &self,
        id: &cluster::OperationId,
    ) -> Result<cluster::JoinOperation, ClientError>;

    /// Query the original issuer's retained evidence; never authorizes admission.
    async fn inspect_admission(
        &self,
        request: &cluster::AdmissionInspectionRequest,
    ) -> Result<cluster::AdmissionInspection, ClientError>;

    /// Command the local worker to leave its current cluster and return to standalone.
    async fn leave(
        &self,
        request: &cluster::LeaveRequest,
    ) -> Result<cluster::LeaveReceipt, ClientError>;

    /// Collect bounded local-view pages, not a globally consistent snapshot.
    async fn list(&self, filter: &MembersSelector) -> Result<Vec<node::Inspection>, ClientError>;

    /// Inspect a formation-assigned identity. The PoC supports indirect reads;
    /// unsupported source modes return an explicit error.
    async fn get(
        &self,
        id: &NodeId,
        source: Option<InspectSource>,
    ) -> Result<node::Inspection, ClientError>;

    /// Kick-out a node from the cluster and create a tombstone
    async fn remove(&self, id: &NodeId, mode: &RemoveMode) -> Result<(), ClientError>;
}

/// Cluster interface for clients implementations.
#[async_trait]
pub trait ClientApi {
    /// Get cluster-level API
    fn cluster(&self) -> impl ClusterApi;
    /// Get blocklist APIs
    fn blocklist(&self) -> impl BlocklistApi;
    /// Get graveyard (tombstones) APIs
    fn graveyard(&self) -> impl GraveyardApi;
    /// Get results APIs
    fn results(&self) -> impl ResultsApi;
    /// Get workload management API
    fn workload(&self) -> impl WorkloadApi;
    /// Get checkpoint APIs
    fn checkpoints(&self) -> impl CheckpointApi;
    /// Get membership API
    fn membership(&self) -> impl MembershipApi;

    // ── Cluster resource ──────────────────────────────────────────────────────

    /// Retrieve privileged, formation/certificate-bound bootstrap material.
    async fn get_join_token(&self) -> Result<cluster::JoinMaterial, ClientError>;
    async fn create_join_token(&self) -> Result<JoinToken, ClientError>;

    // ── Node resource ─────────────────────────────────────────────────────────

    async fn diagnose_node(
        &self,
        id: NodeId,
        source_id: Option<NodeId>,
    ) -> Result<DiagnosticResult, ClientError>;
}

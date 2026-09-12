#![allow(unused_variables, unused_imports)]
//! HTTP implementation of [`ClusterApi`].
//!
//! Networking is implemented using reqwest crate.

use std::collections::HashMap;
use std::io::Cursor;
use std::io::Read;
use std::net::ToSocketAddrs;
use std::{fs::File, path::PathBuf, time::Duration};

use anyhow::{Context, Result};
use async_trait::async_trait;
use futures_util::StreamExt;
use http::StatusCode;
use tokio::io::AsyncWriteExt;
// #[cfg(feature = "rustls")]
use reqwest::{Certificate, Client, ClientBuilder};
use reqwest::{Identity, Url};
use reqwest_middleware::{ClientBuilder as MiddlewareClientBuilder, ClientWithMiddleware};

use super::ClientError;
use crate::client::cbor_middleware::CborContentMiddleware;
use crate::model::blocklist::{self, BlocklistAddRequest, BlocklistAddResult};
use crate::model::checkpoint::{self, CheckpointId, Filter, Record};
use crate::model::cluster::{ClusterSpec, JoinIntent, LockIntent, MembersSelector};
use crate::model::node::InspectSource;
use crate::model::workload::Accepted;
use crate::model::{
    API_ROUTE_CLUSTER, API_ROUTE_CLUSTER_AUDIT, API_ROUTE_CLUSTER_BLOCKLIST,
    API_ROUTE_CLUSTER_EVENTS, API_ROUTE_CLUSTER_LOCK, API_ROUTE_CLUSTER_LOGS,
    API_ROUTE_CLUSTER_NODES, API_ROUTE_CLUSTER_RESULTS, API_ROUTE_CLUSTER_TOKEN,
    API_ROUTE_CLUSTER_TOMBSTONES, API_ROUTE_CLUSTER_WORKLOAD, API_ROUTE_CLUSTER_WORKLOAD_CHECK,
    API_ROUTE_CLUSTER_WORKLOAD_CHECKPOINT, ApiRoute, EntriesRemoved, QuerySet, ResponseCollection,
    ResponseData, cluster, node, result, tombstones,
};
use crate::{
    client::bearer_auth::BearerMiddleware,
    client::netrc_auth::NetrcMiddleware,
    client::{
        BlocklistApi, CheckpointApi, ClusterAddress, ClusterApi, Credentials, GraveyardApi,
        MembershipApi, ResultsApi, WorkloadApi,
    },
    model::{
        ApiResponse,
        audit::{AuditEvent, AuditEventType, AuditFilter, AuditOutcome},
        cluster::WorkloadCompatibilityReport,
        cluster::{ClusterEvent, EventFilter, JoinToken, LogFilter, LogLevel, LogLine},
        node::{
            ConnectionStats, DiagnosticResult, MemberState, NodeAccepts, NodeCapabilities, NodeId,
            NodeLimits, NodeSpec, NodeStatus, NodeStorage, RemoveMode,
        },
        result::ResultId,
        workload::{
            self, DomainDiscretization, DomainSpec, ExternalResource, ModelSpec, SimulationState,
            SimulationStateIntent, SimulationStateTransitionMode, SimulationStream,
            SpaceDiscretization, TimeDiscretization, WorkloadSpec, WorkloadStatus,
        },
    },
};

// ── ClusterClient ─────────────────────────────────────────────────────────────

/// Client for the orishu node Client API. Shared by `orishu-ctl` and `orishu-monitor`.
///
/// Connections are established lazily on the first call. The client is cheap to clone;
/// the underlying transport will be shared.
///
/// # Access tiers
///
/// - **Tier 1** — [`ClusterAddress::UnixSocket`] with no credentials:
///   read-only, no authentication required (same OS user as the node).
/// - **Tier 2** — any address with [`Credentials::Token`] or [`Credentials::Mtls`]:
///   full read/write access.
///
/// # Example
///
/// ```rust,no_run
/// use orishu::client::{ClusterAddress, Credentials};
/// use orishu::client::http_client::{HttClientOptions, HttpClusterClient};
///
/// // Connect to the local node (Tier 1, no auth)
/// let client = HttpClusterClient::new(ClusterAddress::default(), HttClientOptions::default()).unwrap();
///
/// // Connect to a remote cluster using a token (Tier 2)
/// let client = HttpClusterClient::new(
///     "cluster.orishu.local".parse().unwrap(),
///     HttClientOptions{
///         credentials: Some(Credentials::Token("my-operator-token".into())),
///         ..Default::default()
///     },
/// ).unwrap();
/// ```
pub struct HttpClusterClient {
    // address: ClusterAddress,
    // credentials: Credentials,
    // Transport handle will live here once implemented (e.g. reqwest::Client).
    client: ClientWithMiddleware,
    base_url: reqwest::Url,
}

#[derive(Debug, Clone, Default)]
pub struct HttClientOptions {
    /// Credentials to be used for remote server communication.
    /// No credentials is only valid for local read-only operations (Unix socket, same OS user).
    pub credentials: Option<Credentials>,

    /// Optional connection timeout setting for HTTP client.
    pub timeout: Option<Duration>,

    /// Path to TLS certificate PEM file (enables TLS on TCP listeners).
    pub tls_cert: Option<PathBuf>,

    /// Path to TLS private key PEM file.
    pub tls_key: Option<PathBuf>,
}

impl HttClientOptions {
    fn certificate(&self) -> Result<Option<Certificate>> {
        if let Some(path) = &self.tls_cert {
            let mut buf = Vec::new();
            File::open(path)?.read_to_end(&mut buf)?;
            Ok(Some(reqwest::Certificate::from_pem(&buf)?))
        } else {
            Ok(None)
        }
    }

    // #[cfg(feature = "native-tls")]
    // fn identity(&self) -> Result<Option<Identity>> {
    //     self.tls_key.as_ref().map_or(Ok(None), |path| {
    //         let mut buf = Vec::new();
    //         File::open(path)?.read_to_end(&mut buf)?;
    //         Ok(Some(Identity::from_pkcs12_der(&buf, "123456")?))
    //     })
    // }

    #[cfg(feature = "rustls")]
    fn identity(&self) -> Result<Option<Identity>> {
        self.tls_key.as_ref().map_or(Ok(None), |path| {
            let mut buf = Vec::new();
            File::open(path)?.read_to_end(&mut buf)?;
            Ok(Some(Identity::from_pem(&buf)?))
        })
    }
}

fn build_client(options: HttClientOptions) -> Result<ClientWithMiddleware> {
    // Operator mutations target a particular worker. A redirect must not silently
    // change that authority (or carry a credential to an unintended endpoint).
    let mut cb = Client::builder().redirect(reqwest::redirect::Policy::none());
    if let Some(timeout) = options.timeout {
        cb = cb.connect_timeout(timeout).timeout(timeout);
    }

    if let Some(cert) = options.certificate()? {
        cb = cb.add_root_certificate(cert);
    }

    if let Some(identity) = options.identity()? {
        cb = cb.identity(identity);
    }

    let mut middleware_cb =
        MiddlewareClientBuilder::new(cb.build()?).with(CborContentMiddleware::default());

    if let Some(auth) = options.credentials
        && let Credentials::Token(token) = auth
    {
        middleware_cb = middleware_cb.with(BearerMiddleware::with_token(token.as_str()));
    }

    // TODO: Should it be conditional? Incompatible with unix_socket! Consider re-write
    if let Ok(netrc) = NetrcMiddleware::new() {
        middleware_cb = middleware_cb.with_init(netrc);
    }

    Ok(middleware_cb.build())
}

/// Build the transport for a [`ClusterAddress::UnixSocket`] address.
///
/// Unix domain sockets carry the Tier 1 (same OS user, unauthenticated) access path,
/// so the URL authority is a placeholder: the socket path selects the peer, not DNS.
#[cfg(unix)]
fn build_unix_socket_client(
    path: PathBuf,
    options: HttClientOptions,
) -> Result<(ClientWithMiddleware, Url)> {
    // Create a client configured to use the Unix socket
    let mut builder = ClientBuilder::new()
        .unix_socket(path)
        .redirect(reqwest::redirect::Policy::none());
    if let Some(timeout) = options.timeout {
        builder = builder.timeout(timeout);
    }
    let client = builder.build()?;
    let mut builder = MiddlewareClientBuilder::new(client).with(CborContentMiddleware::default());

    if let Some(auth) = options.credentials
        && let Credentials::Token(token) = auth
    {
        builder = builder.with(BearerMiddleware::with_token(token.as_str()));
    }

    Ok((
        builder.build(),
        reqwest::Url::parse("http://localhost/api/v1/")?,
    ))
}

/// Reject a [`ClusterAddress::UnixSocket`] address on platforms without Unix
/// domain sockets (Windows), where the client plane is only reachable over TCP.
#[cfg(not(unix))]
fn build_unix_socket_client(
    path: PathBuf,
    _options: HttClientOptions,
) -> Result<(ClientWithMiddleware, Url)> {
    anyhow::bail!(
        "cannot connect to Unix socket {}: Unix domain sockets are not supported on this platform; \
         address the node over TCP as IP:port or hostname[:port] instead",
        path.display()
    )
}

impl HttpClusterClient {
    /// Create a client for the given address and credentials.
    ///
    /// Returns an error for a [`ClusterAddress::UnixSocket`] address on platforms
    /// without Unix domain socket support (Windows).
    pub fn new(address: ClusterAddress, options: HttClientOptions) -> Result<Self> {
        let (client, base_url) = match address {
            ClusterAddress::UnixSocket(path) => build_unix_socket_client(path, options)?,
            ClusterAddress::Ip(ref socket_addr) => (
                build_client(options)?,
                reqwest::Url::parse(format!("https://{}/api/v1/", socket_addr).as_str())?,
            ),
            // TODO: In case of a host name, spec calls for resolving the name first into a list of IPs
            // FIXME: It makes no sense to call 'join' in case of such logic.
            ClusterAddress::Host { host, port } => {
                let authority = format!("{}:{}", host, port);
                let ips = resolve_hostname_std(&authority)?;
                (
                    build_client(options)?,
                    reqwest::Url::parse(format!("https://{}/api/v1/", authority).as_str())?,
                )
            }
        };

        Ok(Self { client, base_url })
    }
}

struct ApiCaller {
    base_url: Url,
    client: ClientWithMiddleware,
}

impl ApiCaller {
    fn serialize<T: serde::Serialize>(&self, payload: &T) -> Result<Vec<u8>, ClientError> {
        let mut data = Vec::new();
        ciborium::into_writer(payload, &mut data)
            .map_err(|e| ClientError::DataSerialization(e.to_string()))?;

        Ok(data)
    }

    fn with_resource(&self, resource: &ApiRoute) -> Result<Url, ClientError> {
        self.base_url
            .join(&resource.to_string())
            .map_err(|e| ClientError::ConnectionFailed(e.to_string()))
    }

    async fn map_response(
        &self,
        res: reqwest_middleware::Result<reqwest::Response>,
    ) -> Result<ApiResponse, ClientError> {
        match res {
            Ok(response) => {
                let status = response.status();

                if status == StatusCode::NO_CONTENT {
                    return Ok(ApiResponse::Ok { data: None });
                }

                let bytes = response.bytes().await.map_err(|e| {
                    ClientError::TransportError(format!("failed to read response body: {}", e))
                })?;

                if bytes.is_empty() {
                    return Err(match status {
                        StatusCode::UNAUTHORIZED => ClientError::AuthenticationRequired,
                        StatusCode::FORBIDDEN => ClientError::AuthorizationDenied,
                        StatusCode::NOT_FOUND => {
                            ClientError::ResourceNotFound("not found".to_string())
                        }
                        _ => ClientError::TransportError(format!(
                            "empty response body with status {}",
                            status.as_str()
                        )),
                    });
                }

                let envelope: ApiResponse =
                    ciborium::from_reader(Cursor::new(bytes)).map_err(|e| {
                        ClientError::TransportError(format!(
                            "failed to deserialize response body: {}",
                            e
                        ))
                    })?;
                if !status.is_success() {
                    return match envelope {
                        ApiResponse::Error { message, .. } => Err(ClientError::ApiError {
                            status: status.as_u16(),
                            message,
                        }),
                        _ => Err(ClientError::TransportError(format!(
                            "non-success HTTP status {} carried a success envelope",
                            status.as_u16()
                        ))),
                    };
                }
                Ok(envelope)
            }
            Err(e) => {
                if e.is_connect() {
                    Err(ClientError::ConnectionFailed(e.to_string()))
                } else if e.is_timeout() || e.is_redirect() || e.is_decode() {
                    Err(ClientError::TransportError(e.to_string()))
                } else {
                    Err(ClientError::ApiError {
                        status: e.status().unwrap_or_default().as_u16(),
                        message: e.to_string(),
                    })
                }
            }
        }
    }

    pub async fn get(&self, resource: &ApiRoute) -> Result<ApiResponse, ClientError> {
        let res = self.client.get(self.with_resource(resource)?).send().await;

        self.map_response(res).await
    }

    pub async fn post<T: serde::Serialize>(
        &self,
        resource: &ApiRoute,
        payload: &T,
    ) -> Result<ApiResponse, ClientError> {
        let res = self
            .client
            .post(self.with_resource(resource)?)
            .body(self.serialize(&payload)?)
            .send()
            .await;

        self.map_response(res).await
    }

    pub async fn put<T: serde::Serialize>(
        &self,
        resource: &ApiRoute,
        payload: &T,
    ) -> Result<ApiResponse, ClientError> {
        let res = self
            .client
            .put(self.with_resource(resource)?)
            .body(self.serialize(&payload)?)
            .send()
            .await;

        self.map_response(res).await
    }

    pub async fn patch<T: serde::Serialize>(
        &self,
        resource: &ApiRoute,
        payload: &T,
    ) -> Result<ApiResponse, ClientError> {
        let res = self
            .client
            .patch(self.with_resource(resource)?)
            .body(self.serialize(&payload)?)
            .send()
            .await;

        self.map_response(res).await
    }

    pub async fn delete(&self, resource: &ApiRoute) -> Result<ApiResponse, ClientError> {
        let res = self
            .client
            .delete(self.with_resource(resource)?)
            .send()
            .await;

        self.map_response(res).await
    }

    /// Download a resource as a binary stream, writing chunks directly to `file`.
    ///
    /// Error handling mirrors [`Self::map_response`]: middleware errors are
    /// classified the same way, and non-success HTTP status codes are mapped to
    /// the appropriate [`ClientError`] variant (including deserialising CBOR
    /// error bodies when present).
    pub async fn download_to(
        &self,
        resource: &ApiRoute,
        writer: &mut (dyn tokio::io::AsyncWrite + Unpin + Send),
    ) -> Result<(), ClientError> {
        let res = self.client.get(self.with_resource(resource)?).send().await;

        let response = match res {
            Ok(r) => r,
            Err(e) => {
                return if e.is_connect() {
                    Err(ClientError::ConnectionFailed(e.to_string()))
                } else if e.is_timeout() || e.is_redirect() || e.is_decode() {
                    Err(ClientError::TransportError(e.to_string()))
                } else {
                    Err(ClientError::ApiError {
                        status: e.status().unwrap_or_default().as_u16(),
                        message: e.to_string(),
                    })
                };
            }
        };

        let status = response.status();

        if status.is_success() {
            let mut stream = response.bytes_stream();
            while let Some(chunk_result) = stream.next().await {
                let chunk = chunk_result.map_err(|e| {
                    ClientError::TransportError(format!("download stream error: {e}"))
                })?;
                writer.write_all(&chunk).await.map_err(|e| {
                    ClientError::TransportError(format!("failed to write download data: {e}"))
                })?;
            }

            writer.flush().await.map_err(|e| {
                ClientError::TransportError(format!("failed to flush download data: {e}"))
            })?;
            return Ok(());
        }

        // Non-success status — buffer the (small) error body and classify.
        let bytes = response.bytes().await.map_err(|e| {
            ClientError::TransportError(format!("failed to read error response: {e}"))
        })?;

        if bytes.is_empty() {
            return Err(match status {
                StatusCode::UNAUTHORIZED => ClientError::AuthenticationRequired,
                StatusCode::FORBIDDEN => ClientError::AuthorizationDenied,
                StatusCode::NOT_FOUND => ClientError::ResourceNotFound("not found".to_string()),
                _ => ClientError::TransportError(format!(
                    "empty response body with status {}",
                    status.as_str()
                )),
            });
        }

        match ciborium::from_reader::<ApiResponse, _>(Cursor::new(bytes)) {
            Ok(ApiResponse::Error { code, message }) => Err(ClientError::ApiError {
                status: status.as_u16(),
                message,
            }),
            _ => Err(ClientError::TransportError(format!(
                "download failed with status {}",
                status.as_str()
            ))),
        }
    }
}

fn expect_no_data(api_response: ApiResponse) -> Result<(), ClientError> {
    match api_response {
        // If API response body is an error, map it into Err()
        ApiResponse::Error { code, message } => Err(ClientError::ApiError { status: 0, message }),
        // Note: we do drop response data here, because well behaved server is not expected to respond here.
        ApiResponse::Ok { data } => {
            if let Some(data) = data {
                Err(ClientError::TransportError(
                    "expected no response, received incorrect response type".to_string(),
                ))
            } else {
                Ok(())
            }
        }
        ApiResponse::OkCollection { .. } => Err(ClientError::TransportError(
            "expected no response, received collection response".to_string(),
        )),
    }
}

fn expect_data<T: TryFrom<ResponseData, Error = String>>(
    api_response: ApiResponse,
) -> Result<T, ClientError> {
    match api_response {
        ApiResponse::Error { code, message } => Err(ClientError::ApiError { status: 0, message }),
        ApiResponse::Ok { data } => {
            if let Some(data) = data {
                T::try_from(data).map_err(ClientError::TransportError)
            } else {
                Err(ClientError::TransportError(
                    "expected response data, got none".to_string(),
                ))
            }
        }
        ApiResponse::OkCollection { .. } => Err(ClientError::TransportError(
            "expected single response, received collection response".to_string(),
        )),
    }
}

fn expect_collection<T: TryFrom<ResponseCollection, Error = String>>(
    api_response: ApiResponse,
) -> Result<Vec<T>, ClientError> {
    match api_response {
        ApiResponse::Error { code, message } => Err(ClientError::ApiError { status: 0, message }),
        ApiResponse::OkCollection { data, .. } => data
            .into_iter()
            .map(|item| T::try_from(item).map_err(ClientError::TransportError))
            .collect(),
        ApiResponse::Ok { .. } => Err(ClientError::TransportError(
            "expected collection response, received single response".to_string(),
        )),
    }
}

/// CRUD APIs managing cluster-wide blocklist resource.
#[async_trait]
impl BlocklistApi for ApiCaller {
    async fn add(&self, req: &BlocklistAddRequest) -> Result<BlocklistAddResult, ClientError> {
        let api_response = self.post(&API_ROUTE_CLUSTER_BLOCKLIST, req).await?;

        let result: BlocklistAddResult = expect_data(api_response)?;
        Ok(result)
    }

    async fn list(&self, filter: &blocklist::Filter) -> Result<Vec<blocklist::Entry>, ClientError> {
        let api_response = self
            .get(&API_ROUTE_CLUSTER_BLOCKLIST.with_query_param(filter))
            .await?;

        let result: Vec<blocklist::Entry> = expect_collection(api_response)?;
        Ok(result)
    }

    async fn remove(&self, entry_id: &str) -> Result<(), ClientError> {
        expect_no_data(
            self.delete(&API_ROUTE_CLUSTER_BLOCKLIST.with_id(entry_id))
                .await?,
        )
    }
}

#[async_trait]
impl GraveyardApi for ApiCaller {
    async fn list(
        &self,
        filter: &tombstones::TombstoneFilter,
    ) -> Result<Vec<tombstones::TombstoneRecord>, ClientError> {
        let api_response = self
            .get(&API_ROUTE_CLUSTER_TOMBSTONES.with_query_param(filter))
            .await?;

        let result: Vec<tombstones::TombstoneRecord> = expect_collection(api_response)?;
        Ok(result)
    }

    async fn clear(&self, id: &NodeId) -> Result<(), ClientError> {
        expect_no_data(
            self.delete(&API_ROUTE_CLUSTER_TOMBSTONES.with_id(id))
                .await?,
        )
    }
}

#[async_trait]
impl ClusterApi for ApiCaller {
    async fn summary(&self) -> Result<cluster::Summary, ClientError> {
        let api_response = self.get(&API_ROUTE_CLUSTER).await?;

        let result: cluster::Summary = expect_data(api_response)?;
        Ok(result)
    }

    async fn events(&self, filter: &EventFilter) -> Result<Vec<ClusterEvent>, ClientError> {
        let api_response = self
            .get(&API_ROUTE_CLUSTER_EVENTS.with_query_param(filter))
            .await?;

        let result: Vec<ClusterEvent> = expect_collection(api_response)?;
        Ok(result)
    }

    async fn logs(&self, filter: &LogFilter) -> Result<Vec<LogLine>, ClientError> {
        let api_response = self
            .get(&API_ROUTE_CLUSTER_LOGS.with_query_param(filter))
            .await?;

        let result: Vec<LogLine> = expect_collection(api_response)?;
        Ok(result)
    }

    async fn audit_log(&self, filter: &AuditFilter) -> Result<Vec<AuditEvent>, ClientError> {
        let api_response = self
            .get(&API_ROUTE_CLUSTER_AUDIT.with_query_param(filter))
            .await?;

        let result: Vec<AuditEvent> = expect_collection(api_response)?;
        Ok(result)
    }
}

#[async_trait]
impl WorkloadApi for ApiCaller {
    async fn load(&self, manifest: &workload::Manifest) -> Result<workload::Accepted, ClientError> {
        let api_response = self.put(&API_ROUTE_CLUSTER_WORKLOAD, &manifest).await?;
        let result: workload::Accepted = expect_data(api_response)?;
        Ok(result)
    }

    async fn get(&self) -> Result<Option<workload::Manifest>, ClientError> {
        let api_response = self.get(&API_ROUTE_CLUSTER_WORKLOAD).await?;
        let result: Option<workload::Manifest> = expect_data(api_response)?;
        Ok(result)
    }

    async fn unload(&self, mode: &SimulationStateTransitionMode) -> Result<(), ClientError> {
        let route = API_ROUTE_CLUSTER_WORKLOAD.with_query_param(mode);
        expect_no_data(self.delete(&route).await?)
    }

    async fn check(
        &self,
        manifest: &workload::Manifest,
    ) -> Result<WorkloadCompatibilityReport, ClientError> {
        let api_response = self
            .post(&API_ROUTE_CLUSTER_WORKLOAD_CHECK, &manifest)
            .await?;
        let result: WorkloadCompatibilityReport = expect_data(api_response)?;
        Ok(result)
    }

    async fn start(
        &self,
        mode: &SimulationStateTransitionMode,
        checkpoint: Option<u64>,
    ) -> Result<(), ClientError> {
        let run_intent = SimulationStateIntent {
            run_state: SimulationState::Running,
            transition_mode: mode.clone(),
            steps_limit: None,
            checkpoint,
            reset_to: None,
        };
        expect_no_data(self.patch(&API_ROUTE_CLUSTER_WORKLOAD, &run_intent).await?)
    }

    async fn step(&self, steps: u128, checkpoint: Option<u64>) -> Result<(), ClientError> {
        let run_intent = SimulationStateIntent {
            run_state: SimulationState::Running,
            transition_mode: SimulationStateTransitionMode::WaitForAll,
            steps_limit: Some(steps),
            reset_to: None,
            checkpoint,
        };
        expect_no_data(self.patch(&API_ROUTE_CLUSTER_WORKLOAD, &run_intent).await?)
    }

    async fn reset(&self, checkpoint: CheckpointId) -> Result<(), ClientError> {
        let run_intent = SimulationStateIntent {
            run_state: SimulationState::Stopped,
            transition_mode: SimulationStateTransitionMode::WaitForAll,
            steps_limit: None,
            reset_to: Some(checkpoint),
            checkpoint: None,
        };
        expect_no_data(self.patch(&API_ROUTE_CLUSTER_WORKLOAD, &run_intent).await?)
    }

    async fn stop(&self, mode: &SimulationStateTransitionMode) -> Result<(), ClientError> {
        let run_intent = SimulationStateIntent {
            run_state: SimulationState::Stopped,
            transition_mode: mode.clone(),
            steps_limit: None,
            reset_to: None,
            checkpoint: None,
        };
        expect_no_data(self.patch(&API_ROUTE_CLUSTER_WORKLOAD, &run_intent).await?)
    }

    async fn stream(&self) -> Result<SimulationStream, ClientError> {
        todo!()
    }
}

#[async_trait]
impl MembershipApi for ApiCaller {
    async fn set_lock(
        &self,
        request: &crate::model::cluster::LockRequest,
    ) -> Result<crate::model::cluster::LockReceipt, ClientError> {
        let receipt: crate::model::cluster::LockReceipt =
            expect_data(self.post(&API_ROUTE_CLUSTER_LOCK, request).await?)?;
        if receipt.operation_id != request.operation_id
            || receipt.formation_id != request.formation_id
            || receipt.locked != request.locked
        {
            return Err(ClientError::TransportError(
                "mismatched lock acceptance receipt".into(),
            ));
        }
        Ok(receipt)
    }

    async fn is_lock(&self) -> Result<LockIntent, ClientError> {
        let summary: crate::model::cluster::Summary =
            expect_data(self.get(&API_ROUTE_CLUSTER).await?)?;
        Ok(LockIntent {
            locked: summary.membership_locked,
        })
    }

    async fn join(
        &self,
        req: &cluster::JoinRequest,
    ) -> Result<cluster::JoinOperation, ClientError> {
        req.material
            .validate()
            .map_err(|_| ClientError::TransportError("invalid join material".into()))?;
        let api_response = self
            .post(&crate::model::API_ROUTE_MEMBERSHIP_JOINS, req)
            .await?;
        let result: cluster::JoinOperation = expect_data(api_response)?;
        if result.operation_id != req.operation_id
            || result.source_formation_id != req.formation_id
            || result.target_formation_id != req.material.formation_id
            || result.recovery_reference.as_ref().is_some_and(|reference| {
                reference.introducer_node_id != req.material.introducer_node_id
                    || reference.introducer_fingerprint != req.material.introducer_fingerprint
            })
        {
            return Err(ClientError::TransportError(
                "mismatched join operation receipt".into(),
            ));
        }
        Ok(result)
    }

    async fn join_status(
        &self,
        id: &cluster::OperationId,
    ) -> Result<cluster::JoinOperation, ClientError> {
        let result: cluster::JoinOperation = expect_data(
            self.get(&crate::model::API_ROUTE_MEMBERSHIP_JOINS.with_id(String::from(id.clone())))
                .await?,
        )?;
        if &result.operation_id != id {
            return Err(ClientError::TransportError(
                "mismatched join operation status".into(),
            ));
        }
        Ok(result)
    }

    async fn inspect_admission(
        &self,
        request: &cluster::AdmissionInspectionRequest,
    ) -> Result<cluster::AdmissionInspection, ClientError> {
        let result: cluster::AdmissionInspection = expect_data(
            self.post(&crate::model::API_ROUTE_ADMISSION_INSPECTIONS, request)
                .await?,
        )?;
        if result.request != *request
            || (!matches!(
                result.outcome,
                cluster::AdmissionInspectionOutcome::WrongIssuer
            ) && (result.source_formation_id != request.formation_id
                || result.source_node_id != request.reference.introducer_node_id))
        {
            return Err(ClientError::TransportError(
                "mismatched admission inspection".into(),
            ));
        }
        Ok(result)
    }

    async fn leave(
        &self,
        request: &cluster::LeaveRequest,
    ) -> Result<cluster::LeaveReceipt, ClientError> {
        let result: cluster::LeaveReceipt = expect_data(
            self.post(&crate::model::API_ROUTE_MEMBERSHIP_LEAVES, request)
                .await?,
        )?;
        if result.operation_id != request.operation_id
            || result.previous_formation_id != request.formation_id
            || result.current.participation != cluster::Participation::Standalone
            || result.current.member_count != 1
            || (result.changed
                && (result.current.formation_id == result.previous_formation_id
                    || result.current.source_node_id == result.previous_node_id))
            || (!result.changed
                && (result.current.formation_id != result.previous_formation_id
                    || result.current.source_node_id != result.previous_node_id))
        {
            return Err(ClientError::TransportError(
                "mismatched leave acceptance receipt".into(),
            ));
        }
        Ok(result)
    }

    async fn list(&self, filter: &MembersSelector) -> Result<Vec<node::Inspection>, ClientError> {
        let invalid =
            || ClientError::TransportError("invalid or over-budget membership listing".into());
        let state = filter.member_state.as_ref().map(|s| s.to_ascii_lowercase());
        if state
            .as_deref()
            .is_some_and(|s| !matches!(s, "alive" | "suspected" | "dead" | "removed"))
            || filter
                .role
                .as_deref()
                .is_some_and(|s| !matches!(s, "introducer" | "worker"))
        {
            return Err(invalid());
        }
        tokio::time::timeout(Duration::from_secs(30), async {
            let mut result: Vec<node::Inspection> = Vec::new();
            let mut identity = None;
            let mut after: Option<NodeId> = None;
            let mut endpoint_bytes = 0usize;
            loop {
                let mut query = Url::parse("http://local.invalid/").expect("static URL");
                if let Some((formation, _)) = &identity {
                    query
                        .query_pairs_mut()
                        .append_pair("formationId", &format!("{formation}"));
                }
                if let Some(after) = &after {
                    query.query_pairs_mut().append_pair("after", after.as_ref());
                }
                let page: node::MembershipPage = expect_data(
                    self.get(
                        &API_ROUTE_CLUSTER_NODES.with_query(query.query().unwrap_or_default()),
                    )
                    .await?,
                )?;
                if page.members.len() > 4
                    || result.len() + page.members.len() > 4096
                    || identity
                        .as_ref()
                        .is_some_and(|(f, s)| f != &page.formation_id || s != &page.source_node_id)
                    || page.next_after.as_ref().is_some_and(|next| {
                        page.members.last().is_none_or(|last| &last.node_id != next)
                    })
                {
                    return Err(invalid());
                }
                for member in &page.members {
                    if member.formation_id != page.formation_id
                        || member.source_node_id != page.source_node_id
                        || after
                            .as_ref()
                            .is_some_and(|previous| previous >= &member.node_id)
                    {
                        return Err(invalid());
                    }
                    endpoint_bytes = endpoint_bytes.saturating_add(
                        member
                            .peer_endpoints
                            .iter()
                            .chain(&member.client_endpoints)
                            .map(String::len)
                            .sum::<usize>(),
                    );
                    if endpoint_bytes > 16 * 1024 * 1024 {
                        return Err(invalid());
                    }
                    after = Some(member.node_id.clone());
                }
                identity = Some((page.formation_id, page.source_node_id));
                result.extend(page.members);
                if page.next_after.is_none() {
                    break;
                }
                if result.len() == 4096 {
                    return Err(invalid());
                }
            }
            result.retain(|member| {
                let liveness = match member.liveness {
                    MemberState::Alive => "alive",
                    MemberState::Suspected => "suspected",
                    MemberState::Dead => "dead",
                    MemberState::Removed => "removed",
                };
                state.as_deref().is_none_or(|s| s == liveness)
                    && filter
                        .name
                        .as_ref()
                        .is_none_or(|name| name == member.worker_name.as_str())
                    && match filter.role.as_deref() {
                        Some("introducer") => member.accepts.peers,
                        Some("worker") => member.accepts.work,
                        _ => true,
                    }
            });
            Ok(result)
        })
        .await
        .map_err(|_| ClientError::TransportError("membership listing deadline exceeded".into()))?
    }

    async fn get(
        &self,
        id: &NodeId,
        source: Option<InspectSource>,
    ) -> Result<node::Inspection, ClientError> {
        let route = match source {
            Some(s) => API_ROUTE_CLUSTER_NODES
                .with_id(id)
                .with_query(format!("source={}", s)),
            None => API_ROUTE_CLUSTER_NODES.with_id(id),
        };
        let api_response = self.get(&route).await?;
        let result: node::Inspection = expect_data(api_response)?;
        if &result.node_id != id {
            return Err(ClientError::TransportError(
                "mismatched node inspection identity".into(),
            ));
        }
        Ok(result)
    }

    async fn remove(&self, id: &NodeId, mode: &RemoveMode) -> Result<(), ClientError> {
        expect_no_data(
            self.delete(&API_ROUTE_CLUSTER_NODES.with_id(id).with_query_param(mode))
                .await?,
        )
    }
}

#[async_trait]
impl CheckpointApi for ApiCaller {
    async fn list(
        &self,
        filter: &checkpoint::Filter,
    ) -> Result<Vec<checkpoint::Record>, ClientError> {
        let api_response = self
            .get(&API_ROUTE_CLUSTER_WORKLOAD_CHECKPOINT.with_query_param(filter))
            .await?;

        let result: Vec<Record> = expect_collection(api_response)?;
        Ok(result)
    }

    async fn get(&self, id: &CheckpointId) -> Result<Record, ClientError> {
        let api_response = self
            .get(&API_ROUTE_CLUSTER_WORKLOAD_CHECKPOINT.with_id(id))
            .await?;
        let result: Record = expect_data(api_response)?;
        Ok(result)
    }

    async fn download(
        &self,
        id: &CheckpointId,
        writer: &mut (dyn tokio::io::AsyncWrite + Unpin + Send),
    ) -> Result<(), ClientError> {
        let route = API_ROUTE_CLUSTER_WORKLOAD_CHECKPOINT
            .with_id(id)
            .with_query("download=true");
        self.download_to(&route, writer).await
    }

    async fn delete(&self, id: &CheckpointId) -> Result<(), ClientError> {
        expect_no_data(
            self.delete(&API_ROUTE_CLUSTER_WORKLOAD_CHECKPOINT.with_id(id))
                .await?,
        )
    }

    async fn purge(&self, filter: &checkpoint::Filter) -> Result<EntriesRemoved, ClientError> {
        let api_response = self
            .delete(&API_ROUTE_CLUSTER_WORKLOAD_CHECKPOINT.with_query_param(filter))
            .await?;
        let result: EntriesRemoved = expect_data(api_response)?;
        Ok(result)
    }
}

#[async_trait]
impl ResultsApi for ApiCaller {
    async fn list(&self, filter: &result::Filter) -> Result<Vec<result::Record>, ClientError> {
        let api_response = self
            .get(&API_ROUTE_CLUSTER_RESULTS.with_query_param(filter))
            .await?;

        let result: Vec<result::Record> = expect_collection(api_response)?;
        Ok(result)
    }

    async fn get(&self, id: &ResultId) -> Result<result::Record, ClientError> {
        let api_response = self.get(&API_ROUTE_CLUSTER_RESULTS.with_id(id)).await?;
        let result: result::Record = expect_data(api_response)?;
        Ok(result)
    }

    async fn download(
        &self,
        id: &ResultId,
        writer: &mut (dyn tokio::io::AsyncWrite + Unpin + Send),
    ) -> Result<(), ClientError> {
        let route = API_ROUTE_CLUSTER_RESULTS
            .with_id(id)
            .with_query("download=true");
        self.download_to(&route, writer).await
    }
    async fn delete(&self, id: &ResultId) -> Result<(), ClientError> {
        expect_no_data(self.delete(&API_ROUTE_CLUSTER_RESULTS.with_id(id)).await?)
    }

    async fn purge(&self, filter: &result::Filter) -> Result<EntriesRemoved, ClientError> {
        let api_response = self
            .delete(&API_ROUTE_CLUSTER_RESULTS.with_query_param(filter))
            .await?;
        let result: EntriesRemoved = expect_data(api_response)?;
        Ok(result)
    }
}

#[async_trait]
impl super::ClientApi for HttpClusterClient {
    /// Get cluster-level API
    fn cluster(&self) -> impl ClusterApi {
        ApiCaller {
            base_url: self.base_url.clone(),
            client: self.client.clone(),
        }
    }
    /// Get blocklist APIs
    fn blocklist(&self) -> impl BlocklistApi {
        ApiCaller {
            base_url: self.base_url.clone(),
            client: self.client.clone(),
        }
    }
    /// Get graveyard (tombstones) APIs
    fn graveyard(&self) -> impl GraveyardApi {
        ApiCaller {
            base_url: self.base_url.clone(),
            client: self.client.clone(),
        }
    }
    /// Get results APIs
    fn results(&self) -> impl ResultsApi {
        ApiCaller {
            base_url: self.base_url.clone(),
            client: self.client.clone(),
        }
    }
    /// Get workload management API
    fn workload(&self) -> impl WorkloadApi {
        ApiCaller {
            base_url: self.base_url.clone(),
            client: self.client.clone(),
        }
    }
    /// Get checkpoint APIs
    fn checkpoints(&self) -> impl CheckpointApi {
        ApiCaller {
            base_url: self.base_url.clone(),
            client: self.client.clone(),
        }
    }
    /// Get membership API
    fn membership(&self) -> impl MembershipApi {
        ApiCaller {
            base_url: self.base_url.clone(),
            client: self.client.clone(),
        }
    }

    // ── Cluster resource ──────────────────────────────────────────────────────

    async fn create_join_token(&self) -> Result<JoinToken, ClientError> {
        let caller = ApiCaller {
            base_url: self.base_url.clone(),
            client: self.client.clone(),
        };
        let api_response = caller
            .post(&API_ROUTE_CLUSTER_TOKEN, &serde_json::json!({}))
            .await?;
        let result: JoinToken = expect_data(api_response)?;
        Ok(result)
    }

    async fn get_join_token(&self) -> Result<cluster::JoinMaterial, ClientError> {
        let caller = ApiCaller {
            base_url: self.base_url.clone(),
            client: self.client.clone(),
        };
        let api_response = caller.get(&API_ROUTE_CLUSTER_TOKEN).await?;
        let result: cluster::JoinMaterial = expect_data(api_response)?;
        Ok(result)
    }

    // ── Node resource ─────────────────────────────────────────────────────────

    async fn diagnose_node(
        &self,
        id: NodeId,
        source: Option<NodeId>,
    ) -> Result<DiagnosticResult, ClientError> {
        // /cluster/nodes/:id/diagnostics?from=
        let route = match source {
            Some(s) => API_ROUTE_CLUSTER_NODES
                .with_id(id)
                .with_id("diagnostics")
                .with_query(format!("from={}", s)),
            None => API_ROUTE_CLUSTER_NODES.with_id(id).with_id("diagnostics"),
        };
        let caller = ApiCaller {
            base_url: self.base_url.clone(),
            client: self.client.clone(),
        };

        let api_response = caller.get(&route).await?;
        let result: DiagnosticResult = expect_data(api_response)?;
        Ok(result)
    }
}

fn resolve_hostname_std(host: &String) -> Result<Vec<std::net::SocketAddr>> {
    Ok(host.to_socket_addrs()?.collect())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use crate::model::{ApiResponse, manifest};
    use pretty_assertions::assert_eq;
    use reqwest::Client;
    use reqwest_middleware::ClientBuilder as MiddlewareClientBuilder;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Build an ApiCaller pointing at the given mock server with CBOR middleware.
    fn make_caller(server: &MockServer) -> ApiCaller {
        let client = MiddlewareClientBuilder::new(Client::builder().build().unwrap())
            .with(CborContentMiddleware::default())
            .build();
        ApiCaller {
            base_url: Url::parse(&format!("{}/api/v1/", server.uri())).unwrap(),
            client,
        }
    }

    /// CBOR-encode an ApiResponse into bytes (mirrors what a real server would send).
    fn cbor_encode(response: &ApiResponse) -> Vec<u8> {
        let mut buf = Vec::new();
        ciborium::into_writer(response, &mut buf).unwrap();
        buf
    }

    // ── serialize / with_resource ────────────────────────────────────────────

    #[test]
    fn test_serialize_roundtrips_cbor() {
        let client = MiddlewareClientBuilder::new(Client::builder().build().unwrap()).build();
        let caller = ApiCaller {
            base_url: Url::parse("http://localhost/api/v1/").unwrap(),
            client,
        };

        let payload = "hello".to_string();
        let bytes = caller.serialize(&payload).unwrap();
        let decoded: String = ciborium::from_reader(Cursor::new(bytes)).unwrap();
        assert_eq!(decoded, "hello");
    }

    #[test]
    fn test_with_resource_joins_url_correctly() {
        let client = MiddlewareClientBuilder::new(Client::builder().build().unwrap()).build();
        let caller = ApiCaller {
            base_url: Url::parse("http://localhost/api/v1/").unwrap(),
            client,
        };

        assert_eq!(
            caller
                .with_resource(&ApiRoute::from_static("cluster"))
                .unwrap()
                .as_str(),
            "http://localhost/api/v1/cluster"
        );
        assert_eq!(
            caller
                .with_resource(&API_ROUTE_CLUSTER_RESULTS.with_id(123))
                .unwrap()
                .as_str(),
            "http://localhost/api/v1/cluster/results/123"
        );
    }

    // ── GET ──────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_get_204_returns_ok_with_no_data() {
        let server = MockServer::start().await;
        Mock::given(method(http::Method::GET))
            .and(path("/api/v1/resource"))
            .respond_with(ResponseTemplate::new(StatusCode::NO_CONTENT))
            .mount(&server)
            .await;

        let caller = make_caller(&server);
        let result = caller
            .get(&ApiRoute::from_static("resource"))
            .await
            .unwrap();

        match result {
            ApiResponse::Ok { data } => {
                assert!(data.is_none());
            }
            other => panic!("expected ApiResponse::Ok {{ data: None }}, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_get_200_deserializes_cbor_ok_response() {
        let server = MockServer::start().await;
        let body = ApiResponse::Ok { data: None };

        Mock::given(method(http::Method::GET))
            .and(path("/api/v1/resource"))
            .respond_with(ResponseTemplate::new(StatusCode::OK).set_body_bytes(cbor_encode(&body)))
            .mount(&server)
            .await;

        let caller = make_caller(&server);
        let result = caller
            .get(&ApiRoute::from_static("resource"))
            .await
            .unwrap();

        match result {
            ApiResponse::Ok { data } => {
                assert!(data.is_none());
            }
            other => panic!("expected ApiResponse::Ok, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_get_deserializes_cbor_error_response() {
        let server = MockServer::start().await;
        let body = ApiResponse::Error {
            code: "NOT_FOUND".into(),
            message: "no such thing".into(),
        };

        Mock::given(method(http::Method::GET))
            .and(path("/api/v1/things/42"))
            .respond_with(
                ResponseTemplate::new(StatusCode::NOT_FOUND).set_body_bytes(cbor_encode(&body)),
            )
            .mount(&server)
            .await;

        let caller = make_caller(&server);
        let result = caller
            .get(&ApiRoute::from_static("things/42"))
            .await
            .unwrap_err();

        match result {
            ClientError::ApiError { status, message } => {
                assert_eq!(status, 404);
                assert_eq!(message, "no such thing");
            }
            other => panic!("expected ApiResponse::Error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_get_connection_refused_returns_connection_failed() {
        let client = MiddlewareClientBuilder::new(Client::builder().build().unwrap()).build();
        let caller = ApiCaller {
            base_url: Url::parse("http://127.0.0.1:1/api/v1/").unwrap(),
            client,
        };

        let err = caller
            .get(&ApiRoute::from_static("resource"))
            .await
            .unwrap_err();
        match err {
            ClientError::ConnectionFailed(_) => {}
            other => panic!("expected ConnectionFailed, got {:?}", other),
        }
    }

    // ── POST ─────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_post_sends_cbor_content_type_and_body() {
        let server = MockServer::start().await;

        Mock::given(method(http::Method::POST))
            .and(path("/api/v1/collection"))
            .and(header("Content-Type", "application/cbor"))
            .respond_with(ResponseTemplate::new(StatusCode::NO_CONTENT))
            .expect(1)
            .mount(&server)
            .await;

        let caller = make_caller(&server);
        let payload = "test_data".to_string();
        let result = caller
            .post(&ApiRoute::from_static("collection"), &payload)
            .await
            .unwrap();

        match result {
            ApiResponse::Ok { data } => {
                assert!(data.is_none());
            }
            other => panic!("expected ApiResponse::Ok, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_post_200_deserializes_cbor_body() {
        let server = MockServer::start().await;
        let body = ApiResponse::Ok { data: None };

        Mock::given(method(http::Method::POST))
            .and(path("/api/v1/collection"))
            .respond_with(ResponseTemplate::new(StatusCode::OK).set_body_bytes(cbor_encode(&body)))
            .mount(&server)
            .await;

        let caller = make_caller(&server);
        let result = caller
            .post(&ApiRoute::from_static("collection"), &42u32)
            .await
            .unwrap();

        match result {
            ApiResponse::Ok { data } => {
                assert!(data.is_none());
            }
            other => panic!("expected ApiResponse::Ok, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn admission_inspection_correlates_request_and_issuer_but_preserves_unknown_outcomes() {
        use crate::model::cluster::{
            AdmissionInspection, AdmissionInspectionOutcome as Outcome, AdmissionInspectionRequest,
            JoinRecoveryReference,
        };
        let request = AdmissionInspectionRequest {
            schema_version: 1,
            formation_id: "target".parse().unwrap(),
            reference: JoinRecoveryReference {
                attempt_id: "peer-attempt".parse().unwrap(),
                applicant_fingerprint: "11".repeat(32).parse().unwrap(),
                introducer_node_id: "issuer".parse().unwrap(),
                introducer_fingerprint: "22".repeat(32).parse().unwrap(),
            },
        };
        for case in 0..8 {
            let server = MockServer::start().await;
            let mut report = AdmissionInspection {
                schema_version: 1,
                request: request.clone(),
                source_formation_id: request.formation_id.clone(),
                source_node_id: request.reference.introducer_node_id.clone(),
                outcome: Outcome::CurrentMember {
                    node_id: "assigned".parse().unwrap(),
                },
            };
            match case {
                0 => {}
                1 => report.request.reference.attempt_id = "other-attempt".parse().unwrap(),
                2 => report.source_formation_id = "other-formation".parse().unwrap(),
                3 => report.source_node_id = "other-issuer".parse().unwrap(),
                4 => {
                    report.source_formation_id = "restarted-formation".parse().unwrap();
                    report.source_node_id = "restarted-node".parse().unwrap();
                    report.outcome = Outcome::WrongIssuer;
                }
                5 => report.outcome = Outcome::RecordUnavailable,
                6 => {
                    report.outcome = Outcome::RetiredOrRestricted {
                        node_id: "assigned".parse().unwrap(),
                    }
                }
                7 => report.schema_version = 2,
                _ => unreachable!(),
            }
            let body = ApiResponse::Ok {
                data: Some(crate::model::ResponseData::AdmissionInspection(
                    report.clone(),
                )),
            };
            Mock::given(method(http::Method::POST))
                .and(path("/api/v1/membership/admission-inspections"))
                .and(header("Content-Type", "application/cbor"))
                .respond_with(
                    ResponseTemplate::new(StatusCode::OK).set_body_bytes(cbor_encode(&body)),
                )
                .expect(1)
                .mount(&server)
                .await;
            let result = make_caller(&server).inspect_admission(&request).await;
            if matches!(case, 1..=3 | 7) {
                assert!(
                    result.is_err(),
                    "case {case} must reject mismatched/unsupported evidence"
                );
            } else {
                assert_eq!(
                    result.unwrap(),
                    report,
                    "case {case} must preserve the evidence category"
                );
            }
        }
    }

    #[tokio::test]
    async fn lock_receipt_must_match_the_requested_intent() {
        use crate::model::cluster::{LockReceipt, LockRequest};
        let request = LockRequest {
            schema_version: 1,
            operation_id: "request-1".parse().unwrap(),
            formation_id: "formation-a".parse().unwrap(),
            locked: true,
        };
        for mismatch in 0..3 {
            let server = MockServer::start().await;
            let mut receipt = LockReceipt {
                schema_version: 1,
                operation_id: request.operation_id.clone(),
                formation_id: request.formation_id.clone(),
                source_node_id: "node-a".parse().unwrap(),
                locked: request.locked,
                policy_version: None,
            };
            match mismatch {
                0 => receipt.operation_id = "other-request".parse().unwrap(),
                1 => receipt.formation_id = "other-formation".parse().unwrap(),
                _ => receipt.locked = false,
            }
            let body = ApiResponse::Ok {
                data: Some(crate::model::ResponseData::LockReceipt(receipt)),
            };
            Mock::given(method(http::Method::POST))
                .and(path("/api/v1/cluster/lock"))
                .respond_with(
                    ResponseTemplate::new(StatusCode::OK).set_body_bytes(cbor_encode(&body)),
                )
                .mount(&server)
                .await;
            assert!(matches!(
                make_caller(&server).set_lock(&request).await,
                Err(ClientError::TransportError(_))
            ));
        }
    }

    // ── PUT ──────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_put_sends_cbor_content_type_and_body() {
        let server = MockServer::start().await;

        Mock::given(method(http::Method::PUT))
            .and(path("/api/v1/item/1"))
            .and(header("Content-Type", "application/cbor"))
            .respond_with(ResponseTemplate::new(StatusCode::NO_CONTENT))
            .expect(1)
            .mount(&server)
            .await;

        let caller = make_caller(&server);
        let result = caller
            .put(&ApiRoute::from_static("item/1"), &"updated")
            .await
            .unwrap();

        match result {
            ApiResponse::Ok { data, .. } => assert!(data.is_none()),
            other => panic!("expected ApiResponse::Ok, got {:?}", other),
        }
    }

    // ── PATCH ──────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_patch_sends_cbor_content_type_and_body() {
        let server = MockServer::start().await;

        Mock::given(method(http::Method::PATCH))
            .and(path("/api/v1/item/1"))
            .and(header("Content-Type", "application/cbor"))
            .respond_with(ResponseTemplate::new(StatusCode::NO_CONTENT))
            .expect(1)
            .mount(&server)
            .await;

        let caller = make_caller(&server);
        let result = caller
            .patch(&ApiRoute::from_static("item/1"), &"updated")
            .await
            .unwrap();

        match result {
            ApiResponse::Ok { data, .. } => assert!(data.is_none()),
            other => panic!("expected ApiResponse::Ok, got {:?}", other),
        }
    }

    // ── DELETE ────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_delete_204_returns_ok() {
        let server = MockServer::start().await;

        Mock::given(method(http::Method::DELETE))
            .and(path("/api/v1/item/99"))
            .respond_with(ResponseTemplate::new(StatusCode::NO_CONTENT))
            .mount(&server)
            .await;

        let caller = make_caller(&server);
        let result = caller
            .delete(&ApiRoute::from_static("item/99"))
            .await
            .unwrap();

        match result {
            ApiResponse::Ok { data } => {
                assert!(data.is_none());
            }
            other => panic!("expected ApiResponse::Ok, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_delete_with_error_response() {
        let server = MockServer::start().await;
        let body = ApiResponse::Error {
            code: "FORBIDDEN".into(),
            message: "not allowed".into(),
        };

        Mock::given(method(http::Method::DELETE))
            .and(path("/api/v1/item/1"))
            .respond_with(
                ResponseTemplate::new(StatusCode::FORBIDDEN).set_body_bytes(cbor_encode(&body)),
            )
            .mount(&server)
            .await;

        let caller = make_caller(&server);
        let result = caller
            .delete(&ApiRoute::from_static("item/1"))
            .await
            .unwrap_err();

        match result {
            ClientError::ApiError { status, message } => {
                assert_eq!(status, 403);
                assert_eq!(message, "not allowed");
            }
            other => panic!("expected ApiResponse::Error, got {:?}", other),
        }
    }

    // ── map_response edge cases ──────────────────────────────────────────────

    #[tokio::test]
    async fn non_success_status_cannot_carry_successful_domain_outcome() {
        let server = MockServer::start().await;
        Mock::given(method(http::Method::GET))
            .and(path("/api/v1/mismatch"))
            .respond_with(
                ResponseTemplate::new(StatusCode::SERVICE_UNAVAILABLE)
                    .set_body_bytes(cbor_encode(&ApiResponse::Ok { data: None })),
            )
            .mount(&server)
            .await;
        let error = make_caller(&server)
            .get(&ApiRoute::from_static("mismatch"))
            .await
            .unwrap_err();
        assert!(matches!(error, ClientError::TransportError(message) if message.contains("503")));
    }

    #[tokio::test]
    async fn test_200_with_invalid_cbor_returns_transport_error() {
        let server = MockServer::start().await;

        Mock::given(method(http::Method::GET))
            .and(path("/api/v1/broken"))
            .respond_with(
                ResponseTemplate::new(StatusCode::OK).set_body_bytes(b"this is not cbor".to_vec()),
            )
            .mount(&server)
            .await;

        let caller = make_caller(&server);
        let err = caller
            .get(&ApiRoute::from_static("broken"))
            .await
            .unwrap_err();

        match err {
            ClientError::TransportError(msg) => {
                assert!(
                    msg.contains("failed to deserialize response body"),
                    "unexpected message: {msg}"
                );
            }
            other => panic!("expected TransportError, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_unmatched_route_returns_error() {
        let server = MockServer::start().await;
        // No mocks mounted — wiremock returns 404 with a text body by default,
        // which fails CBOR deserialization → TransportError.

        let caller = make_caller(&server);
        let result = caller.get(&ApiRoute::from_static("nowhere")).await;
        assert!(result.is_err());
    }

    // ── HTTP status → ClientError mapping (empty body) ──────────────────────

    #[tokio::test]
    async fn test_401_empty_body_returns_authentication_required() {
        let server = MockServer::start().await;

        Mock::given(method(http::Method::GET))
            .and(path("/api/v1/secret"))
            .respond_with(ResponseTemplate::new(StatusCode::UNAUTHORIZED))
            .mount(&server)
            .await;

        let caller = make_caller(&server);
        let err = caller
            .get(&ApiRoute::from_static("secret"))
            .await
            .unwrap_err();

        match err {
            ClientError::AuthenticationRequired => {}
            other => panic!("expected AuthenticationRequired, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_403_empty_body_returns_authorization_denied() {
        let server = MockServer::start().await;

        Mock::given(method(http::Method::GET))
            .and(path("/api/v1/admin"))
            .respond_with(ResponseTemplate::new(StatusCode::FORBIDDEN))
            .mount(&server)
            .await;

        let caller = make_caller(&server);
        let err = caller
            .get(&ApiRoute::from_static("admin"))
            .await
            .unwrap_err();

        match err {
            ClientError::AuthorizationDenied => {}
            other => panic!("expected AuthorizationDenied, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_404_empty_body_returns_resource_not_found() {
        let server = MockServer::start().await;

        Mock::given(method(http::Method::GET))
            .and(path("/api/v1/missing"))
            .respond_with(ResponseTemplate::new(StatusCode::NOT_FOUND))
            .mount(&server)
            .await;

        let caller = make_caller(&server);
        let err = caller
            .get(&ApiRoute::from_static("missing"))
            .await
            .unwrap_err();

        match err {
            ClientError::ResourceNotFound(_) => {}
            other => panic!("expected ResourceNotFound, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_500_empty_body_returns_transport_error() {
        let server = MockServer::start().await;

        Mock::given(method(http::Method::GET))
            .and(path("/api/v1/crash"))
            .respond_with(ResponseTemplate::new(StatusCode::INTERNAL_SERVER_ERROR))
            .mount(&server)
            .await;

        let caller = make_caller(&server);
        let err = caller
            .get(&ApiRoute::from_static("crash"))
            .await
            .unwrap_err();

        match err {
            ClientError::TransportError(msg) => {
                assert!(msg.contains("500"), "unexpected message: {msg}");
            }
            other => panic!("expected TransportError, got {:?}", other),
        }
    }

    // ── expect_no_data / expect_data helpers ─────────────────────────────────

    #[test]
    fn test_expect_no_data_ok_without_data() {
        let resp = ApiResponse::Ok { data: None };
        assert!(expect_no_data(resp).is_ok());
    }

    #[test]
    fn test_expect_no_data_rejects_unexpected_data() {
        let manifest = cluster::manifest(
            "test",
            ClusterSpec {
                membership_locked: false,
                workload: None,
            },
        );
        let resp = ApiResponse::Ok {
            data: Some(ResponseData::ClusterManifest(manifest)),
        };
        let err = expect_no_data(resp).unwrap_err();
        match err {
            ClientError::TransportError(msg) => {
                assert!(msg.contains("expected no response"), "unexpected: {msg}");
            }
            other => panic!("expected TransportError, got {:?}", other),
        }
    }

    #[test]
    fn test_expect_no_data_maps_api_error() {
        let resp = ApiResponse::Error {
            code: "FAIL".into(),
            message: "something broke".into(),
        };
        let err = expect_no_data(resp).unwrap_err();
        match err {
            ClientError::ApiError { message, .. } => {
                assert_eq!(message, "something broke");
            }
            other => panic!("expected ApiError, got {:?}", other),
        }
    }

    #[test]
    fn test_expect_data_extracts_cluster_manifest() {
        let manifest = cluster::manifest(
            "my-cluster",
            ClusterSpec {
                membership_locked: false,
                workload: None,
            },
        );
        let resp = ApiResponse::Ok {
            data: Some(ResponseData::ClusterManifest(manifest)),
        };

        let result: cluster::Manifest = expect_data(resp).unwrap();
        assert_eq!(
            result.metadata.name,
            manifest::Name::from_str("my-cluster").unwrap()
        );
    }

    #[test]
    fn test_expect_data_rejects_wrong_variant() {
        let manifest = cluster::manifest(
            "test",
            ClusterSpec {
                membership_locked: false,
                workload: None,
            },
        );
        let resp = ApiResponse::Ok {
            data: Some(ResponseData::ClusterManifest(manifest)),
        };

        // Ask for ResultArtifact but the data contains ClusterManifest
        let err = expect_data::<crate::model::result::Record>(resp).unwrap_err();
        match err {
            ClientError::TransportError(msg) => {
                assert!(msg.contains("expected ResultRecord"), "unexpected: {msg}");
            }
            other => panic!("expected TransportError, got {:?}", other),
        }
    }

    #[test]
    fn test_expect_data_errors_on_none() {
        let resp = ApiResponse::Ok { data: None };

        let err = expect_data::<cluster::Manifest>(resp).unwrap_err();
        match err {
            ClientError::TransportError(msg) => {
                assert!(
                    msg.contains("expected response data, got none"),
                    "unexpected: {msg}"
                );
            }
            other => panic!("expected TransportError, got {:?}", other),
        }
    }

    #[test]
    fn test_expect_data_maps_api_error() {
        let resp = ApiResponse::Error {
            code: "ERR".into(),
            message: "not found".into(),
        };

        let err = expect_data::<cluster::Manifest>(resp).unwrap_err();
        match err {
            ClientError::ApiError { message, .. } => {
                assert_eq!(message, "not found");
            }
            other => panic!("expected ApiError, got {:?}", other),
        }
    }
}

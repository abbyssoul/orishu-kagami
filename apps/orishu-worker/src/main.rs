mod client_assembly;
mod config;
#[cfg(feature = "observability")]
mod diagnostics;

#[cfg(all(test, unix))]
mod client_flow_tests;
#[cfg(all(test, unix))]
mod client_pressure_tests;

#[cfg(all(feature = "formation-fault-test", not(debug_assertions)))]
compile_error!("formation-fault-test is forbidden in release builds");

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use config::RuntimeConfig;
use orishu_worker::{
    credentials::WorkerCredentials,
    runtime::{RunningWorker, WorkerRuntime},
};
use salvo::conn::rustls::{Keycert, RustlsConfig};
use salvo::prelude::*;
use salvo::server::ServerHandle;
use tokio::signal;
use tokio::task::JoinHandle;

/// Fixed formation-PoC limits, applied before route authentication. Handler
/// semaphores separately bound admitted requests, not incomplete HTTP heads
/// or transport writes stalled by clients that stop reading.
fn client_server<A>(acceptor: A) -> Server<client_assembly::AssemblyAcceptor<A>>
where
    A: salvo::conn::Acceptor,
    A::Stream: tokio::io::AsyncRead + tokio::io::AsyncWrite,
{
    let mut server = Server::new(client_assembly::AssemblyAcceptor(acceptor))
        .max_connections(64)
        .fuse_config(
            salvo::fuse::FuseConfig::default()
                .with_tls_handshake_timeout(std::time::Duration::from_secs(10))
                .with_http1_header_timeout(std::time::Duration::from_secs(5))
                .with_connection_idle_timeout(std::time::Duration::from_secs(10))
                .with_write_stall_timeout(std::time::Duration::from_secs(5)),
        );
    server.http1_mut().max_buf_size(8192).max_headers(32);
    server
        .http2_mut()
        .max_concurrent_streams(16)
        .max_header_list_size(8192)
        .header_table_size(4096)
        .max_frame_size(16384)
        .initial_stream_window_size(65535)
        .initial_connection_window_size(65535)
        .adaptive_window(false)
        .max_send_buf_size(16384);
    server
}

// ── Handlers ────────────────────────────────────────────────────────────────

struct ClusterSummaryHandler {
    runtime: Arc<RunningWorker>,
    local: bool,
}

#[handler]
impl ClusterSummaryHandler {
    async fn handle(&self, req: &mut Request, res: &mut Response) {
        if !self.local {
            let values = req.headers().get_all("authorization");
            let mut values = values.iter();
            let token = values
                .next()
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.strip_prefix("Bearer "));
            if values.next().is_some()
                || !token.is_some_and(|token| self.runtime.authorize_operator(token))
            {
                res.status_code(StatusCode::UNAUTHORIZED);
                res.add_header("WWW-Authenticate", "Bearer", true)
                    .expect("static header");
                write_api_response(
                    res,
                    &orishu::model::ApiResponse::Error {
                        code: "Unauthorized".into(),
                        message: "operator credential required".into(),
                    },
                );
                return;
            }
        }
        let summary = match self.runtime.summary() {
            Ok(summary) => summary,
            Err(_) => {
                res.status_code(StatusCode::SERVICE_UNAVAILABLE);
                write_api_response(
                    res,
                    &orishu::model::ApiResponse::Error {
                        code: "OwnerUnavailable".into(),
                        message: "membership owner is not running".into(),
                    },
                );
                return;
            }
        };
        let node = summary.source_node_id.to_string();
        let response = orishu::model::ApiResponse::Ok {
            data: Some(orishu::model::ResponseData::ClusterSummary(summary)),
        };
        res.add_header("X-Node-Id", node, true)
            .expect("validated node identity");
        write_api_response(res, &response);
    }
}

fn write_api_response(res: &mut Response, response: &orishu::model::ApiResponse) {
    let bytes = orishu_worker::peer::codec::encode(response).expect("bounded local response");
    res.add_header("Content-Type", "application/cbor", true)
        .expect("static content type");
    res.add_header("Cache-Control", "no-store", true)
        .expect("static cache policy");
    res.write_body(bytes).expect("response body");
}

enum MembershipMutation {
    Lock,
    Leave,
}

struct MembershipMutationHandler {
    kind: MembershipMutation,
    runtime: Arc<RunningWorker>,
    capacity: Arc<tokio::sync::Semaphore>,
}

struct JoinOperationHandler {
    runtime: Arc<RunningWorker>,
    capacity: Arc<tokio::sync::Semaphore>,
}
#[handler]
impl JoinOperationHandler {
    async fn handle(&self, req: &mut Request, res: &mut Response) {
        let mut values = req.headers().get_all("authorization").iter();
        let valid = values
            .next()
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .is_some_and(|v| self.runtime.authorize_operator(v));
        if !valid || values.next().is_some() {
            res.add_header("WWW-Authenticate", "Bearer", true)
                .expect("static header");
            api_error(res, StatusCode::UNAUTHORIZED, "Unauthorized");
            return;
        }
        let Ok(_permit) = self.capacity.try_acquire() else {
            api_error(res, StatusCode::SERVICE_UNAVAILABLE, "Overloaded");
            return;
        };
        if req.uri().query().is_some_and(|q| !q.is_empty())
            || req.headers().contains_key("if-match")
        {
            api_error(res, StatusCode::BAD_REQUEST, "UnsupportedPrecondition");
            return;
        }
        let status_id = req.param::<String>("operation");
        let submission = status_id.is_none();
        let result = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            if let Some(id) = status_id {
                let id = id
                    .parse()
                    .map_err(|_| (StatusCode::BAD_REQUEST, "InvalidOperationId"))?;
                return self
                    .runtime
                    .join_status(id)
                    .await
                    .map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "OwnerUnavailable"))?
                    .ok_or((StatusCode::NOT_FOUND, "UnknownOperation"));
            }
            if req.headers().contains_key("content-encoding")
                || req
                    .headers()
                    .get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .map(|v| v.split(';').next().unwrap().trim())
                    != Some("application/cbor")
            {
                return Err((StatusCode::UNSUPPORTED_MEDIA_TYPE, "UnsupportedMediaType"));
            }
            let bytes = req
                .payload_with_max_size(16 * 1024)
                .await
                .map_err(|_| (StatusCode::BAD_REQUEST, "InvalidBody"))?;
            let request = orishu_worker::peer::codec::decode(bytes)
                .map_err(|_| (StatusCode::BAD_REQUEST, "InvalidRequest"))?;
            self.runtime.submit_join(request).await.map_err(|error| {
                use orishu_worker::{join_operations::OperationError, runtime::JoinSubmitError};
                match error {
                    JoinSubmitError::Driver(_) => {
                        (StatusCode::SERVICE_UNAVAILABLE, "OutcomeUnknown")
                    }
                    JoinSubmitError::Operation(OperationError::Stale) => {
                        (StatusCode::PRECONDITION_FAILED, "StaleFormation")
                    }
                    JoinSubmitError::Operation(OperationError::Conflict) => {
                        (StatusCode::CONFLICT, "OperationConflict")
                    }
                    JoinSubmitError::Operation(OperationError::Busy) => {
                        (StatusCode::CONFLICT, "ParticipationUnavailable")
                    }
                    JoinSubmitError::Operation(OperationError::Full) => {
                        (StatusCode::SERVICE_UNAVAILABLE, "OperationHistoryFull")
                    }
                    JoinSubmitError::Operation(_) => (StatusCode::BAD_REQUEST, "InvalidRequest"),
                }
            })
        })
        .await;
        match result {
            Ok(Ok(operation)) => {
                if let Ok(summary) = self.runtime.summary() {
                    res.add_header("X-Node-Id", summary.source_node_id.to_string(), true)
                        .expect("validated identity");
                }
                use orishu::model::cluster::JoinOperationState as State;
                if submission
                    && matches!(
                        operation.state,
                        State::Connecting | State::Admitting | State::CatchingUp { .. }
                    )
                {
                    res.status_code(StatusCode::ACCEPTED);
                }
                res.add_header(
                    "Location",
                    format!(
                        "/api/v1/membership/joins/{}",
                        String::from(operation.operation_id.clone())
                    ),
                    true,
                )
                .expect("validated operation ID");
                write_api_response(
                    res,
                    &orishu::model::ApiResponse::Ok {
                        data: Some(orishu::model::ResponseData::JoinOperation(operation)),
                    },
                );
            }
            Ok(Err((status, code))) => api_error(res, status, code),
            Err(_) => api_error(res, StatusCode::GATEWAY_TIMEOUT, "OutcomeUnknown"),
        }
    }
}

struct AdmissionInspectionHandler {
    runtime: Arc<RunningWorker>,
    capacity: Arc<tokio::sync::Semaphore>,
}
#[handler]
impl AdmissionInspectionHandler {
    async fn handle(&self, req: &mut Request, res: &mut Response) {
        let mut headers = req.headers().get_all("authorization").iter();
        let authorized = headers
            .next()
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .is_some_and(|v| self.runtime.authorize_operator(v));
        if !authorized || headers.next().is_some() {
            res.add_header("WWW-Authenticate", "Bearer", true)
                .expect("static header");
            api_error(res, StatusCode::UNAUTHORIZED, "Unauthorized");
            return;
        }
        let Ok(_permit) = self.capacity.try_acquire() else {
            api_error(res, StatusCode::SERVICE_UNAVAILABLE, "Overloaded");
            return;
        };
        let outcome = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            if req.uri().query().is_some_and(|q| !q.is_empty())
                || req.headers().contains_key("if-match")
            {
                return Err((StatusCode::BAD_REQUEST, "UnsupportedPrecondition"));
            }
            if req.headers().contains_key("content-encoding")
                || req
                    .headers()
                    .get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .map(|v| v.split(';').next().unwrap().trim())
                    != Some("application/cbor")
            {
                return Err((StatusCode::UNSUPPORTED_MEDIA_TYPE, "UnsupportedMediaType"));
            }
            let bytes = req
                .payload_with_max_size(4096)
                .await
                .map_err(|_| (StatusCode::BAD_REQUEST, "InvalidBody"))?;
            let request = orishu_worker::peer::codec::decode(bytes)
                .map_err(|_| (StatusCode::BAD_REQUEST, "InvalidRequest"))?;
            self.runtime
                .inspect_admission(request)
                .await
                .map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "OwnerUnavailable"))
        })
        .await;
        match outcome {
            Ok(Ok(report)) => {
                res.status_code(StatusCode::OK);
                write_api_response(
                    res,
                    &orishu::model::ApiResponse::Ok {
                        data: Some(orishu::model::ResponseData::AdmissionInspection(report)),
                    },
                );
            }
            Ok(Err((status, code))) => api_error(res, status, code),
            Err(_) => api_error(res, StatusCode::GATEWAY_TIMEOUT, "InspectionUnavailable"),
        }
    }
}

struct JoinMaterialHandler {
    runtime: Arc<RunningWorker>,
    capacity: Arc<tokio::sync::Semaphore>,
}
#[handler]
impl JoinMaterialHandler {
    async fn handle(&self, req: &mut Request, res: &mut Response) {
        let outcome = async {
            let mut headers = req.headers().get_all("authorization").iter();
            let authorized = headers
                .next()
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "))
                .is_some_and(|v| self.runtime.authorize_operator(v));
            if !authorized || headers.next().is_some() {
                return Err((
                    StatusCode::UNAUTHORIZED,
                    "Unauthorized",
                    "operator credential required",
                ));
            }
            let unavailable = (
                StatusCode::SERVICE_UNAVAILABLE,
                "JoinMaterialUnavailable",
                "join material unavailable or owner overloaded",
            );
            let _permit = self.capacity.try_acquire().map_err(|_| unavailable)?;
            let receive = self.runtime.join_material().map_err(|_| unavailable)?;
            tokio::time::timeout(std::time::Duration::from_secs(5), receive)
                .await
                .map_err(|_| unavailable)?
                .map_err(|_| unavailable)?
                .ok_or(unavailable)
        }
        .await;
        match outcome {
            Ok(material) => write_api_response(
                res,
                &orishu::model::ApiResponse::Ok {
                    data: Some(orishu::model::ResponseData::JoinMaterial(material)),
                },
            ),
            Err((status, code, message)) => {
                res.status_code(status);
                if status == StatusCode::UNAUTHORIZED {
                    res.add_header("WWW-Authenticate", "Bearer", true)
                        .expect("static header");
                }
                write_api_response(
                    res,
                    &orishu::model::ApiResponse::Error {
                        code: code.into(),
                        message: message.into(),
                    },
                );
            }
        }
    }
}

struct NodeInspectionHandler {
    runtime: Arc<RunningWorker>,
    local: bool,
    capacity: Arc<tokio::sync::Semaphore>,
}

impl NodeInspectionHandler {
    async fn list(&self, req: &mut Request, res: &mut Response) {
        let outcome = async {
            let invalid = (
                StatusCode::BAD_REQUEST,
                "InvalidPage",
                "expected bounded formationId/after cursor",
            );
            if req.uri().query().is_some_and(|q| q.len() > 1024) {
                return Err(invalid);
            }
            if req.queries().iter_all().any(|(key, values)| {
                !matches!(key.as_str(), "formationId" | "after") || values.len() != 1
            }) {
                return Err(invalid);
            }
            let formation = req
                .query::<String>("formationId")
                .map(|v| v.parse())
                .transpose()
                .map_err(|_| invalid)?;
            let after = req
                .query::<String>("after")
                .map(|v| v.parse())
                .transpose()
                .map_err(|_| invalid)?;
            if after.is_some() && formation.is_none() {
                return Err(invalid);
            }
            let unavailable = (
                StatusCode::SERVICE_UNAVAILABLE,
                "OwnerUnavailable",
                "membership read unavailable or overloaded",
            );
            let _permit = self.capacity.try_acquire().map_err(|_| unavailable)?;
            let receive = self
                .runtime
                .list(formation, after)
                .map_err(|_| unavailable)?;
            tokio::time::timeout(std::time::Duration::from_secs(5), receive)
                .await
                .map_err(|_| {
                    (
                        StatusCode::GATEWAY_TIMEOUT,
                        "ReadTimeout",
                        "membership read deadline exceeded",
                    )
                })?
                .map_err(|_| unavailable)?
                .ok_or((
                    StatusCode::PRECONDITION_FAILED,
                    "StaleFormation",
                    "formation changed; restart listing",
                ))
        }
        .await;
        match outcome {
            Ok(page) => {
                res.add_header("X-Node-Id", page.source_node_id.to_string(), true)
                    .expect("validated identity");
                write_api_response(
                    res,
                    &orishu::model::ApiResponse::Ok {
                        data: Some(orishu::model::ResponseData::MembershipPage(page)),
                    },
                );
            }
            Err((status, code, message)) => {
                res.status_code(status);
                write_api_response(
                    res,
                    &orishu::model::ApiResponse::Error {
                        code: code.into(),
                        message: message.into(),
                    },
                );
            }
        }
    }
}

#[handler]
impl NodeInspectionHandler {
    async fn handle(&self, req: &mut Request, res: &mut Response) {
        if !self.local {
            let mut values = req.headers().get_all("authorization").iter();
            let authorized = values
                .next()
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "))
                .is_some_and(|v| self.runtime.authorize_operator(v));
            if !authorized || values.next().is_some() {
                res.status_code(StatusCode::UNAUTHORIZED);
                res.add_header("WWW-Authenticate", "Bearer", true)
                    .expect("static header");
                write_api_response(
                    res,
                    &orishu::model::ApiResponse::Error {
                        code: "Unauthorized".into(),
                        message: "operator credential required".into(),
                    },
                );
                return;
            }
        }
        if req.param::<String>("node").is_none() {
            self.list(req, res).await;
            return;
        }
        let outcome = async {
            if !matches!(req.uri().query(), None | Some("") | Some("source=indirect")) {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "UnsupportedSource",
                    "only indirect membership inspection is supported",
                ));
            }
            let node = req
                .param::<String>("node")
                .and_then(|v| v.parse().ok())
                .ok_or((
                    StatusCode::BAD_REQUEST,
                    "InvalidNodeId",
                    "a valid formation-assigned node ID is required",
                ))?;
            let _permit = self.capacity.try_acquire().map_err(|_| {
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Overloaded",
                    "membership read capacity exhausted",
                )
            })?;
            let receive = self.runtime.inspect(node).map_err(|_| {
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "OwnerUnavailable",
                    "membership owner unavailable or overloaded",
                )
            })?;
            let inspection = tokio::time::timeout(std::time::Duration::from_secs(5), receive)
                .await
                .map_err(|_| {
                    (
                        StatusCode::GATEWAY_TIMEOUT,
                        "ReadTimeout",
                        "membership read deadline exceeded",
                    )
                })?
                .map_err(|_| {
                    (
                        StatusCode::SERVICE_UNAVAILABLE,
                        "OwnerUnavailable",
                        "membership owner stopped",
                    )
                })?
                .ok_or((
                    StatusCode::NOT_FOUND,
                    "UnknownNode",
                    "node is absent from the local formation view",
                ))?;
            Ok(inspection)
        }
        .await;
        match outcome {
            Ok(inspection) => {
                res.add_header("X-Node-Id", inspection.source_node_id.to_string(), true)
                    .expect("validated identity");
                write_api_response(
                    res,
                    &orishu::model::ApiResponse::Ok {
                        data: Some(orishu::model::ResponseData::NodeInspection(inspection)),
                    },
                );
            }
            Err((status, code, message)) => {
                res.status_code(status);
                write_api_response(
                    res,
                    &orishu::model::ApiResponse::Error {
                        code: code.into(),
                        message: message.into(),
                    },
                );
            }
        }
    }
}

#[handler]
impl MembershipMutationHandler {
    async fn handle(&self, req: &mut Request, res: &mut Response) {
        // All mutations, including Unix-socket requests, require this worker's
        // operator token. Never inspect an unauthenticated request body.
        let mut values = req.headers().get_all("authorization").iter();
        let valid = values
            .next()
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .is_some_and(|value| self.runtime.authorize_operator(value));
        if !valid || values.next().is_some() {
            res.add_header("WWW-Authenticate", "Bearer", true)
                .expect("static header");
            api_error(res, StatusCode::UNAUTHORIZED, "Unauthorized");
            return;
        }
        let Ok(_permit) = self.capacity.try_acquire() else {
            api_error(res, StatusCode::SERVICE_UNAVAILABLE, "Overloaded");
            return;
        };
        if req.headers().contains_key("if-match") {
            api_error(res, StatusCode::BAD_REQUEST, "UnsupportedPrecondition");
            return;
        }
        if req.headers().contains_key("content-encoding")
            || req
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok())
                .map(|value| value.split(';').next().unwrap().trim())
                != Some("application/cbor")
        {
            api_error(
                res,
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "UnsupportedMediaType",
            );
            return;
        }
        let result = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let bytes = req
                .payload_with_max_size(4096)
                .await
                .map_err(|_| (StatusCode::BAD_REQUEST, "InvalidBody"))?;
            if matches!(self.kind, MembershipMutation::Leave) {
                let request: orishu::model::cluster::LeaveRequest =
                    orishu_worker::peer::codec::decode(bytes)
                        .map_err(|_| (StatusCode::BAD_REQUEST, "InvalidRequest"))?;
                let receive = self.runtime.leave_operation(request).map_err(|error| {
                    (
                        StatusCode::SERVICE_UNAVAILABLE,
                        if error == orishu_worker::driver::DriverError::Overloaded {
                            "Overloaded"
                        } else {
                            "OwnerUnavailable"
                        },
                    )
                })?;
                return receive
                    .await
                    .map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "OutcomeUnknown"))?
                    .map(orishu::model::ResponseData::LeaveReceipt)
                    .map_err(|error| {
                        use orishu_worker::driver::LeaveError;
                        match error {
                            LeaveError::InvalidSchema => {
                                (StatusCode::BAD_REQUEST, "InvalidRequest")
                            }
                            LeaveError::Conflict => (StatusCode::CONFLICT, "OperationConflict"),
                            LeaveError::HistoryFull => {
                                (StatusCode::SERVICE_UNAVAILABLE, "OperationHistoryFull")
                            }
                            LeaveError::StaleFormation => {
                                (StatusCode::PRECONDITION_FAILED, "StaleFormation")
                            }
                            LeaveError::Unavailable => {
                                (StatusCode::CONFLICT, "ParticipationUnavailable")
                            }
                            LeaveError::Driver(_) => {
                                (StatusCode::SERVICE_UNAVAILABLE, "OutcomeUnknown")
                            }
                        }
                    });
            }
            let request: orishu::model::cluster::LockRequest =
                orishu_worker::peer::codec::decode(bytes)
                    .map_err(|_| (StatusCode::BAD_REQUEST, "InvalidRequest"))?;
            let receive = self.runtime.lock_operation(request).map_err(|error| {
                let code = if error == orishu_worker::driver::DriverError::Overloaded {
                    "Overloaded"
                } else {
                    "OwnerUnavailable"
                };
                (StatusCode::SERVICE_UNAVAILABLE, code)
            })?;
            let outcome = receive
                .await
                .map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "OutcomeUnknown"))?;
            outcome
                .map(orishu::model::ResponseData::LockReceipt)
                .map_err(|error| {
                    use orishu_worker::driver::LockError;
                    match error {
                        LockError::StaleFormation => {
                            (StatusCode::PRECONDITION_FAILED, "StaleFormation")
                        }
                        LockError::Conflict => (StatusCode::CONFLICT, "OperationConflict"),
                        LockError::HistoryFull => {
                            (StatusCode::SERVICE_UNAVAILABLE, "OperationHistoryFull")
                        }
                        LockError::InvalidSchema => (StatusCode::BAD_REQUEST, "InvalidRequest"),
                        LockError::Unavailable => {
                            (StatusCode::CONFLICT, "ParticipationUnavailable")
                        }
                        LockError::Rejected => (StatusCode::CONFLICT, "PolicyRejected"),
                        LockError::Driver(_) => (StatusCode::SERVICE_UNAVAILABLE, "OutcomeUnknown"),
                    }
                })
        })
        .await;
        match result {
            Ok(Ok(receipt)) => {
                if let Ok(summary) = self.runtime.summary() {
                    res.add_header("X-Node-Id", summary.source_node_id.to_string(), true)
                        .expect("validated identity");
                }
                write_api_response(
                    res,
                    &orishu::model::ApiResponse::Ok {
                        data: Some(receipt),
                    },
                );
            }
            Ok(Err((status, code))) => api_error(res, status, code),
            Err(_) => api_error(res, StatusCode::GATEWAY_TIMEOUT, "OutcomeUnknown"),
        }
    }
}

fn api_error(res: &mut Response, status: StatusCode, code: &'static str) {
    res.status_code(status);
    write_api_response(
        res,
        &orishu::model::ApiResponse::Error {
            code: code.into(),
            message: code.into(),
        },
    );
}

// ── Listen address ──────────────────────────────────────────────────────────

/// A listen address: either a TCP/IP socket or a Unix domain socket.
///
/// Accepted formats:
/// - `$XDG_RUNTIME_DIR/orishu/worker.sock` — absolute path → Unix socket
/// - `./custom.sock` — relative path → Unix socket
/// - `unix:/path/to/sock` — explicit scheme → Unix socket
/// - `0.0.0.0:8698` — IPv4 + port → TCP
/// - `[::]:8698` — IPv6 + port → TCP
#[derive(Debug, Clone, PartialEq, Eq)]
enum ListenAddress {
    Tcp(SocketAddr),
    Unix(PathBuf),
}

impl std::fmt::Display for ListenAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tcp(addr) => write!(f, "{addr}"),
            Self::Unix(path) => write!(f, "{}", path.display()),
        }
    }
}

impl std::str::FromStr for ListenAddress {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.starts_with('/') || s.starts_with("./") {
            return Ok(Self::Unix(PathBuf::from(s)));
        }
        if let Some(path) = s.strip_prefix("unix:") {
            return Ok(Self::Unix(PathBuf::from(path)));
        }
        s.parse::<SocketAddr>()
            .map(Self::Tcp)
            .map_err(|e| format!("invalid listen address {s:?}: {e}"))
    }
}

// ── CLI ─────────────────────────────────────────────────────────────────────

/// orishu-worker: the orishu cluster worker daemon.
#[derive(Debug, Parser)]
#[command(name = "orishu-worker")]
struct Cli {
    #[command(flatten)]
    tracing: config::TracingConfig,
    /// Enable the optional loopback process probes and Prometheus health metrics.
    #[arg(long = "observability.enabled", env = "ORISHU_OBSERVABILITY_ENABLED", action = clap::ArgAction::Set)]
    observability_enabled: Option<bool>,
    /// Diagnostics bind (default when enabled: 127.0.0.1:9168; loopback only).
    #[arg(long = "observability.bind", env = "ORISHU_OBSERVABILITY_BIND")]
    observability_bind: Option<SocketAddr>,
    /// Serve /metrics when diagnostics are enabled (default: true).
    #[arg(long = "observability.metrics", env = "ORISHU_OBSERVABILITY_METRICS", action = clap::ArgAction::Set)]
    observability_metrics: Option<bool>,
    /// Serve /livez, /readyz and /startupz when diagnostics are enabled (default: true).
    #[arg(long = "observability.probes", env = "ORISHU_OBSERVABILITY_PROBES", action = clap::ArgAction::Set)]
    observability_probes: Option<bool>,
    /// Development fault build only: discard the first accepted join ACK.
    #[cfg(feature = "formation-fault-test")]
    #[arg(long, hide = true)]
    test_lose_next_join_ack: bool,
    /// Development fault build only: exit after admission insertion, before ACK.
    #[cfg(feature = "formation-fault-test")]
    #[arg(long, hide = true, conflicts_with = "test_lose_next_join_ack")]
    test_crash_after_join: bool,
    /// Development fault build only: remove accepted member before its ACK.
    #[cfg(feature = "formation-fault-test")]
    #[arg(long, hide = true, conflicts_with_all = ["test_lose_next_join_ack", "test_crash_after_join"])]
    test_remove_after_join: bool,
    /// Development fault build only: block accepted certificate before its ACK.
    #[cfg(feature = "formation-fault-test")]
    #[arg(long, hide = true, conflicts_with_all = ["test_lose_next_join_ack", "test_crash_after_join", "test_remove_after_join"])]
    test_block_after_join: bool,
    /// Development Unix fixture: SIGUSR1 ejects the recorded introducer or sole peer.
    #[cfg(feature = "formation-fault-test")]
    #[arg(long, hide = true, conflicts_with_all = ["test_lose_next_join_ack", "test_crash_after_join", "test_remove_after_join", "test_block_after_join"])]
    test_eject_peer_on_signal: bool,
    /// Development fault build only: drop the next voluntary departure datagrams.
    #[cfg(feature = "formation-fault-test")]
    #[arg(long, hide = true, conflicts_with_all = ["test_lose_next_join_ack", "test_crash_after_join", "test_remove_after_join", "test_block_after_join", "test_eject_peer_on_signal"])]
    test_drop_next_departure: bool,
    /// Private per-worker identity directory (must be absolute).
    #[arg(long, env = "ORISHU_STATE_DIR")]
    state_dir: Option<PathBuf>,

    /// Non-unique worker label.
    #[arg(long, env = "ORISHU_WORKER_NAME")]
    name: Option<orishu_membership::WorkerName>,

    /// Label for each fresh standalone formation, not its identity.
    #[arg(long, env = "ORISHU_CLUSTER_NAME")]
    cluster_name: Option<orishu_membership::ClusterName>,
    /// Path to worker config file in the shared YAML format.
    #[arg(long)]
    config: Option<PathBuf>,

    /// Listen addresses (TCP socket or Unix socket path). Repeatable.
    /// Defaults to `$XDG_RUNTIME_DIR/orishu/worker.sock` if not specified.
    #[arg(short, long = "listen.clients")]
    listen: Vec<ListenAddress>,

    /// Explicit QUIC peer bind address (one listener in the current PoC).
    #[arg(long = "listen.peers", env = "ORISHU_LISTEN_PEERS")]
    peer_listen: Option<std::net::SocketAddr>,

    /// Reachable peer address; required for wildcard binds, never port zero.
    #[arg(long = "advertise.peers", env = "ORISHU_ADVERTISE_PEERS")]
    peer_advertise: Option<std::net::SocketAddr>,

    /// Permit introduction when local formation credentials/state are ready.
    #[arg(long = "accepts.peers", env = "ORISHU_ACCEPTS_PEERS", action = clap::ArgAction::Set)]
    accepts_peers: Option<bool>,

    /// Path to TLS certificate PEM file (enables TLS on TCP listeners).
    #[arg(long)]
    tls_cert: Option<PathBuf>,

    /// Path to TLS private key PEM file.
    #[arg(long)]
    tls_key: Option<PathBuf>,
}

// ── Main ────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    // Apply once before any listener/file startup. In particular, there must be
    // no world-connectable interval between Unix bind and explicit chmod.
    #[cfg(unix)]
    rustix::process::umask(rustix::fs::Mode::from_raw_mode(0o077));
    let cli = Cli::parse();
    let runtime = resolve_runtime_config(&cli).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2);
    });
    #[cfg(feature = "formation-fault-test")]
    if (cli.test_lose_next_join_ack
        || cli.test_crash_after_join
        || cli.test_remove_after_join
        || cli.test_block_after_join
        || cli.test_eject_peer_on_signal
        || cli.test_drop_next_departure)
        && (!runtime
            .peer_listen
            .is_some_and(|address| address.ip().is_loopback())
            || !runtime
                .listen
                .iter()
                .all(|address| matches!(address, ListenAddress::Unix(_)))
            || cli.state_dir.is_none())
    {
        eprintln!(
            "formation fault tests require loopback peers, Unix clients and an explicit private state directory"
        );
        std::process::exit(2);
    }

    #[cfg(feature = "otlp-tracing")]
    let tracing = runtime.tracing.prepare().unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(2);
    });

    // Reserve diagnostics before credentials, owner tasks or client sockets
    // exist. Retain the acceptor: probing a port and rebinding would race.
    #[cfg(feature = "observability")]
    let diagnostics_acceptor = if runtime.observability.enabled == Some(true) {
        let bind = runtime
            .observability
            .bind
            .expect("validated diagnostics bind");
        Some(
            TcpListener::new(bind)
                .try_bind()
                .await
                .unwrap_or_else(|error| {
                    eprintln!("cannot bind diagnostics listener {bind}: {error}");
                    std::process::exit(2);
                }),
        )
    } else {
        None
    };

    let state = WorkerCredentials::load_or_create(
        runtime
            .state_dir
            .as_deref()
            .expect("finalized state directory"),
    )
    .map_err(orishu_worker::runtime::StartupError::from)
    .and_then(|credentials| {
        WorkerRuntime::standalone(
            credentials,
            runtime
                .worker_name
                .clone()
                .unwrap_or_else(|| orishu_membership::WorkerName::new("orishu-worker").unwrap()),
            runtime
                .cluster_name
                .clone()
                .unwrap_or_else(|| orishu_membership::ClusterName::new("standalone").unwrap()),
            runtime.listen.iter().map(ToString::to_string).collect(),
        )
    })
    .unwrap_or_else(|error| {
        eprintln!("worker startup failed: {error}");
        std::process::exit(2);
    });
    let state = if let Some(bind) = runtime.peer_listen {
        state
            .bind_peer(bind, runtime.peer_advertise)
            .unwrap_or_else(|error| {
                eprintln!("worker startup failed: {error}");
                std::process::exit(2);
            })
    } else {
        state
    };
    let state = state
        .with_peer_admission(runtime.accepts_peers.unwrap_or(false))
        .unwrap_or_else(|error| {
            eprintln!("worker startup failed: {error}");
            std::process::exit(2);
        });
    #[cfg(feature = "observability")]
    let state = state.with_peer_metrics(
        runtime.observability.enabled == Some(true) && runtime.observability.metrics_enabled(),
    );
    let (state, owner_task) = state.start();
    let state = Arc::new(state);
    #[cfg(feature = "formation-fault-test")]
    if cli.test_drop_next_departure {
        state.test_drop_next_departure().await;
    }
    #[cfg(all(feature = "formation-fault-test", not(unix)))]
    if cli.test_eject_peer_on_signal {
        eprintln!("ejection fixture requires Unix signals");
        std::process::exit(2);
    }
    #[cfg(all(feature = "formation-fault-test", unix))]
    if cli.test_eject_peer_on_signal {
        let mut signal =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::user_defined1())
                .expect("register development fixture signal");
        let fixture_state = Arc::clone(&state);
        tokio::spawn(async move {
            if signal.recv().await.is_some() {
                match fixture_state.test_eject_peer_fixture().await {
                    Ok(node) => println!("FORMATION_TEST_EJECTION_SENT {node}"),
                    Err(_) => eprintln!("FORMATION_TEST_EJECTION_FAILED"),
                }
            }
        });
    }
    #[cfg(feature = "formation-fault-test")]
    if cli.test_crash_after_join {
        state.test_crash_after_next_join().await;
    }
    #[cfg(feature = "formation-fault-test")]
    if cli.test_lose_next_join_ack || cli.test_remove_after_join || cli.test_block_after_join {
        let removing = cli.test_remove_after_join;
        let blocking = cli.test_block_after_join;
        let observed = if blocking {
            state.test_block_after_next_join().await
        } else if removing {
            state.test_remove_after_next_join().await
        } else {
            state.test_lose_next_join_ack().await
        };
        tokio::spawn(async move {
            if let Ok(assigned) = observed.await {
                if blocking {
                    println!("FORMATION_TEST_BLOCKED_ACK {assigned}");
                } else if removing {
                    println!("FORMATION_TEST_REMOVED_ACK {assigned}");
                } else {
                    println!("FORMATION_TEST_LOST_ACK {assigned}");
                }
            }
        });
    }
    state.start_peer_maintenance();
    state.start_catchup_maintenance();

    let tls_config = match (&runtime.tls_cert, &runtime.tls_key) {
        (Some(cert_path), Some(key_path)) => {
            let cert = std::fs::read(cert_path).expect("failed to read TLS certificate");
            let key = std::fs::read(key_path).expect("failed to read TLS private key");
            Some(RustlsConfig::new(Keycert::new().cert(cert).key(key)))
        }
        _ => None,
    };

    let mut server_handles: Vec<ServerHandle> = Vec::new();
    let mut tasks: Vec<JoinHandle<()>> = Vec::new();
    let mutation_capacity = Arc::new(tokio::sync::Semaphore::new(16));
    let inspection_capacity = Arc::new(tokio::sync::Semaphore::new(16));
    #[cfg(feature = "otlp-tracing")]
    let tracing = tracing.map(|(queue, exporter)| {
        let (stop, signal) = tokio::sync::oneshot::channel();
        (
            queue,
            stop,
            exporter.counters(),
            tokio::spawn(exporter.run(signal)),
        )
    });
    #[cfg(feature = "observability")]
    let request_metrics = Arc::new(diagnostics::Requests::default());

    for addr in &runtime.listen {
        let router = Router::new()
            .push(
                Router::with_path("api/v1/membership/admission-inspections").post(
                    AdmissionInspectionHandler {
                        runtime: state.clone(),
                        capacity: inspection_capacity.clone(),
                    },
                ),
            )
            .push(
                Router::with_path("api/v1/membership/leaves").post(MembershipMutationHandler {
                    kind: MembershipMutation::Leave,
                    runtime: state.clone(),
                    capacity: mutation_capacity.clone(),
                }),
            )
            .push(
                Router::with_path("api/v1/membership/joins").post(JoinOperationHandler {
                    runtime: state.clone(),
                    capacity: mutation_capacity.clone(),
                }),
            )
            .push(
                Router::with_path("api/v1/membership/joins/{operation}").get(
                    JoinOperationHandler {
                        runtime: state.clone(),
                        capacity: inspection_capacity.clone(),
                    },
                ),
            )
            .push(
                Router::with_path("api/v1/cluster/token").get(JoinMaterialHandler {
                    runtime: state.clone(),
                    capacity: inspection_capacity.clone(),
                }),
            )
            .push(
                Router::with_path("api/v1/cluster/nodes").get(NodeInspectionHandler {
                    runtime: state.clone(),
                    local: matches!(addr, ListenAddress::Unix(_)),
                    capacity: inspection_capacity.clone(),
                }),
            )
            .push(
                Router::with_path("api/v1/cluster/nodes/{node}").get(NodeInspectionHandler {
                    runtime: state.clone(),
                    local: matches!(addr, ListenAddress::Unix(_)),
                    capacity: inspection_capacity.clone(),
                }),
            )
            .push(
                Router::with_path("api/v1/cluster").get(ClusterSummaryHandler {
                    runtime: state.clone(),
                    local: matches!(addr, ListenAddress::Unix(_)),
                }),
            )
            .push(
                Router::with_path("api/v1/cluster/lock").post(MembershipMutationHandler {
                    kind: MembershipMutation::Lock,
                    runtime: state.clone(),
                    capacity: mutation_capacity.clone(),
                }),
            );
        let service = Service::new(router);
        #[cfg(feature = "otlp-tracing")]
        let service = if let Some((queue, _, _, _)) = &tracing {
            service.hoop(orishu_worker::trace_export::TraceRequests(queue.clone()))
        } else {
            service
        };
        #[cfg(feature = "observability")]
        let service = if runtime.observability.enabled == Some(true)
            && runtime.observability.metrics_enabled()
        {
            service.hoop(diagnostics::Accounting(request_metrics.clone()))
        } else {
            service
        };
        match addr {
            ListenAddress::Tcp(socket_addr) => {
                println!("Listening on TCP socket: {}", socket_addr);
                if let Some(ref tls) = tls_config {
                    let acceptor = TcpListener::new(*socket_addr)
                        .rustls(tls.clone())
                        .bind()
                        .await;
                    let server = client_server(acceptor);
                    server_handles.push(server.handle());
                    let health_role = state.required_role();
                    tasks.push(tokio::spawn(async move {
                        let _health_role = health_role;
                        server.try_serve(service).await.expect("TLS server failed");
                    }));
                } else {
                    let acceptor = TcpListener::new(*socket_addr).bind().await;
                    let server = client_server(acceptor);
                    server_handles.push(server.handle());
                    let health_role = state.required_role();
                    tasks.push(tokio::spawn(async move {
                        let _health_role = health_role;
                        server.try_serve(service).await.expect("TCP server failed");
                    }));
                }
            }
            ListenAddress::Unix(path) => {
                orishu_worker::unix_socket::prepare(path)
                    .await
                    .unwrap_or_else(|error| {
                        eprintln!("{error}");
                        std::process::exit(2);
                    });
                println!("Listening on Unix socket: {}", path.display());

                use std::os::unix::fs::PermissionsExt;
                let acceptor = UnixListener::new(path.clone())
                    .permissions(std::fs::Permissions::from_mode(0o600))
                    .bind()
                    .await;
                let server = client_server(acceptor);
                let lease = orishu_worker::unix_socket::SocketLease::bound(path)
                    .expect("successfully bound socket ownership");
                server_handles.push(server.handle());
                let health_role = state.required_role();
                tasks.push(tokio::spawn(async move {
                    let _health_role = health_role;
                    let _lease = lease;
                    server.try_serve(service).await.expect("Unix server failed");
                }));
            }
        }
    }

    #[cfg(feature = "observability")]
    if let Some(acceptor) = diagnostics_acceptor {
        let server = client_server(acceptor).max_connections(16);
        server_handles.push(server.handle());
        #[cfg(feature = "otlp-tracing")]
        let router = diagnostics::router_with_traces(
            state.clone(),
            &runtime.observability,
            request_metrics,
            tracing.as_ref().map(|(queue, _, counters, _)| {
                diagnostics::Traces(queue.clone(), counters.clone())
            }),
        );
        #[cfg(not(feature = "otlp-tracing"))]
        let router = diagnostics::router(state.clone(), &runtime.observability, request_metrics);
        // Diagnostics failure is not membership/process-role failure. It must
        // not make a healthy worker unready or stop scientific/peer work.
        tasks.push(tokio::spawn(async move {
            if server.try_serve(router).await.is_err() {
                eprintln!("diagnostics listener stopped");
            }
        }));
    }
    state.mark_initialized();
    let supervised_servers = server_handles.clone();
    let supervisor = tokio::spawn(async move {
        let outcome = owner_task.await;
        let failed = !matches!(outcome, Ok(Ok(())));
        if failed {
            eprintln!("membership owner failed; stopping client listeners");
        }
        for handle in supervised_servers {
            handle.stop_graceful(Some(std::time::Duration::from_secs(1)));
        }
        failed
    });
    // The owner supervisor alone stops client servers. Salvo consumes the
    // first stop command before draining connections, so a competing signal
    // handler deadline could prevent the supervisor's bounded drain taking effect.
    tokio::spawn(listen_shutdown_signal(state.clone()));

    for task in tasks {
        let _ = task.await;
    }
    let _ = state.shutdown().await;
    #[cfg(feature = "otlp-tracing")]
    if let Some((queue, stop, _, task)) = tracing {
        let _ = stop.send(());
        match task.await {
            Ok(stats) => eprintln!("trace exporter stopped: {stats:?}"),
            Err(_) => eprintln!("trace exporter task failed"),
        }
        eprintln!("trace queue stopped: {:?}", queue.stats());
    }
    if supervisor.await.unwrap_or(true) {
        std::process::exit(1);
    }
}

// ── Shutdown ────────────────────────────────────────────────────────────────

async fn listen_shutdown_signal(runtime: Arc<RunningWorker>) {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(windows)]
    let terminate = async {
        signal::windows::ctrl_c()
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    tokio::select! {
        _ = ctrl_c => println!("ctrl_c signal received"),
        _ = terminate => println!("terminate signal received"),
    };

    let _ = runtime.shutdown().await;
}

fn resolve_runtime_config(cli: &Cli) -> Result<RuntimeConfig, String> {
    let mut runtime = match cli.config.as_deref() {
        Some(path) => RuntimeConfig::from_file(path)?,
        None => RuntimeConfig::default(),
    };

    runtime.apply_cli(&cli.listen, cli.tls_cert.as_deref(), cli.tls_key.as_deref());
    runtime.tracing.overlay(&cli.tracing);
    if let Some(enabled) = cli.observability_enabled {
        runtime.observability.enabled = Some(enabled);
    }
    if let Some(bind) = cli.observability_bind {
        runtime.observability.bind = Some(bind);
    }
    if let Some(metrics) = cli.observability_metrics {
        runtime.observability.metrics = Some(metrics);
    }
    if let Some(probes) = cli.observability_probes {
        runtime.observability.probes = Some(probes);
    }
    if let Some(enabled) = cli.accepts_peers {
        runtime.accepts_peers = Some(enabled);
    }
    if let Some(bind) = cli.peer_listen {
        runtime.peer_listen = Some(bind);
    }
    if let Some(advertise) = cli.peer_advertise {
        runtime.peer_advertise = Some(advertise);
    }
    if let Some(path) = &cli.state_dir {
        runtime.state_dir = Some(path.clone());
    }
    if let Some(name) = &cli.name {
        runtime.worker_name = Some(name.clone());
    }
    if let Some(name) = &cli.cluster_name {
        runtime.cluster_name = Some(name.clone());
    }
    runtime.finalize()
}

#[cfg(test)]
mod fault_switch_tests {
    use super::*;

    #[test]
    fn departure_loss_is_test_only_and_excludes_other_faults() {
        let parsed = Cli::try_parse_from(["orishu-worker", "--test-drop-next-departure"]);
        #[cfg(feature = "formation-fault-test")]
        {
            assert!(parsed.unwrap().test_drop_next_departure);
            for other in [
                "--test-lose-next-join-ack",
                "--test-crash-after-join",
                "--test-remove-after-join",
                "--test-block-after-join",
                "--test-eject-peer-on-signal",
            ] {
                assert_eq!(
                    Cli::try_parse_from(["orishu-worker", "--test-drop-next-departure", other])
                        .unwrap_err()
                        .kind(),
                    clap::error::ErrorKind::ArgumentConflict
                );
            }
        }
        #[cfg(not(feature = "formation-fault-test"))]
        assert_eq!(
            parsed.unwrap_err().kind(),
            clap::error::ErrorKind::UnknownArgument
        );
    }

    #[test]
    fn ejection_fixture_is_test_only_and_excludes_other_faults() {
        let parsed = Cli::try_parse_from(["orishu-worker", "--test-eject-peer-on-signal"]);
        #[cfg(feature = "formation-fault-test")]
        {
            assert!(parsed.unwrap().test_eject_peer_on_signal);
            for other in [
                "--test-lose-next-join-ack",
                "--test-crash-after-join",
                "--test-remove-after-join",
                "--test-block-after-join",
            ] {
                assert_eq!(
                    Cli::try_parse_from(["orishu-worker", "--test-eject-peer-on-signal", other])
                        .unwrap_err()
                        .kind(),
                    clap::error::ErrorKind::ArgumentConflict
                );
            }
        }
        #[cfg(not(feature = "formation-fault-test"))]
        assert_eq!(
            parsed.unwrap_err().kind(),
            clap::error::ErrorKind::UnknownArgument
        );
    }

    #[test]
    fn block_switch_is_test_only_and_excludes_other_faults() {
        let parsed = Cli::try_parse_from(["orishu-worker", "--test-block-after-join"]);
        #[cfg(feature = "formation-fault-test")]
        {
            assert!(parsed.unwrap().test_block_after_join);
            for other in [
                "--test-lose-next-join-ack",
                "--test-crash-after-join",
                "--test-remove-after-join",
            ] {
                assert_eq!(
                    Cli::try_parse_from(["orishu-worker", "--test-block-after-join", other])
                        .unwrap_err()
                        .kind(),
                    clap::error::ErrorKind::ArgumentConflict
                );
            }
        }
        #[cfg(not(feature = "formation-fault-test"))]
        assert_eq!(
            parsed.unwrap_err().kind(),
            clap::error::ErrorKind::UnknownArgument
        );
    }

    #[test]
    fn removal_switch_is_test_only_and_excludes_other_faults() {
        let parsed = Cli::try_parse_from(["orishu-worker", "--test-remove-after-join"]);
        #[cfg(feature = "formation-fault-test")]
        {
            assert!(parsed.unwrap().test_remove_after_join);
            for other in ["--test-lose-next-join-ack", "--test-crash-after-join"] {
                assert_eq!(
                    Cli::try_parse_from(["orishu-worker", "--test-remove-after-join", other])
                        .unwrap_err()
                        .kind(),
                    clap::error::ErrorKind::ArgumentConflict
                );
            }
        }
        #[cfg(not(feature = "formation-fault-test"))]
        assert_eq!(
            parsed.unwrap_err().kind(),
            clap::error::ErrorKind::UnknownArgument
        );
    }

    #[test]
    fn fault_switch_requires_explicit_test_build() {
        let parsed = Cli::try_parse_from(["orishu-worker", "--test-lose-next-join-ack"]);
        #[cfg(feature = "formation-fault-test")]
        assert!(parsed.unwrap().test_lose_next_join_ack);
        #[cfg(not(feature = "formation-fault-test"))]
        assert_eq!(
            parsed.unwrap_err().kind(),
            clap::error::ErrorKind::UnknownArgument
        );
    }

    #[test]
    fn issuer_crash_switch_requires_test_build_and_excludes_ack_loss_switch() {
        let parsed = Cli::try_parse_from(["orishu-worker", "--test-crash-after-join"]);
        #[cfg(feature = "formation-fault-test")]
        {
            assert!(parsed.unwrap().test_crash_after_join);
            assert_eq!(
                Cli::try_parse_from([
                    "orishu-worker",
                    "--test-crash-after-join",
                    "--test-lose-next-join-ack"
                ])
                .unwrap_err()
                .kind(),
                clap::error::ErrorKind::ArgumentConflict
            );
        }
        #[cfg(not(feature = "formation-fault-test"))]
        assert_eq!(
            parsed.unwrap_err().kind(),
            clap::error::ErrorKind::UnknownArgument
        );
    }
}

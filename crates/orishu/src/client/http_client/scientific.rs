//! Client adapter for the opt-in scientific-load HTTP profile. No provider
//! resolution, implicit lock, redirect, polling loop or fresh-ID retry.
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::{StatusCode, header};
use serde::Deserialize;

use super::HttpClusterClient;
use crate::model::{
    API_ROUTE_RETAINED_RUN, API_ROUTE_RUN_LOAD_LOOKUP, API_ROUTE_RUN_LOADS,
    run::RunDescriptor,
    run_load::{LoadReceipt, LoadRequest, LoadState, MAX_LOAD_REQUEST_BYTES, RUN_LOAD_MEDIA_TYPE},
};

mod codec;

const MAX_BUNDLE_BYTES: usize = 128 * 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 16 * 1024;
const RESPONSE_BODY_TIMEOUT: Duration = Duration::from_secs(5);
const READ_TIMEOUT: Duration = Duration::from_secs(10);
const SUBMIT_TIMEOUT: Duration = Duration::from_secs(70);

/// Bounded client failures. An error after sending a submission is never proof
/// of rollback: reconcile the original request at the same worker by lookup.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ScientificError {
    /// Local input could not be sent; no network request was started.
    #[error("invalid scientific request: {0}")]
    Input(&'static str),
    /// Transport failed; no unbounded diagnostic or credential is retained.
    #[error("scientific transport failed; operation outcome may be unknown")]
    Transport,
    /// The absolute request or response-body budget expired.
    #[error("scientific request deadline exceeded; operation outcome may be unknown")]
    Deadline,
    /// An untrusted server reply violated framing, schema or correlation.
    #[error("invalid scientific response: {0}")]
    Protocol(&'static str),
    /// A bounded typed HTTP refusal, not an accepted scientific outcome.
    #[error("scientific HTTP {status}: {code}: {message}")]
    Http {
        /// Actual HTTP response status.
        status: u16,
        /// Server's bounded machine-readable code.
        code: String,
        /// Server's bounded diagnostic; callers must treat it as untrusted text.
        message: String,
    },
}

/// Borrow the configured worker transport. Every method requires its operator
/// credential, including local reads. Request lifetime does not own remote runs.
pub struct ScientificClient<'a> {
    host: &'a HttpClusterClient,
}
impl<'a> ScientificClient<'a> {
    pub(super) fn new(host: &'a HttpClusterClient) -> Self {
        Self { host }
    }

    /// Submit one complete portable bundle for exactly this identified intent.
    /// The caller supplies the immutable root and operation ID; this method never
    /// invents them, verifies plugins, changes membership or retries. A returned
    /// Pending receipt is not acceptance. Refused/Indeterminate remain outcomes.
    ///
    /// Takes ownership of the bounded bundle without concatenating/copying it:
    /// memory is O(bundle bytes + bounded metadata), not a second upload-sized
    /// buffer. The worker independently verifies the complete selected closure.
    pub async fn submit(
        &self,
        intent: &LoadRequest,
        bundle: Vec<u8>,
    ) -> Result<LoadReceipt, ScientificError> {
        if bundle.is_empty() || bundle.len() > MAX_BUNDLE_BYTES {
            return Err(ScientificError::Input("bundle size"));
        }
        let metadata = request_bytes(intent)?;
        let mut prefix = Vec::with_capacity(metadata.len() + 4);
        prefix.extend_from_slice(&(metadata.len() as u32).to_be_bytes());
        prefix.extend_from_slice(&metadata);
        let length = prefix.len() + bundle.len();
        let body = reqwest::Body::wrap_stream(futures_util::stream::iter([
            Ok::<_, std::io::Error>(prefix),
            Ok(bundle),
        ]));
        let request = self
            .host
            .client
            .post(self.url(&API_ROUTE_RUN_LOADS)?)
            .header(header::CONTENT_TYPE, RUN_LOAD_MEDIA_TYPE)
            .header(header::ACCEPT, "application/cbor")
            .header(header::CONTENT_LENGTH, length)
            .body(body);
        let (status, data) = exchange(request, SUBMIT_TIMEOUT).await?;
        let Data::RunLoadReceipt(receipt) = data else {
            return Err(ScientificError::Protocol("expected load receipt"));
        };
        correlate(intent, &receipt)?;
        if status != StatusCode::OK
            && !(status == StatusCode::ACCEPTED && receipt.state() == &LoadState::Pending)
        {
            return Err(ScientificError::Protocol("invalid receipt status"));
        }
        Ok(receipt)
    }

    /// Query the exact original operation, including after a lost upload reply
    /// or process restart. Only 404/OperationNotFound means no recorded receipt;
    /// a disabled/missing API, conflict or unhealthy journal remains an error.
    pub async fn lookup(
        &self,
        intent: &LoadRequest,
    ) -> Result<Option<LoadReceipt>, ScientificError> {
        let bytes = request_bytes(intent)?;
        let request = self
            .host
            .client
            .post(self.url(&API_ROUTE_RUN_LOAD_LOOKUP)?)
            .header(header::CONTENT_TYPE, "application/cbor")
            .header(header::ACCEPT, "application/cbor")
            .header(header::CONTENT_LENGTH, bytes.len())
            .body(bytes);
        let result = exchange(request, READ_TIMEOUT).await;
        match result {
            Err(ScientificError::Http {
                status: 404, code, ..
            }) if code == "OperationNotFound" => Ok(None),
            Ok((StatusCode::OK, Data::RunLoadReceipt(receipt))) => {
                correlate(intent, &receipt)?;
                Ok(Some(receipt))
            }
            Ok(_) => Err(ScientificError::Protocol("expected lookup receipt")),
            Err(error) => Err(error),
        }
    }

    /// Discover the worker's currently usable retained descriptor, independently
    /// of historical receipt health. It need not match a previously loaded run:
    /// callers must explicitly compare identity before issuing later controls.
    pub async fn current(&self) -> Result<Option<RunDescriptor>, ScientificError> {
        let request = self
            .host
            .client
            .get(self.url(&API_ROUTE_RETAINED_RUN)?)
            .header(header::ACCEPT, "application/cbor");
        match exchange(request, READ_TIMEOUT).await? {
            (StatusCode::OK, Data::RetainedRun(run)) => {
                if run
                    .as_ref()
                    .is_some_and(|run| run.identity().workload_epoch().get() == 0)
                {
                    return Err(ScientificError::Protocol("unallocated run epoch"));
                }
                Ok(run)
            }
            _ => Err(ScientificError::Protocol("expected retained descriptor")),
        }
    }

    fn url(&self, route: &crate::model::ApiRoute) -> Result<reqwest::Url, ScientificError> {
        self.host
            .base_url
            .join(&route.to_string())
            .map_err(|_| ScientificError::Input("invalid worker URL"))
    }
}

fn request_bytes(request: &LoadRequest) -> Result<Vec<u8>, ScientificError> {
    // Every field has a typed small bound; no arbitrary collections are encoded.
    let mut bytes = Vec::new();
    ciborium::into_writer(request, &mut bytes)
        .map_err(|_| ScientificError::Input("request encoding"))?;
    if bytes.len() > MAX_LOAD_REQUEST_BYTES {
        return Err(ScientificError::Input("request size"));
    }
    Ok(bytes)
}

fn correlate(intent: &LoadRequest, receipt: &LoadReceipt) -> Result<(), ScientificError> {
    if receipt.request() != intent {
        return Err(ScientificError::Protocol(
            "receipt belongs to another request",
        ));
    }
    // LoadReceipt deserialization also checks the accepted descriptor against
    // its embedded request, including nonzero epoch. Source node is historical,
    // so it must not be compared to a new incarnation's current node identity.
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
enum Reply {
    Ok { data: Data },
    Error { code: String, message: String },
}
#[derive(Deserialize)]
enum Data {
    RunLoadReceipt(LoadReceipt),
    RetainedRun(Option<RunDescriptor>),
}

async fn exchange(
    request: reqwest_middleware::RequestBuilder,
    budget: Duration,
) -> Result<(StatusCode, Data), ScientificError> {
    tokio::time::timeout(budget, async {
        let response = request
            .send()
            .await
            .map_err(|_| ScientificError::Transport)?;
        let status = response.status();
        let headers = response.headers();
        let mut content_type = headers.get_all(header::CONTENT_TYPE).iter();
        if content_type.next().and_then(|value| value.to_str().ok()) != Some("application/cbor")
            || content_type.next().is_some()
            || headers.contains_key(header::CONTENT_ENCODING)
        {
            return Err(ScientificError::Protocol("response media type"));
        }
        if response
            .content_length()
            .is_some_and(|size| size > MAX_RESPONSE_BYTES as u64)
        {
            return Err(ScientificError::Protocol("response byte limit"));
        }
        let bytes = tokio::time::timeout(RESPONSE_BODY_TIMEOUT, async {
            let mut bytes = Vec::new();
            let mut stream = response.bytes_stream();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|_| ScientificError::Transport)?;
                if chunk.len() > MAX_RESPONSE_BYTES - bytes.len() {
                    return Err(ScientificError::Protocol("response byte limit"));
                }
                bytes.extend_from_slice(&chunk);
            }
            Ok(bytes)
        })
        .await
        .map_err(|_| ScientificError::Deadline)??;
        codec::validate(&bytes)
            .map_err(|_| ScientificError::Protocol("response CBOR limits/shape"))?;
        let reply: Reply = ciborium::from_reader(&bytes[..])
            .map_err(|_| ScientificError::Protocol("response schema"))?;
        match reply {
            Reply::Ok { data } if status.is_success() => Ok((status, data)),
            Reply::Error { code, message }
                if status.is_client_error() || status.is_server_error() =>
            {
                Err(ScientificError::Http {
                    status: status.as_u16(),
                    code,
                    message,
                })
            }
            _ => Err(ScientificError::Protocol(
                "response envelope/status mismatch",
            )),
        }
    })
    .await
    .map_err(|_| ScientificError::Deadline)?
}

#[cfg(test)]
mod tests;

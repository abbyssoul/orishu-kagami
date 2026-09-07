//! Single-flight OTLP/HTTP delivery. Never reads ambient exporter credentials.
use super::EncodedBatch;
use crate::trace_destination::TraceEndpoint;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceResponse;
use prost::Message;
use reqwest::{
    Client, Url,
    header::{AUTHORIZATION, CONTENT_ENCODING, CONTENT_TYPE, HeaderValue},
};
use std::time::Duration;

/// Fixed diagnostics: collector URLs, response bodies and tokens are not echoed.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DeliveryError {
    /// Endpoint, TLS, token or limits do not satisfy the collector contract.
    #[error("invalid trace collector transport configuration")]
    Configuration,
    /// The whole attempt exhausted its time budget.
    #[error("trace collector attempt timed out")]
    Timeout,
    /// DNS, TLS or HTTP IO failed; remote details are deliberately omitted.
    #[error("trace collector transport failed")]
    Transport,
    /// Non-200 response; no redirect or implicit retry follows it.
    #[error("trace collector returned HTTP {0}")]
    HttpStatus(u16),
    /// Decoded body would exceed the configured cap.
    #[error("trace collector response exceeds byte budget")]
    ResponseTooLarge,
    /// Unsupported media/encoding or invalid protobuf/partial-success counts.
    #[error("invalid trace collector response")]
    InvalidResponse,
}

/// Collector-reported result, not scientific or command authority.
#[derive(Debug, PartialEq, Eq)]
pub struct DeliveryOutcome {
    /// Number of spans the response says were rejected.
    pub rejected: usize,
    /// Collector supplied a warning; its untrusted text is not retained.
    pub warning: bool,
}

/// Explicit collector connection. Mutable delivery enforces one attempt at a time
/// per instance; the owning export task must also bound its queue and shutdown.
pub struct HttpDelivery {
    client: Client,
    endpoint: Url,
    authorization: Option<HeaderValue>,
    timeout: Duration,
    response_bytes: usize,
}

impl HttpDelivery {
    /// Build without connecting. HTTPS requires an explicitly prepared rustls
    /// config; plaintext is limited to literal loopback and cannot carry a token.
    /// File loading and trust-root selection belong to worker startup, not here.
    pub fn new(
        endpoint: &TraceEndpoint,
        tls: Option<rustls::ClientConfig>,
        bearer: Option<&str>,
        timeout: Duration,
        response_bytes: usize,
    ) -> Result<Self, DeliveryError> {
        let uri = endpoint
            .validated_uri()
            .map_err(|_| DeliveryError::Configuration)?;
        let https = uri.scheme_str() == Some("https");
        if https != tls.is_some()
            || (!https && bearer.is_some())
            || !(Duration::from_millis(100)..=Duration::from_secs(10)).contains(&timeout)
            || !(1024..=65536).contains(&response_bytes)
        {
            return Err(DeliveryError::Configuration);
        }
        let endpoint = Url::parse(&uri.to_string()).map_err(|_| DeliveryError::Configuration)?;
        let authorization = bearer
            .map(|token| {
                if token.is_empty()
                    || token.len() > 4096
                    || !token
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"-._~+/=".contains(&byte))
                {
                    return Err(DeliveryError::Configuration);
                }
                let mut value = HeaderValue::from_str(&format!("Bearer {token}"))
                    .map_err(|_| DeliveryError::Configuration)?;
                value.set_sensitive(true);
                Ok(value)
            })
            .transpose()?;
        let mut builder = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .http1_only()
            .pool_max_idle_per_host(0)
            .no_gzip()
            .no_brotli()
            .no_deflate()
            .no_zstd()
            .timeout(timeout)
            .connect_timeout(timeout);
        if let Some(mut tls) = tls {
            tls.enable_early_data = false;
            tls.key_log = std::sync::Arc::new(rustls::NoKeyLog {});
            builder = builder.tls_backend_preconfigured(tls);
        }
        let client = builder.build().map_err(|_| DeliveryError::Configuration)?;
        Ok(Self {
            client,
            endpoint,
            authorization,
            timeout,
            response_bytes,
        })
    }

    /// Send one already bounded request with no implicit retry. The deadline
    /// includes connection, headers and streaming body. An empty batch does no IO.
    /// Shutdown must cap this future with the owner's remaining global deadline.
    pub async fn deliver(&mut self, batch: EncodedBatch) -> Result<DeliveryOutcome, DeliveryError> {
        if batch.consumed == 0 {
            return Ok(DeliveryOutcome {
                rejected: 0,
                warning: false,
            });
        }
        tokio::time::timeout(self.timeout, self.attempt(batch))
            .await
            .map_err(|_| DeliveryError::Timeout)?
    }

    async fn attempt(&self, batch: EncodedBatch) -> Result<DeliveryOutcome, DeliveryError> {
        let mut request = self
            .client
            .post(self.endpoint.clone())
            .header(CONTENT_TYPE, "application/x-protobuf")
            .body(batch.body);
        if let Some(value) = &self.authorization {
            request = request.header(AUTHORIZATION, value);
        }
        let mut response = request.send().await.map_err(transport_error)?;
        if response.status().as_u16() != 200 {
            return Err(DeliveryError::HttpStatus(response.status().as_u16()));
        }
        if response.headers().get(CONTENT_ENCODING).is_some()
            || response
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                != Some("application/x-protobuf")
        {
            return Err(DeliveryError::InvalidResponse);
        }
        if response
            .content_length()
            .is_some_and(|size| size > self.response_bytes as u64)
        {
            return Err(DeliveryError::ResponseTooLarge);
        }
        let mut body = Vec::with_capacity(self.response_bytes);
        while let Some(chunk) = response.chunk().await.map_err(transport_error)? {
            if chunk.len() > self.response_bytes - body.len() {
                return Err(DeliveryError::ResponseTooLarge);
            }
            body.extend_from_slice(&chunk);
        }
        let reply = ExportTraceServiceResponse::decode(body.as_slice())
            .map_err(|_| DeliveryError::InvalidResponse)?;
        let partial = reply.partial_success.unwrap_or_default();
        let rejected =
            usize::try_from(partial.rejected_spans).map_err(|_| DeliveryError::InvalidResponse)?;
        if rejected > batch.consumed {
            return Err(DeliveryError::InvalidResponse);
        }
        Ok(DeliveryOutcome {
            rejected,
            warning: !partial.error_message.is_empty(),
        })
    }
}

fn transport_error(error: reqwest::Error) -> DeliveryError {
    if error.is_timeout() {
        DeliveryError::Timeout
    } else {
        DeliveryError::Transport
    }
}

#[cfg(test)]
mod tls_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace_export::{BatchLimits, Operation, Outcome, SpanRecord};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    pub(super) fn batch() -> EncodedBatch {
        let record = SpanRecord::new(
            [1; 16],
            [2; 8],
            None,
            Operation::ClientRequest,
            1,
            2,
            Outcome::Completed,
        )
        .unwrap();
        BatchLimits::new(1, 1024)
            .unwrap()
            .encode(&[record])
            .unwrap()
    }

    async fn exchange(response: &[u8], stall: bool) -> Result<DeliveryOutcome, DeliveryError> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let response = response.to_vec();
        let endpoint: TraceEndpoint = format!("http://{address}/custom/traces").parse().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut head = Vec::new();
            while !head.ends_with(b"\r\n\r\n") {
                assert!(head.len() < 4096);
                head.push(stream.read_u8().await.unwrap());
            }
            let head = String::from_utf8(head).unwrap().to_ascii_lowercase();
            assert!(head.starts_with("post /custom/traces http/1.1\r\n"));
            assert!(head.contains("content-type: application/x-protobuf\r\n"));
            assert!(!head.contains("authorization:") && !head.contains("accept-encoding:"));
            assert!(!head.contains("ambient-secret-marker"));
            let length: usize = head
                .lines()
                .find_map(|line| line.strip_prefix("content-length: "))
                .unwrap()
                .parse()
                .unwrap();
            assert!(length <= 1024);
            let mut body = vec![0; length];
            stream.read_exact(&mut body).await.unwrap();
            let decoded = super::super::ExportTraceServiceRequest::decode(body.as_slice()).unwrap();
            assert_eq!(
                decoded.resource_spans[0].scope_spans[0].spans[0].trace_id,
                vec![1; 16]
            );
            stream.write_all(&response).await.unwrap();
            if stall {
                std::future::pending::<()>().await;
            }
        });
        let mut delivery =
            HttpDelivery::new(&endpoint, None, None, Duration::from_millis(100), 1024).unwrap();
        let result = delivery.deliver(batch()).await;
        if stall {
            server.abort();
            assert!(server.await.unwrap_err().is_cancelled());
        } else {
            tokio::time::timeout(Duration::from_secs(2), server)
                .await
                .unwrap()
                .unwrap();
        }
        result
    }

    #[tokio::test]
    async fn real_http_receives_protobuf_and_bounded_response_outcomes() {
        assert_eq!(exchange(b"HTTP/1.1 200 OK\r\nContent-Type: application/x-protobuf\r\nContent-Length: 0\r\n\r\n", false).await.unwrap(),
            DeliveryOutcome { rejected: 0, warning: false });
        assert_eq!(exchange(b"HTTP/1.1 307 Temporary Redirect\r\nLocation: http://127.0.0.1:1/leak\r\nContent-Length: 0\r\n\r\n", false).await,
            Err(DeliveryError::HttpStatus(307)));
        assert_eq!(exchange(b"HTTP/1.1 200 OK\r\nContent-Type: application/x-protobuf\r\nContent-Length: 1025\r\n\r\n", false).await,
            Err(DeliveryError::ResponseTooLarge));
        assert_eq!(exchange(b"HTTP/1.1 200 OK\r\nContent-Type: application/x-protobuf\r\nTransfer-Encoding: chunked\r\n\r\n", true).await,
            Err(DeliveryError::Timeout));
        assert_eq!(exchange(b"HTTP/1.1 200 OK\r\nContent-Type: application/x-protobuf\r\nContent-Length: 1\r\n\r\n\xff", false).await,
            Err(DeliveryError::InvalidResponse));
    }

    #[tokio::test]
    async fn chunked_byte_bound_and_partial_success_are_checked() {
        for (size, result) in [
            (
                1024,
                Ok(DeliveryOutcome {
                    rejected: 0,
                    warning: false,
                }),
            ),
            (1025, Err(DeliveryError::ResponseTooLarge)),
        ] {
            // Legal unknown protobuf field, without a Content-Length to trust.
            let mut body = vec![
                0x7a,
                ((size - 3) & 0x7f) as u8 | 0x80,
                ((size - 3) >> 7) as u8,
            ];
            body.resize(size, 0);
            let mut response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/x-protobuf\r\nTransfer-Encoding: chunked\r\n\r\n{size:x}\r\n").into_bytes();
            response.extend_from_slice(&body);
            response.extend_from_slice(b"\r\n0\r\n\r\n");
            assert_eq!(exchange(&response, false).await, result);
        }
        for (rejected, result) in [
            (
                0,
                Ok(DeliveryOutcome {
                    rejected: 0,
                    warning: true,
                }),
            ),
            (
                1,
                Ok(DeliveryOutcome {
                    rejected: 1,
                    warning: true,
                }),
            ),
            (2, Err(DeliveryError::InvalidResponse)),
            (-1, Err(DeliveryError::InvalidResponse)),
        ] {
            let body = ExportTraceServiceResponse {
                partial_success: Some(
                    opentelemetry_proto::tonic::collector::trace::v1::ExportTracePartialSuccess {
                        rejected_spans: rejected,
                        error_message: "untrusted-secret-marker".into(),
                    },
                ),
            }
            .encode_to_vec();
            let mut response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/x-protobuf\r\nContent-Length: {}\r\n\r\n", body.len()).into_bytes();
            response.extend_from_slice(&body);
            let actual = exchange(&response, false).await;
            assert!(!format!("{actual:?}").contains("untrusted-secret-marker"));
            assert_eq!(actual, result);
        }
        assert_eq!(exchange(b"HTTP/1.1 200 OK\r\nContent-Type: application/x-protobuf\r\nContent-Encoding: gzip\r\nContent-Length: 0\r\n\r\n", false).await,
            Err(DeliveryError::InvalidResponse));
    }

    #[test]
    fn insecure_credentials_and_missing_tls_are_configuration_errors() {
        let local = "http://127.0.0.1:4318/v1/traces".parse().unwrap();
        let remote = "https://collector.example/v1/traces".parse().unwrap();
        assert!(matches!(
            HttpDelivery::new(&local, None, Some("secret"), Duration::from_secs(1), 1024),
            Err(DeliveryError::Configuration)
        ));
        assert!(matches!(
            HttpDelivery::new(&remote, None, None, Duration::from_secs(1), 1024),
            Err(DeliveryError::Configuration)
        ));
    }

    #[tokio::test]
    async fn ambient_exporter_headers_and_proxies_do_not_change_delivery() {
        // Isolate process environment: Rust tests share an address space, so never
        // set/remove global variables in this process to exercise configuration.
        let mut child = tokio::process::Command::new(std::env::current_exe().unwrap());
        child
            .arg("--exact")
            .arg("trace_export::http::tests::real_http_receives_protobuf_and_bounded_response_outcomes")
            .env("OTEL_EXPORTER_OTLP_ENDPOINT", "http://127.0.0.1:1/ambient-secret-marker")
            .env("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT", "http://127.0.0.1:1/ambient-secret-marker")
            .env("OTEL_EXPORTER_OTLP_HEADERS", "authorization=Bearer ambient-secret-marker")
            .env("OTEL_EXPORTER_OTLP_TRACES_HEADERS", "x-injected=ambient-secret-marker")
            .env("HTTP_PROXY", "http://127.0.0.1:1")
            .env("HTTPS_PROXY", "http://127.0.0.1:1")
            .env("ALL_PROXY", "http://127.0.0.1:1")
            .env("http_proxy", "http://127.0.0.1:1")
            .env("https_proxy", "http://127.0.0.1:1")
            .env("all_proxy", "http://127.0.0.1:1")
            .env("NO_PROXY", "")
            .env("no_proxy", "")
            .kill_on_drop(true);
        let mut child = child.spawn().unwrap();
        match tokio::time::timeout(Duration::from_secs(5), child.wait()).await {
            Ok(status) => assert!(status.unwrap().success()),
            Err(_) => {
                child.kill().await.unwrap();
                child.wait().await.unwrap();
                panic!("isolated exporter-environment check exceeded deadline");
            }
        }
    }
}

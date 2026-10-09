//! Independent bounded observation acquisition, encoding and response delivery.
use super::*;
use orishu::model::run_observation::*;
use orishu_plugin::{
    ArtifactDigest,
    execution::{FieldObservation, SAMPLE_REQUEST_SCHEMA, SampleRequest, SampleScratch},
};
use orishu_runtime::{Buffer, OperationControl};
use orishu_worker::driver::run::RunError;

// Every chunk retains the allocation AND its budgets, including a final chunk
// already buffered by HTTP after the producer task has finished.
struct EncodedObservation<G, B = Vec<u8>> {
    bytes: B,
    _guards: G,
}
impl<G, B: AsRef<[u8]>> AsRef<[u8]> for EncodedObservation<G, B> {
    fn as_ref(&self) -> &[u8] {
        self.bytes.as_ref()
    }
}

pub(super) struct ObjectHandler {
    pub(super) runtime: Arc<RunningWorker>,
    pub(super) capacity: Arc<tokio::sync::Semaphore>,
}

/// The two field operations share the object observer pool, never command slots.
pub(super) struct FieldHandler {
    pub(super) runtime: Arc<RunningWorker>,
    pub(super) capacity: Arc<tokio::sync::Semaphore>,
    pub(super) sample: bool,
}

#[handler]
impl FieldHandler {
    async fn handle(&self, req: &mut Request, res: &mut Response) {
        let valid = header(req, "authorization")
            .ok()
            .flatten()
            .and_then(|value| value.strip_prefix("Bearer "))
            .is_some_and(|token| self.runtime.authorize_operator(token));
        if !valid {
            res.add_header("WWW-Authenticate", "Bearer", true)
                .expect("static header");
            super::super::api_error(res, StatusCode::UNAUTHORIZED, "Unauthorized");
            return;
        }
        let Ok(permit) = self.capacity.clone().try_acquire_owned() else {
            super::super::api_error(res, StatusCode::SERVICE_UNAVAILABLE, "ObserverBusy");
            return;
        };
        let result = async {
            if req.uri().query().is_some()
                || [
                    "if-match",
                    "content-encoding",
                    "transfer-encoding",
                    "trailer",
                ]
                .iter()
                .any(|name| req.headers().contains_key(*name))
            {
                return Err((StatusCode::BAD_REQUEST, "UnsupportedFraming"));
            }
            let media = if self.sample {
                FIELD_SAMPLE_REQUEST_MEDIA_TYPE
            } else {
                "application/cbor"
            };
            if header(req, "content-type")? != Some(media) {
                return Err((StatusCode::UNSUPPORTED_MEDIA_TYPE, "UnsupportedMediaType"));
            }
            let maximum = MAX_OBJECT_REQUEST_BYTES
                + if self.sample {
                    4 + MAX_FIELD_SAMPLE_BYTES
                } else {
                    0
                };
            let total = length(req, maximum as u64)?;
            let mut body = HttpBody::new(req.take_body());
            let (intent, query): (FieldObservationRequest, Option<Vec<u8>>) = if self.sample {
                tokio::time::timeout(METADATA_TIMEOUT, async {
                    if total < 4 {
                        return Err((StatusCode::BAD_REQUEST, "InvalidLength"));
                    }
                    let size = body
                        .read_u32()
                        .await
                        .map_err(|_| (StatusCode::BAD_REQUEST, "InvalidBody"))?
                        as u64;
                    if size == 0
                        || size > MAX_OBJECT_REQUEST_BYTES as u64
                        || size + 4 >= total
                        || total - size - 4 > MAX_FIELD_SAMPLE_BYTES as u64
                    {
                        return Err((StatusCode::BAD_REQUEST, "InvalidLength"));
                    }
                    let mut bytes = vec![0; size as usize];
                    body.read_exact(&mut bytes)
                        .await
                        .map_err(|_| (StatusCode::BAD_REQUEST, "InvalidBody"))?;
                    let intent = orishu_worker::peer::codec::decode(&bytes)
                        .map_err(|_| (StatusCode::BAD_REQUEST, "InvalidRequest"))?;
                    let mut query = Vec::new();
                    body.take(total - size - 4 + 1)
                        .read_to_end(&mut query)
                        .await
                        .map_err(|_| (StatusCode::BAD_REQUEST, "InvalidBody"))?;
                    if query.len() as u64 != total - size - 4 {
                        return Err((StatusCode::BAD_REQUEST, "InvalidLength"));
                    }
                    Ok((intent, Some(query)))
                })
                .await
                .map_err(|_| (StatusCode::REQUEST_TIMEOUT, "MetadataDeadline"))??
            } else {
                (metadata(&mut body, total).await?, None)
            };
            let coordinator = self
                .runtime
                .load_coordinator()
                .ok_or((StatusCode::SERVICE_UNAVAILABLE, "ScientificUnavailable"))?;
            let run = coordinator
                .run(intent.run())
                .map_err(|_| (StatusCode::NOT_FOUND, "RunUnavailable"))?;
            let lease = tokio::time::timeout(
                Duration::from_secs(5),
                run.acquire_field_at(intent.field().clone(), intent.boundary()),
            )
            .await
            .map_err(|_| (StatusCode::GATEWAY_TIMEOUT, "ObservationDeadline"))?
            .map_err(|error| match error {
                RunError::StaleBoundary { .. } => (StatusCode::CONFLICT, "StaleBoundary"),
                RunError::Closed | RunError::Publication => {
                    (StatusCode::NOT_FOUND, "RunUnavailable")
                }
                _ => (StatusCode::SERVICE_UNAVAILABLE, "FieldUnavailable"),
            })?;
            // All context serialization, point scans and guest work happen away
            // from the scientific executor, retaining budgets until they finish.
            let encoded = tokio::task::spawn_blocking(move || {
                let bytes = if let Some(query) = query {
                    let limits = field_sample_limits(lease.context());
                    let request =
                        SampleRequest::read(&query, &mut SampleScratch::default(), limits)
                            .map_err(|_| (StatusCode::BAD_REQUEST, "InvalidSample"))?;
                    request
                        .check_context(lease.context())
                        .map_err(|_| (StatusCode::CONFLICT, "SampleContextMismatch"))?;
                    if &request.metadata().snapshot != lease.snapshot() {
                        return Err((StatusCode::CONFLICT, "SampleSnapshotMismatch"));
                    }
                    request
                        .layout(limits)
                        .map_err(|_| (StatusCode::BAD_REQUEST, "SampleLimit"))?;
                    let count = request.len() as u64;
                    let control = OperationControl::new(Duration::from_secs(4))
                        .map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "SamplingUnavailable"))?;
                    let samples = lease
                        .sample(
                            Buffer {
                                schema: SAMPLE_REQUEST_SCHEMA.into(),
                                value_count: count,
                                bytes: query.into(),
                            },
                            control,
                        )
                        .map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "SamplingUnavailable"))?;
                    samples.bytes
                } else {
                    FieldObservation::new(lease.snapshot().clone(), lease.context().clone())
                        .and_then(|value| value.to_cbor())
                        .map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "ObservationLimit"))?
                        .into()
                };
                let digest = ArtifactDigest::sha256_of(&bytes);
                Ok::<_, (StatusCode, &'static str)>((
                    bytes::Bytes::from_owner(EncodedObservation {
                        bytes,
                        _guards: (lease, permit),
                    }),
                    digest,
                ))
            });
            tokio::time::timeout(Duration::from_secs(5), encoded)
                .await
                .map_err(|_| (StatusCode::GATEWAY_TIMEOUT, "ObservationDeadline"))?
                .map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "ObservationUnavailable"))?
        }
        .await;
        match result {
            Err((status, code)) => super::super::api_error(res, status, code),
            Ok((bytes, digest)) => deliver(
                res,
                bytes,
                digest,
                if self.sample {
                    FIELD_SAMPLE_RESPONSE_MEDIA_TYPE
                } else {
                    FIELD_OBSERVATION_MEDIA_TYPE
                },
            ),
        }
    }
}

#[handler]
impl ObjectHandler {
    async fn handle(&self, req: &mut Request, res: &mut Response) {
        let valid = header(req, "authorization")
            .ok()
            .flatten()
            .and_then(|value| value.strip_prefix("Bearer "))
            .is_some_and(|token| self.runtime.authorize_operator(token));
        if !valid {
            res.add_header("WWW-Authenticate", "Bearer", true)
                .expect("static header");
            super::super::api_error(res, StatusCode::UNAUTHORIZED, "Unauthorized");
            return;
        }
        let Ok(permit) = self.capacity.clone().try_acquire_owned() else {
            super::super::api_error(res, StatusCode::SERVICE_UNAVAILABLE, "ObserverBusy");
            return;
        };
        let result = async {
            if req.uri().query().is_some()
                || [
                    "if-match",
                    "content-encoding",
                    "transfer-encoding",
                    "trailer",
                ]
                .iter()
                .any(|name| req.headers().contains_key(*name))
            {
                return Err((StatusCode::BAD_REQUEST, "UnsupportedFraming"));
            }
            if header(req, "content-type")? != Some("application/cbor") {
                return Err((StatusCode::UNSUPPORTED_MEDIA_TYPE, "UnsupportedMediaType"));
            }
            let total = length(req, MAX_OBJECT_REQUEST_BYTES as u64)?;
            let request: ObjectObservationRequest =
                metadata(&mut HttpBody::new(req.take_body()), total).await?;
            let coordinator = self
                .runtime
                .load_coordinator()
                .ok_or((StatusCode::SERVICE_UNAVAILABLE, "ScientificUnavailable"))?;
            let run = coordinator
                .run(request.run())
                .map_err(|_| (StatusCode::NOT_FOUND, "RunUnavailable"))?;
            let lease = tokio::time::timeout(
                Duration::from_secs(5),
                run.acquire_objects_at(request.boundary()),
            )
            .await
            .map_err(|_| (StatusCode::GATEWAY_TIMEOUT, "ObservationDeadline"))?
            .map_err(|error| match error {
                RunError::StaleBoundary { .. } => (StatusCode::CONFLICT, "StaleBoundary"),
                RunError::Closed | RunError::Publication => {
                    (StatusCode::NOT_FOUND, "RunUnavailable")
                }
                _ => (StatusCode::SERVICE_UNAVAILABLE, "ObserverBusy"),
            })?;
            // The blocking closure owns the observer permit even if this request
            // disappears. No CPU encoding or large buffers enter the executor.
            let encoded = tokio::task::spawn_blocking(move || {
                let mut output = Vec::new();
                let digest = lease
                    .encode(&mut output, object_limits())
                    .map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "ObservationLimit"))?;
                let bytes = bytes::Bytes::from_owner(EncodedObservation {
                    bytes: output,
                    _guards: (lease, permit),
                });
                Ok::<_, (StatusCode, &'static str)>((bytes, digest))
            });
            tokio::time::timeout(Duration::from_secs(5), encoded)
                .await
                .map_err(|_| (StatusCode::GATEWAY_TIMEOUT, "ObservationDeadline"))?
                .map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "ObservationUnavailable"))?
        }
        .await;
        match result {
            Err((status, code)) => super::super::api_error(res, status, code),
            Ok((bytes, digest)) => deliver(res, bytes, digest, OBJECT_OBSERVATION_MEDIA_TYPE),
        }
    }
}

fn deliver(
    res: &mut Response,
    mut bytes: bytes::Bytes,
    digest: ArtifactDigest,
    media: &'static str,
) {
    res.status_code(StatusCode::OK);
    res.add_header("Content-Type", media, true)
        .expect("static header");
    res.add_header("Cache-Control", "no-store", true)
        .expect("static header");
    res.add_header("Content-Length", bytes.len().to_string(), true)
        .expect("bounded length");
    res.add_header(OBJECT_OBSERVATION_DIGEST_HEADER, digest.to_string(), true)
        .expect("canonical digest");
    let mut sender = res.channel();
    tokio::spawn(async move {
        // Retain both budgets through actual bounded delivery, not
        // just handler return. Channel backpressure cannot stop a run.
        let _ = tokio::time::timeout(Duration::from_secs(10), async {
            while !bytes.is_empty() {
                sender
                    .send_data(bytes.split_to(bytes.len().min(64 * 1024)))
                    .await?;
            }
            Ok::<_, io::Error>(())
        })
        .await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn final_shared_chunk_retains_capacity_after_producer_and_body_drop() {
        let budget = Arc::new(tokio::sync::Semaphore::new(1));
        let permit = budget.clone().try_acquire_owned().unwrap();
        let mut body = bytes::Bytes::from_owner(EncodedObservation {
            bytes: vec![7; 128 * 1024],
            _guards: permit,
        });
        let earlier = body.split_to(64 * 1024);
        let final_chunk = body.clone();
        drop(body);
        drop(earlier);
        assert_eq!(budget.available_permits(), 0);
        assert_eq!(final_chunk[0], 7);
        drop(final_chunk);
        assert_eq!(budget.available_permits(), 1);
    }
}

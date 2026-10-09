//! Observer-side point selection and cold JSON projection; never field physics.
use super::*;
use orishu::model::run_observation::{FieldObservationRequest, field_sample_limits};
use orishu_plugin::{ArtifactDigest, FiniteF64, LocalContributionId, execution::*};
use serde::ser::{SerializeSeq, SerializeStruct};

#[derive(Debug, Args)]
pub struct FieldArgs {
    #[command(flatten)]
    run: RunArgs,
    /// Exact committed boundary; stale reads are refused, never refreshed.
    #[arg(long)]
    boundary: u64,
    /// Configured field instance ID, not a plugin or family name.
    #[arg(long)]
    field: orishu_workload::ComponentInstanceId,
}
impl FieldArgs {
    pub(super) fn request(&self) -> FieldObservationRequest {
        FieldObservationRequest::new(self.run.identity(), self.boundary, self.field.clone())
    }
}
#[derive(Clone, Debug)]
struct Position([FiniteF64; 3]);
fn position(text: &str) -> Result<Position, &'static str> {
    let mut parts = text.split(',');
    let mut values = [FiniteF64::new(0.0).expect("finite"); 3];
    for value in &mut values {
        *value = parts
            .next()
            .and_then(|v| v.parse::<f64>().ok())
            .and_then(|v| FiniteF64::new(v).ok())
            .ok_or("point requires three finite SI coordinates: x,y,z")?;
    }
    if parts.next().is_some() {
        return Err("point requires exactly three coordinates");
    }
    Ok(Position(values))
}
#[derive(Debug, Args)]
pub struct SampleArgs {
    #[command(flatten)]
    field: FieldArgs,
    /// Observer-scoped query identity, retained in scientific sample metadata.
    #[arg(long)]
    request_id: u64,
    /// Exact selected model binding slot from `workload field`; repeat in desired order.
    #[arg(long = "channel", required = true)]
    channels: Vec<LocalContributionId>,
    /// Sample position in metres as x,y,z; repeat (up to 4096). IDs are zero-based argument order.
    #[arg(long = "point", required = true, allow_hyphen_values = true, value_parser = position)]
    points: Vec<Position>,
}
impl SampleArgs {
    pub(super) fn intent(&self) -> SampleIntent {
        SampleIntent {
            field: self.field.request(),
            request_id: self.request_id,
            channels: self.channels.clone(),
            points: self
                .points
                .iter()
                .enumerate()
                .map(|(id, p)| SamplePoint {
                    id: id as u64,
                    position_metres: p.0,
                })
                .collect(),
        }
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SampleIntent {
    field: FieldObservationRequest,
    request_id: u64,
    channels: Vec<LocalContributionId>,
    points: Vec<SamplePoint>,
}
impl SampleIntent {
    pub(super) fn validate(&self) -> Result<(), Failure> {
        if self.points.is_empty()
            || self.points.len() > 4096
            || self.channels.is_empty()
            || self.channels.len() > 16
            || self
                .channels
                .iter()
                .enumerate()
                .any(|(i, c)| self.channels[..i].contains(c))
        {
            return Err(Failure::local(
                "request",
                "sample_limits",
                "require 1–4096 points and 1–16 distinct channel slots",
            ));
        }
        Ok(())
    }
}
pub(super) async fn sample(
    client: &HttpClusterClient,
    intent: &SampleIntent,
    mut report: Report,
) -> Report {
    let field = match client.scientific().field(&intent.field).await {
        Ok(Some(field)) => field,
        Ok(None) => {
            return Report {
                outcome: Outcome::Empty,
                ..report
            };
        }
        Err(error) => return report.fail(Failure::network(error, false)),
    };
    // Persist the exact descriptor even on a later refusal; it is not a live status
    // claim or a guarantee that the subsequent query succeeded.
    report.field = Some(field.clone());
    let channels = intent
        .channels
        .iter()
        .map(|slot| {
            field
                .context
                .observables
                .iter()
                .find(|binding| &binding.slot == slot)
                .map(|binding| binding.channel.clone())
        })
        .collect::<Option<Vec<_>>>();
    let Some(channels) = channels else {
        return report.fail(Failure::local("request", "channel_unavailable", "selected model does not declare every requested channel slot; inspect field observables"));
    };
    let query = (|| {
        let metadata = field.request(intent.request_id, channels).map_err(|_| ())?;
        let mut bytes = vec![];
        encode_sample_request(
            &metadata,
            &intent.points,
            &mut bytes,
            &mut SampleScratch::default(),
            field_sample_limits(&field.context),
        )
        .map_err(|_| ())?;
        Ok::<_, ()>(bytes)
    })();
    let Ok(query) = query else {
        return report.fail(Failure::local(
            "request",
            "sample_limits",
            "query exceeds selected field sampling policy",
        ));
    };
    match client
        .scientific()
        .samples(&intent.field, &field, &query)
        .await
    {
        Ok(Some(bytes)) => Report {
            outcome: Outcome::Observed,
            samples: Some(SampleReport {
                field,
                query,
                bytes,
            }),
            ..report
        },
        Ok(None) => Report {
            outcome: Outcome::Empty,
            ..report
        },
        Err(error) => report.fail(Failure::network(error, false)),
    }
}

/// Immutable validated packet pair. JSON serializes cells directly, without
/// per-cell vectors. Revalidation and final JSON buffering are cold CLI costs.
pub(super) struct SampleReport {
    field: FieldObservation,
    query: Vec<u8>,
    bytes: Vec<u8>,
}
impl std::fmt::Debug for SampleReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SampleReport").finish_non_exhaustive()
    }
}
impl Serialize for SampleReport {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let limits = field_sample_limits(&self.field.context);
        let query = SampleRequest::read(&self.query, &mut SampleScratch::default(), limits)
            .expect("validated immutable query");
        let response = SampleResponse::read(&self.bytes, &query, &self.field.context, limits)
            .expect("validated immutable response");
        let mut output = serializer.serialize_struct("Samples", 5)?;
        output.serialize_field("requestDigest", &ArtifactDigest::sha256_of(&self.query))?;
        output.serialize_field("responseDigest", &ArtifactDigest::sha256_of(&self.bytes))?;
        output.serialize_field("metadata", query.metadata())?;
        output.serialize_field("coverage", "complete-request")?;
        output.serialize_field(
            "readings",
            &Rows {
                query: &query,
                response: &response,
            },
        )?;
        output.end()
    }
}
struct Rows<'a> {
    query: &'a SampleRequest<'a>,
    response: &'a SampleResponse<'a>,
}
impl Serialize for Rows<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut rows = serializer.serialize_seq(Some(self.query.len()))?;
        for (index, point) in self.query.points().enumerate() {
            #[derive(Serialize)]
            struct Row<'a> {
                point: SamplePoint,
                channels: Cells<'a>,
            }
            rows.serialize_element(&Row {
                point,
                channels: Cells {
                    response: self.response,
                    point: index,
                    count: self.query.metadata().channels.len(),
                },
            })?;
        }
        rows.end()
    }
}
struct Cells<'a> {
    response: &'a SampleResponse<'a>,
    point: usize,
    count: usize,
}
impl Serialize for Cells<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut cells = serializer.serialize_seq(Some(self.count))?;
        for channel in 0..self.count {
            cells.serialize_element(&Cell(
                self.response
                    .cell(self.point, channel)
                    .expect("complete response"),
            ))?;
        }
        cells.end()
    }
}
struct Cell<'a>(SampleCell<'a>);
impl Serialize for Cell<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match &self.0 {
            SampleCell::Valid {
                values,
                quality_flags,
            } => {
                struct Values<'a>(&'a SampleValues<'a>);
                impl Serialize for Values<'_> {
                    fn serialize<S: serde::Serializer>(
                        &self,
                        serializer: S,
                    ) -> Result<S::Ok, S::Error> {
                        serializer.collect_seq(self.0.iter())
                    }
                }
                let mut cell = serializer.serialize_struct("SampleCell", 3)?;
                cell.serialize_field("valid", &true)?;
                cell.serialize_field("qualityFlags", quality_flags)?;
                cell.serialize_field("values", &Values(values))?;
                cell.end()
            }
            SampleCell::Invalid(reason) => {
                let mut cell = serializer.serialize_struct("SampleCell", 2)?;
                cell.serialize_field("valid", &false)?;
                cell.serialize_field("reason", reason)?;
                cell.end()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn coordinates_are_exact_finite_triples_in_argument_order() {
        assert_eq!(
            position("-1,2,3").unwrap().0.map(FiniteF64::get),
            [-1.0, 2.0, 3.0]
        );
        for bad in ["", "1,2", "1,2,3,4", "NaN,0,0", "inf,0,0", "1e999,0,0"] {
            assert!(position(bad).is_err());
        }
    }
}

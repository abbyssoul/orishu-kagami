//! Client-local, one-shot field inspection. No private field layout or physics.
use orishu::{
    client::http_client::{HttpClusterClient, ScientificError},
    model::{
        run_command::RunStatus,
        run_observation::{FieldObservationRequest, field_sample_limits},
    },
};
use orishu_plugin::{ArtifactDigest, FiniteF64, execution::*};
use std::{collections::BTreeSet, ops::Range, sync::Arc};
pub mod vectors;

const TEXT_BYTES: usize = 64 * 1024;
const DISPLAY_CELLS: usize = 256;
const DISPLAY_VALUES: usize = 16;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Instance(String),
    Points(String),
    Channel {
        generation: u64,
        index: usize,
    },
    Inspect,
    Sample,
    VectorLength(String),
    /// Explicitly interpret this exact vector-3 channel as world X/Y/Z.
    Vectors {
        generation: u64,
        channel: usize,
    },
    HideVectors,
}

pub(super) enum Job {
    Vectors(vectors::Request),
    Inspect(FieldObservationRequest, f64),
    Sample {
        request: FieldObservationRequest,
        field: Box<FieldObservation>,
        query: Vec<u8>,
    },
}
pub(super) enum Reply {
    Descriptor(FieldObservationRequest, Box<FieldObservation>),
    Samples(Arc<Report>),
    Vectors(Box<vectors::Vectors>),
}

/// Retained exact packet pair plus a bounded cold display projection. The full
/// response was validated; UI truncation never changes scientific completeness.
pub struct Report {
    pub field: FieldObservation,
    pub metadata: SampleMetadata,
    pub request_digest: ArtifactDigest,
    pub response_digest: ArtifactDigest,
    pub points: usize,
    pub cells: usize,
    pub rows: Vec<Row>,
    pub values: Vec<FiniteF64>,
    query: Vec<u8>,
    bytes: Vec<u8>,
}
pub struct Row {
    pub point: SamplePoint,
    pub channel: usize,
    pub invalidity: Option<SampleInvalidity>,
    pub quality: Option<u32>,
    pub values: Range<usize>,
    pub total_values: usize,
    /// Bounded text rendered without decoding or formatting packets per frame.
    pub display: String,
}

pub struct Inspector {
    pub instance: String,
    pub points: String,
    channels: BTreeSet<usize>,
    generation: u64,
    descriptor: Option<(FieldObservationRequest, FieldObservation)>,
    report: Option<Arc<Report>>,
    pub vector_length: String,
    vector_settings: Option<vectors::Settings>,
    vector_attempt_scale: Option<kagami_session::SceneScale>,
    vectors: Option<Box<vectors::Vectors>>,
}
impl Default for Inspector {
    fn default() -> Self {
        Self {
            instance: String::new(),
            points: "0,0,0".into(),
            channels: BTreeSet::new(),
            generation: 0,
            descriptor: None,
            report: None,
            vector_length: "1".into(),
            vector_settings: None,
            vector_attempt_scale: None,
            vectors: None,
        }
    }
}
impl Inspector {
    pub fn channel_selected(&self, index: usize) -> bool {
        self.channels.contains(&index)
    }
    pub fn has_channels(&self) -> bool {
        !self.channels.is_empty()
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn descriptor(&self) -> Option<&FieldObservation> {
        self.descriptor.as_ref().map(|(_, field)| field)
    }
    pub fn report(&self) -> Option<&Report> {
        self.report.as_deref()
    }
    pub fn vectors(&self) -> Option<&vectors::Vectors> {
        self.vectors.as_deref()
    }
    fn clear_vectors(&mut self) {
        self.generation = self
            .generation
            .checked_add(1)
            .expect("observer generation exhausted");
        self.vector_settings = None;
        self.vector_attempt_scale = None;
        self.vectors = None;
    }
    fn changed(&mut self) {
        self.clear_vectors();
        self.report = None;
    }
    pub(super) fn clear(&mut self) {
        self.changed();
        self.descriptor = None;
        self.channels.clear();
    }
    pub(super) fn act(
        &mut self,
        action: Action,
        status: Option<&RunStatus>,
        idle: bool,
        scale: kagami_session::SceneScale,
    ) -> Result<Option<Job>, &'static str> {
        match action {
            Action::VectorLength(value) => {
                if value.len() > 128 {
                    return Err("Arrow length text exceeds 128 bytes.");
                }
                self.clear_vectors();
                self.vector_length = value;
                return Ok(None);
            }
            Action::HideVectors => {
                self.clear_vectors();
                return Ok(None);
            }
            Action::Instance(value) => {
                if value.len() > 128 {
                    return Err("Field instance exceeds its input bound.");
                }
                self.clear();
                self.instance = value;
                return Ok(None);
            }
            Action::Points(value) => {
                if value.len() > TEXT_BYTES {
                    return Err("Point text exceeds 64 KiB.");
                }
                self.changed();
                self.points = value;
                return Ok(None);
            }
            Action::Channel { generation, index } => {
                if generation != self.generation {
                    return Err("Channel choice is stale; inspect the current descriptor.");
                }
                let field = self.descriptor().ok_or("Inspect a field first.")?;
                if index >= field.context.observables.len() {
                    return Err("Channel is not in the displayed descriptor.");
                }
                if !self.channels.remove(&index) {
                    if self.channels.len() == 16 {
                        return Err("Select at most 16 channels.");
                    }
                    self.channels.insert(index);
                }
                self.changed();
                return Ok(None);
            }
            _ => {}
        }
        if !idle {
            return Err("A run request still owns the background slot.");
        }
        let status = status.ok_or("Observe an exact run before inspecting its fields.")?;
        if let Action::Vectors {
            generation,
            channel,
        } = action
        {
            if generation != self.generation {
                return Err("Vector choice is stale; use the current report.");
            }
            let report = self.report.as_ref().ok_or("Sample the field first.")?;
            if !vectors::supported(report, channel) {
                return Err("Only an exact vector-3 channel can be mapped to world X/Y/Z.");
            }
            let length_metres = vectors::length(&self.vector_length)?;
            let report = report.clone();
            self.clear_vectors();
            let settings = vectors::Settings {
                channel,
                length_metres,
            };
            self.vector_settings = Some(settings);
            self.vector_attempt_scale = Some(scale);
            return Ok(Some(Job::Vectors(vectors::Request {
                report,
                settings,
                scale,
            })));
        }
        if matches!(action, Action::Inspect) {
            let instance = self
                .instance
                .parse()
                .map_err(|_| "Enter an exact configured field instance ID.")?;
            self.clear();
            return Ok(Some(Job::Inspect(
                FieldObservationRequest::new(
                    status.descriptor().identity().clone(),
                    status.boundary(),
                    instance,
                ),
                status.time_seconds(),
            )));
        }
        let (request, field) = self
            .descriptor
            .as_ref()
            .ok_or("Inspect a field descriptor first.")?;
        if request.run() != status.descriptor().identity()
            || request.boundary() != status.boundary()
        {
            return Err(
                "Field descriptor belongs to a different run/boundary; inspect again explicitly.",
            );
        }
        if self.channels.is_empty() {
            return Err("Choose at least one exact channel.");
        }
        let points = parse_points(&self.points)?;
        let channels = self
            .channels
            .iter()
            .map(|i| field.context.observables[*i].channel.clone())
            .collect();
        let metadata = field
            .request(self.generation, channels)
            .map_err(|_| "Invalid selected channels.")?;
        let mut query = vec![];
        encode_sample_request(
            &metadata,
            &points,
            &mut query,
            &mut SampleScratch::default(),
            field_sample_limits(&field.context),
        )
        .map_err(|_| "Point/channel query exceeds the selected field's policy.")?;
        let job = Job::Sample {
            request: request.clone(),
            field: Box::new(field.clone()),
            query,
        };
        self.changed();
        Ok(Some(job))
    }
    pub(super) fn adopt(&mut self, reply: Reply) {
        match reply {
            Reply::Descriptor(request, field) => self.descriptor = Some((request, *field)),
            Reply::Samples(report) => self.report = Some(report),
            Reply::Vectors(vectors) => self.vectors = Some(vectors),
        }
    }
    pub(super) fn reproject(&mut self, scale: kagami_session::SceneScale) -> Option<Job> {
        let settings = self.vector_settings?;
        let report = self.report.as_ref()?;
        if self.vector_attempt_scale == Some(scale) {
            return None;
        }
        // One attempt per scale: a failure is shown, never retried every poll.
        self.vector_attempt_scale = Some(scale);
        Some(Job::Vectors(vectors::Request {
            report: report.clone(),
            settings,
            scale,
        }))
    }
}

fn parse_points(text: &str) -> Result<Vec<SamplePoint>, &'static str> {
    if text.len() > TEXT_BYTES {
        return Err("Point text exceeds 64 KiB.");
    }
    let count = text.split(';').count();
    if count > 4096 {
        return Err("At most 4096 points per query.");
    }
    text.split(';')
        .enumerate()
        .map(|(id, point)| {
            let mut parts = point.split(',');
            let mut coordinates = [FiniteF64::new(0.0).expect("finite"); 3];
            for coordinate in &mut coordinates {
                *coordinate = parts
                    .next()
                    .and_then(|s| s.trim().parse::<f64>().ok())
                    .and_then(|v| FiniteF64::new(v).ok())
                    .ok_or(
                        "Each point requires exactly three finite SI coordinates: x,y,z; x,y,z.",
                    )?;
            }
            if parts.next().is_some() {
                return Err("Each point requires exactly three coordinates.");
            }
            Ok(SamplePoint {
                id: id as u64,
                position_metres: coordinates,
            })
        })
        .collect()
}

pub(super) async fn perform(
    client: &HttpClusterClient,
    job: Job,
) -> Result<Reply, ScientificError> {
    match job {
        Job::Vectors(_) => unreachable!("local projection handled before connection IO"),
        Job::Inspect(request, expected_time) => {
            let field =
                client
                    .scientific()
                    .field(&request)
                    .await?
                    .ok_or(ScientificError::Protocol(
                        "exact retained field unavailable",
                    ))?;
            if !matches!(field.snapshot.source, SnapshotSource::Committed { time_seconds, .. } if time_seconds.get() == expected_time)
            {
                return Err(ScientificError::Protocol(
                    "status and field observation time disagree",
                ));
            }
            Ok(Reply::Descriptor(request, Box::new(field)))
        }
        Job::Sample {
            request,
            field,
            query,
        } => {
            let bytes = client
                .scientific()
                .samples(&request, &field, &query)
                .await?
                .ok_or(ScientificError::Protocol(
                    "exact retained field unavailable",
                ))?;
            Ok(Reply::Samples(Arc::new(project(*field, query, bytes)?)))
        }
    }
}

fn project(
    field: FieldObservation,
    query: Vec<u8>,
    bytes: Vec<u8>,
) -> Result<Report, ScientificError> {
    let limits = field_sample_limits(&field.context);
    let parsed = SampleRequest::read(&query, &mut SampleScratch::default(), limits)
        .map_err(|_| ScientificError::Protocol("invalid sample query"))?;
    let response = SampleResponse::read(&bytes, &parsed, &field.context, limits)
        .map_err(|_| ScientificError::Protocol("invalid sample response"))?;
    let cells = parsed.len() * parsed.metadata().channels.len();
    let mut rows = Vec::with_capacity(cells.min(DISPLAY_CELLS));
    let mut values = Vec::with_capacity(cells.min(DISPLAY_CELLS) * DISPLAY_VALUES);
    'points: for (point_index, point) in parsed.points().enumerate() {
        for channel in 0..parsed.metadata().channels.len() {
            if rows.len() == DISPLAY_CELLS {
                break 'points;
            }
            let start = values.len();
            let (invalidity, quality, total_values) = match response
                .cell(point_index, channel)
                .expect("validated complete response")
            {
                SampleCell::Invalid(reason) => (Some(reason), None, 0),
                SampleCell::Valid {
                    values: components,
                    quality_flags,
                } => {
                    let count = components.iter().len();
                    values.extend(components.iter().take(DISPLAY_VALUES));
                    (None, Some(quality_flags), count)
                }
            };
            let displayed = &values[start..];
            let value = if let Some(reason) = invalidity {
                format!("Unavailable: {reason:?}")
            } else {
                let components = displayed
                    .iter()
                    .map(|v| v.get().to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "[{components}] ({} of {total_values} SI values), quality={quality:?}",
                    displayed.len()
                )
            };
            let display = format!(
                "Point {} {:?} m\n{}: {value}",
                point.id,
                point.position_metres.map(|v| v.get()),
                parsed.metadata().channels[channel].schema.name
            );
            rows.push(Row {
                point,
                channel,
                invalidity,
                quality,
                values: start..values.len(),
                total_values,
                display,
            });
        }
    }
    let metadata = parsed.metadata().clone();
    let points = parsed.len();
    Ok(Report {
        field,
        metadata,
        points,
        cells,
        rows,
        values,
        request_digest: ArtifactDigest::sha256_of(&query),
        response_digest: ArtifactDigest::sha256_of(&bytes),
        query,
        bytes,
    })
}

impl Reply {
    pub(super) fn reflects(&self, marker: &str) -> bool {
        let contains = |bytes: &[u8]| bytes.windows(marker.len()).any(|s| s == marker.as_bytes());
        let field = match self {
            Self::Vectors(_) => return false,
            Self::Descriptor(_, field) => field,
            Self::Samples(report) => {
                if contains(&report.query) || contains(&report.bytes) {
                    return true;
                }
                &report.field
            }
        };
        contains(&serde_json::to_vec(field).expect("validated bounded descriptor"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn point_generation_is_bounded_finite_ordered_and_observer_owned() {
        let points = parse_points("1, 2,3; -4,5,6").unwrap();
        assert_eq!(points.len(), 2);
        assert_eq!(points[1].id, 1);
        assert_eq!(points[1].position_metres[0].get(), -4.0);
        for text in ["", "1,2", "1,2,3,4", "NaN,0,0", "1,2,3;", "inf,0,0"] {
            assert!(parse_points(text).is_err());
        }
        assert!(parse_points(&"0,0,0;".repeat(4096)).is_err());
        assert!(parse_points(&"0".repeat(TEXT_BYTES + 1)).is_err());
    }
}

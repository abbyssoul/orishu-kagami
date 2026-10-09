//! Observer-only direction projection. A vector-shaped channel is not
//! automatically a world-space vector: the UI requires explicit X/Y/Z mapping.
use super::Report;
use kagami_renderer::{Arrow, ArrowBatch};
use kagami_session::SceneScale;
use orishu::model::run_observation::field_sample_limits;
use orishu_plugin::{Shape, execution::*};
use std::sync::Arc;

/// Geometry for one validated report/channel, presentation length and scale.
/// O(points) cold work; immutable O(points) storage, no per-frame sample decoding.
pub struct Vectors {
    pub scale: SceneScale,
    pub arrows: Arc<ArrowBatch>,
    pub channel: usize,
    pub length_metres: f64,
    pub points: usize,
    pub invalid: usize,
    pub zero: usize,
    pub omitted_scale: usize,
}
#[derive(Clone, Copy)]
pub(super) struct Settings {
    pub channel: usize,
    pub length_metres: f64,
}
pub(crate) struct Request {
    pub(super) report: Arc<Report>,
    pub(super) settings: Settings,
    pub(super) scale: SceneScale,
}

pub(super) fn supported(report: &Report, channel: usize) -> bool {
    report
        .metadata
        .channels
        .get(channel)
        .is_some_and(|c| c.schema.shape == Shape::Vector { length: 3 })
}
pub(super) fn length(text: &str) -> Result<f64, &'static str> {
    if text.len() > 128 {
        return Err("Arrow length text exceeds 128 bytes.");
    }
    text.trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && *v > 0.)
        .ok_or("Enter a finite positive arrow length in metres (presentation only).")
}

pub(crate) fn project(request: Request) -> Result<Vectors, &'static str> {
    let Request {
        report,
        settings,
        scale,
    } = request;
    if !supported(&report, settings.channel)
        || !settings.length_metres.is_finite()
        || settings.length_metres <= 0.
    {
        return Err("Direction projection requires an exact vector-3 channel and positive length.");
    }
    let limits = field_sample_limits(&report.field.context);
    let query = SampleRequest::read(&report.query, &mut SampleScratch::default(), limits)
        .map_err(|_| "Invalid retained sampling request.")?;
    let response = SampleResponse::read(&report.bytes, &query, &report.field.context, limits)
        .map_err(|_| "Invalid retained sampling response.")?;
    if query.len() > kagami_renderer::MAX_ARROWS {
        return Err("Direction query exceeds 4096 points.");
    }
    let mut arrows = Vec::with_capacity(query.len());
    let mut invalid = 0;
    let mut zero = 0;
    let mut omitted_scale = 0;
    for (index, point) in query.points().enumerate() {
        match response
            .cell(index, settings.channel)
            .expect("validated complete response")
        {
            SampleCell::Invalid(_) => invalid += 1,
            SampleCell::Valid { values, .. } => {
                let mut values = values.iter();
                let vector =
                    std::array::from_fn(|_| values.next().expect("vector3 validated").get());
                match arrow(
                    scale,
                    point.position_metres.map(|v| v.get()),
                    vector,
                    settings.length_metres,
                ) {
                    Projection::Arrow(a) => arrows.push(a),
                    Projection::Zero => zero += 1,
                    Projection::Omitted => omitted_scale += 1,
                }
            }
        }
    }
    Ok(Vectors {
        scale,
        arrows: Arc::new(ArrowBatch::new(arrows).expect("bounded finite arrows")),
        channel: settings.channel,
        length_metres: settings.length_metres,
        points: query.len(),
        invalid,
        zero,
        omitted_scale,
    })
}

enum Projection {
    Arrow(Arrow),
    Zero,
    Omitted,
}
fn arrow(scale: SceneScale, point: [f64; 3], vector: [f64; 3], length: f64) -> Projection {
    let max = vector.iter().map(|v| v.abs()).fold(0.0, f64::max);
    if max == 0. {
        return Projection::Zero;
    }
    // Scale before squaring: finite huge or subnormal scientific values must
    // not become an overflow/underflow-induced fabricated zero direction.
    let relative = vector.map(|v| v / max);
    let norm = relative.iter().map(|v| v * v).sum::<f64>().sqrt();
    let end = std::array::from_fn(|i| point[i] + (relative[i] / norm) * length);
    let Some(start) = crate::viewport::geometry_units(scale, point) else {
        return Projection::Omitted;
    };
    let Some(end) = crate::viewport::geometry_units(scale, end) else {
        return Projection::Omitted;
    };
    // Equal narrowed endpoints do not have a representable direction.
    Arrow::new(start, end, [0.3, 1.0, 0.4]).map_or(Projection::Omitted, Projection::Arrow)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn direction_normalization_is_scaled_finite_and_presentation_only() {
        for value in [f64::MAX, 1.0, f64::from_bits(1)] {
            let Projection::Arrow(a) = arrow(
                SceneScale::NANOMETRE,
                [2e-9, 0., 0.],
                [0., -value, 0.],
                3e-9,
            ) else {
                panic!("representable")
            };
            assert_eq!(a.start(), [2., 0., 0.]);
            assert_eq!(a.end(), [2., -3., 0.]);
        }
        assert!(matches!(
            arrow(SceneScale::METRE, [0.; 3], [0.; 3], 1.),
            Projection::Zero
        ));
        assert!(matches!(
            arrow(SceneScale::NANOMETRE, [f64::MAX; 3], [1.; 3], 1.),
            Projection::Omitted
        ));
        assert!(matches!(
            arrow(SceneScale::METRE, [1.; 3], [1.; 3], 1e-100),
            Projection::Omitted
        ));
        for bad in ["0", "-1", "NaN", "inf", "", "1 m"] {
            assert!(length(bad).is_err());
        }
        assert!(length(&"1".repeat(129)).is_err());
        assert_eq!(length("1e-9"), Ok(1e-9));
    }
}

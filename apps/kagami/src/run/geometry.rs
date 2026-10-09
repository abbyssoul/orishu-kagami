//! Presentation-only extraction and SI-to-render conversion for committed
//! objects. No scientific state, physical radii or authored transforms are made.
use kagami_renderer::{Marker, MarkerBatch};
use kagami_session::SceneScale;
use std::sync::Arc;

pub(super) struct Sample {
    pub position_metres: [f64; 3],
    pub dynamic: bool,
}
/// A bounded projection for exactly one snapshot and scene scale. Camera motion
/// changes matrices only; scale changes reproject off-window from cached SI data.
pub struct Geometry {
    pub scale: SceneScale,
    pub markers: Arc<MarkerBatch>,
    pub omitted_scale: usize,
    pub omitted_capacity: usize,
}
pub(super) fn project(samples: &[Sample], total: usize, scale: SceneScale) -> Geometry {
    let mut markers = Vec::with_capacity(samples.len());
    let mut omitted_scale = 0;
    for sample in samples {
        if let Some(position) = crate::viewport::geometry_units(scale, sample.position_metres) {
            let color = if sample.dynamic {
                [1.0, 0.65, 0.1]
            } else {
                [0.2, 0.75, 1.0]
            };
            markers.push(Marker::new(position, color).expect("finite geometry and fixed RGB"));
        } else {
            omitted_scale += 1;
        }
    }
    Geometry {
        scale,
        markers: Arc::new(MarkerBatch::new(markers).expect("extraction bounds markers")),
        omitted_scale,
        omitted_capacity: total - samples.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maximum_extraction_has_explicit_capacity_accounting() {
        let samples: Vec<_> = (0..kagami_renderer::MAX_MARKERS)
            .map(|_| Sample {
                position_metres: [0.0; 3],
                dynamic: false,
            })
            .collect();
        let projected = project(&samples, samples.len() + 2, SceneScale::METRE);
        assert_eq!(
            projected.markers.markers().len(),
            kagami_renderer::MAX_MARKERS
        );
        assert_eq!(projected.omitted_capacity, 2);
        assert_eq!(projected.omitted_scale, 0);
    }
    #[test]
    fn conversion_scales_then_narrows_and_reports_each_omission() {
        let samples = [
            Sample {
                position_metres: [2e-9, -1e-9, 0.0],
                dynamic: true,
            },
            Sample {
                position_metres: [f64::MAX, 0.0, 0.0],
                dynamic: false,
            },
        ];
        let projected = project(&samples, 3, SceneScale::NANOMETRE);
        assert_eq!(projected.markers.markers().len(), 1);
        assert_eq!(projected.markers.markers()[0].position(), [2.0, -1.0, 0.0]);
        assert_eq!(projected.omitted_scale, 1);
        assert_eq!(projected.omitted_capacity, 1);
        assert_eq!(samples[0].position_metres, [2e-9, -1e-9, 0.0]);
    }
}

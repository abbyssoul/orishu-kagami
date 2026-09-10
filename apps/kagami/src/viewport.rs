//! The 3D viewport, and the one conversion between a saved view and a camera.
//!
//! `kagami-session` owns the pose because it has to be persisted and bounded;
//! `kagami-renderer` owns the matrices because it has to feed `wgpu`. Neither
//! depends on the other, so the conversion lives here — in the shell, where
//! adapters belong.

use kagami_renderer::{Camera, SceneProgram};
use kagami_session::{AuthoringView, MAX_CAMERA_TARGET_UNITS, Projection, SceneScale};

use crate::message::Message;
use crate::model::Model;
use iced::widget::shader;
use iced::{Element, Length};

pub fn view(model: &Model) -> Element<'_, Message> {
    // Built from the current view every frame rather than held: the camera is
    // the model's, so there is exactly one of it and it cannot be reset by a
    // change to the widget tree.
    shader(SceneProgram::new(camera_for(model.current_view())))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// The renderer's camera for a saved authoring view.
///
/// Two conversions, in this order, and the order is the point:
///
/// 1. **Divide by the scene scale.** Every distance leaves `kagami-session` in
///    SI metres and enters the renderer in render units. This is what lets one
///    camera serve a molecule and a planetary orbit: the renderer's fixed near
///    plane, far plane and grid spacing are all in render units, so they are
///    the right size at every physical scale without being retuned.
/// 2. **Narrow to `f32`.** Doing it *after* the division is what makes the
///    result's magnitude — rather than its absolute distance from an arbitrary
///    origin — decide how much of `f32`'s seven digits it consumes. A 2 nm
///    radius narrowed first is 2e-9 and vanishes into the near plane; divided
///    first it is 2.0.
///
/// The narrowing is also *checked*, because `as f32` answers an out-of-range
/// `f64` with infinity rather than an error, and one infinite component makes
/// every matrix derived from it non-finite. `AuthoringView` already bounds its
/// pose against its scale, so nothing arriving through a document can be out
/// of range — this makes the *boundary* total rather than relying on that,
/// since `Camera` is public and its other callers are not bound by a file's
/// rules.
fn camera_for(view: AuthoringView) -> Camera {
    let scale = view.scale();
    let camera = view.camera();
    let [x, y, z] = camera.target().to_array();
    let render = |metres: f64| camera_units(scale, metres);
    Camera::new(
        [render(x), render(y), render(z)],
        render(camera.distance()),
        // Angles are not lengths, so the scale has nothing to say about them.
        narrow_angle(camera.yaw()),
        narrow_angle(camera.pitch()),
        match view.projection() {
            Projection::Perspective => kagami_renderer::Projection::Perspective,
            Projection::Orthographic => kagami_renderer::Projection::Orthographic,
        },
    )
}

/// A physical position in render space, or `None` if it does not fit there.
///
/// **The geometry path**, and deliberately not the camera's. A document
/// object's position either converts or it does not: a scale small enough and a
/// coordinate large enough have no render-space answer, and the honest response
/// is to *omit* the object. Drawing it at the edge of the reachable region
/// instead would put it somewhere it is not, which is worse than not drawing
/// it — a viewer cannot tell a saturated position from a real one.
///
/// Nothing renders document geometry yet; this is the conversion every later
/// object, field and trail consumer must use rather than casting metres to
/// `f32` directly.
pub fn geometry_units(scale: SceneScale, world_metres: [f64; 3]) -> Option<[f32; 3]> {
    let mut units = [0.0_f32; 3];
    for (slot, metres) in units.iter_mut().zip(world_metres) {
        let value = scale.to_render(metres)?;
        // Inside what the renderer can resolve, not merely finite: beyond this
        // the `f32` narrowing stops being able to tell the point from its
        // neighbours, so a coordinate here would be fiction.
        if value.abs() > MAX_CAMERA_TARGET_UNITS {
            return None;
        }
        *slot = value as f32;
    }
    Some(units)
}

/// One camera distance in render units, saturating rather than declining.
///
/// **The camera path**, which is a different question from the geometry one
/// above. A camera must always render *something* — there is no "omit the
/// viewport" — so where geometry declines, this clamps to the reachable limit
/// and draws from somewhere absurd but defined.
///
/// `AuthoringView` bounds its pose against its scale, so the conversion cannot
/// actually fail for a view that came from a document or a gesture. This exists
/// to keep the boundary total for a caller that assembled a `Camera` some other
/// way, and the saturation is confined to it.
fn camera_units(scale: SceneScale, metres: f64) -> f32 {
    match scale.to_render(metres) {
        Some(units) => clamp_units(units),
        // Unreachable through `AuthoringView`. Saturating towards the limit the
        // value was heading for keeps the view defined and pointing the right
        // way, which a zero would not.
        None if metres.is_sign_negative() => -MAX_CAMERA_TARGET_UNITS as f32,
        None => MAX_CAMERA_TARGET_UNITS as f32,
    }
}

/// Narrow an angle, which no scale applies to.
fn narrow_angle(radians: f64) -> f32 {
    if radians.is_nan() {
        return 0.0;
    }
    radians as f32
}

/// Bring a render-unit value inside what the renderer can resolve.
fn clamp_units(units: f64) -> f32 {
    if units.is_nan() {
        return 0.0;
    }
    units.clamp(-MAX_CAMERA_TARGET_UNITS, MAX_CAMERA_TARGET_UNITS) as f32
}

#[cfg(test)]
mod tests {
    use kagami_document::Vector3;
    use kagami_session::{CameraPose, MAX_CAMERA_DISTANCE_UNITS, SceneScale};

    use super::*;

    #[test]
    fn narrowing_never_produces_a_non_finite_coordinate() {
        for value in [
            1e40,
            -1e40,
            f64::MAX,
            f64::MIN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NAN,
            0.0,
            -1.5,
            6.371e6,
        ] {
            assert!(clamp_units(value).is_finite(), "{value}");
        }
        assert_eq!(clamp_units(f64::NAN), 0.0);
        assert_eq!(clamp_units(-1.5), -1.5);

        // The camera path saturates towards the limit it was heading for, so a
        // view assembled outside `AuthoringView` still points the right way.
        assert_eq!(
            camera_units(SceneScale::from_metres(1e-30).expect("supported"), f64::MAX),
            MAX_CAMERA_TARGET_UNITS as f32
        );
        assert_eq!(
            camera_units(
                SceneScale::from_metres(1e-30).expect("supported"),
                -f64::MAX
            ),
            -MAX_CAMERA_TARGET_UNITS as f32
        );
    }

    #[test]
    fn geometry_is_omitted_rather_than_pinned_to_the_edge() {
        // The policy the review named: geometry that has no render-space answer
        // is dropped, not drawn somewhere it is not. A saturated position is
        // indistinguishable from a real one to whoever is looking at it.
        let tiny = SceneScale::from_metres(1e-30).expect("supported");
        assert_eq!(
            geometry_units(tiny, [f64::MAX, 0.0, 0.0]),
            None,
            "a coordinate with no quotient is omitted"
        );
        assert_eq!(geometry_units(tiny, [f64::NAN, 0.0, 0.0]), None);
        assert_eq!(geometry_units(tiny, [f64::INFINITY, 0.0, 0.0]), None);
        // Finite, convertible, and still past what the renderer can resolve.
        assert_eq!(
            geometry_units(SceneScale::METRE, [2.0 * MAX_CAMERA_TARGET_UNITS, 0.0, 0.0]),
            None
        );
        // One bad component omits the whole point rather than half of it.
        assert_eq!(
            geometry_units(SceneScale::METRE, [0.0, 0.0, f64::MAX]),
            None
        );

        // What is in range converts exactly.
        assert_eq!(
            geometry_units(SceneScale::NANOMETRE, [2.0e-9, -1.0e-9, 0.0]),
            Some([2.0, -1.0, 0.0])
        );
    }

    #[test]
    fn an_extreme_pose_yields_a_finite_view_projection_at_every_scale() {
        // The failure this guards: a pose whose narrowed components are
        // infinite makes every entry of the view-projection matrix non-finite,
        // and the scene is then undefined. Swept across the preset range
        // because the conversion the narrowing sees now depends on the scale.
        let far = Vector3::new(1e40, -1e40, 1e40).expect("finite f64");
        for scale in SceneScale::PRESETS.iter().copied() {
            for projection in [Projection::Perspective, Projection::Orthographic] {
                let view = AuthoringView::new(
                    projection,
                    scale,
                    CameraPose::new(far, MAX_CAMERA_DISTANCE_UNITS * scale.metres(), 0.4, 0.3)
                        .expect("names a camera"),
                )
                .expect("bounded");

                let matrix = camera_for(view).view_projection_matrix(16.0 / 9.0);
                assert!(
                    matrix.to_cols_array().iter().all(|entry| entry.is_finite()),
                    "{scale:?} {projection:?}: {matrix:?}"
                );
            }
        }
    }

    /// A physical position in SI metres, through the production conversion and
    /// projection, as normalised device coordinates.
    ///
    /// The whole path a document object's geometry will take once a consumer
    /// renders one: metres → render units → `f32` → view-projection → clip →
    /// NDC. Nothing here is a renderer-private shortcut.
    fn project(view: AuthoringView, world_metres: [f64; 3], aspect: f32) -> [f32; 3] {
        let point = geometry_units(view.scale(), world_metres)
            .expect("the tested geometry is inside what its scale can convert");
        camera_for(view)
            .project(point, aspect)
            .expect("the point is in front of the camera at every tested scale")
    }

    #[test]
    fn the_same_structure_viewed_at_its_own_scale_lands_in_the_same_place() {
        // The integration proof slice 4 asks for, on synthetic geometry rather
        // than on the camera alone: a structure `n` units across, viewed from
        // `m` units away, occupies the same fraction of the screen whatever the
        // *physical* size of a unit. That is what "one camera serves both
        // extremes" has to mean, and it is the property the divide-then-narrow
        // order buys.
        //
        // The scene: a marker one unit above the origin along +z, seen from 20
        // units away with no elevation. At pitch zero the world +z axis is
        // screen-up, so the offset shows in NDC y. At nanometre scale this is a
        // 1 nm feature on a molecule; at light-year scale it is a one-light-year
        // separation.
        let aspect = 16.0 / 9.0;
        let reference = {
            let scale = SceneScale::METRE;
            let view = AuthoringView::new(
                Projection::Perspective,
                scale,
                CameraPose::new(Vector3::ZERO, 20.0 * scale.metres(), 0.0, 0.0)
                    .expect("names a camera"),
            )
            .expect("bounded");
            project(view, [0.0, 0.0, scale.metres()], aspect)
        };

        for projection in [Projection::Perspective, Projection::Orthographic] {
            for scale in SceneScale::PRESETS.iter().copied() {
                let view = AuthoringView::new(
                    projection,
                    scale,
                    CameraPose::new(Vector3::ZERO, 20.0 * scale.metres(), 0.0, 0.0)
                        .expect("names a camera"),
                )
                .expect("bounded");
                let ndc = project(view, [0.0, 0.0, scale.metres()], aspect);

                assert!(
                    ndc.iter().all(|value| value.is_finite()),
                    "{scale:?} {projection:?}: {ndc:?}"
                );
                // On screen, not collapsed onto the axis and not flung off it.
                assert!(
                    ndc[1].abs() > 1.0e-3 && ndc[1].abs() < 1.0,
                    "a one-unit feature must be visible at {scale:?} {projection:?}: {ndc:?}"
                );
                if projection == Projection::Perspective {
                    assert!(
                        (ndc[1] - reference[1]).abs() < 1.0e-3,
                        "{scale:?} must frame it like metre scale does: {ndc:?} vs {reference:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_two_nanometre_feature_is_invisible_without_a_scale_and_visible_with_one() {
        // The failure a scale exists to fix, stated as a comparison. Under the
        // metre scale a 2 nm offset projects to essentially zero — the same
        // pixel as the origin, which is why a molecule could not be inspected.
        let aspect = 16.0 / 9.0;
        // Along +z, which is screen-up at this orientation.
        let two_nanometres = [0.0, 0.0, 2.0e-9];

        let unscaled = AuthoringView::new(
            Projection::Perspective,
            SceneScale::METRE,
            CameraPose::new(Vector3::ZERO, 20.0, 0.0, 0.0).expect("names a camera"),
        )
        .expect("bounded");
        let flat = project(unscaled, two_nanometres, aspect);
        assert!(
            flat[1].abs() < 1.0e-9,
            "at metre scale a nanometre feature is not distinguishable: {flat:?}"
        );

        let scaled = AuthoringView::new(
            Projection::Perspective,
            SceneScale::NANOMETRE,
            CameraPose::new(Vector3::ZERO, 20.0e-9, 0.0, 0.0).expect("names a camera"),
        )
        .expect("bounded");
        let visible = project(scaled, two_nanometres, aspect);
        assert!(
            visible[1].abs() > 1.0e-2,
            "at nanometre scale the same feature is plainly on screen: {visible:?}"
        );
    }

    #[test]
    fn the_camera_reaches_the_same_render_distance_at_every_scale() {
        // What the division buys: whatever the physical scale, the renderer
        // receives a distance inside the window its fixed near and far planes
        // were built for. That is why one camera can serve a molecule and an
        // orbit without the projection matrices being retuned.
        for scale in SceneScale::PRESETS.iter().copied() {
            let view = AuthoringView::new(
                Projection::Perspective,
                scale,
                CameraPose::new(
                    Vector3::ZERO,
                    MAX_CAMERA_DISTANCE_UNITS * scale.metres(),
                    0.0,
                    0.0,
                )
                .expect("names a camera"),
            )
            .expect("bounded");

            let distance = camera_for(view).distance;
            assert!(
                (f64::from(distance) - MAX_CAMERA_DISTANCE_UNITS).abs() < 1.0,
                "{scale:?}: {distance}"
            );
        }
    }
}

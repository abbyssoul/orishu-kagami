use glam::{Mat4, Vec3};

/// How the scene is projected onto the window.
///
/// The render-time spelling of the choice ADR 0022 persists. Deliberately a
/// separate type from `kagami_session::Projection`: the persisted contract must
/// not depend on a renderer's representation, so the two meet at a conversion
/// in the app rather than by sharing a definition. Neither is a physical
/// parameter, and no solver ever sees one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Projection {
    /// A perspective frustum, so depth reads as depth.
    #[default]
    Perspective,
    /// A parallel projection, so relative extents are comparable by eye.
    Orthographic,
}

/// An orbit camera in spherical coordinates around a `target` point,
/// using a Z-up convention.
///
/// A value the application hands in each frame rather than state this crate
/// keeps. Where the camera *is* has to survive a save (ADR 0022), and widget
/// state cannot be read at save time — so `kagami-session` owns the pose and
/// its bounds, and this type is what that pose looks like to `wgpu`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub target: Vec3,
    pub distance: f32,
    /// Radians, rotation around the world Z axis.
    pub yaw: f32,
    /// Radians, elevation above the target's horizontal plane.
    pub pitch: f32,
    /// How the scene is projected.
    pub projection: Projection,
}

/// Vertical field of view for [`Projection::Perspective`], in radians.
const VERTICAL_FOV: f32 = 45.0 * std::f32::consts::PI / 180.0;
const NEAR_PLANE: f32 = 0.1;
const FAR_PLANE: f32 = 5000.0;

impl Default for Camera {
    fn default() -> Self {
        Self {
            target: Vec3::ZERO,
            distance: 18.0,
            yaw: -45.0_f32.to_radians(),
            pitch: 35.0_f32.to_radians(),
            projection: Projection::Perspective,
        }
    }
}

impl Camera {
    /// Build a camera from plain components.
    ///
    /// Takes an array rather than a [`Vec3`] so a caller converting from a
    /// persisted pose does not have to depend on this crate's linear-algebra
    /// choice to hand one over — which is the whole reason the persisted pose
    /// is not a [`Vec3`] in the first place.
    pub const fn new(
        target: [f32; 3],
        distance: f32,
        yaw: f32,
        pitch: f32,
        projection: Projection,
    ) -> Self {
        Self {
            target: Vec3::new(target[0], target[1], target[2]),
            distance,
            yaw,
            pitch,
            projection,
        }
    }

    /// The unit vector from the target towards the camera.
    ///
    /// Analytic, so it is exactly unit-length at every pose. That matters
    /// more than it looks: it is what [`Self::view_matrix`] uses instead of
    /// recovering a direction by subtracting two points.
    pub fn offset_direction(&self) -> Vec3 {
        let cos_pitch = self.pitch.cos();
        Vec3::new(
            cos_pitch * self.yaw.cos(),
            cos_pitch * self.yaw.sin(),
            self.pitch.sin(),
        )
    }

    pub fn eye(&self) -> Vec3 {
        self.target + self.offset_direction() * self.distance
    }

    /// The world-to-view transform.
    ///
    /// Built from the *direction* rather than from an eye/target pair.
    /// `look_at` recovers the view direction by subtracting the two, and in
    /// `f32` that subtraction cancels completely once the target is large
    /// relative to the orbit distance — a target at 1e30 and a distance of
    /// 2000 give an eye that rounds to exactly the target, whereupon
    /// normalising a zero-length vector fills the matrix with NaN and the
    /// scene is undefined.
    ///
    /// The orbit camera already knows its direction analytically, so deriving
    /// it by subtraction was throwing away precision it had. This form
    /// degrades the way large coordinates should: the translation loses
    /// precision, and the orientation stays exact.
    pub fn view_matrix(&self) -> Mat4 {
        glam::camera::rh::view::look_to_mat4(self.eye(), -self.offset_direction(), Vec3::Z)
    }

    /// The projection matrix for the active [`Projection`].
    ///
    /// The orthographic extent is derived from the orbit distance and the
    /// perspective field of view, so it frames exactly what the perspective
    /// camera framed at the target plane. Switching projection therefore keeps
    /// the scene the same apparent size instead of jumping — which matters
    /// because the choice is meant to change how depth reads, not how big
    /// anything looks.
    pub fn projection_matrix(&self, aspect_ratio: f32) -> Mat4 {
        let aspect_ratio = aspect_ratio.max(0.01);
        match self.projection {
            Projection::Perspective => glam::camera::rh::proj::directx::perspective(
                VERTICAL_FOV,
                aspect_ratio,
                NEAR_PLANE,
                FAR_PLANE,
            ),
            Projection::Orthographic => {
                let half_height = self.distance * (VERTICAL_FOV / 2.0).tan();
                let half_width = half_height * aspect_ratio;
                glam::camera::rh::proj::directx::orthographic(
                    -half_width,
                    half_width,
                    -half_height,
                    half_height,
                    NEAR_PLANE,
                    FAR_PLANE,
                )
            }
        }
    }

    pub fn view_projection_matrix(&self, aspect_ratio: f32) -> Mat4 {
        self.projection_matrix(aspect_ratio) * self.view_matrix()
    }

    /// Where a render-space point lands, in normalised device coordinates.
    ///
    /// `None` when the point does not land anywhere: outside the depth range
    /// between the near and far planes, or through a matrix that arithmetic has
    /// made non-finite. A caller gets no coordinate rather than a fabricated
    /// one — which is what lets an out-of-range point be *omitted* instead of
    /// saturated onto a boundary position it does not occupy.
    ///
    /// # Both projections need the depth test, and only one needs the `w` test
    ///
    /// Under perspective, a point behind the eye has a negative `w`, so
    /// checking `w` alone catches it. Under **orthographic** it does not:
    /// `w` stays at one wherever the point is, so a point well behind the
    /// camera divides cleanly and produces perfectly plausible coordinates.
    /// What actually distinguishes it is depth. These matrices are built for
    /// the DirectX/WebGPU convention, where a visible point has NDC `z` in
    /// `[0, 1]`; behind the near plane is below zero and past the far plane is
    /// above one. Testing that is what makes the answer right for both.
    ///
    /// Takes and returns plain arrays so a caller need not adopt this crate's
    /// linear-algebra choice to ask the question.
    pub fn project(&self, point: [f32; 3], aspect_ratio: f32) -> Option<[f32; 3]> {
        let clip = self.view_projection_matrix(aspect_ratio)
            * Vec3::new(point[0], point[1], point[2]).extend(1.0);
        if !clip.is_finite() || clip.w <= 0.0 {
            return None;
        }
        let ndc = [clip.x / clip.w, clip.y / clip.w, clip.z / clip.w];
        if !ndc.iter().all(|value| value.is_finite()) {
            return None;
        }
        // The declared clipping policy, checked rather than assumed.
        (0.0..=1.0).contains(&ndc[2]).then_some(ndc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_projections_frame_the_target_plane_alike() {
        // The property the derived orthographic extent exists for: a point on
        // the target plane lands in the same place under either projection, so
        // switching does not resize the scene.
        let perspective = Camera::default();
        let orthographic = Camera {
            projection: Projection::Orthographic,
            ..perspective
        };
        let aspect = 16.0 / 9.0;

        // A point offset from the target within its own plane, reached by
        // stepping along the camera's right vector.
        let right = (perspective.target - perspective.eye())
            .normalize()
            .cross(Vec3::Z)
            .normalize();
        let point = perspective.target + right * 3.0;

        let project = |camera: &Camera| {
            let clip = camera.view_projection_matrix(aspect) * point.extend(1.0);
            Vec3::new(clip.x / clip.w, clip.y / clip.w, clip.z / clip.w)
        };
        let a = project(&perspective);
        let b = project(&orthographic);
        assert!((a.x - b.x).abs() < 1e-4, "{a:?} vs {b:?}");
        assert!((a.y - b.y).abs() < 1e-4, "{a:?} vs {b:?}");
    }

    #[test]
    fn an_orthographic_projection_does_not_shrink_with_depth() {
        // What makes it orthographic: two equal-length steps at different
        // depths span equally in clip space.
        let camera = Camera {
            projection: Projection::Orthographic,
            target: Vec3::ZERO,
            distance: 50.0,
            yaw: 0.0,
            pitch: 0.0,
        };
        let matrix = camera.view_projection_matrix(1.0);
        let span = |depth: f32| {
            let near = matrix * Vec3::new(depth, -1.0, 0.0).extend(1.0);
            let far = matrix * Vec3::new(depth, 1.0, 0.0).extend(1.0);
            (far.y / far.w - near.y / near.w).abs()
        };
        assert!((span(0.0) - span(-20.0)).abs() < 1e-5);
    }

    #[test]
    fn the_eye_sits_at_the_orbit_distance() {
        let camera = Camera::default();
        assert!((camera.eye().distance(camera.target) - camera.distance).abs() < 1e-4);
    }

    #[test]
    fn a_view_matrix_stays_finite_at_extreme_coordinates() {
        // Regression: `look_at` recovered the direction by subtracting eye
        // from target, and in f32 that cancels once the target dwarfs the
        // orbit distance — normalising the zero vector then filled every
        // entry with NaN. The direction is known analytically, so nothing
        // needs to be recovered.
        for magnitude in [0.0, 1.0, 1e6, 1e20, 1e30, f32::MAX / 4.0] {
            let camera = Camera {
                target: Vec3::splat(magnitude),
                distance: 2000.0,
                yaw: 0.4,
                pitch: 0.3,
                projection: Projection::Perspective,
            };
            let matrix = camera.view_projection_matrix(16.0 / 9.0);
            assert!(
                matrix.to_cols_array().iter().all(|entry| entry.is_finite()),
                "target {magnitude:e}: {matrix:?}"
            );
        }
    }

    #[test]
    fn a_point_behind_the_eye_is_omitted_under_both_projections() {
        // Regression: only `clip.w` was checked, which catches a behind-eye
        // point under perspective and nothing at all under orthographic —
        // where `w` stays at one and a point well behind the camera divided
        // cleanly into perfectly plausible coordinates.
        //
        // Camera at the origin looking down -x from (18, 0, 0), so anything at
        // larger x is behind it.
        for projection in [Projection::Perspective, Projection::Orthographic] {
            let camera = Camera::new([0.0, 0.0, 0.0], 18.0, 0.0, 0.0, projection);

            assert_eq!(
                camera.project([100.0, 0.0, 0.0], 1.0),
                None,
                "{projection:?} must omit a point behind the eye"
            );
            assert_eq!(
                camera.project([19.0, 0.0, 0.0], 1.0),
                None,
                "{projection:?} must omit a point nearer than the near plane"
            );
            assert_eq!(
                camera.project([-1.0e6, 0.0, 0.0], 1.0),
                None,
                "{projection:?} must omit a point beyond the far plane"
            );

            // And the target itself, squarely in front, does project.
            let ndc = camera
                .project([0.0, 0.0, 0.0], 1.0)
                .expect("the target is visible");
            assert!(
                ndc.iter().all(|value| value.is_finite()),
                "{projection:?}: {ndc:?}"
            );
            assert!(
                (0.0..=1.0).contains(&ndc[2]),
                "{projection:?}: depth {} is outside the declared range",
                ndc[2]
            );
        }
    }

    #[test]
    fn the_view_direction_is_exactly_unit_length() {
        // What makes the matrix above well-conditioned regardless of where the
        // target is.
        for (yaw, pitch) in [(0.0, 0.0), (0.4, 0.3), (-2.0, 1.5), (3.1, -1.5)] {
            let camera = Camera {
                yaw,
                pitch,
                ..Camera::default()
            };
            assert!(
                (camera.offset_direction().length() - 1.0).abs() < 1e-6,
                "{yaw} {pitch}"
            );
        }
    }
}

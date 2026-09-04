use glam::{Mat4, Vec3};

/// An orbit camera in spherical coordinates around a `target` point,
/// using a Z-up convention.
#[derive(Debug, Clone, Copy)]
pub struct Camera {
    pub target: Vec3,
    pub distance: f32,
    /// Radians, rotation around the world Z axis.
    pub yaw: f32,
    /// Radians, elevation above the target's horizontal plane.
    pub pitch: f32,
}

const MIN_DISTANCE: f32 = 1.0;
const MAX_DISTANCE: f32 = 2000.0;
const MAX_PITCH: f32 = 89.0 * std::f32::consts::PI / 180.0;
const ORBIT_SENSITIVITY: f32 = 0.008;
const PAN_SENSITIVITY: f32 = 0.0015;
const ZOOM_SENSITIVITY: f32 = 0.1;

impl Default for Camera {
    fn default() -> Self {
        Self {
            target: Vec3::ZERO,
            distance: 18.0,
            yaw: -45.0_f32.to_radians(),
            pitch: 35.0_f32.to_radians(),
        }
    }
}

impl Camera {
    pub fn eye(&self) -> Vec3 {
        let cos_pitch = self.pitch.cos();
        let direction = Vec3::new(
            cos_pitch * self.yaw.cos(),
            cos_pitch * self.yaw.sin(),
            self.pitch.sin(),
        );

        self.target + direction * self.distance
    }

    pub fn view_matrix(&self) -> Mat4 {
        glam::camera::rh::view::look_at_mat4(self.eye(), self.target, Vec3::Z)
    }

    pub fn projection_matrix(&self, aspect_ratio: f32) -> Mat4 {
        glam::camera::rh::proj::directx::perspective(
            45.0_f32.to_radians(),
            aspect_ratio.max(0.01),
            0.1,
            5000.0,
        )
    }

    pub fn view_projection_matrix(&self, aspect_ratio: f32) -> Mat4 {
        self.projection_matrix(aspect_ratio) * self.view_matrix()
    }

    pub fn orbit(&mut self, delta_x: f32, delta_y: f32) {
        self.yaw -= delta_x * ORBIT_SENSITIVITY;
        self.pitch = (self.pitch + delta_y * ORBIT_SENSITIVITY).clamp(-MAX_PITCH, MAX_PITCH);
    }

    pub fn pan(&mut self, delta_x: f32, delta_y: f32) {
        let forward = (self.target - self.eye()).normalize_or_zero();
        let right = forward.cross(Vec3::Z).normalize_or_zero();
        let up = right.cross(forward).normalize_or_zero();
        let scale = self.distance * PAN_SENSITIVITY;

        self.target += (-right * delta_x + up * delta_y) * scale;
    }

    pub fn dolly(&mut self, amount: f32) {
        self.distance =
            (self.distance * (1.0 - amount * ZOOM_SENSITIVITY)).clamp(MIN_DISTANCE, MAX_DISTANCE);
    }
}

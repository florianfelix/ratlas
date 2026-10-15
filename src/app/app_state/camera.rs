#[derive(Debug, Clone)]
pub struct Camera {
    pub position: glam::Vec3,
    pub target: glam::Vec3,
    pub up: glam::Vec3,
    pub fov: f32, // in radians
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            position: glam::Vec3::new(0.0, 0.2, 2.8),
            target: glam::Vec3::ZERO,
            up: glam::Vec3::Y,
            fov: 45.0_f32.to_radians(),
        }
    }
}

impl Camera {
    pub const SPHERE_RADIUS: f32 = 1.0;
    pub const MIN_ALTITUDE: f32 = 0.18; // Keep camera safely outside sphere and above near clip plane
    pub const MAX_DISTANCE: f32 = 20.0;

    pub fn new(position: glam::Vec3, target: glam::Vec3, up: glam::Vec3, fov: f32) -> Self {
        Self {
            position,
            target,
            up,
            fov,
        }
    }

    /// Resets the camera to default position, target, and orientation
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Computes the view matrix looking at `target`
    pub fn view_matrix(&self) -> glam::Mat4 {
        glam::camera::rh::view::look_at_mat4(self.position, self.target, self.up)
    }

    /// Computes the WebGPU perspective projection matrix
    pub fn projection_matrix(&self, aspect_ratio: f32) -> glam::Mat4 {
        glam::camera::rh::proj::directx::perspective(self.fov, aspect_ratio.max(0.1), 0.1, 100.0)
    }

    /// Rotates the camera position around `target` using spherical coordinates
    /// delta_x: horizontal mouse motion (azimuth)
    /// delta_y: vertical mouse motion (elevation)
    pub fn rotate_around_target(&mut self, delta_x: f32, delta_y: f32) {
        let offset = self.position - self.target;
        let radius = offset.length();
        if radius < 0.001 {
            return;
        }

        // Spherical coordinates
        // theta: azimuthal angle around Y in XZ plane
        let mut theta = offset.z.atan2(offset.x);
        // phi: elevation angle from XZ plane towards Y
        let mut phi = (offset.y / radius).clamp(-1.0, 1.0).asin();

        // Adjust angles with sensitivity
        let sensitivity = 0.005;
        theta += delta_x * sensitivity;
        phi = (phi + delta_y * sensitivity).clamp(-89.0_f32.to_radians(), 89.0_f32.to_radians());

        // Convert back to cartesian coordinates
        let cos_phi = phi.cos();
        let new_offset = glam::Vec3::new(
            radius * cos_phi * theta.cos(),
            radius * phi.sin(),
            radius * cos_phi * theta.sin(),
        );

        self.position = self.target + new_offset;
    }

    /// Zoom in / out with step size proportional to altitude above the sphere surface,
    /// clamped to always remain outside the sphere.
    pub fn zoom(&mut self, delta: f32) {
        let offset = self.position - self.target;
        let current_radius = offset.length();
        if current_radius < 0.001 {
            return;
        }

        let min_radius = Self::SPHERE_RADIUS + Self::MIN_ALTITUDE;
        let max_radius = Self::MAX_DISTANCE;

        // Step size is proportional to altitude above the sphere surface (slower/finer closer in)
        let altitude = (current_radius - Self::SPHERE_RADIUS).max(0.0);
        let step = delta * (altitude + 0.15);
        let new_radius = (current_radius - step).clamp(min_radius, max_radius);

        self.position = self.target + offset * (new_radius / current_radius);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zoom_step_proportional_to_distance() {
        let mut cam_far = Camera {
            position: glam::Vec3::new(0.0, 0.0, 5.0),
            target: glam::Vec3::ZERO,
            up: glam::Vec3::Y,
            fov: 45.0_f32.to_radians(),
        };
        let mut cam_near = Camera {
            position: glam::Vec3::new(0.0, 0.0, 1.2),
            target: glam::Vec3::ZERO,
            up: glam::Vec3::Y,
            fov: 45.0_f32.to_radians(),
        };

        cam_far.zoom(0.1);
        cam_near.zoom(0.1);

        let delta_far = 5.0 - (cam_far.position - cam_far.target).length();
        let delta_near = 1.2 - (cam_near.position - cam_near.target).length();

        // Step size when far away should be strictly greater than step size when close to the sphere
        assert!(delta_far > delta_near);
    }

    #[test]
    fn test_zoom_capped_outside_sphere() {
        let mut cam = Camera::default();
        // Zoom in aggressively
        for _ in 0..100 {
            cam.zoom(5.0);
        }

        let dist = (cam.position - cam.target).length();
        assert!(
            dist >= Camera::SPHERE_RADIUS + Camera::MIN_ALTITUDE,
            "Camera distance {dist} penetrated inside sphere surface!"
        );
        assert!(dist > Camera::SPHERE_RADIUS);
    }
}

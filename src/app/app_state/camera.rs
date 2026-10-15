#[derive(Debug, Clone)]
pub struct Camera {
    pub position: glam::Vec3,
    pub target: glam::Vec3,
    pub up: glam::Vec3,
    pub fov: f32, // in radians

    // Spherical coordinates
    pub theta: f32,  // Azimuth angle (horizontal rotation around Y)
    pub phi: f32,    // Elevation angle (vertical angle from XZ plane)
    pub radius: f32, // Current distance to target

    // Damped targets for smooth interpolation
    pub target_theta: f32,
    pub target_phi: f32,
    pub target_radius: f32,

    // Inertial angular velocity for natural momentum
    pub velocity: glam::Vec2,
}

impl Default for Camera {
    fn default() -> Self {
        let position = glam::Vec3::new(0.0, 0.2, 2.8);
        let target = glam::Vec3::ZERO;
        let up = glam::Vec3::Y;
        let fov = 45.0_f32.to_radians();

        let offset = position - target;
        let radius = offset.length();
        let theta = offset.z.atan2(offset.x);
        let phi = (offset.y / radius).clamp(-1.0, 1.0).asin();

        Self {
            position,
            target,
            up,
            fov,
            theta,
            phi,
            radius,
            target_theta: theta,
            target_phi: phi,
            target_radius: radius,
            velocity: glam::Vec2::ZERO,
        }
    }
}

impl Camera {
    pub const SPHERE_RADIUS: f32 = 1.0;
    pub const MIN_ALTITUDE: f32 = 0.18; // Keep camera safely outside sphere and above near clip plane
    pub const MAX_DISTANCE: f32 = 20.0;

    pub fn new(position: glam::Vec3, target: glam::Vec3, up: glam::Vec3, fov: f32) -> Self {
        let offset = position - target;
        let radius = offset
            .length()
            .max(Self::SPHERE_RADIUS + Self::MIN_ALTITUDE);
        let theta = offset.z.atan2(offset.x);
        let phi = (offset.y / radius).clamp(-1.0, 1.0).asin();

        Self {
            position,
            target,
            up,
            fov,
            theta,
            phi,
            radius,
            target_theta: theta,
            target_phi: phi,
            target_radius: radius,
            velocity: glam::Vec2::ZERO,
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

    /// Rotates the camera target around the center using distance-scaled sensitivity and momentum.
    /// delta_x: horizontal mouse motion (azimuth)
    /// delta_y: vertical mouse motion (elevation)
    pub fn rotate_around_target(&mut self, delta_x: f32, delta_y: f32) {
        // Distance-scaled sensitivity: when zoomed in close to the sphere surface,
        // scale angular speed with altitude so that 1 pixel of mouse movement corresponds to
        // a smooth, high-resolution translation on the surface rather than a huge jump.
        let altitude = (self.target_radius - Self::SPHERE_RADIUS).max(0.01);
        let sensitivity = 0.003 * (altitude / 1.8).clamp(0.06, 1.2);

        let d_theta = delta_x * sensitivity;
        let d_phi = delta_y * sensitivity;

        self.target_theta += d_theta;
        self.target_phi =
            (self.target_phi + d_phi).clamp(-89.0_f32.to_radians(), 89.0_f32.to_radians());

        // Smoothly blend mouse movement into velocity
        self.velocity = self.velocity * 0.3 + glam::Vec2::new(d_theta, d_phi) * 0.7;
    }

    /// Handles mouse release to clamp momentum velocity gracefully
    pub fn stop_drag(&mut self) {
        self.velocity = self.velocity.clamp_length_max(0.025);
    }

    /// Zoom in / out with step size proportional to altitude above the sphere surface,
    /// clamped to always remain outside the sphere.
    pub fn zoom(&mut self, delta: f32) {
        let min_radius = Self::SPHERE_RADIUS + Self::MIN_ALTITUDE;
        let max_radius = Self::MAX_DISTANCE;

        // Step size is proportional to altitude above the sphere surface (slower/finer closer in)
        let altitude = (self.target_radius - Self::SPHERE_RADIUS).max(0.0);
        let step = delta * (altitude + 0.15);
        self.target_radius = (self.target_radius - step).clamp(min_radius, max_radius);
    }

    /// Smoothly updates camera spherical coordinates toward targets using damping and inertia.
    /// Returns true if the camera is still actively moving (requiring continuous frame redraws).
    pub fn update(&mut self) -> bool {
        let damping = 0.22_f32; // Smooth and responsive exponential ease

        // Apply and decay momentum velocity
        self.velocity *= 0.88;
        if self.velocity.length_squared() > 1e-7 {
            self.target_theta += self.velocity.x;
            self.target_phi = (self.target_phi + self.velocity.y)
                .clamp(-89.0_f32.to_radians(), 89.0_f32.to_radians());
        } else {
            self.velocity = glam::Vec2::ZERO;
        }

        let d_theta = self.target_theta - self.theta;
        let d_phi = self.target_phi - self.phi;
        let d_radius = self.target_radius - self.radius;

        let is_moving = d_theta.abs() > 1e-5
            || d_phi.abs() > 1e-5
            || d_radius.abs() > 1e-5
            || self.velocity.length_squared() > 1e-7;

        if is_moving {
            self.theta += d_theta * damping;
            self.phi += d_phi * damping;
            self.radius += d_radius * damping;

            self.update_cartesian_position();
        } else {
            self.theta = self.target_theta;
            self.phi = self.target_phi;
            self.radius = self.target_radius;
            self.update_cartesian_position();
        }

        is_moving
    }

    fn update_cartesian_position(&mut self) {
        let cos_phi = self.phi.cos();
        let new_offset = glam::Vec3::new(
            self.radius * cos_phi * self.theta.cos(),
            self.radius * self.phi.sin(),
            self.radius * cos_phi * self.theta.sin(),
        );
        self.position = self.target + new_offset;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zoom_step_proportional_to_distance() {
        let mut cam_far = Camera::new(
            glam::Vec3::new(0.0, 0.0, 5.0),
            glam::Vec3::ZERO,
            glam::Vec3::Y,
            45.0_f32.to_radians(),
        );
        let mut cam_near = Camera::new(
            glam::Vec3::new(0.0, 0.0, 1.2),
            glam::Vec3::ZERO,
            glam::Vec3::Y,
            45.0_f32.to_radians(),
        );

        cam_far.zoom(0.1);
        cam_near.zoom(0.1);

        let delta_far = 5.0 - cam_far.target_radius;
        let delta_near = 1.2 - cam_near.target_radius;

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

        assert!(
            cam.target_radius >= Camera::SPHERE_RADIUS + Camera::MIN_ALTITUDE,
            "Camera target radius penetrated inside sphere surface!"
        );

        // Step animation to completion
        for _ in 0..100 {
            cam.update();
        }

        let dist = (cam.position - cam.target).length();
        assert!(
            dist >= Camera::SPHERE_RADIUS + Camera::MIN_ALTITUDE - 1e-4,
            "Camera distance {dist} penetrated inside sphere surface!"
        );
    }

    #[test]
    fn test_distance_scaled_rotation_sensitivity() {
        let mut cam_far = Camera::new(
            glam::Vec3::new(0.0, 0.0, 5.0),
            glam::Vec3::ZERO,
            glam::Vec3::Y,
            45.0_f32.to_radians(),
        );
        let mut cam_near = Camera::new(
            glam::Vec3::new(0.0, 0.0, 1.2),
            glam::Vec3::ZERO,
            glam::Vec3::Y,
            45.0_f32.to_radians(),
        );

        cam_far.rotate_around_target(10.0, 0.0);
        cam_near.rotate_around_target(10.0, 0.0);

        let delta_theta_far = (cam_far.target_theta - cam_far.theta).abs();
        let delta_theta_near = (cam_near.target_theta - cam_near.theta).abs();

        // Rotation sensitivity near surface must be finer (smaller angle per mouse pixel) than far away
        assert!(delta_theta_near < delta_theta_far);
    }
}

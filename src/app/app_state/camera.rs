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
    pub fn new(position: glam::Vec3, target: glam::Vec3, up: glam::Vec3, fov: f32) -> Self {
        Self {
            position,
            target,
            up,
            fov,
        }
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

    /// Zoom in / out by scaling the distance to target
    pub fn zoom(&mut self, delta: f32) {
        let offset = self.position - self.target;
        let radius = offset.length();
        let new_radius = (radius - delta).clamp(0.5, 50.0);
        if radius > 0.001 {
            self.position = self.target + offset * (new_radius / radius);
        }
    }
}

use std::sync::Arc;

use crate::{
    app::app_state::{AppState, canvas_renderer::CanvasRenderer},
    data::material::Material,
};

impl AppState {
    pub fn set_earth_base_texture(
        &mut self,
        canvas_renderer: &CanvasRenderer,
        texture: EarthBaseTexture,
    ) {
        if self.earth_config.base_texture == texture {
            return;
        }
        self.earth_config.base_texture = texture;
        let resolved_path = crate::data::resolve_asset_path(texture.asset_path());
        let new_material = Arc::new(Material::from_image_path(
            &canvas_renderer.gpu.device,
            &canvas_renderer.gpu.queue,
            canvas_renderer.pipelines.default_pipeline.clone(),
            &canvas_renderer.pipelines.texture_layout,
            &resolved_path,
        ));
        for obj in self.world.get_by_tag_mut("earth") {
            obj.set_material(new_material.clone());
        }
    }

    pub fn cycle_earth_base_texture(&mut self, canvas_renderer: &CanvasRenderer) {
        let next_tex = match self.earth_config.base_texture {
            EarthBaseTexture::Day => EarthBaseTexture::Night,
            EarthBaseTexture::Night => EarthBaseTexture::Specular,
            EarthBaseTexture::Specular => EarthBaseTexture::Day,
        };
        self.set_earth_base_texture(canvas_renderer, next_tex);
    }

    pub fn set_show_country_outlines(&mut self, show: bool) {
        self.earth_config.show_country_outlines = show;
        for obj in self.world.get_by_tag_mut("country_outlines") {
            obj.visible = show;
        }
    }

    pub fn toggle_country_outlines(&mut self) {
        let new_show = !self.earth_config.show_country_outlines;
        self.set_show_country_outlines(new_show);
    }

    pub fn set_country_outline_color(&mut self, color: glam::Vec4) {
        self.earth_config.outline_color = color;
        for obj in self.world.get_by_tag_mut("country_outlines") {
            obj.color_tint = color;
        }
    }
    /// Smoothly animates camera rotation around the globe until it is centered
    /// over the active cluster of station markers.
    pub fn focus_on_country(&mut self) {
        let mut sum = glam::Vec3::ZERO;
        let mut count = 0;
        for marker in &self.marker_instances {
            sum += marker.position;
            count += 1;
        }

        if count > 0 {
            let center_normal = (sum / count as f32).normalize();
            // Smoothly fly camera to face the centroid of the country's station cluster
            // Gently bring the camera into a comfortable viewing altitude (~2.1 units)
            let target_dist = self.camera.radius.clamp(1.8, 2.3);
            self.camera
                .fly_to_direction(center_normal, Some(target_dist));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EarthBaseTexture {
    #[default]
    Day,
    Night,
    Specular,
}

impl EarthBaseTexture {
    #[allow(dead_code)]
    pub const ALL: [EarthBaseTexture; 3] = [
        EarthBaseTexture::Day,
        EarthBaseTexture::Night,
        EarthBaseTexture::Specular,
    ];

    pub fn asset_path(&self) -> &'static str {
        match self {
            EarthBaseTexture::Day => "assets/sphere_diffuse.jpg",
            EarthBaseTexture::Night => "assets/sphere_diffuse_night.jpg",
            EarthBaseTexture::Specular => "assets/8k_earth_specular_map.jpg",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            EarthBaseTexture::Day => "Day",
            EarthBaseTexture::Night => "Night",
            EarthBaseTexture::Specular => "Specular",
        }
    }
}

impl std::fmt::Display for EarthBaseTexture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EarthConfig {
    pub show_country_outlines: bool,
    pub outline_color: glam::Vec4,
    pub base_texture: EarthBaseTexture,
}

impl Default for EarthConfig {
    fn default() -> Self {
        Self {
            show_country_outlines: false,
            outline_color: glam::Vec4::new(0.2, 0.8, 1.0, 0.85),
            base_texture: EarthBaseTexture::default(),
        }
    }
}

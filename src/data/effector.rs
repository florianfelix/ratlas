use glam::{Vec3, Vec4};

#[derive(Debug, Clone)]
pub enum EffectorKind {
    /// Distance-based scale down (e.g. parented to camera position):
    /// When distance is close (d <= min_dist), scales down to min_scale.
    /// When distance is far (d >= max_dist), scales up to max_scale.
    /// Uses smoothstep S-curve interpolation between min_dist and max_dist.
    DistanceProximity {
        min_dist: f32,
        max_dist: f32,
        min_scale: f32,
        max_scale: f32,
    },
    /// Spherical lens / magnifier (e.g. positioned at mouse point on globe):
    /// Objects within radius r are magnified with a smoothstep S-curve:
    /// At center (dist = 0): scale multiplier is (1.0 + max_boost)
    /// At boundary (dist >= radius): scale multiplier is 1.0
    /// Also supports optional color tint boost inside the lens.
    SphericalLens {
        radius: f32,
        max_boost: f32,
        tint: Option<Vec4>,
    },
    /// Spherical color tint effector:
    /// Applies color tint within radius with smoothstep falloff.
    ColorField {
        radius: f32,
        tint: Vec4,
        intensity: f32,
    },
    /// Standalone picking effector:
    /// Encapsulates a spatial selection sphere for hover and click picking,
    /// decoupling picking radius and logic from visual lens scaling.
    Picker { radius: f32 },
}

/// Standalone picking effector:
/// Encapsulates spatial selection queries within an active radius.
/// Can be used as a standalone object or inserted into the general `Effector` list.
#[derive(Debug, Clone)]
pub struct PickingEffector {
    pub name: String,
    pub enabled: bool,
    pub position: Vec3,
    pub radius: f32,
}

impl PickingEffector {
    pub fn new(radius: f32) -> Self {
        Self {
            name: "MousePickingEffector".to_string(),
            enabled: true,
            position: Vec3::ZERO,
            radius,
        }
    }

    /// Evaluates candidate items against this picker's center and radius.
    /// Returns the closest item within radius: `Some((item, distance))`.
    pub fn pick_closest<T>(
        &self,
        candidates: impl IntoIterator<Item = (T, Vec3)>,
    ) -> Option<(T, f32)> {
        if !self.enabled {
            return None;
        }

        let mut closest: Option<(T, f32)> = None;
        for (item, pos) in candidates {
            let dist = (pos - self.position).length();
            if dist <= self.radius {
                if closest.as_ref().map_or(true, |(_, min_d)| dist < *min_d) {
                    closest = Some((item, dist));
                }
            }
        }
        closest
    }

    /// Converts this picking effector into a general `Effector`.
    pub fn to_effector(&self) -> Effector {
        Effector {
            name: self.name.clone(),
            enabled: self.enabled,
            position: self.position,
            kind: EffectorKind::Picker {
                radius: self.radius,
            },
        }
    }
}

impl From<PickingEffector> for Effector {
    fn from(p: PickingEffector) -> Self {
        p.to_effector()
    }
}

#[derive(Debug, Clone)]
pub struct Effector {
    pub name: String,
    pub enabled: bool,
    pub position: Vec3,
    pub kind: EffectorKind,
}

impl Effector {
    pub fn new_camera_proximity(
        min_dist: f32,
        max_dist: f32,
        min_scale: f32,
        max_scale: f32,
    ) -> Self {
        Self {
            name: "CameraProximityEffector".to_string(),
            enabled: true,
            position: Vec3::ZERO,
            kind: EffectorKind::DistanceProximity {
                min_dist,
                max_dist,
                min_scale,
                max_scale,
            },
        }
    }

    pub fn new_mouse_lens(radius: f32, max_boost: f32, tint: Option<Vec4>) -> Self {
        Self {
            name: "MouseGlobeLens".to_string(),
            enabled: true,
            position: Vec3::ZERO,
            kind: EffectorKind::SphericalLens {
                radius,
                max_boost,
                tint,
            },
        }
    }

    pub fn new_picker(radius: f32) -> Self {
        PickingEffector::new(radius).into()
    }

    pub fn is_picker(&self) -> bool {
        matches!(self.kind, EffectorKind::Picker { .. })
    }

    /// Returns the active spherical radius of this effector if it has one.
    pub fn radius(&self) -> Option<f32> {
        match &self.kind {
            EffectorKind::SphericalLens { radius, .. } => Some(*radius),
            EffectorKind::ColorField { radius, .. } => Some(*radius),
            EffectorKind::Picker { radius } => Some(*radius),
            EffectorKind::DistanceProximity { .. } => None,
        }
    }

    /// Evaluates candidate items against this effector's position and radius.
    /// Returns the closest item that falls within the radius: `Some((item, distance))`.
    pub fn pick_closest<T>(
        &self,
        candidates: impl IntoIterator<Item = (T, Vec3)>,
    ) -> Option<(T, f32)> {
        if !self.enabled {
            return None;
        }
        let radius = self.radius()?;
        let mut closest: Option<(T, f32)> = None;

        for (item, pos) in candidates {
            let dist = (pos - self.position).length();
            if dist <= radius {
                if closest.as_ref().map_or(true, |(_, min_d)| dist < *min_d) {
                    closest = Some((item, dist));
                }
            }
        }

        closest
    }

    /// Evaluates the scale multiplier and color tint contribution for a target point in world space.
    /// Returns (scale_multiplier, optional_color_tint_add).
    pub fn evaluate(&self, target_pos: Vec3) -> (f32, Option<Vec4>) {
        if !self.enabled {
            return (1.0, None);
        }

        match &self.kind {
            EffectorKind::DistanceProximity {
                min_dist,
                max_dist,
                min_scale,
                max_scale,
            } => {
                let dist = (target_pos - self.position).length();
                let t = ((dist - min_dist) / (max_dist - min_dist).max(1e-4)).clamp(0.0, 1.0);
                // Smoothstep S-curve
                let s = t * t * (3.0 - 2.0 * t);
                let scale = min_scale + (max_scale - min_scale) * s;
                (scale, None)
            }
            EffectorKind::SphericalLens {
                radius,
                max_boost,
                tint,
            } => {
                let dist = (target_pos - self.position).length();
                if dist >= *radius {
                    return (1.0, None);
                }

                // S-curve falloff: 1.0 at center, 0.0 at edge
                let normalized = 1.0 - (dist / radius.max(1e-4));
                let s_curve = normalized * normalized * (3.0 - 2.0 * normalized);

                let scale_mult = 1.0 + max_boost * s_curve;
                let color_out = tint.map(|t| t * s_curve);
                (scale_mult, color_out)
            }
            EffectorKind::ColorField {
                radius,
                tint,
                intensity,
            } => {
                let dist = (target_pos - self.position).length();
                if dist >= *radius {
                    return (1.0, None);
                }
                let normalized = 1.0 - (dist / radius.max(1e-4));
                let s_curve = normalized * normalized * (3.0 - 2.0 * normalized);
                let effective_tint = *tint * (s_curve * intensity);
                (1.0, Some(effective_tint))
            }
            EffectorKind::Picker { .. } => {
                // Picking effector does not alter base geometry scale or color tint
                (1.0, None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_distance_proximity_effector() {
        let mut effector = Effector::new_camera_proximity(0.2, 2.0, 0.4, 1.0);
        effector.position = Vec3::new(0.0, 0.0, 3.0);

        // Near target (dist = 0.2)
        let (scale_near, _) = effector.evaluate(Vec3::new(0.0, 0.0, 2.8));
        assert!((scale_near - 0.4).abs() < 1e-3);

        // Far target (dist = 2.0)
        let (scale_far, _) = effector.evaluate(Vec3::new(0.0, 0.0, 1.0));
        assert!((scale_far - 1.0).abs() < 1e-3);

        // Mid target: should be strictly between 0.4 and 1.0
        let (scale_mid, _) = effector.evaluate(Vec3::new(0.0, 0.0, 1.9));
        assert!(scale_mid > 0.4 && scale_mid < 1.0);
    }

    #[test]
    fn test_spherical_lens_s_curve() {
        let mut effector = Effector::new_mouse_lens(0.5, 1.5, Some(Vec4::new(0.1, 0.2, 0.3, 0.0)));
        effector.position = Vec3::new(1.0, 0.0, 0.0);

        // Exact center: maximum boost (1.0 + 1.5 = 2.5)
        let (scale_center, tint_center) = effector.evaluate(Vec3::new(1.0, 0.0, 0.0));
        assert!((scale_center - 2.5).abs() < 1e-4);
        assert!(tint_center.is_some());

        // Beyond radius: no boost (1.0)
        let (scale_outside, tint_outside) = effector.evaluate(Vec3::new(1.6, 0.0, 0.0));
        assert!((scale_outside - 1.0).abs() < 1e-4);
        assert!(tint_outside.is_none());
    }

    #[test]
    fn test_standalone_picking_effector() {
        let mut picker = PickingEffector::new(0.05);
        picker.position = Vec3::new(0.0, 0.0, 1.0);

        let items = vec![
            ("item_close", Vec3::new(0.01, 0.0, 1.0)), // dist = 0.01 <= 0.05
            ("item_farther", Vec3::new(0.04, 0.0, 1.0)), // dist = 0.04 <= 0.05
            ("item_outside", Vec3::new(0.10, 0.0, 1.0)), // dist = 0.10 > 0.05
        ];

        let result = picker.pick_closest(items.clone());
        assert_eq!(result.map(|(id, _)| id), Some("item_close"));

        // If disabled, returns None
        picker.enabled = false;
        let disabled_result = picker.pick_closest(items.clone());
        assert_eq!(disabled_result, None);

        // Test conversion to general Effector and picking through Effector
        let mut general_picker: Effector = PickingEffector::new(0.05).into();
        general_picker.position = Vec3::new(0.0, 0.0, 1.0);
        assert!(general_picker.is_picker());
        assert_eq!(general_picker.radius(), Some(0.05));

        let general_result = general_picker.pick_closest(items);
        assert_eq!(general_result.map(|(id, _)| id), Some("item_close"));
    }
}

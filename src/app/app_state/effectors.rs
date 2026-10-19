use crate::app::app_state::AppState;

impl AppState {
    /// Evaluates all active effectors and updates marker scales and color tints.
    /// Also performs horizon/backface culling on spherical station markers to skip
    /// occluded markers on the back side of the Earth.
    pub fn apply_effectors(&mut self) {
        // Sync camera effector position with camera
        if let Some(cam_eff) = self
            .effectors
            .iter_mut()
            .find(|e| e.name == "CameraProximityEffector")
        {
            cam_eff.position = self.camera.position;
        }

        // Sync mouse lens and picking effector positions with mouse globe hit point
        for effector in &mut self.effectors {
            if effector.name == "MouseGlobeLens" || effector.is_picker() {
                if let Some(globe_pos) = self.mouse_globe_pos {
                    effector.enabled = true;
                    effector.position = globe_pos;
                } else {
                    effector.enabled = false;
                }
            }
        }

        let cam_pos = self.camera.position;
        // Small margin (-0.05) ensures markers right at the horizon silhouette
        // don't pop out while their 3D geometry is still partially visible.
        const HORIZON_CULL_MARGIN: f32 = -0.05;

        let cam_eff_opt = self
            .effectors
            .iter()
            .find(|e| e.name == "CameraProximityEffector");

        for marker in &mut self.marker_instances {
            // Horizon / Backface Culling:
            // Test if the marker's outward normal is facing the camera.
            // For a sphere centered at origin, marker.position points along outward normal.
            // to_cam = cam_pos - marker.position. If marker.position.dot(to_cam) <= margin,
            // the marker is behind the horizon of the Earth sphere.
            let to_cam = cam_pos - marker.position;
            let is_visible = marker.position.dot(to_cam) > HORIZON_CULL_MARGIN;
            marker.visible = is_visible;

            if !is_visible {
                continue;
            }

            let mut current_scale = marker.base_scale;
            if let Some(cam_eff) = cam_eff_opt {
                let (scale_mult, _) = cam_eff.evaluate(marker.position);
                current_scale *= scale_mult;
            }

            marker.scale = current_scale;
            marker.color_tint = glam::Vec4::ONE;
        }

        // Mouse Globe Lens: only evaluate the few stations within spatial grid radius
        if let Some(globe_pos) = self.mouse_globe_pos {
            if let Some(lens_eff) = self.effectors.iter().find(|e| e.name == "MouseGlobeLens") {
                if lens_eff.enabled {
                    if let Some(radius) = lens_eff.radius() {
                        let nearby = self.spatial_grid.query_radius(globe_pos, radius);
                        for station in nearby {
                            if station.instance_index < self.marker_instances.len() {
                                let marker = &mut self.marker_instances[station.instance_index];
                                if marker.visible {
                                    let (scale_mult, tint_opt) = lens_eff.evaluate(marker.position);
                                    marker.scale *= scale_mult;
                                    if let Some(t) = tint_opt {
                                        marker.color_tint += t;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Apply hover and selection highlights
        if let Some(ref hovered_id) = self.stations.hovered {
            for marker in &mut self.marker_instances {
                if &marker.id == hovered_id {
                    marker.scale *= 1.25;
                    marker.color_tint *= glam::Vec4::new(0.7, 1.3, 1.5, 1.0);
                    break;
                }
            }
        }
        if let Some(ref selected_id) = self.stations.selected {
            for marker in &mut self.marker_instances {
                if &marker.id == selected_id {
                    marker.scale *= 1.15;
                    marker.color_tint *= glam::Vec4::new(1.3, 1.1, 0.4, 1.0);
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::app_state::{dummy_station, Ray};

    #[test]
    fn test_picking_effector_and_hover() {
        let mut state = AppState::default();
        let stations = vec![dummy_station("1", "Pick Me")];
        state.update_station_markers(stations);

        let target_pos = state.marker_instances[0].position;
        let ray = Ray {
            origin: target_pos.normalize() * 3.0,
            direction: -target_pos.normalize(),
        };

        let picked = state.pick_station(&ray);
        assert!(picked.is_some());
        assert_eq!(picked.unwrap().stationuuid, "1");

        // Set hovered station
        let changed = state.set_hovered_station(Some("1".to_string()));
        assert!(changed);
        assert_eq!(state.stations.hovered, Some("1".to_string()));

        // Effector scaled the hovered marker up by 1.25
        assert!((state.marker_instances[0].scale.x - 0.06 * 1.25).abs() < 1e-4);
    }
}

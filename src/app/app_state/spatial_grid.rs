use glam::Vec3;

#[derive(Debug, Clone)]
pub struct SpatialStation {
    pub uuid: String,
    pub position: Vec3,
    pub lat: f32,
    pub lon: f32,
    pub instance_index: usize,
}

#[derive(Debug, Clone)]
pub struct GlobeSpatialGrid {
    pub cell_size_deg: f32,
    pub num_rows: usize, // Latitude bins (-90° to +90°)
    pub num_cols: usize, // Longitude bins (-180° to +180°)
    pub cells: Vec<Vec<SpatialStation>>,
}

impl Default for GlobeSpatialGrid {
    fn default() -> Self {
        Self::new(10.0) // 10 degree bins: 18 rows x 36 cols = 648 cells
    }
}

impl GlobeSpatialGrid {
    pub fn new(cell_size_deg: f32) -> Self {
        let cell_size = cell_size_deg.max(1.0);
        let num_rows = (180.0 / cell_size).round() as usize;
        let num_cols = (360.0 / cell_size).round() as usize;
        let total_cells = num_rows * num_cols;

        Self {
            cell_size_deg: cell_size,
            num_rows,
            num_cols,
            cells: vec![Vec::new(); total_cells],
        }
    }

    /// Clears all cells in the grid.
    pub fn clear(&mut self) {
        for cell in &mut self.cells {
            cell.clear();
        }
    }

    /// Converts a 3D cartesian point on/near the sphere to latitude and longitude (in degrees).
    /// Aligns with the equirectangular projection used in `RenderObject::sphere_coords`.
    pub fn cartesian_to_lat_lon(pos: Vec3) -> (f32, f32) {
        let r = pos.length();
        if r < 1e-6 {
            return (0.0, 0.0);
        }
        let lat_rad = (pos.y / r).clamp(-1.0, 1.0).asin();
        let lon_rad = (-pos.z).atan2(pos.x);
        (lat_rad.to_degrees(), lon_rad.to_degrees())
    }

    /// Maps latitude and longitude to grid cell indices `(row, col)`.
    pub fn lat_lon_to_cell(&self, lat: f32, lon: f32) -> (usize, usize) {
        let clamped_lat = lat.clamp(-90.0, 90.0);
        let row =
            (((clamped_lat + 90.0) / self.cell_size_deg).floor() as usize).min(self.num_rows - 1);

        let norm_lon = (lon + 180.0).rem_euclid(360.0);
        let col = (((norm_lon) / self.cell_size_deg).floor() as usize).min(self.num_cols - 1);

        (row, col)
    }

    #[inline]
    pub fn cell_index(&self, row: usize, col: usize) -> usize {
        row * self.num_cols + col
    }

    /// Inserts a spatial station entry into the appropriate grid cell.
    pub fn insert(&mut self, station: SpatialStation) {
        let (row, col) = self.lat_lon_to_cell(station.lat, station.lon);
        let idx = self.cell_index(row, col);
        self.cells[idx].push(station);
    }

    /// Finds the closest spatial station to `center` within the given 3D distance `radius`.
    /// Only inspects the grid cells that intersect the spherical cap defined by `radius`.
    pub fn query_closest(&self, center: Vec3, radius: f32) -> Option<(&SpatialStation, f32)> {
        let (center_lat, center_lon) = Self::cartesian_to_lat_lon(center);
        let (center_row, center_col) = self.lat_lon_to_cell(center_lat, center_lon);

        let delta_deg = radius.to_degrees();
        let delta_rows = (delta_deg / self.cell_size_deg).ceil() as i32 + 1;

        let cos_lat = center_lat.to_radians().cos().abs();
        let delta_cols = if cos_lat < 0.15 {
            // Near the poles, longitude meridians converge; check all columns
            self.num_cols as i32
        } else {
            ((delta_deg / (self.cell_size_deg * cos_lat)).ceil() as i32 + 1)
                .min(self.num_cols as i32)
        };

        let mut closest: Option<(&SpatialStation, f32)> = None;
        let r_min = (center_row as i32 - delta_rows).max(0) as usize;
        let r_max = (center_row as i32 + delta_rows).min(self.num_rows as i32 - 1) as usize;

        for r in r_min..=r_max {
            for c_offset in -delta_cols..=delta_cols {
                let c = (center_col as i32 + c_offset).rem_euclid(self.num_cols as i32) as usize;
                let cell_idx = self.cell_index(r, c);
                for station in &self.cells[cell_idx] {
                    let dist = (station.position - center).length();
                    let current_max = closest.as_ref().map_or(radius, |(_, min_d)| *min_d);
                    if dist <= current_max {
                        closest = Some((station, dist));
                    }
                }
            }
        }

        closest
    }

    /// Returns all spatial stations within 3D distance `radius` of `center`.
    pub fn query_radius(&self, center: Vec3, radius: f32) -> Vec<&SpatialStation> {
        let (center_lat, center_lon) = Self::cartesian_to_lat_lon(center);
        let (center_row, center_col) = self.lat_lon_to_cell(center_lat, center_lon);

        let delta_deg = radius.to_degrees();
        let delta_rows = (delta_deg / self.cell_size_deg).ceil() as i32 + 1;

        let cos_lat = center_lat.to_radians().cos().abs();
        let delta_cols = if cos_lat < 0.15 {
            self.num_cols as i32
        } else {
            ((delta_deg / (self.cell_size_deg * cos_lat)).ceil() as i32 + 1)
                .min(self.num_cols as i32)
        };

        let mut results = Vec::new();
        let r_min = (center_row as i32 - delta_rows).max(0) as usize;
        let r_max = (center_row as i32 + delta_rows).min(self.num_rows as i32 - 1) as usize;

        for r in r_min..=r_max {
            for c_offset in -delta_cols..=delta_cols {
                let c = (center_col as i32 + c_offset).rem_euclid(self.num_cols as i32) as usize;
                let cell_idx = self.cell_index(r, c);
                for station in &self.cells[cell_idx] {
                    if (station.position - center).length() <= radius {
                        results.push(station);
                    }
                }
            }
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cartesian_to_lat_lon_roundtrip() {
        // Greenwich equator (0, 0)
        let pos_greenwich = Vec3::new(1.0, 0.0, 0.0);
        let (lat, lon) = GlobeSpatialGrid::cartesian_to_lat_lon(pos_greenwich);
        assert!(lat.abs() < 1e-4);
        assert!(lon.abs() < 1e-4);

        // North pole (90, 0)
        let pos_north = Vec3::new(0.0, 1.0, 0.0);
        let (lat_n, lon_n) = GlobeSpatialGrid::cartesian_to_lat_lon(pos_north);
        assert!((lat_n - 90.0).abs() < 1e-4);
        assert!(lon_n.abs() < 1e-4);

        // 90 deg East (0, 90)
        let pos_e = Vec3::new(0.0, 0.0, -1.0);
        let (lat_e, lon_e) = GlobeSpatialGrid::cartesian_to_lat_lon(pos_e);
        assert!(lat_e.abs() < 1e-4);
        assert!((lon_e - 90.0).abs() < 1e-4);

        // 90 deg West (0, -90)
        let pos_w = Vec3::new(0.0, 0.0, 1.0);
        let (lat_w, lon_w) = GlobeSpatialGrid::cartesian_to_lat_lon(pos_w);
        assert!(lat_w.abs() < 1e-4);
        assert!((lon_w - (-90.0)).abs() < 1e-4);
    }

    #[test]
    fn test_grid_lat_lon_binning() {
        let grid = GlobeSpatialGrid::new(10.0);
        assert_eq!(grid.num_rows, 18);
        assert_eq!(grid.num_cols, 36);

        // South Pole (-90, -180) -> row 0, col 0
        let (r, c) = grid.lat_lon_to_cell(-90.0, -180.0);
        assert_eq!((r, c), (0, 0));

        // Equator at prime meridian (0, 0) -> row 9, col 18
        let (r, c) = grid.lat_lon_to_cell(0.0, 0.0);
        assert_eq!((r, c), (9, 18));

        // North Pole (90, 180) -> row 17, col 0 (wrapped)
        let (r, c) = grid.lat_lon_to_cell(90.0, 180.0);
        assert_eq!(r, 17);
        assert_eq!(c, 0);
    }

    #[test]
    fn test_query_closest_and_radius() {
        let mut grid = GlobeSpatialGrid::new(10.0);

        // Insert stations around Paris (48.85, 2.35)
        let paris_pos = Vec3::new(0.657, 0.753, -0.027);
        grid.insert(SpatialStation {
            uuid: "paris-1".to_string(),
            position: paris_pos,
            lat: 48.85,
            lon: 2.35,
            instance_index: 0,
        });

        // Insert a station in London (51.5, -0.12)
        let london_pos = Vec3::new(0.622, 0.782, 0.001);
        grid.insert(SpatialStation {
            uuid: "london-1".to_string(),
            position: london_pos,
            lat: 51.5,
            lon: -0.12,
            instance_index: 1,
        });

        // Insert a station in Tokyo (35.67, 139.65)
        let tokyo_pos = Vec3::new(-0.619, 0.583, -0.526);
        grid.insert(SpatialStation {
            uuid: "tokyo-1".to_string(),
            position: tokyo_pos,
            lat: 35.67,
            lon: 139.65,
            instance_index: 2,
        });

        // Query close to Paris within radius 0.05
        let query_point = paris_pos + Vec3::new(0.005, 0.0, 0.0);
        let closest = grid.query_closest(query_point, 0.05);
        assert!(closest.is_some());
        assert_eq!(closest.unwrap().0.uuid, "paris-1");

        // Query with large radius including Paris and London, but not Tokyo
        let nearby = grid.query_radius(paris_pos, 0.1);
        assert_eq!(nearby.len(), 2);
        let uuids: Vec<_> = nearby.iter().map(|s| s.uuid.as_str()).collect();
        assert!(uuids.contains(&"paris-1"));
        assert!(uuids.contains(&"london-1"));
        assert!(!uuids.contains(&"tokyo-1"));
    }

    #[test]
    fn test_antimeridian_wrapping() {
        let mut grid = GlobeSpatialGrid::new(10.0);

        // Station just west of antimeridian (lat 0, lon 179)
        let pos_w = Vec3::new(-0.9998, 0.0, -0.0174);
        grid.insert(SpatialStation {
            uuid: "station-w".to_string(),
            position: pos_w,
            lat: 0.0,
            lon: 179.0,
            instance_index: 0,
        });

        // Station just east of antimeridian (lat 0, lon -179)
        let pos_e = Vec3::new(-0.9998, 0.0, 0.0174);
        grid.insert(SpatialStation {
            uuid: "station-e".to_string(),
            position: pos_e,
            lat: 0.0,
            lon: -179.0,
            instance_index: 1,
        });

        // Query right at antimeridian (lat 0, lon 180)
        let antimeridian = Vec3::new(-1.0, 0.0, 0.0);
        let nearby = grid.query_radius(antimeridian, 0.05);
        assert_eq!(nearby.len(), 2);
    }
}

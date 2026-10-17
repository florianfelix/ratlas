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

    /// Returns the number of non-empty cells in the grid.
    #[allow(dead_code)]
    pub fn cell_count(&self) -> usize {
        self.cells.iter().filter(|c| !c.is_empty()).count()
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
        let lat_clamped = lat.clamp(-90.0, 90.0);
        let row = (((lat_clamped + 90.0) / self.cell_size_deg).floor() as usize)
            .min(self.num_rows.saturating_sub(1));

        let lon_normalized = lon.rem_euclid(360.0); // [0.0, 360.0)
        let col = ((lon_normalized / self.cell_size_deg).floor() as usize)
            .min(self.num_cols.saturating_sub(1));

        (row, col)
    }

    #[inline]
    fn cell_index(&self, row: usize, col: usize) -> usize {
        row * self.num_cols + col
    }

    /// Inserts a spatial station into its corresponding grid cell.
    pub fn insert(&mut self, station: SpatialStation) {
        let (row, col) = self.lat_lon_to_cell(station.lat, station.lon);
        let idx = self.cell_index(row, col);
        self.cells[idx].push(station);
    }

    /// Fast O(1) query returning all stations within a radius (in 3D world units) from a given center.
    /// Computes the bounding lat/lon box to check only the adjacent cells.
    pub fn query_radius(&self, center: Vec3, radius: f32) -> Vec<&SpatialStation> {
        let (center_lat, center_lon) = Self::cartesian_to_lat_lon(center);

        // Approximate angular radius: chord length = 2 * sin(theta / 2). For small theta, theta ~ radius (on unit sphere)
        let delta_deg = (radius / std::f32::consts::PI * 180.0).max(self.cell_size_deg);

        let min_lat = center_lat - delta_deg;
        let max_lat = center_lat + delta_deg;
        let (min_row, _) = self.lat_lon_to_cell(min_lat, 0.0);
        let (max_row, _) = self.lat_lon_to_cell(max_lat, 0.0);

        let cos_lat = center_lat.to_radians().cos().abs().max(0.01);
        let delta_lon_deg = delta_deg / cos_lat;
        let min_lon = center_lon - delta_lon_deg;
        let _max_lon = center_lon + delta_lon_deg;

        let num_cols_to_check =
            ((2.0 * delta_lon_deg / self.cell_size_deg).ceil() as usize + 2).min(self.num_cols);
        let (_, start_col) = self.lat_lon_to_cell(0.0, min_lon);

        let radius_sq = radius * radius;
        let mut results = Vec::new();

        for r in min_row..=max_row {
            for c_offset in 0..num_cols_to_check {
                let c = (start_col + c_offset) % self.num_cols;
                let idx = self.cell_index(r, c);
                for station in &self.cells[idx] {
                    if station.position.distance_squared(center) <= radius_sq {
                        results.push(station);
                    }
                }
            }
        }

        results
    }

    /// Queries the single closest station within `max_radius` in O(1) time.
    pub fn query_closest(&self, center: Vec3, max_radius: f32) -> Option<(&SpatialStation, f32)> {
        let candidates = self.query_radius(center, max_radius);
        let mut closest = None;
        let mut min_dist_sq = max_radius * max_radius;

        for station in candidates {
            let dist_sq = station.position.distance_squared(center);
            if dist_sq < min_dist_sq {
                min_dist_sq = dist_sq;
                closest = Some((station, dist_sq.sqrt()));
            }
        }

        closest
    }
}

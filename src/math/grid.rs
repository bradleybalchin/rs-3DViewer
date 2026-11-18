//! # Spatial Grid
//! Uniform grid of cubic cells used to bucket points for O(1) neighbour lookups
//! (Poisson sampling, voxel sampling, normal estimation, ...)
//!
//! The grid is anchored at the min corner of the point cloud's bounding box, so cell boundaries
//! line up with the cloud rather than the origin (result is unchanged if the cloud is translated)
//! and all cell indices are >= 0

use glam::DVec3;

/// Voxel grid, finds min value for bounding and performs cell mapping
#[derive(Clone, Copy)]
pub struct VoxelGrid { min: [f32; 3], res: f32 }

impl VoxelGrid {
    /// Grid of cells with side `res`, anchored at the min corner of `positions`. \
    /// Takes any iterator of positions so callers don't copy the cloud, e.g.
    /// `VoxelGrid::new(points.iter().map(|p| p.pos), res)` or `VoxelGrid::new(positions.iter().copied(), res)`
    pub fn new(positions: impl IntoIterator<Item = [f32; 3]>, res: f32) -> Self {
        let min = positions.into_iter().fold([f32::INFINITY; 3], |m, p| {
            [m[0].min(p[0]), m[1].min(p[1]), m[2].min(p[2])]
        });
        Self { min, res }
    }

    /// Position of a cell's minimum corner, in f64
    #[inline]
    pub fn corner(&self, key: [i32; 3]) -> DVec3 {
        let res = self.res as f64;
        DVec3::new(
            self.min[0] as f64 + key[0] as f64 * res,
            self.min[1] as f64 + key[1] as f64 * res,
            self.min[2] as f64 + key[2] as f64 * res,
        )
    }

    // inline - optimise by replacing function call with copy of funciton body at compile time
    /// Map float positions to voxel grid cells
    #[inline]
    pub fn cell(&self, p: [f32; 3]) -> [i32; 3] {
        [((p[0] - self.min[0]) / self.res).floor() as i32,
         ((p[1] - self.min[1]) / self.res).floor() as i32,
         ((p[2] - self.min[2]) / self.res).floor() as i32]
    }

    /// The 3x3x3 block of cell keys around `key` (including `key` itself). \
    /// With cell size = search radius r, any point within r of a point in `key` lies in one of these 27 cells
    #[inline]
    pub fn neighbours(key: [i32; 3]) -> impl Iterator<Item = [i32; 3]> {
        (-1..=1).flat_map(move |dx| (-1..=1).flat_map(move |dy| (-1..=1).map(move |dz| {
            [key[0] + dx, key[1] + dy, key[2] + dz]
        })))
    }
}

// Accmulation of posisitions and colour values for a grid cell (n is number of points)
#[derive(Default)]
pub struct Acc { 
    pub sum: [f64; 3],  // sum of positions
    pub rgba: [u64; 4], // sum of rgb byte values
    pub n: u32,         // number of points
    pub psum : [f64;6]  // sum of products of positions (for covariance calculations) (xx,xy,xz,yy,yz,zz)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use rand::{RngExt, SeedableRng};
    use crate::render::vertex::GpuPoint;
    use super::VoxelGrid;

    fn gen_points(n: usize, seed: u64) -> Vec<GpuPoint> {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        (0..n).map(|_| GpuPoint::new([
            rng.random_range(-50.0..50.0), rng.random_range(-50.0..50.0), rng.random_range(-50.0..50.0)
        ], 0)).collect()
    }

    // The min corner of the cloud is in cell [0,0,0], and no point has a negative index
    #[test]
    fn grid_anchored_at_min() {
        let pnts = gen_points(1_000, 1);
        let vg = VoxelGrid::new(pnts.iter().map(|p| p.pos), 2.0);
        assert_eq!(vg.cell(vg.min), [0, 0, 0]);
        for p in &pnts {
            assert!(vg.cell(p.pos).iter().all(|&i| i >= 0), "{:?} -> {:?}", p.pos, vg.cell(p.pos));
        }
    }

    // Every point lies inside the cell it maps to: corner <= p < corner + res (small f32 rounding allowed)
    #[test]
    fn grid_point_inside_its_cell() {
        let pnts = gen_points(1_000, 2);
        for res in [0.3f32, 1.0, 7.5] {
            let vg = VoxelGrid::new(pnts.iter().map(|p| p.pos), res);
            for p in &pnts {
                let c = vg.corner(vg.cell(p.pos));
                let pos = [p.pos[0] as f64, p.pos[1] as f64, p.pos[2] as f64];
                let corner = [c.x, c.y, c.z];
                for i in 0..3 {
                    let d = pos[i] - corner[i];
                    assert!(d >= -1e-4 && d < res as f64 + 1e-4, "res={res}: {:?} not in cell with corner {c}", p.pos);
                }
            }
        }
    }

    // neighbours gives 27 distinct cells, including the centre, all within 1 in every axis
    #[test]
    fn grid_neighbours_3x3x3() {
        let key = [5, -2, 0];
        let ns: Vec<[i32; 3]> = VoxelGrid::neighbours(key).collect();
        let unique: HashSet<[i32; 3]> = ns.iter().copied().collect();
        assert_eq!(ns.len(), 27);
        assert_eq!(unique.len(), 27);
        assert!(unique.contains(&key));
        assert!(ns.iter().all(|n| (0..3).all(|i| (n[i] - key[i]).abs() <= 1)));
    }

    // Empty input must not panic
    #[test]
    fn grid_empty_input() {
        let vg = VoxelGrid::new(std::iter::empty(), 1.0);
        let _ = vg.corner([0, 0, 0]);
    }
}

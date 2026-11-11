//! Voxelisation Sampling
//! Builds a 3D bounding box of the space containing the pointclouds
//! Divided into a grid of 3D cells of a specified size/resoltuion

use rustc_hash::FxHashMap;
use std::{time::Instant};
use crate::render::vertex::GpuPoint;

/// Grid sampling : all points within a voxel are replaced with a single representive point (average of grid members) \
/// This function is O(N)
pub fn sample_grid(points : &Vec<GpuPoint>, res : f32) -> Result<Vec<GpuPoint>, String> {
    let start = Instant::now(); // timing
    eprintln!("Voxel Grid Sampling res = {:.2}] Beginning Sampling", res);
    // Reject negative or NaN values for resolution
    if res.is_nan() || res.is_sign_negative() {
        return Err("resolution must be positive".into());
    }
    if res == 0.0 {return Ok(points.clone())}

    // create voxel grid (vg provides cell mappings for hashmap)
    let vg = VoxelGrid::new(points, res);
    let mut grid: FxHashMap<[i32; 3], Acc> = FxHashMap::default();

    // for each point, find grid cell and add position and colour to accumulator for that grid cell
    for p in points {
        let a = grid.entry(vg.cell(p.pos)).or_default();
        for i in 0..3 { a.sum[i] += p.pos[i] as f64;}
        for (i, b) in p.color.to_le_bytes().iter().enumerate() { a.rgba[i] += *b as u64;}      
        a.n += 1;      
    }

    // return vector from grid with each Accumulator used to caclulate average point for grid space
    let selected: Vec<GpuPoint> = grid.into_values().map(|a| {
        let n = a.n as f64;
        let pos = [(a.sum[0] / n) as f32, (a.sum[1] / n) as f32, (a.sum[2] / n) as f32];
        let c = a.rgba.map(|v| (v / a.n as u64) as u8);
        GpuPoint::new(pos, u32::from_le_bytes(c))
    }).collect();
    let ms = |d: std::time::Duration| d.as_secs_f64() * 1000.0;
    eprintln!("[Voxel Grid Sampling res = {:.2}] Input Count: {:?} Sample Count: {:?}, Output, Time: {:.1} ms", res, points.len(), selected.len(), ms(start.elapsed()));

    Ok(selected)

}

// Voxel grid, finds min value for bounding and performs cell mapping
#[derive(Clone, Copy)]
pub struct VoxelGrid { min: [f32; 3], res: f32 }

impl VoxelGrid {
    pub fn new(points: &[GpuPoint], res: f32) -> Self {
        let min = points.iter().fold([f32::INFINITY; 3], |m, p| {
            [m[0].min(p.pos[0]), m[1].min(p.pos[1]), m[2].min(p.pos[2])]
        });
        Self { min, res }
    }

    // inline - optimise by replacing function call with copy of funciton body at compile time
    /// Map float positions to voxel grid cells
    #[inline]
    pub fn cell(&self, p: [f32; 3]) -> [i32; 3] {
        [((p[0] - self.min[0]) / self.res).floor() as i32,
         ((p[1] - self.min[1]) / self.res).floor() as i32,
         ((p[2] - self.min[2]) / self.res).floor() as i32]
    }
}


// Accmulation of posisitions and colour values for a grid cell (n is number of points)
#[derive(Default)]
struct Acc { sum: [f64; 3], rgba: [u64; 4], n: u32 }

#[cfg(test)]
mod tests {

    use std::collections::HashSet;

    use more_asserts::{self, assert_le};
    use rand::{RngExt, SeedableRng};
    use crate::render::vertex::GpuPoint;
    use super::{sample_grid};

    // generate random GpuPoint vector for testing
    // points are uniform in a cube of side `extent` centred on the origin, with random colours
    fn gen_test_data(numpoints : usize, seed : u64) -> Vec<GpuPoint>{
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let half = 50.0_f32; // cube side 100 (-50 to 50)

        (0..numpoints)
            .map(|_| {
                let pos = [
                    rng.random_range(-half..half),
                    rng.random_range(-half..half),
                    rng.random_range(-half..half),
                ];
                GpuPoint::new(pos, rng.random())
            })
            .collect()
    }

    // Sample point count is always less than or equal to initial vector
    #[test]
    fn grid_sample_weakly_smaller(){
        let pnts = gen_test_data(1_000, 42);
        let rs = [0.0, 0.5, 1.0, 2.0, 5.0, 10.0, 100.0];

        for r in rs {
            assert_le!(sample_grid(&pnts, r).unwrap().len(), pnts.len(), "res={r}")
        }
    }
    
    // res must be a positive f32 value, sample should accept no other values for r
    #[test]
    fn grid_res_negative_or_nan() {
        let pnts = gen_test_data(1_000, 42);

        assert!(if let Err(_) = sample_grid(&pnts, -1.0) {true} else {false}, "Negative res");
        assert!(if let Err(_) = sample_grid(&pnts, f32::NAN) {true} else {false}, "NaN res");

    }

    // Output sample contains no duplicate points
    #[test]
    fn grid_sample_no_duplicates() {
        let pnts = gen_test_data(1_000, 42);
        let rs = [0.0, 0.5, 1.0, 2.0, 5.0, 10.0, 100.0];

        for r in rs {
            let sample = sample_grid(&pnts, r).unwrap();
            let mut seen: HashSet<GpuPoint> = HashSet::new();
            for p in sample {
                assert!(seen.insert(p), "r={r}")
            }   
        }
    }

    // if res is zero, then the sample should equal the input (no sampling performed)
    #[test]
    fn grid_res_zero() {
        let pnts = gen_test_data(1_000, 42);
        let r = 0.0f32;

        let sample = sample_grid(&pnts, r).unwrap();
        assert_eq!(sample.len(), pnts.len(), "Len: sample {:?} != input {:?}", sample.len(), pnts.len());
        for p in pnts {
            assert!(sample.contains(&p));
        }
    }

    // if res covers the entire pointcloud bounds, then only one point should be included
    #[test]
    fn grid_res_larger_than_input(){
        let pnts = gen_test_data(1_000, 42);
        let r = f32::MAX;
        
        assert_eq!(sample_grid(&pnts, r).unwrap().len(), 1)
    }
}
//! # Poisson Sampling
//! sample input point cloud such that no two points are kept within some min distance r
//! while points still appear randomly placed rather than grid alligned

use std::{time::Instant};
use rustc_hash::{FxHashMap, FxBuildHasher};

use crate::math::grid::VoxelGrid;
use crate::render::vertex::GpuPoint;

/// Poisson sample `points` to return subset such that no two points are within radius `r` of eachother \
/// This function is O(N)
pub fn sample(points : &Vec<GpuPoint>, r : f32) -> Result<Vec<GpuPoint>,String>{
    let start = Instant::now(); // timing
    eprintln!("[Poission Sampling r = {:.2}] Beginning Sampling", r);

    // Reject negative or NaN values for r
    if r.is_nan() || r.is_sign_negative() {
        return Err("r must be positive".into());
    }
    if r == 0.0 {return  Ok(points.to_vec());} // when r = 0 every point is kept

    let r2 = r*r;   // compare squares to avoid sqrts in loop

    // spacial grid with cell size c = r, allows for neighbor checking in O(1) time
    //      any point within r of p must lie in p's cell or one of its 26 neighbours -> check 3x3x3 block (27 lookups)
    //      (Bridson's c = r/sqrt(3) gives at most one point per cell but needs a 5x5x5 block = 125 lookups)
    //      points kept >= r apart can only pack a bounded number into an r-sized cell, so each bucket stays small
    let vg = VoxelGrid::new(points.iter().map(|p| p.pos), r);
    let mut grid: FxHashMap<[i32;3], Vec<usize>> = FxHashMap::with_hasher(FxBuildHasher);
    let mut selected: Vec<GpuPoint> = Vec::new();

    'points : for p in points {
        let k = vg.cell(p.pos);
        for n in VoxelGrid::neighbours(k) {            // check the 3x3x3 area
            if let Some (bucket) = grid.get(&n) {
                // if some grid space in 3x3x3 bucket contains an point closer than r , then conintue (dont include in output)
                for &i in bucket{
                   if dist2(selected[i].pos, p.pos) < r2 {continue 'points;}
                }
            }
        }
        // add new point to selected and index added to spacial grid
        grid.entry(k).or_default().push(selected.len());
        selected.push(*p);
    }


    let ms = |d: std::time::Duration| d.as_secs_f64() * 1000.0;
    eprintln!("[Poission Sampling r = {:.2}] Input Count: {:?} Sample Count: {:?}, Output, Time: {:.1} ms", r, points.len(), selected.len(), ms(start.elapsed()));


    Ok(selected)
}

/// Calculate distance between two point positions
/// sqrt ( (x2-x1)^2 + (y2-y1)^2 + (z2-z1)^2 )
#[cfg(test)]
fn dist(v1 : [f32;3], v2 : [f32;3]) -> f32 {
    v1.iter().zip(v2).map(|(x, y)| (x-y).powi(2)).sum::<f32>().sqrt()
}

/// Calculate distance between two point positions and return the squared result r2
fn dist2(v1 : [f32;3], v2 : [f32;3]) -> f32 {
    v1.iter().zip(v2).map(|(x, y)| (x-y).powi(2)).sum::<f32>()
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use more_asserts::{self, assert_le};
    use rand::{RngExt, SeedableRng};
    use crate::render::vertex::GpuPoint;
    use super::{sample,dist};

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
    fn sample_weakly_smaller(){
        let pnts = gen_test_data(1_000, 42);
        let rs = [0.0, 0.5, 1.0, 2.0, 5.0, 10.0, 100.0];

        for r in rs {
            assert_le!(sample(&pnts, r).unwrap().len(), pnts.len(), "r={r}")
        }
    }

    // Output sample is a subset of input - all outputs points must appear exactly in the input
    #[test]
    fn sample_is_subset(){
        let pnts = gen_test_data(1_000, 42);
        let rs = [0.0, 0.5, 1.0, 2.0, 5.0, 10.0, 100.0];

        for r in rs {
            let sample = sample(&pnts, r).unwrap();
            assert!(sample.iter().all(|p| pnts.contains(p)), "r={r}")
        }
    }

    // Output sample contains no duplicate points
    #[test]
    fn sample_no_duplicates() {
        let pnts = gen_test_data(1_000, 42);
        let rs = [0.0, 0.5, 1.0, 2.0, 5.0, 10.0, 100.0];

        for r in rs {
            let sample = sample(&pnts, r).unwrap();
            let mut seen: HashSet<GpuPoint> = HashSet::new();
            for p in sample {
                assert!(seen.insert(p), "r={r}")
            }   
        }
    }

    // r must be a positive f32 value, sample should accept no other values for r
    #[test]
    fn r_negative_or_nan() {
        let pnts = gen_test_data(1_000, 42);

        assert!(if let Err(_) = sample(&pnts, -1.0) {true} else {false}, "Negative r");
        assert!(if let Err(_) = sample(&pnts, f32::NAN) {true} else {false}, "NaN r");

    }

    //No two points are closer than r
    #[test]
    fn sample_respect_min_distance(){
        let pnts = gen_test_data(1_000, 42);
        let rs = [0.0, 0.5, 1.0, 2.0, 5.0, 10.0, 100.0];

        for r in rs {
            let sample = sample(&pnts, r).unwrap();
            assert!(check_dist(r, &sample), "r = {r}")
        }


    }

    // Return true/false depending in if any distance between two points is less than r
    fn check_dist(r : f32, points : &Vec<GpuPoint>) -> bool {
        let mut check_points = points.clone();
        for p in points{
            check_points.remove(0);
            for check_p in &check_points {
                if dist(p.pos, check_p.pos) < r { return  false;}
            }
        }
        true
    }

    // There is no excluded input point p that is not within r of some kept point 
    // i.e. cannot add another any more points to the sample without violating the r constraint
    #[test]
    fn subset_is_maximal(){
        let pnts = gen_test_data(1_000, 42);
        let rs = [0.0, 0.5, 1.0, 2.0, 5.0, 10.0, 100.0];

        for r in rs {
            let sample = sample(&pnts, r).unwrap();

            // excluded - not contained in sample
            let excluded: Vec<&GpuPoint> = pnts.iter().filter(|p| !sample.contains(p)).collect();

            // every excluded point must be blocked by at least one kept point
            for p in &excluded {
                assert!(
                    sample.iter().any(|s| dist(s.pos, p.pos) < r),
                    "r={r}: excluded point {:?} is not within r of any kept point, so the sample is not maximal",
                    p.pos
                );
            }

            // sanity: kept + excluded accounts for every input point
            assert_eq!(sample.len() + excluded.len(), pnts.len(), "r={r}");
        }
    }

    // Points on the x axis at 0, 1, 2, 3, 4 with r = 1.5: keeps 0, 2, 4 and drops 1, 3.
    #[test]
    fn subset_is_maximal_known_case(){
        let pnts: Vec<GpuPoint> = (0..5)
            .map(|i| GpuPoint::new([i as f32, 0.0, 0.0], 0xFFFF_FFFF))
            .collect();

        let sample = sample(&pnts, 1.5).unwrap();
        let xs: Vec<f32> = sample.iter().map(|p| p.pos[0]).collect();
        assert_eq!(xs, vec![0.0, 2.0, 4.0]);
    }

    // Grid-accelerated sample must match the brute-force greedy reference exactly (same points, same order)
    #[test]
    fn matches_brute_force_reference(){
        let rs = [0.1, 0.5, 1.0, 2.0, 3.7, 5.0, 10.0, 33.3, 100.0];
        for seed in [1, 42, 1234] {
            let pnts = gen_test_data(2_000, seed);
            for r in rs {
                let mut expected: Vec<GpuPoint> = Vec::new();
                for p in &pnts {
                    if !expected.iter().any(|s| dist(s.pos, p.pos) < r) { expected.push(*p); }
                }
                let got = sample(&pnts, r).unwrap();
                assert!(got == expected, "seed={seed} r={r}: grid kept {} points, brute force kept {}", got.len(), expected.len());
            }
        }
    }

    // if r is zero, then the sample should equal the input
    #[test]
    fn r_zero() {
        let pnts = gen_test_data(1_000, 42);
        let r = 0.0f32;

        let sample = sample(&pnts, r).unwrap();
        assert_eq!(sample.len(), pnts.len(), "Len: sample {:?} != input {:?}", sample.len(), pnts.len());
        for p in pnts {
            assert!(sample.contains(&p));
        }
    }

    // if r covers the entire pointcloud bounds, then only one point should be include
    #[test]
    fn r_larger_than_input(){
        let pnts = gen_test_data(1_000, 42);
        let r = f32::MAX;
        
        assert_eq!(sample(&pnts, r).unwrap().len(), 1)
    }
}
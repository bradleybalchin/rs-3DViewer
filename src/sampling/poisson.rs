//! # Poisson Sampling
//! sample input point cloud such that no two points are kept within some min distance r
//! while points still appear randomly placed rather than grid alligned

use std::collections::HashSet;
use std::time::Instant;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use crate::render::vertex::GpuPoint;

/// Poisson sample `points` to return subset such that no two points are within radius `r` of eachother
pub fn sample(points : &Vec<GpuPoint>, r : f32) -> Result<Vec<GpuPoint>,String>{
    let start = Instant::now(); // timing

    // Reject negative or NaN values for r
    if r.is_nan() || r.is_sign_negative() {
        return Err("r must be positive".into());
    }

    // If not within r of an already selected point, then add to selected, else ignore it from the sample
    let mut selected: Vec<GpuPoint> = Vec::new();
    for p in points.clone() {
        if selected.par_iter().any(|fp| dist(fp.pos, p.pos) < r) { continue;}
        selected.push(p);
    }

    let ms = |d: std::time::Duration| d.as_secs_f64() * 1000.0;
    eprintln!("[Poission Sampling r = {:.2}] Input Count: {:?} Sample Count: {:?}, Output, Time: {:.1} ms", r, points.len(), selected.len(), ms(start.elapsed()));


    Ok(selected)
}

/// Calculate distance between two point positions
/// sqrt ( (x2-x1)^2 + (y2-y1)^2 + (z2-z1)^2 )
fn dist(v1 : [f32;3], v2 : [f32;3]) -> f32 {
    v1.iter().zip(v2).map(|(x, y)| (x-y).powi(2)).sum::<f32>().sqrt()
}

#[cfg(test)]
mod tests {
    use std::{collections::{HashMap, HashSet}, ops::Bound::Excluded};

use more_asserts::{self, assert_le};
use rand::{RngExt, SeedableRng};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
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

        assert!(if let Err(s) = sample(&pnts, -1.0) {true} else {false}, "Negative r");
        assert!(if let Err(s) = sample(&pnts, f32::NAN) {true} else {false}, "NaN r");

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
}
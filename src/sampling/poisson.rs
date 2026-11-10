//! # Poisson Sampling
//! sample input point cloud such that no two points are kept within some min distance r
//! while points still appear randomly placed rather than grid alligned

use crate::render::vertex::GpuPoint;

/// Poisson sample `points` to return subset such that no two points are within radius `r` of eachother
fn sample(points : &Vec<GpuPoint>, r : f32) -> Result<Vec<GpuPoint>,String>{
    Ok(points.clone())
}



#[cfg(test)]
mod tests {
    use std::collections::HashSet;

use more_asserts::{self, assert_le};
use rand::{RngExt, SeedableRng};
    use crate::render::vertex::GpuPoint;
    use super::sample;

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

    // // r must be a positive f32 value, sample should accept no other values for r
    // #[test]
    // fn r_negative_or_nan() {

    // }

    // No two points are closer than r
    // #[test]
    // fn sample_respect_min_distance(){

    // }

    // // There is no excluded input point p that is not within r of some kept point 
    // #[test]
    // fn subset_is_maximal(){

    // }
}
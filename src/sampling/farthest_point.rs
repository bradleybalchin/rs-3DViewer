//! # Farthest Point Sampling
//! 
//! Randomly choose point p, and add to selected set A,
//! Each iteration, choose new point p not in A that is furthest away from any point in A
//! Repeat until A has target point number

use std::time::Instant;

use rustc_hash::FxHashSet;

use crate::render::vertex::GpuPoint;

/// Sample `points` using Farthest Point Sampling to a target size `target`
/// 
/// This function is `O(N * M)` for `N` points and target sample size `M` 
/// 
/// ### Method
/// 1. Randomly select some point p ∈ Points
/// 2. Add to sample set A
/// 3. For each point in P \ A, calculate the min distance to any point in A
/// 4. Add the point with the highest min distance to A
/// 5. repeat steps 3 and 4 until A has the target number of points 
pub fn sample(points: &Vec<GpuPoint>, target: usize) -> Result<Vec<GpuPoint>, String> {
    let start = Instant::now();
    eprintln!("[FPS t = {target} ] Beginning Sampling");

    let points: FxHashSet<&GpuPoint> = FxHashSet::from_iter(points.iter());
    let selected = Vec::new();




    let ms = |d: std::time::Duration| d.as_secs_f64() * 1000.0;
    eprintln!("[NSS t = {target}] Input Count: {:?} Sample Count: {:?}, Output, Time: {:.1} ms", points.len(), selected.len(), ms(start.elapsed()));
    Ok(selected)

}



#[cfg(test)]
mod tests { 

    use std::collections::HashSet;

use rand::{RngExt, SeedableRng};
    use crate::{render::vertex::GpuPoint, sampling::farthest_point::tests};
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

    // output sample is a subset of the original points with no duplicate points
    #[test]
    fn farthest_point_sample_is_subset_no_duplicates(){
        let pnts = gen_test_data(10_000, 42);
        for t in [10, 200, 1000, 5000] {
            let s = sample(&pnts, t).unwrap();
            let input: HashSet<GpuPoint> = pnts.iter().copied().collect();
            let mut seen: HashSet<GpuPoint> = HashSet::new();
            for p in &s {
                assert!(input.contains(p), "target = {t}: output point {:?} not in input", p.pos);
                assert!(seen.insert(*p), "target = {t}: duplicate point {:?}", p.pos);
            }
        }
    }

    // size of the output sample is equal to the input target (when target is less than num points)
    #[test]
    fn farthest_point_sample_size_is_target(){
        let pnts = gen_test_data(10_000, 42);
        for t in [10, 200, 1000, 5000] {
            let s= sample(&pnts, t).unwrap().len();
            assert_eq!(s, t, "sample = {s} , target = {t}" )
        }
    }

    // if target is equal to larger than number of points, the the output sample is equal to the full point array
    #[test]
    fn farthest_point_target_larger_then_points_returns_points(){
        let pnts = gen_test_data(10_000, 42);
        for t in [10_000, 10_0001, 50_000] {
            let s= sample(&pnts, t).unwrap().len();
            assert_eq!(s, pnts.len(), "sample = {s} , target = {t}" )
        }
    }

    // if target is 0, then the result is an empty set
    #[test]
    fn farthest_point_target_0_returns_empty(){
        let pnts = gen_test_data(10_000, 42);
        let s= sample(&pnts, 0).unwrap();
        assert_eq!(s, vec![], "sample not empty for the target = 0");
    }

    


}
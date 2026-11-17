//! # Normal Space Sampling
//! 
//! Selects points so that the surface normal vectors are distributed as uniformly
//! as possible across the angular space

use std::time::Instant;

use rand::{SeedableRng, seq::SliceRandom};
use rustc_hash::FxHashMap;
use glam::DVec3;

use crate::{math::normals, render::vertex::GpuPoint};

/// Squares per side of each cube face: 3 faces x k x k buckets (k = 4 -> 48 buckets)
const BINS_PER_FACE: u32 = 4;

/// Bucket for points with no reliable normal (face 3 is not a real cube face)
const NONE_BUCKET: Bucket = (3, 0, 0);

/// Fixed seed for shuffling buckets, so the same input always gives the same sample
const SEED: u64 = 42;

/// Cube-map bucket of a direction: (face axis 0/1/2 = x/y/z, square i, square j)
type Bucket = (u8, u32, u32);

/// Create a sample of `points` using normal space sampling \
/// `r` is the normal estimation cell size (must be > 0) \
/// `target` is the number of output points \
///
///  Samples points uniformly across normal angular space
/// 
/// ### Method
/// * Estimate normals for each point \
/// * Points are grouped into buckets by normal direction, then drawn evenly from every bucket,
/// so orientations that are rare in the input (e.g. a small wall next to a large floor) keep a fair share. \
/// * Points with no reliable normal get their own bucket, which gets the same share as each direction bucket.
pub fn sample(points: &Vec<GpuPoint>, r: f32, target: usize) -> Result<Vec<GpuPoint>, String>{
    let start = Instant::now();
    eprintln!("[NSS t = {target} r = {:.2}] Beginning Sampling", r);

    // Reject zero, negative or NaN r (no normals can be estimated)
    if r.is_nan() || r <= 0.0 {
        return Err("r must be > 0".into());
    }

    // bucket each point by its normal direction
    //  cube map -> which grid square of a cube face the normal hits, travelling out from the cube's centre
    //  n and -n land in the same bucket, so the arbitrary sign of estimated normals doesn't matter
    let mut bucket_map: FxHashMap<Bucket, Vec<usize>> = FxHashMap::default();
    for (i, n) in normals::estimate_normals(points, r).into_iter().enumerate() {
        let b = n.map_or(NONE_BUCKET, |n: DVec3| cube_bucket(n, BINS_PER_FACE));
        bucket_map.entry(b).or_default().push(i);
    }

    // sort buckets by key so the shuffle order (and so the output) doesn't depend on hash map order,
    // then shuffle each bucket's points so draws within a bucket are spread out, not in input order
    let mut groups: Vec<(Bucket, Vec<usize>)> = bucket_map.into_iter().collect();
    groups.sort_unstable_by_key(|(b, _)| *b);

    let mut rng = rand::rngs::StdRng::seed_from_u64(SEED);
    for (_, idx) in &mut groups {
        idx.shuffle(&mut rng);
    }

    // can't take more points than exist (also handles target = usize::MAX)
    let target = target.min(points.len());
    let mut selected: Vec<GpuPoint> = Vec::with_capacity(target);

    // round-robin: each round takes one point from every non-empty group
    while selected.len() < target {
        for (_, idx) in &mut groups {
            if selected.len() == target { break; }
            if let Some(i) = idx.pop() { selected.push(points[i]); }
        }
        // remove groups that ran out, so later rounds only visit groups with points left
        groups.retain(|(_, idx)| !idx.is_empty());
    }

    let ms = |d: std::time::Duration| d.as_secs_f64() * 1000.0;
    eprintln!("[NSS t = {target} r = {:.2}] Input Count: {:?} Sample Count: {:?}, Output, Time: {:.1} ms", r, points.len(), selected.len(), ms(start.elapsed()));

    Ok(selected)

}

/// Which square of which cube face direction `n` hits, travelling out from the cube's centre. \
/// Each face is split into a `k` x `k` grid of squares.
///
/// `n` and `-n` always give the same bucket, so only 3 faces are used (not 6).
/// `n` must be non-zero; it does not need to be unit length.
///
/// ### Method
/// 1. Face = axis of the largest |component| (the cube face the direction hits first).
/// 2. Position on the face: divide the other two components by the major one -> u, v in [-1, 1].
///    Dividing by the *signed* major component folds n and -n together: (-n.x)/(-n.z) = n.x/n.z.
/// 3. Square: map [-1, 1] to 0..k (clamped, so u = 1 lands in the last square).
fn cube_bucket(n: DVec3, k: u32) -> Bucket {
    debug_assert!(k > 0, "k must be > 0");

    let a = n.abs().max_position();          // face: axis of largest |component|
    let (b, c) = ((a + 1) % 3, (a + 2) % 3); // the other two axes
    let u = n[b] / n[a];                     // position on the face, in [-1, 1]
    let v = n[c] / n[a];

    // [-1, 1] -> 0..k
    let square = |t: f64| (((t + 1.0) * 0.5 * k as f64) as u32).min(k - 1);
    (a as u8, square(u), square(v))
}




#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use rand::{RngExt, SeedableRng};
    use crate::render::vertex::GpuPoint;
    use glam::DVec3;
    use super::{sample, cube_bucket, Bucket};
 
    // flat plane z = 0 covering [0, extent) x [0, extent)
    fn gen_plane(numpoints: usize, extent: f32, seed: u64) -> Vec<GpuPoint> {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        (0..numpoints)
            .map(|_| GpuPoint::new([rng.random_range(0.0..extent), rng.random_range(0.0..extent), 0.0], rng.random()))
            .collect()
    }

    // L-shape: a large floor (z = 0) and a small wall (x = 0), both 10 x 10
    // `floor` and `wall` are the point counts of each, shuffled together
    fn gen_l_shape(floor: usize, wall: usize, seed: u64) -> Vec<GpuPoint> {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let mut pnts: Vec<GpuPoint> = (0..floor)
            .map(|_| GpuPoint::new([rng.random_range(0.0..10.0), rng.random_range(0.0..10.0), 0.0], rng.random()))
            .collect();
        pnts.extend((0..wall).map(|_| GpuPoint::new([0.0, rng.random_range(0.0..10.0), rng.random_range(0.0..10.0)], rng.random())));
        // interleave floor and wall so input order doesn't favour either
        for i in (1..pnts.len()).rev() {
            let j = rng.random_range(0..=i);
            pnts.swap(i, j);
        }
        pnts
    }

    // point lies on the wall (x = 0) and not on the floor line
    fn on_wall(p: &GpuPoint) -> bool { p.pos[0] == 0.0 && p.pos[2] > 0.0 }

    // Output is a subset of the input, with no duplicates
    #[test]
    fn sample_is_subset_no_duplicates(){
        let pnts = gen_l_shape(9_000, 1_000, 80);
        for target in [1, 100, 1_000, 5_000] {
            let s = sample(&pnts, 1.0, target).unwrap();
            let input: HashSet<GpuPoint> = pnts.iter().copied().collect();
            let mut seen: HashSet<GpuPoint> = HashSet::new();
            for p in &s {
                assert!(input.contains(p), "target = {target}: output point {:?} not in input", p.pos);
                assert!(seen.insert(*p), "target = {target}: duplicate point {:?}", p.pos);
            }
        }
    }

    // Returns exactly `target` points when the input has enough (every point here has a normal)
    #[test]
    fn sample_returns_target_count(){
        let pnts = gen_plane(10_000, 10.0, 81);
        for target in [1, 10, 500, 9_999] {
            assert_eq!(sample(&pnts, 1.0, target).unwrap().len(), target, "target = {target}");
        }
    }

    // target 0 gives no points
    #[test]
    fn sample_target_zero(){
        let pnts = gen_plane(1_000, 10.0, 82);
        assert!(sample(&pnts, 1.0, 0).unwrap().is_empty());
    }

    // target >= input size keeps every point
    #[test]
    fn sample_target_larger_than_input(){
        let pnts = gen_plane(1_000, 10.0, 83);
        for target in [1_000, 5_000, usize::MAX] {
            let s = sample(&pnts, 1.0, target).unwrap();
            assert_eq!(s.len(), pnts.len(), "target = {target}");
        }
    }

    // Empty input gives empty output, not a panic
    #[test]
    fn sample_empty_input(){
        let pnts: Vec<GpuPoint> = Vec::new();
        assert!(sample(&pnts, 1.0, 100).unwrap().is_empty());
    }

    // r is the normal-estimation cell size, so it must be > 0: zero, negative and NaN are errors
    #[test]
    fn sample_r_zero_negative_or_nan(){
        let pnts = gen_plane(1_000, 10.0, 84);
        assert!(sample(&pnts, 0.0, 100).is_err(), "r = 0");
        assert!(sample(&pnts, -1.0, 100).is_err(), "r = -1");
        assert!(sample(&pnts, f32::NAN, 100).is_err(), "r = NaN");
    }

    // Same input and parameters give the same output (fixed seed, no randomness between runs)
    #[test]
    fn sample_deterministic(){
        let pnts = gen_l_shape(9_000, 1_000, 85);
        let a = sample(&pnts, 1.0, 1_000).unwrap();
        let b = sample(&pnts, 1.0, 1_000).unwrap();
        assert!(a == b, "two runs gave different samples");
    }

    // The point of NSS: the wall is 10% of the input but has its own normal direction,
    // so it should get a far larger share of the sample (ideally about half)
    #[test]
    fn sample_boosts_minority_normal(){
        let pnts = gen_l_shape(18_000, 2_000, 86);
        let s = sample(&pnts, 1.0, 2_000).unwrap();

        let input_share = pnts.iter().filter(|p| on_wall(p)).count() as f64 / pnts.len() as f64;
        let sample_share = s.iter().filter(|p| on_wall(p)).count() as f64 / s.len() as f64;
        assert!(sample_share > 0.3,
            "wall share: input {:.1}%, sample {:.1}% (expected > 30%)", input_share * 100.0, sample_share * 100.0);
    }

    // random unit direction
    fn random_dir(rng: &mut rand::rngs::StdRng) -> DVec3 {
        loop {
            let d = DVec3::new(rng.random_range(-1.0..1.0), rng.random_range(-1.0..1.0), rng.random_range(-1.0..1.0));
            if d.length_squared() > 1e-6 { return d.normalize(); }
        }
    }

    // n and -n are the same surface orientation, so must share a bucket
    #[test]
    fn cube_bucket_same_for_n_and_minus_n(){
        let mut rng = rand::rngs::StdRng::seed_from_u64(90);
        for _ in 0..10_000 {
            let n = random_dir(&mut rng);
            assert_eq!(cube_bucket(n, 4), cube_bucket(-n, 4), "n = {n}");
        }
        // near the equator, where rounding noise flips the z sign
        assert_eq!(cube_bucket(DVec3::new(1.0, 0.0, 1e-17), 4), cube_bucket(DVec3::new(-1.0, 0.0, 1e-17), 4));
    }

    // face is 0..3 and both squares are 0..k, for any direction and k
    #[test]
    fn cube_bucket_in_range(){
        let mut rng = rand::rngs::StdRng::seed_from_u64(91);
        for k in [1, 2, 4, 7] {
            for _ in 0..2_000 {
                let n = random_dir(&mut rng);
                let (f, i, j) = cube_bucket(n, k);
                assert!(f < 3 && i < k && j < k, "k = {k}, n = {n} -> {:?}", (f, i, j));
            }
            // cube edges and corners (u or v exactly +-1)
            for n in [DVec3::new(1.0, 1.0, 0.0), DVec3::new(1.0, -1.0, 1.0), DVec3::ONE] {
                let (f, i, j) = cube_bucket(n, k);
                assert!(f < 3 && i < k && j < k, "k = {k}, n = {n} -> {:?}", (f, i, j));
            }
        }
    }

    // worked example: (0.3, -0.2, 0.93), k = 4 -> face z, u = 0.32 -> 2, v = -0.22 -> 1
    #[test]
    fn cube_bucket_known_case(){
        assert_eq!(cube_bucket(DVec3::new(0.3, -0.2, 0.93), 4), (2, 2, 1));
        assert_eq!(cube_bucket(DVec3::Z, 4), (2, 2, 2));   // axis -> centre of its face (u = v = 0)
        assert_eq!(cube_bucket(DVec3::X, 4).0, 0);
        assert_eq!(cube_bucket(DVec3::Y, 4).0, 1);
    }

    // floor (normal z) and wall (normal x) of the L-shape land in different buckets
    #[test]
    fn cube_bucket_floor_and_wall_differ(){
        assert_ne!(cube_bucket(DVec3::Z, 4), cube_bucket(DVec3::X, 4));
    }

    // random directions reach every one of the 3 x k x k buckets
    #[test]
    fn cube_bucket_all_buckets_used(){
        let mut rng = rand::rngs::StdRng::seed_from_u64(92);
        let k = 4;
        let used: HashSet<Bucket> = (0..20_000).map(|_| cube_bucket(random_dir(&mut rng), k)).collect();
        assert_eq!(used.len(), (3 * k * k) as usize);
    }
}

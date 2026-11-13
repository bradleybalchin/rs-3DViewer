//! Voxelisation Sampling
//! Builds a 3D bounding box of the space containing the pointclouds
//! Divided into a grid of 3D cells of a specified size/resoltuion

use rustc_hash::FxHashMap;
use std::{time::Instant};
use crate::render::vertex::GpuPoint;
use crate::math::eigenvalues;
use crate::math::grid::{VoxelGrid,Acc};
use glam::{DVec3};

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
    let vg = VoxelGrid::new(points.iter().map(|p| p.pos), res);
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

/// Feature Aware Sampling: for each voxel, calculate how "featured" it is (e.g. not a flat plane) 
/// and then preserve more detaile in areas of higher features
pub fn sample_feature_aware(points : &Vec<GpuPoint>, res : f32) -> Result<Vec<GpuPoint>, String> {
    let start = Instant::now(); // timing
    eprintln!("Voxel Grid Sampling res = {:.2}] Beginning Sampling", res);
    // Reject negative or NaN values for resolution
    if res.is_nan() || res.is_sign_negative() {
        return Err("resolution must be positive".into());
    }
    if res == 0.0 {return Ok(points.clone())}

    // create voxel grid (vg provides cell mappings for hashmap)
    let vg = VoxelGrid::new(points.iter().map(|p| p.pos), res);
    let mut grid: FxHashMap<[i32; 3], Acc> = FxHashMap::default();

    // covariance matrix calulation
    //  Var(x)      Cov(x,y)    Cov(x,z)
    //  Cov(x,y)    Var(y)      Cov(y,z)
    //  Cov(z,x)    Cov(z,y)    Var(z)           

    // sym_eigenvalues([xx, xy, xz, yy, yz, zz]
    // var = sum((xi-mean)^2) / N
    // cov = sum((xi-xmean) (yi-ymean)) / N
    // use positions relative to the cell corner to avoid covariance losing precision

    // from sums of products (for single pass covariance calculating)
    // (x-xm) (y-ym) = xy - xym - yxm + mxmy
    // sum((x-xm) (y-ym)) = sum(xy) - sum(x)ym - sum(y)xm + n*xmym
    //                      = sum(xy) n*xmym - n*xmym + n*xmym
    //                      = sum(xy) - n*xmym
    // cov(x,y) = (sum(xy) / n) - xmym
    // average of products minus product of avarages

    // PASS 1: for each point, find grid cell and update sums
    //  positions are taken relative to the cell corner (d = p - corner) in f64, so the sums of products precise
    for p in points {
        let key = vg.cell(p.pos);
        let d = DVec3::new(p.pos[0] as f64, p.pos[1] as f64, p.pos[2] as f64) - vg.corner(key);

        let a = grid.entry(key).or_default();
        a.n += 1;
        a.sum[0] += d.x;
        a.sum[1] += d.y;
        a.sum[2] += d.z;
        a.psum[0] += d.x * d.x;   // xx
        a.psum[1] += d.x * d.y;   // xy
        a.psum[2] += d.x * d.z;   // xz
        a.psum[3] += d.y * d.y;   // yy
        a.psum[4] += d.y * d.z;   // yz
        a.psum[5] += d.z * d.z;   // zz
    }

    // BETWEEN PASSES: classify each cell as flat or feature from its covariance eigenvalues
    let cells: FxHashMap<[i32; 3], Cell> = grid.into_iter().map(|(key, a)| {
        let n = a.n as f64;
        let m = DVec3::from_array(a.sum) / n;     // mean, relative to cell corner

        // cov = average of products - product of averages
        let cov = [
            a.psum[0] / n - m.x * m.x,   // xx
            a.psum[1] / n - m.x * m.y,   // xy
            a.psum[2] / n - m.x * m.z,   // xz
            a.psum[3] / n - m.y * m.y,   // yy
            a.psum[4] / n - m.y * m.z,   // yz
            a.psum[5] / n - m.z * m.z,   // zz
        ];

        // too few points for a reliable covariance -> treat as flat (one representative)
        let feature = a.n >= MIN_FEATURE_POINTS && surface_variation(cov) > SIGMA_THRESHOLD;
        (key, Cell { centroid: m, feature })
    }).collect();

    // PASS 2: choose representatives using `cells`
    //  each point competes for a "slot" - the slot keeps the real input point nearest its target
    //  flat cell:    one slot per cell, target = cell centroid
    //  feature cell: SUB^3 slots per cell (one per sub-cell), target = sub-cell centroid (4 subdivisions = 64 slots)
    let sub_res = res as f64 / SUBDIVISIONS as f64;
    // finds slots a point competes for
    let slot_of = |p: &GpuPoint| -> (Slot, DVec3, &Cell) {
        let key = vg.cell(p.pos);
        let d = DVec3::new(p.pos[0] as f64, p.pos[1] as f64, p.pos[2] as f64) - vg.corner(key);
        let cell = &cells[&key];
        let sub = if cell.feature { sub_index(d, sub_res) } else { 0 };
        ((key, sub), d, cell)
    };

    // PASS 2a: sub-cell centroids (feature cells only) - sum of d and count per sub-cell

    // point count and sum of positions per cell
    let mut sub_sums: FxHashMap<Slot, (DVec3, u32)> = FxHashMap::default();
    for p in points {
        let (slot, d, cell) = slot_of(p);
        if cell.feature {
            let s = sub_sums.entry(slot).or_insert((DVec3::ZERO, 0));
            s.0 += d;
            s.1 += 1;
        }
    }

    // calc centroids (average position) for all subdivisions in slots
    let sub_centroids: FxHashMap<Slot, DVec3> = sub_sums.into_iter()
        .map(|(slot, (sum, n))| (slot, sum / n as f64))
        .collect();

    // PASS 2b: for each slot keep the point nearest its target (distance^2, index into points)
    let mut best: FxHashMap<Slot, (f64, usize)> = FxHashMap::default();
    for (i, p) in points.iter().enumerate() {
        let (slot, d, cell) = slot_of(p);
        // target is centroid of subdivision (if feature) or of the cell (if non-feature)
        let target = if cell.feature { sub_centroids[&slot] } else { cell.centroid };
        let d2 = d.distance_squared(target);

        match best.get_mut(&slot) {
            None => { best.insert(slot, (d2, i)); }
            Some(b) => if closer(d2, p, b.0, &points[b.1]) { *b = (d2, i); },
        }
    }

    let selected: Vec<GpuPoint> = best.into_values().map(|(_, i)| points[i]).collect();

    let ms = |d: std::time::Duration| d.as_secs_f64() * 1000.0;
    eprintln!("[Feature Aware Sampling res = {:.2}] Input Count: {:?} Sample Count: {:?}, Output, Time: {:.1} ms", res, points.len(), selected.len(), ms(start.elapsed()));

    Ok(selected)

}


/// Minimum points in a cell for its covariance to be trusted; below this a cell is treated as flat
const MIN_FEATURE_POINTS: u32 = 5;

/// Surface variation above which a cell counts as a feature.
/// Flat plane ~0 up to 1/3 (random scatter in all directions)
const SIGMA_THRESHOLD: f64 = 0.02;

/// Feature cells are split into SUBDIVISIONS^3 sub-cells, each keeping one point.
/// Higher keeps more detail at edges/corners (and more points).
const SUBDIVISIONS: u16 = 4;

/// A representative slot: (cell key, sub-cell index). Flat cells only use sub-cell 0.
type Slot = ([i32; 3], u16);

/// Index of the sub-cell containing `d` (position relative to the cell corner), sub-cells of size `sub_res`.
/// Clamped because f32 rounding in `VoxelGrid::cell` can leave d fractionally outside [0, res].
fn sub_index(d: DVec3, sub_res: f64) -> u16 {
    let max = (SUBDIVISIONS - 1) as f64;
    let s = (d / sub_res).floor().clamp(DVec3::ZERO, DVec3::splat(max));
    (s.x as u16 * SUBDIVISIONS + s.y as u16) * SUBDIVISIONS + s.z as u16
}

/// True if candidate `p` (distance^2 `d2` from target) should replace the current best.
/// Ties broken on position then colour, never on input index, so the result is independent of input order.
fn closer(d2: f64, p: &GpuPoint, best_d2: f64, best: &GpuPoint) -> bool {
    d2.total_cmp(&best_d2)
        .then_with(|| p.pos[0].total_cmp(&best.pos[0]))
        .then_with(|| p.pos[1].total_cmp(&best.pos[1]))
        .then_with(|| p.pos[2].total_cmp(&best.pos[2]))
        .then_with(|| p.color.cmp(&best.color))
        .is_lt()
}

/// Classification of one grid cell after pass 1
struct Cell {
    centroid: DVec3, // mean position, relative to the cell corner
    feature: bool,   // true if the cell is not flat (edge, corner, scatter)
}

/// Surface variation Ïƒ = Î»3 / (Î»1 + Î»2 + Î»3) of a covariance matrix [xx, xy, xz, yy, yz, zz].
/// 0 for a perfect plane (or line), up to 1/3 for isotropic scatter.
fn surface_variation(cov: [f64; 6]) -> f64 {
    let [l1, l2, l3] = eigenvalues::sym_eigenvalues(cov).map(|l| l.max(0.0)); // rounding can give tiny negatives
    let sum = l1 + l2 + l3;
    if sum > 0.0 { l3 / sum } else { 0.0 } // sum = 0: all points identical
}



#[cfg(test)]
mod tests {

    use std::collections::HashSet;

    use more_asserts::{self, assert_le, assert_lt};
    use rand::{RngExt, SeedableRng};
    use crate::render::vertex::GpuPoint;
    use super::{sample_grid,sample_feature_aware};

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

    // FEATURE AWARE SAMPLING /////////////////////
    #[test]
    fn feature_aware_sample_weakly_smaller(){
        let pnts = gen_test_data(1_000, 42);
        let rs = [0.0, 0.5, 1.0, 2.0, 5.0, 10.0, 100.0];

        for r in rs {
            assert_le!(sample_feature_aware(&pnts, r).unwrap().len(), pnts.len(), "res={r}")
        }
    }
    
    // res must be a positive f32 value, sample should accept no other values for r
    #[test]
    fn feature_res_negative_or_nan() {
        let pnts = gen_test_data(1_000, 42);

        assert!(if let Err(_) = sample_feature_aware(&pnts, -1.0) {true} else {false}, "Negative res");
        assert!(if let Err(_) = sample_feature_aware(&pnts, f32::NAN) {true} else {false}, "NaN res");

    }

    // Output sample contains no duplicate points
    #[test]
    fn feature_aware_sample_no_duplicates() {
        let pnts = gen_test_data(1_000, 42);
        let rs = [0.0, 0.5, 1.0, 2.0, 5.0, 10.0, 100.0];

        for r in rs {
            let sample = sample_feature_aware(&pnts, r).unwrap();
            let mut seen: HashSet<GpuPoint> = HashSet::new();
            for p in sample {
                assert!(seen.insert(p), "r={r}")
            }   
        }
    }

    // if res is zero, then the sample should equal the input (no sampling performed)
    #[test]
    fn feature_aware_res_zero() {
        let pnts = gen_test_data(1_000, 42);
        let r = 0.0f32;

        let sample = sample_feature_aware(&pnts, r).unwrap();
        assert_eq!(sample.len(), pnts.len(), "Len: sample {:?} != input {:?}", sample.len(), pnts.len());
        for p in pnts {
            assert!(sample.contains(&p));
        }
    }

    // - - shape generation for feature aware testing

    // flat plane z = 0 covering [0, extent) x [0, extent)
    fn gen_plane(numpoints: usize, extent: f32, seed: u64) -> Vec<GpuPoint> {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        (0..numpoints)
            .map(|_| GpuPoint::new([rng.random_range(0.0..extent), rng.random_range(0.0..extent), 0.0], rng.random()))
            .collect()
    }

    // "L" shape: floor (z = 0) and wall (y = 0) meeting at a sharp edge along the x axis (y = 0, z = 0)
    fn gen_l_shape(points_per_plane: usize, extent: f32, seed: u64) -> Vec<GpuPoint> {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let mut pnts = Vec::with_capacity(points_per_plane * 2);
        for _ in 0..points_per_plane {
            pnts.push(GpuPoint::new([rng.random_range(0.0..extent), rng.random_range(0.0..extent), 0.0], rng.random()));
            pnts.push(GpuPoint::new([rng.random_range(0.0..extent), 0.0, rng.random_range(0.0..extent)], rng.random()));
        }
        pnts
    }

    // points on the 6 faces of the cube [0, side]^3
    fn gen_cube_surface(points_per_face: usize, side: f32, seed: u64) -> Vec<GpuPoint> {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let mut pnts = Vec::with_capacity(points_per_face * 6);
        for face in 0..6 {
            let axis = face % 3;                                  // which axis is fixed
            let fixed = if face < 3 { 0.0 } else { side };     // which side of the cube
            for _ in 0..points_per_face {
                let mut pos = [rng.random_range(0.0..side), rng.random_range(0.0..side), rng.random_range(0.0..side)];
                pos[axis] = fixed;
                pnts.push(GpuPoint::new(pos, rng.random()));
            }
        }
        pnts
    }

    // distance from p to the L-shape edge line (the x axis)
    fn dist_to_edge(p: &GpuPoint) -> f32 {
        (p.pos[1] * p.pos[1] + p.pos[2] * p.pos[2]).sqrt()
    }

    fn dist3(a: [f32; 3], b: [f32; 3]) -> f32 {
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
    }

    // - -

    // covariance [xx, xy, xz, yy, yz, zz] of a set of positions
    fn covariance(pts: &[[f64; 3]]) -> [f64; 6] {
        let n = pts.len() as f64;
        let m = pts.iter().fold([0.0; 3], |s, p| [s[0] + p[0], s[1] + p[1], s[2] + p[2]]).map(|v| v / n);
        let mut c = [0.0; 6];
        for p in pts {
            let d = [p[0] - m[0], p[1] - m[1], p[2] - m[2]];
            let t = [d[0] * d[0], d[0] * d[1], d[0] * d[2], d[1] * d[1], d[1] * d[2], d[2] * d[2]];
            for i in 0..6 { c[i] += t[i] / n; }
        }
        c
    }

    // SIGMA_THRESHOLD separates a flat cell from an edge cell (one unit voxel of each)
    #[test]
    fn surface_variation_flat_vs_edge() {
        use super::{surface_variation, SIGMA_THRESHOLD};
        let mut rng = rand::rngs::StdRng::seed_from_u64(31);

        let flat: Vec<[f64; 3]> = (0..200).map(|_| [rng.random_range(0.0..1.0), rng.random_range(0.0..1.0), 0.0]).collect();
        let edge: Vec<[f64; 3]> = (0..200).map(|i| {
            let (a, b) = (rng.random_range(0.0..1.0), rng.random_range(0.0..1.0));
            if i % 2 == 0 { [a, b, 0.0] } else { [a, 0.0, b] } // floor or wall
        }).collect();

        let (s_flat, s_edge) = (surface_variation(covariance(&flat)), surface_variation(covariance(&edge)));
        assert!(s_flat < SIGMA_THRESHOLD, "flat cell sigma {s_flat}");
        assert!(s_edge > SIGMA_THRESHOLD, "edge cell sigma {s_edge}");
    }

    // Empty input is valid and gives empty output
    #[test]
    fn feature_aware_empty_input() {
        let pnts: Vec<GpuPoint> = Vec::new();
        assert!(sample_feature_aware(&pnts, 1.0).unwrap().is_empty());
    }

    // feature aware keeps real input points, not centroids / averages
    #[test]
    fn feature_aware_output_is_subset_of_input() {
        let pnts = gen_l_shape(5_000, 10.0, 7);
        for r in [0.5, 1.0, 2.0] {
            let input: HashSet<GpuPoint> = pnts.iter().copied().collect();
            let sample = sample_feature_aware(&pnts, r).unwrap();
            assert!(sample.iter().all(|p| input.contains(p)), "res={r}");
        }
    }

    // flat plane with no features - keep ~ 1 point per voxel, so roughly the same as grid sampling 
    #[test]
    fn feature_aware_flat_plane_reduces_like_grid() {
        let pnts = gen_plane(20_000, 10.0, 3);
        for r in [0.5, 1.0, 2.0] {
            let fa = sample_feature_aware(&pnts, r).unwrap().len();
            let grid = sample_grid(&pnts, r).unwrap().len();
            assert_le!(fa as f32, grid as f32 * 1.05, "res={r}: feature aware kept {fa}, grid kept {grid}");
            assert_lt!(fa, pnts.len() / 4, "res={r}: flat plane barely reduced ({fa} of {})", pnts.len());
        }
    }

    // A single flat voxel covering the whole plane should collapse to one point (like with grid sampling)
    #[test]
    fn feature_aware_res_larger_than_flat_input() {
        let pnts = gen_plane(1_000, 10.0, 3);
        assert_eq!(sample_feature_aware(&pnts, 1_000.0).unwrap().len(), 1);
    }

    // An L shape has a sharp edge, so feature aware sampling should keep more points than plain grid sampling
    #[test]
    fn feature_aware_edge_keeps_more_than_grid() {
        let pnts = gen_l_shape(10_000, 10.0, 11);
        for r in [0.5, 1.0] {
            let fa = sample_feature_aware(&pnts, r).unwrap().len();
            let grid = sample_grid(&pnts, r).unwrap().len();
            assert_lt!(grid, fa, "res={r}: feature aware kept {fa}, grid kept {grid}");
        }
    }

    // Points kept near the edge should be denser (relative to input) than points kept on the flat parts
    #[test]
    fn feature_aware_edge_region_denser_than_flat_region() {
        let pnts = gen_l_shape(10_000, 10.0, 13);
        let r = 1.0;
        let sample = sample_feature_aware(&pnts, r).unwrap();

        let near = |p: &&GpuPoint| dist_to_edge(p) < r;           // voxels touching the edge
        let far = |p: &&GpuPoint| dist_to_edge(p) > 3.0 * r;      // clearly flat region

        let kept_near = sample.iter().filter(near).count() as f32 / pnts.iter().filter(near).count() as f32;
        let kept_far = sample.iter().filter(far).count() as f32 / pnts.iter().filter(far).count() as f32;
        assert!(kept_near > 2.0 * kept_far, "kept fraction near edge {kept_near:.3} vs flat {kept_far:.3}");
    }

    // The edge itself should still be represented: grid centroids sit inside the corner (~0.35 res away),
    // feature aware should keep points actually on/next to the edge line
    #[test]
    fn feature_aware_keeps_points_on_edge() {
        let pnts = gen_l_shape(10_000, 10.0, 17);
        let r = 1.0;
        let close = 0.1 * r;

        let fa_on_edge = sample_feature_aware(&pnts, r).unwrap().iter().filter(|p| dist_to_edge(p) < close).count();
        let grid_on_edge = sample_grid(&pnts, r).unwrap().iter().filter(|p| dist_to_edge(p) < close).count();
        assert!(fa_on_edge > grid_on_edge, "points within {close} of edge: feature aware {fa_on_edge}, grid {grid_on_edge}");
        assert!(fa_on_edge >= 10, "only {fa_on_edge} points kept on a 10 unit long edge");
    }

    // Cube corners survive heavy reduction better than with grid sampling
    #[test]
    fn feature_aware_preserves_cube_corners() {
        let side = 10.0;
        let pnts = gen_cube_surface(20_000, side, 19);
        let r = 1.0;
        let fa = sample_feature_aware(&pnts, r).unwrap();
        let grid = sample_grid(&pnts, r).unwrap();

        // return distance from point c to closes point in s
        // maps distance to each point in s, then uses fold to find minimum from them (iteravely compares with f32::min)
        // fold(start, f) -> where f(running, item), each iteration, replace running with function output and item = next item in iteration (beginning with start) 
        let nearest = |s: &Vec<GpuPoint>, c: [f32; 3]| s.iter().map(|p| dist3(p.pos, c)).fold(f32::INFINITY, f32::min);
        for &x in &[0.0, side] { for &y in &[0.0, side] { for &z in &[0.0, side] {// each side (0,0,0), (0,0,10), etc.
            let c = [x, y, z];
            let (d_fa, d_grid) = (nearest(&fa, c), nearest(&grid, c));
            assert!(d_fa < d_grid, "corner {c:?}: feature aware nearest {d_fa:.3}, grid nearest {d_grid:.3}");
            assert!(d_fa < 0.5 * r, "corner {c:?}: nearest kept point {d_fa:.3} away");
        }}}
    }

    // Voxels with too few points for a reliable covariance are treated as non-feature (one point per voxel)
    #[test]
    fn feature_aware_sparse_voxels_not_features() {
        // 2 points per voxel, voxels spaced far apart
        let mut pnts = Vec::new();
        for i in 0..50 {
            let base = i as f32 * 10.0;
            pnts.push(GpuPoint::new([base + 0.1, 0.2, 0.3], 1));
            pnts.push(GpuPoint::new([base + 0.6, 0.7, 0.4], 2));
        }
        assert_eq!(sample_feature_aware(&pnts, 1.0).unwrap().len(), 50);
    }

    // Result does not depend on input order
    #[test]
    fn feature_aware_order_independent() {
        use rand::seq::SliceRandom;
        let pnts = gen_l_shape(5_000, 10.0, 23);
        let mut shuffled = pnts.clone();
        shuffled.shuffle(&mut rand::rngs::StdRng::seed_from_u64(99));

        let a: HashSet<GpuPoint> = sample_feature_aware(&pnts, 1.0).unwrap().into_iter().collect();
        let b: HashSet<GpuPoint> = sample_feature_aware(&shuffled, 1.0).unwrap().into_iter().collect();
        assert!(a == b, "different output for shuffled input ({} vs {} points)", a.len(), b.len());
    }

    // Translating the cloud far from the origin should not change the result (catches covariance precision loss)
    #[test]
    fn feature_aware_translation_invariant() {
        let pnts = gen_l_shape(10_000, 10.0, 29);
        let offset = [10_000.0f32, -10_000.0, 5_000.0];
        let moved: Vec<GpuPoint> = pnts.iter()
            .map(|p| GpuPoint::new([p.pos[0] + offset[0], p.pos[1] + offset[1], p.pos[2] + offset[2]], p.color))
            .collect();

        let a = sample_feature_aware(&pnts, 1.0).unwrap().len() as f32;
        let b = sample_feature_aware(&moved, 1.0).unwrap().len() as f32;
        // small tolerance: f32 rounding of the offset can move a few points across voxel boundaries
        assert!((a - b).abs() <= 0.02 * a, "original kept {a}, translated kept {b}");
    }


    // GRID SAMPLING //////////////////////////////


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
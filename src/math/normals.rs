//! # Normals
//! 
//! Directional vector that points at a perpedicular angle to the surface it is on

use glam::DVec3;
use rustc_hash::FxHashMap;

use crate::math::grid;
use crate::math::eigenvalues;

use crate::render::vertex::GpuPoint;

// Position trait -> return position so that math does not have to depend on the GpuPoint Rendering object
pub trait Position { fn pos(&self) -> [f32; 3]; }
impl Position for GpuPoint { fn pos(&self) -> [f32; 3] { self.pos } }
impl Position for [f32; 3]  { fn pos(&self) -> [f32; 3] { *self } }

/// Estimate a surface normal for every point, using one normal per grid cell.
/// Returns one entry per input point (same order): the normal of that point's cell,
/// or `None` if the cell has no reliable normal.
///
/// All output normal vectors are normalised (magnitude = 1) \
/// This function is O(N)
/// 
/// ### Method
/// 1. Bucket points into a spatial grid of r x r x r cells (`VoxelGrid`).
/// 2. Per cell, accumulate sums and sums of products of positions (relative to the cell corner for precision)
///    and compute the 3x3 covariance matrix.
/// 3. Eigenvalues of the covariance (`sym_eigenvalues`): λ1 >= λ2 >= λ3.
/// 4. Normal = unit eigenvector of the smallest eigenvalue λ3 (direction of least spread).
///
/// None when the cell has too few points, or λ3 is repeated (points on a line, or isotropic scatter),
/// so the normal is not unique. The sign is arbitrary: n and -n are both valid.
fn estimate_normals<P: Position>(points : &[P], r : f32) -> Vec<Option<DVec3>>{

    // if r is zero, negative or NaN, return no normals
    if r ==0.0 || r.is_sign_negative() || r.is_nan() {
        return vec![None; points.len()];
    }


    // create spacial grid (vg provides cell mappings for hashmap)
    let vg = grid::VoxelGrid::new(points.iter().map(|p| p.pos()), r);
    let mut grid: FxHashMap<[i32; 3], grid::Acc> = FxHashMap::default();


    // PASS 1: for each point, find grid cell and update sums
    //  positions are taken relative to the cell corner (d = p - corner) in f64, so the sums of products precise
    for p in points {
        let key = vg.cell(p.pos());
        let d = DVec3::new(p.pos()[0] as f64, p.pos()[1] as f64, p.pos()[2] as f64) - vg.corner(key);

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

    // Between passes : calculate normals for each grid cell ()
    let cell_norms: FxHashMap<[i32; 3], Option<DVec3>> = grid.into_iter().map(|(key, a)| {
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

        // eigenvalues
        let [l1, l2, l3] = eigenvalues::sym_eigenvalues(cov).map(|l| l.max(0.0)); // rounding can give tiny negatives
        // use smallest eigen vector (direction perpendicular to main 'plane' of data )
        (key, eigenvalues::eigenvector(cov, l3))

    }).collect();

    // Pass 2, assign each point a normal based on its voxel grid (coarse version)
    // TODO, assign normal based in distance to centroid of grid cell and nearby cells?
    points.iter().map(|p| cell_norms[&vg.cell(p.pos())]).collect()

    
}



#[cfg(test)]
mod tests { 
    use glam::DVec3;
use rand::{RngExt, SeedableRng};

    use crate::render::vertex::GpuPoint;

    use super::estimate_normals;

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

    // flat plane z = 0 covering [0, extent) x [0, extent)
    fn gen_plane(numpoints: usize, extent: f32, seed: u64) -> Vec<GpuPoint> {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        (0..numpoints)
            .map(|_| GpuPoint::new([rng.random_range(0.0..extent), rng.random_range(0.0..extent), 0.0], rng.random()))
            .collect()
    }

    // Returns a single normal per point
    #[test]
    fn estimate_normals_one_normal_per_point(){
        let pnts = gen_test_data(10_000, 66);
        let rs = [0.0, 0.5, 1.0, 2.0, 5.0, 10.0];

        for r in rs {
            let norms = estimate_normals(&pnts, r);
            assert_eq!(pnts.len(), norms.len(), "points = {}, norms = {}", pnts.len(), norms.len());
        }
    }

    // r must be a non-zero positive value, if not no normals should be generated (all None)
    #[test]
    fn estimate_normals_r_zero_negative_or_nan(){
        let pnts = gen_test_data(1_000, 66);

        assert_eq!(estimate_normals(&pnts, -1.0), vec![None; pnts.len()], "r = -1.0");
        assert_eq!(estimate_normals(&pnts, 0.0), vec![None; pnts.len()], "r = 0.0");
        assert_eq!(estimate_normals(&pnts, f32::NAN), vec![None; pnts.len()], "r = NaN");
    }

    // All normals on a flat plane are identical and not None
    #[test]
    fn estimate_normals_plane_normals_identical(){
        let pnts = gen_plane(20_000, 10.0, 67);
        for r in [0.5, 1.0, 2.0] {
            let norms = estimate_normals(&pnts, r);
            for n in &norms {
                let n = n.expect("plane point should have a normal");
                // Cross product = 0 (within tolerance) normal vectors are parallel (identical direction) fine if opposite (normal vectors sign is arbitrary)
                assert!(n.cross(DVec3::Z).length() < 1e-9, "r = {r}: normal {n} not parallel to z"); // within tolerance
            }

        }
    }

    // All normal vectors are normalised (magnitude = 1)
    #[test]
    fn estimate_normals_plane_normals_normalised(){
    let pnts = gen_plane(20_000, 10.0, 66);
    for r in [0.5, 1.0, 2.0, 5.0] {
        let ns: Vec<DVec3> = estimate_normals(&pnts, r).into_iter().flatten().collect();
        assert!(!ns.is_empty(), "r = {r}: no normals produced");
        assert!(ns.iter().all(|n| (n.length() - 1.0).abs() < 1e-9), "r = {r}");
    }
}
}
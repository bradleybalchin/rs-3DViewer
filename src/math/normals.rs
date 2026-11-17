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

/// Estimate a surface normal for every point, by fitting a plane to each grid cell
/// and blending the planes of nearby cells for each point.
/// Returns one entry per input point (same order), or `None` if no nearby cell has a reliable normal.
///
/// All output normal vectors are normalised (magnitude = 1) \
/// This function is O(N)
///
/// ### Method
/// 1. Bucket points into a spatial grid of r x r x r cells (`VoxelGrid`).
/// 2. Per cell, accumulate sums and sums of products of positions (relative to the cell corner for precision)
///    and compute the 3x3 covariance matrix.
/// 3. Eigenvalues of the covariance (`sym_eigenvalues`): λ1 >= λ2 >= λ3.
///    Cell normal = unit eigenvector of the smallest eigenvalue λ3 (direction of least spread).
///    Surface variation λ3 / (λ1 + λ2 + λ3) marks mixed cells (edges, corners) that fit no single plane.
/// 4. Per point, over the 3x3x3 block of cells:
///    - reference = normal of the planar cell whose plane the point lies closest to
///      (chosen per point, so points in a cell that straddles an edge still pick their own surface)
///    - blend the normals that face within 30° of the reference, weighted by distance to the cell
///      and distance to the cell's plane, leaving out mixed cells when planar ones are available
///
/// A cell has no normal when it has fewer than 5 points, or λ3 is repeated (points on a line, or isotropic
/// scatter). The sign is arbitrary: n and -n are both valid.
pub fn estimate_normals<P: Position>(points : &[P], r : f32) -> Vec<Option<DVec3>>{

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

    // Between passes : calculate normals for each grid cell
    const MIN_POINTS: u32 = 5;            // 3 points always fit a perfect plane, so need more
    const EDGE_COS: f64 = 0.866;          // cos 30°: neighbours facing more than this differently are a different surface
    const MAX_REF_VARIATION: f64 = 0.01;  // cells with surface variation above this are mixed (edges, corners)

    // cell -> fitted plane; only cells that have a normal
    let cells: FxHashMap<[i32; 3], CellPlane> = grid.into_iter().filter_map(|(key, a)| {
        if a.n < MIN_POINTS { return None; }
        let n = a.n as f64;
        // covariance : average of products - products of avarages
        let m = DVec3::from_array(a.sum) / n;
        let cov = [
            a.psum[0] / n - m.x * m.x,   // xx
            a.psum[1] / n - m.x * m.y,   // xy
            a.psum[2] / n - m.x * m.z,   // xz
            a.psum[3] / n - m.y * m.y,   // yy
            a.psum[4] / n - m.y * m.z,   // yz
            a.psum[5] / n - m.z * m.z,   // zz
        ];
        let [l1, l2, l3] = eigenvalues::sym_eigenvalues(cov).map(|l| l.max(0.0));
        let normal = eigenvalues::eigenvector(cov, l3)?;     // None -> cell left out
        let total = l1 + l2 + l3;
        let variation = if total > 0.0 { l3 / total } else { 0.0 };   // 0 = perfect plane, 1/3 = no plane
        Some((key, CellPlane { centroid: vg.corner(key) + m, normal, variation }))
    }).collect();

    // PASS 2: give each point a blend of nearby cell normals
    //
    //  weight = closeness in space x closeness to the cell's plane ("bilateral" weight)
    //   spatial: Gaussian exp(-d² / 2σ²), σ = r/2   (d = 0.5r -> 0.61, d = r -> 0.14, d = 1.5r -> 0.01)
    //   plane:   Gaussian exp(-h² / 2τ²), τ = r/10  where h = distance from the point to the cell's plane
    //  so cells on a different surface, or mixed cells whose plane fits neither surface, count for little
    let two_sigma2 = 2.0 * (r as f64 * 0.5).powi(2);
    let two_tau2 = 2.0 * (r as f64 * 0.1).powi(2);

    points.iter().map(|p| {
        let pos = DVec3::from_array(p.pos().map(|c| c as f64));
        let key = vg.cell(p.pos());

        // every cell in the 3x3x3 block that has a normal, with the point's distance to its plane
        let nbrs: Vec<(&CellPlane, f64)> = grid::VoxelGrid::neighbours(key)
            .filter_map(|k| cells.get(&k))
            .map(|c| (c, (pos - c.centroid).dot(c.normal).abs()))
            .collect();

        // reference direction, chosen per point (not per cell): the plane this point lies closest to.
        //  A roof point in a cell that also holds wall points lies on the roof cells' plane, so it picks the roof
        //  even though its own cell's (mixed) normal is wall-ish.
        //  Prefer clean, planar cells; fall back to any cell when there are none nearby (e.g. on a tight curve).

        // return the closes normal to the plane the point lies on
        let closest = |planar_only: bool| nbrs.iter()
            .filter(|(c, _)| !planar_only || c.variation <= MAX_REF_VARIATION)
            .min_by(|a, b| a.1.total_cmp(&b.1)) // min distance
            .map(|(c, _)| c.normal);                                            // return normal
        // reference direction (chosen per point) -> closest normal in neighbors to the point
        let reference = closest(true).or_else(|| closest(false))?;

        // weighted sum of neighbour normals that face the same way as the reference
        //  planar_only: leave out mixed cells, whose tilted normal can be within EDGE_COS of the reference
        //  and would otherwise pull points near an edge towards the other surface
        let blend = |planar_only: bool| {
            let mut sum = DVec3::ZERO;
            for &(c, h) in &nbrs {
                if planar_only && c.variation > MAX_REF_VARIATION { continue; }
                let d = reference.dot(c.normal);
                if d.abs() < EDGE_COS { continue; }                 // different surface (across an edge): skip
                let n = if d < 0.0 { -c.normal } else { c.normal }; // make signs agree before adding
                let w = (-(pos - c.centroid).length_squared() / two_sigma2 - h * h / two_tau2).exp();
                sum += n * w;
            }
            sum.try_normalize()                                     // None if nothing contributed
        };
        blend(true).or_else(|| blend(false))
    }).collect()
}

/// Plane fitted to one grid cell's points
struct CellPlane {
    centroid: DVec3,    // mean position (world space), a point on the plane
    normal: DVec3,      // unit normal (arbitrary sign)
    variation: f64,     // surface variation λ3 / (λ1 + λ2 + λ3): 0 for a perfect plane, higher for edges and corners
}

/// Components with magnitude at or below this count as zero in `canonical`,
/// so rounding noise near the equator (z ≈ ±1e-17) doesn't decide the sign
const CANONICAL_EPS: f64 = 1e-9;

/// Choose one sign for a normal, so that `n` and `-n` map to the same vector. \
/// Estimated normals have an arbitrary sign, but both describe the same surface orientation.
///
/// Flips `n` if z < 0. If z ≈ 0 (normal lies in the xy plane), decides on x instead, then y.
/// `canonical(n) == canonical(-n)` exactly, because `n` and `-n` have the same magnitudes
/// so always take the same branch.
pub fn canonical(n: DVec3) -> DVec3 {
    let flip = if n.z.abs() > CANONICAL_EPS { n.z < 0.0 }
          else if n.x.abs() > CANONICAL_EPS { n.x < 0.0 }
          else { n.y < 0.0 };
    if flip { -n } else { n }
}



#[cfg(test)]
mod tests { 
    use glam::DVec3;
use rand::{RngExt, SeedableRng};

    use crate::render::vertex::GpuPoint;

    use super::{estimate_normals, canonical};

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

    // n and -n give exactly the same canonical vector, and it is n or -n (length unchanged)
    #[test]
    fn canonical_same_for_n_and_minus_n(){
        let mut rng = rand::rngs::StdRng::seed_from_u64(70);
        for _ in 0..10_000 {
            let n = DVec3::new(rng.random_range(-1.0..1.0), rng.random_range(-1.0..1.0), rng.random_range(-1.0..1.0)).normalize();
            let c = canonical(n);
            assert_eq!(c, canonical(-n), "n = {n}");
            assert!(c == n || c == -n, "n = {n}, canonical = {c}");
        }
    }

    // Result points into the upper hemisphere (z >= 0); on the equator x >= 0; on the y axis y >= 0
    #[test]
    fn canonical_hemisphere(){
        assert_eq!(canonical(DVec3::new(0.0, 0.6, -0.8)), DVec3::new(-0.0, -0.6, 0.8));
        assert_eq!(canonical(DVec3::new(0.0, 0.6,  0.8)), DVec3::new(0.0, 0.6, 0.8));   // already canonical: unchanged
        assert_eq!(canonical(DVec3::new(-0.6, 0.8, 0.0)), DVec3::new(0.6, -0.8, -0.0)); // z = 0: decided by x
        assert_eq!(canonical(DVec3::new(0.0, -1.0, 0.0)), DVec3::new(-0.0, 1.0, -0.0)); // z = x = 0: decided by y
    }

    // Rounding noise in z near the equator is ignored: the sign is decided by x, not by the noise
    #[test]
    fn canonical_ignores_rounding_noise(){
        let a = canonical(DVec3::new(-1.0, 0.0,  1e-17));
        let b = canonical(DVec3::new(-1.0, 0.0, -1e-17));
        assert!(a.x > 0.0 && b.x > 0.0, "a = {a}, b = {b}");
    }

    // ---------- normal assignment (pass 2) ----------

    // random unit vector (uniform on the sphere)
    fn random_unit(rng: &mut rand::rngs::StdRng) -> DVec3 {
        loop {
            let d = DVec3::new(rng.random_range(-1.0..1.0), rng.random_range(-1.0..1.0), rng.random_range(-1.0..1.0));
            let l2 = d.length_squared();
            if l2 > 1e-6 && l2 <= 1.0 { return d.normalize(); }
        }
    }

    // sphere of radius `radius` centred on the origin, with each point moved up to `noise` along its radius
    // the true normal at p is p.normalize()
    fn gen_sphere(numpoints: usize, radius: f64, noise: f64, seed: u64) -> Vec<GpuPoint> {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        (0..numpoints)
            .map(|_| {
                let p = random_unit(&mut rng) * (radius + rng.random_range(-noise..=noise));
                GpuPoint::new([p.x as f32, p.y as f32, p.z as f32], 0)
            })
            .collect()
    }

    // L-shape: floor z = 0 over [0,10) x [0,10), wall x = 0 over y in [0,10), z in [0,10)
    // returns (points, is_wall) so tests know the true normal of each point
    fn gen_l_shape(floor: usize, wall: usize, seed: u64) -> (Vec<GpuPoint>, Vec<bool>) {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let mut pnts = Vec::with_capacity(floor + wall);
        let mut is_wall = Vec::with_capacity(floor + wall);
        for _ in 0..floor {
            pnts.push(GpuPoint::new([rng.random_range(0.0..10.0), rng.random_range(0.0..10.0), 0.0], 0));
            is_wall.push(false);
        }
        for _ in 0..wall {
            pnts.push(GpuPoint::new([0.0, rng.random_range(0.0..10.0), rng.random_range(0.0..10.0)], 0));
            is_wall.push(true);
        }
        (pnts, is_wall)
    }

    // angle between two normals in degrees, ignoring sign (n and -n are the same orientation)
    fn angle_deg(a: DVec3, b: DVec3) -> f64 {
        a.dot(b).abs().min(1.0).acos().to_degrees()
    }

    // point position as f64
    fn pos(p: &GpuPoint) -> DVec3 { DVec3::new(p.pos[0] as f64, p.pos[1] as f64, p.pos[2] as f64) }

    // Normals on a noisy sphere point along the radius: on average within a few degrees of the true normal,
    // and almost every point gets a normal
    #[test]
    fn sphere_normals_accurate(){
        let pnts = gen_sphere(50_000, 5.0, 0.01, 100);
        let norms = estimate_normals(&pnts, 1.0);

        let errs: Vec<f64> = pnts.iter().zip(&norms)
            .filter_map(|(p, n)| n.map(|n| angle_deg(n, pos(p).normalize())))
            .collect();
        let found = errs.len() as f64 / pnts.len() as f64;
        let mean = errs.iter().sum::<f64>() / errs.len() as f64;

        assert!(found > 0.95, "only {:.1}% of points got a normal", found * 100.0);
        assert!(mean < 5.0, "mean error {mean:.2} degrees");
    }

    // Normals change smoothly across cell boundaries: two points very close together but in different cells
    // should have almost the same normal.
    // Coarse (one normal per cell) fails this: on a sphere of radius 5 with r = 1, neighbouring cells'
    // normals differ by about r / radius = 0.2 rad (~11 degrees), so every boundary is a visible step.
    #[test]
    fn sphere_normals_continuous_across_cells(){
        let (radius, r) = (5.0, 1.0_f32);
        let mut pnts = gen_sphere(50_000, radius, 0.0, 101);

        // add close pairs: p and q = p moved 0.02 along the surface; track their indices
        let mut rng = rand::rngs::StdRng::seed_from_u64(102);
        let mut pairs: Vec<(usize, usize)> = Vec::new();
        for _ in 0..5_000 {
            let n = random_unit(&mut rng);
            let t = n.cross(random_unit(&mut rng)).normalize();     // tangent direction
            let p = n * radius;
            let q = (p + t * 0.02).normalize() * radius;
            pairs.push((pnts.len(), pnts.len() + 1));
            pnts.push(GpuPoint::new([p.x as f32, p.y as f32, p.z as f32], 0));
            pnts.push(GpuPoint::new([q.x as f32, q.y as f32, q.z as f32], 0));
        }

        let norms = estimate_normals(&pnts, r);
        let vg = crate::math::grid::VoxelGrid::new(pnts.iter().map(|p| p.pos), r);   // same grid as estimate_normals

        // only pairs that straddle a cell boundary, and where both got a normal
        let jumps: Vec<f64> = pairs.iter()
            .filter(|&&(a, b)| vg.cell(pnts[a].pos) != vg.cell(pnts[b].pos))
            .filter_map(|&(a, b)| Some(angle_deg(norms[a]?, norms[b]?)))
            .collect();
        assert!(jumps.len() > 50, "too few pairs straddle a boundary ({}) for a meaningful test", jumps.len());

        let mean = jumps.iter().sum::<f64>() / jumps.len() as f64;
        assert!(mean < 2.0, "mean jump across cell boundaries {mean:.2} degrees ({} pairs)", jumps.len());
    }

    // Blending must not blur across a sharp edge: points well away from the L-shape's corner line
    // keep the pure normal of their own surface (floor -> z, wall -> x)
    #[test]
    fn l_shape_edge_not_blurred(){
        let r = 1.0_f32;
        let (pnts, is_wall) = gen_l_shape(40_000, 40_000, 103);
        let norms = estimate_normals(&pnts, r);

        for ((p, n), wall) in pnts.iter().zip(&norms).zip(&is_wall) {
            let dist_to_edge = (p.pos[0].powi(2) + p.pos[2].powi(2)).sqrt();   // distance to the line x = z = 0
            if dist_to_edge < 2.5 * r { continue; }

            let truth = if *wall { DVec3::X } else { DVec3::Z };
            let n = n.unwrap_or_else(|| panic!("no normal at {:?}", p.pos));
            assert!(angle_deg(n, truth) < 1.0, "{:?} ({}): normal {n} is {:.2} degrees off",
                p.pos, if *wall { "wall" } else { "floor" }, angle_deg(n, truth));
        }
    }

    // Shuffling the input doesn't change any point's normal (up to sign and rounding)
    #[test]
    fn normals_independent_of_input_order(){
        use rand::seq::SliceRandom;

        let pnts = gen_sphere(20_000, 5.0, 0.01, 104);
        let norms = estimate_normals(&pnts, 1.0);

        let mut order: Vec<usize> = (0..pnts.len()).collect();
        order.shuffle(&mut rand::rngs::StdRng::seed_from_u64(105));
        let shuffled: Vec<GpuPoint> = order.iter().map(|&i| pnts[i]).collect();
        let shuffled_norms = estimate_normals(&shuffled, 1.0);

        for (k, &i) in order.iter().enumerate() {
            match (norms[i], shuffled_norms[k]) {
                (Some(a), Some(b)) => assert!(angle_deg(a, b) < 1e-4, "{:?}: {a} vs {b}", pnts[i].pos),
                (None, None) => {}
                (a, b) => panic!("{:?}: {a:?} vs {b:?}", pnts[i].pos),
            }
        }
    }

    // A cell with fewer than 5 points has no reliable normal: 3 points always fit a perfect plane,
    // so the "normal" would just be noise. An isolated group of 4 points gets None; 5 coplanar points get z.
    #[test]
    fn too_few_points_no_normal(){
        let four: Vec<[f32; 3]> = vec![[0.1, 0.1, 0.1], [0.6, 0.2, 0.3], [0.3, 0.7, 0.2], [0.5, 0.5, 0.8]];
        assert!(estimate_normals(&four, 10.0).iter().all(|n| n.is_none()), "{:?}", estimate_normals(&four, 10.0));

        let five: Vec<[f32; 3]> = vec![[0.1, 0.1, 0.0], [0.6, 0.2, 0.0], [0.3, 0.7, 0.0], [0.5, 0.5, 0.0], [0.9, 0.3, 0.0]];
        for n in estimate_normals(&five, 10.0) {
            let n = n.expect("5 coplanar points should get a normal");
            assert!(angle_deg(n, DVec3::Z) < 1e-6, "normal {n}");
        }
    }

    // Like the roof in the viewer: a floor (z = 0) meets a vertical wall along an edge at 30° to the grid axes,
    // so the cells along the edge hold points from both surfaces. Points a short distance (0.3r) from the edge
    // must still get their own surface's normal, not a wall/floor mix copied from their cell.
    #[test]
    fn diagonal_edge_not_blurred(){
        let r = 1.0_f32;
        let (s, c) = (30f64.to_radians().sin(), 30f64.to_radians().cos());
        let wall_normal = DVec3::new(-s, c, 0.0);           // wall contains the edge direction (c, s, 0) and z

        let mut rng = rand::rngs::StdRng::seed_from_u64(106);
        let mut pnts: Vec<GpuPoint> = Vec::new();
        let mut truth: Vec<(DVec3, f64)> = Vec::new();      // (true normal, distance to edge)

        // floor on the +wall_normal side of the edge line, within a 10 x 10 square
        while pnts.len() < 60_000 {
            let p = DVec3::new(rng.random_range(0.0..10.0), rng.random_range(0.0..10.0), 0.0);
            let side = p.dot(wall_normal);                   // distance from the edge line in the floor
            if side <= 0.0 { continue; }
            pnts.push(GpuPoint::new([p.x as f32, p.y as f32, 0.0], 0));
            truth.push((DVec3::Z, side));
        }
        // wall: along the edge for t in [0, 11), height h in [0, 10)
        for _ in 0..60_000 {
            let (t, h) = (rng.random_range(0.0..11.0), rng.random_range(0.0..10.0));
            pnts.push(GpuPoint::new([(t * c) as f32, (t * s) as f32, h as f32], 0));
            truth.push((wall_normal, h));
        }

        let norms = estimate_normals(&pnts, r);
        let mut bad = 0;
        let mut checked = 0;
        let mut worst = (0.0, [0.0f32; 3]);
        for ((p, n), (t, dist)) in pnts.iter().zip(&norms).zip(&truth) {
            if *dist < 0.3 * r as f64 { continue; }
            checked += 1;
            let err = n.map_or(90.0, |n| angle_deg(n, *t));
            if err > 5.0 { bad += 1; }
            if err > worst.0 { worst = (err, p.pos); }
        }
        assert!(bad == 0, "{bad} of {checked} points are more than 5 degrees off (worst {:.1} degrees at {:?})", worst.0, worst.1);
    }
}

use glam::{DMat3, DVec3};
use std::f64::consts::PI;


/// Eigenvalues of a symmetric 3x3 matrix, sorted largest first (l1 >= l2 >= l3). \
/// Input is the upper triangle of A: [xx, xy, xz, yy, yz, zz] (e.g. a 3D covariance matrix). \
///
/// ### Trigonometric solution
/// If A = pB + qI. Then A and B share eigenvectors, and β is an eigenvalue of B  \
/// iff α = pβ + q is an eigenvalue of A. \
/// - q = tr(A)/3 (mean eigenvalue), so tr(B) = 0 \
/// - p = sqrt(tr((A - qI)^2) / 6), so the eigenvalues of B satisfy Σβ² = 6 \
///
/// With these choices B's characteristic polynomial is  β³ - 3β - det(B) = 0.  \
/// Substituting β = 2cos φ and using cos 3φ = 4cos³φ - 3cos φ gives cos 3φ = det(B)/2, \
/// so φ = acos(det(B)/2) / 3 and β ∈ {2cos φ, 2cos(φ + 2π/3), 2cos(φ + 4π/3)}. \
/// Since φ ∈ [0, π/3], these are the largest, smallest and middle roots respectively. \
///
/// Ref: https://en.wikipedia.org/wiki/Eigenvalue_algorithm#3×3_matrices
pub fn sym_eigenvalues([xx, xy, xz, yy, yz, zz]: [f64; 6]) -> [f64; 3] {
    
    // if matrix is diagonal, then eigenvalues are the diagonal values
    let p1 = xy * xy + xz * xz + yz * yz;
    if p1 == 0.0 {
        let mut d = [xx,yy,zz];
        d.sort_by(|a,b| b.total_cmp(a));
        return d;
    }

    // 1. let q = tr(A) / 3           trace - sum of all diagonal componets
    let q = (xx + yy + zz) / 3.0;    

    // 2. let p = (tr((A-qI)^2)/6)^1/2
    let p2 = (xx - q).powi(2) + (yy - q).powi(2) + (zz - q).powi(2) + 2.0 * p1;
    let p = (p2 / 6.0).sqrt();                                     // spread of eigenvalues

    // 3. B = (A - qI) / p, whose eigenvalues lie in [-2, 2], rearranged from A = pB + qI
    //    - if A = pB + qI then A and B have the same eigenvectors
    let b = DMat3::from_cols_array(&[
        (xx - q) / p, xy / p,       xz / p,
        xy / p,       (yy - q) / p, yz / p,
        xz / p,       yz / p,       (zz - q) / p,
    ]);

    // clamp guards rounding -1 <= r <= 1
    let r = (b.determinant() / 2.0).clamp(-1.0, 1.0);              
    let phi = r.acos() / 3.0;

    // the eigenvalues satisfy eig3 <= eig2 <= eig1
    let l1 = q + 2.0 * p * phi.cos();
    let l3 = q + 2.0 * p * (phi + 2.0 * PI / 3.0).cos();
    let l2 = 3.0 * q - l1 - l3;                                    // trace = l1 + l2 + l3
    [l1, l2, l3]

}


// From wikipedia:
// % Given a real symmetric 3x3 matrix A, compute the eigenvalues
// % Note that acos and cos operate on angles in radians

// p1 = A(1,2)^2 + A(1,3)^2 + A(2,3)^2
// if (p1 == 0) 
//    % A is diagonal.
//    eig1 = A(1,1)
//    eig2 = A(2,2)
//    eig3 = A(3,3)
// else
//    q = trace(A)/3               % trace(A) is the sum of all diagonal values
//    p2 = (A(1,1) - q)^2 + (A(2,2) - q)^2 + (A(3,3) - q)^2 + 2 * p1
//    p = sqrt(p2 / 6)
//    B = (1 / p) * (A - q * I)    % I is the identity matrix
//    r = det(B) / 2

//    % In exact arithmetic for a symmetric matrix  -1 <= r <= 1
//    % but computation error can leave it slightly outside this range.
//    if (r <= -1) 
//       phi = pi / 3
//    elseif (r >= 1)
//       phi = 0
//    else
//       phi = acos(r) / 3
//    end

//    % the eigenvalues satisfy eig3 <= eig2 <= eig1
//    eig1 = q + 2 * p * cos(phi)
//    eig3 = q + 2 * p * cos(phi + (2*pi/3))
//    eig2 = 3 * q - eig1 - eig3     % since trace(A) = eig1 + eig2 + eig3
// end

/// Unit eigenvector for eigenvalue `l` of a symmetric 3x3 matrix A, given as [xx, xy, xz, yy, yz, zz]. \
/// Returns None if the eigenvector is not unique, i.e. `l` is a repeated eigenvalue
/// ### Method
/// Let M = A - λI. v is an eigenvector iff Av = λv iff Mv = 0. \
/// Each entry of Mv is a row of M dotted with v, so Mv = 0 iff v is perpendicular to every row of M. \
/// The number of independent rows (rank of M) decides how many such directions exist:
/// - rank 2 (λ not repeated): rows span a plane -> v is that plane's normal -> unique (up to sign)
/// - rank 1 (λ repeated twice): rows all parallel -> any direction perpendicular to them works -> not unique
/// - rank 0 (λ repeated three times, A = λI): rows all zero -> every direction works -> not unique
///
/// The cross product of two rows is perpendicular to both, and is zero iff they are parallel (or one is zero),
/// so: some row-pair cross product is non-zero  <=>  rank 2  <=>  unique eigenvector (that cross product).
pub fn eigenvector([xx, xy, xz, yy, yz, zz]: [f64; 6], l:f64) -> Option<DVec3> {

    //  M = A - λI    -> flatten space along the eigenvector direction
    // shifts every eigenvalue down by λ and leaves eigenvectors unchanged
    // if λ is an eigenvalue of A, then one eigenvalue will be zero (0 scale in that direction)
    let r0 = DVec3::new(xx - l, xy, xz);
    let r1 = DVec3::new(xy, yy - l, yz);
    let r2 = DVec3::new(xz, yz, zz - l);

    // Take a cross product of each row against each other
    // gives the directions perpendicular to the plane the rows form
    // a x b cross products is : perpendicular to both and length = area they form
    // all three rows lie on the same plane -> (since other direction was flattened A - λI)
    //  > two non-parallel rows define that plane
    //
    // their cross product will be perpendicular to that plane -> the eigenvector (direction flattened in previous step)
    // if λ is a repeated eigenvalue, two eigenvalues become zero -> two directions in A would have been flattened in previous step
    //  - collapsed to 1 dimension -> all parallel, so all cross products will be zero 
    //
    // picks the largest cross product for stability (largest area formed - most independant pair)
    let c = [r0.cross(r1), r0.cross(r2), r1.cross(r2)].into_iter()
        .max_by(|a,b| a.length_squared().total_cmp(&b.length_squared()))?;

    // Check if the largest cross product is non-zero (valid eigenvector) or rounding noise (near zero)
    // check is relative to row length
    // when all rows are zero -> c is 0 and row2 is 0, (0>0) so None returned
    // normalise to unit length
    let row2 = r0.length_squared().max(r1.length_squared()).max(r2.length_squared());
    (c.length_squared() > 1e-12 * row2 * row2).then(|| c.normalize())

}

#[cfg(test)]
mod tests { 

    use glam::{DMat3, DQuat, DVec3};
    use rand::{RngExt, SeedableRng};
    use super::sym_eigenvalues;

    // generate n random symmetric matrices for testing, as upper triangles [xx, xy, xz, yy, yz, zz]
    // entries uniform in [-10, 10)
    fn gen_sym_matrices3(n: usize, seed: u64) -> Vec<[f64; 6]> {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        (0..n)
            .map(|_| std::array::from_fn(|_| rng.random_range(-10.0..10.0)))
            .collect()
    }

    // full 3x3 matrix from upper triangle
    fn to_mat([xx, xy, xz, yy, yz, zz]: [f64; 6]) -> DMat3 {
        DMat3::from_cols_array(&[xx, xy, xz, xy, yy, yz, xz, yz, zz])
    }

    // upper triangle from a (symmetric) 3x3 matrix - glam is column major, so col j row i = axis_j[i]
    fn upper(m: DMat3) -> [f64; 6] {
        [m.x_axis.x, m.y_axis.x, m.z_axis.x, m.y_axis.y, m.z_axis.y, m.z_axis.z]
    }

    // approximate equality, tolerance relative to the size of the values involved
    fn approx(a: f64, b: f64, scale: f64) -> bool {
        (a - b).abs() <= 1e-9 * scale.max(1.0)
    }

    // Diagonal covariance matrix -eigenvalues are exactly its diagonals
    #[test]
    fn sym_eigenvalues_diagonal_matrix(){
        let cov = [3.0, 0.0, 0.0, 1.0, 0.0, 2.0];
        let ls = sym_eigenvalues(cov);

        assert_eq!(ls, [cov[0],cov[5],cov[3]]);
    }

    // Test against known off-diagonal matrix ([[2, 1, 0], [1, 2, 0], [0, 0, 3]]) eigenvalues
    #[test]
    fn sym_eigenvalues_known_off_diagonal(){
        let cov = [2.0, 1.0, 0.0, 2.0, 0.0, 3.0];
        let ls = sym_eigenvalues(cov).map(f64::round); // small rounding errors can be ignored

        assert_eq!([3.0, 3.0, 1.0], ls);
    }

    // For any matrix: sum of eigenvalues = trace, product of eigenvalues = determinant
    #[test]
    fn sym_eigenvalues_trace_and_determinant(){
        for m in gen_sym_matrices3(1_000, 42) {
            let [l1, l2, l3] = sym_eigenvalues(m);
            let a = to_mat(m);
            let trace = m[0] + m[3] + m[5];
            let det = a.determinant();
            // scale tolerances by the largest eigenvalue magnitude (det scales with its cube)
            let s = l1.abs().max(l3.abs());

            assert!(approx(l1 + l2 + l3, trace, s), "{m:?}: sum {} != trace {trace}", l1 + l2 + l3);
            assert!(approx(l1 * l2 * l3, det, s * s * s), "{m:?}: product {} != det {det}", l1 * l2 * l3);
        }
    }

    // Eigenvalues are always returned largest first
    #[test]
    fn sym_eigenvalues_sorted(){
        for m in gen_sym_matrices3(1_000, 7) {
            let [l1, l2, l3] = sym_eigenvalues(m);
            assert!(l1 >= l2 && l2 >= l3, "{m:?}: not sorted {:?}", [l1, l2, l3]);
        }
        // diagonal shortcut path must sort too
        assert_eq!(sym_eigenvalues([1.0, 0.0, 0.0, 3.0, 0.0, 2.0]), [3.0, 2.0, 1.0]);
    }

    // Each eigenvalue λ is a root of the characteristic polynomial: det(A - λI) = 0
    #[test]
    fn sym_eigenvalues_are_roots(){
        for m in gen_sym_matrices3(1_000, 3) {
            let a = to_mat(m);
            let ls = sym_eigenvalues(m);
            let s = ls[0].abs().max(ls[2].abs());
            for l in ls {
                let d = (a - DMat3::from_diagonal(DVec3::splat(l))).determinant();
                assert!(approx(d, 0.0, s * s * s * 1e3), "{m:?}: det(A - {l}I) = {d}");
            }
        }
    }

    // Build A = R D Rᵀ from known eigenvalues D and a random rotation R, then recover D
    #[test]
    fn sym_eigenvalues_recovers_rotated_diagonal(){
        let mut rng = rand::rngs::StdRng::seed_from_u64(11);
        for _ in 0..1_000 {
            // expected / known eigenvalues
            let mut expected: [f64; 3] = std::array::from_fn(|_| rng.random_range(0.0..10.0));
            expected.sort_by(|a, b| b.total_cmp(a));

            // random rotation axis and angle
            let axis = DVec3::new(rng.random_range(-1.0..1.0), rng.random_range(-1.0..1.0), rng.random_range(-1.0..1.0))
                .normalize_or(DVec3::Z);
            let rot = DMat3::from_quat(DQuat::from_axis_angle(axis, rng.random_range(0.0..std::f64::consts::TAU)));
            // rotate diagonal matrix with known eigenvalues, transpose the rotation
            let a = rot * DMat3::from_diagonal(DVec3::from_array(expected)) * rot.transpose();

            // calculated eigenvalues should be within tolerance of known values
            let got = sym_eigenvalues(upper(a));
            for i in 0..3 {
                // repeated/near-equal eigenvalues are the closed form's least accurate case, hence looser tolerance
                assert!((got[i] - expected[i]).abs() < 1e-6, "expected {expected:?}, got {got:?}");
            }
        }
    }

    // All equal eigenvalues (A = cI) - takes the diagonal path, avoids divide by zero p = 0
    #[test]
    fn sym_eigenvalues_identity_multiple(){
        assert_eq!(sym_eigenvalues([2.5, 0.0, 0.0, 2.5, 0.0, 2.5]), [2.5, 2.5, 2.5]);
        assert_eq!(sym_eigenvalues([0.0; 6]), [0.0, 0.0, 0.0]);
    }

    // Repeated eigenvalue in a non-diagonal matrix: [[1,1,1],[1,1,1],[1,1,1]] has eigenvalues 3, 0, 0
    #[test]
    fn sym_eigenvalues_repeated_non_diagonal(){
        let ls = sym_eigenvalues([1.0, 1.0, 1.0, 1.0, 1.0, 1.0]);
        for (got, want) in ls.iter().zip([3.0, 0.0, 0.0]) {
            assert!((got - want).abs() < 1e-9, "got {ls:?}");
        }
    }

    // Covariance matrices are positive semi-definite, so eigenvalues are >= 0 (up to rounding)
    #[test]
    fn sym_eigenvalues_covariance_non_negative(){
        let mut rng = rand::rngs::StdRng::seed_from_u64(5);
        for _ in 0..200 {
            // covariance of random points squashed onto a near-plane (z small) - smallest eigenvalue near 0
            let pts: Vec<DVec3> = (0..50)
                .map(|_| DVec3::new(rng.random_range(-5.0..5.0), rng.random_range(-5.0..5.0), rng.random_range(-0.01..0.01)))
                .collect();
            let mean = pts.iter().copied().sum::<DVec3>() / pts.len() as f64;
            let mut cov = [0.0; 6];
            for p in &pts {
                let d = *p - mean;
                let terms = [d.x * d.x, d.x * d.y, d.x * d.z, d.y * d.y, d.y * d.z, d.z * d.z];
                for i in 0..6 { cov[i] += terms[i] / pts.len() as f64; }
            }
            let [l1, _, l3] = sym_eigenvalues(cov);
            assert!(l3 > -1e-9 * l1, "smallest eigenvalue {l3} negative beyond rounding (l1 = {l1})");
            assert!(l3 < 1e-3, "near-planar points should have tiny l3, got {l3}");
        }
    }

    use super::eigenvector;

    // true if a and b point along the same axis (eigenvectors have no fixed sign, so compare |a·b|)
    fn parallel(a: DVec3, b: DVec3) -> bool {
        (a.normalize().dot(b.normalize()).abs() - 1.0).abs() < 1e-9
    }

    // Lines have a repeated eigenvalue 0 (λ2 = λ3), so its eigenvector is not unique -> None
    // checked with exact eigenvalue 0 and with the (slightly inexact) value from sym_eigenvalues
    #[test]
    fn eigenvector_none_when_not_unique(){
        let lines = [
            ([2.0, 0.0, 0.0, 0.0, 0.0, 0.0], "x"),
            ([0.0, 0.0, 0.0, 2.0, 0.0, 0.0], "y"),
            ([0.0, 0.0, 0.0, 0.0, 0.0, 2.0], "z"),
            ([2.0, 2.0, 0.0, 2.0, 0.0, 0.0], "xy diagonal"),
            ([0.0, 0.0, 0.0, 2.0, 2.0, 2.0], "yz diagonal"),
        ];
        for (cov, name) in lines {
            let l3 = sym_eigenvalues(cov)[2];
            assert_eq!(eigenvector(cov, 0.0), None, "line {name}, exact λ = 0");
            assert_eq!(eigenvector(cov, l3), None, "line {name}, computed λ3 = {l3:e}");
        }

        // all eigenvalues equal (isotropic) - no unique direction for any of them
        assert_eq!(eigenvector([2.0, 0.0, 0.0, 2.0, 0.0, 2.0], 2.0), None, "isotropic");
    }

    // Unique eigenvalues give the expected unit direction
    #[test]
    fn eigenvector_unique_cases(){
        // line along x: the non-repeated eigenvalue 2 has eigenvector ±x
        let v = eigenvector([2.0, 0.0, 0.0, 0.0, 0.0, 0.0], 2.0).expect("line, λ = 2");
        assert!(parallel(v, DVec3::X), "line x, λ = 2: got {v}");

        // plane in y-z (points spread in y and z): normal (λ = 0) is ±x
        let cov_plane_yz = [0.0, 0.0, 0.0, 2.0, 0.0, 2.0];
        let v = eigenvector(cov_plane_yz, sym_eigenvalues(cov_plane_yz)[2]).expect("plane yz normal");
        assert!(parallel(v, DVec3::X), "plane yz normal: got {v}");

        // diagonal line along (1,1,0): λ = 4 has eigenvector ±(1,1,0)/√2
        let v = eigenvector([2.0, 2.0, 0.0, 2.0, 0.0, 0.0], 4.0).expect("xy diagonal, λ = 4");
        assert!(parallel(v, DVec3::new(1.0, 1.0, 0.0)), "xy diagonal: got {v}");

        // results are unit length
        assert!((v.length() - 1.0).abs() < 1e-12, "not normalised: |v| = {}", v.length());
    }

    // For random symmetric matrices (distinct eigenvalues), every eigenvector satisfies A v = λ v
    #[test]
    fn eigenvector_satisfies_definition(){
        for m in gen_sym_matrices3(1_000, 17) {
            let a = to_mat(m);
            let ls = sym_eigenvalues(m);
            for l in ls {
                let v = eigenvector(m, l).expect("random matrices have distinct eigenvalues");
                // the zero vector trivially satisfies A v = λ v, so require unit length too
                assert!((v.length() - 1.0).abs() < 1e-9, "{m:?}, λ = {l}: |v| = {}", v.length());
                let err = (a * v - l * v).length();
                assert!(err < 1e-6 * ls[0].abs().max(ls[2].abs()).max(1.0), "{m:?}, λ = {l}: |Av - λv| = {err}");
            }
        }
    }
}
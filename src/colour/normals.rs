use glam::DVec3;

use crate::{math::normals::estimate_normals, render::vertex::GpuPoint};



/// Colour given to points with no reliable normal: mid grey, opaque (packed RGBA8, R in the low byte)
pub const NO_NORMAL_COLOUR: u32 = 0xFF80_8080;

/// Pack a normal into an RGBA8 colour (R in the low byte, matching `GpuPoint::color`). \
/// R, G, B = |x|, |y|, |z| scaled to 0..255, alpha = 255.
///
/// Uses absolute values because the sign of an estimated normal is arbitrary: n and -n get the same colour.
/// So a horizontal floor (normal ±z) is blue, a wall facing ±x is red, a wall facing ±y is green,
/// and slopes are mixes. `n` should be unit length.
pub fn normal_to_rgba(n: DVec3) -> u32 {
    let byte = |c: f64| (c.abs().clamp(0.0, 1.0) * 255.0).round() as u32;
    byte(n.x) | (byte(n.y) << 8) | (byte(n.z) << 16) | (0xFF << 24)
}

/// Estimate normals with cell size `r` (see `estimate_normals`), then recolour every point by its normal
/// (see `normal_to_rgba`). Points with no normal become `NO_NORMAL_COLOUR`. Positions are unchanged.
pub fn colour_by_normals(points: &mut [GpuPoint], r: f32) {
    let norms = estimate_normals(points, r);
    for (p, n) in points.iter_mut().zip(norms) {
        p.color = n.map_or(NO_NORMAL_COLOUR, normal_to_rgba);
    }
}

#[cfg(test)]
mod tests { 
    use glam::DVec3;
    use rand::{RngExt, SeedableRng};
    use crate::render::vertex::GpuPoint;
    use super::{normal_to_rgba, colour_by_normals, NO_NORMAL_COLOUR};

    // flat plane z = 0 covering [0, extent) x [0, extent)
    fn gen_plane(numpoints: usize, extent: f32, seed: u64) -> Vec<GpuPoint> {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        (0..numpoints)
            .map(|_| GpuPoint::new([rng.random_range(0.0..extent), rng.random_range(0.0..extent), 0.0], rng.random()))
            .collect()
    }

    // axis normals map to pure red / green / blue, and n and -n give the same colour
    #[test]
    fn normal_to_rgba_axes_and_sign(){
        assert_eq!(normal_to_rgba(DVec3::X), 0xFF00_00FF);
        assert_eq!(normal_to_rgba(DVec3::Y), 0xFF00_FF00);
        assert_eq!(normal_to_rgba(DVec3::Z), 0xFFFF_0000);
        let n = DVec3::new(0.3, -0.5, 0.81).normalize();
        assert_eq!(normal_to_rgba(n), normal_to_rgba(-n));
    }

    // a flat plane z = 0 is coloured blue everywhere; positions are unchanged
    #[test]
    fn colour_by_normals_plane_is_blue(){
        let mut pnts = gen_plane(20_000, 10.0, 68);
        let before: Vec<[f32; 3]> = pnts.iter().map(|p| p.pos).collect();
        colour_by_normals(&mut pnts, 1.0);
        assert!(pnts.iter().all(|p| p.color == 0xFFFF_0000));
        assert!(pnts.iter().map(|p| p.pos).eq(before));
    }

    // invalid r gives no normals, so every point becomes the no-normal grey
    #[test]
    fn colour_by_normals_invalid_r_is_grey(){
        let mut pnts = gen_plane(1_000, 10.0, 69);
        colour_by_normals(&mut pnts, 0.0);
        assert!(pnts.iter().all(|p| p.color == NO_NORMAL_COLOUR));
    }
}
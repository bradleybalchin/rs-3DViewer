use bytemuck::{Pod, Zeroable};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use std::{path::Path};
use las::{Reader};

// single point (position + colour)
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct GpuPoint {
    pub pos: [f32; 3], // relative to `origin`
    pub color: u32,    // packed RGBA8
}

// points and local origin loaded from file
pub struct LoadMessage {
    pub points: Vec<GpuPoint>,
    pub origin: [f64; 3], // add back to get real-world coordinatesa
}

// Load the laz/las file
pub fn load(path: &Path) -> Result<LoadMessage, las::Error> {
    let mut reader = Reader::from_path(path)?;

    // bounding box, center of box is local origin
    let b = reader.header().bounds();
    let origin = [
        (b.min.x + b.max.x) * 0.5,
        (b.min.y + b.max.y) * 0.5,
        (b.min.z + b.max.z) * 0.5,
    ];

    // find total points and create correct sized arrays for points
    let total_points = reader.header().number_of_points() as usize;
    let mut las_points: Vec<las::Point> = Vec::with_capacity(total_points);
    reader.read_all_points_into(&mut las_points)?;

    // parraellel iteration with rayon (par_iter)
    let points : Vec<GpuPoint> = las_points.par_iter()
        .map(|p| {
            GpuPoint { pos: [p.x as f32,p.y as f32,p.z as f32], color: 0 }
        }).collect();

    
    // Return the point and origin
    Ok(LoadMessage { points, origin })
}

// pack u16 colours into u32 (8 bit colour, 1 byte per colour channel)
fn pack(r: u16, g: u16, b: u16) -> u32 {
    // first widen each to u32 (size of output)
    // mask with 0xFF - keeping only the first 8 bits, rest are set to zero
    // shift 0, 8, 16, 24 bits to correct location within the u32 so R G B A can be stored (A stored as constant 255)
    // combined with or operations to produce final u32 with 8bits ber channel
    (r as u32 & 0xFF) | ((g as u32 & 0xFF) << 8) | ((b as u32 & 0xFF) << 16) | (0xFF << 24)
}

#[cfg(test)]
mod tests {
    use super::pack;

    /// Should Correctly pack u8 bits into u32 (8 bits per colour and 0xFF alpha)
    #[test]
    fn pack_formats_rgb_into_rgba_u32() {
        assert_eq!(pack(0x01, 0x02, 0x03), 0xFF030201);
        assert_eq!(pack(0xAB, 0xCD, 0xEF), 0xFFEFCDAB);
    }

    /// Only keep the lowest 8 bits of each input, should not bleed into neighbors
    #[test]
    fn pack_masks_excess_bits() {
        assert_eq!(pack(0x0100, 0x0200, 0x0300), 0xFF000000);
        assert_eq!(pack(0xFF, 0x00FF, 0x000F), 0xFF0FFFFF);
        assert_eq!(pack(0x12AB, 0x34CD, 0x56EF), 0xFFEFCDAB);
    }
}
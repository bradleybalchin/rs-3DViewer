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

// pack u16 colours into u32 (8 bit colour)
fn pack(r: u16, g: u16, b: u16) -> u32 {
    todo!()
}
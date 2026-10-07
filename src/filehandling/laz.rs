use bytemuck::{Pod, Zeroable};
use std::path::Path;

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
    todo!()
}

// pack u16 colours into u32 (8 bit colour)
fn pack(r: u16, g: u16, b: u16) -> u32 {
    todo!()
}
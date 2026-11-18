use std::path::Path;
use std::time::Instant;
use las::{Reader};
use rayon::{iter::ParallelIterator, slice::ParallelSlice};

use crate::render::vertex::GpuPoint;
use crate::colour::true_colour;

// points and local origin loaded from file
pub struct LoadMessage {
    pub points: Vec<GpuPoint>,
    pub origin: [f64; 3], // add back to get real-world coordinatesa
}

// Load the laz/las file
pub fn load(path: &Path) -> Result<LoadMessage, las::Error> {
    let start = Instant::now();
    let mut reader = Reader::from_path(path)?;
    let open_time = start.elapsed();

    // bounding box, center of box is local origin
    let b = reader.header().bounds();
    let origin = [
        (b.min.x + b.max.x) * 0.5,
        (b.min.y + b.max.y) * 0.5,
        (b.min.z + b.max.z) * 0.5,
    ];

    // read every point into columnar point data (one column per field)
    // with the `laz-parallel` feature this decompresses the LAZ chunks across all cores (via rayon)
    let read_start = Instant::now();
    let data = reader.read_all()?;
    let read_time = read_start.elapsed();
    let convert_start = Instant::now();

    // coord transforms for recrods
    let t = *data.transforms();
    // point format for data
    let fmt = data.format();
    // rgb offset for the given data format
    let rgb_offset = fmt.has_color.then(|| {
        if fmt.is_extended { 30 } else if fmt.has_gps_time { 28 } else { 20 }
    });
    // Parrallel reading of records using rayon
    let points: Vec<GpuPoint> = data.raw_bytes().par_chunks_exact(data.record_len())
        .map(|rec| {
            // read integers from bytes (i32 for position, u16 for color), little endian
            let i32_at = |o : usize | i32::from_le_bytes(rec[o..o + 4].try_into().unwrap()); // 4 bytes (32)
            let u16_at = |o : usize | u16::from_le_bytes([rec[o], rec[o + 1]]);              // 2 bytes (16)

            // positions relative to the local origin, as real-world coordinates lose precision in f32
            let pos = [
                (t.x.direct(i32_at(0)) - origin[0]) as f32,
                (t.y.direct(i32_at(4)) - origin[1]) as f32,
                (t.z.direct(i32_at(8)) - origin[2]) as f32,
            ];

            // pack colours to u32 if the point format has them, otherwise use white
            let color = match rgb_offset {
                Some(c) => true_colour::pack(u16_at(c), u16_at(c+2), u16_at(c+4)),
                None => 0xFFFFFFFF,
            };

            GpuPoint { pos, color}
        }).collect();

    let convert_time = convert_start.elapsed();
    let ms = |d: std::time::Duration| d.as_secs_f64() * 1000.0;
    eprintln!(
        "[load {} points] open {:.1} ms | read/decompress {:.1} ms | convert {:.1} ms | total {:.1} ms",
        points.len(),
        ms(open_time),
        ms(read_time),
        ms(convert_time),
        ms(start.elapsed()),
    );

    // Return the point and origin
    Ok(LoadMessage { points, origin })
}


use std::path::Path;
use std::time::Instant;
use las::{Reader};
use rayon::{iter::ParallelIterator, slice::ParallelSlice};

use crate::render::vertex::GpuPoint;

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
                Some(c) => pack(u16_at(c), u16_at(c+2), u16_at(c+4)),
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

// pack u16 colours into u32 (8 bit colour, 1 byte per colour channel)
fn pack(r: u16, g: u16, b: u16) -> u32 {
    // LAS colours use the full 16 bit range, so shift right by 8 to keep the most significant byte of each
    // shift 0, 8, 16, 24 bits to correct location within the u32 so R G B A can be stored (A stored as constant 255)
    // combined with or operations to produce final u32 with 8bits ber channel
    (r as u32 >> 8) | ((g as u32 >> 8) << 8) | ((b as u32 >> 8) << 16) | (0xFF << 24)
}

#[cfg(test)]
mod tests {
    use super::pack;

    /// Should correctly pack the high byte of each u16 into a u32 (8 bits per colour and 0xFF alpha)
    #[test]
    fn pack_formats_rgb_into_rgba_u32() {
        assert_eq!(pack(0x0100, 0x0200, 0x0300), 0xFF030201);
        assert_eq!(pack(0xAB00, 0xCD00, 0xEF00), 0xFFEFCDAB);
    }

    /// Only keep the highest 8 bits of each input, the low bytes should not bleed into neighbors
    #[test]
    fn pack_discards_low_bytes() {
        assert_eq!(pack(0x00FF, 0x00FF, 0x00FF), 0xFF000000);
        assert_eq!(pack(0xFFFF, 0xFFFF, 0xFFFF), 0xFFFFFFFF);
        assert_eq!(pack(0xAB12, 0xCD34, 0xEF56), 0xFFEFCDAB);
    }
}
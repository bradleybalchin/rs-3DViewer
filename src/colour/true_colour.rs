
// pack u16 colours into u32 (8 bit colour, 1 byte per colour channel)
pub fn pack(r: u16, g: u16, b: u16) -> u32 {
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
//! Turns a decoded (and already downscaled) frame into JPEG bytes.

use crate::scale::Frame;
use jpeg_encoder::{ColorType, Encoder};

pub fn to_jpeg(frame: &Frame, quality: u8) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let encoder = Encoder::new(&mut out, quality);
    encoder
        .encode(
            &frame.rgb,
            frame.width as u16,
            frame.height as u16,
            ColorType::Rgb,
        )
        .map_err(|e| e.to_string())?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_a_solid_frame() {
        let frame = Frame {
            width: 8,
            height: 8,
            rgb: vec![128u8; 8 * 8 * 3],
        };
        let jpeg = to_jpeg(&frame, 75).unwrap();
        // JPEG magic bytes.
        assert_eq!(&jpeg[0..2], &[0xFF, 0xD8]);
    }
}

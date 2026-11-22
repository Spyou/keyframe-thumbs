//! Shrinks a decoded RGB24 frame down to thumbnail size before JPEG encoding.

pub struct Frame {
    pub width: u32,
    pub height: u32,
    /// Tightly packed RGB24 (3 bytes per pixel, no row padding).
    pub rgb: Vec<u8>,
}

/// Nearest-neighbour downscale to `target_width`, keeping the source aspect
/// ratio. Good enough for a thumbnail — no need for a proper filter at this
/// output size.
pub fn downscale(frame: &Frame, target_width: u32) -> Frame {
    if frame.width <= target_width {
        return Frame {
            width: frame.width,
            height: frame.height,
            rgb: frame.rgb.clone(),
        };
    }
    let target_height = ((frame.height as u64 * target_width as u64) / frame.width as u64) as u32;
    let mut out = vec![0u8; (target_width * target_height * 3) as usize];
    for y in 0..target_height {
        let src_y = (y as u64 * frame.height as u64 / target_height as u64) as u32;
        for x in 0..target_width {
            let src_x = (x as u64 * frame.width as u64 / target_width as u64) as u32;
            let src_i = ((src_y * frame.width + src_x) * 3) as usize;
            let dst_i = ((y * target_width + x) * 3) as usize;
            out[dst_i..dst_i + 3].copy_from_slice(&frame.rgb[src_i..src_i + 3]);
        }
    }
    Frame {
        width: target_width,
        height: target_height,
        rgb: out,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_aspect_ratio() {
        let frame = Frame {
            width: 1920,
            height: 1080,
            rgb: vec![0u8; 1920 * 1080 * 3],
        };
        let small = downscale(&frame, 320);
        assert_eq!(small.width, 320);
        assert_eq!(small.height, 180);
        assert_eq!(small.rgb.len(), 320 * 180 * 3);
    }

    #[test]
    fn leaves_already_small_frames_alone() {
        let frame = Frame {
            width: 100,
            height: 50,
            rgb: vec![1u8; 100 * 50 * 3],
        };
        let out = downscale(&frame, 320);
        assert_eq!(out.width, 100);
        assert_eq!(out.height, 50);
    }
}

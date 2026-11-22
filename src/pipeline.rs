//! Wires the modules together: find the keyframe nearest `time_ms`, decode
//! it, shrink it, encode it. Any failure anywhere in that chain just means
//! no thumbnail — never a crash, never a stall for the caller.

use crate::{codec, encode, scale, source};

const THUMBNAIL_WIDTH: u32 = 320;
const JPEG_QUALITY: u8 = 75;

pub fn thumbnail_for(url: &str, headers: &[(String, String)], time_ms: u64) -> Option<Vec<u8>> {
    let keyframes = source::open(url, headers).ok()?;
    let nearest = keyframes
        .iter()
        .filter(|k| k.time_ms <= time_ms)
        .last()
        .or_else(|| keyframes.first())?;
    // `source::open` doesn't fetch packet bytes yet — that lands with the
    // FFmpeg backend, same milestone as `codec::decode_keyframe`.
    let packet: &[u8] = &[];
    let _ = nearest;
    let frame = codec::decode_keyframe(packet, "").ok()?;
    let small = scale::downscale(&frame, THUMBNAIL_WIDTH);
    encode::to_jpeg(&small, JPEG_QUALITY).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_panics_when_the_source_cant_open() {
        // source::open always errs until FFmpeg is wired in — this just
        // locks in that the whole chain degrades to `None`, not a panic.
        assert_eq!(thumbnail_for("http://example.invalid/x.mp4", &[], 5_000), None);
    }
}

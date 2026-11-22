//! Wires the modules together: find the keyframe nearest `time_ms`, decode
//! it, shrink it, encode it. Any failure anywhere in that chain just means
//! no thumbnail — never a crash, never a stall for the caller.

use crate::{codec, encode, scale, source};

const THUMBNAIL_WIDTH: u32 = 320;
const JPEG_QUALITY: u8 = 75;

/// `path` is a local file for now — see [`source`]'s doc comment for why
/// remote URLs aren't wired up yet. `_headers` is accepted already so the
/// JNI-facing signature doesn't have to change when they are.
pub fn thumbnail_for(path: &str, _headers: &[(String, String)], time_ms: u64) -> Option<Vec<u8>> {
    let encoded = source::open_and_seek(path, time_ms).ok()?;
    let frame = codec::decode_keyframe(&encoded).ok()?;
    let small = scale::downscale(&frame, THUMBNAIL_WIDTH);
    encode::to_jpeg(&small, JPEG_QUALITY).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_panics_when_the_source_cant_open() {
        // On non-Android hosts (this test's target) FFmpeg isn't linked at
        // all, so source::open_and_seek always errs — this locks in that the
        // whole chain degrades to `None`, not a panic, regardless of why.
        assert_eq!(thumbnail_for("/nonexistent.mp4", &[], 5_000), None);
    }
}

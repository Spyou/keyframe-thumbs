//! Decodes a single keyframe's raw bytes into an RGB24 frame.
//!
//! Not implemented yet. This is where FFmpeg's `avcodec` does the real
//! decode, plus the manual HEVC/Dolby Vision bitstream fixups a lone
//! keyframe needs to decode correctly outside of a real stream. Blocked on
//! vendoring FFmpeg for Android (next milestone).

use crate::scale::Frame;

pub fn decode_keyframe(_packet: &[u8], _codec: &str) -> Result<Frame, String> {
    Err("ffmpeg backend not linked yet".into())
}

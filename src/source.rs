//! Opens a remote file over HTTP range requests and reads its keyframe
//! index (timestamp + byte offset per keyframe), without downloading
//! anything else.
//!
//! Not implemented yet. This needs FFmpeg's `avformat` linked in for
//! Android, which is the next milestone after this scaffold — every call
//! here is a placeholder so the rest of the crate can be built and tested
//! against it first.

pub struct Keyframe {
    pub time_ms: u64,
    pub byte_offset: u64,
}

pub fn open(_url: &str, _headers: &[(String, String)]) -> Result<Vec<Keyframe>, String> {
    Err("ffmpeg backend not linked yet".into())
}

//! Wires the modules together: resolve an HLS URL down to the one segment
//! that covers `time_ms` (plain files pass through unchanged), check the
//! cache, and if it's a miss, find the keyframe, decode it, shrink it,
//! encode it. Any failure anywhere in that chain just means no thumbnail —
//! never a crash, never a stall for the caller.

use crate::{codec, encode, hls, net, scale, session, source};

const THUMBNAIL_WIDTH: u32 = 320;
const JPEG_QUALITY: u8 = 75;

pub fn thumbnail_for(url: &str, headers: &[(String, String)], time_ms: u64) -> Option<Vec<u8>> {
    session::cached_or(url, time_ms, || {
        let (target_url, local_time_ms) = resolve_target(url, headers, time_ms)?;
        let encoded = source::open_and_seek(&target_url, local_time_ms, headers).ok()?;
        let frame = codec::decode_keyframe(&encoded).ok()?;
        let small = scale::downscale(&frame, THUMBNAIL_WIDTH);
        encode::to_jpeg(&small, JPEG_QUALITY).ok()
    })
}

/// For an HLS URL: fetches the playlist, picks the segment covering
/// `time_ms`, and returns that segment's URL with `time_ms` rebased to its
/// own start (each segment is its own standalone container with a
/// zero-based timeline). Anything else passes through unchanged.
fn resolve_target(
    url: &str,
    headers: &[(String, String)],
    time_ms: u64,
) -> Option<(String, u64)> {
    if !url.contains(".m3u8") {
        return Some((url.to_string(), time_ms));
    }
    let text = net::fetch_text(url, headers).ok()?;
    let segments = hls::parse_playlist(url, &text);
    let idx = hls::segment_for_time(&segments, time_ms)?;
    let seg = &segments[idx];
    Some((seg.url.clone(), time_ms.saturating_sub(seg.start_ms)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_panics_when_the_source_cant_open() {
        // On non-Android hosts (this test's target) FFmpeg isn't linked at
        // all, so source::open_and_seek always errs — this locks in that the
        // whole chain degrades to `None`, not a panic, regardless of why.
        assert_eq!(
            thumbnail_for("/nonexistent-pipeline-test.mp4", &[], 5_000),
            None
        );
    }

    #[test]
    fn non_hls_urls_pass_through_unchanged() {
        assert_eq!(
            resolve_target("/local/file.mp4", &[], 5_000),
            Some(("/local/file.mp4".to_string(), 5_000))
        );
    }
}

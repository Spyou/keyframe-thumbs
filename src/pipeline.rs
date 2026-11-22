//! Wires the modules together: resolve an HLS URL down to the one segment
//! that covers `time_ms` (plain files pass through unchanged), check the
//! cache, and if it's a miss, find the keyframe, decode it, shrink it,
//! encode it. Any failure anywhere in that chain just means no thumbnail —
//! never a crash, never a stall for the caller.

use crate::{codec, encode, hls, net, scale, session, source};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

const THUMBNAIL_WIDTH: u32 = 320;
const JPEG_QUALITY: u8 = 75;

/// Parsed playlists, keyed by the m3u8 URL. A VOD playlist's segment list
/// never changes, so fetching and parsing it once per video (not once per
/// scrub tick) is the single biggest latency win available here — the
/// playlist text itself can be hundreds of KB.
static PLAYLIST_CACHE: OnceLock<Mutex<HashMap<String, Vec<hls::Segment>>>> = OnceLock::new();

fn cached_playlist(
    url: &str,
    headers: &[(String, String)],
) -> Result<Vec<hls::Segment>, String> {
    let cache = PLAYLIST_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(hit) = cache.lock().unwrap().get(url) {
        return Ok(hit.clone());
    }
    let text = net::fetch_text(url, headers)?;
    let segments = hls::parse_playlist(url, &text);
    cache
        .lock()
        .unwrap()
        .insert(url.to_string(), segments.clone());
    Ok(segments)
}

pub fn thumbnail_for(url: &str, headers: &[(String, String)], time_ms: u64) -> Option<Vec<u8>> {
    session::cached_or(url, time_ms, || {
        let (target_url, local_time_ms) = resolve_target(url, headers, time_ms)?;
        let encoded = source::open_and_seek(&target_url, local_time_ms, headers).ok()?;
        let frame = codec::decode_keyframe(&encoded).ok()?;
        let small = scale::downscale(&frame, THUMBNAIL_WIDTH);
        encode::to_jpeg(&small, JPEG_QUALITY).ok()
    })
}

/// Same pipeline as [`thumbnail_for`] but keeps the error string from
/// whichever stage failed, instead of collapsing everything to `None`.
/// Temporary diagnostic entry point — not part of the stable JNI surface.
pub fn thumbnail_for_debug(
    url: &str,
    headers: &[(String, String)],
    time_ms: u64,
) -> Result<Vec<u8>, String> {
    let (target_url, local_time_ms) = if url.contains(".m3u8") {
        let segments = cached_playlist(url, headers).map_err(|e| format!("fetch_text: {e}"))?;
        let idx = hls::segment_for_time(&segments, time_ms).ok_or_else(|| {
            format!(
                "segment_for_time: {} segments parsed, none cover {time_ms}ms",
                segments.len()
            )
        })?;
        let seg = &segments[idx];
        (seg.url.clone(), time_ms.saturating_sub(seg.start_ms))
    } else {
        (url.to_string(), time_ms)
    };
    let encoded = source::open_and_seek(&target_url, local_time_ms, headers)
        .map_err(|e| format!("open_and_seek: {e}"))?;
    let frame = codec::decode_keyframe(&encoded).map_err(|e| format!("decode_keyframe: {e}"))?;
    let small = scale::downscale(&frame, THUMBNAIL_WIDTH);
    encode::to_jpeg(&small, JPEG_QUALITY).map_err(|e| format!("to_jpeg: {e}"))
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
    let segments = cached_playlist(url, headers).ok()?;
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

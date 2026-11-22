//! Maps a scrub timestamp to the HLS segment that covers it, so a segment
//! can be treated as a standalone file by the rest of the pipeline.

#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub url: String,
    pub start_ms: u64,
    pub duration_ms: u64,
}

/// Index of the segment covering `time_ms`, or the last segment if `time_ms`
/// runs past the end of the list.
pub fn segment_for_time(segments: &[Segment], time_ms: u64) -> Option<usize> {
    if segments.is_empty() {
        return None;
    }
    let idx = segments
        .iter()
        .position(|s| time_ms < s.start_ms + s.duration_ms);
    Some(idx.unwrap_or(segments.len() - 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start_ms: u64, duration_ms: u64) -> Segment {
        Segment {
            url: format!("seg-{start_ms}.ts"),
            start_ms,
            duration_ms,
        }
    }

    #[test]
    fn empty_list_has_no_segment() {
        assert_eq!(segment_for_time(&[], 0), None);
    }

    #[test]
    fn picks_the_segment_covering_the_time() {
        let segments = vec![seg(0, 1000), seg(1000, 1000), seg(2000, 1000)];
        assert_eq!(segment_for_time(&segments, 0), Some(0));
        assert_eq!(segment_for_time(&segments, 999), Some(0));
        assert_eq!(segment_for_time(&segments, 1000), Some(1));
        assert_eq!(segment_for_time(&segments, 2500), Some(2));
    }

    #[test]
    fn clamps_to_the_last_segment_past_the_end() {
        let segments = vec![seg(0, 1000), seg(1000, 1000)];
        assert_eq!(segment_for_time(&segments, 50_000), Some(1));
    }
}

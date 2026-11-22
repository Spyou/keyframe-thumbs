//! Maps a scrub timestamp to the HLS segment that covers it, so a segment
//! can be treated as a standalone file by the rest of the pipeline.

#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub url: String,
    pub start_ms: u64,
    pub duration_ms: u64,
}

/// Parses an m3u8 playlist's `#EXTINF`/URI pairs into a segment list, with
/// each segment's `start_ms` running total computed as we go. Relative URIs
/// are resolved against `base_url`.
pub fn parse_playlist(base_url: &str, text: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut pending_duration_ms: Option<u64> = None;
    let mut start_ms = 0u64;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("#EXTINF:") {
            let secs: f64 = rest
                .split(',')
                .next()
                .unwrap_or("0")
                .trim()
                .parse()
                .unwrap_or(0.0);
            pending_duration_ms = Some((secs * 1000.0).round() as u64);
        } else if !line.starts_with('#') {
            let duration_ms = pending_duration_ms.take().unwrap_or(0);
            segments.push(Segment {
                url: resolve(base_url, line),
                start_ms,
                duration_ms,
            });
            start_ms += duration_ms;
        }
    }
    segments
}

fn resolve(base_url: &str, uri: &str) -> String {
    if uri.starts_with("http://") || uri.starts_with("https://") {
        uri.to_string()
    } else {
        match base_url.rfind('/') {
            Some(idx) => format!("{}/{}", &base_url[..idx], uri),
            None => uri.to_string(),
        }
    }
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

    #[test]
    fn parses_extinf_and_resolves_relative_uris() {
        let playlist = "#EXTM3U\n#EXTINF:4.004,\nseg0.ts\n#EXTINF:4.004,\nseg1.ts\n#EXT-X-ENDLIST\n";
        let segments = parse_playlist("https://cdn.example.com/show/ep1/master.m3u8", playlist);
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].url, "https://cdn.example.com/show/ep1/seg0.ts");
        assert_eq!(segments[0].start_ms, 0);
        assert_eq!(segments[0].duration_ms, 4004);
        assert_eq!(segments[1].url, "https://cdn.example.com/show/ep1/seg1.ts");
        assert_eq!(segments[1].start_ms, 4004);
    }

    #[test]
    fn keeps_absolute_uris_as_is() {
        let playlist = "#EXTINF:2,\nhttps://other-cdn.example.com/seg0.ts\n";
        let segments = parse_playlist("https://cdn.example.com/master.m3u8", playlist);
        assert_eq!(segments[0].url, "https://other-cdn.example.com/seg0.ts");
    }
}

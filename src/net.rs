//! A plain HTTP(S) client doing range-requests. FFmpeg never sees the
//! network: it only ever sees bytes this hands it through a custom
//! AVIOContext, so no TLS library needs to be vendored into FFmpeg itself
//! (rustls, pulled in via `ureq`, handles that here).

use std::io::Read;
use std::sync::OnceLock;
use std::time::Duration;

const CHUNK: u64 = 256 * 1024;
const AVSEEK_SIZE: i32 = 0x10000; // AVSEEK_SIZE (avio.h) — asks for size, not a real seek.

fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(2))
            .timeout(Duration::from_secs(4))
            .build()
    })
}

/// Sequential-read view over a remote file, backed by HTTP Range requests.
/// "Seeking" just repositions an internal cursor — no I/O happens until the
/// next [`read`](RangeReader::read).
pub struct RangeReader {
    url: String,
    headers: Vec<(String, String)>,
    size: u64,
    pos: u64,
    buf: Vec<u8>,
    buf_start: u64,
}

impl RangeReader {
    pub fn open(url: &str, headers: &[(String, String)]) -> Result<Self, String> {
        let mut r = RangeReader {
            url: url.to_string(),
            headers: headers.to_vec(),
            size: 0,
            pos: 0,
            buf: Vec::new(),
            buf_start: 0,
        };
        r.fetch(0, CHUNK)?;
        Ok(r)
    }

    fn fetch(&mut self, start: u64, len: u64) -> Result<(), String> {
        let end = start + len.max(1) - 1;
        let mut req = agent()
            .get(&self.url)
            .set("Range", &format!("bytes={start}-{end}"));
        for (k, v) in &self.headers {
            req = req.set(k, v);
        }
        let resp = req.call().map_err(|e| e.to_string())?;
        if let Some(cr) = resp.header("Content-Range") {
            if let Some(total) = cr.rsplit('/').next() {
                if let Ok(t) = total.parse() {
                    self.size = t;
                }
            }
        } else if self.size == 0 {
            if let Some(len_hdr) = resp.header("Content-Length") {
                self.size = len_hdr.parse().unwrap_or(0);
            }
        }
        let mut body = Vec::new();
        resp.into_reader()
            .take(len + 1)
            .read_to_end(&mut body)
            .map_err(|e| e.to_string())?;
        self.buf_start = start;
        self.buf = body;
        Ok(())
    }

    /// Reads forward from the current cursor, refilling its buffer with one
    /// more range-request when the cursor has moved past it.
    pub fn read(&mut self, out: &mut [u8]) -> Result<usize, String> {
        if self.size > 0 && self.pos >= self.size {
            return Ok(0);
        }
        let have = self.pos >= self.buf_start && self.pos < self.buf_start + self.buf.len() as u64;
        if !have {
            let want = (out.len() as u64).max(CHUNK);
            self.fetch(self.pos, want)?;
        }
        let offset = (self.pos - self.buf_start) as usize;
        if offset >= self.buf.len() {
            return Ok(0);
        }
        let n = (self.buf.len() - offset).min(out.len());
        out[..n].copy_from_slice(&self.buf[offset..offset + n]);
        self.pos += n as u64;
        Ok(n)
    }

    pub fn seek(&mut self, offset: i64, whence: i32) -> i64 {
        let result = compute_seek(self.pos, self.size, offset, whence);
        if result >= 0 && whence != AVSEEK_SIZE {
            self.pos = result as u64;
        }
        result
    }
}

/// Pulled out of [`RangeReader::seek`] so the seek-math (SEEK_SET/CUR/END +
/// AVSEEK_SIZE) is testable without a network call.
fn compute_seek(pos: u64, size: u64, offset: i64, whence: i32) -> i64 {
    if whence == AVSEEK_SIZE {
        return size as i64;
    }
    let new_pos = match whence {
        0 => offset,                  // SEEK_SET
        1 => pos as i64 + offset,     // SEEK_CUR
        2 => size as i64 + offset,    // SEEK_END
        _ => return -1,
    };
    if new_pos < 0 {
        -1
    } else {
        new_pos
    }
}

/// Plain (non-range) GET, for small text resources like an m3u8 playlist.
pub fn fetch_text(url: &str, headers: &[(String, String)]) -> Result<String, String> {
    let mut req = agent().get(url);
    for (k, v) in headers {
        req = req.set(k, v);
    }
    req.call()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seek_set_moves_to_absolute_offset() {
        assert_eq!(compute_seek(100, 1000, 50, 0), 50);
    }

    #[test]
    fn seek_cur_moves_relative_to_position() {
        assert_eq!(compute_seek(100, 1000, 25, 1), 125);
        assert_eq!(compute_seek(100, 1000, -25, 1), 75);
    }

    #[test]
    fn seek_end_moves_relative_to_size() {
        assert_eq!(compute_seek(100, 1000, -10, 2), 990);
    }

    #[test]
    fn avseek_size_reports_size_without_moving() {
        assert_eq!(compute_seek(100, 1000, 0, AVSEEK_SIZE), 1000);
    }

    #[test]
    fn negative_result_is_rejected() {
        assert_eq!(compute_seek(10, 1000, -50, 0), -1);
    }
}

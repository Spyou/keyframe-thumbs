//! Opens a file (local path or remote HTTP(S) URL), seeks to the keyframe
//! nearest a timestamp, and hands back that one packet's raw bytes plus
//! enough codec info to decode it.
//!
//! Remote URLs are read through [`crate::net::RangeReader`] bridged into
//! FFmpeg via a custom `AVIOContext` — FFmpeg never does its own networking
//! or TLS here, it only ever sees bytes we hand it.

pub struct EncodedFrame {
    pub codec_id: i32,
    pub extradata: Vec<u8>,
    pub width: i32,
    pub height: i32,
    pub packet: Vec<u8>,
}

#[cfg(not(target_os = "android"))]
pub fn open_and_seek(
    _url: &str,
    _time_ms: u64,
    _headers: &[(String, String)],
) -> Result<EncodedFrame, String> {
    Err("FFmpeg is only linked for Android builds".into())
}

#[cfg(target_os = "android")]
pub fn open_and_seek(
    url: &str,
    time_ms: u64,
    headers: &[(String, String)],
) -> Result<EncodedFrame, String> {
    if url.starts_with("http://") || url.starts_with("https://") {
        remote::open_and_seek(url, time_ms, headers)
    } else {
        local::open_and_seek(url, time_ms)
    }
}

/// Shared by both backends once they have an open `AVFormatContext`: find
/// the video stream, seek near `time_ms`, and read forward to the first
/// packet that actually belongs to it.
#[cfg(target_os = "android")]
fn read_nearest_keyframe(
    fmt_ctx: *mut ffi::AVFormatContext,
    time_ms: u64,
) -> Result<EncodedFrame, String> {
    use ffi::*;
    use std::ptr;

    struct PacketGuard(*mut AVPacket);
    impl Drop for PacketGuard {
        fn drop(&mut self) {
            unsafe { av_packet_free(&mut self.0) };
        }
    }

    unsafe {
        let stream_idx = av_find_best_stream(
            fmt_ctx,
            AVMediaType_AVMEDIA_TYPE_VIDEO,
            -1,
            -1,
            ptr::null_mut(),
            0,
        );
        if stream_idx < 0 {
            return Err("no video stream".into());
        }
        let stream = *(*fmt_ctx).streams.add(stream_idx as usize);
        let codecpar = (*stream).codecpar;

        let tb = (*stream).time_base;
        let target_ts = (time_ms as i64 * tb.den as i64) / (1000 * tb.num.max(1) as i64);

        // AVSEEK_FLAG_BACKWARD (avformat.h) — a #define, not a real symbol,
        // so bindgen doesn't expose it; its value is stable across FFmpeg
        // versions.
        const AVSEEK_FLAG_BACKWARD: i32 = 1;
        if av_seek_frame(fmt_ctx, stream_idx, target_ts, AVSEEK_FLAG_BACKWARD) < 0 {
            return Err("av_seek_frame failed".into());
        }

        let pkt = av_packet_alloc();
        if pkt.is_null() {
            return Err("av_packet_alloc failed".into());
        }
        let _pkt_guard = PacketGuard(pkt);

        // Read until we land a packet from our video stream — the demuxer
        // may hand back other streams' packets (audio) interleaved with it.
        loop {
            let rc = av_read_frame(fmt_ctx, pkt);
            if rc < 0 {
                return Err(format!("av_read_frame failed: {rc}"));
            }
            if (*pkt).stream_index == stream_idx {
                break;
            }
            av_packet_unref(pkt);
        }

        let extradata = if !(*codecpar).extradata.is_null() && (*codecpar).extradata_size > 0 {
            std::slice::from_raw_parts(
                (*codecpar).extradata,
                (*codecpar).extradata_size as usize,
            )
            .to_vec()
        } else {
            Vec::new()
        };
        let packet_bytes = std::slice::from_raw_parts((*pkt).data, (*pkt).size as usize).to_vec();

        Ok(EncodedFrame {
            codec_id: (*codecpar).codec_id as i32,
            extradata,
            width: (*codecpar).width,
            height: (*codecpar).height,
            packet: packet_bytes,
        })
    }
}

#[cfg(target_os = "android")]
use crate::ffi;

#[cfg(target_os = "android")]
mod local {
    use super::{ffi, read_nearest_keyframe, EncodedFrame};
    use std::ffi::CString;
    use std::ptr;

    struct FormatGuard(*mut ffi::AVFormatContext);
    impl Drop for FormatGuard {
        fn drop(&mut self) {
            unsafe { ffi::avformat_close_input(&mut self.0) };
        }
    }

    pub fn open_and_seek(path: &str, time_ms: u64) -> Result<EncodedFrame, String> {
        unsafe {
            let c_path = CString::new(path).map_err(|e| e.to_string())?;
            let mut fmt_ctx: *mut ffi::AVFormatContext = ptr::null_mut();
            let rc = ffi::avformat_open_input(
                &mut fmt_ctx,
                c_path.as_ptr(),
                ptr::null_mut(),
                ptr::null_mut(),
            );
            if rc < 0 || fmt_ctx.is_null() {
                return Err(format!("avformat_open_input failed: {rc}"));
            }
            let _guard = FormatGuard(fmt_ctx);
            if ffi::avformat_find_stream_info(fmt_ctx, ptr::null_mut()) < 0 {
                return Err("avformat_find_stream_info failed".into());
            }
            read_nearest_keyframe(fmt_ctx, time_ms)
        }
    }
}

#[cfg(target_os = "android")]
pub fn release() {
    remote::release();
}

#[cfg(target_os = "android")]
mod remote {
    use super::{ffi, read_nearest_keyframe, EncodedFrame};
    use crate::net::RangeReader;
    use std::ffi::CString;
    use std::os::raw::{c_int, c_void};
    use std::ptr;

    const AVIO_BUFFER_SIZE: usize = 32 * 1024;
    const AVFMT_FLAG_CUSTOM_IO: i32 = 0x0080; // avformat.h — not exposed as a real symbol either.
    const AVERROR_EOF: c_int = -541478725; // FFERRTAG('E','O','F',' '), stable across versions.

    struct AvioGuard(*mut ffi::AVIOContext);
    impl Drop for AvioGuard {
        fn drop(&mut self) {
            unsafe {
                if !self.0.is_null() {
                    // avio's buffer may have been reallocated internally by
                    // the time we get here — free the CURRENT one, not the
                    // one we originally handed it.
                    ffi::av_freep(&mut (*self.0).buffer as *mut _ as *mut c_void);
                    ffi::avio_context_free(&mut self.0);
                }
            }
        }
    }
    struct FormatGuard(*mut ffi::AVFormatContext);
    impl Drop for FormatGuard {
        fn drop(&mut self) {
            unsafe {
                if !self.0.is_null() {
                    ffi::avformat_close_input(&mut self.0);
                }
            }
        }
    }
    /// Reclaims the [`RangeReader`] we boxed and handed to FFmpeg as the
    /// AVIOContext's opaque pointer. Must outlive `AvioGuard`/`FormatGuard`
    /// (declared first here, so it drops LAST — Rust drops locals in reverse
    /// declaration order) since FFmpeg may still call back into it while
    /// tearing those down.
    struct ReaderGuard(*mut c_void);
    impl Drop for ReaderGuard {
        fn drop(&mut self) {
            unsafe { drop(Box::from_raw(self.0 as *mut RangeReader)) };
        }
    }

    unsafe extern "C" fn read_packet(opaque: *mut c_void, buf: *mut u8, buf_size: c_int) -> c_int {
        let reader = &mut *(opaque as *mut RangeReader);
        let out = std::slice::from_raw_parts_mut(buf, buf_size.max(0) as usize);
        match reader.read(out) {
            Ok(0) => AVERROR_EOF,
            Ok(n) => n as c_int,
            Err(_) => -1,
        }
    }

    unsafe extern "C" fn seek(opaque: *mut c_void, offset: i64, whence: c_int) -> i64 {
        let reader = &mut *(opaque as *mut RangeReader);
        reader.seek(offset, whence)
    }

    // Fields drop in order. FFmpeg closes before the custom I/O and reader.
    struct RemoteSession {
        format: FormatGuard,
        _avio: AvioGuard,
        _reader: ReaderGuard,
        url: String,
        headers: Vec<(String, String)>,
    }

    thread_local! {
        static SESSION: std::cell::RefCell<Option<RemoteSession>> = const {
            std::cell::RefCell::new(None)
        };
    }

    pub fn release() {
        SESSION.with(|session| session.borrow_mut().take());
    }

    pub fn open_and_seek(
        url: &str,
        time_ms: u64,
        headers: &[(String, String)],
    ) -> Result<EncodedFrame, String> {
        SESSION.with(|session| {
            let mut active = session.borrow_mut();
            if !active.as_ref().is_some_and(|source| source.url == url && source.headers == headers) {
                *active = None;
                *active = Some(open(url, headers)?);
            }
            let result = read_nearest_keyframe(active.as_ref().unwrap().format.0, time_ms);
            if result.is_err() {
                *active = None;
            }
            result
        })
    }

    fn open(url: &str, headers: &[(String, String)]) -> Result<RemoteSession, String> {
        let reader =
            Box::new(RangeReader::open(url, headers).map_err(|e| format!("range reader: {e}"))?);
        let opaque = Box::into_raw(reader) as *mut c_void;
        let _reader_guard = ReaderGuard(opaque);

        unsafe {
            let io_buf = ffi::av_malloc(AVIO_BUFFER_SIZE) as *mut u8;
            if io_buf.is_null() {
                return Err("av_malloc for avio buffer failed".into());
            }
            let avio_ctx = ffi::avio_alloc_context(
                io_buf,
                AVIO_BUFFER_SIZE as c_int,
                0,
                opaque,
                Some(read_packet),
                None,
                Some(seek),
            );
            if avio_ctx.is_null() {
                ffi::av_free(io_buf as *mut c_void);
                return Err("avio_alloc_context failed".into());
            }
            let _avio_guard = AvioGuard(avio_ctx);

            let fmt_ctx = ffi::avformat_alloc_context();
            if fmt_ctx.is_null() {
                return Err("avformat_alloc_context failed".into());
            }
            (*fmt_ctx).pb = avio_ctx;
            (*fmt_ctx).flags |= AVFMT_FLAG_CUSTOM_IO;

            let mut fmt_ctx = fmt_ctx;
            let empty = CString::new("").unwrap();
            let rc = ffi::avformat_open_input(
                &mut fmt_ctx,
                empty.as_ptr(),
                ptr::null_mut(),
                ptr::null_mut(),
            );
            if rc < 0 {
                if !fmt_ctx.is_null() {
                    ffi::avformat_close_input(&mut fmt_ctx);
                }
                return Err(format!("avformat_open_input (remote) failed: {rc}"));
            }
            let format = FormatGuard(fmt_ctx);
            if ffi::avformat_find_stream_info(fmt_ctx, ptr::null_mut()) < 0 {
                return Err("avformat_find_stream_info failed".into());
            }
            Ok(RemoteSession {
                format,
                _avio: _avio_guard,
                _reader: _reader_guard,
                url: url.to_string(),
                headers: headers.to_vec(),
            })
        }
    }
}

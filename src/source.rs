//! Opens a file, seeks to the keyframe nearest a timestamp, and hands back
//! that one packet's raw bytes plus enough codec info to decode it.
//!
//! v1 is **local files only** (the `url` param is a filesystem path). Remote
//! HTTP(S) range-based reading is the next milestone — it needs either a
//! custom AVIOContext fed by our own range-request client, or FFmpeg's own
//! http/https protocols (which need a TLS backend, not vendored yet).

pub struct EncodedFrame {
    pub codec_id: i32,
    pub extradata: Vec<u8>,
    pub width: i32,
    pub height: i32,
    pub packet: Vec<u8>,
}

#[cfg(not(target_os = "android"))]
pub fn open_and_seek(_path: &str, _time_ms: u64) -> Result<EncodedFrame, String> {
    Err("FFmpeg is only linked for Android builds".into())
}

#[cfg(target_os = "android")]
pub fn open_and_seek(path: &str, time_ms: u64) -> Result<EncodedFrame, String> {
    use crate::ffi::*;
    use std::ffi::CString;
    use std::ptr;

    struct FormatGuard(*mut AVFormatContext);
    impl Drop for FormatGuard {
        fn drop(&mut self) {
            unsafe { avformat_close_input(&mut self.0) };
        }
    }
    struct PacketGuard(*mut AVPacket);
    impl Drop for PacketGuard {
        fn drop(&mut self) {
            unsafe { av_packet_free(&mut self.0) };
        }
    }

    unsafe {
        let c_path = CString::new(path).map_err(|e| e.to_string())?;
        let mut fmt_ctx: *mut AVFormatContext = ptr::null_mut();
        let rc = avformat_open_input(
            &mut fmt_ctx,
            c_path.as_ptr(),
            ptr::null_mut(),
            ptr::null_mut(),
        );
        if rc < 0 || fmt_ctx.is_null() {
            return Err(format!("avformat_open_input failed: {rc}"));
        }
        let _guard = FormatGuard(fmt_ctx);

        if avformat_find_stream_info(fmt_ctx, ptr::null_mut()) < 0 {
            return Err("avformat_find_stream_info failed".into());
        }

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

        // Scale the wall-clock timestamp into this stream's own time_base.
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

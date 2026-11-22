//! Decodes one encoded packet (from [`crate::source`]) into an RGB24 frame.

use crate::scale::Frame;
use crate::source::EncodedFrame;

#[cfg(not(target_os = "android"))]
pub fn decode_keyframe(_encoded: &EncodedFrame) -> Result<Frame, String> {
    Err("FFmpeg is only linked for Android builds".into())
}

#[cfg(target_os = "android")]
pub fn decode_keyframe(encoded: &EncodedFrame) -> Result<Frame, String> {
    use crate::ffi::*;
    use std::ptr;

    struct CodecCtxGuard(*mut AVCodecContext);
    impl Drop for CodecCtxGuard {
        fn drop(&mut self) {
            unsafe { avcodec_free_context(&mut self.0) };
        }
    }
    struct FrameGuard(*mut AVFrame);
    impl Drop for FrameGuard {
        fn drop(&mut self) {
            unsafe { av_frame_free(&mut self.0) };
        }
    }
    struct SwsGuard(*mut SwsContext);
    impl Drop for SwsGuard {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { sws_freeContext(self.0) };
            }
        }
    }

    unsafe {
        let codec = avcodec_find_decoder(std::mem::transmute(encoded.codec_id));
        if codec.is_null() {
            return Err("no decoder for this codec".into());
        }
        let ctx = avcodec_alloc_context3(codec);
        if ctx.is_null() {
            return Err("avcodec_alloc_context3 failed".into());
        }
        let _ctx_guard = CodecCtxGuard(ctx);
        (*ctx).width = encoded.width;
        (*ctx).height = encoded.height;
        if !encoded.extradata.is_empty() {
            // FFmpeg expects AV_INPUT_BUFFER_PADDING_SIZE extra zeroed bytes
            // past the end — av_malloc'd so avcodec can own/free it with the
            // context.
            let size = encoded.extradata.len();
            let buf = av_malloc(size + 64) as *mut u8;
            if buf.is_null() {
                return Err("av_malloc for extradata failed".into());
            }
            ptr::copy_nonoverlapping(encoded.extradata.as_ptr(), buf, size);
            ptr::write_bytes(buf.add(size), 0, 64);
            (*ctx).extradata = buf;
            (*ctx).extradata_size = size as i32;
        }

        if avcodec_open2(ctx, codec, ptr::null_mut()) < 0 {
            return Err("avcodec_open2 failed".into());
        }

        let pkt = av_packet_alloc();
        if pkt.is_null() {
            return Err("av_packet_alloc failed".into());
        }
        let data = av_malloc(encoded.packet.len() + 64) as *mut u8;
        ptr::copy_nonoverlapping(encoded.packet.as_ptr(), data, encoded.packet.len());
        ptr::write_bytes(data.add(encoded.packet.len()), 0, 64);
        (*pkt).data = data;
        (*pkt).size = encoded.packet.len() as i32;

        if avcodec_send_packet(ctx, pkt) < 0 {
            av_packet_free(&mut (pkt as *mut AVPacket));
            return Err("avcodec_send_packet failed".into());
        }

        let frame = av_frame_alloc();
        if frame.is_null() {
            av_packet_free(&mut (pkt as *mut AVPacket));
            return Err("av_frame_alloc failed".into());
        }
        let _frame_guard = FrameGuard(frame);

        // A single intra keyframe is almost always ready on the first
        // receive; retry a few times (flushing with a null packet) to cover
        // decoders that buffer one frame before emitting anything.
        let mut got_frame = false;
        for attempt in 0..4 {
            let rc = avcodec_receive_frame(ctx, frame);
            if rc == 0 {
                got_frame = true;
                break;
            }
            const EAGAIN: i32 = 11;
            if rc == -EAGAIN && attempt < 3 {
                avcodec_send_packet(ctx, ptr::null());
                continue;
            }
            break;
        }
        av_packet_free(&mut (pkt as *mut AVPacket));
        if !got_frame {
            return Err("decoder never produced a frame".into());
        }

        let width = (*frame).width as u32;
        let height = (*frame).height as u32;
        let src_fmt = (*frame).format;

        let sws = sws_getContext(
            width as i32,
            height as i32,
            std::mem::transmute(src_fmt),
            width as i32,
            height as i32,
            AVPixelFormat_AV_PIX_FMT_RGB24,
            4, // SWS_BILINEAR — irrelevant here since src/dst size match
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null(),
        );
        if sws.is_null() {
            return Err("sws_getContext failed".into());
        }
        let _sws_guard = SwsGuard(sws);

        let mut rgb = vec![0u8; (width * height * 3) as usize];
        let dst_linesize = [(width * 3) as i32, 0, 0, 0];
        let mut dst_ptr = [rgb.as_mut_ptr(), ptr::null_mut(), ptr::null_mut(), ptr::null_mut()];

        sws_scale(
            sws,
            (*frame).data.as_ptr() as *const *const u8,
            (*frame).linesize.as_ptr(),
            0,
            height as i32,
            dst_ptr.as_mut_ptr(),
            dst_linesize.as_ptr(),
        );

        Ok(Frame { width, height, rgb })
    }
}

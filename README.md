# keyframe-thumbs

Generates seek-bar thumbnail previews by decoding a single video keyframe
at a time, instead of running a real playback session. Works against a
remote URL (HTTP range requests, no full download) or an HLS segment.

## Status

Scaffolding stage. `hls`, `scale`, `encode` and `session` are real and
tested. `source` (keyframe index + range reads) and `codec` (FFmpeg decode)
are stubs — next milestone.

## Building for Android

```
cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 -o jniLibs build --release
```

Produces `libkeyframe_thumbs.so` per ABI under `jniLibs/`, exporting
`Java_keyframethumbs_Native_requestThumbnail`. A consuming app declares a
Kotlin/Java class named exactly `keyframethumbs.Native` with a matching
`external fun requestThumbnail(url: String, timeMs: Long): ByteArray` to
call into it — no dependency on the consumer's own package name.

## Testing

```
cargo test
```

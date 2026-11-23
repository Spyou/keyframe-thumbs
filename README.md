# keyframe-thumbs

Generates seek-bar thumbnail previews by decoding a single video keyframe
at a time, instead of running a real playback session. Works against a
remote URL (HTTP range requests, no full download) or an HLS segment.

## Status

`hls`, `scale`, `encode`, `session` and the local-file decode path
(`source` + `codec`, backed by a vendored FFmpeg) are real and tested.
Remote HTTP(S) range-based reading isn't wired up yet — `source::open_and_seek`
takes a local file path for now.

## Building for Android

First build (or rebuild) the vendored FFmpeg static libs — not committed to
git, see `vendor/build_android.sh`:

```
export ANDROID_NDK_HOME=/path/to/ndk
./vendor/build_android.sh arm64-v8a
./vendor/build_android.sh armeabi-v7a
./vendor/build_android.sh x86_64
./vendor/build_android.sh x86   # needs EXTRA_CONFIGURE="--disable-asm" — see the script
```

Then:

```
cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 -t x86 -o jniLibs build --release
```

Produces `libkeyframe_thumbs.so` per ABI under `jniLibs/`, exporting
`Java_keyframethumbs_Native_requestThumbnail`. A consuming app declares a
Kotlin/Java class named exactly `keyframethumbs.Native` with a matching
`external fun requestThumbnail(url: String, timeMs: Long, headers: Array<String>): ByteArray`
to call into it — no dependency on the consumer's own package name. `headers`
is flat, alternating key/value (`arrayOf("Referer", "https://...", "Cookie", "...")`).

## Testing

```
cargo test
```

## License

MIT — see [LICENSE](LICENSE).

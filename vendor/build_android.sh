#!/bin/bash
# Cross-compiles a trimmed FFmpeg (decode-only, the codecs/containers this
# crate actually needs) for one Android ABI, via the NDK toolchain.
set -euo pipefail

ABI="$1" # arm64-v8a | armeabi-v7a | x86_64 | x86
# x86 needs EXTRA_CONFIGURE="--disable-asm" — clang's inline-asm register
# allocation for the 32-bit CABAC decoder fails on this target; x86 Android
# devices are effectively extinct, so losing hand-written asm there costs
# nothing worth chasing further.
API=21
NDK="${ANDROID_NDK_HOME:?set ANDROID_NDK_HOME}"
TOOLCHAIN="$NDK/toolchains/llvm/prebuilt/darwin-x86_64"
SRC_DIR="$(cd "$(dirname "$0")/ffmpeg-7.1" && pwd)"
OUT_DIR="$(cd "$(dirname "$0")" && pwd)/out/$ABI"

case "$ABI" in
  arm64-v8a)      ARCH=aarch64; CC_TRIPLE=aarch64-linux-android ;;
  armeabi-v7a)    ARCH=arm;     CC_TRIPLE=armv7a-linux-androideabi ;;
  x86_64)         ARCH=x86_64;  CC_TRIPLE=x86_64-linux-android ;;
  x86)            ARCH=x86;     CC_TRIPLE=i686-linux-android ;;
  *) echo "unknown ABI $ABI"; exit 1 ;;
esac

CC="$TOOLCHAIN/bin/${CC_TRIPLE}${API}-clang"
CXX="$TOOLCHAIN/bin/${CC_TRIPLE}${API}-clang++"
AR="$TOOLCHAIN/bin/llvm-ar"
NM="$TOOLCHAIN/bin/llvm-nm"
STRIP="$TOOLCHAIN/bin/llvm-strip"
RANLIB="$TOOLCHAIN/bin/llvm-ranlib"

mkdir -p "$OUT_DIR"
cd "$SRC_DIR"

# Decode-only, the exact pieces this crate needs: mp4/mkv/TS container
# parsing (demuxers), H.264/HEVC decode, and swscale for pixel-format
# conversion. Everything else (muxers, network protocols, filters, devices,
# audio resampling) is off — we feed bytes in ourselves via a custom
# AVIOContext, so FFmpeg never needs its own network code.
./configure \
  --target-os=android \
  --arch="$ARCH" \
  --cpu=generic \
  --enable-cross-compile \
  --cc="$CC" \
  --cxx="$CXX" \
  --ar="$AR" \
  --nm="$NM" \
  --strip="$STRIP" \
  --ranlib="$RANLIB" \
  --sysroot="$TOOLCHAIN/sysroot" \
  --enable-pic \
  --disable-shared \
  --enable-static \
  --disable-doc \
  --disable-programs \
  --disable-avdevice \
  --disable-avfilter \
  --disable-swresample \
  --disable-network \
  --disable-everything \
  --enable-decoder=h264,hevc \
  --enable-parser=h264,hevc \
  --enable-demuxer=mov,matroska,mpegts \
  --enable-swscale \
  ${EXTRA_CONFIGURE:-} \
  --prefix="$OUT_DIR" \
  --extra-cflags="-Os -fPIC" \
  --extra-ldflags="-static-libstdc++"

make clean || true
make -j"$(sysctl -n hw.ncpu)"
make install

echo "Built $ABI -> $OUT_DIR"

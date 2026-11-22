mod codec;
mod encode;
mod hls;
mod pipeline;
mod scale;
mod session;
mod source;

use jni::objects::{JByteArray, JClass, JString};
use jni::sys::jlong;
use jni::JNIEnv;

/// Exposed to any app's Kotlin/Java side as `keyframethumbs.Native`, so no
/// consumer's own package name needs to be baked in here — a thin Kotlin
/// shim class under that exact name is all a consumer needs to declare to
/// call into this library.
#[no_mangle]
pub extern "system" fn Java_keyframethumbs_Native_requestThumbnail<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    url: JString<'local>,
    time_ms: jlong,
) -> JByteArray<'local> {
    let url: String = env
        .get_string(&url)
        .map(|s| s.into())
        .unwrap_or_default();
    let jpeg = pipeline::thumbnail_for(&url, &[], time_ms.max(0) as u64);
    match jpeg {
        Some(bytes) => env
            .byte_array_from_slice(&bytes)
            .unwrap_or_else(|_| empty_array(&mut env)),
        None => empty_array(&mut env),
    }
}

fn empty_array<'local>(env: &mut JNIEnv<'local>) -> JByteArray<'local> {
    // An empty array, not a Java null, so the Kotlin side has one shape to
    // handle ("no thumbnail") instead of two.
    env.new_byte_array(0).expect("allocating an empty array")
}

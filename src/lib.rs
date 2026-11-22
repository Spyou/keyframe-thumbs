mod codec;
mod encode;
mod ffi;
mod hls;
mod net;
mod pipeline;
mod scale;
mod session;
mod source;

use jni::objects::{JByteArray, JClass, JObjectArray, JString};
use jni::sys::jlong;
use jni::JNIEnv;

/// Exposed to any app's Kotlin/Java side as `keyframethumbs.Native`, so no
/// consumer's own package name needs to be baked in here — a thin Kotlin
/// shim class under that exact name is all a consumer needs to declare to
/// call into this library.
///
/// `headers` is a flat `Array<String>` of alternating key, value, value, key,
/// value... (odd length / unpaired trailing key is just dropped) — avoids
/// needing a JNI Map conversion or a JSON dependency for what's otherwise a
/// handful of strings.
#[no_mangle]
pub extern "system" fn Java_keyframethumbs_Native_requestThumbnail<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    url: JString<'local>,
    time_ms: jlong,
    headers: JObjectArray<'local>,
) -> JByteArray<'local> {
    let url: String = env.get_string(&url).map(|s| s.into()).unwrap_or_default();
    let headers = read_headers(&mut env, &headers);
    let jpeg = pipeline::thumbnail_for(&url, &headers, time_ms.max(0) as u64);
    match jpeg {
        Some(bytes) => env
            .byte_array_from_slice(&bytes)
            .unwrap_or_else(|_| empty_array(&mut env)),
        None => empty_array(&mut env),
    }
}

/// Call on the same worker thread that requests thumbnails.
#[no_mangle]
pub extern "system" fn Java_keyframethumbs_Native_releaseThumbnailSession(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
) {
    #[cfg(target_os = "android")]
    source::release();
}

/// Diagnostic-only: same inputs as `requestThumbnail`, but returns
/// "OK:<n bytes>" or "ERR:<stage>: <message>" as a string instead of the
/// JPEG — lets a failure be read directly off a log line instead of
/// collapsing to an empty array. Not part of the stable surface.
#[no_mangle]
pub extern "system" fn Java_keyframethumbs_Native_requestThumbnailDebug<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    url: JString<'local>,
    time_ms: jlong,
    headers: JObjectArray<'local>,
) -> JString<'local> {
    let url: String = env.get_string(&url).map(|s| s.into()).unwrap_or_default();
    let headers = read_headers(&mut env, &headers);
    let result = pipeline::thumbnail_for_debug(&url, &headers, time_ms.max(0) as u64);
    let msg = match result {
        Ok(bytes) => format!("OK:{} bytes", bytes.len()),
        Err(e) => format!("ERR:{e}"),
    };
    env.new_string(msg).expect("allocating result string")
}

fn read_headers<'local>(
    env: &mut JNIEnv<'local>,
    headers: &JObjectArray<'local>,
) -> Vec<(String, String)> {
    let len = env.get_array_length(headers).unwrap_or(0);
    let mut flat = Vec::with_capacity(len.max(0) as usize);
    for i in 0..len {
        let s: String = match env.get_object_array_element(headers, i) {
            Ok(obj) => {
                let jstr: JString = obj.into();
                env.get_string(&jstr)
                    .map(|s| s.into())
                    .unwrap_or_default()
            }
            Err(_) => String::new(),
        };
        flat.push(s);
    }
    flat.chunks_exact(2)
        .map(|pair| (pair[0].clone(), pair[1].clone()))
        .collect()
}

fn empty_array<'local>(env: &mut JNIEnv<'local>) -> JByteArray<'local> {
    // An empty array, not a Java null, so the Kotlin side has one shape to
    // handle ("no thumbnail") instead of two.
    env.new_byte_array(0).expect("allocating an empty array")
}

use std::env;
use std::path::PathBuf;

fn main() {
    let target = env::var("TARGET").unwrap();
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

    let (abi, clang_target) = match target.as_str() {
        "aarch64-linux-android" => ("arm64-v8a", "aarch64-linux-android21"),
        "armv7-linux-androideabi" => ("armeabi-v7a", "armv7a-linux-androideabi21"),
        "x86_64-linux-android" => ("x86_64", "x86_64-linux-android21"),
        "i686-linux-android" => ("x86", "i686-linux-android21"),
        other => {
            // Non-Android targets (plain `cargo test` on the host while
            // iterating on the pure-Rust modules) skip FFmpeg entirely —
            // only the Android build links it.
            println!("cargo:warning=skipping FFmpeg link for host target {other}");
            return;
        }
    };

    let vendor_dir = PathBuf::from(&manifest_dir).join("vendor/out").join(abi);
    let lib_dir = vendor_dir.join("lib");
    let include_dir = vendor_dir.join("include");

    if !lib_dir.exists() {
        panic!(
            "no vendored FFmpeg libs for {abi} at {} — run vendor/build_android.sh {abi} first",
            lib_dir.display()
        );
    }

    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=static=avformat");
    println!("cargo:rustc-link-lib=static=avcodec");
    println!("cargo:rustc-link-lib=static=avutil");
    println!("cargo:rustc-link-lib=static=swscale");
    println!("cargo:rustc-link-lib=dylib=z");
    println!("cargo:rustc-link-lib=dylib=m");

    let ndk = env::var("ANDROID_NDK_HOME").expect("ANDROID_NDK_HOME must be set to build for Android");
    let sysroot = PathBuf::from(&ndk).join("toolchains/llvm/prebuilt/darwin-x86_64/sysroot");

    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        .clang_arg(format!("-I{}", include_dir.display()))
        .clang_arg(format!("--target={clang_target}"))
        .clang_arg(format!("--sysroot={}", sysroot.display()))
        .allowlist_function("av_.*")
        .allowlist_function("avio_.*")
        .allowlist_function("avformat_.*")
        .allowlist_function("avcodec_.*")
        .allowlist_function("sws_.*")
        .allowlist_type("AV.*")
        .allowlist_type("Sws.*")
        .allowlist_var("AV_.*")
        .layout_tests(false)
        .generate()
        .expect("generating FFmpeg bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("writing FFmpeg bindings");
}

//! Raw FFmpeg bindings, generated at build time by `build.rs` from the
//! vendored headers under `vendor/out/<abi>/include`. Everything in here is
//! `unsafe` by nature — [`codec`] and [`source`] are the only callers, and
//! they're responsible for using it correctly (matching FFmpeg's own
//! calling conventions: who allocates, who frees, which calls can return
//! EAGAIN, etc).
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]

#[cfg(target_os = "android")]
include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

//! The startup chain for the C++ talker beside it (issue 1512).
//!
//! The same arrangement as the `c/talker` sibling, whose `main.rs` explains it
//! at length: on a board with no RTOS the port IS the board, and this board's
//! port — reset vector, CMSDK Timer0 clock, LAN9118 + smoltcp, semihosting
//! console — is Rust with no C entry point. So the link root is this crate,
//! `build.rs` compiles `src/talker.cpp` into it, and `NROS_APP_MAIN_REGISTER()`
//! there emits the `app_main()` called below once the board is up.
//!
//! One thing is C++-specific: the generated message types serialize in RUST.
//! `nros generate cpp` emits, per message, a `#[repr(C)]` mirror plus CDR field
//! serializers (`*_types.rs`) and the `nros_cpp_{serialize,publish,…}_*` C
//! exports the C++ header calls (`*_exports.rs`). On the cmake road a
//! per-package staticlib wraps them (`cmake/ffi_lib_rs.in`); here they are
//! compiled into this binary, in [`msg_glue`].

#![no_std]
#![no_main]

use nros_board_mps2_an385::{Config, entry, run_bare};

unsafe extern "C" {
    /// Emitted by `NROS_APP_MAIN_REGISTER()` in `src/talker.cpp`.
    fn app_main();
}

/// The generated C++ message glue, under the prelude it expects.
///
/// The cargo-rooted counterpart of `cmake/ffi_lib_rs.in`'s module-level half
/// (that template's `#![no_std]` and `#[panic_handler]` are crate-level and
/// belong to its own staticlib; this image already has both). What the glue
/// names without declaring is exactly these four things: the `nros_serdes`
/// codec types, `fixed_str` for the C++ `char[N]` string fields, and the
/// `nros_cpp_publish_raw` the `publish` exports hand their bytes to — which
/// `nros-cpp` defines and this binary links.
#[allow(
    non_camel_case_types,
    dead_code,
    clippy::all,
    unused_imports,
    unsafe_op_in_unsafe_fn
)]
mod msg_glue {
    use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

    unsafe extern "C" {
        fn nros_cpp_publish_raw(handle: *mut core::ffi::c_void, data: *const u8, len: usize)
        -> i32;
    }

    /// View a fixed-capacity C string buffer (`char[N]`, NUL-terminated) as a
    /// `&str`, stopping at the FIRST NUL — the bytes after it are whatever the
    /// C++ side left there. Same contract as the template's copy.
    fn fixed_str(buf: &[u8]) -> &str {
        let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        core::str::from_utf8(&buf[..end]).unwrap_or("")
    }

    include!(concat!(env!("OUT_DIR"), "/cpp_msg_glue.rs"));
}

/// Keep the C and C++ `#[no_mangle]` surfaces in the link.
///
/// `nros-cpp` is an RLIB here and the board links with `--gc-sections`, so the
/// entry points only `talker.cpp` calls would otherwise be dropped before the
/// C++ object asks for them. `FORCE_LINK_ANCHOR` is the crate's own answer to
/// exactly this ("the runtime root references THIS with its own `#[used]`"):
/// the C ABI, the C++ FFI and the selected backend's register closure.
fn anchor_cpp_surface() {
    core::hint::black_box(&nros_cpp::FORCE_LINK_ANCHOR);
}

#[entry]
fn main() -> ! {
    anchor_cpp_surface();

    // The backend is registered by the C++ side's `nros_app_register_backends()`
    // (in `src/talker.cpp`), which `nros_cpp_init` calls — the same hook the
    // cmake road generates. Bare metal walks no `.init_array`.
    run_bare(Config::default(), |_cfg| {
        // SAFETY: `app_main` is the entry the header's own macro emits; it takes
        // no arguments and returns nothing, and the board is up by now.
        unsafe { app_main() };
        Ok::<(), core::convert::Infallible>(())
    })
}

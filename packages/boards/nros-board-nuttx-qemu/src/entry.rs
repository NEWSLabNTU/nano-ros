//! NuttX init entry point override.
//!
//! NuttX boots the kernel, then starts the init task by calling
//! `CONFIG_INIT_ENTRYPOINT` (default: `nsh_main`). The default `nsh_main`
//! from NuttX's apps library starts an interactive shell — our application
//! code is never called.
//!
//! This module provides a custom `nsh_main` that calls the image's `main`
//! symbol. Because this symbol is in the main binary, it takes precedence over
//! the archive definition in `libapps.a`.
//!
//! Call chain: NuttX init → `nsh_main` (ours) → `main`
//!
//! phase-359 W7 — that `main` used to be libstd's `lang_start` shim, which then
//! called the user's Rust `fn main()`. The family is `no_std` now, so
//! `nros::main!()` emits this exact C-ABI symbol directly and the chain is one
//! call shorter. Nothing here changed: this file always went through the C ABI,
//! which is why it kept working.

use core::ffi::c_char;

unsafe extern "C" {
    fn main(argc: i32, argv: *const *const c_char) -> i32;
    fn nsh_initialize() -> i32;
}

/// Override NuttX's default `nsh_main` to run the Rust application.
///
/// NuttX's scheduler calls this as the init task (PID 1). We first call
/// `nsh_initialize()` to run the standard NSH init sequence (board bringup,
/// network init, filesystem mounts), then redirect to Rust's `main`.
///
/// Without `nsh_initialize()`, virtio device discovery (via FDT),
/// network interface configuration (via netinit), and other board-level
/// initialization would be skipped.
#[unsafe(no_mangle)]
pub extern "C" fn nsh_main(argc: i32, argv: *const *const c_char) -> i32 {
    // issue 0710 — publish the nros_log sink list HERE.
    //
    // This is a boot funnel that issue 0708's rule cannot see: it is
    // `pub extern "C" fn nsh_main`, not `pub fn run*`, so neither that fix nor
    // `check-board-log-sink` reached it. An image entering this way — the
    // logging smoke fixture does — ran with no sink list and dropped every
    // record, which is how it emitted nothing while the gate stayed green.
    //
    // After `nsh_initialize()`, because that is board bringup (console among
    // it) and a record published before the console exists has nowhere to go.
    unsafe {
        nsh_initialize();
        ::nros_log::init_default();
        main(argc, argv)
    }
}

// Prevent linker from garbage-collecting nsh_main when --gc-sections is active.
// The NuttX kernel (libsched.a) references nsh_main, but the Rust linker may
// not see that reference early enough to keep the symbol.
#[used]
static _NSH_MAIN_REF: unsafe extern "C" fn(i32, *const *const c_char) -> i32 = nsh_main;

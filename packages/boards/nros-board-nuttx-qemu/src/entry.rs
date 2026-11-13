//! NuttX init entry point.
//!
//! NuttX boots the kernel, then starts the init task by calling the function
//! `CONFIG_INIT_ENTRYPOINT` names. This board's defconfigs name
//! `nros_nuttx_entry`, defined here, so the kernel starts the image directly
//! (RFC-0126 decision 18, phase-550 W11).
//!
//! Call chain: NuttX init → `nros_nuttx_entry` → `main`
//!
//! This used to override the `nsh_main` symbol instead, relying on the board
//! rlib preceding `libapps.a` on the link line so that ours won over NSH's.
//! Naming the entry in `CONFIG_INIT_ENTRYPOINT` is NuttX's own mechanism, and
//! it depends on no link order.
//!
//! phase-359 W7 — that `main` used to be libstd's `lang_start` shim, which then
//! called the user's Rust `fn main()`. The family is `no_std` now, so
//! `nros::main!()` emits this exact C-ABI symbol directly and the chain is one
//! call shorter.

use core::ffi::c_char;

unsafe extern "C" {
    fn main(argc: i32, argv: *const *const c_char) -> i32;
    fn nsh_initialize() -> i32;
}

/// The init task NuttX starts (`CONFIG_INIT_ENTRYPOINT`), which runs the image.
///
/// NuttX's scheduler calls this as the init task (PID 1). We first call
/// `nsh_initialize()` to run the standard NSH init sequence (board bringup,
/// network init, filesystem mounts), then redirect to Rust's `main`.
///
/// Without `nsh_initialize()`, virtio device discovery (via FDT),
/// network interface configuration (via netinit), and other board-level
/// initialization would be skipped.
#[unsafe(no_mangle)]
pub extern "C" fn nros_nuttx_entry(argc: i32, argv: *const *const c_char) -> i32 {
    // issue 0710 — publish the nros_log sink list HERE.
    //
    // This is a boot funnel that issue 0708's rule cannot see: it is
    // `pub extern "C" fn nros_nuttx_entry`, not `pub fn run*`, so neither that fix nor
    // `check-board-log-sink` reached it. An image entering this way — the
    // logging smoke fixture does — ran with no sink list and dropped every
    // record, which is how it emitted nothing while the gate stayed green.
    //
    // After `nsh_initialize()`, because that is board bringup (console among
    // it) and a record published before the console exists has nowhere to go.
    unsafe {
        nsh_initialize();
        ::nros_platform_cffi::log::init_default();
        main(argc, argv)
    }
}

// Prevent the linker from garbage-collecting the entry when --gc-sections is
// active. The NuttX kernel (libsched.a) references it through
// CONFIG_INIT_ENTRYPOINT, but the Rust linker may not see that reference early
// enough to keep the symbol.
#[used]
static _NROS_NUTTX_ENTRY_REF: unsafe extern "C" fn(i32, *const *const c_char) -> i32 =
    nros_nuttx_entry;

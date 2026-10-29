//! NuttX kernel + FFI entry point for C/C++ examples.
//!
//! This binary provides the NuttX kernel (via -Z build-std=core,alloc) and calls
//! `nros_app_main()` defined in C/C++ code (linked by CMake).

// phase-359 W7 — `no_std` + `no_main`, like the other C-runtime families' bins.
// The image no longer compiles the standard library, so libstd's `lang_start`
// does not supply the `main` symbol NuttX's `nsh_main` calls; this bin defines
// it directly. `nros-c` (linked below) owns the image's `#[panic_handler]` and
// `#[global_allocator]`, which is why the board crate is taken here with
// `default-features = false`.
#![no_std]
#![no_main]

// Force-link crates so their symbols are available to C/C++ code.
// nros_board_nuttx_qemu provides the NuttX kernel + board startup code.
extern crate nros_board_nuttx_qemu;
extern crate nros_c;
extern crate nros_cpp;
extern crate nros_rmw_zenoh;

unsafe extern "C" {
    /// The app's entry, defined in C/C++ beside its `NROS_APP_MAIN_REGISTER()`.
    fn nros_app_main(argc: i32, argv: *mut *mut core::ffi::c_char) -> i32;
}

/// The image entry NuttX's `nsh_main` calls. Signature matches the
/// `extern "C" fn main(argc, argv)` that `nros-board-nuttx-qemu`'s `entry.rs`
/// declares and calls.
#[unsafe(no_mangle)]
pub extern "C" fn main(_argc: i32, _argv: *const *const core::ffi::c_char) -> i32 {
    // Phase 104.A — bare-metal callers explicitly register the RMW
    // backend before `Executor::open`. POSIX hosts auto-register via
    // `.init_array`; this target doesn't walk that section.
    nros_rmw_zenoh::register().expect("Failed to register RMW backend");

    // Issue 2048 — NuttX owns `eth0` (RFC-0109 D2): the kernel brought it up
    // from its `defconfig` before `nsh_main` reached us. This used to push an
    // address in (issue #130, ~~`configure_entry_eth0`~~ from a never-baked
    // ~~`NROS_IP`~~/~~`NROS_GATEWAY`~~/~~`NROS_PREFIX`~~); it now reports what the kernel
    // applied, the same line the Rust entry path prints.
    nros_board_nuttx_qemu::report_kernel_eth0();

    // Issue 1770 — return the APP's status, the `int main` shim's contract
    // (`<rclc/detail/app_main.h>`). This called the VOID shim's `app_main` and
    // returned 0, so nsh saw success for an app that failed.
    // SAFETY: `nros_app_main` is the C entry every app defines; `argc = 0` with
    // a null `argv` is what the header's own VOID shim passes.
    unsafe { nros_app_main(0, core::ptr::null_mut()) }
}

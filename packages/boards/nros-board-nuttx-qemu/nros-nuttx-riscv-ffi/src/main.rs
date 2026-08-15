//! NuttX kernel + FFI entry point for C/C++ examples.
//!
//! This binary provides the NuttX kernel (via -Z build-std=core,alloc) and calls
//! `app_main()` defined in C/C++ code (linked by CMake).


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
    fn app_main();
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
    unsafe { app_main() };
    0
}

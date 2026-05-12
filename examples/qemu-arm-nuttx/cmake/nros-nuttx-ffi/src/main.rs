//! NuttX kernel + FFI entry point for C/C++ examples.
//!
//! This binary provides the NuttX kernel (via -Z build-std=std) and calls
//! `app_main()` defined in C/C++ code (linked by CMake).

// Force-link crates so their symbols are available to C/C++ code.
// nros_board_nuttx_qemu_arm provides the NuttX kernel + board startup code.
extern crate nros_board_nuttx_qemu_arm;
extern crate nros_c;
extern crate nros_cpp;
extern crate nros_rmw_zenoh;

unsafe extern "C" {
    fn app_main();
}

fn main() {
    // Phase 115.L.x — install zenoh-pico C-vtable backend before
    // the C/C++ entry point opens its nros session.
    nros_rmw_zenoh::register().expect("zenoh RMW register failed");
    unsafe { app_main() };
}

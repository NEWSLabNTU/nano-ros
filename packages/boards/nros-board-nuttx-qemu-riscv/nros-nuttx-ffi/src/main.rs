//! NuttX kernel + FFI entry point for C/C++ examples.
//!
//! This binary provides the NuttX kernel (via -Z build-std=std) and calls
//! `app_main()` defined in C/C++ code (linked by CMake).

// Force-link crates so their symbols are available to C/C++ code.
// nros_board_nuttx_qemu_riscv provides the NuttX kernel + board startup code.
extern crate nros_board_nuttx_qemu_riscv;
extern crate nros_c;
extern crate nros_cpp;
extern crate nros_rmw_zenoh;

unsafe extern "C" {
    fn app_main();
}

fn main() {
    // Phase 104.A — bare-metal callers explicitly register the RMW
    // backend before `Executor::open`. POSIX hosts auto-register via
    // `.init_array`; this target doesn't walk that section.
    nros_rmw_zenoh::register().expect("Failed to register RMW backend");
    unsafe { app_main() };
}

//! Issue 2131 -- the C++ heap-free-tier image.
//!
//! `heap-free-poc-mps2`'s C++ twin. That image proves the RUST runtime path
//! links with no allocation symbol; this one proves the same of the C and C++
//! surfaces: `nros-cpp` and `nros-c` built without `alloc`, their whole
//! `#[no_mangle]` ABI pulled into the link by `FORCE_LINK_ANCHOR`, and a C++
//! program (`src/poc.cpp`) that opens the executor, creates a node, declares,
//! sets and reads a parameter, makes a timer and spins. The gate
//! (`scripts/check-no-alloc-image.py --tier heap-free`) reads the linked ELF:
//! no `#[global_allocator]`, no `__rust_alloc`, no `malloc`, no
//! `nros_platform_alloc`.
//!
//! No RMW backend is linked, for the reason the Rust image gives: every
//! in-tree transport allocates in its vendored C. `rclcpp::init_in()` is
//! therefore refused at RUN time, which the program reports as status 0, the
//! success marker. The node, parameter and spin calls stay in the image
//! because the init's outcome is opaque to the compiler (it reads the backend
//! registry). Status 2 means init unexpectedly succeeded.
//!
//! Like the bare-metal C++ examples, the link root is Rust: this board's port
//! (reset vector, semihosting console) is Rust with no C entry point, and
//! `cortex-m-rt` runs no C++ static constructors, so `poc.cpp` keeps its
//! handles in `main`'s frame.

#![no_std]
#![no_main]

use cortex_m_rt::entry;
use cortex_m_semihosting::{debug, hprintln};

// Force-link the per-platform export so the `nros_platform_*` C ABI resolves
// (staticlib DCE drops the rlib otherwise).
extern crate nros_platform_mps2_an385 as _;

unsafe extern "C" {
    /// `poc.cpp`'s `main`, renamed and forwarded by `nros_cc_flags::app_main`.
    fn nros_app_main(argc: i32, argv: *mut *mut core::ffi::c_char) -> i32;
}

#[entry]
fn main() -> ! {
    // The whole C and C++ ABI in the link, not only what `poc.cpp` calls: the
    // gate is about the surface a C++ image CAN reach, and `--gc-sections`
    // would otherwise drop what this program does not name.
    core::hint::black_box(&nros_cpp::FORCE_LINK_ANCHOR);
    nros_platform_cffi::log::init_default();

    // SAFETY: `nros_app_main` is the `extern "C"` forwarder build.rs generates;
    // `argc = 0` with a null `argv` is what the VOID shim passes.
    let status = unsafe { nros_app_main(0, core::ptr::null_mut()) };
    match status {
        0 => {
            hprintln!("HEAP-FREE-CPP-POC: init refused (no RMW linked) -- link property holds");
            debug::exit(debug::EXIT_SUCCESS);
        }
        other => {
            hprintln!("HEAP-FREE-CPP-POC: unexpected status {}", other);
            debug::exit(debug::EXIT_FAILURE);
        }
    }
    loop {}
}

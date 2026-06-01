#![no_std]
#![no_main]

use panic_semihosting as _;

// Phase 212.M.5.a.3 — keep the per-pkg `__nros_component_<pkg>_register`
// symbols alive against `--gc-sections`. `nros::component!()` already
// emits the fn with `#[unsafe(no_mangle)]` + a `#[used]` presence
// marker, but cargo only pulls the rlibs into the link graph when
// the binary references *some* symbol from each — `extern crate _`
// is the canonical no-cost reference.
extern crate talker_pkg as _;
extern crate listener_pkg as _;

#[unsafe(no_mangle)]
extern "C" fn _start() -> ! {
    freertos_qemu_mps2_an385_bsp::nros_run()
}

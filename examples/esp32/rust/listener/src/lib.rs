//! ESP-IDF listener — Phase 212.M.7 Entry pkg.
//!
//! Migrated from the pre-212 `platform-bare-metal` (esp-hal) shape to
//! the ESP-IDF `idf.py` workflow. The crate compiles as a staticlib
//! linked into the IDF `main` component; the C entry point
//! (`main/app_main.c`) calls `rust_app_main()` after IDF finishes
//! board init.
//!
//! Wire-through bar (Phase 212.M.7): the integration shell
//! (`integrations/nano-ros/CMakeLists.txt`) pulls in the umbrella
//! `NanoRos::NanoRos` staticlib (XRCE C-FFI vtable + nros-platform-
//! freertos shim) and codegen-bakes a `system_main.c` when a
//! bringup is wired. This Rust crate is intentionally minimal until
//! Wi-Fi bring-up + real Pub/Sub wiring lands (follow-up): it
//! exports `rust_app_main()` so the C app_main can call into Rust,
//! mirroring the H.5 fixture's `nros_component_*` stub shape.

#![no_std]

// Phase 212.M.7 — staticlib needs a `#[panic_handler]` at compile
// time. Provide a minimal abort-loop here; once real code lands the
// example will switch to `panic-halt` or wire `esp-backtrace`.
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}

/// Entry point invoked by `main/app_main.c` after IDF boots.
///
/// `extern "C"` and `#[no_mangle]` so the C side can name it
/// directly. Returns `i32` for easy `app_main` propagation
/// (0 = ok).
#[unsafe(no_mangle)]
pub extern "C" fn rust_app_main() -> i32 {
    // TODO(212.M.7 follow-up): Wi-Fi bring-up via `esp_wifi_*` is
    // expected to happen in `app_main.c` before this call. Once the
    // network stack is live, open an Executor with an XRCE locator,
    // declare a `std_msgs/Int32` subscription on `/chatter` and spin.
    // The pre-212 `esp-hal` code is preserved in git history (commit
    // before this migration) for reference.
    0
}

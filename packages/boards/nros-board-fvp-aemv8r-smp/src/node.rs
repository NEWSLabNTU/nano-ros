//! Board lifecycle.
//!
//! Zephyr brings the system up itself: boot, MMU, drivers, network
//! stack. By the time `run` is called all of that is already done — no
//! `init_hardware` work remains beyond touching the platform crate so
//! its FFI symbols (clock, sleep, RNG) are linked into the final
//! image.

use crate::Config;

/// Touch the platform crate so Zephyr's linker pulls in the platform
/// FFI shims (clock, sleep, RNG). On native_sim the link succeeds
/// without this; on the FVP target the symbols are pulled by the
/// Zephyr Rust application via this re-export.
pub fn init_hardware() {
    // Reference a public constant from `nros-platform` so the platform
    // C ABI provider gets pulled into the final link map even when LTO
    // is on. (Phase 121.3 — formerly via `nros-platform-zephyr`.)
    let _ = nros_platform::NET_SOCKET_SIZE;

    // Phase 248 C5a (#60 T4) — the board owns RMW registration. Register the
    // linked Cyclone DDS backend into the CFFI vtable before the app closure
    // opens a session. Zephyr (`target_os = "none"`) is linkme-blind + runs no
    // `.init_array` walk, so the auto-register section is a no-op; this explicit,
    // idempotent call is the registration path (mirrors `nros::__register_linked_rmw`).
    // The C++ `nros_rmw_cyclonedds_register` symbol it calls is CMake-provided.
    // Gated on the board's own `rmw-cyclonedds` feature.
    #[cfg(feature = "rmw-cyclonedds")]
    let _ = nros_rmw_cyclonedds_sys::register();
}

/// Run the user closure once hardware + network are initialised. The
/// closure receives the resolved [`Config`] and is expected to build
/// an `Executor` via `nros::Node::new(...)` etc.
///
/// This is a thin wrapper — `Zephyr` itself owns the main thread and
/// ROS-style spinning is driven by the Cyclone backend's worker.
pub fn run<F>(config: Config, app: F) -> !
where
    F: FnOnce(&Config),
{
    init_hardware();
    app(&config);
    loop {
        // The Zephyr scheduler keeps Cyclone's RX threads + the user's
        // executor alive — `run` parks the calling thread.
        unsafe extern "C" {
            fn k_sleep_ms(ms: i32);
        }
        unsafe { k_sleep_ms(1000) };
    }
}

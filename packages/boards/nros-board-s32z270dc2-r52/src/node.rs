//! Board lifecycle.
//!
//! Zephyr brings the system up: boot, MPU, ENETC, network stack. By
//! the time `run` is called all of that is already done — `init_hardware`
//! only ensures Zephyr's linker pulls in the platform crate's FFI
//! shims (clock, sleep, RNG).

use crate::Config;

pub fn init_hardware() {
    // Pull the platform C ABI provider into the final link map even
    // under LTO. (Phase 121.3 — formerly via `nros-platform-zephyr`.)
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

pub fn run<F>(config: Config, app: F) -> !
where
    F: FnOnce(&Config),
{
    init_hardware();
    app(&config);
    loop {
        unsafe extern "C" {
            fn k_sleep_ms(ms: i32);
        }
        unsafe { k_sleep_ms(1000) };
    }
}

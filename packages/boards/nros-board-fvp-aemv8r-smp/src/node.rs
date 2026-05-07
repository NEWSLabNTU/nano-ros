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
    // Reference a public constant from `nros-platform-zephyr` so the
    // crate gets pulled into the final link map even when LTO is on.
    let _ = nros_platform_zephyr::NET_SOCKET_SIZE;
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

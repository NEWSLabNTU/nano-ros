//! Board lifecycle.
//!
//! Zephyr brings the system up: boot, MPU, ENETC, network stack. By
//! the time `run` is called all of that is already done — `init_hardware`
//! only ensures Zephyr's linker pulls in the platform crate's FFI
//! shims (clock, sleep, RNG).

use crate::Config;

pub fn init_hardware() {
    let _ = nros_platform_zephyr::NET_SOCKET_SIZE;
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

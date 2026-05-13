//! Global `critical_section::Impl` registration backed by the
//! canonical `nros_platform_critical_section_acquire` and
//! `nros_platform_critical_section_release` C symbols (Phase 121.9).
//!
//! Any binary that needs the global `critical_section::Impl` (dust-dds,
//! `nros-rmw-{xrce,zenoh}`, `embassy-sync`, …) links this crate. The
//! body delegates to the active platform's C port — Cortex-M PRIMASK
//! on FreeRTOS / bare-metal Cortex-M, CPSR I-bit on Cortex-R5,
//! `irq_lock` on Zephyr, `pthread_mutex` on POSIX / NuttX,
//! `tx_interrupt_control` on ThreadX, `portENTER_CRITICAL` on ESP-IDF.
//!
//! Drop-in replacement for the per-arch `critical_section` features on
//! `nros-platform-{freertos,…}` that landed in Phase 97.1.cs / 100.1.
//! Single-line consumer integration: depend on this crate and the
//! `set_impl!` happens at crate-load time.

#![no_std]

unsafe extern "C" {
    fn nros_platform_critical_section_acquire() -> u32;
    fn nros_platform_critical_section_release(token: u32);
}

struct PlatformCs;
critical_section::set_impl!(PlatformCs);

unsafe impl critical_section::Impl for PlatformCs {
    unsafe fn acquire() -> critical_section::RawRestoreState {
        unsafe { nros_platform_critical_section_acquire() }
    }

    unsafe fn release(token: critical_section::RawRestoreState) {
        unsafe { nros_platform_critical_section_release(token) }
    }
}

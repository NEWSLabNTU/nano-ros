//! phase-510 W5 — loan publish on FreeRTOS + Cortex-M3 + zenoh-pico, with the
//! payload copies counted ON TARGET (issue 0814).
//!
//! Issue 0814's measurement (2026-10-18) found the loan path is the only
//! zero-copy publish on zenoh — on the HOST, with an `LD_PRELOAD` `memcpy`
//! interposer. A bare-metal image has no preload, so this crate links with
//! `--wrap` (`build.rs`) and counts here instead: every `memcpy` and ARM EABI
//! `__aeabi_memcpy*` call goes through [`note`], which counts the calls that
//! copy exactly [`PAYLOAD_LEN`] bytes while a window is armed.
//!
//! A delivery test alone would pass whether or not the loan avoided the copy
//! (the lane 0814 rejected), so the publisher reports both numbers and the
//! host test asserts the difference.

#![no_std]

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use loan_copy_count_nros_selection::nros_board_mps2_an385_freertos::{BaseConfig, Config};

/// zenoh router on the host, reached through slirp's gateway. 7850 is in the
/// FreeRTOS window (7800+) and assigned to no baked cell (`alloc`'s bake
/// table), so this pair never shares a router with a concurrent `rtos_e2e`
/// lane; `tests/lending_freertos_e2e.rs` starts the router on the same port.
pub const LOCATOR: &str = "tcp/10.0.2.2:7850";
pub const DOMAIN: u32 = 0;
pub const TOPIC: &str = "/loan_copy";
pub const TYPE_NAME: &str = "std_msgs/msg/dds_/String_";
pub const TYPE_HASH: &str = "RIHS01_loan_copy_count_4242424242424242424242424242424242";

/// The payload size the counter attributes copies to. Large enough that no
/// header or key-expression copy has the same length, small enough for the
/// backend's lend arena.
pub const PAYLOAD_LEN: usize = 200;

/// Samples published per path.
pub const PER_PATH: u32 = 20;

/// The first payload byte names the path that sent the sample, so the
/// subscriber can count each path's deliveries separately.
pub const MARK_RAW: u8 = b'R';
pub const MARK_LOAN: u8 = b'L';

static ARMED: AtomicBool = AtomicBool::new(false);
static PAYLOAD_COPIES: AtomicU32 = AtomicU32::new(0);

/// Count the payload-sized copies made while `f` runs.
pub fn count_payload_copies<R>(f: impl FnOnce() -> R) -> (R, u32) {
    PAYLOAD_COPIES.store(0, Ordering::SeqCst);
    ARMED.store(true, Ordering::SeqCst);
    let r = f();
    ARMED.store(false, Ordering::SeqCst);
    (r, PAYLOAD_COPIES.load(Ordering::SeqCst))
}

#[inline(always)]
fn note(n: usize) {
    if n == PAYLOAD_LEN && ARMED.load(Ordering::Relaxed) {
        PAYLOAD_COPIES.fetch_add(1, Ordering::Relaxed);
    }
}

unsafe extern "C" {
    fn __real_memcpy(dst: *mut u8, src: *const u8, n: usize) -> *mut u8;
    fn __real___aeabi_memcpy(dst: *mut u8, src: *const u8, n: usize);
    fn __real___aeabi_memcpy4(dst: *mut u8, src: *const u8, n: usize);
    fn __real___aeabi_memcpy8(dst: *mut u8, src: *const u8, n: usize);
}

/// # Safety
/// The `memcpy` contract; forwarded unchanged to the original.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wrap_memcpy(dst: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    note(n);
    unsafe { __real_memcpy(dst, src, n) }
}

/// # Safety
/// The `__aeabi_memcpy` contract; forwarded unchanged to the original.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wrap___aeabi_memcpy(dst: *mut u8, src: *const u8, n: usize) {
    note(n);
    unsafe { __real___aeabi_memcpy(dst, src, n) }
}

/// # Safety
/// The `__aeabi_memcpy4` contract; forwarded unchanged to the original.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wrap___aeabi_memcpy4(dst: *mut u8, src: *const u8, n: usize) {
    note(n);
    unsafe { __real___aeabi_memcpy4(dst, src, n) }
}

/// # Safety
/// The `__aeabi_memcpy8` contract; forwarded unchanged to the original.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wrap___aeabi_memcpy8(dst: *mut u8, src: *const u8, n: usize) {
    note(n);
    unsafe { __real___aeabi_memcpy8(dst, src, n) }
}

fn config(host: u8) -> Config {
    Config {
        base: BaseConfig {
            mac: [0x02, 0x00, 0x00, 0x00, 0x00, host],
            ip: [10, 0, 2, host],
            netmask: [255, 255, 255, 0],
            gateway: [10, 0, 2, 2],
            zenoh_locator: LOCATOR,
            domain_id: DOMAIN,
        },
        ..Config::default()
    }
}

/// Distinct addresses from the wake-latency bench's 20/21.
pub fn publisher_config() -> Config {
    config(30)
}

pub fn subscriber_config() -> Config {
    config(31)
}

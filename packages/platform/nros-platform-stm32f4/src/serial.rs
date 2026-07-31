//! `PlatformSerial` impl for STM32F4.
//!
//! **Status (Phase 80.14.4b)**: surface-complete, uses a fn-pointer
//! vtable that the board crate (`nros-board-stm32f4`) populates at init
//! time. Same rationale as `nros-platform-mps2-an385::serial`:
//! the platform ZST doesn't own the UART peripheral — the board
//! crate does, because its USART driver lives there with the rest
//! of the STM32F4-specific bring-up (clock tree, pinmux, etc.).
//! The vtable shim keeps that ownership split while still offering
//! `<Stm32f4Platform as PlatformSerial>::*` to cross-crate consumers.
//!
//! `zpico-serial` continues to use its own `SerialPort` trait +
//! `register_port` path; this impl is additive for future RMWs.

use core::cell::UnsafeCell;

use crate::Stm32f4Platform;

/// Function-pointer vtable supplied by the board crate.
#[repr(C)]
pub struct SerialVTable {
    pub open: unsafe fn(path: *const u8) -> u8,
    pub close: unsafe fn(h: u8),
    pub configure: unsafe fn(h: u8, baudrate: u32) -> i8,
    pub read: unsafe fn(h: u8, buf: *mut u8, len: usize, timeout_ms: u32) -> usize,
    pub write: unsafe fn(h: u8, buf: *const u8, len: usize) -> usize,
}

#[repr(transparent)]
struct SharedCell<T>(UnsafeCell<T>);
// SAFETY: single-threaded, single-writer at init.
unsafe impl<T> Sync for SharedCell<T> {}

static VTABLE: SharedCell<Option<SerialVTable>> = SharedCell(UnsafeCell::new(None));

/// Register the serial vtable. Board crates call this during
/// `init_serial()`, before any consumer calls
/// `<Stm32f4Platform as PlatformSerial>::*`.
///
/// # Safety
///
/// Must not be called concurrently. Single-writer.
pub unsafe fn register_serial_vtable(v: SerialVTable) {
    unsafe {
        *VTABLE.0.get() = Some(v);
    }
}

#[inline]
fn vtable() -> Option<&'static SerialVTable> {
    unsafe { (*VTABLE.0.get()).as_ref() }
}

impl nros_platform_api::PlatformSerial for Stm32f4Platform {
    type Handle = u8;
    const INVALID: u8 = u8::MAX;

    fn is_valid(h: u8) -> bool {
        h != u8::MAX
    }

    fn open(path: *const u8) -> u8 {
        match vtable() {
            Some(v) => unsafe { (v.open)(path) },
            None => u8::MAX,
        }
    }

    fn close(h: u8) {
        if let Some(v) = vtable() {
            unsafe { (v.close)(h) }
        }
    }

    fn configure(h: u8, baudrate: u32) -> i8 {
        match vtable() {
            Some(v) => unsafe { (v.configure)(h, baudrate) },
            None => -1,
        }
    }

    fn read(h: u8, buf: *mut u8, len: usize, timeout_ms: u32) -> usize {
        match vtable() {
            Some(v) => unsafe { (v.read)(h, buf, len, timeout_ms) },
            None => usize::MAX,
        }
    }

    fn write(h: u8, buf: *const u8, len: usize) -> usize {
        match vtable() {
            Some(v) => unsafe { (v.write)(h, buf, len) },
            None => usize::MAX,
        }
    }
}

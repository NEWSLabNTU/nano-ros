//! `PlatformSerial` impl for MPS2-AN385.
//!
//! **Status (Phase 80.14.4b)**: surface-complete, uses a fn-pointer
//! vtable that the board crate (`nros-mps2-an385`) populates at init
//! time. The platform ZST doesn't own the UART peripheral itself —
//! the board crate does, because the driver (`cmsdk-uart`) lives
//! there along with every other board-specific detail (pinmux, clock
//! tree, etc.). A vtable lets us keep that ownership split while
//! still offering `<Mps2An385Platform as PlatformSerial>::*` to
//! cross-crate consumers.
//!
//! The `zpico-serial` stack continues to use its own `SerialPort`
//! trait + `register_port` path; this trait impl is strictly
//! additive for future RMWs that want serial on bare-metal.
//!
//! **Wiring**: in the board crate's `init_serial()`, call
//! [`register_serial_vtable`] with fn pointers that dispatch to the
//! board-owned UART driver. The vtable registers globally; there is
//! exactly one MPS2-AN385 board per firmware.

use core::cell::UnsafeCell;

use crate::Mps2An385Platform;

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
// SAFETY: access is single-threaded on bare-metal; the vtable is
// written exactly once during `init_serial` before any UART I/O.
unsafe impl<T> Sync for SharedCell<T> {}

static VTABLE: SharedCell<Option<SerialVTable>> = SharedCell(UnsafeCell::new(None));

/// Register the serial vtable. Board crates call this during
/// `init_serial()`, before any consumer calls
/// `<Mps2An385Platform as PlatformSerial>::*`.
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
    // SAFETY: single-threaded read; `register_serial_vtable` writes
    // this cell exactly once before any caller runs.
    unsafe { (*VTABLE.0.get()).as_ref() }
}

impl nros_platform_api::PlatformSerial for Mps2An385Platform {
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

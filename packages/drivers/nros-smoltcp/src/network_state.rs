//! Shared `static` holder for the `(Interface, SocketSet, Device)` triple
//! that every bare-metal board crate needs for its `smoltcp_network_poll`
//! FFI callback.
//!
//! Previously each board (MPS2-AN385, ESP32, ESP32-QEMU, STM32F4) kept
//! its own `static mut GLOBAL_IFACE` / `GLOBAL_SOCKETS` / `GLOBAL_DEVICE`
//! trio with hand-rolled `set_network_state` / `clear_network_state` /
//! `smoltcp_network_poll` functions — four near-identical 63-line files.
//! [`NetworkState<D>`] collapses them to a `const` constructor plus
//! `set` / `clear` / `poll` methods, using [`AtomicPtr`] instead of
//! `static mut` so callers never touch raw unsafe globals directly.
//!
//! Boards whose `Device` implementation only exists for `&mut T` (e.g.
//! STM32F4's `EthernetDMA`) go through [`NetworkState::poll_via_ref`]
//! instead.

use core::sync::atomic::{AtomicPtr, Ordering};

use smoltcp::iface::{Interface, SocketSet};
use smoltcp::phy::Device;

use crate::SmoltcpBridge;

/// Static holder for the poll-callback triple.
///
/// Each board crate constructs one of these as a `static` with a `const`
/// constructor, then fills it in during node init via [`set`][Self::set].
/// The [`poll`][Self::poll] / [`poll_via_ref`][Self::poll_via_ref]
/// methods are intended to be called from the board's `#[unsafe(no_mangle)]
/// pub unsafe extern "C" fn smoltcp_network_poll()` wrapper.
pub struct NetworkState<D: 'static> {
    iface: AtomicPtr<Interface>,
    sockets: AtomicPtr<SocketSet<'static>>,
    device: AtomicPtr<D>,
}

impl<D: 'static> NetworkState<D> {
    /// Construct an empty `NetworkState`. Usable in `static` context.
    pub const fn new() -> Self {
        Self {
            iface: AtomicPtr::new(core::ptr::null_mut()),
            sockets: AtomicPtr::new(core::ptr::null_mut()),
            device: AtomicPtr::new(core::ptr::null_mut()),
        }
    }

    /// Install the board's `Interface`, `SocketSet` and `Device` pointers.
    ///
    /// # Safety
    /// Every pointer must remain valid (i.e. the `Interface` / `SocketSet` /
    /// `Device` instances must stay live) until [`clear`][Self::clear] is
    /// called or the program exits. Callers must also ensure no other code
    /// mutates these instances while a poll is in flight.
    pub unsafe fn set(
        &self,
        iface: *mut Interface,
        sockets: *mut SocketSet<'static>,
        device: *mut D,
    ) {
        self.iface.store(iface, Ordering::Release);
        self.sockets.store(sockets, Ordering::Release);
        self.device.store(device, Ordering::Release);
    }

    /// Clear all three pointers. Subsequent `poll` calls short-circuit.
    ///
    /// # Safety
    /// Must only be called once the node has finished using the network
    /// stack — the `Interface` / `SocketSet` / `Device` instances can then
    /// be dropped safely.
    pub unsafe fn clear(&self) {
        self.iface.store(core::ptr::null_mut(), Ordering::Release);
        self.sockets.store(core::ptr::null_mut(), Ordering::Release);
        self.device.store(core::ptr::null_mut(), Ordering::Release);
    }

    /// Drive one smoltcp poll cycle. No-op if any pointer is null.
    ///
    /// Use this when `D: Device` — e.g. `Lan9118`, `OpenEth`, `WifiDevice`.
    ///
    /// # Safety
    /// See [`set`][Self::set] — the installed pointers must still be valid.
    pub unsafe fn poll(&self)
    where
        D: Device,
    {
        let iface = self.iface.load(Ordering::Acquire);
        let sockets = self.sockets.load(Ordering::Acquire);
        let device = self.device.load(Ordering::Acquire);
        if iface.is_null() || sockets.is_null() || device.is_null() {
            return;
        }
        unsafe {
            SmoltcpBridge::poll(&mut *iface, &mut *device, &mut *sockets);
        }
    }

    /// Drive one smoltcp poll cycle for devices where `Device` is only
    /// implemented on `&mut D` (e.g. STM32F4's `EthernetDMA`).
    ///
    /// # Safety
    /// See [`set`][Self::set].
    pub unsafe fn poll_via_ref(&self)
    where
        for<'a> &'a mut D: Device,
    {
        let iface = self.iface.load(Ordering::Acquire);
        let sockets = self.sockets.load(Ordering::Acquire);
        let device = self.device.load(Ordering::Acquire);
        if iface.is_null() || sockets.is_null() || device.is_null() {
            return;
        }
        unsafe {
            let mut device_ref = &mut *device;
            SmoltcpBridge::poll(&mut *iface, &mut device_ref, &mut *sockets);
        }
    }
}

impl<D: 'static> Default for NetworkState<D> {
    fn default() -> Self {
        Self::new()
    }
}

// `AtomicPtr<T>` is `Sync` unconditionally; the `Device` is not accessed
// concurrently (poll runs from one task). The static holder is safe to
// share across threads because the callers must still respect the unsafe
// contract on `set` / `clear`.
unsafe impl<D: 'static> Sync for NetworkState<D> {}

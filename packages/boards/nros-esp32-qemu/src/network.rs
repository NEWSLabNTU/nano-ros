//! Network poll callback and global state for ESP32-C3 QEMU (OpenEth).
//!
//! Thin wrapper around [`nros_smoltcp::NetworkState`].

use nros_smoltcp::NetworkState;
use openeth_smoltcp::OpenEth;
use smoltcp::iface::{Interface, SocketSet};

static NETWORK_STATE: NetworkState<OpenEth> = NetworkState::new();

/// Set the network state pointers (called by nros-esp32-qemu during node init).
///
/// # Safety
/// The pointers must remain valid for the lifetime of the node.
pub unsafe fn set_network_state(
    iface: *mut Interface,
    sockets: *mut SocketSet<'static>,
    device: *mut (),
) {
    unsafe { NETWORK_STATE.set(iface, sockets, device as *mut OpenEth) }
}

/// Clear network state (called by nros-esp32-qemu on node drop).
///
/// # Safety
/// Must only be called after the node is done using the network.
pub unsafe fn clear_network_state() {
    unsafe { NETWORK_STATE.clear() }
}

/// Network poll callback called by the transport crate's smoltcp_poll().
///
/// In addition to driving smoltcp, this toggles the OpenETH MODER register's
/// RXEN bit every N calls. QEMU's `open_eth` model only flushes queued
/// ingress packets when RXEN *transitions* from 0→1
/// (see `open_eth_moder_host_write` → `open_eth_notify_can_receive`). If the
/// guest busy-loops, slirp-generated packets (SYN-ACK, keep-alives) can sit
/// queued indefinitely without the transition. Toggling RXEN off→on forces
/// a flush; it is cheap (two MMIO writes per poll batch) and safe because
/// the ring descriptors are unaffected.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn smoltcp_network_poll() {
    unsafe {
        use core::cell::UnsafeCell;
        struct S(UnsafeCell<u32>);
        unsafe impl Sync for S {}
        static CNT: S = S(UnsafeCell::new(0));
        let c = CNT.0.get();
        *c = c.read().wrapping_add(1);
        if *c % 8 == 0 {
            let moder_addr = 0x600C_D000usize as *mut u32;
            let cur = core::ptr::read_volatile(moder_addr);
            // Drop RXEN, then restore — triggers qemu_flush_queued_packets().
            core::ptr::write_volatile(moder_addr, cur & !0x1);
            core::ptr::write_volatile(moder_addr, cur);
        }
        NETWORK_STATE.poll();
    }
}

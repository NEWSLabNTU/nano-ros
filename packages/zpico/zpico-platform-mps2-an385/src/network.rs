//! Network poll callback and global state for MPS2-AN385
//!
//! Provides the `smoltcp_network_poll()` FFI callback invoked by
//! zpico-smoltcp during network operations. The board crate (nros-mps2-an385)
//! calls `set_network_state()` during init to populate the globals.

use core::ptr;

use lan9118_smoltcp::Lan9118;
use smoltcp::iface::{Interface, SocketSet};
use zpico_smoltcp::SmoltcpBridge;

// Global state for poll callback
static mut GLOBAL_IFACE: *mut Interface = ptr::null_mut();
static mut GLOBAL_SOCKETS: *mut SocketSet<'static> = ptr::null_mut();
static mut GLOBAL_DEVICE: *mut () = ptr::null_mut();

/// Set the network state pointers (called by nros-mps2-an385 during node init)
///
/// # Safety
///
/// The pointers must remain valid for the lifetime of the node.
pub unsafe fn set_network_state(
    iface: *mut Interface,
    sockets: *mut SocketSet<'static>,
    device: *mut (),
) {
    unsafe {
        GLOBAL_IFACE = iface;
        GLOBAL_SOCKETS = sockets;
        GLOBAL_DEVICE = device;
    }
}

/// Clear network state (called by nros-mps2-an385 on node drop)
///
/// # Safety
///
/// Must only be called after the node is done using the network.
pub unsafe fn clear_network_state() {
    unsafe {
        GLOBAL_IFACE = ptr::null_mut();
        GLOBAL_SOCKETS = ptr::null_mut();
        GLOBAL_DEVICE = ptr::null_mut();
    }
}

/// Network poll callback called by the transport crate's smoltcp_poll()
#[unsafe(no_mangle)]
pub unsafe extern "C" fn smoltcp_network_poll() {
    unsafe {
        if GLOBAL_IFACE.is_null() || GLOBAL_SOCKETS.is_null() || GLOBAL_DEVICE.is_null() {
            return;
        }

        let eth = &mut *(GLOBAL_DEVICE as *mut Lan9118);
        let iface = &mut *GLOBAL_IFACE;
        let sockets = &mut *GLOBAL_SOCKETS;

        SmoltcpBridge::poll(iface, eth, sockets);
        crate::clock::advance_clock_ms(1);
    }
}

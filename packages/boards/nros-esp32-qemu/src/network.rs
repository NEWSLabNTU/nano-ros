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
#[unsafe(no_mangle)]
pub unsafe extern "C" fn smoltcp_network_poll() {
    unsafe { NETWORK_STATE.poll() }
}

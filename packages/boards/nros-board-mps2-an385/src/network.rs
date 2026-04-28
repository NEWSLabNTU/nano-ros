//! Network poll callback and global state for MPS2-AN385 (LAN9118).
//!
//! Thin wrapper around [`nros_smoltcp::NetworkState`].

use lan9118_smoltcp::Lan9118;
use nros_smoltcp::NetworkState;
use smoltcp::iface::{Interface, SocketSet};

static NETWORK_STATE: NetworkState<Lan9118> = NetworkState::new();

/// Set the network state pointers (called during node init).
///
/// # Safety
/// The pointers must remain valid for the lifetime of the node.
pub unsafe fn set_network_state(
    iface: *mut Interface,
    sockets: *mut SocketSet<'static>,
    device: *mut (),
) {
    unsafe { NETWORK_STATE.set(iface, sockets, device as *mut Lan9118) }
}

/// Clear network state (called on node drop).
///
/// # Safety
/// Must only be called after the node is done using the network.
pub unsafe fn clear_network_state() {
    unsafe { NETWORK_STATE.clear() }
}

/// Network poll callback called by nros-smoltcp's `smoltcp_poll()`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn smoltcp_network_poll() {
    unsafe { NETWORK_STATE.poll() }
}

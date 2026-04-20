//! Network poll callback and global state for ESP32-C3 WiFi.
//!
//! Thin wrapper around [`nros_smoltcp::NetworkState`]: holds the
//! `(Interface, SocketSet, WifiDevice)` triple in a single `static` with
//! atomic pointers and dispatches to `SmoltcpBridge::poll` on each tick.

use esp_radio::wifi::WifiDevice;
use nros_smoltcp::NetworkState;
use smoltcp::iface::{Interface, SocketSet};

static NETWORK_STATE: NetworkState<WifiDevice> = NetworkState::new();

/// Set the network state pointers (called by nros-esp32 during node init).
///
/// # Safety
/// The pointers must remain valid for the lifetime of the node.
pub unsafe fn set_network_state(
    iface: *mut Interface,
    sockets: *mut SocketSet<'static>,
    device: *mut (),
) {
    unsafe { NETWORK_STATE.set(iface, sockets, device as *mut WifiDevice) }
}

/// Clear network state (called by nros-esp32 on node drop).
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

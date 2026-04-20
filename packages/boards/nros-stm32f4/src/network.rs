//! Network poll callback and global state for STM32F4.
//!
//! Thin wrapper around [`nros_smoltcp::NetworkState`], with the STM32F4
//! quirk that `stm32_eth` only implements `smoltcp::phy::Device` for
//! `&mut EthernetDMA`, not `EthernetDMA` directly — so we dispatch via
//! [`NetworkState::poll_via_ref`] instead of [`NetworkState::poll`].

use nros_smoltcp::NetworkState;
use smoltcp::iface::{Interface, SocketSet};
use stm32_eth::dma::EthernetDMA;

static NETWORK_STATE: NetworkState<EthernetDMA<'static, 'static>> = NetworkState::new();

/// Set the network state pointers (called by nros-stm32f4 during node init).
///
/// # Safety
/// The pointers must remain valid for the lifetime of the node.
pub unsafe fn set_network_state(
    iface: *mut Interface,
    sockets: *mut SocketSet<'static>,
    dma: *mut EthernetDMA<'static, 'static>,
) {
    unsafe { NETWORK_STATE.set(iface, sockets, dma) }
}

/// Clear network state (called by nros-stm32f4 on node drop).
///
/// # Safety
/// Must only be called after the node is done using the network.
pub unsafe fn clear_network_state() {
    unsafe { NETWORK_STATE.clear() }
}

/// Network poll callback called by the transport crate's smoltcp_poll().
#[unsafe(no_mangle)]
pub unsafe extern "C" fn smoltcp_network_poll() {
    unsafe {
        NETWORK_STATE.poll_via_ref();
        nros_platform_stm32f4::clock::update_from_dwt();
    }
}

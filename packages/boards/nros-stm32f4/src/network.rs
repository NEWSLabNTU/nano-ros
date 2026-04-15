//! Network poll callback and global state for STM32F4
//!
//! Provides the `smoltcp_network_poll()` FFI callback invoked by
//! nros-smoltcp during network operations. The board crate (nros-stm32f4)
//! calls `set_network_state()` during init to populate the globals.

use core::ptr;

use smoltcp::iface::{Interface, SocketSet};
use stm32_eth::dma::EthernetDMA;
use nros_smoltcp::SmoltcpBridge;

// Global state for poll callback
static mut GLOBAL_IFACE: *mut Interface = ptr::null_mut();
static mut GLOBAL_SOCKETS: *mut SocketSet<'static> = ptr::null_mut();
static mut GLOBAL_DMA: *mut EthernetDMA<'static, 'static> = ptr::null_mut();

/// Set the network state pointers (called by nros-stm32f4 during node init)
///
/// # Safety
///
/// The pointers must remain valid for the lifetime of the node.
pub unsafe fn set_network_state(
    iface: *mut Interface,
    sockets: *mut SocketSet<'static>,
    dma: *mut EthernetDMA<'static, 'static>,
) {
    unsafe {
        GLOBAL_IFACE = iface;
        GLOBAL_SOCKETS = sockets;
        GLOBAL_DMA = dma;
    }
}

/// Clear network state (called by nros-stm32f4 on node drop)
///
/// # Safety
///
/// Must only be called after the node is done using the network.
pub unsafe fn clear_network_state() {
    unsafe {
        GLOBAL_IFACE = ptr::null_mut();
        GLOBAL_SOCKETS = ptr::null_mut();
        GLOBAL_DMA = ptr::null_mut();
    }
}

/// Network poll callback called by the transport crate's smoltcp_poll()
#[unsafe(no_mangle)]
pub unsafe extern "C" fn smoltcp_network_poll() {
    unsafe {
        if GLOBAL_IFACE.is_null() || GLOBAL_SOCKETS.is_null() || GLOBAL_DMA.is_null() {
            return;
        }

        let dma = &mut *GLOBAL_DMA;
        let iface = &mut *GLOBAL_IFACE;
        let sockets = &mut *GLOBAL_SOCKETS;

        // stm32-eth implements Device for &mut EthernetDMA, not EthernetDMA directly
        let mut dma_ref = dma;
        SmoltcpBridge::poll(iface, &mut dma_ref, sockets);
        nros_platform_stm32f4::clock::update_from_dwt();
    }
}

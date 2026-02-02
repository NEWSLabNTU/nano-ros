//! Static buffer management for smoltcp sockets
//!
//! This module provides pre-allocated buffers for TCP sockets, avoiding
//! dynamic memory allocation on bare-metal systems.

/// Maximum number of sockets supported
pub const MAX_SOCKETS: usize = 4;

/// TCP socket buffer size (2KB per direction)
pub const TCP_BUFFER_SIZE: usize = 2048;

/// Socket storage for smoltcp interface
pub static mut SOCKET_STORAGE: [smoltcp::iface::SocketStorage<'static>; MAX_SOCKETS] =
    [smoltcp::iface::SocketStorage::EMPTY; MAX_SOCKETS];

// TCP buffers for each socket (RX and TX)
pub static mut TCP_RX_BUFFER_0: [u8; TCP_BUFFER_SIZE] = [0u8; TCP_BUFFER_SIZE];
pub static mut TCP_TX_BUFFER_0: [u8; TCP_BUFFER_SIZE] = [0u8; TCP_BUFFER_SIZE];
pub static mut TCP_RX_BUFFER_1: [u8; TCP_BUFFER_SIZE] = [0u8; TCP_BUFFER_SIZE];
pub static mut TCP_TX_BUFFER_1: [u8; TCP_BUFFER_SIZE] = [0u8; TCP_BUFFER_SIZE];
pub static mut TCP_RX_BUFFER_2: [u8; TCP_BUFFER_SIZE] = [0u8; TCP_BUFFER_SIZE];
pub static mut TCP_TX_BUFFER_2: [u8; TCP_BUFFER_SIZE] = [0u8; TCP_BUFFER_SIZE];
pub static mut TCP_RX_BUFFER_3: [u8; TCP_BUFFER_SIZE] = [0u8; TCP_BUFFER_SIZE];
pub static mut TCP_TX_BUFFER_3: [u8; TCP_BUFFER_SIZE] = [0u8; TCP_BUFFER_SIZE];

/// Get a pair of static TCP buffers for the given socket index
///
/// # Safety
///
/// - Each index should only be used once
/// - The returned references have 'static lifetime and must not be used concurrently
///
/// # Panics
///
/// Panics if index >= MAX_SOCKETS
#[allow(static_mut_refs)]
pub unsafe fn get_tcp_buffers(index: usize) -> (&'static mut [u8], &'static mut [u8]) {
    match index {
        0 => (&mut TCP_RX_BUFFER_0, &mut TCP_TX_BUFFER_0),
        1 => (&mut TCP_RX_BUFFER_1, &mut TCP_TX_BUFFER_1),
        2 => (&mut TCP_RX_BUFFER_2, &mut TCP_TX_BUFFER_2),
        3 => (&mut TCP_RX_BUFFER_3, &mut TCP_TX_BUFFER_3),
        _ => panic!("Socket index out of range"),
    }
}

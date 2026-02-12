//! smoltcp TCP transport for nano-ros
//!
//! Provides [`SmoltcpBridge`] for managing TCP socket state and data transfer
//! between zenoh-pico and smoltcp, plus Rust implementations of the zenoh-pico
//! TCP platform symbols (`_z_open_tcp`, `_z_read_tcp`, etc.).
//!
//! # Usage
//!
//! Platform crates (BSPs) depend on this crate and wire it to their hardware:
//!
//! 1. Call [`SmoltcpBridge::init()`] at startup
//! 2. Create smoltcp sockets using [`create_and_register_sockets()`]
//! 3. Register a poll callback via [`set_poll_callback()`]
//! 4. The callback should call [`SmoltcpBridge::poll()`] with the platform's
//!    `Interface`, `Device`, and `SocketSet`

#![no_std]

mod bridge;
mod tcp;

pub use bridge::{SmoltcpBridge, MAX_SOCKETS, SOCKET_BUFFER_SIZE};

// Re-export smoltcp types needed by platform crates
pub use smoltcp::iface::{Interface, SocketSet, SocketStorage};
pub use smoltcp::phy::Device;
pub use smoltcp::socket::tcp::{Socket as TcpSocket, SocketBuffer as TcpSocketBuffer};

/// Get pre-allocated socket storage for smoltcp's `SocketSet`.
///
/// # Safety
///
/// Must only be called once. The returned reference has `'static` lifetime
/// and must not be used concurrently.
#[allow(static_mut_refs)]
pub unsafe fn get_socket_storage() -> &'static mut [SocketStorage<'static>; MAX_SOCKETS] {
    static mut SOCKET_STORAGE: [SocketStorage<'static>; MAX_SOCKETS] =
        [SocketStorage::EMPTY; MAX_SOCKETS];
    unsafe { &mut SOCKET_STORAGE }
}

/// TCP buffer size constant for smoltcp socket creation.
pub const TCP_BUFFER_SIZE: usize = SOCKET_BUFFER_SIZE;

// Individual static TCP buffers for each socket (RX and TX).
// We use individual statics (not an array of arrays) so each can be
// borrowed independently with 'static lifetime.
static mut TCP_RX_BUFFER_0: [u8; TCP_BUFFER_SIZE] = [0u8; TCP_BUFFER_SIZE];
static mut TCP_TX_BUFFER_0: [u8; TCP_BUFFER_SIZE] = [0u8; TCP_BUFFER_SIZE];
static mut TCP_RX_BUFFER_1: [u8; TCP_BUFFER_SIZE] = [0u8; TCP_BUFFER_SIZE];
static mut TCP_TX_BUFFER_1: [u8; TCP_BUFFER_SIZE] = [0u8; TCP_BUFFER_SIZE];
static mut TCP_RX_BUFFER_2: [u8; TCP_BUFFER_SIZE] = [0u8; TCP_BUFFER_SIZE];
static mut TCP_TX_BUFFER_2: [u8; TCP_BUFFER_SIZE] = [0u8; TCP_BUFFER_SIZE];
static mut TCP_RX_BUFFER_3: [u8; TCP_BUFFER_SIZE] = [0u8; TCP_BUFFER_SIZE];
static mut TCP_TX_BUFFER_3: [u8; TCP_BUFFER_SIZE] = [0u8; TCP_BUFFER_SIZE];

/// Get a pair of static TCP buffers for the given socket index.
///
/// # Safety
///
/// - Each index should only be used once
/// - The returned references have `'static` lifetime and must not be used concurrently
///
/// # Panics
///
/// Panics if `index >= MAX_SOCKETS`
#[allow(static_mut_refs)]
pub unsafe fn get_tcp_buffers(index: usize) -> (&'static mut [u8], &'static mut [u8]) {
    unsafe {
        match index {
            0 => (&mut TCP_RX_BUFFER_0, &mut TCP_TX_BUFFER_0),
            1 => (&mut TCP_RX_BUFFER_1, &mut TCP_TX_BUFFER_1),
            2 => (&mut TCP_RX_BUFFER_2, &mut TCP_TX_BUFFER_2),
            3 => (&mut TCP_RX_BUFFER_3, &mut TCP_TX_BUFFER_3),
            _ => panic!("Socket index out of range"),
        }
    }
}

/// Create TCP sockets and register them with the bridge.
///
/// Creates `MAX_SOCKETS` TCP sockets in the given `SocketSet` and registers
/// each with [`SmoltcpBridge`]. This is the standard setup sequence for
/// platform crates.
///
/// # Safety
///
/// Must only be called once after [`SmoltcpBridge::init()`].
pub unsafe fn create_and_register_sockets(sockets: &mut SocketSet<'static>) {
    for i in 0..MAX_SOCKETS {
        let (rx_buf, tx_buf) = unsafe { get_tcp_buffers(i) };
        let rx = TcpSocketBuffer::new(&mut rx_buf[..]);
        let tx = TcpSocketBuffer::new(&mut tx_buf[..]);
        let tcp_socket = TcpSocket::new(rx, tx);
        let handle = sockets.add(tcp_socket);
        // SocketHandle is a newtype over usize — transmute to get the raw index
        let handle_raw: usize = unsafe { core::mem::transmute(handle) };
        SmoltcpBridge::register_socket(handle_raw);
    }
}

/// Set the poll callback function.
///
/// The callback is invoked by `smoltcp_poll()` (called from `z_sleep_ms` in
/// system.c) to pump the network stack. Platform crates register a callback
/// that calls `SmoltcpBridge::poll()` with their owned resources.
pub fn set_poll_callback(callback: unsafe extern "C" fn()) {
    bridge::set_poll_callback_fn(callback);
}

/// FFI export: poll the network stack via the registered callback.
///
/// Called from `system.c`'s `z_sleep_ms` implementation.
#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_poll() -> i32 {
    bridge::do_poll()
}

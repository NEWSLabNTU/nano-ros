//! SmoltcpZenohBridge - Connects zenoh-pico socket operations to smoltcp
//!
//! This module provides the FFI functions that zenoh-pico's C platform layer
//! calls to perform socket operations, bridging them to actual smoltcp sockets.
//!
//! This is hardware-agnostic and shared between QEMU and ESP32 BSPs.

use core::ptr;

use smoltcp::iface::{Interface, PollResult, SocketHandle, SocketSet};
use smoltcp::phy::Device;
use smoltcp::socket::tcp::{Socket as TcpSocket, State as TcpState};
use smoltcp::wire::{IpAddress, IpEndpoint, Ipv4Address};

use crate::clock;

// ============================================================================
// Configuration
// ============================================================================

/// Maximum number of sockets
pub const MAX_SOCKETS: usize = 4;

/// Socket buffer size
const SOCKET_BUFFER_SIZE: usize = 2048;

/// Next ephemeral port
static mut NEXT_EPHEMERAL_PORT: u16 = 49152;

// ============================================================================
// Socket State
// ============================================================================

/// State for a single socket
#[derive(Clone, Copy)]
struct SocketEntry {
    /// Socket is allocated to zenoh-pico
    allocated: bool,
    /// smoltcp socket handle (raw index, converted to SocketHandle when used)
    /// usize::MAX means no handle assigned
    handle_raw: usize,
    /// Remote IP address
    remote_ip: [u8; 4],
    /// Remote port
    remote_port: u16,
    /// Local port
    local_port: u16,
    /// Connection state (for zenoh-pico)
    connected: bool,
    /// RX buffer position and length
    rx_pos: usize,
    rx_len: usize,
    /// TX buffer position and length
    tx_pos: usize,
    tx_len: usize,
}

impl SocketEntry {
    /// Check if this entry has a valid socket handle
    fn has_handle(&self) -> bool {
        self.handle_raw != usize::MAX
    }

    /// Get the socket handle (panics if no handle assigned)
    fn handle(&self) -> SocketHandle {
        debug_assert!(self.has_handle());
        // Safe: SocketHandle is a newtype wrapper around usize with same layout
        unsafe { core::mem::transmute(self.handle_raw) }
    }
}

impl Default for SocketEntry {
    fn default() -> Self {
        Self {
            allocated: false,
            handle_raw: usize::MAX,
            remote_ip: [0; 4],
            remote_port: 0,
            local_port: 0,
            connected: false,
            rx_pos: 0,
            rx_len: 0,
            tx_pos: 0,
            tx_len: 0,
        }
    }
}

/// Global socket table
static mut SOCKET_TABLE: [SocketEntry; MAX_SOCKETS] = [SocketEntry {
    allocated: false,
    handle_raw: usize::MAX,
    remote_ip: [0; 4],
    remote_port: 0,
    local_port: 0,
    connected: false,
    rx_pos: 0,
    rx_len: 0,
    tx_pos: 0,
    tx_len: 0,
}; MAX_SOCKETS];

/// Socket RX/TX buffers
static mut SOCKET_RX_BUFFERS: [[u8; SOCKET_BUFFER_SIZE]; MAX_SOCKETS] =
    [[0u8; SOCKET_BUFFER_SIZE]; MAX_SOCKETS];
static mut SOCKET_TX_BUFFERS: [[u8; SOCKET_BUFFER_SIZE]; MAX_SOCKETS] =
    [[0u8; SOCKET_BUFFER_SIZE]; MAX_SOCKETS];

// ============================================================================
// Bridge State
// ============================================================================

/// Bridge state holder
struct BridgeState {
    initialized: bool,
}

static mut BRIDGE_STATE: BridgeState = BridgeState { initialized: false };

// ============================================================================
// Debug counters
// ============================================================================

static mut SOCKET_OPEN_COUNT: u32 = 0;
static mut SOCKET_CONNECT_COUNT: u32 = 0;
static mut SOCKET_SEND_COUNT: u32 = 0;
static mut SOCKET_RECV_COUNT: u32 = 0;
static mut SOCKET_BYTES_SENT: u32 = 0;
static mut SOCKET_BYTES_RECV: u32 = 0;
static mut SMOLTCP_TX_COUNT: u32 = 0;
static mut SMOLTCP_RX_COUNT: u32 = 0;
static mut SMOLTCP_TX_BYTES: u32 = 0;
static mut SMOLTCP_RX_BYTES: u32 = 0;
static mut IS_CONNECTED_CHECK_COUNT: u32 = 0;
static mut IS_CONNECTED_TRUE_COUNT: u32 = 0;
static mut SMOLTCP_POLL_COUNT: u32 = 0;

// ============================================================================
// SmoltcpZenohBridge
// ============================================================================

/// Bridge between zenoh-pico and smoltcp
///
/// This struct manages the integration between zenoh-pico's socket operations
/// and the smoltcp TCP/IP stack.
pub struct SmoltcpZenohBridge;

impl SmoltcpZenohBridge {
    /// Initialize the bridge
    ///
    /// Must be called before any socket operations.
    pub fn init() {
        unsafe {
            // Reset socket table
            let table = &raw mut SOCKET_TABLE;
            for i in 0..MAX_SOCKETS {
                (*table)[i] = SocketEntry::default();
            }
            BRIDGE_STATE.initialized = true;
        }
    }

    /// Poll the network interface and transfer data
    ///
    /// This must be called periodically to:
    /// 1. Poll the smoltcp interface
    /// 2. Transfer data between zenoh-pico buffers and smoltcp sockets
    ///
    /// Returns true if any network activity occurred.
    pub fn poll<D: Device>(iface: &mut Interface, device: &mut D, sockets: &mut SocketSet) -> bool {
        let timestamp = clock::now();

        // Poll the interface
        let activity = iface.poll(timestamp, device, sockets);

        // Process each active socket
        unsafe {
            let table = &raw mut SOCKET_TABLE;
            for idx in 0..MAX_SOCKETS {
                let entry = &mut (*table)[idx];
                if !entry.allocated {
                    continue;
                }

                if entry.has_handle() {
                    let handle = entry.handle();
                    let socket = sockets.get_mut::<TcpSocket>(handle);

                    // Check if we need to initiate a connection
                    if entry.remote_port > 0 && !entry.connected {
                        match socket.state() {
                            TcpState::Closed => {
                                // Initiate connection
                                let remote = IpEndpoint::new(
                                    IpAddress::Ipv4(Ipv4Address::new(
                                        entry.remote_ip[0],
                                        entry.remote_ip[1],
                                        entry.remote_ip[2],
                                        entry.remote_ip[3],
                                    )),
                                    entry.remote_port,
                                );
                                let _ = socket.connect(iface.context(), remote, entry.local_port);
                            }
                            TcpState::Established => {
                                entry.connected = true;
                            }
                            TcpState::SynSent | TcpState::SynReceived => {
                                // Connection in progress - nothing to do
                            }
                            _ => {
                                // Connection failed or in unexpected state
                            }
                        }
                    }

                    // Update connection state and transfer data
                    match socket.state() {
                        TcpState::Established => {
                            entry.connected = true;

                            // Transfer TX data to socket
                            if entry.tx_len > entry.tx_pos && socket.can_send() {
                                let tx_buf = &SOCKET_TX_BUFFERS[idx];
                                let data = &tx_buf[entry.tx_pos..entry.tx_len];
                                if let Ok(sent) = socket.send_slice(data) {
                                    entry.tx_pos += sent;
                                    SMOLTCP_TX_COUNT += 1;
                                    SMOLTCP_TX_BYTES += sent as u32;
                                    if entry.tx_pos >= entry.tx_len {
                                        entry.tx_pos = 0;
                                        entry.tx_len = 0;
                                    }
                                }
                            }

                            // Transfer RX data from socket
                            if socket.can_recv() {
                                // Compact RX buffer if needed
                                if entry.rx_pos > 0 {
                                    let remaining = entry.rx_len - entry.rx_pos;
                                    let rx_buf = &mut SOCKET_RX_BUFFERS[idx];
                                    rx_buf.copy_within(entry.rx_pos..entry.rx_len, 0);
                                    entry.rx_len = remaining;
                                    entry.rx_pos = 0;
                                }

                                // Read more data if space available
                                let available = SOCKET_BUFFER_SIZE - entry.rx_len;
                                if available > 0 {
                                    let rx_buf = &mut SOCKET_RX_BUFFERS[idx];
                                    if let Ok(received) =
                                        socket.recv_slice(&mut rx_buf[entry.rx_len..])
                                    {
                                        entry.rx_len += received;
                                        SMOLTCP_RX_COUNT += 1;
                                        SMOLTCP_RX_BYTES += received as u32;
                                    }
                                }
                            }
                        }
                        TcpState::Closed | TcpState::TimeWait => {
                            entry.connected = false;
                        }
                        _ => {}
                    }
                }
            }
        }

        // Convert PollResult to bool
        matches!(activity, PollResult::SocketStateChanged)
    }
}

// ============================================================================
// FFI Exports for zenoh-pico platform layer
// ============================================================================

/// Initialize the platform (idempotent - only runs once)
#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_init() -> i32 {
    unsafe {
        // Only initialize once - don't reset if already initialized
        if BRIDGE_STATE.initialized {
            return 0;
        }
    }
    SmoltcpZenohBridge::init();
    0
}

/// Cleanup the platform
#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_cleanup() {
    // Nothing to cleanup for static allocations
}

/// Register a pre-created smoltcp socket with the bridge
#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_register_socket(handle: usize) -> i32 {
    unsafe {
        let table = &raw mut SOCKET_TABLE;
        for i in 0..MAX_SOCKETS {
            let entry = &mut (*table)[i];
            if !entry.has_handle() {
                entry.allocated = false;
                entry.handle_raw = handle;
                entry.connected = false;
                entry.local_port = NEXT_EPHEMERAL_PORT;
                NEXT_EPHEMERAL_PORT = NEXT_EPHEMERAL_PORT.wrapping_add(1);
                if NEXT_EPHEMERAL_PORT < 49152 {
                    NEXT_EPHEMERAL_PORT = 49152;
                }
                entry.rx_pos = 0;
                entry.rx_len = 0;
                entry.tx_pos = 0;
                entry.tx_len = 0;
                return i as i32;
            }
        }
        -1
    }
}

/// Get the socket_open count (for debugging)
#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_get_socket_open_count() -> u32 {
    unsafe { SOCKET_OPEN_COUNT }
}

/// Allocate a new socket
#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_socket_open() -> i32 {
    unsafe {
        SOCKET_OPEN_COUNT += 1;

        let table = &raw mut SOCKET_TABLE;
        for i in 0..MAX_SOCKETS {
            let entry = &mut (*table)[i];
            if !entry.allocated && entry.has_handle() {
                entry.allocated = true;
                entry.connected = false;
                entry.remote_ip = [0; 4];
                entry.remote_port = 0;
                entry.rx_pos = 0;
                entry.rx_len = 0;
                entry.tx_pos = 0;
                entry.tx_len = 0;
                return i as i32;
            }
        }
        -1
    }
}

/// Get the socket_connect count (for debugging)
#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_get_socket_connect_count() -> u32 {
    unsafe { SOCKET_CONNECT_COUNT }
}

/// Initiate a TCP connection
#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn smoltcp_socket_connect(handle: i32, ip: *const u8, port: u16) -> i32 {
    unsafe {
        SOCKET_CONNECT_COUNT += 1;
    }

    if handle < 0 || handle >= MAX_SOCKETS as i32 || ip.is_null() {
        return -1;
    }

    unsafe {
        let entry = &mut SOCKET_TABLE[handle as usize];
        if !entry.allocated {
            return -1;
        }

        ptr::copy_nonoverlapping(ip, entry.remote_ip.as_mut_ptr(), 4);
        entry.remote_port = port;

        0
    }
}

/// Get is_connected check count (for debugging)
#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_get_is_connected_check_count() -> u32 {
    unsafe { IS_CONNECTED_CHECK_COUNT }
}

/// Get is_connected true count (for debugging)
#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_get_is_connected_true_count() -> u32 {
    unsafe { IS_CONNECTED_TRUE_COUNT }
}

/// Check if socket is connected
#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_socket_is_connected(handle: i32) -> i32 {
    unsafe {
        IS_CONNECTED_CHECK_COUNT += 1;
    }

    if handle < 0 || handle >= MAX_SOCKETS as i32 {
        return 0;
    }

    unsafe {
        let entry = &SOCKET_TABLE[handle as usize];
        if entry.allocated && entry.connected {
            IS_CONNECTED_TRUE_COUNT += 1;
            1
        } else {
            0
        }
    }
}

/// Close a socket
#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_socket_close(handle: i32) -> i32 {
    if handle < 0 || handle >= MAX_SOCKETS as i32 {
        return -1;
    }

    unsafe {
        let entry = &mut SOCKET_TABLE[handle as usize];
        entry.allocated = false;
        entry.connected = false;
        entry.remote_ip = [0; 4];
        entry.remote_port = 0;
        0
    }
}

/// Check if socket can receive data
#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_socket_can_recv(handle: i32) -> i32 {
    if handle < 0 || handle >= MAX_SOCKETS as i32 {
        return 0;
    }

    unsafe {
        let entry = &SOCKET_TABLE[handle as usize];
        if entry.allocated && entry.rx_len > entry.rx_pos {
            1
        } else {
            0
        }
    }
}

/// Check if socket can send data
#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_socket_can_send(handle: i32) -> i32 {
    if handle < 0 || handle >= MAX_SOCKETS as i32 {
        return 0;
    }

    unsafe {
        let entry = &SOCKET_TABLE[handle as usize];
        if entry.allocated && entry.connected && entry.tx_len < SOCKET_BUFFER_SIZE {
            1
        } else {
            0
        }
    }
}

/// Receive data from socket
#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn smoltcp_socket_recv(handle: i32, buf: *mut u8, len: usize) -> i32 {
    if handle < 0 || handle >= MAX_SOCKETS as i32 || buf.is_null() || len == 0 {
        return -1;
    }

    unsafe {
        SOCKET_RECV_COUNT += 1;

        let entry = &mut SOCKET_TABLE[handle as usize];
        if !entry.allocated {
            return -1;
        }

        let available = entry.rx_len.saturating_sub(entry.rx_pos);
        if available == 0 {
            return 0;
        }

        let to_copy = available.min(len);
        let rx_buf = &SOCKET_RX_BUFFERS[handle as usize];
        ptr::copy_nonoverlapping(rx_buf[entry.rx_pos..].as_ptr(), buf, to_copy);
        entry.rx_pos += to_copy;

        SOCKET_BYTES_RECV += to_copy as u32;

        if entry.rx_pos >= entry.rx_len {
            entry.rx_pos = 0;
            entry.rx_len = 0;
        }

        to_copy as i32
    }
}

/// Send data to socket
#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn smoltcp_socket_send(handle: i32, buf: *const u8, len: usize) -> i32 {
    if handle < 0 || handle >= MAX_SOCKETS as i32 || buf.is_null() || len == 0 {
        return -1;
    }

    unsafe {
        SOCKET_SEND_COUNT += 1;

        let entry = &mut SOCKET_TABLE[handle as usize];
        if !entry.allocated || !entry.connected {
            return -1;
        }

        let available = SOCKET_BUFFER_SIZE.saturating_sub(entry.tx_len);
        if available == 0 {
            return 0;
        }

        let to_copy = available.min(len);
        let tx_buf = &mut SOCKET_TX_BUFFERS[handle as usize];
        ptr::copy_nonoverlapping(buf, tx_buf[entry.tx_len..].as_mut_ptr(), to_copy);
        entry.tx_len += to_copy;

        SOCKET_BYTES_SENT += to_copy as u32;

        to_copy as i32
    }
}

/// Get socket remote address
#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn smoltcp_socket_get_remote(handle: i32, ip: *mut u8, port: *mut u16) -> i32 {
    if handle < 0 || handle >= MAX_SOCKETS as i32 {
        return -1;
    }

    unsafe {
        let entry = &SOCKET_TABLE[handle as usize];
        if !entry.allocated {
            return -1;
        }

        if !ip.is_null() {
            ptr::copy_nonoverlapping(entry.remote_ip.as_ptr(), ip, 4);
        }
        if !port.is_null() {
            *port = entry.remote_port;
        }

        0
    }
}

/// Set socket connected state
#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_socket_set_connected(handle: i32, connected: bool) {
    if handle >= 0 && (handle as usize) < MAX_SOCKETS {
        unsafe {
            SOCKET_TABLE[handle as usize].connected = connected;
        }
    }
}

/// Push received data into socket RX buffer
#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn smoltcp_socket_push_rx(handle: i32, data: *const u8, len: usize) -> i32 {
    if handle < 0 || handle >= MAX_SOCKETS as i32 || data.is_null() || len == 0 {
        return -1;
    }

    unsafe {
        let entry = &mut SOCKET_TABLE[handle as usize];
        if !entry.allocated {
            return -1;
        }

        if entry.rx_pos > 0 {
            let remaining = entry.rx_len - entry.rx_pos;
            let rx_buf = &mut SOCKET_RX_BUFFERS[handle as usize];
            rx_buf.copy_within(entry.rx_pos..entry.rx_len, 0);
            entry.rx_len = remaining;
            entry.rx_pos = 0;
        }

        let available = SOCKET_BUFFER_SIZE.saturating_sub(entry.rx_len);
        if available == 0 {
            return 0;
        }

        let to_copy = available.min(len);
        let rx_buf = &mut SOCKET_RX_BUFFERS[handle as usize];
        ptr::copy_nonoverlapping(data, rx_buf[entry.rx_len..].as_mut_ptr(), to_copy);
        entry.rx_len += to_copy;

        to_copy as i32
    }
}

/// Pop pending data from socket TX buffer
#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn smoltcp_socket_pop_tx(handle: i32, buf: *mut u8, max_len: usize) -> i32 {
    if handle < 0 || handle >= MAX_SOCKETS as i32 || buf.is_null() || max_len == 0 {
        return -1;
    }

    unsafe {
        let entry = &mut SOCKET_TABLE[handle as usize];
        if !entry.allocated {
            return -1;
        }

        let to_copy = entry.tx_len.min(max_len);
        if to_copy == 0 {
            return 0;
        }

        let tx_buf = &SOCKET_TX_BUFFERS[handle as usize];
        ptr::copy_nonoverlapping(tx_buf.as_ptr(), buf, to_copy);

        if to_copy < entry.tx_len {
            let remaining = entry.tx_len - to_copy;
            let tx_buf = &mut SOCKET_TX_BUFFERS[handle as usize];
            tx_buf.copy_within(to_copy..entry.tx_len, 0);
            entry.tx_len = remaining;
        } else {
            entry.tx_len = 0;
        }

        to_copy as i32
    }
}

// ============================================================================
// Debug FFI exports
// ============================================================================

#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_get_tcp_tx_count() -> u32 {
    unsafe { SMOLTCP_TX_COUNT }
}

#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_get_tcp_rx_count() -> u32 {
    unsafe { SMOLTCP_RX_COUNT }
}

#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_get_tcp_tx_bytes() -> u32 {
    unsafe { SMOLTCP_TX_BYTES }
}

#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_get_tcp_rx_bytes() -> u32 {
    unsafe { SMOLTCP_RX_BYTES }
}

#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_get_send_count() -> u32 {
    unsafe { SOCKET_SEND_COUNT }
}

#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_get_recv_count() -> u32 {
    unsafe { SOCKET_RECV_COUNT }
}

#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_get_bytes_sent() -> u32 {
    unsafe { SOCKET_BYTES_SENT }
}

#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_get_bytes_recv() -> u32 {
    unsafe { SOCKET_BYTES_RECV }
}

#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_get_poll_count() -> u32 {
    unsafe { SMOLTCP_POLL_COUNT }
}

// ============================================================================
// Memory Allocator (bump allocator for zenoh-pico)
// ============================================================================

const HEAP_SIZE: usize = 64 * 1024;
static mut HEAP_MEM: [u8; HEAP_SIZE] = [0u8; HEAP_SIZE];
static mut HEAP_POS: usize = 0;

#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_alloc(size: usize) -> *mut core::ffi::c_void {
    unsafe {
        let aligned_pos = (HEAP_POS + 7) & !7;
        let new_pos = aligned_pos + size;

        if new_pos > HEAP_SIZE {
            return ptr::null_mut();
        }

        HEAP_POS = new_pos;
        HEAP_MEM[aligned_pos..].as_mut_ptr() as *mut core::ffi::c_void
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_realloc(
    ptr: *mut core::ffi::c_void,
    size: usize,
) -> *mut core::ffi::c_void {
    if ptr.is_null() {
        return smoltcp_alloc(size);
    }
    if size == 0 {
        return core::ptr::null_mut();
    }
    smoltcp_alloc(size)
}

#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_free(_ptr: *mut core::ffi::c_void) {
    // No-op: bump allocator doesn't support deallocation
}

// ============================================================================
// Random Number Generator
// ============================================================================

static mut RNG_STATE: u32 = 0x12345678;

#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_seed_random(seed: u32) {
    unsafe {
        RNG_STATE = if seed == 0 { 0x12345678 } else { seed };
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_random_u32() -> u32 {
    unsafe {
        let mut x = RNG_STATE;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        RNG_STATE = x;
        x
    }
}

// ============================================================================
// Poll Callback
// ============================================================================

pub type PollCallbackFn = Option<unsafe extern "C" fn()>;
static mut POLL_CALLBACK: PollCallbackFn = None;

#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_set_poll_callback(callback: PollCallbackFn) {
    unsafe {
        POLL_CALLBACK = callback;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_has_poll_callback() -> i32 {
    unsafe {
        let cb = &raw const POLL_CALLBACK;
        if (*cb).is_some() { 1 } else { 0 }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_poll() -> i32 {
    unsafe {
        SMOLTCP_POLL_COUNT += 1;
        if let Some(callback) = POLL_CALLBACK {
            callback();
            0
        } else {
            -1
        }
    }
}

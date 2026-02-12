//! SmoltcpBridge - Socket table management and smoltcp data transfer
//!
//! Manages the mapping between zenoh-pico's socket handles (small integer
//! indices) and smoltcp's `SocketHandle` type. Provides methods for opening,
//! connecting, sending, and receiving on sockets, as well as the poll loop
//! that transfers data between zenoh-pico's staging buffers and smoltcp's
//! internal socket buffers.

use smoltcp::iface::{Interface, PollResult, SocketHandle, SocketSet};
use smoltcp::phy::Device;
use smoltcp::socket::tcp::{Socket as TcpSocket, State as TcpState};
use smoltcp::wire::{IpAddress, IpEndpoint, Ipv4Address};

// ============================================================================
// Configuration
// ============================================================================

/// Maximum number of concurrent sockets
pub const MAX_SOCKETS: usize = 4;

/// Per-socket staging buffer size (bytes)
pub const SOCKET_BUFFER_SIZE: usize = 2048;

/// Next ephemeral port counter
static mut NEXT_EPHEMERAL_PORT: u16 = 49152;

// ============================================================================
// Socket State
// ============================================================================

/// State for a single socket in the bridge table
#[derive(Clone, Copy)]
struct SocketEntry {
    /// Socket is allocated to zenoh-pico
    allocated: bool,
    /// smoltcp socket handle (raw index, converted to SocketHandle when used).
    /// `usize::MAX` means no handle assigned.
    handle_raw: usize,
    /// Remote IPv4 address
    remote_ip: [u8; 4],
    /// Remote port
    remote_port: u16,
    /// Local port
    local_port: u16,
    /// Connection state (for zenoh-pico)
    connected: bool,
    /// RX staging buffer position and length
    rx_pos: usize,
    rx_len: usize,
    /// TX staging buffer position and length
    tx_pos: usize,
    tx_len: usize,
}

impl SocketEntry {
    fn has_handle(&self) -> bool {
        self.handle_raw != usize::MAX
    }

    fn handle(&self) -> SocketHandle {
        debug_assert!(self.has_handle());
        // SocketHandle is a newtype wrapper around usize with same layout
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

/// Socket RX/TX staging buffers (used between zenoh-pico and smoltcp poll)
static mut SOCKET_RX_BUFFERS: [[u8; SOCKET_BUFFER_SIZE]; MAX_SOCKETS] =
    [[0u8; SOCKET_BUFFER_SIZE]; MAX_SOCKETS];
static mut SOCKET_TX_BUFFERS: [[u8; SOCKET_BUFFER_SIZE]; MAX_SOCKETS] =
    [[0u8; SOCKET_BUFFER_SIZE]; MAX_SOCKETS];

// ============================================================================
// Bridge State
// ============================================================================

struct BridgeState {
    initialized: bool,
}

static mut BRIDGE_STATE: BridgeState = BridgeState { initialized: false };

// ============================================================================
// Poll Callback
// ============================================================================

type PollCallbackFn = Option<unsafe extern "C" fn()>;
static mut POLL_CALLBACK: PollCallbackFn = None;
static mut SMOLTCP_POLL_COUNT: u32 = 0;

pub(crate) fn set_poll_callback_fn(callback: unsafe extern "C" fn()) {
    unsafe {
        POLL_CALLBACK = Some(callback);
    }
}

pub(crate) fn do_poll() -> i32 {
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

// ============================================================================
// Clock (resolved at link time from platform crate / system.c)
// ============================================================================

unsafe extern "C" {
    /// Millisecond clock — provided by the platform crate or system.c
    fn smoltcp_clock_now_ms() -> u64;
}

// ============================================================================
// SmoltcpBridge
// ============================================================================

/// Bridge between zenoh-pico socket operations and the smoltcp TCP/IP stack.
///
/// All methods are static — the bridge uses module-level statics for the
/// socket table and staging buffers. This avoids lifetime issues with
/// `'static` references in `no_std` contexts.
pub struct SmoltcpBridge;

impl SmoltcpBridge {
    /// Initialize the bridge. Must be called before any socket operations.
    pub fn init() {
        unsafe {
            let table = &raw mut SOCKET_TABLE;
            for i in 0..MAX_SOCKETS {
                (*table)[i] = SocketEntry::default();
            }
            BRIDGE_STATE.initialized = true;
        }
    }

    /// Register a pre-created smoltcp socket handle with the bridge.
    ///
    /// Returns the bridge slot index, or -1 if no slots available.
    pub fn register_socket(handle: usize) -> i32 {
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

    /// Poll the network interface and transfer data between staging buffers
    /// and smoltcp sockets.
    ///
    /// Must be called periodically. Returns `true` if any network activity
    /// occurred.
    pub fn poll<D: Device>(
        iface: &mut Interface,
        device: &mut D,
        sockets: &mut SocketSet,
    ) -> bool {
        let timestamp = smoltcp::time::Instant::from_millis(unsafe { smoltcp_clock_now_ms() } as i64);

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
                                // Connection in progress
                            }
                            _ => {
                                // Connection failed or unexpected state
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

        matches!(activity, PollResult::SocketStateChanged)
    }

    // ========================================================================
    // Internal socket operations (called from tcp.rs)
    // ========================================================================

    /// Allocate a socket from the table. Returns slot index or -1.
    pub(crate) fn socket_open() -> i32 {
        unsafe {
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

    /// Set the remote endpoint for a socket. Returns 0 on success, -1 on error.
    pub(crate) fn socket_connect(handle: i32, ip: &[u8; 4], port: u16) -> i32 {
        if handle < 0 || handle >= MAX_SOCKETS as i32 {
            return -1;
        }

        unsafe {
            let entry = &mut SOCKET_TABLE[handle as usize];
            if !entry.allocated {
                return -1;
            }

            entry.remote_ip = *ip;
            entry.remote_port = port;
            0
        }
    }

    /// Check if a socket is connected. Returns true if connected.
    pub(crate) fn socket_is_connected(handle: i32) -> bool {
        if handle < 0 || handle >= MAX_SOCKETS as i32 {
            return false;
        }

        unsafe {
            let entry = &SOCKET_TABLE[handle as usize];
            entry.allocated && entry.connected
        }
    }

    /// Close a socket. Returns 0 on success, -1 on error.
    pub(crate) fn socket_close(handle: i32) -> i32 {
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

    /// Check if socket has data available to receive.
    pub(crate) fn socket_can_recv(handle: i32) -> bool {
        if handle < 0 || handle >= MAX_SOCKETS as i32 {
            return false;
        }

        unsafe {
            let entry = &SOCKET_TABLE[handle as usize];
            entry.allocated && entry.rx_len > entry.rx_pos
        }
    }

    /// Check if socket can accept data for sending.
    pub(crate) fn socket_can_send(handle: i32) -> bool {
        if handle < 0 || handle >= MAX_SOCKETS as i32 {
            return false;
        }

        unsafe {
            let entry = &SOCKET_TABLE[handle as usize];
            entry.allocated && entry.connected && entry.tx_len < SOCKET_BUFFER_SIZE
        }
    }

    /// Receive data from the socket's staging buffer.
    /// Returns bytes copied, 0 if no data, or -1 on error.
    pub(crate) fn socket_recv(handle: i32, buf: &mut [u8]) -> i32 {
        if handle < 0 || handle >= MAX_SOCKETS as i32 || buf.is_empty() {
            return -1;
        }

        unsafe {
            let entry = &mut SOCKET_TABLE[handle as usize];
            if !entry.allocated {
                return -1;
            }

            let available = entry.rx_len.saturating_sub(entry.rx_pos);
            if available == 0 {
                return 0;
            }

            let to_copy = available.min(buf.len());
            let rx_buf = &SOCKET_RX_BUFFERS[handle as usize];
            buf[..to_copy].copy_from_slice(&rx_buf[entry.rx_pos..entry.rx_pos + to_copy]);
            entry.rx_pos += to_copy;

            if entry.rx_pos >= entry.rx_len {
                entry.rx_pos = 0;
                entry.rx_len = 0;
            }

            to_copy as i32
        }
    }

    /// Send data into the socket's TX staging buffer.
    /// Returns bytes copied, 0 if buffer full, or -1 on error.
    pub(crate) fn socket_send(handle: i32, data: &[u8]) -> i32 {
        if handle < 0 || handle >= MAX_SOCKETS as i32 || data.is_empty() {
            return -1;
        }

        unsafe {
            let entry = &mut SOCKET_TABLE[handle as usize];
            if !entry.allocated || !entry.connected {
                return -1;
            }

            let available = SOCKET_BUFFER_SIZE.saturating_sub(entry.tx_len);
            if available == 0 {
                return 0;
            }

            let to_copy = available.min(data.len());
            let tx_buf = &mut SOCKET_TX_BUFFERS[handle as usize];
            tx_buf[entry.tx_len..entry.tx_len + to_copy].copy_from_slice(&data[..to_copy]);
            entry.tx_len += to_copy;

            to_copy as i32
        }
    }

    /// Get current clock in milliseconds (delegates to platform).
    pub(crate) fn clock_now_ms() -> u64 {
        unsafe { smoltcp_clock_now_ms() }
    }

    /// Trigger a poll via the registered callback.
    pub(crate) fn poll_network() {
        do_poll();
    }
}

// ============================================================================
// FFI Exports — legacy symbols still needed by BSPs until migration (32.5)
// ============================================================================

#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_init() -> i32 {
    unsafe {
        if BRIDGE_STATE.initialized {
            return 0;
        }
    }
    SmoltcpBridge::init();
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_cleanup() {
    // Nothing to cleanup for static allocations
}

#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_register_socket(handle: usize) -> i32 {
    SmoltcpBridge::register_socket(handle)
}

#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_socket_open() -> i32 {
    SmoltcpBridge::socket_open()
}

#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn smoltcp_socket_connect(handle: i32, ip: *const u8, port: u16) -> i32 {
    if ip.is_null() {
        return -1;
    }
    let ip_bytes: [u8; 4] = unsafe { *(ip as *const [u8; 4]) };
    SmoltcpBridge::socket_connect(handle, &ip_bytes, port)
}

#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_socket_is_connected(handle: i32) -> i32 {
    if SmoltcpBridge::socket_is_connected(handle) {
        1
    } else {
        0
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_socket_close(handle: i32) -> i32 {
    SmoltcpBridge::socket_close(handle)
}

#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_socket_can_recv(handle: i32) -> i32 {
    if SmoltcpBridge::socket_can_recv(handle) {
        1
    } else {
        0
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_socket_can_send(handle: i32) -> i32 {
    if SmoltcpBridge::socket_can_send(handle) {
        1
    } else {
        0
    }
}

#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn smoltcp_socket_recv(handle: i32, buf: *mut u8, len: usize) -> i32 {
    if buf.is_null() || len == 0 {
        return -1;
    }
    let slice = unsafe { core::slice::from_raw_parts_mut(buf, len) };
    SmoltcpBridge::socket_recv(handle, slice)
}

#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn smoltcp_socket_send(handle: i32, buf: *const u8, len: usize) -> i32 {
    if buf.is_null() || len == 0 {
        return -1;
    }
    let slice = unsafe { core::slice::from_raw_parts(buf, len) };
    SmoltcpBridge::socket_send(handle, slice)
}

#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_has_poll_callback() -> i32 {
    unsafe {
        let cb = &raw const POLL_CALLBACK;
        if (*cb).is_some() { 1 } else { 0 }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_set_poll_callback(callback: PollCallbackFn) {
    unsafe {
        POLL_CALLBACK = callback;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_get_poll_count() -> u32 {
    unsafe { SMOLTCP_POLL_COUNT }
}

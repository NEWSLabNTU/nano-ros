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
use smoltcp::socket::udp::{Socket as UdpSocket, UdpMetadata};
use smoltcp::wire::{IpAddress, IpEndpoint, Ipv4Address};

// ============================================================================
// Configuration
// ============================================================================

pub use crate::config::{MAX_SOCKETS, MAX_UDP_SOCKETS, SOCKET_BUFFER_SIZE};
pub(crate) use crate::config::{CONNECT_TIMEOUT_MS, SOCKET_TIMEOUT_MS};

/// RFC 6056 ephemeral port range lower bound.
const EPHEMERAL_PORT_START: u16 = 49152;

/// RFC 6056 ephemeral port range size (49152..65535).
const EPHEMERAL_PORT_RANGE: u16 = 65535 - EPHEMERAL_PORT_START;

/// Next ephemeral port counter
static mut NEXT_EPHEMERAL_PORT: u16 = EPHEMERAL_PORT_START;

// ============================================================================
// Staging State (shared between TCP and UDP)
// ============================================================================

/// Staging buffer state for bidirectional data transfer between zenoh-pico
/// and smoltcp.
///
/// Tracks read/write cursors within external RX and TX buffers. Used by both
/// `SocketEntry` (TCP) and `UdpSocketEntry` (UDP).
#[derive(Clone, Copy)]
struct StagingState {
    /// RX: next byte to consume (read cursor)
    rx_pos: usize,
    /// RX: one past last valid byte (write cursor)
    rx_len: usize,
    /// TX: next byte to send (read cursor)
    tx_pos: usize,
    /// TX: one past last valid byte (write cursor)
    tx_len: usize,
}

impl StagingState {
    const INIT: Self = Self {
        rx_pos: 0,
        rx_len: 0,
        tx_pos: 0,
        tx_len: 0,
    };

    fn reset(&mut self) {
        *self = Self::INIT;
    }

    fn has_rx_data(&self) -> bool {
        self.rx_len > self.rx_pos
    }

    fn has_tx_space(&self) -> bool {
        self.tx_len < SOCKET_BUFFER_SIZE
    }

    fn has_tx_pending(&self) -> bool {
        self.tx_len > self.tx_pos
    }

    /// Pending TX data slice.
    fn tx_pending<'a>(&self, tx_buf: &'a [u8; SOCKET_BUFFER_SIZE]) -> &'a [u8] {
        &tx_buf[self.tx_pos..self.tx_len]
    }

    /// Available space at the end of the RX buffer.
    fn rx_space(&self) -> usize {
        SOCKET_BUFFER_SIZE - self.rx_len
    }

    /// Read from the RX staging buffer into `dst`.
    ///
    /// Returns bytes copied, or 0 if no data available.
    fn recv(&mut self, rx_buf: &[u8; SOCKET_BUFFER_SIZE], dst: &mut [u8]) -> i32 {
        let available = self.rx_len.saturating_sub(self.rx_pos);
        if available == 0 {
            return 0;
        }

        let to_copy = available.min(dst.len());
        dst[..to_copy].copy_from_slice(&rx_buf[self.rx_pos..self.rx_pos + to_copy]);
        self.rx_pos += to_copy;

        if self.rx_pos >= self.rx_len {
            self.rx_pos = 0;
            self.rx_len = 0;
        }

        to_copy as i32
    }

    /// Write `data` into the TX staging buffer.
    ///
    /// Returns bytes copied, or 0 if buffer full.
    fn send(&mut self, tx_buf: &mut [u8; SOCKET_BUFFER_SIZE], data: &[u8]) -> i32 {
        let available = SOCKET_BUFFER_SIZE.saturating_sub(self.tx_len);
        if available == 0 {
            return 0;
        }

        let to_copy = available.min(data.len());
        tx_buf[self.tx_len..self.tx_len + to_copy].copy_from_slice(&data[..to_copy]);
        self.tx_len += to_copy;

        to_copy as i32
    }

    /// Compact RX buffer by shifting unconsumed data to the front.
    fn compact_rx(&mut self, rx_buf: &mut [u8; SOCKET_BUFFER_SIZE]) {
        if self.rx_pos > 0 {
            let remaining = self.rx_len - self.rx_pos;
            rx_buf.copy_within(self.rx_pos..self.rx_len, 0);
            self.rx_len = remaining;
            self.rx_pos = 0;
        }
    }

    /// Record that the socket sent `sent` bytes from the TX buffer
    /// (incremental drain for TCP).
    fn advance_tx(&mut self, sent: usize) {
        self.tx_pos += sent;
        if self.tx_pos >= self.tx_len {
            self.tx_pos = 0;
            self.tx_len = 0;
        }
    }

    /// Reset TX after an atomic send (UDP sends entire datagram at once).
    fn reset_tx(&mut self) {
        self.tx_pos = 0;
        self.tx_len = 0;
    }

    /// Record that `received` bytes were written into rx_buf[rx_len..].
    fn advance_rx(&mut self, received: usize) {
        self.rx_len += received;
    }
}

// ============================================================================
// Ephemeral Port Allocation
// ============================================================================

/// Seed the ephemeral port counter to avoid 4-tuple collisions.
///
/// On bare-metal, smoltcp always starts from port 49152. If the host kernel
/// still has a TCP socket in FIN-WAIT or TIME-WAIT for the same 4-tuple
/// (src IP:port → dst IP:port) from a previous QEMU run, the new SYN is
/// dropped. Seeding with a value derived from the IP address or clock
/// randomizes the starting port to avoid this.
///
/// Call this after hardware init but before opening any sockets.
pub fn seed_ephemeral_port(seed: u16) {
    unsafe {
        NEXT_EPHEMERAL_PORT = EPHEMERAL_PORT_START + (seed % EPHEMERAL_PORT_RANGE);
    }
}

/// Allocate the next ephemeral port (RFC 6056, starting at 49152).
fn allocate_ephemeral_port() -> u16 {
    unsafe {
        let port = NEXT_EPHEMERAL_PORT;
        NEXT_EPHEMERAL_PORT = NEXT_EPHEMERAL_PORT.wrapping_add(1);
        if NEXT_EPHEMERAL_PORT < EPHEMERAL_PORT_START {
            NEXT_EPHEMERAL_PORT = EPHEMERAL_PORT_START;
        }
        port
    }
}

// ============================================================================
// Socket State
// ============================================================================

/// State for a single TCP socket in the bridge table
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
    /// RX/TX staging buffer state
    staging: StagingState,
}

impl SocketEntry {
    const INIT: Self = Self {
        allocated: false,
        handle_raw: usize::MAX,
        remote_ip: [0; 4],
        remote_port: 0,
        local_port: 0,
        connected: false,
        staging: StagingState::INIT,
    };

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
        Self::INIT
    }
}

/// Global socket table
static mut SOCKET_TABLE: [SocketEntry; MAX_SOCKETS] = [SocketEntry::INIT; MAX_SOCKETS];

/// Socket RX/TX staging buffers (used between zenoh-pico and smoltcp poll)
static mut SOCKET_RX_BUFFERS: [[u8; SOCKET_BUFFER_SIZE]; MAX_SOCKETS] =
    [[0u8; SOCKET_BUFFER_SIZE]; MAX_SOCKETS];
static mut SOCKET_TX_BUFFERS: [[u8; SOCKET_BUFFER_SIZE]; MAX_SOCKETS] =
    [[0u8; SOCKET_BUFFER_SIZE]; MAX_SOCKETS];

// ============================================================================
// UDP Socket State
// ============================================================================

/// State for a single UDP socket in the bridge table
#[derive(Clone, Copy)]
struct UdpSocketEntry {
    /// Socket is allocated to zenoh-pico
    allocated: bool,
    /// smoltcp socket handle (raw index, converted to SocketHandle when used).
    /// `usize::MAX` means no handle assigned.
    handle_raw: usize,
    /// Remote IPv4 address (for sendto)
    remote_ip: [u8; 4],
    /// Remote port (for sendto)
    remote_port: u16,
    /// Local port (bound port)
    local_port: u16,
    /// RX/TX staging buffer state
    staging: StagingState,
    /// TX target endpoint (per-packet, set by send)
    tx_remote_ip: [u8; 4],
    tx_remote_port: u16,
}

impl UdpSocketEntry {
    const INIT: Self = Self {
        allocated: false,
        handle_raw: usize::MAX,
        remote_ip: [0; 4],
        remote_port: 0,
        local_port: 0,
        staging: StagingState::INIT,
        tx_remote_ip: [0; 4],
        tx_remote_port: 0,
    };

    fn has_handle(&self) -> bool {
        self.handle_raw != usize::MAX
    }

    fn handle(&self) -> SocketHandle {
        debug_assert!(self.has_handle());
        unsafe { core::mem::transmute(self.handle_raw) }
    }
}

impl Default for UdpSocketEntry {
    fn default() -> Self {
        Self::INIT
    }
}

/// Global UDP socket table
static mut UDP_SOCKET_TABLE: [UdpSocketEntry; MAX_UDP_SOCKETS] =
    [UdpSocketEntry::INIT; MAX_UDP_SOCKETS];

/// UDP socket RX/TX staging buffers
static mut UDP_SOCKET_RX_BUFFERS: [[u8; SOCKET_BUFFER_SIZE]; MAX_UDP_SOCKETS] =
    [[0u8; SOCKET_BUFFER_SIZE]; MAX_UDP_SOCKETS];
static mut UDP_SOCKET_TX_BUFFERS: [[u8; SOCKET_BUFFER_SIZE]; MAX_UDP_SOCKETS] =
    [[0u8; SOCKET_BUFFER_SIZE]; MAX_UDP_SOCKETS];

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
            let udp_table = &raw mut UDP_SOCKET_TABLE;
            for i in 0..MAX_UDP_SOCKETS {
                (*udp_table)[i] = UdpSocketEntry::default();
            }
            BRIDGE_STATE.initialized = true;
        }
    }

    /// Register a pre-created smoltcp TCP socket handle with the bridge.
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
                    entry.local_port = allocate_ephemeral_port();
                    entry.staging.reset();
                    return i as i32;
                }
            }
            -1
        }
    }

    /// Register a pre-created smoltcp UDP socket handle with the bridge.
    ///
    /// Returns the bridge slot index, or -1 if no slots available.
    pub fn register_udp_socket(handle: usize) -> i32 {
        unsafe {
            let table = &raw mut UDP_SOCKET_TABLE;
            for i in 0..MAX_UDP_SOCKETS {
                let entry = &mut (*table)[i];
                if !entry.has_handle() {
                    entry.allocated = false;
                    entry.handle_raw = handle;
                    entry.local_port = allocate_ephemeral_port();
                    entry.staging.reset();
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

        // Process each active TCP socket
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

                            // Transfer TX data to socket (incremental)
                            if entry.staging.has_tx_pending() && socket.can_send() {
                                let tx_buf = &SOCKET_TX_BUFFERS[idx];
                                let data = entry.staging.tx_pending(tx_buf);
                                if let Ok(sent) = socket.send_slice(data) {
                                    entry.staging.advance_tx(sent);
                                }
                            }

                            // Transfer RX data from socket
                            if socket.can_recv() {
                                entry.staging.compact_rx(&mut SOCKET_RX_BUFFERS[idx]);

                                let space = entry.staging.rx_space();
                                if space > 0 {
                                    let rx_buf = &mut SOCKET_RX_BUFFERS[idx];
                                    if let Ok(received) =
                                        socket.recv_slice(&mut rx_buf[entry.staging.rx_len..])
                                    {
                                        entry.staging.advance_rx(received);
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

        // Process each active UDP socket
        unsafe {
            let table = &raw mut UDP_SOCKET_TABLE;
            for idx in 0..MAX_UDP_SOCKETS {
                let entry = &mut (*table)[idx];
                if !entry.allocated || !entry.has_handle() {
                    continue;
                }

                let handle = entry.handle();
                let socket = sockets.get_mut::<UdpSocket>(handle);

                // Auto-bind to ephemeral port if not yet bound
                if !socket.is_open() {
                    let _ = socket.bind(entry.local_port);
                }

                // Transfer TX data to socket (atomic — entire datagram)
                if entry.staging.has_tx_pending() && socket.can_send() {
                    let tx_buf = &UDP_SOCKET_TX_BUFFERS[idx];
                    let data = entry.staging.tx_pending(tx_buf);
                    let meta = UdpMetadata {
                        endpoint: IpEndpoint::new(
                            IpAddress::Ipv4(Ipv4Address::new(
                                entry.tx_remote_ip[0],
                                entry.tx_remote_ip[1],
                                entry.tx_remote_ip[2],
                                entry.tx_remote_ip[3],
                            )),
                            entry.tx_remote_port,
                        ),
                        local_address: None,
                        meta: Default::default(),
                    };
                    if socket.send_slice(data, meta).is_ok() {
                        entry.staging.reset_tx();
                    }
                }

                // Transfer RX data from socket
                if socket.can_recv() {
                    entry.staging.compact_rx(&mut UDP_SOCKET_RX_BUFFERS[idx]);

                    let space = entry.staging.rx_space();
                    if space > 0 {
                        let rx_buf = &mut UDP_SOCKET_RX_BUFFERS[idx];
                        if let Ok((received, _meta)) =
                            socket.recv_slice(&mut rx_buf[entry.staging.rx_len..])
                        {
                            entry.staging.advance_rx(received);
                        }
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
                    entry.staging.reset();
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
            entry.allocated && entry.staging.has_rx_data()
        }
    }

    /// Check if socket can accept data for sending.
    pub(crate) fn socket_can_send(handle: i32) -> bool {
        if handle < 0 || handle >= MAX_SOCKETS as i32 {
            return false;
        }

        unsafe {
            let entry = &SOCKET_TABLE[handle as usize];
            entry.allocated && entry.connected && entry.staging.has_tx_space()
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

            entry.staging.recv(&SOCKET_RX_BUFFERS[handle as usize], buf)
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

            entry.staging.send(&mut SOCKET_TX_BUFFERS[handle as usize], data)
        }
    }

    // ========================================================================
    // Internal UDP socket operations (called from udp.rs)
    // ========================================================================

    /// Allocate a UDP socket from the table. Returns slot index or -1.
    pub(crate) fn udp_socket_open() -> i32 {
        unsafe {
            let table = &raw mut UDP_SOCKET_TABLE;
            for i in 0..MAX_UDP_SOCKETS {
                let entry = &mut (*table)[i];
                if !entry.allocated && entry.has_handle() {
                    entry.allocated = true;
                    entry.remote_ip = [0; 4];
                    entry.remote_port = 0;
                    entry.staging.reset();
                    entry.tx_remote_ip = [0; 4];
                    entry.tx_remote_port = 0;
                    return i as i32;
                }
            }
            -1
        }
    }

    /// Set the remote endpoint for a UDP socket. Returns 0 on success, -1 on error.
    pub(crate) fn udp_socket_set_remote(handle: i32, ip: &[u8; 4], port: u16) -> i32 {
        if handle < 0 || handle >= MAX_UDP_SOCKETS as i32 {
            return -1;
        }

        unsafe {
            let entry = &mut UDP_SOCKET_TABLE[handle as usize];
            if !entry.allocated {
                return -1;
            }

            entry.remote_ip = *ip;
            entry.remote_port = port;
            0
        }
    }

    /// Get the local port for a UDP socket.
    #[allow(dead_code)]
    pub(crate) fn udp_socket_local_port(handle: i32) -> u16 {
        if handle < 0 || handle >= MAX_UDP_SOCKETS as i32 {
            return 0;
        }

        unsafe { UDP_SOCKET_TABLE[handle as usize].local_port }
    }

    /// Close a UDP socket. Returns 0 on success, -1 on error.
    pub(crate) fn udp_socket_close(handle: i32) -> i32 {
        if handle < 0 || handle >= MAX_UDP_SOCKETS as i32 {
            return -1;
        }

        unsafe {
            let entry = &mut UDP_SOCKET_TABLE[handle as usize];
            entry.allocated = false;
            entry.remote_ip = [0; 4];
            entry.remote_port = 0;
            entry.tx_remote_ip = [0; 4];
            entry.tx_remote_port = 0;
            0
        }
    }

    /// Check if UDP socket has data available to receive.
    pub(crate) fn udp_socket_can_recv(handle: i32) -> bool {
        if handle < 0 || handle >= MAX_UDP_SOCKETS as i32 {
            return false;
        }

        unsafe {
            let entry = &UDP_SOCKET_TABLE[handle as usize];
            entry.allocated && entry.staging.has_rx_data()
        }
    }

    /// Check if UDP socket can accept data for sending.
    pub(crate) fn udp_socket_can_send(handle: i32) -> bool {
        if handle < 0 || handle >= MAX_UDP_SOCKETS as i32 {
            return false;
        }

        unsafe {
            let entry = &UDP_SOCKET_TABLE[handle as usize];
            entry.allocated && entry.staging.has_tx_space()
        }
    }

    /// Receive data from the UDP socket's staging buffer.
    /// Returns bytes copied, 0 if no data, or -1 on error.
    pub(crate) fn udp_socket_recv(handle: i32, buf: &mut [u8]) -> i32 {
        if handle < 0 || handle >= MAX_UDP_SOCKETS as i32 || buf.is_empty() {
            return -1;
        }

        unsafe {
            let entry = &mut UDP_SOCKET_TABLE[handle as usize];
            if !entry.allocated {
                return -1;
            }

            entry
                .staging
                .recv(&UDP_SOCKET_RX_BUFFERS[handle as usize], buf)
        }
    }

    /// Send data into the UDP socket's TX staging buffer with a per-packet endpoint.
    /// Returns bytes copied, 0 if buffer full, or -1 on error.
    pub(crate) fn udp_socket_send(handle: i32, data: &[u8], ip: &[u8; 4], port: u16) -> i32 {
        if handle < 0 || handle >= MAX_UDP_SOCKETS as i32 || data.is_empty() {
            return -1;
        }

        unsafe {
            let entry = &mut UDP_SOCKET_TABLE[handle as usize];
            if !entry.allocated {
                return -1;
            }

            let result = entry
                .staging
                .send(&mut UDP_SOCKET_TX_BUFFERS[handle as usize], data);
            entry.tx_remote_ip = *ip;
            entry.tx_remote_port = port;

            result
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

/// Poll the network stack from C code.
///
/// Used by TLS platform symbols (tls_bare_metal.c) to pump the cooperative
/// smoltcp stack during TLS handshake and I/O operations.
#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_poll_network() {
    SmoltcpBridge::poll_network();
}

/// Get current time in milliseconds from C code.
///
/// Used by TLS platform symbols for timeout handling.
#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_clock_ms() -> u64 {
    SmoltcpBridge::clock_now_ms()
}

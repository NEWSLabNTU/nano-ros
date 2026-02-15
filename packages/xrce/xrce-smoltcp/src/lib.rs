//! smoltcp UDP transport for XRCE-DDS custom transport
//!
//! Provides [`XrceSmoltcpTransport`] for bridging XRCE-DDS custom transport
//! callbacks to smoltcp UDP sockets. Platform crates (BSPs) depend on this
//! crate and wire it to their hardware.
//!
//! # Usage
//!
//! 1. Call [`XrceSmoltcpTransport::init()`] with agent endpoint and local port
//! 2. Create a smoltcp `SocketSet` using [`get_socket_storage()`]
//! 3. Call [`XrceSmoltcpTransport::create_and_register_socket()`]
//! 4. Register a poll callback via [`XrceSmoltcpTransport::set_poll_callback()`]
//! 5. Pass callbacks from [`XrceSmoltcpTransport`] to
//!    `uxr_set_custom_transport_callbacks()`
//! 6. The poll callback should call [`XrceSmoltcpTransport::poll()`] with the
//!    platform's `Interface`, `Device`, and `SocketSet`

#![no_std]
#![allow(static_mut_refs)]

use core::ffi::c_int;

use smoltcp::iface::{PollResult, SocketHandle};
use smoltcp::socket::udp::{
    PacketBuffer as UdpPacketBuffer, PacketMetadata as UdpPacketMetadata, Socket as UdpSocket,
};
use smoltcp::wire::{IpAddress, IpEndpoint, Ipv4Address};

use xrce_sys::uxrCustomTransport;

// Re-export smoltcp types needed by platform crates
pub use smoltcp::iface::{Interface, SocketSet, SocketStorage};
pub use smoltcp::phy::Device;

// ============================================================================
// Configuration
// ============================================================================

/// UDP payload buffer size for both TX and RX.
/// Matches the XRCE transport MTU to avoid truncation.
pub const UDP_BUFFER_SIZE: usize = xrce_sys::XRCE_TRANSPORT_MTU;

/// Number of packet metadata slots per direction.
const UDP_META_COUNT: usize = 4;

// ============================================================================
// Static Buffers
// ============================================================================

// smoltcp UDP socket internal buffers (owned by smoltcp after socket creation)
static mut UDP_RX_META: [UdpPacketMetadata; UDP_META_COUNT] =
    [UdpPacketMetadata::EMPTY; UDP_META_COUNT];
static mut UDP_RX_PAYLOAD: [u8; UDP_BUFFER_SIZE] = [0u8; UDP_BUFFER_SIZE];
static mut UDP_TX_META: [UdpPacketMetadata; UDP_META_COUNT] =
    [UdpPacketMetadata::EMPTY; UDP_META_COUNT];
static mut UDP_TX_PAYLOAD: [u8; UDP_BUFFER_SIZE] = [0u8; UDP_BUFFER_SIZE];

// Staging buffers: bridge between XRCE-DDS callbacks and the smoltcp poll loop.
// Write callback copies outgoing data here; poll pushes it to the socket.
// Poll copies incoming data here; read callback consumes it.
static mut TX_STAGING: [u8; UDP_BUFFER_SIZE] = [0u8; UDP_BUFFER_SIZE];
static mut TX_STAGING_LEN: usize = 0;
static mut RX_STAGING: [u8; UDP_BUFFER_SIZE] = [0u8; UDP_BUFFER_SIZE];
static mut RX_STAGING_LEN: usize = 0;

// ============================================================================
// Global State
// ============================================================================

static mut AGENT_IP: [u8; 4] = [0; 4];
static mut AGENT_PORT: u16 = 0;
static mut LOCAL_PORT: u16 = 0;
static mut UDP_HANDLE_RAW: usize = usize::MAX;
static mut POLL_CALLBACK: Option<unsafe extern "C" fn()> = None;
static mut INITIALIZED: bool = false;

// ============================================================================
// Clock (resolved at link time from platform crate)
// ============================================================================

unsafe extern "C" {
    /// Millisecond clock — provided by the platform crate.
    /// Same symbol as zpico-smoltcp so platform crates only need one clock.
    fn smoltcp_clock_now_ms() -> u64;
}

// ============================================================================
// XrceSmoltcpTransport
// ============================================================================

/// XRCE-DDS UDP transport over smoltcp.
///
/// All methods are static — the transport uses module-level statics for
/// buffers and state. This avoids lifetime issues in `no_std` contexts.
///
/// # Architecture
///
/// ```text
/// XRCE-DDS session
///   ↕ write/read callbacks
/// TX/RX staging buffers (this crate)
///   ↕ poll() transfers data
/// smoltcp UDP socket
///   ↕ iface.poll()
/// Ethernet device (platform crate)
/// ```
pub struct XrceSmoltcpTransport;

impl XrceSmoltcpTransport {
    /// Initialize the transport with agent endpoint and local UDP port.
    ///
    /// Must be called before [`create_and_register_socket()`](Self::create_and_register_socket).
    pub fn init(agent_ip: [u8; 4], agent_port: u16, local_port: u16) {
        unsafe {
            AGENT_IP = agent_ip;
            AGENT_PORT = agent_port;
            LOCAL_PORT = local_port;
            TX_STAGING_LEN = 0;
            RX_STAGING_LEN = 0;
            UDP_HANDLE_RAW = usize::MAX;
            INITIALIZED = true;
        }
    }

    /// Get pre-allocated socket storage for smoltcp's `SocketSet`.
    ///
    /// Provides storage for 1 UDP socket. If the platform crate needs
    /// additional sockets, it should provide its own larger storage.
    ///
    /// # Safety
    ///
    /// Must only be called once. The returned reference has `'static` lifetime
    /// and must not be used concurrently.
    pub unsafe fn get_socket_storage() -> &'static mut [SocketStorage<'static>; 1] {
        static mut STORAGE: [SocketStorage<'static>; 1] = [SocketStorage::EMPTY; 1];
        unsafe { &mut STORAGE }
    }

    /// Create a UDP socket with static buffers, add it to the `SocketSet`,
    /// bind it to the configured local port, and register its handle.
    ///
    /// Must be called after [`init()`](Self::init) and before any XRCE-DDS
    /// operations.
    ///
    /// # Safety
    ///
    /// Must only be called once. The static buffers are consumed by smoltcp
    /// and must not be used concurrently.
    pub unsafe fn create_and_register_socket(sockets: &mut SocketSet<'static>) {
        unsafe {
            let rx = UdpPacketBuffer::new(&mut UDP_RX_META[..], &mut UDP_RX_PAYLOAD[..]);
            let tx = UdpPacketBuffer::new(&mut UDP_TX_META[..], &mut UDP_TX_PAYLOAD[..]);
            let udp_socket = UdpSocket::new(rx, tx);
            let handle = sockets.add(udp_socket);

            // Bind socket to local port
            let socket = sockets.get_mut::<UdpSocket>(handle);
            socket.bind(LOCAL_PORT).unwrap();

            UDP_HANDLE_RAW = core::mem::transmute::<SocketHandle, usize>(handle);
        }
    }

    /// Set the network poll callback.
    ///
    /// The callback is invoked by the transport's read/write functions to
    /// pump the network stack. Platform crates register a callback that
    /// calls [`XrceSmoltcpTransport::poll()`] with their owned resources.
    pub fn set_poll_callback(callback: unsafe extern "C" fn()) {
        unsafe {
            POLL_CALLBACK = Some(callback);
        }
    }

    /// Get the `open` transport callback for `uxr_set_custom_transport_callbacks`.
    pub fn open_callback() -> xrce_sys::open_custom_func {
        Some(xrce_open)
    }

    /// Get the `close` transport callback for `uxr_set_custom_transport_callbacks`.
    pub fn close_callback() -> xrce_sys::close_custom_func {
        Some(xrce_close)
    }

    /// Get the `write` transport callback for `uxr_set_custom_transport_callbacks`.
    pub fn write_callback() -> xrce_sys::write_custom_func {
        Some(xrce_write)
    }

    /// Get the `read` transport callback for `uxr_set_custom_transport_callbacks`.
    pub fn read_callback() -> xrce_sys::read_custom_func {
        Some(xrce_read)
    }

    /// Poll the network interface and transfer data between staging buffers
    /// and the smoltcp UDP socket.
    ///
    /// Called by the platform crate's poll callback. Returns `true` if any
    /// network activity occurred.
    pub fn poll<D: Device>(
        iface: &mut Interface,
        device: &mut D,
        sockets: &mut SocketSet,
    ) -> bool {
        let timestamp =
            smoltcp::time::Instant::from_millis(unsafe { smoltcp_clock_now_ms() } as i64);

        unsafe {
            if UDP_HANDLE_RAW == usize::MAX {
                return false;
            }
            let handle: SocketHandle = core::mem::transmute::<usize, SocketHandle>(UDP_HANDLE_RAW);

            // Push TX staging to socket
            if TX_STAGING_LEN > 0 {
                let socket = sockets.get_mut::<UdpSocket>(handle);
                if socket.can_send() {
                    let endpoint = IpEndpoint::new(
                        IpAddress::Ipv4(Ipv4Address::new(
                            AGENT_IP[0],
                            AGENT_IP[1],
                            AGENT_IP[2],
                            AGENT_IP[3],
                        )),
                        AGENT_PORT,
                    );
                    if socket
                        .send_slice(&TX_STAGING[..TX_STAGING_LEN], endpoint)
                        .is_ok()
                    {
                        TX_STAGING_LEN = 0;
                    }
                }
            }

            // Poll network (flushes TX, processes RX)
            let activity = iface.poll(timestamp, device, sockets);

            // Pull from socket to RX staging (only if staging is empty)
            if RX_STAGING_LEN == 0 {
                let socket = sockets.get_mut::<UdpSocket>(handle);
                if socket.can_recv()
                    && let Ok((len, _endpoint)) = socket.recv_slice(&mut RX_STAGING)
                {
                    RX_STAGING_LEN = len;
                }
            }

            matches!(activity, PollResult::SocketStateChanged)
        }
    }

    /// Trigger a poll via the registered callback.
    fn poll_network() {
        unsafe {
            if let Some(callback) = POLL_CALLBACK {
                callback();
            }
        }
    }

    /// Get current clock in milliseconds (delegates to platform).
    fn clock_now_ms() -> u64 {
        unsafe { smoltcp_clock_now_ms() }
    }
}

// ============================================================================
// XRCE-DDS Custom Transport Callbacks
// ============================================================================

/// Open callback: verifies the transport is initialized.
unsafe extern "C" fn xrce_open(_transport: *mut uxrCustomTransport) -> bool {
    unsafe { INITIALIZED && UDP_HANDLE_RAW != usize::MAX }
}

/// Close callback: no-op (socket lifetime is managed by the platform crate).
unsafe extern "C" fn xrce_close(_transport: *mut uxrCustomTransport) -> bool {
    true
}

/// Write callback: sends a UDP datagram to the agent via smoltcp.
///
/// Copies data to the TX staging buffer and polls the network to flush.
/// Returns the number of bytes written (== `length` on success, 0 on error).
unsafe extern "C" fn xrce_write(
    _transport: *mut uxrCustomTransport,
    buffer: *const u8,
    length: usize,
    error_code: *mut u8,
) -> usize {
    unsafe {
        if length > UDP_BUFFER_SIZE || length == 0 {
            *error_code = 1;
            return 0;
        }

        // If TX staging still has data from a previous write, poll to flush
        if TX_STAGING_LEN > 0 {
            XrceSmoltcpTransport::poll_network();
        }

        // If still can't stage new data, report error
        if TX_STAGING_LEN > 0 {
            *error_code = 1;
            return 0;
        }

        // Copy to TX staging
        let data = core::slice::from_raw_parts(buffer, length);
        TX_STAGING[..length].copy_from_slice(data);
        TX_STAGING_LEN = length;

        // Poll to flush the datagram
        XrceSmoltcpTransport::poll_network();

        length
    }
}

/// Read callback: receives a UDP datagram from the agent via smoltcp.
///
/// Polls the network in a loop until data arrives or timeout expires.
/// Returns the number of bytes read (0 on timeout).
unsafe extern "C" fn xrce_read(
    _transport: *mut uxrCustomTransport,
    buffer: *mut u8,
    length: usize,
    timeout: c_int,
    error_code: *mut u8,
) -> usize {
    unsafe {
        let _ = error_code; // Timeout is not an error for XRCE-DDS
        let start = XrceSmoltcpTransport::clock_now_ms();
        let timeout_ms = if timeout < 0 { u64::MAX } else { timeout as u64 };

        loop {
            XrceSmoltcpTransport::poll_network();

            if RX_STAGING_LEN > 0 {
                let to_copy = RX_STAGING_LEN.min(length);
                let out = core::slice::from_raw_parts_mut(buffer, to_copy);
                out.copy_from_slice(&RX_STAGING[..to_copy]);
                RX_STAGING_LEN = 0;
                return to_copy;
            }

            let elapsed = XrceSmoltcpTransport::clock_now_ms().wrapping_sub(start);
            if elapsed >= timeout_ms {
                return 0;
            }
        }
    }
}

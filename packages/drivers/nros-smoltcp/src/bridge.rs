//! SmoltcpBridge - Socket table management and smoltcp data transfer
//!
//! Manages the mapping between socket handles (small integer indices) and
//! smoltcp's `SocketHandle` type. Provides methods for opening, connecting,
//! sending, and receiving on sockets, as well as the poll loop that transfers
//! data between staging buffers and smoltcp's internal socket buffers.
//!
//! This crate is RMW-agnostic — it can be used by zenoh-pico, XRCE-DDS,
//! or any other middleware that needs TCP/UDP networking on bare-metal.

use smoltcp::{
    iface::{Interface, PollResult, SocketHandle, SocketSet},
    phy::Device,
    socket::{
        tcp::{Socket as TcpSocket, State as TcpState},
        udp::{Socket as UdpSocket, UdpMetadata},
    },
    wire::{IpAddress, IpEndpoint, Ipv4Address},
};

// ============================================================================
// Configuration
// ============================================================================

pub use crate::config::{
    CONNECT_TIMEOUT_MS, MAX_SOCKETS, MAX_UDP_SOCKETS, SOCKET_BUFFER_SIZE, SOCKET_TIMEOUT_MS,
};

/// RFC 6056 ephemeral port range lower bound.
const EPHEMERAL_PORT_START: u16 = 49152;

/// RFC 6056 ephemeral port range size (49152..65535).
const EPHEMERAL_PORT_RANGE: u16 = 65535 - EPHEMERAL_PORT_START;

/// Next ephemeral port counter
static mut NEXT_EPHEMERAL_PORT: u16 = EPHEMERAL_PORT_START;

// ============================================================================
// Staging State (shared between TCP and UDP)
// ============================================================================

/// Staging buffer state for bidirectional data transfer.
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
/// from a previous QEMU run, the new SYN is dropped. Seeding with a value
/// derived from the IP address or clock randomizes the starting port.
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

/// State for a single TCP socket in the bridge table.
#[derive(Clone, Copy)]
struct SocketEntry {
    allocated: bool,
    handle_raw: usize,
    remote_ip: [u8; 4],
    remote_port: u16,
    local_port: u16,
    connected: bool,
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

/// Socket RX/TX staging buffers
static mut SOCKET_RX_BUFFERS: [[u8; SOCKET_BUFFER_SIZE]; MAX_SOCKETS] =
    [[0u8; SOCKET_BUFFER_SIZE]; MAX_SOCKETS];
static mut SOCKET_TX_BUFFERS: [[u8; SOCKET_BUFFER_SIZE]; MAX_SOCKETS] =
    [[0u8; SOCKET_BUFFER_SIZE]; MAX_SOCKETS];

// ============================================================================
// UDP Socket State
// ============================================================================

/// State for a single UDP socket in the bridge table.
#[derive(Clone, Copy)]
struct UdpSocketEntry {
    allocated: bool,
    handle_raw: usize,
    remote_ip: [u8; 4],
    remote_port: u16,
    local_port: u16,
    staging: StagingState,
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
// Phase 71.26 — multicast group join queue
// ============================================================================
//
// `Interface::join_multicast_group()` requires a `&mut Interface` plus a
// timestamp, so it can only be invoked from `SmoltcpBridge::poll`. Other
// call sites (e.g. `PlatformUdpMulticast::mcast_listen` from a board
// crate) push group addresses into a small queue here; `poll` drains it
// at the start of each iteration.
//
// `MAX_MULTICAST_PENDING` covers the SPDP / SEDP set (one builtin
// multicast group + a handful of user-topic groups) with headroom.
// `MULTICAST_JOINED` tracks groups already passed to the interface so
// repeated `mcast_listen` calls on the same address don't spam the
// IGMP report path.

const MAX_MULTICAST_GROUPS: usize = 8;

static mut MULTICAST_PENDING: [Option<Ipv4Address>; MAX_MULTICAST_GROUPS] =
    [None; MAX_MULTICAST_GROUPS];
static mut MULTICAST_JOINED: [Option<Ipv4Address>; MAX_MULTICAST_GROUPS] =
    [None; MAX_MULTICAST_GROUPS];

/// Queue an IPv4 multicast group for the next poll to join via
/// `Interface::join_multicast_group`.
///
/// Returns `false` if the queue is full or the group is already joined
/// / pending. Idempotent — duplicate calls are no-ops.
pub fn queue_multicast_join(group: Ipv4Address) -> bool {
    unsafe {
        let pending = &raw mut MULTICAST_PENDING;
        let joined = &raw const MULTICAST_JOINED;
        // Already joined?
        for slot in (*joined).iter() {
            if *slot == Some(group) {
                return true;
            }
        }
        // Already pending?
        for slot in (*pending).iter() {
            if *slot == Some(group) {
                return true;
            }
        }
        // Insert into first empty pending slot.
        for slot in (*pending).iter_mut() {
            if slot.is_none() {
                *slot = Some(group);
                return true;
            }
        }
        false
    }
}

// Phase 97.3 mcast-join diagnostic counters.
static MCAST_JOIN_ATTEMPTS: portable_atomic::AtomicU32 = portable_atomic::AtomicU32::new(0);
static MCAST_JOIN_OK: portable_atomic::AtomicU32 = portable_atomic::AtomicU32::new(0);
static MCAST_JOIN_ERR_UNADDR: portable_atomic::AtomicU32 = portable_atomic::AtomicU32::new(0);
static MCAST_JOIN_ERR_FULL: portable_atomic::AtomicU32 = portable_atomic::AtomicU32::new(0);

/// Snapshot of multicast-join counters (attempts, ok, err_unaddressable, err_full).
pub fn mcast_join_counters() -> (u32, u32, u32, u32) {
    use portable_atomic::Ordering;
    (
        MCAST_JOIN_ATTEMPTS.load(Ordering::Relaxed),
        MCAST_JOIN_OK.load(Ordering::Relaxed),
        MCAST_JOIN_ERR_UNADDR.load(Ordering::Relaxed),
        MCAST_JOIN_ERR_FULL.load(Ordering::Relaxed),
    )
}

/// Drain the pending-join queue, calling `iface.join_multicast_group`
/// for each address. Called once per `SmoltcpBridge::poll` iteration.
fn drain_multicast_joins<D: Device>(
    iface: &mut Interface,
    device: &mut D,
    timestamp: smoltcp::time::Instant,
) {
    use portable_atomic::Ordering;
    let _ = (device, timestamp);
    unsafe {
        let pending = &raw mut MULTICAST_PENDING;
        let joined = &raw mut MULTICAST_JOINED;
        for i in 0..MAX_MULTICAST_GROUPS {
            if let Some(group) = (*pending)[i].take() {
                MCAST_JOIN_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
                match iface.join_multicast_group(IpAddress::Ipv4(group)) {
                    Ok(()) => {
                        MCAST_JOIN_OK.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(smoltcp::iface::MulticastError::Unaddressable) => {
                        MCAST_JOIN_ERR_UNADDR.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(smoltcp::iface::MulticastError::GroupTableFull) => {
                        MCAST_JOIN_ERR_FULL.fetch_add(1, Ordering::Relaxed);
                    }
                    #[allow(unreachable_patterns)]
                    Err(_) => {
                        // Forward-compat for new variants — count as full.
                        MCAST_JOIN_ERR_FULL.fetch_add(1, Ordering::Relaxed);
                    }
                }
                // Record in the joined table so we don't double-join later.
                for slot in (*joined).iter_mut() {
                    if slot.is_none() {
                        *slot = Some(group);
                        break;
                    }
                }
            }
        }
    }
}

// ============================================================================
// Poll Callback
// ============================================================================

type PollCallbackFn = Option<unsafe extern "C" fn()>;
static mut POLL_CALLBACK: PollCallbackFn = None;
static mut IDLE_CALLBACK: PollCallbackFn = None;
static mut SMOLTCP_POLL_COUNT: u32 = 0;

// Phase 127.A — wire-level diagnostic counters. `do_poll` increments
// `DO_POLL_CALLS`; `BRIDGE_POLL_CALLS` counts how many times the
// registered callback dispatched into `SmoltcpBridge::poll`. If the
// two are widely different, either the callback was never registered or
// the callback ran but `NetworkState::poll` short-circuited on null
// pointers. `BRIDGE_TX_DRAINED_BYTES` tracks how many staged bytes the
// bridge actually pushed into smoltcp socket TX queues.
static DO_POLL_CALLS: portable_atomic::AtomicU32 = portable_atomic::AtomicU32::new(0);
static DO_POLL_CB_HITS: portable_atomic::AtomicU32 = portable_atomic::AtomicU32::new(0);
static BRIDGE_POLL_CALLS: portable_atomic::AtomicU32 = portable_atomic::AtomicU32::new(0);
static BRIDGE_TX_DRAINED_BYTES: portable_atomic::AtomicU32 = portable_atomic::AtomicU32::new(0);
static BRIDGE_RX_DRAINED_BYTES: portable_atomic::AtomicU32 = portable_atomic::AtomicU32::new(0);
static TCP_RECV_BYTES_OUT: portable_atomic::AtomicU32 = portable_atomic::AtomicU32::new(0);

/// Snapshot of `do_poll`/bridge-poll diagnostic counters.
///
/// Returns `(do_poll_calls, do_poll_callback_hits, bridge_poll_calls,
/// bridge_tx_drained_bytes)`. Useful for instrumenting bring-up where
/// staged bytes accumulate but never reach the wire.
pub fn poll_diagnostics() -> (u32, u32, u32, u32) {
    use portable_atomic::Ordering;
    (
        DO_POLL_CALLS.load(Ordering::Relaxed),
        DO_POLL_CB_HITS.load(Ordering::Relaxed),
        BRIDGE_POLL_CALLS.load(Ordering::Relaxed),
        BRIDGE_TX_DRAINED_BYTES.load(Ordering::Relaxed),
    )
}

/// Phase 127.D — extended counters: (rx_drained, tcp_recv_out).
/// `rx_drained` = bytes transferred socket.recv_slice → RX staging.
/// `tcp_recv_out` = bytes returned to caller by `tcp_recv` (= bytes
/// zenoh-pico actually read off the staging buffer).
pub fn rx_diagnostics() -> (u32, u32) {
    use portable_atomic::Ordering;
    (
        BRIDGE_RX_DRAINED_BYTES.load(Ordering::Relaxed),
        TCP_RECV_BYTES_OUT.load(Ordering::Relaxed),
    )
}

/// Set the poll callback function.
pub fn set_poll_callback(callback: unsafe extern "C" fn()) {
    unsafe {
        POLL_CALLBACK = Some(callback);
    }
}

/// Phase 127.D — install an idle callback invoked once at the end of
/// each [`do_poll`] iteration. Boards with an armed IRQ source may
/// register `cortex_m::asm::wfi` (or equivalent) here so the tight
/// connect/send/recv loops inside `<PlatformTcp>::open`/`send`/`read`
/// release the CPU to QEMU's main loop between polls.
///
/// Default = unset; callers without an armed IRQ should leave it
/// unset because `wfi` with no pending interrupt deadlocks.
pub fn set_idle_callback(callback: unsafe extern "C" fn()) {
    unsafe {
        IDLE_CALLBACK = Some(callback);
    }
}

/// Clear the idle callback. Subsequent [`do_poll`] invocations skip
/// the idle step.
pub fn clear_idle_callback() {
    unsafe {
        IDLE_CALLBACK = None;
    }
}

/// Invoke the registered poll callback.
///
/// Returns 0 if callback was invoked, -1 if no callback registered.
pub fn do_poll() -> i32 {
    use portable_atomic::Ordering;
    DO_POLL_CALLS.fetch_add(1, Ordering::Relaxed);
    let rc = unsafe {
        SMOLTCP_POLL_COUNT += 1;
        if let Some(callback) = POLL_CALLBACK {
            DO_POLL_CB_HITS.fetch_add(1, Ordering::Relaxed);
            callback();
            0
        } else {
            -1
        }
    };
    // Phase 127.D — yield CPU between polls when the board armed an
    // IRQ source. Runs even when no poll callback is registered so a
    // misconfigured board with idle-only hooks still ticks.
    unsafe {
        if let Some(idle) = IDLE_CALLBACK {
            idle();
        }
    }
    rc
}

/// Check if a poll callback is registered.
pub fn has_poll_callback() -> bool {
    unsafe {
        let cb = &raw const POLL_CALLBACK;
        (*cb).is_some()
    }
}

/// Get the total number of polls executed.
pub fn poll_count() -> u32 {
    unsafe { SMOLTCP_POLL_COUNT }
}

// ============================================================================
// Clock (resolved at link time from platform crate)
// ============================================================================

unsafe extern "C" {
    /// Millisecond clock — provided by the board crate's platform implementation.
    fn smoltcp_clock_now_ms() -> u64;
}

// ============================================================================
// SmoltcpBridge
// ============================================================================

/// Bridge between socket operations and the smoltcp TCP/IP stack.
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

    /// Check if the bridge has been initialized.
    pub fn is_initialized() -> bool {
        unsafe { BRIDGE_STATE.initialized }
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
    ///
    /// Sequence (Phase 127.A):
    ///   1. Drain TX staging → smoltcp socket TX queues.
    ///   2. `iface.poll()` — pulls socket TX out to the device, drains device
    ///      RX into socket RX queues.
    ///   3. Drain socket RX queues → RX staging buffers.
    ///   4. Second `iface.poll()` — flushes any device-side state changes
    ///      triggered by the RX drain (e.g. window updates / ACKs).
    ///
    /// Pre-127.A the function did `iface.poll` first and the TX drain second,
    /// so newly-staged bytes had to wait for the NEXT `poll_network()`
    /// invocation before they reached the wire. Combined with how
    /// `<P as PlatformTcp>::send` calls `poll_network` once per loop
    /// iteration, that gap let staging accumulate even though `socket.can_send()`
    /// was true on the Established socket.
    pub fn poll<D: Device>(iface: &mut Interface, device: &mut D, sockets: &mut SocketSet) -> bool {
        use portable_atomic::Ordering;
        BRIDGE_POLL_CALLS.fetch_add(1, Ordering::Relaxed);

        let timestamp =
            smoltcp::time::Instant::from_millis(unsafe { smoltcp_clock_now_ms() } as i64);

        // Phase 71.26 — drain any multicast joins queued by
        // `mcast_listen` since the previous poll, so the IP layer
        // sees the IGMP membership before the first inbound packet
        // arrives.
        drain_multicast_joins(iface, device, timestamp);

        // 1. Stage-to-socket TX drain + connect kickoff.
        unsafe {
            let table = &raw mut SOCKET_TABLE;
            for idx in 0..MAX_SOCKETS {
                let entry = &mut (*table)[idx];
                if !entry.allocated || !entry.has_handle() {
                    continue;
                }

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
                        TcpState::SynSent | TcpState::SynReceived => {}
                        _ => {}
                    }
                }

                if socket.state() == TcpState::Established {
                    entry.connected = true;
                    if entry.staging.has_tx_pending() && socket.can_send() {
                        let tx_buf = &SOCKET_TX_BUFFERS[idx];
                        let data = entry.staging.tx_pending(tx_buf);
                        if let Ok(sent) = socket.send_slice(data) {
                            entry.staging.advance_tx(sent);
                            if sent > 0 {
                                BRIDGE_TX_DRAINED_BYTES.fetch_add(sent as u32, Ordering::Relaxed);
                            }
                        }
                    }
                }
            }

            let table = &raw mut UDP_SOCKET_TABLE;
            for idx in 0..MAX_UDP_SOCKETS {
                let entry = &mut (*table)[idx];
                if !entry.allocated || !entry.has_handle() {
                    continue;
                }
                let handle = entry.handle();
                let socket = sockets.get_mut::<UdpSocket>(handle);
                if !socket.is_open() {
                    let _ = socket.bind(entry.local_port);
                }
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
            }
        }

        // 2. Push newly-queued socket TX out to the wire AND pull device RX.
        let activity = iface.poll(timestamp, device, sockets);

        // 3. Drain socket RX → staging + reconcile post-poll TCP state.
        unsafe {
            let table = &raw mut SOCKET_TABLE;
            for idx in 0..MAX_SOCKETS {
                let entry = &mut (*table)[idx];
                if !entry.allocated || !entry.has_handle() {
                    continue;
                }
                let handle = entry.handle();
                let socket = sockets.get_mut::<TcpSocket>(handle);
                match socket.state() {
                    TcpState::Established => {
                        entry.connected = true;
                        if socket.can_recv() {
                            entry.staging.compact_rx(&mut SOCKET_RX_BUFFERS[idx]);
                            let space = entry.staging.rx_space();
                            if space > 0 {
                                let rx_buf = &mut SOCKET_RX_BUFFERS[idx];
                                if let Ok(received) =
                                    socket.recv_slice(&mut rx_buf[entry.staging.rx_len..])
                                {
                                    entry.staging.advance_rx(received);
                                    if received > 0 {
                                        BRIDGE_RX_DRAINED_BYTES
                                            .fetch_add(received as u32, Ordering::Relaxed);
                                    }
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

            let table = &raw mut UDP_SOCKET_TABLE;
            for idx in 0..MAX_UDP_SOCKETS {
                let entry = &mut (*table)[idx];
                if !entry.allocated || !entry.has_handle() {
                    continue;
                }
                let handle = entry.handle();
                let socket = sockets.get_mut::<UdpSocket>(handle);
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

        // 4. Flush ACKs / window updates triggered by the RX drain so
        //    peers see the new advertised window in the same poll. Without
        //    this, a saturated peer can be left holding bytes for an extra
        //    poll cycle.
        let _ = iface.poll(timestamp, device, sockets);

        matches!(activity, PollResult::SocketStateChanged)
    }

    // ========================================================================
    // TCP socket operations
    // ========================================================================

    /// Allocate a TCP socket from the table. Returns slot index or -1.
    pub fn tcp_open() -> i32 {
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

    /// Set the remote endpoint for a TCP socket. Returns 0 on success, -1 on error.
    pub fn tcp_connect(handle: i32, ip: &[u8; 4], port: u16) -> i32 {
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

    /// Check if a TCP socket is connected.
    pub fn tcp_is_connected(handle: i32) -> bool {
        if handle < 0 || handle >= MAX_SOCKETS as i32 {
            return false;
        }

        unsafe {
            let entry = &SOCKET_TABLE[handle as usize];
            entry.allocated && entry.connected
        }
    }

    /// Close a TCP socket. Returns 0 on success, -1 on error.
    pub fn tcp_close(handle: i32) -> i32 {
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

    /// Check if TCP socket has data available to receive.
    pub fn tcp_can_recv(handle: i32) -> bool {
        if handle < 0 || handle >= MAX_SOCKETS as i32 {
            return false;
        }

        unsafe {
            let entry = &SOCKET_TABLE[handle as usize];
            entry.allocated && entry.staging.has_rx_data()
        }
    }

    /// Check if TCP socket can accept data for sending.
    pub fn tcp_can_send(handle: i32) -> bool {
        if handle < 0 || handle >= MAX_SOCKETS as i32 {
            return false;
        }

        unsafe {
            let entry = &SOCKET_TABLE[handle as usize];
            entry.allocated && entry.connected && entry.staging.has_tx_space()
        }
    }

    /// Receive data from the TCP socket's staging buffer.
    /// Returns bytes copied, 0 if no data, or -1 on error.
    pub fn tcp_recv(handle: i32, buf: &mut [u8]) -> i32 {
        if handle < 0 || handle >= MAX_SOCKETS as i32 || buf.is_empty() {
            return -1;
        }

        unsafe {
            let entry = &mut SOCKET_TABLE[handle as usize];
            if !entry.allocated {
                return -1;
            }

            let got = entry.staging.recv(&SOCKET_RX_BUFFERS[handle as usize], buf);
            if got > 0 {
                TCP_RECV_BYTES_OUT.fetch_add(got as u32, portable_atomic::Ordering::Relaxed);
            }
            got
        }
    }

    /// Send data into the TCP socket's TX staging buffer.
    /// Returns bytes copied, 0 if buffer full, or -1 on error.
    pub fn tcp_send(handle: i32, data: &[u8]) -> i32 {
        if handle < 0 || handle >= MAX_SOCKETS as i32 || data.is_empty() {
            return -1;
        }

        unsafe {
            let entry = &mut SOCKET_TABLE[handle as usize];
            if !entry.allocated || !entry.connected {
                return -1;
            }

            entry
                .staging
                .send(&mut SOCKET_TX_BUFFERS[handle as usize], data)
        }
    }

    // ========================================================================
    // UDP socket operations
    // ========================================================================

    /// Allocate a UDP socket from the table. Returns slot index or -1.
    pub fn udp_open() -> i32 {
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

    /// Phase 71.21 — bind a UDP socket to a specific local port.
    ///
    /// Stamps the entry's `local_port` so the next `do_poll()` calls
    /// `socket.bind(port)` on the underlying smoltcp UDP socket.
    /// Required by DDS, which needs a deterministic SPDP/SEDP source
    /// port (see RTPS PSM §9.6.1.4 — the multicast metatraffic port
    /// `7400 + 250·domain_id` and the unicast variants).
    pub fn udp_set_local_port(handle: i32, local_port: u16) -> i32 {
        if handle < 0 || handle >= MAX_UDP_SOCKETS as i32 {
            return -1;
        }
        unsafe {
            let entry = &mut UDP_SOCKET_TABLE[handle as usize];
            if !entry.allocated {
                return -1;
            }
            entry.local_port = local_port;
            0
        }
    }

    /// Set the remote endpoint for a UDP socket. Returns 0 on success, -1 on error.
    pub fn udp_set_remote(handle: i32, ip: &[u8; 4], port: u16) -> i32 {
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

    /// Close a UDP socket. Returns 0 on success, -1 on error.
    pub fn udp_close(handle: i32) -> i32 {
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
    pub fn udp_can_recv(handle: i32) -> bool {
        if handle < 0 || handle >= MAX_UDP_SOCKETS as i32 {
            return false;
        }

        unsafe {
            let entry = &UDP_SOCKET_TABLE[handle as usize];
            entry.allocated && entry.staging.has_rx_data()
        }
    }

    /// Check if UDP socket can accept data for sending.
    pub fn udp_can_send(handle: i32) -> bool {
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
    pub fn udp_recv(handle: i32, buf: &mut [u8]) -> i32 {
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
    pub fn udp_send(handle: i32, data: &[u8], ip: &[u8; 4], port: u16) -> i32 {
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

    // ========================================================================
    // Clock and poll helpers
    // ========================================================================

    /// Get current clock in milliseconds (delegates to platform).
    pub fn clock_now_ms() -> u64 {
        unsafe { smoltcp_clock_now_ms() }
    }

    /// Trigger a poll via the registered callback.
    pub fn poll_network() {
        do_poll();
    }
}

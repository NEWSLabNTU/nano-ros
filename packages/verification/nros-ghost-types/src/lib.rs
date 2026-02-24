//! Ghost model types for nros verification.
//!
//! This crate defines ghost model types — manually audited mirrors of production
//! types with private fields. Ghost types have all-public fields with primitive
//! Rust types, enabling two uses:
//!
//! 1. **Production crate tests** (`#[cfg(test)]`) construct ghost types from
//!    private fields to verify structural correspondence. If a field is renamed
//!    or retyped, the construction fails to compile.
//!
//! 2. **Verus verification crate** imports ghost types and registers them via
//!    `external_type_specification` for use in deductive proofs.
//!
//! See `docs/design/ghost-model-validation.md` for the full validation strategy.

#![no_std]

// ======================================================================
// CDR Serialization
// ======================================================================

/// Ghost model of `CdrWriter<'a>` / `CdrReader<'a>`.
///
/// Mirrors private fields in `nros-serdes/src/cdr.rs`:
/// - `buf: &'a mut [u8]` (writer) / `buf: &'a [u8]` (reader) → modeled as `buf_len: usize`
/// - `pos: usize` → `pos: usize`
/// - `origin: usize` → `origin: usize`
///
/// Source (cdr.rs:9-13, 198-202):
/// ```ignore
/// pub struct CdrWriter<'a> {
///     buf: &'a mut [u8],
///     pos: usize,
///     origin: usize,
/// }
/// ```
pub struct CdrGhost {
    /// Buffer length (`buf.len()` — not the buffer itself)
    pub buf_len: usize,
    /// Current write/read position
    pub pos: usize,
    /// Alignment origin (set by CDR header)
    pub origin: usize,
}

// ======================================================================
// Subscriber Buffer
// ======================================================================

/// Ghost model of `SubscriberBuffer` state.
///
/// Models the state machine of the subscriber's static buffer in
/// `nros-rmw/src/shim.rs`. Each subscriber has one 1024-byte
/// static buffer with atomic `has_data`, `overflow`, and `len` fields.
///
/// Source (shim.rs):
/// ```ignore
/// struct SubscriberBuffer {
///     data: [u8; SUBSCRIBER_BUFFER_SIZE],
///     has_data: AtomicBool,
///     overflow: AtomicBool,
///     locked: AtomicBool,
///     len: AtomicUsize,
///     // ... attachment fields omitted ...
/// }
/// ```
pub struct SubscriberBufferGhost {
    /// Whether the buffer contains unprocessed data
    pub has_data: bool,
    /// Whether the last callback detected a message exceeding buffer capacity
    pub overflow: bool,
    /// Whether a reader is currently accessing this buffer (prevents callback writes)
    pub locked: bool,
    /// Length of valid payload data in the buffer
    pub stored_len: usize,
    /// Static buffer capacity (always 1024 in production)
    pub buf_capacity: usize,
}

// ======================================================================
// Service Buffer
// ======================================================================

/// Ghost model of `ServiceBuffer` state.
///
/// Models the state machine of the service server's static buffer in
/// `nros-rmw-zenoh/src/shim.rs`. Each service server has one 1024-byte
/// static buffer with atomic `has_request`, `overflow`, and `len` fields.
///
/// Source (shim.rs):
/// ```ignore
/// struct ServiceBuffer {
///     data: [u8; SERVICE_BUFFER_SIZE],
///     keyexpr: [u8; 256],
///     has_request: AtomicBool,
///     overflow: AtomicBool,
///     len: AtomicUsize,
///     keyexpr_len: AtomicUsize,
///     sequence_number: AtomicSeqCounter,
/// }
/// ```
pub struct ServiceBufferGhost {
    /// Whether the buffer contains an unprocessed request
    pub has_request: bool,
    /// Whether the last callback detected a request exceeding buffer capacity
    pub overflow: bool,
    /// Length of valid request data in the buffer
    pub stored_len: usize,
    /// Static buffer capacity (always 1024 in production)
    pub buf_capacity: usize,
}

// ======================================================================
// Buffer State Machine Operations
// ======================================================================

/// Result of a subscriber buffer read operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubReadResult {
    /// No data available
    Empty,
    /// Data available and successfully read
    Ok,
    /// Overflow detected — message was too large
    Overflow,
}

/// Result of a service buffer read operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SvcReadResult {
    /// No request available
    Empty,
    /// Request available and successfully read
    Ok,
    /// Overflow detected — request was too large
    Overflow,
}

impl SubscriberBufferGhost {
    /// Initial empty state.
    pub fn new(buf_capacity: usize) -> Self {
        Self {
            has_data: false,
            overflow: false,
            locked: false,
            stored_len: 0,
            buf_capacity,
        }
    }

    /// Models the transport callback writing into the buffer.
    ///
    /// Returns `true` if the write succeeded, `false` if the message was
    /// dropped (buffer locked).
    pub fn callback_write(&mut self, msg_len: usize) -> bool {
        if self.locked {
            // Reader is processing — drop message
            return false;
        }
        if msg_len > self.buf_capacity {
            self.overflow = true;
            self.has_data = true;
            // stored_len is not updated on overflow
        } else {
            self.overflow = false;
            self.stored_len = msg_len;
            self.has_data = true;
        }
        true
    }

    /// Models `try_recv_raw` — copy-based read with lock window.
    pub fn try_recv_raw(&mut self, user_buf_capacity: usize) -> SubReadResult {
        if !self.has_data {
            return SubReadResult::Empty;
        }
        if self.overflow {
            self.overflow = false;
            self.has_data = false;
            return SubReadResult::Overflow;
        }
        // Lock window: locked=true → copy → locked=false → has_data=false
        self.locked = true;
        let fits = self.stored_len <= user_buf_capacity;
        self.locked = false;
        self.has_data = false;
        if fits {
            SubReadResult::Ok
        } else {
            SubReadResult::Overflow
        }
    }

    /// Models `process_raw_in_place` — in-place read with lock window.
    pub fn process_in_place(&mut self) -> SubReadResult {
        if !self.has_data {
            return SubReadResult::Empty;
        }
        if self.overflow {
            self.overflow = false;
            self.has_data = false;
            return SubReadResult::Overflow;
        }
        // Lock window: locked=true → f(&data[..len]) → locked=false → has_data=false
        self.locked = true;
        // Callback would be dropped here if it fired
        self.locked = false;
        self.has_data = false;
        SubReadResult::Ok
    }
}

impl ServiceBufferGhost {
    /// Initial empty state.
    pub fn new(buf_capacity: usize) -> Self {
        Self {
            has_request: false,
            overflow: false,
            stored_len: 0,
            buf_capacity,
        }
    }

    /// Models the queryable/request callback writing into the buffer.
    pub fn callback_write(&mut self, msg_len: usize) {
        if msg_len > self.buf_capacity {
            self.overflow = true;
            self.has_request = true;
        } else {
            self.overflow = false;
            self.stored_len = msg_len;
            self.has_request = true;
        }
    }

    /// Models `try_recv_request`.
    pub fn try_recv_request(&mut self) -> SvcReadResult {
        if !self.has_request {
            return SvcReadResult::Empty;
        }
        if self.overflow {
            self.overflow = false;
            self.has_request = false;
            return SvcReadResult::Overflow;
        }
        self.has_request = false;
        SvcReadResult::Ok
    }
}

// ======================================================================
// Staging Buffer (SmoltcpBridge)
// ======================================================================

/// Ghost model of the SmoltcpBridge staging buffer state machine.
///
/// Models the `rx_pos/rx_len/tx_pos/tx_len` fields of `SocketEntry` (TCP) and
/// `UdpSocketEntry` (UDP) in `zpico-smoltcp/src/bridge.rs`. Both TCP and UDP
/// use identical staging buffer logic for recv/send/compact/fill.
///
/// Source (bridge.rs:32-54 for TCP, 109-132 for UDP):
/// ```ignore
/// struct SocketEntry {
///     // ... connection fields ...
///     rx_pos: usize,
///     rx_len: usize,
///     tx_pos: usize,
///     tx_len: usize,
/// }
/// ```
///
/// Invariants:
/// - `rx_pos <= rx_len <= capacity`
/// - `tx_pos <= tx_len <= capacity`
pub struct StagingBufferGhost {
    /// Current read position in the RX buffer
    pub rx_pos: usize,
    /// Total valid bytes in the RX buffer (from position 0)
    pub rx_len: usize,
    /// Current drain position in the TX buffer (only advanced by poll)
    pub tx_pos: usize,
    /// Total valid bytes in the TX buffer (from position 0)
    pub tx_len: usize,
    /// Buffer capacity (SOCKET_BUFFER_SIZE in production)
    pub capacity: usize,
}

impl StagingBufferGhost {
    /// Initial empty state.
    pub fn new(capacity: usize) -> Self {
        Self {
            rx_pos: 0,
            rx_len: 0,
            tx_pos: 0,
            tx_len: 0,
            capacity,
        }
    }

    /// Models `socket_recv` / `udp_socket_recv` (bridge.rs:582-610, 737-764).
    ///
    /// Copies `min(available, user_buf_len)` bytes from `rx_buf[rx_pos..]`,
    /// advances `rx_pos`. When `rx_pos >= rx_len`, resets both to 0.
    ///
    /// Returns bytes copied.
    pub fn recv(&mut self, user_buf_len: usize) -> usize {
        let available = self.rx_len.saturating_sub(self.rx_pos);
        if available == 0 {
            return 0;
        }
        let to_copy = available.min(user_buf_len);
        self.rx_pos += to_copy;

        // Reset if all consumed
        if self.rx_pos >= self.rx_len {
            self.rx_pos = 0;
            self.rx_len = 0;
        }
        to_copy
    }

    /// Models `socket_send` / `udp_socket_send` (bridge.rs:614-637, 769-794).
    ///
    /// Appends `min(available_space, data_len)` bytes at `tx_buf[tx_len..]`,
    /// advances `tx_len`. Note: `tx_pos` is NOT modified (only poll drains TX).
    ///
    /// Returns bytes copied.
    pub fn send(&mut self, data_len: usize) -> usize {
        let available = self.capacity.saturating_sub(self.tx_len);
        if available == 0 {
            return 0;
        }
        let to_copy = available.min(data_len);
        self.tx_len += to_copy;
        to_copy
    }

    /// Models RX buffer compaction in `poll()` (bridge.rs:387-392, 459-465).
    ///
    /// Moves `rx_buf[rx_pos..rx_len]` to `rx_buf[0..rx_len-rx_pos]`.
    /// Called during poll when `rx_pos > 0` and socket can receive.
    pub fn compact_rx(&mut self) {
        if self.rx_pos > 0 {
            let remaining = self.rx_len - self.rx_pos;
            self.rx_len = remaining;
            self.rx_pos = 0;
        }
    }

    /// Models TX drain in `poll()` for TCP (bridge.rs:372-382).
    ///
    /// Sends `sent` bytes from `tx_buf[tx_pos..tx_len]`, advances `tx_pos`.
    /// When `tx_pos >= tx_len`, resets both to 0.
    pub fn drain_tx(&mut self, sent: usize) {
        if sent > 0 {
            self.tx_pos += sent;
            if self.tx_pos >= self.tx_len {
                self.tx_pos = 0;
                self.tx_len = 0;
            }
        }
    }

    /// Models RX fill in `poll()` (bridge.rs:395-404, 467-476).
    ///
    /// Appends `received` bytes at `rx_buf[rx_len..]` from the smoltcp socket.
    /// Called after compaction.
    pub fn fill_rx(&mut self, received: usize) {
        self.rx_len += received;
    }
}

// ======================================================================
// Ephemeral Port Counter
// ======================================================================

/// Start of the IANA ephemeral port range.
pub const EPHEMERAL_PORT_START: u16 = 49152;

/// Models the ephemeral port increment logic in `SmoltcpBridge` (bridge.rs:269-271, 296-298).
///
/// Production code:
/// ```ignore
/// NEXT_EPHEMERAL_PORT = NEXT_EPHEMERAL_PORT.wrapping_add(1);
/// if NEXT_EPHEMERAL_PORT < EPHEMERAL_PORT_START {
///     NEXT_EPHEMERAL_PORT = EPHEMERAL_PORT_START;
/// }
/// ```
///
/// Used by both `register_socket` (TCP) and `register_udp_socket` (UDP).
pub fn ephemeral_port_next(current: u16) -> u16 {
    let next = current.wrapping_add(1);
    if next < EPHEMERAL_PORT_START {
        EPHEMERAL_PORT_START
    } else {
        next
    }
}

// ======================================================================
// Kani Bounded Model Checking
// ======================================================================

#[cfg(kani)]
mod verification {
    use super::*;

    // ------------------------------------------------------------------
    // SubscriberBuffer invariants
    // ------------------------------------------------------------------

    /// After any callback, if has_data && !overflow, then stored_len <= buf_capacity.
    #[kani::proof]
    fn sub_callback_len_bounded() {
        let cap: usize = kani::any();
        kani::assume(cap > 0 && cap <= 65536);
        let mut buf = SubscriberBufferGhost::new(cap);

        let msg_len: usize = kani::any();
        kani::assume(msg_len <= 65536);
        buf.callback_write(msg_len);

        if buf.has_data && !buf.overflow {
            assert!(buf.stored_len <= buf.buf_capacity);
        }
    }

    /// Overflow is set iff the message exceeded capacity.
    #[kani::proof]
    fn sub_callback_overflow_iff_too_large() {
        let cap: usize = kani::any();
        kani::assume(cap > 0 && cap <= 65536);
        let mut buf = SubscriberBufferGhost::new(cap);

        let msg_len: usize = kani::any();
        kani::assume(msg_len <= 65536);
        buf.callback_write(msg_len);

        assert_eq!(buf.overflow, msg_len > cap);
    }

    /// A locked buffer drops the callback — state unchanged.
    #[kani::proof]
    fn sub_locked_drops_callback() {
        let cap: usize = kani::any();
        kani::assume(cap > 0 && cap <= 65536);
        let mut buf = SubscriberBufferGhost::new(cap);

        // Put some data in the buffer first
        let first_len: usize = kani::any();
        kani::assume(first_len > 0 && first_len <= cap);
        buf.callback_write(first_len);

        // Start in-place read (sets locked=true mid-operation)
        // Simulate the lock window manually
        buf.locked = true;
        let snapshot_has_data = buf.has_data;
        let snapshot_overflow = buf.overflow;
        let snapshot_len = buf.stored_len;

        let msg_len: usize = kani::any();
        kani::assume(msg_len <= 65536);
        let accepted = buf.callback_write(msg_len);

        assert!(!accepted);
        assert_eq!(buf.has_data, snapshot_has_data);
        assert_eq!(buf.overflow, snapshot_overflow);
        assert_eq!(buf.stored_len, snapshot_len);

        buf.locked = false;
    }

    /// Overflow is never silently consumed — reading returns Overflow error.
    #[kani::proof]
    fn sub_overflow_detected_on_read() {
        let cap: usize = kani::any();
        kani::assume(cap > 0 && cap <= 65536);
        let mut buf = SubscriberBufferGhost::new(cap);

        let msg_len: usize = kani::any();
        kani::assume(msg_len > cap && msg_len <= 65536);
        buf.callback_write(msg_len);

        let result = buf.try_recv_raw(cap);
        assert_eq!(result, SubReadResult::Overflow);
    }

    /// Overflow detected via in-place path too.
    #[kani::proof]
    fn sub_overflow_detected_in_place() {
        let cap: usize = kani::any();
        kani::assume(cap > 0 && cap <= 65536);
        let mut buf = SubscriberBufferGhost::new(cap);

        let msg_len: usize = kani::any();
        kani::assume(msg_len > cap && msg_len <= 65536);
        buf.callback_write(msg_len);

        let result = buf.process_in_place();
        assert_eq!(result, SubReadResult::Overflow);
    }

    /// After successful read, buffer is ready for next message.
    #[kani::proof]
    fn sub_read_clears_state() {
        let cap: usize = kani::any();
        kani::assume(cap > 0 && cap <= 65536);
        let mut buf = SubscriberBufferGhost::new(cap);

        let msg_len: usize = kani::any();
        kani::assume(msg_len > 0 && msg_len <= cap);
        buf.callback_write(msg_len);
        assert!(buf.has_data);

        let result = buf.process_in_place();
        assert_eq!(result, SubReadResult::Ok);
        assert!(!buf.has_data);
        assert!(!buf.overflow);
        assert!(!buf.locked);
    }

    /// Reading from empty buffer returns Empty, not garbage.
    #[kani::proof]
    fn sub_empty_read_is_empty() {
        let cap: usize = kani::any();
        kani::assume(cap > 0 && cap <= 65536);
        let mut buf = SubscriberBufferGhost::new(cap);

        assert_eq!(buf.try_recv_raw(cap), SubReadResult::Empty);
        assert_eq!(buf.process_in_place(), SubReadResult::Empty);
    }

    /// Write–read–write–read cycle: buffer handles back-to-back correctly.
    #[kani::proof]
    fn sub_write_read_cycle() {
        let cap: usize = kani::any();
        kani::assume(cap > 0 && cap <= 65536);
        let mut buf = SubscriberBufferGhost::new(cap);

        let len1: usize = kani::any();
        kani::assume(len1 > 0 && len1 <= cap);
        buf.callback_write(len1);
        let r1 = buf.process_in_place();
        assert_eq!(r1, SubReadResult::Ok);

        let len2: usize = kani::any();
        kani::assume(len2 > 0 && len2 <= cap);
        buf.callback_write(len2);
        let r2 = buf.process_in_place();
        assert_eq!(r2, SubReadResult::Ok);
    }

    /// Overflow then normal: overflow is cleared by read, next write succeeds.
    #[kani::proof]
    fn sub_overflow_then_normal() {
        let cap: usize = kani::any();
        kani::assume(cap > 0 && cap <= 65536);
        let mut buf = SubscriberBufferGhost::new(cap);

        // Trigger overflow
        let big: usize = kani::any();
        kani::assume(big > cap && big <= 65536);
        buf.callback_write(big);
        let r1 = buf.process_in_place();
        assert_eq!(r1, SubReadResult::Overflow);

        // Normal write should now succeed
        let small: usize = kani::any();
        kani::assume(small > 0 && small <= cap);
        buf.callback_write(small);
        let r2 = buf.process_in_place();
        assert_eq!(r2, SubReadResult::Ok);
    }

    /// Buffer capacity is preserved across all operations.
    #[kani::proof]
    fn sub_capacity_immutable() {
        let cap: usize = kani::any();
        kani::assume(cap > 0 && cap <= 65536);
        let mut buf = SubscriberBufferGhost::new(cap);
        let original_cap = buf.buf_capacity;

        let msg_len: usize = kani::any();
        kani::assume(msg_len <= 65536);
        buf.callback_write(msg_len);
        assert_eq!(buf.buf_capacity, original_cap);

        let _ = buf.process_in_place();
        assert_eq!(buf.buf_capacity, original_cap);
    }

    // ------------------------------------------------------------------
    // ServiceBuffer invariants
    // ------------------------------------------------------------------

    /// After any callback, if has_request && !overflow, then stored_len <= buf_capacity.
    #[kani::proof]
    fn svc_callback_len_bounded() {
        let cap: usize = kani::any();
        kani::assume(cap > 0 && cap <= 65536);
        let mut buf = ServiceBufferGhost::new(cap);

        let msg_len: usize = kani::any();
        kani::assume(msg_len <= 65536);
        buf.callback_write(msg_len);

        if buf.has_request && !buf.overflow {
            assert!(buf.stored_len <= buf.buf_capacity);
        }
    }

    /// Service overflow is detected — never silently consumed.
    #[kani::proof]
    fn svc_overflow_detected() {
        let cap: usize = kani::any();
        kani::assume(cap > 0 && cap <= 65536);
        let mut buf = ServiceBufferGhost::new(cap);

        let msg_len: usize = kani::any();
        kani::assume(msg_len > cap && msg_len <= 65536);
        buf.callback_write(msg_len);

        let result = buf.try_recv_request();
        assert_eq!(result, SvcReadResult::Overflow);
    }

    /// Service read clears state for next request.
    #[kani::proof]
    fn svc_read_clears_state() {
        let cap: usize = kani::any();
        kani::assume(cap > 0 && cap <= 65536);
        let mut buf = ServiceBufferGhost::new(cap);

        let msg_len: usize = kani::any();
        kani::assume(msg_len > 0 && msg_len <= cap);
        buf.callback_write(msg_len);

        let result = buf.try_recv_request();
        assert_eq!(result, SvcReadResult::Ok);
        assert!(!buf.has_request);
        assert!(!buf.overflow);
    }

    /// Empty service buffer returns Empty.
    #[kani::proof]
    fn svc_empty_read_is_empty() {
        let cap: usize = kani::any();
        kani::assume(cap > 0 && cap <= 65536);
        let mut buf = ServiceBufferGhost::new(cap);

        assert_eq!(buf.try_recv_request(), SvcReadResult::Empty);
    }

    /// Service overflow→read→write→read cycle.
    #[kani::proof]
    fn svc_overflow_then_normal() {
        let cap: usize = kani::any();
        kani::assume(cap > 0 && cap <= 65536);
        let mut buf = ServiceBufferGhost::new(cap);

        let big: usize = kani::any();
        kani::assume(big > cap && big <= 65536);
        buf.callback_write(big);
        let r1 = buf.try_recv_request();
        assert_eq!(r1, SvcReadResult::Overflow);

        let small: usize = kani::any();
        kani::assume(small > 0 && small <= cap);
        buf.callback_write(small);
        let r2 = buf.try_recv_request();
        assert_eq!(r2, SvcReadResult::Ok);
    }

    /// Service capacity is preserved across all operations.
    #[kani::proof]
    fn svc_capacity_immutable() {
        let cap: usize = kani::any();
        kani::assume(cap > 0 && cap <= 65536);
        let mut buf = ServiceBufferGhost::new(cap);
        let original_cap = buf.buf_capacity;

        let msg_len: usize = kani::any();
        kani::assume(msg_len <= 65536);
        buf.callback_write(msg_len);
        assert_eq!(buf.buf_capacity, original_cap);

        let _ = buf.try_recv_request();
        assert_eq!(buf.buf_capacity, original_cap);
    }

    // ------------------------------------------------------------------
    // StagingBuffer invariants (Phase 56.3)
    // ------------------------------------------------------------------

    /// After recv, `rx_pos <= rx_len <= capacity`.
    #[kani::proof]
    fn staging_invariant_after_recv() {
        let cap: usize = kani::any();
        kani::assume(cap > 0 && cap <= 256);
        let mut buf = StagingBufferGhost::new(cap);

        // Fill some data
        let fill: usize = kani::any();
        kani::assume(fill <= cap);
        buf.rx_len = fill;

        // Partially consume
        let consume_pos: usize = kani::any();
        kani::assume(consume_pos <= fill);
        buf.rx_pos = consume_pos;

        let user_len: usize = kani::any();
        kani::assume(user_len > 0 && user_len <= cap);
        buf.recv(user_len);

        assert!(buf.rx_pos <= buf.rx_len);
        assert!(buf.rx_len <= buf.capacity);
    }

    /// After send, `tx_pos <= tx_len <= capacity`.
    #[kani::proof]
    fn staging_invariant_after_send() {
        let cap: usize = kani::any();
        kani::assume(cap > 0 && cap <= 256);
        let mut buf = StagingBufferGhost::new(cap);

        // Pre-existing TX data
        let existing: usize = kani::any();
        kani::assume(existing <= cap);
        buf.tx_len = existing;

        let data_len: usize = kani::any();
        kani::assume(data_len > 0 && data_len <= cap);
        buf.send(data_len);

        assert!(buf.tx_pos <= buf.tx_len);
        assert!(buf.tx_len <= buf.capacity);
    }

    /// Compaction preserves data length and resets rx_pos to 0.
    #[kani::proof]
    fn staging_compact_preserves_data_length() {
        let cap: usize = kani::any();
        kani::assume(cap > 0 && cap <= 256);
        let mut buf = StagingBufferGhost::new(cap);

        let rx_len: usize = kani::any();
        kani::assume(rx_len <= cap);
        buf.rx_len = rx_len;

        let rx_pos: usize = kani::any();
        kani::assume(rx_pos <= rx_len);
        buf.rx_pos = rx_pos;

        let old_available = buf.rx_len - buf.rx_pos;
        buf.compact_rx();

        assert_eq!(buf.rx_pos, 0);
        assert_eq!(buf.rx_len, old_available);
        assert!(buf.rx_len <= buf.capacity);
    }

    /// If data is available (rx_len > rx_pos), recv returns > 0.
    #[kani::proof]
    fn staging_recv_progress() {
        let cap: usize = kani::any();
        kani::assume(cap > 0 && cap <= 256);
        let mut buf = StagingBufferGhost::new(cap);

        let rx_len: usize = kani::any();
        kani::assume(rx_len > 0 && rx_len <= cap);
        buf.rx_len = rx_len;

        let rx_pos: usize = kani::any();
        kani::assume(rx_pos < rx_len); // strictly less → data available
        buf.rx_pos = rx_pos;

        let user_len: usize = kani::any();
        kani::assume(user_len > 0 && user_len <= cap);
        let copied = buf.recv(user_len);

        assert!(copied > 0);
    }

    /// If space is available (tx_len < capacity), send returns > 0.
    #[kani::proof]
    fn staging_send_progress() {
        let cap: usize = kani::any();
        kani::assume(cap > 0 && cap <= 256);
        let mut buf = StagingBufferGhost::new(cap);

        let tx_len: usize = kani::any();
        kani::assume(tx_len < cap); // strictly less → space available
        buf.tx_len = tx_len;

        let data_len: usize = kani::any();
        kani::assume(data_len > 0 && data_len <= cap);
        let copied = buf.send(data_len);

        assert!(copied > 0);
    }

    /// Full send→drain→fill→recv cycle preserves all invariants.
    #[kani::proof]
    fn staging_full_cycle() {
        let cap: usize = kani::any();
        kani::assume(cap > 0 && cap <= 64);
        let mut buf = StagingBufferGhost::new(cap);

        // Step 1: Application sends data
        let send_len: usize = kani::any();
        kani::assume(send_len > 0 && send_len <= cap);
        let sent = buf.send(send_len);
        assert!(sent > 0);
        assert!(buf.tx_pos <= buf.tx_len);
        assert!(buf.tx_len <= buf.capacity);

        // Step 2: Poll drains TX to socket
        let drain_amount: usize = kani::any();
        kani::assume(drain_amount <= buf.tx_len - buf.tx_pos);
        buf.drain_tx(drain_amount);
        assert!(buf.tx_pos <= buf.tx_len);
        assert!(buf.tx_len <= buf.capacity);

        // Step 3: Poll compacts RX + fills from socket
        buf.compact_rx();
        let received: usize = kani::any();
        kani::assume(received <= buf.capacity - buf.rx_len);
        buf.fill_rx(received);
        assert!(buf.rx_pos <= buf.rx_len);
        assert!(buf.rx_len <= buf.capacity);

        // Step 4: Application receives data
        if buf.rx_len > buf.rx_pos {
            let user_len: usize = kani::any();
            kani::assume(user_len > 0 && user_len <= cap);
            let copied = buf.recv(user_len);
            assert!(copied > 0);
        }
        assert!(buf.rx_pos <= buf.rx_len);
        assert!(buf.rx_len <= buf.capacity);
    }

    /// rx_pos + available data never exceeds capacity; tx_len never exceeds capacity.
    #[kani::proof]
    fn staging_no_overlap() {
        let cap: usize = kani::any();
        kani::assume(cap > 0 && cap <= 256);
        let mut buf = StagingBufferGhost::new(cap);

        // Arbitrary valid state
        let rx_pos: usize = kani::any();
        let rx_len: usize = kani::any();
        kani::assume(rx_pos <= rx_len && rx_len <= cap);
        buf.rx_pos = rx_pos;
        buf.rx_len = rx_len;

        // Compact + fill
        buf.compact_rx();
        let received: usize = kani::any();
        kani::assume(received <= buf.capacity - buf.rx_len);
        buf.fill_rx(received);

        assert!(buf.rx_len <= buf.capacity);
        assert!(buf.rx_pos <= buf.rx_len);

        // TX invariant
        let tx_len: usize = kani::any();
        kani::assume(tx_len <= cap);
        buf.tx_len = tx_len;
        let data_len: usize = kani::any();
        kani::assume(data_len <= cap);
        buf.send(data_len);

        assert!(buf.tx_len <= buf.capacity);
    }

    /// When rx_pos == rx_len (empty), recv returns 0.
    #[kani::proof]
    fn staging_empty_recv_returns_zero() {
        let cap: usize = kani::any();
        kani::assume(cap > 0 && cap <= 256);
        let mut buf = StagingBufferGhost::new(cap);

        // Set rx_pos == rx_len (empty buffer — either both 0 or both equal)
        let pos: usize = kani::any();
        kani::assume(pos <= cap);
        buf.rx_pos = pos;
        buf.rx_len = pos;

        let user_len: usize = kani::any();
        kani::assume(user_len > 0 && user_len <= cap);
        let copied = buf.recv(user_len);

        assert_eq!(copied, 0);
    }

    /// When tx_len == capacity (full), send returns 0.
    #[kani::proof]
    fn staging_full_send_returns_zero() {
        let cap: usize = kani::any();
        kani::assume(cap > 0 && cap <= 256);
        let mut buf = StagingBufferGhost::new(cap);

        buf.tx_len = cap; // buffer is full

        let data_len: usize = kani::any();
        kani::assume(data_len > 0 && data_len <= cap);
        let copied = buf.send(data_len);

        assert_eq!(copied, 0);
    }

    // ------------------------------------------------------------------
    // Ephemeral port invariants (Phase 56.4)
    // ------------------------------------------------------------------

    /// For any input, the result is in [49152, 65535].
    #[kani::proof]
    fn ephemeral_port_stays_in_range() {
        let current: u16 = kani::any();
        let next = ephemeral_port_next(current);
        assert!(next >= EPHEMERAL_PORT_START);
        // u16::MAX == 65535, so next <= 65535 is guaranteed by the type
    }

    /// When current == 65535 (u16::MAX), wrapping_add(1) overflows to 0,
    /// which is < EPHEMERAL_PORT_START, so result is EPHEMERAL_PORT_START.
    #[kani::proof]
    fn ephemeral_port_wraps_correctly() {
        let next = ephemeral_port_next(65535);
        assert_eq!(next, EPHEMERAL_PORT_START);
    }

    /// When current < 65535 and current >= EPHEMERAL_PORT_START - 1,
    /// the result is current + 1 (no wrap needed).
    #[kani::proof]
    fn ephemeral_port_increments() {
        let current: u16 = kani::any();
        kani::assume(current < 65535);
        let next = ephemeral_port_next(current);
        if current >= EPHEMERAL_PORT_START - 1 {
            assert_eq!(next, current + 1);
        } else {
            // Below range: floor kicks in
            assert_eq!(next, EPHEMERAL_PORT_START);
        }
    }
}

// ======================================================================
// Publish Call Chain
// ======================================================================

/// Ghost model for the publish call chain.
///
/// Models the result of each layer in the publish path:
///
/// Source (nros-node/src/executor/handles.rs):
/// ```ignore
/// pub fn publish_with_buffer<const BUF: usize>(...) -> Result<(), NodeError> {
///     let mut writer = CdrWriter::new_with_header(&mut buffer)
///         .map_err(|_| NodeError::BufferTooSmall)?;
///     msg.serialize(&mut writer)
///         .map_err(|_| NodeError::Serialization)?;
///     self.publisher.publish_raw(&buffer[..len]).map_err(|e| e.into())
/// }
/// ```
pub struct PublishChainGhost {
    /// Whether CdrWriter::new_with_header succeeded
    pub header_ok: bool,
    /// Whether msg.serialize() succeeded
    pub serialize_ok: bool,
    /// Whether publisher.publish_raw() succeeded
    pub publish_raw_ok: bool,
}

// ======================================================================
// Executor / spin_once
// ======================================================================

/// Ghost model of `spin_once()` control flow.
///
/// Models the two execution paths in `PollingExecutor::spin_once()`
/// (executor.rs:1178-1229):
///
/// ```ignore
/// fn spin_once(&mut self, delta_ms: u64) -> SpinOnceResult {
///     if !self.trigger.evaluate(&ready_mask) {
///         // PATH A: trigger false → only timers
///     }
///     // PATH B: trigger true → subs + services + timers
/// }
/// ```
pub struct SpinOnceGhost {
    /// Whether the trigger evaluated to true
    pub trigger_result: bool,
    /// Number of subscriptions processed (0 if trigger false)
    pub subs_processed: usize,
    /// Number of services handled (0 if trigger false)
    pub services_handled: usize,
    /// Number of timers fired (always processed)
    pub timers_fired: usize,
}

// ======================================================================
// Executor Progress (Phase 37.3)
// ======================================================================

/// Ghost model of `SpinOnceResult` with error counters.
///
/// Extends `SpinOnceGhost` with the error counters added in Phase 37.1b.
/// Used by progress proofs to verify no-silent-data-loss invariant.
///
/// Source (executor.rs:77-88):
/// ```ignore
/// pub struct SpinOnceResult {
///     pub subscriptions_processed: usize,
///     pub timers_fired: usize,
///     pub services_handled: usize,
///     pub subscription_errors: usize,
///     pub service_errors: usize,
/// }
/// ```
pub struct SpinOnceResultGhost {
    /// Number of subscriptions processed successfully
    pub subs_processed: usize,
    /// Number of timers fired
    pub timers_fired: usize,
    /// Number of services handled successfully
    pub services_handled: usize,
    /// Number of subscription processing errors
    pub sub_errors: usize,
    /// Number of service processing errors
    pub svc_errors: usize,
}

// ======================================================================
// Timer State Machine
// ======================================================================

/// Ghost model of timer mode (mirrors `nros_node::timer::TimerMode`).
///
/// Source (timer.rs):
/// ```ignore
/// pub enum TimerMode {
///     Repeating,
///     OneShot,
///     Inert,
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerModeGhost {
    Repeating,
    OneShot,
    Inert,
}

/// Ghost model of timer state (mirrors `nros_node::timer::TimerState`).
///
/// Only includes the fields relevant to scheduling correctness — callbacks are
/// excluded because they don't affect when/whether a timer fires.
///
/// Source (timer.rs):
/// ```ignore
/// pub(crate) struct TimerState {
///     period_ms: u64,
///     elapsed_ms: u64,
///     mode: TimerMode,
///     canceled: bool,
///     // callback omitted
/// }
/// ```
pub struct TimerGhost {
    /// Timer period in milliseconds
    pub period_ms: u64,
    /// Elapsed time since last fire
    pub elapsed_ms: u64,
    /// Timer mode (repeating, one-shot, or inert)
    pub mode: TimerModeGhost,
    /// Whether the timer has been canceled
    pub canceled: bool,
}

// ======================================================================
// Parameter Server
// ======================================================================

/// Ghost model of `ParameterServer` state.
///
/// Mirrors private fields in `nros-params/src/server.rs`:
///
/// Source (server.rs:47-52):
/// ```ignore
/// pub struct ParameterServer {
///     entries: [Option<ParameterEntry>; MAX_PARAMETERS],
///     count: usize,
/// }
/// ```
///
/// `MAX_PARAMETERS = 32` (server.rs:13).
pub struct ParamServerGhost {
    /// Number of parameters currently stored
    pub count: usize,
    /// Maximum parameter capacity
    pub max: usize,
}

/// Ghost model of `ParameterValue` discriminant structure.
///
/// Mirrors 10 variants from `nros-params/src/types.rs:52-81`.
/// Array and string payloads are abstracted (heapless types not importable
/// into Verus). Scalar payloads (bool, i64) are preserved for roundtrip proofs.
///
/// Source (types.rs:52-81):
/// ```ignore
/// pub enum ParameterValue {
///     NotSet, Bool(bool), Integer(i64), Double(f64),
///     String(...), ByteArray(...), BoolArray(...),
///     IntegerArray(...), DoubleArray(...), StringArray(...),
/// }
/// ```
pub enum ParameterValueGhost {
    NotSet,
    Bool(bool),
    Integer(i64),
    /// f64 payload abstracted (Verus has no f64 support)
    Double,
    /// heapless::String payload abstracted
    String,
    /// `heapless::Vec<u8>` payload abstracted
    ByteArray,
    /// `heapless::Vec<bool>` payload abstracted
    BoolArray,
    /// `heapless::Vec<i64>` payload abstracted
    IntegerArray,
    /// `heapless::Vec<f64>` payload abstracted
    DoubleArray,
    /// `heapless::Vec<String>` payload abstracted
    StringArray,
}

// ======================================================================
// E2E Safety Protocol
// ======================================================================

/// Ghost model of `IntegrityStatus` (nros-rmw/src/safety.rs).
///
/// `crc_valid: Option<bool>` is decomposed into two bools since ghost types
/// use only primitives (no Option).
///
/// Source (safety.rs:61-73):
/// ```ignore
/// pub struct IntegrityStatus {
///     pub gap: i64,
///     pub duplicate: bool,
///     pub crc_valid: Option<bool>,
/// }
/// ```
pub struct IntegrityStatusGhost {
    /// Sequence gap (0 = normal, >0 = messages lost)
    pub gap: i64,
    /// True if this message is a duplicate or out-of-order
    pub duplicate: bool,
    /// Models `crc_valid.is_some()`
    pub crc_known: bool,
    /// Models `crc_valid == Some(true)` (only meaningful when crc_known)
    pub crc_ok: bool,
}

/// Ghost model of `SafetyValidator` (nros-rmw/src/safety.rs).
///
/// Source (safety.rs:92-97):
/// ```ignore
/// pub struct SafetyValidator {
///     expected_seq: i64,
///     initialized: bool,
/// }
/// ```
pub struct SafetyValidatorGhost {
    /// Next expected sequence number
    pub expected_seq: i64,
    /// Whether we've received at least one message
    pub initialized: bool,
}

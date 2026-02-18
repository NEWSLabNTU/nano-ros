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
    /// heapless::Vec<u8> payload abstracted
    ByteArray,
    /// heapless::Vec<bool> payload abstracted
    BoolArray,
    /// heapless::Vec<i64> payload abstracted
    IntegerArray,
    /// heapless::Vec<f64> payload abstracted
    DoubleArray,
    /// heapless::Vec<String> payload abstracted
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

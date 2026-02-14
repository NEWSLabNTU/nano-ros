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
/// Source (shim.rs:853-876):
/// ```ignore
/// struct SubscriberBuffer {
///     data: [u8; 1024],
///     has_data: AtomicBool,
///     overflow: AtomicBool,
///     len: AtomicUsize,
///     // ... attachment fields omitted ...
/// }
/// ```
pub struct SubscriberBufferGhost {
    /// Whether the buffer contains unprocessed data
    pub has_data: bool,
    /// Whether the last callback detected a message exceeding buffer capacity
    pub overflow: bool,
    /// Length of valid payload data in the buffer
    pub stored_len: usize,
    /// Static buffer capacity (always 1024 in production)
    pub buf_capacity: usize,
}

// ======================================================================
// Publish Call Chain
// ======================================================================

/// Ghost model for the publish call chain.
///
/// Models the result of each layer in the publish path:
///
/// Source (nros-node/src/shim.rs:596-609):
/// ```ignore
/// pub fn publish_with_buffer<const BUF: usize>(...) -> Result<(), ShimNodeError> {
///     let mut writer = CdrWriter::new_with_header(&mut buffer)
///         .map_err(|_| ShimNodeError::BufferTooSmall)?;
///     msg.serialize(&mut writer)
///         .map_err(|_| ShimNodeError::Serialization)?;
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

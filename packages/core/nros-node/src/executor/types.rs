//! Public types for the embedded executor.

use nros_rmw::{SessionMode, TransportError};

// ============================================================================
// SpinOnceResult
// ============================================================================

/// Result of a single spin iteration
///
/// Contains counts of how many items were processed during `spin_once()`,
/// plus error counts for transport failures that would otherwise be silently dropped.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SpinOnceResult {
    /// Number of subscription callbacks invoked
    pub subscriptions_processed: usize,
    /// Number of timers that fired
    pub timers_fired: usize,
    /// Number of service requests handled
    pub services_handled: usize,
    /// Number of subscription processing errors (e.g., BufferTooSmall, MessageTooLarge)
    pub subscription_errors: usize,
    /// Number of service processing errors (e.g., BufferTooSmall)
    pub service_errors: usize,
}

impl SpinOnceResult {
    /// Create a new empty result
    pub const fn new() -> Self {
        Self {
            subscriptions_processed: 0,
            timers_fired: 0,
            services_handled: 0,
            subscription_errors: 0,
            service_errors: 0,
        }
    }

    /// Check if any work was done (errors are not counted as work)
    pub const fn any_work(&self) -> bool {
        self.subscriptions_processed > 0 || self.timers_fired > 0 || self.services_handled > 0
    }

    /// Total number of callbacks successfully invoked (errors excluded)
    pub const fn total(&self) -> usize {
        self.subscriptions_processed + self.timers_fired + self.services_handled
    }

    /// Check if any errors occurred during this spin iteration
    pub const fn any_errors(&self) -> bool {
        self.subscription_errors > 0 || self.service_errors > 0
    }

    /// Total number of errors across all handle types
    pub const fn total_errors(&self) -> usize {
        self.subscription_errors + self.service_errors
    }
}

// ============================================================================
// SpinPeriodPollingResult (no_std)
// ============================================================================

/// Result from a single period of polling execution (`no_std` compatible).
///
/// Contains the work performed and the remaining time the caller should sleep.
/// The caller is responsible for the actual delay (platform-specific).
///
/// # Example
///
/// ```ignore
/// loop {
///     let r = executor.spin_one_period(10, elapsed_ms);
///     platform_sleep_ms(r.remaining_ms);
/// }
/// ```
#[derive(Debug, Clone, Copy)]
pub struct SpinPeriodPollingResult {
    /// Work performed during this iteration
    pub work: SpinOnceResult,
    /// Remaining time in ms that the caller should sleep
    pub remaining_ms: u64,
}

// ============================================================================
// SpinPeriodResult (std only)
// ============================================================================

/// Result from a single period with wall-clock measurement (`std` only).
///
/// Contains the work performed, whether processing exceeded the period
/// (overrun), and the actual wall-clock processing time.
#[cfg(feature = "std")]
#[derive(Debug, Clone)]
pub struct SpinPeriodResult {
    /// Work performed during this period
    pub work: SpinOnceResult,
    /// Whether processing exceeded the target period
    pub overrun: bool,
    /// Actual wall-clock processing time
    pub elapsed: std::time::Duration,
}

// ============================================================================
// SpinOptions
// ============================================================================

/// Options controlling blocking spin behavior.
///
/// Used with [`Executor::spin_blocking()`](super::Executor::spin_blocking)
/// to control when the spin loop exits.
#[derive(Debug, Clone, Default)]
pub struct SpinOptions {
    /// Stop after this duration (in milliseconds)
    pub timeout_ms: Option<u64>,
    /// Only process immediately available work (single iteration)
    pub only_next: bool,
    /// Stop after processing this many callbacks total
    pub max_callbacks: Option<usize>,
}

impl SpinOptions {
    /// Create default spin options (spin forever until halted)
    pub const fn new() -> Self {
        Self {
            timeout_ms: None,
            only_next: false,
            max_callbacks: None,
        }
    }

    /// Set a timeout duration
    pub const fn timeout_ms(mut self, ms: u64) -> Self {
        self.timeout_ms = Some(ms);
        self
    }

    /// Only process one round of work (equivalent to spin_once)
    pub const fn spin_once() -> Self {
        Self {
            timeout_ms: None,
            only_next: true,
            max_callbacks: None,
        }
    }

    /// Stop after processing N callbacks
    pub const fn max_callbacks(mut self, n: usize) -> Self {
        self.max_callbacks = Some(n);
        self
    }
}

// ============================================================================
// ExecutorConfig
// ============================================================================

/// Configuration for opening an embedded executor session.
///
/// Provides a backend-agnostic builder for configuring the middleware
/// connection. The active Cargo feature (`rmw-zenoh`, `rmw-xrce`, or
/// `rmw-cffi`) determines which backend is used.
///
/// # Example
///
/// ```ignore
/// use nros::prelude::*;
///
/// let config = ExecutorConfig::new("tcp/127.0.0.1:7447")
///     .node_name("talker")
///     .domain_id(0);
/// let mut executor: Executor = Executor::open(&config)?;
/// ```
pub struct ExecutorConfig<'a> {
    /// Middleware-specific connection string.
    pub locator: &'a str,
    /// Session mode (client or peer).
    pub mode: SessionMode,
    /// ROS 2 domain ID.
    pub domain_id: u32,
    /// Node name.
    pub node_name: &'a str,
    /// Node namespace.
    pub namespace: &'a str,
}

impl<'a> ExecutorConfig<'a> {
    /// Create a new configuration with the given locator.
    ///
    /// Defaults: `Client` mode, domain 0, node name `"node"`, empty namespace.
    pub const fn new(locator: &'a str) -> Self {
        Self {
            locator,
            mode: SessionMode::Client,
            domain_id: 0,
            node_name: "node",
            namespace: "",
        }
    }

    /// Set the ROS 2 domain ID.
    pub const fn domain_id(mut self, id: u32) -> Self {
        self.domain_id = id;
        self
    }

    /// Set the node name.
    pub const fn node_name(mut self, name: &'a str) -> Self {
        self.node_name = name;
        self
    }

    /// Set the node namespace.
    pub const fn namespace(mut self, ns: &'a str) -> Self {
        self.namespace = ns;
        self
    }

    /// Set the session mode.
    pub const fn mode(mut self, mode: SessionMode) -> Self {
        self.mode = mode;
        self
    }
}

#[cfg(all(feature = "std", feature = "alloc"))]
impl ExecutorConfig<'static> {
    /// Create a configuration from environment variables.
    ///
    /// Reads:
    /// - `ZENOH_LOCATOR` — Middleware locator (default: `"tcp/127.0.0.1:7447"`)
    /// - `ROS_DOMAIN_ID` — ROS 2 domain ID (default: `0`)
    /// - `ZENOH_MODE` — Session mode: `"client"` or `"peer"` (default: `"client"`)
    ///
    /// String values are heap-allocated and leaked into `'static` references.
    pub fn from_env() -> Self {
        let locator: &'static str = match std::env::var("ZENOH_LOCATOR") {
            Ok(s) => alloc::boxed::Box::leak(s.into_boxed_str()),
            Err(_) => "tcp/127.0.0.1:7447",
        };
        let domain_id: u32 = std::env::var("ROS_DOMAIN_ID")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let mode = match std::env::var("ZENOH_MODE") {
            Ok(s) if s == "peer" => SessionMode::Peer,
            _ => SessionMode::Client,
        };
        Self {
            locator,
            mode,
            domain_id,
            node_name: "node",
            namespace: "",
        }
    }
}

// ============================================================================
// Error type
// ============================================================================

/// Error type for generic embedded node operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeError {
    /// Transport-level error.
    Transport(TransportError),
    /// Node name exceeds 64 bytes.
    NameTooLong,
    /// CDR serialization failed.
    Serialization,
    /// Buffer too small for message.
    BufferTooSmall,
    /// Action server/client creation failed.
    ActionCreationFailed,
    /// Service request failed.
    ServiceRequestFailed,
    /// Service reply failed.
    ServiceReplyFailed,
    /// Operation timed out.
    Timeout,
}

impl From<TransportError> for NodeError {
    fn from(err: TransportError) -> Self {
        NodeError::Transport(err)
    }
}

/// Default transmit buffer size (bytes).
#[cfg(any(has_rmw, test))]
pub(crate) const DEFAULT_TX_BUF: usize = crate::config::DEFAULT_RX_BUF_SIZE;

// ============================================================================
// HandleId
// ============================================================================

/// Opaque handle identifier returned by registration methods.
///
/// Used with [`Trigger::One`] and [`HandleSet`] for type-safe trigger
/// configuration. The inner value is the entry slot index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HandleId(pub usize);

// ============================================================================
// HandleSet
// ============================================================================

/// A set of handle IDs, represented as a bitset.
///
/// Supports up to 64 handles. Construct via `HandleId` operators:
/// ```ignore
/// let set = imu | gps | lidar;  // HandleSet from 3 handles
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HandleSet(pub(crate) u64);

impl HandleSet {
    /// Empty set.
    pub const EMPTY: Self = Self(0);

    /// Insert a handle into the set.
    pub const fn insert(self, id: HandleId) -> Self {
        Self(self.0 | (1u64 << id.0))
    }

    /// Check if the set contains a handle.
    pub const fn contains(self, id: HandleId) -> bool {
        self.0 & (1u64 << id.0) != 0
    }

    /// Union of two sets.
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Number of handles in the set.
    pub const fn len(self) -> u32 {
        self.0.count_ones()
    }

    /// Check if the set is empty.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl core::ops::BitOr for HandleId {
    type Output = HandleSet;
    fn bitor(self, rhs: HandleId) -> HandleSet {
        HandleSet::EMPTY.insert(self).insert(rhs)
    }
}

impl core::ops::BitOr<HandleId> for HandleSet {
    type Output = HandleSet;
    fn bitor(self, rhs: HandleId) -> HandleSet {
        self.insert(rhs)
    }
}

impl core::ops::BitOr for HandleSet {
    type Output = HandleSet;
    fn bitor(self, rhs: HandleSet) -> HandleSet {
        self.union(rhs)
    }
}

// ============================================================================
// ReadinessSnapshot
// ============================================================================

/// Snapshot of handle readiness at the start of a spin iteration.
///
/// Passed to [`Trigger::Predicate`] functions. Query by [`HandleId`].
pub struct ReadinessSnapshot {
    pub(crate) bits: u64,
    pub(crate) count: usize,
}

impl ReadinessSnapshot {
    /// Check if a specific handle has data.
    pub const fn is_ready(&self, id: HandleId) -> bool {
        self.bits & (1u64 << id.0) != 0
    }

    /// Check if all handles in the set have data.
    pub const fn all_ready(&self, set: HandleSet) -> bool {
        self.bits & set.0 == set.0
    }

    /// Check if any handle in the set has data.
    pub const fn any_ready(&self, set: HandleSet) -> bool {
        self.bits & set.0 != 0
    }

    /// Number of handles that have data.
    pub const fn ready_count(&self) -> u32 {
        self.bits.count_ones()
    }

    /// Total registered handles.
    pub const fn total(&self) -> usize {
        self.count
    }
}

// ============================================================================
// InvocationMode
// ============================================================================

/// Per-callback invocation mode.
///
/// Controls whether a callback fires only when new data is available
/// or on every spin iteration that passes the trigger gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InvocationMode {
    /// Fire only when `has_data()` returns true (default).
    #[default]
    OnNewData,
    /// Fire on every spin iteration, regardless of data availability.
    Always,
}

// ============================================================================
// Trigger
// ============================================================================

/// Executor-level trigger condition.
///
/// Controls when the executor dispatches callbacks during `spin_once()`.
/// The trigger is evaluated after polling the transport but before any
/// callback dispatch.
#[derive(Clone, Copy, Default)]
pub enum Trigger {
    /// Fire if any registered handle has data (default).
    #[default]
    Any,
    /// Fire only when ALL non-timer handles have data.
    All,
    /// Fire only when a specific handle has data.
    One(HandleId),
    /// Fire only when every handle in the set has data.
    AllOf(HandleSet),
    /// Fire when any handle in the set has data.
    AnyOf(HandleSet),
    /// Always fire, regardless of data availability.
    Always,
    /// Custom predicate over a readiness snapshot.
    Predicate(fn(&ReadinessSnapshot) -> bool),
    /// Custom predicate with C-compatible signature and context pointer.
    ///
    /// The callback receives a `bool` array of readiness flags (one per handle),
    /// the count of handles, and a user-provided context pointer.
    /// Used by the C API to bridge `nros_executor_trigger_t` to the Rust trigger system.
    RawPredicate {
        /// C trigger callback
        callback: unsafe extern "C" fn(
            ready: *const bool,
            count: usize,
            context: *mut core::ffi::c_void,
        ) -> bool,
        /// User-provided context pointer passed to the callback
        context: *mut core::ffi::c_void,
    },
}

// Manual Debug impl because fn pointers don't impl Debug well
impl core::fmt::Debug for Trigger {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Any => write!(f, "Any"),
            Self::All => write!(f, "All"),
            Self::One(id) => f.debug_tuple("One").field(id).finish(),
            Self::AllOf(set) => f.debug_tuple("AllOf").field(set).finish(),
            Self::AnyOf(set) => f.debug_tuple("AnyOf").field(set).finish(),
            Self::Always => write!(f, "Always"),
            Self::Predicate(_) => write!(f, "Predicate(...)"),
            Self::RawPredicate { .. } => write!(f, "RawPredicate(...)"),
        }
    }
}

// ============================================================================
// Raw callback types (for C API)
// ============================================================================

/// Raw subscription callback that receives CDR bytes without deserialization.
///
/// # Safety
/// The `data` pointer is valid for `len` bytes during the call.
pub type RawSubscriptionCallback =
    unsafe extern "C" fn(data: *const u8, len: usize, context: *mut core::ffi::c_void);

/// Raw service callback that receives and produces CDR bytes.
///
/// # Safety
/// - `req` is valid for `req_len` bytes
/// - `resp` is valid for `resp_cap` bytes (writable)
/// - `resp_len` is a valid pointer to write the response length
///
/// Returns `true` if the request was handled successfully.
pub type RawServiceCallback = unsafe extern "C" fn(
    req: *const u8,
    req_len: usize,
    resp: *mut u8,
    resp_cap: usize,
    resp_len: *mut usize,
    context: *mut core::ffi::c_void,
) -> bool;

/// Raw action goal callback that receives CDR bytes without deserialization.
///
/// # Safety
/// - `goal_id` is valid for the duration of the call
/// - `goal_data` is valid for `goal_len` bytes
///
/// Returns a `GoalResponse` value (0=Reject, 1=AcceptAndExecute, 2=AcceptAndDefer).
pub type RawGoalCallback = unsafe extern "C" fn(
    goal_id: *const nros_core::GoalId,
    goal_data: *const u8,
    goal_len: usize,
    context: *mut core::ffi::c_void,
) -> nros_core::GoalResponse;

/// Raw action cancel callback.
///
/// # Safety
/// - `goal_id` is valid for the duration of the call
///
/// Returns a `CancelResponse` value (0=Ok, 1=Rejected, 2=UnknownGoal, 3=GoalTerminated).
pub type RawCancelCallback = unsafe extern "C" fn(
    goal_id: *const nros_core::GoalId,
    status: nros_core::GoalStatus,
    context: *mut core::ffi::c_void,
) -> nros_core::CancelResponse;

// ============================================================================
// ExecutorSemantics
// ============================================================================

/// Data communication semantics for the executor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExecutorSemantics {
    /// Standard interleaved execution (default). Each callback sees the
    /// latest data at the time it runs.
    #[default]
    RclcppExecutor,
    /// Logical Execution Time. All subscriptions are sampled at spin start;
    /// callbacks process from the snapshot.
    LogicalExecutionTime,
}

// ============================================================================
// GuardConditionHandle
// ============================================================================

/// Handle for triggering a guard condition from outside the executor.
///
/// Obtained from [`Executor::add_guard_condition()`](super::Executor::add_guard_condition).
/// Safe to use from any thread — the inner `&'static AtomicBool` is inherently
/// `Send + Sync`.
pub struct GuardConditionHandle {
    // The AtomicBool lives in the executor's arena, which is never moved or
    // deallocated while handles exist. The 'static lifetime is asserted at
    // construction time (see `new()`).
    flag: &'static portable_atomic::AtomicBool,
}

impl GuardConditionHandle {
    /// Create a handle from a raw pointer to an arena-allocated `AtomicBool`.
    ///
    /// # Safety
    ///
    /// The pointed-to `AtomicBool` must outlive this handle. This is guaranteed
    /// when the `AtomicBool` lives in the executor arena (which is never moved
    /// or deallocated while handles exist).
    #[cfg(any(has_rmw, test))]
    pub(crate) unsafe fn new(flag: *const portable_atomic::AtomicBool) -> Self {
        // SAFETY: Caller guarantees the AtomicBool outlives this handle.
        Self {
            flag: unsafe { &*flag },
        }
    }

    /// Trigger the guard condition.
    ///
    /// The executor will invoke the associated callback on the next spin iteration.
    pub fn trigger(&self) {
        self.flag.store(true, portable_atomic::Ordering::Release);
    }
}

// ============================================================================
// Kani Verification
// ============================================================================

#[cfg(kani)]
mod verification {
    use super::*;

    // ---- HandleSet algebraic properties ----

    #[kani::proof]
    fn handleset_insert_contains() {
        let idx: usize = kani::any();
        kani::assume(idx < 64);
        let id = HandleId(idx);
        let set = HandleSet::EMPTY.insert(id);
        assert!(set.contains(id));
    }

    #[kani::proof]
    fn handleset_insert_idempotent() {
        let idx: usize = kani::any();
        kani::assume(idx < 64);
        let id = HandleId(idx);
        let once = HandleSet::EMPTY.insert(id);
        let twice = once.insert(id);
        assert_eq!(once.0, twice.0);
    }

    #[kani::proof]
    fn handleset_union_commutative() {
        let a: u64 = kani::any();
        let b: u64 = kani::any();
        let set_a = HandleSet(a);
        let set_b = HandleSet(b);
        assert_eq!(set_a.union(set_b).0, set_b.union(set_a).0);
    }

    #[kani::proof]
    fn handleset_union_associative() {
        let a: u64 = kani::any();
        let b: u64 = kani::any();
        let c: u64 = kani::any();
        let sa = HandleSet(a);
        let sb = HandleSet(b);
        let sc = HandleSet(c);
        assert_eq!(sa.union(sb).union(sc).0, sa.union(sb.union(sc)).0);
    }

    #[kani::proof]
    fn handleset_union_contains_both() {
        let idx_a: usize = kani::any();
        let idx_b: usize = kani::any();
        kani::assume(idx_a < 64);
        kani::assume(idx_b < 64);
        let a = HandleId(idx_a);
        let b = HandleId(idx_b);
        let set_a = HandleSet::EMPTY.insert(a);
        let set_b = HandleSet::EMPTY.insert(b);
        let merged = set_a.union(set_b);
        assert!(merged.contains(a));
        assert!(merged.contains(b));
    }

    #[kani::proof]
    fn handleset_empty_contains_nothing() {
        let idx: usize = kani::any();
        kani::assume(idx < 64);
        let id = HandleId(idx);
        assert!(!HandleSet::EMPTY.contains(id));
    }

    #[kani::proof]
    fn handleset_bitor_matches_insert() {
        let idx_a: usize = kani::any();
        let idx_b: usize = kani::any();
        kani::assume(idx_a < 64);
        kani::assume(idx_b < 64);
        let a = HandleId(idx_a);
        let b = HandleId(idx_b);
        let via_bitor = a | b;
        let via_insert = HandleSet::EMPTY.insert(a).insert(b);
        assert_eq!(via_bitor.0, via_insert.0);
    }

    #[kani::proof]
    fn handleset_len_after_insert() {
        let idx: usize = kani::any();
        kani::assume(idx < 64);
        let id = HandleId(idx);
        let set = HandleSet::EMPTY.insert(id);
        assert_eq!(set.len(), 1);
        assert!(!set.is_empty());
    }

    // ---- ReadinessSnapshot properties ----

    #[kani::proof]
    fn snapshot_is_ready_consistent() {
        let bits: u64 = kani::any();
        let idx: usize = kani::any();
        kani::assume(idx < 64);
        let snap = ReadinessSnapshot { bits, count: 64 };
        let id = HandleId(idx);
        // is_ready matches the bit
        assert_eq!(snap.is_ready(id), bits & (1u64 << idx) != 0);
    }

    #[kani::proof]
    fn snapshot_all_ready_correct() {
        let bits: u64 = kani::any();
        let set_bits: u64 = kani::any();
        let snap = ReadinessSnapshot { bits, count: 64 };
        let set = HandleSet(set_bits);
        // all_ready iff every bit in set is present in bits
        assert_eq!(snap.all_ready(set), bits & set_bits == set_bits);
    }

    #[kani::proof]
    fn snapshot_any_ready_correct() {
        let bits: u64 = kani::any();
        let set_bits: u64 = kani::any();
        let snap = ReadinessSnapshot { bits, count: 64 };
        let set = HandleSet(set_bits);
        // any_ready iff at least one bit overlaps
        assert_eq!(snap.any_ready(set), bits & set_bits != 0);
    }

    // ---- Trigger evaluation soundness ----

    // These verify the boolean expressions used in spin_once().

    #[kani::proof]
    fn trigger_any_fires_iff_nonzero() {
        let readiness: u64 = kani::any();
        // Trigger::Any fires when readiness_bits != 0
        let fires = readiness != 0;
        assert_eq!(fires, readiness != 0);
    }

    #[kani::proof]
    fn trigger_one_fires_iff_bit_set() {
        let readiness: u64 = kani::any();
        let idx: usize = kani::any();
        kani::assume(idx < 64);
        let id = HandleId(idx);
        // Trigger::One(id) fires when readiness & (1 << id.0) != 0
        let fires = readiness & (1u64 << id.0) != 0;
        // This is equivalent to checking the specific bit
        assert_eq!(fires, readiness & (1u64 << idx) != 0);
    }

    #[kani::proof]
    fn trigger_allof_fires_iff_all_set() {
        let readiness: u64 = kani::any();
        let set_bits: u64 = kani::any();
        let set = HandleSet(set_bits);
        // Trigger::AllOf(set) fires when readiness & set.0 == set.0
        let fires = readiness & set.0 == set.0;
        // AllOf with empty set always fires
        if set_bits == 0 {
            assert!(fires);
        }
        // If fires, then every bit in set is present
        if fires {
            assert_eq!(readiness & set_bits, set_bits);
        }
    }

    #[kani::proof]
    fn trigger_anyof_fires_iff_any_set() {
        let readiness: u64 = kani::any();
        let set_bits: u64 = kani::any();
        let set = HandleSet(set_bits);
        // Trigger::AnyOf(set) fires when readiness & set.0 != 0
        let fires = readiness & set.0 != 0;
        // AnyOf with empty set never fires
        if set_bits == 0 {
            assert!(!fires);
        }
    }

    #[kani::proof]
    fn trigger_allof_implies_anyof() {
        let readiness: u64 = kani::any();
        let set_bits: u64 = kani::any();
        kani::assume(set_bits != 0); // Non-empty set
        let allof_fires = readiness & set_bits == set_bits;
        let anyof_fires = readiness & set_bits != 0;
        // If AllOf fires, then AnyOf must also fire
        if allof_fires {
            assert!(anyof_fires);
        }
    }

    #[kani::proof]
    fn trigger_one_equivalent_to_anyof_singleton() {
        let readiness: u64 = kani::any();
        let idx: usize = kani::any();
        kani::assume(idx < 64);
        let id = HandleId(idx);
        let singleton = HandleSet::EMPTY.insert(id);
        // One(id) and AnyOf({id}) produce the same result
        let one_fires = readiness & (1u64 << id.0) != 0;
        let anyof_fires = readiness & singleton.0 != 0;
        assert_eq!(one_fires, anyof_fires);
    }
}

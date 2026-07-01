//! Executor API for nros C API.
//!
//! Thin wrapper over `nros_node::Executor`. All dispatch logic, trigger
//! evaluation, LET semantics, and I/O driving are delegated to the Rust
//! executor — this module only handles C FFI translation.

use core::{
    ffi::{c_char, c_int},
    ptr,
};

use crate::{
    action::{
        ActionServerInternal, cancel_callback_trampoline, goal_callback_trampoline,
        nros_action_client_state_t, nros_action_client_t, nros_action_server_state_t,
        nros_action_server_t, nros_goal_status_t, nros_goal_uuid_t,
    },
    error::*,
    guard_condition::{nros_guard_condition_state_t, nros_guard_condition_t},
    node::nros_node_t,
    service::{
        client_response_trampoline, nros_client_state_t, nros_client_t, nros_service_state_t,
        nros_service_t,
    },
    subscription::{nros_subscription_state_t, nros_subscription_t},
    support::{nros_support_state_t, nros_support_t},
    timer::{nros_timer_state_t, nros_timer_t},
};

pub use crate::config::*;
use crate::constants::NROS_MAX_CONCURRENT_GOALS;

// ============================================================================
// Internal executor type
// ============================================================================

/// The concrete nros-node executor type used by the C API.
///
/// Sizes are configured via `NROS_EXECUTOR_MAX_CBS` and `NROS_EXECUTOR_ARENA_SIZE`
/// environment variables at build time (matching nros-node's build.rs).
// phase-271 — the executor borrows its per-entry storage (`Executor<'static>`);
// the C API keeps it heap-free by carving that backing from the SAME pinned
// `_opaque` buffer, laid out as [`nros_node::ExecutorInlineStorage`] (executor
// header at offset 0, backing tail). The executor still lives at offset 0, so
// [`get_executor`] / drop are unchanged.
pub(crate) type CExecutor = nros_node::Executor<'static>;

/// `u64` words of per-entry backing the inline executor carves from the tail of
/// its `_opaque` buffer (default sizing — same as the Rust `alloc` convenience).
#[cfg(feature = "rmw-cffi")]
pub(crate) const EXECUTOR_BACKING_U64S: usize = nros_node::ExecutorSizing::DEFAULT.u64_len();

// Compile-time assertion: inline opaque storage must fit the executor header
// PLUS its carved backing (the `ExecutorInlineStorage` layout).
#[cfg(feature = "rmw-cffi")]
const _: () = assert!(
    core::mem::size_of::<nros_node::ExecutorInlineStorage>()
        <= EXECUTOR_OPAQUE_U64S * core::mem::size_of::<u64>(),
    "EXECUTOR_OPAQUE_U64S too small for Executor + backing — increase \
     NROS_EXECUTOR_ARENA_SIZE or NROS_EXECUTOR_MAX_CBS, or adjust the overhead in build.rs"
);

/// Get a mutable reference to the internal executor from opaque storage.
///
/// # Safety
/// The opaque storage must contain a live, initialized `CExecutor`.
#[inline]
pub(crate) unsafe fn get_executor(opaque: &mut [u64; EXECUTOR_OPAQUE_U64S]) -> &mut CExecutor {
    &mut *(opaque.as_mut_ptr() as *mut CExecutor)
}

/// Get a mutable reference to the internal executor from a raw pointer.
///
/// Used by the action server module which stores a raw pointer to the
/// executor's opaque storage.
///
/// # Safety
/// `ptr` must point to the `_opaque` field of a live, initialized
/// `nros_executor_t`.
#[inline]
pub(crate) unsafe fn get_executor_from_ptr(ptr: *mut core::ffi::c_void) -> &'static mut CExecutor {
    &mut *(ptr as *mut CExecutor)
}

/// Propagate node identity from a C node into the executor before
/// registering an entity. The `add_*_raw_*` methods read
/// `self.node_name` / `self.namespace` to build the liveliness keyexpr;
/// without identity, no liveliness token is declared and rmw_zenoh
/// subscribers won't discover the entity.
///
/// # Safety
/// `node` must be NULL or point to an initialized `nros_node_t` with
/// valid `name_len` / `namespace_len`.
unsafe fn set_executor_node_identity(rust_exec: &mut CExecutor, node: *const nros_node_t) {
    if node.is_null() {
        return;
    }
    let node_ref = &*node;
    let name_str = core::str::from_utf8_unchecked(&node_ref.name[..node_ref.name_len]);
    let ns_str = core::str::from_utf8_unchecked(&node_ref.namespace[..node_ref.namespace_len]);
    rust_exec.set_node_identity(name_str, ns_str);
}

// ============================================================================
// C types (kept for API compatibility)
// ============================================================================

/// Trigger function type for executor.
///
/// A trigger function receives a boolean array indicating which handles have
/// data ready, along with the count of handles. It returns true if the executor
/// should process callbacks.
///
/// # Parameters
/// * `ready` - Pointer to boolean array (one per handle)
/// * `count` - Number of elements in the array
/// * `context` - User-provided context pointer
///
/// # Returns
/// * `true` if executor should process callbacks
/// * `false` if executor should skip processing
pub type nros_executor_trigger_t = Option<
    unsafe extern "C" fn(ready: *const bool, count: usize, context: *mut core::ffi::c_void) -> bool,
>;

/// Callback invocation mode
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_executor_invocation_t {
    /// Only invoke callback when new data is available
    NROS_EXECUTOR_ON_NEW_DATA = 0,
    /// Always invoke callback (even with NULL data)
    NROS_EXECUTOR_ALWAYS = 1,
}

/// Executor data communication semantics
///
/// Defines when data is taken from DDS during spin operations.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_executor_semantics_t {
    /// RCLCPP executor semantics: Data is taken from DDS just before
    /// the corresponding callback is called.
    NROS_SEMANTICS_RCLCPP_EXECUTOR = 0,
    /// Logical Execution Time (LET) semantics: At one sampling point,
    /// new data of all ready subscriptions are taken from DDS.
    /// During sequential processing, the data from that sampling point
    /// is used. New data arriving after the sampling point is not
    /// considered until the next spin iteration.
    NROS_SEMANTICS_LOGICAL_EXECUTION_TIME = 1,
}

/// Executor state
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_executor_state_t {
    /// Not initialized
    NROS_EXECUTOR_STATE_UNINITIALIZED = 0,
    /// Initialized and ready
    NROS_EXECUTOR_STATE_INITIALIZED = 1,
    /// Currently spinning
    NROS_EXECUTOR_STATE_SPINNING = 2,
    /// Shutdown
    NROS_EXECUTOR_STATE_SHUTDOWN = 3,
}

/// Executor structure.
///
/// The executor delegates all dispatch logic to an internal
/// executor. The C struct retains state, timeout, and
/// per-type counters for API compatibility.
///
/// The internal executor is stored inline in `_opaque` — no heap
/// allocation is needed. The storage size is computed at build time
/// from `NROS_EXECUTOR_MAX_CBS` and `NROS_EXECUTOR_ARENA_SIZE`.
#[repr(C)]
pub struct nros_executor_t {
    /// Current state
    pub state: nros_executor_state_t,
    /// Timeout in nanoseconds for spin_some
    pub timeout_ns: u64,
    /// Data communication semantics
    pub semantics: nros_executor_semantics_t,
    /// Pointer to support context
    pub support: *const nros_support_t,
    /// Trigger function (NULL = default "any" trigger)
    pub trigger: nros_executor_trigger_t,
    /// User context for trigger function
    pub trigger_context: *mut core::ffi::c_void,
    /// Number of handles registered
    pub handle_count: usize,
    /// Maximum handles (configured at init)
    pub max_handles: usize,
    /// Number of subscription handles
    pub subscription_count: usize,
    /// Number of timer handles
    pub timer_count: usize,
    /// Number of service handles
    pub service_count: usize,
    /// Next invocation time in nanoseconds for drift-compensated spin_period
    pub invocation_time_ns: u64,
    /// Reentrancy guard: set to `true` while `spin_once` is dispatching
    /// callbacks. Blocking helpers (`nros_client_call`, `nros_action_send_goal`,
    /// etc.) check this flag and return `NROS_RET_REENTRANT` if set.
    pub in_dispatch: bool,
    /// Inline opaque storage for the executor.
    /// Managed by nros_executor_init/fini — no heap allocation needed.
    pub _opaque: [u64; EXECUTOR_OPAQUE_U64S],
}

impl Default for nros_executor_t {
    fn default() -> Self {
        Self {
            state: nros_executor_state_t::NROS_EXECUTOR_STATE_UNINITIALIZED,
            timeout_ns: 100_000_000, // 100ms default
            semantics: nros_executor_semantics_t::NROS_SEMANTICS_RCLCPP_EXECUTOR,
            support: ptr::null(),
            trigger: None,
            trigger_context: ptr::null_mut(),
            handle_count: 0,
            max_handles: NROS_EXECUTOR_MAX_HANDLES,
            subscription_count: 0,
            timer_count: 0,
            service_count: 0,
            invocation_time_ns: 0,
            in_dispatch: false,
            #[allow(clippy::large_stack_arrays)] // Intentional: inline opaque storage avoids heap
            _opaque: [0u64; EXECUTOR_OPAQUE_U64S],
        }
    }
}

/// Get a zero-initialized executor.
#[unsafe(no_mangle)]
pub extern "C" fn nros_executor_get_zero_initialized() -> nros_executor_t {
    nros_executor_t::default()
}

/// Initialize an executor.
///
/// # Parameters
/// * `executor` - Pointer to a zero-initialized executor
/// * `support` - Pointer to an initialized support context
/// * `max_handles` - Maximum number of handles (capped at NROS_EXECUTOR_MAX_HANDLES)
///
/// # Returns
/// * `NROS_RET_OK` on success
/// * `NROS_RET_INVALID_ARGUMENT` if any pointer is NULL or max_handles is 0
/// * `NROS_RET_NOT_INIT` if support is not initialized
///
/// # Safety
/// * All pointers must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_init(
    executor: *mut nros_executor_t,
    support: *const nros_support_t,
    max_handles: usize,
) -> nros_ret_t {
    validate_not_null!(executor, support);

    if max_handles == 0 {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let executor = &mut *executor;
    let support_ref = &*support;

    validate_state!(
        executor,
        nros_executor_state_t::NROS_EXECUTOR_STATE_UNINITIALIZED,
        NROS_RET_BAD_SEQUENCE
    );
    validate_state!(
        support_ref,
        nros_support_state_t::NROS_SUPPORT_STATE_INITIALIZED
    );

    // Create the internal nros-node executor using a borrowed session pointer.
    // Written directly into inline opaque storage — no heap allocation.
    let session_ptr = support_ref.get_session_ptr();
    if session_ptr.is_null() {
        return NROS_RET_NOT_INIT;
    }

    // `mut` only needed under `feature = "std"` where the
    // env-var-driven primary-identity block below mutates it;
    // on `no_std` (FreeRTOS / NuttX / ThreadX) the mutation
    // path compiles out and `-D unused-mut` would otherwise
    // hard-fail every cmake build of the C examples.
    // phase-271 — carve the per-entry backing from the tail of this same
    // (pinned, caller-owned, never-moved) `_opaque` buffer, then write the
    // executor header at offset 0. No heap. SAFETY: `_opaque` is sized for
    // `ExecutorInlineStorage` (asserted above); the buffer outlives the executor
    // (C owns it for the program) so treating the backing as `&'static mut` is
    // sound, and the header/backing sub-regions are disjoint.
    let inline = executor._opaque.as_mut_ptr() as *mut nros_node::ExecutorInlineStorage;
    let backing: &'static mut [core::mem::MaybeUninit<u64>] =
        core::slice::from_raw_parts_mut((*inline).backing.as_mut_ptr(), EXECUTOR_BACKING_U64S);
    #[allow(unused_mut)]
    let mut rust_exec =
        CExecutor::from_session_ptr_in(session_ptr, backing, nros_node::ExecutorSizing::DEFAULT);
    // Phase 156 — populate executor's primary identity fields
    // so `NodeBuilder::resolve_session_slot` can return slot 0
    // when a C-side `nros_executor_node_init(rmw_name, ...)`
    // names the same backend the support session opened
    // against. Mirror env-var resolution `open_session` uses so
    // primary picks line up.
    #[cfg(feature = "std")]
    {
        let name = std::env::var("NROS_RMW").unwrap_or_default();
        let support_locator =
            core::str::from_utf8_unchecked(&support_ref.locator[..support_ref.locator_len]);
        rust_exec.set_primary_identity(&name, support_locator);
    }
    ptr::write(executor._opaque.as_mut_ptr() as *mut CExecutor, rust_exec);

    executor.max_handles = max_handles.min(NROS_EXECUTOR_MAX_HANDLES);
    executor.handle_count = 0;
    executor.support = support;
    executor.timeout_ns = 100_000_000; // 100ms default
    executor.state = nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED;

    NROS_RET_OK
}

/// Set the executor timeout.
///
/// # Safety
/// * `executor` must be a valid pointer to an initialized executor
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_set_timeout(
    executor: *mut nros_executor_t,
    timeout_ns: u64,
) -> nros_ret_t {
    validate_not_null!(executor);

    let executor = &mut *executor;

    if executor.state == nros_executor_state_t::NROS_EXECUTOR_STATE_UNINITIALIZED
        || executor.state == nros_executor_state_t::NROS_EXECUTOR_STATE_SHUTDOWN
    {
        return NROS_RET_NOT_INIT;
    }

    executor.timeout_ns = timeout_ns;
    NROS_RET_OK
}

/// Phase 124.F.3 — session-level connectivity probe.
///
/// Sends a wire-level round-trip ("is the peer / agent / router
/// reachable?") and waits up to `timeout_ms`. Mirrors micro-ROS's
/// `rmw_uros_ping_agent`. Useful for reconnect-on-link-loss
/// patterns: bare-metal code calls `ping(100)` periodically and
/// tears down / re-opens the session on timeout.
///
/// # Returns
/// * `NROS_RET_OK` — peer responded within budget.
/// * `NROS_RET_TIMEOUT` — no reply before `timeout_ms`.
/// * `NROS_RET_UNSUPPORTED` — active backend can't probe.
/// * `NROS_RET_NOT_INIT` — executor not initialised.
/// * `NROS_RET_INVALID_ARGUMENT` — `executor` is NULL.
///
/// # Safety
/// * `executor` must be a valid pointer to an initialized executor.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_ping(
    executor: *mut nros_executor_t,
    timeout_ms: i32,
) -> nros_ret_t {
    validate_not_null!(executor);
    let exec_t = &mut *executor;
    if exec_t.state == nros_executor_state_t::NROS_EXECUTOR_STATE_UNINITIALIZED
        || exec_t.state == nros_executor_state_t::NROS_EXECUTOR_STATE_SHUTDOWN
    {
        return NROS_RET_NOT_INIT;
    }
    #[cfg(feature = "rmw-cffi")]
    {
        let exec = get_executor(&mut exec_t._opaque);
        match exec.ping(timeout_ms) {
            Ok(()) => NROS_RET_OK,
            Err(nros_node::NodeError::Transport(nros_rmw::TransportError::Timeout)) => {
                NROS_RET_TIMEOUT
            }
            Err(nros_node::NodeError::Transport(nros_rmw::TransportError::Unsupported)) => {
                NROS_RET_UNSUPPORTED
            }
            Err(_) => NROS_RET_ERROR,
        }
    }
    #[cfg(not(feature = "rmw-cffi"))]
    {
        let _ = timeout_ms;
        NROS_RET_UNSUPPORTED
    }
}

/// Set data communication semantics.
///
/// # Safety
/// * `executor` must be a valid pointer to an initialized executor
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_set_semantics(
    executor: *mut nros_executor_t,
    semantics: nros_executor_semantics_t,
) -> nros_ret_t {
    validate_not_null!(executor);

    let executor = &mut *executor;

    if executor.state == nros_executor_state_t::NROS_EXECUTOR_STATE_UNINITIALIZED
        || executor.state == nros_executor_state_t::NROS_EXECUTOR_STATE_SHUTDOWN
    {
        return NROS_RET_NOT_INIT;
    }

    executor.semantics = semantics;

    // Forward to the internal executor
    if executor.state == nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED
        || executor.state == nros_executor_state_t::NROS_EXECUTOR_STATE_SPINNING
    {
        let rust_exec = get_executor(&mut executor._opaque);
        rust_exec.set_semantics(match semantics {
            nros_executor_semantics_t::NROS_SEMANTICS_RCLCPP_EXECUTOR => {
                nros_node::ExecutorSemantics::RclcppExecutor
            }
            nros_executor_semantics_t::NROS_SEMANTICS_LOGICAL_EXECUTION_TIME => {
                nros_node::ExecutorSemantics::LogicalExecutionTime
            }
        });
    }

    NROS_RET_OK
}

/// Phase 104.C.8.b — initialize a Node via the executor's
/// [`node_builder`](nros_node::Executor::node_builder) chain.
///
/// Thin wrapper over Rust's
/// `executor.node_builder(name).rmw(...).locator(...).domain_id(...).
/// namespace(...).sched(...).build()`. Materialises a Node inside the
/// executor's node table and stores the returned NodeId in
/// `node.node_id` so subsequent
/// [`nros_executor_register_subscription`] / `_service` / `_client` /
/// `_action_*` calls route through `register_*_on(NodeId, ...)`
/// instead of the legacy single-Node path.
///
/// Replaces the pre-104.C ordering of `support_init → node_init →
/// executor_init` with the rclcpp-aligned `support_init → executor_init →
/// executor_node_init`. The old `nros_node_init` / `nros_node_init_ex`
/// entry points are preserved for source compatibility — they still
/// drive the single-Node legacy path and leave `node.node_id = 0`.
///
/// # Parameters
/// * `executor` — Pointer to an initialised executor.
/// * `node` — Pointer to a zero-initialised node. Populated on success.
/// * `name` — Node name (null-terminated). Must not be NULL.
/// * `options` — Pointer to populated `nros_node_options_t`. NULL =
///   default options (no rmw override, inherits executor's locator
///   + domain, executor-default SchedContext).
///
/// # Returns
/// * `NROS_RET_OK` on success.
/// * `NROS_RET_INVALID_ARGUMENT` on NULL pointers / bad strings.
/// * `NROS_RET_BAD_SEQUENCE` if node is already initialised.
/// * `NROS_RET_NOT_INIT` if executor isn't initialised.
/// * `NROS_RET_ERROR` if the executor's node table is full
///   (`NROS_EXECUTOR_MAX_NODES`) or the backend session open failed.
///
/// # Safety
/// All pointer arguments must satisfy their per-parameter rules. `options`
/// length fields must not overrun their buffers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_node_init(
    executor: *mut nros_executor_t,
    node: *mut nros_node_t,
    name: *const c_char,
    options: *const crate::node::nros_node_options_t,
) -> nros_ret_t {
    use crate::{
        constants::{MAX_LOCATOR_LEN, MAX_NAMESPACE_LEN, MAX_RMW_NAME_LEN},
        node::{NROS_DOMAIN_ID_INHERIT, nros_node_options_t, nros_node_state_t},
    };

    validate_not_null!(executor, node, name);

    let executor = &mut *executor;
    let node_ref = &mut *node;

    validate_state!(
        executor,
        nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED
    );
    if node_ref.state != nros_node_state_t::NROS_NODE_STATE_UNINITIALIZED {
        return NROS_RET_BAD_SEQUENCE;
    }

    // Length-bound + copy node name into struct.
    node_ref.name_len = crate::util::copy_cstr_into(name, &mut node_ref.name);
    if node_ref.name_len == 0 {
        return NROS_RET_INVALID_ARGUMENT;
    }

    // Stack-copy a defaulted options struct when caller passed NULL,
    // so the rest of the function reads a uniform shape.
    let default_opts = nros_node_options_t::default();
    let opts = if options.is_null() {
        &default_opts
    } else {
        let opts_ref = &*options;
        if opts_ref.namespace_len > MAX_NAMESPACE_LEN
            || opts_ref.rmw_name_len > MAX_RMW_NAME_LEN
            || opts_ref.locator_len > MAX_LOCATOR_LEN
        {
            return NROS_RET_INVALID_ARGUMENT;
        }
        opts_ref
    };

    // Mirror options into node struct so subsequent helpers can read
    // namespace / rmw / domain_id without consulting `options` again.
    node_ref.namespace[..opts.namespace_len].copy_from_slice(&opts.namespace[..opts.namespace_len]);
    node_ref.namespace_len = opts.namespace_len;
    node_ref.rmw_name[..opts.rmw_name_len].copy_from_slice(&opts.rmw_name[..opts.rmw_name_len]);
    node_ref.rmw_name_len = opts.rmw_name_len;
    node_ref.domain_id_override = opts.domain_id_override;
    node_ref.sched_context_id = opts.sched_context_id;

    // Drive the Rust executor's NodeBuilder.
    let rust_exec = get_executor(&mut executor._opaque);
    let name_str = core::str::from_utf8_unchecked(&node_ref.name[..node_ref.name_len]);
    let mut builder = rust_exec.node_builder(name_str);
    if opts.rmw_name_len > 0 {
        builder = builder.rmw(core::str::from_utf8_unchecked(
            &opts.rmw_name[..opts.rmw_name_len],
        ));
    }
    if opts.locator_len > 0 {
        builder = builder.locator(core::str::from_utf8_unchecked(
            &opts.locator[..opts.locator_len],
        ));
    }
    if opts.domain_id_override != NROS_DOMAIN_ID_INHERIT {
        builder = builder.domain_id(opts.domain_id_override);
    }
    if opts.namespace_len > 0 {
        builder = builder.namespace(core::str::from_utf8_unchecked(
            &opts.namespace[..opts.namespace_len],
        ));
    }
    if opts.sched_context_id != 0 {
        builder = builder.sched(nros_node::executor::sched_context::SchedContextId(
            opts.sched_context_id,
        ));
    }
    let node_id = match builder.build() {
        Ok(id) => id,
        Err(_) => return NROS_RET_ERROR,
    };

    // Persist NodeId so handle-creation paths can hit the `_on()`
    // multi-Session variants. Support pointer stays NULL on this path —
    // legacy single-Node paths key off support, multi-Node paths key
    // off node_id + executor pointer (Phase 156 Sub-bug D).
    node_ref.node_id = node_id.raw();
    node_ref.support = core::ptr::null();
    node_ref.executor = executor as *const nros_executor_t;
    node_ref.state = nros_node_state_t::NROS_NODE_STATE_INITIALIZED;

    NROS_RET_OK
}

/// Set the trigger condition for the executor.
///
/// # Safety
/// * `executor` must be a valid pointer to an initialized executor
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_set_trigger(
    executor: *mut nros_executor_t,
    trigger: nros_executor_trigger_t,
    context: *mut core::ffi::c_void,
) -> nros_ret_t {
    validate_not_null!(executor);

    let executor = &mut *executor;

    if executor.state == nros_executor_state_t::NROS_EXECUTOR_STATE_UNINITIALIZED
        || executor.state == nros_executor_state_t::NROS_EXECUTOR_STATE_SHUTDOWN
    {
        return NROS_RET_NOT_INIT;
    }

    executor.trigger = trigger;
    executor.trigger_context = context;

    // Forward to the internal executor
    let rust_exec = get_executor(&mut executor._opaque);
    match trigger {
        Some(cb) => {
            rust_exec.set_trigger(nros_node::Trigger::RawPredicate {
                callback: cb,
                context,
            });
        }
        None => {
            rust_exec.set_trigger(nros_node::Trigger::Any);
        }
    }

    NROS_RET_OK
}

// ============================================================================
// Built-in trigger functions (kept as C-exported convenience wrappers)
// ============================================================================

/// Built-in trigger: fire when ANY handle has data ready.
///
/// # Safety
/// * `ready` must point to a valid array of at least `count` booleans
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_trigger_any(
    ready: *const bool,
    count: usize,
    context: *mut core::ffi::c_void,
) -> bool {
    let _ = context;
    for i in 0..count {
        if *ready.add(i) {
            return true;
        }
    }
    false
}

/// Built-in trigger: fire when ALL handles have data ready.
///
/// # Safety
/// * `ready` must point to a valid array of at least `count` booleans
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_trigger_all(
    ready: *const bool,
    count: usize,
    context: *mut core::ffi::c_void,
) -> bool {
    let _ = context;
    if count == 0 {
        return false;
    }
    for i in 0..count {
        if !*ready.add(i) {
            return false;
        }
    }
    true
}

/// Built-in trigger: always fire (unconditionally).
///
/// # Safety
/// * `ready` and `count` are unused
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_trigger_always(
    ready: *const bool,
    count: usize,
    context: *mut core::ffi::c_void,
) -> bool {
    let _ = (ready, count, context);
    true
}

/// Built-in trigger: fire when the handle at the index stored in context has data.
///
/// `context` must point to a caller-owned `size_t` holding the handle
/// index. Passing `(void*)(size_t)idx` directly is NOT supported — that
/// pattern is UB on strict-alignment targets and CHERI, and the function
/// will dereference the pointer.
///
/// Recommended usage:
/// ```c
/// static size_t my_trigger_index = 2;
/// nros_executor_set_trigger(&exec, nros_executor_trigger_one, &my_trigger_index);
/// ```
///
/// # Safety
/// * `ready` must point to a valid array of at least `count` booleans.
/// * `context` must point to a valid `size_t` alive for the trigger's lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_trigger_one(
    ready: *const bool,
    count: usize,
    context: *mut core::ffi::c_void,
) -> bool {
    if context.is_null() {
        return false;
    }
    let index = *(context as *const usize);
    if index < count {
        *ready.add(index)
    } else {
        false
    }
}

// ============================================================================
// Handle registration — delegated to nros-node Executor
// ============================================================================

/// Add a subscription to the executor.
///
/// Extracts metadata from the subscription struct and registers a raw-bytes
/// callback with the internal nros-node executor. The RMW subscriber handle
/// is created here (moved from subscription init).
///
/// # Safety
/// * All pointers must be valid and point to initialized objects
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_register_subscription(
    executor: *mut nros_executor_t,
    subscription: *mut nros_subscription_t,
    invocation: nros_executor_invocation_t,
) -> nros_ret_t {
    validate_not_null!(executor, subscription);

    let executor = &mut *executor;
    let subscription_ref = &*subscription;

    validate_state!(
        executor,
        nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED
    );
    validate_state!(
        subscription_ref,
        nros_subscription_state_t::NROS_SUBSCRIPTION_STATE_INITIALIZED
    );

    // Check capacity
    if executor.handle_count >= executor.max_handles {
        return NROS_RET_FULL;
    }

    {
        let rust_exec = get_executor(&mut executor._opaque);

        // Extract metadata from subscription struct
        let topic_str = core::str::from_utf8_unchecked(
            &subscription_ref.topic_name[..subscription_ref.topic_name_len],
        );
        let type_str = core::str::from_utf8_unchecked(
            &subscription_ref.type_name[..subscription_ref.type_name_len],
        );
        let type_hash_str = core::str::from_utf8_unchecked(
            &subscription_ref.type_hash[..subscription_ref.type_hash_len],
        );

        // Get QoS settings from the subscription
        let qos = subscription_ref.get_qos_settings();

        // Get callback and context
        let callback = match subscription_ref.get_callback() {
            Some(cb) => cb,
            None => return NROS_RET_INVALID_ARGUMENT,
        };
        let context = subscription_ref.get_context();

        // Propagate node identity into the executor so the underlying
        // create_subscriber call gets liveliness keyexpr metadata.
        set_executor_node_identity(rust_exec, subscription_ref.node);

        // Phase 104.C.8.b — when the Node was created via
        // `nros_executor_node_init`, route through `_on(NodeId, ...)`
        // so multi-RMW bridges land on the right session. Legacy
        // `nros_node_init`-style Nodes carry `node_id == 0` and fall
        // through to the single-Node entry point.
        let node_raw_id = if subscription_ref.node.is_null() {
            0
        } else {
            (*subscription_ref.node).node_id
        };
        // Phase 189.M2.b — the single kept C-FFI subscription core.
        let node_id =
            (node_raw_id != 0).then(|| nros_node::executor::NodeId::from_raw(node_raw_id));
        let result = rust_exec.add_arena_subscription_c_callback::<MESSAGE_BUFFER_SIZE>(
            node_id,
            topic_str,
            type_str,
            type_hash_str,
            qos,
            callback,
            context,
            None, // Phase 273 W3: group threading is via nros_executor_register_subscription_in_group
        );

        match result {
            Ok(handle_id) => {
                // Store the handle ID in the subscription for later reference
                let sub_mut = &mut *subscription;
                sub_mut.set_handle_id(handle_id);

                // Set invocation mode
                if invocation == nros_executor_invocation_t::NROS_EXECUTOR_ALWAYS {
                    rust_exec.set_invocation(handle_id, nros_node::InvocationMode::Always);
                }

                // Phase 189.M3 — apply a scheduling-context binding
                // requested via `nros_subscription_init_with_options`.
                // `0` = inherit the default (no-op). A non-zero slot
                // must be a valid id from
                // `nros_executor_create_sched_context`; an unknown id
                // fails the registration so the caller learns the
                // binding was rejected rather than silently dropped.
                let requested_sc = sub_mut.sched_context_id;
                if requested_sc != 0 {
                    let sc_id = nros_node::executor::sched_context::SchedContextId(requested_sc);
                    if rust_exec
                        .bind_handle_to_sched_context(handle_id, sc_id)
                        .is_err()
                    {
                        return NROS_RET_INVALID_ARGUMENT;
                    }
                }

                executor.handle_count += 1;
                executor.subscription_count += 1;
                NROS_RET_OK
            }
            Err(_) => NROS_RET_ERROR,
        }
    }
}

/// Phase 189.M3.4 — register a raw subscription whose callback also receives
/// the sample's wire **attachment** (the C analog of the Rust
/// `node.subscription(t).generic(..).message_info()` builder; rclc's
/// generic-with-info subscription). Direct-arg form (no `nros_subscription_t`
/// struct): the callback signature differs from the plain
/// [`nros_subscription_callback_t`], so this is its own entry point rather than
/// a flag on `nros_executor_register_subscription`.
///
/// `node` may be NULL (legacy single-Node path) or a Node created via
/// `nros_executor_node_init` (routes to that Node's session). `qos` may be NULL
/// (defaults). Cross-RMW bridges read the `bridge_origin` tag from the
/// attachment for echo suppression.
///
/// # Safety
/// All non-NULL pointers must be valid; the C strings must be NUL-terminated
/// UTF-8 valid for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_register_subscription_raw_with_info(
    executor: *mut nros_executor_t,
    node: *const nros_node_t,
    topic_name: *const c_char,
    type_name: *const c_char,
    type_hash: *const c_char,
    qos: *const crate::qos::nros_qos_t,
    callback: crate::subscription::nros_subscription_info_callback_t,
    context: *mut core::ffi::c_void,
) -> nros_ret_t {
    validate_not_null!(executor, topic_name, type_name, type_hash);
    let Some(cb) = callback else {
        return NROS_RET_INVALID_ARGUMENT;
    };

    let executor = &mut *executor;
    validate_state!(
        executor,
        nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED
    );
    if executor.handle_count >= executor.max_handles {
        return NROS_RET_FULL;
    }

    let (Ok(topic_str), Ok(type_str), Ok(type_hash_str)) = (
        core::ffi::CStr::from_ptr(topic_name).to_str(),
        core::ffi::CStr::from_ptr(type_name).to_str(),
        core::ffi::CStr::from_ptr(type_hash).to_str(),
    ) else {
        return NROS_RET_INVALID_ARGUMENT;
    };

    let qos_settings = if qos.is_null() {
        nros_node::QosSettings::default()
    } else {
        (*qos).to_qos_settings()
    };

    {
        let rust_exec = get_executor(&mut executor._opaque);
        set_executor_node_identity(rust_exec, node);
        let node_raw_id = if node.is_null() { 0 } else { (*node).node_id };
        let node_id =
            (node_raw_id != 0).then(|| nros_node::executor::NodeId::from_raw(node_raw_id));
        let result = rust_exec.add_arena_subscription_c_info_callback::<MESSAGE_BUFFER_SIZE>(
            node_id,
            topic_str,
            type_str,
            type_hash_str,
            qos_settings,
            cb,
            context,
        );
        match result {
            Ok(_handle_id) => {
                executor.handle_count += 1;
                executor.subscription_count += 1;
                NROS_RET_OK
            }
            Err(_) => NROS_RET_ERROR,
        }
    }
}

/// Add a timer to the executor.
///
/// Wraps the C timer callback in a closure and registers it with the
/// internal nros-node executor.
///
/// # Safety
/// * All pointers must be valid and point to initialized objects
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_register_timer(
    executor: *mut nros_executor_t,
    timer: *mut nros_timer_t,
) -> nros_ret_t {
    validate_not_null!(executor, timer);

    let executor = &mut *executor;
    let timer_ref = &*timer;

    validate_state!(
        executor,
        nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED
    );
    validate_state!(timer_ref, nros_timer_state_t::NROS_TIMER_STATE_RUNNING);

    // Check capacity
    if executor.handle_count >= executor.max_handles {
        return NROS_RET_FULL;
    }

    {
        let rust_exec = get_executor(&mut executor._opaque);

        // Get the C callback and context from the timer
        let c_callback = match timer_ref.get_callback() {
            Some(cb) => cb,
            None => return NROS_RET_INVALID_ARGUMENT,
        };
        let c_context = timer_ref.get_context();
        let timer_ptr = timer;

        // Wrap the C callback in a Rust closure
        let wrapper = move || {
            // SAFETY: The C callback and timer/context pointers remain valid
            // for the lifetime of the executor (same guarantee as rclc).
            c_callback(timer_ptr, c_context);
        };

        // Convert period from nanoseconds to milliseconds
        let period_ms = timer_ref.period_ns / 1_000_000;
        if period_ms == 0 {
            return NROS_RET_INVALID_ARGUMENT;
        }

        // Register with the nros-node executor
        let period = nros_node::TimerDuration::from_millis(period_ms);
        match rust_exec.register_timer(period, wrapper) {
            Ok(handle_id) => {
                // Store handle ID and executor pointer for cancel/reset operations
                let timer_mut = &mut *timer;
                timer_mut.set_handle_id(handle_id);
                timer_mut.set_executor_ptr(executor._opaque.as_mut_ptr() as *mut core::ffi::c_void);

                executor.handle_count += 1;
                executor.timer_count += 1;
                NROS_RET_OK
            }
            Err(_) => NROS_RET_ERROR,
        }
    }
}

/// Phase 273 (RFC-0047) — register a subscription in a named callback group.
///
/// Identical to `nros_executor_register_subscription` but additionally passes
/// the group name to the executor so the seeded `group_sched_table` can bind
/// the callback to the group's `SchedContext`. `callback_group` may be NULL or
/// an empty string — both behave identically to `nros_executor_register_subscription`.
///
/// # Safety
/// All non-NULL pointers must be valid and point to initialized objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_register_subscription_in_group(
    executor: *mut nros_executor_t,
    subscription: *mut nros_subscription_t,
    invocation: nros_executor_invocation_t,
    callback_group: *const c_char,
) -> nros_ret_t {
    validate_not_null!(executor, subscription);

    let executor = &mut *executor;
    let subscription_ref = &*subscription;

    validate_state!(
        executor,
        nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED
    );
    validate_state!(
        subscription_ref,
        nros_subscription_state_t::NROS_SUBSCRIPTION_STATE_INITIALIZED
    );

    if executor.handle_count >= executor.max_handles {
        return NROS_RET_FULL;
    }

    {
        let rust_exec = get_executor(&mut executor._opaque);

        let topic_str = core::str::from_utf8_unchecked(
            &subscription_ref.topic_name[..subscription_ref.topic_name_len],
        );
        let type_str = core::str::from_utf8_unchecked(
            &subscription_ref.type_name[..subscription_ref.type_name_len],
        );
        let type_hash_str = core::str::from_utf8_unchecked(
            &subscription_ref.type_hash[..subscription_ref.type_hash_len],
        );
        let qos = subscription_ref.get_qos_settings();
        let callback = match subscription_ref.get_callback() {
            Some(cb) => cb,
            None => return NROS_RET_INVALID_ARGUMENT,
        };
        let context = subscription_ref.get_context();

        set_executor_node_identity(rust_exec, subscription_ref.node);

        let node_raw_id = if subscription_ref.node.is_null() {
            0
        } else {
            (*subscription_ref.node).node_id
        };
        let node_id =
            (node_raw_id != 0).then(|| nros_node::executor::NodeId::from_raw(node_raw_id));

        // Extract the group name from the C string (NULL or empty ⇒ None).
        let group_str = if callback_group.is_null() {
            None
        } else {
            let s = core::ffi::CStr::from_ptr(callback_group)
                .to_str()
                .unwrap_or("");
            if s.is_empty() { None } else { Some(s) }
        };

        let result = rust_exec.add_arena_subscription_c_callback::<MESSAGE_BUFFER_SIZE>(
            node_id,
            topic_str,
            type_str,
            type_hash_str,
            qos,
            callback,
            context,
            group_str,
        );

        match result {
            Ok(handle_id) => {
                let sub_mut = &mut *subscription;
                sub_mut.set_handle_id(handle_id);

                // Apply invocation override if not the default (on-new-data = 0).
                if invocation == nros_executor_invocation_t::NROS_EXECUTOR_ALWAYS {
                    rust_exec.set_invocation(handle_id, nros_node::InvocationMode::Always);
                }

                executor.handle_count += 1;
                NROS_RET_OK
            }
            Err(_) => NROS_RET_ERROR,
        }
    }
}

/// Phase 273 (RFC-0047) — register a timer in a named callback group.
///
/// Identical to `nros_executor_register_timer` but additionally passes the
/// group name so the seeded `group_sched_table` can bind the callback to the
/// group's `SchedContext`. `callback_group` may be NULL or empty — both behave
/// identically to `nros_executor_register_timer`.
///
/// # Safety
/// All non-NULL pointers must be valid and point to initialized objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_register_timer_in_group(
    executor: *mut nros_executor_t,
    timer: *mut nros_timer_t,
    callback_group: *const c_char,
) -> nros_ret_t {
    validate_not_null!(executor, timer);

    let executor = &mut *executor;
    let timer_ref = &*timer;

    validate_state!(
        executor,
        nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED
    );
    validate_state!(timer_ref, nros_timer_state_t::NROS_TIMER_STATE_RUNNING);

    if executor.handle_count >= executor.max_handles {
        return NROS_RET_FULL;
    }

    {
        let rust_exec = get_executor(&mut executor._opaque);

        let c_callback = match timer_ref.get_callback() {
            Some(cb) => cb,
            None => return NROS_RET_INVALID_ARGUMENT,
        };
        let c_context = timer_ref.get_context();
        let timer_ptr = timer;

        let wrapper = move || {
            c_callback(timer_ptr, c_context);
        };

        let period_ms = timer_ref.period_ns / 1_000_000;
        if period_ms == 0 {
            return NROS_RET_INVALID_ARGUMENT;
        }

        // Extract the group name (NULL or empty ⇒ None).
        let group_str = if callback_group.is_null() {
            None
        } else {
            let s = core::ffi::CStr::from_ptr(callback_group)
                .to_str()
                .unwrap_or("");
            if s.is_empty() { None } else { Some(s) }
        };

        // Node identity: the C timer struct doesn't carry a node_id today.
        // Group lookup falls back to the executor's primary node when no
        // node_id is threaded — sufficient for the single-node C use case.
        let period = nros_node::TimerDuration::from_millis(period_ms);
        match rust_exec.register_timer_on(None, period, wrapper, group_str) {
            Ok(handle_id) => {
                let timer_mut = &mut *timer;
                timer_mut.set_handle_id(handle_id);
                timer_mut.set_executor_ptr(executor._opaque.as_mut_ptr() as *mut core::ffi::c_void);

                executor.handle_count += 1;
                executor.timer_count += 1;
                NROS_RET_OK
            }
            Err(_) => NROS_RET_ERROR,
        }
    }
}

/// Add a service to the executor.
///
/// Extracts metadata from the service struct and registers a raw-bytes
/// service callback with the internal nros-node executor.
///
/// # Safety
/// * All pointers must be valid and point to initialized objects
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_register_service(
    executor: *mut nros_executor_t,
    service: *mut nros_service_t,
) -> nros_ret_t {
    validate_not_null!(executor, service);

    let executor = &mut *executor;
    let service_ref = &*service;

    validate_state!(
        executor,
        nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED
    );
    validate_state!(
        service_ref,
        nros_service_state_t::NROS_SERVICE_STATE_INITIALIZED
    );

    // Check capacity
    if executor.handle_count >= executor.max_handles {
        return NROS_RET_FULL;
    }

    {
        let rust_exec = get_executor(&mut executor._opaque);

        // Extract metadata from service struct
        let service_name = core::str::from_utf8_unchecked(
            &service_ref.service_name[..service_ref.service_name_len],
        );
        let type_str =
            core::str::from_utf8_unchecked(&service_ref.type_name[..service_ref.type_name_len]);
        let type_hash_str =
            core::str::from_utf8_unchecked(&service_ref.type_hash[..service_ref.type_hash_len]);

        // Get callback and context
        let callback = match service_ref.get_callback() {
            Some(cb) => cb,
            None => return NROS_RET_INVALID_ARGUMENT,
        };
        let context = service_ref.get_context();

        // Propagate node identity for liveliness key expression.
        set_executor_node_identity(rust_exec, service_ref.node);

        // Phase 104.C.8.b — route multi-Node services through the
        // `_on(NodeId, ...)` variant when the Node was created via
        // `nros_executor_node_init`.
        let node_raw_id = if service_ref.node.is_null() {
            0
        } else {
            (*service_ref.node).node_id
        };
        // Phase 193.4 — the service's QoS (set via nros_service_init_with_qos;
        // defaults to services_default via nros_service_init).
        let svc_qos = service_ref.get_qos_settings();
        // Phase 189.M3.3.a — capture the requested sched-context slot before the
        // `&mut *service` reborrow in the Ok arm (avoids an aliasing borrow).
        let requested_sc = service_ref.sched_context_id;
        let result = if node_raw_id != 0 {
            rust_exec.register_service_raw_sized_on::<MESSAGE_BUFFER_SIZE, MESSAGE_BUFFER_SIZE>(
                nros_node::executor::NodeId::from_raw(node_raw_id),
                service_name,
                type_str,
                type_hash_str,
                svc_qos,
                callback,
                context,
            )
        } else {
            rust_exec.register_service_raw_sized::<MESSAGE_BUFFER_SIZE, MESSAGE_BUFFER_SIZE>(
                service_name,
                type_str,
                type_hash_str,
                svc_qos,
                callback,
                context,
            )
        };

        match result {
            Ok(handle_id) => {
                // Phase 189.M3.3.a — apply a scheduling-context binding requested
                // via `nros_service_init_with_options`. Done *before* the
                // `executor as *mut _` store below so `rust_exec`'s borrow of
                // `executor._opaque` ends here (no overlap with the whole-executor
                // reborrow). `0` = inherit the default (no-op). An unknown slot
                // fails the registration so the caller learns the binding was
                // rejected rather than silently dropped (mirrors subscriptions).
                if requested_sc != 0 {
                    let sc_id = nros_node::executor::sched_context::SchedContextId(requested_sc);
                    if rust_exec
                        .bind_handle_to_sched_context(handle_id, sc_id)
                        .is_err()
                    {
                        return NROS_RET_INVALID_ARGUMENT;
                    }
                }

                let service_mut = &mut *service;
                service_mut._internal.arena_entry_index = handle_id.0 as i32;
                service_mut._internal.executor_ptr = executor as *mut _ as *mut core::ffi::c_void;

                executor.handle_count += 1;
                executor.service_count += 1;
                NROS_RET_OK
            }
            Err(_) => NROS_RET_ERROR,
        }
    }
}

/// Add a service client to the executor (Phase 82).
///
/// Creates the underlying `RmwServiceClient` inside the executor's arena
/// and stashes the executor pointer + arena entry index into the
/// client's `_internal` blob so subsequent calls to `nros_client_call`,
/// `nros_client_send_request_async`, and friends can drive the executor
/// without taking it as an explicit argument.
///
/// Must be called exactly once after `nros_client_init` and before any
/// send/call. Calling it twice on the same client returns
/// `NROS_RET_BAD_SEQUENCE`.
///
/// # Safety
/// * Both pointers must reference initialized objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_add_client(
    executor: *mut nros_executor_t,
    client: *mut nros_client_t,
) -> nros_ret_t {
    validate_not_null!(executor, client);

    let executor = &mut *executor;
    let client_ref = &*client;

    validate_state!(
        executor,
        nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED
    );
    validate_state!(
        client_ref,
        nros_client_state_t::NROS_CLIENT_STATE_INITIALIZED
    );

    if executor.handle_count >= executor.max_handles {
        return NROS_RET_FULL;
    }

    {
        let opaque_ptr = executor._opaque.as_mut_ptr() as *mut core::ffi::c_void;
        let rust_exec = get_executor_from_ptr(opaque_ptr);

        let service_name =
            core::str::from_utf8_unchecked(&client_ref.service_name[..client_ref.service_name_len]);
        let type_str =
            core::str::from_utf8_unchecked(&client_ref.type_name[..client_ref.type_name_len]);
        let type_hash_str =
            core::str::from_utf8_unchecked(&client_ref.type_hash[..client_ref.type_hash_len]);

        // Trampoline always installed — checks the C struct's
        // response_callback at invocation time, so blocking wrappers
        // (nros_client_call) can install one-shot callbacks AFTER
        // registration without re-registering with the arena.
        let cb: Option<nros_node::RawResponseCallback> = Some(client_response_trampoline);
        let client_ctx = client as *mut core::ffi::c_void;

        // Propagate node identity for liveliness key expression.
        set_executor_node_identity(rust_exec, client_ref.node);

        // Phase 104.C.8.b — service-client multi-Node dispatch.
        let node_raw_id = if client_ref.node.is_null() {
            0
        } else {
            (*client_ref.node).node_id
        };
        // Phase 193.4b — the client's QoS (set via nros_client_init_with_qos;
        // defaults to services_default via nros_client_init).
        let client_qos = client_ref.get_qos_settings();
        // Phase 189.M3.3.a — capture the requested sched-context slot before the
        // `&mut *client` reborrow in the Ok arm (avoids an aliasing borrow).
        let requested_sc = client_ref.sched_context_id;
        let result = if node_raw_id != 0 {
            rust_exec.register_service_client_raw_sized_on::<MESSAGE_BUFFER_SIZE>(
                nros_node::executor::NodeId::from_raw(node_raw_id),
                service_name,
                type_str,
                type_hash_str,
                client_qos,
                cb,
                client_ctx,
            )
        } else {
            rust_exec.register_service_client_raw_sized::<MESSAGE_BUFFER_SIZE>(
                service_name,
                type_str,
                type_hash_str,
                client_qos,
                cb,
                client_ctx,
            )
        };

        match result {
            Ok(handle_id) => {
                // Phase 189.M3.3.a — apply a sched-context binding requested via
                // `nros_client_init_with_options`, *before* the `executor as
                // *mut _` store below so `rust_exec`'s `executor._opaque` borrow
                // ends here (no overlap with the whole-executor reborrow). `0` =
                // inherit (no-op); an unknown slot fails registration.
                if requested_sc != 0 {
                    let sc_id = nros_node::executor::sched_context::SchedContextId(requested_sc);
                    if rust_exec
                        .bind_handle_to_sched_context(handle_id, sc_id)
                        .is_err()
                    {
                        return NROS_RET_INVALID_ARGUMENT;
                    }
                }

                let client_mut = &mut *client;
                client_mut._internal.arena_entry_index = handle_id.0 as i32;
                client_mut._internal.executor_ptr = executor as *mut _ as *mut core::ffi::c_void;
                client_mut.state = nros_client_state_t::NROS_CLIENT_STATE_REGISTERED;

                executor.handle_count += 1;
                NROS_RET_OK
            }
            Err(_) => NROS_RET_ERROR,
        }
    }
}

/// Add a guard condition to the executor.
///
/// # Safety
/// * All pointers must be valid and point to initialized objects
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_register_guard_condition(
    executor: *mut nros_executor_t,
    guard: *mut nros_guard_condition_t,
) -> nros_ret_t {
    validate_not_null!(executor, guard);

    let executor = &mut *executor;
    let guard_ref = &*guard;

    validate_state!(
        executor,
        nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED
    );
    validate_state!(
        guard_ref,
        nros_guard_condition_state_t::NROS_GUARD_CONDITION_STATE_INITIALIZED
    );

    // Check capacity
    if executor.handle_count >= executor.max_handles {
        return NROS_RET_FULL;
    }

    {
        let rust_exec = get_executor(&mut executor._opaque);

        // Get the C callback and context from the guard condition
        let c_callback = guard_ref.get_callback();
        let c_context = guard_ref.get_context();

        // Wrap the C callback in a Rust closure
        let wrapper = move || {
            if let Some(cb) = c_callback {
                // SAFETY: The C callback and context remain valid for the
                // lifetime of the executor.
                cb(c_context);
            }
        };

        match rust_exec.register_guard_condition(wrapper) {
            Ok((handle_id, guard_handle)) => {
                let guard_mut = &mut *guard;
                guard_mut.set_handle_id(handle_id);
                guard_mut.set_guard_handle(guard_handle);

                executor.handle_count += 1;
                NROS_RET_OK
            }
            Err(_) => NROS_RET_ERROR,
        }
    }
}

/// Add an action server to the executor.
///
/// Extracts metadata from the action server struct, creates callback
/// trampolines, and registers with the internal executor.
///
/// # Safety
/// * All pointers must be valid and point to initialized objects
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_register_action_server(
    executor: *mut nros_executor_t,
    server: *mut nros_action_server_t,
) -> nros_ret_t {
    validate_not_null!(executor, server);

    let executor = &mut *executor;
    let server_ref = &*server;

    validate_state!(
        executor,
        nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED
    );
    validate_state!(
        server_ref,
        nros_action_server_state_t::NROS_ACTION_SERVER_STATE_INITIALIZED
    );

    // Check capacity
    if executor.handle_count >= executor.max_handles {
        return NROS_RET_FULL;
    }

    {
        // Grab the opaque pointer before borrowing for get_executor to avoid double borrow.
        let opaque_ptr = executor._opaque.as_mut_ptr() as *mut core::ffi::c_void;
        let rust_exec = get_executor_from_ptr(opaque_ptr);

        // Extract metadata from action server struct
        let action_name =
            core::str::from_utf8_unchecked(&server_ref.action_name[..server_ref.action_name_len]);
        let type_str =
            core::str::from_utf8_unchecked(&server_ref.type_name[..server_ref.type_name_len]);
        let type_hash_str =
            core::str::from_utf8_unchecked(&server_ref.type_hash[..server_ref.type_hash_len]);

        // Get the goal callback (required — validated during init)
        let c_goal_callback = match server_ref.goal_callback {
            Some(cb) => cb,
            None => return NROS_RET_INVALID_ARGUMENT,
        };

        // Create the internal struct (handle filled after registration).
        // Phase 87.5: ActionServerInternal is now a typed `#[repr(C)]` field,
        // not an opaque blob — assign by value.
        let server_mut = &mut *server;
        server_mut._internal = ActionServerInternal {
            handle: nros_node::ActionServerRawHandle::invalid(),
            executor_ptr: opaque_ptr,
            c_goal_callback,
            c_cancel_callback: server_ref.cancel_callback,
            c_accepted_callback: server_ref.accepted_callback,
            c_context: server_ref.context,
            server_ptr: server,
        };
        let context =
            (&mut server_mut._internal) as *mut ActionServerInternal as *mut core::ffi::c_void;

        // Propagate node identity for liveliness key expression.
        set_executor_node_identity(rust_exec, server_ref.node);

        // Phase 104.C.8.b — action-server multi-Node dispatch.
        let node_raw_id = if server_ref.node.is_null() {
            0
        } else {
            (*server_ref.node).node_id
        };

        // Register with the nros-node executor using trampolines. The
        // accepted_callback_trampoline is invoked by the arena *after* the
        // accept reply is sent, so the user's long-running execution does
        // not delay the reply the client is blocking on.
        // Phase 193.4b — the action server's QoS (set via
        // nros_action_server_init_with_qos; defaults to services_default via
        // nros_action_server_init). Applies to the three underlying service
        // servers.
        let server_qos = server_ref.get_qos_settings();
        // Phase 189.M3.3.b — requested sched-context slot (bind applied at the
        // Ok arm using the action server's goal-service handle).
        let requested_sc = server_ref.sched_context_id;
        let node_id = if node_raw_id != 0 {
            Some(nros_node::executor::NodeId::from_raw(node_raw_id))
        } else {
            None
        };
        let result = rust_exec
            .register_action_server_raw_sized::<MESSAGE_BUFFER_SIZE, MESSAGE_BUFFER_SIZE, MESSAGE_BUFFER_SIZE, NROS_MAX_CONCURRENT_GOALS>(
                nros_node::RawActionServerSpec {
                    node_id,
                    action_name,
                    type_name: type_str,
                    type_hash: type_hash_str,
                    qos: server_qos,
                    goal_callback: goal_callback_trampoline,
                    cancel_callback: cancel_callback_trampoline,
                    accepted_callback: Some(crate::action::accepted_callback_trampoline),
                    context,
                },
            );

        match result {
            Ok(handle) => {
                // Phase 189.M3.3.b — bind the action's goal-service handle to the
                // requested sched context (governs the action's callback
                // dispatch). `0` = inherit (no-op); an unknown slot fails
                // registration (mirrors subscriptions/services).
                if requested_sc != 0 {
                    let sc_id = nros_node::executor::sched_context::SchedContextId(requested_sc);
                    if rust_exec
                        .bind_handle_to_sched_context(handle.handle_id(), sc_id)
                        .is_err()
                    {
                        return NROS_RET_INVALID_ARGUMENT;
                    }
                }
                // Fill in the handle now that registration succeeded
                server_mut._internal.handle = handle;
                executor.handle_count += 1;
                NROS_RET_OK
            }
            Err(_) => {
                // Registration failed — reset the internal back to invalid.
                server_mut._internal = ActionServerInternal::invalid_default();
                NROS_RET_ERROR
            }
        }
    }
}

/// Register an action client with the executor for async (non-blocking) operation.
///
/// After registration, `nros_executor_spin_some` polls the action client's
/// pending requests (goal response, feedback, result) and invokes the
/// registered callbacks.
///
/// The action client must already be initialized via `nros_action_client_init`.
/// Callbacks should be set via `nros_action_client_set_goal_response_callback`,
/// `nros_action_client_set_feedback_callback`, and `nros_action_client_set_result_callback`
/// before or after this call.
///
/// # Safety
/// * `executor` and `client` must be valid pointers to initialized structs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_register_action_client(
    executor: *mut nros_executor_t,
    client: *mut nros_action_client_t,
) -> nros_ret_t {
    validate_not_null!(executor, client);

    let executor = &mut *executor;
    let client_ref = &*client;

    validate_state!(
        executor,
        nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED
    );
    validate_state!(
        client_ref,
        nros_action_client_state_t::NROS_ACTION_CLIENT_STATE_INITIALIZED
    );

    if executor.handle_count >= executor.max_handles {
        return NROS_RET_FULL;
    }

    {
        let opaque_ptr = executor._opaque.as_mut_ptr() as *mut core::ffi::c_void;
        let rust_exec = get_executor_from_ptr(opaque_ptr);

        // Always register trampolines — they check the C struct's callback
        // pointer at invocation time, so they handle None gracefully. This is
        // critical: the blocking wrappers (nros_action_send_goal, etc.) install
        // temporary callbacks on the C struct AFTER registration. If we only
        // register trampolines when callbacks are non-None at registration time,
        // the arena will consume replies without invoking the trampoline,
        // causing the blocking wrapper's flag to never be set (→ timeout).
        let goal_response_cb: Option<nros_node::executor::RawGoalResponseCallback> =
            Some(goal_response_trampoline as nros_node::executor::RawGoalResponseCallback);

        let feedback_cb: Option<nros_node::executor::RawFeedbackCallback> =
            Some(feedback_trampoline as nros_node::executor::RawFeedbackCallback);

        let result_cb: Option<nros_node::executor::RawResultCallback> =
            Some(result_trampoline as nros_node::executor::RawResultCallback);

        let client_ctx = client as *mut core::ffi::c_void;

        let action_name =
            core::str::from_utf8_unchecked(&client_ref.action_name[..client_ref.action_name_len]);
        let type_str =
            core::str::from_utf8_unchecked(&client_ref.type_name[..client_ref.type_name_len]);
        let type_hash_str =
            core::str::from_utf8_unchecked(&client_ref.type_hash[..client_ref.type_hash_len]);

        // Propagate node identity for liveliness key expression.
        set_executor_node_identity(rust_exec, client_ref.node);

        // Phase 104.C.8.b — action-client multi-Node dispatch.
        // `spec.node_id` selects the target Node's session (or the
        // executor's own node when `None`).
        let node_raw_id = if client_ref.node.is_null() {
            0
        } else {
            (*client_ref.node).node_id
        };
        let node_id = if node_raw_id != 0 {
            Some(nros_node::executor::NodeId::from_raw(node_raw_id))
        } else {
            None
        };
        // Phase 189.M3.3.b — requested sched-context slot (bind applied at the
        // Ok arm; the client handle's entry_index is its callback slot).
        let requested_sc = client_ref.sched_context_id;

        // Create a NEW ActionClientCore in the arena via register_action_client_raw.
        // The async send functions will use this core (not the client's original).
        // Both share the same global zenoh session, so the arena core's service
        // clients can communicate with the server independently.
        let result = rust_exec.register_action_client_raw(nros_node::RawActionClientSpec {
            node_id,
            action_name,
            type_name: type_str,
            type_hash: type_hash_str,
            goal_response_callback: goal_response_cb,
            feedback_callback: feedback_cb,
            result_callback: result_cb,
            context: client_ctx,
        });

        match result {
            Ok(handle) => {
                let client_mut = &mut *client;
                client_mut._internal.arena_entry_index = handle.entry_index() as i32;
                client_mut._internal.executor_ptr = opaque_ptr;

                // Phase 189.M3.3.b — bind the client's callback slot to the
                // requested sched context (the handle's entry_index is the
                // entries[] slot). `0` = inherit (no-op); unknown slot fails.
                if requested_sc != 0 {
                    let sc_id = nros_node::executor::sched_context::SchedContextId(requested_sc);
                    let handle_id = nros_node::executor::HandleId(handle.entry_index());
                    if rust_exec
                        .bind_handle_to_sched_context(handle_id, sc_id)
                        .is_err()
                    {
                        return NROS_RET_INVALID_ARGUMENT;
                    }
                }

                executor.handle_count += 1;
                NROS_RET_OK
            }
            Err(_) => NROS_RET_ERROR,
        }
    }
}

/// Goal response trampoline — adapts nros-node callback to C API callback.
///
/// # Safety
/// `context` must point to a valid `nros_action_client_t`.
unsafe extern "C" fn goal_response_trampoline(
    goal_id: *const nros_node::GoalId,
    accepted: bool,
    context: *mut core::ffi::c_void,
) {
    let client = &*(context as *const nros_action_client_t);
    if let Some(cb) = client.goal_response_callback {
        let uuid = nros_goal_uuid_t {
            uuid: (*goal_id).uuid,
        };
        cb(&uuid, accepted, client.context);
    }
}

/// Feedback trampoline — adapts nros-node callback to C API callback.
///
/// # Safety
/// `context` must point to a valid `nros_action_client_t`.
unsafe extern "C" fn feedback_trampoline(
    goal_id: *const nros_node::GoalId,
    feedback_data: *const u8,
    feedback_len: usize,
    context: *mut core::ffi::c_void,
) {
    let client = &*(context as *const nros_action_client_t);
    if let Some(cb) = client.feedback_callback {
        let uuid = nros_goal_uuid_t {
            uuid: (*goal_id).uuid,
        };
        cb(&uuid, feedback_data, feedback_len, client.context);
    }
}

/// Result trampoline — adapts nros-node callback to C API callback.
///
/// # Safety
/// `context` must point to a valid `nros_action_client_t`.
unsafe extern "C" fn result_trampoline(
    goal_id: *const nros_node::GoalId,
    status: nros_node::GoalStatus,
    result_data: *const u8,
    result_len: usize,
    context: *mut core::ffi::c_void,
) {
    let client = &*(context as *const nros_action_client_t);
    if let Some(cb) = client.result_callback {
        let uuid = nros_goal_uuid_t {
            uuid: (*goal_id).uuid,
        };
        let c_status = match status {
            nros_node::GoalStatus::Succeeded => nros_goal_status_t::NROS_GOAL_STATUS_SUCCEEDED,
            nros_node::GoalStatus::Canceled => nros_goal_status_t::NROS_GOAL_STATUS_CANCELED,
            nros_node::GoalStatus::Aborted => nros_goal_status_t::NROS_GOAL_STATUS_ABORTED,
            _ => nros_goal_status_t::NROS_GOAL_STATUS_UNKNOWN,
        };
        cb(&uuid, c_status, result_data, result_len, client.context);
    }
}

// ============================================================================
// Spin functions — delegated to nros-node executor
// ============================================================================

/// Spin the executor once.
///
/// Drives middleware I/O, then dispatches ready callbacks.
///
/// # Safety
/// * `executor` must be a valid pointer to an initialized executor
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_spin_some(
    executor: *mut nros_executor_t,
    timeout_ns: u64,
) -> nros_ret_t {
    validate_not_null!(executor);

    let executor = &mut *executor;

    // Accept both INITIALIZED and SPINNING states
    if executor.state != nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED
        && executor.state != nros_executor_state_t::NROS_EXECUTOR_STATE_SPINNING
    {
        return NROS_RET_NOT_INIT;
    }

    {
        let rust_exec = get_executor(&mut executor._opaque);

        // Convert timeout from nanoseconds to milliseconds (i32) for nros-node
        let timeout_ms: u64 = if timeout_ns > 0 {
            (timeout_ns / 1_000_000).max(1)
        } else {
            0
        };

        // spin_once drives I/O internally and handles trigger evaluation,
        // LET semantics, and dispatch. Guard against reentrancy so
        // blocking helpers called from inside a callback are detected.
        executor.in_dispatch = true;
        let result = rust_exec.spin_once(core::time::Duration::from_millis(timeout_ms));
        executor.in_dispatch = false;

        if result.any_work() {
            NROS_RET_OK
        } else {
            NROS_RET_TIMEOUT
        }
    }
}

/// Spin the executor forever.
///
/// # Safety
/// * `executor` must be a valid pointer to an initialized executor
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_spin(executor: *mut nros_executor_t) -> nros_ret_t {
    validate_not_null!(executor);

    let executor_ref = &mut *executor;

    validate_state!(
        executor_ref,
        nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED
    );

    executor_ref.state = nros_executor_state_t::NROS_EXECUTOR_STATE_SPINNING;

    // Spin until shutdown
    while executor_ref.state == nros_executor_state_t::NROS_EXECUTOR_STATE_SPINNING {
        let _ = nros_executor_spin_some(executor, executor_ref.timeout_ns);
    }

    NROS_RET_OK
}

/// Spin the executor with a fixed period.
///
/// # Safety
/// * `executor` must be a valid pointer to an initialized executor
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_spin_period(
    executor: *mut nros_executor_t,
    period_ns: u64,
) -> nros_ret_t {
    validate_not_null!(executor);

    if period_ns == 0 {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let executor_ref = &mut *executor;

    validate_state!(
        executor_ref,
        nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED
    );

    executor_ref.state = nros_executor_state_t::NROS_EXECUTOR_STATE_SPINNING;
    executor_ref.invocation_time_ns = crate::platform::get_time_ns();

    while executor_ref.state == nros_executor_state_t::NROS_EXECUTOR_STATE_SPINNING {
        // `period_ns` is an upper bound on how long `drive_io` will block.
        // The timer delta credited to spin_once is the *real* wall-clock
        // elapsed inside drive_io (measured via std::time::Instant when
        // available), not `period_ns` itself — transports like zenoh-pico's
        // condvar wake early on data arrival, and treating the requested
        // timeout as the delta would tick timers faster than wall-clock.
        let _ = nros_executor_spin_some(executor, period_ns);

        // Accumulate next invocation time to prevent drift
        executor_ref.invocation_time_ns += period_ns;
        let now = crate::platform::get_time_ns();
        if executor_ref.invocation_time_ns > now {
            crate::platform::sleep_ns(executor_ref.invocation_time_ns - now);
        }
    }

    NROS_RET_OK
}

/// Spin the executor for one period.
///
/// # Safety
/// * `executor` must be a valid pointer to an initialized executor
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_spin_one_period(
    executor: *mut nros_executor_t,
    period_ns: u64,
) -> nros_ret_t {
    validate_not_null!(executor);

    if period_ns == 0 {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let executor_ref = &mut *executor;

    if executor_ref.state != nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED
        && executor_ref.state != nros_executor_state_t::NROS_EXECUTOR_STATE_SPINNING
    {
        return NROS_RET_NOT_INIT;
    }

    let start = crate::platform::get_time_ns();

    // `period_ns` bounds how long `drive_io` may block. spin_once
    // measures the actual elapsed wall-clock and credits that — not
    // `period_ns` — to timers. See `nros_executor_spin_period` above.
    let _ = nros_executor_spin_some(executor, period_ns);

    // Sleep for remaining time in period
    let elapsed = crate::platform::get_time_ns().saturating_sub(start);
    if elapsed < period_ns {
        crate::platform::sleep_ns(period_ns - elapsed);
    }

    NROS_RET_OK
}

/// Stop a spinning executor.
///
/// # Safety
/// * `executor` must be a valid pointer
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_stop(executor: *mut nros_executor_t) -> nros_ret_t {
    validate_not_null!(executor);

    let executor = &mut *executor;

    if executor.state == nros_executor_state_t::NROS_EXECUTOR_STATE_SPINNING {
        executor.state = nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED;
    }

    NROS_RET_OK
}

/// Finalize an executor.
///
/// # Safety
/// * `executor` must be a valid pointer
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_fini(executor: *mut nros_executor_t) -> nros_ret_t {
    validate_not_null!(executor);

    let executor = &mut *executor;

    if executor.state == nros_executor_state_t::NROS_EXECUTOR_STATE_UNINITIALIZED {
        return NROS_RET_NOT_INIT;
    }

    // Drop the internal executor in-place — arena entries are cleaned up
    core::ptr::drop_in_place(executor._opaque.as_mut_ptr() as *mut CExecutor);
    #[allow(clippy::large_stack_arrays)] // Intentional: zero-fill inline opaque storage
    {
        executor._opaque = [0u64; EXECUTOR_OPAQUE_U64S];
    }
    executor.handle_count = 0;
    executor.subscription_count = 0;
    executor.timer_count = 0;
    executor.service_count = 0;
    executor.support = ptr::null();
    executor.state = nros_executor_state_t::NROS_EXECUTOR_STATE_SHUTDOWN;

    NROS_RET_OK
}

// ============================================================================
// Query functions
// ============================================================================

/// Get the number of handles in the executor.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_get_handle_count(executor: *const nros_executor_t) -> c_int {
    if executor.is_null() {
        return 0;
    }

    let executor = &*executor;
    executor.handle_count as c_int
}

/// Check if executor is valid (initialized).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_is_valid(executor: *const nros_executor_t) -> bool {
    if executor.is_null() {
        return false;
    }

    let executor = &*executor;
    matches!(
        executor.state,
        nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED
            | nros_executor_state_t::NROS_EXECUTOR_STATE_SPINNING
    )
}

/// Get remaining total handle capacity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_get_remaining_handles(
    executor: *const nros_executor_t,
) -> c_int {
    if executor.is_null() {
        return -1;
    }

    let executor = &*executor;
    (executor.max_handles - executor.handle_count) as c_int
}

// ============================================================================
// Kani verification
// ============================================================================

#[cfg(kani)]
mod verification {
    use super::*;
    use crate::error::*;

    #[kani::proof]
    #[kani::unwind(5)]
    fn executor_init_null_ptrs() {
        // NULL executor → INVALID_ARGUMENT
        let support = crate::support::nros_support_get_zero_initialized();
        assert_eq!(
            unsafe { nros_executor_init(core::ptr::null_mut(), &support, 4) },
            NROS_RET_INVALID_ARGUMENT,
        );

        // NULL support → INVALID_ARGUMENT
        let mut executor = nros_executor_get_zero_initialized();
        assert_eq!(
            unsafe { nros_executor_init(&mut executor, core::ptr::null(), 4) },
            NROS_RET_INVALID_ARGUMENT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn executor_zero_initialized_state() {
        let executor = nros_executor_get_zero_initialized();
        assert_eq!(
            executor.state,
            nros_executor_state_t::NROS_EXECUTOR_STATE_UNINITIALIZED,
        );
        assert!(executor.support.is_null());
        assert_eq!(executor.handle_count, 0);
    }
}

// =============================================================================
// Phase 110.B / 110.C — SchedContext C-API surface
// =============================================================================

/// Scheduling class — picks the runtime queue + selection policy.
/// Mirrors `nros_node::executor::sched_context::SchedClass`.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum nros_sched_class_t {
    NROS_SCHED_CLASS_FIFO = 0,
    NROS_SCHED_CLASS_EDF = 1,
    NROS_SCHED_CLASS_SPORADIC = 2,
    NROS_SCHED_CLASS_BEST_EFFORT = 3,
    NROS_SCHED_CLASS_TIME_TRIGGERED = 4,
}

/// Criticality bucket. Lower numeric value = higher priority.
/// Mirrors `nros_node::executor::sched_context::Priority`.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum nros_sched_priority_t {
    NROS_SCHED_PRIORITY_CRITICAL = 0,
    NROS_SCHED_PRIORITY_NORMAL = 1,
    NROS_SCHED_PRIORITY_BEST_EFFORT = 2,
}

/// Deadline interpretation policy.
/// Mirrors `nros_node::executor::sched_context::DeadlinePolicy`.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum nros_deadline_policy_t {
    NROS_DEADLINE_POLICY_RELEASED = 0,
    NROS_DEADLINE_POLICY_ACTIVATED = 1,
    NROS_DEADLINE_POLICY_INHERITED = 2,
}

/// Scheduling-context descriptor passed to
/// [`nros_executor_create_sched_context`].
///
/// Time fields use a `0` sentinel for "absent" (mirrors the Rust
/// `OptUs` newtype). Cbindgen emits these as plain `uint32_t`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
#[allow(non_camel_case_types)]
pub struct nros_sched_context_t {
    pub class: nros_sched_class_t,
    pub priority: nros_sched_priority_t,
    pub deadline_policy: nros_deadline_policy_t,
    /// Period in microseconds (0 = absent).
    pub period_us: u32,
    /// Budget in microseconds (0 = absent).
    pub budget_us: u32,
    /// Deadline in microseconds (0 = absent).
    pub deadline_us: u32,
    /// Phase 110.F — opt-in OS-level priority for per-callback
    /// dispatch. `0` = no per-callback OS priority (default cooperative
    /// path runs every callback). Numeric meaning is platform-defined.
    pub os_pri: u8,
    /// Phase 110.G — TT-window offset within the executor's major
    /// frame, microseconds. `0` (with `tt_window_duration_us = 0`) =
    /// no TT gate.
    pub tt_window_offset_us: u32,
    /// Phase 110.G — TT-window length in microseconds. `0` disables
    /// the TT gate for this SC.
    pub tt_window_duration_us: u32,
}

/// Identifier of a registered scheduling context. `0` is the
/// auto-created default `Fifo` SC. Mirrors
/// `nros_node::executor::sched_context::SchedContextId`.
#[allow(non_camel_case_types)]
pub type nros_sched_context_id_t = u8;

/// Identifier of the auto-created default `Fifo`-class SC. Every
/// callback registered without an explicit binding maps to it.
/// Phase 110.B.
#[unsafe(no_mangle)]
pub extern "C" fn nros_executor_default_sched_context_id() -> nros_sched_context_id_t {
    0
}

fn convert_sched_context(
    cfg: &nros_sched_context_t,
) -> nros_node::executor::sched_context::SchedContext {
    use nros_node::executor::sched_context::{
        DeadlinePolicy, OptUs, Priority, SchedClass, SchedContext,
    };
    #[allow(deprecated)]
    SchedContext {
        class: match cfg.class {
            nros_sched_class_t::NROS_SCHED_CLASS_FIFO => SchedClass::Fifo,
            nros_sched_class_t::NROS_SCHED_CLASS_EDF => SchedClass::Edf,
            nros_sched_class_t::NROS_SCHED_CLASS_SPORADIC => SchedClass::Sporadic,
            nros_sched_class_t::NROS_SCHED_CLASS_BEST_EFFORT => SchedClass::BestEffort,
            // Phase 110.G refactor: TimeTriggered class is deprecated;
            // accept the C-side enum value but route to Fifo. Callers
            // should switch to populating tt_window_offset_us /
            // tt_window_duration_us for the gate semantics.
            nros_sched_class_t::NROS_SCHED_CLASS_TIME_TRIGGERED => SchedClass::Fifo,
        },
        priority: match cfg.priority {
            nros_sched_priority_t::NROS_SCHED_PRIORITY_CRITICAL => Priority::Critical,
            nros_sched_priority_t::NROS_SCHED_PRIORITY_NORMAL => Priority::Normal,
            nros_sched_priority_t::NROS_SCHED_PRIORITY_BEST_EFFORT => Priority::BestEffort,
        },
        deadline_policy: match cfg.deadline_policy {
            nros_deadline_policy_t::NROS_DEADLINE_POLICY_RELEASED => DeadlinePolicy::Released,
            nros_deadline_policy_t::NROS_DEADLINE_POLICY_ACTIVATED => DeadlinePolicy::Activated,
            nros_deadline_policy_t::NROS_DEADLINE_POLICY_INHERITED => DeadlinePolicy::Inherited,
        },
        period_us: OptUs::from_us(cfg.period_us),
        budget_us: OptUs::from_us(cfg.budget_us),
        deadline_us: OptUs::from_us(cfg.deadline_us),
        os_pri: cfg.os_pri,
        tt_window_offset_us: OptUs::from_us(cfg.tt_window_offset_us),
        tt_window_duration_us: OptUs::from_us(cfg.tt_window_duration_us),
    }
}

/// Phase 110.G — enable TT dispatch on this executor by setting the
/// major-frame length in microseconds. `0` disables the gate.
///
/// # Safety
/// `executor` must be a valid pointer to an initialized executor.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_register_time_triggered_dispatcher(
    executor: *mut nros_executor_t,
    major_frame_us: u32,
) -> nros_ret_t {
    validate_not_null!(executor);
    let executor = &mut *executor;
    validate_state!(
        executor,
        nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED
    );
    let rust_exec = get_executor(&mut executor._opaque);
    rust_exec.register_time_triggered_dispatcher(major_frame_us);
    NROS_RET_OK
}

/// Register a new scheduling context with the executor. Phase 110.B.
///
/// On success writes the new `SchedContextId` through `out_sc_id` and
/// returns `NROS_RET_OK`. Returns `NROS_RET_FULL` when no slot is
/// available (build-time `NROS_EXECUTOR_MAX_SC` exhausted).
///
/// # Safety
/// All pointers must be valid and the executor initialized.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_create_sched_context(
    executor: *mut nros_executor_t,
    cfg: *const nros_sched_context_t,
    out_sc_id: *mut nros_sched_context_id_t,
) -> nros_ret_t {
    validate_not_null!(executor, cfg, out_sc_id);
    let executor = &mut *executor;
    validate_state!(
        executor,
        nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED
    );
    let rust_exec = get_executor(&mut executor._opaque);
    let sc = convert_sched_context(&*cfg);
    match rust_exec.create_sched_context(sc) {
        Ok(id) => {
            *out_sc_id = id.0;
            NROS_RET_OK
        }
        Err(_) => NROS_RET_FULL,
    }
}

/// Bind a registered callback to a scheduling context. The next
/// `spin_once` cycle dispatches that callback through the SC's queue
/// (FIFO bitmap or EDF heap, in the SC's priority bucket).
/// Phase 110.B.
///
/// `handle` is the index returned by the corresponding
/// `nros_executor_add_*` call. `sc_id` must be a value previously
/// returned from [`nros_executor_create_sched_context`] (or 0 for the
/// auto-created default Fifo SC).
///
/// Returns `NROS_RET_INVALID_ARGUMENT` for an out-of-range handle, an
/// empty entry slot, or an unknown `sc_id`.
///
/// # Safety
/// `executor` must be a valid pointer to an initialized executor.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_bind_handle_to_sched_context(
    executor: *mut nros_executor_t,
    handle: usize,
    sc_id: nros_sched_context_id_t,
) -> nros_ret_t {
    validate_not_null!(executor);
    let executor = &mut *executor;
    validate_state!(
        executor,
        nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED
    );
    let rust_exec = get_executor(&mut executor._opaque);
    let h = nros_node::executor::HandleId(handle);
    let id = nros_node::executor::sched_context::SchedContextId(sc_id);
    match rust_exec.bind_handle_to_sched_context(h, id) {
        Ok(()) => NROS_RET_OK,
        Err(_) => NROS_RET_INVALID_ARGUMENT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trigger_any_matches_behavior() {
        unsafe {
            let ready = [true, false, true];
            assert!(nros_executor_trigger_any(
                ready.as_ptr(),
                ready.len(),
                ptr::null_mut()
            ));

            let ready = [false, false, false];
            assert!(!nros_executor_trigger_any(
                ready.as_ptr(),
                ready.len(),
                ptr::null_mut()
            ));

            assert!(!nros_executor_trigger_any([].as_ptr(), 0, ptr::null_mut()));
        }
    }

    #[test]
    fn test_trigger_all_matches_behavior() {
        unsafe {
            let ready = [true, true, true];
            assert!(nros_executor_trigger_all(
                ready.as_ptr(),
                ready.len(),
                ptr::null_mut()
            ));

            let ready = [true, false, true];
            assert!(!nros_executor_trigger_all(
                ready.as_ptr(),
                ready.len(),
                ptr::null_mut()
            ));

            let ready = [false, false, false];
            assert!(!nros_executor_trigger_all(
                ready.as_ptr(),
                ready.len(),
                ptr::null_mut()
            ));

            assert!(!nros_executor_trigger_all([].as_ptr(), 0, ptr::null_mut()));
        }
    }

    #[test]
    fn test_trigger_always_matches_behavior() {
        unsafe {
            assert!(nros_executor_trigger_always(
                [].as_ptr(),
                0,
                ptr::null_mut()
            ));

            let ready = [false, false];
            assert!(nros_executor_trigger_always(
                ready.as_ptr(),
                ready.len(),
                ptr::null_mut()
            ));
        }
    }

    #[test]
    fn test_trigger_one_matches_behavior() {
        unsafe {
            let ready = [false, true, false];
            let mut idx: usize = 1;
            assert!(nros_executor_trigger_one(
                ready.as_ptr(),
                ready.len(),
                &mut idx as *mut usize as *mut core::ffi::c_void,
            ));

            idx = 0;
            assert!(!nros_executor_trigger_one(
                ready.as_ptr(),
                ready.len(),
                &mut idx as *mut usize as *mut core::ffi::c_void,
            ));

            idx = 10;
            assert!(!nros_executor_trigger_one(
                ready.as_ptr(),
                ready.len(),
                &mut idx as *mut usize as *mut core::ffi::c_void,
            ));

            // NULL context returns false (no dereference).
            assert!(!nros_executor_trigger_one(
                ready.as_ptr(),
                ready.len(),
                core::ptr::null_mut(),
            ));
        }
    }

    #[test]
    fn test_trigger_all_matches_rust_behavior() {
        let test_cases: &[(&[bool], bool)] = &[
            (&[true, true, true], true),
            (&[true, false, true], false),
            (&[false, false, false], false),
            (&[true], true),
            (&[false], false),
            (&[], false),
        ];

        for (case, expected) in test_cases {
            let c_result =
                unsafe { nros_executor_trigger_all(case.as_ptr(), case.len(), ptr::null_mut()) };
            assert_eq!(
                c_result, *expected,
                "trigger_all mismatch for {:?}: got {}, expected {}",
                case, c_result, expected
            );
        }
    }

    #[test]
    fn test_set_trigger_requires_init() {
        unsafe {
            let mut executor = nros_executor_get_zero_initialized();

            let ret = nros_executor_set_trigger(
                &mut executor,
                Some(nros_executor_trigger_all),
                ptr::null_mut(),
            );
            assert_eq!(ret, NROS_RET_NOT_INIT);
        }
    }

    #[test]
    fn test_set_trigger_null_executor() {
        unsafe {
            let ret = nros_executor_set_trigger(
                ptr::null_mut(),
                Some(nros_executor_trigger_all),
                ptr::null_mut(),
            );
            assert_eq!(ret, NROS_RET_INVALID_ARGUMENT);
        }
    }

    #[test]
    fn test_set_semantics_rclcpp() {
        unsafe {
            // Manually initialize (no real session needed for semantics test)
            let mut executor = nros_executor_get_zero_initialized();
            executor.state = nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED;
            executor.max_handles = 4;

            assert_eq!(
                executor.semantics,
                nros_executor_semantics_t::NROS_SEMANTICS_RCLCPP_EXECUTOR
            );

            let ret = nros_executor_set_semantics(
                &mut executor,
                nros_executor_semantics_t::NROS_SEMANTICS_RCLCPP_EXECUTOR,
            );
            assert_eq!(ret, NROS_RET_OK);
            assert_eq!(
                executor.semantics,
                nros_executor_semantics_t::NROS_SEMANTICS_RCLCPP_EXECUTOR
            );
        }
    }

    #[test]
    fn test_set_semantics_let() {
        unsafe {
            // Manually initialize (no real session needed for semantics test)
            let mut executor = nros_executor_get_zero_initialized();
            executor.state = nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED;
            executor.max_handles = 4;

            let ret = nros_executor_set_semantics(
                &mut executor,
                nros_executor_semantics_t::NROS_SEMANTICS_LOGICAL_EXECUTION_TIME,
            );
            assert_eq!(ret, NROS_RET_OK);
            assert_eq!(
                executor.semantics,
                nros_executor_semantics_t::NROS_SEMANTICS_LOGICAL_EXECUTION_TIME
            );
        }
    }

    #[test]
    fn test_set_semantics_requires_init() {
        unsafe {
            let mut executor = nros_executor_get_zero_initialized();

            let ret = nros_executor_set_semantics(
                &mut executor,
                nros_executor_semantics_t::NROS_SEMANTICS_LOGICAL_EXECUTION_TIME,
            );
            assert_eq!(ret, NROS_RET_NOT_INIT);
        }
    }

    #[test]
    fn test_spin_one_period_null() {
        unsafe {
            let ret = nros_executor_spin_one_period(ptr::null_mut(), 10_000_000);
            assert_eq!(ret, NROS_RET_INVALID_ARGUMENT);
        }
    }

    #[test]
    fn test_spin_one_period_zero_period() {
        unsafe {
            let mut executor = nros_executor_get_zero_initialized();
            let ret = nros_executor_spin_one_period(&mut executor, 0);
            assert_eq!(ret, NROS_RET_INVALID_ARGUMENT);
        }
    }

    #[test]
    fn test_spin_one_period_not_init() {
        unsafe {
            let mut executor = nros_executor_get_zero_initialized();
            let ret = nros_executor_spin_one_period(&mut executor, 10_000_000);
            assert_eq!(ret, NROS_RET_NOT_INIT);
        }
    }

    #[test]
    fn test_spin_period_null() {
        unsafe {
            let ret = nros_executor_spin_period(ptr::null_mut(), 10_000_000);
            assert_eq!(ret, NROS_RET_INVALID_ARGUMENT);
        }
    }

    #[test]
    fn test_spin_period_zero_period() {
        unsafe {
            let mut executor = nros_executor_get_zero_initialized();
            let ret = nros_executor_spin_period(&mut executor, 0);
            assert_eq!(ret, NROS_RET_INVALID_ARGUMENT);
        }
    }

    #[test]
    fn test_invocation_time_ns_initialized() {
        let executor = nros_executor_get_zero_initialized();
        assert_eq!(executor.invocation_time_ns, 0);
    }

    #[test]
    fn test_per_type_counters_initialized_to_zero() {
        let executor = nros_executor_get_zero_initialized();
        assert_eq!(executor.subscription_count, 0);
        assert_eq!(executor.timer_count, 0);
        assert_eq!(executor.service_count, 0);
    }

    #[test]
    fn test_remaining_handles_null() {
        unsafe {
            assert_eq!(nros_executor_get_remaining_handles(ptr::null()), -1);
        }
    }

    #[test]
    fn test_remaining_capacity_initial() {
        unsafe {
            let mut executor = nros_executor_get_zero_initialized();
            executor.state = nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED;
            executor.max_handles = NROS_EXECUTOR_MAX_HANDLES;

            assert_eq!(
                nros_executor_get_remaining_handles(&executor),
                NROS_EXECUTOR_MAX_HANDLES as c_int
            );
        }
    }

    #[test]
    fn test_max_handles_equals_max_cbs() {
        assert_eq!(NROS_EXECUTOR_MAX_HANDLES, nros_node::config::MAX_CBS);
    }
}

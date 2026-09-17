//! Guard condition API for nros C API.
//!
//! Guard conditions provide a mechanism for signaling the executor from
//! another thread. They are used for shutdown requests, custom triggers,
//! and inter-thread communication.

use core::{ffi::c_void, ptr};

use crate::{constants::GUARD_HANDLE_OPAQUE_U64S, error::*};

// ============================================================================
// Guard Condition Types
// ============================================================================

/// Guard condition state.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_guard_condition_state_t {
    /// Not initialized
    NROS_GUARD_CONDITION_STATE_UNINITIALIZED = 0,
    /// Initialized and ready
    NROS_GUARD_CONDITION_STATE_INITIALIZED = 1,
    /// Shutdown
    NROS_GUARD_CONDITION_STATE_SHUTDOWN = 2,
}

/// Guard condition callback type.
pub type nros_guard_condition_callback_t = Option<unsafe extern "C" fn(context: *mut c_void)>;

/// Guard condition structure.
#[repr(C)]
pub struct nros_guard_condition_t {
    /// Current state
    pub state: nros_guard_condition_state_t,
    /// Triggered flag (volatile for cross-thread visibility)
    pub triggered: bool,
    /// Callback function
    pub callback: nros_guard_condition_callback_t,
    /// User context pointer
    pub context: *mut c_void,
    /// Handle ID from executor registration (SIZE_MAX = not registered)
    pub handle_id: usize,
    /// Whether the guard handle has been initialized
    pub _guard_valid: bool,
    /// Inline opaque storage for the guard condition handle (set by executor).
    /// Avoids heap allocation — managed by executor registration / guard_condition_fini.
    pub _guard_opaque: [u64; GUARD_HANDLE_OPAQUE_U64S],
}

// GUARD_HANDLE_OPAQUE_U64S is computed from size_of::<GuardCondition>() in
// opaque_sizes.rs — always large enough by construction.

// Safety: The triggered flag is designed for cross-thread access.
// The callback and context are only accessed from the executor thread.
unsafe impl Send for nros_guard_condition_t {}
unsafe impl Sync for nros_guard_condition_t {}

impl Default for nros_guard_condition_t {
    fn default() -> Self {
        Self {
            state: nros_guard_condition_state_t::NROS_GUARD_CONDITION_STATE_UNINITIALIZED,
            triggered: false,
            callback: None,
            context: ptr::null_mut(),
            handle_id: usize::MAX,
            _guard_valid: false,
            _guard_opaque: [0u64; GUARD_HANDLE_OPAQUE_U64S],
        }
    }
}

impl nros_guard_condition_t {
    // `get_callback` / `get_context` lived here and are gone with phase-417
    // W4.e: their one caller was `nros_executor_add_guard_condition`, reading
    // back the pair a separate `set_callback` had stored so it could build the
    // registration closure. The closure is built from the creation call's own
    // arguments now, so there is nothing to read back.

    /// Set the handle ID from executor registration.
    pub(crate) fn set_handle_id(&mut self, id: nros_node::HandleId) {
        self.handle_id = id.0;
    }

    /// Set the guard handle for external triggering.
    pub(crate) fn set_guard_handle(&mut self, handle: nros_node::GuardCondition) {
        unsafe {
            core::ptr::write(
                self._guard_opaque.as_mut_ptr() as *mut nros_node::GuardCondition,
                handle,
            );
        }
        self._guard_valid = true;
    }

    /// Get the guard handle for triggering.
    pub(crate) fn get_guard_handle(&self) -> Option<&nros_node::GuardCondition> {
        if !self._guard_valid {
            None
        } else {
            Some(unsafe { &*(self._guard_opaque.as_ptr() as *const nros_node::GuardCondition) })
        }
    }
}

// ============================================================================
// Guard Condition Functions
// ============================================================================

/// Get a zero-initialized guard condition.
#[unsafe(no_mangle)]
pub extern "C" fn rcl_get_zero_initialized_guard_condition() -> nros_guard_condition_t {
    nros_guard_condition_t::default()
}

/// Create a guard condition on `node` — the ONE creation shape (phase-417
/// W4.e, RFC-0089 stage 4).
///
/// A guard condition is the cross-thread / ISR wake source: any thread may call
/// [`nros_guard_condition_trigger`] and `callback` runs on the executor's next
/// spin. The flag lives in the executor's arena, so creation has to reach an
/// executor — and the thing a caller has in hand is a NODE, which is why the
/// node is the owner in all three of our languages. `callback` may be NULL for
/// a wake with no work attached; `context` is handed back to it unchanged.
///
/// **This replaced three calls, and until 2026-09-18 the first two produced an
/// INERT object.** `nros_guard_condition_init(guard, support)` only zeroed
/// fields — nothing reached the arena until `nros_executor_add_guard_condition`
/// ran, so between the two the handle read `handle_id == SIZE_MAX` and a
/// trigger fell back to a local flag no executor watches. Binding the callback
/// after the fact was also the shape we already REFUSE in C++ at
/// `GuardCondition::set_on_trigger_callback`, so one constraint was enforced in
/// two languages and contradicted in the third. All three names are retired
/// with no forwarder (stage 6 step B): the whole in-tree cost was one example.
///
/// # Ordering
/// `node` must be bound to an executor through `nros_executor_node_init` — the
/// executor exists first, then the node. A node from the legacy
/// `rclc_node_init_default` path reaches no executor and gets
/// `NROS_RET_NOT_INIT`, which is what `nros_node_resolve_name` answers for the
/// same reason: the capability is UNREACHABLE from that node, not absent.
///
/// # Returns
/// * `NROS_RET_OK` — created, registered, and the callback is bound.
/// * `NROS_RET_INVALID_ARGUMENT` — `node` or `out` is NULL.
/// * `NROS_RET_NOT_INIT` — the node is uninitialised, or is not bound to an
///   executor.
/// * `NROS_RET_STALE_NODE` — the node slot has been retired.
/// * `NROS_RET_BAD_SEQUENCE` — `out` is not zero-initialised (a double create).
/// * `NROS_RET_FULL` — the executor's handle table is full.
///
/// # Safety
/// * `node` must point to a valid, executor-bound `nros_node_t`.
/// * `out` must point to writable storage that outlives the executor —
///   registration is one-way and the arena records the entity pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_node_create_guard_condition(
    node: *mut crate::node::nros_node_t,
    out: *mut nros_guard_condition_t,
    callback: nros_guard_condition_callback_t,
    context: *mut c_void,
) -> nros_ret_t {
    validate_not_null!(node, out);

    let node_ref = &*node;
    validate_state!(
        node_ref,
        crate::node::nros_node_state_t::NROS_NODE_STATE_INITIALIZED
    );

    let guard = &mut *out;
    validate_state!(
        guard,
        nros_guard_condition_state_t::NROS_GUARD_CONDITION_STATE_UNINITIALIZED,
        NROS_RET_BAD_SEQUENCE
    );

    // The arena the flag lives in belongs to the executor; a node that reaches
    // none cannot create one. Same answer, same reason, as
    // `nros_node_resolve_name` gives for the remap table.
    //
    // The predicate is "is this node BOUND to an executor", NOT
    // `is_multi_session()` — that one also requires `node_id != 0`, and the
    // FIRST node `nros_executor_node_init` builds takes slot 0 (the primary
    // slot; `executor_param_node_keying.c` asserts exactly that). A node's
    // right to create an entity does not depend on how many siblings it has.
    if node_ref.executor.is_null() {
        return NROS_RET_NOT_INIT;
    }
    if !crate::node::node_ref_is_live(crate::node::node_ref_of(node)) {
        return NROS_RET_STALE_NODE;
    }

    let executor = &mut *(node_ref.executor as *mut crate::executor::nros_executor_t);
    validate_state!(
        executor,
        crate::executor::nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED
    );
    if executor.handle_count >= executor.max_handles {
        return NROS_RET_FULL;
    }

    let node_id = nros_node::executor::NodeId::from_raw(node_ref.node_id);
    let rust_exec = crate::executor::get_executor(&mut executor._opaque);

    // The callback is bound HERE, at creation, and cannot be swapped later.
    let wrapper = move || {
        if let Some(cb) = callback {
            // SAFETY: the C callback and its context remain valid for the
            // lifetime of the executor — registration is one-way.
            cb(context);
        }
    };

    match rust_exec.register_guard_condition_on(Some(node_id), wrapper) {
        Ok((handle_id, guard_handle)) => {
            guard.callback = callback;
            guard.context = context;
            guard.triggered = false;
            guard.set_handle_id(handle_id);
            guard.set_guard_handle(guard_handle);
            guard.state = nros_guard_condition_state_t::NROS_GUARD_CONDITION_STATE_INITIALIZED;
            crate::executor::record_trigger_entity(
                &mut executor._handle_entities,
                handle_id,
                out as *mut c_void,
            );
            executor.handle_count += 1;
            NROS_RET_OK
        }
        Err(_) => NROS_RET_ERROR,
    }
}

/// Trigger a guard condition — thread-safe and lock-free.
///
/// Stores into the atomic flag the executor's arena holds for this guard and
/// then fires the runtime wake hook, so an ISR or a foreign task releases a
/// `spin_once` already blocked in `drive_io` rather than waiting out its poll
/// timeout. The callback runs on the executor's thread, at its next spin.
///
/// **This is `nros_generated.h`'s `nros_<entity>_<verb>` spelling, and until
/// 2026-09-18 it DID NOT EXIST** — only rcl's verb-first
/// [`rcl_trigger_guard_condition`] did, while the ledger, this crate's own docs
/// and the custom-platform example's README all named this one. The example's
/// README told a reader to call a symbol nothing defined.
///
/// # Safety
/// * `guard` must be NULL or point to a valid `nros_guard_condition_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_guard_condition_trigger(
    guard: *mut nros_guard_condition_t,
) -> nros_ret_t {
    validate_not_null!(guard);

    let guard = &mut *guard;

    validate_state!(
        guard,
        nros_guard_condition_state_t::NROS_GUARD_CONDITION_STATE_INITIALIZED
    );

    // Registered — trigger through the executor's arena flag. This is now the
    // only state an initialised guard condition can be in: a guard condition
    // exists because `nros_node_create_guard_condition` registered it.
    if let Some(handle) = guard.get_guard_handle() {
        handle.trigger();
        return NROS_RET_OK;
    }

    // A guard whose handle went away (post-`fini` reuse) still answers the
    // polling readers, so keep the local flag consistent rather than lying.
    crate::platform::atomic_store_bool(&mut guard.triggered as *mut bool, true);

    NROS_RET_OK
}

/// rcl's spelling of [`nros_guard_condition_trigger`], forwarding to it.
///
/// Kept, not retired: rcl HAS this name, and RFC-0089's alias rule makes a name
/// upstream has load-bearing rather than a courtesy — a ported rcl node writes
/// `rcl_trigger_guard_condition(&gc)` and must keep compiling. It is not
/// deprecated for the same reason.
///
/// # Safety
/// See [`nros_guard_condition_trigger`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rcl_trigger_guard_condition(
    guard: *mut nros_guard_condition_t,
) -> nros_ret_t {
    nros_guard_condition_trigger(guard)
}

/// Is this guard condition triggered and not yet dispatched?
///
/// The RFC-0022 POLLING tier: a task that owns its own loop and never calls a
/// callback still needs to see the flag another thread or an ISR set. rcl has
/// no such reader, so this is ours.
///
/// **It reads the ARENA flag, which is the one `nros_guard_condition_trigger`
/// writes** — and it had to, from the moment creation became registration. The
/// local `triggered` byte this used to read was only ever written by the
/// fallback path of an UNREGISTERED guard, so with the retirement of
/// `nros_guard_condition_init` it would have answered `false` for every guard
/// condition that exists: a reader that compiles, runs, and is always wrong.
/// The executor CONSUMES the flag when it dispatches, so this answers "set and
/// not yet dispatched"; a caller that both polls this and spins the executor is
/// racing itself.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_guard_condition_is_triggered(
    guard: *const nros_guard_condition_t,
) -> bool {
    if guard.is_null() {
        return false;
    }

    let guard = &*guard;

    if guard.state != nros_guard_condition_state_t::NROS_GUARD_CONDITION_STATE_INITIALIZED {
        return false;
    }

    if let Some(handle) = guard.get_guard_handle() {
        return handle.is_triggered();
    }

    crate::platform::atomic_load_bool(&guard.triggered as *const bool)
}

/// Clear the triggered flag without dispatching — the other half of
/// [`nros_guard_condition_is_triggered`], for a polling owner that handled the
/// event itself. Reads the same arena flag, for the same reason.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_guard_condition_clear(
    guard: *mut nros_guard_condition_t,
) -> nros_ret_t {
    validate_not_null!(guard);

    let guard = &mut *guard;

    if let Some(handle) = guard.get_guard_handle() {
        let _ = handle.clear();
        return NROS_RET_OK;
    }

    crate::platform::atomic_store_bool(&mut guard.triggered as *mut bool, false);

    NROS_RET_OK
}

/// Check if guard condition is valid (initialized).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_guard_condition_is_valid(
    guard: *const nros_guard_condition_t,
) -> bool {
    if guard.is_null() {
        return false;
    }

    let guard = &*guard;
    guard.state == nros_guard_condition_state_t::NROS_GUARD_CONDITION_STATE_INITIALIZED
}

/// Finalize a guard condition.
///
/// IDEMPOTENT: a zero-initialised or already-finalised handle is not an error
/// and returns `NROS_RET_OK`. A NULL pointer is still
/// `NROS_RET_INVALID_ARGUMENT`.
///
/// That is rcl's contract, MEASURED rather than assumed (phase-417 stage 3 —
/// the ledger row carried an explicit evidence bound because the host it was
/// written on had only the Humble headers, which do not state the guarantee in
/// words). `rcl/src/rcl/guard_condition.c` @ humble:
///
/// ```text
/// rcl_guard_condition_fini(rcl_guard_condition_t * guard_condition)
/// {
///   RCL_CHECK_ARGUMENT_FOR_NULL(guard_condition, RCL_RET_INVALID_ARGUMENT);
///   rcl_ret_t result = RCL_RET_OK;
///   if (guard_condition->impl) { … }
///   return result;
/// }
/// ```
///
/// so NULL errors and `impl == NULL` — a zero-initialised or already-finalised
/// handle — returns OK. Ours returned `NROS_RET_NOT_INIT` for the second case,
/// and since `rcl_*_fini` is `RCL_WARN_UNUSED` upstream, a ported cleanup path
/// that checks its return reported a shutdown failure that had not happened.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rcl_guard_condition_fini(
    guard: *mut nros_guard_condition_t,
) -> nros_ret_t {
    validate_not_null!(guard);

    let guard = &mut *guard;

    if guard.state != nros_guard_condition_state_t::NROS_GUARD_CONDITION_STATE_INITIALIZED {
        // Nothing to tear down. Idempotent, per rcl.
        return NROS_RET_OK;
    }

    // Drop the inline guard handle if initialized
    if guard._guard_valid {
        core::ptr::drop_in_place(guard._guard_opaque.as_mut_ptr() as *mut nros_node::GuardCondition);
    }

    guard.triggered = false;
    guard.callback = None;
    guard.context = ptr::null_mut();
    guard.handle_id = usize::MAX;
    guard._guard_valid = false;
    guard._guard_opaque = [0u64; GUARD_HANDLE_OPAQUE_U64S];
    guard.state = nros_guard_condition_state_t::NROS_GUARD_CONDITION_STATE_SHUTDOWN;

    NROS_RET_OK
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// phase-417 stage 3 / ledger row `c:guard_condition_fini`.
    ///
    /// rcl's `fini` family is idempotent: `rcl_guard_condition_fini` errors only
    /// on a NULL pointer, and returns `RCL_RET_OK` when `impl` is NULL — a
    /// zero-initialised or already-finalised handle. Ours returned
    /// `NROS_RET_NOT_INIT` for the second case, so a ported cleanup path that
    /// checks the return (it is `RCL_WARN_UNUSED` upstream) reported a shutdown
    /// failure that had not happened.
    #[test]
    fn guard_condition_fini_is_idempotent() {
        unsafe {
            let mut guard = rcl_get_zero_initialized_guard_condition();
            assert_eq!(
                rcl_guard_condition_fini(&mut guard),
                NROS_RET_OK,
                "fini on a zero-initialised guard condition is not an error"
            );

            guard.state = nros_guard_condition_state_t::NROS_GUARD_CONDITION_STATE_SHUTDOWN;
            assert_eq!(
                rcl_guard_condition_fini(&mut guard),
                NROS_RET_OK,
                "a repeated fini is not an error"
            );

            assert_eq!(
                rcl_guard_condition_fini(ptr::null_mut()),
                NROS_RET_INVALID_ARGUMENT,
                "a NULL pointer is still an argument error, as in rcl"
            );
        }
    }

    #[test]
    fn test_guard_condition_default() {
        let guard = rcl_get_zero_initialized_guard_condition();
        assert_eq!(
            guard.state,
            nros_guard_condition_state_t::NROS_GUARD_CONDITION_STATE_UNINITIALIZED
        );
        assert!(!guard.triggered);
        assert!(guard.callback.is_none());
        assert!(guard.context.is_null());
    }

    // phase-417 W4.e — `nros_guard_condition_init` / `_set_callback` are
    // RETIRED. Their null-argument tests became these three, on the verb that
    // replaced them; the behavioural half (a created guard is REGISTERED, and a
    // trigger from another thread wakes the executor) needs a live executor and
    // a backend, so it is the run probe `tests/run/node_guard_condition.c` on
    // the `just check c` lane.
    #[test]
    fn create_guard_condition_rejects_a_null_node() {
        unsafe {
            let mut guard = rcl_get_zero_initialized_guard_condition();
            let ret = nros_node_create_guard_condition(
                ptr::null_mut(),
                &mut guard,
                None,
                ptr::null_mut(),
            );
            assert_eq!(ret, NROS_RET_INVALID_ARGUMENT);
        }
    }

    #[test]
    fn create_guard_condition_rejects_a_null_out() {
        unsafe {
            let mut node = crate::node::nros_node_t::default();
            let ret =
                nros_node_create_guard_condition(&mut node, ptr::null_mut(), None, ptr::null_mut());
            assert_eq!(ret, NROS_RET_INVALID_ARGUMENT);
        }
    }

    /// An UNBOUND node reaches no executor, so it reaches no arena — and the
    /// arena is where the flag lives. `NROS_RET_NOT_INIT` is the same answer
    /// `nros_node_resolve_name` gives for the same reason, rather than the
    /// silently inert object the retired three-call shape produced.
    #[test]
    fn create_guard_condition_refuses_a_node_with_no_executor() {
        unsafe {
            let mut node = crate::node::nros_node_t::default();
            node.state = crate::node::nros_node_state_t::NROS_NODE_STATE_INITIALIZED;
            let mut guard = rcl_get_zero_initialized_guard_condition();
            let ret =
                nros_node_create_guard_condition(&mut node, &mut guard, None, ptr::null_mut());
            assert_eq!(ret, NROS_RET_NOT_INIT);
            assert_eq!(
                guard.handle_id,
                usize::MAX,
                "a refused creation must leave the handle unregistered"
            );
            assert_eq!(
                guard.state,
                nros_guard_condition_state_t::NROS_GUARD_CONDITION_STATE_UNINITIALIZED
            );
        }
    }

    #[test]
    fn test_guard_condition_trigger_not_init() {
        unsafe {
            let mut guard = rcl_get_zero_initialized_guard_condition();
            assert_eq!(nros_guard_condition_trigger(&mut guard), NROS_RET_NOT_INIT);
            assert_eq!(rcl_trigger_guard_condition(&mut guard), NROS_RET_NOT_INIT);
        }
    }

    #[test]
    fn test_guard_condition_trigger_null() {
        unsafe {
            assert_eq!(
                nros_guard_condition_trigger(ptr::null_mut()),
                NROS_RET_INVALID_ARGUMENT
            );
            assert_eq!(
                rcl_trigger_guard_condition(ptr::null_mut()),
                NROS_RET_INVALID_ARGUMENT
            );
        }
    }

    /// phase-417 W4.e — `nros_guard_condition_trigger` did not EXIST: the
    /// ledger, this crate's own docs and the custom-platform README all named
    /// it while only rcl's verb-first spelling was exported. Both ship now and
    /// rcl's forwards, so they cannot answer differently.
    #[test]
    fn both_trigger_spellings_reach_one_implementation() {
        unsafe {
            let mut ours = rcl_get_zero_initialized_guard_condition();
            ours.state = nros_guard_condition_state_t::NROS_GUARD_CONDITION_STATE_INITIALIZED;
            let mut theirs = rcl_get_zero_initialized_guard_condition();
            theirs.state = nros_guard_condition_state_t::NROS_GUARD_CONDITION_STATE_INITIALIZED;

            assert_eq!(nros_guard_condition_trigger(&mut ours), NROS_RET_OK);
            assert_eq!(rcl_trigger_guard_condition(&mut theirs), NROS_RET_OK);
            assert_eq!(
                nros_guard_condition_is_triggered(&ours),
                nros_guard_condition_is_triggered(&theirs),
                "the two spellings must observe the same effect"
            );
        }
    }

    #[test]
    fn test_guard_condition_is_triggered_null() {
        unsafe {
            let result = nros_guard_condition_is_triggered(ptr::null());
            assert!(!result);
        }
    }

    #[test]
    fn test_guard_condition_is_triggered_not_init() {
        unsafe {
            let guard = rcl_get_zero_initialized_guard_condition();
            let result = nros_guard_condition_is_triggered(&guard);
            assert!(!result);
        }
    }

    #[test]
    fn test_guard_condition_clear_null() {
        unsafe {
            let ret = nros_guard_condition_clear(ptr::null_mut());
            assert_eq!(ret, NROS_RET_INVALID_ARGUMENT);
        }
    }

    #[test]
    fn test_guard_condition_is_valid_null() {
        unsafe {
            let result = nros_guard_condition_is_valid(ptr::null());
            assert!(!result);
        }
    }

    #[test]
    fn test_guard_condition_is_valid_not_init() {
        unsafe {
            let guard = rcl_get_zero_initialized_guard_condition();
            let result = nros_guard_condition_is_valid(&guard);
            assert!(!result);
        }
    }

    #[test]
    fn test_guard_condition_fini_null() {
        unsafe {
            let ret = rcl_guard_condition_fini(ptr::null_mut());
            assert_eq!(ret, NROS_RET_INVALID_ARGUMENT);
        }
    }

    // `test_guard_condition_fini_not_init` used to live here, asserting
    // `NROS_RET_NOT_INIT` for a zero-initialised handle. It PINNED the
    // divergence rather than recording it — rcl returns OK for exactly that
    // case. Replaced by `guard_condition_fini_is_idempotent` above
    // (phase-417 stage 3).

    // Test with a mock initialized guard condition
    #[test]
    fn test_guard_condition_trigger_and_clear() {
        unsafe {
            // Manually set up an initialized guard condition (bypassing support check)
            let mut guard = rcl_get_zero_initialized_guard_condition();
            guard.state = nros_guard_condition_state_t::NROS_GUARD_CONDITION_STATE_INITIALIZED;

            // Initially not triggered
            assert!(!nros_guard_condition_is_triggered(&guard));

            // Trigger it
            let ret = nros_guard_condition_trigger(&mut guard);
            assert_eq!(ret, NROS_RET_OK);
            assert!(nros_guard_condition_is_triggered(&guard));

            // Clear it
            let ret = nros_guard_condition_clear(&mut guard);
            assert_eq!(ret, NROS_RET_OK);
            assert!(!nros_guard_condition_is_triggered(&guard));
        }
    }

    #[test]
    fn test_guard_condition_is_valid_initialized() {
        unsafe {
            let mut guard = rcl_get_zero_initialized_guard_condition();
            guard.state = nros_guard_condition_state_t::NROS_GUARD_CONDITION_STATE_INITIALIZED;

            let result = nros_guard_condition_is_valid(&guard);
            assert!(result);
        }
    }

    #[test]
    fn test_guard_condition_fini_initialized() {
        unsafe {
            let mut guard = rcl_get_zero_initialized_guard_condition();
            guard.state = nros_guard_condition_state_t::NROS_GUARD_CONDITION_STATE_INITIALIZED;
            guard.triggered = true;

            let ret = rcl_guard_condition_fini(&mut guard);
            assert_eq!(ret, NROS_RET_OK);
            assert_eq!(
                guard.state,
                nros_guard_condition_state_t::NROS_GUARD_CONDITION_STATE_SHUTDOWN
            );
            assert!(!guard.triggered);
        }
    }

    // `test_guard_condition_set_callback_*` (three of them) lived here and are
    // gone with the verb: a callback is bound at CREATION now, which is the
    // constraint C++ already enforced at
    // `GuardCondition::set_on_trigger_callback` and C contradicted. What they
    // asserted — the pair is stored and readable — the run probe asserts
    // against a callback that actually fires.
}

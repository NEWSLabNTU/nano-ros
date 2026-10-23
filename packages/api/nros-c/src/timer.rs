//! Timer API for nros C API.
//!
//! Timers provide periodic callbacks for time-based operations.

use core::{ffi::c_void, ptr};

use crate::{
    clock::{rcl_clock_t, rclc_clock_state_t},
    error::*,
};

/// Timer callback function type.
///
/// # Parameters
/// * `timer` - Pointer to the timer that triggered
/// * `context` - User-provided context pointer
pub type rclc_timer_callback_with_context_t =
    Option<unsafe extern "C" fn(timer: *mut rcl_timer_t, context: *mut c_void)>;

/// When a timer activation was due and when it ran — Jazzy rcl's
/// `rcl_timer_call_info_t` (issue 1974), same name, same two fields.
///
/// Both are `rcl_time_point_value_t` nanoseconds on the TIMER'S clock: the
/// clock it was initialised on (`rclc_timer_init_on_clock*`), or the platform
/// steady clock for a wall timer. `actual_call_time >= expected_call_time`
/// always; the difference is the activation's lateness.
///
/// The executor's timer dispatcher is the one producer
/// (`nros_node::TimerCallInfo`); this is a field-for-field copy of it, handed
/// to an `rclc_timer_callback_with_info_t` for the duration of the call.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct rcl_timer_call_info_t {
    /// The deadline the activation was serving (ns, timer's clock).
    pub expected_call_time: i64,
    /// When the callback ran (ns, timer's clock).
    pub actual_call_time: i64,
}

impl From<nros_node::TimerCallInfo> for rcl_timer_call_info_t {
    fn from(info: nros_node::TimerCallInfo) -> Self {
        Self {
            expected_call_time: info.expected_call_time,
            actual_call_time: info.actual_call_time,
        }
    }
}

/// Timer callback that also receives the activation's call info — issue 1974.
///
/// rcl hands call info out through `rcl_timer_call_with_info`, a dispatch
/// entry point the caller's own wait-set loop calls; here the executor
/// dispatches, so the info rides on the callback instead. `info` is valid for
/// the duration of the call only.
///
/// # Parameters
/// * `timer` - Pointer to the timer that triggered
/// * `info` - Expected vs actual call time of this activation
/// * `context` - User-provided context pointer
pub type rclc_timer_callback_with_info_t = Option<
    unsafe extern "C" fn(
        timer: *mut rcl_timer_t,
        info: *const rcl_timer_call_info_t,
        context: *mut c_void,
    ),
>;

/// Timer state
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum rclc_timer_state_t {
    /// Not initialized
    RCLC_TIMER_STATE_UNINITIALIZED = 0,
    /// Initialized and running
    RCLC_TIMER_STATE_RUNNING = 1,
    /// Initialized but canceled
    RCLC_TIMER_STATE_CANCELED = 2,
    /// Shutdown
    RCLC_TIMER_STATE_SHUTDOWN = 3,
}

/// Timer structure.
///
/// Already an arena handle before phase-487: `rclc_executor_add_timer`
/// creates the timer inside the executor arena and records
/// `(handle_id, _executor)` here, and every live accessor forwards there. The
/// period, callback, context and clock are the REGISTRATION INPUTS:
/// rclc's `rclc_timer_init_default(timer, support, period, callback)` takes
/// them at init, before any executor exists, and a removed timer can be added
/// again, so they stay. phase-487 W3 removed the one field that was a copy,
/// `last_call_time_ns`: written by init and reset, read by nothing, and stale
/// for any timer the executor ran (issue 1049).
#[repr(C)]
pub struct rcl_timer_t {
    /// Current state
    pub state: rclc_timer_state_t,
    /// Period in nanoseconds
    pub period_ns: u64,
    /// User callback function
    pub callback: rclc_timer_callback_with_context_t,
    /// User context pointer
    pub context: *mut c_void,
    /// The clock this timer is scheduled against, or NULL for the WALL timer
    /// `rclc_timer_init_with_context` creates (phase-430 W1).
    ///
    /// rcl's `rcl_timer_init` takes a `rcl_clock_t *` here and rcl's timer
    /// keeps it; ours does the same, in the same position, so a reader of one
    /// recognises the other. The clock must outlive the timer — the executor
    /// reads its `type` at `rclc_executor_add_timer` time, not at init.
    pub clock: *const rcl_clock_t,
    /// Handle ID from executor registration (SIZE_MAX = not registered)
    pub handle_id: usize,
    /// Opaque pointer to internal executor (set by rclc_executor_add_timer)
    pub _executor: *mut c_void,
    /// The call-info callback `rclc_timer_init_with_info` registers, or NULL
    /// (issue 1974). Exactly one of `callback` / `info_callback` is set on an
    /// initialised timer. APPENDED, so every field above keeps its offset.
    pub info_callback: rclc_timer_callback_with_info_t,
}

impl Default for rcl_timer_t {
    fn default() -> Self {
        Self {
            state: rclc_timer_state_t::RCLC_TIMER_STATE_UNINITIALIZED,
            period_ns: 0,
            callback: None,
            context: ptr::null_mut(),
            clock: ptr::null(),
            handle_id: usize::MAX,
            _executor: ptr::null_mut(),
            info_callback: None,
        }
    }
}

// Internal helper methods for executor
impl rcl_timer_t {
    /// Get the callback function
    pub(crate) fn get_callback(&self) -> rclc_timer_callback_with_context_t {
        self.callback
    }

    /// Get the call-info callback (issue 1974)
    pub(crate) fn get_info_callback(&self) -> rclc_timer_callback_with_info_t {
        self.info_callback
    }

    /// Get the user context
    pub(crate) fn get_context(&self) -> *mut c_void {
        self.context
    }

    /// Set the handle ID from executor registration
    pub(crate) fn set_handle_id(&mut self, id: nros_node::HandleId) {
        self.handle_id = id.to_raw();
    }

    /// Set the executor pointer (called by rclc_executor_add_timer)
    pub(crate) fn set_executor_ptr(&mut self, executor: *mut c_void) {
        self._executor = executor;
    }
}

/// Get a zero-initialized timer.
#[unsafe(no_mangle)]
pub extern "C" fn rcl_get_zero_initialized_timer() -> rcl_timer_t {
    rcl_timer_t::default()
}

/// Initialize a WALL timer — the clock-less case (phase-430 W1).
///
/// phase-492 W1.b: this was spelled `rcl_timer_init`, which is rcl's name for
/// a different contract (`<rclc/detail/rcl_compat.h>` §4), and took the support as a
/// validity witness it never otherwise read. It takes rclc's `_with_context`
/// spelling — the callback gets the caller's context — and no witness: the
/// timer belongs to no node and no session until `rclc_executor_add_timer`,
/// which is where the executor is checked. So a C component the generated
/// entry hosts, which has no support, creates timers like any other caller.
///
/// The timer is scheduled against the executor's monotonic spin delta, which
/// is what every wall-timer caller has always got and is what rclcpp
/// spells `create_wall_timer`: a paused simulator does not pause it. For a
/// timer that follows ROS time — a bag replay, a simulator's `/clock` — use
/// [`rclc_timer_init_on_clock`], which takes the clock in the position
/// `rcl_timer_init` puts it.
///
/// # Parameters
/// * `timer` - Pointer to a zero-initialized timer
/// * `period_ns` - Timer period in nanoseconds
/// * `callback` - Callback function to invoke when timer fires
/// * `context` - User context pointer passed to callback (can be NULL)
///
/// # Returns
/// * `RCL_RET_OK` on success
/// * `RCL_RET_INVALID_ARGUMENT` if any required pointer is NULL or period is 0
///
/// # Safety
/// * All required pointers must be valid
/// * `callback` must be a valid function pointer
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_timer_init_with_context(
    timer: *mut rcl_timer_t,
    period_ns: u64,
    callback: rclc_timer_callback_with_context_t,
    context: *mut c_void,
) -> rcl_ret_t {
    unsafe {
        timer_init_inner(
            timer,
            ptr::null(),
            period_ns,
            TimerCallback::Plain(callback),
            context,
            true,
        )
    }
}

/// [`rclc_timer_init_with_context`] with Jazzy's `autostart` flag — issue 1974,
/// our counterpart of `rclc_timer_init_default2`.
///
/// `autostart == false` creates the timer CANCELLED: `rclc_executor_add_timer`
/// registers it cancelled and it does not fire until `rcl_timer_reset`, whose
/// call starts its first period. `true` is exactly
/// [`rclc_timer_init_with_context`].
///
/// Not spelled `rclc_timer_init_default2` for the reason
/// `rclc_timer_init_default` is refused (`<rclc/detail/rcl_compat.h>` §4):
/// rclc's callback receives the time since its last call, ours the caller's
/// context.
///
/// # Safety
/// As [`rclc_timer_init_with_context`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_timer_init_with_context2(
    timer: *mut rcl_timer_t,
    period_ns: u64,
    callback: rclc_timer_callback_with_context_t,
    context: *mut c_void,
    autostart: bool,
) -> rcl_ret_t {
    unsafe {
        timer_init_inner(
            timer,
            ptr::null(),
            period_ns,
            TimerCallback::Plain(callback),
            context,
            autostart,
        )
    }
}

/// [`rclc_timer_init_on_clock`] with Jazzy's `autostart` flag — issue 1974,
/// our counterpart of `rcl_timer_init2(..., autostart)`: clock second, as
/// rcl's, and `autostart` last, as rcl's.
///
/// `autostart == false` creates the timer cancelled; `rcl_timer_reset` starts
/// it, and its first period counts from that reset on the timer's own clock.
///
/// # Safety
/// As [`rclc_timer_init_on_clock`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_timer_init_on_clock2(
    timer: *mut rcl_timer_t,
    clock: *const rcl_clock_t,
    period_ns: u64,
    callback: rclc_timer_callback_with_context_t,
    context: *mut c_void,
    autostart: bool,
) -> rcl_ret_t {
    if clock.is_null() {
        return RCL_RET_INVALID_ARGUMENT;
    }
    unsafe {
        timer_init_inner(
            timer,
            clock,
            period_ns,
            TimerCallback::Plain(callback),
            context,
            autostart,
        )
    }
}

/// Initialise a timer whose callback receives the activation's
/// [`rcl_timer_call_info_t`] — issue 1974, the C counterpart of Jazzy's
/// `rcl_timer_call_with_info` / `rclcpp::TimerInfoCallbackType`.
///
/// `clock` NULL is the wall timer ([`rclc_timer_init_with_context`]'s
/// schedule, call times on the platform steady clock); a non-NULL clock
/// selects the schedule exactly as [`rclc_timer_init_on_clock`] does, and the
/// call times are on that clock. `autostart` as in
/// [`rclc_timer_init_on_clock2`].
///
/// # Returns
/// * `RCL_RET_OK` on success
/// * `RCL_RET_INVALID_ARGUMENT` if `timer` or `callback` is NULL, the period
///   is 0, or the clock names no schedule
/// * `RCL_RET_NOT_INIT` if the clock is not initialized
///
/// # Safety
/// As [`rclc_timer_init_on_clock`]; `clock` may be NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_timer_init_with_info(
    timer: *mut rcl_timer_t,
    clock: *const rcl_clock_t,
    period_ns: u64,
    callback: rclc_timer_callback_with_info_t,
    context: *mut c_void,
    autostart: bool,
) -> rcl_ret_t {
    unsafe {
        timer_init_inner(
            timer,
            clock,
            period_ns,
            TimerCallback::WithInfo(callback),
            context,
            autostart,
        )
    }
}

/// Which of the two callback flavours an init verb supplied.
enum TimerCallback {
    Plain(rclc_timer_callback_with_context_t),
    WithInfo(rclc_timer_callback_with_info_t),
}

/// Initialize a timer scheduled against `clock` — rcl's shape (phase-430 W1).
///
/// `rcl_timer_init(timer, clock, context, period, callback, allocator)` takes
/// its `rcl_clock_t *` immediately after the timer, and so does this: the
/// clock is the second parameter. That is RFC-0089's "C
/// takes rcl's spellings" applied to the ARGUMENT LIST, which is the half of
/// the spelling that survives our different callback contract —
/// `<rclc/detail/rcl_compat.h>` §4 records why the NAME `rcl_timer_init` stays
/// refused (rcl hands the callback the time since its last call, we hand it
/// the user's context, so a ported timer callback is a different FUNCTION and
/// taking the name would be a compile-and-differ).
///
/// The clock's TYPE selects the schedule, through the one mapping
/// [`crate::clock::nros_timer_clock_source`]:
///
/// * `RCL_STEADY_TIME` — identical to [`rclc_timer_init_with_context`].
/// * `RCL_ROS_TIME` — follows the ROS clock. It stops while a
///   simulator is paused, halves with a bag replayed at 0.5x, and restarts its
///   period on a backwards jump. With no `/clock` override installed it reads
///   system time, the same fallback `rclcpp::Clock` has, so a node written for
///   simulation still runs standalone.
/// * `RCL_SYSTEM_TIME` — the wall clock, NTP steps and all.
///
/// An UNINITIALIZED clock is REJECTED rather than treated as steady: a timer
/// created on a clock the caller never initialised would silently be a wall
/// timer, which is the confusion this verb exists to remove.
///
/// The clock must outlive the timer. Its type is read again by
/// `rclc_executor_add_timer`, which is where the schedule is chosen; rcl has
/// the same obligation and states it the same way.
///
/// # Parameters
/// * `timer` - Pointer to a zero-initialized timer
/// * `clock` - Pointer to an initialized clock (must NOT be NULL — the
///   clock-less form is [`rclc_timer_init_with_context`])
/// * `period_ns` - Timer period in nanoseconds
/// * `callback` - Callback function to invoke when timer fires
/// * `context` - User context pointer passed to callback (can be NULL)
///
/// # Returns
/// * `RCL_RET_OK` on success
/// * `RCL_RET_INVALID_ARGUMENT` if any required pointer is NULL, the period
///   is 0, or the clock names no schedule (uninitialized clock type)
/// * `RCL_RET_NOT_INIT` if the clock is not initialized
///
/// # Safety
/// * All required pointers must be valid
/// * `callback` must be a valid function pointer
/// * `clock` must remain valid for as long as the timer is registered
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_timer_init_on_clock(
    timer: *mut rcl_timer_t,
    clock: *const rcl_clock_t,
    period_ns: u64,
    callback: rclc_timer_callback_with_context_t,
    context: *mut c_void,
) -> rcl_ret_t {
    if clock.is_null() {
        return RCL_RET_INVALID_ARGUMENT;
    }
    unsafe {
        timer_init_inner(
            timer,
            clock,
            period_ns,
            TimerCallback::Plain(callback),
            context,
            true,
        )
    }
}

/// The one body behind every init verb. `clock` NULL means the wall timer;
/// `autostart == false` leaves the timer CANCELLED (issue 1974).
///
/// # Safety
/// As the callers'.
unsafe fn timer_init_inner(
    timer: *mut rcl_timer_t,
    clock: *const rcl_clock_t,
    period_ns: u64,
    callback: TimerCallback,
    context: *mut c_void,
    autostart: bool,
) -> rcl_ret_t {
    validate_not_null!(timer);

    let (callback, info_callback) = match callback {
        TimerCallback::Plain(cb) => (cb, None),
        TimerCallback::WithInfo(cb) => (None, cb),
    };
    if (callback.is_none() && info_callback.is_none()) || period_ns == 0 {
        return RCL_RET_INVALID_ARGUMENT;
    }

    let timer = &mut *timer;

    validate_state!(
        timer,
        rclc_timer_state_t::RCLC_TIMER_STATE_UNINITIALIZED,
        RCLC_RET_BAD_SEQUENCE
    );

    // The clock is validated HERE as well as at registration, so a caller that
    // hands over an uninitialised clock hears about it at the call that named
    // it rather than two calls later from `rclc_executor_add_timer`.
    if !clock.is_null() {
        let clock_ref = &*clock;
        if clock_ref.state != rclc_clock_state_t::RCLC_CLOCK_STATE_READY {
            return RCL_RET_NOT_INIT;
        }
        if crate::clock::nros_timer_clock_source(clock_ref.r#type as u8).is_none() {
            return RCL_RET_INVALID_ARGUMENT;
        }
    }

    timer.period_ns = period_ns;
    timer.callback = callback;
    timer.info_callback = info_callback;
    timer.context = context;
    timer.clock = clock;
    timer.state = if autostart {
        rclc_timer_state_t::RCLC_TIMER_STATE_RUNNING
    } else {
        rclc_timer_state_t::RCLC_TIMER_STATE_CANCELED
    };

    RCL_RET_OK
}

/// Cancel a timer.
///
/// A canceled timer will not fire, but can be reset to start again.
/// If registered with an executor, forwards to the executor's cancel_timer.
///
/// # Parameters
/// * `timer` - Pointer to an initialized timer
///
/// # Returns
/// * `RCL_RET_OK` on success
/// * `RCL_RET_INVALID_ARGUMENT` if timer is NULL
/// * `RCL_RET_NOT_INIT` if not initialized
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rcl_timer_cancel(timer: *mut rcl_timer_t) -> rcl_ret_t {
    validate_not_null!(timer);

    let timer = &mut *timer;

    match timer.state {
        rclc_timer_state_t::RCLC_TIMER_STATE_RUNNING => {
            // Forward to executor if registered
            if !timer._executor.is_null() && timer.handle_id != usize::MAX {
                let exec = &mut *(timer._executor as *mut crate::executor::CExecutor);
                let _ = exec.cancel_timer(nros_node::HandleId::from_raw(timer.handle_id));
            }

            timer.state = rclc_timer_state_t::RCLC_TIMER_STATE_CANCELED;
            RCL_RET_OK
        }
        rclc_timer_state_t::RCLC_TIMER_STATE_CANCELED => {
            // Already canceled
            RCL_RET_OK
        }
        _ => RCL_RET_NOT_INIT,
    }
}

/// Reset a timer.
///
/// This resets the timer's last call time and starts it running again
/// if it was canceled. If registered with an executor, forwards to the
/// executor's reset_timer.
///
/// # Parameters
/// * `timer` - Pointer to an initialized timer
///
/// # Returns
/// * `RCL_RET_OK` on success
/// * `RCL_RET_INVALID_ARGUMENT` if timer is NULL
/// * `RCL_RET_NOT_INIT` if not initialized
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rcl_timer_reset(timer: *mut rcl_timer_t) -> rcl_ret_t {
    validate_not_null!(timer);

    let timer = &mut *timer;

    match timer.state {
        rclc_timer_state_t::RCLC_TIMER_STATE_RUNNING
        | rclc_timer_state_t::RCLC_TIMER_STATE_CANCELED => {
            // Forward to executor if registered
            if !timer._executor.is_null() && timer.handle_id != usize::MAX {
                let exec = &mut *(timer._executor as *mut crate::executor::CExecutor);
                let _ = exec.reset_timer(nros_node::HandleId::from_raw(timer.handle_id));
            }

            timer.state = rclc_timer_state_t::RCLC_TIMER_STATE_RUNNING;
            RCL_RET_OK
        }
        _ => RCL_RET_NOT_INIT,
    }
}

/// Finalize a timer.
///
/// IDEMPOTENT, per `rcl/timer.h` verbatim: "A timer that is already invalid
/// (zero initialized) or `NULL` will not fail." Both of the codes this used to
/// return on those paths — `RCL_RET_INVALID_ARGUMENT` for NULL,
/// `RCL_RET_NOT_INIT` for a second `fini` — are values upstream promises never
/// to produce, and `RCL_WARN_UNUSED` asks the caller to check, so a ported
/// cleanup reported a shutdown failure that had not happened (phase-417 stage 3,
/// ledger row `c:timer_fini`). Nothing about a fixed arena forbids accepting the
/// call: there is no allocation to release and the release is already idempotent.
///
/// # Parameters
/// * `timer` - Pointer to a timer, or NULL
///
/// # Returns
/// * `RCL_RET_OK` — always. A NULL timer and an already-finalised one are
///   successes with nothing to do, not errors.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rcl_timer_fini(timer: *mut rcl_timer_t) -> rcl_ret_t {
    if timer.is_null() {
        return RCL_RET_OK;
    }

    let timer = &mut *timer;

    if timer.state == rclc_timer_state_t::RCLC_TIMER_STATE_UNINITIALIZED
        || timer.state == rclc_timer_state_t::RCLC_TIMER_STATE_SHUTDOWN
    {
        return RCL_RET_OK;
    }

    timer.callback = None;
    timer.context = ptr::null_mut();
    timer.handle_id = usize::MAX;
    timer._executor = ptr::null_mut();
    timer.state = rclc_timer_state_t::RCLC_TIMER_STATE_SHUTDOWN;

    RCL_RET_OK
}

// `rcl_timer_is_ready` and `nros_timer_call` were previously exposed
// as public C symbols for users who wanted to drive timers manually.
// The executor arena now owns timer readiness evaluation and callback
// dispatch end-to-end (see `packages/api/nros-c/src/executor.rs`'s
// timer handling), so those entry points never fired in normal flow
// and duplicated logic that the arena was already doing. Both
// functions are removed from the public C ABI as of Phase 84.B5.

/// Check if timer is valid (initialized and not shutdown).
///
/// # Parameters
/// * `timer` - Pointer to a timer
///
/// # Returns
/// * `true` if valid, `false` if invalid or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_timer_is_valid(timer: *const rcl_timer_t) -> bool {
    if timer.is_null() {
        return false;
    }

    let timer = &*timer;
    matches!(
        timer.state,
        rclc_timer_state_t::RCLC_TIMER_STATE_RUNNING
            | rclc_timer_state_t::RCLC_TIMER_STATE_CANCELED
    )
}

/// Get the timer period in nanoseconds.
///
/// # Parameters
/// * `timer` - Pointer to a timer
///
/// # Returns
/// * Period in nanoseconds, or 0 if invalid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_timer_get_period(timer: *const rcl_timer_t) -> u64 {
    if timer.is_null() {
        return 0;
    }

    let timer = &*timer;
    timer.period_ns
}

/// Nanoseconds until this timer next fires — NEGATIVE if it is overdue.
///
/// rcl's `rcl_timer_get_time_until_next_call(timer, int64_t *)`, marked
/// `RCL_WARN_UNUSED`. phase-417 stage 3 (ledger row
/// `c:timer_get_time_until_next_call`) and issue 1049 are the same defect seen
/// from two sides, and the old shape — `uint64_t f(timer, uint64_t now)` — lost
/// three things at once:
///
/// * the ERROR CHANNEL. The doc comment said the result was "0 if ready now or
///   invalid", so ready-now, NULL, uninitialised, finalised and unregistered
///   were one value and a caller could not tell a due timer from a dead one.
/// * OVERDUE. rcl's header: "a negative value indicates the timer call is
///   overdue by that amount". Unsigned cannot express it, so lateness read as
///   `0` — which is also what "fires now" reads as.
/// * the SOURCE OF TRUTH. It computed from `rcl_timer_t::last_call_time_ns`
///   (removed by phase-487 W3), which `rclc_timer_init_with_context` and `rcl_timer_reset`
///   wrote and no dispatch path ever updated, so for any timer the executor was actually running the
///   answer was permanently `0` (issue 1049). It forwards to the arena now, the
///   same way W5.c's [`rcl_timer_get_time_since_last_call`] and
///   [`rcl_timer_is_ready`] do, and the caller-supplied "now" is gone with it —
///   a second time base is what let the two drift apart.
///
/// **Divergence from rcl, inherited from the sibling accessors:** the arena's
/// timer accounting is MICROSECOND-based (issue #505), so this nanosecond value
/// is a microsecond quantity scaled by 1000. The unit is rcl's; the resolution
/// is ours.
///
/// `RCL_RET_TIMER_INVALID` and `RCL_RET_TIMER_CANCELED` have no counterpart in
/// `rcl_ret_t` and `<rclc/detail/rcl_compat.h>` deliberately does not map them, so
/// the four rcl outcomes reach a caller as three: those two and "not
/// registered" all arrive as `RCL_RET_NOT_INIT`.
///
/// # Returns
/// * `RCL_RET_OK` with `*time_until_next_call_ns` written
/// * `RCL_RET_INVALID_ARGUMENT` if either pointer is NULL
/// * `RCL_RET_NOT_INIT` if the timer is uninitialised, finalised, or not
///   registered with an executor — nothing is advancing its clock, so it has no
///   time until its next call rather than a time of zero
///
/// # Safety
/// * `timer` must be NULL or point to a valid `rcl_timer_t`.
/// * `time_until_next_call_ns` must be NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rcl_timer_get_time_until_next_call(
    timer: *const rcl_timer_t,
    time_until_next_call_ns: *mut i64,
) -> rcl_ret_t {
    validate_not_null!(timer, time_until_next_call_ns);
    match time_until_next_call_ns_of(&*timer) {
        Ok(remaining_ns) => {
            *time_until_next_call_ns = remaining_ns;
            RCL_RET_OK
        }
        Err(ret) => ret,
    }
}

/// The "how long until this timer next fires" answer, computed ONCE.
///
/// Two entry points report it — [`rcl_timer_get_time_until_next_call`]
/// relative and [`rcl_timer_get_next_call_time`] as an absolute point on the
/// timer's clock — and rcl's own pair are `next_call_time` and
/// `next_call_time - now`, so they cannot disagree upstream either. Deriving
/// it twice here is the shape CLAUDE.md's "fix the CLASS" rule names: a later
/// change to the readiness accounting would have to find both sites.
///
/// `Err` carries the `rcl_ret_t` the caller returns unchanged.
///
/// # Safety
/// `timer` must point to a valid `rcl_timer_t`.
unsafe fn time_until_next_call_ns_of(timer: &rcl_timer_t) -> Result<i64, rcl_ret_t> {
    match timer.state {
        rclc_timer_state_t::RCLC_TIMER_STATE_RUNNING
        | rclc_timer_state_t::RCLC_TIMER_STATE_CANCELED => {}
        _ => return Err(RCL_RET_NOT_INIT),
    }

    let Some((exec, id)) = registered_handle(timer) else {
        return Err(RCL_RET_NOT_INIT);
    };
    // phase-417 G6 — `period - elapsed`, SIGNED and saturating, is
    // `Executor::timer_time_until_next_call_ns` now. It was a `const fn` here
    // until `cpp:Timer::time_until_trigger` needed the same subtraction from
    // `nros-cpp`: a second crate deriving it would have been a second place
    // for the sign convention and the microsecond scaling to drift, which is
    // the class CLAUDE.md's "ONE shared helper, never a second spelling" rule
    // names. The signedness assertions moved with it.
    exec.timer_time_until_next_call_ns(id)
        .ok_or(RCL_RET_NOT_INIT)
}

// ============================================================================
// phase-417 W5.c — the timer accessors filed as `gap`
//
// All three are FORWARDERS onto the executor arena's `TimerEntry`, reached
// through the `(handle_id, _executor)` pair `rclc_executor_add_timer`
// installs — the same pair `rcl_timer_cancel` and `rcl_timer_reset` have
// always used. That indirection is why these were gaps: a registered timer's
// live state is the arena's, not this struct's, and this struct's copy is
// stale by construction (nothing wrote `last_call_time_ns` after
// registration, and phase-487 W3 removed it). Reading the arena is a forward to the source of truth, not
// a second one (RFC-0002 / RFC-0019).
//
// Shape: `rcl_ret_t` + an out-param, per rcl's own
// `rcl_timer_is_ready(timer, bool *)`. A bare `bool`/`uint64_t` return would
// have to spend a legal value on "cannot answer" — an unregistered timer has
// no elapsed time, and reporting that as `0` is the defect issue 1008 was.
//
// phase-417 stage 3 made it FOUR: `rcl_timer_get_time_until_next_call` (issue
// 1049, defined above beside `rclc_timer_get_period` where its old bare-value
// form lived) is the same forward, for the same reason, and it reached this
// shape last because it was the one that already had a name and a plausible
// return type.
// ============================================================================

/// Resolve a timer to its executor + arena handle, or `None` when it is not
/// registered with one.
///
/// # Safety
/// `timer` must be non-NULL and point to a valid `rcl_timer_t`.
unsafe fn registered_handle(
    timer: &rcl_timer_t,
) -> Option<(crate::executor::CExecRef<'static>, nros_node::HandleId)> {
    if timer._executor.is_null() || timer.handle_id == usize::MAX {
        return None;
    }
    let exec = unsafe { crate::executor::get_executor_from_ptr(timer._executor) };
    Some((exec, nros_node::HandleId::from_raw(timer.handle_id)))
}

/// Has this timer been cancelled?
///
/// rcl's `rcl_timer_is_canceled(timer, bool *)`. Gap `c:timer_is_canceled`
/// records that our C timer had no cancel predicate under any spelling while
/// C++ and Rust both did.
///
/// Forwards to `Executor::timer_is_canceled` for a registered timer — the
/// arena flag `arena::timer_try_process` actually consults, and the one
/// `rcl_timer_cancel` sets through `Executor::cancel_timer`. A timer that
/// has been initialised but not yet added to an executor has no arena entry,
/// so its own `rclc_timer_state_t` is the whole truth and is read instead.
///
/// # Returns
/// * `RCL_RET_OK` with `*is_canceled` written
/// * `RCL_RET_INVALID_ARGUMENT` if either pointer is NULL
/// * `RCL_RET_NOT_INIT` if the timer is uninitialised or finalised
///
/// # Safety
/// * `timer` must be NULL or point to a valid `rcl_timer_t`.
/// * `is_canceled` must be NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rcl_timer_is_canceled(
    timer: *const rcl_timer_t,
    is_canceled: *mut bool,
) -> rcl_ret_t {
    validate_not_null!(timer, is_canceled);
    let timer_ref = &*timer;

    match timer_ref.state {
        rclc_timer_state_t::RCLC_TIMER_STATE_RUNNING
        | rclc_timer_state_t::RCLC_TIMER_STATE_CANCELED => {}
        _ => return RCL_RET_NOT_INIT,
    }

    *is_canceled = match registered_handle(timer_ref) {
        Some((exec, id)) => exec.timer_is_canceled(id),
        None => timer_ref.state == rclc_timer_state_t::RCLC_TIMER_STATE_CANCELED,
    };
    RCL_RET_OK
}

/// Would this timer fire on the next `nros_executor_spin_*` pass?
///
/// rcl's `rcl_timer_is_ready(timer, bool *)`. Gap `c:timer_is_ready` records
/// that Rust had the predicate and C did not.
///
/// Forwards to `Executor::timer_is_ready`, which evaluates
/// `arena::timer_try_process`'s own guard against the arena entry — cancelled
/// is never ready, a fired one-shot is never ready again, otherwise
/// `elapsed >= period`. Deliberately not re-derived here: a readiness answer
/// that can disagree with the dispatcher is worse than no answer.
///
/// # Returns
/// * `RCL_RET_OK` with `*is_ready` written
/// * `RCL_RET_INVALID_ARGUMENT` if either pointer is NULL
/// * `RCL_RET_NOT_INIT` if the timer is uninitialised, finalised, or not
///   registered with an executor — an unregistered timer is dispatched by
///   nobody, so it has no readiness rather than a readiness of `false`
///
/// # Safety
/// * `timer` must be NULL or point to a valid `rcl_timer_t`.
/// * `is_ready` must be NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rcl_timer_is_ready(
    timer: *const rcl_timer_t,
    is_ready: *mut bool,
) -> rcl_ret_t {
    validate_not_null!(timer, is_ready);
    let timer_ref = &*timer;

    match timer_ref.state {
        rclc_timer_state_t::RCLC_TIMER_STATE_RUNNING
        | rclc_timer_state_t::RCLC_TIMER_STATE_CANCELED => {}
        _ => return RCL_RET_NOT_INIT,
    }

    let Some((exec, id)) = registered_handle(timer_ref) else {
        return RCL_RET_NOT_INIT;
    };
    match exec.timer_is_ready(id) {
        Some(ready) => {
            *is_ready = ready;
            RCL_RET_OK
        }
        None => RCL_RET_NOT_INIT,
    }
}

/// Nanoseconds accumulated since this timer last fired.
///
/// rcl's `rcl_timer_get_time_since_last_call(timer, int64_t *)`. Gap
/// `c:timer_get_time_since_last_call` records that Rust had it and C did not.
///
/// Forwards to `Executor::timer_elapsed_us`, the arena's `elapsed_us`
/// counter. **This is why the accessor could not read `rcl_timer_t` and had
/// to reach the executor:** `rcl_timer_t::last_call_time_ns` was written by
/// `rclc_timer_init_with_context` and `rcl_timer_reset` and by nothing else — no
/// dispatch path updated it — so a struct-local computation answered `0` for
/// every timer that had ever fired. phase-487 W3 removed the field.
///
/// **Divergence from rcl, inherited:** the executor's timer accounting is
/// MICROSECOND-based (issue #505), so the nanosecond value this reports is a
/// microsecond quantity scaled by 1000, not a nanosecond measurement. The
/// unit and the `int64_t` out-type are rcl's; the resolution is ours.
///
/// # Returns
/// * `RCL_RET_OK` with `*time_since_last_call_ns` written
/// * `RCL_RET_INVALID_ARGUMENT` if either pointer is NULL
/// * `RCL_RET_NOT_INIT` if the timer is uninitialised, finalised, or not
///   registered with an executor — nothing has been advancing its clock, so
///   there is no elapsed time rather than an elapsed time of zero
///
/// # Safety
/// * `timer` must be NULL or point to a valid `rcl_timer_t`.
/// * `time_since_last_call_ns` must be NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rcl_timer_get_time_since_last_call(
    timer: *const rcl_timer_t,
    time_since_last_call_ns: *mut i64,
) -> rcl_ret_t {
    validate_not_null!(timer, time_since_last_call_ns);
    let timer_ref = &*timer;

    match timer_ref.state {
        rclc_timer_state_t::RCLC_TIMER_STATE_RUNNING
        | rclc_timer_state_t::RCLC_TIMER_STATE_CANCELED => {}
        _ => return RCL_RET_NOT_INIT,
    }

    let Some((exec, id)) = registered_handle(timer_ref) else {
        return RCL_RET_NOT_INIT;
    };
    match exec.timer_elapsed_us(id) {
        Some(elapsed_us) => {
            // rcl's `rcl_time_point_value_t` (phase-492 W1.b); elapsed time is
            // never negative, so the cast only narrows the range.
            *time_since_last_call_ns =
                i64::try_from(elapsed_us.saturating_mul(1_000)).unwrap_or(i64::MAX);
            RCL_RET_OK
        }
        None => RCL_RET_NOT_INIT,
    }
}

// ============================================================================
// phase-417 W5.c (2026-09-13) — the last two timer rows the ledger filed `gap`
//
// `c:timer_exchange_period` and `c:timer_get_next_call_time` are the same two
// forwarders one more time: the live period and the live elapsed count belong
// to the arena's `TimerHeader`, not to `rcl_timer_t`, so both reach it through
// the `(handle_id, _executor)` pair `rclc_executor_add_timer` installs.
//
// Both take rcl's SPELLING and rcl's SIGNATURE exactly, because both are
// expressible here without changing a type: rcl's `int64_t` nanoseconds are
// ours, and the `(const timer *, out)` shape is the one the four siblings above
// already have.
// ============================================================================

/// Swap this timer's period, reporting the one it had. **Nanoseconds.**
///
/// rcl's `rcl_timer_exchange_period(const rcl_timer_t *, int64_t new_period,
/// int64_t *old_period)`, at rcl's arity and in rcl's order. Gap
/// `c:timer_exchange_period` recorded that a node whose rate is a parameter had
/// to destroy and recreate its timer, because neither C nor Rust could change
/// one in place.
///
/// Writes BOTH copies of the period, which is the whole reason this is not a
/// one-line setter:
///
/// * the arena's `TimerHeader::period_us`, when the timer is registered — the
///   number `arena::timer_try_process` compares `elapsed_us` against, so this
///   is the period the dispatcher will honour on its next pass;
/// * `rcl_timer_t::period_ns` always — the registration INPUT
///   `rclc_executor_add_timer` re-reads, and what `rclc_timer_get_period`
///   returns. Leaving it stale would make the getter disagree with the
///   dispatcher, which is the "compiles and differs" RFC-0089 exists to stop.
///
/// **`old_period` is the exact nanosecond value last supplied**, not the
/// microsecond-quantised one the arena holds: the struct's copy is written
/// unquantised by `rclc_timer_init_with_context` and by this function, so a caller reading
/// back what it wrote gets what it wrote. The DISPATCHER's resolution is still
/// microseconds (issue #505), so a period below 1 µs schedules as 0 — the same
/// quantisation `rcl_timer_init` has always applied, surfaced here rather than
/// introduced.
///
/// **The elapsed count is NOT rewound**, which is rcl's behaviour: upstream
/// exchanges the period and touches nothing else. A timer shortened below its
/// accumulated elapsed time is ready immediately; one lengthened past it waits
/// longer. Call `rcl_timer_reset` after if the new period should start fresh.
///
/// ## Why `const` on a function that writes
///
/// rcl's signature is `const rcl_timer_t *` and it mutates through
/// `timer->impl`, so a ported call site passes a handle it holds by value and
/// expects the write. Ours has no `impl` indirection, so the write is through a
/// cast. That is sound for every reachable timer: `rcl_timer_init` takes
/// `struct rcl_timer_t *`, so no timer that was ever initialised is a
/// `const`-defined object. Taking a non-`const` parameter instead would have
/// been a silent divergence for exactly the ported callers that hold a
/// `const rcl_timer_t *`, and C reports that as a warning.
///
/// # Returns
/// * `RCL_RET_OK` with `*old_period` written and the new period in force
/// * `RCL_RET_INVALID_ARGUMENT` if either pointer is NULL, or `new_period` is
///   not positive — `rcl_timer_init` refuses a zero period, and a setter that
///   accepted one would leave the timer in a state its own constructor forbids
/// * `RCL_RET_NOT_INIT` if the timer is uninitialised or finalised, or is
///   registered with an executor that no longer knows the handle
///
/// # Safety
/// * `timer` must be NULL or point to a valid, non-`const`-defined
///   `rcl_timer_t`.
/// * `old_period` must be NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rcl_timer_exchange_period(
    timer: *const rcl_timer_t,
    new_period: i64,
    old_period: *mut i64,
) -> rcl_ret_t {
    validate_not_null!(timer, old_period);
    if new_period <= 0 {
        return RCL_RET_INVALID_ARGUMENT;
    }
    let timer_ref = &*timer;

    match timer_ref.state {
        rclc_timer_state_t::RCLC_TIMER_STATE_RUNNING
        | rclc_timer_state_t::RCLC_TIMER_STATE_CANCELED => {}
        _ => return RCL_RET_NOT_INIT,
    }

    let new_period_ns = new_period as u64;
    if let Some((mut exec, id)) = registered_handle(timer_ref) {
        // A registered timer whose handle the executor no longer recognises is
        // the `NOT_INIT` the sibling accessors report; reporting OK here would
        // claim a dispatcher change that did not happen.
        if exec
            .exchange_timer_period_us(id, new_period_ns / 1_000)
            .is_none()
        {
            return RCL_RET_NOT_INIT;
        }
    }

    let previous_ns = timer_ref.period_ns;
    // SAFETY: see "Why `const` on a function that writes" above — the pointee
    // was declared non-`const` by whichever call to `rcl_timer_init` brought
    // this timer out of `UNINITIALIZED`, which the state check above proved
    // happened.
    (*(timer as *mut rcl_timer_t)).period_ns = new_period_ns;
    *old_period = previous_ns.min(i64::MAX as u64) as i64;
    RCL_RET_OK
}

/// When this timer next fires, as a point on the clock it is scheduled against.
///
/// rcl's `rcl_timer_get_next_call_time(const rcl_timer_t *, int64_t *)`. Gap
/// `c:timer_get_next_call_time` recorded C as lacking it; the row's `why` said
/// "Rust has it (`Timer::time_until_next_call`)", which named the RELATIVE
/// sibling — upstream ships both, and `rcl_timer_get_time_until_next_call`
/// (phase-417 stage 3) is the relative one. This is the absolute one, and it is
/// `now + remaining` over the SAME derivation, so the pair cannot disagree.
///
/// ## Which clock "now" is read from
///
/// The timer's own, which is the property that makes the answer comparable
/// with anything else the caller timestamps:
///
/// * a timer created by `rclc_timer_init_on_clock` carries an
///   `rcl_clock_t *`, and that clock is read — so a `RCL_ROS_TIME`
///   timer answers on `/clock`'s timeline and stops advancing when the
///   playback does, exactly as its dispatch does;
/// * a timer created by `rcl_timer_init` is the WALL timer (rclcpp's
///   `create_wall_timer`) and carries no clock handle. Its answer is on the
///   platform STEADY clock — the same monotonic source whose spin deltas
///   advance the arena's `elapsed_us`, so it is the timeline the timer is
///   actually scheduled on rather than a wall-clock approximation of it.
///
/// **Divergence from rcl, inherited from the sibling accessors:** the arena's
/// timer accounting is MICROSECOND-based (issue #505), so the remaining half of
/// this sum has microsecond resolution. The clock read does not.
///
/// `RCL_RET_TIMER_INVALID` and `RCL_RET_TIMER_CANCELED` have no counterpart in
/// `rcl_ret_t` (`<rclc/detail/rcl_compat.h>` declines to map them), so those and "not
/// registered with an executor" all arrive as `RCL_RET_NOT_INIT`.
///
/// # Returns
/// * `RCL_RET_OK` with `*next_call_time` written
/// * `RCL_RET_INVALID_ARGUMENT` if either pointer is NULL
/// * `RCL_RET_NOT_INIT` if the timer is uninitialised, finalised, or not
///   registered with an executor — nothing is advancing its clock, so it has no
///   next call rather than a next call of zero
/// * whatever `rclc_clock_get_now_ns` reports for a timer whose attached clock
///   is not readable
///
/// # Safety
/// * `timer` must be NULL or point to a valid `rcl_timer_t`.
/// * `next_call_time` must be NULL or writable.
/// * A non-NULL `timer->clock` must still point to a live `rcl_clock_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rcl_timer_get_next_call_time(
    timer: *const rcl_timer_t,
    next_call_time: *mut i64,
) -> rcl_ret_t {
    validate_not_null!(timer, next_call_time);
    let timer_ref = &*timer;

    let remaining_ns = match time_until_next_call_ns_of(timer_ref) {
        Ok(remaining_ns) => remaining_ns,
        Err(ret) => return ret,
    };

    let now_ns = if timer_ref.clock.is_null() {
        crate::clock::get_steady_time_ns().min(i64::MAX as u64) as i64
    } else {
        let mut now_ns: i64 = 0;
        let ret = crate::clock::rcl_clock_get_now(timer_ref.clock, &mut now_ns);
        if ret != RCL_RET_OK {
            return ret;
        }
        now_ns
    };

    *next_call_time = now_ns.saturating_add(remaining_ns);
    RCL_RET_OK
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::{
        rcl_duration_t, rclc_duration_from_nanoseconds, rclc_time_from_nanoseconds, rclc_time_t,
    };

    /// A timer that has been `rcl_timer_init`'d but never added to an
    /// executor. Its arena entry does not exist, so nothing is advancing its
    /// clock.
    fn unregistered_running_timer() -> rcl_timer_t {
        rcl_timer_t {
            state: rclc_timer_state_t::RCLC_TIMER_STATE_RUNNING,
            period_ns: 1_000_000,
            ..rcl_timer_t::default()
        }
    }

    /// phase-417 W5.c — "no executor" is reported, not answered as zero.
    ///
    /// This is the whole reason the accessor forwards to the arena instead of
    /// reading `rcl_timer_t`: the struct's `last_call_time_ns` (removed by
    /// phase-487 W3) was written by `init`/`reset` and by nothing else, so a
    /// struct-local computation returned `0` for every timer — including this one, which has no elapsed
    /// time at all rather than an elapsed time of zero (issue 1008's shape).
    #[test]
    fn elapsed_on_an_unregistered_timer_is_unanswerable_not_zero() {
        let timer = unregistered_running_timer();
        let mut out: i64 = 0xdead_beef;
        assert_eq!(
            unsafe { rcl_timer_get_time_since_last_call(&timer, &mut out) },
            RCL_RET_NOT_INIT
        );
        assert_eq!(
            out, 0xdead_beef,
            "a failed read must not write the out-param"
        );

        let mut ready = true;
        assert_eq!(
            unsafe { rcl_timer_is_ready(&timer, &mut ready) },
            RCL_RET_NOT_INIT
        );
        assert!(ready, "a failed read must not write the out-param");
    }

    /// Cancellation is the one predicate an unregistered timer CAN answer:
    /// with no arena entry, its own `rclc_timer_state_t` is the whole truth,
    /// and `rcl_timer_cancel` maintains it on this path.
    #[test]
    fn cancel_state_is_readable_without_an_executor() {
        let mut timer = unregistered_running_timer();
        let mut canceled = true;
        assert_eq!(
            unsafe { rcl_timer_is_canceled(&timer, &mut canceled) },
            RCL_RET_OK
        );
        assert!(!canceled);

        assert_eq!(unsafe { rcl_timer_cancel(&mut timer) }, RCL_RET_OK);
        assert_eq!(
            unsafe { rcl_timer_is_canceled(&timer, &mut canceled) },
            RCL_RET_OK
        );
        assert!(canceled);
    }

    /// A finalised or never-initialised timer has no state to report under
    /// any of the three accessors.
    #[test]
    fn an_uninitialised_timer_answers_nothing() {
        let timer = rcl_timer_t::default();
        let mut b = false;
        let mut n: i64 = 0;
        assert_eq!(
            unsafe { rcl_timer_is_canceled(&timer, &mut b) },
            RCL_RET_NOT_INIT
        );
        assert_eq!(
            unsafe { rcl_timer_is_ready(&timer, &mut b) },
            RCL_RET_NOT_INIT
        );
        assert_eq!(
            unsafe { rcl_timer_get_time_since_last_call(&timer, &mut n) },
            RCL_RET_NOT_INIT
        );
    }

    /// phase-417 stage 3, ledger row `c:timer_get_time_until_next_call`, and
    /// issue 1049.
    ///
    /// Upstream's `rcl_timer_get_time_until_next_call` is `rcl_ret_t` + an
    /// `int64_t *` out-param, and it distinguishes OK from the ways it cannot
    /// answer. Returning the value as a bare `uint64_t` spent `0` on five
    /// cases at once — ready now, NULL, uninitialised, finalised, unregistered.
    #[test]
    fn time_until_next_call_reports_rather_than_answering_zero() {
        let timer = unregistered_running_timer();
        let mut out: i64 = 0xdead_beef;

        assert_eq!(
            unsafe { rcl_timer_get_time_until_next_call(&timer, &mut out) },
            RCL_RET_NOT_INIT,
            "nothing is advancing an unregistered timer's clock"
        );
        assert_eq!(
            out, 0xdead_beef,
            "a failed read must not write the out-param"
        );

        assert_eq!(
            unsafe { rcl_timer_get_time_until_next_call(core::ptr::null(), &mut out) },
            RCL_RET_INVALID_ARGUMENT
        );
        assert_eq!(
            unsafe { rcl_timer_get_time_until_next_call(&timer, core::ptr::null_mut()) },
            RCL_RET_INVALID_ARGUMENT
        );

        let uninitialised = rcl_timer_t::default();
        assert_eq!(
            unsafe { rcl_timer_get_time_until_next_call(&uninitialised, &mut out) },
            RCL_RET_NOT_INIT
        );
    }

    /// phase-417 stage 3, ledger row `c:timer_fini`.
    ///
    /// `rcl/timer.h`, verbatim: "A timer that is already invalid (zero
    /// initialized) or `NULL` will not fail." `RCL_WARN_UNUSED` asks the caller
    /// to check the return, so a cleanup that does report a failure at shutdown
    /// that upstream defines as success.
    #[test]
    fn fini_is_idempotent_and_accepts_null() {
        assert_eq!(
            unsafe { rcl_timer_fini(core::ptr::null_mut()) },
            RCL_RET_OK,
            "upstream: a NULL timer will not fail"
        );

        let mut zeroed = rcl_timer_t::default();
        assert_eq!(
            unsafe { rcl_timer_fini(&mut zeroed) },
            RCL_RET_OK,
            "upstream: an already-invalid (zero initialized) timer will not fail"
        );

        let mut timer = unregistered_running_timer();
        assert_eq!(unsafe { rcl_timer_fini(&mut timer) }, RCL_RET_OK);
        assert_eq!(
            timer.state,
            rclc_timer_state_t::RCLC_TIMER_STATE_SHUTDOWN,
            "the first fini must still do the work"
        );
        assert_eq!(
            unsafe { rcl_timer_fini(&mut timer) },
            RCL_RET_OK,
            "a second fini is a no-op, not an error"
        );
        assert_eq!(timer.state, rclc_timer_state_t::RCLC_TIMER_STATE_SHUTDOWN);
    }

    /// phase-417 W5.c, ledger row `c:timer_exchange_period`.
    ///
    /// The row's `why`: "a node whose rate is a parameter has to destroy and
    /// recreate the timer". A parameter callback is the caller this exists for,
    /// so the property that matters is that the exchange is READABLE afterwards
    /// — `rclc_timer_get_period` must report the new value, not the value the
    /// constructor was given. Two sources of truth is the defect this could
    /// have shipped with, since the live period is the ARENA's and the struct
    /// carries a second copy that a later `rclc_executor_add_timer` re-reads.
    ///
    /// The arena half of the same claim — that the DISPATCHER honours the new
    /// period and that `elapsed_us` is not rewound — is asserted in
    /// `nros-node`'s own tests
    /// (`exchanging_a_timers_period_changes_the_cadence_without_rewinding_it`),
    /// where an executor exists. A `nros-c` unit test has none.
    #[test]
    fn exchanging_a_period_is_readable_through_the_getter() {
        let timer = unregistered_running_timer();
        assert_eq!(unsafe { rclc_timer_get_period(&timer) }, 1_000_000);

        let mut old: i64 = 0;
        assert_eq!(
            unsafe { rcl_timer_exchange_period(&timer, 250_000, &mut old) },
            RCL_RET_OK
        );
        assert_eq!(
            old, 1_000_000,
            "the exchange reports the period it replaced"
        );
        assert_eq!(
            unsafe { rclc_timer_get_period(&timer) },
            250_000,
            "the getter must not keep reporting the constructor's value"
        );

        // ...and again, so the second exchange reports the FIRST exchange's
        // value rather than the constructor's.
        assert_eq!(
            unsafe { rcl_timer_exchange_period(&timer, 7_000, &mut old) },
            RCL_RET_OK
        );
        assert_eq!(old, 250_000);
        assert_eq!(unsafe { rclc_timer_get_period(&timer) }, 7_000);
    }

    /// A period the constructor would refuse must not be reachable through the
    /// setter — `rcl_timer_init` rejects `period_ns == 0`, and rcl rejects a
    /// negative one, so accepting either here would leave a timer in a state
    /// its own constructor forbids.
    #[test]
    fn a_period_the_constructor_refuses_is_refused_here_too() {
        let timer = unregistered_running_timer();
        let mut old: i64 = 0xdead_beef;

        for bad in [0_i64, -1, i64::MIN] {
            assert_eq!(
                unsafe { rcl_timer_exchange_period(&timer, bad, &mut old) },
                RCL_RET_INVALID_ARGUMENT,
                "period {bad} must be refused"
            );
        }
        assert_eq!(
            old, 0xdead_beef,
            "a refused exchange must not write the out-param"
        );
        assert_eq!(
            unsafe { rclc_timer_get_period(&timer) },
            1_000_000,
            "a refused exchange must not change the period either"
        );
    }

    /// phase-417 W5.c, ledger row `c:timer_get_next_call_time`.
    ///
    /// Upstream distinguishes "cannot answer" from an answer, and an absolute
    /// time point has no safe sentinel — `0` is a legal point on every clock we
    /// support. So the unanswerable cases must arrive as a status, with the
    /// out-param untouched. Unregistered is one of them for the same reason
    /// `rcl_timer_get_time_until_next_call` reports it: nothing is advancing
    /// the timer's clock, so there is no next call rather than a next call now.
    #[test]
    fn next_call_time_reports_rather_than_answering_zero() {
        let timer = unregistered_running_timer();
        let mut out: i64 = 0xdead_beef;

        assert_eq!(
            unsafe { rcl_timer_get_next_call_time(&timer, &mut out) },
            RCL_RET_NOT_INIT,
            "nothing is advancing an unregistered timer's clock"
        );
        assert_eq!(
            out, 0xdead_beef,
            "a failed read must not write the out-param"
        );

        let uninitialised = rcl_timer_t::default();
        assert_eq!(
            unsafe { rcl_timer_get_next_call_time(&uninitialised, &mut out) },
            RCL_RET_NOT_INIT
        );
        assert_eq!(
            unsafe { rcl_timer_get_next_call_time(core::ptr::null(), &mut out) },
            RCL_RET_INVALID_ARGUMENT
        );
        assert_eq!(
            unsafe { rcl_timer_get_next_call_time(&timer, core::ptr::null_mut()) },
            RCL_RET_INVALID_ARGUMENT
        );
        assert_eq!(out, 0xdead_beef);
    }

    /// NULL in any position is refused, never dereferenced.
    #[test]
    fn null_arguments_are_refused() {
        let timer = unregistered_running_timer();
        let mut b = false;
        let mut n: i64 = 0;
        assert_eq!(
            unsafe { rcl_timer_is_canceled(core::ptr::null(), &mut b) },
            RCL_RET_INVALID_ARGUMENT
        );
        assert_eq!(
            unsafe { rcl_timer_is_canceled(&timer, core::ptr::null_mut()) },
            RCL_RET_INVALID_ARGUMENT
        );
        assert_eq!(
            unsafe { rcl_timer_is_ready(&timer, core::ptr::null_mut()) },
            RCL_RET_INVALID_ARGUMENT
        );
        assert_eq!(
            unsafe { rcl_timer_get_time_since_last_call(core::ptr::null(), &mut n) },
            RCL_RET_INVALID_ARGUMENT
        );
        let mut i: i64 = 0;
        assert_eq!(
            unsafe { rcl_timer_exchange_period(core::ptr::null(), 1_000, &mut i) },
            RCL_RET_INVALID_ARGUMENT
        );
        assert_eq!(
            unsafe { rcl_timer_exchange_period(&timer, 1_000, core::ptr::null_mut()) },
            RCL_RET_INVALID_ARGUMENT
        );
        assert_eq!(
            unsafe { rcl_timer_get_next_call_time(core::ptr::null(), &mut i) },
            RCL_RET_INVALID_ARGUMENT
        );
        assert_eq!(
            unsafe { rcl_timer_get_next_call_time(&timer, core::ptr::null_mut()) },
            RCL_RET_INVALID_ARGUMENT
        );
    }

    // ========================================================================
    // `rcl_difference_times` — the arithmetic behind the `static inline` in
    // <rclc/timer.h>.
    //
    // The function itself is header-only (no state, so no exported symbol),
    // and the C probe `tests/compile/node_timer_accessors.c` runs its BODY.
    // What is asserted here is the pair of claims the header makes about that
    // body, which are properties of the two entry points it composes and are
    // therefore testable in Rust: the subtraction cannot overflow, and the
    // re-encode saturates rather than wrapping.
    // ========================================================================

    /// The composition the header's `static inline` performs, verbatim.
    fn difference_times(start: rclc_time_t, finish: rclc_time_t) -> rcl_duration_t {
        let start_ns = unsafe { crate::clock::rclc_time_to_nanoseconds(&start) };
        let finish_ns = unsafe { crate::clock::rclc_time_to_nanoseconds(&finish) };
        rclc_duration_from_nanoseconds(finish_ns - start_ns)
    }

    #[test]
    fn difference_is_finish_minus_start() {
        let start = rclc_time_from_nanoseconds(1_000_000_000);
        let finish = rclc_time_from_nanoseconds(2_500_000_000);
        let d = difference_times(start, finish);
        assert_eq!((d.sec, d.nanosec), (1, 500_000_000));
    }

    /// Negative spans are ordinary, and `builtin_interfaces` encodes them by
    /// FLOOR division with a non-negative remainder: -1.5 s is
    /// `{sec: -2, nanosec: 5e8}`. Truncation would produce `{0, 5e8}`, which
    /// decodes as +0.5 s — the sign flip issue 0799 fixed one layer down, and
    /// the reason this inline delegates to `rclc_duration_from_nanoseconds`
    /// instead of splitting the value itself.
    #[test]
    fn a_finish_before_start_is_negative_and_floors() {
        let start = rclc_time_from_nanoseconds(2_500_000_000);
        let finish = rclc_time_from_nanoseconds(1_000_000_000);
        let d = difference_times(start, finish);
        assert_eq!((d.sec, d.nanosec), (-2, 500_000_000));

        let zero = difference_times(start, start);
        assert_eq!((zero.sec, zero.nanosec), (0, 0));
    }

    /// The header claims the SUBTRACTION cannot overflow, and the claim rests
    /// on `rclc_time_t`'s `int32_t sec`: a decoded time point is bounded by
    /// ~±2.15e18 ns, so any difference is bounded by ~±4.3e18 — inside
    /// `i64`'s ±9.22e18. Asserted at the extremes with a CHECKED subtraction,
    /// so a widening of the time encoding fails here rather than becoming
    /// undefined behaviour in a header nobody re-reads. (rcl's own
    /// `rcl_difference_times` has no such bound: its time point is a bare
    /// `int64_t`, and it subtracts with plain signed arithmetic.)
    #[test]
    fn the_subtraction_cannot_overflow_at_the_encodings_extremes() {
        let max = rclc_time_t {
            sec: i32::MAX,
            nanosec: 999_999_999,
        };
        let min = rclc_time_t {
            sec: i32::MIN,
            nanosec: 0,
        };
        let max_ns = unsafe { crate::clock::rclc_time_to_nanoseconds(&max) };
        let min_ns = unsafe { crate::clock::rclc_time_to_nanoseconds(&min) };

        assert!(max_ns.checked_sub(min_ns).is_some());
        assert!(min_ns.checked_sub(max_ns).is_some());
        // And the bound itself, so a widened `sec` type trips this test with
        // an arithmetic message rather than silently.
        assert!(max_ns.saturating_sub(min_ns) < i64::MAX / 2);
    }

    /// What CAN exceed its type is the re-encoded result: a span of ±4.3e9
    /// seconds does not fit `rcl_duration_t`'s `int32_t sec`.
    /// `rclc_duration_from_nanoseconds` saturates, keeping the encoding
    /// monotone — it does not wrap, which would flip the sign of a span.
    #[test]
    fn an_unrepresentable_span_saturates_rather_than_wrapping() {
        let max = rclc_time_t {
            sec: i32::MAX,
            nanosec: 999_999_999,
        };
        let min = rclc_time_t {
            sec: i32::MIN,
            nanosec: 0,
        };

        let forward = difference_times(min, max);
        assert_eq!(forward.sec, i32::MAX, "must clamp, not wrap negative");
        let backward = difference_times(max, min);
        assert_eq!(backward.sec, i32::MIN, "must clamp, not wrap positive");
    }
}

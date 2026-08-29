//! Callback arena infrastructure (all pub(crate)).

use core::marker::PhantomData;

use nros_core::{
    BorrowedMessage, CdrReader, DeserializeBorrowed, MessageInfo, RawMessageInfo, RosAction,
    RosMessage, RosService,
};
use nros_rmw::{ServiceTrait, Subscription, TransportError};

use super::{
    action_core::{ActionClientCore, ActionServerCore},
    handles::{ActionServer, ActiveGoal},
    spsc_ring::SpscRing,
    triple_buffer::TripleBuffer,
    types::{
        InvocationMode, NodeError, RawAcceptedCallback, RawCancelCallback, RawFeedbackCallback,
        RawGoalCallback, RawGoalResponseCallback, RawResponseCallback, RawResultCallback,
        RawServiceCallback, RawSubscriptionCallback, RawSubscriptionInfoCallback,
    },
};
use crate::session;

// ============================================================================
// Callback metadata
// ============================================================================

/// Kind of registered callback entry.
#[derive(Clone, Copy)]
pub(crate) enum EntryKind {
    Subscription,
    Service,
    ServiceClient,
    Timer,
    ActionServer,
    ActionClient,
    GuardCondition,
}

/// Metadata for a type-erased callback stored in the arena.
///
/// Each entry records where the concrete entry struct lives in the arena
/// and carries monomorphized function pointers for dispatch and cleanup.
#[derive(Clone, Copy)]
pub(crate) struct CallbackMeta {
    /// Byte offset into the arena where the concrete entry starts.
    pub(crate) offset: usize,
    /// What kind of entry this is (for `SpinOnceResult` counters).
    pub(crate) kind: EntryKind,
    /// Monomorphized dispatch: tries to receive and process one message/request.
    /// Returns `Ok(true)` if work was done, `Ok(false)` if nothing available.
    /// The `u64` parameter is `delta_us` (used by timer entries, ignored by others).
    pub(crate) try_process: unsafe fn(*mut u8, u64) -> Result<bool, TransportError>,
    /// Monomorphized readiness check: returns true if the entry has data.
    pub(crate) has_data: unsafe fn(*const u8) -> bool,
    /// Monomorphized LET pre-sample: reads data from transport into the entry's
    /// buffer without invoking the callback. No-op for non-subscription entries.
    pub(crate) pre_sample: unsafe fn(*mut u8),
    /// Per-callback invocation mode.
    pub(crate) invocation: InvocationMode,
    /// Monomorphized drop: runs destructors on the concrete entry.
    pub(crate) drop_fn: unsafe fn(*mut u8),
}

// ============================================================================
// Concrete entry types
// ============================================================================

/// Concrete subscription entry stored in the arena (with MessageInfo).
#[repr(C)]
pub(crate) struct SubInfoEntry<M, F, const RX_BUF: usize> {
    pub(crate) handle: session::RmwSubscriber,
    pub(crate) buffer: [u8; RX_BUF],
    /// Length of pre-sampled LET data (0 = not sampled).
    pub(crate) sampled_len: usize,
    pub(crate) callback: F,
    pub(crate) _phantom: PhantomData<M>,
}

/// Concrete subscription entry stored in the arena (with safety validation).
#[cfg(feature = "safety-e2e")]
#[repr(C)]
pub(crate) struct SubSafetyEntry<M, F, const RX_BUF: usize> {
    pub(crate) handle: session::RmwSubscriber,
    pub(crate) buffer: [u8; RX_BUF],
    /// Length of pre-sampled LET data (0 = not sampled).
    pub(crate) sampled_len: usize,
    pub(crate) callback: F,
    pub(crate) _phantom: PhantomData<M>,
}

/// Concrete service entry stored in the arena.
#[repr(C)]
pub(crate) struct SrvEntry<Svc: RosService, F, const REQ_BUF: usize, const REPLY_BUF: usize> {
    pub(crate) handle: session::RmwServiceServer,
    pub(crate) req_buffer: [u8; REQ_BUF],
    pub(crate) reply_buffer: [u8; REPLY_BUF],
    pub(crate) callback: F,
    pub(crate) _phantom: PhantomData<Svc>,
}

/// What a periodic timer does with periods it missed while its executor
/// was blocked (issue #505).
///
/// A tier that loses the CPU for several periods — a long-running
/// callback, or preemption by a higher-priority band — resumes with
/// accumulated elapsed time worth more than one period.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimerOverrunPolicy {
    /// Coalesce the backlog: fire ONCE and drop the missed periods,
    /// counting them in `overruns`. Phase is preserved (the sub-period
    /// remainder carries over), so activations stay aligned to the
    /// original cadence grid instead of drifting by each stall.
    ///
    /// The default, matching rclcpp (which advances the next call time
    /// past "now") and Zephyr's `k_timer` (which coalesces expiries and
    /// reports a count).
    #[default]
    Skip,
    /// Replay every missed period, one activation per `try_process`
    /// pass, until the backlog drains. Correct for timers that count or
    /// accumulate, where each activation is a unit of work that must
    /// not be lost. A replay burst runs back-to-back at dispatch speed,
    /// NOT at the declared cadence — control loops usually want `Skip`.
    CatchUp,
}

/// Concrete timer entry stored in the arena.
///
/// The first fields (up to `callback`) share layout with [`TimerHeader`],
/// enabling type-erased access to timer state (cancel, reset, period query).
#[repr(C)]
pub(crate) struct TimerEntry<F> {
    pub(crate) period_us: u64,
    pub(crate) elapsed_us: u64,
    /// Periods dropped by [`TimerOverrunPolicy::Skip`] (issue #505).
    /// Saturating; never cleared by the dispatcher.
    pub(crate) overruns: u32,
    /// `overruns` as of the last `timer-overrun-runtime` monitor check,
    /// so the rule reports newly dropped activations rather than the
    /// running total. Lives here rather than in a table parallel to
    /// `entries`, whose capacity is a runtime slice length.
    pub(crate) overruns_reported: u32,
    pub(crate) oneshot: bool,
    pub(crate) fired: bool,
    pub(crate) cancelled: bool,
    pub(crate) overrun_policy: TimerOverrunPolicy,
    pub(crate) callback: F,
}

/// Type-erased header for timer entries.
///
/// Shares layout with the initial fields of `TimerEntry<F>` (both `#[repr(C)]`),
/// so a `*mut TimerHeader` can safely read/write the timer state fields
/// regardless of the concrete closure type `F`.
#[repr(C)]
pub(crate) struct TimerHeader {
    pub(crate) period_us: u64,
    pub(crate) elapsed_us: u64,
    pub(crate) overruns: u32,
    pub(crate) overruns_reported: u32,
    pub(crate) oneshot: bool,
    pub(crate) fired: bool,
    pub(crate) cancelled: bool,
    pub(crate) overrun_policy: TimerOverrunPolicy,
}

/// Concrete action server entry stored in the arena.
#[repr(C)]
pub(crate) struct ActionServerArenaEntry<
    A: RosAction,
    GoalF,
    CancelF,
    const GOAL_BUF: usize,
    const RESULT_BUF: usize,
    const FEEDBACK_BUF: usize,
    const MAX_GOALS: usize,
> {
    pub(crate) server: ActionServer<A, GOAL_BUF, RESULT_BUF, FEEDBACK_BUF, MAX_GOALS>,
    pub(crate) goal_callback: GoalF,
    pub(crate) cancel_callback: CancelF,
}

/// Concrete action server entry for raw (untyped) callbacks.
///
/// Uses [`ActionServerCore`] directly (no typed `ActionServer<A>` wrapper).
#[repr(C)]
pub(crate) struct ActionServerRawArenaEntry<
    const GOAL_BUF: usize,
    const RESULT_BUF: usize,
    const FEEDBACK_BUF: usize,
    const MAX_GOALS: usize,
> {
    pub(crate) core: ActionServerCore<GOAL_BUF, RESULT_BUF, FEEDBACK_BUF, MAX_GOALS>,
    pub(crate) goal_callback: RawGoalCallback,
    pub(crate) cancel_callback: RawCancelCallback,
    /// Optional hook fired after the accept reply has been sent. Used by the
    /// C API so user-supplied long-running `accepted_callback`s run *after*
    /// the client has observed the accept instead of blocking the reply.
    pub(crate) accepted_callback: Option<RawAcceptedCallback>,
    pub(crate) context: *mut core::ffi::c_void,
}

/// Concrete action client entry for raw (untyped) async callbacks.
///
/// Contains the `ActionClientCore` plus callback function pointers for
/// goal response, feedback, and result. The executor polls the core's
/// non-blocking methods during `spin_once` and invokes the callbacks.
#[repr(C)]
pub(crate) struct ActionClientRawArenaEntry<
    const GOAL_BUF: usize,
    const RESULT_BUF: usize,
    const FEEDBACK_BUF: usize,
> {
    pub(crate) core: ActionClientCore<GOAL_BUF, RESULT_BUF, FEEDBACK_BUF>,
    pub(crate) goal_response_callback: Option<RawGoalResponseCallback>,
    pub(crate) feedback_callback: Option<RawFeedbackCallback>,
    pub(crate) result_callback: Option<RawResultCallback>,
    pub(crate) context: *mut core::ffi::c_void,
}

/// Concrete service-client entry for raw (untyped) async polling.
///
/// Holds the `RmwServiceClient` plus a single-shot reply buffer and a
/// callback fn pointer. The executor dispatches via
/// `service_client_raw_try_process` which checks `reply_ready` (set by
/// the transport waker) before calling `try_recv_reply_raw`. This
/// avoids busy-polling `get_check` on every spin tick.
///
/// Single in-flight request per entry: a second `send_request` while
/// `pending` is still `true` is the user's responsibility to avoid (the
/// C wrapper checks at the call site).
#[repr(C)]
pub struct ServiceClientRawArenaEntry<const REPLY_BUF: usize> {
    pub handle: session::RmwServiceClient,
    pub reply_buffer: [u8; REPLY_BUF],
    pub pending: bool,
    /// Set by the transport waker when a reply arrives for this slot.
    /// Checked by `try_process` to avoid blind polling.
    pub reply_ready: core::sync::atomic::AtomicBool,
    pub callback: Option<RawResponseCallback>,
    pub context: *mut core::ffi::c_void,
}

/// Concrete service entry for raw (untyped) callbacks.
#[repr(C)]
pub(crate) struct SrvRawEntry<const REQ_BUF: usize, const REPLY_BUF: usize> {
    pub(crate) handle: session::RmwServiceServer,
    pub(crate) req_buffer: [u8; REQ_BUF],
    pub(crate) reply_buffer: [u8; REPLY_BUF],
    pub(crate) callback: RawServiceCallback,
    pub(crate) context: *mut core::ffi::c_void,
}

/// Concrete guard condition entry stored in the arena.
#[repr(C)]
pub(crate) struct GuardConditionEntry<F> {
    pub(crate) flag: portable_atomic::AtomicBool,
    pub(crate) callback: F,
}

// ============================================================================
// QoS-driven buffered subscription entries (Phase 73)
// ============================================================================

/// Buffer strategy selected by QoS depth at subscription registration time.
///
/// The buffer data lives in a trailing region immediately after the
/// `SubBufferedEntry` struct in the arena.
pub(crate) enum BufferStrategy {
    /// `KEEP_LAST(1)`: 3 slots, latest-value semantics, writer never blocks.
    Triple(TripleBuffer),
    /// `KEEP_LAST(N)` where N > 1: N+1 slots, FIFO ordering, bounded drops.
    Ring(SpscRing),
}

impl BufferStrategy {
    /// Check if new data is available.
    pub(crate) fn has_data(&self) -> bool {
        match self {
            BufferStrategy::Triple(tb) => tb.has_data(),
            BufferStrategy::Ring(ring) => ring.has_data(),
        }
    }
}

/// Compute the number of buffer slots and trailing region size for a given
/// QoS depth and per-slot buffer size.
///
/// Returns `(slot_count, trailing_bytes)`.
pub(crate) fn buffered_region_size(depth: u32, slot_size: usize) -> (usize, usize) {
    if depth <= 1 {
        // Triple buffer: 3 fixed slots
        (
            TripleBuffer::SLOT_COUNT,
            TripleBuffer::SLOT_COUNT * slot_size,
        )
    } else {
        let d = depth as usize;
        (SpscRing::slot_count(d), SpscRing::region_size(d, slot_size))
    }
}

/// Subscription entry with QoS-driven buffer strategy (Phase 73).
///
/// Unlike the legacy single-buffer pattern, this entry
/// stores a [`BufferStrategy`] that manages a trailing buffer region
/// allocated from the arena at registration time.
///
/// # Arena layout
///
/// ```text
/// [SubBufferedEntry<M, F> struct][trailing: slot_count × slot_size bytes]
///  ↑ offset                      ↑ buffer managed by BufferStrategy
/// ```
/// W3b.5 — a contracted subscriber's age hook: the endpoint's cell plus
/// the epoch clock, captured at registration. `None` = uncontracted (or
/// no epoch source / no stamp in the type) — the take path costs one
/// `Option` branch.
pub(crate) type AgeMon = (
    &'static crate::executor::monitor::SubMonitorCell,
    fn() -> u64,
);

/// W3b.5 — peek the message stamp from the raw CDR buffer and record its
/// take-age. Compiled away per-type when `M::STAMP_OFFSET` is `None`.
#[inline]
pub(crate) fn observe_age<M: RosMessage>(raw: &[u8], mon: &Option<AgeMon>) {
    if let (Some((cell, epoch)), Some(off)) = (mon, M::STAMP_OFFSET)
        && let Some(stamp_us) = crate::executor::monitor::peek_stamp_us(raw, off)
    {
        cell.observe(stamp_us, epoch());
    }
}

#[repr(C)]
pub(crate) struct SubBufferedEntry<M, F> {
    pub(crate) handle: session::RmwSubscriber,
    pub(crate) buffer: BufferStrategy,
    pub(crate) callback: F,
    /// W3b.5 — age hook for contracted endpoints.
    pub(crate) age_mon: Option<AgeMon>,
    pub(crate) _phantom: PhantomData<M>,
}

/// Drain the RMW subscriber handle into the buffer strategy.
///
/// Calls `try_recv_raw()` on the subscriber handle and writes received data
/// into the triple buffer's write slot or the SPSC ring's next push slot.
///
/// # Safety
/// `entry` must be a valid mutable reference to a `SubBufferedEntry`.
/// Issue 0757 — how many takes this process has dropped, for throttling.
///
/// A COUNTER, not a per-entry field, and deliberately: the arena's entry
/// structs are sized by knob at build time (`EXECUTOR_OPAQUE_U64S`), so adding
/// a field here would move every image's executor footprint to buy a log line.
/// The cost is that the report cannot name the topic — see `report_dropped_take`.
static DROPPED_TAKES: portable_atomic::AtomicU32 = portable_atomic::AtomicU32::new(0);

/// issue 0900 — has the arena-headroom advisory been emitted?
///
/// A STATIC, for the reason [`DROPPED_TAKES`] is one: an `Executor` field would
/// move `EXECUTOR_OPAQUE_U64S` and therefore every image's executor footprint,
/// to buy a diagnostic. Process-scoped rather than per-executor is also the
/// RIGHT scope here, not merely the cheap one — the number this names
/// (`NROS_EXECUTOR_ARENA_SIZE`) is a BUILD-TIME constant, identical for every
/// executor in the image, so saying it twice adds nothing.
static ARENA_ADVISORY_DONE: portable_atomic::AtomicBool = portable_atomic::AtomicBool::new(false);

/// Report an arena that is far larger than the entities registered in it.
///
/// `ARENA_SIZE` is derived by budgeting EVERY slot at the ActionClient worst
/// case (`nros-node/build.rs`), so an image with no action client carries
/// several times what it can use — 74,240 bytes against ~16 KiB for a
/// pub/sub-only workload at the defaults. The arena is a BUMP allocator, so
/// `arena_used` is the exact claimed total rather than a reservation, and the
/// allocator has therefore always known the right answer and never said it.
///
/// The override exists (`NROS_EXECUTOR_ARENA_SIZE`, or
/// `CONFIG_NROS_EXECUTOR_ARENA_SIZE` on Zephyr) and is already load-bearing:
/// the FreeRTOS action examples pin it to 8192 and still need a 64 KB app task
/// stack, a number someone found by hitting "Invalid mbox" and working
/// backwards. That is the shape of issues 0271/0739 — a knob nobody can
/// enumerate is a knob nobody sets — and this turns it from folklore into a
/// measurement.
///
/// **The arena is on the STACK**, not in `.bss`: `Executor` holds
/// `arena: [MaybeUninit<u8>; ARENA_SIZE]` inline, so `mem-report` cannot see it
/// and the cost lands on whichever task calls spin
/// (`docs/reference/platform-implementation-notes.md`, FreeRTOS pitfalls).
///
/// `nros_log`, never stdio: this is reached on `no_std` targets and inside
/// Zephyr `native_sim`, where a Rust `std` stdio call is FATAL (issue 0589).
#[cold]
fn report_arena_headroom(used: usize, capacity: usize) {
    // Round the suggestion up so a small later registration still fits, and
    // keep it a multiple of 1024 because that is the unit the knob is written
    // in everywhere else in the tree.
    let suggest = used.next_multiple_of(1024).max(1024);
    // BUDGET: `nros_log`'s call-site format buffer is 256 bytes by default
    // (`buffer-size-256`), and overflow truncates with a `…` rather than
    // dropping the record. The first draft of this line ran ~450 bytes and was
    // cut mid-number, so the sink received "executor arena is 74240 bytes and
    // 32…" — every word of the explanation and NONE of the value to set. A
    // diagnostic that explains itself past the budget delivers exactly the
    // folklore it was written to replace. So: the actionable value FIRST, the
    // reasoning in this comment and issue 0900, and a test that fails on `…`.
    nros_log::nros_info!(
        nros_log::get_logger("nros"),
        "arena over-provisioned: set NROS_EXECUTOR_ARENA_SIZE={suggest}          (Zephyr: CONFIG_ prefix). {used}/{capacity} bytes claimed at first          spin, and the arena is INLINE ON THE TASK STACK. Later registrations          need more. issue 0900"
    );
}

/// One-shot arena-headroom check, called on the first `spin_once`.
///
/// First spin, not registration end, because there is no "registration end" —
/// an app may register lazily, and this is why the message says
/// "at first spin" and names what it measured rather than asserting a total.
///
/// Hot-path cost is one relaxed load on a branch that is taken exactly once.
pub(crate) fn maybe_report_arena_headroom(used: usize, capacity: usize) {
    if ARENA_ADVISORY_DONE.load(portable_atomic::Ordering::Relaxed) {
        return;
    }
    // Only ever set, never cleared, so a racing second spin at worst emits the
    // line twice — cheaper than an AcqRel on every spin to prevent a duplicate
    // advisory.
    ARENA_ADVISORY_DONE.store(true, portable_atomic::Ordering::Relaxed);
    if arena_is_over_provisioned(used, capacity) {
        report_arena_headroom(used, capacity);
    }
}

/// Is this arena grossly larger than what registered in it?
///
/// Half is the threshold because the derivation's own error is a factor of
/// ~4.5 at the defaults (74,240 bytes budgeted against ~16 KiB for a
/// pub/sub-only image), so "under half" is unambiguous rather than a rounding
/// artifact, and an image that genuinely uses most of its arena stays quiet.
///
/// A zero capacity is NOT over-provisioned: that is the sentinel bug issue 0460
/// produced on Zephyr (a literal `0` forwarded instead of the derivation), and
/// it fails loudly on the first registration. Calling it over-provisioned would
/// bury a fatal misconfiguration under an advisory about wasted space.
///
/// Separated from [`maybe_report_arena_headroom`] so it is testable: the
/// reporter is one-shot on a process-scoped flag, so a test that called it
/// twice would silently check nothing the second time.
pub(crate) const fn arena_is_over_provisioned(used: usize, capacity: usize) -> bool {
    capacity != 0 && used.saturating_mul(2) <= capacity
}

#[cfg(test)]
mod arena_headroom_tests {
    use super::arena_is_over_provisioned;

    #[test]
    fn gross_over_provision_is_reported() {
        // The measured shape: a talker's handful of entries against the
        // worst-case-derived 74,240.
        assert!(arena_is_over_provisioned(4_096, 74_240));
        assert!(arena_is_over_provisioned(0, 74_240));
    }

    #[test]
    fn a_well_sized_arena_stays_quiet() {
        assert!(!arena_is_over_provisioned(60_000, 74_240));
        // Exactly half is the boundary and IS reported; one byte more is not.
        assert!(arena_is_over_provisioned(37_120, 74_240));
        assert!(!arena_is_over_provisioned(37_121, 74_240));
    }

    #[test]
    fn a_zero_capacity_arena_is_a_fault_not_headroom() {
        // Issue 0460's sentinel bug. It fails on the first registration; an
        // advisory about wasted space would bury that.
        assert!(!arena_is_over_provisioned(0, 0));
    }

    #[test]
    fn overflow_cannot_panic_the_check() {
        // `used` is bounded by `capacity` in practice, but the doubling must
        // not be the thing that decides that.
        assert!(!arena_is_over_provisioned(usize::MAX, 74_240));
    }
}

/// Say that a take was thrown away, on the first one and every 64th after.
///
/// Issue 0757, RFC-0052 fail-loud. `try_recv_raw` returns `BufferTooSmall` when
/// a reassembled sample exceeds the subscription buffer, and this path used to
/// discard EVERY non-OK take. At transport level cyclone has already completed
/// and ACKed the sample by then, so the subscription looks matched and healthy
/// from every outside probe (`ros2 topic info -v`, tshark ACKNACK analysis)
/// while the application waits forever. That is how 13.4 KiB Autoware
/// trajectories were silently dropped by every Zephyr image for the whole life
/// of the lane: small degenerate samples fit the 1 KiB default, so every green
/// marker stayed green, and attribution needed a consumer-side tshark session.
///
/// **What this can and cannot say.** The buffer capacity is known here and is
/// the actionable half — it names the knob to raise
/// (`NROS_SUBSCRIPTION_BUFFER_SIZE`, or `ZPICO_SUBSCRIBER_BUFFER_SIZE` /
/// `ZPICO_SUBSCRIBER_LARGE_SIZE` on zenoh). The SAMPLE size is not: the C ABI
/// contract is "non-negative = bytes produced, negative = error code"
/// (`rmw_vtable.h`), with no required-length out-param, so the backend cannot
/// report how big the sample was. The topic is not either: `SubBufferedEntry`
/// carries no name and adding one changes arena sizing. Both are ABI/struct
/// changes worth doing on their own merits, not smuggled in behind a log line.
///
/// `nros_log`, never stdio: this site is reached on `no_std` targets and inside
/// Zephyr `native_sim`, where a Rust `std` stdio call is FATAL (issue 0589).
#[cold]
fn report_dropped_take(err: &TransportError, buf_len: usize) {
    let n = DROPPED_TAKES.fetch_add(1, portable_atomic::Ordering::Relaxed);
    // First, then every 64th. A 40-participant graph must not turn one
    // misconfigured subscription into a log flood (issue 0371's shape).
    if n != 0 && !n.is_multiple_of(64) {
        return;
    }
    nros_log::nros_error!(
        nros_log::get_logger("nros"),
        "subscription take DROPPED ({err:?}); buffer is {buf_len} bytes. The \
         sample was received and ACKed, then discarded — raise the subscription \
         buffer knob if this is BufferTooSmall. Dropped {} so far (issue 0757)",
        n + 1
    );
}

unsafe fn drain_into_buffer<M, F>(
    entry: &mut SubBufferedEntry<M, F>,
) -> Result<(), TransportError> {
    match &entry.buffer {
        BufferStrategy::Triple(tb) => {
            let slot = tb.write_slot();
            let cap = slot.len();
            if let Some(len) = entry.handle.try_recv_raw(slot).inspect_err(|e| {
                report_dropped_take(e, cap);
            })? {
                tb.writer_publish(len);
            }
        }
        BufferStrategy::Ring(ring) => {
            while let Some(slot) = ring.try_push() {
                let cap = slot.len();
                match entry.handle.try_recv_raw(slot).inspect_err(|e| {
                    report_dropped_take(e, cap);
                })? {
                    Some(len) => ring.commit_push(len),
                    None => break,
                }
            }
        }
    }
    Ok(())
}

/// Monomorphized dispatch for buffered subscriptions.
///
/// First drains the RMW subscriber into the buffer strategy (triple buffer
/// or SPSC ring), then dispatches from the buffer to the user callback.
///
/// # Safety
/// `ptr` must point to a valid, aligned `SubBufferedEntry<M, F>`.
pub(crate) unsafe fn sub_buffered_try_process<M, F>(
    ptr: *mut u8,
    _delta_us: u64,
) -> Result<bool, TransportError>
where
    M: RosMessage,
    F: FnMut(&M),
{
    let entry = unsafe { &mut *(ptr as *mut SubBufferedEntry<M, F>) };

    // Phase 1: drain RMW subscriber → buffer strategy
    // Issue 0757 — let a transport error OUT, exactly as issue 0737 does for
    // the C copy below. Anything already buffered from an earlier spin is still
    // dispatched on the next call.
    unsafe { drain_into_buffer(entry)? };

    // Phase 2: dispatch from buffer → user callback
    match &entry.buffer {
        BufferStrategy::Triple(tb) => match tb.reader_acquire() {
            Some((data, len)) => {
                observe_age::<M>(&data[..len], &entry.age_mon);
                let mut reader = CdrReader::new_with_header(&data[..len])
                    .map_err(|_| TransportError::DeserializationError)?;
                let msg = M::deserialize(&mut reader)
                    .map_err(|_| TransportError::DeserializationError)?;
                (entry.callback)(&msg);
                Ok(true)
            }
            None => Ok(false),
        },
        BufferStrategy::Ring(ring) => {
            let mut did_work = false;
            while let Some((data, len)) = ring.try_pop() {
                observe_age::<M>(&data[..len], &entry.age_mon);
                let mut reader = CdrReader::new_with_header(&data[..len])
                    .map_err(|_| TransportError::DeserializationError)?;
                let msg = M::deserialize(&mut reader)
                    .map_err(|_| TransportError::DeserializationError)?;
                (entry.callback)(&msg);
                ring.commit_pop();
                did_work = true;
            }
            Ok(did_work)
        }
    }
}

/// Readiness check for buffered subscriptions.
///
/// Checks the RMW subscriber handle first (new data available from transport),
/// then the buffer strategy (data already drained into triple buffer/ring).
///
/// # Safety
/// `ptr` must point to a valid `SubBufferedEntry<M, F>`.
pub(crate) unsafe fn sub_buffered_has_data<M, F>(ptr: *const u8) -> bool {
    let entry = unsafe { &*(ptr as *const SubBufferedEntry<M, F>) };
    // Check RMW handle first (data may be in static buffer, not yet drained)
    entry.handle.has_data() || entry.buffer.has_data()
}

// ============================================================================
// In-place typed subscription (Phase 231 Wave 0.2 — RFC-0038)
// ============================================================================

/// In-place typed subscription entry — **no arena buffer**.
///
/// Unlike [`SubBufferedEntry`], this carries no trailing `BufferStrategy`: the
/// callback deserializes directly from the backend's borrowed receive slot via
/// [`Subscriber::process_raw_in_place`], so copy #1 (ring → arena) and the arena
/// buffer are both gone. Selected at registration when the backend advertises
/// `supports_process_in_place()`.
#[repr(C)]
pub(crate) struct SubInplaceEntry<M, F> {
    pub(crate) handle: session::RmwSubscriber,
    pub(crate) callback: F,
    /// W3b.5 — age hook for contracted endpoints.
    pub(crate) age_mon: Option<AgeMon>,
    pub(crate) _phantom: PhantomData<M>,
}

/// Monomorphized in-place dispatch for typed subscriptions.
///
/// Drains all pending messages from the backend, deserializing + invoking the
/// callback directly from each borrowed slot. Returns `Ok(true)` if any message
/// was dispatched.
///
/// # Safety
/// `ptr` must point to a valid, aligned `SubInplaceEntry<M, F>`.
pub(crate) unsafe fn sub_inplace_try_process<M, F>(
    ptr: *mut u8,
    _delta_us: u64,
) -> Result<bool, TransportError>
where
    M: RosMessage,
    F: FnMut(&M),
{
    let entry = unsafe { &mut *(ptr as *mut SubInplaceEntry<M, F>) };
    // Split-borrow the handle and callback (disjoint fields).
    let SubInplaceEntry {
        handle,
        callback,
        age_mon,
        ..
    } = entry;
    let mut did_work = false;
    loop {
        let mut deser_err = false;
        let processed = handle.process_raw_in_place(|raw| {
            observe_age::<M>(raw, age_mon);
            match CdrReader::new_with_header(raw) {
                Ok(mut reader) => match M::deserialize(&mut reader) {
                    Ok(msg) => (callback)(&msg),
                    Err(_) => deser_err = true,
                },
                Err(_) => deser_err = true,
            }
        })?;
        if deser_err {
            return Err(TransportError::DeserializationError);
        }
        if processed {
            did_work = true;
        } else {
            break;
        }
    }
    Ok(did_work)
}

/// Readiness check for in-place typed subscriptions.
///
/// # Safety
/// `ptr` must point to a valid `SubInplaceEntry<M, F>`.
pub(crate) unsafe fn sub_inplace_has_data<M, F>(ptr: *const u8) -> bool {
    let entry = unsafe { &*(ptr as *const SubInplaceEntry<M, F>) };
    entry.handle.has_data()
}

// ============================================================================
// Zero-copy raw buffered subscription (Phase 73.10)
// ============================================================================

/// Buffered subscription entry for zero-copy raw callbacks.
///
/// The callback receives `&[u8]` (CDR data) borrowing directly from the
/// triple buffer's read slot or SPSC ring's pop slot. For borrowed message
/// types (e.g., `Image<'a>`), the callback calls `deserialize_borrowed()`
/// on the data, giving the message a lifetime tied to the callback scope.
#[repr(C)]
pub(crate) struct SubBufferedRawEntry<F> {
    pub(crate) handle: session::RmwSubscriber,
    pub(crate) buffer: BufferStrategy,
    pub(crate) callback: F,
}

/// Drain helper for raw buffered entries.
///
/// Issue 0757 — the THIRD copy of this drain, and it had the same swallow as
/// the typed one above. Fixed the way issue 0737 fixed the C copy
/// (`drain_into_buffer_raw_c`): let the error OUT so `spin_once` counts it,
/// rather than inventing a second remedy for one defect.
unsafe fn drain_into_buffer_raw<F>(
    entry: &mut SubBufferedRawEntry<F>,
) -> Result<(), TransportError> {
    match &entry.buffer {
        BufferStrategy::Triple(tb) => {
            let slot = tb.write_slot();
            let cap = slot.len();
            if let Some(len) = entry.handle.try_recv_raw(slot).inspect_err(|e| {
                report_dropped_take(e, cap);
            })? {
                tb.writer_publish(len);
            }
        }
        BufferStrategy::Ring(ring) => {
            while let Some(slot) = ring.try_push() {
                let cap = slot.len();
                match entry.handle.try_recv_raw(slot).inspect_err(|e| {
                    report_dropped_take(e, cap);
                })? {
                    Some(len) => ring.commit_push(len),
                    None => break,
                }
            }
        }
    }
    Ok(())
}

/// Dispatch for zero-copy raw buffered subscriptions.
///
/// Drains the RMW handle into the buffer, then passes the raw CDR slice
/// to the callback. The callback borrows from the buffer slot — no copy.
///
/// # Safety
/// `ptr` must point to a valid, aligned `SubBufferedRawEntry<F>`.
pub(crate) unsafe fn sub_buffered_raw_try_process<F>(
    ptr: *mut u8,
    _delta_us: u64,
) -> Result<bool, TransportError>
where
    F: FnMut(&[u8]),
{
    let entry = unsafe { &mut *(ptr as *mut SubBufferedRawEntry<F>) };

    // Issue 0757 — see `drain_into_buffer`; the error reaches `spin_once`'s
    // `subscription_errors` instead of vanishing.
    unsafe { drain_into_buffer_raw(entry)? };

    match &entry.buffer {
        BufferStrategy::Triple(tb) => match tb.reader_acquire() {
            Some((data, len)) => {
                (entry.callback)(&data[..len]);
                Ok(true)
            }
            None => Ok(false),
        },
        BufferStrategy::Ring(ring) => {
            let mut did_work = false;
            while let Some((data, len)) = ring.try_pop() {
                (entry.callback)(&data[..len]);
                ring.commit_pop();
                did_work = true;
            }
            Ok(did_work)
        }
    }
}

/// Readiness check for raw buffered subscriptions.
///
/// # Safety
/// `ptr` must point to a valid `SubBufferedRawEntry<F>`.
pub(crate) unsafe fn sub_buffered_raw_has_data<F>(ptr: *const u8) -> bool {
    let entry = unsafe { &*(ptr as *const SubBufferedRawEntry<F>) };
    entry.handle.has_data() || entry.buffer.has_data()
}

// ============================================================================
// Borrowed (zero-copy) buffered subscription (Phase 229.6, issue 0007)
// ============================================================================

/// Buffered subscription entry for borrowed (zero-copy) message callbacks.
///
/// The callback receives `&B::View<'a>` — a lifetime-carrying message whose
/// unbounded sequence/string fields borrow directly from the triple buffer's
/// read slot (no arena copy, no `heapless::Vec` copy). The view is materialised
/// per dispatch via [`DeserializeBorrowed`] and dropped before the slot is
/// released, so the borrow never outlives the buffer.
///
/// **Triple-buffer only.** A borrowed view must reference exactly one
/// well-defined slot for the duration of the callback; an SPSC ring (depth > 1)
/// holds several samples in flight with no single such slot. Registration
/// rejects `qos.depth > 1` for borrowed subscriptions, so `buffer` is always
/// [`BufferStrategy::Triple`] here.
#[repr(C)]
pub(crate) struct SubBufferedBorrowedEntry<B, F> {
    pub(crate) handle: session::RmwSubscriber,
    pub(crate) buffer: BufferStrategy,
    pub(crate) callback: F,
    pub(crate) _phantom: PhantomData<B>,
}

/// Dispatch for borrowed (zero-copy) buffered subscriptions.
///
/// Drains the RMW handle into the triple buffer, then materialises a borrowed
/// `B::View<'_>` over the read slot and hands it to the callback. The view
/// borrows the slot; it is dropped at the end of the callback, before the next
/// dispatch can publish over the slot.
///
/// # Safety
/// `ptr` must point to a valid, aligned `SubBufferedBorrowedEntry<B, F>`.
pub(crate) unsafe fn sub_buffered_borrowed_try_process<B, F>(
    ptr: *mut u8,
    _delta_us: u64,
) -> Result<bool, TransportError>
where
    B: BorrowedMessage,
    F: for<'a> FnMut(&B::View<'a>),
{
    let entry = unsafe { &mut *(ptr as *mut SubBufferedBorrowedEntry<B, F>) };

    // Borrowed subscriptions are triple-buffer only (enforced at registration).
    let tb = match &entry.buffer {
        BufferStrategy::Triple(tb) => tb,
        // Unreachable: registration rejects depth > 1. Treat as no work.
        BufferStrategy::Ring(_) => return Ok(false),
    };

    // Phase 1: drain RMW subscriber → triple buffer write slot.
    //
    // Issue 0757 — the FOURTH copy of this drain (the borrowed/zero-copy path)
    // and it swallowed non-OK takes like the others. Same remedy as issue 0737's
    // C copy: report the actionable size, then let the error out.
    {
        let slot = tb.write_slot();
        let cap = slot.len();
        if let Some(len) = entry.handle.try_recv_raw(slot).inspect_err(|e| {
            report_dropped_take(e, cap);
        })? {
            tb.writer_publish(len);
        }
    }

    // Phase 2: borrow the read slot and deserialize a view over it (no copy).
    match tb.reader_acquire() {
        Some((data, len)) => {
            let mut reader = CdrReader::new_with_header(&data[..len])
                .map_err(|_| TransportError::DeserializationError)?;
            let msg = <B::View<'_> as DeserializeBorrowed>::deserialize_borrowed(&mut reader)
                .map_err(|_| TransportError::DeserializationError)?;
            (entry.callback)(&msg);
            Ok(true)
        }
        None => Ok(false),
    }
}

/// Readiness check for borrowed buffered subscriptions.
///
/// # Safety
/// `ptr` must point to a valid `SubBufferedBorrowedEntry<B, F>`.
pub(crate) unsafe fn sub_buffered_borrowed_has_data<B, F>(ptr: *const u8) -> bool {
    let entry = unsafe { &*(ptr as *const SubBufferedBorrowedEntry<B, F>) };
    entry.handle.has_data() || entry.buffer.has_data()
}

// ============================================================================
// Raw buffered subscription with attachment / MessageInfo (Phase 189.M1)
// ============================================================================

/// Staging cap for a raw subscription's wire attachment (`bridge_origin`
/// tags and similar are small). Attachment bytes longer than this are
/// truncated by the backend's `try_recv_raw_with_attachment`.
pub(crate) const RAW_INFO_ATT_CAP: usize = 256;

/// Raw buffered subscription entry that surfaces the sample's wire
/// attachment as a [`RawMessageInfo`] to the callback
/// (`FnMut(&[u8], &RawMessageInfo)`).
///
/// Unlike [`SubBufferedRawEntry`] (Triple/Ring `BufferStrategy`), this
/// uses a flat inline payload buffer + a flat attachment buffer so the
/// attachment travels with its message — the decoupled producer/consumer
/// slots of a triple/ring buffer cannot carry per-message side data.
/// One sample per dispatch (mirrors [`SubInfoEntry`]).
#[repr(C)]
pub(crate) struct SubBufferedRawInfoEntry<F, const RX_BUF: usize> {
    pub(crate) handle: session::RmwSubscriber,
    pub(crate) buffer: [u8; RX_BUF],
    pub(crate) att: [u8; RAW_INFO_ATT_CAP],
    pub(crate) callback: F,
}

/// Dispatch for raw buffered subscriptions with attachment.
///
/// # Safety
/// `ptr` must point to a valid, aligned `SubBufferedRawInfoEntry<F, RX_BUF>`.
pub(crate) unsafe fn sub_buffered_raw_info_try_process<F, const RX_BUF: usize>(
    ptr: *mut u8,
    _delta_us: u64,
) -> Result<bool, TransportError>
where
    F: FnMut(&[u8], &RawMessageInfo),
{
    let entry = unsafe { &mut *(ptr as *mut SubBufferedRawInfoEntry<F, RX_BUF>) };
    match entry
        .handle
        .try_recv_raw_with_attachment(&mut entry.buffer, &mut entry.att)
    {
        Ok(Some((len, att_len))) => {
            let info = RawMessageInfo::new(&entry.att[..att_len]);
            (entry.callback)(&entry.buffer[..len], &info);
            Ok(true)
        }
        Ok(None) => Ok(false),
        Err(_) => Err(TransportError::DeserializationError),
    }
}

/// Readiness check for raw buffered subscriptions with attachment.
///
/// # Safety
/// `ptr` must point to a valid `SubBufferedRawInfoEntry<F, RX_BUF>`.
pub(crate) unsafe fn sub_buffered_raw_info_has_data<F, const RX_BUF: usize>(
    ptr: *const u8,
) -> bool {
    let entry = unsafe { &*(ptr as *const SubBufferedRawInfoEntry<F, RX_BUF>) };
    entry.handle.has_data()
}

/// C-style (fn-ptr + context) raw buffered subscription with attachment
/// (Phase 189.M3.4 — the C analog of [`SubBufferedRawInfoEntry`]). Flat
/// payload + attachment buffers, one sample per dispatch.
#[repr(C)]
pub(crate) struct SubBufferedRawInfoCEntry<const RX_BUF: usize> {
    pub(crate) handle: session::RmwSubscriber,
    pub(crate) buffer: [u8; RX_BUF],
    pub(crate) att: [u8; RAW_INFO_ATT_CAP],
    pub(crate) callback: RawSubscriptionInfoCallback,
    pub(crate) context: *mut core::ffi::c_void,
}

/// Dispatch for the C-style raw buffered subscription with attachment.
///
/// # Safety
/// `ptr` must point to a valid, aligned `SubBufferedRawInfoCEntry<RX_BUF>`.
pub(crate) unsafe fn sub_buffered_raw_info_c_try_process<const RX_BUF: usize>(
    ptr: *mut u8,
    _delta_us: u64,
) -> Result<bool, TransportError> {
    let entry = unsafe { &mut *(ptr as *mut SubBufferedRawInfoCEntry<RX_BUF>) };
    match entry
        .handle
        .try_recv_raw_with_attachment(&mut entry.buffer, &mut entry.att)
    {
        Ok(Some((len, att_len))) => {
            unsafe {
                (entry.callback)(
                    entry.buffer.as_ptr(),
                    len,
                    entry.att.as_ptr(),
                    att_len,
                    entry.context,
                )
            };
            Ok(true)
        }
        Ok(None) => Ok(false),
        Err(_) => Err(TransportError::DeserializationError),
    }
}

/// Readiness check for the C-style raw buffered subscription with attachment.
///
/// # Safety
/// `ptr` must point to a valid `SubBufferedRawInfoCEntry<RX_BUF>`.
pub(crate) unsafe fn sub_buffered_raw_info_c_has_data<const RX_BUF: usize>(ptr: *const u8) -> bool {
    let entry = unsafe { &*(ptr as *const SubBufferedRawInfoCEntry<RX_BUF>) };
    entry.handle.has_data()
}

/// Phase 250 (Wave 2) — generic (type-erased) raw buffered subscription that
/// surfaces E2E [`IntegrityStatus`](nros_rmw::IntegrityStatus) (CRC + sequence
/// gap/dup) alongside the raw CDR bytes (`FnMut(&[u8], &IntegrityStatus)`).
///
/// The type-erased analog of [`SubSafetyEntry`]: the validator lives in the
/// `RmwSubscriber` and `try_recv_validated` produces the status, so no typed
/// `M` is needed (the declarative `Node` path is generic). Flat inline payload
/// buffer; one sample per dispatch.
#[cfg(feature = "safety-e2e")]
#[repr(C)]
pub(crate) struct SubBufferedRawSafetyEntry<F, const RX_BUF: usize> {
    pub(crate) handle: session::RmwSubscriber,
    pub(crate) buffer: [u8; RX_BUF],
    pub(crate) callback: F,
}

/// Dispatch for the generic raw safety subscription: validate-receive into the
/// buffer, then pass the raw slice + status to the callback.
///
/// # Safety
/// `ptr` must point to a valid, aligned `SubBufferedRawSafetyEntry<F, RX_BUF>`.
#[cfg(feature = "safety-e2e")]
pub(crate) unsafe fn sub_buffered_raw_safety_try_process<F, const RX_BUF: usize>(
    ptr: *mut u8,
    _delta_us: u64,
) -> Result<bool, TransportError>
where
    F: FnMut(&[u8], &nros_rmw::IntegrityStatus),
{
    let entry = unsafe { &mut *(ptr as *mut SubBufferedRawSafetyEntry<F, RX_BUF>) };
    match entry.handle.try_recv_validated(&mut entry.buffer) {
        Ok(Some((len, status))) => {
            (entry.callback)(&entry.buffer[..len], &status);
            Ok(true)
        }
        Ok(None) => Ok(false),
        Err(_) => Err(TransportError::DeserializationError),
    }
}

/// Readiness check for the generic raw safety subscription.
///
/// # Safety
/// `ptr` must point to a valid `SubBufferedRawSafetyEntry<F, RX_BUF>`.
#[cfg(feature = "safety-e2e")]
pub(crate) unsafe fn sub_buffered_raw_safety_has_data<F, const RX_BUF: usize>(
    ptr: *const u8,
) -> bool {
    let entry = unsafe { &*(ptr as *const SubBufferedRawSafetyEntry<F, RX_BUF>) };
    entry.handle.has_data()
}

/// Phase 269 W3 — the C analog of [`SubBufferedRawSafetyEntry`]: same flat inline
/// payload buffer + `try_recv_validated` dispatch, but the callback is a plain
/// C function pointer (`RawSubscriptionSafetyCallback`) that receives the integrity
/// scalars alongside the CDR bytes. No generic type parameter — monomorphism over
/// the `RX_BUF` const only.
#[cfg(feature = "safety-e2e")]
#[repr(C)]
pub(crate) struct SubBufferedRawSafetyCEntry<const RX_BUF: usize> {
    pub(crate) handle: session::RmwSubscriber,
    pub(crate) buffer: [u8; RX_BUF],
    pub(crate) callback: super::types::RawSubscriptionSafetyCallback,
    pub(crate) context: *mut core::ffi::c_void,
}

/// Dispatch for the C-style raw validated subscription: validate-receive into the
/// buffer, then pass the raw slice + unpacked integrity scalars to the callback.
///
/// # Safety
/// `ptr` must point to a valid, aligned `SubBufferedRawSafetyCEntry<RX_BUF>`.
#[cfg(feature = "safety-e2e")]
pub(crate) unsafe fn sub_buffered_raw_safety_c_try_process<const RX_BUF: usize>(
    ptr: *mut u8,
    _delta_us: u64,
) -> Result<bool, TransportError> {
    let entry = unsafe { &mut *(ptr as *mut SubBufferedRawSafetyCEntry<RX_BUF>) };
    match entry.handle.try_recv_validated(&mut entry.buffer) {
        Ok(Some((len, status))) => {
            let crc_valid: i8 = match status.crc_valid {
                Some(true) => 1,
                Some(false) => 0,
                None => -1,
            };
            unsafe {
                (entry.callback)(
                    entry.buffer.as_ptr(),
                    len,
                    status.gap,
                    status.duplicate,
                    crc_valid,
                    entry.context,
                )
            };
            Ok(true)
        }
        Ok(None) => Ok(false),
        Err(_) => Err(TransportError::DeserializationError),
    }
}

/// Readiness check for the C-style raw validated subscription.
///
/// # Safety
/// `ptr` must point to a valid `SubBufferedRawSafetyCEntry<RX_BUF>`.
#[cfg(feature = "safety-e2e")]
pub(crate) unsafe fn sub_buffered_raw_safety_c_has_data<const RX_BUF: usize>(
    ptr: *const u8,
) -> bool {
    let entry = unsafe { &*(ptr as *const SubBufferedRawSafetyCEntry<RX_BUF>) };
    entry.handle.has_data()
}

/// Buffered subscription entry for C-style raw callbacks (function pointer + context).
///
/// Same as `SubBufferedRawEntry` but uses `RawSubscriptionCallback` instead of
/// a Rust closure. Used by the C API and by `register_subscription_raw_*` methods.
#[repr(C)]
pub(crate) struct SubBufferedRawCEntry {
    pub(crate) handle: session::RmwSubscriber,
    pub(crate) buffer: BufferStrategy,
    pub(crate) callback: RawSubscriptionCallback,
    pub(crate) context: *mut core::ffi::c_void,
}

/// Drain helper for C-style raw buffered entries.
/// Issue 0737 — a transport ERROR is not "no data", and conflating them
/// destroys the sample without a trace.
///
/// Both arms used to read `if let Ok(Some(len)) = … else { break }`, which
/// treats `Err(_)` exactly like `Ok(None)`. The backend has ALREADY consumed
/// the sample by the time it reports the error, so the message is gone and the
/// only observable is that nothing arrived — indistinguishable from a publisher
/// that never published. 0737 spent two hosts' investigations inside that
/// ambiguity while the executor's own `alive — … 0 error(s)` line reported
/// health, because the error never reached the counter that prints it.
///
/// Now it propagates: `spin_once` maps an `Err` from `try_process` to
/// `subscription_errors`, so the count stops lying and the failure has a name.
unsafe fn drain_into_buffer_raw_c(entry: &mut SubBufferedRawCEntry) -> Result<(), TransportError> {
    match &entry.buffer {
        BufferStrategy::Triple(tb) => {
            let slot = tb.write_slot();
            if let Some(len) = entry.handle.try_recv_raw(slot)? {
                tb.writer_publish(len);
            }
        }
        BufferStrategy::Ring(ring) => {
            while let Some(slot) = ring.try_push() {
                match entry.handle.try_recv_raw(slot)? {
                    Some(len) => ring.commit_push(len),
                    None => break,
                }
            }
        }
    }
    Ok(())
}

/// Dispatch for C-style raw buffered subscriptions.
///
/// # Safety
/// `ptr` must point to a valid, aligned `SubBufferedRawCEntry`.
pub(crate) unsafe fn sub_buffered_raw_c_try_process(
    ptr: *mut u8,
    _delta_us: u64,
) -> Result<bool, TransportError> {
    let entry = unsafe { &mut *(ptr as *mut SubBufferedRawCEntry) };

    // Issue 0737 — drain first, and let a transport error OUT. Anything already
    // buffered from an earlier spin is still dispatched below on the next call.
    unsafe { drain_into_buffer_raw_c(entry)? };

    match &entry.buffer {
        BufferStrategy::Triple(tb) => match tb.reader_acquire() {
            Some((data, len)) => {
                unsafe { (entry.callback)(data.as_ptr(), len, entry.context) };
                Ok(true)
            }
            None => Ok(false),
        },
        BufferStrategy::Ring(ring) => {
            let mut did_work = false;
            while let Some((data, len)) = ring.try_pop() {
                unsafe { (entry.callback)(data.as_ptr(), len, entry.context) };
                ring.commit_pop();
                did_work = true;
            }
            Ok(did_work)
        }
    }
}

/// Readiness check for C-style raw buffered subscriptions.
///
/// # Safety
/// `ptr` must point to a valid `SubBufferedRawCEntry`.
pub(crate) unsafe fn sub_buffered_raw_c_has_data(ptr: *const u8) -> bool {
    let entry = unsafe { &*(ptr as *const SubBufferedRawCEntry) };
    entry.handle.has_data() || entry.buffer.has_data()
}

// ============================================================================
// Dispatch functions
// ============================================================================

/// Monomorphized subscription dispatch function (with MessageInfo).
///
/// # Safety
/// `ptr` must point to a valid, aligned `SubInfoEntry<M, F, RX_BUF>`.
pub(crate) unsafe fn sub_info_try_process<M, F, const RX_BUF: usize>(
    ptr: *mut u8,
    _delta_us: u64,
) -> Result<bool, TransportError>
where
    M: RosMessage,
    F: FnMut(&M, Option<&MessageInfo>),
{
    let entry = unsafe { &mut *(ptr as *mut SubInfoEntry<M, F, RX_BUF>) };

    // LET mode: use pre-sampled data if available (no MessageInfo in snapshot)
    if entry.sampled_len > 0 {
        let len = entry.sampled_len;
        entry.sampled_len = 0;
        let mut reader = CdrReader::new_with_header(&entry.buffer[..len])
            .map_err(|_| TransportError::DeserializationError)?;
        let msg = M::deserialize(&mut reader).map_err(|_| TransportError::DeserializationError)?;
        (entry.callback)(&msg, None);
        return Ok(true);
    }

    match entry.handle.try_recv_raw_with_info(&mut entry.buffer) {
        Ok(Some((len, info))) => {
            let mut reader = CdrReader::new_with_header(&entry.buffer[..len])
                .map_err(|_| TransportError::DeserializationError)?;
            let msg =
                M::deserialize(&mut reader).map_err(|_| TransportError::DeserializationError)?;
            (entry.callback)(&msg, info.as_ref());
            Ok(true)
        }
        Ok(None) => Ok(false),
        Err(_) => Err(TransportError::DeserializationError),
    }
}

/// Monomorphized subscription dispatch function (with safety validation).
///
/// # Safety
/// `ptr` must point to a valid, aligned `SubSafetyEntry<M, F, RX_BUF>`.
#[cfg(feature = "safety-e2e")]
pub(crate) unsafe fn sub_safety_try_process<M, F, const RX_BUF: usize>(
    ptr: *mut u8,
    _delta_us: u64,
) -> Result<bool, TransportError>
where
    M: RosMessage,
    F: FnMut(&M, &nros_rmw::IntegrityStatus),
{
    let entry = unsafe { &mut *(ptr as *mut SubSafetyEntry<M, F, RX_BUF>) };

    // LET mode: use pre-sampled data (no IntegrityStatus in snapshot)
    if entry.sampled_len > 0 {
        let len = entry.sampled_len;
        entry.sampled_len = 0;
        let mut reader = CdrReader::new_with_header(&entry.buffer[..len])
            .map_err(|_| TransportError::DeserializationError)?;
        let msg = M::deserialize(&mut reader).map_err(|_| TransportError::DeserializationError)?;
        (entry.callback)(
            &msg,
            &nros_rmw::IntegrityStatus {
                gap: 0,
                duplicate: false,
                crc_valid: None,
            },
        );
        return Ok(true);
    }

    match entry.handle.try_recv_validated(&mut entry.buffer) {
        Ok(Some((len, status))) => {
            let mut reader = CdrReader::new_with_header(&entry.buffer[..len])
                .map_err(|_| TransportError::DeserializationError)?;
            let msg =
                M::deserialize(&mut reader).map_err(|_| TransportError::DeserializationError)?;
            (entry.callback)(&msg, &status);
            Ok(true)
        }
        Ok(None) => Ok(false),
        Err(_) => Err(TransportError::DeserializationError),
    }
}

/// Monomorphized service dispatch function.
///
/// # Safety
/// `ptr` must point to a valid, aligned `SrvEntry<Svc, F, REQ_BUF, REPLY_BUF>`.
pub(crate) unsafe fn srv_try_process<Svc, F, const REQ_BUF: usize, const REPLY_BUF: usize>(
    ptr: *mut u8,
    _delta_us: u64,
) -> Result<bool, TransportError>
where
    Svc: RosService,
    F: FnMut(&Svc::Request) -> Svc::Reply,
{
    let entry = unsafe { &mut *(ptr as *mut SrvEntry<Svc, F, REQ_BUF, REPLY_BUF>) };
    // Split borrow: destructure entry to avoid aliasing issues
    let SrvEntry {
        handle,
        req_buffer,
        reply_buffer,
        callback,
        ..
    } = entry;
    handle
        .handle_request::<Svc>(req_buffer, reply_buffer, |req| (callback)(req))
        .map_err(|_| TransportError::ServiceReplyFailed)
}

/// Monomorphized drop function for arena entries.
///
/// # Safety
/// `ptr` must point to a valid, aligned `T` that has not been dropped.
pub(crate) unsafe fn drop_entry<T>(ptr: *mut u8) {
    unsafe { core::ptr::drop_in_place(ptr as *mut T) };
}

/// Monomorphized timer dispatch function.
///
/// # Safety
/// `ptr` must point to a valid, aligned `TimerEntry<F>`.
pub(crate) unsafe fn timer_try_process<F>(
    ptr: *mut u8,
    delta_us: u64,
) -> Result<bool, TransportError>
where
    F: FnMut(),
{
    let entry = unsafe { &mut *(ptr as *mut TimerEntry<F>) };

    // Cancelled or one-shot already fired
    if entry.cancelled || (entry.oneshot && entry.fired) {
        return Ok(false);
    }

    entry.elapsed_us = entry.elapsed_us.saturating_add(delta_us);

    if entry.elapsed_us >= entry.period_us {
        (entry.callback)();
        if entry.oneshot {
            entry.fired = true;
        } else if entry.period_us == 0 {
            entry.elapsed_us = 0;
        } else {
            // Issue #505 — a stall leaves `elapsed_us` worth several
            // periods. `CatchUp` subtracts one period per pass, so the
            // backlog replays back-to-back; `Skip` drops the whole
            // backlog in one step and counts it, keeping the remainder
            // so activations stay on the original phase grid.
            match entry.overrun_policy {
                TimerOverrunPolicy::CatchUp => {
                    entry.elapsed_us = entry.elapsed_us.saturating_sub(entry.period_us);
                }
                TimerOverrunPolicy::Skip => {
                    let missed = entry.elapsed_us / entry.period_us - 1;
                    if missed > 0 {
                        entry.overruns = entry
                            .overruns
                            .saturating_add(u32::try_from(missed).unwrap_or(u32::MAX));
                    }
                    entry.elapsed_us %= entry.period_us;
                }
            }
        }
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Monomorphized action server dispatch function.
///
/// Polls goal acceptance, cancel handling, and result serving.
///
/// # Safety
/// `ptr` must point to a valid, aligned `ActionServerArenaEntry<...>`.
pub(crate) unsafe fn action_server_try_process<
    A,
    GoalF,
    CancelF,
    const GOAL_BUF: usize,
    const RESULT_BUF: usize,
    const FEEDBACK_BUF: usize,
    const MAX_GOALS: usize,
>(
    ptr: *mut u8,
    _delta_us: u64,
) -> Result<bool, TransportError>
where
    A: RosAction,
    A::Goal: Clone,
    A::Result: Clone + Default,
    GoalF: FnMut(&nros_core::GoalId, &A::Goal) -> nros_core::GoalResponse,
    CancelF: FnMut(&nros_core::GoalId, nros_core::GoalStatus) -> nros_core::CancelResponse,
{
    let entry = unsafe {
        &mut *(ptr as *mut ActionServerArenaEntry<
            A,
            GoalF,
            CancelF,
            GOAL_BUF,
            RESULT_BUF,
            FEEDBACK_BUF,
            MAX_GOALS,
        >)
    };
    let ActionServerArenaEntry {
        server,
        goal_callback,
        cancel_callback,
    } = entry;

    let mut did_work = false;

    // Handle cancels first
    if matches!(
        server.try_handle_cancel(|id, st| (cancel_callback)(id, st)),
        Ok(Some(_))
    ) {
        did_work = true;
    }

    // Handle new goals
    if matches!(
        server.try_accept_goal(|id, g| (goal_callback)(id, g)),
        Ok(Some(_))
    ) {
        did_work = true;
    }

    // Handle result requests
    if matches!(server.try_handle_get_result(), Ok(Some(_))) {
        did_work = true;
    }

    Ok(did_work)
}

/// Monomorphized raw action server dispatch function.
///
/// Polls goal acceptance, cancel handling, and result serving using raw bytes.
///
/// # Safety
/// `ptr` must point to a valid, aligned `ActionServerRawArenaEntry<...>`.
pub(crate) unsafe fn action_server_raw_try_process<
    const GOAL_BUF: usize,
    const RESULT_BUF: usize,
    const FEEDBACK_BUF: usize,
    const MAX_GOALS: usize,
>(
    ptr: *mut u8,
    _delta_us: u64,
) -> Result<bool, TransportError> {
    let entry = unsafe {
        &mut *(ptr as *mut ActionServerRawArenaEntry<GOAL_BUF, RESULT_BUF, FEEDBACK_BUF, MAX_GOALS>)
    };
    let ActionServerRawArenaEntry {
        core,
        goal_callback,
        cancel_callback,
        accepted_callback,
        context,
    } = entry;

    let mut did_work = false;

    // Handle cancels first
    if let Ok(Some(_)) =
        core.try_handle_cancel(|id, st| unsafe { (*cancel_callback)(id, st, *context) })
    {
        did_work = true;
    }

    // Handle new goals
    if let Ok(Some(raw_req)) = core.try_recv_goal_request() {
        let goal_data = &core.goal_buffer()[..raw_req.data_len];
        let response = unsafe {
            (*goal_callback)(
                &raw_req.goal_id,
                goal_data.as_ptr(),
                raw_req.data_len,
                *context,
            )
        };

        if response.is_accepted() {
            // Send the accept reply *before* running any long-running
            // post-accept hook so the client observes acceptance promptly.
            let _ = core.accept_goal(raw_req.goal_id, raw_req.sequence_number);
            if let Some(post) = *accepted_callback {
                unsafe { post(&raw_req.goal_id, *context) };
            }
        } else {
            let _ = core.reject_goal(raw_req.sequence_number);
        }
        did_work = true;
    }

    // Handle result requests (empty default result for raw API)
    if let Ok(Some(_)) = core.try_handle_get_result_raw(&[]) {
        did_work = true;
    }

    Ok(did_work)
}

/// Monomorphized raw action client dispatch function.
///
/// Polls the action client core's non-blocking methods:
/// 1. Goal acceptance reply (`try_recv_send_goal_reply`)
/// 2. Feedback (`try_recv_feedback_raw`)
/// 3. Result reply (`try_recv_get_result_reply`)
///
/// Invokes the corresponding callback when data is available.
///
/// # Safety
/// `ptr` must point to a valid, aligned `ActionClientRawArenaEntry<...>`.
/// RFC-0069 / issues 0418 + 0035 — **retired as a correctness mechanism.**
///
/// This sniffed whether a payload already began with a CDR encapsulation header
/// (`00 <id> <opts> <opts>`) and read it directly if so. That is a VALUE test,
/// not a framing test, and it cannot tell the two apart: a leading `uint32` of
/// 256 serializes little-endian as `00 01 00 00` — byte for byte the LE encap
/// header. A sequence of length 256 is enough.
///
/// It was harmless while only Cyclone took the false branch. 0418 stopped the
/// producer writing an inner header, so EVERY payload began consulting it, and
/// a body whose first word happened to look like a header had that word eaten
/// as framing — issue #35's "sequence deserialized to len 0", reached through a
/// payload value instead of a framing bug.
///
/// Kept only for the pre-0418 diagnostic below, never for a decode decision.
#[cfg(test)]
fn payload_has_cdr_encap(p: &[u8]) -> bool {
    p.len() >= 4 && p[0] == 0x00 && matches!(p[1], 0x00 | 0x01 | 0x06 | 0x07 | 0x0a | 0x0b)
}

/// Deserialize an action result/feedback field payload, restoring the per-message
/// CDR encapsulation header the backend's typed framing may have stripped (#175).
/// `raw` is the field bytes at the payload offset; `top_encap` is the enclosing
/// message's leading 4-byte encap (always a valid header). When `raw` already
/// begins with an encap (zenoh/XRCE) it is read directly; when it does not
/// (Cyclone `dds_stream` drops the inner encap of a nested message field) the
/// top-level encap is spliced in front into a `CAP`-byte scratch buffer first.
fn read_action_field<M: nros_serdes::Deserialize, const CAP: usize>(
    top_encap: &[u8],
    raw: &[u8],
) -> Option<M> {
    // ALWAYS splice (RFC-0069 / issue 0418). Post-0418 the producer writes no
    // inner encapsulation header, and neither does Cyclone's `dds_stream` (it
    // drops the inner encap of a nested field, #175) — so the payload is field
    // bytes in every case and the enclosing message's encap is the right one to
    // read them with.
    //
    // This used to branch on `payload_has_cdr_encap(raw)`. That sniff is a value
    // test and gets it wrong for a body starting `00 01 00 00` (a `uint32` of
    // 256, e.g. a 256-element sequence), eating the leading data word — see the
    // retired helper above.
    if top_encap.len() < 4 || raw.len() + 4 > CAP {
        return None;
    }
    let mut buf = [0u8; CAP];
    buf[0..4].copy_from_slice(&top_encap[0..4]);
    buf[4..4 + raw.len()].copy_from_slice(raw);
    let mut reader = CdrReader::new_with_header(&buf[..4 + raw.len()]).ok()?;
    M::deserialize(&mut reader).ok()
}

pub(crate) unsafe fn action_client_raw_try_process<
    const GOAL_BUF: usize,
    const RESULT_BUF: usize,
    const FEEDBACK_BUF: usize,
>(
    ptr: *mut u8,
    _delta_us: u64,
) -> Result<bool, TransportError> {
    let entry = unsafe {
        &mut *(ptr as *mut ActionClientRawArenaEntry<GOAL_BUF, RESULT_BUF, FEEDBACK_BUF>)
    };
    let ActionClientRawArenaEntry {
        core,
        goal_response_callback,
        feedback_callback,
        result_callback,
        context,
    } = entry;

    let mut did_work = false;

    // 1. Poll goal acceptance reply
    if let Ok(Some(total_len)) = core.try_recv_send_goal_reply() {
        if let Some(cb) = goal_response_callback {
            // Reply CDR: header (4) + accepted (u8) + stamp
            let accepted = total_len >= 5 && core.result_buffer[4] != 0;
            // Extract GoalId from the last sent goal
            let goal_id = nros_core::GoalId {
                uuid: {
                    let mut uuid = [0u8; 16];
                    let counter = core.goal_counter.to_le_bytes();
                    uuid[..8].copy_from_slice(&counter);
                    uuid
                },
            };
            unsafe { cb(&goal_id, accepted, *context) };
        }
        did_work = true;
    }

    // 2. Poll feedback
    if let Ok(Some((goal_id, total_len))) = core.try_recv_feedback_raw() {
        if let Some(cb) = feedback_callback {
            // Feedback buffer layout from `publish_feedback_raw` in
            // `action_core.rs`:
            //   bytes 0..4   outer CDR header (`new_with_header`)
            //   bytes 4..20  GoalId.uuid (16 bytes, fixed `uint8[16]`,
            //                no length prefix — ROS 2
            //                `unique_identifier_msgs/UUID`; see
            //                `action_core::write_goal_id`)
            //   bytes 20..   payload — exactly the bytes the caller
            //                of `publish_feedback_raw` handed in
            //                (typed serializers like `ffi_serialize`
            //                write a CDR header at the front).
            //
            // 233.6: the GoalId carries NO sequence-length prefix (it did
            // pre-233.6, which made the offset `4 + 4 + 16`; that framing
            // self-matched nano-ros peers but a real `rcl_action` peer
            // rejects the extra 4 bytes).
            const FEEDBACK_PAYLOAD_OFFSET: usize = 4 + 16;
            if total_len > FEEDBACK_PAYLOAD_OFFSET {
                // #175 + RFC-0069/0418 — the payload is field bytes on every
                // backend now: Cyclone's `dds_stream` drops the inner encap of a
                // nested field, and since 0418 the producer never writes one. So
                // splice the enclosing message's encap unconditionally. This used
                // to branch on an encap SNIFF, which mis-fires on a body whose
                // first word is 0x00010000 (see the retired helper).
                let raw = &core.feedback_buffer[FEEDBACK_PAYLOAD_OFFSET..total_len];
                let mut spliced = [0u8; FEEDBACK_BUF];
                let n = raw.len().min(FEEDBACK_BUF - 4);
                spliced[0..4].copy_from_slice(&core.feedback_buffer[0..4]);
                spliced[4..4 + n].copy_from_slice(&raw[..n]);
                unsafe { cb(&goal_id, spliced.as_ptr(), 4 + n, *context) };
            }
        }
        did_work = true;
    }

    // 3. Poll result reply
    if let Ok(Some(total_len)) = core.try_recv_get_result_reply() {
        if let Some(cb) = result_callback {
            // Reply layout from `try_handle_get_result_raw` in
            // `action_core.rs`:
            //   bytes 0..4   outer CDR header (`new_with_header`)
            //   byte  4      status (i8)
            //   bytes 5..8   align(4) pad
            //   bytes 8..    payload — exactly the bytes the caller
            //                of `complete_goal_raw` handed in (typed
            //                serializers like `ffi_serialize` write
            //                a CDR header at the front, which is
            //                why the alignment pad above is sized
            //                to land the payload at a 4-byte boundary).
            //
            // The trampoline forwards `payload` to the C/C++
            // callback verbatim — the cpp wrapper expects to see
            // the inner CDR header that `ffi_serialize` wrote.
            // Earlier code used `result_offset = 5` and skipped
            // only the status byte; that leaked the 3 alignment
            // pad bytes into the payload prefix and blew up
            // `ffi_deserialize`, surfacing as an empty result on
            // the cpp/xrce action client (Phase 96.1 follow-up).
            const RESULT_PAYLOAD_OFFSET: usize = 8;
            if total_len >= RESULT_PAYLOAD_OFFSET {
                let status_byte = core.result_buffer[4];
                let status = match status_byte {
                    4 => nros_core::GoalStatus::Succeeded,
                    5 => nros_core::GoalStatus::Canceled,
                    6 => nros_core::GoalStatus::Aborted,
                    _ => nros_core::GoalStatus::Unknown,
                };
                // Extract GoalId from the last sent goal
                let goal_id = nros_core::GoalId {
                    uuid: {
                        let mut uuid = [0u8; 16];
                        let counter = core.goal_counter.to_le_bytes();
                        uuid[..8].copy_from_slice(&counter);
                        uuid
                    },
                };
                // #175 + RFC-0069/0418 — restore the result's CDR encapsulation
                // header. The fields arrive raw at `RESULT_PAYLOAD_OFFSET` on
                // every backend now: Cyclone sends `GetResult_Response` as a
                // TYPED sample whose `result` is a NESTED field, so `dds_stream`
                // consumes the inner encap; and since 0418 the producer never
                // writes one. The consumer (`CallbackCtx::message` /
                // `ffi_deserialize`) reads with `new_with_header`, so splice the
                // reply's top-level encap (`result_buffer[0..4]`, always valid)
                // in front unconditionally.
                //
                // This used to branch on an encap SNIFF, justified by the claim
                // that "a raw CDR field never begins with an encoding
                // identifier". That claim is false — a leading `int32` of 256 is
                // `00 01 00 00` — and once 0418 made every payload header-less
                // the sniff decided every decode, so such a body had its first
                // word eaten as framing: the very corruption this comment warns
                // about, caused by the guard against it. See the retired
                // `payload_has_cdr_encap`.
                let raw = &core.result_buffer[RESULT_PAYLOAD_OFFSET..total_len];
                let mut spliced = [0u8; RESULT_BUF];
                let n = raw.len().min(RESULT_BUF - 4);
                spliced[0..4].copy_from_slice(&core.result_buffer[0..4]);
                spliced[4..4 + n].copy_from_slice(&raw[..n]);
                unsafe { cb(&goal_id, status, spliced.as_ptr(), 4 + n, *context) };
            }
        }
        did_work = true;
    }

    Ok(did_work)
}

/// Monomorphized raw service-client dispatch function.
///
/// Checks `reply_ready` (set by the transport waker) before calling
/// `try_recv_reply_raw`. This avoids blind polling on every spin tick —
/// the only cost per tick is an atomic load when no reply is pending.
///
/// # Safety
/// `ptr` must point to a valid, aligned `ServiceClientRawArenaEntry<REPLY_BUF>`.
pub(crate) unsafe fn service_client_raw_try_process<const REPLY_BUF: usize>(
    ptr: *mut u8,
    _delta_us: u64,
) -> Result<bool, TransportError> {
    use core::sync::atomic::Ordering;
    use nros_rmw::ClientTrait;
    let entry = unsafe { &mut *(ptr as *mut ServiceClientRawArenaEntry<REPLY_BUF>) };

    if !entry.pending {
        return Ok(false);
    }

    // Clear the waker flag if set (consumed by this check).
    entry.reply_ready.store(false, Ordering::Release);

    // Issue 0778 — one call per arena entry today, so the sequence id is
    // discarded rather than absent. Correlating here means keying entries by
    // it, which is the follow-up that issue tracks.
    match entry.handle.try_recv_reply_raw(&mut entry.reply_buffer) {
        Ok(Some((len, _seq))) => {
            entry.pending = false;
            if let Some(cb) = entry.callback {
                unsafe { cb(entry.reply_buffer.as_ptr(), len, entry.context) };
            }
            Ok(true)
        }
        Ok(None) => Ok(false),
        Err(_) => {
            entry.pending = false;
            Err(TransportError::ServiceRequestFailed)
        }
    }
}

/// RFC-0041 / Phase 239.1 — F-independent prefix of a typed service-client
/// callback entry. `#[repr(C)]` guarantees it is the leading member of every
/// [`ServiceClientCallbackEntry`] regardless of the closure type `F`, so a
/// `ServiceClientCallback` handle can hold a `*mut` to it and send requests
/// (serialize → `send_request_raw` → set `pending`) without naming `F`.
#[repr(C)]
pub struct ServiceClientSendHeader<const REPLY_BUF: usize> {
    pub handle: session::RmwServiceClient,
    pub reply_buffer: [u8; REPLY_BUF],
    pub pending: bool,
    /// Set by the transport waker when a reply arrives (mirrors the raw entry).
    pub reply_ready: core::sync::atomic::AtomicBool,
}

/// Typed service-client callback entry (RFC-0041, Phase 239.1). The executor
/// eager-drains the reply at `spin_once` and dispatches it as a deserialized
/// `Svc::Reply` to the user closure — the typed analogue of
/// [`ServiceClientRawArenaEntry`]. The send side goes through the embedded
/// [`ServiceClientSendHeader`] (offset 0) via a `ServiceClientCallback` handle.
#[repr(C)]
pub(crate) struct ServiceClientCallbackEntry<Svc: RosService, F, const REPLY_BUF: usize> {
    pub(crate) hdr: ServiceClientSendHeader<REPLY_BUF>,
    pub(crate) callback: F,
    pub(crate) _phantom: PhantomData<Svc>,
}

/// Monomorphized typed service-client dispatch (RFC-0041, Phase 239.1).
///
/// Mirrors [`service_client_raw_try_process`] but deserializes the reply into
/// `Svc::Reply` and invokes the typed closure. Single in-flight request gated by
/// `hdr.pending`; the reply view is dropped before return (no escape).
///
/// # Safety
/// `ptr` must point to a valid, aligned `ServiceClientCallbackEntry<Svc, F, REPLY_BUF>`.
pub(crate) unsafe fn service_client_callback_try_process<Svc, F, const REPLY_BUF: usize>(
    ptr: *mut u8,
    _delta_us: u64,
) -> Result<bool, TransportError>
where
    Svc: RosService,
    F: FnMut(&Svc::Reply),
{
    use core::sync::atomic::Ordering;
    use nros_rmw::ClientTrait;
    let entry = unsafe { &mut *(ptr as *mut ServiceClientCallbackEntry<Svc, F, REPLY_BUF>) };

    if !entry.hdr.pending {
        return Ok(false);
    }
    entry.hdr.reply_ready.store(false, Ordering::Release);

    match entry
        .hdr
        .handle
        .try_recv_reply_raw(&mut entry.hdr.reply_buffer)
    {
        Ok(Some((len, _seq))) => {
            entry.hdr.pending = false;
            let mut reader = CdrReader::new_with_header(&entry.hdr.reply_buffer[..len])
                .map_err(|_| TransportError::DeserializationError)?;
            // Fully-qualify the `Deserialize` trait (mirrors the
            // `DeserializeBorrowed` call above): arena.rs imports
            // `DeserializeBorrowed` but not `Deserialize`, so the bare
            // `Svc::Reply::deserialize` only resolved when a default/std feature
            // happened to glob it into scope — under `rmw-cffi` (embedded) it
            // failed E0599. The fully-qualified path resolves under every feature.
            let reply = <Svc::Reply as nros_serdes::Deserialize>::deserialize(&mut reader)
                .map_err(|_| TransportError::DeserializationError)?;
            (entry.callback)(&reply);
            Ok(true)
        }
        Ok(None) => Ok(false),
        Err(_) => {
            entry.hdr.pending = false;
            Err(TransportError::ServiceRequestFailed)
        }
    }
}

/// Typed action-client callback entry (RFC-0041, Phase 239.2). The executor
/// eager-drains the three client receives (goal-response / feedback / result)
/// at `spin_once` and dispatches them as deserialized `A::Feedback` / `A::Result`
/// to typed closures — the typed analogue of [`ActionClientRawArenaEntry`]. The
/// send side (`send_goal` / `get_result`) goes through the embedded `core`
/// (offset 0) via an `ActionClientCallback` handle.
#[repr(C)]
pub(crate) struct ActionClientCallbackEntry<
    A: RosAction,
    GRespF,
    FbF,
    ResF,
    const GOAL_BUF: usize,
    const RESULT_BUF: usize,
    const FEEDBACK_BUF: usize,
> {
    pub(crate) core: ActionClientCore<GOAL_BUF, RESULT_BUF, FEEDBACK_BUF>,
    /// RFC-0041 / Phase 239.5 — the feedback stream's QoS-depth buffer. The
    /// callback path drains `core.feedback_subscriber` directly into this ring
    /// (depth > 1) or triple-buffer (depth ≤ 1), so a burst of feedbacks between
    /// spins is buffered/reported instead of overwriting a single slot — and the
    /// shared `ActionClientCore` buffers (the `Promise` path) stay untouched.
    pub(crate) feedback_buffer: BufferStrategy,
    pub(crate) on_goal_response: GRespF,
    pub(crate) on_feedback: FbF,
    pub(crate) on_result: ResF,
    pub(crate) _phantom: PhantomData<A>,
}

/// Reconstruct a `GoalId` from the core's monotonically increasing counter
/// (mirrors `action_client_raw_try_process`).
#[inline]
fn goal_id_from_counter(counter: u64) -> nros_core::GoalId {
    let mut uuid = [0u8; 16];
    uuid[..8].copy_from_slice(&counter.to_le_bytes());
    nros_core::GoalId { uuid }
}

/// Decode one raw feedback slot (outer header + GoalId at [4..20] + inner-CDR
/// payload at `offset`) and invoke the typed feedback closure (Phase 239.5).
#[inline]
fn dispatch_feedback<A, F, const FEEDBACK_BUF: usize>(
    data: &[u8],
    offset: usize,
    on_feedback: &mut F,
) where
    A: RosAction,
    F: FnMut(&nros_core::GoalId, &A::Feedback),
{
    if data.len() <= offset {
        return;
    }
    let mut uuid = [0u8; 16];
    uuid.copy_from_slice(&data[4..20]);
    let goal_id = nros_core::GoalId { uuid };
    // #175 — restore the feedback's per-message encap if a typed transport
    // framing (Cyclone) stripped it; the message's top-level encap is `data[0..4]`.
    if let Some(fb) = read_action_field::<A::Feedback, FEEDBACK_BUF>(&data[0..4], &data[offset..]) {
        on_feedback(&goal_id, &fb);
    }
}

/// Monomorphized typed action-client dispatch (RFC-0041, Phase 239.2). Mirrors
/// [`action_client_raw_try_process`] but deserializes the feedback / result
/// payloads into `A::Feedback` / `A::Result` and invokes the typed closures.
///
/// # Safety
/// `ptr` must point to a valid, aligned `ActionClientCallbackEntry<…>`.
#[allow(clippy::type_complexity)]
pub(crate) unsafe fn action_client_callback_try_process<
    A,
    GRespF,
    FbF,
    ResF,
    const GOAL_BUF: usize,
    const RESULT_BUF: usize,
    const FEEDBACK_BUF: usize,
>(
    ptr: *mut u8,
    _delta_us: u64,
) -> Result<bool, TransportError>
where
    A: RosAction,
    GRespF: FnMut(&nros_core::GoalId, bool),
    FbF: FnMut(&nros_core::GoalId, &A::Feedback),
    ResF: FnMut(&nros_core::GoalId, nros_core::GoalStatus, &A::Result),
{
    let entry = unsafe {
        &mut *(ptr as *mut ActionClientCallbackEntry<
            A,
            GRespF,
            FbF,
            ResF,
            GOAL_BUF,
            RESULT_BUF,
            FEEDBACK_BUF,
        >)
    };
    let ActionClientCallbackEntry {
        core,
        feedback_buffer,
        on_goal_response,
        on_feedback,
        on_result,
        _phantom,
    } = entry;

    let mut did_work = false;

    // 1. Goal-acceptance reply (single-outstanding → gated single buffer).
    if let Ok(Some(total_len)) = core.try_recv_send_goal_reply() {
        let accepted = total_len >= 5 && core.result_buffer[4] != 0;
        let goal_id = goal_id_from_counter(core.goal_counter);
        on_goal_response(&goal_id, accepted);
        did_work = true;
    }

    // 2. Feedback — a stream: drain `feedback_subscriber` into the QoS-depth ring
    //    (Phase 239.5), then dispatch each slot. Each slot holds the raw feedback
    //    message: outer CDR header (4) + GoalId (16) + inner-CDR payload; see the
    //    raw dispatcher for the layout rationale (233.6).
    {
        const FEEDBACK_PAYLOAD_OFFSET: usize = 4 + 16;
        match feedback_buffer {
            BufferStrategy::Triple(tb) => {
                let slot = tb.write_slot();
                if let Ok(Some(len)) = core.feedback_subscriber.try_recv_raw(slot) {
                    tb.writer_publish(len);
                }
                if let Some((data, len)) = tb.reader_acquire() {
                    dispatch_feedback::<A, _, FEEDBACK_BUF>(
                        &data[..len],
                        FEEDBACK_PAYLOAD_OFFSET,
                        on_feedback,
                    );
                    did_work = true;
                }
            }
            BufferStrategy::Ring(ring) => {
                while let Some(slot) = ring.try_push() {
                    match core.feedback_subscriber.try_recv_raw(slot) {
                        Ok(Some(len)) => ring.commit_push(len),
                        _ => break,
                    }
                }
                while let Some((data, len)) = ring.try_pop() {
                    dispatch_feedback::<A, _, FEEDBACK_BUF>(
                        &data[..len],
                        FEEDBACK_PAYLOAD_OFFSET,
                        on_feedback,
                    );
                    ring.commit_pop();
                    did_work = true;
                }
            }
        }
    }

    // 3. Result reply — status at byte 4, payload at [8 ..] (header + status +
    //    align pad); see the raw dispatcher (Phase 96.1).
    if let Ok(Some(total_len)) = core.try_recv_get_result_reply() {
        const RESULT_PAYLOAD_OFFSET: usize = 8;
        if total_len >= RESULT_PAYLOAD_OFFSET {
            let status = match core.result_buffer[4] {
                4 => nros_core::GoalStatus::Succeeded,
                5 => nros_core::GoalStatus::Canceled,
                6 => nros_core::GoalStatus::Aborted,
                _ => nros_core::GoalStatus::Unknown,
            };
            let goal_id = goal_id_from_counter(core.goal_counter);
            // #175 — restore the result's per-message encap if a typed transport
            // framing (Cyclone) stripped it; the reply's top-level encap is
            // `result_buffer[0..4]`.
            if let Some(res) = read_action_field::<A::Result, RESULT_BUF>(
                &core.result_buffer[0..4],
                &core.result_buffer[RESULT_PAYLOAD_OFFSET..total_len],
            ) {
                on_result(&goal_id, status, &res);
            }
        }
        did_work = true;
    }

    Ok(did_work)
}

/// Monomorphized raw service dispatch function.
///
/// # Safety
/// `ptr` must point to a valid, aligned `SrvRawEntry<REQ_BUF, REPLY_BUF>`.
pub(crate) unsafe fn srv_raw_try_process<const REQ_BUF: usize, const REPLY_BUF: usize>(
    ptr: *mut u8,
    _delta_us: u64,
) -> Result<bool, TransportError> {
    let entry = unsafe { &mut *(ptr as *mut SrvRawEntry<REQ_BUF, REPLY_BUF>) };
    let SrvRawEntry {
        handle,
        req_buffer,
        reply_buffer,
        callback,
        context,
    } = entry;
    let buf_start = req_buffer.as_ptr() as usize;
    let (data_offset, data_len, seq_num) = match handle.try_recv_request(req_buffer) {
        Ok(Some(request)) => {
            let offset = (request.data.as_ptr() as usize).saturating_sub(buf_start);
            let len = request.data.len();
            let seq = request.sequence_number;
            (offset, len, seq)
        }
        Ok(None) => return Ok(false),
        Err(_) => return Err(TransportError::ServiceReplyFailed),
    };

    let mut resp_len: usize = 0;
    let ok = unsafe {
        (*callback)(
            req_buffer.as_ptr().add(data_offset),
            data_len,
            reply_buffer.as_mut_ptr(),
            REPLY_BUF,
            &mut resp_len,
            *context,
        )
    };
    if ok && resp_len > 0 {
        handle
            .send_response(seq_num, &reply_buffer[..resp_len])
            .map_err(|_| TransportError::ServiceReplyFailed)?;
    }
    Ok(true)
}

/// Monomorphized guard condition dispatch function.
///
/// # Safety
/// `ptr` must point to a valid, aligned `GuardConditionEntry<F>`.
pub(crate) unsafe fn guard_try_process<F>(
    ptr: *mut u8,
    _delta_us: u64,
) -> Result<bool, TransportError>
where
    F: FnMut(),
{
    let entry = unsafe { &mut *(ptr as *mut GuardConditionEntry<F>) };
    if entry.flag.swap(false, portable_atomic::Ordering::AcqRel) {
        (entry.callback)();
        Ok(true)
    } else {
        Ok(false)
    }
}

// ============================================================================
// Readiness check functions
// ============================================================================

/// SubInfoEntry readiness.
///
/// # Safety
/// `ptr` must point to a valid `SubInfoEntry<M, F, RX_BUF>`.
pub(crate) unsafe fn sub_info_has_data<M, F, const RX_BUF: usize>(ptr: *const u8) -> bool {
    let entry = unsafe { &*(ptr as *const SubInfoEntry<M, F, RX_BUF>) };
    entry.handle.has_data()
}

/// SubSafetyEntry readiness.
///
/// # Safety
/// `ptr` must point to a valid `SubSafetyEntry<M, F, RX_BUF>`.
#[cfg(feature = "safety-e2e")]
pub(crate) unsafe fn sub_safety_has_data<M, F, const RX_BUF: usize>(ptr: *const u8) -> bool {
    let entry = unsafe { &*(ptr as *const SubSafetyEntry<M, F, RX_BUF>) };
    entry.handle.has_data()
}

/// Service readiness: check `has_request()` on the service handle.
///
/// # Safety
/// `ptr` must point to a valid `SrvEntry<Svc, F, RQ, RP>`.
pub(crate) unsafe fn srv_has_data<Svc: RosService, F, const RQ: usize, const RP: usize>(
    ptr: *const u8,
) -> bool {
    let entry = unsafe { &*(ptr as *const SrvEntry<Svc, F, RQ, RP>) };
    entry.handle.has_request()
}

/// Raw service readiness.
///
/// # Safety
/// `ptr` must point to a valid `SrvRawEntry<RQ, RP>`.
pub(crate) unsafe fn srv_raw_has_data<const RQ: usize, const RP: usize>(ptr: *const u8) -> bool {
    let entry = unsafe { &*(ptr as *const SrvRawEntry<RQ, RP>) };
    entry.handle.has_request()
}

/// Guard condition readiness: check the atomic flag.
///
/// # Safety
/// `ptr` must point to a valid `GuardConditionEntry<F>`.
pub(crate) unsafe fn guard_has_data<F>(ptr: *const u8) -> bool {
    let entry = unsafe { &*(ptr as *const GuardConditionEntry<F>) };
    entry.flag.load(portable_atomic::Ordering::Acquire)
}

/// Timers and action entries are always considered ready.
pub(crate) unsafe fn always_ready(_ptr: *const u8) -> bool {
    true
}

// ============================================================================
// LET pre-sample functions
// ============================================================================

/// Pre-sample a typed subscription with MessageInfo for LET mode.
///
/// # Safety
/// `ptr` must point to a valid, aligned `SubInfoEntry<M, F, RX_BUF>`.
pub(crate) unsafe fn sub_info_pre_sample<M, F, const RX_BUF: usize>(ptr: *mut u8) {
    let entry = unsafe { &mut *(ptr as *mut SubInfoEntry<M, F, RX_BUF>) };
    // For LET, we sample only the data (MessageInfo is not preserved in the snapshot)
    entry.sampled_len = match entry.handle.try_recv_raw(&mut entry.buffer) {
        Ok(Some(len)) => len,
        _ => 0,
    };
}

/// Pre-sample a safety subscription for LET mode.
///
/// # Safety
/// `ptr` must point to a valid, aligned `SubSafetyEntry<M, F, RX_BUF>`.
#[cfg(feature = "safety-e2e")]
pub(crate) unsafe fn sub_safety_pre_sample<M, F, const RX_BUF: usize>(ptr: *mut u8) {
    let entry = unsafe { &mut *(ptr as *mut SubSafetyEntry<M, F, RX_BUF>) };
    entry.sampled_len = match entry.handle.try_recv_raw(&mut entry.buffer) {
        Ok(Some(len)) => len,
        _ => 0,
    };
}

/// No-op pre-sample for non-subscription entries (services, timers, etc.).
pub(crate) unsafe fn no_pre_sample(_ptr: *mut u8) {}

// ============================================================================
// Monomorphized handle operation functions
// ============================================================================

/// Action server: publish feedback via arena entry.
///
/// # Safety
/// `ptr` must point to a valid `ActionServerArenaEntry`.
pub(crate) unsafe fn as_publish_feedback<
    A,
    GoalF,
    CancelF,
    const GB: usize,
    const RB: usize,
    const FB: usize,
    const MG: usize,
>(
    ptr: *mut u8,
    goal_id: &nros_core::GoalId,
    feedback: &A::Feedback,
) -> Result<(), NodeError>
where
    A: RosAction,
{
    let entry =
        unsafe { &mut *(ptr as *mut ActionServerArenaEntry<A, GoalF, CancelF, GB, RB, FB, MG>) };
    entry.server.publish_feedback(goal_id, feedback)
}

/// Action server: complete a goal via arena entry.
///
/// # Safety
/// `ptr` must point to a valid `ActionServerArenaEntry`.
pub(crate) unsafe fn as_complete_goal<
    A,
    GoalF,
    CancelF,
    const GB: usize,
    const RB: usize,
    const FB: usize,
    const MG: usize,
>(
    ptr: *mut u8,
    goal_id: &nros_core::GoalId,
    status: nros_core::GoalStatus,
    result: A::Result,
) -> Result<(), NodeError>
where
    A: RosAction,
{
    let entry =
        unsafe { &mut *(ptr as *mut ActionServerArenaEntry<A, GoalF, CancelF, GB, RB, FB, MG>) };
    entry.server.complete_goal(goal_id, status, result)
}

/// Action server: set goal status via arena entry.
///
/// # Safety
/// `ptr` must point to a valid `ActionServerArenaEntry`.
pub(crate) unsafe fn as_set_goal_status<
    A,
    GoalF,
    CancelF,
    const GB: usize,
    const RB: usize,
    const FB: usize,
    const MG: usize,
>(
    ptr: *mut u8,
    goal_id: &nros_core::GoalId,
    status: nros_core::GoalStatus,
) where
    A: RosAction,
{
    let entry =
        unsafe { &mut *(ptr as *mut ActionServerArenaEntry<A, GoalF, CancelF, GB, RB, FB, MG>) };
    entry.server.set_goal_status(goal_id, status);
}

/// Action server: get active goal count via arena entry.
///
/// # Safety
/// `ptr` must point to a valid `ActionServerArenaEntry`.
pub(crate) unsafe fn as_active_goal_count<
    A,
    GoalF,
    CancelF,
    const GB: usize,
    const RB: usize,
    const FB: usize,
    const MG: usize,
>(
    ptr: *const u8,
) -> usize
where
    A: RosAction,
{
    let entry =
        unsafe { &*(ptr as *const ActionServerArenaEntry<A, GoalF, CancelF, GB, RB, FB, MG>) };
    entry.server.active_goal_count()
}

/// Raw action server: publish feedback via arena entry.
///
/// # Safety
/// `ptr` must point to a valid `ActionServerRawArenaEntry`.
pub(crate) unsafe fn as_raw_publish_feedback<
    const GB: usize,
    const RB: usize,
    const FB: usize,
    const MG: usize,
>(
    ptr: *mut u8,
    goal_id: &nros_core::GoalId,
    feedback_data: *const u8,
    feedback_len: usize,
) -> Result<(), NodeError> {
    let entry = unsafe { &mut *(ptr as *mut ActionServerRawArenaEntry<GB, RB, FB, MG>) };
    let feedback_cdr = unsafe { core::slice::from_raw_parts(feedback_data, feedback_len) };
    entry.core.publish_feedback_raw(goal_id, feedback_cdr)
}

/// Raw action server: complete a goal via arena entry.
///
/// # Safety
/// `ptr` must point to a valid `ActionServerRawArenaEntry`.
pub(crate) unsafe fn as_raw_complete_goal<
    const GB: usize,
    const RB: usize,
    const FB: usize,
    const MG: usize,
>(
    ptr: *mut u8,
    goal_id: &nros_core::GoalId,
    status: nros_core::GoalStatus,
    result_data: *const u8,
    result_len: usize,
) -> Result<(), NodeError> {
    let entry = unsafe { &mut *(ptr as *mut ActionServerRawArenaEntry<GB, RB, FB, MG>) };
    let result_cdr = unsafe { core::slice::from_raw_parts(result_data, result_len) };
    entry.core.complete_goal_raw(goal_id, status, result_cdr)
}

/// Raw action server: set goal status via arena entry.
///
/// # Safety
/// `ptr` must point to a valid `ActionServerRawArenaEntry`.
pub(crate) unsafe fn as_raw_set_goal_status<
    const GB: usize,
    const RB: usize,
    const FB: usize,
    const MG: usize,
>(
    ptr: *mut u8,
    goal_id: &nros_core::GoalId,
    status: nros_core::GoalStatus,
) {
    let entry = unsafe { &mut *(ptr as *mut ActionServerRawArenaEntry<GB, RB, FB, MG>) };
    entry.core.set_goal_status(goal_id, status);
}

/// Raw action server: get active goal count via arena entry.
///
/// # Safety
/// `ptr` must point to a valid `ActionServerRawArenaEntry`.
pub(crate) unsafe fn as_raw_active_goal_count<
    const GB: usize,
    const RB: usize,
    const FB: usize,
    const MG: usize,
>(
    ptr: *const u8,
) -> usize {
    let entry = unsafe { &*(ptr as *const ActionServerRawArenaEntry<GB, RB, FB, MG>) };
    entry.core.active_goal_count()
}

/// Raw action server: iterate active goals via arena entry.
///
/// # Safety
/// `ptr` must point to a valid `ActionServerRawArenaEntry`.
pub(crate) unsafe fn as_raw_for_each_active_goal<
    const GB: usize,
    const RB: usize,
    const FB: usize,
    const MG: usize,
>(
    ptr: *const u8,
    f: &mut dyn FnMut(&super::action_core::RawActiveGoal),
) {
    let entry = unsafe { &*(ptr as *const ActionServerRawArenaEntry<GB, RB, FB, MG>) };
    for goal in entry.core.active_goals() {
        f(goal);
    }
}

/// Action server: iterate active goals via arena entry.
///
/// Calls `f` for each active goal, reconstructing `ActiveGoal<A>` from
/// the core's `RawActiveGoal` and the parallel typed goals vec.
///
/// # Safety
/// `ptr` must point to a valid `ActionServerArenaEntry`.
pub(crate) unsafe fn as_for_each_active_goal<
    A,
    GoalF,
    CancelF,
    const GB: usize,
    const RB: usize,
    const FB: usize,
    const MG: usize,
>(
    ptr: *const u8,
    f: &mut dyn FnMut(&ActiveGoal<A>),
) where
    A: RosAction + 'static,
    A::Goal: Clone,
{
    let entry =
        unsafe { &*(ptr as *const ActionServerArenaEntry<A, GoalF, CancelF, GB, RB, FB, MG>) };
    for (i, raw_goal) in entry.server.core.active_goals().iter().enumerate() {
        let active = ActiveGoal {
            goal_id: raw_goal.goal_id,
            status: raw_goal.status,
            goal: entry.server.typed_goals[i].clone(),
        };
        f(&active);
    }
}

#[cfg(test)]
mod borrowed_sub_tests {
    use nros_core::{CdrReader, CdrWriter, DeserError};

    use super::*;

    // Hand-written borrowed message mirroring what codegen will emit for
    // `{ uint32 width; uint8[] data; }` in `borrowed` mode (Phase 229.6).
    struct ImageView<'a> {
        width: u32,
        data: &'a [u8],
    }

    impl<'a> DeserializeBorrowed<'a> for ImageView<'a> {
        fn deserialize_borrowed(reader: &mut CdrReader<'a>) -> Result<Self, DeserError> {
            let width = reader.read_u32()?;
            let data = reader.read_slice_u8()?;
            Ok(ImageView { width, data })
        }
    }

    // Zero-sized borrowed-family marker (codegen emits `struct ImageBorrow;`).
    struct ImageBorrow;
    impl BorrowedMessage for ImageBorrow {
        type View<'a> = ImageView<'a>;
        const TYPE_NAME: &'static str = "test_msgs::msg::dds_::Image_";
        const TYPE_HASH: &'static str = "borrowed-test-hash";
    }

    // The borrowed view must alias the source CDR buffer (no `heapless::Vec`
    // copy) — the whole point of `borrowed` mode (issue 0007).
    #[test]
    fn borrowed_view_is_zero_copy_into_source_buffer() {
        let payload: [u8; 64] = core::array::from_fn(|i| i as u8);
        let mut buf = [0u8; 128];
        let written = {
            let mut w = CdrWriter::new_with_header(&mut buf).unwrap();
            w.write_u32(7).unwrap();
            w.write_sequence_len(payload.len()).unwrap();
            w.write_bytes(&payload).unwrap();
            w.position()
        };

        let mut reader = CdrReader::new_with_header(&buf[..written]).unwrap();
        let view = ImageView::deserialize_borrowed(&mut reader).unwrap();

        assert_eq!(view.width, 7);
        assert_eq!(view.data, &payload[..]);

        // The borrowed slice points INTO `buf`, proving zero-copy.
        let buf_start = buf.as_ptr() as usize;
        let buf_end = buf_start + buf.len();
        let data_ptr = view.data.as_ptr() as usize;
        assert!(
            data_ptr >= buf_start && data_ptr < buf_end,
            "borrowed data must alias the source buffer (zero-copy)"
        );
    }

    // Phase 231 Wave 3 (RFC-0038) — single-copy proof. The in-place subscription
    // entry carries handle + callback only; the buffered entry additionally
    // carries the arena `BufferStrategy` (the copy-#1 staging buffer). So the
    // in-place entry is strictly smaller — proving the arena buffer (and copy #1)
    // is gone for backends that support in-place dispatch.
    #[test]
    fn inplace_entry_drops_the_arena_buffer() {
        type Cb = fn(&u32);
        assert!(
            core::mem::size_of::<SubInplaceEntry<u32, Cb>>()
                < core::mem::size_of::<SubBufferedEntry<u32, Cb>>(),
            "in-place entry must be smaller than the buffered entry (no arena BufferStrategy)"
        );
    }

    // Compile-time proof that the codegen marker + GAT + a borrowed closure
    // satisfy exactly the bounds the executor's borrowed dispatch
    // (`sub_buffered_borrowed_try_process`) and registration require.
    fn assert_borrowed_sub_bounds<B, F>(_callback: F)
    where
        B: BorrowedMessage + 'static,
        F: for<'a> FnMut(&B::View<'a>) + 'static,
    {
    }

    #[test]
    fn borrowed_marker_satisfies_dispatch_bounds() {
        assert_borrowed_sub_bounds::<ImageBorrow, _>(|view: &ImageView<'_>| {
            let _ = view.width;
            let _ = view.data.len();
        });
    }
}

/// RFC-0069 / issues 0418 + 0035 — the action payload envelope carries exactly
/// ONE CDR header, and reading it back must not eat a data word.
///
/// The RFC lists this as the acceptance item "most likely to be skipped: its
/// absence is why the divergence survived a redesign in the first place." These
/// sit at the level the corruption happens — `read_action_field`, the consumer
/// half of the 0418 producer/consumer pair.
#[cfg(test)]
mod action_envelope_tests {
    use nros_core::{CdrWriter, Deserialize, Serialize};

    use super::*;

    /// Three `int32`s. The FIRST is the one that matters: give it the value 256
    /// and the body's leading bytes are `00 01 00 00` — byte for byte the
    /// little-endian CDR encapsulation header.
    #[derive(Debug, PartialEq)]
    struct Body {
        first: i32,
        rest: [i32; 2],
    }

    impl Serialize for Body {
        fn serialize(&self, w: &mut CdrWriter) -> Result<(), nros_serdes::SerError> {
            w.write_i32(self.first)?;
            for v in &self.rest {
                w.write_i32(*v)?;
            }
            Ok(())
        }
    }

    impl Deserialize for Body {
        fn deserialize(r: &mut CdrReader) -> Result<Self, nros_serdes::DeserError> {
            Ok(Body {
                first: r.read_i32()?,
                rest: [r.read_i32()?, r.read_i32()?],
            })
        }
    }

    const LE_ENCAP: [u8; 4] = [0x00, 0x01, 0x00, 0x00];

    /// Serialize the way the 0418 PRODUCER does: fields only, no inner header.
    fn produce_headerless(body: &Body, buf: &mut [u8]) -> usize {
        let mut w = CdrWriter::new(buf);
        body.serialize(&mut w).expect("serialize");
        w.position()
    }

    #[test]
    fn headerless_payload_round_trips() {
        let body = Body {
            first: 7,
            rest: [1, 1],
        };
        let mut buf = [0u8; 64];
        let n = produce_headerless(&body, &mut buf);

        let got = read_action_field::<Body, 64>(&LE_ENCAP, &buf[..n])
            .expect("consumer must decode the headerless payload");
        assert_eq!(got, body, "issue 0035: the reader ate a data word");
    }

    /// THE hazard 0418's own doc names and did not test.
    ///
    /// `payload_has_cdr_encap` was a VALUE sniff: a leading `int32` of 256 is
    /// `00 01 00 00`, indistinguishable from the LE encap header. While the
    /// producer wrote an inner header only Cyclone took the other branch, so it
    /// never mattered; once 0418 made every payload headerless the sniff was
    /// consulted for all of them, and this body had its first word eaten as
    /// framing — `first` came back as `rest[0]`. Issue #35 reached through a
    /// payload VALUE rather than a framing bug.
    #[test]
    fn a_leading_word_that_looks_like_an_encap_is_data_not_framing() {
        let body = Body {
            first: 256,
            rest: [11, 12],
        };
        let mut buf = [0u8; 64];
        let n = produce_headerless(&body, &mut buf);

        assert_eq!(
            &buf[..4],
            &LE_ENCAP,
            "precondition: an int32 of 256 IS the LE encap byte pattern"
        );
        assert!(
            payload_has_cdr_encap(&buf[..n]),
            "precondition: the retired sniff cannot tell this from a header"
        );

        let got = read_action_field::<Body, 64>(&LE_ENCAP, &buf[..n]).expect("must decode");
        assert_eq!(
            got, body,
            "the leading word is DATA — reading it as framing is issue #35"
        );
    }

    /// The version break RFC-0069 accepts, asserted rather than assumed.
    ///
    /// A pre-0418 peer sends `[inner header][fields]`. The consumer no longer
    /// sniffs, so it splices unconditionally and the inner header is read as
    /// data. Old and new images are wire-incompatible on action payloads — the
    /// RFC's "silent version skew" risk, made explicit here so the next reader
    /// finds it as a decision and not as a mystery.
    #[test]
    fn a_pre_0418_double_header_payload_does_not_decode_as_itself() {
        let body = Body {
            first: 5,
            rest: [6, 7],
        };
        let mut inner = [0u8; 64];
        let n = produce_headerless(&body, &mut inner);
        let mut withhdr = [0u8; 68];
        withhdr[..4].copy_from_slice(&LE_ENCAP);
        withhdr[4..4 + n].copy_from_slice(&inner[..n]);

        let got = read_action_field::<Body, 68>(&LE_ENCAP, &withhdr[..4 + n]);
        assert_ne!(
            got,
            Some(body),
            "a pre-0418 payload must NOT silently appear to decode — the wire \
             format changed and the skew is expected to fail"
        );
    }
}

/// Issue #505 — timer overrun policy. `timer_try_process` takes the
/// elapsed delta directly, so a stall is expressible as a single large
/// delta and every case here is a pure unit test.
#[cfg(test)]
mod timer_overrun_tests {
    use super::*;

    /// Drive `entry` through one `try_process` pass, crediting `delta_ms`
    /// worth of elapsed time (the dispatcher's unit is microseconds).
    fn step<F: FnMut()>(entry: &mut TimerEntry<F>, delta_ms: u64) -> bool {
        // SAFETY: `entry` is a live, aligned `TimerEntry<F>`.
        unsafe {
            timer_try_process::<F>((entry as *mut TimerEntry<F>).cast::<u8>(), delta_ms * 1000)
                .unwrap()
        }
    }

    /// Same, in microseconds — the dispatcher's native unit.
    fn step_us<F: FnMut()>(entry: &mut TimerEntry<F>, delta_us: u64) -> bool {
        // SAFETY: `entry` is a live, aligned `TimerEntry<F>`.
        unsafe {
            timer_try_process::<F>((entry as *mut TimerEntry<F>).cast::<u8>(), delta_us).unwrap()
        }
    }

    fn periodic(period_us: u64, policy: TimerOverrunPolicy) -> TimerEntry<impl FnMut()> {
        TimerEntry {
            period_us,
            elapsed_us: 0,
            overruns: 0,
            overruns_reported: 0,
            oneshot: false,
            fired: false,
            cancelled: false,
            overrun_policy: policy,
            callback: || {},
        }
    }

    #[test]
    fn skip_is_the_default_policy() {
        assert_eq!(TimerOverrunPolicy::default(), TimerOverrunPolicy::Skip);
    }

    #[test]
    fn skip_coalesces_a_stall_into_one_activation() {
        // The observed failure: a ~200 ms preemption of a 10 ms timer
        // replayed as a burst of activations. Under Skip the backlog
        // costs exactly one activation plus a counter bump.
        let mut t = periodic(10_000, TimerOverrunPolicy::Skip);
        assert!(step(&mut t, 205));
        assert_eq!(t.overruns, 19, "205 ms of a 10 ms period = 20 due, 1 fired");
        // Nothing is owed: the next pass waits out the remaining period.
        assert!(!step(&mut t, 4));
        assert!(step(&mut t, 1));
    }

    #[test]
    fn skip_preserves_the_phase_grid() {
        // 205 ms leaves a 5 ms remainder; keeping it means the next fire
        // lands 5 ms later, back on the original 10 ms grid, instead of
        // re-anchoring to the moment the stall ended.
        let mut t = periodic(10_000, TimerOverrunPolicy::Skip);
        assert!(step(&mut t, 205));
        assert_eq!(t.elapsed_us, 5_000);
    }

    #[test]
    fn catchup_replays_every_missed_period() {
        let mut t = periodic(10_000, TimerOverrunPolicy::CatchUp);
        assert!(step(&mut t, 205));
        // One activation per pass until the backlog drains, with no
        // further time credited.
        let mut replays = 0;
        while step(&mut t, 0) {
            replays += 1;
            assert!(replays < 64, "backlog must drain");
        }
        assert_eq!(replays, 19);
        assert_eq!(t.overruns, 0, "CatchUp loses nothing, so counts nothing");
    }

    #[test]
    fn on_time_ticks_never_count_as_overruns() {
        let mut t = periodic(10_000, TimerOverrunPolicy::Skip);
        for _ in 0..100 {
            assert!(step(&mut t, 10));
        }
        assert_eq!(t.overruns, 0);
        assert_eq!(t.elapsed_us, 0);
    }

    #[test]
    fn a_single_late_period_is_one_overrun() {
        // Exactly two periods due = one fired, one dropped.
        let mut t = periodic(10_000, TimerOverrunPolicy::Skip);
        assert!(step(&mut t, 20));
        assert_eq!(t.overruns, 1);
    }

    #[test]
    fn oneshot_ignores_the_policy() {
        let mut t = TimerEntry {
            period_us: 10_000,
            elapsed_us: 0,
            overruns: 0,
            overruns_reported: 0,
            oneshot: true,
            fired: false,
            cancelled: false,
            overrun_policy: TimerOverrunPolicy::Skip,
            callback: || {},
        };
        assert!(step(&mut t, 205));
        assert_eq!(t.overruns, 0);
        assert!(!step(&mut t, 205), "a fired one-shot stays fired");
    }

    #[test]
    fn sub_millisecond_periods_are_expressible() {
        // Issue #505 — the dispatcher's unit is microseconds, so a
        // 500 us period is a real period rather than the "fires every
        // spin" degenerate case a millisecond field forced it into.
        let mut t = periodic(500, TimerOverrunPolicy::Skip);
        // 200 us of credit is not enough...
        assert!(!step_us(&mut t, 200));
        // ...600 us total is.
        assert!(step_us(&mut t, 400));
        assert_eq!(t.overruns, 0);
        assert_eq!(t.elapsed_us, 100, "phase remainder survives");
    }

    #[test]
    fn a_stalled_sub_millisecond_timer_counts_whole_periods() {
        let mut t = periodic(500, TimerOverrunPolicy::Skip);
        assert!(step_us(&mut t, 5_000)); // 10 periods due
        assert_eq!(t.overruns, 9);
    }

    #[test]
    fn zero_period_does_not_divide_by_zero() {
        // Degenerate but reachable: `TimerDuration::from_millis(0)` or a
        // sub-millisecond period truncated to 0 by `as_millis`.
        let mut t = periodic(0, TimerOverrunPolicy::Skip);
        assert!(step(&mut t, 5));
        assert_eq!(t.overruns, 0);
        assert_eq!(t.elapsed_us, 0);
    }

    #[test]
    fn overrun_count_saturates_instead_of_wrapping() {
        let mut t = periodic(1_000, TimerOverrunPolicy::Skip);
        t.overruns = u32::MAX - 1;
        assert!(step(&mut t, 1_000));
        assert_eq!(t.overruns, u32::MAX);
    }
}

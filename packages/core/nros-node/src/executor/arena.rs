//! Callback arena infrastructure (all pub(crate)).

use core::marker::PhantomData;

use nros_core::MessageInfo;
use nros_core::{CdrReader, RosAction, RosMessage, RosService};
use nros_rmw::{ServiceServerTrait, Subscriber, TransportError};

use super::action_core::ActionServerCore;
use super::handles::{ActionServer, ActiveGoal};
use super::spsc_ring::SpscRing;
use super::triple_buffer::TripleBuffer;
use super::types::{
    InvocationMode, NodeError, RawCancelCallback, RawGoalCallback, RawServiceCallback,
    RawSubscriptionCallback,
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
    Timer,
    ActionServer,
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
    /// The `u64` parameter is `delta_ms` (used by timer entries, ignored by others).
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

/// Concrete subscription entry stored in the arena.
#[repr(C)]
pub(crate) struct SubEntry<M, F, const RX_BUF: usize> {
    pub(crate) handle: session::RmwSubscriber,
    pub(crate) buffer: [u8; RX_BUF],
    /// Length of pre-sampled LET data (0 = not sampled).
    pub(crate) sampled_len: usize,
    pub(crate) callback: F,
    pub(crate) _phantom: PhantomData<M>,
}

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

/// Concrete timer entry stored in the arena.
///
/// The first fields (up to `callback`) share layout with [`TimerHeader`],
/// enabling type-erased access to timer state (cancel, reset, period query).
#[repr(C)]
pub(crate) struct TimerEntry<F> {
    pub(crate) period_ms: u64,
    pub(crate) elapsed_ms: u64,
    pub(crate) oneshot: bool,
    pub(crate) fired: bool,
    pub(crate) cancelled: bool,
    pub(crate) callback: F,
}

/// Type-erased header for timer entries.
///
/// Shares layout with the initial fields of `TimerEntry<F>` (both `#[repr(C)]`),
/// so a `*mut TimerHeader` can safely read/write the timer state fields
/// regardless of the concrete closure type `F`.
#[repr(C)]
pub(crate) struct TimerHeader {
    pub(crate) period_ms: u64,
    pub(crate) elapsed_ms: u64,
    pub(crate) oneshot: bool,
    pub(crate) fired: bool,
    pub(crate) cancelled: bool,
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
    pub(crate) context: *mut core::ffi::c_void,
}

/// Concrete subscription entry for raw (untyped) callbacks.
#[repr(C)]
pub(crate) struct SubRawEntry<const RX_BUF: usize> {
    pub(crate) handle: session::RmwSubscriber,
    pub(crate) buffer: [u8; RX_BUF],
    /// Length of pre-sampled LET data (0 = not sampled).
    pub(crate) sampled_len: usize,
    pub(crate) callback: RawSubscriptionCallback,
    pub(crate) context: *mut core::ffi::c_void,
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
/// Unlike [`SubEntry`] which embeds a fixed `[u8; RX_BUF]`, this entry
/// stores a [`BufferStrategy`] that manages a trailing buffer region
/// allocated from the arena at registration time.
///
/// # Arena layout
///
/// ```text
/// [SubBufferedEntry<M, F> struct][trailing: slot_count × slot_size bytes]
///  ↑ offset                      ↑ buffer managed by BufferStrategy
/// ```
#[repr(C)]
pub(crate) struct SubBufferedEntry<M, F> {
    pub(crate) handle: session::RmwSubscriber,
    pub(crate) buffer: BufferStrategy,
    pub(crate) callback: F,
    pub(crate) _phantom: PhantomData<M>,
}

/// Drain the RMW subscriber handle into the buffer strategy.
///
/// Calls `try_recv_raw()` on the subscriber handle and writes received data
/// into the triple buffer's write slot or the SPSC ring's next push slot.
/// This bridges the existing `SUBSCRIBER_BUFFERS`-based RMW path with the
/// new arena-based buffer strategy.
///
/// # Safety
/// `entry` must be a valid mutable reference to a `SubBufferedEntry`.
unsafe fn drain_into_buffer<M, F>(entry: &mut SubBufferedEntry<M, F>) {
    match &entry.buffer {
        BufferStrategy::Triple(tb) => {
            let slot = tb.write_slot();
            if let Ok(Some(len)) = entry.handle.try_recv_raw(slot) {
                tb.writer_publish(len);
            }
        }
        BufferStrategy::Ring(ring) => {
            // Drain all pending messages into ring slots
            while let Some(slot) = ring.try_push() {
                if let Ok(Some(len)) = entry.handle.try_recv_raw(slot) {
                    ring.commit_push(len);
                } else {
                    break; // no more data
                }
            }
        }
    }
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
    _delta_ms: u64,
) -> Result<bool, TransportError>
where
    M: RosMessage,
    F: FnMut(&M),
{
    let entry = unsafe { &mut *(ptr as *mut SubBufferedEntry<M, F>) };

    // Phase 1: drain RMW subscriber → buffer strategy
    unsafe { drain_into_buffer(entry) };

    // Phase 2: dispatch from buffer → user callback
    match &entry.buffer {
        BufferStrategy::Triple(tb) => match tb.reader_acquire() {
            Some((data, len)) => {
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
/// Checks the RMW subscriber handle first (data in SUBSCRIBER_BUFFERS),
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
// Dispatch functions
// ============================================================================

/// Monomorphized subscription dispatch function.
///
/// # Safety
/// `ptr` must point to a valid, aligned `SubEntry<M, F, RX_BUF>`.
pub(crate) unsafe fn sub_try_process<M, F, const RX_BUF: usize>(
    ptr: *mut u8,
    _delta_ms: u64,
) -> Result<bool, TransportError>
where
    M: RosMessage,
    F: FnMut(&M),
{
    let entry = unsafe { &mut *(ptr as *mut SubEntry<M, F, RX_BUF>) };

    // LET mode: use pre-sampled data if available
    let recv_len = if entry.sampled_len > 0 {
        let len = entry.sampled_len;
        entry.sampled_len = 0;
        Some(len)
    } else {
        match entry.handle.try_recv_raw(&mut entry.buffer) {
            Ok(v) => v,
            Err(_) => return Err(TransportError::DeserializationError),
        }
    };

    match recv_len {
        Some(len) => {
            let mut reader = CdrReader::new_with_header(&entry.buffer[..len])
                .map_err(|_| TransportError::DeserializationError)?;
            let msg =
                M::deserialize(&mut reader).map_err(|_| TransportError::DeserializationError)?;
            (entry.callback)(&msg);
            Ok(true)
        }
        None => Ok(false),
    }
}

/// Monomorphized subscription dispatch function (with MessageInfo).
///
/// # Safety
/// `ptr` must point to a valid, aligned `SubInfoEntry<M, F, RX_BUF>`.
pub(crate) unsafe fn sub_info_try_process<M, F, const RX_BUF: usize>(
    ptr: *mut u8,
    _delta_ms: u64,
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
    _delta_ms: u64,
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
    _delta_ms: u64,
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
    delta_ms: u64,
) -> Result<bool, TransportError>
where
    F: FnMut(),
{
    let entry = unsafe { &mut *(ptr as *mut TimerEntry<F>) };

    // Cancelled or one-shot already fired
    if entry.cancelled || (entry.oneshot && entry.fired) {
        return Ok(false);
    }

    entry.elapsed_ms = entry.elapsed_ms.saturating_add(delta_ms);

    if entry.elapsed_ms >= entry.period_ms {
        (entry.callback)();
        if entry.oneshot {
            entry.fired = true;
        } else {
            entry.elapsed_ms = entry.elapsed_ms.saturating_sub(entry.period_ms);
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
    _delta_ms: u64,
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
    _delta_ms: u64,
) -> Result<bool, TransportError> {
    let entry = unsafe {
        &mut *(ptr as *mut ActionServerRawArenaEntry<GOAL_BUF, RESULT_BUF, FEEDBACK_BUF, MAX_GOALS>)
    };
    let ActionServerRawArenaEntry {
        core,
        goal_callback,
        cancel_callback,
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
            let _ = core.accept_goal(raw_req.goal_id, raw_req.sequence_number);
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

/// Monomorphized raw subscription dispatch function.
///
/// # Safety
/// `ptr` must point to a valid, aligned `SubRawEntry<RX_BUF>`.
pub(crate) unsafe fn sub_raw_try_process<const RX_BUF: usize>(
    ptr: *mut u8,
    _delta_ms: u64,
) -> Result<bool, TransportError> {
    let entry = unsafe { &mut *(ptr as *mut SubRawEntry<RX_BUF>) };

    // LET mode: use pre-sampled data if available
    let recv_len = if entry.sampled_len > 0 {
        let len = entry.sampled_len;
        entry.sampled_len = 0;
        Some(len)
    } else {
        match entry.handle.try_recv_raw(&mut entry.buffer) {
            Ok(v) => v,
            Err(_) => return Err(TransportError::DeserializationError),
        }
    };

    match recv_len {
        Some(len) => {
            unsafe {
                (entry.callback)(entry.buffer.as_ptr(), len, entry.context);
            }
            Ok(true)
        }
        None => Ok(false),
    }
}

/// Monomorphized raw service dispatch function.
///
/// # Safety
/// `ptr` must point to a valid, aligned `SrvRawEntry<REQ_BUF, REPLY_BUF>`.
pub(crate) unsafe fn srv_raw_try_process<const REQ_BUF: usize, const REPLY_BUF: usize>(
    ptr: *mut u8,
    _delta_ms: u64,
) -> Result<bool, TransportError> {
    let entry = unsafe { &mut *(ptr as *mut SrvRawEntry<REQ_BUF, REPLY_BUF>) };
    let SrvRawEntry {
        handle,
        req_buffer,
        reply_buffer,
        callback,
        context,
    } = entry;
    let (data_len, seq_num) = match handle.try_recv_request(req_buffer) {
        Ok(Some(request)) => {
            let len = request.data.len();
            let seq = request.sequence_number;
            (len, seq)
        }
        Ok(None) => return Ok(false),
        Err(_) => return Err(TransportError::ServiceReplyFailed),
    };

    let mut resp_len: usize = 0;
    let ok = unsafe {
        (*callback)(
            req_buffer.as_ptr(),
            data_len,
            reply_buffer.as_mut_ptr(),
            REPLY_BUF,
            &mut resp_len,
            *context,
        )
    };
    if ok && resp_len > 0 {
        handle
            .send_reply(seq_num, &reply_buffer[..resp_len])
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
    _delta_ms: u64,
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

/// Subscription readiness: check `has_data()` on the subscriber handle.
///
/// # Safety
/// `ptr` must point to a valid `SubEntry<M, F, RX_BUF>`.
pub(crate) unsafe fn sub_has_data<M, F, const RX_BUF: usize>(ptr: *const u8) -> bool {
    let entry = unsafe { &*(ptr as *const SubEntry<M, F, RX_BUF>) };
    entry.handle.has_data()
}

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

/// Raw subscription readiness.
///
/// # Safety
/// `ptr` must point to a valid `SubRawEntry<RX_BUF>`.
pub(crate) unsafe fn sub_raw_has_data<const RX_BUF: usize>(ptr: *const u8) -> bool {
    let entry = unsafe { &*(ptr as *const SubRawEntry<RX_BUF>) };
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

/// Pre-sample a typed subscription for LET mode.
///
/// Reads data from the transport into the entry's buffer and stores the
/// length in `sampled_len`. The callback is NOT invoked.
///
/// # Safety
/// `ptr` must point to a valid, aligned `SubEntry<M, F, RX_BUF>`.
pub(crate) unsafe fn sub_pre_sample<M, F, const RX_BUF: usize>(ptr: *mut u8) {
    let entry = unsafe { &mut *(ptr as *mut SubEntry<M, F, RX_BUF>) };
    entry.sampled_len = match entry.handle.try_recv_raw(&mut entry.buffer) {
        Ok(Some(len)) => len,
        _ => 0,
    };
}

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

/// Pre-sample a raw subscription for LET mode.
///
/// # Safety
/// `ptr` must point to a valid, aligned `SubRawEntry<RX_BUF>`.
pub(crate) unsafe fn sub_raw_pre_sample<const RX_BUF: usize>(ptr: *mut u8) {
    let entry = unsafe { &mut *(ptr as *mut SubRawEntry<RX_BUF>) };
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
) where
    A: RosAction,
{
    let entry =
        unsafe { &mut *(ptr as *mut ActionServerArenaEntry<A, GoalF, CancelF, GB, RB, FB, MG>) };
    entry.server.complete_goal(goal_id, status, result);
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
) {
    let entry = unsafe { &mut *(ptr as *mut ActionServerRawArenaEntry<GB, RB, FB, MG>) };
    let result_cdr = unsafe { core::slice::from_raw_parts(result_data, result_len) };
    entry.core.complete_goal_raw(goal_id, status, result_cdr);
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

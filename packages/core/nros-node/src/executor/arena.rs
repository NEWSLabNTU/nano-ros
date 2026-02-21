//! Callback arena infrastructure (all pub(crate)).

use core::marker::PhantomData;

use nros_core::MessageInfo;
use nros_core::{CdrReader, RosAction, RosMessage, RosService};
use nros_rmw::{Publisher, ServiceServerTrait, Subscriber, TransportError};

use super::handles::{EmbeddedActionClient, EmbeddedActionServer};
use super::types::{InvocationMode, NodeError, RawServiceCallback, RawSubscriptionCallback};

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
pub(crate) struct SubEntry<M, Sub, F, const RX_BUF: usize> {
    pub(crate) handle: Sub,
    pub(crate) buffer: [u8; RX_BUF],
    /// Length of pre-sampled LET data (0 = not sampled).
    pub(crate) sampled_len: usize,
    pub(crate) callback: F,
    pub(crate) _phantom: PhantomData<M>,
}

/// Concrete subscription entry stored in the arena (with MessageInfo).
#[repr(C)]
pub(crate) struct SubInfoEntry<M, Sub, F, const RX_BUF: usize> {
    pub(crate) handle: Sub,
    pub(crate) buffer: [u8; RX_BUF],
    /// Length of pre-sampled LET data (0 = not sampled).
    pub(crate) sampled_len: usize,
    pub(crate) callback: F,
    pub(crate) _phantom: PhantomData<M>,
}

/// Concrete subscription entry stored in the arena (with safety validation).
#[cfg(feature = "safety-e2e")]
#[repr(C)]
pub(crate) struct SubSafetyEntry<M, Sub, F, const RX_BUF: usize> {
    pub(crate) handle: Sub,
    pub(crate) buffer: [u8; RX_BUF],
    /// Length of pre-sampled LET data (0 = not sampled).
    pub(crate) sampled_len: usize,
    pub(crate) callback: F,
    pub(crate) _phantom: PhantomData<M>,
}

/// Concrete service entry stored in the arena.
#[repr(C)]
pub(crate) struct SrvEntry<Svc: RosService, Srv, F, const REQ_BUF: usize, const REPLY_BUF: usize> {
    pub(crate) handle: Srv,
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
    Srv,
    Pub,
    GoalF,
    CancelF,
    const GOAL_BUF: usize,
    const RESULT_BUF: usize,
    const FEEDBACK_BUF: usize,
    const MAX_GOALS: usize,
> {
    pub(crate) server:
        EmbeddedActionServer<A, Srv, Pub, GOAL_BUF, RESULT_BUF, FEEDBACK_BUF, MAX_GOALS>,
    pub(crate) goal_callback: GoalF,
    pub(crate) cancel_callback: CancelF,
}

/// Concrete action client entry stored in the arena.
#[repr(C)]
pub(crate) struct ActionClientArenaEntry<
    A: RosAction,
    Cli,
    Sub,
    FeedbackF,
    const GOAL_BUF: usize,
    const RESULT_BUF: usize,
    const FEEDBACK_BUF: usize,
> {
    pub(crate) client: EmbeddedActionClient<A, Cli, Sub, GOAL_BUF, RESULT_BUF, FEEDBACK_BUF>,
    pub(crate) feedback_callback: FeedbackF,
}

/// Concrete subscription entry for raw (untyped) callbacks.
#[repr(C)]
pub(crate) struct SubRawEntry<Sub, const RX_BUF: usize> {
    pub(crate) handle: Sub,
    pub(crate) buffer: [u8; RX_BUF],
    /// Length of pre-sampled LET data (0 = not sampled).
    pub(crate) sampled_len: usize,
    pub(crate) callback: RawSubscriptionCallback,
    pub(crate) context: *mut core::ffi::c_void,
}

/// Concrete service entry for raw (untyped) callbacks.
#[repr(C)]
pub(crate) struct SrvRawEntry<Srv, const REQ_BUF: usize, const REPLY_BUF: usize> {
    pub(crate) handle: Srv,
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
// Dispatch functions
// ============================================================================

/// Monomorphized subscription dispatch function.
///
/// # Safety
/// `ptr` must point to a valid, aligned `SubEntry<M, Sub, F, RX_BUF>`.
pub(crate) unsafe fn sub_try_process<M, Sub, F, const RX_BUF: usize>(
    ptr: *mut u8,
    _delta_ms: u64,
) -> Result<bool, TransportError>
where
    M: RosMessage,
    Sub: Subscriber,
    F: FnMut(&M),
{
    let entry = unsafe { &mut *(ptr as *mut SubEntry<M, Sub, F, RX_BUF>) };

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
/// `ptr` must point to a valid, aligned `SubInfoEntry<M, Sub, F, RX_BUF>`.
pub(crate) unsafe fn sub_info_try_process<M, Sub, F, const RX_BUF: usize>(
    ptr: *mut u8,
    _delta_ms: u64,
) -> Result<bool, TransportError>
where
    M: RosMessage,
    Sub: Subscriber,
    F: FnMut(&M, Option<&MessageInfo>),
{
    let entry = unsafe { &mut *(ptr as *mut SubInfoEntry<M, Sub, F, RX_BUF>) };

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
/// `ptr` must point to a valid, aligned `SubSafetyEntry<M, Sub, F, RX_BUF>`.
#[cfg(feature = "safety-e2e")]
pub(crate) unsafe fn sub_safety_try_process<M, Sub, F, const RX_BUF: usize>(
    ptr: *mut u8,
    _delta_ms: u64,
) -> Result<bool, TransportError>
where
    M: RosMessage,
    Sub: Subscriber,
    F: FnMut(&M, &nros_rmw::IntegrityStatus),
{
    let entry = unsafe { &mut *(ptr as *mut SubSafetyEntry<M, Sub, F, RX_BUF>) };

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
/// `ptr` must point to a valid, aligned `SrvEntry<Svc, Srv, F, REQ_BUF, REPLY_BUF>`.
pub(crate) unsafe fn srv_try_process<Svc, Srv, F, const REQ_BUF: usize, const REPLY_BUF: usize>(
    ptr: *mut u8,
    _delta_ms: u64,
) -> Result<bool, TransportError>
where
    Svc: RosService,
    Srv: ServiceServerTrait,
    F: FnMut(&Svc::Request) -> Svc::Reply,
    Srv::Error: From<TransportError>,
{
    let entry = unsafe { &mut *(ptr as *mut SrvEntry<Svc, Srv, F, REQ_BUF, REPLY_BUF>) };
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
    Srv,
    Pub,
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
    Srv: ServiceServerTrait,
    Pub: Publisher,
    GoalF: FnMut(&A::Goal) -> nros_core::GoalResponse,
    CancelF: FnMut(&nros_core::GoalId, nros_core::GoalStatus) -> nros_core::CancelResponse,
{
    let entry = unsafe {
        &mut *(ptr as *mut ActionServerArenaEntry<
            A,
            Srv,
            Pub,
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
    if matches!(server.try_accept_goal(|g| (goal_callback)(g)), Ok(Some(_))) {
        did_work = true;
    }

    // Handle result requests
    if matches!(server.try_handle_get_result(), Ok(Some(_))) {
        did_work = true;
    }

    Ok(did_work)
}

/// Monomorphized action client dispatch function.
///
/// Polls feedback from the action server.
///
/// # Safety
/// `ptr` must point to a valid, aligned `ActionClientArenaEntry<...>`.
pub(crate) unsafe fn action_client_try_process<
    A,
    Cli,
    Sub,
    FeedbackF,
    const GOAL_BUF: usize,
    const RESULT_BUF: usize,
    const FEEDBACK_BUF: usize,
>(
    ptr: *mut u8,
    _delta_ms: u64,
) -> Result<bool, TransportError>
where
    A: RosAction,
    Cli: ServiceClientTrait,
    Sub: Subscriber,
    FeedbackF: FnMut(&nros_core::GoalId, &A::Feedback),
{
    let entry = unsafe {
        &mut *(ptr as *mut ActionClientArenaEntry<
            A,
            Cli,
            Sub,
            FeedbackF,
            GOAL_BUF,
            RESULT_BUF,
            FEEDBACK_BUF,
        >)
    };
    let ActionClientArenaEntry {
        client,
        feedback_callback,
    } = entry;

    match client.try_recv_feedback() {
        Ok(Some((goal_id, feedback))) => {
            (feedback_callback)(&goal_id, &feedback);
            Ok(true)
        }
        Ok(None) => Ok(false),
        Err(_) => Err(TransportError::DeserializationError),
    }
}

/// Monomorphized raw subscription dispatch function.
///
/// # Safety
/// `ptr` must point to a valid, aligned `SubRawEntry<Sub, RX_BUF>`.
pub(crate) unsafe fn sub_raw_try_process<Sub, const RX_BUF: usize>(
    ptr: *mut u8,
    _delta_ms: u64,
) -> Result<bool, TransportError>
where
    Sub: Subscriber,
{
    let entry = unsafe { &mut *(ptr as *mut SubRawEntry<Sub, RX_BUF>) };

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
/// `ptr` must point to a valid, aligned `SrvRawEntry<Srv, REQ_BUF, REPLY_BUF>`.
pub(crate) unsafe fn srv_raw_try_process<Srv, const REQ_BUF: usize, const REPLY_BUF: usize>(
    ptr: *mut u8,
    _delta_ms: u64,
) -> Result<bool, TransportError>
where
    Srv: ServiceServerTrait,
    Srv::Error: From<TransportError>,
{
    let entry = unsafe { &mut *(ptr as *mut SrvRawEntry<Srv, REQ_BUF, REPLY_BUF>) };
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
/// `ptr` must point to a valid `SubEntry<M, Sub, F, RX_BUF>`.
pub(crate) unsafe fn sub_has_data<M, Sub, F, const RX_BUF: usize>(ptr: *const u8) -> bool
where
    Sub: Subscriber,
{
    let entry = unsafe { &*(ptr as *const SubEntry<M, Sub, F, RX_BUF>) };
    entry.handle.has_data()
}

/// SubInfoEntry readiness.
///
/// # Safety
/// `ptr` must point to a valid `SubInfoEntry<M, Sub, F, RX_BUF>`.
pub(crate) unsafe fn sub_info_has_data<M, Sub, F, const RX_BUF: usize>(ptr: *const u8) -> bool
where
    Sub: Subscriber,
{
    let entry = unsafe { &*(ptr as *const SubInfoEntry<M, Sub, F, RX_BUF>) };
    entry.handle.has_data()
}

/// SubSafetyEntry readiness.
///
/// # Safety
/// `ptr` must point to a valid `SubSafetyEntry<M, Sub, F, RX_BUF>`.
#[cfg(feature = "safety-e2e")]
pub(crate) unsafe fn sub_safety_has_data<M, Sub, F, const RX_BUF: usize>(ptr: *const u8) -> bool
where
    Sub: Subscriber,
{
    let entry = unsafe { &*(ptr as *const SubSafetyEntry<M, Sub, F, RX_BUF>) };
    entry.handle.has_data()
}

/// Raw subscription readiness.
///
/// # Safety
/// `ptr` must point to a valid `SubRawEntry<Sub, RX_BUF>`.
pub(crate) unsafe fn sub_raw_has_data<Sub, const RX_BUF: usize>(ptr: *const u8) -> bool
where
    Sub: Subscriber,
{
    let entry = unsafe { &*(ptr as *const SubRawEntry<Sub, RX_BUF>) };
    entry.handle.has_data()
}

/// Service readiness: check `has_request()` on the service handle.
///
/// # Safety
/// `ptr` must point to a valid `SrvEntry<Svc, Srv, F, RQ, RP>`.
pub(crate) unsafe fn srv_has_data<Svc: RosService, Srv, F, const RQ: usize, const RP: usize>(
    ptr: *const u8,
) -> bool
where
    Srv: ServiceServerTrait,
{
    let entry = unsafe { &*(ptr as *const SrvEntry<Svc, Srv, F, RQ, RP>) };
    entry.handle.has_request()
}

/// Raw service readiness.
///
/// # Safety
/// `ptr` must point to a valid `SrvRawEntry<Srv, RQ, RP>`.
pub(crate) unsafe fn srv_raw_has_data<Srv, const RQ: usize, const RP: usize>(ptr: *const u8) -> bool
where
    Srv: ServiceServerTrait,
{
    let entry = unsafe { &*(ptr as *const SrvRawEntry<Srv, RQ, RP>) };
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
/// `ptr` must point to a valid, aligned `SubEntry<M, Sub, F, RX_BUF>`.
pub(crate) unsafe fn sub_pre_sample<M, Sub, F, const RX_BUF: usize>(ptr: *mut u8)
where
    Sub: Subscriber,
{
    let entry = unsafe { &mut *(ptr as *mut SubEntry<M, Sub, F, RX_BUF>) };
    entry.sampled_len = match entry.handle.try_recv_raw(&mut entry.buffer) {
        Ok(Some(len)) => len,
        _ => 0,
    };
}

/// Pre-sample a typed subscription with MessageInfo for LET mode.
///
/// # Safety
/// `ptr` must point to a valid, aligned `SubInfoEntry<M, Sub, F, RX_BUF>`.
pub(crate) unsafe fn sub_info_pre_sample<M, Sub, F, const RX_BUF: usize>(ptr: *mut u8)
where
    Sub: Subscriber,
{
    let entry = unsafe { &mut *(ptr as *mut SubInfoEntry<M, Sub, F, RX_BUF>) };
    // For LET, we sample only the data (MessageInfo is not preserved in the snapshot)
    entry.sampled_len = match entry.handle.try_recv_raw(&mut entry.buffer) {
        Ok(Some(len)) => len,
        _ => 0,
    };
}

/// Pre-sample a safety subscription for LET mode.
///
/// # Safety
/// `ptr` must point to a valid, aligned `SubSafetyEntry<M, Sub, F, RX_BUF>`.
#[cfg(feature = "safety-e2e")]
pub(crate) unsafe fn sub_safety_pre_sample<M, Sub, F, const RX_BUF: usize>(ptr: *mut u8)
where
    Sub: Subscriber,
{
    let entry = unsafe { &mut *(ptr as *mut SubSafetyEntry<M, Sub, F, RX_BUF>) };
    entry.sampled_len = match entry.handle.try_recv_raw(&mut entry.buffer) {
        Ok(Some(len)) => len,
        _ => 0,
    };
}

/// Pre-sample a raw subscription for LET mode.
///
/// # Safety
/// `ptr` must point to a valid, aligned `SubRawEntry<Sub, RX_BUF>`.
pub(crate) unsafe fn sub_raw_pre_sample<Sub, const RX_BUF: usize>(ptr: *mut u8)
where
    Sub: Subscriber,
{
    let entry = unsafe { &mut *(ptr as *mut SubRawEntry<Sub, RX_BUF>) };
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
    Srv,
    Pub,
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
    Srv: ServiceServerTrait,
    Pub: Publisher,
{
    let entry = unsafe {
        &mut *(ptr as *mut ActionServerArenaEntry<A, Srv, Pub, GoalF, CancelF, GB, RB, FB, MG>)
    };
    entry.server.publish_feedback(goal_id, feedback)
}

/// Action server: complete a goal via arena entry.
///
/// # Safety
/// `ptr` must point to a valid `ActionServerArenaEntry`.
pub(crate) unsafe fn as_complete_goal<
    A,
    Srv,
    Pub,
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
    Srv: ServiceServerTrait,
    Pub: Publisher,
{
    let entry = unsafe {
        &mut *(ptr as *mut ActionServerArenaEntry<A, Srv, Pub, GoalF, CancelF, GB, RB, FB, MG>)
    };
    entry.server.complete_goal(goal_id, status, result);
}

/// Action server: set goal status via arena entry.
///
/// # Safety
/// `ptr` must point to a valid `ActionServerArenaEntry`.
pub(crate) unsafe fn as_set_goal_status<
    A,
    Srv,
    Pub,
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
    Srv: ServiceServerTrait,
    Pub: Publisher,
{
    let entry = unsafe {
        &mut *(ptr as *mut ActionServerArenaEntry<A, Srv, Pub, GoalF, CancelF, GB, RB, FB, MG>)
    };
    entry.server.set_goal_status(goal_id, status);
}

/// Action client: send goal via arena entry.
///
/// # Safety
/// `ptr` must point to a valid `ActionClientArenaEntry`.
pub(crate) unsafe fn ac_send_goal<
    A,
    Cli,
    Sub,
    FeedbackF,
    const GB: usize,
    const RB: usize,
    const FB: usize,
>(
    ptr: *mut u8,
    goal: &A::Goal,
) -> Result<nros_core::GoalId, NodeError>
where
    A: RosAction,
    Cli: ServiceClientTrait,
    Sub: Subscriber,
{
    let entry =
        unsafe { &mut *(ptr as *mut ActionClientArenaEntry<A, Cli, Sub, FeedbackF, GB, RB, FB>) };
    entry.client.send_goal_blocking(goal)
}

/// Action client: cancel goal via arena entry.
///
/// # Safety
/// `ptr` must point to a valid `ActionClientArenaEntry`.
pub(crate) unsafe fn ac_cancel_goal<
    A,
    Cli,
    Sub,
    FeedbackF,
    const GB: usize,
    const RB: usize,
    const FB: usize,
>(
    ptr: *mut u8,
    goal_id: &nros_core::GoalId,
) -> Result<nros_core::CancelResponse, NodeError>
where
    A: RosAction,
    Cli: ServiceClientTrait,
    Sub: Subscriber,
{
    let entry =
        unsafe { &mut *(ptr as *mut ActionClientArenaEntry<A, Cli, Sub, FeedbackF, GB, RB, FB>) };
    entry.client.cancel_goal_blocking(goal_id)
}

/// Action client: get result via arena entry.
///
/// # Safety
/// `ptr` must point to a valid `ActionClientArenaEntry`.
pub(crate) unsafe fn ac_get_result<
    A,
    Cli,
    Sub,
    FeedbackF,
    const GB: usize,
    const RB: usize,
    const FB: usize,
>(
    ptr: *mut u8,
    goal_id: &nros_core::GoalId,
) -> Result<(nros_core::GoalStatus, A::Result), NodeError>
where
    A: RosAction,
    Cli: ServiceClientTrait,
    Sub: Subscriber,
{
    let entry =
        unsafe { &mut *(ptr as *mut ActionClientArenaEntry<A, Cli, Sub, FeedbackF, GB, RB, FB>) };
    entry.client.get_result_blocking(goal_id)
}

use nros_rmw::ServiceClientTrait;

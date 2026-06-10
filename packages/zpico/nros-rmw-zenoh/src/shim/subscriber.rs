//! ZenohSubscriber and ZenohZeroCopySubscriber implementations

use core::marker::PhantomData;

use atomic_waker::AtomicWaker;
use portable_atomic::{AtomicUsize, Ordering};

use nros_rmw::{Subscriber, TransportError};

use super::{
    KEYEXPR_BUFFER_SIZE, KEYEXPR_STRING_SIZE, MessageInfo, RMW_ATTACHMENT_SIZE,
    SUBSCRIBER_ATTACHMENT_BUF_SIZE, SUBSCRIBER_BUFFER_SIZE, SUBSCRIBER_RING_DEPTH,
};
use crate::{
    keyexpr::TopicKeyExpr,
    zpico::{Context, ZPICO_MAX_SUBSCRIBERS, zpico_ring_desc_t},
};

#[cfg(feature = "safety-e2e")]
use super::SAFETY_CRC_SIZE;

#[cfg(feature = "std")]
use super::signal_executor_wake;

// ============================================================================
// SubscriberBuffer
// ============================================================================

/// Shared buffer for subscriber callbacks.
///
/// Phase 124.D.3.c — SPSC ring. The C shim is the sole producer
/// (writes payload + attachment + lengths into the slot at
/// `ring_tail % SUBSCRIBER_RING_DEPTH`, then Release-stores
/// `ring_tail + 1`). The Rust shim is the sole consumer (reads the
/// slot at `ring_head % SUBSCRIBER_RING_DEPTH`, then Release-stores
/// `ring_head + 1`). Ring empty when `head == tail`, full when
/// `tail - head == SUBSCRIBER_RING_DEPTH`.
///
/// Replaces the previous single-slot + `locked` flag design — a
/// burst of up to `SUBSCRIBER_RING_DEPTH` messages arriving between
/// two `try_recv` calls is now buffered instead of dropped, and
/// `try_recv_sequence` can drain the whole ring in one call. No
/// lock is needed: the SPSC discipline + the Release/Acquire fence
/// on `ring_tail` / `ring_head` covers the cross-FFI handoff.
pub(super) struct SubscriberBuffer {
    /// Ring of payload slots.
    pub(super) ring_payload: [[u8; SUBSCRIBER_BUFFER_SIZE]; SUBSCRIBER_RING_DEPTH],
    /// Ring of attachment slots, parallel to `ring_payload`.
    pub(super) ring_att: [[u8; SUBSCRIBER_ATTACHMENT_BUF_SIZE]; SUBSCRIBER_RING_DEPTH],
    /// Per-slot payload byte length. Written by the C shim before
    /// its Release-store to `ring_tail`; read by the Rust shim
    /// after its Acquire-load.
    pub(super) ring_len: [usize; SUBSCRIBER_RING_DEPTH],
    /// Per-slot attachment byte length.
    pub(super) ring_att_len: [usize; SUBSCRIBER_RING_DEPTH],
    /// Consumer counter — advanced only by the Rust shim.
    pub(super) ring_head: AtomicUsize,
    /// Producer counter — advanced only by the C shim.
    pub(super) ring_tail: AtomicUsize,
    /// Descriptor handed to the C shim at subscribe time. The raw
    /// pointers reference this same `SubscriberBuffer` (a
    /// `static mut` element — its address is stable for the
    /// program's lifetime). Filled in `ZenohSubscriber::new`.
    pub(super) ring_desc: zpico_ring_desc_t,
    /// Async waker — registered by `Future::poll()`, woken from the
    /// notify callback when data arrives. Enables event-driven
    /// async without busy-polling.
    pub(super) waker: AtomicWaker,
}

impl SubscriberBuffer {
    pub(super) const fn new() -> Self {
        Self {
            ring_payload: [[0u8; SUBSCRIBER_BUFFER_SIZE]; SUBSCRIBER_RING_DEPTH],
            ring_att: [[0u8; SUBSCRIBER_ATTACHMENT_BUF_SIZE]; SUBSCRIBER_RING_DEPTH],
            ring_len: [0usize; SUBSCRIBER_RING_DEPTH],
            ring_att_len: [0usize; SUBSCRIBER_RING_DEPTH],
            ring_head: AtomicUsize::new(0),
            ring_tail: AtomicUsize::new(0),
            ring_desc: zpico_ring_desc_t {
                payload_base: core::ptr::null_mut(),
                payload_stride: 0,
                att_base: core::ptr::null_mut(),
                att_stride: 0,
                slot_count: 0,
                payload_len: core::ptr::null_mut(),
                att_len: core::ptr::null_mut(),
                head: core::ptr::null_mut(),
                tail: core::ptr::null_mut(),
            },
            waker: AtomicWaker::new(),
        }
    }

    /// True when the ring holds at least one un-consumed message.
    pub(super) fn has_data(&self) -> bool {
        self.ring_head.load(Ordering::Acquire) != self.ring_tail.load(Ordering::Acquire)
    }

    /// Index of the head slot if the ring is non-empty. Does NOT
    /// advance `ring_head` — the caller reads the slot, then calls
    /// [`consume_head`](Self::consume_head). The Acquire-load of
    /// `ring_tail` synchronises-with the C producer's Release-store,
    /// so the per-slot payload / attachment / length writes that
    /// happened-before that store are visible here.
    pub(super) fn peek_head_slot(&self) -> Option<usize> {
        let head = self.ring_head.load(Ordering::Acquire);
        let tail = self.ring_tail.load(Ordering::Acquire);
        if head == tail {
            None
        } else {
            Some(head % SUBSCRIBER_RING_DEPTH)
        }
    }

    /// Advance `ring_head` past the slot returned by the most recent
    /// [`peek_head_slot`](Self::peek_head_slot). Release-store so the
    /// C producer's Acquire-load of `ring_head` sees the slot freed.
    pub(super) fn consume_head(&self) {
        let head = self.ring_head.load(Ordering::Acquire);
        self.ring_head
            .store(head.wrapping_add(1), Ordering::Release);
    }

    /// Populate `ring_desc` so the C shim can produce into this
    /// buffer. Must be called once, after the buffer's static
    /// address is known (i.e. from `ZenohSubscriber::new`).
    pub(super) fn init_ring_desc(&mut self) {
        self.ring_desc = zpico_ring_desc_t {
            payload_base: self.ring_payload.as_mut_ptr() as *mut u8,
            payload_stride: SUBSCRIBER_BUFFER_SIZE,
            att_base: self.ring_att.as_mut_ptr() as *mut u8,
            att_stride: SUBSCRIBER_ATTACHMENT_BUF_SIZE,
            slot_count: SUBSCRIBER_RING_DEPTH,
            payload_len: self.ring_len.as_mut_ptr(),
            att_len: self.ring_att_len.as_mut_ptr(),
            head: self.ring_head.as_ptr(),
            tail: self.ring_tail.as_ptr(),
        };
    }
}

/// Static buffers for subscribers.
///
/// Count matches `ZPICO_MAX_SUBSCRIBERS` from zpico-sys (the C shim
/// allocates the same number of subscriber entries). We use static buffers
/// because the shim callback mechanism requires a static context pointer.
static mut SUBSCRIBER_BUFFERS: [SubscriberBuffer; ZPICO_MAX_SUBSCRIBERS] =
    [const { SubscriberBuffer::new() }; ZPICO_MAX_SUBSCRIBERS];

/// Next available buffer index
pub(super) static NEXT_BUFFER_INDEX: AtomicUsize = AtomicUsize::new(0);

// ============================================================================
// SubscriberBufferRef — safe accessor wrapper
// ============================================================================

/// Safe accessor for a statically-allocated subscriber buffer.
///
/// Encapsulates the `unsafe` access to `SUBSCRIBER_BUFFERS` by validating
/// the index once at construction time. Subsequent accesses via [`get()`]
/// are safe because the index is guaranteed in-bounds.
///
/// # Safety invariant
///
/// `SUBSCRIBER_BUFFERS` is a module-level `static mut` with a fixed address
/// and element count equal to `ZPICO_MAX_SUBSCRIBERS`. The index is validated
/// at construction and never changes, so every `get()` / `get_mut()` call
/// dereferences a valid, in-bounds element.
pub(super) struct SubscriberBufferRef {
    index: usize,
}

impl SubscriberBufferRef {
    /// Create a new buffer reference with bounds validation.
    ///
    /// # Panics
    ///
    /// Panics if `index >= ZPICO_MAX_SUBSCRIBERS`.
    pub(super) fn new(index: usize) -> Self {
        assert!(
            index < ZPICO_MAX_SUBSCRIBERS,
            "subscriber buffer index out of bounds: {index} >= {ZPICO_MAX_SUBSCRIBERS}"
        );
        Self { index }
    }

    /// Get an immutable reference to the subscriber buffer.
    ///
    /// Returns a `'static` reference — `SUBSCRIBER_BUFFERS` is a
    /// module-level `static mut` whose elements live for the
    /// program's lifetime, so the borrow is genuinely `'static` and
    /// callers don't have to keep the `SubscriberBufferRef` alive.
    ///
    /// Safety is guaranteed by the bounds check at construction time.
    /// All shared fields use atomic types, preventing data races.
    pub(super) fn get(&self) -> &'static SubscriberBuffer {
        // Safety: index was validated at construction time.
        // SUBSCRIBER_BUFFERS is a module-level static with fixed address.
        unsafe { &SUBSCRIBER_BUFFERS[self.index] }
    }

    /// Get a mutable reference to the subscriber buffer.
    ///
    /// Only called from callbacks, which are invoked synchronously
    /// (single-threaded) by zenoh-pico — no concurrent mutable access.
    pub(super) fn get_mut(&mut self) -> &mut SubscriberBuffer {
        // Safety: index was validated at construction time.
        // Mutable access is only used by callbacks invoked synchronously
        // by zenoh-pico, so there are no concurrent mutable accesses.
        unsafe { &mut SUBSCRIBER_BUFFERS[self.index] }
    }
}

/// Notify callback invoked by the C shim once per message arrival.
///
/// Phase 124.D.3.c — in ring mode the C shim has already written the
/// payload, attachment, and per-slot lengths into the next free ring
/// slot and Release-stored `ring_tail` before calling this. So the
/// callback only has to fire the async waker / executor wake — there
/// is nothing left for it to copy. The `len` / `attachment` args are
/// unused (the consumer reads them from the ring slot). On a
/// full-ring or oversized-payload drop the C shim still calls us so
/// the waker observes the arrival attempt.
extern "C" fn subscriber_notify_callback(
    len: usize,
    _attachment: *const u8,
    _attachment_len: usize,
    ctx: *mut core::ffi::c_void,
) {
    let buffer_index = ctx as usize;
    if buffer_index >= ZPICO_MAX_SUBSCRIBERS {
        return;
    }

    // Phase 160.L.2 — C shim signals an oversized-payload drop by
    // calling notify with `len > SUBSCRIBER_BUFFER_SIZE` and a NULL
    // payload (see `zpico.c:595-599`). Bump a per-subscriber counter
    // so user code can observe drops that would otherwise be silent
    // — the test harness asserts on this in
    // `test_zenoh_overflow_detection`, and it doubles as a
    // user-visible signal that the subscriber's QoS / buffer sizing
    // is wrong for the producer's payload size.
    if len > SUBSCRIBER_BUFFER_SIZE {
        OVERFLOW_DROPS.fetch_add(1, Ordering::Relaxed);
    }

    let buf_ref = SubscriberBufferRef {
        index: buffer_index,
    };
    let buffer = buf_ref.get();

    // Wake any async task waiting for data on this subscriber.
    buffer.waker.wake();

    // Wake the executor spin loop (if waiting).
    #[cfg(feature = "std")]
    signal_executor_wake();
}

/// Cumulative count of incoming samples that exceeded
/// `SUBSCRIBER_BUFFER_SIZE` and were therefore dropped by the C shim
/// before they could land in any subscriber ring. Bumped by
/// [`subscriber_notify_callback`] when the C side signals an
/// oversized-payload drop. Process-wide counter — every subscriber
/// shares the same atomic, which mirrors how the C shim drops are
/// reported (the notify callback doesn't carry a subscriber-specific
/// slot index past the `ctx` we already use for waker dispatch).
static OVERFLOW_DROPS: portable_atomic::AtomicU32 = portable_atomic::AtomicU32::new(0);

/// Read the cumulative overflow-drop count. Useful for tests that
/// want to assert on the silent-drop path; production code should
/// size the subscriber buffer up-front.
pub fn overflow_drops_total() -> u32 {
    OVERFLOW_DROPS.load(Ordering::Relaxed)
}

// ============================================================================
// ZenohSubscriber
// ============================================================================

/// Zenoh subscriber wrapping nros-rmw-zenoh ZenohSubscriber
pub struct ZenohSubscriber {
    /// The subscriber handle (kept alive to maintain subscription)
    _subscriber: crate::zpico::Subscriber<'static>,
    /// Safe accessor for the static subscriber buffer
    buf: SubscriberBufferRef,
    /// Liveliness token for ROS 2 graph discovery (kept alive for subscriber lifetime)
    _liveliness: Option<super::LivelinessToken>,
    /// E2E safety validator (tracks sequence numbers, validates CRC)
    #[cfg(feature = "safety-e2e")]
    safety_validator: nros_rmw::SafetyValidator,
    /// Phase 108.C.zenoh.5 — next expected sequence number, used to
    /// detect publisher gaps in the attachment-encoded seq stream and
    /// fire `MessageLost` events. Initialised to `0` (= "no message
    /// observed yet"); first `try_recv_raw` synchronises to the
    /// publisher's seq w/o reporting a gap.
    next_expected_seq: core::cell::Cell<i64>,
    /// Cumulative count of messages dropped between this subscriber's
    /// observed seq stream and the publisher's seq stream. Used as
    /// `CountStatus::total_count` per the nros event contract.
    msg_lost_total: core::cell::Cell<u32>,
    /// Phase 108.A — registered `MessageLost` callback slot.
    msg_lost_cb: core::cell::Cell<Option<EventReg>>,
    /// Phase 108.C.zenoh.3 — sample lifespan in ms (`0` = infinite).
    /// Captured from QoS at create time; samples whose attachment
    /// timestamp is older than `now - lifespan_ms` are dropped in
    /// `try_recv_raw` (return `Ok(None)` as if no data was present).
    lifespan_ms: u32,
    /// Phase 108.C.zenoh.2 — deadline period in ms (`0` = infinite).
    /// Captured from QoS at create time; if `now - last_msg_at_ms`
    /// exceeds it, fire `RequestedDeadlineMissed`.
    deadline_ms: u32,
    /// Last successful receive timestamp in ms (platform clock).
    /// Initialised at creation time to suppress an immediate "missed"
    /// at sub-create.
    last_msg_at_ms: core::cell::Cell<u64>,
    /// Last `RequestedDeadlineMissed` fire-time so we don't spam
    /// callbacks for a continually-late publisher; we fire at most
    /// once per deadline period.
    last_deadline_fire_ms: core::cell::Cell<u64>,
    /// Cumulative `RequestedDeadlineMissed` count, used as
    /// `CountStatus::total_count`.
    deadline_total: core::cell::Cell<u32>,
    /// Cumulative dropped-by-lifespan count (folded into
    /// `MessageLost` events — lifespan-expired samples count as lost).
    deadline_cb: core::cell::Cell<Option<EventReg>>,
    /// Phase 108.C.zenoh.4 — registered `LivelinessChanged` callback.
    /// Fired from `has_data` / `try_recv_raw` after a periodic
    /// `liveliness_get_*` poll detects an alive-state transition for
    /// any publisher matching the subscriber's wildcard liveliness
    /// keyexpr.
    #[cfg(not(feature = "platform-bare-metal"))]
    liveliness_cb: core::cell::Cell<Option<EventReg>>,
    /// Wildcard liveliness keyexpr matching any publisher on this
    /// subscriber's (topic, type). Populated at create.
    #[cfg(not(feature = "platform-bare-metal"))]
    liveliness_keyexpr: heapless::String<256>,
    /// Liveliness-poll context — handle of an in-flight
    /// `liveliness_get_start` query (None = idle), the timestamp of
    /// the most recent poll start, and the previously observed alive
    /// state.
    #[cfg(not(feature = "platform-bare-metal"))]
    liveliness_poll: core::cell::Cell<LivelinessPoll>,
    /// Raw pointer to the owning session's `Context`. Used by the
    /// LIVELINESS poll loop to issue `liveliness_get_*` queries.
    /// SAFETY: the Context is owned by `ZenohSession`, which outlives
    /// every entity it spawns (entities are created via Session and
    /// dropped before Session::close).
    #[cfg(not(feature = "platform-bare-metal"))]
    context: *const Context,
    /// Phantom to indicate we don't own the buffer
    _phantom: PhantomData<()>,
}

/// Phase 108.C.zenoh.4 — liveliness-poll state. Owned by the
/// subscriber via `Cell` since the subscriber is `!Sync`.
#[derive(Clone, Copy)]
#[cfg(not(feature = "platform-bare-metal"))]
struct LivelinessPoll {
    /// Slot handle of an in-flight `liveliness_get_start` query, or
    /// `-1` when idle.
    handle: i32,
    /// Wall-clock ms when the most recent poll was started.
    started_at_ms: u64,
    /// Last observed alive-state (any matching publisher visible).
    /// Initialised to `false`; the first transition to `true` fires
    /// `alive_count_change = +1`.
    last_alive: bool,
    /// Cumulative running count for `LivelinessChangedStatus.alive_count`.
    alive_count: u16,
}

#[cfg(not(feature = "platform-bare-metal"))]
impl LivelinessPoll {
    const IDLE: Self = Self {
        handle: -1,
        started_at_ms: 0,
        last_alive: false,
        alive_count: 0,
    };
}

/// Liveliness-poll cadence. We don't expose a knob because polling
/// faster than ~1 Hz spams the network without benefit; coarser than
/// ~5 s loses transitions. Sub side honors `liveliness_lease_ms` from
/// QoS by clamping the poll window to half the lease (so we observe
/// at least two probes per lease period).
#[cfg(not(feature = "platform-bare-metal"))]
const LIVELINESS_POLL_DEFAULT_MS: u64 = 1_000;
#[cfg(not(feature = "platform-bare-metal"))]
const LIVELINESS_POLL_TIMEOUT_MS: u32 = 100;

/// Phase 108.A — single-slot event registration. The cb is
/// `unsafe extern "C" fn` (always Send); user_ctx outlives the
/// subscriber per Phase 108.A.7's per-entity event registry.
#[derive(Clone, Copy)]
struct EventReg {
    cb: nros_rmw::EventCallback,
    user_ctx: *mut core::ffi::c_void,
}

/// Phase 108.C.zenoh — read the platform clock in ms.
///
/// Phase 129.C.3.a — call the canonical `nros_platform_*` C
/// symbol directly instead of routing through `ConcretePlatform`.
fn now_ms() -> u64 {
    unsafe extern "C" {
        fn nros_platform_time_now_ms() -> u64;
    }
    unsafe { nros_platform_time_now_ms() }
}

impl ZenohSubscriber {
    /// Create a new subscriber for the given topic
    pub fn new(
        context: &Context,
        topic: &nros_rmw::TopicInfo,
        liveliness: Option<super::LivelinessToken>,
        qos: &nros_rmw::QosSettings,
    ) -> Result<Self, TransportError> {
        // Phase 108.C.zenoh.4 — wildcard liveliness keyexpr matching
        // any publisher on this (topic, type). Built once and stored
        // for reuse on each LIVELINESS poll on hosted targets.
        #[cfg(not(feature = "platform-bare-metal"))]
        let liveliness_keyexpr: heapless::String<256> =
            super::Ros2Liveliness::publisher_keyexpr_wildcard(topic.domain_id, topic);
        // Allocate a buffer index
        let buffer_index = NEXT_BUFFER_INDEX.fetch_add(1, Ordering::SeqCst);
        if buffer_index >= ZPICO_MAX_SUBSCRIBERS {
            // Roll back and return error
            NEXT_BUFFER_INDEX.fetch_sub(1, Ordering::SeqCst);
            return Err(TransportError::SubscriberCreationFailed);
        }

        let mut buf = SubscriberBufferRef::new(buffer_index);

        // Generate the topic key with wildcard for type hash
        let key: heapless::String<KEYEXPR_STRING_SIZE> = topic.to_key_wildcard();

        #[cfg(feature = "std")]
        log::debug!("Subscriber data keyexpr: {}", key.as_str());

        // Create null-terminated keyexpr
        let mut keyexpr_buf = [0u8; KEYEXPR_BUFFER_SIZE];
        let bytes = key.as_bytes();
        if bytes.len() >= keyexpr_buf.len() {
            return Err(TransportError::TopicNameInvalid);
        }
        keyexpr_buf[..bytes.len()].copy_from_slice(bytes);
        keyexpr_buf[bytes.len()] = 0;

        // Phase 124.D.3.c — create subscriber with the SPSC ring.
        // The C shim reads each payload directly into the next free
        // ring slot of SUBSCRIBER_BUFFERS[buffer_index] via
        // `z_bytes_reader_read()`, advances `ring_tail`, and fires
        // the notify callback. A burst is buffered up to
        // SUBSCRIBER_RING_DEPTH deep instead of overwriting a single
        // slot. `init_ring_desc` populates the descriptor's raw
        // pointers from the buffer's (stable) static address.
        let subscriber = unsafe {
            let buffer = buf.get_mut();
            buffer.init_ring_desc();
            let desc_ptr: *mut zpico_ring_desc_t = &mut buffer.ring_desc;
            let sub_result = context.declare_subscriber_ring_raw(
                &keyexpr_buf,
                desc_ptr,
                subscriber_notify_callback,
                buffer_index as *mut core::ffi::c_void,
            );
            match sub_result {
                Ok(s) => core::mem::transmute::<
                    crate::zpico::Subscriber<'_>,
                    crate::zpico::Subscriber<'static>,
                >(s),
                Err(e) => {
                    NEXT_BUFFER_INDEX.fetch_sub(1, Ordering::SeqCst);
                    return Err(TransportError::from(e));
                }
            }
        };

        let now = now_ms();
        Ok(Self {
            _subscriber: subscriber,
            buf,
            _liveliness: liveliness,
            #[cfg(feature = "safety-e2e")]
            safety_validator: nros_rmw::SafetyValidator::new(),
            next_expected_seq: core::cell::Cell::new(0),
            msg_lost_total: core::cell::Cell::new(0),
            msg_lost_cb: core::cell::Cell::new(None),
            lifespan_ms: qos.lifespan_ms,
            deadline_ms: qos.deadline_ms,
            last_msg_at_ms: core::cell::Cell::new(now),
            last_deadline_fire_ms: core::cell::Cell::new(now),
            deadline_total: core::cell::Cell::new(0),
            deadline_cb: core::cell::Cell::new(None),
            #[cfg(not(feature = "platform-bare-metal"))]
            liveliness_cb: core::cell::Cell::new(None),
            #[cfg(not(feature = "platform-bare-metal"))]
            liveliness_keyexpr,
            #[cfg(not(feature = "platform-bare-metal"))]
            liveliness_poll: core::cell::Cell::new(LivelinessPoll::IDLE),
            #[cfg(not(feature = "platform-bare-metal"))]
            context: context as *const Context,
            _phantom: PhantomData,
        })
    }

    pub(super) fn set_liveliness(&mut self, liveliness: Option<super::LivelinessToken>) {
        self._liveliness = liveliness;
    }

    /// Phase 108.C.zenoh.4 — liveliness poll loop. Polls `zpico`'s
    /// one-shot `liveliness_get_*` API on a coarse cadence (default
    /// 1s, halved when QoS sets `liveliness_lease_ms`) and fires
    /// `LivelinessChanged` on alive-state transitions. Single-slot
    /// alive (any matching publisher) — DDS's per-publisher
    /// alive_count is approximated to {0, 1}; ROS 2 apps that only
    /// care about "any publisher present" get correct semantics, apps
    /// counting individual publishers see one entry. Exact per-pub
    /// counting needs a long-lived `z_liveliness_declare_subscriber`
    /// shim, which is the next sub-phase if requested.
    fn check_liveliness_and_fire(&self) {
        #[cfg(feature = "platform-bare-metal")]
        {
            return;
        }

        #[cfg(not(feature = "platform-bare-metal"))]
        {
            if self.liveliness_cb.get().is_none() {
                return; // No callback registered → don't burn cycles polling.
            }
            // SAFETY: see `context` field doc.
            let context: &Context = unsafe { &*self.context };
            let now = now_ms();
            let mut state = self.liveliness_poll.get();

            // 1. If a query is in flight, poll it; on completion record
            //    the new alive state and clear the handle.
            //
            // Phase 108.C.zenoh.4-followup — read `liveliness_get_count`
            // BEFORE `liveliness_get_check` because the latter releases the
            // slot on terminal result.
            if state.handle >= 0 {
                let count = context.liveliness_get_count(state.handle).unwrap_or(0);
                match context.liveliness_get_check(state.handle) {
                    Ok(true) => {
                        // At least one matching token responded; `count` is
                        // the exact reply count.
                        self.handle_count_transition(count.max(1) as u16, &mut state);
                    }
                    Ok(false) => {
                        // Still waiting; keep handle for next poll.
                    }
                    Err(_) => {
                        // Timeout (no matching publisher) or error → 0 alive.
                        self.handle_count_transition(0, &mut state);
                    }
                }
            }

            // 2. If idle and the cadence has elapsed, start a fresh query.
            if state.handle < 0 {
                let interval = self.liveliness_poll_interval_ms();
                if now >= state.started_at_ms.saturating_add(interval) {
                    // Liveliness keyexpr must be null-terminated for the
                    // C bridge.
                    let mut nul = heapless::Vec::<u8, 257>::new();
                    let _ = nul.extend_from_slice(self.liveliness_keyexpr.as_bytes());
                    let _ = nul.push(0);
                    if let Ok(handle) =
                        context.liveliness_get_start(nul.as_slice(), LIVELINESS_POLL_TIMEOUT_MS)
                    {
                        state.handle = handle;
                        state.started_at_ms = now;
                    }
                }
            }

            self.liveliness_poll.set(state);
        }
    }

    /// Phase 108.C.zenoh.4-followup — fire `LivelinessChanged` with
    /// the actual delta between the previous and new alive count.
    /// `new_count` is the number of unique publishers that responded
    /// to the most recent wildcard liveliness query.
    #[cfg(not(feature = "platform-bare-metal"))]
    fn handle_count_transition(&self, new_count: u16, state: &mut LivelinessPoll) {
        // Always clear the handle on terminal result.
        state.handle = -1;
        let prev = state.alive_count;
        if new_count == prev {
            // No transition — also keep last_alive in sync for any
            // legacy field dependents.
            state.last_alive = new_count > 0;
            return;
        }
        let (alive_count_change, not_alive_count_change) = if new_count > prev {
            ((new_count - prev) as i16, 0i16)
        } else {
            (-((prev - new_count) as i16), (prev - new_count) as i16)
        };
        state.alive_count = new_count;
        state.last_alive = new_count > 0;
        if let Some(reg) = self.liveliness_cb.get() {
            let status = nros_rmw::LivelinessChangedStatus {
                alive_count: new_count,
                not_alive_count: 0,
                alive_count_change,
                not_alive_count_change,
            };
            // SAFETY: cb is `unsafe extern "C" fn`; user_ctx outlives
            // entity per Phase 108.A.7.
            unsafe {
                (reg.cb)(
                    nros_rmw::EventKind::LivelinessChanged,
                    &status as *const _ as *const core::ffi::c_void,
                    reg.user_ctx,
                );
            }
        }
    }

    #[cfg(not(feature = "platform-bare-metal"))]
    fn liveliness_poll_interval_ms(&self) -> u64 {
        // Half the lease so we observe ≥ 2 probes per lease window.
        // 0 (no lease set) → default 1s.
        // Any backend that fires this code path also has a working
        // platform clock so non-zero `now` is guaranteed.
        // We don't propagate the QoS field through to here yet (would
        // need another `Cell<u32>` field); use the default for now.
        LIVELINESS_POLL_DEFAULT_MS
    }

    /// Phase 108.C.zenoh.3 — read the publisher-supplied timestamp
    /// out of the most recent attachment. Returns `0` if no attachment
    /// is present. Called from `try_recv_raw` to enforce LIFESPAN.
    fn attachment_timestamp_ms(&self) -> u64 {
        let buffer = self.buf.get();
        // Inspect the head ring slot — the message `try_recv_raw` is
        // about to deliver. Empty ring → no timestamp.
        let Some(slot) = buffer.peek_head_slot() else {
            return 0;
        };
        let attachment_len = buffer.ring_att_len[slot];
        if attachment_len < RMW_ATTACHMENT_SIZE {
            return 0;
        }
        let att = &buffer.ring_att[slot];
        // Bytes 8..16 are the i64 timestamp (LE) per
        // ZenohPublisher::serialize_attachment. Convert ns → ms.
        let ts_ns = i64::from_le_bytes([
            att[8], att[9], att[10], att[11], att[12], att[13], att[14], att[15],
        ]);
        if ts_ns <= 0 {
            0
        } else {
            (ts_ns as u64) / 1_000_000
        }
    }

    /// Phase 108.C.zenoh.2 — fire the registered `RequestedDeadlineMissed`
    /// callback when the gap since the last successful receive exceeds
    /// `deadline_ms`. Called from `has_data` / `try_recv_raw` so deadline
    /// is checked on every spin cycle that touches this subscriber.
    /// Rate-limited: at most one fire per deadline period.
    fn check_deadline_and_fire(&self) {
        if self.deadline_ms == 0 {
            return;
        }
        let now = now_ms();
        let last = self.last_msg_at_ms.get();
        if now < last.saturating_add(self.deadline_ms as u64) {
            return; // Within deadline.
        }
        let last_fire = self.last_deadline_fire_ms.get();
        if now < last_fire.saturating_add(self.deadline_ms as u64) {
            return; // Already fired this deadline period.
        }
        self.last_deadline_fire_ms.set(now);
        let total = self.deadline_total.get().saturating_add(1);
        self.deadline_total.set(total);
        if let Some(reg) = self.deadline_cb.get() {
            let status = nros_rmw::CountStatus {
                total_count: total,
                total_count_change: 1,
            };
            // SAFETY: cb is `unsafe extern "C" fn`; user_ctx outlives
            // entity per Phase 108.A.7's per-entity event registry.
            unsafe {
                (reg.cb)(
                    nros_rmw::EventKind::RequestedDeadlineMissed,
                    &status as *const _ as *const core::ffi::c_void,
                    reg.user_ctx,
                );
            }
        }
    }

    /// Phase 108.C.zenoh.5 — peek the just-received attachment for a
    /// sequence number, detect gaps against `next_expected_seq`, and
    /// fire the registered `MessageLost` callback if any are dropped.
    /// Called from `try_recv_raw` AFTER the payload is copied so the
    /// status-event delivery is observable to the user as a side-
    /// effect of receive (matching dust-DDS sample-lost semantics).
    fn check_msg_lost_and_fire(&self) {
        let buffer = self.buf.get();
        // Inspect the head ring slot — the message just copied out by
        // `try_recv_raw`, not yet consumed.
        let Some(slot) = buffer.peek_head_slot() else {
            return;
        };
        let attachment_len = buffer.ring_att_len[slot];
        if attachment_len < RMW_ATTACHMENT_SIZE {
            return; // No attachment, no seq → can't detect gaps.
        }
        let att = &buffer.ring_att[slot];
        let seq = i64::from_le_bytes([
            att[0], att[1], att[2], att[3], att[4], att[5], att[6], att[7],
        ]);
        let expected = self.next_expected_seq.get();
        // First message: synchronise w/o reporting; expected stays 0
        // until we see a real seq, then we set expected = seq + 1.
        let gap = if expected == 0 {
            0
        } else if seq > expected {
            (seq - expected) as u64
        } else {
            // Out-of-order or duplicate — treat as zero loss.
            0
        };
        self.next_expected_seq.set(seq.saturating_add(1));
        if gap == 0 {
            return;
        }
        let delta = u32::try_from(gap).unwrap_or(u32::MAX);
        let total = self.msg_lost_total.get().saturating_add(delta);
        self.msg_lost_total.set(total);
        if let Some(reg) = self.msg_lost_cb.get() {
            let status = nros_rmw::CountStatus {
                total_count: total,
                total_count_change: delta,
            };
            // SAFETY: cb is `unsafe extern "C" fn` matching
            // EventCallback; user_ctx outlives this call (entity owns
            // the Box backing it; freed in nros-node's per-entity
            // event-registry on Drop).
            unsafe {
                (reg.cb)(
                    nros_rmw::EventKind::MessageLost,
                    &status as *const _ as *const core::ffi::c_void,
                    reg.user_ctx,
                );
            }
        }
    }
}

impl ZenohSubscriber {
    /// Try to receive a validated message with E2E integrity status.
    ///
    /// Checks CRC-32 integrity and sequence continuity. Returns
    /// `(payload_len, IntegrityStatus)` so the caller can decide whether
    /// to trust the data.
    ///
    /// The payload bytes are written to `buf[..len]`.
    #[cfg(feature = "safety-e2e")]
    pub fn try_recv_validated(
        &mut self,
        buf: &mut [u8],
    ) -> Result<Option<(usize, nros_rmw::IntegrityStatus)>, TransportError> {
        let buffer = self.buf.get();

        let Some(slot) = buffer.peek_head_slot() else {
            return Ok(None);
        };

        let len = buffer.ring_len[slot];
        if len > buf.len() {
            // Oversized for the caller's buffer — drop the slot so the
            // subscription isn't permanently stuck.
            buffer.consume_head();
            return Err(TransportError::BufferTooSmall);
        }

        // Copy payload out of the ring slot. SPSC: the C producer
        // never touches this slot while head points at it.
        buf[..len].copy_from_slice(&buffer.ring_payload[slot][..len]);

        // Parse attachment for sequence number and CRC.
        let attachment_len = buffer.ring_att_len[slot];
        let (message_seq, crc_valid) = if attachment_len >= RMW_ATTACHMENT_SIZE {
            let att = &buffer.ring_att[slot];
            let seq = i64::from_le_bytes([
                att[0], att[1], att[2], att[3], att[4], att[5], att[6], att[7],
            ]);

            let crc_result = if attachment_len >= RMW_ATTACHMENT_SIZE + SAFETY_CRC_SIZE {
                let received_crc = u32::from_le_bytes([
                    att[RMW_ATTACHMENT_SIZE],
                    att[RMW_ATTACHMENT_SIZE + 1],
                    att[RMW_ATTACHMENT_SIZE + 2],
                    att[RMW_ATTACHMENT_SIZE + 3],
                ]);
                let computed_crc = nros_rmw::crc32(&buf[..len]);
                Some(received_crc == computed_crc)
            } else {
                None
            };

            (seq, crc_result)
        } else {
            (0, None)
        };

        buffer.consume_head();

        let status = self.safety_validator.validate(message_seq, crc_valid);
        Ok(Some((len, status)))
    }

    /// Try to receive raw data along with message info from attachment
    ///
    /// Returns `Ok(Some((len, info)))` if data is available, where:
    /// - `len` is the number of bytes written to the buffer
    /// - `info` is the parsed message info (if attachment was present)
    ///
    /// Returns `Ok(None)` if no data is available.
    pub fn try_recv_with_info(
        &mut self,
        buf: &mut [u8],
    ) -> Result<Option<(usize, Option<MessageInfo>)>, TransportError> {
        let buffer = self.buf.get();

        let Some(slot) = buffer.peek_head_slot() else {
            return Ok(None);
        };

        let len = buffer.ring_len[slot];
        if len > buf.len() {
            // Oversized for the caller's buffer — drop the slot; the
            // subscription recovers on the next message.
            buffer.consume_head();
            return Err(TransportError::BufferTooSmall);
        }

        buf[..len].copy_from_slice(&buffer.ring_payload[slot][..len]);

        let attachment_len = buffer.ring_att_len[slot];
        let message_info = if attachment_len > 0 {
            MessageInfo::from_attachment(&buffer.ring_att[slot][..attachment_len])
        } else {
            None
        };

        buffer.consume_head();

        Ok(Some((len, message_info)))
    }
}

impl Subscriber for ZenohSubscriber {
    type Error = TransportError;

    fn register_waker(&self, waker: &core::task::Waker) {
        self.buf.get().waker.register(waker);
    }

    fn try_recv_raw(&mut self, buf: &mut [u8]) -> Result<Option<usize>, Self::Error> {
        let buffer = self.buf.get();

        // Phase 108.C.zenoh.2 — check deadline expiry on every poll
        // (whether or not data is ready). Rate-limited internally.
        self.check_deadline_and_fire();

        let Some(slot) = buffer.peek_head_slot() else {
            return Ok(None);
        };

        let len = buffer.ring_len[slot];
        if len > buf.len() {
            // Oversized for the caller's buffer — drop the slot; the
            // subscription recovers on the next message.
            buffer.consume_head();
            return Err(TransportError::BufferTooSmall);
        }

        // Phase 108.C.zenoh.3 — LIFESPAN check. If the sample's
        // attachment timestamp is older than `now - lifespan_ms`, drop
        // it. The dropped sample counts as a missed delivery from the
        // subscriber's POV, but we don't fire MessageLost here —
        // lifespan-expired samples aren't "lost in transit", they
        // arrived but were filtered.
        if self.lifespan_ms != 0 {
            let ts = self.attachment_timestamp_ms();
            if ts != 0 {
                let now = now_ms();
                if now > ts.saturating_add(self.lifespan_ms as u64) {
                    buffer.consume_head();
                    return Ok(None);
                }
            }
        }

        // Copy data out of the ring slot. SPSC: the C producer never
        // touches this slot while head points at it.
        buf[..len].copy_from_slice(&buffer.ring_payload[slot][..len]);

        // Phase 108.C.zenoh.5 — detect publisher seq gap before
        // advancing head so the attachment is still valid.
        self.check_msg_lost_and_fire();
        // Phase 108.C.zenoh.2 — successful receive resets deadline.
        self.last_msg_at_ms.set(now_ms());

        buffer.consume_head();

        Ok(Some(len))
    }

    fn has_data(&self) -> bool {
        // Phase 108.C.zenoh.2 — opportunistically check deadline on
        // every has_data poll. Cheap (one clock read + compare). The
        // executor calls has_data each spin to scan the readiness
        // bitmap, so this gives deadline checks the same cadence as
        // message dispatch.
        self.check_deadline_and_fire();
        // Phase 108.C.zenoh.4 — drive the LIVELINESS poll loop on the
        // same cadence. The loop has its own internal time-gated
        // start, so calling on every has_data is cheap (one clock
        // read + cell-load + cell-store when idle).
        self.check_liveliness_and_fire();
        self.buf.get().has_data()
    }

    fn supports_event(&self, kind: nros_rmw::EventKind) -> bool {
        // Phase 108.C.zenoh — MessageLost via attachment seq gap (.5),
        // RequestedDeadlineMissed via clock-based poll (.2),
        // LivelinessChanged surface only (.4) — global liveliness-
        // subscriber bridge fires it from a session-side
        // z_liveliness_declare_subscriber callback. LIFESPAN is a
        // filter, not an event, so no event kind for it.
        if matches!(
            kind,
            nros_rmw::EventKind::MessageLost | nros_rmw::EventKind::RequestedDeadlineMissed
        ) {
            return true;
        }

        #[cfg(not(feature = "platform-bare-metal"))]
        {
            matches!(kind, nros_rmw::EventKind::LivelinessChanged)
        }
        #[cfg(feature = "platform-bare-metal")]
        {
            false
        }
    }

    unsafe fn register_event_callback(
        &mut self,
        kind: nros_rmw::EventKind,
        deadline_ms: u32,
        cb: nros_rmw::EventCallback,
        user_ctx: *mut core::ffi::c_void,
    ) -> Result<(), TransportError> {
        match kind {
            nros_rmw::EventKind::MessageLost => {
                self.msg_lost_cb.set(Some(EventReg { cb, user_ctx }));
                Ok(())
            }
            nros_rmw::EventKind::RequestedDeadlineMissed => {
                // The Phase 108 doc says deadline_ms is consulted only
                // for this event kind; if QoS already declared a
                // non-zero deadline_ms at create time, prefer that.
                // Otherwise allow the registration to set/upgrade it.
                if self.deadline_ms == 0 && deadline_ms != 0 {
                    // SAFETY: lifespan_ms / deadline_ms are inherent
                    // u32 fields; we set via an interior write. No
                    // aliasing concern because Subscriber is owned by
                    // a single thread (`!Sync`).
                    let p = self as *const Self as *mut Self;
                    unsafe { (*p).deadline_ms = deadline_ms };
                }
                self.deadline_cb.set(Some(EventReg { cb, user_ctx }));
                Ok(())
            }
            nros_rmw::EventKind::LivelinessChanged => {
                #[cfg(feature = "platform-bare-metal")]
                {
                    Err(TransportError::Unsupported)
                }
                #[cfg(not(feature = "platform-bare-metal"))]
                {
                    // Slot landed; the session-side liveliness shim that
                    // routes z_liveliness_declare_subscriber callbacks to
                    // these slots is part of 108.C.zenoh.4 follow-up; for
                    // now the slot accepts registrations but never fires.
                    self.liveliness_cb.set(Some(EventReg { cb, user_ctx }));
                    Ok(())
                }
            }
            _ => Err(TransportError::Unsupported),
        }
    }

    fn supports_process_in_place(&self) -> bool {
        true
    }

    fn process_raw_in_place(&mut self, f: impl FnOnce(&[u8])) -> Result<bool, Self::Error> {
        let buffer = self.buf.get();

        let Some(slot) = buffer.peek_head_slot() else {
            return Ok(false);
        };

        let len = buffer.ring_len[slot];
        // Process in-place out of the ring slot, then advance head.
        f(&buffer.ring_payload[slot][..len]);
        buffer.consume_head();

        Ok(true)
    }

    // Phase 231 Wave 0.1 — in-place dispatch with the co-located attachment.
    // Borrows the ring slot's payload + parses its attachment into the canonical
    // `nros_core::MessageInfo` (same conversion as `try_recv_raw_with_info`) for
    // `f`, then advances head. Promoted from the former inherent method.
    fn process_raw_in_place_with_info(
        &mut self,
        f: impl FnOnce(&[u8], Option<nros_core::MessageInfo>),
    ) -> Result<bool, Self::Error> {
        let buffer = self.buf.get();

        let Some(slot) = buffer.peek_head_slot() else {
            return Ok(false);
        };

        let len = buffer.ring_len[slot];

        // Parse attachment (small: 33-37 bytes) into the core MessageInfo.
        let attachment_len = buffer.ring_att_len[slot];
        let core_info = if attachment_len > 0 {
            MessageInfo::from_attachment(&buffer.ring_att[slot][..attachment_len]).map(|zi| {
                let mut info = nros_core::MessageInfo::new();
                info.set_publication_sequence_number(zi.sequence_number);
                info.set_source_timestamp(nros_core::Time::from_nanos(zi.timestamp_ns));
                info.set_publisher_gid(zi.publisher_gid);
                info
            })
        } else {
            None
        };

        f(&buffer.ring_payload[slot][..len], core_info);

        buffer.consume_head();

        Ok(true)
    }

    // Phase 124.D.3.c — native batch take. Drains up to `max_msgs`
    // queued messages out of the SPSC ring in one call, each into a
    // `per_msg_cap`-strided slot of `buf`. Oversized messages
    // (payload > per_msg_cap) are dropped individually rather than
    // erroring the whole batch — the burst-drain caller wants
    // forward progress. Returns the count actually delivered.
    fn try_recv_sequence(
        &mut self,
        buf: &mut [u8],
        per_msg_cap: usize,
        max_msgs: usize,
        out_lens: &mut [usize],
    ) -> Result<usize, Self::Error> {
        if per_msg_cap == 0 || max_msgs == 0 {
            return Ok(0);
        }
        let buffer = self.buf.get();
        let limit = max_msgs.min(out_lens.len());
        let need = limit
            .checked_mul(per_msg_cap)
            .ok_or(TransportError::BufferTooSmall)?;
        if buf.len() < need {
            return Err(TransportError::BufferTooSmall);
        }

        let mut count = 0;
        while count < limit {
            let Some(slot) = buffer.peek_head_slot() else {
                break;
            };
            let len = buffer.ring_len[slot];
            if len > per_msg_cap {
                // Oversized for the caller's per-slot cap — drop this
                // message, keep draining the rest of the burst.
                buffer.consume_head();
                continue;
            }
            let off = count * per_msg_cap;
            buf[off..off + len].copy_from_slice(&buffer.ring_payload[slot][..len]);
            out_lens[count] = len;
            buffer.consume_head();
            count += 1;
        }
        // A successful drain resets the deadline like a single recv.
        if count > 0 {
            self.last_msg_at_ms.set(now_ms());
        }
        Ok(count)
    }

    fn try_recv_raw_with_info(
        &mut self,
        buf: &mut [u8],
    ) -> Result<Option<(usize, Option<nros_core::MessageInfo>)>, Self::Error> {
        // Delegate to the inherent method which parses the zenoh attachment
        match self.try_recv_with_info(buf)? {
            Some((len, zenoh_info)) => {
                let core_info = zenoh_info.map(|zi| {
                    let mut info = nros_core::MessageInfo::new();
                    info.set_publication_sequence_number(zi.sequence_number);
                    info.set_source_timestamp(nros_core::Time::from_nanos(zi.timestamp_ns));
                    info.set_publisher_gid(zi.publisher_gid);
                    info
                });
                Ok(Some((len, core_info)))
            }
            None => Ok(None),
        }
    }

    #[cfg(feature = "safety-e2e")]
    fn try_recv_validated(
        &mut self,
        buf: &mut [u8],
    ) -> Result<Option<(usize, nros_rmw::IntegrityStatus)>, Self::Error> {
        // Delegate to the inherent safety validation method
        ZenohSubscriber::try_recv_validated(self, buf)
    }

    fn deserialization_error(&self) -> Self::Error {
        TransportError::DeserializationError
    }
}

// ============================================================================
// Phase 99.F — ZenohSubscriber SlotBorrowing (zero-copy receive)
// ============================================================================

#[cfg(feature = "lending")]
mod borrowing {
    use super::*;

    /// Backend-lent read-only view into the subscriber's static receive
    /// buffer. Phase 124.D.3.c — borrows the head ring slot's payload
    /// for the lifetime of the view. The SPSC discipline guarantees
    /// the C producer never writes the slot `ring_head` points at, so
    /// no explicit lock is needed; `Drop` advances `ring_head`
    /// (consume-on-borrow semantics, matching `try_recv_raw`).
    pub struct ZenohView<'a> {
        bytes: &'a [u8],
        buffer: &'a SubscriberBuffer,
    }

    impl<'a> AsRef<[u8]> for ZenohView<'a> {
        fn as_ref(&self) -> &[u8] {
            self.bytes
        }
    }

    impl<'a> Drop for ZenohView<'a> {
        fn drop(&mut self) {
            // Advance the consumer counter so the borrowed slot is
            // released back to the C producer.
            self.buffer.consume_head();
        }
    }

    impl nros_rmw::SlotBorrowing for ZenohSubscriber {
        type View<'a>
            = ZenohView<'a>
        where
            Self: 'a;

        fn try_borrow(&mut self) -> Result<Option<Self::View<'_>>, TransportError> {
            // SubscriberBufferRef::get() returns a &SubscriberBuffer whose
            // backing storage is 'static (lives in SUBSCRIBER_BUFFERS).
            // Re-tie that 'static reference to the lifetime of `&mut self`
            // by wrapping it in ZenohView (whose `'_` is implicit on
            // `Self::View<'_>` and bound by `Self: 'a` in the trait def).
            let buffer = self.buf.get();

            let Some(slot) = buffer.peek_head_slot() else {
                return Ok(None);
            };
            let len = buffer.ring_len[slot];

            // SAFETY: SPSC — the C producer never writes the head slot
            // while `ring_head` points at it (its full-check stops it
            // from lapping the consumer). The borrow is valid until
            // ZenohView::drop advances `ring_head`.
            let bytes =
                unsafe { core::slice::from_raw_parts(buffer.ring_payload[slot].as_ptr(), len) };

            Ok(Some(ZenohView { bytes, buffer }))
        }
    }
}

#[cfg(feature = "lending")]
pub use borrowing::ZenohView;

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
pub(super) mod tests {
    extern crate alloc;
    use super::*;
    use nros_rmw::TransportError;

    // --- Subscription buffer helpers ---

    /// Phase 124.D.3.c — simulate the C-shim SPSC ring producer.
    /// Pushes `payload` into the slot at `ring_tail % DEPTH` and
    /// advances `ring_tail`. Drops the message (no advance) when the
    /// ring is full, the payload is oversized, or the payload is
    /// empty — exactly mirroring the C `sample_handler` ring branch.
    pub(in crate::shim) fn simulate_subscription_callback(slot: usize, payload: &[u8]) {
        let mut buf_ref = SubscriberBufferRef::new(slot);
        let buffer = buf_ref.get_mut();

        if payload.is_empty() {
            return; // Empty probe — dropped by the C producer.
        }
        let head = buffer.ring_head.load(Ordering::Acquire);
        let tail = buffer.ring_tail.load(Ordering::Acquire);
        if tail - head >= SUBSCRIBER_RING_DEPTH {
            return; // Ring full — drop.
        }
        if payload.len() > SUBSCRIBER_BUFFER_SIZE {
            return; // Oversized for a slot — drop.
        }
        let s = tail % SUBSCRIBER_RING_DEPTH;
        buffer.ring_payload[s][..payload.len()].copy_from_slice(payload);
        buffer.ring_len[s] = payload.len();
        buffer.ring_att_len[s] = 0;
        buffer.ring_tail.store(tail + 1, Ordering::Release);
    }

    /// Reset a subscriber ring to the empty state.
    pub(in crate::shim) fn reset_subscriber_buffer(slot: usize) {
        let mut buf_ref = SubscriberBufferRef::new(slot);
        let buffer = buf_ref.get_mut();
        buffer.ring_head.store(0, Ordering::Release);
        buffer.ring_tail.store(0, Ordering::Release);
    }

    /// Try to receive one message from a subscriber ring slot.
    /// Replicates `try_recv_raw` logic for testing without a zenoh session.
    pub(in crate::shim) fn try_recv_subscription(
        slot: usize,
        recv_buf: &mut [u8],
    ) -> Result<Option<usize>, TransportError> {
        let buf_ref = SubscriberBufferRef::new(slot);
        let buffer = buf_ref.get();

        let Some(s) = buffer.peek_head_slot() else {
            return Ok(None);
        };
        let len = buffer.ring_len[s];
        if len > recv_buf.len() {
            buffer.consume_head();
            return Err(TransportError::BufferTooSmall);
        }
        recv_buf[..len].copy_from_slice(&buffer.ring_payload[s][..len]);
        buffer.consume_head();
        Ok(Some(len))
    }

    /// Process subscription data in-place (mirrors `process_raw_in_place` logic).
    fn process_in_place_subscription(
        slot: usize,
    ) -> Result<Option<alloc::vec::Vec<u8>>, TransportError> {
        let buf_ref = SubscriberBufferRef::new(slot);
        let buffer = buf_ref.get();

        let Some(s) = buffer.peek_head_slot() else {
            return Ok(None);
        };
        let len = buffer.ring_len[s];
        let data = buffer.ring_payload[s][..len].to_vec();
        buffer.consume_head();
        Ok(Some(data))
    }

    // ========================================================================
    // 37.1a: Subscription buffer state machine tests
    // ========================================================================

    #[test]
    fn sub_buf_idle_poll() {
        let slot = 0;
        reset_subscriber_buffer(slot);

        let mut buf = [0u8; 1024];
        let result = try_recv_subscription(slot, &mut buf);
        assert!(matches!(result, Ok(None)));

        // Ring empty.
        let buffer = SubscriberBufferRef::new(slot).get();
        assert!(!buffer.has_data());
    }

    #[test]
    fn sub_buf_normal_delivery() {
        let slot = 1;
        reset_subscriber_buffer(slot);

        let payload = [0x42u8; 100];
        simulate_subscription_callback(slot, &payload);

        let buffer = SubscriberBufferRef::new(slot).get();
        assert!(buffer.has_data());

        let mut recv_buf = [0u8; 1024];
        let result = try_recv_subscription(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(100))));
        assert_eq!(&recv_buf[..100], &payload);

        assert!(!buffer.has_data());
    }

    #[test]
    fn sub_buf_max_payload() {
        let slot = 2;
        reset_subscriber_buffer(slot);

        // Exactly SUBSCRIBER_BUFFER_SIZE = max slot capacity.
        let payload = [0xFFu8; SUBSCRIBER_BUFFER_SIZE];
        simulate_subscription_callback(slot, &payload);

        let buffer = SubscriberBufferRef::new(slot).get();
        assert!(buffer.has_data());

        let mut recv_buf = [0u8; SUBSCRIBER_BUFFER_SIZE];
        let result = try_recv_subscription(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(n)) if n == SUBSCRIBER_BUFFER_SIZE));
        assert_eq!(&recv_buf, &payload);
    }

    #[test]
    fn sub_buf_oversized_dropped_by_producer() {
        let slot = 3;
        reset_subscriber_buffer(slot);

        // Payload larger than a ring slot — the C producer (here the
        // simulate helper) drops it silently without advancing tail.
        let payload = [0xAAu8; SUBSCRIBER_BUFFER_SIZE + 1];
        simulate_subscription_callback(slot, &payload);

        let buffer = SubscriberBufferRef::new(slot).get();
        assert!(!buffer.has_data(), "oversized message must be dropped");

        let mut recv_buf = [0u8; SUBSCRIBER_BUFFER_SIZE];
        let result = try_recv_subscription(slot, &mut recv_buf);
        assert!(matches!(result, Ok(None)));

        // Recovery: next normal callback is accepted.
        simulate_subscription_callback(slot, b"recovered");
        let result = try_recv_subscription(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(9))));
        assert_eq!(&recv_buf[..9], b"recovered");
    }

    #[test]
    fn sub_buf_caller_too_small() {
        let slot = 4;
        reset_subscriber_buffer(slot);

        // Store 512 bytes, try to receive into a 256-byte buffer.
        let payload = [0xBBu8; 512];
        simulate_subscription_callback(slot, &payload);

        let mut small_buf = [0u8; 256];
        let result = try_recv_subscription(slot, &mut small_buf);
        assert!(matches!(result, Err(TransportError::BufferTooSmall)));

        // Slot consumed (the message that didn't fit is dropped).
        let buffer = SubscriberBufferRef::new(slot).get();
        assert!(!buffer.has_data());

        // Recovery: next callback accepted.
        simulate_subscription_callback(slot, b"small");
        let mut recv_buf = [0u8; 1024];
        let result = try_recv_subscription(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(5))));
        assert_eq!(&recv_buf[..5], b"small");
    }

    #[test]
    fn sub_buf_ring_buffers_burst() {
        // Phase 124.D.3.c — two callbacks without an intervening recv
        // are BOTH buffered (ring), not last-message-wins.
        let slot = 5;
        reset_subscriber_buffer(slot);

        simulate_subscription_callback(slot, b"first_msg");
        simulate_subscription_callback(slot, b"second_msg");

        let mut recv_buf = [0u8; 1024];
        let result = try_recv_subscription(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(9))));
        assert_eq!(&recv_buf[..9], b"first_msg");

        let result = try_recv_subscription(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(10))));
        assert_eq!(&recv_buf[..10], b"second_msg");

        // Ring drained.
        assert!(matches!(
            try_recv_subscription(slot, &mut recv_buf),
            Ok(None)
        ));
    }

    #[test]
    fn sub_buf_ring_full_drops_excess() {
        // Filling the ring past SUBSCRIBER_RING_DEPTH drops the
        // overflow message; the buffered ones still drain in order.
        let slot = 6;
        reset_subscriber_buffer(slot);

        for i in 0..SUBSCRIBER_RING_DEPTH {
            let msg = [i as u8; 4];
            simulate_subscription_callback(slot, &msg);
        }
        // One more — ring is full, this is dropped.
        simulate_subscription_callback(slot, &[0xFFu8; 4]);

        let mut recv_buf = [0u8; 16];
        for i in 0..SUBSCRIBER_RING_DEPTH {
            let result = try_recv_subscription(slot, &mut recv_buf);
            assert!(matches!(result, Ok(Some(4))));
            assert_eq!(&recv_buf[..4], &[i as u8; 4]);
        }
        // The dropped message never appears.
        assert!(matches!(
            try_recv_subscription(slot, &mut recv_buf),
            Ok(None)
        ));
    }

    #[test]
    fn sub_buf_double_consume() {
        let slot = 6;
        reset_subscriber_buffer(slot);

        simulate_subscription_callback(slot, b"data");

        let mut recv_buf = [0u8; 1024];
        let result = try_recv_subscription(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(4))));

        // Second recv returns None — ring drained.
        let result = try_recv_subscription(slot, &mut recv_buf);
        assert!(matches!(result, Ok(None)));
    }

    #[test]
    fn sub_buf_oversized_then_normal() {
        let slot = 7;
        reset_subscriber_buffer(slot);

        // Oversized → dropped by producer → normal → delivered.
        simulate_subscription_callback(slot, &[0u8; SUBSCRIBER_BUFFER_SIZE + 1]);
        let mut recv_buf = [0u8; 1024];
        let result = try_recv_subscription(slot, &mut recv_buf);
        assert!(matches!(result, Ok(None)));

        simulate_subscription_callback(slot, b"after_oversized");
        let result = try_recv_subscription(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(15))));
        assert_eq!(&recv_buf[..15], b"after_oversized");
    }

    #[test]
    fn sub_buf_zero_length_payload_dropped() {
        // Empty probes are dropped by the producer — they never
        // occupy a ring slot.
        let slot = 0;
        reset_subscriber_buffer(slot);

        simulate_subscription_callback(slot, b"");

        let buffer = SubscriberBufferRef::new(slot).get();
        assert!(!buffer.has_data());

        let mut recv_buf = [0u8; 1024];
        let result = try_recv_subscription(slot, &mut recv_buf);
        assert!(matches!(result, Ok(None)));
    }

    #[test]
    fn sub_buf_all_slots_independent() {
        let slot_a = 0;
        let slot_b = 7;
        reset_subscriber_buffer(slot_a);
        reset_subscriber_buffer(slot_b);

        simulate_subscription_callback(slot_a, b"slot_zero");
        simulate_subscription_callback(slot_b, b"slot_seven");

        // Consume slot_b first.
        let mut recv_buf = [0u8; 1024];
        let result = try_recv_subscription(slot_b, &mut recv_buf);
        assert!(matches!(result, Ok(Some(10))));
        assert_eq!(&recv_buf[..10], b"slot_seven");

        // slot_a still has data.
        let buffer_a = SubscriberBufferRef::new(slot_a).get();
        assert!(buffer_a.has_data());

        let result = try_recv_subscription(slot_a, &mut recv_buf);
        assert!(matches!(result, Ok(Some(9))));
        assert_eq!(&recv_buf[..9], b"slot_zero");
    }

    // ========================================================================
    // Phase 124.D.3.c — in-place processing over the ring
    // ========================================================================

    #[test]
    fn sub_buf_in_place_matches_copy() {
        let slot = 0;
        reset_subscriber_buffer(slot);

        // Write 100-byte payload, try_recv (copy path) → capture bytes.
        let payload = [0x42u8; 100];
        simulate_subscription_callback(slot, &payload);

        let mut recv_buf = [0u8; 1024];
        let copy_result = try_recv_subscription(slot, &mut recv_buf);
        assert!(matches!(copy_result, Ok(Some(100))));
        let copy_bytes = recv_buf[..100].to_vec();

        // Reset, write same payload, process_in_place → capture bytes.
        reset_subscriber_buffer(slot);
        simulate_subscription_callback(slot, &payload);

        let in_place_result = process_in_place_subscription(slot);
        assert!(matches!(in_place_result, Ok(Some(_))));
        let in_place_bytes = in_place_result.unwrap().unwrap();

        // Both paths must produce identical data.
        assert_eq!(copy_bytes, in_place_bytes);
    }

    #[test]
    fn sub_buf_in_place_idle() {
        let slot = 1;
        reset_subscriber_buffer(slot);

        // Empty ring → process_in_place returns Ok(None).
        let result = process_in_place_subscription(slot);
        assert!(matches!(result, Ok(None)));
    }

    #[test]
    fn sub_buf_in_place_drains_ring_in_order() {
        // Successive in-place reads drain the ring head-first.
        let slot = 2;
        reset_subscriber_buffer(slot);

        simulate_subscription_callback(slot, b"aaa");
        simulate_subscription_callback(slot, b"bbbb");

        let first = process_in_place_subscription(slot).unwrap().unwrap();
        assert_eq!(&first, b"aaa");
        let second = process_in_place_subscription(slot).unwrap().unwrap();
        assert_eq!(&second, b"bbbb");
        assert!(matches!(process_in_place_subscription(slot), Ok(None)));
    }

    #[test]
    fn sub_buf_consume_advances_head() {
        // Consuming N messages advances ring_head by exactly N.
        let slot = 3;
        reset_subscriber_buffer(slot);

        simulate_subscription_callback(slot, b"one");
        simulate_subscription_callback(slot, b"two");

        let buffer = SubscriberBufferRef::new(slot).get();
        let head_before = buffer.ring_head.load(Ordering::Acquire);
        assert_eq!(head_before, 0);

        let mut recv_buf = [0u8; 16];
        let _ = try_recv_subscription(slot, &mut recv_buf);
        let _ = try_recv_subscription(slot, &mut recv_buf);

        let head_after = buffer.ring_head.load(Ordering::Acquire);
        assert_eq!(head_after, 2);
        assert_eq!(
            buffer.ring_tail.load(Ordering::Acquire),
            head_after,
            "ring drained → head == tail"
        );
    }
}

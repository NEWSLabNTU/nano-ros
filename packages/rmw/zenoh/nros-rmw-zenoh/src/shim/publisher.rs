//! ZenohPublisher implementation

use portable_atomic::Ordering;

use nros_rmw::{Publisher, TransportError};

use super::{
    AtomicSeqCounter, Context, KEYEXPR_BUFFER_SIZE, KEYEXPR_STRING_SIZE, LivelinessToken,
    RMW_ATTACHMENT_SIZE, RMW_GID_SIZE, RmwAttachment,
};
use crate::keyexpr::TopicKeyExpr;

#[cfg(feature = "safety-e2e")]
use super::RMW_ATTACHMENT_SIZE_WITH_CRC;

// ============================================================================
// phase-455 W5 / issue 1341 — TRANSIENT_LOCAL retention, the PUBLISHER half
// ============================================================================

/// Publisher-side transient-local durability, served by query-on-match.
///
/// # What a stock peer actually does, measured rather than recalled
///
/// `rmw_zenoh_cpp` 0.1.9 builds its endpoints on zenoh's `ze_advanced_publisher`
/// / `ze_advanced_subscriber`. Read from a router's own debug log on
/// 2026-09-13, a stock TRANSIENT_LOCAL pair produces exactly this:
///
/// ```text
/// Declare queryable  77/tl_probe/std_msgs::msg::dds_::String_/…/@adv/pub/<zid>/<eid>/_
/// Declare subscriber 77/tl_probe/std_msgs::msg::dds_::String_/…
/// Declare subscriber 77/tl_probe/std_msgs::msg::dds_::String_/…/@adv/pub/**
/// Route query    for 77/tl_probe/std_msgs::msg::dds_::String_/…/@adv/**
/// Route query    for 77/tl_probe/std_msgs::msg::dds_::String_/…/@adv/pub/<zid>/<eid>/_
/// ```
///
/// So the cache of a transient-local publisher is a QUERYABLE at
/// `<topic keyexpr>/@adv/pub/<zid>/<eid>/_`, and a subscriber that joins issues
/// a GLOBAL history query at `<topic keyexpr>/@adv/**` — which INTERSECTS that
/// key. That global query is what serves the case the profile exists for: a
/// client attaching after a goal has already terminated. The per-publisher
/// query beside it is late-joiner detection, driven by a zenoh liveliness token
/// under the same `@adv` prefix, and this module declares NO such token (see
/// "What this does not do").
///
/// The reply carries the TOPIC keyexpr, not the queryable's — that is what the
/// cache does and why the advanced subscriber accepts a reply keyexpr differing
/// from the query's.
///
/// # What this does not do
///
/// * **The subscriber half.** Querying a stock transient-local publisher on
///   match, so a nano-ros subscription gets a latched topic's last value, is a
///   real capability and is a separate item; `shim/qos.rs` still REFUSES
///   TRANSIENT_LOCAL on a subscription rather than pretending.
/// * **A `@adv` liveliness token**, so a stock subscriber that existed BEFORE
///   this publisher does not detect it as a late joiner and does not query it
///   individually. It receives the live samples from that moment on, which is
///   what a volatile publisher would have given it, plus whatever the global
///   query at its own creation collected.
/// * **Retain a STREAMED publish.** `publish_streamed` produces its payload
///   through caller callbacks, so it holds no contiguous buffer this module
///   could copy without invoking the producer a second time. It says so once,
///   at WARN, rather than leaving the retention silently behind the live
///   stream. The LOANED path (`commit_slot`) is NOT in this exception — the
///   arena slice is contiguous, so a transient-local publisher retains from it.
/// * **Retain more than `TL_RETAIN_DEPTH` samples.** The depth is a constant 1,
///   which is exactly `rcl_action_qos_profile_status_default`'s KEEP_LAST(1);
///   `shim/qos.rs` grants a deeper request down to it and ADVERTISES the grant,
///   so the graph never claims a history this keeps. Making it a knob is the
///   extension point, and it would multiply the pool below by that many.
pub(super) mod transient_local {
    use super::*;
    use core::{cell::UnsafeCell, ffi::c_void};

    use portable_atomic::{AtomicBool, AtomicI32, AtomicPtr, AtomicU32, AtomicUsize};

    use crate::config::{MAX_TL_PUBLISHERS, TL_RETAIN_BYTES};

    /// The widest attachment a retained sample can carry — the same 33 or 37
    /// bytes `publish_raw` builds, so the reply is byte-identical to the live
    /// publication a peer would have received.
    #[cfg(not(feature = "safety-e2e"))]
    const TL_ATTACHMENT_MAX: usize = RMW_ATTACHMENT_SIZE;
    #[cfg(feature = "safety-e2e")]
    const TL_ATTACHMENT_MAX: usize = RMW_ATTACHMENT_SIZE_WITH_CRC;

    /// One publisher's retained sample plus everything its query callback needs
    /// to answer without touching the `ZenohPublisher` that owns it.
    ///
    /// The callback runs on zenoh-pico's read task and is handed only a `void*`
    /// context, so it cannot borrow the publisher; it reads THIS, which is
    /// process-static and outlives any entity.
    ///
    /// `nros-pool: MAX_TL_PUBLISHERS × (TL_RETAIN_BYTES + KEYEXPR_BUFFER_SIZE +
    /// TL_ATTACHMENT_MAX)` — priced because, unlike `LendArena`, this one is
    /// reached by a SHIPPED image: every zenoh action server retains its
    /// `/status`.
    pub(crate) struct RetainSlot {
        /// Claimed by a live TRANSIENT_LOCAL publisher.
        claimed: AtomicBool,
        /// Held across the publisher's write so the read side can tell a
        /// half-written sample from a complete one. The two never run on the
        /// same thread — the writer is the application, the reader is the
        /// zenoh-pico read task — and a reader that loses the race declines the
        /// query rather than replying with a torn buffer. A declined query is
        /// the same outcome as no retention yet, which a late joiner already
        /// has to tolerate.
        writing: AtomicBool,
        /// A complete sample is retained.
        valid: AtomicBool,
        session: AtomicPtr<zpico_sys::zpico_session_t>,
        queryable: AtomicI32,
        len: AtomicUsize,
        att_len: AtomicUsize,
        /// How many publishes were too large to retain. Read by
        /// [`RetainSlot::oversize_drops`] so a test can assert the refusal
        /// rather than grep a log line.
        oversize: AtomicU32,
        data: UnsafeCell<[u8; TL_RETAIN_BYTES]>,
        att: UnsafeCell<[u8; TL_ATTACHMENT_MAX]>,
        /// The TOPIC keyexpr, null-terminated — what the reply is sent on.
        reply_keyexpr: UnsafeCell<[u8; KEYEXPR_BUFFER_SIZE]>,
    }

    // SAFETY: the three `UnsafeCell` buffers are written only by the slot's
    // owning publisher, between `writing = true` and `writing = false`, and read
    // only by the query callback, which returns early while `writing` is set.
    unsafe impl Sync for RetainSlot {}

    impl RetainSlot {
        const fn new() -> Self {
            Self {
                claimed: AtomicBool::new(false),
                writing: AtomicBool::new(false),
                valid: AtomicBool::new(false),
                session: AtomicPtr::new(core::ptr::null_mut()),
                queryable: AtomicI32::new(-1),
                len: AtomicUsize::new(0),
                att_len: AtomicUsize::new(0),
                oversize: AtomicU32::new(0),
                data: UnsafeCell::new([0u8; TL_RETAIN_BYTES]),
                att: UnsafeCell::new([0u8; TL_ATTACHMENT_MAX]),
                reply_keyexpr: UnsafeCell::new([0u8; KEYEXPR_BUFFER_SIZE]),
            }
        }

        /// How many publishes this slot refused to retain because they exceeded
        /// `ZPICO_TL_RETAIN_BYTES`.
        pub(crate) fn oversize_drops(&self) -> u32 {
            self.oversize.load(Ordering::Relaxed)
        }
    }

    /// The process-wide retention pool.
    ///
    /// Length is `ZPICO_MAX_TL_PUBLISHERS`, which an image that declares its
    /// endpoints DERIVES — zero included. A zero-length pool is legal and
    /// costs nothing: [`claim`] iterates an empty range and returns `None`, and
    /// `ZenohPublisher::new` turns that into a refusal naming the knob. Nothing
    /// here indexes the pool without having been handed an index by `claim`,
    /// which is the condition `check-c-array-pool-floors` requires of the C
    /// arrays one language over before it allows a zero (issues 1015, 1033).
    pub(crate) static TL_SLOTS: [RetainSlot; MAX_TL_PUBLISHERS] =
        [const { RetainSlot::new() }; MAX_TL_PUBLISHERS];

    /// Entity ids for the `@adv` keyexpr. Only has to be unique within this
    /// session's own `@adv` namespace, which is keyed by our zid.
    static NEXT_ADV_EID: AtomicU32 = AtomicU32::new(0);

    pub(crate) fn next_adv_eid() -> u32 {
        NEXT_ADV_EID.fetch_add(1, Ordering::Relaxed)
    }

    /// Claim a free retention slot, or `None` when the pool is full.
    pub(crate) fn claim() -> Option<usize> {
        TL_SLOTS.iter().position(|s| {
            s.claimed
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        })
    }

    /// Return a slot. The caller must have undeclared its queryable first.
    pub(crate) fn release(slot: usize) {
        let s = &TL_SLOTS[slot];
        s.valid.store(false, Ordering::Release);
        s.len.store(0, Ordering::Relaxed);
        s.att_len.store(0, Ordering::Relaxed);
        s.oversize.store(0, Ordering::Relaxed);
        s.session.store(core::ptr::null_mut(), Ordering::Release);
        s.queryable.store(-1, Ordering::Release);
        s.claimed.store(false, Ordering::Release);
    }

    /// Wire a claimed slot to the session and queryable that will answer for
    /// it, and to the keyexpr its replies go out on.
    pub(crate) fn arm(
        slot: usize,
        session: *mut zpico_sys::zpico_session_t,
        queryable: i32,
        reply_keyexpr: &[u8],
    ) {
        let s = &TL_SLOTS[slot];
        // SAFETY: called from `ZenohPublisher::new` before the queryable handle
        // is published below, so no callback can be reading this slot yet.
        let ke = unsafe { &mut *s.reply_keyexpr.get() };
        let n = reply_keyexpr.len().min(KEYEXPR_BUFFER_SIZE - 1);
        ke[..n].copy_from_slice(&reply_keyexpr[..n]);
        ke[n] = 0;
        s.session.store(session, Ordering::Release);
        // LAST: a non-negative handle is what the callback treats as "this slot
        // is answerable".
        s.queryable.store(queryable, Ordering::Release);
    }

    /// Retain one sample. Returns `false` when it did not fit, in which case
    /// the previously retained sample is DROPPED rather than left in place: a
    /// late joiner served a stale value under a KEEP_LAST(1) promise is worse
    /// than one served nothing, because nothing is a condition it can detect.
    pub(crate) fn retain(slot: usize, data: &[u8], attachment: &[u8]) -> bool {
        let s = &TL_SLOTS[slot];
        s.writing.store(true, Ordering::Release);
        s.valid.store(false, Ordering::Release);
        let fits = data.len() <= TL_RETAIN_BYTES && attachment.len() <= TL_ATTACHMENT_MAX;
        if fits {
            // SAFETY: `writing` is set, so the query callback declines rather
            // than reading; the only other writer is this publisher.
            unsafe {
                (&mut *s.data.get())[..data.len()].copy_from_slice(data);
                (&mut *s.att.get())[..attachment.len()].copy_from_slice(attachment);
            }
            s.len.store(data.len(), Ordering::Relaxed);
            s.att_len.store(attachment.len(), Ordering::Relaxed);
        } else {
            s.oversize.store(
                s.oversize.load(Ordering::Relaxed).saturating_add(1),
                Ordering::Relaxed,
            );
        }
        s.valid.store(fits, Ordering::Release);
        s.writing.store(false, Ordering::Release);
        fits
    }

    /// Reported once per process: the answer and the knob are the same for
    /// every slot, so a line per publisher would be a line per publisher.
    static OVERSIZE_REPORTED: AtomicBool = AtomicBool::new(false);
    /// Its one caller is `publish_streamed`, which is itself
    /// `cfg(not(safety-e2e))` — the streamed path cannot build the trailing
    /// CRC over a payload it never holds contiguously. The gate is on the
    /// REPORTER rather than an `#[allow(dead_code)]` because a dead
    /// `#[allow]` is how a reporter that stops being called goes unnoticed;
    /// under `safety-e2e` there is no streamed publish to report on.
    #[cfg(not(feature = "safety-e2e"))]
    static STREAMED_REPORTED: AtomicBool = AtomicBool::new(false);

    pub(crate) fn report_oversize_once(name: &str, len: usize) {
        if !OVERSIZE_REPORTED.swap(true, Ordering::Relaxed) {
            nros_log::log_warn!(
                nros_log::get_logger("nros_rmw_zenoh"),
                "qos: transient-local publisher '{}' published {} bytes; the retention \
                 slot holds {}. The sample was sent but NOT retained, so a late joiner \
                 gets nothing rather than a truncated message. Raise \
                 ZPICO_TL_RETAIN_BYTES.",
                name,
                len,
                TL_RETAIN_BYTES
            );
        }
    }

    #[cfg(not(feature = "safety-e2e"))]
    pub(crate) fn report_unretainable_path_once(name: &str, path: &str) {
        if !STREAMED_REPORTED.swap(true, Ordering::Relaxed) {
            nros_log::log_warn!(
                nros_log::get_logger("nros_rmw_zenoh"),
                "qos: transient-local publisher '{}' used the {} path, which holds no \
                 contiguous payload to retain. The sample was sent; the retained one is \
                 unchanged, so a late joiner sees the last sample published through \
                 `publish_raw`.",
                name,
                path
            );
        }
    }

    /// The queryable callback a transient-local publisher declares.
    ///
    /// `ctx` is the slot index. Nothing else is safe to hand it: the callback
    /// outlives no borrow and the pool is static.
    ///
    /// # The reply-slot protocol, and why the order matters
    ///
    /// `query_handler` (C) clones the query into a reply slot BEFORE calling
    /// this, and reclaims that slot afterwards only if nobody took the seq
    /// (issue 0902). So a callback with nothing to say must return WITHOUT
    /// taking the seq — taking it and then not replying is exactly the leak
    /// that made an action server stop answering after four liveliness probes.
    pub(crate) extern "C" fn query_callback(
        _keyexpr: *const core::ffi::c_char,
        _keyexpr_len: usize,
        _payload: *const u8,
        _payload_len: usize,
        ctx: *mut c_void,
    ) {
        let slot = ctx as usize;
        if slot >= MAX_TL_PUBLISHERS {
            return;
        }
        let s = &TL_SLOTS[slot];
        let session = s.session.load(Ordering::Acquire);
        let queryable = s.queryable.load(Ordering::Acquire);
        if session.is_null() || queryable < 0 {
            return;
        }
        // Nothing retained, or a write in flight: decline, and leave the seq
        // for `query_handler` to reclaim.
        if !s.valid.load(Ordering::Acquire) || s.writing.load(Ordering::Acquire) {
            return;
        }
        // SAFETY: `valid` is set and `writing` is clear, so the buffers hold a
        // complete sample and the only writer is quiescent.
        let (data, att, ke) = unsafe {
            let len = s.len.load(Ordering::Relaxed);
            let att_len = s.att_len.load(Ordering::Relaxed);
            (
                &(&*s.data.get())[..len],
                &(&*s.att.get())[..att_len],
                &*s.reply_keyexpr.get(),
            )
        };
        let seq = unsafe { zpico_sys::zpico_queryable_take_reply_seq(session, queryable) };
        if seq < 0 {
            return;
        }
        // `ke` is null-terminated by `arm`, which is the contract
        // `zpico_query_reply`'s `const char*` takes.
        let rc = unsafe {
            zpico_sys::zpico_query_reply(
                session,
                queryable,
                seq,
                ke.as_ptr().cast(),
                data.as_ptr(),
                data.len(),
                att.as_ptr(),
                att.len(),
            )
        };
        // A failed reply has already released the slot inside
        // `zpico_query_reply`; there is no second chance to take and nothing a
        // callback on the read task can usefully do about it. The late joiner
        // sees a query that returned no sample, which is the same outcome as
        // "nothing retained yet".
        let _ = rc;
    }
}

// ============================================================================
// ZenohPublisher
// ============================================================================

/// Zenoh publisher wrapping nros-rmw-zenoh ZenohPublisher
///
/// Includes RMW attachment support for rmw_zenoh compatibility.
pub struct ZenohPublisher {
    publisher: crate::zpico::Publisher<'static>,
    /// issue 1437 — the profile `qos::admit` GRANTED for this entity, which is
    /// what [`nros_rmw::Publisher::actual_qos`] answers.
    ///
    /// The shim grants rather than echoes: the depth is clamped to the ring it
    /// actually enforces, reliability is granted RELIABLE whatever was asked
    /// (zenoh-pico blocks on congestion unconditionally), and a transient-local
    /// publisher advertises the retention depth it really serves. The same
    /// profile goes into the liveliness token a `rmw_zenoh_cpp` peer parses,
    /// so what a caller reads back here and what the graph carries are one
    /// value and cannot disagree.
    ///
    /// No `Unknown` policy appears in it: `admit` either grants a concrete
    /// value or refuses the create, so this backend has an answer for every
    /// field.
    granted_qos: nros_rmw::QoSProfile,

    /// RMW GID (generated once per publisher)
    rmw_gid: [u8; RMW_GID_SIZE],
    /// Sequence number counter (atomic for interior mutability)
    sequence_counter: AtomicSeqCounter,
    /// Liveliness token for ROS 2 graph discovery (kept alive for publisher lifetime)
    _liveliness: Option<LivelinessToken>,
    /// Phase 99.F: per-publisher TX arena for SlotLending. Exists only
    /// when the `lending` feature is on.
    #[cfg(feature = "lending")]
    pub(super) lend_arena: lending::LendArena,
    /// Phase 108.C.zenoh.2 — offered-deadline period in ms (`0` =
    /// infinite). Captured from QoS at create time.
    deadline_ms: u32,
    /// Last successful publish timestamp in ms (platform clock).
    last_publish_at_ms: core::cell::Cell<u64>,
    /// Last `OfferedDeadlineMissed` fire timestamp; rate-limits
    /// callbacks to at most one per deadline period.
    last_deadline_fire_ms: core::cell::Cell<u64>,
    /// Cumulative `OfferedDeadlineMissed` count.
    deadline_total: core::cell::Cell<u32>,
    /// Phase 108.A — registered `OfferedDeadlineMissed` callback slot.
    deadline_cb: core::cell::Cell<Option<EventReg>>,
    /// Phase 108.A — registered `LivelinessLost` callback slot.
    liveliness_lost_cb: core::cell::Cell<Option<EventReg>>,
    /// Phase 108.C.zenoh.4-followup — liveliness kind captured from
    /// QoS at create time. `LivelinessLost` only fires when kind is
    /// `ManualByTopic` / `ManualByNode` AND `liveliness_lease_ms > 0`.
    /// AUTOMATIC + NONE never fire — zenoh session keepalive covers
    /// AUTOMATIC; NONE means the app opted out.
    liveliness_kind: nros_rmw::QoSLivelinessPolicy,
    /// Phase 108.C.zenoh.4-followup — liveliness lease in ms. `0` =
    /// infinite (no LivelinessLost firing).
    liveliness_lease_ms: u32,
    /// Phase 108.C.zenoh.4-followup — last `assert_liveliness()` (or
    /// `publish_raw` when liveliness_kind is one of the manual modes)
    /// timestamp in ms.
    last_assert_at_ms: core::cell::Cell<u64>,
    /// Phase 108.C.zenoh.4-followup — last `LivelinessLost` fire
    /// timestamp; rate-limits callbacks to ≤ 1 per lease window.
    last_liveliness_lost_fire_ms: core::cell::Cell<u64>,
    /// Phase 108.C.zenoh.4-followup — cumulative `LivelinessLost` count.
    liveliness_lost_total: core::cell::Cell<u32>,
    /// phase-455 W5 / issue 1341 — present exactly when this publisher was
    /// granted TRANSIENT_LOCAL durability. `None` for a VOLATILE publisher,
    /// which is every publisher in the tree bar an action server's `/status`
    /// and a deliberately latched topic, so the cost is paid by the images
    /// that asked for it.
    retention: Option<TransientLocalRetention>,
    /// The publisher's ROS name, for the one-shot retention diagnostics. A
    /// `&'static str` is not available here (the name is a `&str` on a
    /// `TopicInfo` that does not outlive `new`), and a heapless copy is cheaper
    /// than making every warning site take the name as an argument it does not
    /// have.
    name: heapless::String<KEYEXPR_STRING_SIZE>,
}

/// One claimed retention slot plus the queryable that answers from it.
///
/// A struct with its own `Drop` rather than two fields on `ZenohPublisher`,
/// because the ORDER is load-bearing: the queryable must be undeclared before
/// the slot is released, or a query already in flight reads a slot that has
/// been handed to another publisher. Fields drop after the `Drop` body, so the
/// body takes the queryable out and drops it first.
struct TransientLocalRetention {
    slot: usize,
    queryable: Option<crate::zpico::Queryable>,
}

impl Drop for TransientLocalRetention {
    fn drop(&mut self) {
        drop(self.queryable.take());
        transient_local::release(self.slot);
    }
}

/// Phase 108.A — single-slot event registration. cb is `unsafe extern
/// "C" fn` (always Send); user_ctx outlives entity.
#[derive(Clone, Copy)]
struct EventReg {
    cb: nros_rmw::EventCallback,
    user_ctx: *mut core::ffi::c_void,
}

/// Phase 108.C.zenoh — read the platform clock in ms.
///
/// Phase 129.C.3.a — call the canonical `nros_platform_*` C
/// symbol directly instead of routing through `ConcretePlatform`.
/// Drops this crate's `nros-platform/platform-<rtos>` forward.
fn now_ms() -> u64 {
    unsafe extern "C" {
        fn nros_platform_time_now_ns() -> u64;
    }
    // Issue 0532 item 5 — the ABI is nanoseconds now; this caller wants ms.
    unsafe { nros_platform_time_now_ns() / 1_000_000 }
}

impl ZenohPublisher {
    /// Create a new publisher for the given topic
    pub fn new(
        context: &Context,
        topic: &nros_rmw::TopicInfo,
        liveliness: Option<LivelinessToken>,
        qos: &nros_rmw::QoSProfile,
    ) -> Result<Self, TransportError> {
        // Generate the topic key with null terminator
        let key: heapless::String<KEYEXPR_STRING_SIZE> = topic.to_key();

        #[cfg(feature = "std")]
        log::debug!("Publisher data keyexpr: {}", key.as_str());

        // Create null-terminated keyexpr
        let mut keyexpr_buf = [0u8; KEYEXPR_BUFFER_SIZE];
        let bytes = key.as_bytes();
        if bytes.len() >= keyexpr_buf.len() {
            return Err(TransportError::TopicNameInvalid);
        }
        keyexpr_buf[..bytes.len()].copy_from_slice(bytes);
        keyexpr_buf[bytes.len()] = 0;

        // Safety: We need to extend the lifetime because ZenohPublisher borrows from Context.
        // This is safe because:
        // 1. ZenohPublisher is stored in ZenohSession which owns the Context
        // 2. The underlying C shim manages its own state
        // 3. We transmute the lifetime to 'static for storage
        let publisher = unsafe {
            let pub_result = context.declare_publisher(&keyexpr_buf, topic.tx_express);
            match pub_result {
                Ok(p) => core::mem::transmute::<
                    crate::zpico::Publisher<'_>,
                    crate::zpico::Publisher<'static>,
                >(p),
                Err(e) => return Err(TransportError::from(e)),
            }
        };

        // phase-455 W5 / issue 1341 — the transient-local half. `admit` has
        // already refused TRANSIENT_LOCAL for every kind but a publisher and
        // granted the depth this retention actually serves, so reaching here
        // with TransientLocal means "serve it".
        let retention = if qos.durability == nros_rmw::QoSDurabilityPolicy::TransientLocal {
            Some(Self::declare_retention(
                context,
                key.as_str(),
                &keyexpr_buf,
            )?)
        } else {
            None
        };

        let now = now_ms();
        Ok(Self {
            publisher,
            rmw_gid: RmwAttachment::generate_gid(),
            sequence_counter: AtomicSeqCounter::new(0),
            _liveliness: liveliness,
            #[cfg(feature = "lending")]
            lend_arena: lending::LendArena::new(),
            // nros-qos-honours: DEADLINE — carried onto the publisher and
            // checked in `check_offered_deadline`, which fires
            // `OfferedDeadlineMissed` (rate-limited to one per window). 0 and
            // DURATION_INFINITE_MS both mean "no check".
            deadline_ms: qos.deadline_ms,
            last_publish_at_ms: core::cell::Cell::new(now),
            last_deadline_fire_ms: core::cell::Cell::new(now),
            deadline_total: core::cell::Cell::new(0),
            deadline_cb: core::cell::Cell::new(None),
            liveliness_lost_cb: core::cell::Cell::new(None),
            liveliness_kind: qos.liveliness_kind,
            liveliness_lease_ms: qos.liveliness_lease_ms,
            last_assert_at_ms: core::cell::Cell::new(now),
            last_liveliness_lost_fire_ms: core::cell::Cell::new(now),
            liveliness_lost_total: core::cell::Cell::new(0),
            retention,
            name: heapless::String::try_from(topic.name).unwrap_or_default(),
            granted_qos: *qos,
        })
    }

    /// Claim a retention slot and declare the cache queryable a stock
    /// `ze_advanced_subscriber` queries.
    ///
    /// The keyexpr is `<topic keyexpr>/@adv/pub/<zid>/<eid>/_`, which is the
    /// shape measured off a stock pair (see the module doc). The trailing `_`
    /// is zenoh's empty-metadata chunk; the `zid` is ours, in the same LSB-first
    /// hex the ROS liveliness tokens use, and the `eid` only has to be unique
    /// under that zid.
    fn declare_retention(
        context: &Context,
        topic_key: &str,
        topic_keyexpr_nul: &[u8; KEYEXPR_BUFFER_SIZE],
    ) -> Result<TransientLocalRetention, TransportError> {
        let Some(slot) = transient_local::claim() else {
            // issue 0460's shape, one pool over: name the knob, say what the
            // pool is for, and do it through `nros_log` so an embedded image
            // that has no `std` logger still gets the sentence. The build-time
            // refusal in `build.rs` is the one that catches this on a DECLARED
            // image; this arm is the undeclared road's answer.
            // Issue 1378's second defect, fixed for the whole family: THE CAUSE
            // AND THE KNOB COME FIRST, the topic key LAST. `nros_log`'s
            // call-site buffer is 256 bytes and truncates with a `…`
            // (`nros_log::buffer`), and a ROS action topic key is ~90 of them
            // ('0/fibonacci/_action/status/action_msgs::msg::dds_::GoalStatusArray_/
            // TypeHashNotSupported'), so a message that opens with the topic
            // spends its budget before it says anything. That is how issue 1378
            // was reported reading `…could not be declared (Full…` — cut one
            // word into the only fact the reader came for.
            nros_log::log_error!(
                nros_log::get_logger("nros_rmw_zenoh"),
                "qos: TRANSIENT_LOCAL retention pool exhausted ({} slot(s), all taken) \
                 — raise ZPICO_MAX_TL_PUBLISHERS. Each such publisher retains its last \
                 sample and answers a late joiner's query from it. topic '{}'",
                crate::config::MAX_TL_PUBLISHERS,
                topic_key
            );
            return Err(TransportError::Backend(
                "zenoh transient-local retention pool exhausted — raise \
                 ZPICO_MAX_TL_PUBLISHERS. A TRANSIENT_LOCAL publisher retains its last \
                 sample and declares a queryable to serve it to a late joiner.",
            ));
        };
        let mut adv: heapless::String<KEYEXPR_STRING_SIZE> = heapless::String::new();
        let mut hex = [0u8; 32];
        match context.zid() {
            Ok(zid) => zid.to_hex_bytes(&mut hex),
            Err(e) => {
                transient_local::release(slot);
                return Err(TransportError::from(e));
            }
        }
        let eid = transient_local::next_adv_eid();
        let mut eid_buf: heapless::String<12> = heapless::String::new();
        let _ = core::fmt::Write::write_fmt(&mut eid_buf, format_args!("{eid}"));
        let built = (|| -> Result<(), ()> {
            adv.push_str(topic_key).map_err(|_| ())?;
            adv.push_str("/@adv/pub/").map_err(|_| ())?;
            adv.push_str(core::str::from_utf8(&hex).map_err(|_| ())?)
                .map_err(|_| ())?;
            adv.push('/').map_err(|_| ())?;
            adv.push_str(eid_buf.as_str()).map_err(|_| ())?;
            adv.push_str("/_").map_err(|_| ())
        })();
        if built.is_err() {
            transient_local::release(slot);
            nros_log::log_error!(
                nros_log::get_logger("nros_rmw_zenoh"),
                "qos: TRANSIENT_LOCAL cache keyexpr does not fit \
                 NROS_KEYEXPR_STRING_SIZE={}. The cache key is the topic key plus 47 \
                 bytes; raise the knob. topic '{}'",
                KEYEXPR_STRING_SIZE,
                topic_key
            );
            return Err(TransportError::TopicNameInvalid);
        }
        let mut adv_nul = [0u8; KEYEXPR_BUFFER_SIZE];
        let bytes = adv.as_bytes();
        if bytes.len() >= adv_nul.len() {
            transient_local::release(slot);
            return Err(TransportError::TopicNameInvalid);
        }
        adv_nul[..bytes.len()].copy_from_slice(bytes);
        adv_nul[bytes.len()] = 0;

        // Wire the reply keyexpr BEFORE the queryable exists, so the first
        // query cannot find a slot that knows where to reply but not how.
        transient_local::arm(
            slot,
            context.handle(),
            -1,
            &topic_keyexpr_nul[..=topic_key.len().min(KEYEXPR_BUFFER_SIZE - 1)],
        );
        // SAFETY: the callback's context is a slot INDEX into a process-static
        // pool, so it is valid for the queryable's whole life and beyond.
        let queryable = unsafe {
            context.declare_queryable_raw(
                &adv_nul,
                transient_local::query_callback,
                slot as *mut core::ffi::c_void,
            )
        };
        let queryable = match queryable {
            Ok(q) => q,
            Err(e) => {
                transient_local::release(slot);
                nros_log::log_error!(
                    nros_log::get_logger("nros_rmw_zenoh"),
                    // THE LINE ISSUE 1378 WAS REPORTED FROM, and it was cut at
                    // `(Full…` because the topic key ate the 256-byte buffer
                    // before the reason and the knob were reached. Cause first
                    // now; the topic is the part that may truncate.
                    "qos: TRANSIENT_LOCAL cache queryable refused ({:?}); if Full raise \
                     ZPICO_MAX_QUERYABLES (Zephyr: CONFIG_NROS_MAX_QUERYABLES) — a TL \
                     publisher is a queryable on top of every service server, and an \
                     action server has one for /status. topic '{}'",
                    e,
                    topic_key
                );
                return Err(TransportError::from(e));
            }
        };
        transient_local::arm(
            slot,
            context.handle(),
            queryable.handle(),
            &topic_keyexpr_nul[..=topic_key.len().min(KEYEXPR_BUFFER_SIZE - 1)],
        );
        Ok(TransientLocalRetention {
            slot,
            queryable: Some(queryable),
        })
    }

    pub(super) fn set_liveliness(&mut self, liveliness: Option<LivelinessToken>) {
        self._liveliness = liveliness;
    }

    /// Phase 108.C.zenoh.{2,3} — current platform time as nanoseconds
    /// for the RMW attachment. Falls back to a per-publisher
    /// monotonic counter when the platform clock returns 0 (bare
    /// no-std smoke build w/o concrete platform).
    fn current_timestamp(&self) -> i64 {
        let ms = now_ms();
        if ms == 0 {
            // No real clock — preserve the old monotonic-counter
            // behaviour so existing tests aren't disrupted.
            #[allow(clippy::unnecessary_cast)] // i32→i64 on embedded, no-op on std
            return self
                .sequence_counter
                .load(Ordering::Relaxed)
                .saturating_mul(1_000_000) as i64;
        }
        // Cap to i64 max to avoid overflow on long-running shims.
        (ms.min(i64::MAX as u64) as i64).saturating_mul(1_000_000)
    }

    /// Phase 108.C.zenoh.2 — fire `OfferedDeadlineMissed` if we
    /// haven't published within the deadline window. Called from
    /// `publish_raw`; rate-limited to one fire per deadline.
    fn check_offered_deadline(&self) {
        if self.deadline_ms == 0 {
            return;
        }
        let now = now_ms();
        let last = self.last_publish_at_ms.get();
        let dl = self.deadline_ms as u64;
        if now < last.saturating_add(dl) {
            return;
        }
        let last_fire = self.last_deadline_fire_ms.get();
        if now < last_fire.saturating_add(dl) {
            return;
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
            // entity per Phase 108.A.7.
            unsafe {
                (reg.cb)(
                    nros_rmw::EventKind::OfferedDeadlineMissed,
                    &status as *const _ as *const core::ffi::c_void,
                    reg.user_ctx,
                );
            }
        }
    }

    /// Phase 108.C.zenoh.4-followup — fire `LivelinessLost` when the
    /// gap since the last manual assertion exceeds `liveliness_lease_ms`.
    /// Only fires for `ManualByTopic` / `ManualByNode` kinds with a
    /// non-zero lease. Rate-limited to one fire per lease window.
    /// Called from `publish_raw`; if the app stops publishing entirely,
    /// no event fires (publisher path has no spin tick).
    fn check_liveliness_lost(&self) {
        if !self.liveliness_kind.is_manual() {
            return;
        }
        if self.liveliness_lease_ms == 0 {
            return;
        }
        let now = now_ms();
        if now == 0 {
            return;
        }
        let lease = self.liveliness_lease_ms as u64;
        let last_assert = self.last_assert_at_ms.get();
        if now < last_assert.saturating_add(lease) {
            return;
        }
        let last_fire = self.last_liveliness_lost_fire_ms.get();
        if now < last_fire.saturating_add(lease) {
            return;
        }
        self.last_liveliness_lost_fire_ms.set(now);
        let total = self.liveliness_lost_total.get().saturating_add(1);
        self.liveliness_lost_total.set(total);
        if let Some(reg) = self.liveliness_lost_cb.get() {
            let status = nros_rmw::CountStatus {
                total_count: total,
                total_count_change: 1,
            };
            // SAFETY: cb is `unsafe extern "C" fn`; user_ctx outlives
            // entity per Phase 108.A.7.
            unsafe {
                (reg.cb)(
                    nros_rmw::EventKind::LivelinessLost,
                    &status as *const _ as *const core::ffi::c_void,
                    reg.user_ctx,
                );
            }
        }
    }

    /// phase-455 W5 — keep this sample for a late joiner, when this publisher
    /// was granted TRANSIENT_LOCAL. A no-op on a VOLATILE publisher, which is
    /// the reason the branch is a `match` on an `Option` rather than a flag:
    /// there is no slot, so there is nothing to test against.
    fn retain_sample(&self, data: &[u8], attachment: &[u8]) {
        let Some(retention) = self.retention.as_ref() else {
            return;
        };
        if !transient_local::retain(retention.slot, data, attachment) {
            transient_local::report_oversize_once(self.name.as_str(), data.len());
        }
    }

    /// phase-455 W5 — a publish path that holds no contiguous payload says so,
    /// once, instead of leaving the retention silently behind the live stream.
    ///
    /// `cfg`-gated with its one caller, `publish_streamed`. See the note on
    /// `transient_local::STREAMED_REPORTED`.
    #[cfg(not(feature = "safety-e2e"))]
    fn report_unretainable(&self, path: &str) {
        if self.retention.is_some() {
            transient_local::report_unretainable_path_once(self.name.as_str(), path);
        }
    }

    /// issue 1341 — how many publishes this publisher's retention slot refused
    /// because they exceeded `ZPICO_TL_RETAIN_BYTES`, and `None` when it is not
    /// a transient-local publisher at all. A counter rather than a log grep,
    /// for the same reason the reply-slot refusals are one (phase-455 W1).
    pub fn transient_local_oversize_drops(&self) -> Option<u32> {
        self.retention
            .as_ref()
            .map(|r| transient_local::TL_SLOTS[r.slot].oversize_drops())
    }

    /// Serialize attachment for RMW compatibility
    fn serialize_attachment(&self, seq: i64, ts: i64, buf: &mut [u8; RMW_ATTACHMENT_SIZE]) {
        // Sequence number (little-endian)
        buf[0..8].copy_from_slice(&seq.to_le_bytes());
        // Timestamp (little-endian)
        buf[8..16].copy_from_slice(&ts.to_le_bytes());
        // VLE length (16 fits in single byte)
        buf[16] = RMW_GID_SIZE as u8;
        // GID bytes
        buf[17..33].copy_from_slice(&self.rmw_gid);
    }
}

impl Publisher for ZenohPublisher {
    type Error = TransportError;

    fn publish_raw(&self, data: &[u8]) -> Result<(), Self::Error> {
        // Phase 108.C.zenoh.2 — fire OfferedDeadlineMissed BEFORE the
        // publish so the user observes the late-publish event with
        // the correct delta (last_publish_at gets bumped after).
        self.check_offered_deadline();
        // Phase 108.C.zenoh.4-followup — same idea for LivelinessLost.
        // Manual liveliness kinds: a publish does NOT count as a
        // liveliness assertion (only `assert_liveliness()` does), so
        // the check fires when the lease has expired since the last
        // explicit assert.
        self.check_liveliness_lost();

        // Get next sequence number and timestamp atomically
        #[allow(clippy::useless_conversion)] // i32→i64 on embedded, no-op on std
        let seq: i64 = (self.sequence_counter.fetch_add(1, Ordering::Relaxed) + 1).into();
        let ts = self.current_timestamp();

        // Without safety-e2e: 33-byte attachment
        #[cfg(not(feature = "safety-e2e"))]
        let result: Result<(), Self::Error> = {
            let mut att_buf = [0u8; RMW_ATTACHMENT_SIZE];
            self.serialize_attachment(seq, ts, &mut att_buf);

            #[cfg(feature = "std")]
            log::trace!(
                "Publishing {} bytes with attachment: seq={}, ts={}, gid={:02x?}",
                data.len(),
                seq,
                ts,
                &self.rmw_gid[..4],
            );

            let r = self
                .publisher
                .publish_with_attachment(data, Some(&att_buf))
                .map_err(TransportError::from);
            if r.is_ok() {
                self.retain_sample(data, &att_buf);
            }
            r
        };

        // With safety-e2e: 37-byte attachment (33 + 4-byte CRC of payload)
        #[cfg(feature = "safety-e2e")]
        let result: Result<(), Self::Error> = {
            let mut att_buf = [0u8; RMW_ATTACHMENT_SIZE_WITH_CRC];
            self.serialize_attachment(
                seq,
                ts,
                (&mut att_buf[..RMW_ATTACHMENT_SIZE]).try_into().unwrap(),
            );

            // Compute CRC-32 over CDR payload and append
            let crc = nros_rmw::crc32(data);
            att_buf[RMW_ATTACHMENT_SIZE..RMW_ATTACHMENT_SIZE_WITH_CRC]
                .copy_from_slice(&crc.to_le_bytes());

            #[cfg(feature = "std")]
            log::trace!(
                "Publishing {} bytes with safety attachment: seq={}, ts={}, crc={:#010x}",
                data.len(),
                seq,
                ts,
                crc,
            );

            let r = self
                .publisher
                .publish_with_attachment(data, Some(&att_buf))
                .map_err(TransportError::from);
            if r.is_ok() {
                self.retain_sample(data, &att_buf);
            }
            r
        };

        // Phase 108.C.zenoh.2 — only update last_publish_at on a
        // successful wire write so a failed publish doesn't reset the
        // deadline window.
        if result.is_ok() {
            self.last_publish_at_ms.set(now_ms());
        }
        result
    }

    /// Phase 124.E.3 — native streamed publish.
    ///
    /// Drives zenoh-pico's `z_bytes_writer` so the payload assembles
    /// directly inside zenoh's allocator-managed `z_owned_bytes_t`
    /// rather than first into a caller-side staging buffer. The
    /// ROS-interop attachment (sequence number + source timestamp +
    /// GID) is built here exactly like `publish_raw` and handed to
    /// the C shim alongside the chunk callback.
    ///
    /// `safety-e2e` builds fall through to the default staging-buffer
    /// path: the safety attachment's trailing CRC-32 is computed over
    /// the *whole* payload, which the streamed path never holds
    /// contiguously. Incremental CRC across the writer chunks is
    /// possible but out of scope for the v1 surface.
    #[cfg(not(feature = "safety-e2e"))]
    unsafe fn publish_streamed(
        &self,
        size_cb: unsafe extern "C" fn(out_total_len: *mut usize, user_ctx: *mut core::ffi::c_void),
        chunk_cb: unsafe extern "C" fn(
            out_buf: *mut u8,
            cap: usize,
            out_written: *mut usize,
            user_ctx: *mut core::ffi::c_void,
        ),
        user_ctx: *mut core::ffi::c_void,
    ) -> Result<(), Self::Error> {
        self.check_offered_deadline();
        self.check_liveliness_lost();
        self.report_unretainable("streamed publish");

        #[allow(clippy::useless_conversion)] // i32→i64 on embedded, no-op on std
        let seq: i64 = (self.sequence_counter.fetch_add(1, Ordering::Relaxed) + 1).into();
        let ts = self.current_timestamp();
        let mut att_buf = [0u8; RMW_ATTACHMENT_SIZE];
        self.serialize_attachment(seq, ts, &mut att_buf);

        // Resolve the total length up-front so the C shim can size
        // the writer in one shot.
        let mut total: usize = 0;
        // SAFETY: `size_cb` writes a single `usize` via the
        // out-pointer per the 124.E.1 contract.
        unsafe { size_cb(&mut total as *mut usize, user_ctx) };

        let rc = unsafe {
            zpico_sys::zpico_publish_streamed(
                self.publisher.session(),
                self.publisher.handle(),
                total,
                Some(chunk_cb),
                user_ctx,
                att_buf.as_ptr(),
                att_buf.len(),
            )
        };
        if rc == 0 {
            self.last_publish_at_ms.set(now_ms());
            Ok(())
        } else {
            Err(TransportError::PublishFailed)
        }
    }

    /// Phase 108.C.zenoh.4-followup — manual liveliness assertion.
    ///
    /// **`Unsupported` under a manual kind, and that is the correction**
    /// (the phase-467 RMW gap-closure design study's Row 7). This returned
    /// `Ok(())` unconditionally while sending NOTHING: zenoh-pico's
    /// liveliness is a SESSION-scoped keepalive, and a per-topic lease a peer
    /// can renew on demand is a DDS concept this transport does not have.
    /// Re-declaring the `@ros2_lv` token on every assert was the alternative
    /// and is worse than silence — a peer reads the churn as leave/join. So
    /// the honest answer is the one `rmw_vtable.h` already prescribes for a
    /// backend without manual liveliness. Cyclone DOES implement this
    /// (`dds_assert_liveliness`), which is why the divergence is per backend
    /// and not a property of the API.
    ///
    /// The LOCAL record survives, deliberately. `last_assert_at_ms` drives
    /// `check_liveliness_lost`, a watchdog that fires this
    /// publisher's own `LivelinessLost` callback when the app stops
    /// asserting — a real, working, LOCAL capability. Recording the call and
    /// then reporting `Unsupported` is not a contradiction: the return value
    /// says what a PEER can see.
    ///
    /// `Ok(())` for `Automatic` (zenoh's session keepalive covers it) and
    /// `None` — under those kinds there was nothing to assert, so nothing
    /// was lost.
    fn assert_liveliness(&self) -> Result<(), Self::Error> {
        if !self.liveliness_kind.is_manual() {
            return Ok(());
        }
        let now = now_ms();
        if now != 0 {
            self.last_assert_at_ms.set(now);
        }
        Err(TransportError::Unsupported)
    }

    /// Upstream `rmw_get_gid_for_publisher` — the phase-467 RMW gap-closure
    /// design study's Q1, step 2 for the shim road.
    ///
    /// zenoh is the one backend that can answer this from a value it already
    /// PUBLISHES: `rmw_gid` is the 16-byte id stamped into every outgoing
    /// attachment, so a subscriber's
    /// [`MessageInfo::publisher_gid`](nros_core::MessageInfo::publisher_gid)
    /// and this answer are the SAME BYTES, zero-extended by the one spelling
    /// of the padding. Nothing here touches the wire — the attachment stays
    /// 16 bytes, which is `rmw_zenoh_cpp`'s layout and whose reader rejects
    /// any other length.
    ///
    /// What it is NOT: derived from anything a stock ROS 2 peer would compute
    /// for the same publisher. `generate_gid()` is a counter and a stack
    /// address, not the session's `ZenohId`; making it mean something to a
    /// peer is issue 1495 and moves a wire value.
    fn get_gid(&self) -> Result<[u8; nros_rmw::PUBLISHER_GID_SIZE], Self::Error> {
        Ok(nros_rmw::pad_publisher_gid(&self.rmw_gid))
    }

    fn buffer_error(&self) -> Self::Error {
        TransportError::BufferTooSmall
    }

    fn serialization_error(&self) -> Self::Error {
        TransportError::SerializationError
    }

    fn supports_event(&self, kind: nros_rmw::EventKind) -> bool {
        // Phase 108.C.zenoh — pub side surfaces OfferedDeadlineMissed
        // (clock-based check on publish_raw) and a slot for
        // LivelinessLost (not fired today; needs per-publisher
        // keepalive-timer infra, separate phase).
        matches!(
            kind,
            nros_rmw::EventKind::OfferedDeadlineMissed | nros_rmw::EventKind::LivelinessLost
        )
    }

    unsafe fn register_event_callback(
        &mut self,
        kind: nros_rmw::EventKind,
        deadline_ms: u32,
        cb: nros_rmw::EventCallback,
        user_ctx: *mut core::ffi::c_void,
    ) -> Result<(), TransportError> {
        match kind {
            nros_rmw::EventKind::OfferedDeadlineMissed => {
                if self.deadline_ms == 0 && deadline_ms != 0 {
                    let p = self as *const Self as *mut Self;
                    unsafe { (*p).deadline_ms = deadline_ms };
                }
                self.deadline_cb.set(Some(EventReg { cb, user_ctx }));
                Ok(())
            }
            nros_rmw::EventKind::LivelinessLost => {
                // Slot landed; never fired today (see struct doc).
                self.liveliness_lost_cb.set(Some(EventReg { cb, user_ctx }));
                Ok(())
            }
            _ => Err(TransportError::Unsupported),
        }
    }

    /// What this backend GRANTED, which for a zenoh publisher is not always
    /// what was asked: RELIABLE whatever the request said, and a
    /// transient-local publisher's depth clamped to the retention it serves.
    /// Same value the liveliness token advertises to the graph.
    fn actual_qos(&self) -> nros_rmw::QoSProfile {
        self.granted_qos
    }
}

// ============================================================================
// Phase 99.F — ZenohPublisher SlotLending (zero-copy publish)
// ============================================================================

#[cfg(feature = "lending")]
mod lending {
    use super::*;
    use core::{
        cell::UnsafeCell,
        sync::atomic::{AtomicBool, Ordering as CoreOrdering},
    };

    /// Per-publisher TX arena slot capacity — set at build time via
    /// `ZPICO_PUBLISHER_TX_BUFFER_SIZE` (default 1024). It is the ceiling
    /// `try_claim` enforces: a loan longer than this is `TooLarge`, so a
    /// consumer publishing images or scans through the zero-copy path raises
    /// this knob rather than editing the crate (issue 0813).
    pub const ZENOH_TX_BUF: usize = crate::config::PUBLISHER_TX_BUFFER_SIZE;

    /// Backend-owned arena for a `ZenohPublisher`. Single-slot;
    /// concurrent loans return Err(WouldBlock).
    ///
    /// issue 0813 — the arena is embedded in each `ZenohPublisher`, and a
    /// session holds at most `ZPICO_MAX_PUBLISHERS` of them, so the knob's cost
    /// is that product. It carries NO `// nros-pool:` annotation, deliberately:
    /// the arena exists only under the `lending` feature, which issue 0814
    /// measured as enabled by exactly one posix test crate and by no shipped
    /// image. `nm` on a built zenoh example finds zero of these. A pool row
    /// would publish 8,192 bytes of cost that no consumer's image actually
    /// pays, in the one page people use to rightsize a board — the opposite of
    /// what the inventory is for. The KNOB is enumerated either way, which is
    /// what issue 0813 asked for; annotate the pool when `lending` reaches a
    /// shipped image, and verify the figure with `just mem-report` then.
    #[allow(dead_code)]
    pub(crate) struct LendArena {
        busy: AtomicBool,
        buf: UnsafeCell<[u8; ZENOH_TX_BUF]>,
    }

    // SAFETY: `busy` flag enforces exclusive access; only the loan
    // holder may mutate `buf` until commit/discard.
    unsafe impl Sync for LendArena {}

    impl LendArena {
        pub(super) const fn new() -> Self {
            Self {
                busy: AtomicBool::new(false),
                buf: UnsafeCell::new([0u8; ZENOH_TX_BUF]),
            }
        }

        // `clippy::mut_from_ref` is the right lint to raise here and the wrong
        // verdict: the `&mut` is minted from a `&self` because the CAS above is
        // what grants exclusivity, not the borrow. A caller that loses the CAS
        // gets `WouldBlock` and never sees the slice, so at most one `&mut`
        // exists at a time — which is the invariant the `unsafe impl Sync` is
        // written against. Surfaced 2026-08-24 by issue 0779: no lane clippies
        // `lending`, so this had never been raised at all.
        #[allow(clippy::mut_from_ref)]
        pub(super) fn try_claim(&self, len: usize) -> Result<&mut [u8], TransportError> {
            if len > ZENOH_TX_BUF {
                return Err(TransportError::TooLarge);
            }
            if self
                .busy
                .compare_exchange(false, true, CoreOrdering::AcqRel, CoreOrdering::Acquire)
                .is_err()
            {
                return Err(TransportError::WouldBlock);
            }
            // SAFETY: busy CAS won; exclusive access until release.
            let buf_ref: &mut [u8; ZENOH_TX_BUF] = unsafe { &mut *self.buf.get() };
            Ok(&mut buf_ref[..len])
        }

        pub(super) fn release(&self) {
            self.busy.store(false, CoreOrdering::Release);
        }
    }

    /// Backend-lent writable slot into ZenohPublisher's arena. Lifetime
    /// tied to `&'a ZenohPublisher` so it can't outlive the underlying
    /// zenoh session.
    pub struct ZenohSlot<'a> {
        bytes: &'a mut [u8],
        publisher: &'a ZenohPublisher,
    }

    impl<'a> AsMut<[u8]> for ZenohSlot<'a> {
        fn as_mut(&mut self) -> &mut [u8] {
            self.bytes
        }
    }

    impl<'a> ZenohSlot<'a> {
        /// Phase 124.A.4.b — truncate the slot to `actual_len` bytes.
        /// Called by the cffi `pub_commit` trampoline when the caller's
        /// `actual_len` is shorter than the originally loaned capacity.
        /// No-op when `actual_len >= self.bytes.len()`.
        pub fn truncate(&mut self, actual_len: usize) {
            if actual_len >= self.bytes.len() {
                return;
            }
            // SAFETY: re-borrow the same buffer with a shorter prefix.
            // The slot owns the busy-flag exclusivity so this is sound
            // for the lifetime of `self`.
            let ptr = self.bytes.as_mut_ptr();
            self.bytes = unsafe { core::slice::from_raw_parts_mut(ptr, actual_len) };
        }

        /// Issue 0812 — re-materialise a publisher's outstanding loan
        /// from nothing but its length.
        ///
        /// The arena is single-slot and `try_claim` always hands out a
        /// prefix of the same buffer, so a LIVE loan is fully described
        /// by its publisher plus the length it was granted. The cffi
        /// loan trampolines use that to carry a loan across the C
        /// boundary as an INTEGER token rather than a heap-allocated
        /// slot; this is the other half of that encoding.
        ///
        /// Dropping the returned slot releases the arena, exactly as
        /// dropping the original would have.
        ///
        /// # Safety
        /// * `publisher` must have exactly one loan outstanding — a
        ///   `try_lend_slot` that returned `Some` and has since been
        ///   neither committed nor discarded, so the busy flag is still
        ///   set and no other `ZenohSlot` for it exists.
        /// * `len` must be the length that loan was granted with.
        pub unsafe fn from_outstanding_loan(
            publisher: &'a ZenohPublisher,
            len: usize,
        ) -> ZenohSlot<'a> {
            // SAFETY: the busy flag is still set (the loan is
            // outstanding and this is its only holder), which is the
            // same grant of exclusivity `try_claim` mints its `&mut`
            // under.
            let buf: &'a mut [u8; ZENOH_TX_BUF] = unsafe { &mut *publisher.lend_arena.buf.get() };
            let end = len.min(ZENOH_TX_BUF);
            ZenohSlot {
                bytes: &mut buf[..end],
                publisher,
            }
        }
    }

    impl<'a> Drop for ZenohSlot<'a> {
        fn drop(&mut self) {
            // Always release the arena slot. commit_slot also calls
            // release indirectly via ownership transfer + drop.
            self.publisher.lend_arena.release();
        }
    }

    impl ZenohPublisher {
        // Wire the arena into the constructor — see Phase 99.F note in
        // `new()` for why this lives outside the main impl block.
        #[allow(dead_code)]
        pub(crate) const fn lend_arena_init() -> LendArena {
            LendArena::new()
        }
    }

    impl nros_rmw::SlotLending for ZenohPublisher {
        type Slot<'a>
            = ZenohSlot<'a>
        where
            Self: 'a;

        fn try_lend_slot(&self, len: usize) -> Result<Option<Self::Slot<'_>>, TransportError> {
            match self.lend_arena.try_claim(len) {
                Ok(bytes) => Ok(Some(ZenohSlot {
                    bytes,
                    publisher: self,
                })),
                Err(TransportError::WouldBlock) => Ok(None),
                Err(e) => Err(e),
            }
        }

        fn commit_slot(&self, slot: Self::Slot<'_>) -> Result<(), TransportError> {
            // Build the RMW attachment as in publish_raw.
            #[allow(clippy::useless_conversion)]
            let seq: i64 = (self.sequence_counter.fetch_add(1, Ordering::Relaxed) + 1).into();
            let ts = self.current_timestamp();
            let mut att_buf = [0u8; RMW_ATTACHMENT_SIZE];
            self.serialize_attachment(seq, ts, &mut att_buf);

            // Aliased publish: zenoh-pico calls z_bytes_from_static_buf
            // — no payload copy. Bytes consumed synchronously by
            // z_publisher_put before return on posix/embedded.
            let res = self
                .publisher
                .publish_with_attachment_aliased(slot.bytes, Some(&att_buf))
                .map_err(TransportError::from);
            // phase-455 W5 — the loan path DOES hold a contiguous payload (the
            // arena slice), so a transient-local publisher retains it here too.
            // The copy is the price of retention and is paid only by a
            // publisher that asked for TRANSIENT_LOCAL; the zero-copy claim is
            // about the WIRE, which is still aliased.
            if res.is_ok() {
                self.retain_sample(slot.bytes, &att_buf);
            }
            // slot drops here, releasing the arena.
            res
        }
    }
}

#[cfg(feature = "lending")]
pub use lending::{ZENOH_TX_BUF, ZenohSlot};

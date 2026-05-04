//! ZenohPublisher implementation

use portable_atomic::Ordering;

use nros_rmw::{Publisher, TransportError};

use super::{
    AtomicSeqCounter, Context, KEYEXPR_BUFFER_SIZE, KEYEXPR_STRING_SIZE, LivelinessToken,
    RMW_ATTACHMENT_SIZE, RMW_GID_SIZE, RmwAttachment, TIMESTAMP_INCREMENT_NS,
};
use crate::keyexpr::TopicKeyExpr;

#[cfg(feature = "safety-e2e")]
use super::RMW_ATTACHMENT_SIZE_WITH_CRC;

// ============================================================================
// ZenohPublisher
// ============================================================================

/// Zenoh publisher wrapping nros-rmw-zenoh ZenohPublisher
///
/// Includes RMW attachment support for rmw_zenoh compatibility.
pub struct ZenohPublisher {
    publisher: crate::zpico::Publisher<'static>,
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
    /// Wired but not fired today (zenoh tokens persist until
    /// undeclared; "liveliness lost" semantics need a per-publisher
    /// keepalive timer that's part of 108.C.zenoh.4 follow-up).
    liveliness_lost_cb: core::cell::Cell<Option<EventReg>>,
}

/// Phase 108.A — single-slot event registration. cb is `unsafe extern
/// "C" fn` (always Send); user_ctx outlives entity.
#[derive(Clone, Copy)]
struct EventReg {
    cb: nros_rmw::EventCallback,
    user_ctx: *mut core::ffi::c_void,
}

/// Phase 108.C.zenoh — read the platform clock in ms.
fn now_ms() -> u64 {
    use nros_platform::PlatformClock as _;
    <nros_platform::ConcretePlatform as nros_platform::PlatformClock>::clock_ms()
}

impl ZenohPublisher {
    /// Create a new publisher for the given topic
    pub fn new(
        context: &Context,
        topic: &nros_rmw::TopicInfo,
        liveliness: Option<LivelinessToken>,
        qos: &nros_rmw::QosSettings,
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
            let pub_result = context.declare_publisher(&keyexpr_buf);
            match pub_result {
                Ok(p) => core::mem::transmute::<
                    crate::zpico::Publisher<'_>,
                    crate::zpico::Publisher<'static>,
                >(p),
                Err(e) => return Err(TransportError::from(e)),
            }
        };

        let now = now_ms();
        Ok(Self {
            publisher,
            rmw_gid: RmwAttachment::generate_gid(),
            sequence_counter: AtomicSeqCounter::new(0),
            _liveliness: liveliness,
            #[cfg(feature = "lending")]
            lend_arena: lending::LendArena::new(),
            deadline_ms: qos.deadline_ms,
            last_publish_at_ms: core::cell::Cell::new(now),
            last_deadline_fire_ms: core::cell::Cell::new(now),
            deadline_total: core::cell::Cell::new(0),
            deadline_cb: core::cell::Cell::new(None),
            liveliness_lost_cb: core::cell::Cell::new(None),
        })
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

            self.publisher
                .publish_with_attachment(data, Some(&att_buf))
                .map_err(TransportError::from)
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

            self.publisher
                .publish_with_attachment(data, Some(&att_buf))
                .map_err(TransportError::from)
        };

        // Phase 108.C.zenoh.2 — only update last_publish_at on a
        // successful wire write so a failed publish doesn't reset the
        // deadline window.
        if result.is_ok() {
            self.last_publish_at_ms.set(now_ms());
        }
        result
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

    /// Per-publisher TX arena slot capacity.
    pub const ZENOH_TX_BUF: usize = 1024;

    /// Backend-owned arena for a `ZenohPublisher`. Single-slot;
    /// concurrent loans return Err(WouldBlock).
    pub(super) struct LendArena {
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
        pub(super) const fn lend_arena_init() -> LendArena {
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
            // slot drops here, releasing the arena.
            res
        }
    }
}

#[cfg(feature = "lending")]
pub use lending::{ZENOH_TX_BUF, ZenohSlot};

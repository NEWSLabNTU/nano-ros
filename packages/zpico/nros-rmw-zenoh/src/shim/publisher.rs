//! ZenohPublisher implementation

use portable_atomic::Ordering;

use nros_rmw::{Publisher, TransportError};

use super::{
    AtomicSeqCounter, Context, KEYEXPR_BUFFER_SIZE, KEYEXPR_STRING_SIZE, LivelinessToken,
    RMW_ATTACHMENT_SIZE, RMW_GID_SIZE, RmwAttachment, TIMESTAMP_INCREMENT_NS,
};
use crate::keyexpr::TopicKeyExpr;

#[cfg(feature = "safety-e2e")]
use super::{RMW_ATTACHMENT_SIZE_WITH_CRC, SAFETY_CRC_SIZE};

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
    /// Timestamp counter (until platform time is available)
    timestamp_counter: AtomicSeqCounter,
    /// Liveliness token for ROS 2 graph discovery (kept alive for publisher lifetime)
    _liveliness: Option<LivelinessToken>,
    /// Phase 95.F: per-publisher TX arena for SlotLending. Exists only
    /// when the `lending` feature is on.
    #[cfg(feature = "lending")]
    pub(super) lend_arena: lending::LendArena,
}

impl ZenohPublisher {
    /// Create a new publisher for the given topic
    pub fn new(
        context: &Context,
        topic: &nros_rmw::TopicInfo,
        liveliness: Option<LivelinessToken>,
    ) -> Result<Self, TransportError> {
        // Generate the topic key with null terminator
        let key: heapless::String<KEYEXPR_STRING_SIZE> = topic.to_key();

        #[cfg(feature = "std")]
        log::debug!("Publisher data keyexpr: {}", key.as_str());

        // Create null-terminated keyexpr
        let mut keyexpr_buf = [0u8; KEYEXPR_BUFFER_SIZE];
        let bytes = key.as_bytes();
        if bytes.len() >= keyexpr_buf.len() {
            return Err(TransportError::InvalidConfig);
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

        Ok(Self {
            publisher,
            rmw_gid: RmwAttachment::generate_gid(),
            sequence_counter: AtomicSeqCounter::new(0),
            timestamp_counter: AtomicSeqCounter::new(0),
            _liveliness: liveliness,
            #[cfg(feature = "lending")]
            lend_arena: lending::LendArena::new(),
        })
    }

    /// Get current timestamp in nanoseconds (placeholder until platform time available)
    fn current_timestamp(&self) -> i64 {
        // Increment by 1ms equivalent
        #[allow(clippy::useless_conversion)] // i32→i64 on embedded, no-op on std
        self.timestamp_counter
            .fetch_add(TIMESTAMP_INCREMENT_NS as _, Ordering::Relaxed)
            .into()
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
        // Get next sequence number and timestamp atomically
        #[allow(clippy::useless_conversion)] // i32→i64 on embedded, no-op on std
        let seq: i64 = (self.sequence_counter.fetch_add(1, Ordering::Relaxed) + 1).into();
        let ts = self.current_timestamp();

        // Without safety-e2e: 33-byte attachment
        #[cfg(not(feature = "safety-e2e"))]
        {
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
        }

        // With safety-e2e: 37-byte attachment (33 + 4-byte CRC of payload)
        #[cfg(feature = "safety-e2e")]
        {
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
        }
    }

    fn buffer_error(&self) -> Self::Error {
        TransportError::BufferTooSmall
    }

    fn serialization_error(&self) -> Self::Error {
        TransportError::SerializationError
    }
}

// ============================================================================
// Phase 95.F — ZenohPublisher SlotLending (zero-copy publish)
// ============================================================================

#[cfg(feature = "lending")]
mod lending {
    use super::*;
    use core::cell::UnsafeCell;
    use core::sync::atomic::{AtomicBool, Ordering as CoreOrdering};

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
                .compare_exchange(
                    false,
                    true,
                    CoreOrdering::AcqRel,
                    CoreOrdering::Acquire,
                )
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
        // Wire the arena into the constructor — see Phase 95.F note in
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

        fn try_lend_slot(
            &self,
            len: usize,
        ) -> Result<Option<Self::Slot<'_>>, TransportError> {
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
            let seq: i64 =
                (self.sequence_counter.fetch_add(1, Ordering::Relaxed) + 1).into();
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
pub use lending::{ZenohSlot, ZENOH_TX_BUF};

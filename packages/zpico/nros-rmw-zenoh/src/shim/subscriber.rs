//! ZenohSubscriber and ZenohZeroCopySubscriber implementations

use core::marker::PhantomData;

use atomic_waker::AtomicWaker;
use portable_atomic::{AtomicBool, AtomicUsize, Ordering};

use nros_rmw::{Subscriber, TransportError};

use super::{
    KEYEXPR_BUFFER_SIZE, KEYEXPR_STRING_SIZE, MessageInfo, SUBSCRIBER_ATTACHMENT_BUF_SIZE,
    SUBSCRIBER_BUFFER_SIZE,
};
use crate::keyexpr::TopicKeyExpr;
use crate::zpico::{Context, ZPICO_MAX_SUBSCRIBERS};

#[cfg(feature = "safety-e2e")]
use super::{RMW_ATTACHMENT_SIZE, SAFETY_CRC_SIZE};

#[cfg(feature = "std")]
use super::signal_executor_wake;

// ============================================================================
// SubscriberBuffer
// ============================================================================

/// Shared buffer for subscriber callbacks
///
/// This buffer stores the most recent message received by the subscriber,
/// including the RMW attachment data for MessageInfo support.
/// The callback writes to this buffer, and `try_recv_raw` reads from it.
pub(super) struct SubscriberBuffer {
    /// Buffer for received payload data (statically allocated)
    pub(super) data: [u8; SUBSCRIBER_BUFFER_SIZE],
    /// Buffer for received attachment data (33 or 37 bytes depending on safety-e2e)
    pub(super) attachment: [u8; SUBSCRIBER_ATTACHMENT_BUF_SIZE],
    /// Flag indicating new data is available
    pub(super) has_data: AtomicBool,
    /// Flag indicating the incoming message exceeded the buffer capacity.
    /// Set by the callback when `len > data.len()`. Checked by `try_recv_raw`
    /// which returns `Err(MessageTooLarge)` and clears this flag.
    pub(super) overflow: AtomicBool,
    /// Flag indicating a reader is currently accessing this buffer.
    /// Set by `try_recv_raw` / `process_raw_in_place` before reading, cleared
    /// after. The callback checks this flag and drops the message if locked,
    /// preventing a data race where the callback overwrites the buffer mid-read.
    pub(super) locked: AtomicBool,
    /// Length of valid payload data
    pub(super) len: AtomicUsize,
    /// Length of valid attachment data
    pub(super) attachment_len: AtomicUsize,
    /// Async waker — registered by `Future::poll()`, woken from callback
    /// when data arrives. Enables event-driven async without busy-polling.
    pub(super) waker: AtomicWaker,
}

impl SubscriberBuffer {
    pub(super) const fn new() -> Self {
        Self {
            data: [0u8; SUBSCRIBER_BUFFER_SIZE],
            attachment: [0u8; SUBSCRIBER_ATTACHMENT_BUF_SIZE],
            has_data: AtomicBool::new(false),
            overflow: AtomicBool::new(false),
            locked: AtomicBool::new(false),
            len: AtomicUsize::new(0),
            attachment_len: AtomicUsize::new(0),
            waker: AtomicWaker::new(),
        }
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
    /// Safety is guaranteed by the bounds check at construction time.
    /// All shared fields use atomic types, preventing data races.
    pub(super) fn get(&self) -> &SubscriberBuffer {
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

/// Notify callback invoked by the C shim after direct-write to the static buffer.
///
/// The payload is already in `SUBSCRIBER_BUFFERS[buffer_index].data`. This callback
/// only stores the length, attachment, and signals data availability.
extern "C" fn subscriber_notify_callback(
    len: usize,
    attachment: *const u8,
    attachment_len: usize,
    ctx: *mut core::ffi::c_void,
) {
    let buffer_index = ctx as usize;
    if buffer_index >= ZPICO_MAX_SUBSCRIBERS {
        return;
    }

    let mut buf_ref = SubscriberBufferRef {
        index: buffer_index,
    };
    let buffer = buf_ref.get_mut();

    if len > buffer.data.len() {
        // Overflow: the C shim called us with the oversized length so we can flag it.
        buffer.overflow.store(true, Ordering::Release);
        buffer.has_data.store(true, Ordering::Release);
    } else {
        // Payload already written by C shim — just store metadata
        buffer.overflow.store(false, Ordering::Release);
        buffer.len.store(len, Ordering::Release);

        // Copy attachment data if present
        if !attachment.is_null() && attachment_len > 0 {
            let att_copy_len = attachment_len.min(buffer.attachment.len());
            // Safety: attachment pointer is valid for att_copy_len bytes (from C shim)
            unsafe {
                core::ptr::copy_nonoverlapping(
                    attachment,
                    buffer.attachment.as_mut_ptr(),
                    att_copy_len,
                );
            }
            buffer.attachment_len.store(att_copy_len, Ordering::Release);
        } else {
            buffer.attachment_len.store(0, Ordering::Release);
        }

        buffer.has_data.store(true, Ordering::Release);
    }

    // Wake any async task waiting for data on this subscriber
    buffer.waker.wake();

    // Wake the executor spin loop (if waiting)
    #[cfg(feature = "std")]
    signal_executor_wake();
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
    /// Phantom to indicate we don't own the buffer
    _phantom: PhantomData<()>,
}

impl ZenohSubscriber {
    /// Create a new subscriber for the given topic
    pub fn new(
        context: &Context,
        topic: &nros_rmw::TopicInfo,
        liveliness: Option<super::LivelinessToken>,
    ) -> Result<Self, TransportError> {
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

        // Create subscriber with direct-write: the C shim reads payload directly
        // into SUBSCRIBER_BUFFERS[buffer_index].data via z_bytes_reader_read(),
        // avoiding the z_bytes_to_slice() malloc.
        let subscriber = unsafe {
            let buffer = buf.get_mut();
            let buf_ptr = buffer.data.as_mut_ptr();
            let buf_capacity = buffer.data.len();
            // AtomicBool is guaranteed to have the same in-memory representation
            // as bool on all Rust targets (size 1, align 1). The C shim reads
            // this via __atomic_load_n(ptr, __ATOMIC_ACQUIRE), which requires a
            // pointer to the underlying bool storage — hence the cast.
            let locked_ptr = buffer.locked.as_ptr() as *const bool;
            let sub_result = context.declare_subscriber_direct_write_raw(
                &keyexpr_buf,
                buf_ptr,
                buf_capacity,
                locked_ptr,
                subscriber_notify_callback,
                buffer_index as *mut core::ffi::c_void,
            );
            match sub_result {
                Ok(s) => core::mem::transmute::<
                    crate::zpico::Subscriber<'_>,
                    crate::zpico::Subscriber<'static>,
                >(s),
                Err(e) => return Err(TransportError::from(e)),
            }
        };

        Ok(Self {
            _subscriber: subscriber,
            buf,
            _liveliness: liveliness,
            #[cfg(feature = "safety-e2e")]
            safety_validator: nros_rmw::SafetyValidator::new(),
            _phantom: PhantomData,
        })
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

        if !buffer.has_data.load(Ordering::Acquire) {
            return Ok(None);
        }

        // Check for overflow
        if buffer.overflow.load(Ordering::Acquire) {
            buffer.overflow.store(false, Ordering::Release);
            buffer.has_data.store(false, Ordering::Release);
            return Err(TransportError::MessageTooLarge);
        }

        let len = buffer.len.load(Ordering::Acquire);
        if len > buf.len() {
            buffer.has_data.store(false, Ordering::Release);
            return Err(TransportError::BufferTooSmall);
        }

        // Lock buffer to prevent callback from overwriting during copy
        buffer.locked.store(true, Ordering::Release);

        // Copy payload data
        // Safety: data is valid up to len bytes, buffer is locked
        unsafe {
            core::ptr::copy_nonoverlapping(buffer.data.as_ptr(), buf.as_mut_ptr(), len);
        }

        // Parse attachment for sequence number and CRC
        let attachment_len = buffer.attachment_len.load(Ordering::Acquire);
        let (message_seq, crc_valid) = if attachment_len >= RMW_ATTACHMENT_SIZE {
            // Extract sequence number (bytes 0..8, LE)
            let att = &buffer.attachment;
            let seq = i64::from_le_bytes([
                att[0], att[1], att[2], att[3], att[4], att[5], att[6], att[7],
            ]);

            // Check for CRC (bytes 33..37)
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
                // No CRC in attachment (sender doesn't have safety-e2e)
                None
            };

            (seq, crc_result)
        } else {
            // No attachment at all — cannot validate
            (0, None)
        };

        buffer.locked.store(false, Ordering::Release);
        buffer.has_data.store(false, Ordering::Release);

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

        if !buffer.has_data.load(Ordering::Acquire) {
            return Ok(None);
        }

        // Check for overflow (message exceeded static buffer capacity)
        if buffer.overflow.load(Ordering::Acquire) {
            buffer.overflow.store(false, Ordering::Release);
            buffer.has_data.store(false, Ordering::Release);
            return Err(TransportError::MessageTooLarge);
        }

        let len = buffer.len.load(Ordering::Acquire);
        if len > buf.len() {
            // Clear has_data to avoid permanently stuck subscription — the oversized
            // message is dropped, but the subscription recovers on the next message.
            buffer.has_data.store(false, Ordering::Release);
            return Err(TransportError::BufferTooSmall);
        }

        // Lock buffer to prevent callback from overwriting during copy
        buffer.locked.store(true, Ordering::Release);

        // Copy payload data
        // Safety: Data is valid up to len bytes, buffer is locked
        unsafe {
            core::ptr::copy_nonoverlapping(buffer.data.as_ptr(), buf.as_mut_ptr(), len);
        }

        // Parse attachment if present
        let attachment_len = buffer.attachment_len.load(Ordering::Acquire);
        let message_info = if attachment_len > 0 {
            let attachment_slice = &buffer.attachment[..attachment_len];
            MessageInfo::from_attachment(attachment_slice)
        } else {
            None
        };

        buffer.locked.store(false, Ordering::Release);
        buffer.has_data.store(false, Ordering::Release);

        Ok(Some((len, message_info)))
    }

    /// Process the received message in-place with message info, without copying.
    ///
    /// Calls `f` with a reference to the raw CDR bytes and optional message info.
    /// The buffer is locked during `f`.
    ///
    /// Returns `Ok(true)` if a message was available and `f` was called.
    pub fn process_raw_in_place_with_info(
        &mut self,
        f: impl FnOnce(&[u8], Option<MessageInfo>),
    ) -> Result<bool, TransportError> {
        let buffer = self.buf.get();

        if !buffer.has_data.load(Ordering::Acquire) {
            return Ok(false);
        }

        if buffer.overflow.load(Ordering::Acquire) {
            buffer.overflow.store(false, Ordering::Release);
            buffer.has_data.store(false, Ordering::Release);
            return Err(TransportError::MessageTooLarge);
        }

        let len = buffer.len.load(Ordering::Acquire);

        buffer.locked.store(true, Ordering::Release);

        // Parse attachment while locked (attachment is small: 33-37 bytes)
        let attachment_len = buffer.attachment_len.load(Ordering::Acquire);
        let message_info = if attachment_len > 0 {
            let attachment_slice = &buffer.attachment[..attachment_len];
            MessageInfo::from_attachment(attachment_slice)
        } else {
            None
        };

        f(&buffer.data[..len], message_info);

        buffer.locked.store(false, Ordering::Release);
        buffer.has_data.store(false, Ordering::Release);

        Ok(true)
    }
}

impl Subscriber for ZenohSubscriber {
    type Error = TransportError;

    fn has_data(&self) -> bool {
        self.buf.get().has_data.load(Ordering::Acquire)
    }

    fn register_waker(&self, waker: &core::task::Waker) {
        self.buf.get().waker.register(waker);
    }

    fn try_recv_raw(&mut self, buf: &mut [u8]) -> Result<Option<usize>, Self::Error> {
        let buffer = self.buf.get();

        if !buffer.has_data.load(Ordering::Acquire) {
            return Ok(None);
        }

        // Check for overflow (message exceeded static buffer capacity)
        if buffer.overflow.load(Ordering::Acquire) {
            buffer.overflow.store(false, Ordering::Release);
            buffer.has_data.store(false, Ordering::Release);
            return Err(TransportError::MessageTooLarge);
        }

        let len = buffer.len.load(Ordering::Acquire);
        if len > buf.len() {
            // Clear has_data to avoid permanently stuck subscription — the oversized
            // message is dropped, but the subscription recovers on the next message.
            buffer.has_data.store(false, Ordering::Release);
            return Err(TransportError::BufferTooSmall);
        }

        // Lock buffer to prevent callback from overwriting during copy
        buffer.locked.store(true, Ordering::Release);

        // Copy data
        // Safety: Data is valid up to len bytes, buffer is locked
        unsafe {
            core::ptr::copy_nonoverlapping(buffer.data.as_ptr(), buf.as_mut_ptr(), len);
        }

        buffer.locked.store(false, Ordering::Release);
        buffer.has_data.store(false, Ordering::Release);

        Ok(Some(len))
    }

    fn process_raw_in_place(&mut self, f: impl FnOnce(&[u8])) -> Result<bool, Self::Error> {
        let buffer = self.buf.get();

        if !buffer.has_data.load(Ordering::Acquire) {
            return Ok(false);
        }

        if buffer.overflow.load(Ordering::Acquire) {
            buffer.overflow.store(false, Ordering::Release);
            buffer.has_data.store(false, Ordering::Release);
            return Err(TransportError::MessageTooLarge);
        }

        let len = buffer.len.load(Ordering::Acquire);

        // Lock buffer, process in-place, then unlock
        buffer.locked.store(true, Ordering::Release);
        f(&buffer.data[..len]);
        buffer.locked.store(false, Ordering::Release);
        buffer.has_data.store(false, Ordering::Release);

        Ok(true)
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
    use core::sync::atomic::Ordering as CoreOrdering;

    /// Backend-lent read-only view into the subscriber's static receive
    /// buffer. Holds the buffer's `locked` flag for the lifetime of the
    /// view so the C-side notify callback can't overwrite the bytes
    /// while the user is reading them. On drop the lock is released and
    /// `has_data` cleared (consume-on-borrow semantics, matching
    /// `try_recv_raw`).
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
            // Release the buffer lock first so `locked` is consistent
            // with `has_data`. The C callback gates writes on `locked`.
            self.buffer.locked.store(false, CoreOrdering::Release);
            self.buffer.has_data.store(false, CoreOrdering::Release);
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

            if !buffer.has_data.load(CoreOrdering::Acquire) {
                return Ok(None);
            }
            if buffer.overflow.load(CoreOrdering::Acquire) {
                buffer.overflow.store(false, CoreOrdering::Release);
                buffer.has_data.store(false, CoreOrdering::Release);
                return Err(TransportError::MessageTooLarge);
            }

            let len = buffer.len.load(CoreOrdering::Acquire);

            // Lock against the C callback before we hand out a borrow
            // into `buffer.data` so the callback can't overwrite the
            // bytes while the user is reading.
            buffer.locked.store(true, CoreOrdering::Release);

            // SAFETY: data is valid up to len; locked=true blocks the
            // notify callback from overwriting until ZenohView::drop.
            let bytes = unsafe { core::slice::from_raw_parts(buffer.data.as_ptr(), len) };

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
mod tests {
    use super::*;
    use nros_rmw::TransportError;

    // --- Subscription buffer helpers ---

    /// Simulate a subscription callback by writing directly to the subscriber buffer.
    /// Mirrors the logic in `subscriber_callback_with_attachment` (post-40.4: checks locked).
    pub(in crate::shim) fn simulate_subscription_callback(slot: usize, payload: &[u8]) {
        let mut buf_ref = SubscriberBufferRef::new(slot);
        let buffer = buf_ref.get_mut();

        // Post-40.4: check locked flag — drop message if reader is processing
        if buffer.locked.load(Ordering::Acquire) {
            return;
        }

        if payload.len() > buffer.data.len() {
            buffer.overflow.store(true, Ordering::Release);
            buffer.has_data.store(true, Ordering::Release);
        } else {
            buffer.overflow.store(false, Ordering::Release);
            buffer.data[..payload.len()].copy_from_slice(payload);
            buffer.len.store(payload.len(), Ordering::Release);
            buffer.attachment_len.store(0, Ordering::Release);
            buffer.has_data.store(true, Ordering::Release);
        }
    }

    /// Reset a subscriber buffer to idle state.
    pub(in crate::shim) fn reset_subscriber_buffer(slot: usize) {
        let mut buf_ref = SubscriberBufferRef::new(slot);
        let buffer = buf_ref.get_mut();
        buffer.has_data.store(false, Ordering::Release);
        buffer.overflow.store(false, Ordering::Release);
        buffer.locked.store(false, Ordering::Release);
        buffer.len.store(0, Ordering::Release);
        buffer.attachment_len.store(0, Ordering::Release);
    }

    /// Try to receive from a subscriber buffer slot.
    /// Replicates `try_recv_raw` logic for testing without a zenoh session.
    pub(in crate::shim) fn try_recv_subscription(
        slot: usize,
        recv_buf: &mut [u8],
    ) -> Result<Option<usize>, TransportError> {
        let buf_ref = SubscriberBufferRef::new(slot);
        let buffer = buf_ref.get();

        if !buffer.has_data.load(Ordering::Acquire) {
            return Ok(None);
        }

        if buffer.overflow.load(Ordering::Acquire) {
            buffer.overflow.store(false, Ordering::Release);
            buffer.has_data.store(false, Ordering::Release);
            return Err(TransportError::MessageTooLarge);
        }

        let len = buffer.len.load(Ordering::Acquire);
        if len > recv_buf.len() {
            buffer.has_data.store(false, Ordering::Release);
            return Err(TransportError::BufferTooSmall);
        }

        // Safety: Data is valid up to len bytes
        unsafe {
            core::ptr::copy_nonoverlapping(buffer.data.as_ptr(), recv_buf.as_mut_ptr(), len);
        }
        buffer.has_data.store(false, Ordering::Release);

        Ok(Some(len))
    }

    /// Process subscription data in-place (mirrors `process_raw_in_place` logic).
    fn process_in_place_subscription(
        slot: usize,
    ) -> Result<Option<alloc::vec::Vec<u8>>, TransportError> {
        let buf_ref = SubscriberBufferRef::new(slot);
        let buffer = buf_ref.get();

        if !buffer.has_data.load(Ordering::Acquire) {
            return Ok(None);
        }

        if buffer.overflow.load(Ordering::Acquire) {
            buffer.overflow.store(false, Ordering::Release);
            buffer.has_data.store(false, Ordering::Release);
            return Err(TransportError::MessageTooLarge);
        }

        let len = buffer.len.load(Ordering::Acquire);
        buffer.locked.store(true, Ordering::Release);

        // Read data in-place (equivalent to closure in process_raw_in_place)
        let data = buffer.data[..len].to_vec();

        buffer.locked.store(false, Ordering::Release);
        buffer.has_data.store(false, Ordering::Release);

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

        // State unchanged
        let buffer = SubscriberBufferRef::new(slot).get();
        assert!(!buffer.has_data.load(Ordering::Acquire));
        assert!(!buffer.overflow.load(Ordering::Acquire));
    }

    #[test]
    fn sub_buf_normal_delivery() {
        let slot = 1;
        reset_subscriber_buffer(slot);

        let payload = [0x42u8; 100];
        simulate_subscription_callback(slot, &payload);

        let buffer = SubscriberBufferRef::new(slot).get();
        assert!(buffer.has_data.load(Ordering::Acquire));
        assert!(!buffer.overflow.load(Ordering::Acquire));

        let mut recv_buf = [0u8; 1024];
        let result = try_recv_subscription(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(100))));
        assert_eq!(&recv_buf[..100], &payload);

        assert!(!buffer.has_data.load(Ordering::Acquire));
    }

    #[test]
    fn sub_buf_max_payload() {
        let slot = 2;
        reset_subscriber_buffer(slot);

        // Exactly 1024 bytes = max capacity
        let payload = [0xFFu8; 1024];
        simulate_subscription_callback(slot, &payload);

        let buffer = SubscriberBufferRef::new(slot).get();
        assert!(buffer.has_data.load(Ordering::Acquire));
        assert!(!buffer.overflow.load(Ordering::Acquire));

        let mut recv_buf = [0u8; 1024];
        let result = try_recv_subscription(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(1024))));
        assert_eq!(&recv_buf, &payload);
    }

    #[test]
    fn sub_buf_overflow_recovery() {
        let slot = 3;
        reset_subscriber_buffer(slot);

        // 2000 bytes exceeds 1024 capacity → overflow
        let payload = [0xAAu8; 2000];
        simulate_subscription_callback(slot, &payload);

        let buffer = SubscriberBufferRef::new(slot).get();
        assert!(buffer.has_data.load(Ordering::Acquire));
        assert!(buffer.overflow.load(Ordering::Acquire));

        let mut recv_buf = [0u8; 1024];
        let result = try_recv_subscription(slot, &mut recv_buf);
        assert!(matches!(result, Err(TransportError::MessageTooLarge)));

        // Both flags cleared
        assert!(!buffer.has_data.load(Ordering::Acquire));
        assert!(!buffer.overflow.load(Ordering::Acquire));

        // Recovery: next normal callback is accepted
        simulate_subscription_callback(slot, b"recovered");
        let result = try_recv_subscription(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(9))));
        assert_eq!(&recv_buf[..9], b"recovered");
    }

    #[test]
    fn sub_buf_caller_too_small() {
        let slot = 4;
        reset_subscriber_buffer(slot);

        // Store 512 bytes, try to receive into 256-byte buffer
        let payload = [0xBBu8; 512];
        simulate_subscription_callback(slot, &payload);

        let mut small_buf = [0u8; 256];
        let result = try_recv_subscription(slot, &mut small_buf);
        assert!(matches!(result, Err(TransportError::BufferTooSmall)));

        // has_data cleared (the oversized message is dropped)
        let buffer = SubscriberBufferRef::new(slot).get();
        assert!(!buffer.has_data.load(Ordering::Acquire));

        // Recovery: next callback accepted
        simulate_subscription_callback(slot, b"small");
        let mut recv_buf = [0u8; 1024];
        let result = try_recv_subscription(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(5))));
        assert_eq!(&recv_buf[..5], b"small");
    }

    #[test]
    fn sub_buf_overwrite_unread() {
        let slot = 5;
        reset_subscriber_buffer(slot);

        // Two callbacks without intervening recv
        simulate_subscription_callback(slot, b"first_msg");
        simulate_subscription_callback(slot, b"second_msg");

        // Only second message delivered (last-message-wins)
        let mut recv_buf = [0u8; 1024];
        let result = try_recv_subscription(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(10))));
        assert_eq!(&recv_buf[..10], b"second_msg");
    }

    #[test]
    fn sub_buf_double_consume() {
        let slot = 6;
        reset_subscriber_buffer(slot);

        simulate_subscription_callback(slot, b"data");

        let mut recv_buf = [0u8; 1024];
        let result = try_recv_subscription(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(4))));

        // Second recv returns None
        let result = try_recv_subscription(slot, &mut recv_buf);
        assert!(matches!(result, Ok(None)));
    }

    #[test]
    fn sub_buf_overflow_then_normal() {
        let slot = 7;
        reset_subscriber_buffer(slot);

        // Oversized → overflow error → normal → delivered
        simulate_subscription_callback(slot, &[0u8; 2000]);
        let mut recv_buf = [0u8; 1024];
        let result = try_recv_subscription(slot, &mut recv_buf);
        assert!(matches!(result, Err(TransportError::MessageTooLarge)));

        simulate_subscription_callback(slot, b"after_overflow");
        let result = try_recv_subscription(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(14))));
        assert_eq!(&recv_buf[..14], b"after_overflow");
    }

    #[test]
    fn sub_buf_zero_length_payload() {
        let slot = 0;
        reset_subscriber_buffer(slot);

        simulate_subscription_callback(slot, b"");

        let buffer = SubscriberBufferRef::new(slot).get();
        assert!(buffer.has_data.load(Ordering::Acquire));

        let mut recv_buf = [0u8; 1024];
        let result = try_recv_subscription(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(0))));
    }

    #[test]
    fn sub_buf_all_slots_independent() {
        let slot_a = 0;
        let slot_b = 7;
        reset_subscriber_buffer(slot_a);
        reset_subscriber_buffer(slot_b);

        simulate_subscription_callback(slot_a, b"slot_zero");
        simulate_subscription_callback(slot_b, b"slot_seven");

        // Consume slot_b first
        let mut recv_buf = [0u8; 1024];
        let result = try_recv_subscription(slot_b, &mut recv_buf);
        assert!(matches!(result, Ok(Some(10))));
        assert_eq!(&recv_buf[..10], b"slot_seven");

        // slot_a still has data
        let buffer_a = SubscriberBufferRef::new(slot_a).get();
        assert!(buffer_a.has_data.load(Ordering::Acquire));

        let result = try_recv_subscription(slot_a, &mut recv_buf);
        assert!(matches!(result, Ok(Some(9))));
        assert_eq!(&recv_buf[..9], b"slot_zero");
    }

    // ========================================================================
    // 40.4 Part E: In-place processing and lock correctness tests
    // ========================================================================

    #[test]
    fn sub_buf_in_place_matches_copy() {
        let slot = 0;
        reset_subscriber_buffer(slot);

        // Write 100-byte payload, try_recv (copy path) → capture bytes
        let payload = [0x42u8; 100];
        simulate_subscription_callback(slot, &payload);

        let mut recv_buf = [0u8; 1024];
        let copy_result = try_recv_subscription(slot, &mut recv_buf);
        assert!(matches!(copy_result, Ok(Some(100))));
        let copy_bytes = recv_buf[..100].to_vec();

        // Reset, write same payload, process_in_place → capture bytes
        reset_subscriber_buffer(slot);
        simulate_subscription_callback(slot, &payload);

        let in_place_result = process_in_place_subscription(slot);
        assert!(matches!(in_place_result, Ok(Some(_))));
        let in_place_bytes = in_place_result.unwrap().unwrap();

        // Both paths must produce identical data
        assert_eq!(copy_bytes, in_place_bytes);
    }

    #[test]
    fn sub_buf_in_place_overflow() {
        let slot = 1;
        reset_subscriber_buffer(slot);

        // Write oversized payload (2000 bytes > 1024 capacity)
        simulate_subscription_callback(slot, &[0xBBu8; 2000]);

        let result = process_in_place_subscription(slot);
        assert!(matches!(result, Err(TransportError::MessageTooLarge)));

        // Both flags cleared after overflow
        let buffer = SubscriberBufferRef::new(slot).get();
        assert!(!buffer.has_data.load(Ordering::Acquire));
        assert!(!buffer.overflow.load(Ordering::Acquire));
    }

    #[test]
    fn sub_buf_locked_drops_message() {
        let slot = 2;
        reset_subscriber_buffer(slot);

        // Write "original" payload
        let original = [0x11u8; 100];
        simulate_subscription_callback(slot, &original);
        let buf_ref = SubscriberBufferRef::new(slot);
        assert!(buf_ref.get().has_data.load(Ordering::Acquire));

        // Manually set locked=true (simulating in-place processing)
        buf_ref.get().locked.store(true, Ordering::Release);

        // Attempt callback with "replacement" — should be dropped
        let replacement = [0x22u8; 100];
        simulate_subscription_callback(slot, &replacement);

        // Buffer still contains original data (100 bytes of 0x11)
        let stored_len = buf_ref.get().len.load(Ordering::Acquire);
        assert_eq!(stored_len, 100);
        assert_eq!(&buf_ref.get().data[..100], &original);

        // Unlock and verify next callback succeeds
        buf_ref.get().locked.store(false, Ordering::Release);

        simulate_subscription_callback(slot, &replacement);
        let stored_len = buf_ref.get().len.load(Ordering::Acquire);
        assert_eq!(stored_len, 100);
        assert_eq!(&buf_ref.get().data[..100], &replacement);

        reset_subscriber_buffer(slot);
    }

    #[test]
    fn sub_buf_locked_state_during_in_place() {
        let slot = 3;
        reset_subscriber_buffer(slot);

        // Write payload to buffer
        simulate_subscription_callback(slot, b"test_lock_state");

        let buf_ref = SubscriberBufferRef::new(slot);

        // Verify locked=false before processing
        assert!(!buf_ref.get().locked.load(Ordering::Acquire));

        // Process in-place — during the closure the buffer should be locked
        let buffer = buf_ref.get();
        assert!(buffer.has_data.load(Ordering::Acquire));

        let len = buffer.len.load(Ordering::Acquire);
        buffer.locked.store(true, Ordering::Release);

        // While locked, verify the flag is set
        assert!(buffer.locked.load(Ordering::Acquire));

        // Read data (simulating closure)
        let _data = buffer.data[..len].to_vec();

        // Unlock and clear
        buffer.locked.store(false, Ordering::Release);
        buffer.has_data.store(false, Ordering::Release);

        // After processing: locked=false, has_data=false
        assert!(!buffer.locked.load(Ordering::Acquire));
        assert!(!buffer.has_data.load(Ordering::Acquire));
    }
}

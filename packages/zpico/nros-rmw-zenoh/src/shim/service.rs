//! ZenohServiceServer and ZenohServiceClient implementations

use core::marker::PhantomData;

use atomic_waker::AtomicWaker;
use portable_atomic::{AtomicBool, AtomicUsize, Ordering};

use nros_rmw::{
    ServiceClientTrait, ServiceInfo, ServiceRequest, ServiceServerTrait, TransportError,
};

use super::{
    AtomicSeqCounter, Context, KEYEXPR_BUFFER_SIZE, KEYEXPR_STRING_SIZE, SERVICE_BUFFER_SIZE,
};
use crate::keyexpr::ServiceKeyExpr;
use crate::zpico::{self, Queryable, ZPICO_MAX_QUERYABLES};

#[cfg(feature = "std")]
use super::signal_executor_wake;

// ============================================================================
// ServiceBuffer
// ============================================================================

/// Shared buffer for service server callbacks
pub(super) struct ServiceBuffer {
    /// Buffer for received request data
    pub(super) data: [u8; SERVICE_BUFFER_SIZE],
    /// Buffer for keyexpr (for reply)
    pub(super) keyexpr: [u8; 256],
    /// Flag indicating new request is available
    pub(super) has_request: AtomicBool,
    /// Flag indicating the incoming request exceeded the buffer capacity.
    /// Set by the callback when `payload_len > data.len()`. Checked by
    /// `try_recv_request` which returns `Err(MessageTooLarge)` and clears this flag.
    pub(super) overflow: AtomicBool,
    /// Length of valid data
    pub(super) len: AtomicUsize,
    /// Length of keyexpr
    pub(super) keyexpr_len: AtomicUsize,
    /// Sequence number (counter)
    pub(super) sequence_number: AtomicSeqCounter,
}

impl ServiceBuffer {
    pub(super) const fn new() -> Self {
        Self {
            data: [0u8; SERVICE_BUFFER_SIZE],
            keyexpr: [0u8; 256],
            has_request: AtomicBool::new(false),
            overflow: AtomicBool::new(false),
            len: AtomicUsize::new(0),
            keyexpr_len: AtomicUsize::new(0),
            sequence_number: AtomicSeqCounter::new(0),
        }
    }
}

/// Static buffers for service servers.
///
/// Count matches `ZPICO_MAX_QUERYABLES` from zpico-sys.
static mut SERVICE_BUFFERS: [ServiceBuffer; ZPICO_MAX_QUERYABLES] =
    [const { ServiceBuffer::new() }; ZPICO_MAX_QUERYABLES];

/// Next available service buffer index
static NEXT_SERVICE_BUFFER_INDEX: AtomicUsize = AtomicUsize::new(0);

// ============================================================================
// ServiceBufferRef — safe accessor wrapper
// ============================================================================

/// Safe accessor for a statically-allocated service buffer.
///
/// Encapsulates the `unsafe` access to `SERVICE_BUFFERS` by validating
/// the index once at construction time. Subsequent accesses via [`get()`]
/// are safe because the index is guaranteed in-bounds.
///
/// # Safety invariant
///
/// `SERVICE_BUFFERS` is a module-level `static mut` with a fixed address
/// and element count equal to `ZPICO_MAX_QUERYABLES`. The index is validated
/// at construction and never changes, so every `get()` / `get_mut()` call
/// dereferences a valid, in-bounds element.
pub(super) struct ServiceBufferRef {
    index: usize,
}

impl ServiceBufferRef {
    /// Create a new buffer reference with bounds validation.
    ///
    /// # Panics
    ///
    /// Panics if `index >= ZPICO_MAX_QUERYABLES`.
    pub(super) fn new(index: usize) -> Self {
        assert!(
            index < ZPICO_MAX_QUERYABLES,
            "service buffer index out of bounds: {index} >= {ZPICO_MAX_QUERYABLES}"
        );
        Self { index }
    }

    /// Get an immutable reference to the service buffer.
    ///
    /// Safety is guaranteed by the bounds check at construction time.
    /// All shared fields use atomic types, preventing data races.
    pub(super) fn get(&self) -> &ServiceBuffer {
        // Safety: index was validated at construction time.
        // SERVICE_BUFFERS is a module-level static with fixed address.
        unsafe { &SERVICE_BUFFERS[self.index] }
    }

    /// Get a mutable reference to the service buffer.
    ///
    /// Only called from callbacks, which are invoked synchronously
    /// (single-threaded) by zenoh-pico — no concurrent mutable access.
    pub(super) fn get_mut(&mut self) -> &mut ServiceBuffer {
        // Safety: index was validated at construction time.
        // Mutable access is only used by callbacks invoked synchronously
        // by zenoh-pico, so there are no concurrent mutable accesses.
        unsafe { &mut SERVICE_BUFFERS[self.index] }
    }
}

/// Sequence counter for service requests
pub(super) static SERVICE_SEQ_COUNTER: AtomicSeqCounter = AtomicSeqCounter::new(0);

/// Callback function invoked by the C shim when queries arrive
extern "C" fn queryable_callback(
    keyexpr: *const core::ffi::c_char,
    keyexpr_len: usize,
    payload: *const u8,
    payload_len: usize,
    ctx: *mut core::ffi::c_void,
) {
    let buffer_index = ctx as usize;
    if buffer_index >= ZPICO_MAX_QUERYABLES {
        return;
    }

    let mut buf_ref = ServiceBufferRef {
        index: buffer_index,
    };
    let buffer = buf_ref.get_mut();

    // Copy keyexpr
    let keyexpr_copy_len = keyexpr_len.min(buffer.keyexpr.len() - 1);
    // Safety: keyexpr pointer is valid for keyexpr_copy_len bytes (from C shim)
    unsafe {
        core::ptr::copy_nonoverlapping(
            keyexpr as *const u8,
            buffer.keyexpr.as_mut_ptr(),
            keyexpr_copy_len,
        );
    }
    buffer.keyexpr[keyexpr_copy_len] = 0; // Null terminate
    buffer
        .keyexpr_len
        .store(keyexpr_copy_len, Ordering::Release);

    if payload_len > buffer.data.len() {
        // Request exceeds static buffer capacity — flag as overflow.
        // Store keyexpr + sequence_number for diagnostics, but skip payload.
        buffer.overflow.store(true, Ordering::Release);
        let seq = SERVICE_SEQ_COUNTER.fetch_add(1, Ordering::Relaxed);
        buffer.sequence_number.store(seq, Ordering::Release);
        buffer.has_request.store(true, Ordering::Release);
    } else {
        // Normal case: copy payload
        buffer.overflow.store(false, Ordering::Release);
        if !payload.is_null() && payload_len > 0 {
            // Safety: payload pointer is valid for payload_len bytes (from C shim)
            unsafe {
                core::ptr::copy_nonoverlapping(payload, buffer.data.as_mut_ptr(), payload_len);
            }
        }
        buffer.len.store(payload_len, Ordering::Release);

        // Set sequence number
        let seq = SERVICE_SEQ_COUNTER.fetch_add(1, Ordering::Relaxed);
        buffer.sequence_number.store(seq, Ordering::Release);

        buffer.has_request.store(true, Ordering::Release);
    }

    // Wake the executor spin loop (if waiting)
    #[cfg(feature = "std")]
    signal_executor_wake();
}

// ============================================================================
// ZenohServiceServer
// ============================================================================

/// Zenoh service server using queryables
///
/// Receives service requests via queryable callbacks.
/// Note: The reply mechanism is limited due to the callback model.
pub struct ZenohServiceServer {
    /// The queryable handle (kept alive to maintain registration)
    _queryable: Queryable,
    /// Safe accessor for the static service buffer
    buf: ServiceBufferRef,
    /// Keyexpr buffer for replying (copied from last request)
    reply_keyexpr: [u8; 256],
    /// Keyexpr length
    reply_keyexpr_len: usize,
    /// Reference to context for replying
    context: *const Context,
    /// Phantom to indicate ownership
    _phantom: PhantomData<()>,
}

impl ZenohServiceServer {
    /// Create a new service server for the given service
    pub fn new(context: &Context, service: &ServiceInfo) -> Result<Self, TransportError> {
        // Allocate a buffer index
        let buffer_index = NEXT_SERVICE_BUFFER_INDEX.fetch_add(1, Ordering::SeqCst);
        if buffer_index >= ZPICO_MAX_QUERYABLES {
            NEXT_SERVICE_BUFFER_INDEX.fetch_sub(1, Ordering::SeqCst);
            return Err(TransportError::ServiceServerCreationFailed);
        }

        // Generate the service key
        let key: heapless::String<KEYEXPR_STRING_SIZE> = service.to_key();

        // Create null-terminated keyexpr
        let mut keyexpr_buf = [0u8; KEYEXPR_BUFFER_SIZE];
        let bytes = key.as_bytes();
        if bytes.len() >= keyexpr_buf.len() {
            return Err(TransportError::InvalidConfig);
        }
        keyexpr_buf[..bytes.len()].copy_from_slice(bytes);
        keyexpr_buf[bytes.len()] = 0;

        // Create queryable with callback
        let queryable = unsafe {
            context.declare_queryable_raw(
                &keyexpr_buf,
                queryable_callback,
                buffer_index as *mut core::ffi::c_void,
            )
        }
        .map_err(TransportError::from)?;

        Ok(Self {
            _queryable: queryable,
            buf: ServiceBufferRef::new(buffer_index),
            reply_keyexpr: [0u8; 256],
            reply_keyexpr_len: 0,
            context: context as *const Context,
            _phantom: PhantomData,
        })
    }
}

impl ServiceServerTrait for ZenohServiceServer {
    type Error = TransportError;

    fn has_request(&self) -> bool {
        self.buf.get().has_request.load(Ordering::Acquire)
    }

    fn try_recv_request<'a>(
        &mut self,
        buf: &'a mut [u8],
    ) -> Result<Option<ServiceRequest<'a>>, Self::Error> {
        let buffer = self.buf.get();

        if !buffer.has_request.load(Ordering::Acquire) {
            return Ok(None);
        }

        // Check for overflow (request exceeded static buffer capacity)
        if buffer.overflow.load(Ordering::Acquire) {
            buffer.overflow.store(false, Ordering::Release);
            buffer.has_request.store(false, Ordering::Release);
            return Err(TransportError::MessageTooLarge);
        }

        let len = buffer.len.load(Ordering::Acquire);
        if len > buf.len() {
            // Clear has_request to avoid permanently stuck service — the oversized
            // request is dropped, but the service recovers on the next request.
            buffer.has_request.store(false, Ordering::Release);
            return Err(TransportError::BufferTooSmall);
        }

        // Copy data and keyexpr under FFI guard to prevent callback from
        // overwriting the buffer mid-read (service buffers have no `locked` flag).
        zpico::ffi_guard(|| {
            // Safety: buffer data and keyexpr are valid up to their respective lengths
            unsafe {
                core::ptr::copy_nonoverlapping(buffer.data.as_ptr(), buf.as_mut_ptr(), len);

                // Save keyexpr for potential reply
                let keyexpr_len = buffer.keyexpr_len.load(Ordering::Acquire);
                core::ptr::copy_nonoverlapping(
                    buffer.keyexpr.as_ptr(),
                    self.reply_keyexpr.as_mut_ptr(),
                    keyexpr_len,
                );
                self.reply_keyexpr[keyexpr_len] = 0;
                self.reply_keyexpr_len = keyexpr_len;
            }
        });

        #[allow(clippy::useless_conversion)] // i32→i64 on embedded, no-op on std
        let seq: i64 = buffer.sequence_number.load(Ordering::Acquire).into();
        buffer.has_request.store(false, Ordering::Release);

        Ok(Some(ServiceRequest {
            data: &buf[..len],
            sequence_number: seq,
        }))
    }

    fn send_reply(&mut self, _sequence_number: i64, data: &[u8]) -> Result<(), Self::Error> {
        if self.reply_keyexpr_len == 0 {
            return Err(TransportError::ServiceReplyFailed);
        }

        // Get context reference
        let context = unsafe { &*self.context };

        // Send reply using the queryable handle and stored keyexpr
        context
            .query_reply(
                self._queryable.handle(),
                &self.reply_keyexpr[..=self.reply_keyexpr_len],
                data,
                None,
            )
            .map_err(|_| TransportError::ServiceReplyFailed)?;

        // Clear the stored keyexpr
        self.reply_keyexpr_len = 0;

        Ok(())
    }
}

// ============================================================================
// Reply Wakers (for async service client)
// ============================================================================

use crate::zpico::ZPICO_MAX_PENDING_GETS;

/// One AtomicWaker per pending get slot. Registered by `Promise::poll()`,
/// woken from the C shim when a reply arrives or the channel closes.
static REPLY_WAKERS: [AtomicWaker; ZPICO_MAX_PENDING_GETS] =
    [const { AtomicWaker::new() }; ZPICO_MAX_PENDING_GETS];

/// C callback invoked by zpico.c when a pending get slot gets a reply.
///
/// # Safety
///
/// Called from C (pending_get_reply_handler / pending_get_dropper).
/// `slot` must be in range [0, ZPICO_MAX_PENDING_GETS).
unsafe extern "C" fn reply_waker_callback(slot: i32) {
    if slot >= 0 && (slot as usize) < ZPICO_MAX_PENDING_GETS {
        REPLY_WAKERS[slot as usize].wake();
    }
}

/// Register the reply waker callback with the C shim.
///
/// Called once during session initialization.
pub(super) fn register_reply_waker() {
    unsafe {
        zpico_sys::zpico_set_reply_waker(Some(reply_waker_callback));
    }
}

// ============================================================================
// Service Client
// ============================================================================

/// Default timeout for service calls in milliseconds
const SERVICE_DEFAULT_TIMEOUT_MS: u32 = 5000;

/// Zenoh service client using z_get queries
///
/// Service clients send requests via z_get and receive responses from queryables.
pub struct ZenohServiceClient {
    /// Service key expression (null-terminated)
    keyexpr: [u8; 257],
    /// Length of valid keyexpr
    keyexpr_len: usize,
    /// Reference to context for making queries
    context: *const Context,
    /// Timeout in milliseconds
    timeout_ms: u32,
    /// Handle to a pending non-blocking get operation (None if idle)
    pending_handle: Option<i32>,
    /// Phantom to indicate ownership
    _phantom: PhantomData<()>,
}

impl ZenohServiceClient {
    /// Create a new service client for the given service
    pub fn new(context: &Context, service: &ServiceInfo) -> Result<Self, TransportError> {
        // Generate wildcard service key for queries (matches any type hash from ROS 2)
        let key: heapless::String<KEYEXPR_STRING_SIZE> = service.to_key_wildcard();

        // Create null-terminated keyexpr
        let mut keyexpr_buf = [0u8; KEYEXPR_BUFFER_SIZE];
        let bytes = key.as_bytes();
        if bytes.len() >= keyexpr_buf.len() {
            return Err(TransportError::InvalidConfig);
        }
        keyexpr_buf[..bytes.len()].copy_from_slice(bytes);
        keyexpr_buf[bytes.len()] = 0;

        #[cfg(feature = "std")]
        log::debug!("Service client keyexpr: {}", key.as_str());

        Ok(Self {
            keyexpr: keyexpr_buf,
            keyexpr_len: bytes.len(),
            context: context as *const Context,
            timeout_ms: SERVICE_DEFAULT_TIMEOUT_MS,
            pending_handle: None,
            _phantom: PhantomData,
        })
    }

    /// Set the timeout for service calls
    pub fn set_timeout(&mut self, timeout_ms: u32) {
        self.timeout_ms = timeout_ms;
    }
}

impl ServiceClientTrait for ZenohServiceClient {
    type Error = TransportError;

    fn register_waker(&self, waker: &core::task::Waker) {
        if let Some(handle) = self.pending_handle
            && (handle as usize) < ZPICO_MAX_PENDING_GETS
        {
            REPLY_WAKERS[handle as usize].register(waker);
        }
    }

    fn call_raw(&mut self, request: &[u8], reply_buf: &mut [u8]) -> Result<usize, Self::Error> {
        // Get context reference
        let context = unsafe { &*self.context };

        // Call z_get and wait for reply
        let result = context
            .get(
                &self.keyexpr[..=self.keyexpr_len],
                request,
                reply_buf,
                self.timeout_ms,
            )
            .map_err(TransportError::from)?;

        Ok(result)
    }

    fn send_request_raw(&mut self, request: &[u8]) -> Result<(), Self::Error> {
        let context = unsafe { &*self.context };

        let handle = context
            .get_start(&self.keyexpr[..=self.keyexpr_len], request, self.timeout_ms)
            .map_err(TransportError::from)?;

        self.pending_handle = Some(handle);
        Ok(())
    }

    fn try_recv_reply_raw(&mut self, reply_buf: &mut [u8]) -> Result<Option<usize>, Self::Error> {
        let handle = match self.pending_handle {
            Some(h) => h,
            None => return Ok(None),
        };

        let context = unsafe { &*self.context };

        match context.get_check(handle, reply_buf) {
            Ok(Some(len)) => {
                self.pending_handle = None;
                Ok(Some(len))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                self.pending_handle = None;
                Err(TransportError::from(e))
            }
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use nros_rmw::TransportError;

    // --- Service buffer helpers ---

    /// Simulate a service request callback by writing directly to the service buffer.
    pub(in crate::shim) fn simulate_service_request(slot: usize, payload: &[u8], keyexpr: &[u8]) {
        let mut buf_ref = ServiceBufferRef::new(slot);
        let buffer = buf_ref.get_mut();
        let copy_len = payload.len().min(buffer.data.len());
        buffer.data[..copy_len].copy_from_slice(&payload[..copy_len]);
        buffer.len.store(copy_len, Ordering::Release);

        let klen = keyexpr.len().min(buffer.keyexpr.len() - 1);
        buffer.keyexpr[..klen].copy_from_slice(&keyexpr[..klen]);
        buffer.keyexpr[klen] = 0;
        buffer.keyexpr_len.store(klen, Ordering::Release);

        let seq = SERVICE_SEQ_COUNTER.fetch_add(1, Ordering::Relaxed);
        buffer.sequence_number.store(seq, Ordering::Release);

        buffer.has_request.store(true, Ordering::Release);
    }

    /// Reset a service buffer to idle state.
    pub(in crate::shim) fn reset_service_buffer(slot: usize) {
        let mut buf_ref = ServiceBufferRef::new(slot);
        let buffer = buf_ref.get_mut();
        buffer.has_request.store(false, Ordering::Release);
        buffer.len.store(0, Ordering::Release);
        buffer.keyexpr_len.store(0, Ordering::Release);
    }

    /// Try to receive a service request from a buffer slot.
    /// Replicates `try_recv_request` logic for testing without a zenoh queryable.
    pub(in crate::shim) fn try_recv_service(
        slot: usize,
        recv_buf: &mut [u8],
    ) -> Result<Option<usize>, TransportError> {
        let buf_ref = ServiceBufferRef::new(slot);
        let buffer = buf_ref.get();

        if !buffer.has_request.load(Ordering::Acquire) {
            return Ok(None);
        }

        let len = buffer.len.load(Ordering::Acquire);
        if len > recv_buf.len() {
            buffer.has_request.store(false, Ordering::Release);
            return Err(TransportError::BufferTooSmall);
        }

        // Safety: Data is valid up to len bytes
        unsafe {
            core::ptr::copy_nonoverlapping(buffer.data.as_ptr(), recv_buf.as_mut_ptr(), len);
        }

        buffer.has_request.store(false, Ordering::Release);
        Ok(Some(len))
    }

    /// Read the keyexpr from a service buffer slot (for keyexpr preservation tests).
    fn read_service_keyexpr(slot: usize) -> heapless::Vec<u8, 256> {
        let buf_ref = ServiceBufferRef::new(slot);
        let buffer = buf_ref.get();
        let klen = buffer.keyexpr_len.load(Ordering::Acquire);
        let mut v = heapless::Vec::new();
        for i in 0..klen {
            let _ = v.push(buffer.keyexpr[i]);
        }
        v
    }

    /// Read the sequence number from a service buffer slot.
    fn read_service_seq(slot: usize) -> i64 {
        let buf_ref = ServiceBufferRef::new(slot);
        buf_ref.get().sequence_number.load(Ordering::Acquire).into()
    }

    // ========================================================================
    // 37.1: Service buffer bug fix tests
    // ========================================================================

    #[test]
    fn service_buf_oversized_request_clears_has_request() {
        let slot = 6;
        reset_service_buffer(slot);

        let payload = [0xABu8; 512];
        simulate_service_request(slot, &payload, b"test/service");

        let mut small_buf = [0u8; 256];
        let result = try_recv_service(slot, &mut small_buf);
        assert!(matches!(result, Err(TransportError::BufferTooSmall)));

        let buffer = ServiceBufferRef::new(slot).get();
        assert!(
            !buffer.has_request.load(Ordering::Acquire),
            "has_request must be cleared after BufferTooSmall to avoid stuck state"
        );

        simulate_service_request(slot, b"hello", b"test/service");
        let mut recv_buf = [0u8; 1024];
        let result = try_recv_service(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(5))));
        assert_eq!(&recv_buf[..5], b"hello");

        reset_service_buffer(slot);
    }

    #[test]
    fn service_buf_normal_request_after_stuck_recovery() {
        let slot = 5;
        reset_service_buffer(slot);

        simulate_service_request(slot, b"first", b"svc/a");
        let mut buf = [0u8; 1024];
        let result = try_recv_service(slot, &mut buf);
        assert!(matches!(result, Ok(Some(5))));
        assert_eq!(&buf[..5], b"first");

        let result = try_recv_service(slot, &mut buf);
        assert!(matches!(result, Ok(None)));

        simulate_service_request(slot, b"second", b"svc/a");
        let result = try_recv_service(slot, &mut buf);
        assert!(matches!(result, Ok(Some(6))));
        assert_eq!(&buf[..6], b"second");

        reset_service_buffer(slot);
    }

    // ========================================================================
    // 37.1a: Service buffer state machine tests
    // ========================================================================

    #[test]
    fn svc_buf_idle_poll() {
        let slot = 0;
        reset_service_buffer(slot);

        let mut buf = [0u8; 1024];
        let result = try_recv_service(slot, &mut buf);
        assert!(matches!(result, Ok(None)));

        let buffer = ServiceBufferRef::new(slot).get();
        assert!(!buffer.has_request.load(Ordering::Acquire));
    }

    #[test]
    fn svc_buf_normal_request() {
        let slot = 1;
        reset_service_buffer(slot);

        simulate_service_request(slot, b"request_data", b"svc/test");

        let buffer = ServiceBufferRef::new(slot).get();
        assert!(buffer.has_request.load(Ordering::Acquire));

        let mut recv_buf = [0u8; 1024];
        let result = try_recv_service(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(12))));
        assert_eq!(&recv_buf[..12], b"request_data");

        assert!(!buffer.has_request.load(Ordering::Acquire));
    }

    #[test]
    fn svc_buf_max_payload() {
        let slot = 2;
        reset_service_buffer(slot);

        // Exactly 1024 bytes = max capacity
        let payload = [0xCCu8; 1024];
        simulate_service_request(slot, &payload, b"svc/big");

        let mut recv_buf = [0u8; 1024];
        let result = try_recv_service(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(1024))));
        assert_eq!(&recv_buf, &payload);
    }

    #[test]
    fn svc_buf_caller_too_small_recovery() {
        let slot = 3;
        reset_service_buffer(slot);

        // Store 512 bytes, receive into 256-byte buffer
        let payload = [0xDDu8; 512];
        simulate_service_request(slot, &payload, b"svc/test");

        let mut small_buf = [0u8; 256];
        let result = try_recv_service(slot, &mut small_buf);
        assert!(matches!(result, Err(TransportError::BufferTooSmall)));

        // has_request cleared (post-fix behavior)
        let buffer = ServiceBufferRef::new(slot).get();
        assert!(!buffer.has_request.load(Ordering::Acquire));

        // Next request accepted
        simulate_service_request(slot, b"ok", b"svc/test");
        let mut recv_buf = [0u8; 1024];
        let result = try_recv_service(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(2))));
        assert_eq!(&recv_buf[..2], b"ok");
    }

    #[test]
    fn svc_buf_overwrite_unread() {
        let slot = 4;
        reset_service_buffer(slot);

        simulate_service_request(slot, b"first_req", b"svc/a");
        simulate_service_request(slot, b"second_req", b"svc/a");

        // Only second request delivered
        let mut recv_buf = [0u8; 1024];
        let result = try_recv_service(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(10))));
        assert_eq!(&recv_buf[..10], b"second_req");
    }

    #[test]
    fn svc_buf_double_consume() {
        let slot = 0;
        reset_service_buffer(slot);

        simulate_service_request(slot, b"once", b"svc/a");

        let mut recv_buf = [0u8; 1024];
        let result = try_recv_service(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(4))));

        let result = try_recv_service(slot, &mut recv_buf);
        assert!(matches!(result, Ok(None)));
    }

    #[test]
    fn svc_buf_sequence_numbers() {
        let slot = 7;
        reset_service_buffer(slot);

        // Three sequential requests — sequence numbers should increment
        simulate_service_request(slot, b"r1", b"svc/a");
        let seq1 = read_service_seq(slot);

        // Consume before next request
        let mut buf = [0u8; 1024];
        let _ = try_recv_service(slot, &mut buf);

        simulate_service_request(slot, b"r2", b"svc/a");
        let seq2 = read_service_seq(slot);
        let _ = try_recv_service(slot, &mut buf);

        simulate_service_request(slot, b"r3", b"svc/a");
        let seq3 = read_service_seq(slot);
        let _ = try_recv_service(slot, &mut buf);

        assert!(seq2 > seq1, "seq2 ({seq2}) should be > seq1 ({seq1})");
        assert!(seq3 > seq2, "seq3 ({seq3}) should be > seq2 ({seq2})");
    }

    #[test]
    fn svc_buf_keyexpr_preserved() {
        let slot = 1;
        reset_service_buffer(slot);

        let keyexpr = b"0/my_service/example_interfaces::srv::dds_::AddTwoInts/Reply";
        simulate_service_request(slot, b"payload", keyexpr);

        let stored = read_service_keyexpr(slot);
        assert_eq!(stored.as_slice(), keyexpr);

        // Consume and verify keyexpr was available during request
        let mut recv_buf = [0u8; 1024];
        let result = try_recv_service(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(7))));
    }

    #[test]
    fn svc_buf_all_slots_independent() {
        let slot_a = 0;
        let slot_b = 7;
        reset_service_buffer(slot_a);
        reset_service_buffer(slot_b);

        simulate_service_request(slot_a, b"req_zero", b"svc/0");
        simulate_service_request(slot_b, b"req_seven", b"svc/7");

        // Consume slot_b first
        let mut recv_buf = [0u8; 1024];
        let result = try_recv_service(slot_b, &mut recv_buf);
        assert!(matches!(result, Ok(Some(9))));
        assert_eq!(&recv_buf[..9], b"req_seven");

        // slot_a still has request
        let buffer_a = ServiceBufferRef::new(slot_a).get();
        assert!(buffer_a.has_request.load(Ordering::Acquire));

        let result = try_recv_service(slot_a, &mut recv_buf);
        assert!(matches!(result, Ok(Some(8))));
        assert_eq!(&recv_buf[..8], b"req_zero");
    }
}

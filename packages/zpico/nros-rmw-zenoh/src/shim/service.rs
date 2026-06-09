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
use crate::{
    keyexpr::ServiceKeyExpr,
    zpico::{self, Queryable, ZPICO_MAX_QUERYABLES},
};

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
    /// Phase 122.3.c.6.e — waker registered by event-driven service
    /// servers. Woken by `queryable_callback` after a request lands.
    pub(super) waker: AtomicWaker,
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
            waker: AtomicWaker::new(),
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
// `keyexpr as *const u8` is a no-op on platforms where `c_char == u8` (ARM) but
// a real reinterpret where `c_char == i8` (x86) — keep it for portability.
#[allow(clippy::unnecessary_cast)]
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

    // Drop empty-payload queries — they come from background discovery /
    // liveliness probes that zenoh-pico delivers through the same
    // queryable callback as real service requests. Flagging them as
    // `has_request` consumes the slot before the actual CDR-prefixed
    // request lands; the deserializer then trips on the empty buffer
    // and `handle_request` reports `ServiceReplyFailed`.
    if payload.is_null() || payload_len == 0 {
        return;
    }

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
        // Safety: payload pointer is valid for payload_len bytes (from C shim)
        unsafe {
            core::ptr::copy_nonoverlapping(payload, buffer.data.as_mut_ptr(), payload_len);
        }
        buffer.len.store(payload_len, Ordering::Release);

        // Set sequence number
        let seq = SERVICE_SEQ_COUNTER.fetch_add(1, Ordering::Relaxed);
        buffer.sequence_number.store(seq, Ordering::Release);

        buffer.has_request.store(true, Ordering::Release);
    }

    // Phase 122.3.c.6.e — wake any task that registered a Waker on
    // this server (event-driven callers).
    buffer.waker.wake();

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
    /// Liveliness token for ROS 2 graph discovery (kept alive for server lifetime)
    _liveliness: Option<super::LivelinessToken>,
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
    pub fn new(
        context: &Context,
        service: &ServiceInfo,
        liveliness: Option<super::LivelinessToken>,
    ) -> Result<Self, TransportError> {
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
            return Err(TransportError::TopicNameInvalid);
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
        .map_err(|e| {
            NEXT_SERVICE_BUFFER_INDEX.fetch_sub(1, Ordering::SeqCst);
            TransportError::from(e)
        })?;

        Ok(Self {
            _queryable: queryable,
            buf: ServiceBufferRef::new(buffer_index),
            _liveliness: liveliness,
            reply_keyexpr: [0u8; 256],
            reply_keyexpr_len: 0,
            context: context as *const Context,
            _phantom: PhantomData,
        })
    }

    pub(super) fn set_liveliness(&mut self, liveliness: Option<super::LivelinessToken>) {
        self._liveliness = liveliness;
    }
}

impl ServiceServerTrait for ZenohServiceServer {
    type Error = TransportError;

    fn has_request(&self) -> bool {
        self.buf.get().has_request.load(Ordering::Acquire)
    }

    fn register_waker(&self, waker: &core::task::Waker) {
        self.buf.get().waker.register(waker);
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

// SERVICE_DEFAULT_TIMEOUT_MS is generated by build.rs from the
// NROS_SERVICE_TIMEOUT_MS env var (default 30000).
use crate::config::SERVICE_DEFAULT_TIMEOUT_MS;

/// Zenoh service client using z_get queries
///
/// Service clients send requests via z_get and receive responses from queryables.
pub struct ZenohServiceClient {
    /// Service key expression (null-terminated)
    keyexpr: [u8; 257],
    /// Length of valid keyexpr
    keyexpr_len: usize,
    /// Wildcard liveliness keyexpr matching any service-server token for
    /// this service (null-terminated). Used by `start_server_discovery`.
    discovery_keyexpr: [u8; 257],
    /// Length of valid `discovery_keyexpr`.
    discovery_keyexpr_len: usize,
    /// Latched result of the most recent `start_server_discovery`/poll
    /// pair. Set to `Some(true)` once the first liveliness reply arrives
    /// so subsequent `is_server_ready` calls can answer without a round
    /// trip. Reset to `None` when discovery hasn't been started.
    server_seen: bool,
    /// Slot handle of an in-flight liveliness query (None if idle).
    discovery_handle: Option<i32>,
    /// Liveliness token for ROS 2 graph discovery (kept alive for client lifetime)
    _liveliness: Option<super::LivelinessToken>,
    /// Reference to context for making queries
    context: *const Context,
    /// Timeout in milliseconds
    timeout_ms: u32,
    /// Handles for outstanding non-blocking get operations.
    ///
    /// Was `Option<i32>` (single handle). The C-API blocking
    /// `nros_client_call` resends the request every ~500 ms during a
    /// discovery race (Phase 89.12 cold-boot fix), each resend calling
    /// `send_request_raw` → `zpico_get_start` → fresh slot. Storing only
    /// the latest handle dropped the older slots: when the server's
    /// reply finally arrived on slot N (older than the current handle),
    /// `pending_get_reply_handler` set `received=true` on slot N but
    /// nothing polled it. The slot eventually had its dropper fire on
    /// zenoh-pico's query timeout (`Z_CONFIG_SOCKET_TIMEOUT`, 5 s on
    /// Zephyr), so `zpico_get_check` never returned the data to the
    /// caller. Tracking ALL outstanding handles + polling each in
    /// `try_recv_reply_raw` returns the first reply that lands,
    /// regardless of which generation of resend produced it.
    /// Capacity matches the C-side slot pool so we can never lose a
    /// handle the C allocator successfully returned.
    pending_handles: heapless::Vec<i32, ZPICO_MAX_PENDING_GETS>,
    /// Phantom to indicate ownership
    _phantom: PhantomData<()>,
}

impl ZenohServiceClient {
    /// Create a new service client for the given service
    pub fn new(
        context: &Context,
        service: &ServiceInfo,
        liveliness: Option<super::LivelinessToken>,
    ) -> Result<Self, TransportError> {
        // Generate wildcard service key for queries (matches any type hash from ROS 2).
        let key: heapless::String<KEYEXPR_STRING_SIZE> = service.to_key_wildcard();

        // Create null-terminated keyexpr
        let mut keyexpr_buf = [0u8; KEYEXPR_BUFFER_SIZE];
        let bytes = key.as_bytes();
        if bytes.len() >= keyexpr_buf.len() {
            return Err(TransportError::TopicNameInvalid);
        }
        keyexpr_buf[..bytes.len()].copy_from_slice(bytes);
        keyexpr_buf[bytes.len()] = 0;

        // Build the wildcard liveliness keyexpr we'll query in
        // `start_server_discovery`. Null-terminate for the C shim.
        let liv: heapless::String<KEYEXPR_STRING_SIZE> =
            super::Ros2Liveliness::service_server_keyexpr_wildcard(service.domain_id, service);
        let mut discovery_buf = [0u8; KEYEXPR_BUFFER_SIZE];
        let liv_bytes = liv.as_bytes();
        if liv_bytes.len() >= discovery_buf.len() {
            return Err(TransportError::TopicNameInvalid);
        }
        discovery_buf[..liv_bytes.len()].copy_from_slice(liv_bytes);
        discovery_buf[liv_bytes.len()] = 0;

        #[cfg(feature = "std")]
        log::debug!("Service client keyexpr: {}", key.as_str());

        Ok(Self {
            keyexpr: keyexpr_buf,
            keyexpr_len: bytes.len(),
            discovery_keyexpr: discovery_buf,
            discovery_keyexpr_len: liv_bytes.len(),
            server_seen: false,
            discovery_handle: None,
            _liveliness: liveliness,
            context: context as *const Context,
            timeout_ms: SERVICE_DEFAULT_TIMEOUT_MS,
            pending_handles: heapless::Vec::new(),
            _phantom: PhantomData,
        })
    }

    /// Set the timeout for service calls
    pub fn set_timeout(&mut self, timeout_ms: u32) {
        self.timeout_ms = timeout_ms;
    }

    /// Append a newly-allocated slot handle to the outstanding list.
    ///
    /// When the list is full we drop the OLDEST handle, not the new
    /// one — the C side has handed us a real slot and refusing to
    /// remember it would lose its reply. The dropped handle's reply
    /// (if any) is forfeited; that slot is recycled by the C
    /// allocator once its dropper fires (zenoh-pico query timeout).
    /// In practice this only triggers when `nros_client_call`'s
    /// resend loop produces more than `ZPICO_MAX_PENDING_GETS`
    /// generations in a single user-visible call — unusual.
    fn track_outstanding(&mut self, handle: i32) {
        if self.pending_handles.is_full() {
            self.pending_handles.remove(0);
        }
        // Cannot fail — we just made room above.
        let _ = self.pending_handles.push(handle);
    }
}

impl ServiceClientTrait for ZenohServiceClient {
    type Error = TransportError;

    #[allow(deprecated)]
    fn call_raw(&mut self, request: &[u8], reply_buf: &mut [u8]) -> Result<usize, Self::Error> {
        self.send_request_raw(request)?;

        #[cfg(feature = "std")]
        {
            let deadline = std::time::Instant::now()
                + std::time::Duration::from_millis(self.timeout_ms as u64);
            loop {
                if let Some(len) = self.try_recv_reply_raw(reply_buf)? {
                    return Ok(len);
                }
                if std::time::Instant::now() >= deadline {
                    self.pending_handles.clear();
                    return Err(TransportError::Timeout);
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }

        #[cfg(not(feature = "std"))]
        {
            #[cfg(not(feature = "platform-threadx"))]
            unsafe extern "C" {
                fn z_sleep_ms(time: usize) -> i8;
            }
            let attempts = (self.timeout_ms / 5).max(1);
            for _ in 0..attempts {
                if let Some(len) = self.try_recv_reply_raw(reply_buf)? {
                    return Ok(len);
                }
                #[cfg(feature = "platform-threadx")]
                unsafe {
                    let _ = zpico_sys::zpico_spin_once(5);
                }
                #[cfg(not(feature = "platform-threadx"))]
                unsafe {
                    z_sleep_ms(5)
                };
            }
            self.pending_handles.clear();
            Err(TransportError::Timeout)
        }
    }

    fn register_waker(&self, waker: &core::task::Waker) {
        // Wake on any outstanding handle — `nros_client_call`'s resend
        // can leave several gens in flight; any of them could complete
        // first (see `pending_handles` docs).
        for &handle in &self.pending_handles {
            if (handle as usize) < ZPICO_MAX_PENDING_GETS {
                REPLY_WAKERS[handle as usize].register(waker);
            }
        }
    }

    fn send_request_raw(&mut self, request: &[u8]) -> Result<(), Self::Error> {
        let context = unsafe { &*self.context };

        // Phase 89.12 #14 + Phase 89.13 flake fix: retry `zpico_get_start`
        // with a bounded wall-clock budget to cover two distinct race
        // classes on multi-threaded zpico backends (POSIX / Zephyr /
        // NuttX / FreeRTOS+lwIP):
        //
        // 1. **Dropper-pending race** (tens of microseconds). A z_get
        //    issued while zenoh-pico is mid-finalization of a *previous*
        //    query — the dropper callback for the prior get_check is
        //    enqueued but hasn't run yet — can be transiently rejected
        //    by the session's pending-query table. Typical surface:
        //        let (_, mut p) = client.send_goal(&g)?;
        //        ... p.try_recv() sees the accept reply ...
        //        let r = client.get_result(&id)?;  // flaked here
        //    Resolves within a few μs once the scheduler runs the
        //    lease / read tasks.
        //
        // 2. **Cold-boot discovery race** (hundreds of milliseconds).
        //    On NuttX QEMU cold start, the Rust client boots in
        //    parallel with the server (the test harness can't delay
        //    the in-guest client, and the pubsub shape already
        //    requires parallel launch). The first `call()` can fire
        //    before zenoh-pico has discovered the server's queryable
        //    via router gossip. 3 tight retries all hit the same
        //    unresolved state within microseconds — the test saw
        //    `Application error: ServiceRequestFailed` as the first
        //    call on NuttX Rust service / action E2E.
        //
        // 800 ms total budget on std covers both cases comfortably
        // (cold-boot discovery empirically lands in 200–600 ms on
        // QEMU NuttX). Between attempts, yield ~5 ms via
        // `thread::sleep` so zenoh-pico's background pthread(s) can
        // advance the session state — spin-looping here starves the
        // lease / read task on single-core QEMU hosts. On no_std
        // fallback we keep the original tight 3-retry count: bare
        // metal / single-threaded zpico has no parallel progress to
        // wait on, and the dropper-pending race there is the only
        // reproducible failure mode.
        // rustc warns "value assigned to `last_err` is never read" because
        // only the *last* assignment in the loop is observable, and the
        // happy path exits via `return Ok(())`. Suppress — the value IS
        // read on the timeout/exhaustion fallthrough at the bottom.
        #[allow(unused_assignments)]
        let mut last_err = None;
        #[cfg(feature = "std")]
        {
            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(800);
            loop {
                match context.get_start(
                    &self.keyexpr[..=self.keyexpr_len],
                    request,
                    self.timeout_ms,
                ) {
                    Ok(handle) => {
                        self.track_outstanding(handle);
                        return Ok(());
                    }
                    Err(e) => last_err = Some(e),
                }
                if std::time::Instant::now() >= deadline {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
        #[cfg(not(feature = "std"))]
        {
            // 80 × 5 ms = 400 ms budget. Covers cold-boot discovery on
            // multi-threaded zpico backends (FreeRTOS+lwIP, ThreadX+NetX)
            // where the lease / read task needs scheduler quanta to
            // advance the session state past pending-query / queryable
            // gossip. `z_sleep_ms` yields cooperatively on those
            // backends; on bare-metal single-threaded zpico it's a
            // busy-loop fallback but the count keeps it bounded.
            #[cfg(not(feature = "platform-threadx"))]
            unsafe extern "C" {
                fn z_sleep_ms(time: usize) -> i8;
            }
            const MAX_ATTEMPTS: u32 = 80;
            const SLEEP_MS: usize = 5;
            for attempt in 0..MAX_ATTEMPTS {
                match context.get_start(
                    &self.keyexpr[..=self.keyexpr_len],
                    request,
                    self.timeout_ms,
                ) {
                    Ok(handle) => {
                        self.track_outstanding(handle);
                        return Ok(());
                    }
                    Err(e) => last_err = Some(e),
                }
                if attempt + 1 < MAX_ATTEMPTS {
                    #[cfg(feature = "platform-threadx")]
                    unsafe {
                        let _ = zpico_sys::zpico_spin_once(SLEEP_MS as u32);
                    }
                    #[cfg(not(feature = "platform-threadx"))]
                    unsafe {
                        z_sleep_ms(SLEEP_MS)
                    };
                }
            }
        }
        Err(TransportError::from(last_err.unwrap()))
    }

    fn try_recv_reply_raw(&mut self, reply_buf: &mut [u8]) -> Result<Option<usize>, Self::Error> {
        if self.pending_handles.is_empty() {
            return Ok(None);
        }

        let context = unsafe { &*self.context };

        #[cfg(not(feature = "std"))]
        {
            let _ = context.spin_once(0);
        }

        // Poll every outstanding handle. First reply wins — the
        // resend loop in the C-API blocking caller leaves multiple
        // generations of the same logical request in flight, and any
        // one of them is a valid response (queryable is idempotent at
        // the application layer; the server logs request count but
        // computes the same answer). See `pending_handles` docs.
        //
        // Newest first matches the common case where the latest send
        // is what completed — most calls allocate only one slot, so
        // we get out in one iteration.
        let mut hit_idx: Option<usize> = None;
        let mut hit_len: usize = 0;
        let mut hard_err: Option<Self::Error> = None;
        for (idx, &handle) in self.pending_handles.iter().enumerate().rev() {
            match context.get_check(handle, reply_buf) {
                Ok(Some(len)) => {
                    hit_idx = Some(idx);
                    hit_len = len;
                    break;
                }
                Ok(None) => continue,
                Err(e) => {
                    // Note the error but keep checking the others —
                    // one slot's dropper-only timeout shouldn't lose
                    // a sibling's still-pending reply. If everyone
                    // errored we'll surface the last one.
                    hard_err = Some(TransportError::from(e));
                }
            }
        }

        if hit_idx.is_some() {
            // A reply landed — release every other outstanding slot.
            // They'll drain via zenoh-pico's dropper on the query
            // timeout; we just stop polling them.
            self.pending_handles.clear();
            return Ok(Some(hit_len));
        }

        if let Some(e) = hard_err {
            // Every outstanding handle errored (e.g. each got a
            // dropper-only timeout without data). Surface the failure.
            self.pending_handles.clear();
            return Err(e);
        }

        Ok(None)
    }

    fn start_server_discovery(&mut self, timeout_ms: u32) -> Result<(), Self::Error> {
        // Idempotent: a previous query in flight is fine — let it run.
        if self.discovery_handle.is_some() {
            return Ok(());
        }
        // Already proven the server is visible; no need to re-query.
        if self.server_seen {
            return Ok(());
        }
        let context = unsafe { &*self.context };
        let handle = context
            .liveliness_get_start(
                &self.discovery_keyexpr[..=self.discovery_keyexpr_len],
                timeout_ms,
            )
            .map_err(TransportError::from)?;
        self.discovery_handle = Some(handle);
        Ok(())
    }

    fn poll_server_discovery(&mut self) -> Result<Option<bool>, Self::Error> {
        // Latched success: once we've seen a token, the server is "ready"
        // for the rest of this client's lifetime. ROS 2's discovery model
        // doesn't require us to re-prove a server's existence on every
        // call (rclcpp's `service_is_ready` snapshot semantic); if the
        // server later goes away, individual `call()`s will time out at
        // the reply side.
        if self.server_seen {
            return Ok(Some(true));
        }
        let handle = match self.discovery_handle {
            Some(h) => h,
            None => return Ok(Some(false)),
        };
        let context = unsafe { &*self.context };
        match context.liveliness_get_check(handle) {
            Ok(true) => {
                self.discovery_handle = None;
                self.server_seen = true;
                Ok(Some(true))
            }
            Ok(false) => Ok(None),
            Err(crate::zpico::ZpicoError::Timeout) => {
                // Dropper fired with no replies — no server seen.
                self.discovery_handle = None;
                Ok(Some(false))
            }
            Err(e) => {
                self.discovery_handle = None;
                Err(TransportError::from(e))
            }
        }
    }

    fn is_server_ready(&self) -> bool {
        self.server_seen
    }

    fn server_available(&self) -> Result<bool, TransportError> {
        // Phase 124.C.2 — zenoh-pico tracks matched queryables via the
        // session's liveliness subscription. `server_seen` already
        // reflects "at least one matching queryable advertised", which
        // is the answer this probe wants.
        Ok(self.server_seen)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
pub(super) mod tests {
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

        // Phase 212.x3 — `ServiceBufferRef::get` returns `&ServiceBuffer` tied to
        // the lifetime of `&self`, so the temporary must outlive the borrow.
        let buf_ref = ServiceBufferRef::new(slot);
        let buffer = buf_ref.get();
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

        let buf_ref = ServiceBufferRef::new(slot);
        let buffer = buf_ref.get();
        assert!(!buffer.has_request.load(Ordering::Acquire));
    }

    #[test]
    fn svc_buf_normal_request() {
        let slot = 1;
        reset_service_buffer(slot);

        simulate_service_request(slot, b"request_data", b"svc/test");

        let buf_ref = ServiceBufferRef::new(slot);
        let buffer = buf_ref.get();
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
        let buf_ref = ServiceBufferRef::new(slot);
        let buffer = buf_ref.get();
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
        let buf_ref_a = ServiceBufferRef::new(slot_a);
        let buffer_a = buf_ref_a.get();
        assert!(buffer_a.has_request.load(Ordering::Acquire));

        let result = try_recv_service(slot_a, &mut recv_buf);
        assert!(matches!(result, Ok(Some(8))));
        assert_eq!(&recv_buf[..8], b"req_zero");
    }
}

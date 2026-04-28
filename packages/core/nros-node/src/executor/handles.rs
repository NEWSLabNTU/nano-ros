//! Entity wrapper types for the embedded executor.

use core::marker::PhantomData;

use nros_core::{CdrReader, CdrWriter, Deserialize, RosAction, RosMessage, RosService, Serialize};
use nros_rmw::{Publisher, ServiceClientTrait, ServiceServerTrait, Subscriber, TransportError};

use crate::session;

use super::types::{DEFAULT_TX_BUF, NodeError};

/// Default polling interval (ms) for sync wait loops.
const DEFAULT_SPIN_INTERVAL_MS: u64 = 10;

/// Check whether the given budget has been exhausted.
///
/// `std` builds measure wall-clock against `Instant::now()`; `no_std`
/// builds count iterations and exhaust after `max_iterations` calls.
///
/// **Phase 89.8**: the plain `max_iters` approach is insufficient on
/// multi-threaded zpico backends (POSIX/Zephyr/NuttX). There,
/// `executor.spin_once(10ms)` waits on a condvar that zenoh-pico's
/// background tasks signal on any inbound frame (keep-alives,
/// discovery gossip, interest messages). Each signal returns the
/// spin well before the 10 ms budget, so a nominal
/// `1000 × 10 ms = 10 s` iteration count collapses to milliseconds
/// of real time and the wait returns `Timeout` long before the
/// awaited reply can arrive.
///
/// Same class of bug 89.2 fixed in `nros-c`'s blocking service call
/// and 89.3 fixed in `nros-cpp`'s action-client helpers. The
/// maintainer explicitly flagged this `Promise::wait` / `wait_next`
/// path in the 89.2 commit: *"Promise::wait in nros-node has the
/// same structural bug but currently passes all tests. Left on
/// max_spins until a test surfaces it."* The NuttX Rust action
/// E2E is that test.
struct WaitBudget {
    #[cfg(feature = "std")]
    deadline: std::time::Instant,
    // For no_std + rmw-zenoh, we can get real wall-clock milliseconds
    // via `z_clock_now()` (exported unconditionally by
    // `zpico-platform-shim`). This is the same primitive nros-cpp
    // uses in Phase 89.3. The fallback `remaining: u64` counter is
    // only reached on builds that have neither `std` nor
    // `rmw-zenoh`, which in practice means a hypothetical pure
    // rmw-xrce no_std build — that path is load-sensitive but
    // has no active test triggering it today.
    #[cfg(all(not(feature = "std"), feature = "rmw-zenoh"))]
    deadline_ms: u64,
    #[cfg(all(not(feature = "std"), not(feature = "rmw-zenoh")))]
    remaining: u64,
}

#[cfg(all(not(feature = "std"), feature = "rmw-zenoh"))]
unsafe extern "C" {
    fn z_clock_now() -> usize;
}

impl WaitBudget {
    fn new(_max_iterations: u64, _timeout: core::time::Duration) -> Self {
        #[cfg(feature = "std")]
        {
            Self {
                deadline: std::time::Instant::now() + _timeout,
            }
        }
        #[cfg(all(not(feature = "std"), feature = "rmw-zenoh"))]
        {
            let now_ms = unsafe { z_clock_now() } as u64;
            let ms = _timeout.as_millis().min(u64::MAX as u128) as u64;
            Self {
                deadline_ms: now_ms.saturating_add(ms),
            }
        }
        #[cfg(all(not(feature = "std"), not(feature = "rmw-zenoh")))]
        {
            Self {
                remaining: _max_iterations,
            }
        }
    }

    fn tick(&mut self) -> bool {
        #[cfg(feature = "std")]
        {
            std::time::Instant::now() < self.deadline
        }
        #[cfg(all(not(feature = "std"), feature = "rmw-zenoh"))]
        {
            let now_ms = unsafe { z_clock_now() } as u64;
            now_ms < self.deadline_ms
        }
        #[cfg(all(not(feature = "std"), not(feature = "rmw-zenoh")))]
        {
            if self.remaining == 0 {
                false
            } else {
                self.remaining -= 1;
                true
            }
        }
    }
}

/// UUID byte count in a ROS 2 GoalId.
///
/// CDR encoding: 4-byte sequence-length prefix (`read_u32`) + 16 UUID bytes.
const GOAL_UUID_SIZE: usize = 16;

// ============================================================================
// EmbeddedPublisher
// ============================================================================

/// Typed publisher handle.
pub struct EmbeddedPublisher<M> {
    pub(crate) handle: session::RmwPublisher,
    pub(crate) _phantom: PhantomData<M>,
}

impl<M: RosMessage> EmbeddedPublisher<M> {
    /// Publish a message using the default buffer size.
    pub fn publish(&self, msg: &M) -> Result<(), NodeError> {
        self.publish_with_buffer::<DEFAULT_TX_BUF>(msg)
    }

    /// Publish a message with a custom buffer size.
    pub fn publish_with_buffer<const BUF: usize>(&self, msg: &M) -> Result<(), NodeError> {
        let mut buffer = [0u8; BUF];
        let mut writer =
            CdrWriter::new_with_header(&mut buffer).map_err(|_| NodeError::BufferTooSmall)?;
        msg.serialize(&mut writer)
            .map_err(|_| NodeError::Serialization)?;
        let len = writer.position();
        self.handle
            .publish_raw(&buffer[..len])
            .map_err(|_| NodeError::Transport(TransportError::PublishFailed))
    }

    /// Publish raw CDR-encoded data (must include CDR header).
    pub fn publish_raw(&self, data: &[u8]) -> Result<(), NodeError> {
        self.handle
            .publish_raw(data)
            .map_err(|_| NodeError::Transport(TransportError::PublishFailed))
    }
}

// ============================================================================
// EmbeddedRawPublisher — typeless publisher for non-ROS message wire formats
// ============================================================================

/// Default size of each per-publisher arena slot, in bytes.
pub const DEFAULT_LOAN_BUF: usize = 1024;

use core::cell::UnsafeCell;
use core::sync::atomic::Ordering;
// portable-atomic AtomicBool — resolves to native on targets that
// support it, software fallback on those that don't (e.g. some
// Xtensa ESP32 SoCs). nros-node compiles into example crates that
// target both.
use portable_atomic::AtomicBool;

/// Typeless publisher handle. Use when the wire format is not ROS CDR
/// (e.g. PX4 uORB raw POD bytes, custom binary protocols).
///
/// Two publish paths:
///
/// - [`publish_raw`](Self::publish_raw): user supplies a `&[u8]`, backend
///   memcpys into its outbound buffer. One copy.
/// - [`try_loan`](Self::try_loan): backend (or per-publisher arena fallback)
///   hands user a `&mut [u8]` slice. User writes in place. [`PublishLoan::commit`]
///   triggers the wire write. Zero-copy on backends with native lending
///   (Phase 99: zenoh-pico `unstable-zenoh-api`, XRCE-DDS); single-memcpy
///   fallback on backends without (uORB).
///
/// The const-generic `TX_BUF` sizes the inline arena slot (default
/// [`DEFAULT_LOAN_BUF`]). Loans larger than `TX_BUF` return
/// `LoanError::TooLarge`.
pub struct EmbeddedRawPublisher<const TX_BUF: usize = DEFAULT_LOAN_BUF> {
    pub(crate) handle: session::RmwPublisher,
    /// Single-slot arena: writable buffer + busy flag. SLOTS=1 in v1
    /// (concurrent loans on the same publisher return WouldBlock).
    pub(crate) arena: TxArena<TX_BUF>,
}

/// Single-slot per-publisher arena. Concurrent `try_loan` calls on the
/// same publisher race on the busy flag; loser gets `WouldBlock`.
pub(crate) struct TxArena<const TX_BUF: usize> {
    busy: AtomicBool,
    buf: UnsafeCell<[u8; TX_BUF]>,
}

// SAFETY: Sync-ness of the arena is enforced by the `busy` flag — only
// the thread that won the CAS may access `buf`, and only until commit/
// discard releases the slot.
unsafe impl<const TX_BUF: usize> Sync for TxArena<TX_BUF> {}

impl<const TX_BUF: usize> TxArena<TX_BUF> {
    pub(crate) const fn new() -> Self {
        Self {
            busy: AtomicBool::new(false),
            buf: UnsafeCell::new([0u8; TX_BUF]),
        }
    }

    /// Try to claim the arena slot. Returns a raw pointer + len pair on
    /// success; caller wraps it in a `PublishLoan`. Returns `false` if
    /// the slot is already loaned.
    ///
    /// `&self` returning `&mut` is sound because the `busy` flag
    /// gates exclusivity at runtime — the CAS in this function is
    /// the only writer, and `release()` is only callable through the
    /// loan's `Drop`.
    #[allow(clippy::mut_from_ref)]
    fn try_claim(&self, len: usize) -> Result<&mut [u8], LoanError> {
        if len > TX_BUF {
            return Err(LoanError::TooLarge);
        }
        if self
            .busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(LoanError::WouldBlock);
        }
        // SAFETY: we just won the busy CAS; we hold exclusive access
        // until release(). Lifetime is tied to `&self` for the loan.
        let buf_ref: &mut [u8; TX_BUF] = unsafe { &mut *self.buf.get() };
        Ok(&mut buf_ref[..len])
    }

    fn release(&self) {
        self.busy.store(false, Ordering::Release);
    }
}

impl<const TX_BUF: usize> EmbeddedRawPublisher<TX_BUF> {
    /// Publish a pre-encoded byte slice. The byte format depends entirely
    /// on the active RMW backend:
    ///
    /// - **zenoh / XRCE-DDS / DDS**: CDR-encoded payload including the
    ///   4-byte CDR header.
    /// - **uORB**: raw POD struct bytes (no header). Length must equal
    ///   `size_of::<T::Msg>()` for the registered topic.
    pub fn publish_raw(&self, data: &[u8]) -> Result<(), NodeError> {
        self.handle
            .publish_raw(data)
            .map_err(|_| NodeError::Transport(TransportError::PublishFailed))
    }

    /// Reserve a writable slot of `len` bytes. Caller writes into the
    /// returned [`PublishLoan`] and calls [`PublishLoan::commit`] to
    /// publish. Never blocks; returns [`LoanError::WouldBlock`] when the
    /// arena slot is already in use, or [`LoanError::TooLarge`] when
    /// `len` exceeds the const-generic `TX_BUF` capacity.
    pub fn try_loan(&self, len: usize) -> Result<PublishLoan<'_, TX_BUF>, LoanError> {
        let slice = self.arena.try_claim(len)?;
        Ok(PublishLoan {
            publisher: self,
            slice,
            committed: false,
        })
    }

    /// Sync blocking loan with timeout. Spins the executor until the
    /// arena slot is free or `timeout` elapses.
    ///
    /// Useful when you publish from a sync context that owns the
    /// executor and want to block on a busy arena (rare — single-slot
    /// arena means contention only when concurrent task tries the same
    /// publisher, in which case the offending other task should have
    /// committed promptly).
    pub fn loan_with_timeout(
        &self,
        len: usize,
        executor: &mut super::Executor,
        timeout: core::time::Duration,
    ) -> Result<PublishLoan<'_, TX_BUF>, LoanError> {
        if len > TX_BUF {
            return Err(LoanError::TooLarge);
        }
        let spin_interval = core::time::Duration::from_millis(DEFAULT_SPIN_INTERVAL_MS);
        let timeout_ms = timeout.as_millis().min(u64::MAX as u128) as u64;
        let max_spins = (timeout_ms / DEFAULT_SPIN_INTERVAL_MS).max(1);
        let mut budget = WaitBudget::new(max_spins, timeout);
        loop {
            match self.try_loan(len) {
                Ok(loan) => return Ok(loan),
                Err(LoanError::WouldBlock) => {
                    executor.spin_once(spin_interval);
                    if !budget.tick() {
                        return Err(LoanError::WouldBlock);
                    }
                }
                Err(other) => return Err(other),
            }
        }
    }
}

/// Error type for [`EmbeddedRawPublisher::try_loan`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoanError {
    /// Requested length exceeds the publisher's arena slot capacity.
    TooLarge,
    /// Arena slot already in use; another publish is in flight on this
    /// publisher. Retry after the other loan commits or discards.
    WouldBlock,
    /// Backend rejected the publish at commit time.
    Backend(TransportError),
}

impl From<TransportError> for LoanError {
    fn from(e: TransportError) -> Self {
        LoanError::Backend(e)
    }
}

/// Writable loan into a [`EmbeddedRawPublisher`]'s arena slot.
///
/// User fills `as_mut()` then calls [`commit`](Self::commit) to publish,
/// or [`discard`](Self::discard) to release the slot without publishing.
/// Dropping without either silently discards (slot freed); a
/// `#[must_use]` warning catches accidental drops at compile time.
#[must_use = "PublishLoan must be committed or discarded; dropping silently rolls back"]
pub struct PublishLoan<'a, const TX_BUF: usize> {
    publisher: &'a EmbeddedRawPublisher<TX_BUF>,
    slice: &'a mut [u8],
    committed: bool,
}

impl<'a, const TX_BUF: usize> PublishLoan<'a, TX_BUF> {
    /// Mutable view into the loaned bytes. Caller writes message data here.
    #[allow(clippy::should_implement_trait)]
    pub fn as_mut(&mut self) -> &mut [u8] {
        self.slice
    }

    /// Commit the loan: hand the bytes to the backend's `publish_raw`,
    /// then release the arena slot. Returns the backend's publish error
    /// if any (slot is released regardless).
    pub fn commit(mut self) -> Result<(), LoanError> {
        let res = self
            .publisher
            .handle
            .publish_raw(self.slice)
            .map_err(|_| LoanError::Backend(TransportError::PublishFailed));
        self.committed = true;
        // Drop runs and releases the slot.
        res
    }

    /// Discard the loan without publishing. Equivalent to dropping, but
    /// explicit (no #[must_use] warning).
    pub fn discard(mut self) {
        self.committed = true; // Suppress Drop's "discard" log if any.
        drop(self);
    }
}

impl<'a, const TX_BUF: usize> Drop for PublishLoan<'a, TX_BUF> {
    fn drop(&mut self) {
        // Slot always returned to the free pool; whether the bytes were
        // actually published is encoded in `committed`. Future telemetry
        // hook could log uncommitted drops in debug builds.
        self.publisher.arena.release();
    }
}

// ============================================================================
// Subscription
// ============================================================================

/// Typed subscription handle with internal receive buffer.
pub struct Subscription<M, const RX_BUF: usize = { crate::config::DEFAULT_RX_BUF_SIZE }> {
    pub(crate) handle: session::RmwSubscriber,
    pub(crate) buffer: [u8; RX_BUF],
    pub(crate) _phantom: PhantomData<M>,
}

impl<M: RosMessage, const RX_BUF: usize> Subscription<M, RX_BUF> {
    /// Try to receive a typed message (non-blocking).
    pub fn try_recv(&mut self) -> Result<Option<M>, NodeError> {
        match self
            .handle
            .try_recv_raw(&mut self.buffer)
            .map_err(|_| NodeError::Transport(TransportError::DeserializationError))?
        {
            Some(len) => {
                let mut reader = CdrReader::new_with_header(&self.buffer[..len])
                    .map_err(|_| NodeError::Transport(TransportError::DeserializationError))?;
                let msg = M::deserialize(&mut reader)
                    .map_err(|_| NodeError::Transport(TransportError::DeserializationError))?;
                Ok(Some(msg))
            }
            None => Ok(None),
        }
    }

    /// Try to receive raw CDR-encoded data (non-blocking).
    pub fn try_recv_raw(&mut self) -> Result<Option<usize>, NodeError> {
        self.handle
            .try_recv_raw(&mut self.buffer)
            .map_err(|_| NodeError::Transport(TransportError::DeserializationError))
    }

    /// Get the receive buffer (valid after `try_recv_raw`).
    pub fn buffer(&self) -> &[u8] {
        &self.buffer
    }

    /// Check if data is available without consuming it.
    pub fn has_data(&self) -> bool {
        self.handle.has_data()
    }

    /// Process the received message in-place without copying.
    pub fn process_in_place(&mut self, f: impl FnOnce(&M)) -> Result<bool, NodeError> {
        let mut deser_err = false;
        let processed = self
            .handle
            .process_raw_in_place(|raw| {
                match CdrReader::new_with_header(raw).and_then(|mut r| M::deserialize(&mut r)) {
                    Ok(msg) => f(&msg),
                    Err(_) => deser_err = true,
                }
            })
            .map_err(|_| NodeError::Transport(TransportError::DeserializationError))?;

        if deser_err {
            return Err(NodeError::Transport(TransportError::DeserializationError));
        }
        Ok(processed)
    }

    /// Async: wait for the next message (no `futures` dependency needed).
    ///
    /// Requires a background task running `executor.spin_async()` to drive
    /// I/O. Returns `Ok(msg)` on the next received message, or `Err` if the
    /// transport reports an error.
    ///
    /// When the `stream` feature is enabled, prefer `StreamExt::next()` /
    /// `TryStreamExt::try_next()` for combinator support.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut sub = node.create_subscription::<Int32>("/topic")?;
    /// loop {
    ///     let msg = sub.recv().await?;
    ///     /* handle msg */
    /// }
    /// ```
    pub async fn recv(&mut self) -> Result<M, NodeError> {
        core::future::poll_fn(|cx| {
            // Register the waker FIRST, then check for data. This ordering
            // closes the race window where a subscriber callback fires
            // between `try_recv` returning `None` and the waker being
            // registered — the wake would otherwise be delivered to the
            // previous waker (or nowhere) and the task would hang.
            self.handle.register_waker(cx.waker());
            match self.try_recv() {
                Ok(Some(msg)) => core::task::Poll::Ready(Ok(msg)),
                Ok(None) => core::task::Poll::Pending,
                Err(e) => core::task::Poll::Ready(Err(e)),
            }
        })
        .await
    }

    /// Sync: wait for the next message, spinning the executor.
    ///
    /// Returns `Ok(Some(msg))` if a message arrives within `timeout_ms`,
    /// or `Ok(None)` on timeout. Unlike [`Promise::wait()`], timeout is
    /// not an error — the caller typically retries in a loop.
    ///
    /// # Example
    ///
    /// ```ignore
    /// while let Some(msg) = sub.wait_next(&mut executor, core::time::Duration::from_millis(1000))? {
    ///     /* handle msg */
    /// }
    /// ```
    pub fn wait_next(
        &mut self,
        executor: &mut super::Executor,
        timeout: core::time::Duration,
    ) -> Result<Option<M>, NodeError> {
        let spin_interval = core::time::Duration::from_millis(DEFAULT_SPIN_INTERVAL_MS);
        let timeout_ms = timeout.as_millis().min(u64::MAX as u128) as u64;
        let max_spins = (timeout_ms / DEFAULT_SPIN_INTERVAL_MS).max(1);
        let mut budget = WaitBudget::new(max_spins, timeout);
        loop {
            executor.spin_once(spin_interval);
            if let Some(msg) = self.try_recv()? {
                return Ok(Some(msg));
            }
            if !budget.tick() {
                return Ok(None);
            }
        }
    }
}

// ============================================================================
// RawSubscription — typeless subscription for non-ROS message wire formats
// ============================================================================

/// Typeless subscription handle. Counterpart of [`EmbeddedRawPublisher`].
///
/// The user owns the decoding step: call [`try_recv_raw`](Self::try_recv_raw)
/// to fill an internal buffer with bytes whose format depends on the active
/// RMW backend, then interpret them however is appropriate (memcpy, custom
/// parser, …).
pub struct RawSubscription<const RX_BUF: usize = { crate::config::DEFAULT_RX_BUF_SIZE }> {
    pub(crate) handle: session::RmwSubscriber,
    pub(crate) buffer: [u8; RX_BUF],
}

impl<const RX_BUF: usize> RawSubscription<RX_BUF> {
    /// Try to receive raw bytes (non-blocking). Returns `Ok(Some(len))`
    /// with the message length on success; the bytes live in
    /// [`buffer`](Self::buffer) until the next call.
    pub fn try_recv_raw(&mut self) -> Result<Option<usize>, NodeError> {
        self.handle
            .try_recv_raw(&mut self.buffer)
            .map_err(|_| NodeError::Transport(TransportError::DeserializationError))
    }

    /// Get the receive buffer (valid after [`try_recv_raw`](Self::try_recv_raw)).
    pub fn buffer(&self) -> &[u8] {
        &self.buffer
    }

    /// Check if data is available without consuming it.
    pub fn has_data(&self) -> bool {
        self.handle.has_data()
    }

    /// Try to borrow the next available message in place. Returns
    /// `Ok(None)` if no message is ready; never blocks.
    ///
    /// The returned [`RecvView`] borrows the subscriber's internal
    /// receive buffer. Lifetime is tied to `&mut self` — only one view
    /// can be live at a time, and the next `try_borrow` / `try_recv_raw`
    /// call invalidates the previous view's bytes.
    ///
    /// View is `!Send + !Sync` to discourage holding it across `.await`
    /// or thread boundaries (would block subsequent receives on the
    /// same subscriber).
    pub fn try_borrow(&mut self) -> Result<Option<RecvView<'_>>, NodeError> {
        match self.try_recv_raw()? {
            Some(len) => Ok(Some(RecvView {
                bytes: &self.buffer[..len],
                _marker: core::marker::PhantomData,
            })),
            None => Ok(None),
        }
    }

    /// Async-await on the next message, returning a [`RecvView`].
    /// Mirrors the `Subscription::recv` pattern but typeless.
    ///
    /// Backend wake source: `Subscriber::register_waker`. Same race-
    /// safe register-then-check ordering as `Subscription::recv`.
    pub async fn borrow(&mut self) -> Result<RecvView<'_>, NodeError> {
        // Loop until try_borrow returns Some. Poll-and-register pattern
        // identical to Subscription::recv. The borrow returned by
        // try_borrow ties RecvView's lifetime to &mut self for the rest
        // of this fn body, so there's no lifetime trick needed.
        loop {
            // SAFETY-of-pattern: we register the waker BEFORE we check
            // for data so any concurrent backend callback that fires
            // between try_borrow returning None and the waker landing
            // wakes us, not the previous waker (or nowhere).
            core::future::poll_fn(|cx| {
                self.handle.register_waker(cx.waker());
                if self.has_data() {
                    core::task::Poll::Ready(())
                } else {
                    core::task::Poll::Pending
                }
            })
            .await;
            if let Some(view) = self.try_borrow()? {
                // Re-borrow to hand the view out with a fresh lifetime
                // tied to the outer &mut self.
                let len = view.as_ref().len();
                return Ok(RecvView {
                    bytes: &self.buffer[..len],
                    _marker: core::marker::PhantomData,
                });
            }
            // Spurious wake — loop and re-register.
        }
    }

    /// Sync blocking borrow with timeout. Spins the executor until a
    /// message is available or `timeout` elapses.
    ///
    /// Returns `Ok(Some(view))` on success, `Ok(None)` on timeout.
    /// The view's lifetime is tied to `&mut self`.
    pub fn borrow_with_timeout(
        &mut self,
        executor: &mut super::Executor,
        timeout: core::time::Duration,
    ) -> Result<Option<RecvView<'_>>, NodeError> {
        let spin_interval = core::time::Duration::from_millis(DEFAULT_SPIN_INTERVAL_MS);
        let timeout_ms = timeout.as_millis().min(u64::MAX as u128) as u64;
        let max_spins = (timeout_ms / DEFAULT_SPIN_INTERVAL_MS).max(1);
        let mut budget = WaitBudget::new(max_spins, timeout);
        loop {
            executor.spin_once(spin_interval);
            if self.has_data() {
                return self.try_borrow();
            }
            if !budget.tick() {
                return Ok(None);
            }
        }
    }
}

/// Read-only view into a [`RawSubscription`]'s receive buffer.
///
/// `!Send + !Sync`: cannot cross `.await` or threads. Drop releases
/// any backend lock + lets the next message advance.
pub struct RecvView<'a> {
    bytes: &'a [u8],
    _marker: core::marker::PhantomData<*const ()>,
}

impl<'a> core::ops::Deref for RecvView<'a> {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        self.bytes
    }
}

impl<'a> AsRef<[u8]> for RecvView<'a> {
    fn as_ref(&self) -> &[u8] {
        self.bytes
    }
}

#[cfg(feature = "stream")]
impl<M: RosMessage + Unpin, const RX_BUF: usize> futures_core::Stream for Subscription<M, RX_BUF> {
    type Item = Result<M, NodeError>;

    fn poll_next(
        self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Option<Self::Item>> {
        let this = self.get_mut();
        // Register-then-check: see Subscription::recv for rationale.
        this.handle.register_waker(cx.waker());
        match this.try_recv() {
            Ok(Some(msg)) => core::task::Poll::Ready(Some(Ok(msg))),
            Ok(None) => core::task::Poll::Pending,
            Err(e) => core::task::Poll::Ready(Some(Err(e))),
        }
    }
}

// ============================================================================
// EmbeddedServiceServer
// ============================================================================

/// Typed service server handle with internal buffers.
pub struct EmbeddedServiceServer<
    Svc: RosService,
    const REQ_BUF: usize = { crate::config::DEFAULT_RX_BUF_SIZE },
    const REPLY_BUF: usize = { crate::config::DEFAULT_RX_BUF_SIZE },
> {
    pub(crate) handle: session::RmwServiceServer,
    pub(crate) req_buffer: [u8; REQ_BUF],
    pub(crate) reply_buffer: [u8; REPLY_BUF],
    pub(crate) _phantom: PhantomData<Svc>,
}

impl<Svc: RosService, const REQ_BUF: usize, const REPLY_BUF: usize>
    EmbeddedServiceServer<Svc, REQ_BUF, REPLY_BUF>
{
    /// Handle an incoming service request.
    ///
    /// Returns `Ok(true)` if a request was handled, `Ok(false)` if none available.
    pub fn handle_request(
        &mut self,
        handler: impl FnOnce(&Svc::Request) -> Svc::Reply,
    ) -> Result<bool, NodeError> {
        self.handle
            .handle_request::<Svc>(&mut self.req_buffer, &mut self.reply_buffer, handler)
            .map_err(|_| NodeError::ServiceReplyFailed)
    }

    /// Handle a request with a heap-allocated reply (for large response types).
    ///
    /// Used by parameter services and lifecycle services (large response structs
    /// that overflow the stack). Returns `Ok(true)` if a request was handled,
    /// `Ok(false)` if none available.
    #[cfg(any(feature = "param-services", feature = "lifecycle-services"))]
    pub fn handle_request_boxed(
        &mut self,
        handler: impl FnOnce(&Svc::Request) -> alloc::boxed::Box<Svc::Reply>,
    ) -> Result<bool, NodeError> {
        self.handle
            .handle_request_boxed::<Svc>(&mut self.req_buffer, &mut self.reply_buffer, handler)
            .map_err(|_| NodeError::ServiceReplyFailed)
    }

    /// Check if a request is available.
    pub fn has_request(&self) -> bool {
        self.handle.has_request()
    }
}

// ============================================================================
// EmbeddedServiceClient
// ============================================================================

/// Typed service client handle with internal buffers.
pub struct EmbeddedServiceClient<
    Svc: RosService,
    const REQ_BUF: usize = { crate::config::DEFAULT_RX_BUF_SIZE },
    const REPLY_BUF: usize = { crate::config::DEFAULT_RX_BUF_SIZE },
> {
    pub(crate) handle: session::RmwServiceClient,
    pub(crate) req_buffer: [u8; REQ_BUF],
    pub(crate) reply_buffer: [u8; REPLY_BUF],
    /// Phase 84.D3: set after a successful `send_request`, cleared on a
    /// successful `Promise::try_recv`. Guards against "drop Promise
    /// without awaiting, then `call()` again" which would otherwise
    /// deliver the stale reply to the new caller.
    pub(crate) in_flight: bool,
    pub(crate) _phantom: PhantomData<Svc>,
}

impl<Svc: RosService, const REQ_BUF: usize, const REPLY_BUF: usize>
    EmbeddedServiceClient<Svc, REQ_BUF, REPLY_BUF>
{
    /// Call the service (non-blocking). Returns a [`Promise`] that can be polled.
    ///
    /// Use with `Executor::spin_once()` to drive I/O while waiting:
    ///
    /// ```ignore
    /// let mut promise = client.call(&request)?;
    /// loop {
    ///     executor.spin_once(core::time::Duration::from_millis(10));
    ///     if let Some(reply) = promise.try_recv()? {
    ///         break;
    ///     }
    /// }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`NodeError::RequestInFlight`] if a previous call's reply
    /// has not been received. This prevents the old hazard where dropping
    /// a [`Promise`] without awaiting its reply left the stale reply
    /// queued to land on the next [`call`](Self::call). Resolve by
    /// polling the existing promise to completion or calling
    /// [`reset_in_flight`](Self::reset_in_flight).
    pub fn call(&mut self, request: &Svc::Request) -> Result<Promise<'_, Svc::Reply>, NodeError> {
        if self.in_flight {
            return Err(NodeError::RequestInFlight);
        }

        // Serialize request into req_buffer
        let mut writer = CdrWriter::new_with_header(&mut self.req_buffer)
            .map_err(|_| NodeError::BufferTooSmall)?;
        request
            .serialize(&mut writer)
            .map_err(|_| NodeError::Serialization)?;
        let req_len = writer.position();

        // Send the request (non-blocking)
        self.handle
            .send_request_raw(&self.req_buffer[..req_len])
            .map_err(|_| NodeError::ServiceRequestFailed)?;

        self.in_flight = true;

        Ok(Promise {
            handle: &mut self.handle,
            reply_buffer: &mut self.reply_buffer,
            parse: cdr_deserialize_reply::<Svc>,
            in_flight_flag: &mut self.in_flight,
        })
    }

    /// Explicitly clear the in-flight flag (Phase 84.D3).
    ///
    /// Call this if a previous [`Promise`] was dropped without completing
    /// and you want to abandon the pending reply. The next
    /// [`call`](Self::call) will proceed but may still observe the stale
    /// reply if one is in the transport's queue — callers that need strict
    /// correctness should drain / ignore one extra `try_recv` first.
    pub fn reset_in_flight(&mut self) {
        self.in_flight = false;
    }

    /// Block until at least one matching service server is discoverable on
    /// the network, or `timeout` elapses.
    ///
    /// Returns `Ok(true)` if a matching server reported back inside the
    /// budget; `Ok(false)` on timeout (no server visible). Mirrors
    /// `rclcpp::ClientBase::wait_for_service` and
    /// `rclpy.client.Client.wait_for_service`.
    ///
    /// On the Zenoh backend this issues a `z_liveliness_get` against the
    /// matching server's wildcarded liveliness keyexpr; the executor is
    /// spun cooperatively while the query is in flight so other
    /// subscribers / timers continue to make progress. Backends without
    /// liveliness discovery answer `Ok(true)` immediately (default trait
    /// impl in `nros-rmw`), so the call is a no-op cost when discovery
    /// isn't supported.
    ///
    /// Recommended usage — gate the first `call()` on this:
    ///
    /// ```ignore
    /// let mut client = node.create_client::<AddTwoInts>("/add_two_ints")?;
    /// if !client.wait_for_service(&mut executor, Duration::from_secs(5))? {
    ///     return Err(NodeError::Timeout);
    /// }
    /// let mut promise = client.call(&request)?;
    /// ```
    ///
    /// Once the server is observed, the result is latched: subsequent
    /// `service_is_ready` checks return `true` without another round
    /// trip. This matches `rclcpp`'s snapshot semantic — discovery isn't
    /// re-proven on every call.
    pub fn wait_for_service(
        &mut self,
        executor: &mut super::Executor,
        timeout: core::time::Duration,
    ) -> Result<bool, NodeError> {
        // Already proven once — don't re-query.
        if self.handle.is_server_ready() {
            return Ok(true);
        }
        let spin_interval = core::time::Duration::from_millis(DEFAULT_SPIN_INTERVAL_MS);
        let max_spins = (timeout.as_millis() as u64 / DEFAULT_SPIN_INTERVAL_MS).max(1);
        let mut budget = WaitBudget::new(max_spins, timeout);
        // Per-query budget. A liveliness_get is a single-shot probe of the
        // router's current token list; if the server hasn't declared its
        // token yet when our query arrives, the router replies "no
        // matching tokens" and the query terminates. We loop, re-issuing
        // shorter probes until either a matching token is observed or the
        // outer wall-clock budget expires. 1 s per probe balances "see
        // freshly-declared tokens quickly" against "burn fewer FFI
        // round-trips on a healthy network".
        const PROBE_TIMEOUT_MS: u32 = 1000;
        loop {
            self.handle
                .start_server_discovery(PROBE_TIMEOUT_MS)
                .map_err(|_| NodeError::ServiceRequestFailed)?;
            // Drain this probe to completion (token reply or empty FINAL).
            loop {
                executor.spin_once(spin_interval);
                match self
                    .handle
                    .poll_server_discovery()
                    .map_err(|_| NodeError::ServiceRequestFailed)?
                {
                    Some(true) => return Ok(true),
                    Some(false) => break, // probe finished empty — re-issue
                    None => {}            // still in flight
                }
                if !budget.tick() {
                    return Ok(false);
                }
            }
            if !budget.tick() {
                return Ok(false);
            }
        }
    }

    /// Snapshot whether a matching service server is currently visible.
    ///
    /// Non-blocking. Matches `rclcpp::ClientBase::service_is_ready` and
    /// `rclpy.client.Client.service_is_ready`. Backends without liveliness
    /// discovery return `true` (assume always reachable).
    pub fn service_is_ready(&self) -> bool {
        self.handle.is_server_ready()
    }
}

// ============================================================================
// Promise
// ============================================================================

/// A pending reply from a non-blocking service or action call.
///
/// Poll with [`try_recv()`](Promise::try_recv) to check for the reply.
/// Implements [`Future`](core::future::Future) for use with async executors.
pub struct Promise<'a, T> {
    pub(crate) handle: &'a mut session::RmwServiceClient,
    pub(crate) reply_buffer: &'a mut [u8],
    pub(crate) parse: fn(&[u8]) -> Result<T, NodeError>,
    /// Phase 84.D3: cleared on a successful `try_recv` so the client's
    /// next `call()` can proceed. If the `Promise` is dropped before the
    /// reply is consumed, the flag stays set — forcing the user to
    /// explicitly acknowledge the abandoned call via
    /// `reset_in_flight()`.
    pub(crate) in_flight_flag: &'a mut bool,
}

impl<T> Promise<'_, T> {
    /// Try to receive the reply (non-blocking).
    ///
    /// Returns `Ok(Some(reply))` if the reply has arrived,
    /// `Ok(None)` if still pending.
    pub fn try_recv(&mut self) -> Result<Option<T>, NodeError> {
        match self
            .handle
            .try_recv_reply_raw(self.reply_buffer)
            .map_err(|_| NodeError::ServiceRequestFailed)?
        {
            Some(len) => {
                let reply = (self.parse)(&self.reply_buffer[..len])?;
                // Reply consumed — allow the client to issue another call.
                *self.in_flight_flag = false;
                Ok(Some(reply))
            }
            None => Ok(None),
        }
    }
}

impl<T> Promise<'_, T> {
    /// Block until the reply arrives, spinning the executor.
    ///
    /// Internally calls `executor.spin_once()` in a loop until the reply
    /// arrives or `timeout_ms` is exhausted. This is equivalent to the
    /// manual spin+poll loop pattern but more ergonomic for simple use cases.
    ///
    /// No borrow conflict: `executor` and `self` (which borrows the standalone
    /// client) are disjoint objects.
    ///
    /// # Errors
    ///
    /// Returns [`NodeError::Timeout`] if the reply does not arrive within
    /// `timeout_ms` milliseconds.
    pub fn wait(
        &mut self,
        executor: &mut super::Executor,
        timeout: core::time::Duration,
    ) -> Result<T, NodeError> {
        let spin_interval = core::time::Duration::from_millis(DEFAULT_SPIN_INTERVAL_MS);
        let timeout_ms = timeout.as_millis().min(u64::MAX as u128) as u64;
        let max_spins = (timeout_ms / DEFAULT_SPIN_INTERVAL_MS).max(1);
        let mut budget = WaitBudget::new(max_spins, timeout);
        // Always spin at least once so a zero-timeout still polls.
        loop {
            executor.spin_once(spin_interval);
            if let Some(result) = self.try_recv()? {
                return Ok(result);
            }
            if !budget.tick() {
                return Err(NodeError::Timeout);
            }
        }
    }
}

impl<T> core::future::Future for Promise<'_, T> {
    type Output = Result<T, NodeError>;

    fn poll(
        self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Self::Output> {
        let this = self.get_mut();
        // Register-then-check (closes the race where a reply lands
        // between try_recv returning None and the waker registering).
        this.handle.register_waker(cx.waker());
        match this.try_recv() {
            Ok(Some(reply)) => core::task::Poll::Ready(Ok(reply)),
            Ok(None) => core::task::Poll::Pending,
            Err(e) => core::task::Poll::Ready(Err(e)),
        }
    }
}

/// Deserialize a CDR-encoded service reply.
fn cdr_deserialize_reply<Svc: RosService>(data: &[u8]) -> Result<Svc::Reply, NodeError> {
    let mut reader =
        CdrReader::new_with_header(data).map_err(|_| NodeError::ServiceRequestFailed)?;
    Svc::Reply::deserialize(&mut reader).map_err(|_| NodeError::ServiceRequestFailed)
}

// ============================================================================
// Action types
// ============================================================================

/// Active goal tracking for action server.
#[derive(Clone)]
pub struct ActiveGoal<A: RosAction> {
    /// Goal ID.
    pub goal_id: nros_core::GoalId,
    /// Current status.
    pub status: nros_core::GoalStatus,
    /// The goal data.
    pub goal: A::Goal,
}

/// Completed goal with result.
pub struct CompletedGoal<A: RosAction> {
    /// Goal ID.
    pub goal_id: nros_core::GoalId,
    /// Final status.
    pub status: nros_core::GoalStatus,
    /// The result data.
    pub result: A::Result,
}

// ============================================================================
// ActionServer
// ============================================================================

/// Typed action server with goal state management.
///
/// Wraps [`ActionServerCore`](super::action_core::ActionServerCore) for
/// raw-bytes protocol handling, adding typed goal/feedback/result
/// serialization at the boundary.
pub struct ActionServer<
    A: RosAction,
    const GOAL_BUF: usize = { crate::config::DEFAULT_RX_BUF_SIZE },
    const RESULT_BUF: usize = { crate::config::DEFAULT_RX_BUF_SIZE },
    const FEEDBACK_BUF: usize = { crate::config::DEFAULT_RX_BUF_SIZE },
    const MAX_GOALS: usize = 4,
> {
    pub(crate) core:
        super::action_core::ActionServerCore<GOAL_BUF, RESULT_BUF, FEEDBACK_BUF, MAX_GOALS>,
    /// Typed goal data parallel to `core.active_goals`.
    pub(crate) typed_goals: heapless::Vec<A::Goal, MAX_GOALS>,
    /// Completed goals with typed results.
    pub(crate) completed_goals: heapless::Vec<CompletedGoal<A>, MAX_GOALS>,
}

impl<
    A: RosAction,
    const GOAL_BUF: usize,
    const RESULT_BUF: usize,
    const FEEDBACK_BUF: usize,
    const MAX_GOALS: usize,
> ActionServer<A, GOAL_BUF, RESULT_BUF, FEEDBACK_BUF, MAX_GOALS>
{
    /// Try to accept a new goal.
    ///
    /// Checks for incoming send_goal requests. If one is available, calls the
    /// handler to decide acceptance. Returns the goal ID if accepted.
    pub fn try_accept_goal(
        &mut self,
        goal_handler: impl FnOnce(&nros_core::GoalId, &A::Goal) -> nros_core::GoalResponse,
    ) -> Result<Option<nros_core::GoalId>, NodeError>
    where
        A::Goal: Clone,
    {
        let raw_req = self.core.try_recv_goal_request()?;
        let raw_req = match raw_req {
            Some(r) => r,
            None => return Ok(None),
        };

        // Deserialize the goal from the buffer at the offset captured
        // by the core (DDS prepends an 8-byte seq prefix; zenoh uses 0).
        let buf = self.core.goal_buffer();
        let start = raw_req.data_offset;
        let end = start + raw_req.data_len;
        let mut reader = CdrReader::new_with_header(&buf[start..end])
            .map_err(|_| NodeError::Transport(TransportError::DeserializationError))?;
        // Skip past the GoalId (CDR length prefix + UUID bytes)
        let _ = reader.read_u32();
        for _ in 0..GOAL_UUID_SIZE {
            let _ = reader.read_u8();
        }
        let goal = A::Goal::deserialize(&mut reader)
            .map_err(|_| NodeError::Transport(TransportError::DeserializationError))?;

        let response = goal_handler(&raw_req.goal_id, &goal);
        let accepted = response.is_accepted();

        if accepted {
            self.core
                .accept_goal(raw_req.goal_id, raw_req.sequence_number)?;
            let _ = self.typed_goals.push(goal);
            Ok(Some(raw_req.goal_id))
        } else {
            self.core.reject_goal(raw_req.sequence_number)?;
            Ok(None)
        }
    }

    /// Publish feedback for a goal.
    pub fn publish_feedback(
        &mut self,
        goal_id: &nros_core::GoalId,
        feedback: &A::Feedback,
    ) -> Result<(), NodeError> {
        // Serialize feedback into a temp buffer (without CDR header or GoalId)
        let mut tmp = [0u8; FEEDBACK_BUF];
        let mut writer = CdrWriter::new(&mut tmp);
        feedback
            .serialize(&mut writer)
            .map_err(|_| NodeError::Serialization)?;
        let feedback_len = writer.position();

        self.core
            .publish_feedback_raw(goal_id, &tmp[..feedback_len])
    }

    /// Set a goal's status.
    ///
    /// Also publishes the updated `GoalStatusArray` on the status topic.
    pub fn set_goal_status(&mut self, goal_id: &nros_core::GoalId, status: nros_core::GoalStatus) {
        self.core.set_goal_status(goal_id, status);
    }

    /// Complete a goal and store the result.
    ///
    /// Also publishes the updated `GoalStatusArray` on the status topic.
    pub fn complete_goal(
        &mut self,
        goal_id: &nros_core::GoalId,
        status: nros_core::GoalStatus,
        result: A::Result,
    ) {
        // Serialize result for the core slab
        let mut tmp = [0u8; RESULT_BUF];
        let mut writer = CdrWriter::new(&mut tmp);
        let result_len = match result.serialize(&mut writer) {
            Ok(()) => writer.position(),
            Err(_) => 0,
        };

        // Remove typed goal parallel to core's active_goals removal
        if let Some(pos) = self
            .core
            .active_goals()
            .iter()
            .position(|g| g.goal_id.uuid == goal_id.uuid)
        {
            self.typed_goals.swap_remove(pos);
        }

        self.core
            .complete_goal_raw(goal_id, status, &tmp[..result_len]);

        let _ = self.completed_goals.push(CompletedGoal {
            goal_id: *goal_id,
            status,
            result,
        });
    }

    /// Try to handle a cancel_goal request.
    pub fn try_handle_cancel(
        &mut self,
        cancel_handler: impl FnOnce(
            &nros_core::GoalId,
            nros_core::GoalStatus,
        ) -> nros_core::CancelResponse,
    ) -> Result<Option<(nros_core::GoalId, nros_core::CancelResponse)>, NodeError> {
        self.core.try_handle_cancel(cancel_handler)
    }

    /// Try to handle a get_result request.
    pub fn try_handle_get_result(&mut self) -> Result<Option<nros_core::GoalId>, NodeError>
    where
        A::Result: Clone + Default,
    {
        // Serialize default result for non-completed goals
        let mut default_buf = [0u8; RESULT_BUF];
        let mut writer = CdrWriter::new(&mut default_buf);
        let default_len = match A::Result::default().serialize(&mut writer) {
            Ok(()) => writer.position(),
            Err(_) => 0,
        };

        self.core
            .try_handle_get_result_raw(&default_buf[..default_len])
    }

    /// Drain all pending server-side work in one call.
    ///
    /// Calls `try_accept_goal`, `try_handle_cancel`, and
    /// `try_handle_get_result` in sequence. Invoke this on every
    /// `spin_once` iteration in manual-poll code — otherwise clients
    /// will hang on `get_result` because `create_action_server()`
    /// servers are not arena-registered.
    ///
    /// The two callbacks may be called zero or one times per `poll()`:
    ///   * `on_goal` fires when a new goal arrives.
    ///   * `on_cancel` fires when a cancel request arrives.
    ///
    /// Get-result requests are drained unconditionally (no callback
    /// needed — the result is pulled from the goal's stored state).
    ///
    /// # Example
    /// ```ignore
    /// let mut server = node.create_action_server::<Fibonacci>("/fibonacci")?;
    /// loop {
    ///     executor.spin_once(Duration::from_millis(10));
    ///     server.poll(
    ///         |id, goal| {
    ///             /* accept or reject based on `goal` */
    ///             GoalResponse::AcceptAndExecute
    ///         },
    ///         |_id, _status| CancelResponse::Accept,
    ///     )?;
    /// }
    /// ```
    pub fn poll<GF, CF>(&mut self, mut on_goal: GF, mut on_cancel: CF) -> Result<(), NodeError>
    where
        GF: FnMut(&nros_core::GoalId, &A::Goal) -> nros_core::GoalResponse,
        CF: FnMut(&nros_core::GoalId, nros_core::GoalStatus) -> nros_core::CancelResponse,
        A::Goal: Clone,
        A::Result: Clone + Default,
    {
        let _ = self.try_accept_goal(|id, goal| on_goal(id, goal))?;
        let _ = self.try_handle_cancel(|id, status| on_cancel(id, status))?;
        let _ = self.try_handle_get_result()?;
        Ok(())
    }

    /// Get a reference to an active goal.
    pub fn get_goal(&self, goal_id: &nros_core::GoalId) -> Option<ActiveGoal<A>>
    where
        A::Goal: Clone,
    {
        self.core
            .active_goals()
            .iter()
            .enumerate()
            .find(|(_, g)| g.goal_id.uuid == goal_id.uuid)
            .map(|(i, raw)| ActiveGoal {
                goal_id: raw.goal_id,
                status: raw.status,
                goal: self.typed_goals[i].clone(),
            })
    }

    /// Get the number of active goals.
    pub fn active_goal_count(&self) -> usize {
        self.core.active_goal_count()
    }
}

// ============================================================================
// ActionClient
// ============================================================================

/// Typed action client handle.
///
/// Wraps [`ActionClientCore`](super::action_core::ActionClientCore) for
/// raw-bytes protocol handling, adding typed goal/feedback/result
/// serialization at the boundary.
pub struct ActionClient<
    A: RosAction,
    const GOAL_BUF: usize = { crate::config::DEFAULT_RX_BUF_SIZE },
    const RESULT_BUF: usize = { crate::config::DEFAULT_RX_BUF_SIZE },
    const FEEDBACK_BUF: usize = { crate::config::DEFAULT_RX_BUF_SIZE },
> {
    pub(crate) core: super::action_core::ActionClientCore<GOAL_BUF, RESULT_BUF, FEEDBACK_BUF>,
    pub(crate) _phantom: PhantomData<A>,
}

impl<A: RosAction, const GOAL_BUF: usize, const RESULT_BUF: usize, const FEEDBACK_BUF: usize>
    ActionClient<A, GOAL_BUF, RESULT_BUF, FEEDBACK_BUF>
{
    /// Send a goal (non-blocking). Returns the goal ID and a [`Promise`] for acceptance.
    ///
    /// The promise resolves to `true` if accepted, `false` if rejected.
    pub fn send_goal(
        &mut self,
        goal: &A::Goal,
    ) -> Result<(nros_core::GoalId, Promise<'_, bool>), NodeError> {
        if self.core.in_flight_send_goal {
            return Err(NodeError::RequestInFlight);
        }

        // Serialize goal into a temp buffer (without CDR header or GoalId)
        let mut tmp = [0u8; GOAL_BUF];
        let mut writer = CdrWriter::new(&mut tmp);
        goal.serialize(&mut writer)
            .map_err(|_| NodeError::Serialization)?;
        let goal_len = writer.position();

        let goal_id = self.core.send_goal_raw(&tmp[..goal_len])?;
        self.core.in_flight_send_goal = true;

        Ok((
            goal_id,
            Promise {
                handle: &mut self.core.send_goal_client,
                reply_buffer: &mut self.core.result_buffer,
                parse: parse_goal_accepted,
                in_flight_flag: &mut self.core.in_flight_send_goal,
            },
        ))
    }

    /// Try to receive feedback (non-blocking).
    pub fn try_recv_feedback(
        &mut self,
    ) -> Result<Option<(nros_core::GoalId, A::Feedback)>, NodeError> {
        let (goal_id, len) = match self.core.try_recv_feedback_raw()? {
            Some(v) => v,
            None => return Ok(None),
        };

        // Deserialize feedback from the core's feedback buffer (after GoalId)
        let mut reader = CdrReader::new_with_header(&self.core.feedback_buffer[..len])
            .map_err(|_| NodeError::Transport(TransportError::DeserializationError))?;
        // Skip GoalId (CDR length prefix + UUID bytes)
        let _ = reader.read_u32();
        for _ in 0..GOAL_UUID_SIZE {
            let _ = reader.read_u8();
        }

        let feedback = A::Feedback::deserialize(&mut reader)
            .map_err(|_| NodeError::Transport(TransportError::DeserializationError))?;

        Ok(Some((goal_id, feedback)))
    }

    /// Cancel a goal (non-blocking). Returns a [`Promise`] for the cancel response.
    pub fn cancel_goal(
        &mut self,
        goal_id: &nros_core::GoalId,
    ) -> Result<Promise<'_, nros_core::CancelResponse>, NodeError> {
        if self.core.in_flight_cancel {
            return Err(NodeError::RequestInFlight);
        }
        self.core.send_cancel_request(goal_id)?;
        self.core.in_flight_cancel = true;

        Ok(Promise {
            handle: &mut self.core.cancel_goal_client,
            reply_buffer: &mut self.core.result_buffer,
            parse: parse_cancel_response,
            in_flight_flag: &mut self.core.in_flight_cancel,
        })
    }

    /// Get the result of a completed goal (non-blocking). Returns a [`Promise`].
    pub fn get_result(
        &mut self,
        goal_id: &nros_core::GoalId,
    ) -> Result<Promise<'_, (nros_core::GoalStatus, A::Result)>, NodeError> {
        if self.core.in_flight_get_result {
            return Err(NodeError::RequestInFlight);
        }
        self.core.send_get_result_request(goal_id)?;
        self.core.in_flight_get_result = true;

        Ok(Promise {
            handle: &mut self.core.get_result_client,
            reply_buffer: &mut self.core.result_buffer,
            parse: parse_result_response::<A>,
            in_flight_flag: &mut self.core.in_flight_get_result,
        })
    }

    /// Explicitly clear the "send_goal reply in flight" flag (Phase 84.D3).
    pub fn reset_send_goal_in_flight(&mut self) {
        self.core.in_flight_send_goal = false;
    }

    /// Block until the action server's send-goal queryable is discoverable
    /// on the network, or `timeout` elapses.
    ///
    /// Returns `Ok(true)` on discovery, `Ok(false)` on timeout. Mirrors
    /// `rclcpp_action::Client::wait_for_action_server`.
    ///
    /// Implementation: probes the action's `send_goal` service-server
    /// liveliness keyexpr via the same primitive as
    /// [`Client::wait_for_service`]. Once that service is reachable the
    /// remaining four action entities (cancel queryable + feedback /
    /// status / result publishers) are also reachable in practice — they
    /// were declared by the same server in one batch.
    pub fn wait_for_action_server(
        &mut self,
        executor: &mut super::Executor,
        timeout: core::time::Duration,
    ) -> Result<bool, NodeError> {
        if self.core.send_goal_client.is_server_ready() {
            return Ok(true);
        }
        let spin_interval = core::time::Duration::from_millis(DEFAULT_SPIN_INTERVAL_MS);
        let max_spins = (timeout.as_millis() as u64 / DEFAULT_SPIN_INTERVAL_MS).max(1);
        let mut budget = WaitBudget::new(max_spins, timeout);
        // See `Client::wait_for_service` for the re-probe rationale: a
        // single liveliness_get samples the router's current token list
        // and terminates; we loop with shorter per-probe timeouts so the
        // outer budget covers servers that come up after we start
        // waiting.
        const PROBE_TIMEOUT_MS: u32 = 1000;
        loop {
            self.core
                .send_goal_client
                .start_server_discovery(PROBE_TIMEOUT_MS)
                .map_err(|_| NodeError::ServiceRequestFailed)?;
            loop {
                executor.spin_once(spin_interval);
                match self
                    .core
                    .send_goal_client
                    .poll_server_discovery()
                    .map_err(|_| NodeError::ServiceRequestFailed)?
                {
                    Some(true) => return Ok(true),
                    Some(false) => break,
                    None => {}
                }
                if !budget.tick() {
                    return Ok(false);
                }
            }
            if !budget.tick() {
                return Ok(false);
            }
        }
    }

    /// Snapshot whether the action server is currently visible.
    /// Mirrors `rclcpp_action::Client::action_server_is_ready`.
    pub fn action_server_is_ready(&self) -> bool {
        self.core.send_goal_client.is_server_ready()
    }

    /// Explicitly clear the "cancel reply in flight" flag (Phase 84.D3).
    pub fn reset_cancel_in_flight(&mut self) {
        self.core.in_flight_cancel = false;
    }

    /// Explicitly clear the "get_result reply in flight" flag (Phase 84.D3).
    pub fn reset_get_result_in_flight(&mut self) {
        self.core.in_flight_get_result = false;
    }

    /// Create a feedback stream (receives feedback for all goals).
    ///
    /// The stream borrows `&mut self` exclusively. Drop it before calling
    /// `get_result()` or `cancel_goal()`.
    pub fn feedback_stream(&mut self) -> FeedbackStream<'_, A, GOAL_BUF, RESULT_BUF, FEEDBACK_BUF> {
        FeedbackStream { client: self }
    }

    /// Create a goal-filtered feedback stream.
    ///
    /// Only yields feedback for the given `goal_id`, returning `A::Feedback`
    /// directly (without the `GoalId` wrapper).
    pub fn feedback_stream_for(
        &mut self,
        goal_id: nros_core::GoalId,
    ) -> GoalFeedbackStream<'_, A, GOAL_BUF, RESULT_BUF, FEEDBACK_BUF> {
        GoalFeedbackStream {
            client: self,
            goal_id,
        }
    }
}

// ============================================================================
// FeedbackStream
// ============================================================================

/// A stream of feedback messages from an action server.
///
/// Created by [`ActionClient::feedback_stream()`]. Receives feedback for
/// all active goals. The stream never self-terminates — use combinators
/// like `take_while` or `break` to stop.
///
/// Three access modes:
/// - **Async (`Stream`)**: Enable the `stream` feature for
///   `futures_core::Stream` + `StreamExt` combinators
/// - **Async (no deps)**: Use `next()` in
///   `while let` loops (always available)
/// - **Sync**: Use [`wait_next()`](FeedbackStream::wait_next) which
///   drives the executor internally
pub struct FeedbackStream<
    'a,
    A: RosAction,
    const GOAL_BUF: usize = { crate::config::DEFAULT_RX_BUF_SIZE },
    const RESULT_BUF: usize = { crate::config::DEFAULT_RX_BUF_SIZE },
    const FEEDBACK_BUF: usize = { crate::config::DEFAULT_RX_BUF_SIZE },
> {
    client: &'a mut ActionClient<A, GOAL_BUF, RESULT_BUF, FEEDBACK_BUF>,
}

impl<A: RosAction, const GOAL_BUF: usize, const RESULT_BUF: usize, const FEEDBACK_BUF: usize>
    FeedbackStream<'_, A, GOAL_BUF, RESULT_BUF, FEEDBACK_BUF>
{
    /// Async: wait for the next feedback message (no `futures` dependency needed).
    ///
    /// Requires a background task running `executor.spin_async()` to drive
    /// I/O. Returns `None` only on error.
    ///
    /// When the `stream` feature is enabled, prefer `StreamExt::next()` or
    /// `TryStreamExt::try_next()` for combinator support.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut stream = client.feedback_stream();
    /// while let Some(result) = stream.recv().await {
    ///     let (goal_id, feedback) = result?;
    ///     // process feedback...
    /// }
    /// ```
    pub async fn recv(&mut self) -> Option<Result<(nros_core::GoalId, A::Feedback), NodeError>> {
        core::future::poll_fn(|cx| {
            // Register-then-check (closes the AtomicWaker race).
            self.client
                .core
                .feedback_subscriber
                .register_waker(cx.waker());
            match self.client.try_recv_feedback() {
                Ok(Some(item)) => core::task::Poll::Ready(Some(Ok(item))),
                Ok(None) => core::task::Poll::Pending,
                Err(e) => core::task::Poll::Ready(Some(Err(e))),
            }
        })
        .await
    }

    /// Sync: wait for the next feedback message, spinning the executor.
    ///
    /// Returns `Ok(Some(feedback))` if a message arrives within `timeout_ms`,
    /// or `Ok(None)` on timeout. Unlike [`Promise::wait()`], timeout is not
    /// an error — the caller typically retries in a loop.
    pub fn wait_next(
        &mut self,
        executor: &mut super::Executor,
        timeout: core::time::Duration,
    ) -> Result<Option<(nros_core::GoalId, A::Feedback)>, NodeError> {
        let spin_interval = core::time::Duration::from_millis(DEFAULT_SPIN_INTERVAL_MS);
        let timeout_ms = timeout.as_millis().min(u64::MAX as u128) as u64;
        let max_spins = (timeout_ms / DEFAULT_SPIN_INTERVAL_MS).max(1);
        let mut budget = WaitBudget::new(max_spins, timeout);
        loop {
            executor.spin_once(spin_interval);
            if let Some(item) = self.client.try_recv_feedback()? {
                return Ok(Some(item));
            }
            if !budget.tick() {
                return Ok(None);
            }
        }
    }
}

#[cfg(feature = "stream")]
impl<A: RosAction, const GOAL_BUF: usize, const RESULT_BUF: usize, const FEEDBACK_BUF: usize>
    futures_core::Stream for FeedbackStream<'_, A, GOAL_BUF, RESULT_BUF, FEEDBACK_BUF>
{
    type Item = Result<(nros_core::GoalId, A::Feedback), NodeError>;

    fn poll_next(
        self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Option<Self::Item>> {
        let this = self.get_mut();
        // Register-then-check (closes the AtomicWaker race).
        this.client
            .core
            .feedback_subscriber
            .register_waker(cx.waker());
        match this.client.try_recv_feedback() {
            Ok(Some(item)) => core::task::Poll::Ready(Some(Ok(item))),
            Ok(None) => core::task::Poll::Pending,
            Err(e) => core::task::Poll::Ready(Some(Err(e))),
        }
    }
}

// ============================================================================
// GoalFeedbackStream
// ============================================================================

/// A goal-filtered feedback stream.
///
/// Created by [`ActionClient::feedback_stream_for()`]. Only yields feedback
/// messages matching the specified goal ID.
pub struct GoalFeedbackStream<
    'a,
    A: RosAction,
    const GOAL_BUF: usize = { crate::config::DEFAULT_RX_BUF_SIZE },
    const RESULT_BUF: usize = { crate::config::DEFAULT_RX_BUF_SIZE },
    const FEEDBACK_BUF: usize = { crate::config::DEFAULT_RX_BUF_SIZE },
> {
    client: &'a mut ActionClient<A, GOAL_BUF, RESULT_BUF, FEEDBACK_BUF>,
    goal_id: nros_core::GoalId,
}

impl<A: RosAction, const GOAL_BUF: usize, const RESULT_BUF: usize, const FEEDBACK_BUF: usize>
    GoalFeedbackStream<'_, A, GOAL_BUF, RESULT_BUF, FEEDBACK_BUF>
{
    /// Async: wait for the next feedback message for this goal (no `futures` dependency needed).
    ///
    /// When the `stream` feature is enabled, prefer `StreamExt::next()` or
    /// `TryStreamExt::try_next()` for combinator support.
    pub async fn recv(&mut self) -> Option<Result<A::Feedback, NodeError>> {
        core::future::poll_fn(|cx| {
            // Register-then-check (closes the AtomicWaker race). The
            // waker is registered once for both the "no data" and
            // "wrong goal" branches that fall through to Pending.
            self.client
                .core
                .feedback_subscriber
                .register_waker(cx.waker());
            match self.client.try_recv_feedback() {
                Ok(Some((id, feedback))) if id.uuid == self.goal_id.uuid => {
                    core::task::Poll::Ready(Some(Ok(feedback)))
                }
                // Feedback for a different goal — keep waiting.
                Ok(Some(_)) => core::task::Poll::Pending,
                Ok(None) => core::task::Poll::Pending,
                Err(e) => core::task::Poll::Ready(Some(Err(e))),
            }
        })
        .await
    }

    /// Sync: wait for the next feedback message for this goal, spinning the executor.
    pub fn wait_next(
        &mut self,
        executor: &mut super::Executor,
        timeout: core::time::Duration,
    ) -> Result<Option<A::Feedback>, NodeError> {
        let spin_interval = core::time::Duration::from_millis(DEFAULT_SPIN_INTERVAL_MS);
        let timeout_ms = timeout.as_millis().min(u64::MAX as u128) as u64;
        let max_spins = (timeout_ms / DEFAULT_SPIN_INTERVAL_MS).max(1);
        let mut budget = WaitBudget::new(max_spins, timeout);
        loop {
            executor.spin_once(spin_interval);
            if let Some((id, feedback)) = self.client.try_recv_feedback()?
                && id.uuid == self.goal_id.uuid
            {
                return Ok(Some(feedback));
            }
            if !budget.tick() {
                return Ok(None);
            }
        }
    }
}

#[cfg(feature = "stream")]
impl<A: RosAction, const GOAL_BUF: usize, const RESULT_BUF: usize, const FEEDBACK_BUF: usize>
    futures_core::Stream for GoalFeedbackStream<'_, A, GOAL_BUF, RESULT_BUF, FEEDBACK_BUF>
{
    type Item = Result<A::Feedback, NodeError>;

    fn poll_next(
        self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Option<Self::Item>> {
        let this = self.get_mut();
        // Register-then-check (closes the AtomicWaker race).
        this.client
            .core
            .feedback_subscriber
            .register_waker(cx.waker());
        match this.client.try_recv_feedback() {
            Ok(Some((id, feedback))) if id.uuid == this.goal_id.uuid => {
                core::task::Poll::Ready(Some(Ok(feedback)))
            }
            Ok(Some(_)) => core::task::Poll::Pending,
            Ok(None) => core::task::Poll::Pending,
            Err(e) => core::task::Poll::Ready(Some(Err(e))),
        }
    }
}

/// Parse a goal acceptance response (bool).
fn parse_goal_accepted(data: &[u8]) -> Result<bool, NodeError> {
    let mut reader =
        CdrReader::new_with_header(data).map_err(|_| NodeError::ServiceRequestFailed)?;
    let accepted = reader.read_u8().unwrap_or(0) != 0;
    Ok(accepted)
}

/// Parse a cancel response.
fn parse_cancel_response(data: &[u8]) -> Result<nros_core::CancelResponse, NodeError> {
    let mut reader =
        CdrReader::new_with_header(data).map_err(|_| NodeError::ServiceRequestFailed)?;
    let return_code = reader.read_i8().unwrap_or(2);
    Ok(nros_core::CancelResponse::from_i8(return_code).unwrap_or_default())
}

/// Parse an action result response (status + result).
fn parse_result_response<A: RosAction>(
    data: &[u8],
) -> Result<(nros_core::GoalStatus, A::Result), NodeError> {
    let mut reader =
        CdrReader::new_with_header(data).map_err(|_| NodeError::ServiceRequestFailed)?;
    let status_code = reader.read_i8().unwrap_or(0);
    let status = nros_core::GoalStatus::from_i8(status_code).unwrap_or_default();
    let result =
        A::Result::deserialize(&mut reader).map_err(|_| NodeError::ServiceRequestFailed)?;
    Ok((status, result))
}

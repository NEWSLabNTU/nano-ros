//! [`UorbSubscriber`] implements [`nros_rmw::Subscriber`] over uORB.
//!
//! Phase 99.L design: byte-shaped only. Holds a
//! [`px4_uorb::RawSubscription`] which lazily registers an
//! `orb_register_callback` on first `try_recv_raw` and writes
//! into the caller's buffer on each subsequent call. No registry,
//! no name lookup, no critical_section.

use core::{cell::Cell, ffi::c_void};

use nros_rmw::{Subscriber, TransportError};
use px4_sys::orb_metadata;
use px4_uorb::RawSubscription;

/// Byte-shaped subscriber handle for one uORB topic.
///
/// Holds the raw subscription state directly. `try_recv_raw` calls
/// `orb_copy` (via the raw FFI wrapper) through the lazy
/// callback registration — single FFI path, no lookup, no lock.
pub struct UorbSubscriber {
    inner: RawSubscription,
    /// Phase 108.C.uorb.2 — registered MessageLost callback. Fires
    /// from `try_recv_raw` after a successful receive whenever
    /// `RawSubscription::missed_count()` reports a non-zero gap. uORB
    /// only surfaces one Tier-1 event kind, so a single slot suffices.
    msg_lost_cb: Cell<Option<MsgLostReg>>,
    /// Cumulative lost count over the entity's lifetime — reported in
    /// `CountStatus::total_count` per the nros event contract.
    total_lost: Cell<u32>,
}

#[derive(Clone, Copy)]
struct MsgLostReg {
    cb: nros_rmw::EventCallback,
    user_ctx: *mut c_void,
}

impl UorbSubscriber {
    /// Construct a subscriber bound to `metadata` on the given
    /// multi-instance index. Lazy-registers the underlying
    /// callback on first `try_recv_raw` / `register_waker`.
    pub(crate) fn new(metadata: &'static orb_metadata, instance: u8) -> Self {
        Self {
            inner: RawSubscription::with_instance(metadata, instance),
            msg_lost_cb: Cell::new(None),
            total_lost: Cell::new(0),
        }
    }

    /// Borrow the metadata this subscriber was constructed with.
    pub fn metadata(&self) -> &'static orb_metadata {
        self.inner.metadata()
    }

    /// Phase 108.C.uorb.2 — fire the registered MessageLost callback
    /// (if any) when uORB reports messages were dropped between the
    /// previous and current `try_recv` step.
    fn check_lost_and_fire(&self) {
        let lost = self.inner.missed_count();
        if lost == 0 {
            return;
        }
        // Accumulate lifetime total so CountStatus.total_count is
        // monotonic per ROS event semantics.
        let total = self.total_lost.get().saturating_add(lost);
        self.total_lost.set(total);
        if let Some(reg) = self.msg_lost_cb.get() {
            let status = nros_rmw::CountStatus {
                total_count: total,
                total_count_change: lost,
            };
            // SAFETY: cb is `unsafe extern "C" fn` matching the
            // EventCallback signature; user_ctx outlives this call
            // (entity owns the Box backing it; freed in
            // nros-node's per-entity event-registry on Drop).
            unsafe {
                (reg.cb)(
                    nros_rmw::EventKind::MessageLost,
                    &status as *const _ as *const c_void,
                    reg.user_ctx,
                );
            }
        }
    }
}

impl core::fmt::Debug for UorbSubscriber {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("UorbSubscriber").finish()
    }
}

impl Subscriber for UorbSubscriber {
    type Error = TransportError;

    fn try_recv_raw(&mut self, buf: &mut [u8]) -> Result<Option<usize>, Self::Error> {
        let n = self.inner.try_recv(buf);
        // Drain the lost counter unconditionally — between two polls
        // there may have been a lost burst even if the most recent
        // try_recv returns None (publisher fired N messages but the
        // most recent landed in the buffer slot the previous poll
        // already consumed; rare but bounded by topic semantics).
        self.check_lost_and_fire();
        Ok(n)
    }

    fn deserialization_error(&self) -> Self::Error {
        TransportError::DeserializationError
    }

    fn register_waker(&self, waker: &core::task::Waker) {
        self.inner.register_waker(waker);
    }

    fn supports_event(&self, kind: nros_rmw::EventKind) -> bool {
        matches!(kind, nros_rmw::EventKind::MessageLost)
    }

    unsafe fn register_event_callback(
        &mut self,
        kind: nros_rmw::EventKind,
        _deadline_ms: u32,
        cb: nros_rmw::EventCallback,
        user_ctx: *mut c_void,
    ) -> Result<(), TransportError> {
        match kind {
            nros_rmw::EventKind::MessageLost => {
                self.msg_lost_cb.set(Some(MsgLostReg { cb, user_ctx }));
                Ok(())
            }
            _ => Err(TransportError::Unsupported),
        }
    }
}

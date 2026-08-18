//! Phase 104.E.3 — cross-priority handoff queue.
//!
//! Bridges that span priority boundaries (sub callback at
//! priority A, pub callback at priority B) need a bounded
//! handoff queue between the two so the high-priority sub
//! doesn't block on the lower-priority pub's transport drain.
//! The existing pattern is `Arc<Mutex<heapless::Vec<M, N>>>` +
//! a timer-driven pub; this module wraps it in a small
//! `Handoff<M, N>` type so bridge code stays terse.
//!
//! Optional sugar — the manual pattern remains supported. The
//! spec (Phase 104.E.3) explicitly lists this as "optional"
//! convenience to avoid forcing every bridge to use the same
//! shape.
//!
//! ```ignore
//! use nros_node::executor::handoff::Handoff;
//! use std::sync::Arc;
//!
//! // Shared bounded queue, N = 32, message type M.
//! let q: Arc<Handoff<MyMsg, 32>> = Arc::new(Handoff::new());
//!
//! // High-priority ingress: push into the queue inside the
//! // sub callback. `push` is non-blocking — overflow returns
//! // `Err(msg)` so the high-pri side never stalls.
//! let q_pub = Arc::clone(&q);
//! executor.register_subscription::<MyMsg, _>(topic, move |msg: &MyMsg| {
//!     let _ = q_pub.push(msg.clone());  // drop on overflow
//! })?;
//!
//! // Low-priority egress: timer drains the queue + publishes.
//! let q_sub = Arc::clone(&q);
//! let pub_out = ...;
//! executor.register_timer(period, move || {
//!     while let Some(msg) = q_sub.pop() {
//!         let _ = pub_out.publish(&msg);
//!     }
//! })?;
//! ```
//!
//! Cross-priority safety: every `push` / `pop` takes the
//! internal mutex for the duration of one queue slot
//! operation (O(1)). On PiCAS-aware dispatchers (Phase 110.F)
//! the mutex inherits the holder's effective priority, so
//! the low-pri drain doesn't priority-invert the high-pri
//! push.
//!
//! `std`-gated, and phase-359 W10 measured WHY — the reason recorded above was
//! not the one that holds.
//!
//! It said the `alloc` path "needs a lock-free SPSC queue (heapless::spsc
//! requires a `.split()` call that doesn't compose with Arc-sharing)". That
//! describes a design this module does not use: what is here is a
//! `heapless::Vec` behind a MUTEX, and the only thing keeping it hosted is
//! `std::sync::Mutex`.
//!
//! A portable one exists — `nros_rmw::sync::Mutex`, spin or critical-section —
//! but `nros-node` depends on `nros-rmw` with `default-features = false` and
//! neither sync feature is on, so reaching it means adding a dependency edge
//! (`spin`) to EVERY build of this crate. `Handoff` has no consumer anywhere in
//! the tree — not one call site in `packages/` or `examples/` — so that trade
//! is not obviously worth making, and making it silently would be worse.
//!
//! Left std-gated deliberately. The decision this wants is whether an unused
//! public API justifies a dependency in a core crate, which is a question for
//! whoever owns the API rather than for the flavour campaign.

use std::sync::Mutex;

use heapless::Vec;

/// Bounded FIFO between two callbacks running on different
/// `SchedContext`s. Generic over message type `M` (must be
/// `Send` for cross-thread executors) and capacity `N`.
///
/// Constructed via [`Handoff::new`]; share between callbacks
/// via `std::sync::Arc<Handoff<M, N>>`.
#[derive(Debug)]
pub struct Handoff<M, const N: usize> {
    inner: Mutex<Vec<M, N>>,
}

impl<M, const N: usize> Default for Handoff<M, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M, const N: usize> Handoff<M, N> {
    /// Empty queue.
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Vec::new()),
        }
    }

    /// Push a message. Non-blocking on the high-priority side —
    /// returns `Err(msg)` when the queue is full so the caller
    /// can decide whether to drop, overwrite, or escalate.
    /// O(1) under the internal mutex.
    pub fn push(&self, msg: M) -> Result<(), M> {
        let Ok(mut guard) = self.inner.lock() else {
            return Err(msg);
        };
        guard.push(msg)
    }

    /// Pop one message. Returns `None` when the queue is empty.
    /// O(N) under the internal mutex (shifts the tail of the
    /// `heapless::Vec`); switch to a true ring buffer if the
    /// dispatcher's bench shows this as a hotspot.
    pub fn pop(&self) -> Option<M> {
        let mut guard = self.inner.lock().ok()?;
        if guard.is_empty() {
            None
        } else {
            Some(guard.remove(0))
        }
    }

    /// Current depth. Useful for monitoring + telemetry.
    pub fn len(&self) -> usize {
        self.inner.lock().map(|g| g.len()).unwrap_or(0)
    }

    /// True when empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// True when at capacity. Caller's `push` will return
    /// `Err(msg)` on the next call until a `pop` drains a slot.
    pub fn is_full(&self) -> bool {
        self.len() >= N
    }

    /// Compile-time capacity.
    pub const fn capacity(&self) -> usize {
        N
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_pop_fifo() {
        let q: Handoff<i32, 4> = Handoff::new();
        assert!(q.is_empty());
        assert_eq!(q.capacity(), 4);
        q.push(1).unwrap();
        q.push(2).unwrap();
        q.push(3).unwrap();
        assert_eq!(q.len(), 3);
        assert_eq!(q.pop(), Some(1));
        assert_eq!(q.pop(), Some(2));
        assert_eq!(q.pop(), Some(3));
        assert_eq!(q.pop(), None);
    }

    #[test]
    fn overflow_returns_err_msg() {
        let q: Handoff<i32, 2> = Handoff::new();
        q.push(1).unwrap();
        q.push(2).unwrap();
        assert!(q.is_full());
        assert_eq!(q.push(3), Err(3));
        // Drain + retry succeeds.
        q.pop();
        q.push(3).unwrap();
        assert_eq!(q.len(), 2);
    }

    #[test]
    fn shared_across_threads() {
        use std::{sync::Arc, thread};

        let q: Arc<Handoff<u32, 8>> = Arc::new(Handoff::new());
        let q_prod = Arc::clone(&q);
        let producer = thread::spawn(move || {
            for i in 0..16u32 {
                let _ = q_prod.push(i);
                thread::sleep(core::time::Duration::from_micros(50));
            }
        });
        let mut drained: Vec<u32, 32> = Vec::new();
        for _ in 0..200 {
            while let Some(v) = q.pop() {
                let _ = drained.push(v);
            }
            thread::sleep(core::time::Duration::from_micros(100));
        }
        producer.join().unwrap();
        // Best-effort drain — some pushes may have hit the
        // bounded cap and returned Err. We assert at least
        // SOME drained + monotonic order on whatever made it
        // through.
        assert!(!drained.is_empty());
        for w in drained.windows(2) {
            assert!(w[0] < w[1]);
        }
    }
}

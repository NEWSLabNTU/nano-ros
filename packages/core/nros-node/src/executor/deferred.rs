//! Issue 2146 (RFC-0112 D2, the **Defer** part of the Tier 0 contract) --
//! deferred dispatch for a hand-written host.
//!
//! A host that schedules work by priority (RTIC, a work queue, an RTOS task
//! per callback class) wants the executor to find out WHAT is ready, and to
//! run it LATER, in a task the host picks. Until this module the only form of
//! that was the per-node `__nros_node_<pkg>_on_callback` slot ABI that
//! `nros::node!` emitted, so a caller that did not go through the macros could
//! not use it. (Nothing ever filled or drained that registry; issue 2161
//! retired it.)
//!
//! The shape is three calls on [`Executor`], with no macro-private type:
//!
//! 1. [`Executor::set_deferred`] marks one registered callback (by the
//!    [`HandleId`] its registration returned) as deferred. The spin never runs
//!    it inline again.
//! 2. [`Executor::set_defer_notifier`] installs a plain
//!    `fn(ctx, HandleId)`. When a deferred callback becomes ready, the spin
//!    ENQUEUES it and calls the notifier once, so the host can pend the task
//!    that will run it. The notifier runs on the spinning thread, inside the
//!    pass: it must not block and cannot reach the executor.
//! 3. The host drains from its own task: [`Executor::run_deferred`] runs every
//!    queued callback, [`Executor::run_deferred_handle`] runs one (for a host
//!    that maps callbacks onto tasks of different priorities).
//!
//! The queue is one bit per callback slot (the executor already caps its
//! callback table at 64 slots, `MAX_CALLBACK_SLOTS`), so it allocates nothing
//! and an enqueue of a callback that is already queued is a no-op: the
//! callback runs once per drain, and takes what one dispatch takes. Data left
//! behind is found by the next spin, which queues the callback again.
//!
//! Timers cannot be deferred. A timer's readiness is decided INSIDE its
//! dispatch (it consumes the spin's elapsed time there), so there is nothing
//! to enqueue ahead of running it; [`Executor::set_deferred`] refuses one.

use super::{
    arena::EntryKind,
    spin::Executor,
    types::{CallbackMark, DeferNotify, HandleId, SpinOnceResult},
};

impl<'s> Executor<'s> {
    /// Defer (or stop deferring) one registered callback.
    ///
    /// A deferred callback is never run by `spin_once` / `spin_some`: when it
    /// is ready the spin queues it and calls the notifier from
    /// [`set_defer_notifier`](Self::set_defer_notifier), and it runs when the
    /// host calls [`run_deferred`](Self::run_deferred) or
    /// [`run_deferred_handle`](Self::run_deferred_handle).
    ///
    /// Returns `false`, changing nothing, when `id` names no live callback or
    /// names a timer (see the module docs). Turning deferral OFF also drops
    /// the callback from the queue; it stays ready, so the next spin runs it
    /// inline.
    ///
    /// A deferred callback takes no part in the spin's trigger
    /// ([`set_trigger`](Self::set_trigger)): it is neither "ready" for
    /// `Trigger::Any` nor required by `Trigger::All`.
    pub fn set_deferred(&mut self, id: HandleId, deferred: bool) -> bool {
        let Some(slot) = self.resolve_handle(id) else {
            return false;
        };
        let Some(meta) = self.entries.get(slot).and_then(|e| e.as_ref()) else {
            return false;
        };
        if matches!(meta.kind, EntryKind::Timer) {
            return false;
        }
        let bit = 1u64 << slot;
        if deferred {
            self.deferred_mask |= bit;
        } else {
            self.deferred_mask &= !bit;
            self.deferred_pending &= !bit;
        }
        true
    }

    /// phase-520 W3 -- snapshot which callback slots are occupied now, for
    /// [`set_deferred_since`](Self::set_deferred_since).
    pub fn callback_mark(&self) -> CallbackMark {
        let mut occupied = 0u64;
        for (slot, entry) in self.entries.iter().enumerate().take(64) {
            if entry.is_some() {
                occupied |= 1u64 << slot;
            }
        }
        CallbackMark(occupied)
    }

    /// phase-520 W3 -- defer (or stop deferring) every callback registered
    /// since `mark` was taken: the slots occupied now that were not occupied
    /// then. Returns how many callbacks it changed.
    ///
    /// This is how a whole component is deferred by whoever installs it
    /// without naming its handles: take the mark, run the component's
    /// `register`, then defer since the mark. `nros::setup!` does exactly that
    /// for a `[[component]]` with `dispatch = "deferred"`. Timers are skipped
    /// (they cannot be deferred, see the module docs) and are not counted.
    pub fn set_deferred_since(&mut self, mark: CallbackMark, deferred: bool) -> usize {
        let fresh = self.callback_mark().0 & !mark.0;
        let mut changed = 0;
        let mut rest = fresh;
        while rest != 0 {
            let slot = rest.trailing_zeros() as usize;
            rest &= rest - 1;
            if self.set_deferred(self.handle_for(slot), deferred) {
                changed += 1;
            }
        }
        changed
    }

    /// Whether `id` names a deferred callback.
    pub fn is_deferred(&self, id: HandleId) -> bool {
        self.resolve_handle(id)
            .is_some_and(|slot| self.deferred_mask & (1u64 << slot) != 0)
    }

    /// Install the notifier the spin calls when it queues a deferred
    /// callback, with the context it is called with. Replaces any earlier
    /// notifier.
    ///
    /// Called once per queueing: a callback that is already queued is not
    /// queued (or notified) again until a drain has run it. With no notifier
    /// the queue still fills, and the host can poll
    /// [`has_deferred`](Self::has_deferred).
    ///
    /// `ctx` is handed back verbatim and never dereferenced by the executor;
    /// whatever it points to must stay valid while the notifier is installed.
    pub fn set_defer_notifier(&mut self, notify: DeferNotify, ctx: *mut core::ffi::c_void) {
        self.defer_notify = Some((notify, ctx));
    }

    /// Remove the notifier. Deferred callbacks keep being queued.
    pub fn clear_defer_notifier(&mut self) {
        self.defer_notify = None;
    }

    /// Whether any deferred callback is queued and waiting for a drain.
    pub fn has_deferred(&self) -> bool {
        self.deferred_pending != 0
    }

    /// Whether the deferred callback `id` is queued.
    pub fn is_deferred_pending(&self, id: HandleId) -> bool {
        self.resolve_handle(id)
            .is_some_and(|slot| self.deferred_pending & (1u64 << slot) != 0)
    }

    /// Run every queued deferred callback once, in slot (registration)
    /// order, and empty the queue. The counts are `spin_once`'s.
    ///
    /// Call this from the host task that owns deferred work. It does no I/O
    /// and never waits: a callback that finds no data (it was taken some
    /// other way) is an `Ok(false)` dispatch, counted as nothing.
    pub fn run_deferred(&mut self) -> SpinOnceResult {
        let mut result = SpinOnceResult::new();
        self.reap_deferred_releases();
        let mut rest = self.deferred_pending;
        while rest != 0 {
            let slot = rest.trailing_zeros() as usize;
            rest &= rest - 1;
            self.dispatch_deferred_slot(slot, &mut result);
        }
        result
    }

    /// Run the deferred callback `id` once if it is queued. Returns whether it
    /// ran (`false` when it was not queued or `id` names no live callback).
    ///
    /// For a host that maps deferred callbacks onto tasks of different
    /// priorities: the notifier says which one is ready, and the task that
    /// owns it runs exactly that one.
    pub fn run_deferred_handle(&mut self, id: HandleId) -> bool {
        let Some(slot) = self.resolve_handle(id) else {
            return false;
        };
        self.reap_deferred_releases();
        let mut result = SpinOnceResult::new();
        self.dispatch_deferred_slot(slot, &mut result)
    }

    /// The spin's half: move every deferred bit of `ready` into the queue,
    /// notify the newly queued ones, and return what is left for the spin to
    /// dispatch inline.
    pub(crate) fn defer_ready(&mut self, ready: u64) -> u64 {
        let deferred = ready & self.deferred_mask;
        if deferred == 0 {
            return ready;
        }
        let fresh = deferred & !self.deferred_pending;
        self.deferred_pending |= deferred;
        if let Some((notify, ctx)) = self.defer_notify {
            let mut rest = fresh;
            while rest != 0 {
                let slot = rest.trailing_zeros() as usize;
                rest &= rest - 1;
                notify(ctx, self.handle_for(slot));
            }
        }
        ready & !self.deferred_mask
    }

    /// Forget a slot's deferral when its entry is released, so the slot's
    /// next occupant starts inline and unqueued.
    pub(crate) fn forget_deferred(&mut self, slot: usize) {
        if slot < 64 {
            let bit = 1u64 << slot;
            self.deferred_mask &= !bit;
            self.deferred_pending &= !bit;
        }
    }

    fn dispatch_deferred_slot(&mut self, slot: usize, result: &mut SpinOnceResult) -> bool {
        if slot >= 64 {
            return false;
        }
        let bit = 1u64 << slot;
        if self.deferred_pending & bit == 0 {
            return false;
        }
        // Held: its release is pending (it never runs again) or its callback
        // is already running below us. Either way not now; a held slot that
        // still has data is queued again by the next spin.
        if Self::slot_held(self.slot_tags, slot) {
            return false;
        }
        self.deferred_pending &= !bit;
        let Some(meta) = self.entries.get(slot).and_then(|e| e.as_ref()) else {
            return false;
        };
        let kind = meta.kind;
        let try_process = meta.try_process;
        // SAFETY: `meta.offset` is this live entry's arena slot, written by its
        // registration; `try_process` matches the entry's concrete type.
        let data_ptr = unsafe { (self.arena.as_mut_ptr() as *mut u8).add(meta.offset) };
        Self::begin_dispatch(self.slot_tags, slot);
        // SAFETY: as the spin's own dispatch: `data_ptr` is the live entry
        // `try_process` was registered for, and nothing else runs it (the slot
        // is marked dispatching). `delta_us` is only read by a timer, which
        // cannot be deferred, so 0 is never observed.
        let processed = unsafe { try_process(data_ptr, 0, slot as u8) };
        self.finish_dispatch(slot);
        // One dispatch takes what one call takes; an attributed slot is asked
        // `has_data` again next spin, exactly as the inline drain arranges.
        #[cfg(all(feature = "alloc", feature = "rmw-cffi"))]
        {
            self.entity_recheck |= bit & self.entity_attributed;
        }
        match processed {
            Ok(true) => match kind {
                EntryKind::Subscription => result.subscriptions_processed += 1,
                EntryKind::Service
                | EntryKind::ServiceClient
                | EntryKind::ActionServer
                | EntryKind::ActionClient => result.services_handled += 1,
                EntryKind::Timer => result.timers_fired += 1,
                EntryKind::GuardCondition => {}
            },
            Ok(false) => {}
            Err(_) => match kind {
                EntryKind::Subscription => result.subscription_errors += 1,
                EntryKind::Service
                | EntryKind::ServiceClient
                | EntryKind::ActionServer
                | EntryKind::ActionClient => result.service_errors += 1,
                EntryKind::Timer | EntryKind::GuardCondition => {}
            },
        }
        true
    }
}

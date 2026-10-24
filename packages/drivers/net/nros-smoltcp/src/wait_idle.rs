//! issue 2022 — a tickless idle for the transport's own waits.
//!
//! A bare-metal transport is poll-only: zenoh-pico and XRCE read the network
//! only by polling smoltcp, so RFC-0106 D5 keeps the receive-blocking wait on
//! bare metal and the executor never parks there. The wait that actually runs
//! is a loop — `zpico_spin_once`'s read loop, a TCP connect, a `read_exact` —
//! and until this module every one of them busy-polled until its deadline.
//!
//! The obvious place for an idle, [`do_poll`][crate::do_poll]'s idle hook, is
//! the wrong one for a tickless sleep: it runs right after the poll and BEFORE
//! the loop looks at what the poll delivered, so sleeping there to the loop's
//! deadline would hold a reply that has already arrived. The board-installed
//! `wfi` there is safe only because the RTIC tick bounds it to 1 ms. So a wait
//! loop calls [`idle_until_ms`] itself, after it has checked and found
//! nothing, with the deadline it is already measuring against.
//!
//! What the board supplies is the primitive: [`WaitIdleFn`] sleeps until the
//! deadline or until something that could change the answer happens (a frame
//! arrives). This module only narrows the deadline by smoltcp's own timers
//! (retransmit, delayed ACK, ARP), so a sleeping loop never sits on a stack
//! that needs servicing. With no primitive installed the call returns at once
//! and the loop busy-polls exactly as before, which is every board but the one
//! that installs it.

use core::sync::atomic::{AtomicPtr, Ordering};

use crate::deadline::{NOTHING_PENDING_US, deadline_source_ctx, next_deadline_us};

unsafe extern "C" {
    /// The platform wall clock, nanoseconds — the one the bridge polls with.
    fn nros_platform_time_now_ns() -> u64;
}

/// A board's sleep primitive: return no later than `deadline_ns` (on the
/// platform clock), and sooner if a frame arrives.
///
/// It may return early for any reason; every caller re-polls and re-checks.
/// It must NOT sleep when a frame is already waiting — the check and the sleep
/// belong under one interrupt mask, or a frame that lands between them is a
/// lost wake.
pub type WaitIdleFn = unsafe extern "C" fn(deadline_ns: u64);

static WAIT_IDLE: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

/// Install the board's sleep primitive. Only a board whose primitive has a
/// wake source for incoming frames may install one; without it a sleep to the
/// deadline is the blind sleep RFC-0106 D5 forbids.
pub fn set_wait_idle(f: WaitIdleFn) {
    WAIT_IDLE.store(f as *mut (), Ordering::Release);
}

/// Remove the primitive. Wait loops go back to busy-polling.
pub fn clear_wait_idle() {
    WAIT_IDLE.store(core::ptr::null_mut(), Ordering::Release);
}

/// Whether a primitive is installed.
pub fn has_wait_idle() -> bool {
    !WAIT_IDLE.load(Ordering::Acquire).is_null()
}

/// The deadline a sleep may run to, or `None` when it must not sleep at all.
///
/// `deadline_ms` is the loop's own (on `SmoltcpBridge::clock_now_ms`, which is
/// the nanosecond clock floored, so `deadline_ms * 1e6` is the instant that
/// clock first reads `deadline_ms`). `stack_us` is
/// [`next_deadline_us`]'s answer, [`NOTHING_PENDING_US`] meaning none.
pub(crate) fn sleep_deadline_ns(deadline_ms: u64, now_ns: u64, stack_us: u64) -> Option<u64> {
    let mut deadline_ns = deadline_ms.saturating_mul(1_000_000);
    if stack_us != NOTHING_PENDING_US {
        deadline_ns = deadline_ns.min(now_ns.saturating_add(stack_us.saturating_mul(1_000)));
    }
    (deadline_ns > now_ns).then_some(deadline_ns)
}

/// Called by a wait loop that has polled, checked, and found nothing to do.
/// Sleeps until `deadline_ms`, smoltcp's next timer, or a frame — whichever is
/// first — if the board installed a primitive; returns at once otherwise.
pub fn idle_until_ms(deadline_ms: u64) {
    let f = WAIT_IDLE.load(Ordering::Acquire);
    if f.is_null() {
        return;
    }
    // SAFETY: the platform clock ABI, resolved for every image that links this
    // crate (the bridge polls with it).
    let now_ns = unsafe { nros_platform_time_now_ns() };
    // SAFETY: the context is the static source, armed by `NetworkState::set`;
    // no `&mut` to the interface is live here, since the loop's poll returned.
    let stack_us = unsafe { next_deadline_us(deadline_source_ctx()) };
    if let Some(deadline_ns) = sleep_deadline_ns(deadline_ms, now_ns, stack_us) {
        // SAFETY: only `set_wait_idle` stores here, and it takes a `WaitIdleFn`.
        let f: WaitIdleFn = unsafe { core::mem::transmute::<*mut (), WaitIdleFn>(f) };
        // SAFETY: the board's primitive, called from the loop's own context.
        unsafe { f(deadline_ns) };
    }
}

/// [`idle_until_ms`] for the zpico C shim's read loop (`zpico_spin_once`'s
/// smoltcp branch), which owns the deadline of the steady-state wait.
#[unsafe(no_mangle)]
pub extern "C" fn nros_smoltcp_idle_until_ms(deadline_ms: u64) {
    idle_until_ms(deadline_ms);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The loop's own deadline, with nothing scheduled in the stack.
    #[test]
    fn an_idle_stack_sleeps_to_the_loop_deadline() {
        assert_eq!(
            sleep_deadline_ns(1_010, 1_000_000_000, NOTHING_PENDING_US),
            Some(1_010_000_000)
        );
    }

    /// A retransmit due before the loop's deadline wakes the sleep for it.
    #[test]
    fn a_stack_timer_shortens_the_sleep() {
        assert_eq!(
            sleep_deadline_ns(1_010, 1_000_000_000, 2_000),
            Some(1_002_000_000)
        );
    }

    /// And never lengthens it: a stack timer past the loop's deadline is moot.
    #[test]
    fn a_later_stack_timer_does_not_lengthen_it() {
        assert_eq!(
            sleep_deadline_ns(1_010, 1_000_000_000, 50_000),
            Some(1_010_000_000)
        );
    }

    /// An overdue stack (`Some(0)` from `poll_delay`, e.g. a socket with data
    /// it can send now) must not sleep at all — the next poll services it.
    #[test]
    fn an_overdue_stack_does_not_sleep() {
        assert_eq!(sleep_deadline_ns(1_010, 1_000_000_000, 0), None);
    }

    /// A loop already at its deadline does not sleep either.
    #[test]
    fn a_passed_deadline_does_not_sleep() {
        assert_eq!(
            sleep_deadline_ns(1_000, 1_000_000_000, NOTHING_PENDING_US),
            None
        );
        assert_eq!(
            sleep_deadline_ns(999, 1_000_000_000, NOTHING_PENDING_US),
            None
        );
    }
}

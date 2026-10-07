//! One dispatch handed to an OS-priority worker, and what the worker does with
//! it — issue 1705.
//!
//! Split out of `os_priority` so the HAND-OFF protocol compiles, and is
//! tested, in every configuration. The worker POOL needs a platform (tasks and
//! a wake primitive, `rmw-cffi`), and the unit tests run without one. What
//! makes a worker dispatch safe against a concurrent release is not the pool,
//! though. It is the flag protocol on the entry's [`SlotTag`]:
//!
//! 1. The spin thread sets `WORKER_QUEUED` BEFORE it enqueues the item (after,
//!    and a fast worker could clear a bit that was never set, leaving it set
//!    for good). If the enqueue is refused it clears the bit again and runs
//!    the entry cooperatively.
//! 2. While `WORKER_QUEUED` is set, the spin thread neither re-sends the entry
//!    nor runs it cooperatively, so exactly one thread is ever inside its
//!    `try_process`. `os_priority` stated this invariant from the start, and
//!    until 1705 nothing enforced it: every ready cycle sent the entry again.
//! 3. A release that finds `WORKER_QUEUED` set marks `RELEASE_PENDING` and
//!    returns. The entry and its arena bytes stay intact.
//! 4. The worker, here, runs the item only if no release is pending, then
//!    clears `WORKER_QUEUED` with Release ordering. A queued item for a
//!    released entry is therefore never dispatched.
//! 5. The spin thread's `reap_deferred_releases` (top of every `spin_once`)
//!    sees `RELEASE_PENDING` with nothing in flight (Acquire) and completes
//!    the release.

// The pool's own predicate (see `os_priority`), or a unit-test build, which
// drives this protocol without the pool.
#![cfg(any(
    // The tests that drive it: `executor/tests.rs`'s own predicate, plus `std`
    // (they run the worker body on a `std::thread`).
    all(test, feature = "std", not(feature = "rmw-cffi")),
    all(
        has_rmw,
        feature = "alloc",
        feature = "rmw-cffi",
        feature = "scheduler-os-priority"
    )
))]

use super::types::SlotTag;

/// One dispatch handed to a worker.
///
/// Addresses as integers rather than pointers so the item is plainly `Send`:
/// the executor's arena and its slot-tag table outlive every worker (the pool
/// is dropped, which JOINS each worker, before the executor's backing is
/// released), and the worker reconstitutes the addresses on the far side.
#[derive(Clone, Copy)]
pub(crate) struct WorkItem {
    pub(crate) arena_base: usize,
    pub(crate) arena_offset: usize,
    pub(crate) try_process: unsafe fn(*mut u8, u64, u8) -> Result<bool, nros_rmw::TransportError>,
    pub(crate) delta_us: u64,
    /// The entry's slot index, carried so the leaf callback hooks can name
    /// the callback they bracket (phase 8,
    /// `docs/design/callback_tracing.rst`). This path is the reason those
    /// hooks had to be thread-safe from day one: it runs `try_process` on a
    /// WORKER task, so two callbacks can legitimately be in flight at once
    /// and their events interleave in the capture. Keying every event on the
    /// handle, rather than holding an open span in a single slot, is what
    /// lets the decoder pair them anyway.
    pub(crate) desc_idx: u8,
    /// Issue 1705 — the address of the entry's [`SlotTag`], whose
    /// `WORKER_QUEUED` bit this item holds until [`run_work_item`] clears it.
    pub(crate) tag: usize,
}

// SAFETY: the item carries addresses, not borrows. What they reach is the
// executor's arena and slot-tag table, which outlive every worker (see the
// struct docs). Exclusive access to the ENTRY is the `WORKER_QUEUED` protocol
// in the module docs: while this item exists, no other thread runs the entry's
// `try_process` and no release drops it.
unsafe impl Send for WorkItem {}

/// Run one item on the worker task: dispatch the entry unless its release is
/// pending, then let go of it.
///
/// # Safety
/// `item` must have been built by the spin thread for a live entry, with
/// `WORKER_QUEUED` set on `item.tag` before it was handed over, and the
/// executor that owns the arena and the tag table must outlive this call.
pub(crate) unsafe fn run_work_item(item: &WorkItem) {
    // SAFETY: the caller's contract: `tag` addresses a `SlotTag` in a table
    // that outlives this call. Only its atomic `flags` is touched here.
    let tag = unsafe { &*(item.tag as *const SlotTag) };
    if tag.flags() & SlotTag::RELEASE_PENDING == 0 {
        let data = (item.arena_base as *mut u8).wrapping_add(item.arena_offset);
        // SAFETY: `WORKER_QUEUED` is held, so the entry is live and no other
        // thread is in its `try_process` (module docs, steps 2 and 3).
        let _ = unsafe { (item.try_process)(data, item.delta_us, item.desc_idx) };
    }
    // Release: everything the callback wrote happens-before the spin thread's
    // Acquire read that sees the bit clear and drops the entry.
    tag.clear(SlotTag::WORKER_QUEUED);
}

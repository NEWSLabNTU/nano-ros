//! phase-359 W10 — one place that spawns a platform task from Rust.
//!
//! Two executor-owned workers need a thread: the per-OS-priority pool
//! ([`super::os_priority`]) and the signalfd forwarder in `spin.rs`. Both were
//! `std::thread`; both are platform tasks now, so the allocate-spawn-join
//! sequence lives here rather than being written twice.
//!
//! The storage size is ASKED FOR (`nros_platform_task_storage_{size,align}`),
//! never assumed. A hard-coded size is issue 0570 exactly: a Rust-side
//! `pthread_attr_t` mirror was 36 bytes shorter than NuttX's, and
//! `pthread_attr_init` wrote the difference into the caller's frame. The probes
//! exist so no caller has to guess.

// Both consumers are feature-selected AND one is `target_os = "linux"`, so
// there are reachable combinations (e.g. `signal-fd-wake` off-Linux with no
// `scheduler-os-priority`) where this module compiles with no caller. Enumerating
// them in the `mod` gate would be a predicate nobody can keep correct; saying so
// once here is the cheaper truth.
#![allow(dead_code)]

use core::ffi::c_void;

unsafe extern "C" {
    fn nros_platform_task_init(
        task: *mut c_void,
        attr: *mut c_void,
        entry: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
        arg: *mut c_void,
    ) -> i8;
    fn nros_platform_task_join(task: *mut c_void) -> i8;
    fn nros_platform_task_storage_size() -> usize;
    fn nros_platform_task_storage_align() -> usize;
}

/// A spawned platform task plus the storage the platform tracks it in.
///
/// Joining is [`join`](Self::join) rather than `Drop`, because both callers
/// have to signal their worker to stop BEFORE waiting for it — a `Drop` that
/// joined implicitly would deadlock against a worker still blocked on its own
/// wait.
pub(crate) struct PlatformTask {
    ptr: *mut u8,
    layout: core::alloc::Layout,
}

impl PlatformTask {
    /// Spawn `entry(arg)`, or `None` when this platform cannot host a task
    /// (no storage sizing, allocation failure, or a refused spawn).
    ///
    /// # Safety
    /// `arg` must remain valid and pointed-to until [`join`](Self::join)
    /// returns — the task dereferences it.
    pub(crate) unsafe fn spawn(
        entry: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
        arg: *mut c_void,
    ) -> Option<Self> {
        // SAFETY: both probes are documented pure functions, callable before
        // any task exists.
        let (size, align) = unsafe {
            (
                nros_platform_task_storage_size(),
                nros_platform_task_storage_align(),
            )
        };
        if size == 0 || align == 0 {
            return None;
        }
        let layout = core::alloc::Layout::from_size_align(size, align).ok()?;
        // SAFETY: `layout` has non-zero size.
        let ptr = unsafe { alloc::alloc::alloc(layout) };
        if ptr.is_null() {
            return None;
        }
        // SAFETY: `ptr` is storage of the size/alignment the platform asked
        // for; `entry`/`arg` are the caller's contract.
        let rc = unsafe {
            nros_platform_task_init(ptr as *mut c_void, core::ptr::null_mut(), entry, arg)
        };
        if rc != 0 {
            // SAFETY: same pair just returned by `alloc`; no task took it.
            unsafe { alloc::alloc::dealloc(ptr, layout) };
            return None;
        }
        Some(Self { ptr, layout })
    }

    /// Block until the task exits, then release its storage.
    ///
    /// The caller must already have told the task to stop; this only waits.
    pub(crate) fn join(self) {
        // SAFETY: `ptr` holds the handle `task_init` wrote.
        unsafe { nros_platform_task_join(self.ptr as *mut c_void) };
        // SAFETY: the task has exited, so nothing else references the storage;
        // `ptr`/`layout` are the pair `spawn` allocated.
        unsafe { alloc::alloc::dealloc(self.ptr, self.layout) };
        core::mem::forget(self);
    }
}

impl Drop for PlatformTask {
    fn drop(&mut self) {
        // Reached only if a caller dropped the handle without joining, which
        // leaves a running task pointed at storage we are about to free. Leak
        // the storage instead: a leak is recoverable, a use-after-free by a
        // live task is not.
        //
        // `join` calls `mem::forget`, so the normal path never lands here.
    }
}

//! phase-359 W10 — one place that spawns a platform task from Rust.
//!
//! Moved here from `nros-node::executor::platform_task`: three Rust callers now
//! need it (`nros-node`'s worker pool and `open_threaded`, `nros-cpp`'s native
//! tier runtime), and a helper for an ABI belongs beside the ABI rather than
//! inside one of its consumers.
//!
//! Two executor-owned workers need a thread: the per-OS-priority pool
//! (`os_priority` in `nros-node`'s executor) and the signalfd forwarder in `spin.rs`. Both were
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

#[cfg(feature = "alloc")]
use core::ffi::c_void;

/// The ABI's task attributes, mirrored for the `extern` declaration below.
///
/// phase-364 W3 — this is `nros_platform_task_attr_t` from `<nros/platform.h>`.
/// It is declared here rather than imported because `nros-node` does not depend
/// on `nros-platform-cffi` (where the generated bindings live) — the same reason
/// the wake and task symbols are declared here by hand. The layout is checked
/// against the header by `check-ffi-struct-mirrors`; see the note at the spawn
/// site for what a drift would cost.
//
// Gated with the spawn path that uses it: `stack_unused_bytes` below needs no
// allocator, so the module is compiled without the `alloc` feature too, and an
// ungated private struct would be dead code there. `check-ffi-struct-mirrors`
// reads the source, not a build, so the layout stays checked either way.
#[cfg(feature = "alloc")]
#[repr(C)]
struct TaskAttr {
    name: *const core::ffi::c_char,
    stack_bytes: usize,
    stack_mem: *mut c_void,
    priority: i32,
    core: i8,
    flags: u8,
}

/// `INT32_MIN` — inherit the creating task's priority.
#[cfg(feature = "alloc")]
const PRIORITY_INHERIT: i32 = i32::MIN;

// The stack probe is a plain query with no arguments and no storage, so it is
// declared unconditionally — see `stack_unused_bytes`.
unsafe extern "C" {
    fn nros_platform_task_stack_unused_bytes() -> usize;
}

// Spawning is the half that allocates.
#[cfg(feature = "alloc")]
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
#[cfg(feature = "alloc")]
pub struct PlatformTask {
    ptr: *mut u8,
    layout: core::alloc::Layout,
}

#[cfg(feature = "alloc")]
impl PlatformTask {
    /// Spawn `entry(arg)`, or `None` when this platform cannot host a task
    /// (no storage sizing, allocation failure, or a refused spawn).
    ///
    /// # Safety
    /// `arg` must remain valid and pointed-to until [`join`](Self::join)
    /// returns — the task dereferences it.
    pub unsafe fn spawn(
        entry: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
        arg: *mut c_void,
        stack_bytes: usize,
        name: *const core::ffi::c_char,
    ) -> Option<Self> {
        // SAFETY: forwarded unchanged; the caller's contract is unchanged.
        unsafe { Self::spawn_with(entry, arg, name, stack_bytes, PRIORITY_INHERIT as i64) }
    }

    /// Spawn `entry(arg)` stating a PRIORITY as well.
    ///
    /// phase-359 W10 — `spawn` inherits the creating task's priority, which is
    /// right for the executor's own workers and wrong for a TIER: a tier's
    /// priority is the thing its author declared. `priority <= 0` means
    /// "unstated" and inherits, matching how the board descriptors spell an
    /// absent priority; anything positive is the kernel's own number and is
    /// passed through the band's RAW escape hatch, because a
    /// `[tiers.<name>.<rtos>] priority` is already in the kernel's units.
    ///
    /// # Safety
    /// Same as [`spawn`](Self::spawn).
    pub unsafe fn spawn_with(
        entry: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
        arg: *mut c_void,
        name: *const core::ffi::c_char,
        stack_bytes: usize,
        priority: i64,
    ) -> Option<Self> {
        /// `NROS_PLATFORM_PRIORITY_RAW(n)` from `<nros/platform.h>`.
        const fn raw(n: i32) -> i32 {
            -0x4000_0000 - n
        }
        let priority = i32::try_from(priority)
            .ok()
            .filter(|p| *p > 0)
            .map_or(PRIORITY_INHERIT, raw);
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
        // phase-364 W3 — a real attribute, not `NULL`.
        //
        // Passing `NULL` was correct on four ports and a guaranteed failure on
        // ThreadX, whose `task_init` required an attr carrying the stack. W3
        // made `NULL` mean "every default" everywhere, so `NULL` would work now
        // — but the executor's workers do have a stack size to state, and
        // stating it is what phase-359 W7 had to write a bespoke C shim to do.
        let mut attr = TaskAttr {
            name,
            stack_bytes,
            stack_mem: core::ptr::null_mut(),
            priority,
            core: -1,
            flags: 0,
        };
        // SAFETY: `ptr` is storage of the size/alignment the platform asked
        // for; `attr` outlives the call (the port copies what it needs);
        // `entry`/`arg` are the caller's contract.
        let rc = unsafe {
            nros_platform_task_init(
                ptr as *mut c_void,
                (&raw mut attr) as *mut c_void,
                entry,
                arg,
            )
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
    pub fn join(self) {
        // SAFETY: `ptr` holds the handle `task_init` wrote.
        unsafe { nros_platform_task_join(self.ptr as *mut c_void) };
        // SAFETY: the task has exited, so nothing else references the storage;
        // `ptr`/`layout` are the pair `spawn` allocated.
        unsafe { alloc::alloc::dealloc(self.ptr, self.layout) };
        core::mem::forget(self);
    }
}

#[cfg(feature = "alloc")]
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

/// Smallest number of bytes ever left unused on the CALLING task's stack, or
/// `0` if this port does not instrument it.
///
/// The heap has had `heap_used_bytes` since RFC-0034 D7; the stack had
/// nothing, and stack overflow is how one component corrupts another's state.
/// A tier asks this about ITSELF -- both kernels answer for the calling task
/// with no handle, and answering for an arbitrary one needs a native handle
/// this ABI does not carry.
///
/// `0` means "not instrumented", not "no headroom".
pub fn stack_unused_bytes() -> usize {
    // SAFETY: a plain query with no arguments and no state; every port either
    // answers or returns 0.
    unsafe { nros_platform_task_stack_unused_bytes() }
}

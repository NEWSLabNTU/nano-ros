//! Stack instrumentation — the part of the task ABI that needs no allocator.
//!
//! ISSUE 1080. `stack_unused_bytes` lived in [`crate::task`], which is
//! `#[cfg(feature = "alloc")]` because `PlatformTask` allocates its own stack
//! and join slot. This function allocates nothing — it is one `extern "C"` call
//! returning a `usize` — so the gate was about its NEIGHBOURS, not about it.
//!
//! That mattered the moment something outside `alloc` wanted it.
//! `nros-node`'s `check_stack_headroom_rule` (`cb2be0ca4`) called it
//! unconditionally, and every `no_std`-without-`alloc` image stopped compiling:
//!
//! ```text
//! error[E0433]: cannot find `task` in `nros_platform_api`
//!   --> packages/core/nros-node/src/executor/spin.rs:2415:41
//!   note: found an item that was configured out
//! ```
//!
//! **Guarding the CALL SITE would have been the wrong fix.** The stack-headroom
//! rule is a safety feature, and the targets that lose it under an
//! `#[cfg(feature = "alloc")]` are exactly the small embedded ones where a
//! stack overflow is how one component corrupts another's state. A gate that
//! silently removes a safety check on the platforms it exists for is worse than
//! the build error that revealed it.
//!
//! So the function moves to where its own requirements put it. `crate::task`
//! re-exports it, so every existing caller is unchanged.

unsafe extern "C" {
    fn nros_platform_task_stack_unused_bytes() -> usize;
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

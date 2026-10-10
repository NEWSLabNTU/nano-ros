//! Issue 1535 — what a `void* executor` handed across the language seam IS.
//!
//! Two different objects travel as `void*` and are both called "the executor
//! handle":
//!
//! * a Rust entry's `RuntimeCtx::executor_handle()` — a bare
//!   `*mut Executor<'static>`;
//! * a C/C++ entry's `rclcpp::global_handle()` (and every tier's `executor`) —
//!   an nros-cpp `CppContext`, which carries the executor at a NON-zero offset,
//!   behind the [`CPP_CONTEXT_TAG`] word at offset 0.
//!
//! nros-cpp's own entry points tell them apart by that tag (issue 0436). The
//! seams in THIS crate that take a handle — the cross-language component
//! install `__nros_component_<pkg>_install` and the contract-monitor install —
//! used to cast whatever arrived straight to `*mut Executor<'static>`, so a
//! C/C++ entry installing a Rust node handed the node a pointer that was off by
//! the tag. Whether that crashed depended on where rustc happened to put the
//! fields of a `repr(Rust)` `CppContext`: the mixed Zephyr image SEGVed on its
//! first publish while every layout that put the executor first ran by luck.
//!
//! One answer now, two halves: nros-cpp pins the tag at offset 0 (`repr(C)`)
//! and exports `nros_cpp_executor_inner`, which the generated C++ entry calls
//! before it reaches a Rust install; and [`executor_from_handle`] here REFUSES a
//! handle that is still tagged, so a caller that forgets is a clean error
//! rather than a corrupted executor.

// The classification is crate-internal and its one caller needs `rmw-cffi`;
// it stays un-gated so the `env,std` unit lane, which links no RMW, can test it.
#![cfg_attr(not(feature = "rmw-cffi"), allow(dead_code))]

use core::ffi::c_void;

/// The word nros-cpp stamps at offset 0 of a live `CppContext` (issue 0436).
///
/// Defined HERE, in the lower crate, because both sides read it: nros-cpp
/// stamps and checks it, and [`executor_from_handle`] refuses a handle that
/// carries it. One constant, so the two cannot drift. Arbitrary, but not a
/// plausible pointer or small integer.
pub const CPP_CONTEXT_TAG: u64 = 0x6E52_4F53_4350_5001; // "nROSCP\x50\x01"

/// Whether `handle` points at an nros-cpp executor CONTEXT rather than at a bare
/// `Executor` — i.e. whether its first word is [`CPP_CONTEXT_TAG`].
///
/// # Safety
/// `handle` must be non-null and point to at least 8 readable bytes. Both
/// handle kinds satisfy that: a `CppContext` starts with the tag, and an
/// `Executor` is far larger than one word.
pub(crate) unsafe fn is_cpp_context(handle: *const c_void) -> bool {
    // Unaligned: an `Executor` on a 32-bit target need not be 8-aligned.
    unsafe { core::ptr::read_unaligned(handle as *const u64) == CPP_CONTEXT_TAG }
}

/// Why a handle cannot be used as an `Executor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HandleRefusal {
    /// The handle was null.
    Null,
    /// The handle is an nros-cpp `CppContext`, not an `Executor`. The C/C++
    /// caller must pass `nros_cpp_executor_inner(handle)` instead.
    CppContext,
}

/// Issue 1535 — classify a `void*` handed to a seam that wants a bare
/// `*mut Executor<'static>`: `Ok(())` when it may be cast, otherwise the
/// reason it may not.
///
/// The ONE check every such seam makes before the cast (see
/// [`executor_from_handle`]). Split out so the decision is testable without an
/// executor, which needs an RMW backend linked.
///
/// # Safety
/// `handle` must be null or point to at least 8 readable bytes.
pub(crate) unsafe fn check_executor_handle(handle: *const c_void) -> Result<(), HandleRefusal> {
    if handle.is_null() {
        return Err(HandleRefusal::Null);
    }
    if unsafe { is_cpp_context(handle) } {
        return Err(HandleRefusal::CppContext);
    }
    Ok(())
}

/// Issue 1535 — THE conversion from a `void*` executor handle to the
/// `Executor` behind it, for every seam in this crate that takes one.
///
/// Refuses (and logs, naming `seam`) a null handle or an nros-cpp
/// `CppContext`; the latter is a caller bug that used to corrupt the executor
/// silently.
///
/// # Safety
/// `handle` must be null, an nros-cpp executor context, or a live
/// `*mut Executor<'static>` that nothing else references for the returned
/// borrow's lifetime.
#[cfg(feature = "rmw-cffi")]
pub(crate) unsafe fn executor_from_handle<'a>(
    handle: *mut c_void,
    seam: &'static str,
) -> Result<&'a mut nros_node::Executor<'static>, HandleRefusal> {
    match unsafe { check_executor_handle(handle) } {
        Ok(()) => Ok(unsafe { &mut *(handle as *mut nros_node::Executor<'static>) }),
        Err(refusal) => {
            if refusal == HandleRefusal::CppContext {
                nros_log::log_error!(
                    nros_log::get_logger("nros.executor_handle"),
                    "{}: handed an nros-cpp executor CONTEXT where an Executor was expected; a \
                     C/C++ caller must pass nros_cpp_executor_inner(handle) (issue 1535)",
                    seam
                );
            }
            Err(refusal)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cpp_context_handle_is_refused() {
        // What a live `CppContext` looks like to this crate: the tag at offset
        // 0, then the executor. Before issue 1535 the install seam cast this
        // straight to `*mut Executor` and wrote through the tag.
        let ctx: [u64; 4] = [CPP_CONTEXT_TAG, 0, 0, 0];
        assert_eq!(
            unsafe { check_executor_handle(ctx.as_ptr().cast()) },
            Err(HandleRefusal::CppContext)
        );
    }

    #[test]
    fn the_executor_inside_a_cpp_context_is_accepted() {
        // `nros_cpp_executor_inner` hands over the word AFTER the tag.
        let ctx: [u64; 4] = [CPP_CONTEXT_TAG, 0x1234, 0, 0];
        assert_eq!(
            unsafe { check_executor_handle(ctx[1..].as_ptr().cast()) },
            Ok(())
        );
    }

    #[test]
    fn a_null_handle_is_refused() {
        assert_eq!(
            unsafe { check_executor_handle(core::ptr::null()) },
            Err(HandleRefusal::Null)
        );
    }
}

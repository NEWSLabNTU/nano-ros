//! Fallible heap allocation for entity-creation paths (issue 1551, defect 2).
//!
//! # Why this exists
//!
//! `Box::new` cannot fail: when the global allocator returns NULL it calls
//! `alloc::alloc::handle_alloc_error`, and in a `std` build that runs
//! libstd's `default_alloc_error_hook`, which writes
//! `"memory allocation of N bytes failed"` to **fd 2** before aborting.
//!
//! On a Zephyr `native_sim` image fd 2 is the POSIX fdtable's `stdinout`
//! entry, whose write re-enters `zvfs_write` forever (issue 0589). So the
//! message never appears: the thread overflows its stack inside a recursive
//! `k_mutex` and the process dies with SIGSEGV (exit 139). Measured on the
//! derived-tiers-cpp image, ~104,735 frames deep, from a `Box::new` in the
//! RMW adapter whose C caller had a perfectly good error return.
//!
//! None of the three levers that would make the OOM handler itself safe are
//! available on the pinned stable toolchain (measured on rustc 1.98.1):
//! `std::alloc::set_alloc_error_hook` (feature `alloc_error_hook`),
//! `#[alloc_error_handler]` (feature `alloc_error_handler`) and `-Zoom=panic`
//! are all nightly-only, and a `GlobalAlloc` cannot tell a fallible caller
//! from an infallible one, so the allocator cannot halt on the caller's
//! behalf without also breaking every `try_reserve`. The remaining lever is
//! at the CALL SITE: an allocation whose failure the caller can report must
//! not go through the infallible API at all.
//!
//! # The rule
//!
//! On a path that creates an entity (session, publisher, subscription,
//! service, client, event registration) AND already has an error return to
//! a C/C++ caller, allocate through [`try_box`] and map `Err` to the
//! caller's out-of-memory code (`NROS_RMW_RET_BAD_ALLOC` /
//! [`crate::TransportError::BadAlloc`]). `Box::try_new` is the std spelling
//! and is unstable; this is its stable equivalent, and the one spelling in
//! the tree.

use alloc::boxed::Box;
use core::alloc::Layout;

/// Move `value` into a new heap allocation, or hand it back if the global
/// allocator is exhausted.
///
/// The stable equivalent of the unstable `Box::try_new`. On `Err(value)` the
/// caller still owns `value` and decides how to dispose of it — for a backend
/// handle, dropping it undeclares the entity the backend just created, which
/// is what keeps a failed create from leaking a half-registered entity.
///
/// A zero-sized `T` never allocates and always succeeds, exactly as
/// `Box::new` does.
pub fn try_box<T>(value: T) -> Result<Box<T>, T> {
    let layout = Layout::new::<T>();
    if layout.size() == 0 {
        // `Box::new` of a ZST performs no allocation, so it cannot reach the
        // OOM handler.
        return Ok(Box::new(value));
    }
    // SAFETY: `layout` has non-zero size (checked above).
    let ptr = unsafe { alloc::alloc::alloc(layout) }.cast::<T>();
    if ptr.is_null() {
        return Err(value);
    }
    // SAFETY: `ptr` is a fresh, non-null allocation from the global
    // allocator with `Layout::new::<T>()`, which is exactly the memory layout
    // `Box<T>` documents it owns and frees; `write` initialises it without
    // reading the uninitialised contents.
    unsafe {
        ptr.write(value);
        Ok(Box::from_raw(ptr))
    }
}

/// An UNINITIALISED heap allocation for one `T`, or `None` if the global
/// allocator is exhausted.
///
/// The fallible form of `Box::<T>::new_uninit()`, for a value too large to
/// build on the stack and move in (issue 0756 -- the parameter store is
/// 285,696 bytes at the default sizing, and [`try_box`] takes its value BY
/// VALUE, which materialises all of it in the caller's frame first). The
/// caller initialises through the pointer and then `assume_init`s, exactly as
/// with `new_uninit`.
///
/// Issue 1706 -- that store is the one allocation every parameter
/// declaration reaches, on every RTOS, and `new_uninit` sent its failure to
/// the OOM handler: a halt (Zephyr's `HEAP EXHAUSTED` then issue 0589's
/// recursion) or a `MALLOC FAILED` hang (FreeRTOS heap_4), never a refusal the
/// declaration could return.
pub fn try_box_uninit<T>() -> Option<Box<core::mem::MaybeUninit<T>>> {
    let layout = Layout::new::<T>();
    if layout.size() == 0 {
        return Some(Box::new_uninit());
    }
    // SAFETY: `layout` has non-zero size (checked above).
    let ptr = unsafe { alloc::alloc::alloc(layout) }.cast::<core::mem::MaybeUninit<T>>();
    if ptr.is_null() {
        return None;
    }
    // SAFETY: a fresh, non-null allocation from the global allocator with
    // `Layout::new::<T>()`, which is `MaybeUninit<T>`'s layout too; a
    // `MaybeUninit` needs no initialisation to be owned by a `Box`.
    Some(unsafe { Box::from_raw(ptr) })
}

/// A zeroed `len`-byte heap buffer, or `None` if the allocator is exhausted.
///
/// The fallible form of `vec![0u8; len].into_boxed_slice()`, which aborts
/// through the same OOM hook as `Box::new`. `try_reserve_exact` is stable;
/// the capacity it records is exactly `len`, so `into_boxed_slice` does not
/// reallocate.
pub fn try_zeroed_bytes(len: usize) -> Option<Box<[u8]>> {
    let mut v = alloc::vec::Vec::new();
    v.try_reserve_exact(len).ok()?;
    v.resize(len, 0u8);
    Some(v.into_boxed_slice())
}

#[cfg(test)]
mod tests {
    use super::{try_box, try_zeroed_bytes};

    #[test]
    fn zeroed_bytes_are_zero_and_exact() {
        let b = try_zeroed_bytes(37).expect("host heap is not exhausted");
        assert_eq!(b.len(), 37);
        assert!(b.iter().all(|&x| x == 0));
    }

    #[test]
    fn an_impossible_request_is_none_not_an_abort() {
        // `isize::MAX` bytes is a capacity overflow / allocation failure that
        // `vec![0; n]` would turn into a process abort.
        assert!(try_zeroed_bytes(isize::MAX as usize).is_none());
    }

    #[test]
    fn boxes_a_sized_value() {
        let b = try_box([7u64; 4]).expect("host heap is not exhausted");
        assert_eq!(*b, [7u64; 4]);
    }

    #[test]
    fn an_impossible_uninit_box_is_none_not_an_abort() {
        // Issue 1706 -- the parameter store's allocation, at a size no heap
        // has: `Box::new_uninit` would reach the OOM handler.
        #[cfg(target_pointer_width = "64")]
        assert!(super::try_box_uninit::<[u8; 1usize << 60]>().is_none());
        let mut b = super::try_box_uninit::<[u64; 4]>().expect("host heap is not exhausted");
        b.write([3u64; 4]);
        // SAFETY: written just above.
        assert_eq!(*unsafe { b.assume_init() }, [3u64; 4]);
    }

    #[test]
    fn boxes_a_zero_sized_value() {
        struct Zst;
        assert!(try_box(Zst).is_ok());
    }

    #[test]
    fn a_box_from_try_box_drops_its_value_once() {
        use core::sync::atomic::{AtomicUsize, Ordering};
        static DROPS: AtomicUsize = AtomicUsize::new(0);
        struct Counted;
        impl Drop for Counted {
            fn drop(&mut self) {
                DROPS.fetch_add(1, Ordering::SeqCst);
            }
        }
        drop(try_box(Counted).ok().expect("host heap is not exhausted"));
        assert_eq!(DROPS.load(Ordering::SeqCst), 1);
    }
}

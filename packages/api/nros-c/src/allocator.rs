//! `rcl_allocator_t` — upstream's allocator argument, accepted and never used
//! to allocate (phase-492 W1.b).
//!
//! rcl and rclc thread an allocator through every init call
//! (`rclc_support_init(&support, argc, argv, &allocator)`,
//! `rclc_executor_init(&executor, &support.context, n, &allocator)`,
//! `rcl_clock_init(type, &clock, &allocator)`). Those signatures are taken
//! as rcl's, so a ported call site compiles unchanged — which needs the type
//! and `rcl_get_default_allocator()` to exist.
//!
//! The envelope (RFC-0089 ADOPT-BOUNDED): nothing here allocates. Every
//! object lives in caller storage or the executor's carved backing
//! (RFC-0105 D10), so the allocator has nothing to do. The default one says
//! so if called: `allocate` / `reallocate` / `zero_allocate` return NULL and
//! `deallocate` does nothing. An init that takes an allocator accepts NULL or
//! the default and REFUSES any other one with `RCL_RET_INVALID_ARGUMENT`,
//! because a caller who supplies their own allocator expects it to be used,
//! and silently ignoring it is the "compiles and differs" RFC-0089 forbids.

use core::ffi::c_void;

/// rcutils' allocator, field for field (`rcutils/allocator.h`), under rcl's
/// name. Never used to allocate here; see the module doc.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct rcl_allocator_t {
    /// Allocate `size` bytes.
    pub allocate: Option<unsafe extern "C" fn(size: usize, state: *mut c_void) -> *mut c_void>,
    /// Free a block from `allocate`.
    pub deallocate: Option<unsafe extern "C" fn(pointer: *mut c_void, state: *mut c_void)>,
    /// Resize a block.
    pub reallocate: Option<
        unsafe extern "C" fn(pointer: *mut c_void, size: usize, state: *mut c_void) -> *mut c_void,
    >,
    /// Allocate and zero `number_of_elements * size_of_element` bytes.
    pub zero_allocate: Option<
        unsafe extern "C" fn(
            number_of_elements: usize,
            size_of_element: usize,
            state: *mut c_void,
        ) -> *mut c_void,
    >,
    /// Implementation state, handed to every function.
    pub state: *mut c_void,
}

unsafe extern "C" fn refuse_allocate(_size: usize, _state: *mut c_void) -> *mut c_void {
    core::ptr::null_mut()
}

unsafe extern "C" fn ignore_deallocate(_pointer: *mut c_void, _state: *mut c_void) {}

unsafe extern "C" fn refuse_reallocate(
    _pointer: *mut c_void,
    _size: usize,
    _state: *mut c_void,
) -> *mut c_void {
    core::ptr::null_mut()
}

unsafe extern "C" fn refuse_zero_allocate(
    _number_of_elements: usize,
    _size_of_element: usize,
    _state: *mut c_void,
) -> *mut c_void {
    core::ptr::null_mut()
}

/// The default allocator: rcl's spelling, our envelope. Its functions
/// allocate nothing (NULL from every allocating call). Pass it, or NULL, to
/// any init that takes an allocator.
#[unsafe(no_mangle)]
pub extern "C" fn rcl_get_default_allocator() -> rcl_allocator_t {
    rcl_allocator_t {
        allocate: Some(refuse_allocate),
        deallocate: Some(ignore_deallocate),
        reallocate: Some(refuse_reallocate),
        zero_allocate: Some(refuse_zero_allocate),
        state: core::ptr::null_mut(),
    }
}

/// Is `allocator` one an init may accept: NULL, or the default?
///
/// # Safety
/// `allocator` must be NULL or point to a readable `rcl_allocator_t`.
pub(crate) unsafe fn allocator_is_accepted(allocator: *const rcl_allocator_t) -> bool {
    let Some(a) = (unsafe { allocator.as_ref() }) else {
        return true;
    };
    let d = rcl_get_default_allocator();
    a.allocate.map(|f| f as usize) == d.allocate.map(|f| f as usize)
        && a.deallocate.map(|f| f as usize) == d.deallocate.map(|f| f as usize)
        && a.reallocate.map(|f| f as usize) == d.reallocate.map(|f| f as usize)
        && a.zero_allocate.map(|f| f as usize) == d.zero_allocate.map(|f| f as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_allocator_allocates_nothing_and_is_accepted() {
        let d = rcl_get_default_allocator();
        assert!(unsafe { (d.allocate.unwrap())(16, d.state) }.is_null());
        assert!(unsafe { (d.zero_allocate.unwrap())(2, 8, d.state) }.is_null());
        assert!(unsafe { allocator_is_accepted(&d) });
        assert!(unsafe { allocator_is_accepted(core::ptr::null()) });
    }

    #[test]
    fn a_caller_supplied_allocator_is_refused() {
        unsafe extern "C" fn mine(_s: usize, _st: *mut c_void) -> *mut c_void {
            core::ptr::null_mut()
        }
        let mut a = rcl_get_default_allocator();
        a.allocate = Some(mine);
        assert!(!unsafe { allocator_is_accepted(&a) });
    }
}

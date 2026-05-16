//! Phase 104.B.2 + B.3 — named-registry behaviour tests.
//!
//! Lives as an integration test (separate binary) so the registry
//! `static` starts fresh — unit tests in `lib.rs` share state and
//! couldn't assert "registry is empty initially".

use core::{ffi::c_char, mem::MaybeUninit, sync::atomic::Ordering};

// The deprecated `nros_rmw_cffi_register` is exercised here on purpose:
// this integration test pins the back-compat behaviour of the legacy
// unnamed shim alongside the canonical `_register_named` entry.
#[allow(deprecated)]
use nros_rmw_cffi::nros_rmw_cffi_register;
use nros_rmw_cffi::{
    NROS_RMW_RET_ERROR, NROS_RMW_RET_INVALID_ARGUMENT, NROS_RMW_RET_OK, NrosRmwVtable,
    backend_registered, nros_rmw_cffi_lookup, nros_rmw_cffi_register_named,
    nros_rmw_cffi_registered_names,
};

// One addressable vtable shared across all tests. The registry never
// invokes any fn pointer; we only need pointer equality + non-null.
// `Sync` is asserted via the marker — we never write to the vtable
// after construction.
struct StaticVtable(MaybeUninit<NrosRmwVtable>);
unsafe impl Sync for StaticVtable {}

static VTABLE_INIT: portable_atomic::AtomicBool = portable_atomic::AtomicBool::new(false);
static mut VTABLE_BUF: StaticVtable = StaticVtable(MaybeUninit::uninit());

fn dummy_vtable() -> &'static NrosRmwVtable {
    // Idempotent init: first caller writes the vtable, all callers
    // observe via atomic acquire on the flag.
    if !VTABLE_INIT.load(Ordering::Acquire) {
        // Single-threaded init under cargo-test default; benign
        // double-write if multiple threads race here (same value).
        unsafe extern "C" fn noop() {}
        let p = noop as *const () as *const ();
        let n = core::mem::size_of::<NrosRmwVtable>() / core::mem::size_of::<*const ()>();
        unsafe {
            let raw = (&raw mut VTABLE_BUF.0) as *mut *const ();
            for i in 0..n {
                raw.add(i).write(p);
            }
        }
        VTABLE_INIT.store(true, Ordering::Release);
    }
    unsafe { &*(&raw const VTABLE_BUF.0).cast::<NrosRmwVtable>() }
}

fn fresh_registry_is_empty() {
    assert!(!backend_registered());
    assert!(unsafe { nros_rmw_cffi_lookup(c"zenoh".as_ptr()) }.is_null());
    assert_eq!(
        unsafe { nros_rmw_cffi_registered_names(core::ptr::null_mut(), 0) },
        0
    );
}

fn register_two_named_backends() {
    let v = dummy_vtable();
    let count_before = unsafe { nros_rmw_cffi_registered_names(core::ptr::null_mut(), 0) };

    let r1 = unsafe { nros_rmw_cffi_register_named(c"zenoh".as_ptr(), v) };
    let r2 = unsafe { nros_rmw_cffi_register_named(c"xrce".as_ptr(), v) };
    assert_eq!(r1, NROS_RMW_RET_OK);
    assert_eq!(r2, NROS_RMW_RET_OK);

    assert!(backend_registered());
    assert!(!unsafe { nros_rmw_cffi_lookup(c"zenoh".as_ptr()) }.is_null());
    assert!(!unsafe { nros_rmw_cffi_lookup(c"xrce".as_ptr()) }.is_null());
    assert!(unsafe { nros_rmw_cffi_lookup(c"not-a-backend".as_ptr()) }.is_null());

    let mut buf: [*const c_char; 8] = [core::ptr::null(); 8];
    let count = unsafe { nros_rmw_cffi_registered_names(buf.as_mut_ptr(), buf.len()) };
    assert_eq!(count, count_before + 2);
}

fn duplicate_register_overwrites_idempotently() {
    let v = dummy_vtable();
    let count_before = unsafe { nros_rmw_cffi_registered_names(core::ptr::null_mut(), 0) };

    // First register.
    assert_eq!(
        unsafe { nros_rmw_cffi_register_named(c"dds".as_ptr(), v) },
        NROS_RMW_RET_OK
    );
    let p1 = unsafe { nros_rmw_cffi_lookup(c"dds".as_ptr()) };
    assert!(!p1.is_null());

    // Re-register same name with same vtable — idempotent, no new slot.
    assert_eq!(
        unsafe { nros_rmw_cffi_register_named(c"dds".as_ptr(), v) },
        NROS_RMW_RET_OK
    );
    let p2 = unsafe { nros_rmw_cffi_lookup(c"dds".as_ptr()) };
    assert_eq!(p1, p2);

    // The "default" name from legacy `nros_rmw_cffi_register` adds
    // another slot. Both coexist. The deprecation attribute is
    // intentional — this test exercises the back-compat shim.
    #[allow(deprecated)]
    let rc = unsafe { nros_rmw_cffi_register(v) };
    assert_eq!(rc, NROS_RMW_RET_OK);
    assert!(!unsafe { nros_rmw_cffi_lookup(c"default".as_ptr()) }.is_null());

    let count = unsafe { nros_rmw_cffi_registered_names(core::ptr::null_mut(), 0) };
    assert_eq!(count, count_before + 2, "dds + default");
}

fn null_name_rejected() {
    let v = dummy_vtable();
    assert_eq!(
        unsafe { nros_rmw_cffi_register_named(core::ptr::null(), v) },
        NROS_RMW_RET_INVALID_ARGUMENT
    );
    assert!(unsafe { nros_rmw_cffi_lookup(core::ptr::null()) }.is_null());
}

fn null_vtable_rejected() {
    assert_eq!(
        unsafe { nros_rmw_cffi_register_named(c"x".as_ptr(), core::ptr::null()) },
        NROS_RMW_RET_INVALID_ARGUMENT
    );
}

fn empty_name_rejected() {
    let v = dummy_vtable();
    assert_eq!(
        unsafe { nros_rmw_cffi_register_named(c"".as_ptr(), v) },
        NROS_RMW_RET_INVALID_ARGUMENT
    );
}

fn capacity_full_returns_error() {
    let v = dummy_vtable();
    // Default MAX_BACKENDS = 8. Fill remaining registry slots, then
    // over-register to hit the cap.
    let count_before = unsafe { nros_rmw_cffi_registered_names(core::ptr::null_mut(), 0) } as usize;
    let names: [&core::ffi::CStr; 8] = [c"a", c"b", c"c", c"d", c"e", c"f", c"g", c"h"];
    for n in names.iter().take(8 - count_before) {
        assert_eq!(
            unsafe { nros_rmw_cffi_register_named(n.as_ptr(), v) },
            NROS_RMW_RET_OK,
            "register {} should succeed",
            n.to_str().unwrap()
        );
    }

    // 9th distinct name overflows.
    let r = unsafe { nros_rmw_cffi_register_named(c"overflow".as_ptr(), v) };
    assert_eq!(r, NROS_RMW_RET_ERROR);
    assert!(unsafe { nros_rmw_cffi_lookup(c"overflow".as_ptr()) }.is_null());
}

#[test]
fn registry_behaviour_is_consistent() {
    fresh_registry_is_empty();
    null_name_rejected();
    null_vtable_rejected();
    empty_name_rejected();
    register_two_named_backends();
    duplicate_register_overwrites_idempotently();
    capacity_full_returns_error();
}

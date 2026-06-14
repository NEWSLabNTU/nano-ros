//! Phase 128.A — linker-section walker behaviour tests.
//!
//! Integration test (own binary) so the registry `static` and the
//! walker's `WALKED` flag start fresh.
//!
//! Requires `linkme-register` (default-on): the test registers via
//! `#[distributed_slice(RMW_INIT_ENTRIES)]`, which only exists when the
//! `linkme_backed` module is compiled. Under `--no-default-features` that
//! module is the empty stub and `RMW_INIT_ENTRIES` is a plain static, so the
//! test cannot compile — gate the whole binary on the feature (phase-248 gated
//! the slice but not this test).
#![cfg(feature = "linkme-register")]
//!
//! The test installs two `RmwInitEntry` function pointers into
//! `.nros_rmw_init` via the same `#[link_section]` mechanism backends
//! use. The walker should discover both, call each once, and the
//! second walker invocation should report zero entries.
//!
//! Selection policy (NROS_RMW env / single / ambiguous / unknown / no
//! backend) is covered too — the test installs zero, one, or two
//! backends and asserts the matching [`BackendResolution`] variant.

use core::{
    ffi::c_char,
    mem::MaybeUninit,
    sync::atomic::{AtomicU32, Ordering},
};

use linkme::distributed_slice;
use nros_rmw_cffi::{
    BackendResolution, NROS_RMW_RET_AMBIGUOUS_BACKEND, NROS_RMW_RET_NO_BACKEND, NROS_RMW_RET_OK,
    NROS_RMW_RET_UNKNOWN_BACKEND, NrosRmwVtable, RMW_INIT_ENTRIES, RmwInitEntry,
    backend_registered, backend_resolution_to_ret, nros_rmw_cffi_register_named,
    nros_rmw_cffi_registered_names, nros_rmw_cffi_walk_init_section, resolve_backend,
};

// One addressable vtable shared by every entry. The registry never
// invokes any fn pointer; we only need pointer equality + non-null.
struct StaticVtable(MaybeUninit<NrosRmwVtable>);
unsafe impl Sync for StaticVtable {}

static VTABLE_INIT: portable_atomic::AtomicBool = portable_atomic::AtomicBool::new(false);
static mut VTABLE_BUF: StaticVtable = StaticVtable(MaybeUninit::uninit());

fn dummy_vtable() -> &'static NrosRmwVtable {
    if !VTABLE_INIT.load(Ordering::Acquire) {
        unsafe extern "C" fn noop() {}
        let p = noop as *const ();
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

// Each entry increments its own counter so the test can assert which
// entries were invoked. Counters are read after the walk.
static ENTRY_A_HITS: AtomicU32 = AtomicU32::new(0);
static ENTRY_B_HITS: AtomicU32 = AtomicU32::new(0);

unsafe extern "C" fn entry_a() {
    ENTRY_A_HITS.fetch_add(1, Ordering::Relaxed);
    let rc = unsafe { nros_rmw_cffi_register_named(c"section_test_a".as_ptr(), dummy_vtable()) };
    assert_eq!(rc, NROS_RMW_RET_OK);
}

unsafe extern "C" fn entry_b() {
    ENTRY_B_HITS.fetch_add(1, Ordering::Relaxed);
    let rc = unsafe { nros_rmw_cffi_register_named(c"section_test_b".as_ptr(), dummy_vtable()) };
    assert_eq!(rc, NROS_RMW_RET_OK);
}

// Two entries land in the distributed slice the same way backend
// crates contribute theirs.
#[distributed_slice(RMW_INIT_ENTRIES)]
static SECTION_ENTRY_A: RmwInitEntry = entry_a;
#[distributed_slice(RMW_INIT_ENTRIES)]
static SECTION_ENTRY_B: RmwInitEntry = entry_b;

fn assert_no_backend_initially() {
    // Walker has NOT been called yet — registry should be empty.
    assert!(!backend_registered());
    assert!(matches!(
        resolve_backend(None),
        BackendResolution::NoBackend
    ));
    assert_eq!(
        backend_resolution_to_ret(&resolve_backend(None)),
        NROS_RMW_RET_NO_BACKEND
    );
}

fn first_walk_invokes_every_entry_once() {
    let n = unsafe { nros_rmw_cffi_walk_init_section() };
    // Exactly two entries linked from this TU. If other test entries
    // accidentally land in the same section (e.g., the c-stub-test
    // feature), the count would diverge — those features are off in
    // the default test build.
    assert_eq!(n, 2, "walker should discover both .nros_rmw_init entries");
    assert_eq!(ENTRY_A_HITS.load(Ordering::Relaxed), 1);
    assert_eq!(ENTRY_B_HITS.load(Ordering::Relaxed), 1);
    assert!(backend_registered());
    assert_eq!(
        unsafe { nros_rmw_cffi_registered_names(core::ptr::null_mut(), 0) },
        2
    );
}

fn second_walk_is_idempotent() {
    let n = unsafe { nros_rmw_cffi_walk_init_section() };
    assert_eq!(n, 0, "walker should fire only once per process");
    assert_eq!(ENTRY_A_HITS.load(Ordering::Relaxed), 1);
    assert_eq!(ENTRY_B_HITS.load(Ordering::Relaxed), 1);
}

fn resolve_with_two_backends() {
    // No selector → ambiguous.
    assert!(matches!(
        resolve_backend(None),
        BackendResolution::Ambiguous
    ));
    assert_eq!(
        backend_resolution_to_ret(&resolve_backend(None)),
        NROS_RMW_RET_AMBIGUOUS_BACKEND
    );

    // Known selector → Single.
    assert!(matches!(
        resolve_backend(Some(b"section_test_a")),
        BackendResolution::Single(_)
    ));
    assert!(matches!(
        resolve_backend(Some(b"section_test_b")),
        BackendResolution::Single(_)
    ));

    // Unknown selector.
    assert!(matches!(
        resolve_backend(Some(b"section_test_nope")),
        BackendResolution::Unknown
    ));
    assert_eq!(
        backend_resolution_to_ret(&resolve_backend(Some(b"section_test_nope"))),
        NROS_RMW_RET_UNKNOWN_BACKEND
    );
}

fn registered_names_listed() {
    let mut buf: [*const c_char; 4] = [core::ptr::null(); 4];
    let n = unsafe { nros_rmw_cffi_registered_names(buf.as_mut_ptr(), buf.len()) };
    assert_eq!(n, 2);
    let names: alloc::vec::Vec<&str> = buf[..n]
        .iter()
        .map(|p| {
            assert!(!p.is_null());
            unsafe { core::ffi::CStr::from_ptr(*p) }.to_str().unwrap()
        })
        .collect();
    assert!(names.contains(&"section_test_a"));
    assert!(names.contains(&"section_test_b"));
}

extern crate alloc;

#[test]
fn section_walker_full_lifecycle() {
    // Order matters — the registry static is process-wide, and the
    // walker's WALKED flag is one-shot in production. We rely on a
    // single test body so the sequence runs deterministically.
    assert_no_backend_initially();
    first_walk_invokes_every_entry_once();
    second_walk_is_idempotent();
    resolve_with_two_backends();
    registered_names_listed();
}

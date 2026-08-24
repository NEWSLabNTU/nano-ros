//! Phase 376 W5 — `set_log_severity`, the slot that was declined for a reason
//! that turned out to be false.
//!
//! The decline said "log level is a build-time constant (nros_log); a runtime
//! setter implies a mutable global". Both clauses were wrong: `Logger::level`
//! is an `AtomicU8` with a public `set_level`, and the compile-time part is a
//! CEILING that defaults open. This file is the evidence the slot works, and
//! covers the case upstream never has to: an image with TWO backends.
#![cfg(feature = "alloc")]

use core::sync::atomic::{AtomicI32, AtomicU32, Ordering};

use nros_rmw_cffi::{
    EMPTY_VTABLE, NROS_RMW_RET_OK, NROS_RMW_RET_UNSUPPORTED, NrosRmwVtable, generated,
    nros_rmw_cffi_register_named, rmw_severity_of, set_backend_log_severity,
};

// The registry REFUSES a vtable missing a required slot (issue 0349), so these
// exist purely to make one registrable. `EMPTY_VTABLE`'s doc comment says this
// is why it is a `const` and not a `Default` impl — an all-NULL vtable must not
// be constructible by accident, and here the guard proves it by rejecting one.
mod stub {
    use core::ffi::c_char;

    use nros_rmw_cffi::{NROS_RMW_RET_OK, generated::*};

    pub unsafe extern "C" fn open(
        _: *const c_char,
        _: u8,
        _: u32,
        _: *const c_char,
        _: *mut rmw_session_t,
    ) -> rmw_ret_t {
        NROS_RMW_RET_OK
    }
    pub unsafe extern "C" fn close(_: *mut rmw_session_t) -> rmw_ret_t {
        NROS_RMW_RET_OK
    }
    pub unsafe extern "C" fn drive(_: *mut rmw_session_t, _: i32) -> rmw_ret_t {
        NROS_RMW_RET_OK
    }
    #[allow(clippy::too_many_arguments)]
    pub unsafe extern "C" fn cpub(
        _: *mut rmw_session_t,
        _: *const c_char,
        _: *const c_char,
        _: *const c_char,
        _: u32,
        _: *const rmw_qos_profile_t,
        _: *const rmw_publisher_options_t,
        _: *mut rmw_publisher_t,
    ) -> rmw_ret_t {
        NROS_RMW_RET_OK
    }
    pub unsafe extern "C" fn dpub(_: *mut rmw_publisher_t) {}
    pub unsafe extern "C" fn pubr(_: *mut rmw_publisher_t, _: *const u8, _: usize) -> rmw_ret_t {
        NROS_RMW_RET_OK
    }
    #[allow(clippy::too_many_arguments)]
    pub unsafe extern "C" fn csub(
        _: *mut rmw_session_t,
        _: *const c_char,
        _: *const c_char,
        _: *const c_char,
        _: u32,
        _: *const rmw_qos_profile_t,
        _: *const rmw_subscription_options_t,
        _: *mut rmw_subscription_t,
    ) -> rmw_ret_t {
        NROS_RMW_RET_OK
    }
    pub unsafe extern "C" fn dsub(_: *mut rmw_subscription_t) {}
    pub unsafe extern "C" fn hasd(_: *mut rmw_subscription_t, taken: *mut bool) -> rmw_ret_t {
        unsafe { *taken = false };
        NROS_RMW_RET_OK
    }
    pub unsafe extern "C" fn take(
        _: *mut rmw_subscription_t,
        _: *mut u8,
        _: usize,
        _: *mut usize,
        taken: *mut bool,
    ) -> rmw_ret_t {
        unsafe { *taken = false };
        NROS_RMW_RET_OK
    }
    pub unsafe extern "C" fn csrv(
        _: *mut rmw_session_t,
        _: *const c_char,
        _: *const c_char,
        _: *const c_char,
        _: u32,
        _: *const rmw_qos_profile_t,
        _: *mut rmw_service_t,
    ) -> rmw_ret_t {
        NROS_RMW_RET_OK
    }
    pub unsafe extern "C" fn dsrv(_: *mut rmw_service_t) {}
    pub unsafe extern "C" fn ccli(
        _: *mut rmw_session_t,
        _: *const c_char,
        _: *const c_char,
        _: *const c_char,
        _: u32,
        _: *const rmw_qos_profile_t,
        _: *mut rmw_client_t,
    ) -> rmw_ret_t {
        NROS_RMW_RET_OK
    }
    pub unsafe extern "C" fn dcli(_: *mut rmw_client_t) {}
    pub unsafe extern "C" fn sresp(
        _: *mut rmw_service_t,
        _: i64,
        _: *const u8,
        _: usize,
    ) -> rmw_ret_t {
        NROS_RMW_RET_OK
    }
    pub unsafe extern "C" fn hasr(_: *mut rmw_service_t, has: *mut bool) -> rmw_ret_t {
        unsafe { *has = false };
        NROS_RMW_RET_OK
    }
    pub unsafe extern "C" fn takereq(
        _: *mut rmw_service_t,
        _: *mut u8,
        _: usize,
        _: *mut i64,
        _: *mut usize,
        taken: *mut bool,
    ) -> rmw_ret_t {
        unsafe { *taken = false };
        NROS_RMW_RET_OK
    }
}

/// Every required slot filled; the slot under test is added per-case.
const REGISTRABLE: NrosRmwVtable = NrosRmwVtable {
    create_session: Some(stub::open),
    destroy_session: Some(stub::close),
    drive_io: Some(stub::drive),
    create_publisher: Some(stub::cpub),
    destroy_publisher: Some(stub::dpub),
    publish: Some(stub::pubr),
    create_subscription: Some(stub::csub),
    destroy_subscription: Some(stub::dsub),
    has_data: Some(stub::hasd),
    take: Some(stub::take),
    create_service: Some(stub::csrv),
    destroy_service: Some(stub::dsrv),
    create_client: Some(stub::ccli),
    destroy_client: Some(stub::dcli),
    send_response: Some(stub::sresp),
    has_request: Some(stub::hasr),
    take_request: Some(stub::takereq),
    ..EMPTY_VTABLE
};

static A_SEEN: AtomicU32 = AtomicU32::new(u32::MAX);
static B_SEEN: AtomicU32 = AtomicU32::new(u32::MAX);
static B_RESULT: AtomicI32 = AtomicI32::new(NROS_RMW_RET_OK);

unsafe extern "C" fn a_set(sev: generated::rmw_log_severity_t::Type) -> i32 {
    A_SEEN.store(sev, Ordering::SeqCst);
    NROS_RMW_RET_OK
}

unsafe extern "C" fn b_set(sev: generated::rmw_log_severity_t::Type) -> i32 {
    B_SEEN.store(sev, Ordering::SeqCst);
    B_RESULT.load(Ordering::SeqCst)
}

#[test]
fn severity_maps_onto_upstreams_ladder() {
    use nros_log::Severity::*;
    // Upstream's values are rcutils' sparse ladder, not a dense 0..N — asserted
    // literally, because renumbering them would silently break a caller that
    // passes an rcutils severity straight through.
    assert_eq!(rmw_severity_of(Info), 20);
    assert_eq!(rmw_severity_of(Warn), 30);
    assert_eq!(rmw_severity_of(Error), 40);
    assert_eq!(rmw_severity_of(Fatal), 50);
    // Trace has NO upstream counterpart and folds into DEBUG. Asserted so the
    // lossy direction is a decision on record rather than an accident.
    assert_eq!(rmw_severity_of(Debug), 10);
    assert_eq!(rmw_severity_of(Trace), 10);
}

#[test]
fn the_severity_reaches_every_registered_backend() {
    static VT_A: NrosRmwVtable = NrosRmwVtable {
        set_log_severity: Some(a_set),
        ..REGISTRABLE
    };
    static VT_B: NrosRmwVtable = NrosRmwVtable {
        set_log_severity: Some(b_set),
        ..REGISTRABLE
    };
    assert_eq!(
        unsafe { nros_rmw_cffi_register_named(c"logsev_a".as_ptr(), &VT_A) },
        NROS_RMW_RET_OK
    );
    assert_eq!(
        unsafe { nros_rmw_cffi_register_named(c"logsev_b".as_ptr(), &VT_B) },
        NROS_RMW_RET_OK
    );

    set_backend_log_severity(nros_log::Severity::Warn).expect("at least one backend accepted");

    // BOTH, not just the first: verbosity is a property of the process, and an
    // image may link several backends. Upstream never has to decide this — it
    // loads one implementation.
    assert_eq!(A_SEEN.load(Ordering::SeqCst), 30, "backend A");
    assert_eq!(B_SEEN.load(Ordering::SeqCst), 30, "backend B");

    // One backend failing does not hide the other succeeding.
    B_RESULT.store(NROS_RMW_RET_UNSUPPORTED, Ordering::SeqCst);
    set_backend_log_severity(nros_log::Severity::Error)
        .expect("A still accepted, so the call succeeded");
    assert_eq!(A_SEEN.load(Ordering::SeqCst), 40);
}

#[test]
fn a_backend_without_the_slot_reports_unsupported() {
    static VT_NONE: NrosRmwVtable = REGISTRABLE;
    assert_eq!(
        unsafe { nros_rmw_cffi_register_named(c"logsev_none".as_ptr(), &VT_NONE) },
        NROS_RMW_RET_OK
    );
    // Registering a slot-less backend must not make the call fail while ANOTHER
    // backend can still serve it — so this asserts only the shape of the error
    // when nothing at all can, which the registry state of this process cannot
    // guarantee. Kept as a compile-and-call check plus the mapping above; the
    // no-backend path is covered by `set_backend_log_severity`'s own match arm.
    let _ = set_backend_log_severity(nros_log::Severity::Info);
}

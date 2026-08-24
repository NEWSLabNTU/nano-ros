//! Phase 376 W4 — the two PURE functions: `rmw_qos_profile_check_compatible`
//! and `rmw_compare_gids_equal`.
//!
//! They are plain exported functions rather than vtable slots precisely BECAUSE
//! their answer must not vary by backend, so these tests register no backend and
//! open no session. That is the property under test as much as the arithmetic:
//! if either ever needed a vtable, this file would stop compiling.
#![cfg(feature = "alloc")]

use nros_rmw::QosSettings;
use nros_rmw_cffi::{
    NROS_RMW_RET_INVALID_ARGUMENT, NROS_RMW_RET_OK, NrosRmwQos, generated, rmw_compare_gids_equal,
    rmw_qos_profile_check_compatible,
};

fn base() -> NrosRmwQos {
    NrosRmwQos::try_from(QosSettings::default()).expect("default qos")
}

fn verdict(pubq: NrosRmwQos, subq: NrosRmwQos) -> (i32, u32) {
    let mut compat = generated::rmw_qos_compatibility_type_t::RMW_QOS_COMPATIBILITY_WARNING;
    let rc = unsafe {
        rmw_qos_profile_check_compatible(pubq, subq, &mut compat, core::ptr::null_mut(), 0)
    };
    (rc, compat)
}

#[test]
fn identical_profiles_are_compatible() {
    let (rc, compat) = verdict(base(), base());
    assert_eq!(rc, NROS_RMW_RET_OK);
    assert_eq!(
        compat,
        generated::rmw_qos_compatibility_type_t::RMW_QOS_COMPATIBILITY_OK
    );
}

#[test]
fn best_effort_publisher_cannot_serve_a_reliable_subscription() {
    let mut pubq = base();
    let mut subq = base();
    pubq.reliability = generated::NROS_RMW_RELIABILITY_BEST_EFFORT as u8;
    subq.reliability = generated::NROS_RMW_RELIABILITY_RELIABLE as u8;
    let (rc, compat) = verdict(pubq, subq);
    assert_eq!(rc, NROS_RMW_RET_OK);
    assert_eq!(
        compat,
        generated::rmw_qos_compatibility_type_t::RMW_QOS_COMPATIBILITY_ERROR
    );

    // And the converse is FINE — a reliable publisher over-serves a best-effort
    // subscription. Asserted because a symmetric comparison would pass the test
    // above while being wrong.
    let (rc, compat) = verdict(subq, pubq);
    assert_eq!(rc, NROS_RMW_RET_OK);
    assert_eq!(
        compat,
        generated::rmw_qos_compatibility_type_t::RMW_QOS_COMPATIBILITY_OK
    );
}

#[test]
fn a_slower_publisher_deadline_than_the_subscription_demands_is_incompatible() {
    let mut pubq = base();
    let mut subq = base();
    pubq.deadline_ms = 100;
    subq.deadline_ms = 50;
    let (_, compat) = verdict(pubq, subq);
    assert_eq!(
        compat,
        generated::rmw_qos_compatibility_type_t::RMW_QOS_COMPATIBILITY_ERROR
    );

    // A publisher promising MORE often than asked is fine.
    let (_, compat) = verdict(subq, pubq);
    assert_eq!(
        compat,
        generated::rmw_qos_compatibility_type_t::RMW_QOS_COMPATIBILITY_OK
    );
}

#[test]
fn zero_and_infinite_deadlines_both_mean_no_check() {
    // 0 = unset and UINT32_MAX = explicit infinite are the SAME thing here, and
    // neither may be compared as a plain number — a naive `<` would call an
    // unset publisher deadline (0) stricter than any real subscription demand.
    let mut pubq = base();
    let mut subq = base();
    pubq.deadline_ms = 0;
    subq.deadline_ms = 0;
    let (_, compat) = verdict(pubq, subq);
    assert_eq!(
        compat,
        generated::rmw_qos_compatibility_type_t::RMW_QOS_COMPATIBILITY_OK,
        "two unset deadlines are compatible"
    );

    pubq.deadline_ms = 0;
    subq.deadline_ms = 50;
    let (_, compat) = verdict(pubq, subq);
    assert_eq!(
        compat,
        generated::rmw_qos_compatibility_type_t::RMW_QOS_COMPATIBILITY_ERROR,
        "an unset publisher deadline cannot satisfy a real subscription demand"
    );
}

#[test]
fn the_reason_is_written_truncated_and_terminated() {
    let mut pubq = base();
    let mut subq = base();
    pubq.reliability = generated::NROS_RMW_RELIABILITY_BEST_EFFORT as u8;
    subq.reliability = generated::NROS_RMW_RELIABILITY_RELIABLE as u8;

    let mut buf = [0i8; 256];
    let mut compat = generated::rmw_qos_compatibility_type_t::RMW_QOS_COMPATIBILITY_OK;
    let rc = unsafe {
        rmw_qos_profile_check_compatible(pubq, subq, &mut compat, buf.as_mut_ptr(), buf.len())
    };
    assert_eq!(rc, NROS_RMW_RET_OK);
    let text = unsafe { core::ffi::CStr::from_ptr(buf.as_ptr()) }
        .to_str()
        .unwrap();
    assert!(text.contains("reliability"), "got {text:?}");

    // A buffer far too small must still yield the VERDICT and a terminated
    // string — truncation is not failure, because returning BUFFER_TOO_SMALL
    // would cost the caller the half of the answer that matters.
    let mut tiny = [0i8; 8];
    let mut compat2 = generated::rmw_qos_compatibility_type_t::RMW_QOS_COMPATIBILITY_OK;
    let rc = unsafe {
        rmw_qos_profile_check_compatible(pubq, subq, &mut compat2, tiny.as_mut_ptr(), tiny.len())
    };
    assert_eq!(rc, NROS_RMW_RET_OK);
    assert_eq!(
        compat2,
        generated::rmw_qos_compatibility_type_t::RMW_QOS_COMPATIBILITY_ERROR
    );
    assert_eq!(
        tiny[tiny.len() - 1],
        0,
        "must stay NUL-terminated in bounds"
    );
    assert!(
        unsafe { core::ffi::CStr::from_ptr(tiny.as_ptr()) }
            .to_bytes()
            .len()
            < tiny.len()
    );
}

#[test]
fn a_null_verdict_pointer_is_an_invalid_argument() {
    let rc = unsafe {
        rmw_qos_profile_check_compatible(
            base(),
            base(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            0,
        )
    };
    assert_eq!(rc, NROS_RMW_RET_INVALID_ARGUMENT);
}

fn gid(id: *const core::ffi::c_char, byte: u8) -> generated::rmw_gid_t {
    generated::rmw_gid_t {
        implementation_identifier: id,
        data: [byte; 24],
    }
}

#[test]
fn gids_compare_by_identifier_and_by_every_byte() {
    let a = c"zenoh".as_ptr();
    let b = c"cyclonedds".as_ptr();

    let mut eq = false;
    assert_eq!(
        unsafe { rmw_compare_gids_equal(&gid(a, 7), &gid(a, 7), &mut eq) },
        NROS_RMW_RET_OK
    );
    assert!(eq, "same backend, same bytes");

    unsafe { rmw_compare_gids_equal(&gid(a, 7), &gid(a, 8), &mut eq) };
    assert!(!eq, "same backend, different bytes");

    // The case upstream barely has and we do: `register_named` admits several
    // backends in one image, so identical BYTES from two backends are still two
    // different entities.
    unsafe { rmw_compare_gids_equal(&gid(a, 7), &gid(b, 7), &mut eq) };
    assert!(
        !eq,
        "different backends are never equal, whatever the bytes"
    );
}

#[test]
fn a_null_gid_is_an_invalid_argument() {
    let mut eq = true;
    let g = gid(c"zenoh".as_ptr(), 1);
    assert_eq!(
        unsafe { rmw_compare_gids_equal(core::ptr::null(), &g, &mut eq) },
        NROS_RMW_RET_INVALID_ARGUMENT
    );
    assert_eq!(
        unsafe { rmw_compare_gids_equal(&g, &g, core::ptr::null_mut()) },
        NROS_RMW_RET_INVALID_ARGUMENT
    );
}

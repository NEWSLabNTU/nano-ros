//! Phase 376 W4 — the two PURE functions: `rmw_qos_profile_check_compatible`
//! and `rmw_compare_gids_equal`.
//!
//! They are plain exported functions rather than vtable slots precisely BECAUSE
//! their answer must not vary by backend, so these tests register no backend and
//! open no session. That is the property under test as much as the arithmetic:
//! if either ever needed a vtable, this file would stop compiling.
#![cfg(feature = "alloc")]

use nros_rmw::QoSProfile;
use nros_rmw_cffi::{
    NROS_RMW_RET_INVALID_ARGUMENT, NROS_RMW_RET_OK, NrosRmwQos, generated, rmw_compare_gids_equal,
    rmw_qos_profile_check_compatible,
};

fn base() -> NrosRmwQos {
    NrosRmwQos::try_from(QoSProfile::default()).expect("default qos")
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

    let mut buf = [0 as core::ffi::c_char; 256];
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
    let mut tiny = [0 as core::ffi::c_char; 8];
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

// ============================================================================
// Phase 376 W5/B2 — the policy VALUES are upstream's
// ============================================================================

/// These numbers cross the ABI: `rmw_qos_profile_check_compatible` is a name
/// upstream owns and we export, and cyclonedds turns this struct into real DDS
/// QoS that a ROS peer matches against.
///
/// They disagreed with upstream until 2026-08-24, and the disagreement was
/// invisible: ours were a dense 0/1 pair per policy, so `history == 1` meant
/// KEEP_LAST to upstream and KEEP_ALL to us — opposite answers to one question
/// — and liveliness had MANUAL_BY_NODE and MANUAL_BY_TOPIC swapped. Nothing
/// failed to compile then and nothing would fail to compile if they drifted
/// again, which is what this test is for.
///
/// Values transcribed from Humble's `rmw/types.h`.
#[test]
fn policy_values_match_upstreams_numbering() {
    use nros_rmw_cffi::generated::{self, rmw_liveliness_kind_t as lk};

    // RMW_QOS_POLICY_RELIABILITY_{SYSTEM_DEFAULT,RELIABLE,BEST_EFFORT,UNKNOWN}
    assert_eq!(generated::NROS_RMW_RELIABILITY_SYSTEM_DEFAULT, 0);
    assert_eq!(generated::NROS_RMW_RELIABILITY_RELIABLE, 1);
    assert_eq!(generated::NROS_RMW_RELIABILITY_BEST_EFFORT, 2);
    assert_eq!(generated::NROS_RMW_RELIABILITY_UNKNOWN, 3);

    // RMW_QOS_POLICY_HISTORY_{SYSTEM_DEFAULT,KEEP_LAST,KEEP_ALL,UNKNOWN}
    assert_eq!(generated::NROS_RMW_HISTORY_SYSTEM_DEFAULT, 0);
    assert_eq!(generated::NROS_RMW_HISTORY_KEEP_LAST, 1);
    assert_eq!(generated::NROS_RMW_HISTORY_KEEP_ALL, 2);
    assert_eq!(generated::NROS_RMW_HISTORY_UNKNOWN, 3);

    // RMW_QOS_POLICY_DURABILITY_{SYSTEM_DEFAULT,TRANSIENT_LOCAL,VOLATILE,UNKNOWN}
    assert_eq!(generated::NROS_RMW_DURABILITY_SYSTEM_DEFAULT, 0);
    assert_eq!(generated::NROS_RMW_DURABILITY_TRANSIENT_LOCAL, 1);
    assert_eq!(generated::NROS_RMW_DURABILITY_VOLATILE, 2);
    assert_eq!(generated::NROS_RMW_DURABILITY_UNKNOWN, 3);

    // RMW_QOS_POLICY_LIVELINESS_* — 2 and 3 are the pair that was swapped.
    assert_eq!(lk::NROS_RMW_LIVELINESS_SYSTEM_DEFAULT, 0);
    assert_eq!(lk::NROS_RMW_LIVELINESS_AUTOMATIC, 1);
    assert_eq!(lk::NROS_RMW_LIVELINESS_MANUAL_BY_NODE, 2);
    assert_eq!(lk::NROS_RMW_LIVELINESS_MANUAL_BY_TOPIC, 3);
    assert_eq!(lk::NROS_RMW_LIVELINESS_UNKNOWN, 4);
}

/// The Rust enum's discriminant IS the C ABI value — `liveliness_kind` is
/// written with `as u8` — so the two cannot be checked separately.
#[test]
fn the_rust_liveliness_enum_carries_the_abi_values() {
    use nros_rmw::QoSLivelinessPolicy as P;
    use nros_rmw_cffi::generated::rmw_liveliness_kind_t as lk;

    assert_eq!(P::None as u32, lk::NROS_RMW_LIVELINESS_SYSTEM_DEFAULT);
    assert_eq!(P::Automatic as u32, lk::NROS_RMW_LIVELINESS_AUTOMATIC);
    assert_eq!(
        P::ManualByNode as u32,
        lk::NROS_RMW_LIVELINESS_MANUAL_BY_NODE
    );
    assert_eq!(
        P::ManualByTopic as u32,
        lk::NROS_RMW_LIVELINESS_MANUAL_BY_TOPIC
    );
}

/// An undetermined policy is an ABSENCE, so it warns rather than passing or
/// failing — upstream's `RMW_QOS_COMPATIBILITY_WARNING`, unreachable here until
/// B2 gave the policies an UNKNOWN to report.
#[test]
fn an_unknown_policy_warns_but_a_real_clash_still_errors() {
    use generated::rmw_qos_compatibility_type_t as C;
    use nros_rmw_cffi::{
        NROS_RMW_QOS_PROFILE_DEFAULT, generated, nros_rmw_qos_incompatibility_mask,
    };

    let mut verdict = C::RMW_QOS_COMPATIBILITY_OK;
    let mut mask = 0u32;

    // Baseline: two identical, fully-determined profiles are OK.
    let rc = unsafe {
        nros_rmw_qos_incompatibility_mask(
            NROS_RMW_QOS_PROFILE_DEFAULT,
            NROS_RMW_QOS_PROFILE_DEFAULT,
            &mut verdict,
            &mut mask,
        )
    };
    assert_eq!(rc, nros_rmw_cffi::NROS_RMW_RET_OK);
    assert_eq!(verdict, C::RMW_QOS_COMPATIBILITY_OK);
    assert_eq!(mask, 0);

    // One policy the backend could not read back: compatible so far as we can
    // tell, and the caller is told we could not tell everything.
    let mut undetermined = NROS_RMW_QOS_PROFILE_DEFAULT;
    undetermined.durability = generated::NROS_RMW_DURABILITY_UNKNOWN as u8;
    let rc = unsafe {
        nros_rmw_qos_incompatibility_mask(
            undetermined,
            NROS_RMW_QOS_PROFILE_DEFAULT,
            &mut verdict,
            &mut mask,
        )
    };
    assert_eq!(rc, nros_rmw_cffi::NROS_RMW_RET_OK);
    assert_eq!(verdict, C::RMW_QOS_COMPATIBILITY_WARNING);
    assert_eq!(mask, 0, "an unknown is not a clash");

    // A real clash outranks the unknown — softening this to a warning would
    // hide something the caller can act on.
    let mut best_effort_pub = NROS_RMW_QOS_PROFILE_DEFAULT;
    best_effort_pub.reliability = generated::NROS_RMW_RELIABILITY_BEST_EFFORT as u8;
    best_effort_pub.durability = generated::NROS_RMW_DURABILITY_UNKNOWN as u8;
    let rc = unsafe {
        nros_rmw_qos_incompatibility_mask(
            best_effort_pub,
            NROS_RMW_QOS_PROFILE_DEFAULT,
            &mut verdict,
            &mut mask,
        )
    };
    assert_eq!(rc, nros_rmw_cffi::NROS_RMW_RET_OK);
    assert_eq!(verdict, C::RMW_QOS_COMPATIBILITY_ERROR);
    assert_ne!(
        mask & generated::nros_rmw_qos_clash_t::NROS_RMW_QOS_CLASH_RELIABILITY,
        0
    );
}

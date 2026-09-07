//! phase-427 W10 — `Context::create_executor` / `create_executor_in`.
//!
//! Here rather than in `nros`'s own `--lib` tests for a link reason, not a
//! taste one: turning `rmw-cffi` on in that crate's test binary pulls
//! `nros-node`'s platform externs (`nros_platform_clock_ns`,
//! `nros_platform_wake_*`) with no port to satisfy them, so `cargo test -p
//! nros --features rmw-cffi` does not link at all. This crate has a host
//! platform in its graph and does.
//!
//! No backend is registered under `rclrs-port-test` and none is needed: what
//! is under test is the SAFE half — the backing-length refusal that stands
//! between a caller and `Executor::open_in`'s `unsafe` contract, and that an
//! open failure comes back as a value naming the backend's own error rather
//! than as a panic.

#![cfg(feature = "rclrs-port-test")]

// issues 0619 / 0612 — a dependency nothing references is not linked, and the
// POSIX C port is reached only through `#[no_mangle]` symbols `nros-node`
// declares `extern`. Without this anchor the test binary fails to link on
// `nros_platform_clock_ns` / `nros_platform_wake_*`, which reads like a
// missing system library rather than a missing dep edge.
use nros_platform_cffi as _;

use core::mem::MaybeUninit;

use nros::{Context, ExecutorSizing, InitError};

/// A context that reads nothing from the environment, so the test says the
/// same thing on every host.
fn ctx() -> Context {
    Context::from_baked(Some("tcp/127.0.0.1:7447"), Some("3")).expect("valid bake")
}

#[test]
fn the_bake_reaches_the_session_unchanged() {
    let ctx = ctx();
    assert_eq!(ctx.locator, "tcp/127.0.0.1:7447");
    assert_eq!(ctx.domain_id, 3);
    // The identity the executor opens with is this one MINUS a node name:
    // `Context::config(name)` is the old shape and still sets one, and the two
    // must not quietly converge.
    assert_eq!(ctx.config("talker").node_name, "talker");
}

#[test]
fn a_backing_one_word_short_is_refused_by_length() {
    let needed = ExecutorSizing::DEFAULT.u64_len();
    assert!(needed > 1, "a default executor needs more than one word");

    let mut backing = vec![MaybeUninit::<u64>::uninit(); needed - 1];
    // `Executor` is neither `Debug` nor `PartialEq`, so compare the error the
    // call maps to. One word short is still short — the check is `<`, not a
    // rounded-down guess.
    assert_eq!(
        ctx().create_executor_in(&mut backing).err(),
        Some(InitError::BackingTooSmall {
            needed,
            given: needed - 1
        }),
    );

    // An empty slice is the same refusal, not a different failure mode.
    assert!(matches!(
        ctx().create_executor_in(&mut []).err(),
        Some(InitError::BackingTooSmall { given: 0, .. })
    ));
}

#[test]
fn the_length_refusal_happens_before_the_rmw_is_consulted() {
    // The ORDER is the property. This build registers no backend, so anything
    // that reaches the RMW comes back `ExecutorOpenFailed` — if the length
    // check ran second, a caller with a short `static` would be told the
    // router is missing instead of that their array is too small.
    let mut backing = vec![MaybeUninit::<u64>::uninit(); 1];
    assert!(matches!(
        ctx().create_executor_in(&mut backing).err(),
        Some(InitError::BackingTooSmall { .. })
    ));
}

#[test]
fn a_correctly_sized_backing_gets_past_the_check_and_reaches_the_open() {
    // The other side of the same order: at the required length the wrapper
    // stops refusing and the RMW answers. It answers "no backend" here, which
    // is the point — the failure moved from the caller's storage to the
    // session, and the error variant says which.
    let mut backing = vec![MaybeUninit::<u64>::uninit(); ExecutorSizing::DEFAULT.u64_len()];
    match ctx().create_executor_in(&mut backing) {
        Err(InitError::ExecutorOpenFailed(_)) => {}
        Err(other) => panic!("expected the open to be what failed, got {other:?}"),
        // A build that DOES link a backend (feature unification under
        // `--all-features`) opens a session; the length check still passed,
        // which is what this test is about.
        Ok(_) => {}
    }
}

#[test]
fn an_open_failure_is_a_value_carrying_the_backend_error() {
    // `create_executor` leaks its own backing, so there is no length to get
    // wrong — the only thing it can report is the open. What matters is the
    // CHANNEL: a `Result` carrying the backend's own `NodeError`, so "no
    // backend registered" and "the router is not there" stay distinguishable
    // (issue 0465). A panic here would be the failure.
    match ctx().create_executor() {
        Err(InitError::ExecutorOpenFailed(e)) => {
            // Not flattened to a flag — the payload is the backend's.
            let rendered = format!("{e:?}");
            assert!(!rendered.is_empty());
        }
        Err(other) => panic!("expected ExecutorOpenFailed, got {other:?}"),
        Ok(_) => {}
    }
}

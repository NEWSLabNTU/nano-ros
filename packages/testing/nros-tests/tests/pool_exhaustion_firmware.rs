//! issue 0697 — the zenoh session-pool exhaustion arm, asserted on a `no_std`
//! target.
//!
//! `zpico.rs` returns `ZpicoError::Full` when the pool is exhausted and logs the
//! one message that explains it. Issue 0589 moved that log from `std::eprintln!`
//! to `nros_log` **so it would reach `no_std` targets**, on the reasoning that
//! "firmware is where a fixed-size pool actually fills". Nothing on firmware ever
//! reached it: no embedded build raised `ZPICO_MAX_SESSIONS`, and no test
//! asserted `Full` on any platform — the one reference outside the backend was a
//! doc comment explaining that a native test SKIPS when the pool is 1.
//!
//! Two assertions, and the second is the half 0589 was about:
//!
//!   1. the exhausted pool reports `Full`, not a transport error (issue 0465 —
//!      that distinction is what stopped an exhausted pool reading as
//!      `Transport(ConnectionFailed)` for two months);
//!   2. the explanation reaches the CONSOLE of a `no_std` image.
//!
//! (2) could not have passed before issue 0708, which is why this cell did not
//! exist: this board family's boot funnel never published an `nros_log` sink
//! list, so the record was dropped before any console.

use nros_tests::{
    fixtures::{ManagedProcess, build_pool_exhaustion_threadx_linux},
    output::{POOL_EXHAUSTION_VERDICT, ZENOH_SESSION_POOL_EXHAUSTED},
};
use std::{process::Command, time::Duration};

#[test]
fn zenoh_pool_exhaustion_reports_full_and_says_why_on_firmware() {
    let bin = match build_pool_exhaustion_threadx_linux() {
        Ok(p) => p,
        Err(e) => nros_tests::skip!("pool-exhaustion fixture not built: {e}"),
    };

    let mut proc = ManagedProcess::spawn_command(Command::new(bin), "pool-exhaustion")
        .expect("spawn the pool-exhaustion image");
    let out = proc.collect_until(POOL_EXHAUSTION_VERDICT, Duration::from_secs(30));
    proc.kill();

    // The image refuses to print its verdict unless it took the `Full` arm, so
    // this covers assertion (1). Asserted on the OUTPUT rather than on an exit
    // code because a `no_std` image's status is all a harness would otherwise
    // get, and "exit 1" cannot say which of the three wrong outcomes happened.
    assert!(
        out.contains(POOL_EXHAUSTION_VERDICT),
        "the image did not report `Full` for a second session on a pool of 1 \
         (issue 0697). Full output:\n{out}"
    );

    // (2) — the half issue 0589 hardened and nothing had executed. This is NOT
    // redundant with the line above: the verdict is printed by the FIXTURE, the
    // marker by the BACKEND, and 0589's change was about the backend's record
    // surviving on a target where `std` stdio is fatal.
    assert!(
        out.contains(ZENOH_SESSION_POOL_EXHAUSTED),
        "the pool-exhaustion diagnostic never reached the console of a no_std \
         image — the record is being dropped before any sink (issues 0589, 0708). \
         Full output:\n{out}"
    );
}

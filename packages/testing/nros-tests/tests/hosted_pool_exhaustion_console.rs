//! Issue 2492 — a hosted image that exhausts a zenoh pool says so on its
//! console BEFORE it is killed.
//!
//! When the shim refuses a liveliness token, the entity still works but is
//! invisible to ROS 2 tooling, so the console is the only place the cause
//! shows. Two diagnostics are written for it, and on a hosted image both were
//! lost:
//!
//!   * the C shim's `printk` was `printf` to stdout, which is block-buffered
//!     when stdout is a pipe or a file. A harness stops an image with a signal,
//!     so the line sat in the buffer and died with the process. Under SIGTERM it
//!     appeared only because a clean exit flushes stdio;
//!   * `nros-rmw-zenoh`'s `log_error!` went to `nros_log`, which HOLDS a
//!     record until a sink is installed. A plain-API `main` (this fixture's:
//!     `nros::init()` then `Executor::open`) reached no call that installs
//!     one, so the record never left the early ring, under any signal.
//!
//! Issue 2482 went unexplained for exactly this reason. The fixture is the
//! two-node parameter image with a liveliness pool of 4, far short of its ~18
//! tokens. Both halves are asserted on output read from the LIVE pipe, and the
//! image is killed only after: a buffered line is invisible to that read, so
//! the wait fails while the process is still running rather than passing on
//! what an exit flushed.

use nros_tests::{
    fixtures::{
        ManagedProcess, RequireFixture, ZenohRouter,
        build_native_param_two_node_talker_short_liveliness, require_zenohd, zenohd_unique,
    },
    output::{ZENOH_LIVELINESS_DECLARE_FULL, ZPICO_LIVELINESS_POOL_EXHAUSTED},
};
use rstest::rstest;
use std::{process::Command, time::Duration};

#[rstest]
fn an_exhausted_liveliness_pool_is_on_the_console_before_the_image_is_killed(
    zenohd_unique: ZenohRouter,
) {
    require_zenohd();
    let binary = build_native_param_two_node_talker_short_liveliness()
        .require("param-two-node-talker-short-liveliness");

    let mut cmd = Command::new(binary);
    // No `RUST_LOG`: the record must reach the console at the default level.
    cmd.env("NROS_LOCATOR", zenohd_unique.locator())
        .env("NROS_SESSION_MODE", "client");
    let mut proc = ManagedProcess::spawn_image(cmd, "short-liveliness")
        .expect("failed to start the short-liveliness image");

    // Both lines are written in the same declare call, the C one first, so
    // once the Rust record has arrived the C line is already on the pipe or
    // was lost to a buffer.
    let out = proc.collect_until(ZENOH_LIVELINESS_DECLARE_FULL, Duration::from_secs(20));
    proc.kill();

    assert!(
        out.contains(ZENOH_LIVELINESS_DECLARE_FULL),
        "`nros-rmw-zenoh`'s record for a refused liveliness token never reached \
         the console of a running hosted image. It is held in `nros_log`'s early \
         ring until a sink is installed, and a plain-API `main` gets its sinks \
         from `nros_board_linux::register_linked_rmw` (issue 2492). Output:\n{out}"
    );
    assert!(
        out.contains(ZPICO_LIVELINESS_POOL_EXHAUSTED),
        "the zenoh shim's `{ZPICO_LIVELINESS_POOL_EXHAUSTED}` line was not on the \
         console while the image ran: a hosted `printk` must flush, or a killed \
         image loses it (issue 2492). Output:\n{out}"
    );
}

//! phase-296 W5.13 / issue #260 — the POSIX core-pin (`placement` dim) is
//! applied AND kernel-ACCEPTED at runtime. Unlike the RTOS SMP arms
//! (zephyr/nuttx/freertos/threadx), whose realtime fixtures are all
//! uniprocessor and therefore only ever exercise the fallback note, a Linux
//! host is genuinely multi-core and `sched_setaffinity` needs no privilege — so
//! the ws-realtime-rust `high` tier's `posix.core: 0` pins for real. This is the
//! FIRST runtime accept-arm proof of the core-pin consumer.
//!
//! Boots the native (posix) rust realtime entry as a host process on an
//! ephemeral router (same shape as `realtime_tiers_e2e`'s `native_rust` cell —
//! per-process router, no baked port, so no nextest port group is needed).
//!
//! Run with: `cargo nextest run -p nros-tests --test posix_core_pin_applied`.

use nros_tests::{
    fixtures::{ZenohRouter, build_native_workspace_rust_realtime_entry},
    output::{POSIX_CORE_PIN_FALLBACK_MARKER, POSIX_CORE_PIN_MARKER},
    process::ManagedProcess,
};
use std::{process::Command, time::Duration};

#[test]
fn posix_core_pin_applied_at_runtime() {
    let entry = build_native_workspace_rust_realtime_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("native rust realtime fixture unavailable: {e}"));

    let router = ZenohRouter::start_unique()
        .unwrap_or_else(|e| nros_tests::skip!("zenohd failed to start: {e}"));

    let mut cmd = Command::new(&entry);
    cmd.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", router.locator())
        .env("NROS_SESSION_MODE", "client")
        .env("NROS_ENTRY_SPIN_MS", "8000")
        .env("NROS_ENTRY_SPIN_STEP_MS", "5");
    let mut guest = ManagedProcess::spawn_command(cmd, "posix-core-pin-entry")
        .unwrap_or_else(|e| panic!("spawn native realtime entry: {e}"));

    // The marker prints at tier setup, early in boot.
    let log = guest
        .wait_for_output_pattern("nros: core pin", Duration::from_secs(20))
        .unwrap_or_else(|e| {
            guest.kill();
            panic!(
                "the ws-realtime-rust `high` tier declares `posix.core` but the native \
                 boot produced NEITHER the accept marker (`{POSIX_CORE_PIN_MARKER}`) NOR \
                 the fallback note (`{POSIX_CORE_PIN_FALLBACK_MARKER}`) — the placement \
                 dim was silently dropped (RFC-0052 fail-loud violation). err: {e:?}"
            )
        });
    guest.kill();

    // On a multi-core host with an unrestricted cpuset, CPU 0 always exists and
    // the pin is honored — assert the ACCEPT arm (the #260 runtime proof), not
    // merely fail-loud. The accept and fallback strings are disjoint (the
    // fallback carries `FAILED ` between `pin ` and `tier=`), so a match on the
    // accept marker means a genuine accept line is present. A cpuset that
    // excludes CPU 0 is the only way this would flip to fallback.
    assert!(
        log.contains(POSIX_CORE_PIN_MARKER),
        "expected the POSIX core-pin ACCEPT marker (`{POSIX_CORE_PIN_MARKER}`) — \
         sched_setaffinity(cpu 0) should succeed on any multi-core host with an \
         unrestricted cpuset; saw fallback (`{POSIX_CORE_PIN_FALLBACK_MARKER}`)? \
         log:\n{log}"
    );
    eprintln!("posix core pin: KERNEL-ACCEPTED (sched_setaffinity)");
}

//! phase-296 #0266 — the ThreadX round-robin time slice (the time-slicing dim)
//! is honored end-to-end: the realtime-rust `low`/boot tier declares
//! `threadx.time_slice_us: 5000`, and the board prints the trace marker when the
//! slice is applied (`tx_thread_time_slice_change` on the boot thread /
//! `tx_thread_create` slice param on spawned tiers). ThreadX honors a per-thread
//! slice unconditionally, so this is accept-only (no fallback) — the marker's
//! presence IS the proof the previously-hardwired `TX_NO_TIME_SLICE` is gone.
//!
//! Boots the SAME threadx-linux host image the `threadx_linux_rust` realtime
//! cell uses (shared image + baked router port — serialized via the
//! `threadx-realtime-rust-port` nextest group). `low` is the BOOT tier on
//! ThreadX (`resolve_tiers` sorts descending by raw number), so the marker
//! comes from the boot `tx_thread_time_slice_change` path.
//!
//! Run with: `cargo nextest run -p nros-tests --test threadx_time_slice_applied`.

use nros_tests::{
    alloc::port_of,
    fixtures::{ZenohRouter, build_threadx_workspace_rust_realtime_entry},
    matrix::{Lang, PlatformId, Workload},
    output::THREADX_TIME_SLICE_MARKER,
    process::ManagedProcess,
};
use std::{process::Command, time::Duration};

#[test]
fn threadx_time_slice_applied_for_declaring_tier() {
    let entry = build_threadx_workspace_rust_realtime_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("threadx-linux realtime fixture unavailable: {e}"));

    let port = port_of(
        PlatformId::ThreadxLinux,
        Lang::Rust,
        Workload::RealtimeTiers,
    );
    let _router = ZenohRouter::start_on("127.0.0.1", port)
        .unwrap_or_else(|e| nros_tests::skip!("zenohd failed to start on {port}: {e}"));

    let mut cmd = Command::new(&entry);
    cmd.env("RUST_LOG", "info");
    let mut guest = ManagedProcess::spawn_command(cmd, "threadx-time-slice-entry")
        .unwrap_or_else(|e| panic!("spawn threadx-linux realtime entry: {e}"));

    let log = guest
        .wait_for_output_pattern(THREADX_TIME_SLICE_MARKER, Duration::from_secs(30))
        .unwrap_or_else(|e| {
            guest.kill();
            panic!(
                "declared time_slice_us produced no `{THREADX_TIME_SLICE_MARKER}` marker \
                 — the round-robin slice was silently dropped (the tier still runs with \
                 the old hardwired TX_NO_TIME_SLICE). err: {e:?}"
            )
        });
    guest.kill();

    let hits = nros_tests::count_pattern(&log, THREADX_TIME_SLICE_MARKER);
    // Exactly ONE tier (`low`) declares a time slice in this fixture.
    assert_eq!(
        hits, 1,
        "expected the time-slice marker for exactly the 1 declaring tier (`low`); \
         saw {hits} in:\n{log}"
    );
}

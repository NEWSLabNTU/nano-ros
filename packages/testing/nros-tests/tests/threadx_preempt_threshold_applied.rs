//! phase-296 W5.10 — the ThreadX preemption threshold (the RFC-0052
//! `non_preempt_scope` dim) is honored end-to-end: the ws-realtime-rust
//! `high` tier declares `threadx.preempt_threshold: 3`, and the board prints
//! the trace marker ONLY when the kernel actually accepted the threshold
//! (`tx_thread_create` at spawn / `tx_thread_preemption_change` on the boot
//! reprioritize) — the W5.5 marker discipline.
//!
//! Boots the SAME threadx-linux host image `realtime_tiers_e2e.rs`'s
//! `threadx_linux_rust` cell uses (shared image + baked router port —
//! serialized via the `threadx-realtime-rust-port` nextest group). On
//! ThreadX `resolve_tiers` sorts descending by raw number, so `high`
//! (threshold-carrying) is a SPAWNED tier — the marker comes from the
//! `tx_thread_create` path.
//!
//! Run with: `cargo nextest run -p nros-tests --test threadx_preempt_threshold_applied`.

use nros_tests::{
    alloc::port_of,
    fixtures::{ZenohRouter, build_threadx_workspace_rust_realtime_entry},
    matrix::{Lang, PlatformId, Workload},
    output::THREADX_PREEMPT_MARKER,
    process::ManagedProcess,
};
use std::{process::Command, time::Duration};

#[test]
fn threadx_preempt_threshold_applied_for_high_tier() {
    let entry = build_threadx_workspace_rust_realtime_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("threadx-linux realtime fixture unavailable: {e}"));

    // Same shape as the realtime cell: host process dialing the baked
    // allocator port.
    let port = port_of(
        PlatformId::ThreadxLinux,
        Lang::Rust,
        Workload::RealtimeTiers,
    );
    let _router = ZenohRouter::start_on("127.0.0.1", port)
        .unwrap_or_else(|e| nros_tests::skip!("zenohd failed to start on {port}: {e}"));

    let mut cmd = Command::new(&entry);
    cmd.env("RUST_LOG", "info");
    let mut guest = ManagedProcess::spawn_command(cmd, "threadx-preempt-entry")
        .unwrap_or_else(|e| panic!("spawn threadx-linux realtime entry: {e}"));

    // The marker prints at tier bring-up (spawn path), well before steady
    // state; 30 s covers a cold boot with headroom.
    let log = guest
        .wait_for_output_pattern(THREADX_PREEMPT_MARKER, Duration::from_secs(30))
        .unwrap_or_else(|e| {
            guest.kill();
            panic!(
                "declared preempt_threshold produced no `{THREADX_PREEMPT_MARKER}` \
                 marker — the non_preempt_scope was silently dropped (RFC-0052 \
                 fail-loud violation). err: {e:?}"
            )
        });
    guest.kill();

    let hits = nros_tests::count_pattern(&log, THREADX_PREEMPT_MARKER);
    // Exactly ONE tier (`high`) declares a threshold in this fixture.
    assert_eq!(
        hits, 1,
        "expected the kernel-accepted threshold marker for exactly the 1 \
         declaring tier (`high`); saw {hits} in:\n{log}"
    );
}

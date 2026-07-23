//! phase-296 W5.5 Task 4 — the Zephyr Native EDF claim is honored end-to-end:
//! a real-time tier carrying a `deadline_us` gets `k_thread_deadline_set`
//! applied at boot (trace-confirmed), not merely recorded `Native` on the
//! host by `sched_caps_from_deploy` (Task 1).
//!
//! Reuses the SAME fixture `realtime_tiers_e2e.rs` boots for its
//! `zephyr_rust` cell — `ws-realtime-rust`'s Zephyr native_sim entry
//! (`build_zephyr_workspace_rust_realtime_entry`) — which Task 3 extended
//! in place: `demo_bringup/config/system_model.yaml`'s `high` tier is now
//! `class = real_time` with a ZEPHYR-SCOPED `zephyr.deadline_us = 10000`
//! (`low` stays plain, no deadline). So exactly ONE tier applies the kernel
//! EDF deadline at boot; this test asserts exactly that count, trace-marker
//! confirmed via `nros_tests::output::ZEPHYR_EDF_DEADLINE_MARKER` (Task 2).
//!
//! Run with: `cargo nextest run -p nros-tests --test zephyr_edf_deadline_applied`.

use nros_tests::{
    alloc::port_of,
    fixtures::{ZenohRouter, build_zephyr_workspace_rust_realtime_entry},
    matrix::{Lang, PlatformId, Workload},
    output::ZEPHYR_EDF_DEADLINE_MARKER,
    zephyr::{ZephyrPlatform, ZephyrProcess},
};
use std::time::Duration;

#[test]
fn zephyr_edf_deadline_applied_for_real_time_tier() {
    // Precondition discipline: missing/stale fixture -> skip!, never a bare
    // eprintln+return (that would report a false PASS).
    let entry = build_zephyr_workspace_rust_realtime_entry()
        .unwrap_or_else(|e| nros_tests::skip!("ws-realtime-rust zephyr fixture unavailable: {e}"));

    // The boot-tier `apply_tier_deadline` call only runs AFTER
    // `Executor::open` succeeds (entry_tiers.rs `run_tiers`), so the image's
    // baked zenoh locator needs a live router — same port the
    // `realtime_tiers_e2e.rs` `zephyr_rust` cell starts for this exact
    // fixture (`nros_tests::alloc::port_of`, the ONE allocator formula the
    // fixture bake and the test both use).
    let port = port_of(
        PlatformId::ZephyrNativeSim,
        Lang::Rust,
        Workload::RealtimeTiers,
    );
    let _router = ZenohRouter::start_on("127.0.0.1", port)
        .unwrap_or_else(|e| nros_tests::skip!("zenohd failed to start on {port}: {e}"));

    let mut zephyr = ZephyrProcess::start(&entry, ZephyrPlatform::NativeSim)
        .unwrap_or_else(|e| panic!("boot zephyr native_sim: {e}"));

    // The marker fires at tier setup time, early in `run_tiers` boot —
    // well before the 12 s spin budget other cells give this same image
    // in `realtime_tiers_e2e.rs`. Wait on the marker itself (not a fixed
    // sleep) so this stays robust under load; 30 s covers a cold
    // native_sim boot with headroom.
    let log = zephyr.wait_for_pattern(ZEPHYR_EDF_DEADLINE_MARKER, Duration::from_secs(30));
    zephyr.kill();

    let hits = nros_tests::count_pattern(&log, ZEPHYR_EDF_DEADLINE_MARKER);
    // Task 3 baked exactly ONE real_time tier with a zephyr-scoped
    // `deadline_us` (`high`; `low` carries none) — assert the exact count
    // this fixture produces, not a generic "at least one".
    assert_eq!(
        hits, 1,
        "expected k_thread_deadline_set applied for exactly the 1 real-time \
         deadline tier this fixture bakes (`high`); saw {hits} \
         `{ZEPHYR_EDF_DEADLINE_MARKER}` line(s) in:\n{log}"
    );
}

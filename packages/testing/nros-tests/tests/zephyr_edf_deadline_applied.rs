//! phase-296 W5.5 Task 4 (+ the W5.8 C/C++ follow-up) — the Zephyr Native
//! EDF claim is honored end-to-end IN EVERY LANGUAGE ARM: a real-time tier
//! carrying a `deadline_us` gets `k_thread_deadline_set` applied at boot
//! (trace-confirmed), not merely recorded `Native` on the host by
//! `sched_caps_from_deploy`.
//!
//! Reuses the SAME fixtures `realtime_tiers_e2e.rs` boots for its zephyr
//! cells — the `ws-realtime-{rust,cpp,c}` Zephyr native_sim entries, whose
//! `demo_bringup/config/system_model.yaml` `high` tier is `class =
//! real_time` with a ZEPHYR-SCOPED `zephyr.deadline_us = 10000` (`low`
//! stays plain). So exactly ONE tier applies the kernel EDF deadline at
//! boot per image; each case asserts that exact count via
//! `nros_tests::output::ZEPHYR_EDF_DEADLINE_MARKER`. The Rust arm goes
//! through `entry_tiers.rs::apply_tier_deadline` (W5.5); the C/C++ arms
//! through `zephyr_run_tiers.c::zephyr_apply_tier_deadline` (W5.8) — the
//! marker literal is identical by contract (three-way lockstep with
//! `output.rs`).
//!
//! Run with: `cargo nextest run -p nros-tests --test zephyr_edf_deadline_applied`.

use nros_tests::{
    alloc::port_of,
    fixtures::{
        ZenohRouter, build_zephyr_workspace_c_realtime_entry,
        build_zephyr_workspace_cpp_realtime_entry, build_zephyr_workspace_rust_realtime_entry,
    },
    matrix::{Lang, PlatformId, Workload},
    output::ZEPHYR_EDF_DEADLINE_MARKER,
    zephyr::{ZephyrPlatform, ZephyrProcess},
};
use std::{path::PathBuf, time::Duration};

/// Boot one language arm's realtime image against its baked router port and
/// assert exactly ONE `k_thread_deadline_set` application (the `high` tier).
fn assert_edf_applied(
    lang: Lang,
    lang_name: &str,
    resolver: fn() -> nros_tests::TestResult<PathBuf>,
) {
    // Precondition discipline: missing/stale fixture -> skip!, never a bare
    // eprintln+return (that would report a false PASS).
    let entry = resolver().unwrap_or_else(|e| {
        nros_tests::skip!("ws-realtime-{lang_name} zephyr fixture unavailable: {e}")
    });

    // The boot-tier deadline apply only runs AFTER the session open
    // succeeds, so the image's baked zenoh locator needs a live router —
    // the SAME per-(platform, lang, workload) port the fixture's locator
    // was baked with (`nros_tests::alloc::port_of`, the one allocator
    // formula the bake and the tests share).
    let port = port_of(PlatformId::ZephyrNativeSim, lang, Workload::RealtimeTiers);
    let _router = ZenohRouter::start_on("127.0.0.1", port)
        .unwrap_or_else(|e| nros_tests::skip!("zenohd failed to start on {port}: {e}"));

    let mut zephyr = ZephyrProcess::start(&entry, ZephyrPlatform::NativeSim)
        .unwrap_or_else(|e| panic!("boot zephyr native_sim ({lang_name}): {e}"));

    // The marker fires at tier setup time, early in boot — wait on the
    // marker itself (not a fixed sleep); 30 s covers a cold native_sim
    // boot with headroom.
    let log = zephyr.wait_for_pattern(ZEPHYR_EDF_DEADLINE_MARKER, Duration::from_secs(30));
    zephyr.kill();

    let hits = nros_tests::count_pattern(&log, ZEPHYR_EDF_DEADLINE_MARKER);
    // Each fixture bakes exactly ONE real_time tier with a zephyr-scoped
    // `deadline_us` (`high`; `low` carries none) — assert the exact count,
    // not a generic "at least one".
    assert_eq!(
        hits, 1,
        "[{lang_name}] expected k_thread_deadline_set applied for exactly the \
         1 real-time deadline tier this fixture bakes (`high`); saw {hits} \
         `{ZEPHYR_EDF_DEADLINE_MARKER}` line(s) in:\n{log}"
    );
}

/// W5.5 — the Rust arm (`entry_tiers.rs::apply_tier_deadline`).
#[test]
fn zephyr_edf_deadline_applied_for_real_time_tier() {
    assert_edf_applied(
        Lang::Rust,
        "rust",
        build_zephyr_workspace_rust_realtime_entry,
    );
}

/// W5.8 follow-up — the C++ arm (`zephyr_run_tiers.c`).
#[test]
fn zephyr_edf_deadline_applied_cpp() {
    assert_edf_applied(Lang::Cpp, "cpp", build_zephyr_workspace_cpp_realtime_entry);
}

/// W5.8 follow-up — the C arm (same C seam, C-language components).
#[test]
fn zephyr_edf_deadline_applied_c() {
    assert_edf_applied(Lang::C, "c", build_zephyr_workspace_c_realtime_entry);
}

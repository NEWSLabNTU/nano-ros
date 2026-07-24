//! phase-296 W5.11 — the Zephyr CPU-pin (`placement` dim, RFC-0052) is NEVER
//! silently dropped: a tier that declares a zephyr-scoped `core` (the
//! ws-realtime-rust `low` tier pins to CPU 0) must produce, at boot, EITHER the
//! kernel-accept marker (`k_thread_cpu_pin` honored — [`ZEPHYR_CORE_PIN_MARKER`])
//! OR the honest fallback note (the image lacks `CONFIG_SCHED_CPU_MASK_PIN_ONLY`
//! / SMP so the pin cannot be honored — [`ZEPHYR_CORE_PIN_FALLBACK_MARKER`]).
//! Either way the declared placement is on the record; silence is the failure
//! mode (the RFC-0052 fail-loud contract). The current single-CPU native_sim
//! fixture has no SMP/pin config, so today the FALLBACK arm fires; an SMP
//! fixture would flip it to ACCEPT — this test is green across that transition
//! by design (mirrors `nuttx_sporadic_budget_applied`'s two-mode assertion).
//!
//! Boots the Rust arm image (`entry_tiers.rs::apply_tier_core_pin`); the C/C++
//! arm prints the same lockstepped literal via `zephyr_apply_core_pin`.
//!
//! Run with: `cargo nextest run -p nros-tests --test zephyr_core_pin_applied`.

use nros_tests::{
    alloc::port_of,
    fixtures::{ZenohRouter, build_zephyr_workspace_rust_realtime_entry},
    matrix::{Lang, PlatformId, Workload},
    output::{ZEPHYR_CORE_PIN_FALLBACK_MARKER, ZEPHYR_CORE_PIN_MARKER},
    zephyr::{ZephyrPlatform, ZephyrProcess},
};
use std::time::Duration;

#[test]
fn zephyr_core_pin_never_silently_dropped() {
    let entry = build_zephyr_workspace_rust_realtime_entry()
        .unwrap_or_else(|e| nros_tests::skip!("ws-realtime-rust zephyr fixture unavailable: {e}"));

    // The core-pin apply runs at tier setup, but the boot tier's setup only
    // proceeds after the session opens — the baked locator needs a live router
    // on the SAME per-(platform, lang, workload) port the fixture baked.
    let port = port_of(
        PlatformId::ZephyrNativeSim,
        Lang::Rust,
        Workload::RealtimeTiers,
    );
    let _router = ZenohRouter::start_on("127.0.0.1", port)
        .unwrap_or_else(|e| nros_tests::skip!("zenohd failed to start on {port}: {e}"));

    let mut zephyr = ZephyrProcess::start(&entry, ZephyrPlatform::NativeSim)
        .unwrap_or_else(|e| panic!("boot zephyr native_sim (rust): {e}"));

    // Accept ("core pin tier=") and fallback ("core pin FAILED tier=") share no
    // prefix, so wait on the common stem, then classify. 30 s covers a cold
    // native_sim boot with headroom.
    let log = zephyr.wait_for_pattern("nros: core pin", Duration::from_secs(30));
    zephyr.kill();

    let accepted = log.contains(ZEPHYR_CORE_PIN_MARKER);
    let fallback = log.contains(ZEPHYR_CORE_PIN_FALLBACK_MARKER);
    assert!(
        accepted || fallback,
        "the ws-realtime-rust `low` tier declares a zephyr-scoped `core` but boot \
         produced NEITHER the kernel-accept marker (`{ZEPHYR_CORE_PIN_MARKER}`) NOR \
         the honest fallback note (`{ZEPHYR_CORE_PIN_FALLBACK_MARKER}`) — the \
         placement dim was silently dropped (RFC-0052 fail-loud violation). log:\n{log}"
    );
    // Informational: which arm ran (flips to `accepted` once an SMP fixture with
    // CONFIG_SCHED_CPU_MASK_PIN_ONLY bakes the pin as honorable).
    eprintln!(
        "zephyr core pin: {}",
        if accepted {
            "KERNEL-ACCEPTED (k_thread_cpu_pin honored)"
        } else {
            "honest fallback (no SMP/CONFIG_SCHED_CPU_MASK_PIN_ONLY; tier runs unpinned)"
        }
    );
}

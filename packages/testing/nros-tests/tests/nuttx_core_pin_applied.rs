//! phase-296 W5.11 — the NuttX SMP core-pin (`placement` dim, RFC-0052) is
//! NEVER silently dropped: a tier that declares a nuttx-scoped `core` (the
//! ws-realtime-rust `low` tier pins to CPU 0) must produce, at boot, EITHER the
//! kernel-accept marker (`pthread_setaffinity_np` honored —
//! [`NUTTX_CORE_PIN_MARKER`]) OR the honest fallback note (the image lacks
//! `CONFIG_SMP`, so there is no affinity API to honor it —
//! [`NUTTX_CORE_PIN_FALLBACK_MARKER`]). Either way the declared placement is on
//! the record; silence is the failure mode (the RFC-0052 fail-loud contract).
//!
//! Before W5.11 the tier ABI carried `core_plus1` but NO NuttX consumer applied
//! it — a declared `core` was silently dropped. W5.11 adds the board-seam helper
//! `nros_nuttx_apply_current_affinity` (externed + self-applied by the Rust
//! `nros-board-nuttx` run_tiers, and called by the C/C++ seam). The current
//! qemu-arm-virt fixture is single-core (no `CONFIG_SMP`), so today the FALLBACK
//! arm fires; an SMP kernel would flip it to ACCEPT — this test is green across
//! that transition by design (mirrors `nuttx_sporadic_budget_applied`).
//!
//! Boots the Rust arm image (the `apply_tier_affinity` extern under test).
//!
//! Run with: `cargo nextest run -p nros-tests --test nuttx_core_pin_applied`.

use nros_tests::{
    alloc::port_of,
    fixtures::{ZenohRouter, build_nuttx_workspace_rust_realtime_entry},
    matrix::{Lang, PlatformId, Workload},
    output::{NUTTX_CORE_PIN_FALLBACK_MARKER, NUTTX_CORE_PIN_MARKER},
    qemu::QemuProcess,
};
use std::time::Duration;

#[test]
fn nuttx_core_pin_never_silently_dropped() {
    let entry = build_nuttx_workspace_rust_realtime_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("nuttx rust realtime fixture unavailable: {e}"));

    // Same shape as realtime_tiers_e2e's nuttx cells: router on the baked
    // slirp-visible port (0.0.0.0 — the guest dials the slirp gateway).
    let port = port_of(PlatformId::NuttxArm, Lang::Rust, Workload::RealtimeTiers);
    let _router = ZenohRouter::start_on("0.0.0.0", port)
        .unwrap_or_else(|e| nros_tests::skip!("zenohd failed to start on {port}: {e}"));

    let mut qemu = QemuProcess::start_nuttx_virt(&entry, true)
        .unwrap_or_else(|e| panic!("boot NuttX arm-virt QEMU: {e}"));

    // Accept ("core pin tier=") and fallback ("core pin FAILED tier=") share no
    // prefix, so wait on the common stem, then classify. The `low` tier (which
    // declares `core`) is a SPAWNED tier on nuttx (resolve_tiers descending →
    // tiers[0]=`high`=boot), so the marker comes from `nuttx_run_one_tier`.
    let log = qemu.wait_for_output_pattern("nros: core pin", Duration::from_secs(90));
    qemu.kill();
    let log = log.unwrap_or_else(|e| {
        panic!(
            "the ws-realtime-rust `low` tier declares a nuttx-scoped `core` but boot \
             produced NEITHER the kernel-accept marker (`{NUTTX_CORE_PIN_MARKER}`) NOR \
             the honest fallback note (`{NUTTX_CORE_PIN_FALLBACK_MARKER}`) — the \
             placement dim was silently dropped (RFC-0052 fail-loud violation). err: {e:?}"
        )
    });

    let accepted = log.contains(NUTTX_CORE_PIN_MARKER);
    let fallback = log.contains(NUTTX_CORE_PIN_FALLBACK_MARKER);
    assert!(
        accepted || fallback,
        "matched the stem but neither full marker present — literals drifted from \
         nuttx_run_tiers.c? log:\n{log}"
    );
    // Informational: which arm ran (flips to `accepted` on an SMP kernel).
    eprintln!(
        "nuttx core pin: {}",
        if accepted {
            "KERNEL-ACCEPTED (pthread_setaffinity_np honored)"
        } else {
            "honest fallback (kernel lacks CONFIG_SMP; tier runs unpinned)"
        }
    );
}

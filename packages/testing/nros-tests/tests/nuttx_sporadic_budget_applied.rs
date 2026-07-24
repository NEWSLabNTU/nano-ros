//! phase-296 W5.9 — the NuttX sporadic-server policy is NEVER silently
//! dropped: a tier that declares `class: real_time` + a nuttx-scoped
//! `budget_us`/`period_us` (the ws-realtime `high` tier) must produce, at
//! boot, EITHER the kernel-accept marker (`SCHED_SPORADIC` applied —
//! [`NUTTX_SPORADIC_MARKER`]) OR the honest fallback note (kernel built
//! without `CONFIG_SCHED_SPORADIC` — [`NUTTX_SPORADIC_FALLBACK_MARKER`]).
//! Either way the declared policy is on the record; silence is the failure
//! mode (the RFC-0052 fail-loud contract). The current prebuilt kernel
//! export lacks the config, so today the FALLBACK arm fires; after a kernel
//! re-provision with the W5.9 defconfig the ACCEPT arm takes over — this
//! test is green across that transition by design.
//!
//! Boots the C++ arm image (the C seam under test —
//! `nros_nuttx_apply_current_sporadic`; the Rust arm externs the same
//! helper but its realtime cell is #246-red pre-existing, so the C++ image
//! is the vehicle).
//!
//! Run with: `cargo nextest run -p nros-tests --test nuttx_sporadic_budget_applied`.

use nros_tests::{
    alloc::port_of,
    fixtures::{ZenohRouter, build_nuttx_workspace_cpp_realtime_entry},
    matrix::{Lang, PlatformId, Workload},
    output::{NUTTX_SPORADIC_FALLBACK_MARKER, NUTTX_SPORADIC_MARKER},
    qemu::QemuProcess,
};
use std::time::Duration;

#[test]
fn nuttx_sporadic_policy_never_silently_dropped() {
    let entry = build_nuttx_workspace_cpp_realtime_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("nuttx cpp realtime fixture unavailable: {e}"));

    // Same shape as realtime_tiers_e2e's nuttx cells: router on the baked
    // slirp-visible port (0.0.0.0 — the guest dials the slirp gateway).
    let port = port_of(PlatformId::NuttxArm, Lang::Cpp, Workload::RealtimeTiers);
    let _router = ZenohRouter::start_on("0.0.0.0", port)
        .unwrap_or_else(|e| nros_tests::skip!("zenohd failed to start on {port}: {e}"));

    let mut qemu = QemuProcess::start_nuttx_virt(&entry, true)
        .unwrap_or_else(|e| panic!("boot NuttX arm-virt QEMU: {e}"));

    // Wait on EITHER marker (the accept and the fallback share no prefix, so
    // wait on the common stem `nros: sporadic budget`), with cold-QEMU
    // headroom; then classify.
    let log = qemu.wait_for_output_pattern("nros: sporadic budget", Duration::from_secs(90));
    qemu.kill();
    let log = log.unwrap_or_else(|e| {
        panic!(
            "declared sporadic policy produced NEITHER the kernel-accept marker \
             (`{NUTTX_SPORADIC_MARKER}`) NOR the honest fallback note \
             (`{NUTTX_SPORADIC_FALLBACK_MARKER}`) — the policy was silently \
             dropped (RFC-0052 fail-loud violation). err: {e:?}"
        )
    });

    let accepted = log.contains(NUTTX_SPORADIC_MARKER);
    let fallback = log.contains(NUTTX_SPORADIC_FALLBACK_MARKER);
    assert!(
        accepted || fallback,
        "matched the stem but neither full marker present — literals drifted \
         from nuttx_run_tiers.c? log:\n{log}"
    );
    // Informational: which arm ran (flips to `accepted` once the kernel is
    // re-provisioned with CONFIG_SCHED_SPORADIC).
    eprintln!(
        "nuttx sporadic policy: {}",
        if accepted {
            "KERNEL-ACCEPTED (SCHED_SPORADIC live)"
        } else {
            "honest fallback (kernel lacks CONFIG_SCHED_SPORADIC; executor SchedContext enforces)"
        }
    );
}

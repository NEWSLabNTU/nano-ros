//! phase-302 W3/W5 (issue 0263) — the NuttX **Rust** arm's spawned tiers
//! adopt their declared SCHED_FIFO priority at entry
//! (`nros_nuttx_apply_current_priority`): the marker
//! ([`NUTTX_TIER_PRIORITY_MARKER`]) must appear at boot for the spawned
//! tier, or the loud failure note — silence is the failure mode (the
//! RFC-0052 fail-loud contract). Before W3 the priority was silently
//! dropped off the sporadic path (std::thread has no priority attr; the
//! C arm sets it at pthread_create).
//!
//! Boots the RUST arm image — the arm under test; the C shim is shared
//! with the C/C++ create-time path, so only the Rust self-apply needs a
//! dedicated proof.
//!
//! Run with: `cargo nextest run -p nros-tests --test nuttx_tier_priority_applied`.

use nros_tests::{
    alloc::port_of,
    fixtures::{ZenohRouter, build_nuttx_workspace_rust_realtime_entry},
    matrix::{Lang, PlatformId, Workload},
    output::NUTTX_TIER_PRIORITY_MARKER,
    qemu::QemuProcess,
};
use std::time::Duration;

#[test]
fn nuttx_rust_tier_priority_never_silently_dropped() {
    let entry = build_nuttx_workspace_rust_realtime_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("nuttx rust realtime fixture unavailable: {e}"));

    let port = port_of(PlatformId::NuttxArm, Lang::Rust, Workload::RealtimeTiers);
    let _router = ZenohRouter::start_on("0.0.0.0", port)
        .unwrap_or_else(|e| nros_tests::skip!("zenohd failed to start on {port}: {e}"));

    let mut qemu = QemuProcess::start_nuttx_virt(&entry, true)
        .unwrap_or_else(|e| panic!("boot NuttX arm-virt QEMU: {e}"));

    // The accept and failure prints share the `nros: tier priority` stem;
    // wait on that with cold-QEMU headroom, then classify.
    let log = qemu.wait_for_output_pattern("nros: tier priority", Duration::from_secs(90));
    qemu.kill();
    let log = log.unwrap_or_else(|e| {
        panic!(
            "declared tier priority produced NEITHER the adopt marker \
             (`{NUTTX_TIER_PRIORITY_MARKER}`) NOR the loud failure note — the \
             priority was silently dropped again (issue 0263 regression). \
             err: {e:?}"
        )
    });

    assert!(
        log.contains(NUTTX_TIER_PRIORITY_MARKER) || log.contains("tier priority FAILED"),
        "matched the stem but neither the adopt marker nor the failure note: {log}"
    );
}

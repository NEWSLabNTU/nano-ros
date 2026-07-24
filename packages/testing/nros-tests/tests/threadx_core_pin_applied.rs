//! phase-296 W5.13 — the ThreadX core-pin (`placement` dim, RFC-0052's "SMP
//! core exclude") is NEVER silently dropped: the ws-realtime-rust `low` tier
//! declares `threadx.core: 0`, and the board must print, at tier bring-up,
//! EITHER the kernel-accept marker (`tx_thread_smp_core_exclude` honored —
//! [`THREADX_CORE_PIN_MARKER`]) OR the honest fallback note (the image lacks
//! `TX_THREAD_SMP`, so there is no core-affinity API —
//! [`THREADX_CORE_PIN_FALLBACK_MARKER`]).
//!
//! The threadx-linux port is non-SMP, so today the FALLBACK arm fires; an SMP
//! ThreadX build flips it to ACCEPT — green across that transition by design
//! (the two-mode shape of `nuttx_sporadic_budget_applied`).
//!
//! Boots the SAME threadx-linux host image the `threadx_linux_rust` realtime
//! cell uses (shared image + baked router port — serialized via the
//! `threadx-realtime-rust-port` nextest group). `low` is the BOOT tier on
//! ThreadX (`resolve_tiers` sorts descending by raw number), so the note comes
//! from the boot path.
//!
//! Run with: `cargo nextest run -p nros-tests --test threadx_core_pin_applied`.

use nros_tests::{
    alloc::port_of,
    fixtures::{ZenohRouter, build_threadx_workspace_rust_realtime_entry},
    matrix::{Lang, PlatformId, Workload},
    output::{THREADX_CORE_PIN_FALLBACK_MARKER, THREADX_CORE_PIN_MARKER},
    process::ManagedProcess,
};
use std::{process::Command, time::Duration};

#[test]
fn threadx_core_pin_never_silently_dropped() {
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
    let mut guest = ManagedProcess::spawn_command(cmd, "threadx-core-pin-entry")
        .unwrap_or_else(|e| panic!("spawn threadx-linux realtime entry: {e}"));

    // Accept ("core pin tier=") and fallback ("core pin FAILED tier=") share no
    // prefix, so wait on the common stem, then classify.
    let log = guest
        .wait_for_output_pattern("nros: core pin", Duration::from_secs(30))
        .unwrap_or_else(|e| {
            guest.kill();
            panic!(
                "the ws-realtime-rust `low` tier declares `threadx.core` but boot \
                 produced NEITHER the accept marker (`{THREADX_CORE_PIN_MARKER}`) NOR \
                 the honest fallback note (`{THREADX_CORE_PIN_FALLBACK_MARKER}`) — the \
                 placement dim was silently dropped (RFC-0052 fail-loud violation). err: {e:?}"
            )
        });
    guest.kill();

    let accepted = log.contains(THREADX_CORE_PIN_MARKER);
    let fallback = log.contains(THREADX_CORE_PIN_FALLBACK_MARKER);
    assert!(
        accepted || fallback,
        "matched the stem but neither full marker present — literals drifted from \
         nros-board-threadx/src/entry.rs? log:\n{log}"
    );
    eprintln!(
        "threadx core pin: {}",
        if accepted {
            "KERNEL-ACCEPTED (tx_thread_smp_core_exclude)"
        } else {
            "honest fallback (build lacks TX_THREAD_SMP; tier runs unpinned)"
        }
    );
}

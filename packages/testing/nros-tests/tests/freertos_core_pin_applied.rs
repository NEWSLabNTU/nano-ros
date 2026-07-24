//! phase-296 W5.11 — the FreeRTOS core-pin (`placement` dim, RFC-0052) is NEVER
//! silently dropped: a tier that declares a freertos-scoped `core` (the
//! ws-realtime-cpp-mps2 `low` tier pins to core 0) must produce, at tier
//! creation, EITHER the kernel-accept marker (`vTaskCoreAffinitySet` on a
//! `configUSE_CORE_AFFINITY` build — [`FREERTOS_CORE_PIN_MARKER`]) OR the honest
//! fallback note (a uniprocessor build has no affinity API —
//! [`FREERTOS_CORE_PIN_FALLBACK_MARKER`]). Before W5.11 the uniprocessor branch
//! was a SILENT `(void)task` — the placement was dropped with no trace.
//!
//! The mps2-an385 target is uniprocessor, so today the FALLBACK arm fires; an
//! SMP FreeRTOS build flips it to ACCEPT — green across that transition by
//! design (the two-mode shape of `nuttx_sporadic_budget_applied`). The note
//! reaches the test over the semihosting console (the freertos QEMU runs with
//! `-semihosting-config`).
//!
//! Run with: `cargo nextest run -p nros-tests --test freertos_core_pin_applied`.

use nros_tests::{
    alloc::port_of,
    fixtures::{ZenohRouter, build_freertos_workspace_cpp_realtime_entry},
    matrix::{Lang, PlatformId, Workload},
    output::{FREERTOS_CORE_PIN_FALLBACK_MARKER, FREERTOS_CORE_PIN_MARKER},
    qemu::QemuProcess,
};
use std::time::Duration;

#[test]
fn freertos_core_pin_never_silently_dropped() {
    let entry = build_freertos_workspace_cpp_realtime_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("freertos cpp realtime fixture unavailable: {e}"));

    // The `low` tier is spawned only AFTER the boot tier's session connects
    // (issue #144 ordering), so the guest needs its baked host router — the
    // same per-(platform, lang, workload) port the fixture baked.
    let port = port_of(PlatformId::FreertosMps2, Lang::Cpp, Workload::RealtimeTiers);
    let _router = ZenohRouter::start_on("0.0.0.0", port)
        .unwrap_or_else(|e| nros_tests::skip!("zenohd failed to start on {port}: {e}"));

    let mut qemu = QemuProcess::start_mps2_an385_freertos_slirp(&entry)
        .unwrap_or_else(|e| panic!("boot mps2-an385 FreeRTOS QEMU: {e}"));

    // Accept ("core pin tier=") and fallback ("core pin FAILED tier=") share no
    // prefix, so wait on the common stem, then classify. 90 s covers the cold
    // FreeRTOS boot + host-session connect that precedes the spawned `low` tier.
    let log = qemu.wait_for_output_pattern("nros: core pin", Duration::from_secs(90));
    qemu.kill();
    let log = log.unwrap_or_else(|e| {
        panic!(
            "the ws-realtime-cpp-mps2 `low` tier declares a freertos-scoped `core` but \
             boot produced NEITHER the accept marker (`{FREERTOS_CORE_PIN_MARKER}`) NOR \
             the honest fallback note (`{FREERTOS_CORE_PIN_FALLBACK_MARKER}`) — the \
             placement dim was silently dropped (RFC-0052 fail-loud violation). err: {e:?}"
        )
    });

    let accepted = log.contains(FREERTOS_CORE_PIN_MARKER);
    let fallback = log.contains(FREERTOS_CORE_PIN_FALLBACK_MARKER);
    assert!(
        accepted || fallback,
        "matched the stem but neither full marker present — literals drifted from \
         freertos_run_tiers.c? log:\n{log}"
    );
    eprintln!(
        "freertos core pin: {}",
        if accepted {
            "ACCEPTED (vTaskCoreAffinitySet)"
        } else {
            "honest fallback (uniprocessor build, no configUSE_CORE_AFFINITY)"
        }
    );
}

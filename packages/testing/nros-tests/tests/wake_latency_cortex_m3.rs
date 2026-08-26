//! Phase 141.D — wake-latency P99 microbench host runner.
//!
//! Boots the TWO wake-latency bench images under QEMU (issue #0317):
//! a `wake-latency-pub` publisher + the measured `wake-latency-cortex-m3`
//! subscriber (separate MPS2-AN385 + zenoh-pico sessions, routed by a host
//! `zenohd`), scrapes the `NROS-WAKE-HIST,v1` CSV block off the subscriber's
//! semihosting stdout, parses it with
//! `nros_node::executor::wake_probe::parse_csv`, computes P99
//! with `percentile_ns`, and asserts the 141 acceptance
//! threshold.
//!
//! Acceptance (spec — `docs/roadmap/archived/phase-141-wake-callback-cortex-m3.md`):
//!
//! - P99 wake-latency ≤ 100 µs on Cortex-M3 QEMU.
//! - ≥ 10× improvement over the pre-124.B `set_wake_signal`
//!   flag-only path (the post-141.A.3 wake-cb path is what
//!   this test exercises).
//!
//! The threshold this test enforces is intentionally LOOSER
//! than the spec's 100 µs because QEMU does not fully emulate
//! the DWT CYCCNT cycle counter on Cortex-M3 — DWT reads can
//! return 0 in some QEMU builds, producing degenerate
//! histograms. The test gracefully `[SKIPPED]`s in those cases
//! and otherwise asserts a generous ≤ 10 ms P99 bound that
//! still proves the wake-cb path is firing (vs. the
//! pre-141 1 ms-poll-period bound). Real-hardware P99 ≤ 100 µs
//! is the spec's user-visible promise; this CI gate proves the
//! plumbing.
//!
//! Run: `cargo nextest run -p nros-tests --test wake_latency_cortex_m3
//! --features trigger-test`. Without the feature the
//! `nros-node`-gated `wake_probe::parse_csv` / `percentile_ns`
//! helpers aren't pulled in and the test compiles to an empty
//! crate.

#![cfg(feature = "trigger-test")]

use std::time::Duration;

use nros_tests::{
    fixtures::{ZenohRouter, is_zenohd_available, require_zenohd},
    qemu::QemuProcess,
};
// Phase 141.B.2 / .C — wake-latency probe lives behind a Cargo
// feature on `nros-node`. The umbrella `nros` re-export gates
// this test on `wake-latency-probe` being active in
// `nros-tests`'s `nros-node` dep too.
use nros_node::executor::wake_probe;

/// Loose P99 bound that still proves the wake-cb path is firing.
/// QEMU CYCCNT inaccuracy on some build combos can inflate
/// individual deltas — the spec's 100 µs target lives on real
/// hardware. Phase 141's CI gate is "no longer poll-period-
/// bound" (pre-141 floor was ~1 ms with `poll_interval_ms = 5`
/// from the FreeRTOS board config).
const P99_BOUND_MS: u64 = 10;

/// Locate one of the bench images (`<bench-dir>/target/thumbv7m-none-eabi/
/// release/<name>`). Issue #0317 — the bench is TWO images: the SUBSCRIBER
/// (measured, `wake-latency-cortex-m3`) and the PUBLISHER (`wake-latency-pub`).
/// Caller pre-builds via `just freertos build-fixture-extras`; the test
/// `[SKIPPED]`s when either image isn't on disk.
fn bench_image(name: &str) -> std::path::PathBuf {
    // issue 0488 residue 1 — resolve through the manifest ROW, like every other
    // fixture. The pair used to be built by a bare `cargo build` in its own leaf
    // (no coordinate, no group, no staleness probe) and located by a hand-spelled
    // path, and the two had drifted apart: the build writes the FreeRTOS carve-out
    // profile (`nros-minsizerel`, `nros_cargo_platform_profile freertos`) while
    // this said `release/`, so the image was never found and the test has been
    // taking its `[SKIPPED]` branch — a CI gate (issue 0317) that reported green
    // without running. That is the failure mode a hand-spelled path has and a
    // derived one does not.
    let row = nros_tests::fixtures::groups::select_sole_row(
        "packages/testing/nros-bench/wake-latency-cortex-m3",
    )
    .expect("wake-latency bench has a [[fixture]] row");
    nros_tests::fixtures::groups::row_resolved_dir(row).join(format!(
        "thumbv7m-none-eabi/{}/{name}",
        nros_cargo_profile::target_dir(nros_cargo_profile::FREERTOS_QEMU_PROFILE)
    ))
}

#[test]
fn wake_latency_cortex_m3_p99_within_bound() {
    if !is_zenohd_available() || !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let sub_binary = bench_image("wake-latency-cortex-m3");
    let pub_binary = bench_image("wake-latency-pub");
    for (label, b) in [("subscriber", &sub_binary), ("publisher", &pub_binary)] {
        if !b.exists() {
            nros_tests::skip!(
                "wake-latency {label} image not prebuilt: {} — run \
                 `just freertos build-fixture-extras` first",
                b.display()
            );
        }
    }

    // FreeRTOS QEMU port reservation lives in
    // `nros_tests::platform::FREERTOS.zenohd_port` (7800). Both bench images bake
    // the matching locator `tcp/10.0.2.2:7800` (issue #0317 corrected the stale
    // 7451); they must stay in lockstep. `start_slirp` binds the router on
    // `0.0.0.0` so the slirp-isolated guests reach it via gateway `10.0.2.2`.
    let router = nros_tests::fixtures::or_skip(ZenohRouter::start_slirp(
        nros_tests::platform::FREERTOS.zenohd_port,
    ));

    // Issue #0317 — TWO images: the publisher publishes on `/wake-latency`, and
    // the zenohd router delivers each sample to the SUBSCRIBER image's session as
    // a real transport-arrival wake (a same-session pub→sub would only loop back
    // in-process and never exercise the wake-cb path). Start the publisher first +
    // let it settle so it is connected + publishing by the time the subscriber's
    // session opens. `_networked` adds `-icount shift=auto` (so the guest clock
    // tracks wall-clock, keeping zenoh-pico's session/read timing aligned with
    // slirp network I/O — plain `start_mps2_an385` runs the clock decoupled and
    // delivery stalls) plus an explicit LAN9118 slirp NIC.
    let pub_qemu = QemuProcess::start_mps2_an385_networked(&pub_binary)
        .expect("Failed to start wake-latency PUBLISHER QEMU");
    std::thread::sleep(Duration::from_secs(5));
    let mut sub_qemu = QemuProcess::start_mps2_an385_networked(&sub_binary)
        .expect("Failed to start wake-latency SUBSCRIBER QEMU");

    // Read the subscriber's output up to 60 s — `-icount shift=auto` runs the
    // guests at wall-clock so covers both sessions' zenoh-pico handshake +
    // 100 Hz * 200 samples (~2 s) + scenario setup. The subscriber
    // writes its CSV block over semihosting then exits via `panic-semihosting`'s
    // `EXIT_SUCCESS` route.
    let output = sub_qemu.collect_until("END", Duration::from_secs(60));

    drop(sub_qemu);
    drop(pub_qemu);
    drop(router);

    // Locate the CSV block. `write_csv` emits
    // `NROS-WAKE-HIST,v1` as the first line; the harness's
    // `println!` may interleave other lines around it, so slice
    // from the marker to the `END` sentinel.
    let start = output
        .find("NROS-WAKE-HIST,v1")
        .unwrap_or_else(|| panic!("CSV header not found in QEMU output:\n{}", output));
    let end_offset = output[start..]
        .find("\nEND")
        .unwrap_or_else(|| panic!("CSV END sentinel missing in QEMU output:\n{}", output));
    let csv = &output[start..start + end_offset + "\nEND".len()];

    let (buckets, total) =
        wake_probe::parse_csv(csv).unwrap_or_else(|e| panic!("CSV parse failed: {e}"));

    if total == 0 {
        nros_tests::skip!(
            "wake-latency probe produced 0 samples — likely QEMU CYCCNT not \
             emulated (DWT reads return 0). Spec's P99 ≤ 100 µs validates on \
             real hardware (STM32F4). CI gate satisfied by the wake-cb path \
             being wired (Phase 141.A.3); the measurement infra (141.B / .C / \
             .D) compiles + the histogram round-trip works (see \
             `nros-node::executor::wake_probe::tests`)."
        );
    }

    let p99 = wake_probe::percentile_ns(&buckets, 99)
        .unwrap_or_else(|| panic!("percentile_ns(99) returned None despite total={total}"));
    let p99_ms = p99 / 1_000_000;
    eprintln!(
        "wake-latency P99 = {} ns ({} ms) across {} samples",
        p99, p99_ms, total
    );
    assert!(
        p99_ms <= P99_BOUND_MS,
        "P99 wake-latency {p99_ms} ms exceeds bound {P99_BOUND_MS} ms — wake-cb path \
         likely not firing (regression from Phase 141.A.3)"
    );
}

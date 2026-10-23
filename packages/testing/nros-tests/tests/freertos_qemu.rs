//! FreeRTOS QEMU MPS2-AN385 integration tests
//!
//! **Bucket (phase-329 W4): KEEP — not matrix cells.** The standard FreeRTOS
//! pubsub/service/action DELIVERY e2e is the `rtos_e2e.rs` matrix consumer
//! (platform × lang × variant). What remains here is one-offs. The FreeRTOS
//! rust CycloneDDS boot / local-pubsub tests that lived here were `#[ignore]`d
//! from phase-220.C, when their fixture was retired, until they were deleted;
//! that coordinate is a `CarveOut` in `matrix::CELLS` now, which says how to
//! revive it.
//!
//! Tests that verify FreeRTOS examples build and run on QEMU MPS2-AN385 (Cortex-M3).
//! FreeRTOS examples use `thumbv7m-none-eabi` target with `no_std` + lwIP networking.
//!
//! Prerequisites:
//! - `FREERTOS_DIR` env var pointing to FreeRTOS kernel source (e.g., `third-party/freertos/kernel`)
//! - `LWIP_DIR` env var pointing to lwIP source (e.g., `third-party/freertos/lwip`)
//! - `arm-none-eabi-gcc` toolchain installed
//! - `qemu-system-arm` with MPS2-AN385 machine support
//!
//! The E2E test bodies live in `tests/rtos_e2e.rs` (parametrised over
//! platform × language × variant). This file keeps the prerequisite
//! detection test and the all-examples-build smoke test so FreeRTOS
//! diagnostics remain greppable under `--test freertos_qemu`.
//!
//! Run with: `just test-freertos`
//! Or: `cargo nextest run -p nros-tests --test freertos_qemu`

use nros_tests::fixtures::{
    QemuProcess, RequireFixture, build_freertos_workspace_cpp_an536_entry,
    freertos::{is_arm_gcc_available, is_freertos_available, is_lwip_available},
    is_qemu_available,
};
use std::time::Duration;

// =============================================================================
// Prerequisite checks
// =============================================================================

/// Require the FreeRTOS prerequisites; fail (unmet) loudly, naming which one,
/// otherwise — after the lane check.
///
/// Issue 1135 — this returned `bool` and every caller wrote
/// `if !require_freertos() { nros_tests::unmet!("require_freertos check failed"); }`. That is the
/// CORRECT verdict spelled uninformatively: the real reason was an
/// `eprintln!` inside the helper, and `--failure-output never` (what the
/// `just` recipes pass) eats it, so the log said only "check failed". A guard
/// that skips where it KNOWS the reason names the reason, and cannot be
/// misused by a caller who writes a bare `return` instead.
fn require_freertos() {
    // Issue 1900 — the lane FIRST. Every test here boots a `freertos`-row
    // image (mps2-an385 / mps3-an536), so a lane that selects no such
    // coordinate deselects it before the kernel/lwIP/gcc probes below.
    nros_tests::fixtures::lane::require_platform_in_lane(
        &[nros_tests::matrix::PlatformId::FreertosMps2],
        "a FreeRTOS QEMU image",
    );
    if !is_freertos_available() {
        nros_tests::unmet!(
            "no FreeRTOS kernel tree provisioned — run `just freertos setup`, or set FREERTOS_DIR"
        );
    }
    if !is_lwip_available() {
        nros_tests::unmet!("no lwIP tree provisioned — run `just freertos setup`, or set LWIP_DIR");
    }
    if !is_arm_gcc_available() {
        nros_tests::unmet!(
            "arm-none-eabi-gcc not found — install it (`sudo apt install gcc-arm-none-eabi`)"
        );
    }
}

// =============================================================================
// Prerequisite detection tests (always run)
// =============================================================================

// `test_freertos_detection` removed: it read five `is_*_available()` booleans
// and printed them, asserting nothing, so it reported PASS on a host with no
// FreeRTOS, no lwIP, no arm-none-eabi-gcc, no QEMU and no zenohd. All five
// probes stay load-bearing as the `unmet!` guards on the real tests, where a
// `false` stops the run. Forbidden repo-wide by `check-no-vacuous-tests`.

// =============================================================================
// (Phase 182.3) `test_freertos_all_examples_build` removed — it rebuilt every
// FreeRTOS example, exactly what `build-all` / `build-test-fixtures` does
// before `test-all` (the `_require-fixtures` preflight gates on it). The
// per-role binaries are consumed by the `rtos_e2e` Platform__Freertos tests.
// =============================================================================

// =============================================================================
// Issue 1624 — the non-zenoh FreeRTOS heap default, kept MEASURED
// =============================================================================

/// The KiB `FreeRTOSConfig.h` states for a non-zenoh (Cyclone / XRCE) image —
/// read from the header, because the header is what the image compiles.
fn shared_header_dds_heap_bytes() -> usize {
    let header = include_str!("../../../boards/nros-board-freertos/config/FreeRTOSConfig.h");
    let kb: usize = header
        .lines()
        .find_map(|l| {
            l.trim()
                .strip_prefix("#define NROS_FREERTOS_DDS_HEAP_KB")
                .map(|rest| rest.trim().parse().expect("a decimal KiB literal"))
        })
        .expect("FreeRTOSConfig.h defines NROS_FREERTOS_DDS_HEAP_KB");
    kb * 1024
}

/// `nros: heap peak <used> of <total> bytes` -> `used`.
fn heap_peak_bytes(output: &str) -> Option<usize> {
    output.lines().find_map(|l| {
        l.split("nros: heap peak ")
            .nth(1)?
            .split_whitespace()
            .next()?
            .parse()
            .ok()
    })
}

/// Issue 1624 — the AN536 Cyclone C++ entry boots, DELIVERS in-image, and its
/// heap peak fits the non-zenoh default `FreeRTOSConfig.h` states.
///
/// That default is derived from this image (`default_dds_heap_bytes` in
/// `nros-board-common`): it is the emulated twin of the link-only S32Z270 row,
/// the only in-tree image that compiles the default. The board itself states
/// 32 MiB, so the peak printed here is unconstrained — what the image NEEDS,
/// not what it was given. A peak above the default means the default is now
/// wrong for the image it was measured on: re-derive it from this line.
///
/// Delivery is asserted first because a peak from an image whose session never
/// opened is not a Cyclone working set — issue 1634 is exactly that: the image
/// died at `RMW session open failed` and still printed a 96,744-byte "peak".
#[test]
fn an536_cyclonedds_cpp_entry_delivers_within_the_dds_heap_default() {
    require_freertos();
    if !is_qemu_available() {
        nros_tests::unmet!("qemu-system-arm not found");
    }
    let binary = build_freertos_workspace_cpp_an536_entry().require("mps3-an536 Cyclone C++ entry");
    // A group per process, so two runs on one host do not share a LAN.
    let port = 40_000 + (std::process::id() % 20_000);
    let mut qemu = QemuProcess::start_mps3_an536_mcast(binary, &format!("230.0.16.24:{port}"))
        .expect("spawn qemu mps3-an536");
    let output = qemu.collect_until("nros: heap peak", Duration::from_secs(120));
    let cmdline = qemu.command_line().to_string();
    qemu.kill();

    let listener = nros_tests::output::INT32_LISTENER_LOG_PREFIX;
    let received = output.matches(listener).count();
    assert!(
        received >= 3,
        "the AN536 Cyclone entry did not deliver in-image ({received} `{listener}` \
         lines) — its heap peak is not a Cyclone working set.\nqemu: {cmdline}\n\
         output:\n{output}"
    );
    let peak = heap_peak_bytes(&output).unwrap_or_else(|| {
        panic!("no `nros: heap peak` line within 120 s.\nqemu: {cmdline}\noutput:\n{output}")
    });
    let budget = shared_header_dds_heap_bytes();
    assert!(
        peak <= budget,
        "the AN536 Cyclone entry peaks at {peak} bytes of heap, above the {budget}-byte \
         non-zenoh default in nros-board-freertos/config/FreeRTOSConfig.h — re-derive \
         `default_dds_heap_bytes` (nros-board-common) from this line (issue 1624).\n\
         qemu: {cmdline}"
    );
}

/// RFC-0051 §7c (phase-511) — the cases this file runs are exactly the
/// cells `matrix::consumer_of` assigns to it. Fixture-free: a cell added,
/// retiered or reassigned without this list moving is a red here.
#[test]
fn cases_bound_to_matrix_cells() {
    use nros_tests::matrix::{
        Consumer, Kind as K, Lang as L, PlatformId as P, Rmw as R, Workload as W,
        assert_consumer_cases,
    };
    assert_consumer_cases(
        Consumer::FreertosQemu,
        &[(
            P::FreertosMps2,
            L::Cpp,
            R::Cyclonedds,
            W::EntryPubsub,
            K::Workspace,
        )],
    );
}

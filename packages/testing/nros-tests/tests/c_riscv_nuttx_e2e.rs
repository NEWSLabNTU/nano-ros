//! **Bucket (phase-329 W4): KEEP — not a matrix cell / not a rtos_e2e dup.**
//! `rtos_e2e.rs`'s `Platform` enum covers Freertos/Nuttx(arm)/ThreadxLinux/
//! ThreadxRiscv64 — NOT NuttxRiscv. This is the ONLY runtime delivery test for
//! the riscv-nuttx C lane, so it stays until that platform joins the matrix.
//!
//! #199 follow-up — the FIRST riscv-nuttx (rv-virt) **C-lane RUNTIME** e2e:
//! the standalone `examples/rv-virt-nuttx/c/talker` kernel image (the very
//! binary whose link #199 fixed) boots under the phase-285 W2 harness and its
//! `/chatter` `std_msgs/String` publishes are observed CROSS-PROCESS by a
//! native listener through a host zenoh router. Before this test the riscv C
//! lane was link-checked only (see archived issues 0165/0199).
//!
//! The guest dials the router through the QEMU slirp gateway (10.0.2.2 → host)
//! at the baked allocator port (`alloc::port_of(NuttxRiscv, C, Pubsub)`, the
//! fixture row's `NROS_ENTRY_LOCATOR`); the observer dials it on loopback.
//! No TAP/bridge/root.
//!
//! The fixture is built by `just nuttx build-riscv-c`; this test fails as an
//! unmet precondition when the talker ELF / zenohd / qemu-system-riscv32 are
//! absent.
//!
//! Run with: `cargo nextest run -p nros-tests --test c_riscv_nuttx_e2e`

use nros_tests::{
    alloc::port_of,
    fixtures::{
        ManagedProcess, QemuProcess, RequireFixture, ZenohRouter, build_native_listener,
        build_nuttx_riscv_c_talker, require_zenohd,
    },
    matrix::{Lang, PlatformId, Workload},
};
use std::{process::Command, time::Duration};

/// The router port baked into the riscv C talker's locator — the allocator's
/// (nuttx-riscv, c, pubsub) number, matching the `rv-virt-nuttx/c/talker`
/// fixture row's `NROS_ENTRY_LOCATOR` bake.
const C_RISCV_NUTTX_TALKER_PORT: u16 = port_of(PlatformId::NuttxRiscv, Lang::C, Workload::Pubsub);

#[test]
fn c_riscv_nuttx_talker_delivers_cross_process() {
    // Issue 1685 — deselect before the host probes when the lane holds no
    // riscv NuttX coordinate.
    nros_tests::fixtures::lane::require_platform_in_lane(
        &[PlatformId::NuttxRiscv],
        "the riscv NuttX C talker",
    );
    require_zenohd();
    // Issue 1884 — the probe the boot needs (`-M virt`), not ESP32's.
    if !nros_tests::qemu::is_qemu_riscv32_virt_available() {
        nros_tests::unmet!("qemu-system-riscv32 with the `virt` machine not found");
    }

    let talker = build_nuttx_riscv_c_talker()
        .map(|p| p.to_path_buf())
        .require("riscv-nuttx C talker");
    let listener = build_native_listener()
        .map(|p| p.to_path_buf())
        .require("native listener");

    // Router on the baked port; listen on 0.0.0.0 so the slirp guest
    // (10.0.2.2 gateway) can reach it.
    let router = ZenohRouter::start_on("0.0.0.0", C_RISCV_NUTTX_TALKER_PORT).unwrap_or_else(|e| {
        nros_tests::unmet!("zenohd failed to start on {C_RISCV_NUTTX_TALKER_PORT}: {e}")
    });
    let _ = &router;

    // Observer first, so its subscription is live before the guest publishes.
    let mut obs = {
        let mut cmd = Command::new(&listener);
        cmd.env(
            "NROS_LOCATOR",
            format!("tcp/127.0.0.1:{C_RISCV_NUTTX_TALKER_PORT}"),
        )
        .env("RUST_LOG", "info");
        ManagedProcess::spawn_command(cmd, "native-listener")
            .unwrap_or_else(|e| panic!("spawn native listener: {e}"))
    };
    obs.wait_for_output_pattern(
        nros_tests::output::LISTENER_READY_MARKER,
        Duration::from_secs(10),
    )
    .unwrap_or_else(|_| {
        obs.kill();
        panic!("native listener never became ready")
    });

    // Boot the rv-virt kernel image (runs until killed).
    let mut qemu = QemuProcess::start_nuttx_riscv(&talker, true)
        .unwrap_or_else(|e| panic!("boot NuttX rv-virt QEMU: {e}"));

    let out = obs
        .wait_for_output_count(
            nros_tests::output::LISTENER_LOG_PREFIX,
            3,
            Duration::from_secs(90),
        )
        .unwrap_or_else(|_| {
            qemu.kill();
            obs.kill();
            panic!(
                "native listener never received the riscv-nuttx C talker's /chatter — \
                 the riscv C-lane runtime delivery did not work (archived 0199 fixed the \
                 link; this is the runtime half)"
            )
        });

    qemu.kill();
    obs.kill();

    let n = nros_tests::count_pattern(&out, nros_tests::output::LISTENER_LOG_PREFIX);
    assert!(n >= 3, "expected ≥3 cross-process deliveries, got {n}");
}

/// RFC-0051 §7c (phase-510 W3) — the cases this file runs are exactly the
/// Runtime cells `matrix::consumer_of` assigns to it. Fixture-free: a cell
/// added, retiered or reassigned without this list moving is a red here.
#[test]
fn cases_bound_to_matrix_cells() {
    use nros_tests::matrix::{
        Consumer, Kind as K, Lang as L, PlatformId as P, Rmw as R, Workload as W,
        assert_consumer_cases,
    };
    assert_consumer_cases(
        Consumer::CRiscvNuttx,
        &[(P::NuttxRiscv, L::C, R::Zenoh, W::Pubsub, K::Example)],
    );
}

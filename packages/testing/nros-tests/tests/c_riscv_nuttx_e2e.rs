//! #199 follow-up — the FIRST riscv-nuttx (rv-virt) **C-lane RUNTIME** e2e:
//! the standalone `examples/qemu-riscv-nuttx/c/talker` kernel image (the very
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
//! The fixture is built by `just nuttx build-riscv-c`; this test skips cleanly
//! when the talker ELF / zenohd / qemu-system-riscv32 are absent.
//!
//! Run with: `cargo nextest run -p nros-tests --test c_riscv_nuttx_e2e`

use nros_tests::{
    alloc::port_of,
    fixtures::{
        ManagedProcess, QemuProcess, ZenohRouter, build_native_listener,
        build_nuttx_riscv_c_talker, require_zenohd,
    },
    matrix::{Lang, PlatformId, Workload},
};
use std::{process::Command, time::Duration};

/// The router port baked into the riscv C talker's locator — the allocator's
/// (nuttx-riscv, c, pubsub) number, matching the `qemu-riscv-nuttx/c/talker`
/// fixture row's `NROS_ENTRY_LOCATOR` bake.
const C_RISCV_NUTTX_TALKER_PORT: u16 = port_of(PlatformId::NuttxRiscv, Lang::C, Workload::Pubsub);

#[test]
fn c_riscv_nuttx_talker_delivers_cross_process() {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    if !nros_tests::esp32::is_qemu_riscv32_available() {
        nros_tests::skip!("qemu-system-riscv32 not found");
    }

    let talker = build_nuttx_riscv_c_talker()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("riscv-nuttx C talker not built: {e}"));
    let listener = build_native_listener()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("native listener fixture not built: {e}"));

    // Router on the baked port; listen on 0.0.0.0 so the slirp guest
    // (10.0.2.2 gateway) can reach it.
    let router = ZenohRouter::start_on("0.0.0.0", C_RISCV_NUTTX_TALKER_PORT).unwrap_or_else(|e| {
        nros_tests::skip!("zenohd failed to start on {C_RISCV_NUTTX_TALKER_PORT}: {e}")
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
    obs.wait_for_output_pattern("Waiting for", Duration::from_secs(10))
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

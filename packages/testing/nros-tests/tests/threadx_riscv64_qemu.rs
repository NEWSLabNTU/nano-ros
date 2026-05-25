//! ThreadX QEMU RISC-V 64-bit integration tests
//!
//! Tests that verify ThreadX QEMU RISC-V examples build and run on QEMU virt
//! machine with virtio-net networking. Examples use `riscv64gc-unknown-none-elf`
//! target with `no_std` + NetX Duo networking over virtio-net.
//!
//! The E2E test bodies live in `tests/rtos_e2e.rs` (parametrised over
//! platform × language × variant).
//!
//! Prerequisites:
//! - `THREADX_DIR` env var pointing to ThreadX source (e.g., `third-party/threadx/kernel`)
//! - `NETX_DIR` env var pointing to NetX Duo source (e.g., `third-party/threadx/netxduo`)
//! - `riscv64-unknown-elf-gcc` cross-compiler installed
//! - `qemu-system-riscv64` with virt machine support
//! - zenohd: `just build-zenohd`
//!
//! Run with: `just test-threadx-riscv64`
//! Or: `cargo nextest run -p nros-tests --test threadx_riscv64_qemu`

use std::time::Duration;

use nros_tests::fixtures::{
    QemuProcess, is_qemu_riscv64_available, is_zenohd_available, qemu_supports_dgram_unix,
    threadx_riscv64::{
        build_threadx_rv64_action_client, build_threadx_rv64_action_server,
        build_threadx_rv64_listener, build_threadx_rv64_service_client,
        build_threadx_rv64_service_server, build_threadx_rv64_talker, is_netx_available,
        is_riscv_gcc_available, is_threadx_available,
    },
};

// =============================================================================
// Prerequisite checks
// =============================================================================

/// Skip test if ThreadX RISC-V build prerequisites are not available
fn require_threadx_riscv64() -> bool {
    if !is_threadx_available() {
        eprintln!("Skipping test: THREADX_DIR not set or invalid");
        eprintln!("Run: just setup-threadx && source .envrc");
        return false;
    }
    if !is_netx_available() {
        eprintln!("Skipping test: NETX_DIR not set or invalid");
        eprintln!("Run: just setup-threadx && source .envrc");
        return false;
    }
    if !is_riscv_gcc_available() {
        eprintln!("Skipping test: riscv64-unknown-elf-gcc not found");
        eprintln!("Install: sudo apt install gcc-riscv64-unknown-elf");
        return false;
    }
    true
}

// =============================================================================
// Prerequisite detection tests (always run)
// =============================================================================

#[test]
fn test_threadx_riscv64_detection() {
    let threadx = is_threadx_available();
    let netx = is_netx_available();
    let riscv_gcc = is_riscv_gcc_available();
    let qemu_rv64 = is_qemu_riscv64_available();
    let zenohd = is_zenohd_available();
    eprintln!("ThreadX available: {}", threadx);
    eprintln!("NetX Duo available: {}", netx);
    eprintln!("riscv64-unknown-elf-gcc available: {}", riscv_gcc);
    eprintln!("QEMU RISC-V 64 available: {}", qemu_rv64);
    eprintln!("zenohd available: {}", zenohd);
}

// =============================================================================
// Build tests (require THREADX_DIR + NETX_DIR + riscv64-unknown-elf-gcc)
// =============================================================================

#[test]
fn test_threadx_riscv64_all_examples_build() {
    if !require_threadx_riscv64() {
        nros_tests::skip!("require_threadx_riscv64 check failed");
    }

    let results = [
        ("talker", build_threadx_rv64_talker()),
        ("listener", build_threadx_rv64_listener()),
        ("service-server", build_threadx_rv64_service_server()),
        ("service-client", build_threadx_rv64_service_client()),
        ("action-server", build_threadx_rv64_action_server()),
        ("action-client", build_threadx_rv64_action_client()),
    ];

    let mut all_ok = true;
    for (name, result) in &results {
        match result {
            Ok(path) => eprintln!("  OK: {} -> {}", name, path.display()),
            Err(e) => {
                eprintln!("  FAIL: {} -> {:?}", name, e);
                all_ok = false;
            }
        }
    }

    assert!(
        all_ok,
        "Not all ThreadX QEMU RISC-V examples built successfully"
    );
}

// =============================================================================
// CycloneDDS two-QEMU peer interop (Phase 177.26)
// =============================================================================

/// Two ThreadX RISC-V64 QEMU nodes running the CycloneDDS C talker and
/// listener, wired together over an AF_UNIX SOCK_DGRAM L2 tunnel (no slirp
/// isolation), exchange a `std_msgs/Int32` sample on `/chatter`.
///
/// This exercises the real cross-node RTPS path: SPDP multicast discovery
/// over NetX Duo + IGMP (Phase 177.26 flips the ThreadX Cyclone profile's
/// `AllowMulticast` from `false` to `spdp`), unicast RTPS data delivery,
/// and CDR decode on the subscriber.
///
/// Requires the CycloneDDS fixtures to be prebuilt:
///   just cyclonedds threadx-cross-probe
///   NROS_THREADX_RV64_CYCLONEDDS_FIXTURES=1 just threadx_riscv64 build-fixtures
///
/// Ignored: multicast discovery now works on the publisher side — the
/// ThreadX ddsrt port fixes (multicast byte-order join + multi-iovec
/// datagram `sendto`, cyclonedds fork `e8ce7315`) let the talker join the
/// SPDP group and publish without `conn_write` errors. The remaining
/// blocker is a *distinct, pre-existing* issue: the listener aborts at
/// `nros_executor_register_subscription -> -1` inside the nano-ros Rust
/// executor (arena/capacity), before the Cyclone subscriber is created —
/// orthogonal to multicast. Tracked as Phase 177.26. Run with `--ignored`
/// once the subscriber can be registered (and `e8ce7315` is on the pinned
/// cyclonedds commit).
#[test]
#[ignore = "Phase 177.26: listener register_subscription fails in the nano-ros executor (arena), pre-existing; multicast discovery TX is fixed"]
fn test_threadx_riscv64_cyclonedds_two_qemu_pubsub() {
    if !require_threadx_riscv64() {
        nros_tests::skip!("require_threadx_riscv64 check failed");
    }
    if !is_qemu_riscv64_available() {
        nros_tests::skip!("qemu-system-riscv64 not found");
    }
    if !qemu_supports_dgram_unix() {
        nros_tests::skip!(
            "qemu-system-riscv64 does not support `-netdev dgram` (needs QEMU >= 7.2)"
        );
    }

    let root = nros_tests::project_root();
    let talker_bin = root
        .join("examples/qemu-riscv64-threadx/c/talker/build-cyclonedds/riscv64_threadx_c_talker");
    let listener_bin = root.join(
        "examples/qemu-riscv64-threadx/c/listener/build-cyclonedds/riscv64_threadx_c_listener",
    );
    if !talker_bin.exists() || !listener_bin.exists() {
        nros_tests::skip!(
            "CycloneDDS ThreadX fixtures missing; build with: \
             just cyclonedds threadx-cross-probe && \
             NROS_THREADX_RV64_CYCLONEDDS_FIXTURES=1 just threadx_riscv64 build-fixtures"
        );
    }

    // AF_UNIX dgram socket pair (kept short to stay under sun_path's 108-byte
    // limit). Each QEMU binds its `local` path and sends to the peer's path.
    let sock_dir = root.join("tmp");
    std::fs::create_dir_all(&sock_dir).expect("create tmp dir");
    let sock_talker = sock_dir.join("tx_rv64_cyc_talker.sock");
    let sock_listener = sock_dir.join("tx_rv64_cyc_listener.sock");
    let _ = std::fs::remove_file(&sock_talker);
    let _ = std::fs::remove_file(&sock_listener);
    let sock_talker = sock_talker.to_str().expect("utf-8 socket path");
    let sock_listener = sock_listener.to_str().expect("utf-8 socket path");

    // MACs match each node's config.toml (talker 10.0.2.40/:56,
    // listener 10.0.2.41/:57) so QEMU's device MAC equals the NetX-assigned
    // address.
    const TALKER_MAC: &str = "52:54:00:12:34:56";
    const LISTENER_MAC: &str = "52:54:00:12:34:57";

    // Subscriber first so it has joined the SPDP multicast group before the
    // talker announces; SPDP re-announces periodically regardless.
    let mut listener = QemuProcess::start_riscv64_virt_dgram(
        &listener_bin,
        sock_listener,
        sock_talker,
        LISTENER_MAC,
    )
    .expect("start listener QEMU");
    let _talker =
        QemuProcess::start_riscv64_virt_dgram(&talker_bin, sock_talker, sock_listener, TALKER_MAC)
            .expect("start talker QEMU");

    // Wait for the listener to decode at least one sample. Discovery +
    // first delivery completes in a few seconds when it works; the generous
    // window covers SPDP retry cadence. `_talker` is dropped (and killed) at
    // end of scope.
    let result = listener.wait_for_output_pattern("Received:", Duration::from_secs(90));
    listener.kill();

    match result {
        Ok(output) => {
            assert!(
                output.contains("Received:"),
                "listener did not decode a sample.\n--- listener output ---\n{output}"
            );
        }
        Err(e) => {
            panic!("listener never received a CycloneDDS sample from the peer ThreadX node: {e:?}")
        }
    }
}

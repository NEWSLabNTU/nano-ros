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
    QemuProcess, is_qemu_riscv64_available, is_zenohd_available, qemu_riscv64_supports_dgram_unix,
    threadx_riscv64::{is_netx_available, is_riscv_gcc_available, is_threadx_available},
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
// (Phase 182.3) `test_threadx_riscv64_all_examples_build` removed — it rebuilt
// every ThreadX-RV64 example, which `build-all` / `build-test-fixtures` already
// do before `test-all` (the `_require-fixtures` preflight). The per-role
// binaries are consumed by the `rtos_e2e` Platform__ThreadxRiscv64 tests.
// =============================================================================

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
/// The CycloneDDS fixtures are built by default (Phase 203 decision):
///   just threadx_riscv64 build-fixtures   # or `just build-all`
/// (opt out with NROS_THREADX_RV64_CYCLONEDDS_FIXTURES=0).
///
/// Phase 177.26 — ThreadX↔ThreadX Cyclone RTPS works end-to-end. Two fixes
/// landed it: the cyclonedds ThreadX ddsrt port joins SPDP multicast with an
/// `INADDR_ANY` interface (NetX BSD `IP_ADD_MEMBERSHIP` byte-order, fork
/// `nano-ros`@`12b4af2c`), and the nano-ros Cyclone subscriber allocates its
/// RX take buffer from the ddsrt heap rather than libc (`std::calloc` returns
/// NULL on the unwired ThreadX libc heap; 177.26.RX.2). The earlier
/// `register_subscription -> -1` symptom closed under 177.28.
///
/// Transport: prefers `-netdev dgram` (QEMU ≥ 7.2 — point-to-point AF_UNIX
/// pair, CI-isolated). On older QEMU it falls back to `-netdev socket,mcast`
/// (shared host L2). Both put the two nodes on one link; reliable RTPS
/// retransmission covers any cross-process loss on the mcast path.
#[test]
fn test_threadx_riscv64_cyclonedds_two_qemu_pubsub() {
    if !require_threadx_riscv64() {
        nros_tests::skip!("require_threadx_riscv64 check failed");
    }
    if !is_qemu_riscv64_available() {
        nros_tests::skip!("qemu-system-riscv64 not found");
    }

    let root = nros_tests::project_root();
    let talker_bin = root.join("examples/qemu-riscv64-threadx/c/talker/build-cyclonedds/c_talker");
    let listener_bin =
        root.join("examples/qemu-riscv64-threadx/c/listener/build-cyclonedds/c_listener");
    if !talker_bin.exists() || !listener_bin.exists() {
        nros_tests::skip!(
            "CycloneDDS ThreadX fixtures missing; build with: \
             just threadx_riscv64 build-fixtures (or just build-all). They build \
             by default — was NROS_THREADX_RV64_CYCLONEDDS_FIXTURES=0 set?"
        );
    }

    // MACs match each node's config.toml (talker 10.0.2.40/:56,
    // listener 10.0.2.41/:57) so QEMU's device MAC equals the NetX-assigned
    // address.
    const TALKER_MAC: &str = "52:54:00:12:34:56";
    const LISTENER_MAC: &str = "52:54:00:12:34:57";

    // Subscriber first so it has joined the SPDP multicast group before the
    // talker announces (CLAUDE.md QEMU-test convention; SPDP re-announces
    // periodically regardless). Both transports place the pair on one L2 link.
    let (mut listener, _talker) = if qemu_riscv64_supports_dgram_unix() {
        // AF_UNIX dgram pair (kept short to stay under sun_path's 108-byte
        // limit). Each QEMU binds its `local` path, sends to the peer's.
        let sock_dir = root.join("tmp");
        std::fs::create_dir_all(&sock_dir).expect("create tmp dir");
        let sock_talker = sock_dir.join("tx_rv64_cyc_talker.sock");
        let sock_listener = sock_dir.join("tx_rv64_cyc_listener.sock");
        let _ = std::fs::remove_file(&sock_talker);
        let _ = std::fs::remove_file(&sock_listener);
        let sock_talker = sock_talker.to_str().expect("utf-8 socket path");
        let sock_listener = sock_listener.to_str().expect("utf-8 socket path");

        let listener = QemuProcess::start_riscv64_virt_dgram(
            &listener_bin,
            sock_listener,
            sock_talker,
            LISTENER_MAC,
        )
        .expect("start listener QEMU (dgram)");
        std::thread::sleep(Duration::from_secs(4));
        let talker = QemuProcess::start_riscv64_virt_dgram(
            &talker_bin,
            sock_talker,
            sock_listener,
            TALKER_MAC,
        )
        .expect("start talker QEMU (dgram)");
        (listener, talker)
    } else {
        // QEMU < 7.2 fallback: shared `-netdev socket,mcast` segment. The
        // group is dedicated to this test so it can't cross-talk with other
        // platforms' mcast-socket harnesses (threadx tests run single-threaded
        // in their nextest group).
        const MCAST: &str = "230.0.0.7:11700";
        let listener = QemuProcess::start_riscv64_virt_mcast(&listener_bin, MCAST, LISTENER_MAC)
            .expect("start listener QEMU (socket,mcast)");
        std::thread::sleep(Duration::from_secs(4));
        let talker = QemuProcess::start_riscv64_virt_mcast(&talker_bin, MCAST, TALKER_MAC)
            .expect("start talker QEMU (socket,mcast)");
        (listener, talker)
    };

    // Wait for the listener to decode at least one sample. Discovery +
    // first delivery completes in a few seconds when it works; the generous
    // window covers SPDP retry cadence. `_talker` is dropped (and killed) at
    // end of scope.
    let result = listener.wait_for_output_pattern(
        nros_tests::output::LISTENER_LOG_PREFIX,
        Duration::from_secs(90),
    );
    listener.kill();

    match result {
        Ok(output) => {
            nros_tests::output::assert_listener(&output, 1);
        }
        Err(e) => {
            panic!("listener never received a CycloneDDS sample from the peer ThreadX node: {e:?}")
        }
    }
}

// =============================================================================
// CycloneDDS two-QEMU peer interop — RUST examples (issue #214)
// =============================================================================

/// Two ThreadX RISC-V64 QEMU nodes running the RUST CycloneDDS talker and
/// listener exchange `std_msgs/String` on `/chatter` — the rust sibling of
/// `test_threadx_riscv64_cyclonedds_two_qemu_pubsub`. Closes the #214 gap:
/// the rust cyclone fixtures previously had NO test consumer, the deploy-less
/// `run_app_thread(Config::default())` boot ran every image with the same
/// MAC/IP/domain (identity collapse), and the domain never matched the
/// fixture bake. `Config::default()` now applies the build-env identity
/// (`NROS_APP_NET_{IP,MAC}_LAST`, `NROS_DOMAIN_ID` — set per-example by
/// `nros_threadx_rv64_rust_cyclone_app`), so the pair boots as
/// 192.0.3.10/:56 (talker) + 192.0.3.11/:57 (listener) on the fixture domain.
///
/// The QEMU device MACs mirror the firmware bake (same convention as the C
/// test). Descriptors register via the board `.init_array` walk (#195/#205).
#[test]
fn test_threadx_riscv64_cyclonedds_two_qemu_rust_pubsub() {
    use nros_tests::fixtures::{Rmw, build_threadx_rv64_rust_example_rmw};

    if !require_threadx_riscv64() {
        nros_tests::skip!("require_threadx_riscv64 check failed");
    }
    if !is_qemu_riscv64_available() {
        nros_tests::skip!("qemu-system-riscv64 not found");
    }

    let talker_bin = build_threadx_rv64_rust_example_rmw(
        "talker",
        "riscv64_threadx_rust_talker_cyclonedds",
        Rmw::Cyclonedds,
    )
    .unwrap_or_else(|e| {
        nros_tests::skip!(
            "rust cyclone talker fixture missing (just threadx_riscv64 build-fixtures): {e:?}"
        )
    });
    let listener_bin = build_threadx_rv64_rust_example_rmw(
        "listener",
        "riscv64_threadx_rust_listener_cyclonedds",
        Rmw::Cyclonedds,
    )
    .unwrap_or_else(|e| {
        nros_tests::skip!(
            "rust cyclone listener fixture missing (just threadx_riscv64 build-fixtures): {e:?}"
        )
    });

    const TALKER_MAC: &str = "52:54:00:12:34:56";
    const LISTENER_MAC: &str = "52:54:00:12:34:57";

    // Subscriber first (SPDP re-announces cover the rest); both transports put
    // the two nodes on one L2 link — same shapes as the C sibling.
    let (mut listener, _talker) = if qemu_riscv64_supports_dgram_unix() {
        let root = nros_tests::project_root();
        let sock_dir = root.join("tmp");
        std::fs::create_dir_all(&sock_dir).expect("create tmp dir");
        let sock_talker = sock_dir.join("tx_rv64_cyc_rs_talker.sock");
        let sock_listener = sock_dir.join("tx_rv64_cyc_rs_listener.sock");
        let _ = std::fs::remove_file(&sock_talker);
        let _ = std::fs::remove_file(&sock_listener);
        let sock_talker = sock_talker.to_str().expect("utf-8 socket path");
        let sock_listener = sock_listener.to_str().expect("utf-8 socket path");

        let listener = QemuProcess::start_riscv64_virt_dgram(
            &listener_bin,
            sock_listener,
            sock_talker,
            LISTENER_MAC,
        )
        .expect("start listener QEMU (dgram)");
        std::thread::sleep(Duration::from_secs(4));
        let talker = QemuProcess::start_riscv64_virt_dgram(
            &talker_bin,
            sock_talker,
            sock_listener,
            TALKER_MAC,
        )
        .expect("start talker QEMU (dgram)");
        (listener, talker)
    } else {
        // Dedicated mcast group — distinct from the C test's so the two can
        // run concurrently within the serialized qemu group.
        const MCAST: &str = "230.0.0.7:11701";
        let listener = QemuProcess::start_riscv64_virt_mcast(&listener_bin, MCAST, LISTENER_MAC)
            .expect("start listener QEMU (socket,mcast)");
        std::thread::sleep(Duration::from_secs(4));
        let talker = QemuProcess::start_riscv64_virt_mcast(&talker_bin, MCAST, TALKER_MAC)
            .expect("start talker QEMU (socket,mcast)");
        (listener, talker)
    };

    let result = listener.wait_for_output_pattern(
        nros_tests::output::LISTENER_LOG_PREFIX,
        Duration::from_secs(90),
    );
    listener.kill();

    match result {
        Ok(output) => {
            nros_tests::output::assert_listener(&output, 1);
        }
        Err(e) => panic!(
            "rust listener never received a CycloneDDS sample from the peer ThreadX node: {e:?}"
        ),
    }
}

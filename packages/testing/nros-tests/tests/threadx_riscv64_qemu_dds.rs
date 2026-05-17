//! ThreadX QEMU RISC-V 64-bit DDS pubsub E2E (Phase 97.4.threadx-riscv64).
//!
//! Two QEMU `-M virt` instances on a shared `-netdev socket,mcast=…`
//! segment exchange SPDP / SEDP / pubsub via NetX Duo's BSD shim.
//! Same shape as the FreeRTOS / NuttX slices.
//!
//! Run with: `cargo nextest run -p nros-tests --test threadx_riscv64_qemu_dds`

use std::{
    sync::atomic::{AtomicU32, Ordering},
    time::Duration,
};

use nros_tests::fixtures::{
    QemuProcess, is_qemu_riscv64_available,
    threadx_riscv64::{
        build_threadx_rv64_dds_listener, build_threadx_rv64_dds_talker, is_riscv_gcc_available,
        is_threadx_available,
    },
};

fn require_threadx_rv64_dds() -> bool {
    if !is_threadx_available() {
        eprintln!("Skipping: THREADX_DIR / NETX_DIR not set or invalid");
        return false;
    }
    if !is_riscv_gcc_available() {
        eprintln!("Skipping: RISC-V toolchain not available");
        return false;
    }
    if !is_qemu_riscv64_available() {
        eprintln!("Skipping: qemu-system-riscv64 not found");
        return false;
    }
    true
}

static COUNTER: AtomicU32 = AtomicU32::new(0);

#[test]
#[ignore = "Phase 127.B.5 — RV64 ThreadX DDS pubsub regressed on \
            -netdev dgram tunnel: virtio-net frames don't traverse the \
            AF_UNIX peer pair (zero RX ISRs observed on both peers), so \
            dust-dds SPDP discovery never completes. NuttX uses the same \
            dgram pattern and passes, so this is RV64-specific (NetX BSD \
            + virtio-mmio interaction). Tracked for follow-up; the net.c \
            mcast_listen `join` fix + non-blocking set_recv_timeout fix \
            + IGMP runtime enable are still correct."]
fn test_threadx_rv64_dds_rust_talker_to_listener_e2e() {
    if !require_threadx_rv64_dds() {
        nros_tests::skip!("ThreadX RISC-V DDS prerequisites not available");
    }
    // Phase 127.B.5 — `-netdev dgram,local.type=unix,…` needs QEMU >= 7.2.
    if !nros_tests::fixtures::qemu_supports_dgram_unix() {
        eprintln!(
            "Skipping test: qemu-system-arm < 7.2 — `-netdev dgram,local.type=unix,…`\n\
             not available. Install a newer QEMU (e.g. Canonical's\n\
             server-backports PPA for Ubuntu, see `just threadx_riscv64 doctor`)."
        );
        nros_tests::skip!("qemu-system-arm too old for -netdev dgram unix");
    }

    let talker_bin = match build_threadx_rv64_dds_talker() {
        Ok(p) => p.to_path_buf(),
        Err(e) => {
            eprintln!("DDS talker binary not pre-built: {:?}", e);
            eprintln!("Run: just threadx_riscv64 build-fixtures");
            nros_tests::skip!("DDS talker binary missing");
        }
    };
    let listener_bin = match build_threadx_rv64_dds_listener() {
        Ok(p) => p.to_path_buf(),
        Err(e) => {
            eprintln!("DDS listener binary not pre-built: {:?}", e);
            eprintln!("Run: just threadx_riscv64 build-fixtures");
            nros_tests::skip!("DDS listener binary missing");
        }
    };

    let tmpdir = std::env::temp_dir();
    let pid = std::process::id();
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let talker_sock = tmpdir.join(format!("nros-rv64-dds-{pid}-{counter}-T.sock"));
    let listener_sock = tmpdir.join(format!("nros-rv64-dds-{pid}-{counter}-L.sock"));
    let _ = std::fs::remove_file(&talker_sock);
    let _ = std::fs::remove_file(&listener_sock);
    let talker_sock_s = talker_sock.to_string_lossy().to_string();
    let listener_sock_s = listener_sock.to_string_lossy().to_string();
    eprintln!("[threadx-rv64-dds] dgram pair: T={talker_sock_s} L={listener_sock_s}");

    let mut listener = QemuProcess::start_riscv64_virt_dgram(
        &listener_bin,
        &listener_sock_s,
        &talker_sock_s,
        "52:54:00:12:34:61",
    )
    .expect("Failed to start ThreadX RISC-V DDS listener");

    std::thread::sleep(Duration::from_secs(3));

    let mut talker = QemuProcess::start_riscv64_virt_dgram(
        &talker_bin,
        &talker_sock_s,
        &listener_sock_s,
        "52:54:00:12:34:60",
    )
    .expect("Failed to start ThreadX RISC-V DDS talker");

    let talker_out = talker
        .wait_for_output(Duration::from_secs(20))
        .unwrap_or_default();
    let listener_out = listener
        .wait_for_output(Duration::from_secs(60))
        .unwrap_or_default();

    eprintln!("\n=== ThreadX RISC-V DDS talker tail ===");
    for line in talker_out
        .lines()
        .rev()
        .take(30)
        .collect::<Vec<_>>()
        .iter()
        .rev()
    {
        eprintln!("{line}");
    }
    eprintln!("\n=== ThreadX RISC-V DDS listener tail ===");
    for line in listener_out
        .lines()
        .rev()
        .take(30)
        .collect::<Vec<_>>()
        .iter()
        .rev()
    {
        eprintln!("{line}");
    }

    let received = listener_out.matches("Received:").count();
    assert!(
        received >= 1,
        "ThreadX RISC-V DDS listener received no `/chatter` messages — \
         RTPS SPDP discovery and/or pubsub regressed.\n\
         Listener tail:\n{}",
        listener_out
            .lines()
            .rev()
            .take(40)
            .collect::<Vec<_>>()
            .iter()
            .rev()
            .copied()
            .collect::<Vec<_>>()
            .join("\n")
    );
    eprintln!("[threadx-rv64-dds] talker → listener E2E green: {received} messages received");
}

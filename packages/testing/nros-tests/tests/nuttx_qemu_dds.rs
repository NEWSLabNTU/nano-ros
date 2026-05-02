//! NuttX QEMU virt DDS pubsub E2E (Phase 97.4.nuttx).
//!
//! Two QEMU instances with `qemu-system-arm -M virt -cpu cortex-a7`
//! launched on a shared `-netdev socket,mcast=…` segment. Talker
//! publishes `std_msgs/Int32` on `/chatter`, listener subscribes,
//! both discover each other via SPDP on `239.255.0.1:7400`. Same
//! shape as the FreeRTOS DDS test, just on NuttX's POSIX socket
//! stack and virtio-net-device backend.
//!
//! Run with: `cargo nextest run -p nros-tests --test nuttx_qemu_dds`

use std::{
    sync::atomic::{AtomicU32, Ordering},
    time::Duration,
};

use nros_tests::fixtures::{
    QemuProcess, is_qemu_available,
    nuttx::{
        build_nuttx_dds_listener, build_nuttx_dds_talker, is_nuttx_available, is_nuttx_configured,
        is_nuttx_toolchain_available,
    },
};

fn require_nuttx_dds() -> bool {
    if !is_nuttx_available() {
        eprintln!("Skipping: NUTTX_DIR not set or invalid");
        return false;
    }
    if !is_nuttx_configured() {
        eprintln!("Skipping: NuttX kernel not configured (run: just nuttx setup)");
        return false;
    }
    if !is_nuttx_toolchain_available() {
        eprintln!("Skipping: NuttX nightly toolchain not available");
        return false;
    }
    if !is_qemu_available() {
        eprintln!("Skipping: qemu-system-arm not found");
        return false;
    }
    true
}

/// Per-test mcast group + port — same convention as the FreeRTOS test
/// (last octet rotates over PID + counter, port over the high bits).
fn pick_mcast_addr_port() -> String {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let pid = std::process::id();
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let last = ((pid ^ n) & 0xff) as u8;
    let port = 17000 + (((pid ^ n) >> 8) & 0x3fff) as u16;
    format!("230.10.0.{last}:{port}")
}

#[test]
fn test_nuttx_dds_rust_talker_to_listener_e2e() {
    if !require_nuttx_dds() {
        nros_tests::skip!("NuttX DDS prerequisites not available");
    }

    let talker_bin = match build_nuttx_dds_talker() {
        Ok(p) => p.to_path_buf(),
        Err(e) => {
            eprintln!("DDS talker binary not pre-built: {:?}", e);
            eprintln!("Run: just nuttx build-fixtures");
            nros_tests::skip!("DDS talker binary missing");
        }
    };
    let listener_bin = match build_nuttx_dds_listener() {
        Ok(p) => p.to_path_buf(),
        Err(e) => {
            eprintln!("DDS listener binary not pre-built: {:?}", e);
            eprintln!("Run: just nuttx build-fixtures");
            nros_tests::skip!("DDS listener binary missing");
        }
    };

    let mcast = pick_mcast_addr_port();
    eprintln!("[nuttx-dds] mcast group/port = {mcast}");

    // Listener first (subscribes before talker publishes), then a
    // brief stabilisation window for SPDP discovery, then talker.
    let mut listener =
        QemuProcess::start_nuttx_virt_mcast(&listener_bin, &mcast, "52:54:00:12:34:71")
            .expect("Failed to start NuttX DDS listener");

    std::thread::sleep(Duration::from_secs(3));

    let mut talker = QemuProcess::start_nuttx_virt_mcast(&talker_bin, &mcast, "52:54:00:12:34:70")
        .expect("Failed to start NuttX DDS talker");

    let talker_out = talker
        .wait_for_output(Duration::from_secs(20))
        .unwrap_or_default();
    let listener_out = listener
        .wait_for_output(Duration::from_secs(60))
        .unwrap_or_default();

    eprintln!("\n=== NuttX DDS talker tail ===");
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
    eprintln!("\n=== NuttX DDS listener tail ===");
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
        "NuttX DDS listener received no `/chatter` messages — RTPS \
         SPDP discovery and/or pubsub regressed.\n\
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
    eprintln!("[nuttx-dds] talker → listener E2E green: {received} messages received");
}

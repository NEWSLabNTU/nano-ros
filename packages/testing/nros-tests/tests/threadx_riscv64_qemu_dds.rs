//! ThreadX QEMU RISC-V 64-bit DDS pubsub E2E (Phase 97.4.threadx-riscv64).
//!
//! Two QEMU `-M virt` instances on a shared `-netdev socket,mcast=…`
//! segment exchange SPDP / SEDP / pubsub via NetX Duo's BSD shim.
//! Same shape as the FreeRTOS / NuttX slices.
//!
//! Run with: `cargo nextest run -p nros-tests --test threadx_riscv64_qemu_dds`

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use nros_tests::fixtures::QemuProcess;
use nros_tests::fixtures::is_qemu_riscv64_available;
use nros_tests::fixtures::threadx_riscv64::{
    build_threadx_rv64_dds_listener, build_threadx_rv64_dds_talker, is_riscv_gcc_available,
    is_threadx_available,
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

fn pick_mcast_addr_port() -> String {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let pid = std::process::id();
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let last = ((pid ^ n) & 0xff) as u8;
    let port = 17000 + (((pid ^ n) >> 8) & 0x3fff) as u16;
    format!("230.10.0.{last}:{port}")
}

#[test]
fn test_threadx_rv64_dds_rust_talker_to_listener_e2e() {
    if !require_threadx_rv64_dds() {
        nros_tests::skip!("ThreadX RISC-V DDS prerequisites not available");
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

    let mcast = pick_mcast_addr_port();
    eprintln!("[threadx-rv64-dds] mcast group/port = {mcast}");

    let mut listener = QemuProcess::start_riscv64_virt_mcast(
        &listener_bin,
        &mcast,
        "52:54:00:12:34:61",
    )
    .expect("Failed to start ThreadX RISC-V DDS listener");

    std::thread::sleep(Duration::from_secs(3));

    let mut talker = QemuProcess::start_riscv64_virt_mcast(
        &talker_bin,
        &mcast,
        "52:54:00:12:34:60",
    )
    .expect("Failed to start ThreadX RISC-V DDS talker");

    let talker_out = talker.wait_for_output(Duration::from_secs(20)).unwrap_or_default();
    let listener_out = listener
        .wait_for_output(Duration::from_secs(60))
        .unwrap_or_default();

    eprintln!("\n=== ThreadX RISC-V DDS talker tail ===");
    for line in talker_out.lines().rev().take(30).collect::<Vec<_>>().iter().rev() {
        eprintln!("{line}");
    }
    eprintln!("\n=== ThreadX RISC-V DDS listener tail ===");
    for line in listener_out.lines().rev().take(30).collect::<Vec<_>>().iter().rev() {
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
    eprintln!(
        "[threadx-rv64-dds] talker → listener E2E green: {received} messages received"
    );
}

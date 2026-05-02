//! ESP32-C3 QEMU DDS pubsub E2E (Phase 97.4.esp32-qemu / Phase 101.7).
//!
//! Two QEMU instances launched with `qemu-system-riscv32 -M esp32c3
//! -nic socket,model=open_eth,mcast=…` share a virtual L2 segment on
//! the host. The talker publishes `std_msgs/Int32` on `/chatter`,
//! the listener subscribes and logs `Received: <N>` lines. SPDP /
//! SEDP discovery flows over the same mcast segment.
//!
//! Same shape as the FreeRTOS / NuttX / ThreadX-RV64 / bare-metal
//! MPS2-AN385 slices, just on RISC-V `imc` ESP32-C3 + OpenETH +
//! smoltcp + the cooperative `nostd-runtime` dust-dds path with
//! `portable-atomic` Arc substitution (Phase 101.4).
//!
//! Run with: `cargo nextest run -p nros-tests --test esp32_qemu_dds`

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use nros_tests::esp32::{require_qemu_riscv32, require_riscv32_target, start_esp32_qemu_mcast};
use nros_tests::fixtures::{
    build_esp32_qemu_dds_listener_flash, build_esp32_qemu_dds_talker_flash,
};

fn require_esp32_dds() -> bool {
    require_qemu_riscv32() && require_riscv32_target()
}

fn pick_mcast_addr_port() -> String {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let pid = std::process::id();
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let last = ((pid ^ n) & 0xff) as u8;
    let port = 17000 + (((pid ^ n) >> 8) & 0x3fff) as u16;
    format!("230.10.4.{last}:{port}")
}

/// **#[ignore]d** — Phase 101.7 runtime blocker: `dust-dds`'s
/// `DcpsDomainParticipant::new` exhausts the 192 KiB DDS heap
/// (the largest static carve-out that fits alongside `.bss` +
/// `.stack` in ESP32-C3's 400 KiB DRAM). Both peers boot but
/// panic at `library/alloc/src/alloc.rs:573:9`
/// (`handle_alloc_error`) before reaching publish/subscribe.
///
/// The build-time path is fully unblocked (Phase 101.{2,3,4,5}
/// all green): `cargo build -p esp32-qemu-dds-{talker,listener}
/// --release` produces flashable images that contain the full
/// dust-dds + nros-rmw-dds + portable-atomic stack. The remaining
/// blocker is purely heap budget, not toolchain or compile-time
/// gating.
///
/// To re-enable: trim dust-dds's `DcpsDomainParticipant` builtin
/// entity count (~13 actor mailboxes today) or move some
/// allocations to a separately-sized SPI-RAM region. Both are
/// out of scope for Phase 101.
#[test]
#[ignore]
fn test_esp32_qemu_dds_rust_talker_to_listener_e2e() {
    if !require_esp32_dds() {
        nros_tests::skip!("ESP32-C3 QEMU DDS prerequisites not available");
    }

    let talker_bin = match build_esp32_qemu_dds_talker_flash() {
        Ok(p) => p.to_path_buf(),
        Err(e) => {
            eprintln!("DDS talker flash image not built: {:?}", e);
            eprintln!(
                "Build the ELF + flash image inline; the fixture handles it. \
                 If this fails, espflash + the pinned nightly toolchain are needed."
            );
            nros_tests::skip!("DDS talker flash image missing");
        }
    };
    let listener_bin = match build_esp32_qemu_dds_listener_flash() {
        Ok(p) => p.to_path_buf(),
        Err(e) => {
            eprintln!("DDS listener flash image not built: {:?}", e);
            nros_tests::skip!("DDS listener flash image missing");
        }
    };

    let mcast = pick_mcast_addr_port();
    eprintln!("[esp32-dds] mcast group/port = {mcast}");

    // Listener first (subscribe before publishes), then a brief
    // discovery window, then talker.
    let mut listener = start_esp32_qemu_mcast(&listener_bin, &mcast, "02:00:00:00:00:01")
        .expect("Failed to start ESP32 DDS listener");

    std::thread::sleep(Duration::from_secs(3));

    let mut talker = start_esp32_qemu_mcast(&talker_bin, &mcast, "02:00:00:00:00:00")
        .expect("Failed to start ESP32 DDS talker");

    let _talker_out = talker
        .wait_for_output(Duration::from_secs(20))
        .unwrap_or_default();
    let listener_out = listener
        .wait_for_output(Duration::from_secs(60))
        .unwrap_or_default();

    eprintln!("\n=== ESP32 DDS listener tail ===");
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
        "ESP32 DDS listener received no `/chatter` messages — \
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
    eprintln!("[esp32-dds] talker → listener E2E green: {received} messages received");
}

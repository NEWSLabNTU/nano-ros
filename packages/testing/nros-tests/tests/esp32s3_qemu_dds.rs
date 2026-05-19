//! ESP32-S3 QEMU DDS pubsub E2E (Phase 117.5).
//!
//! Sibling of `esp32_qemu_dds.rs` on the Xtensa LX7 path. Two
//! `qemu-system-xtensa -M esp32s3` instances share a `-nic
//! socket,model=open_eth,mcast=…` segment; the talker publishes
//! `std_msgs/Int32` on `/chatter`, the listener subscribes and
//! logs `Received: <N>` lines. RTPS SPDP / SEDP discovery flows
//! over the same virtual L2 mcast segment that already works on
//! the C3 sibling.
//!
//! Phase 117's whole point is that ESP32-C3 (riscv32imc, 400 KiB
//! SRAM, no PSRAM) panics in `DcpsDomainParticipant::new ->
//! handle_alloc_error` because the largest static heap that fits
//! is ~192 KiB; ESP32-S3 (Xtensa LX7, 512 KiB SRAM, 8-16 MiB
//! octal PSRAM) has the headroom. This test runs the full RTPS
//! protocol surface (builtin entities NOT trimmed) and asserts
//! ≥1 `/chatter` message reaches the listener — same bar as
//! every other QEMU DDS slice in the project.
//!
//! Run with: `cargo nextest run -p nros-tests --test esp32s3_qemu_dds`
//!
//! Preconditions: see `nros_tests::esp32s3::require_qemu_xtensa` +
//! `require_xtensa_esp32s3_target`. Skips cleanly when either the
//! Espressif `qemu-system-xtensa` fork (with `esp32s3` machine
//! model) or the `+esp` rustc toolchain (espup) is missing.
//!
//! Phase 117.2b status note (heap routing):
//! The platform crate's `dds-heap` is currently a 192 KiB
//! internal-SRAM static (transitional default — matches the C3
//! cap). The PSRAM-backed 1 MiB shape via `#[link_section =
//! ".ext_ram.bss"]` + `esp_hal::psram::init` is gated on Phase
//! 117.2b. Until that lands, this E2E behaves identically to the
//! `#[ignore]`'d C3 sibling: build succeeds, but
//! `DcpsDomainParticipant::new` may still `handle_alloc_error` at
//! runtime if RTPS state exceeds the 192 KiB carve-out. Marked
//! `#[ignore]` for the same reason — promote to default once
//! 117.2b lands.

use std::{
    sync::atomic::{AtomicU32, Ordering},
    time::Duration,
};

use nros_tests::{
    esp32s3::{require_qemu_xtensa, require_xtensa_esp32s3_target, start_esp32s3_qemu_mcast},
    fixtures::{build_esp32s3_qemu_dds_listener_flash, build_esp32s3_qemu_dds_talker_flash},
};

fn require_esp32s3_dds() -> bool {
    require_qemu_xtensa() && require_xtensa_esp32s3_target()
}

fn pick_mcast_addr_port() -> String {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let pid = std::process::id();
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let last = ((pid ^ n) & 0xff) as u8;
    // Port range 17000+ matches the C3 sibling; the two ranges
    // don't collide because each suite gets its own
    // `start_esp32{,s3}_qemu_mcast` net seg per QEMU process pair.
    let port = 17000 + (((pid ^ n) >> 8) & 0x3fff) as u16;
    format!("230.10.5.{last}:{port}")
}

/// Phase 117.2c runtime gate landed: esp-alloc multi-region heap
/// (96 KiB internal SRAM + ~8 MiB PSRAM) routed through one
/// `EspHeap` global allocator. dust-dds's `DcpsDomainParticipant`
/// + builtin actors no longer hit `handle_alloc_error` at the
/// 192 KiB cap.
///
/// Stays `#[ignore]`d until on-host QEMU run is verified —
/// pending the first `cargo nextest run -E
/// 'binary(esp32s3_qemu_dds)' --run-ignored=all` smoke. Promote
/// to default once it goes green and 117.2c's hardware-correctness
/// caveat (atomic-in-PSRAM) is either accepted as QEMU-only or
/// resolved via Allocator-API split.
#[test]
#[ignore]
fn test_esp32s3_qemu_dds_rust_talker_to_listener_e2e() {
    if !require_esp32s3_dds() {
        nros_tests::skip!("ESP32-S3 QEMU DDS prerequisites not available");
    }

    let talker_bin = match build_esp32s3_qemu_dds_talker_flash() {
        Ok(p) => p.to_path_buf(),
        Err(e) => {
            eprintln!("DDS talker flash image not built: {:?}", e);
            eprintln!(
                "Confirm `espup install --targets esp32s3` ran AND \
                 `. $HOME/export-esp.sh` was sourced in this shell."
            );
            nros_tests::skip!("DDS talker flash image missing");
        }
    };
    let listener_bin = match build_esp32s3_qemu_dds_listener_flash() {
        Ok(p) => p.to_path_buf(),
        Err(e) => {
            eprintln!("DDS listener flash image not built: {:?}", e);
            nros_tests::skip!("DDS listener flash image missing");
        }
    };

    let mcast = pick_mcast_addr_port();
    eprintln!("[esp32s3-dds] mcast group/port = {mcast}");

    // Listener first (subscribe before publishes), then a brief
    // discovery window, then talker.
    let mut listener = start_esp32s3_qemu_mcast(&listener_bin, &mcast, "02:00:00:00:00:11")
        .expect("Failed to start ESP32-S3 DDS listener");

    std::thread::sleep(Duration::from_secs(3));

    let mut talker = start_esp32s3_qemu_mcast(&talker_bin, &mcast, "02:00:00:00:00:10")
        .expect("Failed to start ESP32-S3 DDS talker");

    let _talker_out = talker
        .wait_for_output(Duration::from_secs(20))
        .unwrap_or_default();
    let listener_out = listener
        .wait_for_output(Duration::from_secs(60))
        .unwrap_or_default();

    eprintln!("\n=== ESP32-S3 DDS listener tail ===");
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
        "ESP32-S3 DDS listener received no `/chatter` messages — \
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
    eprintln!("[esp32s3-dds] talker → listener E2E green: {received} messages received");
}

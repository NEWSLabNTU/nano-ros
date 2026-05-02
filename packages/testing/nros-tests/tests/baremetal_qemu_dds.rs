//! Bare-metal QEMU MPS2-AN385 DDS pubsub E2E (Phase 97.3.mps2-an385).
//!
//! Two QEMU instances launched with `-nic socket,mcast=…` share a
//! virtual L2 segment on the host. The talker publishes
//! `std_msgs/Int32` on `/chatter`, the listener subscribes and logs
//! `Received: <N>` lines. SPDP / SEDP discovery flows over the same
//! mcast segment.
//!
//! Same shape as the FreeRTOS / NuttX / ThreadX-RV64 slices, just on
//! Cortex-M3 + bare-metal `nros-platform-mps2-an385` + `lan9118-smoltcp`
//! + the cooperative `nostd-runtime` dust-dds path.
//!
//! Run with: `cargo nextest run -p nros-tests --test baremetal_qemu_dds`

use std::{
    sync::atomic::{AtomicU32, Ordering},
    time::Duration,
};

use nros_tests::fixtures::{
    QemuProcess, build_qemu_baremetal_dds_listener, build_qemu_baremetal_dds_talker,
    is_qemu_available,
};

fn require_baremetal_dds() -> bool {
    if !is_qemu_available() {
        eprintln!("Skipping: qemu-system-arm not found");
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
    format!("230.10.2.{last}:{port}")
}

#[test]
fn test_baremetal_dds_rust_talker_to_listener_e2e() {
    if !require_baremetal_dds() {
        nros_tests::skip!("bare-metal DDS prerequisites not available");
    }

    let talker_bin = match build_qemu_baremetal_dds_talker() {
        Ok(p) => p.to_path_buf(),
        Err(e) => {
            eprintln!("DDS talker binary not pre-built: {:?}", e);
            eprintln!(
                "Run: cargo build --release --manifest-path \
                 examples/qemu-arm-baremetal/rust/dds/talker/Cargo.toml"
            );
            nros_tests::skip!("DDS talker binary missing");
        }
    };
    let listener_bin = match build_qemu_baremetal_dds_listener() {
        Ok(p) => p.to_path_buf(),
        Err(e) => {
            eprintln!("DDS listener binary not pre-built: {:?}", e);
            eprintln!(
                "Run: cargo build --release --manifest-path \
                 examples/qemu-arm-baremetal/rust/dds/listener/Cargo.toml"
            );
            nros_tests::skip!("DDS listener binary missing");
        }
    };

    let mcast = pick_mcast_addr_port();
    eprintln!("[baremetal-dds] mcast group/port = {mcast}");

    let mut listener =
        QemuProcess::start_mps2_an385_mcast(&listener_bin, &mcast, "02:00:00:00:00:01")
            .expect("Failed to start bare-metal DDS listener");

    std::thread::sleep(Duration::from_secs(3));

    let mut talker = QemuProcess::start_mps2_an385_mcast(&talker_bin, &mcast, "02:00:00:00:00:00")
        .expect("Failed to start bare-metal DDS talker");

    let _talker_out = talker
        .wait_for_output(Duration::from_secs(20))
        .unwrap_or_default();
    let listener_out = listener
        .wait_for_output(Duration::from_secs(60))
        .unwrap_or_default();

    eprintln!("\n=== bare-metal DDS listener tail ===");
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
        "bare-metal DDS listener received no `/chatter` messages — \
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
    eprintln!("[baremetal-dds] talker → listener E2E green: {received} messages received");
}

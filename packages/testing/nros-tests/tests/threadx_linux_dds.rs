//! ThreadX Linux DDS pubsub E2E (Phase 97.4.threadx-linux).
//!
//! Two ThreadX-Linux processes share the existing veth bridge
//! (`veth-tx0` / `veth-tx1` connected via `qemu-br`); SPDP / SEDP /
//! pubsub flow over `239.255.0.1:7400` on top of NetX Duo's BSD shim.
//!
//! Run with: `cargo nextest run -p nros-tests --test threadx_linux_dds`

use std::time::Duration;

use nros_tests::fixtures::is_veth_bridge_available;
use nros_tests::fixtures::threadx_linux::{
    build_threadx_dds_listener, build_threadx_dds_talker, is_nsos_netx_available,
    is_threadx_available,
};
use nros_tests::process::ManagedProcess;

fn require_threadx_dds() -> bool {
    if !is_threadx_available() {
        eprintln!("Skipping: THREADX_DIR not set or invalid");
        return false;
    }
    if !is_nsos_netx_available() {
        eprintln!("Skipping: nsos-netx driver not available");
        return false;
    }
    if !is_veth_bridge_available() {
        eprintln!("Skipping: veth bridge (qemu-br + veth-tx0 + veth-tx1) not configured");
        eprintln!("Run: just threadx_linux setup-bridge");
        return false;
    }
    true
}

#[test]
fn test_threadx_linux_dds_rust_talker_to_listener_e2e() {
    if !require_threadx_dds() {
        nros_tests::skip!("ThreadX-Linux DDS prerequisites not available");
    }

    let talker_bin = match build_threadx_dds_talker() {
        Ok(p) => p.to_path_buf(),
        Err(e) => {
            eprintln!("DDS talker binary not pre-built: {:?}", e);
            eprintln!("Run: just threadx_linux build-fixtures");
            nros_tests::skip!("DDS talker binary missing");
        }
    };
    let listener_bin = match build_threadx_dds_listener() {
        Ok(p) => p.to_path_buf(),
        Err(e) => {
            eprintln!("DDS listener binary not pre-built: {:?}", e);
            eprintln!("Run: just threadx_linux build-fixtures");
            nros_tests::skip!("DDS listener binary missing");
        }
    };

    let mut listener = ManagedProcess::spawn(&listener_bin, &[], "threadx-linux-dds-listener")
        .expect("Failed to spawn listener");

    std::thread::sleep(Duration::from_secs(3));

    let mut talker = ManagedProcess::spawn(&talker_bin, &[], "threadx-linux-dds-talker")
        .expect("Failed to spawn talker");

    // Phase 97.4.threadx-linux — SEDP on the cooperative single-thread
    // `nostd-runtime` is slow; the match closes 30+ seconds after
    // discovery. Give a generous window before declaring failure.
    let talker_out = talker
        .wait_for_output(Duration::from_secs(30))
        .unwrap_or_default();
    let listener_out = listener
        .wait_for_output(Duration::from_secs(90))
        .unwrap_or_default();

    eprintln!("\n=== ThreadX Linux DDS talker tail ===");
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
    eprintln!("\n=== ThreadX Linux DDS listener tail ===");
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
        "ThreadX Linux DDS listener received no `/chatter` messages — \
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
    eprintln!("[threadx-linux-dds] talker → listener E2E green: {received} messages received");
}

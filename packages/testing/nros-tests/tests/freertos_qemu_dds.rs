//! FreeRTOS QEMU MPS2-AN385 DDS pubsub E2E (Phase 97.4.freertos).
//!
//! Two QEMU instances launched with `-netdev socket,mcast=…` share
//! a virtual L2 segment on the host. The talker publishes
//! `std_msgs/Int32` on `/chatter`, the listener subscribes and logs
//! `Received: <N>` lines. SPDP / SEDP discovery flows over the same
//! mcast segment — same code path production DDS-on-FreeRTOS
//! deployments use.
//!
//! Mirrors the Zephyr A9 `test_zephyr_dds_rust_talker_to_listener_a9_e2e`
//! pattern from Phase 92, just on Cortex-M3 + MPS2-AN385 + lwIP.
//!
//! Prerequisites (test panics with skip message if missing):
//!   * `FREERTOS_DIR` + `LWIP_DIR` env vars
//!   * `arm-none-eabi-gcc` toolchain
//!   * `qemu-system-arm` with `mps2-an385` machine
//!
//! Run with: `cargo nextest run -p nros-tests --test freertos_qemu_dds`

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use nros_tests::fixtures::QemuProcess;
use nros_tests::fixtures::freertos::{
    build_freertos_dds_listener, build_freertos_dds_talker, is_arm_gcc_available,
    is_freertos_available, is_lwip_available,
};
use nros_tests::fixtures::is_qemu_available;

fn require_freertos_dds() -> bool {
    if !is_freertos_available() {
        eprintln!("Skipping: FREERTOS_DIR not set or invalid");
        return false;
    }
    if !is_lwip_available() {
        eprintln!("Skipping: LWIP_DIR not set or invalid");
        return false;
    }
    if !is_arm_gcc_available() {
        eprintln!("Skipping: arm-none-eabi-gcc not found");
        return false;
    }
    if !is_qemu_available() {
        eprintln!("Skipping: qemu-system-arm not found");
        return false;
    }
    true
}

/// Pick a unique mcast group + port per test invocation so concurrent
/// nextest runs don't bridge into each other. Uses a process-local
/// counter ratcheted off PID; still cheap on the kernel side because
/// QEMU's `-netdev socket,mcast=…` only joins the group while the
/// instance is alive.
fn pick_mcast_addr_port() -> String {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let pid = std::process::id();
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    // 230.0.0.0/8 is reserved for "private" mcast, won't collide with
    // RTPS's 239.255.0.1. Last octet rotates over PID + counter.
    let last = ((pid ^ n) & 0xff) as u8;
    let port = 17000 + (((pid ^ n) >> 8) & 0x3fff) as u16;
    format!("230.10.0.{last}:{port}")
}

/// **#[ignore]d**: Phase 97.4.freertos runtime bring-up incomplete.
/// Build, launch, and network-init paths all work — the listener
/// boots, initialises lwIP, and prints "Network ready". Beyond that
/// `Executor::open()` blocks before reaching
/// "Subscribing to /chatter", suggesting
/// `NrosUdpTransportFactory::create_participant` hangs on one of the
/// SPDP / SEDP socket binds (most likely `IP_ADD_MEMBERSHIP` via
/// lwIP `setsockopt` or the multicast metatraffic port bind).
///
/// Re-enable once the FreeRTOS runtime path matches the Zephyr A9
/// path that ships green. The infrastructure (board crate decoupling,
/// `critical_section::Impl`, `.ARM.extab` linker placement, IGMP-
/// enabled lwIP, mcast-socket QEMU launcher, talker / listener
/// crates, `just freertos build-fixtures` recipe entry, nros-tests
/// binary fixtures, nextest test-group routing) is all in place —
/// this test is the runtime smoke that gates flipping
/// 97.4.freertos from `[~]` to `[x]`.
#[test]
#[ignore]
fn test_freertos_dds_rust_talker_to_listener_e2e() {
    if !require_freertos_dds() {
        nros_tests::skip!("FreeRTOS DDS prerequisites not available");
    }

    let talker_bin = match build_freertos_dds_talker() {
        Ok(p) => p.to_path_buf(),
        Err(e) => {
            eprintln!("DDS talker binary not pre-built: {:?}", e);
            eprintln!("Run: just freertos build-fixtures");
            nros_tests::skip!("DDS talker binary missing");
        }
    };
    let listener_bin = match build_freertos_dds_listener() {
        Ok(p) => p.to_path_buf(),
        Err(e) => {
            eprintln!("DDS listener binary not pre-built: {:?}", e);
            eprintln!("Run: just freertos build-fixtures");
            nros_tests::skip!("DDS listener binary missing");
        }
    };

    let mcast = pick_mcast_addr_port();
    eprintln!("[freertos-dds] mcast group/port = {mcast}");

    // Listener first (subscribes before talker publishes), then a
    // brief stabilisation window for SPDP discovery, then talker.
    // Talker MAC matches its `config.toml` (02:00:00:00:00:00);
    // listener uses 02:00:00:00:00:01 — same convention as the
    // Zephyr A9 mcast pair.
    let mut listener =
        QemuProcess::start_mps2_an385_mcast(&listener_bin, &mcast, "02:00:00:00:00:01")
            .expect("Failed to start FreeRTOS DDS listener");

    std::thread::sleep(Duration::from_secs(3));

    let mut talker =
        QemuProcess::start_mps2_an385_mcast(&talker_bin, &mcast, "02:00:00:00:00:00")
            .expect("Failed to start FreeRTOS DDS talker");

    // Drain talker output for a window so it actually publishes some
    // messages before we assess the listener.
    let talker_out = talker.wait_for_output(Duration::from_secs(20)).unwrap_or_default();
    let listener_out = listener
        .wait_for_output(Duration::from_secs(60))
        .unwrap_or_default();

    eprintln!("\n=== FreeRTOS DDS talker tail ===");
    for line in talker_out.lines().rev().take(30).collect::<Vec<_>>().iter().rev() {
        eprintln!("{line}");
    }
    eprintln!("\n=== FreeRTOS DDS listener tail ===");
    for line in listener_out.lines().rev().take(30).collect::<Vec<_>>().iter().rev() {
        eprintln!("{line}");
    }

    let received = listener_out.matches("Received:").count();
    let cross_instance = listener_out.matches("src=10.0.2.20").count();
    let self_loopback = listener_out.matches("src=10.0.2.21").count();
    eprintln!(
        "[freertos-dds] listener saw cross-instance frames={} self-loopback frames={}",
        cross_instance, self_loopback
    );
    assert!(
        received >= 1,
        "FreeRTOS DDS listener received no `/chatter` messages — \
         RTPS SPDP discovery and/or pubsub regressed. \
         cross_instance={cross_instance} self_loopback={self_loopback}\n\
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
        "[freertos-dds] talker → listener E2E green: {received} messages received"
    );
}

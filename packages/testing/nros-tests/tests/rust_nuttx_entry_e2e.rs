//! Issue #130 (phase-280 W3) — runtime E2E for the Rust `nros::main!` NuttX
//! (QEMU arm-virt) Entry image. Proves commit `703e840dd`: the Rust entry path
//! (`<QemuArmVirt as nros_platform::BoardEntry>::run` → `entry_net_init`) pushes
//! the guest IP into `eth0` via `SIOCSIFADDR` before `Executor::open`, so the
//! guest reaches the host zenoh router through QEMU slirp instead of dying in
//! `Executor::open` with `Transport(ConnectionFailed)`.
//!
//! Before `703e840dd` this could not be proven at runtime: the six
//! `examples/qemu-arm-nuttx/rust/{role}_entry` demos were only BUILD-asserted
//! (`nuttx_entry_build.rs`), and the resolver-level staleness a bare
//! `cargo nextest` would need was absent. This is the C-entry sibling of
//! `c_nuttx_entry_e2e.rs`, but on the Rust entry path — the one `entry_net_init`
//! actually fixes.
//!
//! Delivery is observed CROSS-PROCESS: the QEMU guest runs the `talker_entry`
//! image (`nuttx_rs_talker`, publishes `std_msgs/Int32` on `/chatter`, baked
//! `NROS_LOCATOR = tcp/10.0.2.2:7452`, domain 0), and a SEPARATE native Rust
//! listener (`examples/native/rust/listener`, subscribes `/chatter` Int32, logs
//! `Received: N`) receives it through a host zenohd.
//! The guest dials the router via the slirp gateway (10.0.2.2 → host); the
//! observer dials 127.0.0.1. No TAP / bridge / root.
//!
//! The entry ELF is prebuilt by `just nuttx build-examples`; this test skips
//! cleanly when the ELF / zenohd / qemu are absent (never a bare
//! `eprintln!`+return — CLAUDE.md).
//!
//! Run with: `cargo nextest run -p nros-tests --test rust_nuttx_entry_e2e`

use nros_tests::fixtures::{
    ManagedProcess, QemuProcess, ZenohRouter, build_native_listener, is_qemu_available, nuttx,
    require_zenohd,
};
use std::{process::Command, time::Duration};

/// The router port baked into the standalone NuttX Rust entry demos'
/// `NROS_LOCATOR` (see `examples/fixtures.toml`: `tcp/10.0.2.2:7452`). The
/// embedded locator is fixed at build time — the guest cannot be redirected.
const NUTTX_ENTRY_PORT: u16 = 7452;

#[test]
fn rust_nuttx_entry_delivers_cross_process() {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    if !is_qemu_available() {
        nros_tests::skip!("qemu-system-arm not found");
    }

    // Prebuilt entry ELF (consume-only — no compilation in the test body). Absent
    // fixture → skip, exactly like the C entry sibling.
    let entry = nuttx::require_entry_binary("talker", "nuttx_rs_talker_entry")
        .unwrap_or_else(|e| nros_tests::skip!("NuttX Rust talker_entry not built: {e}"));
    let observer = build_native_listener()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("native Rust listener fixture not built: {e}"));

    // Router bound on 0.0.0.0 so the slirp guest (via host 10.0.2.2) reaches it;
    // the native observer dials the same port on loopback.
    let router = ZenohRouter::start_on("0.0.0.0", NUTTX_ENTRY_PORT)
        .unwrap_or_else(|e| nros_tests::skip!("zenohd failed to start on {NUTTX_ENTRY_PORT}: {e}"));
    let observer_locator = format!("tcp/127.0.0.1:{NUTTX_ENTRY_PORT}");
    let _ = router;

    // External observer (native listener) first, so its subscription is live
    // before the QEMU guest's talker publishes. `RUST_LOG=info` is required for
    // the listener's `info!("I heard: …")` line to surface.
    let mut obs = {
        let mut cmd = Command::new(&observer);
        cmd.env("RUST_LOG", "info")
            .env("NROS_LOCATOR", &observer_locator);
        ManagedProcess::spawn_command(cmd, "native-observer")
            .unwrap_or_else(|e| panic!("spawn observer: {e}"))
    };
    obs.wait_for_output_pattern("Waiting for", Duration::from_secs(10))
        .unwrap_or_else(|_| {
            obs.kill();
            panic!("native observer listener never became ready")
        });

    // Boot the NuttX entry image in QEMU with networking. The image runs until
    // killed (no bounded spin on the embedded target).
    let mut qemu = QemuProcess::start_nuttx_virt(&entry, true)
        .unwrap_or_else(|e| panic!("boot NuttX QEMU: {e}"));

    // The observer prints `Received: N` per delivered Int32 message — ≥3
    // confirms the guest's entry-path talker reached a separate process through
    // the router, i.e. `entry_net_init` configured eth0 and the connect
    // succeeded. NuttX cold boot + 5 s net warm-up + zenoh connect is slow, so
    // allow a generous window.
    let out = obs
        .wait_for_output_count(
            nros_tests::output::INT32_LISTENER_LOG_PREFIX,
            3,
            Duration::from_secs(90),
        )
        .unwrap_or_else(|_| {
            qemu.kill();
            obs.kill();
            panic!(
                "native observer never received the NuttX Rust entry image's /chatter — \
                 the entry-path eth0 config (#130 / entry_net_init) did not bring the guest \
                 onto the router"
            )
        });

    qemu.kill();
    obs.kill();

    let n = nros_tests::count_pattern(&out, nros_tests::output::INT32_LISTENER_LOG_PREFIX);
    assert!(n >= 3, "expected ≥3 cross-process deliveries, got {n}");
}

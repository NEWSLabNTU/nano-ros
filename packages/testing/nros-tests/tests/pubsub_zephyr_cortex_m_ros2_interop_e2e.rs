//! phase-441 W1 — the live-peer crossing, one BOARD over.
//!
//! ## What this cell is for
//!
//! `interop::CELLS` had exactly one non-Linux runnable row before this file:
//! `zephyr-qos-rust-zenoh` on `PlatformId::ZephyrNativeSim`. native_sim is
//! `native_sim/native/64` — `CONFIG_NET_SOCKETS_OFFLOAD=y`, so the sockets are
//! the HOST's, the libc is the host's and the pointers are the host's 64 bits.
//! That cell proves our zenoh-pico code talks to `rmw_zenoh_cpp`; it does not
//! prove it does so from a device, because no RTOS network stack is ever in the
//! path.
//!
//! This cell moves the one axis that changes: the image is a 32-bit Cortex-M3
//! (`mps2_an385`) running Zephyr's IN-KERNEL IP stack over the `eth_smsc911x`
//! driver against QEMU's lan9118, reaching the host through SLIRP. Same RMW
//! (zenoh-pico), same peer (a stock `rmw_zenoh_cpp` node), same direction
//! (nano → ROS), same crossing mechanism (a baked TCP locator to a router on
//! the host, no multicast discovery — SLIRP is unicast-only and TAP would need
//! `ip tuntap add`, i.e. root).
//!
//! ## Two assertions, and both are load-bearing
//!
//! 1. `IPv4 address: 10.0.2.15` — Zephyr's own `net_config` announcing the
//!    static address from `cmake/zephyr/mps2-an385.conf`. Only the in-kernel
//!    stack driving a real ethernet controller can print it, so it is the
//!    evidence that this coordinate is a second witness rather than a second
//!    spelling of native_sim. Its sibling `zephyr_cortex_m_qemu` asserts the
//!    same line for the same reason.
//! 2. `ros2 topic echo` receives `/chatter`. That is the interop claim, and the
//!    reason the first assertion alone would not do: the pubsub cell in
//!    `matrix::CELLS` asserts the image PRINTS `Publishing:`, which the C talker
//!    does whenever `nros_cpp_publish_raw` returns 0 — a local return code, not
//!    a delivery. Nothing in the tree observed a sample leaving this board until
//!    this test.
//!
//! ## Why C and not the Rust QoS entry the phase doc names
//!
//! Two reasons, one of them a correction. The correction: phase-441's W1 text
//! (and `matrix.rs`, and `binaries/mod.rs`) says Rust cannot build for this
//! board because of issue 0432. **0432 was RESOLVED 2026-08-12 by phase-346
//! W2/W3** and `zephyr_cortex_m_rust_zenoh_pubsub_e2e` has run the Rust leaf
//! since; the three stale statements are corrected in the same commit as this
//! file.
//!
//! The reason that survives is minimality. There is no C, C++ *or* Rust QoS
//! workspace entry for any board but `native_sim/native/64`
//! (`examples/workspaces/features/src/` has three zephyr entries, all
//! native_sim), so a QoS cell here means authoring an entry, a
//! `[[workspace_fixture]]` row, a west build name and a port bake — four new
//! things, none of them the axis W1 exists to move. The C talker leaf is
//! ALREADY built by the west lane at this exact coordinate
//! (`examples/fixtures.toml`, `build-cortex-m-c-talker-zenoh`, locator
//! `tcp/10.0.2.2:10700`), so this cell adds a peer and nothing else. The
//! workload axis moves from `Qos` to `Pubsub`; the board axis moves, which is
//! what the phase is about.
//!
//! ## Prerequisites
//!
//! - ROS 2 + `rmw_zenoh_cpp` (skips when absent — a host with no `/opt/ros`
//!   cannot run this cell at all)
//! - `just zephyr build-fixtures` (the west leaves lane builds
//!   `build-cortex-m-c-talker-zenoh`)
//! - `qemu-system-arm` with `mps2-an385` machine support
//!
//! Run with: `just zephyr test-ros2-cortex-m`, or
//! `cargo nextest run -p nros-tests --test pubsub_zephyr_cortex_m_ros2_interop_e2e`

use nros_tests::{
    alloc::port_of,
    fixtures::{QemuProcess, Rmw, ZenohRouter, build_zephyr_cortex_m_example, is_qemu_available},
    matrix::{Lang, PlatformId, Workload},
    ros2::{DEFAULT_ROS_DISTRO, require_ros2, ros2_env_setup_with_locator},
    skip,
};
use std::{
    process::Command,
    time::{Duration, Instant},
};

/// The router port baked into the C talker leaf — the allocator's
/// (zephyr-cortex-m, C, Pubsub) number, matching the west lane's
/// `west_zenoh_locator = "tcp/10.0.2.2:10700"` bake in `examples/fixtures.toml`.
/// Derived rather than spelled so a platform-index change moves the bake and
/// this test together instead of leaving one behind.
const CORTEX_M_C_TALKER_PORT: u16 =
    port_of(PlatformId::ZephyrQemuCortexM, Lang::C, Workload::Pubsub);

/// Zephyr's `net_config` announcing the static SLIRP address. See the module
/// docs: this is the distinguishing evidence of the board.
const NET_STACK_READY_MARKER: &str = "IPv4 address: 10.0.2.15";

/// How long to wait for the guest to reach "network ready". The board reaches
/// it at ~2.15 s; 60 s is generous headroom for a loaded host without becoming
/// the nextest per-test timeout.
const NET_READY_BUDGET: Duration = Duration::from_secs(60);

/// How long to poll `ros2 topic echo` for. Each attempt pays multi-second ros2
/// CLI startup, so this is a handful of attempts, not many.
const ECHO_BUDGET: Duration = Duration::from_secs(45);

#[test]
fn nros_zephyr_cortex_m_publisher_reaches_ros2_topic_echo() {
    if !require_ros2() {
        skip!(
            "ROS 2 / rmw_zenoh_cpp not available — install it from apt \
             (`ros-$ROS_DISTRO-rmw-zenoh-cpp`, declared in nros-sdk-index.toml). \
             This cell's peer IS a stock ROS 2 node; there is nothing to measure \
             without one."
        );
    }
    if !is_qemu_available() {
        skip!("qemu-system-arm not found — this cell boots an mps2_an385 guest");
    }

    // Not a `skip!` on error: since issue 0584 an absent IN-LANE fixture is a
    // hard failure, because the lane gate already promised it exists, and the
    // resolver raises its own `[SKIPPED]` for an out-of-lane coordinate. See
    // the same comment in `zephyr_cortex_m_qemu.rs` (issues 0806/0807) — a skip
    // here relabels STALE as missing and the cell silently stops running.
    let binary = build_zephyr_cortex_m_example("c", "talker", Rmw::Zenoh).unwrap_or_else(|e| {
        panic!(
            "zephyr/c/talker for mps2_an385 did not resolve. The lane gate already \
             asserted this fixture is built, so this is a real failure, not a setup \
             condition — read the verdict before rebuilding: {e:?}"
        )
    });

    // Bind 0.0.0.0, not loopback: the guest reaches the host through SLIRP's
    // 10.0.2.2 gateway and a loopback-only listener leaves those SYNs
    // unreachable. `locator()` still hands the HOST-side peer
    // `tcp/127.0.0.1:<port>`, which is the same router.
    let router = ZenohRouter::start_slirp(CORTEX_M_C_TALKER_PORT).unwrap_or_else(|e| {
        panic!("failed to start zenohd on {CORTEX_M_C_TALKER_PORT} for the SLIRP guest: {e:?}")
    });
    let locator = router.locator();

    let mut qemu = QemuProcess::start_mps2_an385_networked(&binary)
        .expect("spawn Zephyr Cortex-M zenoh talker");

    // Wait for the in-kernel stack before asking ROS 2 anything: until
    // `net_config` has assigned 10.0.2.15 the guest cannot have opened its TCP
    // session to the router, so every echo attempt before this point is spent
    // on a session that does not exist yet.
    let boot = qemu.collect_until(NET_STACK_READY_MARKER, NET_READY_BUDGET);
    assert!(
        boot.contains(NET_STACK_READY_MARKER),
        "Zephyr's in-kernel net stack never assigned the static address — the \
         eth_smsc911x driver did not come up, so nothing this cell measures had \
         a chance to happen.\nOutput:\n{boot}"
    );

    // Poll `ros2 topic echo --once` until a sample lands. Same shape as
    // `qos_zephyr_ros2_interop_e2e`: each attempt pays the ros2 CLI startup, so
    // budget generously and bail on the first success.
    let (env, _config_guard) = ros2_env_setup_with_locator(DEFAULT_ROS_DISTRO, &locator);
    let deadline = Instant::now() + ECHO_BUDGET;
    let mut last = String::new();
    let mut delivered = false;
    while Instant::now() < deadline {
        let script = format!(
            "{env} && timeout 12 ros2 topic echo --once /chatter std_msgs/msg/String \
             --no-daemon --spin-time 2 2>&1"
        );
        let out = Command::new("bash")
            .args(["-c", &script])
            .output()
            .expect("failed to spawn bash for ros2 invocation");
        last = String::from_utf8_lossy(&out.stdout).into_owned();
        if last.contains("data:") {
            delivered = true;
            break;
        }
    }

    // Read up to the guest's NEXT publish before killing it, so a failure
    // message carries the board's own transcript from the window ros2 was
    // polling in rather than only the boot. The talker's timer is 500 ms, so a
    // healthy image returns here immediately; an image that died mid-run
    // returns the whole 2 s window, which is itself the answer.
    let tail = qemu.collect_until(
        nros_tests::output::TALKER_LOG_PREFIX,
        Duration::from_secs(2),
    );
    qemu.kill();

    assert!(
        delivered,
        "`ros2 topic echo` (rmw_zenoh_cpp) never received /chatter from the Zephyr \
         Cortex-M talker. The board's IP stack came up (the assertion above passed), \
         so this is the WIRE, not the boot: a 32-bit guest publishing through \
         zenoh-pico over Zephyr's own TCP stack did not reach a stock ROS 2 \
         subscriber.\nrouter: {}\nlast ros2 output:\n{last}\nguest boot:\n{boot}\nguest tail:\n{tail}",
        router.launch_line()
    );
}

// Issue 0352 / phase-324 — bind this test to `interop::CELLS`. The coordinates
// below must equal what the list declares for
// `pubsub_zephyr_cortex_m_ros2_interop_e2e`; adding/retiring an interop cell for
// this test, or drifting a cell's coordinate (issue 0341 defect 2), turns this
// RED. Needs no fixtures — runs in tier 1.
#[test]
fn cases_bound_to_interop_cells() {
    #[allow(unused_imports)]
    use nros_tests::matrix::{Lang::*, PlatformId::*, Rmw::*, Workload::*};
    nros_tests::interop::assert_test_bound(
        "pubsub_zephyr_cortex_m_ros2_interop_e2e",
        &[(ZephyrQemuCortexM, C, Zenoh, Pubsub)],
    );
}

//! phase-441 W3 — a SECOND KERNEL meets a live ROS 2 peer.
//!
//! The tree's only other on-target live-peer cell is
//! `qos_zephyr_ros2_interop_e2e`, and phase-441's own analysis is why this file
//! exists: that cell runs on `native_sim/native/64`, where the sockets are the
//! HOST's (NSOS), the pointer width is the host's and the libc is the host's.
//! It proves our zenoh-pico code talks to `rmw_zenoh_cpp`; it does not prove it
//! does so from anything shaped like a device.
//!
//! This one moves five axes at once and keeps the crossing mechanism identical:
//!
//! | axis | zephyr cell | here |
//! | --- | --- | --- |
//! | kernel | Zephyr | **FreeRTOS** |
//! | IP stack | host (NSOS offload) | **lwIP over emulated LAN9118** |
//! | pointers | 64-bit host | **32-bit thumbv7m** |
//! | libc | host glibc | **newlib-nano** |
//! | API | Rust | **C** (`nros_cpp_publisher_create`) |
//!
//! ## How the guest reaches the peer, and why it is affordable
//!
//! `scripts/qemu/launch-mps2-an385.sh` offers slirp and TAP. TAP needs
//! `ip tuntap add` — root — and CLAUDE.md forbids `sudo` outright, so slirp is
//! the only path an agent or a CI runner can take. Slirp is **unicast only**:
//! the guest sits on 192.0.3.x and the gateway 192.0.3.1 forwards to the host
//! machine (`QemuProcess::start_mps2_an385_freertos_slirp` passes
//! `net=192.0.3.0/24,host=192.0.3.1` precisely so the board's static lwIP
//! config and QEMU's virtual net agree).
//!
//! Unicast-only is exactly what zenoh-pico client mode already does: the image
//! carries a COMPILE-TIME `NROS_ENTRY_LOCATOR` of `tcp/192.0.3.1:<port>` (baked
//! by the `workspace-c-freertos` row of `examples/fixtures.toml`) and dials it.
//! Nothing about this cell needs a multicast crossing — which is what makes it
//! the RMW to do first. Cyclone's SPDP would need a configured unicast peer
//! list plus a QEMU `hostfwd` this tree does not emit; that is issue 1251 and
//! phase-441 W2's measurement, deliberately not this cell's problem.
//!
//! The router is ROS's own `rmw_zenohd` (RFC-0075 — we ship none), started on
//! `0.0.0.0` so the slirp gateway can forward to it, while the ROS 2 peer dials
//! `tcp/127.0.0.1:<port>`. One router, two sides, no multicast anywhere.
//!
//! ## What it asserts
//!
//! The image's `demo_bringup` talker publishes a CDR `std_msgs/msg/Int32`
//! counter on `/chatter` at 1 Hz. A stock `ros2 topic echo` (rmw_zenoh_cpp,
//! multicast scouting off via the session config `ros2_env_setup_with_locator`
//! writes) must receive one. That is the nano-pub → ros2-sub direction, the same
//! direction issue #141 found dead against a healthy publisher on Zephyr.
//!
//! ## Preconditions, all of which SKIP
//!
//! ROS 2 + `rmw_zenoh_cpp`, the FreeRTOS kernel + lwIP trees, `arm-none-eabi-gcc`,
//! `qemu-system-arm`, and the `workspace-c-freertos` fixture
//! (`just freertos build-fixtures`). A host with no ROS skips — that is the
//! correct outcome, not a failure, and a bare `cargo nextest` renders the
//! `skip!` panic as FAILED (only `just test-all`'s junit rewrite converts it).
//!
//! Its baked router port is shared with `entry_e2e`'s freertos_c cell, so
//! `.config/nextest.toml` puts this binary in `matrix-consumers-serial` —
//! ABOVE the `binary(~freertos)` → `qemu-emulated` override, or that one claims
//! it first and the serialization silently does not happen (phase-373 W1's
//! defect, one binary over).
//!
//! Run with:
//! `just freertos test-ros2` (or
//! `cargo nextest run -p nros-tests --test pubsub_freertos_ros2_interop_e2e`)

use nros_tests::{
    alloc::port_of,
    fixtures::{
        QemuProcess, ZenohRouter, build_freertos_workspace_c_entry, freertos, is_qemu_available,
    },
    matrix::{Lang, PlatformId, Workload},
    ros2::{DEFAULT_ROS_DISTRO, require_ros2, ros2_env_setup_with_locator},
    skip,
};
use std::{
    process::Command,
    time::{Duration, Instant},
};

/// The router port baked into the FreeRTOS C workspace entry — the allocator's
/// `(freertos-mps2, c, entry-pubsub)` number, the SAME formula the fixture
/// baker uses for that row's `NROS_ENTRY_LOCATOR` cmake def. Never a literal:
/// the image and the router can then not disagree by hand.
const FREERTOS_C_ENTRY_PORT: u16 =
    port_of(PlatformId::FreertosMps2, Lang::C, Workload::EntryPubsub);

/// How long to keep asking `ros2 topic echo` for a sample. The guest is an
/// emulated Cortex-M3 doing a cold boot, an lwIP DHCP-less bring-up and a
/// zenoh-pico session handshake before it publishes anything, and each attempt
/// pays multi-second ros2 CLI startup on top; `entry_e2e`'s QEMU cells budget
/// 60–90 s for the same boot with a native observer.
const DELIVERY_WINDOW: Duration = Duration::from_secs(150);

#[test]
fn nros_freertos_mps2_publisher_reaches_ros2_topic_echo() {
    if !require_ros2() {
        skip!(
            "ROS 2 / rmw_zenoh_cpp not available — install it from apt \
             (`ros-$ROS_DISTRO-rmw-zenoh-cpp`, declared in nros-sdk-index.toml)."
        );
    }
    if !freertos::is_freertos_available() {
        skip!("FREERTOS_DIR not set or invalid — `just setup freertos`");
    }
    if !freertos::is_lwip_available() {
        skip!("LWIP_DIR not set or invalid — `just setup freertos`");
    }
    if !freertos::is_arm_gcc_available() {
        skip!("arm-none-eabi-gcc not found — `nros setup --tool arm-none-eabi-gcc`");
    }
    if !is_qemu_available() {
        skip!("qemu-system-arm not found");
    }

    let entry = build_freertos_workspace_c_entry()
        .unwrap_or_else(|e| skip!("freertos C workspace entry not built: {e}"));

    // 0.0.0.0, not 127.0.0.1: the guest reaches this router through the slirp
    // gateway 192.0.3.1, which is a DIFFERENT host address than loopback. The
    // ROS 2 peer below dials loopback for the same one router.
    let _router = ZenohRouter::start_on("0.0.0.0", FREERTOS_C_ENTRY_PORT)
        .unwrap_or_else(|e| skip!("zenohd failed to start on {FREERTOS_C_ENTRY_PORT}: {e}"));
    let peer_locator = format!("tcp/127.0.0.1:{FREERTOS_C_ENTRY_PORT}");

    let mut guest = QemuProcess::start_mps2_an385_freertos_slirp(&entry)
        .unwrap_or_else(|e| panic!("boot freertos QEMU (mps2-an385): {e}"));

    // Poll `ros2 topic echo --once` until a sample lands. Each attempt pays
    // multi-second ros2 CLI startup, so bail on the first success.
    let (env, _config_guard) = ros2_env_setup_with_locator(DEFAULT_ROS_DISTRO, &peer_locator);
    let deadline = Instant::now() + DELIVERY_WINDOW;
    let mut last = String::new();
    let mut delivered = false;
    while Instant::now() < deadline {
        let script = format!(
            "{env} && timeout 12 ros2 topic echo --once /chatter std_msgs/msg/Int32 \
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

    // The guest's own console is the only diagnosis available when this fails:
    // a board that never got a lwIP address, never opened the TCP session or
    // died in the kernel all look identical from the ROS side — an empty
    // `ros2 topic echo`. issue 0667's shape in particular, an undersized task
    // stack, lands as `*** MALLOC FAILED ***` rather than as a stack-overflow
    // message, because heap_4 hands out the stack. Collected only on the
    // failure path, and as a DRAIN rather than a wait: `collect_until` returns
    // whatever was printed whether or not its pattern appears, and renders a
    // harness fault INTO the text instead of dropping it. Defaulting away a
    // `wait_for_*` error does the opposite — it reports an empty console for a
    // guest that had plenty to say, which is issue 0670 and what
    // `check-wait-evidence-discarded` refuses. The sentinel is a string the
    // image cannot emit, so the call always runs its whole window and hands
    // back the entire console.
    const DRAIN_SENTINEL: &str = "\u{1}nros-drain-sentinel";
    let guest_console = if delivered {
        String::new()
    } else {
        let log = guest.collect_until(DRAIN_SENTINEL, Duration::from_secs(2));
        let note = nros_tests::output::runtime_silence_note(&log)
            .map(|n| format!("\n  {n}"))
            .unwrap_or_default();
        format!("{note}\n  --- guest output ---\n{log}\n  --- end guest output ---")
    };
    guest.kill();

    assert!(
        delivered,
        "`ros2 topic echo` (rmw_zenoh_cpp) never received the FreeRTOS/MPS2-AN385 \
         entry's 1 Hz `/chatter` publish within {}s — the nano-pub → ros2-sub \
         direction does not cross a second kernel (phase-441 W3).\n\
         router: tcp/0.0.0.0:{FREERTOS_C_ENTRY_PORT} (guest dials \
         tcp/192.0.3.1:{FREERTOS_C_ENTRY_PORT} through slirp; peer dials \
         {peer_locator})\n\
         last ros2 output:\n{last}\n\
         guest console:\n{guest_console}",
        DELIVERY_WINDOW.as_secs()
    );
}

// Issue 0352 / phase-324 — bind this test to `interop::CELLS`. The coordinates
// below must equal what the list declares for `pubsub_freertos_ros2_interop_e2e`;
// adding/retiring an interop cell for this test, or drifting a cell's coordinate
// (issue 0341 defect 2), turns this RED. Needs no fixtures — runs in tier 1.
#[test]
fn cases_bound_to_interop_cells() {
    #[allow(unused_imports)]
    use nros_tests::matrix::{Lang::*, PlatformId::*, Rmw::*, Workload::*};
    nros_tests::interop::assert_test_bound(
        "pubsub_freertos_ros2_interop_e2e",
        &[(FreertosMps2, C, Zenoh, EntryPubsub)],
    );
}

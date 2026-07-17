//! phase-295 W3.b — THE multihost runtime-delivery matrix consumer
//! (RFC-0051).
//!
//! Consolidates the 5 per-cell multihost runtime files
//! (`multihost_runtime_e2e`, `{c,cpp,mixed}_multihost_e2e`,
//! `multihost_zephyr_entry_e2e`) into one parametrized test over the
//! `Workload::Multihost` cells of the test matrix (`nros_tests::matrix`).
//! The SOURCE-level half of the story — `nros codegen entry --host robotN`
//! emits an entry registering only that host's nodes — stays a separate
//! codegen gate (`tests/multihost_partition_bake.rs`); this file proves the
//! RUNTIME half: the per-host entries, booted as two separate processes
//! (the multi-host topology), actually exchange `/chatter` across hosts.
//!
//! Every cell: `multihost.launch.xml` places the talker on `robot1` and the
//! listener on `robot2` (`<node machine="…">`). Rust workspaces bake the
//! partition via `nros::main!(launch = …, host = "robotN")` (Phase 211.F);
//! C/C++/mixed workspaces via the CMake `nano_ros_entry(HOST <id> …)`
//! passthrough (phase-263 Track C). Cross-process by construction, so the
//! zenoh-pico in-process write-filter limitation (issue 0096 /
//! `deployed_native_system_e2e`) does not apply.
//!
//! Two observation styles, preserved from the per-cell files ([`Proof`]):
//! - **Hosted-spin cells** (rust robot2): the env-gated hosted spin counts
//!   subscription callbacks and prints `message_callbacks=N` on exit;
//!   N ≥ 1 proves cross-host delivery.
//! - **Listener-stdout cells** (C/C++/mixed robot2): the listener prints
//!   `Received: <n>` per delivered message; ≥3 proves delivery.
//!
//! The `zephyr_rust` cell (phase-276 W6 / #102 H1) swaps robot1 for the
//! Zephyr native_sim image of the SAME per-host entry — one embedded host +
//! one native host meeting at zenohd.
//!
//! NOTE (phase-295 W4): the `port` column mirrors the locator bake in the
//! west lane (`scripts/build/zephyr-fixture-leaves.sh`,
//! `CONFIG_NROS_ZENOH_LOCATOR="tcp/127.0.0.1:17853"`) until W4 re-bakes it
//! through the matrix allocator. `None` = native ephemeral isolation.
//!
//! Run with: `cargo nextest run -p nros-tests --test multihost_e2e`
//! (filter one cell: `-E 'binary(multihost_e2e) and test(native_mixed)'`).

use nros_tests::{
    TestResult,
    fixtures::{
        ManagedProcess, ZenohRouter, ZephyrPlatform, ZephyrProcess,
        build_native_workspace_c_entry_robot1, build_native_workspace_c_entry_robot2,
        build_native_workspace_cpp_entry_robot1, build_native_workspace_cpp_entry_robot2,
        build_native_workspace_mixed_entry_robot1, build_native_workspace_mixed_entry_robot2,
        build_native_workspace_rust_entry_robot1, build_native_workspace_rust_entry_robot2,
        build_zephyr_workspace_rust_multihost_robot1_entry, require_zenohd,
    },
};
use rstest::rstest;
use std::{path::PathBuf, process::Command, time::Duration};

// =============================================================================
// Cell table types
// =============================================================================

/// How the robot1 (talker) side boots.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Boot {
    /// Both hosts are native processes (ephemeral router).
    Native,
    /// robot1 is a Zephyr native_sim image (west-lane fixture; skips when
    /// absent), robot2 stays native.
    ZephyrNativeSim,
}

/// The per-cell delivery assertion, preserved 1:1 from the
/// pre-consolidation files.
#[derive(Copy, Clone, Debug)]
enum Proof {
    /// Rust robot2: env-gated hosted spin exits printing
    /// `message_callbacks=N`; N ≥ 1 proves the listener's subscription
    /// callback fired on robot1's cross-host publishes.
    HostedSpinCallbacks,
    /// C/C++/mixed robot2: the listener prints `Received: <n>` per
    /// delivered message; ≥3 proves cross-host delivery.
    ListenerCount3,
}

/// How the test knows robot2's subscription is live before robot1 starts
/// publishing.
#[derive(Copy, Clone, Debug)]
enum Robot2Ready {
    /// The C workspace listener prints a ready marker.
    Marker,
    /// The C++ listener prints no ready marker — settle on a fixed delay.
    SettleMs(u64),
    /// Rust hosted-spin cells just start robot2 first (its spin budget
    /// absorbs the discovery window).
    None,
}

type Resolver = fn() -> TestResult<PathBuf>;

/// One multihost matrix cell.
struct Cell {
    platform: &'static str,
    lang: &'static str,
    robot1: Resolver,
    robot2: Resolver,
    /// Baked router port (mirrors the west-lane locator bake until the
    /// phase-295 W4 allocator re-bake). `None` = ephemeral (native).
    port: Option<u16>,
    boot: Boot,
    proof: Proof,
    ready: Robot2Ready,
    /// Provenance / nuance — folded into failure messages so a red cell
    /// still names the seam it pins.
    note: &'static str,
}

enum Guest {
    Managed(ManagedProcess),
    Zephyr(ZephyrProcess),
}

impl Guest {
    fn kill(&mut self) {
        match self {
            Guest::Managed(p) => p.kill(),
            Guest::Zephyr(p) => p.kill(),
        }
    }
}

// =============================================================================
// Shared helpers
// =============================================================================

/// Spawn a native per-host entry. Rust entries get the full hosted-spin env
/// (RUST_LOG + step + optional callback-count expectation); C-family entries
/// spin on their own cadence and only need locator/mode/budget.
#[allow(clippy::too_many_arguments)]
fn spawn_native_entry(
    entry: &PathBuf,
    label: &'static str,
    locator: &str,
    spin_ms: u32,
    rust_shape: bool,
    expect_callbacks: bool,
) -> ManagedProcess {
    let mut cmd = Command::new(entry);
    cmd.env("NROS_LOCATOR", locator)
        .env("NROS_SESSION_MODE", "client")
        .env("NROS_ENTRY_SPIN_MS", spin_ms.to_string());
    if rust_shape {
        cmd.env("RUST_LOG", "info")
            .env("NROS_ENTRY_SPIN_STEP_MS", "10");
    }
    if expect_callbacks {
        cmd.env("NROS_ENTRY_EXPECT_MESSAGE_CALLBACKS", "1");
    }
    ManagedProcess::spawn_command(cmd, label).unwrap_or_else(|e| panic!("spawn {label}: {e}"))
}

// =============================================================================
// The parametrized matrix consumer
// =============================================================================

/// One multihost cell: boot robot2 (listener) then robot1 (talker) as two
/// processes and prove `/chatter` crosses the host boundary per the cell's
/// [`Proof`]. Case names carry `<platform>_<lang>` so nextest `test(...)`
/// filters can slice (e.g. `test(zephyr_rust)`).
#[rstest]
// Native (ephemeral router).
#[case::native_rust(Cell {
    platform: "native", lang: "rust",
    robot1: || build_native_workspace_rust_entry_robot1().map(|p| p.to_path_buf()),
    robot2: || build_native_workspace_rust_entry_robot2().map(|p| p.to_path_buf()),
    port: None, boot: Boot::Native,
    proof: Proof::HostedSpinCallbacks, ready: Robot2Ready::None,
    note: "Phase 211.F: `nros::main!(launch = …, host = robotN)` macro host filter \
           bakes talker-only/listener-only entries",
})]
#[case::native_c(Cell {
    platform: "native", lang: "c",
    robot1: || build_native_workspace_c_entry_robot1().map(|p| p.to_path_buf()),
    robot2: || build_native_workspace_c_entry_robot2().map(|p| p.to_path_buf()),
    port: None, boot: Boot::Native,
    proof: Proof::ListenerCount3, ready: Robot2Ready::Marker,
    note: "phase-263 Track C: CMake `nano_ros_entry(HOST <id>)` passthrough shells \
           `nros codegen entry --host <id>` — C parity with the Rust macro bake",
})]
#[case::native_cpp(Cell {
    platform: "native", lang: "cpp",
    robot1: || build_native_workspace_cpp_entry_robot1().map(|p| p.to_path_buf()),
    robot2: || build_native_workspace_cpp_entry_robot2().map(|p| p.to_path_buf()),
    port: None, boot: Boot::Native,
    proof: Proof::ListenerCount3, ready: Robot2Ready::SettleMs(1500),
    note: "phase-263 Track C: C++ per-host entries; the C++ listener prints no ready \
           marker (only `Received:`), hence the settle delay",
})]
#[case::native_mixed(Cell {
    platform: "native", lang: "mixed",
    robot1: || build_native_workspace_mixed_entry_robot1().map(|p| p.to_path_buf()),
    robot2: || build_native_workspace_mixed_entry_robot2().map(|p| p.to_path_buf()),
    port: None, boot: Boot::Native,
    proof: Proof::ListenerCount3, ready: Robot2Ready::SettleMs(1500),
    note: "phase-263 Track C: genuinely mixed-language multihost — robot1 bakes the C \
           talker + Rust heartbeat, robot2 the C++ listener",
})]
// Zephyr native_sim robot1 + native robot2 (west lane).
#[case::zephyr_rust(Cell {
    platform: "zephyr", lang: "rust",
    robot1: build_zephyr_workspace_rust_multihost_robot1_entry,
    robot2: || build_native_workspace_rust_entry_robot2().map(|p| p.to_path_buf()),
    port: Some(17853), boot: Boot::ZephyrNativeSim,
    proof: Proof::HostedSpinCallbacks, ready: Robot2Ready::None,
    note: "phase-276 W6 / #102 H1: multihost-on-embedded — the robot1 talker baked \
           into a Zephyr native_sim image, delivering to the native robot2 listener",
})]
fn multihost(#[case] cell: Cell) {
    // The zephyr cell historically gates on the router START (below) rather
    // than a zenohd probe — keep that shape.
    if cell.boot == Boot::Native && !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let robot1 = (cell.robot1)().unwrap_or_else(|e| {
        nros_tests::skip!(
            "{} {} robot1 entry fixture not built: {e}",
            cell.platform,
            cell.lang
        )
    });
    let robot2 = (cell.robot2)().unwrap_or_else(|e| {
        nros_tests::skip!(
            "{} {} robot2 entry fixture not built: {e}",
            cell.platform,
            cell.lang
        )
    });

    // Router: ephemeral on native; otherwise the EXACT port the west-lane
    // fixture's CONFIG_NROS_ZENOH_LOCATOR was baked with.
    let router = match cell.port {
        None => ZenohRouter::start_unique()
            .unwrap_or_else(|e| nros_tests::skip!("zenohd failed to start: {e}")),
        Some(port) => ZenohRouter::start_on("127.0.0.1", port)
            .unwrap_or_else(|e| nros_tests::skip!("zenohd failed to start on {port}: {e}")),
    };
    let locator = router.locator();

    // Cold-boot budgets: the zephyr cell's robot2 spins longer to absorb the
    // native_sim boot + discovery window (pre-consolidation values).
    let (r2_spin_ms, spin_wait) = match cell.boot {
        Boot::Native => (12000, Duration::from_secs(20)),
        Boot::ZephyrNativeSim => (20000, Duration::from_secs(35)),
    };

    // robot2 (listener) first, so its subscription is live before robot1
    // publishes.
    let rust_shape = matches!(cell.proof, Proof::HostedSpinCallbacks);
    let expect_callbacks = rust_shape;
    let mut r2 = spawn_native_entry(
        &robot2,
        "robot2-listener",
        &locator,
        r2_spin_ms,
        rust_shape,
        expect_callbacks,
    );
    match cell.ready {
        Robot2Ready::Marker => {
            r2.wait_for_output_pattern(
                nros_tests::output::WS_C_LISTENER_READY_MARKER,
                Duration::from_secs(10),
            )
            .unwrap_or_else(|_| {
                r2.kill();
                panic!(
                    "[{} {}] robot2 listener never became ready ({})",
                    cell.platform, cell.lang, cell.note
                )
            });
        }
        Robot2Ready::SettleMs(ms) => std::thread::sleep(Duration::from_millis(ms)),
        Robot2Ready::None => {}
    }

    let mut r1 = match cell.boot {
        Boot::Native => Guest::Managed(spawn_native_entry(
            &robot1,
            "robot1-talker",
            &locator,
            if rust_shape { 9000 } else { 12000 },
            rust_shape,
            false,
        )),
        Boot::ZephyrNativeSim => Guest::Zephyr(
            ZephyrProcess::start(&robot1, ZephyrPlatform::NativeSim)
                .unwrap_or_else(|e| panic!("boot zephyr native_sim: {e}")),
        ),
    };

    match cell.proof {
        Proof::HostedSpinCallbacks => {
            // robot2's hosted spin exits printing the callback count once
            // its budget elapses; wait for that line.
            let out = r2
                .wait_for_output_pattern(nros_tests::output::HOSTED_SPIN_COMPLETE_MARKER, spin_wait)
                .unwrap_or_else(|_| {
                    r1.kill();
                    r2.kill();
                    panic!(
                        "[{} {}] robot2 listener did not finish its hosted spin ({})",
                        cell.platform, cell.lang, cell.note
                    )
                });
            r1.kill();
            r2.kill();

            // `message_callbacks=N` — the listener's subscription callback
            // fired on the talker's cross-host publishes; N ≥ 1 proves
            // multi-host delivery.
            let key = nros_tests::output::HOSTED_SPIN_MESSAGE_CALLBACKS_KEY;
            let delivered = out
                .lines()
                .filter_map(|l| l.split(key).nth(1))
                .filter_map(|s| s.split_whitespace().next())
                .filter_map(|n| n.parse::<u32>().ok())
                .any(|n| n >= 1);
            assert!(
                delivered,
                "[{} {}] robot2 (listener) saw no /chatter callbacks from the robot1 \
                 talker — cross-host delivery failed (expected `{key}N` with N>=1; {}):\n{out}",
                cell.platform, cell.lang, cell.note
            );
        }
        Proof::ListenerCount3 => {
            // robot2 prints `Received: <n>` per delivered message — 3
            // confirms cross-host delivery through the partitioned entries.
            let prefix = nros_tests::output::INT32_LISTENER_LOG_PREFIX;
            let out = r2
                .wait_for_output_count(prefix, 3, Duration::from_secs(18))
                .unwrap_or_else(|_| {
                    r1.kill();
                    r2.kill();
                    panic!(
                        "[{} {}] robot2 (listener-only host entry) never received robot1's \
                         /chatter — the multihost host-partition delivery did not work ({})",
                        cell.platform, cell.lang, cell.note
                    )
                });
            r1.kill();
            r2.kill();

            let n = nros_tests::count_pattern(&out, prefix);
            assert!(
                n >= 3,
                "[{} {}] expected ≥3 cross-host deliveries on robot2, got {n} ({})",
                cell.platform,
                cell.lang,
                cell.note
            );
        }
    }
}

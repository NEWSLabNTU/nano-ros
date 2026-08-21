//! phase-295 W3.b — THE multihost runtime-delivery matrix consumer
//! (RFC-0051).
//!
//! Consolidates the 5 per-cell multihost runtime files
//! (`multihost_runtime_e2e`, `{c,cpp,mixed}_multihost_e2e`,
//! `multihost_zephyr_entry_e2e`) into one parametrized test over the
//! `Workload::Multihost` cells of the test matrix (`nros_tests::matrix`).
//! The SOURCE-level half of the story — resolving with `host:=robotN`
//! yields a per-host model and `nros codegen entry --model` emits an entry
//! registering only that host's nodes — stays a separate codegen gate
//! (`tests/multihost_partition_bake.rs`); this file proves the RUNTIME half:
//! the per-host entries, booted as two separate processes (the multi-host
//! topology), actually exchange `/chatter` across hosts.
//!
//! Every cell: `multihost.launch.xml` gates the talker on `robot1` and the
//! listener on `robot2` via the `host` launch argument (`if=` conditions —
//! phase-326 / issue 0364; the ROS 1-ism `<node machine=…>` is gone). Every
//! workspace consumes the committed per-host models
//! (`multihost_robotN_model.yaml`): Rust via `nros::main!(model = …)`,
//! C/C++/mixed via `nano_ros_add_executable(MODEL …)`. Cross-process by
//! construction, so the
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
//! Isolation (phase-295 W4): the zephyr cell's `port` is the ONE
//! allocator's `Multihost` number (`nros_tests::alloc::port_of`) — the
//! SAME formula the west lane bakes into `CONFIG_NROS_ZENOH_LOCATOR`
//! (`scripts/build/zephyr-fixture-leaves.sh`). `None` = native ephemeral
//! isolation.
//!
//! Run (phase-329 W1 — ONE test derived from `matrix::CELLS`, no per-cell
//! `#[case]`): `cargo nextest run -p nros-tests --test multihost_e2e`. Every
//! `Workload::Multihost` / `Kind::Workspace` / `Tier::Runtime` cell of the
//! matrix runs in the single `multihost` test; adding such a cell to `CELLS`
//! makes it run here with no edit beyond its `exec_for` execution arm (a
//! missing arm is a hard failure, so a new cell can never silently skip).

use nros_tests::{
    TestResult,
    alloc::port_of,
    fixtures::{
        ManagedProcess, ZenohRouter, ZephyrPlatform, ZephyrProcess,
        build_native_workspace_c_entry_robot1, build_native_workspace_c_entry_robot2,
        build_native_workspace_cpp_entry_robot1, build_native_workspace_cpp_entry_robot2,
        build_native_workspace_mixed_entry_robot1, build_native_workspace_mixed_entry_robot2,
        build_native_workspace_rust_entry_robot1, build_native_workspace_rust_entry_robot2,
        build_zephyr_workspace_rust_multihost_robot1_entry, require_zenohd,
    },
    matrix::{Cell as MCell, Kind as MK, Lang as ML, PlatformId as MP, Tier as MT, Workload as MW},
};
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

/// The per-cell EXECUTION data for one multihost matrix cell. The coordinate
/// (`platform`, `lang`, `rmw`) lives in the `matrix::Cell`; this carries only
/// what the matrix cannot express — how the two hosts boot, resolve, and are
/// proven. Keyed by coordinate in [`exec_for`].
struct Exec {
    robot1: Resolver,
    robot2: Resolver,
    /// Baked router port — the allocator's number (matches the west-lane
    /// locator bake). `None` = ephemeral (native).
    port: Option<u16>,
    boot: Boot,
    proof: Proof,
    ready: Robot2Ready,
    /// Provenance / nuance — folded into failure messages so a red cell
    /// still names the seam it pins.
    note: &'static str,
}

/// Map a Multihost matrix coordinate to its execution data. A coordinate with
/// no arm is a HARD panic: adding a `Multihost`/`Workspace`/`Runtime` cell to
/// `matrix::CELLS` forces an arm here, so a new cell can never silently skip
/// (phase-329 W1).
fn exec_for(platform: MP, lang: ML) -> Exec {
    match (platform, lang) {
        (MP::Linux, ML::Rust) => Exec {
            robot1: || build_native_workspace_rust_entry_robot1().map(|p| p.to_path_buf()),
            robot2: || build_native_workspace_rust_entry_robot2().map(|p| p.to_path_buf()),
            port: None,
            boot: Boot::Native,
            proof: Proof::HostedSpinCallbacks,
            ready: Robot2Ready::None,
            note: "phase-326: per-host models (`host:=robotN` resolve) bake talker-only / \
                   listener-only entries",
        },
        (MP::Linux, ML::C) => Exec {
            robot1: || build_native_workspace_c_entry_robot1().map(|p| p.to_path_buf()),
            robot2: || build_native_workspace_c_entry_robot2().map(|p| p.to_path_buf()),
            port: None,
            boot: Boot::Native,
            proof: Proof::ListenerCount3,
            ready: Robot2Ready::Marker,
            note: "phase-326: the C entry consumes the per-host model via \
                   `nano_ros_add_executable(MODEL …)` — C parity with the Rust macro bake",
        },
        (MP::Linux, ML::Cpp) => Exec {
            robot1: || build_native_workspace_cpp_entry_robot1().map(|p| p.to_path_buf()),
            robot2: || build_native_workspace_cpp_entry_robot2().map(|p| p.to_path_buf()),
            port: None,
            boot: Boot::Native,
            proof: Proof::ListenerCount3,
            ready: Robot2Ready::SettleMs(1500),
            note: "phase-263 Track C: C++ per-host entries; the C++ listener prints no ready \
                   marker (only `Received:`), hence the settle delay",
        },
        (MP::Linux, ML::Mixed) => Exec {
            robot1: || build_native_workspace_mixed_entry_robot1().map(|p| p.to_path_buf()),
            robot2: || build_native_workspace_mixed_entry_robot2().map(|p| p.to_path_buf()),
            port: None,
            boot: Boot::Native,
            proof: Proof::ListenerCount3,
            ready: Robot2Ready::SettleMs(1500),
            note: "phase-263 Track C: genuinely mixed-language multihost — robot1 bakes the C \
                   talker + Rust heartbeat, robot2 the C++ listener",
        },
        (MP::ZephyrNativeSim, ML::Rust) => Exec {
            robot1: build_zephyr_workspace_rust_multihost_robot1_entry,
            robot2: || build_native_workspace_rust_entry_robot2().map(|p| p.to_path_buf()),
            port: Some(port_of(MP::ZephyrNativeSim, ML::Rust, MW::Multihost)),
            boot: Boot::ZephyrNativeSim,
            proof: Proof::HostedSpinCallbacks,
            ready: Robot2Ready::None,
            note: "phase-276 W6 / #102 H1: multihost-on-embedded — the robot1 talker baked \
                   into a Zephyr native_sim image, delivering to the native robot2 listener",
        },
        (p, l) => panic!(
            "multihost_e2e: no execution mapping for matrix cell {p:?}/{l:?} — add an \
             `exec_for` arm (phase-329 W1: a new Multihost/Workspace/Runtime cell must \
             wire its boot+resolvers here)"
        ),
    }
}

/// Human coordinate strings for failure messages (were the old `Cell.platform`
/// / `Cell.lang` string fields).
fn plat_str(p: MP) -> &'static str {
    match p {
        MP::ZephyrNativeSim => "zephyr",
        _ => "native",
    }
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

/// THE multihost matrix consumer (phase-329 W1). Iterates every
/// `Multihost`/`Workspace`/`Runtime` cell of `matrix::CELLS` — the case list is
/// DERIVED, not hand-written — and runs each in one process. Per-cell skips and
/// failures are caught so one skipped fixture never aborts the rest: the test
/// fails iff a cell genuinely failed, and skips iff every cell skipped (matching
/// the pre-derivation per-`#[case]` skip semantics, aggregated).
#[test]
fn multihost() {
    let cells: Vec<&MCell> = nros_tests::matrix::CELLS
        .iter()
        .filter(|c| {
            matches!(c.workload, MW::Multihost)
                && matches!(c.kind, MK::Workspace)
                && matches!(c.tier, MT::Runtime)
        })
        .collect();
    assert!(
        !cells.is_empty(),
        "matrix regression: no Multihost/Workspace/Runtime cells — this consumer must have work"
    );

    // issue 0571 — narrow by LANE before running anything: this test's NAME
    // carries no platform token, so `scripts/test/lane-filter.sh native` cannot
    // exclude its embedded cells the way it excludes a platform-named binary.
    // Without this a tier-1 host boots whatever QEMU images happen to exist.
    let out_of_lane: Vec<String> = cells
        .iter()
        .filter(|c| !nros_tests::lane_scope::admits(c.platform))
        .map(|c| nros_tests::lane_scope::skip_note(c.platform, c.lang.as_str()))
        .collect();
    let cells: Vec<&MCell> = cells
        .into_iter()
        .filter(|c| nros_tests::lane_scope::admits(c.platform))
        .collect();
    if !out_of_lane.is_empty() {
        eprintln!(
            "multihost: {} cell(s) out of lane:\n  {}",
            out_of_lane.len(),
            out_of_lane.join("\n  ")
        );
    }
    if cells.is_empty() {
        // The class is `lane`, not the `capability` a plain `skip!` defaults to
        // (issue 0584). Every cell here is out of the RUN's lane — the fixtures
        // were deliberately not built — which is a different fact from "this
        // machine cannot do it", and the two are counted separately in the
        // sweep summary. `baremetal_run_plan_runtime` already carries the
        // classed spelling and a comment about getting it wrong once.
        nros_tests::skip_class!(
            lane,
            "every cell is out of this run's lane:\n  {}",
            out_of_lane.join("\n  ")
        );
    }

    // Silence the caught per-cell panics' default backtrace noise; the loop
    // classifies and re-reports them itself.
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let mut skipped: Vec<String> = Vec::new();
    let mut failed: Vec<String> = Vec::new();
    for c in &cells {
        let label = format!("{}/{}", plat_str(c.platform), c.lang.as_str());
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_cell(c)));
        if let Err(p) = res {
            let msg = p
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "<non-string panic>".to_string());
            if nros_tests::skip_marker::is_skip(&msg) {
                skipped.push(format!("{label}: {msg}"));
            } else {
                failed.push(format!("{label}: {msg}"));
            }
        }
    }
    std::panic::set_hook(prev_hook);

    assert!(
        failed.is_empty(),
        "multihost: {} of {} cell(s) FAILED:\n  {}",
        failed.len(),
        cells.len(),
        failed.join("\n  ")
    );
    if skipped.len() == cells.len() {
        nros_tests::skip!(
            "all {} multihost cell(s) skipped:\n  {}",
            skipped.len(),
            skipped.join("\n  ")
        );
    }
}

/// Boot robot2 (listener) then robot1 (talker) as two processes and prove
/// `/chatter` crosses the host boundary per the cell's [`Proof`]. Panics with
/// `[SKIPPED] …` (via `skip!`) on an unmet precondition; the caller classifies.
fn run_cell(pcell: &MCell) {
    let platform = plat_str(pcell.platform);
    let lang = pcell.lang.as_str();
    let cell = exec_for(pcell.platform, pcell.lang);
    // The zephyr cell historically gates on the router START (below) rather
    // than a zenohd probe — keep that shape.
    if cell.boot == Boot::Native && !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let robot1 = (cell.robot1)().unwrap_or_else(|e| {
        nros_tests::skip!("{} {} robot1 entry fixture not built: {e}", platform, lang)
    });
    let robot2 = (cell.robot2)().unwrap_or_else(|e| {
        nros_tests::skip!("{} {} robot2 entry fixture not built: {e}", platform, lang)
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
                nros_tests::output::LISTENER_WAITING_BANNER,
                Duration::from_secs(10),
            )
            .unwrap_or_else(|_| {
                r2.kill();
                panic!(
                    "[{} {}] robot2 listener never became ready ({})",
                    platform, lang, cell.note
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
                        platform, lang, cell.note
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
                platform, lang, cell.note
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
                        platform, lang, cell.note
                    )
                });
            r1.kill();
            r2.kill();

            let n = nros_tests::count_pattern(&out, prefix);
            assert!(
                n >= 3,
                "[{} {}] expected ≥3 cross-host deliveries on robot2, got {n} ({})",
                platform,
                lang,
                cell.note
            );
        }
    }
}

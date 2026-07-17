//! phase-295 W3.b — THE native workspace-feature matrix consumer (RFC-0051).
//!
//! Consolidates the per-cell native workspace-workload files —
//! `{c_,cpp_,mixed_}custom_msg_workspace_e2e`,
//! `{,c_,cpp_,mixed_}logging_workspace_e2e`,
//! `{c_,cpp_,mixed_}qos_workspace_e2e`, `lifecycle_workspace_e2e` +
//! `cpp_c_lifecycle_autostart_e2e`, and `safety_workspace_e2e` +
//! `cpp_c_safety_integrity_e2e` — into one parametrized test over the
//! native `Workload::{CustomMsg,Logging,Qos,Lifecycle,Safety}` workspace
//! cells of the test matrix (`nros_tests::matrix`). The task list named
//! only the two rust lifecycle/safety files, but the C/C++ halves of those
//! matrix rows lived in the `cpp_c_*` twin files — they are the same
//! family and fold in here (the `cpp_lifecycle_node_wrapper_e2e` stays: it
//! pins the `nros::LifecycleNode` WRAPPER API, not the workspace cell).
//!
//! Five observation styles, preserved 1:1 from the per-cell files
//! ([`Proof`]):
//! - **CustomMsg** (phase-263 B6): the workspace-local `custom_msgs/Reading`
//!   schema flows cross-process — C/C++/mixed talker + listener entries,
//!   the listener prints ≥3 decoded `reading seq=` lines AND the `temp=`
//!   second field (full CDR layout, not just a counter).
//! - **Logging** (phase-263 A5 / phase-264 W3): a Node pkg's
//!   `nros_info!` / `NROS_LOG_INFO(nros_log_default_logger(), …)` reaches
//!   the entry's OWN stdout — process-local, no subscriber (issue 0096
//!   does not apply to logging). Per-lang markers differ (the mixed ws
//!   reuses the C talker).
//! - **Qos** (phase-263 B4): a NON-DEFAULT per-entity profile (reliable +
//!   transient_local + keep_last(10), set IN CODE on both endpoints)
//!   connects + delivers cross-process; the talker boots FIRST so the
//!   listener joins late. No transient-local *replay* assertion (zenoh
//!   provides none out of the box) — a QoS mismatch delivers nothing.
//! - **Lifecycle** (phase-263 A3 / phase-269 W2): `[lifecycle] autostart =
//!   "active"` drives Configure→Activate at boot with NO manual `ros2
//!   lifecycle set`, observed over the REP-2002 service surface (requires
//!   ROS 2 + rmw_zenoh_cpp; skips when absent).
//! - **Safety** (phase-263 B1 / phase-269 W3): CRC-validated delivery —
//!   talker attaches a CRC per `/chatter` publish, the listener's
//!   validated subscription republishes the CRC-valid count on `/safe_ok`,
//!   an external `int32-sink` sees the count climb.
//!
//! All cells are native. The pre-consolidation files pinned arbitrary
//! fixed router ports (17881–17883 logging, 17911/17933/17934 custom-msg,
//! 17921/17931/17932 qos); none is a fixture bake — `examples/fixtures.toml`
//! carries NO port/locator for any of these workspaces and every native
//! entry takes `NROS_LOCATOR` at runtime — so the consolidation moves the
//! whole family onto `ZenohRouter::start_unique` ephemeral isolation
//! (lifecycle/safety already used ephemeral ports).
//!
//! Run with: `cargo nextest run -p nros-tests --test workspace_features_e2e`
//! (filter one workload: `-E 'binary(workspace_features_e2e) and test(qos)'`).

use nros_tests::{
    TestResult,
    fixtures::{
        ManagedProcess, ZenohRouter, build_int32_sink,
        build_native_workspace_c_custom_msg_listener_entry,
        build_native_workspace_c_custom_msg_talker_entry, build_native_workspace_c_entry,
        build_native_workspace_c_lifecycle_entry, build_native_workspace_c_qos_listener_entry,
        build_native_workspace_c_qos_talker_entry, build_native_workspace_c_safety_listener_entry,
        build_native_workspace_c_safety_talker_entry,
        build_native_workspace_cpp_custom_msg_listener_entry,
        build_native_workspace_cpp_custom_msg_talker_entry, build_native_workspace_cpp_entry,
        build_native_workspace_cpp_lifecycle_entry, build_native_workspace_cpp_qos_listener_entry,
        build_native_workspace_cpp_qos_talker_entry,
        build_native_workspace_cpp_safety_listener_entry,
        build_native_workspace_cpp_safety_talker_entry,
        build_native_workspace_mixed_custom_msg_listener_entry,
        build_native_workspace_mixed_custom_msg_talker_entry, build_native_workspace_mixed_entry,
        build_native_workspace_mixed_qos_listener_entry,
        build_native_workspace_mixed_qos_talker_entry, build_native_workspace_rust_entry,
        build_native_workspace_rust_lifecycle_entry,
        build_native_workspace_rust_safety_listener_entry,
        build_native_workspace_rust_safety_talker_entry, require_zenohd,
    },
    ros2::{DEFAULT_ROS_DISTRO, require_ros2, ros2_env_setup_with_locator},
};
use rstest::rstest;
use std::{
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};

// =============================================================================
// Cell table types
// =============================================================================

type Resolver = fn() -> TestResult<PathBuf>;

/// The per-cell topology + assertion, preserved 1:1 from the
/// pre-consolidation files.
#[derive(Copy, Clone, Debug)]
enum Proof {
    /// Talker-first pair; the listener must print ≥3 decoded
    /// `reading seq=` lines AND the `temp=` second field.
    CustomMsgFields,
    /// Single entry; ≥3 lines carrying the per-lang log marker must reach
    /// the entry's OWN stdout (process-local — no subscriber).
    LoggingLines(&'static str),
    /// Talker-first pair with the non-default QoS profile in code on both
    /// endpoints; the late-joining listener must print ≥3 `Received:`.
    QosMatchedCount,
    /// Single autostart entry; `ros2 lifecycle get` on the discovered
    /// managed node must report `active` with no manual transition.
    LifecycleActive,
    /// listener entry + talker entry + external `/safe_ok` sink; the sink
    /// must see ≥3 climbing CRC-valid counts. Per-cell spin/wait budgets
    /// preserved from the rust (16 s/22 s) vs C-family (20 s/25 s) files.
    SafetyCrcCount { spin_ms: u32, wait_secs: u64 },
}

/// One native workspace-feature matrix cell.
struct Cell {
    lang: &'static str,
    workload: &'static str,
    /// The (only / first-booted) entry: the logging/lifecycle entry, or
    /// the pair's TALKER (custom-msg, qos) / safety talker.
    entry: Resolver,
    /// The pair's LISTENER entry (`None` for single-entry workloads).
    peer: Option<Resolver>,
    proof: Proof,
    /// Provenance / nuance — folded into failure messages so a red cell
    /// still names the seam it pins.
    note: &'static str,
}

// =============================================================================
// Shared helpers
// =============================================================================

/// Spawn a C-family custom-msg/qos entry (spins on its own; only the
/// locator — the pre-consolidation shape).
fn spawn_locator_only(entry: &PathBuf, label: &'static str, locator: &str) -> ManagedProcess {
    let mut cmd = Command::new(entry);
    cmd.env("NROS_LOCATOR", locator);
    ManagedProcess::spawn_command(cmd, label).unwrap_or_else(|e| panic!("spawn {label}: {e}"))
}

/// Spawn a hosted-spin entry (`nros::main!` / generated C entry) with the
/// standard env set. `step_ms: None` preserves the C-family logging shape
/// (those files never set `NROS_ENTRY_SPIN_STEP_MS`).
fn spawn_spinning(
    entry: &PathBuf,
    label: &'static str,
    locator: &str,
    spin_ms: u32,
    step_ms: Option<u32>,
) -> ManagedProcess {
    let mut cmd = Command::new(entry);
    cmd.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", locator)
        .env("NROS_SESSION_MODE", "client")
        .env("NROS_ENTRY_SPIN_MS", spin_ms.to_string());
    if let Some(step) = step_ms {
        cmd.env("NROS_ENTRY_SPIN_STEP_MS", step.to_string());
    }
    ManagedProcess::spawn_command(cmd, label).unwrap_or_else(|e| panic!("spawn {label}: {e}"))
}

/// Spawn a native `int32-sink` observer on `topic` (prints `Received: <n>`
/// per message) dialing `locator`; blocks until its subscription is live.
fn spawn_listener(topic: &'static str, locator: &str) -> ManagedProcess {
    let listener = build_int32_sink()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("int32-sink fixture not built: {e}"));
    let mut cmd = Command::new(listener);
    cmd.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", locator)
        .env("NROS_SESSION_MODE", "client")
        .env("NROS_SUB_TOPIC", topic);
    let mut proc =
        ManagedProcess::spawn_command(cmd, topic).unwrap_or_else(|e| panic!("spawn {topic}: {e}"));
    proc.wait_for_output_pattern(
        nros_tests::output::INT32_SINK_READY_MARKER,
        Duration::from_secs(10),
    )
    .unwrap_or_else(|_| panic!("{topic} listener did not become ready"));
    proc
}

/// Run `ros2 <subcommand>` against `locator`; return combined stdout+stderr.
fn run_ros2(locator: &str, subcommand: &str) -> String {
    let (env, _config_guard) = ros2_env_setup_with_locator(DEFAULT_ROS_DISTRO, locator);
    let script = format!("{env} && timeout 10 ros2 {subcommand} 2>&1");
    let out = Command::new("bash")
        .args(["-c", &script])
        .output()
        .expect("failed to spawn bash for ros2 invocation");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Poll `ros2 <subcommand>` until its output contains `marker`
/// (case-insensitive) or timeout.
fn poll_ros2_until(locator: &str, subcommand: &str, marker: &str, timeout: Duration) -> String {
    let deadline = Instant::now() + timeout;
    let marker_lc = marker.to_lowercase();
    let mut last = String::new();
    while Instant::now() < deadline {
        last = run_ros2(locator, subcommand);
        if last.to_lowercase().contains(&marker_lc) {
            return last;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    last
}

/// First `/`-prefixed node name in `ros2 lifecycle nodes` output, if any.
fn first_lifecycle_node(nodes_out: &str) -> Option<String> {
    nodes_out
        .lines()
        .map(|l| l.trim())
        .find(|l| l.starts_with('/'))
        .map(|l| l.to_string())
}

/// Resolve a cell entry, skipping when the fixture is not built.
fn resolve(r: Resolver, cell: &Cell, role: &str) -> PathBuf {
    r().unwrap_or_else(|e| {
        nros_tests::skip!(
            "{} {} {role} entry not built: {e}",
            cell.lang,
            cell.workload
        )
    })
}

// =============================================================================
// The parametrized matrix consumer
// =============================================================================

/// One native workspace-feature cell: boot the workspace entr(y/ies) on an
/// ephemeral router and prove the workload's contract per the cell's
/// [`Proof`]. Case names carry `native_<lang>_<workload>` so nextest
/// `test(...)` filters can slice (e.g. `test(native_c_qos)`).
#[rstest]
// CustomMsg — phase-263 B6: workspace-local `custom_msgs/Reading` schema
// crosses processes (talker + listener entries, RFC-0043 raw-CDR idiom).
#[case::native_c_custom_msg(Cell {
    lang: "c", workload: "custom_msg",
    entry: || build_native_workspace_c_custom_msg_talker_entry().map(|p| p.to_path_buf()),
    peer: Some(|| build_native_workspace_c_custom_msg_listener_entry().map(|p| p.to_path_buf())),
    proof: Proof::CustomMsgFields,
    note: "phase-263 B6 C projection: reading_{talker,listener}_pkg carry the type name \
           as a string + hand-code the CDR (RFC-0043 typed-component idiom); the \
           differentiator is the WORKSPACE-LOCAL schema",
})]
#[case::native_cpp_custom_msg(Cell {
    lang: "cpp", workload: "custom_msg",
    entry: || build_native_workspace_cpp_custom_msg_talker_entry().map(|p| p.to_path_buf()),
    peer: Some(|| build_native_workspace_cpp_custom_msg_listener_entry().map(|p| p.to_path_buf())),
    proof: Proof::CustomMsgFields,
    note: "phase-263 B6 C++ projection: raw-CDR idiom, no generated interface archive \
           linked — dodges any cpp codegen edge",
})]
#[case::native_mixed_custom_msg(Cell {
    lang: "mixed", workload: "custom_msg",
    entry: || build_native_workspace_mixed_custom_msg_talker_entry().map(|p| p.to_path_buf()),
    peer: Some(|| build_native_workspace_mixed_custom_msg_listener_entry().map(|p| p.to_path_buf())),
    proof: Proof::CustomMsgFields,
    note: "phase-263 B6 MIXED projection: the C reading pkgs reused verbatim, driven by \
           a C++ TYPED entry carrier",
})]
// Logging — phase-263 A5: a Node pkg's log call reaches the entry's OWN
// stdout (board boot-time default sink, phase-264 W3). Process-local.
#[case::native_rust_logging(Cell {
    lang: "rust", workload: "logging",
    entry: || build_native_workspace_rust_entry().map(|p| p.to_path_buf()),
    peer: None,
    proof: Proof::LoggingLines(nros_tests::output::WS_RUST_LOGGING_MARKER),
    note: "phase-263 A5 / phase-264 W3: nros-board-posix registers the default platform \
           sink at boot — talker_pkg's nros_info! needs no per-app init",
})]
#[case::native_c_logging(Cell {
    lang: "c", workload: "logging",
    entry: || build_native_workspace_c_entry().map(|p| p.to_path_buf()),
    peer: None,
    proof: Proof::LoggingLines(nros_tests::output::WS_C_LOGGING_MARKER),
    note: "phase-263 A5 C projection: NROS_LOG_INFO(nros_log_default_logger(), …) — a \
           NULL logger handle DROPS the record (the A5 C/C++ finding)",
})]
#[case::native_cpp_logging(Cell {
    lang: "cpp", workload: "logging",
    entry: || build_native_workspace_cpp_entry().map(|p| p.to_path_buf()),
    peer: None,
    proof: Proof::LoggingLines(nros_tests::output::WS_CPP_LOGGING_MARKER),
    note: "phase-263 A5 C++ projection: same facade chain (nros_log_emit_fmt → \
           DEFAULT_LOGGER → lazy default sink → posix writer)",
})]
#[case::native_mixed_logging(Cell {
    lang: "mixed", workload: "logging",
    entry: || build_native_workspace_mixed_entry().map(|p| p.to_path_buf()),
    peer: None,
    proof: Proof::LoggingLines(nros_tests::output::WS_C_LOGGING_MARKER),
    note: "phase-263 A5 MIXED projection: the mixed ws reuses the C talker, so its cell \
           greps the C marker",
})]
// Qos — phase-263 B4: non-default per-entity profile IN CODE on both
// endpoints (reliable + transient_local + keep_last(10)); late joiner.
#[case::native_c_qos(Cell {
    lang: "c", workload: "qos",
    entry: || build_native_workspace_c_qos_talker_entry().map(|p| p.to_path_buf()),
    peer: Some(|| build_native_workspace_c_qos_listener_entry().map(|p| p.to_path_buf())),
    proof: Proof::QosMatchedCount,
    note: "phase-263 B4 C projection: nros_cpp_qos_t by value to nros_cpp_publisher_create \
           (not nros_c_qos_default()); listener declares the BYTE-IDENTICAL profile",
})]
#[case::native_cpp_qos(Cell {
    lang: "cpp", workload: "qos",
    entry: || build_native_workspace_cpp_qos_talker_entry().map(|p| p.to_path_buf()),
    peer: Some(|| build_native_workspace_cpp_qos_listener_entry().map(|p| p.to_path_buf())),
    proof: Proof::QosMatchedCount,
    note: "phase-263 B4 C++ projection: fluent nros::QoS builder \
           (.reliable().transient_local().keep_last(10)) into Node::create_publisher",
})]
#[case::native_mixed_qos(Cell {
    lang: "mixed", workload: "qos",
    entry: || build_native_workspace_mixed_qos_talker_entry().map(|p| p.to_path_buf()),
    peer: Some(|| build_native_workspace_mixed_qos_listener_entry().map(|p| p.to_path_buf())),
    proof: Proof::QosMatchedCount,
    note: "phase-263 B4 MIXED projection: the C qos pkgs reused verbatim under a C++ \
           TYPED entry carrier (run_components)",
})]
// Lifecycle — autostart reaches `active` on its own, observed over the
// REP-2002 service surface (ros2 CLI; requires ROS 2 + rmw_zenoh_cpp).
#[case::native_rust_lifecycle(Cell {
    lang: "rust", workload: "lifecycle",
    entry: || build_native_workspace_rust_lifecycle_entry().map(|p| p.to_path_buf()),
    peer: None,
    proof: Proof::LifecycleActive,
    note: "phase-263 A3: `[lifecycle] autostart = \"active\"` + nros/lifecycle-services — \
           nros::main! (phase-264 W2) registers the 5 REP-2002 services and drives \
           Configure→Activate at boot",
})]
#[case::native_c_lifecycle(Cell {
    lang: "c", workload: "lifecycle",
    entry: || build_native_workspace_c_lifecycle_entry().map(|p| p.to_path_buf()),
    peer: None,
    proof: Proof::LifecycleActive,
    note: "phase-269 W2 C: the generated __nros_entry_setup (emit_c.rs) calls \
           nros_cpp_lifecycle_autostart(executor, 2u)",
})]
#[case::native_cpp_lifecycle(Cell {
    lang: "cpp", workload: "lifecycle",
    entry: || build_native_workspace_cpp_lifecycle_entry().map(|p| p.to_path_buf()),
    peer: None,
    proof: Proof::LifecycleActive,
    note: "phase-269 W2 C++: __nros_entry_setup calls nros_cpp_lifecycle_autostart(__exec, \
           2u) via ::nros::global_handle()",
})]
// Safety — CRC-validated delivery cross-process; the listener entry
// republishes the CRC-valid count on /safe_ok (issue 0096 topology).
#[case::native_rust_safety(Cell {
    lang: "rust", workload: "safety",
    entry: || build_native_workspace_rust_safety_talker_entry().map(|p| p.to_path_buf()),
    peer: Some(|| build_native_workspace_rust_safety_listener_entry().map(|p| p.to_path_buf())),
    proof: Proof::SafetyCrcCount { spin_ms: 16000, wait_secs: 22 },
    note: "phase-263 B1: entry bakes `safety-e2e` → backend-attached CRC-32 + seq per \
           publish; safe_listener validates + reads CallbackCtx::integrity() and \
           republishes the valid count",
})]
#[case::native_c_safety(Cell {
    lang: "c", workload: "safety",
    entry: || build_native_workspace_c_safety_talker_entry().map(|p| p.to_path_buf()),
    peer: Some(|| build_native_workspace_c_safety_listener_entry().map(|p| p.to_path_buf())),
    proof: Proof::SafetyCrcCount { spin_ms: 20000, wait_secs: 25 },
    note: "phase-269 W3 C: nros_cpp_subscription_register_validated callback receives \
           crc_valid == 1 per integrity-passing frame (NANO_ROS_SAFETY_E2E=ON fixture)",
})]
#[case::native_cpp_safety(Cell {
    lang: "cpp", workload: "safety",
    entry: || build_native_workspace_cpp_safety_talker_entry().map(|p| p.to_path_buf()),
    peer: Some(|| build_native_workspace_cpp_safety_listener_entry().map(|p| p.to_path_buf())),
    proof: Proof::SafetyCrcCount { spin_ms: 20000, wait_secs: 25 },
    note: "phase-269 W3 C++: node.create_subscription_with_safety<M>() delivers \
           (const M&, const nros_cpp_integrity_status_t&) with crc_valid == 1",
})]
fn workspace_features(#[case] cell: Cell) {
    // Gate: lifecycle cells assert over the ros2 CLI (skip without ROS 2 +
    // rmw_zenoh_cpp — same contract as the other interop tests); everything
    // else needs only zenohd.
    if matches!(cell.proof, Proof::LifecycleActive) {
        if !require_ros2() {
            nros_tests::skip!("ROS 2 / rmw_zenoh_cpp not available — run: just rmw_zenoh setup");
        }
    } else if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let entry = resolve(cell.entry, &cell, "talker/entry");
    let peer = cell.peer.map(|r| resolve(r, &cell, "listener"));

    // Native-only family: every cell gets an ephemeral router (no fixture
    // bakes a port for any of these workspaces — see the module doc).
    let router = ZenohRouter::start_unique()
        .unwrap_or_else(|e| nros_tests::skip!("zenohd failed to start: {e}"));
    let locator = router.locator();

    match cell.proof {
        Proof::CustomMsgFields => {
            // Talker first so the publisher is discoverable when the
            // listener joins.
            let mut tlk = spawn_locator_only(&entry, "custom-msg-talker", &locator);
            tlk.wait_for_output_pattern(
                nros_tests::output::WS_CUSTOM_MSG_SENT_PREFIX,
                Duration::from_secs(10),
            )
            .unwrap_or_else(|_| {
                tlk.kill();
                panic!(
                    "[{} {}] reading_talker never published ({})",
                    cell.lang, cell.workload, cell.note
                )
            });
            let peer = peer.expect("custom-msg cells carry a listener entry");
            let mut lis = spawn_locator_only(&peer, "custom-msg-listener", &locator);

            let prefix = nros_tests::output::WS_CUSTOM_MSG_READING_PREFIX;
            let out = lis
                .wait_for_output_count(prefix, 3, Duration::from_secs(60))
                .unwrap_or_else(|_| {
                    lis.kill();
                    tlk.kill();
                    panic!(
                        "[{} {}] reading_listener never received 3 custom-msg samples — \
                         the cross-process workspace-local custom-message delivery did \
                         not work ({})",
                        cell.lang, cell.workload, cell.note
                    )
                });
            lis.kill();
            tlk.kill();

            // The talker ramps `sequence` 0,1,2,…; early pre-discovery
            // samples may be missed, so assert the field appears ≥3× rather
            // than a strict value prefix.
            let n = nros_tests::count_pattern(&out, prefix);
            assert!(
                n >= 3,
                "[{} {}] expected ≥3 custom-msg receives, got {n}.\n{out}",
                cell.lang,
                cell.workload
            );
            // The decoded temperature field must also be present (a
            // non-trivial second field), proving the full CDR layout — not
            // just a counter — survives the round-trip.
            assert!(
                out.contains(nros_tests::output::WS_CUSTOM_MSG_TEMP_FIELD),
                "[{} {}] listener output missing the decoded temperature field — CDR \
                 decode wrong ({}).\n{out}",
                cell.lang,
                cell.workload,
                cell.note
            );
        }

        Proof::LoggingLines(marker) => {
            // Rust's `nros::main!` hosted spin historically also got a
            // STEP_MS; the C-family entries never did — preserved.
            let step = (cell.lang == "rust").then_some(10);
            let mut proc = spawn_spinning(&entry, "logging-entry", &locator, 8000, step);

            // The talker logs once per 1 Hz tick; 3 lines confirms the sink
            // is live and the node's log calls keep reaching stdout.
            let out = proc
                .wait_for_output_count(marker, 3, Duration::from_secs(18))
                .unwrap_or_else(|_| {
                    proc.kill();
                    panic!(
                        "[{} {}] the workspace node's log line never reached the entry's \
                         stdout — the node-log facade chain is broken ({})",
                        cell.lang, cell.workload, cell.note
                    )
                });
            proc.kill();

            let n = nros_tests::count_pattern(&out, marker);
            assert!(
                n >= 3,
                "[{} {}] expected ≥3 node log lines on stdout, got {n}",
                cell.lang,
                cell.workload
            );
        }

        Proof::QosMatchedCount => {
            // Talker first (the QoS-tagged publisher boots + keeps
            // publishing at 1 Hz), so the listener joins LATE — proving the
            // QoS-matched endpoints discover + connect across processes.
            let mut tlk = spawn_locator_only(&entry, "qos-talker", &locator);
            tlk.wait_for_output_pattern(
                nros_tests::output::INT32_TALKER_LOG_PREFIX,
                Duration::from_secs(10),
            )
            .unwrap_or_else(|_| {
                tlk.kill();
                panic!(
                    "[{} {}] qos_talker never published ({})",
                    cell.lang, cell.workload, cell.note
                )
            });
            let peer = peer.expect("qos cells carry a listener entry");
            let mut lis = spawn_locator_only(&peer, "qos-listener", &locator);

            let prefix = nros_tests::output::INT32_LISTENER_LOG_PREFIX;
            let out = lis
                .wait_for_output_count(prefix, 3, Duration::from_secs(60))
                .unwrap_or_else(|_| {
                    lis.kill();
                    tlk.kill();
                    panic!(
                        "[{} {}] qos_listener never received 3 QoS-matched samples — the \
                         cross-process per-entity QoS-matched delivery did not work (QoS \
                         mismatch or wiring break) ({})",
                        cell.lang, cell.workload, cell.note
                    )
                });
            lis.kill();
            tlk.kill();

            // Early pre-discovery samples may be missed, so assert ≥3
            // receives (proves the non-default profile, declared per-entity
            // on both endpoints, connects + delivers end-to-end).
            let n = nros_tests::count_pattern(&out, prefix);
            assert!(
                n >= 3,
                "[{} {}] expected ≥3 QoS-matched receives, got {n}.\n{out}",
                cell.lang,
                cell.workload
            );
        }

        Proof::LifecycleActive => {
            let mut node = spawn_spinning(&entry, "lifecycle-entry", &locator, 30000, Some(10));

            // Discover the managed node (the entry's executor node hosting
            // the 5 services) — robust to the executor node name.
            let nodes_out =
                poll_ros2_until(&locator, "lifecycle nodes", "/", Duration::from_secs(20));
            let lifecycle_node = first_lifecycle_node(&nodes_out).unwrap_or_else(|| {
                node.kill();
                panic!(
                    "[{} {}] `ros2 lifecycle nodes` listed no managed node — the workspace \
                     entry's REP-2002 services are not on the wire ({}):\n{nodes_out}",
                    cell.lang, cell.workload, cell.note
                )
            });

            // Autostart should already have driven it to active — no manual
            // `ros2 lifecycle set` issued.
            let state = poll_ros2_until(
                &locator,
                &format!("lifecycle get --no-daemon --spin-time 0.1 {lifecycle_node}"),
                "active",
                Duration::from_secs(20),
            );
            node.kill();

            assert!(
                state.to_lowercase().contains("active"),
                "[{} {}] expected the autostart-managed node {lifecycle_node} to be \
                 `active` at boot ({}), got:\n{state}",
                cell.lang,
                cell.workload,
                cell.note
            );
        }

        Proof::SafetyCrcCount { spin_ms, wait_secs } => {
            // /safe_ok sink first, then the listener entry (its /chatter
            // safety subscription must be up before the talker publishes).
            let mut sub = spawn_listener("/safe_ok", &locator);
            let peer = peer.expect("safety cells carry a listener entry");
            let mut listener =
                spawn_spinning(&peer, "safety-listener", &locator, spin_ms, Some(10));
            std::thread::sleep(Duration::from_millis(1000));
            let mut tlk = spawn_spinning(&entry, "safety-talker", &locator, spin_ms, Some(10));

            // The talker publishes /chatter at 1 Hz with a CRC; each
            // CRC-validated receive republishes the count on /safe_ok.
            // Seeing 3 confirms the validate path holds.
            let prefix = nros_tests::output::INT32_LISTENER_LOG_PREFIX;
            let out = sub
                .wait_for_output_count(prefix, 3, Duration::from_secs(wait_secs))
                .unwrap_or_else(|_| {
                    tlk.kill();
                    listener.kill();
                    sub.kill();
                    panic!(
                        "[{} {}] /safe_ok never saw 3 CRC-validated publishes — the \
                         cross-process E2E safety path (talker → backend CRC → runtime \
                         validate → integrity-read → republish) failed ({})",
                        cell.lang, cell.workload, cell.note
                    )
                });
            tlk.kill();
            listener.kill();
            sub.kill();

            let n = nros_tests::count_pattern(&out, prefix);
            assert!(
                n >= 3,
                "[{} {}] expected ≥3 CRC-validated /safe_ok publishes, got {n}\n{out}",
                cell.lang,
                cell.workload
            );
        }
    }
}

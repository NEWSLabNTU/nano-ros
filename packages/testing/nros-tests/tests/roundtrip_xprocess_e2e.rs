//! phase-295 W3.b — THE cross-process service/action roundtrip matrix
//! consumer (RFC-0051).
//!
//! Consolidates the 8 per-cell `{,c_,cpp_,mixed_}{service,action}_roundtrip_
//! xprocess_e2e` files into one parametrized test over the native workspace
//! roundtrip cells (lang × workload). Every cell is the phase-263 A1/A4
//! Track-D shape: issue 0096 means an in-process (same-executor) service or
//! action server+client can't talk, so each cell boots the workspace's
//! server and client entries as TWO processes and proves the FULL
//! cross-process chain — request → (separate server process) compute →
//! reply/result → client.
//!
//! Two observation styles, preserved from the per-cell files ([`Proof`]):
//! - **Rust cells** observe via a THIRD process: the declarative client
//!   republishes what the server computed (`add_client` → `/sum`,
//!   `fibonacci_client` → `/fib_result`) and an external `int32-sink`
//!   subscriber asserts the server-computed values (sums `1,2,3`; fib last
//!   element `55`).
//! - **C / C++ / mixed cells** assert on the CLIENT's own stdout (`sum: N`
//!   ×3 / `result last=55`) after waiting for the server's `server ready`
//!   marker. These clients use the POLL-model component surfaces
//!   (`send_request`+`take_response`, `send_goal_async`/`get_result_async`
//!   +`poll`), not Rust's blocking `call_for_name` — a component callback
//!   must never block the executor.
//!
//! Cell nuances carried over (see each case's `note`): the mixed service
//! cell is a genuinely cross-LANGUAGE pair (C server + C++ client) since
//! issue #203 closed; the mixed action cell reuses the pure-C Fibonacci
//! pkgs verbatim (cpp variant blocked on the action_msgs cpp-codegen gap —
//! cross-language is preserved at the workspace level).
//!
//! All cells are native with EPHEMERAL routers. The pre-consolidation
//! C/C++/mixed files pinned arbitrary fixed ports (17871–17873 /
//! 17891–17893); those were never fixture bakes — every entry takes
//! `NROS_LOCATOR` at runtime — so the consolidation moves them onto
//! `ZenohRouter::start_unique` isolation like the Rust cells.
//!
//! Run with: `cargo nextest run -p nros-tests --test roundtrip_xprocess_e2e`
//! (filter one lang: `-E 'binary(roundtrip_xprocess_e2e) and test(mixed)'`).

use nros_tests::{
    TestResult,
    fixtures::{
        ManagedProcess, ZenohRouter, build_native_workspace_c_action_client_entry,
        build_native_workspace_c_action_server_entry,
        build_native_workspace_c_service_client_entry,
        build_native_workspace_c_service_server_entry,
        build_native_workspace_cpp_action_client_entry,
        build_native_workspace_cpp_action_server_entry,
        build_native_workspace_cpp_service_client_entry,
        build_native_workspace_cpp_service_server_entry,
        build_native_workspace_mixed_action_client_entry,
        build_native_workspace_mixed_action_server_entry,
        build_native_workspace_mixed_service_client_entry,
        build_native_workspace_mixed_service_server_entry,
        build_native_workspace_rust_action_client_entry,
        build_native_workspace_rust_action_server_entry,
        build_native_workspace_rust_service_client_entry,
        build_native_workspace_rust_service_server_entry, require_zenohd,
    },
    matrix::{Cell as MCell, Kind as MK, Lang as ML, Tier as MT, Workload as MW},
};
use std::{path::PathBuf, process::Command, time::Duration};

// =============================================================================
// Cell table types
// =============================================================================

/// The per-cell assertion + process topology, preserved 1:1 from the
/// pre-consolidation files.
#[derive(Copy, Clone, Debug)]
enum Proof {
    /// Rust declarative service chain: `add_client` calls `add_server`
    /// (a=0,1,2,…, b=1) and republishes each reply on `/sum`; an external
    /// `int32-sink` must see the server-computed sums `1, 2, 3`.
    ListenerSums,
    /// Rust declarative action chain: `fibonacci_client` sends one goal
    /// (order 10) and its result callback republishes the LAST sequence
    /// element on `/fib_result`; an external `int32-sink` must see `55`.
    ListenerFibLast,
    /// C-family poll-model service chain: wait for the server's
    /// `server ready`, then the client's stdout must show ≥3 server-computed
    /// `sum: N` replies with values 1, 2, 3 (early pre-discovery requests
    /// may be dropped + resent, so assert the VALUES appear, not a strict
    /// prefix).
    ClientSums,
    /// C-family poll-model action chain: wait for `server ready`, then the
    /// client's stdout must show `result last=55` (order 10 →
    /// `0,1,1,2,3,5,8,13,21,34,55`).
    ClientFibLast,
}

type Resolver = fn() -> TestResult<PathBuf>;

/// The per-cell EXECUTION data for one roundtrip matrix cell (all native — see
/// the module doc). The coordinate lives in `matrix::Cell`; this carries only
/// server/client resolvers + the proof style. Keyed by coordinate in
/// [`exec_for`].
struct Exec {
    server: Resolver,
    client: Resolver,
    proof: Proof,
    /// Provenance / nuance — folded into failure messages so a red cell
    /// still names the seam it pins.
    note: &'static str,
}

/// Map a native workspace `(lang, workload∈{Service,Action})` coordinate to its
/// execution data. An unmapped coordinate is a HARD panic: adding such a cell to
/// `matrix::CELLS` forces an arm here (phase-329 W1).
fn exec_for(lang: ML, workload: MW) -> Exec {
    match (lang, workload) {
        (ML::Rust, MW::Service) => Exec {
            server: || build_native_workspace_rust_service_server_entry().map(|p| p.to_path_buf()),
            client: || build_native_workspace_rust_service_client_entry().map(|p| p.to_path_buf()),
            proof: Proof::ListenerSums,
            note: "phase-263 A1: declarative AddTwoInts chain — client `call_for_name` → \
                   server `on_callback` sum → reply → client republish on /sum",
        },
        (ML::Rust, MW::Action) => Exec {
            server: || build_native_workspace_rust_action_server_entry().map(|p| p.to_path_buf()),
            client: || build_native_workspace_rust_action_client_entry().map(|p| p.to_path_buf()),
            proof: Proof::ListenerFibLast,
            note: "phase-263 A4: declarative Fibonacci chain — send_goal → accept in \
                   on_callback → feedback+result in tick → auto-delivered result callback \
                   republishes the last element on /fib_result",
        },
        (ML::C, MW::Service) => Exec {
            server: || build_native_workspace_c_service_server_entry().map(|p| p.to_path_buf()),
            client: || build_native_workspace_c_service_client_entry().map(|p| p.to_path_buf()),
            proof: Proof::ClientSums,
            note: "phase-263 A1 C projection: c_add_server_pkg + c_add_client_pkg over the \
                   poll-model client (nros_cpp_service_client_send_request + take_response)",
        },
        (ML::C, MW::Action) => Exec {
            server: || build_native_workspace_c_action_server_entry().map(|p| p.to_path_buf()),
            client: || build_native_workspace_c_action_client_entry().map(|p| p.to_path_buf()),
            proof: Proof::ClientFibLast,
            note: "phase-263 A4 C projection: c_fib_server_pkg + c_fib_client_pkg over the \
                   poll-model action client (send_goal_async / get_result_async / poll)",
        },
        (ML::Cpp, MW::Service) => Exec {
            server: || build_native_workspace_cpp_service_server_entry().map(|p| p.to_path_buf()),
            client: || build_native_workspace_cpp_service_client_entry().map(|p| p.to_path_buf()),
            proof: Proof::ClientSums,
            note: "phase-263 A1 C++ projection: cpp_add_server_pkg + cpp_add_client_pkg over \
                   the poll-model service client",
        },
        (ML::Cpp, MW::Action) => Exec {
            server: || build_native_workspace_cpp_action_server_entry().map(|p| p.to_path_buf()),
            client: || build_native_workspace_cpp_action_client_entry().map(|p| p.to_path_buf()),
            proof: Proof::ClientFibLast,
            note: "phase-263 A4 C++ projection: cpp_fib_server_pkg + cpp_fib_client_pkg over \
                   the raw poll-model action client (create_action_client_raw + set_callbacks)",
        },
        (ML::Mixed, MW::Service) => Exec {
            server: || build_native_workspace_mixed_service_server_entry().map(|p| p.to_path_buf()),
            client: || build_native_workspace_mixed_service_client_entry().map(|p| p.to_path_buf()),
            proof: Proof::ClientSums,
            note: "phase-263 A1 MIXED projection: genuinely cross-LANGUAGE since #203 closed — \
                   C server (c_add_server_pkg) + C++ client (cpp_add_client_pkg)",
        },
        (ML::Mixed, MW::Action) => Exec {
            server: || build_native_workspace_mixed_action_server_entry().map(|p| p.to_path_buf()),
            client: || build_native_workspace_mixed_action_client_entry().map(|p| p.to_path_buf()),
            proof: Proof::ClientFibLast,
            note: "phase-263 A4 MIXED projection: reuses the pure-C Fibonacci pkgs verbatim \
                   (cpp variant blocked on the action_msgs cpp-codegen gap); cross-language \
                   lives at the workspace level (C talker + C++ listener + Rust heartbeat)",
        },
        (l, w) => panic!(
            "roundtrip_xprocess_e2e: no execution mapping for matrix cell {l:?}/{w:?} — add an \
             `exec_for` arm (phase-329 W1: a new native workspace Service/Action cell must wire \
             its server+client here)"
        ),
    }
}

fn wl_str(w: MW) -> &'static str {
    match w {
        MW::Action => "action",
        _ => "service",
    }
}

// =============================================================================
// Shared helpers
// =============================================================================

/// Spawn a Rust workspace entry (`nros::main!` env-gated hosted spin).
fn spawn_rust_entry(
    entry: &PathBuf,
    label: &'static str,
    locator: &str,
    spin_ms: u32,
) -> ManagedProcess {
    let mut cmd = Command::new(entry);
    cmd.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", locator)
        .env("NROS_SESSION_MODE", "client")
        .env("NROS_ENTRY_SPIN_MS", spin_ms.to_string())
        .env("NROS_ENTRY_SPIN_STEP_MS", "10");
    ManagedProcess::spawn_command(cmd, label).unwrap_or_else(|e| panic!("spawn {label}: {e}"))
}

/// Spawn a C/C++/mixed workspace entry (spins on its own; only the locator).
fn spawn_c_entry(entry: &PathBuf, label: &'static str, locator: &str) -> ManagedProcess {
    let mut cmd = Command::new(entry);
    cmd.env("NROS_LOCATOR", locator);
    ManagedProcess::spawn_command(cmd, label).unwrap_or_else(|e| panic!("spawn {label}: {e}"))
}

// =============================================================================
// The parametrized matrix consumer
// =============================================================================

/// THE cross-process roundtrip matrix consumer (phase-329 W1). Iterates every
/// `Workspace`/`Service`|`Action`/`Runtime` cell of `matrix::CELLS` (all
/// native) — the case list is DERIVED — and runs each in one process, catching
/// per-cell skips/failures so one missing fixture never aborts the rest.
#[test]
fn roundtrip_xprocess() {
    let cells: Vec<&MCell> = nros_tests::matrix::CELLS
        .iter()
        .filter(|c| {
            matches!(c.kind, MK::Workspace)
                && matches!(c.workload, MW::Service | MW::Action)
                && matches!(c.tier, MT::Runtime)
        })
        .collect();
    assert!(
        !cells.is_empty(),
        "matrix regression: no Workspace Service/Action runtime cells for this consumer"
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
            "roundtrip_xprocess: {} cell(s) out of lane:\n  {}",
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

    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let mut skipped: Vec<String> = Vec::new();
    let mut failed: Vec<String> = Vec::new();
    for c in &cells {
        let label = format!("{}/{}", c.lang.as_str(), wl_str(c.workload));
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
        "roundtrip_xprocess: {} of {} cell(s) FAILED:\n  {}",
        failed.len(),
        cells.len(),
        failed.join("\n  ")
    );
    if skipped.len() == cells.len() {
        nros_tests::skip!(
            "all {} roundtrip cell(s) skipped:\n  {}",
            skipped.len(),
            skipped.join("\n  ")
        );
    }
}

/// Boot the workspace's server + client entries as two processes and prove the
/// server-computed values come back per the cell's [`Proof`]. Panics with
/// `[SKIPPED] …` on an unmet precondition; the caller classifies.
fn run_cell(pcell: &MCell) {
    let lang = pcell.lang.as_str();
    let workload = wl_str(pcell.workload);
    let cell = exec_for(pcell.lang, pcell.workload);
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let server = (cell.server)()
        .unwrap_or_else(|e| nros_tests::skip!("{} {} server entry not built: {e}", lang, workload));
    let client = (cell.client)()
        .unwrap_or_else(|e| nros_tests::skip!("{} {} client entry not built: {e}", lang, workload));

    // Native-only family: every cell gets an ephemeral router.
    let router = ZenohRouter::start_unique()
        .unwrap_or_else(|e| nros_tests::skip!("zenohd failed to start: {e}"));
    let locator = router.locator();

    match cell.proof {
        // Rust cells: external observer on the client's republish topic.
        Proof::ListenerSums | Proof::ListenerFibLast => {
            let (topic, spin_ms) = match cell.proof {
                Proof::ListenerSums => ("/sum", 16000),
                _ => ("/fib_result", 20000),
            };
            let mut listener = nros_tests::fixtures::spawn_int32_sink(Some(topic), &locator);
            let mut srv = spawn_rust_entry(&server, "roundtrip-server", &locator, spin_ms);
            // Give the server a moment to register its queryable/action
            // before the client calls (pre-consolidation shape).
            std::thread::sleep(Duration::from_millis(1000));
            let mut cli = spawn_rust_entry(&client, "roundtrip-client", &locator, spin_ms);

            if matches!(cell.proof, Proof::ListenerSums) {
                // add_client calls at 1 Hz with a = 0,1,2,… → the server
                // returns 1,2,3,…; the third confirms ≥3 round-trips.
                let out = listener
                    .wait_for_output_count(
                        nros_tests::output::INT32_LISTENER_LOG_PREFIX,
                        3,
                        Duration::from_secs(22),
                    )
                    .unwrap_or_else(|_| {
                        cli.kill();
                        srv.kill();
                        listener.kill();
                        panic!(
                            "[{} {}] {topic} subscriber never saw 3 server-computed sums — \
                             the cross-process round-trip did not complete ({})",
                            lang, workload, cell.note
                        )
                    });
                cli.kill();
                srv.kill();
                listener.kill();
                // The first three sums must be exactly the server-computed
                // 1, 2, 3 (a=0,1,2 + b=1).
                for n in [1, 2, 3] {
                    let expected = nros_tests::output::int32_listener_line(n);
                    assert!(
                        out.contains(expected.as_str()),
                        "[{} {}] expected server-computed sum line {expected:?} on {topic} \
                         ({}); got:\n{out}",
                        lang,
                        workload,
                        cell.note
                    );
                }
            } else {
                // The result's last sequence element is 55 (fib, order 10).
                let expected = nros_tests::output::int32_listener_line(55);
                let out = listener
                    .wait_for_output_pattern(expected.as_str(), Duration::from_secs(25))
                    .unwrap_or_else(|_| {
                        cli.kill();
                        srv.kill();
                        listener.kill();
                        panic!(
                            "[{} {}] {topic} never saw the server-computed result (55) — \
                             the cross-process round-trip did not complete ({})",
                            lang, workload, cell.note
                        )
                    });
                cli.kill();
                srv.kill();
                listener.kill();
                assert!(
                    out.contains(expected.as_str()),
                    "[{} {}] expected the Fibonacci result last element 55 on {topic} \
                     ({}); got:\n{out}",
                    lang,
                    workload,
                    cell.note
                );
            }
        }

        // C-family cells: server-ready marker, then the client's stdout.
        Proof::ClientSums | Proof::ClientFibLast => {
            // Server first — it must be discoverable before the client's
            // requests/goal land.
            let mut srv = spawn_c_entry(&server, "roundtrip-server", &locator);
            srv.wait_for_output_pattern(
                nros_tests::output::WS_SERVER_READY_MARKER,
                Duration::from_secs(10),
            )
            .unwrap_or_else(|_| {
                srv.kill();
                panic!(
                    "[{} {}] server never became ready ({})",
                    lang, workload, cell.note
                )
            });
            let mut cli = spawn_c_entry(&client, "roundtrip-client", &locator);

            if matches!(cell.proof, Proof::ClientSums) {
                let prefix = nros_tests::output::WS_SERVICE_CLIENT_SUM_PREFIX;
                let out = cli
                    .wait_for_output_count(prefix, 3, Duration::from_secs(60))
                    .unwrap_or_else(|_| {
                        cli.kill();
                        srv.kill();
                        panic!(
                            "[{} {}] client never received 3 server-computed sums — the \
                             cross-process service round-trip did not work ({})",
                            lang, workload, cell.note
                        )
                    });
                cli.kill();
                srv.kill();
                // a + b = a + 1 for a = 0,1,2,… → 1, 2, 3 (values, not a
                // strict prefix — early pre-discovery requests may be
                // dropped + resent).
                for n in [1, 2, 3] {
                    let expected = nros_tests::output::ws_service_client_sum_line(n);
                    assert!(
                        out.contains(expected.as_str()),
                        "[{} {}] client output missing `{expected}` — server-side compute \
                         or round-trip wrong ({}).\n{out}",
                        lang,
                        workload,
                        cell.note
                    );
                }
                let n = nros_tests::count_pattern(&out, prefix);
                assert!(
                    n >= 3,
                    "[{} {}] expected ≥3 service round-trips, got {n}",
                    lang,
                    workload
                );
            } else {
                let expected = nros_tests::output::ws_action_result_last_line(55);
                let out = cli
                    .wait_for_output_pattern(expected.as_str(), Duration::from_secs(60))
                    .unwrap_or_else(|_| {
                        cli.kill();
                        srv.kill();
                        panic!(
                            "[{} {}] client never received the server-computed Fibonacci \
                             result — the cross-process action round-trip did not work ({})",
                            lang, workload, cell.note
                        )
                    });
                cli.kill();
                srv.kill();
                // order = 10 → 0,1,1,2,3,5,8,13,21,34,55 → last element 55.
                assert!(
                    out.contains(expected.as_str()),
                    "[{} {}] client output missing `{expected}` — server-side compute or \
                     round-trip wrong ({}).\n{out}",
                    lang,
                    workload,
                    cell.note
                );
            }
        }
    }
}

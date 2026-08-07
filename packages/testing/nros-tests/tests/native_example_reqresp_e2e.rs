//! phase-329 W4 — THE native-example service + action matrix consumer.
//!
//! The fold vehicle for the native `Example` `Service` and `Action` cells:
//! derives its cases from `matrix::CELLS` (`Native` / `Service`|`Action` /
//! `Example` / `Runtime`) instead of the hand-written `services.rs` / `actions.rs`
//! one-offs. Built ADDITIVELY — runs alongside them until run-proven, then their
//! delivery cases fold in (their startup/error cases stay, labelled).
//!
//! Each cell is the cross-process req/resp shape (issue 0096 — a server + client
//! in one executor can't talk): boot the server, wait its ready marker, boot the
//! client, and prove the client logs the server-computed result. Service →
//! `Result of add_two_ints: 5` (the demo `2 + 3`); Action → `Result received:`
//! (Fibonacci). Per-RMW isolation is the pubsub consumer's, faithful to the
//! working `services`/`actions`/`native_api`/`xrce.rs` tests.
//!
//! Run: `cargo nextest run -p nros-tests --test native_example_reqresp_e2e`.

use nros_tests::{
    TestResult,
    fixtures::{
        ManagedProcess, Rmw as FixtureRmw, XrceAgent, ZenohRouter, build_native_c_example_rmw,
        build_native_cpp_example_rmw, build_native_rust_example_rmw, require_xrce_agent,
        require_zenohd,
    },
    matrix::{Cell as MCell, Kind as MK, Lang as ML, Rmw as MR, Tier as MT, Workload as MW},
    output::{ACTION_RESULT_PREFIX, FIBONACCI_ORDER_10_SEQUENCE, SERVICE_RESULT_PREFIX},
};
use std::{path::PathBuf, process::Command, time::Duration};

fn fixture_rmw(r: MR) -> FixtureRmw {
    match r {
        MR::Zenoh => FixtureRmw::Zenoh,
        MR::Cyclonedds => FixtureRmw::Cyclonedds,
        MR::Xrce => FixtureRmw::Xrce,
        MR::Uorb => unreachable!("no uorb native example req/resp cell"),
    }
}

fn resolve(lang: ML, case: &str, binary: &str, rmw: MR) -> TestResult<PathBuf> {
    let fr = fixture_rmw(rmw);
    match lang {
        ML::Rust => build_native_rust_example_rmw(case, binary, fr).map(|p| p.to_path_buf()),
        ML::C => build_native_c_example_rmw(case, binary, fr).map(|p| p.to_path_buf()),
        ML::Cpp => build_native_cpp_example_rmw(case, binary, fr).map(|p| p.to_path_buf()),
        ML::Mixed => unreachable!("no mixed native example req/resp cell"),
    }
}

/// `(server_case, server_bin, client_case, client_bin, ready_marker,
/// result_marker)` for a `(lang, workload)`. The Rust example `[[bin]]` names are
/// irregular; C/C++ are the `c_*`/`cpp_*` cmake targets.
fn roles(
    lang: ML,
    workload: MW,
) -> (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
) {
    match (lang, workload) {
        (ML::Rust, MW::Service) => (
            "service-server",
            "add_two_ints_server",
            "service-client",
            "service-client",
            "Waiting for service",
            SERVICE_RESULT_PREFIX,
        ),
        (ML::C, MW::Service) => (
            "service-server",
            "c_service_server",
            "service-client",
            "c_service_client",
            "Waiting for service",
            SERVICE_RESULT_PREFIX,
        ),
        (ML::Cpp, MW::Service) => (
            "service-server",
            "cpp_service_server",
            "service-client",
            "cpp_service_client",
            "Waiting for service",
            SERVICE_RESULT_PREFIX,
        ),
        (ML::Rust, MW::Action) => (
            "action-server",
            "action-server",
            "action-client",
            "action-client",
            "Waiting for action",
            ACTION_RESULT_PREFIX,
        ),
        (ML::C, MW::Action) => (
            "action-server",
            "c_action_server",
            "action-client",
            "c_action_client",
            "Waiting for action",
            ACTION_RESULT_PREFIX,
        ),
        (ML::Cpp, MW::Action) => (
            "action-server",
            "cpp_action_server",
            "action-client",
            "cpp_action_client",
            "Waiting for action",
            ACTION_RESULT_PREFIX,
        ),
        (l, w) => panic!("native_example_reqresp_e2e: no roles for {l:?}/{w:?}"),
    }
}

fn lang_str(l: ML) -> &'static str {
    match l {
        ML::Rust => "rust",
        ML::C => "c",
        ML::Cpp => "cpp",
        ML::Mixed => "mixed",
    }
}
fn rmw_str(r: MR) -> &'static str {
    match r {
        MR::Zenoh => "zenoh",
        MR::Cyclonedds => "cyclone",
        MR::Xrce => "xrce",
        MR::Uorb => "uorb",
    }
}
fn wl_str(w: MW) -> &'static str {
    match w {
        MW::Action => "action",
        _ => "service",
    }
}

#[test]
fn native_example_reqresp() {
    let cells: Vec<&MCell> = nros_tests::matrix::CELLS
        .iter()
        .filter(|c| {
            matches!(c.platform, nros_tests::matrix::PlatformId::Linux)
                && matches!(c.kind, MK::Example)
                && matches!(c.workload, MW::Service | MW::Action)
                && matches!(c.tier, MT::Runtime)
        })
        .collect();
    assert!(
        !cells.is_empty(),
        "matrix regression: no Native Service/Action example runtime cells"
    );

    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let mut skipped: Vec<String> = Vec::new();
    let mut failed: Vec<String> = Vec::new();
    for c in &cells {
        let label = format!(
            "{}/{}/{}",
            lang_str(c.lang),
            rmw_str(c.rmw),
            wl_str(c.workload)
        );
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_cell(c)));
        if let Err(p) = res {
            let msg = p
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "<non-string panic>".to_string());
            if msg.contains("[SKIPPED]") {
                skipped.push(format!("{label}: {msg}"));
            } else {
                failed.push(format!("{label}: {msg}"));
            }
        }
    }
    std::panic::set_hook(prev_hook);

    assert!(
        failed.is_empty(),
        "native_example_reqresp: {} of {} cell(s) FAILED:\n  {}",
        failed.len(),
        cells.len(),
        failed.join("\n  ")
    );
    if skipped.len() == cells.len() {
        nros_tests::skip!(
            "all {} native req/resp cell(s) skipped:\n  {}",
            skipped.len(),
            skipped.join("\n  ")
        );
    }
}

/// Boot server → client and prove the client logs the server-computed result.
fn run_cell(cell: &MCell) {
    let lang = cell.lang;
    let (srv_case, srv_bin, cli_case, cli_bin, ready, result) = roles(lang, cell.workload);
    let server = resolve(lang, srv_case, srv_bin, cell.rmw)
        .unwrap_or_else(|e| nros_tests::skip!("{} server fixture not built: {e}", lang_str(lang)));
    let client = resolve(lang, cli_case, cli_bin, cell.rmw)
        .unwrap_or_else(|e| nros_tests::skip!("{} client fixture not built: {e}", lang_str(lang)));

    let mut server_cmd = Command::new(&server);
    let mut client_cmd = Command::new(&client);
    server_cmd.env("RUST_LOG", "info");
    client_cmd.env("RUST_LOG", "info");

    let mut _zenohd: Option<ZenohRouter> = None;
    let mut _agent: Option<XrceAgent> = None;

    match cell.rmw {
        MR::Zenoh => {
            if !require_zenohd() {
                nros_tests::skip!("zenohd not found");
            }
            let router = ZenohRouter::start_unique()
                .unwrap_or_else(|e| nros_tests::skip!("zenohd failed to start: {e}"));
            server_cmd.env("NROS_LOCATOR", router.locator());
            client_cmd.env("NROS_LOCATOR", router.locator());
            _zenohd = Some(router);
        }
        MR::Cyclonedds => {
            let domain = nros_tests::unique_ros_domain_id().to_string();
            let cyclone_lib = nros_tests::project_root().join("build/install/lib");
            let ldp = match std::env::var_os("LD_LIBRARY_PATH") {
                Some(existing) if !existing.is_empty() => {
                    let mut paths = vec![cyclone_lib];
                    paths.extend(std::env::split_paths(&existing));
                    std::env::join_paths(paths).expect("valid LD_LIBRARY_PATH")
                }
                _ => cyclone_lib.into_os_string(),
            };
            for cmd in [&mut server_cmd, &mut client_cmd] {
                cmd.env("ROS_DOMAIN_ID", &domain)
                    .env("LD_LIBRARY_PATH", &ldp);
            }
        }
        MR::Xrce => {
            if !require_xrce_agent() {
                nros_tests::skip!("XRCE agent not available");
            }
            let agent = XrceAgent::start_unique()
                .unwrap_or_else(|e| nros_tests::skip!("XRCE Agent failed to start: {e:?}"));
            let addr = agent.addr();
            let domain = nros_tests::unique_ros_domain_id().to_string();
            for cmd in [&mut server_cmd, &mut client_cmd] {
                cmd.env("NROS_LOCATOR", &addr)
                    .env("XRCE_AGENT_ADDR", &addr)
                    .env("ROS_DOMAIN_ID", &domain);
            }
            _agent = Some(agent);
        }
        MR::Uorb => nros_tests::skip!("uorb has no native example req/resp cell"),
    }

    // Server first; wait its ready marker so the client's request finds it.
    let mut srv = ManagedProcess::spawn_command(server_cmd, "native-example-server")
        .unwrap_or_else(|e| panic!("[{}] spawn server: {e}", rmw_str(cell.rmw)));
    let _ = srv.wait_for_output_pattern(ready, Duration::from_secs(30));
    std::thread::sleep(Duration::from_secs(1));

    let mut cli = ManagedProcess::spawn_command(client_cmd, "native-example-client")
        .unwrap_or_else(|e| {
            srv.kill();
            panic!("[{}] spawn client: {e}", rmw_str(cell.rmw))
        });

    let out = cli
        .wait_for_output_pattern(result, Duration::from_secs(30))
        .unwrap_or_else(|_| {
            cli.wait_for_all_output(Duration::from_secs(2))
                .unwrap_or_default()
        });
    cli.kill();
    srv.kill();

    assert!(
        out.contains(result),
        "[{}/{}/{}] client never logged the server-computed result (`{result}`) — the \
         cross-process {} round-trip did not complete:\n{out}",
        lang_str(lang),
        rmw_str(cell.rmw),
        wl_str(cell.workload),
        wl_str(cell.workload),
    );

    // Issue 0453 — the action rows assert the PAYLOAD, not just the prefix.
    //
    // `ACTION_RESULT_PREFIX` alone is printed by a client that decoded a zeroed
    // default result, so it cannot tell a delivered goal from a dropped one.
    // That is how #0448 (the Rust client shipped two CDR encapsulations, so
    // Fast-DDS dropped every goal) stayed green across this whole matrix while
    // only the XRCE↔ROS 2 interop test caught it — and how #0461 (the server
    // decoded the goal UUID as `order`) hid too, because a nano-ros client's
    // UUID begins with a counter and so `order` always looked plausible.
    //
    // This is assertable now that all three example servers derive their output
    // from `goal.order` on the SAME convention (order N yields N+1 elements,
    // matching ROS 2 `action_tutorials`): the Rust server stores the accepted
    // order (#0450), the C server does too (#0453), and the C++ loop moved from
    // `i < order` to `i <= order` (#0453). Every client requests order 10, and
    // all three print the sequence `", "`-separated, so one constant serves the
    // whole matrix.
    if cell.workload == MW::Action {
        assert!(
            out.contains(FIBONACCI_ORDER_10_SEQUENCE),
            "[{}/{}/action] client logged `{result}` but NOT the order-10 sequence \
             `{FIBONACCI_ORDER_10_SEQUENCE}` — the goal payload did not survive the \
             round-trip. A result line proves the client DECODED something, not that \
             the server ever saw the goal (issues 0453 / 0448 / 0461):\n{out}",
            lang_str(lang),
            rmw_str(cell.rmw),
        );
    }
}

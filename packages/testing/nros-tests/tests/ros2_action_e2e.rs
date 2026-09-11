//! Issue 0976 — an OUTSIDE WITNESS for the Cyclone action wire format.
//!
//! `service.cpp` carries five per-action-type adapters that reshape CDR:
//! `strip_goal_id_len_at` removes a 4-byte length prefix before a goal UUID,
//! `strip_nested_cdr_at` removes a nested encapsulation header from inside a
//! message, and three more special-case `_SendGoal_*` / `_GetResult_*`. Two of
//! them DELETE BYTES — they correct a serialization that would otherwise be
//! wrong on the wire.
//!
//! Until this file, the only thing exercising them was
//! `test_native_cyclonedds_rust_action`, which is nano-ros server to nano-ros
//! client. Both ends share whatever convention the adapters implement, so that
//! test passes whether the bytes are ROS 2's or not — the single property the
//! adapters exist to provide is the one property it cannot observe. The message
//! and service paths already had witnesses (`ros2_pubsub_e2e`, `ros2_srv_e2e`);
//! actions had none.
//!
//! This is that witness: a stock `ros2 action …`, over `rmw_cyclonedds_cpp`,
//! against a nano-ros action node — in BOTH directions, because the adapters
//! sit on both sides of the service path. It is also what unblocks issue 0970's
//! service half — migrating `service.cpp` to a blob sertype would change the
//! action wire format, and nothing in the tree could tell.
//!
//! Interop cells: `native-action-rust-cyclone-{r2n,n2r}` and, since phase-455
//! W3, `native-action-rust-zenoh-{r2n,n2r}` (`nros_tests::interop::CELLS`).
//! This file is the ACTIONS family's only live-peer coverage — every action row
//! in `matrix::CELLS` is nano-to-nano.
//!
//! **The zenoh half is a different subject, not a second copy.** Its cells were
//! added because the reply-slot table issue 0902 exhausted lives in
//! zenoh-pico's queryable path, which none of the Cyclone cases above touches:
//! a service server IS a queryable there, so a SendGoal, a GetResult and a
//! CancelGoal each consume one of four `ZPICO_MAX_PENDING_REPLIES` slots. Their
//! setup and their `nextest` group differ accordingly; the block before those
//! two cases says how and why.

use std::{
    process::Command,
    time::{Duration, Instant},
};

use nros_tests::{
    fixtures,
    output::{ACTION_FEEDBACK_PREFIX, ACTION_RESULT_PREFIX},
    ros_env::{HostRosEnv, Middleware, RosEnv},
    ros2::{DEFAULT_ROS_DISTRO, is_ros2_package_available, require_ros2, require_ros2_cyclonedds},
};

/// The stock ROS 2 action server the nano-ros CLIENT direction drives.
const PEER_ACTION_SERVER_PKG: &str = "examples_rclcpp_minimal_action_server";

/// A domain no concurrent copy of this test will pick.
///
/// Same scheme as the shell harnesses and `nros_tests::unique_ros_domain_id`:
/// a fixed domain is a shared bus, and two overlapping runs discover each
/// other's writers — which reads as a delivery bug rather than a collision
/// (issue 0580).
fn action_domain() -> u8 {
    nros_tests::unique_ros_domain_id()
}

/// Single-quote a path for the shell `RosEnv::run` builds.
///
/// The fixture path comes from the build tree, so it is ours rather than a
/// user's — but quoting it is one line and an unquoted path with a space
/// fails as "command not found", which reads as a missing fixture.
fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Block until `/fibonacci` is on the bus, and FAIL — naming discovery — if it
/// never gets there.
///
/// This replaces a flat `sleep(6)`, which answers the wrong question in both
/// directions: when discovery is merely slow the sleep expires early and the
/// failure surfaces one layer down as a missing `Goal accepted`, which reads as
/// a wire-format defect — the exact misattribution this file exists to prevent
/// (see the `strip_goal_id_len_at` note below). When discovery is fast the
/// sleep is six seconds of nothing.
///
/// **NO `--no-daemon` here, and it is the one verb that cannot take it.**
/// Every other query helper in `nros_tests::ros2` passes it, for a real reason
/// — the ros2cli daemon is a singleton keyed on `ROS_DOMAIN_ID` alone and
/// serves a stale `RMW_IMPLEMENTATION` snapshot to whoever asks second — and
/// this helper copied the habit without checking. Measured on Humble:
/// `topic info`, `topic list`, `service list`, `param {list,get,set,describe}`,
/// `node {list,info}` all accept it; `ros2 action list` answers
/// `error: unrecognized arguments: --no-daemon` and its `-h` never mentions it.
///
/// Dropping it is safe HERE, which is not the same as safe: the hazard is two
/// RMWs sharing one domain, and every case in this file runs on its own
/// `unique_ros_domain_id()`, so the daemon it talks to is keyed to a domain
/// nobody else is using.
fn await_fibonacci_action(env: &HostRosEnv, whose: &str) {
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut last;
    loop {
        last = env
            .run_text("timeout --foreground 10 ros2 action list 2>&1")
            .unwrap_or_else(|e| format!("<`ros2 action list` failed: {e}>"));
        if last.lines().any(|l| l.trim() == "/fibonacci") {
            return;
        }
        // Fail FAST when the command itself could not run. Polling a usage
        // error 40 times spends the whole 20 s budget and then reports it as a
        // discovery timeout — which is how an unsupported FLAG spent a full
        // box run looking like an actions defect.
        if last.contains("unrecognized arguments")
            || last.contains("ros2: error:")
            || last.starts_with("<`ros2 action list` failed:")
        {
            panic!("`ros2 action list` could not run, so discovery was never measured:\n{last}");
        }
        if Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    panic!(
        "the {whose} `/fibonacci` action never appeared in `ros2 action list` \
         within 20 s, so the goal below could only have failed for a discovery \
         reason wearing a wire-format costume. Last listing:\n{last}"
    );
}

/// A real ROS 2 client drives a goal on the nano-ros action server.
///
/// Asserts the RESULT CONTENT, not merely that the call returned: the adapters
/// move bytes around inside the goal and result messages, so a wrong shape
/// shows up as a wrong sequence or a goal that is never accepted. Checking only
/// for a zero exit would pass on a server that accepted the goal and computed
/// nothing, which is the vacuous shape `check-no-vacuous-tests` exists for.
///
/// Interop cell: `native-action-rust-cyclone-r2n`.
#[test]
fn a_stock_ros2_client_drives_the_nano_ros_action_server() {
    if !require_ros2_cyclonedds() {
        nros_tests::skip!("ROS 2 + rmw_cyclonedds_cpp not available");
    }

    let server_bin = fixtures::build_native_rust_example_rmw(
        "action-server",
        "action-server",
        fixtures::Rmw::Cyclonedds,
    )
    .unwrap_or_else(|e| {
        nros_tests::skip!("native rust cyclonedds action-server fixture: {e}");
    });

    let domain = action_domain();
    let mut server_cmd = Command::new(&server_bin);
    server_cmd
        .env("RMW_IMPLEMENTATION", "rmw_cyclonedds_cpp")
        .env("ROS_DOMAIN_ID", domain.to_string());
    // Issue 1137 — the same bus as the `HostRosEnv` peer below. That env string
    // funnels through `ros2_env_setup_rmw_with_domain`, which has exported a
    // loopback-only `CYCLONEDDS_URI` since issue 1009; without the matching pin
    // here the two participants sit on different interfaces and `send_goal`
    // fails to discover the action at all.
    nros_tests::dds_isolation::apply_to_command(&mut server_cmd);
    let mut server = server_cmd
        .spawn()
        .expect("start the nano-ros action server");

    // `RosEnv`, not a hand-rolled `source /opt/ros/...` (RFC-0058). A second
    // spelling drifts from the first invisibly: the last one dropped the peer's
    // ROS_DOMAIN_ID, and another hardcoded `humble` so every guarded test
    // skipped forever on a jazzy host. `check-ros-env-spelling` caught this
    // file doing exactly that.
    let env = HostRosEnv::new(
        DEFAULT_ROS_DISTRO,
        Middleware::Cyclonedds { domain_id: domain },
    );

    // Discovery over DDS is not instant, and `send_goal` on an undiscovered
    // action fails rather than waiting.
    await_fibonacci_action(&env, "nano-ros");

    let out = env
        .run("timeout 60 ros2 action send_goal /fibonacci example_interfaces/action/Fibonacci '{order: 5}'")
        .expect("run ros2 action send_goal");

    // Whether the server was still ALIVE, asked before killing it. A server
    // that died mid-goal and a server that answered wrongly produce the same
    // missing `SUCCEEDED`, and the two want different investigations.
    let died = server.try_wait().ok().flatten();
    let _ = server.kill();
    let _ = server.wait();

    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(
        died.is_none(),
        "the nano-ros action server exited on its own ({died:?}) before the \
         goal finished — the client output below is about a dead peer, not \
         about the wire format.\n{text}"
    );
    assert!(
        text.contains("Goal accepted"),
        "a stock ROS 2 client must get the goal ACCEPTED — the server has to \
         read a real ROS 2 SendGoal request, UUID and all.\n\
         NOTE this does NOT exercise `strip_goal_id_len_at` (issue 0976): that \
         adapter fires only when nano-ros WRITES a SendGoal/GetResult request, \
         and here `ros2` writes them. What guards the server's receive side is \
         `split_wire_header`'s deliberate non-insertion (service.cpp:589-599, \
         issue #68). The direction where nano-ros writes them is the sibling \
         test below.\n{text}"
    );
    assert!(
        text.contains("SUCCEEDED"),
        "the goal must reach SUCCEEDED, not merely be accepted.\n{text}"
    );
    // Fibonacci(5) — the result travels the `_GetResult_Response_` path, which
    // is the one carrying the hand-built struct workaround (phase 171.0.b).
    for n in ["0", "1", "2", "3", "5"] {
        assert!(
            text.contains(&format!("- {n}")),
            "the result sequence must contain {n}; a reshaped result decodes \
             to the wrong numbers rather than failing.\n{text}"
        );
    }
}

/// The nano-ros action CLIENT drives a stock ROS 2 action server.
///
/// The reverse of the cell above, and not redundant with it: the adapters sit
/// on BOTH sides of the service path. Sending a goal exercises
/// `strip_goal_id_len_at` and `strip_nested_cdr_at` on what nano-ros WRITES;
/// receiving feedback and a result exercises the take path against bytes a real
/// ROS 2 server produced. One direction passing says nothing about the other —
/// which is the whole reason a nano-ros-to-nano-ros test could not settle this.
///
/// The peer is [`PEER_ACTION_SERVER_PKG`], which serves `/fibonacci` over
/// `example_interfaces/action/Fibonacci` — the same action and type the
/// nano-ros client targets.
///
/// Interop cell: `native-action-rust-cyclone-n2r`.
#[test]
fn the_nano_ros_action_client_drives_a_stock_ros2_server() {
    if !require_ros2_cyclonedds() {
        nros_tests::skip!("ROS 2 + rmw_cyclonedds_cpp not available");
    }
    // The peer is a SEPARATE apt package from the distro and from the RMW, so a
    // host that has both can still lack it. Without this guard the absence
    // lands as a failed `Goal accepted` — an unprovisioned host reported as a
    // wire-format red, which is the inverse of a false skip and just as
    // misleading (`HostRosEnv::available`'s reasoning, one package further out).
    if !is_ros2_package_available(DEFAULT_ROS_DISTRO, PEER_ACTION_SERVER_PKG) {
        nros_tests::skip!(
            "ROS 2 peer package `{PEER_ACTION_SERVER_PKG}` not installed \
             (apt: ros-{DEFAULT_ROS_DISTRO}-examples-rclcpp-minimal-action-server)"
        );
    }

    let client_bin = fixtures::build_native_rust_example_rmw(
        "action-client",
        "action-client",
        fixtures::Rmw::Cyclonedds,
    )
    .unwrap_or_else(|e| {
        nros_tests::skip!("native rust cyclonedds action-client fixture: {e}");
    });

    let domain = action_domain();
    let env = HostRosEnv::new(
        DEFAULT_ROS_DISTRO,
        Middleware::Cyclonedds { domain_id: domain },
    );

    // `RosPeer` kills the whole process group on drop, so a failed assertion
    // below cannot leave a `ros2` server behind to collide with the next run —
    // the orphan-peer shape that makes a later test fail for an earlier test's
    // reason.
    let mut server = env
        .spawn(
            "minimal_action_server",
            &format!("ros2 run {PEER_ACTION_SERVER_PKG} action_server_member_functions"),
        )
        .expect("start the stock ROS 2 action server");

    await_fibonacci_action(&env, "stock ROS 2");

    // Through the ROS env with an explicit `timeout`, NOT a bare
    // `Command::output()`. The client blocks waiting for a server it has not
    // discovered, and `output()` has no deadline — the first version of this
    // test hung for ten minutes instead of failing in one. A test that cannot
    // fail in bounded time is not a test.
    let out = env
        .run(&format!(
            "timeout 60 {}",
            shell_escape(&client_bin.to_string_lossy())
        ))
        .expect("run the nano-ros action client");

    let peer_alive = server.is_running();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    // Same reason as the forward cell: a peer that died mid-goal and a peer
    // that answered wrongly are one symptom and two different bugs.
    assert!(
        peer_alive,
        "the stock ROS 2 action server exited before the goal finished — the \
         client output below is about a dead peer, not about the wire \
         format.\n{text}"
    );
    assert!(
        text.contains("Goal accepted"),
        "a real ROS 2 server must ACCEPT the goal nano-ros wrote — this is the \
         side `strip_goal_id_len_at` and `strip_nested_cdr_at` reshape.\n{text}"
    );
    // Fibonacci(10). Asserted as the whole sequence: a reshaped result decodes
    // to wrong numbers rather than failing, so a substring like "Result" would
    // pass on garbage.
    //
    // Through `nros_tests::output::*`, not a literal — this half of the string
    // is OUR example's banner and the tree slims those (phase-277 broke ~10
    // tests grepping retired markers). The sequence after it is the payload and
    // stays written out here, because that is what the assertion is about.
    assert!(
        text.contains(&format!(
            "{ACTION_RESULT_PREFIX} [0, 1, 1, 2, 3, 5, 8, 13, 21, 34, 55]"
        )),
        "the result from a stock server must decode to Fibonacci(10).\n{text}"
    );
    // Feedback travels a different message than the result, and only the
    // feedback path exercises the server's own periodic publish.
    assert!(
        text.contains(ACTION_FEEDBACK_PREFIX),
        "feedback from a stock server must reach the nano-ros client.\n{text}"
    );
}

// ── phase-455 W3 — the same two directions over zenoh ───────────────────────
//
// Interop cells `native-action-rust-zenoh-r2n` / `-n2r`. The two Cyclone cases
// above were the whole of the ACTIONS family's live-peer coverage, so
// zenoh-pico — the backend whose reply-slot table phase-455 is about — had
// never met a stock ROS 2 peer on this path at all.
//
// THREE things differ from the Cyclone half, and each is a property of zenoh
// rather than a style choice:
//
// 1. **A router.** `rmw_zenoh_cpp` peers meet at a `rmw_zenohd`, not by
//    multicast discovery, so every case here starts one with
//    `ZenohRouter::start_unique` (ephemeral port, multicast off) and hands both
//    sides the same locator. That unique port — NOT a domain — is the
//    isolation: it is what keeps a concurrent run's traffic out.
//
// 2. **ROS domain 0, deliberately.** `unique_ros_domain_id()` cannot be used
//    here: the nano side's domain is a COMPILE-TIME bake
//    (`option_env!("NROS_DOMAIN_ID")`, read in `nros`'s build script), the
//    `linux/rust/zenoh` action fixtures bake no value, and the domain is the
//    FIRST segment of an `rmw_zenoh` keyexpr — so a peer on any other domain
//    simply never sees the node. Giving the peer a unique domain would produce
//    a silent no-discovery that reads exactly like a delivery bug. This is the
//    same choice `interop_e2e`'s zenoh cells make through
//    `ros2_env_setup_with_locator`, which passes 0 for the same reason.
//
//    The cost is that these two cases share the ros2cli daemon singleton, which
//    is keyed on `ROS_DOMAIN_ID` alone. That is why they belong to the
//    single-threaded `ros2-interop` nextest group rather than to
//    `host-dds-ros2-interop` with their Cyclone siblings — see the override in
//    `.config/nextest.toml`, which must sit ABOVE the `binary(=ros2_action_e2e)`
//    one because nextest takes the FIRST match per setting.
//
// 3. **No `dds_isolation`.** Issue 1009's profile pin is a Fast-DDS / Cyclone
//    mechanism; there is no RTPS participant anywhere in these two cases, and
//    applying it would pin nothing while reading as if it did.

/// The ROS domain the zenoh cells run on. See point 2 above — this is a
/// CHOICE, not an omission, and `unique_ros_domain_id()` would be wrong here.
const ZENOH_CELL_DOMAIN: u8 = 0;

/// A `HostRosEnv` whose `ros2` commands speak `rmw_zenoh_cpp` through `locator`.
fn zenoh_env(locator: &str) -> HostRosEnv {
    HostRosEnv::new(
        DEFAULT_ROS_DISTRO,
        Middleware::Zenoh {
            locator: locator.to_string(),
            domain_id: ZENOH_CELL_DOMAIN,
        },
    )
}

/// A real ROS 2 client drives a goal on the nano-ros zenoh action server.
///
/// The zenoh twin of `a_stock_ros2_client_drives_the_nano_ros_action_server`,
/// and not redundant with it: the two backends share no code on this path. The
/// Cyclone cell's subject is `service.cpp`'s five CDR adapters; this one's is
/// the zenoh-pico queryable a service server IS — where a SendGoal, a
/// GetResult and a CancelGoal each take one of the four
/// `ZPICO_MAX_PENDING_REPLIES` slots whose exhaustion issue 0902 measured and
/// phase-455 W1 gives an observable.
///
/// Interop cell: `native-action-rust-zenoh-r2n`.
#[test]
fn a_stock_ros2_client_drives_the_nano_ros_action_server_over_zenoh() {
    if !require_ros2() {
        nros_tests::skip!("ROS 2 + rmw_zenoh_cpp not available");
    }

    let server_bin = fixtures::build_native_rust_example_rmw(
        "action-server",
        "action-server",
        fixtures::Rmw::Zenoh,
    )
    .unwrap_or_else(|e| {
        nros_tests::skip!("native rust zenoh action-server fixture: {e}");
    });

    let router = fixtures::or_skip(fixtures::ZenohRouter::start_unique());
    let locator = router.locator();

    let mut server = Command::new(&server_bin)
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator)
        .spawn()
        .expect("start the nano-ros zenoh action server");

    let env = zenoh_env(&locator);

    // Same discovery gate as the Cyclone half, and the same reason: a flat
    // sleep turns a slow-discovery run into a missing `Goal accepted`, which
    // reads as a wire-format defect.
    await_fibonacci_action(&env, "nano-ros");

    let out = env
        .run("timeout 60 ros2 action send_goal /fibonacci example_interfaces/action/Fibonacci '{order: 5}'")
        .expect("run ros2 action send_goal");

    let died = server.try_wait().ok().flatten();
    let _ = server.kill();
    let _ = server.wait();

    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(
        died.is_none(),
        "the nano-ros zenoh action server exited on its own ({died:?}) before \
         the goal finished — the client output below is about a dead peer, not \
         about the wire format.\n{text}"
    );
    assert!(
        text.contains("Goal accepted"),
        "a stock ROS 2 client must get the goal ACCEPTED over zenoh — the \
         server has to answer the SendGoal query, which is one reply slot.\n{text}"
    );
    assert!(
        text.contains("SUCCEEDED"),
        "the goal must reach SUCCEEDED, not merely be accepted. A goal that is \
         accepted and never completes is issue 0902's exact symptom: the result \
         travels a SECOND query (GetResult) and therefore a second reply \
         slot.\n{text}"
    );
    // Fibonacci(5), asserted as values rather than as a marker: a result that
    // decoded wrongly produces numbers, not an error.
    for n in ["0", "1", "2", "3", "5"] {
        assert!(
            text.contains(&format!("- {n}")),
            "the result sequence must contain {n}; a reshaped result decodes \
             to the wrong numbers rather than failing.\n{text}"
        );
    }
}

/// The nano-ros zenoh action CLIENT drives a stock ROS 2 action server.
///
/// The reverse direction, and the one where nano-ros is the QUERIER rather than
/// the queryable — so it exercises the other half of the zenoh service path:
/// `zpico_get*`'s own reply handling, which is what the 18 existing
/// `zpico_get_diag_counters` entries already count, against bytes a real
/// `rmw_zenoh_cpp` server produced.
///
/// Interop cell: `native-action-rust-zenoh-n2r`.
#[test]
fn the_nano_ros_action_client_drives_a_stock_ros2_server_over_zenoh() {
    if !require_ros2() {
        nros_tests::skip!("ROS 2 + rmw_zenoh_cpp not available");
    }
    // Same separate-package guard as the Cyclone n2r cell: without it a host
    // that has ROS 2 and `rmw_zenoh_cpp` but not this peer reports a
    // wire-format red instead of a skip.
    if !is_ros2_package_available(DEFAULT_ROS_DISTRO, PEER_ACTION_SERVER_PKG) {
        nros_tests::skip!(
            "ROS 2 peer package `{PEER_ACTION_SERVER_PKG}` not installed \
             (apt: ros-{DEFAULT_ROS_DISTRO}-examples-rclcpp-minimal-action-server)"
        );
    }

    let client_bin = fixtures::build_native_rust_example_rmw(
        "action-client",
        "action-client",
        fixtures::Rmw::Zenoh,
    )
    .unwrap_or_else(|e| {
        nros_tests::skip!("native rust zenoh action-client fixture: {e}");
    });

    let router = fixtures::or_skip(fixtures::ZenohRouter::start_unique());
    let locator = router.locator();
    let env = zenoh_env(&locator);

    // `RosPeer` kills the whole process group on drop, so a failed assertion
    // cannot leave a `ros2` server behind for the next run to collide with.
    let mut server = env
        .spawn(
            "minimal_action_server",
            &format!("ros2 run {PEER_ACTION_SERVER_PKG} action_server_member_functions"),
        )
        .expect("start the stock ROS 2 action server");

    await_fibonacci_action(&env, "stock ROS 2");

    // Spawned DIRECTLY rather than through `env.run`, unlike the Cyclone
    // sibling: the nano side needs `NROS_LOCATOR` and nothing whatsoever from
    // the ROS environment, and running it under `source setup.bash` would put
    // a `LD_LIBRARY_PATH` it does not use in front of the binary. The
    // `timeout` is still mandatory and for the Cyclone case's reason — the
    // client blocks on an undiscovered server and a test that cannot fail in
    // bounded time is not a test.
    let out = Command::new("timeout")
        .arg("60")
        .arg(&client_bin)
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator)
        .output()
        .expect("run the nano-ros zenoh action client");

    let peer_alive = server.is_running();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(
        peer_alive,
        "the stock ROS 2 action server exited before the goal finished — the \
         client output below is about a dead peer, not about the wire \
         format.\n{text}"
    );
    assert!(
        text.contains("Goal accepted"),
        "a real `rmw_zenoh_cpp` server must ACCEPT the goal nano-ros wrote.\n{text}"
    );
    assert!(
        text.contains(&format!(
            "{ACTION_RESULT_PREFIX} [0, 1, 1, 2, 3, 5, 8, 13, 21, 34, 55]"
        )),
        "the result from a stock server must decode to Fibonacci(10). A client \
         that prints feedback and never a result is the nano-ros-side shape of \
         issue 0902 — accepted, then nothing.\n{text}"
    );
    assert!(
        text.contains(ACTION_FEEDBACK_PREFIX),
        "feedback from a stock server must reach the nano-ros client.\n{text}"
    );
}

// phase-433 W6 — bind this file to `interop::CELLS`. Directions collapse (per
// `interop::coords_for`), so each BACKEND is one entry however many directions
// the file grows: four cells, two coordinates. phase-455 W3 added the zenoh
// one. Drift between the cases here and the rows there turns this RED. Needs no
// fixtures and no ROS 2 — it runs in tier 1.
#[test]
fn cases_bound_to_interop_cells() {
    #[allow(unused_imports)]
    use nros_tests::matrix::{Lang::*, PlatformId::*, Rmw::*, Workload::*};
    nros_tests::interop::assert_test_bound(
        "ros2_action_e2e",
        &[
            (Linux, Rust, Cyclonedds, Action),
            (Linux, Rust, Zenoh, Action),
        ],
    );
}

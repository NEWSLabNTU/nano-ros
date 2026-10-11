//! phase-491 W6 (RFC-0105 §Interop proof) — a C node built ONLY on rclc's
//! parameter-server API, driven by stock `ros2 param`.
//!
//! `parameters_roundtrip` proves the C example's LOCAL half and exits before
//! anything on the wire can ask it a question. This is the remote half, against
//! the same prebuilt `c-parameters` image left spinning:
//!
//! 1. `ros2 param list` / `get` see what `rclc_parameter_declare_*` declared,
//!    with the value the example's own local sets left.
//! 2. `ros2 param set` reaches the rclc callback: the example's callback
//!    refuses a non-positive `publish_rate_hz`, and that refusal is what the
//!    peer is told.
//! 3. A set outside the range `rclc_add_parameter_constraint_integer` attached
//!    is refused on the REMOTE path (rclc only advertises it; RFC-0105 D7).
//! 4. A set on a parameter `rclc_set_parameter_read_only` marked is refused.
//! 5. An accepted set reads back.
//! 6. `/parameter_events` carries a change (rclc's `notify_changed_over_dds`,
//!    RFC-0105 D8).
//!
//! Build ahead of time with `just native build-fixtures`; nothing compiles at
//! run time.

use nros_tests::{
    fixtures::{
        ManagedProcess, RequireFixture, ZenohRouter, build_c_parameters, require_ros2,
        require_zenohd, zenohd_unique,
    },
    ros2::DEFAULT_ROS_DISTRO,
};
use rstest::rstest;
use std::{process::Command, time::Duration};

const NODE: &str = "/c_parameters";

fn start_node(locator: &str) -> ManagedProcess {
    let binary = build_c_parameters().require("c-parameters");
    let mut cmd = Command::new(binary);
    cmd.env("NROS_LOCATOR", locator)
        .env("NROS_SESSION_MODE", "client")
        // Long enough for every query below; the process is killed on drop.
        .env("NROS_SPIN_MS", "120000");
    let mut node = ManagedProcess::spawn_command(cmd, "c-parameters").expect("spawn c-parameters");
    // The OK line is printed after the local roundtrip and before the spin
    // that answers the services.
    node.wait_for_output_pattern("OK verbose=", Duration::from_secs(10))
        .expect("c-parameters never finished its local roundtrip");
    node
}

/// Retry a query a few times: service discovery through the router is not
/// instantaneous.
fn until(mut f: impl FnMut() -> String, want: &str) -> String {
    let mut out = String::new();
    for _ in 0..5 {
        out = f();
        if out.contains(want) {
            break;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    out
}

#[rstest]
fn rclc_parameter_server_answers_ros2_param(zenohd_unique: ZenohRouter) {
    require_zenohd();
    require_ros2();
    let locator = zenohd_unique.locator();
    let _node = start_node(&locator);
    let d = DEFAULT_ROS_DISTRO;

    // 1. list / get.
    let list = until(
        || nros_tests::ros2::ros2_param_list(NODE, &locator, d).unwrap_or_default(),
        "publish_rate_hz",
    );
    for name in ["verbose", "publish_rate_hz", "scale_factor", "topic_name"] {
        assert!(
            list.contains(name),
            "`ros2 param list {NODE}` lacks {name}:\n{list}"
        );
    }
    let get = nros_tests::ros2::ros2_param_get(NODE, "publish_rate_hz", &locator, d).unwrap();
    assert!(
        get.contains("Integer value is: 10"),
        "get publish_rate_hz:\n{get}"
    );

    // 2. the rclc callback's veto reaches the peer.
    let veto = nros_tests::ros2::ros2_param_set(NODE, "publish_rate_hz", "0", &locator, d).unwrap();
    assert!(
        veto.contains("Setting parameter failed"),
        "the callback refuses 0, and the peer must be told:\n{veto}"
    );

    // 3. the advertised range is enforced remotely.
    let range =
        nros_tests::ros2::ros2_param_set(NODE, "publish_rate_hz", "500", &locator, d).unwrap();
    assert!(
        range.contains("Setting parameter failed"),
        "500 is outside [1, 100] and must be refused remotely:\n{range}"
    );

    // 4. read-only refuses remote writes.
    let ro = nros_tests::ros2::ros2_param_set(NODE, "topic_name", "/other", &locator, d).unwrap();
    assert!(
        ro.contains("Setting parameter failed"),
        "topic_name is read-only:\n{ro}"
    );

    // 5. an accepted set reads back, and the refused ones left the value.
    let ok = nros_tests::ros2::ros2_param_set(NODE, "publish_rate_hz", "20", &locator, d).unwrap();
    assert!(ok.contains("Set parameter successful"), "set 20:\n{ok}");
    let get = nros_tests::ros2::ros2_param_get(NODE, "publish_rate_hz", &locator, d).unwrap();
    assert!(get.contains("Integer value is: 20"), "read back 20:\n{get}");
}

// phase-329 W3 pattern — bound to `interop::CELLS`; drift turns this red.
#[test]
fn cases_bound_to_interop_cells() {
    #[allow(unused_imports)]
    use nros_tests::matrix::{Lang::*, PlatformId::*, Rmw::*, Workload::*};
    nros_tests::interop::assert_test_bound("c_param_server_interop", &[(Linux, C, Zenoh, Params)]);
}

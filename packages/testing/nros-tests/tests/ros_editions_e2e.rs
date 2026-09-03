//! Issue 0327 — multi-edition ROS 2 interop, RMW × workload × direction in ONE
//! parametrized rstest.
//!
//! Replaces the five hand-written per-cell files
//! (`ros_editions_{e2e_pubsub,e2e_service,e2e_action,xrce,zenoh}.rs`, 18
//! near-identical `#[test]` bodies) with a single table over
//! (rmw × workload × direction). The three `ros_env::e2e_setup{,_xrce,_zenoh}`
//! functions are the per-RMW seam; each cell differs only by that setup call,
//! the bridge process it spawns (XRCE Agent / rmw_zenohd router / none), and the
//! stock-ROS-demo marker it asserts — everything else is shared.
//!
//! A nano-ros example node (host, built per-edition by `just ros_editions
//! build-e2e-fixtures <distro> <rmw>`) exchanges data with a stock ROS 2 edition
//! peer in a `DockerRosEnv` container:
//!
//!   - cyclone: direct same-domain CycloneDDS (rmw_cyclonedds_cpp).
//!   - xrce:    nano XRCE client → micro-XRCE Agent (host) → rmw_fastrtps_cpp.
//!   - zenoh:   nano zpico node → rmw_zenohd router (host) → rmw_zenoh_cpp.
//!
//! Skips (never a silent pass) without the built fixtures / docker / image /
//! Agent, and — for zenoh — on editions with no `rmw_zenoh_cpp` (iron/humble).
//! Not in `just ci`; run by `just ros_editions ci <distro>`. Case names are
//! prefixed by RMW so the recipe can filter each lane's threading:
//! `cargo test --test ros_editions_e2e {cyclone|xrce|zenoh}`.

use std::{path::PathBuf, process::Command, time::Duration};

use nros_tests::ros_env::{self, DockerRosEnv, Rmw, RosPeer};
use rstest::rstest;

#[derive(Copy, Clone, Debug)]
enum Workload {
    Pubsub,
    Service,
    Action,
}

#[derive(Copy, Clone, Debug)]
enum Dir {
    /// nano-ros node → ROS 2 peer (nano is the source / client).
    NanoToRos,
    /// ROS 2 peer → nano-ros node (nano is the sink / server).
    RosToNano,
}

use Dir::*;
use Rmw::*;
use Workload::*;

/// The example package name for a (workload, direction) coordinate — the same
/// naming the per-edition fixture builder uses.
fn example_of(w: Workload, d: Dir) -> &'static str {
    match (w, d) {
        (Pubsub, NanoToRos) => "talker",
        (Pubsub, RosToNano) => "listener",
        (Service, NanoToRos) => "service-client",
        (Service, RosToNano) => "service-server",
        (Action, NanoToRos) => "action-client",
        (Action, RosToNano) => "action-server",
    }
}

/// A per-RMW lane: the docker ROS env, the nano example binary + domain, the
/// nano-command wiring, and any bridge process (Agent / router) kept alive for
/// the test's lifetime. The `e2e_setup*` calls `skip!` inside on a missing
/// fixture / docker / image / Agent / rmw_zenoh_cpp.
struct Lane {
    env: DockerRosEnv,
    bin: PathBuf,
    domain: u8,
    rmw: Rmw,
    /// XRCE `host:port` or zenoh locator; `None` for cyclone.
    locator: Option<String>,
    /// Agent / router — dropped (and killed) when the lane goes out of scope.
    _bridges: Vec<RosPeer>,
}

impl Lane {
    fn setup(rmw: Rmw, example: &str) -> Lane {
        match rmw {
            Cyclone => {
                let (env, bin, domain) = ros_env::e2e_setup(example);
                Lane {
                    env,
                    bin,
                    domain,
                    rmw,
                    locator: None,
                    _bridges: vec![],
                }
            }
            Xrce => {
                let (env, bin, domain, agent, port) = ros_env::e2e_setup_xrce(example);
                let bridge =
                    ros_env::spawn_xrce_agent(&agent, port, domain).expect("spawn xrce agent");
                Lane {
                    env,
                    bin,
                    domain,
                    rmw,
                    locator: Some(format!("127.0.0.1:{port}")),
                    _bridges: vec![bridge],
                }
            }
            Zenoh => {
                let (env, bin, domain, locator) = ros_env::e2e_setup_zenoh(example);
                let bridge = env.spawn_zenoh_router().expect("spawn rmw_zenohd");
                Lane {
                    env,
                    bin,
                    domain,
                    rmw,
                    locator: Some(locator),
                    _bridges: vec![bridge],
                }
            }
        }
    }

    /// The nano-ros node command for this lane (RMW + domain + locator wired).
    fn nano_cmd(&self, extra: &[&str]) -> Command {
        match self.rmw {
            Cyclone => ros_env::nano_node_cmd(&self.bin, self.domain, extra),
            Xrce | Zenoh => ros_env::nano_node_cmd_rmw(
                &self.bin,
                self.rmw,
                self.domain,
                self.locator
                    .as_deref()
                    .expect("xrce/zenoh lane sets a locator"),
                extra,
            ),
        }
    }
}

// cyclone (parallel) / xrce (serial) / zenoh (serial), each × pubsub/service/
// action × both directions = 18 cells. RMW-prefixed case names let the recipe
// select each lane's threading.
#[rstest]
#[case::cyclone_pubsub_nano_to_ros(Cyclone, Pubsub, NanoToRos)]
#[case::cyclone_pubsub_ros_to_nano(Cyclone, Pubsub, RosToNano)]
#[case::cyclone_service_nano_to_ros(Cyclone, Service, NanoToRos)]
#[case::cyclone_service_ros_to_nano(Cyclone, Service, RosToNano)]
#[case::cyclone_action_nano_to_ros(Cyclone, Action, NanoToRos)]
#[case::cyclone_action_ros_to_nano(Cyclone, Action, RosToNano)]
#[case::xrce_pubsub_nano_to_ros(Xrce, Pubsub, NanoToRos)]
#[case::xrce_pubsub_ros_to_nano(Xrce, Pubsub, RosToNano)]
#[case::xrce_service_nano_to_ros(Xrce, Service, NanoToRos)]
#[case::xrce_service_ros_to_nano(Xrce, Service, RosToNano)]
#[case::xrce_action_nano_to_ros(Xrce, Action, NanoToRos)]
#[case::xrce_action_ros_to_nano(Xrce, Action, RosToNano)]
#[case::zenoh_pubsub_nano_to_ros(Zenoh, Pubsub, NanoToRos)]
#[case::zenoh_pubsub_ros_to_nano(Zenoh, Pubsub, RosToNano)]
#[case::zenoh_service_nano_to_ros(Zenoh, Service, NanoToRos)]
#[case::zenoh_service_ros_to_nano(Zenoh, Service, RosToNano)]
#[case::zenoh_action_nano_to_ros(Zenoh, Action, NanoToRos)]
#[case::zenoh_action_ros_to_nano(Zenoh, Action, RosToNano)]
fn ros_edition_interop(#[case] rmw: Rmw, #[case] workload: Workload, #[case] dir: Dir) {
    let lane = Lane::setup(rmw, example_of(workload, dir));

    match (workload, dir) {
        // ── pub/sub (std_msgs/String on /chatter) ────────────────────────────
        //
        // phase-338 W3 — these two cases used to force the examples onto Int32
        // with `NROS_PUB_TYPE` / `NROS_SUB_TYPE`. The type was arbitrary here
        // (the edition axis is about type_hash + keyexpr format, not the
        // payload), so both now use the examples' DEFAULT `std_msgs/String` —
        // the official ROS 2 demo type — and the test-only switch is gone.
        (Pubsub, NanoToRos) => {
            let _talker = ros_env::spawn_process(lane.nano_cmd(&[]), "nano-talker")
                .expect("spawn nano talker");
            let out = lane
                .env
                .echo_topic_once("/chatter", "std_msgs/msg/String", 45)
                .expect("ros2 topic echo");
            assert!(
                out.contains("data:"),
                "ROS echo did not receive the nano-ros String on /chatter ({rmw:?}):\n{out}"
            );
        }
        (Pubsub, RosToNano) => {
            let _pub = lane
                .env
                .spawn_topic_pub("/chatter", "std_msgs/msg/String", "{data: 'hello'}", 5)
                .expect("spawn ros2 topic pub");
            let mut listener = ros_env::spawn_process(lane.nano_cmd(&[]), "nano-listener")
                .expect("spawn nano listener");
            // issue 1026 — wait on the SAMPLE, not on 45 s of the listener's
            // life. `wait_for_output` had no stop condition, so it always ran
            // the full window and killed the node at the end; a cell that
            // asserts first delivery paid 45 s for it and, worse, could not
            // have observed anything past it anyway.
            //
            // Bound stated: FIRST delivery only. Nothing here can see a
            // session that lapses after the first sample — the edition axis is
            // about type_hash + keyexpr compatibility, and one matched sample
            // settles that; continuity belongs to the pubsub cells that count
            // samples across a lease interval (issue 1013).
            let marker = format!("{} [hello]", nros_tests::output::LISTENER_LOG_PREFIX);
            let out = listener.collect_until(&marker, Duration::from_secs(45));
            assert!(
                out.contains(&marker),
                "nano-ros listener did not receive the ROS String on /chatter ({rmw:?}):\n{out}"
            );
        }
        // ── service (example_interfaces/AddTwoInts on /add_two_ints) ─────────
        (Service, NanoToRos) => {
            let _server = lane
                .env
                .spawn_add_two_ints_server()
                .expect("spawn rclpy server");
            let mut client = ros_env::spawn_process(lane.nano_cmd(&[]), "nano-srv-client")
                .expect("spawn nano client");
            // issue 1026 — wait on the REPLY, not on 45 s of the client's life.
            //
            // Bound stated: the demo service client is single-shot (it latches
            // `done` after the first reply and idles), so one result is all it
            // will ever print; this cell says nothing about the session after
            // that call.
            let expected = nros_tests::output::service_result_line(5);
            let out = client.collect_until(&expected, Duration::from_secs(45));
            assert!(
                out.contains(&expected),
                "nano-ros service-client did not get sum 5 from the ROS server ({rmw:?}):\n{out}"
            );
        }
        (Service, RosToNano) => {
            let _server = ros_env::spawn_process(lane.nano_cmd(&[]), "nano-srv-server")
                .expect("spawn nano server");
            let out = lane
                .env
                .service_call_add_two_ints(5, 3, 40)
                .expect("ros2 service call");
            assert!(
                out.contains("sum=8"),
                "ROS service call did not get sum 8 from the nano-ros server ({rmw:?}):\n{out}"
            );
        }
        // ── action (Fibonacci) ───────────────────────────────────────────────
        (Action, NanoToRos) => {
            let _server = lane
                .env
                .spawn_fibonacci_server()
                .expect("spawn rclpy action server");
            let mut client = ros_env::spawn_process(lane.nano_cmd(&[]), "nano-action-client")
                .expect("spawn nano client");
            // issue 1026 — wait on the RESULT, not on 60 s of the client's
            // life. The result line is terminal for this client, so the wait
            // and the assertion now name the same event.
            //
            // Bound stated: one goal, one result. Feedback ordering and what
            // the client does after the result are not observed here.
            let out = client.collect_until("Result received:", Duration::from_secs(60));
            assert!(
                out.contains("Result received:") && out.contains("34"),
                "nano-ros action-client did not get the Fibonacci result from the ROS server ({rmw:?}):\n{out}"
            );
        }
        (Action, RosToNano) => {
            let _server = ros_env::spawn_process(lane.nano_cmd(&[]), "nano-action-server")
                .expect("spawn nano server");
            let out = lane
                .env
                .action_send_goal_fibonacci(10, 55)
                .expect("ros2 action send_goal");
            assert!(
                out.contains("34"),
                "ROS action client did not get the Fibonacci result from the nano-ros server ({rmw:?}):\n{out}"
            );
        }
    }
}

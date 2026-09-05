//! A stand-in simulator: publishes `rosgraph_msgs/msg/Clock` on `/clock`.
//!
//! phase-425 W5. Every other sim-time test installs a ROS time by calling the
//! setter, which proves the timer arithmetic and says nothing about the wire.
//! This is the other half: a real publisher, a real topic, a real RMW, and a
//! separate process — the shape `ros2 bag play --clock` and every simulator
//! actually present to a node.
//!
//! It does not simulate anything. It advances a counter, which is all `/clock`
//! ever carries.
//!
//! * `NROS_SIM_RATE` — simulated milliseconds per real millisecond. Default 10,
//!   i.e. the bag plays at 10x. Integer; 0 means "publish the same time
//!   forever", which is a legitimate thing for a paused simulator to do.
//! * `NROS_SIM_STEP_MS` — real interval between samples. Default 10. Each
//!   sample advances simulated time by `STEP_MS * RATE`, so a CONSUMER with a
//!   `period_ms` timer needs `STEP_MS * RATE <= period_ms` to see the timer
//!   tick at the replay rate: `TimerOverrunPolicy::Skip` coalesces a jump worth
//!   several periods into one activation. Publish coarsely and the clock still
//!   advances at the full rate while the tick rate collapses to 1x — correct,
//!   and confusing if you were expecting otherwise.
//! * `NROS_SIM_RUN_MS` — publish for this long, then STOP and idle. Default
//!   3000. The stop is the point: a listener that keeps ticking afterwards is
//!   not on the simulator's clock.
//! * `NROS_SIM_EPOCH_MS` — simulated time of the first sample. Default 0.

use log::info;
use nros::prelude::*;
use nros_rosgraph_msgs::msg::Clock;

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn main() {
    env_logger::init();
    nros_board_linux::register_linked_rmw();

    let rate = env_u64("NROS_SIM_RATE", 10);
    let step_ms = env_u64("NROS_SIM_STEP_MS", 10).max(1);
    let run_ms = env_u64("NROS_SIM_RUN_MS", 3000);
    let epoch_ms = env_u64("NROS_SIM_EPOCH_MS", 0);

    info!("nros Sim Clock Publisher (test fixture)");
    info!("SIMCLOCK_PUB_CONFIG rate={rate} step_ms={step_ms} run_ms={run_ms}");

    let ctx = nros::init_with_launch_auto().expect("nros init failed");
    let cfg = ctx.config("sim_clock");
    let mut executor: Executor = Executor::open(&cfg).expect("Failed to open session");
    let nid = executor
        .node_builder("sim_clock")
        .build()
        .expect("Failed to build node");

    // `ClockQoS` on the publisher side too: best effort, keep-last 1. A
    // simulator that has to retransmit an old tick is worse than one that skips
    // it, which is why rclcpp spells this profile the way it does.
    let pub_ = executor
        .node_mut(nid)
        .publisher("/clock")
        .typed::<Clock>()
        .qos(nros::QoSProfile::clock_default())
        .build()
        .expect("Failed to create /clock publisher");

    let mut sim_ms = epoch_ms;
    let mut published = 0u64;
    let mut elapsed_ms = 0u64;

    while elapsed_ms < run_ms {
        let msg = Clock {
            clock: nros_builtin_interfaces_clock::msg::Time {
                sec: (sim_ms / 1000) as i32,
                nanosec: ((sim_ms % 1000) * 1_000_000) as u32,
            },
        };
        if pub_.publish(&msg).is_ok() {
            published += 1;
        }
        executor.spin_once(core::time::Duration::from_millis(step_ms));
        elapsed_ms += step_ms;
        sim_ms += step_ms * rate;
    }

    // The marker the test waits for before it starts measuring the SILENT half.
    info!("SIMCLOCK_PUB_STOPPED published={published} sim_ms={sim_ms}");

    // Keep the session alive and publish NOTHING. Exiting would take the
    // participant out of the graph, and a listener that stops ticking because
    // its peer vanished has demonstrated nothing about `/clock`.
    let idle_ms = env_u64("NROS_SIM_IDLE_MS", 4000);
    let mut idled = 0u64;
    while idled < idle_ms {
        executor.spin_once(core::time::Duration::from_millis(step_ms));
        idled += step_ms;
    }
    info!("SIMCLOCK_PUB_DONE");
}

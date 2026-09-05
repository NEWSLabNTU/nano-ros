//! A node whose timers can be told apart by which clock drives them.
//!
//! phase-425 W5, the end-to-end half. Declares `use_sim_time` true and lets the
//! runtime attach the `/clock` source — no explicit `install_ros_time_source`
//! call, because the claim under test is the ROS-facing one: a parameter, from
//! a launch file or a params YAML, is enough.
//!
//! Two timers of the SAME period run side by side:
//!
//! * a `TimerClockSource::Ros` timer, which advances with `/clock`;
//! * a wall timer, which advances with the platform's monotonic clock.
//!
//! One line per real second carries both counts, so a reader can see the ratio
//! move when a simulator speeds up and collapse when it stops. Two counts from
//! one process, on one executor, is what makes this evidence rather than a
//! measurement of the machine's load: the wall timer IS the control.
//!
//! * `NROS_SIM_PERIOD_MS` — the period both timers run at. Default 100.
//! * `NROS_SIM_OBSERVE_MS` — how long to run. Default 7000.

use log::info;
use nros::prelude::*;
// Everything comes through the umbrella, deliberately: a fixture that reached
// into `nros-node` would prove the capability exists in the core and not that
// an application can name it — which is exactly the gap this fixture found
// (`sim-time` and `TimerClockSource` were both unreachable from `nros`).
use nros::{TimerClockSource, TimerDuration};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn main() {
    env_logger::init();
    nros_board_linux::register_linked_rmw();

    let period_ms = env_u64("NROS_SIM_PERIOD_MS", 100).max(1);
    let observe_ms = env_u64("NROS_SIM_OBSERVE_MS", 7000);

    info!("nros Sim Clock Listener (test fixture)");

    let ctx = nros::init_with_launch_auto().expect("nros init failed");
    let cfg = ctx.config("sim_listener");
    let mut executor: Executor = Executor::open(&cfg).expect("Failed to open session");

    // THE POINT OF THE FIXTURE. Nothing below asks for a time source; this
    // parameter is the whole request, exactly as in ROS 2 — the runtime
    // reconciles it on the next spin, once a node exists to hang the
    // subscription on.
    executor.declare_parameter("use_sim_time", nros::ParameterValue::Bool(true));

    let _nid = executor
        .node_builder("sim_listener")
        .build()
        .expect("Failed to build node");

    let ros_ticks = Arc::new(AtomicU64::new(0));
    let wall_ticks = Arc::new(AtomicU64::new(0));
    let r = ros_ticks.clone();
    let w = wall_ticks.clone();

    executor
        .register_timer_on_clock(
            TimerDuration::from_millis(period_ms),
            TimerClockSource::Ros,
            move || {
                r.fetch_add(1, Ordering::Relaxed);
            },
        )
        .expect("Failed to register the ROS-time timer");
    executor
        .register_timer(TimerDuration::from_millis(period_ms), move || {
            w.fetch_add(1, Ordering::Relaxed);
        })
        .expect("Failed to register the wall timer");

    let started = std::time::Instant::now();
    let mut next_report = std::time::Duration::from_secs(1);
    while started.elapsed() < std::time::Duration::from_millis(observe_ms) {
        // 5 ms, not the timer period: the ROS timer can only fire on a spin, so
        // a spin quantum equal to the period would cap its rate at the wall
        // timer's and the ratio this fixture measures could never exceed 1.
        executor.spin_once(core::time::Duration::from_millis(5));
        if started.elapsed() >= next_report {
            report(&executor, &ros_ticks, &wall_ticks, next_report.as_secs());
            next_report += std::time::Duration::from_secs(1);
        }
    }
    report(&executor, &ros_ticks, &wall_ticks, next_report.as_secs());
    info!("SIMCLOCK_LISTENER_DONE");
}

fn report(executor: &Executor, ros: &AtomicU64, wall: &AtomicU64, t: u64) {
    let clock = nros::Clock::ros_time();
    // `active` is the predicate a user would read (`ros_time_is_active`): it is
    // false until a sample lands, so the first line or two legitimately say
    // `active=false` while discovery completes.
    info!(
        "SIMCLOCK t={t} ros={} wall={} ros_now_ms={} active={} attached={}",
        ros.load(Ordering::Relaxed),
        wall.load(Ordering::Relaxed),
        clock.now().to_nanos() / 1_000_000,
        nros::Clock::is_ros_time_override_active(),
        executor.ros_time_source_installed(),
    );
}

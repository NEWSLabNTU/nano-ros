//! Phase 210.D.3 — Rust mixed-workspace consumer.
//!
//! Imports msgs from BOTH worlds via the auto-managed
//! `[patch.crates-io]` block in this pkg's Cargo.toml (written by
//! `nros ws sync`):
//!
//!   * `local_msgs::msg::Greeting`      — workspace pkg
//!   * `extra_msgs::msg::Echo`          — workspace pkg, depends on local_msgs
//!   * `geometry_msgs::msg::Point`      — AMENT
//!   * `sensor_msgs::msg::Imu`          — AMENT (transitively pulls std_msgs +
//!                                        geometry_msgs)
//!
//! Build:
//!
//!   $ cd <fixture>
//!   $ NROS_REPO_DIR=<nano-ros-root> nros ws sync
//!   $ cd src/rust_consumer && cargo build      # plain cargo, no wrapper
//!
//! Run (zenoh router must be up):
//!
//!   $ zenohd --listen tcp/127.0.0.1:7447 &
//!   $ ./target/debug/rust_consumer

use log::info;
use nros::prelude::*;

use extra_msgs::msg::Echo;
use geometry_msgs::msg::Point;
use local_msgs::msg::Greeting;
use sensor_msgs::msg::Imu;

fn main() {
    env_logger::init();
    info!("rust_consumer — workspace + AMENT msg coverage proof");

    nros_rmw_zenoh::register().expect("zenoh register failed");

    let locator =
        std::env::var("NROS_LOCATOR").unwrap_or_else(|_| "tcp/127.0.0.1:7447".to_string());
    let config = ExecutorConfig::new(&locator)
        .node_name("rust_consumer")
        .domain_id(0);
    let mut executor: Executor = Executor::open(&config).expect("open executor");
    let mut node = executor
        .create_node("rust_consumer")
        .expect("create node");

    let greet_pub = node
        .create_publisher::<Greeting>("/greetings")
        .expect("create greetings pub");
    let echo_pub = node
        .create_publisher::<Echo>("/echoes")
        .expect("create echoes pub");
    let point_pub = node
        .create_publisher::<Point>("/points")
        .expect("create points pub");
    let imu_pub = node
        .create_publisher::<Imu>("/imu")
        .expect("create imu pub");

    info!("publishing 5 ticks then exiting");
    for seq in 0..5 {
        let mut g = Greeting::default();
        g.sequence = seq;
        let _ = greet_pub.publish(&g);

        let mut e = Echo::default();
        e.original = g;
        e.hop_count = 1;
        let _ = echo_pub.publish(&e);

        let mut p = Point::default();
        p.x = seq as f64;
        p.y = (seq * 2) as f64;
        p.z = (seq * 3) as f64;
        let _ = point_pub.publish(&p);

        let mut imu = Imu::default();
        imu.linear_acceleration.x = 9.81;
        let _ = imu_pub.publish(&imu);

        info!("tick {seq}");
        executor.spin_once(std::time::Duration::from_millis(500));
    }
    info!("done.");
}

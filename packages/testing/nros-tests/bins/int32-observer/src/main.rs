//! phase-276 W5 — generic `std_msgs/Int32` topic observer.
//!
//! A raw zenoh subscriber on an env-selected topic that logs `Received:` per
//! sample. The cross-process assertion half for embedded-image e2es (e.g. the
//! `ws-qos-rust` Zephyr entry) whose on-target nodes republish a counter on an
//! observer topic: the paired test spawns this bin against the same router,
//! boots the image, and asserts N `Received:` lines — proving the on-target
//! delivery path reached the wire. Raw subscription (no generated message
//! crate), same approach as the `qos-override-pubsub` fixture.
//!
//! * `NROS_LOCATOR` — zenoh locator. Default `tcp/127.0.0.1:7447`.
//! * `NROS_TOPIC` — topic to observe. Default `/qos_ok`.

use core::time::Duration;

use log::info;
use nros::{Executor, ExecutorConfig};

const TYPE_NAME: &str = "std_msgs::msg::dds_::Int32_";
const TYPE_HASH: &str = "TypeHashNotSupported";

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    nros_rmw_zenoh::register().expect("register zenoh backend");

    let locator = std::env::var("NROS_LOCATOR").unwrap_or_else(|_| "tcp/127.0.0.1:7447".into());
    let topic = std::env::var("NROS_TOPIC").unwrap_or_else(|_| "/qos_ok".into());

    let cfg = ExecutorConfig::new(&locator)
        .node_name("int32_observer")
        .namespace("/");
    let mut exec = Executor::open_with_rmw("zenoh", &cfg).expect("open zenoh session");

    let mut sub = {
        let mut node = exec.create_node("int32_observer").expect("create node");
        node.create_subscription_raw(&topic, TYPE_NAME, TYPE_HASH)
            .expect("create raw Int32 subscription")
    };

    info!("Observing {topic}...");
    loop {
        let _ = exec.spin_once(Duration::from_millis(10));
        while let Ok(Some(n)) = sub.try_recv_raw() {
            info!("Received: {n} bytes");
        }
    }
}

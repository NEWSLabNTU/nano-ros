//! Phase 211 acceptance — nano-ros ↔ real ROS 2 (`demo_nodes_cpp`) interop.
//!
//! A raw `std_msgs/String` subscriber on `/chatter`, the topic + type the stock
//! `ros2 run demo_nodes_cpp talker` publishes ("Hello World: N" at 1 Hz). Run
//! against that talker over `rmw_zenoh_cpp` (both joined to one `zenohd`) to
//! prove a nano-ros node interoperates with an UNMODIFIED upstream ROS 2 node on
//! the ROS graph — the "behaves like real ROS production" half of the pipeline
//! the synthetic `demo_bringup` fixture can't show.
//!
//! Raw subscription (no generated msg crate): the keyexpr is
//! `<domain>/chatter/std_msgs::msg::dds_::String_/<hash>`; the bytes are the
//! talker's CDR-encoded String, which we don't decode — the assertion is that
//! cross-vendor bytes arrive. Mirrors the `bridge-zenoh-to-xrce-fwd` /
//! `qos-override-pubsub` raw approach.
//!
//! Env: `NROS_LOCATOR` (zenoh locator, default `tcp/127.0.0.1:7447`).

use core::time::Duration;

use log::{error, info};
use nros::{Executor, ExecutorConfig};

const TOPIC: &str = "/chatter";
const TYPE_NAME: &str = "std_msgs::msg::dds_::String_";
const TYPE_HASH: &str = "TypeHashNotSupported";

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    nros_rmw_zenoh::register().expect("register zenoh backend");

    let locator = std::env::var("NROS_LOCATOR").unwrap_or_else(|_| "tcp/127.0.0.1:7447".into());
    info!("=== Phase 211 demo_nodes_cpp interop: nano-ros String sub on {TOPIC} ===");

    let cfg = ExecutorConfig::new(&locator)
        .node_name("nros_string_sub")
        .namespace("/");
    let mut exec = Executor::open_with_rmw("zenoh", &cfg).expect("open zenoh session");

    let mut sub = {
        let mut node = exec.create_node("nros_string_sub").expect("create node");
        node.create_subscription_raw(TOPIC, TYPE_NAME, TYPE_HASH)
            .unwrap_or_else(|e| {
                error!("subscription create failed: {e:?}");
                std::process::exit(3);
            })
    };
    info!("Waiting for std_msgs/String on {TOPIC} (run `ros2 run demo_nodes_cpp talker`)...");

    loop {
        let _ = exec.spin_once(Duration::from_millis(10));
        while let Ok(Some(n)) = sub.try_recv_raw() {
            info!("Received: {n} bytes");
        }
    }
}

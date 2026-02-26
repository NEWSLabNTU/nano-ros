//! # ROS 2 Interoperability
//!
//! nros communicates with standard ROS 2 nodes via `rmw_zenoh_cpp`.  Both
//! sides connect to the same zenohd router (or peer directly).
//!
//! ## Quick start (3 terminals)
//!
//! ```bash
//! # 1. Router
//! zenohd --listen tcp/127.0.0.1:7447
//!
//! # 2. nros talker
//! cd examples/native/rust/zenoh/talker
//! RUST_LOG=info cargo run --features zenoh
//!
//! # 3. ROS 2 listener
//! source /opt/ros/humble/setup.bash
//! export RMW_IMPLEMENTATION=rmw_zenoh_cpp
//! ros2 topic echo /chatter std_msgs/msg/Int32 --qos-reliability best_effort
//! ```
//!
//! ## Common issues
//!
//! - **Topic not visible** in `ros2 topic list` — nros must declare
//!   liveliness tokens (handled automatically by the transport layer).
//!   Verify zenohd is reachable.
//! - **No messages received** — check that the data key expression uses
//!   `TypeHashNotSupported` (Humble) not `RIHS01_…` in the data topic.
//! - **QoS mismatch** — nros defaults to BEST_EFFORT; pass
//!   `--qos-reliability best_effort` on the ROS 2 subscriber.
//! - **rmw_zenoh not connecting** — force client mode on the ROS 2 side:
//!   `export ZENOH_CONFIG_OVERRIDE='mode="client";connect/endpoints=["tcp/127.0.0.1:7447"]'`
//! - **Humble vs Iron** — Humble uses `TypeHashNotSupported`; Iron+ uses
//!   `RIHS01_<sha256>` (requires the `ros-iron` feature).

//! phase-309 W5 residual — nano-ros CycloneDDS `geometry_msgs/PoseStamped`
//! publisher fixture for the multi-edition interop lane.
//!
//! Publishes the #0267 vector (position 1.5/2.5/-3.5, orientation.w=1.0,
//! frame_id "map", stamp 7.9) on `/pose` over the CycloneDDS RMW, on the ROS
//! domain given by `ROS_DOMAIN_ID` (default 0). A per-edition `domain_bridge`
//! + echo (in the edition's container) then asserts the depth-2 nested values
//! survive — the product-node version of the harness's stock-publisher bridge
//! lane. Built against GENERATED `geometry_msgs` bindings (host or, for a true
//! per-edition run, regenerated in the edition container — see phase-309 W5).

use core::fmt::Write as _;

use geometry_msgs::msg::PoseStamped;
use log::{error, info};
use nros::prelude::*;

fn main() {
    nros_board_linux::register_linked_rmw();
    env_logger::init();

    let ctx = nros::init_with_launch_auto().expect("nros init failed");
    let cfg = ctx.config("ros_edition_pose_pub");
    let mut executor: Executor = Executor::open(&cfg).expect("Failed to open session");

    let publisher = {
        let mut node = executor
            .create_node("ros_edition_pose_pub")
            .expect("Failed to create node");
        let pub_ = node
            .create_publisher::<PoseStamped>("/pose")
            .expect("Failed to create publisher");
        info!("Publisher created for /pose (geometry_msgs/PoseStamped)");
        pub_
    };

    executor
        .register_timer(nros::TimerDuration::from_millis(200), move || {
            let mut msg = PoseStamped::default();
            msg.header.stamp.sec = 7;
            msg.header.stamp.nanosec = 9;
            let _ = write!(msg.header.frame_id, "map");
            msg.pose.position.x = 1.5;
            msg.pose.position.y = 2.5;
            msg.pose.position.z = -3.5;
            msg.pose.orientation.w = 1.0;
            match publisher.publish(&msg) {
                Ok(()) => info!("Publishing PoseStamped x={} z={}", msg.pose.position.x, msg.pose.position.z),
                Err(e) => error!("Publish error: {:?}", e),
            }
        })
        .expect("Failed to register publish timer");

    executor
        .spin_blocking(SpinOptions::default())
        .expect("spin_blocking error");
}

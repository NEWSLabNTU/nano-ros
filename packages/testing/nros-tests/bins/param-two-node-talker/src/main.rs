//! Two-node parameter fixture (phase-426 W6).
//!
//! `param-chatter-talker` is the ONE-node parameter fixture, and one node is
//! exactly what the defect this phase fixes could not tell apart: before W3 the
//! six REP-2002 parameter services were published under the EXECUTOR's identity,
//! so an image composing several nodes onto one executor exposed one FQN however
//! many nodes it had, and two nodes declaring the same parameter name collided
//! in one flat table. A single-node fixture is green either way.
//!
//! So this image composes TWO nodes on one executor and gives them overlapping
//! parameter names on purpose:
//!
//! | node     | parameter | value | note                                     |
//! | -------- | --------- | ----- | ---------------------------------------- |
//! | `alpha`  | `rate`    | 10    | same NAME as beta's, different value     |
//! | `alpha`  | `stepped` | 10    | `IntegerRange { 0..=100, step 5 }`       |
//! | `beta`   | `rate`    | 20    | same NAME as alpha's, different value    |
//!
//! `stepped` is `alpha`'s alone — a `ros2 param list /beta` that shows it is a
//! flat table wearing two FQNs. Its step constraint is the wire half of issue
//! 1150 (`step` was stored, described, and never enforced); the fact that no
//! node here opts into `allow_undeclared_parameters` is the wire half of issue
//! 1151 (a `ros2 param set` on a name nobody declared reported success and
//! created a second, invisible parameter).
//!
//! Each node also publishes `std_msgs/Int32` on its own topic, because a node
//! with no endpoints is not something a `ros2 node list` is a fair test of.
//!
//! Consumed by `tests/params_per_node_interop.rs`.

use log::{error, info};
use nros::{ParameterDescriptor, ParameterType, prelude::*};
use std_msgs::msg::Int32;

/// Printed once per registered set, after the first spin has reconciled them.
///
/// The wire is what W6 is about, but this line is the in-process answer beside
/// it: `Executor::parameter_service_node_names` is the list the six servers are
/// built from, so a test that sees `/alpha` and `/beta` on the wire and here has
/// checked both ends of the same claim. Mirrored by
/// `nros_tests::output::PARAM_SERVICE_NODE_PREFIX`.
const PARAM_SERVICE_NODE_PREFIX: &str = "param services registered for node: ";

fn main() {
    env_logger::init();
    // issue 1268 / phase-444 W6 — register whichever backend the `rmw-*` feature
    // linked, through the same seam the examples and `graph-probe` use. This was
    // a hardcoded `nros_rmw_zenoh::register()`, which pinned the fixture to one
    // RMW; a parameter fixture that can only speak zenoh cannot answer whether
    // CYCLONE serves the six `rcl_interfaces` services — and Cyclone did not,
    // for want of a type descriptor, while the zenoh cell stayed green.
    nros_board_linux::register_linked_rmw();

    info!("nros two-node parameter fixture");

    let ctx = nros::init_with_launch_auto().expect("nros init failed");
    let cfg = ctx.config("param_two_node");
    let mut executor: Executor = Executor::open(&cfg).expect("Failed to open session");

    // The request is recorded here and satisfied by the reconcile at the head of
    // the first spin — the order every generated entry has, and the order W3's
    // acceptance test pins: `apply_param_services` runs before the per-node
    // `register` calls, so the node table is still empty at request time.
    executor
        .register_parameter_services()
        .expect("Failed to register parameter services");

    // `build()` hands back the NodeId the parameter store is keyed by. The
    // later `create_node` calls dedup onto these same records (same name, same
    // namespace), so the publisher and the parameters belong to one node.
    let alpha = executor
        .node_builder("alpha")
        .build()
        .expect("Failed to register node alpha");
    let beta = executor
        .node_builder("beta")
        .build()
        .expect("Failed to register node beta");

    // The same NAME on both nodes, with different values. Pre-W1 this was one
    // flat table and the second declaration was a collision.
    assert!(
        executor.declare_parameter_on(alpha, "rate", ParameterValue::Integer(10)),
        "alpha/rate declaration refused"
    );
    assert!(
        executor.declare_parameter_on(beta, "rate", ParameterValue::Integer(20)),
        "beta/rate declaration refused"
    );

    // alpha's alone, and constrained: 0..=100 on a step-5 lattice (issue 1150).
    let stepped = ParameterDescriptor::new("stepped", ParameterType::Integer)
        .expect("descriptor name fits")
        .with_description("step-5 lattice; an off-step `ros2 param set` must be refused")
        .with_integer_range(0, 100, 5);
    assert!(
        executor.declare_parameter_with_descriptor_on(
            alpha,
            "stepped",
            ParameterValue::Integer(10),
            stepped,
        ),
        "alpha/stepped declaration refused"
    );

    let alpha_pub = {
        let mut node = executor
            .create_node("alpha")
            .expect("Failed to create node alpha");
        node.create_publisher::<Int32>("/alpha_chatter")
            .expect("Failed to create alpha publisher")
    };
    let beta_pub = {
        let mut node = executor
            .create_node("beta")
            .expect("Failed to create node beta");
        node.create_publisher::<Int32>("/beta_chatter")
            .expect("Failed to create beta publisher")
    };

    let mut alpha_count: i32 = 0;
    executor
        .register_timer(nros::TimerDuration::from_millis(500), move || {
            let msg = Int32 { data: alpha_count };
            if let Err(e) = alpha_pub.publish(&msg) {
                error!("alpha publish error: {:?}", e);
            }
            alpha_count = alpha_count.wrapping_add(1);
        })
        .expect("Failed to register alpha timer");
    let mut beta_count: i32 = 0;
    executor
        .register_timer(nros::TimerDuration::from_millis(500), move || {
            let msg = Int32 { data: beta_count };
            if let Err(e) = beta_pub.publish(&msg) {
                error!("beta publish error: {:?}", e);
            }
            beta_count = beta_count.wrapping_add(1);
        })
        .expect("Failed to register beta timer");

    // One spin to run the reconcile, THEN report — a readiness marker printed
    // before the services exist is the shape that makes a test wait for nothing.
    let _ = executor.spin_once(core::time::Duration::from_millis(50));
    for fqn in executor.parameter_service_node_names().iter() {
        info!("{}{}", PARAM_SERVICE_NODE_PREFIX, fqn.as_str());
    }

    executor
        .spin(SpinOptions::default())
        .expect("spin error");
}

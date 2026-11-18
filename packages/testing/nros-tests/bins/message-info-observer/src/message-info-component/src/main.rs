//! Component-API MessageInfo observer (issue 2349).
//!
//! The same observation `message-info-observer` makes, written the way a
//! component is: a `Component` + `ExecutableNode` whose subscription is
//! declared with `create_subscription_for_callback_name_with_message_info`,
//! and whose callback reads the metadata from `CallbackCtx::message_info()`.
//! It prints the same line, so the test asserts the same facts on both.
//!
//! Driven board-less by `ExecutorNodeRuntime::from_executor`, as
//! `declarative-safety-listener` is, so the component's declaration goes
//! through the declarative runtime's registrar: the path a launched component
//! takes.
//!
//! # Output contract
//!
//! Per received message, two lines:
//!
//! ```text
//! I heard: [<data>]
//! seq=<publication_sequence_number> gid=<hex> ts=<source_timestamp_nanos> rx=<received_timestamp_nanos>
//! ```
//!
//! A message whose callback sees no metadata prints `MessageInfo ABSENT` at
//! error level instead of the `seq=` line, so the test reads it as a failure,
//! never as a quiet run.

use log::{error, info};
use nros::{
    Callback, CallbackCtx, Component, DeclarativeNode, ExecutableNode, Executor, NodeContext,
    NodeOptions, NodeResult, node_runtime::ExecutorNodeRuntime,
};
use std_msgs::msg::String as StringMsg;

struct Observer;

impl Component for Observer {
    const NAME: &'static str = "listener";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("listener"))?;
        // The subject: a declaration that asks for the message's metadata.
        node.create_subscription_for_callback_name_with_message_info::<StringMsg>(
            "on_chatter",
            "/chatter",
        )?;
        info!("Subscriber created for topic: /chatter");
        info!("Waiting for messages with MessageInfo on /chatter...");
        Ok(())
    }
}

impl ExecutableNode for Observer {
    type State = ();
    const HAS_TICK: bool = false;

    fn init() -> Self::State {}

    fn on_callback(_state: &mut Self::State, cb: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if cb.as_str() != "on_chatter" {
            return;
        }
        let Ok(msg) = ctx.message::<StringMsg>() else {
            return;
        };
        match ctx.message_info() {
            Some(mi) => {
                let gid = mi
                    .publisher_gid()
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>();
                info!("I heard: [{}]", msg.data);
                info!(
                    "seq={} gid={} ts={} rx={}",
                    mi.publication_sequence_number(),
                    gid,
                    mi.source_timestamp().to_nanos(),
                    mi.received_timestamp().to_nanos(),
                );
            }
            None => error!(
                "MessageInfo ABSENT for a message on /chatter (payload=[{}])",
                msg.data
            ),
        }
    }
}

fn main() {
    env_logger::init();
    nros_board_linux::register_linked_rmw();

    // Contains "Listener" for the same reason message-info-observer's banner
    // does: the e2e spawn helpers key readiness off that word.
    info!("nros MessageInfo Component Listener (test fixture)");

    let ctx = nros::init().expect("nros::init failed");
    let cfg = ctx.config("listener");
    let executor: Executor = Executor::open(&cfg).expect("Failed to open session");

    let mut runtime = ExecutorNodeRuntime::from_executor(executor);
    runtime
        .register_node::<Observer>()
        .expect("register the MessageInfo component");

    if let Err(e) = runtime.spin() {
        error!("Spin error: {e:?}");
    }
}

//! Phase 250 Wave 5 — declarative E2E-safety listener (cross-process subscriber).
//!
//! A `Node` + `ExecutableNode` that opts a subscription into integrity
//! validation via the declarative `.safety()` API
//! ([`create_subscription_for_callback_name_with_safety`]) and reads the
//! per-message [`IntegrityStatus`] in its callback through
//! [`CallbackCtx::integrity`]. Driven board-less by
//! [`ExecutorNodeRuntime::from_executor`] over a plain zenoh `Executor` (the
//! same shape as the `qos-override-pubsub` fixture), so no board / `nros::main!`
//! scaffold is needed.
//!
//! Paired with `tests/safety_e2e.rs`: the safety talker (publisher, attaches
//! CRC + sequence) runs in a separate process over zenohd; this subscriber
//! receives and logs `[SAFETY] ... crc=ok` — proving the declarative safety
//! path surfaces real wire integrity end-to-end (zenoh-pico does not deliver
//! in-process, so the topology is cross-process).

use log::{error, info};
use nros::{
    Callback, CallbackCtx, ExecutableNode, Executor, ExecutorConfig, Node, NodeContext,
    NodeOptions, NodeResult, node_runtime::ExecutorNodeRuntime,
};
use std_msgs::msg::Int32;

/// Declarative listener: a `.safety()` subscription on `/chatter`.
struct SafetyListener;

impl Node for SafetyListener {
    const NAME: &'static str = "listener";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("listener"))?;
        // The declarative `.safety()` opt-in (Phase 250 Wave 2b): the runtime
        // registers this as an integrity-validating subscription, so the
        // callback's `ctx.integrity()` is populated.
        node.create_subscription_for_callback_name_with_safety::<Int32>("on_chatter", "/chatter")?;
        info!("Safety subscriber created for topic: /chatter");
        info!("Waiting for Int32 messages on /chatter...");
        Ok(())
    }
}

impl ExecutableNode for SafetyListener {
    type State = u64;

    fn init() -> Self::State {
        0
    }

    fn on_callback(state: &mut Self::State, cb: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if cb.as_str() != "on_chatter" {
            return;
        }
        let Ok(msg) = ctx.message::<Int32>() else {
            return;
        };
        *state += 1;
        // Read the integrity status alongside the message (Shape A). `INTEGRITY`
        // is printed exactly when `ctx.integrity()` is `Some` — i.e. the declarative
        // `.safety()` opt-in surfaced the status. `NO-INTEGRITY` would mean it was a
        // basic (non-safety) subscription. The `crc=` sub-field is the rmw layer's
        // CRC verdict (ok / FAIL / n-a-when-absent), shared with the imperative path.
        match ctx.integrity() {
            Some(s) => info!(
                "[{}] Received: data={} [SAFETY] INTEGRITY seq_gap={} dup={} crc={}",
                *state,
                msg.data,
                s.gap,
                s.duplicate,
                match s.crc_valid {
                    Some(true) => "ok",
                    Some(false) => "FAIL",
                    None => "n-a",
                },
            ),
            None => info!(
                "[{}] Received: data={} [SAFETY] NO-INTEGRITY",
                *state, msg.data
            ),
        }
    }
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    nros_rmw_zenoh::register().expect("register zenoh backend");

    let locator = std::env::var("NROS_LOCATOR").unwrap_or_else(|_| "tcp/127.0.0.1:7447".into());
    info!("=== Phase 250 Wave 5 declarative-safety-listener: locator={locator} ===");

    let cfg = ExecutorConfig::new(&locator)
        .node_name("listener")
        .namespace("/");
    let executor = Executor::open_with_rmw("zenoh", &cfg).expect("open zenoh session");

    let mut runtime = ExecutorNodeRuntime::from_executor(executor);
    runtime
        .register_node::<SafetyListener>()
        .expect("register declarative safety listener");

    // Spin until killed by the test harness. `spin()` blocks on the executor's
    // halt flag (never raised here) at a 10 ms tick.
    if let Err(e) = runtime.spin() {
        error!("spin error: {e:?}");
    }
}

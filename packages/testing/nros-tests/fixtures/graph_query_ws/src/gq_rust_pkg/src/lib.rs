//! The Rust half of the graph-query parity probe.
//!
//! A 100 ms timer marks a probe due. The tick, where a component reaches the
//! graph queries, asks for the graph's node, topic and service counts and for
//! `/chatter`'s publisher count and type. Once `/chatter` has a publisher and
//! the answer has held for five probes, it prints the answer once.

#![no_std]

extern crate alloc;

use alloc::string::String;
use core::fmt::Write as _;

use nros::{
    Callback, CallbackCtx, Component, DeclarativeNode, ExecutableNode, NodeContext, NodeOptions,
    NodeResult, TickCtx, TimerDuration,
};

const STABLE_PROBES: u32 = 5;

pub struct Probe;

impl Component for Probe {
    const NAME: &'static str = "gq_rust";

    const ENTITY_BOUNDS: nros::EntityBounds = nros::EntityBounds::exact(0, 0, 0, 0, 0);

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("gq_rust"))?;
        let _timer =
            node.create_timer_for_callback_name("probe", TimerDuration::from_millis(100))?;
        Ok(())
    }
}

pub struct State {
    due: bool,
    done: bool,
    last: String,
    stable: u32,
}

/// One probe's answer, or `None` when a query failed.
fn answer(ctx: &mut TickCtx<'_>) -> Option<(usize, String)> {
    let publishers = ctx.count_publishers("/chatter").ok()?;
    let mut nodes = 0usize;
    ctx.get_node_names(&mut |_, _| {
        nodes += 1;
        true
    })
    .ok()?;
    let mut topics = 0usize;
    let mut chatter_type = String::new();
    ctx.get_topic_names_and_types(&mut |name, types| {
        topics += 1;
        if name == "/chatter"
            && let Some(t) = types.first()
        {
            chatter_type = String::from(*t);
        }
        true
    })
    .ok()?;
    let mut services = 0usize;
    ctx.get_service_names_and_types(&mut |_, _| {
        services += 1;
        true
    })
    .ok()?;
    let mut s = String::new();
    write!(
        s,
        "nodes={nodes} topics={topics} services={services} chatter_publishers={publishers} \
         chatter_type={chatter_type}"
    )
    .ok()?;
    Some((publishers, s))
}

impl ExecutableNode for Probe {
    type State = State;

    fn init() -> Self::State {
        State {
            due: false,
            done: false,
            last: String::new(),
            stable: 0,
        }
    }

    fn on_callback(state: &mut Self::State, callback: Callback<'_>, _ctx: &mut CallbackCtx<'_>) {
        if callback.is_named("probe") && !state.done {
            state.due = true;
        }
    }

    fn tick(state: &mut Self::State, ctx: &mut TickCtx<'_>) {
        if !state.due || state.done {
            return;
        }
        state.due = false;
        let Some((publishers, now)) = answer(ctx) else {
            return;
        };
        if now == state.last {
            state.stable += 1;
        } else {
            state.last = now;
            state.stable = 0;
        }
        if publishers == 0 || state.stable < STABLE_PROBES {
            return;
        }
        state.done = true;
        nros::log_info!(
            nros::get_logger("gq_rust"),
            "GRAPH_QUERY lang=rust {}",
            state.last
        );
    }
}

nros::node!(Probe);

//! A pre-boot step, declared on the image rather than written into an entry
//! (RFC-0065 D5, amended 2026-10-24; issue 1511).
//!
//! The image says
//!
//! ```toml
//! [image.linux_rust_before_boot]
//! before_boot = "rust_before_boot_talker_pkg::prepare"
//! ```
//!
//! and the generated entry calls [`prepare`] FIRST, before the board opens the
//! executor. Nothing has booted at that point: no session, no executor, and on
//! an embedded board not even the board's own hardware init. So `prepare` does its work (here, it
//! only records that it ran) and returns; an `Err` stops the image before
//! anything else starts.
//!
//! The node then says, on every tick, whether `prepare` ran before it was
//! registered. That is the order the hook promises, and the line the
//! `before_boot` cell asserts.

#![no_std]

use core::sync::atomic::{AtomicBool, Ordering};

use nros::{
    Callback, CallbackCtx, Component, DeclarativeNode, ExecutableNode, NodeContext, NodeOptions,
    NodeResult, TimerDuration,
};

/// Set by [`prepare`]; read when the node registers.
static PREPARED: AtomicBool = AtomicBool::new(false);

/// The image's `before_boot` step. A real one opens a device, checks a
/// calibration, or waits for a peripheral; its signature is fixed by
/// `nros::main!`.
pub fn prepare() -> Result<(), &'static str> {
    PREPARED.store(true, Ordering::SeqCst);
    Ok(())
}

/// Log period. 4 Hz keeps the cell's three-line budget short.
const PERIOD_MS: u64 = 250;

/// Logs the boot order on every tick.
pub struct BeforeBootTalker;

impl Component for BeforeBootTalker {
    const NAME: &'static str = "before_boot_talker";

    // No publishers, servers or clients: the cell reads this node's log.
    const ENTITY_BOUNDS: nros::EntityBounds = nros::EntityBounds::exact(0, 0, 0, 0, 0);

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("before_boot_talker"))?;
        let _timer =
            node.create_timer_for_callback_name("on_tick", TimerDuration::from_millis(PERIOD_MS))?;
        Ok(())
    }
}

impl ExecutableNode for BeforeBootTalker {
    /// Whether `prepare` had run when the node registered, and a tick count.
    type State = (bool, u32);

    fn init() -> Self::State {
        (PREPARED.load(Ordering::SeqCst), 0)
    }

    fn on_callback(state: &mut Self::State, callback: Callback<'_>, _ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_tick" {
            state.1 = state.1.wrapping_add(1);
            if state.0 {
                log::info!("before_boot ran first seq={}", state.1);
            } else {
                log::info!("before_boot DID NOT run before register seq={}", state.1);
            }
        }
    }
}

nros::node!(BeforeBootTalker);

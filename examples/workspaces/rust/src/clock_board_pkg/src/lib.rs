//! A stub board's wall clock (RFC-0141 D2, phase-582 W1d): the board hook
//! `nros_board_wall_clock_ns`, answering one fixed instant, and a node that
//! prints what the wall clock then reads.
//!
//! Only the `linux_talker_clock_board` image launches this package, and that
//! image states `[knobs.clock] source = "board"`, so the port asks the hook on
//! its first wall-clock read. Every other image of the bringup links neither
//! the hook nor anything that references it.
//!
//! The node prints, once a second,
//! `clock-board: wall_ns=<ns> synced=<0|1> source=<id> steps=<n>`: the wall
//! clock through `nros::Clock::system()`, then the state query
//! (`nros::wall_clock_state()`). `source` is the `NROS_WALL_CLOCK_SOURCE_*`
//! value (3 is `board`).

#![no_std]

use nros::{
    Callback, CallbackCtx, Clock, Component, DeclarativeNode, ExecutableNode, NodeContext,
    NodeOptions, NodeResult, TimerDuration,
};

/// The instant the stub board answers: 2023-11-14T22:13:20Z.
pub const BOARD_INSTANT_NS: u64 = 1_700_000_000_000_000_000;

/// The board's wall-clock hook: writes the board's instant and reports it set.
///
/// # Safety
///
/// `out_ns` is null or valid for a `u64` write, as the port passes it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_board_wall_clock_ns(out_ns: *mut u64) -> bool {
    if out_ns.is_null() {
        return false;
    }
    // SAFETY: non-null, and valid for the write per this fn's contract.
    unsafe { out_ns.write(BOARD_INSTANT_NS) };
    true
}

/// A node with one 1 s timer that prints the wall clock.
pub struct ClockBoard;

impl Component for ClockBoard {
    const NAME: &'static str = "clock_board";
    const ENTITY_BOUNDS: nros::EntityBounds = nros::EntityBounds::exact(0, 0, 0, 0, 0);

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        // The port references the hook from C, after this crate on the link
        // line, and a Rust rlib's `#[no_mangle]` function nothing in Rust
        // reaches may not be in the objects the linker takes. Naming it here
        // puts it in them.
        core::hint::black_box(nros_board_wall_clock_ns as unsafe extern "C" fn(*mut u64) -> bool);
        let mut node = ctx.create_node(NodeOptions::new("clock_board"))?;
        let _timer =
            node.create_timer_for_callback_name("on_tick", TimerDuration::from_millis(1000))?;
        Ok(())
    }
}

impl ExecutableNode for ClockBoard {
    type State = ();
    const HAS_TICK: bool = false;

    fn init() -> Self::State {}

    fn on_callback(_state: &mut Self::State, callback: Callback<'_>, _ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_tick" {
            let wall_ns = Clock::system().now().to_nanos();
            match nros::wall_clock_state() {
                Some(st) => log::info!(
                    "clock-board: wall_ns={} synced={} source={} steps={}",
                    wall_ns,
                    u8::from(st.synced),
                    st.source_id,
                    st.steps
                ),
                None => log::info!("clock-board: wall_ns={} state=none", wall_ns),
            }
        }
    }
}

nros::node!(ClockBoard);

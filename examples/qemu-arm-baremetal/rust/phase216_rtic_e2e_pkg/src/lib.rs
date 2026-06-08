#![no_std]

use nros::{
    Callback, CallbackCtx, DispatchStrategy, ExecutableNode, Node, NodeContext, NodeResult, TickCtx,
};

pub struct E2eNode;

pub struct E2eState {
    fired: bool,
}

impl Node for E2eNode {
    const NAME: &'static str = "phase216_rtic_e2e";
    const DISPATCH: DispatchStrategy = DispatchStrategy::Deferred;

    fn register(_ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        Ok(())
    }
}

impl ExecutableNode for E2eNode {
    type State = E2eState;

    fn init() -> Self::State {
        E2eState { fired: false }
    }

    fn on_callback(state: &mut Self::State, callback: Callback<'_>, _ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "__nros_e2e" && !state.fired {
            state.fired = true;
            nros_board_mps2_an385::exit_success();
        }
    }

    fn tick(_state: &mut Self::State, _ctx: &mut TickCtx<'_>) {}
}

nros::node!(E2eNode);

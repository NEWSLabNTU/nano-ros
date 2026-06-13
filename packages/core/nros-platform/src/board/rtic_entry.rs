//! [`RticBoardEntry`] — Phase 216.B.1.
//!
//! Sibling to [`super::BoardEntry`] for **framework-owned-spin**
//! boards. Where `BoardEntry::run` owns the boot lifecycle and
//! drives the executor itself, `RticBoardEntry` hands the runtime
//! over to RTIC: the `nros::main!()` proc-macro (216.B.3) generates a
//! `#[rtic::app]` module that calls [`RticBoardEntry::init_hardware`]
//! from inside the framework-generated `#[init]` body, stashes the
//! returned `(Executor, Runtime)` pair in `#[local]` storage, and
//! lets RTIC's interrupt-driven scheduler drive dispatch.
//!
//! ```ignore
//! impl RticBoardEntry for RticStm32F4 {
//!     type Pac = stm32f4xx_hal::pac::Peripherals;
//!     type Core = cortex_m::Peripherals;
//!     type Executor = nros::Executor;
//!     type Runtime = RticRuntime;
//!
//!     const DISPATCHERS: &'static [&'static str] = &["USART1", "USART2"];
//!
//!     fn init_hardware(
//!         device: Self::Pac,
//!         core: Self::Core,
//!     ) -> (Self::Executor, Self::Runtime) {
//!         // … clock / pin / transport bringup, build Executor + Runtime …
//!     }
//! }
//! ```
//!
//! ## Layering note
//!
//! `nros-platform` sits **below** `nros` in the dep graph (`nros`
//! depends on `nros-platform`, not the other way around). That
//! forces two abstractions here:
//!
//! 1. [`RticBoardEntry::Executor`] is an opaque assoc type — concrete
//!    board impls plug in `nros::Executor`, but the trait surface
//!    cannot name it without inverting the dep graph.
//! 2. [`RticBoardEntry::Core`] is an opaque assoc type for the same
//!    reason against `cortex_m`. Every Cortex-M chip board will
//!    pick `cortex_m::Peripherals`, but pulling `cortex_m` into
//!    `nros-platform` would force the dep on every consumer
//!    (POSIX, Zephyr, FreeRTOS, …) that has no use for it.

use super::{Board, DeployOverlay, runtime::NodeDispatchRuntime};

/// Board-side hook for RTIC integration. The `nros::main!()`
/// proc-macro (216.B.3) generates a `#[rtic::app]` module that calls
/// [`Self::init_hardware`] from inside the framework-generated
/// `#[init]` body and wires the returned pair into RTIC `#[local]`
/// storage.
///
/// Distinct from [`super::BoardEntry`] (board-owns-spin) and
/// [planned] `EmbassyBoardEntry` (216.C.1, executor-owns-spin via
/// `embassy_executor::Spawner`).
pub trait RticBoardEntry: Board {
    /// Chip Peripheral Access Crate handle (e.g.
    /// `stm32f4xx_hal::pac::Peripherals`). Whatever the RTIC
    /// `#[rtic::app(device = …)]` attribute expects as the `device`
    /// peripheral struct.
    type Pac: 'static;

    /// Core peripheral handle. Typically `cortex_m::Peripherals` on
    /// Cortex-M chips but kept abstract so `nros-platform` doesn't
    /// take a transitive `cortex_m` dep that every POSIX / Zephyr /
    /// RTOS consumer would inherit.
    type Core: 'static;

    /// Executor type the board hands back. Concrete board impls plug
    /// in `nros::Executor`; the assoc type keeps the layering clean
    /// (`nros-platform` does not depend on `nros`). The proc-macro
    /// stashes this value in RTIC `#[local]` storage.
    type Executor: 'static;

    /// Dispatch sink the proc-macro wires into RTIC `#[local]`
    /// storage. Required to implement
    /// [`NodeDispatchRuntime`] so signaled callbacks queued from
    /// RTIC tasks reach the registered Node pkgs.
    ///
    /// Per Phase 216.A.2, `NodeDispatchRuntime` already carries
    /// `signal_callback` + `dispatch_strategy`; the RTIC runtime
    /// impl uses `DispatchStrategy::Deferred` and routes signals
    /// through a `heapless::spsc::Producer` into an RTIC software
    /// task (see Phase 216.B.2).
    type Runtime: NodeDispatchRuntime + 'static;

    /// RTIC `dispatchers = [...]` list, declared at the board layer
    /// so each chip pins its own interrupt slots (e.g. `&["USART1",
    /// "USART2"]`). The proc-macro splices this into the generated
    /// `#[rtic::app(dispatchers = …)]` attribute.
    const DISPATCHERS: &'static [&'static str];

    /// Run from inside the proc-macro-generated `#[init]` body.
    /// Returns the `(Executor, Runtime)` pair the macro stashes in
    /// RTIC `#[local]` storage. The board impl owns clock / pin /
    /// transport bringup before constructing the executor + runtime
    /// pair.
    fn init_hardware(device: Self::Pac, core: Self::Core) -> (Self::Executor, Self::Runtime);

    /// Like [`init_hardware`](Self::init_hardware) but applies a deploy-metadata
    /// overlay (Phase 244.D1) to the board's compiled-in net/locator `Config`
    /// before opening the executor. `nros::main!()` calls THIS from the
    /// generated `#[init]` body, passing the
    /// `[package.metadata.nros.deploy.<board>]` block.
    ///
    /// The default ignores `deploy` and forwards to
    /// [`init_hardware`](Self::init_hardware), so existing RTIC boards are
    /// unchanged. Boards with a baked net `Config` (the bare-metal firmware
    /// boards) override it so each Entry pkg can pin its own ip / locator /
    /// gateway — required when two RTIC firmwares share one board on the same
    /// QEMU network (e.g. the talker-rtic / listener-rtic pub/sub pair).
    fn init_hardware_with_deploy(
        device: Self::Pac,
        core: Self::Core,
        _deploy: &DeployOverlay,
    ) -> (Self::Executor, Self::Runtime) {
        <Self as RticBoardEntry>::init_hardware(device, core)
    }
}

#[cfg(test)]
mod tests {
    //! Compile-time smoke test: a dummy `RticBoardEntry` impl wires
    //! through every assoc type / const slot and the `Board`
    //! super-trait chain (`BoardInit + BoardPrint + BoardExit`). The
    //! impl is never invoked at runtime — the test is purely about
    //! the trait surface accepting a real-shaped board type without
    //! any extra bounds creep.
    use super::*;
    use crate::board::{
        BoardExit, BoardInit, BoardPrint, NodeDispatchFn, NodeDispatchRuntime, NodeInitFn,
        NodeRegisterFn, NodeTickFn,
    };

    struct DummyPac;
    struct DummyCore;
    struct DummyExecutor;
    struct DummyRuntime;
    struct DummyBoard;

    impl BoardInit for DummyBoard {
        fn init_hardware() {}
    }
    impl BoardPrint for DummyBoard {
        fn println(_args: core::fmt::Arguments<'_>) {}
    }
    impl BoardExit for DummyBoard {
        fn exit_success() -> ! {
            // Test impl — never executed; the trait surface only
            // requires the signature.
            loop {}
        }
        fn exit_failure() -> ! {
            loop {}
        }
    }

    impl NodeDispatchRuntime for DummyRuntime {
        fn register_dispatch_slot_dyn(
            &mut self,
            _register: NodeRegisterFn,
            _init: NodeInitFn,
            _dispatch: NodeDispatchFn,
            _tick: NodeTickFn,
            _name: &'static str,
        ) -> Result<(), ()> {
            Err(())
        }

        fn spin_once(&mut self, _timeout_ms: u32) -> Result<(), ()> {
            Err(())
        }
    }

    impl RticBoardEntry for DummyBoard {
        type Pac = DummyPac;
        type Core = DummyCore;
        type Executor = DummyExecutor;
        type Runtime = DummyRuntime;

        const DISPATCHERS: &'static [&'static str] = &["USART1", "USART2"];

        fn init_hardware(_device: Self::Pac, _core: Self::Core) -> (Self::Executor, Self::Runtime) {
            (DummyExecutor, DummyRuntime)
        }
    }

    #[test]
    fn dummy_board_satisfies_rtic_board_entry() {
        // Trait-method call confirms the assoc types + const slot
        // line up. We never spin the returned pair.
        let (_exec, _rt) = <DummyBoard as RticBoardEntry>::init_hardware(DummyPac, DummyCore);
        assert_eq!(<DummyBoard as RticBoardEntry>::DISPATCHERS.len(), 2);
    }
}

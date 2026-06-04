//! # nros-board-embassy-stm32f4 — Phase 216.C.2 (skeleton)
//!
//! Embassy-flavored STM32F4 board crate. Sibling to
//! [`nros-board-stm32f4`](../nros-board-stm32f4) (board-owns-spin via
//! `BoardEntry::run`) and the forthcoming
//! [`nros-board-rtic-stm32f4`](../nros-board-rtic-stm32f4) (Phase
//! 216.B.2, RTIC-owns-spin).
//!
//! This crate exposes [`EmbassyStm32F4`] — a unit struct that
//! implements the [`Board`] super-trait family ([`BoardInit`],
//! [`BoardPrint`], [`BoardExit`]) plus the framework-owned-spin
//! [`EmbassyBoardEntry`] hook (Phase 216.C.1). The `nros::main!()`
//! proc-macro (Phase 216.C.3) reads the `[package.metadata.nros.board]
//! framework = "embassy"` key in this crate's `Cargo.toml` and routes
//! the generated entry point onto an `#[embassy_executor::main]`
//! shape that calls `<EmbassyStm32F4 as EmbassyBoardEntry>::init_hardware`.
//!
//! ## Skeleton status
//!
//! Phase 216.C.2 lands the **trait surface only**. Every method body
//! is `todo!()` so reviewers don't expect a working runtime:
//!
//! - [`BoardInit::init_hardware`] — `todo!()`. Real impl mirrors
//!   `examples/stm32f4/rust/talker-embassy/src/main.rs`:
//!   1. `embassy_stm32::init(Default::default())` → `Peripherals`.
//!   2. Configure clock tree (HSE + PLL) via `embassy_stm32::Config`.
//!   3. Wire GPIO / LED (PB7 on NUCLEO-F429ZI).
//!   4. Bring up the network stack (Ethernet via `embassy-net` +
//!      `embassy-stm32`'s `eth` peri) or UART transport.
//! - [`BoardPrint::println`] — `todo!()`. Wire to `defmt::info!`
//!   over RTT, matching the example crate.
//! - [`BoardExit::{exit_success, exit_failure}`] — `todo!()`. On
//!   real hardware, halt in `wfi`; on probe-attached runs, fall back
//!   to `cortex_m::peripheral::SCB::sys_reset()`.
//! - [`EmbassyBoardEntry::init_hardware`] — `todo!()`. Drives the
//!   above through the Embassy `Spawner` and hands back the
//!   `(Executor, EmbassyRuntime)` pair the proc-macro then spawns
//!   dispatch tasks against.
//!
//! ## Layering decision: `type Executor = ()`
//!
//! Pulling `nros` as a dep here would force every consumer of
//! `nros-board-embassy-stm32f4` to also resolve the full `nros`
//! tree — overkill for a skeleton whose `init_hardware` is `todo!()`.
//! The trait permits any `'static` type for [`EmbassyBoardEntry::Executor`],
//! so the skeleton picks `()`. Phase 216.C.2 follow-ups switch this to
//! `nros::Executor` once the real bringup body lands.

#![no_std]
// Phase 216.C.2 — skeleton; every fn body is `todo!()` so the
// `nros-platform-stm32f4` / `nros-board-common` deps look unused to
// the lint pass until the follow-up wires real init.
#![allow(unused_imports, dead_code)]

use embassy_executor::Spawner;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};

use nros_platform::{
    BoardExit, BoardInit, BoardPrint, DispatchStrategy, EmbassyBoardEntry, NodeDispatchFn,
    NodeDispatchRuntime, NodeInitFn, NodeRegisterFn, NodeTickFn, SignaledCallback,
};

// Re-export cortex_m_rt + defmt so user Entry pkgs that copy the
// existing `nros-board-stm32f4` surface keep working without an extra
// dep.
pub use cortex_m_rt::entry;
pub use defmt;

/// Embassy-flavored STM32F4 board. Phase 216.C.2 skeleton — every
/// Board / EmbassyBoardEntry method is `todo!()`. The trait surface is
/// what 216.C.3 macro routing needs.
pub struct EmbassyStm32F4;

// ---- Board super-trait family (BoardInit + BoardPrint + BoardExit) ----

impl BoardInit for EmbassyStm32F4 {
    fn init_hardware() {
        // Phase 216.C.2 follow-up — mirror
        // `examples/stm32f4/rust/talker-embassy/src/main.rs`:
        //   let p = embassy_stm32::init(Default::default());
        //   // clock tree + pin mux + transport bringup.
        // The Embassy entry point calls `EmbassyBoardEntry::init_hardware`
        // (below) with the Spawner handle; the standalone
        // `BoardInit::init_hardware` slot here covers the
        // BoardEntry::run path used by sibling non-Embassy boards.
        todo!("Phase 216.C.2 follow-up — embassy_stm32::init + clock/pin/transport bringup");
    }
}

impl BoardPrint for EmbassyStm32F4 {
    fn println(_args: core::fmt::Arguments<'_>) {
        // Phase 216.C.2 follow-up — wire `defmt::info!("{}", args)`
        // over RTT (the example uses `defmt_rtt`).
        todo!("Phase 216.C.2 follow-up — wire defmt-rtt println bridge");
    }
}

impl BoardExit for EmbassyStm32F4 {
    fn exit_success() -> ! {
        // Phase 216.C.2 follow-up — halt in `wfi` or
        // `cortex_m::peripheral::SCB::sys_reset()` depending on
        // whether a debugger is attached.
        todo!("Phase 216.C.2 follow-up — exit_success (halt/reset)");
    }

    fn exit_failure() -> ! {
        // Phase 216.C.2 follow-up — same as exit_success with a
        // failure indicator (e.g. LED pattern + panic-probe surface).
        todo!("Phase 216.C.2 follow-up — exit_failure (halt/reset)");
    }
}

// ---- Runtime sink ----

/// Embassy dispatch runtime — declares `DispatchStrategy::Deferred`
/// so the macro routes signaled callbacks through an
/// `embassy_sync::channel::Channel` (Phase 216.C.2 follow-up).
///
/// Skeleton: the channel slot isn't wired yet. The struct sits empty
/// so the trait surface is callable; `signal_callback` panics with a
/// `todo!()` until the follow-up lands.
pub struct EmbassyRuntime {
    // Phase 216.C.2 follow-up — wire the per-board callback channel:
    //
    //   pub(crate) channel: &'static Channel<
    //       CriticalSectionRawMutex,
    //       SignaledCallback<'static>,
    //       { <EmbassyStm32F4 as EmbassyBoardEntry>::CHANNEL_CAPACITY },
    //   >,
    //
    // Built from a `static_cell::StaticCell` in `init_hardware` so the
    // dispatch task can receive on it from a long-lived Embassy task.
    _private: (),
}

impl EmbassyRuntime {
    /// Skeleton constructor. Phase 216.C.2 follow-up: take the
    /// `&'static Channel<…>` handle the proc-macro / init code wires up.
    pub const fn new() -> Self {
        Self { _private: () }
    }
}

impl Default for EmbassyRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl NodeDispatchRuntime for EmbassyRuntime {
    fn register_dispatch_slot_dyn(
        &mut self,
        _register: NodeRegisterFn,
        _init: NodeInitFn,
        _dispatch: NodeDispatchFn,
        _tick: NodeTickFn,
        _name: &'static str,
    ) -> Result<(), ()> {
        // Phase 216.C.2 follow-up — wrap an `ExecutorNodeRuntime`-style
        // sink (lives in `nros::component_runtime`) and forward through.
        // Skeleton: surface unwired by returning `Err(())` so callers
        // fail loudly rather than silently succeed.
        Err(())
    }

    fn spin_once(&mut self, _timeout_ms: u32) -> Result<(), ()> {
        // Phase 216.C.2 follow-up — when DispatchStrategy::Deferred is
        // active, `spin_once` typically yields (Embassy owns the
        // scheduler). The macro-generated dispatch task pulls callback
        // signals from the channel; the executor's spin path lives in
        // a sibling Embassy task wired by `init_hardware`.
        Err(())
    }

    fn signal_callback(&mut self, _cb: SignaledCallback<'_>) {
        // Phase 216.C.2 follow-up — `self.channel.try_send(cb).ok();`
        // (drop-on-full surfaces via a counter the dispatch task logs).
        todo!("Phase 216.C.2 follow-up — wire embassy_sync::channel try_send");
    }

    fn dispatch_strategy(&self) -> DispatchStrategy {
        DispatchStrategy::Deferred
    }
}

// ---- EmbassyBoardEntry impl (Phase 216.C.1 trait surface) ----

impl EmbassyBoardEntry for EmbassyStm32F4 {
    type Spawner = Spawner;

    // Phase 216.C.2 skeleton — `()` keeps `nros` out of the dep tree
    // for the trait surface. Follow-up swaps to `nros::Executor` once
    // the real bringup body lands. The trait permits any `'static`
    // type so the skeleton round-trips through the proc-macro
    // (Phase 216.C.3) unchanged.
    type Executor = ();

    type Runtime = EmbassyRuntime;

    // Inherits `CHANNEL_CAPACITY = 32` from the trait default. No
    // override — STM32F4 callback density fits comfortably in 32.

    fn init_hardware(_spawner: Self::Spawner) -> (Self::Executor, Self::Runtime) {
        // Phase 216.C.2 follow-up — drive the Embassy bringup:
        //   1. `let p = embassy_stm32::init(Default::default());`
        //   2. Clock tree (HSE 8 MHz → PLL 168 MHz for F429).
        //   3. GPIO / LED on PB7 (NUCLEO-F429ZI).
        //   4. Transport bringup (Ethernet via embassy-net or UART).
        //   5. Build `nros::Executor` from the wired transport.
        //   6. Build `EmbassyRuntime { channel: STATIC_CELL.init(...) }`.
        //   7. Spawn a dispatch task that pulls from the channel and
        //      forwards into the executor.
        //   8. Return `(executor, runtime)`.
        todo!("Phase 216.C.2 follow-up — embassy bringup + executor + runtime wiring");
    }
}

//! # nros-board-rtic-stm32f4
//!
//! Phase 216.B.2 — RTIC + STM32F4 board crate. Sibling to the
//! direct-exec [`nros-board-stm32f4`](https://docs.rs/nros-board-stm32f4)
//! crate; both target the same chip family (STM32F4xx, Cortex-M4F)
//! but route the boot lifecycle through different surfaces:
//!
//! - `nros-board-stm32f4` impls
//!   [`nros_platform::BoardEntry::run`] and owns the spin loop.
//! - `nros-board-rtic-stm32f4` (this crate) impls
//!   [`nros_platform::RticBoardEntry`] and hands ownership of the
//!   runtime over to the RTIC framework — `nros::main!()` (Phase
//!   216.B.3) emits a `#[rtic::app]` module that calls
//!   [`RticStm32F4::init_hardware`] from the framework-generated
//!   `#[init]` body, stashes the returned
//!   `(Executor, Runtime)` pair in `#[local]` storage, and routes
//!   signaled callbacks through the [`RticRuntime`] SPSC queue into
//!   an RTIC software task.
//!
//! ## Scope of this skeleton
//!
//! Only the **trait surface** that 216.B.3's proc-macro routing
//! depends on. Specifically:
//!
//! 1. [`RticStm32F4`] unit struct + the four
//!    [`BoardInit`] / [`BoardPrint`] / [`BoardExit`] /
//!    [`RticBoardEntry`] trait impls.
//! 2. The `[package.metadata.nros.board] framework = "rtic"` knob
//!    in `Cargo.toml` so 216.D.1's `nros check` reads the routing.
//! 3. The [`RticStm32F4::DISPATCHERS`] const declaring which RTIC
//!    interrupt slots the proc-macro splices into the generated
//!    `#[rtic::app(dispatchers = …)]` attribute.
//!
//! Everything else is `todo!()`:
//!
//! - [`RticBoardEntry::init_hardware`] body — the actual clock /
//!   pin / Ethernet bringup is lifted from `nros-board-stm32f4`'s
//!   `init_hardware` free fn in Phase 216.B.3 once the proc-macro
//!   integration shape is settled. See the doc-comment on
//!   [`RticBoardEntry::init_hardware`] for the migration checklist
//!   (mirrors the Pattern A escape-hatch in
//!   `examples/stm32f4/rust/talker-rtic/src/main.rs`).
//! - [`RticRuntime::signal_callback`] body — the SPSC producer
//!   wiring lands with the proc-macro emit since both halves of
//!   the queue (board crate produces, generated task consumes) must
//!   agree on the underlying `heapless::spsc::Queue` shape.
//!
//! ## Layering note
//!
//! `nros-platform` sits below `nros` in the dep graph, which means
//! the [`RticBoardEntry::Executor`] associated type cannot be
//! `nros::Executor` at the trait surface (Phase 216.B.1 docs that
//! constraint). The trait surface is opaque; this skeleton picks
//! `()` for the assoc type so the crate `cargo check`s without
//! pulling `nros` itself. The 216.B.3 migration revisits this once
//! the proc-macro emit knows what concrete type the
//! framework-generated `#[local]` storage needs.

#![no_std]

use core::fmt::Arguments;

use nros_platform::{
    BoardExit, BoardInit, BoardPrint, DispatchStrategy, NodeDispatchFn, NodeDispatchRuntime,
    NodeInitFn, NodeRegisterFn, NodeTickFn, RticBoardEntry, SignaledCallback,
};

// Re-export the cortex-m / cortex-m-rt entry attribute + defmt
// macros so user Entry pkgs and the 216.B.3 proc-macro emit can
// reach them through one crate. Mirrors the direct-exec sibling's
// re-export shape.
pub use cortex_m_rt::entry;
pub use defmt;
pub use nros_platform_stm32f4;

/// Phase 216.B.2 — RTIC board ZST.
///
/// Carries the [`BoardInit`] / [`BoardPrint`] / [`BoardExit`]
/// super-trait family plus the [`RticBoardEntry`] hook the
/// `nros::main!()` proc-macro (216.B.3) routes through.
pub struct RticStm32F4;

// ---------------------------------------------------------------
// Board super-trait family — mirrors `nros-board-stm32f4`'s impls.
// ---------------------------------------------------------------

impl BoardInit for RticStm32F4 {
    fn init_hardware() {
        // The platform-trait `BoardInit::init_hardware()` is
        // parameterless and runs at boot. The real hardware
        // bringup happens later inside
        // `RticBoardEntry::init_hardware(device, core)` because
        // RTIC owns the PAC + core peripherals — by the time the
        // `#[rtic::app]` `#[init]` body fires, this no-arg hook
        // has already run, but the PAC/Core handles are wrapped
        // by the framework and only emerge inside the generated
        // `init::Context`.
        //
        // For the skeleton we no-op. Future phases may move
        // pre-RTIC SoC bringup (clock floor, CCM enables that
        // RTIC's `#[init]` assumes) into this hook; today the
        // direct-exec sibling does the same work inside its
        // `BoardEntry::run` body, so leaving this empty keeps
        // parity with the existing pattern.
    }
}

impl BoardPrint for RticStm32F4 {
    fn println(args: Arguments<'_>) {
        defmt::info!("{}", defmt::Display2Format(&args));
    }
}

impl BoardExit for RticStm32F4 {
    fn exit_success() -> ! {
        defmt::info!("nros: application complete; entering idle loop");
        loop {
            cortex_m::asm::wfi();
        }
    }

    fn exit_failure() -> ! {
        defmt::error!("nros: application error; entering idle loop");
        loop {
            cortex_m::asm::wfi();
        }
    }
}

// ---------------------------------------------------------------
// RticBoardEntry impl — Phase 216.B.2 trait surface.
// ---------------------------------------------------------------

impl RticBoardEntry for RticStm32F4 {
    /// STM32F4 HAL Peripheral Access Crate handle — matches the
    /// `device = stm32f4xx_hal::pac` attribute the RTIC
    /// `#[rtic::app]` proc-macro expects.
    type Pac = stm32f4xx_hal::pac::Peripherals;

    /// Cortex-M core peripherals.
    type Core = cortex_m::Peripherals;

    /// Phase 216.B.2 — opaque placeholder. `nros::Executor`
    /// concrete type isn't pulled here because `nros-platform`
    /// sits below `nros` in the dep graph (the trait surface is
    /// opaque for exactly that reason). The 216.B.3 migration
    /// swaps this for the concrete `nros::Executor` once the
    /// proc-macro emit knows what RTIC `#[local]` storage needs.
    type Executor = ();

    /// Phase 216.B.2 — minimal `NodeDispatchRuntime` impl that
    /// advertises [`DispatchStrategy::Deferred`]. The SPSC queue
    /// + RTIC software task plumbing lands with the proc-macro
    /// emit (216.B.3) since both halves of the queue must agree
    /// on the underlying `heapless::spsc::Queue` shape.
    type Runtime = RticRuntime;

    /// RTIC interrupt slots reserved for software tasks. The
    /// proc-macro (216.B.3) splices this into the generated
    /// `#[rtic::app(dispatchers = […])]` attribute. STM32F4 has
    /// plenty of unused USART peripherals; we reserve USART1 +
    /// USART2 for `__nros_dispatch` and `__nros_spin`, matching
    /// the Pattern A escape-hatch in
    /// `examples/stm32f4/rust/talker-rtic/src/main.rs`.
    const DISPATCHERS: &'static [&'static str] = &["USART1", "USART2"];

    fn init_hardware(_device: Self::Pac, _core: Self::Core) -> (Self::Executor, Self::Runtime) {
        // Phase 216.B.2 SKELETON. The real body is filled in
        // by 216.B.3 once the proc-macro routing settles on the
        // concrete `Self::Executor` type. The migration brings in
        // the work the direct-exec sibling does inside its
        // `init_hardware` free fn (lives at
        // `packages/boards/nros-board-stm32f4/src/node.rs`):
        //
        //   1. `rcc.constrain()` → HSE-driven 168 MHz sysclk.
        //   2. Enable DWT cycle counter via DCB + DWT.
        //   3. `clock::init(sysclk_hz)` (DWT-backed monotonic).
        //   4. `nros_platform_stm32f4::sleep::init_clock()`.
        //   5. `#[cfg(feature = "ethernet")]` — RMII pin mux,
        //      `stm32_eth::new_with_mii(...)`, PHY probe.
        //   6. `setup_network(...)` — smoltcp Interface + IP +
        //      SocketSet + `SmoltcpBridge::init()` + the network
        //      poll-callback wiring.
        //   7. Build the `nros::Executor` via `Executor::open(...)`.
        //   8. Build the `RticRuntime` (SPSC `Producer` half).
        //   9. Return `(executor, runtime)`.
        //
        // The escape-hatch reference (Pattern A) lives at
        // `examples/stm32f4/rust/talker-rtic/src/main.rs`; once
        // 216.B.3 lands, that example is the canonical migration
        // target — its `#[init]` body becomes the generated
        // `#[rtic::app]` emit and the per-Node setup moves into
        // the macro's input.
        todo!(
            "Phase 216.B.2 skeleton — `init_hardware` body lands with the \
             216.B.3 proc-macro emit. See doc-comment above for the migration checklist."
        )
    }
}

// ---------------------------------------------------------------
// RticRuntime — `NodeDispatchRuntime` impl skeleton.
// ---------------------------------------------------------------

/// Phase 216.B.2 — board-side dispatch sink for RTIC. The
/// `nros::main!()` proc-macro (216.B.3) stashes this in
/// `#[local]` storage and routes `NodeDispatchRuntime::signal_callback`
/// calls into a `heapless::spsc::Producer`; the consumer half
/// dequeues from inside a framework-spawned `__nros_dispatch`
/// software task.
///
/// Skeleton today: every method `todo!()`s. The SPSC queue +
/// software-task wiring lands with the proc-macro emit since both
/// halves must agree on the queue shape.
pub struct RticRuntime {
    // Phase 216.B.3 fills this with a `heapless::spsc::Producer<…>`
    // half. Empty today so the trait surface compiles without
    // pulling `heapless` (already a workspace dep, but no point
    // before the consumer half exists).
    _private: (),
}

impl RticRuntime {
    /// Construct an empty runtime. The proc-macro emit (216.B.3)
    /// is the only caller; user code never names this fn.
    pub const fn new() -> Self {
        Self { _private: () }
    }
}

impl Default for RticRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl NodeDispatchRuntime for RticRuntime {
    fn register_dispatch_slot_dyn(
        &mut self,
        _register: NodeRegisterFn,
        _init: NodeInitFn,
        _dispatch: NodeDispatchFn,
        _tick: NodeTickFn,
        _name: &'static str,
    ) -> Result<(), ()> {
        // Phase 216.B.2 skeleton — register lands with the
        // proc-macro emit (216.B.3).
        todo!("Phase 216.B.2 — register_dispatch_slot_dyn body lands with 216.B.3")
    }

    fn spin_once(&mut self, _timeout_ms: u32) -> Result<(), ()> {
        // RTIC owns the spin loop via a framework-generated
        // software task (`__nros_spin`); the board-side runtime
        // does not drive the executor directly. The proc-macro
        // emit (216.B.3) decides whether this hook stays a
        // no-op `Ok(())` or routes through the executor handle
        // stashed in `#[local]` storage.
        todo!("Phase 216.B.2 — spin_once body lands with 216.B.3 proc-macro emit")
    }

    fn signal_callback(&mut self, _cb: SignaledCallback<'_>) {
        // Phase 216.B.2 skeleton — the SPSC `Producer::enqueue(…)`
        // call lands with the proc-macro emit (216.B.3) since
        // both halves of the queue must agree on the underlying
        // `heapless::spsc::Queue` shape.
        todo!("Phase 216.B.2 — signal_callback SPSC enqueue lands with 216.B.3")
    }

    fn dispatch_strategy(&self) -> DispatchStrategy {
        // Per the Phase 216.B.1 doc-comment on `RticBoardEntry`,
        // the RTIC runtime impl reports `Deferred` — callbacks
        // fire from a framework-owned software task, not from
        // `spin_once`.
        DispatchStrategy::Deferred
    }
}

/// Convenient prelude module mirroring the direct-exec sibling.
pub mod prelude {
    pub use crate::{RticRuntime, RticStm32F4, entry};
    pub use defmt::{debug, error, info, trace, warn};
}

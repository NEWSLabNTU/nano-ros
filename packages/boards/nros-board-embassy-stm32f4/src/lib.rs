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
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    channel::{Channel, TrySendError},
};

use nros_platform::{
    BoardExit, BoardInit, BoardPrint, DispatchStrategy, EmbassyBoardEntry, NodeDispatchFn,
    NodeDispatchRuntime, NodeInitFn, NodeRegisterFn, NodeTickFn, SignaledCallback,
};

/// Channel depth used by [`EmbassyRuntime`]. Mirrors
/// `<EmbassyStm32F4 as EmbassyBoardEntry>::CHANNEL_CAPACITY` (the
/// trait-default 32). Kept as a free `const` so the static
/// [`CALLBACK_CHANNEL`] declaration below — which can't refer to
/// trait-assoc consts in a generic-expression position without
/// `feature(generic_const_exprs)` — has a concrete value to bind. A
/// compile-time assert keeps them locked together.
pub const CHANNEL_CAPACITY: usize = 32;

const _: () = assert!(
    CHANNEL_CAPACITY == <EmbassyStm32F4 as EmbassyBoardEntry>::CHANNEL_CAPACITY,
    "module-level CHANNEL_CAPACITY drifted from EmbassyBoardEntry::CHANNEL_CAPACITY",
);

/// Channel payload — owned wrapper around [`SignaledCallback<'static>`].
///
/// The wrapper exists for two reasons:
///
/// 1. [`SignaledCallback`] carries a `*mut core::ffi::c_void` which is
///    `!Send`. Embassy's [`Channel`] holds its queue inside an
///    `embassy_sync::blocking_mutex::Mutex<R, RefCell<...>>`, and that
///    mutex requires its protected payload to be `Send` for the
///    [`Channel`] to be `Sync` (the unsafe-impl bound in
///    `embassy_sync::blocking_mutex::Mutex` is `R: RawMutex + Sync, T:
///    ?Sized + Send`). Putting the channel in a `static` therefore
///    requires `Send` on the payload. The Phase 216.A.2 dispatch
///    contract is that the producer
///    (`nros::ExecutorNodeRuntime::signal_callback`) hands over a
///    `ctx_ptr` whose target is owned by the executor and stays valid
///    for the duration of the dispatch. Under that contract,
///    transferring the envelope across the Embassy task boundary is
///    sound; the `unsafe impl Send` below concentrates the assumption.
/// 2. The trait surface is `signal_callback(&mut self, cb:
///    SignaledCallback<'_>)`, but static-channel storage requires
///    `SignaledCallback<'static>`. The `'_` is a lifetime erasure in
///    the trait signature; today's only producer
///    (`ExecutorNodeRuntime`) sources `cb_id` from a `&'static str`
///    on `nros::CallbackId` and `ctx_ptr` is a raw pointer with no
///    lifetime of its own. The lifetime extension is a no-op at
///    runtime; see the comment at the call site in
///    [`EmbassyRuntime::signal_callback`].
#[repr(transparent)]
struct SignaledCallbackEnvelope(SignaledCallback<'static>);

// SAFETY: see the doc comment on `SignaledCallbackEnvelope`. The
// `*mut c_void` `ctx_ptr` field is the sole reason `SignaledCallback`
// is `!Send`; the Phase 216.A.2 dispatch contract guarantees the
// target lives for the dispatcher's lifetime, so handing the envelope
// to the Embassy dispatch task is sound.
unsafe impl Send for SignaledCallbackEnvelope {}

/// Static callback queue backing [`EmbassyRuntime`]. Single channel
/// per binary — the macro-generated `#[embassy_executor::main]` body
/// hands `&CALLBACK_CHANNEL` to both the [`EmbassyRuntime`] and the
/// long-lived dispatch task (Phase 216.C.2 follow-up wiring in
/// `init_hardware`; 216.C.3 follow-up for the task side).
///
/// `CriticalSectionRawMutex` matches the existing `EmbassyRuntime`
/// channel-shape comment (and is the safe default for single-core
/// Cortex-M where producers may run from ISR context). Boards that
/// know their producers are task-only could swap to
/// `NoopRawMutex` in a follow-up.
static CALLBACK_CHANNEL: Channel<
    CriticalSectionRawMutex,
    SignaledCallbackEnvelope,
    CHANNEL_CAPACITY,
> = Channel::new();

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
/// The channel handle is a `&'static` borrow of the crate-level
/// [`CALLBACK_CHANNEL`]. [`EmbassyRuntime::new`] returns a runtime
/// already wired to that static; the macro-generated
/// `#[embassy_executor::main]` body (Phase 216.C.3) hands the same
/// `&CALLBACK_CHANNEL` to the long-lived dispatch task so it can
/// `receive` whatever [`signal_callback`] enqueues.
pub struct EmbassyRuntime {
    /// Producer end of the per-board callback channel. The receiver
    /// side is the dispatch task spawned by `init_hardware`
    /// (216.C.2 follow-up) and driven by `nros::main!()`-emitted glue
    /// (216.C.3 follow-up).
    channel: &'static Channel<CriticalSectionRawMutex, SignaledCallbackEnvelope, CHANNEL_CAPACITY>,
}

impl EmbassyRuntime {
    /// Build an [`EmbassyRuntime`] bound to the crate-level static
    /// [`CALLBACK_CHANNEL`]. Phase 216.C.2 follow-up — `init_hardware`
    /// pairs this with a dispatch task that calls
    /// `CALLBACK_CHANNEL.receive()`.
    pub const fn new() -> Self {
        Self {
            channel: &CALLBACK_CHANNEL,
        }
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

    fn signal_callback(&mut self, cb: SignaledCallback<'_>) {
        // Phase 216.C.2 follow-up — non-blocking enqueue. Drops on
        // full so the producer (executor / RMW callback path) never
        // blocks; the dispatch task drains and acts on what survives.
        //
        // SAFETY: lifetime extension `SignaledCallback<'_>` →
        // `SignaledCallback<'static>` is sound here because the only
        // producer in tree (`nros::ExecutorNodeRuntime::signal_callback`,
        // Phase 216.A.2) builds the envelope from a `&'static str`
        // `cb_id` (carried by `nros::CallbackId(&'static str)`) and a
        // `*mut c_void` `ctx_ptr` which has no lifetime of its own. The
        // `'_` on the trait surface is a lifetime erasure for callers
        // that don't have an obvious `'static` annotation; no caller
        // hands us a non-`'static` `cb_id` today.
        let envelope = SignaledCallbackEnvelope(unsafe {
            core::mem::transmute::<SignaledCallback<'_>, SignaledCallback<'static>>(cb)
        });
        match self.channel.try_send(envelope) {
            Ok(()) => {}
            Err(TrySendError::Full(dropped)) => {
                // Drop on full. The dispatch task (216.C.3 follow-up)
                // can log a counter; for now surface via defmt so a
                // probe-attached run sees the drop. `defmt::warn!`
                // expands to a no-op when no defmt sink is linked, so
                // host-side `cargo check` stays clean.
                defmt::warn!(
                    "EmbassyRuntime: callback queue full — dropped {}",
                    dropped.0.cb_id
                );
            }
        }
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

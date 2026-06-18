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
//! ## Phase 216.C.2 follow-up — what landed
//!
//! Mirrors the sibling `nros-board-rtic-stm32f4` (Phase 216.B.2
//! follow-up, `0891ed336`) — the Embassy variant flips its `todo!()`
//! skeleton over to a real bringup body so the proc-macro emit
//! (216.C.3) compiles against the real `Self::Executor` type:
//!
//! - [`BoardInit::init_hardware`] — no-op. Embassy never invokes the
//!   parameterless slot (entry runs through `#[embassy_executor::main]`
//!   → `EmbassyBoardEntry::init_hardware(spawner)`); kept here for
//!   trait-impl completeness.
//! - [`BoardPrint::println`] — `defmt::info!("{}", args)` via
//!   `defmt::Display2Format`, matching the sibling RTIC board crate.
//! - [`BoardExit::{exit_success, exit_failure}`] — `defmt::info!`/
//!   `defmt::error!` then a `wfi` idle loop.
//! - [`EmbassyBoardEntry::init_hardware`] —
//!   `embassy_stm32::init(Default::default())` + `nros_rmw_zenoh::register()`
//!   + `Executor::open(&ExecutorConfig::new(locator).domain_id(d).node_name(name))`
//!   where `locator` / `d` / `name` come from `option_env!("NROS_LOCATOR")` /
//!   `option_env!("NROS_DOMAIN_ID")` / `option_env!("NROS_NODE_NAME")` with
//!   fallback to `tcp/127.0.0.1:7447` / `0` / `"nros"`. Returns
//!   `(executor, EmbassyRuntime::new())`. The Spawner is accepted
//!   but unused today (no framework-task spawn yet); the
//!   proc-macro emit owns the spin + dispatch sidekicks.
//! - [`EmbassyBoardEntry::Executor`] flips from `()` to
//!   [`::nros::Executor`] — the assoc-type opacity at the trait
//!   surface (`nros-platform` sits below `nros`) is resolved at
//!   this board layer, same as the RTIC sibling.
//!
//! ## Still placeholder
//!
//! - **No transport bringup.** The pre-migration talker-embassy
//!   example never wired ethernet / serial either — it blinked an
//!   LED + spawned placeholder tasks. `Executor::open` without a
//!   reachable zenoh-pico transport will fail when the
//!   `__nros_spin_task` first ticks; a probe-attached run surfaces
//!   the failure via the `panic!` inside `init_hardware`. Wiring
//!   `embassy_net::Ethernet` + a zenoh-pico bridge is the next
//!   216.C wave (parallel to the 216.B.3 spin task body wiring on
//!   the RTIC side).
//! - `embassy_stm32::Config::default()` picks HSI / no PLL; a
//!   future follow-up bumps to HSE → PLL 180 MHz to match the
//!   sibling direct-exec `nros-board-stm32f4` (168 MHz).
//! - [`EmbassyRuntime::spin_once`] returns `Err(())` — Embassy owns the
//!   spin loop via a framework-spawned task, not this sink. (Phase 258
//!   Track 2 retired the `register_dispatch_slot_dyn` registration bridge;
//!   owned-spin registration is via the `install_node_typed` seam.)
//!
//! ## Layering note
//!
//! `nros-platform` sits below `nros` in the dep graph, which means
//! the [`EmbassyBoardEntry::Executor`] associated type **at the
//! trait surface** cannot be `nros::Executor` (Phase 216.C.1 docs
//! that constraint). Concrete board impls — including this crate —
//! resolve the assoc type at the board layer where pulling `nros`
//! is OK (only the platform / cffi crates have to stay below `nros`
//! in the graph). The pre-followup `Self::Executor = ()`
//! placeholder was a stand-in until the bringup body was ready;
//! the follow-up flips to `Self::Executor = ::nros::Executor`,
//! mirroring the RTIC sibling.

#![no_std]
// Phase 216.C.2 follow-up — `nros-platform-stm32f4` /
// `nros-board-common` are carried for symmetry with `nros-board-stm32f4`
// (allocator + libc stubs the link pulls in transitively); the file
// itself doesn't name them. Keep the allow so a fresh build doesn't
// surface as a noisy unused-dep diagnostic. The next 216.C wave —
// transport bringup + Spawner-spawned Ethernet driver — will start
// naming them and the allow can shrink to `unused_imports` then.
#![allow(unused_imports, dead_code)]

use embassy_executor::Spawner;
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    channel::{Channel, TrySendError},
};

use nros_platform::{
    BoardExit, BoardInit, BoardPrint, DispatchStrategy, EmbassyBoardEntry, NodeDispatchRuntime,
    SignaledCallback,
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
pub struct SignaledCallbackEnvelope(SignaledCallback<'static>);

// SAFETY: see the doc comment on `SignaledCallbackEnvelope`. The
// `*mut c_void` `ctx_ptr` field is the sole reason `SignaledCallback`
// is `!Send`; the Phase 216.A.2 dispatch contract guarantees the
// target lives for the dispatcher's lifetime, so handing the envelope
// to the Embassy dispatch task is sound.
unsafe impl Send for SignaledCallbackEnvelope {}

impl SignaledCallbackEnvelope {
    /// Borrow the contained callback. Mirrors the RTIC sibling's
    /// `SignaledCallbackEnvelope::callback` accessor (Phase 216.B.2
    /// follow-up) so a dispatch task pulling envelopes off the
    /// `CALLBACK_CHANNEL` can inspect `cb_id` without consuming.
    pub fn callback(&self) -> &SignaledCallback<'static> {
        &self.0
    }

    /// Unwrap the envelope. The Phase 216 final dispatch task
    /// (`nros::main!()`-emitted `__nros_run_task`) consumes the
    /// envelope this way before routing `(cb_id, ctx_ptr)` into
    /// `Executor::dispatch_callback`. Mirrors the RTIC sibling.
    pub fn into_inner(self) -> SignaledCallback<'static> {
        self.0
    }
}

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

/// Default zenoh locator the Embassy board boots with when no
/// `NROS_LOCATOR` env override is set at build time. Matches the
/// pre-followup hardcoded value so an un-customised `cargo check`
/// produces byte-identical output.
const DEFAULT_LOCATOR: &str = "tcp/127.0.0.1:7447";

/// Default ROS 2 domain id. Mirrors the platform-wide default
/// (`ROS_DOMAIN_ID=0`) and the pre-followup hardcoded value.
const DEFAULT_DOMAIN_ID: u32 = 0;

/// Default node name on the boot-time `ExecutorConfig`. Mirrors the
/// sibling RTIC board crate's default.
const DEFAULT_NODE_NAME: &str = "nros";

/// Parse a decimal `u32` from a string. Returns `None` on empty input
/// or any non-digit byte. Local helper used by the `init_hardware`
/// env-var fallback to decode `option_env!("NROS_DOMAIN_ID")`. Kept
/// in-crate so we don't pull `core::str::FromStr::parse()`
/// (which monomorphises through a formatter path that adds a few KB
/// on `cargo size`).
fn parse_decimal_u32(s: &str) -> Option<u32> {
    let mut result: u32 = 0;
    let mut has_digit = false;
    for b in s.as_bytes() {
        match b {
            b'0'..=b'9' => {
                result = result.checked_mul(10)?.checked_add((*b - b'0') as u32)?;
                has_digit = true;
            }
            _ => return None,
        }
    }
    if has_digit { Some(result) } else { None }
}

/// Embassy-flavored STM32F4 board. Phase 216.C.2 skeleton — every
/// Board / EmbassyBoardEntry method is `todo!()`. The trait surface is
/// what 216.C.3 macro routing needs.
pub struct EmbassyStm32F4;

// ---- Board super-trait family (BoardInit + BoardPrint + BoardExit) ----

impl BoardInit for EmbassyStm32F4 {
    fn init_hardware() {
        // The `BoardInit::init_hardware()` slot is parameterless and
        // runs from the legacy `nros_board_common::run` direct-exec
        // driver — a code path Embassy boards do NOT take (Embassy
        // owns the entry point via `#[embassy_executor::main]`). The
        // real bringup happens inside
        // `EmbassyBoardEntry::init_hardware(spawner)` because Embassy
        // owns the Spawner handle plus the peripheral split via
        // `embassy_stm32::init`.
        //
        // No-op here to keep parity with the sibling
        // `RticBoardEntry`'s skeleton hook (216.B.2 follow-up): the
        // proc-macro emit (216.C.3) routes through the Embassy
        // generator and never invokes this slot. Future phases may
        // promote pre-Spawner SoC bringup (e.g. clock floor) into
        // this hook; today the body is empty.
    }
}

impl BoardPrint for EmbassyStm32F4 {
    fn println(args: core::fmt::Arguments<'_>) {
        // Mirrors the sibling RTIC board crate (216.B.2 follow-up)
        // and the direct-exec `nros-board-stm32f4::Stm32F4` impl:
        // route through `defmt::info!`, which the Entry pkg's
        // `defmt-rtt` global logger surfaces over RTT to a
        // probe-attached host.
        defmt::info!("{}", defmt::Display2Format(&args));
    }
}

impl BoardExit for EmbassyStm32F4 {
    fn exit_success() -> ! {
        // Mirrors the sibling RTIC board crate (216.B.2 follow-up)
        // and the direct-exec `nros-board-stm32f4::Stm32F4` impl: log
        // the completion + enter `wfi` idle. Probe-attached runs
        // observe the defmt line; a flashed board parks until reset.
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

    /// Phase 216 final dispatch hook — non-blocking poll for the
    /// next [`SignaledCallbackEnvelope`] sitting on the per-board
    /// callback channel. The `nros::main!()`-emitted
    /// `__nros_run_task` (`packages/core/nros-macros/src/main_macro.rs`,
    /// `Framework::Embassy` branch) drains this in a tight loop
    /// after each `executor.spin_once(_)` pass so a callback burst
    /// can't starve the executor / Embassy scheduler.
    ///
    /// Returns `None` when the channel is empty (no callback has
    /// been signaled this cycle). The async `recv` sibling could
    /// land later — today's dispatch task pairs `try_recv` with a
    /// short `Timer::after_millis(1)` await for pacing, mirroring
    /// the RTIC sibling's SPSC `Consumer::dequeue` loop. `embassy_sync`
    /// `Channel::try_receive` returns `Err(TryReceiveError::Empty)`
    /// when nothing is queued; we flatten that into the more idiomatic
    /// `Option` shape.
    pub fn try_recv(&self) -> Option<SignaledCallbackEnvelope> {
        self.channel.try_receive().ok()
    }
}

impl Default for EmbassyRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl NodeDispatchRuntime for EmbassyRuntime {
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

    /// Phase 216.C.2 follow-up — wired to the concrete
    /// [`nros::Executor`] now that [`Self::init_hardware`] returns a
    /// real instance. The `Self::Executor` projection feeds the
    /// proc-macro-emitted `#[embassy_executor::main]` body (Phase
    /// 216.C.3,
    /// `packages/core/nros-macros/src/main_macro.rs`), so the macro
    /// stores the executor in a `StaticCell` / task-`#[init]` storage
    /// owned by the `__nros_spin_task` sidekick. The sibling
    /// [`EmbassyBoardEntry`] trait surface keeps this opaque (it
    /// sits below `nros` in the dep graph); the concrete pick lives
    /// here at the board layer, mirroring the RTIC sibling
    /// `nros-board-rtic-stm32f4` (216.B.2 follow-up).
    type Executor = ::nros::Executor;

    type Runtime = EmbassyRuntime;

    // Inherits `CHANNEL_CAPACITY = 32` from the trait default. No
    // override — STM32F4 callback density fits comfortably in 32.

    fn init_hardware(_spawner: Self::Spawner) -> (Self::Executor, Self::Runtime) {
        // Phase 216.C.2 follow-up — real bringup body. Steps mirror
        // the pre-migration Pattern A escape-hatch
        // (`examples/stm32f4/rust/talker-embassy/src/main.rs` before
        // commit `3598b3a2d`) and the sibling RTIC board crate's
        // 216.B.2 follow-up. The Embassy variant differs from RTIC
        // in two places:
        //
        // 1. Peripheral bringup goes through `embassy_stm32::init(…)`
        //    instead of `stm32f4xx_hal::pac::Peripherals::take` +
        //    `cortex_m::Peripherals::take`. Embassy owns its own
        //    RCC / PLL configuration story, so the direct-exec
        //    sibling's `nros_board_stm32f4::init_hardware(&Config,
        //    pac, core)` free fn is NOT reused here — its
        //    `pac::Peripherals` argument conflicts with Embassy's
        //    peripheral split. A future follow-up may introduce a
        //    shared `nros_board_embassy_stm32f4::init_embassy_hardware(spawner)`
        //    free fn (mirroring the RTIC reuse pattern) if the
        //    bringup body grows enough to warrant single-sourcing.
        //
        // 2. The Embassy `Spawner` is passed in (used by follow-up
        //    waves to spawn Ethernet driver + the executor spin
        //    sidekick). Today's body doesn't spawn any framework
        //    tasks yet — the proc-macro emit (216.C.3) owns the
        //    `__nros_spin_task` + `__nros_dispatch_task` sidekicks.
        //
        // Steps:
        //
        //   1. `embassy_stm32::init(Default::default())` — clock
        //      tree + peripheral split. Defaults pick HSI / no PLL;
        //      a future follow-up threads `[[transport]]` knobs from
        //      `nros.toml` via `BoardTransportConfig` so the same
        //      board crate covers NUCLEO-F429ZI (HSE → 180 MHz) and
        //      other STM32F4 variants. The returned `Peripherals`
        //      handle is dropped today — no transport bringup is
        //      wired yet (placeholder, see below).
        //   2. Register the RMW backend before `Executor::open` —
        //      bare-metal has no `.init_array` walk (same as the
        //      sibling RTIC board crate; same as the pre-migration
        //      Pattern A escape-hatch).
        //   3. `Executor::open` with a config matching the RTIC
        //      sibling's defaults (zenoh `tcp/127.0.0.1:7447`
        //      locator, domain 0, node name "nros"). A future
        //      follow-up threads `nros.toml` knobs in.
        //   4. Return `(executor, EmbassyRuntime::new())` — the
        //      runtime wraps a `&'static` borrow of
        //      `CALLBACK_CHANNEL`; the proc-macro emit's dispatch
        //      task drains the receiver side.
        //
        // ### What's still placeholder
        //
        // - **No transport bringup.** The pre-migration
        //   talker-embassy example only blinked an LED + spawned
        //   placeholder tasks — it never wired ethernet / serial
        //   nor opened a real zenoh session. The Pattern A code
        //   the task brief points us to therefore had no real
        //   transport either. `Executor::open` without a
        //   reachable zenoh-pico transport will fail when the
        //   `__nros_spin_task` first ticks; a probe-attached run
        //   surfaces the failure via the `panic!` below. Wiring
        //   `embassy_net::Ethernet` + a `embassy-net`/zenoh-pico
        //   bridge is the next 216.C wave (parallel to the
        //   216.B.3 spin task body wiring on the RTIC side).
        // - `embassy_stm32::Config::default()` picks HSI; a future
        //   follow-up bumps to HSE → PLL 180 MHz to match the
        //   sibling RTIC board crate (which clocks at 168 MHz via
        //   `nros_board_stm32f4`).
        // - `EmbassyRuntime::register_dispatch_slot_dyn` /
        //   `spin_once` still return `Err(())` — same skeleton as
        //   the RTIC sibling; the proc-macro emit wraps an
        //   `ExecutorNodeRuntime` sink and forwards through.
        // - The Spawner is accepted but unused today. Embassy
        //   Ethernet driver task spawn lives in the next wave.

        // Step 1: Embassy HAL bringup. Defaults pick HSI / no PLL;
        // the chip-init returns a `Peripherals` handle the future
        // transport wave needs. Today the handle is dropped — no
        // GPIO / Ethernet wiring yet.
        let _p = embassy_stm32::init(Default::default());

        // Step 2: explicit RMW backend registration (bare-metal
        // targets don't walk `.init_array`). Mirrors the sibling
        // RTIC board crate. `nros_rmw_zenoh::register` is
        // idempotent w.r.t. double-register (returns
        // `Err(AlreadyRegistered)`); we panic on any other error so
        // a probe-attached run surfaces the failure loudly.
        // Phase 248 C1 (#60 T4) — gated behind the optional `rmw-zenoh`
        // feature so the board can build DDS-/XRCE-only.
        #[cfg(feature = "rmw-zenoh")]
        match nros_rmw_zenoh::register() {
            Ok(()) => {}
            Err(e) => {
                defmt::error!(
                    "EmbassyStm32F4::init_hardware: nros_rmw_zenoh::register failed: {:?}",
                    defmt::Debug2Format(&e)
                );
                panic!("nros_rmw_zenoh::register failed");
            }
        }

        // Step 3: open the Executor against the configured locator
        // + domain. Phase 216 follow-up — values come from
        // build-time env vars via [`option_env!`] with a fallback to
        // the previously hardcoded defaults
        // (`tcp/127.0.0.1:7447`, domain 0, node name "nros"). The
        // env-var seam is the pragmatic interim between the
        // previous hardcoded constants and a full `nros.toml` →
        // [`nros_platform::BoardTransportConfig`] reader (which
        // needs a codegen-driven Entry pkg landing first).
        //
        // Override knobs:
        //
        //   - `NROS_LOCATOR` — overrides the zenoh locator
        //   - `NROS_DOMAIN_ID` — overrides the ROS domain id
        //     (parsed decimal)
        //   - `NROS_NODE_NAME` — overrides the node name
        //
        // Default behaviour (no env override) matches the previous
        // hardcoded shape so a fresh `cargo check` is byte-identical.
        // Why no `nros_board_stm32f4::Config` here: the direct-exec
        // sibling board crate depends on `stm32f4xx-hal` which would
        // collide with `embassy-stm32`'s peripheral split at link
        // time, so the Embassy crate carries its own minimal
        // fallback constants rather than reusing the sibling
        // builder. A future `BoardTransportConfig` reader will land
        // a board-local `Config` struct here.
        let locator: &'static str = option_env!("NROS_LOCATOR").unwrap_or(DEFAULT_LOCATOR);
        let domain_id: u32 = option_env!("NROS_DOMAIN_ID")
            .and_then(parse_decimal_u32)
            .unwrap_or(DEFAULT_DOMAIN_ID);
        let node_name: &'static str = option_env!("NROS_NODE_NAME").unwrap_or(DEFAULT_NODE_NAME);

        let exec_config = ::nros::ExecutorConfig::new(locator)
            .domain_id(domain_id)
            .node_name(node_name);
        let executor = match ::nros::Executor::open(&exec_config) {
            Ok(e) => e,
            Err(err) => {
                defmt::error!(
                    "EmbassyStm32F4::init_hardware: Executor::open failed: {:?}",
                    defmt::Debug2Format(&err)
                );
                panic!("Executor::open failed");
            }
        };

        // Step 4: hand back the executor + runtime pair. The
        // proc-macro emit (216.C.3) consumes both, stashing the
        // executor in a `StaticCell` and handing the runtime's
        // backing `&CALLBACK_CHANNEL` to the long-lived
        // `__nros_dispatch_task` sidekick.
        (executor, EmbassyRuntime::new())
    }
}

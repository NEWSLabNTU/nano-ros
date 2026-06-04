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
//! ## Phase 216.B.2 follow-up — what landed
//!
//! 1. [`RticStm32F4`] unit struct + the four
//!    [`BoardInit`] / [`BoardPrint`] / [`BoardExit`] /
//!    [`RticBoardEntry`] trait impls.
//! 2. The `[package.metadata.nros.board] framework = "rtic"` knob
//!    in `Cargo.toml` so 216.D.1's `nros check` reads the routing.
//! 3. The [`RticStm32F4::DISPATCHERS`] const declaring which RTIC
//!    interrupt slots the proc-macro splices into the generated
//!    `#[rtic::app(dispatchers = …)]` attribute.
//! 4. A working [`RticRuntime`] SPSC machinery — a crate-level
//!    `static mut` [`heapless::spsc::Queue`] holding
//!    `SignaledCallback<'static>` envelopes, a one-shot
//!    [`take_dispatch_queue`] splitter exposing the
//!    `(Producer, Consumer)` halves, and an [`RticRuntime::signal_callback`]
//!    body that calls `Producer::enqueue` and surfaces drops via
//!    `defmt::warn!`. Mirrors the sibling `EmbassyRuntime` shape
//!    (Phase 216.C.2 follow-up) with `heapless::spsc` instead of
//!    `embassy_sync::Channel`.
//! 5. An [`RticRuntime::dispatch_strategy`] returning
//!    [`DispatchStrategy::Deferred`] so 216.D.1's `nros check`
//!    cross-validates each Node pkg's `Node::DISPATCH` against the
//!    deferred surface.
//!
//! ## What's now wired (216.B.2 follow-up bringup)
//!
//! - [`RticBoardEntry::init_hardware`] body — clocks / DWT / sleep
//!   / RMII pin mux / `stm32_eth` / smoltcp Interface + IP +
//!   SmoltcpBridge + network poll-callback wiring all happen
//!   inline. The body delegates to the direct-exec sibling
//!   `nros_board_stm32f4::init_hardware(&Config, device, core)` —
//!   the bringup logic is single-sourced across both board
//!   variants. After hardware is up the body calls
//!   `nros_rmw_zenoh::register()` (bare-metal `.init_array`
//!   doesn't walk auto-register sites) and constructs the
//!   `Executor` via `Executor::open(&ExecutorConfig::new(locator)
//!   .domain_id(domain).node_name("nros"))`. The returned
//!   `(Executor, RticRuntime)` pair lands in RTIC `#[local]`
//!   storage where the macro-emitted `__nros_spin` /
//!   `__nros_dispatch` software tasks own them.
//! - [`RticBoardEntry::Executor`] is the concrete
//!   [`::nros::Executor`] — the assoc-type opacity at the
//!   trait surface (`nros-platform` sits below `nros`) is
//!   resolved at this board layer.
//!
//! ## Still `todo!()`
//!
//! - [`NodeDispatchRuntime::register_dispatch_slot_dyn`] +
//!   [`NodeDispatchRuntime::spin_once`] — they return `Err(())`
//!   today. The proc-macro emit (216.B.3) wraps an
//!   `ExecutorNodeRuntime`-style sink and forwards through; spin is
//!   driven by a framework-spawned RTIC software task pulling from
//!   the [`take_dispatch_queue`] consumer half, not from this
//!   trait method.
//! - The macro-emitted `__nros_spin` task body is still
//!   `core::future::pending` (`packages/core/nros-macros/src/main_macro.rs`,
//!   216.B.3 follow-up). The `Executor` is built here so the
//!   dep graph + macro emit compile against the real
//!   `Self::Executor` type, even though the spin task hasn't
//!   driven it yet.
//! - The bringup hardcodes `Config::nucleo_f429zi()` defaults
//!   (NUCLEO-F429ZI IP / MAC / locator). A follow-up threads
//!   `nros.toml` `[[transport]]` / `[node]` knobs in via
//!   [`nros_platform::BoardTransportConfig`].
//!
//! ## Layering note
//!
//! `nros-platform` sits below `nros` in the dep graph, which means
//! the [`RticBoardEntry::Executor`] associated type **at the trait
//! surface** cannot be `nros::Executor` (Phase 216.B.1 docs that
//! constraint). Concrete board impls — including this crate —
//! resolve the assoc type at the board layer where pulling `nros`
//! is OK (only the platform / cffi crates have to stay below
//! `nros` in the graph). The pre-followup `Self::Executor = ()`
//! placeholder was a stand-in until the bringup body was ready;
//! the follow-up flips to `Self::Executor = ::nros::Executor`.

#![no_std]

use core::{
    fmt::Arguments,
    sync::atomic::{AtomicBool, Ordering},
};

use heapless::spsc::{Consumer, Producer, Queue};
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

/// Queue depth used by [`RticRuntime`]. Sized to match the sibling
/// `EmbassyRuntime` channel (Phase 216.C.2 follow-up,
/// `CHANNEL_CAPACITY = 32`). STM32F4 callback density fits
/// comfortably in 32; oversized = wasted SRAM, undersized = drops
/// when bursts arrive faster than the dispatch task drains.
///
/// Kept as a free `const` so the static [`CALLBACK_QUEUE`] declaration
/// below — which can't refer to trait-assoc consts in a
/// generic-expression position without `feature(generic_const_exprs)`
/// — has a concrete value to bind. The 216.B.3 proc-macro emit may
/// expose a board-override knob later.
pub const QUEUE_CAPACITY: usize = 32;

/// Queue payload — owned wrapper around [`SignaledCallback<'static>`].
///
/// The wrapper exists for the same two reasons the sibling
/// `EmbassyRuntime` envelope does (see
/// `packages/boards/nros-board-embassy-stm32f4/src/lib.rs`):
///
/// 1. [`SignaledCallback`] carries a `*mut core::ffi::c_void` which is
///    `!Send`. `heapless::spsc::Queue` is `Sync` when its element type
///    is `Send`; for the crate-level `static mut` storage below to be
///    soundly shared between an interrupt-context producer and a
///    task-context consumer the payload must be `Send`. The
///    `unsafe impl Send` below concentrates the assumption that the
///    Phase 216.A.2 producer
///    (`nros::ExecutorNodeRuntime::signal_callback`) hands over a
///    `ctx_ptr` whose target stays valid for the dispatch lifetime.
/// 2. The trait surface is `signal_callback(&mut self, cb:
///    SignaledCallback<'_>)`, but the static queue stores
///    `SignaledCallback<'static>`. The `'_` is a lifetime erasure in
///    the trait signature; today's only producer
///    (`ExecutorNodeRuntime`) sources `cb_id` from a `&'static str`
///    on `nros::CallbackId` and `ctx_ptr` is a raw pointer with no
///    lifetime of its own. The lifetime extension is a no-op at
///    runtime; see the comment at the call site in
///    [`RticRuntime::signal_callback`].
#[repr(transparent)]
pub struct SignaledCallbackEnvelope(SignaledCallback<'static>);

// SAFETY: see the doc comment on `SignaledCallbackEnvelope`. The
// `*mut c_void` `ctx_ptr` field is the sole reason `SignaledCallback`
// is `!Send`; the Phase 216.A.2 dispatch contract guarantees the
// target lives for the dispatcher's lifetime, so handing the envelope
// across the RTIC task boundary is sound.
unsafe impl Send for SignaledCallbackEnvelope {}

impl SignaledCallbackEnvelope {
    /// Borrow the contained callback. The dispatch task pulls this
    /// to look up the per-Node trampoline via `cb_id` and invoke it
    /// with the `ctx_ptr`.
    pub fn callback(&self) -> &SignaledCallback<'static> {
        &self.0
    }

    /// Unwrap the envelope. The 216.B.3 follow-up dispatch task
    /// consumes the envelope this way before routing to the
    /// per-Node trampoline.
    pub fn into_inner(self) -> SignaledCallback<'static> {
        self.0
    }
}

/// Static callback queue backing [`RticRuntime`]. Single queue per
/// binary — the macro-generated `#[rtic::app]` `#[init]` body
/// (216.B.3 follow-up) calls [`take_dispatch_queue`] exactly once to
/// extract the `(Producer, Consumer)` halves; the producer is stashed
/// inside the [`RticRuntime`] returned by [`RticBoardEntry::init_hardware`]
/// and the consumer is handed to the framework-spawned `__nros_dispatch`
/// software task.
///
/// `Queue::new()` is `const fn`, so the storage lives in `.bss` (or
/// `.data` zero-init) with no `StaticCell` / `MaybeUninit` dance.
/// `static mut` is mandatory because `Queue::split` consumes a
/// `&'static mut Self` — `&'static` alone would block the split. The
/// [`DISPATCH_QUEUE_CLAIMED`] flag below makes the one-shot extraction
/// safe across re-entry.
static mut CALLBACK_QUEUE: Queue<SignaledCallbackEnvelope, QUEUE_CAPACITY> = Queue::new();

/// One-shot extraction guard for [`CALLBACK_QUEUE`]. Flips from `false`
/// → `true` on the first [`take_dispatch_queue`] call; subsequent
/// calls return `None`.
static DISPATCH_QUEUE_CLAIMED: AtomicBool = AtomicBool::new(false);

/// Extract the `(Producer, Consumer)` halves of the crate-level
/// [`CALLBACK_QUEUE`]. The macro-generated `#[rtic::app]` `#[init]`
/// body (216.B.3 follow-up) calls this exactly once at boot:
///
/// - The `Producer` half feeds [`RticRuntime`] (returned by
///   [`RticBoardEntry::init_hardware`] and stashed in RTIC `#[local]`
///   storage).
/// - The `Consumer` half feeds the framework-spawned `__nros_dispatch`
///   software task, which dequeues envelopes and invokes per-Node
///   trampolines.
///
/// Returns `Some((Producer, Consumer))` on the first call,
/// `None` thereafter. The second-call return is what makes calling
/// this from a non-`#[init]` context an obvious bug; the user-facing
/// 216.B.3 macro emit asserts on `expect("dispatch queue already
/// claimed")` to surface the mis-wire loudly.
///
/// # Safety contract
///
/// The first call takes a `&'static mut` reference to
/// [`CALLBACK_QUEUE`] via raw pointer; the [`DISPATCH_QUEUE_CLAIMED`]
/// flag ensures the `&'static mut` is unique. Concurrent calls
/// race on the `compare_exchange` — only one wins, the others
/// observe `Err(true)` and return `None`. Soundness rests on no
/// other code path taking `&CALLBACK_QUEUE` / `&mut CALLBACK_QUEUE`
/// outside this fn; the `static mut` is private to this module.
pub fn take_dispatch_queue() -> Option<(
    Producer<'static, SignaledCallbackEnvelope, QUEUE_CAPACITY>,
    Consumer<'static, SignaledCallbackEnvelope, QUEUE_CAPACITY>,
)> {
    if DISPATCH_QUEUE_CLAIMED
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return None;
    }

    // SAFETY: the `compare_exchange` above guarantees we are the
    // unique caller granted the `&'static mut`. No other code path
    // in this crate touches `CALLBACK_QUEUE`; the `static mut` is
    // private to this module.
    let queue: &'static mut Queue<SignaledCallbackEnvelope, QUEUE_CAPACITY> =
        unsafe { &mut *core::ptr::addr_of_mut!(CALLBACK_QUEUE) };
    Some(queue.split())
}

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

    /// Phase 216.B.2 follow-up — wired to the concrete
    /// [`nros::Executor`] now that [`Self::init_hardware`] returns a
    /// real instance. The `Self::Executor` projection feeds the
    /// proc-macro-emitted `#[local] struct Local { executor: …,
    /// runtime: … }` field
    /// (`packages/core/nros-macros/src/main_macro.rs`), so the
    /// macro emit stores the executor in RTIC `#[local]` storage
    /// owned by the `__nros_spin` task. The sibling
    /// [`RticBoardEntry`] trait surface keeps this opaque (it sits
    /// below `nros` in the dep graph); the concrete pick lives
    /// here at the board layer. Sibling `EmbassyBoardEntry`
    /// (Phase 216.C.2) still has `type Executor = ()` until its
    /// parallel `init_hardware` body lands.
    type Executor = ::nros::Executor;

    /// Phase 216.B.2 follow-up — wired `NodeDispatchRuntime` impl
    /// that owns the SPSC producer half of [`CALLBACK_QUEUE`] and
    /// advertises [`DispatchStrategy::Deferred`]. The 216.B.3
    /// proc-macro emit completes the wiring by spawning a
    /// `__nros_dispatch` software task that drains the consumer
    /// half via [`take_dispatch_consumer`].
    type Runtime = RticRuntime;

    /// RTIC interrupt slots reserved for software tasks. The
    /// proc-macro (216.B.3) splices this into the generated
    /// `#[rtic::app(dispatchers = […])]` attribute. STM32F4 has
    /// plenty of unused USART peripherals; we reserve USART1 +
    /// USART2 for `__nros_dispatch` and `__nros_spin`, matching
    /// the Pattern A escape-hatch in
    /// `examples/stm32f4/rust/talker-rtic/src/main.rs`.
    const DISPATCHERS: &'static [&'static str] = &["USART1", "USART2"];

    fn init_hardware(device: Self::Pac, core: Self::Core) -> (Self::Executor, Self::Runtime) {
        // Phase 216.B.2 follow-up — real bringup body. Steps
        // mirror the pre-migration Pattern A escape-hatch
        // (`examples/stm32f4/rust/talker-rtic/src/main.rs` before
        // commit `a7620ab43`):
        //
        //   1. Build a board `Config` (NUCLEO-F429ZI defaults today;
        //      a future follow-up threads `[[transport]]`/`[node]`
        //      knobs from `nros.toml` via `BoardTransportConfig`).
        //   2. Delegate to the direct-exec sibling's
        //      `nros_board_stm32f4::init_hardware(&config, device, core)`
        //      — that fn brings up clocks (HSE → 168 MHz), DWT
        //      cycle counter, `nros_platform_stm32f4::sleep`, then
        //      (under `ethernet`) RMII pin mux + `stm32_eth` +
        //      smoltcp Interface + IP + SmoltcpBridge + the network
        //      poll-callback wiring. Single-source the bringup so
        //      a fix in the direct-exec board propagates here too.
        //      Drops the returned `SYST` peripheral — RTIC's
        //      dispatchers run on USART1/USART2 (per
        //      [`Self::DISPATCHERS`]), and the proc-macro emit's
        //      `__nros_spin` task doesn't need a monotonic. A
        //      future follow-up may surface the `SYST` for users
        //      who want `rtic-monotonics::systick`; today it's
        //      unused.
        //   3. Register the RMW backend before `Executor::open` —
        //      bare-metal targets don't walk `.init_array`, so the
        //      backend has to be wired explicitly (same call the
        //      legacy Pattern A example made directly).
        //   4. `Executor::open` with a config derived from the
        //      board `Config`. The proc-macro emit (Phase 216.B.3)
        //      stashes the returned `Executor` in RTIC `#[local]`
        //      storage owned by `__nros_spin`.
        //   5. Split the dispatch SPSC queue and stash the
        //      consumer half on `DISPATCH_CONSUMER_SLOT` so the
        //      macro-emitted `__nros_dispatch` task can claim it
        //      (the two halves can't ride together in the
        //      `(Executor, Runtime)` tuple — see the doc-comment
        //      on `DISPATCH_CONSUMER_SLOT`).
        //
        // ### What's still placeholder
        //
        // - `RticRuntime::register_dispatch_slot_dyn` /
        //   `RticRuntime::spin_once` still return `Err(())` — the
        //   proc-macro emit (216.B.3) wraps an
        //   `ExecutorNodeRuntime`-style sink and forwards through.
        // - `Config` is hardcoded to `nucleo_f429zi()`. The
        //   `nros.toml` → `BoardTransportConfig` plumbing is a
        //   separate follow-up; today the locator + IP / MAC come
        //   from the board's default preset.
        // - The macro-emitted `__nros_spin` task body is still
        //   `core::future::pending` (not real `executor.spin_once`);
        //   that lands alongside the per-Node trampoline registration
        //   in the next 216.B wave. The executor is built here so
        //   the macro emit + dep graph compile against the real
        //   `Self::Executor` type, even though the spin task hasn't
        //   driven it yet.

        // Step 1–2: hardware bringup via the direct-exec sibling.
        let config = nros_board_stm32f4::Config::nucleo_f429zi();
        let _syst = nros_board_stm32f4::init_hardware(&config, device, core);

        // Step 3: explicit RMW backend registration (bare-metal
        // has no `.init_array` walk). `nros_rmw_zenoh::register`
        // is idempotent w.r.t. double-register (returns
        // `Err(AlreadyRegistered)`); we panic on any other error
        // so a probe-attached run surfaces the failure loudly.
        match nros_rmw_zenoh::register() {
            Ok(()) => {}
            Err(e) => {
                defmt::error!(
                    "RticStm32F4::init_hardware: nros_rmw_zenoh::register failed: {:?}",
                    defmt::Debug2Format(&e)
                );
                panic!("nros_rmw_zenoh::register failed");
            }
        }

        // Step 4: open the Executor against the configured
        // locator + domain. The proc-macro emit stashes the
        // returned value in RTIC `#[local]` storage owned by
        // `__nros_spin`.
        let exec_config = ::nros::ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("nros");
        let executor = match ::nros::Executor::open(&exec_config) {
            Ok(e) => e,
            Err(err) => {
                defmt::error!(
                    "RticStm32F4::init_hardware: Executor::open failed: {:?}",
                    defmt::Debug2Format(&err)
                );
                panic!("Executor::open failed");
            }
        };

        // Step 5: split the dispatch SPSC + stash the consumer
        // half. Unchanged from the 216.B.2 skeleton — the proc-
        // macro emit fishes the consumer out via
        // `take_dispatch_consumer()` from the `__nros_dispatch`
        // task's `#[init]` setup.
        let (producer, consumer) = take_dispatch_queue()
            .expect("RticStm32F4::init_hardware: dispatch queue already claimed");
        stash_dispatch_consumer(consumer);

        (executor, RticRuntime::with_producer(producer))
    }
}

/// One-shot slot for the dispatch [`Consumer`] half. Populated by
/// [`RticBoardEntry::init_hardware`] via [`stash_dispatch_consumer`]
/// and drained by the 216.B.3 proc-macro-emitted `__nros_dispatch`
/// software task via [`take_dispatch_consumer`].
///
/// The slot lives behind a `static mut Option<…>` because RTIC's
/// `#[local]` storage in the generated app module is populated from
/// the `#[init]` return tuple, and the consumer can't ride along
/// with the `(Executor, Runtime)` pair that already lives in
/// `init::LocalResources` (different task, different `#[local]`
/// fields). The Acquire/Release ordering on [`DISPATCH_CONSUMER_STASHED`]
/// publishes the slot mutation.
static mut DISPATCH_CONSUMER_SLOT: Option<
    Consumer<'static, SignaledCallbackEnvelope, QUEUE_CAPACITY>,
> = None;

/// Tracks whether [`DISPATCH_CONSUMER_SLOT`] is populated. Set on
/// [`stash_dispatch_consumer`]; cleared on [`take_dispatch_consumer`].
static DISPATCH_CONSUMER_STASHED: AtomicBool = AtomicBool::new(false);

fn stash_dispatch_consumer(consumer: Consumer<'static, SignaledCallbackEnvelope, QUEUE_CAPACITY>) {
    // SAFETY: called exactly once from `RticBoardEntry::init_hardware`
    // before any task spawn; no concurrent reader exists yet. The
    // store-Release publishes the slot mutation to subsequent
    // `take_dispatch_consumer` Acquire loads.
    unsafe {
        let slot = core::ptr::addr_of_mut!(DISPATCH_CONSUMER_SLOT);
        (*slot) = Some(consumer);
    }
    DISPATCH_CONSUMER_STASHED.store(true, Ordering::Release);
}

/// Take the stashed dispatch [`Consumer`] half. The 216.B.3
/// proc-macro-emitted `__nros_dispatch` task calls this once to
/// move the consumer into its `#[local]` storage. Returns `None`
/// when called before [`RticBoardEntry::init_hardware`] runs, or
/// when already taken.
pub fn take_dispatch_consumer()
-> Option<Consumer<'static, SignaledCallbackEnvelope, QUEUE_CAPACITY>> {
    if !DISPATCH_CONSUMER_STASHED.swap(false, Ordering::AcqRel) {
        return None;
    }
    // SAFETY: the swap above grants unique access to the slot —
    // the prior `Release` store in `stash_dispatch_consumer` is
    // synchronized by the Acquire side of the swap.
    unsafe {
        let slot = core::ptr::addr_of_mut!(DISPATCH_CONSUMER_SLOT);
        (*slot).take()
    }
}

// ---------------------------------------------------------------
// RticRuntime — `NodeDispatchRuntime` impl.
// ---------------------------------------------------------------

/// Phase 216.B.2 follow-up — board-side dispatch sink for RTIC. The
/// `nros::main!()` proc-macro (216.B.3) stashes this in RTIC
/// `#[local]` storage and the executor-side
/// `nros::ExecutorNodeRuntime` forwards each
/// `NodeDispatchRuntime::signal_callback` call into the contained
/// [`heapless::spsc::Producer`]. The consumer half (extracted via
/// [`take_dispatch_consumer`]) lives in a framework-spawned
/// `__nros_dispatch` software task that drains envelopes and
/// invokes per-Node trampolines via `cb_id` lookup.
///
/// Constructed exclusively by [`RticBoardEntry::init_hardware`];
/// user code never names [`RticRuntime::with_producer`].
pub struct RticRuntime {
    /// Producer half of [`CALLBACK_QUEUE`]. `None` only during the
    /// transient [`RticRuntime::new`] state used by 216.B.3 macro
    /// emit code paths that build the runtime before the queue
    /// split; live runtime always has `Some`.
    producer: Option<Producer<'static, SignaledCallbackEnvelope, QUEUE_CAPACITY>>,
}

impl RticRuntime {
    /// Construct an empty runtime (no producer wired). Useful only
    /// for test fixtures + the 216.B.3 macro emit's interim state;
    /// production callers go through [`RticBoardEntry::init_hardware`]
    /// which calls [`RticRuntime::with_producer`].
    pub const fn new() -> Self {
        Self { producer: None }
    }

    /// Wrap a producer half into a runtime. Called from
    /// [`RticBoardEntry::init_hardware`]; user code does not.
    pub const fn with_producer(
        producer: Producer<'static, SignaledCallbackEnvelope, QUEUE_CAPACITY>,
    ) -> Self {
        Self {
            producer: Some(producer),
        }
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
        // Phase 216.B.2 follow-up — wrap an `ExecutorNodeRuntime`-style
        // sink (lives in `nros::component_runtime`) and forward
        // through. Skeleton: surface unwired by returning `Err(())`
        // so callers fail loudly rather than silently succeed.
        // Mirrors `EmbassyRuntime::register_dispatch_slot_dyn`'s
        // skeleton behavior (Phase 216.C.2 follow-up).
        Err(())
    }

    fn spin_once(&mut self, _timeout_ms: u32) -> Result<(), ()> {
        // RTIC owns the spin loop via a framework-generated
        // software task (`__nros_spin`); the board-side runtime
        // does not drive the executor directly. Returning `Err(())`
        // mirrors the Embassy sibling — the macro-generated
        // dispatch task does the work, and a caller invoking
        // `spin_once` on the runtime is a wiring bug. The 216.B.3
        // proc-macro emit may swap this for an `Ok(())` no-op if
        // any callsite turns out to invoke it harmlessly.
        Err(())
    }

    fn signal_callback(&mut self, cb: SignaledCallback<'_>) {
        // Phase 216.B.2 follow-up — non-blocking SPSC enqueue. Drops
        // on full so the producer (executor / RMW callback path)
        // never blocks; the dispatch task drains and acts on what
        // survives. Mirrors `EmbassyRuntime::signal_callback`
        // (216.C.2 follow-up) with `heapless::spsc::Producer` in
        // place of `embassy_sync::Channel`.
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

        let Some(producer) = self.producer.as_mut() else {
            // No producer wired — the runtime was built via
            // `RticRuntime::new()` (the transient skeleton path)
            // rather than `RticBoardEntry::init_hardware`. Surface
            // via defmt so a probe-attached run sees the
            // mis-wire. `defmt::warn!` expands to a no-op when no
            // defmt sink is linked, so host-side `cargo check`
            // stays clean.
            defmt::warn!(
                "RticRuntime: signal_callback called on producer-less runtime — dropped {}",
                envelope.0.cb_id
            );
            return;
        };

        match producer.enqueue(envelope) {
            Ok(()) => {}
            Err(dropped) => {
                // Queue full. Drop the envelope + surface via
                // defmt. The dispatch task (216.B.3 follow-up)
                // can log a counter once it lands.
                defmt::warn!(
                    "RticRuntime: callback queue full — dropped {}",
                    dropped.0.cb_id
                );
            }
        }
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
    pub use crate::{
        RticRuntime, RticStm32F4, SignaledCallbackEnvelope, entry, take_dispatch_consumer,
        take_dispatch_queue,
    };
    pub use defmt::{debug, error, info, trace, warn};
}

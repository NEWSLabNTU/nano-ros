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
//!   [`::nros::Executor<'static>`] — the assoc-type opacity at the
//!   trait surface (`nros-platform` sits below `nros`) is
//!   resolved at this board layer.
//!
//! ## Still `todo!()`
//!
//! - [`NodeDispatchRuntime::spin_once`] — returns `Err(())`
//!   today. (Phase 258 Track 2 retired the `register_dispatch_slot_dyn`
//!   registration bridge; owned-spin registration is via the
//!   `install_node_typed` seam.) Spin is
//!   driven by a framework-spawned RTIC software task pulling from
//!   the [`take_dispatch_queue`] consumer half, not from this
//!   trait method.
//! - The macro-emitted `__nros_spin` task body is still
//!   `core::future::pending` (`packages/core/nros-macros/src/main_macro.rs`,
//!   216.B.3 follow-up). The `Executor` is built here so the
//!   dep graph + macro emit compile against the real
//!   `Self::Executor` type, even though the spin task hasn't
//!   driven it yet.
//! - The bringup starts from `Config::nucleo_f429zi()` (NUCLEO-F429ZI
//!   IP / MAC defaults) and overlays the locator / domain id / node
//!   name from build-time env vars `NROS_LOCATOR` / `NROS_DOMAIN_ID`
//!   / `NROS_NODE_NAME` (resolved via [`option_env!`]) — values flow
//!   through [`nros_platform::BoardConfig::zenoh_locator`] /
//!   [`nros_platform::BoardConfig::domain_id`] so a future
//!   `nros.toml` reader can swap in without touching the call shape.
//!   A follow-up threads the full `nros.toml` `[[transport]]` /
//!   `[node]` plumbing via [`nros_platform::BoardTransportConfig`]
//!   (needs a codegen-driven Entry pkg landing first).
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
//! the follow-up flips to `Self::Executor = ::nros::Executor<'static>`.

#![no_std]

use core::{
    fmt::Arguments,
    sync::atomic::{AtomicBool, Ordering},
};

use heapless::spsc::{Consumer, Producer, Queue};
use nros_platform::{
    BoardExit, BoardInit, BoardPrint, DeployOverlay, DispatchStrategy, NodeDispatchRuntime,
    RticBoardEntry, SignaledCallback,
};

// Re-export the cortex-m / cortex-m-rt entry attribute + defmt
// macros so user Entry pkgs and the 216.B.3 proc-macro emit can
// reach them through one crate. Mirrors the direct-exec sibling's
// re-export shape.
pub use cortex_m_rt::entry;
pub use defmt;
pub use nros_platform_stm32f4;

// Issue 0028 — provide the single `defmt::timestamp!` that defmt requires every
// binary to define. The RTIC examples collapse their whole body to
// `nros::main!()` and link `defmt_rtt` but never define a timestamp themselves,
// so without this the `_defmt_timestamp` symbol is undefined and they fail to
// link. Defining it here (the crate every RTIC example links, and which the
// plain `#[entry]` `talker` does NOT — it carries its own) gives all RTIC
// examples one provider with no duplicate-symbol risk. Constant 0 mirrors the
// talker; defmt timestamps are cosmetic for these fixtures.
defmt::timestamp!("{=u64:us}", { 0 });

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

/// Parse a decimal `u32` from a string. Returns `None` on empty input or
/// any non-digit byte.
///
/// Used by the `init_hardware` env-var fallback to decode
/// `option_env!("NROS_DOMAIN_ID")`. Local to this crate so we don't pull
/// `core::str::FromStr`'s `parse()` (which monomorphises through a
/// formatter path that adds a few KB on `cargo size`).
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
// Private init helper — single-sources the hardware bringup body
// shared by `init_hardware` (env-var node name) and
// `init_hardware_with_deploy` (boot_config node name).
// ---------------------------------------------------------------

/// Core STM32F4 RTIC bringup: clocks → HAL init → RMW register →
/// `Executor::open` → SPSC split.  `node_name` is the caller-resolved
/// string (from `option_env!` on the non-deploy path, or from
/// `deploy.boot_config` on the deploy path).
///
/// Phase 216.B.2 follow-up — transport config (locator / domain_id)
/// still comes from `option_env!` / `Config::nucleo_f429zi()` defaults.
/// Only the node-name source changes between the two call paths.
/// #178 — hardware-ready deferred-open carrier (see the mps2-an385 sibling).
/// Holds the `'static` transport params so [`RticStm32F4::open_executor`] can
/// open the executor from the RTIC run task instead of `#[init]` (where RTIC
/// masks interrupts, deadlocking the zenoh TCP handshake).
pub struct RticBoot {
    locator: &'static str,
    domain_id: u32,
    node_name: &'static str,
}

fn rtic_stm32f4_init(
    device: stm32f4xx_hal::pac::Peripherals,
    core: cortex_m::Peripherals,
    node_name: &'static str,
) -> (RticBoot, RticRuntime) {
    // Step 1–2: hardware bringup via the direct-exec sibling.
    //
    // Transport config (locator / domain_id) comes from build-time env
    // vars via `option_env!` with a fallback to the board's
    // `Config::nucleo_f429zi()` defaults (today `tcp/192.168.1.1:7447`,
    // domain 0). Locator and domain are deliberately NOT sourced from
    // `deploy.boot_config` here (the RTIC board's
    // `qemu_config_with_overlay` / `qemu_config` pattern is the
    // authoritative locator/domain source for bare-metal boards; see
    // the sibling `nros-board-rtic-mps2-an385`).
    //
    // Override knobs:
    //   - `NROS_LOCATOR`   — overrides `config.zenoh_locator`
    //   - `NROS_DOMAIN_ID` — overrides `config.domain_id` (parsed decimal)
    let mut config = nros_board_stm32f4::Config::nucleo_f429zi();
    if let Some(loc) = option_env!("NROS_LOCATOR") {
        config = config.zenoh_locator(loc);
    }
    if let Some(d) = option_env!("NROS_DOMAIN_ID").and_then(parse_decimal_u32) {
        config = config.domain_id(d);
    }

    let _syst = nros_board_stm32f4::init_hardware(&config, device, core);

    // Step 3: explicit RMW backend registration (bare-metal
    // has no `.init_array` walk). `nros_rmw_zenoh::register`
    // is idempotent w.r.t. double-register (returns
    // `Err(AlreadyRegistered)`); we panic on any other error
    // so a probe-attached run surfaces the failure loudly.
    // Phase 248 C1 (#60 T4) — gated behind the optional `rmw-zenoh`
    // feature so the board can build DDS-/XRCE-only.
    #[cfg(feature = "rmw-zenoh")]
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

    // Step 4: split the dispatch SPSC + stash the consumer half.
    // The proc-macro emit fishes the consumer out via
    // `take_dispatch_consumer()` from the `__nros_run` task.
    let (producer, consumer) =
        take_dispatch_queue().expect("RticStm32F4::init_hardware: dispatch queue already claimed");
    stash_dispatch_consumer(consumer);

    // #178 — do NOT open the executor here (RTIC masks interrupts in `#[init]`,
    // so the blocking zenoh connect would deadlock). Return the `'static`
    // transport params in a `Boot` carrier; `open_executor` (run task) opens it.
    // `config.zenoh_locator` is a `pub &'static str` field (option_env / baked
    // literal) — read it directly, not through the lifetime-erased
    // `BoardConfig::zenoh_locator(&config)` accessor.
    let boot = RticBoot {
        locator: config.zenoh_locator,
        domain_id: config.domain_id,
        node_name,
    };
    (boot, RticRuntime::with_producer(producer))
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
    type Executor = ::nros::Executor<'static>;

    /// Phase 216.B.2 follow-up — wired `NodeDispatchRuntime` impl
    /// that owns the SPSC producer half of [`CALLBACK_QUEUE`] and
    /// advertises [`DispatchStrategy::Deferred`]. The 216.B.3
    /// proc-macro emit completes the wiring by spawning a
    /// `__nros_dispatch` software task that drains the consumer
    /// half via [`take_dispatch_consumer`].
    type Runtime = RticRuntime;

    /// #178 — hardware-ready deferred-open carrier; see [`RticBoot`].
    type Boot = RticBoot;

    /// RTIC interrupt slots reserved for software tasks. The
    /// proc-macro (216.B.3) splices this into the generated
    /// `#[rtic::app(dispatchers = […])]` attribute. STM32F4 has
    /// plenty of unused USART peripherals; we reserve USART1 +
    /// USART2 for `__nros_dispatch` and `__nros_spin`, matching
    /// the Pattern A escape-hatch in
    /// `examples/stm32f4/rust/talker-rtic/src/main.rs`.
    const DISPATCHERS: &'static [&'static str] = &["USART1", "USART2"];

    fn init_hardware(device: Self::Pac, core: Self::Core) -> (Self::Boot, Self::Runtime) {
        // Phase 216.B.2 follow-up — transport config (locator / domain_id)
        // from build-time env vars; node name falls back to the
        // board-historical default `"nros"`. The deploy-overlay path
        // (issue #98 / RFC-0045) goes through `init_hardware_with_deploy`.
        let node_name: &'static str = option_env!("NROS_NODE_NAME").unwrap_or("nros");
        rtic_stm32f4_init(device, core, node_name)
    }

    /// Issue #98 / RFC-0045 — override the default-delegate impl to read the
    /// node name from the baked `.nros_boot_config` in `deploy.boot_config`,
    /// falling back to the board-historical `"nros"`.  Locator and domain keep
    /// their env-var / `nucleo_f429zi()` origin (unchanged from before W4e).
    fn init_hardware_with_deploy(
        device: Self::Pac,
        core: Self::Core,
        deploy: &DeployOverlay,
    ) -> (Self::Boot, Self::Runtime) {
        let node_name = deploy
            .boot_config
            .map(::nros::BootConfig::from_baked)
            .and_then(|b| b.node_name)
            .unwrap_or("nros");
        rtic_stm32f4_init(device, core, node_name)
    }

    /// #178 — the blocking executor open, called from the RTIC run task
    /// (interrupts live). Opening in `#[init]` would deadlock the zenoh
    /// TCP handshake (no timer/RX IRQ while RTIC masks interrupts).
    fn open_executor(boot: Self::Boot) -> Self::Executor {
        let exec_config = ::nros::ExecutorConfig::new(boot.locator)
            .domain_id(boot.domain_id)
            .node_name(boot.node_name);
        match ::nros::Executor::open(&exec_config) {
            Ok(e) => e,
            Err(err) => {
                defmt::error!(
                    "RticStm32F4::open_executor: Executor::open failed: {:?}",
                    defmt::Debug2Format(&err)
                );
                panic!("Executor::open failed");
            }
        }
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

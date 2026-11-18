//! RTIC board-entry surface for QEMU MPS2-AN385 — the `rtic` feature.
//!
//! This surface was once a separate RTIC board crate. That crate was
//! never independent: it depended on this one and called its `init_hardware` /
//! `exit_success` / `exit_failure` / `enable_wfi_idle`, so all it added was the
//! framework-owned entry surface — the deferred-callback SPSC queue, the
//! [`RticBoardEntry`] impl, and the CMSDK dual-timer tick. What it *also* carried
//! was a second board `Config` builder and a second ~~`mask_to_prefix`~~, which
//! drifted from this crate's (see [`crate::Config::qemu_slirp`]).
//!
//! The [`RticMps2An385`] ZST stays a distinct type from [`crate::Mps2An385`]:
//! they are two entry shapes on one board (RTIC owns its own reset vector and
//! dispatch; direct-exec spins inline), and the board descriptor's
//! `entry_types` names this one for an image with `framework = "rtic"`.

use core::{
    fmt::Arguments,
    sync::atomic::{AtomicBool, Ordering},
};
// Only the synthetic-callback E2E path uses this.
#[cfg(feature = "e2e-synthetic-callback")]
use core::mem::MaybeUninit;

use heapless::spsc::{Consumer, Producer, Queue};
#[cfg(feature = "e2e-synthetic-callback")]
use nros::__macro::PublisherResolver;
use nros_platform::{
    BoardExit, BoardInit, BoardPrint, DeployOverlay, DispatchStrategy, NodeDispatchRuntime,
    RticBoardEntry, SignaledCallback,
};

use crate::Config;

pub const QUEUE_CAPACITY: usize = 32;

#[repr(transparent)]
pub struct SignaledCallbackEnvelope(SignaledCallback<'static>);

unsafe impl Send for SignaledCallbackEnvelope {}

impl SignaledCallbackEnvelope {
    pub fn into_inner(self) -> SignaledCallback<'static> {
        self.0
    }
}

static mut CALLBACK_QUEUE: Queue<SignaledCallbackEnvelope, QUEUE_CAPACITY> = Queue::new();
static DISPATCH_QUEUE_CLAIMED: AtomicBool = AtomicBool::new(false);

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

    // SAFETY: the claim flag grants unique access to the private static queue.
    let queue: &'static mut Queue<SignaledCallbackEnvelope, QUEUE_CAPACITY> =
        unsafe { &mut *core::ptr::addr_of_mut!(CALLBACK_QUEUE) };
    Some(queue.split())
}

static mut DISPATCH_CONSUMER_SLOT: Option<
    Consumer<'static, SignaledCallbackEnvelope, QUEUE_CAPACITY>,
> = None;
static DISPATCH_CONSUMER_STASHED: AtomicBool = AtomicBool::new(false);

fn stash_dispatch_consumer(consumer: Consumer<'static, SignaledCallbackEnvelope, QUEUE_CAPACITY>) {
    // SAFETY: called once during RTIC init before tasks spawn.
    unsafe {
        let slot = core::ptr::addr_of_mut!(DISPATCH_CONSUMER_SLOT);
        (*slot) = Some(consumer);
    }
    DISPATCH_CONSUMER_STASHED.store(true, Ordering::Release);
}

pub fn take_dispatch_consumer()
-> Option<Consumer<'static, SignaledCallbackEnvelope, QUEUE_CAPACITY>> {
    if !DISPATCH_CONSUMER_STASHED.swap(false, Ordering::AcqRel) {
        return None;
    }
    // SAFETY: the swap grants unique access to the slot.
    unsafe {
        let slot = core::ptr::addr_of_mut!(DISPATCH_CONSUMER_SLOT);
        (*slot).take()
    }
}

pub struct RticRuntime {
    producer: Option<Producer<'static, SignaledCallbackEnvelope, QUEUE_CAPACITY>>,
}

impl RticRuntime {
    pub const fn new() -> Self {
        Self { producer: None }
    }

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
        Err(())
    }

    fn signal_callback(&mut self, cb: SignaledCallback<'_>) {
        let envelope = SignaledCallbackEnvelope(unsafe {
            core::mem::transmute::<SignaledCallback<'_>, SignaledCallback<'static>>(cb)
        });
        if let Some(producer) = self.producer.as_mut() {
            let _ = producer.enqueue(envelope);
        }
    }

    fn dispatch_strategy(&self) -> DispatchStrategy {
        DispatchStrategy::Deferred
    }
}

pub struct RticMps2An385;

impl BoardInit for RticMps2An385 {
    fn init_hardware() {}
}

impl BoardPrint for RticMps2An385 {
    fn println(args: Arguments<'_>) {
        cortex_m_semihosting::hprintln!("{}", args);
    }
}

impl BoardExit for RticMps2An385 {
    fn exit_success() -> ! {
        crate::exit_success()
    }

    fn exit_failure() -> ! {
        crate::exit_failure()
    }
}

/// Phase 244.D1 — overlay the leaf `system.toml`'s `[image.*]` deployment
/// identity onto [`Config::qemu_slirp`], so each RTIC Entry pkg can pin its own
/// address / locator (required when the talker-rtic + listener-rtic pub/sub
/// pair share this board on one QEMU network). `None` fields keep the baked
/// default; the address is the image's resolved network (phase-497 W2).
fn config_with_overlay(deploy: &DeployOverlay) -> Config {
    let mut config = Config::qemu_slirp();
    if let Some(locator) = deploy.locator {
        config.zenoh_locator = locator;
    }
    config.apply_image_net(deploy.net);
    if let Some(domain_id) = deploy.domain_id {
        config.domain_id = domain_id;
    }
    config
}

/// Hardware-ready deferred-open carrier. Holds the `'static` transport
/// parameters so [`RticMps2An385::open_executor`] can open the executor from
/// the RTIC run task (interrupts live) instead of `#[init]` (interrupts masked,
/// which would deadlock the zenoh TCP handshake).
pub struct RticBoot {
    locator: &'static str,
    domain_id: u32,
    node_name: &'static str,
}

/// Shared RTIC `#[init]` body: bring up the board from `config`, register the
/// linked RMW backend, split the dispatch SPSC, and build the dispatch runtime.
/// #178 — does NOT open the executor (that blocking connect is deferred to
/// [`RticMps2An385::open_executor`]); returns the `(RticBoot, RticRuntime)` pair.
///
/// `deploy` — the leaf `system.toml`'s `[image.*]` overlay; `None` on
/// the no-deploy code path.  Issue #98 / RFC-0045 — node name comes from
/// `deploy.boot_config` (the baked `.nros_boot_config`), falling back to the
/// board-historical default `"nros-rtic-mps2"`.
fn init_with_config(config: Config, deploy: Option<&DeployOverlay>) -> (RticBoot, RticRuntime) {
    crate::init_hardware(&config);

    // Phase 289 (#191 class) — install the agnostic `nros_log` dispatcher so
    // the Node pkgs' `log_info!` marker lines (`Publishing:` / `I heard:`)
    // reach the semihosting console. The direct-exec entry does this in its
    // `BoardEntry` boot (`crate::entry`); the RTIC entry never did — nodes
    // registered their `Logger` against an uninitialized dispatcher and every
    // record was silently dropped.
    nros_platform_cffi::log::init_default();

    // phase-338 W7 — bridge `log` on the RTIC path too, not just `entry::boot`.
    // Without this an RTIC node body written against `log::info!` compiles and
    // prints NOTHING, which is the silent-failure mode W7.b exists to avoid:
    // the facade must not depend on which of the board's two boot paths ran.
    crate::log_bridge::install_semihosting_log_bridge();

    // Phase 248 C1 (#60 T4) — gated behind the optional `rmw-zenoh`
    // feature so the board can build DDS-/XRCE-only; another `nros-rmw-*`
    // crate then registers the linked backend. (No network I/O — a
    // section-free registry insert — so it stays in `#[init]`.)
    #[cfg(feature = "rmw-zenoh")]
    match nros_rmw_zenoh::register() {
        Ok(()) => {}
        Err(_) => {
            crate::exit_failure();
        }
    }

    // Issue #98 / RFC-0045 — node name from the baked `.nros_boot_config`
    // when a deploy overlay is present; fall back to `option_env!("NROS_NODE_NAME")`
    // (compile-time override, mirrors `nros-board-embassy-stm32f4::init_hardware`)
    // and finally to the board-historical default so undeployed firmware keeps
    // its prior identity.
    let node_name = deploy
        .and_then(|d| d.boot_config)
        .map(::nros::BootConfig::from_baked)
        .and_then(|b| b.node_name)
        .or(option_env!("NROS_NODE_NAME"))
        .unwrap_or("nros-rtic-mps2");

    let (producer, consumer) = take_dispatch_queue()
        .expect("RticMps2An385::init_hardware: dispatch queue already claimed");
    stash_dispatch_consumer(consumer);
    // `mut` is only needed by the synthetic-callback enqueue below.
    #[cfg_attr(not(feature = "e2e-synthetic-callback"), allow(unused_mut))]
    let mut runtime = RticRuntime::with_producer(producer);

    #[cfg(feature = "e2e-synthetic-callback")]
    enqueue_e2e_callback(&mut runtime);

    // `config.zenoh_locator` is a `&'static str` (option_env / baked literal);
    // node_name likewise — so the carrier is `'static`, no borrow of `config`.
    let boot = RticBoot {
        locator: config.zenoh_locator,
        domain_id: config.domain_id,
        node_name,
    };
    (boot, runtime)
}

// phase-521 W6 (RFC-0113 D2) — the 1 ms CMSDK dual-timer tick (phase-289) is
// gone. It existed to wake the `wfi` inside the run task's busy-waits, and it
// woke an idle image 1000 times a second for no work. The run task now parks
// in the port's wake object (`tickless`: a SysTick one-shot at the deadline
// plus the LAN9118 RX interrupt, which RTIC binds to `on_rx_irq`), as the
// direct-exec boot does. The dual timer is free again; TIMER0 stays the
// monotonic clock and TIMER1 the sporadic-server timer (issue 1804).

impl RticBoardEntry for RticMps2An385 {
    type Pac = mps2_an385_pac::Peripherals;
    type Core = cortex_m::Peripherals;
    type Executor = ::nros::Executor<'static>;
    type Runtime = RticRuntime;
    type Boot = RticBoot;

    /// phase-562 W10 -- one RTIC software-task priority per dispatcher, so
    /// this list bounds how many derived levels an image may have. `I2S`
    /// (NVIC 14) and `TSC` (NVIC 15) are added because QEMU's mps2 model
    /// instantiates no I2S or touch-screen controller: nothing ever raises
    /// those lines, so pending them is the only way they fire. The macro
    /// keeps its own copy (it cannot read this at expansion time) and
    /// asserts at compile time that the two agree.
    const DISPATCHERS: &'static [&'static str] = &["UARTRX0", "UARTTX0", "I2S", "TSC"];

    fn init_hardware(_device: Self::Pac, _core: Self::Core) -> (Self::Boot, Self::Runtime) {
        init_with_config(Config::qemu_slirp(), None)
    }

    fn init_hardware_with_deploy(
        _device: Self::Pac,
        _core: Self::Core,
        deploy: &DeployOverlay,
    ) -> (Self::Boot, Self::Runtime) {
        init_with_config(config_with_overlay(deploy), Some(deploy))
    }

    /// #178 — the blocking executor open, called from the RTIC run task.
    fn open_executor(boot: Self::Boot) -> Self::Executor {
        let exec_config = ::nros::ExecutorConfig::new(boot.locator)
            .domain_id(boot.domain_id)
            .node_name(boot.node_name);
        match ::nros::Executor::open(&exec_config) {
            Ok(e) => e,
            Err(_) => crate::exit_failure(),
        }
    }

    /// phase-521 W6 (RFC-0113 D4) — the LAN9118 RX interrupt, an RTIC
    /// hardware task (`binds = ETHERNET`). It masks the source; the wake is
    /// the point: the run task's park (`tickless`) sees the frame pending and
    /// returns, and the next drive reads it.
    fn on_rx_irq() {
        #[cfg(feature = "ethernet")]
        crate::tickless::on_eth_rx_irq();
    }

    /// phase-521 W6 — install the tickless idle (SysTick one-shot + LAN9118
    /// RX interrupt) now that interrupts are live and the network is up, so
    /// the zenoh connect and every executor park sleep to their deadline or
    /// the next frame. It replaced the phase-289 `wfi` hook and its 1 ms tick.
    fn on_interrupts_live() {
        #[cfg(feature = "ethernet")]
        crate::tickless::install();
    }

    // phase-562 W10 -- a multi-level image's executor lock and level wake.

    fn level_lock_init(ceiling: u8) -> bool {
        level_lock::init(ceiling)
    }

    unsafe extern "C" fn level_lock(_ctx: *mut core::ffi::c_void) -> i8 {
        level_lock::lock()
    }

    unsafe extern "C" fn level_unlock(_ctx: *mut core::ffi::c_void) -> i8 {
        level_lock::unlock()
    }

    unsafe extern "C" fn level_thread_id() -> usize {
        level_lock::active_exception()
    }

    fn arm_level_wake(after_us: Option<u64>) {
        level_wake::arm(after_us);
    }

    fn on_level_wake_irq() {
        level_wake::acknowledge();
    }
}

/// phase-562 W10 -- the executor lock of a multi-level RTIC image: a BASEPRI
/// priority ceiling at the image's most urgent level.
///
/// Holding it masks every level's dispatcher, so no level task enters the
/// executor while another is in its bookkeeping, and nothing blocks: on one
/// core a task that could contend is simply not running. The two hardware
/// tasks above the ceiling (the RX interrupt and the level wake) never take
/// it. Recursive by depth: only the outermost release restores the priority
/// the outermost take raised from.
mod level_lock {
    use core::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

    /// The ceiling as a BASEPRI value; 0 until `init` (and 0 masks nothing).
    static CEILING: AtomicU8 = AtomicU8::new(0);
    /// Holds taken and not yet released.
    static DEPTH: AtomicUsize = AtomicUsize::new(0);
    /// BASEPRI before the outermost take.
    static SAVED: AtomicU8 = AtomicU8::new(0);

    /// RTIC's logical-to-hardware priority mapping: logical 1 is the least
    /// urgent, and a higher logical priority is a numerically LOWER BASEPRI.
    pub(super) fn init(ceiling: u8) -> bool {
        let bits = mps2_an385_pac::NVIC_PRIO_BITS;
        if ceiling == 0 || u32::from(ceiling) >= (1u32 << bits) {
            return false;
        }
        let hw = (((1u16 << bits) - u16::from(ceiling)) << (8 - bits)) as u8;
        CEILING.store(hw, Ordering::Relaxed);
        true
    }

    pub(super) fn lock() -> i8 {
        let ceiling = CEILING.load(Ordering::Relaxed);
        if ceiling == 0 {
            return -1;
        }
        // Raise first: once BASEPRI is at the ceiling no level task can run,
        // so the depth and the saved value below are this holder's alone.
        let before = cortex_m::register::basepri::read();
        // Raising BASEPRI only masks interrupts (a safe call); the restore is
        // the matching `unlock`.
        cortex_m::register::basepri_max::write(ceiling);
        if DEPTH.fetch_add(1, Ordering::Relaxed) == 0 {
            SAVED.store(before, Ordering::Relaxed);
        }
        0
    }

    pub(super) fn unlock() -> i8 {
        match DEPTH.load(Ordering::Relaxed) {
            0 => -1,
            1 => {
                DEPTH.store(0, Ordering::Relaxed);
                // SAFETY: restores the value the outermost `lock` raised from.
                unsafe { cortex_m::register::basepri::write(SAVED.load(Ordering::Relaxed)) };
                0
            }
            n => {
                DEPTH.store(n - 1, Ordering::Relaxed);
                0
            }
        }
    }

    /// The active exception number (IPSR): one per RTIC priority level, as
    /// each level runs on its own dispatcher interrupt. Offset by one so the
    /// value is never 0, which the lock reads as "no thread".
    pub(super) fn active_exception() -> usize {
        let ipsr: u32;
        // SAFETY: reading IPSR has no side effects.
        unsafe { core::arch::asm!("mrs {}, IPSR", out(reg) ipsr, options(nomem, nostack)) };
        (ipsr & 0x1FF) as usize + 1
    }
}

/// phase-562 W10 -- the level wake: a one-shot on the CMSDK dual timer, the
/// only counter here nobody else owns (TIMER0 is the monotonic clock, TIMER1
/// the sporadic-server timer, SysTick the transport idle's one-shot; the dual
/// timer was RTIC's phase-289 tick, removed in phase-521 W6). Its interrupt,
/// `DUALTIMER` (NVIC 10), is the macro's level-wake hardware task.
mod level_wake {
    // CMSDK APB dual timer (QEMU `hw/timer/cmsdk-apb-dualtimer.c`, mapped at
    // 0x4000_2000 by `hw/arm/mps2.c`). Its first counter: LOAD 0x00, CONTROL
    // 0x08 (bit0 one-shot, bit1 32-bit, bit5 INTEN, bit7 EN), INTCLR 0x0C (any
    // write clears). Clocked at the 25 MHz system clock.
    const BASE: usize = 0x4000_2000;
    const LOAD: usize = 0x00;
    const CONTROL: usize = 0x08;
    const INTCLR: usize = 0x0C;
    const CTRL_ONESHOT: u32 = 1 << 0;
    const CTRL_SIZE_32: u32 = 1 << 1;
    const CTRL_INTEN: u32 = 1 << 5;
    const CTRL_EN: u32 = 1 << 7;
    const TICKS_PER_US: u64 = 25;

    const _: () = assert!(
        BASE != nros_platform_mps2_an385::clock::CLOCK_TIMER_BASE
            && BASE != nros_platform_mps2_an385::sporadic_timer::SPORADIC_TIMER_BASE,
        "the level wake must not share a counter with the clock or the sporadic server"
    );

    fn write(off: usize, v: u32) {
        // SAFETY: CMSDK dual-timer MMIO on a single core. `arm` writes inside
        // a critical section; `acknowledge` writes only INTCLR, from the timer's
        // own interrupt, and a clear landing between `arm`'s writes clears an
        // interrupt `arm` is about to re-arm anyway.
        unsafe { core::ptr::write_volatile((BASE + off) as *mut u32, v) };
    }

    pub(super) fn arm(after_us: Option<u64>) {
        cortex_m::interrupt::free(|_| {
            write(CONTROL, 0);
            write(INTCLR, 1);
            if let Some(us) = after_us {
                // A zero load never fires, so a due deadline fires at once on
                // the next tick. A deadline past 32 bits (~171 s) fires early,
                // and the level then re-arms for the rest.
                let ticks = (us.saturating_mul(TICKS_PER_US)).clamp(1, u64::from(u32::MAX)) as u32;
                write(LOAD, ticks);
                write(CONTROL, CTRL_EN | CTRL_INTEN | CTRL_SIZE_32 | CTRL_ONESHOT);
            }
        });
    }

    pub(super) fn acknowledge() {
        write(INTCLR, 1);
    }
}

#[cfg(feature = "e2e-synthetic-callback")]
struct NoopResolver;

#[cfg(feature = "e2e-synthetic-callback")]
impl PublisherResolver for NoopResolver {
    fn publish_raw(&self, _entity_id: &str, _data: &[u8]) -> ::nros::NodeResult<()> {
        Ok(())
    }
}

#[cfg(feature = "e2e-synthetic-callback")]
static NOOP_RESOLVER: NoopResolver = NoopResolver;

#[cfg(feature = "e2e-synthetic-callback")]
static mut E2E_CTX: MaybeUninit<::nros::CallbackCtx<'static>> = MaybeUninit::uninit();

#[cfg(feature = "e2e-synthetic-callback")]
fn enqueue_e2e_callback(runtime: &mut RticRuntime) {
    // SAFETY: initialized once during RTIC init before tasks spawn; the storage
    // is static and lives for the firmware lifetime.
    let ctx: &'static mut ::nros::CallbackCtx<'static> = unsafe {
        let slot = core::ptr::addr_of_mut!(E2E_CTX);
        (*slot).write(::nros::CallbackCtx::new(&[], &NOOP_RESOLVER));
        (&mut *slot).assume_init_mut()
    };
    runtime.signal_callback(SignaledCallback {
        cb_id: "__nros_e2e",
        ctx_ptr: ctx as *mut ::nros::CallbackCtx<'static> as *mut core::ffi::c_void,
    });
}

pub mod prelude {
    pub use crate::{
        entry,
        rtic::{
            RticMps2An385, RticRuntime, SignaledCallbackEnvelope, take_dispatch_consumer,
            take_dispatch_queue,
        },
    };
}

//! RTIC board-entry support for QEMU MPS2-AN385.
//!
//! This crate exists so Phase 216.B can be validated end-to-end in an
//! emulator. It reuses the direct-exec `nros-board-mps2-an385` hardware
//! bringup, then adds the framework-owned RTIC entry surface and deferred
//! callback SPSC queue expected by `nros::main!()`.

#![no_std]

use core::{
    fmt::Arguments,
    sync::atomic::{AtomicBool, Ordering},
};
// Only the synthetic-callback E2E path uses these.
#[cfg(feature = "e2e-synthetic-callback")]
use core::mem::MaybeUninit;

use heapless::spsc::{Consumer, Producer, Queue};
#[cfg(feature = "e2e-synthetic-callback")]
use nros::PublisherResolver;
use nros_platform::{
    BoardExit, BoardInit, BoardPrint, DispatchStrategy, NodeDispatchRuntime, RticBoardEntry,
    SignaledCallback,
};

pub use cortex_m_rt::entry;
pub use nros_board_mps2_an385;

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
        nros_board_mps2_an385::exit_success()
    }

    fn exit_failure() -> ! {
        nros_board_mps2_an385::exit_failure()
    }
}

fn parse_decimal_u32(s: &str) -> Option<u32> {
    let mut result = 0u32;
    let mut any = false;
    for b in s.as_bytes() {
        match b {
            b'0'..=b'9' => {
                result = result.checked_mul(10)?.checked_add((*b - b'0') as u32)?;
                any = true;
            }
            _ => return None,
        }
    }
    any.then_some(result)
}

fn qemu_config() -> nros_board_mps2_an385::Config {
    let locator = option_env!("NROS_LOCATOR").unwrap_or("tcp/10.0.2.2:7450");
    let domain_id = option_env!("NROS_DOMAIN_ID")
        .and_then(parse_decimal_u32)
        .unwrap_or(0);
    nros_board_mps2_an385::Config {
        mac: [0x02, 0x00, 0x00, 0x00, 0x00, 0x00],
        ip: [10, 0, 2, 10],
        prefix: 24,
        gateway: [10, 0, 2, 2],
        zenoh_locator: locator,
        domain_id,
    }
}

/// Phase 244.D1 — overlay a `[package.metadata.nros.deploy.rtic-mps2-an385]`
/// block onto [`qemu_config`], so each RTIC Entry pkg can pin its own
/// ip / locator / gateway (required when the talker-rtic + listener-rtic
/// pub/sub pair share this board on one QEMU network). `None` fields keep the
/// baked default.
fn qemu_config_with_overlay(
    deploy: &nros_platform::DeployOverlay,
) -> nros_board_mps2_an385::Config {
    let mut config = qemu_config();
    if let Some(locator) = deploy.locator {
        config.zenoh_locator = locator;
    }
    if let Some(ip) = deploy.ip {
        config.ip = ip;
    }
    if let Some(gateway) = deploy.gateway {
        config.gateway = gateway;
    }
    if let Some(netmask) = deploy.netmask {
        config.prefix = netmask.iter().map(|b| b.count_ones() as u8).sum();
    }
    if let Some(domain_id) = deploy.domain_id {
        config.domain_id = domain_id;
    }
    config
}

/// Shared RTIC `#[init]` body: bring up the board from `config`, register the
/// linked RMW backend, open the executor, and build the dispatch runtime.
///
/// `deploy` — the `[package.metadata.nros.deploy.<board>]` overlay; `None` on
/// the no-deploy code path.  Issue #98 / RFC-0045 — node name comes from
/// `deploy.boot_config` (the baked `.nros_boot_config`), falling back to the
/// board-historical default `"nros-rtic-mps2"`.
fn init_with_config(
    config: nros_board_mps2_an385::Config,
    deploy: Option<&nros_platform::DeployOverlay>,
) -> (::nros::Executor, RticRuntime) {
    nros_board_mps2_an385::init_hardware(&config);

    // Phase 248 C1 (#60 T4) — gated behind the optional `rmw-zenoh`
    // feature so the board can build DDS-/XRCE-only; another `nros-rmw-*`
    // crate then registers the linked backend.
    #[cfg(feature = "rmw-zenoh")]
    match nros_rmw_zenoh::register() {
        Ok(()) => {}
        Err(_) => {
            nros_board_mps2_an385::exit_failure();
        }
    }

    // Issue #98 / RFC-0045 — node name from the baked `.nros_boot_config`
    // when a deploy overlay is present; fall back to the board-historical
    // default so undeployed firmware keeps its prior identity.
    let node_name = deploy
        .and_then(|d| d.boot_config)
        .map(::nros::BootConfig::from_baked)
        .and_then(|b| b.node_name)
        .unwrap_or("nros-rtic-mps2");
    let exec_config = ::nros::ExecutorConfig::new(config.zenoh_locator)
        .domain_id(config.domain_id)
        .node_name(node_name);
    let executor = match ::nros::Executor::open(&exec_config) {
        Ok(e) => e,
        Err(_) => nros_board_mps2_an385::exit_failure(),
    };

    let (producer, consumer) = take_dispatch_queue()
        .expect("RticMps2An385::init_hardware: dispatch queue already claimed");
    stash_dispatch_consumer(consumer);
    // `mut` is only needed by the synthetic-callback enqueue below.
    #[cfg_attr(not(feature = "e2e-synthetic-callback"), allow(unused_mut))]
    let mut runtime = RticRuntime::with_producer(producer);

    #[cfg(feature = "e2e-synthetic-callback")]
    enqueue_e2e_callback(&mut runtime);

    (executor, runtime)
}

impl RticBoardEntry for RticMps2An385 {
    type Pac = mps2_an385_pac::Peripherals;
    type Core = cortex_m::Peripherals;
    type Executor = ::nros::Executor;
    type Runtime = RticRuntime;

    const DISPATCHERS: &'static [&'static str] = &["UARTRX0", "UARTTX0"];

    fn init_hardware(_device: Self::Pac, _core: Self::Core) -> (Self::Executor, Self::Runtime) {
        init_with_config(qemu_config(), None)
    }

    fn init_hardware_with_deploy(
        _device: Self::Pac,
        _core: Self::Core,
        deploy: &nros_platform::DeployOverlay,
    ) -> (Self::Executor, Self::Runtime) {
        init_with_config(qemu_config_with_overlay(deploy), Some(deploy))
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
        RticMps2An385, RticRuntime, SignaledCallbackEnvelope, entry, take_dispatch_consumer,
        take_dispatch_queue,
    };
}

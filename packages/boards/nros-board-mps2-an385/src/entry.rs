//! Phase 244.D1 enabler — `nros_platform::BoardEntry` for the pure
//! bare-metal (no-RTOS) MPS2-AN385 board.
//!
//! Mirrors the FreeRTOS family driver's `BoardEntry` shim
//! (`nros-board-freertos/src/entry.rs`) but for direct bare-metal Cortex-M
//! execution: there is no kernel task to spawn and no scheduler to start, so
//! the boot scaffold runs inline on the reset thread —
//! `init_hardware` (clock + ethernet/serial bring-up) → open the `Executor`
//! → wrap it in an `ExecutorNodeRuntime` + `RuntimeCtx` → hand it to the
//! codegen-emitted `setup` closure (the launch-resolved `register(...)`
//! calls) → spin forever. The reset entry itself is emitted by `nros::main!()`
//! (`#[cortex_m_rt::entry]`); this file owns only the post-reset boot body.
//!
//! The linked RMW backend is registered by the macro
//! (`__register_linked_rmw()` before `BoardEntry::run_with_deploy`), so this
//! board stays RMW-agnostic — it never names a concrete backend.

use nros::{BootConfig, Executor, ExecutorConfig, node_runtime::ExecutorNodeRuntime};
use nros_platform::{
    BakedBootConfig, BoardEntry, BoardExit, BoardInit, BoardPrint, DeployOverlay,
    NodeDispatchRuntime, RuntimeCtx,
};

use crate::{Config, init_hardware, node::Mps2An385};

// Additive impls of the new `nros_platform::board` trait set (parameterless
// `init_hardware`) that `BoardEntry: Board` requires. The legacy
// `nros_board_common::{BoardInit,BoardPrint,BoardExit}` impls in `node.rs` stay
// for the `run(Config, closure)` path; these mirror their bodies. Real
// hardware init runs in `boot()` via `crate::init_hardware(&cfg)`, so the
// parameterless trait method is a no-op.
impl BoardInit for Mps2An385 {
    fn init_hardware() {}
}

impl BoardPrint for Mps2An385 {
    fn println(args: core::fmt::Arguments<'_>) {
        use core::fmt::Write;
        if let Ok(mut stdout) = crate::cortex_m_semihosting::hio::hstdout() {
            let _ = writeln!(stdout, "{args}");
        }
    }
}

impl BoardExit for Mps2An385 {
    fn exit_success() -> ! {
        crate::exit_success()
    }

    fn exit_failure() -> ! {
        crate::exit_failure()
    }
}

/// The board's boot [`Config`] before any deploy overlay. Ethernet is the
/// default link; a board built `serial`-only (no `ethernet`) boots the UART
/// link (`Config::serial_default`, locator `serial/UART_0#…`) — phase-244.D1
/// serial deploys. Ethernet wins when both features are on (its `Config` has
/// the full field set).
#[cfg(feature = "ethernet")]
fn base_config() -> Config {
    Config::default()
}
#[cfg(all(feature = "serial", not(feature = "ethernet")))]
fn base_config() -> Config {
    Config::serial_default()
}

/// Build the board boot [`Config`] from the per-link base default, overlaying
/// any `[image.*]` fields the leaf's `system.toml` supplied
/// (issue #48 cause 1). `None` fields keep the board default. The ip/gateway/
/// netmask overlay is ethernet-only (the serial `Config` has no IP fields); the
/// locator + domain overlay applies to both links.
fn config_with_overlay(deploy: &DeployOverlay) -> Config {
    let mut cfg = base_config();
    if let Some(locator) = deploy.locator {
        cfg.zenoh_locator = locator;
    }
    #[cfg(feature = "ethernet")]
    {
        if let Some(ip) = deploy.ip {
            cfg.ip = ip;
        }
        if let Some(gateway) = deploy.gateway {
            cfg.gateway = gateway;
        }
        if let Some(netmask) = deploy.netmask {
            // phase-337 W6.a — was a local popcount `mask_to_prefix`, one of
            // two copies (the other in the folded RTIC crate). One spelling.
            cfg.prefix = nros_board_common::prefix_from_netmask(netmask);
        }
    }
    if let Some(domain_id) = deploy.domain_id {
        cfg.domain_id = domain_id;
    }
    cfg
}

/// Shared boot body: hardware/network bring-up → executor → runtime → user
/// setup → spin. Never returns on the happy path (the firmware loops for its
/// lifetime); a setup `Err` propagates out so the macro can route to
/// `exit_failure`.
///
/// `boot_config` — the baked `.nros_boot_config` static from `nros::main!()`,
/// supplied by `run_with_deploy` (issue #98 / RFC-0045). `None` when called
/// from the no-deploy `run` path (keeps historical `"nros_app"` default).
fn boot<F, E>(cfg: Config, boot_config: Option<&'static BakedBootConfig>, setup: F) -> Result<(), E>
where
    F: FnOnce(&mut RuntimeCtx<'_>) -> Result<(), E>,
    E: core::fmt::Debug,
{
    // Clock + ethernet/serial bring-up. Must precede any executor / socket op.
    init_hardware(&cfg);

    // Phase 244.D1 — install the agnostic `nros_log` dispatcher so declarative
    // nodes can `log_info!` (the mps2-an385 semihosting `PlatformLog` already
    // ships; this only wires the dispatcher to the default sinks). Replaces the
    // per-example `nros_log::init(...)` that used to live in each talker's boot
    // closure. Nodes still `register_logger(&LOGGER)` in their `register()`.
    nros_platform_cffi::log::init_default();

    // phase-338 W7 — bridge the `log` facade too, so BOTH work here.
    //
    // This board was the last one whose node bodies had to use `log_info!`:
    // every other platform bridges `log`, so a body written against `log`
    // compiled here and printed nothing. That made the logging facade a board
    // property leaking into user source, which is the defect class this phase
    // exists to remove.
    //
    // Note the direction. W7 was drafted the other way round — add `nros_log`
    // to the boards that lack it — on the premise that `log` needs `std`.
    // `no_std` is indeed not what stops `log`: THIS board bridges it on a
    // `no_std` target through semihosting, one line below. So `log` is the
    // user-facing facade wherever it can be installed and `nros_log` stays the
    // platform/ABI layer (it is what `nros_platform_log_write`, and therefore
    // the C API, is built on).
    //
    // Issue 1048 — W7's evidence for that premise USED to be
    // esp32-c3-baremetal ("it bridges `log` on a `no_std` target through
    // `esp_println`"), and that claim was FALSE: the call was there, it did
    // nothing, and nobody had read the console. `log::set_logger` is
    // `#[cfg(target_has_atomic = "ptr")]`, esp32-c3 is `riscv32imc` (no `A`
    // extension), so on that board the facade cannot hold a logger at all and
    // dropped every record for as long as the claim stood. What separates the
    // two boards is ATOMICS, not `std` and not `no_std`: thumbv7m has them,
    // riscv32imc and thumbv6m do not. A board on a non-atomic target must
    // deliver through `nros_log` + the platform writer, which is why that half
    // is not optional anywhere.
    crate::log_bridge::install_semihosting_log_bridge();

    // Phase 248 C5a (#60 T4) — the board owns RMW selection: register the linked
    // zenoh backend into the CFFI vtable here, before `Executor::open`.
    // Bare-metal (`target_os = "none"`) is linkme-blind + runs no `.init_array`,
    // so the auto-register section is a no-op; this explicit, idempotent call is
    // the registration path (mirrors `crate::rtic::init_with_config`).
    // Gated on the board's own `rmw-zenoh` feature so DDS-/XRCE-only builds drop it.
    #[cfg(feature = "rmw-zenoh")]
    if let Err(err) = nros_rmw_zenoh::register() {
        Mps2An385::println(format_args!(""));
        Mps2An385::println(format_args!("zenoh RMW register failed: {err:?}"));
        Mps2An385::exit_failure();
    }

    // Issue #98 / RFC-0045 — node name from the baked `.nros_boot_config` (a
    // launch that names the node overrides the board default); locator/domain
    // unchanged from the board config (NOT env vars — bare-metal libc has no
    // host `getenv` trampoline on QEMU).
    let baked = boot_config.map(BootConfig::from_baked).unwrap_or_default();
    // Issue 1434 — ONE spelling of the board rung
    // (`BootConfig::over_board_defaults`). This was a hand-written struct
    // literal, six of them across four board crates, and every one wrote
    // `namespace: None` — so a launch-declared namespace reached the blob,
    // `from_baked` read it, and the board dropped it here. Identity (name,
    // namespace) comes from the bake; the locator and domain stay the
    // board's, unchanged, and issue 1050's `rmw` rides the bake as before.
    let exec_cfg = ExecutorConfig::resolve(baked.over_board_defaults(
        cfg.zenoh_locator,
        cfg.domain_id,
        "nros_app",
    ));
    let executor = match Executor::open(&exec_cfg) {
        Ok(executor) => executor,
        Err(err) => {
            Mps2An385::println(format_args!(""));
            Mps2An385::println(format_args!("Executor::open failed: {err:?}"));
            Mps2An385::exit_failure();
        }
    };

    let mut runtime_inner = ExecutorNodeRuntime::from_executor(executor);

    // phase-436 B2 — the first real deadline source in the tree. The executor
    // parks on a `min` over declared deadlines; until now every wired port had
    // a park primitive and nothing to bound it but the caller's budget, so an
    // image slept its whole 10 ms quantum past a TCP retransmit smoltcp had
    // already scheduled.
    //
    // The registration lives HERE and not in the driver because the layering
    // runs the other way: `nros-smoltcp` is below `nros-node` and cannot name
    // an `Executor`. It exports the C-ABI pair; the board, which depends on
    // both, joins them. The `ctx` is a `'static` singleton, so it cannot
    // outlive what it points at, and the source is INERT until
    // `set_network_state` arms it.
    //
    // Refusal is reported, never swallowed: a source silently dropped would
    // leave the executor sleeping past a deadline it had been told about.
    #[cfg(feature = "ethernet")]
    match runtime_inner.executor_mut().register_wake_source(
        nros_smoltcp::next_deadline_us,
        nros_smoltcp::deadline_source_ctx(),
    ) {
        Ok(id) => Mps2An385::println(format_args!("smoltcp deadline source registered as {id:?}")),
        Err(err) => Mps2An385::println(format_args!("smoltcp deadline source REFUSED: {err:?}")),
    }

    let mut runtime = RuntimeCtx::with_runtime(&mut runtime_inner);

    setup(&mut runtime)?;

    Mps2An385::println(format_args!(""));
    Mps2An385::println(format_args!(
        "Application setup complete — entering spin loop."
    ));
    loop {
        if let Err(err) = NodeDispatchRuntime::spin_once(&mut runtime_inner, 10) {
            Mps2An385::println(format_args!(""));
            Mps2An385::println(format_args!("spin_once error: {err:?}"));
            Mps2An385::exit_failure();
        }
        #[cfg(feature = "ethernet")]
        report_first_platform_park(&mut runtime_inner);
    }
}

/// phase-436 B2 — announce the first park a PLATFORM source won, once.
///
/// Path B's exit criterion is a target run in which `last_park()` attributes a
/// park to `Platform(n)`. `Executor::last_park` records that on every spin and
/// nothing on a Rust board could read it out, so the answer existed and was
/// unobservable — the same shape as the `poll_delay` this work item is about.
/// One line, latched, so a long run is not a log of one message.
///
/// **Silence would be the wrong answer.** A platform source only WINS the park
/// when its deadline beats the caller's budget, and this loop's budget is
/// 10 ms while most of smoltcp's deadlines are longer (a SYN retransmit backs
/// off from 1 s). So "no line" is ambiguous between "the source is not wired"
/// and "the source is wired and further out than the budget" — which is
/// exactly the failure class where nothing printing reads as nothing wrong.
/// After `REPORT_AFTER_SPINS` the report fires anyway and names what DID win,
/// so one line always appears and it always says which of the two happened.
#[cfg(feature = "ethernet")]
fn report_first_platform_park(runtime: &mut ExecutorNodeRuntime) {
    use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

    /// ~10 s at this loop's 10 ms budget. Long enough that a stack with
    /// anything to do has had its turn.
    const REPORT_AFTER_SPINS: u32 = 1000;

    static REPORTED: AtomicBool = AtomicBool::new(false);
    static SPINS: AtomicU32 = AtomicU32::new(0);
    if REPORTED.load(Ordering::Relaxed) {
        return;
    }
    let (bound_us, source) = runtime.executor_mut().last_park();
    match source {
        nros::WakeSourceId::Platform(idx) => {
            REPORTED.store(true, Ordering::Relaxed);
            Mps2An385::println(format_args!(
                "phase-436 B2: park bounded by Platform({idx}) at {bound_us} us"
            ));
        }
        other => {
            let n = SPINS.fetch_add(1, Ordering::Relaxed) + 1;
            if n >= REPORT_AFTER_SPINS {
                REPORTED.store(true, Ordering::Relaxed);
                Mps2An385::println(format_args!(
                    "phase-436 B2: no Platform park in {n} spins; last was \
                     {other:?} at {bound_us} us — the smoltcp source is \
                     registered and further out than the budget"
                ));
            }
        }
    }
}

impl BoardEntry for Mps2An385 {
    fn run<F, E>(setup: F) -> Result<(), E>
    where
        F: FnOnce(&mut RuntimeCtx<'_>) -> Result<(), E>,
        E: core::fmt::Debug,
    {
        boot(base_config(), None, setup)
    }

    fn run_with_deploy<F, E>(deploy: &DeployOverlay, setup: F) -> Result<(), E>
    where
        F: FnOnce(&mut RuntimeCtx<'_>) -> Result<(), E>,
        E: core::fmt::Debug,
    {
        boot(config_with_overlay(deploy), deploy.boot_config, setup)
    }

    /// Phase-244.D1 — install the XRCE-over-UART custom transport when the
    /// deploy overlay requests `transport = "xrce"`. `nros::main!()` calls this
    /// immediately before `__register_linked_rmw()`, so the vtable is in place
    /// before the XRCE backend registers (the ordering `set_custom_transport_ops`
    /// requires). Wraps the board's shared CMSDK UART0 (`framing = true` selects
    /// XRCE HDLC framing for the byte-stream link). No-op without the
    /// `xrce-transport` feature or for any other `transport` value.
    fn setup_transport(deploy: &DeployOverlay) {
        #[cfg(feature = "xrce-transport")]
        if deploy.transport == Some("xrce") {
            let ops = crate::xrce_transport::xrce_transport_ops();
            // SAFETY: `ops`' fn pointers are static; XRCE's custom-transport
            // contract (no concurrent read/write, no ISR invocation) is met by
            // the single-threaded bare-metal executor.
            if unsafe { nros_rmw_xrce_cffi::set_custom_transport_ops(&ops, true) }.is_err() {
                Mps2An385::println(format_args!("XRCE custom transport install failed"));
                Mps2An385::exit_failure();
            }
            // #189 — register the XRCE backend explicitly. Bare-metal runs no
            // `.init_array`, so the linkme auto-register in nros-rmw-xrce-cffi
            // never fires (#163 class), and `__register_linked_rmw()` is a
            // Phase-249 no-op: without this call NO backend is registered and
            // `Executor::open` fails before a single byte reaches the UART.
            // Mirrors the explicit `nros_rmw_zenoh::register()` in `boot()`.
            if let Err(err) = nros_rmw_xrce_cffi::register() {
                Mps2An385::println(format_args!("XRCE RMW register failed: {err:?}"));
                Mps2An385::exit_failure();
            }
        }
        let _ = deploy;
    }
}

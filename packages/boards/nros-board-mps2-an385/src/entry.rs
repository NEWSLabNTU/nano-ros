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

use nros::{Executor, ExecutorConfig, node_runtime::ExecutorNodeRuntime};
use nros_platform::{
    BoardEntry, BoardExit, BoardInit, BoardPrint, DeployOverlay, NodeDispatchRuntime, RuntimeCtx,
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

/// Convert a dotted netmask (`255.255.255.0`) to a CIDR prefix length.
fn mask_to_prefix(mask: [u8; 4]) -> u8 {
    mask.iter().map(|b| b.count_ones() as u8).sum()
}

/// Build the board boot [`Config`] from `Config::default()` (the per-feature
/// ethernet/serial default), overlaying any `[package.metadata.nros.deploy.<board>]`
/// fields the Entry pkg supplied (issue #48 cause 1). `None` fields keep the
/// board default — the bare-metal net threading folded into D1.
fn config_with_overlay(deploy: &DeployOverlay) -> Config {
    let mut cfg = Config::default();
    if let Some(locator) = deploy.locator {
        cfg.zenoh_locator = locator;
    }
    if let Some(ip) = deploy.ip {
        cfg.ip = ip;
    }
    if let Some(gateway) = deploy.gateway {
        cfg.gateway = gateway;
    }
    if let Some(netmask) = deploy.netmask {
        cfg.prefix = mask_to_prefix(netmask);
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
fn boot<F, E>(cfg: Config, setup: F) -> Result<(), E>
where
    F: FnOnce(&mut RuntimeCtx<'_>) -> Result<(), E>,
    E: core::fmt::Debug,
{
    // Clock + ethernet/serial bring-up. Must precede any executor / socket op.
    init_hardware(&cfg);

    // Phase 244.D1 — install the agnostic `nros_log` dispatcher so declarative
    // nodes can `nros_info!` (the mps2-an385 semihosting `PlatformLog` already
    // ships; this only wires the dispatcher to the default sinks). Replaces the
    // per-example `nros_log::init(...)` that used to live in each talker's boot
    // closure. Nodes still `register_logger(&LOGGER)` in their `register()`.
    nros_log::init(nros_log::sinks::default());

    // Phase 248 C5a (#60 T4) — the board owns RMW selection: register the linked
    // zenoh backend into the CFFI vtable here, before `Executor::open`.
    // Bare-metal (`target_os = "none"`) is linkme-blind + runs no `.init_array`,
    // so the auto-register section is a no-op; this explicit, idempotent call is
    // the registration path (mirrors `nros-board-rtic-mps2-an385::init_with_config`).
    // Gated on the board's own `rmw-zenoh` feature so DDS-/XRCE-only builds drop it.
    #[cfg(feature = "rmw-zenoh")]
    if let Err(err) = nros_rmw_zenoh::register() {
        Mps2An385::println(format_args!(""));
        Mps2An385::println(format_args!("zenoh RMW register failed: {err:?}"));
        Mps2An385::exit_failure();
    }

    // Locator + domain come from the board Config (deploy overlay or default),
    // NOT env vars — bare-metal libc has no host `getenv` trampoline on QEMU.
    let exec_cfg = ExecutorConfig::new(cfg.zenoh_locator)
        .domain_id(cfg.domain_id)
        .node_name("nros_app");
    let executor = match Executor::open(&exec_cfg) {
        Ok(executor) => executor,
        Err(err) => {
            Mps2An385::println(format_args!(""));
            Mps2An385::println(format_args!("Executor::open failed: {err:?}"));
            Mps2An385::exit_failure();
        }
    };

    let mut runtime_inner = ExecutorNodeRuntime::from_executor(executor);
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
    }
}

impl BoardEntry for Mps2An385 {
    fn run<F, E>(setup: F) -> Result<(), E>
    where
        F: FnOnce(&mut RuntimeCtx<'_>) -> Result<(), E>,
        E: core::fmt::Debug,
    {
        boot(Config::default(), setup)
    }

    fn run_with_deploy<F, E>(deploy: &DeployOverlay, setup: F) -> Result<(), E>
    where
        F: FnOnce(&mut RuntimeCtx<'_>) -> Result<(), E>,
        E: core::fmt::Debug,
    {
        boot(config_with_overlay(deploy), setup)
    }
}

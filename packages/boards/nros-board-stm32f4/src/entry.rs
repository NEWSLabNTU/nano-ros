//! Phase 244.C5 enabler — `nros_platform::BoardEntry` for the bare-metal
//! STM32F4 board.
//!
//! Mirrors the D1 `nros-board-mps2-an385/src/entry.rs` enabler for direct
//! Cortex-M execution: there is no kernel task to spawn, so the boot scaffold
//! runs inline on the reset thread — `init_hardware` (clock + ethernet/serial
//! bring-up) → open the `Executor` → wrap it in an `ExecutorNodeRuntime` +
//! `RuntimeCtx` → hand it to the codegen-emitted `setup` closure (the
//! launch-resolved `register(...)` calls) → spin forever. The reset entry
//! itself is emitted by `nros::main!()` (`#[cortex_m_rt::entry]`); this file
//! owns only the post-reset boot body.
//!
//! The linked RMW backend is registered by the macro
//! (`__register_linked_rmw()` before `BoardEntry::run_with_deploy`), so this
//! board stays RMW-agnostic — it never names a concrete backend.

use nros::{BootConfig, Executor, ExecutorConfig, node_runtime::ExecutorNodeRuntime};
use nros_platform::{
    BakedBootConfig, BoardEntry, BoardExit, BoardInit, BoardPrint, DeployOverlay,
    NodeDispatchRuntime, RuntimeCtx,
};

use crate::{Config, node::Stm32F4};

// Parameterless `nros_platform::board` trait set that `BoardEntry: Board`
// requires. The legacy `nros_board_common::{BoardInit,BoardPrint,BoardExit}`
// impls in `node.rs` stay for the `run(Config, closure)` path; these mirror
// their bodies. Real hardware init runs in `boot()` via the
// `nros_board_common::BoardInit::init_hardware(&cfg)` (it takes the PAC + core
// peripherals internally), so the parameterless method here is a no-op.
impl BoardInit for Stm32F4 {
    fn init_hardware() {}
}

impl BoardPrint for Stm32F4 {
    fn println(args: core::fmt::Arguments<'_>) {
        defmt::info!("{}", defmt::Display2Format(&args));
    }
}

impl BoardExit for Stm32F4 {
    fn exit_success() -> ! {
        defmt::info!("Entering idle loop");
        loop {
            cortex_m::asm::wfi();
        }
    }

    fn exit_failure() -> ! {
        loop {
            cortex_m::asm::wfi();
        }
    }
}

/// Convert a dotted netmask (`255.255.255.0`) to a CIDR prefix length.
fn mask_to_prefix(mask: [u8; 4]) -> u8 {
    mask.iter().map(|b| b.count_ones() as u8).sum()
}

/// Build the board boot [`Config`] from `Config::default()`, overlaying any
/// `[package.metadata.nros.deploy.<board>]` fields the Entry pkg supplied. `None`
/// fields keep the board default (uses the E5 `DeployOverlay`).
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
/// setup → spin. Never returns on the happy path.
///
/// `boot_config` — the baked `.nros_boot_config` static from `nros::main!()`,
/// supplied by `run_with_deploy` (issue #98 / RFC-0045). `None` when called from
/// the no-deploy `run` path (keeps historical `"nros_app"` default).
fn boot<F, E>(cfg: Config, boot_config: Option<&'static BakedBootConfig>, setup: F) -> Result<(), E>
where
    F: FnOnce(&mut RuntimeCtx<'_>) -> Result<(), E>,
    E: core::fmt::Debug,
{
    // Clock + ethernet/serial bring-up (takes the PAC + core peripherals
    // internally). Must precede any executor / socket op.
    <Stm32F4 as nros_board_common::BoardInit>::init_hardware(&cfg);

    // Agnostic `nros_log` dispatcher so declarative nodes can `nros_info!`
    // without a per-example boot closure (the stm32f4 defmt `PlatformLog` ships
    // already; this only wires the dispatcher to the default sinks).
    nros_log::init(nros_log::sinks::default());

    // Phase 248 C5a (#60 T4) — the board owns RMW selection: register the linked
    // zenoh backend into the CFFI vtable before `Executor::open`. Bare-metal
    // (`target_os = "none"`) is linkme-blind + runs no `.init_array`, so the
    // auto-register section is a no-op; this explicit, idempotent call is the
    // registration path (mirrors `nros-board-rtic-stm32f4::init_hardware`). Gated
    // on the board's own `rmw-zenoh` so DDS-/serial-only builds drop it.
    #[cfg(feature = "rmw-zenoh")]
    if let Err(err) = nros_rmw_zenoh::register() {
        Stm32F4::println(format_args!("zenoh RMW register failed: {err:?}"));
        Stm32F4::exit_failure();
    }

    // Issue #98 / RFC-0045 — node name from the baked `.nros_boot_config` (a
    // launch that names the node overrides the board default); locator/domain
    // unchanged from the board's existing config (NOT env vars — bare-metal
    // libc has no host `getenv` on the target).
    let baked = boot_config.map(BootConfig::from_baked).unwrap_or_default();
    let exec_cfg = ExecutorConfig::resolve(
        BootConfig {
            node_name: baked.node_name.or(Some("nros_app")),
            locator: Some(cfg.zenoh_locator),
            domain_id: Some(cfg.domain_id),
            namespace: None,
        },
        /* hosted_env = */ false,
    );
    let executor = match Executor::open(&exec_cfg) {
        Ok(executor) => executor,
        Err(err) => {
            Stm32F4::println(format_args!("Executor::open failed: {err:?}"));
            Stm32F4::exit_failure();
        }
    };

    let mut runtime_inner = ExecutorNodeRuntime::from_executor(executor);
    let mut runtime = RuntimeCtx::with_runtime(&mut runtime_inner);

    setup(&mut runtime)?;

    Stm32F4::println(format_args!(
        "Application setup complete — entering spin loop."
    ));
    loop {
        if let Err(err) = NodeDispatchRuntime::spin_once(&mut runtime_inner, 10) {
            Stm32F4::println(format_args!("spin_once error: {err:?}"));
            Stm32F4::exit_failure();
        }
    }
}

impl BoardEntry for Stm32F4 {
    fn run<F, E>(setup: F) -> Result<(), E>
    where
        F: FnOnce(&mut RuntimeCtx<'_>) -> Result<(), E>,
        E: core::fmt::Debug,
    {
        boot(Config::default(), None, setup)
    }

    fn run_with_deploy<F, E>(deploy: &DeployOverlay, setup: F) -> Result<(), E>
    where
        F: FnOnce(&mut RuntimeCtx<'_>) -> Result<(), E>,
        E: core::fmt::Debug,
    {
        boot(config_with_overlay(deploy), deploy.boot_config, setup)
    }
}

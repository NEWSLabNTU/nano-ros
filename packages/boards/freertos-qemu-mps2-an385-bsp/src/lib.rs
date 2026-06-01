//! Phase 212.H.3 / M.5.a.3 — FreeRTOS BSP crate (cargo-native adapter).
//!
//! Re-exports `nros-board-mps2-an385-freertos` and layers the Phase 212
//! system-codegen shape on top. The heavyweight FreeRTOS-Kernel + lwIP +
//! LAN9118 compile is owned by the underlying board crate; this crate's
//! `build.rs` only adds `nros_config_generated.h` + `system_main.rs`
//! (per the `docs/design/rtos-integration-pattern.md` §3 contract).
//!
//! # Usage
//!
//! ```ignore
//! // firmware/src/main.rs — 5 lines
//! #![no_std]
//! #![no_main]
//! use panic_semihosting as _;
//! #[unsafe(no_mangle)]
//! extern "C" fn _start() -> ! { freertos_qemu_mps2_an385_bsp::nros_run() }
//! ```
//!
//! M.5.a.3 — the build-script-generated `system_main.rs` is included
//! below; it declares each `[[component]]`'s mangled register fn
//! (M.5.a.1 ABI), bundles them into `NROS_REGISTER_FNS`, and
//! [`nros_run`] walks that slice against an
//! [`nros::ExecutorComponentRuntime`] inside the FreeRTOS application
//! task spun up by `board::run`.

#![no_std]

extern crate alloc;

// Force-link the underlying board crate so its `cargo:rustc-link-lib`
// directives + linker scripts reach the final firmware. Without the
// `extern crate _` reference, cargo would drop the rlib at link time
// for `staticlib`-less consumers.
extern crate nros_board_mps2_an385_freertos as board;
// Keep the active RMW backend's static-init linked in: bare-metal
// FreeRTOS does not walk POSIX constructor sections, so we call
// `nros_rmw_zenoh::register()` explicitly inside `nros_run`.
#[cfg(feature = "rmw-zenoh")]
extern crate nros_rmw_zenoh as _;

pub use nros_board_mps2_an385_freertos::{
    Config, Mps2An385, exit_failure, exit_success, init_hardware, println,
};

// Pull in the build-script-generated bake. Declares:
//   pub const NROS_SYSTEM_NAME / NROS_DOMAIN_ID / NROS_ZENOH_LOCATOR
//   pub static NROS_REGISTER_FNS: &[nros::ComponentRegisterFn]
//   pub const NROS_COMPONENT_NAMES: &[&str]
// plus the `extern "Rust"` decls of each per-pkg
// `__nros_component_<pkg>_register` symbol.
include!(concat!(env!("NROS_SYSTEM_DIR"), "/system_main.rs"));

/// Phase 212.H.3 / M.5.a.3 entry point.
///
/// Drives the codegen-system bake against a live executor:
///
/// 1. Initialise FreeRTOS + lwIP via `board::run` (the per-board
///    closure form already owns network bring-up + the application
///    task spawn).
/// 2. Inside the application task, open an `Executor` against the
///    baked `(NROS_ZENOH_LOCATOR, NROS_DOMAIN_ID)` pair.
/// 3. Walk `NROS_REGISTER_FNS` through an `ExecutorComponentRuntime`
///    — each entry materialises its component's declared nodes /
///    publishers / subscriptions / timers on the executor.
/// 4. Spin until the executor halts (never, on embedded) — the M.5.a.2
///    `DeclarativeSlot` does not dispatch user callback bodies (see
///    M.5.a.2 doc-comment "Coverage today"); registration is the
///    M.5.a.3 acceptance signal.
///
/// Marked `-> !` because the FreeRTOS scheduler never returns.
pub fn nros_run() -> ! {
    let cfg = Config::default();
    board::run(cfg, |cfg: &Config| -> Result<(), &'static str> {
        nros_system_run(cfg)
    })
}

/// Per-task entry — opens the executor, registers every component
/// from `NROS_REGISTER_FNS`, then spins. Factored out so the
/// build-script-generated bake can drive it directly without the
/// turbofish gymnastics `board::run` needs.
pub fn nros_system_run(cfg: &Config) -> Result<(), &'static str> {
    use core::time::Duration;
    use nros::{Executor, ExecutorConfig, component_runtime::ExecutorComponentRuntime};

    println!(
        "[nros-system] bringup `{}` — {} component(s), domain {}",
        NROS_SYSTEM_NAME,
        NROS_REGISTER_FNS.len(),
        NROS_DOMAIN_ID,
    );

    #[cfg(feature = "rmw-zenoh")]
    {
        nros_rmw_zenoh::register().map_err(|_| "nros_rmw_zenoh::register failed")?;
    }

    let locator = if NROS_ZENOH_LOCATOR.is_empty() {
        cfg.zenoh_locator
    } else {
        NROS_ZENOH_LOCATOR
    };
    let exec_cfg = ExecutorConfig::new(locator)
        .domain_id(NROS_DOMAIN_ID)
        .node_name(NROS_SYSTEM_NAME);
    let executor = Executor::open(&exec_cfg).map_err(|_| "Executor::open failed")?;
    let mut runtime = ExecutorComponentRuntime::from_executor(executor);

    // Walk the per-pkg register fns. We can't call
    // `nros_run_components` (std-only spin), so we drive the same
    // shape inline through `register_dispatch_slot` — that's the
    // Phase 212.M.5.a.4 BSP entry which pairs each `_register` with
    // the matching `_init` / `_dispatch` / `_tick` symbols and
    // installs them into an `ExecutorComponentRuntime` slot. Callback
    // bodies now fire from the spin loop.
    assert_eq!(NROS_REGISTER_FNS.len(), NROS_INIT_FNS.len());
    assert_eq!(NROS_REGISTER_FNS.len(), NROS_DISPATCH_FNS.len());
    assert_eq!(NROS_REGISTER_FNS.len(), NROS_TICK_FNS.len());
    for (idx, register_fn) in NROS_REGISTER_FNS.iter().enumerate() {
        let name = NROS_COMPONENT_NAMES
            .get(idx)
            .copied()
            .unwrap_or("<unknown>");
        println!("[nros-system] dispatching register: {}", name);
        runtime
            .register_dispatch_slot(
                *register_fn,
                NROS_INIT_FNS[idx],
                NROS_DISPATCH_FNS[idx],
                NROS_TICK_FNS[idx],
            )
            .map_err(|_| "register_dispatch_slot failed")?;
    }

    println!("[nros-system] entering spin loop");
    loop {
        runtime
            .spin_once(Duration::from_millis(10))
            .map_err(|_| "spin_once failed")?;
    }
}

// Phase 212.M.5.a.4 — the old `register_via_runtime` / `BspRegisterShim`
// hand-rolled shim (which materialised only nodes, dropped entities,
// and no-op'd callbacks) is gone. The BSP now drives
// `ExecutorComponentRuntime::register_dispatch_slot` directly, which
// reuses the same `ExecutorSink` that powers the typed
// `register_component::<C>()` path — entities, pubs, subs, timers, AND
// the per-pkg `on_callback` / `tick` bodies all wire end-to-end.

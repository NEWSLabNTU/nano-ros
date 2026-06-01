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
    // shape inline: a private one-shot sink per pkg, executor
    // borrowed mutably, no callback dispatch (M.5.a.4 follow-up).
    for (idx, register_fn) in NROS_REGISTER_FNS.iter().enumerate() {
        let name = NROS_COMPONENT_NAMES
            .get(idx)
            .copied()
            .unwrap_or("<unknown>");
        println!("[nros-system] dispatching register: {}", name);
        register_via_runtime(&mut runtime, *register_fn)?;
    }

    println!("[nros-system] entering spin loop");
    loop {
        runtime
            .spin_once(Duration::from_millis(10))
            .map_err(|_| "spin_once failed")?;
    }
}

/// `ExecutorComponentRuntime::nros_run_components` lives behind
/// `#[cfg(feature = "std")]` (it owns the halt-flag-driven spin
/// loop). For `no_std` BSPs we register each fn against a private
/// `ComponentContext` directly and let the caller own the spin
/// cadence.
fn register_via_runtime(
    runtime: &mut nros::ExecutorComponentRuntime,
    register_fn: nros::ComponentRegisterFn,
) -> Result<(), &'static str> {
    use nros::ComponentContext;

    let mut shim = BspRegisterShim::new(runtime);
    let runtime_dyn: &mut dyn nros::ComponentRuntime = &mut shim;
    let mut ctx = ComponentContext::new("<bsp>", runtime_dyn);
    (register_fn)(&mut ctx).map_err(|_| "component register failed")
}

/// Trivial `ComponentRuntime` adapter that forwards `create_node`
/// onto the live executor; `create_entity` validates the node
/// reference without realising the entity. The richer typed sink in
/// `ExecutorComponentRuntime::register_component` is the path that
/// wires callbacks; M.5.a.4 generalises this for the BSP bake.
struct BspRegisterShim<'a> {
    runtime: &'a mut nros::ExecutorComponentRuntime,
    nodes: alloc::vec::Vec<(alloc::string::String, nros_node::executor::NodeId)>,
}

impl<'a> BspRegisterShim<'a> {
    fn new(runtime: &'a mut nros::ExecutorComponentRuntime) -> Self {
        Self {
            runtime,
            nodes: alloc::vec::Vec::new(),
        }
    }

    fn contains_node(&self, stable_id: &str) -> bool {
        self.nodes.iter().any(|(s, _)| s == stable_id)
    }
}

impl nros::ComponentRuntime for BspRegisterShim<'_> {
    fn create_node(
        &mut self,
        id: nros::NodeId<'_>,
        options: nros::NodeOptions<'_>,
    ) -> nros::ComponentResult<()> {
        let executor = self.runtime.executor_mut();
        let node_id = executor
            .node_builder(options.name)
            .namespace(options.namespace)
            .domain_id(options.domain_id)
            .build()
            .map_err(|_| nros::ComponentError::Runtime)?;
        self.nodes
            .push((alloc::string::String::from(id.as_str()), node_id));
        Ok(())
    }

    fn create_entity(&mut self, metadata: nros::EntityMetadata) -> nros::ComponentResult<()> {
        if !self.contains_node(metadata.node_id.as_str()) {
            return Err(nros::ComponentError::Runtime);
        }
        Ok(())
    }

    fn record_callback_effect(
        &mut self,
        _callback_id: nros::CallbackId<'_>,
        _kind: nros::CallbackEffectKind,
        _entity_id: nros::EntityId<'_>,
    ) -> nros::ComponentResult<()> {
        Ok(())
    }
}

//! # nros-board-nuttx
//!
//! **Generic NuttX board scaffolding for nano-ros.**
//!
//! Layer-2 entry-point in the board / BSP abstraction described in
//! `docs/design/0012-board-bsp-integration-architecture.md`. Unlike the
//! `nros-board-{freertos, threadx}` siblings, this crate is THIN
//! by design — NuttX owns the kernel build through its own
//! `apps/external/nano-ros/` + `Make.defs` + `Kconfig` integration
//! (see `integrations/nuttx/` and the Phase 152.7 polish). The
//! Cargo side only needs to ship `Config` + `run` + board-init
//! hooks; there is no `build.rs` bundling the NuttX kernel
//! sources here.
//!
//! ## 152.4.A scaffolding
//!
//! Opt-in `reference-qemu-arm` feature re-exports `Config` + `run`
//! from `nros-board-nuttx-qemu-arm` so future overlays
//! (`nros-board-px4-fmu-v5-nuttx`, `nros-board-<vendor>-<board>-nuttx`)
//! depend on this crate name + can extend the `Config` shape +
//! patch board-specific init via `#[no_mangle]` hooks.
//!
//! 152.4.B (deferred) carves the per-board `Config` / `init_hardware`
//! variation into a `BoardInit` trait so the per-board crate
//! shrinks to a `pub struct MyBoard; impl BoardInit for MyBoard
//! { ... }`. Today the per-board crate hand-rolls `Config`.
//!
//! ## Public contract
//!
//! Two boot-driver shapes coexist during the 212.N migration:
//!
//! ### Legacy (152.4.B) — config-carrying
//!
//! - `Config` — TOML-loaded network + zenoh config.
//! - `run(Config, FnOnce(&Config) -> Result<(), E>) -> !` — entry
//!   point. For NuttX this is a regular Rust `main` that initialises
//!   nros + drops into the user closure; the NuttX kernel is already
//!   up by the time `main` runs (NuttX init is the OS, not something
//!   this crate boots). Diverges via `std::process::exit`.
//! - `run_generic::<B>(cfg, f) -> !` — kernel-agnostic generic over
//!   the legacy [`nros_board_common::BoardInit`] (which carries a
//!   `type Config`).
//! - `init_hardware()` — board-specific peripheral wakes (sensors,
//!   displays, vendor-specific GPIO that NuttX's `apps/` discovery
//!   doesn't auto-configure).
//!
//! ### Phase 212.N.2 — `BoardEntry`-shaped `run_entry`
//!
//! - [`run_entry`] (free fn) — mirrors the
//!   [`nros_platform::BoardEntry::run`] signature so codegen-emitted
//!   `main.rs` can call it without owning a [`Config`]. Parameterised
//!   on a 212.N.1 [`nros_platform::BoardInit`] impl `B` whose
//!   `init_hardware()` takes no argument (overlay state, if any,
//!   lives in `B`'s impl block or in a separate per-board `Config`
//!   the Entry pkg threads through the `setup` closure).
//! - Returns the [`Result`] the closure produces. NuttX is hosted +
//!   POSIX-shaped: `fn main` ends, libstd's runtime calls `exit(0)`.
//!   That is the only family in 212.N.2 where `run_entry` does not
//!   diverge — POSIX hands `exit_success` / `_failure` off to libc,
//!   FreeRTOS / ThreadX never let `main` return at all, but NuttX's
//!   shell dispatch reclaims the task on a normal return. Returning
//!   the `Result` keeps it observable to a hosted test harness.
//! - No transport-bringup / network-wait step. NuttX brings up
//!   `eth0` (virtio-net etc.) during kernel boot before `main`
//!   runs; `init_hardware` re-applies IP overrides (qemu-arm overlay
//!   uses `SIOCSIFADDR`) and the 5 s sleep at the top of `run_entry`
//!   covers the virtio-net link-up race documented in `node::run`.
//!
//! ## SDK env-var contract
//!
//! NuttX owns the kernel build; the Cargo side reads:
//!
//! | Var | Purpose |
//! |---|---|
//! | `NUTTX_DIR` | Source root for header discovery (used by `nros-platform-cffi`'s NuttX C port). |
//!
//! Compared to FreeRTOS / ThreadX scaffolds, no kernel-source /
//! port-dir / config-dir env vars are read here. NuttX's own
//! `make menuconfig` + `defconfig` flow drives all of that.

// `std` is reachable (and required by `run_entry` / `run_generic`) when the
// reference feature is on OR the target is NuttX (hosted, ships std). The
// no_std predicate must match the std-using bodies' `cfg(any(feature =
// "reference-qemu-arm", target_os = "nuttx"))` gate — else a NuttX entry
// built WITHOUT the feature (e.g. via `nros-board-nuttx-qemu-arm`) compiles
// this crate as no_std while its `std::` bodies are active → build errors.
#![cfg_attr(not(any(feature = "reference-qemu-arm", target_os = "nuttx")), no_std)]

// Phase 152.4.B — re-export the kernel-agnostic BoardInit trait so
// overlays can `use nros_board_nuttx::BoardInit` without naming
// nros-board-common directly. Once 152.4.B.2's overlay refactor
// lands, the per-board crate impls this trait and the generic
// `run::<B>` shim below consumes it.
pub use nros_board_common::BoardInit;

#[cfg(feature = "reference-qemu-arm")]
pub use nros_board_nuttx_qemu_arm::{Config, init_hardware, run};

/// Phase 152.4.B — generic NuttX entry point.
///
/// Drives every NuttX overlay's boot: invokes the board's
/// `BoardInit::init_hardware`, sleeps briefly for NuttX
/// networking to settle (the kernel runs `NETINIT_*` synchronously
/// before `main`, but virtio-net link-up isn't atomic), then
/// hands control to the user closure. Closure return code maps to
/// `std::process::exit(0)` / `(1)`.
///
/// Per-board overlay's `run` calls into this with the matching
/// `BoardInit` impl:
/// ```ignore
/// pub fn run<F, E>(cfg: Config, f: F) -> !
/// where
///     F: FnOnce(&Config) -> Result<(), E>,
///     E: std::fmt::Debug,
/// {
///     nros_board_nuttx::run_generic::<QemuArmVirt, _, _>(cfg, f)
/// }
/// ```
///
/// Available only when `std` is reachable (NuttX targets bring
/// their own `std`). Bare `cargo check` without a NuttX target +
/// without `reference-qemu-arm` skips the impl.
#[cfg(any(feature = "reference-qemu-arm", target_os = "nuttx"))]
pub fn run_generic<B, F, E>(cfg: B::Config, f: F) -> !
where
    B: BoardInit,
    F: FnOnce(&B::Config) -> std::result::Result<(), E>,
    E: std::fmt::Debug,
{
    B::init_hardware(&cfg);

    // NuttX virtio-net needs a brief warm-up after kernel
    // `NETINIT_*` before `connect()` succeeds.
    std::thread::sleep(std::time::Duration::from_secs(5));

    use std::io::Write as _;
    let _ = std::io::stdout().flush();

    match f(&cfg) {
        Ok(()) => {
            let _ = std::io::stdout().flush();
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("Application error: {:?}", e);
            let _ = std::io::stdout().flush();
            std::process::exit(1);
        }
    }
}

/// Phase 212.N.2 — `BoardEntry`-shaped NuttX entry point.
///
/// Mirrors the [`nros_platform::BoardEntry::run`] signature so the
/// Phase 212.N.4 codegen-emitted Entry pkg `main.rs` can call into
/// the NuttX family driver without owning a [`Config`]:
///
/// ```ignore
/// use nros_board_nuttx::run_entry;
/// use nros_board_nuttx_qemu_arm::QemuArmVirt;
///
/// fn main() -> Result<(), MyError> {
///     run_entry::<QemuArmVirt, _, _>(|runtime| {
///         // codegen-emitted (Phase 212.N.4)
///         run_plan(runtime)
///     })
/// }
/// ```
///
/// ## Lifecycle
///
/// 1. [`nros_platform::BoardInit::init_hardware`] (no-arg variant
///    from the 212.N.1 trait family — distinct from the legacy
///    [`nros_board_common::BoardInit::init_hardware`] which takes a
///    `&Config`). Per-board overlay state, if any, lives inside `B`'s
///    impl block.
/// 2. 5-second NuttX virtio-net warm-up — kernel `NETINIT_*` runs
///    synchronously before `main`, but link-up isn't atomic;
///    `connect_timeout` doesn't observe a partially-up interface.
///    Same magic number `run` / `run_generic` use.
/// 3. Flush stdout (NuttX line-buffers around `write(2)`).
/// 4. Build a [`nros_platform::RuntimeCtx`]. Today this is the
///    [`nros_platform::RuntimeCtx::with_runtime`] placeholder; Phase 212.N.4
///    codegen will populate `params` / `remaps` / `env` from the
///    launch overlay + `--ros-args` CLI parsing.
/// 5. Invoke `setup(&mut runtime)` and **return its result**.
///
/// ## Why this does not diverge
///
/// Sibling family drivers in 212.N.2 each diverge into
/// `BoardExit::exit_*`:
///
/// - `nros-board-posix` calls `std::process::exit(0|1)` —
///   libstd's runtime hands the integer to `_exit(2)`.
/// - `nros-board-freertos` traps in an infinite loop — the FreeRTOS
///   scheduler never permits `main` to return.
/// - `nros-board-threadx` traps similarly — `tx_kernel_enter` never
///   returns.
///
/// NuttX is the carve-out: the shell's task-dispatch loop spawns the
/// application via `task_create` (or `nsh` builtin dispatch) and
/// reclaims the task when its entry returns, exactly like a normal
/// POSIX `main`. Returning the [`Result`] (rather than collapsing to
/// `!` via `exit`) keeps the application status observable to a
/// hosted test harness that wants to drive `run_entry` without
/// killing the test process.
///
/// Production NuttX targets typically pair `run_entry` with the
/// usual `fn main() -> Result<…>` shape; the libstd runtime's
/// `lang_start` then maps `Ok(())` → exit-status-0 and `Err(_)` →
/// exit-status-1 on return, so the user observes the same exit
/// semantics as the diverging siblings.
///
/// ## SDK availability
///
/// Compiled only when `std` is reachable — gated on the same
/// `reference-qemu-arm` / `target_os = "nuttx"` predicate as
/// [`run_generic`] so a bare `cargo check` without a NuttX target
/// + without the reference feature skips this body. The `run_entry`
/// symbol therefore only exists in builds that can actually call it.
#[cfg(any(feature = "reference-qemu-arm", target_os = "nuttx"))]
pub fn run_entry<B, F, E>(
    boot_config: Option<&'static nros_platform::BakedBootConfig>,
    setup: F,
) -> Result<(), E>
where
    B: nros_platform::BoardInit,
    F: FnOnce(&mut nros_platform::RuntimeCtx<'_>) -> Result<(), E>,
    E: core::fmt::Debug,
{
    <B as nros_platform::BoardInit>::init_hardware();

    // NuttX virtio-net needs a brief warm-up after kernel
    // `NETINIT_*` before `connect()` succeeds. Magic number matches
    // `run` / `run_generic`; future work could probe link state
    // via `SIOCGIFFLAGS` instead.
    std::thread::sleep(std::time::Duration::from_secs(5));

    use std::io::Write as _;
    let _ = std::io::stdout().flush();

    // Phase 212.N.7 step-3.5 — open the executor + wrap it in an
    // `ExecutorNodeRuntime` so the codegen-emitted `run_plan(runtime)`
    // body can register components against a live RMW session.
    //
    // Locator/domain are baked at COMPILE time on NuttX, not read from
    // the runtime env. Although NuttX ships `std` + libc `getenv`, the
    // QEMU guest has no environment populated, so `from_env()` would
    // silently fall back to its loopback default (`tcp/127.0.0.1:7447`)
    // — the connection then never leaves the guest over virtio-net and
    // fails fast with `Transport(ConnectionFailed)`. Bake via
    // `option_env!` (the freertos/esp32 pattern; CLAUDE.md "compile-time
    // on embedded") and fall back to `from_env` only when nothing was
    // baked (hosted/dev use).
    const BAKED_LOCATOR: Option<&str> = option_env!("NROS_LOCATOR");
    const BAKED_DOMAIN: Option<&str> = option_env!("NROS_DOMAIN_ID");
    // Issue #98 / RFC-0045 — derive the node name from the baked boot config
    // supplied by `run_with_deploy`; fall back to `"nros_app"` when called from
    // `run` (boot_config = None) or when the baked config carries no name.
    // Hoisted out of the BAKED_LOCATOR match so the no-baked-locator path
    // (`from_env`) also applies the launch-declared node name (W4d fix).
    let node_name: &'static str = boot_config
        .map(::nros::BootConfig::from_baked)
        .and_then(|b| b.node_name)
        .unwrap_or("nros_app");
    let exec_cfg = match BAKED_LOCATOR {
        Some(loc) => {
            let mut cfg = ::nros::ExecutorConfig::new(loc).node_name(node_name);
            if let Some(d) = BAKED_DOMAIN.and_then(|s| s.parse::<u32>().ok()) {
                cfg = cfg.domain_id(d);
            }
            cfg
        }
        None => ::nros::ExecutorConfig::from_env().node_name(node_name),
    };

    // Explicitly register the zenoh RMW backend before opening the executor.
    // The unified-RMW `nros_rmw_register_backend!` macro is a no-op on NuttX
    // (linkme has no NuttX support) and the flat image does not run the
    // auto-register `.init_array` path, so without this the CFFI vtable has
    // no transport and `Executor::open` fails with `Transport(ConnectionFailed)`.
    #[cfg(feature = "rmw-zenoh")]
    if let Err(err) = ::nros_rmw_zenoh::register() {
        eprintln!("nros: zenoh RMW backend register failed: {:?}", err);
    }

    let executor = match ::nros::Executor::open(&exec_cfg) {
        Ok(e) => e,
        Err(err) => {
            eprintln!("Executor::open failed: {:?}", err);
            let _ = std::io::stderr().flush();
            std::process::exit(1);
        }
    };
    // #132 — install a stdout `log::Log` sink so the chatter examples'
    // `log::info!("Publishing:" / "I heard:")` reach the console. The facade is
    // otherwise dark on NuttX, so pub/sub delivery was invisible to the e2e
    // harness even when it worked. Idempotent + before the readiness marker.
    install_stdout_logger();

    // #132 — stable boot-readiness marker. A subscriber-only entry
    // (`listener_entry`) prints nothing until it receives, so the rtos_e2e
    // harness had no line to gate "session up, node registered" on (the C
    // examples' "Waiting for messages" is C-only). Emit one after the session
    // opens and before spin — greppable. The pattern is a test contract.
    println!("nros entry ready");
    let _ = std::io::stdout().flush();

    let mut crt = ::nros::node_runtime::ExecutorNodeRuntime::from_executor(executor);
    let mut runtime = nros_platform::RuntimeCtx::with_runtime(&mut crt);
    let setup_result = setup(&mut runtime);

    let _ = std::io::stdout().flush();
    if let Err(ref e) = setup_result {
        eprintln!("Application error: {:?}", e);
        let _ = std::io::stderr().flush();
        return setup_result;
    }

    // Phase 212.N.7 step-3.5 — embedded RTOS spin loop. NuttX is a
    // shell-dispatched POSIX-style hosted env: returning would have
    // the shell reclaim the task, so the application would stop
    // dispatching component callbacks. Spin forever like the FreeRTOS
    // / ThreadX siblings; the user terminates via signal or shell.
    loop {
        if let Err(err) = nros_platform::NodeDispatchRuntime::spin_once(&mut crt, 10) {
            eprintln!("spin_once error: {:?}", err);
            let _ = std::io::stderr().flush();
            std::process::exit(1);
        }
    }
}

/// #132 — process-wide `log::Log` sink that writes each record to stdout as
/// `<message>` (the examples pre-format the level/prefix into the message
/// text). Installed once by [`run_entry`] so `log::info!` from the chatter /
/// service / action examples reaches the NuttX serial console; without it the
/// `log` facade drops every record on the floor (there is no default sink),
/// and the rtos_e2e harness could not observe pub/sub delivery even though the
/// transport worked. Idempotent — the `log` crate ignores a second
/// `set_logger`, and the `Once` guard avoids the racey double-set path.
#[cfg(any(feature = "reference-qemu-arm", target_os = "nuttx"))]
fn install_stdout_logger() {
    use std::io::Write as _;
    use std::sync::Once;

    struct StdoutLogger;
    impl log::Log for StdoutLogger {
        fn enabled(&self, _: &log::Metadata<'_>) -> bool {
            true
        }
        fn log(&self, record: &log::Record<'_>) {
            // The examples bake the full human line into the message
            // (`Publishing: '...'` / `I heard: [...]`), so emit it verbatim.
            let mut out = std::io::stdout();
            let _ = writeln!(out, "{}", record.args());
            let _ = out.flush();
        }
        fn flush(&self) {
            let _ = std::io::stdout().flush();
        }
    }
    static LOGGER: StdoutLogger = StdoutLogger;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if log::set_logger(&LOGGER).is_ok() {
            log::set_max_level(log::LevelFilter::Trace);
        }
    });
}

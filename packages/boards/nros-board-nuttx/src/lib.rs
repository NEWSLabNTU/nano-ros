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
//! Opt-in `reference-qemu` feature pulls the board overlay crate
//! `nros-board-nuttx-qemu` (one crate, both QEMU witnesses) so overlays
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
// "reference-qemu", target_os = "nuttx"))` gate — else a NuttX entry
// built WITHOUT the feature (e.g. via `nros-board-nuttx-qemu`) compiles
// this crate as no_std while its `std::` bodies are active → build errors.
#![cfg_attr(not(any(feature = "reference-qemu", target_os = "nuttx")), no_std)]

// Phase 313 W-nuttx (#0243) — the legacy `nros_board_common::board_init` path is
// RETIRED for the NuttX family: the generic `run_generic<B>` shim, the
// `nros_board_common::BoardInit` re-export it consumed, and the `reference-qemu`
// scaffolding re-export of the per-board free `run` are all gone. The live entries
// are the `nros_platform`-shaped `run_entry` / `run_tiers` below (consumed by
// `nros::main!` via each board's `impl nros_platform::BoardEntry`).

/// Phase 212.N.2 — `BoardEntry`-shaped NuttX entry point.
///
/// Mirrors the [`nros_platform::BoardEntry::run`] signature so the
/// Phase 212.N.4 codegen-emitted Entry pkg `main.rs` can call into
/// the NuttX family driver without owning a [`Config`]:
///
/// ```ignore
/// use nros_board_nuttx::run_entry;
/// use nros_board_nuttx_qemu::NuttxQemu;
///
/// fn main() -> Result<(), MyError> {
///     run_entry::<NuttxQemu, _, _>(|runtime| {
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
/// - `nros-board-linux` calls `std::process::exit(0|1)` —
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
/// `reference-qemu` / `target_os = "nuttx"` predicate as
/// [`run_generic`] so a bare `cargo check` without a NuttX target
/// + without the reference feature skips this body. The `run_entry`
/// symbol therefore only exists in builds that can actually call it.
#[cfg(any(feature = "reference-qemu", target_os = "nuttx"))]
pub fn run_entry<B, F, E>(
    boot_config: Option<&'static nros_platform::BakedBootConfig>,
    setup: F,
) -> Result<(), E>
where
    B: nros_platform::BoardInit,
    F: FnOnce(&mut nros_platform::RuntimeCtx<'_>) -> Result<(), E>,
    E: core::fmt::Debug,
{
    // issue 0572 — a panic on this guest is INVISIBLE: Rust prints the message
    // and location to stderr, and stderr never reaches the NuttX serial console
    // here (the same finding that hid every `eprintln!` diagnostic in this
    // function). A boot tier that panics after spawning its siblings would look
    // exactly like one that silently stopped scheduling. Route it to stdout.
    // (For 0572 itself this came back NEGATIVE — no panic — which is why the
    // hook stays: that was worth knowing and cost a build to learn.)
    {
        use std::io::Write as _;
        let prev = std::panic::take_hook();
        std::panic::set_hook(std::boxed::Box::new(move |info| {
            println!("nros: PANIC {info}");
            let _ = std::io::stdout().flush();
            prev(info);
        }));
    }

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
        println!("nros: zenoh RMW backend register failed: {:?}", err);
    }

    let executor = match ::nros::Executor::open(&exec_cfg) {
        Ok(e) => e,
        Err(err) => {
            println!("Executor::open failed: {:?}", err);
            let _ = std::io::stdout().flush();
            std::process::exit(1);
        }
    };
    // #132 — install a stdout `log::Log` sink so the chatter examples'
    // `log::info!("Publishing:" / "I heard:")` reach the console. The facade is
    // otherwise dark on NuttX, so pub/sub delivery was invisible to the e2e
    // harness even when it worked. Idempotent + before the readiness marker.
    install_stdout_logger();

    // #132 — stable boot-readiness marker. A subscriber-only entry
    // (`listener-entry`) prints nothing until it receives, so the rtos_e2e
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
        println!("Application error: {:?}", e);
        let _ = std::io::stdout().flush();
        return setup_result;
    }

    // Phase 212.N.7 step-3.5 — embedded RTOS spin loop. NuttX is a
    // shell-dispatched POSIX-style hosted env: returning would have
    // the shell reclaim the task, so the application would stop
    // dispatching component callbacks. Spin forever like the FreeRTOS
    // / ThreadX siblings; the user terminates via signal or shell.
    loop {
        if let Err(err) = nros_platform::NodeDispatchRuntime::spin_once(&mut crt, 10) {
            println!("spin_once error: {:?}", err);
            let _ = std::io::stdout().flush();
            std::process::exit(1);
        }
    }
}

// phase-296 W5.9 — NuttX kernel sporadic server, self-applied on the
// CALLING thread. Defined in the board seam C file (`nuttx_run_tiers.c`,
// compiled by the board crate's build.rs) so `struct sched_param`'s
// config-gated sporadic fields are laid out per THIS kernel's config (the
// #131 layout-mirror trap avoided). Returns 1 when the kernel accepted
// SCHED_SPORADIC, 0 otherwise (no CONFIG_SCHED_SPORADIC / no policy
// declared / kernel rejection — the C side logs the marker or the loud
// fallback; the executor's cooperative Sporadic SchedContext remains the
// enforcement either way).
#[cfg(target_os = "nuttx")]
unsafe extern "C" {
    fn nros_nuttx_apply_current_sporadic(
        name: *const core::ffi::c_char,
        tier_class: *const core::ffi::c_char,
        budget_us: u64,
        period_us: u64,
        priority: i64,
    ) -> i32;
}

// phase-296 W5.11 — NuttX SMP core affinity (the placement dim), self-applied
// on the CALLING thread. Defined in the board seam C file (`nuttx_run_tiers.c`)
// so `cpu_set_t` / `pthread_setaffinity_np` lay out per THIS kernel's config
// (config-gated on CONFIG_SMP — the #131 layout trap avoided; Rust never
// mirrors the set). Returns 1 when the kernel accepted the pin, 0 otherwise
// (unpinned tier / no CONFIG_SMP / rejection — the C side logs the accept
// marker or the loud fallback note). The ABI carried `core_plus1` since W2 but
// had NO consumer before this — a declared `core` was silently dropped.
#[cfg(target_os = "nuttx")]
unsafe extern "C" {
    fn nros_nuttx_apply_current_affinity(name: *const core::ffi::c_char, core_plus1: u32) -> i32;
}

// phase-302 W3 (issue 0263) — the C shim that adopts the tier's declared
// SCHED_FIFO priority on the calling thread (nuttx_run_tiers.c, shared with
// the C arm's create-time path). std's Builder has no priority attr, so the
// Rust arm self-applies at tier entry — without this a non-sporadic tier ran
// at the parent's priority (invisible until contention).
unsafe extern "C" {
    fn nros_nuttx_apply_current_priority(name: *const core::ffi::c_char, priority: u32) -> i32;
}

/// Adopt the tier's declared priority on the current thread (no-op
/// off-target and when the tier declares none). Name crosses the FFI as a
/// NUL-terminated stack copy, mirroring [`apply_tier_affinity`].
#[cfg(target_os = "nuttx")]
fn apply_tier_priority(tier: &nros_platform::TierSpec<'_>) {
    if tier.priority <= 0 {
        return;
    }
    let mut name_buf = [0u8; 64];
    let n = tier.name.len().min(63);
    name_buf[..n].copy_from_slice(&tier.name.as_bytes()[..n]);
    unsafe {
        nros_nuttx_apply_current_priority(
            name_buf.as_ptr() as *const core::ffi::c_char,
            tier.priority as u32,
        );
    }
}

#[cfg(not(target_os = "nuttx"))]
#[inline]
fn apply_tier_priority(_tier: &nros_platform::TierSpec<'_>) {}

/// Default per-tier pthread stack for spawned Rust tiers (issue #246). Mirrors
/// the C glue's `NROS_NUTTX_TIER_STACK_BYTES` intent but sized at NuttX's own
/// `CONFIG_PTHREAD_STACK_DEFAULT` (64 KiB): the executor arena lives on the
/// heap (`nros_platform_alloc`), so this only carries the zenoh-pico/executor
/// call frames. `TierSpec::stack_bytes` (when non-zero) overrides it.
#[cfg(any(feature = "reference-qemu", target_os = "nuttx"))]
const NUTTX_TIER_STACK_DEFAULT_BYTES: usize = 65536;

/// Self-apply the tier's kernel sporadic policy (no-op off-target and when
/// the tier declares no budget/period). Name/class cross the FFI as
/// NUL-terminated stack copies (TierSpec strings are `&str`, not C strings).
#[cfg(target_os = "nuttx")]
fn apply_tier_sporadic(tier: &nros_platform::TierSpec<'_>) {
    let (Some(class), Some(budget), Some(period)) = (tier.class, tier.budget_us, tier.period_us)
    else {
        return;
    };
    let mut name_buf = [0u8; 64];
    let n = tier.name.len().min(63);
    name_buf[..n].copy_from_slice(&tier.name.as_bytes()[..n]);
    let mut class_buf = [0u8; 32];
    let c = class.len().min(31);
    class_buf[..c].copy_from_slice(&class.as_bytes()[..c]);
    unsafe {
        nros_nuttx_apply_current_sporadic(
            name_buf.as_ptr() as *const core::ffi::c_char,
            class_buf.as_ptr() as *const core::ffi::c_char,
            budget,
            period,
            tier.priority,
        );
    }
}

#[cfg(not(target_os = "nuttx"))]
#[inline]
fn apply_tier_sporadic(_tier: &nros_platform::TierSpec<'_>) {}

/// phase-296 W5.11 — self-apply the tier's SMP core pin (no-op off-target and
/// when the tier declares no `core`). The `core + 1` encoding (0 = unpinned)
/// matches the C emit. Name crosses the FFI as a NUL-terminated stack copy.
/// Safe on the session-owning boot tier too: a core pin does not budget-cap the
/// thread, so (unlike the sporadic server, #246) it never starves the shared
/// session flush.
#[cfg(target_os = "nuttx")]
fn apply_tier_affinity(tier: &nros_platform::TierSpec<'_>) {
    let Some(core) = tier.core else {
        return;
    };
    let mut name_buf = [0u8; 64];
    let n = tier.name.len().min(63);
    name_buf[..n].copy_from_slice(&tier.name.as_bytes()[..n]);
    unsafe {
        nros_nuttx_apply_current_affinity(
            name_buf.as_ptr() as *const core::ffi::c_char,
            core.saturating_add(1),
        );
    }
}

#[cfg(not(target_os = "nuttx"))]
#[inline]
fn apply_tier_affinity(_tier: &nros_platform::TierSpec<'_>) {}

// issue 0579 / phase-358 W4 — this doc block and the `#[cfg]` below it were
// STRANDED ~150 lines above, before the `apply_tier_*` extern blocks that got
// inserted between them and this fn. Attributes bind to the NEXT item, so the
// cfg was guarding an `unsafe extern "C"` block and `run_tiers` — which uses
// `println!` and threads — was compiled UNCONDITIONALLY. It has not bitten
// because this crate is workspace-excluded and only ever built for NuttX. The
// rustdoc `unused doc comment` error on those extern blocks was the symptom.
/// phase-281 W3-nuttx (RFC-0015 Model 1) — per-tier multi-task NuttX entry.
///
/// The multi-tier sibling of [`run_entry`]: opens the ONE RMW session, then
/// runs one [`nros::Executor`] per [`nros_platform::TierSpec`] over that shared
/// session. NuttX ships `std` and its zenoh-pico build sets
/// `Z_FEATURE_MULTI_THREAD = 1` (`platforms/nuttx/nros-platform.toml`
/// `[platform.nuttx]`), so `std::thread` maps onto NuttX pthreads and this
/// mirrors the **native posix** [`nros_board_linux`] `run_tiers` (a scoped
/// thread per tier over one session) rather than the FFI k_thread shim the
/// Zephyr / bare-metal boards need.
///
/// ## Ordering (issue #144 — the interest-handshake race)
///
/// zenoh-pico entity declares carry an interest handshake; two threads that
/// declare concurrently race it, and the losing publisher's write filter can
/// stay closed (every put silently dropped). To avoid it we run the **boot
/// tier's `setup` FIRST on the boot task** (its declares finish before any
/// other tier starts), THEN spawn the remaining tiers. A spawned tier's `setup`
/// overlaps only the boot tier's *spin* (keepalives / data, not declares) — the
/// two-tier demo is therefore race-free. (For the single-tier deploy the
/// byte-identical [`run_entry`] path is used instead.)
///
/// `setup` is `Fn` (invoked once per tier) + `Sync` (spawned tiers share
/// `&setup`); it must register entities only — this fn owns each tier's
/// `active_groups` filter + the spin loop. Blocks forever (the boot tier's spin
/// never returns); returns only if the boot tier's `setup` fails before spin.
#[cfg(any(feature = "reference-qemu", target_os = "nuttx"))]
pub fn run_tiers<B, F, E>(
    boot_config: Option<&'static nros_platform::BakedBootConfig>,
    tiers: &[nros_platform::TierSpec<'_>],
    setup: F,
) -> Result<(), E>
where
    B: nros_platform::BoardInit,
    F: Fn(&mut nros_platform::RuntimeCtx<'_>) -> Result<(), E> + Sync,
    E: core::fmt::Debug,
{
    use std::io::Write as _;

    <B as nros_platform::BoardInit>::init_hardware();

    // NuttX virtio-net warm-up — same magic number + rationale as `run_entry`.
    std::thread::sleep(std::time::Duration::from_secs(5));
    let _ = std::io::stdout().flush();

    if tiers.is_empty() {
        println!("nros: run_tiers called with no tiers — nothing to run");
        std::process::exit(1);
    }

    // Baked locator / domain / node name — identical to `run_entry` (compile-time
    // on embedded; the QEMU guest has no populated env, so `from_env` would fall
    // back to loopback and never leave the guest). See `run_entry` for detail.
    const BAKED_LOCATOR: Option<&str> = option_env!("NROS_LOCATOR");
    const BAKED_DOMAIN: Option<&str> = option_env!("NROS_DOMAIN_ID");
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

    // NuttX has no linkme / `.init_array` auto-register, so the backend register
    // is explicit (mirrors `run_entry`).
    #[cfg(feature = "rmw-zenoh")]
    if let Err(err) = ::nros_rmw_zenoh::register() {
        println!("nros: zenoh RMW backend register failed: {:?}", err);
    }

    // The boot task opens the one session and owns it for the program's life
    // (the boot tier's spin loop never returns).
    let boot_exec = match ::nros::Executor::open(&exec_cfg) {
        Ok(e) => e,
        Err(err) => {
            println!(
                "nros: Executor::open failed ({:?}); multi-tier entry needs a live session \
                 — aborting.",
                err
            );
            let _ = std::io::stdout().flush();
            std::process::exit(1);
        }
    };
    install_stdout_logger();
    // Boot-readiness marker (same contract as `run_entry`) + a multi-tier marker
    // an E2E can gate on ("this image entered the per-tier run with a live
    // session"); the single-tier `run_entry` never prints the latter.
    println!("nros entry ready");
    println!(
        "nros: multi-tier run — {} tier(s) over one session",
        tiers.len()
    );
    let _ = std::io::stdout().flush();

    let mut boot_crt = ::nros::node_runtime::ExecutorNodeRuntime::from_executor(boot_exec);

    // issue #144 — boot-tier declares FIRST, before spawning any other tier.
    let boot_tier = &tiers[0];
    // issue 0572 — say WHICH tier is the session-owning boot tier, and with what.
    // The boot tier is the one that prints no priority marker (it keeps the
    // inherited FIFO priority by design), so the console showed the spawned
    // tiers only and the reader could not tell whether tiers[0] was the tier
    // they meant, nor whether its knobs survived the bake. On STDOUT with the
    // rest: this guest's stderr does not reach the serial console.
    println!(
        "nros: boot tier `{}` (session owner) — groups {:?}, class {:?}, \
         budget {:?} us, period {:?} us, spin {} us, priority {}",
        boot_tier.name,
        boot_tier.groups,
        boot_tier.class,
        boot_tier.budget_us,
        boot_tier.period_us,
        boot_tier.spin_period_us,
        // issue 0579 — the EFFECTIVE priority, so "accepted and dropped" is
        // visible from the console instead of needing a crash dump's tier
        // table to notice. `0` means the tier declared none and the thread
        // keeps whatever the init task was started with.
        boot_tier.priority
    );
    let _ = std::io::stdout().flush();
    boot_crt.executor_mut().set_active_groups(boot_tier.groups);
    // W5.4 — shared tier→SchedContext lowering (Sporadic / EDF / TT). BUT the
    // boot tier is the SESSION OWNER: `apply_tier_sched_policy` installs the
    // lowered context as the executor's *default* SchedContext, which gates
    // EVERY dispatch on this executor — including the spin loop that flushes
    // the one shared zenoh-pico session for all tiers. A budget/sporadic policy
    // there caps the session flush and starves delivery (issue #246: the
    // high/ctrl publisher delivered exactly ONE sample). Unlike the Rust
    // default-SC model, the C++ path binds the lowered context per HANDLE, so
    // its boot-tier flush stays Fifo — which is exactly why the C/C++ siblings
    // pass with the same model. Mirror that: the session-owning boot tier keeps
    // the default Fifo SchedContext (drop its budget/period), so only the EDF
    // deadline dim — which does not gate throughput — can still lower here.
    let boot_is_budgeted = boot_tier.class == Some("real_time")
        && boot_tier.budget_us.is_some()
        && boot_tier.period_us.is_some();
    boot_crt.apply_tier_sched_policy(
        boot_tier.class,
        if boot_is_budgeted {
            None
        } else {
            boot_tier.period_us
        },
        if boot_is_budgeted {
            None
        } else {
            boot_tier.budget_us
        },
        boot_tier.deadline_us,
        boot_tier.deadline_policy,
    );
    // phase-296 W5.9 / issue #246 — likewise DON'T self-apply the kernel
    // SCHED_SPORADIC server to the boot tier here (nor below): it would drop
    // this session-owning thread to `sched_ss_low_priority` when its budget is
    // spent, stalling the shared flush. Spawn every tier at the boot tier's
    // normal FIFO priority; spawned NON-owner tiers self-apply the budget dim's
    // kernel + cooperative realization in `nuttx_run_one_tier`.
    {
        let mut ctx = nros_platform::RuntimeCtx::with_runtime(&mut boot_crt);
        if let Err(e) = setup(&mut ctx) {
            // issue 0572 — STDOUT. This guest's stderr does not reach the serial
            // console, so every `println!` diagnostic in this function has been
            // invisible: a boot-tier setup failure, a failed tier spawn, a spin
            // error. Issue 0565 taught the harness to capture the console for
            // exactly these lines, and they could never appear in it.
            println!("nros: boot tier `{}` setup FAILED: {:?}", boot_tier.name, e);
            let _ = std::io::stdout().flush();
            return Err(e);
        }
    }

    let shared = NuttxSharedSession(boot_crt.executor_mut().session_ptr());
    let setup = &setup;
    std::thread::scope(|scope| {
        // Spawn every non-boot tier; each borrows the shared session pointer +
        // `&setup`. The boot declares are already done, so these only overlap the
        // boot tier's spin.
        for tier in &tiers[1..] {
            // issue 0572 — the spawned tiers' identity + groups, same reason.
            println!(
                "nros: spawning tier `{}` — groups {:?}, class {:?}, spin {} us",
                tier.name, tier.groups, tier.class, tier.spin_period_us
            );
            let _ = std::io::stdout().flush();
            // issue #246 — a Rust `std::thread` spawned with no explicit stack
            // requests the std default (2 MiB), which `pthread_create` cannot
            // satisfy from NuttX's small kernel heap → ENOMEM ("failed to spawn
            // tier"). The C/C++ sibling glue always passes an explicit
            // `pthread_attr_setstacksize` (16 KiB default / `stack_bytes`
            // override) precisely because the executor arena lives on the heap
            // (`nros_platform_alloc`), so a tier stack only carries call frames.
            // Mirror that: honour `stack_bytes`, else a 64 KiB default
            // (== NuttX's own `CONFIG_PTHREAD_STACK_DEFAULT`, generous for the
            // zenoh-pico/executor call depth the Rust closures reach).
            let stack_bytes = if tier.stack_bytes > 0 {
                tier.stack_bytes
            } else {
                NUTTX_TIER_STACK_DEFAULT_BYTES
            };
            // issue #246 — NuttX `pthread_create` from Rust std can fail
            // TRANSIENTLY under host/QEMU load (observed: an `io::Error` with no
            // OS errno, distinct from the deterministic 2-MiB-stack ENOMEM the
            // explicit `stack_size` above fixes). A single failure drops the
            // whole tier for the run → the low tier never delivers → the cell
            // times out (the historical #246 flake). Retry a few times with a
            // yield between attempts; the closure captures only Copy/ref state
            // (`shared`, `tier`, `setup`), so it is cheap to rebuild per attempt.
            const SPAWN_ATTEMPTS: u32 = 5;
            let mut spawned = false;
            for attempt in 1..=SPAWN_ATTEMPTS {
                let builder = std::thread::Builder::new()
                    .name(format!("nros-tier-{}", tier.name))
                    .stack_size(stack_bytes);
                match builder.spawn_scoped(scope, move || {
                    // Re-bind the whole wrapper so the closure captures the
                    // `Send` `NuttxSharedSession`, not the bare `*mut` field.
                    let shared = shared;
                    // SAFETY: `shared.0` aliases the boot executor's session,
                    // kept alive for this scope by `thread::scope`.
                    let exec = unsafe { ::nros::Executor::open_with_session(shared.0) };
                    nuttx_run_one_tier::<F, E>(exec, tier, setup);
                }) {
                    Ok(_) => {
                        spawned = true;
                        break;
                    }
                    Err(e) => {
                        println!(
                            "nros: spawn tier `{}` attempt {}/{} failed (stack {} B, \
                             kind {:?}, os error {:?}): {e}",
                            tier.name,
                            attempt,
                            SPAWN_ATTEMPTS,
                            stack_bytes,
                            e.kind(),
                            e.raw_os_error()
                        );
                        let _ = std::io::stdout().flush();
                        std::thread::yield_now();
                    }
                }
            }
            if !spawned {
                println!(
                    "nros: FAILED to spawn tier `{}` after {} attempts — tier will not run",
                    tier.name, SPAWN_ATTEMPTS
                );
                let _ = std::io::stdout().flush();
            }
        }
        // phase-296 W5.9 / issue #246 — the boot tier is the SESSION OWNER:
        // its spin drives the one shared zenoh-pico session's TX flush for
        // EVERY tier (all spawned tiers borrow this session). A kernel
        // SCHED_SPORADIC server would drop this thread to `sched_ss_low_priority`
        // (== `SCHED_FIFO` min, prio 1) the moment its budget is spent, stalling
        // the whole session's flush and starving delivery on all tiers (observed:
        // the high/ctrl publisher delivered exactly ONE sample, then the flush
        // stalled). So the session-owning boot tier stays SCHED_FIFO — it is
        // NEVER budget-capped. The budget dim's kernel-Native realization applies
        // to NON-owner tiers, which self-apply it in `nuttx_run_one_tier`.
        if boot_is_budgeted {
            println!(
                "nros: tier `{}` declares a sporadic budget but is the session-owning \
                 boot tier — kept SCHED_FIFO (a kernel/cooperative budget cap would \
                 stall the shared session flush; non-owner tiers realize the budget)",
                boot_tier.name
            );
            let _ = std::io::stdout().flush();
        }
        // phase-296 W5.11 — placement dim: the boot tier self-pins to its
        // declared `core` (safe here — a core pin, unlike the sporadic budget
        // above, does not cap CPU so it cannot starve the shared flush).
        apply_tier_affinity(boot_tier);
        // issue 0579 — and its declared PRIORITY, for exactly the reason the
        // affinity call above gives. `apply_tier_priority` was called from
        // `nuttx_run_one_tier` only, so tiers[0] parsed its
        // `[tiers.<name>.nuttx] priority`, baked it into the TierSpec, carried
        // it to the board and dropped it.
        //
        // Dropping one number out of an ORDERING does not make that tier
        // "default" — it silently reorders the set: a spawned tier declaring
        // 105 outranks a boot tier that declared 110, the inverse of what the
        // author wrote, with no diagnostic. It lands on the worst tier to get
        // wrong, since the boot tier is the session owner whose spin drives the
        // shared zenoh-pico flush.
        //
        // This is NOT issue 0246. That rule keeps the kernel SPORADIC SERVER
        // off the session owner because a spent budget drops it to
        // `sched_ss_low_priority` and stalls the shared flush — a mechanism
        // that CAPS CPU. A plain `pthread_setschedparam` priority caps nothing,
        // which is the same distinction the affinity comment draws, and
        // `boot_is_budgeted` above keeps the budget off the owner independently.
        //
        // ThreadX takes this answer too (`nros_threadx_set_current_priority`,
        // whose comment names the same inversion); Zephyr takes the other one,
        // sorting so tiers[0] is the numerically-largest = lowest-priority tier
        // and never needs to outrank anything (issue 0251). Two answers exist;
        // this board now has one of them rather than neither.
        apply_tier_priority(boot_tier);
        nuttx_spin_tier_forever(&mut boot_crt, boot_tier);
    });

    // Unreachable: the boot tier's spin loop never returns.
    Ok(())
}

/// `Send` wrapper for the shared raw session pointer so it can cross the
/// `std::thread::scope` boundary (the mirror of `nros-board-linux`'s
/// `SharedSession`). The pointed-to RMW session type is `pub(crate)` in
/// `nros-node`, so the wrapper is generic over `T` and never names it — `T` is
/// inferred from [`nros::Executor::session_ptr`]. Sharing the pointer is sound
/// under the per-tier contract: the boot executor owns the one session, the RMW
/// backend serializes concurrent access through its own locks (zenoh-pico
/// `Z_FEATURE_MULTI_THREAD = 1` on NuttX), and `thread::scope` guarantees no
/// spawned tier outlives the owner.
#[cfg(any(feature = "reference-qemu", target_os = "nuttx"))]
struct NuttxSharedSession<T>(*mut T);
#[cfg(any(feature = "reference-qemu", target_os = "nuttx"))]
impl<T> Clone for NuttxSharedSession<T> {
    fn clone(&self) -> Self {
        *self
    }
}
#[cfg(any(feature = "reference-qemu", target_os = "nuttx"))]
impl<T> Copy for NuttxSharedSession<T> {}
// SAFETY: the per-tier model shares one RMW session across tier tasks by design;
// concurrent access is serialized inside the backend.
#[cfg(any(feature = "reference-qemu", target_os = "nuttx"))]
unsafe impl<T> Send for NuttxSharedSession<T> {}

/// Register + spin one tier on a freshly-opened borrowed-session executor
/// (spawned-tier path).
#[cfg(any(feature = "reference-qemu", target_os = "nuttx"))]
fn nuttx_run_one_tier<F, E>(
    exec: ::nros::Executor<'static>,
    tier: &nros_platform::TierSpec<'_>,
    setup: &F,
) where
    F: Fn(&mut nros_platform::RuntimeCtx<'_>) -> Result<(), E>,
    E: core::fmt::Debug,
{
    use std::io::Write as _;

    let mut crt = ::nros::node_runtime::ExecutorNodeRuntime::from_executor(exec);
    crt.executor_mut().set_active_groups(tier.groups);
    // W5.4 — shared tier→SchedContext lowering (Sporadic / EDF / TT).
    crt.apply_tier_sched_policy(
        tier.class,
        tier.period_us,
        tier.budget_us,
        tier.deadline_us,
        tier.deadline_policy,
    );
    // phase-302 W3 (issue 0263) — adopt the declared SCHED_FIFO priority
    // (std spawn carries no priority attr; the C arm sets it at create).
    apply_tier_priority(tier);
    // phase-296 W5.9 — kernel sporadic server for this tier thread, when declared.
    apply_tier_sporadic(tier);
    // phase-296 W5.11 — placement dim: SMP core pin for this tier, when declared.
    apply_tier_affinity(tier);
    {
        let mut ctx = nros_platform::RuntimeCtx::with_runtime(&mut crt);
        if let Err(e) = setup(&mut ctx) {
            // issue 0572 — STDOUT; this guest's stderr never reaches the console.
            println!(
                "nros: tier `{}` setup FAILED: {:?} — tier task exiting",
                tier.name, e
            );
            let _ = std::io::stdout().flush();
            return;
        }
    }
    nuttx_spin_tier_forever(&mut crt, tier);
}

/// Drive a tier executor's `spin_once` at its declared period, forever.
#[cfg(any(feature = "reference-qemu", target_os = "nuttx"))]
fn nuttx_spin_tier_forever(
    crt: &mut ::nros::node_runtime::ExecutorNodeRuntime,
    tier: &nros_platform::TierSpec<'_>,
) {
    use std::io::Write as _;

    // issue 0572 — which wait the executor's spin will take. The primary
    // (session-owning) executor sleeps in the wake primitive when the backend
    // installed a wake callback; a borrowed one polls. `storage_size() == 0`
    // means no platform wake primitive is linked and the executor falls back to
    // a std `Condvar` — which on NuttX is the documented hang (this port's spin
    // is supposed to use `sem_timedwait`).
    unsafe extern "C" {
        fn nros_platform_wake_storage_size() -> usize;
    }
    // SAFETY: documented pure probe, callable before init.
    let wake_bytes = unsafe { nros_platform_wake_storage_size() };
    println!(
        "nros: tier `{}` entering spin — wake primitive {} ({} byte(s))",
        tier.name,
        if wake_bytes == 0 {
            "ABSENT (std Condvar fallback)"
        } else {
            "available"
        },
        wake_bytes
    );
    let _ = std::io::stdout().flush();

    let period_ms = ((tier.spin_period_us / 1000).max(1)) as u32;
    // issue 0572 — a per-tier heartbeat carrying the counts `spin_once` used to
    // discard. A tier that dispatches NOTHING and a tier that is not running at
    // all look identical from outside the guest (a silent topic), and that
    // ambiguity is what left this cell undiagnosed. Once per ~5 s of spins, so
    // it costs one line per tier per five seconds on the serial console.
    // ~1 s, not 5: the e2e kills the guest a couple of seconds after the slow
    // tier's anchor, so a 5 s heartbeat never printed once.
    let heartbeat_every = (1_000_000 / tier.spin_period_us.max(1)).max(1);
    let mut iters: u64 = 0;
    let (mut timers, mut subs, mut errs) = (0usize, 0usize, 0usize);
    let mut announced_first = false;
    loop {
        match crt.spin_once_counted(std::time::Duration::from_millis(period_ms as u64)) {
            Ok(r) => {
                timers += r.timers_fired;
                subs += r.subscriptions_processed;
                errs += r.subscription_errors + r.service_errors;
            }
            Err(err) => {
                // STDOUT: this guest's stderr never reaches the serial console.
                println!("nros: tier `{}` spin error: {:?}", tier.name, err);
                let _ = std::io::stdout().flush();
            }
        }
        iters += 1;
        if iters == 1 {
            // The loop is ALIVE. Distinguishes "spinning but never dispatching"
            // from "never reached the spin at all" — two very different bugs
            // that both present as a silent topic.
            println!("nros: tier `{}` completed spin 1", tier.name);
            let _ = std::io::stdout().flush();
        }
        // One-shot, and the datum that does not depend on how long the guest
        // lives: did this tier EVER dispatch anything?
        if !announced_first && (timers > 0 || subs > 0) {
            announced_first = true;
            println!(
                "nros: tier `{}` FIRST dispatch at spin {} — {} timer(s), {} sub callback(s)",
                tier.name, iters, timers, subs
            );
            let _ = std::io::stdout().flush();
        }
        if iters % heartbeat_every == 0 {
            println!(
                "nros: tier `{}` alive — {} spin(s), {} timer(s) fired, {} sub callback(s), {} error(s)",
                tier.name, iters, timers, subs, errs
            );
            let _ = std::io::stdout().flush();
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
#[cfg(any(feature = "reference-qemu", target_os = "nuttx"))]
fn install_stdout_logger() {
    use std::{io::Write as _, sync::Once};

    struct StdoutLogger;
    impl log::Log for StdoutLogger {
        fn enabled(&self, _: &log::Metadata<'_>) -> bool {
            true
        }
        fn log(&self, record: &log::Record<'_>) {
            // The examples bake the full human line into the message
            // (`Publishing: '...'` / `I heard: [...]`), so emit it verbatim.
            let mut out = std::io::stdout();
            // `[LEVEL]` prefix — parity with `nros_log`'s sink; see
            // nros-board-linux for why the tag is load-bearing.
            let _ = writeln!(out, "[{}] {}", record.level(), record.args());
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

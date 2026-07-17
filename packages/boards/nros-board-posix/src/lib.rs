//! POSIX family driver — Phase 212.N.2.
//!
//! Implements the `Board` trait family from `nros_platform` (the
//! traits live in `packages/core/nros-platform/src/board/` and are
//! re-exported at the `nros_platform` crate root) for the hosted
//! POSIX target (Linux, macOS, BSD). This is the simplest of the
//! family driver crates:
//!
//! - `init_hardware` is a no-op — libstd's runtime already brought up
//!   the heap, stdio, signal handlers and threading by the time
//!   `fn main` reaches us.
//! - `println` writes to `STDOUT_FILENO` via libstd's `Stdout` (which
//!   ultimately calls `write(2)` — matches the contract documented in
//!   `nros_platform::BoardPrint`).
//! - `exit_success` / `exit_failure` call `std::process::exit`.
//! - There is no `TransportBringup` / `NetworkWait` impl — POSIX
//!   sockets are open as soon as `init_hardware` returns. The trait
//!   surface treats both as optional mixins; their absence is the
//!   whole point.
//!
//! ## `BoardEntry::run` body
//!
//! The body sequences the lifecycle the trait surface documents:
//!
//! ```text
//! init_hardware()            // no-op
//! ↓
//! build RuntimeCtx           // empty by default; Phase 212.N.4
//!                            // codegen will populate from env / CLI
//! ↓
//! setup(&mut ctx)            // user closure — typically
//!                            // codegen-emitted `run_plan(runtime)`
//!                            // which owns nros::init + Executor::open
//!                            // + Executor::spin internally
//! ↓
//! exit_success() or          // -> !
//! exit_failure()
//! ```
//!
//! Note that the **executor lifecycle is deliberately owned by the
//! `setup` callback** rather than by `run` itself. Every existing
//! POSIX Entry pkg `main.rs` (see `examples/native/rust/talker`) opens
//! its own `Executor`, registers timers / nodes, and calls
//! `spin_blocking` from inside what becomes the `setup` closure once
//! Phase 212.N.4 codegen lands. `run` would have nothing portable to
//! say about which `Executor` instance to spin or how, so it stays out
//! of that decision. The seam is documented under "Open seams" below.

#![forbid(unsafe_op_in_unsafe_fn)]

use std::io::Write as _;

// `nros_platform::board` is `mod board;` (private); the Board trait
// family is re-exported at the crate root.
use nros_platform::{BoardEntry, BoardExit, BoardInit, BoardPrint, RuntimeCtx, TierSpec};

/// `Send` wrapper for the shared raw session pointer so it can cross the
/// `std::thread::scope` boundary. The pointed-to RMW session type is
/// `pub(crate)` in `nros-node` (unnameable here), so the wrapper is
/// generic over `T` and never names it — `T` is inferred from
/// [`nros::Executor::session_ptr`]. Sharing the pointer is sound under
/// the per-tier contract: the boot executor owns the one session, the
/// RMW backend serializes concurrent access through its own locks, and
/// `thread::scope` guarantees no spawned tier outlives the owner.
struct SharedSession<T>(*mut T);
// Hand-written Copy/Clone: `#[derive]` would add a spurious `T: Copy`
// bound (the session type isn't `Copy`), but a raw pointer always is.
impl<T> Clone for SharedSession<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for SharedSession<T> {}
// SAFETY: the per-tier model shares one RMW session across tier tasks by
// design; concurrent access is serialized inside the backend.
unsafe impl<T> Send for SharedSession<T> {}

/// POSIX family driver ZST. Plug into an Entry pkg `main.rs` via:
///
/// ```ignore
/// use nros_board_posix::PosixBoard;
/// use nros_platform::board::BoardEntry;
///
/// fn main() {
///     let _ = <PosixBoard as BoardEntry>::run(|runtime| {
///         // codegen-emitted (Phase 212.N.4)
///         run_plan(runtime)
///     });
/// }
/// ```
pub struct PosixBoard;

impl BoardInit for PosixBoard {
    /// POSIX needs no hardware init: libstd's runtime already
    /// initialized the heap, stdio, signal handlers and threading
    /// before `fn main` ran. Kept as a documented no-op so the
    /// lifecycle in [`BoardEntry::run`] is uniform across families.
    #[inline]
    fn init_hardware() {}
}

impl BoardPrint for PosixBoard {
    fn println(args: core::fmt::Arguments<'_>) {
        // Write to a stdout lock so concurrent threads don't
        // interleave a single line. `libc::write(STDOUT_FILENO, …)`
        // would also satisfy the trait, but libstd's `Stdout` already
        // bottoms out in `write(2)` and adds line-buffered locking
        // that we'd otherwise have to rebuild. If the write fails
        // (closed stdout, broken pipe) we deliberately swallow the
        // error — a board-print failure shouldn't tear down the boot.
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{}", args);
    }
}

impl BoardExit for PosixBoard {
    fn exit_success() -> ! {
        std::process::exit(0)
    }

    fn exit_failure() -> ! {
        std::process::exit(1)
    }
}

impl BoardEntry for PosixBoard {
    /// Drive the boot → setup → exit flow. POSIX has no transport
    /// bringup or network-wait step:
    ///
    /// 1. [`BoardInit::init_hardware`] (no-op).
    /// 2. Open the live [`nros::Executor`] from the env-derived
    ///    [`nros::ExecutorConfig`] (`ROS_DOMAIN_ID`, `NROS_LOCATOR`,
    ///    `NROS_SESSION_MODE`) and wrap it in an
    ///    [`nros::node_runtime::ExecutorNodeRuntime`] —
    ///    Phase 212.N.7 step-3.5. The codegen-emitted
    ///    `run_plan(runtime)` body now talks to a real executor.
    /// 3. Build a [`RuntimeCtx`] backed by that runtime.
    /// 4. Invoke `setup(&mut ctx)`.
    /// 5. Log the result via [`BoardPrint::println`].
    /// 6. Diverge into [`BoardExit::exit_success`] or
    ///    [`BoardExit::exit_failure`].
    ///
    /// Native (POSIX) does **not** enter an infinite spin loop after
    /// `setup` returns — POSIX-shaped applications drive their own
    /// spinning inside `setup` (e.g. a codegen `run_plan` that calls
    /// `Executor::spin_blocking`, or an Entry pkg main that simply
    /// exits when the closure finishes). The contract mirrors the
    /// hosted nuttx carve-out and matches the existing
    /// `nros-board-posix` doc comment ("the executor open + spin
    /// happens *inside* `setup`"). The change here is that the open
    /// step is now done **for** the closure rather than by it — the
    /// `setup` body receives a live runtime sink through
    /// `RuntimeCtx::runtime` and dispatches Node pkg `register`
    /// calls into it.
    fn run<F, E>(setup: F) -> Result<(), E>
    where
        F: FnOnce(&mut RuntimeCtx<'_>) -> Result<(), E>,
        E: core::fmt::Debug,
    {
        Self::run_with_deploy(&nros_platform::DeployOverlay::default(), setup)
    }

    /// Issue #98 — name the primary session (the ROS graph node name) from the
    /// launch file's single `<node>` when `deploy.node_name` is set. Locator/IP
    /// stay env-driven (issue #48); this override exists only to thread the node
    /// name onto the boot `ExecutorConfig`. `run` forwards here with a default
    /// (empty) overlay, so this is the single hosted boot body.
    fn run_with_deploy<F, E>(deploy: &nros_platform::DeployOverlay, setup: F) -> Result<(), E>
    where
        F: FnOnce(&mut RuntimeCtx<'_>) -> Result<(), E>,
        E: core::fmt::Debug,
    {
        // Default sizing (the build-time `MAX_CBS`/`ARENA_SIZE`).
        Self::boot_hosted(deploy, None, setup)
    }

    /// phase-271 (issue #110) — hosted boot sized to the entry's own declared
    /// topology (`[package.metadata.nros.entry] max_callbacks` /
    /// `max_sched_contexts`), so a fat native entry (>default `MAX_CBS`
    /// callbacks) fits WITHOUT a workspace-global `NROS_EXECUTOR_MAX_CBS`.
    fn run_with_deploy_sized<F, E>(
        deploy: &nros_platform::DeployOverlay,
        max_cbs: usize,
        max_sched_contexts: usize,
        setup: F,
    ) -> Result<(), E>
    where
        F: FnOnce(&mut RuntimeCtx<'_>) -> Result<(), E>,
        E: core::fmt::Debug,
    {
        Self::boot_hosted(deploy, Some((max_cbs, max_sched_contexts)), setup)
    }
}

impl PosixBoard {
    /// phase-271 (issue #98 + #110) — the single hosted boot body shared by
    /// [`BoardEntry::run_with_deploy`] (default sizing, `sizing = None`) and
    /// [`BoardEntry::run_with_deploy_sized`] (`sizing = Some((max_cbs,
    /// max_sched_contexts))` — a `0` sched-context count means "use the build
    /// default"). The ONLY difference between the two paths is whether the
    /// executor opens via `Executor::open` or `Executor::open_sized`.
    fn boot_hosted<F, E>(
        deploy: &nros_platform::DeployOverlay,
        sizing: Option<(usize, usize)>,
        setup: F,
    ) -> Result<(), E>
    where
        F: FnOnce(&mut RuntimeCtx<'_>) -> Result<(), E>,
        E: core::fmt::Debug,
    {
        <Self as BoardInit>::init_hardware();
        // Phase 264 W3 — wire the default log sink (host → stdout/stderr) so a Node
        // pkg's `nros_info!` produces output without per-app `nros_log::init`.
        // Idempotent (swaps the sink list atomically).
        ::nros_log::init(::nros_log::sinks::default());

        // Phase 212.N.7 step-3.5 — open the executor + wrap it in an
        // `ExecutorNodeRuntime` so the codegen-emitted `run_plan(runtime)` body
        // can register components against a live RMW session. Env-derived config
        // picks up `ROS_DOMAIN_ID` / `NROS_LOCATOR` / `NROS_SESSION_MODE` at
        // runtime — the host-side carve-out from the embedded compile-time
        // domain-id contract documented in CLAUDE.md.
        //
        // If executor open fails (no RMW backend linked, or the configured
        // router/peer is unreachable), we fall back to
        // [`nros_platform::NullNodeRuntime`] so the setup closure still runs. The
        // fall-back errors loud on any `register_dispatch_slot_dyn` call.
        // Issue #98 — route the single-node launch overlay through the W1
        // resolver (RFC-0045 precedence model A): env > baked > compiled default.
        // Only `node_name` is mapped from the overlay; locator/domain/namespace
        // stay `None` so env keeps authority over them (issue #48 preserved).
        let exec_cfg = ::nros::ExecutorConfig::resolve(
            ::nros::BootConfig {
                node_name: deploy.node_name,
                ..Default::default()
            },
            /* hosted_env = */ true,
        );
        // phase-271 — open at the entry's declared sizing when supplied.
        let opened = match sizing {
            None => ::nros::Executor::open(&exec_cfg),
            Some((cbs, sc)) => {
                let sc = if sc == 0 {
                    ::nros::ExecutorSizing::DEFAULT.sc
                } else {
                    sc
                };
                ::nros::Executor::open_sized(
                    &exec_cfg,
                    ::nros::ExecutorSizing {
                        cbs,
                        sc,
                        arena: ::nros::arena_size_for(cbs),
                    },
                )
            }
        };
        let mut crt_real: Option<::nros::node_runtime::ExecutorNodeRuntime> = match opened {
            Ok(e) => Some(::nros::node_runtime::ExecutorNodeRuntime::from_executor(e)),
            Err(err) => {
                <Self as BoardPrint>::println(format_args!(
                    "nros: Executor::open failed ({err:?}); proceeding with NullNodeRuntime — \
                     `run_plan` register calls will fail loud."
                ));
                None
            }
        };
        let mut crt_null = ::nros_platform::NullNodeRuntime;
        let result = match crt_real.as_mut() {
            Some(crt) => {
                let mut runtime = RuntimeCtx::with_runtime(crt);
                setup(&mut runtime)
            }
            None => {
                let mut runtime = RuntimeCtx::with_runtime(&mut crt_null);
                setup(&mut runtime)
            }
        };
        match result {
            Ok(()) => {
                <Self as BoardPrint>::println(format_args!("nros: application complete"));
                <Self as BoardExit>::exit_success();
            }
            Err(e) => {
                <Self as BoardPrint>::println(format_args!("nros: application error: {e:?}"));
                <Self as BoardExit>::exit_failure();
            }
        }
    }

    /// Phase 228.E — per-tier multi-task entry. Opens the one RMW
    /// session, then runs one `Executor` per [`TierSpec`] over that
    /// shared session: the highest-priority tier (`tiers[0]`, the
    /// resolver orders highest-first) runs on the boot task; the rest
    /// are spawned as `std::thread`s. Each tier sets its
    /// `active_groups` filter, runs `setup` (register-only — only this
    /// tier's callbacks take), then spins forever.
    ///
    /// `setup` is `Fn` (not `FnOnce`) — it is invoked once per tier
    /// executor — and `Sync`, since spawned tiers share `&setup`. It
    /// must register entities only; the spin loop is owned here so the
    /// board can install the group filter first. (The single-tier
    /// [`BoardEntry::run`] path, where `setup` owns the spin, is
    /// unchanged.)
    ///
    /// Native preemption uses the default scheduler; the normalized
    /// [`TierSpec::priority`] is advisory here (strict ordering needs
    /// `SCHED_FIFO` + privileges). The FreeRTOS port maps it to real
    /// task priorities (RFC-0016). Blocks forever (server semantics);
    /// returns only if a tier `setup` fails before the spin loop.
    pub fn run_tiers<F, E>(
        _deploy: &nros_platform::DeployOverlay,
        tiers: &[TierSpec<'_>],
        setup: F,
    ) -> Result<(), E>
    where
        F: Fn(&mut RuntimeCtx<'_>) -> Result<(), E> + Sync,
        E: core::fmt::Debug,
    {
        // Issue #48 — hosted boards take their locator from `from_env()`, so the
        // deploy overlay is ignored here (kept for signature parity with the
        // firmware boards' `run_tiers`).
        <Self as BoardInit>::init_hardware();
        // Phase 264 W3 — default log sink at boot (see `run`).
        ::nros_log::init(::nros_log::sinks::default());

        if tiers.is_empty() {
            <Self as BoardPrint>::println(format_args!(
                "nros: run_tiers called with no tiers — nothing to run"
            ));
            <Self as BoardExit>::exit_failure();
        }

        // Open the one session on the boot task; it owns the session for
        // the program's life (the boot tier's spin loop never returns).
        let exec_cfg = ::nros::ExecutorConfig::from_env();
        let boot_exec = match ::nros::Executor::open(&exec_cfg) {
            Ok(e) => e,
            Err(err) => {
                <Self as BoardPrint>::println(format_args!(
                    "nros: Executor::open failed ({err:?}); multi-tier entry needs a live \
                     session — aborting."
                ));
                <Self as BoardExit>::exit_failure();
            }
        };
        let mut boot_crt = ::nros::node_runtime::ExecutorNodeRuntime::from_executor(boot_exec);
        let shared = SharedSession(boot_crt.executor_mut().session_ptr());

        let setup = &setup;
        std::thread::scope(|scope| {
            // Spawn every tier after the first; each borrows the shared
            // session pointer and `&setup` from the enclosing scope.
            for tier in &tiers[1..] {
                let builder = std::thread::Builder::new().name(format!("nros-tier-{}", tier.name));
                let spawn = builder.spawn_scoped(scope, move || {
                    // Re-bind the whole wrapper so the closure captures the
                    // `Send` `SharedSession`, not the bare `*mut` field
                    // (edition-2021 disjoint capture would grab the field).
                    let shared = shared;
                    // SAFETY: `shared.0` aliases the boot executor's
                    // session, kept alive for this scope by `thread::scope`.
                    let exec = unsafe { ::nros::Executor::open_with_session(shared.0) };
                    run_one_tier::<Self, F, E>(exec, tier, setup);
                });
                if let Err(e) = spawn {
                    <Self as BoardPrint>::println(format_args!(
                        "nros: failed to spawn tier `{}`: {e}",
                        tier.name
                    ));
                }
            }
            // Reached once the session is open + every non-boot tier task is
            // spawned; the boot tier then registers + spins below. Unique line
            // (the single-tier path never prints it) so an E2E can confirm the
            // emitted binary entered the per-tier run with a live session.
            <Self as BoardPrint>::println(format_args!(
                "nros: multi-tier run — {} tier(s) over one session",
                tiers.len()
            ));
            // Boot tier runs on this task, reusing the owning executor.
            run_boot_tier::<Self, F, E>(&mut boot_crt, &tiers[0], setup);
        });

        // Unreachable: the boot tier's spin loop never returns.
        Ok(())
    }
}

/// RFC-0052 / phase-296 W3a — lower a tier's RTOS-agnostic scheduling
/// policy onto the tier executor's DEFAULT scheduling context. One
/// Executor per tier means "the tier's policy" == "this executor's
/// default SC"; per-group/per-handle bindings still take precedence.
///
/// v1 lowers the sporadic-budget shape (`class = "real_time"` with
/// `budget_us` + `period_us` → `SchedClass::Sporadic`) and `best_effort`;
/// `time_triggered` window registration is W3b (needs the major-frame
/// dispatcher wired at this altitude too). A tier with no class/budget
/// leaves the default Fifo SC untouched — byte-identical pre-W3 behavior.
fn apply_tier_sched(crt: &mut ::nros::node_runtime::ExecutorNodeRuntime, tier: &TierSpec<'_>) {
    use ::nros::{OptUs, SchedClass, SchedContext};
    let sporadic =
        tier.class == Some("real_time") && tier.budget_us.is_some() && tier.period_us.is_some();
    let best_effort = tier.class == Some("best_effort");
    if !sporadic && !best_effort && tier.deadline_us.is_none() {
        return;
    }
    let mut sc = SchedContext::default();
    if sporadic {
        sc.class = SchedClass::Sporadic;
        sc.budget_us = OptUs::from_us(tier.budget_us.unwrap_or(0).min(u32::MAX as u64) as u32);
        sc.period_us = OptUs::from_us(tier.period_us.unwrap_or(0).min(u32::MAX as u64) as u32);
    } else if best_effort {
        sc.class = SchedClass::BestEffort;
    }
    if let Some(d) = tier.deadline_us {
        sc.deadline_us = OptUs::from_us(d.min(u32::MAX as u64) as u32);
    }
    crt.executor_mut().set_default_sched_context(sc);
}

/// Register + spin one tier on a freshly-opened borrowed-session
/// executor (spawned-tier path).
fn run_one_tier<B, F, E>(exec: ::nros::Executor<'static>, tier: &TierSpec<'_>, setup: &F)
where
    B: BoardPrint,
    F: Fn(&mut RuntimeCtx<'_>) -> Result<(), E>,
    E: core::fmt::Debug,
{
    let mut crt = ::nros::node_runtime::ExecutorNodeRuntime::from_executor(exec);
    crt.executor_mut().set_active_groups(tier.groups);
    apply_tier_sched(&mut crt, tier);
    {
        let mut ctx = RuntimeCtx::with_runtime(&mut crt);
        if let Err(e) = setup(&mut ctx) {
            B::println(format_args!(
                "nros: tier `{}` setup failed: {e:?} — tier task exiting",
                tier.name
            ));
            return;
        }
    }
    spin_forever::<B>(&mut crt, tier);
}

/// Register + spin the boot tier on the session-owning executor.
fn run_boot_tier<B, F, E>(
    crt: &mut ::nros::node_runtime::ExecutorNodeRuntime,
    tier: &TierSpec<'_>,
    setup: &F,
) where
    B: BoardPrint,
    F: Fn(&mut RuntimeCtx<'_>) -> Result<(), E>,
    E: core::fmt::Debug,
{
    crt.executor_mut().set_active_groups(tier.groups);
    apply_tier_sched(crt, tier);
    {
        let mut ctx = RuntimeCtx::with_runtime(crt);
        if let Err(e) = setup(&mut ctx) {
            B::println(format_args!(
                "nros: boot tier `{}` setup failed: {e:?}",
                tier.name
            ));
            return;
        }
    }
    spin_forever::<B>(crt, tier);
}

/// Drive a tier executor's `spin_once` at its declared period, forever.
fn spin_forever<B: BoardPrint>(
    crt: &mut ::nros::node_runtime::ExecutorNodeRuntime,
    tier: &TierSpec<'_>,
) {
    let period = std::time::Duration::from_micros(tier.spin_period_us.max(1));
    loop {
        if let Err(e) = crt.spin_once(period) {
            B::println(format_args!("nros: tier `{}` spin error: {e:?}", tier.name));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_hardware_is_noop() {
        // Smoke: calling `init_hardware` from a unit test must not
        // panic or affect global state.
        <PosixBoard as BoardInit>::init_hardware();
    }

    #[test]
    fn println_writes_without_panicking() {
        <PosixBoard as BoardPrint>::println(format_args!("nros-board-posix: hello from unit test"));
    }

    // Note: `BoardEntry::run` itself can't be unit-tested directly
    // because both exit branches diverge (`-> !`) via
    // `std::process::exit`, which would kill the test process. The
    // doc comment on `BoardEntry::run` explicitly preserves the
    // `-> Result` shape on the *callback* path so production boards
    // can still wrap the trait in a non-diverging test harness; that
    // harness lives outside this crate.
}

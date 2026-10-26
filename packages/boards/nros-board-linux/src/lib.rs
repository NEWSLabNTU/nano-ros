//! The Linux host board — Phase 212.N.2 family driver + phase-337 W8.a.
//!
//! **Naming (RFC-0064 R3).** The PLATFORM stays `posix`: `nros-platform-posix`
//! is a genuine portability seam written to POSIX, and the platform layer names
//! software-stack facts (RFC-0049's duty rule). The BOARD is `linux`, because
//! the board layer names what we actually claim to support and a tier-1 promise
//! means "`just ci` exercises it" — which only Linux does (every CI job is
//! `ubuntu-*`, and `nros-platform-posix/src/timer.c` calls `timer_create` with
//! no fallback). `macos` / `freebsd` can later join as their own boards on this
//! unchanged platform; that is the one-source-many-targets model applied to
//! hosted OSes, and its prerequisite is a `timer_create` fallback.
//!
//! phase-337 W8.a merged `nros-board-native` (a 226-line delegating shim) and
//! `nros-board-posix` (the family driver) into this one crate. `board_path_for`
//! already mapped BOTH deploy keys to the same ZST, so the shim was named by no
//! generated entry — ceremony, not a behaviour boundary.
//!
//! Implements the `Board` trait family from `nros_platform` (the
//! traits live in `packages/platform/nros-platform/src/board/` and are
//! re-exported at the `nros_platform` crate root) for the hosted
//! LINUX target. Not "POSIX (Linux, macOS, BSD)", which this said and
//! which the crate cannot honour: `apply_tier_affinity` below calls
//! `sched_setaffinity` with `cpu_set_t` / `CPU_SET`, none of which
//! libc defines for apple, and nothing here is `cfg(target_os)`-gated.
//! The PLATFORM beneath it (`nros-platform-posix`) is POSIX-clean —
//! see the Naming note above. This is the simplest of the
//! family driver crates:
//!
//! - `init_hardware` is a no-op — libstd's runtime already brought up
//!   the heap, stdio, signal handlers and threading by the time
//!   `fn main` reaches us.
//! - `println` writes to `STDOUT_FILENO` via libstd's `Stdout` (which
//!   ultimately calls `write(2)` — matches the contract documented in
//!   `nros_platform::BoardPrint`).
//! - `exit_success` / `exit_failure` call `std::process::exit`.
//! - There is no device bring-up step — POSIX sockets are open as
//!   soon as `init_hardware` returns. This used to be phrased as the
//!   absence of a `TransportBringup` / `NetworkWait` impl; phase-206
//!   W4 removed those traits (issue 1067), so the absence is now
//!   universal and this board's point is simply that it needs no
//!   bring-up at all.
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
//! POSIX Entry pkg `main.rs` (see `examples/linux/rust/talker`) opens
//! its own `Executor`, registers timers / nodes, and calls
//! `spin` from inside what becomes the `setup` closure once
//! Phase 212.N.4 codegen lands. `run` would have nothing portable to
//! say about which `Executor` instance to spin or how, so it stays out
//! of that decision. The seam is documented under "Open seams" below.

#![forbid(unsafe_op_in_unsafe_fn)]

// Phase 248 C5a (#60 T4) — the BOARD is the RMW selection point. Under its
// own `rmw-zenoh` feature it force-links the zenoh backend rlib so the
// backend's `RMW_INIT_ENTRIES` self-register section survives stable-Rust
// rlib pruning and reaches the final binary, WITHOUT relying on the `nros`
// umbrella's `rmw-zenoh` feature. Mirrors the `__FORCE_LINK_ZENOH` static in
// `nros/src/lib.rs` (referencing `register` keeps both the symbol and its
// linker section alive — strictly stronger than the prior `extern crate _`,
// which only kept the rlib). On a host (linkme-aware + `.init_array`) the
// section auto-registers; the static guarantees it is not pruned first.
// Cycle-free: the backend crate does not depend on this board crate. Inert
// unless `rmw-zenoh` selects the backend.
#[cfg(feature = "rmw-zenoh")]
#[doc(hidden)]
#[used]
pub static __FORCE_LINK_ZENOH: fn() -> Result<(), nros_rmw_zenoh::RegisterError> =
    nros_rmw_zenoh::register;

use std::io::Write as _;

// `nros_platform::board` is `mod board;` (private); the Board trait
// family is re-exported at the crate root.
use nros_platform::{BoardEntry, BoardExit, BoardInit, BoardPrint, RuntimeCtx, TierSpec};

/// POSIX family driver ZST. Plug into an Entry pkg `main.rs` via:
///
/// ```ignore
/// use nros_board_linux::LinuxBoard;
/// use nros_platform::board::BoardEntry;
///
/// fn main() {
///     let _ = <LinuxBoard as BoardEntry>::run(|runtime| {
///         // codegen-emitted (Phase 212.N.4)
///         run_plan(runtime)
///     });
/// }
/// ```
pub struct LinuxBoard;

impl BoardInit for LinuxBoard {
    /// POSIX needs no hardware init: libstd's runtime already
    /// initialized the heap, stdio, signal handlers and threading
    /// before `fn main` ran. Kept as a documented no-op so the
    /// lifecycle in [`BoardEntry::run`] is uniform across families.
    #[inline]
    fn init_hardware() {}
}

impl BoardPrint for LinuxBoard {
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

impl BoardExit for LinuxBoard {
    fn exit_success() -> ! {
        std::process::exit(0)
    }

    fn exit_failure() -> ! {
        std::process::exit(1)
    }
}

/// Phase 249 P3.5 — the board owns the RMW registration on EVERY OS (hosted
/// included), not just `target_os="none"`. Calling the linked backend's
/// `register()` explicitly before the executor opens is the one universal
/// trigger; it replaces the hosted reliance on the linkme `.init_array` walk
/// (retired in P4) and lets the weak `nros_app_register_backends` default die.
/// Gated on the board's own `rmw-<x>` feature; the call also force-links the
/// backend (strictly stronger than the `#[used] __FORCE_LINK_*` static). Inert
/// when no backend feature is selected.
///
/// Phase 277 W2.b — public so a plain-API example (no `BoardEntry` /
/// `nros::main!` framework, just `LinuxBoard` + `Executor::open` directly) can
/// trigger registration itself before opening the executor. Idempotent — safe
/// to call more than once; re-registering an already-registered backend is a
/// no-op. Apps that boot through `nros::main!()` or any `BoardEntry::run*` /
/// `run_tiers` entry point never need to call this themselves — every boot path
/// below funnels through `boot_hosted` / `run_tiers`, which call it.
#[inline]
pub fn register_linked_rmw() {
    #[cfg(feature = "rmw-zenoh")]
    {
        let _ = nros_rmw_zenoh::register();
    }
    #[cfg(feature = "rmw-xrce")]
    {
        let _ = nros_rmw_xrce_cffi::register();
    }
    #[cfg(feature = "rmw-cyclonedds")]
    {
        let _ = nros_rmw_cyclonedds_sys::register();
    }
}

/// phase-463 W2 -- `$NROS_CENSUS_OUT`, the census switch, as this funnel sees
/// it.
///
/// The switch belongs to the HOSTED funnels and to nothing else: they are the
/// two places in the tree that resolve through the `env` capability (issue
/// 0687), and no RTOS board has it, so census mode does not exist on the RTOS
/// road rather than being compiled out of it. `nros-cpp`'s
/// `nros_board_native_run_components_named` produces the C/C++ census; this
/// one, the Rust entry's funnel, produces the Rust one (issue 1419) through the
/// same hooks (`nros::census_hooks`) when built with the `census` feature, and
/// refuses by name when built without it.
///
/// Empty is unset -- the same reading the C++ funnel gives it.
fn census_requested() -> Option<std::ffi::OsString> {
    let raw = std::env::var_os("NROS_CENSUS_OUT")?;
    if raw.is_empty() { None } else { Some(raw) }
}

/// phase-463 W7 -- `$NROS_PROFILE_OUT`, the profile switch, as this funnel
/// sees it: the Rust entry's half of `nros-cpp`'s `profile_arm`. Called before
/// the executor opens, so every registration is recorded. Idempotent (a tiered
/// boot opens one executor per tier). An image built without the `profile`
/// feature REFUSES rather than booting with no file -- the census's rule.
fn profile_arm() {
    let Some(path) = std::env::var_os("NROS_PROFILE_OUT").filter(|p| !p.is_empty()) else {
        return;
    };
    #[cfg(feature = "profile")]
    {
        static PATH: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
        // Write-then-rename, so a reader (or a SIGTERM mid-write) never sees half.
        fn sink(doc: &str) {
            let Some(path) = PATH.get() else { return };
            let mut tmp = path.clone().into_os_string();
            tmp.push(".tmp");
            if std::fs::write(&tmp, doc).is_ok() {
                let _ = std::fs::rename(&tmp, path);
            }
        }
        // Unique per live thread: the address of a thread-local.
        fn thread_key() -> usize {
            std::thread_local! { static KEY: u8 = const { 0 }; }
            KEY.with(|k| k as *const u8 as usize)
        }
        if !::nros::profile::enabled() {
            let _ = PATH.set(std::path::PathBuf::from(&path));
            ::nros::profile::install(thread_key, sink);
            let launch = std::env::current_exe()
                .ok()
                .and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()))
                .unwrap_or_default();
            let inputs = std::env::var("NROS_PROFILE_INPUTS")
                .unwrap_or_else(|_| "live run; no replayed input set was named".into());
            ::nros::profile::enable(&launch, &inputs);
        }
    }
    #[cfg(not(feature = "profile"))]
    {
        std::eprintln!(
            "nros profile: $NROS_PROFILE_OUT=`{}` but this Rust entry was built without \
             `nros-board-linux/profile` -- the generated host entry enables it; a hand-written \
             one must name it",
            path.to_string_lossy()
        );
        <LinuxBoard as BoardExit>::exit_failure();
    }
}

/// Issue 1419 -- arm census mode, BEFORE the session opens: register the
/// recording backend and select it by name, the C++ funnel's
/// `census_select_backend` for the Rust entry. `$NROS_RMW` is FORCED rather
/// than required of the caller, so `$NROS_CENSUS_OUT` is a switch and not half
/// of one (a census against a transporting backend would need a router and
/// record nothing).
///
/// Returns the executor sizing a census opens at (`nros::census_hooks::
/// CENSUS_SIZING`, the executor's own ceilings -- never the contract's), or
/// `None` in an image built without the `census` feature, which then refuses
/// in [`census_finish`] rather than booting normally with no file.
#[cfg(feature = "census")]
fn census_arm() -> Option<::nros::ExecutorSizing> {
    let ret = nros_rmw_metadata::nros_rmw_metadata_register();
    if ret != 0 {
        std::eprintln!(
            "nros census: registering the recording backend failed (rc={ret}); the RMW \
             registry holds {} slot(s) -- is the recorder's slot (`recorder-slot`) missing?",
            nros_rmw_metadata::REGISTRY_SLOTS
        );
    }
    // SAFETY: the boot funnel, before the executor opens and before any tier
    // thread is spawned -- the process is single-threaded here.
    unsafe { std::env::set_var("NROS_RMW", nros::census_hooks::RECORDER_RMW) };
    Some(::nros::census_hooks::CENSUS_SIZING)
}

#[cfg(not(feature = "census"))]
fn census_arm() -> Option<::nros::ExecutorSizing> {
    None
}

/// Issue 1419 -- write the census where `$NROS_CENSUS_OUT` names, INSTEAD of
/// the spin, and exit. Written EVEN WHEN `setup` failed, with the failure still
/// the exit status: the CLI marks such a census incomplete and every check
/// against it refuses (the C++ funnel's rule), which beats discarding what the
/// recorder saw.
///
/// Serialization is `nros::metadata_mode::to_json` -- the ONE emitter the C++
/// funnel and the probe use, so a check reads one schema.
#[cfg(feature = "census")]
fn census_finish(path: &std::ffi::OsStr, session: &str, setup_ok: bool) -> ! {
    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_default();
    let export = ::nros::node_metadata::SourceMetadataExport::new(session, session)
        .executable(&exe)
        .language("rust");
    let written = if ::nros::metadata_mode::entity_count() == 0 {
        std::eprintln!(
            "nros census: the setup path created no entity at all -- refusing to write a census \
             a check would read as \"this image declares nothing\""
        );
        false
    } else {
        match ::nros::metadata_mode::to_json(&export) {
            Ok(json) => match std::fs::write(path, json) {
                Ok(()) => true,
                Err(e) => {
                    std::eprintln!(
                        "nros census: cannot write `{}`: {e}",
                        path.to_string_lossy()
                    );
                    false
                }
            },
            Err(_) => {
                std::eprintln!("nros census: serializing the recorder failed");
                false
            }
        }
    };
    if written && setup_ok {
        <LinuxBoard as BoardExit>::exit_success();
    }
    <LinuxBoard as BoardExit>::exit_failure();
}

/// The census switch in a Rust entry built WITHOUT the `census` feature: no
/// recorder, so no census. A refusal rather than a silent normal boot -- the
/// caller asked for a file by naming it.
#[cfg(not(feature = "census"))]
fn census_finish(path: &std::ffi::OsStr, _session: &str, _setup_ok: bool) -> ! {
    std::eprintln!(
        "nros census: $NROS_CENSUS_OUT=`{}` but this Rust entry was built without \
         `nros-board-linux/census` -- the generated host entry enables it; a hand-written one \
         must name it",
        path.to_string_lossy()
    );
    <LinuxBoard as BoardExit>::exit_failure();
}

impl BoardEntry for LinuxBoard {
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
    /// `Executor::spin`, or an Entry pkg main that simply
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

/// The BAKED rung this board hands `resolve_hosted` — the launch-declared
/// IDENTITY of the primary session, and nothing else.
///
/// Issue 1442. This board has TWO boot funnels (`boot_hosted` for the
/// single-executor path, `run_tiers` for the tiered one) and they built this
/// value separately: issue 1434 gave both the namespace, and `node_name`
/// reached only the first, so a tiered image's session was called `node`
/// however its launch named it. One function is the fix, because the drift was
/// never in the resolution — it was in there being two places to remember.
///
/// What is here and what is NOT is the whole content:
///
/// * `node_name` — from `DeployOverlay::node_name`, whose own doc says it "IS
///   honored on hosted boards" (issue #98). NOT from the blob: on hosted, the
///   macro writes the launch-declared name into the overlay field, and reading
///   both would be two answers to one question.
/// * `namespace` — from the `.nros_boot_config` BLOB, because the blob is what
///   a post-link patcher rewrites (RFC-0045); a second `DeployOverlay` field
///   for it would disagree the first time anyone patched one. ABSENT, never
///   `Some("")`: an unbaked namespace must fall through to the compiled
///   default, not shadow the env rung above it.
/// * `locator` / `domain_id` — deliberately absent. Issue #48: a HOST resolves
///   its connect facts from the environment, so leaving them `None` is what
///   keeps `$NROS_LOCATOR` / `$ROS_DOMAIN_ID` authoritative. Identity is not a
///   connect fact, which is the distinction issue 1434 drew and this keeps.
///
/// Both identity fields sit BELOW the environment either way —
/// `try_resolve_with` reads `env.node_name.or(baked.node_name)` and
/// `env.namespace.or(baked.namespace)` — so filling them costs
/// `$NROS_NODE_NAME` / `$NROS_NODE_NAMESPACE` nothing.
fn hosted_baked_rung(deploy: &nros_platform::DeployOverlay) -> ::nros::BootConfig<'static> {
    let blob = deploy
        .boot_config
        .map(::nros::BootConfig::from_baked)
        .unwrap_or_default();
    ::nros::BootConfig {
        node_name: deploy.node_name,
        namespace: blob.namespace,
        ..Default::default()
    }
}

/// Issue 1732 — close a hosted image's RMW session before the process exits.
///
/// `Executor::close` first, then the drop: the same order `nros_cpp_fini` uses
/// for the C and C++ entries, so the pre-shutdown hooks run while the session
/// is still open (issue 0790) and the backend's `destroy_session` is what ends
/// it — for XRCE that is `uxr_delete_session`, which is the only thing that
/// makes an Agent forget a client's participant.
///
/// A failed close is reported and not fatal: the process is exiting either
/// way, and the report is the only trace the operator gets.
fn close_session<B: BoardPrint>(crt: Option<::nros::node_runtime::ExecutorNodeRuntime>) {
    let Some(mut crt) = crt else {
        return;
    };
    if ::nros_platform::termination::requested() {
        B::println(format_args!(
            "nros: termination signal received; closing the RMW session"
        ));
    }
    if let Err(e) = crt.executor_mut().close() {
        B::println(format_args!("nros: closing the RMW session failed: {e:?}"));
    }
    drop(crt);
}

impl LinuxBoard {
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
        // Phase 249 P3.5 / phase-337 W8.a — register the linked backend before
        // the executor opens. Was the deleted `nros-board-native` shim's job;
        // it now sits on the ONE hosted boot funnel instead of on four
        // forwarding methods.
        register_linked_rmw();
        <Self as BoardInit>::init_hardware();
        // Phase 264 W3 — wire the default log sink (host → stdout/stderr) so a Node
        // pkg's `log_info!` produces output without per-app `nros_log::init`.
        // Idempotent (swaps the sink list atomically).
        ::nros_platform_cffi::log::init_default();
        // phase-338 W3 — and the `log` facade's host sink, for the same reason.
        // Four of the five scheduled-platform node bodies log via `log::info!`
        // (their boards bridge it: threadx's `install_uart_logger`, nuttx's
        // stdout logger); this board only ever inited `nros_log`, so such a body
        // ran on native and printed nothing at all. Bridging both here lets ONE
        // node body work on every board. W7 unifies the two stacks.
        install_stdout_log_bridge();
        // Issue 1732 — a SIGTERM / SIGINT ends the spin instead of the process,
        // so the session below is closed on the way out. Before the session
        // opens, so a signal during registration is not lost.
        ::nros_platform::termination::install_guard();

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
        // `locator` and `domain` stay `None` so env keeps authority over them
        // (issue #48 preserved).
        // issue 0687 — the ONE `hosted_env = true` in the tree, now spelled as
        // what it is: this board runs on a host, so it resolves through the
        // edge that reads the environment. Every other board passed `false`.
        //
        // Issue 1434 — the NAMESPACE is mapped too, and it is NOT the same
        // carve-out the locator and the domain get. "Leave it `None` so env
        // keeps authority" was true of the other two and false of this one:
        // `try_resolve_with` reads `env.namespace.or(baked.namespace)`, so a
        // baked namespace is already BELOW `$NROS_NODE_NAMESPACE` and passing
        // it costs the environment nothing (`namespace_resolves_over_all_
        // three_rungs` in nros-node pins that ladder). With it left `None` the
        // hosted road had no bottom rung at all: `nros::main!` bakes the
        // launch-declared namespace into `.nros_boot_config` on hosted exactly
        // as it does on bare metal (`deploy_overlay_tokens` always sets
        // `boot_config`), and this board read only `node_name` out of it.
        //
        // The blob is reached through `deploy.boot_config` rather than through
        // a new `DeployOverlay` field, because the blob is what a post-link
        // patcher rewrites — a second overlay field would be a second answer
        // to "what namespace is this image deployed under", and the two would
        // disagree the first time anyone patched one.
        //
        // Issue 1419 — census mode is decided BEFORE the session opens, because
        // it decides which backend opens it (`census_arm` selects the recorder
        // through `$NROS_RMW`, which `resolve_hosted` reads) and at what sizing.
        // phase-463 W7 -- the profile switch, before the executor opens.
        profile_arm();
        let census = census_requested();
        let census_sizing = census.as_ref().and_then(|_| census_arm());
        let exec_cfg = ::nros::env::resolve_hosted(hosted_baked_rung(deploy));
        // phase-271 — open at the entry's declared sizing when supplied.
        let opened = match (census_sizing, sizing) {
            // Issue 1419 — a census is not sized by the contract it checks.
            (Some(census_sizing), _) => ::nros::Executor::open_sized(&exec_cfg, census_sizing),
            (None, None) => ::nros::Executor::open(&exec_cfg),
            (None, Some((cbs, sc))) => {
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
                        // phase-409 — the entry declares callbacks, not Nodes
                        // (`[package.metadata.nros.entry] max_callbacks`), so the
                        // Node-scaled tables keep the build-time default.
                        ..::nros::ExecutorSizing::DEFAULT
                    },
                )
            }
        };
        let mut crt_real: Option<::nros::node_runtime::ExecutorNodeRuntime> = match opened {
            Ok(e) => {
                let mut crt = ::nros::node_runtime::ExecutorNodeRuntime::from_executor(e);
                // phase-436 — the single-executor entry parks like the tier
                // runners do.
                install_park(&mut crt);
                Some(crt)
            }
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
        // phase-463 W2 / issue 1419 -- census mode reaches its answer where the
        // spin ends, which on this funnel is here: the generated Rust entry
        // registers and then would spin INSIDE `setup`, and its spin helpers
        // return at once under `$NROS_CENSUS_OUT`, so this is the first point at
        // which every declaration is complete. Written whether or not `setup`
        // succeeded; see `census_finish`.
        if let Some(path) = census {
            let session = deploy.node_name.unwrap_or("node");
            census_finish(&path, session, result.is_ok());
        }
        // Issue 1732 — close the session BEFORE the process exits, on the
        // success path and the error path alike. `exit_*` is
        // `std::process::exit`, which runs no destructor, so the executor (and
        // the session inside it) used to die with the process without ever
        // reaching `destroy_session`. An XRCE Agent has no lease on its
        // clients: it kept the dead image's participant, with every topic,
        // service and node on it, until the Agent itself restarted.
        close_session::<Self>(crt_real.take());
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

    /// Phase 228.E — per-tier multi-task entry. Opens the one RMW session,
    /// then (issue 2016, RFC-0108 "As built in P10") runs each [`TierSpec`]
    /// as one LEVEL over the core's ONE executor
    /// ([`nros::levels::LevelCore`]): the least urgent level runs on the
    /// calling thread and owns the session (issue 0636), every other level is
    /// a worker `std::thread` over the same executor, started once the level
    /// before it has set up (issue #144). Each level sets its
    /// `active_groups` filter and runs `setup` (register-only — only that
    /// level's callbacks take, and the level runs them), then spins until a
    /// termination signal (issue 1732) or a bounded run's deadline.
    ///
    /// `setup` is `Fn` (not `FnOnce`) — it is invoked once per level — and
    /// `Sync`, since the level threads share `&setup`. It must register
    /// entities only; the spin loop is owned here so the board can install
    /// the group filter first. (The single-tier [`BoardEntry::run`] path,
    /// where `setup` owns the spin, is unchanged.)
    ///
    /// Each level thread adopts its tier's SCHED_FIFO priority when the
    /// process may (`CAP_SYS_NICE`), and says so or why not. Blocks until
    /// SIGTERM / SIGINT (server semantics), then joins the level threads,
    /// takes the runtime back ([`nros::levels::LevelCore::into_runtime`]),
    /// closes the session and returns `Ok` (issue 1732). A level whose setup
    /// fails fails the boot.
    pub fn run_tiers<F, E>(
        deploy: &nros_platform::DeployOverlay,
        tiers: &[TierSpec<'_>],
        // issue 1571 — the spawned tiers' executor backing: the entry's `.bss`
        // `TierExecutorBacking`, emitted by `nros::main!`.
        tier_backing: &'static mut [::nros::TierExecutorBackingSlot],
        setup: F,
    ) -> Result<(), E>
    where
        F: Fn(&mut RuntimeCtx<'_>) -> Result<(), E> + Sync,
        E: core::fmt::Debug,
    {
        // Issue #48 — hosted boards take their locator from the environment, so
        // the overlay's CONNECT fields are ignored here (the firmware boards'
        // `run_tiers` consumes them). Its IDENTITY fields are not connect
        // facts and are no longer ignored: issue 1434 read the baked namespace
        // out of `deploy.boot_config`, and issue 1442 added `deploy.node_name`
        // beside it, both at the session open below. That is the same line
        // `boot_hosted` draws.
        // phase-337 W8.a — the second boot funnel, so it registers too.
        register_linked_rmw();
        <Self as BoardInit>::init_hardware();
        // Phase 264 W3 — default log sink at boot (see `run`).
        ::nros_platform_cffi::log::init_default();
        // Same rationale as `run` (phase-338 W3): bridge the `log` facade
        // too, or a node body using `log::info!` prints nothing on the
        // multi-tier native entry while working on every RTOS board.
        install_stdout_log_bridge();
        // Issue 1732 — see `boot_hosted`: a termination signal ends every
        // tier's spin, and the session is closed once they have all returned.
        ::nros_platform::termination::install_guard();

        if tiers.is_empty() {
            <Self as BoardPrint>::println(format_args!(
                "nros: run_tiers called with no tiers — nothing to run"
            ));
            <Self as BoardExit>::exit_failure();
        }
        // issue 1571 — the one refusal, before the session opens: every spawned
        // tier needs a slot of the entry's `.bss` backing. Hosted, so the heap
        // has no fixed reservation and this moves nothing a budget pays for —
        // but it is the same METHOD every board uses, so one entry expansion
        // serves them all.
        if let Err(short) = ::nros::check_tier_executor_backing(tiers.len(), tier_backing) {
            <Self as BoardPrint>::println(format_args!("nros: {short}"));
            <Self as BoardExit>::exit_failure();
        }

        // RFC-0079 / issue 0765 — place the transport ABOVE every tier, BEFORE
        // the session opens. The read/lease tasks are created during open and
        // `zpico_set_task_config` is a process-wide default consulted at that
        // moment; after open it changes nothing.
        //
        // Necessary here in a way it is not on the RTOSes. Once tier priorities
        // became real (issue 0765) the tiers run SCHED_FIFO while the read and
        // lease tasks stay on SCHED_OTHER, and a SCHED_FIFO thread outranks
        // every SCHED_OTHER thread unconditionally — so the app preempted the
        // link it publishes over, always, with no arrangement in which it did
        // not. That is not one of the two legitimate orderings issue 0623
        // describes; it is the absence of a choice.
        //
        // DERIVED, not a literal: the right value depends on the tier table
        // this entry was generated with, and a constant that stops being above
        // the tiers when someone edits `system.toml` is worse than no constant.
        // Best-effort like every other priority on this port — without
        // `CAP_SYS_NICE` the attribute is ignored at spawn and the tiers'
        // own refusal line has already said why.
        // `feature = "rmw-zenoh"` as well as `unix`: the symbol below lives in
        // zpico-sys's C shim, which is only in the graph when the zenoh backend
        // is selected. Guarded on `unix` alone, a board built for another RMW
        // referenced a symbol nothing linked — `undefined symbol:
        // zpico_set_task_config`, at LINK time in the consumer, naming neither
        // this crate nor the missing backend.
        //
        // Same family as issue 0919 one layer up: a component assuming zenoh is
        // present because it usually is. The priority plan below is about
        // TRANSPORT tasks, and a build with no zenoh transport has none to
        // place — so skipping it is the correct behaviour, not a degradation.
        #[cfg(all(unix, feature = "rmw-zenoh"))]
        {
            // RFC-0079 — the RESERVED band's floor, not `max_tier + 1`.
            //
            // Deriving it from the tiers was circular: a reserved band defined
            // by the tiers it is meant to sit above moves whenever they do, and
            // cannot be checked statically because nothing knows it until the
            // entry runs. Every other port states a fixed band (FreeRTOS 4,
            // NuttX 100, ThreadX 14) with the pool below it; POSIX now does
            // too — `[board.priority_plan]` in packages/boards/linux, reserved
            // 90..99, pool 1..89, cross-checked against this constant by
            // `check-tier-priority-plan` so the two cannot drift.
            const TRANSPORT_BAND_FLOOR: u32 = 90;
            let max_tier = tiers.iter().map(|t| t.priority).max().unwrap_or(0);
            if max_tier > 0 {
                let transport = TRANSPORT_BAND_FLOOR;
                unsafe extern "C" {
                    fn zpico_set_task_config(
                        read_priority: u32,
                        read_stack_bytes: u32,
                        lease_priority: u32,
                        lease_stack_bytes: u32,
                    );
                }
                // 0 stack bytes = "leave the port's default alone" — this
                // states a PRIORITY, not a stack size.
                unsafe { zpico_set_task_config(transport, 0, transport, 0) };
                <Self as BoardPrint>::println(format_args!(
                    "nros: transport tasks at SCHED_FIFO {} (reserved band floor; most \
                     urgent tier is {})",
                    transport, max_tier
                ));
            }
        }

        // Open the one session on the boot task; it owns the session for
        // the program's life (until a termination signal ends every spin).
        //
        // Issue 1434 — this was `ExecutorConfig::from_env()`, which issue
        // 0687's own test pins as `resolve_hosted(BootConfig::default())`: the
        // env rung over NOTHING. So the tiered hosted entry had no baked rung
        // at all, and a launch-declared namespace stopped here exactly as it
        // did on the single-executor path above. Spelling it as
        // `resolve_hosted` with the blob's namespace is the SAME call with the
        // one rung filled in; `$NROS_NODE_NAMESPACE` still outranks it.
        //
        // Issue 1442, the OLDER half — `node_name` travels too, and it is a
        // separate defect from the namespace above with its own history.
        // `run_tiers` has ignored `deploy` since issue #48 ("kept for
        // signature parity"), so this was the one boot funnel on which
        // `DeployOverlay::node_name` did nothing — while the field's own
        // doc-comment on `nros_platform::DeployOverlay` says it is "applied to
        // the boot `ExecutorConfig` by the board, so unlike `locator` this IS
        // honored on hosted boards". That was true of `boot_hosted` and false
        // here, which is the shape 1442 exists to close: half an identity
        // threaded reads exactly like a whole one.
        //
        // It DOES change what `ros2 node list` prints for a tiered native
        // image whose launch names one node — from `/node` to that name — and
        // that is the point rather than a side effect: the single-executor
        // hosted path has printed the launch-declared name since issue #98,
        // and two boot funnels of one board disagreeing about a node's name is
        // not a contract anyone chose. `$NROS_NODE_NAME` still outranks it
        // (`env.node_name.or(baked.node_name)`), so nothing that overrode the
        // name before overrides it less now.
        //
        // The source is `deploy.node_name`, the same rung `boot_hosted` reads,
        // NOT `blob.node_name`: on hosted the macro puts the launch-declared
        // name in the overlay field, and reading the blob instead would be a
        // second answer to one question.
        // Issue 1419 — census mode, decided before the session opens (see
        // `boot_hosted`).
        // phase-463 W7 -- the profile switch, before the executor opens.
        profile_arm();
        let census = census_requested();
        let census_sizing = census.as_ref().and_then(|_| census_arm());
        let exec_cfg = ::nros::env::resolve_hosted(hosted_baked_rung(deploy));
        let opened = match census_sizing {
            Some(census_sizing) => ::nros::Executor::open_sized(&exec_cfg, census_sizing),
            None => ::nros::Executor::open(&exec_cfg),
        };
        let boot_exec = match opened {
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

        // Issue 1419 — a census of a TIERED entry is ONE registration pass on the
        // boot executor with no group filter (`active_groups` unset admits every
        // entity), so every tier's entities are recorded exactly once and no
        // tier thread is spawned. A census describes what the code CREATES; the
        // tier split is placement, which the contract states separately.
        if let Some(path) = census {
            let result = {
                let mut runtime = RuntimeCtx::with_runtime(&mut boot_crt);
                setup(&mut runtime)
            };
            if let Err(e) = &result {
                <Self as BoardPrint>::println(format_args!("nros: application error: {e:?}"));
            }
            census_finish(&path, deploy.node_name.unwrap_or("node"), result.is_ok());
        }

        // issue 2016 (RFC-0108 "As built in P10") -- each tier is one LEVEL
        // over the core's ONE executor (`nros::levels::LevelCore`), the shape
        // the C ABI's `nros_board_native_run_levels_in` and the Zephyr and
        // ThreadX Rust runners have. The least urgent level runs on this
        // thread and owns the session (issue 0636: an owner that outranks its
        // peers and then spins starves them); every other level is a worker
        // thread over the same executor, started once the level before it has
        // set up (issue #144), so no two setups overlap on the session. The
        // tiers' executor backing is still checked against the table (issue
        // 1571), but one executor serves every level: the slots go unused.
        let _ = tier_backing;
        if tiers.len() > ::nros::MAX_LEVELS {
            <Self as BoardPrint>::println(format_args!(
                "nros: {} tiers is past the {} levels a core holds",
                tiers.len(),
                ::nros::MAX_LEVELS
            ));
            <Self as BoardExit>::exit_failure();
        }
        let n = tiers.len();
        // Each tier's level: its urgency among the tiers, POSIX
        // bigger-is-urgent, equal priorities in table order.
        let level_of =
            |i: usize| ::nros::tier_level_by(n, i, false, |j| tiers[j].priority).unwrap_or(0);
        let boot_index = (0..n).max_by_key(|&i| (level_of(i), i)).unwrap_or(0);
        let boot_tier = &tiers[boot_index];

        // The boot level's own lowering, on the executor before it is shared.
        // issue 0765 -- its declared priority too: it runs on the calling
        // thread, the tier most easily forgotten.
        apply_tier_priority(boot_tier);
        // issue 1172 -- `Err` means the filter did not fit, and
        // `set_active_groups` has already cleared it and left filtering ON,
        // so this level registers NOTHING rather than registering on a
        // quietly narrower set.
        let _ = boot_crt.executor_mut().set_active_groups(boot_tier.groups);
        install_park(&mut boot_crt);
        // Issue 2018 -- the boot level's policy lowers onto its own context
        // below, once the level core seats it, not onto the default.
        apply_tier_affinity::<Self>(boot_tier);

        // The executor's lock, before a second thread can reach it: the
        // POSIX port's recursive `PTHREAD_PRIO_INHERIT` mutex and
        // `pthread_self`, so each pass releases it around its callbacks and
        // its park. One level needs none. The mutex lives for the process.
        let mutex: *mut core::ffi::c_void = if n > 1 {
            // SAFETY: a pure size query of the linked port.
            let words = unsafe { nros_platform_mutex_rec_storage_size() }
                .div_ceil(8)
                .max(1);
            let m: &'static mut [u64] = Box::leak(vec![0u64; words].into_boxed_slice());
            let mp = m.as_mut_ptr() as *mut core::ffi::c_void;
            // SAFETY: `mp` is `words` writable words, leaked for the process.
            if unsafe { nros_platform_mutex_rec_init(mp) } != 0 {
                <Self as BoardPrint>::println(format_args!(
                    "nros: native run_levels: the executor lock was not installed; refused"
                ));
                <Self as BoardExit>::exit_failure();
            }
            mp
        } else {
            core::ptr::null_mut()
        };
        // SAFETY: no second thread exists yet; the mutex was initialised
        // above (or is unused with one level); `pthread_self` names the
        // calling thread.
        let core = match unsafe {
            ::nros::levels::LevelCore::new(
                boot_crt,
                u8::try_from(n).unwrap_or(u8::MAX),
                level_of(boot_index),
                mutex,
                nros_platform_mutex_rec_lock,
                nros_platform_mutex_rec_unlock,
                posix_thread_id,
            )
        } {
            Ok(c) => c,
            Err(e) => {
                <Self as BoardPrint>::println(format_args!(
                    "nros: native run_levels: the level runtime refused: {e:?}"
                ));
                <Self as BoardExit>::exit_failure();
            }
        };

        // issue #144 -- the boot level's setup FIRST.
        // SAFETY: the runtime reference lives inside the closure only.
        let rc = unsafe {
            core.with_runtime(|crt| {
                // Issue 2018 -- the boot level owns the session and drives the
                // transport, so it gets no kernel reservation (a throttled
                // owner stalls every level's I/O, issue #246's shape); its
                // budget, if any, is the executor's, gating its callbacks only.
                apply_level_sched::<Self>(crt, boot_tier, false);
                let mut ctx = RuntimeCtx::with_runtime(crt);
                setup(&mut ctx)
            })
        };
        if let Err(e) = rc {
            <Self as BoardPrint>::println(format_args!(
                "nros: boot tier `{}` setup failed: {e:?}",
                boot_tier.name
            ));
            // SAFETY: no other level was started.
            close_session::<Self>(Some(unsafe { core.into_runtime() }));
            return Err(e);
        }

        let setup = &setup;
        let deadline = entry_spin_deadline();
        let stop = std::sync::atomic::AtomicBool::new(false);
        let stop = &stop;
        let mut failed = false;
        std::thread::scope(|scope| {
            for (i, tier) in tiers.iter().enumerate() {
                if i == boot_index {
                    continue;
                }
                let level = level_of(i);
                // 0 setting up, 1 set up, 2 failed.
                let state = std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0));
                let st = std::sync::Arc::clone(&state);
                let mut builder =
                    std::thread::Builder::new().name(format!("nros-tier-{}", tier.name));
                // phase-302 W2 (issue 0262) -- a declared per-tier stack.
                if tier.stack_bytes > 0 {
                    builder = builder.stack_size(tier.stack_bytes);
                }
                let spawn = builder.spawn_scoped(scope, move || {
                    use std::sync::atomic::Ordering::SeqCst;
                    // issue 0765 -- the level's own priority before any work.
                    apply_tier_priority(tier);
                    apply_tier_affinity::<Self>(tier);
                    // SAFETY: once, on this level's own thread.
                    let worker = match unsafe { core.open_level(level) } {
                        Ok(w) => w,
                        Err(e) => {
                            <Self as BoardPrint>::println(format_args!(
                                "nros: tier `{}` did not open as level {level}: {e:?}",
                                tier.name
                            ));
                            st.store(2, SeqCst);
                            return;
                        }
                    };
                    // SAFETY: the runtime reference lives inside the closure.
                    // Issue 2018 -- the kernel's reservation on this level's
                    // thread; the level's context defers to it when it took.
                    let kernel = apply_tier_deadline(tier);
                    let rc = unsafe {
                        core.with_runtime(|crt| {
                            let _ = crt.executor_mut().set_active_groups(tier.groups);
                            apply_level_sched::<Self>(crt, tier, kernel);
                            let mut ctx = RuntimeCtx::with_runtime(crt);
                            setup(&mut ctx)
                        })
                    };
                    if let Err(e) = rc {
                        <Self as BoardPrint>::println(format_args!(
                            "nros: tier `{}` setup failed: {e:?}",
                            tier.name
                        ));
                        st.store(2, SeqCst);
                        return;
                    }
                    st.store(1, SeqCst);
                    let period = std::time::Duration::from_micros(tier.spin_period_us.max(1));
                    // issue 0636 option 3 -- every iteration reaches a
                    // scheduling point.
                    let mut gap = ::nros_platform::TierSpinGap::new(tier.spin_period_us);
                    while !stop.load(SeqCst) && !run_should_end(deadline) {
                        let iter = gap.mark();
                        // SAFETY: `worker` is this thread's, from `open_level`.
                        unsafe { core.spin_level_once(worker, period) };
                        gap.after_spin(iter);
                    }
                });
                if let Err(e) = spawn {
                    <Self as BoardPrint>::println(format_args!(
                        "nros: failed to spawn tier `{}`: {e}",
                        tier.name
                    ));
                    failed = true;
                    break;
                }
                // The next level starts only once this one has set up.
                while state.load(std::sync::atomic::Ordering::SeqCst) == 0 {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
                if state.load(std::sync::atomic::Ordering::SeqCst) == 2 {
                    <Self as BoardPrint>::println(format_args!(
                        "nros: level {level} (`{}`) failed: failing the boot",
                        tier.name
                    ));
                    failed = true;
                    break;
                }
            }
            if !failed {
                // Unique lines (the single-tier path never prints them) so an
                // E2E can confirm the image entered the level run with a live
                // session.
                <Self as BoardPrint>::println(format_args!(
                    "nros: multi-tier run — {} tier(s) over one session",
                    n
                ));
                <Self as BoardPrint>::println(format_args!(
                    "nros: multi-level run — {n} level(s) over one executor"
                ));
                let period = std::time::Duration::from_micros(boot_tier.spin_period_us.max(1));
                let mut gap = ::nros_platform::TierSpinGap::new(boot_tier.spin_period_us);
                while !run_should_end(deadline) {
                    let iter = gap.mark();
                    // SAFETY: this is the boot level's thread.
                    unsafe { core.spin_boot_once(period) };
                    gap.after_spin(iter);
                }
            }
            stop.store(true, std::sync::atomic::Ordering::SeqCst);
        });

        // Issue 1732 -- reached once a termination signal (or the bounded
        // run's deadline) has ended every level's spin and `thread::scope`
        // has joined the workers, so nothing else reaches the executor. Take
        // the runtime back from the levels (issue 2016), close the session,
        // then return: the generated `main` returns and the process exits 0.
        // SAFETY: every other level's thread was joined by the scope above.
        close_session::<Self>(Some(unsafe { core.into_runtime() }));
        if failed {
            <Self as BoardExit>::exit_failure();
        }
        <Self as BoardPrint>::println(format_args!("nros: application complete"));
        Ok(())
    }
}

// issue 2016 -- the POSIX port's recursive priority-inheriting mutex, the
// level runner's executor lock (`nros-platform-posix`).
unsafe extern "C" {
    fn nros_platform_mutex_rec_storage_size() -> usize;
    fn nros_platform_mutex_rec_init(m: *mut core::ffi::c_void) -> i8;
    fn nros_platform_mutex_rec_lock(m: *mut core::ffi::c_void) -> i8;
    fn nros_platform_mutex_rec_unlock(m: *mut core::ffi::c_void) -> i8;
}

/// The executor lock's thread identity: the calling thread's `pthread_t`.
unsafe extern "C" fn posix_thread_id() -> usize {
    // SAFETY: `pthread_self` takes nothing and never fails.
    unsafe { libc::pthread_self() as usize }
}

/// Whether a level run ends: a termination signal (issue 1732) or a bounded
/// run's deadline (`NROS_ENTRY_SPIN_MS`, issue 1310).
fn run_should_end(deadline: Option<std::time::Instant>) -> bool {
    ::nros_platform::termination::requested()
        || deadline.is_some_and(|d| std::time::Instant::now() >= d)
}

// phase-436 C2 — the POSIX port's microsecond park (`sem_clockwait` on an
// absolute CLOCK_MONOTONIC deadline) and its granularity, which is the
// monotonic clock's resolution: 1 us on a Linux host with hrtimers, one tick
// (CONFIG_USEC_PER_TICK) on NuttX, which compiles the same POSIX source. Without
// it every park here is floored at the `wake_wait_ms` ABI's millisecond.
//
// `//` not `///`: rustdoc documents nothing on an extern block.
unsafe extern "C" {
    fn nros_platform_wake_park_until_us(w: *mut core::ffi::c_void, deadline_us: u64) -> i8;
    fn nros_platform_wake_park_granularity_us() -> u64;
}

/// Install the park on `crt`'s executor. Advisory: a build with no wake object
/// keeps the path it had.
fn install_park(crt: &mut ::nros::node_runtime::ExecutorNodeRuntime) {
    let granularity = unsafe { nros_platform_wake_park_granularity_us() };
    let _ = ::nros::port_park::install_port_park(
        crt.executor_mut(),
        nros_platform_wake_park_until_us,
        granularity,
    );
}

/// RFC-0052 / phase-296 W3a, issue 2018 (RFC-0108 As-built) -- lower a
/// level's RTOS-agnostic scheduling policy onto the level's OWN scheduling
/// context (`apply_level_sched_policy`), which what its setup registers binds
/// to, rather than onto the executor's default, which every level on the core
/// shares. Call it on the registering level, before its setup.
///
/// `kernel`: the kernel enforces the level's budget on its thread
/// (`SCHED_DEADLINE`, [`apply_tier_deadline`]), so the executor does not
/// enforce it a second time; otherwise the executor's cooperative budget does,
/// with the same numbers. One enforcer per level, and the line says which.
fn apply_level_sched<B: BoardPrint>(
    crt: &mut ::nros::node_runtime::ExecutorNodeRuntime,
    tier: &TierSpec<'_>,
    kernel: bool,
) {
    let sc = crt.apply_level_sched_policy(
        tier.class,
        tier.period_us,
        tier.budget_us,
        tier.deadline_us,
        tier.deadline_policy,
        kernel,
    );
    if let (Some(_), Some(b), Some(p)) = (sc, tier.budget_us, tier.period_us) {
        B::println(format_args!(
            "nros: level budget tier=`{}` {b}us/{p}us enforced by the {}",
            tier.name,
            if kernel { "KERNEL" } else { "EXECUTOR" }
        ));
    }
}

#[cfg(unix)]
unsafe extern "C" {
    fn nros_posix_apply_current_deadline(
        name: *const core::ffi::c_char,
        tier_class: *const core::ffi::c_char,
        budget_us: u64,
        period_us: u64,
        deadline_us: u64,
    ) -> i32;
}

/// Issue 2018 -- put the CALLING level thread under `SCHED_DEADLINE` with its
/// tier's budget, period and deadline (`nros-platform-posix`'s
/// `nros_posix_apply_current_deadline`, shared with the C ABI's level
/// runner). `true` when the kernel accepted it. Without `CAP_SYS_NICE` the
/// kernel refuses, the C side says so once with the fix, and the executor's
/// budget is the enforcement. A tier that declares no reservation is
/// untouched.
#[cfg(unix)]
fn apply_tier_deadline(tier: &TierSpec<'_>) -> bool {
    let (Some(class), Some(budget), Some(period)) = (tier.class, tier.budget_us, tier.period_us)
    else {
        return false;
    };
    let (Ok(name), Ok(class)) = (
        std::ffi::CString::new(tier.name),
        std::ffi::CString::new(class),
    ) else {
        return false;
    };
    // SAFETY: both strings are NUL-terminated and outlive the call.
    unsafe {
        nros_posix_apply_current_deadline(
            name.as_ptr(),
            class.as_ptr(),
            budget,
            period,
            tier.deadline_us.unwrap_or(0),
        ) == 1
    }
}

#[cfg(not(unix))]
fn apply_tier_deadline(_tier: &TierSpec<'_>) -> bool {
    false
}

/// phase-296 W5.13 — the placement dim's LINUX realization: pin the CALLING
/// tier thread to its declared `core` via `sched_setaffinity` (no-op when the
/// tier declares no core). Unlike the RTOS SMP arms (zephyr/nuttx/freertos/
/// threadx), a Linux host is genuinely multi-core and the call needs no
/// privilege, so this is the FIRST RUNTIME ACCEPT-arm proof of the core-pin
/// consumer (issue #260). Never silently drops: a rejected pin (bad cpu id)
/// falls back LOUDLY and the tier runs unpinned.
///
/// **Linux, not POSIX** — this doc said "POSIX" and the distinction is the
/// whole reason the board is named `linux`. `sched_setaffinity`, `cpu_set_t`
/// and `CPU_SET` are absent from libc's apple module, and nothing here is
/// `cfg(target_os)`-gated, so this crate does not build on macOS. Gating it to
/// a loud no-op off Linux is what would make the board's reach `posix`; until
/// someone needs that, the accurate word is the one in the crate name.
fn apply_tier_affinity<B: BoardPrint>(tier: &TierSpec<'_>) {
    let Some(core) = tier.core else {
        return;
    };
    // SAFETY: libc FFI — a zeroed `cpu_set_t` with a single CPU bit set, applied
    // to the calling thread (pid 0). No aliasing, no allocation.
    let accepted = unsafe {
        let mut set: libc::cpu_set_t = core::mem::zeroed();
        libc::CPU_SET(core as usize, &mut set);
        libc::sched_setaffinity(0, core::mem::size_of::<libc::cpu_set_t>(), &set) == 0
    };
    if accepted {
        // Literal mirrors `nros_tests::output::LINUX_CORE_PIN_MARKER`.
        B::println(format_args!(
            "nros: core pin tier=`{}` cpu={}",
            tier.name, core
        ));
    } else {
        // Literal mirrors `nros_tests::output::LINUX_CORE_PIN_FALLBACK_MARKER`.
        B::println(format_args!(
            "nros: core pin FAILED tier=`{}` cpu={} — sched_setaffinity failed, \
             tier runs unpinned",
            tier.name, core
        ));
    }
}

// NOTE: this text describes a different function and is preserved verbatim,
// only demoted from `///` to `//`. A doc comment cannot attach to an `extern`
// block, so under the lane's `-D warnings` it was a hard `unused_doc_comments`
// error; whoever owns issue 0765 should say what it meant to say.
// Register + spin one tier on a freshly-opened borrowed-session
// executor (spawned-tier path).
// RFC-0079 / issue 0765 — see `apply_tier_priority` below.
#[cfg(unix)]
unsafe extern "C" {
    fn nros_posix_apply_current_priority(name: *const core::ffi::c_char, priority: u32) -> i32;
}

/// Adopt this tier's declared SCHED_FIFO priority on the CALLING thread, and
/// report the outcome.
///
/// The native board spawns tiers with `std::thread::scope`, which never reaches
/// `nros-platform-posix`'s `task_init` — so the attribute path that gives NuttX
/// its priorities cannot reach a native tier, and every `[tiers.*.posix]
/// priority` in the tree was inert with nothing saying so. This is the same
/// self-apply-at-tier-entry shape NuttX and FreeRTOS already use: one
/// implementation per port, one marker, and a refusal that is REPORTED.
///
/// Not fatal when refused. An unprivileged host is the normal case for `just
/// ci`, and trading a silent degradation for a stopped test suite would be a
/// bad bargain; the operator gets a line naming `setcap cap_sys_nice+ep`
/// instead.
#[cfg(unix)]
fn apply_tier_priority(tier: &TierSpec<'_>) {
    if tier.priority <= 0 {
        return;
    }
    let prio = tier.priority.clamp(0, i64::from(u32::MAX)) as u32;
    // The tier name reaches the C side for the marker; a NUL-bearing name is
    // simply reported as `?` rather than failing the tier.
    match std::ffi::CString::new(tier.name) {
        Ok(c) => unsafe {
            nros_posix_apply_current_priority(c.as_ptr(), prio);
        },
        Err(_) => unsafe {
            nros_posix_apply_current_priority(core::ptr::null(), prio);
        },
    }
}

#[cfg(not(unix))]
fn apply_tier_priority(_tier: &TierSpec<'_>) {}

/// The deadline a numeric `NROS_ENTRY_SPIN_MS` puts on a tiered run, or `None`
/// for an unbounded one (unset, `0`, `forever`, or not a number).
///
/// The single-executor road (`nros::main!`'s hosted spin) and the C/C++ tiered
/// runner (`nros-cpp`'s boot loop) both honour the knob; this road ignored it,
/// so a tiered Rust entry run by a bounded observer never exited and the
/// runtime registration test could only report it as HUNG (issue 1310). Each
/// tier reads the same deadline and ends at it, and `thread::scope` then joins
/// them, so the image closes its session and exits as it does on a signal.
fn entry_spin_deadline() -> Option<std::time::Instant> {
    let ms: u64 = std::env::var("NROS_ENTRY_SPIN_MS")
        .ok()?
        .trim()
        .parse()
        .ok()?;
    (ms > 0).then(|| std::time::Instant::now() + std::time::Duration::from_millis(ms))
}

/// phase-338 W3 — route the `log` facade to stdout on hosted targets.
///
/// Mirrors `nros-board-nuttx::install_stdout_logger`. `set_logger` may only
/// succeed once per process, and the `is_ok()` guard makes this a NO-OP when
/// the application installed its own (the imperative examples call
/// `env_logger::init()` in `main` before the board ever boots) — so this adds a
/// sink where there was none and never fights one that exists.
fn install_stdout_log_bridge() {
    use std::{io::Write as _, sync::Once};

    struct StdoutLogger;
    impl log::Log for StdoutLogger {
        // Issue 1949: nros-log's threshold for the record's target, not
        // `true` — `set_max_level(Trace)` below is the ceiling a later
        // `Logger::set_default_level` needs, not a filter.
        fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
            nros_log::log_compat::admits(metadata)
        }
        fn log(&self, record: &log::Record<'_>) {
            // The `log` macros never call `enabled`.
            if !nros_log::log_compat::admits(record.metadata()) {
                return;
            }
            // The examples bake the full human line into the message
            // (`Publishing: '...'` / `I heard: [...]`), so the message is
            // emitted verbatim — the e2e harness greps those markers with
            // `contains`, so the level prefix below leaves every one intact.
            //
            // The prefix is not decoration. Issue 0309 made the workspace
            // logging proof require the level tag specifically so that a line
            // carrying the marker but no tag reads as "this record bypassed the
            // logging facade". When this bridge landed (phase-338 W3) it became
            // the Rust lane's `log::Log`, and printing `record.args()` bare made
            // every Rust record look exactly like the bypass the proof was
            // written to catch — `workspace_features_e2e` rust/logging went to
            // 0 tagged lines while c/cpp/mixed stayed green, because those go
            // through the C API to `nros_platform_log_write`, which prefixes.
            //
            // Labels match `severity_label_log` in nros-platform-posix so the
            // two lanes render a record the same way.
            let level = match record.level() {
                log::Level::Trace => "TRACE",
                log::Level::Debug => "DEBUG",
                log::Level::Info => "INFO",
                log::Level::Warn => "WARN",
                log::Level::Error => "ERROR",
            };
            let mut out = std::io::stdout();
            let _ = writeln!(out, "[{level}] {}", record.args());
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
            // Issue 1988: `RUST_LOG` is the host's runtime level, and #1949's
            // threshold had left nothing reading it. Only when THIS sink is
            // installed — an application that installed `env_logger` reads
            // the variable itself.
            if let Some(level) = std::env::var("RUST_LOG")
                .ok()
                .as_deref()
                .and_then(rust_log_default_level)
            {
                nros_log::Logger::set_default_level(level);
            }
        }
    });
}

/// The default threshold a `RUST_LOG` value states, if it states one.
///
/// Issue 1988. Honours env_logger's GLOBAL directive — a bare level among the
/// comma-separated directives (`trace`, `info,foo=debug`); the last one wins,
/// and `off` maps to `Fatal`, which no `log` record reaches. Per-target
/// directives (`foo=debug`, a bare `foo`) are IGNORED: a `log` target is a
/// `::` module path and nros-log's hierarchy is dotted, so an exact-name
/// logger would match one module, not the prefix env_logger means, and a
/// directive that half-works reads as working. A `/regex` suffix is dropped.
fn rust_log_default_level(value: &str) -> Option<nros_log::Severity> {
    use nros_log::Severity;
    let spec = value.split('/').next().unwrap_or("");
    let mut level = None;
    for directive in spec.split(',').map(str::trim) {
        let found = match directive.to_ascii_lowercase().as_str() {
            "trace" => Severity::Trace,
            "debug" => Severity::Debug,
            "info" => Severity::Info,
            "warn" => Severity::Warn,
            "error" => Severity::Error,
            "off" => Severity::Fatal,
            _ => continue,
        };
        level = Some(found);
    }
    level
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "C" {
        fn nros_posix_deadline_attr(
            tier_class: *const core::ffi::c_char,
            budget_us: u64,
            period_us: u64,
            deadline_us: u64,
            runtime_ns: *mut u64,
            deadline_ns: *mut u64,
            period_ns: *mut u64,
        ) -> i32;
    }

    fn deadline_attr(class: &core::ffi::CStr, b: u64, p: u64, d: u64) -> Option<(u64, u64, u64)> {
        let (mut rt, mut dl, mut pd) = (0u64, 0u64, 0u64);
        // SAFETY: a pure computation over the arguments and three out-params.
        let ok =
            unsafe { nros_posix_deadline_attr(class.as_ptr(), b, p, d, &mut rt, &mut dl, &mut pd) };
        (ok == 1).then_some((rt, dl, pd))
    }

    /// Issue 2018 -- a level's budget becomes a SCHED_DEADLINE reservation:
    /// runtime = budget, period = period, deadline = the declared one when it
    /// lies within [budget, period], else the period. Anything else asks the
    /// kernel for nothing.
    #[test]
    fn a_level_budget_maps_onto_a_deadline_reservation() {
        assert_eq!(
            deadline_attr(c"real_time", 5_000, 10_000, 0),
            Some((5_000_000, 10_000_000, 10_000_000))
        );
        assert_eq!(
            deadline_attr(c"real_time", 2_000, 10_000, 4_000),
            Some((2_000_000, 4_000_000, 10_000_000)),
            "a deadline inside [budget, period] is the reservation's"
        );
        assert_eq!(
            deadline_attr(c"real_time", 2_000, 10_000, 1_000),
            Some((2_000_000, 10_000_000, 10_000_000)),
            "a deadline below the budget is unsatisfiable: the period stands in"
        );
        assert_eq!(deadline_attr(c"real_time", 0, 10_000, 0), None, "no budget");
        assert_eq!(deadline_attr(c"real_time", 5_000, 0, 0), None, "no period");
        assert_eq!(
            deadline_attr(c"real_time", 20_000, 10_000, 0),
            None,
            "budget > period"
        );
        assert_eq!(
            deadline_attr(c"best_effort", 5_000, 10_000, 0),
            None,
            "not real_time"
        );
    }

    /// Issue 2018 -- the kernel throttles a level thread that runs past its
    /// SCHED_DEADLINE budget. Needs `CAP_SYS_NICE`: where the kernel refuses
    /// (EPERM, the common unprivileged host), the call must report the
    /// refusal and leave the thread on its old policy, which is what this
    /// asserts instead -- the throttle half then has NOT run, and the test
    /// says so on its output.
    #[test]
    fn a_level_past_its_deadline_budget_is_throttled_by_the_kernel() {
        let tier = TierSpec {
            name: "overrun",
            class: Some("real_time"),
            budget_us: Some(2_000),
            period_us: Some(10_000),
            ..TierSpec::single()
        };
        let handle = std::thread::spawn(move || {
            let accepted = apply_tier_deadline(&tier);
            // SAFETY: plain libc reads of the calling thread's policy and clock.
            let policy = unsafe { libc::sched_getscheduler(0) };
            let cpu_ns = || {
                let mut ts = libc::timespec {
                    tv_sec: 0,
                    tv_nsec: 0,
                };
                unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut ts) };
                ts.tv_sec as u64 * 1_000_000_000 + ts.tv_nsec as u64
            };
            if !accepted {
                return (false, policy, 0u128);
            }
            // 100 ms of CPU against 2 ms per 10 ms: about 100 ms of wall time
            // unthrottled. Issue 2066 -- the kernel enforces the budget at the
            // scheduler tick unless HRTICK_DL is on (it is off by default), so
            // a slot can overrun to the next tick. With CAP_SYS_NICE on a
            // HZ=250 host this burn took 380-390 ms (a 26% share, not 20%),
            // and a 20 ms burn took 70 ms, under the 80 ms this test once
            // required. The long burn keeps the overruns small next to the
            // bound.
            let start = std::time::Instant::now();
            let cpu0 = cpu_ns();
            while cpu_ns() - cpu0 < 100_000_000 {
                core::hint::spin_loop();
            }
            (true, policy, start.elapsed().as_millis())
        });
        let (accepted, policy, wall_ms) = handle.join().expect("the level thread");
        if !accepted {
            assert_ne!(
                policy, 6,
                "a refused reservation leaves the thread off SCHED_DEADLINE"
            );
            println!(
                "issue 2018: SCHED_DEADLINE refused on this host (no CAP_SYS_NICE): the \
                 fallback was checked, the kernel throttle was NOT run"
            );
            return;
        }
        assert_eq!(policy, 6, "the level thread runs under SCHED_DEADLINE");
        assert!(
            wall_ms >= 300,
            "100 ms of CPU at 2 ms per 10 ms took only {wall_ms} ms of wall time: \
             the kernel did not throttle the level"
        );
    }

    #[test]
    fn init_hardware_is_noop() {
        // Smoke: calling `init_hardware` from a unit test must not
        // panic or affect global state.
        <LinuxBoard as BoardInit>::init_hardware();
    }

    /// Issue 1949: the hosted `log` sink no longer prints every `trace!` in
    /// every crate. The publish path's `log::trace!` is the record that wrote
    /// a flushed stdout line per publish; at the default threshold it is
    /// refused, while `info!` still gets through.
    #[test]
    fn stdout_log_bridge_filters_below_the_default_threshold() {
        install_stdout_log_bridge();
        let meta = |level| {
            log::Metadata::builder()
                .level(level)
                .target("nros_rmw_zenoh::shim::publisher")
                .build()
        };
        assert!(!log::logger().enabled(&meta(log::Level::Trace)));
        assert!(!log::logger().enabled(&meta(log::Level::Debug)));
        assert!(log::logger().enabled(&meta(log::Level::Info)));
        // The ceiling stays at Trace, so lowering the threshold later works.
        assert_eq!(log::max_level(), log::LevelFilter::Trace);
    }

    /// Issue 1988: the bare level is the default threshold `RUST_LOG`
    /// states; per-target directives and junk state none. Pure, so it does not
    /// move the process default the test above reads.
    #[test]
    fn rust_log_states_a_default_level_only_through_a_bare_level() {
        use nros_log::Severity;
        assert_eq!(rust_log_default_level("trace"), Some(Severity::Trace));
        assert_eq!(rust_log_default_level(" DEBUG "), Some(Severity::Debug));
        assert_eq!(
            rust_log_default_level("info,nros_rmw_zenoh=trace"),
            Some(Severity::Info)
        );
        assert_eq!(rust_log_default_level("warn,error"), Some(Severity::Error));
        assert_eq!(rust_log_default_level("off"), Some(Severity::Fatal));
        assert_eq!(
            rust_log_default_level("debug/publish"),
            Some(Severity::Debug)
        );
        assert_eq!(rust_log_default_level("nros_rmw_zenoh=trace"), None);
        assert_eq!(rust_log_default_level("nros_rmw_zenoh"), None);
        assert_eq!(rust_log_default_level(""), None);
        assert_eq!(rust_log_default_level("verbose"), None);
    }

    #[test]
    fn println_writes_without_panicking() {
        <LinuxBoard as BoardPrint>::println(format_args!("nros-board-linux: hello from unit test"));
    }

    // Note: `BoardEntry::run` itself can't be unit-tested directly
    // because both exit branches diverge (`-> !`) via
    // `std::process::exit`, which would kill the test process. The
    // doc comment on `BoardEntry::run` explicitly preserves the
    // `-> Result` shape on the *callback* path so production boards
    // can still wrap the trait in a non-diverging test harness; that
    // harness lives outside this crate.
    //
    // What IS unit-testable is the value both funnels hand the resolver, which
    // is where issue 1442's hosted half lived. `hosted_baked_rung` is that
    // value, and it is now the only place either funnel computes it — so these
    // tests bind BOTH `boot_hosted` and `run_tiers` by construction rather
    // than by two copies of one assertion.

    static BAKED_BOTH: ::nros::BakedBootConfig =
        ::nros::BakedBootConfig::new(Some("frombake"), None, None, Some("/island"));
    static BAKED_NO_NS: ::nros::BakedBootConfig =
        ::nros::BakedBootConfig::new(Some("frombake"), None, None, None);

    fn overlay(
        node_name: Option<&'static str>,
        boot_config: Option<&'static ::nros::BakedBootConfig>,
    ) -> nros_platform::DeployOverlay {
        nros_platform::DeployOverlay {
            node_name,
            boot_config,
            ..Default::default()
        }
    }

    /// Issue 1442, the NAMESPACE half (1434's carve-out): the baked rung
    /// carries the launch-declared namespace, read from the BLOB — the field a
    /// post-link patcher of `.nros_boot_config` rewrites.
    #[test]
    fn the_baked_rung_carries_the_blobs_namespace() {
        let d = overlay(None, Some(&BAKED_BOTH));
        assert_eq!(
            hosted_baked_rung(&d).namespace,
            Some("/island"),
            "the namespace must reach the resolver as the baked rung"
        );
    }

    /// The negative direction, which matters as much: a blob with
    /// `BOOT_SET_NAMESPACE` clear leaves the next rung speaking. ABSENT, never
    /// `Some("")` — the two resolve to the same string, so nothing downstream
    /// could tell them apart and only the `Option` can be asserted on.
    #[test]
    fn an_unbaked_namespace_stays_absent_in_the_baked_rung() {
        for (what, d) in [
            ("no namespace bit", overlay(None, Some(&BAKED_NO_NS))),
            ("no blob at all", overlay(None, None)),
        ] {
            assert_eq!(
                hosted_baked_rung(&d).namespace,
                None,
                "{what}: an unbaked namespace must fall through, not deliver \"\""
            );
        }
    }

    /// Issue 1442, the `node_name` half — a SEPARATE defect with its own
    /// history (issue #48, "kept for signature parity"). The name comes from
    /// `DeployOverlay::node_name`, the rung whose own doc-comment says it "IS
    /// honored on hosted boards"; `run_tiers` was the one funnel where that
    /// was false.
    #[test]
    fn the_baked_rung_carries_the_overlays_node_name() {
        let d = overlay(Some("talker"), Some(&BAKED_BOTH));
        assert_eq!(
            hosted_baked_rung(&d).node_name,
            Some("talker"),
            "the overlay's node name must reach the resolver as the baked rung"
        );
        // And NOT the blob's copy: on hosted the macro writes the
        // launch-declared name into the overlay field, so reading the blob
        // instead would be a second answer to one question. `BAKED_BOTH` bakes
        // `frombake` precisely so the two can be told apart.
        assert_eq!(
            hosted_baked_rung(&overlay(None, Some(&BAKED_BOTH))).node_name,
            None
        );
    }

    /// Issue #48 is PRESERVED: the connect facts stay absent, so a HOST keeps
    /// resolving them from the environment. Identity is not a connect fact,
    /// which is the whole distinction issues 1434 and 1442 rest on — if this
    /// ever starts returning a locator, `$NROS_LOCATOR` stops being
    /// authoritative for every hosted image.
    #[test]
    fn the_baked_rung_names_no_connect_facts() {
        let d = overlay(Some("talker"), Some(&BAKED_BOTH));
        let rung = hosted_baked_rung(&d);
        assert_eq!(
            rung.locator, None,
            "locator must stay env-driven (issue #48)"
        );
        assert_eq!(
            rung.domain_id, None,
            "domain must stay env-driven (issue #48)"
        );
    }
}

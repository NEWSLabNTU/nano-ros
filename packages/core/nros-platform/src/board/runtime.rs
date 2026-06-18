//! [`RuntimeCtx`] — Phase 212.N.1.
//!
//! Per-invocation runtime context handed to `BoardEntry::run`'s
//! `setup` callback. Carries the overlay knobs the codegen
//! `run_plan(runtime)` body reads:
//!
//! - **params** — `(key, value)` pairs from launch XML
//!   `<param name="…" value="…"/>` or `--ros-args -p k:=v`.
//! - **remaps** — `(from, to)` topic/service renames.
//! - **env** — environment-style key/value pairs (POSIX `getenv`
//!   shape) accessible from no_std boards via this struct rather
//!   than a `libc::getenv` call.
//! - **runtime** — `&mut dyn NodeDispatchRuntime` sink the
//!   codegen-emitted `run_plan(runtime)` body forwards each Node
//!   pkg's `register(runtime)` call into (Phase 212.N.7 step-3.2).
//!   Populated by each `BoardEntry::run` impl after opening its
//!   executor; defaults to a no-op sink when constructed via
//!   [`RuntimeCtx::with_runtime`].
//!
//! ## no_std-safe shape
//!
//! Slice-of-tuples kept on the boot stack. No allocation, no
//! `core::collections`. Codegen owns the storage and passes a
//! `&mut RuntimeCtx<'_>` whose backing slices live in `static`s.
//!
//! Hosted boards (POSIX) may instead build a longer-lived owned
//! variant on the heap; the trait surface is slice-based so
//! both shapes work.

use super::dispatch::DispatchStrategy;

/// Layer-clean substitute for `nros::node_metadata::CallbackId` +
/// `nros::node::CallbackCtx` at the [`NodeDispatchRuntime`] boundary
/// (Phase 216.A.2). `nros-platform` sits below `nros` in the dep
/// graph, so the trait surface cannot reference those types
/// directly. The `nros`-side runtime impl wraps a real
/// `(CallbackId, &mut CallbackCtx)` pair into this opaque shape
/// before invoking [`NodeDispatchRuntime::signal_callback`]; the
/// concrete dispatcher casts `ctx_ptr` back to
/// `&mut nros::CallbackCtx<'_>` at the call site.
///
/// `#[repr(C)]` keeps the layout stable across the (same-language
/// today, FFI-shaped tomorrow) `nros-platform` ↔ `nros` boundary.
#[repr(C)]
pub struct SignaledCallback<'a> {
    /// Stable identifier string carried by `nros::CallbackId(&'a str)`.
    pub cb_id: &'a str,

    /// Erased pointer to the `nros::CallbackCtx<'_>` the dispatcher
    /// will drive. The `nros`-side `NodeDispatchRuntime` impl casts
    /// back to `&mut nros::CallbackCtx<'_>` before invoking the
    /// component body.
    pub ctx_ptr: *mut core::ffi::c_void,
}

// Phase 258 (Track 2, w5) — the opaque per-Node `extern "Rust" fn()` aliases
// (`NodeRegisterFn` / `NodeInitFn` / `NodeDispatchFn` / `NodeTickFn`) are gone.
// They anchored the retired `register_dispatch_slot_dyn` four-fn-ptr bridge
// (owned-spin declarative register), which the install seam replaced — Rust
// owned-spin now registers via `RuntimeCtx::runtime.executor_handle()` +
// `nros::install_node_typed` like the C/C++ typed entries.

/// Node runtime sink the codegen-emitted `run_plan(runtime)`
/// body talks to (Phase 212.N.7 step-3.1).
///
/// Object-safe + `no_std`. The concrete impl
/// (`ExecutorNodeRuntime` in `nros`) owns the live executor;
/// `BoardEntry::run` installs it on the per-boot
/// [`RuntimeCtx::runtime`] slot before invoking the user `setup`
/// closure.
///
/// Phase 214.K.1 — renamed from `NodeRuntime` to disambiguate from
/// the user-facing `nros::NodeRuntime` metadata-sink trait in
/// `packages/core/nros/src/node.rs:112`. The two traits live at
/// different layers (board-side dispatch sink vs user-side metadata
/// declaration sink) and the previous shared name forced explicit
/// `nros_platform::` / `nros::` qualification at every use site +
/// produced confusing `impl NodeRuntime for X` ambiguity. A
/// `#[deprecated]` `pub use NodeDispatchRuntime as NodeRuntime;`
/// re-export sits at the crate module level for one release cycle.
pub trait NodeDispatchRuntime {
    /// Drive the underlying executor for at most `timeout_ms`
    /// milliseconds. `Ok(())` on a clean spin (including timeout);
    /// `Err(())` if the executor surfaces a spin error.
    ///
    /// `Result<_, ()>` is deliberate: the board entry-point callers
    /// (`nros-board-{freertos,nuttx,threadx}` spin loops) only
    /// `{:?}`-print the error and `B::exit_failure()` — a typed enum
    /// would carry no extra info across the trait boundary, since the
    /// underlying `ExecutorError` from `nros::node_runtime` is mapped
    /// to `()` at the impl site (`impl NodeDispatchRuntime for
    /// ExecutorNodeRuntime`). `#[allow]` keeps the surface narrow.
    #[allow(clippy::result_unit_err)]
    fn spin_once(&mut self, timeout_ms: u32) -> Result<(), ()>;

    /// Phase 258 (Track 2, 2a) — raw `*mut Executor` (as `void*`) for the
    /// owned-spin entry, so a Node pkg's `register(runtime)` wrapper can call
    /// the uniform `__nros_component_<pkg>_install(.., executor, ..)` seam
    /// (`nros::install_node_typed`) instead of the retired opaque-fn-ptr
    /// `register_dispatch_slot_dyn` bridge. A pointer crosses the
    /// `nros-platform` → `nros` layering wall cleanly (the concrete
    /// `ExecutorNodeRuntime` lives in `nros`; this trait can't name it).
    ///
    /// Default `null` — sinks without a live executor (e.g.
    /// [`NullNodeRuntime`], framework-dispatch-only runtimes) report no
    /// handle; the install path treats null as a registration error.
    fn executor_handle(&mut self) -> *mut core::ffi::c_void {
        core::ptr::null_mut()
    }

    /// Observability counters from hosted/runtime tests.
    ///
    /// Returns `(all_callbacks, message_callbacks)`. Implementations
    /// that cannot observe callback dispatch keep the default zeros.
    fn observed_callback_counts(&self) -> (usize, usize) {
        (0, 0)
    }

    /// Hand a signaled callback to the framework-side dispatcher
    /// (Phase 216.A.2). Only meaningful for `DispatchStrategy::Deferred`
    /// (RTIC / Embassy) runtimes — `Inline` runtimes drive callbacks
    /// directly from `spin_once` and never call this. The default panic
    /// surfaces the mis-wire loudly rather than silently dropping the
    /// callback signal.
    fn signal_callback(&mut self, _cb: SignaledCallback<'_>) {
        panic!("signal_callback not implemented for Inline runtime");
    }

    /// Declare how this runtime delivers callbacks (Phase 216.A.2).
    /// `nros check` (Phase 216.D.1) cross-validates each Node pkg's
    /// `Node::DISPATCH` against this value. Defaults to `Inline` so
    /// every existing impl reports the historical behavior unchanged.
    fn dispatch_strategy(&self) -> DispatchStrategy {
        DispatchStrategy::Inline
    }
}

/// No-op [`NodeDispatchRuntime`] for tests / placeholders. Every call
/// returns `Err(())` so callers that depend on a populated runtime
/// fail loud rather than silently no-op.
///
/// `BoardEntry::run` impls replace this with a real
/// `ExecutorNodeRuntime`-backed sink before invoking the user
/// `setup` closure.
#[derive(Debug, Default)]
pub struct NullNodeRuntime;

impl NodeDispatchRuntime for NullNodeRuntime {
    fn spin_once(&mut self, _timeout_ms: u32) -> Result<(), ()> {
        Err(())
    }
}

/// Runtime context handed to `BoardEntry::run(setup)`.
///
/// All three overlay slices may be empty. A board's launch overlay
/// typically populates `params` + `remaps`; `env` is rarely set on
/// embedded.
pub struct RuntimeCtx<'a> {
    /// `<param name=… value=…/>` from launch XML, or
    /// `-p name:=value` CLI overrides.
    pub params: &'a [(&'a str, &'a str)],

    /// Topic / service / action remaps: `(from, to)`.
    pub remaps: &'a [(&'a str, &'a str)],

    /// Environment-style key/value pairs (mostly POSIX). Empty on
    /// embedded boards.
    pub env: &'a [(&'a str, &'a str)],

    /// Node runtime sink. `BoardEntry::run` populates this with
    /// the live `ExecutorNodeRuntime`-backed impl before invoking
    /// the user `setup` closure. The codegen-emitted
    /// `run_plan(runtime)` body calls `<pkg>::register(runtime)` once per Node
    /// pkg, which installs through `runtime.executor_handle()` +
    /// `nros::install_node_typed` (Phase 258, Track 2).
    ///
    /// Defaults to a [`NullNodeRuntime`] when the context is
    /// built via [`RuntimeCtx::with_runtime`]. That sink errors
    /// every call so test fixtures that forget to wire a real runtime
    /// fail loud.
    pub runtime: &'a mut dyn NodeDispatchRuntime,
}

impl core::fmt::Debug for RuntimeCtx<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RuntimeCtx")
            .field("params", &self.params)
            .field("remaps", &self.remaps)
            .field("env", &self.env)
            .field("runtime", &"<dyn NodeDispatchRuntime>")
            .finish()
    }
}

impl<'a> RuntimeCtx<'a> {
    /// Build a [`RuntimeCtx`] with no params / remaps / env and the
    /// given runtime sink. The common shape `BoardEntry::run`
    /// constructs after opening its executor.
    ///
    /// For test fixtures that don't need a populated runtime, pass a
    /// `&mut NullNodeRuntime` — every call against the sink
    /// returns `Err(())`, surfacing the missing wiring.
    pub fn with_runtime(runtime: &'a mut dyn NodeDispatchRuntime) -> Self {
        Self {
            params: &[],
            remaps: &[],
            env: &[],
            runtime,
        }
    }

    /// Build a [`RuntimeCtx`] with explicit overlay slices + runtime
    /// sink (Phase 212.N.7 step-3.2).
    pub fn new(
        runtime: &'a mut dyn NodeDispatchRuntime,
        params: &'a [(&'a str, &'a str)],
        remaps: &'a [(&'a str, &'a str)],
        env: &'a [(&'a str, &'a str)],
    ) -> Self {
        Self {
            params,
            remaps,
            env,
            runtime,
        }
    }

    /// Lookup a param by name; first match wins. Linear scan
    /// because the slice is typically small (≤ a dozen entries).
    pub fn param(&self, name: &str) -> Option<&'a str> {
        self.params
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| *v)
    }

    /// Lookup a remap by the original (`from`) name; returns the
    /// rewritten name when remapped, else `None`.
    pub fn remap(&self, from: &str) -> Option<&'a str> {
        self.remaps
            .iter()
            .find(|(k, _)| *k == from)
            .map(|(_, v)| *v)
    }

    /// Lookup an env entry by name.
    pub fn env_var(&self, name: &str) -> Option<&'a str> {
        self.env.iter().find(|(k, _)| *k == name).map(|(_, v)| *v)
    }
}

/// Error returned by the codegen-emitted `run_plan(runtime)` body
/// (Phase 212.N.4) and by Node pkg `register(runtime)` wrappers
/// (Phase 212.N.7 step-2).
///
/// `no_std`-safe — variants are string-typed so embedded Entry pkgs
/// don't need to pull `thiserror`/`anyhow` to print. The
/// out-of-tree `nros-build` codegen library re-exports this type so
/// emitted code references `::nros_platform::RuntimeError`, NOT
/// `::nros_build::RuntimeError` — the embedded Entry pkg's runtime
/// path then doesn't need `nros-build` as a runtime dep (build-dep
/// only).
#[derive(Debug)]
#[non_exhaustive]
pub enum RuntimeError {
    /// A node's `register(runtime)` call failed. The string carries the
    /// node pkg name.
    ///
    /// Phase 212.N.12 hard-renamed the legacy `ComponentRegister` variant
    /// to `NodeRegister` to match the rclcpp_components / ROS 2 launch.xml
    /// `<node pkg=…>` convention.
    NodeRegister(&'static str),

    /// The hosted Entry spin loop failed or did not observe the
    /// requested runtime condition before its bounded test deadline.
    Spin,
}

impl core::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NodeRegister(msg) => write!(f, "node register failed: {msg}"),
            Self::Spin => write!(f, "entry spin failed"),
        }
    }
}

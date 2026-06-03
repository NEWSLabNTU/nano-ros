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
//! - **runtime** — `&mut dyn ComponentRuntime` sink the
//!   codegen-emitted `run_plan(runtime)` body forwards each Component
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

/// Per-Component pkg `register` fn pointer (Phase 212.N.7 step-3.1).
///
/// Real signature lives in `nros::ComponentRegisterFn` (see
/// `packages/core/nros/src/component_runtime.rs`). The platform
/// layer treats it as an opaque pointer so `nros-platform` does not
/// need to depend on `nros` (that would invert the dep graph). The
/// `ExecutorComponentRuntime` impl in `nros` `transmute`s back to
/// the typed signature at the FFI boundary.
///
/// `extern "Rust" fn()` is the smallest concrete `fn` type
/// (zero-arg, no return). Coercing the real typed fn pointer to
/// this anchor requires `core::mem::transmute` — non-`as`. The
/// macro emit (Phase 212.N.7 step-3.4) carries the transmute so
/// individual Component pkgs never spell it.
pub type ComponentRegisterFn = extern "Rust" fn();

/// Per-Component pkg `init` fn pointer (Phase 212.N.7 step-3.1).
///
/// See [`ComponentRegisterFn`] for the opaque-pointer rationale.
pub type ComponentInitFn = extern "Rust" fn();

/// Per-Component pkg `dispatch` fn pointer (Phase 212.N.7 step-3.1).
///
/// See [`ComponentRegisterFn`] for the opaque-pointer rationale.
pub type ComponentDispatchFn = extern "Rust" fn();

/// Per-Component pkg `tick` fn pointer (Phase 212.N.7 step-3.1).
///
/// See [`ComponentRegisterFn`] for the opaque-pointer rationale.
pub type ComponentTickFn = extern "Rust" fn();

/// Component runtime sink the codegen-emitted `run_plan(runtime)`
/// body talks to (Phase 212.N.7 step-3.1).
///
/// Object-safe + `no_std`. The concrete impl
/// (`ExecutorComponentRuntime` in `nros`) owns the live executor;
/// `BoardEntry::run` installs it on the per-boot
/// [`RuntimeCtx::runtime`] slot before invoking the user `setup`
/// closure.
///
/// The fn-pointer parameters are the opaque
/// [`ComponentRegisterFn`] / [`ComponentInitFn`] /
/// [`ComponentDispatchFn`] / [`ComponentTickFn`] aliases — the
/// real-typed counterparts live in `nros`. The implementor
/// `mem::transmute`s back at the call site (see
/// `impl nros_platform::ComponentRuntime for ExecutorComponentRuntime`
/// in `packages/core/nros/src/component_runtime.rs`).
pub trait ComponentRuntime {
    /// Register a single Component pkg by its four `extern "Rust"` fn
    /// pointers + a static name for diagnostics. Returns `Err(())`
    /// when the executor rejects the registration (no detail surfaces
    /// across the trait — Component pkgs map this back to
    /// [`RuntimeError::ComponentRegister`] with the pkg name).
    fn register_dispatch_slot_dyn(
        &mut self,
        register: ComponentRegisterFn,
        init: ComponentInitFn,
        dispatch: ComponentDispatchFn,
        tick: ComponentTickFn,
        name: &'static str,
    ) -> Result<(), ()>;

    /// Drive the underlying executor for at most `timeout_ms`
    /// milliseconds. `Ok(())` on a clean spin (including timeout);
    /// `Err(())` if the executor surfaces a spin error.
    fn spin_once(&mut self, timeout_ms: u32) -> Result<(), ()>;
}

/// No-op [`ComponentRuntime`] for tests / placeholders. Every call
/// returns `Err(())` so callers that depend on a populated runtime
/// fail loud rather than silently no-op.
///
/// `BoardEntry::run` impls replace this with a real
/// `ExecutorComponentRuntime`-backed sink before invoking the user
/// `setup` closure.
#[derive(Debug, Default)]
pub struct NullComponentRuntime;

impl ComponentRuntime for NullComponentRuntime {
    fn register_dispatch_slot_dyn(
        &mut self,
        _register: ComponentRegisterFn,
        _init: ComponentInitFn,
        _dispatch: ComponentDispatchFn,
        _tick: ComponentTickFn,
        _name: &'static str,
    ) -> Result<(), ()> {
        Err(())
    }

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

    /// Component runtime sink. `BoardEntry::run` populates this with
    /// the live `ExecutorComponentRuntime`-backed impl before invoking
    /// the user `setup` closure. The codegen-emitted
    /// `run_plan(runtime)` body calls
    /// `runtime.runtime.register_dispatch_slot_dyn(...)` once per
    /// Component pkg.
    ///
    /// Defaults to a [`NullComponentRuntime`] when the context is
    /// built via [`RuntimeCtx::with_runtime`]. That sink errors
    /// every call so test fixtures that forget to wire a real runtime
    /// fail loud.
    pub runtime: &'a mut dyn ComponentRuntime,
}

impl core::fmt::Debug for RuntimeCtx<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RuntimeCtx")
            .field("params", &self.params)
            .field("remaps", &self.remaps)
            .field("env", &self.env)
            .field("runtime", &"<dyn ComponentRuntime>")
            .finish()
    }
}

impl<'a> RuntimeCtx<'a> {
    /// Build a [`RuntimeCtx`] with no params / remaps / env and the
    /// given runtime sink. The common shape `BoardEntry::run`
    /// constructs after opening its executor.
    ///
    /// For test fixtures that don't need a populated runtime, pass a
    /// `&mut NullComponentRuntime` — every call against the sink
    /// returns `Err(())`, surfacing the missing wiring.
    pub fn with_runtime(runtime: &'a mut dyn ComponentRuntime) -> Self {
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
        runtime: &'a mut dyn ComponentRuntime,
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
/// (Phase 212.N.4) and by Component pkg `register(runtime)` wrappers
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
    /// Phase 212.N.12 renamed `ComponentRegister` → `NodeRegister` to
    /// match the rclcpp_components / ROS 2 launch.xml `<node pkg=…>`
    /// convention. The old variant stays as a deprecated alias for one
    /// release.
    NodeRegister(&'static str),
}

impl RuntimeError {
    /// Deprecated alias for [`Self::NodeRegister`] (Phase 212.N.12).
    /// Constructs the renamed variant; old hand-written `match`
    /// arms that read `RuntimeError::ComponentRegister(_)` keep
    /// compiling as long as they match `NodeRegister` instead.
    #[deprecated(
        since = "212.N.12",
        note = "renamed to `RuntimeError::NodeRegister`; remove in a future release"
    )]
    #[allow(non_snake_case)]
    pub const fn ComponentRegister(msg: &'static str) -> Self {
        Self::NodeRegister(msg)
    }
}

impl core::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NodeRegister(msg) => write!(f, "node register failed: {msg}"),
        }
    }
}

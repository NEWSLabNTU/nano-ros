//! Phase 212.L.5 — top-level init API; phase-427 W9 — on every target.
//!
//! [`Context`] is WHERE this image is connected — locator, domain, RMW,
//! session mode, and the source those came from (RFC-0089 "Context and
//! `init`, settled"). One per process or image; a resolved value, not an
//! entity on the graph. A `Node` is a named participant with its own
//! entities, and one image holds several of those on one `Context`
//! (RFC-0047).
//!
//! ## Where the values come from
//!
//! * **Hosted** (feature `env`): the process environment (`ROS_DOMAIN_ID`,
//!   `NROS_LOCATOR`, `NROS_SESSION_MODE`, `NROS_RMW` / `RMW_IMPLEMENTATION`)
//!   through the one hosted resolver, `crate::env::try_resolve_hosted`. A
//!   launcher projects per-node values into that environment before exec.
//! * **Freestanding** (no `env`): there is no process and no environment; the
//!   same values are BAKED at compile time — `option_env!("NROS_LOCATOR")` and
//!   `option_env!("NROS_DOMAIN_ID")`, exported into this crate's build by
//!   `nros_zephyr_build::bake_nros_config()` from Kconfig (`build.rs`), or
//!   present in the process environment of the build for the other RTOS lanes
//!   (the board crates read the same two names the same way). The semantics
//!   survive with the source moved from run time to build time: "the
//!   environment this image was built for or launched in".
//!
//! The `env` feature gates ONLY the process-environment reader and the
//! argv/launch entry points; `Context`, [`ContextSource`], [`InitError`],
//! [`InitOptions`] and [`Context::config`] compile on every target. The floor
//! for `Context` itself is `alloc`: `locator` and `rmw` are owned `String`s
//! so the value outlives whatever transient produced it (an env cache, a
//! parsed launch file, a harness-supplied override).
//!
//! ## The rclrs family
//!
//! [`Context::default_from_env`], [`Context::from_env`] and (hosted only)
//! [`Context::new`] are the `rclrs::Context` constructors under the same
//! names; [`InitOptions`] is rclrs's, with the one option it has. [`init()`] is
//! the C++-symmetric anchor (`rclcpp::init`) and equals `default_from_env()`.
//!
//! Three patterns are supported (per the Phase 212.L canonical pkg shape):
//!
//! 1. **Node pkg** — register via the [`nros::node!`](crate::node!)
//!    macro (Phase 172 W.3); the generated runtime owns the spin loop and
//!    builds its `Context` through [`Context::default_from_env`].
//! 2. **Application pkg + launch-aware** — call [`init_with_launch_auto`] (or
//!    [`init_with_launch`] for an explicit path). The returned [`Context`]
//!    carries launch-resolved fields (domain id, locator, RMW choice). User
//!    code drives its own spin via `Executor::open` +
//!    `Executor::spin`.
//! 3. **Application pkg + custom spin** — call [`init()`] /
//!    [`Context::default_from_env`] (or [`init_with_args`] / [`Context::new`]
//!    for argv-style entry). Launch file is ignored; env vars +
//!    `ExecutorConfig::from_env()` semantics still apply.
//!
//! To actually open a session, call [`Context::create_executor`] (`alloc`) or
//! [`Context::create_executor_in`] (caller-supplied backing, nothing leaked),
//! then name each node with `Executor::create_node`. The older spelling —
//! materialise an [`crate::ExecutorConfig`] via [`Context::config`] and pass it
//! to `Executor::open` — still works and is what a single-node entry wants;
//! phase-427 W10 added the pair because it cannot express the second node.
//!
//! ## Launch overlay (current limitation)
//!
//! `init_with_launch_auto` / `init_with_launch` currently consume the
//! launch-resolved knobs the parent `nros launch` process exports via env
//! vars (`ROS_DOMAIN_ID`, `NROS_LOCATOR`, `NROS_SESSION_MODE`,
//! `RMW_IMPLEMENTATION`, plus the placeholder `NROS_RUNTIME_OVERLAY` for
//! the future structured overlay path). The launch XML is NOT parsed
//! in-process; the runtime trusts the launcher to project the relevant
//! params / remaps / env into the child environment. A follow-up wave wires
//! the structured overlay (Option A — `nros launch --emit-runtime-overlay`
//! → JSON sidecar consumed here). See Phase 212.L.5 notes.

#[cfg(feature = "env")]
// phase-359 W10 — kept, and it is not a spelling that can be unwound.
// `init_with_launch` verifies a launch file EXISTS, which is a filesystem
// question; `AsRef<Path>` is also what a Rust caller expects to pass a
// `PathBuf`, a `&str` or a `Path` to. Narrowing the signature to `&str` would
// trade a real ergonomic for one census point, which is moving the number
// rather than the build. `init_with_launch` is behind `env`, which requires
// `std`, so nothing here is reachable without one.
use std::path::Path;

#[cfg(all(feature = "alloc", feature = "rmw-cffi"))]
use core::mem::MaybeUninit;

#[cfg(feature = "alloc")]
use nros_node::DOMAIN_ID_MAX;
#[cfg(feature = "alloc")]
use nros_node::ExecutorConfig;
// phase-427 W10 — the open failure a `create_executor*` call can carry back.
// Ungated: `NodeError` lives in `nros_node::executor::types`, which is compiled
// on every target, and [`InitError`] is on every target too (W9).
use nros_node::NodeError;
#[cfg(all(feature = "alloc", feature = "rmw-cffi"))]
use nros_node::{Executor, ExecutorSizing};
#[cfg(feature = "alloc")]
use nros_rmw::SessionMode;

/// Errors returned by the init API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InitError {
    /// `init_with_launch(path)` was passed a path that does not exist or
    /// could not be read.
    LaunchFileNotFound,
    /// The launch file existed but could not be parsed.
    ///
    /// Phase 212.L.5 ships a stub — actual XML parsing arrives with the
    /// runtime-overlay wave. Until then this variant is unused.
    LaunchParseFailed,
    /// A launch-derived env var (`ROS_DOMAIN_ID`, etc.) failed to parse —
    /// or, freestanding, the baked `NROS_DOMAIN_ID` is not a decimal integer.
    EnvParseFailed,
    /// A domain id — from the environment, the bake, or an
    /// [`InitOptions::with_domain_id`] override — exceeds
    /// [`crate::DOMAIN_ID_MAX`]. Rejected here rather than
    /// handed to a backend that can only fail later (the #206 rule).
    DomainIdOutOfRange,
    /// phase-427 W10 — [`Context::create_executor`] /
    /// [`Context::create_executor_in`] resolved the identity and the RMW
    /// session open then failed.
    ///
    /// The backend's own [`NodeError`] is CARRIED, not flattened into a flag:
    /// `Transport(InvalidConfig)` (no backend registered, or more than one and
    /// no selector) and `Transport(ConnectionFailed)` (the router is not there)
    /// are different problems with different fixes, and issue 0465 is what
    /// collapsing them cost the last time.
    ExecutorOpenFailed(NodeError),
    /// phase-427 W10 — [`Context::create_executor_in`] was handed a backing
    /// smaller than the executor's tables need.
    ///
    /// Refused rather than accepted: `Executor::open_in` is `unsafe` precisely
    /// because a short backing is undefined behaviour, so the safe wrapper
    /// checks the length and names both numbers. `needed` is
    /// `ExecutorSizing::DEFAULT.u64_len()` for this build — it moves with
    /// `NROS_EXECUTOR_MAX_CBS` / `_MAX_SC` / `_ARENA_SIZE` / `_MAX_NODES`, so a
    /// caller sizing a `static` from a literal will hear about it here rather
    /// than corrupting memory later.
    BackingTooSmall {
        /// `u64` words the executor's tables need.
        needed: usize,
        /// `u64` words the caller supplied.
        given: usize,
    },
}

impl core::fmt::Display for InitError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            InitError::LaunchFileNotFound => f.write_str("launch file not found"),
            InitError::LaunchParseFailed => f.write_str("launch file parse failed"),
            InitError::EnvParseFailed => f.write_str("env var parse failed"),
            InitError::DomainIdOutOfRange => f.write_str("domain id out of range"),
            InitError::ExecutorOpenFailed(e) => {
                write!(f, "RMW session open failed: {e:?}")
            }
            InitError::BackingTooSmall { needed, given } => write!(
                f,
                "executor backing too small: {given} u64 words supplied, {needed} needed"
            ),
        }
    }
}

// `core::error::Error` — `std::error::Error` is a re-export of it since Rust
// 1.81, so this is the same trait, and it needs no feature: phase-427 W9 took
// this module out from behind `env`.
impl core::error::Error for InitError {}

/// Phase 212.L.5 — resolved init context.
///
/// Returned by every `init*` entry point and every `Context::*` constructor.
/// Carries the fields the user needs to construct an [`ExecutorConfig`] and
/// open a session.
///
/// Fields are owned (`String`) so the `Context` can outlive transient parents
/// (env caches, parsed launch files, a harness-supplied locator). That is why
/// the type's feature floor is `alloc`.
///
/// ADOPT-BOUNDED against `rclrs::Context`, which OWNS the rcl context. This is
/// the resolved identity as a plain value, so:
/// - There is no `ok()`: the value has no shutdown state; liveness is the
///   executor's.
/// - `domain_id` is a public field, not a `domain_id()` method.
/// - Executors are opened FROM it: [`create_executor`](Self::create_executor)
///   leaks an executor-lifetime backing (needs `alloc`), and
///   [`create_executor_in`](Self::create_executor_in) takes caller storage.
#[cfg(feature = "alloc")]
#[derive(Debug, Clone)]
pub struct Context {
    /// ROS 2 domain ID (`ROS_DOMAIN_ID`, default 0; baked `NROS_DOMAIN_ID`
    /// freestanding).
    pub domain_id: u32,
    /// Middleware locator (`NROS_LOCATOR` / legacy `ZENOH_LOCATOR`; baked
    /// `NROS_LOCATOR` freestanding). Empty means ABSENT: `nros` is
    /// RMW-agnostic, so the linked backend substitutes its own default
    /// (issue 0330).
    pub locator: alloc::string::String,
    /// Session mode (`NROS_SESSION_MODE` / legacy `ZENOH_MODE`, default `Client`).
    pub mode: SessionMode,
    /// RMW implementation hint (`RMW_IMPLEMENTATION` /  `NROS_RMW`).
    ///
    /// Empty when neither var is set, and always empty freestanding (an image
    /// links exactly one backend). The runtime uses this to pick a primary
    /// backend when multiple are linked; see `crate::internals::open_session`.
    pub rmw: alloc::string::String,
    /// Source of this context — useful for diagnostics + tests.
    pub source: ContextSource,
    /// The `-r`/`--remap` rules `init_with_args` / `Context::new` parsed
    /// out of `--ros-args`, in argv order. Installed as the FALLBACK remap
    /// tier of every executor this context creates
    /// (`Context::create_executor`, `Context::create_executor_in`).
    ///
    /// rcl keeps its global arguments on the context for the same reason: the
    /// context is what the process was started with, and every node created
    /// from it inherits them. Private, because nothing outside the parse
    /// should write a rule here — and a rule that bypassed the parse would
    /// bypass its refusals too.
    #[cfg_attr(not(feature = "env"), allow(dead_code))]
    ros_args: alloc::vec::Vec<ArgvRemap>,
}

/// One `-r [node:]from:=to` rule, owned so a [`Context`] can outlive the
/// argument vector it came from.
#[cfg(feature = "alloc")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(feature = "env"), allow(dead_code))]
struct ArgvRemap {
    node: Option<alloc::string::String>,
    from: alloc::string::String,
    to: alloc::string::String,
}

/// Where the [`Context`] came from. Diagnostics only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextSource {
    /// Built from env vars by [`init()`] / [`init_with_args`] /
    /// [`Context::new`], or by [`Context::default_from_env`] /
    /// [`Context::from_env`] on a hosted build.
    Env,
    /// Built from a launch file (path supplied to [`init_with_launch`]) or
    /// auto-discovered via [`init_with_launch_auto`]. The launch XML itself
    /// is NOT yet parsed (see module docs); the launcher's projected env
    /// is the source of truth for now.
    Launch,
    /// phase-427 W9 — built from the constants baked into this crate at
    /// compile time (`NROS_LOCATOR`, `NROS_DOMAIN_ID`) by [`Context::baked`],
    /// which is what [`Context::default_from_env`] / [`Context::from_env`]
    /// resolve to on a freestanding build. The entry macros
    /// (`nros::main!`'s Zephyr arm, `nros::zephyr_component_main!`) produce
    /// this one.
    Baked,
}

/// phase-427 W9 — `rclrs::InitOptions`, with the one option it has.
///
/// Consumed by [`Context::from_env`] and [`Context::new`]; `None` for the
/// domain keeps whatever the environment (hosted) or the bake (freestanding)
/// resolved.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InitOptions {
    domain_id: Option<u32>,
}

impl InitOptions {
    /// Default options: nothing overridden.
    pub const fn new() -> Self {
        Self { domain_id: None }
    }

    /// Override the domain id (`Some`) or keep the resolved one (`None`).
    ///
    /// `usize` because that is rclrs's signature. A value above `u32::MAX`
    /// cannot be a domain id at all; it saturates here and the constructor
    /// that consumes the options then rejects it as
    /// [`InitError::DomainIdOutOfRange`], which is the loud place — a builder
    /// method has no `Result` to carry the refusal in.
    pub const fn with_domain_id(mut self, domain_id: Option<usize>) -> Self {
        self.domain_id = match domain_id {
            Some(d) if d > u32::MAX as usize => Some(u32::MAX),
            Some(d) => Some(d as u32),
            None => None,
        };
        self
    }

    /// The in-place twin of [`with_domain_id`](Self::with_domain_id) — rclrs
    /// has both, and a ported `opts.set_domain_id(Some(7))` compiles as
    /// written. Same saturation.
    pub fn set_domain_id(&mut self, domain_id: Option<usize>) {
        *self = self.with_domain_id(domain_id);
    }

    /// The domain override, if any.
    ///
    /// `Option<usize>`, which is rclrs's signature and the same reason
    /// [`with_domain_id`](Self::with_domain_id) takes one. phase-467: this
    /// returned `Option<u32>` — the STORED width — so the two setters spoke
    /// rclrs and the getter spoke the field, and a ported
    /// `let d: Option<usize> = opts.domain_id();` failed on a type nobody
    /// chose. The value is still stored as a `u32` and `with_domain_id`'s
    /// saturation still applies, so a round trip through `u32::MAX + 1`
    /// reports `u32::MAX`; the refusal stays on the consuming constructor
    /// ([`InitError::DomainIdOutOfRange`]), which is where a `Result` exists
    /// to carry it.
    pub const fn domain_id(&self) -> Option<usize> {
        match self.domain_id {
            Some(d) => Some(d as usize),
            None => None,
        }
    }
}

#[cfg(feature = "alloc")]
/// The range check every constructor applies to a domain id, whatever its
/// source (env, bake, override): the same bound `try_resolve_hosted` enforces
/// ("INCLUDING a baked one"), so a freestanding image cannot boot on a domain
/// a hosted process would have refused.
fn check_domain_id(domain_id: u32) -> Result<u32, InitError> {
    if domain_id > DOMAIN_ID_MAX {
        Err(InitError::DomainIdOutOfRange)
    } else {
        Ok(domain_id)
    }
}

#[cfg(feature = "alloc")]
impl Context {
    /// `rclrs::Context::domain_id()` — the resolved ROS domain.
    ///
    /// `usize`, because that is rclrs's return type (0.7.0,
    /// `rclrs/src/context.rs`) and this name exists so a ported
    /// `ctx.domain_id()` compiles. The field stays `pub` and stays `u32`:
    /// `Context` is a plain value with nothing to hide, `u32` is the width
    /// every producer and consumer of a domain in this tree speaks
    /// (`BoardConfig::domain_id`, `ExecutorConfig::domain_id`,
    /// `NodeHandle::domain_id`, the `NROS_DOMAIN_ID` bake), and fields and
    /// methods are in different namespaces in Rust, so the two coexist. The
    /// widening is lossless on every target here — `usize` is at least 32
    /// bits on all of them, down to `thumbv7m-none-eabi`.
    ///
    /// phase-467 row `rust:Context::domain_id`. The row proposed `-> u32`
    /// once and that would have been a second, quieter port break; it then
    /// argued `usize` from consistency with [`InitOptions`], which was only
    /// two thirds true — the setters took `usize`, the getter returned
    /// `Option<u32>`. Both getters speak `usize` now, so the argument is the
    /// one this crate can actually make: it is upstream's type.
    #[must_use]
    pub const fn domain_id(&self) -> usize {
        self.domain_id as usize
    }

    /// Materialise an [`ExecutorConfig`] for a node with the given name.
    ///
    /// The returned config borrows from `self`, so callers usually do:
    ///
    /// ```ignore
    /// let ctx = nros::Context::default_from_env()?;
    /// let cfg = ctx.config("talker");
    /// let mut executor = nros::Executor::open(&cfg)?;
    /// ```
    ///
    /// phase-427 W10 — this is the shape where the executor IS the node it was
    /// configured with, and it stays: an entry that opens exactly one node
    /// wants exactly this. What it cannot express is the second node, which is
    /// why `Context::create_executor()` + `Executor::create_node(name)` now sit
    /// beside it — the session takes no name, and each node takes its own.
    // phase-427 W10 — the one legitimate remaining use of the deprecated
    // builder spelling: this method IS the executor-is-the-node shape, so it
    // asks for it deliberately rather than by not having heard.
    ///
    /// # Refused when the context carries `--ros-args` remaps
    ///
    /// An [`ExecutorConfig`] has nowhere to put them, so the executor
    /// `Executor::open` builds from it would never see them — the silent drop
    /// `init_with_args` exists to prevent. Such a context PANICS here,
    /// naming `Context::create_executor`, which installs them.
    #[allow(deprecated)]
    pub fn config<'a>(&'a self, node_name: &'a str) -> ExecutorConfig<'a> {
        #[cfg(feature = "env")]
        if !self.ros_args.is_empty() {
            refuse_ros_args(&RosArgsRefusal::UnreachableExecutor);
        }
        ExecutorConfig::new(self.locator.as_str())
            .node_name(node_name)
            .domain_id(self.domain_id)
            .mode(self.mode)
    }

    /// `rclrs::Context::default_from_env()` — the environment, default
    /// options.
    ///
    /// Hosted (feature `env`) this reads the process environment and is
    /// exactly [`init()`]; freestanding it is [`Context::baked`], the build
    /// environment this image was compiled for. RFC-0089 "Context and `init`,
    /// settled": the semantics survive with the source moved from run time to
    /// build time, which is the bounded half of this adoption.
    pub fn default_from_env() -> Result<Context, InitError> {
        #[cfg(feature = "env")]
        {
            init()
        }
        #[cfg(not(feature = "env"))]
        {
            Self::baked()
        }
    }

    /// `rclrs::Context::from_env(options)` — [`default_from_env`](Self::default_from_env)
    /// with the [`InitOptions`] applied on top: a
    /// `Some` domain replaces the resolved one, `None` keeps it.
    ///
    /// Bounded: [`InitOptions`] carries rclrs's one option, `domain_id`, and no
    /// other, so an option rclrs adds later is a compile error here. A domain above
    /// `DOMAIN_ID_MAX` is [`InitError::DomainIdOutOfRange`]. Freestanding, the base
    /// the options override is the BAKED context ([`Context::baked`]), not a
    /// process environment.
    pub fn from_env(options: InitOptions) -> Result<Context, InitError> {
        let mut ctx = Self::default_from_env()?;
        if let Some(domain_id) = options.domain_id {
            ctx.domain_id = check_domain_id(domain_id)?;
        }
        Ok(ctx)
    }

    /// `rclrs::Context::new(args, options)` — the argv constructor. HOSTED
    /// ONLY: a freestanding image has no argv, so the name is absent there and
    /// the compiler says so.
    ///
    /// **What it does with the arguments:** the same as [`init_with_args`] —
    /// `-r`/`--remap` rules inside `--ros-args` are parsed onto the returned
    /// context and every other ROS argument is REFUSED LOUDLY at the call
    /// ([`REFUSE_INIT_ARGS`]). Arguments outside a `--ros-args` scope are the
    /// program's own and are left alone.
    ///
    /// `S: AsRef<str>` is a superset of rclrs's `Item = String`, so
    /// `Context::new(std::env::args(), InitOptions::new())` compiles as
    /// written.
    #[cfg(feature = "env")]
    pub fn new<I, S>(args: I, options: InitOptions) -> Result<Context, InitError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let ros_args = parse_ros_args_or_refuse(args);
        let mut context = Self::from_env(options)?;
        context.ros_args = ros_args;
        Ok(context)
    }

    /// phase-427 W9 — the freestanding constructor: the constants baked into
    /// this crate at compile time.
    ///
    /// * `NROS_LOCATOR` — the middleware locator. Unset or empty ⇒ empty ⇒
    ///   the linked backend's own default (issue 0330; for zenoh-pico that is
    ///   multicast scouting, which native_sim NSOS cannot satisfy — hence the
    ///   bake).
    /// * `NROS_DOMAIN_ID` — the domain. Unset ⇒ 0; non-numeric ⇒
    ///   [`InitError::EnvParseFailed`]; above `DOMAIN_ID_MAX` ⇒
    ///   [`InitError::DomainIdOutOfRange`].
    ///
    /// Both names reach this crate's build the same way they reach the board
    /// crates: on Zephyr, `build.rs` re-exports `CONFIG_NROS_ZENOH_LOCATOR` /
    /// `CONFIG_NROS_DOMAIN_ID` from `$DOTCONFIG` (`nros_zephyr_build::
    /// bake_nros_config`, the issue-0460 channel); on the other RTOS lanes the
    /// fixture build exports them into the process environment. This is the
    /// ONE `option_env!` site for the pair — the entry macros used to carry
    /// their own copies (and `nros::main!`'s Zephyr arm had lost the domain
    /// half, issue 0161's class), which is what "one source of the baked
    /// shape" replaces.
    ///
    /// Mode is `Client` and `rmw` is empty: an image links exactly one
    /// backend and nothing bakes a session mode today.
    ///
    /// Hosted builds have this too — it answers "what was baked", which a
    /// hosted process rarely wants; [`Context::default_from_env`] is the
    /// constructor that picks the right source for the build.
    pub fn baked() -> Result<Context, InitError> {
        Self::from_baked(option_env!("NROS_LOCATOR"), option_env!("NROS_DOMAIN_ID"))
    }

    /// The parse behind [`Context::baked`], with the two constants injected
    /// so a unit test can exercise every arm without a build environment.
    /// Public because the macros in `nros-macros` expand in another crate;
    /// not API.
    #[doc(hidden)]
    pub fn from_baked(
        baked_locator: Option<&str>,
        baked_domain_id: Option<&str>,
    ) -> Result<Context, InitError> {
        let domain_id = match baked_domain_id {
            Some(d) => check_domain_id(d.trim().parse().map_err(|_| InitError::EnvParseFailed)?)?,
            None => 0,
        };
        Ok(Context {
            domain_id,
            locator: alloc::string::String::from(baked_locator.unwrap_or("")),
            mode: SessionMode::Client,
            rmw: alloc::string::String::new(),
            source: ContextSource::Baked,
            ros_args: alloc::vec::Vec::new(),
        })
    }

    /// Replace the locator with one handed in at run time, when there is one.
    ///
    /// #166 / phase-286 W1 — native_sim test parallelism: the harness launches
    /// an image with `-testargs --nros-locator=<loc>` and a per-test router on
    /// that ephemeral port, and `nros_runtime_locator_override()`
    /// (`nros-platform-zephyr`, argv-backed) hands it back here. `None` or an
    /// empty string keeps the baked value, so on real hardware — where the hook
    /// returns NULL — the bake stands. The entry macros call this after
    /// [`Context::default_from_env`]; a hand-written entry may too.
    pub fn with_locator_override(mut self, locator: Option<&str>) -> Self {
        if let Some(loc) = locator {
            if !loc.is_empty() {
                self.locator = alloc::string::String::from(loc);
            }
        }
        self
    }

    /// phase-427 W10 — the config the executor's SESSION opens with: this
    /// context's locator, domain and mode, and no node name.
    ///
    /// The counterpart of [`config`](Self::config), which is for the older
    /// shape where the executor IS the node it was configured with. Here the
    /// session carries no ROS identity of its own — `Executor::create_node`
    /// supplies that, once per node, and one executor holds several
    /// (RFC-0047). What lands in `ExecutorConfig::node_name` is therefore
    /// whatever [`ExecutorConfig::new`] defaults to; it names the transport
    /// session for diagnostics, not a participant on the graph.
    #[cfg(feature = "rmw-cffi")]
    fn session_config(&self) -> ExecutorConfig<'_> {
        ExecutorConfig::new(self.locator.as_str())
            .domain_id(self.domain_id)
            .mode(self.mode)
    }
}

/// phase-427 W10 — `rclrs::Context::create_executor`, and the ours-only
/// no-leak twin beside it.
///
/// Both are gated on `rmw-cffi`, because an executor without the vtable
/// runtime has no session to open; `create_executor` is additionally gated on
/// `alloc`, because it is the one that leaks a default-sized backing.
#[cfg(all(feature = "alloc", feature = "rmw-cffi"))]
impl Context {
    /// `rclrs::Context::create_executor()` — open the session this context
    /// resolved and hand back the [`Executor`] that owns it.
    ///
    /// This is the entry a ported rclrs `main` lands on: upstream writes
    /// `context.create_basic_executor()` (or `create_executor(runtime)`) and
    /// gets an `Executor` back; ours returns a `Result`, so the port is a `?`.
    /// The runtime argument has no counterpart here and is not accepted — one
    /// executor per RTOS task is the model (RFC-0002), and there is no runtime
    /// to choose between.
    ///
    /// The node name is NOT taken here. Name nodes with
    /// [`Executor::create_node`], which is where several named nodes on one
    /// session already live (RFC-0047).
    ///
    /// # Available on `alloc` only
    ///
    /// It leaks an executor-lifetime backing at
    /// [`ExecutorSizing::DEFAULT`] — the reason it is `Executor<'static>` and
    /// the reason it needs an allocator. A target without one calls
    /// [`create_executor_in`](Self::create_executor_in) with its own
    /// `static`, which is the same open with the storage supplied rather than
    /// leaked. (Today [`Context`] itself has an `alloc` floor — its `locator`
    /// and `rmw` are owned `String`s — so the pair moves together; if that
    /// floor ever drops, this is the half that stays behind.)
    ///
    /// ```ignore
    /// let context = nros::Context::default_from_env()?;
    /// let mut executor = context.create_executor()?;
    /// let mut node = executor.create_node("talker")?;
    /// ```
    ///
    /// The `-r` rules `init_with_args` / `Context::new` parsed are
    /// installed in the new executor as its fallback remap tier.
    pub fn create_executor(&self) -> Result<Executor<'static>, InitError> {
        let mut executor =
            Executor::open(&self.session_config()).map_err(InitError::ExecutorOpenFailed)?;
        self.install_ros_args(&mut executor);
        Ok(executor)
    }

    /// Put this context's `--ros-args` remaps into `executor`'s fallback tier.
    /// A rule that does not fit is REFUSED rather than dropped.
    fn install_ros_args(&self, executor: &mut Executor<'_>) {
        #[cfg(feature = "env")]
        {
            let rules = self.ros_args.iter().map(|r| nros_node::ros_args::RemapArg {
                node: r.node.as_deref(),
                from: r.from.as_str(),
                to: r.to.as_str(),
            });
            if let Err(rule) = nros_node::ros_args::install_argv_remaps(executor, rules) {
                refuse_ros_args(&RosArgsRefusal::DoesNotFit {
                    node: rule.node,
                    from: rule.from,
                    to: rule.to,
                });
            }
        }
        #[cfg(not(feature = "env"))]
        let _ = executor;
    }

    /// OURS — [`create_executor`](Self::create_executor) with the storage
    /// supplied by the caller instead of leaked.
    ///
    /// The freestanding form. An RTOS or bare-metal entry owns a
    /// `static mut [MaybeUninit<u64>; N]` (or the array the `nros::main!`
    /// macro sizes for it) and hands it in; nothing is allocated, and the
    /// returned executor borrows the backing for `'b` rather than living
    /// forever. rclrs has no counterpart — it allocates.
    ///
    /// `backing` must hold at least `ExecutorSizing::DEFAULT.u64_len()` words.
    /// A shorter slice is [`InitError::BackingTooSmall`] naming both numbers,
    /// never undefined behaviour: this is the safe wrapper around the `unsafe`
    /// `Executor::open_in`, and the length check is what makes it safe. Size
    /// the array with `ExecutorSizing::DEFAULT.u64_len()` rather than a
    /// literal, since the default moves with `NROS_EXECUTOR_MAX_CBS` and its
    /// siblings.
    ///
    /// ```ignore
    /// static mut BACKING: [MaybeUninit<u64>; N] = [MaybeUninit::uninit(); N];
    /// let context = nros::Context::default_from_env()?;
    /// // SAFETY: single-threaded entry, taken once.
    /// let mut executor = context.create_executor_in(unsafe { &mut *&raw mut BACKING })?;
    /// let mut node = executor.create_node("talker")?;
    /// ```
    pub fn create_executor_in<'b>(
        &self,
        backing: &'b mut [MaybeUninit<u64>],
    ) -> Result<Executor<'b>, InitError> {
        let sizing = ExecutorSizing::DEFAULT;
        let needed = sizing.u64_len();
        let given = backing.len();
        if given < needed {
            return Err(InitError::BackingTooSmall { needed, given });
        }
        // SAFETY: `backing` is at least `sizing.u64_len()` words (checked
        // directly above), is `u64`-aligned by its element type, is uniquely
        // borrowed for `'b`, and the returned `Executor<'b>` is the only thing
        // that can reach it for that lifetime.
        let mut executor = unsafe { Executor::open_in(&self.session_config(), backing, sizing) }
            .map_err(InitError::ExecutorOpenFailed)?;
        self.install_ros_args(&mut executor);
        Ok(executor)
    }
}

#[cfg(feature = "env")]
fn read_env_context(source: ContextSource) -> Result<Context, InitError> {
    // issue 0687 — through the ONE resolver, not a third parse of the same four
    // variables. This function had its own copy, and the copies had drifted:
    // it did not warn on the legacy `$ZENOH_LOCATOR`/`$ZENOH_MODE` spellings
    // the other reader deprecates, and it range-checked nothing, so
    // `ROS_DOMAIN_ID=300` reached a backend that could only fail later.
    //
    // Issue 0330 — no backend default for the locator: `nros` is RMW-agnostic,
    // so unset env leaves it EMPTY (= absent) and the linked backend
    // substitutes its own (zenoh: `nros_rmw_zenoh::DEFAULT_LOCATOR`; xrce: its
    // agent default; cyclonedds ignores the locator entirely). That is what
    // `resolve_hosted` does with an empty `BootConfig` too.
    let resolved =
        crate::env::try_resolve_hosted(nros_node::BootConfig::default()).map_err(|e| match e {
            nros_node::BootConfigError::DomainIdRange => InitError::DomainIdOutOfRange,
            nros_node::BootConfigError::DomainIdParse => InitError::EnvParseFailed,
        })?;
    let locator = alloc::string::String::from(resolved.locator);
    let domain_id = resolved.domain_id;
    let mode = resolved.mode;
    // issue 0687 — the `$NROS_RMW` half comes from the shared selector; the
    // `RMW_IMPLEMENTATION` fallback stays HERE and only here. `Context.rmw` is
    // a ROS-vocabulary HINT (`rmw_cyclonedds_cpp`), not the cffi registry
    // selector (`cyclonedds`) — folding the two together would hand a ROS name
    // to `resolve_backend`, which answers `Unknown` and fails the open.
    let rmw = crate::rmw_selector()
        .map(|s| alloc::string::String::from(s.as_str()))
        .or_else(|| std::env::var("RMW_IMPLEMENTATION").ok())
        .unwrap_or_default();
    Ok(Context {
        domain_id,
        locator,
        mode,
        rmw,
        source,
        ros_args: alloc::vec::Vec::new(),
    })
}

/// Pattern 3 — raw init, launch file ignored. The C++-symmetric anchor:
/// `rclcpp::init()` in the Rust spelling, and EQUAL to
/// [`Context::default_from_env`] on a hosted build (RFC-0089 "Context and
/// `init`, settled" — each language follows its own upstream; rclrs spells
/// this one on the type, rclcpp as a free function, and both are here).
///
/// Reads env vars (`ROS_DOMAIN_ID`, `NROS_LOCATOR`, `NROS_SESSION_MODE`,
/// `NROS_RMW` / `RMW_IMPLEMENTATION`) and returns a [`Context`]. The
/// caller owns the spin loop — typically `Executor::open(&ctx.config(name))`
/// followed by `spin` or a hand-rolled `spin_once` loop.
#[cfg(feature = "env")]
pub fn init() -> Result<Context, InitError> {
    read_env_context(ContextSource::Env)
}

/// The refusal [`init_with_args`] and [`Context::new`] emit for a ROS
/// argument nano-ros does not honour. Panics carry this text followed by the
/// refused argument. `NROS_RCLCPP_REFUSE_INIT_ARGV` is the C++ twin
/// (`packages/api/nros-cpp/include/nros/log.hpp`), which still refuses
/// `--ros-args` outright.
#[cfg(feature = "env")]
pub const REFUSE_INIT_ARGS: &str = "nros::init_with_args / nros::Context::new was given a --ros-args \
argument nano-ros cannot honour (RFC-0089, phase-417 W3.b). Proceeding would DISCARD it -- the \
'compiles and differs' the rule forbids. HONOURED inside --ros-args ... --: -r / --remap \
[node:]from:=to, applied as the FALLBACK beneath any remap the launch file projected for the same \
name (RFC-0046; rcl's local-before-global), through Context::create_executor / \
create_executor_in. REFUSED: -p / --param / --params-file (runtime parameters belong to RFC-0015 \
section 9's channel), node-identity remaps (__node, __name, __ns), -e / --enclave, the log flags, \
and any token that is not a ROS flag. A nros sync image does not read argv at all: its launch \
remaps and parameters are projected into the GENERATED ENTRY at BUILD time.";

/// Does `args` open a `--ros-args` scope at all?
///
/// No longer the refusal's predicate — since the `-r` parse landed, a vector
/// with `--ros-args` is parsed, and only what it cannot honour is refused
/// (`nros_node::ros_args::parse_ros_args`). Kept because "will this argv be
/// read?" is still a question a caller can ask before handing it over.
///
/// Exact match only. `--ros-args-extra` is not the flag, and a prefix match
/// would claim an argument nano-ros never reads — pinned by a unit test.
#[cfg(feature = "env")]
pub fn args_have_ros_args<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter().any(|a| a.as_ref() == "--ros-args")
}

/// Why a `--ros-args` vector, or a context carrying one, was refused.
#[cfg(feature = "env")]
enum RosArgsRefusal<'a> {
    /// The parse itself refused an argument.
    Parse(nros_node::ros_args::RosArgsError<'a>),
    /// A parsed rule did not fit the executor's remap table.
    #[cfg(feature = "rmw-cffi")]
    DoesNotFit {
        node: Option<&'a str>,
        from: &'a str,
        to: &'a str,
    },
    /// [`Context::config`] was asked for a config that cannot carry the rules.
    UnreachableExecutor,
}

#[cfg(feature = "env")]
impl core::fmt::Display for RosArgsRefusal<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Parse(e) => e.fmt(f),
            #[cfg(feature = "rmw-cffi")]
            Self::DoesNotFit { node, from, to } => {
                let node = node.map(|n| alloc::format!("{n}:")).unwrap_or_default();
                write!(
                    f,
                    "`-r {node}{from}:={to}` does not fit the executor's remap table (MAX_REMAPS rules, \
                     shared with launch remaps; names up to {} bytes)",
                    nros_node::names::MAX_RESOLVED_NAME_LEN
                )
            }
            Self::UnreachableExecutor => f.write_str(
                "Context::config() cannot carry this context's --ros-args remaps into the executor \
                 Executor::open builds from it; use Context::create_executor() or \
                 create_executor_in(), which install them",
            ),
        }
    }
}

/// The one refusal site: logged through `nros_log` (never `std::println!`,
/// issue 0589), then a panic carrying [`REFUSE_INIT_ARGS`] and the reason —
/// the shape `rclcpp::init(argc, argv)` has in `nros.hpp`, and the shape
/// rclcpp's own `UnknownROSArgsError` has.
#[cfg(feature = "env")]
fn refuse_ros_args(why: &RosArgsRefusal<'_>) -> ! {
    nros_log::log_error!(
        nros_log::get_logger("nros"),
        "{}\n  refused: {}",
        REFUSE_INIT_ARGS,
        why
    );
    panic!("{REFUSE_INIT_ARGS}\n  refused: {why}");
}

/// Parse `args` (rcl's grammar — `nros_node::ros_args`) into owned rules, or
/// refuse. Shared by [`init_with_args`] and [`Context::new`], so the two stay
/// one behaviour.
#[cfg(feature = "env")]
fn parse_ros_args_or_refuse<I, S>(args: I) -> alloc::vec::Vec<ArgvRemap>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let owned: alloc::vec::Vec<alloc::string::String> = args
        .into_iter()
        .map(|a| alloc::string::String::from(a.as_ref()))
        .collect();
    let mut rules = alloc::vec::Vec::new();
    let parsed = nros_node::ros_args::parse_ros_args(owned.iter().map(|a| a.as_str()), |r| {
        rules.push(ArgvRemap {
            node: r.node.map(alloc::string::String::from),
            from: alloc::string::String::from(r.from),
            to: alloc::string::String::from(r.to),
        })
    });
    if let Err(e) = parsed {
        refuse_ros_args(&RosArgsRefusal::Parse(e));
    }
    rules
}

/// Pattern 3 — like [`init()`] but takes the process arguments, the way
/// `rclcpp::init(argc, argv)` / rclrs's `Context::new(args, ..)` do. The
/// free-function twin of [`Context::new`] with default options.
///
/// # What it does with the arguments
///
/// It parses `--ros-args` with rcl's grammar (Humble `rcl/arguments.h`): ROS
/// arguments live between `--ros-args` and `--` (or the end), several scopes
/// may appear, and everything outside them is the program's own and is left
/// alone.
///
/// * **`-r` / `--remap [node:]from:=to` — honoured.** The rules ride on the
///   returned [`Context`] and are installed in every executor it creates
///   (`Context::create_executor` / `Context::create_executor_in`) as the
///   FALLBACK remap tier: a launch-projected rule for the same name wins
///   (RFC-0046), exactly as rcl checks a node's local arguments before the
///   process's global ones. They reach entities on every road — the handle
///   `Executor::create_node` returns, `nros::node!` components, and the C and
///   C++ APIs.
/// * **Everything else inside a scope — refused, loudly**: parameter
///   overrides (RFC-0015 §9 owns that channel), node-identity remaps, enclaves,
///   log flags, unknown tokens. Logged through `nros_log`, then a panic
///   carrying [`REFUSE_INIT_ARGS`] and the argument. rclcpp refuses unknown
///   ROS arguments too (`UnknownROSArgsError`).
///
/// # Where the rules do NOT reach
///
/// [`Context::config`] refuses a context that carries rules, because an
/// `ExecutorConfig` cannot carry them. Entities created through the
/// executor's own `add_*` registration methods or a `node_mut` context are
/// not remapped either: those are also where the C ABI and the component
/// sink hand their ALREADY-RESOLVED names, so remapping there would apply a
/// rule twice.
#[cfg(feature = "env")]
pub fn init_with_args<I, S>(args: I) -> Result<Context, InitError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let ros_args = parse_ros_args_or_refuse(args);
    let mut context = init()?;
    context.ros_args = ros_args;
    Ok(context)
}

#[cfg(all(test, feature = "env"))]
mod ros_args_refusal_tests {
    use super::*;

    #[test]
    fn predicate_matches_the_flag_exactly() {
        let _env = crate::env::test_env_lock();
        assert!(args_have_ros_args(["node", "--ros-args", "-p", "x:=1"]));
        assert!(args_have_ros_args(["--ros-args"]));
        // A prefix match would refuse an argument nano-ros never drops.
        assert!(!args_have_ros_args(["--ros-args-extra"]));
        assert!(!args_have_ros_args(["--ros-arg"]));
        assert!(!args_have_ros_args(["node", "--verbose"]));
        assert!(!args_have_ros_args(core::iter::empty::<&str>()));
    }

    fn remap(node: Option<&str>, from: &str, to: &str) -> ArgvRemap {
        ArgvRemap {
            node: node.map(alloc::string::String::from),
            from: alloc::string::String::from(from),
            to: alloc::string::String::from(to),
        }
    }

    #[test]
    fn remap_rules_ride_on_the_context_in_argv_order() {
        let _env = crate::env::test_env_lock();
        let ctx = init_with_args([
            "/usr/bin/talker",
            "--ros-args",
            "-r",
            "chatter:=/other",
            "--remap",
            "talker:~/out:=/wire",
            "--",
            "positional",
        ])
        .expect("init");
        assert_eq!(
            ctx.ros_args,
            [
                remap(None, "chatter", "/other"),
                remap(Some("talker"), "~/out", "/wire")
            ]
        );
    }

    #[test]
    fn an_empty_scope_is_not_a_refusal() {
        let _env = crate::env::test_env_lock();
        assert!(
            init_with_args(["--ros-args"])
                .expect("init")
                .ros_args
                .is_empty()
        );
    }

    #[test]
    #[should_panic(expected = "refused: `-p`")]
    fn parameter_overrides_are_still_refused_by_name() {
        let _env = crate::env::test_env_lock();
        let _ = init_with_args(["/usr/bin/talker", "--ros-args", "-r", "a:=b", "-p", "x:=1"]);
    }

    #[test]
    #[should_panic(expected = "remaps a node's identity")]
    fn identity_remaps_are_refused() {
        let _env = crate::env::test_env_lock();
        let _ = init_with_args(["--ros-args", "-r", "__ns:=/robot"]);
    }

    #[test]
    #[should_panic(expected = "Context::config() cannot carry")]
    fn config_refuses_a_context_that_carries_remaps() {
        let _env = crate::env::test_env_lock();
        let ctx = init_with_args(["--ros-args", "-r", "chatter:=/other"]).expect("init");
        let _ = ctx.config("talker");
    }

    #[test]
    fn config_is_unaffected_without_remaps() {
        let _env = crate::env::test_env_lock();
        let ctx = init_with_args(["/usr/bin/talker", "--verbose"]).expect("init");
        assert_eq!(ctx.config("talker").node_name, "talker");
    }

    #[test]
    fn plain_args_pass_through_to_init() {
        let _env = crate::env::test_env_lock();
        // Same answer as `init()` — the arguments are not consulted, and no
        // refusal fires. Both read the same environment, so compare the
        // resolved knobs rather than asserting a particular value.
        let via_args = init_with_args(["/usr/bin/talker", "--verbose", "positional"]);
        let plain = init();
        match (via_args, plain) {
            (Ok(a), Ok(b)) => {
                assert_eq!(a.domain_id, b.domain_id);
                assert_eq!(a.locator, b.locator);
                assert_eq!(a.rmw, b.rmw);
            }
            (Err(a), Err(b)) => assert_eq!(a, b),
            (a, b) => panic!("init_with_args and init disagree: {a:?} vs {b:?}"),
        }
    }

    // phase-427 W9 — the rclrs family on a hosted build.

    fn same_identity(a: &Context, b: &Context) {
        assert_eq!(a.domain_id, b.domain_id);
        assert_eq!(a.locator, b.locator);
        assert_eq!(a.mode, b.mode);
        assert_eq!(a.rmw, b.rmw);
        assert_eq!(a.source, b.source);
    }

    #[test]
    fn default_from_env_is_init() {
        let _env = crate::env::test_env_lock();
        match (Context::default_from_env(), init()) {
            (Ok(a), Ok(b)) => {
                same_identity(&a, &b);
                assert_eq!(a.source, ContextSource::Env);
            }
            (Err(a), Err(b)) => assert_eq!(a, b),
            (a, b) => panic!("default_from_env and init disagree: {a:?} vs {b:?}"),
        }
    }

    #[test]
    fn from_env_honours_the_domain_override() {
        let _env = crate::env::test_env_lock();
        let env = match init() {
            Ok(c) => c,
            Err(e) => panic!("init() failed on the test host: {e:?}"),
        };
        // A value the environment is not carrying, so the override is visible
        // whatever `ROS_DOMAIN_ID` says.
        let wanted = if env.domain_id == 7 { 9 } else { 7 };
        let ctx = Context::from_env(InitOptions::new().with_domain_id(Some(wanted as usize)))
            .expect("from_env with a valid override");
        assert_eq!(ctx.domain_id, wanted);
        // Everything else is untouched.
        assert_eq!(ctx.locator, env.locator);
        assert_eq!(ctx.rmw, env.rmw);
        assert_eq!(ctx.source, ContextSource::Env);
    }

    #[test]
    fn from_env_with_no_override_keeps_the_env_value() {
        let _env = crate::env::test_env_lock();
        match (
            Context::from_env(InitOptions::new().with_domain_id(None)),
            init(),
        ) {
            (Ok(a), Ok(b)) => same_identity(&a, &b),
            (Err(a), Err(b)) => assert_eq!(a, b),
            (a, b) => panic!("from_env(None) and init disagree: {a:?} vs {b:?}"),
        }
    }

    #[test]
    fn from_env_rejects_an_out_of_range_override() {
        let _env = crate::env::test_env_lock();
        if init().is_err() {
            // The environment itself is invalid; the override is never reached.
            return;
        }
        assert_eq!(
            Context::from_env(InitOptions::new().with_domain_id(Some(DOMAIN_ID_MAX as usize + 1)))
                .map(|c| c.domain_id),
            Err(InitError::DomainIdOutOfRange)
        );
        // A value no `u32` can hold saturates in the builder and is refused
        // here, not truncated into a plausible small number.
        assert_eq!(
            Context::from_env(InitOptions::new().with_domain_id(Some(usize::MAX)))
                .map(|c| c.domain_id),
            Err(InitError::DomainIdOutOfRange)
        );
    }

    #[test]
    #[should_panic(expected = "refused: `--enclave`")]
    fn context_new_refuses_what_it_cannot_honour() {
        let _env = crate::env::test_env_lock();
        let _ = Context::new(
            [
                "/usr/bin/talker",
                "--ros-args",
                "-r",
                "chatter:=/other",
                "--enclave",
                "/e",
            ],
            InitOptions::new(),
        );
    }

    #[test]
    fn context_new_parses_exactly_like_init_with_args() {
        let _env = crate::env::test_env_lock();
        let argv = ["/usr/bin/talker", "--ros-args", "-r", "chatter:=/other"];
        let a = Context::new(argv, InitOptions::new()).expect("Context::new");
        let b = init_with_args(argv).expect("init_with_args");
        assert_eq!(a.ros_args, b.ros_args);
        assert_eq!(a.ros_args, [remap(None, "chatter", "/other")]);
    }

    #[test]
    fn context_new_passes_plain_args_through() {
        let _env = crate::env::test_env_lock();
        let opts = InitOptions::new().with_domain_id(Some(3));
        match (
            Context::new(["/usr/bin/talker", "--verbose", "positional"], opts),
            Context::from_env(opts),
        ) {
            (Ok(a), Ok(b)) => {
                same_identity(&a, &b);
                assert_eq!(a.domain_id, 3);
            }
            (Err(a), Err(b)) => assert_eq!(a, b),
            (a, b) => panic!("Context::new and from_env disagree: {a:?} vs {b:?}"),
        }
    }

    #[test]
    fn context_new_accepts_rclrs_argv_shape() {
        let _env = crate::env::test_env_lock();
        // `impl IntoIterator<Item = String>` is what rclrs takes; ours must
        // accept the same call unchanged.
        let args: alloc::vec::Vec<alloc::string::String> =
            alloc::vec![alloc::string::String::from("talker")];
        let _ = Context::new(args, InitOptions::default());
    }
}

/// phase-427 W9 — the freestanding constructor's parse, with the constants
/// injected. Runs on the `alloc` floor (`cargo test -p nros --features alloc
/// --lib`); nothing here reads the environment.
#[cfg(all(test, feature = "alloc"))]
mod baked_tests {
    use super::*;

    #[test]
    fn nothing_baked_is_domain_zero_and_an_absent_locator() {
        let ctx = Context::from_baked(None, None).expect("nothing baked is valid");
        assert_eq!(ctx.domain_id, 0);
        assert_eq!(ctx.locator, "");
        assert_eq!(ctx.mode, SessionMode::Client);
        assert_eq!(ctx.rmw, "");
        assert_eq!(ctx.source, ContextSource::Baked);
    }

    #[test]
    fn baked_values_are_carried() {
        let ctx = Context::from_baked(Some("tcp/127.0.0.1:7456"), Some("42")).expect("valid bake");
        assert_eq!(ctx.locator, "tcp/127.0.0.1:7456");
        assert_eq!(ctx.domain_id, 42);
        assert_eq!(ctx.source, ContextSource::Baked);
        // The config it materialises is the one the macros used to assemble
        // by hand: locator, domain, node name, client mode.
        let cfg = ctx.config("talker");
        assert_eq!(cfg.locator, "tcp/127.0.0.1:7456");
        assert_eq!(cfg.domain_id, 42);
        assert_eq!(cfg.node_name, "talker");
        assert_eq!(cfg.mode, SessionMode::Client);
    }

    #[test]
    fn an_empty_baked_locator_is_absent() {
        let ctx = Context::from_baked(Some(""), None).expect("empty is absent, not an error");
        assert_eq!(ctx.locator, "");
    }

    #[test]
    fn a_non_numeric_baked_domain_is_refused() {
        assert_eq!(
            Context::from_baked(None, Some("seven")).map(|c| c.domain_id),
            Err(InitError::EnvParseFailed)
        );
        assert_eq!(
            Context::from_baked(None, Some("-1")).map(|c| c.domain_id),
            Err(InitError::EnvParseFailed)
        );
    }

    #[test]
    fn an_out_of_range_baked_domain_is_refused() {
        // The bound `try_resolve_hosted` applies, so an image cannot be built
        // onto a domain a hosted process would refuse to boot on.
        assert_eq!(
            Context::from_baked(None, Some("300")).map(|c| c.domain_id),
            Err(InitError::DomainIdOutOfRange)
        );
        assert_eq!(
            Context::from_baked(None, Some("232")).map(|c| c.domain_id),
            Ok(DOMAIN_ID_MAX)
        );
    }

    #[test]
    fn locator_override_applies_only_when_present_and_non_empty() {
        let baked = || Context::from_baked(Some("tcp/10.0.2.2:7447"), Some("1")).unwrap();
        assert_eq!(
            baked()
                .with_locator_override(Some("tcp/127.0.0.1:7456"))
                .locator,
            "tcp/127.0.0.1:7456"
        );
        assert_eq!(
            baked().with_locator_override(None).locator,
            "tcp/10.0.2.2:7447"
        );
        assert_eq!(
            baked().with_locator_override(Some("")).locator,
            "tcp/10.0.2.2:7447"
        );
        // The override touches nothing else.
        let ctx = baked().with_locator_override(Some("tcp/127.0.0.1:7456"));
        assert_eq!(ctx.domain_id, 1);
        assert_eq!(ctx.source, ContextSource::Baked);
    }

    /// phase-467 row `rust:Context::domain_id`. Two claims, and the second is
    /// the one worth a test: the accessor answers the field, and it answers it
    /// as a `usize` — rclrs's type — so a ported `let d: usize =
    /// ctx.domain_id();` compiles. The annotation is the assertion; without it
    /// an integer literal would infer whatever the accessor returns and the
    /// test would pass against a `u32` too.
    #[test]
    fn context_domain_id_reads_the_field_as_rclrs_types_it() {
        let ctx = Context::from_baked(Some("tcp/10.0.2.2:7447"), Some("7")).unwrap();
        let d: usize = ctx.domain_id();
        assert_eq!(d, 7);
        assert_eq!(ctx.domain_id(), ctx.domain_id as usize);
        // The field did not move: `Context` is a plain struct and both
        // spellings reach the same byte.
        assert_eq!(ctx.domain_id, 7u32);
    }

    #[test]
    fn baked_reads_this_crates_build_environment() {
        // The values depend on the build; the SHAPE does not. Whatever was
        // baked, the source is `Baked`, and a parse failure would have come
        // from a bake this test cannot control — report it rather than hide it.
        match Context::baked() {
            Ok(ctx) => assert_eq!(ctx.source, ContextSource::Baked),
            Err(e) => panic!(
                "NROS_DOMAIN_ID was baked into this test build with a value the constructor refuses: {e:?}"
            ),
        }
    }

    #[test]
    fn init_options_default_overrides_nothing() {
        assert_eq!(InitOptions::new(), InitOptions::default());
        assert_eq!(InitOptions::new().domain_id(), None);
        assert_eq!(
            InitOptions::new().with_domain_id(Some(5)).domain_id(),
            Some(5)
        );
        assert_eq!(
            InitOptions::new()
                .with_domain_id(Some(5))
                .with_domain_id(None)
                .domain_id(),
            None
        );
        // Saturates rather than truncates: `u32::MAX + 1` must not read as 0.
        // Reported back as a `usize`, which is the getter's type since
        // phase-467 — the saturation is in the STORED `u32` and the getter
        // only widens it, so this is the one place the two widths are both
        // visible in one line.
        assert_eq!(
            InitOptions::new()
                .with_domain_id(Some(u32::MAX as usize + 1))
                .domain_id(),
            Some(u32::MAX as usize)
        );
        // The in-place spelling is the same operation.
        let mut opts = InitOptions::new();
        opts.set_domain_id(Some(9));
        assert_eq!(opts, InitOptions::new().with_domain_id(Some(9)));
        opts.set_domain_id(None);
        assert_eq!(opts, InitOptions::new());
    }

    /// phase-427 W10 — the two variants `create_executor*` can produce say
    /// WHICH thing went wrong, in words. `alloc` only: the errors are values
    /// and need no RMW to construct, which is the point of testing them here
    /// rather than beside a session.
    #[test]
    fn the_executor_errors_say_what_happened() {
        use alloc::string::ToString as _;

        let too_small = InitError::BackingTooSmall {
            needed: 4096,
            given: 8,
        }
        .to_string();
        // Both numbers, because "too small" without them tells a caller
        // nothing they can act on.
        assert!(too_small.contains("4096"), "{too_small}");
        assert!(too_small.contains('8'), "{too_small}");

        // The backend's own error survives the wrapping — issue 0465's rule:
        // an exhausted table and a missing router must not read alike.
        let open_failed = InitError::ExecutorOpenFailed(NodeError::NodeTableFull).to_string();
        assert!(open_failed.contains("NodeTableFull"), "{open_failed}");
        assert_ne!(
            open_failed,
            InitError::ExecutorOpenFailed(NodeError::NameTooLong).to_string()
        );
    }

    #[cfg(not(feature = "env"))]
    #[test]
    fn freestanding_default_from_env_is_the_bake() {
        // Without `env` there is no process environment to read; the family
        // resolves to the bake, and the override still lands on top of it.
        let ctx = Context::default_from_env().expect("bake");
        assert_eq!(ctx.source, ContextSource::Baked);
        let ctx = Context::from_env(InitOptions::new().with_domain_id(Some(11))).expect("bake");
        assert_eq!(ctx.domain_id, 11);
        assert_eq!(ctx.source, ContextSource::Baked);
    }
}

/// Pattern 2 — launch-aware init.
///
/// Resolves the launch file via:
///
/// 1. `$NROS_RUNTIME_OVERLAY` — when set, the path points at a JSON sidecar
///    written by `nros launch --emit-runtime-overlay`. (NOT yet consumed;
///    placeholder for the follow-up wave.)
/// 2. `<CARGO_MANIFEST_DIR>/launch/<pkg>.launch.xml` or
///    `<CARGO_MANIFEST_DIR>/launch/system.launch.xml`. (NOT yet parsed;
///    placeholder.)
/// 3. The env vars described in [`init()`] — the launcher projects launch
///    params into the child env before `exec()`, so the env path is the
///    de-facto launch overlay today.
///
/// Returns a [`Context`] whose `source = ContextSource::Launch` so callers
/// can introspect whether the run is launch-driven.
#[cfg(feature = "env")]
pub fn init_with_launch_auto() -> Result<Context, InitError> {
    // TODO (Phase 212.L.5 follow-up):
    //   1. If $NROS_RUNTIME_OVERLAY is set, read the JSON sidecar and fold
    //      its params/remaps/env into the Context.
    //   2. Else walk <CARGO_MANIFEST_DIR>/launch/* and parse the XML
    //      in-process (Option B — only if Option A overhead is rejected).
    // For now the env path is the only overlay channel.
    read_env_context(ContextSource::Launch)
}

/// Pattern 2 — explicit-path variant of [`init_with_launch_auto`].
///
/// Verifies the file exists (so misspelled paths fail fast at init time)
/// but does NOT yet parse the XML; the launcher's projected env is the
/// active overlay. See the module-level notes for the follow-up plan.
#[cfg(feature = "env")]
pub fn init_with_launch(path: impl AsRef<Path>) -> Result<Context, InitError> {
    let p = path.as_ref();
    if !p.exists() {
        return Err(InitError::LaunchFileNotFound);
    }
    // TODO (Phase 212.L.5 follow-up): parse the launch XML and fold params
    // / remaps / env into the returned Context. Today we only verify the
    // file exists and fall through to the env overlay path.
    read_env_context(ContextSource::Launch)
}

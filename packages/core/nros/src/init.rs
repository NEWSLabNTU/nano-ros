//! Phase 212.L.5 — top-level init API.
//!
//! Three patterns are supported (per the Phase 212.L canonical pkg shape):
//!
//! 1. **Component pkg** — register via the [`nros::component!`](crate::component)
//!    macro (Phase 172 W.3); the generated runtime owns the spin loop.
//! 2. **Application pkg + launch-aware** — call [`init_with_launch_auto`] (or
//!    [`init_with_launch`] for an explicit path). The returned [`Context`]
//!    carries launch-resolved fields (domain id, locator, RMW choice). User
//!    code drives its own spin via [`crate::Executor::open`] +
//!    [`crate::Executor::spin_blocking`].
//! 3. **Application pkg + custom spin** — call [`init`] (or [`init_with_args`]
//!    for argv-style overrides). Launch file is ignored; env vars +
//!    `ExecutorConfig::from_env()` semantics still apply.
//!
//! The [`Context`] struct is a thin holder of the resolved init knobs. To
//! actually open a session, materialise an [`crate::ExecutorConfig`] via
//! [`Context::config`] and pass it to [`crate::Executor::open`].
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

#[cfg(feature = "std")]
use std::path::Path;

use nros_node::ExecutorConfig;
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
    /// A launch-derived env var (`ROS_DOMAIN_ID`, etc.) failed to parse.
    EnvParseFailed,
}

impl core::fmt::Display for InitError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            InitError::LaunchFileNotFound => f.write_str("launch file not found"),
            InitError::LaunchParseFailed => f.write_str("launch file parse failed"),
            InitError::EnvParseFailed => f.write_str("env var parse failed"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for InitError {}

/// Phase 212.L.5 — resolved init context.
///
/// Returned by every `init*` entry point. Carries the fields the user
/// needs to construct an [`ExecutorConfig`] and open a session.
///
/// Fields are owned (`String` on hosted builds) so the `Context` can
/// outlive transient parents (env caches, parsed launch files).
#[cfg(feature = "std")]
#[derive(Debug, Clone)]
pub struct Context {
    /// ROS 2 domain ID (`ROS_DOMAIN_ID`, default 0).
    pub domain_id: u32,
    /// Middleware locator (`NROS_LOCATOR` / legacy `ZENOH_LOCATOR`).
    pub locator: std::string::String,
    /// Session mode (`NROS_SESSION_MODE` / legacy `ZENOH_MODE`, default `Client`).
    pub mode: SessionMode,
    /// RMW implementation hint (`RMW_IMPLEMENTATION` /  `NROS_RMW`).
    ///
    /// Empty when neither var is set. The runtime uses this to pick a
    /// primary backend when multiple are linked; see
    /// [`crate::internals::open_session`].
    pub rmw: std::string::String,
    /// Source of this context — useful for diagnostics + tests.
    pub source: ContextSource,
}

/// Where the [`Context`] came from. Diagnostics only.
#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextSource {
    /// Built from env vars by [`init`] / [`init_with_args`].
    Env,
    /// Built from a launch file (path supplied to [`init_with_launch`]) or
    /// auto-discovered via [`init_with_launch_auto`]. The launch XML itself
    /// is NOT yet parsed (see module docs); the launcher's projected env
    /// is the source of truth for now.
    Launch,
}

#[cfg(feature = "std")]
impl Context {
    /// Materialise an [`ExecutorConfig`] for a node with the given name.
    ///
    /// The returned config borrows from `self`, so callers usually do:
    ///
    /// ```ignore
    /// let ctx = nros::init()?;
    /// let cfg = ctx.config("talker");
    /// let mut executor = nros::Executor::open(&cfg)?;
    /// ```
    pub fn config<'a>(&'a self, node_name: &'a str) -> ExecutorConfig<'a> {
        ExecutorConfig::new(self.locator.as_str())
            .node_name(node_name)
            .domain_id(self.domain_id)
            .mode(self.mode)
    }
}

#[cfg(feature = "std")]
fn read_env_context(source: ContextSource) -> Result<Context, InitError> {
    let locator = std::env::var("NROS_LOCATOR")
        .or_else(|_| std::env::var("ZENOH_LOCATOR"))
        .unwrap_or_else(|_| std::string::String::from("tcp/127.0.0.1:7447"));
    let domain_id = match std::env::var("ROS_DOMAIN_ID") {
        Ok(s) if !s.is_empty() => s.parse::<u32>().map_err(|_| InitError::EnvParseFailed)?,
        _ => 0,
    };
    let mode_str = std::env::var("NROS_SESSION_MODE")
        .or_else(|_| std::env::var("ZENOH_MODE"))
        .unwrap_or_default();
    let mode = match mode_str.as_str() {
        "peer" => SessionMode::Peer,
        _ => SessionMode::Client,
    };
    let rmw = std::env::var("NROS_RMW")
        .or_else(|_| std::env::var("RMW_IMPLEMENTATION"))
        .unwrap_or_default();
    Ok(Context {
        domain_id,
        locator,
        mode,
        rmw,
        source,
    })
}

/// Pattern 3 — raw init, launch file ignored.
///
/// Reads env vars (`ROS_DOMAIN_ID`, `NROS_LOCATOR`, `NROS_SESSION_MODE`,
/// `NROS_RMW` / `RMW_IMPLEMENTATION`) and returns a [`Context`]. The
/// caller owns the spin loop — typically `Executor::open(&ctx.config(name))`
/// followed by `spin_blocking` or a hand-rolled `spin_once` loop.
#[cfg(feature = "std")]
pub fn init() -> Result<Context, InitError> {
    read_env_context(ContextSource::Env)
}

/// Pattern 3 — like [`init`] but accepts a `[--arg=value, ...]`-style argv
/// iterator. Currently a thin wrapper over [`init`] that ignores the args;
/// the structured argv parse (`--ros-args -p foo:=42`, etc.) lands with the
/// runtime-overlay wave.
#[cfg(feature = "std")]
pub fn init_with_args<I, S>(_args: I) -> Result<Context, InitError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    // TODO (Phase 212.L.5 follow-up): parse `--ros-args` style flags.
    init()
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
/// 3. The env vars described in [`init`] — the launcher projects launch
///    params into the child env before `exec()`, so the env path is the
///    de-facto launch overlay today.
///
/// Returns a [`Context`] whose `source = ContextSource::Launch` so callers
/// can introspect whether the run is launch-driven.
#[cfg(feature = "std")]
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
#[cfg(feature = "std")]
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

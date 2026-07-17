//! Public types for the embedded executor.

use nros_rmw::{SessionMode, TransportError};

// ============================================================================
// SpinOnceResult
// ============================================================================

/// Result of a single spin iteration
///
/// Contains counts of how many items were processed during `spin_once()`,
/// plus error counts for transport failures that would otherwise be silently dropped.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SpinOnceResult {
    /// Number of subscription callbacks invoked
    pub subscriptions_processed: usize,
    /// Number of timers that fired
    pub timers_fired: usize,
    /// Number of service requests handled
    pub services_handled: usize,
    /// Number of subscription processing errors (e.g., BufferTooSmall, MessageTooLarge)
    pub subscription_errors: usize,
    /// Number of service processing errors (e.g., BufferTooSmall)
    pub service_errors: usize,
}

impl SpinOnceResult {
    /// Create a new empty result
    pub const fn new() -> Self {
        Self {
            subscriptions_processed: 0,
            timers_fired: 0,
            services_handled: 0,
            subscription_errors: 0,
            service_errors: 0,
        }
    }

    /// Check if any work was done (errors are not counted as work)
    pub const fn any_work(&self) -> bool {
        self.subscriptions_processed > 0 || self.timers_fired > 0 || self.services_handled > 0
    }

    /// Total number of callbacks successfully invoked (errors excluded)
    pub const fn total(&self) -> usize {
        self.subscriptions_processed + self.timers_fired + self.services_handled
    }

    /// Check if any errors occurred during this spin iteration
    pub const fn any_errors(&self) -> bool {
        self.subscription_errors > 0 || self.service_errors > 0
    }

    /// Total number of errors across all handle types
    pub const fn total_errors(&self) -> usize {
        self.subscription_errors + self.service_errors
    }
}

// ============================================================================
// SpinPeriodPollingResult (no_std)
// ============================================================================

/// Result from a single period of polling execution (`no_std` compatible).
///
/// Contains the work performed and the remaining time the caller should sleep.
/// The caller is responsible for the actual delay (platform-specific).
///
/// # Example
///
/// ```ignore
/// loop {
///     let r = executor.spin_one_period(10, elapsed_ms);
///     platform_sleep_ms(r.remaining_ms);
/// }
/// ```
#[derive(Debug, Clone, Copy)]
pub struct SpinPeriodPollingResult {
    /// Work performed during this iteration
    pub work: SpinOnceResult,
    /// Remaining time in ms that the caller should sleep
    pub remaining_ms: u64,
}

// ============================================================================
// SpinPeriodResult (std only)
// ============================================================================

/// Result from a single period with wall-clock measurement (`std` only).
///
/// Contains the work performed, whether processing exceeded the period
/// (overrun), and the actual wall-clock processing time.
#[cfg(feature = "std")]
#[derive(Debug, Clone)]
pub struct SpinPeriodResult {
    /// Work performed during this period
    pub work: SpinOnceResult,
    /// Whether processing exceeded the target period
    pub overrun: bool,
    /// Actual wall-clock processing time
    pub elapsed: std::time::Duration,
}

// ============================================================================
// SpinOptions
// ============================================================================

/// Options controlling blocking spin behavior.
///
/// Used with `Executor::spin_blocking`
/// to control when the spin loop exits.
#[derive(Debug, Clone, Default)]
pub struct SpinOptions {
    /// Stop after this duration (in milliseconds)
    pub timeout_ms: Option<u64>,
    /// Only process immediately available work (single iteration)
    pub only_next: bool,
    /// Stop after processing this many callbacks total
    pub max_callbacks: Option<usize>,
}

impl SpinOptions {
    /// Create default spin options (spin forever until halted)
    pub const fn new() -> Self {
        Self {
            timeout_ms: None,
            only_next: false,
            max_callbacks: None,
        }
    }

    /// Set a timeout duration
    pub const fn timeout_ms(mut self, ms: u64) -> Self {
        self.timeout_ms = Some(ms);
        self
    }

    /// Only process one round of work (equivalent to spin_once)
    pub const fn spin_once() -> Self {
        Self {
            timeout_ms: None,
            only_next: true,
            max_callbacks: None,
        }
    }

    /// Stop after processing N callbacks
    pub const fn max_callbacks(mut self, n: usize) -> Self {
        self.max_callbacks = Some(n);
        self
    }
}

// ============================================================================
// Configuration constants and defaults
// ============================================================================

/// Default middleware locator for hosted environments.
#[cfg(feature = "std")]
const DEFAULT_LOCATOR: &str = "tcp/127.0.0.1:7447";

/// RFC-0045 / issue #206 — maximum valid ROS 2 domain ID. The ROS 2 / DDS
/// convention (RTPS port arithmetic) caps usable domains at 232; values
/// above it are a configuration error in EVERY language front-end (never a
/// silent clamp or silent 0). Mirrored into the generated C header — keep
/// the mirror in sync (the #160 drift class).
pub const DOMAIN_ID_MAX: u32 = 232;

/// Issue #227 — C/C++-ABI escape for an EXPLICIT domain 0.
///
/// The C/C++ init surface carries `domain_id` as a `u8` where `0` is the
/// UNSET sentinel (the #206 model-A / ROS-convention decision: unset defers
/// to env > baked macro > default). That makes a literal domain 0
/// unreachable once an image bakes a nonzero `NROS_ENTRY_DOMAIN_ID`. Since
/// valid domains cap at [`DOMAIN_ID_MAX`] (232), the value 255 is free:
/// passing it means "explicitly domain 0 — do NOT treat as unset". Hosted
/// env still overrides it (model A), like every other explicit argument.
pub const DOMAIN_ID_EXPLICIT_ZERO_C_ABI: u8 = 255;

/// Map the C/C++ ABI's `u8` domain argument onto the resolver's baked rung.
///
/// `0` → `None` (unset; the ladder decides), 255
/// ([`DOMAIN_ID_EXPLICIT_ZERO_C_ABI`]) → `Some(0)` (explicit zero), anything
/// else → `Some(n)`. Values in `233..=254` pass through so
/// [`ExecutorConfig::try_resolve`] rejects them loudly
/// ([`BootConfigError::DomainIdRange`]) instead of this edge inventing its
/// own validation.
pub fn baked_domain_from_c_abi(raw: u8) -> Option<u32> {
    match raw {
        0 => None,
        DOMAIN_ID_EXPLICIT_ZERO_C_ABI => Some(0),
        n => Some(n as u32),
    }
}

/// RFC-0045 / issue #206 — boot-config resolution error. Malformed or
/// out-of-range identity input (env or baked) is an ERROR, never a silent
/// fallback: a typo'd `ROS_DOMAIN_ID` must not invisibly move a node to
/// domain 0 (the pre-#206 C++ behavior) or be silently ignored (the
/// pre-#206 behavior of this resolver).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootConfigError {
    /// `ROS_DOMAIN_ID` env var set but not a decimal integer.
    DomainIdParse,
    /// Domain ID (env or baked) exceeds [`DOMAIN_ID_MAX`].
    DomainIdRange,
}

impl core::fmt::Display for BootConfigError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BootConfigError::DomainIdParse => {
                write!(f, "ROS_DOMAIN_ID is set but is not a decimal integer")
            }
            BootConfigError::DomainIdRange => {
                write!(f, "domain id exceeds DOMAIN_ID_MAX ({DOMAIN_ID_MAX})")
            }
        }
    }
}

// ============================================================================
// ExecutorConfig
// ============================================================================

/// Configuration for opening an embedded executor session.
///
/// Provides a backend-agnostic builder for configuring the middleware
/// connection. The active Cargo feature (`rmw-zenoh`, `rmw-xrce`, or
/// `rmw-cffi`) determines which backend is used.
///
/// # Example
///
/// ```ignore
/// use nros::prelude::*;
///
/// let config = ExecutorConfig::new("tcp/127.0.0.1:7447")
///     .node_name("talker")
///     .domain_id(0);
/// let mut executor: Executor = Executor::open(&config)?;
/// ```
pub struct ExecutorConfig<'a> {
    /// Middleware-specific connection string.
    pub locator: &'a str,
    /// Session mode (client or peer).
    pub mode: SessionMode,
    /// ROS 2 domain ID.
    pub domain_id: u32,
    /// Node name.
    pub node_name: &'a str,
    /// Node namespace.
    pub namespace: &'a str,
    /// Monotonic microsecond clock for no_std timer accounting.
    #[cfg(not(feature = "std"))]
    pub clock_us: Option<fn() -> u64>,
    /// RFC-0052 / phase-296 W3b.2 — wall-clock µs since the UNIX epoch,
    /// for `now - header.stamp` age monitors. Distinct from the monotonic
    /// `clock_us`: `header.stamp` is ROS (wall) time. `None` = no epoch
    /// source on this target — a baked `max_age_ms` contract with no
    /// epoch source is a BAKE-time error (fail-loud, never a
    /// silently-dead monitor). On `std` targets the executor falls back
    /// to `SystemTime::now()` when unset.
    pub epoch_us: Option<fn() -> u64>,
}

impl<'a> ExecutorConfig<'a> {
    /// Create a new configuration with the given locator.
    ///
    /// Defaults: `Client` mode, domain 0, node name `"node"`, empty namespace.
    pub const fn new(locator: &'a str) -> Self {
        Self {
            locator,
            mode: SessionMode::Client,
            domain_id: 0,
            node_name: "node",
            namespace: "",
            #[cfg(not(feature = "std"))]
            clock_us: None,
            epoch_us: None,
        }
    }

    /// Phase 104.C.3.3.b — `Default`-style constructor with an
    /// empty locator. Most users want `ExecutorConfig::from_env()`
    /// to pick up `ZENOH_LOCATOR` / `ROS_DOMAIN_ID`; this is the
    /// rclcpp-`NodeOptions{}` shape for callers that set every
    /// field explicitly via the chaining setters.
    pub const fn default_const() -> Self {
        Self::new("")
    }
}

impl Default for ExecutorConfig<'_> {
    fn default() -> Self {
        Self::default_const()
    }
}

impl<'a> ExecutorConfig<'a> {
    /// Set the ROS 2 domain ID.
    pub const fn domain_id(mut self, id: u32) -> Self {
        self.domain_id = id;
        self
    }

    /// Set the node name.
    pub const fn node_name(mut self, name: &'a str) -> Self {
        self.node_name = name;
        self
    }

    /// Set the node namespace.
    pub const fn namespace(mut self, ns: &'a str) -> Self {
        self.namespace = ns;
        self
    }

    /// Set the session mode.
    pub const fn mode(mut self, mode: SessionMode) -> Self {
        self.mode = mode;
        self
    }

    /// RFC-0052 W3b.2 — set the wall-clock (epoch µs) source.
    pub const fn epoch_us(mut self, epoch: fn() -> u64) -> Self {
        self.epoch_us = Some(epoch);
        self
    }

    /// Set a monotonic microsecond clock for no_std executor timers.
    #[cfg(not(feature = "std"))]
    pub const fn clock_us(mut self, clock: fn() -> u64) -> Self {
        self.clock_us = Some(clock);
        self
    }
}

#[cfg(feature = "std")]
struct EnvCache {
    locator: std::string::String,
    domain_id: u32,
    mode: SessionMode,
    /// RFC-0045 model A — `NROS_NODE_NAME` env rung (issue #206 parity).
    node_name: std::string::String,
}

#[cfg(feature = "std")]
static ENV_CACHE: std::sync::OnceLock<EnvCache> = std::sync::OnceLock::new();

#[cfg(feature = "std")]
fn env_cache() -> &'static EnvCache {
    ENV_CACHE.get_or_init(|| {
        // Prefer NROS_LOCATOR / NROS_SESSION_MODE; accept legacy ZENOH_*
        // names with a stderr deprecation warning.
        let locator = std::env::var("NROS_LOCATOR")
            .or_else(|_| {
                std::env::var("ZENOH_LOCATOR").inspect(|_| {
                    std::eprintln!("nros: ZENOH_LOCATOR is deprecated; use NROS_LOCATOR instead");
                })
            })
            .unwrap_or_else(|_| std::string::String::from(DEFAULT_LOCATOR));
        let domain_id = std::env::var("ROS_DOMAIN_ID")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let mode_str = std::env::var("NROS_SESSION_MODE")
            .or_else(|_| {
                std::env::var("ZENOH_MODE").inspect(|_| {
                    std::eprintln!("nros: ZENOH_MODE is deprecated; use NROS_SESSION_MODE instead");
                })
            })
            .ok();
        let mode = match mode_str.as_deref() {
            Some("peer") => SessionMode::Peer,
            _ => SessionMode::Client,
        };
        let node_name = std::env::var("NROS_NODE_NAME").unwrap_or_default();
        EnvCache {
            locator,
            domain_id,
            mode,
            node_name,
        }
    })
}

#[cfg(feature = "std")]
impl ExecutorConfig<'static> {
    /// Create a configuration from environment variables.
    ///
    /// Reads:
    /// - `NROS_LOCATOR` — Middleware locator (default: `"tcp/127.0.0.1:7447"`).
    ///   Legacy name `ZENOH_LOCATOR` is accepted with a deprecation warning.
    /// - `ROS_DOMAIN_ID` — ROS 2 domain ID (default: `0`).
    /// - `NROS_SESSION_MODE` — Session mode: `"client"` or `"peer"` (default:
    ///   `"client"`). Legacy name `ZENOH_MODE` is accepted with a deprecation warning.
    ///
    /// Env-var values are cached in a process-global `OnceLock` on the
    /// first call and reused for the process lifetime — repeated calls
    /// do NOT re-read the environment and do NOT accrete memory. The
    /// returned `&'static str` fields point into the cache.
    pub fn from_env() -> Self {
        let cache = env_cache();
        Self {
            locator: cache.locator.as_str(),
            mode: cache.mode,
            domain_id: cache.domain_id,
            node_name: "node",
            namespace: "",
            epoch_us: Some(std_epoch_us),
        }
    }
}

// ============================================================================
// BootConfig + ExecutorConfig::resolve  (RFC-0045)
// ============================================================================

/// Session-identity subset a caller supplies to `ExecutorConfig::resolve`.
///
/// `None` on a field means "not specified — fall through to the next
/// precedence level".  The precedence model (A) resolved by
/// [`ExecutorConfig::resolve`] is:
///
/// ```text
/// env (only if hosted_env && the var is set)
///   > baked (a Some field here)
///     > compiled default
/// ```
///
/// Fields are resolved **independently**: an env locator and a baked
/// `node_name` can both apply in the same call.
///
/// Note: `mode` (session mode) is **not** configurable through `BootConfig`.
/// `BootConfig` carries only `node_name`, `locator`, `domain_id`, and `namespace`.
/// Session mode falls through to the env-cache default (`SessionMode::Client`).
#[derive(Debug, Default, Clone, Copy)]
pub struct BootConfig<'a> {
    /// Node name override.  Maps to [`ExecutorConfig::node_name`].
    pub node_name: Option<&'a str>,
    /// Middleware locator override.  Maps to [`ExecutorConfig::locator`].
    pub locator: Option<&'a str>,
    /// ROS 2 domain ID override.  Maps to [`ExecutorConfig::domain_id`].
    pub domain_id: Option<u32>,
    /// Node namespace override.  Maps to [`ExecutorConfig::namespace`].
    pub namespace: Option<&'a str>,
}

impl<'a> ExecutorConfig<'a> {
    /// Resolve boot config under precedence model A (RFC-0045).
    ///
    /// Per-field precedence (evaluated independently):
    /// `env (hosted_env && var set) > baked > compiled default`.
    ///
    /// `hosted_env=true` enables the env-override layer (`std` only).
    /// Embedded callers always pass `false`; the env layer compiles out
    /// on `no_std` regardless of the flag value.
    ///
    /// When `hosted_env=true` the env layer is queried fresh from the
    /// process environment at call time; string storage for env-derived
    /// fields comes from the process-global [`EnvCache`] (same backing
    /// store as [`ExecutorConfig::from_env`]).  Passing
    /// `BootConfig::default()` with `hosted_env=true` is therefore
    /// equivalent to calling `from_env()` directly.
    ///
    /// **Note on env coupling:** The env-presence checks below
    /// (`NROS_LOCATOR`/`ZENOH_LOCATOR`/`ROS_DOMAIN_ID`) must stay in sync
    /// with the env vars that [`env_cache()`] reads. If a new locator or
    /// domain env var is added there, add the corresponding presence check
    /// here too.
    pub fn resolve(baked: BootConfig<'a>, hosted_env: bool) -> ExecutorConfig<'a> {
        match Self::try_resolve(baked, hosted_env) {
            Ok(cfg) => cfg,
            // Fail-loud (repo rule): invalid identity config at boot is a
            // configuration error, never a silent domain-0 node. FFI shims
            // that need an error code call `try_resolve` directly.
            Err(e) => panic!("nros boot-config resolution failed: {e}"),
        }
    }

    /// RFC-0045 / issue #206 — fallible resolve. Same precedence model A as
    /// [`resolve`](Self::resolve); returns [`BootConfigError`] instead of
    /// panicking on malformed / out-of-range identity input, so the C / C++
    /// FFI shims can surface a return code. Validation is uniform across
    /// languages:
    ///
    /// - `ROS_DOMAIN_ID` set but non-numeric → `DomainIdParse` (the pre-#206
    ///   C++ header silently collapsed this to domain 0; this resolver
    ///   silently ignored it — both were wrong).
    /// - any resolved domain id > [`DOMAIN_ID_MAX`] → `DomainIdRange`
    ///   (including a BAKED value — the DDS backend would only fail later).
    /// - `NROS_NODE_NAME` joins the hosted env rung (model A parity).
    pub fn try_resolve(
        baked: BootConfig<'a>,
        hosted_env: bool,
    ) -> Result<ExecutorConfig<'a>, BootConfigError> {
        // ── hosted path (std only) ──────────────────────────────────────────
        #[cfg(feature = "std")]
        if hosted_env {
            let env = env_cache();

            // Check current process environment (fresh read, not cached) so
            // tests that set/unset vars with EnvGuard see the correct result
            // even when the OnceLock was pre-initialized by an earlier test.
            let locator_from_env =
                std::env::var("NROS_LOCATOR").is_ok() || std::env::var("ZENOH_LOCATOR").is_ok();
            let domain_id_from_env = match std::env::var("ROS_DOMAIN_ID") {
                Ok(s) if !s.is_empty() => {
                    // #206 — malformed is an ERROR, not a silent skip.
                    let v = s
                        .trim()
                        .parse::<u32>()
                        .map_err(|_| BootConfigError::DomainIdParse)?;
                    if v > DOMAIN_ID_MAX {
                        return Err(BootConfigError::DomainIdRange);
                    }
                    true
                }
                _ => false,
            };
            let node_name_from_env = std::env::var("NROS_NODE_NAME")
                .map(|s| !s.is_empty())
                .unwrap_or(false);

            let domain_id = if domain_id_from_env {
                env.domain_id
            } else {
                baked.domain_id.unwrap_or(0)
            };
            if domain_id > DOMAIN_ID_MAX {
                return Err(BootConfigError::DomainIdRange);
            }

            return Ok(ExecutorConfig {
                locator: if locator_from_env {
                    // String value from cache (same source as from_env()).
                    env.locator.as_str()
                } else if let Some(l) = baked.locator {
                    l
                } else {
                    // Hosted compiled default matches from_env()'s fallback.
                    DEFAULT_LOCATOR
                },
                mode: env.mode,
                domain_id,
                node_name: if node_name_from_env {
                    env.node_name.as_str()
                } else {
                    baked.node_name.unwrap_or("node")
                },
                namespace: baked.namespace.unwrap_or(""),
                epoch_us: Some(std_epoch_us),
            });
        }

        // ── embedded / hosted_env=false path (also compiles on no_std) ─────
        let _ = hosted_env; // suppress unused-var on no_std
        let domain_id = baked.domain_id.unwrap_or(0);
        if domain_id > DOMAIN_ID_MAX {
            return Err(BootConfigError::DomainIdRange);
        }
        Ok(ExecutorConfig {
            locator: baked.locator.unwrap_or(""),
            mode: nros_rmw::SessionMode::Client,
            domain_id,
            node_name: baked.node_name.unwrap_or("node"),
            namespace: baked.namespace.unwrap_or(""),
            #[cfg(not(feature = "std"))]
            clock_us: None,
            epoch_us: None,
        })
    }
}

// ============================================================================
// Error type
// ============================================================================

/// Error type for generic embedded node operations.
///
/// Not `Copy` — `NodeError::Transport` wraps a [`TransportError`] which
/// carries owned diagnostic strings (`Backend` / `BackendDynamic`). Rust
/// callers that matched on `NodeError` by value may need `ref` arms or
/// `.clone()`; C/C++ callers are unaffected (they see an integer
/// `nros_ret_t`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeError {
    /// Transport-level error.
    Transport(TransportError),
    /// Node name exceeds 64 bytes.
    NameTooLong,
    /// CDR serialization failed.
    Serialization,
    /// CDR deserialization failed.
    Deserialization,
    /// Buffer too small for message.
    BufferTooSmall,
    /// Action server/client creation failed.
    ActionCreationFailed,
    /// Service request failed.
    ServiceRequestFailed,
    /// Service reply failed.
    ServiceReplyFailed,
    /// Operation timed out.
    Timeout,
    /// A required subsystem has not been initialized (e.g. parameter
    /// services have not been registered on the executor).
    NotInitialized,
    /// The client / action client already has a request in flight that
    /// hasn't been consumed. Phase 84.D3: fixes the hazard where dropping
    /// a `Promise` without awaiting its reply left the stale reply queued
    /// to be delivered to the *next* call. Resolve by either polling the
    /// existing promise to completion or calling `reset_in_flight()`.
    RequestInFlight,
    /// Phase 110.B — `create_sched_context` ran out of slots
    /// (`MAX_SC` exceeded).
    NoSchedContextSlot,
    /// Phase 110.B — `bind_handle_to_sched_context` was called with
    /// an out-of-range handle, an empty entry slot, or an unknown
    /// `SchedContextId`.
    InvalidSchedContextBinding,
    /// Phase 104.C.2 — `Executor::node_builder(...).build()` was
    /// called when the per-Executor node table is full
    /// (`NROS_EXECUTOR_MAX_NODES` reached).
    NodeTableFull,
    /// Issue 0095 — the executor's fixed callback-entry table is full
    /// (`NROS_EXECUTOR_MAX_CBS`, default 4): a timer / subscription / service /
    /// action could not claim a slot. Distinct from `BufferTooSmall` so the
    /// register seam can name the knob.
    ExecutorFull,
    /// Phase 104.C.2 — requested RMW backend does not match the
    /// Executor's open session (single-session restriction lifts in
    /// Phase 104.C.3 when the per-Node session cache lands).
    BackendMismatch,
}

impl From<TransportError> for NodeError {
    fn from(err: TransportError) -> Self {
        NodeError::Transport(err)
    }
}

impl From<nros_core::SerError> for NodeError {
    fn from(_: nros_core::SerError) -> Self {
        NodeError::Serialization
    }
}

impl From<nros_core::DeserError> for NodeError {
    fn from(_: nros_core::DeserError) -> Self {
        NodeError::Deserialization
    }
}

/// Default transmit buffer size (bytes).
#[cfg(any(has_rmw, test))]
pub(crate) const DEFAULT_TX_BUF: usize = crate::config::DEFAULT_RX_BUF_SIZE;

// ============================================================================
// Phase 110.A — Activator + ReadySet + Dispatcher
// ============================================================================

/// Index into the executor's `entries[]` array. Phase 110.A caps at
/// 64 to match the existing readiness bitmap width; if a future
/// MAX_HANDLES bump goes past 64 the type widens accordingly.
#[cfg(any(has_rmw, test))]
pub(crate) type DescIdx = u8;

/// Sort key used to order callbacks within a `ReadySet`.
///
/// Phase 110.A: registration-order — `sort_key` mirrors `desc_idx`
/// numerically so `FifoReadySet` preserves bit-for-bit dispatch order.
/// Phase 110.B will widen this to encode an EDF deadline ahead of
/// `desc_idx`.
#[cfg(any(has_rmw, test))]
#[allow(dead_code)] // Phase 110.A — wired in 110.A.b spin_once rewire.
pub(crate) type SortKey = u32;

/// One ready callback queued for dispatch. Stored in the `ReadySet`,
/// consumed by the `Dispatcher`. Full handle metadata (callback fn,
/// data offset, kind) is reconstructed from
/// `Executor::entries[desc_idx]` at dispatch time so the ready set
/// itself stays compact.
#[cfg(any(has_rmw, test))]
#[allow(dead_code)] // Phase 110.A — wired in 110.A.b spin_once rewire.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) struct ActiveJob {
    pub sort_key: SortKey,
    pub desc_idx: DescIdx,
}

/// How aggressively the dispatcher drains the `ReadySet`.
///
/// `Latched` (default) preserves today's `spin_once` semantics:
/// callbacks that become ready *during* dispatch wait for the next
/// cycle. `Greedy` re-runs the activator after each callback so newly
/// ready entries fire in the same cycle — soft-RT pipelines that want
/// chain-style propagation use this.
#[cfg(any(has_rmw, test))]
#[allow(dead_code)] // Phase 110.B introduces the user-facing knob.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
pub(crate) enum DrainMode {
    #[default]
    Latched,
    Greedy,
}

// ============================================================================
// HandleId
// ============================================================================

/// Opaque handle identifier returned by registration methods.
///
/// Used with [`Trigger::One`] and [`HandleSet`] for type-safe trigger
/// configuration. The inner value is the entry slot index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HandleId(pub usize);

// ============================================================================
// HandleSet
// ============================================================================

/// A set of handle IDs, represented as a bitset.
///
/// Supports up to 64 handles. Construct via `HandleId` operators:
/// ```ignore
/// let set = imu | gps | lidar;  // HandleSet from 3 handles
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HandleSet(pub(crate) u64);

impl HandleSet {
    /// Empty set.
    pub const EMPTY: Self = Self(0);

    /// Insert a handle into the set.
    pub const fn insert(self, id: HandleId) -> Self {
        Self(self.0 | (1u64 << id.0))
    }

    /// Check if the set contains a handle.
    pub const fn contains(self, id: HandleId) -> bool {
        self.0 & (1u64 << id.0) != 0
    }

    /// Union of two sets.
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Number of handles in the set.
    pub const fn len(self) -> u32 {
        self.0.count_ones()
    }

    /// Check if the set is empty.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl core::ops::BitOr for HandleId {
    type Output = HandleSet;
    fn bitor(self, rhs: HandleId) -> HandleSet {
        HandleSet::EMPTY.insert(self).insert(rhs)
    }
}

impl core::ops::BitOr<HandleId> for HandleSet {
    type Output = HandleSet;
    fn bitor(self, rhs: HandleId) -> HandleSet {
        self.insert(rhs)
    }
}

impl core::ops::BitOr for HandleSet {
    type Output = HandleSet;
    fn bitor(self, rhs: HandleSet) -> HandleSet {
        self.union(rhs)
    }
}

// ============================================================================
// ReadinessSnapshot
// ============================================================================

/// Snapshot of handle readiness at the start of a spin iteration.
///
/// Passed to [`Trigger::Predicate`] functions. Query by [`HandleId`].
pub struct ReadinessSnapshot {
    pub(crate) bits: u64,
    pub(crate) count: usize,
}

impl ReadinessSnapshot {
    /// Check if a specific handle has data.
    pub const fn is_ready(&self, id: HandleId) -> bool {
        self.bits & (1u64 << id.0) != 0
    }

    /// Check if all handles in the set have data.
    pub const fn all_ready(&self, set: HandleSet) -> bool {
        self.bits & set.0 == set.0
    }

    /// Check if any handle in the set has data.
    pub const fn any_ready(&self, set: HandleSet) -> bool {
        self.bits & set.0 != 0
    }

    /// Number of handles that have data.
    pub const fn ready_count(&self) -> u32 {
        self.bits.count_ones()
    }

    /// Total registered handles.
    pub const fn total(&self) -> usize {
        self.count
    }
}

// ============================================================================
// InvocationMode
// ============================================================================

/// Per-callback invocation mode.
///
/// Controls whether a callback fires only when new data is available
/// or on every spin iteration that passes the trigger gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InvocationMode {
    /// Fire only when `has_data()` returns true (default).
    #[default]
    OnNewData,
    /// Fire on every spin iteration, regardless of data availability.
    Always,
}

// ============================================================================
// Trigger
// ============================================================================

/// Executor-level trigger condition.
///
/// Controls when the executor dispatches callbacks during `spin_once()`.
/// The trigger is evaluated after polling the transport but before any
/// callback dispatch.
#[derive(Clone, Copy, Default)]
pub enum Trigger {
    /// Fire if any registered handle has data (default).
    #[default]
    Any,
    /// Fire only when ALL non-timer handles have data.
    All,
    /// Fire only when a specific handle has data.
    One(HandleId),
    /// Fire only when every handle in the set has data.
    AllOf(HandleSet),
    /// Fire when any handle in the set has data.
    AnyOf(HandleSet),
    /// Always fire, regardless of data availability.
    Always,
    /// Custom predicate over a readiness snapshot.
    Predicate(fn(&ReadinessSnapshot) -> bool),
    /// Custom predicate with C-compatible signature and context pointer.
    ///
    /// The callback receives a `bool` array of readiness flags (one per handle),
    /// the count of handles, and a user-provided context pointer.
    /// Used by the C API to bridge `nros_executor_trigger_t` to the Rust trigger system.
    RawPredicate {
        /// C trigger callback
        callback: unsafe extern "C" fn(
            ready: *const bool,
            count: usize,
            context: *mut core::ffi::c_void,
        ) -> bool,
        /// User-provided context pointer passed to the callback
        context: *mut core::ffi::c_void,
    },
}

// Manual Debug impl because fn pointers don't impl Debug well
impl core::fmt::Debug for Trigger {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Any => write!(f, "Any"),
            Self::All => write!(f, "All"),
            Self::One(id) => f.debug_tuple("One").field(id).finish(),
            Self::AllOf(set) => f.debug_tuple("AllOf").field(set).finish(),
            Self::AnyOf(set) => f.debug_tuple("AnyOf").field(set).finish(),
            Self::Always => write!(f, "Always"),
            Self::Predicate(_) => write!(f, "Predicate(...)"),
            Self::RawPredicate { .. } => write!(f, "RawPredicate(...)"),
        }
    }
}

// ============================================================================
// Raw callback types (for C API)
// ============================================================================

/// Raw subscription callback that receives CDR bytes without deserialization.
///
/// # Safety
/// The `data` pointer is valid for `len` bytes during the call.
pub type RawSubscriptionCallback =
    unsafe extern "C" fn(data: *const u8, len: usize, context: *mut core::ffi::c_void);

/// Raw subscription callback that also receives the incoming sample's
/// wire-level attachment (Phase 189.M3.4 — the C analog of the Rust
/// `FnMut(&[u8], &RawMessageInfo)` builder path). `attachment` is valid
/// for `attachment_len` bytes during the call; `attachment_len == 0`
/// means the sample carried no attachment. Cross-RMW bridges read the
/// `bridge_origin` tag from it.
///
/// # Safety
/// `data` is valid for `len` bytes and `attachment` for `attachment_len`
/// bytes, during the call only.
pub type RawSubscriptionInfoCallback = unsafe extern "C" fn(
    data: *const u8,
    len: usize,
    attachment: *const u8,
    attachment_len: usize,
    context: *mut core::ffi::c_void,
);

/// Phase 269 W3 — raw subscription callback that ALSO surfaces the sample's E2E
/// integrity status (CRC + sequence gap/dup) — the C/C++ component-callback
/// projection of Rust's `FnMut(&[u8], &IntegrityStatus)` (used by
/// `register_subscription_buffered_raw_safety_on`).
///
/// The executor unpacks `nros_rmw::IntegrityStatus` into three plain scalars to
/// keep this callback type free of any external-crate struct dependency:
///   * `gap`       — sequence-number gap since the last in-order sample (0 = none)
///   * `duplicate` — `true` if the sequence number was already seen
///   * `crc_valid` — `1` = CRC ok, `0` = CRC mismatch, `-1` = no CRC on the wire
///
/// Requires the `safety-e2e` feature; the C/C++ registration FFI
/// (`nros_cpp_subscription_register_validated`) is gated on the same feature.
///
/// # Safety
/// `data` is valid for `len` bytes during the call only.
#[cfg(feature = "safety-e2e")]
pub type RawSubscriptionSafetyCallback = unsafe extern "C" fn(
    data: *const u8,
    len: usize,
    gap: i64,
    duplicate: bool,
    crc_valid: i8,
    context: *mut core::ffi::c_void,
);

/// Raw service callback that receives and produces CDR bytes.
///
/// # Safety
/// - `req` is valid for `req_len` bytes
/// - `resp` is valid for `resp_cap` bytes (writable)
/// - `resp_len` is a valid pointer to write the response length
///
/// Returns `true` if the request was handled successfully.
pub type RawServiceCallback = unsafe extern "C" fn(
    req: *const u8,
    req_len: usize,
    resp: *mut u8,
    resp_cap: usize,
    resp_len: *mut usize,
    context: *mut core::ffi::c_void,
) -> bool;

/// Raw service-client response callback.
///
/// Invoked by the executor's arena dispatch when a previously-sent
/// service request has its response delivered. The C/C++ blocking
/// wrappers install a one-shot trampoline that flips a static flag;
/// async users register their own callback via the C API.
///
/// # Safety
/// - `data` is valid for `len` bytes during the call.
pub type RawResponseCallback =
    unsafe extern "C" fn(data: *const u8, len: usize, context: *mut core::ffi::c_void);

/// Raw action goal callback that receives CDR bytes without deserialization.
///
/// # Safety
/// - `goal_id` is valid for the duration of the call
/// - `goal_data` is valid for `goal_len` bytes
///
/// Returns a `GoalResponse` value (0=Reject, 1=AcceptAndExecute, 2=AcceptAndDefer).
pub type RawGoalCallback = unsafe extern "C" fn(
    goal_id: *const nros_core::GoalId,
    goal_data: *const u8,
    goal_len: usize,
    context: *mut core::ffi::c_void,
) -> nros_core::GoalResponse;

/// Raw accepted-goal hook.
///
/// Called immediately after the accept reply has been sent to the client
/// (i.e. after `ActionServerCore::accept_goal`). Used by the C API so that
/// the user's `accepted_callback` can run *after* the client has observed
/// the accept, without blocking the accept reply on a long-running
/// execution inside the goal-decision callback.
///
/// # Safety
/// - `goal_id` is valid for the duration of the call.
pub type RawAcceptedCallback =
    unsafe extern "C" fn(goal_id: *const nros_core::GoalId, context: *mut core::ffi::c_void);

/// Raw action cancel callback.
///
/// # Safety
/// - `goal_id` is valid for the duration of the call
///
/// Returns a `CancelResponse` value (0=Ok, 1=Rejected, 2=UnknownGoal, 3=GoalTerminated).
pub type RawCancelCallback = unsafe extern "C" fn(
    goal_id: *const nros_core::GoalId,
    status: nros_core::GoalStatus,
    context: *mut core::ffi::c_void,
) -> nros_core::CancelResponse;

/// Raw action client goal-response callback.
///
/// Called when the action server accepts or rejects a goal.
///
/// # Safety
/// - `goal_id` is valid for the duration of the call
///
/// `accepted` is `true` if the goal was accepted, `false` if rejected.
pub type RawGoalResponseCallback = unsafe extern "C" fn(
    goal_id: *const nros_core::GoalId,
    accepted: bool,
    context: *mut core::ffi::c_void,
);

/// Raw action client result callback.
///
/// Called when the action result is received.
///
/// # Safety
/// - `goal_id` is valid for the duration of the call
/// - `result_data` points to `result_len` valid bytes (CDR-encoded result)
pub type RawResultCallback = unsafe extern "C" fn(
    goal_id: *const nros_core::GoalId,
    status: nros_core::GoalStatus,
    result_data: *const u8,
    result_len: usize,
    context: *mut core::ffi::c_void,
);

/// Raw action client feedback callback.
///
/// Called when feedback is received for an active goal.
///
/// # Safety
/// - `goal_id` is valid for the duration of the call
/// - `feedback_data` points to `feedback_len` valid bytes (CDR-encoded feedback)
pub type RawFeedbackCallback = unsafe extern "C" fn(
    goal_id: *const nros_core::GoalId,
    feedback_data: *const u8,
    feedback_len: usize,
    context: *mut core::ffi::c_void,
);

// ============================================================================
// ExecutorSemantics
// ============================================================================

/// Data communication semantics for the executor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExecutorSemantics {
    /// Standard interleaved execution (default). Each callback sees the
    /// latest data at the time it runs.
    #[default]
    RclcppExecutor,
    /// Logical Execution Time. All subscriptions are sampled at spin start;
    /// callbacks process from the snapshot.
    LogicalExecutionTime,
}

// ============================================================================
// GuardConditionHandle
// ============================================================================

/// Handle for triggering a guard condition from outside the executor.
///
/// Obtained from `Executor::register_guard_condition`.
/// Safe to use from any thread — the inner `&'static AtomicBool` is inherently
/// `Send + Sync`.
pub struct GuardConditionHandle {
    // The AtomicBool lives in the executor's arena, which is never moved or
    // deallocated while handles exist. The 'static lifetime is asserted at
    // construction time (see `new()`).
    flag: &'static portable_atomic::AtomicBool,
    /// Phase 124.B.5 — runtime wake callback. On std + rmw-cffi
    /// builds the executor sets this to `nros_rmw_runtime_wake_cb`
    /// with `ctx` pointing at the executor's WakeCtx. `trigger`
    /// invokes it after writing the arena flag so a `spin_once`
    /// blocked on `wake_cv` resumes immediately (sub-poll wake
    /// latency). Bare/no-std builds leave it `None` — the arena
    /// flag is observed on the next spin iteration as before.
    wake_cb: Option<unsafe extern "C" fn(ctx: *mut core::ffi::c_void)>,
    wake_ctx: *mut core::ffi::c_void,
}

// SAFETY: `wake_cb` is a plain function pointer; `wake_ctx` points
// at a WakeCtx Arc allocated on Executor::new and never freed before
// Executor::drop. Both are safe to share across threads.
unsafe impl Send for GuardConditionHandle {}
unsafe impl Sync for GuardConditionHandle {}

impl GuardConditionHandle {
    /// Create a handle from a raw pointer to an arena-allocated `AtomicBool`.
    ///
    /// # Safety
    ///
    /// The pointed-to `AtomicBool` must outlive this handle. This is guaranteed
    /// when the `AtomicBool` lives in the executor arena (which is never moved
    /// or deallocated while handles exist).
    #[cfg(any(has_rmw, test))]
    pub(crate) unsafe fn new(flag: *const portable_atomic::AtomicBool) -> Self {
        // SAFETY: Caller guarantees the AtomicBool outlives this handle.
        Self {
            flag: unsafe { &*flag },
            wake_cb: None,
            wake_ctx: core::ptr::null_mut(),
        }
    }

    /// Phase 124.B.5 — install the executor's wake callback. Called
    /// once at handle creation; the executor passes its
    /// `nros_rmw_runtime_wake_cb` + WakeCtx pointer here so
    /// `trigger()` can signal the wake condvar from any thread / ISR.
    #[cfg(any(all(feature = "std", feature = "rmw-cffi"), test))]
    #[allow(dead_code)] // Wired by register_guard_condition under cfg.
    pub(crate) fn set_wake_cb(
        &mut self,
        cb: unsafe extern "C" fn(ctx: *mut core::ffi::c_void),
        ctx: *mut core::ffi::c_void,
    ) {
        self.wake_cb = Some(cb);
        self.wake_ctx = ctx;
    }

    /// Trigger the guard condition.
    ///
    /// The executor will invoke the associated callback on the next spin iteration.
    /// On std + rmw-cffi builds, `trigger` also signals the executor's
    /// wake condvar so a blocked `spin_once` resumes immediately.
    pub fn trigger(&self) {
        self.flag.store(true, portable_atomic::Ordering::Release);
        if let Some(cb) = self.wake_cb {
            // SAFETY: cb + ctx installed by Executor::register_guard_condition;
            // ctx points at WakeCtx valid for Executor's lifetime.
            unsafe { cb(self.wake_ctx) };
        }
    }
}

// ============================================================================
// BakedBootConfig re-import — RFC-0045 "Single embedded bake site"
//
// The type and its consts live in nros-platform-api so that nros-platform's
// DeployOverlay can hold a `&'static BakedBootConfig` without a dep cycle.
// Re-imported here so BootConfig::from_baked (below) can reference the type.
// ============================================================================

// Re-export so `nros-node` consumers that used `nros_node::BakedBootConfig` etc.
// continue to compile. The nros-node lib.rs re-export uses nros_platform_api
// directly, but internal users in other nros-node submodules see these via
// `use types::*`.
pub use nros_platform_api::{
    BOOT_SET_DOMAIN, BOOT_SET_LOCATOR, BOOT_SET_NAMESPACE, BOOT_SET_NODE_NAME, BakedBootConfig,
    NROS_BOOT_CONFIG_MAGIC, NROS_BOOT_CONFIG_VERSION,
};

/// Find the length of the non-NUL prefix in `buf`.
///
/// Returns the index of the first `0` byte, or `buf.len()` if there is none.
fn nul_len(buf: &[u8]) -> usize {
    let mut i = 0;
    while i < buf.len() {
        if buf[i] == 0 {
            return i;
        }
        i += 1;
    }
    buf.len()
}

impl<'a> BootConfig<'a> {
    /// Read a baked config into the plain-field `BootConfig` the resolver consumes.
    ///
    /// Returns all-`None` (→ resolver uses compiled defaults) if `magic`/`version`
    /// don't match — defensive against a corrupt or zero-initialised section.
    ///
    /// Each `Option` is `Some` iff its `set_flags` bit is set; string values are
    /// the bytes up to the first NUL (or full buffer if no NUL).  Invalid UTF-8 in
    /// a set field is treated as unset for that field.
    pub fn from_baked(baked: &'a nros_platform_api::BakedBootConfig) -> BootConfig<'a> {
        // Validate the fingerprint.
        if baked.magic != NROS_BOOT_CONFIG_MAGIC || baked.version != NROS_BOOT_CONFIG_VERSION {
            return BootConfig::default();
        }

        let node_name = if baked.set_flags & BOOT_SET_NODE_NAME != 0 {
            let len = nul_len(&baked.node_name);
            core::str::from_utf8(&baked.node_name[..len]).ok()
        } else {
            None
        };

        let locator = if baked.set_flags & BOOT_SET_LOCATOR != 0 {
            let len = nul_len(&baked.locator);
            core::str::from_utf8(&baked.locator[..len]).ok()
        } else {
            None
        };

        let domain_id = if baked.set_flags & BOOT_SET_DOMAIN != 0 {
            Some(baked.domain_id)
        } else {
            None
        };

        let namespace = if baked.set_flags & BOOT_SET_NAMESPACE != 0 {
            let len = nul_len(&baked.namespace);
            core::str::from_utf8(&baked.namespace[..len]).ok()
        } else {
            None
        };

        BootConfig {
            node_name,
            locator,
            domain_id,
            namespace,
        }
    }
}

// ============================================================================
// BootConfig / resolve unit tests (std only)
// ============================================================================

/// RFC-0052 W3b.3 — split epoch-µs into the `builtin_interfaces/Time`
/// field pair `(sec, nanosec)` for `header.stamp` population. Types-free
/// (each workspace has its own generated `Time`); assign the tuple to the
/// struct's fields.
pub const fn epoch_us_to_stamp(us: u64) -> (i32, u32) {
    ((us / 1_000_000) as i32, ((us % 1_000_000) * 1_000) as u32)
}

/// RFC-0052 W3b.2 — hosted wall-clock source: µs since the UNIX epoch.
#[cfg(feature = "std")]
pub fn std_epoch_us() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod boot_config_tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    /// Process-wide mutex that serialises all env-touching tests.
    ///
    /// `cargo test` runs `#[test]`s in parallel within a single binary by
    /// default.  Tests that mutate `NROS_LOCATOR` / `ROS_DOMAIN_ID` must
    /// hold this lock for the duration to avoid races with each other.
    /// (`cargo nextest` runs each test in its own process so the lock is
    /// always uncontended, but taking it is still correct.)
    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    /// RAII guard that saves and restores a single env var.
    struct EnvGuard {
        key: &'static str,
        prev: Option<std::ffi::OsString>,
    }

    impl EnvGuard {
        /// Set `key` to `value`, saving the old value for restoration.
        ///
        /// # Safety
        /// Tests serialise all env mutations through [`env_lock()`].
        fn set(key: &'static str, value: &str) -> Self {
            let prev = std::env::var_os(key);
            // SAFETY: serialised via env_lock().
            unsafe { std::env::set_var(key, value) };
            Self { key, prev }
        }

        /// Remove `key` from the environment, saving its previous value.
        ///
        /// # Safety
        /// Tests serialise all env mutations through [`env_lock()`].
        fn unset(key: &'static str) -> Self {
            let prev = std::env::var_os(key);
            // SAFETY: serialised via env_lock().
            unsafe { std::env::remove_var(key) };
            Self { key, prev }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            // SAFETY: serialised via env_lock().
            unsafe {
                match &self.prev {
                    Some(v) => std::env::set_var(self.key, v),
                    None => std::env::remove_var(self.key),
                }
            }
        }
    }

    // ── T1: no-op regression — resolve(default, true) ≡ from_env() ─────────

    /// `resolve(BootConfig::default(), true)` must be field-for-field
    /// identical to `from_env()`, regardless of what env vars are set.
    #[test]
    fn noop_resolve_matches_from_env() {
        let _g = env_lock().lock().unwrap();

        let resolved = ExecutorConfig::resolve(BootConfig::default(), true);
        let env_cfg = ExecutorConfig::from_env();

        assert_eq!(resolved.locator, env_cfg.locator);
        assert_eq!(resolved.mode, env_cfg.mode);
        assert_eq!(resolved.domain_id, env_cfg.domain_id);
        assert_eq!(resolved.node_name, env_cfg.node_name);
        assert_eq!(resolved.namespace, env_cfg.namespace);
    }

    // ── T2: hosted_env=false ignores env ─────────────────────────────────────

    /// When `hosted_env=false`, env vars must be completely ignored even if
    /// they are set in the process environment.
    #[test]
    fn embedded_ignores_env_vars() {
        let _g = env_lock().lock().unwrap();
        let _e1 = EnvGuard::set("NROS_LOCATOR", "tcp/should-be-ignored:1234");
        let _e2 = EnvGuard::set("ROS_DOMAIN_ID", "99");

        let resolved = ExecutorConfig::resolve(BootConfig::default(), false);

        // Embedded compiled defaults: locator="" (ExecutorConfig::new("")),
        // domain_id=0, node_name="node", namespace="".
        assert_eq!(resolved.locator, "");
        assert_eq!(resolved.domain_id, 0);
        assert_eq!(resolved.node_name, "node");
        assert_eq!(resolved.namespace, "");
    }

    // ── T3: baked overrides compiled default (embedded path) ─────────────────

    /// Baked fields override the compiled default on the `hosted_env=false`
    /// path; unspecified baked fields keep their compiled default.
    #[test]
    fn baked_overrides_compiled_default() {
        let baked = BootConfig {
            node_name: Some("talker"),
            domain_id: Some(7),
            ..Default::default()
        };
        let resolved = ExecutorConfig::resolve(baked, false);

        assert_eq!(resolved.node_name, "talker");
        assert_eq!(resolved.domain_id, 7);
        // locator and namespace were not baked → compiled defaults.
        assert_eq!(resolved.locator, "");
        assert_eq!(resolved.namespace, "");
    }

    // ── T4: env overrides baked on hosted ────────────────────────────────────

    /// When `hosted_env=true` and `NROS_LOCATOR` is set in the process
    /// environment, the env value must win over a baked locator.
    #[test]
    fn env_overrides_baked_on_hosted() {
        let _g = env_lock().lock().unwrap();
        let _e = EnvGuard::set("NROS_LOCATOR", "tcp/env:7447");

        let baked = BootConfig {
            locator: Some("tcp/baked:9999"),
            ..Default::default()
        };
        let resolved = ExecutorConfig::resolve(baked, true);
        let env_cfg = ExecutorConfig::from_env();

        // Load-bearing assertion: baked value did NOT win.
        // `env_cache()` is a process-global OnceLock, so the exact env string
        // is only observable when this test initializes the cache first.
        // The key check is that the baked locator was not returned.
        assert_ne!(
            resolved.locator, "tcp/baked:9999",
            "baked locator must not override env"
        );

        // Secondary check: env value matches the cache (both draw from
        // the same source, so they are always consistent).
        assert_eq!(
            resolved.locator, env_cfg.locator,
            "env locator should win over baked locator"
        );
    }

    // ── T5: baked used when env var is unset on hosted ───────────────────────

    /// When `hosted_env=true` but `NROS_LOCATOR` is absent, the baked
    /// locator must be used.
    #[test]
    fn baked_used_when_env_unset_on_hosted() {
        let _g = env_lock().lock().unwrap();
        let _e1 = EnvGuard::unset("NROS_LOCATOR");
        let _e2 = EnvGuard::unset("ZENOH_LOCATOR");

        let baked = BootConfig {
            locator: Some("tcp/baked-only:8888"),
            ..Default::default()
        };
        let resolved = ExecutorConfig::resolve(baked, true);

        assert_eq!(
            resolved.locator, "tcp/baked-only:8888",
            "baked locator must be used when env var is absent"
        );
    }

    // ── T6: per-field independence ────────────────────────────────────────────

    /// A baked `node_name` and an env-derived `locator` must both apply
    /// independently in the same `resolve` call.
    #[test]
    fn per_field_independence_baked_name_env_locator() {
        let _g = env_lock().lock().unwrap();
        let _e = EnvGuard::set("NROS_LOCATOR", "tcp/env:7447");

        let baked = BootConfig {
            node_name: Some("my_talker"),
            // No locator baked — env should supply it.
            ..Default::default()
        };
        let resolved = ExecutorConfig::resolve(baked, true);
        let env_cfg = ExecutorConfig::from_env();

        // Env supplies the locator.
        assert_eq!(
            resolved.locator, env_cfg.locator,
            "env locator must apply when locator is not baked"
        );
        // Baked supplies the node name.
        assert_eq!(
            resolved.node_name, "my_talker",
            "baked node_name must apply even when locator comes from env"
        );
    }
    // ── #206 / RFC-0045 — try_resolve validation + env parity ──────────────

    #[test]
    fn try_resolve_malformed_domain_env_errors() {
        let _l = env_lock().lock().unwrap();
        let _g = EnvGuard::set("ROS_DOMAIN_ID", "not-a-number");
        let err = match ExecutorConfig::try_resolve(BootConfig::default(), true) {
            Err(e) => e,
            Ok(_) => panic!("expected DomainIdParse error"),
        };
        assert_eq!(err, BootConfigError::DomainIdParse);
    }

    #[test]
    fn try_resolve_domain_env_over_max_errors() {
        let _l = env_lock().lock().unwrap();
        let _g = EnvGuard::set("ROS_DOMAIN_ID", "233");
        let err = match ExecutorConfig::try_resolve(BootConfig::default(), true) {
            Err(e) => e,
            Ok(_) => panic!("expected DomainIdRange error"),
        };
        assert_eq!(err, BootConfigError::DomainIdRange);
    }

    #[test]
    fn try_resolve_baked_domain_over_max_errors_both_paths() {
        let _l = env_lock().lock().unwrap();
        let _g = EnvGuard::unset("ROS_DOMAIN_ID");
        let baked = BootConfig {
            domain_id: Some(DOMAIN_ID_MAX + 1),
            ..BootConfig::default()
        };
        assert!(matches!(
            ExecutorConfig::try_resolve(baked, true),
            Err(BootConfigError::DomainIdRange)
        ));
        let baked = BootConfig {
            domain_id: Some(DOMAIN_ID_MAX + 1),
            ..BootConfig::default()
        };
        assert!(
            matches!(
                ExecutorConfig::try_resolve(baked, false),
                Err(BootConfigError::DomainIdRange)
            ),
            "embedded path validates the baked value too"
        );
    }

    /// Issue #227 — the C-ABI mapping: 0 = unset, 255 = explicit zero,
    /// everything else passes through (233..=254 reach the resolver's range
    /// check and fail there, not here).
    #[test]
    fn baked_domain_from_c_abi_mapping() {
        assert_eq!(baked_domain_from_c_abi(0), None);
        assert_eq!(
            baked_domain_from_c_abi(DOMAIN_ID_EXPLICIT_ZERO_C_ABI),
            Some(0)
        );
        assert_eq!(baked_domain_from_c_abi(61), Some(61));
        assert_eq!(baked_domain_from_c_abi(232), Some(232));
        // Free-range values above DOMAIN_ID_MAX are NOT swallowed…
        assert_eq!(baked_domain_from_c_abi(233), Some(233));
        // …and the resolver rejects them loudly.
        let baked = BootConfig {
            domain_id: baked_domain_from_c_abi(233),
            ..BootConfig::default()
        };
        assert!(matches!(
            ExecutorConfig::try_resolve(baked, false),
            Err(BootConfigError::DomainIdRange)
        ));
        // Explicit zero resolves to domain 0 even on the embedded path
        // (no env rung to save it).
        let baked = BootConfig {
            domain_id: baked_domain_from_c_abi(DOMAIN_ID_EXPLICIT_ZERO_C_ABI),
            ..BootConfig::default()
        };
        let cfg = match ExecutorConfig::try_resolve(baked, false) {
            Ok(c) => c,
            Err(e) => panic!("explicit zero must resolve: {e}"),
        };
        assert_eq!(cfg.domain_id, 0);
    }

    #[test]
    fn try_resolve_domain_max_is_valid() {
        let _l = env_lock().lock().unwrap();
        let _g = EnvGuard::unset("ROS_DOMAIN_ID");
        let baked = BootConfig {
            domain_id: Some(DOMAIN_ID_MAX),
            ..BootConfig::default()
        };
        let cfg = match ExecutorConfig::try_resolve(baked, false) {
            Ok(c) => c,
            Err(e) => panic!("DOMAIN_ID_MAX must be valid: {e}"),
        };
        assert_eq!(cfg.domain_id, DOMAIN_ID_MAX);
    }

    #[test]
    fn try_resolve_node_name_env_rung() {
        let _l = env_lock().lock().unwrap();
        // NOTE: env_cache() is a process-global OnceLock — in `cargo test`
        // (shared process) another test may have initialized it before
        // NROS_NODE_NAME was set, so only the PRESENCE gate is assertable
        // process-independently. Under nextest (process per test) the value
        // asserts exactly.
        let _g = EnvGuard::set("NROS_NODE_NAME", "env_node");
        let baked = BootConfig {
            node_name: Some("baked_node"),
            ..BootConfig::default()
        };
        let cfg = match ExecutorConfig::try_resolve(baked, true) {
            Ok(c) => c,
            Err(e) => panic!("resolve failed: {e}"),
        };
        assert_ne!(cfg.node_name, "baked_node", "env rung must override baked");
    }

    #[test]
    fn try_resolve_env_overrides_baked_locator_model_a() {
        let _l = env_lock().lock().unwrap();
        let _g = EnvGuard::set("NROS_LOCATOR", "tcp/10.0.0.9:9999");
        let baked = BootConfig {
            locator: Some("tcp/1.2.3.4:1"),
            ..BootConfig::default()
        };
        let cfg = match ExecutorConfig::try_resolve(baked, true) {
            Ok(c) => c,
            Err(e) => panic!("resolve failed: {e}"),
        };
        assert_ne!(
            cfg.locator, "tcp/1.2.3.4:1",
            "model A: hosted env overrides the baked/explicit value"
        );
    }
}

// ============================================================================
// BakedBootConfig round-trip tests (no_std-compatible, run under std test runner)
//
// These test the full round-trip BakedBootConfig::new → BootConfig::from_baked.
// Pure BakedBootConfig::new / pack unit tests live in nros-platform-api.
// ============================================================================

#[cfg(test)]
mod baked_boot_config_tests {
    // BakedBootConfig + consts come from nros_platform_api via super::*.
    use super::*;

    // ── T-BB1: round-trip — typical mixed case ────────────────────────────────

    /// Pack node_name + locator + domain_id (namespace absent), then unpack and
    /// verify each field matches.  The absent namespace must be None.
    #[test]
    fn round_trip_typical() {
        let baked = BakedBootConfig::new(
            Some("param_talker"),
            Some("tcp/10.0.0.5:7447"),
            Some(7),
            None,
        );
        let cfg = BootConfig::from_baked(&baked);

        assert_eq!(cfg.node_name, Some("param_talker"));
        assert_eq!(cfg.locator, Some("tcp/10.0.0.5:7447"));
        assert_eq!(cfg.domain_id, Some(7));
        assert_eq!(cfg.namespace, None);
    }

    // ── T-BB2: all-None — no fields set ──────────────────────────────────────

    /// When every argument is None the set_flags must be zero and from_baked
    /// must return all-None.
    #[test]
    fn all_none_round_trips_to_default() {
        let baked = BakedBootConfig::new(None, None, None, None);
        assert_eq!(baked.set_flags, 0);
        let cfg = BootConfig::from_baked(&baked);
        assert_eq!(cfg.node_name, None);
        assert_eq!(cfg.locator, None);
        assert_eq!(cfg.domain_id, None);
        assert_eq!(cfg.namespace, None);
    }

    // ── T-BB3: bad magic → all-None ──────────────────────────────────────────

    /// A BakedBootConfig with a wrong magic word must be treated as unrecognised
    /// and from_baked must return all-None.
    #[test]
    fn bad_magic_returns_default() {
        let mut baked =
            BakedBootConfig::new(Some("talker"), Some("tcp/1.2.3.4:7447"), Some(1), None);
        baked.magic = 0; // corrupt the fingerprint
        let cfg = BootConfig::from_baked(&baked);
        assert_eq!(cfg.node_name, None);
        assert_eq!(cfg.locator, None);
        assert_eq!(cfg.domain_id, None);
        assert_eq!(cfg.namespace, None);
    }

    // ── T-BB4: bad version → all-None ────────────────────────────────────────

    /// A BakedBootConfig with the right magic but a wrong version must likewise
    /// return all-None.
    #[test]
    fn bad_version_returns_default() {
        let mut baked = BakedBootConfig::new(Some("talker"), None, None, None);
        baked.version = 99; // future/unknown version
        let cfg = BootConfig::from_baked(&baked);
        assert_eq!(cfg.node_name, None);
    }

    // ── T-BB5: NUL-trim — short string round-trips without trailing NULs ─────

    /// A node_name shorter than 64 bytes must unpack to exactly the original
    /// string, with no trailing NUL characters in the &str.
    #[test]
    fn nul_trim_short_name() {
        let name = "robot";
        let baked = BakedBootConfig::new(Some(name), None, None, None);
        let cfg = BootConfig::from_baked(&baked);
        assert_eq!(cfg.node_name, Some(name));
        assert_eq!(cfg.node_name.unwrap().len(), name.len());
    }

    // ── T-BB6: each field independent ────────────────────────────────────────

    /// Setting only node_name leaves the others None.
    #[test]
    fn only_node_name_set() {
        let baked = BakedBootConfig::new(Some("solo"), None, None, None);
        let cfg = BootConfig::from_baked(&baked);
        assert_eq!(cfg.node_name, Some("solo"));
        assert_eq!(cfg.locator, None);
        assert_eq!(cfg.domain_id, None);
        assert_eq!(cfg.namespace, None);
    }

    /// Setting only locator leaves the others None.
    #[test]
    fn only_locator_set() {
        let baked = BakedBootConfig::new(None, Some("tcp/127.0.0.1:7447"), None, None);
        let cfg = BootConfig::from_baked(&baked);
        assert_eq!(cfg.node_name, None);
        assert_eq!(cfg.locator, Some("tcp/127.0.0.1:7447"));
        assert_eq!(cfg.domain_id, None);
        assert_eq!(cfg.namespace, None);
    }

    /// Setting only domain_id leaves the others None.
    #[test]
    fn only_domain_id_set() {
        let baked = BakedBootConfig::new(None, None, Some(42), None);
        let cfg = BootConfig::from_baked(&baked);
        assert_eq!(cfg.node_name, None);
        assert_eq!(cfg.locator, None);
        assert_eq!(cfg.domain_id, Some(42));
        assert_eq!(cfg.namespace, None);
    }

    /// Setting only namespace leaves the others None.
    #[test]
    fn only_namespace_set() {
        let baked = BakedBootConfig::new(None, None, None, Some("/robot"));
        let cfg = BootConfig::from_baked(&baked);
        assert_eq!(cfg.node_name, None);
        assert_eq!(cfg.locator, None);
        assert_eq!(cfg.domain_id, None);
        assert_eq!(cfg.namespace, Some("/robot"));
    }

    // ── T-BB7: full-buffer-length string (boundary) ───────────────────────────

    /// A node_name of exactly 64 bytes must compile and round-trip correctly
    /// (the buffer is fully populated with no NUL terminator, so nul_len
    /// returns 64 and the whole buffer is the string value).
    #[test]
    fn full_64_byte_name_round_trips() {
        // Exactly 64 ASCII bytes.
        let name = "abcdefghijklmnopqrstuvwxyz012345abcdefghijklmnopqrstuvwxyz012345";
        assert_eq!(name.len(), 64);
        let baked = BakedBootConfig::new(Some(name), None, None, None);
        let cfg = BootConfig::from_baked(&baked);
        assert_eq!(cfg.node_name, Some(name));
    }

    // ── T-BB8: set_flags bit-pattern is exact ─────────────────────────────────

    /// Verify the set_flags bitmask matches the expected bit positions.
    #[test]
    fn set_flags_bits_correct() {
        let baked = BakedBootConfig::new(
            Some("n"), // bit 0
            None,
            Some(0),  // bit 2
            Some(""), // bit 3
        );
        assert_eq!(
            baked.set_flags,
            BOOT_SET_NODE_NAME | BOOT_SET_DOMAIN | BOOT_SET_NAMESPACE
        );
    }

    // ── Compile-failure comment ───────────────────────────────────────────────
    // Uncommenting the line below must FAIL to compile because the string
    // exceeds the 64-byte node_name buffer.  Do NOT uncomment in CI.
    //
    // const _: BakedBootConfig = BakedBootConfig::new(
    //     Some("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"), // 65 A's (> 64-byte buffer)
    //     None, None, None,
    // );
}

// ============================================================================
// Kani Verification
// ============================================================================

#[cfg(kani)]
mod verification {
    use super::*;

    // ---- HandleSet algebraic properties ----

    #[kani::proof]
    fn handleset_insert_contains() {
        let idx: usize = kani::any();
        kani::assume(idx < 64);
        let id = HandleId(idx);
        let set = HandleSet::EMPTY.insert(id);
        assert!(set.contains(id));
    }

    #[kani::proof]
    fn handleset_insert_idempotent() {
        let idx: usize = kani::any();
        kani::assume(idx < 64);
        let id = HandleId(idx);
        let once = HandleSet::EMPTY.insert(id);
        let twice = once.insert(id);
        assert_eq!(once.0, twice.0);
    }

    #[kani::proof]
    fn handleset_union_commutative() {
        let a: u64 = kani::any();
        let b: u64 = kani::any();
        let set_a = HandleSet(a);
        let set_b = HandleSet(b);
        assert_eq!(set_a.union(set_b).0, set_b.union(set_a).0);
    }

    #[kani::proof]
    fn handleset_union_associative() {
        let a: u64 = kani::any();
        let b: u64 = kani::any();
        let c: u64 = kani::any();
        let sa = HandleSet(a);
        let sb = HandleSet(b);
        let sc = HandleSet(c);
        assert_eq!(sa.union(sb).union(sc).0, sa.union(sb.union(sc)).0);
    }

    #[kani::proof]
    fn handleset_union_contains_both() {
        let idx_a: usize = kani::any();
        let idx_b: usize = kani::any();
        kani::assume(idx_a < 64);
        kani::assume(idx_b < 64);
        let a = HandleId(idx_a);
        let b = HandleId(idx_b);
        let set_a = HandleSet::EMPTY.insert(a);
        let set_b = HandleSet::EMPTY.insert(b);
        let merged = set_a.union(set_b);
        assert!(merged.contains(a));
        assert!(merged.contains(b));
    }

    #[kani::proof]
    fn handleset_empty_contains_nothing() {
        let idx: usize = kani::any();
        kani::assume(idx < 64);
        let id = HandleId(idx);
        assert!(!HandleSet::EMPTY.contains(id));
    }

    #[kani::proof]
    fn handleset_bitor_matches_insert() {
        let idx_a: usize = kani::any();
        let idx_b: usize = kani::any();
        kani::assume(idx_a < 64);
        kani::assume(idx_b < 64);
        let a = HandleId(idx_a);
        let b = HandleId(idx_b);
        let via_bitor = a | b;
        let via_insert = HandleSet::EMPTY.insert(a).insert(b);
        assert_eq!(via_bitor.0, via_insert.0);
    }

    #[kani::proof]
    fn handleset_len_after_insert() {
        let idx: usize = kani::any();
        kani::assume(idx < 64);
        let id = HandleId(idx);
        let set = HandleSet::EMPTY.insert(id);
        assert_eq!(set.len(), 1);
        assert!(!set.is_empty());
    }

    // ---- ReadinessSnapshot properties ----

    #[kani::proof]
    fn snapshot_is_ready_consistent() {
        let bits: u64 = kani::any();
        let idx: usize = kani::any();
        kani::assume(idx < 64);
        let snap = ReadinessSnapshot { bits, count: 64 };
        let id = HandleId(idx);
        // is_ready matches the bit
        assert_eq!(snap.is_ready(id), bits & (1u64 << idx) != 0);
    }

    #[kani::proof]
    fn snapshot_all_ready_correct() {
        let bits: u64 = kani::any();
        let set_bits: u64 = kani::any();
        let snap = ReadinessSnapshot { bits, count: 64 };
        let set = HandleSet(set_bits);
        // all_ready iff every bit in set is present in bits
        assert_eq!(snap.all_ready(set), bits & set_bits == set_bits);
    }

    #[kani::proof]
    fn snapshot_any_ready_correct() {
        let bits: u64 = kani::any();
        let set_bits: u64 = kani::any();
        let snap = ReadinessSnapshot { bits, count: 64 };
        let set = HandleSet(set_bits);
        // any_ready iff at least one bit overlaps
        assert_eq!(snap.any_ready(set), bits & set_bits != 0);
    }

    // ---- Trigger evaluation soundness ----

    // These verify the boolean expressions used in spin_once().

    #[kani::proof]
    fn trigger_any_fires_iff_nonzero() {
        let readiness: u64 = kani::any();
        // Trigger::Any fires when readiness_bits != 0
        let fires = readiness != 0;
        assert_eq!(fires, readiness != 0);
    }

    #[kani::proof]
    fn trigger_one_fires_iff_bit_set() {
        let readiness: u64 = kani::any();
        let idx: usize = kani::any();
        kani::assume(idx < 64);
        let id = HandleId(idx);
        // Trigger::One(id) fires when readiness & (1 << id.0) != 0
        let fires = readiness & (1u64 << id.0) != 0;
        // This is equivalent to checking the specific bit
        assert_eq!(fires, readiness & (1u64 << idx) != 0);
    }

    #[kani::proof]
    fn trigger_allof_fires_iff_all_set() {
        let readiness: u64 = kani::any();
        let set_bits: u64 = kani::any();
        let set = HandleSet(set_bits);
        // Trigger::AllOf(set) fires when readiness & set.0 == set.0
        let fires = readiness & set.0 == set.0;
        // AllOf with empty set always fires
        if set_bits == 0 {
            assert!(fires);
        }
        // If fires, then every bit in set is present
        if fires {
            assert_eq!(readiness & set_bits, set_bits);
        }
    }

    #[kani::proof]
    fn trigger_anyof_fires_iff_any_set() {
        let readiness: u64 = kani::any();
        let set_bits: u64 = kani::any();
        let set = HandleSet(set_bits);
        // Trigger::AnyOf(set) fires when readiness & set.0 != 0
        let fires = readiness & set.0 != 0;
        // AnyOf with empty set never fires
        if set_bits == 0 {
            assert!(!fires);
        }
    }

    #[kani::proof]
    fn trigger_allof_implies_anyof() {
        let readiness: u64 = kani::any();
        let set_bits: u64 = kani::any();
        kani::assume(set_bits != 0); // Non-empty set
        let allof_fires = readiness & set_bits == set_bits;
        let anyof_fires = readiness & set_bits != 0;
        // If AllOf fires, then AnyOf must also fire
        if allof_fires {
            assert!(anyof_fires);
        }
    }

    #[kani::proof]
    fn trigger_one_equivalent_to_anyof_singleton() {
        let readiness: u64 = kani::any();
        let idx: usize = kani::any();
        kani::assume(idx < 64);
        let id = HandleId(idx);
        let singleton = HandleSet::EMPTY.insert(id);
        // One(id) and AnyOf({id}) produce the same result
        let one_fires = readiness & (1u64 << id.0) != 0;
        let anyof_fires = readiness & singleton.0 != 0;
        assert_eq!(one_fires, anyof_fires);
    }
}

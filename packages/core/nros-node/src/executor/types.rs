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
/// Used with [`Executor::spin_blocking()`](super::Executor::spin_blocking)
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
/// let mut executor = Executor::open(&config)?;
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
        }
    }

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
}

#[cfg(all(feature = "std", feature = "alloc"))]
impl ExecutorConfig<'static> {
    /// Create a configuration from environment variables.
    ///
    /// Reads:
    /// - `ZENOH_LOCATOR` — Middleware locator (default: `"tcp/127.0.0.1:7447"`)
    /// - `ROS_DOMAIN_ID` — ROS 2 domain ID (default: `0`)
    /// - `ZENOH_MODE` — Session mode: `"client"` or `"peer"` (default: `"client"`)
    ///
    /// String values are heap-allocated and leaked into `'static` references.
    pub fn from_env() -> Self {
        let locator: &'static str = match std::env::var("ZENOH_LOCATOR") {
            Ok(s) => alloc::boxed::Box::leak(s.into_boxed_str()),
            Err(_) => "tcp/127.0.0.1:7447",
        };
        let domain_id: u32 = std::env::var("ROS_DOMAIN_ID")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let mode = match std::env::var("ZENOH_MODE") {
            Ok(s) if s == "peer" => SessionMode::Peer,
            _ => SessionMode::Client,
        };
        Self {
            locator,
            mode,
            domain_id,
            node_name: "node",
            namespace: "",
        }
    }
}

// ============================================================================
// Error type
// ============================================================================

/// Error type for generic embedded node operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeError {
    /// Transport-level error.
    Transport(TransportError),
    /// Node name exceeds 64 bytes.
    NameTooLong,
    /// CDR serialization failed.
    Serialization,
    /// Buffer too small for message.
    BufferTooSmall,
    /// Action server/client creation failed.
    ActionCreationFailed,
    /// Service request failed.
    ServiceRequestFailed,
    /// Service reply failed.
    ServiceReplyFailed,
}

impl From<TransportError> for NodeError {
    fn from(err: TransportError) -> Self {
        NodeError::Transport(err)
    }
}

/// Default transmit buffer size (bytes).
pub(crate) const DEFAULT_TX_BUF: usize = 1024;

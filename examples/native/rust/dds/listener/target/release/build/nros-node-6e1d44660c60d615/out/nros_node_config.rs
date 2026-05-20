/// Maximum number of executor callback slots (set via NROS_EXECUTOR_MAX_CBS, default 4).
pub const MAX_CBS: usize = 4;

/// Maximum number of `SchedContext` slots per executor (set via NROS_EXECUTOR_MAX_SC, default 8). Phase 110.B.
pub const MAX_SC: usize = 8;

/// Executor arena size in bytes (derived from MAX_CBS and RX_BUF_SIZE).
pub const ARENA_SIZE: usize = 16384;

/// Default subscription receive buffer size in bytes (set via NROS_SUBSCRIPTION_BUFFER_SIZE, default 1024).
pub const DEFAULT_RX_BUF_SIZE: usize = 1024;

/// Parameter service request/reply buffer size in bytes (set via NROS_PARAM_SERVICE_BUFFER_SIZE, default 4096).
pub const PARAM_SERVICE_BUFFER_SIZE: usize = 4096;

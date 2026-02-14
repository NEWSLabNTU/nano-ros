//! ROS 2 compatible logging utilities
//!
//! This module provides a `Logger` type that wraps the `log` crate with
//! node-specific context, matching the rclrs logging patterns.
//!
//! # Example
//!
//! ```text
//! let logger = node.logger();
//! logger.info("Node started");
//! logger.warn("Low battery");
//! logger.error("Connection failed");
//! ```
//!
//! # Conditional Logging
//!
//! The logger supports modifiers for conditional logging:
//!
//! ```
//! use nros_core::{Logger, OnceFlag};
//!
//! let logger = Logger::new("my_node");
//!
//! // Log only once (requires a static flag)
//! static LOGGED: OnceFlag = OnceFlag::new();
//! logger.info_once(&LOGGED, "This logs only once");
//!
//! // Skip the first occurrence
//! static SKIP: OnceFlag = OnceFlag::new();
//! logger.warn_skip_first(&SKIP, "This skips the first call");
//!
//! // Rate-limited logging (requires tracking elapsed time)
//! let mut last_log_ms: u64 = 0;
//! let current_time_ms: u64 = 1000; // From your clock
//! logger.info_throttle(&mut last_log_ms, current_time_ms, 1000, "Rate limited to 1Hz");
//! ```
//!
//! # Embedded Integration
//!
//! For embedded targets, this logger integrates with the `log` crate facade,
//! allowing you to use any `log`-compatible backend:
//!
//! ## Desktop/std targets
//! Use `env_logger` or similar:
//! ```text
//! env_logger::init();
//! ```
//!
//! ## Embedded targets with defmt
//! Use `defmt-log` to bridge `log` calls to `defmt` for minimal overhead:
//! ```text
//! // In Cargo.toml:
//! // defmt = "0.3"
//! // defmt-log = "0.1"
//!
//! // In your code:
//! use defmt_log as _;  // Links the logger
//! ```
//!
//! This approach stores format strings on the host (not target), making it
//! ideal for resource-constrained microcontrollers.
//!
//! ## RTIC/Embassy
//! Both frameworks commonly use `defmt` directly. The `defmt-log` bridge
//! allows nros logging to integrate seamlessly with your existing
//! `defmt` setup.

use core::sync::atomic::{AtomicBool, Ordering};

/// A flag for tracking one-time logging
///
/// Use this with `Logger::*_once()` or `Logger::*_skip_first()` methods
/// to control conditional logging. The flag should be declared as a static
/// to ensure it persists across calls.
///
/// # Example
///
/// ```
/// use nros_core::{Logger, OnceFlag};
///
/// static LOGGED: OnceFlag = OnceFlag::new();
/// let logger = Logger::new("my_node");
///
/// // Only the first call will actually log
/// for _ in 0..10 {
///     logger.info_once(&LOGGED, "This only logs once");
/// }
/// ```
pub struct OnceFlag {
    triggered: AtomicBool,
}

impl OnceFlag {
    /// Create a new once flag (not yet triggered)
    pub const fn new() -> Self {
        Self {
            triggered: AtomicBool::new(false),
        }
    }

    /// Check if this is the first time being called, and mark as triggered
    ///
    /// Returns `true` on the first call, `false` on subsequent calls.
    ///
    /// Uses load+store instead of compare_exchange for compatibility with
    /// targets lacking CAS (e.g., riscv32imc without the A extension).
    /// This is safe for nros's single-core embedded use cases.
    pub fn check_first(&self) -> bool {
        if !self.triggered.load(Ordering::Acquire) {
            self.triggered.store(true, Ordering::Release);
            true
        } else {
            false
        }
    }

    /// Check if the flag has been triggered
    pub fn is_triggered(&self) -> bool {
        self.triggered.load(Ordering::SeqCst)
    }

    /// Reset the flag to untriggered state
    pub fn reset(&self) {
        self.triggered.store(false, Ordering::SeqCst);
    }
}

impl Default for OnceFlag {
    fn default() -> Self {
        Self::new()
    }
}

// OnceFlag is Send + Sync because AtomicBool is Send + Sync
unsafe impl Send for OnceFlag {}
unsafe impl Sync for OnceFlag {}

impl core::fmt::Debug for OnceFlag {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("OnceFlag")
            .field("triggered", &self.is_triggered())
            .finish()
    }
}

/// A logger associated with a ROS node
///
/// Provides methods for logging at different severity levels.
/// The logger includes the node name in log output for context.
#[derive(Debug, Clone)]
pub struct Logger<'a> {
    /// The node name for log context
    node_name: &'a str,
}

impl<'a> Logger<'a> {
    /// Create a new logger for the given node name
    pub const fn new(node_name: &'a str) -> Self {
        Self { node_name }
    }

    /// Get the node name associated with this logger
    pub const fn node_name(&self) -> &str {
        self.node_name
    }

    /// Log a message at the DEBUG level
    #[inline]
    pub fn debug(&self, message: &str) {
        log::debug!(target: self.node_name, "{}", message);
    }

    /// Log a message at the INFO level
    #[inline]
    pub fn info(&self, message: &str) {
        log::info!(target: self.node_name, "{}", message);
    }

    /// Log a message at the WARN level
    #[inline]
    pub fn warn(&self, message: &str) {
        log::warn!(target: self.node_name, "{}", message);
    }

    /// Log a message at the ERROR level
    #[inline]
    pub fn error(&self, message: &str) {
        log::error!(target: self.node_name, "{}", message);
    }

    /// Log a message at the TRACE level
    #[inline]
    pub fn trace(&self, message: &str) {
        log::trace!(target: self.node_name, "{}", message);
    }

    // --- Once modifiers (log only first occurrence) ---

    /// Log a DEBUG message only once (first call only)
    ///
    /// The `flag` should be a static `OnceFlag` to track whether the message
    /// has already been logged.
    #[inline]
    pub fn debug_once(&self, flag: &OnceFlag, message: &str) {
        if flag.check_first() {
            self.debug(message);
        }
    }

    /// Log an INFO message only once (first call only)
    #[inline]
    pub fn info_once(&self, flag: &OnceFlag, message: &str) {
        if flag.check_first() {
            self.info(message);
        }
    }

    /// Log a WARN message only once (first call only)
    #[inline]
    pub fn warn_once(&self, flag: &OnceFlag, message: &str) {
        if flag.check_first() {
            self.warn(message);
        }
    }

    /// Log an ERROR message only once (first call only)
    #[inline]
    pub fn error_once(&self, flag: &OnceFlag, message: &str) {
        if flag.check_first() {
            self.error(message);
        }
    }

    /// Log a TRACE message only once (first call only)
    #[inline]
    pub fn trace_once(&self, flag: &OnceFlag, message: &str) {
        if flag.check_first() {
            self.trace(message);
        }
    }

    // --- Skip first modifiers (skip first occurrence, log all subsequent) ---

    /// Log a DEBUG message, skipping the first occurrence
    ///
    /// The `flag` should be a static `OnceFlag` to track whether the first
    /// call has been skipped.
    #[inline]
    pub fn debug_skip_first(&self, flag: &OnceFlag, message: &str) {
        if !flag.check_first() {
            self.debug(message);
        }
    }

    /// Log an INFO message, skipping the first occurrence
    #[inline]
    pub fn info_skip_first(&self, flag: &OnceFlag, message: &str) {
        if !flag.check_first() {
            self.info(message);
        }
    }

    /// Log a WARN message, skipping the first occurrence
    #[inline]
    pub fn warn_skip_first(&self, flag: &OnceFlag, message: &str) {
        if !flag.check_first() {
            self.warn(message);
        }
    }

    /// Log an ERROR message, skipping the first occurrence
    #[inline]
    pub fn error_skip_first(&self, flag: &OnceFlag, message: &str) {
        if !flag.check_first() {
            self.error(message);
        }
    }

    /// Log a TRACE message, skipping the first occurrence
    #[inline]
    pub fn trace_skip_first(&self, flag: &OnceFlag, message: &str) {
        if !flag.check_first() {
            self.trace(message);
        }
    }

    // --- Throttle modifiers (rate-limit logging) ---

    /// Log a DEBUG message with rate limiting
    ///
    /// # Arguments
    /// * `last_log_time` - Mutable reference to track last log time (in milliseconds)
    /// * `current_time_ms` - Current time in milliseconds (from your clock)
    /// * `interval_ms` - Minimum interval between logs in milliseconds
    /// * `message` - The message to log
    ///
    /// # Example
    /// ```text
    /// static mut LAST_LOG: u64 = 0;
    /// let now_ms = clock.now_ms();
    /// // Safety: only accessed from single thread
    /// unsafe { logger.debug_throttle(&mut LAST_LOG, now_ms, 1000, "Rate limited"); }
    /// ```
    #[inline]
    pub fn debug_throttle(
        &self,
        last_log_time: &mut u64,
        current_time_ms: u64,
        interval_ms: u64,
        message: &str,
    ) {
        if Self::should_log_throttled(last_log_time, current_time_ms, interval_ms) {
            self.debug(message);
        }
    }

    /// Log an INFO message with rate limiting
    #[inline]
    pub fn info_throttle(
        &self,
        last_log_time: &mut u64,
        current_time_ms: u64,
        interval_ms: u64,
        message: &str,
    ) {
        if Self::should_log_throttled(last_log_time, current_time_ms, interval_ms) {
            self.info(message);
        }
    }

    /// Log a WARN message with rate limiting
    #[inline]
    pub fn warn_throttle(
        &self,
        last_log_time: &mut u64,
        current_time_ms: u64,
        interval_ms: u64,
        message: &str,
    ) {
        if Self::should_log_throttled(last_log_time, current_time_ms, interval_ms) {
            self.warn(message);
        }
    }

    /// Log an ERROR message with rate limiting
    #[inline]
    pub fn error_throttle(
        &self,
        last_log_time: &mut u64,
        current_time_ms: u64,
        interval_ms: u64,
        message: &str,
    ) {
        if Self::should_log_throttled(last_log_time, current_time_ms, interval_ms) {
            self.error(message);
        }
    }

    /// Log a TRACE message with rate limiting
    #[inline]
    pub fn trace_throttle(
        &self,
        last_log_time: &mut u64,
        current_time_ms: u64,
        interval_ms: u64,
        message: &str,
    ) {
        if Self::should_log_throttled(last_log_time, current_time_ms, interval_ms) {
            self.trace(message);
        }
    }

    /// Helper to check if enough time has passed for throttled logging
    #[inline]
    fn should_log_throttled(
        last_log_time: &mut u64,
        current_time_ms: u64,
        interval_ms: u64,
    ) -> bool {
        if current_time_ms >= *last_log_time + interval_ms {
            *last_log_time = current_time_ms;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logger_creation() {
        let logger = Logger::new("test_node");
        assert_eq!(logger.node_name(), "test_node");
    }

    #[test]
    fn test_once_flag_first_call() {
        let flag = OnceFlag::new();
        assert!(!flag.is_triggered());

        // First call should return true
        assert!(flag.check_first());
        assert!(flag.is_triggered());

        // Subsequent calls should return false
        assert!(!flag.check_first());
        assert!(!flag.check_first());
    }

    #[test]
    fn test_once_flag_reset() {
        let flag = OnceFlag::new();

        assert!(flag.check_first());
        assert!(flag.is_triggered());

        flag.reset();
        assert!(!flag.is_triggered());

        // After reset, check_first should return true again
        assert!(flag.check_first());
    }

    #[test]
    fn test_once_flag_default() {
        let flag = OnceFlag::default();
        assert!(!flag.is_triggered());
    }

    #[test]
    fn test_once_flag_debug() {
        // Just verify Debug is implemented and doesn't panic
        let flag = OnceFlag::new();
        let _ = core::format_args!("{:?}", flag);

        flag.check_first();
        let _ = core::format_args!("{:?}", flag);
    }

    #[test]
    fn test_throttle_logic() {
        let mut last_log_time: u64 = 0;

        // First call after interval has passed should log
        // (initializing to 0 means "log when time >= interval")
        assert!(Logger::should_log_throttled(&mut last_log_time, 1000, 1000));
        assert_eq!(last_log_time, 1000);

        // Call at time 1500 should not log (within interval)
        assert!(!Logger::should_log_throttled(
            &mut last_log_time,
            1500,
            1000
        ));
        assert_eq!(last_log_time, 1000);

        // Call at time 1999 should not log (within interval)
        assert!(!Logger::should_log_throttled(
            &mut last_log_time,
            1999,
            1000
        ));
        assert_eq!(last_log_time, 1000);

        // Call at time 2000 should log (interval passed)
        assert!(Logger::should_log_throttled(&mut last_log_time, 2000, 1000));
        assert_eq!(last_log_time, 2000);

        // Call at time 2500 should not log (within new interval)
        assert!(!Logger::should_log_throttled(
            &mut last_log_time,
            2500,
            1000
        ));
        assert_eq!(last_log_time, 2000);

        // Call at time 3000 should log (new interval passed)
        assert!(Logger::should_log_throttled(&mut last_log_time, 3000, 1000));
        assert_eq!(last_log_time, 3000);
    }

    #[test]
    fn test_throttle_zero_interval() {
        let mut last_log_time: u64 = 0;

        // With zero interval, every call should log
        assert!(Logger::should_log_throttled(&mut last_log_time, 0, 0));
        assert!(Logger::should_log_throttled(&mut last_log_time, 0, 0));
        assert!(Logger::should_log_throttled(&mut last_log_time, 1, 0));
    }
}

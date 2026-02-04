//! ROS 2 compatible logging utilities
//!
//! This module provides a `Logger` type that wraps the `log` crate with
//! node-specific context, matching the rclrs logging patterns.
//!
//! # Example
//!
//! ```ignore
//! let logger = node.logger();
//! logger.info("Node started");
//! logger.warn("Low battery");
//! logger.error("Connection failed");
//! ```
//!
//! # Embedded Integration
//!
//! For embedded targets, this logger integrates with the `log` crate facade,
//! allowing you to use any `log`-compatible backend:
//!
//! ## Desktop/std targets
//! Use `env_logger` or similar:
//! ```ignore
//! env_logger::init();
//! ```
//!
//! ## Embedded targets with defmt
//! Use `defmt-log` to bridge `log` calls to `defmt` for minimal overhead:
//! ```ignore
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
//! allows nano-ros logging to integrate seamlessly with your existing
//! `defmt` setup.

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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logger_creation() {
        let logger = Logger::new("test_node");
        assert_eq!(logger.node_name(), "test_node");
    }
}

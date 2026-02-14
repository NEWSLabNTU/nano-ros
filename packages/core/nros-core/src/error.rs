//! Unified error types for nros
//!
//! This module provides comprehensive error types that align with rclrs patterns
//! while maintaining `no_std` compatibility for embedded systems.
//!
//! # Error Types
//!
//! - [`NanoRosError`] - Main unified error type
//! - [`RclReturnCode`] - RCL-compatible error codes
//! - [`ErrorContext`] - Context information (topic, service, node names)
//! - [`NanoRosErrorFilter`] - Trait for filtering expected errors
//! - [`TakeFailedAsNone`] - Trait for converting take failures to `Option`
//!
//! # Example
//!
//! ```
//! use nros_core::{NanoRosError, RclReturnCode};
//!
//! fn publish_message() -> Result<(), NanoRosError> {
//!     Err(NanoRosError::timeout())
//! }
//!
//! let result = publish_message();
//! assert!(result.unwrap_err().is_timeout());
//! ```

use core::fmt;
use nros_serdes::{DeserError, SerError};

/// RCL-compatible return codes
///
/// These codes match the RCL C library return codes for interoperability.
/// Most codes are organized by category (1xx for init, 2xx for node, etc.).
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RclReturnCode {
    /// Success
    Ok = 0,
    /// Unspecified error
    Error = 1,
    /// Timeout occurred
    Timeout = 2,
    /// Unsupported operation
    Unsupported = 3,
    /// Failed to allocate memory
    BadAlloc = 10,
    /// Argument to function was invalid
    InvalidArgument = 11,

    // 1xx: Initialization errors
    /// Already initialized
    AlreadyInit = 100,
    /// Not yet initialized
    NotInit = 101,
    /// Topic name does not pass validation
    TopicNameInvalid = 103,
    /// Service name does not pass validation
    ServiceNameInvalid = 104,
    /// Already shutdown
    AlreadyShutdown = 106,

    // 2xx: Node errors
    /// Invalid node
    NodeInvalid = 200,
    /// Invalid node name
    NodeInvalidName = 201,
    /// Invalid node namespace
    NodeInvalidNamespace = 202,

    // 3xx: Publisher errors
    /// Invalid publisher
    PublisherInvalid = 300,

    // 4xx: Subscription errors
    /// Invalid subscription
    SubscriptionInvalid = 400,
    /// Failed to take a message from the subscription
    SubscriptionTakeFailed = 401,

    // 5xx: Client errors
    /// Invalid client
    ClientInvalid = 500,
    /// Failed to take a response from the client
    ClientTakeFailed = 501,

    // 6xx: Service errors
    /// Invalid service
    ServiceInvalid = 600,
    /// Failed to take a request from the service
    ServiceTakeFailed = 601,

    // 8xx: Timer errors
    /// Invalid timer
    TimerInvalid = 800,
    /// Timer was canceled
    TimerCanceled = 801,

    // 21xx: Action errors
    /// Action goal accepted
    ActionGoalAccepted = 2100,
    /// Action goal rejected
    ActionGoalRejected = 2101,
    /// Action client is invalid
    ActionClientInvalid = 2102,
    /// Action client failed to take response
    ActionClientTakeFailed = 2103,
    /// Action server is invalid
    ActionServerInvalid = 2200,
    /// Action server failed to take request
    ActionServerTakeFailed = 2201,
    /// Action goal handle invalid
    ActionGoalHandleInvalid = 2300,
}

impl RclReturnCode {
    /// Returns the numeric value of this return code
    pub const fn as_i32(self) -> i32 {
        self as i32
    }

    /// Try to convert from an i32 value
    pub fn try_from_i32(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::Ok),
            1 => Some(Self::Error),
            2 => Some(Self::Timeout),
            3 => Some(Self::Unsupported),
            10 => Some(Self::BadAlloc),
            11 => Some(Self::InvalidArgument),
            100 => Some(Self::AlreadyInit),
            101 => Some(Self::NotInit),
            103 => Some(Self::TopicNameInvalid),
            104 => Some(Self::ServiceNameInvalid),
            106 => Some(Self::AlreadyShutdown),
            200 => Some(Self::NodeInvalid),
            201 => Some(Self::NodeInvalidName),
            202 => Some(Self::NodeInvalidNamespace),
            300 => Some(Self::PublisherInvalid),
            400 => Some(Self::SubscriptionInvalid),
            401 => Some(Self::SubscriptionTakeFailed),
            500 => Some(Self::ClientInvalid),
            501 => Some(Self::ClientTakeFailed),
            600 => Some(Self::ServiceInvalid),
            601 => Some(Self::ServiceTakeFailed),
            800 => Some(Self::TimerInvalid),
            801 => Some(Self::TimerCanceled),
            2100 => Some(Self::ActionGoalAccepted),
            2101 => Some(Self::ActionGoalRejected),
            2102 => Some(Self::ActionClientInvalid),
            2103 => Some(Self::ActionClientTakeFailed),
            2200 => Some(Self::ActionServerInvalid),
            2201 => Some(Self::ActionServerTakeFailed),
            2300 => Some(Self::ActionGoalHandleInvalid),
            _ => None,
        }
    }
}

impl fmt::Display for RclReturnCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            Self::Ok => "Operation successful (RCL_RET_OK)",
            Self::Error => "Unspecified error (RCL_RET_ERROR)",
            Self::Timeout => "Timeout occurred (RCL_RET_TIMEOUT)",
            Self::Unsupported => "Unsupported operation (RCL_RET_UNSUPPORTED)",
            Self::BadAlloc => "Failed to allocate memory (RCL_RET_BAD_ALLOC)",
            Self::InvalidArgument => "Invalid argument (RCL_RET_INVALID_ARGUMENT)",
            Self::AlreadyInit => "Already initialized (RCL_RET_ALREADY_INIT)",
            Self::NotInit => "Not initialized (RCL_RET_NOT_INIT)",
            Self::TopicNameInvalid => "Invalid topic name (RCL_RET_TOPIC_NAME_INVALID)",
            Self::ServiceNameInvalid => "Invalid service name (RCL_RET_SERVICE_NAME_INVALID)",
            Self::AlreadyShutdown => "Already shutdown (RCL_RET_ALREADY_SHUTDOWN)",
            Self::NodeInvalid => "Invalid node (RCL_RET_NODE_INVALID)",
            Self::NodeInvalidName => "Invalid node name (RCL_RET_NODE_INVALID_NAME)",
            Self::NodeInvalidNamespace => "Invalid node namespace (RCL_RET_NODE_INVALID_NAMESPACE)",
            Self::PublisherInvalid => "Invalid publisher (RCL_RET_PUBLISHER_INVALID)",
            Self::SubscriptionInvalid => "Invalid subscription (RCL_RET_SUBSCRIPTION_INVALID)",
            Self::SubscriptionTakeFailed => {
                "Failed to take message (RCL_RET_SUBSCRIPTION_TAKE_FAILED)"
            }
            Self::ClientInvalid => "Invalid client (RCL_RET_CLIENT_INVALID)",
            Self::ClientTakeFailed => "Failed to take response (RCL_RET_CLIENT_TAKE_FAILED)",
            Self::ServiceInvalid => "Invalid service (RCL_RET_SERVICE_INVALID)",
            Self::ServiceTakeFailed => "Failed to take request (RCL_RET_SERVICE_TAKE_FAILED)",
            Self::TimerInvalid => "Invalid timer (RCL_RET_TIMER_INVALID)",
            Self::TimerCanceled => "Timer was canceled (RCL_RET_TIMER_CANCELED)",
            Self::ActionGoalAccepted => "Action goal accepted (RCL_RET_ACTION_GOAL_ACCEPTED)",
            Self::ActionGoalRejected => "Action goal rejected (RCL_RET_ACTION_GOAL_REJECTED)",
            Self::ActionClientInvalid => "Invalid action client (RCL_RET_ACTION_CLIENT_INVALID)",
            Self::ActionClientTakeFailed => {
                "Action client take failed (RCL_RET_ACTION_CLIENT_TAKE_FAILED)"
            }
            Self::ActionServerInvalid => "Invalid action server (RCL_RET_ACTION_SERVER_INVALID)",
            Self::ActionServerTakeFailed => {
                "Action server take failed (RCL_RET_ACTION_SERVER_TAKE_FAILED)"
            }
            Self::ActionGoalHandleInvalid => {
                "Invalid action goal handle (RCL_RET_ACTION_GOAL_HANDLE_INVALID)"
            }
        };
        write!(f, "{}", msg)
    }
}

/// Main error type for nros operations
///
/// This error type provides comprehensive coverage of all failure modes in nros,
/// with optional context information (topic name, service name, etc.) when available.
///
/// # Error Categories
///
/// - **Serialization**: CDR encoding/decoding failures
/// - **Transport**: Network and communication failures
/// - **Node/Context**: Node creation and management failures
/// - **Publisher/Subscriber**: Pub/sub failures
/// - **Service/Client**: Service call failures
/// - **Action**: Action server/client failures
/// - **Timer**: Timer management failures
/// - **Parameter**: Parameter declaration/access failures
///
/// # Example
///
/// ```
/// use nros_core::NanoRosError;
///
/// let err = NanoRosError::timeout();
/// assert!(err.is_timeout());
/// assert!(!err.is_take_failed());
///
/// // Errors with context
/// let err = NanoRosError::topic_name_invalid("/invalid topic!");
/// assert!(err.context().is_some());
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NanoRosError {
    /// The error code
    code: RclReturnCode,
    /// Optional context (topic name, service name, node name, etc.)
    context: Option<ErrorContext>,
    /// Nested error for serialization/deserialization failures
    nested: Option<NestedError>,
}

/// Context information for errors
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorContext {
    /// Topic name that caused the error
    Topic(&'static str),
    /// Service name that caused the error
    Service(&'static str),
    /// Node name that caused the error
    Node(&'static str),
    /// Action name that caused the error
    Action(&'static str),
    /// Timer ID that caused the error
    Timer(usize),
    /// Parameter name that caused the error
    Parameter(&'static str),
    /// Custom context message
    Custom(&'static str),
}

impl fmt::Display for ErrorContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Topic(name) => write!(f, "topic '{}'", name),
            Self::Service(name) => write!(f, "service '{}'", name),
            Self::Node(name) => write!(f, "node '{}'", name),
            Self::Action(name) => write!(f, "action '{}'", name),
            Self::Timer(id) => write!(f, "timer {}", id),
            Self::Parameter(name) => write!(f, "parameter '{}'", name),
            Self::Custom(msg) => write!(f, "{}", msg),
        }
    }
}

/// Nested error for wrapping serialization/deserialization errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NestedError {
    /// Serialization error
    Ser(SerError),
    /// Deserialization error
    Deser(DeserError),
}

impl NanoRosError {
    // === Constructors ===

    /// Create a new error with the given code
    pub const fn new(code: RclReturnCode) -> Self {
        Self {
            code,
            context: None,
            nested: None,
        }
    }

    /// Create a new error with context
    pub const fn with_context(code: RclReturnCode, context: ErrorContext) -> Self {
        Self {
            code,
            context: Some(context),
            nested: None,
        }
    }

    /// Create a timeout error
    pub const fn timeout() -> Self {
        Self::new(RclReturnCode::Timeout)
    }

    /// Create an invalid argument error
    pub const fn invalid_argument() -> Self {
        Self::new(RclReturnCode::InvalidArgument)
    }

    /// Create an unsupported operation error
    pub const fn unsupported() -> Self {
        Self::new(RclReturnCode::Unsupported)
    }

    /// Create an allocation failure error
    pub const fn bad_alloc() -> Self {
        Self::new(RclReturnCode::BadAlloc)
    }

    /// Create a generic error
    pub const fn error() -> Self {
        Self::new(RclReturnCode::Error)
    }

    // === Node errors ===

    /// Create an invalid node error
    pub const fn node_invalid() -> Self {
        Self::new(RclReturnCode::NodeInvalid)
    }

    /// Create an invalid node name error with context
    pub const fn node_invalid_name(name: &'static str) -> Self {
        Self::with_context(RclReturnCode::NodeInvalidName, ErrorContext::Node(name))
    }

    /// Create an invalid node namespace error with context
    pub const fn node_invalid_namespace(namespace: &'static str) -> Self {
        Self::with_context(
            RclReturnCode::NodeInvalidNamespace,
            ErrorContext::Custom(namespace),
        )
    }

    // === Topic errors ===

    /// Create an invalid topic name error with context
    pub const fn topic_name_invalid(topic: &'static str) -> Self {
        Self::with_context(RclReturnCode::TopicNameInvalid, ErrorContext::Topic(topic))
    }

    /// Create a publisher invalid error
    pub const fn publisher_invalid() -> Self {
        Self::new(RclReturnCode::PublisherInvalid)
    }

    /// Create a subscription invalid error
    pub const fn subscription_invalid() -> Self {
        Self::new(RclReturnCode::SubscriptionInvalid)
    }

    /// Create a subscription take failed error
    pub const fn subscription_take_failed() -> Self {
        Self::new(RclReturnCode::SubscriptionTakeFailed)
    }

    // === Service errors ===

    /// Create an invalid service name error with context
    pub const fn service_name_invalid(service: &'static str) -> Self {
        Self::with_context(
            RclReturnCode::ServiceNameInvalid,
            ErrorContext::Service(service),
        )
    }

    /// Create a service invalid error
    pub const fn service_invalid() -> Self {
        Self::new(RclReturnCode::ServiceInvalid)
    }

    /// Create a service take failed error
    pub const fn service_take_failed() -> Self {
        Self::new(RclReturnCode::ServiceTakeFailed)
    }

    /// Create a client invalid error
    pub const fn client_invalid() -> Self {
        Self::new(RclReturnCode::ClientInvalid)
    }

    /// Create a client take failed error
    pub const fn client_take_failed() -> Self {
        Self::new(RclReturnCode::ClientTakeFailed)
    }

    // === Timer errors ===

    /// Create a timer invalid error
    pub const fn timer_invalid() -> Self {
        Self::new(RclReturnCode::TimerInvalid)
    }

    /// Create a timer canceled error
    pub const fn timer_canceled() -> Self {
        Self::new(RclReturnCode::TimerCanceled)
    }

    // === Action errors ===

    /// Create an action goal rejected error
    pub const fn action_goal_rejected() -> Self {
        Self::new(RclReturnCode::ActionGoalRejected)
    }

    /// Create an action client invalid error
    pub const fn action_client_invalid() -> Self {
        Self::new(RclReturnCode::ActionClientInvalid)
    }

    /// Create an action server invalid error
    pub const fn action_server_invalid() -> Self {
        Self::new(RclReturnCode::ActionServerInvalid)
    }

    /// Create an action goal handle invalid error
    pub const fn action_goal_handle_invalid() -> Self {
        Self::new(RclReturnCode::ActionGoalHandleInvalid)
    }

    // === Init/Shutdown errors ===

    /// Create an already initialized error
    pub const fn already_init() -> Self {
        Self::new(RclReturnCode::AlreadyInit)
    }

    /// Create a not initialized error
    pub const fn not_init() -> Self {
        Self::new(RclReturnCode::NotInit)
    }

    /// Create an already shutdown error
    pub const fn already_shutdown() -> Self {
        Self::new(RclReturnCode::AlreadyShutdown)
    }

    // === Serialization errors ===

    /// Create a serialization error
    pub fn serialization(err: SerError) -> Self {
        Self {
            code: RclReturnCode::Error,
            context: None,
            nested: Some(NestedError::Ser(err)),
        }
    }

    /// Create a deserialization error
    pub fn deserialization(err: DeserError) -> Self {
        Self {
            code: RclReturnCode::Error,
            context: None,
            nested: Some(NestedError::Deser(err)),
        }
    }

    // === Query methods ===

    /// Returns the error code
    pub const fn code(&self) -> RclReturnCode {
        self.code
    }

    /// Returns the error context, if any
    pub const fn context(&self) -> Option<&ErrorContext> {
        self.context.as_ref()
    }

    /// Returns the nested error, if any
    pub const fn nested(&self) -> Option<&NestedError> {
        self.nested.as_ref()
    }

    /// Returns true if this error was due to a timeout
    pub const fn is_timeout(&self) -> bool {
        matches!(self.code, RclReturnCode::Timeout)
    }

    /// Returns true if this error was because a take operation failed
    /// (subscription, service, client, or action take failed)
    pub const fn is_take_failed(&self) -> bool {
        matches!(
            self.code,
            RclReturnCode::SubscriptionTakeFailed
                | RclReturnCode::ServiceTakeFailed
                | RclReturnCode::ClientTakeFailed
                | RclReturnCode::ActionServerTakeFailed
                | RclReturnCode::ActionClientTakeFailed
        )
    }

    /// Returns true if this is an action-related error
    pub const fn is_action_error(&self) -> bool {
        matches!(
            self.code,
            RclReturnCode::ActionGoalAccepted
                | RclReturnCode::ActionGoalRejected
                | RclReturnCode::ActionClientInvalid
                | RclReturnCode::ActionClientTakeFailed
                | RclReturnCode::ActionServerInvalid
                | RclReturnCode::ActionServerTakeFailed
                | RclReturnCode::ActionGoalHandleInvalid
        )
    }

    /// Returns true if this is a serialization or deserialization error
    pub const fn is_serialization_error(&self) -> bool {
        self.nested.is_some()
    }
}

impl fmt::Display for NanoRosError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Write the main error message
        write!(f, "{}", self.code)?;

        // Add context if available
        if let Some(ctx) = &self.context {
            write!(f, " ({})", ctx)?;
        }

        // Add nested error if available
        if let Some(nested) = &self.nested {
            match nested {
                NestedError::Ser(e) => write!(f, ": {}", e)?,
                NestedError::Deser(e) => write!(f, ": {}", e)?,
            }
        }

        Ok(())
    }
}

// Implement std::error::Error when std is available
#[cfg(feature = "std")]
impl std::error::Error for NanoRosError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        // Nested errors don't implement std::error::Error in no_std
        // so we can't return them as source. This is a limitation of no_std.
        None
    }
}

#[cfg(feature = "std")]
impl std::error::Error for RclReturnCode {}

// === Conversions ===

impl From<SerError> for NanoRosError {
    fn from(e: SerError) -> Self {
        Self::serialization(e)
    }
}

impl From<DeserError> for NanoRosError {
    fn from(e: DeserError) -> Self {
        Self::deserialization(e)
    }
}

impl From<RclReturnCode> for NanoRosError {
    fn from(code: RclReturnCode) -> Self {
        Self::new(code)
    }
}

// === Error filtering (matching rclrs patterns) ===

/// A helper trait to handle common error filtering patterns
///
/// This trait provides methods similar to rclrs for filtering errors
/// that are expected in normal operation (timeouts, take failures).
pub trait NanoRosErrorFilter {
    /// The output type after filtering
    type Output;

    /// If the result was a timeout error, change it to `Ok(())`
    fn timeout_ok(self) -> Self::Output;

    /// If a take operation failed, change the result to `Ok(())`
    fn take_failed_ok(self) -> Self::Output;

    /// Filter out both timeouts and take failures
    fn ignore_non_errors(self) -> Self::Output
    where
        Self: Sized,
        Self::Output: From<Self>,
    {
        // Default implementation chains the two filters
        self.timeout_ok()
    }
}

impl NanoRosErrorFilter for Result<(), NanoRosError> {
    type Output = Result<(), NanoRosError>;

    fn timeout_ok(self) -> Self::Output {
        match self {
            Ok(()) => Ok(()),
            Err(e) if e.is_timeout() => Ok(()),
            Err(e) => Err(e),
        }
    }

    fn take_failed_ok(self) -> Self::Output {
        match self {
            Ok(()) => Ok(()),
            Err(e) if e.is_take_failed() => Ok(()),
            Err(e) => Err(e),
        }
    }

    fn ignore_non_errors(self) -> Self::Output {
        self.timeout_ok().take_failed_ok()
    }
}

/// A helper trait to convert take failures to None
///
/// This is useful when you want to distinguish between "no data available"
/// (returns None) and actual errors (returns Err).
pub trait TakeFailedAsNone {
    /// The value type
    type T;

    /// If the take failed, return `Ok(None)`. Otherwise return `Ok(Some(value))`.
    fn take_failed_as_none(self) -> Result<Option<Self::T>, NanoRosError>;
}

impl<T> TakeFailedAsNone for Result<T, NanoRosError> {
    type T = T;

    fn take_failed_as_none(self) -> Result<Option<T>, NanoRosError> {
        match self {
            Ok(value) => Ok(Some(value)),
            Err(e) if e.is_take_failed() => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate alloc;
    use alloc::format;

    use super::*;

    #[test]
    fn test_rcl_return_code_display() {
        assert!(format!("{}", RclReturnCode::Ok).contains("RCL_RET_OK"));
        assert!(format!("{}", RclReturnCode::Timeout).contains("RCL_RET_TIMEOUT"));
        assert!(
            format!("{}", RclReturnCode::NodeInvalidName).contains("RCL_RET_NODE_INVALID_NAME")
        );
    }

    #[test]
    fn test_rcl_return_code_try_from() {
        assert_eq!(RclReturnCode::try_from_i32(0), Some(RclReturnCode::Ok));
        assert_eq!(RclReturnCode::try_from_i32(2), Some(RclReturnCode::Timeout));
        assert_eq!(
            RclReturnCode::try_from_i32(201),
            Some(RclReturnCode::NodeInvalidName)
        );
        assert_eq!(RclReturnCode::try_from_i32(9999), None);
    }

    #[test]
    fn test_nano_ros_error_timeout() {
        let err = NanoRosError::timeout();
        assert!(err.is_timeout());
        assert!(!err.is_take_failed());
        assert_eq!(err.code(), RclReturnCode::Timeout);
    }

    #[test]
    fn test_nano_ros_error_take_failed() {
        let err = NanoRosError::subscription_take_failed();
        assert!(err.is_take_failed());
        assert!(!err.is_timeout());

        let err = NanoRosError::client_take_failed();
        assert!(err.is_take_failed());

        let err = NanoRosError::service_take_failed();
        assert!(err.is_take_failed());
    }

    #[test]
    fn test_nano_ros_error_with_context() {
        let err = NanoRosError::topic_name_invalid("/bad topic");
        assert!(err.context().is_some());
        if let Some(ErrorContext::Topic(name)) = err.context() {
            assert_eq!(*name, "/bad topic");
        } else {
            panic!("Expected Topic context");
        }
    }

    #[test]
    fn test_nano_ros_error_display() {
        let err = NanoRosError::timeout();
        let msg = format!("{}", err);
        assert!(msg.contains("Timeout"));

        let err = NanoRosError::topic_name_invalid("/test");
        let msg = format!("{}", err);
        assert!(msg.contains("topic"));
        assert!(msg.contains("/test"));
    }

    #[test]
    fn test_nano_ros_error_from_ser_error() {
        let err: NanoRosError = SerError::BufferTooSmall.into();
        assert!(err.is_serialization_error());
        assert!(matches!(err.nested(), Some(NestedError::Ser(_))));
    }

    #[test]
    fn test_nano_ros_error_from_deser_error() {
        let err: NanoRosError = DeserError::UnexpectedEof.into();
        assert!(err.is_serialization_error());
        assert!(matches!(err.nested(), Some(NestedError::Deser(_))));
    }

    #[test]
    fn test_error_filter_timeout_ok() {
        let result: Result<(), NanoRosError> = Err(NanoRosError::timeout());
        assert!(result.timeout_ok().is_ok());

        let result: Result<(), NanoRosError> = Err(NanoRosError::error());
        assert!(result.timeout_ok().is_err());

        let result: Result<(), NanoRosError> = Ok(());
        assert!(result.timeout_ok().is_ok());
    }

    #[test]
    fn test_error_filter_take_failed_ok() {
        let result: Result<(), NanoRosError> = Err(NanoRosError::subscription_take_failed());
        assert!(result.take_failed_ok().is_ok());

        let result: Result<(), NanoRosError> = Err(NanoRosError::error());
        assert!(result.take_failed_ok().is_err());
    }

    #[test]
    fn test_take_failed_as_none() {
        let result: Result<i32, NanoRosError> = Err(NanoRosError::subscription_take_failed());
        assert_eq!(result.take_failed_as_none().unwrap(), None);

        let result: Result<i32, NanoRosError> = Ok(42);
        assert_eq!(result.take_failed_as_none().unwrap(), Some(42));

        let result: Result<i32, NanoRosError> = Err(NanoRosError::timeout());
        assert!(result.take_failed_as_none().is_err());
    }

    #[test]
    fn test_action_error_detection() {
        let err = NanoRosError::action_goal_rejected();
        assert!(err.is_action_error());

        let err = NanoRosError::action_client_invalid();
        assert!(err.is_action_error());

        let err = NanoRosError::timeout();
        assert!(!err.is_action_error());
    }

    #[test]
    fn test_error_context_display() {
        let ctx = ErrorContext::Topic("/my_topic");
        assert!(format!("{}", ctx).contains("topic"));
        assert!(format!("{}", ctx).contains("/my_topic"));

        let ctx = ErrorContext::Service("/my_service");
        assert!(format!("{}", ctx).contains("service"));

        let ctx = ErrorContext::Timer(42);
        assert!(format!("{}", ctx).contains("42"));
    }
}

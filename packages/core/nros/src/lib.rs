//! # nros
//!
//! A lightweight ROS 2 client library for embedded systems.
//!
//! This crate provides a unified API for building ROS 2 nodes in Rust,
//! with support for `no_std` environments and embedded targets.
//!
//! ## Features
//!
//! - **no_std compatible**: Works on bare-metal and RTOS targets
//! - **Zero-copy where possible**: Minimizes memory allocations
//! - **Type-safe**: Compile-time verification of message types
//! - **ROS 2 compatible**: Interoperates with standard ROS 2 nodes via rmw_zenoh
//!
//! ## Quick Start
//!
//! ```ignore
//! use nros::prelude::*;
//! use std_msgs::msg::Int32;
//!
//! let config = ExecutorConfig::from_env().node_name("my_node");
//! let mut executor = Executor::open(&config)?;
//!
//! let mut node = executor.create_node("my_node")?;
//! let publisher = node.create_publisher::<Int32>("/my_topic")?;
//! publisher.publish(&Int32 { data: 42 })?;
//!
//! executor.add_subscription::<Int32, _>("/topic", |msg: &Int32| {
//!     println!("Received: {}", msg.data);
//! })?;
//!
//! executor.spin_blocking(SpinOptions::default());
//! ```
//!
//! ## Executor Sizing
//!
//! The executor's static memory layout is controlled via environment variables
//! at build time:
//!
//! - **`NROS_EXECUTOR_MAX_CBS`** (default 4) — maximum number of registered
//!   callbacks (subscriptions + timers + services + guard conditions).
//! - **`NROS_EXECUTOR_ARENA_SIZE`** (default 4096) — byte budget for storing
//!   callback closures inline.
//!
//! For messages larger than the default 1024-byte receive buffer, use the
//! `_sized` method variants (e.g., `add_subscription_sized`) to specify a
//! custom buffer size.
//!
//! ## Transport Backends
//!
//! The transport backend is selected at compile time via feature flags:
//!
//! - `rmw-zenoh` → zenoh-pico transport
//! - `rmw-xrce` → XRCE-DDS transport
//!
//! The concrete session type is resolved automatically. Advanced users
//! who need it can access it via `nros::internals::RmwSession`.
//!
//! ## Crate Features
//!
//! Three orthogonal feature axes:
//!
//! **RMW backend** (select one):
//! - `rmw-zenoh` - zenoh-pico transport backend
//! - `rmw-xrce` - XRCE-DDS transport backend
//!
//! **Platform** (select one):
//! - `platform-posix` - Desktop/Linux
//! - `platform-zephyr` - Zephyr RTOS
//! - `platform-bare-metal` - Bare-metal targets
//!
//! **ROS version** (select one):
//! - `ros-humble` - ROS 2 Humble
//! - `ros-iron` - ROS 2 Iron
//!
//! **Other**:
//! - `std` (default) - Enable standard library support
//! - `alloc` - Enable heap allocation without full std
//!
//! ## Further Reading
//!
//! - [`guide`] — tutorials: getting started, services, configuration,
//!   ROS 2 interop, and troubleshooting
//! - [Message Generation](https://github.com/jerry73204/nano-ros/blob/main/docs/guides/message-generation.md)
//!   — codegen reference (all options, output structure, bundled interfaces)
//! - [Environment Variables](https://github.com/jerry73204/nano-ros/blob/main/docs/reference/environment-variables.md)
//!   — complete buffer tuning reference
//! - [ROS 2 Interop](https://github.com/jerry73204/nano-ros/blob/main/docs/reference/rmw_zenoh_interop.md)
//!   — protocol details (key expressions, liveliness, attachments)
//! - [Examples](https://github.com/jerry73204/nano-ros/tree/main/examples)
//!   — working examples by platform (native, QEMU, ESP32, Zephyr)

#![no_std]

// ── Feature validation (mutual exclusivity) ─────────────────────────────
// At most one RMW backend.
#[cfg(all(feature = "rmw-zenoh", feature = "rmw-xrce"))]
compile_error!("`rmw-zenoh` and `rmw-xrce` are mutually exclusive — select one RMW backend.");
#[cfg(all(feature = "rmw-cffi", feature = "rmw-zenoh"))]
compile_error!("`rmw-cffi` and `rmw-zenoh` are mutually exclusive.");
#[cfg(all(feature = "rmw-cffi", feature = "rmw-xrce"))]
compile_error!("`rmw-cffi` and `rmw-xrce` are mutually exclusive.");

// At most one platform.
#[cfg(any(
    all(feature = "platform-posix", feature = "platform-zephyr"),
    all(feature = "platform-posix", feature = "platform-bare-metal"),
    all(feature = "platform-posix", feature = "platform-freertos"),
    all(feature = "platform-posix", feature = "platform-nuttx"),
    all(feature = "platform-posix", feature = "platform-threadx"),
    all(feature = "platform-zephyr", feature = "platform-bare-metal"),
    all(feature = "platform-zephyr", feature = "platform-freertos"),
    all(feature = "platform-zephyr", feature = "platform-nuttx"),
    all(feature = "platform-zephyr", feature = "platform-threadx"),
    all(feature = "platform-bare-metal", feature = "platform-freertos"),
    all(feature = "platform-bare-metal", feature = "platform-nuttx"),
    all(feature = "platform-bare-metal", feature = "platform-threadx"),
    all(feature = "platform-freertos", feature = "platform-nuttx"),
    all(feature = "platform-freertos", feature = "platform-threadx"),
    all(feature = "platform-nuttx", feature = "platform-threadx"),
))]
compile_error!(
    "Platform features are mutually exclusive — select at most one of: \
     `platform-posix`, `platform-zephyr`, `platform-bare-metal`, \
     `platform-freertos`, `platform-nuttx`, `platform-threadx`."
);

// At most one ROS edition.
#[cfg(all(feature = "ros-humble", feature = "ros-iron"))]
compile_error!("`ros-humble` and `ros-iron` are mutually exclusive — select one ROS edition.");

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod guide;

// Re-export core types
pub use nros_core::{
    CdrReader, CdrWriter, Clock, ClockType, DeserError, Deserialize, Duration, Logger, MessageInfo,
    PUBLISHER_GID_SIZE, RosMessage, RosService, SerError, Serialize, Time,
};

// Re-export heapless for generated message types and examples
pub use nros_core::heapless;

// Re-export node types
pub use nros_node::{NodeConfig, PublisherHandle, StandaloneNode, SubscriberHandle};

// Re-export standalone node options (when no RMW backend is active)
#[cfg(not(feature = "rmw-zenoh"))]
pub use nros_node::{PublisherOptions, SubscriberOptions};

// Re-export timer types
pub use nros_node::{
    DEFAULT_MAX_TIMERS, TimerCallbackFn, TimerDuration, TimerHandle, TimerMode, TimerState,
};

// Re-export transport types (middleware-agnostic)
pub use nros_rmw::{
    Publisher, QosDurabilityPolicy, QosHistoryPolicy, QosReliabilityPolicy, QosSettings, Rmw,
    RmwConfig, ServiceClientTrait, ServiceInfo, ServiceRequest, ServiceServerTrait, Session,
    SessionMode, Subscriber, TopicInfo, Transport, TransportConfig, TransportError,
};

// Re-export safety types when feature is enabled
#[cfg(feature = "safety-e2e")]
pub use nros_rmw::{IntegrityStatus, SafetyValidator, crc32};

/// Backend-specific internal types.
///
/// These types are implementation details of the transport backends.
/// Most users should use the high-level APIs (`Executor`, etc.)
/// instead of these types directly.
///
/// The `Rmw*` type aliases resolve to whichever backend is active at compile time,
/// providing a backend-agnostic way to reference concrete transport types.
pub mod internals {
    // Zenoh backend internal types
    #[cfg(feature = "rmw-zenoh")]
    pub use nros_rmw_zenoh::{
        LivelinessToken, RMW_GID_SIZE, RmwAttachment, Ros2Liveliness, ZenohId, ZenohPublisher,
        ZenohServiceClient, ZenohServiceServer, ZenohSession, ZenohSubscriber, ZenohTransport,
    };

    // ── Backend-agnostic type aliases ────────────────────────────────────
    // These resolve to the concrete types of the active RMW backend.

    #[cfg(feature = "rmw-zenoh")]
    pub type RmwSession = nros_rmw_zenoh::ZenohSession;
    #[cfg(feature = "rmw-zenoh")]
    pub type RmwPublisher = nros_rmw_zenoh::ZenohPublisher;
    #[cfg(feature = "rmw-zenoh")]
    pub type RmwSubscriber = nros_rmw_zenoh::ZenohSubscriber;
    #[cfg(feature = "rmw-zenoh")]
    pub type RmwServiceServer = nros_rmw_zenoh::ZenohServiceServer;
    #[cfg(feature = "rmw-zenoh")]
    pub type RmwServiceClient = nros_rmw_zenoh::ZenohServiceClient;

    #[cfg(feature = "rmw-xrce")]
    pub use nros_rmw_xrce::{
        XrcePublisher, XrceRmw, XrceServiceClient, XrceServiceServer, XrceSession, XrceSubscriber,
    };

    /// XRCE-DDS transport initialization helpers.
    ///
    /// Most users should use `Executor::open()` which auto-initializes
    /// the transport. These are provided for advanced use cases.
    #[cfg(feature = "rmw-xrce")]
    pub mod xrce_transport {
        /// Initialize POSIX UDP transport for XRCE-DDS.
        ///
        /// # Safety
        ///
        /// Must not be called concurrently. Only one transport may be active.
        #[cfg(feature = "xrce-udp")]
        pub unsafe fn init_posix_udp(agent_addr: &str) {
            unsafe {
                nros_rmw_xrce::posix_udp::init_posix_udp_transport(agent_addr);
            }
        }

        /// Initialize POSIX serial transport for XRCE-DDS.
        ///
        /// # Safety
        ///
        /// Must not be called concurrently. Only one transport may be active.
        #[cfg(feature = "xrce-serial")]
        pub unsafe fn init_posix_serial(pty_path: &str) {
            unsafe {
                nros_rmw_xrce::posix_serial::init_posix_serial_transport(pty_path);
            }
        }
    }

    #[cfg(feature = "rmw-xrce")]
    pub type RmwSession = nros_rmw_xrce::XrceSession;
    #[cfg(feature = "rmw-xrce")]
    pub type RmwPublisher = nros_rmw_xrce::XrcePublisher;
    #[cfg(feature = "rmw-xrce")]
    pub type RmwSubscriber = nros_rmw_xrce::XrceSubscriber;
    #[cfg(feature = "rmw-xrce")]
    pub type RmwServiceServer = nros_rmw_xrce::XrceServiceServer;
    #[cfg(feature = "rmw-xrce")]
    pub type RmwServiceClient = nros_rmw_xrce::XrceServiceClient;

    #[cfg(feature = "rmw-cffi")]
    pub type RmwSession = nros_rmw_cffi::CffiSession;
    #[cfg(feature = "rmw-cffi")]
    pub type RmwPublisher = nros_rmw_cffi::CffiPublisher;
    #[cfg(feature = "rmw-cffi")]
    pub type RmwSubscriber = nros_rmw_cffi::CffiSubscriber;
    #[cfg(feature = "rmw-cffi")]
    pub type RmwServiceServer = nros_rmw_cffi::CffiServiceServer;
    #[cfg(feature = "rmw-cffi")]
    pub type RmwServiceClient = nros_rmw_cffi::CffiServiceClient;

    #[cfg(feature = "rmw-dds")]
    pub type RmwSession = nros_rmw_dds::DdsSession;
    #[cfg(feature = "rmw-dds")]
    pub type RmwPublisher = nros_rmw_dds::DdsPublisher;
    #[cfg(feature = "rmw-dds")]
    pub type RmwSubscriber = nros_rmw_dds::DdsSubscriber;
    #[cfg(feature = "rmw-dds")]
    pub type RmwServiceServer = nros_rmw_dds::DdsServiceServer;
    #[cfg(feature = "rmw-dds")]
    pub type RmwServiceClient = nros_rmw_dds::DdsServiceClient;

    /// Open a new middleware session.
    ///
    /// Wraps the backend-specific session constructor behind a common signature.
    /// Used by the C API (`nros-c`); Rust users should prefer `Executor::open()`.
    ///
    /// - **Zenoh**: `domain_id` and `node_name` are ignored (zenoh uses `locator` and `mode`).
    /// - **XRCE-DDS**: `locator` is the agent address (e.g., `"127.0.0.1:2019"`).
    ///   Transport must match the active transport feature (`xrce-udp` or `xrce-serial`).
    #[cfg(any(
        feature = "rmw-zenoh",
        feature = "rmw-xrce",
        feature = "rmw-dds",
        feature = "rmw-cffi"
    ))]
    pub fn open_session(
        locator: &str,
        mode: nros_rmw::SessionMode,
        domain_id: u32,
        node_name: &str,
    ) -> Result<RmwSession, nros_rmw::TransportError> {
        #[cfg(feature = "rmw-zenoh")]
        {
            use nros_rmw::TransportConfig;

            let _ = (domain_id, node_name);
            let config = TransportConfig {
                locator: Some(locator),
                mode,
                properties: &[],
            };
            RmwSession::new(&config).map_err(|_| nros_rmw::TransportError::ConnectionFailed)
        }

        #[cfg(all(feature = "rmw-xrce", not(feature = "rmw-zenoh")))]
        {
            use nros_rmw::Rmw;

            // Initialize transport based on active transport feature
            #[cfg(feature = "xrce-udp")]
            unsafe {
                nros_rmw_xrce::posix_udp::init_posix_udp_transport(locator);
            }

            #[cfg(feature = "xrce-serial")]
            unsafe {
                nros_rmw_xrce::posix_serial::init_posix_serial_transport(locator);
            }

            #[cfg(feature = "platform-zephyr")]
            unsafe {
                nros_rmw_xrce::zephyr::init_zephyr_transport(locator);
            }

            let config = nros_rmw::RmwConfig {
                locator,
                mode,
                domain_id,
                node_name,
                namespace: "",
            };
            nros_rmw_xrce::XrceRmw::open(&config)
                .map_err(|_| nros_rmw::TransportError::ConnectionFailed)
        }

        #[cfg(all(
            feature = "rmw-dds",
            not(feature = "rmw-zenoh"),
            not(feature = "rmw-xrce"),
        ))]
        {
            use nros_rmw::Rmw;

            let config = nros_rmw::RmwConfig {
                locator,
                mode,
                domain_id,
                node_name,
                namespace: "",
            };
            nros_rmw_dds::DdsRmw::open(&config)
                .map_err(|_| nros_rmw::TransportError::ConnectionFailed)
        }

        #[cfg(all(
            feature = "rmw-cffi",
            not(feature = "rmw-zenoh"),
            not(feature = "rmw-xrce"),
            not(feature = "rmw-dds"),
        ))]
        {
            use nros_rmw::Rmw;

            let config = nros_rmw::RmwConfig {
                locator,
                mode,
                domain_id,
                node_name,
                namespace: "",
            };
            nros_rmw_cffi::CffiRmw::open(&config)
                .map_err(|_| nros_rmw::TransportError::ConnectionFailed)
        }
    }

    /// Drive middleware I/O for pull-based backends.
    ///
    /// Delegates to [`Session::drive_io()`](nros_rmw::Session::drive_io),
    /// which each backend implements appropriately (no-op for push-based,
    /// poll for pull-based).
    ///
    /// Used by the C API executor before polling handles.
    #[cfg(any(
        feature = "rmw-zenoh",
        feature = "rmw-xrce",
        feature = "rmw-dds",
        feature = "rmw-cffi"
    ))]
    pub fn drive_session_io(session: &mut RmwSession, timeout_ms: i32) {
        use nros_rmw::Session;
        let _ = session.drive_io(timeout_ms);
    }
}

// Re-export types that don't depend on RMW (always available)
pub use nros_node::{
    ExecutorConfig, ExecutorSemantics, GuardConditionHandle, HandleId, HandleSet, InvocationMode,
    NodeError, RawCancelCallback, RawGoalCallback, RawServiceCallback, RawSubscriptionCallback,
    ReadinessSnapshot, SpinOnceResult, SpinOptions, SpinPeriodPollingResult, Trigger,
};

// Re-export RMW-dependent types (require an active transport backend)
#[cfg(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi"
))]
pub use nros_node::{
    ActionClient, ActionClientCore, ActionServer, ActionServerCore, ActionServerHandle,
    ActionServerRawHandle, ActiveGoal, CompletedGoal, EmbeddedPublisher, EmbeddedServiceClient,
    EmbeddedServiceServer, Executor, FeedbackStream, GoalFeedbackStream, Node, Promise,
    RawActiveGoal, Subscription,
};

#[cfg(all(
    feature = "std",
    any(
        feature = "rmw-zenoh",
        feature = "rmw-xrce",
        feature = "rmw-dds",
        feature = "rmw-cffi"
    )
))]
pub use nros_node::SpinPeriodResult;

// Re-export service types
pub use nros_core::{ServiceClient, ServiceServer};

// Re-export action types
pub use nros_core::{
    CancelResponse, GoalId, GoalInfo, GoalResponse, GoalStatus, GoalStatusStamped, RosAction,
};

// Re-export lifecycle types (always available, no_std compatible)
pub use nros_core::{LifecycleState, LifecycleTransition, TransitionResult};
pub use nros_node::{LifecycleCallbackFn, LifecycleError, LifecyclePollingNode};

// Re-export parameter types
pub use nros_params::{
    MandatoryParameter, OptionalParameter, Parameter, ParameterBuilder, ParameterDescriptor,
    ParameterError, ParameterServer, ParameterType, ParameterValue, ParameterVariant,
    ReadOnlyParameter, SetParameterResult,
};

/// Prelude module for convenient imports
///
/// Import everything you need with a single statement:
/// ```
/// use nros::prelude::*;
/// ```
pub mod prelude {
    pub use crate::{
        CdrReader, CdrWriter, Deserialize, Logger, MessageInfo, NodeConfig, PublisherHandle,
        QosDurabilityPolicy, QosHistoryPolicy, QosReliabilityPolicy, QosSettings, RosMessage,
        RosService, Serialize, StandaloneNode, SubscriberHandle, TopicInfo,
    };

    // Re-export lifecycle types
    pub use crate::{
        LifecycleCallbackFn, LifecycleError, LifecyclePollingNode, LifecycleState,
        LifecycleTransition, TransitionResult,
    };

    // Re-export executor config types (always available)
    pub use crate::{
        ExecutorConfig, NodeError, SessionMode, SpinOnceResult, SpinOptions,
        SpinPeriodPollingResult,
    };

    // Re-export RMW-dependent executor types
    #[cfg(any(
        feature = "rmw-zenoh",
        feature = "rmw-xrce",
        feature = "rmw-dds",
        feature = "rmw-cffi"
    ))]
    pub use crate::{EmbeddedPublisher, Executor, Node, Subscription};

    // Standalone node options (no-transport simulation mode)
    #[cfg(not(feature = "rmw-zenoh"))]
    pub use crate::{PublisherOptions, SubscriberOptions};

    #[cfg(all(
        feature = "std",
        any(
            feature = "rmw-zenoh",
            feature = "rmw-xrce",
            feature = "rmw-dds",
            feature = "rmw-cffi"
        )
    ))]
    pub use crate::SpinPeriodResult;

    // Re-export parameter types
    pub use crate::{ParameterServer, ParameterType, ParameterValue};

    // Re-export typed parameter API (rclrs-compatible builder pattern)
    pub use crate::{
        MandatoryParameter, OptionalParameter, ParameterBuilder, ParameterError, ParameterVariant,
        ReadOnlyParameter,
    };

    // Re-export action types
    pub use crate::{GoalId, GoalInfo, GoalResponse, GoalStatus, GoalStatusStamped, RosAction};

    // Re-export Time, Duration, Clock from core
    pub use nros_core::{Clock, ClockType, Duration, Time};

    // Re-export timer types
    pub use crate::{TimerCallbackFn, TimerDuration, TimerHandle, TimerMode};
}

/// Derive macros for message types
///
/// Use these macros to generate message serialization code.
/// These macros help you create custom message types that are compatible
/// with ROS 2's CDR serialization format.
pub mod derive {
    pub use nros_macros::RosMessage;
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_prelude_imports() {
        // This test just verifies that the prelude compiles
        use crate::prelude::*;

        let _ = NodeConfig::new("test_node", "/");
        let _ = QosSettings::BEST_EFFORT;
    }
}

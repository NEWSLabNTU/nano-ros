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
//! Message types are generated from ROS 2 interface packages using `cargo nano-ros generate`.
//! See the examples for how to set up bindings.
//!
//! ```ignore
//! use nros::prelude::*;
//! use std_msgs::msg::Int32;  // Generated bindings
//!
//! // Create a node
//! let config = NodeConfig::new("my_node");
//! let mut node = ConnectedNode::connect(config, "tcp/127.0.0.1:7447")
//!     .expect("Failed to connect");
//!
//! // Create a publisher
//! let publisher = node.create_publisher::<Int32>("/my_topic")
//!     .expect("Failed to create publisher");
//!
//! // Publish a message
//! let msg = Int32 { data: 42 };
//! publisher.publish(&msg).expect("Failed to publish");
//! ```
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
    all(feature = "platform-zephyr", feature = "platform-bare-metal"),
))]
compile_error!(
    "`platform-posix`, `platform-zephyr`, and `platform-bare-metal` are mutually exclusive."
);

// At most one ROS edition.
#[cfg(all(feature = "ros-humble", feature = "ros-iron"))]
compile_error!("`ros-humble` and `ros-iron` are mutually exclusive — select one ROS edition.");

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

// Re-export core types
pub use nros_core::{
    CdrReader, CdrWriter, Clock, ClockType, DeserError, Deserialize, Duration, Logger, MessageInfo,
    PUBLISHER_GID_SIZE, RosMessage, RosService, SerError, Serialize, Time,
};

// Re-export heapless for generated message types and examples
pub use nros_core::heapless;

// Re-export node types
pub use nros_node::{
    NodeConfig, PublisherHandle, PublisherOptions, StandaloneNode, SubscriberHandle,
    SubscriberOptions,
};

// Re-export timer types
pub use nros_node::{
    DEFAULT_MAX_TIMERS, TimerCallbackFn, TimerDuration, TimerHandle, TimerMode, TimerState,
};

// Re-export connected node types (requires rmw-zenoh + alloc)
#[cfg(all(feature = "rmw-zenoh", feature = "alloc"))]
pub use nros_node::{
    ConnectedActionClient, ConnectedActionServer, ConnectedNode, ConnectedNodeError,
    ConnectedPublisher, ConnectedServiceClient, ConnectedServiceServer, ConnectedSubscriber,
    DEFAULT_TX_BUFFER_SIZE,
};

// Re-export error types (available without alloc)
#[cfg(feature = "rmw-zenoh")]
pub use nros_node::RclrsError;

// Re-export new rclrs-style API types (requires rmw-zenoh + alloc)
#[cfg(all(feature = "rmw-zenoh", feature = "alloc"))]
pub use nros_node::{
    Context, InitOptions, IntoNodeOptions, IntoPublisherOptions, IntoSubscriberOptions, Node,
    NodeNameExt, NodeOptions,
};

// Re-export executor types (with zenoh and alloc features)
#[cfg(all(feature = "rmw-zenoh", feature = "alloc"))]
pub use nros_node::{
    Executor, NodeHandle, NodeState, PollingExecutor, SpinOnceResult, SpinOptions,
    SpinPeriodPollingResult, SubscriptionCallback, SubscriptionCallbackWithInfo,
};

// Re-export safety-e2e executor callback
#[cfg(all(feature = "rmw-zenoh", feature = "alloc", feature = "safety-e2e"))]
pub use nros_node::SubscriptionCallbackWithSafety;

// Re-export BasicExecutor, SpinPeriodResult, and Promise (with zenoh and std features)
#[cfg(all(feature = "rmw-zenoh", feature = "std"))]
pub use nros_node::{BasicExecutor, Promise, SpinPeriodResult};

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
/// Most users should use the high-level APIs (`Context`, `Executor`, `EmbeddedExecutor`, etc.)
/// instead of these types directly.
///
/// The `Rmw*` type aliases resolve to whichever backend is active at compile time,
/// providing a backend-agnostic way to reference concrete transport types.
pub mod internals {
    // Zenoh backend internal types
    #[cfg(feature = "rmw-zenoh")]
    pub use nros_rmw_zenoh::{
        RMW_GID_SIZE, RmwAttachment, Ros2Liveliness, ShimLivelinessToken, ShimPublisher,
        ShimServiceClient, ShimServiceServer, ShimSession, ShimSubscriber, ShimTransport,
        ShimZenohId, ZenohId, ZenohServiceClient, ZenohServiceServer, ZenohSession, ZenohTransport,
    };

    // ── Backend-agnostic type aliases ────────────────────────────────────
    // These resolve to the concrete types of the active RMW backend.

    #[cfg(feature = "rmw-zenoh")]
    pub type RmwSession = nros_rmw_zenoh::ShimSession;
    #[cfg(feature = "rmw-zenoh")]
    pub type RmwPublisher = nros_rmw_zenoh::ShimPublisher;
    #[cfg(feature = "rmw-zenoh")]
    pub type RmwSubscriber = nros_rmw_zenoh::ShimSubscriber;
    #[cfg(feature = "rmw-zenoh")]
    pub type RmwServiceServer = nros_rmw_zenoh::ShimServiceServer;
    #[cfg(feature = "rmw-zenoh")]
    pub type RmwServiceClient = nros_rmw_zenoh::ShimServiceClient;

    #[cfg(feature = "rmw-xrce")]
    pub use nros_rmw_xrce::{
        XrcePublisher, XrceRmw, XrceServiceClient, XrceServiceServer, XrceSession, XrceSubscriber,
    };

    /// XRCE-DDS transport initialization helpers.
    ///
    /// Most users should use `EmbeddedExecutor::open()` which auto-initializes
    /// the transport. These are provided for advanced use cases.
    #[cfg(feature = "rmw-xrce")]
    pub mod xrce_transport {
        /// Initialize POSIX UDP transport for XRCE-DDS.
        #[cfg(feature = "xrce-udp")]
        pub fn init_posix_udp(agent_addr: &str) {
            unsafe {
                nros_rmw_xrce::posix_udp::init_posix_udp_transport(agent_addr);
            }
        }

        /// Initialize POSIX serial transport for XRCE-DDS.
        #[cfg(feature = "xrce-serial")]
        pub fn init_posix_serial(pty_path: &str) {
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

    /// Open a new middleware session.
    ///
    /// Wraps the backend-specific session constructor behind a common signature.
    /// Used by the C API (`nros-c`); Rust users should prefer `Context::new()`.
    ///
    /// - **Zenoh**: `domain_id` and `node_name` are ignored (zenoh uses `locator` and `mode`).
    /// - **XRCE-DDS**: `locator` is the agent address (e.g., `"127.0.0.1:2019"`).
    ///   Transport must match the active transport feature (`xrce-udp` or `xrce-serial`).
    #[cfg(any(feature = "rmw-zenoh", feature = "rmw-xrce", feature = "rmw-cffi"))]
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
            feature = "rmw-cffi",
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
    #[cfg(any(feature = "rmw-zenoh", feature = "rmw-xrce", feature = "rmw-cffi"))]
    pub fn drive_session_io(session: &mut RmwSession, timeout_ms: i32) {
        use nros_rmw::Session;
        let _ = session.drive_io(timeout_ms);
    }
}

// Re-export embedded node types (always available, no feature gate)
pub use nros_node::{
    EmbeddedActionClient, EmbeddedActionServer, EmbeddedActiveGoal, EmbeddedCompletedGoal,
    EmbeddedConfig, EmbeddedExecutor, EmbeddedNode, EmbeddedNodeError, EmbeddedPublisher,
    EmbeddedServiceClient, EmbeddedServiceServer, EmbeddedSubscription,
};

// Re-export service types
pub use nros_core::{ServiceClient, ServiceServer};

// Re-export action types
pub use nros_core::{
    ActionClient, ActionServer, CancelResponse, GoalId, GoalInfo, GoalResponse, GoalStatus,
    GoalStatusStamped, RosAction,
};

// Re-export trigger types
pub use nros_node::{Trigger, TriggerCondition, TriggerFn};

// Re-export lifecycle types (always available, no_std compatible)
pub use nros_core::{LifecycleState, LifecycleTransition, TransitionResult};
pub use nros_node::{LifecycleCallbackFn, LifecycleError, LifecyclePollingNode};

#[cfg(all(feature = "rmw-zenoh", feature = "alloc"))]
pub use nros_node::LifecycleNode;

// Re-export parameter types
pub use nros_params::{
    Parameter, ParameterDescriptor, ParameterServer, ParameterType, ParameterValue,
    SetParameterResult,
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
        PublisherOptions, QosDurabilityPolicy, QosHistoryPolicy, QosReliabilityPolicy, QosSettings,
        RosMessage, RosService, Serialize, StandaloneNode, SubscriberHandle, SubscriberOptions,
        TopicInfo,
    };

    #[cfg(all(feature = "rmw-zenoh", feature = "alloc"))]
    pub use crate::{
        ConnectedActionClient, ConnectedActionServer, ConnectedNode, ConnectedNodeError,
        ConnectedPublisher, ConnectedServiceClient, ConnectedServiceServer, ConnectedSubscriber,
        SessionMode,
    };

    // Re-export error types
    #[cfg(feature = "rmw-zenoh")]
    pub use crate::RclrsError;

    // Re-export new rclrs-style API
    #[cfg(all(feature = "rmw-zenoh", feature = "alloc"))]
    pub use crate::{
        Context, InitOptions, IntoNodeOptions, IntoPublisherOptions, IntoSubscriberOptions, Node,
        NodeNameExt, NodeOptions,
    };

    // Re-export executor types
    #[cfg(all(feature = "rmw-zenoh", feature = "alloc"))]
    pub use crate::{
        Executor, PollingExecutor, SpinOnceResult, SpinOptions, SpinPeriodPollingResult,
        SubscriptionCallback, SubscriptionCallbackWithInfo,
    };

    // Re-export trigger types
    pub use crate::{Trigger, TriggerCondition, TriggerFn};

    // Re-export lifecycle types
    pub use crate::{
        LifecycleCallbackFn, LifecycleError, LifecyclePollingNode, LifecycleState,
        LifecycleTransition, TransitionResult,
    };

    #[cfg(all(feature = "rmw-zenoh", feature = "alloc"))]
    pub use crate::LifecycleNode;

    // Re-export BasicExecutor, SpinPeriodResult, and Promise
    #[cfg(all(feature = "rmw-zenoh", feature = "std"))]
    pub use crate::{BasicExecutor, Promise, SpinPeriodResult};

    // Re-export generic embedded node types
    pub use crate::{
        EmbeddedConfig, EmbeddedExecutor, EmbeddedNode, EmbeddedNodeError, EmbeddedPublisher,
        EmbeddedSubscription,
    };

    // Re-export parameter types
    pub use crate::{ParameterServer, ParameterType, ParameterValue};

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

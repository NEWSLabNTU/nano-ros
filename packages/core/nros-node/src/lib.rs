//! Node abstraction for nros
//!
//! This crate provides the high-level Node API for creating ROS 2 compatible
//! publishers and subscribers on embedded systems.
//!
//! # Executor-Based API
//!
//! The executor-based API provides a unified interface that works on both
//! std (desktop) and no_std (embedded) targets.
//!
//! ## Desktop Example
//!
//! ```ignore
//! use nros::prelude::*;
//! use std_msgs::msg::Int32;
//!
//! let config = ExecutorConfig::from_env().node_name("my_node");
//! let mut executor: Executor<_> = Executor::open(&config)?;
//!
//! // Register subscription callback
//! executor.add_subscription::<Int32, _>("/topic", |msg: &Int32| {
//!     println!("Received: {}", msg.data);
//! })?;
//!
//! // Spin (processes callbacks)
//! executor.spin_blocking(SpinOptions::default());
//! ```
//!
//! ## Embedded Example
//!
//! ```ignore
//! use nros::prelude::*;
//! use std_msgs::msg::Int32;
//!
//! let config = ExecutorConfig { locator: "tcp/192.168.1.1:7447", ..Default::default() };
//! let mut executor: Executor<_> = Executor::open(&config)?;
//!
//! // Register subscription callback
//! executor.add_subscription::<Int32, _>("/cmd", |msg: &Int32| {
//!     // process message...
//! })?;
//!
//! // In your main loop:
//! loop {
//!     executor.spin_once(10);
//!     // platform delay...
//! }
//! ```
//!
//! # Features
//!
//! - `std` - Enable standard library support (spin_blocking)
//! - `alloc` - Enable heap allocation (parameter service boxed replies)

#![no_std]

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod config;
pub mod executor;
pub mod lifecycle;
mod node;
mod publisher;
mod subscriber;
pub mod timer;

#[cfg(feature = "param-services")]
pub mod parameter_services;

// Re-export parameter types when param-services is enabled
#[cfg(feature = "param-services")]
pub use nros_params::{ParameterDescriptor, ParameterServer, ParameterType, ParameterValue};

// Export standalone node (without transport)
pub use node::{Node as StandaloneNode, NodeConfig, NodeError as StandaloneNodeError};

pub use publisher::PublisherHandle;
pub use subscriber::SubscriberHandle;

// Re-export transport types for convenience
pub use nros_rmw::{
    ActionInfo, QosDurabilityPolicy, QosHistoryPolicy, QosReliabilityPolicy, QosSettings,
    TopicInfo, TransportConfig, TransportError,
};

// Re-export safety types when feature is enabled
#[cfg(feature = "safety-e2e")]
pub use nros_rmw::{IntegrityStatus, SafetyValidator};

// Re-export options for standalone node when zenoh feature is not enabled
#[cfg(not(feature = "rmw-zenoh"))]
pub use node::{PublisherOptions, SubscriberOptions};

// Re-export session mode (used by ExecutorConfig)
pub use nros_rmw::SessionMode;

// Re-export timer types
pub use timer::{
    DEFAULT_MAX_TIMERS, TimerCallbackFn, TimerDuration, TimerHandle, TimerMode, TimerState,
};

// Re-export lifecycle types
pub use lifecycle::{LifecycleCallbackFn, LifecycleError, LifecyclePollingNode};

// Re-export generic embedded node types (always available, no feature gate)
pub use executor::{
    ActionClient, ActionClientCore, ActionServer, ActionServerCore, ActionServerHandle,
    ActionServerRawHandle, ActiveGoal, CompletedGoal, EmbeddedPublisher, EmbeddedServiceClient,
    EmbeddedServiceServer, Executor, ExecutorConfig, ExecutorSemantics, FeedbackStream,
    GoalFeedbackStream, GuardConditionHandle, HandleId, HandleSet, InvocationMode, Node, NodeError,
    Promise, RawActiveGoal, RawCancelCallback, RawGoalCallback, RawServiceCallback,
    RawSubscriptionCallback, ReadinessSnapshot, SpinOnceResult, SpinOptions,
    SpinPeriodPollingResult, Subscription, Trigger,
};

#[cfg(feature = "std")]
pub use executor::SpinPeriodResult;

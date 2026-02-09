//! Core types, traits, and abstractions for nano-ros
//!
//! This crate provides the foundational types and traits for nano-ros:
//! - `RosMessage` trait for message types
//! - `RosService` trait for service types
//! - `RosAction` trait for action types
//! - `ServiceServer` and `ServiceClient` for service communication
//! - Time and Duration types
//! - Error types

#![no_std]

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod action;
pub mod clock;
pub mod error;
pub mod lifecycle;
pub mod logger;
pub mod message_info;
pub mod service;
pub mod time;
pub mod types;

pub use action::{
    ActionClient, ActionServer, CancelResponse, GoalId, GoalInfo, GoalResponse, GoalStatus,
    GoalStatusStamped, RosAction,
};
pub use clock::{Clock, ClockType};
pub use error::{
    ErrorContext, NanoRosError, NanoRosErrorFilter, NestedError, RclReturnCode, TakeFailedAsNone,
};
pub use lifecycle::{LifecycleState, LifecycleTransition, TransitionResult};
pub use logger::{Logger, OnceFlag};
pub use message_info::{MessageInfo, PUBLISHER_GID_SIZE};
pub use service::{ServiceCallback, ServiceClient, ServiceRequest, ServiceResult, ServiceServer};
pub use time::{Duration, Time};
pub use types::{RosMessage, RosService};

// Re-export serdes types for convenience
pub use nano_ros_serdes::{CdrReader, CdrWriter, DeserError, Deserialize, SerError, Serialize};

// Re-export heapless for generated message types
pub use heapless;

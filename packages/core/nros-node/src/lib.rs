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
//! let mut executor: Executor = Executor::open(&config)?;
//!
//! // Register subscription callback
//! let node = executor.node_builder("my_node").build()?;
//! executor.node_mut(node).create_subscription::<Int32, _>("/topic", |msg: &Int32| {
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
//! let mut executor: Executor = Executor::open(&config)?;
//!
//! // Register subscription callback
//! let node = executor.node_builder("my_node").build()?;
//! executor.node_mut(node).create_subscription::<Int32, _>("/cmd", |msg: &Int32| {
//!     // process message...
//! })?;
//!
//! // In your main loop:
//! loop {
//!     executor.spin_once(core::time::Duration::from_millis(10));
//!     // platform delay...
//! }
//! ```
//!
//! # Features
//!
//! - `std` - Enable standard library support (spin_blocking)
//! - `alloc` - Enable heap allocation (parameter service boxed replies)

#![no_std]

// Phase 248 (C2) — `nros-rmw-cyclonedds[-sys]` deps removed (issue #60,
// Tier 1). Per-type descriptor registration is now the generic
// `nros_rmw::register_type_descriptor` seam (see `rmw_type_registry`);
// the Cyclone backend installs its registrar from its own crate. The
// `__cyclonedds-link` marker feature (no dep edge) still emits
// `cfg(rmw_needs_type_descriptors)` to compile the schema-passing body +
// `M: Message` bound for builds where a descriptor-needing backend is
// linked by the umbrella.

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod c_waker;
pub mod config;
/// Phase 212.K.7.6.b — runtime cyclonedds type-descriptor registry hook.
pub mod rmw_type_registry;

/// Server-discovery probe cadence (issue #224 — one shared constant; was
/// independently defined at four call sites across nros-node and nros-c).
/// One probe per second balances "see freshly-declared tokens quickly"
/// against "burn fewer FFI round-trips on a healthy network"; the outer
/// wall-clock budget is the caller's.
pub const SERVER_DISCOVERY_PROBE_TIMEOUT_MS: u32 = 1000;
pub mod executor;
pub mod lifecycle;
pub mod limits;
mod node;
mod publisher;
#[cfg(any(has_rmw, test))]
pub mod session;
mod subscriber;
pub mod timer;

// MockSession only matters when neither a real RMW backend feature
// nor lifecycle-services is enabled — the same gate as
// `session::ConcreteSession = MockSession` and the executor tests in
// `executor/mod.rs:42`. Compiling mock.rs unconditionally under
// `cfg(test)` produced "never constructed / never used" warnings on
// `cargo build --tests` when feature-unification activated a real
// RMW backend (e.g. workspace builds with `rmw-uorb` on).
#[cfg(all(test, not(feature = "rmw-cffi")))]
pub(crate) mod mock;

// Issue 0092 — the service servers these modules build (`executor::
// EmbeddedServiceServer`) only exist when an RMW backend is present
// (`#[cfg(any(has_rmw, test))]` on `executor::handles`). Gate the modules on
// `has_rmw` too — `--features {lifecycle,param}-services` with no RMW otherwise
// fails to resolve `EmbeddedServiceServer`. Service servers are meaningless
// without a backend; every shipping app/entry selects an RMW (→ has_rmw). The
// `test` arm keeps the modules in test builds.
#[cfg(all(feature = "param-services", any(has_rmw, test)))]
pub mod parameter_services;

// Re-export parameter types when param-services is enabled
#[cfg(feature = "param-services")]
pub use nros_params::{
    ParameterDescriptor, ParameterServer, ParameterType, ParameterValue, SetParameterResult,
};

#[cfg(all(feature = "lifecycle-services", any(has_rmw, test)))]
pub mod lifecycle_services;

// Export standalone node (without transport)
pub use node::{Node as StandaloneNode, NodeConfig, NodeError as StandaloneNodeError};

pub use publisher::PublisherHandle;
pub use subscriber::SubscriberHandle;

// Re-export transport types for convenience
pub use nros_rmw::{
    ActionInfo, QosDurabilityPolicy, QosHistoryPolicy, QosLivelinessPolicy, QosPolicyMask,
    QosReliabilityPolicy, QosSettings, ServiceInfo, TopicInfo, TransportConfig, TransportError,
};

// Re-export RMW protocol traits so thin wrappers (nros-c, nros-cpp) can
// pull them through nros-node instead of going around it. Phase 91.B.
pub use nros_rmw::{Publisher, ServiceClientTrait, ServiceServerTrait, Session, Subscriber};

// Re-export action protocol types from nros-core. Same motivation as the
// RMW trait re-exports above — keeps thin wrappers off the
// nros-core::* path. Phase 91.B5.
pub use nros_core::{CancelResponse, GoalId, GoalResponse, GoalStatus};

// Re-export lifecycle protocol types. Phase 91.B2.
pub use nros_core::lifecycle::{LifecycleState, LifecycleTransition, TransitionResult};

// Re-export CDR ser/de types so the C-side serialization helpers in
// nros-c/src/cdr.rs don't have to reach past nros-node either. These
// are themselves re-exports from nros-serdes via nros-core; collecting
// them here keeps the import boundary uniform. Phase 91.B6.
pub use nros_core::{CdrReader, CdrWriter, DeserError, SerError};

// Re-export safety types when feature is enabled
#[cfg(feature = "safety-e2e")]
pub use nros_rmw::{IntegrityStatus, SafetyValidator};

// Re-export publisher/subscriber options (topic + QoS; backend-agnostic).
pub use node::{PublisherOptions, SubscriberOptions};

// Re-export session mode (used by ExecutorConfig)
pub use nros_rmw::SessionMode;

// Re-export timer types
pub use timer::{TimerCallbackFn, TimerDuration, TimerHandle, TimerMode, TimerState};

// Re-export lifecycle types
pub use lifecycle::{LifecycleCallbackFn, LifecycleError, LifecyclePollingNode};

// Re-export types that don't depend on RMW (always available)
pub use executor::{
    BOOT_SET_DOMAIN, BOOT_SET_LOCATOR, BOOT_SET_NAMESPACE, BOOT_SET_NODE_NAME, BakedBootConfig,
    BootConfig, BootConfigError, DOMAIN_ID_EXPLICIT_ZERO_C_ABI, DOMAIN_ID_MAX, ExecutorConfig,
    ExecutorSemantics, GuardConditionHandle, HandleId, HandleSet, InvocationMode,
    NROS_BOOT_CONFIG_MAGIC, NROS_BOOT_CONFIG_VERSION, NodeError, RawAcceptedCallback,
    RawCancelCallback, RawGoalCallback, RawResponseCallback, RawServiceCallback,
    RawSubscriptionCallback, ReadinessSnapshot, SpinOnceResult, SpinOptions,
    SpinPeriodPollingResult, Trigger, baked_domain_from_c_abi,
};

// Re-export RMW-dependent executor types
#[cfg(any(has_rmw, test))]
pub use executor::{
    ActionClient, ActionClientCore, ActionServer, ActionServerCore, ActionServerHandle,
    ActionServerRawHandle, ActiveGoal, CallbackGroup, CompletedGoal, EmbeddedPublisher,
    EmbeddedRawPublisher, EmbeddedServiceClient, EmbeddedServiceServer, Executor, FeedbackStream,
    GoalFeedbackStream, LoanError, NodeHandle, Promise, PublishLoan, RawActionClientSpec,
    RawActionServerSpec, RawActiveGoal, RawServiceClient, RawServiceServer, RawSubscription,
    RecvView, SessionHandle, Subscription, executor_storage_layout, executor_storage_u64_len,
};
#[cfg(any(has_rmw, test))]
pub use executor::{ExecutorInlineStorage, ExecutorSizing};

// Phase 173.5 — bridge multi-session spec (consumed by the generated
// orchestration package's `Executor::open_multi`). Gated to match
// `executor::SessionSpec` (needs the cffi vtable surface).
#[cfg(all(any(has_rmw, test), feature = "rmw-cffi"))]
pub use executor::SessionSpec;

#[cfg(all(feature = "std", any(has_rmw, test)))]
pub use executor::SpinPeriodResult;

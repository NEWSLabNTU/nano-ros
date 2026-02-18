//! Unified Executor API for nros
//!
//! This module provides the executor abstraction that works on both std and no_std targets.
//!
//! # Architecture
//!
//! ```text
//! Context → Executor → Node
//! ```
//!
//! - **Context**: Entry point, creates executors
//! - **Executor**: Owns nodes, processes callbacks via `spin_once()`
//! - **Node**: Creates publishers, subscribers, timers
//!
//! # Executor Types
//!
//! - **PollingExecutor**: Always available, no_std compatible. User calls `spin_once()` manually.
//! - **BasicExecutor**: std only. Has blocking `spin()`, `spin_period()`.
//!
//! # Example (RTIC/Embedded)
//!
//! ```ignore
//! let ctx = Context::new(InitOptions::new().locator("tcp/192.168.1.1:7447"))?;
//! let mut executor = ctx.create_polling_executor();
//! let node = executor.create_node("my_node")?;
//!
//! node.create_subscription::<Int32>("/topic", handle_message as fn(&Int32))?;
//!
//! // In your main loop or RTIC task:
//! loop {
//!     executor.spin_once(10);  // 10ms delta
//!     // delay...
//! }
//! ```
//!
//! # Example (Desktop)
//!
//! ```ignore
//! let ctx = Context::new(InitOptions::new())?;
//! let mut executor = ctx.create_basic_executor();
//! let node = executor.create_node("my_node")?;
//!
//! node.create_subscription::<Int32>("/topic", |msg| {
//!     println!("Received: {}", msg.data);
//! })?;
//!
//! executor.spin(SpinOptions::default());
//! ```

use nros_core::{Deserialize, MessageInfo, RosMessage, Time};

use crate::error::RclrsError;
#[cfg(feature = "rmw-zenoh")]
use crate::options::{IntoPublisherOptions, IntoSubscriberOptions};
use crate::timer::TimerDuration;
use crate::trigger::{Trigger, TriggerCondition, TriggerFn};

#[cfg(all(feature = "rmw-zenoh", feature = "alloc"))]
#[allow(unused_imports)] // Some imports only used with specific feature combinations
use crate::{
    ConnectedNode, ConnectedPublisher, ConnectedServiceServer, ConnectedSubscriber,
    DEFAULT_MAX_TIMERS, DEFAULT_MAX_TOKENS, DEFAULT_REPLY_BUFFER_SIZE, DEFAULT_REQ_BUFFER_SIZE,
    DEFAULT_RX_BUFFER_SIZE, IntoNodeOptions, NodeConfig,
};

#[cfg(feature = "rmw-zenoh")]
use nros_rmw::TransportConfig;

// ═══════════════════════════════════════════════════════════════════════════
// SPIN RESULT AND OPTIONS
// ═══════════════════════════════════════════════════════════════════════════

// SpinOnceResult is defined in generic.rs (always available, no feature gate)
pub use crate::generic::SpinOnceResult;

/// Result from a single period of execution (std only)
///
/// Contains the work performed during the period, whether the processing
/// exceeded the period (overrun), and the actual processing time.
#[cfg(feature = "std")]
#[derive(Debug, Clone)]
pub struct SpinPeriodResult {
    /// Work performed during this period
    pub work: SpinOnceResult,
    /// Whether processing exceeded the period (overrun)
    pub overrun: bool,
    /// Actual processing time
    pub elapsed: std::time::Duration,
}

/// Result from a single period of polling execution (no_std compatible)
///
/// Contains the work performed during the period and the remaining time
/// in milliseconds that the caller should sleep. This is no_std compatible —
/// the caller is responsible for the actual delay.
#[derive(Debug, Clone, Copy)]
pub struct SpinPeriodPollingResult {
    /// Work performed during this period
    pub work: SpinOnceResult,
    /// Remaining time in ms that the caller should sleep
    pub remaining_ms: u64,
}

/// Options controlling spin behavior (for BasicExecutor)
#[derive(Debug, Clone, Default)]
pub struct SpinOptions {
    /// Stop after this duration (in milliseconds)
    pub timeout_ms: Option<u64>,
    /// Only process immediately available work (spin_once semantics)
    pub only_next: bool,
    /// Stop after processing this many callbacks
    pub max_callbacks: Option<usize>,
}

impl SpinOptions {
    /// Create default spin options (spin forever)
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

// ═══════════════════════════════════════════════════════════════════════════
// CALLBACK TRAITS
// ═══════════════════════════════════════════════════════════════════════════

/// Trait for subscription callbacks
///
/// Implemented for:
/// - Function pointers `fn(&M)` (no_std compatible, when alloc is disabled)
/// - Any `FnMut(&M) + Send` (includes fn pointers and closures, requires alloc)
pub trait SubscriptionCallback<M: RosMessage>: Send {
    /// Invoke the callback with a message
    fn call(&mut self, msg: &M);
}

// When alloc is enabled: use blanket impl for FnMut (covers both fn pointers and closures)
#[cfg(feature = "alloc")]
impl<M: RosMessage, F: FnMut(&M) + Send> SubscriptionCallback<M> for F {
    fn call(&mut self, msg: &M) {
        (self)(msg)
    }
}

// When alloc is disabled: only support function pointers
#[cfg(not(feature = "alloc"))]
impl<M: RosMessage> SubscriptionCallback<M> for fn(&M) {
    fn call(&mut self, msg: &M) {
        (self)(msg)
    }
}

/// Trait for subscription callbacks that also receive message metadata
///
/// Implemented for:
/// - Function pointers `fn(&M, &MessageInfo)` (no_std compatible, when alloc is disabled)
/// - Any `FnMut(&M, &MessageInfo) + Send` (includes fn pointers and closures, requires alloc)
///
/// # Example
///
/// ```ignore
/// node.create_subscription_with_info::<Int32, _>("/topic", |msg, info| {
///     println!("Received {} at {:?}", msg.data, info.source_timestamp());
/// })?;
/// ```
pub trait SubscriptionCallbackWithInfo<M: RosMessage>: Send {
    /// Invoke the callback with a message and its metadata
    fn call(&mut self, msg: &M, info: &MessageInfo);
}

// When alloc is enabled: use blanket impl for FnMut
#[cfg(feature = "alloc")]
impl<M: RosMessage, F: FnMut(&M, &MessageInfo) + Send> SubscriptionCallbackWithInfo<M> for F {
    fn call(&mut self, msg: &M, info: &MessageInfo) {
        (self)(msg, info)
    }
}

// When alloc is disabled: only support function pointers
#[cfg(not(feature = "alloc"))]
impl<M: RosMessage> SubscriptionCallbackWithInfo<M> for fn(&M, &MessageInfo) {
    fn call(&mut self, msg: &M, info: &MessageInfo) {
        (self)(msg, info)
    }
}

/// Trait for subscription callbacks that also receive E2E safety integrity status
///
/// Implemented for:
/// - Function pointers `fn(&M, &IntegrityStatus)` (no_std compatible, when alloc is disabled)
/// - Any `FnMut(&M, &IntegrityStatus) + Send` (includes fn pointers and closures, requires alloc)
///
/// # Example
///
/// ```ignore
/// node.create_subscription_with_safety::<Int32, _>("/topic", |msg, status| {
///     println!("Received {} (valid={})", msg.data, status.is_valid());
/// })?;
/// ```
#[cfg(feature = "safety-e2e")]
pub trait SubscriptionCallbackWithSafety<M: RosMessage>: Send {
    /// Invoke the callback with a message and its integrity status
    fn call(&mut self, msg: &M, status: &nros_rmw::IntegrityStatus);
}

#[cfg(all(feature = "safety-e2e", feature = "alloc"))]
impl<M: RosMessage, F: FnMut(&M, &nros_rmw::IntegrityStatus) + Send>
    SubscriptionCallbackWithSafety<M> for F
{
    fn call(&mut self, msg: &M, status: &nros_rmw::IntegrityStatus) {
        (self)(msg, status)
    }
}

#[cfg(all(feature = "safety-e2e", not(feature = "alloc")))]
impl<M: RosMessage> SubscriptionCallbackWithSafety<M> for fn(&M, &nros_rmw::IntegrityStatus) {
    fn call(&mut self, msg: &M, status: &nros_rmw::IntegrityStatus) {
        (self)(msg, status)
    }
}

/// Trait for timer callbacks
///
/// Implemented for:
/// - Function pointers `fn()` (no_std compatible, when alloc is disabled)
/// - Any `FnMut() + Send` (includes fn pointers and closures, requires alloc)
pub trait ExecutorTimerCallback: Send {
    /// Invoke the callback
    fn call(&mut self);
}

// When alloc is enabled: use blanket impl for FnMut
#[cfg(feature = "alloc")]
impl<F: FnMut() + Send> ExecutorTimerCallback for F {
    fn call(&mut self) {
        (self)()
    }
}

// When alloc is disabled: only support function pointers
#[cfg(not(feature = "alloc"))]
impl ExecutorTimerCallback for fn() {
    fn call(&mut self) {
        (self)()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SUBSCRIPTION HANDLE
// ═══════════════════════════════════════════════════════════════════════════

/// Handle to a subscription created through NodeHandle
///
/// This handle can be used to cancel the subscription.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubscriptionHandle {
    index: usize,
}

impl SubscriptionHandle {
    pub(crate) fn new(index: usize) -> Self {
        Self { index }
    }

    /// Get the subscription index
    pub fn index(&self) -> usize {
        self.index
    }
}

#[cfg(feature = "rmw-zenoh")]
use nros_core::{RosAction, RosService};

/// Trait for service callbacks
#[cfg(all(feature = "rmw-zenoh", feature = "alloc"))]
pub trait ServiceCallback<S: RosService>: Send {
    /// Invoke the callback with a request and return a reply
    fn call(&mut self, request: &S::Request) -> S::Reply;
}

#[cfg(all(feature = "rmw-zenoh", feature = "alloc"))]
impl<S: RosService, F: FnMut(&S::Request) -> S::Reply + Send> ServiceCallback<S> for F {
    fn call(&mut self, request: &S::Request) -> S::Reply {
        (self)(request)
    }
}

/// Type-erased service callback for storing in executor
#[cfg(all(feature = "rmw-zenoh", feature = "alloc"))]
pub(crate) trait ErasedServiceCallback {
    /// Check if a request is available without consuming it
    fn has_data(&self) -> bool;
    /// Try to receive and handle a service request, returns true if handled
    fn try_handle(&mut self) -> Result<bool, RclrsError>;
}

/// Service entry combining server and callback
#[cfg(all(feature = "rmw-zenoh", feature = "alloc"))]
pub(crate) struct ServiceEntry<
    S: RosService,
    const REQ_BUF: usize = DEFAULT_REQ_BUFFER_SIZE,
    const REPLY_BUF: usize = DEFAULT_REPLY_BUFFER_SIZE,
    C: ServiceCallback<S> = fn(&<S as RosService>::Request) -> <S as RosService>::Reply,
> {
    pub server: ConnectedServiceServer<S, REQ_BUF, REPLY_BUF>,
    pub callback: C,
}

#[cfg(all(feature = "rmw-zenoh", feature = "alloc"))]
impl<
    S: RosService + Send,
    const REQ_BUF: usize,
    const REPLY_BUF: usize,
    C: ServiceCallback<S> + Send,
> ErasedServiceCallback for ServiceEntry<S, REQ_BUF, REPLY_BUF, C>
{
    fn has_data(&self) -> bool {
        self.server.has_request()
    }

    fn try_handle(&mut self) -> Result<bool, RclrsError> {
        self.server
            .handle_request(|req| self.callback.call(req))
            .map_err(|e| e.into())
    }
}

/// Handle to a service created through NodeHandle
#[cfg(feature = "rmw-zenoh")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceHandle {
    index: usize,
}

#[cfg(feature = "rmw-zenoh")]
impl ServiceHandle {
    pub(crate) fn new(index: usize) -> Self {
        Self { index }
    }

    /// Get the service index
    pub fn index(&self) -> usize {
        self.index
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TYPE-ERASED CALLBACK (internal)
// ═══════════════════════════════════════════════════════════════════════════

/// Type-erased subscription callback for storing in executor
#[cfg(feature = "rmw-zenoh")]
pub(crate) trait ErasedCallback {
    /// Check if data is available without consuming it
    fn has_data(&self) -> bool;
    /// Try to receive and process a message, returns true if message was processed
    fn try_process(&mut self) -> Result<bool, RclrsError>;
}

/// Subscription entry combining subscriber and callback
#[cfg(all(feature = "rmw-zenoh", not(feature = "unstable-zenoh-api")))]
pub(crate) struct SubscriptionEntry<
    M: RosMessage,
    const RX_BUF: usize = DEFAULT_RX_BUFFER_SIZE,
    C: SubscriptionCallback<M> = fn(&M),
> {
    pub subscriber: ConnectedSubscriber<M, RX_BUF>,
    pub callback: C,
}

#[cfg(all(feature = "rmw-zenoh", not(feature = "unstable-zenoh-api")))]
impl<M: RosMessage + Deserialize + Send, const RX_BUF: usize, C: SubscriptionCallback<M>>
    ErasedCallback for SubscriptionEntry<M, RX_BUF, C>
{
    fn has_data(&self) -> bool {
        self.subscriber.has_data()
    }

    fn try_process(&mut self) -> Result<bool, RclrsError> {
        let callback = &mut self.callback;
        self.subscriber
            .process_in_place(|msg| {
                callback.call(msg);
            })
            .map_err(|_| RclrsError::DeserializationFailed)
    }
}

/// Subscription entry combining subscriber and callback with MessageInfo
#[cfg(all(feature = "rmw-zenoh", not(feature = "unstable-zenoh-api")))]
pub(crate) struct SubscriptionEntryWithInfo<
    M: RosMessage,
    const RX_BUF: usize = DEFAULT_RX_BUFFER_SIZE,
    C: SubscriptionCallbackWithInfo<M> = fn(&M, &MessageInfo),
> {
    pub subscriber: ConnectedSubscriber<M, RX_BUF>,
    pub callback: C,
}

#[cfg(all(feature = "rmw-zenoh", not(feature = "unstable-zenoh-api")))]
impl<M: RosMessage + Deserialize + Send, const RX_BUF: usize, C: SubscriptionCallbackWithInfo<M>>
    ErasedCallback for SubscriptionEntryWithInfo<M, RX_BUF, C>
{
    fn has_data(&self) -> bool {
        self.subscriber.has_data()
    }

    fn try_process(&mut self) -> Result<bool, RclrsError> {
        let callback = &mut self.callback;
        self.subscriber
            .process_in_place_with_info(|msg, info| {
                callback.call(msg, info);
            })
            .map_err(|_| RclrsError::DeserializationFailed)
    }
}

/// Subscription entry combining subscriber and callback with safety integrity status
#[cfg(all(feature = "rmw-zenoh", feature = "safety-e2e"))]
pub(crate) struct SubscriptionEntryWithSafety<
    M: RosMessage,
    const RX_BUF: usize = DEFAULT_RX_BUFFER_SIZE,
    C: SubscriptionCallbackWithSafety<M> = fn(&M, &nros_rmw::IntegrityStatus),
> {
    pub subscriber: ConnectedSubscriber<M, RX_BUF>,
    pub callback: C,
}

#[cfg(all(feature = "rmw-zenoh", feature = "safety-e2e"))]
impl<M: RosMessage + Deserialize + Send, const RX_BUF: usize, C: SubscriptionCallbackWithSafety<M>>
    ErasedCallback for SubscriptionEntryWithSafety<M, RX_BUF, C>
{
    fn has_data(&self) -> bool {
        self.subscriber.has_data()
    }

    fn try_process(&mut self) -> Result<bool, RclrsError> {
        // Safety-e2e still uses the copy-based path because try_recv_validated
        // needs CRC verification on the buffer copy. In-place would require
        // running the CRC check inside the lock window — acceptable but
        // deferred to keep this change focused.
        match self.subscriber.try_recv_safe() {
            Ok(Some((msg, status))) => {
                self.callback.call(&msg, &status);
                Ok(true)
            }
            Ok(None) => Ok(false),
            Err(_) => Err(RclrsError::DeserializationFailed),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// ZERO-COPY SUBSCRIPTION ENTRY (unstable-zenoh-api)
// ═══════════════════════════════════════════════════════════════════════════

/// Subscription entry for zero-copy mode.
///
/// Data is consumed inline by the callback during the zenoh-pico receive path.
/// The executor only needs to keep this entry alive — `has_data()` always
/// returns false because there is no buffer to poll.
#[cfg(all(
    feature = "rmw-zenoh",
    feature = "unstable-zenoh-api",
    feature = "alloc"
))]
pub(crate) struct SubscriptionEntryZeroCopy {
    _subscriber: nros_rmw_zenoh::ShimZeroCopySubscriber,
}

#[cfg(all(
    feature = "rmw-zenoh",
    feature = "unstable-zenoh-api",
    feature = "alloc"
))]
impl ErasedCallback for SubscriptionEntryZeroCopy {
    fn has_data(&self) -> bool {
        false // Data is consumed inline by the zero-copy callback
    }

    fn try_process(&mut self) -> Result<bool, RclrsError> {
        Ok(false) // Nothing to poll — all processing happens in callback
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// NODE STATE (internal)
// ═══════════════════════════════════════════════════════════════════════════

/// Maximum subscriptions per node (for no_std)
pub const DEFAULT_MAX_SUBSCRIPTIONS: usize = 8;

/// Maximum services per node
pub const DEFAULT_MAX_SERVICES: usize = 4;

/// Internal node state owned by executor
#[cfg(feature = "rmw-zenoh")]
pub struct NodeState<
    const MAX_TOKENS: usize = DEFAULT_MAX_TOKENS,
    const MAX_TIMERS: usize = DEFAULT_MAX_TIMERS,
    const MAX_SUBS: usize = DEFAULT_MAX_SUBSCRIPTIONS,
    const MAX_SERVICES: usize = DEFAULT_MAX_SERVICES,
> {
    /// The underlying connected node
    pub(crate) inner: ConnectedNode<MAX_TOKENS, MAX_TIMERS>,
    /// Subscriptions with their callbacks (boxed for type erasure)
    #[cfg(feature = "alloc")]
    pub(crate) subscriptions: alloc::vec::Vec<alloc::boxed::Box<dyn ErasedCallback>>,
    #[cfg(not(feature = "alloc"))]
    pub(crate) subscriptions: (),
    /// Services with their callbacks (boxed for type erasure)
    #[cfg(feature = "alloc")]
    pub(crate) services: alloc::vec::Vec<alloc::boxed::Box<dyn ErasedServiceCallback>>,
    #[cfg(not(feature = "alloc"))]
    pub(crate) services: (),
    /// Parameter service servers (boxed to avoid 48KB+ on stack)
    #[cfg(feature = "param-services")]
    pub(crate) param_services:
        Option<alloc::boxed::Box<crate::parameter_services::ParameterServiceServers>>,
}

#[cfg(feature = "rmw-zenoh")]
impl<
    const MAX_TOKENS: usize,
    const MAX_TIMERS: usize,
    const MAX_SUBS: usize,
    const MAX_SERVICES: usize,
> NodeState<MAX_TOKENS, MAX_TIMERS, MAX_SUBS, MAX_SERVICES>
{
    /// Create a new node state
    pub(crate) fn new(inner: ConnectedNode<MAX_TOKENS, MAX_TIMERS>) -> Self {
        Self {
            inner,
            #[cfg(feature = "alloc")]
            subscriptions: alloc::vec::Vec::new(),
            #[cfg(not(feature = "alloc"))]
            subscriptions: (),
            #[cfg(feature = "alloc")]
            services: alloc::vec::Vec::new(),
            #[cfg(not(feature = "alloc"))]
            services: (),
            #[cfg(feature = "param-services")]
            param_services: None,
        }
    }

    /// Get the node name
    pub fn name(&self) -> &str {
        self.inner.name()
    }

    /// Get the node namespace
    pub fn namespace(&self) -> &str {
        self.inner.namespace()
    }

    /// Process all subscriptions, returns count of messages processed
    #[cfg(feature = "alloc")]
    pub(crate) fn process_subscriptions(&mut self) -> Result<usize, RclrsError> {
        let mut count = 0;
        for sub in &mut self.subscriptions {
            while sub.try_process()? {
                count += 1;
            }
        }
        Ok(count)
    }

    /// Process all services, returns count of requests handled
    #[cfg(feature = "alloc")]
    pub(crate) fn process_services(&mut self) -> Result<usize, RclrsError> {
        let mut count = 0;
        for srv in &mut self.services {
            if srv.try_handle()? {
                count += 1;
            }
        }

        // Process parameter services separately (split borrow pattern)
        #[cfg(feature = "param-services")]
        if let Some(ref mut param_srv) = self.param_services {
            count += param_srv
                .process(&mut self.inner.parameter_server)
                .map_err(RclrsError::from)?;
        }

        Ok(count)
    }

    /// Process timers
    pub(crate) fn process_timers(&mut self, delta_ms: u64) -> usize {
        self.inner.process_timers(delta_ms)
    }

    /// Poll for incoming data (RTIC/polling mode)
    #[cfg(any(feature = "rtic", feature = "polling"))]
    pub(crate) fn poll_read(&mut self) -> Result<(), RclrsError> {
        self.inner.poll_read().map_err(|_| RclrsError::PollFailed)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// NODE HANDLE
// ═══════════════════════════════════════════════════════════════════════════

/// Handle to a node owned by an executor
///
/// This handle provides access to create publishers, subscribers, timers, etc.
/// The actual node data is owned by the executor.
#[cfg(feature = "rmw-zenoh")]
pub struct NodeHandle<
    'a,
    const MAX_TOKENS: usize = DEFAULT_MAX_TOKENS,
    const MAX_TIMERS: usize = DEFAULT_MAX_TIMERS,
    const MAX_SUBS: usize = DEFAULT_MAX_SUBSCRIPTIONS,
    const MAX_SERVICES: usize = DEFAULT_MAX_SERVICES,
> {
    pub(crate) node: &'a mut NodeState<MAX_TOKENS, MAX_TIMERS, MAX_SUBS, MAX_SERVICES>,
}

#[cfg(feature = "rmw-zenoh")]
impl<
    'a,
    const MAX_TOKENS: usize,
    const MAX_TIMERS: usize,
    const MAX_SUBS: usize,
    const MAX_SERVICES: usize,
> NodeHandle<'a, MAX_TOKENS, MAX_TIMERS, MAX_SUBS, MAX_SERVICES>
{
    /// Create a new node handle
    pub(crate) fn new(
        node: &'a mut NodeState<MAX_TOKENS, MAX_TIMERS, MAX_SUBS, MAX_SERVICES>,
    ) -> Self {
        Self { node }
    }

    /// Get the node name
    pub fn name(&self) -> &str {
        self.node.name()
    }

    /// Get the node namespace
    pub fn namespace(&self) -> &str {
        self.node.namespace()
    }

    /// Get the fully qualified node name
    #[cfg(feature = "alloc")]
    pub fn fully_qualified_name(&self) -> alloc::string::String {
        let ns = self.namespace();
        let name = self.name();
        if ns == "/" {
            alloc::format!("/{}", name)
        } else {
            alloc::format!("{}/{}", ns, name)
        }
    }

    /// Get the node's clock
    pub fn get_clock(&self) -> &nros_core::Clock {
        self.node.inner.get_clock()
    }

    /// Get the current time from the node's clock
    pub fn now(&self) -> Time {
        self.node.inner.now()
    }

    /// Get a logger for this node
    ///
    /// The logger includes the node name in log output for context.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let logger = node.logger();
    /// logger.info("Node started");
    /// logger.warn("Low battery");
    /// ```
    pub fn logger(&self) -> nros_core::Logger<'_> {
        nros_core::Logger::new(self.node.inner.name())
    }

    /// Register the 6 ROS 2 parameter services for this node.
    ///
    /// After calling this, `ros2 param list`, `ros2 param get`, and `ros2 param set`
    /// will work with this node's parameters.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let node = executor.create_node("my_node")?;
    /// node.declare_parameter("speed", 1.0)?;
    /// node.register_parameter_services()?;
    /// // Now: ros2 param get /my_node speed → 1.0
    /// ```
    #[cfg(feature = "param-services")]
    pub fn register_parameter_services(&mut self) -> Result<(), RclrsError> {
        let servers = self
            .node
            .inner
            .register_parameter_services()
            .map_err(RclrsError::from)?;
        self.node.param_services = Some(alloc::boxed::Box::new(servers));
        Ok(())
    }

    /// Create a publisher for the given topic
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Simple: topic string only (uses default QoS)
    /// let pub1 = node.create_publisher::<Int32>("/chatter")?;
    ///
    /// // Fluent: topic with QoS options
    /// let pub2 = node.create_publisher::<Int32>(
    ///     PublisherOptions::new("/chatter").reliable().keep_last(10)
    /// )?;
    /// ```
    pub fn create_publisher<'b, M: RosMessage>(
        &mut self,
        options: impl IntoPublisherOptions<'b>,
    ) -> Result<ConnectedPublisher<M>, RclrsError> {
        self.node
            .inner
            .create_publisher::<M>(options.into_publisher_options())
            .map_err(|_| RclrsError::PublisherCreationFailed)
    }

    /// Create a subscription with a callback
    ///
    /// The callback will be invoked during `executor.spin_once()` when messages arrive.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Simple: topic string only (uses default QoS)
    /// node.create_subscription::<Int32, _>("/topic", |msg| {
    ///     println!("Received: {}", msg.data);
    /// })?;
    ///
    /// // Fluent: topic with QoS options
    /// node.create_subscription::<Int32, _>(
    ///     SubscriberOptions::new("/topic").reliable().keep_last(10),
    ///     |msg| { println!("Received: {}", msg.data); }
    /// )?;
    /// ```
    #[cfg(feature = "alloc")]
    pub fn create_subscription<'b, M, C>(
        &mut self,
        options: impl IntoSubscriberOptions<'b>,
        callback: C,
    ) -> Result<SubscriptionHandle, RclrsError>
    where
        M: RosMessage + Deserialize + Send + 'static,
        C: SubscriptionCallback<M> + 'static,
    {
        if self.node.subscriptions.len() >= MAX_SUBS {
            return Err(RclrsError::SubscriptionStorageFull);
        }

        let opts = options.into_subscriber_options();

        // Zero-copy path: deserialize directly from zenoh-pico's receive buffer
        #[cfg(feature = "unstable-zenoh-api")]
        {
            let mut callback = callback;
            let sub = self
                .node
                .inner
                .create_zero_copy_subscriber::<M>(opts, move |raw, _info| {
                    use nros_core::CdrReader;
                    if let Ok(mut reader) = CdrReader::new_with_header(raw)
                        && let Ok(msg) = M::deserialize(&mut reader)
                    {
                        callback.call(&msg);
                    }
                })
                .map_err(|_| RclrsError::SubscriberCreationFailed)?;

            let entry = SubscriptionEntryZeroCopy { _subscriber: sub };
            let index = self.node.subscriptions.len();
            self.node.subscriptions.push(alloc::boxed::Box::new(entry));
            Ok(SubscriptionHandle::new(index))
        }

        // Standard poll-based path (direct-write + in-place deserialization)
        #[cfg(not(feature = "unstable-zenoh-api"))]
        {
            let subscriber = self
                .node
                .inner
                .create_subscriber_sized::<M, DEFAULT_RX_BUFFER_SIZE>(opts)
                .map_err(|_| RclrsError::SubscriberCreationFailed)?;

            let entry = SubscriptionEntry {
                subscriber,
                callback,
            };

            let index = self.node.subscriptions.len();
            self.node.subscriptions.push(alloc::boxed::Box::new(entry));

            Ok(SubscriptionHandle::new(index))
        }
    }

    /// Create a subscription with a callback that also receives message metadata
    ///
    /// The callback will be invoked during `executor.spin_once()` when messages arrive.
    /// The callback receives both the message and `MessageInfo` containing metadata
    /// such as timestamps and publisher GID.
    ///
    /// # Example
    ///
    /// ```ignore
    /// node.create_subscription_with_info::<Int32, _>("/topic", |msg, info| {
    ///     println!("Received {} at {:?}", msg.data, info.source_timestamp());
    /// })?;
    /// ```
    ///
    /// # Note
    ///
    /// Currently MessageInfo contains default values as the transport layer
    /// doesn't yet extract RMW attachment data on receive.
    #[cfg(feature = "alloc")]
    pub fn create_subscription_with_info<'b, M, C>(
        &mut self,
        options: impl IntoSubscriberOptions<'b>,
        callback: C,
    ) -> Result<SubscriptionHandle, RclrsError>
    where
        M: RosMessage + Deserialize + Send + 'static,
        C: SubscriptionCallbackWithInfo<M> + 'static,
    {
        if self.node.subscriptions.len() >= MAX_SUBS {
            return Err(RclrsError::SubscriptionStorageFull);
        }

        let opts = options.into_subscriber_options();

        // Zero-copy path: deserialize directly from zenoh-pico's receive buffer
        #[cfg(feature = "unstable-zenoh-api")]
        {
            let mut callback = callback;
            let sub = self
                .node
                .inner
                .create_zero_copy_subscriber::<M>(opts, move |raw, info| {
                    use nros_core::{CdrReader, Time};
                    if let Ok(mut reader) = CdrReader::new_with_header(raw)
                        && let Ok(msg) = M::deserialize(&mut reader)
                    {
                        let mut msg_info = MessageInfo::new();
                        if let Some(ti) = info {
                            let secs = (ti.timestamp_ns / 1_000_000_000) as i32;
                            let nsecs = (ti.timestamp_ns % 1_000_000_000) as u32;
                            msg_info.set_source_timestamp(Time::new(secs, nsecs));
                            msg_info.set_publication_sequence_number(ti.sequence_number);
                            msg_info.set_publisher_gid(ti.publisher_gid);
                        }
                        callback.call(&msg, &msg_info);
                    }
                })
                .map_err(|_| RclrsError::SubscriberCreationFailed)?;

            let entry = SubscriptionEntryZeroCopy { _subscriber: sub };
            let index = self.node.subscriptions.len();
            self.node.subscriptions.push(alloc::boxed::Box::new(entry));
            Ok(SubscriptionHandle::new(index))
        }

        // Standard poll-based path
        #[cfg(not(feature = "unstable-zenoh-api"))]
        {
            let subscriber = self
                .node
                .inner
                .create_subscriber_sized::<M, DEFAULT_RX_BUFFER_SIZE>(opts)
                .map_err(|_| RclrsError::SubscriberCreationFailed)?;

            let entry = SubscriptionEntryWithInfo {
                subscriber,
                callback,
            };

            let index = self.node.subscriptions.len();
            self.node.subscriptions.push(alloc::boxed::Box::new(entry));

            Ok(SubscriptionHandle::new(index))
        }
    }

    /// Create a subscription with a callback that receives E2E safety integrity status.
    ///
    /// The callback will be invoked during `executor.spin_once()` when messages arrive.
    /// The callback receives both the message and `IntegrityStatus` containing
    /// sequence gap detection, duplicate detection, and CRC validation results.
    ///
    /// # Example
    ///
    /// ```ignore
    /// node.create_subscription_with_safety::<Int32, _>(
    ///     SubscriberOptions::new("/chatter").reliable().keep_last(10),
    ///     |msg, status| {
    ///         println!("Received {} (valid={})", msg.data, status.is_valid());
    ///     },
    /// )?;
    /// ```
    #[cfg(all(feature = "alloc", feature = "safety-e2e"))]
    pub fn create_subscription_with_safety<'b, M, C>(
        &mut self,
        options: impl IntoSubscriberOptions<'b>,
        callback: C,
    ) -> Result<SubscriptionHandle, RclrsError>
    where
        M: RosMessage + Deserialize + Send + 'static,
        C: SubscriptionCallbackWithSafety<M> + 'static,
    {
        if self.node.subscriptions.len() >= MAX_SUBS {
            return Err(RclrsError::SubscriptionStorageFull);
        }

        let subscriber = self
            .node
            .inner
            .create_subscriber_sized::<M, DEFAULT_RX_BUFFER_SIZE>(options.into_subscriber_options())
            .map_err(|_| RclrsError::SubscriberCreationFailed)?;

        let entry = SubscriptionEntryWithSafety {
            subscriber,
            callback,
        };

        let index = self.node.subscriptions.len();
        self.node.subscriptions.push(alloc::boxed::Box::new(entry));

        Ok(SubscriptionHandle::new(index))
    }

    /// Create a service with a callback.
    ///
    /// The callback will be invoked during `executor.spin_once()` when a request arrives.
    #[cfg(feature = "alloc")]
    pub fn create_service<S, C>(
        &mut self,
        service_name: &str,
        callback: C,
    ) -> Result<ServiceHandle, RclrsError>
    where
        S: RosService + Send + 'static,
        C: ServiceCallback<S> + 'static,
    {
        if self.node.services.len() >= MAX_SERVICES {
            return Err(RclrsError::ServiceStorageFull);
        }

        let server = self
            .node
            .inner
            .create_service::<S>(service_name)
            .map_err(RclrsError::from)?;

        let entry = ServiceEntry { server, callback };

        let index = self.node.services.len();
        self.node.services.push(alloc::boxed::Box::new(entry));

        Ok(ServiceHandle::new(index))
    }

    /// Create a service client.
    #[cfg(all(feature = "rmw-zenoh", feature = "alloc"))]
    pub fn create_client<S: RosService>(
        &mut self,
        service_name: &str,
    ) -> Result<crate::ConnectedServiceClient<S>, RclrsError> {
        self.node
            .inner
            .create_client::<S>(service_name)
            .map_err(RclrsError::from)
    }

    /// Create an action server.
    #[cfg(all(feature = "rmw-zenoh", feature = "alloc"))]
    pub fn create_action_server<A: RosAction>(
        &mut self,
        action_name: &str,
    ) -> Result<crate::ConnectedActionServer<A>, RclrsError> {
        self.node
            .inner
            .create_action_server::<A>(action_name)
            .map_err(RclrsError::from)
    }

    /// Create an action client.
    #[cfg(all(feature = "rmw-zenoh", feature = "alloc"))]
    pub fn create_action_client<A: RosAction>(
        &mut self,
        action_name: &str,
    ) -> Result<crate::ConnectedActionClient<A>, RclrsError> {
        self.node
            .inner
            .create_action_client::<A>(action_name)
            .map_err(RclrsError::from)
    }

    /// Create a timer with a callback
    ///
    /// The callback will be invoked during `executor.spin_once()` when the timer fires.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Function pointer
    /// fn on_timer() {
    ///     println!("Timer fired!");
    /// }
    /// node.create_timer(TimerDuration::from_millis(1000), on_timer)?;
    ///
    /// // Closure (requires alloc)
    /// node.create_timer(TimerDuration::from_millis(1000), || {
    ///     println!("Timer fired!");
    /// })?;
    /// ```
    pub fn create_timer(
        &mut self,
        period: TimerDuration,
        callback: crate::timer::TimerCallbackFn,
    ) -> Result<crate::timer::TimerHandle, RclrsError> {
        self.node
            .inner
            .create_timer_repeating(period, callback)
            .map_err(|_| RclrsError::TimerCreationFailed)
    }

    /// Create a timer with a boxed callback (requires alloc)
    #[cfg(feature = "alloc")]
    pub fn create_timer_boxed<F>(
        &mut self,
        period: TimerDuration,
        callback: F,
    ) -> Result<crate::timer::TimerHandle, RclrsError>
    where
        F: FnMut() + Send + 'static,
    {
        self.node
            .inner
            .create_timer_repeating_boxed(period, callback)
            .map_err(|_| RclrsError::TimerCreationFailed)
    }

    /// Declare a typed parameter for this node.
    ///
    /// This returns a `ParameterBuilder` which provides a fluent API for
    /// configuring and declaring the parameter.
    ///
    /// # Type Parameters
    ///
    /// - `T`: The Rust type of the parameter, which must implement `ParameterVariant`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let speed: MandatoryParameter<f64> = node
    ///     .declare_parameter("speed")
    ///     .default(5.0)
    ///     .description("Maximum speed in m/s")
    ///     .float_range(0.0, 10.0, 0.1)?
    ///     .mandatory()?;
    /// ```
    pub fn declare_parameter<'b, T: nros_params::ParameterVariant>(
        &'b mut self,
        name: &'b str,
    ) -> nros_params::ParameterBuilder<'b, T> {
        // Need to pass a mutable reference to the inner parameter_server
        nros_params::ParameterBuilder::new(&mut self.node.inner.parameter_server, name)
    }

    /// Provides an interface for dynamically accessing parameters not explicitly
    /// declared via `declare_parameter`.
    ///
    /// This allows interaction with parameters that might be set externally
    /// or are not known at compile time.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let undeclared = node.use_undeclared_parameters();
    /// if let Some(value) = undeclared.get_integer("some_dynamic_param") {
    ///     println!("Dynamic param: {}", value);
    /// }
    /// ```
    #[cfg(feature = "rmw-zenoh")]
    pub fn use_undeclared_parameters(&mut self) -> nros_params::UndeclaredParameters<'_> {
        nros_params::UndeclaredParameters::new(&mut self.node.inner.parameter_server)
    }

    // ═══════════════════════════════════════════════════════════════════════
    // CAPACITY QUERIES
    // ═══════════════════════════════════════════════════════════════════════

    /// Remaining liveliness token capacity (publishers + subscribers)
    pub fn remaining_token_capacity(&self) -> usize {
        self.node.inner.remaining_token_capacity()
    }

    /// Remaining timer capacity
    pub fn remaining_timer_capacity(&self) -> usize {
        self.node.inner.remaining_timer_capacity()
    }

    /// Remaining subscription callback capacity
    #[cfg(feature = "alloc")]
    pub fn remaining_subscription_capacity(&self) -> usize {
        MAX_SUBS - self.node.subscriptions.len()
    }

    /// Remaining service callback capacity
    #[cfg(feature = "alloc")]
    pub fn remaining_service_capacity(&self) -> usize {
        MAX_SERVICES - self.node.services.len()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// EXECUTOR TRAIT
// ═══════════════════════════════════════════════════════════════════════════

/// Common trait for all executors
///
/// This trait provides the minimal interface that all executors implement.
/// Use this for writing generic code that works with any executor type.
///
/// # Example
///
/// ```ignore
/// fn setup_robot<E: Executor>(executor: &mut E) -> Result<(), RclrsError> {
///     let node = executor.create_node_with_handle("robot")?;
///     // ...
///     Ok(())
/// }
/// ```
#[cfg(feature = "rmw-zenoh")]
pub trait Executor {
    /// Process one iteration of pending work
    ///
    /// - Polls transport for incoming messages
    /// - Invokes subscription callbacks for received messages
    /// - Fires ready timers
    ///
    /// # Arguments
    /// * `delta_ms` - Time elapsed since last call (for timer processing)
    ///
    /// # Returns
    /// Result with counts of processed items
    fn spin_once(&mut self, delta_ms: u64) -> SpinOnceResult;
}

/// Extended trait for executors with blocking/async spin (std only)
#[cfg(all(feature = "rmw-zenoh", feature = "std"))]
pub trait SpinExecutor: Executor {
    /// Blocking spin loop
    ///
    /// Runs until completion (halt, timeout, or max callbacks reached).
    ///
    /// # Returns
    /// `Ok(())` on normal completion, `Err` if an error occurred during spinning.
    fn spin(&mut self, opts: SpinOptions) -> Result<(), RclrsError>;

    /// Spin at a fixed rate, compensating for processing time.
    ///
    /// Blocks until halted. Uses wall-clock time to maintain the target rate.
    fn spin_period(&mut self, period: std::time::Duration) -> Result<(), RclrsError>;
}
// ═══════════════════════════════════════════════════════════════════════════
// POLLING EXECUTOR (no_std compatible)
// ═══════════════════════════════════════════════════════════════════════════

/// Maximum number of nodes in a PollingExecutor
pub const DEFAULT_MAX_NODES: usize = 4;

/// Executor for manual polling (RTIC, Embassy, bare-metal)
///
/// This executor requires the user to call `spin_once()` periodically.
/// It does NOT spawn background threads or use async runtimes.
///
/// # Type Parameters
///
/// - `MAX_NODES`: Maximum number of nodes this executor can manage
///
/// # Example
///
/// ```ignore
/// let ctx = Context::new(InitOptions::new().locator("tcp/192.168.1.1:7447"))?;
/// let mut executor: PollingExecutor<2> = ctx.create_polling_executor();
///
/// let node = executor.create_node("my_node")?;
/// node.create_subscription::<Int32>("/topic", handle_msg)?;
///
/// // In your main loop:
/// loop {
///     executor.spin_once(10);  // 10ms delta
///     // platform delay...
/// }
/// ```
#[cfg(feature = "rmw-zenoh")]
pub struct PollingExecutor<const MAX_NODES: usize = DEFAULT_MAX_NODES> {
    /// Domain ID for creating nodes
    domain_id: u32,
    /// Transport configuration
    transport_config: TransportConfig<'static>,
    /// Nodes owned by this executor
    nodes: heapless::Vec<NodeState, MAX_NODES>,
    /// Trigger condition controlling when callbacks are processed
    trigger: Trigger,
}

#[cfg(feature = "rmw-zenoh")]
impl<const MAX_NODES: usize> PollingExecutor<MAX_NODES> {
    /// Create a new polling executor
    pub(crate) fn new(domain_id: u32, transport_config: TransportConfig<'static>) -> Self {
        Self {
            domain_id,
            transport_config,
            nodes: heapless::Vec::new(),
            trigger: Trigger::default(),
        }
    }

    /// Create a node managed by this executor
    ///
    /// Returns a `NodeHandle` that can be used to create publishers, subscribers, etc.
    /// The node is owned by the executor and will be processed during `spin_once()`.
    pub fn create_node<'a, 'b>(
        &'a mut self,
        opts: impl IntoNodeOptions<'b>,
    ) -> Result<NodeHandle<'a>, RclrsError> {
        let node_opts = opts.into_node_options();

        let config = NodeConfig {
            name: node_opts.name,
            namespace: node_opts.namespace.unwrap_or("/"),
            domain_id: self.domain_id,
        };

        let inner = ConnectedNode::new(config, &self.transport_config)
            .map_err(|_| RclrsError::NodeCreationFailed)?;

        let node_state = NodeState::new(inner);
        self.nodes
            .push(node_state)
            .map_err(|_| RclrsError::ExecutorFull)?;

        let node = self.nodes.last_mut().unwrap();
        Ok(NodeHandle::new(node))
    }

    /// Create a lifecycle-managed node.
    ///
    /// Creates a regular node through the executor and wraps it in a
    /// [`LifecycleNode`] state machine starting in the `Unconfigured` state.
    #[cfg(feature = "alloc")]
    pub fn create_lifecycle_node<'a, 'b>(
        &'a mut self,
        opts: impl IntoNodeOptions<'b>,
    ) -> Result<crate::lifecycle::LifecycleNode<'a>, RclrsError> {
        let handle = self.create_node(opts)?;
        Ok(crate::lifecycle::LifecycleNode::new(handle))
    }

    /// Get the number of nodes in this executor
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Remaining node capacity
    pub fn remaining_node_capacity(&self) -> usize {
        MAX_NODES - self.nodes.len()
    }

    /// Set the trigger condition for this executor
    ///
    /// The trigger controls when `spin_once()` processes callbacks.
    /// Default is `TriggerCondition::Any` (process when any handle has data).
    pub fn set_trigger(&mut self, condition: TriggerCondition) {
        self.trigger = Trigger::Builtin(condition);
    }

    /// Set a custom trigger function for this executor
    ///
    /// The function receives a slice of booleans (one per handle) indicating
    /// which handles have data ready, and returns true if callbacks should be processed.
    pub fn set_custom_trigger(&mut self, trigger: TriggerFn) {
        self.trigger = Trigger::Custom(trigger);
    }

    /// Process one period. Returns remaining time in ms that the caller should sleep.
    ///
    /// This is no_std compatible — the caller is responsible for the actual delay.
    /// The `elapsed_ms` parameter is the time elapsed since the last call, used for
    /// timer processing. The `period_ms` parameter is the target period.
    ///
    /// # Arguments
    /// * `period_ms` - Target period in milliseconds
    /// * `elapsed_ms` - Time elapsed since last call in milliseconds
    ///
    /// # Example
    ///
    /// ```ignore
    /// loop {
    ///     let start = platform_time_ms();
    ///     let result = executor.spin_one_period(10, elapsed_ms);
    ///     if result.remaining_ms > 0 {
    ///         platform_sleep_ms(result.remaining_ms);
    ///     }
    ///     elapsed_ms = platform_time_ms() - start;
    /// }
    /// ```
    pub fn spin_one_period(&mut self, period_ms: u64, elapsed_ms: u64) -> SpinPeriodPollingResult {
        let result = self.spin_once(elapsed_ms);
        SpinPeriodPollingResult {
            work: result,
            remaining_ms: period_ms.saturating_sub(elapsed_ms),
        }
    }

    /// Process one iteration of all nodes
    ///
    /// Call this from your RTIC task or main loop. Typically every 10ms.
    ///
    /// # Arguments
    /// * `delta_ms` - Time elapsed since last call (for timer processing)
    pub fn spin_once(&mut self, delta_ms: u64) -> SpinOnceResult {
        let mut result = SpinOnceResult::new();

        // Poll for incoming data (if using rtic/polling mode)
        #[cfg(any(feature = "rtic", feature = "polling"))]
        for node in &mut self.nodes {
            let _ = node.poll_read();
        }

        // Collect ready mask and evaluate trigger
        #[cfg(feature = "alloc")]
        {
            let mut ready_mask: heapless::Vec<bool, 64> = heapless::Vec::new();

            for node in &self.nodes {
                for sub in &node.subscriptions {
                    let _ = ready_mask.push(sub.has_data());
                }
                for srv in &node.services {
                    let _ = ready_mask.push(srv.has_data());
                }
            }
            // Timers are always evaluated (not gated by trigger)
            // Check trigger condition for subscriptions and services
            if !self.trigger.evaluate(&ready_mask) {
                // Trigger not satisfied — still process timers
                for node in &mut self.nodes {
                    result.timers_fired += node.process_timers(delta_ms);
                }
                return result;
            }
        }

        for node in &mut self.nodes {
            // Process subscriptions
            #[cfg(feature = "alloc")]
            match node.process_subscriptions() {
                Ok(count) => result.subscriptions_processed += count,
                Err(_) => result.subscription_errors += 1,
            }

            // Process services
            #[cfg(feature = "alloc")]
            match node.process_services() {
                Ok(count) => result.services_handled += count,
                Err(_) => result.service_errors += 1,
            }

            // Process timers
            result.timers_fired += node.process_timers(delta_ms);
        }

        result
    }
}

#[cfg(feature = "rmw-zenoh")]
impl<const MAX_NODES: usize> Executor for PollingExecutor<MAX_NODES> {
    fn spin_once(&mut self, delta_ms: u64) -> SpinOnceResult {
        PollingExecutor::spin_once(self, delta_ms)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// BASIC EXECUTOR (std only)
// ═══════════════════════════════════════════════════════════════════════════

/// Full-featured executor with blocking spin (std only)
///
/// Use `spin()` for a blocking spin loop, or `spin_once()` with a timeout
/// for integration with external event loops.
///
/// # Example
///
/// ```ignore
/// let ctx = Context::new(InitOptions::new())?;
/// let mut executor = ctx.create_basic_executor();
///
/// let node = executor.create_node("my_node")?;
/// node.create_subscription::<Int32>("/topic", |msg| {
///     println!("Received: {}", msg.data);
/// })?;
///
/// // Blocking spin
/// executor.spin(SpinOptions::default());
/// ```
#[cfg(all(feature = "rmw-zenoh", feature = "std"))]
pub struct BasicExecutor {
    /// Domain ID for creating nodes
    domain_id: u32,
    /// Transport configuration
    transport_config: TransportConfig<'static>,
    /// Nodes owned by this executor
    nodes: std::vec::Vec<NodeState>,
    /// Flag to request halt
    halt_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Trigger condition controlling when callbacks are processed
    trigger: Trigger,
}

#[cfg(all(feature = "rmw-zenoh", feature = "std"))]
impl BasicExecutor {
    /// Create a new basic executor
    pub(crate) fn new(domain_id: u32, transport_config: TransportConfig<'static>) -> Self {
        Self {
            domain_id,
            transport_config,
            nodes: std::vec::Vec::new(),
            halt_flag: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            trigger: Trigger::default(),
        }
    }

    /// Create a node managed by this executor
    pub fn create_node<'a, 'b>(
        &'a mut self,
        opts: impl IntoNodeOptions<'b>,
    ) -> Result<NodeHandle<'a>, RclrsError> {
        let node_opts = opts.into_node_options();

        let config = NodeConfig {
            name: node_opts.name,
            namespace: node_opts.namespace.unwrap_or("/"),
            domain_id: self.domain_id,
        };

        let inner = ConnectedNode::new(config, &self.transport_config)
            .map_err(|_| RclrsError::NodeCreationFailed)?;

        let node_state = NodeState::new(inner);
        self.nodes.push(node_state);

        let node = self.nodes.last_mut().unwrap();
        Ok(NodeHandle::new(node))
    }

    /// Create a lifecycle-managed node.
    ///
    /// Creates a regular node through the executor and wraps it in a
    /// [`LifecycleNode`] state machine starting in the `Unconfigured` state.
    pub fn create_lifecycle_node<'a, 'b>(
        &'a mut self,
        opts: impl IntoNodeOptions<'b>,
    ) -> Result<crate::lifecycle::LifecycleNode<'a>, RclrsError> {
        let handle = self.create_node(opts)?;
        Ok(crate::lifecycle::LifecycleNode::new(handle))
    }

    /// Get the number of nodes in this executor
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Set the trigger condition for this executor
    ///
    /// The trigger controls when `spin_once()` processes callbacks.
    /// Default is `TriggerCondition::Any` (process when any handle has data).
    pub fn set_trigger(&mut self, condition: TriggerCondition) {
        self.trigger = Trigger::Builtin(condition);
    }

    /// Set a custom trigger function for this executor
    ///
    /// The function receives a slice of booleans (one per handle) indicating
    /// which handles have data ready, and returns true if callbacks should be processed.
    pub fn set_custom_trigger(&mut self, trigger: TriggerFn) {
        self.trigger = Trigger::Custom(trigger);
    }

    /// Set a boxed closure as trigger (std only)
    ///
    /// Like `set_custom_trigger` but accepts a closure that can capture state.
    pub fn set_trigger_fn(&mut self, trigger: impl Fn(&[bool]) -> bool + Send + 'static) {
        self.trigger = Trigger::Boxed(alloc::boxed::Box::new(trigger));
    }

    /// Process one iteration of all nodes
    pub fn spin_once(&mut self, delta_ms: u64) -> SpinOnceResult {
        let mut result = SpinOnceResult::new();

        // Collect ready mask and evaluate trigger
        {
            let mut ready_mask: std::vec::Vec<bool> = std::vec::Vec::new();

            for node in &self.nodes {
                for sub in &node.subscriptions {
                    ready_mask.push(sub.has_data());
                }
                for srv in &node.services {
                    ready_mask.push(srv.has_data());
                }
            }

            if !self.trigger.evaluate(&ready_mask) {
                // Trigger not satisfied — still process timers
                for node in &mut self.nodes {
                    result.timers_fired += node.process_timers(delta_ms);
                }
                return result;
            }
        }

        for node in &mut self.nodes {
            // Process subscriptions
            #[cfg(feature = "alloc")]
            match node.process_subscriptions() {
                Ok(count) => result.subscriptions_processed += count,
                Err(_) => result.subscription_errors += 1,
            }

            // Process services
            #[cfg(feature = "alloc")]
            match node.process_services() {
                Ok(count) => result.services_handled += count,
                Err(_) => result.service_errors += 1,
            }

            // Process timers
            result.timers_fired += node.process_timers(delta_ms);
        }

        result
    }

    /// Blocking spin loop
    ///
    /// Runs until one of:
    /// - `halt()` is called
    /// - Timeout expires (if set in options)
    /// - Max callbacks reached (if set in options)
    /// - `only_next` is true (single iteration)
    ///
    /// # Arguments
    /// * `opts` - Options controlling spin behavior
    ///
    /// # Returns
    /// `Ok(())` on normal completion, `Err` if an error occurred during spinning.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let ctx = Context::default_from_env()?;
    /// let mut executor = ctx.create_basic_executor();
    /// let node = executor.create_node("my_node")?;
    ///
    /// // Spin until halt() is called
    /// executor.spin(SpinOptions::default())?;
    ///
    /// // Spin with timeout
    /// executor.spin(SpinOptions::new().timeout_ms(5000))?;
    /// ```
    pub fn spin(&mut self, opts: SpinOptions) -> Result<(), RclrsError> {
        use std::time::{Duration, Instant};

        const POLL_INTERVAL_MS: u64 = 10;

        let start = Instant::now();
        let timeout = opts.timeout_ms.map(Duration::from_millis);
        let mut total_callbacks = 0usize;

        self.halt_flag
            .store(false, std::sync::atomic::Ordering::SeqCst);

        loop {
            // Check halt flag
            if self.halt_flag.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }

            // Check timeout
            if timeout.is_some_and(|t| start.elapsed() >= t) {
                break;
            }

            // Spin once
            let result = self.spin_once(POLL_INTERVAL_MS);
            total_callbacks += result.total();

            // Check max callbacks
            if opts.max_callbacks.is_some_and(|max| total_callbacks >= max) {
                break;
            }

            // Single iteration mode
            if opts.only_next {
                break;
            }

            // Wait for data or timeout — condvar is signaled by transport
            // callbacks when new subscription/service data arrives, giving
            // near-zero latency dispatch. Falls back to polling interval
            // timeout when idle (same CPU usage as before).
            #[cfg(feature = "rmw-zenoh")]
            nros_rmw_zenoh::wait_for_executor_wake(Duration::from_millis(POLL_INTERVAL_MS));
            #[cfg(not(feature = "rmw-zenoh"))]
            std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
        }

        Ok(())
    }

    /// Execute one period: spin_once + sleep for remainder of period.
    ///
    /// Returns the spin result, whether the period was exceeded (overrun),
    /// and the actual processing time.
    ///
    /// # Arguments
    /// * `period` - Target period duration
    ///
    /// # Example
    ///
    /// ```ignore
    /// // 100Hz control loop with overrun detection
    /// let period = std::time::Duration::from_millis(10);
    /// let result = executor.spin_one_period(period);
    /// if result.overrun {
    ///     log::warn!("Period overrun: {:?}", result.elapsed);
    /// }
    /// ```
    pub fn spin_one_period(&mut self, period: std::time::Duration) -> SpinPeriodResult {
        let start = std::time::Instant::now();
        let period_ms = period.as_millis() as u64;
        let result = self.spin_once(period_ms.max(1));
        let elapsed = start.elapsed();
        let overrun = elapsed > period;
        if !overrun {
            std::thread::sleep(period - elapsed);
        }
        SpinPeriodResult {
            work: result,
            overrun,
            elapsed,
        }
    }

    /// Spin at a fixed rate, compensating for processing time.
    ///
    /// Blocks until `halt()` is called. Uses wall-clock time to maintain the
    /// target rate, compensating for callback processing time. The next
    /// invocation time is accumulated (not reset to now + period) to prevent
    /// cumulative drift, matching rclc's `rclc_executor_spin_period()` pattern.
    ///
    /// # Arguments
    /// * `period` - Target period duration
    ///
    /// # Returns
    /// `Ok(())` when halted.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // 100Hz control loop
    /// executor.spin_period(std::time::Duration::from_millis(10))?;
    /// ```
    pub fn spin_period(&mut self, period: std::time::Duration) -> Result<(), RclrsError> {
        self.halt_flag
            .store(false, std::sync::atomic::Ordering::SeqCst);
        let mut next_invocation = std::time::Instant::now() + period;

        loop {
            if self.halt_flag.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }

            let period_ms = period.as_millis() as u64;
            self.spin_once(period_ms.max(1));

            let now = std::time::Instant::now();
            if now < next_invocation {
                std::thread::sleep(next_invocation - now);
            }
            // Accumulate to prevent drift (not = now + period)
            next_invocation += period;
        }
        Ok(())
    }

    /// Request the executor to stop spinning
    ///
    /// This sets a flag that will cause `spin()` or `spin_period()` to exit
    /// on its next iteration. Safe to call from another thread.
    pub fn halt(&self) {
        self.halt_flag
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Check if halt has been requested
    pub fn is_halted(&self) -> bool {
        self.halt_flag.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Get a clone of the halt flag for use in signal handlers
    pub fn halt_flag(&self) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
        self.halt_flag.clone()
    }
}

#[cfg(all(feature = "rmw-zenoh", feature = "std"))]
impl Executor for BasicExecutor {
    fn spin_once(&mut self, delta_ms: u64) -> SpinOnceResult {
        BasicExecutor::spin_once(self, delta_ms)
    }
}

#[cfg(all(feature = "rmw-zenoh", feature = "std"))]
impl SpinExecutor for BasicExecutor {
    fn spin(&mut self, opts: SpinOptions) -> Result<(), RclrsError> {
        BasicExecutor::spin(self, opts)
    }

    fn spin_period(&mut self, period: std::time::Duration) -> Result<(), RclrsError> {
        BasicExecutor::spin_period(self, period)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spin_once_result() {
        let result = SpinOnceResult::new();
        assert_eq!(result.subscriptions_processed, 0);
        assert_eq!(result.timers_fired, 0);
        assert_eq!(result.services_handled, 0);
        assert_eq!(result.subscription_errors, 0);
        assert_eq!(result.service_errors, 0);
        assert!(!result.any_work());
        assert!(!result.any_errors());
        assert_eq!(result.total(), 0);
        assert_eq!(result.total_errors(), 0);

        let result = SpinOnceResult {
            subscriptions_processed: 2,
            timers_fired: 1,
            services_handled: 0,
            subscription_errors: 0,
            service_errors: 0,
        };
        assert!(result.any_work());
        assert!(!result.any_errors());
        assert_eq!(result.total(), 3);

        // Errors don't count as work
        let result = SpinOnceResult {
            subscriptions_processed: 0,
            timers_fired: 0,
            services_handled: 0,
            subscription_errors: 1,
            service_errors: 2,
        };
        assert!(!result.any_work());
        assert!(result.any_errors());
        assert_eq!(result.total(), 0);
        assert_eq!(result.total_errors(), 3);
    }

    #[test]
    fn test_spin_options() {
        let opts = SpinOptions::new();
        assert!(opts.timeout_ms.is_none());
        assert!(!opts.only_next);
        assert!(opts.max_callbacks.is_none());

        let opts = SpinOptions::spin_once();
        assert!(opts.only_next);

        let opts = SpinOptions::new().timeout_ms(5000).max_callbacks(100);
        assert_eq!(opts.timeout_ms, Some(5000));
        assert_eq!(opts.max_callbacks, Some(100));
    }

    #[test]
    fn test_subscription_handle() {
        let handle = SubscriptionHandle::new(42);
        assert_eq!(handle.index(), 42);
    }

    #[test]
    fn test_spin_period_result() {
        let result = SpinPeriodResult {
            work: SpinOnceResult {
                subscriptions_processed: 1,
                timers_fired: 2,
                services_handled: 0,
                subscription_errors: 0,
                service_errors: 0,
            },
            overrun: false,
            elapsed: std::time::Duration::from_millis(5),
        };
        assert!(result.work.any_work());
        assert!(!result.overrun);
        assert_eq!(result.elapsed.as_millis(), 5);

        let overrun_result = SpinPeriodResult {
            work: SpinOnceResult::new(),
            overrun: true,
            elapsed: std::time::Duration::from_millis(15),
        };
        assert!(overrun_result.overrun);
    }

    #[test]
    fn test_spin_period_polling_result() {
        // No overrun: remaining_ms = period - elapsed
        let result = SpinPeriodPollingResult {
            work: SpinOnceResult::new(),
            remaining_ms: 7,
        };
        assert_eq!(result.remaining_ms, 7);
        assert!(!result.work.any_work());

        // Overrun: remaining_ms saturates to 0
        let period_ms: u64 = 10;
        let elapsed_ms: u64 = 15;
        let remaining = period_ms.saturating_sub(elapsed_ms);
        assert_eq!(remaining, 0);
    }

    #[test]
    fn test_default_max_services() {
        assert_eq!(DEFAULT_MAX_SERVICES, 4);
    }

    #[test]
    fn test_default_max_subscriptions() {
        assert_eq!(DEFAULT_MAX_SUBSCRIPTIONS, 8);
    }

    #[test]
    fn test_default_max_nodes() {
        assert_eq!(DEFAULT_MAX_NODES, 4);
    }

    // ========================================================================
    // Ghost Type Validation (SpinOnceResultGhost)
    // ========================================================================

    /// Structural check: construct SpinOnceResultGhost from SpinOnceResult fields.
    /// If a field is renamed or retyped, this fails to compile.
    #[test]
    fn ghost_spin_once_result_correspondence() {
        use nros_ghost_types::SpinOnceResultGhost;

        let result = SpinOnceResult {
            subscriptions_processed: 3,
            timers_fired: 1,
            services_handled: 2,
            subscription_errors: 1,
            service_errors: 0,
        };

        let ghost = SpinOnceResultGhost {
            subs_processed: result.subscriptions_processed,
            timers_fired: result.timers_fired,
            services_handled: result.services_handled,
            sub_errors: result.subscription_errors,
            svc_errors: result.service_errors,
        };

        assert_eq!(ghost.subs_processed, 3);
        assert_eq!(ghost.timers_fired, 1);
        assert_eq!(ghost.services_handled, 2);
        assert_eq!(ghost.sub_errors, 1);
        assert_eq!(ghost.svc_errors, 0);
    }

    /// Ghost model of new/default state matches production defaults.
    #[test]
    fn ghost_spin_once_result_new_state() {
        use nros_ghost_types::SpinOnceResultGhost;

        let result = SpinOnceResult::new();
        let ghost = SpinOnceResultGhost {
            subs_processed: result.subscriptions_processed,
            timers_fired: result.timers_fired,
            services_handled: result.services_handled,
            sub_errors: result.subscription_errors,
            svc_errors: result.service_errors,
        };

        assert_eq!(ghost.subs_processed, 0);
        assert_eq!(ghost.timers_fired, 0);
        assert_eq!(ghost.services_handled, 0);
        assert_eq!(ghost.sub_errors, 0);
        assert_eq!(ghost.svc_errors, 0);
    }
}

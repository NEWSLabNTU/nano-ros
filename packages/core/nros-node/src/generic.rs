//! Generic embedded node API — backend-agnostic via `Session` trait.
//!
//! Provides [`EmbeddedExecutor<S>`] and [`EmbeddedNode<S>`] that work with any
//! [`Session`] implementation (zenoh, XRCE-DDS, or third-party backends).
//!
//! # Example
//!
//! ```ignore
//! use nros_node::generic::*;
//! use std_msgs::msg::Int32;
//!
//! // Any Session implementation works:
//! let session = MyBackend::open(&config)?;
//! let mut executor = EmbeddedExecutor::from_session(session);
//! let mut node = executor.create_node("my_node")?;
//!
//! let publisher = node.create_publisher::<Int32>("/chatter")?;
//! publisher.publish(&Int32 { data: 42 })?;
//!
//! loop {
//!     executor.drive_io(10)?;
//! }
//! ```

use core::marker::PhantomData;
use core::mem::MaybeUninit;

use nros_core::{CdrReader, CdrWriter, Deserialize, RosAction, RosMessage, RosService, Serialize};
use nros_rmw::{
    ActionInfo, Publisher, QosSettings, ServiceClientTrait, ServiceInfo, ServiceServerTrait,
    Session, SessionMode, Subscriber, TopicInfo, TransportError,
};

use crate::timer::TimerDuration;

// ============================================================================
// SpinOnceResult
// ============================================================================

/// Result of a single spin iteration
///
/// Contains counts of how many items were processed during `spin_once()`,
/// plus error counts for transport failures that would otherwise be silently dropped.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SpinOnceResult {
    /// Number of subscription callbacks invoked
    pub subscriptions_processed: usize,
    /// Number of timers that fired
    pub timers_fired: usize,
    /// Number of service requests handled
    pub services_handled: usize,
    /// Number of subscription processing errors (e.g., BufferTooSmall, MessageTooLarge)
    pub subscription_errors: usize,
    /// Number of service processing errors (e.g., BufferTooSmall)
    pub service_errors: usize,
}

impl SpinOnceResult {
    /// Create a new empty result
    pub const fn new() -> Self {
        Self {
            subscriptions_processed: 0,
            timers_fired: 0,
            services_handled: 0,
            subscription_errors: 0,
            service_errors: 0,
        }
    }

    /// Check if any work was done (errors are not counted as work)
    pub const fn any_work(&self) -> bool {
        self.subscriptions_processed > 0 || self.timers_fired > 0 || self.services_handled > 0
    }

    /// Total number of callbacks successfully invoked (errors excluded)
    pub const fn total(&self) -> usize {
        self.subscriptions_processed + self.timers_fired + self.services_handled
    }

    /// Check if any errors occurred during this spin iteration
    pub const fn any_errors(&self) -> bool {
        self.subscription_errors > 0 || self.service_errors > 0
    }

    /// Total number of errors across all handle types
    pub const fn total_errors(&self) -> usize {
        self.subscription_errors + self.service_errors
    }
}

// ============================================================================
// EmbeddedConfig
// ============================================================================

/// Configuration for opening an embedded executor session.
///
/// Provides a backend-agnostic builder for configuring the middleware
/// connection. The active Cargo feature (`rmw-zenoh`, `rmw-xrce`, or
/// `rmw-cffi`) determines which backend is used.
///
/// # Example
///
/// ```ignore
/// use nros::prelude::*;
///
/// let config = EmbeddedConfig::new("tcp/127.0.0.1:7447")
///     .node_name("talker")
///     .domain_id(0);
/// let mut executor = EmbeddedExecutor::open(&config)?;
/// ```
pub struct EmbeddedConfig<'a> {
    /// Middleware-specific connection string.
    pub locator: &'a str,
    /// Session mode (client or peer).
    pub mode: SessionMode,
    /// ROS 2 domain ID.
    pub domain_id: u32,
    /// Node name.
    pub node_name: &'a str,
    /// Node namespace.
    pub namespace: &'a str,
}

impl<'a> EmbeddedConfig<'a> {
    /// Create a new configuration with the given locator.
    ///
    /// Defaults: `Client` mode, domain 0, node name `"node"`, empty namespace.
    pub const fn new(locator: &'a str) -> Self {
        Self {
            locator,
            mode: SessionMode::Client,
            domain_id: 0,
            node_name: "node",
            namespace: "",
        }
    }

    /// Set the ROS 2 domain ID.
    pub const fn domain_id(mut self, id: u32) -> Self {
        self.domain_id = id;
        self
    }

    /// Set the node name.
    pub const fn node_name(mut self, name: &'a str) -> Self {
        self.node_name = name;
        self
    }

    /// Set the node namespace.
    pub const fn namespace(mut self, ns: &'a str) -> Self {
        self.namespace = ns;
        self
    }

    /// Set the session mode.
    pub const fn mode(mut self, mode: SessionMode) -> Self {
        self.mode = mode;
        self
    }
}

// ============================================================================
// EmbeddedExecutor::open() factory methods
// ============================================================================

#[cfg(any(feature = "rmw-xrce", feature = "rmw-cffi"))]
use nros_rmw::Rmw;

#[cfg(feature = "rmw-zenoh")]
impl EmbeddedExecutor<nros_rmw_zenoh::ShimSession> {
    /// Open a new executor session using the zenoh-pico backend.
    ///
    /// Connects to the zenoh router at the locator specified in `config`.
    /// Returns a plain executor (no callback arena). For an executor with
    /// arena-based callbacks, use [`from_session()`](Self::from_session)
    /// with explicit const generics.
    pub fn open(config: &EmbeddedConfig<'_>) -> Result<Self, EmbeddedNodeError> {
        let tc = nros_rmw::TransportConfig {
            locator: Some(config.locator),
            mode: config.mode,
            properties: &[],
        };
        let session = nros_rmw_zenoh::ShimSession::new(&tc)
            .map_err(|_| EmbeddedNodeError::Transport(TransportError::ConnectionFailed))?;
        Ok(Self::from_session(session))
    }
}

#[cfg(feature = "rmw-xrce")]
impl EmbeddedExecutor<nros_rmw_xrce::XrceSession> {
    /// Open a new executor session using the XRCE-DDS backend.
    ///
    /// Automatically initializes the active transport (POSIX UDP, POSIX serial,
    /// or Zephyr BSD socket) before connecting to the XRCE agent.
    pub fn open(config: &EmbeddedConfig<'_>) -> Result<Self, EmbeddedNodeError> {
        // Auto-init transport based on active feature
        #[cfg(feature = "posix-udp")]
        unsafe {
            nros_rmw_xrce::posix_udp::init_posix_udp_transport(config.locator);
        }
        #[cfg(feature = "posix-serial")]
        unsafe {
            nros_rmw_xrce::posix_serial::init_posix_serial_transport(config.locator);
        }
        #[cfg(feature = "platform-zephyr")]
        unsafe {
            nros_rmw_xrce::zephyr::init_zephyr_transport();
        }

        let rmw_config = nros_rmw::RmwConfig {
            locator: config.locator,
            mode: config.mode,
            domain_id: config.domain_id,
            node_name: config.node_name,
            namespace: config.namespace,
        };
        let session = nros_rmw_xrce::XrceRmw::open(&rmw_config)
            .map_err(|_| EmbeddedNodeError::Transport(TransportError::ConnectionFailed))?;
        Ok(Self::from_session(session))
    }
}

#[cfg(feature = "rmw-cffi")]
impl EmbeddedExecutor<nros_rmw_cffi::CffiSession> {
    /// Open a new executor session using the C FFI backend.
    pub fn open(config: &EmbeddedConfig<'_>) -> Result<Self, EmbeddedNodeError> {
        let rmw_config = nros_rmw::RmwConfig {
            locator: config.locator,
            mode: config.mode,
            domain_id: config.domain_id,
            node_name: config.node_name,
            namespace: config.namespace,
        };
        let session = nros_rmw_cffi::CffiRmw::open(&rmw_config)
            .map_err(|_| EmbeddedNodeError::Transport(TransportError::ConnectionFailed))?;
        Ok(Self::from_session(session))
    }
}

/// Default transmit buffer size (bytes).
const DEFAULT_TX_BUF: usize = 1024;

// ============================================================================
// Error type
// ============================================================================

/// Error type for generic embedded node operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddedNodeError {
    /// Transport-level error.
    Transport(TransportError),
    /// Node name exceeds 64 bytes.
    NameTooLong,
    /// CDR serialization failed.
    Serialization,
    /// Buffer too small for message.
    BufferTooSmall,
    /// Action server/client creation failed.
    ActionCreationFailed,
    /// Service request failed.
    ServiceRequestFailed,
    /// Service reply failed.
    ServiceReplyFailed,
}

impl From<TransportError> for EmbeddedNodeError {
    fn from(err: TransportError) -> Self {
        EmbeddedNodeError::Transport(err)
    }
}

// ============================================================================
// Callback Arena Infrastructure
// ============================================================================

/// Kind of registered callback entry.
#[derive(Clone, Copy)]
enum EntryKind {
    Subscription,
    Service,
    Timer,
    ActionServer,
    ActionClient,
}

/// Metadata for a type-erased callback stored in the arena.
///
/// Each entry records where the concrete entry struct lives in the arena
/// and carries monomorphized function pointers for dispatch and cleanup.
#[derive(Clone, Copy)]
struct CallbackMeta {
    /// Byte offset into the arena where the concrete entry starts.
    offset: usize,
    /// What kind of entry this is (for `SpinOnceResult` counters).
    kind: EntryKind,
    /// Monomorphized dispatch: tries to receive and process one message/request.
    /// Returns `Ok(true)` if work was done, `Ok(false)` if nothing available.
    /// The `u64` parameter is `delta_ms` (used by timer entries, ignored by others).
    try_process: unsafe fn(*mut u8, u64) -> Result<bool, TransportError>,
    /// Monomorphized drop: runs destructors on the concrete entry.
    drop_fn: unsafe fn(*mut u8),
}

/// Concrete subscription entry stored in the arena.
#[repr(C)]
struct SubEntry<M, Sub, F, const RX_BUF: usize> {
    handle: Sub,
    buffer: [u8; RX_BUF],
    callback: F,
    _phantom: PhantomData<M>,
}

/// Concrete service entry stored in the arena.
#[repr(C)]
struct SrvEntry<Svc: RosService, Srv, F, const REQ_BUF: usize, const REPLY_BUF: usize> {
    handle: Srv,
    req_buffer: [u8; REQ_BUF],
    reply_buffer: [u8; REPLY_BUF],
    callback: F,
    _phantom: PhantomData<Svc>,
}

/// Monomorphized subscription dispatch function.
///
/// # Safety
/// `ptr` must point to a valid, aligned `SubEntry<M, Sub, F, RX_BUF>`.
unsafe fn sub_try_process<M, Sub, F, const RX_BUF: usize>(
    ptr: *mut u8,
    _delta_ms: u64,
) -> Result<bool, TransportError>
where
    M: RosMessage,
    Sub: Subscriber,
    F: FnMut(&M),
{
    let entry = unsafe { &mut *(ptr as *mut SubEntry<M, Sub, F, RX_BUF>) };
    match entry.handle.try_recv_raw(&mut entry.buffer) {
        Ok(Some(len)) => {
            let mut reader = CdrReader::new_with_header(&entry.buffer[..len])
                .map_err(|_| TransportError::DeserializationError)?;
            let msg =
                M::deserialize(&mut reader).map_err(|_| TransportError::DeserializationError)?;
            (entry.callback)(&msg);
            Ok(true)
        }
        Ok(None) => Ok(false),
        Err(_) => Err(TransportError::DeserializationError),
    }
}

/// Monomorphized service dispatch function.
///
/// # Safety
/// `ptr` must point to a valid, aligned `SrvEntry<Svc, Srv, F, REQ_BUF, REPLY_BUF>`.
unsafe fn srv_try_process<Svc, Srv, F, const REQ_BUF: usize, const REPLY_BUF: usize>(
    ptr: *mut u8,
    _delta_ms: u64,
) -> Result<bool, TransportError>
where
    Svc: RosService,
    Srv: ServiceServerTrait,
    F: FnMut(&Svc::Request) -> Svc::Reply,
    Srv::Error: From<TransportError>,
{
    let entry = unsafe { &mut *(ptr as *mut SrvEntry<Svc, Srv, F, REQ_BUF, REPLY_BUF>) };
    // Split borrow: destructure entry to avoid aliasing issues
    let SrvEntry {
        handle,
        req_buffer,
        reply_buffer,
        callback,
        ..
    } = entry;
    handle
        .handle_request::<Svc>(req_buffer, reply_buffer, |req| (callback)(req))
        .map_err(|_| TransportError::ServiceReplyFailed)
}

/// Monomorphized drop function for arena entries.
///
/// # Safety
/// `ptr` must point to a valid, aligned `T` that has not been dropped.
unsafe fn drop_entry<T>(ptr: *mut u8) {
    unsafe { core::ptr::drop_in_place(ptr as *mut T) };
}

/// Concrete timer entry stored in the arena.
#[repr(C)]
struct TimerEntry<F> {
    period_ms: u64,
    elapsed_ms: u64,
    oneshot: bool,
    fired: bool,
    callback: F,
}

/// Monomorphized timer dispatch function.
///
/// # Safety
/// `ptr` must point to a valid, aligned `TimerEntry<F>`.
unsafe fn timer_try_process<F>(ptr: *mut u8, delta_ms: u64) -> Result<bool, TransportError>
where
    F: FnMut(),
{
    let entry = unsafe { &mut *(ptr as *mut TimerEntry<F>) };

    // One-shot already fired
    if entry.oneshot && entry.fired {
        return Ok(false);
    }

    entry.elapsed_ms = entry.elapsed_ms.saturating_add(delta_ms);

    if entry.elapsed_ms >= entry.period_ms {
        (entry.callback)();
        if entry.oneshot {
            entry.fired = true;
        } else {
            entry.elapsed_ms = entry.elapsed_ms.saturating_sub(entry.period_ms);
        }
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Concrete action server entry stored in the arena.
#[repr(C)]
struct ActionServerArenaEntry<
    A: RosAction,
    Srv,
    Pub,
    GoalF,
    CancelF,
    const GOAL_BUF: usize,
    const RESULT_BUF: usize,
    const FEEDBACK_BUF: usize,
    const MAX_GOALS: usize,
> {
    server: EmbeddedActionServer<A, Srv, Pub, GOAL_BUF, RESULT_BUF, FEEDBACK_BUF, MAX_GOALS>,
    goal_callback: GoalF,
    cancel_callback: CancelF,
}

/// Monomorphized action server dispatch function.
///
/// Polls goal acceptance, cancel handling, and result serving.
///
/// # Safety
/// `ptr` must point to a valid, aligned `ActionServerArenaEntry<...>`.
unsafe fn action_server_try_process<
    A,
    Srv,
    Pub,
    GoalF,
    CancelF,
    const GOAL_BUF: usize,
    const RESULT_BUF: usize,
    const FEEDBACK_BUF: usize,
    const MAX_GOALS: usize,
>(
    ptr: *mut u8,
    _delta_ms: u64,
) -> Result<bool, TransportError>
where
    A: RosAction,
    A::Goal: Clone,
    A::Result: Clone + Default,
    Srv: ServiceServerTrait,
    Pub: Publisher,
    GoalF: FnMut(&A::Goal) -> nros_core::GoalResponse,
    CancelF: FnMut(&nros_core::GoalId, nros_core::GoalStatus) -> nros_core::CancelResponse,
{
    let entry = unsafe {
        &mut *(ptr as *mut ActionServerArenaEntry<
            A,
            Srv,
            Pub,
            GoalF,
            CancelF,
            GOAL_BUF,
            RESULT_BUF,
            FEEDBACK_BUF,
            MAX_GOALS,
        >)
    };
    let ActionServerArenaEntry {
        server,
        goal_callback,
        cancel_callback,
    } = entry;

    let mut did_work = false;

    // Handle cancels first
    if matches!(
        server.try_handle_cancel(|id, st| (cancel_callback)(id, st)),
        Ok(Some(_))
    ) {
        did_work = true;
    }

    // Handle new goals
    if matches!(server.try_accept_goal(|g| (goal_callback)(g)), Ok(Some(_))) {
        did_work = true;
    }

    // Handle result requests
    if matches!(server.try_handle_get_result(), Ok(Some(_))) {
        did_work = true;
    }

    Ok(did_work)
}

/// Concrete action client entry stored in the arena.
#[repr(C)]
struct ActionClientArenaEntry<
    A: RosAction,
    Cli,
    Sub,
    FeedbackF,
    const GOAL_BUF: usize,
    const RESULT_BUF: usize,
    const FEEDBACK_BUF: usize,
> {
    client: EmbeddedActionClient<A, Cli, Sub, GOAL_BUF, RESULT_BUF, FEEDBACK_BUF>,
    feedback_callback: FeedbackF,
}

/// Monomorphized action client dispatch function.
///
/// Polls feedback from the action server.
///
/// # Safety
/// `ptr` must point to a valid, aligned `ActionClientArenaEntry<...>`.
unsafe fn action_client_try_process<
    A,
    Cli,
    Sub,
    FeedbackF,
    const GOAL_BUF: usize,
    const RESULT_BUF: usize,
    const FEEDBACK_BUF: usize,
>(
    ptr: *mut u8,
    _delta_ms: u64,
) -> Result<bool, TransportError>
where
    A: RosAction,
    Cli: ServiceClientTrait,
    Sub: Subscriber,
    FeedbackF: FnMut(&nros_core::GoalId, &A::Feedback),
{
    let entry = unsafe {
        &mut *(ptr as *mut ActionClientArenaEntry<
            A,
            Cli,
            Sub,
            FeedbackF,
            GOAL_BUF,
            RESULT_BUF,
            FEEDBACK_BUF,
        >)
    };
    let ActionClientArenaEntry {
        client,
        feedback_callback,
    } = entry;

    match client.try_recv_feedback() {
        Ok(Some((goal_id, feedback))) => {
            (feedback_callback)(&goal_id, &feedback);
            Ok(true)
        }
        Ok(None) => Ok(false),
        Err(_) => Err(TransportError::DeserializationError),
    }
}

// ============================================================================
// Monomorphized handle operation functions
// ============================================================================

/// Action server: publish feedback via arena entry.
///
/// # Safety
/// `ptr` must point to a valid `ActionServerArenaEntry`.
unsafe fn as_publish_feedback<
    A,
    Srv,
    Pub,
    GoalF,
    CancelF,
    const GB: usize,
    const RB: usize,
    const FB: usize,
    const MG: usize,
>(
    ptr: *mut u8,
    goal_id: &nros_core::GoalId,
    feedback: &A::Feedback,
) -> Result<(), EmbeddedNodeError>
where
    A: RosAction,
    Srv: ServiceServerTrait,
    Pub: Publisher,
{
    let entry = unsafe {
        &mut *(ptr as *mut ActionServerArenaEntry<A, Srv, Pub, GoalF, CancelF, GB, RB, FB, MG>)
    };
    entry.server.publish_feedback(goal_id, feedback)
}

/// Action server: complete a goal via arena entry.
///
/// # Safety
/// `ptr` must point to a valid `ActionServerArenaEntry`.
unsafe fn as_complete_goal<
    A,
    Srv,
    Pub,
    GoalF,
    CancelF,
    const GB: usize,
    const RB: usize,
    const FB: usize,
    const MG: usize,
>(
    ptr: *mut u8,
    goal_id: &nros_core::GoalId,
    status: nros_core::GoalStatus,
    result: A::Result,
) where
    A: RosAction,
    Srv: ServiceServerTrait,
    Pub: Publisher,
{
    let entry = unsafe {
        &mut *(ptr as *mut ActionServerArenaEntry<A, Srv, Pub, GoalF, CancelF, GB, RB, FB, MG>)
    };
    entry.server.complete_goal(goal_id, status, result);
}

/// Action server: set goal status via arena entry.
///
/// # Safety
/// `ptr` must point to a valid `ActionServerArenaEntry`.
unsafe fn as_set_goal_status<
    A,
    Srv,
    Pub,
    GoalF,
    CancelF,
    const GB: usize,
    const RB: usize,
    const FB: usize,
    const MG: usize,
>(
    ptr: *mut u8,
    goal_id: &nros_core::GoalId,
    status: nros_core::GoalStatus,
) where
    A: RosAction,
    Srv: ServiceServerTrait,
    Pub: Publisher,
{
    let entry = unsafe {
        &mut *(ptr as *mut ActionServerArenaEntry<A, Srv, Pub, GoalF, CancelF, GB, RB, FB, MG>)
    };
    entry.server.set_goal_status(goal_id, status);
}

/// Action client: send goal via arena entry.
///
/// # Safety
/// `ptr` must point to a valid `ActionClientArenaEntry`.
unsafe fn ac_send_goal<A, Cli, Sub, FeedbackF, const GB: usize, const RB: usize, const FB: usize>(
    ptr: *mut u8,
    goal: &A::Goal,
) -> Result<nros_core::GoalId, EmbeddedNodeError>
where
    A: RosAction,
    Cli: ServiceClientTrait,
    Sub: Subscriber,
{
    let entry =
        unsafe { &mut *(ptr as *mut ActionClientArenaEntry<A, Cli, Sub, FeedbackF, GB, RB, FB>) };
    entry.client.send_goal(goal)
}

/// Action client: cancel goal via arena entry.
///
/// # Safety
/// `ptr` must point to a valid `ActionClientArenaEntry`.
unsafe fn ac_cancel_goal<
    A,
    Cli,
    Sub,
    FeedbackF,
    const GB: usize,
    const RB: usize,
    const FB: usize,
>(
    ptr: *mut u8,
    goal_id: &nros_core::GoalId,
) -> Result<nros_core::CancelResponse, EmbeddedNodeError>
where
    A: RosAction,
    Cli: ServiceClientTrait,
    Sub: Subscriber,
{
    let entry =
        unsafe { &mut *(ptr as *mut ActionClientArenaEntry<A, Cli, Sub, FeedbackF, GB, RB, FB>) };
    entry.client.cancel_goal(goal_id)
}

/// Action client: get result via arena entry.
///
/// # Safety
/// `ptr` must point to a valid `ActionClientArenaEntry`.
unsafe fn ac_get_result<A, Cli, Sub, FeedbackF, const GB: usize, const RB: usize, const FB: usize>(
    ptr: *mut u8,
    goal_id: &nros_core::GoalId,
) -> Result<(nros_core::GoalStatus, A::Result), EmbeddedNodeError>
where
    A: RosAction,
    Cli: ServiceClientTrait,
    Sub: Subscriber,
{
    let entry =
        unsafe { &mut *(ptr as *mut ActionClientArenaEntry<A, Cli, Sub, FeedbackF, GB, RB, FB>) };
    entry.client.get_result(goal_id)
}

// ============================================================================
// EmbeddedExecutor<S>
// ============================================================================

/// Backend-agnostic executor that owns a [`Session`].
///
/// Provides `create_node()` for entity creation and `drive_io()` for polling.
///
/// # Callback Mode
///
/// When `MAX_CBS > 0` and `CB_ARENA > 0`, the executor supports arena-based
/// callback registration via [`add_subscription()`](Self::add_subscription)
/// and [`add_service()`](Self::add_service), with dispatch via
/// [`spin_once()`](Self::spin_once). No heap allocation is needed.
///
/// When using the defaults (`MAX_CBS = 0`, `CB_ARENA = 0`), both arrays are
/// zero-sized — zero overhead for existing manual-polling code.
pub struct EmbeddedExecutor<S, const MAX_CBS: usize = 0, const CB_ARENA: usize = 0> {
    session: S,
    arena: [MaybeUninit<u8>; CB_ARENA],
    arena_used: usize,
    entries: [Option<CallbackMeta>; MAX_CBS],
}

impl<S: Session, const MAX_CBS: usize, const CB_ARENA: usize>
    EmbeddedExecutor<S, MAX_CBS, CB_ARENA>
{
    /// Create an executor from an already-opened session.
    pub fn from_session(session: S) -> Self {
        // SAFETY: MaybeUninit::uninit() is always safe; these bytes are only
        // accessed through properly-typed ptr::write / ptr::read via the
        // dispatch function pointers stored in `entries`.
        Self {
            session,
            arena: [MaybeUninit::uninit(); CB_ARENA],
            arena_used: 0,
            entries: [None; MAX_CBS],
        }
    }

    /// Create a node on this executor.
    pub fn create_node(&mut self, name: &str) -> Result<EmbeddedNode<'_, S>, EmbeddedNodeError> {
        if name.len() > 64 {
            return Err(EmbeddedNodeError::NameTooLong);
        }

        let mut node_name = heapless::String::<64>::new();
        node_name
            .push_str(name)
            .map_err(|_| EmbeddedNodeError::NameTooLong)?;

        Ok(EmbeddedNode {
            name: node_name,
            session: &mut self.session,
            domain_id: 0,
        })
    }

    /// Drive transport I/O (poll network, dispatch callbacks).
    pub fn drive_io(&mut self, timeout_ms: i32) -> Result<(), EmbeddedNodeError> {
        self.session
            .drive_io(timeout_ms)
            .map_err(|_| EmbeddedNodeError::Transport(TransportError::PollFailed))
    }

    /// Close the underlying session.
    pub fn close(&mut self) -> Result<(), EmbeddedNodeError> {
        self.session
            .close()
            .map_err(|_| EmbeddedNodeError::Transport(TransportError::ConnectionFailed))
    }

    /// Get a reference to the underlying session.
    pub fn session(&self) -> &S {
        &self.session
    }

    /// Get a mutable reference to the underlying session.
    pub fn session_mut(&mut self) -> &mut S {
        &mut self.session
    }

    // ========================================================================
    // Arena-based callback registration
    // ========================================================================

    /// Bump-allocate space for `T` in the arena. Returns the byte offset.
    fn arena_alloc<T>(&mut self) -> Result<usize, EmbeddedNodeError> {
        let align = core::mem::align_of::<T>();
        let size = core::mem::size_of::<T>();
        let aligned_offset = (self.arena_used + align - 1) & !(align - 1);
        let new_used = aligned_offset + size;
        if new_used > CB_ARENA {
            return Err(EmbeddedNodeError::BufferTooSmall);
        }
        self.arena_used = new_used;
        Ok(aligned_offset)
    }

    /// Find the next free entry slot index.
    fn next_entry_slot(&self) -> Result<usize, EmbeddedNodeError> {
        self.entries
            .iter()
            .position(|e| e.is_none())
            .ok_or(EmbeddedNodeError::BufferTooSmall)
    }

    /// Register a subscription callback with the default 1024-byte receive buffer.
    ///
    /// The callback is stored in the arena and invoked during [`spin_once()`](Self::spin_once).
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut executor: EmbeddedExecutor<_, 4, 4096> = EmbeddedExecutor::open(&config)?;
    /// executor.add_subscription::<Int32, _>("/chatter", |msg: &Int32| {
    ///     // handle message
    /// })?;
    /// loop {
    ///     executor.spin_once(10);
    /// }
    /// ```
    pub fn add_subscription<M, F>(
        &mut self,
        topic_name: &str,
        callback: F,
    ) -> Result<(), EmbeddedNodeError>
    where
        M: RosMessage + 'static,
        F: FnMut(&M) + 'static,
        S::SubscriberHandle: Subscriber,
    {
        self.add_subscription_sized::<M, F, 1024>(topic_name, callback)
    }

    /// Register a subscription callback with a custom receive buffer size.
    pub fn add_subscription_sized<M, F, const RX_BUF: usize>(
        &mut self,
        topic_name: &str,
        callback: F,
    ) -> Result<(), EmbeddedNodeError>
    where
        M: RosMessage + 'static,
        F: FnMut(&M) + 'static,
        S::SubscriberHandle: Subscriber,
    {
        type Entry<M, Sub, F, const N: usize> = SubEntry<M, Sub, F, N>;

        let slot = self.next_entry_slot()?;
        let topic = TopicInfo::new(topic_name, M::TYPE_NAME, M::TYPE_HASH);
        let handle = self
            .session
            .create_subscriber(&topic, QosSettings::default())
            .map_err(|_| EmbeddedNodeError::Transport(TransportError::SubscriberCreationFailed))?;

        let offset = self.arena_alloc::<Entry<M, S::SubscriberHandle, F, RX_BUF>>()?;

        // SAFETY: `arena_alloc` guarantees the offset is within bounds and
        // properly aligned for `Entry`. We write a fully-initialized value.
        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(offset) as *mut Entry<M, S::SubscriberHandle, F, RX_BUF>;
            core::ptr::write(
                entry_ptr,
                Entry {
                    handle,
                    buffer: [0u8; RX_BUF],
                    callback,
                    _phantom: PhantomData,
                },
            );
        }

        self.entries[slot] = Some(CallbackMeta {
            offset,
            kind: EntryKind::Subscription,
            try_process: sub_try_process::<M, S::SubscriberHandle, F, RX_BUF>,
            drop_fn: drop_entry::<Entry<M, S::SubscriberHandle, F, RX_BUF>>,
        });
        Ok(())
    }

    /// Register a service callback with the default 1024-byte buffers.
    ///
    /// The callback is stored in the arena and invoked during [`spin_once()`](Self::spin_once).
    pub fn add_service<Svc, F>(
        &mut self,
        service_name: &str,
        callback: F,
    ) -> Result<(), EmbeddedNodeError>
    where
        Svc: RosService + 'static,
        F: FnMut(&Svc::Request) -> Svc::Reply + 'static,
        S::ServiceServerHandle: ServiceServerTrait,
        <S::ServiceServerHandle as ServiceServerTrait>::Error: From<TransportError>,
    {
        self.add_service_sized::<Svc, F, 1024, 1024>(service_name, callback)
    }

    /// Register a service callback with custom request/reply buffer sizes.
    pub fn add_service_sized<Svc, F, const REQ_BUF: usize, const REPLY_BUF: usize>(
        &mut self,
        service_name: &str,
        callback: F,
    ) -> Result<(), EmbeddedNodeError>
    where
        Svc: RosService + 'static,
        F: FnMut(&Svc::Request) -> Svc::Reply + 'static,
        S::ServiceServerHandle: ServiceServerTrait,
        <S::ServiceServerHandle as ServiceServerTrait>::Error: From<TransportError>,
    {
        type Entry<Svc, Srv, F, const RQ: usize, const RP: usize> = SrvEntry<Svc, Srv, F, RQ, RP>;

        let slot = self.next_entry_slot()?;
        let info = ServiceInfo::new(service_name, Svc::SERVICE_NAME, Svc::SERVICE_HASH);
        let handle = self.session.create_service_server(&info).map_err(|_| {
            EmbeddedNodeError::Transport(TransportError::ServiceServerCreationFailed)
        })?;

        let offset =
            self.arena_alloc::<Entry<Svc, S::ServiceServerHandle, F, REQ_BUF, REPLY_BUF>>()?;

        // SAFETY: same guarantees as add_subscription_sized.
        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(offset)
                as *mut Entry<Svc, S::ServiceServerHandle, F, REQ_BUF, REPLY_BUF>;
            core::ptr::write(
                entry_ptr,
                Entry {
                    handle,
                    req_buffer: [0u8; REQ_BUF],
                    reply_buffer: [0u8; REPLY_BUF],
                    callback,
                    _phantom: PhantomData,
                },
            );
        }

        self.entries[slot] = Some(CallbackMeta {
            offset,
            kind: EntryKind::Service,
            try_process: srv_try_process::<Svc, S::ServiceServerHandle, F, REQ_BUF, REPLY_BUF>,
            drop_fn: drop_entry::<Entry<Svc, S::ServiceServerHandle, F, REQ_BUF, REPLY_BUF>>,
        });
        Ok(())
    }

    // ========================================================================
    // Timer registration
    // ========================================================================

    /// Register a repeating timer callback.
    ///
    /// The callback fires every `period` milliseconds during [`spin_once()`](Self::spin_once).
    /// The timer delta is approximated by the `timeout_ms` argument to `spin_once`.
    pub fn add_timer<F>(
        &mut self,
        period: TimerDuration,
        callback: F,
    ) -> Result<(), EmbeddedNodeError>
    where
        F: FnMut() + 'static,
    {
        let slot = self.next_entry_slot()?;
        let offset = self.arena_alloc::<TimerEntry<F>>()?;

        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(offset) as *mut TimerEntry<F>;
            core::ptr::write(
                entry_ptr,
                TimerEntry {
                    period_ms: period.as_millis(),
                    elapsed_ms: 0,
                    oneshot: false,
                    fired: false,
                    callback,
                },
            );
        }

        self.entries[slot] = Some(CallbackMeta {
            offset,
            kind: EntryKind::Timer,
            try_process: timer_try_process::<F>,
            drop_fn: drop_entry::<TimerEntry<F>>,
        });
        Ok(())
    }

    /// Register a one-shot timer callback.
    ///
    /// The callback fires once after `delay` milliseconds, then becomes inert.
    pub fn add_timer_oneshot<F>(
        &mut self,
        delay: TimerDuration,
        callback: F,
    ) -> Result<(), EmbeddedNodeError>
    where
        F: FnMut() + 'static,
    {
        let slot = self.next_entry_slot()?;
        let offset = self.arena_alloc::<TimerEntry<F>>()?;

        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(offset) as *mut TimerEntry<F>;
            core::ptr::write(
                entry_ptr,
                TimerEntry {
                    period_ms: delay.as_millis(),
                    elapsed_ms: 0,
                    oneshot: true,
                    fired: false,
                    callback,
                },
            );
        }

        self.entries[slot] = Some(CallbackMeta {
            offset,
            kind: EntryKind::Timer,
            try_process: timer_try_process::<F>,
            drop_fn: drop_entry::<TimerEntry<F>>,
        });
        Ok(())
    }

    // ========================================================================
    // Action server registration
    // ========================================================================

    /// Register an action server with goal/cancel callbacks.
    ///
    /// The executor automatically dispatches:
    /// - Goal acceptance via `goal_callback`
    /// - Cancel requests via `cancel_callback`
    /// - Result serving for completed goals
    ///
    /// Use the returned [`ActionServerHandle`] to publish feedback and complete goals.
    ///
    /// Uses default buffer sizes (1024 bytes) and max 4 concurrent goals.
    pub fn add_action_server<A, GoalF, CancelF>(
        &mut self,
        action_name: &str,
        goal_callback: GoalF,
        cancel_callback: CancelF,
    ) -> Result<ActionServerHandle<A>, EmbeddedNodeError>
    where
        A: RosAction + 'static,
        A::Goal: Clone,
        A::Result: Clone + Default,
        GoalF: FnMut(&A::Goal) -> nros_core::GoalResponse + 'static,
        CancelF:
            FnMut(&nros_core::GoalId, nros_core::GoalStatus) -> nros_core::CancelResponse + 'static,
        S::ServiceServerHandle: ServiceServerTrait,
        S::PublisherHandle: Publisher,
    {
        self.add_action_server_sized::<A, GoalF, CancelF, 1024, 1024, 1024, 4>(
            action_name,
            goal_callback,
            cancel_callback,
        )
    }

    /// Register an action server with custom buffer sizes.
    pub fn add_action_server_sized<
        A,
        GoalF,
        CancelF,
        const GOAL_BUF: usize,
        const RESULT_BUF: usize,
        const FEEDBACK_BUF: usize,
        const MAX_GOALS: usize,
    >(
        &mut self,
        action_name: &str,
        goal_callback: GoalF,
        cancel_callback: CancelF,
    ) -> Result<ActionServerHandle<A>, EmbeddedNodeError>
    where
        A: RosAction + 'static,
        A::Goal: Clone,
        A::Result: Clone + Default,
        GoalF: FnMut(&A::Goal) -> nros_core::GoalResponse + 'static,
        CancelF:
            FnMut(&nros_core::GoalId, nros_core::GoalStatus) -> nros_core::CancelResponse + 'static,
        S::ServiceServerHandle: ServiceServerTrait,
        S::PublisherHandle: Publisher,
    {
        type Entry<
            A,
            Srv,
            Pub,
            GoalF,
            CancelF,
            const GB: usize,
            const RB: usize,
            const FB: usize,
            const MG: usize,
        > = ActionServerArenaEntry<A, Srv, Pub, GoalF, CancelF, GB, RB, FB, MG>;

        let slot = self.next_entry_slot()?;

        // Create the action server entities (same logic as EmbeddedNode::create_action_server_sized)
        let action_info = ActionInfo::new(action_name, A::ACTION_NAME, A::ACTION_HASH);

        let send_goal_keyexpr: heapless::String<256> = action_info.send_goal_key();
        let send_goal_info =
            ServiceInfo::new(&send_goal_keyexpr, A::ACTION_NAME, A::ACTION_HASH).with_domain(0);
        let send_goal_server = self
            .session
            .create_service_server(&send_goal_info)
            .map_err(|_| EmbeddedNodeError::ActionCreationFailed)?;

        let cancel_goal_keyexpr: heapless::String<256> = action_info.cancel_goal_key();
        let cancel_goal_info = ServiceInfo::new(
            &cancel_goal_keyexpr,
            "action_msgs::srv::dds_::CancelGoal_",
            A::ACTION_HASH,
        )
        .with_domain(0);
        let cancel_goal_server = self
            .session
            .create_service_server(&cancel_goal_info)
            .map_err(|_| EmbeddedNodeError::ActionCreationFailed)?;

        let get_result_keyexpr: heapless::String<256> = action_info.get_result_key();
        let get_result_info =
            ServiceInfo::new(&get_result_keyexpr, A::ACTION_NAME, A::ACTION_HASH).with_domain(0);
        let get_result_server = self
            .session
            .create_service_server(&get_result_info)
            .map_err(|_| EmbeddedNodeError::ActionCreationFailed)?;

        let feedback_keyexpr: heapless::String<256> = action_info.feedback_key();
        let feedback_topic =
            TopicInfo::new(&feedback_keyexpr, A::ACTION_NAME, A::ACTION_HASH).with_domain(0);
        let feedback_publisher = self
            .session
            .create_publisher(&feedback_topic, QosSettings::BEST_EFFORT)
            .map_err(|_| EmbeddedNodeError::ActionCreationFailed)?;

        let status_keyexpr: heapless::String<256> = action_info.status_key();
        let status_topic = TopicInfo::new(
            &status_keyexpr,
            "action_msgs::msg::dds_::GoalStatusArray_",
            A::ACTION_HASH,
        )
        .with_domain(0);
        let status_publisher = self
            .session
            .create_publisher(&status_topic, QosSettings::BEST_EFFORT)
            .map_err(|_| EmbeddedNodeError::ActionCreationFailed)?;

        let server = EmbeddedActionServer {
            send_goal_server,
            cancel_goal_server,
            get_result_server,
            feedback_publisher,
            _status_publisher: status_publisher,
            active_goals: heapless::Vec::new(),
            completed_goals: heapless::Vec::new(),
            goal_buffer: [0u8; GOAL_BUF],
            result_buffer: [0u8; RESULT_BUF],
            feedback_buffer: [0u8; FEEDBACK_BUF],
            cancel_buffer: [0u8; 256],
        };

        let offset = self.arena_alloc::<Entry<
            A,
            S::ServiceServerHandle,
            S::PublisherHandle,
            GoalF,
            CancelF,
            GOAL_BUF,
            RESULT_BUF,
            FEEDBACK_BUF,
            MAX_GOALS,
        >>()?;

        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(offset)
                as *mut Entry<
                    A,
                    S::ServiceServerHandle,
                    S::PublisherHandle,
                    GoalF,
                    CancelF,
                    GOAL_BUF,
                    RESULT_BUF,
                    FEEDBACK_BUF,
                    MAX_GOALS,
                >;
            core::ptr::write(
                entry_ptr,
                Entry {
                    server,
                    goal_callback,
                    cancel_callback,
                },
            );
        }

        self.entries[slot] = Some(CallbackMeta {
            offset,
            kind: EntryKind::ActionServer,
            try_process: action_server_try_process::<
                A,
                S::ServiceServerHandle,
                S::PublisherHandle,
                GoalF,
                CancelF,
                GOAL_BUF,
                RESULT_BUF,
                FEEDBACK_BUF,
                MAX_GOALS,
            >,
            drop_fn: drop_entry::<
                Entry<
                    A,
                    S::ServiceServerHandle,
                    S::PublisherHandle,
                    GoalF,
                    CancelF,
                    GOAL_BUF,
                    RESULT_BUF,
                    FEEDBACK_BUF,
                    MAX_GOALS,
                >,
            >,
        });

        Ok(ActionServerHandle {
            entry_index: slot,
            publish_feedback_fn: as_publish_feedback::<
                A,
                S::ServiceServerHandle,
                S::PublisherHandle,
                GoalF,
                CancelF,
                GOAL_BUF,
                RESULT_BUF,
                FEEDBACK_BUF,
                MAX_GOALS,
            >,
            complete_goal_fn: as_complete_goal::<
                A,
                S::ServiceServerHandle,
                S::PublisherHandle,
                GoalF,
                CancelF,
                GOAL_BUF,
                RESULT_BUF,
                FEEDBACK_BUF,
                MAX_GOALS,
            >,
            set_goal_status_fn: as_set_goal_status::<
                A,
                S::ServiceServerHandle,
                S::PublisherHandle,
                GoalF,
                CancelF,
                GOAL_BUF,
                RESULT_BUF,
                FEEDBACK_BUF,
                MAX_GOALS,
            >,
            _phantom: PhantomData,
        })
    }

    // ========================================================================
    // Action client registration
    // ========================================================================

    /// Register an action client with a feedback callback.
    ///
    /// The executor automatically dispatches feedback to `feedback_callback`
    /// during [`spin_once()`](Self::spin_once).
    ///
    /// Use the returned [`ActionClientHandle`] to send goals and get results.
    ///
    /// Uses default buffer sizes (1024 bytes).
    pub fn add_action_client<A, FeedbackF>(
        &mut self,
        action_name: &str,
        feedback_callback: FeedbackF,
    ) -> Result<ActionClientHandle<A>, EmbeddedNodeError>
    where
        A: RosAction + 'static,
        FeedbackF: FnMut(&nros_core::GoalId, &A::Feedback) + 'static,
        S::ServiceClientHandle: ServiceClientTrait,
        S::SubscriberHandle: Subscriber,
    {
        self.add_action_client_sized::<A, FeedbackF, 1024, 1024, 1024>(
            action_name,
            feedback_callback,
        )
    }

    /// Register an action client with custom buffer sizes.
    pub fn add_action_client_sized<
        A,
        FeedbackF,
        const GOAL_BUF: usize,
        const RESULT_BUF: usize,
        const FEEDBACK_BUF: usize,
    >(
        &mut self,
        action_name: &str,
        feedback_callback: FeedbackF,
    ) -> Result<ActionClientHandle<A>, EmbeddedNodeError>
    where
        A: RosAction + 'static,
        FeedbackF: FnMut(&nros_core::GoalId, &A::Feedback) + 'static,
        S::ServiceClientHandle: ServiceClientTrait,
        S::SubscriberHandle: Subscriber,
    {
        type Entry<A, Cli, Sub, FeedbackF, const GB: usize, const RB: usize, const FB: usize> =
            ActionClientArenaEntry<A, Cli, Sub, FeedbackF, GB, RB, FB>;

        let slot = self.next_entry_slot()?;

        // Create the action client entities
        let action_info = ActionInfo::new(action_name, A::ACTION_NAME, A::ACTION_HASH);

        let send_goal_keyexpr: heapless::String<256> = action_info.send_goal_key();
        let send_goal_info =
            ServiceInfo::new(&send_goal_keyexpr, A::ACTION_NAME, A::ACTION_HASH).with_domain(0);
        let send_goal_client = self
            .session
            .create_service_client(&send_goal_info)
            .map_err(|_| EmbeddedNodeError::ActionCreationFailed)?;

        let cancel_goal_keyexpr: heapless::String<256> = action_info.cancel_goal_key();
        let cancel_goal_info = ServiceInfo::new(
            &cancel_goal_keyexpr,
            "action_msgs::srv::dds_::CancelGoal_",
            A::ACTION_HASH,
        )
        .with_domain(0);
        let cancel_goal_client = self
            .session
            .create_service_client(&cancel_goal_info)
            .map_err(|_| EmbeddedNodeError::ActionCreationFailed)?;

        let get_result_keyexpr: heapless::String<256> = action_info.get_result_key();
        let get_result_info =
            ServiceInfo::new(&get_result_keyexpr, A::ACTION_NAME, A::ACTION_HASH).with_domain(0);
        let get_result_client = self
            .session
            .create_service_client(&get_result_info)
            .map_err(|_| EmbeddedNodeError::ActionCreationFailed)?;

        let feedback_keyexpr: heapless::String<256> = action_info.feedback_key();
        let feedback_topic =
            TopicInfo::new(&feedback_keyexpr, A::ACTION_NAME, A::ACTION_HASH).with_domain(0);
        let feedback_subscriber = self
            .session
            .create_subscriber(&feedback_topic, QosSettings::BEST_EFFORT)
            .map_err(|_| EmbeddedNodeError::ActionCreationFailed)?;

        let client = EmbeddedActionClient {
            send_goal_client,
            cancel_goal_client,
            get_result_client,
            feedback_subscriber,
            goal_buffer: [0u8; GOAL_BUF],
            result_buffer: [0u8; RESULT_BUF],
            feedback_buffer: [0u8; FEEDBACK_BUF],
            goal_counter: 0,
            _phantom: PhantomData,
        };

        let offset = self.arena_alloc::<Entry<
            A,
            S::ServiceClientHandle,
            S::SubscriberHandle,
            FeedbackF,
            GOAL_BUF,
            RESULT_BUF,
            FEEDBACK_BUF,
        >>()?;

        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(offset)
                as *mut Entry<
                    A,
                    S::ServiceClientHandle,
                    S::SubscriberHandle,
                    FeedbackF,
                    GOAL_BUF,
                    RESULT_BUF,
                    FEEDBACK_BUF,
                >;
            core::ptr::write(
                entry_ptr,
                Entry {
                    client,
                    feedback_callback,
                },
            );
        }

        self.entries[slot] = Some(CallbackMeta {
            offset,
            kind: EntryKind::ActionClient,
            try_process: action_client_try_process::<
                A,
                S::ServiceClientHandle,
                S::SubscriberHandle,
                FeedbackF,
                GOAL_BUF,
                RESULT_BUF,
                FEEDBACK_BUF,
            >,
            drop_fn: drop_entry::<
                Entry<
                    A,
                    S::ServiceClientHandle,
                    S::SubscriberHandle,
                    FeedbackF,
                    GOAL_BUF,
                    RESULT_BUF,
                    FEEDBACK_BUF,
                >,
            >,
        });

        Ok(ActionClientHandle {
            entry_index: slot,
            send_goal_fn: ac_send_goal::<
                A,
                S::ServiceClientHandle,
                S::SubscriberHandle,
                FeedbackF,
                GOAL_BUF,
                RESULT_BUF,
                FEEDBACK_BUF,
            >,
            cancel_goal_fn: ac_cancel_goal::<
                A,
                S::ServiceClientHandle,
                S::SubscriberHandle,
                FeedbackF,
                GOAL_BUF,
                RESULT_BUF,
                FEEDBACK_BUF,
            >,
            get_result_fn: ac_get_result::<
                A,
                S::ServiceClientHandle,
                S::SubscriberHandle,
                FeedbackF,
                GOAL_BUF,
                RESULT_BUF,
                FEEDBACK_BUF,
            >,
            _phantom: PhantomData,
        })
    }

    // ========================================================================
    // spin_once
    // ========================================================================

    /// Drive I/O and dispatch all registered callbacks once.
    ///
    /// 1. Calls [`drive_io()`](Self::drive_io) to pump the transport.
    /// 2. Iterates every registered entry and tries to process one item each.
    ///
    /// Returns a [`SpinOnceResult`] with counts of processed items and errors.
    pub fn spin_once(&mut self, timeout_ms: i32) -> SpinOnceResult {
        let _ = self.session.drive_io(timeout_ms);

        let delta_ms = timeout_ms.max(0) as u64;
        let mut result = SpinOnceResult::new();
        let arena_ptr = self.arena.as_mut_ptr() as *mut u8;

        for meta in self.entries.iter().flatten() {
            // SAFETY: `meta.offset` was set by `arena_alloc` which guarantees
            // alignment and bounds. The function pointer was set at registration
            // time with the matching concrete type.
            let data_ptr = unsafe { arena_ptr.add(meta.offset) };
            match unsafe { (meta.try_process)(data_ptr, delta_ms) } {
                Ok(true) => match meta.kind {
                    EntryKind::Subscription | EntryKind::ActionClient => {
                        result.subscriptions_processed += 1;
                    }
                    EntryKind::Service | EntryKind::ActionServer => {
                        result.services_handled += 1;
                    }
                    EntryKind::Timer => result.timers_fired += 1,
                },
                Ok(false) => {}
                Err(_) => match meta.kind {
                    EntryKind::Subscription | EntryKind::ActionClient => {
                        result.subscription_errors += 1;
                    }
                    EntryKind::Service | EntryKind::ActionServer => {
                        result.service_errors += 1;
                    }
                    EntryKind::Timer => {} // timers don't produce transport errors
                },
            }
        }
        result
    }
}

impl<S, const MAX_CBS: usize, const CB_ARENA: usize> Drop
    for EmbeddedExecutor<S, MAX_CBS, CB_ARENA>
{
    fn drop(&mut self) {
        let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
        for meta in self.entries.iter().flatten() {
            // SAFETY: each entry was written by `ptr::write` in `add_*` and
            // has not been dropped yet. `drop_fn` matches the concrete type.
            unsafe {
                let data_ptr = arena_ptr.add(meta.offset);
                (meta.drop_fn)(data_ptr);
            }
        }
    }
}

// ============================================================================
// Handle types for arena-registered action server/client
// ============================================================================

/// Handle to an action server registered in the executor's arena.
///
/// Returned by [`EmbeddedExecutor::add_action_server()`]. Provides methods
/// to interact with the server (publish feedback, complete goals) while the
/// executor automatically handles goal acceptance, cancel requests, and
/// result serving during [`spin_once()`](EmbeddedExecutor::spin_once).
pub struct ActionServerHandle<A: RosAction> {
    entry_index: usize,
    publish_feedback_fn:
        unsafe fn(*mut u8, &nros_core::GoalId, &A::Feedback) -> Result<(), EmbeddedNodeError>,
    complete_goal_fn: unsafe fn(*mut u8, &nros_core::GoalId, nros_core::GoalStatus, A::Result),
    set_goal_status_fn: unsafe fn(*mut u8, &nros_core::GoalId, nros_core::GoalStatus),
    _phantom: PhantomData<A>,
}

impl<A: RosAction> Clone for ActionServerHandle<A> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<A: RosAction> Copy for ActionServerHandle<A> {}

impl<A: RosAction> ActionServerHandle<A> {
    /// Publish feedback for an active goal.
    pub fn publish_feedback<S: Session, const MAX_CBS: usize, const CB_ARENA: usize>(
        &self,
        executor: &mut EmbeddedExecutor<S, MAX_CBS, CB_ARENA>,
        goal_id: &nros_core::GoalId,
        feedback: &A::Feedback,
    ) -> Result<(), EmbeddedNodeError> {
        let meta = executor.entries[self.entry_index]
            .as_ref()
            .ok_or(EmbeddedNodeError::BufferTooSmall)?;
        let arena_ptr = executor.arena.as_mut_ptr() as *mut u8;
        unsafe {
            let data_ptr = arena_ptr.add(meta.offset);
            (self.publish_feedback_fn)(data_ptr, goal_id, feedback)
        }
    }

    /// Complete a goal with final status and result.
    pub fn complete_goal<S, const MAX_CBS: usize, const CB_ARENA: usize>(
        &self,
        executor: &mut EmbeddedExecutor<S, MAX_CBS, CB_ARENA>,
        goal_id: &nros_core::GoalId,
        status: nros_core::GoalStatus,
        result: A::Result,
    ) {
        if let Some(meta) = executor.entries[self.entry_index].as_ref() {
            let arena_ptr = executor.arena.as_mut_ptr() as *mut u8;
            unsafe {
                let data_ptr = arena_ptr.add(meta.offset);
                (self.complete_goal_fn)(data_ptr, goal_id, status, result);
            }
        }
    }

    /// Update a goal's status.
    pub fn set_goal_status<S, const MAX_CBS: usize, const CB_ARENA: usize>(
        &self,
        executor: &mut EmbeddedExecutor<S, MAX_CBS, CB_ARENA>,
        goal_id: &nros_core::GoalId,
        status: nros_core::GoalStatus,
    ) {
        if let Some(meta) = executor.entries[self.entry_index].as_ref() {
            let arena_ptr = executor.arena.as_mut_ptr() as *mut u8;
            unsafe {
                let data_ptr = arena_ptr.add(meta.offset);
                (self.set_goal_status_fn)(data_ptr, goal_id, status);
            }
        }
    }
}

/// Handle to an action client registered in the executor's arena.
///
/// Returned by [`EmbeddedExecutor::add_action_client()`]. Provides methods
/// to send goals and get results while the executor automatically dispatches
/// feedback to the registered callback during [`spin_once()`](EmbeddedExecutor::spin_once).
#[allow(clippy::type_complexity)]
pub struct ActionClientHandle<A: RosAction> {
    entry_index: usize,
    send_goal_fn: unsafe fn(*mut u8, &A::Goal) -> Result<nros_core::GoalId, EmbeddedNodeError>,
    cancel_goal_fn: unsafe fn(
        *mut u8,
        &nros_core::GoalId,
    ) -> Result<nros_core::CancelResponse, EmbeddedNodeError>,
    get_result_fn: unsafe fn(
        *mut u8,
        &nros_core::GoalId,
    ) -> Result<(nros_core::GoalStatus, A::Result), EmbeddedNodeError>,
    _phantom: PhantomData<A>,
}

impl<A: RosAction> Clone for ActionClientHandle<A> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<A: RosAction> Copy for ActionClientHandle<A> {}

impl<A: RosAction> ActionClientHandle<A> {
    /// Send a goal to the action server (blocks until accepted/rejected).
    pub fn send_goal<S: Session, const MAX_CBS: usize, const CB_ARENA: usize>(
        &self,
        executor: &mut EmbeddedExecutor<S, MAX_CBS, CB_ARENA>,
        goal: &A::Goal,
    ) -> Result<nros_core::GoalId, EmbeddedNodeError> {
        let meta = executor.entries[self.entry_index]
            .as_ref()
            .ok_or(EmbeddedNodeError::BufferTooSmall)?;
        let arena_ptr = executor.arena.as_mut_ptr() as *mut u8;
        unsafe {
            let data_ptr = arena_ptr.add(meta.offset);
            (self.send_goal_fn)(data_ptr, goal)
        }
    }

    /// Cancel an active goal.
    pub fn cancel_goal<S: Session, const MAX_CBS: usize, const CB_ARENA: usize>(
        &self,
        executor: &mut EmbeddedExecutor<S, MAX_CBS, CB_ARENA>,
        goal_id: &nros_core::GoalId,
    ) -> Result<nros_core::CancelResponse, EmbeddedNodeError> {
        let meta = executor.entries[self.entry_index]
            .as_ref()
            .ok_or(EmbeddedNodeError::BufferTooSmall)?;
        let arena_ptr = executor.arena.as_mut_ptr() as *mut u8;
        unsafe {
            let data_ptr = arena_ptr.add(meta.offset);
            (self.cancel_goal_fn)(data_ptr, goal_id)
        }
    }

    /// Get the result of a completed goal.
    pub fn get_result<S: Session, const MAX_CBS: usize, const CB_ARENA: usize>(
        &self,
        executor: &mut EmbeddedExecutor<S, MAX_CBS, CB_ARENA>,
        goal_id: &nros_core::GoalId,
    ) -> Result<(nros_core::GoalStatus, A::Result), EmbeddedNodeError> {
        let meta = executor.entries[self.entry_index]
            .as_ref()
            .ok_or(EmbeddedNodeError::BufferTooSmall)?;
        let arena_ptr = executor.arena.as_mut_ptr() as *mut u8;
        unsafe {
            let data_ptr = arena_ptr.add(meta.offset);
            (self.get_result_fn)(data_ptr, goal_id)
        }
    }
}

// ============================================================================
// EmbeddedNode<S>
// ============================================================================

/// Backend-agnostic node — borrows the session to create typed entities.
pub struct EmbeddedNode<'a, S: Session> {
    name: heapless::String<64>,
    session: &'a mut S,
    domain_id: u32,
}

impl<'a, S: Session> EmbeddedNode<'a, S> {
    /// Get the node name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the domain ID.
    pub fn domain_id(&self) -> u32 {
        self.domain_id
    }

    /// Set the domain ID.
    pub fn set_domain_id(&mut self, domain_id: u32) {
        self.domain_id = domain_id;
    }

    /// Get a mutable reference to the underlying session.
    pub fn session_mut(&mut self) -> &mut S {
        self.session
    }

    // -- Publishers --

    /// Create a publisher for the given topic.
    pub fn create_publisher<M: RosMessage>(
        &mut self,
        topic_name: &str,
    ) -> Result<EmbeddedPublisher<M, S::PublisherHandle>, EmbeddedNodeError> {
        self.create_publisher_with_qos::<M>(topic_name, QosSettings::default())
    }

    /// Create a publisher with custom QoS settings.
    pub fn create_publisher_with_qos<M: RosMessage>(
        &mut self,
        topic_name: &str,
        qos: QosSettings,
    ) -> Result<EmbeddedPublisher<M, S::PublisherHandle>, EmbeddedNodeError> {
        let topic =
            TopicInfo::new(topic_name, M::TYPE_NAME, M::TYPE_HASH).with_domain(self.domain_id);
        let handle = self
            .session
            .create_publisher(&topic, qos)
            .map_err(|_| EmbeddedNodeError::Transport(TransportError::PublisherCreationFailed))?;
        Ok(EmbeddedPublisher {
            handle,
            _phantom: PhantomData,
        })
    }

    // -- Subscriptions --

    /// Create a subscription for the given topic.
    pub fn create_subscription<M: RosMessage>(
        &mut self,
        topic_name: &str,
    ) -> Result<EmbeddedSubscription<M, S::SubscriberHandle, 1024>, EmbeddedNodeError> {
        self.create_subscription_sized::<M, 1024>(topic_name)
    }

    /// Create a subscription with custom buffer size.
    pub fn create_subscription_sized<M: RosMessage, const RX_BUF: usize>(
        &mut self,
        topic_name: &str,
    ) -> Result<EmbeddedSubscription<M, S::SubscriberHandle, RX_BUF>, EmbeddedNodeError> {
        self.create_subscription_with_qos::<M, RX_BUF>(topic_name, QosSettings::default())
    }

    /// Create a subscription with custom QoS and buffer size.
    pub fn create_subscription_with_qos<M: RosMessage, const RX_BUF: usize>(
        &mut self,
        topic_name: &str,
        qos: QosSettings,
    ) -> Result<EmbeddedSubscription<M, S::SubscriberHandle, RX_BUF>, EmbeddedNodeError> {
        let topic =
            TopicInfo::new(topic_name, M::TYPE_NAME, M::TYPE_HASH).with_domain(self.domain_id);
        let handle = self
            .session
            .create_subscriber(&topic, qos)
            .map_err(|_| EmbeddedNodeError::Transport(TransportError::SubscriberCreationFailed))?;
        Ok(EmbeddedSubscription {
            handle,
            buffer: [0u8; RX_BUF],
            _phantom: PhantomData,
        })
    }

    // -- Services --

    /// Create a service server.
    pub fn create_service<Svc: RosService>(
        &mut self,
        service_name: &str,
    ) -> Result<EmbeddedServiceServer<Svc, S::ServiceServerHandle, 1024, 1024>, EmbeddedNodeError>
    {
        self.create_service_sized::<Svc, 1024, 1024>(service_name)
    }

    /// Create a service server with custom buffer sizes.
    pub fn create_service_sized<Svc: RosService, const REQ_BUF: usize, const REPLY_BUF: usize>(
        &mut self,
        service_name: &str,
    ) -> Result<
        EmbeddedServiceServer<Svc, S::ServiceServerHandle, REQ_BUF, REPLY_BUF>,
        EmbeddedNodeError,
    > {
        let info = ServiceInfo::new(service_name, Svc::SERVICE_NAME, Svc::SERVICE_HASH)
            .with_domain(self.domain_id);
        let handle = self.session.create_service_server(&info).map_err(|_| {
            EmbeddedNodeError::Transport(TransportError::ServiceServerCreationFailed)
        })?;
        Ok(EmbeddedServiceServer {
            handle,
            req_buffer: [0u8; REQ_BUF],
            reply_buffer: [0u8; REPLY_BUF],
            _phantom: PhantomData,
        })
    }

    /// Create a service client.
    pub fn create_client<Svc: RosService>(
        &mut self,
        service_name: &str,
    ) -> Result<EmbeddedServiceClient<Svc, S::ServiceClientHandle, 1024, 1024>, EmbeddedNodeError>
    {
        self.create_client_sized::<Svc, 1024, 1024>(service_name)
    }

    /// Create a service client with custom buffer sizes.
    pub fn create_client_sized<Svc: RosService, const REQ_BUF: usize, const REPLY_BUF: usize>(
        &mut self,
        service_name: &str,
    ) -> Result<
        EmbeddedServiceClient<Svc, S::ServiceClientHandle, REQ_BUF, REPLY_BUF>,
        EmbeddedNodeError,
    > {
        let info = ServiceInfo::new(service_name, Svc::SERVICE_NAME, Svc::SERVICE_HASH)
            .with_domain(self.domain_id);
        let handle = self.session.create_service_client(&info).map_err(|_| {
            EmbeddedNodeError::Transport(TransportError::ServiceClientCreationFailed)
        })?;
        Ok(EmbeddedServiceClient {
            handle,
            req_buffer: [0u8; REQ_BUF],
            reply_buffer: [0u8; REPLY_BUF],
            _phantom: PhantomData,
        })
    }

    // -- Actions --

    /// Create an action server.
    pub fn create_action_server<A: RosAction>(
        &mut self,
        action_name: &str,
    ) -> Result<
        EmbeddedActionServer<A, S::ServiceServerHandle, S::PublisherHandle, 1024, 1024, 1024, 4>,
        EmbeddedNodeError,
    > {
        self.create_action_server_sized::<A, 1024, 1024, 1024, 4>(action_name)
    }

    /// Create an action server with custom buffer sizes.
    pub fn create_action_server_sized<
        A: RosAction,
        const GOAL_BUF: usize,
        const RESULT_BUF: usize,
        const FEEDBACK_BUF: usize,
        const MAX_GOALS: usize,
    >(
        &mut self,
        action_name: &str,
    ) -> Result<
        EmbeddedActionServer<
            A,
            S::ServiceServerHandle,
            S::PublisherHandle,
            GOAL_BUF,
            RESULT_BUF,
            FEEDBACK_BUF,
            MAX_GOALS,
        >,
        EmbeddedNodeError,
    > {
        let action_info = ActionInfo::new(action_name, A::ACTION_NAME, A::ACTION_HASH)
            .with_domain(self.domain_id);

        let send_goal_keyexpr: heapless::String<256> = action_info.send_goal_key();
        let send_goal_info =
            ServiceInfo::new(&send_goal_keyexpr, A::ACTION_NAME, A::ACTION_HASH).with_domain(0);
        let send_goal_server = self
            .session
            .create_service_server(&send_goal_info)
            .map_err(|_| EmbeddedNodeError::ActionCreationFailed)?;

        let cancel_goal_keyexpr: heapless::String<256> = action_info.cancel_goal_key();
        let cancel_goal_info = ServiceInfo::new(
            &cancel_goal_keyexpr,
            "action_msgs::srv::dds_::CancelGoal_",
            A::ACTION_HASH,
        )
        .with_domain(0);
        let cancel_goal_server = self
            .session
            .create_service_server(&cancel_goal_info)
            .map_err(|_| EmbeddedNodeError::ActionCreationFailed)?;

        let get_result_keyexpr: heapless::String<256> = action_info.get_result_key();
        let get_result_info =
            ServiceInfo::new(&get_result_keyexpr, A::ACTION_NAME, A::ACTION_HASH).with_domain(0);
        let get_result_server = self
            .session
            .create_service_server(&get_result_info)
            .map_err(|_| EmbeddedNodeError::ActionCreationFailed)?;

        let feedback_keyexpr: heapless::String<256> = action_info.feedback_key();
        let feedback_topic =
            TopicInfo::new(&feedback_keyexpr, A::ACTION_NAME, A::ACTION_HASH).with_domain(0);
        let feedback_publisher = self
            .session
            .create_publisher(&feedback_topic, QosSettings::BEST_EFFORT)
            .map_err(|_| EmbeddedNodeError::ActionCreationFailed)?;

        let status_keyexpr: heapless::String<256> = action_info.status_key();
        let status_topic = TopicInfo::new(
            &status_keyexpr,
            "action_msgs::msg::dds_::GoalStatusArray_",
            A::ACTION_HASH,
        )
        .with_domain(0);
        let status_publisher = self
            .session
            .create_publisher(&status_topic, QosSettings::BEST_EFFORT)
            .map_err(|_| EmbeddedNodeError::ActionCreationFailed)?;

        Ok(EmbeddedActionServer {
            send_goal_server,
            cancel_goal_server,
            get_result_server,
            feedback_publisher,
            _status_publisher: status_publisher,
            active_goals: heapless::Vec::new(),
            completed_goals: heapless::Vec::new(),
            goal_buffer: [0u8; GOAL_BUF],
            result_buffer: [0u8; RESULT_BUF],
            feedback_buffer: [0u8; FEEDBACK_BUF],
            cancel_buffer: [0u8; 256],
        })
    }

    /// Create an action client.
    pub fn create_action_client<A: RosAction>(
        &mut self,
        action_name: &str,
    ) -> Result<
        EmbeddedActionClient<A, S::ServiceClientHandle, S::SubscriberHandle, 1024, 1024, 1024>,
        EmbeddedNodeError,
    > {
        self.create_action_client_sized::<A, 1024, 1024, 1024>(action_name)
    }

    /// Create an action client with custom buffer sizes.
    pub fn create_action_client_sized<
        A: RosAction,
        const GOAL_BUF: usize,
        const RESULT_BUF: usize,
        const FEEDBACK_BUF: usize,
    >(
        &mut self,
        action_name: &str,
    ) -> Result<
        EmbeddedActionClient<
            A,
            S::ServiceClientHandle,
            S::SubscriberHandle,
            GOAL_BUF,
            RESULT_BUF,
            FEEDBACK_BUF,
        >,
        EmbeddedNodeError,
    > {
        let action_info = ActionInfo::new(action_name, A::ACTION_NAME, A::ACTION_HASH)
            .with_domain(self.domain_id);

        let send_goal_keyexpr: heapless::String<256> = action_info.send_goal_key();
        let send_goal_info =
            ServiceInfo::new(&send_goal_keyexpr, A::ACTION_NAME, A::ACTION_HASH).with_domain(0);
        let send_goal_client = self
            .session
            .create_service_client(&send_goal_info)
            .map_err(|_| EmbeddedNodeError::ActionCreationFailed)?;

        let cancel_goal_keyexpr: heapless::String<256> = action_info.cancel_goal_key();
        let cancel_goal_info = ServiceInfo::new(
            &cancel_goal_keyexpr,
            "action_msgs::srv::dds_::CancelGoal_",
            A::ACTION_HASH,
        )
        .with_domain(0);
        let cancel_goal_client = self
            .session
            .create_service_client(&cancel_goal_info)
            .map_err(|_| EmbeddedNodeError::ActionCreationFailed)?;

        let get_result_keyexpr: heapless::String<256> = action_info.get_result_key();
        let get_result_info =
            ServiceInfo::new(&get_result_keyexpr, A::ACTION_NAME, A::ACTION_HASH).with_domain(0);
        let get_result_client = self
            .session
            .create_service_client(&get_result_info)
            .map_err(|_| EmbeddedNodeError::ActionCreationFailed)?;

        let feedback_keyexpr: heapless::String<256> = action_info.feedback_key();
        let feedback_topic =
            TopicInfo::new(&feedback_keyexpr, A::ACTION_NAME, A::ACTION_HASH).with_domain(0);
        let feedback_subscriber = self
            .session
            .create_subscriber(&feedback_topic, QosSettings::BEST_EFFORT)
            .map_err(|_| EmbeddedNodeError::ActionCreationFailed)?;

        Ok(EmbeddedActionClient {
            send_goal_client,
            cancel_goal_client,
            get_result_client,
            feedback_subscriber,
            goal_buffer: [0u8; GOAL_BUF],
            result_buffer: [0u8; RESULT_BUF],
            feedback_buffer: [0u8; FEEDBACK_BUF],
            goal_counter: 0,
            _phantom: PhantomData,
        })
    }
}

// ============================================================================
// EmbeddedPublisher
// ============================================================================

/// Typed publisher handle.
pub struct EmbeddedPublisher<M, P> {
    handle: P,
    _phantom: PhantomData<M>,
}

impl<M: RosMessage, P: Publisher> EmbeddedPublisher<M, P> {
    /// Publish a message using the default buffer size.
    pub fn publish(&self, msg: &M) -> Result<(), EmbeddedNodeError> {
        self.publish_with_buffer::<DEFAULT_TX_BUF>(msg)
    }

    /// Publish a message with a custom buffer size.
    pub fn publish_with_buffer<const BUF: usize>(&self, msg: &M) -> Result<(), EmbeddedNodeError> {
        let mut buffer = [0u8; BUF];
        let mut writer = CdrWriter::new_with_header(&mut buffer)
            .map_err(|_| EmbeddedNodeError::BufferTooSmall)?;
        msg.serialize(&mut writer)
            .map_err(|_| EmbeddedNodeError::Serialization)?;
        let len = writer.position();
        self.handle
            .publish_raw(&buffer[..len])
            .map_err(|_| EmbeddedNodeError::Transport(TransportError::PublishFailed))
    }

    /// Publish raw CDR-encoded data (must include CDR header).
    pub fn publish_raw(&self, data: &[u8]) -> Result<(), EmbeddedNodeError> {
        self.handle
            .publish_raw(data)
            .map_err(|_| EmbeddedNodeError::Transport(TransportError::PublishFailed))
    }
}

// ============================================================================
// EmbeddedSubscription
// ============================================================================

/// Typed subscription handle with internal receive buffer.
pub struct EmbeddedSubscription<M, Sub, const RX_BUF: usize = 1024> {
    handle: Sub,
    buffer: [u8; RX_BUF],
    _phantom: PhantomData<M>,
}

impl<M: RosMessage, Sub: Subscriber, const RX_BUF: usize> EmbeddedSubscription<M, Sub, RX_BUF> {
    /// Try to receive a typed message (non-blocking).
    pub fn try_recv(&mut self) -> Result<Option<M>, EmbeddedNodeError> {
        match self
            .handle
            .try_recv_raw(&mut self.buffer)
            .map_err(|_| EmbeddedNodeError::Transport(TransportError::DeserializationError))?
        {
            Some(len) => {
                let mut reader = CdrReader::new_with_header(&self.buffer[..len]).map_err(|_| {
                    EmbeddedNodeError::Transport(TransportError::DeserializationError)
                })?;
                let msg = M::deserialize(&mut reader).map_err(|_| {
                    EmbeddedNodeError::Transport(TransportError::DeserializationError)
                })?;
                Ok(Some(msg))
            }
            None => Ok(None),
        }
    }

    /// Try to receive raw CDR-encoded data (non-blocking).
    pub fn try_recv_raw(&mut self) -> Result<Option<usize>, EmbeddedNodeError> {
        self.handle
            .try_recv_raw(&mut self.buffer)
            .map_err(|_| EmbeddedNodeError::Transport(TransportError::DeserializationError))
    }

    /// Get the receive buffer (valid after `try_recv_raw`).
    pub fn buffer(&self) -> &[u8] {
        &self.buffer
    }

    /// Check if data is available without consuming it.
    pub fn has_data(&self) -> bool {
        self.handle.has_data()
    }

    /// Process the received message in-place without copying.
    pub fn process_in_place(&mut self, f: impl FnOnce(&M)) -> Result<bool, EmbeddedNodeError> {
        let mut deser_err = false;
        let processed = self
            .handle
            .process_raw_in_place(|raw| {
                match CdrReader::new_with_header(raw).and_then(|mut r| M::deserialize(&mut r)) {
                    Ok(msg) => f(&msg),
                    Err(_) => deser_err = true,
                }
            })
            .map_err(|_| EmbeddedNodeError::Transport(TransportError::DeserializationError))?;

        if deser_err {
            return Err(EmbeddedNodeError::Transport(
                TransportError::DeserializationError,
            ));
        }
        Ok(processed)
    }
}

// ============================================================================
// EmbeddedServiceServer
// ============================================================================

/// Typed service server handle with internal buffers.
pub struct EmbeddedServiceServer<
    Svc: RosService,
    Srv,
    const REQ_BUF: usize = 1024,
    const REPLY_BUF: usize = 1024,
> {
    handle: Srv,
    req_buffer: [u8; REQ_BUF],
    reply_buffer: [u8; REPLY_BUF],
    _phantom: PhantomData<Svc>,
}

impl<Svc: RosService, Srv: ServiceServerTrait, const REQ_BUF: usize, const REPLY_BUF: usize>
    EmbeddedServiceServer<Svc, Srv, REQ_BUF, REPLY_BUF>
where
    Srv::Error: From<TransportError>,
{
    /// Handle an incoming service request.
    ///
    /// Returns `Ok(true)` if a request was handled, `Ok(false)` if none available.
    pub fn handle_request(
        &mut self,
        handler: impl FnOnce(&Svc::Request) -> Svc::Reply,
    ) -> Result<bool, EmbeddedNodeError> {
        self.handle
            .handle_request::<Svc>(&mut self.req_buffer, &mut self.reply_buffer, handler)
            .map_err(|_| EmbeddedNodeError::ServiceReplyFailed)
    }

    /// Check if a request is available.
    pub fn has_request(&self) -> bool {
        self.handle.has_request()
    }
}

// ============================================================================
// EmbeddedServiceClient
// ============================================================================

/// Typed service client handle with internal buffers.
pub struct EmbeddedServiceClient<
    Svc: RosService,
    Cli,
    const REQ_BUF: usize = 1024,
    const REPLY_BUF: usize = 1024,
> {
    handle: Cli,
    req_buffer: [u8; REQ_BUF],
    reply_buffer: [u8; REPLY_BUF],
    _phantom: PhantomData<Svc>,
}

impl<Svc: RosService, Cli: ServiceClientTrait, const REQ_BUF: usize, const REPLY_BUF: usize>
    EmbeddedServiceClient<Svc, Cli, REQ_BUF, REPLY_BUF>
where
    Cli::Error: From<TransportError>,
{
    /// Call the service with a typed request and wait for reply.
    pub fn call(&mut self, request: &Svc::Request) -> Result<Svc::Reply, EmbeddedNodeError> {
        self.handle
            .call::<Svc>(request, &mut self.req_buffer, &mut self.reply_buffer)
            .map_err(|_| EmbeddedNodeError::ServiceRequestFailed)
    }
}

// ============================================================================
// Action types
// ============================================================================

/// Active goal tracking for action server.
#[derive(Clone)]
pub struct EmbeddedActiveGoal<A: RosAction> {
    /// Goal ID.
    pub goal_id: nros_core::GoalId,
    /// Current status.
    pub status: nros_core::GoalStatus,
    /// The goal data.
    pub goal: A::Goal,
}

/// Completed goal with result.
pub struct EmbeddedCompletedGoal<A: RosAction> {
    /// Goal ID.
    pub goal_id: nros_core::GoalId,
    /// Final status.
    pub status: nros_core::GoalStatus,
    /// The result data.
    pub result: A::Result,
}

// ============================================================================
// EmbeddedActionServer
// ============================================================================

/// Typed action server with goal state management.
pub struct EmbeddedActionServer<
    A: RosAction,
    Srv,
    Pub,
    const GOAL_BUF: usize = 1024,
    const RESULT_BUF: usize = 1024,
    const FEEDBACK_BUF: usize = 1024,
    const MAX_GOALS: usize = 4,
> {
    send_goal_server: Srv,
    cancel_goal_server: Srv,
    get_result_server: Srv,
    feedback_publisher: Pub,
    _status_publisher: Pub,
    active_goals: heapless::Vec<EmbeddedActiveGoal<A>, MAX_GOALS>,
    completed_goals: heapless::Vec<EmbeddedCompletedGoal<A>, MAX_GOALS>,
    goal_buffer: [u8; GOAL_BUF],
    result_buffer: [u8; RESULT_BUF],
    feedback_buffer: [u8; FEEDBACK_BUF],
    cancel_buffer: [u8; 256],
}

impl<
    A: RosAction,
    Srv: ServiceServerTrait,
    Pub: Publisher,
    const GOAL_BUF: usize,
    const RESULT_BUF: usize,
    const FEEDBACK_BUF: usize,
    const MAX_GOALS: usize,
> EmbeddedActionServer<A, Srv, Pub, GOAL_BUF, RESULT_BUF, FEEDBACK_BUF, MAX_GOALS>
{
    /// Try to accept a new goal.
    ///
    /// Checks for incoming send_goal requests. If one is available, calls the
    /// handler to decide acceptance. Returns the goal ID if accepted.
    pub fn try_accept_goal(
        &mut self,
        goal_handler: impl FnOnce(&A::Goal) -> nros_core::GoalResponse,
    ) -> Result<Option<nros_core::GoalId>, EmbeddedNodeError>
    where
        A::Goal: Clone,
    {
        let request = self
            .send_goal_server
            .try_recv_request(&mut self.goal_buffer)
            .map_err(|_| EmbeddedNodeError::Transport(TransportError::ServiceRequestFailed))?;

        let request = match request {
            Some(r) => r,
            None => return Ok(None),
        };

        let data_len = request.data.len();
        let sequence_number = request.sequence_number;
        #[allow(clippy::drop_non_drop)]
        drop(request);

        let mut reader = CdrReader::new_with_header(&self.goal_buffer[..data_len])
            .map_err(|_| EmbeddedNodeError::Transport(TransportError::DeserializationError))?;

        // Read goal_id (UUID as CDR sequence)
        let uuid_len = reader
            .read_u32()
            .map_err(|_| EmbeddedNodeError::Transport(TransportError::DeserializationError))?
            as usize;
        let mut goal_id = nros_core::GoalId::default();
        if uuid_len == 16 {
            for byte in &mut goal_id.uuid {
                *byte = reader.read_u8().map_err(|_| {
                    EmbeddedNodeError::Transport(TransportError::DeserializationError)
                })?;
            }
        }

        let goal = A::Goal::deserialize(&mut reader)
            .map_err(|_| EmbeddedNodeError::Transport(TransportError::DeserializationError))?;

        let response = goal_handler(&goal);
        let accepted = response.is_accepted();

        // Serialize response: accepted (bool) + stamp (Time)
        let mut writer = CdrWriter::new_with_header(&mut self.result_buffer)
            .map_err(|_| EmbeddedNodeError::BufferTooSmall)?;
        writer
            .write_u8(if accepted { 1 } else { 0 })
            .map_err(|_| EmbeddedNodeError::Serialization)?;
        writer
            .write_i32(0)
            .map_err(|_| EmbeddedNodeError::Serialization)?;
        writer
            .write_u32(0)
            .map_err(|_| EmbeddedNodeError::Serialization)?;
        let reply_len = writer.position();

        self.send_goal_server
            .send_reply(sequence_number, &self.result_buffer[..reply_len])
            .map_err(|_| EmbeddedNodeError::ServiceReplyFailed)?;

        if accepted {
            let _ = self.active_goals.push(EmbeddedActiveGoal {
                goal_id,
                status: nros_core::GoalStatus::Accepted,
                goal,
            });
            Ok(Some(goal_id))
        } else {
            Ok(None)
        }
    }

    /// Publish feedback for a goal.
    pub fn publish_feedback(
        &mut self,
        goal_id: &nros_core::GoalId,
        feedback: &A::Feedback,
    ) -> Result<(), EmbeddedNodeError> {
        let mut writer = CdrWriter::new_with_header(&mut self.feedback_buffer)
            .map_err(|_| EmbeddedNodeError::BufferTooSmall)?;

        writer
            .write_u32(16)
            .map_err(|_| EmbeddedNodeError::Serialization)?;
        for b in &goal_id.uuid {
            writer
                .write_u8(*b)
                .map_err(|_| EmbeddedNodeError::Serialization)?;
        }

        feedback
            .serialize(&mut writer)
            .map_err(|_| EmbeddedNodeError::Serialization)?;

        let len = writer.position();
        self.feedback_publisher
            .publish_raw(&self.feedback_buffer[..len])
            .map_err(|_| EmbeddedNodeError::Transport(TransportError::PublishFailed))
    }

    /// Set a goal's status.
    pub fn set_goal_status(&mut self, goal_id: &nros_core::GoalId, status: nros_core::GoalStatus) {
        for goal in &mut self.active_goals {
            if goal.goal_id.uuid == goal_id.uuid {
                goal.status = status;
                break;
            }
        }
    }

    /// Complete a goal and store the result.
    pub fn complete_goal(
        &mut self,
        goal_id: &nros_core::GoalId,
        status: nros_core::GoalStatus,
        result: A::Result,
    ) {
        if let Some(pos) = self
            .active_goals
            .iter()
            .position(|g| g.goal_id.uuid == goal_id.uuid)
        {
            self.active_goals.swap_remove(pos);
        }

        let _ = self.completed_goals.push(EmbeddedCompletedGoal {
            goal_id: *goal_id,
            status,
            result,
        });
    }

    /// Try to handle a cancel_goal request.
    pub fn try_handle_cancel(
        &mut self,
        cancel_handler: impl FnOnce(
            &nros_core::GoalId,
            nros_core::GoalStatus,
        ) -> nros_core::CancelResponse,
    ) -> Result<Option<(nros_core::GoalId, nros_core::CancelResponse)>, EmbeddedNodeError> {
        let request = self
            .cancel_goal_server
            .try_recv_request(&mut self.cancel_buffer)
            .map_err(|_| EmbeddedNodeError::Transport(TransportError::ServiceRequestFailed))?;

        let request = match request {
            Some(r) => r,
            None => return Ok(None),
        };

        let data_len = request.data.len();
        let sequence_number = request.sequence_number;
        #[allow(clippy::drop_non_drop)]
        drop(request);

        let mut reader = CdrReader::new_with_header(&self.cancel_buffer[..data_len])
            .map_err(|_| EmbeddedNodeError::Transport(TransportError::DeserializationError))?;

        let mut goal_id = nros_core::GoalId::default();
        let uuid_len = reader.read_u32().unwrap_or(0) as usize;
        if uuid_len == 16 {
            for byte in &mut goal_id.uuid {
                *byte = reader.read_u8().unwrap_or(0);
            }
        }

        let current_status = self
            .active_goals
            .iter()
            .find(|g| g.goal_id.uuid == goal_id.uuid)
            .map(|g| g.status)
            .unwrap_or(nros_core::GoalStatus::Unknown);

        let response = cancel_handler(&goal_id, current_status);

        if response == nros_core::CancelResponse::Ok {
            self.set_goal_status(&goal_id, nros_core::GoalStatus::Canceling);
        }

        // Serialize response: return_code (i8) + goals_canceling (sequence of GoalInfo)
        let mut writer = CdrWriter::new_with_header(&mut self.goal_buffer)
            .map_err(|_| EmbeddedNodeError::BufferTooSmall)?;
        writer
            .write_i8(response as i8)
            .map_err(|_| EmbeddedNodeError::Serialization)?;

        let num_canceling = if response == nros_core::CancelResponse::Ok {
            1u32
        } else {
            0u32
        };
        writer
            .write_u32(num_canceling)
            .map_err(|_| EmbeddedNodeError::Serialization)?;
        if response == nros_core::CancelResponse::Ok {
            writer
                .write_u32(16)
                .map_err(|_| EmbeddedNodeError::Serialization)?;
            for b in &goal_id.uuid {
                writer
                    .write_u8(*b)
                    .map_err(|_| EmbeddedNodeError::Serialization)?;
            }
            writer
                .write_i32(0)
                .map_err(|_| EmbeddedNodeError::Serialization)?;
            writer
                .write_u32(0)
                .map_err(|_| EmbeddedNodeError::Serialization)?;
        }
        let reply_len = writer.position();

        self.cancel_goal_server
            .send_reply(sequence_number, &self.goal_buffer[..reply_len])
            .map_err(|_| EmbeddedNodeError::ServiceReplyFailed)?;

        Ok(Some((goal_id, response)))
    }

    /// Try to handle a get_result request.
    pub fn try_handle_get_result(&mut self) -> Result<Option<nros_core::GoalId>, EmbeddedNodeError>
    where
        A::Result: Clone,
    {
        let request = self
            .get_result_server
            .try_recv_request(&mut self.goal_buffer)
            .map_err(|_| EmbeddedNodeError::Transport(TransportError::ServiceRequestFailed))?;

        let request = match request {
            Some(r) => r,
            None => return Ok(None),
        };

        let data_len = request.data.len();
        let sequence_number = request.sequence_number;
        #[allow(clippy::drop_non_drop)]
        drop(request);

        let mut reader = CdrReader::new_with_header(&self.goal_buffer[..data_len])
            .map_err(|_| EmbeddedNodeError::Transport(TransportError::DeserializationError))?;

        let mut goal_id = nros_core::GoalId::default();
        let uuid_len = reader.read_u32().unwrap_or(0) as usize;
        if uuid_len == 16 {
            for byte in &mut goal_id.uuid {
                *byte = reader.read_u8().unwrap_or(0);
            }
        }

        let completed = self
            .completed_goals
            .iter()
            .find(|g| g.goal_id.uuid == goal_id.uuid);

        let active = self
            .active_goals
            .iter()
            .find(|g| g.goal_id.uuid == goal_id.uuid);

        let mut writer = CdrWriter::new_with_header(&mut self.result_buffer)
            .map_err(|_| EmbeddedNodeError::BufferTooSmall)?;

        if let Some(completed_goal) = completed {
            writer
                .write_i8(completed_goal.status as i8)
                .map_err(|_| EmbeddedNodeError::Serialization)?;
            completed_goal
                .result
                .serialize(&mut writer)
                .map_err(|_| EmbeddedNodeError::Serialization)?;
        } else if let Some(active_goal) = active {
            writer
                .write_i8(active_goal.status as i8)
                .map_err(|_| EmbeddedNodeError::Serialization)?;
            A::Result::default()
                .serialize(&mut writer)
                .map_err(|_| EmbeddedNodeError::Serialization)?;
        } else {
            writer
                .write_i8(nros_core::GoalStatus::Unknown as i8)
                .map_err(|_| EmbeddedNodeError::Serialization)?;
            A::Result::default()
                .serialize(&mut writer)
                .map_err(|_| EmbeddedNodeError::Serialization)?;
        }

        let reply_len = writer.position();
        self.get_result_server
            .send_reply(sequence_number, &self.result_buffer[..reply_len])
            .map_err(|_| EmbeddedNodeError::ServiceReplyFailed)?;

        Ok(Some(goal_id))
    }

    /// Get a reference to an active goal.
    pub fn get_goal(&self, goal_id: &nros_core::GoalId) -> Option<&EmbeddedActiveGoal<A>> {
        self.active_goals
            .iter()
            .find(|g| g.goal_id.uuid == goal_id.uuid)
    }

    /// Get all active goals.
    pub fn active_goals(&self) -> &[EmbeddedActiveGoal<A>] {
        &self.active_goals
    }

    /// Get the number of active goals.
    pub fn active_goal_count(&self) -> usize {
        self.active_goals.len()
    }
}

// ============================================================================
// EmbeddedActionClient
// ============================================================================

/// Typed action client handle.
pub struct EmbeddedActionClient<
    A: RosAction,
    Cli,
    Sub,
    const GOAL_BUF: usize = 1024,
    const RESULT_BUF: usize = 1024,
    const FEEDBACK_BUF: usize = 1024,
> {
    send_goal_client: Cli,
    cancel_goal_client: Cli,
    get_result_client: Cli,
    feedback_subscriber: Sub,
    goal_buffer: [u8; GOAL_BUF],
    result_buffer: [u8; RESULT_BUF],
    feedback_buffer: [u8; FEEDBACK_BUF],
    goal_counter: u64,
    _phantom: PhantomData<A>,
}

impl<
    A: RosAction,
    Cli: ServiceClientTrait,
    Sub: Subscriber,
    const GOAL_BUF: usize,
    const RESULT_BUF: usize,
    const FEEDBACK_BUF: usize,
> EmbeddedActionClient<A, Cli, Sub, GOAL_BUF, RESULT_BUF, FEEDBACK_BUF>
{
    /// Send a goal to the action server.
    pub fn send_goal(&mut self, goal: &A::Goal) -> Result<nros_core::GoalId, EmbeddedNodeError> {
        self.goal_counter += 1;
        let mut goal_id = nros_core::GoalId::default();
        let counter_bytes = self.goal_counter.to_le_bytes();
        goal_id.uuid[..8].copy_from_slice(&counter_bytes);

        let mut writer = CdrWriter::new_with_header(&mut self.goal_buffer)
            .map_err(|_| EmbeddedNodeError::BufferTooSmall)?;

        writer
            .write_u32(16)
            .map_err(|_| EmbeddedNodeError::Serialization)?;
        for b in &goal_id.uuid {
            writer
                .write_u8(*b)
                .map_err(|_| EmbeddedNodeError::Serialization)?;
        }

        goal.serialize(&mut writer)
            .map_err(|_| EmbeddedNodeError::Serialization)?;

        let req_len = writer.position();

        let reply_len = self
            .send_goal_client
            .call_raw(&self.goal_buffer[..req_len], &mut self.result_buffer)
            .map_err(|_| EmbeddedNodeError::ServiceRequestFailed)?;

        let mut reader = CdrReader::new_with_header(&self.result_buffer[..reply_len])
            .map_err(|_| EmbeddedNodeError::Transport(TransportError::DeserializationError))?;

        let accepted = reader.read_u8().unwrap_or(0) != 0;

        if accepted {
            Ok(goal_id)
        } else {
            Err(EmbeddedNodeError::ServiceRequestFailed)
        }
    }

    /// Try to receive feedback (non-blocking).
    pub fn try_recv_feedback(
        &mut self,
    ) -> Result<Option<(nros_core::GoalId, A::Feedback)>, EmbeddedNodeError> {
        let data = self
            .feedback_subscriber
            .try_recv_raw(&mut self.feedback_buffer)
            .map_err(|_| EmbeddedNodeError::Transport(TransportError::DeserializationError))?;

        let len = match data {
            Some(len) => len,
            None => return Ok(None),
        };

        let mut reader = CdrReader::new_with_header(&self.feedback_buffer[..len])
            .map_err(|_| EmbeddedNodeError::Transport(TransportError::DeserializationError))?;

        let mut goal_id = nros_core::GoalId::default();
        let uuid_len = reader.read_u32().unwrap_or(0) as usize;
        if uuid_len == 16 {
            for byte in &mut goal_id.uuid {
                *byte = reader.read_u8().unwrap_or(0);
            }
        }

        let feedback = A::Feedback::deserialize(&mut reader)
            .map_err(|_| EmbeddedNodeError::Transport(TransportError::DeserializationError))?;

        Ok(Some((goal_id, feedback)))
    }

    /// Cancel a goal.
    pub fn cancel_goal(
        &mut self,
        goal_id: &nros_core::GoalId,
    ) -> Result<nros_core::CancelResponse, EmbeddedNodeError> {
        let mut writer = CdrWriter::new_with_header(&mut self.goal_buffer)
            .map_err(|_| EmbeddedNodeError::BufferTooSmall)?;

        writer
            .write_u32(16)
            .map_err(|_| EmbeddedNodeError::Serialization)?;
        for b in &goal_id.uuid {
            writer
                .write_u8(*b)
                .map_err(|_| EmbeddedNodeError::Serialization)?;
        }
        writer
            .write_i32(0)
            .map_err(|_| EmbeddedNodeError::Serialization)?;
        writer
            .write_u32(0)
            .map_err(|_| EmbeddedNodeError::Serialization)?;

        let req_len = writer.position();

        let reply_len = self
            .cancel_goal_client
            .call_raw(&self.goal_buffer[..req_len], &mut self.result_buffer)
            .map_err(|_| EmbeddedNodeError::ServiceRequestFailed)?;

        let mut reader = CdrReader::new_with_header(&self.result_buffer[..reply_len])
            .map_err(|_| EmbeddedNodeError::Transport(TransportError::DeserializationError))?;

        let return_code = reader.read_i8().unwrap_or(2);
        Ok(nros_core::CancelResponse::from_i8(return_code).unwrap_or_default())
    }

    /// Get the result of a completed goal.
    pub fn get_result(
        &mut self,
        goal_id: &nros_core::GoalId,
    ) -> Result<(nros_core::GoalStatus, A::Result), EmbeddedNodeError> {
        let mut writer = CdrWriter::new_with_header(&mut self.goal_buffer)
            .map_err(|_| EmbeddedNodeError::BufferTooSmall)?;

        writer
            .write_u32(16)
            .map_err(|_| EmbeddedNodeError::Serialization)?;
        for b in &goal_id.uuid {
            writer
                .write_u8(*b)
                .map_err(|_| EmbeddedNodeError::Serialization)?;
        }

        let req_len = writer.position();

        let reply_len = self
            .get_result_client
            .call_raw(&self.goal_buffer[..req_len], &mut self.result_buffer)
            .map_err(|_| EmbeddedNodeError::ServiceRequestFailed)?;

        let mut reader = CdrReader::new_with_header(&self.result_buffer[..reply_len])
            .map_err(|_| EmbeddedNodeError::Transport(TransportError::DeserializationError))?;

        let status_code = reader.read_i8().unwrap_or(0);
        let status = nros_core::GoalStatus::from_i8(status_code).unwrap_or_default();

        let result = A::Result::deserialize(&mut reader)
            .map_err(|_| EmbeddedNodeError::Transport(TransportError::DeserializationError))?;

        Ok((status, result))
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use core::cell::Cell;
    use nros_core::{DeserError, SerError};
    use nros_rmw::ServiceRequest;

    #[test]
    fn test_error_conversion() {
        let transport_err = TransportError::ConnectionFailed;
        let node_err: EmbeddedNodeError = transport_err.into();
        assert_eq!(
            node_err,
            EmbeddedNodeError::Transport(TransportError::ConnectionFailed)
        );
    }

    // ====================================================================
    // Mock types for arena callback tests
    // ====================================================================

    /// Simple test message: a single i32.
    #[derive(Debug, Clone, PartialEq)]
    struct TestMsg {
        data: i32,
    }

    impl RosMessage for TestMsg {
        const TYPE_NAME: &'static str = "test/msg/TestMsg";
        const TYPE_HASH: &'static str = "test_hash";
    }

    impl Serialize for TestMsg {
        fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
            writer.write_i32(self.data)
        }
    }

    impl Deserialize for TestMsg {
        fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
            Ok(Self {
                data: reader.read_i32()?,
            })
        }
    }

    /// CDR-encode a TestMsg(value) including CDR header.
    fn encode_test_msg(value: i32) -> ([u8; 256], usize) {
        let mut buf = [0u8; 256];
        let mut writer = CdrWriter::new_with_header(&mut buf).unwrap();
        writer.write_i32(value).unwrap();
        let len = writer.position();
        (buf, len)
    }

    /// Mock subscriber that can be loaded with canned CDR data.
    struct MockSubscriber {
        /// Pre-encoded data to return on the next `try_recv_raw` call.
        pending: Cell<Option<([u8; 256], usize)>>,
    }

    impl MockSubscriber {
        fn new() -> Self {
            Self {
                pending: Cell::new(None),
            }
        }

        fn load(&self, data: [u8; 256], len: usize) {
            self.pending.set(Some((data, len)));
        }
    }

    impl Subscriber for MockSubscriber {
        type Error = TransportError;

        fn try_recv_raw(&mut self, buf: &mut [u8]) -> Result<Option<usize>, TransportError> {
            match self.pending.get() {
                Some((data, len)) => {
                    buf[..len].copy_from_slice(&data[..len]);
                    self.pending.set(None);
                    Ok(Some(len))
                }
                None => Ok(None),
            }
        }

        fn deserialization_error(&self) -> TransportError {
            TransportError::DeserializationError
        }
    }

    /// Mock service server (not used for service tests yet, but needed for Session).
    struct MockServiceServer;

    impl ServiceServerTrait for MockServiceServer {
        type Error = TransportError;

        fn try_recv_request<'a>(
            &mut self,
            _buf: &'a mut [u8],
        ) -> Result<Option<ServiceRequest<'a>>, TransportError> {
            Ok(None)
        }

        fn send_reply(&mut self, _seq: i64, _data: &[u8]) -> Result<(), TransportError> {
            Ok(())
        }
    }

    /// Dummy publisher (never used in callback tests).
    struct MockPublisher;

    impl Publisher for MockPublisher {
        type Error = TransportError;

        fn publish_raw(&self, _data: &[u8]) -> Result<(), TransportError> {
            Ok(())
        }

        fn buffer_error(&self) -> TransportError {
            TransportError::BufferTooSmall
        }

        fn serialization_error(&self) -> TransportError {
            TransportError::SerializationError
        }
    }

    /// Dummy service client.
    struct MockServiceClient;

    impl ServiceClientTrait for MockServiceClient {
        type Error = TransportError;

        fn call_raw(
            &mut self,
            _req: &[u8],
            _reply_buf: &mut [u8],
        ) -> Result<usize, TransportError> {
            Err(TransportError::Timeout)
        }
    }

    /// Mock session that produces mock handles.
    struct MockSession;

    impl MockSession {
        fn new() -> Self {
            Self
        }
    }

    impl Session for MockSession {
        type Error = TransportError;
        type PublisherHandle = MockPublisher;
        type SubscriberHandle = MockSubscriber;
        type ServiceServerHandle = MockServiceServer;
        type ServiceClientHandle = MockServiceClient;

        fn create_publisher(
            &mut self,
            _topic: &TopicInfo,
            _qos: QosSettings,
        ) -> Result<MockPublisher, TransportError> {
            Ok(MockPublisher)
        }

        fn create_subscriber(
            &mut self,
            _topic: &TopicInfo,
            _qos: QosSettings,
        ) -> Result<MockSubscriber, TransportError> {
            Ok(MockSubscriber::new())
        }

        fn create_service_server(
            &mut self,
            _service: &ServiceInfo,
        ) -> Result<MockServiceServer, TransportError> {
            Ok(MockServiceServer)
        }

        fn create_service_client(
            &mut self,
            _service: &ServiceInfo,
        ) -> Result<MockServiceClient, TransportError> {
            Ok(MockServiceClient)
        }

        fn close(&mut self) -> Result<(), TransportError> {
            Ok(())
        }
    }

    // ====================================================================
    // Arena callback tests
    // ====================================================================

    #[test]
    fn test_add_subscription_and_spin_once_no_data() {
        let session = MockSession::new();
        let mut executor: EmbeddedExecutor<MockSession, 4, 4096> =
            EmbeddedExecutor::from_session(session);

        // Register a subscription — callback should never fire
        let called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let called2 = called.clone();
        executor
            .add_subscription::<TestMsg, _>("/test", move |_msg: &TestMsg| {
                called2.store(true, std::sync::atomic::Ordering::SeqCst);
            })
            .unwrap();

        let result = executor.spin_once(0);
        assert_eq!(result.subscriptions_processed, 0);
        assert!(!result.any_work());
        assert!(!called.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn test_add_subscription_and_spin_once_with_data() {
        let session = MockSession::new();
        let mut executor: EmbeddedExecutor<MockSession, 4, 4096> =
            EmbeddedExecutor::from_session(session);

        let received = std::sync::Arc::new(std::sync::Mutex::new(None));
        let received2 = received.clone();
        executor
            .add_subscription::<TestMsg, _>("/test", move |msg: &TestMsg| {
                *received2.lock().unwrap() = Some(msg.data);
            })
            .unwrap();

        // Grab a pointer to the subscriber in the arena so we can load data.
        // The subscriber is stored inside the SubEntry in the arena.
        // We need to reach it through the arena.
        let meta = executor.entries[0].as_ref().unwrap();
        let arena_ptr = executor.arena.as_ptr() as *const u8;
        let sub_ptr = unsafe { arena_ptr.add(meta.offset) } as *const MockSubscriber;

        // Load CDR-encoded TestMsg(42) into the subscriber
        let (data, len) = encode_test_msg(42);
        unsafe { &*sub_ptr }.load(data, len);

        let result = executor.spin_once(0);
        assert_eq!(result.subscriptions_processed, 1);
        assert!(result.any_work());
        assert_eq!(*received.lock().unwrap(), Some(42));
    }

    #[test]
    fn test_multiple_subscriptions() {
        let session = MockSession::new();
        let mut executor: EmbeddedExecutor<MockSession, 4, 8192> =
            EmbeddedExecutor::from_session(session);

        let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count1 = count.clone();
        let count2 = count.clone();

        executor
            .add_subscription::<TestMsg, _>("/topic1", move |_msg: &TestMsg| {
                count1.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            })
            .unwrap();

        executor
            .add_subscription::<TestMsg, _>("/topic2", move |_msg: &TestMsg| {
                count2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            })
            .unwrap();

        // Load data into both subscribers
        let (data, len) = encode_test_msg(10);
        let meta0 = executor.entries[0].as_ref().unwrap();
        let meta1 = executor.entries[1].as_ref().unwrap();
        let arena_ptr = executor.arena.as_ptr() as *const u8;
        unsafe { &*(arena_ptr.add(meta0.offset) as *const MockSubscriber) }.load(data, len);
        let (data2, len2) = encode_test_msg(20);
        unsafe { &*(arena_ptr.add(meta1.offset) as *const MockSubscriber) }.load(data2, len2);

        let result = executor.spin_once(0);
        assert_eq!(result.subscriptions_processed, 2);
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[test]
    fn test_arena_overflow() {
        let session = MockSession::new();
        // Tiny arena — one SubEntry<TestMsg, MockSubscriber, fn, 1024> is ~1040+ bytes
        let mut executor: EmbeddedExecutor<MockSession, 4, 128> =
            EmbeddedExecutor::from_session(session);

        let result = executor.add_subscription::<TestMsg, _>("/test", |_msg: &TestMsg| {});
        assert_eq!(result, Err(EmbeddedNodeError::BufferTooSmall));
    }

    #[test]
    fn test_entry_slots_exhausted() {
        let session = MockSession::new();
        // 1 entry slot but plenty of arena space
        let mut executor: EmbeddedExecutor<MockSession, 1, 8192> =
            EmbeddedExecutor::from_session(session);

        executor
            .add_subscription::<TestMsg, _>("/a", |_msg: &TestMsg| {})
            .unwrap();

        let result = executor.add_subscription::<TestMsg, _>("/b", |_msg: &TestMsg| {});
        assert_eq!(result, Err(EmbeddedNodeError::BufferTooSmall));
    }

    #[test]
    fn test_spin_once_result_counts() {
        let result = SpinOnceResult::new();
        assert!(!result.any_work());
        assert!(!result.any_errors());
        assert_eq!(result.total(), 0);
        assert_eq!(result.total_errors(), 0);

        let result = SpinOnceResult {
            subscriptions_processed: 2,
            timers_fired: 1,
            services_handled: 1,
            subscription_errors: 0,
            service_errors: 0,
        };
        assert!(result.any_work());
        assert!(!result.any_errors());
        assert_eq!(result.total(), 4);
    }

    #[test]
    fn test_drop_runs_without_panic() {
        let session = MockSession::new();
        let mut executor: EmbeddedExecutor<MockSession, 4, 4096> =
            EmbeddedExecutor::from_session(session);

        executor
            .add_subscription::<TestMsg, _>("/test", |_msg: &TestMsg| {})
            .unwrap();

        // executor drops here — Drop impl must not panic
    }

    #[test]
    fn test_zero_sized_executor_spin_once() {
        // Default const generics: MAX_CBS=0, CB_ARENA=0
        let session = MockSession::new();
        let mut executor: EmbeddedExecutor<MockSession, 0, 0> =
            EmbeddedExecutor::from_session(session);

        // spin_once with no entries just calls drive_io
        let result = executor.spin_once(0);
        assert!(!result.any_work());
    }

    #[test]
    fn test_arena_alignment() {
        let session = MockSession::new();
        let mut executor: EmbeddedExecutor<MockSession, 4, 8192> =
            EmbeddedExecutor::from_session(session);

        // Add a subscription, then check the offset is properly aligned
        executor
            .add_subscription::<TestMsg, _>("/test", |_msg: &TestMsg| {})
            .unwrap();

        let meta = executor.entries[0].as_ref().unwrap();
        let entry_align =
            core::mem::align_of::<SubEntry<TestMsg, MockSubscriber, fn(&TestMsg), 1024>>();
        assert_eq!(meta.offset % entry_align, 0);
    }

    // ====================================================================
    // Timer callback tests
    // ====================================================================

    #[test]
    fn test_add_timer_and_fire() {
        let session = MockSession::new();
        let mut executor: EmbeddedExecutor<MockSession, 4, 4096> =
            EmbeddedExecutor::from_session(session);

        let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count2 = count.clone();
        executor
            .add_timer(TimerDuration::from_millis(100), move || {
                count2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            })
            .unwrap();

        // Not enough time elapsed — should not fire
        let result = executor.spin_once(50);
        assert_eq!(result.timers_fired, 0);
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 0);

        // Now enough time elapsed (50 + 60 = 110 >= 100)
        let result = executor.spin_once(60);
        assert_eq!(result.timers_fired, 1);
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn test_timer_repeats() {
        let session = MockSession::new();
        let mut executor: EmbeddedExecutor<MockSession, 4, 4096> =
            EmbeddedExecutor::from_session(session);

        let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count2 = count.clone();
        executor
            .add_timer(TimerDuration::from_millis(100), move || {
                count2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            })
            .unwrap();

        // Fire 3 times
        let _ = executor.spin_once(100);
        let _ = executor.spin_once(100);
        let _ = executor.spin_once(100);
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 3);
    }

    #[test]
    fn test_timer_oneshot_fires_once() {
        let session = MockSession::new();
        let mut executor: EmbeddedExecutor<MockSession, 4, 4096> =
            EmbeddedExecutor::from_session(session);

        let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count2 = count.clone();
        executor
            .add_timer_oneshot(TimerDuration::from_millis(50), move || {
                count2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            })
            .unwrap();

        // First spin fires
        let result = executor.spin_once(60);
        assert_eq!(result.timers_fired, 1);
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);

        // Second spin should NOT fire again
        let result = executor.spin_once(60);
        assert_eq!(result.timers_fired, 0);
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn test_timer_does_not_fire_at_zero_delta() {
        let session = MockSession::new();
        let mut executor: EmbeddedExecutor<MockSession, 4, 4096> =
            EmbeddedExecutor::from_session(session);

        let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count2 = count.clone();
        executor
            .add_timer(TimerDuration::from_millis(100), move || {
                count2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            })
            .unwrap();

        // Zero delta should never fire
        let result = executor.spin_once(0);
        assert_eq!(result.timers_fired, 0);
    }

    #[test]
    fn test_timer_with_subscriptions() {
        let session = MockSession::new();
        let mut executor: EmbeddedExecutor<MockSession, 4, 8192> =
            EmbeddedExecutor::from_session(session);

        let timer_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let timer_count2 = timer_count.clone();
        executor
            .add_timer(TimerDuration::from_millis(100), move || {
                timer_count2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            })
            .unwrap();

        let sub_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let sub_count2 = sub_count.clone();
        executor
            .add_subscription::<TestMsg, _>("/test", move |_msg: &TestMsg| {
                sub_count2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            })
            .unwrap();

        // Load data into subscription
        let (data, len) = encode_test_msg(99);
        let meta1 = executor.entries[1].as_ref().unwrap();
        let arena_ptr = executor.arena.as_ptr() as *const u8;
        unsafe { &*(arena_ptr.add(meta1.offset) as *const MockSubscriber) }.load(data, len);

        let result = executor.spin_once(100);
        assert_eq!(result.timers_fired, 1);
        assert_eq!(result.subscriptions_processed, 1);
        assert_eq!(timer_count.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(sub_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    // ====================================================================
    // Action types for testing
    // ====================================================================

    #[derive(Debug, Clone, Default, PartialEq)]
    struct TestGoal {
        order: i32,
    }

    impl RosMessage for TestGoal {
        const TYPE_NAME: &'static str = "test/action/TestAction_Goal";
        const TYPE_HASH: &'static str = "test_hash";
    }

    impl Serialize for TestGoal {
        fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
            writer.write_i32(self.order)
        }
    }

    impl Deserialize for TestGoal {
        fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
            Ok(Self {
                order: reader.read_i32()?,
            })
        }
    }

    #[derive(Debug, Clone, Default, PartialEq)]
    struct TestResult {
        value: i32,
    }

    impl RosMessage for TestResult {
        const TYPE_NAME: &'static str = "test/action/TestAction_Result";
        const TYPE_HASH: &'static str = "test_hash";
    }

    impl Serialize for TestResult {
        fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
            writer.write_i32(self.value)
        }
    }

    impl Deserialize for TestResult {
        fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
            Ok(Self {
                value: reader.read_i32()?,
            })
        }
    }

    #[derive(Debug, Clone, Default, PartialEq)]
    struct TestFeedback {
        progress: i32,
    }

    impl RosMessage for TestFeedback {
        const TYPE_NAME: &'static str = "test/action/TestAction_Feedback";
        const TYPE_HASH: &'static str = "test_hash";
    }

    impl Serialize for TestFeedback {
        fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
            writer.write_i32(self.progress)
        }
    }

    impl Deserialize for TestFeedback {
        fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
            Ok(Self {
                progress: reader.read_i32()?,
            })
        }
    }

    struct TestAction;

    impl RosAction for TestAction {
        type Goal = TestGoal;
        type Result = TestResult;
        type Feedback = TestFeedback;
        const ACTION_NAME: &'static str = "test/action/dds_/TestAction_";
        const ACTION_HASH: &'static str = "test_hash";
    }

    // ====================================================================
    // Action server tests
    // ====================================================================

    #[test]
    fn test_add_action_server_registers() {
        let session = MockSession::new();
        // Action server arena entry is large — give plenty of space
        let mut executor: EmbeddedExecutor<MockSession, 4, 16384> =
            EmbeddedExecutor::from_session(session);

        let handle = executor
            .add_action_server::<TestAction, _, _>(
                "/test_action",
                |_goal: &TestGoal| nros_core::GoalResponse::AcceptAndExecute,
                |_id: &nros_core::GoalId, _status: nros_core::GoalStatus| {
                    nros_core::CancelResponse::Ok
                },
            )
            .unwrap();

        // Verify the entry was registered
        assert!(executor.entries[0].is_some());
        assert_eq!(handle.entry_index, 0);
    }

    #[test]
    fn test_action_server_spin_once_no_requests() {
        let session = MockSession::new();
        let mut executor: EmbeddedExecutor<MockSession, 4, 16384> =
            EmbeddedExecutor::from_session(session);

        let _handle = executor
            .add_action_server::<TestAction, _, _>(
                "/test_action",
                |_goal: &TestGoal| nros_core::GoalResponse::AcceptAndExecute,
                |_id: &nros_core::GoalId, _status: nros_core::GoalStatus| {
                    nros_core::CancelResponse::Ok
                },
            )
            .unwrap();

        // With no pending requests, spin_once should return no work
        let result = executor.spin_once(10);
        assert_eq!(result.services_handled, 0);
        assert!(!result.any_work());
    }

    // ====================================================================
    // Action client tests
    // ====================================================================

    #[test]
    fn test_add_action_client_registers() {
        let session = MockSession::new();
        let mut executor: EmbeddedExecutor<MockSession, 4, 16384> =
            EmbeddedExecutor::from_session(session);

        let handle = executor
            .add_action_client::<TestAction, _>(
                "/test_action",
                |_id: &nros_core::GoalId, _feedback: &TestFeedback| {},
            )
            .unwrap();

        assert!(executor.entries[0].is_some());
        assert_eq!(handle.entry_index, 0);
    }

    #[test]
    fn test_action_client_spin_once_no_feedback() {
        let session = MockSession::new();
        let mut executor: EmbeddedExecutor<MockSession, 4, 16384> =
            EmbeddedExecutor::from_session(session);

        let _handle = executor
            .add_action_client::<TestAction, _>(
                "/test_action",
                |_id: &nros_core::GoalId, _feedback: &TestFeedback| {},
            )
            .unwrap();

        let result = executor.spin_once(10);
        assert_eq!(result.subscriptions_processed, 0);
        assert!(!result.any_work());
    }

    #[test]
    fn test_action_server_and_client_coexist() {
        let session = MockSession::new();
        let mut executor: EmbeddedExecutor<MockSession, 8, 65536> =
            EmbeddedExecutor::from_session(session);

        let _server_handle = executor
            .add_action_server::<TestAction, _, _>(
                "/test_action",
                |_goal: &TestGoal| nros_core::GoalResponse::AcceptAndExecute,
                |_id: &nros_core::GoalId, _status: nros_core::GoalStatus| {
                    nros_core::CancelResponse::Ok
                },
            )
            .unwrap();

        let _client_handle = executor
            .add_action_client::<TestAction, _>(
                "/test_action",
                |_id: &nros_core::GoalId, _feedback: &TestFeedback| {},
            )
            .unwrap();

        // Both registered
        assert!(executor.entries[0].is_some());
        assert!(executor.entries[1].is_some());

        let result = executor.spin_once(10);
        assert!(!result.any_work());
    }

    #[test]
    fn test_drop_with_mixed_entries() {
        let session = MockSession::new();
        let mut executor: EmbeddedExecutor<MockSession, 8, 65536> =
            EmbeddedExecutor::from_session(session);

        // Register one of each kind
        executor
            .add_subscription::<TestMsg, _>("/sub", |_msg: &TestMsg| {})
            .unwrap();
        executor
            .add_timer(TimerDuration::from_millis(100), || {})
            .unwrap();
        let _server = executor
            .add_action_server::<TestAction, _, _>(
                "/act",
                |_goal: &TestGoal| nros_core::GoalResponse::AcceptAndExecute,
                |_id: &nros_core::GoalId, _status: nros_core::GoalStatus| {
                    nros_core::CancelResponse::Ok
                },
            )
            .unwrap();
        let _client = executor
            .add_action_client::<TestAction, _>(
                "/act",
                |_id: &nros_core::GoalId, _fb: &TestFeedback| {},
            )
            .unwrap();

        // Drop must clean up all 4 entries without panicking
    }
}

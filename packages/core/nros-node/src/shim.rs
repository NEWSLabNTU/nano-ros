//! Shim-based executor and node for embedded platforms
//!
//! This module provides a simplified executor and node API using nros-rmw-zenoh.
//! It's designed for embedded platforms that need manual polling without
//! background threads.
//!
//! # Differences from zenoh-based API
//!
//! - No liveliness tokens (no ROS 2 discovery)
//! - No RMW attachments
//! - Simpler, no_std friendly API
//!
//! # Example
//!
//! ```ignore
//! use nros_node::shim::{ShimExecutor, ShimNode};
//! use std_msgs::msg::Int32;
//!
//! // Create executor with locator
//! let mut executor = ShimExecutor::new(b"tcp/192.168.1.1:7447\0")?;
//!
//! // Create node
//! let node = executor.create_node("my_node")?;
//!
//! // Create publisher
//! let publisher = node.create_publisher::<Int32>("/chatter")?;
//!
//! // In your main loop or RTIC task:
//! loop {
//!     // Poll network and dispatch callbacks
//!     executor.spin_once(10)?;
//!
//!     // Publish periodically
//!     publisher.publish(&Int32 { data: 42 })?;
//!
//!     // platform delay...
//! }
//! ```

use core::marker::PhantomData;

use heapless::String;
use nros_core::{CdrReader, CdrWriter, Deserialize, RosAction, RosMessage, RosService, Serialize};
use nros_rmw::{
    ActionInfo, Publisher, QosSettings, ServiceClientTrait, ServiceInfo, ServiceServerTrait,
    Session, Subscriber, TopicInfo, Transport, TransportConfig, TransportError,
};
use nros_rmw_zenoh::{
    ShimPublisher, ShimServiceClient, ShimServiceServer, ShimSession, ShimSubscriber, ShimTransport,
};

// ============================================================================
// Error Types
// ============================================================================

/// Error type for shim operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShimNodeError {
    /// Transport error
    Transport(TransportError),
    /// Node name too long
    NameTooLong,
    /// Topic name too long
    TopicTooLong,
    /// Maximum publishers reached
    TooManyPublishers,
    /// Maximum subscribers reached
    TooManySubscribers,
    /// Serialization error
    Serialization,
    /// Buffer too small
    BufferTooSmall,
    /// Service request failed
    ServiceRequestFailed,
    /// Service reply failed
    ServiceReplyFailed,
    /// Action server creation failed
    ActionServerCreationFailed,
    /// Action client creation failed
    ActionClientCreationFailed,
}

impl From<TransportError> for ShimNodeError {
    fn from(err: TransportError) -> Self {
        ShimNodeError::Transport(err)
    }
}

// ============================================================================
// ShimExecutor
// ============================================================================

/// Polling-based executor for embedded platforms
///
/// This executor manages a single zenoh session and provides manual polling
/// control. Unlike the full executor, it does not manage multiple nodes
/// internally - you get one node at a time.
///
/// # Type Parameters
///
/// * `MAX_PUBLISHERS` - Maximum number of publishers (default: 8)
/// * `MAX_SUBSCRIBERS` - Maximum number of subscribers (default: 8)
pub struct ShimExecutor {
    session: ShimSession,
}

impl ShimExecutor {
    /// Create a new executor with the given locator
    ///
    /// # Arguments
    ///
    /// * `locator` - Null-terminated connection string (e.g., `b"tcp/192.168.1.1:7447\0"`)
    ///
    /// # Example
    ///
    /// ```ignore
    /// let executor = ShimExecutor::new(b"tcp/192.168.1.1:7447\0")?;
    /// ```
    pub fn new(locator: &[u8]) -> Result<Self, ShimNodeError> {
        // Convert locator to str (removing null terminator for config)
        let locator_str = core::str::from_utf8(locator)
            .map_err(|_| ShimNodeError::Transport(TransportError::InvalidConfig))?
            .trim_end_matches('\0');

        let config = TransportConfig {
            locator: Some(locator_str),
            mode: nros_rmw::SessionMode::Client,
            properties: &[],
        };

        let session = ShimTransport::open(&config)?;

        Ok(Self { session })
    }

    /// Create a new executor with custom transport configuration
    ///
    /// # Arguments
    ///
    /// * `config` - Transport configuration
    pub fn with_config(config: &TransportConfig) -> Result<Self, ShimNodeError> {
        let session = ShimTransport::open(config)?;
        Ok(Self { session })
    }

    /// Create a node on this executor
    ///
    /// # Arguments
    ///
    /// * `name` - Node name
    ///
    /// # Returns
    ///
    /// A new node that can create publishers and subscribers
    pub fn create_node(&mut self, name: &str) -> Result<ShimNode<'_>, ShimNodeError> {
        if name.len() > 64 {
            return Err(ShimNodeError::NameTooLong);
        }

        let mut node_name = String::<64>::new();
        node_name
            .push_str(name)
            .map_err(|_| ShimNodeError::NameTooLong)?;

        Ok(ShimNode {
            name: node_name,
            session: &mut self.session,
            domain_id: 0,
        })
    }

    /// Poll for incoming data and process callbacks
    ///
    /// Call this periodically (recommended: every 10ms) to process network
    /// data and dispatch subscriber callbacks.
    ///
    /// # Arguments
    ///
    /// * `timeout_ms` - Maximum time to wait for data (0 = non-blocking)
    ///
    /// # Returns
    ///
    /// Number of events processed, or error
    pub fn spin_once(&self, timeout_ms: u32) -> Result<i32, ShimNodeError> {
        self.session.spin_once(timeout_ms).map_err(|e| e.into())
    }

    /// Poll for incoming data without keepalive
    ///
    /// Use `spin_once` instead unless you need separate control over
    /// polling and keepalive.
    pub fn poll(&self, timeout_ms: u32) -> Result<i32, ShimNodeError> {
        self.session.poll(timeout_ms).map_err(|e| e.into())
    }

    /// Check if the session is open
    pub fn is_open(&self) -> bool {
        self.session.is_open()
    }

    /// Get a reference to the underlying session
    pub fn session(&self) -> &ShimSession {
        &self.session
    }

    /// Get a mutable reference to the underlying session
    pub fn session_mut(&mut self) -> &mut ShimSession {
        &mut self.session
    }
}

// ============================================================================
// ShimNode
// ============================================================================

/// Node for creating publishers and subscribers
///
/// A node is created through `ShimExecutor::create_node()` and provides
/// methods to create publishers and subscribers for topics.
pub struct ShimNode<'a> {
    name: String<64>,
    session: &'a mut ShimSession,
    domain_id: u32,
}

impl<'a> ShimNode<'a> {
    /// Get the node name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the domain ID
    pub fn domain_id(&self) -> u32 {
        self.domain_id
    }

    /// Set the domain ID
    pub fn set_domain_id(&mut self, domain_id: u32) {
        self.domain_id = domain_id;
    }

    /// Create a publisher for the given topic
    ///
    /// # Arguments
    ///
    /// * `topic_name` - Topic name (e.g., "/chatter")
    ///
    /// # Type Parameters
    ///
    /// * `M` - Message type implementing RosMessage
    ///
    /// # Example
    ///
    /// ```ignore
    /// let publisher = node.create_publisher::<Int32>("/chatter")?;
    /// publisher.publish(&Int32 { data: 42 })?;
    /// ```
    pub fn create_publisher<M: RosMessage>(
        &mut self,
        topic_name: &str,
    ) -> Result<ShimNodePublisher<M>, ShimNodeError> {
        self.create_publisher_with_qos::<M>(topic_name, QosSettings::default())
    }

    /// Create a publisher with custom QoS settings
    pub fn create_publisher_with_qos<M: RosMessage>(
        &mut self,
        topic_name: &str,
        qos: QosSettings,
    ) -> Result<ShimNodePublisher<M>, ShimNodeError> {
        let topic =
            TopicInfo::new(topic_name, M::TYPE_NAME, M::TYPE_HASH).with_domain(self.domain_id);

        let publisher = self.session.create_publisher(&topic, qos)?;

        Ok(ShimNodePublisher {
            publisher,
            _phantom: PhantomData,
        })
    }

    /// Create a subscription for the given topic
    ///
    /// # Arguments
    ///
    /// * `topic_name` - Topic name (e.g., "/chatter")
    ///
    /// # Type Parameters
    ///
    /// * `M` - Message type implementing RosMessage
    /// * `RX_BUF` - Receive buffer size (default: 1024)
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut subscription = node.create_subscription::<Int32>("/chatter")?;
    ///
    /// // In your polling loop:
    /// if let Some(msg) = subscription.try_recv()? {
    ///     // process message
    /// }
    /// ```
    pub fn create_subscription<M: RosMessage>(
        &mut self,
        topic_name: &str,
    ) -> Result<ShimNodeSubscription<M, 1024>, ShimNodeError> {
        self.create_subscription_sized::<M, 1024>(topic_name)
    }

    /// Create a subscription with custom buffer size
    pub fn create_subscription_sized<M: RosMessage, const RX_BUF: usize>(
        &mut self,
        topic_name: &str,
    ) -> Result<ShimNodeSubscription<M, RX_BUF>, ShimNodeError> {
        self.create_subscription_with_qos::<M, RX_BUF>(topic_name, QosSettings::default())
    }

    /// Create a subscription with custom QoS settings and buffer size
    pub fn create_subscription_with_qos<M: RosMessage, const RX_BUF: usize>(
        &mut self,
        topic_name: &str,
        qos: QosSettings,
    ) -> Result<ShimNodeSubscription<M, RX_BUF>, ShimNodeError> {
        let topic =
            TopicInfo::new(topic_name, M::TYPE_NAME, M::TYPE_HASH).with_domain(self.domain_id);

        let subscriber = self.session.create_subscriber(&topic, qos)?;

        Ok(ShimNodeSubscription {
            subscriber,
            buffer: [0u8; RX_BUF],
            _phantom: PhantomData,
        })
    }

    /// Create a service server for the given service
    ///
    /// # Type Parameters
    ///
    /// * `S` - Service type implementing RosService
    pub fn create_service<S: RosService>(
        &mut self,
        service_name: &str,
    ) -> Result<ShimNodeServiceServer<S>, ShimNodeError> {
        self.create_service_sized::<S, 1024, 1024>(service_name)
    }

    /// Create a service server with custom buffer sizes
    pub fn create_service_sized<S: RosService, const REQ_BUF: usize, const REPLY_BUF: usize>(
        &mut self,
        service_name: &str,
    ) -> Result<ShimNodeServiceServer<S, REQ_BUF, REPLY_BUF>, ShimNodeError> {
        let service_info = ServiceInfo::new(service_name, S::SERVICE_NAME, S::SERVICE_HASH)
            .with_domain(self.domain_id);

        let server = self.session.create_service_server(&service_info)?;

        Ok(ShimNodeServiceServer {
            server,
            req_buffer: [0u8; REQ_BUF],
            reply_buffer: [0u8; REPLY_BUF],
            _phantom: PhantomData,
        })
    }

    /// Create a service client for the given service
    ///
    /// # Type Parameters
    ///
    /// * `S` - Service type implementing RosService
    pub fn create_client<S: RosService>(
        &mut self,
        service_name: &str,
    ) -> Result<ShimNodeServiceClient<S>, ShimNodeError> {
        self.create_client_sized::<S, 1024, 1024>(service_name)
    }

    /// Create a service client with custom buffer sizes
    pub fn create_client_sized<S: RosService, const REQ_BUF: usize, const REPLY_BUF: usize>(
        &mut self,
        service_name: &str,
    ) -> Result<ShimNodeServiceClient<S, REQ_BUF, REPLY_BUF>, ShimNodeError> {
        let service_info = ServiceInfo::new(service_name, S::SERVICE_NAME, S::SERVICE_HASH)
            .with_domain(self.domain_id);

        let client = self.session.create_service_client(&service_info)?;

        Ok(ShimNodeServiceClient {
            client,
            req_buffer: [0u8; REQ_BUF],
            reply_buffer: [0u8; REPLY_BUF],
            _phantom: PhantomData,
        })
    }

    /// Create an action server for the given action
    ///
    /// # Type Parameters
    ///
    /// * `A` - Action type implementing RosAction
    pub fn create_action_server<A: RosAction>(
        &mut self,
        action_name: &str,
    ) -> Result<ShimNodeActionServer<A>, ShimNodeError> {
        self.create_action_server_sized::<A, 1024, 1024, 1024, 4>(action_name)
    }

    /// Create an action server with custom buffer sizes
    pub fn create_action_server_sized<
        A: RosAction,
        const GOAL_BUF: usize,
        const RESULT_BUF: usize,
        const FEEDBACK_BUF: usize,
        const MAX_GOALS: usize,
    >(
        &mut self,
        action_name: &str,
    ) -> Result<ShimNodeActionServer<A, GOAL_BUF, RESULT_BUF, FEEDBACK_BUF, MAX_GOALS>, ShimNodeError>
    {
        let action_info = ActionInfo::new(action_name, A::ACTION_NAME, A::ACTION_HASH)
            .with_domain(self.domain_id);

        // Create send_goal service server
        let send_goal_keyexpr: heapless::String<256> = action_info.send_goal_key();
        let send_goal_info =
            ServiceInfo::new(&send_goal_keyexpr, A::ACTION_NAME, A::ACTION_HASH).with_domain(0);
        let send_goal_server = self
            .session
            .create_service_server(&send_goal_info)
            .map_err(|_| ShimNodeError::ActionServerCreationFailed)?;

        // Create cancel_goal service server
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
            .map_err(|_| ShimNodeError::ActionServerCreationFailed)?;

        // Create get_result service server
        let get_result_keyexpr: heapless::String<256> = action_info.get_result_key();
        let get_result_info =
            ServiceInfo::new(&get_result_keyexpr, A::ACTION_NAME, A::ACTION_HASH).with_domain(0);
        let get_result_server = self
            .session
            .create_service_server(&get_result_info)
            .map_err(|_| ShimNodeError::ActionServerCreationFailed)?;

        // Create feedback publisher
        let feedback_keyexpr: heapless::String<256> = action_info.feedback_key();
        let feedback_topic =
            TopicInfo::new(&feedback_keyexpr, A::ACTION_NAME, A::ACTION_HASH).with_domain(0);
        let feedback_publisher = self
            .session
            .create_publisher(&feedback_topic, QosSettings::BEST_EFFORT)
            .map_err(|_| ShimNodeError::ActionServerCreationFailed)?;

        // Create status publisher
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
            .map_err(|_| ShimNodeError::ActionServerCreationFailed)?;

        Ok(ShimNodeActionServer {
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

    /// Create an action client for the given action
    ///
    /// # Type Parameters
    ///
    /// * `A` - Action type implementing RosAction
    pub fn create_action_client<A: RosAction>(
        &mut self,
        action_name: &str,
    ) -> Result<ShimNodeActionClient<A>, ShimNodeError> {
        self.create_action_client_sized::<A, 1024, 1024, 1024>(action_name)
    }

    /// Create an action client with custom buffer sizes
    pub fn create_action_client_sized<
        A: RosAction,
        const GOAL_BUF: usize,
        const RESULT_BUF: usize,
        const FEEDBACK_BUF: usize,
    >(
        &mut self,
        action_name: &str,
    ) -> Result<ShimNodeActionClient<A, GOAL_BUF, RESULT_BUF, FEEDBACK_BUF>, ShimNodeError> {
        let action_info = ActionInfo::new(action_name, A::ACTION_NAME, A::ACTION_HASH)
            .with_domain(self.domain_id);

        // Create send_goal service client
        let send_goal_keyexpr: heapless::String<256> = action_info.send_goal_key();
        let send_goal_info =
            ServiceInfo::new(&send_goal_keyexpr, A::ACTION_NAME, A::ACTION_HASH).with_domain(0);
        let send_goal_client = self
            .session
            .create_service_client(&send_goal_info)
            .map_err(|_| ShimNodeError::ActionClientCreationFailed)?;

        // Create cancel_goal service client
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
            .map_err(|_| ShimNodeError::ActionClientCreationFailed)?;

        // Create get_result service client
        let get_result_keyexpr: heapless::String<256> = action_info.get_result_key();
        let get_result_info =
            ServiceInfo::new(&get_result_keyexpr, A::ACTION_NAME, A::ACTION_HASH).with_domain(0);
        let get_result_client = self
            .session
            .create_service_client(&get_result_info)
            .map_err(|_| ShimNodeError::ActionClientCreationFailed)?;

        // Create feedback subscriber
        let feedback_keyexpr: heapless::String<256> = action_info.feedback_key();
        let feedback_topic =
            TopicInfo::new(&feedback_keyexpr, A::ACTION_NAME, A::ACTION_HASH).with_domain(0);
        let feedback_subscriber = self
            .session
            .create_subscriber(&feedback_topic, QosSettings::BEST_EFFORT)
            .map_err(|_| ShimNodeError::ActionClientCreationFailed)?;

        Ok(ShimNodeActionClient {
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
// ShimNodePublisher
// ============================================================================

/// Publisher handle for a typed message
///
/// Created via `ShimNode::create_publisher()`.
pub struct ShimNodePublisher<M: RosMessage> {
    publisher: ShimPublisher,
    _phantom: PhantomData<M>,
}

impl<M: RosMessage> ShimNodePublisher<M> {
    /// Publish a message
    ///
    /// # Arguments
    ///
    /// * `msg` - Message to publish
    ///
    /// # Returns
    ///
    /// Ok(()) on success, error on failure
    pub fn publish(&self, msg: &M) -> Result<(), ShimNodeError> {
        self.publish_with_buffer::<1024>(msg)
    }

    /// Publish a message with custom buffer size
    pub fn publish_with_buffer<const BUF: usize>(&self, msg: &M) -> Result<(), ShimNodeError> {
        let mut buffer = [0u8; BUF];
        let mut writer =
            CdrWriter::new_with_header(&mut buffer).map_err(|_| ShimNodeError::BufferTooSmall)?;

        msg.serialize(&mut writer)
            .map_err(|_| ShimNodeError::Serialization)?;

        let len = writer.position();
        self.publisher
            .publish_raw(&buffer[..len])
            .map_err(|e| e.into())
    }

    /// Publish raw CDR-encoded data
    ///
    /// The data should already include the CDR header.
    pub fn publish_raw(&self, data: &[u8]) -> Result<(), ShimNodeError> {
        self.publisher.publish_raw(data).map_err(|e| e.into())
    }
}

// ============================================================================
// ShimNodeSubscription
// ============================================================================

/// Subscription handle for a typed message
///
/// Created via `ShimNode::create_subscription()`.
pub struct ShimNodeSubscription<M: RosMessage, const RX_BUF: usize = 1024> {
    subscriber: ShimSubscriber,
    buffer: [u8; RX_BUF],
    _phantom: PhantomData<M>,
}

impl<M: RosMessage, const RX_BUF: usize> ShimNodeSubscription<M, RX_BUF> {
    /// Try to receive a message (non-blocking)
    ///
    /// # Returns
    ///
    /// - `Ok(Some(msg))` if a message is available
    /// - `Ok(None)` if no message is available
    /// - `Err(...)` on error
    pub fn try_recv(&mut self) -> Result<Option<M>, ShimNodeError> {
        use nros_core::CdrReader;

        match self.subscriber.try_recv_raw(&mut self.buffer)? {
            Some(len) => {
                let mut reader = CdrReader::new_with_header(&self.buffer[..len])
                    .map_err(|_| ShimNodeError::Transport(TransportError::DeserializationError))?;

                let msg = M::deserialize(&mut reader)
                    .map_err(|_| ShimNodeError::Transport(TransportError::DeserializationError))?;

                Ok(Some(msg))
            }
            None => Ok(None),
        }
    }

    /// Try to receive raw CDR-encoded data (non-blocking)
    ///
    /// # Returns
    ///
    /// - `Ok(Some(len))` if data is available, with `len` bytes in the buffer
    /// - `Ok(None)` if no data is available
    /// - `Err(...)` on error
    pub fn try_recv_raw(&mut self) -> Result<Option<usize>, ShimNodeError> {
        self.subscriber
            .try_recv_raw(&mut self.buffer)
            .map_err(|e| e.into())
    }

    /// Get the receive buffer
    ///
    /// Use this after `try_recv_raw()` to access the raw data.
    pub fn buffer(&self) -> &[u8] {
        &self.buffer
    }
}

// ============================================================================
// ShimNodeServiceServer
// ============================================================================

/// Service server handle for a typed service
///
/// Created via `ShimNode::create_service()`.
pub struct ShimNodeServiceServer<
    S: RosService,
    const REQ_BUF: usize = 1024,
    const REPLY_BUF: usize = 1024,
> {
    server: ShimServiceServer,
    req_buffer: [u8; REQ_BUF],
    reply_buffer: [u8; REPLY_BUF],
    _phantom: PhantomData<S>,
}

impl<S: RosService, const REQ_BUF: usize, const REPLY_BUF: usize>
    ShimNodeServiceServer<S, REQ_BUF, REPLY_BUF>
{
    /// Handle an incoming service request
    ///
    /// Calls the handler with the deserialized request and sends back the
    /// serialized reply. Returns `Ok(true)` if a request was handled,
    /// `Ok(false)` if no request was available.
    pub fn handle_request(
        &mut self,
        handler: impl FnOnce(&S::Request) -> S::Reply,
    ) -> Result<bool, ShimNodeError> {
        self.server
            .handle_request::<S>(&mut self.req_buffer, &mut self.reply_buffer, handler)
            .map_err(ShimNodeError::Transport)
    }

    /// Check if a request is available
    pub fn has_request(&self) -> bool {
        self.server.has_request()
    }
}

// ============================================================================
// ShimNodeServiceClient
// ============================================================================

/// Service client handle for a typed service
///
/// Created via `ShimNode::create_client()`.
pub struct ShimNodeServiceClient<
    S: RosService,
    const REQ_BUF: usize = 1024,
    const REPLY_BUF: usize = 1024,
> {
    client: ShimServiceClient,
    req_buffer: [u8; REQ_BUF],
    reply_buffer: [u8; REPLY_BUF],
    _phantom: PhantomData<S>,
}

impl<S: RosService, const REQ_BUF: usize, const REPLY_BUF: usize>
    ShimNodeServiceClient<S, REQ_BUF, REPLY_BUF>
{
    /// Call the service with a typed request and wait for reply
    pub fn call(&mut self, request: &S::Request) -> Result<S::Reply, ShimNodeError> {
        self.client
            .call::<S>(request, &mut self.req_buffer, &mut self.reply_buffer)
            .map_err(ShimNodeError::Transport)
    }

    /// Set the timeout for service calls
    pub fn set_timeout(&mut self, timeout_ms: u32) {
        self.client.set_timeout(timeout_ms);
    }
}

// ============================================================================
// ShimNodeActionServer
// ============================================================================

/// Active goal tracking for action server
#[derive(Clone)]
pub struct ShimActiveGoal<A: RosAction> {
    /// Goal ID
    pub goal_id: nros_core::GoalId,
    /// Current status
    pub status: nros_core::GoalStatus,
    /// The goal data
    pub goal: A::Goal,
}

/// Completed goal with result
pub struct ShimCompletedGoal<A: RosAction> {
    /// Goal ID
    pub goal_id: nros_core::GoalId,
    /// Final status
    pub status: nros_core::GoalStatus,
    /// The result data
    pub result: A::Result,
}

/// Action server handle for a typed action
///
/// Created via `ShimNode::create_action_server()`.
pub struct ShimNodeActionServer<
    A: RosAction,
    const GOAL_BUF: usize = 1024,
    const RESULT_BUF: usize = 1024,
    const FEEDBACK_BUF: usize = 1024,
    const MAX_GOALS: usize = 4,
> {
    send_goal_server: ShimServiceServer,
    cancel_goal_server: ShimServiceServer,
    get_result_server: ShimServiceServer,
    feedback_publisher: ShimPublisher,
    _status_publisher: ShimPublisher,
    active_goals: heapless::Vec<ShimActiveGoal<A>, MAX_GOALS>,
    completed_goals: heapless::Vec<ShimCompletedGoal<A>, MAX_GOALS>,
    goal_buffer: [u8; GOAL_BUF],
    result_buffer: [u8; RESULT_BUF],
    feedback_buffer: [u8; FEEDBACK_BUF],
    cancel_buffer: [u8; 256],
}

impl<
    A: RosAction,
    const GOAL_BUF: usize,
    const RESULT_BUF: usize,
    const FEEDBACK_BUF: usize,
    const MAX_GOALS: usize,
> ShimNodeActionServer<A, GOAL_BUF, RESULT_BUF, FEEDBACK_BUF, MAX_GOALS>
{
    /// Try to accept a new goal
    ///
    /// Checks for incoming send_goal requests. If one is available, calls the
    /// handler to decide acceptance. Returns the goal ID if accepted.
    pub fn try_accept_goal(
        &mut self,
        goal_handler: impl FnOnce(&A::Goal) -> nros_core::GoalResponse,
    ) -> Result<Option<nros_core::GoalId>, ShimNodeError>
    where
        A::Goal: Clone,
    {
        // Try to receive a send_goal request
        let request = self
            .send_goal_server
            .try_recv_request(&mut self.goal_buffer)
            .map_err(ShimNodeError::Transport)?;

        let request = match request {
            Some(r) => r,
            None => return Ok(None),
        };

        let data_len = request.data.len();
        let sequence_number = request.sequence_number;
        #[allow(clippy::drop_non_drop)] // ends borrow on self.goal_buffer
        drop(request);

        // Deserialize: goal_id (UUID) + goal
        let mut reader = CdrReader::new_with_header(&self.goal_buffer[..data_len])
            .map_err(|_| ShimNodeError::Transport(TransportError::DeserializationError))?;

        // Read goal_id (UUID as sequence of uint8)
        let uuid_len = reader
            .read_u32()
            .map_err(|_| ShimNodeError::Transport(TransportError::DeserializationError))?
            as usize;
        let mut goal_id = nros_core::GoalId::default();
        if uuid_len == 16 {
            for byte in &mut goal_id.uuid {
                *byte = reader
                    .read_u8()
                    .map_err(|_| ShimNodeError::Transport(TransportError::DeserializationError))?;
            }
        }

        // Read goal
        let goal = A::Goal::deserialize(&mut reader)
            .map_err(|_| ShimNodeError::Transport(TransportError::DeserializationError))?;

        // Call handler
        let response = goal_handler(&goal);
        let accepted = response.is_accepted();

        // Serialize response: accepted (bool) + stamp (Time)
        let mut writer = CdrWriter::new_with_header(&mut self.result_buffer)
            .map_err(|_| ShimNodeError::BufferTooSmall)?;
        writer
            .write_u8(if accepted { 1 } else { 0 })
            .map_err(|_| ShimNodeError::Serialization)?;
        // stamp: sec=0, nanosec=0
        writer
            .write_i32(0)
            .map_err(|_| ShimNodeError::Serialization)?;
        writer
            .write_u32(0)
            .map_err(|_| ShimNodeError::Serialization)?;
        let reply_len = writer.position();

        self.send_goal_server
            .send_reply(sequence_number, &self.result_buffer[..reply_len])
            .map_err(ShimNodeError::Transport)?;

        if accepted {
            let _ = self.active_goals.push(ShimActiveGoal {
                goal_id,
                status: nros_core::GoalStatus::Accepted,
                goal,
            });
            Ok(Some(goal_id))
        } else {
            Ok(None)
        }
    }

    /// Publish feedback for a goal
    pub fn publish_feedback(
        &mut self,
        goal_id: &nros_core::GoalId,
        feedback: &A::Feedback,
    ) -> Result<(), ShimNodeError> {
        use nros_core::CdrWriter;

        let mut writer = CdrWriter::new_with_header(&mut self.feedback_buffer)
            .map_err(|_| ShimNodeError::BufferTooSmall)?;

        // Write goal_id (UUID as sequence)
        writer
            .write_u32(16)
            .map_err(|_| ShimNodeError::Serialization)?;
        for b in &goal_id.uuid {
            writer
                .write_u8(*b)
                .map_err(|_| ShimNodeError::Serialization)?;
        }

        // Write feedback
        feedback
            .serialize(&mut writer)
            .map_err(|_| ShimNodeError::Serialization)?;

        let len = writer.position();
        self.feedback_publisher
            .publish_raw(&self.feedback_buffer[..len])
            .map_err(ShimNodeError::Transport)
    }

    /// Set a goal's status
    pub fn set_goal_status(&mut self, goal_id: &nros_core::GoalId, status: nros_core::GoalStatus) {
        for goal in &mut self.active_goals {
            if goal.goal_id.uuid == goal_id.uuid {
                goal.status = status;
                break;
            }
        }
    }

    /// Complete a goal and store the result
    pub fn complete_goal(
        &mut self,
        goal_id: &nros_core::GoalId,
        status: nros_core::GoalStatus,
        result: A::Result,
    ) {
        // Remove from active goals
        if let Some(pos) = self
            .active_goals
            .iter()
            .position(|g| g.goal_id.uuid == goal_id.uuid)
        {
            self.active_goals.swap_remove(pos);
        }

        // Store in completed goals
        let _ = self.completed_goals.push(ShimCompletedGoal {
            goal_id: *goal_id,
            status,
            result,
        });
    }

    /// Try to handle a cancel_goal request
    ///
    /// Returns the goal ID and cancel response if a request was handled.
    pub fn try_handle_cancel(
        &mut self,
        cancel_handler: impl FnOnce(
            &nros_core::GoalId,
            nros_core::GoalStatus,
        ) -> nros_core::CancelResponse,
    ) -> Result<Option<(nros_core::GoalId, nros_core::CancelResponse)>, ShimNodeError> {
        use nros_core::{CancelResponse, CdrReader, CdrWriter};

        let request = self
            .cancel_goal_server
            .try_recv_request(&mut self.cancel_buffer)
            .map_err(ShimNodeError::Transport)?;

        let request = match request {
            Some(r) => r,
            None => return Ok(None),
        };

        let data_len = request.data.len();
        let sequence_number = request.sequence_number;
        #[allow(clippy::drop_non_drop)] // ends borrow on self.cancel_buffer
        drop(request);

        // Deserialize goal_id from cancel request
        let mut reader = CdrReader::new_with_header(&self.cancel_buffer[..data_len])
            .map_err(|_| ShimNodeError::Transport(TransportError::DeserializationError))?;

        let mut goal_id = nros_core::GoalId::default();
        let uuid_len = reader.read_u32().unwrap_or(0) as usize;
        if uuid_len == 16 {
            for byte in &mut goal_id.uuid {
                *byte = reader.read_u8().unwrap_or(0);
            }
        }

        // Find the goal's current status
        let current_status = self
            .active_goals
            .iter()
            .find(|g| g.goal_id.uuid == goal_id.uuid)
            .map(|g| g.status)
            .unwrap_or(nros_core::GoalStatus::Unknown);

        let response = cancel_handler(&goal_id, current_status);

        // If accepted, update goal status
        if response == CancelResponse::Ok {
            self.set_goal_status(&goal_id, nros_core::GoalStatus::Canceling);
        }

        // Serialize response: return_code (i8) + goals_canceling (sequence of GoalInfo)
        let mut writer = CdrWriter::new_with_header(&mut self.goal_buffer)
            .map_err(|_| ShimNodeError::BufferTooSmall)?;
        writer
            .write_i8(response as i8)
            .map_err(|_| ShimNodeError::Serialization)?;

        // goals_canceling sequence
        let num_canceling = if response == CancelResponse::Ok {
            1u32
        } else {
            0u32
        };
        writer
            .write_u32(num_canceling)
            .map_err(|_| ShimNodeError::Serialization)?;
        if response == CancelResponse::Ok {
            // GoalInfo: goal_id (UUID) + stamp (Time)
            writer
                .write_u32(16)
                .map_err(|_| ShimNodeError::Serialization)?;
            for b in &goal_id.uuid {
                writer
                    .write_u8(*b)
                    .map_err(|_| ShimNodeError::Serialization)?;
            }
            writer
                .write_i32(0)
                .map_err(|_| ShimNodeError::Serialization)?;
            writer
                .write_u32(0)
                .map_err(|_| ShimNodeError::Serialization)?;
        }
        let reply_len = writer.position();

        self.cancel_goal_server
            .send_reply(sequence_number, &self.goal_buffer[..reply_len])
            .map_err(ShimNodeError::Transport)?;

        Ok(Some((goal_id, response)))
    }

    /// Try to handle a get_result request
    ///
    /// Returns the goal ID if a result was returned.
    pub fn try_handle_get_result(&mut self) -> Result<Option<nros_core::GoalId>, ShimNodeError>
    where
        A::Result: Clone,
    {
        use nros_core::{CdrReader, CdrWriter};

        let request = self
            .get_result_server
            .try_recv_request(&mut self.goal_buffer)
            .map_err(ShimNodeError::Transport)?;

        let request = match request {
            Some(r) => r,
            None => return Ok(None),
        };

        let data_len = request.data.len();
        let sequence_number = request.sequence_number;
        #[allow(clippy::drop_non_drop)] // ends borrow on self.goal_buffer
        drop(request);

        // Deserialize goal_id
        let mut reader = CdrReader::new_with_header(&self.goal_buffer[..data_len])
            .map_err(|_| ShimNodeError::Transport(TransportError::DeserializationError))?;

        let mut goal_id = nros_core::GoalId::default();
        let uuid_len = reader.read_u32().unwrap_or(0) as usize;
        if uuid_len == 16 {
            for byte in &mut goal_id.uuid {
                *byte = reader.read_u8().unwrap_or(0);
            }
        }

        // Find in completed goals
        let completed = self
            .completed_goals
            .iter()
            .find(|g| g.goal_id.uuid == goal_id.uuid);

        // Find in active goals (for in-progress status)
        let active = self
            .active_goals
            .iter()
            .find(|g| g.goal_id.uuid == goal_id.uuid);

        // Serialize response: status (i8) + result
        let mut writer = CdrWriter::new_with_header(&mut self.result_buffer)
            .map_err(|_| ShimNodeError::BufferTooSmall)?;

        if let Some(completed_goal) = completed {
            writer
                .write_i8(completed_goal.status as i8)
                .map_err(|_| ShimNodeError::Serialization)?;
            completed_goal
                .result
                .serialize(&mut writer)
                .map_err(|_| ShimNodeError::Serialization)?;
        } else if let Some(active_goal) = active {
            writer
                .write_i8(active_goal.status as i8)
                .map_err(|_| ShimNodeError::Serialization)?;
            // Write default/empty result
            A::Result::default()
                .serialize(&mut writer)
                .map_err(|_| ShimNodeError::Serialization)?;
        } else {
            // Unknown goal
            writer
                .write_i8(nros_core::GoalStatus::Unknown as i8)
                .map_err(|_| ShimNodeError::Serialization)?;
            A::Result::default()
                .serialize(&mut writer)
                .map_err(|_| ShimNodeError::Serialization)?;
        }

        let reply_len = writer.position();
        self.get_result_server
            .send_reply(sequence_number, &self.result_buffer[..reply_len])
            .map_err(ShimNodeError::Transport)?;

        Ok(Some(goal_id))
    }

    /// Get a reference to an active goal
    pub fn get_goal(&self, goal_id: &nros_core::GoalId) -> Option<&ShimActiveGoal<A>> {
        self.active_goals
            .iter()
            .find(|g| g.goal_id.uuid == goal_id.uuid)
    }

    /// Get all active goals
    pub fn active_goals(&self) -> &[ShimActiveGoal<A>] {
        &self.active_goals
    }

    /// Get the number of active goals
    pub fn active_goal_count(&self) -> usize {
        self.active_goals.len()
    }
}

// ============================================================================
// ShimNodeActionClient
// ============================================================================

/// Action client handle for a typed action
///
/// Created via `ShimNode::create_action_client()`.
pub struct ShimNodeActionClient<
    A: RosAction,
    const GOAL_BUF: usize = 1024,
    const RESULT_BUF: usize = 1024,
    const FEEDBACK_BUF: usize = 1024,
> {
    send_goal_client: ShimServiceClient,
    cancel_goal_client: ShimServiceClient,
    get_result_client: ShimServiceClient,
    feedback_subscriber: ShimSubscriber,
    goal_buffer: [u8; GOAL_BUF],
    result_buffer: [u8; RESULT_BUF],
    feedback_buffer: [u8; FEEDBACK_BUF],
    goal_counter: u64,
    _phantom: PhantomData<A>,
}

impl<A: RosAction, const GOAL_BUF: usize, const RESULT_BUF: usize, const FEEDBACK_BUF: usize>
    ShimNodeActionClient<A, GOAL_BUF, RESULT_BUF, FEEDBACK_BUF>
{
    /// Send a goal to the action server
    ///
    /// Returns a GoalHandle indicating whether the goal was accepted.
    pub fn send_goal(&mut self, goal: &A::Goal) -> Result<nros_core::GoalId, ShimNodeError> {
        use nros_core::{CdrReader, CdrWriter};

        // Generate goal ID
        self.goal_counter += 1;
        let mut goal_id = nros_core::GoalId::default();
        let counter_bytes = self.goal_counter.to_le_bytes();
        goal_id.uuid[..8].copy_from_slice(&counter_bytes);

        // Serialize: goal_id (UUID) + goal
        let mut writer = CdrWriter::new_with_header(&mut self.goal_buffer)
            .map_err(|_| ShimNodeError::BufferTooSmall)?;

        // Write UUID as sequence
        writer
            .write_u32(16)
            .map_err(|_| ShimNodeError::Serialization)?;
        for b in &goal_id.uuid {
            writer
                .write_u8(*b)
                .map_err(|_| ShimNodeError::Serialization)?;
        }

        // Write goal
        goal.serialize(&mut writer)
            .map_err(|_| ShimNodeError::Serialization)?;

        let req_len = writer.position();

        // Send request
        let reply_len = self
            .send_goal_client
            .call_raw(&self.goal_buffer[..req_len], &mut self.result_buffer)
            .map_err(ShimNodeError::Transport)?;

        // Deserialize response: accepted (bool) + stamp (Time)
        let mut reader = CdrReader::new_with_header(&self.result_buffer[..reply_len])
            .map_err(|_| ShimNodeError::Transport(TransportError::DeserializationError))?;

        let accepted = reader.read_u8().unwrap_or(0) != 0;

        if accepted {
            Ok(goal_id)
        } else {
            Err(ShimNodeError::ServiceRequestFailed)
        }
    }

    /// Try to receive feedback (non-blocking)
    ///
    /// Returns the goal ID and feedback if available.
    pub fn try_recv_feedback(
        &mut self,
    ) -> Result<Option<(nros_core::GoalId, A::Feedback)>, ShimNodeError> {
        use nros_core::CdrReader;

        let data = self
            .feedback_subscriber
            .try_recv_raw(&mut self.feedback_buffer)
            .map_err(ShimNodeError::Transport)?;

        let len = match data {
            Some(len) => len,
            None => return Ok(None),
        };

        let mut reader = CdrReader::new_with_header(&self.feedback_buffer[..len])
            .map_err(|_| ShimNodeError::Transport(TransportError::DeserializationError))?;

        // Read goal_id
        let mut goal_id = nros_core::GoalId::default();
        let uuid_len = reader.read_u32().unwrap_or(0) as usize;
        if uuid_len == 16 {
            for byte in &mut goal_id.uuid {
                *byte = reader.read_u8().unwrap_or(0);
            }
        }

        // Read feedback
        let feedback = A::Feedback::deserialize(&mut reader)
            .map_err(|_| ShimNodeError::Transport(TransportError::DeserializationError))?;

        Ok(Some((goal_id, feedback)))
    }

    /// Cancel a goal
    pub fn cancel_goal(
        &mut self,
        goal_id: &nros_core::GoalId,
    ) -> Result<nros_core::CancelResponse, ShimNodeError> {
        use nros_core::{CdrReader, CdrWriter};

        // Serialize cancel request: GoalInfo (goal_id + stamp)
        let mut writer = CdrWriter::new_with_header(&mut self.goal_buffer)
            .map_err(|_| ShimNodeError::BufferTooSmall)?;

        // UUID as sequence
        writer
            .write_u32(16)
            .map_err(|_| ShimNodeError::Serialization)?;
        for b in &goal_id.uuid {
            writer
                .write_u8(*b)
                .map_err(|_| ShimNodeError::Serialization)?;
        }
        // stamp
        writer
            .write_i32(0)
            .map_err(|_| ShimNodeError::Serialization)?;
        writer
            .write_u32(0)
            .map_err(|_| ShimNodeError::Serialization)?;

        let req_len = writer.position();

        let reply_len = self
            .cancel_goal_client
            .call_raw(&self.goal_buffer[..req_len], &mut self.result_buffer)
            .map_err(ShimNodeError::Transport)?;

        // Deserialize response: return_code (i8) + goals_canceling
        let mut reader = CdrReader::new_with_header(&self.result_buffer[..reply_len])
            .map_err(|_| ShimNodeError::Transport(TransportError::DeserializationError))?;

        let return_code = reader.read_i8().unwrap_or(2);
        Ok(nros_core::CancelResponse::from_i8(return_code).unwrap_or_default())
    }

    /// Get the result of a completed goal
    pub fn get_result(
        &mut self,
        goal_id: &nros_core::GoalId,
    ) -> Result<(nros_core::GoalStatus, A::Result), ShimNodeError> {
        use nros_core::{CdrReader, CdrWriter};

        // Serialize request: goal_id (UUID)
        let mut writer = CdrWriter::new_with_header(&mut self.goal_buffer)
            .map_err(|_| ShimNodeError::BufferTooSmall)?;

        writer
            .write_u32(16)
            .map_err(|_| ShimNodeError::Serialization)?;
        for b in &goal_id.uuid {
            writer
                .write_u8(*b)
                .map_err(|_| ShimNodeError::Serialization)?;
        }

        let req_len = writer.position();

        let reply_len = self
            .get_result_client
            .call_raw(&self.goal_buffer[..req_len], &mut self.result_buffer)
            .map_err(ShimNodeError::Transport)?;

        // Deserialize response: status (i8) + result
        let mut reader = CdrReader::new_with_header(&self.result_buffer[..reply_len])
            .map_err(|_| ShimNodeError::Transport(TransportError::DeserializationError))?;

        let status_code = reader.read_i8().unwrap_or(0);
        let status = nros_core::GoalStatus::from_i8(status_code).unwrap_or_default();

        let result = A::Result::deserialize(&mut reader)
            .map_err(|_| ShimNodeError::Transport(TransportError::DeserializationError))?;

        Ok((status, result))
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_conversion() {
        let transport_err = TransportError::ConnectionFailed;
        let node_err: ShimNodeError = transport_err.into();
        assert_eq!(
            node_err,
            ShimNodeError::Transport(TransportError::ConnectionFailed)
        );
    }
}

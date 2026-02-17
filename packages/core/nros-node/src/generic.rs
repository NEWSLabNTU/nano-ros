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

use nros_core::{CdrReader, CdrWriter, Deserialize, RosAction, RosMessage, RosService, Serialize};
use nros_rmw::{
    ActionInfo, Publisher, QosSettings, ServiceClientTrait, ServiceInfo, ServiceServerTrait,
    Session, Subscriber, TopicInfo, TransportError,
};

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
// EmbeddedExecutor<S>
// ============================================================================

/// Backend-agnostic executor that owns a [`Session`].
///
/// Provides `create_node()` for entity creation and `drive_io()` for polling.
pub struct EmbeddedExecutor<S> {
    session: S,
}

impl<S: Session> EmbeddedExecutor<S> {
    /// Create an executor from an already-opened session.
    pub fn from_session(session: S) -> Self {
        Self { session }
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

    #[test]
    fn test_error_conversion() {
        let transport_err = TransportError::ConnectionFailed;
        let node_err: EmbeddedNodeError = transport_err.into();
        assert_eq!(
            node_err,
            EmbeddedNodeError::Transport(TransportError::ConnectionFailed)
        );
    }
}

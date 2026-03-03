//! Node — borrows the session to create typed entities.

use core::marker::PhantomData;

use nros_core::{RosAction, RosMessage, RosService};
use nros_rmw::{ActionInfo, QosSettings, ServiceInfo, Session, TopicInfo, TransportError};

use super::handles::{
    ActionClient, ActionServer, EmbeddedPublisher, EmbeddedServiceClient, EmbeddedServiceServer,
    Subscription,
};
use super::types::NodeError;

// ============================================================================
// Node<S>
// ============================================================================

/// Backend-agnostic node — borrows the session to create typed entities.
pub struct Node<'a, S: Session> {
    name: heapless::String<64>,
    namespace: heapless::String<64>,
    session: &'a mut S,
    domain_id: u32,
}

impl<'a, S: Session> Node<'a, S> {
    /// Create a new node (called by Executor::create_node).
    pub(crate) fn new(
        name: heapless::String<64>,
        namespace: heapless::String<64>,
        session: &'a mut S,
        domain_id: u32,
    ) -> Self {
        Self {
            name,
            namespace,
            session,
            domain_id,
        }
    }

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
    ) -> Result<EmbeddedPublisher<M, S::PublisherHandle>, NodeError> {
        self.create_publisher_with_qos::<M>(topic_name, QosSettings::default())
    }

    /// Create a publisher with custom QoS settings.
    pub fn create_publisher_with_qos<M: RosMessage>(
        &mut self,
        topic_name: &str,
        qos: QosSettings,
    ) -> Result<EmbeddedPublisher<M, S::PublisherHandle>, NodeError> {
        let topic = TopicInfo::new(topic_name, M::TYPE_NAME, M::TYPE_HASH)
            .with_domain(self.domain_id)
            .with_node_name(&self.name)
            .with_namespace(&self.namespace);
        let handle = self
            .session
            .create_publisher(&topic, qos)
            .map_err(|_| NodeError::Transport(TransportError::PublisherCreationFailed))?;
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
    ) -> Result<Subscription<M, S::SubscriberHandle, 1024>, NodeError> {
        self.create_subscription_sized::<M, 1024>(topic_name)
    }

    /// Create a subscription with custom buffer size.
    pub fn create_subscription_sized<M: RosMessage, const RX_BUF: usize>(
        &mut self,
        topic_name: &str,
    ) -> Result<Subscription<M, S::SubscriberHandle, RX_BUF>, NodeError> {
        self.create_subscription_with_qos::<M, RX_BUF>(topic_name, QosSettings::default())
    }

    /// Create a subscription with custom QoS and buffer size.
    pub fn create_subscription_with_qos<M: RosMessage, const RX_BUF: usize>(
        &mut self,
        topic_name: &str,
        qos: QosSettings,
    ) -> Result<Subscription<M, S::SubscriberHandle, RX_BUF>, NodeError> {
        let topic = TopicInfo::new(topic_name, M::TYPE_NAME, M::TYPE_HASH)
            .with_domain(self.domain_id)
            .with_node_name(&self.name)
            .with_namespace(&self.namespace);
        let handle = self
            .session
            .create_subscriber(&topic, qos)
            .map_err(|_| NodeError::Transport(TransportError::SubscriberCreationFailed))?;
        Ok(Subscription {
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
    ) -> Result<EmbeddedServiceServer<Svc, S::ServiceServerHandle, 1024, 1024>, NodeError> {
        self.create_service_sized::<Svc, 1024, 1024>(service_name)
    }

    /// Create a service server with custom buffer sizes.
    pub fn create_service_sized<Svc: RosService, const REQ_BUF: usize, const REPLY_BUF: usize>(
        &mut self,
        service_name: &str,
    ) -> Result<EmbeddedServiceServer<Svc, S::ServiceServerHandle, REQ_BUF, REPLY_BUF>, NodeError>
    {
        let info = ServiceInfo::new(service_name, Svc::SERVICE_NAME, Svc::SERVICE_HASH)
            .with_domain(self.domain_id)
            .with_node_name(&self.name)
            .with_namespace(&self.namespace);
        let handle = self
            .session
            .create_service_server(&info)
            .map_err(|_| NodeError::Transport(TransportError::ServiceServerCreationFailed))?;
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
    ) -> Result<EmbeddedServiceClient<Svc, S::ServiceClientHandle, 1024, 1024>, NodeError> {
        self.create_client_sized::<Svc, 1024, 1024>(service_name)
    }

    /// Create a service client with custom buffer sizes.
    pub fn create_client_sized<Svc: RosService, const REQ_BUF: usize, const REPLY_BUF: usize>(
        &mut self,
        service_name: &str,
    ) -> Result<EmbeddedServiceClient<Svc, S::ServiceClientHandle, REQ_BUF, REPLY_BUF>, NodeError>
    {
        let info = ServiceInfo::new(service_name, Svc::SERVICE_NAME, Svc::SERVICE_HASH)
            .with_domain(self.domain_id)
            .with_node_name(&self.name)
            .with_namespace(&self.namespace);
        let handle = self
            .session
            .create_service_client(&info)
            .map_err(|_| NodeError::Transport(TransportError::ServiceClientCreationFailed))?;
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
        ActionServer<A, S::ServiceServerHandle, S::PublisherHandle, 1024, 1024, 1024, 4>,
        NodeError,
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
        ActionServer<
            A,
            S::ServiceServerHandle,
            S::PublisherHandle,
            GOAL_BUF,
            RESULT_BUF,
            FEEDBACK_BUF,
            MAX_GOALS,
        >,
        NodeError,
    > {
        let action_info = ActionInfo::new(action_name, A::ACTION_NAME, A::ACTION_HASH)
            .with_domain(self.domain_id);

        let send_goal_keyexpr: heapless::String<256> = action_info.send_goal_key();
        let send_goal_info =
            ServiceInfo::new(&send_goal_keyexpr, A::ACTION_NAME, A::ACTION_HASH).with_domain(0);
        let send_goal_server = self
            .session
            .create_service_server(&send_goal_info)
            .map_err(|_| NodeError::ActionCreationFailed)?;

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
            .map_err(|_| NodeError::ActionCreationFailed)?;

        let get_result_keyexpr: heapless::String<256> = action_info.get_result_key();
        let get_result_info =
            ServiceInfo::new(&get_result_keyexpr, A::ACTION_NAME, A::ACTION_HASH).with_domain(0);
        let get_result_server = self
            .session
            .create_service_server(&get_result_info)
            .map_err(|_| NodeError::ActionCreationFailed)?;

        let feedback_keyexpr: heapless::String<256> = action_info.feedback_key();
        let feedback_topic =
            TopicInfo::new(&feedback_keyexpr, A::ACTION_NAME, A::ACTION_HASH).with_domain(0);
        let feedback_publisher = self
            .session
            .create_publisher(&feedback_topic, QosSettings::BEST_EFFORT)
            .map_err(|_| NodeError::ActionCreationFailed)?;

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
            .map_err(|_| NodeError::ActionCreationFailed)?;

        Ok(ActionServer {
            core: super::action_core::ActionServerCore {
                send_goal_server,
                cancel_goal_server,
                get_result_server,
                feedback_publisher,
                status_publisher,
                active_goals: heapless::Vec::new(),
                completed_results: heapless::Vec::new(),
                result_slab: [0u8; RESULT_BUF],
                result_slab_used: 0,
                goal_buffer: [0u8; GOAL_BUF],
                feedback_buffer: [0u8; FEEDBACK_BUF],
                cancel_buffer: [0u8; 256],
            },
            typed_goals: heapless::Vec::new(),
            completed_goals: heapless::Vec::new(),
        })
    }

    /// Create an action client.
    pub fn create_action_client<A: RosAction>(
        &mut self,
        action_name: &str,
    ) -> Result<
        ActionClient<A, S::ServiceClientHandle, S::SubscriberHandle, 1024, 1024, 1024>,
        NodeError,
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
        ActionClient<
            A,
            S::ServiceClientHandle,
            S::SubscriberHandle,
            GOAL_BUF,
            RESULT_BUF,
            FEEDBACK_BUF,
        >,
        NodeError,
    > {
        let action_info = ActionInfo::new(action_name, A::ACTION_NAME, A::ACTION_HASH)
            .with_domain(self.domain_id);

        let send_goal_keyexpr: heapless::String<256> = action_info.send_goal_key();
        let send_goal_info =
            ServiceInfo::new(&send_goal_keyexpr, A::ACTION_NAME, A::ACTION_HASH).with_domain(0);
        let send_goal_client = self
            .session
            .create_service_client(&send_goal_info)
            .map_err(|_| NodeError::ActionCreationFailed)?;

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
            .map_err(|_| NodeError::ActionCreationFailed)?;

        let get_result_keyexpr: heapless::String<256> = action_info.get_result_key();
        let get_result_info =
            ServiceInfo::new(&get_result_keyexpr, A::ACTION_NAME, A::ACTION_HASH).with_domain(0);
        let get_result_client = self
            .session
            .create_service_client(&get_result_info)
            .map_err(|_| NodeError::ActionCreationFailed)?;

        let feedback_keyexpr: heapless::String<256> = action_info.feedback_key();
        let feedback_topic =
            TopicInfo::new(&feedback_keyexpr, A::ACTION_NAME, A::ACTION_HASH).with_domain(0);
        let feedback_subscriber = self
            .session
            .create_subscriber(&feedback_topic, QosSettings::BEST_EFFORT)
            .map_err(|_| NodeError::ActionCreationFailed)?;

        Ok(ActionClient {
            core: super::action_core::ActionClientCore {
                send_goal_client,
                cancel_goal_client,
                get_result_client,
                feedback_subscriber,
                goal_buffer: [0u8; GOAL_BUF],
                result_buffer: [0u8; RESULT_BUF],
                feedback_buffer: [0u8; FEEDBACK_BUF],
                goal_counter: 0,
            },
            _phantom: PhantomData,
        })
    }
}

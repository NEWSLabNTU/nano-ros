//! Node — borrows the session to create typed entities.

use core::marker::PhantomData;

use nros_core::{RosAction, RosMessage, RosService};
use nros_rmw::{ActionInfo, QosSettings, ServiceInfo, Session as _, TopicInfo, TransportError};

use crate::session;

use super::{
    handles::{
        ActionClient, ActionServer, EmbeddedPublisher, EmbeddedServiceClient,
        EmbeddedServiceServer, Subscription,
    },
    types::NodeError,
};

// ============================================================================
// Node
// ============================================================================

/// Backend-agnostic node — borrows the session to create typed entities.
pub struct Node<'a> {
    name: heapless::String<64>,
    namespace: heapless::String<64>,
    session: &'a mut session::ConcreteSession,
    domain_id: u32,
}

impl<'a> Node<'a> {
    /// Create a new node (called by Executor::create_node).
    pub(crate) fn new(
        name: heapless::String<64>,
        namespace: heapless::String<64>,
        session: &'a mut session::ConcreteSession,
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
    pub fn session_mut(&mut self) -> &mut session::ConcreteSession {
        self.session
    }

    // ------------------------------------------------------------------
    // Routing-info builders (Phase 91.F)
    //
    // Every `create_*` below threads the same node identity (domain_id +
    // name + namespace) into a TopicInfo / ServiceInfo / ActionInfo. The
    // shape repeats verbatim ~12 times across this file. Centralised
    // here so a future change to the routing-info shape (e.g. adding a
    // `with_security_context`) updates one site instead of twelve, and
    // so the per-`create_*` function bodies focus on the parts that
    // actually differ between them.
    // ------------------------------------------------------------------

    // Associated fns (NOT `&self` methods) so the returned `*Info`
    // value's borrow tracks only the explicit `&str` arguments, not
    // the whole `Node`. A `&self` form would block the immediately-
    // following `self.session.create_*(&info, …)` mut borrow on the
    // `name` / `namespace` reborrow held inside the returned `*Info`,
    // because going through a method call hides the field-disjoint
    // path that lets `&self.name` + `&mut self.session` coexist.
    fn topic_info<'b>(
        domain_id: u32,
        node_name: &'b str,
        namespace: &'b str,
        topic_name: &'b str,
        type_name: &'b str,
        type_hash: &'b str,
    ) -> TopicInfo<'b> {
        TopicInfo::new(topic_name, type_name, type_hash)
            .with_domain(domain_id)
            .with_node_name(node_name)
            .with_namespace(namespace)
    }

    fn service_info<'b>(
        domain_id: u32,
        node_name: &'b str,
        namespace: &'b str,
        service_name: &'b str,
        type_name: &'b str,
        type_hash: &'b str,
    ) -> ServiceInfo<'b> {
        ServiceInfo::new(service_name, type_name, type_hash)
            .with_domain(domain_id)
            .with_node_name(node_name)
            .with_namespace(namespace)
    }

    fn action_info<'b>(
        domain_id: u32,
        action_name: &'b str,
        type_name: &'b str,
        type_hash: &'b str,
    ) -> ActionInfo<'b> {
        // Action root only needs the domain — per-channel ServiceInfo /
        // TopicInfo derived from action_info.{send_goal,cancel_goal,...}_key()
        // carry the full node identity via service_info() / topic_info().
        ActionInfo::new(action_name, type_name, type_hash).with_domain(domain_id)
    }

    // -- Publishers --

    /// Create a publisher for the given topic.
    pub fn create_publisher<M: RosMessage>(
        &mut self,
        topic_name: &str,
    ) -> Result<EmbeddedPublisher<M>, NodeError> {
        self.create_publisher_with_qos::<M>(topic_name, QosSettings::default())
    }

    /// Create a publisher with custom QoS settings.
    pub fn create_publisher_with_qos<M: RosMessage>(
        &mut self,
        topic_name: &str,
        qos: QosSettings,
    ) -> Result<EmbeddedPublisher<M>, NodeError> {
        let topic = Self::topic_info(
            self.domain_id,
            &self.name,
            &self.namespace,
            topic_name,
            M::TYPE_NAME,
            M::TYPE_HASH,
        );
        let handle = self
            .session
            .create_publisher(&topic, qos)
            .map_err(|_| NodeError::Transport(TransportError::PublisherCreationFailed))?;
        Ok(EmbeddedPublisher {
            handle,
            _phantom: PhantomData,
        })
    }

    /// Create a typeless publisher for non-ROS wire formats (e.g. PX4 uORB
    /// raw POD bytes, custom binary protocols). The caller supplies the
    /// `type_name` and `type_hash` strings used by backends that need them
    /// for liveliness/discovery; backends that don't (uORB) can pass any
    /// stable string.
    pub fn create_publisher_raw(
        &mut self,
        topic_name: &str,
        type_name: &str,
        type_hash: &str,
    ) -> Result<crate::executor::handles::EmbeddedRawPublisher, NodeError> {
        self.create_publisher_raw_with_qos(topic_name, type_name, type_hash, QosSettings::default())
    }

    /// Typeless publisher with custom QoS.
    pub fn create_publisher_raw_with_qos(
        &mut self,
        topic_name: &str,
        type_name: &str,
        type_hash: &str,
        qos: QosSettings,
    ) -> Result<crate::executor::handles::EmbeddedRawPublisher, NodeError> {
        let topic = Self::topic_info(
            self.domain_id,
            &self.name,
            &self.namespace,
            topic_name,
            type_name,
            type_hash,
        );
        let handle = self
            .session
            .create_publisher(&topic, qos)
            .map_err(|_| NodeError::Transport(TransportError::PublisherCreationFailed))?;
        Ok(crate::executor::handles::EmbeddedRawPublisher {
            handle,
            arena: crate::executor::handles::TxArena::new(),
        })
    }

    // -- Subscriptions --

    /// Create a subscription for the given topic.
    pub fn create_subscription<M: RosMessage>(
        &mut self,
        topic_name: &str,
    ) -> Result<Subscription<M>, NodeError> {
        self.create_subscription_sized::<M, { crate::config::DEFAULT_RX_BUF_SIZE }>(topic_name)
    }

    /// Create a subscription with custom buffer size.
    pub fn create_subscription_sized<M: RosMessage, const RX_BUF: usize>(
        &mut self,
        topic_name: &str,
    ) -> Result<Subscription<M, RX_BUF>, NodeError> {
        self.create_subscription_with_qos::<M, RX_BUF>(topic_name, QosSettings::default())
    }

    /// Create a subscription with custom QoS and buffer size.
    pub fn create_subscription_with_qos<M: RosMessage, const RX_BUF: usize>(
        &mut self,
        topic_name: &str,
        qos: QosSettings,
    ) -> Result<Subscription<M, RX_BUF>, NodeError> {
        let topic = Self::topic_info(
            self.domain_id,
            &self.name,
            &self.namespace,
            topic_name,
            M::TYPE_NAME,
            M::TYPE_HASH,
        );
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

    /// Create a typeless subscription. Caller decodes raw bytes themselves.
    pub fn create_subscription_raw(
        &mut self,
        topic_name: &str,
        type_name: &str,
        type_hash: &str,
    ) -> Result<crate::executor::handles::RawSubscription, NodeError> {
        self.create_subscription_raw_sized::<{ crate::config::DEFAULT_RX_BUF_SIZE }>(
            topic_name, type_name, type_hash,
        )
    }

    /// Typeless subscription with custom buffer size.
    pub fn create_subscription_raw_sized<const RX_BUF: usize>(
        &mut self,
        topic_name: &str,
        type_name: &str,
        type_hash: &str,
    ) -> Result<crate::executor::handles::RawSubscription<RX_BUF>, NodeError> {
        let topic = Self::topic_info(
            self.domain_id,
            &self.name,
            &self.namespace,
            topic_name,
            type_name,
            type_hash,
        );
        let handle = self
            .session
            .create_subscriber(&topic, QosSettings::default())
            .map_err(|_| NodeError::Transport(TransportError::SubscriberCreationFailed))?;
        Ok(crate::executor::handles::RawSubscription {
            handle,
            buffer: [0u8; RX_BUF],
        })
    }

    // -- Services --

    /// Create a service server.
    pub fn create_service<Svc: RosService>(
        &mut self,
        service_name: &str,
    ) -> Result<EmbeddedServiceServer<Svc>, NodeError> {
        self.create_service_sized::<Svc, { crate::config::DEFAULT_RX_BUF_SIZE }, { crate::config::DEFAULT_RX_BUF_SIZE }>(service_name)
    }

    /// Create a service server with custom buffer sizes.
    pub fn create_service_sized<Svc: RosService, const REQ_BUF: usize, const REPLY_BUF: usize>(
        &mut self,
        service_name: &str,
    ) -> Result<EmbeddedServiceServer<Svc, REQ_BUF, REPLY_BUF>, NodeError> {
        let info = Self::service_info(
            self.domain_id,
            &self.name,
            &self.namespace,
            service_name,
            Svc::SERVICE_NAME,
            Svc::SERVICE_HASH,
        );
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
    ) -> Result<EmbeddedServiceClient<Svc>, NodeError> {
        self.create_client_sized::<Svc, { crate::config::DEFAULT_RX_BUF_SIZE }, { crate::config::DEFAULT_RX_BUF_SIZE }>(service_name)
    }

    /// Create a service client with custom buffer sizes.
    pub fn create_client_sized<Svc: RosService, const REQ_BUF: usize, const REPLY_BUF: usize>(
        &mut self,
        service_name: &str,
    ) -> Result<EmbeddedServiceClient<Svc, REQ_BUF, REPLY_BUF>, NodeError> {
        let info = Self::service_info(
            self.domain_id,
            &self.name,
            &self.namespace,
            service_name,
            Svc::SERVICE_NAME,
            Svc::SERVICE_HASH,
        );
        let handle = self
            .session
            .create_service_client(&info)
            .map_err(|_| NodeError::Transport(TransportError::ServiceClientCreationFailed))?;
        Ok(EmbeddedServiceClient {
            handle,
            req_buffer: [0u8; REQ_BUF],
            reply_buffer: [0u8; REPLY_BUF],
            in_flight: false,
            _phantom: PhantomData,
        })
    }

    // -- Actions --

    /// Create an action server.
    pub fn create_action_server<A: RosAction>(
        &mut self,
        action_name: &str,
    ) -> Result<ActionServer<A>, NodeError> {
        self.create_action_server_sized::<A, { crate::config::DEFAULT_RX_BUF_SIZE }, { crate::config::DEFAULT_RX_BUF_SIZE }, { crate::config::DEFAULT_RX_BUF_SIZE }, 4>(action_name)
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
    ) -> Result<ActionServer<A, GOAL_BUF, RESULT_BUF, FEEDBACK_BUF, MAX_GOALS>, NodeError> {
        let action_info =
            Self::action_info(self.domain_id, action_name, A::ACTION_NAME, A::ACTION_HASH);

        // Each underlying ServiceInfo / TopicInfo also carries the
        // node identity so the Zenoh shim declares a liveliness token
        // for it. Without `with_node_name` the shim's
        // `declare_entity_liveliness` short-circuits (`node_name.and_then`
        // → None) and `wait_for_action_server` has nothing to find.
        let send_goal_keyexpr: heapless::String<256> = action_info.send_goal_key();
        let send_goal_info = Self::service_info(
            self.domain_id,
            &self.name,
            &self.namespace,
            &send_goal_keyexpr,
            A::ACTION_NAME,
            A::ACTION_HASH,
        );
        let send_goal_server = self
            .session
            .create_service_server(&send_goal_info)
            .map_err(|_| NodeError::ActionCreationFailed)?;

        let cancel_goal_keyexpr: heapless::String<256> = action_info.cancel_goal_key();
        let cancel_goal_info = Self::service_info(
            self.domain_id,
            &self.name,
            &self.namespace,
            &cancel_goal_keyexpr,
            "action_msgs::srv::dds_::CancelGoal_",
            A::ACTION_HASH,
        );
        let cancel_goal_server = self
            .session
            .create_service_server(&cancel_goal_info)
            .map_err(|_| NodeError::ActionCreationFailed)?;

        let get_result_keyexpr: heapless::String<256> = action_info.get_result_key();
        let get_result_info = Self::service_info(
            self.domain_id,
            &self.name,
            &self.namespace,
            &get_result_keyexpr,
            A::ACTION_NAME,
            A::ACTION_HASH,
        );
        let get_result_server = self
            .session
            .create_service_server(&get_result_info)
            .map_err(|_| NodeError::ActionCreationFailed)?;

        let feedback_keyexpr: heapless::String<256> = action_info.feedback_key();
        let feedback_topic = Self::topic_info(
            self.domain_id,
            &self.name,
            &self.namespace,
            &feedback_keyexpr,
            A::ACTION_NAME,
            A::ACTION_HASH,
        );
        let feedback_publisher = self
            .session
            .create_publisher(&feedback_topic, QosSettings::BEST_EFFORT)
            .map_err(|_| NodeError::ActionCreationFailed)?;

        let status_keyexpr: heapless::String<256> = action_info.status_key();
        let status_topic = Self::topic_info(
            self.domain_id,
            &self.name,
            &self.namespace,
            &status_keyexpr,
            "action_msgs::msg::dds_::GoalStatusArray_",
            A::ACTION_HASH,
        );
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
    ) -> Result<ActionClient<A>, NodeError> {
        self.create_action_client_sized::<A, { crate::config::DEFAULT_RX_BUF_SIZE }, { crate::config::DEFAULT_RX_BUF_SIZE }, { crate::config::DEFAULT_RX_BUF_SIZE }>(action_name)
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
    ) -> Result<ActionClient<A, GOAL_BUF, RESULT_BUF, FEEDBACK_BUF>, NodeError> {
        let action_info =
            Self::action_info(self.domain_id, action_name, A::ACTION_NAME, A::ACTION_HASH);

        // Mirror `create_action_server_sized`: thread node identity through
        // each underlying ServiceInfo / TopicInfo so the Zenoh shim
        // declares the matching client-side liveliness tokens (and so the
        // discovery wildcard built from `send_goal_info` ends up in the
        // same domain as the server's tokens).
        let send_goal_keyexpr: heapless::String<256> = action_info.send_goal_key();
        let send_goal_info = Self::service_info(
            self.domain_id,
            &self.name,
            &self.namespace,
            &send_goal_keyexpr,
            A::ACTION_NAME,
            A::ACTION_HASH,
        );
        let send_goal_client = self
            .session
            .create_service_client(&send_goal_info)
            .map_err(|_| NodeError::ActionCreationFailed)?;

        let cancel_goal_keyexpr: heapless::String<256> = action_info.cancel_goal_key();
        let cancel_goal_info = Self::service_info(
            self.domain_id,
            &self.name,
            &self.namespace,
            &cancel_goal_keyexpr,
            "action_msgs::srv::dds_::CancelGoal_",
            A::ACTION_HASH,
        );
        let cancel_goal_client = self
            .session
            .create_service_client(&cancel_goal_info)
            .map_err(|_| NodeError::ActionCreationFailed)?;

        let get_result_keyexpr: heapless::String<256> = action_info.get_result_key();
        let get_result_info = Self::service_info(
            self.domain_id,
            &self.name,
            &self.namespace,
            &get_result_keyexpr,
            A::ACTION_NAME,
            A::ACTION_HASH,
        );
        let get_result_client = self
            .session
            .create_service_client(&get_result_info)
            .map_err(|_| NodeError::ActionCreationFailed)?;

        let feedback_keyexpr: heapless::String<256> = action_info.feedback_key();
        let feedback_topic = Self::topic_info(
            self.domain_id,
            &self.name,
            &self.namespace,
            &feedback_keyexpr,
            A::ACTION_NAME,
            A::ACTION_HASH,
        );
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
                in_flight_send_goal: false,
                in_flight_cancel: false,
                in_flight_get_result: false,
            },
            _phantom: PhantomData,
        })
    }
}

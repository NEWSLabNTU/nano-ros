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

    /// Phase 88.12 — return the [`nros_log::Logger`] keyed on the
    /// node name.
    ///
    /// Loggers are interned in nros-log's bounded global table
    /// ([`nros_log::MAX_LOGGERS`] slots). If the caller has
    /// pre-registered a `'static Logger` whose name matches this
    /// node's name (via [`nros_log::register_logger`]), this method
    /// returns that exact reference — so subsequent `nros_*!` calls
    /// share per-logger runtime threshold state with any other call
    /// site that resolves the same name. Otherwise the call returns
    /// [`nros_log::DEFAULT_LOGGER`], keeping the API total.
    ///
    /// ```ignore
    /// // Pre-register if you want a dedicated threshold:
    /// static MY_NODE_LOGGER: nros_log::Logger =
    ///     nros_log::Logger::new("my_node");
    /// nros_log::register_logger(&MY_NODE_LOGGER);
    ///
    /// // Inside any node-creating code:
    /// let logger = node.logger();
    /// nros_log::nros_info!(logger, "started; domain = {}", node.domain_id());
    /// ```
    #[must_use]
    pub fn logger(&self) -> &'static nros_log::Logger {
        nros_log::get_logger(self.name())
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
        // Phase 108.B — synchronous QoS validation against backend's
        // `supported_qos_policies()` mask. No silent downgrade.
        qos.validate_against(nros_rmw::Session::supported_qos_policies(self.session))
            .map_err(NodeError::Transport)?;
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
            event_regs: crate::executor::handles::empty_event_regs(),
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
        qos.validate_against(nros_rmw::Session::supported_qos_policies(self.session))
            .map_err(NodeError::Transport)?;
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
            event_regs: crate::executor::handles::empty_event_regs(),
        })
    }

    /// Phase 189.M1 — the customizable publisher **builder** (the `clone` tier;
    /// see `docs/design/entity-api-tiers.md`). Pick a mode with `.typed::<M>()`
    /// or `.generic(type, hash)`, set knobs (`.qos`), then `.build()`. The
    /// convenient `create_publisher` / `create_publisher_raw` are the `fork`
    /// tier — sugar over this with defaults.
    pub fn publisher<'t>(&mut self, topic: &'t str) -> PublisherBuilder<'_, 'a, 't> {
        PublisherBuilder {
            node: self,
            topic,
            qos: QosSettings::default(),
        }
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
        qos.validate_against(nros_rmw::Session::supported_qos_policies(self.session))
            .map_err(NodeError::Transport)?;
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
            event_regs: crate::executor::handles::empty_event_regs(),
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
            event_regs: crate::executor::handles::empty_event_regs(),
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

    /// Typeless service server. L1 counterpart of [`create_service`]
    /// for the C / C++ FFI shims and callers that own their own
    /// scheduler. Returns a [`crate::executor::handles::RawServiceServer`]
    /// which polls request bytes directly.
    pub fn create_service_raw(
        &mut self,
        service_name: &str,
        type_name: &str,
        type_hash: &str,
    ) -> Result<crate::executor::handles::RawServiceServer, NodeError> {
        self.create_service_raw_sized::<
            { crate::config::DEFAULT_RX_BUF_SIZE },
            { crate::config::DEFAULT_RX_BUF_SIZE },
        >(service_name, type_name, type_hash)
    }

    /// Typeless service server with custom buffer sizes.
    pub fn create_service_raw_sized<const REQ_BUF: usize, const RESP_BUF: usize>(
        &mut self,
        service_name: &str,
        type_name: &str,
        type_hash: &str,
    ) -> Result<crate::executor::handles::RawServiceServer<REQ_BUF, RESP_BUF>, NodeError> {
        let info = Self::service_info(
            self.domain_id,
            &self.name,
            &self.namespace,
            service_name,
            type_name,
            type_hash,
        );
        let handle = self
            .session
            .create_service_server(&info)
            .map_err(|_| NodeError::Transport(TransportError::ServiceServerCreationFailed))?;
        Ok(crate::executor::handles::RawServiceServer::new(handle))
    }

    /// Typeless service client. L1 counterpart of [`create_client`].
    pub fn create_client_raw(
        &mut self,
        service_name: &str,
        type_name: &str,
        type_hash: &str,
    ) -> Result<crate::executor::handles::RawServiceClient, NodeError> {
        self.create_client_raw_sized::<
            { crate::config::DEFAULT_RX_BUF_SIZE },
            { crate::config::DEFAULT_RX_BUF_SIZE },
        >(service_name, type_name, type_hash)
    }

    /// Typeless service client with custom buffer sizes.
    pub fn create_client_raw_sized<const REQ_BUF: usize, const REPLY_BUF: usize>(
        &mut self,
        service_name: &str,
        type_name: &str,
        type_hash: &str,
    ) -> Result<crate::executor::handles::RawServiceClient<REQ_BUF, REPLY_BUF>, NodeError> {
        let info = Self::service_info(
            self.domain_id,
            &self.name,
            &self.namespace,
            service_name,
            type_name,
            type_hash,
        );
        let handle = self
            .session
            .create_service_client(&info)
            .map_err(|_| NodeError::Transport(TransportError::ServiceClientCreationFailed))?;
        Ok(crate::executor::handles::RawServiceClient::new(handle))
    }

    // -- Actions --

    /// Phase 122.3.c.6 — typeless action server. Builds the 5
    /// transport channels (`send_goal` / `cancel_goal` / `get_result`
    /// services + `feedback` / `status` publishers) and returns the
    /// raw `ActionServerCore` directly. Caller owns scheduling —
    /// drives `try_recv_goal_request` / `publish_feedback_raw` /
    /// `complete_goal_raw` / `try_handle_cancel` /
    /// `try_handle_get_result_raw` on the returned core.
    pub fn create_action_server_raw(
        &mut self,
        action_name: &str,
        type_name: &str,
        type_hash: &str,
    ) -> Result<
        super::action_core::ActionServerCore<
            { crate::config::DEFAULT_RX_BUF_SIZE },
            { crate::config::DEFAULT_RX_BUF_SIZE },
            { crate::config::DEFAULT_RX_BUF_SIZE },
            4,
        >,
        NodeError,
    > {
        self.create_action_server_raw_sized::<
            { crate::config::DEFAULT_RX_BUF_SIZE },
            { crate::config::DEFAULT_RX_BUF_SIZE },
            { crate::config::DEFAULT_RX_BUF_SIZE },
            4,
        >(action_name, type_name, type_hash)
    }

    /// Typeless action server with custom buffer + goal-slot sizes.
    pub fn create_action_server_raw_sized<
        const GOAL_BUF: usize,
        const RESULT_BUF: usize,
        const FEEDBACK_BUF: usize,
        const MAX_GOALS: usize,
    >(
        &mut self,
        action_name: &str,
        type_name: &str,
        type_hash: &str,
    ) -> Result<
        super::action_core::ActionServerCore<GOAL_BUF, RESULT_BUF, FEEDBACK_BUF, MAX_GOALS>,
        NodeError,
    > {
        let action_info = Self::action_info(self.domain_id, action_name, type_name, type_hash);

        let send_goal_keyexpr: heapless::String<256> = action_info.send_goal_key();
        let send_goal_info = Self::service_info(
            self.domain_id,
            &self.name,
            &self.namespace,
            &send_goal_keyexpr,
            type_name,
            type_hash,
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
            type_hash,
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
            type_name,
            type_hash,
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
            type_name,
            type_hash,
        );
        let feedback_publisher = self
            .session
            .create_publisher(&feedback_topic, QosSettings::QOS_PROFILE_DEFAULT)
            .map_err(|_| NodeError::ActionCreationFailed)?;

        let status_keyexpr: heapless::String<256> = action_info.status_key();
        let status_topic = Self::topic_info(
            self.domain_id,
            &self.name,
            &self.namespace,
            &status_keyexpr,
            "action_msgs::msg::dds_::GoalStatusArray_",
            type_hash,
        );
        let status_publisher = self
            .session
            .create_publisher(
                &status_topic,
                QosSettings::QOS_PROFILE_ACTION_STATUS_DEFAULT,
            )
            .map_err(|_| NodeError::ActionCreationFailed)?;

        Ok(super::action_core::ActionServerCore {
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
        })
    }

    /// Phase 122.3.c.6 — typeless action client. Same shape as
    /// `create_action_server_raw` but builds the 3 service clients
    /// + 1 feedback subscriber, returns the raw `ActionClientCore`.
    pub fn create_action_client_raw(
        &mut self,
        action_name: &str,
        type_name: &str,
        type_hash: &str,
    ) -> Result<
        super::action_core::ActionClientCore<
            { crate::config::DEFAULT_RX_BUF_SIZE },
            { crate::config::DEFAULT_RX_BUF_SIZE },
            { crate::config::DEFAULT_RX_BUF_SIZE },
        >,
        NodeError,
    > {
        self.create_action_client_raw_sized::<
            { crate::config::DEFAULT_RX_BUF_SIZE },
            { crate::config::DEFAULT_RX_BUF_SIZE },
            { crate::config::DEFAULT_RX_BUF_SIZE },
        >(action_name, type_name, type_hash)
    }

    /// Typeless action client with custom buffer sizes.
    pub fn create_action_client_raw_sized<
        const GOAL_BUF: usize,
        const RESULT_BUF: usize,
        const FEEDBACK_BUF: usize,
    >(
        &mut self,
        action_name: &str,
        type_name: &str,
        type_hash: &str,
    ) -> Result<super::action_core::ActionClientCore<GOAL_BUF, RESULT_BUF, FEEDBACK_BUF>, NodeError>
    {
        let action_info = Self::action_info(self.domain_id, action_name, type_name, type_hash);

        let send_goal_keyexpr: heapless::String<256> = action_info.send_goal_key();
        let send_goal_info = Self::service_info(
            self.domain_id,
            &self.name,
            &self.namespace,
            &send_goal_keyexpr,
            type_name,
            type_hash,
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
            type_hash,
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
            type_name,
            type_hash,
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
            type_name,
            type_hash,
        );
        let feedback_subscriber = self
            .session
            .create_subscriber(&feedback_topic, QosSettings::BEST_EFFORT)
            .map_err(|_| NodeError::ActionCreationFailed)?;

        Ok(super::action_core::ActionClientCore::new(
            send_goal_client,
            cancel_goal_client,
            get_result_client,
            feedback_subscriber,
        ))
    }

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
            .create_publisher(&feedback_topic, QosSettings::QOS_PROFILE_DEFAULT)
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
            .create_publisher(
                &status_topic,
                QosSettings::QOS_PROFILE_ACTION_STATUS_DEFAULT,
            )
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

// ===================================================================
// Phase 189.M1 — entity builders (the `clone` tier)
// ===================================================================

/// Publisher builder — `node.publisher(topic)`. Choose `.typed::<M>()` or
/// `.generic(type, hash)`, optionally `.qos(..)`, then `.build()`.
pub struct PublisherBuilder<'n, 'a, 't> {
    node: &'n mut Node<'a>,
    topic: &'t str,
    qos: QosSettings,
}

impl<'n, 'a, 't> PublisherBuilder<'n, 'a, 't> {
    /// Set the QoS (also settable on the typed/generic builder).
    pub fn qos(mut self, qos: QosSettings) -> Self {
        self.qos = qos;
        self
    }

    /// Typed publisher for a ROS message `M` (mirrors rclcpp/rclrs).
    pub fn typed<M: RosMessage>(self) -> TypedPublisherBuilder<'n, 'a, 't, M> {
        TypedPublisherBuilder {
            node: self.node,
            topic: self.topic,
            qos: self.qos,
            _phantom: PhantomData,
        }
    }

    /// Generic (type-erased) publisher — the rclcpp `create_generic_publisher`
    /// form; raw CDR bytes via `publish_raw`.
    pub fn generic(
        self,
        type_name: &'t str,
        type_hash: &'t str,
    ) -> GenericPublisherBuilder<'n, 'a, 't> {
        GenericPublisherBuilder {
            node: self.node,
            topic: self.topic,
            type_name,
            type_hash,
            qos: self.qos,
        }
    }
}

/// Typed publisher builder (`.typed::<M>()`).
pub struct TypedPublisherBuilder<'n, 'a, 't, M> {
    node: &'n mut Node<'a>,
    topic: &'t str,
    qos: QosSettings,
    _phantom: PhantomData<M>,
}

impl<'n, 'a, 't, M: RosMessage> TypedPublisherBuilder<'n, 'a, 't, M> {
    pub fn qos(mut self, qos: QosSettings) -> Self {
        self.qos = qos;
        self
    }

    pub fn build(self) -> Result<EmbeddedPublisher<M>, NodeError> {
        self.node
            .create_publisher_with_qos::<M>(self.topic, self.qos)
    }
}

/// Generic (type-erased) publisher builder (`.generic(type, hash)`).
pub struct GenericPublisherBuilder<'n, 'a, 't> {
    node: &'n mut Node<'a>,
    topic: &'t str,
    type_name: &'t str,
    type_hash: &'t str,
    qos: QosSettings,
}

impl<'n, 'a, 't> GenericPublisherBuilder<'n, 'a, 't> {
    pub fn qos(mut self, qos: QosSettings) -> Self {
        self.qos = qos;
        self
    }

    pub fn build(self) -> Result<crate::executor::handles::EmbeddedRawPublisher, NodeError> {
        self.node.create_publisher_raw_with_qos(
            self.topic,
            self.type_name,
            self.type_hash,
            self.qos,
        )
    }
}

/// An executor-borrowing node handle — `exec.node(id)`. Hosts the
/// callback-registering entity builders (subscriptions register into the
/// executor's dispatch arena). It is a **short-lived `&mut Executor` borrow**:
/// create entities, then drop it before acquiring the next node handle; entity
/// handles (`HandleId`, publishers) are owned and outlive it (no `Arc` — see
/// `docs/design/entity-api-tiers.md` §Borrow model).
pub struct NodeCtx<'e> {
    executor: &'e mut super::spin::Executor,
    node_id: super::node_record::NodeId,
}

impl<'e> NodeCtx<'e> {
    pub(crate) fn new(
        executor: &'e mut super::spin::Executor,
        node_id: super::node_record::NodeId,
    ) -> Self {
        Self { executor, node_id }
    }

    /// Subscription builder (the `clone` tier). Pick a mode with `.typed::<M>()`
    /// or `.generic(type, hash)`, set knobs (`.qos`), then `.build(callback)`.
    pub fn subscription<'t>(&mut self, topic: &'t str) -> SubscriptionBuilder<'_, 'e, 't> {
        SubscriptionBuilder {
            ctx: self,
            topic,
            qos: QosSettings::default(),
        }
    }

    /// Convenient typed subscription (the `fork` tier — rclcpp/rclrs shape).
    /// Sugar over the builder with default QoS + buffer.
    pub fn create_subscription<M, F>(
        &mut self,
        topic: &str,
        callback: F,
    ) -> Result<super::types::HandleId, NodeError>
    where
        M: RosMessage + 'static,
        F: FnMut(&M) + 'static,
    {
        self.executor
            .register_subscription_buffered_on::<M, F, { crate::config::DEFAULT_RX_BUF_SIZE }>(
                self.node_id,
                topic,
                QosSettings::default(),
                callback,
            )
    }

    /// Convenient generic (type-erased) subscription — rclcpp `create_generic_*`.
    pub fn create_generic_subscription<F>(
        &mut self,
        topic: &str,
        type_name: &str,
        type_hash: &str,
        callback: F,
    ) -> Result<super::types::HandleId, NodeError>
    where
        F: FnMut(&[u8]) + 'static,
    {
        self.executor
            .register_subscription_buffered_raw_on::<F, { crate::config::DEFAULT_RX_BUF_SIZE }>(
                self.node_id,
                topic,
                type_name,
                type_hash,
                QosSettings::default(),
                callback,
            )
    }
}

/// Subscription builder — `node.subscription(topic)`.
pub struct SubscriptionBuilder<'c, 'e, 't> {
    ctx: &'c mut NodeCtx<'e>,
    topic: &'t str,
    qos: QosSettings,
}

impl<'c, 'e, 't> SubscriptionBuilder<'c, 'e, 't> {
    pub fn qos(mut self, qos: QosSettings) -> Self {
        self.qos = qos;
        self
    }

    /// Typed subscription for a ROS message `M`.
    pub fn typed<M: RosMessage + 'static>(self) -> TypedSubscriptionBuilder<'c, 'e, 't, M> {
        TypedSubscriptionBuilder {
            ctx: self.ctx,
            topic: self.topic,
            qos: self.qos,
            _phantom: PhantomData,
        }
    }

    /// Generic (type-erased) subscription — raw CDR bytes to the callback.
    pub fn generic(
        self,
        type_name: &'t str,
        type_hash: &'t str,
    ) -> GenericSubscriptionBuilder<'c, 'e, 't> {
        GenericSubscriptionBuilder {
            ctx: self.ctx,
            topic: self.topic,
            type_name,
            type_hash,
            qos: self.qos,
        }
    }
}

/// Typed subscription builder (`.typed::<M>()`).
pub struct TypedSubscriptionBuilder<'c, 'e, 't, M> {
    ctx: &'c mut NodeCtx<'e>,
    topic: &'t str,
    qos: QosSettings,
    _phantom: PhantomData<M>,
}

impl<'c, 'e, 't, M: RosMessage + 'static> TypedSubscriptionBuilder<'c, 'e, 't, M> {
    pub fn qos(mut self, qos: QosSettings) -> Self {
        self.qos = qos;
        self
    }

    pub fn build<F: FnMut(&M) + 'static>(
        self,
        callback: F,
    ) -> Result<super::types::HandleId, NodeError> {
        self.ctx
            .executor
            .register_subscription_buffered_on::<M, F, { crate::config::DEFAULT_RX_BUF_SIZE }>(
                self.ctx.node_id,
                self.topic,
                self.qos,
                callback,
            )
    }
}

/// Generic (type-erased) subscription builder (`.generic(type, hash)`).
pub struct GenericSubscriptionBuilder<'c, 'e, 't> {
    ctx: &'c mut NodeCtx<'e>,
    topic: &'t str,
    type_name: &'t str,
    type_hash: &'t str,
    qos: QosSettings,
}

impl<'c, 'e, 't> GenericSubscriptionBuilder<'c, 'e, 't> {
    pub fn qos(mut self, qos: QosSettings) -> Self {
        self.qos = qos;
        self
    }

    pub fn build<F: FnMut(&[u8]) + 'static>(
        self,
        callback: F,
    ) -> Result<super::types::HandleId, NodeError> {
        self.ctx
            .executor
            .register_subscription_buffered_raw_on::<F, { crate::config::DEFAULT_RX_BUF_SIZE }>(
                self.ctx.node_id,
                self.topic,
                self.type_name,
                self.type_hash,
                self.qos,
                callback,
            )
    }
}

#[cfg(test)]
mod builder_tests {
    use super::*;
    use crate::{executor::Executor, mock::MockSession};
    use nros_core::{CdrReader, CdrWriter, DeserError, Deserialize, SerError, Serialize};

    struct TestMsg;
    impl RosMessage for TestMsg {
        const TYPE_NAME: &'static str = "test/msg/TestMsg";
        const TYPE_HASH: &'static str = "test_hash";
    }
    impl Serialize for TestMsg {
        fn serialize(&self, _w: &mut CdrWriter) -> Result<(), SerError> {
            Ok(())
        }
    }
    impl Deserialize for TestMsg {
        fn deserialize(_r: &mut CdrReader) -> Result<Self, DeserError> {
            Ok(Self)
        }
    }

    fn s(v: &str) -> heapless::String<64> {
        heapless::String::try_from(v).unwrap()
    }

    #[test]
    fn publisher_builder_typed_and_generic() {
        let mut session = MockSession::new();
        let mut node = Node::new(s("n"), s("/"), &mut session, 0);

        // typed: node.publisher(t).typed::<M>().qos(..).build()
        let _typed = node
            .publisher("/chatter")
            .typed::<TestMsg>()
            .qos(QosSettings::default().keep_last(5))
            .build()
            .expect("typed publisher builds");

        // generic: node.publisher(t).qos(..).generic(type, hash).build()
        let _generic = node
            .publisher("/chatter")
            .qos(QosSettings::default())
            .generic("std_msgs/msg/Int32", "hash")
            .build()
            .expect("generic publisher builds");
    }

    #[test]
    fn subscription_builder_and_convenient() {
        let mut exec: Executor = Executor::from_session(MockSession::new());
        let id = exec.node_builder("n").build().expect("node");

        // builder: typed
        let _h = exec
            .node_mut(id)
            .subscription("/chatter")
            .typed::<TestMsg>()
            .qos(QosSettings::default().keep_last(5))
            .build(|_m: &TestMsg| {})
            .expect("typed subscription builds");

        // builder: generic (raw bytes)
        let _g = exec
            .node_mut(id)
            .subscription("/raw")
            .generic("std_msgs/msg/Int32", "hash")
            .build(|_b: &[u8]| {})
            .expect("generic subscription builds");

        // convenient (fork tier) — one node-ctx at a time, re-acquired
        let _c = exec
            .node_mut(id)
            .create_subscription::<TestMsg, _>("/conv", |_m: &TestMsg| {})
            .expect("convenient typed subscription builds");
    }
}

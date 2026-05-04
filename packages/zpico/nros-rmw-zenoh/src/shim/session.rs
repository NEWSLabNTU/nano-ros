//! ZenohSession implementation

use nros_rmw::{
    QosSettings, ServiceInfo, Session, SessionMode, TopicInfo, TransportConfig, TransportError,
};

use super::{
    CONFIG_PROPERTY_SIZE, Context, LOCATOR_BUFFER_SIZE, LivelinessToken, MAX_SESSION_PROPERTIES,
    Ros2Liveliness, ZenohId,
    publisher::ZenohPublisher,
    service::{ZenohServiceClient, ZenohServiceServer},
    subscriber::ZenohSubscriber,
};

// ============================================================================
// ZenohSession
// ============================================================================

/// Zenoh session wrapping nros-rmw-zenoh Context
///
/// This session requires manual polling via `spin_once()` or `poll()`.
/// There are no background threads.
pub struct ZenohSession {
    context: Context,
}

impl ZenohSession {
    /// Create a new shim session with the given configuration
    ///
    /// # Arguments
    ///
    /// * `config` - Transport configuration with locator and mode
    ///
    /// # Returns
    ///
    /// A new session or error if connection fails
    pub fn new(config: &TransportConfig) -> Result<Self, TransportError> {
        // Build the locator string with null terminator
        let locator = match (&config.mode, config.locator) {
            (SessionMode::Client, Some(loc)) => {
                // Create null-terminated locator
                let mut buf = [0u8; LOCATOR_BUFFER_SIZE];
                let bytes = loc.as_bytes();
                if bytes.len() >= buf.len() {
                    return Err(TransportError::InvalidArgument);
                }
                buf[..bytes.len()].copy_from_slice(bytes);
                buf[bytes.len()] = 0; // Null terminator
                buf
            }
            (SessionMode::Client, None) => {
                return Err(TransportError::InvalidArgument);
            }
            (SessionMode::Peer, _) => {
                // Peer mode - pass null locator
                [0u8; LOCATOR_BUFFER_SIZE]
            }
        };

        // Build mode string
        let mode: &[u8] = match config.mode {
            SessionMode::Client => b"client\0",
            SessionMode::Peer => b"peer\0",
        };

        // Build null-terminated property strings on the stack
        // Each key/value is at most 64 bytes
        let mut key_bufs = [[0u8; CONFIG_PROPERTY_SIZE]; MAX_SESSION_PROPERTIES];
        let mut val_bufs = [[0u8; CONFIG_PROPERTY_SIZE]; MAX_SESSION_PROPERTIES];
        let mut c_props: [crate::zpico::zpico_property_t; MAX_SESSION_PROPERTIES] =
            unsafe { core::mem::zeroed() };

        let mut prop_count = 0usize;

        // Copy explicit properties from config
        for i in 0..config.properties.len().min(MAX_SESSION_PROPERTIES) {
            let (key, value) = config.properties[i];
            let key_bytes = key.as_bytes();
            let val_bytes = value.as_bytes();
            if key_bytes.len() >= CONFIG_PROPERTY_SIZE || val_bytes.len() >= CONFIG_PROPERTY_SIZE {
                continue; // Skip oversized properties
            }
            key_bufs[prop_count][..key_bytes.len()].copy_from_slice(key_bytes);
            key_bufs[prop_count][key_bytes.len()] = 0;
            val_bufs[prop_count][..val_bytes.len()].copy_from_slice(val_bytes);
            val_bufs[prop_count][val_bytes.len()] = 0;
            c_props[prop_count] = crate::zpico::zpico_property_t {
                key: key_bufs[prop_count].as_ptr().cast(),
                value: val_bufs[prop_count].as_ptr().cast(),
            };
            prop_count += 1;
        }

        // Read ZENOH_* env vars as defaults (explicit properties take precedence)
        #[cfg(feature = "std")]
        {
            let env_mappings: &[(&str, &str)] = &[
                ("ZENOH_MULTICAST_SCOUTING", "multicast_scouting"),
                ("ZENOH_SCOUTING_TIMEOUT", "scouting_timeout_ms"),
                ("ZENOH_LISTEN", "listen"),
                // TLS configuration
                ("ZENOH_TLS_ROOT_CA_CERTIFICATE", "root_ca_certificate"),
                (
                    "ZENOH_TLS_ROOT_CA_CERTIFICATE_BASE64",
                    "root_ca_certificate_base64",
                ),
                ("ZENOH_TLS_VERIFY_NAME_ON_CONNECT", "verify_name_on_connect"),
            ];
            for &(env_name, prop_key) in env_mappings {
                if let Ok(val) = std::env::var(env_name) {
                    let already_set = config.properties.iter().any(|(k, _)| *k == prop_key);
                    if !already_set && prop_count < MAX_SESSION_PROPERTIES {
                        let key_bytes = prop_key.as_bytes();
                        let val_bytes = val.as_bytes();
                        if key_bytes.len() < CONFIG_PROPERTY_SIZE
                            && val_bytes.len() < CONFIG_PROPERTY_SIZE
                        {
                            key_bufs[prop_count][..key_bytes.len()].copy_from_slice(key_bytes);
                            key_bufs[prop_count][key_bytes.len()] = 0;
                            val_bufs[prop_count][..val_bytes.len()].copy_from_slice(val_bytes);
                            val_bufs[prop_count][val_bytes.len()] = 0;
                            c_props[prop_count] = crate::zpico::zpico_property_t {
                                key: key_bufs[prop_count].as_ptr().cast(),
                                value: val_bufs[prop_count].as_ptr().cast(),
                            };
                            prop_count += 1;
                        }
                    }
                }
            }
        }

        let locator_opt = if config.mode == SessionMode::Peer && config.locator.is_none() {
            None
        } else {
            Some(locator.as_slice())
        };

        let context = Context::with_config(locator_opt, mode, &c_props[..prop_count])
            .map_err(TransportError::from)?;

        // Register the reply waker callback for async service client support
        super::service::register_reply_waker();

        Ok(Self { context })
    }

    /// Check if the session is open
    pub fn is_open(&self) -> bool {
        self.context.is_open()
    }

    /// Check if this backend requires polling
    ///
    /// For shim transport, this always returns true - manual polling is required.
    pub fn uses_polling(&self) -> bool {
        self.context.uses_polling()
    }

    /// Combined poll and keepalive operation
    ///
    /// This is the recommended way to drive the session. Call this
    /// periodically (e.g., every 10ms) from your main loop or RTIC task.
    ///
    /// # Arguments
    ///
    /// * `timeout_ms` - Maximum time to wait (0 = non-blocking)
    ///
    /// # Returns
    ///
    /// Number of events processed, or error
    pub fn spin_once(&self, timeout_ms: u32) -> Result<i32, TransportError> {
        self.context
            .spin_once(timeout_ms)
            .map_err(TransportError::from)
    }

    /// Get a reference to the underlying Context
    pub fn inner(&self) -> &Context {
        &self.context
    }

    /// Get the session's Zenoh ID
    ///
    /// The Zenoh ID uniquely identifies this session in the Zenoh network.
    /// It is used in liveliness token key expressions for ROS 2 discovery.
    pub fn zid(&self) -> Result<ZenohId, TransportError> {
        self.context.zid().map_err(TransportError::from)
    }

    /// Declare a liveliness token for ROS 2 discovery
    ///
    /// This creates a liveliness token at the given key expression,
    /// allowing ROS 2 nodes using rmw_zenoh to discover this entity.
    ///
    /// The key expression should be null-terminated.
    pub fn declare_liveliness(&self, keyexpr: &[u8]) -> Result<LivelinessToken, TransportError> {
        self.context
            .declare_liveliness(keyexpr)
            .map_err(TransportError::from)
    }

    /// Declare a node liveliness token for ROS 2 participant discovery
    ///
    /// Creates an NN liveliness token so ROS 2 tools (`ros2 node list`)
    /// can discover this node. The token is kept alive for the session lifetime.
    pub fn declare_node_liveliness(
        &self,
        domain_id: u32,
        namespace: &str,
        node_name: &str,
    ) -> Option<LivelinessToken> {
        self.declare_entity_liveliness(|zid| {
            Ros2Liveliness::node_keyexpr::<256>(domain_id, zid, namespace, node_name)
        })
    }

    /// Helper: build a liveliness keyexpr using a closure and declare it.
    fn declare_entity_liveliness(
        &self,
        build_keyexpr: impl FnOnce(&ZenohId) -> heapless::String<256>,
    ) -> Option<LivelinessToken> {
        let zid = self.context.zid().ok()?;
        let keyexpr = build_keyexpr(&zid);

        #[cfg(feature = "std")]
        log::debug!("liveliness keyexpr: {}", keyexpr.as_str());

        let mut buf = [0u8; 257];
        let bytes = keyexpr.as_bytes();
        if bytes.len() < buf.len() {
            buf[..bytes.len()].copy_from_slice(bytes);
            buf[bytes.len()] = 0;
            self.context.declare_liveliness(&buf[..=bytes.len()]).ok()
        } else {
            None
        }
    }
}

impl Session for ZenohSession {
    type Error = TransportError;
    type PublisherHandle = ZenohPublisher;
    type SubscriberHandle = ZenohSubscriber;
    type ServiceServerHandle = ZenohServiceServer;
    type ServiceClientHandle = ZenohServiceClient;

    fn create_publisher(
        &mut self,
        topic: &TopicInfo,
        qos: QosSettings,
    ) -> Result<Self::PublisherHandle, Self::Error> {
        let liveliness_token = topic.node_name.and_then(|node_name| {
            self.declare_entity_liveliness(|zid| {
                Ros2Liveliness::publisher_keyexpr::<256>(
                    topic.domain_id,
                    zid,
                    topic.namespace,
                    node_name,
                    topic,
                    &qos,
                )
            })
        });
        ZenohPublisher::new(&self.context, topic, liveliness_token, &qos)
    }

    fn create_subscriber(
        &mut self,
        topic: &TopicInfo,
        qos: QosSettings,
    ) -> Result<Self::SubscriberHandle, Self::Error> {
        let liveliness_token = topic.node_name.and_then(|node_name| {
            self.declare_entity_liveliness(|zid| {
                Ros2Liveliness::subscriber_keyexpr::<256>(
                    topic.domain_id,
                    zid,
                    topic.namespace,
                    node_name,
                    topic,
                    &qos,
                )
            })
        });
        ZenohSubscriber::new(&self.context, topic, liveliness_token, &qos)
    }

    fn create_service_server(
        &mut self,
        service: &ServiceInfo,
    ) -> Result<Self::ServiceServerHandle, Self::Error> {
        let liveliness_token = service.node_name.and_then(|node_name| {
            self.declare_entity_liveliness(|zid| {
                Ros2Liveliness::service_server_keyexpr::<256>(
                    service.domain_id,
                    zid,
                    service.namespace,
                    node_name,
                    service,
                    &QosSettings::services_default(),
                )
            })
        });
        ZenohServiceServer::new(&self.context, service, liveliness_token)
    }

    fn create_service_client(
        &mut self,
        service: &ServiceInfo,
    ) -> Result<Self::ServiceClientHandle, Self::Error> {
        let liveliness_token = service.node_name.and_then(|node_name| {
            self.declare_entity_liveliness(|zid| {
                Ros2Liveliness::service_client_keyexpr::<256>(
                    service.domain_id,
                    zid,
                    service.namespace,
                    node_name,
                    service,
                    &QosSettings::services_default(),
                )
            })
        });
        ZenohServiceClient::new(&self.context, service, liveliness_token)
    }

    fn close(&mut self) -> Result<(), Self::Error> {
        // Context is closed on drop
        Ok(())
    }

    fn drive_io(&mut self, timeout_ms: i32) -> Result<(), Self::Error> {
        self.spin_once(timeout_ms as u32).map(|_| ())
    }

    fn supported_qos_policies(&self) -> nros_rmw::QosPolicyMask {
        // Phase 108.B/C — zenoh-pico's wire protocol has no native
        // DDS QoS, so the shim emulates everything:
        // - Reliability maps to zenoh congestion-control (CORE).
        // - Durability VOLATILE / History / Depth honoured at the
        //   subscriber buffer level (CORE).
        // - DEADLINE: clock-based check at try_recv_raw (sub) /
        //   publish_raw (pub). 108.C.zenoh.2.
        // - LIFESPAN: subscriber filters samples whose attachment
        //   timestamp is older than `now - lifespan_ms`. 108.C.zenoh.3.
        // - LIVELINESS_AUTOMATIC: zenoh session keepalive covers
        //   "alive while the task lives" trivially. The default
        //   QosSettings sets `liveliness = AUTOMATIC`, so without
        //   this bit every entity create call fails QoS validation.
        //   MANUAL_BY_TOPIC / MANUAL_BY_NODE require an app-driven
        //   assert_liveliness keepalive timer; deferred to 108.C.zenoh.4.
        use nros_rmw::QosPolicyMask;
        QosPolicyMask::CORE
            | QosPolicyMask::DEADLINE
            | QosPolicyMask::LIFESPAN
            | QosPolicyMask::LIVELINESS_AUTOMATIC
    }
}

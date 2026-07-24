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

#[cfg(feature = "std")]
fn append_locator_param(
    buf: &mut [u8; LOCATOR_BUFFER_SIZE],
    len: &mut usize,
    first_param: &mut bool,
    key: &str,
    value: &str,
) -> Result<(), TransportError> {
    let separator = if *first_param { b'#' } else { b';' };
    let needed = 1 + key.len() + 1 + value.len();
    if *len + needed >= buf.len() {
        return Err(TransportError::InvalidArgument);
    }
    buf[*len] = separator;
    *len += 1;
    buf[*len..*len + key.len()].copy_from_slice(key.as_bytes());
    *len += key.len();
    buf[*len] = b'=';
    *len += 1;
    buf[*len..*len + value.len()].copy_from_slice(value.as_bytes());
    *len += value.len();
    *first_param = false;
    Ok(())
}

#[cfg(feature = "std")]
fn append_tls_env_to_locator(
    loc: &str,
    buf: &mut [u8; LOCATOR_BUFFER_SIZE],
    len: &mut usize,
) -> Result<(), TransportError> {
    if !loc.starts_with("tls/") {
        return Ok(());
    }

    let mut first_param = !loc.contains('#');
    if !loc.contains("root_ca_certificate=")
        && let Ok(value) = std::env::var("ZENOH_TLS_ROOT_CA_CERTIFICATE")
    {
        append_locator_param(buf, len, &mut first_param, "root_ca_certificate", &value)?;
    }
    if !loc.contains("root_ca_certificate_base64=")
        && let Ok(value) = std::env::var("ZENOH_TLS_ROOT_CA_CERTIFICATE_BASE64")
    {
        append_locator_param(
            buf,
            len,
            &mut first_param,
            "root_ca_certificate_base64",
            &value,
        )?;
    }
    if !loc.contains("verify_name_on_connect=")
        && let Ok(value) = std::env::var("ZENOH_TLS_VERIFY_NAME_ON_CONNECT")
    {
        append_locator_param(buf, len, &mut first_param, "verify_name_on_connect", &value)?;
    }
    Ok(())
}

// ============================================================================
// ZenohSession
// ============================================================================

/// Maximum number of distinct per-node NN liveliness tokens the session will
/// hold.  Derived from `NROS_EXECUTOR_MAX_NODES` (build.rs) so it tracks the
/// executor's per-process node cap — one session hosts at most that many graph
/// nodes, so a token is never silently dropped on overflow at the default cap.
use crate::config::MAX_PER_NODE_LIVELINESS;

/// Zenoh session wrapping nros-rmw-zenoh Context
///
/// This session requires manual polling via `spin_once()` or `poll()`.
/// There are no background threads.
pub struct ZenohSession {
    context: Context,
    /// Fix #104 — node liveliness token declared at session open so the
    /// primary node is visible in `ros2 node list`.  A dropped
    /// `LivelinessToken` is immediately undeclared, so we MUST hold it
    /// for the entire session lifetime.  `None` when `config.node_name`
    /// is empty or `should_declare_liveliness()` returns `false`.
    node_liveliness: Option<LivelinessToken>,
    /// The node name used for the #104 primary token (from
    /// `TransportConfig::node_name` at open).  Stored so that
    /// `drop_primary_node_liveliness_if_superseded` can compare it against
    /// per-node names from W2 entity creation and decide whether to drop the
    /// primary (multi-node case) or keep it (single-node case).
    primary_node_name: heapless::String<64>,
    /// Phase 268 W2 — one NN liveliness token per distinct node name seen
    /// via entity creation, so each launch component appears as its own node
    /// in `ros2 node list`.  Bounded; held for the session lifetime (dropping
    /// a token undeclares it).
    per_node_liveliness:
        heapless::Vec<(heapless::String<64>, LivelinessToken), MAX_PER_NODE_LIVELINESS>,
    /// Phase 124.B.3 — executor wake callback. Installed by
    /// `set_wake_callback`; non-null after the runtime has wired
    /// the executor through the cffi vtable. Invoked by
    /// `drive_io` after work was observed; the runtime cb does
    /// flag-write + condvar-signal atomically so the executor
    /// wakes from `wake_cv` instead of polling on a deadline.
    wake_cb: core::sync::atomic::AtomicPtr<core::ffi::c_void>,
    wake_ctx: core::sync::atomic::AtomicPtr<core::ffi::c_void>,
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
                #[cfg(feature = "std")]
                let len = {
                    let mut len = bytes.len();
                    append_tls_env_to_locator(loc, &mut buf, &mut len)?;
                    len
                };
                #[cfg(not(feature = "std"))]
                let len = bytes.len();
                buf[len] = 0; // Null terminator
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

        // Fix #104 — declare the node liveliness token so the primary node
        // appears in `ros2 node list`.  Build the session first (token is
        // None), then immediately declare it while the session is live.
        let mut primary_node_name: heapless::String<64> = heapless::String::new();
        let _ = primary_node_name.push_str(config.node_name);
        let mut session = Self {
            context,
            node_liveliness: None,
            primary_node_name,
            per_node_liveliness: heapless::Vec::new(),
            wake_cb: core::sync::atomic::AtomicPtr::new(core::ptr::null_mut()),
            wake_ctx: core::sync::atomic::AtomicPtr::new(core::ptr::null_mut()),
        };

        if !config.node_name.is_empty() {
            // Treat empty namespace as root "/" — that is what the keyexpr
            // builder expects for a top-level node.
            let ns = if config.namespace.is_empty() {
                "/"
            } else {
                config.namespace
            };
            session.node_liveliness =
                session.declare_node_liveliness(config.domain_id, ns, config.node_name);
        }

        Ok(session)
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
        if !self.should_declare_liveliness() {
            return None;
        }

        self.declare_entity_liveliness(|zid| {
            Ros2Liveliness::node_keyexpr::<256>(domain_id, zid, namespace, node_name)
        })
    }

    #[inline]
    fn should_declare_liveliness(&self) -> bool {
        // FreeRTOS QEMU/slirp peer-to-peer fixtures do not need ROS 2
        // discovery tokens for data routing, and current zenoh-pico FreeRTOS
        // liveliness declaration can block once another peer is present.
        !cfg!(feature = "platform-freertos")
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

    /// Phase 268 W2 — lazily declare a per-node NN liveliness token the first
    /// time an entity for `node_name` is created.
    ///
    /// Subsequent calls for the same name are no-ops (dedup).  The token is
    /// held in `per_node_liveliness` for the session lifetime; dropping it
    /// would undeclare the node in `ros2 node list`.
    // Issue #143 — the #129-era Zephyr gate here is LIFTED: the "deadlock"
    // this declare hit was the #139 socket-timeout starvation (5 s recv
    // window serializing every tx), fixed at the root. Per-node tokens are
    // back on every platform, restoring per-component `ros2 node list`
    // fidelity on Zephyr.
    fn ensure_node_liveliness(&mut self, domain_id: u32, namespace: &str, node_name: &str) {
        if node_name.is_empty() {
            return;
        }
        // Dedup: already declared for this name.
        if self
            .per_node_liveliness
            .iter()
            .any(|(n, _)| n.as_str() == node_name)
        {
            return;
        }
        // Treat empty namespace as root "/" — same as the #104 primary path.
        let ns = if namespace.is_empty() { "/" } else { namespace };
        if let Some(tok) = self.declare_node_liveliness(domain_id, ns, node_name) {
            let mut key: heapless::String<64> = heapless::String::new();
            let _ = key.push_str(node_name);
            let _ = self.per_node_liveliness.push((key, tok)); // silent overflow past MAX
            // Gate: a per-node token with a DIFFERENT name supersedes the
            // primary `/node` phantom — drop it so multi-node launches show
            // only their components, not a spurious "node" entry.
            self.drop_primary_node_liveliness_if_superseded(node_name);
        }
    }

    /// Phase 268 W2 — drop the #104 primary node liveliness token when a
    /// per-node token with a DIFFERENT name has been declared.
    ///
    /// **Single-node case**: the one component's name matches the primary name
    /// (e.g. primary `"talker"` + per-node `"talker"`) → keep primary.
    ///
    /// **Multi-node case**: primary generic name (e.g. `"node"`) differs from
    /// the per-node name (e.g. `"talker"`) → drop primary → `ros2 node list`
    /// shows only `/talker` + `/listener`, not a spurious `/node`.
    fn drop_primary_node_liveliness_if_superseded(&mut self, new_name: &str) {
        if self.node_liveliness.is_some()
            && !self.primary_node_name.is_empty()
            && self.primary_node_name.as_str() != new_name
        {
            self.node_liveliness = None; // Drop → immediately undeclares the NN token.
        }
    }
}

impl Session for ZenohSession {
    type Error = TransportError;
    type PublisherHandle = ZenohPublisher;
    type SubscriptionHandle = ZenohSubscriber;
    type ServiceHandle = ZenohServiceServer;
    type ClientHandle = ZenohServiceClient;

    fn create_publisher(
        &mut self,
        topic: &TopicInfo,
        qos: QosSettings,
    ) -> Result<Self::PublisherHandle, Self::Error> {
        let mut publisher = ZenohPublisher::new(&self.context, topic, None, &qos)?;
        // Phase 268 W2 — ensure a per-node NN token for this publisher's node.
        if let Some(node_name) = topic.node_name {
            self.ensure_node_liveliness(topic.domain_id, topic.namespace, node_name);
        }
        let liveliness_token = self
            .should_declare_liveliness()
            .then_some(())
            .and_then(|_| {
                topic.node_name.and_then(|node_name| {
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
                })
            });
        publisher.set_liveliness(liveliness_token);
        Ok(publisher)
    }

    fn create_subscription(
        &mut self,
        topic: &TopicInfo,
        qos: QosSettings,
    ) -> Result<Self::SubscriptionHandle, Self::Error> {
        let mut subscriber = ZenohSubscriber::new(&self.context, topic, None, &qos)?;
        // Phase 268 W2 — ensure a per-node NN token for this subscriber's node.
        if let Some(node_name) = topic.node_name {
            self.ensure_node_liveliness(topic.domain_id, topic.namespace, node_name);
        }
        let liveliness_token = self
            .should_declare_liveliness()
            .then_some(())
            .and_then(|_| {
                topic.node_name.and_then(|node_name| {
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
                })
            });
        subscriber.set_liveliness(liveliness_token);
        Ok(subscriber)
    }

    fn create_service(
        &mut self,
        service: &ServiceInfo,
        qos: QosSettings,
    ) -> Result<Self::ServiceHandle, Self::Error> {
        // TODO(193.1b): zenoh-pico services have no endpoint-level QoS
        // slot (the `None` below is the liveliness token, not QoS) — the
        // requested service QoS cannot be applied to the queryable yet.
        // Thread it through once zenoh-pico exposes per-endpoint QoS.
        let _ = qos;
        let mut server = ZenohServiceServer::new(&self.context, service, None)?;
        // Phase 268 W2 — ensure a per-node NN token for this server's node.
        if let Some(node_name) = service.node_name {
            self.ensure_node_liveliness(service.domain_id, service.namespace, node_name);
        }
        let liveliness_token = self
            .should_declare_liveliness()
            .then_some(())
            .and_then(|_| {
                service.node_name.and_then(|node_name| {
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
                })
            });
        server.set_liveliness(liveliness_token);
        Ok(server)
    }

    fn create_client(
        &mut self,
        service: &ServiceInfo,
        qos: QosSettings,
    ) -> Result<Self::ClientHandle, Self::Error> {
        // TODO(193.1b): zenoh-pico services have no endpoint-level QoS
        // slot — the requested service QoS cannot be applied to the
        // querier yet. Thread it once zenoh-pico exposes per-endpoint QoS.
        let _ = qos;
        // Phase 268 W2 — ensure a per-node NN token for this client's node.
        if let Some(node_name) = service.node_name {
            self.ensure_node_liveliness(service.domain_id, service.namespace, node_name);
        }
        let liveliness_token = self
            .should_declare_liveliness()
            .then_some(())
            .and_then(|_| {
                service.node_name.and_then(|node_name| {
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
                })
            });
        ZenohServiceClient::new(&self.context, service, liveliness_token)
    }

    fn close(&mut self) -> Result<(), Self::Error> {
        // Context is closed on drop
        Ok(())
    }

    fn drive_io(&mut self, timeout_ms: i32) -> Result<(), Self::Error> {
        let res = self.spin_once(timeout_ms as u32);
        // Phase 124.B.3 — when zenoh's spin_once observed any work
        // (n > 0), call the runtime-supplied wake callback so the
        // executor's `wake_cv` is signalled (flag-write +
        // condvar-signal happen atomically inside the cb).
        // Best-effort: if the cb hasn't been installed yet the
        // executor still drains via its deadline-bound cv-wait.
        if matches!(res, Ok(n) if n > 0) {
            let cb = self.wake_cb.load(core::sync::atomic::Ordering::Acquire);
            if !cb.is_null() {
                let ctx = self.wake_ctx.load(core::sync::atomic::Ordering::Acquire);
                // SAFETY: cb was installed via `set_wake_callback`
                // and points at a runtime-owned function; ctx
                // points at WakeCtx allocated for the executor's
                // lifetime.
                let f: unsafe extern "C" fn(*mut core::ffi::c_void) =
                    unsafe { core::mem::transmute(cb) };
                unsafe { f(ctx) };
            }
        }
        res.map(|_| ())
    }

    unsafe fn set_wake_callback(
        &mut self,
        cb: Option<unsafe extern "C" fn(ctx: *mut core::ffi::c_void)>,
        ctx: *mut core::ffi::c_void,
    ) {
        let cb_ptr = cb
            .map(|f| f as *mut core::ffi::c_void)
            .unwrap_or(core::ptr::null_mut());
        self.wake_cb
            .store(cb_ptr, core::sync::atomic::Ordering::Release);
        self.wake_ctx
            .store(ctx, core::sync::atomic::Ordering::Release);
    }

    /// Phase 110.0 — bound the executor's `drive_io` wait against
    /// zenoh-pico's transport keepalive interval.
    ///
    /// zenoh-pico does not expose its internal "next keepalive
    /// timestamp" through FFI, so the shim returns a conservative
    /// upper-bound: `Z_TRANSPORT_LEASE / Z_TRANSPORT_LEASE_EXPIRE_FACTOR`.
    /// With the upstream defaults (lease = 10 000 ms, factor = 3) that
    /// caps wake-late to ~3.3 s on a quiet link — the runtime never
    /// blocks longer than one keepalive interval before returning
    /// control to the executor. Tracking the precise per-call
    /// timestamp would need a hook in `_z_send_keep_alive`; out of
    /// scope for the v1 surface.
    fn next_deadline_ms(&self) -> Option<u32> {
        // Z_TRANSPORT_LEASE = 10000 ms, Z_TRANSPORT_LEASE_EXPIRE_FACTOR = 3
        const ZENOH_KEEPALIVE_INTERVAL_MS: u32 = 10_000 / 3;
        Some(ZENOH_KEEPALIVE_INTERVAL_MS)
    }

    fn ping_session(&mut self, _timeout_ms: i32) -> Result<(), Self::Error> {
        // Phase 124.F.2 — zenoh-pico's closest match to a true ping
        // is `zp_send_keep_alive`. Fire one frame; success means the
        // transport accepted it (TCP / serial / shm send returned OK,
        // i.e. the link is still alive from the local side). Failure
        // surfaces as `Timeout` per the 124.F.1 semantics so callers
        // can tear down + re-open the session on a dead link.
        //
        // The `timeout_ms` argument is ignored — the underlying call
        // is synchronous and non-blocking. We don't honour the budget
        // because the call returns within microseconds either way.
        // True round-trip ping would need a `z_send_ping` API that
        // zenoh-pico hasn't yet exposed; deferred to a follow-up
        // when upstream lands one.
        let rc = unsafe { zpico_sys::zpico_send_keep_alive() };
        if rc == 0 {
            Ok(())
        } else {
            Err(TransportError::Timeout)
        }
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
        // - LIVELINESS_AUTOMATIC: zenoh runtime declares the token
        //   automatically when the publisher is created
        //   (`Ros2Liveliness::publisher_keyexpr`); subscribers track
        //   alive-state via a periodic poll of the wildcard liveliness
        //   keyexpr. Per-publisher count surfaced via
        //   `zpico_liveliness_get_count` (108.C.zenoh.4-followup).
        // - LIVELINESS_MANUAL_BY_TOPIC / MANUAL_BY_NODE: shim-side
        //   keepalive timer. `Publisher::assert_liveliness()`
        //   refreshes the lease; `publish_raw` checks for expiry and
        //   fires `LivelinessLost` rate-limited to ≤ 1 per lease.
        //   (108.C.zenoh.4-followup).
        // - LIVELINESS_LEASE: caller-supplied lease duration honoured
        //   for all liveliness kinds.
        use nros_rmw::QosPolicyMask;
        QosPolicyMask::CORE
            | QosPolicyMask::DEADLINE
            | QosPolicyMask::LIFESPAN
            | QosPolicyMask::LIVELINESS_AUTOMATIC
            | QosPolicyMask::LIVELINESS_MANUAL_BY_TOPIC
            | QosPolicyMask::LIVELINESS_MANUAL_BY_NODE
            | QosPolicyMask::LIVELINESS_LEASE
    }
}

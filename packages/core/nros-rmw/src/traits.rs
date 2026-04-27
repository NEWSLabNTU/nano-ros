//! Transport abstraction traits.
//!
//! Defines the backend-agnostic interface that transport implementations
//! (zenoh-pico, XRCE-DDS) must satisfy. The core trait hierarchy is:
//!
//! - [`Session`] — connection lifecycle and handle creation
//! - [`Publisher`] / [`Subscriber`] — pub/sub data transport
//! - [`ServiceServerTrait`] / [`ServiceClientTrait`] — request/reply
//! - [`Rmw`] — top-level factory that creates sessions

use nros_core::{Deserialize, RosMessage, RosService, Serialize};

/// Topic information for pub/sub
#[derive(Debug, Clone)]
pub struct TopicInfo<'a> {
    /// Topic name (e.g., "/chatter")
    pub name: &'a str,
    /// ROS type name (e.g., "std_msgs::msg::dds_::String_")
    pub type_name: &'a str,
    /// Type hash for compatibility checking
    pub type_hash: &'a str,
    /// Domain ID (default: 0)
    pub domain_id: u32,
    /// Node name for liveliness token generation.
    /// `None` means no node association — no liveliness token will be declared.
    pub node_name: Option<&'a str>,
    /// Node namespace for liveliness token generation (default: "/").
    /// In ROS 2, "/" is the root namespace and the standard default.
    pub namespace: &'a str,
}

impl<'a> TopicInfo<'a> {
    /// Create new topic info
    pub const fn new(name: &'a str, type_name: &'a str, type_hash: &'a str) -> Self {
        Self {
            name,
            type_name,
            type_hash,
            domain_id: 0,
            node_name: None,
            namespace: "/",
        }
    }

    /// Create topic info with custom domain ID
    pub const fn with_domain(mut self, domain_id: u32) -> Self {
        self.domain_id = domain_id;
        self
    }

    /// Set the node name for liveliness token generation
    pub const fn with_node_name(mut self, node_name: &'a str) -> Self {
        self.node_name = Some(node_name);
        self
    }

    /// Set the node namespace for liveliness token generation
    pub const fn with_namespace(mut self, namespace: &'a str) -> Self {
        self.namespace = namespace;
        self
    }
}

/// Service information for service client/server
#[derive(Debug, Clone)]
pub struct ServiceInfo<'a> {
    /// Service name (e.g., "/add_two_ints")
    pub name: &'a str,
    /// ROS service type name (e.g., "example_interfaces::srv::dds_::AddTwoInts_")
    pub type_name: &'a str,
    /// Type hash for compatibility checking
    pub type_hash: &'a str,
    /// Domain ID (default: 0)
    pub domain_id: u32,
    /// Node name for liveliness token generation.
    /// `None` means no node association — no liveliness token will be declared.
    pub node_name: Option<&'a str>,
    /// Node namespace for liveliness token generation (default: "/").
    /// In ROS 2, "/" is the root namespace and the standard default.
    pub namespace: &'a str,
}

/// Action information for action client/server
///
/// Actions in ROS 2 use 5 communication channels:
/// - `send_goal` service: `<action_name>/_action/send_goal`
/// - `cancel_goal` service: `<action_name>/_action/cancel_goal`
/// - `get_result` service: `<action_name>/_action/get_result`
/// - `feedback` topic: `<action_name>/_action/feedback`
/// - `status` topic: `<action_name>/_action/status`
#[derive(Debug, Clone)]
pub struct ActionInfo<'a> {
    /// Action name (e.g., "/fibonacci")
    pub name: &'a str,
    /// ROS action type name (e.g., "example_interfaces::action::dds_::Fibonacci_")
    pub type_name: &'a str,
    /// Type hash for compatibility checking
    pub type_hash: &'a str,
    /// Domain ID (default: 0)
    pub domain_id: u32,
}

impl<'a> ActionInfo<'a> {
    /// Create new action info
    pub const fn new(name: &'a str, type_name: &'a str, type_hash: &'a str) -> Self {
        Self {
            name,
            type_name,
            type_hash,
            domain_id: 0,
        }
    }

    /// Create action info with custom domain ID
    pub const fn with_domain(mut self, domain_id: u32) -> Self {
        self.domain_id = domain_id;
        self
    }

    /// Generate the send_goal service name
    /// Returns: `<action>/_action/send_goal`
    pub fn send_goal_key<const N: usize>(&self) -> heapless::String<N> {
        self.sub_name::<N>("send_goal")
    }

    /// Generate the cancel_goal service name
    /// Returns: `<action>/_action/cancel_goal`
    pub fn cancel_goal_key<const N: usize>(&self) -> heapless::String<N> {
        self.sub_name::<N>("cancel_goal")
    }

    /// Generate the get_result service name
    /// Returns: `<action>/_action/get_result`
    pub fn get_result_key<const N: usize>(&self) -> heapless::String<N> {
        self.sub_name::<N>("get_result")
    }

    /// Generate the feedback topic name
    /// Returns: `<action>/_action/feedback`
    pub fn feedback_key<const N: usize>(&self) -> heapless::String<N> {
        self.sub_name::<N>("feedback")
    }

    /// Generate the status topic name
    /// Returns: `<action>/_action/status`
    pub fn status_key<const N: usize>(&self) -> heapless::String<N> {
        self.sub_name::<N>("status")
    }

    /// Generate a sub-entity name for an action component
    /// Returns: `<action>/_action/<suffix>` (e.g., `fibonacci/_action/send_goal`)
    ///
    /// The caller is responsible for constructing the full key expression
    /// by wrapping this name in a `ServiceInfo` or `TopicInfo` with the
    /// correct sub-service/sub-topic type name.
    fn sub_name<const N: usize>(&self, suffix: &str) -> heapless::String<N> {
        let mut name = heapless::String::new();
        let action_stripped = self.name.trim_matches('/');
        let _ = core::fmt::write(
            &mut name,
            format_args!("/{}/_action/{}", action_stripped, suffix),
        );
        name
    }
}

impl<'a> ServiceInfo<'a> {
    /// Create new service info
    pub const fn new(name: &'a str, type_name: &'a str, type_hash: &'a str) -> Self {
        Self {
            name,
            type_name,
            type_hash,
            domain_id: 0,
            node_name: None,
            namespace: "/",
        }
    }

    /// Create service info with custom domain ID
    pub const fn with_domain(mut self, domain_id: u32) -> Self {
        self.domain_id = domain_id;
        self
    }

    /// Set the node name for liveliness token generation
    pub const fn with_node_name(mut self, node_name: &'a str) -> Self {
        self.node_name = Some(node_name);
        self
    }

    /// Set the node namespace for liveliness token generation
    pub const fn with_namespace(mut self, namespace: &'a str) -> Self {
        self.namespace = namespace;
        self
    }
}

/// Transport error types.
///
/// No longer `Copy` — the `Backend` / `BackendDynamic` variants carry a
/// string diagnostic, which can't be `Copy`. Rust callers that used to
/// copy a `TransportError` value repeatedly now need `.clone()` or
/// `ref` in match arms. C/C++ callers are unaffected — both map
/// `TransportError` to integer codes (`nros_ret_t` / `ErrorCode`)
/// before crossing the FFI boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportError {
    /// Failed to connect to transport
    ConnectionFailed,
    /// Connection was closed
    Disconnected,
    /// Failed to create publisher
    PublisherCreationFailed,
    /// Failed to create subscriber
    SubscriberCreationFailed,
    /// Failed to create service server
    ServiceServerCreationFailed,
    /// Failed to create service client
    ServiceClientCreationFailed,
    /// Failed to publish message
    PublishFailed,
    /// Failed to send service request
    ServiceRequestFailed,
    /// Failed to send service reply
    ServiceReplyFailed,
    /// Serialization error
    SerializationError,
    /// Deserialization error
    DeserializationError,
    /// Buffer too small
    BufferTooSmall,
    /// Incoming message exceeded the static buffer capacity
    MessageTooLarge,
    /// Timeout waiting for message
    Timeout,
    /// Invalid configuration
    InvalidConfig,
    /// Failed to start background tasks
    TaskStartFailed,
    /// Failed to poll for incoming messages
    PollFailed,
    /// Failed to send keepalive
    KeepaliveFailed,
    /// Failed to send join message
    JoinFailed,
    /// Backend-specific error with a `'static` diagnostic string.
    ///
    /// Useful for zenoh-pico / XRCE-DDS return codes that map to a
    /// fixed set of known messages. `no_std`-compatible.
    Backend(&'static str),
    /// Backend-specific error with an owned diagnostic string.
    ///
    /// Available only with the `alloc` feature. Use this when the
    /// diagnostic is formatted at runtime (e.g. from a C error code
    /// plus a socket address).
    #[cfg(feature = "alloc")]
    BackendDynamic(alloc::string::String),
}

/// QoS history policy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QosHistoryPolicy {
    /// Keep last N messages (where N is defined in QosSettings)
    #[default]
    KeepLast,
    /// Keep all messages (up to resource limits)
    KeepAll,
}

/// QoS reliability policy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QosReliabilityPolicy {
    /// Reliable delivery (retransmit if needed).
    ///
    /// Default — matches ROS 2 `rmw_qos_profile_default` and the
    /// `QosSettings::default()` / `QOS_PROFILE_DEFAULT` aggregates.
    #[default]
    Reliable,
    /// Best-effort delivery (no retransmits)
    BestEffort,
}

/// QoS durability policy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QosDurabilityPolicy {
    /// Messages are discarded when subscriber disconnects
    #[default]
    Volatile,
    /// Messages are persisted for late-joining subscribers
    TransientLocal,
}

/// QoS (Quality of Service) settings with builder pattern
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QosSettings {
    /// History policy
    pub history: QosHistoryPolicy,
    /// Reliability policy
    pub reliability: QosReliabilityPolicy,
    /// Durability policy
    pub durability: QosDurabilityPolicy,
    /// History depth (only used if history is KeepLast)
    pub depth: u32,
}

impl Default for QosSettings {
    fn default() -> Self {
        Self::QOS_PROFILE_DEFAULT
    }
}

impl QosSettings {
    /// Create new QoS settings with defaults (matches `QOS_PROFILE_DEFAULT`:
    /// Reliable, Volatile, KeepLast(10)).
    pub const fn new() -> Self {
        Self {
            history: QosHistoryPolicy::KeepLast,
            reliability: QosReliabilityPolicy::Reliable,
            durability: QosDurabilityPolicy::Volatile,
            depth: 10,
        }
    }

    /// Best-effort QoS (for real-time)
    pub const BEST_EFFORT: Self = Self {
        history: QosHistoryPolicy::KeepLast,
        reliability: QosReliabilityPolicy::BestEffort,
        durability: QosDurabilityPolicy::Volatile,
        depth: 1,
    };

    /// Reliable QoS
    pub const RELIABLE: Self = Self {
        history: QosHistoryPolicy::KeepLast,
        reliability: QosReliabilityPolicy::Reliable,
        durability: QosDurabilityPolicy::Volatile,
        depth: 10,
    };

    /// System default QoS profile (matches rmw_qos_profile_system_default)
    pub const QOS_PROFILE_SYSTEM_DEFAULT: Self = Self {
        history: QosHistoryPolicy::KeepLast,
        reliability: QosReliabilityPolicy::Reliable,
        durability: QosDurabilityPolicy::Volatile,
        depth: 1,
    };

    /// Default QoS profile (matches rmw_qos_profile_default)
    pub const QOS_PROFILE_DEFAULT: Self = Self {
        history: QosHistoryPolicy::KeepLast,
        reliability: QosReliabilityPolicy::Reliable,
        durability: QosDurabilityPolicy::Volatile,
        depth: 10,
    };

    /// Sensor data QoS profile (matches rmw_qos_profile_sensor_data)
    pub const QOS_PROFILE_SENSOR_DATA: Self = Self {
        history: QosHistoryPolicy::KeepLast,
        reliability: QosReliabilityPolicy::BestEffort,
        durability: QosDurabilityPolicy::Volatile,
        depth: 5,
    };

    /// Services default QoS profile (matches rmw_qos_profile_services_default)
    pub const QOS_PROFILE_SERVICES_DEFAULT: Self = Self {
        history: QosHistoryPolicy::KeepLast,
        reliability: QosReliabilityPolicy::Reliable,
        durability: QosDurabilityPolicy::Volatile,
        depth: 10,
    };

    /// Parameters QoS profile (matches rmw_qos_profile_parameters)
    pub const QOS_PROFILE_PARAMETERS: Self = Self {
        history: QosHistoryPolicy::KeepLast,
        reliability: QosReliabilityPolicy::Reliable,
        durability: QosDurabilityPolicy::TransientLocal,
        depth: 1000,
    };

    /// Clock QoS profile - same as sensor data but with depth 1
    pub const QOS_PROFILE_CLOCK: Self = Self {
        history: QosHistoryPolicy::KeepLast,
        reliability: QosReliabilityPolicy::BestEffort,
        durability: QosDurabilityPolicy::Volatile,
        depth: 1,
    };

    /// Parameter events QoS profile (matches rmw_qos_profile_parameter_events)
    pub const QOS_PROFILE_PARAMETER_EVENTS: Self = Self {
        history: QosHistoryPolicy::KeepAll,
        reliability: QosReliabilityPolicy::Reliable,
        durability: QosDurabilityPolicy::Volatile,
        depth: 0, // Not used with KeepAll
    };

    /// Action status default QoS profile (matches rcl_action_qos_profile_status_default)
    pub const QOS_PROFILE_ACTION_STATUS_DEFAULT: Self = Self {
        history: QosHistoryPolicy::KeepLast,
        reliability: QosReliabilityPolicy::Reliable,
        durability: QosDurabilityPolicy::TransientLocal,
        depth: 1,
    };

    // --- Static constructor methods (matching rclrs API) ---

    /// Get the default QoS profile for ordinary topics
    pub const fn topics_default() -> Self {
        Self::QOS_PROFILE_DEFAULT
    }

    /// Get the default QoS profile for sensor data topics
    pub const fn sensor_data_default() -> Self {
        Self::QOS_PROFILE_SENSOR_DATA
    }

    /// Get the default QoS profile for services
    pub const fn services_default() -> Self {
        Self::QOS_PROFILE_SERVICES_DEFAULT
    }

    /// Get the default QoS profile for parameter services
    pub const fn parameters_default() -> Self {
        Self::QOS_PROFILE_PARAMETERS
    }

    /// Get the default QoS profile for parameter events
    pub const fn parameter_events_default() -> Self {
        Self::QOS_PROFILE_PARAMETER_EVENTS
    }

    /// Get the system default QoS profile
    pub const fn system_default() -> Self {
        Self::QOS_PROFILE_SYSTEM_DEFAULT
    }

    /// Get the default QoS profile for action status topics
    pub const fn action_status_default() -> Self {
        Self::QOS_PROFILE_ACTION_STATUS_DEFAULT
    }

    /// Get the default QoS profile for clock topics
    pub const fn clock_default() -> Self {
        Self::QOS_PROFILE_CLOCK
    }

    // --- Builder methods ---

    /// Set history to keep last N messages
    pub const fn keep_last(mut self, depth: u32) -> Self {
        self.history = QosHistoryPolicy::KeepLast;
        self.depth = depth;
        self
    }

    /// Set history to keep all messages
    pub const fn keep_all(mut self) -> Self {
        self.history = QosHistoryPolicy::KeepAll;
        self
    }

    /// Set reliability to reliable
    pub const fn reliable(mut self) -> Self {
        self.reliability = QosReliabilityPolicy::Reliable;
        self
    }

    /// Set reliability to best-effort
    pub const fn best_effort(mut self) -> Self {
        self.reliability = QosReliabilityPolicy::BestEffort;
        self
    }

    /// Set durability to volatile
    pub const fn volatile(mut self) -> Self {
        self.durability = QosDurabilityPolicy::Volatile;
        self
    }

    /// Set durability to transient local
    pub const fn transient_local(mut self) -> Self {
        self.durability = QosDurabilityPolicy::TransientLocal;
        self
    }

    /// Set reliability policy explicitly
    pub const fn reliability(mut self, policy: QosReliabilityPolicy) -> Self {
        self.reliability = policy;
        self
    }

    /// Set durability policy explicitly
    pub const fn durability(mut self, policy: QosDurabilityPolicy) -> Self {
        self.durability = policy;
        self
    }

    /// Set history policy explicitly
    pub const fn history(mut self, policy: QosHistoryPolicy) -> Self {
        self.history = policy;
        self
    }

    /// Set history depth explicitly
    pub const fn depth(mut self, depth: u32) -> Self {
        self.depth = depth;
        self
    }

    /// Get history depth (for backwards compatibility)
    pub const fn history_depth(&self) -> u8 {
        if self.depth > 255 {
            255
        } else {
            self.depth as u8
        }
    }
}

/// Transport session configuration
#[derive(Debug, Clone)]
pub struct TransportConfig<'a> {
    /// Peer locator (e.g., "tcp/192.168.1.1:7447" or "serial//dev/ttyUSB0#baudrate=115200")
    pub locator: Option<&'a str>,
    /// Session mode: client, peer, or router
    pub mode: SessionMode,
    /// Additional transport properties (key-value pairs)
    ///
    /// These are passed through to the underlying transport backend.
    /// For zenoh-pico, recognized keys include:
    /// - `"multicast_scouting"` - Enable/disable multicast scouting (`"true"` or `"false"`)
    /// - `"scouting_timeout_ms"` - Scouting timeout in milliseconds
    /// - `"multicast_locator"` - Multicast group address
    /// - `"listen"` - Listen endpoint (e.g., `"tcp/0.0.0.0:0"`)
    /// - `"add_timestamp"` - Add timestamps to messages (`"true"` or `"false"`)
    pub properties: &'a [(&'a str, &'a str)],
}

impl Default for TransportConfig<'_> {
    fn default() -> Self {
        Self {
            locator: None,
            mode: SessionMode::Client,
            properties: &[],
        }
    }
}

/// Middleware-agnostic session configuration.
///
/// `RmwConfig` provides a uniform interface that any RMW backend can
/// interpret. Backends map the universal fields to their own connection
/// parameters and interpret `properties` for anything backend-specific.
///
/// # Examples
///
/// ```
/// use nros_rmw::{RmwConfig, SessionMode};
///
/// let config = RmwConfig {
///     locator: "tcp/192.168.1.1:7447",
///     mode: SessionMode::Client,
///     domain_id: 0,
///     node_name: "talker",
///     namespace: "",
///     properties: &[],
/// };
/// ```
#[derive(Debug, Clone, Copy)]
pub struct RmwConfig<'a> {
    /// Middleware-specific connection string.
    ///
    /// - zenoh: `"tcp/192.168.1.1:7447"` or `"udp/224.0.0.224:7447"`
    /// - XRCE-DDS: `"udp/192.168.1.1:2019"`
    pub locator: &'a str,
    /// Session mode (zenoh: client/peer; XRCE-DDS: always client)
    pub mode: SessionMode,
    /// ROS 2 domain ID (maps to DDS domain or zenoh key prefix)
    pub domain_id: u32,
    /// Node name (e.g., `"talker"`)
    pub node_name: &'a str,
    /// Node namespace (e.g., `""` or `"/ns1"`)
    pub namespace: &'a str,
    /// Backend-specific key/value properties.
    ///
    /// Uniform escape hatch for backend-specific tuning that doesn't fit
    /// the universal fields above. Each backend documents the keys it
    /// understands; unknown keys are ignored. Passing `&[]` is always
    /// valid.
    ///
    /// Examples:
    /// - zenoh: `"tls.root_ca"`, `"scouting.multicast.enabled"`
    /// - XRCE-DDS: `"agent_port"`, `"client_key"`
    pub properties: &'a [(&'a str, &'a str)],
}

impl Default for RmwConfig<'_> {
    fn default() -> Self {
        Self {
            locator: "tcp/127.0.0.1:7447",
            mode: SessionMode::Client,
            domain_id: 0,
            node_name: "node",
            namespace: "",
            properties: &[],
        }
    }
}

/// Locator transport protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocatorProtocol {
    /// TCP transport (e.g., "tcp/127.0.0.1:7447")
    Tcp,
    /// UDP transport (e.g., "udp/192.168.1.50:2019" — common for XRCE-DDS)
    Udp,
    /// Serial/UART transport (e.g., "serial//dev/ttyUSB0#baudrate=115200")
    Serial,
    /// Unknown protocol
    Unknown,
}

/// Parse the protocol from a locator string
pub fn locator_protocol(locator: &str) -> LocatorProtocol {
    if locator.starts_with("tcp/") {
        LocatorProtocol::Tcp
    } else if locator.starts_with("udp/") {
        LocatorProtocol::Udp
    } else if locator.starts_with("serial/") {
        LocatorProtocol::Serial
    } else {
        LocatorProtocol::Unknown
    }
}

/// Validate a locator string format.
///
/// Returns `Ok(())` if the locator is well-formed, or an error message describing
/// the problem. This provides early feedback before zenoh-pico or XRCE-DDS rejects
/// a bad locator.
///
/// Supported formats:
/// - TCP: `tcp/<host>:<port>` (e.g., `tcp/127.0.0.1:7447`)
/// - UDP: `udp/<host>:<port>` (e.g., `udp/192.168.1.50:2019`)
/// - Serial: `serial/<device>#baudrate=<rate>` (e.g., `serial//dev/ttyUSB0#baudrate=115200`)
pub fn validate_locator(locator: &str) -> Result<(), &'static str> {
    match locator_protocol(locator) {
        LocatorProtocol::Tcp => {
            let rest = &locator[4..]; // skip "tcp/"
            if !rest.contains(':') {
                return Err("TCP locator must contain host:port (e.g., tcp/127.0.0.1:7447)");
            }
            Ok(())
        }
        LocatorProtocol::Udp => {
            let rest = &locator[4..]; // skip "udp/"
            if !rest.contains(':') {
                return Err("UDP locator must contain host:port (e.g., udp/192.168.1.50:2019)");
            }
            Ok(())
        }
        LocatorProtocol::Serial => {
            let rest = &locator[7..]; // skip "serial/"
            if rest.is_empty() {
                return Err(
                    "serial locator must specify device (e.g., serial//dev/ttyUSB0#baudrate=115200)",
                );
            }
            if !rest.contains("#baudrate=") {
                return Err(
                    "serial locator must include #baudrate=RATE (e.g., serial//dev/ttyUSB0#baudrate=115200)",
                );
            }
            // Validate baudrate is numeric
            if let Some(baud_str) = rest.split("#baudrate=").nth(1) {
                let baud_str = baud_str.split('#').next().unwrap_or(baud_str);
                if baud_str.parse::<u32>().is_err() {
                    return Err("serial baudrate must be a number");
                }
            }
            Ok(())
        }
        LocatorProtocol::Unknown => {
            Err("unknown locator protocol (expected tcp/, udp/, or serial/)")
        }
    }
}

/// Session mode
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SessionMode {
    /// Connect as client to a router
    #[default]
    Client,
    /// Connect as peer for peer-to-peer communication
    Peer,
}

/// Transport session trait — the per-process anchor an RMW backend
/// gives to the executor.
///
/// # Threading
///
/// `&mut self` on every method means the executor serialises all
/// session calls onto a single thread. A backend may rely on this
/// — no internal locking is required for `create_*` / `close` /
/// `drive_io`. **Publisher / subscriber / service handles created
/// from the session, however, are typically used from worker
/// threads** and must carry their own synchronisation (see the
/// [`Publisher`] / [`Subscriber`] trait docs).
///
/// # Calling pattern
///
/// 1. Open the session (backend-specific factory; not on this trait).
/// 2. `create_*` for every entity at startup. Creating entities mid-
///    flight after `drive_io` has run is allowed but not common.
/// 3. The executor calls `drive_io` periodically. Worker threads
///    publish / receive in parallel.
/// 4. `close` once at shutdown. Entities must be dropped first.
pub trait Session {
    /// Error type for this session
    type Error;
    /// Publisher handle type
    type PublisherHandle;
    /// Subscriber handle type
    type SubscriberHandle;
    /// Service server handle type
    type ServiceServerHandle;
    /// Service client handle type
    type ServiceClientHandle;

    /// Create a publisher bound to this session.
    ///
    /// May allocate transport resources (zenoh declarations, DDS
    /// writers). Returns a handle that outlives the call but not the
    /// session — drop the handle before `close()`.
    fn create_publisher(
        &mut self,
        topic: &TopicInfo,
        qos: QosSettings,
    ) -> Result<Self::PublisherHandle, Self::Error>;

    /// Create a subscriber bound to this session.
    ///
    /// Subscribers may start receiving immediately after creation if
    /// the transport supports late-joining publishers. Late messages
    /// are buffered up to the QoS depth.
    fn create_subscriber(
        &mut self,
        topic: &TopicInfo,
        qos: QosSettings,
    ) -> Result<Self::SubscriberHandle, Self::Error>;

    /// Create a service server bound to this session. Replies are
    /// matched to requests by the sequence number returned from
    /// [`ServiceServerTrait::try_recv_request`].
    fn create_service_server(
        &mut self,
        service: &ServiceInfo,
    ) -> Result<Self::ServiceServerHandle, Self::Error>;

    /// Create a service client bound to this session.
    fn create_service_client(
        &mut self,
        service: &ServiceInfo,
    ) -> Result<Self::ServiceClientHandle, Self::Error>;

    /// Close the session, releasing transport resources. All entity
    /// handles created from this session must already be dropped.
    fn close(&mut self) -> Result<(), Self::Error>;

    /// Drive transport I/O (poll network, dispatch callbacks).
    ///
    /// Both zenoh-pico and XRCE-DDS are pull-based: they require the
    /// application to periodically call this method to read from the
    /// network socket and dispatch incoming messages to subscriber
    /// buffers.
    ///
    /// `timeout_ms` is the maximum time to wait for data (0 = non-blocking;
    /// negative values mean "block indefinitely" — see Phase 84.D7 for the
    /// planned migration to `core::time::Duration`).
    ///
    /// **Required**. There is no default body — both shipped backends
    /// (zenoh and XRCE) must drive I/O, and a silent no-op default was a
    /// trap for third-party implementers. If your backend genuinely
    /// receives data via OS callbacks (push-based) and has nothing to do
    /// here, return `Ok(())` explicitly.
    fn drive_io(&mut self, timeout_ms: i32) -> Result<(), Self::Error>;
}

/// Publisher trait for sending messages.
///
/// # Threading
///
/// `&self` on `publish_raw` — implementors must allow concurrent
/// publishes from multiple threads. Internal locking (or lock-free
/// queues) is the backend's responsibility.
///
/// # Buffer ownership
///
/// `data` in `publish_raw` is borrowed for the duration of the call.
/// The backend must either send it inline or copy into its own
/// buffer before returning — the slice is invalid after the call.
///
/// # Blocking
///
/// `publish_raw` is expected to be non-blocking on best-effort QoS
/// and bounded-blocking on reliable QoS (waiting for outbound queue
/// space). Backends should *not* block waiting for ack from a
/// matched subscriber.
pub trait Publisher {
    /// Error type for publish operations
    type Error;

    /// Publish a CDR-serialised message.
    ///
    /// Returns once the message has been handed to the transport
    /// (queued or fired-and-forgotten depending on QoS). Does **not**
    /// wait for delivery.
    fn publish_raw(&self, data: &[u8]) -> Result<(), Self::Error>;

    /// Publish a typed message (serializes automatically)
    fn publish<M: RosMessage>(&self, msg: &M, buf: &mut [u8]) -> Result<(), Self::Error> {
        use nros_core::CdrWriter;

        let mut writer = CdrWriter::new_with_header(buf).map_err(|_| self.buffer_error())?;
        msg.serialize(&mut writer)
            .map_err(|_| self.serialization_error())?;
        let len = writer.position();
        self.publish_raw(&buf[..len])
    }

    /// Return a buffer-too-small error (implementation specific)
    fn buffer_error(&self) -> Self::Error;

    /// Return a serialization error (implementation specific)
    fn serialization_error(&self) -> Self::Error;
}

/// Subscriber trait for receiving messages.
///
/// # Threading
///
/// `&mut self` on `try_recv_raw` — the executor takes exclusive
/// ownership of the subscriber for the duration of a receive. A
/// backend that wants to allow concurrent receives must split into
/// per-thread sub-handles internally.
///
/// # Buffer ownership
///
/// `buf` is caller-owned. The implementation copies the next ready
/// message into `buf` and returns the byte count. The caller may
/// re-use or drop `buf` immediately after the call.
///
/// # Blocking
///
/// `try_recv_raw` is **non-blocking**: returns `Ok(None)` (or
/// equivalent for backends that map empty into a zero-length read)
/// when no message is ready. Use [`Session::drive_io`] to wait for
/// data; never sleep inside `try_recv_raw`.
pub trait Subscriber {
    /// Error type for receive operations
    type Error;

    /// Check if data is available without consuming it.
    ///
    /// Non-destructive — does not advance the receive cursor.
    /// Conservative default returns `true` (always assume data may
    /// be available); backends should override with a real check
    /// to avoid spurious receive attempts.
    fn has_data(&self) -> bool {
        true
    }

    /// Try to receive one message into `buf`.
    ///
    /// Non-blocking. On success returns `Ok(Some(len))` where `len`
    /// is the byte count written into `buf[..len]`. Returns
    /// `Ok(None)` if no message is ready. If `buf` is too small the
    /// backend may either truncate (and document it) or return an
    /// error (preferred).
    fn try_recv_raw(&mut self, buf: &mut [u8]) -> Result<Option<usize>, Self::Error>;

    /// Try to receive a typed message (non-blocking)
    fn try_recv<M: RosMessage>(&mut self, buf: &mut [u8]) -> Result<Option<M>, Self::Error> {
        use nros_core::CdrReader;

        match self.try_recv_raw(buf)? {
            Some(len) => {
                let mut reader = CdrReader::new_with_header(&buf[..len])
                    .map_err(|_| self.deserialization_error())?;
                let msg = M::deserialize(&mut reader).map_err(|_| self.deserialization_error())?;
                Ok(Some(msg))
            }
            None => Ok(None),
        }
    }

    /// Process the received message in-place without copying.
    ///
    /// Calls `f` with a reference to the raw CDR bytes in the subscriber's
    /// internal receive buffer, avoiding a copy into a caller-provided buffer.
    /// While `f` executes the buffer is exclusively borrowed — any messages
    /// arriving from the transport during that time are dropped to prevent
    /// data races.
    ///
    /// Returns `Ok(true)` if a message was available and `f` was called,
    /// `Ok(false)` if no message was available.
    ///
    /// **Default body**: returns `Err(MessageTooLarge)` — the old default
    /// silently truncated anything larger than 1 KB into a stack buffer,
    /// which broke large messages with no diagnostic. Backends must
    /// override this with a real zero-copy path if they advertise support
    /// for `process_raw_in_place`; callers that hit the default should
    /// use `try_recv_raw` with a caller-sized buffer instead.
    fn process_raw_in_place(&mut self, f: impl FnOnce(&[u8])) -> Result<bool, Self::Error>
    where
        Self::Error: From<TransportError>,
    {
        let _ = f;
        Err(TransportError::MessageTooLarge.into())
    }

    /// Try to receive raw data along with publisher metadata.
    ///
    /// When available, [`MessageInfo`](nros_core::MessageInfo) contains
    /// the publisher's GID (Global Identifier) and source timestamp,
    /// extracted from a transport-level attachment on the incoming message.
    ///
    /// Returns `Ok(Some((len, info)))` if data is available, where:
    /// - `len` is the number of bytes written to the buffer
    /// - `info` is the parsed publisher metadata (if attachment was present)
    ///
    /// Default: delegates to [`try_recv_raw`](Subscriber::try_recv_raw) with no info.
    fn try_recv_raw_with_info(
        &mut self,
        buf: &mut [u8],
    ) -> Result<Option<(usize, Option<nros_core::MessageInfo>)>, Self::Error> {
        self.try_recv_raw(buf).map(|opt| opt.map(|len| (len, None)))
    }

    /// Try to receive raw data with E2E safety validation (CRC + sequence tracking).
    ///
    /// Returns `Ok(Some((len, status)))` if data is available, where:
    /// - `len` is the number of bytes written to the buffer
    /// - `status` is the integrity validation result
    ///
    /// Default: delegates to `try_recv_raw` with no CRC info.
    #[cfg(feature = "safety-e2e")]
    fn try_recv_validated(
        &mut self,
        buf: &mut [u8],
    ) -> Result<Option<(usize, crate::IntegrityStatus)>, Self::Error> {
        self.try_recv_raw(buf).map(|opt| {
            opt.map(|len| {
                (
                    len,
                    crate::IntegrityStatus {
                        gap: 0,
                        duplicate: false,
                        crc_valid: None,
                    },
                )
            })
        })
    }

    /// Register an async waker to be notified when data arrives.
    ///
    /// Called from `Future::poll()` implementations to store the waker.
    /// The transport backend calls `waker.wake()` from its receive callback
    /// when new data is available, enabling event-driven async without
    /// busy-polling.
    ///
    /// Default: no-op (backends that don't support waking simply ignore this).
    fn register_waker(&self, _waker: &core::task::Waker) {}

    /// Return a deserialization error (implementation specific)
    fn deserialization_error(&self) -> Self::Error;
}

/// Service request from a client
pub struct ServiceRequest<'a> {
    /// Raw request data (CDR encoded)
    pub data: &'a [u8],
    /// Sequence number for request/response matching
    pub sequence_number: i64,
}

/// Service server trait for handling requests.
///
/// # Threading
///
/// `&mut self` on `try_recv_request` and `send_reply` — the executor
/// owns the server while a request is being handled. Handler bodies
/// run synchronously on the executor thread; long handlers should
/// dispatch work to a worker queue and reply later via the recorded
/// `sequence_number`.
///
/// # Calling pattern
///
/// 1. Executor calls `try_recv_request(buf)`.
/// 2. If `Some(req)` returned, decode, run handler, encode reply.
/// 3. `send_reply(req.sequence_number, &reply_buf)`.
///
/// `sequence_number` is the canonical request → reply correlation
/// token; backends derive it from the wire-level metadata (zenoh
/// query id, DDS sample identity).
pub trait ServiceServerTrait {
    /// Error type for service operations
    type Error;

    /// Check if a request is available without consuming it.
    ///
    /// Non-destructive. Default returns `true` (always assume one
    /// may be available); backends should override with a real
    /// check.
    fn has_request(&self) -> bool {
        true
    }

    /// Try to receive a service request into `buf` (non-blocking).
    ///
    /// On success returns a `ServiceRequest` that borrows from
    /// `buf`. The borrow is released when the returned struct is
    /// dropped — typically before `send_reply` is called, since
    /// `send_reply` takes `&mut self`.
    fn try_recv_request<'a>(
        &mut self,
        buf: &'a mut [u8],
    ) -> Result<Option<ServiceRequest<'a>>, Self::Error>;

    /// Send a reply for the given sequence number. Non-blocking
    /// from the application's perspective; the backend may queue
    /// the reply for transport-level transmission.
    fn send_reply(&mut self, sequence_number: i64, data: &[u8]) -> Result<(), Self::Error>;

    /// Handle a service request with typed messages
    fn handle_request<S: RosService>(
        &mut self,
        req_buf: &mut [u8],
        reply_buf: &mut [u8],
        handler: impl FnOnce(&S::Request) -> S::Reply,
    ) -> Result<bool, Self::Error>
    where
        Self::Error: From<TransportError>,
    {
        use nros_core::{CdrReader, CdrWriter};

        // First, try to receive a request and extract necessary data.
        // Capture the data slice's offset within `req_buf` so we can
        // re-borrow it after the `ServiceRequest` (which holds a
        // borrow into `req_buf`) is dropped. Some backends prepend a
        // header (DDS: 8-byte sequence number) and place the CDR
        // payload at a non-zero offset in the buffer; others (zenoh)
        // put it at offset 0. Reading from offset 0 unconditionally
        // would feed the prefix bytes to the CDR deserializer and
        // silently corrupt the request.
        let buf_start = req_buf.as_ptr() as usize;
        let (data_offset, data_len, sequence_number) = match self.try_recv_request(req_buf)? {
            Some(request) => {
                let offset = (request.data.as_ptr() as usize).saturating_sub(buf_start);
                (offset, request.data.len(), request.sequence_number)
            }
            None => return Ok(false),
        };

        // Deserialize request from the captured offset.
        let mut reader = CdrReader::new_with_header(&req_buf[data_offset..data_offset + data_len])
            .map_err(|_| TransportError::DeserializationError)?;
        let req = S::Request::deserialize(&mut reader)
            .map_err(|_| TransportError::DeserializationError)?;

        // Call handler
        let reply = handler(&req);

        // Serialize reply
        let mut writer =
            CdrWriter::new_with_header(reply_buf).map_err(|_| TransportError::BufferTooSmall)?;
        reply
            .serialize(&mut writer)
            .map_err(|_| TransportError::SerializationError)?;
        let len = writer.position();

        // Send reply (now we can borrow self mutably again)
        self.send_reply(sequence_number, &reply_buf[..len])?;
        Ok(true)
    }

    /// Handle a service request where the handler returns `Box<S::Reply>`
    ///
    /// Identical to `handle_request` but the handler returns a heap-allocated reply.
    /// This is needed for services with large response types (e.g., parameter services
    /// where `Vec<ParameterValue, 64>` is ~1MB+) that would overflow the stack.
    #[cfg(feature = "alloc")]
    fn handle_request_boxed<S: RosService>(
        &mut self,
        req_buf: &mut [u8],
        reply_buf: &mut [u8],
        handler: impl FnOnce(&S::Request) -> alloc::boxed::Box<S::Reply>,
    ) -> Result<bool, Self::Error>
    where
        Self::Error: From<TransportError>,
    {
        use nros_core::{CdrReader, CdrWriter};

        let buf_start = req_buf.as_ptr() as usize;
        let (data_offset, data_len, sequence_number) = match self.try_recv_request(req_buf)? {
            Some(request) => {
                let offset = (request.data.as_ptr() as usize).saturating_sub(buf_start);
                (offset, request.data.len(), request.sequence_number)
            }
            None => return Ok(false),
        };

        let mut reader = CdrReader::new_with_header(&req_buf[data_offset..data_offset + data_len])
            .map_err(|_| TransportError::DeserializationError)?;
        let req = S::Request::deserialize(&mut reader)
            .map_err(|_| TransportError::DeserializationError)?;

        let reply = handler(&req);

        let mut writer =
            CdrWriter::new_with_header(reply_buf).map_err(|_| TransportError::BufferTooSmall)?;
        reply
            .serialize(&mut writer)
            .map_err(|_| TransportError::SerializationError)?;
        let len = writer.position();

        self.send_reply(sequence_number, &reply_buf[..len])?;
        Ok(true)
    }
}

/// Service client trait for sending requests.
///
/// # Threading
///
/// `&mut self` on every method — the client is single-owner. For
/// fan-out request patterns, create one client per worker thread.
///
/// # Calling pattern
///
/// All in-tree backends route blocking waits through the executor:
///
/// 1. `send_request_raw(buf)` — non-blocking; returns once the
///    request is queued for transmission.
/// 2. The executor's `drive_io` runs.
/// 3. `try_recv_reply_raw(buf)` — non-blocking; returns
///    `Ok(Some(len))` when the reply is back.
///
/// The deprecated [`call_raw`](Self::call_raw) blocking path is
/// kept for backwards compatibility but should not be called.
pub trait ServiceClientTrait {
    /// Error type for service operations
    type Error;

    /// Send a service request and wait for reply (blocking).
    ///
    /// **Deprecated — do not call.** The default body returns `Timeout`
    /// immediately without polling. Use `Client::call` →
    /// `Promise::wait(executor, timeout_ms)` which lets the executor
    /// drive I/O while waiting instead of busy-looping on
    /// `try_recv_reply_raw` with no sleep (which starves the transport
    /// on FreeRTOS / Zephyr single-threaded schedulers).
    ///
    /// Backends that still need an internal blocking path should
    /// override this with a real sleep-between-polls implementation,
    /// but all in-tree backends (zenoh, XRCE) route blocking waits
    /// through the executor.
    #[deprecated(note = "use Client::call → Promise::wait with an executor instead")]
    fn call_raw(&mut self, request: &[u8], _reply_buf: &mut [u8]) -> Result<usize, Self::Error>
    where
        Self::Error: From<TransportError>,
    {
        let _ = request;
        Err(TransportError::Timeout.into())
    }

    /// Send a service request without waiting for a reply (non-blocking).
    ///
    /// The caller must subsequently poll [`try_recv_reply_raw`](Self::try_recv_reply_raw)
    /// to retrieve the reply.
    fn send_request_raw(&mut self, request: &[u8]) -> Result<(), Self::Error>;

    /// Poll for a reply to the most recently sent request (non-blocking).
    ///
    /// Returns `Ok(Some(len))` when a reply has arrived, `Ok(None)` if not yet
    /// available, or `Err` on failure.
    fn try_recv_reply_raw(&mut self, reply_buf: &mut [u8]) -> Result<Option<usize>, Self::Error>;

    /// Send a typed service request without waiting for a reply (non-blocking).
    ///
    /// Serializes the request into `req_buf` and calls [`send_request_raw`](Self::send_request_raw).
    fn send_request<S: RosService>(
        &mut self,
        request: &S::Request,
        req_buf: &mut [u8],
    ) -> Result<(), Self::Error>
    where
        Self::Error: From<TransportError>,
    {
        use nros_core::CdrWriter;

        let mut writer =
            CdrWriter::new_with_header(req_buf).map_err(|_| TransportError::BufferTooSmall)?;
        request
            .serialize(&mut writer)
            .map_err(|_| TransportError::SerializationError)?;
        let req_len = writer.position();

        self.send_request_raw(&req_buf[..req_len])
    }

    /// Poll for a typed reply to the most recently sent request (non-blocking).
    ///
    /// Calls [`try_recv_reply_raw`](Self::try_recv_reply_raw) and deserializes if available.
    fn try_recv_reply<S: RosService>(
        &mut self,
        reply_buf: &mut [u8],
    ) -> Result<Option<S::Reply>, Self::Error>
    where
        Self::Error: From<TransportError>,
    {
        use nros_core::CdrReader;

        match self.try_recv_reply_raw(reply_buf)? {
            Some(len) => {
                let mut reader = CdrReader::new_with_header(&reply_buf[..len])
                    .map_err(|_| TransportError::DeserializationError)?;
                let reply = S::Reply::deserialize(&mut reader)
                    .map_err(|_| TransportError::DeserializationError)?;
                Ok(Some(reply))
            }
            None => Ok(None),
        }
    }

    /// Register an async waker to be notified when a reply arrives.
    ///
    /// Called from `Future::poll()` implementations to store the waker.
    /// The transport backend calls `waker.wake()` from its reply callback
    /// when a response is available, enabling event-driven async without
    /// busy-polling.
    ///
    /// Default: no-op (backends that don't support waking simply ignore this).
    fn register_waker(&self, _waker: &core::task::Waker) {}

    /// Begin a server-discovery query on this client (non-blocking).
    ///
    /// Models `rclcpp::ClientBase::wait_for_service` machinery: the backend
    /// fires off a discovery probe (typically a Zenoh liveliness query
    /// against the matching server's wildcarded liveliness keyexpr) and
    /// the caller polls [`poll_server_discovery`](Self::poll_server_discovery)
    /// to collect the result.
    ///
    /// Default impl: no-op success. Backends without a discovery channel
    /// (or those that always assume the server is reachable) can leave
    /// this default and have `poll_server_discovery` return
    /// `Ok(Some(true))` immediately.
    fn start_server_discovery(&mut self, _timeout_ms: u32) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Poll an in-flight server-discovery query.
    ///
    /// - `Ok(Some(true))` — at least one matching server has reported
    ///   back; safe to send the first request.
    /// - `Ok(Some(false))` — discovery query finished without finding
    ///   any matching server (timeout / no-replies).
    /// - `Ok(None)` — query still in flight.
    /// - `Err(_)` — transport-level failure unrelated to server presence.
    ///
    /// Default impl: returns `Ok(Some(true))` (i.e., "server is always
    /// assumed reachable"). The Zenoh backend overrides this with a
    /// liveliness-token check.
    fn poll_server_discovery(&mut self) -> Result<Option<bool>, Self::Error> {
        Ok(Some(true))
    }

    /// Synchronous, non-blocking check of whether a matching server is
    /// currently visible.
    ///
    /// Mirrors `rclcpp::ClientBase::service_is_ready`. Backends that lack
    /// discovery should keep the default `true` so existing call sites
    /// don't regress.
    ///
    /// Default impl: always `true`.
    fn is_server_ready(&self) -> bool {
        true
    }

    /// Call a service with typed messages (blocking).
    ///
    /// **Deprecated — do not call.** The default body returns `Timeout`
    /// immediately without polling. Use `Client::call` on the executor
    /// instead, which drives I/O while waiting. See
    /// [`call_raw`](Self::call_raw) for the same reasoning.
    #[deprecated(note = "use Client::call → Promise::wait with an executor instead")]
    fn call<S: RosService>(
        &mut self,
        request: &S::Request,
        req_buf: &mut [u8],
        _reply_buf: &mut [u8],
    ) -> Result<S::Reply, Self::Error>
    where
        Self::Error: From<TransportError>,
    {
        use nros_core::CdrWriter;

        // Serialize request so the error surface matches the old impl
        // for the "bad request" path; but skip the receive busy-loop.
        let mut writer =
            CdrWriter::new_with_header(req_buf).map_err(|_| TransportError::BufferTooSmall)?;
        request
            .serialize(&mut writer)
            .map_err(|_| TransportError::SerializationError)?;
        Err(TransportError::Timeout.into())
    }
}

/// Transport backend trait (legacy).
///
/// Use [`Rmw`] for new code. This trait is retained for backward compatibility
/// with existing code that uses [`TransportConfig`] directly.
pub trait Transport {
    /// Error type for this transport
    type Error;
    /// Session type for this transport
    type Session: Session;

    /// Open a new session with the given configuration
    fn open(config: &TransportConfig) -> Result<Self::Session, Self::Error>;
}

/// Factory trait for compile-time middleware selection.
///
/// Embedded crates select a backend via feature flag:
/// ```rust,ignore
/// #[cfg(feature = "rmw-zenoh")]
/// type DefaultRmw = nros_rmw_zenoh::ZenohRmw;
/// ```
///
/// Each backend provides its own `Rmw` implementation that bridges
/// from the middleware-agnostic [`RmwConfig`] to backend-specific
/// initialization.
///
/// Phase 84.E2: `open` consumes `self`. Backends carry their own
/// configuration (agent addresses, serial ports, TLS CA slots)
/// inside the factory value and hand that over to the session at
/// `open` time. All in-repo backends also implement
/// [`Default`]; most callers spell this as
/// `BackendRmw::default().open(&config)`.
pub trait Rmw {
    /// Session type returned by [`open`](Rmw::open)
    type Session: Session;
    /// Error type for session creation
    type Error: core::fmt::Debug;

    /// Open a new middleware session with the given configuration.
    ///
    /// The backend maps [`RmwConfig`] fields to its own connection
    /// parameters (e.g., zenoh locator and session mode, XRCE-DDS
    /// agent address). Any backend-specific pre-open state stored
    /// on `self` (e.g. configured agent IP / port) is moved into the
    /// returned `Session`.
    fn open(self, config: &RmwConfig) -> Result<Self::Session, Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topic_info() {
        let topic = TopicInfo::new("/chatter", "std_msgs::msg::dds_::String_", "abc123");
        assert_eq!(topic.name, "/chatter");
        assert_eq!(topic.domain_id, 0);
    }

    #[test]
    fn test_qos_defaults() {
        let qos = QosSettings::default();
        assert_eq!(qos.reliability, QosReliabilityPolicy::Reliable);
    }

    #[test]
    fn test_action_info() {
        let action = ActionInfo::new(
            "/fibonacci",
            "example_interfaces::action::dds_::Fibonacci_",
            "abc123",
        );
        assert_eq!(action.name, "/fibonacci");
        assert_eq!(action.domain_id, 0);
    }

    #[test]
    fn test_action_info_with_domain() {
        let action = ActionInfo::new(
            "/fibonacci",
            "example_interfaces::action::dds_::Fibonacci_",
            "abc123",
        )
        .with_domain(42);
        assert_eq!(action.domain_id, 42);
    }

    #[test]
    fn test_action_send_goal_key() {
        let action = ActionInfo::new(
            "/fibonacci",
            "example_interfaces::action::dds_::Fibonacci_",
            "abc123",
        )
        .with_domain(0);

        let key: heapless::String<256> = action.send_goal_key();
        // ActionInfo returns the sub-entity name with leading slash for ROS 2 compatibility
        assert_eq!(key.as_str(), "/fibonacci/_action/send_goal");
    }

    #[test]
    fn test_action_feedback_key() {
        let action = ActionInfo::new(
            "/fibonacci",
            "example_interfaces::action::dds_::Fibonacci_",
            "abc123",
        )
        .with_domain(0);

        let key: heapless::String<256> = action.feedback_key();
        assert_eq!(key.as_str(), "/fibonacci/_action/feedback");
    }

    #[test]
    fn test_action_all_sub_names() {
        let action = ActionInfo::new(
            "/fibonacci",
            "example_interfaces::action::dds_::Fibonacci_",
            "abc123",
        )
        .with_domain(0);

        let cancel: heapless::String<256> = action.cancel_goal_key();
        assert_eq!(cancel.as_str(), "/fibonacci/_action/cancel_goal");

        let result: heapless::String<256> = action.get_result_key();
        assert_eq!(result.as_str(), "/fibonacci/_action/get_result");

        let status: heapless::String<256> = action.status_key();
        assert_eq!(status.as_str(), "/fibonacci/_action/status");
    }

    // --- QoS Profile Tests ---

    #[test]
    fn test_qos_profile_sensor_data() {
        let qos = QosSettings::QOS_PROFILE_SENSOR_DATA;
        assert_eq!(qos.reliability, QosReliabilityPolicy::BestEffort);
        assert_eq!(qos.durability, QosDurabilityPolicy::Volatile);
        assert_eq!(qos.history, QosHistoryPolicy::KeepLast);
        assert_eq!(qos.depth, 5);
    }

    #[test]
    fn test_qos_profile_default() {
        let qos = QosSettings::QOS_PROFILE_DEFAULT;
        assert_eq!(qos.reliability, QosReliabilityPolicy::Reliable);
        assert_eq!(qos.durability, QosDurabilityPolicy::Volatile);
        assert_eq!(qos.depth, 10);
    }

    #[test]
    fn test_qos_profile_services_default() {
        let qos = QosSettings::QOS_PROFILE_SERVICES_DEFAULT;
        assert_eq!(qos.reliability, QosReliabilityPolicy::Reliable);
        assert_eq!(qos.durability, QosDurabilityPolicy::Volatile);
    }

    #[test]
    fn test_qos_profile_parameters() {
        let qos = QosSettings::QOS_PROFILE_PARAMETERS;
        assert_eq!(qos.reliability, QosReliabilityPolicy::Reliable);
        assert_eq!(qos.durability, QosDurabilityPolicy::TransientLocal);
        assert_eq!(qos.depth, 1000);
    }

    #[test]
    fn test_qos_profile_clock() {
        let qos = QosSettings::QOS_PROFILE_CLOCK;
        assert_eq!(qos.reliability, QosReliabilityPolicy::BestEffort);
        assert_eq!(qos.depth, 1);
    }

    #[test]
    fn test_qos_profile_parameter_events() {
        let qos = QosSettings::QOS_PROFILE_PARAMETER_EVENTS;
        assert_eq!(qos.reliability, QosReliabilityPolicy::Reliable);
        assert_eq!(qos.history, QosHistoryPolicy::KeepAll);
    }

    #[test]
    fn test_qos_profile_action_status() {
        let qos = QosSettings::QOS_PROFILE_ACTION_STATUS_DEFAULT;
        assert_eq!(qos.reliability, QosReliabilityPolicy::Reliable);
        assert_eq!(qos.durability, QosDurabilityPolicy::TransientLocal);
        assert_eq!(qos.depth, 1);
    }

    #[test]
    fn test_qos_static_constructors() {
        assert_eq!(
            QosSettings::topics_default(),
            QosSettings::QOS_PROFILE_DEFAULT
        );
        assert_eq!(
            QosSettings::sensor_data_default(),
            QosSettings::QOS_PROFILE_SENSOR_DATA
        );
        assert_eq!(
            QosSettings::services_default(),
            QosSettings::QOS_PROFILE_SERVICES_DEFAULT
        );
        assert_eq!(
            QosSettings::parameters_default(),
            QosSettings::QOS_PROFILE_PARAMETERS
        );
        assert_eq!(
            QosSettings::action_status_default(),
            QosSettings::QOS_PROFILE_ACTION_STATUS_DEFAULT
        );
    }

    #[test]
    fn test_qos_builder_explicit_setters() {
        let qos = QosSettings::new()
            .reliability(QosReliabilityPolicy::Reliable)
            .durability(QosDurabilityPolicy::TransientLocal)
            .history(QosHistoryPolicy::KeepAll)
            .depth(100);

        assert_eq!(qos.reliability, QosReliabilityPolicy::Reliable);
        assert_eq!(qos.durability, QosDurabilityPolicy::TransientLocal);
        assert_eq!(qos.history, QosHistoryPolicy::KeepAll);
        assert_eq!(qos.depth, 100);
    }

    #[test]
    fn test_qos_builder_chaining() {
        // Test that builder methods can be chained in any order
        let qos = QosSettings::sensor_data_default()
            .reliable()
            .transient_local()
            .keep_last(20);

        assert_eq!(qos.reliability, QosReliabilityPolicy::Reliable);
        assert_eq!(qos.durability, QosDurabilityPolicy::TransientLocal);
        assert_eq!(qos.history, QosHistoryPolicy::KeepLast);
        assert_eq!(qos.depth, 20);
    }

    #[test]
    fn test_qos_eq_impl() {
        // Verify that PartialEq works correctly via derive on QosSettings
        let qos1 = QosSettings::QOS_PROFILE_DEFAULT;
        let qos2 = QosSettings::topics_default();
        // Both should have same values - verify field by field
        assert_eq!(qos1.reliability, qos2.reliability);
        assert_eq!(qos1.durability, qos2.durability);
        assert_eq!(qos1.history, qos2.history);
        assert_eq!(qos1.depth, qos2.depth);
    }

    // --- Locator validation tests ---

    #[test]
    fn test_locator_protocol_tcp() {
        assert_eq!(locator_protocol("tcp/127.0.0.1:7447"), LocatorProtocol::Tcp);
    }

    #[test]
    fn test_locator_protocol_serial() {
        assert_eq!(
            locator_protocol("serial//dev/ttyUSB0#baudrate=115200"),
            LocatorProtocol::Serial
        );
    }

    #[test]
    fn test_locator_protocol_unknown() {
        assert_eq!(locator_protocol(""), LocatorProtocol::Unknown);
        assert_eq!(locator_protocol("http://foo"), LocatorProtocol::Unknown);
        assert_eq!(locator_protocol("tls/host:port"), LocatorProtocol::Unknown);
    }

    #[test]
    fn test_locator_protocol_udp() {
        assert_eq!(locator_protocol("udp/127.0.0.1:7447"), LocatorProtocol::Udp);
        assert_eq!(
            locator_protocol("udp/192.168.1.50:2019"),
            LocatorProtocol::Udp
        );
    }

    #[test]
    fn test_validate_tcp_locator_ok() {
        assert!(validate_locator("tcp/127.0.0.1:7447").is_ok());
        assert!(validate_locator("tcp/192.168.1.1:7447").is_ok());
    }

    #[test]
    fn test_validate_tcp_locator_missing_port() {
        assert!(validate_locator("tcp/127.0.0.1").is_err());
    }

    #[test]
    fn test_validate_serial_locator_ok() {
        assert!(validate_locator("serial//dev/ttyUSB0#baudrate=115200").is_ok());
        assert!(validate_locator("serial//dev/ttyACM0#baudrate=9600").is_ok());
        assert!(validate_locator("serial/uart1#baudrate=921600").is_ok());
    }

    #[test]
    fn test_validate_serial_locator_empty_device() {
        assert!(validate_locator("serial/").is_err());
    }

    #[test]
    fn test_validate_serial_locator_missing_baudrate() {
        assert!(validate_locator("serial//dev/ttyUSB0").is_err());
    }

    #[test]
    fn test_validate_serial_locator_invalid_baudrate() {
        assert!(validate_locator("serial//dev/ttyUSB0#baudrate=abc").is_err());
    }

    #[test]
    fn test_validate_unknown_protocol() {
        assert!(validate_locator("http://foo").is_err());
        assert!(validate_locator("tls/host:port").is_err());
    }

    #[test]
    fn test_validate_udp_locator_ok() {
        assert!(validate_locator("udp/127.0.0.1:7447").is_ok());
        assert!(validate_locator("udp/192.168.1.50:2019").is_ok());
    }

    #[test]
    fn test_validate_udp_locator_missing_port() {
        assert!(validate_locator("udp/127.0.0.1").is_err());
    }

    // --- RmwConfig Tests ---

    #[test]
    fn test_rmw_config_default() {
        let config = RmwConfig::default();
        assert_eq!(config.locator, "tcp/127.0.0.1:7447");
        assert_eq!(config.mode, SessionMode::Client);
        assert_eq!(config.domain_id, 0);
        assert_eq!(config.node_name, "node");
        assert_eq!(config.namespace, "");
    }

    #[test]
    fn test_rmw_config_custom() {
        let config = RmwConfig {
            locator: "tcp/192.168.1.1:7447",
            mode: SessionMode::Peer,
            domain_id: 42,
            node_name: "talker",
            namespace: "/ns1",
            properties: &[("agent_port", "2019")],
        };
        assert_eq!(config.locator, "tcp/192.168.1.1:7447");
        assert_eq!(config.mode, SessionMode::Peer);
        assert_eq!(config.domain_id, 42);
        assert_eq!(config.node_name, "talker");
        assert_eq!(config.namespace, "/ns1");
        assert_eq!(config.properties.len(), 1);
        assert_eq!(config.properties[0].0, "agent_port");
    }

    #[test]
    fn test_rmw_config_is_copy() {
        let config = RmwConfig::default();
        let config2 = config; // Copy
        assert_eq!(config.locator, config2.locator);
        assert_eq!(config.domain_id, config2.domain_id);
    }

    #[test]
    fn test_rmw_config_clone() {
        let config = RmwConfig::default();
        let cloned = RmwConfig { ..config };
        assert_eq!(cloned.locator, config.locator);
        assert_eq!(cloned.node_name, config.node_name);
    }
}

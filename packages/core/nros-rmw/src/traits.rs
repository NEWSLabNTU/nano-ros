//! Transport abstraction traits
//!
//! These traits define the interface for transport backends (zenoh-pico, etc.)

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
}

impl<'a> TopicInfo<'a> {
    /// Create new topic info
    pub const fn new(name: &'a str, type_name: &'a str, type_hash: &'a str) -> Self {
        Self {
            name,
            type_name,
            type_hash,
            domain_id: 0,
        }
    }

    /// Create topic info with custom domain ID
    pub const fn with_domain(mut self, domain_id: u32) -> Self {
        self.domain_id = domain_id;
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
        }
    }

    /// Create service info with custom domain ID
    pub const fn with_domain(mut self, domain_id: u32) -> Self {
        self.domain_id = domain_id;
        self
    }
}

/// Transport error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    /// Incoming message exceeded the static subscriber buffer size (1024 bytes)
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
    /// Reliable delivery (retransmit if needed)
    Reliable,
    /// Best-effort delivery (no retransmits)
    #[default]
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
    /// Create new QoS settings with defaults
    pub const fn new() -> Self {
        Self {
            history: QosHistoryPolicy::KeepLast,
            reliability: QosReliabilityPolicy::BestEffort,
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
/// Unlike [`TransportConfig`] which carries backend-specific properties,
/// `RmwConfig` provides a uniform interface that any RMW backend can
/// interpret. Backends map these fields to their own connection parameters.
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
}

impl Default for RmwConfig<'_> {
    fn default() -> Self {
        Self {
            locator: "tcp/127.0.0.1:7447",
            mode: SessionMode::Client,
            domain_id: 0,
            node_name: "node",
            namespace: "",
        }
    }
}

/// Locator transport protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocatorProtocol {
    /// TCP transport (e.g., "tcp/127.0.0.1:7447")
    Tcp,
    /// Serial/UART transport (e.g., "serial//dev/ttyUSB0#baudrate=115200")
    Serial,
    /// Unknown protocol
    Unknown,
}

/// Parse the protocol from a locator string
pub fn locator_protocol(locator: &str) -> LocatorProtocol {
    if locator.starts_with("tcp/") {
        LocatorProtocol::Tcp
    } else if locator.starts_with("serial/") {
        LocatorProtocol::Serial
    } else {
        LocatorProtocol::Unknown
    }
}

/// Validate a locator string format.
///
/// Returns `Ok(())` if the locator is well-formed, or an error message describing
/// the problem. This provides early feedback before zenoh-pico rejects a bad locator.
///
/// Supported formats:
/// - TCP: `tcp/<host>:<port>` (e.g., `tcp/127.0.0.1:7447`)
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
        LocatorProtocol::Unknown => Err("unknown locator protocol (expected tcp/ or serial/)"),
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

/// Transport session trait
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

    /// Create a publisher for a topic
    fn create_publisher(
        &mut self,
        topic: &TopicInfo,
        qos: QosSettings,
    ) -> Result<Self::PublisherHandle, Self::Error>;

    /// Create a subscriber for a topic
    fn create_subscriber(
        &mut self,
        topic: &TopicInfo,
        qos: QosSettings,
    ) -> Result<Self::SubscriberHandle, Self::Error>;

    /// Create a service server
    fn create_service_server(
        &mut self,
        service: &ServiceInfo,
    ) -> Result<Self::ServiceServerHandle, Self::Error>;

    /// Create a service client
    fn create_service_client(
        &mut self,
        service: &ServiceInfo,
    ) -> Result<Self::ServiceClientHandle, Self::Error>;

    /// Close the session
    fn close(&mut self) -> Result<(), Self::Error>;
}

/// Publisher trait for sending messages
pub trait Publisher {
    /// Error type for publish operations
    type Error;

    /// Publish a serialized message
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

/// Subscriber trait for receiving messages
pub trait Subscriber {
    /// Error type for receive operations
    type Error;

    /// Check if data is available without consuming it
    ///
    /// Returns true if the subscriber has data ready to be received.
    /// This is a non-destructive check that does not consume the message.
    /// Conservative default returns true (always assume data may be available).
    fn has_data(&self) -> bool {
        true
    }

    /// Try to receive a raw message (non-blocking)
    /// Returns None if no message is available
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

/// Service server trait for handling requests
pub trait ServiceServerTrait {
    /// Error type for service operations
    type Error;

    /// Check if a request is available without consuming it
    ///
    /// Returns true if the service server has a pending request.
    /// This is a non-destructive check that does not consume the request.
    /// Conservative default returns true (always assume a request may be available).
    fn has_request(&self) -> bool {
        true
    }

    /// Try to receive a service request (non-blocking)
    /// The returned ServiceRequest references data in the provided buffer
    fn try_recv_request<'a>(
        &mut self,
        buf: &'a mut [u8],
    ) -> Result<Option<ServiceRequest<'a>>, Self::Error>;

    /// Send a reply to a service request
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

        // First, try to receive a request and extract necessary data
        let (data_len, sequence_number) = match self.try_recv_request(req_buf)? {
            Some(request) => (request.data.len(), request.sequence_number),
            None => return Ok(false),
        };

        // Now we can work with req_buf directly since ServiceRequest has been dropped
        // Deserialize request
        let mut reader = CdrReader::new_with_header(&req_buf[..data_len])
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

        let (data_len, sequence_number) = match self.try_recv_request(req_buf)? {
            Some(request) => (request.data.len(), request.sequence_number),
            None => return Ok(false),
        };

        let mut reader = CdrReader::new_with_header(&req_buf[..data_len])
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

/// Service client trait for sending requests
pub trait ServiceClientTrait {
    /// Error type for service operations
    type Error;

    /// Send a service request and wait for reply
    fn call_raw(&mut self, request: &[u8], reply_buf: &mut [u8]) -> Result<usize, Self::Error>;

    /// Call a service with typed messages
    fn call<S: RosService>(
        &mut self,
        request: &S::Request,
        req_buf: &mut [u8],
        reply_buf: &mut [u8],
    ) -> Result<S::Reply, Self::Error>
    where
        Self::Error: From<TransportError>,
    {
        use nros_core::{CdrReader, CdrWriter};

        // Serialize request
        let mut writer =
            CdrWriter::new_with_header(req_buf).map_err(|_| TransportError::BufferTooSmall)?;
        request
            .serialize(&mut writer)
            .map_err(|_| TransportError::SerializationError)?;
        let req_len = writer.position();

        // Send request and wait for reply
        let reply_len = self.call_raw(&req_buf[..req_len], reply_buf)?;

        // Deserialize reply
        let mut reader = CdrReader::new_with_header(&reply_buf[..reply_len])
            .map_err(|_| TransportError::DeserializationError)?;
        let reply =
            S::Reply::deserialize(&mut reader).map_err(|_| TransportError::DeserializationError)?;

        Ok(reply)
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
/// #[cfg(feature = "zenoh")]
/// type DefaultRmw = nros_rmw_zenoh::ZenohRmw;
/// ```
///
/// Each backend provides its own `Rmw` implementation that bridges
/// from the middleware-agnostic [`RmwConfig`] to backend-specific
/// initialization.
pub trait Rmw {
    /// Session type returned by [`open`](Rmw::open)
    type Session: Session;
    /// Error type for session creation
    type Error: core::fmt::Debug;

    /// Open a new middleware session with the given configuration.
    ///
    /// The backend maps [`RmwConfig`] fields to its own connection
    /// parameters (e.g., zenoh locator and session mode, XRCE-DDS
    /// agent address).
    fn open(config: &RmwConfig) -> Result<Self::Session, Self::Error>;
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
        assert_eq!(
            locator_protocol("udp/127.0.0.1:7447"),
            LocatorProtocol::Unknown
        );
        assert_eq!(locator_protocol(""), LocatorProtocol::Unknown);
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
        assert!(validate_locator("udp/127.0.0.1:7447").is_err());
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
        };
        assert_eq!(config.locator, "tcp/192.168.1.1:7447");
        assert_eq!(config.mode, SessionMode::Peer);
        assert_eq!(config.domain_id, 42);
        assert_eq!(config.node_name, "talker");
        assert_eq!(config.namespace, "/ns1");
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

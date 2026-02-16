//! Context and initialization for rclrs-style API
//!
//! This module provides the Context type and related initialization types
//! that match the rclrs 0.6.0 API pattern.
//!
//! # Unified Executor API
//!
//! The recommended way to use nros is through the executor API:
//!
//! ```ignore
//! use nros::prelude::*;
//!
//! // Create context
//! let ctx = Context::new(InitOptions::new().locator("tcp/127.0.0.1:7447"))?;
//!
//! // Create executor (choose one)
//! let mut executor = ctx.create_basic_executor();      // std: has spin()
//! // let mut executor = ctx.create_polling_executor(); // no_std: manual spin_once()
//!
//! // Create node through executor
//! let node = executor.create_node("my_node")?;
//!
//! // Create subscriptions with callbacks
//! node.create_subscription::<Int32>("/topic", |msg| {
//!     println!("Received: {}", msg.data);
//! })?;
//!
//! // Run the executor
//! executor.spin(SpinOptions::default());
//! ```

use crate::NodeConfig;

#[cfg(feature = "rmw-zenoh")]
use crate::ConnectedNode;

#[cfg(feature = "rmw-zenoh")]
use nros_rmw::{SessionMode, TransportConfig};

#[cfg(all(feature = "rmw-zenoh", feature = "alloc"))]
use crate::executor::{DEFAULT_MAX_NODES, PollingExecutor};

#[cfg(all(feature = "rmw-zenoh", feature = "std"))]
use crate::executor::BasicExecutor;

/// Context for creating executors and nodes
///
/// The Context holds shared initialization state and is the entry point
/// for creating executors. This matches the rclrs API pattern.
///
/// # Recommended: Executor API
///
/// ```ignore
/// use nros::prelude::*;
///
/// let ctx = Context::new(InitOptions::new().locator("tcp/127.0.0.1:7447"))?;
/// let mut executor = ctx.create_basic_executor();
/// let node = executor.create_node("my_node")?;
/// ```
///
/// # Legacy: Direct Node Creation
///
/// ```ignore
/// // Deprecated - use executor API instead
/// let ctx = Context::new(InitOptions::new())?;
/// let node = ctx.create_node("my_node")?;  // Deprecated
/// ```
#[derive(Debug, Clone)]
pub struct Context {
    /// ROS 2 domain ID (defaults to 0)
    domain_id: u32,
    /// Transport configuration for zenoh connections
    #[cfg(feature = "rmw-zenoh")]
    transport_config: TransportConfig<'static>,
}

impl Context {
    /// Create a new context with the given options
    ///
    /// # Arguments
    /// * `options` - Initialization options
    ///
    /// # Returns
    /// A new Context instance
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let context = Context::new(InitOptions::new()
    ///     .with_domain_id(Some(42))
    ///     .locator("tcp/127.0.0.1:7447"))?;
    /// ```
    #[cfg(feature = "rmw-zenoh")]
    pub fn new(options: InitOptions) -> Result<Self, RclrsError> {
        let domain_id = options.domain_id.unwrap_or(0);
        let properties: &'static [(&'static str, &'static str)] = if options.properties.is_empty() {
            &[]
        } else {
            let v: alloc::vec::Vec<_> = options.properties.iter().copied().collect();
            alloc::boxed::Box::leak(v.into_boxed_slice())
        };
        let transport_config = TransportConfig {
            locator: options.locator,
            mode: options.session_mode,
            properties,
        };
        Ok(Self {
            domain_id,
            transport_config,
        })
    }

    /// Create a new context with the given options (non-zenoh version)
    #[cfg(not(feature = "rmw-zenoh"))]
    pub fn new(options: InitOptions) -> Result<Self, RclrsError> {
        let domain_id = options.domain_id.unwrap_or(0);
        Ok(Self { domain_id })
    }

    /// Create a context from environment variables
    ///
    /// Reads the following environment variables:
    /// - `ROS_DOMAIN_ID`: Domain ID (default: 0)
    /// - `ZENOH_MODE`: Session mode - "peer" for peer mode, otherwise client mode
    /// - `ZENOH_LOCATOR`: Locator for client mode (default: "tcp/127.0.0.1:7447")
    /// - `ROS_LOCALHOST_ONLY`: When "1", forces loopback locator and disables multicast
    ///   scouting (Humble+, requires `ros-humble` feature)
    /// - `ROS_AUTOMATIC_DISCOVERY_RANGE`: Controls discovery scope — "LOCALHOST" acts
    ///   like `ROS_LOCALHOST_ONLY=1`, "OFF" disables scouting only (Iron+, requires
    ///   `ros-iron` feature, supersedes `ROS_LOCALHOST_ONLY`)
    /// - `ROS_STATIC_PEERS`: Semicolon-separated locators; first entry overrides
    ///   `ZENOH_LOCATOR` (Iron+, requires `ros-iron` feature)
    ///
    /// In peer mode, no locator is needed as peers discover each other via multicast.
    /// `ROS_LOCALHOST_ONLY` is ignored in peer mode.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Client mode (default)
    /// let context = Context::from_env()?;
    ///
    /// // Peer mode (set ZENOH_MODE=peer)
    /// std::env::set_var("ZENOH_MODE", "peer");
    /// let context = Context::from_env()?;
    ///
    /// // Localhost-only mode
    /// std::env::set_var("ROS_LOCALHOST_ONLY", "1");
    /// let context = Context::from_env()?;
    /// ```
    #[cfg(all(feature = "std", feature = "rmw-zenoh"))]
    pub fn from_env() -> Result<Self, RclrsError> {
        let domain_id = std::env::var("ROS_DOMAIN_ID")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        // Check for peer mode
        let is_peer_mode = std::env::var("ZENOH_MODE")
            .map(|v| v.eq_ignore_ascii_case("peer"))
            .unwrap_or(false);

        // --- ROS discovery env vars ---

        // Compute disable_scouting and force_localhost flags
        #[cfg_attr(
            not(any(feature = "ros-humble", feature = "ros-iron")),
            allow(unused_mut)
        )]
        let mut disable_scouting = false;
        #[cfg_attr(
            not(any(feature = "ros-humble", feature = "ros-iron")),
            allow(unused_mut)
        )]
        let mut force_localhost = false;

        // ROS_LOCALHOST_ONLY (Humble+)
        #[cfg(any(feature = "ros-humble", feature = "ros-iron"))]
        {
            let localhost_only = std::env::var("ROS_LOCALHOST_ONLY")
                .map(|v| v == "1")
                .unwrap_or(false);
            if localhost_only && !is_peer_mode {
                disable_scouting = true;
                force_localhost = true;
            }
        }

        // ROS_AUTOMATIC_DISCOVERY_RANGE (Iron+) — supersedes ROS_LOCALHOST_ONLY
        #[cfg(feature = "ros-iron")]
        {
            if let Ok(range) = std::env::var("ROS_AUTOMATIC_DISCOVERY_RANGE") {
                match range.as_str() {
                    "LOCALHOST" => {
                        if !is_peer_mode {
                            disable_scouting = true;
                            force_localhost = true;
                        }
                    }
                    "OFF" => {
                        disable_scouting = true;
                        // force_localhost is NOT set — keep explicit locator
                    }
                    _ => {
                        // "SUBNET", "SYSTEM_DEFAULT", or unrecognized — no effect
                    }
                }
            }
        }

        // ROS_STATIC_PEERS (Iron+) — first entry overrides ZENOH_LOCATOR
        #[cfg(feature = "ros-iron")]
        let static_peer: Option<&'static str> = std::env::var("ROS_STATIC_PEERS")
            .ok()
            .filter(|s| !s.is_empty())
            .and_then(|s| {
                // Take first semicolon-separated entry
                let first: std::string::String = s.split(';').next().unwrap_or("").trim().into();
                if first.is_empty() {
                    None
                } else {
                    Some(&*std::boxed::Box::leak(first.into_boxed_str()))
                }
            });

        // --- Build TransportConfig ---

        let transport_config = if is_peer_mode {
            TransportConfig {
                locator: None,
                mode: SessionMode::Peer,
                properties: &[],
            }
        } else {
            // Compute effective locator (priority: force_localhost > static_peers > ZENOH_LOCATOR > default)
            let locator: Option<&'static str> = if force_localhost {
                Some("tcp/127.0.0.1:7447")
            } else {
                // Check ROS_STATIC_PEERS first (Iron+)
                #[cfg(feature = "ros-iron")]
                let from_static_peers = static_peer;
                #[cfg(not(feature = "ros-iron"))]
                let from_static_peers: Option<&'static str> = None;

                if let Some(peer) = from_static_peers {
                    Some(peer)
                } else {
                    // Fall back to ZENOH_LOCATOR or default
                    let from_env: Option<&'static str> = std::env::var("ZENOH_LOCATOR")
                        .ok()
                        .map(|s| -> &'static str { std::boxed::Box::leak(s.into_boxed_str()) });
                    from_env.or(Some("tcp/127.0.0.1:7447"))
                }
            };

            // Build properties: static slice when scouting is disabled
            let properties: &'static [(&'static str, &'static str)] = if disable_scouting {
                &[("multicast_scouting", "false")]
            } else {
                &[]
            };

            TransportConfig {
                locator,
                mode: SessionMode::Client,
                properties,
            }
        };

        Ok(Self {
            domain_id,
            transport_config,
        })
    }

    /// Create a context from environment variables (non-zenoh version)
    #[cfg(all(feature = "std", not(feature = "rmw-zenoh")))]
    pub fn from_env() -> Result<Self, RclrsError> {
        let domain_id = std::env::var("ROS_DOMAIN_ID")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        Ok(Self { domain_id })
    }

    /// Create a context with default settings
    ///
    /// This is equivalent to `Context::new(InitOptions::new())` and
    /// uses domain ID 0 with default locator.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let context = Context::default_from_env()?;
    /// ```
    #[cfg(feature = "rmw-zenoh")]
    pub fn default_from_env() -> Result<Self, RclrsError> {
        let transport_config = TransportConfig {
            locator: Some("tcp/127.0.0.1:7447"),
            mode: SessionMode::Client,
            properties: &[],
        };
        Ok(Self {
            domain_id: 0,
            transport_config,
        })
    }

    /// Create a context with default settings (non-zenoh version)
    #[cfg(not(feature = "rmw-zenoh"))]
    pub fn default_from_env() -> Result<Self, RclrsError> {
        Ok(Self { domain_id: 0 })
    }

    /// Get the domain ID for this context
    pub fn domain_id(&self) -> u32 {
        self.domain_id
    }

    /// Check if the context is still valid
    ///
    /// Currently always returns true.
    pub fn ok(&self) -> bool {
        true
    }

    // ═══════════════════════════════════════════════════════════════════════
    // EXECUTOR CREATION (New API)
    // ═══════════════════════════════════════════════════════════════════════

    /// Create a polling executor (no_std compatible)
    ///
    /// The polling executor requires manual calls to `spin_once()` and is
    /// suitable for RTIC, Embassy, or bare-metal applications.
    ///
    /// # Type Parameters
    ///
    /// - `MAX_NODES`: Maximum number of nodes this executor can manage (default: 4)
    ///
    /// # Example
    ///
    /// ```ignore
    /// let ctx = Context::new(InitOptions::new().locator("tcp/192.168.1.1:7447"))?;
    /// let mut executor: PollingExecutor<2> = ctx.create_polling_executor();
    /// let node = executor.create_node("my_node")?;
    ///
    /// // In main loop or RTIC task:
    /// loop {
    ///     executor.spin_once(10);  // 10ms delta
    ///     // delay...
    /// }
    /// ```
    #[cfg(all(feature = "rmw-zenoh", feature = "alloc"))]
    pub fn create_polling_executor<const MAX_NODES: usize>(&self) -> PollingExecutor<MAX_NODES> {
        PollingExecutor::new(self.domain_id, self.transport_config.clone())
    }

    /// Create a polling executor with default capacity
    #[cfg(all(feature = "rmw-zenoh", feature = "alloc"))]
    pub fn create_polling_executor_default(&self) -> PollingExecutor<DEFAULT_MAX_NODES> {
        self.create_polling_executor()
    }

    /// Create a basic executor with full spin support (std only)
    ///
    /// The basic executor provides `spin()` for blocking spin loops and
    /// `halt()` for stopping from another thread.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let ctx = Context::new(InitOptions::new())?;
    /// let mut executor = ctx.create_basic_executor();
    /// let node = executor.create_node("my_node")?;
    ///
    /// node.create_subscription::<Int32>("/topic", |msg| {
    ///     println!("Received: {}", msg.data);
    /// })?;
    ///
    /// // Blocking spin
    /// executor.spin(SpinOptions::default());
    /// ```
    #[cfg(all(feature = "rmw-zenoh", feature = "std"))]
    pub fn create_basic_executor(&self) -> BasicExecutor {
        BasicExecutor::new(self.domain_id, self.transport_config.clone())
    }

    // ═══════════════════════════════════════════════════════════════════════
    // LEGACY NODE CREATION (Deprecated)
    // ═══════════════════════════════════════════════════════════════════════

    /// Create a node using this context (zenoh feature only)
    ///
    /// **Deprecated**: Use `create_polling_executor()` or `create_basic_executor()`
    /// instead for the new executor-based API.
    ///
    /// # Arguments
    /// * `options` - Node name or NodeOptions with optional namespace
    ///
    /// # Returns
    /// A new Node (ConnectedNode)
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Simple node creation (deprecated)
    /// let node = context.create_node("my_node")?;
    ///
    /// // Recommended: use executor API instead
    /// let mut executor = context.create_basic_executor();
    /// let node = executor.create_node("my_node")?;
    /// ```
    #[cfg(feature = "rmw-zenoh")]
    #[deprecated(
        since = "0.2.0",
        note = "Use create_polling_executor() or create_basic_executor() instead"
    )]
    #[allow(deprecated)] // Internal use of ConnectedNode::new() is intentional
    pub fn create_node<'a>(&self, options: impl IntoNodeOptions<'a>) -> Result<Node, RclrsError> {
        let node_options = options.into_node_options();

        let config = NodeConfig {
            name: node_options.name,
            namespace: node_options.namespace.unwrap_or("/"),
            domain_id: self.domain_id,
        };

        let node = ConnectedNode::new(config, &self.transport_config)
            .map_err(|_| RclrsError::NodeCreationFailed)?;

        Ok(node)
    }
}

/// Initialization options for creating a Context
///
/// # Examples
///
/// ```
/// use nros_node::InitOptions;
///
/// let options = InitOptions::new()
///     .with_domain_id(Some(42));
/// ```
///
/// With zenoh transport:
///
/// ```ignore
/// use nros_node::InitOptions;
/// use nros_rmw::SessionMode;
///
/// let options = InitOptions::new()
///     .with_domain_id(Some(0))
///     .locator("tcp/192.168.1.1:7447")
///     .session_mode(SessionMode::Client);
/// ```
#[derive(Debug, Clone)]
pub struct InitOptions {
    /// ROS 2 domain ID (None means use default of 0)
    pub(crate) domain_id: Option<u32>,
    /// Zenoh locator (e.g., "tcp/127.0.0.1:7447")
    #[cfg(feature = "rmw-zenoh")]
    pub(crate) locator: Option<&'static str>,
    /// Session mode (Client or Peer)
    #[cfg(feature = "rmw-zenoh")]
    pub(crate) session_mode: SessionMode,
    /// Additional transport properties (key-value pairs)
    #[cfg(feature = "rmw-zenoh")]
    pub(crate) properties: heapless::Vec<(&'static str, &'static str), 8>,
}

impl Default for InitOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl InitOptions {
    /// Create new initialization options with defaults
    pub fn new() -> Self {
        Self {
            domain_id: None,
            #[cfg(feature = "rmw-zenoh")]
            locator: Some("tcp/127.0.0.1:7447"),
            #[cfg(feature = "rmw-zenoh")]
            session_mode: SessionMode::Client,
            #[cfg(feature = "rmw-zenoh")]
            properties: heapless::Vec::new(),
        }
    }

    /// Set the ROS 2 domain ID
    ///
    /// # Arguments
    /// * `domain_id` - Optional domain ID (None means use default of 0)
    pub fn with_domain_id(mut self, domain_id: Option<u32>) -> Self {
        self.domain_id = domain_id;
        self
    }

    /// Set the domain ID directly
    pub fn domain_id(mut self, id: u32) -> Self {
        self.domain_id = Some(id);
        self
    }

    /// Set the zenoh locator
    ///
    /// # Arguments
    /// * `locator` - Locator string (e.g., "tcp/127.0.0.1:7447")
    #[cfg(feature = "rmw-zenoh")]
    pub fn locator(mut self, locator: &'static str) -> Self {
        self.locator = Some(locator);
        self
    }

    /// Set the session mode
    ///
    /// # Arguments
    /// * `mode` - Session mode (Client or Peer)
    #[cfg(feature = "rmw-zenoh")]
    pub fn session_mode(mut self, mode: SessionMode) -> Self {
        self.session_mode = mode;
        self
    }

    /// Configure for peer mode (no router required)
    #[cfg(feature = "rmw-zenoh")]
    pub fn peer_mode(mut self) -> Self {
        self.session_mode = SessionMode::Peer;
        self.locator = None;
        self
    }

    /// Add a transport property (key-value pair)
    ///
    /// Properties are passed through to the underlying transport backend.
    /// For zenoh-pico, recognized keys include:
    /// - `"multicast_scouting"` - `"true"` or `"false"`
    /// - `"scouting_timeout_ms"` - Timeout in milliseconds
    /// - `"multicast_locator"` - Multicast group address
    /// - `"listen"` - Listen endpoint
    /// - `"add_timestamp"` - `"true"` or `"false"`
    ///
    /// Up to 8 properties can be set. Additional properties are silently ignored.
    #[cfg(feature = "rmw-zenoh")]
    pub fn property(mut self, key: &'static str, value: &'static str) -> Self {
        let _ = self.properties.push((key, value));
        self
    }
}

/// Options for creating a node
///
/// # Examples
///
/// ```
/// use nros_node::NodeOptions;
///
/// let options = NodeOptions::new("my_node")
///     .namespace("/my_namespace");
/// ```
#[derive(Debug, Clone)]
pub struct NodeOptions<'a> {
    /// Node name
    pub name: &'a str,
    /// Node namespace (optional, defaults to "/")
    pub namespace: Option<&'a str>,
}

impl<'a> NodeOptions<'a> {
    /// Create new node options with the given name
    pub fn new(name: &'a str) -> Self {
        Self {
            name,
            namespace: None,
        }
    }

    /// Set the namespace for this node
    ///
    /// # Arguments
    /// * `ns` - Namespace string (should start with "/")
    pub fn namespace(mut self, ns: &'a str) -> Self {
        self.namespace = Some(ns);
        self
    }
}

/// Trait for types that can be converted into NodeOptions
///
/// This enables the fluent API pattern:
/// ```ignore
/// context.create_node("my_node".namespace("/ns"))
/// ```
pub trait IntoNodeOptions<'a> {
    /// Convert into NodeOptions
    fn into_node_options(self) -> NodeOptions<'a>;
}

impl<'a> IntoNodeOptions<'a> for &'a str {
    fn into_node_options(self) -> NodeOptions<'a> {
        NodeOptions::new(self)
    }
}

impl<'a> IntoNodeOptions<'a> for NodeOptions<'a> {
    fn into_node_options(self) -> NodeOptions<'a> {
        self
    }
}

/// Extension trait for string slices to enable fluent node creation
pub trait NodeNameExt<'a> {
    /// Set the namespace for a node name
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let node = context.create_node("my_node".namespace("/ns"))?;
    /// ```
    fn namespace(self, ns: &'a str) -> NodeOptions<'a>;
}

impl<'a> NodeNameExt<'a> for &'a str {
    fn namespace(self, ns: &'a str) -> NodeOptions<'a> {
        NodeOptions::new(self).namespace(ns)
    }
}

/// Node type alias for convenience
///
/// Currently returns raw ConnectedNode. In future phases, this will be
/// wrapped with interior mutability (Mutex/RefCell) for shared ownership.
#[cfg(feature = "rmw-zenoh")]
pub type Node<const MAX_TOKENS: usize = { crate::DEFAULT_MAX_TOKENS }> = ConnectedNode<MAX_TOKENS>;

// RclrsError is defined in crate::error module (no alloc dependency)
pub use crate::error::RclrsError;

#[cfg(feature = "rmw-zenoh")]
impl From<crate::ConnectedNodeError> for RclrsError {
    fn from(e: crate::ConnectedNodeError) -> Self {
        use crate::ConnectedNodeError;
        match e {
            ConnectedNodeError::ConnectionFailed => RclrsError::ConnectionFailed,
            ConnectedNodeError::PublisherCreationFailed => RclrsError::PublisherCreationFailed,
            ConnectedNodeError::SubscriberCreationFailed => RclrsError::SubscriberCreationFailed,
            ConnectedNodeError::ServiceServerCreationFailed => {
                RclrsError::ServiceServerCreationFailed
            }
            ConnectedNodeError::ServiceClientCreationFailed => {
                RclrsError::ServiceClientCreationFailed
            }
            ConnectedNodeError::ActionServerCreationFailed => {
                RclrsError::ActionServerCreationFailed
            }
            ConnectedNodeError::ActionClientCreationFailed => {
                RclrsError::ActionClientCreationFailed
            }
            ConnectedNodeError::PublishFailed => RclrsError::PublishFailed,
            ConnectedNodeError::SerializationFailed => RclrsError::SerializationFailed,
            ConnectedNodeError::DeserializationFailed => RclrsError::DeserializationFailed,
            ConnectedNodeError::BufferTooSmall => RclrsError::BufferTooSmall,
            ConnectedNodeError::MessageTooLarge => RclrsError::MessageTooLarge,
            ConnectedNodeError::NoMessage => RclrsError::NoMessage,
            ConnectedNodeError::ServiceRequestFailed => RclrsError::ServiceRequestFailed,
            ConnectedNodeError::ServiceReplyFailed => RclrsError::ServiceReplyFailed,
            ConnectedNodeError::TaskStartFailed => RclrsError::TaskStartFailed,
            ConnectedNodeError::PollFailed => RclrsError::PollFailed,
            ConnectedNodeError::KeepaliveFailed => RclrsError::KeepaliveFailed,
            ConnectedNodeError::JoinFailed => RclrsError::JoinFailed,
            ConnectedNodeError::GoalRejected => RclrsError::GoalRejected,
            ConnectedNodeError::GoalNotFound => RclrsError::GoalNotFound,
            ConnectedNodeError::ActionServerFull => RclrsError::ActionServerFull,
            ConnectedNodeError::TimerCreationFailed => RclrsError::TimerCreationFailed,
            ConnectedNodeError::TimerNotFound => RclrsError::TimerNotFound,
            ConnectedNodeError::TimerStorageFull => RclrsError::TimerStorageFull,
            ConnectedNodeError::ServiceTimeout => RclrsError::ServiceTimeout,
            ConnectedNodeError::ServiceCancelled => RclrsError::ServiceCancelled,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "alloc")]
    extern crate alloc;
    #[cfg(feature = "alloc")]
    use alloc::vec;

    #[test]
    fn test_init_options() {
        let options = InitOptions::new();
        assert_eq!(options.domain_id, None);

        let options = InitOptions::new().with_domain_id(Some(42));
        assert_eq!(options.domain_id, Some(42));

        let options = InitOptions::new().domain_id(42);
        assert_eq!(options.domain_id, Some(42));
    }

    #[test]
    #[cfg(feature = "rmw-zenoh")]
    fn test_init_options_zenoh() {
        let options = InitOptions::new().locator("tcp/192.168.1.1:7447");
        assert_eq!(options.locator, Some("tcp/192.168.1.1:7447"));

        let options = InitOptions::new().peer_mode();
        assert_eq!(options.session_mode, SessionMode::Peer);
        assert_eq!(options.locator, None);
    }

    #[test]
    #[cfg(feature = "rmw-zenoh")]
    fn test_init_options_properties() {
        let options = InitOptions::new()
            .property("multicast_scouting", "false")
            .property("scouting_timeout_ms", "1000");
        assert_eq!(options.properties.len(), 2);
        assert_eq!(options.properties[0], ("multicast_scouting", "false"));
        assert_eq!(options.properties[1], ("scouting_timeout_ms", "1000"));
    }

    #[test]
    #[cfg(feature = "rmw-zenoh")]
    fn test_init_options_properties_default_empty() {
        let options = InitOptions::new();
        assert!(options.properties.is_empty());
    }

    #[test]
    #[cfg(feature = "rmw-zenoh")]
    fn test_init_options_properties_overflow_silently_ignored() {
        // heapless::Vec<_, 8> can hold at most 8 entries; the 9th push is silently dropped
        let mut options = InitOptions::new();
        for i in 0..10 {
            // Use a match to get &'static str keys
            let key: &'static str = match i {
                0 => "k0",
                1 => "k1",
                2 => "k2",
                3 => "k3",
                4 => "k4",
                5 => "k5",
                6 => "k6",
                7 => "k7",
                _ => "overflow",
            };
            options = options.property(key, "v");
        }
        // Should cap at 8 (heapless::Vec capacity)
        assert_eq!(options.properties.len(), 8);
    }

    #[test]
    #[cfg(feature = "rmw-zenoh")]
    fn test_context_with_properties_stores_config() {
        let context = Context::new(
            InitOptions::new()
                .locator("tcp/127.0.0.1:7447")
                .property("multicast_scouting", "false")
                .property("scouting_timeout_ms", "500"),
        )
        .unwrap();

        // Verify properties are stored in the transport config
        assert_eq!(context.transport_config.properties.len(), 2);
        assert_eq!(
            context.transport_config.properties[0],
            ("multicast_scouting", "false")
        );
        assert_eq!(
            context.transport_config.properties[1],
            ("scouting_timeout_ms", "500")
        );
    }

    #[test]
    #[cfg(feature = "rmw-zenoh")]
    fn test_context_without_properties_has_empty_config() {
        let context = Context::new(InitOptions::new()).unwrap();
        assert!(context.transport_config.properties.is_empty());
    }

    #[test]
    #[cfg(feature = "rmw-zenoh")]
    fn test_context_from_env_has_empty_properties() {
        // Clear env vars that could add properties
        // Safety: test-only env var manipulation, tests run serially via nextest
        unsafe {
            std::env::remove_var("ROS_LOCALHOST_ONLY");
            std::env::remove_var("ROS_AUTOMATIC_DISCOVERY_RANGE");
            std::env::remove_var("ROS_STATIC_PEERS");
            std::env::remove_var("ZENOH_MODE");
            std::env::remove_var("ZENOH_LOCATOR");
        }

        let context = Context::from_env().unwrap();
        assert!(context.transport_config.properties.is_empty());
    }

    #[test]
    #[cfg(feature = "rmw-zenoh")]
    fn test_context_default_from_env_has_empty_properties() {
        let context = Context::default_from_env().unwrap();
        assert!(context.transport_config.properties.is_empty());
    }

    // =========================================================================
    // ROS env var tests (Phase 19b)
    // =========================================================================

    #[test]
    #[cfg(all(feature = "std", feature = "rmw-zenoh", feature = "ros-humble"))]
    fn test_from_env_ros_localhost_only() {
        // Safety: test-only env var manipulation, tests run serially via nextest
        unsafe {
            std::env::set_var("ROS_LOCALHOST_ONLY", "1");
            std::env::remove_var("ZENOH_MODE");
            std::env::remove_var("ZENOH_LOCATOR");
            std::env::remove_var("ROS_AUTOMATIC_DISCOVERY_RANGE");
            std::env::remove_var("ROS_STATIC_PEERS");
        }

        let context = Context::from_env().unwrap();

        assert_eq!(context.transport_config.locator, Some("tcp/127.0.0.1:7447"));
        assert_eq!(context.transport_config.mode, SessionMode::Client);
        assert_eq!(context.transport_config.properties.len(), 1);
        assert_eq!(
            context.transport_config.properties[0],
            ("multicast_scouting", "false")
        );

        unsafe { std::env::remove_var("ROS_LOCALHOST_ONLY") };
    }

    #[test]
    #[cfg(all(feature = "std", feature = "rmw-zenoh", feature = "ros-humble"))]
    fn test_from_env_ros_localhost_only_zero() {
        // Safety: test-only env var manipulation, tests run serially via nextest
        unsafe {
            std::env::set_var("ROS_LOCALHOST_ONLY", "0");
            std::env::remove_var("ZENOH_MODE");
            std::env::remove_var("ZENOH_LOCATOR");
            std::env::remove_var("ROS_AUTOMATIC_DISCOVERY_RANGE");
            std::env::remove_var("ROS_STATIC_PEERS");
        }

        let context = Context::from_env().unwrap();
        assert!(context.transport_config.properties.is_empty());

        unsafe { std::env::remove_var("ROS_LOCALHOST_ONLY") };
    }

    #[test]
    #[cfg(all(feature = "std", feature = "rmw-zenoh", feature = "ros-humble"))]
    fn test_from_env_ros_localhost_only_peer_mode_ignored() {
        // Safety: test-only env var manipulation, tests run serially via nextest
        unsafe {
            std::env::set_var("ROS_LOCALHOST_ONLY", "1");
            std::env::set_var("ZENOH_MODE", "peer");
            std::env::remove_var("ZENOH_LOCATOR");
            std::env::remove_var("ROS_AUTOMATIC_DISCOVERY_RANGE");
            std::env::remove_var("ROS_STATIC_PEERS");
        }

        let context = Context::from_env().unwrap();

        assert_eq!(context.transport_config.mode, SessionMode::Peer);
        assert_eq!(context.transport_config.locator, None);
        assert!(context.transport_config.properties.is_empty());

        unsafe {
            std::env::remove_var("ROS_LOCALHOST_ONLY");
            std::env::remove_var("ZENOH_MODE");
        }
    }

    #[test]
    #[cfg(all(feature = "std", feature = "rmw-zenoh", feature = "ros-iron"))]
    fn test_from_env_discovery_range_localhost() {
        // Safety: test-only env var manipulation, tests run serially via nextest
        unsafe {
            std::env::set_var("ROS_AUTOMATIC_DISCOVERY_RANGE", "LOCALHOST");
            std::env::remove_var("ROS_LOCALHOST_ONLY");
            std::env::remove_var("ZENOH_MODE");
            std::env::remove_var("ZENOH_LOCATOR");
            std::env::remove_var("ROS_STATIC_PEERS");
        }

        let context = Context::from_env().unwrap();

        assert_eq!(context.transport_config.locator, Some("tcp/127.0.0.1:7447"));
        assert_eq!(context.transport_config.properties.len(), 1);
        assert_eq!(
            context.transport_config.properties[0],
            ("multicast_scouting", "false")
        );

        unsafe { std::env::remove_var("ROS_AUTOMATIC_DISCOVERY_RANGE") };
    }

    #[test]
    #[cfg(all(feature = "std", feature = "rmw-zenoh", feature = "ros-iron"))]
    fn test_from_env_discovery_range_off() {
        // Safety: test-only env var manipulation, tests run serially via nextest
        unsafe {
            std::env::set_var("ROS_AUTOMATIC_DISCOVERY_RANGE", "OFF");
            std::env::set_var("ZENOH_LOCATOR", "tcp/10.0.0.1:7447");
            std::env::remove_var("ROS_LOCALHOST_ONLY");
            std::env::remove_var("ZENOH_MODE");
            std::env::remove_var("ROS_STATIC_PEERS");
        }

        let context = Context::from_env().unwrap();

        // Locator should NOT be forced to loopback
        assert_eq!(context.transport_config.locator, Some("tcp/10.0.0.1:7447"));
        // Scouting should be disabled
        assert_eq!(context.transport_config.properties.len(), 1);
        assert_eq!(
            context.transport_config.properties[0],
            ("multicast_scouting", "false")
        );

        unsafe {
            std::env::remove_var("ROS_AUTOMATIC_DISCOVERY_RANGE");
            std::env::remove_var("ZENOH_LOCATOR");
        }
    }

    #[test]
    #[cfg(all(feature = "std", feature = "rmw-zenoh", feature = "ros-iron"))]
    fn test_from_env_discovery_range_supersedes_localhost_only() {
        // Safety: test-only env var manipulation, tests run serially via nextest
        unsafe {
            std::env::set_var("ROS_LOCALHOST_ONLY", "1");
            std::env::set_var("ROS_AUTOMATIC_DISCOVERY_RANGE", "SUBNET");
            std::env::remove_var("ZENOH_MODE");
            std::env::remove_var("ZENOH_LOCATOR");
            std::env::remove_var("ROS_STATIC_PEERS");
        }

        let context = Context::from_env().unwrap();

        // ROS_LOCALHOST_ONLY=1 sets flags, then SUBNET is a no-op in the RANGE handler.
        // So ROS_LOCALHOST_ONLY=1 still takes effect. This is correct ROS 2 behavior:
        // SUBNET means "use default behavior" which doesn't override ROS_LOCALHOST_ONLY.
        assert_eq!(context.transport_config.properties.len(), 1);

        unsafe {
            std::env::remove_var("ROS_LOCALHOST_ONLY");
            std::env::remove_var("ROS_AUTOMATIC_DISCOVERY_RANGE");
        }
    }

    #[test]
    #[cfg(all(feature = "std", feature = "rmw-zenoh", feature = "ros-iron"))]
    fn test_from_env_static_peers() {
        // Safety: test-only env var manipulation, tests run serially via nextest
        unsafe {
            std::env::set_var("ROS_STATIC_PEERS", "tcp/10.0.0.1:7447;tcp/10.0.0.2:7447");
            std::env::set_var("ZENOH_LOCATOR", "tcp/192.168.1.1:7447");
            std::env::remove_var("ROS_LOCALHOST_ONLY");
            std::env::remove_var("ROS_AUTOMATIC_DISCOVERY_RANGE");
            std::env::remove_var("ZENOH_MODE");
        }

        let context = Context::from_env().unwrap();

        // First entry from ROS_STATIC_PEERS should be used
        assert_eq!(context.transport_config.locator, Some("tcp/10.0.0.1:7447"));

        unsafe {
            std::env::remove_var("ROS_STATIC_PEERS");
            std::env::remove_var("ZENOH_LOCATOR");
        }
    }

    #[test]
    fn test_context_creation() {
        let context = Context::new(InitOptions::new()).unwrap();
        assert_eq!(context.domain_id(), 0);
        assert!(context.ok());

        let context = Context::new(InitOptions::new().with_domain_id(Some(42))).unwrap();
        assert_eq!(context.domain_id(), 42);
    }

    #[test]
    fn test_context_default() {
        let context = Context::default_from_env().unwrap();
        assert_eq!(context.domain_id(), 0);
    }

    #[test]
    fn test_node_options() {
        let options = NodeOptions::new("test_node");
        assert_eq!(options.name, "test_node");
        assert_eq!(options.namespace, None);

        let options = NodeOptions::new("test_node").namespace("/test_ns");
        assert_eq!(options.name, "test_node");
        assert_eq!(options.namespace, Some("/test_ns"));
    }

    #[test]
    fn test_into_node_options() {
        let options: NodeOptions = "test_node".into_node_options();
        assert_eq!(options.name, "test_node");
        assert_eq!(options.namespace, None);
    }

    #[test]
    fn test_node_name_ext() {
        let options = "test_node".namespace("/test_ns");
        assert_eq!(options.name, "test_node");
        assert_eq!(options.namespace, Some("/test_ns"));
    }

    #[test]
    #[cfg(feature = "alloc")]
    fn test_rclrs_error_first_error() {
        let errors = vec![RclrsError::ConnectionFailed, RclrsError::PublishFailed];
        let result = RclrsError::first_error(errors);
        assert_eq!(result, Err(RclrsError::ConnectionFailed));

        let errors: alloc::vec::Vec<RclrsError> = vec![];
        let result = RclrsError::first_error(errors);
        assert_eq!(result, Ok(()));
    }
}

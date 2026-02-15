//! Shim transport backend
//!
//! Provides a transport backend using the nros-rmw-zenoh wrapper.
//! This is designed for embedded platforms that need a simpler API than
//! the full zenoh-pico bindings.
//!
//! Requires the `shim` feature flag.
//!
//! # Features
//!
//! - Session management with ZenohId support
//! - Publishers with RMW attachment support for rmw_zenoh compatibility
//! - Subscribers with wildcard matching
//! - Liveliness tokens for ROS 2 discovery
//! - Service servers via queryables
//! - Service clients via z_get queries
//! - Manual polling (no background threads) for embedded systems
//!
//! # Example
//!
//! ```ignore
//! use nros_rmw::{Transport, TransportConfig, SessionMode};
//! use nros_rmw_zenoh::ShimTransport;
//!
//! // Create config
//! let config = TransportConfig {
//!     locator: Some("tcp/192.168.1.1:7447"),
//!     mode: SessionMode::Client,
//!     properties: &[],
//! };
//!
//! // Open session
//! let mut session = ShimTransport::open(&config).expect("Failed to open session");
//!
//! // Must poll periodically
//! session.spin_once(10)?;
//! ```

use core::marker::PhantomData;
use portable_atomic::{AtomicBool, AtomicUsize, Ordering};

// Use AtomicI64 on 64-bit targets, AtomicI32 on 32-bit (e.g. Cortex-M, riscv32)
// portable-atomic provides fetch_add/fetch_sub even on targets without native
// atomic RMW instructions (e.g. riscv32imc) via software fallback.
#[cfg(target_has_atomic = "64")]
type AtomicSeqCounter = portable_atomic::AtomicI64;
#[cfg(not(target_has_atomic = "64"))]
type AtomicSeqCounter = portable_atomic::AtomicI32;

use nros_rmw::{
    Publisher, QosSettings, Rmw, RmwConfig, ServiceClientTrait, ServiceInfo, ServiceRequest,
    ServiceServerTrait, Session, SessionMode, Subscriber, TopicInfo, Transport, TransportConfig,
    TransportError,
};

use crate::keyexpr::{QosKeyExpr, ServiceKeyExpr, TopicKeyExpr};
use crate::zpico::{
    ShimContext, ShimError, ShimLivelinessToken, ShimZenohId, ZENOH_SHIM_MAX_QUERYABLES,
    ZENOH_SHIM_MAX_SUBSCRIBERS, ZENOH_SHIM_RMW_GID_SIZE,
};

// Re-export for convenience
pub use crate::zpico::ShimZenohId as ZenohId;

// ============================================================================
// Constants
// ============================================================================

/// RMW GID size for attachment serialization (16 bytes for Humble)
pub const RMW_GID_SIZE: usize = ZENOH_SHIM_RMW_GID_SIZE;

/// Size of serialized RMW attachment (without safety CRC)
/// Format: sequence_number (8) + timestamp (8) + VLE length (1) + gid (16) = 33 bytes
const RMW_ATTACHMENT_SIZE: usize = 8 + 8 + 1 + RMW_GID_SIZE;

/// Size of the CRC-32 field appended when safety-e2e is enabled
#[cfg(feature = "safety-e2e")]
const SAFETY_CRC_SIZE: usize = 4;

/// Total attachment size with safety CRC (37 bytes)
#[cfg(feature = "safety-e2e")]
const RMW_ATTACHMENT_SIZE_WITH_CRC: usize = RMW_ATTACHMENT_SIZE + SAFETY_CRC_SIZE;

// ============================================================================
// Executor Wake Signal (std only)
// ============================================================================

/// Signal the executor that new data is available.
///
/// Called from subscription and service callbacks (which run on the zenoh-pico
/// background read thread) to wake the executor's `spin()` loop immediately
/// instead of waiting for the poll interval timeout.
#[cfg(feature = "std")]
pub fn signal_executor_wake() {
    let (lock, cvar) = &*EXECUTOR_WAKE;
    if let Ok(mut pending) = lock.lock() {
        *pending = true;
        cvar.notify_one();
    }
}

/// Wait for a wake signal or timeout.
///
/// Returns `true` if woken by a signal, `false` on timeout.
/// Used by `BasicExecutor::spin()` to sleep efficiently between iterations.
#[cfg(feature = "std")]
pub fn wait_for_executor_wake(timeout: core::time::Duration) -> bool {
    let (lock, cvar) = &*EXECUTOR_WAKE;
    if let Ok(mut pending) = lock.lock() {
        if *pending {
            *pending = false;
            return true;
        }
        let result = cvar.wait_timeout(pending, timeout);
        if let Ok((mut guard, _)) = result {
            let was_signaled = *guard;
            *guard = false;
            was_signaled
        } else {
            false
        }
    } else {
        // Mutex poisoned — fall back to sleep behavior
        std::thread::sleep(timeout);
        false
    }
}

#[cfg(feature = "std")]
static EXECUTOR_WAKE: std::sync::LazyLock<(std::sync::Mutex<bool>, std::sync::Condvar)> =
    std::sync::LazyLock::new(|| (std::sync::Mutex::new(false), std::sync::Condvar::new()));

// ============================================================================
// Error Conversion
// ============================================================================

impl From<ShimError> for TransportError {
    fn from(err: ShimError) -> Self {
        match err {
            ShimError::Generic => TransportError::ConnectionFailed,
            ShimError::Config => TransportError::InvalidConfig,
            ShimError::Session => TransportError::ConnectionFailed,
            ShimError::Task => TransportError::TaskStartFailed,
            ShimError::KeyExpr => TransportError::InvalidConfig,
            ShimError::Full => TransportError::PublisherCreationFailed,
            ShimError::Invalid => TransportError::InvalidConfig,
            ShimError::Publish => TransportError::PublishFailed,
            ShimError::NotOpen => TransportError::Disconnected,
            ShimError::Timeout => TransportError::Timeout,
        }
    }
}

// ============================================================================
// RMW Attachment Support
// ============================================================================

/// RMW attachment data for rmw_zenoh compatibility
///
/// This metadata is attached to each published message and is required
/// for ROS 2 nodes using rmw_zenoh_cpp to receive messages.
#[derive(Debug, Clone, Copy)]
pub struct RmwAttachment {
    /// Message sequence number (incremented per publish)
    pub sequence_number: i64,
    /// Timestamp in nanoseconds
    pub timestamp: i64,
    /// RMW Global Identifier (random, generated once per publisher)
    pub rmw_gid: [u8; RMW_GID_SIZE],
}

impl RmwAttachment {
    /// Create a new attachment with a random GID
    pub fn new() -> Self {
        Self {
            sequence_number: 0,
            timestamp: 0,
            rmw_gid: Self::generate_gid(),
        }
    }

    /// Generate a random GID using a simple PRNG
    pub fn generate_gid() -> [u8; RMW_GID_SIZE] {
        let mut gid = [0u8; RMW_GID_SIZE];
        static COUNTER: AtomicSeqCounter = AtomicSeqCounter::new(0);
        let seed = COUNTER.fetch_add(1, Ordering::Relaxed) as u64;
        // Use address of gid as additional entropy
        let addr = &gid as *const _ as u64;
        let mixed = seed.wrapping_mul(0x517cc1b727220a95) ^ addr;

        for (i, byte) in gid.iter_mut().enumerate() {
            let shift = (i % 8) * 8;
            *byte = ((mixed.wrapping_mul((i as u64).wrapping_add(1))) >> shift) as u8;
        }
        gid
    }

    /// Serialize the attachment in the format expected by rmw_zenoh_cpp
    ///
    /// Format:
    /// - int64: sequence_number (little-endian, 8 bytes)
    /// - int64: timestamp (little-endian, 8 bytes)
    /// - VLE length (1 byte for length 16)
    /// - 16 x uint8: GID
    pub fn serialize(&self, buf: &mut [u8; RMW_ATTACHMENT_SIZE]) {
        // Sequence number (little-endian)
        buf[0..8].copy_from_slice(&self.sequence_number.to_le_bytes());
        // Timestamp (little-endian)
        buf[8..16].copy_from_slice(&self.timestamp.to_le_bytes());
        // VLE length (16 fits in single byte)
        buf[16] = RMW_GID_SIZE as u8;
        // GID bytes
        buf[17..33].copy_from_slice(&self.rmw_gid);
    }
}

impl Default for RmwAttachment {
    fn default() -> Self {
        Self::new()
    }
}

impl RmwAttachment {
    /// Deserialize attachment from raw bytes received from RMW
    ///
    /// Format:
    /// - int64: sequence_number (little-endian, 8 bytes)
    /// - int64: timestamp (little-endian, 8 bytes)
    /// - VLE length (1 byte for length 16)
    /// - 16 x uint8: GID
    ///
    /// Returns None if the buffer is too small or malformed.
    pub fn deserialize(buf: &[u8]) -> Option<Self> {
        if buf.len() < RMW_ATTACHMENT_SIZE {
            return None;
        }

        // Parse sequence number (little-endian)
        let sequence_number = i64::from_le_bytes([
            buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7],
        ]);

        // Parse timestamp (little-endian)
        let timestamp = i64::from_le_bytes([
            buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15],
        ]);

        // Parse VLE length (should be 16)
        let gid_len = buf[16] as usize;
        if gid_len != RMW_GID_SIZE {
            return None;
        }

        // Parse GID
        let mut rmw_gid = [0u8; RMW_GID_SIZE];
        rmw_gid.copy_from_slice(&buf[17..33]);

        Some(Self {
            sequence_number,
            timestamp,
            rmw_gid,
        })
    }
}

/// Message information parsed from RMW attachment
///
/// This struct contains metadata about a received message, extracted
/// from the rmw_zenoh attachment.
#[derive(Debug, Clone, Copy)]
pub struct MessageInfo {
    /// Message sequence number from the publisher
    pub sequence_number: i64,
    /// Timestamp in nanoseconds (source time)
    pub timestamp_ns: i64,
    /// Publisher's Global Identifier
    pub publisher_gid: [u8; RMW_GID_SIZE],
}

impl MessageInfo {
    /// Parse MessageInfo from raw attachment data
    ///
    /// Returns None if no attachment or if parsing fails.
    pub fn from_attachment(attachment: &[u8]) -> Option<Self> {
        RmwAttachment::deserialize(attachment).map(|att| Self {
            sequence_number: att.sequence_number,
            timestamp_ns: att.timestamp,
            publisher_gid: att.rmw_gid,
        })
    }
}

// ============================================================================
// Ros2Liveliness Helper
// ============================================================================

/// ROS 2 liveliness key expression builder for the shim transport
///
/// Generates the key expressions required for ROS 2 discovery via rmw_zenoh.
pub struct Ros2Liveliness;

impl Ros2Liveliness {
    /// Build a node liveliness key expression
    ///
    /// Format: `@ros2_lv/<domain_id>/<zid>/0/0/NN/%/<mangled_ns>/<node_name>`
    ///
    /// The namespace is mangled using the same rules as topic names:
    /// - `/` → `%`
    /// - `/demo` → `%demo`
    /// - `/ns/sub` → `%ns%sub`
    pub fn node_keyexpr<const N: usize>(
        domain_id: u32,
        zid: &ShimZenohId,
        namespace: &str,
        node_name: &str,
    ) -> heapless::String<N> {
        let mut key = heapless::String::new();
        let mut zid_hex = [0u8; 32];
        zid.to_hex_bytes(&mut zid_hex);
        let zid_str = core::str::from_utf8(&zid_hex).unwrap_or("");
        let ns_mangled = Self::mangle_topic_name::<64>(namespace);
        let _ = core::fmt::write(
            &mut key,
            format_args!(
                "@ros2_lv/{}/{}/0/0/NN/%/{}/{}",
                domain_id,
                zid_str,
                ns_mangled.as_str(),
                node_name
            ),
        );
        key
    }

    /// Build a publisher liveliness key expression
    ///
    /// Format: `@ros2_lv/<domain_id>/<zid>/0/11/MP/%/<mangled_ns>/<node_name>/<topic>/<type>/<hash>/<qos>`
    /// Note: type_hash already includes the `RIHS01_` prefix from generated code
    pub fn publisher_keyexpr<const N: usize>(
        domain_id: u32,
        zid: &ShimZenohId,
        namespace: &str,
        node_name: &str,
        topic: &TopicInfo,
        qos: &QosSettings,
    ) -> heapless::String<N> {
        let mut key = heapless::String::new();
        let mut zid_hex = [0u8; 32];
        zid.to_hex_bytes(&mut zid_hex);
        let zid_str = core::str::from_utf8(&zid_hex).unwrap_or("");
        // Mangle topic name: replace slashes with percent signs
        let topic_mangled = Self::mangle_topic_name::<64>(topic.name);
        let ns_mangled = Self::mangle_topic_name::<64>(namespace);
        let qos_string: heapless::String<32> = qos.to_qos_string();
        let _ = core::fmt::write(
            &mut key,
            format_args!(
                "@ros2_lv/{}/{}/0/11/MP/%/{}/{}/{}/{}/{}/{}",
                domain_id,
                zid_str,
                ns_mangled.as_str(),
                node_name,
                topic_mangled.as_str(),
                topic.type_name,
                topic.type_hash,
                qos_string.as_str()
            ),
        );
        key
    }

    /// Build a subscriber liveliness key expression
    ///
    /// Format: `@ros2_lv/<domain_id>/<zid>/0/11/MS/%/<mangled_ns>/<node_name>/<topic>/<type>/<hash>/<qos>`
    /// Note: type_hash already includes the `RIHS01_` prefix from generated code
    pub fn subscriber_keyexpr<const N: usize>(
        domain_id: u32,
        zid: &ShimZenohId,
        namespace: &str,
        node_name: &str,
        topic: &TopicInfo,
        qos: &QosSettings,
    ) -> heapless::String<N> {
        let mut key = heapless::String::new();
        let mut zid_hex = [0u8; 32];
        zid.to_hex_bytes(&mut zid_hex);
        let zid_str = core::str::from_utf8(&zid_hex).unwrap_or("");
        let topic_mangled = Self::mangle_topic_name::<64>(topic.name);
        let ns_mangled = Self::mangle_topic_name::<64>(namespace);
        let qos_string: heapless::String<32> = qos.to_qos_string();
        let _ = core::fmt::write(
            &mut key,
            format_args!(
                "@ros2_lv/{}/{}/0/11/MS/%/{}/{}/{}/{}/{}/{}",
                domain_id,
                zid_str,
                ns_mangled.as_str(),
                node_name,
                topic_mangled.as_str(),
                topic.type_name,
                topic.type_hash,
                qos_string.as_str()
            ),
        );
        key
    }

    /// Build a service server liveliness key expression
    ///
    /// Format: `@ros2_lv/<domain_id>/<zid>/0/11/SS/%/<mangled_ns>/<node_name>/<service>/<type>/<hash>/<qos>`
    /// Note: type_hash already includes the `RIHS01_` prefix from generated code
    pub fn service_server_keyexpr<const N: usize>(
        domain_id: u32,
        zid: &ShimZenohId,
        namespace: &str,
        node_name: &str,
        service: &ServiceInfo,
        qos: &QosSettings,
    ) -> heapless::String<N> {
        let mut key = heapless::String::new();
        let mut zid_hex = [0u8; 32];
        zid.to_hex_bytes(&mut zid_hex);
        let zid_str = core::str::from_utf8(&zid_hex).unwrap_or("");
        let service_mangled = Self::mangle_topic_name::<64>(service.name);
        let ns_mangled = Self::mangle_topic_name::<64>(namespace);
        let qos_string: heapless::String<32> = qos.to_qos_string();
        let _ = core::fmt::write(
            &mut key,
            format_args!(
                "@ros2_lv/{}/{}/0/11/SS/%/{}/{}/{}/{}/{}/{}",
                domain_id,
                zid_str,
                ns_mangled.as_str(),
                node_name,
                service_mangled.as_str(),
                service.type_name,
                service.type_hash,
                qos_string.as_str()
            ),
        );
        key
    }

    /// Build a service client liveliness key expression
    ///
    /// Format: `@ros2_lv/<domain_id>/<zid>/0/11/SC/%/<mangled_ns>/<node_name>/<service>/<type>/<hash>/<qos>`
    /// Note: type_hash already includes the `RIHS01_` prefix from generated code
    pub fn service_client_keyexpr<const N: usize>(
        domain_id: u32,
        zid: &ShimZenohId,
        namespace: &str,
        node_name: &str,
        service: &ServiceInfo,
        qos: &QosSettings,
    ) -> heapless::String<N> {
        let mut key = heapless::String::new();
        let mut zid_hex = [0u8; 32];
        zid.to_hex_bytes(&mut zid_hex);
        let zid_str = core::str::from_utf8(&zid_hex).unwrap_or("");
        let service_mangled = Self::mangle_topic_name::<64>(service.name);
        let ns_mangled = Self::mangle_topic_name::<64>(namespace);
        let qos_string: heapless::String<32> = qos.to_qos_string();
        let _ = core::fmt::write(
            &mut key,
            format_args!(
                "@ros2_lv/{}/{}/0/11/SC/%/{}/{}/{}/{}/{}/{}",
                domain_id,
                zid_str,
                ns_mangled.as_str(),
                node_name,
                service_mangled.as_str(),
                service.type_name,
                service.type_hash,
                qos_string.as_str()
            ),
        );
        key
    }

    /// Mangle a topic name by replacing '/' with '%'
    fn mangle_topic_name<const N: usize>(topic: &str) -> heapless::String<N> {
        let mut mangled = heapless::String::new();
        for c in topic.chars() {
            if c == '/' {
                let _ = mangled.push('%');
            } else {
                let _ = mangled.push(c);
            }
        }
        mangled
    }
}

// ============================================================================
// ShimTransport
// ============================================================================

/// Shim transport backend for embedded platforms
///
/// Uses nros-rmw-zenoh for a simplified API suitable for bare-metal systems.
pub struct ShimTransport;

impl Transport for ShimTransport {
    type Error = TransportError;
    type Session = ShimSession;

    fn open(config: &TransportConfig) -> Result<Self::Session, Self::Error> {
        ShimSession::new(config)
    }
}

// ============================================================================
// ZenohRmw
// ============================================================================

/// Zenoh-pico RMW backend for compile-time middleware selection.
///
/// Implements the [`Rmw`] factory trait, bridging from the
/// middleware-agnostic [`RmwConfig`] to zenoh-pico session initialization.
///
/// # Example
///
/// ```ignore
/// use nros_rmw::{Rmw, RmwConfig, SessionMode};
/// use nros_rmw_zenoh::ZenohRmw;
///
/// let config = RmwConfig {
///     locator: "tcp/192.168.1.1:7447",
///     mode: SessionMode::Client,
///     domain_id: 0,
///     node_name: "talker",
///     namespace: "",
/// };
/// let session = ZenohRmw::open(&config).unwrap();
/// ```
pub struct ZenohRmw;

impl Rmw for ZenohRmw {
    type Session = ShimSession;
    type Error = TransportError;

    fn open(config: &RmwConfig) -> Result<Self::Session, Self::Error> {
        let transport_config = TransportConfig {
            locator: Some(config.locator),
            mode: config.mode,
            properties: &[],
        };
        ShimSession::new(&transport_config)
    }
}

// ============================================================================
// ShimSession
// ============================================================================

/// Shim session wrapping nros-rmw-zenoh ShimContext
///
/// This session requires manual polling via `spin_once()` or `poll()`.
/// There are no background threads.
pub struct ShimSession {
    context: ShimContext,
}

impl ShimSession {
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
                let mut buf = [0u8; 128];
                let bytes = loc.as_bytes();
                if bytes.len() >= buf.len() {
                    return Err(TransportError::InvalidConfig);
                }
                buf[..bytes.len()].copy_from_slice(bytes);
                buf[bytes.len()] = 0; // Null terminator
                buf
            }
            (SessionMode::Client, None) => {
                return Err(TransportError::InvalidConfig);
            }
            (SessionMode::Peer, _) => {
                // Peer mode - pass null locator
                [0u8; 128]
            }
        };

        // Build mode string
        let mode: &[u8] = match config.mode {
            SessionMode::Client => b"client\0",
            SessionMode::Peer => b"peer\0",
        };

        // Build null-terminated property strings on the stack
        // Each key/value is at most 64 bytes
        let mut key_bufs = [[0u8; 64]; 8];
        let mut val_bufs = [[0u8; 64]; 8];
        let mut c_props: [crate::zpico::zenoh_shim_property_t; 8] = unsafe { core::mem::zeroed() };

        let mut prop_count = 0usize;

        // Copy explicit properties from config
        for i in 0..config.properties.len().min(8) {
            let (key, value) = config.properties[i];
            let key_bytes = key.as_bytes();
            let val_bytes = value.as_bytes();
            if key_bytes.len() >= 64 || val_bytes.len() >= 64 {
                continue; // Skip oversized properties
            }
            key_bufs[prop_count][..key_bytes.len()].copy_from_slice(key_bytes);
            key_bufs[prop_count][key_bytes.len()] = 0;
            val_bufs[prop_count][..val_bytes.len()].copy_from_slice(val_bytes);
            val_bufs[prop_count][val_bytes.len()] = 0;
            c_props[prop_count] = crate::zpico::zenoh_shim_property_t {
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
            ];
            for &(env_name, prop_key) in env_mappings {
                if let Ok(val) = std::env::var(env_name) {
                    let already_set = config.properties.iter().any(|(k, _)| *k == prop_key);
                    if !already_set && prop_count < 8 {
                        let key_bytes = prop_key.as_bytes();
                        let val_bytes = val.as_bytes();
                        if key_bytes.len() < 64 && val_bytes.len() < 64 {
                            key_bufs[prop_count][..key_bytes.len()].copy_from_slice(key_bytes);
                            key_bufs[prop_count][key_bytes.len()] = 0;
                            val_bufs[prop_count][..val_bytes.len()].copy_from_slice(val_bytes);
                            val_bufs[prop_count][val_bytes.len()] = 0;
                            c_props[prop_count] = crate::zpico::zenoh_shim_property_t {
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

        let context = ShimContext::with_config(locator_opt, mode, &c_props[..prop_count])
            .map_err(TransportError::from)?;

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

    /// Poll for incoming data and process callbacks
    ///
    /// # Arguments
    ///
    /// * `timeout_ms` - Maximum time to wait for data (0 = non-blocking)
    ///
    /// # Returns
    ///
    /// Number of events processed, or error
    pub fn poll(&self, timeout_ms: u32) -> Result<i32, TransportError> {
        self.context.poll(timeout_ms).map_err(TransportError::from)
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

    /// Get a reference to the underlying ShimContext
    pub fn inner(&self) -> &ShimContext {
        &self.context
    }

    /// Get the session's Zenoh ID
    ///
    /// The Zenoh ID uniquely identifies this session in the Zenoh network.
    /// It is used in liveliness token key expressions for ROS 2 discovery.
    pub fn zid(&self) -> Result<ShimZenohId, TransportError> {
        self.context.zid().map_err(TransportError::from)
    }

    /// Declare a liveliness token for ROS 2 discovery
    ///
    /// This creates a liveliness token at the given key expression,
    /// allowing ROS 2 nodes using rmw_zenoh to discover this entity.
    ///
    /// The key expression should be null-terminated.
    pub fn declare_liveliness(
        &self,
        keyexpr: &[u8],
    ) -> Result<ShimLivelinessToken, TransportError> {
        self.context
            .declare_liveliness(keyexpr)
            .map_err(TransportError::from)
    }
}

impl Session for ShimSession {
    type Error = TransportError;
    type PublisherHandle = ShimPublisher;
    type SubscriberHandle = ShimSubscriber;
    type ServiceServerHandle = ShimServiceServer;
    type ServiceClientHandle = ShimServiceClient;

    fn create_publisher(
        &mut self,
        topic: &TopicInfo,
        _qos: QosSettings,
    ) -> Result<Self::PublisherHandle, Self::Error> {
        ShimPublisher::new(&self.context, topic)
    }

    fn create_subscriber(
        &mut self,
        topic: &TopicInfo,
        _qos: QosSettings,
    ) -> Result<Self::SubscriberHandle, Self::Error> {
        ShimSubscriber::new(&self.context, topic)
    }

    fn create_service_server(
        &mut self,
        service: &ServiceInfo,
    ) -> Result<Self::ServiceServerHandle, Self::Error> {
        ShimServiceServer::new(&self.context, service)
    }

    fn create_service_client(
        &mut self,
        service: &ServiceInfo,
    ) -> Result<Self::ServiceClientHandle, Self::Error> {
        ShimServiceClient::new(&self.context, service)
    }

    fn close(&mut self) -> Result<(), Self::Error> {
        // Context is closed on drop
        Ok(())
    }
}

// ============================================================================
// ShimPublisher
// ============================================================================

/// Shim publisher wrapping nros-rmw-zenoh ShimPublisher
///
/// Includes RMW attachment support for rmw_zenoh compatibility.
pub struct ShimPublisher {
    publisher: crate::zpico::ShimPublisher<'static>,
    /// RMW GID (generated once per publisher)
    rmw_gid: [u8; RMW_GID_SIZE],
    /// Sequence number counter (atomic for interior mutability)
    sequence_counter: AtomicSeqCounter,
    /// Timestamp counter (until platform time is available)
    timestamp_counter: AtomicSeqCounter,
}

impl ShimPublisher {
    /// Create a new publisher for the given topic
    pub fn new(context: &ShimContext, topic: &TopicInfo) -> Result<Self, TransportError> {
        // Generate the topic key with null terminator
        let key: heapless::String<256> = topic.to_key();

        #[cfg(feature = "std")]
        log::debug!("Publisher data keyexpr: {}", key.as_str());

        // Create null-terminated keyexpr
        let mut keyexpr_buf = [0u8; 257];
        let bytes = key.as_bytes();
        if bytes.len() >= keyexpr_buf.len() {
            return Err(TransportError::InvalidConfig);
        }
        keyexpr_buf[..bytes.len()].copy_from_slice(bytes);
        keyexpr_buf[bytes.len()] = 0;

        // Safety: We need to extend the lifetime because ShimPublisher borrows from ShimContext.
        // This is safe because:
        // 1. ShimPublisher is stored in ShimSession which owns the ShimContext
        // 2. The underlying C shim manages its own state
        // 3. We transmute the lifetime to 'static for storage
        let publisher = unsafe {
            let pub_result = context.declare_publisher(&keyexpr_buf);
            match pub_result {
                Ok(p) => core::mem::transmute::<
                    crate::zpico::ShimPublisher<'_>,
                    crate::zpico::ShimPublisher<'static>,
                >(p),
                Err(e) => return Err(TransportError::from(e)),
            }
        };

        Ok(Self {
            publisher,
            rmw_gid: RmwAttachment::generate_gid(),
            sequence_counter: AtomicSeqCounter::new(0),
            timestamp_counter: AtomicSeqCounter::new(0),
        })
    }

    /// Get current timestamp in nanoseconds (placeholder until platform time available)
    fn current_timestamp(&self) -> i64 {
        // Increment by 1ms equivalent
        #[allow(clippy::useless_conversion)] // i32→i64 on embedded, no-op on std
        self.timestamp_counter
            .fetch_add(1_000_000, Ordering::Relaxed)
            .into()
    }

    /// Serialize attachment for RMW compatibility
    fn serialize_attachment(&self, seq: i64, ts: i64, buf: &mut [u8; RMW_ATTACHMENT_SIZE]) {
        // Sequence number (little-endian)
        buf[0..8].copy_from_slice(&seq.to_le_bytes());
        // Timestamp (little-endian)
        buf[8..16].copy_from_slice(&ts.to_le_bytes());
        // VLE length (16 fits in single byte)
        buf[16] = RMW_GID_SIZE as u8;
        // GID bytes
        buf[17..33].copy_from_slice(&self.rmw_gid);
    }
}

impl Publisher for ShimPublisher {
    type Error = TransportError;

    fn publish_raw(&self, data: &[u8]) -> Result<(), Self::Error> {
        // Get next sequence number and timestamp atomically
        #[allow(clippy::useless_conversion)] // i32→i64 on embedded, no-op on std
        let seq: i64 = (self.sequence_counter.fetch_add(1, Ordering::Relaxed) + 1).into();
        let ts = self.current_timestamp();

        // Without safety-e2e: 33-byte attachment
        #[cfg(not(feature = "safety-e2e"))]
        {
            let mut att_buf = [0u8; RMW_ATTACHMENT_SIZE];
            self.serialize_attachment(seq, ts, &mut att_buf);

            #[cfg(feature = "std")]
            log::trace!(
                "Publishing {} bytes with attachment: seq={}, ts={}, gid={:02x?}",
                data.len(),
                seq,
                ts,
                &self.rmw_gid[..4],
            );

            self.publisher
                .publish_with_attachment(data, Some(&att_buf))
                .map_err(TransportError::from)
        }

        // With safety-e2e: 37-byte attachment (33 + 4-byte CRC of payload)
        #[cfg(feature = "safety-e2e")]
        {
            let mut att_buf = [0u8; RMW_ATTACHMENT_SIZE_WITH_CRC];
            self.serialize_attachment(
                seq,
                ts,
                (&mut att_buf[..RMW_ATTACHMENT_SIZE]).try_into().unwrap(),
            );

            // Compute CRC-32 over CDR payload and append
            let crc = nros_rmw::crc32(data);
            att_buf[RMW_ATTACHMENT_SIZE..RMW_ATTACHMENT_SIZE_WITH_CRC]
                .copy_from_slice(&crc.to_le_bytes());

            #[cfg(feature = "std")]
            log::trace!(
                "Publishing {} bytes with safety attachment: seq={}, ts={}, crc={:#010x}",
                data.len(),
                seq,
                ts,
                crc,
            );

            self.publisher
                .publish_with_attachment(data, Some(&att_buf))
                .map_err(TransportError::from)
        }
    }

    fn buffer_error(&self) -> Self::Error {
        TransportError::BufferTooSmall
    }

    fn serialization_error(&self) -> Self::Error {
        TransportError::SerializationError
    }
}

// ============================================================================
// ShimSubscriber
// ============================================================================

/// Attachment buffer size: 33 bytes normally, 37 with safety CRC
#[cfg(not(feature = "safety-e2e"))]
const SUBSCRIBER_ATTACHMENT_BUF_SIZE: usize = RMW_ATTACHMENT_SIZE;
#[cfg(feature = "safety-e2e")]
const SUBSCRIBER_ATTACHMENT_BUF_SIZE: usize = RMW_ATTACHMENT_SIZE_WITH_CRC;

/// Default size for subscriber payload buffers (bytes).
pub const SUBSCRIBER_BUFFER_SIZE: usize = 1024;

/// Default size for service request buffers (bytes).
pub const SERVICE_BUFFER_SIZE: usize = 1024;

/// Shared buffer for subscriber callbacks
///
/// This buffer stores the most recent message received by the subscriber,
/// including the RMW attachment data for MessageInfo support.
/// The callback writes to this buffer, and `try_recv_raw` reads from it.
struct SubscriberBuffer {
    /// Buffer for received payload data (statically allocated)
    data: [u8; SUBSCRIBER_BUFFER_SIZE],
    /// Buffer for received attachment data (33 or 37 bytes depending on safety-e2e)
    attachment: [u8; SUBSCRIBER_ATTACHMENT_BUF_SIZE],
    /// Flag indicating new data is available
    has_data: AtomicBool,
    /// Flag indicating the incoming message exceeded the buffer capacity.
    /// Set by the callback when `len > data.len()`. Checked by `try_recv_raw`
    /// which returns `Err(MessageTooLarge)` and clears this flag.
    overflow: AtomicBool,
    /// Length of valid payload data
    len: AtomicUsize,
    /// Length of valid attachment data
    attachment_len: AtomicUsize,
}

impl SubscriberBuffer {
    const fn new() -> Self {
        Self {
            data: [0u8; SUBSCRIBER_BUFFER_SIZE],
            attachment: [0u8; SUBSCRIBER_ATTACHMENT_BUF_SIZE],
            has_data: AtomicBool::new(false),
            overflow: AtomicBool::new(false),
            len: AtomicUsize::new(0),
            attachment_len: AtomicUsize::new(0),
        }
    }
}

/// Static buffers for subscribers.
///
/// Count matches `ZENOH_SHIM_MAX_SUBSCRIBERS` from zpico-sys (the C shim
/// allocates the same number of subscriber entries). We use static buffers
/// because the shim callback mechanism requires a static context pointer.
static mut SUBSCRIBER_BUFFERS: [SubscriberBuffer; ZENOH_SHIM_MAX_SUBSCRIBERS] =
    [const { SubscriberBuffer::new() }; ZENOH_SHIM_MAX_SUBSCRIBERS];

/// Next available buffer index
static NEXT_BUFFER_INDEX: AtomicUsize = AtomicUsize::new(0);

/// Callback function invoked by the C shim when data arrives (with attachment)
extern "C" fn subscriber_callback_with_attachment(
    data: *const u8,
    len: usize,
    attachment: *const u8,
    attachment_len: usize,
    ctx: *mut core::ffi::c_void,
) {
    let buffer_index = ctx as usize;
    if buffer_index >= 8 {
        return;
    }

    // Safety: We control access to SUBSCRIBER_BUFFERS and the callback is single-threaded
    unsafe {
        let buffer = &mut SUBSCRIBER_BUFFERS[buffer_index];

        if len > buffer.data.len() {
            // Message exceeds static buffer capacity — flag as overflow instead of
            // silently truncating. The consumer will see MessageTooLarge and can recover.
            buffer.overflow.store(true, Ordering::Release);
            buffer.has_data.store(true, Ordering::Release);
        } else {
            // Normal case: copy payload data
            buffer.overflow.store(false, Ordering::Release);
            core::ptr::copy_nonoverlapping(data, buffer.data.as_mut_ptr(), len);
            buffer.len.store(len, Ordering::Release);

            // Copy attachment data if present
            if !attachment.is_null() && attachment_len > 0 {
                let att_copy_len = attachment_len.min(buffer.attachment.len());
                core::ptr::copy_nonoverlapping(
                    attachment,
                    buffer.attachment.as_mut_ptr(),
                    att_copy_len,
                );
                buffer.attachment_len.store(att_copy_len, Ordering::Release);
            } else {
                buffer.attachment_len.store(0, Ordering::Release);
            }

            buffer.has_data.store(true, Ordering::Release);
        }

        // Wake the executor spin loop (if waiting)
        #[cfg(feature = "std")]
        signal_executor_wake();
    }
}

/// Shim subscriber wrapping nros-rmw-zenoh ShimSubscriber
pub struct ShimSubscriber {
    /// The subscriber handle (kept alive to maintain subscription)
    _subscriber: crate::zpico::ShimSubscriber<'static>,
    /// Index into the static buffer array
    buffer_index: usize,
    /// E2E safety validator (tracks sequence numbers, validates CRC)
    #[cfg(feature = "safety-e2e")]
    safety_validator: nros_rmw::SafetyValidator,
    /// Phantom to indicate we don't own the buffer
    _phantom: PhantomData<()>,
}

impl ShimSubscriber {
    /// Create a new subscriber for the given topic
    pub fn new(context: &ShimContext, topic: &TopicInfo) -> Result<Self, TransportError> {
        // Allocate a buffer index
        let buffer_index = NEXT_BUFFER_INDEX.fetch_add(1, Ordering::SeqCst);
        if buffer_index >= 8 {
            // Roll back and return error
            NEXT_BUFFER_INDEX.fetch_sub(1, Ordering::SeqCst);
            return Err(TransportError::SubscriberCreationFailed);
        }

        // Generate the topic key with wildcard for type hash
        let key: heapless::String<256> = topic.to_key_wildcard();

        #[cfg(feature = "std")]
        log::debug!("Subscriber data keyexpr: {}", key.as_str());

        // Create null-terminated keyexpr
        let mut keyexpr_buf = [0u8; 257];
        let bytes = key.as_bytes();
        if bytes.len() >= keyexpr_buf.len() {
            return Err(TransportError::InvalidConfig);
        }
        keyexpr_buf[..bytes.len()].copy_from_slice(bytes);
        keyexpr_buf[bytes.len()] = 0;

        // Create subscriber with callback (using attachment-enabled callback for RMW support)
        // Safety: Similar to publisher, we transmute lifetime for storage
        let subscriber = unsafe {
            let sub_result = context.declare_subscriber_with_attachment_raw(
                &keyexpr_buf,
                subscriber_callback_with_attachment,
                buffer_index as *mut core::ffi::c_void,
            );
            match sub_result {
                Ok(s) => core::mem::transmute::<
                    crate::zpico::ShimSubscriber<'_>,
                    crate::zpico::ShimSubscriber<'static>,
                >(s),
                Err(e) => return Err(TransportError::from(e)),
            }
        };

        Ok(Self {
            _subscriber: subscriber,
            buffer_index,
            #[cfg(feature = "safety-e2e")]
            safety_validator: nros_rmw::SafetyValidator::new(),
            _phantom: PhantomData,
        })
    }
}

impl ShimSubscriber {
    /// Try to receive a validated message with E2E integrity status.
    ///
    /// Checks CRC-32 integrity and sequence continuity. Returns
    /// `(payload_len, IntegrityStatus)` so the caller can decide whether
    /// to trust the data.
    ///
    /// The payload bytes are written to `buf[..len]`.
    #[cfg(feature = "safety-e2e")]
    pub fn try_recv_validated(
        &mut self,
        buf: &mut [u8],
    ) -> Result<Option<(usize, nros_rmw::IntegrityStatus)>, TransportError> {
        // Safety: We own this buffer index and access is atomic
        let buffer = unsafe { &SUBSCRIBER_BUFFERS[self.buffer_index] };

        if !buffer.has_data.load(Ordering::Acquire) {
            return Ok(None);
        }

        // Check for overflow
        if buffer.overflow.load(Ordering::Acquire) {
            buffer.overflow.store(false, Ordering::Release);
            buffer.has_data.store(false, Ordering::Release);
            return Err(TransportError::MessageTooLarge);
        }

        let len = buffer.len.load(Ordering::Acquire);
        if len > buf.len() {
            buffer.has_data.store(false, Ordering::Release);
            return Err(TransportError::BufferTooSmall);
        }

        // Copy payload data
        unsafe {
            core::ptr::copy_nonoverlapping(
                SUBSCRIBER_BUFFERS[self.buffer_index].data.as_ptr(),
                buf.as_mut_ptr(),
                len,
            );
        }

        // Parse attachment for sequence number and CRC
        let attachment_len = buffer.attachment_len.load(Ordering::Acquire);
        let (message_seq, crc_valid) = if attachment_len >= RMW_ATTACHMENT_SIZE {
            // Extract sequence number (bytes 0..8, LE)
            let att = unsafe { &SUBSCRIBER_BUFFERS[self.buffer_index].attachment };
            let seq = i64::from_le_bytes([
                att[0], att[1], att[2], att[3], att[4], att[5], att[6], att[7],
            ]);

            // Check for CRC (bytes 33..37)
            let crc_result = if attachment_len >= RMW_ATTACHMENT_SIZE + SAFETY_CRC_SIZE {
                let received_crc = u32::from_le_bytes([
                    att[RMW_ATTACHMENT_SIZE],
                    att[RMW_ATTACHMENT_SIZE + 1],
                    att[RMW_ATTACHMENT_SIZE + 2],
                    att[RMW_ATTACHMENT_SIZE + 3],
                ]);
                let computed_crc = nros_rmw::crc32(&buf[..len]);
                Some(received_crc == computed_crc)
            } else {
                // No CRC in attachment (sender doesn't have safety-e2e)
                None
            };

            (seq, crc_result)
        } else {
            // No attachment at all — cannot validate
            (0, None)
        };

        buffer.has_data.store(false, Ordering::Release);

        let status = self.safety_validator.validate(message_seq, crc_valid);
        Ok(Some((len, status)))
    }

    /// Try to receive raw data along with message info from attachment
    ///
    /// Returns `Ok(Some((len, info)))` if data is available, where:
    /// - `len` is the number of bytes written to the buffer
    /// - `info` is the parsed message info (if attachment was present)
    ///
    /// Returns `Ok(None)` if no data is available.
    pub fn try_recv_with_info(
        &mut self,
        buf: &mut [u8],
    ) -> Result<Option<(usize, Option<MessageInfo>)>, TransportError> {
        // Safety: We own this buffer index and access is atomic
        let buffer = unsafe { &SUBSCRIBER_BUFFERS[self.buffer_index] };

        if !buffer.has_data.load(Ordering::Acquire) {
            return Ok(None);
        }

        // Check for overflow (message exceeded static buffer capacity)
        if buffer.overflow.load(Ordering::Acquire) {
            buffer.overflow.store(false, Ordering::Release);
            buffer.has_data.store(false, Ordering::Release);
            return Err(TransportError::MessageTooLarge);
        }

        let len = buffer.len.load(Ordering::Acquire);
        if len > buf.len() {
            // Clear has_data to avoid permanently stuck subscription — the oversized
            // message is dropped, but the subscription recovers on the next message.
            buffer.has_data.store(false, Ordering::Release);
            return Err(TransportError::BufferTooSmall);
        }

        // Copy payload data
        // Safety: Data is valid up to len bytes
        unsafe {
            core::ptr::copy_nonoverlapping(
                SUBSCRIBER_BUFFERS[self.buffer_index].data.as_ptr(),
                buf.as_mut_ptr(),
                len,
            );
        }

        // Parse attachment if present
        let attachment_len = buffer.attachment_len.load(Ordering::Acquire);
        let message_info = if attachment_len > 0 {
            // Safety: attachment is valid up to attachment_len bytes
            let attachment_slice =
                unsafe { &SUBSCRIBER_BUFFERS[self.buffer_index].attachment[..attachment_len] };
            MessageInfo::from_attachment(attachment_slice)
        } else {
            None
        };

        buffer.has_data.store(false, Ordering::Release);

        Ok(Some((len, message_info)))
    }
}

impl Subscriber for ShimSubscriber {
    type Error = TransportError;

    fn has_data(&self) -> bool {
        // Safety: We own this buffer index and access is atomic
        let buffer = unsafe { &SUBSCRIBER_BUFFERS[self.buffer_index] };
        buffer.has_data.load(Ordering::Acquire)
    }

    fn try_recv_raw(&mut self, buf: &mut [u8]) -> Result<Option<usize>, Self::Error> {
        // Safety: We own this buffer index and access is atomic
        let buffer = unsafe { &SUBSCRIBER_BUFFERS[self.buffer_index] };

        if !buffer.has_data.load(Ordering::Acquire) {
            return Ok(None);
        }

        // Check for overflow (message exceeded static buffer capacity)
        if buffer.overflow.load(Ordering::Acquire) {
            buffer.overflow.store(false, Ordering::Release);
            buffer.has_data.store(false, Ordering::Release);
            return Err(TransportError::MessageTooLarge);
        }

        let len = buffer.len.load(Ordering::Acquire);
        if len > buf.len() {
            // Clear has_data to avoid permanently stuck subscription — the oversized
            // message is dropped, but the subscription recovers on the next message.
            buffer.has_data.store(false, Ordering::Release);
            return Err(TransportError::BufferTooSmall);
        }

        // Copy data and clear flag
        // Safety: Data is valid up to len bytes
        unsafe {
            core::ptr::copy_nonoverlapping(
                SUBSCRIBER_BUFFERS[self.buffer_index].data.as_ptr(),
                buf.as_mut_ptr(),
                len,
            );
        }
        buffer.has_data.store(false, Ordering::Release);

        Ok(Some(len))
    }

    fn deserialization_error(&self) -> Self::Error {
        TransportError::DeserializationError
    }
}

// ============================================================================
// Service Server (using queryables)
// ============================================================================

/// Shared buffer for service server callbacks
struct ServiceBuffer {
    /// Buffer for received request data
    data: [u8; SERVICE_BUFFER_SIZE],
    /// Buffer for keyexpr (for reply)
    keyexpr: [u8; 256],
    /// Flag indicating new request is available
    has_request: AtomicBool,
    /// Flag indicating the incoming request exceeded the buffer capacity.
    /// Set by the callback when `payload_len > data.len()`. Checked by
    /// `try_recv_request` which returns `Err(MessageTooLarge)` and clears this flag.
    overflow: AtomicBool,
    /// Length of valid data
    len: AtomicUsize,
    /// Length of keyexpr
    keyexpr_len: AtomicUsize,
    /// Sequence number (counter)
    sequence_number: AtomicSeqCounter,
}

impl ServiceBuffer {
    const fn new() -> Self {
        Self {
            data: [0u8; SERVICE_BUFFER_SIZE],
            keyexpr: [0u8; 256],
            has_request: AtomicBool::new(false),
            overflow: AtomicBool::new(false),
            len: AtomicUsize::new(0),
            keyexpr_len: AtomicUsize::new(0),
            sequence_number: AtomicSeqCounter::new(0),
        }
    }
}

/// Static buffers for service servers.
///
/// Count matches `ZENOH_SHIM_MAX_QUERYABLES` from zpico-sys.
static mut SERVICE_BUFFERS: [ServiceBuffer; ZENOH_SHIM_MAX_QUERYABLES] =
    [const { ServiceBuffer::new() }; ZENOH_SHIM_MAX_QUERYABLES];

/// Next available service buffer index
static NEXT_SERVICE_BUFFER_INDEX: AtomicUsize = AtomicUsize::new(0);

/// Sequence counter for service requests
static SERVICE_SEQ_COUNTER: AtomicSeqCounter = AtomicSeqCounter::new(0);

/// Callback function invoked by the C shim when queries arrive
extern "C" fn queryable_callback(
    keyexpr: *const core::ffi::c_char,
    keyexpr_len: usize,
    payload: *const u8,
    payload_len: usize,
    ctx: *mut core::ffi::c_void,
) {
    let buffer_index = ctx as usize;
    if buffer_index >= 8 {
        return;
    }

    // Safety: We control access to SERVICE_BUFFERS and the callback is single-threaded
    unsafe {
        let buffer = &mut SERVICE_BUFFERS[buffer_index];

        // Copy keyexpr
        let keyexpr_copy_len = keyexpr_len.min(buffer.keyexpr.len() - 1);
        core::ptr::copy_nonoverlapping(
            keyexpr as *const u8,
            buffer.keyexpr.as_mut_ptr(),
            keyexpr_copy_len,
        );
        buffer.keyexpr[keyexpr_copy_len] = 0; // Null terminate
        buffer
            .keyexpr_len
            .store(keyexpr_copy_len, Ordering::Release);

        if payload_len > buffer.data.len() {
            // Request exceeds static buffer capacity — flag as overflow.
            // Store keyexpr + sequence_number for diagnostics, but skip payload.
            buffer.overflow.store(true, Ordering::Release);
            let seq = SERVICE_SEQ_COUNTER.fetch_add(1, Ordering::Relaxed);
            buffer.sequence_number.store(seq, Ordering::Release);
            buffer.has_request.store(true, Ordering::Release);
        } else {
            // Normal case: copy payload
            buffer.overflow.store(false, Ordering::Release);
            if !payload.is_null() && payload_len > 0 {
                core::ptr::copy_nonoverlapping(payload, buffer.data.as_mut_ptr(), payload_len);
            }
            buffer.len.store(payload_len, Ordering::Release);

            // Set sequence number
            let seq = SERVICE_SEQ_COUNTER.fetch_add(1, Ordering::Relaxed);
            buffer.sequence_number.store(seq, Ordering::Release);

            buffer.has_request.store(true, Ordering::Release);
        }

        // Wake the executor spin loop (if waiting)
        #[cfg(feature = "std")]
        signal_executor_wake();
    }
}

/// Shim service server using queryables
///
/// Receives service requests via queryable callbacks.
/// Note: The reply mechanism is limited due to the callback model.
pub struct ShimServiceServer {
    /// The queryable handle (kept alive to maintain registration)
    _queryable: crate::zpico::ShimQueryable,
    /// Index into the static buffer array
    buffer_index: usize,
    /// Keyexpr buffer for replying (copied from last request)
    reply_keyexpr: [u8; 256],
    /// Keyexpr length
    reply_keyexpr_len: usize,
    /// Reference to context for replying
    context: *const ShimContext,
    /// Phantom to indicate ownership
    _phantom: PhantomData<()>,
}

impl ShimServiceServer {
    /// Create a new service server for the given service
    pub fn new(context: &ShimContext, service: &ServiceInfo) -> Result<Self, TransportError> {
        // Allocate a buffer index
        let buffer_index = NEXT_SERVICE_BUFFER_INDEX.fetch_add(1, Ordering::SeqCst);
        if buffer_index >= 8 {
            NEXT_SERVICE_BUFFER_INDEX.fetch_sub(1, Ordering::SeqCst);
            return Err(TransportError::ServiceServerCreationFailed);
        }

        // Generate the service key
        let key: heapless::String<256> = service.to_key();

        // Create null-terminated keyexpr
        let mut keyexpr_buf = [0u8; 257];
        let bytes = key.as_bytes();
        if bytes.len() >= keyexpr_buf.len() {
            return Err(TransportError::InvalidConfig);
        }
        keyexpr_buf[..bytes.len()].copy_from_slice(bytes);
        keyexpr_buf[bytes.len()] = 0;

        // Create queryable with callback
        let queryable = unsafe {
            context.declare_queryable_raw(
                &keyexpr_buf,
                queryable_callback,
                buffer_index as *mut core::ffi::c_void,
            )
        }
        .map_err(TransportError::from)?;

        Ok(Self {
            _queryable: queryable,
            buffer_index,
            reply_keyexpr: [0u8; 256],
            reply_keyexpr_len: 0,
            context: context as *const ShimContext,
            _phantom: PhantomData,
        })
    }
}

impl ServiceServerTrait for ShimServiceServer {
    type Error = TransportError;

    fn has_request(&self) -> bool {
        // Safety: We own this buffer index and access is atomic
        let buffer = unsafe { &SERVICE_BUFFERS[self.buffer_index] };
        buffer.has_request.load(Ordering::Acquire)
    }

    fn try_recv_request<'a>(
        &mut self,
        buf: &'a mut [u8],
    ) -> Result<Option<ServiceRequest<'a>>, Self::Error> {
        // Safety: We own this buffer index and access is atomic
        let buffer = unsafe { &SERVICE_BUFFERS[self.buffer_index] };

        if !buffer.has_request.load(Ordering::Acquire) {
            return Ok(None);
        }

        // Check for overflow (request exceeded static buffer capacity)
        if buffer.overflow.load(Ordering::Acquire) {
            buffer.overflow.store(false, Ordering::Release);
            buffer.has_request.store(false, Ordering::Release);
            return Err(TransportError::MessageTooLarge);
        }

        let len = buffer.len.load(Ordering::Acquire);
        if len > buf.len() {
            // Clear has_request to avoid permanently stuck service — the oversized
            // request is dropped, but the service recovers on the next request.
            buffer.has_request.store(false, Ordering::Release);
            return Err(TransportError::BufferTooSmall);
        }

        // Copy data and keyexpr
        unsafe {
            core::ptr::copy_nonoverlapping(
                SERVICE_BUFFERS[self.buffer_index].data.as_ptr(),
                buf.as_mut_ptr(),
                len,
            );

            // Save keyexpr for potential reply
            let keyexpr_len = buffer.keyexpr_len.load(Ordering::Acquire);
            core::ptr::copy_nonoverlapping(
                SERVICE_BUFFERS[self.buffer_index].keyexpr.as_ptr(),
                self.reply_keyexpr.as_mut_ptr(),
                keyexpr_len,
            );
            self.reply_keyexpr[keyexpr_len] = 0;
            self.reply_keyexpr_len = keyexpr_len;
        }

        #[allow(clippy::useless_conversion)] // i32→i64 on embedded, no-op on std
        let seq: i64 = buffer.sequence_number.load(Ordering::Acquire).into();
        buffer.has_request.store(false, Ordering::Release);

        Ok(Some(ServiceRequest {
            data: &buf[..len],
            sequence_number: seq,
        }))
    }

    fn send_reply(&mut self, _sequence_number: i64, data: &[u8]) -> Result<(), Self::Error> {
        if self.reply_keyexpr_len == 0 {
            return Err(TransportError::ServiceReplyFailed);
        }

        // Get context reference
        let context = unsafe { &*self.context };

        // Send reply using the queryable handle and stored keyexpr
        context
            .query_reply(
                self._queryable.handle(),
                &self.reply_keyexpr[..=self.reply_keyexpr_len],
                data,
                None,
            )
            .map_err(|_| TransportError::ServiceReplyFailed)?;

        // Clear the stored keyexpr
        self.reply_keyexpr_len = 0;

        Ok(())
    }
}

// ============================================================================
// Service Client
// ============================================================================

/// Default timeout for service calls in milliseconds
const SERVICE_DEFAULT_TIMEOUT_MS: u32 = 5000;

/// Shim service client using z_get queries
///
/// Service clients send requests via z_get and receive responses from queryables.
pub struct ShimServiceClient {
    /// Service key expression (null-terminated)
    keyexpr: [u8; 257],
    /// Length of valid keyexpr
    keyexpr_len: usize,
    /// Reference to context for making queries
    context: *const ShimContext,
    /// Timeout in milliseconds
    timeout_ms: u32,
    /// Phantom to indicate ownership
    _phantom: PhantomData<()>,
}

impl ShimServiceClient {
    /// Create a new service client for the given service
    pub fn new(context: &ShimContext, service: &ServiceInfo) -> Result<Self, TransportError> {
        // Generate wildcard service key for queries (matches any type hash from ROS 2)
        let key: heapless::String<256> = service.to_key_wildcard();

        // Create null-terminated keyexpr
        let mut keyexpr_buf = [0u8; 257];
        let bytes = key.as_bytes();
        if bytes.len() >= keyexpr_buf.len() {
            return Err(TransportError::InvalidConfig);
        }
        keyexpr_buf[..bytes.len()].copy_from_slice(bytes);
        keyexpr_buf[bytes.len()] = 0;

        #[cfg(feature = "std")]
        log::debug!("Service client keyexpr: {}", key.as_str());

        Ok(Self {
            keyexpr: keyexpr_buf,
            keyexpr_len: bytes.len(),
            context: context as *const ShimContext,
            timeout_ms: SERVICE_DEFAULT_TIMEOUT_MS,
            _phantom: PhantomData,
        })
    }

    /// Set the timeout for service calls
    pub fn set_timeout(&mut self, timeout_ms: u32) {
        self.timeout_ms = timeout_ms;
    }
}

impl ServiceClientTrait for ShimServiceClient {
    type Error = TransportError;

    fn call_raw(&mut self, request: &[u8], reply_buf: &mut [u8]) -> Result<usize, Self::Error> {
        // Get context reference
        let context = unsafe { &*self.context };

        // Call z_get and wait for reply
        let result = context
            .get(
                &self.keyexpr[..=self.keyexpr_len],
                request,
                reply_buf,
                self.timeout_ms,
            )
            .map_err(TransportError::from)?;

        Ok(result)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Error Conversion Tests
    // ========================================================================

    #[test]
    fn test_error_conversion() {
        assert_eq!(
            TransportError::from(ShimError::Config),
            TransportError::InvalidConfig
        );
        assert_eq!(
            TransportError::from(ShimError::Publish),
            TransportError::PublishFailed
        );
    }

    // ========================================================================
    // C.6 RMW Zenoh Protocol Verification Tests
    // ========================================================================

    // ------------------------------------------------------------------------
    // RMW Attachment Format Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_rmw_attachment_serialization() {
        let mut att = RmwAttachment::new();
        att.sequence_number = 42;
        att.timestamp = 1000000;

        let mut buf = [0u8; RMW_ATTACHMENT_SIZE];
        att.serialize(&mut buf);

        // Check sequence number (little-endian)
        assert_eq!(&buf[0..8], &42i64.to_le_bytes());
        // Check timestamp (little-endian)
        assert_eq!(&buf[8..16], &1000000i64.to_le_bytes());
        // Check VLE length
        assert_eq!(buf[16], 16);
        // Check GID (should match)
        assert_eq!(&buf[17..33], &att.rmw_gid);
    }

    #[test]
    fn test_rmw_attachment_deserialization() {
        // Create known attachment bytes
        let mut buf = [0u8; RMW_ATTACHMENT_SIZE];
        // Sequence number: 123
        buf[0..8].copy_from_slice(&123i64.to_le_bytes());
        // Timestamp: 456789
        buf[8..16].copy_from_slice(&456789i64.to_le_bytes());
        // VLE length: 16
        buf[16] = 16;
        // GID: 0x01, 0x02, ..., 0x10
        for i in 0..16 {
            buf[17 + i] = (i + 1) as u8;
        }

        let parsed = RmwAttachment::deserialize(&buf);
        assert!(parsed.is_some());
        let att = parsed.unwrap();

        assert_eq!(att.sequence_number, 123);
        assert_eq!(att.timestamp, 456789);
        for i in 0..16 {
            assert_eq!(att.rmw_gid[i], (i + 1) as u8);
        }
    }

    #[test]
    fn test_rmw_attachment_roundtrip() {
        let original = RmwAttachment {
            sequence_number: 999,
            timestamp: 1234567890,
            rmw_gid: [
                0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77,
                0x88, 0x99,
            ],
        };

        let mut buf = [0u8; RMW_ATTACHMENT_SIZE];
        original.serialize(&mut buf);

        let parsed = RmwAttachment::deserialize(&buf).expect("Failed to deserialize");
        assert_eq!(parsed.sequence_number, original.sequence_number);
        assert_eq!(parsed.timestamp, original.timestamp);
        assert_eq!(parsed.rmw_gid, original.rmw_gid);
    }

    #[test]
    fn test_rmw_attachment_deserialize_too_short() {
        let buf = [0u8; 10]; // Too short
        assert!(RmwAttachment::deserialize(&buf).is_none());
    }

    #[test]
    fn test_rmw_attachment_deserialize_wrong_gid_length() {
        let mut buf = [0u8; RMW_ATTACHMENT_SIZE];
        buf[16] = 8; // Wrong GID length (should be 16)
        assert!(RmwAttachment::deserialize(&buf).is_none());
    }

    #[test]
    fn test_message_info_from_attachment() {
        let mut buf = [0u8; RMW_ATTACHMENT_SIZE];
        buf[0..8].copy_from_slice(&42i64.to_le_bytes());
        buf[8..16].copy_from_slice(&1000000i64.to_le_bytes());
        buf[16] = 16;

        let info = MessageInfo::from_attachment(&buf);
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.sequence_number, 42);
        assert_eq!(info.timestamp_ns, 1000000);
    }

    // ------------------------------------------------------------------------
    // Liveliness Token Format Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_ros2_liveliness_mangle() {
        let mangled = Ros2Liveliness::mangle_topic_name::<64>("/chatter");
        assert_eq!(mangled.as_str(), "%chatter");

        let mangled2 = Ros2Liveliness::mangle_topic_name::<64>("/foo/bar/baz");
        assert_eq!(mangled2.as_str(), "%foo%bar%baz");
    }

    #[test]
    fn test_ros2_liveliness_node_keyexpr() {
        let zid = ShimZenohId::from_bytes([
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
            0x0f, 0x10,
        ]);
        // Root namespace: "/" mangles to "%"
        let keyexpr = Ros2Liveliness::node_keyexpr::<256>(0, &zid, "/", "my_node");

        // Format: @ros2_lv/<domain_id>/<zid>/0/0/NN/%/<mangled_ns>/<node_name>
        // ZID is in LSB-first order
        assert!(keyexpr.as_str().starts_with("@ros2_lv/0/"));
        assert!(keyexpr.as_str().contains("/0/0/NN/%/%/"));
        assert!(keyexpr.as_str().ends_with("/my_node"));
    }

    #[test]
    fn test_ros2_liveliness_node_keyexpr_with_namespace() {
        let zid = ShimZenohId::from_bytes([0u8; 16]);

        // Non-root namespace: "/demo" mangles to "%demo"
        let keyexpr = Ros2Liveliness::node_keyexpr::<256>(0, &zid, "/demo", "talker");
        assert!(keyexpr.as_str().contains("/0/0/NN/%/%demo/talker"));

        // Nested namespace: "/ns/sub" mangles to "%ns%sub"
        let keyexpr2 = Ros2Liveliness::node_keyexpr::<256>(0, &zid, "/ns/sub", "my_node");
        assert!(keyexpr2.as_str().contains("/0/0/NN/%/%ns%sub/my_node"));
    }

    #[test]
    fn test_ros2_liveliness_publisher_keyexpr() {
        let zid = ShimZenohId::from_bytes([0u8; 16]);
        let topic = TopicInfo {
            name: "/chatter",
            type_name: "std_msgs::msg::dds_::String_",
            type_hash: "RIHS01_abc123",
            domain_id: 0,
        };
        let qos = QosSettings::QOS_PROFILE_SENSOR_DATA;
        let keyexpr =
            Ros2Liveliness::publisher_keyexpr::<256>(0, &zid, "/", "my_node", &topic, &qos);

        // Format: @ros2_lv/<domain_id>/<zid>/0/11/MP/%/<mangled_ns>/<node_name>/<mangled_topic>/<type>/<hash>/<qos>
        assert!(keyexpr.as_str().starts_with("@ros2_lv/0/"));
        assert!(keyexpr.as_str().contains("/0/11/MP/%/%/"));
        assert!(keyexpr.as_str().contains("/my_node/"));
        assert!(keyexpr.as_str().contains("%chatter/"));
        assert!(keyexpr.as_str().contains("std_msgs::msg::dds_::String_"));
        assert!(keyexpr.as_str().contains("RIHS01_abc123"));
    }

    #[test]
    fn test_ros2_liveliness_publisher_keyexpr_with_namespace() {
        let zid = ShimZenohId::from_bytes([0u8; 16]);
        let topic = TopicInfo {
            name: "/chatter",
            type_name: "std_msgs::msg::dds_::String_",
            type_hash: "RIHS01_abc123",
            domain_id: 0,
        };
        let qos = QosSettings::QOS_PROFILE_SENSOR_DATA;
        let keyexpr =
            Ros2Liveliness::publisher_keyexpr::<256>(0, &zid, "/demo", "talker", &topic, &qos);
        assert!(keyexpr.as_str().contains("/0/11/MP/%/%demo/talker/"));
    }

    #[test]
    fn test_ros2_liveliness_subscriber_keyexpr() {
        let zid = ShimZenohId::from_bytes([0u8; 16]);
        let topic = TopicInfo {
            name: "/chatter",
            type_name: "std_msgs::msg::dds_::Int32_",
            type_hash: "RIHS01_def456",
            domain_id: 0,
        };
        let qos = QosSettings::QOS_PROFILE_SENSOR_DATA;
        let keyexpr =
            Ros2Liveliness::subscriber_keyexpr::<256>(0, &zid, "/", "my_node", &topic, &qos);

        // Format: @ros2_lv/<domain_id>/<zid>/0/11/MS/%/<mangled_ns>/<node_name>/<mangled_topic>/<type>/<hash>/<qos>
        assert!(keyexpr.as_str().starts_with("@ros2_lv/0/"));
        assert!(keyexpr.as_str().contains("/0/11/MS/%/%/"));
        assert!(keyexpr.as_str().contains("/my_node/"));
        assert!(keyexpr.as_str().contains("%chatter/"));
    }

    #[test]
    fn test_ros2_liveliness_service_server_keyexpr() {
        let zid = ShimZenohId::from_bytes([0u8; 16]);
        let service = ServiceInfo {
            name: "/add_two_ints",
            type_name: "example_interfaces::srv::dds_::AddTwoInts",
            type_hash: "RIHS01_abc123",
            domain_id: 0,
        };
        let qos = QosSettings::QOS_PROFILE_SERVICES_DEFAULT;
        let keyexpr =
            Ros2Liveliness::service_server_keyexpr::<256>(0, &zid, "/", "my_node", &service, &qos);

        // Format: @ros2_lv/<domain_id>/<zid>/0/11/SS/%/<mangled_ns>/<node_name>/<mangled_service>/<type>/<hash>/<qos>
        assert!(keyexpr.as_str().starts_with("@ros2_lv/0/"));
        assert!(keyexpr.as_str().contains("/0/11/SS/%/%/"));
        assert!(keyexpr.as_str().contains("/my_node/"));
        assert!(keyexpr.as_str().contains("%add_two_ints/"));
    }

    #[test]
    fn test_ros2_liveliness_service_server_keyexpr_with_namespace() {
        let zid = ShimZenohId::from_bytes([0u8; 16]);
        let service = ServiceInfo {
            name: "/add_two_ints",
            type_name: "example_interfaces::srv::dds_::AddTwoInts",
            type_hash: "RIHS01_abc123",
            domain_id: 0,
        };
        let qos = QosSettings::QOS_PROFILE_SERVICES_DEFAULT;
        let keyexpr = Ros2Liveliness::service_server_keyexpr::<256>(
            0, &zid, "/demo", "my_node", &service, &qos,
        );
        assert!(keyexpr.as_str().contains("/0/11/SS/%/%demo/my_node/"));
    }

    #[test]
    fn test_ros2_liveliness_service_client_keyexpr() {
        let zid = ShimZenohId::from_bytes([0u8; 16]);
        let service = ServiceInfo {
            name: "/add_two_ints",
            type_name: "example_interfaces::srv::dds_::AddTwoInts",
            type_hash: "RIHS01_abc123",
            domain_id: 0,
        };
        let qos = QosSettings::QOS_PROFILE_SERVICES_DEFAULT;
        let keyexpr =
            Ros2Liveliness::service_client_keyexpr::<256>(0, &zid, "/", "my_node", &service, &qos);

        // Format: @ros2_lv/<domain_id>/<zid>/0/11/SC/%/<mangled_ns>/<node_name>/<mangled_service>/<type>/<hash>/<qos>
        assert!(keyexpr.as_str().starts_with("@ros2_lv/0/"));
        assert!(keyexpr.as_str().contains("/0/11/SC/%/%/"));
        assert!(keyexpr.as_str().contains("/my_node/"));
        assert!(keyexpr.as_str().contains("%add_two_ints/"));
    }

    // ------------------------------------------------------------------------
    // Data KeyExpr Format Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_topic_info_to_key_humble() {
        let topic = TopicInfo {
            name: "/chatter",
            type_name: "std_msgs::msg::dds_::Int32_",
            type_hash: "TypeHashNotSupported",
            domain_id: 0,
        };

        let key: heapless::String<128> = topic.to_key();
        // Format: <domain_id>/<topic_name>/<type_name>/<type_hash>
        assert_eq!(
            key.as_str(),
            "0/chatter/std_msgs::msg::dds_::Int32_/TypeHashNotSupported"
        );
    }

    #[test]
    fn test_topic_info_to_key_wildcard() {
        let topic = TopicInfo {
            name: "/chatter",
            type_name: "std_msgs::msg::dds_::Int32_",
            type_hash: "TypeHashNotSupported",
            domain_id: 0,
        };

        let key: heapless::String<128> = topic.to_key_wildcard();
        // Format: <domain_id>/<topic_name>/<type_name>/*
        assert_eq!(key.as_str(), "0/chatter/std_msgs::msg::dds_::Int32_/*");
    }

    // ------------------------------------------------------------------------
    // Service KeyExpr Format Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_service_info_format() {
        let service = ServiceInfo {
            name: "/add_two_ints",
            type_name: "example_interfaces::srv::dds_::AddTwoInts",
            type_hash: "TypeHashNotSupported",
            domain_id: 0,
        };

        // Verify service info fields are correct
        assert_eq!(service.name, "/add_two_ints");
        assert_eq!(
            service.type_name,
            "example_interfaces::srv::dds_::AddTwoInts"
        );
        assert_eq!(service.domain_id, 0);
    }

    // ------------------------------------------------------------------------
    // QoS String Encoding Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_qos_best_effort_volatile() {
        // BEST_EFFORT reliability (2), VOLATILE durability (2), KEEP_LAST history with depth 1
        let qos = "2:2:1,1:,:,:,,";
        assert!(qos.starts_with("2:")); // BEST_EFFORT
        assert!(qos.contains(":2:")); // VOLATILE
    }

    #[test]
    fn test_qos_reliable_transient_local() {
        // RELIABLE reliability (1), TRANSIENT_LOCAL durability (1)
        let qos = "1:1:1,1:,:,:,,";
        assert!(qos.starts_with("1:")); // RELIABLE
    }

    // ------------------------------------------------------------------------
    // ZenohId Format Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_zenoh_id_to_hex_lsb_first() {
        // Test that ZenohId is formatted in LSB-first order
        let zid = ShimZenohId::from_bytes([
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
            0x0f, 0x10,
        ]);
        let mut buf = [0u8; 32];
        zid.to_hex_bytes(&mut buf);
        let hex = core::str::from_utf8(&buf).unwrap();

        // LSB-first: byte 15 first, byte 0 last
        assert_eq!(hex, "100f0e0d0c0b0a090807060504030201");
    }
}

// =============================================================================
// Ghost model validation
// =============================================================================

#[cfg(test)]
mod ghost_checks {
    use super::*;
    use nros_ghost_types::{ServiceBufferGhost, SubscriberBufferGhost};

    /// Structural check: construct SubscriberBufferGhost from SubscriberBuffer private fields.
    /// If a field is renamed or retyped, this fails to compile.
    fn ghost_from_buffer(b: &SubscriberBuffer) -> SubscriberBufferGhost {
        SubscriberBufferGhost {
            has_data: b.has_data.load(Ordering::Relaxed),
            overflow: b.overflow.load(Ordering::Relaxed),
            stored_len: b.len.load(Ordering::Relaxed),
            buf_capacity: b.data.len(),
        }
    }

    #[test]
    fn ghost_new_state() {
        let buffer = SubscriberBuffer::new();
        let ghost = ghost_from_buffer(&buffer);
        assert!(!ghost.has_data);
        assert!(!ghost.overflow);
        assert_eq!(ghost.stored_len, 0);
        assert_eq!(ghost.buf_capacity, SUBSCRIBER_BUFFER_SIZE);
    }

    #[test]
    fn ghost_capacity_constant() {
        let buffer = SubscriberBuffer::new();
        let ghost = ghost_from_buffer(&buffer);
        assert_eq!(ghost.buf_capacity, SUBSCRIBER_BUFFER_SIZE);
    }

    // ========================================================================
    // ServiceBufferGhost Correspondence
    // ========================================================================

    /// Structural check: construct ServiceBufferGhost from ServiceBuffer private fields.
    /// If a field is renamed or retyped, this fails to compile.
    fn ghost_from_service_buffer(b: &ServiceBuffer) -> ServiceBufferGhost {
        ServiceBufferGhost {
            has_request: b.has_request.load(Ordering::Relaxed),
            overflow: b.overflow.load(Ordering::Relaxed),
            stored_len: b.len.load(Ordering::Relaxed),
            buf_capacity: b.data.len(),
        }
    }

    #[test]
    fn ghost_service_new_state() {
        let buffer = ServiceBuffer::new();
        let ghost = ghost_from_service_buffer(&buffer);
        assert!(!ghost.has_request);
        assert!(!ghost.overflow);
        assert_eq!(ghost.stored_len, 0);
        assert_eq!(ghost.buf_capacity, SERVICE_BUFFER_SIZE);
    }

    #[test]
    fn ghost_service_capacity_constant() {
        let buffer = ServiceBuffer::new();
        let ghost = ghost_from_service_buffer(&buffer);
        assert_eq!(ghost.buf_capacity, SERVICE_BUFFER_SIZE);
    }

    #[test]
    fn svc_buf_overflow_signals_error() {
        let buffer = ServiceBuffer::new();
        // Simulate the callback detecting an oversized request
        buffer.overflow.store(true, Ordering::Release);
        buffer.has_request.store(true, Ordering::Release);

        let ghost = ghost_from_service_buffer(&buffer);
        assert!(ghost.has_request);
        assert!(ghost.overflow);
    }

    // ========================================================================
    // E2E Safety Protocol Unit Tests
    // ========================================================================

    /// Helper: build a valid 37-byte safety attachment from seq, timestamp, and payload CRC.
    #[cfg(feature = "safety-e2e")]
    fn build_safety_attachment(
        seq: i64,
        ts: i64,
        payload: &[u8],
    ) -> [u8; RMW_ATTACHMENT_SIZE_WITH_CRC] {
        let crc = nros_rmw::crc32(payload);
        let mut att = [0u8; RMW_ATTACHMENT_SIZE_WITH_CRC];
        att[0..8].copy_from_slice(&seq.to_le_bytes());
        att[8..16].copy_from_slice(&ts.to_le_bytes());
        att[16] = RMW_GID_SIZE as u8; // VLE GID length
        // GID bytes 17..33 left as zero
        att[RMW_ATTACHMENT_SIZE..RMW_ATTACHMENT_SIZE_WITH_CRC].copy_from_slice(&crc.to_le_bytes());
        att
    }

    /// Helper: parse attachment bytes and validate CRC against payload.
    ///
    /// This mirrors the logic in `try_recv_validated()` but is testable
    /// without creating a full `ShimSubscriber` (which requires a zenoh session).
    #[cfg(feature = "safety-e2e")]
    fn validate_from_buffers(
        payload: &[u8],
        attachment: &[u8],
        validator: &mut nros_rmw::SafetyValidator,
    ) -> nros_rmw::IntegrityStatus {
        let (message_seq, crc_valid) = if attachment.len() >= RMW_ATTACHMENT_SIZE {
            let seq = i64::from_le_bytes([
                attachment[0],
                attachment[1],
                attachment[2],
                attachment[3],
                attachment[4],
                attachment[5],
                attachment[6],
                attachment[7],
            ]);

            let crc_result = if attachment.len() >= RMW_ATTACHMENT_SIZE + SAFETY_CRC_SIZE {
                let received_crc = u32::from_le_bytes([
                    attachment[RMW_ATTACHMENT_SIZE],
                    attachment[RMW_ATTACHMENT_SIZE + 1],
                    attachment[RMW_ATTACHMENT_SIZE + 2],
                    attachment[RMW_ATTACHMENT_SIZE + 3],
                ]);
                let computed_crc = nros_rmw::crc32(payload);
                Some(received_crc == computed_crc)
            } else {
                None
            };

            (seq, crc_result)
        } else {
            (0, None)
        };

        validator.validate(message_seq, crc_valid)
    }

    #[cfg(feature = "safety-e2e")]
    #[test]
    fn test_safety_validate_happy_path() {
        let payload = b"\x00\x01\x00\x00\x2a\x00\x00\x00"; // CDR-encoded Int32(42)
        let attachment = build_safety_attachment(0, 1000, payload);

        let mut validator = nros_rmw::SafetyValidator::new();
        let status = validate_from_buffers(payload, &attachment, &mut validator);

        assert!(status.is_valid());
        assert_eq!(status.crc_valid, Some(true));
        assert_eq!(status.gap, 0);
        assert!(!status.duplicate);
    }

    #[cfg(feature = "safety-e2e")]
    #[test]
    fn test_safety_validate_sequential_messages() {
        let mut validator = nros_rmw::SafetyValidator::new();

        for seq in 0..10i64 {
            let payload = seq.to_le_bytes();
            let attachment = build_safety_attachment(seq, seq * 1000, &payload);
            let status = validate_from_buffers(&payload, &attachment, &mut validator);

            assert!(status.is_valid(), "failed at seq {}: {:?}", seq, status);
            assert_eq!(status.gap, 0);
            assert!(!status.duplicate);
            assert_eq!(status.crc_valid, Some(true));
        }
    }

    #[cfg(feature = "safety-e2e")]
    #[test]
    fn test_safety_validate_tampered_crc() {
        let payload = b"hello world CDR data";
        let mut attachment = build_safety_attachment(0, 1000, payload);

        // Tamper with the CRC (flip a bit)
        attachment[RMW_ATTACHMENT_SIZE] ^= 0x01;

        let mut validator = nros_rmw::SafetyValidator::new();
        let status = validate_from_buffers(payload, &attachment, &mut validator);

        assert!(!status.is_valid());
        assert_eq!(status.crc_valid, Some(false));
    }

    #[cfg(feature = "safety-e2e")]
    #[test]
    fn test_safety_validate_tampered_payload() {
        let payload = b"original payload data";
        let attachment = build_safety_attachment(0, 1000, payload);

        // Tamper with the payload (simulating transport corruption)
        let mut tampered_payload = *payload;
        tampered_payload[0] ^= 0xFF;

        let mut validator = nros_rmw::SafetyValidator::new();
        let status = validate_from_buffers(&tampered_payload, &attachment, &mut validator);

        assert!(!status.is_valid());
        assert_eq!(status.crc_valid, Some(false));
    }

    #[cfg(feature = "safety-e2e")]
    #[test]
    fn test_safety_validate_sequence_gap() {
        let mut validator = nros_rmw::SafetyValidator::new();

        // First message: seq 0
        let payload0 = b"msg0";
        let att0 = build_safety_attachment(0, 0, payload0);
        let status = validate_from_buffers(payload0, &att0, &mut validator);
        assert!(status.is_valid());

        // seq 1
        let payload1 = b"msg1";
        let att1 = build_safety_attachment(1, 1000, payload1);
        let status = validate_from_buffers(payload1, &att1, &mut validator);
        assert!(status.is_valid());

        // Skip to seq 5 (gap of 3)
        let payload5 = b"msg5";
        let att5 = build_safety_attachment(5, 5000, payload5);
        let status = validate_from_buffers(payload5, &att5, &mut validator);

        assert!(!status.is_valid());
        assert_eq!(status.gap, 3);
        assert!(!status.duplicate);
        assert_eq!(status.crc_valid, Some(true));
    }

    #[cfg(feature = "safety-e2e")]
    #[test]
    fn test_safety_validate_duplicate() {
        let mut validator = nros_rmw::SafetyValidator::new();

        // seq 0, 1, 2
        for seq in 0..3i64 {
            let payload = seq.to_le_bytes();
            let att = build_safety_attachment(seq, seq * 1000, &payload);
            validate_from_buffers(&payload, &att, &mut validator);
        }

        // Receive seq 1 again (duplicate)
        let payload1 = 1i64.to_le_bytes();
        let att1 = build_safety_attachment(1, 1000, &payload1);
        let status = validate_from_buffers(&payload1, &att1, &mut validator);

        assert!(!status.is_valid());
        assert!(status.duplicate);
        assert_eq!(status.crc_valid, Some(true));
    }

    #[cfg(feature = "safety-e2e")]
    #[test]
    fn test_safety_validate_no_crc_interop() {
        // Simulate a 33-byte attachment (sender without safety-e2e)
        let payload = b"some data";
        let mut attachment = [0u8; RMW_ATTACHMENT_SIZE]; // Only 33 bytes, no CRC
        attachment[0..8].copy_from_slice(&0i64.to_le_bytes());
        attachment[8..16].copy_from_slice(&1000i64.to_le_bytes());
        attachment[16] = RMW_GID_SIZE as u8;

        let mut validator = nros_rmw::SafetyValidator::new();
        let status = validate_from_buffers(payload, &attachment, &mut validator);

        assert!(status.is_valid()); // No CRC is acceptable (interop)
        assert_eq!(status.crc_valid, None);
    }

    #[cfg(feature = "safety-e2e")]
    #[test]
    fn test_safety_attachment_format() {
        let payload = b"test payload for CRC";
        let expected_crc = nros_rmw::crc32(payload);

        let attachment = build_safety_attachment(42, 999999, payload);

        // Verify format: 37 bytes total
        assert_eq!(attachment.len(), 37);

        // Bytes 0..8: sequence number (LE)
        assert_eq!(i64::from_le_bytes(attachment[0..8].try_into().unwrap()), 42);

        // Bytes 8..16: timestamp (LE)
        assert_eq!(
            i64::from_le_bytes(attachment[8..16].try_into().unwrap()),
            999999
        );

        // Byte 16: GID VLE length
        assert_eq!(attachment[16], 16);

        // Bytes 33..37: CRC-32 of payload (LE)
        let crc = u32::from_le_bytes(attachment[33..37].try_into().unwrap());
        assert_eq!(crc, expected_crc);
    }

    // ========================================================================
    // Buffer state machine test helpers (37.1 / 37.1a)
    // ========================================================================

    // --- Subscription buffer helpers ---

    /// Simulate a subscription callback by writing directly to SUBSCRIBER_BUFFERS[slot].
    /// Mirrors the logic in `subscriber_callback_with_attachment`.
    fn simulate_subscription_callback(slot: usize, payload: &[u8]) {
        unsafe {
            let buffer = &mut SUBSCRIBER_BUFFERS[slot];
            if payload.len() > buffer.data.len() {
                buffer.overflow.store(true, Ordering::Release);
                buffer.has_data.store(true, Ordering::Release);
            } else {
                buffer.overflow.store(false, Ordering::Release);
                buffer.data[..payload.len()].copy_from_slice(payload);
                buffer.len.store(payload.len(), Ordering::Release);
                buffer.attachment_len.store(0, Ordering::Release);
                buffer.has_data.store(true, Ordering::Release);
            }
        }
    }

    /// Reset a subscriber buffer to idle state.
    fn reset_subscriber_buffer(slot: usize) {
        unsafe {
            let buffer = &mut SUBSCRIBER_BUFFERS[slot];
            buffer.has_data.store(false, Ordering::Release);
            buffer.overflow.store(false, Ordering::Release);
            buffer.len.store(0, Ordering::Release);
            buffer.attachment_len.store(0, Ordering::Release);
        }
    }

    /// Try to receive from a subscriber buffer slot.
    /// Replicates `try_recv_raw` logic for testing without a zenoh session.
    fn try_recv_subscription(
        slot: usize,
        recv_buf: &mut [u8],
    ) -> Result<Option<usize>, TransportError> {
        let buffer = unsafe { &SUBSCRIBER_BUFFERS[slot] };

        if !buffer.has_data.load(Ordering::Acquire) {
            return Ok(None);
        }

        if buffer.overflow.load(Ordering::Acquire) {
            buffer.overflow.store(false, Ordering::Release);
            buffer.has_data.store(false, Ordering::Release);
            return Err(TransportError::MessageTooLarge);
        }

        let len = buffer.len.load(Ordering::Acquire);
        if len > recv_buf.len() {
            buffer.has_data.store(false, Ordering::Release);
            return Err(TransportError::BufferTooSmall);
        }

        unsafe {
            core::ptr::copy_nonoverlapping(
                SUBSCRIBER_BUFFERS[slot].data.as_ptr(),
                recv_buf.as_mut_ptr(),
                len,
            );
        }
        buffer.has_data.store(false, Ordering::Release);

        Ok(Some(len))
    }

    // --- Service buffer helpers ---

    /// Simulate a service request callback by writing directly to SERVICE_BUFFERS[slot].
    fn simulate_service_request(slot: usize, payload: &[u8], keyexpr: &[u8]) {
        unsafe {
            let buffer = &mut SERVICE_BUFFERS[slot];
            let copy_len = payload.len().min(buffer.data.len());
            buffer.data[..copy_len].copy_from_slice(&payload[..copy_len]);
            buffer.len.store(copy_len, Ordering::Release);

            let klen = keyexpr.len().min(buffer.keyexpr.len() - 1);
            buffer.keyexpr[..klen].copy_from_slice(&keyexpr[..klen]);
            buffer.keyexpr[klen] = 0;
            buffer.keyexpr_len.store(klen, Ordering::Release);

            let seq = SERVICE_SEQ_COUNTER.fetch_add(1, Ordering::Relaxed);
            buffer.sequence_number.store(seq, Ordering::Release);

            buffer.has_request.store(true, Ordering::Release);
        }
    }

    /// Reset a service buffer to idle state.
    fn reset_service_buffer(slot: usize) {
        unsafe {
            let buffer = &mut SERVICE_BUFFERS[slot];
            buffer.has_request.store(false, Ordering::Release);
            buffer.len.store(0, Ordering::Release);
            buffer.keyexpr_len.store(0, Ordering::Release);
        }
    }

    /// Try to receive a service request from a buffer slot.
    /// Replicates `try_recv_request` logic for testing without a zenoh queryable.
    fn try_recv_service(slot: usize, recv_buf: &mut [u8]) -> Result<Option<usize>, TransportError> {
        let buffer = unsafe { &SERVICE_BUFFERS[slot] };

        if !buffer.has_request.load(Ordering::Acquire) {
            return Ok(None);
        }

        let len = buffer.len.load(Ordering::Acquire);
        if len > recv_buf.len() {
            buffer.has_request.store(false, Ordering::Release);
            return Err(TransportError::BufferTooSmall);
        }

        unsafe {
            core::ptr::copy_nonoverlapping(
                SERVICE_BUFFERS[slot].data.as_ptr(),
                recv_buf.as_mut_ptr(),
                len,
            );
        }

        buffer.has_request.store(false, Ordering::Release);
        Ok(Some(len))
    }

    /// Read the keyexpr from a service buffer slot (for keyexpr preservation tests).
    fn read_service_keyexpr(slot: usize) -> heapless::Vec<u8, 256> {
        let buffer = unsafe { &SERVICE_BUFFERS[slot] };
        let klen = buffer.keyexpr_len.load(Ordering::Acquire);
        let mut v = heapless::Vec::new();
        unsafe {
            for i in 0..klen {
                let _ = v.push(SERVICE_BUFFERS[slot].keyexpr[i]);
            }
        }
        v
    }

    /// Read the sequence number from a service buffer slot.
    fn read_service_seq(slot: usize) -> i64 {
        let buffer = unsafe { &SERVICE_BUFFERS[slot] };
        buffer.sequence_number.load(Ordering::Acquire).into()
    }

    // ========================================================================
    // 37.1: Service buffer bug fix tests
    // ========================================================================

    #[test]
    fn service_buf_oversized_request_clears_has_request() {
        let slot = 6;
        reset_service_buffer(slot);

        let payload = [0xABu8; 512];
        simulate_service_request(slot, &payload, b"test/service");

        let mut small_buf = [0u8; 256];
        let result = try_recv_service(slot, &mut small_buf);
        assert!(matches!(result, Err(TransportError::BufferTooSmall)));

        let buffer = unsafe { &SERVICE_BUFFERS[slot] };
        assert!(
            !buffer.has_request.load(Ordering::Acquire),
            "has_request must be cleared after BufferTooSmall to avoid stuck state"
        );

        simulate_service_request(slot, b"hello", b"test/service");
        let mut recv_buf = [0u8; 1024];
        let result = try_recv_service(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(5))));
        assert_eq!(&recv_buf[..5], b"hello");

        reset_service_buffer(slot);
    }

    #[test]
    fn service_buf_normal_request_after_stuck_recovery() {
        let slot = 5;
        reset_service_buffer(slot);

        simulate_service_request(slot, b"first", b"svc/a");
        let mut buf = [0u8; 1024];
        let result = try_recv_service(slot, &mut buf);
        assert!(matches!(result, Ok(Some(5))));
        assert_eq!(&buf[..5], b"first");

        let result = try_recv_service(slot, &mut buf);
        assert!(matches!(result, Ok(None)));

        simulate_service_request(slot, b"second", b"svc/a");
        let result = try_recv_service(slot, &mut buf);
        assert!(matches!(result, Ok(Some(6))));
        assert_eq!(&buf[..6], b"second");

        reset_service_buffer(slot);
    }

    // ========================================================================
    // 37.1a: Subscription buffer state machine tests
    // ========================================================================

    #[test]
    fn sub_buf_idle_poll() {
        let slot = 0;
        reset_subscriber_buffer(slot);

        let mut buf = [0u8; 1024];
        let result = try_recv_subscription(slot, &mut buf);
        assert!(matches!(result, Ok(None)));

        // State unchanged
        let buffer = unsafe { &SUBSCRIBER_BUFFERS[slot] };
        assert!(!buffer.has_data.load(Ordering::Acquire));
        assert!(!buffer.overflow.load(Ordering::Acquire));
    }

    #[test]
    fn sub_buf_normal_delivery() {
        let slot = 1;
        reset_subscriber_buffer(slot);

        let payload = [0x42u8; 100];
        simulate_subscription_callback(slot, &payload);

        let buffer = unsafe { &SUBSCRIBER_BUFFERS[slot] };
        assert!(buffer.has_data.load(Ordering::Acquire));
        assert!(!buffer.overflow.load(Ordering::Acquire));

        let mut recv_buf = [0u8; 1024];
        let result = try_recv_subscription(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(100))));
        assert_eq!(&recv_buf[..100], &payload);

        assert!(!buffer.has_data.load(Ordering::Acquire));
    }

    #[test]
    fn sub_buf_max_payload() {
        let slot = 2;
        reset_subscriber_buffer(slot);

        // Exactly 1024 bytes = max capacity
        let payload = [0xFFu8; 1024];
        simulate_subscription_callback(slot, &payload);

        let buffer = unsafe { &SUBSCRIBER_BUFFERS[slot] };
        assert!(buffer.has_data.load(Ordering::Acquire));
        assert!(!buffer.overflow.load(Ordering::Acquire));

        let mut recv_buf = [0u8; 1024];
        let result = try_recv_subscription(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(1024))));
        assert_eq!(&recv_buf, &payload);
    }

    #[test]
    fn sub_buf_overflow_recovery() {
        let slot = 3;
        reset_subscriber_buffer(slot);

        // 2000 bytes exceeds 1024 capacity → overflow
        let payload = [0xAAu8; 2000];
        simulate_subscription_callback(slot, &payload);

        let buffer = unsafe { &SUBSCRIBER_BUFFERS[slot] };
        assert!(buffer.has_data.load(Ordering::Acquire));
        assert!(buffer.overflow.load(Ordering::Acquire));

        let mut recv_buf = [0u8; 1024];
        let result = try_recv_subscription(slot, &mut recv_buf);
        assert!(matches!(result, Err(TransportError::MessageTooLarge)));

        // Both flags cleared
        assert!(!buffer.has_data.load(Ordering::Acquire));
        assert!(!buffer.overflow.load(Ordering::Acquire));

        // Recovery: next normal callback is accepted
        simulate_subscription_callback(slot, b"recovered");
        let result = try_recv_subscription(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(9))));
        assert_eq!(&recv_buf[..9], b"recovered");
    }

    #[test]
    fn sub_buf_caller_too_small() {
        let slot = 4;
        reset_subscriber_buffer(slot);

        // Store 512 bytes, try to receive into 256-byte buffer
        let payload = [0xBBu8; 512];
        simulate_subscription_callback(slot, &payload);

        let mut small_buf = [0u8; 256];
        let result = try_recv_subscription(slot, &mut small_buf);
        assert!(matches!(result, Err(TransportError::BufferTooSmall)));

        // has_data cleared (the oversized message is dropped)
        let buffer = unsafe { &SUBSCRIBER_BUFFERS[slot] };
        assert!(!buffer.has_data.load(Ordering::Acquire));

        // Recovery: next callback accepted
        simulate_subscription_callback(slot, b"small");
        let mut recv_buf = [0u8; 1024];
        let result = try_recv_subscription(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(5))));
        assert_eq!(&recv_buf[..5], b"small");
    }

    #[test]
    fn sub_buf_overwrite_unread() {
        let slot = 5;
        reset_subscriber_buffer(slot);

        // Two callbacks without intervening recv
        simulate_subscription_callback(slot, b"first_msg");
        simulate_subscription_callback(slot, b"second_msg");

        // Only second message delivered (last-message-wins)
        let mut recv_buf = [0u8; 1024];
        let result = try_recv_subscription(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(10))));
        assert_eq!(&recv_buf[..10], b"second_msg");
    }

    #[test]
    fn sub_buf_double_consume() {
        let slot = 6;
        reset_subscriber_buffer(slot);

        simulate_subscription_callback(slot, b"data");

        let mut recv_buf = [0u8; 1024];
        let result = try_recv_subscription(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(4))));

        // Second recv returns None
        let result = try_recv_subscription(slot, &mut recv_buf);
        assert!(matches!(result, Ok(None)));
    }

    #[test]
    fn sub_buf_overflow_then_normal() {
        let slot = 7;
        reset_subscriber_buffer(slot);

        // Oversized → overflow error → normal → delivered
        simulate_subscription_callback(slot, &[0u8; 2000]);
        let mut recv_buf = [0u8; 1024];
        let result = try_recv_subscription(slot, &mut recv_buf);
        assert!(matches!(result, Err(TransportError::MessageTooLarge)));

        simulate_subscription_callback(slot, b"after_overflow");
        let result = try_recv_subscription(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(14))));
        assert_eq!(&recv_buf[..14], b"after_overflow");
    }

    #[test]
    fn sub_buf_zero_length_payload() {
        let slot = 0;
        reset_subscriber_buffer(slot);

        simulate_subscription_callback(slot, b"");

        let buffer = unsafe { &SUBSCRIBER_BUFFERS[slot] };
        assert!(buffer.has_data.load(Ordering::Acquire));

        let mut recv_buf = [0u8; 1024];
        let result = try_recv_subscription(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(0))));
    }

    #[test]
    fn sub_buf_all_slots_independent() {
        let slot_a = 0;
        let slot_b = 7;
        reset_subscriber_buffer(slot_a);
        reset_subscriber_buffer(slot_b);

        simulate_subscription_callback(slot_a, b"slot_zero");
        simulate_subscription_callback(slot_b, b"slot_seven");

        // Consume slot_b first
        let mut recv_buf = [0u8; 1024];
        let result = try_recv_subscription(slot_b, &mut recv_buf);
        assert!(matches!(result, Ok(Some(10))));
        assert_eq!(&recv_buf[..10], b"slot_seven");

        // slot_a still has data
        let buffer_a = unsafe { &SUBSCRIBER_BUFFERS[slot_a] };
        assert!(buffer_a.has_data.load(Ordering::Acquire));

        let result = try_recv_subscription(slot_a, &mut recv_buf);
        assert!(matches!(result, Ok(Some(9))));
        assert_eq!(&recv_buf[..9], b"slot_zero");
    }

    // ========================================================================
    // 37.1a: Service buffer state machine tests
    // ========================================================================

    #[test]
    fn svc_buf_idle_poll() {
        let slot = 0;
        reset_service_buffer(slot);

        let mut buf = [0u8; 1024];
        let result = try_recv_service(slot, &mut buf);
        assert!(matches!(result, Ok(None)));

        let buffer = unsafe { &SERVICE_BUFFERS[slot] };
        assert!(!buffer.has_request.load(Ordering::Acquire));
    }

    #[test]
    fn svc_buf_normal_request() {
        let slot = 1;
        reset_service_buffer(slot);

        simulate_service_request(slot, b"request_data", b"svc/test");

        let buffer = unsafe { &SERVICE_BUFFERS[slot] };
        assert!(buffer.has_request.load(Ordering::Acquire));

        let mut recv_buf = [0u8; 1024];
        let result = try_recv_service(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(12))));
        assert_eq!(&recv_buf[..12], b"request_data");

        assert!(!buffer.has_request.load(Ordering::Acquire));
    }

    #[test]
    fn svc_buf_max_payload() {
        let slot = 2;
        reset_service_buffer(slot);

        // Exactly 1024 bytes = max capacity
        let payload = [0xCCu8; 1024];
        simulate_service_request(slot, &payload, b"svc/big");

        let mut recv_buf = [0u8; 1024];
        let result = try_recv_service(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(1024))));
        assert_eq!(&recv_buf, &payload);
    }

    #[test]
    fn svc_buf_caller_too_small_recovery() {
        let slot = 3;
        reset_service_buffer(slot);

        // Store 512 bytes, receive into 256-byte buffer
        let payload = [0xDDu8; 512];
        simulate_service_request(slot, &payload, b"svc/test");

        let mut small_buf = [0u8; 256];
        let result = try_recv_service(slot, &mut small_buf);
        assert!(matches!(result, Err(TransportError::BufferTooSmall)));

        // has_request cleared (post-fix behavior)
        let buffer = unsafe { &SERVICE_BUFFERS[slot] };
        assert!(!buffer.has_request.load(Ordering::Acquire));

        // Next request accepted
        simulate_service_request(slot, b"ok", b"svc/test");
        let mut recv_buf = [0u8; 1024];
        let result = try_recv_service(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(2))));
        assert_eq!(&recv_buf[..2], b"ok");
    }

    #[test]
    fn svc_buf_overwrite_unread() {
        let slot = 4;
        reset_service_buffer(slot);

        simulate_service_request(slot, b"first_req", b"svc/a");
        simulate_service_request(slot, b"second_req", b"svc/a");

        // Only second request delivered
        let mut recv_buf = [0u8; 1024];
        let result = try_recv_service(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(10))));
        assert_eq!(&recv_buf[..10], b"second_req");
    }

    #[test]
    fn svc_buf_double_consume() {
        let slot = 0;
        reset_service_buffer(slot);

        simulate_service_request(slot, b"once", b"svc/a");

        let mut recv_buf = [0u8; 1024];
        let result = try_recv_service(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(4))));

        let result = try_recv_service(slot, &mut recv_buf);
        assert!(matches!(result, Ok(None)));
    }

    #[test]
    fn svc_buf_sequence_numbers() {
        let slot = 7;
        reset_service_buffer(slot);

        // Three sequential requests — sequence numbers should increment
        simulate_service_request(slot, b"r1", b"svc/a");
        let seq1 = read_service_seq(slot);

        // Consume before next request
        let mut buf = [0u8; 1024];
        let _ = try_recv_service(slot, &mut buf);

        simulate_service_request(slot, b"r2", b"svc/a");
        let seq2 = read_service_seq(slot);
        let _ = try_recv_service(slot, &mut buf);

        simulate_service_request(slot, b"r3", b"svc/a");
        let seq3 = read_service_seq(slot);
        let _ = try_recv_service(slot, &mut buf);

        assert!(seq2 > seq1, "seq2 ({seq2}) should be > seq1 ({seq1})");
        assert!(seq3 > seq2, "seq3 ({seq3}) should be > seq2 ({seq2})");
    }

    #[test]
    fn svc_buf_keyexpr_preserved() {
        let slot = 1;
        reset_service_buffer(slot);

        let keyexpr = b"0/my_service/example_interfaces::srv::dds_::AddTwoInts/Reply";
        simulate_service_request(slot, b"payload", keyexpr);

        let stored = read_service_keyexpr(slot);
        assert_eq!(stored.as_slice(), keyexpr);

        // Consume and verify keyexpr was available during request
        let mut recv_buf = [0u8; 1024];
        let result = try_recv_service(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(7))));
    }

    #[test]
    fn svc_buf_all_slots_independent() {
        let slot_a = 0;
        let slot_b = 7;
        reset_service_buffer(slot_a);
        reset_service_buffer(slot_b);

        simulate_service_request(slot_a, b"req_zero", b"svc/0");
        simulate_service_request(slot_b, b"req_seven", b"svc/7");

        // Consume slot_b first
        let mut recv_buf = [0u8; 1024];
        let result = try_recv_service(slot_b, &mut recv_buf);
        assert!(matches!(result, Ok(Some(9))));
        assert_eq!(&recv_buf[..9], b"req_seven");

        // slot_a still has request
        let buffer_a = unsafe { &SERVICE_BUFFERS[slot_a] };
        assert!(buffer_a.has_request.load(Ordering::Acquire));

        let result = try_recv_service(slot_a, &mut recv_buf);
        assert!(matches!(result, Ok(Some(8))));
        assert_eq!(&recv_buf[..8], b"req_zero");
    }

    // ========================================================================
    // 37.1a: Cross-buffer interaction tests
    // ========================================================================

    #[test]
    fn sub_svc_independent() {
        // Use slot 2 — subscription array and service array are separate
        let slot = 2;
        reset_subscriber_buffer(slot);
        reset_service_buffer(slot);

        simulate_subscription_callback(slot, b"sub_data");
        simulate_service_request(slot, b"svc_data", b"svc/x");

        let mut recv_buf = [0u8; 1024];
        let sub_result = try_recv_subscription(slot, &mut recv_buf);
        assert!(matches!(sub_result, Ok(Some(8))));
        assert_eq!(&recv_buf[..8], b"sub_data");

        let svc_result = try_recv_service(slot, &mut recv_buf);
        assert!(matches!(svc_result, Ok(Some(8))));
        assert_eq!(&recv_buf[..8], b"svc_data");
    }

    #[test]
    fn sub_overflow_does_not_affect_svc() {
        let slot = 3;
        reset_subscriber_buffer(slot);
        reset_service_buffer(slot);

        // Subscription overflow
        simulate_subscription_callback(slot, &[0u8; 2000]);
        // Service normal request
        simulate_service_request(slot, b"svc_ok", b"svc/x");

        // Subscription has overflow error
        let mut recv_buf = [0u8; 1024];
        let sub_result = try_recv_subscription(slot, &mut recv_buf);
        assert!(matches!(sub_result, Err(TransportError::MessageTooLarge)));

        // Service buffer unaffected
        let svc_result = try_recv_service(slot, &mut recv_buf);
        assert!(matches!(svc_result, Ok(Some(6))));
        assert_eq!(&recv_buf[..6], b"svc_ok");
    }
}

//! XRCE-DDS node API for nros.
//!
//! Provides a typed, safe wrapper around the raw XRCE-DDS RMW backend.
//! This module follows the same pattern as the zenoh shim node API,
//! giving XRCE examples a clean API without exposing raw RMW internals.
//!
//! # Example
//!
//! ```ignore
//! use nros::xrce::*;
//! use std_msgs::msg::Int32;
//!
//! init_posix_udp("127.0.0.1:2019");
//! let mut executor = XrceExecutor::new("xrce_talker", 0)?;
//! let mut node = executor.create_node();
//! let publisher = node.create_publisher::<Int32>("/chatter")?;
//! publisher.publish(&Int32 { data: 42 }, &mut executor)?;
//! executor.spin_once(100);
//! ```

use core::marker::PhantomData;

use nros_core::{CdrReader, Deserialize, RosMessage, RosService, Serialize};
use nros_rmw::{
    Publisher, QosSettings, Rmw, RmwConfig, ServiceClientTrait, ServiceInfo, ServiceServerTrait,
    Session, SessionMode, Subscriber, TopicInfo, TransportError,
};
use nros_rmw_xrce::{
    XrcePublisher, XrceRmw, XrceServiceClient, XrceServiceServer, XrceSession, XrceSubscriber,
};

// ============================================================================
// Safe transport initialization
// ============================================================================

/// Initialize POSIX UDP transport for XRCE-DDS.
///
/// Must be called before [`XrceExecutor::new()`].
///
/// # Panics
///
/// Will not panic, but the subsequent `XrceExecutor::new()` will fail
/// if the agent address is unreachable.
#[cfg(feature = "posix-udp")]
pub fn init_posix_udp(agent_addr: &str) {
    unsafe {
        nros_rmw_xrce::posix_udp::init_posix_udp_transport(agent_addr);
    }
}

/// Initialize POSIX serial transport for XRCE-DDS.
///
/// Must be called before [`XrceExecutor::new()`].
#[cfg(feature = "posix-serial")]
pub fn init_posix_serial(pty_path: &str) {
    unsafe {
        nros_rmw_xrce::posix_serial::init_posix_serial_transport(pty_path);
    }
}

// ============================================================================
// Error type
// ============================================================================

/// Error type for XRCE node operations.
#[derive(Debug)]
pub enum XrceNodeError {
    /// Transport-level error from the XRCE RMW backend.
    Transport(TransportError),
    /// CDR serialization failed.
    Serialization,
    /// CDR deserialization failed.
    Deserialization,
    /// Buffer too small for the message.
    BufferTooSmall,
}

impl From<TransportError> for XrceNodeError {
    fn from(e: TransportError) -> Self {
        XrceNodeError::Transport(e)
    }
}

impl core::fmt::Display for XrceNodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            XrceNodeError::Transport(e) => write!(f, "Transport error: {:?}", e),
            XrceNodeError::Serialization => write!(f, "Serialization error"),
            XrceNodeError::Deserialization => write!(f, "Deserialization error"),
            XrceNodeError::BufferTooSmall => write!(f, "Buffer too small"),
        }
    }
}

// ============================================================================
// XrceExecutor
// ============================================================================

/// XRCE-DDS executor — owns the session and drives I/O.
///
/// Call [`spin_once()`](XrceExecutor::spin_once) in your main loop to
/// process XRCE-DDS network I/O and dispatch callbacks.
pub struct XrceExecutor {
    session: XrceSession,
}

impl XrceExecutor {
    /// Open a new XRCE-DDS session.
    ///
    /// A transport must be initialized first via [`init_posix_udp()`] or
    /// [`init_posix_serial()`].
    pub fn new(node_name: &str, domain_id: u32) -> Result<Self, XrceNodeError> {
        let config = RmwConfig {
            locator: "",
            mode: SessionMode::Client,
            domain_id,
            node_name,
            namespace: "",
        };

        let session = XrceRmw::open(&config).map_err(XrceNodeError::Transport)?;
        Ok(Self { session })
    }

    /// Create a node handle for creating publishers, subscribers, and services.
    pub fn create_node(&mut self) -> XrceNode<'_> {
        XrceNode {
            session: &mut self.session,
        }
    }

    /// Process XRCE-DDS I/O and dispatch callbacks.
    ///
    /// Returns `true` if the session is still active.
    pub fn spin_once(&mut self, timeout_ms: i32) -> bool {
        self.session.spin_once(timeout_ms)
    }

    /// Close the XRCE-DDS session.
    pub fn close(&mut self) -> Result<(), XrceNodeError> {
        self.session.close().map_err(XrceNodeError::Transport)
    }

    /// Get a mutable reference to the underlying session.
    ///
    /// This is useful for advanced use cases like manual action protocol
    /// composition where the raw session is needed alongside typed handles.
    pub fn session_mut(&mut self) -> &mut XrceSession {
        &mut self.session
    }
}

// ============================================================================
// XrceNode
// ============================================================================

/// XRCE-DDS node handle — borrows the session to create typed entities.
pub struct XrceNode<'a> {
    session: &'a mut XrceSession,
}

impl<'a> XrceNode<'a> {
    /// Create a typed publisher on the given topic.
    pub fn create_publisher<M: RosMessage>(
        &mut self,
        topic_name: &str,
    ) -> Result<XrceNodePublisher<M>, XrceNodeError> {
        let topic = TopicInfo::new(topic_name, M::TYPE_NAME, "");
        let inner = self
            .session
            .create_publisher(&topic, QosSettings::RELIABLE)
            .map_err(XrceNodeError::Transport)?;
        Ok(XrceNodePublisher {
            inner,
            _marker: PhantomData,
        })
    }

    /// Create a typed publisher with custom QoS settings.
    pub fn create_publisher_with_qos<M: RosMessage>(
        &mut self,
        topic_name: &str,
        qos: QosSettings,
    ) -> Result<XrceNodePublisher<M>, XrceNodeError> {
        let topic = TopicInfo::new(topic_name, M::TYPE_NAME, "");
        let inner = self
            .session
            .create_publisher(&topic, qos)
            .map_err(XrceNodeError::Transport)?;
        Ok(XrceNodePublisher {
            inner,
            _marker: PhantomData,
        })
    }

    /// Create a typed subscription on the given topic.
    pub fn create_subscription<M: RosMessage>(
        &mut self,
        topic_name: &str,
    ) -> Result<XrceNodeSubscription<M>, XrceNodeError> {
        let topic = TopicInfo::new(topic_name, M::TYPE_NAME, "");
        let inner = self
            .session
            .create_subscriber(&topic, QosSettings::RELIABLE)
            .map_err(XrceNodeError::Transport)?;
        Ok(XrceNodeSubscription {
            inner,
            _marker: PhantomData,
        })
    }

    /// Create a typed subscription with custom QoS settings.
    pub fn create_subscription_with_qos<M: RosMessage>(
        &mut self,
        topic_name: &str,
        qos: QosSettings,
    ) -> Result<XrceNodeSubscription<M>, XrceNodeError> {
        let topic = TopicInfo::new(topic_name, M::TYPE_NAME, "");
        let inner = self
            .session
            .create_subscriber(&topic, qos)
            .map_err(XrceNodeError::Transport)?;
        Ok(XrceNodeSubscription {
            inner,
            _marker: PhantomData,
        })
    }

    /// Create a typed service server.
    pub fn create_service_server<S: RosService>(
        &mut self,
        service_name: &str,
    ) -> Result<XrceNodeServiceServer<S>, XrceNodeError> {
        let info = ServiceInfo::new(service_name, S::SERVICE_NAME, "");
        let inner = self
            .session
            .create_service_server(&info)
            .map_err(XrceNodeError::Transport)?;
        Ok(XrceNodeServiceServer {
            inner,
            _marker: PhantomData,
        })
    }

    /// Create a typed service client.
    pub fn create_service_client<S: RosService>(
        &mut self,
        service_name: &str,
    ) -> Result<XrceNodeServiceClient<S>, XrceNodeError> {
        let info = ServiceInfo::new(service_name, S::SERVICE_NAME, "");
        let inner = self
            .session
            .create_service_client(&info)
            .map_err(XrceNodeError::Transport)?;
        Ok(XrceNodeServiceClient {
            inner,
            _marker: PhantomData,
        })
    }

    /// Get a mutable reference to the underlying session.
    ///
    /// Useful for creating raw publishers/subscribers for manual action
    /// protocol composition.
    pub fn session_mut(&mut self) -> &mut XrceSession {
        self.session
    }
}

// ============================================================================
// Typed publisher
// ============================================================================

/// Typed XRCE publisher with compile-time message type checking.
pub struct XrceNodePublisher<M: RosMessage> {
    inner: XrcePublisher,
    _marker: PhantomData<M>,
}

impl<M: RosMessage + Serialize> XrceNodePublisher<M> {
    /// Publish a typed message using the provided buffer for CDR serialization.
    pub fn publish(&self, msg: &M, buf: &mut [u8]) -> Result<(), XrceNodeError> {
        self.inner
            .publish(msg, buf)
            .map_err(XrceNodeError::Transport)
    }

    /// Publish raw bytes (already CDR-serialized).
    pub fn publish_raw(&self, data: &[u8]) -> Result<(), XrceNodeError> {
        self.inner
            .publish_raw(data)
            .map_err(XrceNodeError::Transport)
    }
}

// ============================================================================
// Typed subscription
// ============================================================================

/// Typed XRCE subscription with compile-time message type checking.
pub struct XrceNodeSubscription<M: RosMessage> {
    inner: XrceSubscriber,
    _marker: PhantomData<M>,
}

impl<M: RosMessage + Deserialize> XrceNodeSubscription<M> {
    /// Try to receive a typed message.
    ///
    /// Returns `Ok(Some(msg))` if a message is available, `Ok(None)` if not.
    pub fn try_recv(&mut self, buf: &mut [u8]) -> Result<Option<M>, XrceNodeError> {
        match self
            .inner
            .try_recv_raw(buf)
            .map_err(XrceNodeError::Transport)?
        {
            Some(len) => {
                let mut reader = CdrReader::new_with_header(&buf[..len])
                    .map_err(|_| XrceNodeError::Deserialization)?;
                let msg =
                    M::deserialize(&mut reader).map_err(|_| XrceNodeError::Deserialization)?;
                Ok(Some(msg))
            }
            None => Ok(None),
        }
    }

    /// Try to receive raw bytes (CDR-serialized).
    pub fn try_recv_raw(&mut self, buf: &mut [u8]) -> Result<Option<usize>, XrceNodeError> {
        self.inner
            .try_recv_raw(buf)
            .map_err(XrceNodeError::Transport)
    }

    /// Process the next message in-place without copying (non-blocking).
    ///
    /// Deserializes the message directly from the transport's internal buffer
    /// and calls `f` with a reference to the typed message. The buffer is locked
    /// during `f`, preventing the callback from overwriting it.
    ///
    /// Returns `Ok(true)` if a message was available and `f` was called,
    /// `Ok(false)` if no message was available.
    pub fn process_in_place(&mut self, f: impl FnOnce(&M)) -> Result<bool, XrceNodeError> {
        use nros_rmw::Subscriber;
        let mut deser_err = false;
        let processed = self
            .inner
            .process_raw_in_place(|raw| {
                match CdrReader::new_with_header(raw).and_then(|mut r| M::deserialize(&mut r)) {
                    Ok(msg) => f(&msg),
                    Err(_) => deser_err = true,
                }
            })
            .map_err(XrceNodeError::Transport)?;

        if deser_err {
            return Err(XrceNodeError::Deserialization);
        }
        Ok(processed)
    }

    /// Check if data is available without consuming it.
    pub fn has_data(&self) -> bool {
        self.inner.has_data()
    }
}

// ============================================================================
// Typed service server
// ============================================================================

/// Typed XRCE service server with compile-time request/reply type checking.
pub struct XrceNodeServiceServer<S: RosService> {
    inner: XrceServiceServer,
    _marker: PhantomData<S>,
}

impl<S: RosService> XrceNodeServiceServer<S>
where
    S::Request: Deserialize,
    S::Reply: Serialize,
{
    /// Handle a pending request using the provided handler function.
    ///
    /// Returns `Ok(true)` if a request was handled, `Ok(false)` if none was pending.
    pub fn handle_request(
        &mut self,
        req_buf: &mut [u8],
        reply_buf: &mut [u8],
        handler: impl FnOnce(&S::Request) -> S::Reply,
    ) -> Result<bool, XrceNodeError> {
        self.inner
            .handle_request::<S>(req_buf, reply_buf, handler)
            .map_err(XrceNodeError::Transport)
    }

    /// Check if a request is pending.
    pub fn has_request(&self) -> bool {
        self.inner.has_request()
    }
}

// ============================================================================
// Typed service client
// ============================================================================

/// Typed XRCE service client with compile-time request/reply type checking.
pub struct XrceNodeServiceClient<S: RosService> {
    inner: XrceServiceClient,
    _marker: PhantomData<S>,
}

impl<S: RosService> XrceNodeServiceClient<S>
where
    S::Request: Serialize,
    S::Reply: Deserialize,
{
    /// Call a service with a typed request and receive a typed reply.
    pub fn call(
        &mut self,
        request: &S::Request,
        req_buf: &mut [u8],
        reply_buf: &mut [u8],
    ) -> Result<S::Reply, XrceNodeError> {
        self.inner
            .call::<S>(request, req_buf, reply_buf)
            .map_err(XrceNodeError::Transport)
    }

    /// Send a raw request and receive a raw reply.
    pub fn call_raw(
        &mut self,
        request: &[u8],
        reply_buf: &mut [u8],
    ) -> Result<usize, XrceNodeError> {
        self.inner
            .call_raw(request, reply_buf)
            .map_err(XrceNodeError::Transport)
    }
}

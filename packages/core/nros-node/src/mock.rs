//! Mock session for unit tests.
//!
//! Provides [`MockSession`] as the `ConcreteSession` when no real RMW
//! backend feature is enabled during test compilation. Module-level
//! cfg gate in `lib.rs:75` matches `executor/mod.rs:42` so mock.rs
//! only compiles when the test paths that actually consume it are
//! also compiled.

use core::cell::Cell;

use nros_rmw::{
    Publisher, QosSettings, ServiceClientTrait, ServiceInfo, ServiceRequest, ServiceServerTrait,
    Session, Subscriber, TopicInfo, TransportError,
};

/// Mock subscriber that can be loaded with canned CDR data.
pub struct MockSubscriber {
    /// Pre-encoded data to return on the next `try_recv_raw` call.
    pub pending: Cell<Option<([u8; 256], usize)>>,
}

impl MockSubscriber {
    pub fn new() -> Self {
        Self {
            pending: Cell::new(None),
        }
    }

    pub fn load(&self, data: [u8; 256], len: usize) {
        self.pending.set(Some((data, len)));
    }
}

impl Subscriber for MockSubscriber {
    type Error = TransportError;

    fn has_data(&self) -> bool {
        self.pending.get().is_some()
    }

    fn try_recv_raw(&mut self, buf: &mut [u8]) -> Result<Option<usize>, TransportError> {
        match self.pending.get() {
            Some((data, len)) => {
                buf[..len].copy_from_slice(&data[..len]);
                self.pending.set(None);
                Ok(Some(len))
            }
            None => Ok(None),
        }
    }

    fn deserialization_error(&self) -> TransportError {
        TransportError::DeserializationError
    }
}

/// Mock service server (needed for Session).
pub struct MockServiceServer;

impl ServiceServerTrait for MockServiceServer {
    type Error = TransportError;

    fn try_recv_request<'a>(
        &mut self,
        _buf: &'a mut [u8],
    ) -> Result<Option<ServiceRequest<'a>>, TransportError> {
        Ok(None)
    }

    fn send_reply(&mut self, _seq: i64, _data: &[u8]) -> Result<(), TransportError> {
        Ok(())
    }
}

/// Dummy publisher (never sends).
pub struct MockPublisher;

impl Publisher for MockPublisher {
    type Error = TransportError;

    fn publish_raw(&self, _data: &[u8]) -> Result<(), TransportError> {
        Ok(())
    }

    fn buffer_error(&self) -> TransportError {
        TransportError::BufferTooSmall
    }

    fn serialization_error(&self) -> TransportError {
        TransportError::SerializationError
    }
}

/// Mock service client with controllable async reply behavior.
pub struct MockServiceClient {
    /// Pre-loaded reply data to return on next `try_recv_reply_raw` call.
    pub pending_reply: Cell<Option<([u8; 256], usize)>>,
}

impl MockServiceClient {
    pub fn new() -> Self {
        Self {
            pending_reply: Cell::new(None),
        }
    }

    /// Load a reply that will be returned by the next `try_recv_reply_raw` call.
    pub fn load_reply(&self, data: [u8; 256], len: usize) {
        self.pending_reply.set(Some((data, len)));
    }
}

impl ServiceClientTrait for MockServiceClient {
    type Error = TransportError;

    #[allow(deprecated)]
    fn call_raw(&mut self, _req: &[u8], _reply_buf: &mut [u8]) -> Result<usize, TransportError> {
        Err(TransportError::Timeout)
    }

    fn send_request_raw(&mut self, _request: &[u8]) -> Result<(), TransportError> {
        Ok(())
    }

    fn try_recv_reply_raw(
        &mut self,
        reply_buf: &mut [u8],
    ) -> Result<Option<usize>, TransportError> {
        match self.pending_reply.get() {
            Some((data, len)) => {
                reply_buf[..len].copy_from_slice(&data[..len]);
                self.pending_reply.set(None);
                Ok(Some(len))
            }
            None => Ok(None),
        }
    }
}

/// Mock session that produces mock handles.
pub struct MockSession;

impl MockSession {
    pub fn new() -> Self {
        Self
    }
}

impl Session for MockSession {
    type Error = TransportError;
    type PublisherHandle = MockPublisher;
    type SubscriberHandle = MockSubscriber;
    type ServiceServerHandle = MockServiceServer;
    type ServiceClientHandle = MockServiceClient;

    fn create_publisher(
        &mut self,
        _topic: &TopicInfo,
        _qos: QosSettings,
    ) -> Result<MockPublisher, TransportError> {
        Ok(MockPublisher)
    }

    fn create_subscriber(
        &mut self,
        _topic: &TopicInfo,
        _qos: QosSettings,
    ) -> Result<MockSubscriber, TransportError> {
        Ok(MockSubscriber::new())
    }

    fn create_service_server(
        &mut self,
        _service: &ServiceInfo,
    ) -> Result<MockServiceServer, TransportError> {
        Ok(MockServiceServer)
    }

    fn create_service_client(
        &mut self,
        _service: &ServiceInfo,
    ) -> Result<MockServiceClient, TransportError> {
        Ok(MockServiceClient::new())
    }

    fn close(&mut self) -> Result<(), TransportError> {
        Ok(())
    }

    fn drive_io(&mut self, _timeout_ms: i32) -> Result<(), TransportError> {
        // Mock transport: no I/O to drive.
        Ok(())
    }
}

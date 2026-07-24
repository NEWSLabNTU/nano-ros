//! Mock session for unit tests.
//!
//! Provides [`MockSession`] as the `ConcreteSession` when no real RMW
//! backend feature is enabled during test compilation. Module-level
//! cfg gate in `lib.rs:75` matches `executor/mod.rs:42` so mock.rs
//! only compiles when the test paths that actually consume it are
//! also compiled.

use core::cell::{Cell, RefCell};

use nros_rmw::{
    ClientTrait, Publisher, QosSettings, ServiceInfo, ServiceRequest, ServiceTrait, Session,
    Subscription, TopicInfo, TransportError,
};

/// Mock subscriber that can be loaded with canned CDR data. Holds a small
/// **queue** (not a single slot) so tests can inject a burst — several messages
/// arriving before a `try_recv_raw`/spin — to exercise the QoS-depth ring
/// (Phase 239.5/7). `load` pushes; `try_recv_raw` pops in FIFO order.
pub struct MockSubscriber {
    queue: RefCell<heapless::Deque<([u8; 256], usize), 8>>,
}

impl MockSubscriber {
    pub fn new() -> Self {
        Self {
            queue: RefCell::new(heapless::Deque::new()),
        }
    }

    /// Enqueue one canned message (FIFO). Silently drops if the queue is full.
    pub fn load(&self, data: [u8; 256], len: usize) {
        let _ = self.queue.borrow_mut().push_back((data, len));
    }
}

impl Subscription for MockSubscriber {
    type Error = TransportError;

    fn has_data(&self) -> bool {
        !self.queue.borrow().is_empty()
    }

    fn try_recv_raw(&mut self, buf: &mut [u8]) -> Result<Option<usize>, TransportError> {
        match self.queue.borrow_mut().pop_front() {
            Some((data, len)) => {
                buf[..len].copy_from_slice(&data[..len]);
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
/// Mock service server that can be loaded with a canned CDR request, so unit
/// tests can drive a service callback through `spin_once` (Phase 189.M3.3.d).
///
/// Phase 237 — `try_recv_request` hands out a distinct, monotonically increasing
/// `sequence_number` per request (the reply-correlation token), and `send_reply`
/// records `(seq, data)` so tests can assert deferred replies route to the right
/// requester — the concurrent-safety the seq-keyed backends guarantee.
pub struct MockServiceServer {
    /// Pre-encoded request returned on the next `try_recv_request` call.
    pub pending: Cell<Option<([u8; 256], usize)>>,
    /// Next correlation token `try_recv_request` will return (then increments).
    pub next_seq: Cell<i64>,
    /// Replies recorded by `send_reply`: `(seq, data, len)`.
    pub sent: core::cell::RefCell<heapless::Vec<(i64, [u8; 256], usize), 8>>,
}

impl MockServiceServer {
    pub fn new() -> Self {
        Self {
            pending: Cell::new(None),
            next_seq: Cell::new(0),
            sent: core::cell::RefCell::new(heapless::Vec::new()),
        }
    }

    pub fn load(&self, data: [u8; 256], len: usize) {
        self.pending.set(Some((data, len)));
    }
}

impl ServiceTrait for MockServiceServer {
    type Error = TransportError;

    fn has_request(&self) -> bool {
        self.pending.get().is_some()
    }

    fn try_recv_request<'a>(
        &mut self,
        buf: &'a mut [u8],
    ) -> Result<Option<ServiceRequest<'a>>, TransportError> {
        match self.pending.get() {
            Some((data, len)) => {
                buf[..len].copy_from_slice(&data[..len]);
                self.pending.set(None);
                let seq = self.next_seq.get();
                self.next_seq.set(seq + 1);
                Ok(Some(ServiceRequest {
                    data: &buf[..len],
                    sequence_number: seq,
                }))
            }
            None => Ok(None),
        }
    }

    fn send_reply(&mut self, seq: i64, data: &[u8]) -> Result<(), TransportError> {
        let mut rec = [0u8; 256];
        let len = data.len().min(rec.len());
        rec[..len].copy_from_slice(&data[..len]);
        // Bounded by the test's expected reply count; ignore overflow.
        let _ = self.sent.borrow_mut().push((seq, rec, len));
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

impl ClientTrait for MockServiceClient {
    type Error = TransportError;

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
    type SubscriptionHandle = MockSubscriber;
    type ServiceHandle = MockServiceServer;
    type ClientHandle = MockServiceClient;

    /// The mock (test) backend supports every QoS policy, so QoS validation
    /// never rejects a test entity (the default `CORE` mask can't even admit
    /// the default profile's liveliness bit).
    fn supported_qos_policies(&self) -> nros_rmw::QosPolicyMask {
        nros_rmw::QosPolicyMask(u32::MAX)
    }

    fn create_publisher(
        &mut self,
        _topic: &TopicInfo,
        _qos: QosSettings,
    ) -> Result<MockPublisher, TransportError> {
        Ok(MockPublisher)
    }

    fn create_subscription(
        &mut self,
        _topic: &TopicInfo,
        _qos: QosSettings,
    ) -> Result<MockSubscriber, TransportError> {
        Ok(MockSubscriber::new())
    }

    fn create_service(
        &mut self,
        _service: &ServiceInfo,
        _qos: QosSettings,
    ) -> Result<MockServiceServer, TransportError> {
        Ok(MockServiceServer::new())
    }

    fn create_client(
        &mut self,
        _service: &ServiceInfo,
        _qos: QosSettings,
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

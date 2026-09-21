//! Mock session for unit tests.
//!
//! Provides [`MockSession`] as the `ConcreteSession` when no real RMW
//! backend feature is enabled during test compilation. Module-level
//! cfg gate in `lib.rs:75` matches `executor/mod.rs:42` so mock.rs
//! only compiles when the test paths that actually consume it are
//! also compiled.

use core::cell::{Cell, RefCell};

use nros_rmw::{
    ClientTrait, GraphEntityKind, Publisher, QoSProfile, ServiceInfo, ServiceRequest, ServiceTrait,
    Session, Subscription, TopicInfo, TransportError,
};

/// Mock subscriber that can be loaded with canned CDR data. Holds a small
/// **queue** (not a single slot) so tests can inject a burst — several messages
/// arriving before a `take_serialized`/spin — to exercise the QoS-depth ring
/// (Phase 239.5/7). `load` pushes; `take_serialized` pops in FIFO order.
/// One queued take: the bytes and their length, or the error the take reports.
/// Named because `clippy::type_complexity` is right that the inline form was
/// unreadable.
type MockTake = Result<([u8; 256], usize), TransportError>;

pub struct MockSubscriber {
    /// FIFO of canned OUTCOMES, not canned messages. A queue of `Ok` only
    /// cannot express the case issue 0757 is about — a take that FAILS — so
    /// every test written against it necessarily exercised the happy path, and
    /// the four copies of the drain loop that swallowed a non-`Ok` take were
    /// unreachable by the unit suite for as long as they existed.
    queue: RefCell<heapless::Deque<MockTake, 8>>,
}

impl MockSubscriber {
    pub fn new() -> Self {
        Self {
            queue: RefCell::new(heapless::Deque::new()),
        }
    }

    /// Enqueue one canned message (FIFO). Silently drops if the queue is full.
    pub fn load(&self, data: [u8; 256], len: usize) {
        let _ = self.queue.borrow_mut().push_back(Ok((data, len)));
    }

    /// Enqueue one canned FAILURE (FIFO), so a test can drive the drain loop's
    /// error arm — issue 0757. `has_data` still reports true for it: that is
    /// exactly the state the bug lived in, a subscription the executor believes
    /// is ready whose take then fails.
    pub fn load_error(&self, err: TransportError) {
        let _ = self.queue.borrow_mut().push_back(Err(err));
    }
}

impl Subscription for MockSubscriber {
    type Error = TransportError;

    fn has_data(&self) -> bool {
        !self.queue.borrow().is_empty()
    }

    fn take_serialized(&mut self, buf: &mut [u8]) -> Result<Option<usize>, TransportError> {
        match self.queue.borrow_mut().pop_front() {
            Some(Ok((data, len))) => {
                buf[..len].copy_from_slice(&data[..len]);
                Ok(Some(len))
            }
            Some(Err(err)) => Err(err),
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
/// Phase 237 — `take_request` hands out a distinct, monotonically increasing
/// `sequence_number` per request (the reply-correlation token), and `send_response`
/// records `(seq, data)` so tests can assert deferred replies route to the right
/// requester — the concurrent-safety the seq-keyed backends guarantee.
pub struct MockServiceServer {
    /// Pre-encoded request returned on the next `take_request` call.
    pub pending: Cell<Option<([u8; 256], usize)>>,
    /// Next correlation token `take_request` will return (then increments).
    pub next_seq: Cell<i64>,
    /// Replies recorded by `send_response`: `(seq, data, len)`.
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

    fn take_request<'a>(
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

    fn send_response(&mut self, seq: i64, data: &[u8]) -> Result<(), TransportError> {
        let mut rec = [0u8; 256];
        let len = data.len().min(rec.len());
        rec[..len].copy_from_slice(&data[..len]);
        // Bounded by the test's expected reply count; ignore overflow.
        let _ = self.sent.borrow_mut().push((seq, rec, len));
        Ok(())
    }
}

/// Largest payload [`MockPublisher`] retains per sample. Sized to
/// `action_core`'s `STATUS_ARRAY_BUF` so a full `GoalStatusArray` is recorded
/// whole; a longer payload is recorded truncated, and the RECORDED length is
/// still the true one so a test can tell truncation from a short message.
pub const MOCK_PUBLISH_RECORD: usize = 512;

/// One recorded publish: the bytes (truncated to [`MOCK_PUBLISH_RECORD`]) and
/// the payload's true length.
pub type MockPublished = ([u8; MOCK_PUBLISH_RECORD], usize);

/// Publisher that sends nowhere but REMEMBERS what it was handed.
///
/// It recorded nothing until issue 1361, so every status-array assertion in the
/// suite was necessarily about the core's internal tables rather than about the
/// bytes a subscriber receives — which is exactly where 1361 lived: the tables
/// were right and the published array was empty. A publisher double that drops
/// its argument can only test the paths that do not care what was published.
///
/// Keeps the most recent 8 samples (older ones drop off the front) plus a total
/// count that window does not bound, so a test can assert both "how many
/// publishes" and "what the last one said".
pub struct MockPublisher {
    published: RefCell<heapless::Deque<MockPublished, 8>>,
    count: Cell<usize>,
}

impl MockPublisher {
    pub fn new() -> Self {
        Self {
            published: RefCell::new(heapless::Deque::new()),
            count: Cell::new(0),
        }
    }

    /// Total number of `publish_raw` calls, including samples aged out of the
    /// retained window.
    pub fn publish_count(&self) -> usize {
        self.count.get()
    }

    /// The most recently published sample, or `None` if nothing was published.
    pub fn last_published(&self) -> Option<MockPublished> {
        self.published.borrow().back().copied()
    }

    /// Forget every retained sample and reset the count — for a test asserting
    /// about publishes made AFTER some setup step.
    pub fn clear_published(&self) {
        self.published.borrow_mut().clear();
        self.count.set(0);
    }
}

impl Default for MockPublisher {
    fn default() -> Self {
        Self::new()
    }
}

impl Publisher for MockPublisher {
    type Error = TransportError;

    fn publish_raw(&self, data: &[u8]) -> Result<(), TransportError> {
        let mut rec = [0u8; MOCK_PUBLISH_RECORD];
        let copied = data.len().min(MOCK_PUBLISH_RECORD);
        rec[..copied].copy_from_slice(&data[..copied]);
        let mut queue = self.published.borrow_mut();
        if queue.is_full() {
            let _ = queue.pop_front();
        }
        let _ = queue.push_back((rec, data.len()));
        self.count.set(self.count.get() + 1);
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
    /// Pre-loaded reply data to return on next `take_response_raw` call.
    pub pending_reply: Cell<Option<([u8; 256], usize)>>,
    /// Issue 0778 — the id handed to the next `send_request_raw`.
    pub next_seq: Cell<i64>,
    /// The id of the most recent send; what a reply is reported against.
    pub last_sent_seq: Cell<Option<i64>>,
}

impl MockServiceClient {
    pub fn new() -> Self {
        Self {
            pending_reply: Cell::new(None),
            next_seq: Cell::new(0),
            last_sent_seq: Cell::new(None),
        }
    }

    /// Load a reply that will be returned by the next `take_response_raw` call.
    pub fn load_reply(&self, data: [u8; 256], len: usize) {
        self.pending_reply.set(Some((data, len)));
    }
}

impl ClientTrait for MockServiceClient {
    type Error = TransportError;

    /// Issue 0778 — a monotonic id, so a test that cares can tell two calls
    /// apart. A mock returning a constant would make the contract untestable
    /// in exactly the direction the issue is about.
    fn send_request_raw(&mut self, _request: &[u8]) -> Result<i64, TransportError> {
        let seq = self.next_seq.get();
        self.next_seq.set(seq + 1);
        self.last_sent_seq.set(Some(seq));
        Ok(seq)
    }

    fn take_response_raw(
        &mut self,
        reply_buf: &mut [u8],
    ) -> Result<Option<(usize, i64)>, TransportError> {
        match self.pending_reply.get() {
            Some((data, len)) => {
                reply_buf[..len].copy_from_slice(&data[..len]);
                self.pending_reply.set(None);
                // The mock answers whatever it was last asked.
                Ok(Some((len, self.last_sent_seq.get().unwrap_or(0))))
            }
            None => Ok(None),
        }
    }
}

/// Mock session that produces mock handles.
pub struct MockSession {
    /// Issue 0790 — optional observer bumped once per [`Session::close`].
    ///
    /// A shutdown hook is handed nothing but its own `*mut c_void`; it cannot
    /// reach the executor that is closing, so "was the session still open when
    /// I ran?" has to be observable from OUTSIDE the executor. This is that
    /// observation, and it is what lets the ordering test assert the ORDER
    /// rather than merely that both hooks ran.
    ///
    /// A `&'static` the TEST owns, not a module-level static: `cargo test` runs
    /// test functions on parallel threads, and one shared counter would make
    /// every close in the suite look like this test's.
    close_observer: Option<&'static core::sync::atomic::AtomicUsize>,
    /// Issue 1268 — make `create_service` fail, so a test can watch what the
    /// executor does with a backend that will not serve the parameter services.
    /// The real one that does this is Cyclone with no registered descriptor for
    /// the `rcl_interfaces` types, which answers `Unsupported`.
    service_create_error: Option<TransportError>,
    /// Issue 1268 — counts `create_service` calls, which is how a test tells
    /// "asked once and gave up" from "asks again on every spin". A `&'static`
    /// the TEST owns, for the reason `close_observer` documents above.
    service_create_attempts: Option<&'static core::sync::atomic::AtomicUsize>,
    /// phase-444 — answer the RFC-0036 graph slots with a canned graph instead
    /// of the trait default.
    ///
    /// OFF by default, and that default is load-bearing: the `Session` trait
    /// answers every graph slot `Unsupported`, which is what a backend with no
    /// graph (XRCE) reports and what the contract says must never be collapsed
    /// into "empty". A test asserting THAT wants the default; a test asserting
    /// a forwarder reaches its own slot wants this.
    graph: bool,
}

impl MockSession {
    pub fn new() -> Self {
        Self {
            close_observer: None,
            service_create_error: None,
            service_create_attempts: None,
            graph: false,
        }
    }

    /// Issue 1268 — a session whose `create_service` always fails with `error`,
    /// counting each attempt into `attempts`.
    pub fn with_failing_service_create(
        error: TransportError,
        attempts: &'static core::sync::atomic::AtomicUsize,
    ) -> Self {
        Self {
            close_observer: None,
            service_create_error: Some(error),
            service_create_attempts: Some(attempts),
            graph: false,
        }
    }

    /// A session whose `close()` bumps `observer`. See the field doc.
    pub fn with_close_observer(observer: &'static core::sync::atomic::AtomicUsize) -> Self {
        Self {
            close_observer: Some(observer),
            service_create_error: None,
            service_create_attempts: None,
            graph: false,
        }
    }

    /// phase-444 — a session that answers the graph slots with [`CANNED`]
    /// instead of `Unsupported`.
    ///
    /// Every slot answers DISTINGUISHABLY, because the bug a forwarder
    /// actually has is reaching the wrong slot: the two counts differ, the
    /// four `by_node` kinds each emit their own marker, and each `by_node`
    /// walk echoes the `node_name` and `node_namespace` it was handed so a
    /// forwarder that swapped them fails rather than passes.
    pub fn with_graph() -> Self {
        Self {
            close_observer: None,
            service_create_error: None,
            service_create_attempts: None,
            graph: true,
        }
    }
}

/// The canned graph [`MockSession::with_graph`] reports. Public so a test
/// asserts against the same constants the mock emits rather than re-typing
/// them (a re-typed literal is how a test stops testing the mapping).
pub mod canned {
    /// `get_node_names` — one node WITH an enclave, one WITHOUT, because
    /// `None` is a partial answer the contract admits and not an error.
    pub const NODES: [(&str, &str, Option<&str>); 2] = [
        ("talker", "/", Some("/enclave_t")),
        ("listener", "/demo", None),
    ];
    /// `get_topic_names_and_types` — one topic carrying TWO types, which is
    /// one visit with two entries and not two visits.
    pub const TOPIC: (&str, [&str; 2]) =
        ("/chatter", ["std_msgs/msg/String", "std_msgs/msg/Header"]);
    /// `get_service_names_and_types`.
    pub const SERVICE: (&str, [&str; 1]) = ("/add_two_ints", ["example_interfaces/srv/AddTwoInts"]);
    /// `count_publishers`. Distinct from [`SUBSCRIBERS`] so a forwarder wired
    /// to the other counter fails.
    pub const PUBLISHERS: usize = 3;
    /// `count_subscribers`.
    pub const SUBSCRIBERS: usize = 7;

    /// The marker `get_names_and_types_by_node` emits for each entity kind.
    /// A forwarder that passes the wrong `GraphEntityKind` sees another
    /// kind's marker, which is the failure this exists to produce.
    pub const BY_NODE_PUBLISHER: &str = "/by_node/publisher";
    /// See [`BY_NODE_PUBLISHER`].
    pub const BY_NODE_SUBSCRIBER: &str = "/by_node/subscriber";
    /// See [`BY_NODE_PUBLISHER`].
    pub const BY_NODE_SERVICE: &str = "/by_node/service";
    /// See [`BY_NODE_PUBLISHER`].
    pub const BY_NODE_CLIENT: &str = "/by_node/client";
    /// The type every `by_node` marker carries.
    pub const BY_NODE_TYPE: &str = "test_msgs/msg/Marker";
}

impl Session for MockSession {
    type Error = TransportError;
    type PublisherHandle = MockPublisher;
    type SubscriptionHandle = MockSubscriber;
    type ServiceHandle = MockServiceServer;
    type ClientHandle = MockServiceClient;

    /// nros-qos-exempt: a TEST double with no transport. It delivers nothing,
    /// so it can neither honour nor violate a policy, and admitting every
    /// profile is what keeps a test about something else from being a test
    /// about QoS validation.
    ///
    /// The comment here used to justify this by the default mask being too
    /// narrow for the default profile's liveliness bit. That reason is gone —
    /// phase-428 W9 made the default EMPTY — and the real reason was never
    /// the default's width.
    fn supported_qos_policies(&self) -> nros_rmw::QoSPolicyMask {
        nros_rmw::QoSPolicyMask(u32::MAX)
    }

    fn create_publisher(
        &mut self,
        _topic: &TopicInfo,
        _qos: QoSProfile,
    ) -> Result<MockPublisher, TransportError> {
        Ok(MockPublisher::new())
    }

    fn create_subscription(
        &mut self,
        _topic: &TopicInfo,
        _qos: QoSProfile,
    ) -> Result<MockSubscriber, TransportError> {
        Ok(MockSubscriber::new())
    }

    fn create_service(
        &mut self,
        _service: &ServiceInfo,
        _qos: QoSProfile,
    ) -> Result<MockServiceServer, TransportError> {
        if let Some(attempts) = self.service_create_attempts {
            attempts.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
        }
        if let Some(e) = self.service_create_error.as_ref() {
            return Err(e.clone());
        }
        Ok(MockServiceServer::new())
    }

    fn create_client(
        &mut self,
        _service: &ServiceInfo,
        _qos: QoSProfile,
    ) -> Result<MockServiceClient, TransportError> {
        Ok(MockServiceClient::new())
    }

    fn close(&mut self) -> Result<(), TransportError> {
        if let Some(observer) = self.close_observer {
            observer.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
        }
        Ok(())
    }

    fn drive_io(&mut self, _timeout_ms: i32) -> Result<(), TransportError> {
        // Mock transport: no I/O to drive.
        Ok(())
    }

    // ---- phase-444 — RFC-0036 graph slots ----
    //
    // Overridden ONLY when `graph` is set. Left alone, every one of these
    // keeps the trait default (`Unsupported`), which is the answer a backend
    // with no graph gives and the answer the contract forbids collapsing into
    // "empty" — so the default mock is itself the fixture for that half.

    fn get_node_names(
        &mut self,
        visit: &mut dyn FnMut(&str, &str, Option<&str>) -> bool,
    ) -> Result<(), Self::Error> {
        if !self.graph {
            return Err(TransportError::Unsupported);
        }
        for (name, ns, enclave) in canned::NODES {
            if !visit(name, ns, enclave) {
                break;
            }
        }
        Ok(())
    }

    fn get_topic_names_and_types(
        &mut self,
        visit: &mut dyn FnMut(&str, &[&str]) -> bool,
    ) -> Result<(), Self::Error> {
        if !self.graph {
            return Err(TransportError::Unsupported);
        }
        let (name, types) = canned::TOPIC;
        visit(name, &types);
        Ok(())
    }

    fn get_service_names_and_types(
        &mut self,
        visit: &mut dyn FnMut(&str, &[&str]) -> bool,
    ) -> Result<(), Self::Error> {
        if !self.graph {
            return Err(TransportError::Unsupported);
        }
        let (name, types) = canned::SERVICE;
        visit(name, &types);
        Ok(())
    }

    fn count_publishers(&mut self, _topic_name: &str) -> Result<usize, Self::Error> {
        if !self.graph {
            return Err(TransportError::Unsupported);
        }
        Ok(canned::PUBLISHERS)
    }

    fn count_subscribers(&mut self, _topic_name: &str) -> Result<usize, Self::Error> {
        if !self.graph {
            return Err(TransportError::Unsupported);
        }
        Ok(canned::SUBSCRIBERS)
    }

    /// Emits THREE entries: the kind's own marker, then the `node_name` and
    /// `node_namespace` verbatim. The marker catches a forwarder that passed
    /// the wrong [`GraphEntityKind`]; echoing the two names catches one that
    /// passed them in the wrong order, which no fixed canned answer can.
    fn get_names_and_types_by_node(
        &mut self,
        kind: GraphEntityKind,
        node_name: &str,
        node_namespace: &str,
        visit: &mut dyn FnMut(&str, &[&str]) -> bool,
    ) -> Result<(), Self::Error> {
        if !self.graph {
            return Err(TransportError::Unsupported);
        }
        let marker = match kind {
            GraphEntityKind::Publisher => canned::BY_NODE_PUBLISHER,
            GraphEntityKind::Subscriber => canned::BY_NODE_SUBSCRIBER,
            GraphEntityKind::Service => canned::BY_NODE_SERVICE,
            GraphEntityKind::Client => canned::BY_NODE_CLIENT,
        };
        for name in [marker, node_name, node_namespace] {
            if !visit(name, &[canned::BY_NODE_TYPE]) {
                break;
            }
        }
        Ok(())
    }
}

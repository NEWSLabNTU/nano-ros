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

/// Bound on the name a mock entity remembers. 64, not the 256 a real
/// `CffiPublisher` keeps: the mock exists to answer a unit test, and a test
/// topic that needs more than 64 bytes is testing the truncation rather than
/// the accessor.
const MOCK_NAME_MAX: usize = 64;

/// Issue 1972 — the status-event registrations a mock entity accepted, so a
/// test can fire one through the same `(kind, payload, ctx)` channel a backend
/// uses and observe what the node API hands the application.
#[derive(Default)]
pub struct MockEventRegs(
    RefCell<
        heapless::Vec<
            (
                nros_rmw::EventKind,
                nros_rmw::EventCallback,
                *mut core::ffi::c_void,
            ),
            5,
        >,
    >,
);

impl MockEventRegs {
    fn register(
        &self,
        kind: nros_rmw::EventKind,
        cb: nros_rmw::EventCallback,
        ctx: *mut core::ffi::c_void,
    ) -> Result<(), TransportError> {
        let mut v = self.0.borrow_mut();
        v.retain(|(k, _, _)| *k != kind);
        v.push((kind, cb, ctx))
            .map_err(|_| TransportError::Unsupported)
    }

    /// Fire `kind` with `payload` (which must be the payload type `kind`
    /// selects). Returns whether a callback was registered for it.
    pub fn fire<T>(&self, kind: nros_rmw::EventKind, payload: &T) -> bool {
        let reg = self.0.borrow().iter().find(|(k, _, _)| *k == kind).copied();
        match reg {
            Some((k, cb, ctx)) => {
                // SAFETY: the registrant paired `cb` with `ctx`; `payload` is
                // the caller's promise that it is `kind`'s payload type.
                unsafe { cb(k, payload as *const T as *const core::ffi::c_void, ctx) };
                true
            }
            None => false,
        }
    }
}

/// The name an entity was created on, retained the way every real backend
/// handle retains it (`CffiPublisher::topic_name` writes the caller's string
/// into the handle at create and never re-resolves it).
///
/// ONE spelling for all four mock entities — phase-444. The accessor on the
/// node-side handle (`EmbeddedPublisher::topic_name` and its five siblings)
/// forwards to the CONCRETE backend handle, so the mock has to answer it too
/// or `nros-node`'s own unit build does not compile.
#[derive(Default, Debug, Clone)]
pub struct MockName(heapless::String<MOCK_NAME_MAX>);

impl MockName {
    /// Retain `name`, truncated at a char boundary if it does not fit.
    pub fn new(name: &str) -> Self {
        let mut buf: heapless::String<MOCK_NAME_MAX> = heapless::String::new();
        for c in name.chars() {
            if buf.push(c).is_err() {
                break;
            }
        }
        Self(buf)
    }

    /// The retained name. `""` for a handle built by a bare `new()`, which is
    /// what a test that never named one asked for.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

pub struct MockSubscriber {
    /// FIFO of canned OUTCOMES, not canned messages. A queue of `Ok` only
    /// cannot express the case issue 0757 is about — a take that FAILS — so
    /// every test written against it necessarily exercised the happy path, and
    /// the four copies of the drain loop that swallowed a non-`Ok` take were
    /// unreachable by the unit suite for as long as they existed.
    queue: RefCell<heapless::Deque<MockTake, 8>>,
    /// phase-444 — the topic this subscription was created on.
    topic_name: MockName,
    /// Does this mock advertise ZERO-COPY in-place dispatch — phase-456 W8.
    ///
    /// Off by default, so every test written against this mock keeps taking the
    /// buffered path it was written for. A test that wants the other arm turns
    /// it on with [`Self::set_in_place`] and gets what zenoh and XRCE give a
    /// real image: `supports_process_in_place` true, and
    /// `process_raw_in_place` handing the callback a borrowed slice of the
    /// queued sample instead of copying it into a caller buffer.
    ///
    /// It was worth adding: until W8 the executor consulted this capability in
    /// exactly one registration path, and the unit suite had no way to reach
    /// the other arm at all.
    in_place: Cell<bool>,
    /// Issue 1643 — what `count_matched_publishers` answers. `None` (the
    /// default) is the trait default, `Unsupported`, which is what every
    /// backend without the slot reports; a test sets `Some(n)` to stand for
    /// one that fills it (Cyclone).
    pub matched: Cell<Option<usize>>,
    /// Issue 1972 — registered status-event callbacks.
    pub events: MockEventRegs,
}

impl MockSubscriber {
    pub fn new() -> Self {
        Self::new_named("")
    }

    /// phase-444 — a subscriber that remembers its topic, so
    /// `Subscription::topic_name` has something to hand back.
    pub fn new_named(topic_name: &str) -> Self {
        Self {
            queue: RefCell::new(heapless::Deque::new()),
            topic_name: MockName::new(topic_name),
            in_place: Cell::new(false),
            matched: Cell::new(None),
            events: MockEventRegs::default(),
        }
    }

    /// The topic this subscription was created on.
    pub fn topic_name(&self) -> &str {
        self.topic_name.as_str()
    }

    /// Make this mock advertise in-place dispatch — phase-456 W8. See
    /// [`Self::in_place`].
    pub fn set_in_place(&self, yes: bool) {
        self.in_place.set(yes);
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

    fn count_matched_publishers(&self) -> Result<usize, TransportError> {
        self.matched.get().ok_or(TransportError::Unsupported)
    }

    fn supports_event(&self, kind: nros_rmw::EventKind) -> bool {
        !kind.is_publisher_side()
    }

    unsafe fn register_event_callback(
        &mut self,
        kind: nros_rmw::EventKind,
        _deadline_ms: u32,
        cb: nros_rmw::EventCallback,
        user_ctx: *mut core::ffi::c_void,
    ) -> Result<(), TransportError> {
        if kind.is_publisher_side() {
            return Err(TransportError::Unsupported);
        }
        self.events.register(kind, cb, user_ctx)
    }

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

    // phase-456 W8 — the in-place arm, off unless a test asks for it.
    fn supports_process_in_place(&self) -> bool {
        self.in_place.get()
    }

    fn process_raw_in_place(&mut self, f: impl FnOnce(&[u8])) -> Result<bool, TransportError> {
        if !self.in_place.get() {
            return Err(TransportError::MessageTooLarge);
        }
        match self.queue.borrow_mut().pop_front() {
            // The borrow is of the popped sample rather than of the queue, which
            // is what a real backend's slot lease gives: the callback sees the
            // bytes and nothing outlives the call.
            Some(Ok((data, len))) => {
                f(&data[..len]);
                Ok(true)
            }
            Some(Err(err)) => Err(err),
            None => Ok(false),
        }
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
    /// phase-444 — the service this server was created on.
    service_name: MockName,
    /// Issue 1352 — an error the next `take_request` returns instead of a
    /// request (a backend that dropped one, e.g. an inbox overflow).
    pending_error: core::cell::RefCell<Option<TransportError>>,
    /// Issue 1942 — the metadata the pending request carries
    /// ([`load_with_info`](Self::load_with_info)); `NOT_REPORTED` by default,
    /// which is what a backend with no per-request metadata reports.
    pending_info: Cell<nros_core::ServiceInfo>,
    /// Issue 1942 — the metadata of the request the last take returned.
    taken_info: Cell<nros_core::ServiceInfo>,
}

impl MockServiceServer {
    pub fn new() -> Self {
        Self::new_named("")
    }

    /// phase-444 — a server that remembers its service name, so
    /// `EmbeddedServiceServer::service_name` has something to hand back.
    pub fn new_named(service_name: &str) -> Self {
        Self {
            pending: Cell::new(None),
            next_seq: Cell::new(0),
            sent: core::cell::RefCell::new(heapless::Vec::new()),
            service_name: MockName::new(service_name),
            pending_error: core::cell::RefCell::new(None),
            pending_info: Cell::new(nros_core::ServiceInfo::NOT_REPORTED),
            taken_info: Cell::new(nros_core::ServiceInfo::NOT_REPORTED),
        }
    }

    /// Issue 1352 — make the next `take_request` fail with `err`, once.
    pub fn load_error(&self, err: TransportError) {
        *self.pending_error.borrow_mut() = Some(err);
    }

    /// The service this server was created on.
    pub fn service_name(&self) -> &str {
        self.service_name.as_str()
    }

    pub fn load(&self, data: [u8; 256], len: usize) {
        self.pending.set(Some((data, len)));
        self.pending_info.set(nros_core::ServiceInfo::NOT_REPORTED);
    }

    /// Issue 1942 — [`load`](Self::load) a request that carries `info`, the
    /// way a backend that reports per-request metadata delivers one.
    pub fn load_with_info(&self, data: [u8; 256], len: usize, info: nros_core::ServiceInfo) {
        self.pending.set(Some((data, len)));
        self.pending_info.set(info);
    }
}

impl ServiceTrait for MockServiceServer {
    type Error = TransportError;

    fn has_request(&self) -> bool {
        self.pending.get().is_some() || self.pending_error.borrow().is_some()
    }

    fn take_request<'a>(
        &mut self,
        buf: &'a mut [u8],
    ) -> Result<Option<ServiceRequest<'a>>, TransportError> {
        if let Some(err) = self.pending_error.borrow_mut().take() {
            return Err(err);
        }
        match self.pending.get() {
            Some((data, len)) => {
                buf[..len].copy_from_slice(&data[..len]);
                self.pending.set(None);
                self.taken_info.set(self.pending_info.get());
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

    fn request_info(&self) -> nros_core::ServiceInfo {
        self.taken_info.get()
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
    /// phase-444 — the topic this publisher was created on.
    topic_name: MockName,
    /// Issue 1643 — what `count_matched_subscriptions` answers. See
    /// [`MockSubscriber::matched`].
    pub matched: Cell<Option<usize>>,
    /// Issue 1972 — registered status-event callbacks.
    pub events: MockEventRegs,
}

impl MockPublisher {
    pub fn new() -> Self {
        Self::new_named("")
    }

    /// phase-444 — a publisher that remembers its topic, so
    /// `EmbeddedPublisher::topic_name` has something to hand back.
    pub fn new_named(topic_name: &str) -> Self {
        Self {
            published: RefCell::new(heapless::Deque::new()),
            count: Cell::new(0),
            topic_name: MockName::new(topic_name),
            matched: Cell::new(None),
            events: MockEventRegs::default(),
        }
    }

    /// The topic this publisher was created on.
    pub fn topic_name(&self) -> &str {
        self.topic_name.as_str()
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

    /// The `i`-th retained sample, oldest first (the window holds the last 8).
    pub fn retained(&self, i: usize) -> Option<MockPublished> {
        self.published.borrow().iter().nth(i).copied()
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

    fn count_matched_subscriptions(&self) -> Result<usize, TransportError> {
        self.matched.get().ok_or(TransportError::Unsupported)
    }

    fn supports_event(&self, kind: nros_rmw::EventKind) -> bool {
        kind.is_publisher_side()
    }

    unsafe fn register_event_callback(
        &mut self,
        kind: nros_rmw::EventKind,
        _deadline_ms: u32,
        cb: nros_rmw::EventCallback,
        user_ctx: *mut core::ffi::c_void,
    ) -> Result<(), TransportError> {
        if !kind.is_publisher_side() {
            return Err(TransportError::Unsupported);
        }
        self.events.register(kind, cb, user_ctx)
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
    /// Issue 2009 — the id the pre-loaded reply reports instead, set by
    /// [`load_reply_for`](Self::load_reply_for).
    pub pending_reply_seq: Cell<Option<i64>>,
    /// phase-444 — the service this client was created on.
    service_name: MockName,
}

impl MockServiceClient {
    pub fn new() -> Self {
        Self::new_named("")
    }

    /// phase-444 — a client that remembers its service name, so
    /// `EmbeddedServiceClient::service_name` has something to hand back.
    pub fn new_named(service_name: &str) -> Self {
        Self {
            pending_reply: Cell::new(None),
            next_seq: Cell::new(0),
            last_sent_seq: Cell::new(None),
            pending_reply_seq: Cell::new(None),
            service_name: MockName::new(service_name),
        }
    }

    /// The service this client was created on.
    pub fn service_name(&self) -> &str {
        self.service_name.as_str()
    }

    /// Load a reply that will be returned by the next `take_response_raw` call.
    pub fn load_reply(&self, data: [u8; 256], len: usize) {
        self.pending_reply.set(Some((data, len)));
        self.pending_reply_seq.set(None);
    }

    /// Issue 2009 — load a reply that answers the request sent as `seq`,
    /// whatever was sent since.
    pub fn load_reply_for(&self, seq: i64, data: [u8; 256], len: usize) {
        self.pending_reply.set(Some((data, len)));
        self.pending_reply_seq.set(Some(seq));
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
                // The mock answers whatever it was last asked, unless the
                // reply was loaded for a named request (issue 2009).
                let seq = self
                    .pending_reply_seq
                    .take()
                    .unwrap_or_else(|| self.last_sent_seq.get().unwrap_or(0));
                Ok(Some((len, seq)))
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
    /// phase-456 W8 — every subscriber this session creates advertises in-place
    /// dispatch, as zenoh and XRCE unconditionally do. The capability belongs to
    /// the BACKEND, so a test picks it when it picks the session rather than
    /// reaching into a subscriber the executor owns.
    subscribers_dispatch_in_place: bool,
    /// Issue 1913 — `(name, type_hash, qos)` of every entity this session
    /// created, in order, so a test can assert which hash each endpoint was
    /// keyed on and (phase-515 R1) which profile it was asked for. Always
    /// recorded; the first 16 are kept.
    #[allow(clippy::type_complexity)]
    pub(crate) created:
        RefCell<heapless::Vec<(heapless::String<96>, heapless::String<80>, QoSProfile), 16>>,
    /// Issue 1944 — the timeout the most recent `drive_io` was ASKED for, so a
    /// test asserts what the executor requested of the transport instead of
    /// timing how long a no-op took on a host that may be loaded.
    pub(crate) last_drive_timeout_ms: Option<i32>,
}

impl MockSession {
    /// Issue 1913 — record one created entity (see the `created` field).
    fn record_created(&self, name: &str, type_hash: &str, qos: QoSProfile) {
        let mut n = heapless::String::new();
        let mut h = heapless::String::new();
        let _ = n.push_str(name);
        let _ = h.push_str(type_hash);
        let _ = self.created.borrow_mut().push((n, h, qos));
    }

    /// The type hash the entity named `name` was created with, if one was.
    pub fn created_hash(&self, name: &str) -> Option<heapless::String<80>> {
        self.created
            .borrow()
            .iter()
            .find(|(n, _, _)| n.as_str() == name)
            .map(|(_, h, _)| h.clone())
    }

    /// phase-515 R1 — the QoS the entity named `name` was REQUESTED with
    /// (what the executor handed the backend), if one was created.
    pub fn created_qos(&self, name: &str) -> Option<QoSProfile> {
        self.created
            .borrow()
            .iter()
            .find(|(n, _, _)| n.as_str() == name)
            .map(|(_, _, q)| *q)
    }

    pub fn new() -> Self {
        Self {
            close_observer: None,
            service_create_error: None,
            service_create_attempts: None,
            graph: false,
            subscribers_dispatch_in_place: false,
            created: RefCell::new(heapless::Vec::new()),
            last_drive_timeout_ms: None,
        }
    }

    /// phase-456 W8 — a session whose subscribers advertise in-place dispatch,
    /// which is what a zenoh or XRCE image has. See the field doc.
    pub fn with_in_place_dispatch() -> Self {
        Self {
            subscribers_dispatch_in_place: true,
            ..Self::new()
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
            subscribers_dispatch_in_place: false,
            created: RefCell::new(heapless::Vec::new()),
            last_drive_timeout_ms: None,
        }
    }

    /// A session whose `close()` bumps `observer`. See the field doc.
    pub fn with_close_observer(observer: &'static core::sync::atomic::AtomicUsize) -> Self {
        Self {
            close_observer: Some(observer),
            service_create_error: None,
            service_create_attempts: None,
            graph: false,
            subscribers_dispatch_in_place: false,
            created: RefCell::new(heapless::Vec::new()),
            last_drive_timeout_ms: None,
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
            subscribers_dispatch_in_place: false,
            created: RefCell::new(heapless::Vec::new()),
            last_drive_timeout_ms: None,
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
    /// `count_clients` (issue 1978). Distinct from every other count so a
    /// forwarder wired to the wrong counter fails.
    pub const CLIENTS: usize = 11;
    /// `count_services` (issue 1978).
    pub const SERVICES: usize = 13;

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

    /// `get_endpoint_info_by_topic(publishers = true)` — the node it reports.
    /// Distinct from [`ENDPOINT_SUBSCRIBER_NODE`] so a forwarder that hands
    /// the slot the wrong side fails; that boolean is the whole difference
    /// between the two entry points.
    pub const ENDPOINT_PUBLISHER_NODE: &str = "/endpoint/publisher_node";
    /// See [`ENDPOINT_PUBLISHER_NODE`].
    pub const ENDPOINT_SUBSCRIBER_NODE: &str = "/endpoint/subscriber_node";
    /// The type the publisher side reports. Also distinct, so a test that
    /// asserts on the type alone still separates the two sides.
    pub const ENDPOINT_PUBLISHER_TYPE: &str = "test_msgs/msg/PublisherSide";
    /// See [`ENDPOINT_PUBLISHER_TYPE`].
    pub const ENDPOINT_SUBSCRIBER_TYPE: &str = "test_msgs/msg/SubscriberSide";
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
        topic: &TopicInfo,
        qos: QoSProfile,
    ) -> Result<MockPublisher, TransportError> {
        self.record_created(topic.name, topic.type_hash, qos);
        Ok(MockPublisher::new_named(topic.name))
    }

    fn create_subscription(
        &mut self,
        topic: &TopicInfo,
        qos: QoSProfile,
    ) -> Result<MockSubscriber, TransportError> {
        self.record_created(topic.name, topic.type_hash, qos);
        let sub = MockSubscriber::new_named(topic.name);
        sub.set_in_place(self.subscribers_dispatch_in_place);
        Ok(sub)
    }

    fn create_service(
        &mut self,
        service: &ServiceInfo,
        qos: QoSProfile,
    ) -> Result<MockServiceServer, TransportError> {
        if let Some(attempts) = self.service_create_attempts {
            attempts.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
        }
        if let Some(e) = self.service_create_error.as_ref() {
            return Err(e.clone());
        }
        self.record_created(service.name, service.type_hash, qos);
        Ok(MockServiceServer::new_named(service.name))
    }

    fn create_client(
        &mut self,
        service: &ServiceInfo,
        qos: QoSProfile,
    ) -> Result<MockServiceClient, TransportError> {
        self.record_created(service.name, service.type_hash, qos);
        Ok(MockServiceClient::new_named(service.name))
    }

    fn close(&mut self) -> Result<(), TransportError> {
        if let Some(observer) = self.close_observer {
            observer.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
        }
        Ok(())
    }

    fn drive_io(&mut self, timeout_ms: i32) -> Result<(), TransportError> {
        // Mock transport: no I/O to drive. Only the request is recorded.
        self.last_drive_timeout_ms = Some(timeout_ms);
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

    fn count_clients(&mut self, _service_name: &str) -> Result<usize, Self::Error> {
        if !self.graph {
            return Err(TransportError::Unsupported);
        }
        Ok(canned::CLIENTS)
    }

    fn count_services(&mut self, _service_name: &str) -> Result<usize, Self::Error> {
        if !self.graph {
            return Err(TransportError::Unsupported);
        }
        Ok(canned::SERVICES)
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

    /// phase-444 — the endpoints on a topic, ONE per side, keyed on the
    /// `publishers` flag.
    ///
    /// The point of the canned answer is that the two sides are
    /// DISTINGUISHABLE: the defect a forwarder of this shape has is handing
    /// the slot the wrong boolean, and a fixture that answers the same row
    /// either way cannot see it. The emitted `is_publisher` echoes the flag,
    /// and `node_namespace` echoes the topic, so a forwarder that drops or
    /// swaps an argument fails too.
    fn get_endpoint_info_by_topic(
        &mut self,
        publishers: bool,
        topic_name: &str,
        visit: &mut dyn FnMut(&nros_rmw::GraphEndpointInfo<'_>) -> bool,
    ) -> Result<(), Self::Error> {
        if !self.graph {
            return Err(TransportError::Unsupported);
        }
        let (node_name, topic_type) = if publishers {
            (
                canned::ENDPOINT_PUBLISHER_NODE,
                canned::ENDPOINT_PUBLISHER_TYPE,
            )
        } else {
            (
                canned::ENDPOINT_SUBSCRIBER_NODE,
                canned::ENDPOINT_SUBSCRIBER_TYPE,
            )
        };
        let info = nros_rmw::GraphEndpointInfo {
            node_name,
            node_namespace: topic_name,
            topic_type,
            is_publisher: publishers,
            endpoint_gid: [0u8; 24],
            qos: nros_rmw::QoSProfile::QOS_PROFILE_UNKNOWN,
        };
        let _ = visit(&info);
        Ok(())
    }
}

/// A lent slot: an owned record of `len` bytes that `commit_slot` publishes.
///
/// Without these two impls a `rmw-lending` build's TEST target did not
/// compile — the lending path names `SlotLending` / `SlotBorrowing` on the
/// concrete session's types, and under `cfg(test)` that session is this mock —
/// so the path was reachable from no unit test, only through `rmw-cffi`.
#[cfg(feature = "rmw-lending")]
pub struct MockSlot {
    buf: [u8; MOCK_PUBLISH_RECORD],
    len: usize,
}

#[cfg(feature = "rmw-lending")]
impl AsMut<[u8]> for MockSlot {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.buf[..self.len]
    }
}

#[cfg(feature = "rmw-lending")]
impl nros_rmw::SlotLending for MockPublisher {
    type Slot<'a> = MockSlot;

    fn try_lend_slot(&self, len: usize) -> Result<Option<MockSlot>, TransportError> {
        // A slot larger than the record can never be lent, so the answer is
        // permanent and an error — `Ok(None)` would read as "retry" (issue 0814).
        if len > MOCK_PUBLISH_RECORD {
            return Err(TransportError::MessageTooLarge);
        }
        Ok(Some(MockSlot {
            buf: [0u8; MOCK_PUBLISH_RECORD],
            len,
        }))
    }

    fn commit_slot(&self, slot: MockSlot) -> Result<(), TransportError> {
        self.publish_raw(&slot.buf[..slot.len])
    }
}

/// A borrowed view: the popped sample, owned, so nothing outlives the take.
#[cfg(feature = "rmw-lending")]
pub struct MockView {
    data: [u8; 256],
    len: usize,
}

#[cfg(feature = "rmw-lending")]
impl AsRef<[u8]> for MockView {
    fn as_ref(&self) -> &[u8] {
        &self.data[..self.len]
    }
}

#[cfg(feature = "rmw-lending")]
impl nros_rmw::SlotBorrowing for MockSubscriber {
    type View<'a> = MockView;

    fn try_borrow(&mut self) -> Result<Option<MockView>, TransportError> {
        match self.queue.borrow_mut().pop_front() {
            Some(Ok((data, len))) => Ok(Some(MockView { data, len })),
            Some(Err(err)) => Err(err),
            None => Ok(None),
        }
    }
}

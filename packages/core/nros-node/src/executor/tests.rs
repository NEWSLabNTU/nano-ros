use super::*;
use core::cell::Cell;
use nros_core::{
    CdrReader, CdrWriter, DeserError, Deserialize, RosAction, RosMessage, SerError, Serialize,
};
use nros_rmw::{
    Publisher, QosSettings, ServiceClientTrait, ServiceInfo, ServiceRequest, ServiceServerTrait,
    Session, Subscriber, TopicInfo, TransportError,
};

use crate::timer::TimerDuration;

#[test]
fn test_error_conversion() {
    let transport_err = TransportError::ConnectionFailed;
    let node_err: NodeError = transport_err.into();
    assert_eq!(
        node_err,
        NodeError::Transport(TransportError::ConnectionFailed)
    );
}

// ====================================================================
// Mock types for arena callback tests
// ====================================================================

/// Simple test message: a single i32.
#[derive(Debug, Clone, PartialEq)]
struct TestMsg {
    data: i32,
}

impl RosMessage for TestMsg {
    const TYPE_NAME: &'static str = "test/msg/TestMsg";
    const TYPE_HASH: &'static str = "test_hash";
}

impl Serialize for TestMsg {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_i32(self.data)
    }
}

impl Deserialize for TestMsg {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            data: reader.read_i32()?,
        })
    }
}

/// CDR-encode a TestMsg(value) including CDR header.
fn encode_test_msg(value: i32) -> ([u8; 256], usize) {
    let mut buf = [0u8; 256];
    let mut writer = CdrWriter::new_with_header(&mut buf).unwrap();
    writer.write_i32(value).unwrap();
    let len = writer.position();
    (buf, len)
}

/// Mock subscriber that can be loaded with canned CDR data.
struct MockSubscriber {
    /// Pre-encoded data to return on the next `try_recv_raw` call.
    pending: Cell<Option<([u8; 256], usize)>>,
}

impl MockSubscriber {
    fn new() -> Self {
        Self {
            pending: Cell::new(None),
        }
    }

    fn load(&self, data: [u8; 256], len: usize) {
        self.pending.set(Some((data, len)));
    }
}

impl Subscriber for MockSubscriber {
    type Error = TransportError;

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

/// Mock service server (not used for service tests yet, but needed for Session).
struct MockServiceServer;

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

/// Dummy publisher (never used in callback tests).
struct MockPublisher;

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

/// Dummy service client.
struct MockServiceClient;

impl ServiceClientTrait for MockServiceClient {
    type Error = TransportError;

    fn call_raw(&mut self, _req: &[u8], _reply_buf: &mut [u8]) -> Result<usize, TransportError> {
        Err(TransportError::Timeout)
    }
}

/// Mock session that produces mock handles.
struct MockSession;

impl MockSession {
    fn new() -> Self {
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
        Ok(MockServiceClient)
    }

    fn close(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
}

// ====================================================================
// Arena callback tests
// ====================================================================

#[test]
fn test_add_subscription_and_spin_once_no_data() {
    let session = MockSession::new();
    let mut executor: Executor<MockSession, 4, 4096> = Executor::from_session(session);

    // Register a subscription — callback should never fire
    let called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let called2 = called.clone();
    executor
        .add_subscription::<TestMsg, _>("/test", move |_msg: &TestMsg| {
            called2.store(true, std::sync::atomic::Ordering::SeqCst);
        })
        .unwrap();

    let result = executor.spin_once(0);
    assert_eq!(result.subscriptions_processed, 0);
    assert!(!result.any_work());
    assert!(!called.load(std::sync::atomic::Ordering::SeqCst));
}

#[test]
fn test_add_subscription_and_spin_once_with_data() {
    let session = MockSession::new();
    let mut executor: Executor<MockSession, 4, 4096> = Executor::from_session(session);

    let received = std::sync::Arc::new(std::sync::Mutex::new(None));
    let received2 = received.clone();
    executor
        .add_subscription::<TestMsg, _>("/test", move |msg: &TestMsg| {
            *received2.lock().unwrap() = Some(msg.data);
        })
        .unwrap();

    // Grab a pointer to the subscriber in the arena so we can load data.
    // The subscriber is stored inside the SubEntry in the arena.
    // We need to reach it through the arena.
    let meta = executor.entries[0].as_ref().unwrap();
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    let sub_ptr = unsafe { arena_ptr.add(meta.offset) } as *const MockSubscriber;

    // Load CDR-encoded TestMsg(42) into the subscriber
    let (data, len) = encode_test_msg(42);
    unsafe { &*sub_ptr }.load(data, len);

    let result = executor.spin_once(0);
    assert_eq!(result.subscriptions_processed, 1);
    assert!(result.any_work());
    assert_eq!(*received.lock().unwrap(), Some(42));
}

#[test]
fn test_multiple_subscriptions() {
    let session = MockSession::new();
    let mut executor: Executor<MockSession, 4, 8192> = Executor::from_session(session);

    let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count1 = count.clone();
    let count2 = count.clone();

    executor
        .add_subscription::<TestMsg, _>("/topic1", move |_msg: &TestMsg| {
            count1.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })
        .unwrap();

    executor
        .add_subscription::<TestMsg, _>("/topic2", move |_msg: &TestMsg| {
            count2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })
        .unwrap();

    // Load data into both subscribers
    let (data, len) = encode_test_msg(10);
    let meta0 = executor.entries[0].as_ref().unwrap();
    let meta1 = executor.entries[1].as_ref().unwrap();
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    unsafe { &*(arena_ptr.add(meta0.offset) as *const MockSubscriber) }.load(data, len);
    let (data2, len2) = encode_test_msg(20);
    unsafe { &*(arena_ptr.add(meta1.offset) as *const MockSubscriber) }.load(data2, len2);

    let result = executor.spin_once(0);
    assert_eq!(result.subscriptions_processed, 2);
    assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 2);
}

#[test]
fn test_arena_overflow() {
    let session = MockSession::new();
    // Tiny arena — one SubEntry<TestMsg, MockSubscriber, fn, 1024> is ~1040+ bytes
    let mut executor: Executor<MockSession, 4, 128> = Executor::from_session(session);

    let result = executor.add_subscription::<TestMsg, _>("/test", |_msg: &TestMsg| {});
    assert_eq!(result, Err(NodeError::BufferTooSmall));
}

#[test]
fn test_entry_slots_exhausted() {
    let session = MockSession::new();
    // 1 entry slot but plenty of arena space
    let mut executor: Executor<MockSession, 1, 8192> = Executor::from_session(session);

    executor
        .add_subscription::<TestMsg, _>("/a", |_msg: &TestMsg| {})
        .unwrap();

    let result = executor.add_subscription::<TestMsg, _>("/b", |_msg: &TestMsg| {});
    assert_eq!(result, Err(NodeError::BufferTooSmall));
}

#[test]
fn test_spin_once_result_counts() {
    let result = SpinOnceResult::new();
    assert!(!result.any_work());
    assert!(!result.any_errors());
    assert_eq!(result.total(), 0);
    assert_eq!(result.total_errors(), 0);

    let result = SpinOnceResult {
        subscriptions_processed: 2,
        timers_fired: 1,
        services_handled: 1,
        subscription_errors: 0,
        service_errors: 0,
    };
    assert!(result.any_work());
    assert!(!result.any_errors());
    assert_eq!(result.total(), 4);
}

#[test]
fn test_drop_runs_without_panic() {
    let session = MockSession::new();
    let mut executor: Executor<MockSession, 4, 4096> = Executor::from_session(session);

    executor
        .add_subscription::<TestMsg, _>("/test", |_msg: &TestMsg| {})
        .unwrap();

    // executor drops here — Drop impl must not panic
}

#[test]
fn test_zero_sized_executor_spin_once() {
    // Default const generics: MAX_CBS=0, CB_ARENA=0
    let session = MockSession::new();
    let mut executor: Executor<MockSession, 0, 0> = Executor::from_session(session);

    // spin_once with no entries just calls drive_io
    let result = executor.spin_once(0);
    assert!(!result.any_work());
}

#[test]
fn test_arena_alignment() {
    let session = MockSession::new();
    let mut executor: Executor<MockSession, 4, 8192> = Executor::from_session(session);

    // Add a subscription, then check the offset is properly aligned
    executor
        .add_subscription::<TestMsg, _>("/test", |_msg: &TestMsg| {})
        .unwrap();

    let meta = executor.entries[0].as_ref().unwrap();
    let entry_align =
        core::mem::align_of::<arena::SubEntry<TestMsg, MockSubscriber, fn(&TestMsg), 1024>>();
    assert_eq!(meta.offset % entry_align, 0);
}

// ====================================================================
// Timer callback tests
// ====================================================================

#[test]
fn test_add_timer_and_fire() {
    let session = MockSession::new();
    let mut executor: Executor<MockSession, 4, 4096> = Executor::from_session(session);

    let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count2 = count.clone();
    executor
        .add_timer(TimerDuration::from_millis(100), move || {
            count2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })
        .unwrap();

    // Not enough time elapsed — should not fire
    let result = executor.spin_once(50);
    assert_eq!(result.timers_fired, 0);
    assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 0);

    // Now enough time elapsed (50 + 60 = 110 >= 100)
    let result = executor.spin_once(60);
    assert_eq!(result.timers_fired, 1);
    assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[test]
fn test_timer_repeats() {
    let session = MockSession::new();
    let mut executor: Executor<MockSession, 4, 4096> = Executor::from_session(session);

    let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count2 = count.clone();
    executor
        .add_timer(TimerDuration::from_millis(100), move || {
            count2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })
        .unwrap();

    // Fire 3 times
    let _ = executor.spin_once(100);
    let _ = executor.spin_once(100);
    let _ = executor.spin_once(100);
    assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 3);
}

#[test]
fn test_timer_oneshot_fires_once() {
    let session = MockSession::new();
    let mut executor: Executor<MockSession, 4, 4096> = Executor::from_session(session);

    let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count2 = count.clone();
    executor
        .add_timer_oneshot(TimerDuration::from_millis(50), move || {
            count2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })
        .unwrap();

    // First spin fires
    let result = executor.spin_once(60);
    assert_eq!(result.timers_fired, 1);
    assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);

    // Second spin should NOT fire again
    let result = executor.spin_once(60);
    assert_eq!(result.timers_fired, 0);
    assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[test]
fn test_timer_does_not_fire_at_zero_delta() {
    let session = MockSession::new();
    let mut executor: Executor<MockSession, 4, 4096> = Executor::from_session(session);

    let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count2 = count.clone();
    executor
        .add_timer(TimerDuration::from_millis(100), move || {
            count2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })
        .unwrap();

    // Zero delta should never fire
    let result = executor.spin_once(0);
    assert_eq!(result.timers_fired, 0);
}

#[test]
fn test_timer_with_subscriptions() {
    let session = MockSession::new();
    let mut executor: Executor<MockSession, 4, 8192> = Executor::from_session(session);

    let timer_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let timer_count2 = timer_count.clone();
    executor
        .add_timer(TimerDuration::from_millis(100), move || {
            timer_count2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })
        .unwrap();

    let sub_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let sub_count2 = sub_count.clone();
    executor
        .add_subscription::<TestMsg, _>("/test", move |_msg: &TestMsg| {
            sub_count2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })
        .unwrap();

    // Load data into subscription
    let (data, len) = encode_test_msg(99);
    let meta1 = executor.entries[1].as_ref().unwrap();
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    unsafe { &*(arena_ptr.add(meta1.offset) as *const MockSubscriber) }.load(data, len);

    let result = executor.spin_once(100);
    assert_eq!(result.timers_fired, 1);
    assert_eq!(result.subscriptions_processed, 1);
    assert_eq!(timer_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(sub_count.load(std::sync::atomic::Ordering::SeqCst), 1);
}

// ====================================================================
// Action types for testing
// ====================================================================

#[derive(Debug, Clone, Default, PartialEq)]
struct TestGoal {
    order: i32,
}

impl RosMessage for TestGoal {
    const TYPE_NAME: &'static str = "test/action/TestAction_Goal";
    const TYPE_HASH: &'static str = "test_hash";
}

impl Serialize for TestGoal {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_i32(self.order)
    }
}

impl Deserialize for TestGoal {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            order: reader.read_i32()?,
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
struct TestResult {
    value: i32,
}

impl RosMessage for TestResult {
    const TYPE_NAME: &'static str = "test/action/TestAction_Result";
    const TYPE_HASH: &'static str = "test_hash";
}

impl Serialize for TestResult {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_i32(self.value)
    }
}

impl Deserialize for TestResult {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            value: reader.read_i32()?,
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
struct TestFeedback {
    progress: i32,
}

impl RosMessage for TestFeedback {
    const TYPE_NAME: &'static str = "test/action/TestAction_Feedback";
    const TYPE_HASH: &'static str = "test_hash";
}

impl Serialize for TestFeedback {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_i32(self.progress)
    }
}

impl Deserialize for TestFeedback {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            progress: reader.read_i32()?,
        })
    }
}

struct TestAction;

impl RosAction for TestAction {
    type Goal = TestGoal;
    type Result = TestResult;
    type Feedback = TestFeedback;
    const ACTION_NAME: &'static str = "test/action/dds_/TestAction_";
    const ACTION_HASH: &'static str = "test_hash";
}

// ====================================================================
// Action server tests
// ====================================================================

#[test]
fn test_add_action_server_registers() {
    let session = MockSession::new();
    // Action server arena entry is large — give plenty of space
    let mut executor: Executor<MockSession, 4, 16384> = Executor::from_session(session);

    let handle = executor
        .add_action_server::<TestAction, _, _>(
            "/test_action",
            |_goal: &TestGoal| nros_core::GoalResponse::AcceptAndExecute,
            |_id: &nros_core::GoalId, _status: nros_core::GoalStatus| nros_core::CancelResponse::Ok,
        )
        .unwrap();

    // Verify the entry was registered
    assert!(executor.entries[0].is_some());
    assert_eq!(handle.entry_index, 0);
}

#[test]
fn test_action_server_spin_once_no_requests() {
    let session = MockSession::new();
    let mut executor: Executor<MockSession, 4, 16384> = Executor::from_session(session);

    let _handle = executor
        .add_action_server::<TestAction, _, _>(
            "/test_action",
            |_goal: &TestGoal| nros_core::GoalResponse::AcceptAndExecute,
            |_id: &nros_core::GoalId, _status: nros_core::GoalStatus| nros_core::CancelResponse::Ok,
        )
        .unwrap();

    // With no pending requests, spin_once should return no work
    let result = executor.spin_once(10);
    assert_eq!(result.services_handled, 0);
    assert!(!result.any_work());
}

// ====================================================================
// Action client tests
// ====================================================================

#[test]
fn test_add_action_client_registers() {
    let session = MockSession::new();
    let mut executor: Executor<MockSession, 4, 16384> = Executor::from_session(session);

    let handle = executor
        .add_action_client::<TestAction, _>(
            "/test_action",
            |_id: &nros_core::GoalId, _feedback: &TestFeedback| {},
        )
        .unwrap();

    assert!(executor.entries[0].is_some());
    assert_eq!(handle.entry_index, 0);
}

#[test]
fn test_action_client_spin_once_no_feedback() {
    let session = MockSession::new();
    let mut executor: Executor<MockSession, 4, 16384> = Executor::from_session(session);

    let _handle = executor
        .add_action_client::<TestAction, _>(
            "/test_action",
            |_id: &nros_core::GoalId, _feedback: &TestFeedback| {},
        )
        .unwrap();

    let result = executor.spin_once(10);
    assert_eq!(result.subscriptions_processed, 0);
    assert!(!result.any_work());
}

#[test]
fn test_action_server_and_client_coexist() {
    let session = MockSession::new();
    let mut executor: Executor<MockSession, 8, 65536> = Executor::from_session(session);

    let _server_handle = executor
        .add_action_server::<TestAction, _, _>(
            "/test_action",
            |_goal: &TestGoal| nros_core::GoalResponse::AcceptAndExecute,
            |_id: &nros_core::GoalId, _status: nros_core::GoalStatus| nros_core::CancelResponse::Ok,
        )
        .unwrap();

    let _client_handle = executor
        .add_action_client::<TestAction, _>(
            "/test_action",
            |_id: &nros_core::GoalId, _feedback: &TestFeedback| {},
        )
        .unwrap();

    // Both registered
    assert!(executor.entries[0].is_some());
    assert!(executor.entries[1].is_some());

    let result = executor.spin_once(10);
    assert!(!result.any_work());
}

#[test]
fn test_drop_with_mixed_entries() {
    let session = MockSession::new();
    let mut executor: Executor<MockSession, 8, 65536> = Executor::from_session(session);

    // Register one of each kind
    executor
        .add_subscription::<TestMsg, _>("/sub", |_msg: &TestMsg| {})
        .unwrap();
    executor
        .add_timer(TimerDuration::from_millis(100), || {})
        .unwrap();
    let _server = executor
        .add_action_server::<TestAction, _, _>(
            "/act",
            |_goal: &TestGoal| nros_core::GoalResponse::AcceptAndExecute,
            |_id: &nros_core::GoalId, _status: nros_core::GoalStatus| nros_core::CancelResponse::Ok,
        )
        .unwrap();
    let _client = executor
        .add_action_client::<TestAction, _>(
            "/act",
            |_id: &nros_core::GoalId, _fb: &TestFeedback| {},
        )
        .unwrap();

    // Drop must clean up all 4 entries without panicking
}

// ====================================================================
// spin_one_period tests (no_std)
// ====================================================================

#[test]
fn test_spin_one_period_remaining_time() {
    let session = MockSession::new();
    let mut executor: Executor<MockSession, 4, 4096> = Executor::from_session(session);

    // elapsed < period → remaining = period - elapsed
    let r = executor.spin_one_period(100, 30);
    assert_eq!(r.remaining_ms, 70);
    assert_eq!(r.work.total(), 0);
}

#[test]
fn test_spin_one_period_overrun() {
    let session = MockSession::new();
    let mut executor: Executor<MockSession, 4, 4096> = Executor::from_session(session);

    // elapsed > period → remaining saturates to 0
    let r = executor.spin_one_period(10, 50);
    assert_eq!(r.remaining_ms, 0);
}

#[test]
fn test_spin_one_period_exact() {
    let session = MockSession::new();
    let mut executor: Executor<MockSession, 4, 4096> = Executor::from_session(session);

    // elapsed == period → remaining = 0
    let r = executor.spin_one_period(42, 42);
    assert_eq!(r.remaining_ms, 0);
}

#[test]
fn test_spin_options_default() {
    let opts = SpinOptions::default();
    assert!(opts.timeout_ms.is_none());
    assert!(!opts.only_next);
    assert!(opts.max_callbacks.is_none());
}

#[test]
fn test_spin_options_builders() {
    let opts = SpinOptions::new().timeout_ms(5000).max_callbacks(10);
    assert_eq!(opts.timeout_ms, Some(5000));
    assert_eq!(opts.max_callbacks, Some(10));
    assert!(!opts.only_next);

    let opts_once = SpinOptions::spin_once();
    assert!(opts_once.only_next);
}

// ====================================================================
// std-gated spin tests
// ====================================================================

#[test]
fn test_spin_blocking_only_next() {
    let session = MockSession::new();
    let mut executor: Executor<MockSession, 4, 4096> = Executor::from_session(session);

    // only_next exits after single iteration
    let result = executor.spin_blocking(SpinOptions::spin_once());
    assert!(result.is_ok());
}

#[test]
fn test_spin_blocking_halt() {
    let session = MockSession::new();
    let mut executor: Executor<MockSession, 4, 4096> = Executor::from_session(session);

    // Pre-set halt flag → exits immediately
    executor.halt();
    assert!(executor.is_halted());

    // spin_blocking resets halt then checks it — so we need a thread
    let halt = executor.halt_flag();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(50));
        halt.store(true, std::sync::atomic::Ordering::SeqCst);
    });
    let result = executor.spin_blocking(SpinOptions::default());
    assert!(result.is_ok());
}

#[test]
fn test_spin_blocking_timeout() {
    let session = MockSession::new();
    let mut executor: Executor<MockSession, 4, 4096> = Executor::from_session(session);

    let start = std::time::Instant::now();
    let result = executor.spin_blocking(SpinOptions::new().timeout_ms(50));
    assert!(result.is_ok());
    // Should exit within a reasonable time after 50ms timeout
    assert!(start.elapsed() < std::time::Duration::from_secs(2));
}

#[test]
fn test_spin_one_period_timed_no_overrun() {
    let session = MockSession::new();
    let mut executor: Executor<MockSession, 4, 4096> = Executor::from_session(session);

    let period = std::time::Duration::from_millis(50);
    let result = executor.spin_one_period_timed(period);
    // Mock session returns instantly, so no overrun
    assert!(!result.overrun);
    assert_eq!(result.work.total(), 0);
}

#[test]
fn test_halt_flag_clone() {
    let session = MockSession::new();
    let executor: Executor<MockSession, 4, 4096> = Executor::from_session(session);

    let flag = executor.halt_flag();
    assert!(!executor.is_halted());

    flag.store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(executor.is_halted());
}

#[test]
fn test_spin_period_halts() {
    let session = MockSession::new();
    let mut executor: Executor<MockSession, 4, 4096> = Executor::from_session(session);

    let halt = executor.halt_flag();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(50));
        halt.store(true, std::sync::atomic::Ordering::SeqCst);
    });

    let result = executor.spin_period(std::time::Duration::from_millis(10));
    assert!(result.is_ok());
}

use super::*;
use nros_core::{
    CdrReader, CdrWriter, DeserError, Deserialize, RosAction, RosMessage, SerError, Serialize,
};
use nros_rmw::TransportError;

use crate::{
    mock::{MockSession, MockSubscriber},
    timer::TimerDuration,
};

/// Sleep `ms` then call `spin_once(0)`. Phase 100 follow-up: spin_once
/// credits the wall-clock since the previous `spin_once` exited (not the
/// requested timeout) to the timer accumulator. Tests that previously
/// relied on `spin_once(N ms)` advancing virtual time by N must now
/// elapse real wall-clock time between calls. MockSession's `drive_io`
/// is a no-op, so the requested timeout adds no real elapsed.
#[cfg(feature = "std")]
fn elapse_then_spin_once(executor: &mut Executor, ms: u64) -> super::types::SpinOnceResult {
    std::thread::sleep(std::time::Duration::from_millis(ms));
    executor.spin_once(core::time::Duration::from_millis(0))
}

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

// ====================================================================
// Arena callback tests
// ====================================================================

#[test]
fn test_add_subscription_and_spin_once_no_data() {
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    // Register a subscription — callback should never fire
    let called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let called2 = called.clone();
    executor
        .add_subscription::<TestMsg, _>("/test", move |_msg: &TestMsg| {
            called2.store(true, std::sync::atomic::Ordering::SeqCst);
        })
        .unwrap();

    let result = executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(result.subscriptions_processed, 0);
    assert!(!result.any_work());
    assert!(!called.load(std::sync::atomic::Ordering::SeqCst));
}

#[test]
fn test_add_subscription_and_spin_once_with_data() {
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    let received = std::sync::Arc::new(std::sync::Mutex::new(None));
    let received2 = received.clone();
    executor
        .add_subscription::<TestMsg, _>("/test", move |msg: &TestMsg| {
            *received2.lock().unwrap() = Some(msg.data);
        })
        .unwrap();

    // Grab a pointer to the subscriber in the arena so we can load data.
    // The subscriber is stored inside the SubBufferedEntry in the arena.
    // We need to reach it through the arena.
    let meta = executor.entries[0].as_ref().unwrap();
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    let sub_ptr = unsafe { arena_ptr.add(meta.offset) } as *const MockSubscriber;

    // Load CDR-encoded TestMsg(42) into the subscriber
    let (data, len) = encode_test_msg(42);
    unsafe { &*sub_ptr }.load(data, len);

    let result = executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(result.subscriptions_processed, 1);
    assert!(result.any_work());
    assert_eq!(*received.lock().unwrap(), Some(42));
}

#[test]
fn test_multiple_subscriptions() {
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

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

    let result = executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(result.subscriptions_processed, 2);
    assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 2);
}

#[test]
fn test_arena_overflow() {
    let session = MockSession::new();
    // Arena is ARENA_SIZE bytes (default ~10KB). Use large subscription buffers
    // (4096 each) so we exhaust the arena before running out of entry slots.
    // Each SubBufferedEntry with triple buffer (3×4096=12KB) is ~12.1KB. At least one
    // fits in the arena but not many — arena should exhaust before 4 subscriptions.
    let mut executor = Executor::from_session(session);

    let topics = ["/a", "/b", "/c", "/d"];
    let mut filled = 0;
    for topic in &topics {
        let result =
            executor.add_subscription_sized::<TestMsg, _, 4096>(topic, |_msg: &TestMsg| {});
        if result.is_err() {
            break;
        }
        filled += 1;
    }

    // We should have been able to add at least 1 but not all 4 (arena too small).
    assert!(filled >= 1, "Should fit at least 1 large subscription");
    assert!(
        filled < 4,
        "Arena should overflow before 4 large subscriptions"
    );

    // Verify the next add fails with BufferTooSmall.
    let result =
        executor.add_subscription_sized::<TestMsg, _, 4096>("/overflow", |_msg: &TestMsg| {});
    assert_eq!(result, Err(NodeError::BufferTooSmall));
}

#[test]
fn test_entry_slots_exhausted() {
    let session = MockSession::new();
    // MAX_CBS=4 slots. Use small buffers to avoid arena overflow before
    // exhausting slots.
    let mut executor = Executor::from_session(session);

    for topic in &["/a", "/b", "/c", "/d"] {
        executor
            .add_subscription_sized::<TestMsg, _, 64>(topic, |_msg: &TestMsg| {})
            .unwrap();
    }

    // 5th registration should fail — all 4 slots are taken.
    let result = executor.add_subscription_sized::<TestMsg, _, 64>("/e", |_msg: &TestMsg| {});
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
    let mut executor: Executor = Executor::from_session(session);

    executor
        .add_subscription::<TestMsg, _>("/test", |_msg: &TestMsg| {})
        .unwrap();

    // executor drops here — Drop impl must not panic
}

#[test]
fn test_executor_spin_once_no_entries() {
    // Executor with no registered callbacks — spin_once just calls drive_io.
    let session = MockSession::new();
    let mut executor = Executor::from_session(session);

    let result = executor.spin_once(core::time::Duration::from_millis(0));
    assert!(!result.any_work());
}

#[test]
fn test_arena_alignment() {
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    // Add a subscription, then check the offset is properly aligned
    executor
        .add_subscription::<TestMsg, _>("/test", |_msg: &TestMsg| {})
        .unwrap();

    let meta = executor.entries[0].as_ref().unwrap();
    let entry_align = core::mem::align_of::<arena::SubBufferedEntry<TestMsg, fn(&TestMsg)>>();
    assert_eq!(meta.offset % entry_align, 0);
}

// ====================================================================
// Timer callback tests
// ====================================================================

#[test]
fn test_add_timer_and_fire() {
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count2 = count.clone();
    executor
        .add_timer(TimerDuration::from_millis(100), move || {
            count2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })
        .unwrap();

    // Not enough time elapsed — should not fire
    let result = elapse_then_spin_once(&mut executor, 50);
    assert_eq!(result.timers_fired, 0);
    assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 0);

    // Now enough time elapsed (50 + 60 = 110 >= 100)
    let result = elapse_then_spin_once(&mut executor, 60);
    assert_eq!(result.timers_fired, 1);
    assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[test]
fn test_timer_repeats() {
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count2 = count.clone();
    executor
        .add_timer(TimerDuration::from_millis(100), move || {
            count2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })
        .unwrap();

    // Fire 3 times
    let _ = elapse_then_spin_once(&mut executor, 100);
    let _ = elapse_then_spin_once(&mut executor, 100);
    let _ = elapse_then_spin_once(&mut executor, 100);
    assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 3);
}

#[test]
fn test_timer_oneshot_fires_once() {
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count2 = count.clone();
    executor
        .add_timer_oneshot(TimerDuration::from_millis(50), move || {
            count2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })
        .unwrap();

    // First spin fires
    let result = elapse_then_spin_once(&mut executor, 60);
    assert_eq!(result.timers_fired, 1);
    assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);

    // Second spin should NOT fire again
    let result = elapse_then_spin_once(&mut executor, 60);
    assert_eq!(result.timers_fired, 0);
    assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[test]
fn test_timer_does_not_fire_at_zero_delta() {
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count2 = count.clone();
    executor
        .add_timer(TimerDuration::from_millis(100), move || {
            count2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })
        .unwrap();

    // Zero delta should never fire
    let result = executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(result.timers_fired, 0);
}

#[test]
fn test_timer_with_subscriptions() {
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

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

    let result = elapse_then_spin_once(&mut executor, 100);
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
    // Use small buffers to fit within the 4096-byte arena.
    let mut executor = Executor::from_session(session);

    let handle = executor
        .add_action_server_sized::<TestAction, _, _, 64, 64, 64, 1>(
            "/test_action",
            |_goal_id, _goal: &TestGoal| nros_core::GoalResponse::AcceptAndExecute,
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
    let mut executor = Executor::from_session(session);

    let _handle = executor
        .add_action_server_sized::<TestAction, _, _, 64, 64, 64, 1>(
            "/test_action",
            |_goal_id, _goal: &TestGoal| nros_core::GoalResponse::AcceptAndExecute,
            |_id: &nros_core::GoalId, _status: nros_core::GoalStatus| nros_core::CancelResponse::Ok,
        )
        .unwrap();

    // With no pending requests, spin_once should return no work
    let result = executor.spin_once(core::time::Duration::from_millis(10));
    assert_eq!(result.services_handled, 0);
    assert!(!result.any_work());
}

#[test]
fn test_action_server_registers_and_spins() {
    let session = MockSession::new();
    let mut executor = Executor::from_session(session);

    let _server_handle = executor
        .add_action_server_sized::<TestAction, _, _, 64, 64, 64, 1>(
            "/test_action",
            |_goal_id, _goal: &TestGoal| nros_core::GoalResponse::AcceptAndExecute,
            |_id: &nros_core::GoalId, _status: nros_core::GoalStatus| nros_core::CancelResponse::Ok,
        )
        .unwrap();

    // Action server registered
    assert!(executor.entries[0].is_some());

    let result = executor.spin_once(core::time::Duration::from_millis(10));
    assert!(!result.any_work());
}

#[test]
fn test_drop_with_mixed_entries() {
    let session = MockSession::new();
    let mut executor = Executor::from_session(session);

    // Register one of each kind — use small buffers to fit in 4096-byte arena.
    executor
        .add_subscription_sized::<TestMsg, _, 64>("/sub", |_msg: &TestMsg| {})
        .unwrap();
    executor
        .add_timer(TimerDuration::from_millis(100), || {})
        .unwrap();
    let _server = executor
        .add_action_server_sized::<TestAction, _, _, 64, 64, 64, 1>(
            "/act",
            |_goal_id, _goal: &TestGoal| nros_core::GoalResponse::AcceptAndExecute,
            |_id: &nros_core::GoalId, _status: nros_core::GoalStatus| nros_core::CancelResponse::Ok,
        )
        .unwrap();

    // Drop must clean up all 3 entries without panicking
}

// ====================================================================
// spin_one_period tests (no_std)
// ====================================================================

#[test]
fn test_spin_one_period_remaining_time() {
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    // elapsed < period → remaining = period - elapsed
    let r = executor.spin_one_period(100, 30);
    assert_eq!(r.remaining_ms, 70);
    assert_eq!(r.work.total(), 0);
}

#[test]
fn test_spin_one_period_overrun() {
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    // elapsed > period → remaining saturates to 0
    let r = executor.spin_one_period(10, 50);
    assert_eq!(r.remaining_ms, 0);
}

#[test]
fn test_spin_one_period_exact() {
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

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
    let mut executor: Executor = Executor::from_session(session);

    // only_next exits after single iteration
    let result = executor.spin_blocking(SpinOptions::spin_once());
    assert!(result.is_ok());
}

#[test]
fn test_spin_blocking_halt() {
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

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
    let mut executor: Executor = Executor::from_session(session);

    let start = std::time::Instant::now();
    let result = executor.spin_blocking(SpinOptions::new().timeout_ms(50));
    assert!(result.is_ok());
    // Should exit within a reasonable time after 50ms timeout
    assert!(start.elapsed() < std::time::Duration::from_secs(2));
}

#[test]
fn test_spin_one_period_timed_no_overrun() {
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    let period = std::time::Duration::from_millis(50);
    let result = executor.spin_one_period_timed(period);
    // Mock session returns instantly, so no overrun
    assert!(!result.overrun);
    assert_eq!(result.work.total(), 0);
}

#[test]
fn test_halt_flag_clone() {
    let session = MockSession::new();
    let executor: Executor = Executor::from_session(session);

    let flag = executor.halt_flag();
    assert!(!executor.is_halted());

    flag.store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(executor.is_halted());
}

#[test]
fn test_spin_period_halts() {
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    let halt = executor.halt_flag();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(50));
        halt.store(true, std::sync::atomic::Ordering::SeqCst);
    });

    let result = executor.spin_period(std::time::Duration::from_millis(10));
    assert!(result.is_ok());
}

// ====================================================================
// Phase 49: HandleId / HandleSet / ReadinessSnapshot tests
// ====================================================================

#[test]
fn test_handle_id_from_add_subscription() {
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    let id = executor
        .add_subscription::<TestMsg, _>("/a", |_msg: &TestMsg| {})
        .unwrap();
    assert_eq!(id, HandleId(0));

    let id2 = executor
        .add_subscription::<TestMsg, _>("/b", |_msg: &TestMsg| {})
        .unwrap();
    assert_eq!(id2, HandleId(1));
}

#[test]
fn test_handle_set_operations() {
    let a = HandleId(0);
    let b = HandleId(1);
    let c = HandleId(5);

    let set = a | b;
    assert!(set.contains(a));
    assert!(set.contains(b));
    assert!(!set.contains(c));
    assert_eq!(set.len(), 2);

    let set2 = set | c;
    assert!(set2.contains(c));
    assert_eq!(set2.len(), 3);

    let empty = HandleSet::EMPTY;
    assert!(empty.is_empty());
    assert_eq!(empty.len(), 0);
}

#[test]
fn test_handle_set_union() {
    let set1 = HandleSet::EMPTY.insert(HandleId(0)).insert(HandleId(2));
    let set2 = HandleSet::EMPTY.insert(HandleId(1)).insert(HandleId(2));
    let union = set1 | set2;
    assert!(union.contains(HandleId(0)));
    assert!(union.contains(HandleId(1)));
    assert!(union.contains(HandleId(2)));
    assert_eq!(union.len(), 3);
}

#[test]
fn test_readiness_snapshot() {
    let snap = ReadinessSnapshot {
        bits: 0b101,
        count: 3,
    };
    assert!(snap.is_ready(HandleId(0)));
    assert!(!snap.is_ready(HandleId(1)));
    assert!(snap.is_ready(HandleId(2)));
    assert_eq!(snap.ready_count(), 2);
    assert_eq!(snap.total(), 3);

    let set = HandleId(0) | HandleId(2);
    assert!(snap.all_ready(set));
    assert!(snap.any_ready(set));

    let set2 = HandleId(0) | HandleId(1);
    assert!(!snap.all_ready(set2));
    assert!(snap.any_ready(set2));
}

// ====================================================================
// Phase 49: Trigger condition tests
// ====================================================================

#[test]
fn test_trigger_any_fires_on_data() {
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);
    executor.set_trigger(Trigger::Any);

    executor
        .add_subscription::<TestMsg, _>("/test", |_msg: &TestMsg| {})
        .unwrap();

    // Load data
    let (data, len) = encode_test_msg(1);
    let meta = executor.entries[0].as_ref().unwrap();
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    unsafe { &*(arena_ptr.add(meta.offset) as *const MockSubscriber) }.load(data, len);

    let result = executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(result.subscriptions_processed, 1);
}

#[test]
fn test_trigger_any_no_data_no_dispatch() {
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);
    executor.set_trigger(Trigger::Any);

    executor
        .add_subscription::<TestMsg, _>("/test", |_msg: &TestMsg| {})
        .unwrap();

    // No data loaded → trigger should not pass (for subscriptions)
    let result = executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(result.subscriptions_processed, 0);
}

#[test]
fn test_trigger_always_fires_without_data() {
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);
    executor.set_trigger(Trigger::Always);

    let called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let called2 = called.clone();
    let id = executor
        .add_subscription::<TestMsg, _>("/test", move |_msg: &TestMsg| {
            called2.store(true, std::sync::atomic::Ordering::SeqCst);
        })
        .unwrap();

    // Set invocation to Always so callback fires even without data
    executor.set_invocation(id, InvocationMode::Always);

    // No data, but trigger Always → dispatch phase runs, callback fires
    let _result = executor.spin_once(core::time::Duration::from_millis(0));
    // Subscription try_recv returns None, so subscriptions_processed stays 0
    // but the callback IS invoked (Always invocation) — try_process returns Ok(false)
    // because there's no actual data
    assert!(!called.load(std::sync::atomic::Ordering::SeqCst));
}

#[test]
fn test_trigger_one_fires_on_specific_handle() {
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    let _id0 = executor
        .add_subscription::<TestMsg, _>("/topic0", |_: &TestMsg| {})
        .unwrap();
    let id1 = executor
        .add_subscription::<TestMsg, _>("/topic1", |_: &TestMsg| {})
        .unwrap();

    executor.set_trigger(Trigger::One(id1));

    // Load data only on topic0 (not the trigger handle)
    let (data, len) = encode_test_msg(1);
    let meta0 = executor.entries[0].as_ref().unwrap();
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    unsafe { &*(arena_ptr.add(meta0.offset) as *const MockSubscriber) }.load(data, len);

    let result = executor.spin_once(core::time::Duration::from_millis(0));
    // Trigger requires handle 1 to have data, but only handle 0 does
    assert_eq!(result.subscriptions_processed, 0);

    // Now load data on topic1
    let (data2, len2) = encode_test_msg(2);
    let meta1 = executor.entries[1].as_ref().unwrap();
    unsafe { &*(arena_ptr.add(meta1.offset) as *const MockSubscriber) }.load(data2, len2);

    let result = executor.spin_once(core::time::Duration::from_millis(0));
    assert!(result.subscriptions_processed >= 1);
}

#[test]
fn test_trigger_predicate() {
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    executor
        .add_subscription::<TestMsg, _>("/test", |_: &TestMsg| {})
        .unwrap();

    // Custom predicate that requires at least 1 ready handle
    executor.set_trigger(Trigger::Predicate(|snap: &ReadinessSnapshot| {
        snap.ready_count() >= 1
    }));

    // No data → predicate returns false
    let result = executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(result.subscriptions_processed, 0);
}

// ====================================================================
// Phase 49: Guard condition tests
// ====================================================================

#[test]
fn test_guard_condition_trigger_fires_callback() {
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    let called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let called2 = called.clone();

    let (_id, handle) = executor
        .add_guard_condition(move || {
            called2.store(true, std::sync::atomic::Ordering::SeqCst);
        })
        .unwrap();

    // Not triggered yet
    let _result = executor.spin_once(core::time::Duration::from_millis(0));
    assert!(!called.load(std::sync::atomic::Ordering::SeqCst));

    // Trigger the guard condition
    handle.trigger();

    let _result = executor.spin_once(core::time::Duration::from_millis(0));
    assert!(called.load(std::sync::atomic::Ordering::SeqCst));
}

#[test]
fn test_guard_condition_clears_after_trigger() {
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count2 = count.clone();

    let (_id, handle) = executor
        .add_guard_condition(move || {
            count2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })
        .unwrap();

    // Trigger once
    handle.trigger();
    executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);

    // Without re-triggering, callback should not fire again
    executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);

    // Trigger again
    handle.trigger();
    executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 2);
}

// ====================================================================
// Phase 49: Raw subscription callback tests
// ====================================================================

#[test]
fn test_raw_subscription_callback() {
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    static RAW_CALLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    static RAW_LEN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    unsafe extern "C" fn raw_cb(_data: *const u8, len: usize, _context: *mut core::ffi::c_void) {
        RAW_CALLED.store(true, std::sync::atomic::Ordering::SeqCst);
        RAW_LEN.store(len, std::sync::atomic::Ordering::SeqCst);
    }

    RAW_CALLED.store(false, std::sync::atomic::Ordering::SeqCst);

    let _id = executor
        .add_subscription_raw(
            "/test",
            "test/msg/TestMsg",
            "test_hash",
            raw_cb,
            core::ptr::null_mut(),
        )
        .unwrap();

    // Load CDR data into the mock subscriber
    let (data, len) = encode_test_msg(99);
    let meta = executor.entries[0].as_ref().unwrap();
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    unsafe {
        let sub_ptr = arena_ptr.add(meta.offset) as *const MockSubscriber;
        (*sub_ptr).load(data, len);
    }

    let result = executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(result.subscriptions_processed, 1);
    assert!(RAW_CALLED.load(std::sync::atomic::Ordering::SeqCst));
    assert_eq!(RAW_LEN.load(std::sync::atomic::Ordering::SeqCst), len);
}

// ====================================================================
// Phase 49: Session borrowing tests
// ====================================================================

#[test]
fn test_from_session_ptr() {
    let mut session = MockSession::new();
    let executor: Executor = unsafe { Executor::from_session_ptr(&mut session) };

    // Session should be accessible
    let _session_ref = executor.session();
}

#[test]
fn test_from_session_ptr_create_node() {
    let mut session = MockSession::new();
    let mut executor: Executor = unsafe { Executor::from_session_ptr(&mut session) };

    let node = executor.create_node("test_node");
    assert!(node.is_ok());
}

// ====================================================================
// Phase 49: InvocationMode tests
// ====================================================================

#[test]
fn test_set_invocation_mode() {
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    let id = executor
        .add_subscription::<TestMsg, _>("/test", |_: &TestMsg| {})
        .unwrap();

    // Default is OnNewData
    assert_eq!(
        executor.entries[id.0].as_ref().unwrap().invocation,
        InvocationMode::OnNewData
    );

    // Change to Always
    executor.set_invocation(id, InvocationMode::Always);
    assert_eq!(
        executor.entries[id.0].as_ref().unwrap().invocation,
        InvocationMode::Always
    );
}

// ====================================================================
// Phase 49: ExecutorSemantics tests
// ====================================================================

#[test]
fn test_set_semantics() {
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    // Default is RclcppExecutor
    assert_eq!(executor.semantics, ExecutorSemantics::RclcppExecutor);

    executor.set_semantics(ExecutorSemantics::LogicalExecutionTime);
    assert_eq!(executor.semantics, ExecutorSemantics::LogicalExecutionTime);
}

// ====================================================================
// Phase 47: LET semantics pre-sample behavior
// ====================================================================

#[test]
fn test_let_semantics_pre_samples_data() {
    // In LET mode, data is pre-sampled into the entry buffer before any
    // callback runs. This test verifies that the callback receives data
    // even though the mock subscriber's pending data is consumed during
    // the pre-sample phase (not during try_process).
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);
    executor.set_semantics(ExecutorSemantics::LogicalExecutionTime);

    let received = std::sync::Arc::new(std::sync::Mutex::new(None));
    let received2 = received.clone();
    executor
        .add_subscription::<TestMsg, _>("/test", move |msg: &TestMsg| {
            *received2.lock().unwrap() = Some(msg.data);
        })
        .unwrap();

    // Load CDR data
    let (data, len) = encode_test_msg(77);
    let meta = executor.entries[0].as_ref().unwrap();
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    unsafe { &*(arena_ptr.add(meta.offset) as *const MockSubscriber) }.load(data, len);

    let result = executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(result.subscriptions_processed, 1);
    assert_eq!(*received.lock().unwrap(), Some(77));
}

#[test]
fn test_let_semantics_raw_subscription() {
    // Verify LET pre-sampling works for raw subscriptions too.
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);
    executor.set_semantics(ExecutorSemantics::LogicalExecutionTime);

    static RAW_LET_LEN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    unsafe extern "C" fn raw_let_cb(_data: *const u8, len: usize, _ctx: *mut core::ffi::c_void) {
        RAW_LET_LEN.store(len, std::sync::atomic::Ordering::SeqCst);
    }

    RAW_LET_LEN.store(0, std::sync::atomic::Ordering::SeqCst);

    executor
        .add_subscription_raw(
            "/test",
            "test/msg/TestMsg",
            "test_hash",
            raw_let_cb,
            core::ptr::null_mut(),
        )
        .unwrap();

    let (data, len) = encode_test_msg(42);
    let meta = executor.entries[0].as_ref().unwrap();
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    unsafe {
        let sub_ptr = arena_ptr.add(meta.offset) as *const MockSubscriber;
        (*sub_ptr).load(data, len);
    }

    let result = executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(result.subscriptions_processed, 1);
    assert_eq!(RAW_LET_LEN.load(std::sync::atomic::Ordering::SeqCst), len);
}

// ====================================================================
// Phase 47: Trigger::All requires all non-timer handles
// ====================================================================

#[test]
fn test_trigger_all_with_mixed_handles() {
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    // Add a timer and a subscription
    executor
        .add_timer(TimerDuration::from_millis(100), || {})
        .unwrap();
    let _sub_id = executor
        .add_subscription::<TestMsg, _>("/test", |_: &TestMsg| {})
        .unwrap();

    executor.set_trigger(Trigger::All);

    // Timer is always ready, but subscription has no data → trigger fails
    let result = executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(result.subscriptions_processed, 0);
    // Timer delta still accumulates

    // Now load data into subscription
    let (data, len) = encode_test_msg(1);
    let meta1 = executor.entries[1].as_ref().unwrap();
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    unsafe { &*(arena_ptr.add(meta1.offset) as *const MockSubscriber) }.load(data, len);

    let result = elapse_then_spin_once(&mut executor, 100);
    assert_eq!(result.subscriptions_processed, 1);
    assert_eq!(result.timers_fired, 1);
}

// ====================================================================
// Phase 47: Trigger::AllOf sensor fusion pattern
// ====================================================================

#[test]
fn test_trigger_allof_fires_when_both_ready() {
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    let id_a = executor
        .add_subscription::<TestMsg, _>("/sensor_a", |_: &TestMsg| {})
        .unwrap();
    let id_b = executor
        .add_subscription::<TestMsg, _>("/sensor_b", |_: &TestMsg| {})
        .unwrap();

    // AllOf — dispatch only when BOTH subscriptions have data
    executor.set_trigger(Trigger::AllOf(id_a | id_b));

    let arena_ptr = executor.arena.as_ptr() as *const u8;
    let off_a = executor.entries[0].as_ref().unwrap().offset;
    let off_b = executor.entries[1].as_ref().unwrap().offset;

    // Load data only into sensor_a → trigger should NOT fire
    let (data, len) = encode_test_msg(1);
    unsafe { &*(arena_ptr.add(off_a) as *const MockSubscriber) }.load(data, len);

    let result = executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(
        result.subscriptions_processed, 0,
        "AllOf should not fire with only one ready"
    );

    // Now load data into both sensors → trigger should fire
    let (data_a, len_a) = encode_test_msg(10);
    let (data_b, len_b) = encode_test_msg(20);
    unsafe { &*(arena_ptr.add(off_a) as *const MockSubscriber) }.load(data_a, len_a);
    unsafe { &*(arena_ptr.add(off_b) as *const MockSubscriber) }.load(data_b, len_b);

    let result = executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(
        result.subscriptions_processed, 2,
        "AllOf should fire when both ready"
    );
}

#[test]
fn test_trigger_allof_empty_set_always_fires() {
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    executor
        .add_subscription::<TestMsg, _>("/test", |_: &TestMsg| {})
        .unwrap();

    // AllOf with empty set → vacuously true, always dispatches
    executor.set_trigger(Trigger::AllOf(HandleSet::EMPTY));

    // No data loaded, but trigger passes (empty set)
    let result = executor.spin_once(core::time::Duration::from_millis(0));
    // Subscription still has no data, so callback won't fire (try_recv returns None)
    assert_eq!(result.subscriptions_processed, 0);
}

// ====================================================================
// Phase 47: Trigger::AnyOf dispatches on any handle in set
// ====================================================================

#[test]
fn test_trigger_anyof_fires_when_one_ready() {
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    let id_a = executor
        .add_subscription::<TestMsg, _>("/topic_a", |_: &TestMsg| {})
        .unwrap();
    let id_b = executor
        .add_subscription::<TestMsg, _>("/topic_b", |_: &TestMsg| {})
        .unwrap();

    // AnyOf — dispatch when ANY handle in set has data
    executor.set_trigger(Trigger::AnyOf(id_a | id_b));

    // No data → trigger should NOT fire
    let result = executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(
        result.subscriptions_processed, 0,
        "AnyOf should not fire with none ready"
    );

    // Load data only into topic_a → trigger SHOULD fire
    let (data, len) = encode_test_msg(42);
    let meta_a = executor.entries[0].as_ref().unwrap();
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    unsafe { &*(arena_ptr.add(meta_a.offset) as *const MockSubscriber) }.load(data, len);

    let result = executor.spin_once(core::time::Duration::from_millis(0));
    assert!(
        result.subscriptions_processed >= 1,
        "AnyOf should fire when one handle ready"
    );
}

#[test]
fn test_trigger_anyof_empty_set_never_fires() {
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    executor
        .add_subscription::<TestMsg, _>("/test", |_: &TestMsg| {})
        .unwrap();

    // AnyOf with empty set → always false, never dispatches
    executor.set_trigger(Trigger::AnyOf(HandleSet::EMPTY));

    // Load data — trigger still won't pass (empty set, bits & 0 == 0)
    let (data, len) = encode_test_msg(1);
    let meta = executor.entries[0].as_ref().unwrap();
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    unsafe { &*(arena_ptr.add(meta.offset) as *const MockSubscriber) }.load(data, len);

    let result = executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(
        result.subscriptions_processed, 0,
        "AnyOf(EMPTY) should never fire"
    );
}

// ====================================================================
// Phase 49: Timer fires even when trigger fails
// ====================================================================

#[test]
fn test_timer_delta_accumulates_when_trigger_fails() {
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count2 = count.clone();

    executor
        .add_timer(TimerDuration::from_millis(100), move || {
            count2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })
        .unwrap();
    let sub_id = executor
        .add_subscription::<TestMsg, _>("/test", |_: &TestMsg| {})
        .unwrap();

    // Trigger requires specific handle that won't have data
    executor.set_trigger(Trigger::One(sub_id));

    // Timer delta accumulates even when trigger fails.
    // When the timer fires during the trigger-failed path, its callback
    // IS invoked (timers always fire regardless of trigger), but the
    // SpinOnceResult is not propagated.
    let _result = elapse_then_spin_once(&mut executor, 50); // elapsed=50, not ready
    assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 0);

    let _result = elapse_then_spin_once(&mut executor, 60); // elapsed=110, fires!
    // Timer callback fired even though trigger didn't pass
    assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);
}

// ====================================================================
// Service type for Promise tests
// ====================================================================

/// Simple test service: AddTwoInts-like.
struct TestService;

#[derive(Debug, Clone, PartialEq)]
struct TestServiceRequest {
    a: i32,
}

#[derive(Debug, Clone, PartialEq)]
struct TestServiceReply {
    sum: i32,
}

impl RosMessage for TestServiceRequest {
    const TYPE_NAME: &'static str = "test/srv/TestService_Request";
    const TYPE_HASH: &'static str = "test_hash";
}

impl Serialize for TestServiceRequest {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_i32(self.a)
    }
}

impl Deserialize for TestServiceRequest {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            a: reader.read_i32()?,
        })
    }
}

impl RosMessage for TestServiceReply {
    const TYPE_NAME: &'static str = "test/srv/TestService_Reply";
    const TYPE_HASH: &'static str = "test_hash";
}

impl Serialize for TestServiceReply {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_i32(self.sum)
    }
}

impl Deserialize for TestServiceReply {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            sum: reader.read_i32()?,
        })
    }
}

impl nros_core::RosService for TestService {
    type Request = TestServiceRequest;
    type Reply = TestServiceReply;
    const SERVICE_NAME: &'static str = "test/srv/dds_/TestService_";
    const SERVICE_HASH: &'static str = "test_hash";
}

// ====================================================================
// Promise tests
// ====================================================================

#[test]
fn test_promise_try_recv_returns_none_then_some() {
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    let mut node = executor.create_node("test").unwrap();
    let mut client = node.create_client::<TestService>("/test_svc").unwrap();

    // Start a non-blocking call
    let request = TestServiceRequest { a: 42 };
    let mut promise = client.call(&request).unwrap();

    // No reply loaded yet — should return None
    assert!(promise.try_recv().unwrap().is_none());

    // Load a CDR-encoded reply into the mock
    let mut reply_buf = [0u8; 256];
    let mut writer = CdrWriter::new_with_header(&mut reply_buf).unwrap();
    writer.write_i32(99).unwrap();
    let reply_len = writer.position();

    // Access the mock client through the promise handle
    promise.handle.load_reply(reply_buf, reply_len);

    // Now try_recv should return the reply
    let reply = promise.try_recv().unwrap().unwrap();
    assert_eq!(reply.sum, 99);
}

// ====================================================================
// Wall-clock-accurate timer accumulation regression test (Phase 100
// follow-up: 232 Hz → 40 Hz fix).
// ====================================================================
//
// `spin_once(timeout)` used to credit the requested `timeout_ms` to the
// timer accumulator regardless of how long `drive_io` actually blocked.
// MockSession::drive_io returns immediately, so a 100 ms `spin_once`
// would tick a 30 ms timer ~3 times even though 0 wall-clock ms had
// elapsed. Under sustained traffic that broke a 30 Hz control loop into
// >200 Hz overshoot.
//
// The fix: measure real elapsed via `Instant::now()` and carry the
// sub-ms remainder across calls. This test asserts a 50 ms timer does
// NOT fire after a single 1 s `spin_once` against a no-op session.
#[test]
#[cfg(feature = "std")]
fn test_spin_once_does_not_credit_timeout_to_timer_delta() {
    use core::{
        sync::atomic::{AtomicU32, Ordering},
        time::Duration,
    };
    static FIRES: AtomicU32 = AtomicU32::new(0);
    FIRES.store(0, Ordering::SeqCst);

    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    // 50 ms periodic timer.
    let _timer = executor
        .add_timer(TimerDuration::from_millis(50), || {
            FIRES.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap();

    // Ask for a 1 s spin. MockSession::drive_io returns instantly, so
    // real elapsed is ~0 ms. With the bug the timer would fire ~20 times
    // (1000 ms / 50 ms). Without the bug, 0 fires.
    let start = std::time::Instant::now();
    executor.spin_once(Duration::from_millis(1000));
    let real_elapsed_ms = start.elapsed().as_millis() as u64;

    let fires = FIRES.load(Ordering::SeqCst);

    // Expected fires = real_elapsed / 50 ms. Allow off-by-one for the
    // residual carry. Pre-fix this would be ~20 regardless of elapsed.
    let expected_max = (real_elapsed_ms / 50 + 1) as u32;
    assert!(
        fires <= expected_max,
        "timer over-fired: got {fires} fires after only {real_elapsed_ms} ms wall-clock \
         (expected ≤ {expected_max}). The pre-fix bug credited the requested \
         timeout (1000 ms) to the timer delta.",
    );
}

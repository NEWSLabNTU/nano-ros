use super::*;
use nros_core::{
    BorrowedMessage, CdrReader, CdrWriter, DeserError, Deserialize, DeserializeBorrowed,
    LeSliceView, RosAction, RosMessage, SerError, Serialize,
};
use nros_rmw::{QosSettings, TransportError};

use crate::{
    mock::{MockServiceServer, MockSession, MockSubscriber},
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

// Phase 212.K.7.6.b — minimal `Message` impl so the typed creators
// (which gain a `MessageForRmw` bound that tightens to
// `RosMessage + Message` under `rmw-cyclonedds`) still accept this
// test fixture. The codegen template emits both impls for real msg
// crates; here we mirror it by hand.
#[cfg(rmw_cyclonedds_present)]
impl nros_serdes::schema::Message for TestMsg {
    const TYPE_NAME: &'static str = "test/msg/TestMsg";
    const FIELDS: &'static [nros_serdes::schema::Field] = &[nros_serdes::schema::Field {
        name: "data",
        ty: nros_serdes::schema::FieldType::Int32,
        offset: 0,
    }];
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
    let nid = executor
        .node_builder("test_add_subscription_and_spin_once_no_data")
        .build()
        .unwrap();

    // Register a subscription — callback should never fire
    let called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let called2 = called.clone();
    executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/test", move |_msg: &TestMsg| {
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

    let nid = executor
        .node_builder("test_add_subscription_and_spin_once_with_data")
        .build()
        .unwrap();
    let received = std::sync::Arc::new(std::sync::Mutex::new(None));
    let received2 = received.clone();
    executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/test", move |msg: &TestMsg| {
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

// ====================================================================
// Borrowed (zero-copy) subscription E2E (Phase 229.6, issue 0007)
//
// These hand-written types mirror EXACTLY what the codegen emits for a
// `{ uint32 width; uint8[] pixels; float32[] ranges; }` message with
// `pixels` + `ranges` in `borrowed` mode (golden test:
// rosidl-codegen test_nros_borrowed_mode_view_and_marker /
// test_nros_borrowed_float_sequence_uses_le_view). The test drives the
// full owned-publish-wire → borrowed-subscribe path through `spin_once`,
// proving the generated shape compiles + decodes against the runtime.
// ====================================================================

/// Owned message — the publish side (matches codegen `Image`).
#[derive(Debug, Clone, PartialEq)]
struct Image {
    width: u32,
    pixels: heapless::Vec<u8, 64>,
    ranges: heapless::Vec<f32, 64>,
}

impl Serialize for Image {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_u32(self.width)?;
        writer.write_u32(self.pixels.len() as u32)?;
        for b in &self.pixels {
            writer.write_u8(*b)?;
        }
        writer.write_u32(self.ranges.len() as u32)?;
        for f in &self.ranges {
            writer.write_f32(*f)?;
        }
        Ok(())
    }
}

/// Borrowed view — the zero-copy receive side (matches codegen `ImageView<'a>`).
struct ImageView<'a> {
    width: u32,
    pixels: &'a [u8],
    ranges: LeSliceView<'a, f32>,
}

impl<'a> DeserializeBorrowed<'a> for ImageView<'a> {
    fn deserialize_borrowed(reader: &mut CdrReader<'a>) -> Result<Self, DeserError> {
        Ok(Self {
            width: reader.read_u32()?,
            pixels: reader.read_slice_u8()?,
            ranges: reader.read_le_slice::<f32>()?,
        })
    }
}

/// ZST marker — matches codegen `ImageBorrow`.
struct ImageBorrow;
impl BorrowedMessage for ImageBorrow {
    type View<'a> = ImageView<'a>;
    const TYPE_NAME: &'static str = "test/msg/Image";
    const TYPE_HASH: &'static str = "image_hash";
}

#[test]
fn borrowed_subscription_e2e_zero_copy_through_spin_once() {
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);
    let nid = executor.node_builder("borrowed_e2e").build().unwrap();

    // Captured on the receive side: (width, pixels copy, ranges decoded).
    type Captured = (u32, std::vec::Vec<u8>, std::vec::Vec<f32>);
    let received: std::sync::Arc<std::sync::Mutex<Option<Captured>>> =
        std::sync::Arc::new(std::sync::Mutex::new(None));
    let received2 = received.clone();

    executor
        .node_mut(nid)
        .create_subscription_borrowed::<ImageBorrow, _>("/image", move |view: &ImageView<'_>| {
            *received2.lock().unwrap() = Some((
                view.width,
                view.pixels.to_vec(),
                view.ranges.iter().collect(),
            ));
        })
        .unwrap();

    // Encode the OWNED message exactly as a publisher would, then feed those
    // wire bytes to the borrowed subscriber's mock handle.
    let msg = Image {
        width: 7,
        pixels: heapless::Vec::from_slice(&[10, 20, 30, 40, 250]).unwrap(),
        ranges: heapless::Vec::from_slice(&[1.5f32, -2.25, 3.0e10]).unwrap(),
    };
    let mut buf = [0u8; 256];
    let len = {
        let mut w = CdrWriter::new_with_header(&mut buf).unwrap();
        msg.serialize(&mut w).unwrap();
        w.position()
    };

    // The MockSubscriber is the first field of SubBufferedBorrowedEntry, so it
    // sits at the entry offset (same layout trick as the typed test above).
    let meta = executor.entries[0].as_ref().unwrap();
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    let sub_ptr = unsafe { arena_ptr.add(meta.offset) } as *const MockSubscriber;
    unsafe { &*sub_ptr }.load(buf, len);

    let result = executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(result.subscriptions_processed, 1);
    assert!(result.any_work());

    let got = received.lock().unwrap().take().expect("callback fired");
    assert_eq!(got.0, 7);
    assert_eq!(got.1, std::vec![10, 20, 30, 40, 250]);
    assert_eq!(got.2, std::vec![1.5f32, -2.25, 3.0e10]);
}

#[test]
fn test_multiple_subscriptions() {
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    let nid = executor
        .node_builder("test_multiple_subscriptions")
        .build()
        .unwrap();
    let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count1 = count.clone();
    let count2 = count.clone();

    executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/topic1", move |_msg: &TestMsg| {
            count1.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })
        .unwrap();

    executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/topic2", move |_msg: &TestMsg| {
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

/// Phase 110.B — when two subscriptions are bound to `Edf` SCs,
/// the one with the earlier deadline dispatches first regardless of
/// registration order.
#[test]
fn test_edf_dispatch_order() {
    use crate::executor::sched_context::{DeadlinePolicy, OptUs, SchedClass, SchedContext};
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    // `firing_order` records the data field of every msg the callbacks
    // see, in dispatch order.
    let firing_order = std::sync::Arc::new(std::sync::Mutex::new(std::vec::Vec::<i32>::new()));
    let order_late = firing_order.clone();
    let order_early = firing_order.clone();

    let nid = executor
        .node_builder("test_edf_dispatch_order")
        .build()
        .unwrap();
    // Registered first → has lower DescIdx → would normally dispatch
    // first under the FIFO path. Bind to a *later* deadline.
    let h_late = executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/late", move |msg: &TestMsg| {
            order_late.lock().unwrap().push(msg.data);
        })
        .unwrap();

    // Registered second → higher DescIdx → would dispatch second under
    // FIFO. Bind to an *earlier* deadline so EDF promotes it.
    let h_early = executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/early", move |msg: &TestMsg| {
            order_early.lock().unwrap().push(msg.data);
        })
        .unwrap();

    let sc_late = executor
        .create_sched_context(SchedContext {
            class: SchedClass::Edf,
            deadline_us: OptUs::from_us(1000),
            deadline_policy: DeadlinePolicy::Activated,
            ..Default::default()
        })
        .unwrap();
    let sc_early = executor
        .create_sched_context(SchedContext {
            class: SchedClass::Edf,
            deadline_us: OptUs::from_us(100),
            deadline_policy: DeadlinePolicy::Activated,
            ..Default::default()
        })
        .unwrap();

    executor
        .bind_handle_to_sched_context(h_late, sc_late)
        .unwrap();
    executor
        .bind_handle_to_sched_context(h_early, sc_early)
        .unwrap();

    // Load data into both subscribers — `data` field identifies which
    // is which in the firing log.
    let (d_late, n_late) = encode_test_msg(10);
    let (d_early, n_early) = encode_test_msg(20);
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    let off_late = executor.entries[0].as_ref().unwrap().offset;
    let off_early = executor.entries[1].as_ref().unwrap().offset;
    unsafe { &*(arena_ptr.add(off_late) as *const MockSubscriber) }.load(d_late, n_late);
    unsafe { &*(arena_ptr.add(off_early) as *const MockSubscriber) }.load(d_early, n_early);

    let result = executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(result.subscriptions_processed, 2);

    let order = firing_order.lock().unwrap();
    // Earlier-deadline (data=20) must precede later-deadline (data=10).
    assert_eq!(*order, std::vec![20, 10]);
}

/// Phase 110.F — `os_pri` worker-pool dispatch routes the bound
/// callback through a worker thread instead of the cooperative path.
/// Smoke test: register a sub bound to an SC with `os_pri = 1`, fire
/// spin_once, verify the worker eventually drains + dispatches.
/// Uses a no-op `apply_policy` (non-root tests can't lift to
/// SCHED_FIFO).
#[cfg(feature = "scheduler-os-priority")]
#[test]
fn test_os_priority_worker_dispatches_callback() {
    use crate::executor::sched_context::{SchedClass, SchedContext};
    use nros_platform_api::SchedPolicy;
    fn apply_noop(_p: SchedPolicy) -> Result<(), nros_platform_api::SchedError> {
        Ok(())
    }
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);
    executor.register_os_priority_dispatcher(apply_noop);
    let nid = executor
        .node_builder("test_os_priority_worker_dispatches_callback")
        .build()
        .unwrap();

    let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count_cb = count.clone();
    let h = executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/picas", move |_msg: &TestMsg| {
            count_cb.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })
        .unwrap();

    let sc_id = executor
        .create_sched_context(SchedContext {
            class: SchedClass::Fifo,
            os_pri: 1,
            ..Default::default()
        })
        .unwrap();
    executor.bind_handle_to_sched_context(h, sc_id).unwrap();

    let arena_ptr = executor.arena.as_ptr() as *const u8;
    let off = executor.entries[0].as_ref().unwrap().offset;
    let (d, n) = encode_test_msg(7);
    unsafe { &*(arena_ptr.add(off) as *const MockSubscriber) }.load(d, n);

    // spin_once routes to worker; sleep gives worker time to drain.
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    std::thread::sleep(std::time::Duration::from_millis(50));

    assert_eq!(
        count.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "os_pri-bound callback must dispatch via worker"
    );
}

/// Phase 110.G — TT-window gate suppresses dispatch when the
/// current monotonic time falls outside `[off, off + duration)`.
/// Coexists with the existing class-based dispatch — this test uses
/// a `Fifo` SC with a TT window set, demonstrating that the gate is
/// orthogonal to class.
#[test]
fn test_tt_window_gate_suppresses_outside_window() {
    use crate::executor::sched_context::{OptUs, SchedClass, SchedContext};
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    // Window = [50ms..51ms) within a 60-second major frame.
    // Test runs in a single spin_once well under 50 ms after the
    // executor's epoch — phase < 50 ms → outside window → dispatch
    // suppressed.
    executor.register_time_triggered_dispatcher(60_000_000);
    let nid = executor
        .node_builder("test_tt_window_gate_suppresses_outside_window")
        .build()
        .unwrap();

    let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count_cb = count.clone();
    let h = executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/tt", move |_msg: &TestMsg| {
            count_cb.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })
        .unwrap();

    // Far-future window so the spin happens outside it.
    let sc_id = executor
        .create_sched_context(SchedContext {
            class: SchedClass::Fifo,
            tt_window_offset_us: OptUs::from_us(50_000_000),
            tt_window_duration_us: OptUs::from_us(1_000),
            ..Default::default()
        })
        .unwrap();
    executor.bind_handle_to_sched_context(h, sc_id).unwrap();

    let arena_ptr = executor.arena.as_ptr() as *const u8;
    let off = executor.entries[0].as_ref().unwrap().offset;
    let (d, n) = encode_test_msg(1);
    unsafe { &*(arena_ptr.add(off) as *const MockSubscriber) }.load(d, n);

    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(
        count.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "TT window gate must suppress dispatch outside the active slot"
    );
}

/// Phase 110.G — schedule-table builder declares + applies a full
/// cyclic schedule in one call: validates window layout, sets
/// major-frame length, creates one SC per window with the right
/// TT-gate fields. Two-window schedule with the first window
/// covering [0..1s) within a 2-second major frame ensures the
/// test's spin (well under 1s after executor construction) fires
/// the entry bound to window-0 and suppresses the one bound to
/// window-1.
#[test]
fn test_time_triggered_dispatch_active_window() {
    use crate::executor::sched_context::{TimeTriggeredSchedule, TimeTriggeredWindow};

    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    let count_w0 = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count_w1 = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let cb0 = count_w0.clone();
    let cb1 = count_w1.clone();
    let nid = executor
        .node_builder("test_apply_time_triggered_schedule")
        .build()
        .unwrap();
    let h0 = executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/tt0", move |_msg: &TestMsg| {
            cb0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })
        .unwrap();
    let h1 = executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/tt1", move |_msg: &TestMsg| {
            cb1.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })
        .unwrap();

    let schedule = TimeTriggeredSchedule::<2>::new_full(
        2_000_000,
        [
            TimeTriggeredWindow::new(0, 1_000_000, "w0"),
            TimeTriggeredWindow::new(1_000_000, 1_000_000, "w1"),
        ],
    );
    let ids = executor
        .apply_time_triggered_schedule(&schedule)
        .expect("schedule should validate");
    executor.bind_handle_to_sched_context(h0, ids[0]).unwrap();
    executor.bind_handle_to_sched_context(h1, ids[1]).unwrap();

    let arena_ptr = executor.arena.as_ptr() as *const u8;
    let off0 = executor.entries[0].as_ref().unwrap().offset;
    let off1 = executor.entries[1].as_ref().unwrap().offset;
    let (d0, n0) = encode_test_msg(10);
    let (d1, n1) = encode_test_msg(20);
    unsafe { &*(arena_ptr.add(off0) as *const MockSubscriber) }.load(d0, n0);
    unsafe { &*(arena_ptr.add(off1) as *const MockSubscriber) }.load(d1, n1);

    let _ = executor.spin_once(core::time::Duration::from_millis(0));

    // Phase < 1s → only the entry bound to window-0 fires; the
    // entry bound to window-1 must stay suppressed.
    assert_eq!(
        count_w0.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "entry bound to window-0 should dispatch inside its active slot"
    );
    assert_eq!(
        count_w1.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "entry bound to window-1 must stay suppressed outside its slot"
    );
}

/// Phase 110.G — schedule validation. Overlapping windows are an
/// authoring bug and surface as a structured error rather than
/// silent precedence between dispatchers.
#[test]
fn test_time_triggered_schedule_rejects_overlapping_windows() {
    use crate::executor::sched_context::{
        TimeTriggeredSchedule, TimeTriggeredScheduleError, TimeTriggeredWindow,
    };

    let bad = TimeTriggeredSchedule::<2>::new_full(
        1_000_000,
        [
            TimeTriggeredWindow::new(0, 600_000, "w0"),
            TimeTriggeredWindow::new(500_000, 200_000, "w1"),
        ],
    );
    let err = bad.validate().unwrap_err();
    assert!(
        matches!(err, TimeTriggeredScheduleError::WindowsOverlap { .. }),
        "overlapping windows must surface as a WindowsOverlap error, got {err:?}"
    );

    let oversize =
        TimeTriggeredSchedule::<1>::new_full(1_000, [TimeTriggeredWindow::new(500, 600, "w0")]);
    let err = oversize.validate().unwrap_err();
    assert!(
        matches!(
            err,
            TimeTriggeredScheduleError::WindowExceedsMajorFrame { .. }
        ),
        "window past major-frame end must surface as WindowExceedsMajorFrame, got {err:?}"
    );
}

/// Phase 110.E — `SchedClass::Sporadic` budget suppression. After the
/// budget is exhausted within a period, the bound subscription's
/// callback no longer fires until the next period boundary refills
/// the budget.
#[test]
fn test_sporadic_budget_exhaustion_suppresses_dispatch() {
    use crate::executor::sched_context::{OptUs, SchedClass, SchedContext};
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    let nid = executor
        .node_builder("test_sporadic_budget_exhaustion")
        .build()
        .unwrap();
    let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count_cb = count.clone();
    let h = executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/sporadic", move |_msg: &TestMsg| {
            count_cb.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })
        .unwrap();

    // 1 us budget per 60 s period — first cycle's `delta_ms` saturates
    // and exhausts the budget immediately, so the second cycle's
    // dispatch is suppressed.
    let sc_id = executor
        .create_sched_context(SchedContext {
            class: SchedClass::Sporadic,
            budget_us: OptUs::from_us(1),
            period_us: OptUs::from_us(60_000_000),
            ..Default::default()
        })
        .unwrap();
    executor.bind_handle_to_sched_context(h, sc_id).unwrap();

    let arena_ptr = executor.arena.as_ptr() as *const u8;
    let off = executor.entries[0].as_ref().unwrap().offset;

    // Cycle 1 — the first `tick` pass refills the budget to 1 us,
    // then deducts the `delta_us` since the executor was constructed
    // (probably 0 us on a fast machine, leaving 1 us). Either way
    // the callback fires and the budget is consumed.
    let (d, n) = encode_test_msg(1);
    unsafe { &*(arena_ptr.add(off) as *const MockSubscriber) }.load(d, n);
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    // Sleep to push elapsed time past 1 us so cycle 2's tick
    // exhausts whatever residual budget remained.
    std::thread::sleep(std::time::Duration::from_millis(2));

    // Cycle 2 — budget is 0; dispatch must be suppressed.
    let initial = count.load(std::sync::atomic::Ordering::SeqCst);
    let (d, n) = encode_test_msg(2);
    unsafe { &*(arena_ptr.add(off) as *const MockSubscriber) }.load(d, n);
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    let after = count.load(std::sync::atomic::Ordering::SeqCst);

    // Strictly assert no new dispatch on cycle 2.
    assert_eq!(
        after, initial,
        "Sporadic SC must suppress dispatch when budget exhausted"
    );
}

/// Phase 110.E.b follow-up — per-callback runtime accounting.
/// When a Sporadic SC has an `AtomicSporadicState` registered (the
/// ISR-driven refill path), `spin_once` measures each bound
/// callback's wall-clock dispatch time + `consume`s those microseconds
/// from the atomic budget. Replaces the cycle-level over-attribution
/// that previously charged the full cycle `delta_us` against every
/// Sporadic SC regardless of which entries actually fired.
#[test]
#[cfg(feature = "alloc")]
fn test_atomic_sporadic_per_callback_runtime_consumed() {
    use crate::executor::{
        sched_context::{OptUs, SchedClass, SchedContext},
        spin::OpaqueTimerHandle,
    };

    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    let nid = executor
        .node_builder("test_atomic_sporadic_per_callback_runtime_consumed")
        .build()
        .unwrap();
    // Subscription that sleeps a known interval so the per-callback
    // dispatch timing is deterministic enough to assert on.
    let h = executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/timed", move |_msg: &TestMsg| {
            std::thread::sleep(std::time::Duration::from_millis(10));
        })
        .unwrap();

    // Sporadic SC with 1 s budget — plenty so dispatch always
    // proceeds; the assertion is on the *consumed* amount, not
    // suppression.
    let sc_id = executor
        .create_sched_context(SchedContext {
            class: SchedClass::Sporadic,
            budget_us: OptUs::from_us(1_000_000),
            period_us: OptUs::from_us(60_000_000),
            ..Default::default()
        })
        .unwrap();
    executor.bind_handle_to_sched_context(h, sc_id).unwrap();

    // Build a no-op `OpaqueTimerHandle` so `register_sporadic_timer`
    // accepts the call — the test doesn't need a real periodic
    // refill; it only needs the `AtomicSporadicState` slot wired so
    // the dispatcher consumes runtime from it.
    extern "C" fn noop_destroy(_handle: *mut core::ffi::c_void) {}
    let fake_timer = unsafe { OpaqueTimerHandle::new(core::ptr::null_mut(), noop_destroy) };
    let state = executor.register_sporadic_timer(sc_id, fake_timer).unwrap();
    let before = state
        .budget_remaining_us
        .load(portable_atomic::Ordering::Acquire);

    // Drive one dispatch — the registered closure sleeps 10 ms.
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    let off = executor.entries[0].as_ref().unwrap().offset;
    let (d, n) = encode_test_msg(7);
    unsafe { &*(arena_ptr.add(off) as *const MockSubscriber) }.load(d, n);
    let _ = executor.spin_once(core::time::Duration::from_millis(0));

    let after = state
        .budget_remaining_us
        .load(portable_atomic::Ordering::Acquire);

    // Per-callback runtime consumed at least the 10 ms (= 10_000 us)
    // sleep, but well under the full 1 s budget — proves dispatch-
    // local measurement, not cycle-level over-attribution.
    let consumed = before.saturating_sub(after);
    assert!(
        consumed >= 10_000,
        "expected at least 10 ms (10000 us) consumed for a 10 ms callback, got {consumed} us"
    );
    assert!(
        consumed < 500_000,
        "consumed {consumed} us suggests cycle-level over-attribution, not per-callback measurement"
    );
}

/// Phase 110.E.b — overrun detection. A Sporadic-bound callback
/// whose measured runtime exceeds the SC's budget bumps
/// `AtomicSporadicState::overrun_count` + stamps `last_overrun_us`.
/// The "oneshot-IRQ-and-cancel" pattern from the design doc is
/// structurally equivalent for cooperative single-thread dispatch
/// (we can't preempt a running callback), so this is the
/// diagnostic signal end-callers consume to tune budgets.
#[test]
#[cfg(feature = "alloc")]
fn test_atomic_overrun_exceeds_budget() {
    use crate::executor::{
        sched_context::{OptUs, SchedClass, SchedContext},
        spin::OpaqueTimerHandle,
    };

    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    let nid = executor
        .node_builder("test_atomic_sporadic_overrun_recorded")
        .build()
        .unwrap();
    // Subscription sleeps 25 ms; budget is 5 ms → must overrun.
    let h = executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/overrun", move |_msg: &TestMsg| {
            std::thread::sleep(std::time::Duration::from_millis(25));
        })
        .unwrap();

    let sc_id = executor
        .create_sched_context(SchedContext {
            class: SchedClass::Sporadic,
            budget_us: OptUs::from_us(5_000), // 5 ms budget
            period_us: OptUs::from_us(60_000_000),
            ..Default::default()
        })
        .unwrap();
    executor.bind_handle_to_sched_context(h, sc_id).unwrap();

    extern "C" fn noop_destroy(_h: *mut core::ffi::c_void) {}
    let fake_timer = unsafe { OpaqueTimerHandle::new(core::ptr::null_mut(), noop_destroy) };
    let state = executor.register_sporadic_timer(sc_id, fake_timer).unwrap();
    assert_eq!(
        state.overrun_count.load(portable_atomic::Ordering::Acquire),
        0
    );

    // Drive one dispatch — the registered closure sleeps 25 ms.
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    let off = executor.entries[0].as_ref().unwrap().offset;
    let (d, n) = encode_test_msg(42);
    unsafe { &*(arena_ptr.add(off) as *const MockSubscriber) }.load(d, n);
    let _ = executor.spin_once(core::time::Duration::from_millis(0));

    let count = state.overrun_count.load(portable_atomic::Ordering::Acquire);
    let last = state
        .last_overrun_us
        .load(portable_atomic::Ordering::Acquire);
    assert_eq!(count, 1, "overrun_count must increment exactly once");
    // Overrun = measured - budget; measured ≥ 25 ms, budget = 5 ms.
    // last_overrun_us should be ≥ 20 ms = 20_000 us.
    assert!(
        last >= 20_000,
        "last_overrun_us {last} should be ≥ 20000 (25 ms callback - 5 ms budget)"
    );

    // `clear_overrun_stats` resets both counters.
    state.clear_overrun_stats();
    assert_eq!(
        state.overrun_count.load(portable_atomic::Ordering::Acquire),
        0
    );
    assert_eq!(
        state
            .last_overrun_us
            .load(portable_atomic::Ordering::Acquire),
        0
    );
}

/// Phase 110.D — multi-executor smoke test. Spawns two Executors,
/// each on its own OS thread with a different `SchedPolicy`. Mirrors
/// the shape of the drone S1 / watchdog S3 acceptance scenarios from
/// the phase doc. Live SCHED_FIFO requires `CAP_SYS_NICE`, so the
/// test uses a no-op `apply_policy` and only asserts the lifecycle
/// works — full timing acceptance for S1 / S3 ships once the
/// integration harness with privileged scheduling is in place.
#[test]
fn test_open_threaded_two_executors_independent_lifecycle() {
    use nros_platform_api::SchedPolicy;
    fn apply_noop(_p: SchedPolicy) -> Result<(), nros_platform_api::SchedError> {
        Ok(())
    }

    // Critical executor — would run at SCHED_FIFO os_pri 90 in a
    // privileged process.
    let crit = Executor::from_session(MockSession::new());
    let crit_handle = unsafe {
        crit.open_threaded(
            SchedPolicy::Fifo { os_pri: 90 },
            apply_noop,
            core::time::Duration::from_millis(1),
        )
    };

    // BE executor — would run at SCHED_FIFO os_pri 10 in a
    // privileged process.
    let be = Executor::from_session(MockSession::new());
    let be_handle = unsafe {
        be.open_threaded(
            SchedPolicy::Fifo { os_pri: 10 },
            apply_noop,
            core::time::Duration::from_millis(5),
        )
    };

    // Let both run a couple of cycles.
    std::thread::sleep(std::time::Duration::from_millis(20));

    // Halt each independently — order shouldn't matter.
    assert!(crit_handle.join().is_ok());
    assert!(be_handle.join().is_ok());
}

/// Phase 110.D.b — smoke test for `Executor::open_threaded`. Spawns
/// the executor onto a fresh OS thread, lets it spin, then halts via
/// the returned `ThreadHandle`.
#[test]
fn test_open_threaded_spawn_and_halt() {
    use nros_platform_api::SchedPolicy;
    let session = MockSession::new();
    let executor: Executor = Executor::from_session(session);

    // Apply-policy fn that always succeeds — running as a non-root
    // unit test we can't actually lift to SCHED_FIFO, so the
    // smoke-test just exercises the spawn / halt / join lifecycle.
    fn apply_noop(_p: SchedPolicy) -> Result<(), nros_platform_api::SchedError> {
        Ok(())
    }

    // SAFETY: `from_session` (Owned) Executor is Send-correct;
    // `unsafe impl Send for Executor` covers it unconditionally.
    let handle = unsafe {
        executor.open_threaded(
            SchedPolicy::Fifo { os_pri: 1 },
            apply_noop,
            core::time::Duration::from_millis(1),
        )
    };

    // Let the executor thread run a couple of spin cycles.
    std::thread::sleep(std::time::Duration::from_millis(20));

    // Halt + join must complete within a generous bound.
    let join_res = handle.join();
    assert!(join_res.is_ok());
}

/// Phase 110.C — `Critical`-bucket callback runs before
/// `BestEffort`-bucket callback when both are ready in the same cycle,
/// regardless of registration order.
#[test]
fn test_bucketed_priority_dispatch_order() {
    use crate::executor::sched_context::{Priority, SchedClass, SchedContext};
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    let firing_order = std::sync::Arc::new(std::sync::Mutex::new(std::vec::Vec::<i32>::new()));
    let o_be = firing_order.clone();
    let o_crit = firing_order.clone();

    let nid = executor
        .node_builder("test_bucketed_priority_dispatch_order")
        .build()
        .unwrap();
    // Registered first (lower DescIdx) — bound to BestEffort.
    let h_be = executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/be", move |msg: &TestMsg| {
            o_be.lock().unwrap().push(msg.data);
        })
        .unwrap();
    // Registered second — bound to Critical so the bucket promotion
    // beats registration order.
    let h_crit = executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/crit", move |msg: &TestMsg| {
            o_crit.lock().unwrap().push(msg.data);
        })
        .unwrap();

    let sc_be = executor
        .create_sched_context(SchedContext {
            class: SchedClass::Fifo,
            priority: Priority::BestEffort,
            ..Default::default()
        })
        .unwrap();
    let sc_crit = executor
        .create_sched_context(SchedContext {
            class: SchedClass::Fifo,
            priority: Priority::Critical,
            ..Default::default()
        })
        .unwrap();
    executor.bind_handle_to_sched_context(h_be, sc_be).unwrap();
    executor
        .bind_handle_to_sched_context(h_crit, sc_crit)
        .unwrap();

    let (d_be, n_be) = encode_test_msg(1);
    let (d_crit, n_crit) = encode_test_msg(2);
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    let off_be = executor.entries[0].as_ref().unwrap().offset;
    let off_crit = executor.entries[1].as_ref().unwrap().offset;
    unsafe { &*(arena_ptr.add(off_be) as *const MockSubscriber) }.load(d_be, n_be);
    unsafe { &*(arena_ptr.add(off_crit) as *const MockSubscriber) }.load(d_crit, n_crit);

    let result = executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(result.subscriptions_processed, 2);

    let order = firing_order.lock().unwrap();
    // Critical (data=2) drains before BestEffort (data=1).
    assert_eq!(*order, std::vec![2, 1]);
}

/// Phase 110.B — default `Fifo` SC binding preserves registration
/// order even when other entries are bound to `Edf` SCs.
#[test]
fn test_fifo_default_binding_preserved_alongside_edf() {
    use crate::executor::sched_context::{OptUs, SchedClass, SchedContext};
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    let firing_order = std::sync::Arc::new(std::sync::Mutex::new(std::vec::Vec::<i32>::new()));
    let o1 = firing_order.clone();
    let o2 = firing_order.clone();

    let nid = executor
        .node_builder("test_fifo_default_binding_preserved_alongside_edf")
        .build()
        .unwrap();
    let _h1 = executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/fifo1", move |msg: &TestMsg| {
            o1.lock().unwrap().push(msg.data);
        })
        .unwrap();
    let h2 = executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/edf", move |msg: &TestMsg| {
            o2.lock().unwrap().push(msg.data);
        })
        .unwrap();

    let sc_edf = executor
        .create_sched_context(SchedContext {
            class: SchedClass::Edf,
            deadline_us: OptUs::from_us(50),
            ..Default::default()
        })
        .unwrap();
    executor.bind_handle_to_sched_context(h2, sc_edf).unwrap();

    let (d1, n1) = encode_test_msg(1);
    let (d2, n2) = encode_test_msg(2);
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    let off1 = executor.entries[0].as_ref().unwrap().offset;
    let off2 = executor.entries[1].as_ref().unwrap().offset;
    unsafe { &*(arena_ptr.add(off1) as *const MockSubscriber) }.load(d1, n1);
    unsafe { &*(arena_ptr.add(off2) as *const MockSubscriber) }.load(d2, n2);

    let result = executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(result.subscriptions_processed, 2);

    // EDF-bound entry (data=2) drains first; FIFO-bound (data=1) second.
    let order = firing_order.lock().unwrap();
    assert_eq!(*order, std::vec![2, 1]);
}

#[test]
fn test_arena_overflow() {
    let session = MockSession::new();
    // Arena is ARENA_SIZE bytes (derived to fit MAX_CBS worst-case ActionClient
    // entries — see `nros-node/build.rs`). Use a subscription RX buffer larger
    // than `ARENA_SIZE / MAX_CBS` so we exhaust the arena before running out
    // of entry slots. Each SubBufferedEntry holds a triple buffer (3 × RX_BUF)
    // plus a per-entry header, so an RX buffer of `ARENA_SIZE / 4` triggers
    // overflow well before `MAX_CBS` registrations.
    const OVERFLOW_RX_BUF: usize = crate::config::ARENA_SIZE / 4;
    let mut executor = Executor::from_session(session);
    let nid = executor
        .node_builder("test_arena_overflow")
        .build()
        .unwrap();

    let topics = ["/a", "/b", "/c", "/d"];
    let mut filled = 0;
    for topic in &topics {
        let result = executor
            .node_mut(nid)
            .subscription(topic)
            .qos(QosSettings::default().keep_last(1))
            .typed::<TestMsg>()
            .rx_buffer::<OVERFLOW_RX_BUF>()
            .build(|_msg: &TestMsg| {});
        if result.is_err() {
            break;
        }
        filled += 1;
    }

    // We should have been able to add at least 1 but not all 4 (arena too small).
    assert!(filled >= 1, "Should fit at least 1 large subscription");
    assert!(
        filled < 4,
        "Arena should overflow before 4 large subscriptions, got {filled}"
    );

    // Verify the next add fails with BufferTooSmall.
    let result = executor
        .node_mut(nid)
        .subscription("/overflow")
        .qos(QosSettings::default().keep_last(1))
        .typed::<TestMsg>()
        .rx_buffer::<OVERFLOW_RX_BUF>()
        .build(|_msg: &TestMsg| {});
    assert_eq!(result, Err(NodeError::BufferTooSmall));
}

#[test]
fn test_entry_slots_exhausted() {
    let session = MockSession::new();
    // MAX_CBS=4 slots. Use small buffers to avoid arena overflow before
    // exhausting slots.
    let mut executor = Executor::from_session(session);
    let nid = executor
        .node_builder("test_entry_slots_exhausted")
        .build()
        .unwrap();

    for topic in &["/a", "/b", "/c", "/d"] {
        executor
            .node_mut(nid)
            .subscription(topic)
            .qos(QosSettings::default().keep_last(1))
            .typed::<TestMsg>()
            .rx_buffer::<64>()
            .build(|_msg: &TestMsg| {})
            .unwrap();
    }

    // 5th registration should fail — all 4 slots are taken.
    let result = executor
        .node_mut(nid)
        .subscription("/e")
        .qos(QosSettings::default().keep_last(1))
        .typed::<TestMsg>()
        .rx_buffer::<64>()
        .build(|_msg: &TestMsg| {});
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

    let nid = executor
        .node_builder("test_drop_runs_without_panic")
        .build()
        .unwrap();
    executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/test", |_msg: &TestMsg| {})
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

    let nid = executor
        .node_builder("test_arena_alignment")
        .build()
        .unwrap();
    // Add a subscription, then check the offset is properly aligned
    executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/test", |_msg: &TestMsg| {})
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
        .register_timer(TimerDuration::from_millis(100), move || {
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
        .register_timer(TimerDuration::from_millis(100), move || {
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
        .register_timer_oneshot(TimerDuration::from_millis(50), move || {
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
        .register_timer(TimerDuration::from_millis(100), move || {
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
        .register_timer(TimerDuration::from_millis(100), move || {
            timer_count2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })
        .unwrap();

    let nid = executor
        .node_builder("test_timer_with_subscriptions")
        .build()
        .unwrap();
    let sub_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let sub_count2 = sub_count.clone();
    executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/test", move |_msg: &TestMsg| {
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

#[cfg(rmw_cyclonedds_present)]
impl nros_serdes::schema::Message for TestGoal {
    const TYPE_NAME: &'static str = "test/action/TestAction_Goal";
    const FIELDS: &'static [nros_serdes::schema::Field] = &[nros_serdes::schema::Field {
        name: "order",
        ty: nros_serdes::schema::FieldType::Int32,
        offset: 0,
    }];
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

#[cfg(rmw_cyclonedds_present)]
impl nros_serdes::schema::Message for TestResult {
    const TYPE_NAME: &'static str = "test/action/TestAction_Result";
    const FIELDS: &'static [nros_serdes::schema::Field] = &[nros_serdes::schema::Field {
        name: "value",
        ty: nros_serdes::schema::FieldType::Int32,
        offset: 0,
    }];
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

#[cfg(rmw_cyclonedds_present)]
impl nros_serdes::schema::Message for TestFeedback {
    const TYPE_NAME: &'static str = "test/action/TestAction_Feedback";
    const FIELDS: &'static [nros_serdes::schema::Field] = &[nros_serdes::schema::Field {
        name: "progress",
        ty: nros_serdes::schema::FieldType::Int32,
        offset: 0,
    }];
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
    // For tests the envelope types reuse the inner message types: the executor
    // tests only exercise the spin-arena registration path, not on-wire CDR
    // round-trips of the action service shapes.
    type SendGoalRequest = TestGoal;
    type SendGoalResponse = TestResult;
    type GetResultRequest = TestGoal;
    type GetResultResponse = TestResult;
    type FeedbackMessage = TestFeedback;
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
        .register_action_server_sized::<TestAction, _, _, 64, 64, 64, 1>(
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
        .register_action_server_sized::<TestAction, _, _, 64, 64, 64, 1>(
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
        .register_action_server_sized::<TestAction, _, _, 64, 64, 64, 1>(
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

    let nid = executor
        .node_builder("test_drop_with_mixed_entries")
        .build()
        .unwrap();
    // Register one of each kind — use small buffers to fit in 4096-byte arena.
    executor
        .node_mut(nid)
        .subscription("/sub")
        .qos(QosSettings::default().keep_last(1))
        .typed::<TestMsg>()
        .rx_buffer::<64>()
        .build(|_msg: &TestMsg| {})
        .unwrap();
    executor
        .register_timer(TimerDuration::from_millis(100), || {})
        .unwrap();
    let _server = executor
        .register_action_server_sized::<TestAction, _, _, 64, 64, 64, 1>(
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

#[test]
fn test_wake_handle_clone() {
    let session = MockSession::new();
    let executor: Executor = Executor::from_session(session);

    let wake = executor.wake_handle();
    assert!(!wake.load(std::sync::atomic::Ordering::SeqCst));

    executor.wake();
    assert!(wake.load(std::sync::atomic::Ordering::SeqCst));
}

#[test]
fn test_wake_cleared_each_spin() {
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    // Pre-arm the flag — spin_once must swap-clear it.
    executor.wake();
    let wake = executor.wake_handle();
    assert!(wake.load(std::sync::atomic::Ordering::SeqCst));

    let _ = executor.spin_once(core::time::Duration::from_millis(1));
    assert!(
        !wake.load(std::sync::atomic::Ordering::SeqCst),
        "spin_once must consume the wake flag"
    );
}

#[test]
fn test_halt_raises_wake_flag() {
    let session = MockSession::new();
    let executor: Executor = Executor::from_session(session);

    let wake = executor.wake_handle();
    assert!(!wake.load(std::sync::atomic::Ordering::SeqCst));

    executor.halt();
    assert!(executor.is_halted());
    assert!(
        wake.load(std::sync::atomic::Ordering::SeqCst),
        "halt() must also set the wake flag so an in-flight spin_once \
         falls through to the halt check on its next iteration"
    );
}

#[test]
fn test_guard_handle_send_across_thread() {
    // Phase 124.B.7.d — GuardConditionHandle must be Send (so a
    // worker thread / signal handler can own it and call trigger()).
    // Sync impl assertion via thread move and rejoin.
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    let (_id, handle) = executor
        .register_guard_condition(|| {})
        .expect("register_guard_condition");

    let t = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(5));
        handle.trigger();
        // Returning ownership-of-nothing here proves the handle
        // moved into the thread; if it weren't Send the compiler
        // would have rejected this.
    });
    t.join().unwrap();
    // No assert on wake_flag here — gated on rmw-cffi feature; this
    // test runs with the default feature set.
}

#[test]
fn test_wake_short_circuits_drive_timeout() {
    // Pre-arming wake_flag should make spin_once skip its blocking
    // wait on drive_io (timeout collapses to 0) and return promptly,
    // even when the caller asked for a 200ms tick.
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    executor.wake();

    let start = std::time::Instant::now();
    let _ = executor.spin_once(core::time::Duration::from_millis(200));
    let elapsed = start.elapsed();
    assert!(
        elapsed < core::time::Duration::from_millis(50),
        "wake_flag set → spin_once must not wait 200ms; elapsed = {elapsed:?}",
    );
}

// ====================================================================
// Phase 49: HandleId / HandleSet / ReadinessSnapshot tests
// ====================================================================

#[test]
fn test_handle_id_from_add_subscription() {
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    let nid = executor
        .node_builder("test_handle_id_from_add_subscription")
        .build()
        .unwrap();
    let id = executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/a", |_msg: &TestMsg| {})
        .unwrap();
    assert_eq!(id, HandleId(0));

    let id2 = executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/b", |_msg: &TestMsg| {})
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
    let nid = executor
        .node_builder("test_trigger_any_fires_on_data")
        .build()
        .unwrap();

    executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/test", |_msg: &TestMsg| {})
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
    let nid = executor
        .node_builder("test_trigger_any_no_data_no_dispatch")
        .build()
        .unwrap();

    executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/test", |_msg: &TestMsg| {})
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
    let nid = executor
        .node_builder("test_trigger_always_fires_without_data")
        .build()
        .unwrap();

    let called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let called2 = called.clone();
    let id = executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/test", move |_msg: &TestMsg| {
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

    let nid = executor
        .node_builder("test_trigger_one_fires_on_specific_handle")
        .build()
        .unwrap();
    let _id0 = executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/topic0", |_: &TestMsg| {})
        .unwrap();
    let id1 = executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/topic1", |_: &TestMsg| {})
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

    let nid = executor
        .node_builder("test_trigger_predicate")
        .build()
        .unwrap();
    executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/test", |_: &TestMsg| {})
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
        .register_guard_condition(move || {
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
        .register_guard_condition(move || {
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
        .add_arena_subscription_c_callback::<{ crate::config::DEFAULT_RX_BUF_SIZE }>(
            None,
            "/test",
            "test/msg/TestMsg",
            "test_hash",
            QosSettings::default().keep_last(1),
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

#[test]
fn test_raw_subscription_info_callback() {
    // Phase 189.M3.4 — the C-fn-ptr-with-attachment subscription path.
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    static INFO_CALLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    static INFO_LEN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    static INFO_ATT_LEN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    unsafe extern "C" fn info_cb(
        _data: *const u8,
        len: usize,
        _att: *const u8,
        att_len: usize,
        _ctx: *mut core::ffi::c_void,
    ) {
        INFO_CALLED.store(true, std::sync::atomic::Ordering::SeqCst);
        INFO_LEN.store(len, std::sync::atomic::Ordering::SeqCst);
        INFO_ATT_LEN.store(att_len, std::sync::atomic::Ordering::SeqCst);
    }

    let _id = executor
        .add_arena_subscription_c_info_callback::<{ crate::config::DEFAULT_RX_BUF_SIZE }>(
            None,
            "/test",
            "test/msg/TestMsg",
            "test_hash",
            QosSettings::default().keep_last(1),
            info_cb,
            core::ptr::null_mut(),
        )
        .unwrap();

    let (data, len) = encode_test_msg(7);
    let meta = executor.entries[0].as_ref().unwrap();
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    unsafe {
        let sub_ptr = arena_ptr.add(meta.offset) as *const MockSubscriber;
        (*sub_ptr).load(data, len);
    }

    let result = executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(result.subscriptions_processed, 1);
    assert!(INFO_CALLED.load(std::sync::atomic::Ordering::SeqCst));
    assert_eq!(INFO_LEN.load(std::sync::atomic::Ordering::SeqCst), len);
    // MockSubscriber has no native attachment ⇒ default 0-length attachment.
    assert_eq!(INFO_ATT_LEN.load(std::sync::atomic::Ordering::SeqCst), 0);
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

    let nid = executor
        .node_builder("test_set_invocation_mode")
        .build()
        .unwrap();
    let id = executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/test", |_: &TestMsg| {})
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
    let nid = executor
        .node_builder("test_let_semantics_pre_samples_data")
        .build()
        .unwrap();

    let received = std::sync::Arc::new(std::sync::Mutex::new(None));
    let received2 = received.clone();
    executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/test", move |msg: &TestMsg| {
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
        .add_arena_subscription_c_callback::<{ crate::config::DEFAULT_RX_BUF_SIZE }>(
            None,
            "/test",
            "test/msg/TestMsg",
            "test_hash",
            QosSettings::default().keep_last(1),
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
        .register_timer(TimerDuration::from_millis(100), || {})
        .unwrap();
    let nid = executor
        .node_builder("test_trigger_all_with_mixed_handles")
        .build()
        .unwrap();
    let _sub_id = executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/test", |_: &TestMsg| {})
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

    let nid = executor
        .node_builder("test_trigger_allof_fires_when_both_ready")
        .build()
        .unwrap();
    let id_a = executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/sensor_a", |_: &TestMsg| {})
        .unwrap();
    let id_b = executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/sensor_b", |_: &TestMsg| {})
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

    let nid = executor
        .node_builder("test_trigger_allof_empty_set_always_fires")
        .build()
        .unwrap();
    executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/test", |_: &TestMsg| {})
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

    let nid = executor
        .node_builder("test_trigger_anyof_fires_when_one_ready")
        .build()
        .unwrap();
    let id_a = executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/topic_a", |_: &TestMsg| {})
        .unwrap();
    let id_b = executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/topic_b", |_: &TestMsg| {})
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

    let nid = executor
        .node_builder("test_trigger_anyof_empty_set_never_fires")
        .build()
        .unwrap();
    executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/test", |_: &TestMsg| {})
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
        .register_timer(TimerDuration::from_millis(100), move || {
            count2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })
        .unwrap();
    let nid = executor
        .node_builder("test_timer_delta_accumulates_when_trigger_fails")
        .build()
        .unwrap();
    let sub_id = executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/test", |_: &TestMsg| {})
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

#[cfg(rmw_cyclonedds_present)]
impl nros_serdes::schema::Message for TestServiceRequest {
    const TYPE_NAME: &'static str = "test/srv/TestService_Request";
    const FIELDS: &'static [nros_serdes::schema::Field] = &[nros_serdes::schema::Field {
        name: "a",
        ty: nros_serdes::schema::FieldType::Int32,
        offset: 0,
    }];
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

#[cfg(rmw_cyclonedds_present)]
impl nros_serdes::schema::Message for TestServiceReply {
    const TYPE_NAME: &'static str = "test/srv/TestService_Reply";
    const FIELDS: &'static [nros_serdes::schema::Field] = &[nros_serdes::schema::Field {
        name: "sum",
        ty: nros_serdes::schema::FieldType::Int32,
        offset: 0,
    }];
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

#[test]
fn test_service_builder_qos() {
    // Phase 193.2 — NodeCtx service builder + convenient create_service.
    let mut exec: Executor = Executor::from_session(MockSession::new());
    let id = exec.node_builder("n").build().unwrap();

    // convenient (fork tier) — default services QoS
    let _h = exec
        .node_mut(id)
        .create_service::<TestService, _>("/svc", |req: &TestServiceRequest| TestServiceReply {
            sum: req.a,
        })
        .expect("convenient service builds");

    // builder (clone tier) — explicit QoS
    let _h2 = exec
        .node_mut(id)
        .service("/svc2")
        .qos(QosSettings::default().reliable().keep_last(5))
        .build::<TestService, _>(|req: &TestServiceRequest| TestServiceReply { sum: req.a })
        .expect("service builder with qos builds");
}

#[test]
fn test_node_service_client_with_qos() {
    // Phase 193.2b — Node session-path create_service_with_qos /
    // create_client_with_qos (rclcpp-style qos overload).
    let mut executor: Executor = Executor::from_session(MockSession::new());
    let mut node = executor.create_node("n").unwrap();
    let q = QosSettings::default().reliable().keep_last(7);
    let _srv = node
        .create_service_with_qos::<TestService>("/svc", q)
        .expect("service with qos");
    let _cli = node
        .create_client_with_qos::<TestService>("/svc", q)
        .expect("client with qos");
}

/// RFC-0041 / Phase 239.4 — a callback-based service client delivers the reply
/// to its typed closure at `spin_once` (no `Promise::try_recv`). Drives the full
/// `call` → inject-reply → dispatch → callback path through the executor with the
/// mock transport.
#[test]
fn test_service_client_callback_fires_at_spin() {
    use crate::executor::arena::ServiceClientSendHeader;

    let mut executor: Executor = Executor::from_session(MockSession::new());
    let nid = executor.node_builder("svc_cb").build().unwrap();

    let got = std::sync::Arc::new(std::sync::Mutex::new(None));
    let got2 = got.clone();
    let mut client = executor
        .node_mut(nid)
        .create_client_with_callback::<TestService, _>("/svc", move |reply: &TestServiceReply| {
            *got2.lock().unwrap() = Some(reply.sum);
        })
        .unwrap();

    // Send a request → `pending = true` (the mock send is a no-op).
    client.call(&TestServiceRequest { a: 7 }).unwrap();
    assert_eq!(*got.lock().unwrap(), None, "no reply delivered yet");

    // Inject a reply into the arena entry's mock client. The send header is the
    // first field of the typed entry, so it sits at the entry offset.
    let off = executor.entries[0].as_ref().unwrap().offset;
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    let hdr = unsafe {
        &*(arena_ptr.add(off)
            as *const ServiceClientSendHeader<{ crate::config::DEFAULT_RX_BUF_SIZE }>)
    };
    let mut buf = [0u8; 256];
    let mut writer = CdrWriter::new_with_header(&mut buf).unwrap();
    writer.write_i32(14).unwrap();
    let len = writer.position();
    let mut reply = [0u8; 256];
    reply[..len].copy_from_slice(&buf[..len]);
    hdr.handle.load_reply(reply, len);

    // Spin → dispatcher drains the reply, deserializes `TestServiceReply`, fires
    // the typed callback.
    let result = executor.spin_once(core::time::Duration::from_millis(0));
    assert!(result.any_work(), "spin did work");
    assert_eq!(
        *got.lock().unwrap(),
        Some(14),
        "callback delivered the deserialized reply"
    );
}

/// RFC-0041 / Phase 239.4 — a callback-based action client delivers
/// goal-response / feedback / result to typed closures at `spin_once`. Drives
/// each receive by injecting into the entry's mock channels and spinning.
#[test]
fn test_action_client_callbacks_fire_at_spin() {
    use crate::executor::action_core::ActionClientCore;

    /// CDR-with-header encode of a single i32 (mirrors a `{ i32 }` message).
    fn encode_i32_cdr(v: i32) -> ([u8; 256], usize) {
        let mut b = [0u8; 256];
        let mut w = CdrWriter::new_with_header(&mut b).unwrap();
        w.write_i32(v).unwrap();
        let n = w.position();
        drop(w);
        (b, n)
    }
    fn buf256(src: &[u8]) -> ([u8; 256], usize) {
        let mut b = [0u8; 256];
        b[..src.len()].copy_from_slice(src);
        (b, src.len())
    }

    let mut executor: Executor = Executor::from_session(MockSession::new());
    let nid = executor.node_builder("act_cb").build().unwrap();

    let goal_resp = std::sync::Arc::new(std::sync::Mutex::new(None));
    let feedback = std::sync::Arc::new(std::sync::Mutex::new(None));
    let result = std::sync::Arc::new(std::sync::Mutex::new(None));
    let (gr2, fb2, rs2) = (goal_resp.clone(), feedback.clone(), result.clone());

    let mut client = executor
        .node_mut(nid)
        .create_action_client_with_callbacks::<TestAction, _, _, _>(
            "/act",
            move |id: &nros_core::GoalId, accepted: bool| {
                let mut c = [0u8; 8];
                c.copy_from_slice(&id.uuid[..8]);
                *gr2.lock().unwrap() = Some((u64::from_le_bytes(c), accepted));
            },
            move |_id: &nros_core::GoalId, fb: &TestFeedback| {
                *fb2.lock().unwrap() = Some(fb.progress);
            },
            move |_id: &nros_core::GoalId, st: nros_core::GoalStatus, r: &TestResult| {
                *rs2.lock().unwrap() = Some((st, r.value));
            },
        )
        .unwrap();

    // Reach the entry's core (first field of the entry → entry offset).
    let off = executor.entries[0].as_ref().unwrap().offset;
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    let core = unsafe {
        &*(arena_ptr.add(off)
            as *const ActionClientCore<
                { crate::config::DEFAULT_RX_BUF_SIZE },
                { crate::config::DEFAULT_RX_BUF_SIZE },
                { crate::config::DEFAULT_RX_BUF_SIZE },
            >)
    };

    // send_goal → goal_counter = 1.
    let goal_id = client.send_goal(&TestGoal { order: 1 }).unwrap();

    // 1. Goal-response: header(4) + accepted(1). `try_recv_send_goal_reply`
    //    copies it into `result_buffer`; the dispatcher reads byte 4.
    let (gr, grl) = buf256(&[0, 1, 0, 0, 1]);
    core.send_goal_client.load_reply(gr, grl);
    executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(
        *goal_resp.lock().unwrap(),
        Some((1u64, true)),
        "goal-response"
    );

    // 2. Feedback: outer header(4) + GoalId(16) + inner CDR(header + i32).
    let mut fb = [0u8; 256];
    fb[0..4].copy_from_slice(&[0, 1, 0, 0]);
    fb[4..12].copy_from_slice(&1u64.to_le_bytes()); // GoalId uuid[..8] = counter 1
    let (inner, il) = encode_i32_cdr(7);
    fb[20..20 + il].copy_from_slice(&inner[..il]);
    core.feedback_subscriber.load(fb, 20 + il);
    executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(*feedback.lock().unwrap(), Some(7), "feedback deserialized");

    // 3. Result: header(4) + status@4 + pad(3) + inner CDR(header + i32) @8.
    client.get_result(&goal_id).unwrap();
    let mut rs = [0u8; 256];
    rs[0..4].copy_from_slice(&[0, 1, 0, 0]);
    rs[4] = 4; // GoalStatus::Succeeded
    let (inner_r, irl) = encode_i32_cdr(99);
    rs[8..8 + irl].copy_from_slice(&inner_r[..irl]);
    core.get_result_client.load_reply(rs, 8 + irl);
    executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(
        *result.lock().unwrap(),
        Some((nros_core::GoalStatus::Succeeded, 99)),
        "result deserialized"
    );
}

/// Phase 189.M3.3.d — runtime proof that a **service** bound to a sched context
/// honours it in `spin_once`: two services bound to EDF contexts dispatch in
/// deadline order, not registration order. This is the runtime payoff of M3.3 —
/// services (now arena-registered + sched-bindable across C/C++) ride the same
/// SC-ordered dispatch as subscriptions (mirrors `test_edf_dispatch_order`).
#[test]
fn test_service_dispatch_respects_sched_context() {
    use crate::executor::sched_context::{DeadlinePolicy, OptUs, SchedClass, SchedContext};
    let session = MockSession::new();
    let mut executor: Executor = Executor::from_session(session);

    let firing_order = std::sync::Arc::new(std::sync::Mutex::new(std::vec::Vec::<i32>::new()));
    let order_late = firing_order.clone();
    let order_early = firing_order.clone();

    let nid = executor
        .node_builder("test_service_dispatch_respects_sched_context")
        .build()
        .unwrap();

    // Registered first (lower DescIdx → FIFO-first) → bind to the LATER deadline.
    let h_late = executor
        .node_mut(nid)
        .create_service::<TestService, _>("/late", move |req: &TestServiceRequest| {
            order_late.lock().unwrap().push(req.a);
            TestServiceReply { sum: req.a }
        })
        .unwrap();
    // Registered second → bind to the EARLIER deadline so EDF promotes it.
    let h_early = executor
        .node_mut(nid)
        .create_service::<TestService, _>("/early", move |req: &TestServiceRequest| {
            order_early.lock().unwrap().push(req.a);
            TestServiceReply { sum: req.a }
        })
        .unwrap();

    let sc_late = executor
        .create_sched_context(SchedContext {
            class: SchedClass::Edf,
            deadline_us: OptUs::from_us(1000),
            deadline_policy: DeadlinePolicy::Activated,
            ..Default::default()
        })
        .unwrap();
    let sc_early = executor
        .create_sched_context(SchedContext {
            class: SchedClass::Edf,
            deadline_us: OptUs::from_us(100),
            deadline_policy: DeadlinePolicy::Activated,
            ..Default::default()
        })
        .unwrap();
    executor
        .bind_handle_to_sched_context(h_late, sc_late)
        .unwrap();
    executor
        .bind_handle_to_sched_context(h_early, sc_early)
        .unwrap();

    // Load a request into each mock server (req.a identifies which fired).
    let (d_late, n_late) = encode_test_msg(10);
    let (d_early, n_early) = encode_test_msg(20);
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    let off_late = executor.entries[0].as_ref().unwrap().offset;
    let off_early = executor.entries[1].as_ref().unwrap().offset;
    unsafe { &*(arena_ptr.add(off_late) as *const MockServiceServer) }.load(d_late, n_late);
    unsafe { &*(arena_ptr.add(off_early) as *const MockServiceServer) }.load(d_early, n_early);

    let _ = executor.spin_once(core::time::Duration::from_millis(0));

    let order = firing_order.lock().unwrap();
    // Earlier-deadline (req.a=20) must precede later-deadline (req.a=10).
    assert_eq!(*order, std::vec![20, 10]);
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
        .register_timer(TimerDuration::from_millis(50), || {
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

// ====================================================================
// Phase 172.K.5 — explicit per-node session-slot selection
// ====================================================================

/// `NodeBuilder::session_idx(n)` binds a Node directly to a pre-opened
/// session slot (the multi-domain routing primitive), bypassing rmw
/// resolution, and validates the slot against the opened set.
#[test]
fn node_builder_session_idx_binds_explicit_slot_and_validates() {
    let mut executor: Executor = Executor::from_session(MockSession::new());
    // Simulate `open_multi` having opened one extra session (slot 1).
    assert!(executor.extra_sessions.push(MockSession::new()).is_ok());

    // Slot 0 = primary.
    let n0 = executor.node_builder("n0").session_idx(0).build().unwrap();
    assert_eq!(executor.node(n0).unwrap().session_idx, 0);

    // Slot 1 = the extra session.
    let n1 = executor.node_builder("n1").session_idx(1).build().unwrap();
    assert_eq!(executor.node(n1).unwrap().session_idx, 1);

    // Out-of-range slot (only 0 + 1 exist) → error, not a silent bad bind.
    assert!(executor.node_builder("bad").session_idx(2).build().is_err());
}

// ============================================================================
// Phase 237 — deferred get_result (concurrent-safe seq routing)
// ============================================================================

/// Two concurrently-active goals each have a `get_result` request arrive while
/// they are still executing. The server must HOLD both replies (rclcpp_action
/// sends get_result right after acceptance) and, on completion, reply to each
/// using ITS OWN correlation token — never cross-wire them. This is the
/// backend-agnostic heart of Option A; the seq routing lives in
/// `ActionServerCore`, shared by the XRCE / Zenoh / Cyclone backends.
#[test]
fn test_get_result_deferred_per_goal_concurrent() {
    use super::action_core::{ActionServerCore, RawActiveGoal};
    use crate::mock::MockPublisher;
    use nros_core::{GoalId, GoalStatus};

    let mut core: ActionServerCore = ActionServerCore::from_channels(
        MockServiceServer::new(),
        MockServiceServer::new(),
        MockServiceServer::new(),
        MockPublisher,
        MockPublisher,
    );

    let g1 = GoalId { uuid: [1u8; 16] };
    let g2 = GoalId { uuid: [2u8; 16] };
    let _ = core.active_goals.push(RawActiveGoal {
        goal_id: g1,
        status: GoalStatus::Executing,
    });
    let _ = core.active_goals.push(RawActiveGoal {
        goal_id: g2,
        status: GoalStatus::Executing,
    });

    // A get_result request is [CDR-LE header][fixed uint8[16] goal_id].
    let mk_req = |g: &GoalId| -> ([u8; 256], usize) {
        let mut b = [0u8; 256];
        b[..4].copy_from_slice(&[0x00, 0x01, 0x00, 0x00]);
        b[4..20].copy_from_slice(&g.uuid);
        (b, 20)
    };
    let default_result = [0u8; 4];

    // g1's get_result arrives first (mock seq 0), then g2's (mock seq 1).
    let (r1, l1) = mk_req(&g1);
    core.get_result_server.load(r1, l1);
    core.try_handle_get_result_raw(&default_result).unwrap();
    let (r2, l2) = mk_req(&g2);
    core.get_result_server.load(r2, l2);
    core.try_handle_get_result_raw(&default_result).unwrap();

    // Both deferred; nothing replied while the goals are active.
    assert_eq!(core.pending_get_results.len(), 2);
    assert_eq!(core.get_result_server.sent.borrow().len(), 0);

    // Reply layout: [4-byte CDR header][i8 status][3 pad][result CDR] → result
    // bytes begin at offset 8.
    const RESULT_OFF: usize = 8;

    // Complete g2 FIRST (out of arrival order) → flush only g2's held request,
    // with g2's token (seq 1) — not g1's.
    let res2 = [0xBBu8; 8];
    core.complete_goal_raw(&g2, GoalStatus::Succeeded, &res2);
    {
        let sent = core.get_result_server.sent.borrow();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, 1, "g2 reply must use g2's correlation token");
        assert_eq!(sent[0].1[4], GoalStatus::Succeeded as u8 as i8 as u8);
        assert_eq!(&sent[0].1[RESULT_OFF..RESULT_OFF + res2.len()], &res2);
    }
    assert_eq!(core.pending_get_results.len(), 1);

    // Complete g1 → flush g1's held request with g1's token (seq 0).
    let res1 = [0xAAu8; 8];
    core.complete_goal_raw(&g1, GoalStatus::Succeeded, &res1);
    {
        let sent = core.get_result_server.sent.borrow();
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[1].0, 0, "g1 reply must use g1's token, not g2's");
        assert_eq!(&sent[1].1[RESULT_OFF..RESULT_OFF + res1.len()], &res1);
    }
    assert_eq!(core.pending_get_results.len(), 0);
}

/// A `get_result` that arrives AFTER the goal already terminated is answered
/// immediately from the completed-results slab (the nano-ros↔nano-ros path),
/// never entering the pending table.
#[test]
fn test_get_result_after_completion_replies_immediately() {
    use super::action_core::ActionServerCore;
    use crate::mock::MockPublisher;
    use nros_core::{GoalId, GoalStatus};

    let mut core: ActionServerCore = ActionServerCore::from_channels(
        MockServiceServer::new(),
        MockServiceServer::new(),
        MockServiceServer::new(),
        MockPublisher,
        MockPublisher,
    );

    let g = GoalId { uuid: [7u8; 16] };
    // Goal completes before any get_result arrives.
    let res = [0xCCu8; 8];
    core.complete_goal_raw(&g, GoalStatus::Succeeded, &res);
    assert_eq!(core.pending_get_results.len(), 0);
    assert_eq!(core.get_result_server.sent.borrow().len(), 0);

    let mut b = [0u8; 256];
    b[..4].copy_from_slice(&[0x00, 0x01, 0x00, 0x00]);
    b[4..20].copy_from_slice(&g.uuid);
    core.get_result_server.load(b, 20);
    core.try_handle_get_result_raw(&[0u8; 4]).unwrap();

    // Replied immediately, not deferred.
    assert_eq!(core.pending_get_results.len(), 0);
    let sent = core.get_result_server.sent.borrow();
    assert_eq!(sent.len(), 1);
    assert_eq!(&sent[0].1[8..8 + res.len()], &res);
}

use super::*;
#[cfg(feature = "param-store")]
use nros_params::ParameterServerOps as _; // phase-515 R10 stage 2

/// An executor over `session` with a HOST clock installed.
///
/// phase-359 W10 — the core has no `std::time` fallback any more: the platform
/// API is the clock, so a build with no port (which the mock-session
/// configuration is, by construction — `mock` is `cfg(not(rmw-cffi))`) has none
/// and says so. A test that measures elapsed time therefore brings its own,
/// which is the same seam a board uses (`ExecutorConfig::clock_us`) and the
/// reason `from_session_with` exists (issue 0709).
///
/// `std::time::Instant` here is not a flavour leak: this file is test code, the
/// census excludes it, and a hosted test harness reading the host clock is
/// exactly right. What was wrong was the CORE offering the same thing as a
/// silent default.
/// Issue 1104 — serialize the tests that touch `time_source`'s process-global
/// and put back what was there.
///
/// `SIM_TIME_ACTIVE` is one flag for the whole process while an `Executor` is
/// one per test, so two tests that both care about it cannot run concurrently
/// and cannot leave it changed. This gives them both properties: the mutex
/// orders them, and the drop restores the value OBSERVED ON ENTRY rather than a
/// guessed default -- the previous cleanup wrote a literal `true`, which is only
/// right by coincidence and says nothing if the default ever moves.
///
/// The lock is only half the fix. It orders the tests that MEAN to touch the
/// flag; what stopped every other test from touching it by accident is
/// the executor's `sim_time_stated` field (an executor nobody told about
/// `use_sim_time` writes nothing), because `reconcile_ros_time_source` runs on
/// every `spin_once` of every executor.
#[cfg(feature = "sim-time")]
struct SimTimeGuard {
    was_active: bool,
    _lock: std::sync::MutexGuard<'static, ()>,
}

#[cfg(feature = "sim-time")]
impl SimTimeGuard {
    fn acquire() -> Self {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        // A poisoned lock means a sibling test panicked while holding it. That
        // is a failure being reported elsewhere, not a reason to fail here too,
        // so take the guard anyway and let the real failure be the one read.
        let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        Self {
            was_active: crate::time_source::is_active(),
            _lock,
        }
    }
}

#[cfg(feature = "sim-time")]
impl Drop for SimTimeGuard {
    fn drop(&mut self) {
        crate::time_source::set_active(self.was_active);
        nros_core::clock::Clock::clear_ros_time_override();
    }
}

fn test_clock_us() -> u64 {
    use std::{sync::OnceLock, time::Instant};
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    EPOCH.get_or_init(Instant::now).elapsed().as_micros() as u64
}

fn executor_with_clock(session: MockSession) -> Executor<'static> {
    executor_with_clock_fn(session, test_clock_us)
}

/// `executor_with_clock` with the clock named at the call site.
///
/// The default one above is deliberately SHARED — see `private_test_clock!`
/// for when it must not be.
fn executor_with_clock_fn(session: MockSession, clock: fn() -> u64) -> Executor<'static> {
    let cfg = ExecutorConfig::default().clock_us(clock);
    Executor::from_session_with(session, &cfg)
}

/// Declare a monotonic µs clock PRIVATE to one test — issue 1105.
///
/// `test_clock_us` is FREE-RUNNING and shared by every test in the process:
/// on `std` its epoch is a `OnceLock` initialised by whichever test ran first,
/// on `alloc` it is a counter advanced by whoever read it last. Either way its
/// value at any point in a test is *how much of the suite ran before that
/// test*, so a test asserting WHERE in a cyclic schedule "now" falls
/// (`now_us % major_frame_us`) is measuring the suite's runtime and not the
/// scheduler. That is the whole of issue 1105: the assertion was true solo and
/// under the default parallel mode, and false 3-of-3 under `--test-threads=1`,
/// for reasons entirely outside the code under test.
///
/// Nothing in `executor_with_clock`'s name warns an author about that, which
/// is why the fix is a clock rather than a one-off phase alignment: a window
/// test that reaches for `private_test_clock!` gets a clock whose value it
/// SETS, so its phase is a constant of the test.
///
/// One invocation per test. Each expands to its own module, so each gets its
/// own static and its own `fn() -> u64` — no lock, no ordering between tests,
/// nothing shared to be disturbed. `claim_at_us` panics on a second claim
/// because two tests sharing one clock is exactly the coupling this removes,
/// and it must not be silent.
///
/// ```ignore
/// private_test_clock!(my_window_clock);
///
/// #[test]
/// fn my_window_test() {
///     my_window_clock::claim_at_us(0);
///     let mut ex = executor_with_clock_fn(MockSession::new(), my_window_clock::now_us);
///     // ... then `my_window_clock::set_us(x)` to put "now" wherever the
///     // assertion needs it.
/// }
/// ```
macro_rules! private_test_clock {
    ($name:ident) => {
        // A clock is declared beside its test, and some of those tests are
        // feature-gated where the declaration is not; an unused clock is not
        // a defect, so it must not fail the build that drops its test.
        #[allow(dead_code)]
        mod $name {
            use portable_atomic::{AtomicBool, AtomicU64, Ordering};

            static NOW_US: AtomicU64 = AtomicU64::new(0);
            static CLAIMED: AtomicBool = AtomicBool::new(false);

            /// The `fn() -> u64` to hand `executor_with_clock_fn`.
            pub fn now_us() -> u64 {
                NOW_US.load(Ordering::SeqCst)
            }

            /// Claim this clock for the calling test and set its starting
            /// phase. A second claim is a hard error, not a shrug: it would
            /// re-create issue 1105 one clock down.
            pub fn claim_at_us(start_us: u64) {
                assert!(
                    !CLAIMED.swap(true, Ordering::SeqCst),
                    concat!(
                        "private clock `",
                        stringify!($name),
                        "` is already claimed — one private clock serves exactly one \
                         test (issue 1105); declare another with private_test_clock!"
                    )
                );
                NOW_US.store(start_us, Ordering::SeqCst);
            }

            /// Move "now". Monotonic by contract — a test asserts a phase, it
            /// does not rewind time — so a backwards step is a hard error.
            pub fn set_us(now_us: u64) {
                let prev = NOW_US.swap(now_us, Ordering::SeqCst);
                assert!(
                    now_us >= prev,
                    concat!(
                        "private clock `",
                        stringify!($name),
                        "` must not go backwards"
                    )
                );
            }

            /// Move "now" forward by `ms`. Also what a callback calls to
            /// COST time: the executor measures a callback's runtime on this
            /// clock, so an advance inside one is a runtime of exactly `ms`,
            /// where a real sleep is "at least `ms`, plus whatever the host
            /// adds".
            pub fn advance_ms(ms: u64) {
                NOW_US.fetch_add(ms * 1_000, Ordering::SeqCst);
            }

            /// Advance this clock by exactly `ms`, then `spin_once(0)` —
            /// issues 1944/1950.
            ///
            /// This used to be ONE shared helper that SLEPT `ms` on the wall
            /// clock, so every step it took was "at least `ms`": a loaded host
            /// added its own delay on top, which fired a 100 ms timer after a
            /// "50 ms" step (1944) and took 11 ms off a "60 ms remaining" floor
            /// (1950). A step on this clock is exactly `ms`, and the spin
            /// itself costs nothing on it however long it takes for real, so a
            /// test may assert an EQUALITY — both bounds at once.
            pub fn elapse_then_spin_once(
                executor: &mut crate::executor::Executor,
                ms: u64,
            ) -> crate::executor::types::SpinOnceResult {
                advance_ms(ms);
                executor.spin_once(core::time::Duration::from_millis(0))
            }
        }
    };
}

use nros_core::{
    CdrReader, CdrWriter, DeserError, Deserialize, DeserializeView, LeSliceView, RosAction,
    RosMessage, SerError, Serialize, ViewableMessage,
};
use nros_rmw::{QoSProfile, TransportError};

use crate::{
    mock::{MockServiceServer, MockSession, MockSubscriber},
    timer::TimerDuration,
};

// Issues 1944/1950 — there is deliberately NO module-level
// `elapse_then_spin_once`. The one that lived here slept `ms` on the wall
// clock against the shared `test_clock_us`, so "elapse 50 ms" meant "at least
// 50 ms, plus whatever a loaded host adds", and every timer assertion built on
// it held only while the host stayed quiet. `spin_once` credits the CLOCK's
// delta since the previous spin (phase 100), so the fix is the clock, not the
// sleep: a test that steps time declares a `private_test_clock!` and calls
// `<clock>::elapse_then_spin_once`, whose step is exact. Leaving no shared
// spelling is the point — a new test cannot reach the wall-clock version by
// habit, because it does not exist.

/// Issue 0355 — a live-but-idle session (`MockSession::drive_io` returns
/// `Ok(())` with no data) must NEVER accumulate `session_io_failures`, no matter
/// how many idle spins elapse. The C spin loops
/// (`rclc_executor_spin`/`rclc_executor_spin_period`) gate their dead-session
/// bail on this counter; the bug was that they instead counted `spin_some`'s
/// idle `RCL_RET_TIMEOUT`, so a healthy C listener that idled longer than
/// `SPIN_ERROR_TOLERANCE * period` before its publisher was discovered got
/// killed — the CycloneDDS ros2→nano 0-delivery defect. Only a genuine
/// `drive_io` error may raise the counter.
#[test]
fn idle_spins_never_raise_session_io_failures() {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);
    // Far more idle spins than the C-side SPIN_ERROR_TOLERANCE (16).
    for _ in 0..64 {
        let r = executor.spin_once(core::time::Duration::from_millis(0));
        assert!(!r.any_work(), "an idle mock session must do no work");
    }
    assert_eq!(
        executor.session_io_failures(),
        0,
        "idle spins over a live session must not accumulate io failures (issue 0355)"
    );
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
// phase-380 W4 — was `#[cfg(rmw_needs_type_descriptors)]`: the schema is
// now required by `MessageForRmw` on EVERY backend, because that is where
// a subscription's build-time size bound comes from.
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

/// Issue 1179 -- `TestMsg` as CODEGEN emits it: the same fields and schema,
/// plus the `RosMessage::MAX_SERIALIZED_BOUND` override every generated type
/// carries (`packs/nros/message.rs.jinja`). `TestMsg` itself stays the
/// hand-written shape, the one that keeps today's default.
#[derive(Debug, Clone, PartialEq)]
struct GeneratedMsg {
    data: i32,
}

impl RosMessage for GeneratedMsg {
    const TYPE_NAME: &'static str = "test/msg/GeneratedMsg";
    const TYPE_HASH: &'static str = "test_hash_generated";
    const MAX_SERIALIZED_BOUND: Option<usize> = nros_serdes::size::max_serialized_bound::<Self>();
}

impl nros_serdes::schema::Message for GeneratedMsg {
    const TYPE_NAME: &'static str = "test/msg/GeneratedMsg";
    const FIELDS: &'static [nros_serdes::schema::Field] = &[nros_serdes::schema::Field {
        name: "data",
        ty: nros_serdes::schema::FieldType::Int32,
        offset: 0,
    }];
}

impl Serialize for GeneratedMsg {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_i32(self.data)
    }
}

impl Deserialize for GeneratedMsg {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            data: reader.read_i32()?,
        })
    }
}

/// Issue 1179 -- a generated type is sized from its own bound on EVERY
/// backend: the answer reads `RosMessage`, so it does not depend on whether
/// `MessageForRmw` carries a schema (it does only under
/// `cfg(rmw_needs_type_descriptors)`, i.e. Cyclone, and this test runs on the
/// arm without it too). Before, this was `None` on zenoh and XRCE.
#[test]
fn a_generated_type_derives_its_receive_size_on_every_backend() {
    let bound = nros_serdes::size::max_serialized_bound::<GeneratedMsg>()
        .expect("an int32 message is bounded");
    let rx = crate::config::DEFAULT_RX_BUF_SIZE;
    assert!(
        bound < rx,
        "the fixture needs a bound below the default buffer"
    );
    assert_eq!(
        crate::rmw_type_registry::default_subscription_rx_bytes::<GeneratedMsg>(rx),
        Some(bound.next_multiple_of(4)),
        "the default receive size is the type's bound, framed to 4"
    );
    assert_eq!(
        crate::rmw_type_registry::subscription_rx_hint::<GeneratedMsg>(rx),
        bound,
        "the backend hint is the type's own bound"
    );
    assert!(crate::rmw_type_registry::subscription_buffer_ok::<
        GeneratedMsg,
    >());
}

/// Issue 1860 / phase-498 W1 -- a `.message_info()` subscription's slot is
/// sized from its type, as the buffered path's is, instead of reserving the
/// image's closure maximum inline. Compared registration against
/// registration, so the entry struct cancels out and the difference is
/// exactly the slot. `TestMsg` (hand-written, no bound) is the control: it
/// keeps `DEFAULT_RX_BUF_SIZE`, as before.
#[test]
fn a_message_info_subscription_is_sized_from_its_type() {
    let rx = crate::config::DEFAULT_RX_BUF_SIZE;
    let bound = nros_serdes::size::max_serialized_bound::<GeneratedMsg>()
        .expect("an int32 message is bounded");
    let framed = bound.next_multiple_of(4);
    assert!(
        framed < rx,
        "the fixture needs a bound below the default buffer"
    );

    let mut executor: Executor = executor_with_clock(MockSession::new());
    let nid = executor.node_builder("info_sizing").build().unwrap();
    let before = executor.arena_used();
    executor
        .node_mut(nid)
        .subscription("/generated")
        .typed::<GeneratedMsg>()
        .message_info()
        .build(|_: &GeneratedMsg, _: Option<&nros_core::MessageInfo>| {})
        .unwrap();
    let generated = executor.arena_used() - before;
    let before = executor.arena_used();
    executor
        .node_mut(nid)
        .subscription("/hand_written")
        .typed::<TestMsg>()
        .message_info()
        .build(|_: &TestMsg, _: Option<&nros_core::MessageInfo>| {})
        .unwrap();
    let hand_written = executor.arena_used() - before;
    // The arena hands out regions on its grain, so compare against the
    // difference a grain-rounded slot can make, not a raw subtraction.
    assert!(
        hand_written > generated,
        "a bounded type must claim less than the default slot: {generated} vs {hand_written}"
    );
    assert!(
        hand_written - generated >= rx - framed - super::spin::ARENA_GRAIN,
        "the saving is the slot's difference ({} B), up to one arena grain; got {} B",
        rx - framed,
        hand_written - generated
    );

    // `.rx_buffer::<N>()` is still verbatim: a named size is not re-derived.
    let before = executor.arena_used();
    executor
        .node_mut(nid)
        .subscription("/named")
        .typed::<GeneratedMsg>()
        .message_info()
        .rx_buffer::<{ crate::config::DEFAULT_RX_BUF_SIZE }>()
        .build(|_: &GeneratedMsg, _: Option<&nros_core::MessageInfo>| {})
        .unwrap();
    assert_eq!(
        executor.arena_used() - before,
        hand_written,
        "a named size claims exactly what the default-sized control claims"
    );
}

/// The other half: a hand-written type that states no bound keeps exactly
/// the caller's default, on every backend (phase-380 W4's promise to
/// `examples/linux/rust/custom-msg`).
#[test]
fn a_hand_written_type_without_a_bound_keeps_the_default() {
    let rx = crate::config::DEFAULT_RX_BUF_SIZE;
    assert_eq!(
        crate::rmw_type_registry::default_subscription_rx_bytes::<TestMsg>(rx),
        None
    );
    assert_eq!(
        crate::rmw_type_registry::subscription_rx_hint::<TestMsg>(rx),
        rx
    );
    assert!(crate::rmw_type_registry::subscription_buffer_ok::<TestMsg>());
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
// Phase 403 W2 -- the type's bound sizes the receive buffer
//
// These MEASURE, they do not merely compile: `arena_used()` is the exact
// claimed total, so the delta across one registration IS the subscription's
// receive-buffer footprint. The phase's own rule is that no wave is done
// until a before/after exists on something that RUNS the changed code, and
// the buffered registration path is what runs it -- a backend advertising
// `supports_process_in_place` (zenoh) returns before allocating any buffer
// at all and is therefore NOT a site where this saves anything.
// ====================================================================

/// A message with no bound at any size, for the fallback arm. `TestMsg` is
/// bounded (one `Int32`), so it cannot exercise it.
#[derive(Debug, Clone, PartialEq)]
struct UnboundedTestMsg {
    data: heapless::String<8>,
}

impl RosMessage for UnboundedTestMsg {
    const TYPE_NAME: &'static str = "test/msg/UnboundedTestMsg";
    const TYPE_HASH: &'static str = "test_hash_unbounded";
}

impl nros_serdes::schema::Message for UnboundedTestMsg {
    const TYPE_NAME: &'static str = "test/msg/UnboundedTestMsg";
    const FIELDS: &'static [nros_serdes::schema::Field] = &[nros_serdes::schema::Field {
        name: "data",
        ty: nros_serdes::schema::FieldType::String,
        offset: 0,
    }];
}

impl Serialize for UnboundedTestMsg {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_string(self.data.as_str())
    }
}

impl Deserialize for UnboundedTestMsg {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        let s = reader.read_string()?;
        let mut data = heapless::String::new();
        // Truncation is fine: nothing here inspects the payload.
        for ch in s.chars() {
            if data.push(ch).is_err() {
                break;
            }
        }
        Ok(Self { data })
    }
}

/// How a `TestMsg` subscription asks for its receive buffer -- the three states
/// phase-392 W3c makes distinguishable, and the reason this is an enum and not
/// the `bool` it was: "did not ask" and "asked for a number" used to mean the
/// same thing.
#[derive(Clone, Copy)]
enum RxAsk {
    /// Name nothing. Derived from the type wherever the bound is reachable.
    Default,
    /// `.rx_buffer::<4096>()` -- the opt-out, exact, and deliberately ABOVE
    /// the global so an opt-out that silently fell back is distinguishable.
    Exact,
    /// `.rx_buffer::<DEFAULT_RX_BUF_SIZE>()` -- the opt-out spelled with the
    /// global. phase-392 W3c: this is what "the global" costs now that it is a
    /// thing a consumer asks for rather than what everyone silently gets.
    Global,
    /// `.rx_buffer_from_type()` -- the explicit derivation, on every backend.
    FromType,
}

/// Arena bytes one `TestMsg` subscription registration claims.
fn arena_delta(
    executor: &mut Executor<'_>,
    nid: super::node_record::NodeId,
    topic: &str,
    qos: QoSProfile,
    ask: RxAsk,
) -> usize {
    let before = executor.arena_used();
    match ask {
        RxAsk::FromType => {
            executor
                .node_mut(nid)
                .subscription(topic)
                .typed::<TestMsg>()
                .qos(qos)
                .rx_buffer_from_type()
                .build(|_: &TestMsg| {})
                .unwrap();
        }
        RxAsk::Exact => {
            executor
                .node_mut(nid)
                .subscription(topic)
                .typed::<TestMsg>()
                .qos(qos)
                .rx_buffer::<4096>()
                .build(|_: &TestMsg| {})
                .unwrap();
        }
        RxAsk::Global => {
            executor
                .node_mut(nid)
                .subscription(topic)
                .typed::<TestMsg>()
                .qos(qos)
                .rx_buffer::<{ crate::config::DEFAULT_RX_BUF_SIZE }>()
                .build(|_: &TestMsg| {})
                .unwrap();
        }
        RxAsk::Default => {
            executor
                .node_mut(nid)
                .subscription(topic)
                .typed::<TestMsg>()
                .qos(qos)
                .build(|_: &TestMsg| {})
                .unwrap();
        }
    }
    executor.arena_used() - before
}

fn qos_with_depth(depth: u32) -> QoSProfile {
    QoSProfile {
        depth,
        ..QoSProfile::default()
    }
}

/// phase-392 W3c -- what a subscription that names NO number is charged.
///
/// This test was ~~`the_default_subscription_buffer_is_unchanged`~~ and asserted the
/// opposite: that a non-opted-in subscription still paid `DEFAULT_RX_BUF_SIZE`
/// per slot. W3c is the decision to invert that -- `.rx_buffer_from_type()`'s
/// own doc named it ("sizing every subscription from its type by default would
/// move every image's arena occupancy at once, which is a decision, not a
/// refactor") -- so the assertion moves with it rather than being deleted.
///
/// It is written against
/// [`default_subscription_rx_bytes`](crate::rmw_type_registry::default_subscription_rx_bytes)
/// rather than a literal because that function has TWO arms and both are
/// correct: it derives where `MessageForRmw` carries a schema (a backend
/// declaring `type-descriptors`) and answers `None` where it does not, and the
/// wiring under test -- "the choke point charges exactly what the default
/// derivation says" -- is the same claim either way. arm_note puts the arm in
/// the failure message so a red is never ambiguous about which one ran.
///
/// Two depths rather than one absolute number, because the entry STRUCT's size
/// is a target detail this test has no business asserting while the SLOT size is
/// exactly what it must pin. `region_size` is linear in the slot size with a
/// depth-dependent slot count, so the difference between two depths isolates it.
#[test]
fn the_default_subscription_buffer_is_derived_from_the_type() {
    let default = crate::config::DEFAULT_RX_BUF_SIZE;
    let derived = crate::rmw_type_registry::default_subscription_rx_bytes::<TestMsg>(default);

    let mut executor: Executor = executor_with_clock(MockSession::new());
    let nid = executor.node_builder("default_sizing").build().unwrap();
    let depth = qos_with_depth(2);

    let named_nothing = arena_delta(&mut executor, nid, "/nothing", depth, RxAsk::Default);
    let named_global = arena_delta(&mut executor, nid, "/global", depth, RxAsk::Global);
    let named_type = arena_delta(&mut executor, nid, "/from_type", depth, RxAsk::FromType);

    // Costs are compared against a REGISTRATION rather than against
    // `buffered_region_size`, because the arena aligns what it hands out and a
    // 12-byte slot is not aligned: the raw region formula is off by the padding
    // at small slot sizes and exact at 1024, which would make this pass on one
    // arm for the wrong reason.
    match derived {
        Some(bound) => {
            assert_eq!(
                named_nothing, named_type,
                "with a schema on `MessageForRmw`, naming nothing must cost \
                 exactly what `.rx_buffer_from_type()` costs -- the derivation \
                 IS the default (bound {bound})"
            );
            assert!(
                named_nothing < named_global,
                "and that must be a saving against the global: naming nothing \
                 claimed {named_nothing} bytes, `.rx_buffer::<{default}>()` \
                 claimed {named_global}"
            );
        }
        None => {
            assert_eq!(
                named_nothing, named_global,
                "without a schema on `MessageForRmw` there is no bound to read \
                 at a type-erased registration site, so naming nothing must \
                 still cost the global {default}"
            );
            assert!(
                named_type < named_nothing,
                "and `.rx_buffer_from_type()`, which names the schema itself, \
                 must still be the cheaper answer on this arm \
                 ({named_type} vs {named_nothing})"
            );
        }
    }
}

/// An executor whose ARENA is exactly `arena` bytes, everything else default.
///
/// [`executor_with_clock`] takes `ExecutorSizing::DEFAULT`, whose arena is the
/// build's derived `ARENA_SIZE`. A test that asks *does what the DERIVATION
/// budgeted hold what the REGISTRATION claims* has to be able to state the
/// budget, which is the only thing this adds.
fn executor_with_arena(session: MockSession, arena: usize) -> Executor<'static> {
    let sizing = super::storage::ExecutorSizing {
        arena,
        ..super::storage::ExecutorSizing::DEFAULT
    };
    let backing: &'static mut [core::mem::MaybeUninit<u64>] =
        alloc::boxed::Box::leak(alloc::boxed::Box::new_uninit_slice(sizing.u64_len()));
    // SAFETY: `backing` is exactly `sizing.u64_len()` words, `'static`, and is
    // handed to this executor alone -- nothing else holds the leaked slice.
    unsafe { Executor::from_session_in(session, backing, sizing) }
}

/// **Issue 1319, reproduced** — phase-454 W5.b.
///
/// The issue was analysis-only: *"Not reproduced as a failing image … An image
/// that declares its entities, subscribes from RUST on zenoh, and sets
/// `NROS_SUBSCRIBER_BUFFER_SIZE` below `NROS_SUBSCRIPTION_BUFFER_SIZE` is the
/// shape that would show it, as `NodeError::BufferTooSmall` at a registration
/// the arena oracle passed."* This is that shape, with the two build-time
/// numbers stated as arenas so the whole thing fits in one process.
///
/// * **the image declares its entities** — so `arena_model::REQUIRED` is the
///   per-endpoint SUM and not the worst-case budget. Here that is the arena
///   this test hands the executor.
/// * **subscribes from Rust** — `.typed::<TestMsg>().build(cb)`, naming no
///   buffer. The default path, which is what a declarative node emits.
/// * **on a SCHEMALESS backend that buffers** — `MessageForRmw` carries no
///   schema, which is `cfg(not(rmw_needs_type_descriptors))` and is why this
///   test is gated on it, and the handle does not advertise in-place dispatch,
///   which `MockSubscriber` does not. On the descriptor arm the registration
///   reaches the type's own bound and the model was already right;
///   `the_default_subscription_buffer_is_derived_from_the_type` pins that row.
///
///   Issue 1319 names zenoh and XRCE for this row and phase-454 W5 measured
///   otherwise: both advertise `supports_process_in_place`, and
///   `register_subscription_buffered_on` tests that BEFORE it computes a slot
///   size, so a typed Rust registration on either claims no region at all (672
///   bytes on `contract-monitor-sub`; issue 1340). What is reproduced here is
///   the mechanism and the two rows that do take it —
///   `RegistrationPath::Unbounded` — a site that states no bound, against a
///   backend that buffers. phase-456 W8 collapsed the five rows to three and
///   took the language out of their names; the C half of this row used to be
///   ~~`CRawNoHint`~~ and was reachable on EVERY backend, because the C path never
///   consulted the in-place capability. It does now, so on zenoh or XRCE a C
///   registration takes `InPlace` like a Rust one.
/// * **the subscribed class below the closure bound** — `MODELLED_SLOT`
///   against `DEFAULT_RX_BUF_SIZE`. The reference island measures 880 against
///   1,496; the RATIO is not what matters, the direction is.
///
/// Both arenas are MEASURED from a registration rather than computed from
/// `buffered_region_size`, because the arena aligns what it hands out and the
/// raw region formula is off by that padding at some slot sizes — which would
/// make this pass or fail for a reason that is not the one under test.
#[cfg(not(rmw_needs_type_descriptors))]
#[test]
fn a_schemaless_subscription_outgrows_an_arena_priced_at_its_types_bound() {
    /// What the BUILD charged before W5.b: the subscription's own type bound,
    /// which `NROS_SUBSCRIBED_TYPE_BOUNDS` carries and `subs_arena` prices at.
    const MODELLED_SLOT: usize = crate::config::DEFAULT_RX_BUF_SIZE / 2;
    const {
        assert!(
            MODELLED_SLOT > 0,
            "the fixture needs a type bound strictly below the closure buffer, \
             or there is no gap to reproduce"
        )
    };
    // KEEP_LAST(1) -- a TripleBuffer, three slots, the island's own depth and
    // the cheapest shape the gap survives.
    let qos = qos_with_depth(1);

    // --- what each number IS, measured on an arena large enough for both ---
    let mut wide = executor_with_arena(MockSession::new(), crate::config::ARENA_SIZE);
    let nid = wide.node_builder("island").build().unwrap();
    let node_overhead = wide.arena_used();
    let modelled = {
        let before = wide.arena_used();
        wide.node_mut(nid)
            .subscription("/modelled")
            .qos(qos)
            .typed::<TestMsg>()
            .rx_buffer::<MODELLED_SLOT>()
            .build(|_: &TestMsg| {})
            .unwrap();
        wide.arena_used() - before
    };
    let claimed = arena_delta(&mut wide, nid, "/claimed", qos, RxAsk::Default);
    assert!(
        claimed > modelled,
        "the fixture did not reproduce the gap: a default registration claimed \
         {claimed} bytes and the model's per-type price is {modelled}. On this \
         arm `default_subscription_rx_bytes` must return `None`, so the \
         registration takes DEFAULT_RX_BUF_SIZE = {} (issue 1319)",
        crate::config::DEFAULT_RX_BUF_SIZE,
    );
    // The gap has the shape the issue reports: the SLOT COUNT times the
    // difference in slot size, and nothing else -- both registrations build the
    // same entry struct. On the island that is `3 x (1496 - 880)` = 1,848 bytes
    // per subscription at depth 1.
    assert_eq!(
        claimed - modelled,
        3 * (crate::config::DEFAULT_RX_BUF_SIZE - MODELLED_SLOT),
        "the shortfall is a TripleBuffer's three slots times the difference in \
         slot size (issue 1319); a different number means the entry struct \
         moved too and this test is measuring two things"
    );

    // --- FAILS FIRST: the arena the OLD model derives ---------------------
    let mut short = executor_with_arena(MockSession::new(), node_overhead + modelled);
    let nid = short.node_builder("island").build().unwrap();
    let err = short
        .node_mut(nid)
        .subscription("/chatter")
        .qos(qos)
        .typed::<TestMsg>()
        .build(|_: &TestMsg| {})
        .expect_err(
            "issue 1319 is that this registration does NOT fit an arena priced \
             at the type's own bound -- if it now fits, the schemaless \
             registration path stopped claiming RX_BUF and this test has \
             outlived the defect it reproduces",
        );
    assert_eq!(
        err,
        NodeError::BufferTooSmall,
        "the shortfall must surface as BufferTooSmall at registration -- that \
         is the symptom issue 1319 predicted and the one an image sees"
    );

    // --- and fits at the number phase-454 W5.b derives --------------------
    let mut sized = executor_with_arena(MockSession::new(), node_overhead + claimed);
    let nid = sized.node_builder("island").build().unwrap();
    sized
        .node_mut(nid)
        .subscription("/chatter")
        .qos(qos)
        .typed::<TestMsg>()
        .build(|_: &TestMsg| {})
        .expect(
            "priced at what the registration path CLAIMS, the same subscription \
             fits -- which is the whole of the W5.b fix",
        );
}

/// phase-457 W3 / **issue 1340** — an in-place BACKEND is not an in-place
/// REGISTRATION, and pricing a subscription on the backend alone under-sizes the
/// arena.
///
/// Issue 1340 -- a GENERIC subscription on a backend that dispatches in place
/// claims no receive region, so it fits an arena priced at the in-place row.
///
/// This test used to be a_generic_subscription_on_an_in_place_backend_still_
/// claims_a_full_region and pinned the OPPOSITE. The generic path -- what a
/// declarative component and the cross-RMW bridge both lower to -- allocated
/// `buffered_region(depth, RX_BUF)` even on a backend that could hand it the
/// sample out of its own slot, so an arena priced at what an in-place
/// registration claims could not hold it and registration failed
/// `NodeError::BufferTooSmall`. That under-size is why phase-457 W3 made the
/// descriptor state `in_place` only for an endpoint whose registration somebody
/// OBSERVED. Its own failure message said what would end it: the generic path
/// consulting the capability too.
///
/// It now does. `register_subscription_buffered_raw_on` takes the in-place arm
/// when `open_subscription` reports one, and its `in_place_capable` is
/// `DeclaredSubscriptionShape::BufferedRaw.in_place_capable()` -- the value the
/// metadata probe states for a declared subscription -- so the row the build
/// prices and the claim the executor makes move together.
///
/// Four claims, each able to fail on its own:
///
/// 1. on an in-place backend the generic claim is an ENTRY, not a region, and
///    the saving is at least one receive slot;
/// 2. an arena priced at what the TYPED in-place registration claims now holds
///    the GENERIC one -- the exact assertion that failed before this fix;
/// 3. on a backend that does NOT dispatch in place, the generic path still
///    reserves its region, because that is the direction that ships a failure;
/// 4. it still DELIVERS, on both arms, so a dispatch that silently drops
///    samples cannot read as a saving.
#[test]
fn a_generic_subscription_on_an_in_place_backend_claims_no_receive_region() {
    use crate::executor::arena::EntryKind;
    static BUFFERED_SEEN: portable_atomic::AtomicUsize = portable_atomic::AtomicUsize::new(0);
    static IN_PLACE_SEEN: portable_atomic::AtomicUsize = portable_atomic::AtomicUsize::new(0);

    let qos = qos_with_depth(1);
    let type_name = <TestMsg as nros_core::RosMessage>::TYPE_NAME;

    // --- (1) + (3): what the generic path claims on each kind of backend ---
    let generic_claim = |exec: &mut Executor<'static>,
                         seen: &'static portable_atomic::AtomicUsize| {
        let nid = exec.node_builder("island").build().unwrap();
        let before = exec.arena_used();
        exec.node_mut(nid)
            .subscription("/generic")
            .qos(qos)
            .generic(type_name, "")
            .build(move |d: &[u8]| seen.store(d.len(), portable_atomic::Ordering::SeqCst))
            .unwrap();
        exec.arena_used() - before
    };

    let mut buffered = executor_with_arena(MockSession::new(), crate::config::ARENA_SIZE);
    let buffered_bytes = generic_claim(&mut buffered, &BUFFERED_SEEN);

    let mut in_place = executor_with_arena(
        MockSession::with_in_place_dispatch(),
        crate::config::ARENA_SIZE,
    );
    let in_place_bytes = generic_claim(&mut in_place, &IN_PLACE_SEEN);

    assert!(
        in_place_bytes < buffered_bytes,
        "on a backend that dispatches in place, a generic registration must claim \
         LESS than on one that does not: in-place {in_place_bytes} vs buffered \
         {buffered_bytes}. Equal means `register_subscription_buffered_raw_on` \
         ignored the in-place report and built a buffered entry anyway -- issue \
         1340's defect"
    );
    // A LOWER bound, not an exact figure: the arena aligns what it hands out and
    // the two entry structs differ in size.
    assert!(
        buffered_bytes - in_place_bytes >= crate::config::DEFAULT_RX_BUF_SIZE,
        "the saving is the receive region, so it must be at least one slot ({} \
         bytes); measured {}",
        crate::config::DEFAULT_RX_BUF_SIZE,
        buffered_bytes - in_place_bytes
    );

    // --- (2): the arena a build prices from the in-place row now holds it ---
    //
    // Priced at what the TYPED in-place registration claims -- the row
    // `subs_arena_from_descriptor` sums for an endpoint stating `in_place`.
    let (node_overhead, typed_claim) = {
        let mut wide = executor_with_arena(
            MockSession::with_in_place_dispatch(),
            crate::config::ARENA_SIZE,
        );
        let nid = wide.node_builder("island").build().unwrap();
        let node_overhead = wide.arena_used();
        let before = wide.arena_used();
        wide.node_mut(nid)
            .subscription("/typed")
            .qos(qos)
            .typed::<TestMsg>()
            .build(|_: &TestMsg| {})
            .unwrap();
        (node_overhead, wide.arena_used() - before)
    };
    let mut sized = executor_with_arena(
        MockSession::with_in_place_dispatch(),
        node_overhead + typed_claim,
    );
    let nid = sized.node_builder("island").build().unwrap();
    sized
        .node_mut(nid)
        .subscription("/generic")
        .qos(qos)
        .generic(type_name, "")
        .build(|_: &[u8]| {})
        .expect(
            "a GENERIC registration must fit an arena priced at what an in-place \
             registration claims -- before issue 1340 it failed BufferTooSmall \
             here, which is the under-size the descriptor's refusal existed for",
        );

    // --- (4): and it still delivers, on both arms ---
    for (exec, seen) in [
        (&mut buffered, &BUFFERED_SEEN),
        (&mut in_place, &IN_PLACE_SEEN),
    ] {
        let (data, len) = encode_test_msg(7);
        let meta = exec
            .entries
            .iter()
            .flatten()
            .find(|m| matches!(m.kind, EntryKind::Subscription))
            .expect("the generic registration left a subscription entry");
        let arena_ptr = exec.arena.as_ptr() as *const u8;
        // Sound for both arms: `SubBufferedRawEntry` and the in-place raw entry
        // are both `#[repr(C)]` with the backend handle as their first field.
        unsafe {
            let sub_ptr = arena_ptr.add(meta.offset) as *const MockSubscriber;
            (*sub_ptr).load(data, len);
        }
        exec.spin_once(core::time::Duration::from_millis(0));
        assert_eq!(
            seen.load(portable_atomic::Ordering::SeqCst),
            len,
            "the generic callback must receive the sample's bytes on both dispatch arms"
        );
    }
}

/// phase-392 W3c -- `.rx_buffer::<N>()` is the OPT-OUT, and it is exact.
///
/// The consumer named a number, so the derivation is skipped and every slot
/// costs `N` -- including when `N` is far larger than the type needs, which is
/// the whole reason the opt-out exists (a peer that pads, a `.msg` bound looser
/// than the traffic). Measured against `buffered_region_size(depth, N)` at two
/// depths for the same reason as its sibling above.
#[test]
fn naming_a_receive_buffer_opts_out_of_the_derivation() {
    const N: usize = 4096;
    // A `const` block: both sides are constants, so this is a compile-time
    // claim about the fixture, not a run-time check. Written as a plain
    // `assert!` it is `clippy::assertions_on_constants`, which `-D warnings`
    // takes red under `cargo clippy -p nros-node --all-targets` — a lane no
    // merge-gating job runs, which is why it sat here.
    const {
        assert!(
            N > crate::config::DEFAULT_RX_BUF_SIZE,
            "the fixture is only meaningful while N is off the default, or an \
             opt-out that silently fell back would still pass"
        )
    };

    let mut executor: Executor = executor_with_clock(MockSession::new());
    let nid = executor.node_builder("explicit_sizing").build().unwrap();

    let shallow = arena_delta(
        &mut executor,
        nid,
        "/shallow",
        qos_with_depth(2),
        RxAsk::Exact,
    );
    let deep = arena_delta(
        &mut executor,
        nid,
        "/deep",
        qos_with_depth(10),
        RxAsk::Exact,
    );

    let (_s, shallow_region) = super::arena::buffered_region_size(2, N);
    let (_d, deep_region) = super::arena::buffered_region_size(10, N);

    assert_eq!(
        deep - shallow,
        deep_region - shallow_region,
        "`.rx_buffer::<{N}>()` must claim exactly {N} bytes per slot; depth 2 \
         claimed {shallow} and depth 10 claimed {deep}"
    );
}

/// The saving, measured. A bounded type's slots shrink to its own bound.
///
/// Measured on this executor at the shipped defaults (`DEFAULT_RX_BUF_SIZE`
/// 1024, `QoSProfile::default().depth` 10, so an 11-slot `SpscRing`):
/// `TestMsg`'s bound is 12 bytes, the buffer region falls 11352 -> 220, and the
/// registration's total arena claim falls **13632 -> 2500 bytes**, i.e. 11132
/// recovered -- 15% of the 74240-byte default arena, from one subscription.
///
/// phase-392 W3c -- the baseline is now `.rx_buffer::<DEFAULT_RX_BUF_SIZE>()`,
/// the OPT-OUT, and not a subscription that names nothing: naming nothing
/// derives too wherever the bound is reachable, so comparing against it would
/// measure zero on exactly the arm the saving is real on. The number under test
/// is unchanged -- this is the same 11132 bytes, priced against the global that
/// a consumer now has to ask for.
#[test]
fn a_bounded_type_shrinks_its_own_receive_buffer() {
    let bound = nros_serdes::size::max_serialized_bound::<TestMsg>()
        .expect("a single Int32 is a bounded type");
    assert!(
        bound < crate::config::DEFAULT_RX_BUF_SIZE,
        "this fixture is only meaningful while the bound ({bound}) is under the \
         default ({}) -- otherwise the assertion below passes for the wrong reason",
        crate::config::DEFAULT_RX_BUF_SIZE
    );

    let mut executor: Executor = executor_with_clock(MockSession::new());
    let nid = executor.node_builder("bound_sizing").build().unwrap();

    let depth = QoSProfile::default().depth;
    let default_delta = arena_delta(
        &mut executor,
        nid,
        "/plain",
        qos_with_depth(depth),
        RxAsk::Global,
    );
    let bound_delta = arena_delta(
        &mut executor,
        nid,
        "/sized",
        qos_with_depth(depth),
        RxAsk::FromType,
    );

    let (_s, default_region) =
        super::arena::buffered_region_size(depth, crate::config::DEFAULT_RX_BUF_SIZE);
    let (_s2, bound_region) = super::arena::buffered_region_size(depth, bound);

    // Exact up to the arena's grain: every region is rounded up to it (issue
    // 1667), so the two deltas each carry up to `ARENA_GRAIN - 1` bytes of
    // rounding the raw region sizes do not.
    assert!(
        (default_delta - bound_delta).abs_diff(default_region - bound_region)
            < super::spin::ARENA_GRAIN,
        "opting in must recover the region difference: \
         default {default_delta} bytes vs bound-sized {bound_delta}, \
         regions {default_region} vs {bound_region}"
    );
    assert!(
        bound_delta < default_delta,
        "the whole point is that it is smaller: {bound_delta} vs {default_delta}"
    );
}

/// An unbounded type yields NO number at all -- not the configured default.
///
/// Every message type is required to carry a bound (`.msg` `string<=64`, or a
/// `cap` in `nros-codegen.toml`), so `.rx_buffer_from_type()` on a type without
/// one is a BUILD error, and this is the predicate that error keys on. Phase
/// 380's rule is unchanged and is what makes erroring correct: `None` means "no
/// bound EXISTS", never "unknown", and inventing a number from a fallback is
/// what it forbids. There is no compile-fail harness in this workspace, so the
/// refusal is proved at the value the `const assert!` reads rather than by
/// compiling the rejected program.
#[test]
fn an_unbounded_type_yields_no_receive_buffer_size() {
    assert_eq!(
        nros_serdes::size::max_serialized_bound::<UnboundedTestMsg>(),
        None,
        "the fixture must actually be unbounded"
    );
    assert_eq!(
        crate::rmw_type_registry::subscription_rx_bytes::<UnboundedTestMsg>(
            crate::config::DEFAULT_RX_BUF_SIZE
        ),
        None,
        "an unbounded type must yield no size; falling back to the configured \
         default is the substitution phase 380 forbids"
    );

    // The non-opted-in path is untouched: an unbounded type still registers at
    // the configured default, because that path never claimed to size from the
    // type. Making an unbounded type impossible in the first place is a codegen
    // change, not this wave's.
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let nid = executor.node_builder("unbounded_sizing").build().unwrap();
    let before = executor.arena_used();
    executor
        .node_mut(nid)
        .subscription("/unbounded_plain")
        .typed::<UnboundedTestMsg>()
        .build(|_: &UnboundedTestMsg| {})
        .unwrap();
    let (_slots, region) = super::arena::buffered_region_size(
        QoSProfile::default().depth,
        crate::config::DEFAULT_RX_BUF_SIZE,
    );
    assert!(
        executor.arena_used() - before > region,
        "an unbounded type that does not opt in still claims the default region"
    );
}

/// A message whose bound is NOT a multiple of 4 — one `int8`, so 4 bytes of
/// encapsulation plus 1.
///
/// `TestMsg` cannot test the framing allowance: a single `i32` bounds at 8,
/// which is already aligned, so rounding is a no-op and the assertion passes
/// whether or not the code rounds. This fixture exists so the test can fail.
/// Only `schema::Message` — `subscription_rx_bytes` asks for nothing else, and
/// the bound is a property of the schema rather than of the transport traits.
struct OddBoundTestMsg;

impl nros_serdes::schema::Message for OddBoundTestMsg {
    const TYPE_NAME: &'static str = "test/msg/OddBoundTestMsg";
    const FIELDS: &'static [nros_serdes::schema::Field] = &[nros_serdes::schema::Field {
        name: "data",
        ty: nros_serdes::schema::FieldType::Int8,
        offset: 0,
    }];
}

/// The receive buffer carries the transport's framing allowance, so it is the
/// bound rounded UP to 4 rather than the bound itself.
///
/// Measured, not assumed: a 25-byte message published by ROS 2 Humble over stock
/// `rmw_cyclonedds` reaches the Cyclone backend as 28 bytes — the RTPS
/// submessage's own alignment, applied by the sender. A buffer sized to the
/// exact bound refuses it, correctly and with the message lost. See
/// `rmw_type_registry::transport_framed`.
#[test]
fn the_receive_buffer_allows_for_transport_framing() {
    let bound = nros_serdes::size::max_serialized_bound::<OddBoundTestMsg>().unwrap();
    assert_ne!(
        bound % 4,
        0,
        "this fixture only proves anything while its bound is misaligned"
    );
    assert_eq!(
        crate::rmw_type_registry::subscription_rx_bytes::<OddBoundTestMsg>(
            crate::config::DEFAULT_RX_BUF_SIZE
        ),
        Some(bound.next_multiple_of(4)),
        "the receive buffer must hold what the transport delivers, not just what \
         the message measures"
    );

    // An already-aligned type must not be inflated.
    let aligned = nros_serdes::size::max_serialized_bound::<TestMsg>().unwrap();
    assert_eq!(aligned % 4, 0, "TestMsg is the aligned control case");
    assert_eq!(
        crate::rmw_type_registry::subscription_rx_bytes::<TestMsg>(
            crate::config::DEFAULT_RX_BUF_SIZE
        ),
        Some(aligned),
    );
}

/// A type whose bound EXCEEDS the ceiling keeps the ceiling. Growing here would
/// spend arena the caller never budgeted.
#[test]
fn a_bound_larger_than_the_ceiling_never_grows_the_buffer() {
    let bound = nros_serdes::size::max_serialized_bound::<TestMsg>().unwrap();
    assert_eq!(
        crate::rmw_type_registry::subscription_rx_bytes::<TestMsg>(bound - 1),
        Some(bound - 1),
        "the caller's ceiling wins; the bound may only shrink the buffer"
    );
}

// ====================================================================
// phase-408 W5b -- the hint sizes the ARENA on the info / validated C paths
//
// W5a routed `rx_buffer_hint` to the BACKEND (payload size class) and stopped
// there, because both entries still stored a real `[u8; RX_BUF]` and `RX_BUF`
// is `DEFAULT_RX_BUF_SIZE` at every call site. W5b moved that slot into the
// arena's trailing region, so the hint now buys the allocation too.
//
// These MEASURE. Each helper registers into a FRESH executor, so `arena_used()`
// starts from the same value every time and the delta differs ONLY by the
// trailing payload slot -- no alignment bookkeeping, no dependence on the entry
// struct's size, which is a target detail these tests have no business pinning.
// Before W5b every number below was identical.
// ====================================================================

unsafe extern "C" fn sizing_info_cb(
    _data: *const u8,
    _len: usize,
    _att: *const u8,
    _att_len: usize,
    _ctx: *mut core::ffi::c_void,
) {
}

/// Arena bytes ONE info-callback C subscription claims at `hint`.
fn info_arena_delta(hint: usize) -> usize {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let before = executor.arena_used();
    executor
        .add_arena_subscription_c_info_callback::<{ crate::config::DEFAULT_RX_BUF_SIZE }>(
            None,
            "/hinted",
            "test/msg/TestMsg",
            "test_hash",
            QoSProfile::default().keep_last(1),
            sizing_info_cb,
            core::ptr::null_mut(),
            hint,
        )
        .unwrap();
    executor.arena_used() - before
}

/// The saving, measured: the slot tracks the hint byte for byte.
///
/// Two hints rather than one absolute number, for the same reason
/// `the_default_subscription_buffer_is_derived_from_the_type` measures deltas -- the
/// difference isolates the payload slot from everything else the registration
/// claims.
#[test]
fn an_info_subscription_sizes_its_arena_slot_from_the_hint() {
    let small = info_arena_delta(128);
    let large = info_arena_delta(512);

    assert_eq!(
        large - small,
        512 - 128,
        "the trailing payload slot must track the hint: 128 claimed {small} \
         bytes, 512 claimed {large}"
    );
    // The delta is the payload slot PLUS the entry struct (an `RmwSubscriber`
    // and a 256-byte attachment staging area), so the absolute number is not a
    // thing to pin -- but it must move, and downwards, which is the whole
    // point: before W5b a hint changed nothing at all here.
    assert!(
        small < info_arena_delta(0),
        "a 128-byte hint must claim less than an unhinted registration; got \
         {small}"
    );
}

/// 0 is "this caller stated nothing", never "zero bytes". It falls back to the
/// configured default, so an unhinted registration claims what it always did.
#[test]
fn an_unhinted_info_subscription_still_claims_the_default() {
    let default = crate::config::DEFAULT_RX_BUF_SIZE;
    let unhinted = info_arena_delta(0);
    let hinted = info_arena_delta(128);

    assert_eq!(
        unhinted - hinted,
        default - 128,
        "a hint of 0 must fall back to {default} bytes, not to zero: unhinted \
         claimed {unhinted}, a 128-byte hint claimed {hinted}"
    );
    assert_eq!(
        unhinted,
        info_arena_delta(default),
        "stating the default explicitly and stating nothing must cost the same"
    );
}

#[cfg(feature = "safety-e2e")]
unsafe extern "C" fn sizing_validated_cb(
    _data: *const u8,
    _len: usize,
    _gap: i64,
    _duplicate: bool,
    _crc_valid: i8,
    _ctx: *mut core::ffi::c_void,
) {
}

/// The validated sibling carries the same change, and it is a SEPARATE
/// registration path -- fixing only the one the report named is how this
/// codebase's recurring defects recur.
#[cfg(feature = "safety-e2e")]
#[test]
fn a_validated_subscription_sizes_its_arena_slot_from_the_hint() {
    fn delta(hint: usize) -> usize {
        let mut executor: Executor = executor_with_clock(MockSession::new());
        let before = executor.arena_used();
        executor
            .add_arena_subscription_c_validated_callback::<{ crate::config::DEFAULT_RX_BUF_SIZE }>(
                None,
                "/hinted_validated",
                "test/msg/TestMsg",
                "test_hash",
                QoSProfile::default().keep_last(1),
                sizing_validated_cb,
                core::ptr::null_mut(),
                hint,
            )
            .unwrap();
        executor.arena_used() - before
    }

    let default = crate::config::DEFAULT_RX_BUF_SIZE;
    let small = delta(128);
    let large = delta(512);
    let unhinted = delta(0);

    assert_eq!(
        large - small,
        512 - 128,
        "the trailing payload slot must track the hint: 128 claimed {small} \
         bytes, 512 claimed {large}"
    );
    assert_eq!(
        unhinted - small,
        default - 128,
        "a hint of 0 must fall back to {default} bytes, not to zero: unhinted \
         claimed {unhinted}, a 128-byte hint claimed {small}"
    );
}

// ====================================================================
// Arena callback tests
// ====================================================================

#[test]
fn test_add_subscription_and_spin_once_no_data() {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);
    let nid = executor
        .node_builder("test_add_subscription_and_spin_once_no_data")
        .build()
        .unwrap();

    // Register a subscription — callback should never fire
    let called = alloc::sync::Arc::new(portable_atomic::AtomicBool::new(false));
    let called2 = called.clone();
    executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/test", move |_msg: &TestMsg| {
            called2.store(true, portable_atomic::Ordering::SeqCst);
        })
        .unwrap();

    let result = executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(result.subscriptions_processed, 0);
    assert!(!result.any_work());
    assert!(!called.load(portable_atomic::Ordering::SeqCst));
}

#[test]
fn test_add_subscription_and_spin_once_with_data() {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);

    let nid = executor
        .node_builder("test_add_subscription_and_spin_once_with_data")
        .build()
        .unwrap();
    let received = alloc::sync::Arc::new(std::sync::Mutex::new(None));
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
#[allow(dead_code)] // only the std-gated zero-copy tests construct these
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
#[allow(dead_code)] // only the std-gated zero-copy tests construct these
struct ImageView<'a> {
    width: u32,
    pixels: &'a [u8],
    ranges: LeSliceView<'a, f32>,
}

impl<'a> DeserializeView<'a> for ImageView<'a> {
    fn deserialize_view(reader: &mut CdrReader<'a>) -> Result<Self, DeserError> {
        Ok(Self {
            width: reader.read_u32()?,
            pixels: reader.read_slice_u8()?,
            ranges: reader.read_le_slice::<f32>()?,
        })
    }
}

/// ZST marker — matches codegen `ImageViewable`.
#[allow(dead_code)] // only the std-gated zero-copy tests construct these
struct ImageViewable;
impl ViewableMessage for ImageViewable {
    type View<'a> = ImageView<'a>;
    const TYPE_NAME: &'static str = "test/msg/Image";
    const TYPE_HASH: &'static str = "image_hash";
}

#[test]
fn borrowed_subscription_e2e_zero_copy_through_spin_once() {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);
    let nid = executor.node_builder("borrowed_e2e").build().unwrap();

    // Captured on the receive side: (width, pixels copy, ranges decoded).
    type Captured = (u32, alloc::vec::Vec<u8>, alloc::vec::Vec<f32>);
    let received: alloc::sync::Arc<std::sync::Mutex<Option<Captured>>> =
        alloc::sync::Arc::new(std::sync::Mutex::new(None));
    let received2 = received.clone();

    executor
        .node_mut(nid)
        .create_subscription_viewable::<ImageViewable, _>("/image", move |view: &ImageView<'_>| {
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

    // The MockSubscriber is the first field of SubBufferedViewEntry, so it
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
    assert_eq!(got.1, alloc::vec![10, 20, 30, 40, 250]);
    assert_eq!(got.2, alloc::vec![1.5f32, -2.25, 3.0e10]);
}

#[test]
fn test_multiple_subscriptions() {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);

    let nid = executor
        .node_builder("test_multiple_subscriptions")
        .build()
        .unwrap();
    let count = alloc::sync::Arc::new(portable_atomic::AtomicUsize::new(0));
    let count1 = count.clone();
    let count2 = count.clone();

    executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/topic1", move |_msg: &TestMsg| {
            count1.fetch_add(1, portable_atomic::Ordering::SeqCst);
        })
        .unwrap();

    executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/topic2", move |_msg: &TestMsg| {
            count2.fetch_add(1, portable_atomic::Ordering::SeqCst);
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
    assert_eq!(count.load(portable_atomic::Ordering::SeqCst), 2);
}

/// Phase 110.B — when two subscriptions are bound to `Edf` SCs,
/// the one with the earlier deadline dispatches first regardless of
/// registration order.
#[test]
fn test_edf_dispatch_order() {
    use crate::executor::sched_context::{DeadlinePolicy, OptUs, SchedClass, SchedContext};
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);

    // `firing_order` records the data field of every msg the callbacks
    // see, in dispatch order.
    let firing_order = alloc::sync::Arc::new(std::sync::Mutex::new(alloc::vec::Vec::<i32>::new()));
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
    assert_eq!(*order, alloc::vec![20, 10]);
}

/// Phase 110.F — an `os_pri`-bound callback is dispatched, on the spin
/// thread. The opt-in worker pool that ran such a callback on an elevated
/// thread is retired (phase-509 P11): `os_pri` is recorded, not acted on, and
/// the callback runs in the ordinary pass like any other.
#[test]
fn an_os_pri_bound_callback_runs_on_the_spin_thread() {
    use crate::executor::sched_context::{SchedClass, SchedContext};
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);
    let nid = executor
        .node_builder("an_os_pri_bound_callback_runs_on_the_spin_thread")
        .build()
        .unwrap();

    let count = alloc::sync::Arc::new(portable_atomic::AtomicUsize::new(0));
    let count_cb = count.clone();
    let h = executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/picas", move |_msg: &TestMsg| {
            count_cb.fetch_add(1, portable_atomic::Ordering::SeqCst);
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

    // Synchronous: the callback has run when `spin_once` returns.
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(
        count.load(portable_atomic::Ordering::SeqCst),
        1,
        "an os_pri-bound callback dispatches in the pass, on the spin thread"
    );
}

private_test_clock!(tt_window_gate_clock);

/// Phase 110.G — TT-window gate suppresses dispatch when the
/// current monotonic time falls outside `[off, off + duration)`.
/// Coexists with the existing class-based dispatch — this test uses
/// a `Fifo` SC with a TT window set, demonstrating that the gate is
/// orthogonal to class.
///
/// Issue 1105 — on its own clock. The old version read the shared
/// free-running `test_clock_us` and relied on the phase landing outside
/// `[50s, 50.001s)` of a 60 s frame, which is a 1-in-60 000 coin toss on the
/// suite's runtime rather than a statement about the gate. Both directions
/// are asserted now, off a clock this test sets: outside the window nothing
/// dispatches, inside it the same entry does, and the only thing that changed
/// between the two is the clock.
#[test]
fn test_tt_window_gate_suppresses_outside_window() {
    use crate::executor::sched_context::{OptUs, SchedClass, SchedContext};
    let session = MockSession::new();
    tt_window_gate_clock::claim_at_us(0);
    let mut executor: Executor = executor_with_clock_fn(session, tt_window_gate_clock::now_us);

    // Window = [50s..50.001s) within a 60-second major frame.
    executor.register_time_triggered_dispatcher(60_000_000);
    let nid = executor
        .node_builder("test_tt_window_gate_suppresses_outside_window")
        .build()
        .unwrap();

    let count = alloc::sync::Arc::new(portable_atomic::AtomicUsize::new(0));
    let count_cb = count.clone();
    let h = executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/tt", move |_msg: &TestMsg| {
            count_cb.fetch_add(1, portable_atomic::Ordering::SeqCst);
        })
        .unwrap();

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
    let subscriber = unsafe { &*(arena_ptr.add(off) as *const MockSubscriber) };

    // Phase 10 s — outside `[50s, 50.001s)`.
    tt_window_gate_clock::set_us(10_000_000);
    let (d, n) = encode_test_msg(1);
    subscriber.load(d, n);
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(
        count.load(portable_atomic::Ordering::SeqCst),
        0,
        "TT window gate must suppress dispatch outside the active slot"
    );

    // Same executor, same entry, same waiting sample — only the clock moved,
    // to phase 50.0005 s, inside the window. Without this half the assertion
    // above passes for a gate that suppresses unconditionally.
    tt_window_gate_clock::set_us(50_000_500);
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(
        count.load(portable_atomic::Ordering::SeqCst),
        1,
        "the suppressed sample must dispatch once the clock reaches the window"
    );
}

private_test_clock!(tt_active_window_clock);

/// Phase 110.G — schedule-table builder declares + applies a full
/// cyclic schedule in one call: validates window layout, sets
/// major-frame length, creates one SC per window with the right
/// TT-gate fields. Two-window schedule, each window 1 s of a 2-second
/// major frame: the entry bound to the CURRENT window fires and the one
/// bound to the other stays suppressed.
///
/// Issue 1105 — "the current window" used to mean *whichever window the
/// suite's own elapsed runtime happened to land in*: the executor read the
/// shared free-running `test_clock_us`, so the asserted phase was
/// `time-spent-in-earlier-tests % 2s`. Solo that is ~0 and the assertion held;
/// after ~340 siblings under `--test-threads=1` it landed in window 1 and the
/// test failed 3 of 3, with nothing about the scheduler having changed.
///
/// It now runs on its own clock (`private_test_clock!`), and asserts the
/// schedule in BOTH slots — the clock is the only thing that moves between
/// them, which is what makes this a statement about the gate.
#[test]
fn test_time_triggered_dispatch_active_window() {
    use crate::executor::sched_context::{TimeTriggeredSchedule, TimeTriggeredWindow};

    let session = MockSession::new();
    tt_active_window_clock::claim_at_us(0);
    let mut executor: Executor = executor_with_clock_fn(session, tt_active_window_clock::now_us);

    let count_w0 = alloc::sync::Arc::new(portable_atomic::AtomicUsize::new(0));
    let count_w1 = alloc::sync::Arc::new(portable_atomic::AtomicUsize::new(0));
    let cb0 = count_w0.clone();
    let cb1 = count_w1.clone();
    let nid = executor
        .node_builder("test_apply_time_triggered_schedule")
        .build()
        .unwrap();
    let h0 = executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/tt0", move |_msg: &TestMsg| {
            cb0.fetch_add(1, portable_atomic::Ordering::SeqCst);
        })
        .unwrap();
    let h1 = executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/tt1", move |_msg: &TestMsg| {
            cb1.fetch_add(1, portable_atomic::Ordering::SeqCst);
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
    let sub0 = unsafe { &*(arena_ptr.add(off0) as *const MockSubscriber) };
    let sub1 = unsafe { &*(arena_ptr.add(off1) as *const MockSubscriber) };
    let (d0, n0) = encode_test_msg(10);
    let (d1, n1) = encode_test_msg(20);
    sub0.load(d0, n0);
    sub1.load(d1, n1);

    // Phase 0.5 s, i.e. inside window-0 by construction rather than by luck.
    tt_active_window_clock::set_us(500_000);
    let _ = executor.spin_once(core::time::Duration::from_millis(0));

    // Only the entry bound to window-0 fires; the entry bound to
    // window-1 must stay suppressed.
    assert_eq!(
        count_w0.load(portable_atomic::Ordering::SeqCst),
        1,
        "entry bound to window-0 should dispatch inside its active slot"
    );
    assert_eq!(
        count_w1.load(portable_atomic::Ordering::SeqCst),
        0,
        "entry bound to window-1 must stay suppressed outside its slot"
    );

    // Advance into window-1 and assert the complement. Same executor, same
    // entries; only the clock moved. Without this half a gate that suppressed
    // window-1 unconditionally — or one that ignored the binding entirely —
    // would still pass.
    sub0.load(d0, n0);
    tt_active_window_clock::set_us(1_500_000);
    let _ = executor.spin_once(core::time::Duration::from_millis(0));

    assert_eq!(
        count_w0.load(portable_atomic::Ordering::SeqCst),
        1,
        "entry bound to window-0 must stay suppressed once the phase leaves its slot"
    );
    assert_eq!(
        count_w1.load(portable_atomic::Ordering::SeqCst),
        1,
        "the sample held back in window-0 must dispatch once window-1 is active"
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

private_test_clock!(sporadic_exhaustion_clock);

/// Phase 110.E — `SchedClass::Sporadic` budget suppression. After the
/// budget is exhausted within a period, the bound subscription's
/// callback no longer fires until the next period boundary refills
/// the budget.
#[test]
fn test_sporadic_budget_exhaustion_suppresses_dispatch() {
    sporadic_exhaustion_clock::claim_at_us(1_000_000);
    use crate::executor::sched_context::{OptUs, SchedClass, SchedContext};
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock_fn(session, sporadic_exhaustion_clock::now_us);

    let nid = executor
        .node_builder("test_sporadic_budget_exhaustion")
        .build()
        .unwrap();
    let count = alloc::sync::Arc::new(portable_atomic::AtomicUsize::new(0));
    let count_cb = count.clone();
    let h = executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/sporadic", move |_msg: &TestMsg| {
            count_cb.fetch_add(1, portable_atomic::Ordering::SeqCst);
            // issue 0736 — the callback must COST something, because callback
            // runtime is now the only thing that consumes budget. This test
            // used to exhaust the budget by sleeping BETWEEN spins, which
            // worked only because the executor charged the inter-spin
            // wall-clock gap to every sporadic SC whether or not it had run.
            // That was the defect; a test that relies on it is asserting the
            // bug. Costing time HERE is the same test of the same rule — "an SC
            // that has spent its budget is not dispatched again" — expressed
            // in the quantity the budget actually bounds.
            //
            // Issue 1944 — the cost is an advance of the executor's private
            // clock, which is what the budget is measured on: exactly 2 ms.
            sporadic_exhaustion_clock::advance_ms(2);
        })
        .unwrap();

    // 1 us budget per 60 s period: one dispatch costs 2 ms of measured
    // runtime, which exhausts it for the rest of the period.
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

    // Cycle 1 — refill puts 1 us in the budget, the callback fires and spends
    // 2 ms of it. The period is 60 s, so nothing refills before cycle 2.
    let (d, n) = encode_test_msg(1);
    unsafe { &*(arena_ptr.add(off) as *const MockSubscriber) }.load(d, n);
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(
        count.load(portable_atomic::Ordering::SeqCst),
        1,
        "cycle 1 must dispatch — otherwise cycle 2 proves nothing"
    );

    // Cycle 2 — budget is 0; dispatch must be suppressed.
    let initial = count.load(portable_atomic::Ordering::SeqCst);
    let (d, n) = encode_test_msg(2);
    unsafe { &*(arena_ptr.add(off) as *const MockSubscriber) }.load(d, n);
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    let after = count.load(portable_atomic::Ordering::SeqCst);

    // Strictly assert no new dispatch on cycle 2.
    assert_eq!(
        after, initial,
        "Sporadic SC must suppress dispatch when budget exhausted"
    );
}

private_test_clock!(per_callback_runtime_clock);

/// Phase 110.E.b follow-up — per-callback runtime accounting.
/// When a Sporadic SC has an `AtomicSporadicState` registered (the
/// ISR-driven refill path), `spin_once` measures each bound
/// callback's wall-clock dispatch time + `consume`s those microseconds
/// from the atomic budget. Replaces the cycle-level over-attribution
/// that previously charged the full cycle `delta_us` against every
/// Sporadic SC regardless of which entries actually fired.
#[test]
fn test_atomic_sporadic_per_callback_runtime_consumed() {
    per_callback_runtime_clock::claim_at_us(1_000_000);
    use crate::executor::{
        sched_context::{OptUs, SchedClass, SchedContext},
        spin::OpaqueTimerHandle,
    };

    let session = MockSession::new();
    let mut executor: Executor =
        executor_with_clock_fn(session, per_callback_runtime_clock::now_us);

    let nid = executor
        .node_builder("test_atomic_sporadic_per_callback_runtime_consumed")
        .build()
        .unwrap();
    // Subscription that costs a known interval on the executor's clock, so
    // the per-callback dispatch time is exact (issue 1944: it was a real
    // sleep, bounded above by 500 ms that a descheduled host could exceed).
    let h = executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/timed", move |_msg: &TestMsg| {
            per_callback_runtime_clock::advance_ms(10);
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

    // Drive one dispatch — the registered closure costs 10 ms.
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    let off = executor.entries[0].as_ref().unwrap().offset;
    let (d, n) = encode_test_msg(7);
    unsafe { &*(arena_ptr.add(off) as *const MockSubscriber) }.load(d, n);
    let _ = executor.spin_once(core::time::Duration::from_millis(0));

    let after = state
        .budget_remaining_us
        .load(portable_atomic::Ordering::Acquire);

    // Per-callback runtime consumed exactly the 10 ms (= 10_000 us) the
    // callback cost, not the 1 s budget or the cycle — proves dispatch-local
    // measurement, not cycle-level over-attribution.
    let consumed = before.saturating_sub(after);
    assert_eq!(
        consumed, 10_000,
        "a 10 ms callback consumes exactly 10 ms of budget"
    );
}

private_test_clock!(atomic_overrun_clock);

/// Phase 110.E.b — overrun detection. A Sporadic-bound callback
/// whose measured runtime exceeds the SC's budget bumps
/// `AtomicSporadicState::overrun_count` + stamps `last_overrun_us`.
/// The "oneshot-IRQ-and-cancel" pattern from the design doc is
/// structurally equivalent for cooperative single-thread dispatch
/// (we can't preempt a running callback), so this is the
/// diagnostic signal end-callers consume to tune budgets.
#[test]
fn test_atomic_overrun_exceeds_budget() {
    atomic_overrun_clock::claim_at_us(1_000_000);
    use crate::executor::{
        sched_context::{OptUs, SchedClass, SchedContext},
        spin::OpaqueTimerHandle,
    };

    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock_fn(session, atomic_overrun_clock::now_us);

    let nid = executor
        .node_builder("test_atomic_sporadic_overrun_recorded")
        .build()
        .unwrap();
    // Subscription costs 25 ms on the executor's clock; budget is 5 ms →
    // must overrun.
    let h = executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/overrun", move |_msg: &TestMsg| {
            atomic_overrun_clock::advance_ms(25);
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

    // Drive one dispatch — the registered closure costs 25 ms.
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
    // Overrun = measured - budget = 25 ms - 5 ms.
    assert_eq!(
        last, 20_000,
        "last_overrun_us is the 25 ms callback less the 5 ms budget"
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
    let crit = executor_with_clock(MockSession::new());
    let crit_handle = unsafe {
        crit.open_threaded(
            SchedPolicy::Fifo { os_pri: 90 },
            apply_noop,
            core::time::Duration::from_millis(1),
        )
    };

    // BE executor — would run at SCHED_FIFO os_pri 10 in a
    // privileged process.
    let be = executor_with_clock(MockSession::new());
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
    let executor: Executor = executor_with_clock(session);

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
    let mut executor: Executor = executor_with_clock(session);

    let firing_order = alloc::sync::Arc::new(std::sync::Mutex::new(alloc::vec::Vec::<i32>::new()));
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
    assert_eq!(*order, alloc::vec![2, 1]);
}

/// phase-509 P2 (RFC-0108 D2) -- one node per entry of `ranks`, registered in
/// that order, each with one subscription carrying its index as data and,
/// for `Some(r)`, seeded with rank `r` before the node is built. Every
/// subscription is made ready, one `spin_once` runs, and the data in
/// dispatch order comes back. `bucket` binds entry `i` to a sched context of
/// that priority when given.
fn ranked_dispatch_order(
    ranks: &[Option<u8>],
    bucket: &[(usize, crate::executor::sched_context::Priority)],
) -> alloc::vec::Vec<i32> {
    use crate::executor::sched_context::{SchedClass, SchedContext};
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);
    let firing_order = alloc::sync::Arc::new(std::sync::Mutex::new(alloc::vec::Vec::<i32>::new()));
    let mut handles = alloc::vec::Vec::new();
    for (i, rank) in ranks.iter().enumerate() {
        let name = alloc::format!("ranked_{i}");
        if let Some(r) = rank {
            executor.bind_node_rank(&name, "/", *r).unwrap();
        }
        let nid = executor.node_builder(&name).build().unwrap();
        let o = firing_order.clone();
        let h = executor
            .node_mut(nid)
            .create_subscription::<TestMsg, _>(&alloc::format!("/t{i}"), move |msg: &TestMsg| {
                o.lock().unwrap().push(msg.data);
            })
            .unwrap();
        handles.push(h);
    }
    for (i, priority) in bucket {
        let sc = executor
            .create_sched_context(SchedContext {
                class: SchedClass::Fifo,
                priority: *priority,
                ..Default::default()
            })
            .unwrap();
        executor
            .bind_handle_to_sched_context(handles[*i], sc)
            .unwrap();
    }
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    for i in 0..ranks.len() {
        let (d, n) = encode_test_msg(i as i32);
        let off = executor.entries[i].as_ref().unwrap().offset;
        unsafe { &*(arena_ptr.add(off) as *const MockSubscriber) }.load(d, n);
    }
    let result = executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(result.subscriptions_processed, ranks.len());
    firing_order.lock().unwrap().clone()
}

/// phase-509 P2 -- three callbacks registered in REVERSE rank order, all
/// ready in one pass, dispatch most urgent first. Before P2 they ran in
/// registration order (`[0, 1, 2]`), whatever the contract ranked them.
#[test]
fn ranked_callbacks_dispatch_in_rank_order_not_registration_order() {
    assert_eq!(
        ranked_dispatch_order(&[Some(2), Some(1), Some(0)], &[]),
        alloc::vec![2, 1, 0]
    );
}

/// phase-509 P2 -- equal ranks keep registration order among themselves.
#[test]
fn equal_ranks_keep_registration_order() {
    assert_eq!(
        ranked_dispatch_order(&[Some(1), Some(1), Some(0), Some(1)], &[]),
        alloc::vec![2, 0, 1, 3]
    );
}

/// phase-509 P2 -- an image with one rank dispatches in exactly today's
/// order: registration order, the same as an image that seeds no rank.
#[test]
fn one_rank_dispatches_in_registration_order() {
    let unseeded = ranked_dispatch_order(&[None, None, None, None], &[]);
    assert_eq!(unseeded, alloc::vec![0, 1, 2, 3]);
    assert_eq!(
        ranked_dispatch_order(&[Some(4), Some(4), Some(4), Some(4)], &[]),
        unseeded
    );
}

/// phase-509 P2 -- the C/C++ `Priority` buckets are three reserved ranks
/// around the contract's: `Critical` beats rank 0 and `BestEffort` waits
/// behind every rank, whatever their registration order.
#[test]
fn priority_buckets_are_reserved_ranks_around_the_contracts() {
    use crate::executor::sched_context::Priority;
    assert_eq!(
        ranked_dispatch_order(
            &[Some(0), Some(200), Some(5), Some(3)],
            &[(0, Priority::BestEffort), (3, Priority::Critical)],
        ),
        alloc::vec![3, 2, 1, 0]
    );
}

/// phase-509 P2 -- the dispatch levels: `Critical` 0, `Normal` 1 + rank
/// (clamped), `BestEffort` the last; each bucket spans exactly its levels.
#[test]
fn dispatch_levels_reserve_the_ends_for_the_buckets() {
    use crate::executor::sched_context::{DISPATCH_LEVELS, MAX_CONTRACT_RANK, Priority};
    assert_eq!(Priority::Critical.dispatch_level(7), 0);
    assert_eq!(Priority::Normal.dispatch_level(0), 1);
    assert_eq!(Priority::Normal.dispatch_level(MAX_CONTRACT_RANK), 30);
    assert_eq!(Priority::Normal.dispatch_level(u8::MAX), 30);
    assert_eq!(
        Priority::BestEffort.dispatch_level(0) as usize,
        DISPATCH_LEVELS - 1
    );
    assert_eq!(Priority::Critical.level_range(), (0, 1));
    assert_eq!(Priority::Normal.level_range(), (1, 31));
    assert_eq!(Priority::BestEffort.level_range(), (31, 32));
}

/// phase-509 P3 -- three ranked nodes, one subscription each: `a` (rank 1)
/// and `c` (rank 2) are ready at the scan; `b` (rank 0) is not, and `a`'s
/// callback makes it ready. Returns the dispatch order of one `spin_once` as
/// node letters. a_readies lets `a` ready `a` again instead of `b`.
fn repick_order(
    reprobe: bool,
    semantics: Option<ExecutorSemantics>,
    a_readies_self: bool,
) -> alloc::string::String {
    use core::sync::atomic::{AtomicPtr, Ordering};
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);
    executor.set_reprobe_each_callback(reprobe);
    if let Some(sem) = semantics {
        executor.set_semantics(sem);
    }
    let order = alloc::sync::Arc::new(std::sync::Mutex::new(alloc::string::String::new()));
    let target = alloc::sync::Arc::new(AtomicPtr::<MockSubscriber>::new(core::ptr::null_mut()));
    for (name, rank) in [("a", 1u8), ("c", 2), ("b", 0)] {
        executor.bind_node_rank(name, "/", rank).unwrap();
        let nid = executor.node_builder(name).build().unwrap();
        let o = order.clone();
        let t = target.clone();
        let is_a = name == "a";
        executor
            .node_mut(nid)
            .create_subscription::<TestMsg, _>(&alloc::format!("/{name}"), move |_m: &TestMsg| {
                o.lock().unwrap().push_str(name);
                if is_a {
                    let p = t.load(Ordering::SeqCst);
                    if !p.is_null() {
                        let (d, n) = encode_test_msg(7);
                        unsafe { &*p }.load(d, n);
                    }
                }
            })
            .unwrap();
    }
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    let sub = |i: usize| -> *mut MockSubscriber {
        let off = executor.entries[i].as_ref().unwrap().offset;
        unsafe { arena_ptr.add(off) as *mut MockSubscriber }
    };
    // a (slot 0) and c (slot 1) ready now; a readies b (slot 2), or itself.
    target.store(
        if a_readies_self { sub(0) } else { sub(2) },
        Ordering::SeqCst,
    );
    for i in [0usize, 1] {
        let (d, n) = encode_test_msg(i as i32);
        unsafe { &*sub(i) }.load(d, n);
    }
    executor.spin_once(core::time::Duration::from_millis(0));
    order.lock().unwrap().clone()
}

/// phase-509 P3 (RFC-0108 D2) -- callback `a` makes the more urgent `b`
/// ready; `b` runs before the less urgent `c` that was already pending in
/// the same pass. Before P3 the pass dispatched the scan's snapshot: `a`,
/// `c`, and `b` only on the next pass.
#[test]
fn a_callback_made_ready_mid_pass_runs_in_rank_order() {
    assert_eq!(repick_order(true, None, false), "abc");
}

/// phase-509 P3 -- with the once-per-pass probe, a slot whose backend cannot
/// attribute an arrival (this mock) joins only at the next scan.
#[test]
fn without_the_reprobe_an_unattributed_slot_waits_for_the_next_pass() {
    assert_eq!(repick_order(false, None, false), "ac");
}

/// phase-509 P3 -- an entry runs at most once per pass, even when its own
/// callback makes it ready again.
#[test]
fn an_entry_runs_once_per_pass_even_when_it_readies_itself() {
    assert_eq!(repick_order(true, None, true), "ac");
}

/// phase-509 P3 -- the LET semantics dispatch the pass's pre-sampled
/// snapshot; nothing joins it mid-pass.
#[test]
fn the_let_semantics_take_nothing_mid_pass() {
    assert_eq!(
        repick_order(true, Some(ExecutorSemantics::LogicalExecutionTime), false),
        "ac"
    );
}

/// phase-509 P3 -- the deferred re-probe question's measurement (RFC-0108
/// Deferred). An executor whose subscriptions (as many as the default
/// executor budget holds, up to 32) are all unattributed (an
/// XRCE- or uORB-shaped image; the mock cannot attribute), 4 of them ready
/// per pass, timed over many passes with the probe once per pass and after
/// every callback. Prints both, per pass and per extra probe; asserts only
/// that both dispatch the same work. Run in release:
///
/// ```text
/// cargo test -p nros-node --lib --release --features alloc,log,sim-time,param-services \
///     reprobe_cost_on_an_unattributed_executor -- --ignored --nocapture
/// ```
#[test]
#[ignore = "a measurement, run by hand (phase-509 P3)"]
fn reprobe_cost_on_an_unattributed_executor() {
    const SUBS: usize = 32;
    const READY: usize = 4;
    static SLOTS: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);
    const PASSES: u32 = 20_000;
    let run = |reprobe: bool| -> (std::time::Duration, usize) {
        let session = MockSession::new();
        let mut executor: Executor = executor_with_clock(session);
        executor.set_reprobe_each_callback(reprobe);
        let nid = executor.node_builder("bench").build().unwrap();
        // As many as the default executor budget holds, up to SUBS.
        let mut subs = 0usize;
        while subs < SUBS
            && executor
                .node_mut(nid)
                .create_subscription::<TestMsg, _>(
                    &alloc::format!("/b{subs}"),
                    move |_m: &TestMsg| {},
                )
                .is_ok()
        {
            subs += 1;
        }
        assert!(subs > READY, "room for {subs} subscriptions only");
        SLOTS.store(subs, core::sync::atomic::Ordering::SeqCst);
        let arena_ptr = executor.arena.as_ptr() as *const u8;
        let offs: alloc::vec::Vec<usize> = (0..subs)
            .map(|i| executor.entries[i].as_ref().unwrap().offset)
            .collect();
        let mut taken = 0usize;
        let mut total = std::time::Duration::ZERO;
        for pass in 0..PASSES {
            for k in 0..READY {
                let i = (pass as usize * 3 + k * 2) % subs;
                let (d, n) = encode_test_msg(k as i32);
                unsafe { &*(arena_ptr.add(offs[i]) as *const MockSubscriber) }.load(d, n);
            }
            let t0 = std::time::Instant::now();
            let r = executor.spin_once(core::time::Duration::from_millis(0));
            total += t0.elapsed();
            taken += r.subscriptions_processed;
        }
        (total, taken)
    };
    // Warm, then measure each twice and keep the faster.
    let _ = run(false);
    let once = run(false).0.min(run(false).0);
    let each = run(true).0.min(run(true).0);
    let (_, taken_once) = run(false);
    let (_, taken_each) = run(true);
    assert_eq!(taken_once, taken_each, "both dispatch the same work");
    let per_pass = |d: std::time::Duration| d.as_nanos() as f64 / PASSES as f64;
    // Each callback re-probes the slots not yet settled: at most SUBS - READY.
    let subs = SLOTS.load(core::sync::atomic::Ordering::SeqCst);
    let extra_probes = (READY * (subs - READY)) as f64;
    // nros-allow-std-stdio: a host-only measurement, #[ignore]d, run by hand
    std::eprintln!(
        "phase-509 P3 reprobe: {subs} unattributed subscriptions, {READY} ready per pass, \
         {PASSES} passes: once per pass {:.0} ns/pass, after every callback {:.0} ns/pass, \
         +{:.0} ns/pass (+{:.1} ns per callback, <= {:.2} ns per extra probe)",
        per_pass(once),
        per_pass(each),
        per_pass(each) - per_pass(once),
        (per_pass(each) - per_pass(once)) / READY as f64,
        (per_pass(each) - per_pass(once)) / extra_probes,
    );
}

/// Phase 110.B — default `Fifo` SC binding preserves registration
/// order even when other entries are bound to `Edf` SCs.
#[test]
fn test_fifo_default_binding_preserved_alongside_edf() {
    use crate::executor::sched_context::{OptUs, SchedClass, SchedContext};
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);

    let firing_order = alloc::sync::Arc::new(std::sync::Mutex::new(alloc::vec::Vec::<i32>::new()));
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
    assert_eq!(*order, alloc::vec![2, 1]);
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
    let mut executor = executor_with_clock(session);
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
            .qos(QoSProfile::default().keep_last(1))
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
        .qos(QoSProfile::default().keep_last(1))
        .typed::<TestMsg>()
        .rx_buffer::<OVERFLOW_RX_BUF>()
        .build(|_msg: &TestMsg| {});
    assert_eq!(result, Err(NodeError::BufferTooSmall));
}

#[test]
fn test_entry_slots_exhausted() {
    // Issue #545 — `MAX_CBS` is a BUILD-TIME knob (`NROS_EXECUTOR_MAX_CBS`,
    // also reachable via `$DOTCONFIG`), and cargo reads `.cargo/config.toml`
    // from the current directory UPWARD — so a workspace that vendors
    // nano-ros and raises the knob for its own image raises it here too.
    // Assume nothing: fill exactly `MAX_CBS` slots, then assert the next
    // one is refused. Small rx buffers so the slot table, not the arena,
    // is the bound being tested.
    let session = MockSession::new();
    let mut executor = executor_with_clock(session);
    let nid = executor
        .node_builder("test_entry_slots_exhausted")
        .build()
        .unwrap();

    for i in 0..crate::config::MAX_CBS {
        let topic = alloc::format!("/slot{i}");
        let r = executor
            .node_mut(nid)
            .subscription(&topic)
            .qos(QoSProfile::default().keep_last(1))
            .typed::<TestMsg>()
            .rx_buffer::<64>()
            .build(|_msg: &TestMsg| {});
        assert!(
            r.is_ok(),
            "registration {i} of MAX_CBS={} failed with {r:?} — if this is \
             BufferTooSmall the ARENA is the binding constraint, not the \
             slot table, and this test needs splitting (see #545)",
            crate::config::MAX_CBS
        );
    }

    // One past the table. Issue 0095: distinct `ExecutorFull` (capacity),
    // not the generic `BufferTooSmall`.
    let result = executor
        .node_mut(nid)
        .subscription("/one-too-many")
        .qos(QoSProfile::default().keep_last(1))
        .typed::<TestMsg>()
        .rx_buffer::<64>()
        .build(|_msg: &TestMsg| {});
    assert_eq!(result, Err(NodeError::ExecutorFull));
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
    let mut executor: Executor = executor_with_clock(session);

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
    let mut executor = executor_with_clock(session);

    let result = executor.spin_once(core::time::Duration::from_millis(0));
    assert!(!result.any_work());
}

#[test]
fn test_arena_alignment() {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);

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

// ===========================================================================
// phase-436 W7.b (issue 1242) — the park granularity comes from the installed
// primitive, not from a constant.
// ===========================================================================

/// With no primitive installed, 1 ms stands: that is the honest floor of the
/// `nros_platform_wake_wait_ms` ABI every port exports today.
#[test]
fn granularity_falls_back_to_the_abi_floor_with_no_primitive() {
    let executor: Executor = executor_with_clock(MockSession::new());
    assert_eq!(executor.park_granularity_us(), 1_000);
}

/// ThreadX pins `TX_TIMER_TICKS_PER_SECOND = 100` — a 10 ms tick. A port that
/// says so must be believed, or the executor reports a 1 ms park as achievable
/// on a platform that cannot deliver one.
#[test]
fn a_coarser_port_granularity_is_honoured() {
    unsafe extern "C" fn park(_c: *mut core::ffi::c_void, _d: u64) -> i8 {
        1
    }
    let mut executor: Executor = executor_with_clock(MockSession::new());
    executor.set_park_primitive(park, core::ptr::null_mut(), 10_000);

    assert_eq!(executor.park_granularity_us(), 10_000);
    assert_eq!(
        executor.round_park_up_us(1_000),
        10_000,
        "a 1 ms request on a 10 ms tick must round up to what the tick can do"
    );
}

/// POSIX's `sem_timedwait` is timespec-native, so a port may declare finer
/// than a millisecond. The old constant floored it at 1 ms and threw the
/// precision away before the primitive was ever called.
#[test]
fn a_finer_port_granularity_is_honoured() {
    unsafe extern "C" fn park(_c: *mut core::ffi::c_void, _d: u64) -> i8 {
        1
    }
    let mut executor: Executor = executor_with_clock(MockSession::new());
    executor.set_park_primitive(park, core::ptr::null_mut(), 1);

    assert_eq!(executor.park_granularity_us(), 1);
    assert_eq!(
        executor.round_park_up_us(1_500),
        1_500,
        "a microsecond-capable port must not be rounded to the ABI floor"
    );
}

/// The jitter report follows the same source. W3 reads the granularity to say
/// what its number is worth, so a wrong granularity makes W3 over-claim — the
/// failure W3 exists to prevent, in W3's own dependency.
#[test]
fn the_jitter_granularity_follows_the_port_not_the_constant() {
    unsafe extern "C" fn park(_c: *mut core::ffi::c_void, _d: u64) -> i8 {
        1
    }
    let mut executor: Executor = executor_with_clock(MockSession::new());
    executor.set_park_primitive(park, core::ptr::null_mut(), 10_000);
    // No declared cadence, so the wait paces the loop and its granularity is
    // the floor on the measurement.
    assert_eq!(executor.release_jitter_granularity_us(), 10_000);
}

// ===========================================================================
// phase-436 W6 — the wake-source seam. Many sources say WHEN; one primitive
// does the waiting.
// ===========================================================================

/// A registered park primitive must actually be USED, and handed the bound the
/// sources agreed on. Storing it without calling it would be the "a slot
/// exists, therefore it works" shape issue 0800 measured.
#[test]
fn spin_once_parks_through_the_registered_primitive() {
    use portable_atomic::{AtomicU64, Ordering};
    static SEEN: AtomicU64 = AtomicU64::new(u64::MAX);

    unsafe extern "C" fn recording_park(_c: *mut core::ffi::c_void, deadline_us: u64) -> i8 {
        SEEN.store(deadline_us, Ordering::SeqCst);
        1
    }

    SEEN.store(u64::MAX, Ordering::SeqCst);
    let mut executor: Executor = executor_with_clock(MockSession::new());
    executor.set_park_primitive(recording_park, core::ptr::null_mut(), 1_000);
    executor
        .register_timer(TimerDuration::from_millis(10), || {})
        .unwrap();

    executor.spin_once(core::time::Duration::from_millis(50));

    assert_eq!(
        SEEN.load(Ordering::SeqCst),
        10_000,
        "the primitive must be handed the 10 ms timer bound, not the 50 ms budget"
    );
}

/// With no primitive installed nothing changes — every existing port keeps the
/// behaviour it has, which is what makes this seam additive.
#[test]
fn no_park_primitive_leaves_the_existing_path_alone() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    assert!(!executor.has_park_primitive());
    executor.spin_once(core::time::Duration::from_millis(5));
    assert_eq!(executor.last_park_achieved_us(), 5_000);
}

/// The BACKEND's next internal event competes in the same `min` and is
/// attributed to itself. It used to be pre-capped into the budget upstream, so
/// a park the session actually won was reported as `CallerBudget` — the
/// attribution named the wrong source.
#[test]
fn a_session_deadline_competes_and_is_attributed_to_the_session() {
    let executor: Executor = executor_with_clock(MockSession::new());
    let (bound_us, winner) = executor.next_wake_bound_attributed_us(50_000, Some(7_000));
    assert_eq!(bound_us, 7_000);
    assert_eq!(
        winner,
        super::spin::WakeSourceId::Session,
        "a park the backend won must say so, not blame the caller's budget"
    );
}

// Issue 2143 -- an exact bound needs a clock that does not move: the timer
// bound is measured from NOW, so a free-running clock shaves microseconds.
private_test_clock!(nearer_timer_vs_session_clock);

/// ... and it loses to a nearer timer like any other member.
#[test]
fn a_nearer_timer_beats_the_session_deadline() {
    nearer_timer_vs_session_clock::claim_at_us(1_000_000);
    let mut executor: Executor =
        executor_with_clock_fn(MockSession::new(), nearer_timer_vs_session_clock::now_us);
    executor
        .register_timer(TimerDuration::from_millis(2), || {})
        .unwrap();
    let (bound_us, winner) = executor.next_wake_bound_attributed_us(50_000, Some(7_000));
    assert_eq!(bound_us, 2_000);
    assert_eq!(winner, super::spin::WakeSourceId::Timer);
}

/// A platform source with a nearer deadline than the caller's budget wins,
/// and the attribution names it. This is the extension point that lets a port
/// contribute `k_poll` / `epoll` / a queue set without the core naming an RTOS.
#[test]
fn a_platform_wake_source_can_shorten_the_park() {
    unsafe extern "C" fn due_in_3ms(_ctx: *mut core::ffi::c_void) -> u64 {
        3_000
    }
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let id = executor
        .register_wake_source(due_in_3ms, core::ptr::null_mut())
        .expect("the first source must fit");

    let (bound_us, winner) = executor.next_wake_bound_attributed_us(50_000, None);
    assert_eq!(bound_us, 3_000, "the platform source is due first");
    assert_eq!(
        winner, id,
        "and the attribution names that source, not a generic id"
    );
}

/// `u64::MAX` is how a source says "nothing pending" — an async source (a
/// socket, a DDS listener) contributes no deadline and instead breaks the park
/// by signalling. It must not collapse the bound to zero.
#[test]
fn a_source_with_nothing_pending_does_not_shorten_the_park() {
    unsafe extern "C" fn nothing(_ctx: *mut core::ffi::c_void) -> u64 {
        u64::MAX
    }
    let mut executor: Executor = executor_with_clock(MockSession::new());
    executor
        .register_wake_source(nothing, core::ptr::null_mut())
        .unwrap();

    let (bound_us, winner) = executor.next_wake_bound_attributed_us(50_000, None);
    assert_eq!(bound_us, 50_000);
    assert_eq!(winner, super::spin::WakeSourceId::CallerBudget);
}

/// The nearest of several sources decides, and the caller's budget is just
/// another member of the set.
#[test]
fn the_nearest_of_several_sources_wins() {
    unsafe extern "C" fn due_20ms(_c: *mut core::ffi::c_void) -> u64 {
        20_000
    }
    unsafe extern "C" fn due_4ms(_c: *mut core::ffi::c_void) -> u64 {
        4_000
    }
    let mut executor: Executor = executor_with_clock(MockSession::new());
    executor
        .register_wake_source(due_20ms, core::ptr::null_mut())
        .unwrap();
    let near = executor
        .register_wake_source(due_4ms, core::ptr::null_mut())
        .unwrap();

    let (bound_us, winner) = executor.next_wake_bound_attributed_us(50_000, None);
    assert_eq!(bound_us, 4_000);
    assert_eq!(winner, near);
}

// Issue 2143 -- an exact bound needs a clock that does not move: the timer
// bound is measured from NOW, so a free-running clock shaves microseconds.
private_test_clock!(timer_vs_platform_source_clock);

/// A timer still beats a platform source that is further out — W1's rule is
/// not special-cased away by the seam, it becomes one member of the same min.
#[test]
fn a_timer_still_competes_with_platform_sources() {
    timer_vs_platform_source_clock::claim_at_us(1_000_000);
    unsafe extern "C" fn due_30ms(_c: *mut core::ffi::c_void) -> u64 {
        30_000
    }
    let mut executor: Executor =
        executor_with_clock_fn(MockSession::new(), timer_vs_platform_source_clock::now_us);
    executor
        .register_wake_source(due_30ms, core::ptr::null_mut())
        .unwrap();
    executor
        .register_timer(TimerDuration::from_millis(6), || {})
        .unwrap();

    let (bound_us, winner) = executor.next_wake_bound_attributed_us(50_000, None);
    assert_eq!(bound_us, 6_000);
    assert_eq!(winner, super::spin::WakeSourceId::Timer);
}

// ==========================================================================
// Issue 1321 — which timers may bound a WALL park
// ==========================================================================

// Issue 2143 -- an exact bound needs a clock that does not move: the timer
// bound is measured from NOW, so a free-running clock shaves microseconds.
private_test_clock!(system_time_timer_park_clock);

/// A `TimerClockSource::System` timer DOES bound the park, and it is decided
/// separately from `Ros` rather than by "not `Steady`".
///
/// Its clock is wall time, so `period_us - elapsed_us` is already in the
/// units the park primitive waits in. Nothing can hold a system clock still,
/// which is the property `Ros` lacks; a step (NTP) makes the bound an
/// estimate, but one that can only shorten a park by a bounded amount and
/// never fire a callback early.
#[test]
fn a_system_time_timer_bounds_the_park() {
    system_time_timer_park_clock::claim_at_us(1_000_000);
    let mut executor: Executor =
        executor_with_clock_fn(MockSession::new(), system_time_timer_park_clock::now_us);
    executor
        .register_timer_on_clock(
            TimerDuration::from_millis(100),
            super::arena::TimerClockSource::System,
            || {},
        )
        .unwrap();

    let (bound_us, winner) = executor.next_wake_bound_attributed_us(500_000, None);
    assert_eq!(
        bound_us, 100_000,
        "a system-time timer's remaining time IS wall microseconds"
    );
    assert_eq!(winner, super::spin::WakeSourceId::Timer);
}

/// Issue 1321 — a ROS-time timer contributes NOTHING to the wall park bound.
///
/// `/clock` is held still, which is a paused bag. Before the fix the timer
/// offered `period_us - elapsed_us` — 100 ms of SIMULATED time — as a bound of
/// WALL microseconds, so the park collapsed from the caller's 500 ms budget to
/// 100 ms and stayed there for as long as the simulator was paused.
#[cfg(feature = "sim-time")]
#[test]
fn a_paused_ros_clock_does_not_bound_the_wall_park() {
    let _sim_time = SimTimeGuard::acquire();
    use nros_core::clock::Clock;

    let mut executor: Executor = executor_with_clock(MockSession::new());
    Clock::set_ros_time_override(10_000 * 1_000_000);
    executor
        .register_timer_on_clock(
            TimerDuration::from_millis(100),
            super::arena::TimerClockSource::Ros,
            || {},
        )
        .unwrap();

    let (bound_us, winner) = executor.next_wake_bound_attributed_us(500_000, None);
    assert_eq!(
        winner,
        super::spin::WakeSourceId::CallerBudget,
        "a ROS-time timer's next activation is a /clock MESSAGE, which wakes \
         drive_io like any sample; its simulated remainder is not a wall deadline"
    );
    assert!(
        bound_us >= 500_000,
        "the park must not be SHORTENED below the caller's budget by a timer \
         measured on another clock; got {bound_us} us"
    );
    assert_eq!(bound_us, 500_000);
}

/// ... and the one thing that costs: the image must not wake once per timer
/// period while the simulator is paused.
///
/// Four consecutive spins with `/clock` held still. Each park is asserted from
/// BELOW — `>= budget` — because an upper bound alone is satisfied by a park
/// that never waited at all, and shortening is precisely the failure here.
#[cfg(feature = "sim-time")]
#[test]
fn a_paused_ros_clock_does_not_wake_the_image_once_per_timer_period() {
    use nros_core::clock::Clock;
    use portable_atomic::{AtomicU64, AtomicUsize, Ordering};

    static SHORTEST_US: AtomicU64 = AtomicU64::new(u64::MAX);
    static PARKS: AtomicUsize = AtomicUsize::new(0);

    unsafe extern "C" fn recording_park(_c: *mut core::ffi::c_void, deadline_us: u64) -> i8 {
        SHORTEST_US.fetch_min(deadline_us, Ordering::SeqCst);
        PARKS.fetch_add(1, Ordering::SeqCst);
        1
    }

    let _sim_time = SimTimeGuard::acquire();
    SHORTEST_US.store(u64::MAX, Ordering::SeqCst);
    PARKS.store(0, Ordering::SeqCst);

    let mut executor: Executor = executor_with_clock(MockSession::new());
    executor.set_park_primitive(recording_park, core::ptr::null_mut(), 1_000);
    Clock::set_ros_time_override(10_000 * 1_000_000);

    let ticks = std::sync::Arc::new(AtomicUsize::new(0));
    let t = ticks.clone();
    executor
        .register_timer_on_clock(
            TimerDuration::from_millis(100),
            super::arena::TimerClockSource::Ros,
            move || {
                t.fetch_add(1, Ordering::SeqCst);
            },
        )
        .unwrap();

    const BUDGET_US: u64 = 500_000;
    for _ in 0..4 {
        executor.spin_once(core::time::Duration::from_micros(BUDGET_US));
    }

    assert_eq!(
        PARKS.load(Ordering::SeqCst),
        4,
        "each spin must reach the park primitive, or this measures nothing"
    );
    // phase-474 I11 -- the budget is the time left to the release grid point,
    // and this park returns at once, so later spins park the REST of the
    // 500 ms period. What must not happen is a park cut to the 100 ms ROS
    // timer.
    assert!(
        SHORTEST_US.load(Ordering::SeqCst) > BUDGET_US / 2,
        "a paused /clock made the image park {} us instead of the {} us budget \
         — once per timer period, forever",
        SHORTEST_US.load(Ordering::SeqCst),
        BUDGET_US
    );
    assert_eq!(
        ticks.load(Ordering::SeqCst),
        0,
        "and the timer itself must still not fire while /clock is paused"
    );
    assert_eq!(
        executor.last_park().1,
        super::spin::WakeSourceId::CallerBudget
    );
}

// Issue 2143 -- an exact bound needs a clock that does not move: the timer
// bound is measured from NOW, so a free-running clock shaves microseconds.
private_test_clock!(wall_timer_beside_ros_timer_clock);

/// phase-436 W1's property, unchanged: a WALL timer beside the ROS-time one
/// still wins the park at its own period. The fix skips one clock source, not
/// the timer deadline source.
#[cfg(feature = "sim-time")]
#[test]
fn a_wall_timer_still_bounds_the_park_beside_a_paused_ros_timer() {
    wall_timer_beside_ros_timer_clock::claim_at_us(1_000_000);
    let _sim_time = SimTimeGuard::acquire();
    use nros_core::clock::Clock;

    let mut executor: Executor = executor_with_clock_fn(
        MockSession::new(),
        wall_timer_beside_ros_timer_clock::now_us,
    );
    Clock::set_ros_time_override(10_000 * 1_000_000);
    executor
        .register_timer_on_clock(
            TimerDuration::from_millis(100),
            super::arena::TimerClockSource::Ros,
            || {},
        )
        .unwrap();
    executor
        .register_timer(TimerDuration::from_millis(10), || {})
        .unwrap();

    let (bound_us, winner) = executor.next_wake_bound_attributed_us(500_000, None);
    assert_eq!(
        bound_us, 10_000,
        "the wall timer keeps its phase-436 W1 bound"
    );
    assert_eq!(winner, super::spin::WakeSourceId::Timer);
}

/// Issue 1321, one function over: the spin-quantization audit (issue #515)
/// compares a timer's period against the declared spin period, both in wall
/// microseconds. A ROS-time timer's activations are quantised by the `/clock`
/// STEP instead, so the sentence the audit would print is about nothing.
///
/// Asserted on the decision rather than on the log line, because the audit
/// only LOGS — the same reason `spin_quantization_us` is recorded at all.
#[test]
fn the_spin_quantization_audit_speaks_only_of_wall_timers() {
    use super::{arena::TimerClockSource, spin::spin_quantization_span};

    assert_eq!(
        spin_quantization_span(TimerClockSource::Steady, 25_000, 10_000),
        Some((20_000, 30_000)),
        "a wall timer whose period is not a multiple of the spin period still \
         gets issue #515's warning"
    );
    assert_eq!(
        spin_quantization_span(TimerClockSource::System, 25_000, 10_000),
        Some((20_000, 30_000)),
        "so does a system-time timer: its activations ARE paced by the spin"
    );
    assert_eq!(
        spin_quantization_span(TimerClockSource::Ros, 25_000, 10_000),
        None,
        "a ROS-time timer's period and the spin period are in different clocks"
    );
    assert_eq!(
        spin_quantization_span(TimerClockSource::Steady, 20_000, 10_000),
        None,
        "an exact multiple has nothing to warn about"
    );
    assert_eq!(
        spin_quantization_span(TimerClockSource::Steady, 0, 10_000),
        None
    );
    assert_eq!(
        spin_quantization_span(TimerClockSource::Steady, 25_000, 0),
        None
    );
}

#[cfg(feature = "sim-time")]
private_test_clock!(budget_skip_ros_clock);

/// Issue 1321's third site, and the one that is NOT bounded: issue 0736's
/// budget-skip path advanced every timer's `elapsed_us` by the executor's WALL
/// delta, so a ROS-time timer bound to a starved sporadic context kept
/// accumulating while `/clock` was paused — and would eventually fire an
/// activation that no simulated time paid for.
#[cfg(feature = "sim-time")]
#[test]
fn a_budget_skipped_ros_timer_does_not_advance_on_wall_time() {
    budget_skip_ros_clock::claim_at_us(1_000_000);
    use crate::executor::sched_context::{OptUs, SchedClass, SchedContext};
    use nros_core::clock::Clock;

    let _sim_time = SimTimeGuard::acquire();
    let mut executor: Executor =
        executor_with_clock_fn(MockSession::new(), budget_skip_ros_clock::now_us);
    Clock::set_ros_time_override(10_000 * 1_000_000);

    // A wall timer that COSTS: callback runtime is what consumes a sporadic
    // budget, and one dispatch of this exhausts 1 us for the next 60 s. The
    // runtime is measured on the executor's clock, so the cost is an advance
    // of it — exactly 2 ms, where a real sleep was "2 ms or more".
    let spender = executor
        .register_timer(TimerDuration::from_millis(1), || {
            budget_skip_ros_clock::advance_ms(2);
        })
        .unwrap();
    let ros = executor
        .register_timer_on_clock(
            TimerDuration::from_millis(100),
            super::arena::TimerClockSource::Ros,
            || {},
        )
        .unwrap();

    let sc_id = executor
        .create_sched_context(SchedContext {
            class: SchedClass::Sporadic,
            budget_us: OptUs::from_us(1),
            period_us: OptUs::from_us(60_000_000),
            ..Default::default()
        })
        .unwrap();
    executor
        .bind_handle_to_sched_context(spender, sc_id)
        .unwrap();
    executor.bind_handle_to_sched_context(ros, sc_id).unwrap();

    // Cycle 1 spends the budget; cycles 2..5 are the skipped ones.
    for _ in 0..5 {
        let _ = budget_skip_ros_clock::elapse_then_spin_once(&mut executor, 50);
    }

    assert_eq!(
        executor.timer_elapsed_us(ros),
        Some(0),
        "a ROS-time timer skipped for want of budget must be advanced on ITS \
         clock, which did not move — not by the executor's wall delta"
    );
}

/// The table is fixed-capacity (no alloc). Overflow is an error, not a silent
/// drop — a source that was quietly discarded would leave the executor
/// sleeping past a deadline it was told about.
#[test]
fn registering_past_capacity_is_refused_not_dropped() {
    unsafe extern "C" fn nothing(_c: *mut core::ffi::c_void) -> u64 {
        u64::MAX
    }
    let mut executor: Executor = executor_with_clock(MockSession::new());
    for _ in 0..super::spin::MAX_WAKE_SOURCES {
        executor
            .register_wake_source(nothing, core::ptr::null_mut())
            .expect("within capacity");
    }
    assert!(
        executor
            .register_wake_source(nothing, core::ptr::null_mut())
            .is_err(),
        "past capacity must refuse rather than silently drop the source"
    );
}

/// The park primitive is singular: only one thing can actually block. Setting
/// it replaces whatever was there, and the executor reports which is live.
#[test]
fn the_park_primitive_is_singular_and_replaceable() {
    unsafe extern "C" fn park_a(_c: *mut core::ffi::c_void, _d: u64) -> i8 {
        0
    }
    let mut executor: Executor = executor_with_clock(MockSession::new());
    assert!(!executor.has_park_primitive(), "none installed by default");
    executor.set_park_primitive(park_a, core::ptr::null_mut(), 1_000);
    assert!(executor.has_park_primitive());
}

/// phase-509 P7 (RFC-0108 D8) -- an executor is level 0 until its runner
/// says otherwise, and every spin checks its level in with the time. Level 7
/// is used by no other test in this process, so its count is this test's.
#[test]
fn an_executor_reports_its_level_and_checks_it_in_on_every_spin() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    assert_eq!(executor.level(), 0);
    assert!(!executor.set_level(super::MAX_LEVELS as u8));
    assert!(executor.set_level(7));
    assert_eq!(executor.level(), 7);
    let before = super::level_spins(7);
    executor.spin_once(core::time::Duration::ZERO);
    executor.spin_once(core::time::Duration::ZERO);
    assert_eq!(super::level_spins(7), before + 2);
    assert!(
        super::level_last_spin_us(7).is_some(),
        "a clocked spin records its time"
    );
}

/// phase-509 P6 (RFC-0108 D3) -- the park and the stack record are the
/// executor's one worker's, and that worker runs every dispatch level: the
/// one-level facade over the scheduler / worker split.
#[test]
fn the_park_and_the_stack_record_are_the_one_workers() {
    unsafe extern "C" fn park_a(_c: *mut core::ffi::c_void, _d: u64) -> i8 {
        0
    }
    let mut executor: Executor = executor_with_clock(MockSession::new());
    assert_eq!(executor.worker.band, super::scheduler::Band::ALL);
    executor.set_park_primitive(park_a, core::ptr::null_mut(), 250);
    assert!(executor.worker.park_primitive.is_some());
    assert_eq!(executor.worker.park_granularity_declared_us, 250);
    executor.set_min_stack_headroom_bytes(512);
    assert_eq!(executor.worker.min_stack_headroom_bytes, 512);
}

// ===========================================================================
// phase-436 W1 (issue 1192) — the park is bounded by the next TIMER deadline.
// ===========================================================================

// ===========================================================================
// phase-436 W3 (issue 1194) — the jitter measurement states the granularity
// it was actually made at, instead of implying microsecond precision the
// pacing mechanism cannot deliver.
// ===========================================================================

/// A loop with no declared cadence is paced BY the blocking wait, so its
/// jitter can only be as fine as the wait — one platform granule.
#[test]
fn jitter_granularity_is_the_park_granule_when_the_wait_paces_the_loop() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    executor.spin_once(core::time::Duration::from_millis(10));

    assert_eq!(
        executor.release_jitter_granularity_us(),
        executor.park_granularity_us(),
        "with no declared cadence the wait is the pacing mechanism, so it is \
         also the floor on the measurement"
    );
}

/// A loop that DECLARES its cadence paces itself — nros-cpp's tiers sleep
/// `platform_sleep_us` between spins, which is finer than the blocking wait.
/// Reporting a millisecond floor there would understate a real measurement.
#[test]
fn a_declared_cadence_means_the_loop_paces_itself_at_microseconds() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    executor.set_spin_nominal_us(5_000);
    executor.spin_once(core::time::Duration::from_millis(10));

    assert_eq!(
        executor.release_jitter_granularity_us(),
        1,
        "a declared cadence is paced by the caller's own sleep, not by the wait"
    );
}

// ===========================================================================
// phase-436 W2 (issue 1193) — the park rounds UP to what the platform can
// actually achieve, and never truncates to a busy loop.
// ===========================================================================

/// A zero timeout means "poll, no cadence claimed". Rounding up must not
/// promote it into a 1 ms sleep — that would turn every `Future::wait` busy
/// drain into a throttled one.
#[test]
fn a_zero_park_stays_zero() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    executor.spin_once(core::time::Duration::ZERO);
    assert_eq!(executor.last_park_achieved_us(), 0);
}

/// Issue 2145 — a poll of `spin_async` must never park the calling thread,
/// because that thread is the async runtime's: a park there delays every
/// sibling future on a single-threaded runtime by the park's length. It used
/// to be `spin_once(1 ms)`, which parks for a granule on every poll.
///
/// The park bound is the executor's own record of what the pass asked the
/// platform for, so this needs no wall-clock measurement (a timing assertion
/// on a loaded host is a flake). It also pins the other half of the current
/// contract: the poll returns `Pending` and has woken its own waker, because
/// until an executor-level notifier exists (issue 2144) that self-wake is how
/// a runtime knows to poll it again.
#[test]
fn spin_async_poll_never_parks_and_wakes_itself() {
    use core::future::Future as _;
    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        task::{Context, Poll, Wake, Waker},
    };

    struct CountingWaker(AtomicUsize);
    impl Wake for CountingWaker {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
        fn wake_by_ref(self: &Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    let mut executor: Executor = executor_with_clock(MockSession::new());
    let counter = Arc::new(CountingWaker(AtomicUsize::new(0)));
    let waker = Waker::from(counter.clone());
    let mut cx = Context::from_waker(&waker);

    {
        let mut fut = core::pin::pin!(executor.spin_async());
        for poll in 1..=3 {
            assert!(
                matches!(fut.as_mut().poll(&mut cx), Poll::Pending),
                "spin_async never completes"
            );
            assert_eq!(
                counter.0.load(Ordering::SeqCst),
                poll,
                "each poll wakes itself exactly once (no notifier yet, issue 2144)"
            );
        }
    }

    assert_eq!(
        executor.last_park().0,
        0,
        "a spin_async poll asked the platform to park: it blocks the runtime thread"
    );
    assert_eq!(executor.last_park_achieved_us(), 0);
}

/// The defect: `as_millis()` truncates, so 500 us became 0 ms, which selects
/// the non-blocking path and turns `spin()` into a 100 % CPU loop. A park
/// shorter than the platform can express must round UP to what it can.
#[test]
fn a_sub_granularity_park_rounds_up_instead_of_collapsing_to_a_spin() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    executor.spin_once(core::time::Duration::from_micros(500));

    let achieved = executor.last_park_achieved_us();
    assert_eq!(
        achieved,
        executor.park_granularity_us(),
        "500 us must round up to one granule, not down to a busy loop; got {achieved} us"
    );
    assert!(achieved > 0, "a non-zero request must never park for zero");
}

/// Rounding is UP, not nearest and not down: 1500 us on a 1 ms-granular port
/// waits 2 ms. Truncating it to 1 ms is a 33 % short wait no caller asked for.
#[test]
fn a_park_rounds_up_never_down() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    executor.spin_once(core::time::Duration::from_micros(1_500));

    let achieved = executor.last_park_achieved_us();
    let g = executor.park_granularity_us();
    assert_eq!(
        achieved,
        2 * g,
        "1500 us must round up to two granules, got {achieved} us"
    );
}

/// A request that already lands on the grid is left alone — rounding must not
/// add a granule to a cadence that was already expressible.
#[test]
fn an_exact_multiple_is_not_inflated() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    executor.spin_once(core::time::Duration::from_millis(5));
    assert_eq!(executor.last_park_achieved_us(), 5_000);
}

/// The rounding must be VISIBLE. A monitor that reports jitter in µs against a
/// cadence the port cannot wait in is reporting precision it does not have
/// (issue 1194) — so the executor states what it actually achieved.
#[test]
fn the_achieved_park_is_reported_separately_from_the_request() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    executor.spin_once(core::time::Duration::from_micros(1_500));

    let (requested_us, _) = executor.last_park();
    assert_eq!(requested_us, 1_500, "the request is reported as asked");
    assert_ne!(
        executor.last_park_achieved_us(),
        requested_us,
        "and the achieved value differs, which is the whole point of reporting both"
    );
}

/// Issue 0515's audit must keep seeing the TIER's declared cadence, not the
/// park bound. Capping the park by the timer deadline (issue 1192) makes the
/// two differ, and if the audit follows the cap it compares the timer period
/// against itself — always an exact multiple, so the warning that issue 0515
/// exists to emit would go silent on every image that registers a timer.
#[test]
fn the_quantization_audit_sees_the_declared_cadence_not_the_park_bound() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    // The cap only bites when the timer is SHORTER than the budget, so that
    // is the case to test: a 33 ms timer under a 50 ms budget (the retired
    // ~~`spin_default`~~'s). If the
    // audit followed the cap it would compare 33 ms against 33 ms — an exact
    // multiple — and stay silent on a tier whose grid genuinely quantizes.
    executor
        .register_timer(TimerDuration::from_millis(33), || {})
        .unwrap();

    executor.spin_once(core::time::Duration::from_millis(50));

    assert_eq!(
        executor.spin_quantization_us, 50_000,
        "the audit must run against the 50 ms declared cadence, not the 33 ms park bound"
    );
}

// Issue 2143 -- an exact bound needs a clock that does not move: the timer
// bound is measured from NOW, so a free-running clock shaves microseconds.
private_test_clock!(spin_once_parks_on_timer_clock);

/// The bound is not merely COMPUTED — `spin_once` must USE it, and must
/// record which source won. Without that the helper could be correct and
/// unused: the shape issue 0736 recorded, a probe on a path no image takes.
#[test]
fn spin_once_parks_on_the_timer_and_says_so() {
    spin_once_parks_on_timer_clock::claim_at_us(1_000_000);
    let mut executor: Executor =
        executor_with_clock_fn(MockSession::new(), spin_once_parks_on_timer_clock::now_us);
    executor
        .register_timer(TimerDuration::from_millis(10), || {})
        .unwrap();

    executor.spin_once(core::time::Duration::from_millis(50));

    let (bound_us, source) = executor.last_park();
    assert_eq!(
        bound_us, 10_000,
        "the 10 ms timer must bound the 50 ms park, got {bound_us} us"
    );
    assert_eq!(
        source,
        super::spin::WakeSourceId::Timer,
        "the timer won this park, and the attribution must name it"
    );
}

/// With no timer, the caller's budget wins and is named as the winner — the
/// attribution has to distinguish "nothing else was pending" from "a timer
/// happened to match the budget".
#[test]
fn spin_once_attributes_an_unbounded_park_to_the_caller() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    executor.spin_once(core::time::Duration::from_millis(50));

    let (bound_us, source) = executor.last_park();
    assert_eq!(bound_us, 50_000);
    assert_eq!(source, super::spin::WakeSourceId::CallerBudget);
}

// Issue 2143 -- an exact bound needs a clock that does not move: the timer
// bound is measured from NOW, so a free-running clock shaves microseconds.
private_test_clock!(timer_shortens_budget_clock);

/// A registered timer is a deadline the EXECUTOR owns. A caller budget longer
/// than that deadline must not let the executor sleep past it.
#[test]
fn a_timer_deadline_shortens_a_longer_caller_budget() {
    timer_shortens_budget_clock::claim_at_us(1_000_000);
    let mut executor: Executor =
        executor_with_clock_fn(MockSession::new(), timer_shortens_budget_clock::now_us);
    executor
        .register_timer(TimerDuration::from_millis(10), || {})
        .unwrap();

    // A 50 ms budget (the retired ~~`spin_default`~~'s); the timer is due in 10 ms.
    let bound_us = executor.next_wake_bound_us(50_000);
    assert_eq!(
        bound_us, 10_000,
        "a 10 ms timer under a 50 ms budget must cap the park at 10 ms, got {bound_us} us"
    );
}

/// The budget still wins when it is the tighter of the two: capping must be a
/// `min`, not "the timer always decides".
#[test]
fn a_shorter_caller_budget_wins_over_a_later_timer() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    executor
        .register_timer(TimerDuration::from_millis(100), || {})
        .unwrap();

    let bound_us = executor.next_wake_bound_us(5_000);
    assert_eq!(
        bound_us, 5_000,
        "a 5 ms budget under a 100 ms timer must stay 5 ms, got {bound_us} us"
    );
}

/// With no timers registered the budget stands unchanged — the cap must not
/// invent a deadline and turn a long idle wait into a poll.
#[test]
fn no_timers_leaves_the_caller_budget_alone() {
    let executor: Executor = executor_with_clock(MockSession::new());
    assert_eq!(executor.next_wake_bound_us(50_000), 50_000);
}

// Issue 2143 -- an exact bound needs a clock that does not move: the timer
// bound is measured from NOW, so a free-running clock shaves microseconds.
private_test_clock!(nearest_timer_clock);

/// The nearest of several timers decides.
#[test]
fn the_nearest_timer_decides_the_bound() {
    nearest_timer_clock::claim_at_us(1_000_000);
    let mut executor: Executor =
        executor_with_clock_fn(MockSession::new(), nearest_timer_clock::now_us);
    executor
        .register_timer(TimerDuration::from_millis(40), || {})
        .unwrap();
    executor
        .register_timer(TimerDuration::from_millis(7), || {})
        .unwrap();
    executor
        .register_timer(TimerDuration::from_millis(25), || {})
        .unwrap();

    assert_eq!(
        executor.next_wake_bound_us(50_000),
        7_000,
        "the soonest of three timers is the one that bounds the park"
    );
}

/// A cancelled timer owes nothing, so it must not hold the park short. Without
/// this the cap would keep waking on a deadline nobody is waiting for.
#[test]
fn a_cancelled_timer_does_not_bound_the_park() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let id = executor
        .register_timer(TimerDuration::from_millis(10), || {})
        .unwrap();
    executor.cancel_timer(id).unwrap();

    assert_eq!(
        executor.next_wake_bound_us(50_000),
        50_000,
        "a cancelled timer is not a deadline"
    );
}

// ============================================================================
// Issue 2143 -- `next_deadline_us`, the Tier 0 time-out a host loop sleeps on
// ============================================================================

private_test_clock!(next_deadline_timer_clock);

/// A 10 ms timer is due in at most 10 ms, and -- the lower bound -- not
/// sooner: an answer of 0 would turn the host's sleep into a busy poll, and
/// one under 10 ms would wake it for nothing. The private clock makes both
/// bounds one exact value, then moves "now" WITHOUT spinning, so the answer
/// is shown to be measured from now rather than from the last spin.
#[test]
fn next_deadline_reports_a_10ms_timer_from_now() {
    next_deadline_timer_clock::claim_at_us(1_000_000);
    let mut executor: Executor =
        executor_with_clock_fn(MockSession::new(), next_deadline_timer_clock::now_us);
    executor
        .register_timer(TimerDuration::from_millis(10), || {})
        .unwrap();

    let due = executor
        .next_deadline_us()
        .expect("a registered timer is a deadline");
    assert!(
        due <= 10_000,
        "a 10 ms timer is due within 10 ms, got {due} us"
    );
    assert!(
        due >= 10_000,
        "and not sooner: nothing has elapsed on this clock, got {due} us"
    );

    // The host spends 4 ms in its own loop; the timer is 4 ms nearer.
    next_deadline_timer_clock::advance_ms(4);
    assert_eq!(executor.next_deadline_us(), Some(6_000));

    // Past the period it is due now, not "due in a negative time".
    next_deadline_timer_clock::advance_ms(20);
    assert_eq!(executor.next_deadline_us(), Some(0));
}

/// Nothing scheduled is `None`, not a made-up period: the host then waits on
/// readiness alone (issue 2144).
#[test]
fn next_deadline_is_none_with_nothing_scheduled() {
    let executor: Executor = executor_with_clock(MockSession::new());
    assert_eq!(executor.next_deadline_us(), None);
}

/// A cancelled timer owes nothing, and so is not a deadline a host wakes for.
#[test]
fn next_deadline_ignores_a_cancelled_timer() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let id = executor
        .register_timer(TimerDuration::from_millis(10), || {})
        .unwrap();
    executor.cancel_timer(id).unwrap();
    assert_eq!(executor.next_deadline_us(), None);
}

/// Every member of the park's `min` reaches the public answer, not just the
/// timers: a platform wake source nearer than the timer decides it.
#[test]
fn next_deadline_includes_a_platform_wake_source() {
    unsafe extern "C" fn due_in_3ms(_ctx: *mut core::ffi::c_void) -> u64 {
        3_000
    }
    let mut executor: Executor = executor_with_clock(MockSession::new());
    executor
        .register_timer(TimerDuration::from_millis(40), || {})
        .unwrap();
    executor
        .register_wake_source(due_in_3ms, core::ptr::null_mut())
        .unwrap();
    assert_eq!(executor.next_deadline_us(), Some(3_000));
}

private_test_clock!(park_since_last_spin_clock);

/// The executor's OWN park reads the same corrected bound: time the host
/// spent between two spins is time the timer already waited. Before issue
/// 2143 the park measured the remainder from the last spin's end and overslept
/// a timer by exactly that gap.
#[test]
fn spin_once_park_counts_time_spent_between_spins() {
    park_since_last_spin_clock::claim_at_us(1_000_000);
    let mut executor: Executor =
        executor_with_clock_fn(MockSession::new(), park_since_last_spin_clock::now_us);
    executor
        .register_timer(TimerDuration::from_millis(10), || {})
        .unwrap();
    executor.spin_once(core::time::Duration::ZERO);

    park_since_last_spin_clock::advance_ms(4);
    executor.spin_once(core::time::Duration::from_millis(50));

    let (bound_us, source) = executor.last_park();
    assert_eq!(
        bound_us, 6_000,
        "4 ms of a 10 ms timer passed between the spins, got {bound_us} us"
    );
    assert_eq!(source, super::spin::WakeSourceId::Timer);
}

private_test_clock!(add_timer_fire_clock);

#[test]
fn test_add_timer_and_fire() {
    add_timer_fire_clock::claim_at_us(1_000_000);
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock_fn(session, add_timer_fire_clock::now_us);

    let count = alloc::sync::Arc::new(portable_atomic::AtomicUsize::new(0));
    let count2 = count.clone();
    executor
        .register_timer(TimerDuration::from_millis(100), move || {
            count2.fetch_add(1, portable_atomic::Ordering::SeqCst);
        })
        .unwrap();

    // Not enough time elapsed — should not fire
    let result = add_timer_fire_clock::elapse_then_spin_once(&mut executor, 50);
    assert_eq!(result.timers_fired, 0);
    assert_eq!(count.load(portable_atomic::Ordering::SeqCst), 0);

    // Now enough time elapsed (50 + 60 = 110 >= 100)
    let result = add_timer_fire_clock::elapse_then_spin_once(&mut executor, 60);
    assert_eq!(result.timers_fired, 1);
    assert_eq!(count.load(portable_atomic::Ordering::SeqCst), 1);
}

#[cfg(feature = "sim-time")]
private_test_clock!(ros_timer_sim_clock);

/// phase-425 W4 — a `TimerClockSource::Ros` timer follows the SIMULATED clock,
/// and a wall timer on the same executor does not.
///
/// One test rather than four, deliberately: the ROS-time override is
/// process-global (one simulated clock per image, as in Rust rclrs), so four
/// tests would race each other inside the one test binary.
///
/// Merging them handles the four; it does not handle the OTHER tests that touch
/// the same globals, which is issue 1104. `SimTimeGuard` does that -- it orders
/// this against them and restores the override on the way out, replacing the
/// hand-written clear this test used to end with.
// Gated to match `SimTimeGuard` and `register_timer_on_clock`, both of which
// are `sim-time`-only. Issue 1104 added the guard to an UNGATED test, so
// `cargo test -p nros-node --features std` — no `sim-time` — stopped compiling
// with `cannot find type SimTimeGuard`. Invisible to `check node-std-tests`,
// which passes `std,sim-time,param-services` and therefore always has it.
#[cfg(feature = "sim-time")]
#[test]
fn ros_time_timer_follows_the_simulated_clock() {
    ros_timer_sim_clock::claim_at_us(1_000_000);
    let _sim_time = SimTimeGuard::acquire();
    use nros_core::clock::Clock;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const MS: i64 = 1_000_000;

    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock_fn(session, ros_timer_sim_clock::now_us);

    // A simulator is publishing /clock: t = 10 s.
    Clock::set_ros_time_override(10_000 * MS);

    let ros_ticks = std::sync::Arc::new(AtomicUsize::new(0));
    let wall_ticks = std::sync::Arc::new(AtomicUsize::new(0));
    let r = ros_ticks.clone();
    let w = wall_ticks.clone();

    executor
        .register_timer_on_clock(
            TimerDuration::from_millis(100),
            super::arena::TimerClockSource::Ros,
            move || {
                r.fetch_add(1, Ordering::SeqCst);
            },
        )
        .unwrap();
    executor
        .register_timer(TimerDuration::from_millis(100), move || {
            w.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap();

    // 1. PAUSED. Real time passes; simulated time does not. The wall timer
    //    fires and the ROS timer must not -- this is the whole point.
    for _ in 0..3 {
        let _ = ros_timer_sim_clock::elapse_then_spin_once(&mut executor, 120);
    }
    assert_eq!(
        ros_ticks.load(Ordering::SeqCst),
        0,
        "a ROS-time timer fired while the simulator was paused: 360 ms of REAL \
         time passed and /clock did not move, so no period elapsed"
    );
    // Exact, not `>= 3`: each 120 ms step on the private clock is one fire of
    // the 100 ms wall timer (20, 40, 60 ms carried), and nothing else moves it.
    assert_eq!(
        wall_ticks.load(Ordering::SeqCst),
        3,
        "the wall timer on the same executor must be unaffected by the pause \
         (that is what makes it the right clock for a watchdog); fired {} times",
        wall_ticks.load(Ordering::SeqCst)
    );

    // 2. RATE. Simulated time advances 100 ms per spin while only ~10 ms of real
    //    time passes: four ROS activations, and the wall timer stays behind.
    let wall_before = wall_ticks.load(Ordering::SeqCst);
    for i in 1..=4 {
        Clock::set_ros_time_override(10_000 * MS + i * 100 * MS);
        let _ = ros_timer_sim_clock::elapse_then_spin_once(&mut executor, 10);
    }
    assert_eq!(
        ros_ticks.load(Ordering::SeqCst),
        4,
        "four 100 ms steps of simulated time on a 100 ms ROS timer must be four \
         activations -- the timer tracks /clock, not the executor's spin delta"
    );
    // 60 ms carried + four 10 ms steps = exactly one wall period. On the wall
    // clock this was an UPPER bound (`<= 1`) that a descheduled host broke.
    assert_eq!(
        wall_ticks.load(Ordering::SeqCst) - wall_before,
        1,
        "40 ms of steady time cannot be four activations of a 100 ms wall timer"
    );

    // 3. BACKWARDS JUMP -- a bag looping, or a simulator reset. The period
    //    restarts; the timer must not stall for the length of the jump.
    Clock::set_ros_time_override(1_000 * MS); // back 9.4 s
    let _ = ros_timer_sim_clock::elapse_then_spin_once(&mut executor, 1);
    assert_eq!(
        ros_ticks.load(Ordering::SeqCst),
        4,
        "a backwards jump is not an elapsed period"
    );
    Clock::set_ros_time_override(1_100 * MS); // one period past the new origin
    let _ = ros_timer_sim_clock::elapse_then_spin_once(&mut executor, 1);
    assert_eq!(
        ros_ticks.load(Ordering::SeqCst),
        5,
        "after a backwards jump the NEXT period must fire on schedule; a timer \
         that accumulated the negative delta would stay dead for 9.4 s"
    );
}

#[cfg(feature = "sim-time")]
private_test_clock!(ros_timer_no_source_clock);

/// A `TimerClockSource::Ros` timer with NO `/clock` source still fires, at its
/// period, on the wall clock — issue 1334.
///
/// This is the acceptance the issue asks for, and it is the behaviour three
/// doc sites promise: `TimerClockSource::Ros` ("system time when none is, the
/// same fallback `rclcpp::Clock` has, so a node built for simulation still runs
/// standalone"), `Executor::register_timer_on_clock`, and phase-430's delta row
/// 15. Before the fix the ROS clock's no-override arm read an in-image counter
/// nothing advances, so `timer_clock_step` measured a step of 0 on every poll
/// and this timer could never reach its period.
///
/// # What this test is, and what it is not
///
/// It is NOT the negative control for 1334 — it cannot be, in this build
/// shape. `nros-node`'s test lane links no platform port (`nros-core`'s
/// `platform-clock` feature is off, checked with `cargo tree`), and with no
/// port the wall clock IS the counter, so the pre- and post-fix code read the
/// same static here and both pass. That is not a hole in the fix, it is the
/// precise statement of it: the defect is "ROS time falls back to the counter
/// instead of to the wall clock", and where there is no wall clock the counter
/// is the honest answer. The control lives where the two are separable — a
/// port-linked build: `nros-core`'s `ros_time_with_no_override_reads_the_port_
/// wall_clock` and `nros-c`'s `ros_time_agrees_with_the_rust_surface`.
///
/// What it IS: the end-to-end guard that the TIMER path follows this clock at
/// all, in the shape an embedded standalone image actually ships — which is
/// the half no clock-level unit test reaches.
#[cfg(feature = "sim-time")]
// The gate is the LOCK, not the feature: this test drives the process-global
// ROS time override (by leaving it cleared) and `SimTimeGuard` — itself
// `#[cfg(feature = "sim-time")]` — is what orders it against the sim-time tests
// that SET it. Without the shared guard the two race (issue 1104's shape).
#[test]
fn ros_timer_fires_on_its_period_with_no_clock_source_installed() {
    ros_timer_no_source_clock::claim_at_us(1_000_000);
    let _sim_time = SimTimeGuard::acquire();
    use nros_core::clock::Clock;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // No simulator: nobody has installed a `/clock` sample, which is the state
    // a standalone image boots in.
    Clock::clear_ros_time_override();
    assert!(
        !Clock::is_ros_time_override_active(),
        "precondition: this test is about the arm with NO override"
    );
    Clock::set_steady_time(1_700_000_000 * 1_000_000_000);

    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock_fn(session, ros_timer_no_source_clock::now_us);

    let ticks = std::sync::Arc::new(AtomicUsize::new(0));
    let t = ticks.clone();
    executor
        .register_timer_on_clock(
            TimerDuration::from_millis(100),
            super::arena::TimerClockSource::Ros,
            move || {
                t.fetch_add(1, Ordering::SeqCst);
            },
        )
        .unwrap();

    // Three periods of wall time. The counter bump is what makes this
    // deterministic in BOTH shapes: with no port linked it is the wall clock,
    // and with one linked it is a no-op beside the port's own clock.
    for _ in 0..3 {
        Clock::update_steady_time_ms(120);
        let _ = ros_timer_no_source_clock::elapse_then_spin_once(&mut executor, 1);
    }

    assert_eq!(
        ticks.load(Ordering::SeqCst),
        3,
        "a Ros timer standing alone must fire once per period of WALL time; \
         zero here is issue 1334 (the fallback was a counter nothing advances)"
    );

    // ...and it is the ROS clock it followed, not the executor's spin delta:
    // hold the clock still and the timer stops, which is what separates this
    // from a `Steady` timer.
    let before = ticks.load(Ordering::SeqCst);
    for _ in 0..3 {
        let _ = ros_timer_no_source_clock::elapse_then_spin_once(&mut executor, 120);
    }
    assert_eq!(
        ticks.load(Ordering::SeqCst),
        before,
        "with its clock held still a Ros timer must not fire, however much \
         real time the executor spends"
    );
}

/// phase-425 W3b — `use_sim_time` is a SWITCH, not a value, and declaring it
/// attaches the `/clock` source the way rclcpp's does.
///
/// The reconciliation, not the wire: a MockSession has no publisher to receive
/// from, so what this asserts is that the subscription gets installed at the
/// right moment and that the gate follows the parameter. The sample path is
/// covered by `time_source::tests` and the timer behaviour by
/// `ros_time_timer_follows_the_simulated_clock`.
#[cfg(all(feature = "sim-time", feature = "param-store"))]
// The `sim-time` gate is not decoration: `SimTimeGuard` is
// `#[cfg(feature = "sim-time")]`, so without it this body names an undeclared
// type (E0433) under the `--features std` invocation of `node-std-tests`.
// The sibling below carries the gate; this one was missed (issue 1059).
#[cfg(feature = "sim-time")]
#[test]
fn use_sim_time_attaches_and_detaches_the_clock_source() {
    let _sim_time = SimTimeGuard::acquire();
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);

    // Declared BEFORE any node exists, which is the order a generated entry
    // uses: `nros::main!` emits `apply_param_services` ahead of its per-node
    // `register` calls.
    assert!(executor.declare_parameter("use_sim_time", nros_params::ParameterValue::Bool(true)));
    assert!(
        !executor.ros_time_source_installed(),
        "declaring the parameter must not subscribe on the spot — there is no \
         node yet to hang the subscription on"
    );

    // A spin with no node must not install and must not give up either: the
    // request stays pending. This is the ordering the reconcile exists for.
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    assert!(
        !executor.ros_time_source_installed(),
        "there is still no node; installing would mean subscribing against a \
         node table entry that does not exist"
    );

    let _ = executor.create_node("sim_node").expect("create node");
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    assert!(
        executor.ros_time_source_installed(),
        "the first spin after the request must install the /clock source; \
         a request that never reconciles is a parameter that does nothing"
    );
    assert!(
        crate::time_source::is_active(),
        "installed and not armed would drop every sample"
    );

    // Idempotent: a second spin must not add a second subscription. A fresh
    // registration would take a new slot, so a stable handle is the evidence.
    let handle = executor.ros_time_source_handle();
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(
        executor.ros_time_source_handle(),
        handle,
        "reconciliation ran twice and subscribed twice"
    );

    // Turning it off stops SAMPLES. The subscription stays — the executor has
    // no entity removal — so the gate is what must change.
    //
    // phase-430 W3 / issue 1202 — through the STORE, which is the only way it
    // can happen: a second `declare_parameter` is a refusal (the name is
    // already there), and the switch no longer follows a refused write. This is
    // the `ros2 param set <node> use_sim_time false` path, verdict first.
    assert!(
        executor
            .params_mut()
            .expect("the declaration above created the store")
            .apply(
                super::node_record::NodeId::PRIMARY.into(),
                "use_sim_time",
                nros_params::ParameterValue::Bool(false),
            )
            .is_ok(),
        "an existing bool parameter set to a bool is an ordinary accepted set"
    );
    executor.refresh_use_sim_time_from_store();
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    assert!(
        !crate::time_source::is_active(),
        "use_sim_time went false and /clock samples would still be installed"
    );

    // The global and the override are put back by `SimTimeGuard`'s drop, which
    // restores what it observed instead of asserting a default.
}

/// phase-430 W3 / issue 1202 — the reserved-parameter switch follows the
/// STORE'S VERDICT, not the caller's proposal.
///
/// `note_reserved_parameter` used to run unconditionally, ahead of
/// `params.server.declare(...)`. A re-declaration is a REFUSAL — the name is
/// already there — so `declare_parameter("use_sim_time", false)` on a node that
/// had declared it `true` returned `false`, changed nothing in the store, and
/// still detached the `/clock` source. That left the executor in a state the
/// parameter cannot name: sim time recorded as ON, no source installed, and
/// nothing to reconcile it back, because the reconcile believes the switch.
///
/// The negative control for the fix is the middle block: on the pre-fix code it
/// reports `is_active() == false` while the store still reads `Some(true)`.
#[cfg(all(feature = "sim-time", feature = "param-store"))]
#[test]
fn use_sim_time_follows_the_store_verdict_not_the_caller() {
    let _sim_time = SimTimeGuard::acquire();
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);
    let _ = executor.create_node("verdict_node").expect("create node");

    // Accepted `true` attaches.
    assert!(
        executor.declare_parameter("use_sim_time", nros_params::ParameterValue::Bool(true)),
        "the first declaration of a name is an acceptance"
    );
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    assert!(
        executor.ros_time_source_installed(),
        "accepted true must attach"
    );
    assert!(
        crate::time_source::is_active(),
        "attached and not armed drops every sample"
    );

    // A REFUSED re-declaration must change neither half. Both are asserted,
    // because the defect was precisely the two halves disagreeing: checking
    // only the store, or only the source, reads as correct on the broken code.
    assert!(
        !executor.declare_parameter("use_sim_time", nros_params::ParameterValue::Bool(false)),
        "a second declaration of a declared name is a refusal, not a set"
    );
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(
        executor.params().and_then(|p| {
            p.get_bool(super::node_record::NodeId::PRIMARY.into(), "use_sim_time")
        }),
        Some(true),
        "a refused declaration must leave the stored value alone"
    );
    assert!(
        crate::time_source::is_active(),
        "the refused declaration detached the source anyway: the store keeps \
         `true` while /clock samples stop being installed, which is an executor \
         with sim time on and no clock source (issue 1202)"
    );
    assert!(
        executor.ros_time_source_installed(),
        "the subscription must survive a refused write"
    );

    // An ACCEPTED false — the wire-facing set, which is how a running node is
    // told — detaches.
    assert!(
        executor
            .params_mut()
            .expect("store")
            .apply(
                super::node_record::NodeId::PRIMARY.into(),
                "use_sim_time",
                nros_params::ParameterValue::Bool(false),
            )
            .is_ok()
    );
    executor.refresh_use_sim_time_from_store();
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    assert!(
        !crate::time_source::is_active(),
        "an accepted false must stop samples being installed"
    );

    // And back on: the switch is a switch, not a one-way latch.
    assert!(
        executor
            .params_mut()
            .expect("store")
            .apply(
                super::node_record::NodeId::PRIMARY.into(),
                "use_sim_time",
                nros_params::ParameterValue::Bool(true),
            )
            .is_ok()
    );
    executor.refresh_use_sim_time_from_store();
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    assert!(
        crate::time_source::is_active(),
        "an accepted true must re-arm the source that is still installed"
    );
}

/// phase-430 W3 / issue 1202 — the sibling declare path carries the same hook.
///
/// `declare_parameter_with_descriptor` had none at all, so the same reserved
/// name meant two different things depending on whether the app passed a
/// descriptor: `use_sim_time = true` with one stored a parameter and attached
/// nothing. Same class as the ordering above, found by sweeping the seam rather
/// than the reported site.
#[cfg(all(feature = "sim-time", feature = "param-store"))]
#[test]
fn use_sim_time_declared_with_a_descriptor_attaches_the_clock_source() {
    let _sim_time = SimTimeGuard::acquire();
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);
    let _ = executor
        .create_node("descriptor_node")
        .expect("create node");

    let descriptor =
        nros_params::ParameterDescriptor::new("use_sim_time", nros_params::ParameterType::Bool)
            .expect("descriptor")
            .with_description("use simulated time");
    assert!(executor.declare_parameter_with_descriptor(
        "use_sim_time",
        nros_params::ParameterValue::Bool(true),
        descriptor,
    ));
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    assert!(
        executor.ros_time_source_installed(),
        "a descriptor is metadata; it cannot change what the reserved name means"
    );
    assert!(crate::time_source::is_active());
}

/// phase-446 F2 -- a description longer than `NROS_MAX_PARAM_DESCRIPTION_LEN`
/// is stored truncated and reported by the NEXT spin, once.
///
/// The log line itself goes to `nros_log`; what this pins is the handover the
/// line depends on: the spin drains the store's record, so a second spin has
/// nothing left to report and the line cannot repeat.
#[cfg(feature = "param-store")]
#[test]
fn a_truncated_description_is_reported_by_the_next_spin_once() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let long = "x".repeat(nros_params::MAX_PARAM_DESCRIPTION_LEN + 1);
    let descriptor =
        nros_params::ParameterDescriptor::new("gain", nros_params::ParameterType::Double)
            .expect("name fits")
            .with_description(&long);
    assert!(executor.declare_parameter_with_descriptor(
        "gain",
        nros_params::ParameterValue::Double(1.0),
        descriptor,
    ));
    let stored = executor
        .params()
        .and_then(|s| s.get_descriptor(nros_params::NodeKey::PRIMARY, "gain"))
        .expect("declared with a descriptor");
    assert_eq!(
        stored.description.len(),
        nros_params::MAX_PARAM_DESCRIPTION_LEN,
        "the prefix is kept, not dropped whole"
    );
    assert!(stored.description_truncated(), "not reported before a spin");

    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    let mut again = 0;
    let reported = executor
        .params_mut()
        .expect("store")
        .take_truncated_descriptions(|_, _| again += 1);
    assert_eq!(
        (reported, again),
        (0, 0),
        "the spin reported it; nothing is left to report twice"
    );
}

/// phase-461 W6 -- `register_parameter_store` is the store without the
/// services: declarations land and nothing is requested, so no spin creates a
/// parameter queryable. With `param-services` compiled, a later
/// `register_parameter_services` keeps the same store and adds the request;
/// without it (issue 1897) there is no such method to call.
#[cfg(feature = "param-store")]
#[test]
fn the_parameter_store_registers_no_service() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    executor.register_parameter_store().expect("store builds");
    assert!(executor.params().is_some(), "the store exists");
    assert!(executor.declare_parameter("rate", nros_params::ParameterValue::Integer(5)));
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    executor
        .register_parameter_store()
        .expect("second call is a no-op");
    assert_eq!(
        executor.get_parameter_integer("rate"),
        Some(5),
        "store kept"
    );
    #[cfg(feature = "param-services")]
    {
        assert!(
            !executor.parameter_services_pending(),
            "a store-only registration requests no service"
        );
        assert!(
            executor
                .params
                .as_deref()
                .is_some_and(|p| !p.requested && p.services.is_empty()),
            "no set of six was requested or created"
        );
        executor
            .register_parameter_services()
            .expect("services join the store");
        assert_eq!(
            executor.get_parameter_integer("rate"),
            Some(5),
            "store kept"
        );
    }
}

// -- phase-426 W1/W2: the executor's parameter API --

/// phase-426 W2 -- `Executor::set_parameter` IS `ParameterServer::apply`.
///
/// The writer was missing entirely (`grep -c 'fn set_parameter'` was 0 in
/// `nros-node`), so `SetParameters` was a registered service with no
/// store-side writer any wrapper could reach, and the C facade wrote the slot
/// through `ParameterServer::set` -- a second answer to "may this set happen".
#[cfg(feature = "param-store")]
#[test]
fn executor_set_parameter_enforces_the_wire_rules() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let descriptor =
        nros_params::ParameterDescriptor::new("speed", nros_params::ParameterType::Double)
            .expect("name fits")
            .with_float_range(0.0, 10.0, 0.5);
    assert!(executor.declare_parameter_with_descriptor(
        "speed",
        nros_params::ParameterValue::Double(1.0),
        descriptor,
    ));

    // In range and on the step lattice.
    assert_eq!(
        executor.set_parameter("speed", nros_params::ParameterValue::Double(2.5)),
        Ok(())
    );
    assert_eq!(
        executor.get_parameter("speed").and_then(|v| v.as_double()),
        Some(2.5)
    );

    // Off the step: refused, and the stored value does not move.
    assert_eq!(
        executor.set_parameter("speed", nros_params::ParameterValue::Double(4.2)),
        Err(nros_params::ParameterError::OutOfRange)
    );
    assert_eq!(
        executor.get_parameter("speed").and_then(|v| v.as_double()),
        Some(2.5)
    );

    // Undeclared: refused with issue 1151's verdict, and NOT created.
    assert_eq!(
        executor.set_parameter("speeed", nros_params::ParameterValue::Double(1.0)),
        Err(nros_params::ParameterError::Undeclared)
    );
    assert!(executor.get_parameter("speeed").is_none());

    // The node option opens it, as upstream.
    executor.allow_undeclared_parameters(true);
    assert_eq!(
        executor.set_parameter("speeed", nros_params::ParameterValue::Double(1.0)),
        Ok(())
    );
}

/// phase-515 R10 -- the NODE is the parameter receiver: rclrs's
/// `declare_parameter::<T>(name)` builder on a `Node` needs no prior store
/// registration, each node's verbs reach its own parameters, and the rest of
/// the per-node surface forwards to the same store the executor forms used.
#[cfg(feature = "param-store")]
#[test]
fn the_node_is_the_parameter_receiver() {
    use super::node_record::NodeId;

    let mut executor: Executor = executor_with_clock(MockSession::new());
    let talker = NodeId::PRIMARY;
    let listener = NodeId::from_raw(1);

    {
        let mut node = executor.node_mut(talker);
        let rate = node
            .declare_parameter::<i64>("rate")
            .expect("the store comes into being on first use")
            .default(10)
            .mandatory()
            .expect("declared");
        assert_eq!(rate.get(), 10);
    }
    {
        let mut node = executor.node_mut(listener);
        let _ = node
            .declare_parameter::<i64>("rate")
            .expect("store")
            .default(20)
            .mandatory()
            .expect("a sibling's identical name is a different parameter");
        assert_eq!(
            node.get_parameter("rate").and_then(|v| v.as_integer()),
            Some(20)
        );
        assert_eq!(
            node.set_parameter("rate", nros_params::ParameterValue::Integer(21)),
            Ok(())
        );
        assert_eq!(
            node.get_parameter_type("rate"),
            Some(nros_params::ParameterType::Integer)
        );
        assert_eq!(node.list_parameters("").filter(|n| *n == "rate").count(), 1);
        assert!(node.set_parameter_read_only("rate", true));
        assert_eq!(
            node.set_parameter("rate", nros_params::ParameterValue::Integer(22)),
            Err(nros_params::ParameterError::ReadOnly)
        );
        assert_eq!(
            node.set_parameter("absent", nros_params::ParameterValue::Integer(1)),
            Err(nros_params::ParameterError::Undeclared)
        );
        node.allow_undeclared_parameters(true);
        assert_eq!(
            node.set_parameter("absent", nros_params::ParameterValue::Integer(1)),
            Ok(())
        );
        assert!(node.undeclare_parameter("absent"));
    }
    assert_eq!(
        executor
            .node_mut(talker)
            .get_parameter("rate")
            .and_then(|v| v.as_integer()),
        Some(10),
        "a set on one node moved its sibling's value"
    );
    assert!(
        executor.node_mut(talker).get_parameter("absent").is_none(),
        "allow_undeclared on one node leaked to its sibling"
    );
}

/// phase-515 R10 stage 1b -- the verbs only the executor's node-id-keyed forms
/// carried before those moved behind `ExecutorFfi` are on the node too.
#[cfg(feature = "param-store")]
#[test]
fn the_node_carries_the_whole_parameter_surface() {
    use nros_params::{ParameterError, ParameterValue};

    use super::node_record::NodeId;

    let mut executor: Executor = executor_with_clock(MockSession::new());
    let mut node = executor.node_mut(NodeId::PRIMARY);

    // rclc's LOCAL set is not bound by read-only; the remote-rules set is.
    let _ = node
        .declare_parameter::<i64>("ro")
        .expect("store")
        .default(1)
        .mandatory()
        .expect("declared");
    assert!(node.set_parameter_read_only("ro", true));
    assert_eq!(
        node.set_parameter("ro", ParameterValue::Integer(2)),
        Err(ParameterError::ReadOnly)
    );
    assert_eq!(
        node.set_parameter_local("ro", ParameterValue::Integer(3)),
        Ok(())
    );
    assert_eq!(
        node.get_parameter("ro").and_then(|v| v.as_integer()),
        Some(3)
    );

    // A remote NOT_SET is refused until the node opts into rclc's delete.
    let _ = node
        .declare_parameter::<i64>("gone")
        .expect("store")
        .default(7)
        .mandatory()
        .expect("declared");
    assert!(node.set_parameter("gone", ParameterValue::NotSet).is_err());
    assert!(node.get_parameter("gone").is_some());
    node.delete_parameters_on_not_set(true);
    assert_eq!(node.set_parameter("gone", ParameterValue::NotSet), Ok(()));
    assert!(node.get_parameter("gone").is_none(), "NOT_SET deleted it");

    // The fill-in-place atomic set writes every element.
    for name in ["a", "b"] {
        let _ = node
            .declare_parameter::<i64>(name)
            .expect("store")
            .default(0)
            .mandatory()
            .expect("declared");
    }
    assert_eq!(
        node.set_parameters_atomically_with(2, |i, slot| {
            *slot = ParameterValue::Integer(10 + i as i64);
            Some(if i == 0 { "a" } else { "b" })
        }),
        Ok(())
    );
    assert_eq!(
        node.get_parameter("a").and_then(|v| v.as_integer()),
        Some(10)
    );
    assert_eq!(
        node.get_parameter("b").and_then(|v| v.as_integer()),
        Some(11)
    );

    // No `-p` on this executor: nothing to list.
    assert_eq!(node.for_each_argv_param_override(|_, _| {}), 0);
    assert_eq!(node.argv_param_overrides().count(), 0);
}

/// phase-426 W1 -- two nodes on one executor keep their parameters apart, and
/// one node's `allow_undeclared` does not reach its sibling.
#[cfg(feature = "param-store")]
#[test]
fn two_nodes_on_one_executor_keep_their_parameters_apart() {
    use super::node_record::NodeId;

    let mut executor: Executor = executor_with_clock(MockSession::new());
    let talker = NodeId::PRIMARY;
    let listener = NodeId::from_raw(1);

    assert!(executor.declare_parameter_on(
        talker,
        "rate",
        nros_params::ParameterValue::Integer(10)
    ));
    assert!(
        executor.declare_parameter_on(listener, "rate", nros_params::ParameterValue::Integer(20)),
        "a sibling node's identical name is a different parameter"
    );

    assert_eq!(
        executor
            .get_parameter_on(talker, "rate")
            .and_then(|v| v.as_integer()),
        Some(10)
    );
    assert_eq!(
        executor
            .get_parameter_on(listener, "rate")
            .and_then(|v| v.as_integer()),
        Some(20)
    );

    assert_eq!(
        executor.set_parameter_on(talker, "rate", nros_params::ParameterValue::Integer(11)),
        Ok(())
    );
    assert_eq!(
        executor
            .get_parameter_on(listener, "rate")
            .and_then(|v| v.as_integer()),
        Some(20),
        "a set on one node moved its sibling's value"
    );

    // The node option is per node.
    executor.allow_undeclared_parameters_on(talker, true);
    assert_eq!(
        executor.set_parameter_on(talker, "fresh", nros_params::ParameterValue::Bool(true)),
        Ok(())
    );
    assert_eq!(
        executor.set_parameter_on(listener, "fresh", nros_params::ParameterValue::Bool(true)),
        Err(nros_params::ParameterError::Undeclared),
        "allow_undeclared leaked from one node to its sibling"
    );
}

/// issue 1272 -- launch parameters are seeded BEFORE the node they belong to
/// is built, keyed by the index `node_builder` will hand that node.
///
/// This is the order a generated C++ entry runs in: every node's launch
/// `<param>` seeds are declared, then the node is constructed, then the next
/// node's seeds. The seed key is therefore a PREDICTION of a `NodeId` that does
/// not exist yet, and it is right only because `build()` numbers nodes by table
/// position. This pins that: seed two nodes with the same parameter name and
/// different values, build them afterwards, and each node must read its own.
/// Before the fix every seed went to `NodeId::PRIMARY` and the second one was
/// refused as a duplicate.
#[cfg(feature = "param-store")]
#[test]
fn launch_seeds_made_before_construction_land_on_the_node_built_later() {
    use super::node_record::NodeId;

    let mut executor: Executor = executor_with_clock(MockSession::new());

    // The entry's first node: seeded while the node table is empty.
    assert_eq!(executor.nodes().len(), 0);
    assert!(executor.declare_parameter_on(
        NodeId::from_raw(0),
        "rate",
        nros_params::ParameterValue::Integer(10)
    ));
    let alpha = executor.node_builder("alpha").build().unwrap();

    // The second node: seeded with the first already built.
    assert_eq!(executor.nodes().len(), 1);
    assert!(
        executor.declare_parameter_on(
            NodeId::from_raw(1),
            "rate",
            nros_params::ParameterValue::Integer(20)
        ),
        "the same name on a second node is a different parameter"
    );
    let beta = executor.node_builder("beta").build().unwrap();

    // The keys the seeds predicted are the keys construction handed out.
    assert_eq!(alpha, NodeId::from_raw(0));
    assert_eq!(beta, NodeId::from_raw(1));

    assert_eq!(
        executor
            .get_parameter_on(alpha, "rate")
            .and_then(|v| v.as_integer()),
        Some(10),
        "alpha lost its own launch value"
    );
    assert_eq!(
        executor
            .get_parameter_on(beta, "rate")
            .and_then(|v| v.as_integer()),
        Some(20),
        "beta read a value seeded for another node"
    );

    // A component constructor declaring the name with its source default must
    // ADOPT the seed (issue 0745's contract), per node.
    assert!(
        !executor.declare_parameter_on(beta, "rate", nros_params::ParameterValue::Integer(99)),
        "a seeded name is already declared on its own node"
    );
    assert_eq!(
        executor
            .get_parameter_on(beta, "rate")
            .and_then(|v| v.as_integer()),
        Some(20),
        "the source default beat the launch value"
    );
}

/// phase-425 W3b — a non-bool `use_sim_time` attaches nothing.
///
/// ROS 2 lets a node declare the name with a wrong type; what must NOT happen is
/// a time source attaching off a value that does not mean "yes".
#[cfg(all(feature = "sim-time", feature = "param-store"))]
#[test]
fn a_non_bool_use_sim_time_attaches_nothing() {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);

    assert!(executor.declare_parameter("use_sim_time", nros_params::ParameterValue::Integer(1)));
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    assert!(
        !executor.ros_time_source_installed(),
        "an INTEGER 1 is not `true`; attaching off it would make the switch \
         answer to any truthy-looking value"
    );
}

// phase-430 W1 — a clock this test SETS, so it can cross a five-second
// threshold without sleeping five seconds. `test_clock_us` is free-running and
// shared by the whole process (issue 1105), so the interval would be measured
// against however much of the suite ran first.
#[cfg(all(feature = "sim-time", feature = "param-store"))]
private_test_clock!(sim_silence_clock);

/// phase-430 W1 — `use_sim_time` true and no `/clock` sample ever: the image
/// says so, ONCE.
///
/// The two states are indistinguishable from outside: an image whose simulator
/// is paused and an image whose `/clock` was never wired both sit there with
/// every ROS-time timer quiet. Only the image can tell them apart, because "no
/// sample has EVER arrived" is a fact about this subscriber and not about the
/// topic.
///
/// Asserted through `sim_time_silence_warnings()` rather than a log grep: the
/// contract is "exactly once", so the assertion has to be able to tell one from
/// two, and `nros_log`'s sink is not present on every target that runs this.
#[cfg(all(feature = "sim-time", feature = "param-store"))]
#[test]
fn use_sim_time_with_no_clock_sample_is_reported_exactly_once() {
    use nros_core::clock::Clock;

    let _sim_time = SimTimeGuard::acquire();
    // A precondition, not a courtesy: the watch's whole question is "has a
    // sample arrived", and a leftover override from a sibling test answers it
    // yes. The guard SERIALIZES against those tests and clears on the way out;
    // this clears on the way in, so the test does not depend on which of them
    // ran last.
    Clock::clear_ros_time_override();

    sim_silence_clock::claim_at_us(0);
    let mut executor: Executor =
        executor_with_clock_fn(MockSession::new(), sim_silence_clock::now_us);

    assert!(
        executor.declare_parameter("use_sim_time", nros_params::ParameterValue::Bool(true)),
        "the app's own declaration must be accepted"
    );
    let _ = executor.create_node("silent_sim").expect("create node");
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    assert!(
        executor.ros_time_source_installed(),
        "the /clock source must be attached before its silence means anything"
    );
    assert_eq!(
        executor.sim_time_silence_warnings(),
        0,
        "the interval starts at ATTACH; nothing has elapsed yet"
    );

    // One microsecond short of the threshold.
    sim_silence_clock::set_us(crate::time_source::SILENCE_WARN_US - 1);
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(
        executor.sim_time_silence_warnings(),
        0,
        "a simulator still coming up must not be accused of being absent"
    );

    // Crossing it.
    sim_silence_clock::set_us(crate::time_source::SILENCE_WARN_US);
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(
        executor.sim_time_silence_warnings(),
        1,
        "use_sim_time is on, /clock has never spoken, and the image said \
         nothing -- which is exactly the silence that reads as a paused \
         simulator"
    );

    // Far past it, many times over: still one. A diagnostic that repeats is
    // the noise it was written to replace.
    sim_silence_clock::set_us(crate::time_source::SILENCE_WARN_US * 4);
    for _ in 0..5 {
        let _ = executor.spin_once(core::time::Duration::from_millis(0));
    }
    assert_eq!(
        executor.sim_time_silence_warnings(),
        1,
        "the warning repeated; the record became the noise"
    );

    // Detach and re-attach: W1 asks for the one-shot to reset, because the
    // second attachment is a new question.
    assert!(
        executor
            .params_mut()
            .expect("the declaration above created the store")
            .apply(
                super::node_record::NodeId::PRIMARY.into(),
                "use_sim_time",
                nros_params::ParameterValue::Bool(false),
            )
            .is_ok()
    );
    executor.refresh_use_sim_time_from_store();
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    assert!(
        executor
            .params_mut()
            .expect("store")
            .apply(
                super::node_record::NodeId::PRIMARY.into(),
                "use_sim_time",
                nros_params::ParameterValue::Bool(true),
            )
            .is_ok()
    );
    executor.refresh_use_sim_time_from_store();
    let _ = executor.spin_once(core::time::Duration::from_millis(0));

    // ...and this time a sample DOES arrive before the interval. That answers
    // the question for good: a simulator that publishes and then stops is a
    // PAUSE, which is the state this diagnostic exists to be distinguished
    // from, so it must stay quiet about it however long the pause runs.
    sim_silence_clock::set_us(crate::time_source::SILENCE_WARN_US * 5);
    Clock::set_ros_time_override(1_000_000_000);
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    Clock::clear_ros_time_override();
    sim_silence_clock::set_us(crate::time_source::SILENCE_WARN_US * 20);
    for _ in 0..5 {
        let _ = executor.spin_once(core::time::Duration::from_millis(0));
    }
    assert_eq!(
        executor.sim_time_silence_warnings(),
        1,
        "/clock spoke and then stopped -- that is a pause, and warning about a \
         pause is the false alarm that teaches everyone to ignore the line"
    );
}

/// phase-430 W2 — `use_sim_time` is declared on a node that never named it,
/// as rclcpp does, so `ros2 param set <node> use_sim_time true` reaches an app
/// with no simulated-time code in it.
///
/// The store refuses an undeclared set (`allow_undeclared` is false), so before
/// this the runtime switch worked on exactly the nodes that least needed it:
/// the ones whose author had already thought about simulated time.
#[cfg(all(feature = "sim-time", feature = "param-store"))]
#[test]
fn use_sim_time_is_declared_on_a_node_that_never_named_it() {
    let _sim_time = SimTimeGuard::acquire();
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);

    // An app that knows nothing about simulated time: it declares one ordinary
    // parameter of its own and stops.
    assert!(executor.declare_parameter("start_value", nros_params::ParameterValue::Integer(0)));

    assert_eq!(
        executor.params().and_then(|p| {
            p.get_bool(super::node_record::NodeId::PRIMARY.into(), "use_sim_time")
        }),
        Some(false),
        "rclcpp declares use_sim_time=false on every node; a `ros2 param list` \
         here showed nothing until the app named it"
    );
    assert!(
        executor
            .params()
            .expect("store")
            .list_names(super::node_record::NodeId::PRIMARY.into())
            .any(|n| n == "use_sim_time"),
        "declared but not listed would be a parameter no tool can discover"
    );

    // The auto-declared default is NOT a statement by anyone: it must not touch
    // the process-global gate and must not attach a source. An image where
    // every executor wrote its default onto that gate is issue 1104.
    let _ = executor.create_node("plain").expect("create node");
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    assert!(
        !executor.ros_time_source_installed(),
        "a defaulted use_sim_time=false must cost an image that will never see \
         a simulator exactly nothing"
    );

    // `ros2 param set /plain use_sim_time true` — the wire path, which is the
    // whole point of the declaration existing.
    assert!(
        executor
            .params_mut()
            .expect("store")
            .apply(
                super::node_record::NodeId::PRIMARY.into(),
                "use_sim_time",
                nros_params::ParameterValue::Bool(true),
            )
            .is_ok(),
        "the set must reach a declared parameter; an undeclared one is refused"
    );
    executor.refresh_use_sim_time_from_store();
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    assert!(
        executor.ros_time_source_installed(),
        "a runtime `ros2 param set <node> use_sim_time true` must attach the \
         /clock source on a node with no sim-time code at all"
    );
}

/// Issue 1268 — a backend that will NEVER serve the parameter services is
/// asked once, not once per spin, and the report names the node and the
/// service.
///
/// The real one is Cyclone: it keys a topic on a registered type descriptor,
/// nothing registered the `rcl_interfaces` ones, and `create_service` answered
/// `Unsupported` forever while the image's own services worked. The executor
/// retried all six on every spin and (before 1271) discarded the result, so an
/// image with no `ros2 param` access said nothing at all about it.
///
/// `Unsupported` is the case that cannot change without a rebuild. A transport
/// that is merely late still gets retried — that is what `permanent` decides,
/// and why this test asserts the ATTEMPT COUNT rather than just the log.
#[cfg(feature = "param-services")]
#[test]
fn a_permanent_parameter_service_failure_is_asked_once_and_names_what_failed() {
    use core::sync::atomic::{AtomicUsize, Ordering};
    static ATTEMPTS: AtomicUsize = AtomicUsize::new(0);
    ATTEMPTS.store(0, Ordering::SeqCst);

    let session = MockSession::with_failing_service_create(TransportError::Unsupported, &ATTEMPTS);
    let cfg = ExecutorConfig::default();
    let mut executor: Executor = Executor::from_session_with(session, &cfg);
    executor.set_node_identity("bare", "/");
    executor
        .register_parameter_services()
        .expect("the request is recorded even with no node yet");

    for _ in 0..5 {
        let _ = executor.spin_once(core::time::Duration::from_millis(0));
    }

    let attempts = ATTEMPTS.load(Ordering::SeqCst);
    assert_eq!(
        attempts, 1,
        "a failure that cannot change must be asked ONCE: five spins made \
         {attempts} create_service call(s). Before issue 1268 this was six per spin, \
         for the life of the image."
    );

    let params = executor.params.as_ref().expect("the store exists");
    assert!(
        params.services.is_empty(),
        "no set of six can be serving when every create_service failed"
    );
    let failure = params
        .reconcile_failure
        .as_ref()
        .expect("the failure must be recorded, or the log can name neither node nor service");
    assert_eq!(
        failure.node_fqn.as_str(),
        "/bare",
        "the report must name the node `ros2 param` could not reach"
    );
    assert_eq!(
        failure.service, "get_parameters",
        "the report must name the service a tool would have called"
    );
    assert!(
        failure.permanent,
        "Unsupported is a property of the build; a retry asks the same question"
    );
    assert!(
        !executor.parameter_services_pending(),
        "a permanent failure is not pending work — that predicate is what stops the retry"
    );
}

/// phase-430 W2 — the same, for a node that declares NOTHING WHATEVER and only
/// registers the six parameter services.
///
/// That node is the one W2 is really for, and it reaches the store by a
/// different door: `register_parameter_services` used to build its own
/// `ParamState` rather than going through `ensure_parameter_store`, so a hook
/// placed only on the declaration path would have missed it entirely — the
/// two-write-paths shape issue 1202 was.
#[cfg(all(feature = "sim-time", feature = "param-services"))]
#[test]
fn use_sim_time_is_declared_by_registering_the_parameter_services_alone() {
    let _sim_time = SimTimeGuard::acquire();
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);

    // The six service names are built from the EXECUTOR's identity, which
    // `from_session_with` does not take from the config the way `open` does.
    executor.set_node_identity("bare", "/");
    let _ = executor.create_node("bare").expect("create node");
    executor
        .register_parameter_services()
        .expect("mock services register");

    assert_eq!(
        executor.params().and_then(|p| {
            p.get_bool(super::node_record::NodeId::PRIMARY.into(), "use_sim_time")
        }),
        Some(false),
        "a node that declared nothing still has use_sim_time in ROS 2, and \
         `ros2 param set` on it is the only way to move an already-running \
         image onto simulated time"
    );
}

/// phase-430 W2 — EVERY node, which is what the work item says and what
/// phase-426 made expressible.
///
/// rclcpp declares `use_sim_time` in every `rclcpp::Node` constructor, and an
/// image composes several nodes onto one executor — that is the model. Before
/// phase-426 the store was one flat table per executor, so "every node" and
/// "the executor" were the same sentence and the distinction could not be
/// tested. It is keyed by node now, and the six parameter services are
/// published per node, so the claim has a shape: the parameter is declared on
/// exactly the nodes a `ros2 param set <node> use_sim_time true` can reach —
/// which is the set `parameter_service_node_names` enumerates.
///
/// A seed on the primary alone would leave the second node's
/// `ros2 param set /beta use_sim_time true` refused as undeclared, which is
/// the pre-W2 behaviour for every node but the first.
#[cfg(all(feature = "sim-time", feature = "param-services"))]
#[test]
fn use_sim_time_is_declared_on_every_node_the_services_are_published_under() {
    let _sim_time = SimTimeGuard::acquire();
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);

    executor.set_node_identity("alpha", "/");
    let alpha = executor.node_builder("alpha").build().expect("alpha");
    let beta = executor.node_builder("beta").build().expect("beta");
    assert_ne!(alpha, beta, "precondition: two distinct node keys");
    executor
        .register_parameter_services()
        .expect("mock services register");

    assert_eq!(
        executor.parameter_service_node_names().len(),
        2,
        "precondition: one set of six per node (phase-426 W3)"
    );
    for node in [alpha, beta] {
        assert_eq!(
            executor
                .params()
                .and_then(|p| p.get_bool(node.into(), "use_sim_time")),
            Some(false),
            "node {} has parameter services but no use_sim_time; `ros2 param \
             set` on it would be refused as undeclared, which is the pre-W2 \
             behaviour this work item removes",
            node.raw()
        );
    }

    // The auto-declared default is still nobody's STATEMENT — seeding N nodes
    // must not become N writes to the process-global gate (issue 1104).
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    assert!(
        !executor.ros_time_source_installed(),
        "an image of nodes that will never see a simulator must pay nothing \
         for the declaration, however many nodes it has"
    );

    // An app naming `use_sim_time` on ONE node says nothing about the other:
    // beta's seed stays a placeholder for whoever declares it next.
    assert!(
        executor.declare_parameter_on(
            alpha,
            "use_sim_time",
            nros_params::ParameterValue::Bool(true)
        ),
        "alpha's own declaration must step alpha's placeholder aside"
    );
    assert_eq!(
        executor
            .params()
            .and_then(|p| p.get_bool(beta.into(), "use_sim_time")),
        Some(false),
        "declaring on alpha must not disturb beta's stored default"
    );
    assert!(
        executor.declare_parameter_on(
            beta,
            "use_sim_time",
            nros_params::ParameterValue::Bool(true)
        ),
        "beta's placeholder must still be a placeholder: a declaration on a \
         SIBLING node is not a declaration on this one"
    );
    assert!(
        !executor.declare_parameter_on(
            alpha,
            "use_sim_time",
            nros_params::ParameterValue::Bool(false)
        ),
        "alpha's placeholder is spent, so a SECOND declaration there is a \
         refusal exactly as it was before W2"
    );
}

/// phase-430 W2 — an application that declares `use_sim_time` ITSELF wins,
/// value and descriptor and all.
///
/// This is the failure mode the auto-declare invites: the store refuses a
/// duplicate name, so a default seeded first would turn the app's own
/// declaration into a refusal and quietly beat it. The ordering matters — the
/// app names another parameter FIRST, so the store (and the seed) already exist
/// when it gets to this one, which is the case a naive implementation gets
/// wrong.
#[cfg(all(feature = "sim-time", feature = "param-store"))]
#[test]
fn an_apps_own_use_sim_time_declaration_beats_the_auto_declared_default() {
    let _sim_time = SimTimeGuard::acquire();
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);

    assert!(executor.declare_parameter("start_value", nros_params::ParameterValue::Integer(0)));
    assert_eq!(
        executor.params().and_then(|p| {
            p.get_bool(super::node_record::NodeId::PRIMARY.into(), "use_sim_time")
        }),
        Some(false),
        "precondition: the seeded default is already in the store"
    );

    assert!(
        executor.declare_parameter("use_sim_time", nros_params::ParameterValue::Bool(true)),
        "the app's own declaration was refused because the auto-declared \
         default had taken the name"
    );
    assert_eq!(
        executor.params().and_then(|p| {
            p.get_bool(super::node_record::NodeId::PRIMARY.into(), "use_sim_time")
        }),
        Some(true),
        "the app asked for true and the defaulted false survived"
    );

    // And it is a real declaration, not just a stored value: it attaches the
    // source, which the seeded default deliberately does not.
    let _ = executor.create_node("app").expect("create node");
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    assert!(
        executor.ros_time_source_installed(),
        "the app's declaration must reach the reserved-parameter hook"
    );
}

/// phase-430 W2 — the descriptor-carrying sibling of the test above.
///
/// A declaration with a descriptor is a different function
/// (`declare_parameter_with_descriptor`), and the same seam has already been
/// wrong in exactly one of its two spellings once (issue 1202, where that path
/// had no reserved hook at all). Both or neither.
#[cfg(all(feature = "sim-time", feature = "param-store"))]
#[test]
fn an_apps_use_sim_time_declaration_with_a_descriptor_beats_the_default() {
    let _sim_time = SimTimeGuard::acquire();
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);

    assert!(executor.declare_parameter("start_value", nros_params::ParameterValue::Integer(0)));

    let descriptor =
        nros_params::ParameterDescriptor::new("use_sim_time", nros_params::ParameterType::Bool)
            .expect("the name fits a descriptor")
            .with_description("follow /clock");
    assert!(
        executor.declare_parameter_with_descriptor(
            "use_sim_time",
            nros_params::ParameterValue::Bool(true),
            descriptor,
        ),
        "the descriptor path must step the seeded default aside too"
    );
    assert_eq!(
        executor.params().and_then(|p| {
            p.get_bool(super::node_record::NodeId::PRIMARY.into(), "use_sim_time")
        }),
        Some(true)
    );
    assert!(
        executor
            .params()
            .and_then(|p| {
                p.get_descriptor(super::node_record::NodeId::PRIMARY.into(), "use_sim_time")
            })
            .is_some(),
        "the app's descriptor must survive; a seeded default carries none"
    );
}

/// Issue 1036 — arena exhaustion must REACH the boot record, not merely return
/// an error.
///
/// The issue's own resolution rests on the record being "unit-tested on the
/// host". Those tests exercise the RECORD (`boot_report::tests` — layout,
/// magic, monotonic stage). Nothing tested the LINK: that the allocator, on
/// failing, actually writes the number an operator would dump. That link is the
/// whole instrument — on the board this was written for, the console UART is
/// not wired, so the record is the only channel, and a record nobody writes to
/// is indistinguishable from the silence it was built to replace.
///
/// `arena_alloc_with_trailing` specifically, because that is the half that was
/// silent (issue 0900 fixed the sibling and left this one), and it is the half
/// carrying every buffered subscription and every action entry — what an island
/// image actually allocates.
///
/// The record is a process-global static and only the FIRST failure is kept
/// (`compare_exchange` from 0), so this test must be the ONLY one in its
/// process that records a failure. It is not named `boot_report*` for exactly
/// that reason: `boot_report::tests` has its own `note_alloc_failed(100, 8)`
/// case, and a shared filter put both in one binary — measured, this assertion
/// caught it reading `(100, 8)`. `just check boot-report-tests` runs the two
/// groups as SEPARATE cargo invocations, which is two processes and therefore
/// two records.
#[cfg(nros_boot_report)]
#[test]
fn arena_exhaustion_reaches_the_boot_record() {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);

    // Untouched to start with, which is also the assertion that nothing else in
    // this run got here first — if it fails, the filter let a sibling in and
    // every number below would be that sibling's.
    let before = crate::boot_report::snapshot();
    assert_eq!(
        (before.failed_alloc_size, before.failed_alloc_shortfall),
        (0, 0),
        "another test recorded an allocation failure before this one; the \
         record keeps only the FIRST, so this run cannot attribute it"
    );

    // `Executor::open` stamped `ExecutorReady` and nothing since. Asserted
    // rather than assumed, because the whole point of the next assertion is
    // that the stage MOVES.
    assert_eq!(
        before.stage,
        crate::boot_report::Stage::ExecutorReady as u32,
        "the executor is open and nothing has registered, so the stage must \
         read ExecutorReady; a different value means another test shared this \
         process"
    );

    // One byte more than the arena can hold, asked for through the path that
    // used to say nothing.
    let capacity = executor.arena.len();
    let ask = capacity + 1;
    let err = executor
        .arena_alloc_with_trailing::<u8>(ask)
        .expect_err("an allocation larger than the whole arena must fail");
    assert!(
        matches!(err, NodeError::BufferTooSmall),
        "unexpected error for an over-large arena request: {err:?}"
    );

    let after = crate::boot_report::snapshot();
    assert!(
        after.failed_alloc_size > 0,
        "the arena refused an allocation and the boot record says nothing. On a \
         board with no console this record is the ONLY channel, so a zero here \
         is the original defect with an instrument bolted on."
    );
    // The SHORTFALL is the number an operator adds to the knob, so it has to be
    // the real difference rather than a flag that something failed.
    assert!(
        after.failed_alloc_shortfall > 0,
        "size recorded but shortfall zero: the record names a failure without \
         naming what would fix it, and the shortfall is the actionable half"
    );
    assert!(
        u64::from(after.failed_alloc_shortfall) <= ask as u64,
        "shortfall {} exceeds the request {ask}, which cannot be a difference \
         against a non-negative capacity",
        after.failed_alloc_shortfall
    );

    // Issue 1036 -- the record must also say WHERE. Until this was wired the
    // stage stayed at `ExecutorReady`, which is the same value an image reports
    // when it opens an executor and dies before registering anything: the dump
    // named the allocation and could not distinguish the failure it exists to
    // catch from never having started. `RegisteringEntities` EXACTLY, not
    // `>=` -- a `>=` here would pass on a record some other test had already
    // pushed to `FirstSpin`, which is the shape that makes an assertion read as
    // coverage while checking nothing.
    assert_eq!(
        after.stage,
        crate::boot_report::Stage::RegisteringEntities as u32,
        "an arena allocation failed and the stage still reads {}; a dump that \
         names the shortfall but not the phase cannot separate 'ran out during \
         registration' from 'never registered anything'",
        after.stage
    );

    // And the record must survive a SECOND failure unchanged: the first
    // allocation to fail is the one that explains the boot, and a later,
    // larger, incidental failure overwriting it would replace the cause with a
    // symptom.
    let _ = executor.arena_alloc_with_trailing::<u8>(ask * 4);
    let again = crate::boot_report::snapshot();
    assert_eq!(
        (again.failed_alloc_size, again.failed_alloc_shortfall),
        (after.failed_alloc_size, after.failed_alloc_shortfall),
        "a second failure overwrote the first; the record must keep the \
         allocation that explains the boot, not the last one to be attempted"
    );
}

/// Issue 1036 -- the WHOLE reporting path, end to end, as far as a host can
/// take it: an arena that runs out, both channels that are supposed to carry
/// that fact, and the decoder an operator actually runs.
///
/// The sibling above (`arena_exhaustion_reaches_the_boot_record`) stops at
/// `snapshot()`, which reads the record through Rust. That is one half of the
/// instrument. The other half is everything between the `.bss` bytes and a
/// number a human types into a Kconfig file:
///
///   1. the record is findable in the ELF by SYMBOL, and the size the symbol
///      table reports is the length a `savemem` must ask for;
///   2. the bytes at that symbol, read as bytes rather than through Rust,
///      decode positionally under `scripts/read-boot-report.py`;
///   3. the decoder's verdict names ARENA EXHAUSTED, the stage it happened in,
///      and the value to set -- not merely "something failed".
///
/// None of that had a test. `check-boot-report-layout` compares two SOURCE
/// files and cannot see a dump; `boot_report::tests` reads through `Snapshot`
/// and never touches the script. So the tool an operator runs on the one board
/// this record was built for had never been run against a record produced by a
/// real failure, in any lane.
///
/// This also asserts the LOG channel, in the same failure, because "reported"
/// means both: `report_arena_exhausted` is what an image with a console gets,
/// and until now nothing asserted it fired from `arena_alloc_with_trailing` at
/// all -- `executor_arena_advisory.rs` asserts the over-provision ADVISORY,
/// which is a different function on a different latch. Deleting the
/// `report_arena_exhausted` call from the `_with_trailing` path passed every
/// test in the tree.
///
/// # What this still does NOT prove
///
/// The dump here is taken with a pointer, not with a debug probe, and the image
/// is a host test binary rather than a Zephyr image built with
/// `CONFIG_NROS_BOOT_REPORT=y`. The board run -- halt, `pyocd commander
/// savemem`, decode -- remains outstanding and issue 1036 stays open for it.
/// What has moved is that everything up to the probe is now exercised by a lane.
///
/// # Why its own cargo invocation
///
/// Two process-globals: the record keeps only the FIRST allocation failure, and
/// `report_arena_exhausted` is one-shot on `ARENA_EXHAUSTED_REPORTED`. Any other
/// test in this binary that exhausts an arena consumes both, and this one would
/// then assert on that test's numbers. `just check node-std-tests` runs it with
/// its own `--exact` filter for exactly that reason, and checks that the filter
/// matched something.
#[cfg(nros_boot_report)]
#[test]
fn an_exhausted_arena_decodes_to_the_knob_an_operator_must_set() {
    use std::io::Write as _;
    // It installs the process-global sink list below; without the lock a
    // sibling that also installs one (`two_nodes_on_one_executor_emit_under_
    // their_own_names`) can have its sink swapped out mid-assertion.
    let _sinks = log_sinks_lock();

    static CAPTURED: std::sync::Mutex<alloc::vec::Vec<alloc::string::String>> =
        std::sync::Mutex::new(alloc::vec::Vec::new());

    struct CapturingSink;
    impl nros_log::LogSink for CapturingSink {
        fn log(&self, record: &nros_log::Record<'_>) {
            // unwrap: poisoned only if another thread already panicked, which
            // would have failed this test anyway.
            CAPTURED
                .lock()
                .unwrap()
                .push(alloc::string::ToString::to_string(record.message));
        }
    }
    static SINK: CapturingSink = CapturingSink;
    static SINKS: &[&dyn nros_log::LogSink] = &[&SINK];
    nros_log::init(SINKS);

    let before = crate::boot_report::snapshot();
    assert_eq!(
        (before.failed_alloc_size, before.failed_alloc_shortfall),
        (0, 0),
        "another test recorded an allocation failure before this one; the \
         record keeps only the FIRST, so this run cannot attribute it"
    );

    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);
    let capacity = executor.arena.len();
    assert!(
        capacity > 0,
        "a zero-capacity arena is issue 0460, not a shortfall to measure"
    );

    // Through `arena_alloc_with_trailing`, the half that was silent and the
    // half every buffered subscription and every action entry takes.
    let err = executor
        .arena_alloc_with_trailing::<u8>(capacity + 1)
        .expect_err("an allocation larger than the whole arena must fail");
    assert!(
        matches!(err, NodeError::BufferTooSmall),
        "unexpected error for an over-large arena request: {err:?}"
    );

    // ---- channel 1: the log line, for an image that HAS a sink ------------
    let records = CAPTURED.lock().unwrap().clone();
    let exhausted = records
        .iter()
        .find(|m| m.contains("arena exhausted"))
        .unwrap_or_else(|| {
            panic!(
                "the arena-exhausted report reached no sink. {} record(s) \
                 captured: {records:?}",
                records.len()
            )
        });
    assert!(
        exhausted.contains("NROS_EXECUTOR_ARENA_SIZE"),
        "the exhaustion line must name the knob, not just say it failed: \
         {exhausted}"
    );
    assert!(
        !exhausted.contains('\u{2026}'),
        "the exhaustion line overflowed nros_log's format buffer and was \
         truncated, so its actionable half never reached the sink: {exhausted}"
    );
    // The #775 residue. The derived arena prices every subscription at the
    // budgeted QoS depth (phase-412 W3b), so a subscription deeper than it is
    // the ordinary way a derived arena runs out, and the line named neither the
    // knob nor the depth to compare against -- nor which entity ran out.
    let depth = crate::config::arena_model::BUDGETED_QOS_DEPTH;
    let budgeted = alloc::format!("NROS_PUBSUB_QOS_DEPTH (budgeted {depth})");
    assert!(
        exhausted.contains(&budgeted),
        "the exhaustion line must name the depth knob AND the depth this image \
         budgeted (`{budgeted}`): {exhausted}"
    );
    assert!(
        exhausted.contains("arena exhausted at u8:"),
        "the exhaustion line must name the entity that ran out (`u8` here, the \
         type this test allocated): {exhausted}"
    );
    // The WIDEST line, derived from this one rather than from a second copy of
    // the format string: every value swapped for its widest realistic width
    // (10-digit counts, a 5-digit depth, a 32-byte entity -- `entity_label`'s
    // cap). A line that fits here with room to spare can still truncate on a
    // board whose numbers are wider, which is how the first advisory was cut.
    let width = |n: u64| alloc::string::ToString::to_string(&n).len();
    // The `want` the line printed is the shortfall the record took.
    let shortfall = u64::from(crate::boot_report::snapshot().failed_alloc_shortfall);
    let widest = exhausted.len() - "u8".len() + 32
        - width(shortfall)
        - width(executor.arena_used() as u64)
        - width(capacity as u64)
        - width(u64::from(depth))
        + 3 * 10
        + 5;
    assert!(
        widest <= nros_log::format_buffer_capacity(),
        "at its widest the exhaustion line is {widest} bytes, past nros_log's \
         {}-byte format buffer; it would truncate on a board with larger \
         numbers: {exhausted}",
        nros_log::format_buffer_capacity()
    );

    // ---- channel 2: the record, read as BYTES and decoded by the script ---
    let snap = crate::boot_report::snapshot();
    assert!(
        snap.failed_alloc_shortfall > 0,
        "nothing to decode: the record did not take the shortfall"
    );

    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root is three levels above packages/core/nros-node")
        .to_path_buf();
    let script = repo.join("scripts/read-boot-report.py");
    assert!(
        script.is_file(),
        "the decoder an operator runs is missing: {}",
        script.display()
    );
    let elf = std::env::current_exe().expect("current_exe");

    // The address AND the length, from the ELF's own symbol table -- the two
    // numbers a `savemem` line is built from. A length that disagreed with the
    // struct would produce a short dump, and a short dump is the one failure
    // this record cannot report on its own.
    let addr_only = std::process::Command::new("python3")
        .arg(&script)
        .arg("--addr-only")
        .arg(&elf)
        .output()
        .expect("python3 is a hard requirement of this repo's lanes");
    assert!(
        addr_only.status.success(),
        "read-boot-report.py --addr-only failed on this binary: {}",
        alloc::string::String::from_utf8_lossy(&addr_only.stderr)
    );
    let addr_line = alloc::string::String::from_utf8_lossy(&addr_only.stdout);
    let symbol_len: u32 = addr_line
        .split_whitespace()
        .nth(1)
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("unparsable --addr-only output: {addr_line:?}"));
    assert_eq!(
        symbol_len,
        crate::boot_report::BootReport::struct_size(),
        "the ELF says the record is {symbol_len} bytes and the struct says {}; \
         an operator sizes `savemem` from the first and the decoder refuses \
         anything short of the second",
        crate::boot_report::BootReport::struct_size()
    );

    // The `savemem`, minus the probe: the record's own bytes, in memory order.
    // Not a re-serialisation of `Snapshot` -- that would assert the test's idea
    // of the layout rather than the compiler's, which is the half
    // `check-boot-report-layout` already cannot see.
    let len = crate::boot_report::BootReport::struct_size() as usize;
    // SAFETY: the record is a live `#[repr(C)]` static of exactly `len` bytes,
    // every field an `AtomicU32`. Reading it as bytes is what a debugger does;
    // no other thread writes it during this test.
    let dump: alloc::vec::Vec<u8> = unsafe {
        core::slice::from_raw_parts(
            (&raw const crate::boot_report::NROS_BOOT_REPORT).cast::<u8>(),
            len,
        )
    }
    .to_vec();

    // `$repo/tmp`, not `/tmp` (AGENTS.md); pid-suffixed so parallel lanes on one
    // checkout cannot collide.
    let tmp = repo.join("tmp");
    std::fs::create_dir_all(&tmp).expect("create $repo/tmp");
    let dump_path = tmp.join(alloc::format!("boot-report-{}.bin", std::process::id()));
    {
        let mut f = std::fs::File::create(&dump_path).expect("write the dump");
        f.write_all(&dump).expect("write the dump");
    }

    let decoded = std::process::Command::new("python3")
        .arg(&script)
        .arg(&elf)
        .arg(&dump_path)
        .output()
        .expect("python3");
    let _ = std::fs::remove_file(&dump_path);

    let out = alloc::string::String::from_utf8_lossy(&decoded.stdout).into_owned();
    let errs = alloc::string::String::from_utf8_lossy(&decoded.stderr).into_owned();
    assert_eq!(
        decoded.status.code(),
        Some(1),
        "the decoder must EXIT NON-ZERO on a record that carries a failure -- a \
         scripted board run reads the status, not the prose.\nstdout:\n{out}\n\
         stderr:\n{errs}"
    );
    assert!(
        out.contains("ARENA EXHAUSTED"),
        "the decoder read a record with a shortfall in it and did not say the \
         arena was exhausted:\n{out}\n{errs}"
    );
    // The number, not a symptom. `capacity + shortfall` is what the arena had
    // to have been for this allocation to fit, and it is the whole reason the
    // shortfall is stored rather than recomputed.
    let want = alloc::format!(
        "set NROS_EXECUTOR_ARENA_SIZE >= {}",
        capacity as u64 + u64::from(snap.failed_alloc_shortfall)
    );
    assert!(
        out.contains(&want),
        "the decoder must name the value to set (`{want}`), not merely report \
         that something did not fit:\n{out}"
    );
    // And WHERE. Issue 1036's other half: before `RegisteringEntities` was
    // wired this line read `ExecutorReady`, which is also what an image prints
    // when it never registered anything.
    assert!(
        out.contains("RegisteringEntities"),
        "the decoded stage must place the failure inside registration:\n{out}"
    );
}

private_test_clock!(timer_repeats_clock);

#[test]
fn test_timer_repeats() {
    timer_repeats_clock::claim_at_us(1_000_000);
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock_fn(session, timer_repeats_clock::now_us);

    let count = alloc::sync::Arc::new(portable_atomic::AtomicUsize::new(0));
    let count2 = count.clone();
    executor
        .register_timer(TimerDuration::from_millis(100), move || {
            count2.fetch_add(1, portable_atomic::Ordering::SeqCst);
        })
        .unwrap();

    // Fire 3 times
    let _ = timer_repeats_clock::elapse_then_spin_once(&mut executor, 100);
    let _ = timer_repeats_clock::elapse_then_spin_once(&mut executor, 100);
    let _ = timer_repeats_clock::elapse_then_spin_once(&mut executor, 100);
    assert_eq!(count.load(portable_atomic::Ordering::SeqCst), 3);
}

private_test_clock!(timer_readiness_clock);

/// phase-417 W5.c — `timer_is_ready` / `timer_elapsed_us` must report what the
/// DISPATCHER is about to do, not a second opinion about it.
///
/// These two exist because the C accessors `rcl_timer_is_ready` and
/// `rcl_timer_get_time_since_last_call` were filed as gaps: the state was
/// already in the arena and nothing exposed it. The risk in exposing it is a
/// readiness answer that drifts from `arena::timer_try_process`'s own guard,
/// so the assertion here is agreement — not-yet-due, due, and due-again after
/// a fire — plus the `None` that a non-timer handle must produce, since a
/// `false` there would report a live timer as merely not ready.
#[test]
fn timer_readiness_and_elapsed_agree_with_the_dispatcher() {
    // A PRIVATE clock the test moves by hand, so each step is exactly the
    // delta it names. This used to sleep for real against the shared
    // free-running clock and bound the readings above as well as below; under
    // a 32-way parallel gate one 40 ms sleep measured 52.9 ms and the upper
    // bound failed, though nothing about the dispatcher had changed.
    timer_readiness_clock::claim_at_us(1_000_000);
    let mut executor: Executor =
        executor_with_clock_fn(MockSession::new(), timer_readiness_clock::now_us);
    let mut now_us = 1_000_000;
    let mut step = |executor: &mut Executor, ms: u64| {
        now_us += ms * 1_000;
        timer_readiness_clock::set_us(now_us);
        executor.spin_once(core::time::Duration::from_millis(0))
    };

    let id = executor
        .register_timer(TimerDuration::from_millis(100), || {})
        .unwrap();

    assert_eq!(executor.timer_elapsed_us(id), Some(0));
    assert_eq!(executor.timer_is_ready(id), Some(false));

    // Short of the period: elapsed advances, readiness does not.
    let result = step(&mut executor, 40);
    assert_eq!(result.timers_fired, 0);
    assert_eq!(executor.timer_elapsed_us(id), Some(40_000), "one 40ms step");
    assert_eq!(executor.timer_is_ready(id), Some(false));

    let result = step(&mut executor, 40);
    assert_eq!(result.timers_fired, 0, "80ms is short of the 100ms period");
    assert_eq!(
        executor.timer_elapsed_us(id),
        Some(80_000),
        "two 40ms steps"
    );
    assert_eq!(executor.timer_is_ready(id), Some(false));

    let result = step(&mut executor, 40);
    assert_eq!(result.timers_fired, 1, "120ms >= 100ms period");
    // The activation rewound `elapsed_us` by one period, and that rewind is
    // exactly what "time since last call" means: the reading must drop below
    // the period rather than keep climbing.
    assert_eq!(
        executor.timer_elapsed_us(id),
        Some(20_000),
        "the fire must rewind by one period"
    );
    assert_eq!(executor.timer_is_ready(id), Some(false));

    // A cancelled timer is never ready, however much time has passed.
    executor.cancel_timer(id).unwrap();
    let _ = step(&mut executor, 500);
    assert_eq!(executor.timer_is_ready(id), Some(false));
    assert!(executor.timer_is_canceled(id));

    // A handle that is not a timer has no answer, which is not `false`.
    let not_a_timer = HandleId::new(id.slot() + 1, id.generation());
    assert_eq!(executor.timer_elapsed_us(not_a_timer), None);
    assert_eq!(executor.timer_is_ready(not_a_timer), None);
}

/// issue 1975 — `timer_clock_source` reports the clock each timer was
/// REGISTERED with (what `rclcpp::TimerBase::get_clock()` reads), and `None`
/// for a handle that is not a timer rather than defaulting to `Steady`.
#[test]
fn timer_clock_source_reports_the_registered_clock() {
    use super::arena::TimerClockSource;
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let wall = executor
        .register_timer(TimerDuration::from_millis(100), || {})
        .unwrap();
    let ros = executor
        .register_timer_on_clock(
            TimerDuration::from_millis(100),
            TimerClockSource::Ros,
            || {},
        )
        .unwrap();
    let system = executor
        .register_timer_on_clock(
            TimerDuration::from_millis(100),
            TimerClockSource::System,
            || {},
        )
        .unwrap();

    assert_eq!(
        executor.timer_clock_source(wall),
        Some(TimerClockSource::Steady)
    );
    assert_eq!(
        executor.timer_clock_source(ros),
        Some(TimerClockSource::Ros)
    );
    assert_eq!(
        executor.timer_clock_source(system),
        Some(TimerClockSource::System)
    );

    let not_a_timer = HandleId::new(system.slot() + 1, system.generation());
    assert_eq!(executor.timer_clock_source(not_a_timer), None);
}

private_test_clock!(time_until_next_clock);

/// phase-417 G6 — `timer_time_until_next_call_ns` is SIGNED, and the sign is
/// the answer.
///
/// The assertions moved here from `nros-c`'s ~~`remaining_period_ns`~~ when the
/// subtraction became the executor's: `cpp:Timer::time_until_trigger` reads
/// it too, and two crates deriving `period - elapsed` is how the sign
/// convention and the microsecond scaling come to disagree. rcl's header:
/// "a negative value indicates the timer call is overdue by that amount" —
/// unsigned made lateness read as `0`, which is also what "fires now" reads as.
///
/// Driven through the executor rather than asserted on a free function,
/// because that is what both wrappers actually call.
///
/// Issue 1950 — on a PRIVATE clock, so every reading is an equality. On the
/// shared wall clock the "60 ms remain" check had to be a range, and its floor
/// was the one load moves toward: `spin_once` credited the spin's real cost on
/// top of the 40 ms slept, so a loaded gate read 48.9 ms. An equality is both
/// bounds at once, so nothing that never advanced can pass it either.
#[test]
fn time_until_next_call_goes_negative_once_a_timer_is_overdue() {
    time_until_next_clock::claim_at_us(1_000_000);
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock_fn(session, time_until_next_clock::now_us);

    // A one-shot, so the fire does not rewind `elapsed_us` and the overdue
    // reading survives to be asserted.
    let id = executor
        .register_timer(TimerDuration::from_millis(100), || {})
        .unwrap();

    // Fresh: the whole period remains, in NANOSECONDS over a microsecond
    // accounting — 100 ms is 100_000 us is 100_000_000 ns.
    assert_eq!(
        executor.timer_time_until_next_call_ns(id),
        Some(100_000_000)
    );

    let _ = time_until_next_clock::elapse_then_spin_once(&mut executor, 40);
    let remaining = executor.timer_time_until_next_call_ns(id).unwrap();
    assert_eq!(
        remaining, 60_000_000,
        "40ms of a 100ms period gone, so 60ms remain"
    );

    // OVERDUE — on a ONE-SHOT that has already fired.
    //
    // Two shapes were measured and neither is this: a REPEATING timer cannot
    // stay overdue, because the arena drains the backlog (`elapsed_us -=
    // period_us`) until it is inside one period again, so even a 350ms step
    // on a 100ms period comes back reading ~10ms REMAINING; and a CANCELLED
    // timer's elapsed count does not advance at all, so its remaining time
    // stays wherever it was when it was cancelled. Both readings are correct
    // and neither is the sign this accessor exists to carry.
    //
    // A fired one-shot is: it is never dispatched again, so nothing rewinds
    // it, and the count keeps climbing past the period. That is exactly rcl's
    // "a negative value indicates the timer call is overdue by that amount",
    // and it is the case unsigned could not carry — issue 1008 reported it as
    // `0`, which is also what "fires now" reads as.
    let oneshot = executor
        .register_timer_oneshot(TimerDuration::from_millis(100), || {})
        .unwrap();
    let result = time_until_next_clock::elapse_then_spin_once(&mut executor, 150);
    assert!(
        result.timers_fired >= 1,
        "the one-shot must have fired (the repeating timer above fires too)"
    );
    let _ = time_until_next_clock::elapse_then_spin_once(&mut executor, 200);
    let overdue = executor.timer_time_until_next_call_ns(oneshot).unwrap();
    assert!(
        overdue < 0,
        "a fired one-shot's elapsed count keeps climbing, so the remaining \
         time must be NEGATIVE, got {overdue}ns"
    );
    // The MAGNITUDE is exact: the fire consumed one period and the
    // accounting stopped there, so what remains is the overshoot of the step
    // that fired it (50ms of the 150ms step), and the later 200ms step adds
    // nothing because nothing dispatches a fired one-shot. The assertion that
    // matters is the SIGN; the value is here so a reading that merely rounds
    // below zero cannot pass.
    assert_eq!(
        overdue, -50_000_000,
        "the overshoot of the firing step must survive as the overdue amount"
    );
    assert_eq!(
        executor.timer_is_ready(oneshot),
        Some(false),
        "overdue is not ready — a fired one-shot never fires again"
    );

    // A handle that is not a timer has no answer — not a remaining time of 0,
    // which is also what "fires now" reads as.
    assert_eq!(
        executor
            .timer_time_until_next_call_ns(HandleId::new(oneshot.slot() + 1, oneshot.generation())),
        None
    );
}

private_test_clock!(exchange_period_clock);

/// phase-417 W5.c — `exchange_timer_period_us` must change what the DISPATCHER
/// does, and must not touch the elapsed count while doing it.
///
/// The C entry point is `rcl_timer_exchange_period`, and ledger row
/// `c:timer_exchange_period` recorded the cost of not having it: a node whose
/// rate is a parameter had to destroy and recreate its timer. Two things could
/// go wrong in a way a compile cannot see, and each is asserted:
///
/// * the write lands somewhere the dispatcher does not read, so the timer keeps
///   its old cadence while `timer_period_us` reports the new one;
/// * the write RESETS `elapsed_us`. rcl's `rcl_timer_exchange_period` swaps the
///   period and touches nothing else, so a timer shortened below its
///   accumulated elapsed time must become ready AT ONCE rather than starting a
///   fresh period — which is the behaviour a rate-lowering parameter callback
///   depends on.
#[test]
fn exchanging_a_timers_period_changes_the_cadence_without_rewinding_it() {
    // A private clock moved by hand (see
    // `timer_readiness_and_elapsed_agree_with_the_dispatcher`): a real sleep
    // bounded above is a load test, not a dispatcher test.
    exchange_period_clock::claim_at_us(1_000_000);
    let mut executor: Executor =
        executor_with_clock_fn(MockSession::new(), exchange_period_clock::now_us);
    let mut now_us = 1_000_000;
    let mut step = |executor: &mut Executor, ms: u64| {
        now_us += ms * 1_000;
        exchange_period_clock::set_us(now_us);
        executor.spin_once(core::time::Duration::from_millis(0))
    };

    let id = executor
        .register_timer(TimerDuration::from_millis(100), || {})
        .unwrap();

    // 40 ms in: not due under the 100 ms period.
    let result = step(&mut executor, 40);
    assert_eq!(result.timers_fired, 0);
    let before = executor.timer_elapsed_us(id).unwrap();
    assert_eq!(before, 40_000, "one 40ms step");

    // Shorten the period to 20 ms. The old value comes back, in the arena's own
    // microseconds.
    assert_eq!(
        executor.exchange_timer_period_us(id, 20_000),
        Some(100_000),
        "the exchange reports the period it replaced"
    );
    assert_eq!(executor.timer_period_us(id), Some(20_000));

    // The elapsed count is untouched — this is the half a "reset it too" reading
    // of the API would get wrong.
    assert_eq!(
        executor.timer_elapsed_us(id),
        Some(before),
        "rcl exchanges the period and touches nothing else"
    );

    // And the DISPATCHER agrees: 40ms already elapsed is past a 20ms period, so
    // the timer is ready now rather than in another 60ms.
    assert_eq!(executor.timer_is_ready(id), Some(true));
    let result = step(&mut executor, 1);
    assert_eq!(
        result.timers_fired, 1,
        "a period shortened below the elapsed count fires immediately"
    );

    // Lengthening works the same way round: still no rewind, and the longer
    // period is what readiness is measured against.
    assert_eq!(executor.exchange_timer_period_us(id, 500_000), Some(20_000));
    assert_eq!(executor.timer_is_ready(id), Some(false));
    let result = step(&mut executor, 100);
    assert_eq!(result.timers_fired, 0, "100ms is short of the new 500ms");

    // A handle that is not a timer has no period to exchange, and reports that
    // rather than silently doing nothing.
    let not_a_timer = HandleId::new(id.slot() + 1, id.generation());
    assert_eq!(executor.exchange_timer_period_us(not_a_timer, 1_000), None);
}

private_test_clock!(oneshot_fires_once_clock);

#[test]
fn test_timer_oneshot_fires_once() {
    oneshot_fires_once_clock::claim_at_us(1_000_000);
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock_fn(session, oneshot_fires_once_clock::now_us);

    let count = alloc::sync::Arc::new(portable_atomic::AtomicUsize::new(0));
    let count2 = count.clone();
    executor
        .register_timer_oneshot(TimerDuration::from_millis(50), move || {
            count2.fetch_add(1, portable_atomic::Ordering::SeqCst);
        })
        .unwrap();

    // First spin fires
    let result = oneshot_fires_once_clock::elapse_then_spin_once(&mut executor, 60);
    assert_eq!(result.timers_fired, 1);
    assert_eq!(count.load(portable_atomic::Ordering::SeqCst), 1);

    // Second spin should NOT fire again
    let result = oneshot_fires_once_clock::elapse_then_spin_once(&mut executor, 60);
    assert_eq!(result.timers_fired, 0);
    assert_eq!(count.load(portable_atomic::Ordering::SeqCst), 1);
}

private_test_clock!(rclrs_timer_verbs_clock);

/// phase-515 R4 — rclrs's three timer verbs on the node handle: repeating
/// fires every period, one-shot fires once, inert fires nothing until
/// `reset_timer` starts it. `core::time::Duration` is accepted as rclrs passes.
#[test]
fn node_creates_rclrs_repeating_oneshot_and_inert_timers() {
    use portable_atomic::{AtomicUsize, Ordering::SeqCst};
    rclrs_timer_verbs_clock::claim_at_us(1_000_000);
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock_fn(session, rclrs_timer_verbs_clock::now_us);
    let nid = executor.node_builder("timers").build().unwrap();

    let rep = alloc::sync::Arc::new(AtomicUsize::new(0));
    let one = alloc::sync::Arc::new(AtomicUsize::new(0));
    let inert = alloc::sync::Arc::new(AtomicUsize::new(0));
    let (r, o, i) = (rep.clone(), one.clone(), inert.clone());
    let mut node = executor.node_mut(nid);
    node.create_timer_repeating(core::time::Duration::from_millis(50), move || {
        r.fetch_add(1, SeqCst);
    })
    .unwrap();
    node.create_timer_oneshot(TimerDuration::from_millis(50), move || {
        o.fetch_add(1, SeqCst);
    })
    .unwrap();
    let inert_id = node
        .create_timer_inert(core::time::Duration::from_millis(50), move || {
            i.fetch_add(1, SeqCst);
        })
        .unwrap();

    for _ in 0..3 {
        rclrs_timer_verbs_clock::elapse_then_spin_once(&mut executor, 60);
    }
    assert_eq!(rep.load(SeqCst), 3, "repeating fires every period");
    assert_eq!(one.load(SeqCst), 1, "one-shot fires once");
    assert_eq!(inert.load(SeqCst), 0, "inert never fires unstarted");
    assert!(executor.timer_is_canceled(inert_id));

    executor.reset_timer(inert_id).unwrap();
    for _ in 0..2 {
        rclrs_timer_verbs_clock::elapse_then_spin_once(&mut executor, 60);
    }
    assert_eq!(inert.load(SeqCst), 2, "a started inert timer repeats");
    assert_eq!(one.load(SeqCst), 1);
}

private_test_clock!(zero_delta_clock);

#[test]
fn test_timer_does_not_fire_at_zero_delta() {
    zero_delta_clock::claim_at_us(1_000_000);
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock_fn(session, zero_delta_clock::now_us);

    let count = alloc::sync::Arc::new(portable_atomic::AtomicUsize::new(0));
    let count2 = count.clone();
    let id = executor
        .register_timer(TimerDuration::from_millis(100), move || {
            count2.fetch_add(1, portable_atomic::Ordering::SeqCst);
        })
        .unwrap();

    // Zero delta should never fire. On a private clock the delta IS zero; on
    // the shared wall clock it was "however long registration took", which a
    // descheduled host stretched past the period (issue 1944's sweep).
    let result = executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(result.timers_fired, 0);
    assert_eq!(executor.timer_elapsed_us(id), Some(0), "no time passed");
}

private_test_clock!(timer_with_subs_clock);

#[test]
fn test_timer_with_subscriptions() {
    timer_with_subs_clock::claim_at_us(1_000_000);
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock_fn(session, timer_with_subs_clock::now_us);

    let timer_count = alloc::sync::Arc::new(portable_atomic::AtomicUsize::new(0));
    let timer_count2 = timer_count.clone();
    executor
        .register_timer(TimerDuration::from_millis(100), move || {
            timer_count2.fetch_add(1, portable_atomic::Ordering::SeqCst);
        })
        .unwrap();

    let nid = executor
        .node_builder("test_timer_with_subscriptions")
        .build()
        .unwrap();
    let sub_count = alloc::sync::Arc::new(portable_atomic::AtomicUsize::new(0));
    let sub_count2 = sub_count.clone();
    executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/test", move |_msg: &TestMsg| {
            sub_count2.fetch_add(1, portable_atomic::Ordering::SeqCst);
        })
        .unwrap();

    // Load data into subscription
    let (data, len) = encode_test_msg(99);
    let meta1 = executor.entries[1].as_ref().unwrap();
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    unsafe { &*(arena_ptr.add(meta1.offset) as *const MockSubscriber) }.load(data, len);

    let result = timer_with_subs_clock::elapse_then_spin_once(&mut executor, 100);
    assert_eq!(result.timers_fired, 1);
    assert_eq!(result.subscriptions_processed, 1);
    assert_eq!(timer_count.load(portable_atomic::Ordering::SeqCst), 1);
    assert_eq!(sub_count.load(portable_atomic::Ordering::SeqCst), 1);
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

// phase-380 W4 — was `#[cfg(rmw_needs_type_descriptors)]`: the schema is
// now required by `MessageForRmw` on EVERY backend, because that is where
// a subscription's build-time size bound comes from.
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

// phase-380 W4 — was `#[cfg(rmw_needs_type_descriptors)]`: the schema is
// now required by `MessageForRmw` on EVERY backend, because that is where
// a subscription's build-time size bound comes from.
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

// phase-380 W4 — was `#[cfg(rmw_needs_type_descriptors)]`: the schema is
// now required by `MessageForRmw` on EVERY backend, because that is where
// a subscription's build-time size bound comes from.
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
    let mut executor = executor_with_clock(session);

    let handle = executor
        .register_action_server_sized::<TestAction, _, _, 64, 64, 64, 1>(
            "/test_action",
            |_goal_id, _goal: &TestGoal| nros_core::GoalResponse::AcceptAndExecute,
            |_id: &nros_core::GoalUuid, _status: nros_core::GoalStatusCode| {
                nros_core::CancelResponse::Accept
            },
        )
        .unwrap();

    // Verify the entry was registered
    assert!(executor.entries[0].is_some());
    assert_eq!(handle.entry_index, 0);
}

#[test]
fn test_action_server_spin_once_no_requests() {
    let session = MockSession::new();
    let mut executor = executor_with_clock(session);

    let _handle = executor
        .register_action_server_sized::<TestAction, _, _, 64, 64, 64, 1>(
            "/test_action",
            |_goal_id, _goal: &TestGoal| nros_core::GoalResponse::AcceptAndExecute,
            |_id: &nros_core::GoalUuid, _status: nros_core::GoalStatusCode| {
                nros_core::CancelResponse::Accept
            },
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
    let mut executor = executor_with_clock(session);

    let _server_handle = executor
        .register_action_server_sized::<TestAction, _, _, 64, 64, 64, 1>(
            "/test_action",
            |_goal_id, _goal: &TestGoal| nros_core::GoalResponse::AcceptAndExecute,
            |_id: &nros_core::GoalUuid, _status: nros_core::GoalStatusCode| {
                nros_core::CancelResponse::Accept
            },
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
    let mut executor = executor_with_clock(session);

    let nid = executor
        .node_builder("test_drop_with_mixed_entries")
        .build()
        .unwrap();
    // Register one of each kind — use small buffers to fit in 4096-byte arena.
    executor
        .node_mut(nid)
        .subscription("/sub")
        .qos(QoSProfile::default().keep_last(1))
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
            |_id: &nros_core::GoalUuid, _status: nros_core::GoalStatusCode| {
                nros_core::CancelResponse::Accept
            },
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
    let mut executor: Executor = executor_with_clock(session);

    // elapsed < period → remaining = period - elapsed
    let r = executor.spin_one_period(100, 30);
    assert_eq!(r.remaining_ms, 70);
    assert_eq!(r.work.total(), 0);
}

#[test]
fn test_spin_one_period_overrun() {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);

    // elapsed > period → remaining saturates to 0
    let r = executor.spin_one_period(10, 50);
    assert_eq!(r.remaining_ms, 0);
}

#[test]
fn test_spin_one_period_exact() {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);

    // elapsed == period → remaining = 0
    let r = executor.spin_one_period(42, 42);
    assert_eq!(r.remaining_ms, 0);
}

#[test]
fn test_spin_options_default() {
    let opts = SpinOptions::default();
    assert!(opts.timeout.is_none());
    assert!(!opts.only_next);
    assert!(opts.max_callbacks.is_none());
    assert!(!opts.stop_on_first_error);
}

/// phase-427 — `SpinOptions` stopped `derive`ing `Default` when it gained
/// `poll_interval`, and this is why: `Duration::default()` is ZERO, so the
/// derive would have turned every `spin(SpinOptions::default())` in the tree
/// into a busy-poll that still passed every functional test. The only
/// observable is the value itself.
#[test]
fn spin_options_default_poll_interval_is_the_named_constant() {
    assert_eq!(
        SpinOptions::default().poll_interval,
        crate::executor::types::DEFAULT_SPIN_POLL_INTERVAL,
        "a zero (or divergent) default poll interval is a busy-poll no other \
         assertion in this suite can see"
    );
    assert!(!SpinOptions::default().poll_interval.is_zero());
    assert_eq!(
        SpinOptions::spin_once().poll_interval,
        crate::executor::types::DEFAULT_SPIN_POLL_INTERVAL,
        "the two const constructors must agree, or `spin_once()` is a second \
         default nobody knows about"
    );
}

#[test]
fn test_spin_options_builders() {
    let opts = SpinOptions::new()
        .timeout(core::time::Duration::from_secs(5))
        .max_callbacks(10);
    assert_eq!(opts.timeout, Some(core::time::Duration::from_secs(5)));
    assert_eq!(opts.max_callbacks, Some(10));
    assert!(!opts.only_next);

    let opts_once = SpinOptions::spin_once();
    assert!(opts_once.only_next);
}

// ====================================================================
// std-gated spin tests
// ====================================================================

#[test]
fn test_spin_only_next() {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);

    // only_next exits after single iteration
    let result = executor.spin(SpinOptions::spin_once());
    assert!(result.is_ok());
}

#[test]
fn test_spin_halt() {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);

    // Pre-set cancel flag → exits immediately. `cancel()`, not the deprecated
    // `halt()`: phase-417 W4.c renamed it to rclcpp's word, and `-D warnings`
    // makes the old spelling a hard error rather than a warning.
    executor.cancel();
    assert!(executor.is_halted());

    // spin resets halt then checks it — so we need a thread
    let halt = executor.halt_flag();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(50));
        halt.store(true, portable_atomic::Ordering::SeqCst);
    });
    let result = executor.spin(SpinOptions::default());
    assert!(result.is_ok());
}

/// issue 0709 — a timeout this build cannot measure is an ERROR, not "forever".
///
/// The clock is cleared directly rather than through a config, because
/// `ExecutorConfig`'s timing fields deliberately cannot clobber the platform
/// default with `None` (issue 0671): "not specified" and "there is none" are
/// different, and only a build with no clock source reaches the second. This
/// test synthesises that build.
///
/// It is the regression test for a ten-hour hang: `spin` used to read
/// "no clock" as "no deadline" and loop until halted, in the one API whose
/// contract is "returns after N ms".
#[test]
fn spin_with_a_timeout_and_no_clock_errors() {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);
    executor.clock_us_fn = None;

    let err = executor
        .spin(SpinOptions::new().timeout(core::time::Duration::from_millis(50)))
        .expect_err("a timeout with no clock must fail, not spin forever");
    assert_eq!(err, NodeError::NotInitialized);
}

/// …and an UNTIMED `spin` still runs, because "until halt" is a
/// promise a clockless build can keep. Halted from a peer thread so the test
/// cannot hang if the guard is ever widened by mistake.
#[test]
fn spin_without_a_timeout_still_runs_with_no_clock() {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);
    executor.clock_us_fn = None;

    let halt = executor.halt_flag();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(50));
        halt.store(true, portable_atomic::Ordering::SeqCst);
    });
    assert!(executor.spin(SpinOptions::new()).is_ok());
}

/// issue 0709 — the sibling guard: a period that cannot be paced is an error
/// rather than a busy-loop pretending to run at `period`.
#[test]
fn spin_period_with_no_clock_errors() {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);
    executor.clock_us_fn = None;

    let err = executor
        .spin_period(core::time::Duration::from_millis(10))
        .expect_err("a period with no clock must fail, not busy-loop");
    assert_eq!(err, NodeError::NotInitialized);
}

// ====================================================================
// spin_some — phase-427
// ====================================================================

/// `spin_some` executes what is ready and RETURNS. The whole point of the verb
/// is the return: `spin_once` with the same budget would sit on the transport
/// for it.
#[test]
fn spin_some_dispatches_the_ready_set_and_returns() {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);

    let nid = executor.node_builder("spin_some_drains").build().unwrap();
    let count = alloc::sync::Arc::new(portable_atomic::AtomicUsize::new(0));
    let count_cb = count.clone();
    executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/ready", move |_msg: &TestMsg| {
            count_cb.fetch_add(1, portable_atomic::Ordering::SeqCst);
        })
        .unwrap();

    let (data, len) = encode_test_msg(7);
    let meta = executor.entries[0].as_ref().unwrap();
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    unsafe {
        let sub_ptr = arena_ptr.add(meta.offset) as *const MockSubscriber;
        (*sub_ptr).load(data, len);
    }

    executor
        .spin_some(core::time::Duration::from_millis(500))
        .expect("a bounded drain on a build with a clock must succeed");

    assert_eq!(
        count.load(portable_atomic::Ordering::SeqCst),
        1,
        "the queued sample was ready, so the drain had to dispatch it"
    );
}

/// An idle executor drains in one pass and comes straight back — and it does so
/// with `Duration::ZERO`, which is rclcpp's own `spin_some` default and the
/// spelling a clockless build must use.
#[test]
fn spin_some_with_zero_max_returns_on_a_clockless_build() {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);
    executor.clock_us_fn = None;

    executor
        .spin_some(core::time::Duration::ZERO)
        .expect("ZERO means 'no limit', which needs no clock to honour");
}

/// issue 0709's rule, third site: a time quantity this build cannot MEASURE is
/// an error, never a silently unbounded loop. Same guard as `spin`'s `timeout`
/// and `spin_period`'s `period`.
#[test]
fn spin_some_with_a_max_and_no_clock_errors() {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);
    executor.clock_us_fn = None;

    let err = executor
        .spin_some(core::time::Duration::from_millis(50))
        .expect_err("a bounded drain with no clock must fail, not drain forever");
    assert_eq!(err, NodeError::NotInitialized);
}

// ====================================================================
// SpinOptions::stop_on_first_error — phase-427
// ====================================================================

/// Load a subscription that always fails its take, so the executor's dispatch
/// seam produces a real `TransportError` rather than a synthesised one.
fn executor_with_a_failing_subscription(name: &str) -> Executor<'static> {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);

    let nid = executor.node_builder(name).build().unwrap();
    executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/lossy", move |_msg: &TestMsg| {})
        .unwrap();

    let arena_ptr = executor.arena.as_ptr() as *const u8;
    let off = executor.entries[0].as_ref().unwrap().offset;
    unsafe { &*(arena_ptr.add(off) as *const MockSubscriber) }
        .load_error(nros_rmw::TransportError::MessageTooLarge);
    executor
}

/// rclrs returns `Vec<RclrsError>` and the caller writes `.first_error()?`.
/// There is no allocator here, so the FIRST error IS the return value — and it
/// has to be the error the callback actually produced, not a stand-in the
/// executor invented from a count.
#[test]
fn spin_returns_the_first_callback_error_when_asked_to_stop() {
    let mut executor = executor_with_a_failing_subscription("stop_on_first_error");

    let err = executor
        .spin(SpinOptions::default().stop_on_first_error(true))
        .expect_err("a failing callback must end a spin that asked to stop");
    assert_eq!(
        err,
        NodeError::Transport(nros_rmw::TransportError::MessageTooLarge),
        "the value has to survive: a count cannot tell the caller what broke"
    );
    assert!(
        !executor.is_spinning(),
        "the error exit must close the spin scope like every other exit, or a \
         peer reading is_spinning() sees a loop that is not running"
    );
}

/// The negative control, and the reason the field defaults to `false`: 27 call
/// sites were migrated from ~~`spin_blocking`~~ on the promise that the default
/// behaviour did not move. A counted-and-continue error is still the default.
#[test]
fn spin_continues_past_a_callback_error_by_default() {
    let mut executor = executor_with_a_failing_subscription("continue_past_error");

    executor
        .spin(SpinOptions::spin_once())
        .expect("the default channel counts a failed callback and keeps going");
}

/// issue 0709 — `from_session_with` is the seam `from_session` never had: a
/// caller that brings its own session can bring its own clock.
///
/// Asserted with `spin_once`, which is BOUNDED — it does one round and
/// returns. Three earlier versions of this test observed the clock through
/// `spin`/`spin_one_period_timed` instead and hung under the parallel
/// harness; a stub clock and a loop that re-reads a clock are a bad pair, and
/// the guard those versions were reaching for is already covered by the two
/// tests above. What is left to prove here is only that the constructor
/// INSTALLS what it is given, and a bounded call proves it without a loop.
#[test]
fn from_session_with_installs_the_callers_clock() {
    fn fake_clock() -> u64 {
        static TICKS: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
        TICKS.fetch_add(1_000, core::sync::atomic::Ordering::SeqCst)
    }

    let cfg = ExecutorConfig::default().clock_us(fake_clock);
    let mut executor: Executor = Executor::from_session_with(MockSession::new(), &cfg);
    // The clock is installed, so a timed spin is no longer refused by the
    // issue-0709 guard — and one bounded round is enough to reach it.
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    assert!(
        executor.clock_us_fn.is_some(),
        "from_session_with must install the config's clock"
    );
    assert!(
        executor.clock_us_fn.map(|c| c()).is_some(),
        "and it must be callable"
    );
}

#[test]
fn test_spin_timeout() {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);

    let start = std::time::Instant::now();
    let result = executor.spin(SpinOptions::new().timeout(core::time::Duration::from_millis(50)));
    assert!(result.is_ok());
    let elapsed = start.elapsed();
    // The subject IS the wall-clock wait, so this one stays on real time. A
    // LOWER bound too (issue 1944's sweep): an upper-bound-only check passes a
    // spin that returned without waiting at all. Load only lengthens the wait;
    // the 2 s ceiling is 40x the timeout.
    //
    // The floor is 50 ms less ONE microsecond, and that microsecond is the
    // executor's clock granule, not slack: `test_clock_us` truncates to whole
    // µs, so 50_000 µs between two truncated reads proves only > 49_999 µs of
    // real time between them. Measured: a `>= 50 ms` floor failed 8 of 20
    // parallel runs at 49.9993..49.9999 ms.
    assert!(
        elapsed >= std::time::Duration::from_micros(49_999),
        "spin returned after {elapsed:?}, before its 50ms timeout"
    );
    assert!(elapsed < std::time::Duration::from_secs(2));
}

private_test_clock!(one_period_timed_clock);

/// `spin_one_period_timed` reports an overrun from the executor's OWN clock.
///
/// Issue 1944 — on a private clock. On the wall clock "the mock returns
/// instantly, so no overrun" was an upper bound on the spin's real cost: a
/// host that descheduled the test for 50 ms reported an overrun that was true
/// of the host and not of the executor. Here a spin costs what its work
/// advances the clock by, so both directions are asserted exactly.
#[test]
fn test_spin_one_period_timed_no_overrun() {
    one_period_timed_clock::claim_at_us(1_000_000);
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock_fn(session, one_period_timed_clock::now_us);

    let period = std::time::Duration::from_millis(50);
    let result = executor.spin_one_period_timed(period);
    // No work, and no clock time spent: no overrun.
    assert!(!result.overrun);
    assert_eq!(result.elapsed, core::time::Duration::ZERO);
    assert_eq!(result.work.total(), 0);

    // A callback that costs 60 ms of a 50 ms period IS an overrun, measured
    // as exactly that cost.
    let (_id, guard) = executor
        .register_guard_condition(|| one_period_timed_clock::advance_ms(60))
        .expect("register_guard_condition");
    guard.trigger();
    let result = executor.spin_one_period_timed(period);
    assert!(result.overrun, "60 ms of work in a 50 ms period overruns");
    assert_eq!(result.elapsed, core::time::Duration::from_millis(60));
}

#[test]
fn test_halt_flag_clone() {
    let session = MockSession::new();
    let executor: Executor = executor_with_clock(session);

    let flag = executor.halt_flag();
    assert!(!executor.is_halted());

    flag.store(true, portable_atomic::Ordering::SeqCst);
    assert!(executor.is_halted());
}

#[test]
fn test_spin_period_halts() {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);

    let halt = executor.halt_flag();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(50));
        halt.store(true, portable_atomic::Ordering::SeqCst);
    });

    let result = executor.spin_period(std::time::Duration::from_millis(10));
    assert!(result.is_ok());
}

#[test]
fn test_wake_handle_clone() {
    let session = MockSession::new();
    let executor: Executor = executor_with_clock(session);

    let wake = executor.wake_handle();
    assert!(!wake.load(portable_atomic::Ordering::SeqCst));

    executor.wake();
    assert!(wake.load(portable_atomic::Ordering::SeqCst));
}

#[test]
fn test_wake_cleared_each_spin() {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);

    // Pre-arm the flag — spin_once must swap-clear it.
    executor.wake();
    let wake = executor.wake_handle();
    assert!(wake.load(portable_atomic::Ordering::SeqCst));

    let _ = executor.spin_once(core::time::Duration::from_millis(1));
    assert!(
        !wake.load(portable_atomic::Ordering::SeqCst),
        "spin_once must consume the wake flag"
    );
}

#[test]
fn test_halt_raises_wake_flag() {
    let session = MockSession::new();
    let executor: Executor = executor_with_clock(session);

    let wake = executor.wake_handle();
    assert!(!wake.load(portable_atomic::Ordering::SeqCst));

    executor.cancel();
    assert!(executor.is_halted());
    assert!(
        wake.load(portable_atomic::Ordering::SeqCst),
        "halt() must also set the wake flag so an in-flight spin_once \
         falls through to the halt check on its next iteration"
    );
}

#[test]
fn test_guard_handle_send_across_thread() {
    // Phase 124.B.7.d — GuardCondition must be Send (so a
    // worker thread / signal handler can own it and call trigger()).
    // Sync impl assertion via thread move and rejoin.
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);

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

/// Pre-arming wake_flag must not leave spin_once blocked in `drive_io` for
/// the caller's 200 ms tick.
///
/// Issue 1944 — asserted on what the executor ASKED the transport for, not on
/// how long the call took. The old `elapsed < 50 ms` timed a mock whose
/// `drive_io` returns at once whatever it is asked, so it could not fail on
/// the executor and could only fail on a host that descheduled the test.
///
/// In this build shape (`std`, mock session, no `rmw-cffi`) the primary drive
/// is non-blocking by construction (`spin.rs`'s std arm), so this pins that
/// arm. The wake-collapse arm proper is `rmw-cffi`-only and is exercised end
/// to end by `nros-c`'s `tests/run/node_guard_condition.c`.
#[test]
fn test_wake_short_circuits_drive_timeout() {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);

    executor.wake();
    let _ = executor.spin_once(core::time::Duration::from_millis(200));
    assert_eq!(
        executor.session().last_drive_timeout_ms,
        Some(0),
        "wake_flag set → spin_once must not hand the transport the 200ms tick"
    );
}

// ====================================================================
// Phase 49: HandleId / HandleSet / ReadinessSnapshot tests
// ====================================================================

#[test]
fn test_handle_id_from_add_subscription() {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);

    let nid = executor
        .node_builder("test_handle_id_from_add_subscription")
        .build()
        .unwrap();
    let id = executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/a", |_msg: &TestMsg| {})
        .unwrap();
    assert_eq!(id.slot(), 0);

    let id2 = executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/b", |_msg: &TestMsg| {})
        .unwrap();
    assert_eq!(id2.slot(), 1);
}

#[test]
fn test_handle_set_operations() {
    let a = HandleId::for_owned_slot(0);
    let b = HandleId::for_owned_slot(1);
    let c = HandleId::for_owned_slot(5);

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
    let set1 = HandleSet::EMPTY
        .insert(HandleId::for_owned_slot(0))
        .insert(HandleId::for_owned_slot(2));
    let set2 = HandleSet::EMPTY
        .insert(HandleId::for_owned_slot(1))
        .insert(HandleId::for_owned_slot(2));
    let union = set1 | set2;
    assert!(union.contains(HandleId::for_owned_slot(0)));
    assert!(union.contains(HandleId::for_owned_slot(1)));
    assert!(union.contains(HandleId::for_owned_slot(2)));
    assert_eq!(union.len(), 3);
}

#[test]
fn test_readiness_snapshot() {
    let snap = ReadinessSnapshot {
        bits: 0b101,
        count: 3,
    };
    assert!(snap.is_ready(HandleId::for_owned_slot(0)));
    assert!(!snap.is_ready(HandleId::for_owned_slot(1)));
    assert!(snap.is_ready(HandleId::for_owned_slot(2)));
    assert_eq!(snap.ready_count(), 2);
    assert_eq!(snap.total(), 3);

    let set = HandleId::for_owned_slot(0) | HandleId::for_owned_slot(2);
    assert!(snap.all_ready(set));
    assert!(snap.any_ready(set));

    let set2 = HandleId::for_owned_slot(0) | HandleId::for_owned_slot(1);
    assert!(!snap.all_ready(set2));
    assert!(snap.any_ready(set2));
}

// ====================================================================
// Phase 49: Trigger condition tests
// ====================================================================

#[test]
fn test_trigger_any_fires_on_data() {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);
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
    let mut executor: Executor = executor_with_clock(session);
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
    let mut executor: Executor = executor_with_clock(session);
    executor.set_trigger(Trigger::Always);
    let nid = executor
        .node_builder("test_trigger_always_fires_without_data")
        .build()
        .unwrap();

    let called = alloc::sync::Arc::new(portable_atomic::AtomicBool::new(false));
    let called2 = called.clone();
    let id = executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/test", move |_msg: &TestMsg| {
            called2.store(true, portable_atomic::Ordering::SeqCst);
        })
        .unwrap();

    // Set invocation to Always so callback fires even without data
    executor.set_invocation(id, InvocationMode::Always);

    // No data, but trigger Always → dispatch phase runs, callback fires
    let _result = executor.spin_once(core::time::Duration::from_millis(0));
    // Subscription take returns None, so subscriptions_processed stays 0
    // but the callback IS invoked (Always invocation) — try_process returns Ok(false)
    // because there's no actual data
    assert!(!called.load(portable_atomic::Ordering::SeqCst));
}

#[test]
fn test_trigger_one_fires_on_specific_handle() {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);

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
    let mut executor: Executor = executor_with_clock(session);

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
    let mut executor: Executor = executor_with_clock(session);

    let called = alloc::sync::Arc::new(portable_atomic::AtomicBool::new(false));
    let called2 = called.clone();

    let (_id, handle) = executor
        .register_guard_condition(move || {
            called2.store(true, portable_atomic::Ordering::SeqCst);
        })
        .unwrap();

    // Not triggered yet
    let _result = executor.spin_once(core::time::Duration::from_millis(0));
    assert!(!called.load(portable_atomic::Ordering::SeqCst));

    // Trigger the guard condition
    handle.trigger();

    let _result = executor.spin_once(core::time::Duration::from_millis(0));
    assert!(called.load(portable_atomic::Ordering::SeqCst));
}

#[test]
fn test_guard_condition_clears_after_trigger() {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);

    let count = alloc::sync::Arc::new(portable_atomic::AtomicUsize::new(0));
    let count2 = count.clone();

    let (_id, handle) = executor
        .register_guard_condition(move || {
            count2.fetch_add(1, portable_atomic::Ordering::SeqCst);
        })
        .unwrap();

    // Trigger once
    handle.trigger();
    executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(count.load(portable_atomic::Ordering::SeqCst), 1);

    // Without re-triggering, callback should not fire again
    executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(count.load(portable_atomic::Ordering::SeqCst), 1);

    // Trigger again
    handle.trigger();
    executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(count.load(portable_atomic::Ordering::SeqCst), 2);
}

/// phase-417 W4.e — the guard condition has ONE owner in all three languages,
/// and in Rust that owner is the node.
///
/// Until this item the only Rust spelling was `Executor::register_guard_condition`
/// — the executor — while C++ created one on the node and C needed three calls
/// on two objects. The ledger recorded that three-owner state as undecided
/// ("Nothing about no_std picks between these"); stage 4 is where our three
/// languages stop disagreeing.
///
/// The node-level verb is not a rename of the executor one: it binds the
/// node, so the guard takes that node's default `SchedContext` like every other
/// handle the node creates. That is the whole content of "the node owns it" for
/// a callback sharing one executor with every other tier in the image.
#[test]
fn a_node_created_guard_condition_fires_and_is_bound_to_its_node() {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);
    let nid = executor.node_builder("guard_owner").build().unwrap();

    let count = alloc::sync::Arc::new(portable_atomic::AtomicUsize::new(0));
    let count2 = count.clone();

    let (id, handle) = {
        let mut node = executor.node_mut(nid);
        node.create_guard_condition(move || {
            count2.fetch_add(1, portable_atomic::Ordering::SeqCst);
        })
        .expect("the node created a guard condition")
    };

    executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(
        count.load(portable_atomic::Ordering::SeqCst),
        0,
        "nothing fires without a trigger"
    );

    handle.trigger();
    executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(
        count.load(portable_atomic::Ordering::SeqCst),
        1,
        "the trigger reached the callback the CREATION call bound"
    );

    // The handle is a real arena slot, usable by every other executor verb that
    // takes one — this is what an unregistered `HandleId` could never be.
    let sc = executor
        .create_sched_context(crate::executor::sched_context::SchedContext {
            class: crate::executor::sched_context::SchedClass::Fifo,
            ..Default::default()
        })
        .unwrap();
    assert!(
        executor.bind_handle_to_sched_context(id, sc).is_ok(),
        "the returned HandleId names a registered slot"
    );
}

/// phase-417 W4.e — the polling half, which Rust and C++ did not have.
///
/// The ledger's `c:guard_condition_is_triggered` recorded the disagreement in
/// its own words: "C ships `is_triggered` + `clear`, C++ ships neither (only
/// `trigger` and `is_valid`), and Rust's `GuardCondition` ships only `trigger`.
/// A polling C++ or Rust user cannot do what a polling C user can."
///
/// The bound is stated and asserted: the executor CONSUMES the flag when it
/// dispatches, so the reader answers "set and not yet dispatched".
#[test]
fn a_guard_condition_flag_is_readable_and_clearable_without_dispatching() {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);

    let count = alloc::sync::Arc::new(portable_atomic::AtomicUsize::new(0));
    let count2 = count.clone();
    let (_id, handle) = executor
        .register_guard_condition(move || {
            count2.fetch_add(1, portable_atomic::Ordering::SeqCst);
        })
        .unwrap();

    assert!(!handle.is_triggered(), "a fresh guard condition is not set");

    handle.trigger();
    assert!(
        handle.is_triggered(),
        "the reader sees the flag BEFORE any spin -- that is what a polling tier needs"
    );

    assert!(handle.clear(), "clear reports what the flag WAS");
    assert!(!handle.is_triggered(), "and it cleared it");

    executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(
        count.load(portable_atomic::Ordering::SeqCst),
        0,
        "a flag the poller took must not also dispatch the callback"
    );

    // And the executor's own consumption is visible to the reader.
    handle.trigger();
    executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(count.load(portable_atomic::Ordering::SeqCst), 1);
    assert!(
        !handle.is_triggered(),
        "the dispatch consumed the flag, and the reader says so"
    );
}

// ====================================================================
// Phase 49: Raw subscription callback tests
// ====================================================================

// ====================================================================
// phase-456 W1: the arena holds the callback's CAPTURE
// ====================================================================

/// The capture survives the death of everything the caller owned.
///
/// This is the property the whole work item exists for. The pre-existing
/// registration passes `context` through untouched, so the caller must keep it
/// alive and unmoved for as long as the executor runs -- which is why a
/// callback-style entity was immovable after registration and why the C++
/// returning `create_subscription` had to heap-allocate a cell.
///
/// So the test drops the caller's buffer before spinning. If the arena were
/// still referencing it, this would read freed stack.
#[test]
fn test_arena_subscription_capture_outlives_the_caller() {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);

    static SEEN: portable_atomic::AtomicUsize = portable_atomic::AtomicUsize::new(0);

    // The "capture": a magic number the callback reads back OUT of its context.
    const MAGIC: usize = 0x5AFE_D00D;

    unsafe extern "C" fn capturing_cb(
        _data: *const u8,
        _len: usize,
        context: *mut core::ffi::c_void,
    ) {
        // The context is the arena's own copy of the capture, not the
        // caller's buffer.
        let value = unsafe { core::ptr::read_unaligned(context as *const usize) };
        SEEN.store(value, portable_atomic::Ordering::SeqCst);
    }

    SEEN.store(0, portable_atomic::Ordering::SeqCst);

    // A NON-NULL decoy context, so that a regression which forgets to repoint
    // `context` at the arena's copy reads THIS and fails the assertion with a
    // legible difference, rather than dereferencing null and dying on a signal.
    // Verified by mutation: with the repoint removed the test reports
    // `DECOY != MAGIC`.
    static DECOY: usize = 0xDEAD_BEEF;

    {
        // Deliberately scoped: this is gone before the first spin.
        let capture = MAGIC.to_ne_bytes();
        executor
            .add_arena_subscription_c_callback_with_capture::<{
                crate::config::DEFAULT_RX_BUF_SIZE
            }>(
                None,
                "/capture",
                "test/msg/TestMsg",
                "test_hash",
                QoSProfile::default().keep_last(1),
                capturing_cb,
                &DECOY as *const usize as *mut core::ffi::c_void,
                None,
                0,
                Some(&capture),
                None,
            )
            .unwrap();
    }

    let (data, len) = encode_test_msg(7);
    let meta = executor.entries[0].as_ref().unwrap();
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    unsafe {
        let sub_ptr = arena_ptr.add(meta.offset) as *const MockSubscriber;
        (*sub_ptr).load(data, len);
    }
    executor.spin_once(core::time::Duration::from_millis(0));

    assert_eq!(
        SEEN.load(portable_atomic::Ordering::SeqCst),
        MAGIC,
        "the dispatch context must be the ARENA's copy of the capture; reading \
         anything else means the caller's buffer was referenced after it died"
    );
}

// ====================================================================
// phase-456 W8 — the C registration path consults the in-place capability
// ====================================================================

/// A C/C++ subscription on an in-place backend claims NO receive region, and
/// still dispatches — phase-456 W8.
///
/// The measurement this item rests on: until W8 the executor asked
/// `supports_process_in_place()` in exactly ONE registration path, the Rust
/// typed one, so every C and C++ subscription on zenoh or XRCE allocated a
/// receive region the backend does not need. The C callback is the better
/// candidate for in-place dispatch, not the worse one —
/// `RawSubscriptionCallback` is `(const uint8_t*, size_t, void*)`, which is
/// already a borrowed-bytes signature.
///
/// Both halves are asserted, because either alone is satisfiable by a bug: an
/// entry that claims nothing and delivers nothing would pass the first, and the
/// old buffered path passes the second.
#[test]
fn a_c_subscription_on_an_in_place_backend_claims_no_receive_region() {
    static IN_PLACE_SEEN: portable_atomic::AtomicUsize = portable_atomic::AtomicUsize::new(0);
    static BUFFERED_SEEN: portable_atomic::AtomicUsize = portable_atomic::AtomicUsize::new(0);

    unsafe extern "C" fn in_place_cb(_d: *const u8, len: usize, _c: *mut core::ffi::c_void) {
        IN_PLACE_SEEN.store(len, portable_atomic::Ordering::SeqCst);
    }
    unsafe extern "C" fn buffered_cb(_d: *const u8, len: usize, _c: *mut core::ffi::c_void) {
        BUFFERED_SEEN.store(len, portable_atomic::Ordering::SeqCst);
    }

    fn register(
        exec: &mut Executor<'static>,
        cb: crate::executor::types::RawSubscriptionCallback,
    ) -> usize {
        let before = exec.arena_used();
        exec.add_arena_subscription_c_callback::<{ crate::config::DEFAULT_RX_BUF_SIZE }>(
            None,
            "/c-in-place",
            "test/msg/TestMsg",
            "test_hash",
            QoSProfile::default().keep_last(1),
            cb,
            core::ptr::null_mut(),
            None,
            0,
        )
        .unwrap();
        exec.arena_used() - before
    }

    let mut buffered: Executor = executor_with_clock(MockSession::new());
    let buffered_bytes = register(&mut buffered, buffered_cb);

    let mut in_place: Executor = executor_with_clock(MockSession::with_in_place_dispatch());
    let in_place_bytes = register(&mut in_place, in_place_cb);

    assert!(
        in_place_bytes < buffered_bytes,
        "an in-place C registration must claim LESS than a buffered one: \
         in-place {in_place_bytes} vs buffered {buffered_bytes}"
    );
    // The saving is the receive region, so it is at least the slot bound the
    // buffered arm reserves three of at depth 1. Asserted as a LOWER bound
    // rather than an exact figure: the arena aligns what it hands out, and the
    // entry structs differ in size too.
    assert!(
        buffered_bytes - in_place_bytes >= crate::config::DEFAULT_RX_BUF_SIZE,
        "the saving must be at least one receive slot ({} bytes); measured {}",
        crate::config::DEFAULT_RX_BUF_SIZE,
        buffered_bytes - in_place_bytes
    );

    // …and it still delivers. Both arms, so a delivery failure cannot read as
    // a saving.
    for (exec, seen) in [
        (&mut buffered, &BUFFERED_SEEN),
        (&mut in_place, &IN_PLACE_SEEN),
    ] {
        let (data, len) = encode_test_msg(7);
        let meta = exec.entries[0].as_ref().unwrap();
        let arena_ptr = exec.arena.as_ptr() as *const u8;
        unsafe {
            let sub_ptr = arena_ptr.add(meta.offset) as *const MockSubscriber;
            (*sub_ptr).load(data, len);
        }
        exec.spin_once(core::time::Duration::from_millis(0));
        assert_eq!(
            seen.load(portable_atomic::Ordering::SeqCst),
            len,
            "the callback must receive the sample's bytes on both dispatch arms"
        );
    }
}

/// A capture LARGER than W1's old fixed budget now registers and dispatches —
/// phase-456 W8.
///
/// This test used to assert the opposite: W1 held the capture in a
/// `[u8; CALLBACK_CAPTURE_BYTES]` inside the entry and REFUSED anything longer,
/// where that constant had to equal the C++ `NROS_CPP_CALLBACK_CAPACITY` macro
/// by construction. The refusal was therefore reachable only when the two
/// languages had drifted apart about a number — a failure mode nobody could
/// test against on purpose, and a bound the C++ side had to know.
///
/// W8 made the capture a runtime length: exactly `capture.len()` bytes of arena
/// (since phase-476 W0, inside the entry's trailing region) with `context`
/// pointing at them. The only bound left is
/// the arena, which every other entry already shares. So the assertion inverts,
/// and the number this test uses (`4 * size_of::<*const ()>() + 1`, W1's budget
/// plus one) is deliberately the one that used to fail.
#[test]
fn test_arena_subscription_capture_longer_than_the_old_budget_dispatches() {
    static SEEN: portable_atomic::AtomicUsize = portable_atomic::AtomicUsize::new(0);
    // Five pointers' worth: one more than W1's `4 * size_of::<*const ()>()`.
    const CAPTURE_WORDS: usize = 5;
    const MAGIC: usize = 0xC0FFEE;

    unsafe extern "C" fn capturing_cb(_data: *const u8, _len: usize, ctx: *mut core::ffi::c_void) {
        // The capture's LAST word is the one a truncating copy would lose.
        let words = ctx as *const usize;
        SEEN.store(
            unsafe { *words.add(CAPTURE_WORDS - 1) },
            portable_atomic::Ordering::SeqCst,
        );
    }

    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);

    {
        // Deliberately scoped: this is gone before the first spin.
        let capture: [usize; CAPTURE_WORDS] = [1, 2, 3, 4, MAGIC];
        let bytes = unsafe {
            core::slice::from_raw_parts(
                capture.as_ptr() as *const u8,
                core::mem::size_of_val(&capture),
            )
        };
        assert!(
            bytes.len() > 4 * core::mem::size_of::<*const ()>(),
            "this test is only meaningful for a capture past W1's old fixed budget"
        );
        executor
            .add_arena_subscription_c_callback_with_capture::<{
                crate::config::DEFAULT_RX_BUF_SIZE
            }>(
                None,
                "/big-capture",
                "test/msg/TestMsg",
                "test_hash",
                QoSProfile::default().keep_last(1),
                capturing_cb,
                core::ptr::null_mut(),
                None,
                0,
                Some(bytes),
                None,
            )
            .expect("a capture longer than the old constant must register, not be refused");
    }

    let (data, len) = encode_test_msg(7);
    let meta = executor.entries[0].as_ref().unwrap();
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    unsafe {
        let sub_ptr = arena_ptr.add(meta.offset) as *const MockSubscriber;
        (*sub_ptr).load(data, len);
    }
    executor.spin_once(core::time::Duration::from_millis(0));

    assert_eq!(
        SEEN.load(portable_atomic::Ordering::SeqCst),
        MAGIC,
        "the arena must hold the WHOLE capture; reading anything else means it \
         was truncated or the caller's buffer was referenced after it died"
    );
}

#[test]
fn test_raw_subscription_callback() {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);

    static RAW_CALLED: portable_atomic::AtomicBool = portable_atomic::AtomicBool::new(false);
    static RAW_LEN: portable_atomic::AtomicUsize = portable_atomic::AtomicUsize::new(0);

    unsafe extern "C" fn raw_cb(_data: *const u8, len: usize, _context: *mut core::ffi::c_void) {
        RAW_CALLED.store(true, portable_atomic::Ordering::SeqCst);
        RAW_LEN.store(len, portable_atomic::Ordering::SeqCst);
    }

    RAW_CALLED.store(false, portable_atomic::Ordering::SeqCst);

    let _id = executor
        .add_arena_subscription_c_callback::<{ crate::config::DEFAULT_RX_BUF_SIZE }>(
            None,
            "/test",
            "test/msg/TestMsg",
            "test_hash",
            QoSProfile::default().keep_last(1),
            raw_cb,
            core::ptr::null_mut(),
            None, // no group
            // phase-402 W2 — no hint in this test; 0 = no opinion.
            0,
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
    assert!(RAW_CALLED.load(portable_atomic::Ordering::SeqCst));
    assert_eq!(RAW_LEN.load(portable_atomic::Ordering::SeqCst), len);
}

#[test]
fn test_raw_subscription_info_callback() {
    // Phase 189.M3.4 — the C-fn-ptr-with-attachment subscription path.
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);

    static INFO_CALLED: portable_atomic::AtomicBool = portable_atomic::AtomicBool::new(false);
    static INFO_LEN: portable_atomic::AtomicUsize = portable_atomic::AtomicUsize::new(0);
    static INFO_ATT_LEN: portable_atomic::AtomicUsize = portable_atomic::AtomicUsize::new(0);

    unsafe extern "C" fn info_cb(
        _data: *const u8,
        len: usize,
        _att: *const u8,
        att_len: usize,
        _ctx: *mut core::ffi::c_void,
    ) {
        INFO_CALLED.store(true, portable_atomic::Ordering::SeqCst);
        INFO_LEN.store(len, portable_atomic::Ordering::SeqCst);
        INFO_ATT_LEN.store(att_len, portable_atomic::Ordering::SeqCst);
    }

    let _id = executor
        .add_arena_subscription_c_info_callback::<{ crate::config::DEFAULT_RX_BUF_SIZE }>(
            None,
            "/test",
            "test/msg/TestMsg",
            "test_hash",
            QoSProfile::default().keep_last(1),
            info_cb,
            core::ptr::null_mut(),
            0, // phase-408 W5a — no hint stated
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
    assert!(INFO_CALLED.load(portable_atomic::Ordering::SeqCst));
    assert_eq!(INFO_LEN.load(portable_atomic::Ordering::SeqCst), len);
    // MockSubscriber has no native attachment ⇒ default 0-length attachment.
    assert_eq!(INFO_ATT_LEN.load(portable_atomic::Ordering::SeqCst), 0);
}

// ====================================================================
// phase-417 W5.a — TYPED C subscription delivery (caller-owned storage)
//
// rclc hands the callback a DESERIALISED message on a path with no
// allocator, by making the caller own the storage. These two tests pin
// both halves of that contract: the success path writes into the
// caller's struct and dispatches, and the failure path does NOT — it
// drops the sample and reports it, rather than handing the callback
// storage that still holds the previous message.
// ====================================================================

/// Caller-owned storage, the shape a generated `<Msg>` struct has.
#[repr(C)]
struct TypedProbeMsg {
    value: i32,
    /// Written by the deserialiser so the test can prove the CDR bytes
    /// reached it, not just that something was called.
    decoded_len: usize,
}

/// The erased form of a generated `<Msg>_deserialize`: writes into caller
/// storage, returns 0 / non-zero.
unsafe extern "C" fn typed_probe_deserialize(
    buffer: *const u8,
    buffer_size: usize,
    msg: *mut core::ffi::c_void,
) -> i32 {
    let bytes = unsafe { core::slice::from_raw_parts(buffer, buffer_size) };
    let Ok(mut reader) = CdrReader::new_with_header(bytes) else {
        return -1;
    };
    let Ok(value) = reader.read_i32() else {
        return -1;
    };
    let out = unsafe { &mut *(msg as *mut TypedProbeMsg) };
    out.value = value;
    out.decoded_len = buffer_size;
    0
}

/// A deserialiser that always refuses — stands in for a sample that does not
/// fit the caller's storage (a string longer than its declared bound refuses
/// exactly this way; `rclc_cdr_read_string` errors on `str_len > max_len`
/// rather than truncating).
unsafe extern "C" fn typed_probe_deserialize_refuses(
    _buffer: *const u8,
    _buffer_size: usize,
    _msg: *mut core::ffi::c_void,
) -> i32 {
    -1
}

static TYPED_CB_CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static TYPED_CB_VALUE: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
static TYPED_CB_LEN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static TYPED_CB_CTX_OK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Orders the two tests that share the `TYPED_CB_*` statics above.
///
/// Same shape, and the same reason, as `SimTimeGuard`: the four statics are
/// PROCESS-global while `#[test]`s run concurrently, so
/// `typed_c_subscription_delivers_into_caller_storage` — whose callback DOES
/// fire — could land its `fetch_add` between the refused-decode test's
/// `store(0)` and its `load`. The refused-decode test then read 1 and reported
/// "a refused decode reached the callback", naming a delivery bug that had not
/// happened. Measured at 7 failures in 15 runs with `--test-threads=2`, and 0
/// in 15 solo or with `--test-threads=1`.
///
/// The counter is the reason a lock is the right tool rather than
/// per-test statics: both tests assert on the SAME question ("did the callback
/// fire, and how often"), so what they need is to not overlap, which is
/// exactly what the lock says.
struct TypedProbeGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl TypedProbeGuard {
    fn acquire() -> Self {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        // A poisoned lock means the sibling panicked while holding it. That is a
        // failure being reported elsewhere, not a reason to fail here too.
        Self {
            _lock: LOCK.lock().unwrap_or_else(|e| e.into_inner()),
        }
    }
}

/// The ported-rclc callback body: it reads FIELDS, and never CDR bytes.
unsafe extern "C" fn typed_probe_callback(
    msg: *const core::ffi::c_void,
    context: *mut core::ffi::c_void,
) {
    let m = unsafe { &*(msg as *const TypedProbeMsg) };
    TYPED_CB_CALLS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    TYPED_CB_VALUE.store(m.value, std::sync::atomic::Ordering::SeqCst);
    TYPED_CB_LEN.store(m.decoded_len, std::sync::atomic::Ordering::SeqCst);
    TYPED_CB_CTX_OK.store(!context.is_null(), std::sync::atomic::Ordering::SeqCst);
}

#[test]
fn typed_c_subscription_delivers_into_caller_storage() {
    let _typed_probe = TypedProbeGuard::acquire();
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);

    TYPED_CB_CALLS.store(0, std::sync::atomic::Ordering::SeqCst);
    TYPED_CB_VALUE.store(0, std::sync::atomic::Ordering::SeqCst);
    TYPED_CB_LEN.store(0, std::sync::atomic::Ordering::SeqCst);
    TYPED_CB_CTX_OK.store(false, std::sync::atomic::Ordering::SeqCst);

    // The caller owns this. Nothing in the executor allocates it, drops it, or
    // copies it — that is the whole mechanism.
    let mut storage = TypedProbeMsg {
        value: 0,
        decoded_len: 0,
    };
    let mut ctx: u32 = 0xC0FFEE;

    let _id = unsafe {
        executor.add_arena_subscription_c_typed_callback::<{ crate::config::DEFAULT_RX_BUF_SIZE }>(
            None,
            "/typed",
            "test/msg/TestMsg",
            "test_hash",
            QoSProfile::default().keep_last(1),
            core::ptr::NonNull::new(&mut storage as *mut TypedProbeMsg as *mut core::ffi::c_void)
                .unwrap(),
            typed_probe_deserialize,
            typed_probe_callback,
            &mut ctx as *mut u32 as *mut core::ffi::c_void,
            None,
            0,
        )
    }
    .unwrap();

    let (data, len) = encode_test_msg(4242);
    let meta = executor.entries[0].as_ref().unwrap();
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    unsafe {
        let sub_ptr = arena_ptr.add(meta.offset) as *const MockSubscriber;
        (*sub_ptr).load(data, len);
    }

    let result = executor.spin_once(core::time::Duration::from_millis(0));

    assert_eq!(result.subscriptions_processed, 1);
    assert_eq!(result.subscription_errors, 0);
    assert_eq!(
        TYPED_CB_CALLS.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "the typed callback must be dispatched exactly once per sample"
    );
    assert_eq!(
        TYPED_CB_VALUE.load(std::sync::atomic::Ordering::SeqCst),
        4242,
        "the callback must see the DESERIALISED field, not CDR bytes"
    );
    assert_eq!(
        TYPED_CB_LEN.load(std::sync::atomic::Ordering::SeqCst),
        len,
        "the deserialiser must have been handed the whole sample"
    );
    assert!(
        TYPED_CB_CTX_OK.load(std::sync::atomic::Ordering::SeqCst),
        "the caller's context must reach the callback"
    );
    // ...and the CALLER's storage is what was written, which is the claim that
    // distinguishes this path from one that deserialises into an arena slot.
    assert_eq!(storage.value, 4242);
    assert_eq!(storage.decoded_len, len);
}

#[test]
fn typed_c_subscription_refused_decode_is_loud_and_undelivered() {
    let _typed_probe = TypedProbeGuard::acquire();
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);

    TYPED_CB_CALLS.store(0, std::sync::atomic::Ordering::SeqCst);

    // Poisoned on purpose: if the executor dispatched on a refused decode, the
    // callback would read THIS and report it as the message that just arrived.
    let mut storage = TypedProbeMsg {
        value: -1,
        decoded_len: 0,
    };

    let failures_before = super::arena::typed_deserialize_failures();

    let _id = unsafe {
        executor.add_arena_subscription_c_typed_callback::<{ crate::config::DEFAULT_RX_BUF_SIZE }>(
            None,
            "/typed_bad",
            "test/msg/TestMsg",
            "test_hash",
            QoSProfile::default().keep_last(1),
            core::ptr::NonNull::new(&mut storage as *mut TypedProbeMsg as *mut core::ffi::c_void)
                .unwrap(),
            typed_probe_deserialize_refuses,
            typed_probe_callback,
            core::ptr::null_mut(),
            None,
            0,
        )
    }
    .unwrap();

    let (data, len) = encode_test_msg(7);
    let meta = executor.entries[0].as_ref().unwrap();
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    unsafe {
        let sub_ptr = arena_ptr.add(meta.offset) as *const MockSubscriber;
        (*sub_ptr).load(data, len);
    }

    let result = executor.spin_once(core::time::Duration::from_millis(0));

    assert_eq!(
        TYPED_CB_CALLS.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "a refused decode must NOT reach the callback — it would be handed the \
         previous message under the impression it was the new one"
    );
    assert_eq!(
        result.subscriptions_processed, 0,
        "a dropped sample is not processed work"
    );
    assert_eq!(
        result.subscription_errors, 1,
        "a refused decode must be OBSERVABLE — silence here is exactly what \
         RFC-0089 forbids"
    );
    assert!(
        result.any_errors(),
        "the spin result must report the failure"
    );
    // `>=`, not `==`. The counter is a PROCESS-GLOBAL atomic and cargo runs this
    // binary's tests on parallel threads, so between `failures_before` and here
    // any other test driving a typed subscription can bump it. Exact equality
    // asserts "no other test failed a decode", which is not this test's
    // business and is not something it can observe; the property under test is
    // that THIS refusal was counted. Exactness made it fail in `check::build`'s
    // parallel lane while passing solo.
    assert!(
        super::arena::typed_deserialize_failures() > failures_before,
        "the refusal must also be counted for the rate-limited nros_log report"
    );
    assert_eq!(
        storage.value, -1,
        "the caller's storage must be left as it was; a partial write the \
         callback never sees is still a write the NEXT dispatch could show"
    );
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
    let mut executor: Executor = executor_with_clock(session);

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
        executor.entries[id.slot()].as_ref().unwrap().invocation,
        InvocationMode::OnNewData
    );

    // Change to Always
    executor.set_invocation(id, InvocationMode::Always);
    assert_eq!(
        executor.entries[id.slot()].as_ref().unwrap().invocation,
        InvocationMode::Always
    );
}

// ====================================================================
// Phase 49: ExecutorSemantics tests
// ====================================================================

#[test]
fn test_set_semantics() {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);

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
    let mut executor: Executor = executor_with_clock(session);
    executor.set_semantics(ExecutorSemantics::LogicalExecutionTime);
    let nid = executor
        .node_builder("test_let_semantics_pre_samples_data")
        .build()
        .unwrap();

    let received = alloc::sync::Arc::new(std::sync::Mutex::new(None));
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
    let mut executor: Executor = executor_with_clock(session);
    executor.set_semantics(ExecutorSemantics::LogicalExecutionTime);

    static RAW_LET_LEN: portable_atomic::AtomicUsize = portable_atomic::AtomicUsize::new(0);

    unsafe extern "C" fn raw_let_cb(_data: *const u8, len: usize, _ctx: *mut core::ffi::c_void) {
        RAW_LET_LEN.store(len, portable_atomic::Ordering::SeqCst);
    }

    RAW_LET_LEN.store(0, portable_atomic::Ordering::SeqCst);

    executor
        .add_arena_subscription_c_callback::<{ crate::config::DEFAULT_RX_BUF_SIZE }>(
            None,
            "/test",
            "test/msg/TestMsg",
            "test_hash",
            QoSProfile::default().keep_last(1),
            raw_let_cb,
            core::ptr::null_mut(),
            None, // no group
            // phase-402 W2 — no hint in this test; 0 = no opinion.
            0,
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
    assert_eq!(RAW_LET_LEN.load(portable_atomic::Ordering::SeqCst), len);
}

// ====================================================================
// Phase 47: Trigger::All requires all non-timer handles
// ====================================================================

private_test_clock!(trigger_all_mixed_clock);

#[test]
fn test_trigger_all_with_mixed_handles() {
    trigger_all_mixed_clock::claim_at_us(1_000_000);
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock_fn(session, trigger_all_mixed_clock::now_us);

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

    let result = trigger_all_mixed_clock::elapse_then_spin_once(&mut executor, 100);
    assert_eq!(result.subscriptions_processed, 1);
    assert_eq!(result.timers_fired, 1);
}

// ====================================================================
// Phase 47: Trigger::AllOf sensor fusion pattern
// ====================================================================

#[test]
fn test_trigger_allof_fires_when_both_ready() {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);

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
    let mut executor: Executor = executor_with_clock(session);

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
    // Subscription still has no data, so callback won't fire (take returns None)
    assert_eq!(result.subscriptions_processed, 0);
}

// ====================================================================
// Phase 47: Trigger::AnyOf dispatches on any handle in set
// ====================================================================

#[test]
fn test_trigger_anyof_fires_when_one_ready() {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);

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
    let mut executor: Executor = executor_with_clock(session);

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

private_test_clock!(trigger_fails_delta_clock);

#[test]
// Needs time to advance INDEPENDENTLY of how often it is read: one waits
// out RATE_CHECK_INTERVAL_US (5 s), the other a timer period. The
// alloc-only clock is a per-read counter, so time only moves when someone
// looks, and neither window can close.
fn test_timer_delta_accumulates_when_trigger_fails() {
    trigger_fails_delta_clock::claim_at_us(1_000_000);
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock_fn(session, trigger_fails_delta_clock::now_us);

    let count = alloc::sync::Arc::new(portable_atomic::AtomicUsize::new(0));
    let count2 = count.clone();

    executor
        .register_timer(TimerDuration::from_millis(100), move || {
            count2.fetch_add(1, portable_atomic::Ordering::SeqCst);
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
    let _result = trigger_fails_delta_clock::elapse_then_spin_once(&mut executor, 50); // elapsed=50, not ready
    assert_eq!(count.load(portable_atomic::Ordering::SeqCst), 0);

    let _result = trigger_fails_delta_clock::elapse_then_spin_once(&mut executor, 60); // elapsed=110, fires!
    // Timer callback fired even though trigger didn't pass
    assert_eq!(count.load(portable_atomic::Ordering::SeqCst), 1);
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

// phase-380 W4 — was `#[cfg(rmw_needs_type_descriptors)]`: the schema is
// now required by `MessageForRmw` on EVERY backend, because that is where
// a subscription's build-time size bound comes from.
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

// phase-380 W4 — was `#[cfg(rmw_needs_type_descriptors)]`: the schema is
// now required by `MessageForRmw` on EVERY backend, because that is where
// a subscription's build-time size bound comes from.
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
    // Phase 193.2 — Node service builder + convenient create_service.
    let mut exec: Executor = executor_with_clock(MockSession::new());
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
        .qos(QoSProfile::default().reliable().keep_last(5))
        .build::<TestService, _>(|req: &TestServiceRequest| TestServiceReply { sum: req.a })
        .expect("service builder with qos builds");
}

#[test]
fn test_node_service_client_with_qos() {
    // Phase 193.2b — Node session-path create_service_with_qos /
    // create_client_with_qos (rclcpp-style qos overload).
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let mut node = executor.create_node("n").unwrap();
    let q = QoSProfile::default().reliable().keep_last(7);
    let _srv = node
        .create_polling_service_with_qos::<TestService>("/svc", q)
        .expect("service with qos");
    let _cli = node
        .create_client_with_qos::<TestService>("/svc", q)
        .expect("client with qos");
}

/// RFC-0041 / Phase 239.4 — a callback-based service client delivers the reply
/// to its typed closure at `spin_once` (no `Promise::take`). Drives the full
/// `call` → inject-reply → dispatch → callback path through the executor with the
/// mock transport.
#[test]
fn test_service_client_callback_fires_at_spin() {
    use crate::executor::arena::ServiceClientSendHeader;

    let mut executor: Executor = executor_with_clock(MockSession::new());
    let nid = executor.node_builder("svc_cb").build().unwrap();

    let got = alloc::sync::Arc::new(std::sync::Mutex::new(None));
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

/// Issue 2148 — `create_client_with_callback_with_info` hands each reply to its
/// closure WITH the `ServiceInfo` the backend reported for that reply (the
/// callback twin of `Promise::take_with_info`), and a reply that reported
/// nothing arrives as `NOT_REPORTED`, not as the previous reply's info.
#[test]
fn callback_client_with_info_reports_each_replys_info() {
    use crate::executor::arena::ServiceClientSendHeader;

    let mut executor: Executor = executor_with_clock(MockSession::new());
    let nid = executor.node_builder("svc_cb_info").build().unwrap();

    type Got = Option<(i32, nros_core::ServiceInfo)>;
    let got = alloc::sync::Arc::new(std::sync::Mutex::new(None::<(i32, nros_core::ServiceInfo)>));
    let got2 = got.clone();
    let mut client = executor
        .node_mut(nid)
        .create_client_with_callback_with_info::<TestService, _>(
            "/svc",
            move |reply: &TestServiceReply, info: &nros_core::ServiceInfo| {
                *got2.lock().unwrap() = Some((reply.sum, *info));
            },
        )
        .unwrap();

    let off = executor.entries[0].as_ref().unwrap().offset;
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    let hdr = unsafe {
        &*(arena_ptr.add(off)
            as *const ServiceClientSendHeader<{ crate::config::DEFAULT_RX_BUF_SIZE }>)
    };

    client.call(&TestServiceRequest { a: 7 }).unwrap();
    let seq = hdr.handle.last_sent_seq.get().unwrap();
    let info = nros_core::ServiceInfo {
        source_timestamp: Some(nros_core::Time::from_nanos(1_000)),
        received_timestamp: Some(nros_core::Time::from_nanos(2_000)),
        request_id: Some(nros_core::RequestId {
            writer_guid: [0x42; nros_core::REQUEST_WRITER_GUID_SIZE],
            sequence_number: seq,
        }),
    };
    let (reply_buf, reply_len) = encode_test_reply(14);
    hdr.handle.load_reply_info(info);
    hdr.handle.load_reply_for(seq, reply_buf, reply_len);
    assert!(
        executor
            .spin_once(core::time::Duration::from_millis(0))
            .any_work()
    );
    let first: Got = got.lock().unwrap().take();
    assert_eq!(first, Some((14, info)), "the reply and ITS info");

    // The next reply reported nothing: NOT_REPORTED, never the last one's.
    client.call(&TestServiceRequest { a: 8 }).unwrap();
    let seq = hdr.handle.last_sent_seq.get().unwrap();
    let (reply_buf, reply_len) = encode_test_reply(15);
    hdr.handle.load_reply_for(seq, reply_buf, reply_len);
    assert!(
        executor
            .spin_once(core::time::Duration::from_millis(0))
            .any_work()
    );
    let second: Got = got.lock().unwrap().take();
    assert_eq!(second, Some((15, nros_core::ServiceInfo::NOT_REPORTED)));
}

/// RFC-0041 / Phase 239.4 — a callback-based action client delivers
/// goal-response / feedback / result to typed closures at `spin_once`. Drives
/// each receive by injecting into the entry's mock channels and spinning.
#[test]
fn test_action_client_callbacks_fire_at_spin() {
    use crate::executor::action_core::ActionClientCore;

    /// CDR-with-header encode of a single i32 (mirrors a `{ i32 }` message).
    /// RFC-0069 / issue 0418 — the action payload is the FIELDS, with no inner
    /// encapsulation header. This used to be `CdrWriter::new_with_header`,
    /// encoding the double-header convention 0418 retired: the enclosing
    /// message carries the only header, and the consumer splices it on.
    ///
    /// These tests kept passing after the producer was fixed because the
    /// consumer still sniffed for an inner header and found one HERE. They were
    /// asserting the old wire format against a new producer.
    fn encode_i32_cdr(v: i32) -> ([u8; 256], usize) {
        let mut b = [0u8; 256];
        let mut w = CdrWriter::new(&mut b);
        w.write_i32(v).unwrap();
        let n = w.position();
        let _ = w; // release the &mut b borrow so b can be returned (avoids clippy::drop_non_drop)
        (b, n)
    }
    fn buf256(src: &[u8]) -> ([u8; 256], usize) {
        let mut b = [0u8; 256];
        b[..src.len()].copy_from_slice(src);
        (b, src.len())
    }

    let mut executor: Executor = executor_with_clock(MockSession::new());
    let nid = executor.node_builder("act_cb").build().unwrap();

    let goal_resp = alloc::sync::Arc::new(std::sync::Mutex::new(None));
    let feedback = alloc::sync::Arc::new(std::sync::Mutex::new(None));
    let result = alloc::sync::Arc::new(std::sync::Mutex::new(None));
    let (gr2, fb2, rs2) = (goal_resp.clone(), feedback.clone(), result.clone());

    let mut client = executor
        .node_mut(nid)
        .create_action_client_with_callbacks::<TestAction, _, _, _>(
            "/act",
            move |id: &nros_core::GoalUuid, accepted: bool| {
                let mut c = [0u8; 8];
                c.copy_from_slice(&id.uuid[..8]);
                *gr2.lock().unwrap() = Some((u64::from_le_bytes(c), accepted));
            },
            move |_id: &nros_core::GoalUuid, fb: &TestFeedback| {
                *fb2.lock().unwrap() = Some(fb.progress);
            },
            move |_id: &nros_core::GoalUuid, st: nros_core::GoalStatusCode, r: &TestResult| {
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

    // 2. Feedback: outer header(4) + GoalUuid(16) + inner CDR(header + i32).
    let mut fb = [0u8; 256];
    fb[0..4].copy_from_slice(&[0, 1, 0, 0]);
    fb[4..12].copy_from_slice(&1u64.to_le_bytes()); // GoalUuid uuid[..8] = counter 1
    let (inner, il) = encode_i32_cdr(7);
    fb[20..20 + il].copy_from_slice(&inner[..il]);
    core.feedback_subscriber.load(fb, 20 + il);
    executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(*feedback.lock().unwrap(), Some(7), "feedback deserialized");

    // 3. Result: header(4) + status@4 + pad(3) + inner CDR(header + i32) @8.
    client.get_result(&goal_id).unwrap();
    let mut rs = [0u8; 256];
    rs[0..4].copy_from_slice(&[0, 1, 0, 0]);
    rs[4] = 4; // GoalStatusCode::Succeeded
    let (inner_r, irl) = encode_i32_cdr(99);
    rs[8..8 + irl].copy_from_slice(&inner_r[..irl]);
    core.get_result_client.load_reply(rs, 8 + irl);
    executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(
        *result.lock().unwrap(),
        Some((nros_core::GoalStatusCode::Succeeded, 99)),
        "result deserialized"
    );
}

/// RFC-0041 / Phase 239.7 — two feedbacks arriving before a spin are **both**
/// delivered via the QoS-depth ring (vs the pre-239 single-buffer overwrite).
#[test]
fn test_action_client_feedback_burst_buffered() {
    use crate::executor::action_core::ActionClientCore;

    /// RFC-0069 / issue 0418 — the action payload is the FIELDS, with no inner
    /// encapsulation header. This used to be `CdrWriter::new_with_header`,
    /// encoding the double-header convention 0418 retired: the enclosing
    /// message carries the only header, and the consumer splices it on.
    ///
    /// These tests kept passing after the producer was fixed because the
    /// consumer still sniffed for an inner header and found one HERE. They were
    /// asserting the old wire format against a new producer.
    fn encode_i32_cdr(v: i32) -> ([u8; 256], usize) {
        let mut b = [0u8; 256];
        let mut w = CdrWriter::new(&mut b);
        w.write_i32(v).unwrap();
        let n = w.position();
        let _ = w; // release the &mut b borrow so b can be returned (avoids clippy::drop_non_drop)
        (b, n)
    }

    let mut executor: Executor = executor_with_clock(MockSession::new());
    let nid = executor.node_builder("act_burst").build().unwrap();

    let feedbacks = alloc::sync::Arc::new(std::sync::Mutex::new(alloc::vec::Vec::<i32>::new()));
    let fb2 = feedbacks.clone();
    let _client = executor
        .node_mut(nid)
        .create_action_client_with_callbacks::<TestAction, _, _, _>(
            "/act",
            |_: &nros_core::GoalUuid, _: bool| {},
            move |_: &nros_core::GoalUuid, fb: &TestFeedback| {
                fb2.lock().unwrap().push(fb.progress);
            },
            |_: &nros_core::GoalUuid, _: nros_core::GoalStatusCode, _: &TestResult| {},
        )
        .unwrap();

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

    // Inject TWO feedbacks into the subscriber queue BEFORE spinning.
    for p in [7i32, 8i32] {
        let mut fb = [0u8; 256];
        fb[0..4].copy_from_slice(&[0, 1, 0, 0]);
        fb[4..12].copy_from_slice(&1u64.to_le_bytes());
        let (inner, il) = encode_i32_cdr(p);
        fb[20..20 + il].copy_from_slice(&inner[..il]);
        core.feedback_subscriber.load(fb, 20 + il);
    }

    // One spin drains BOTH into the ring and dispatches both — no overwrite.
    executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(
        *feedbacks.lock().unwrap(),
        alloc::vec![7, 8],
        "both burst feedbacks delivered via the QoS ring"
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
    let mut executor: Executor = executor_with_clock(session);

    let firing_order = alloc::sync::Arc::new(std::sync::Mutex::new(alloc::vec::Vec::<i32>::new()));
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
    assert_eq!(*order, alloc::vec![20, 10]);
}

/// Issue 1942 — the `(request, ServiceInfo)` callback gets the metadata the
/// backend reported for THAT request, and the reply still reaches the
/// requester. rclrs's `ServiceInfo`.
#[test]
fn service_with_info_callback_receives_the_requests_metadata() {
    use nros_core::{RequestId, ServiceInfo, Time};
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);
    let seen = alloc::sync::Arc::new(std::sync::Mutex::new(alloc::vec::Vec::new()));
    let seen_cb = seen.clone();

    let nid = executor.node_builder("svc_info").build().unwrap();
    executor
        .node_mut(nid)
        .create_service_with_info::<TestService, _>(
            "/svc_info",
            move |req: &TestServiceRequest, info: &ServiceInfo| {
                seen_cb.lock().unwrap().push((req.a, *info));
                TestServiceReply { sum: req.a + 1 }
            },
        )
        .unwrap();

    let reported = ServiceInfo {
        source_timestamp: Some(Time::new(12, 345)),
        received_timestamp: None,
        request_id: Some(RequestId {
            writer_guid: [0xA5; nros_core::REQUEST_WRITER_GUID_SIZE],
            sequence_number: 41,
        }),
    };
    let (data, len) = encode_test_msg(7);
    let off = executor.entries[0].as_ref().unwrap().offset;
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    let server = unsafe { &*(arena_ptr.add(off) as *const MockServiceServer) };
    server.load_with_info(data, len, reported);

    let _ = executor.spin_once(core::time::Duration::from_millis(0));

    assert_eq!(*seen.lock().unwrap(), alloc::vec![(7, reported)]);
    assert_eq!(server.sent.borrow().len(), 1, "the reply was sent");
}

/// Issue 1942 — a backend that reports no per-request metadata hands the
/// callback `NOT_REPORTED`: every field `None`, never a zeroed id or a
/// timestamp of 0 standing in for "unknown". And metadata does not leak from
/// one request into the next.
#[test]
fn service_with_info_reports_nothing_a_backend_did_not_say() {
    use nros_core::{RequestId, ServiceInfo};
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);
    let seen = alloc::sync::Arc::new(std::sync::Mutex::new(alloc::vec::Vec::new()));
    let seen_cb = seen.clone();

    let nid = executor.node_builder("svc_info_none").build().unwrap();
    executor
        .node_mut(nid)
        .service("/svc_info_none")
        .service_info()
        .build::<TestService, _>(move |req: &TestServiceRequest, info: &ServiceInfo| {
            seen_cb.lock().unwrap().push((req.a, *info));
            TestServiceReply { sum: req.a }
        })
        .unwrap();

    let off = executor.entries[0].as_ref().unwrap().offset;
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    let server = unsafe { &*(arena_ptr.add(off) as *const MockServiceServer) };

    let reported = ServiceInfo {
        request_id: Some(RequestId {
            writer_guid: [1; nros_core::REQUEST_WRITER_GUID_SIZE],
            sequence_number: 5,
        }),
        ..ServiceInfo::NOT_REPORTED
    };
    let (d1, n1) = encode_test_msg(1);
    server.load_with_info(d1, n1, reported);
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    let (d2, n2) = encode_test_msg(2);
    server.load(d2, n2);
    let _ = executor.spin_once(core::time::Duration::from_millis(0));

    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0], (1, reported));
    assert_eq!(seen[1], (2, ServiceInfo::NOT_REPORTED));
    assert!(!seen[1].1.is_reported());
}

// ====================================================================
// Promise tests
// ====================================================================

#[test]
fn test_promise_try_recv_returns_none_then_some() {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);

    let mut node = executor.create_node("test").unwrap();
    let mut client = node.create_client::<TestService>("/test_svc").unwrap();

    // Start a non-blocking call
    let request = TestServiceRequest { a: 42 };
    let mut promise = client.call(&request).unwrap();

    // No reply loaded yet — should return None
    assert!(promise.take().unwrap().is_none());

    // Load a CDR-encoded reply into the mock
    let mut reply_buf = [0u8; 256];
    let mut writer = CdrWriter::new_with_header(&mut reply_buf).unwrap();
    writer.write_i32(99).unwrap();
    let reply_len = writer.position();

    // Access the mock client through the promise handle
    promise.handle.load_reply(reply_buf, reply_len);

    // Now take should return the reply
    let reply = promise.take().unwrap().unwrap();
    assert_eq!(reply.sum, 99);
}

/// Issue 2136 — `Promise::take_with_info` hands back the reply's info, the
/// one the backend reported for THAT reply; `take` keeps its old shape.
#[test]
fn promise_take_with_info_reports_the_replys_info() {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);
    let mut node = executor.create_node("test").unwrap();
    let mut client = node.create_client::<TestService>("/test_svc").unwrap();

    let mut promise = client.call(&TestServiceRequest { a: 1 }).unwrap();
    assert!(promise.take_with_info().unwrap().is_none());

    let seq = promise.handle.last_sent_seq.get().unwrap();
    let info = nros_core::ServiceInfo {
        source_timestamp: Some(nros_core::Time::from_nanos(1_000)),
        received_timestamp: Some(nros_core::Time::from_nanos(2_000)),
        request_id: Some(nros_core::RequestId {
            writer_guid: [0x21; nros_core::REQUEST_WRITER_GUID_SIZE],
            sequence_number: seq,
        }),
    };
    let (reply_buf, reply_len) = encode_test_reply(7);
    promise.handle.load_reply_info(info);
    promise.handle.load_reply_for(seq, reply_buf, reply_len);
    let (reply, got) = promise.take_with_info().unwrap().unwrap();
    assert_eq!(reply.sum, 7);
    assert_eq!(got, info);

    // The next call's reply reported nothing: NOT_REPORTED, not the last one's.
    let mut promise = client.call(&TestServiceRequest { a: 2 }).unwrap();
    let seq = promise.handle.last_sent_seq.get().unwrap();
    let (reply_buf, reply_len) = encode_test_reply(8);
    promise.handle.load_reply_for(seq, reply_buf, reply_len);
    let (_, got) = promise.take_with_info().unwrap().unwrap();
    assert_eq!(got, nros_core::ServiceInfo::NOT_REPORTED);
}

/// A CDR `{ i32 }` reply in a mock-sized buffer (issue 2009 tests).
fn encode_test_reply(sum: i32) -> ([u8; 256], usize) {
    let mut reply_buf = [0u8; 256];
    let mut writer = CdrWriter::new_with_header(&mut reply_buf).unwrap();
    writer.write_i32(sum).unwrap();
    let len = writer.position();
    let _ = writer;
    (reply_buf, len)
}

/// Issue 2009 — a reply to a call abandoned with `reset_in_flight` is DROPPED.
/// It used to be delivered to the next call's promise as its answer.
#[test]
fn promise_drops_the_late_reply_of_an_abandoned_call() {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);
    let mut node = executor.create_node("test").unwrap();
    let mut client = node.create_client::<TestService>("/test_svc").unwrap();

    let first_seq;
    {
        let promise = client.call(&TestServiceRequest { a: 1 }).unwrap();
        first_seq = promise.handle.last_sent_seq.get().unwrap();
    }
    // The first promise was dropped without its reply: abandon the call.
    client.reset_in_flight();
    let mut promise = client.call(&TestServiceRequest { a: 2 }).unwrap();
    let second_seq = promise.handle.last_sent_seq.get().unwrap();
    assert_ne!(first_seq, second_seq);

    // The abandoned call's reply arrives late.
    let (late, late_len) = encode_test_reply(111);
    promise.handle.load_reply_for(first_seq, late, late_len);
    assert!(
        promise.take().unwrap().is_none(),
        "the abandoned call's reply must not answer the new call"
    );

    let (own, own_len) = encode_test_reply(222);
    promise.handle.load_reply_for(second_seq, own, own_len);
    assert_eq!(promise.take().unwrap().unwrap().sum, 222);
}

/// Issue 2009 — the callback client dispatches only the reply naming the call
/// in flight; a reply naming another request is dropped, not handed to the
/// callback.
#[test]
fn callback_client_drops_a_reply_for_another_request() {
    use crate::executor::arena::ServiceClientSendHeader;

    let mut executor: Executor = executor_with_clock(MockSession::new());
    let nid = executor.node_builder("svc_cb_seq").build().unwrap();
    let got = alloc::sync::Arc::new(std::sync::Mutex::new(alloc::vec::Vec::new()));
    let got2 = got.clone();
    let mut client = executor
        .node_mut(nid)
        .create_client_with_callback::<TestService, _>("/svc", move |reply: &TestServiceReply| {
            got2.lock().unwrap().push(reply.sum);
        })
        .unwrap();
    client.call(&TestServiceRequest { a: 7 }).unwrap();

    let off = executor.entries[0].as_ref().unwrap().offset;
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    let hdr = unsafe {
        &*(arena_ptr.add(off)
            as *const ServiceClientSendHeader<{ crate::config::DEFAULT_RX_BUF_SIZE }>)
    };
    let sent = hdr.handle.last_sent_seq.get().unwrap();

    let (stray, stray_len) = encode_test_reply(-1);
    hdr.handle.load_reply_for(sent + 100, stray, stray_len);
    executor.spin_once(core::time::Duration::from_millis(0));
    assert!(
        got.lock().unwrap().is_empty(),
        "a stray reply reached the callback"
    );

    let (own, own_len) = encode_test_reply(14);
    hdr.handle.load_reply_for(sent, own, own_len);
    executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(*got.lock().unwrap(), alloc::vec![14]);
}

/// Issue 2009 — an action client with two goals in flight attributes each
/// acceptance to the goal of the request it answers. It used to report every
/// acceptance against the most recently SENT goal (`goal_counter`), so goal 1's
/// acceptance read as goal 2's. And a sub-client full of pending requests
/// refuses the next send rather than sending one it could not match.
#[test]
fn action_client_attributes_replies_by_request_id() {
    use crate::executor::action_core::ActionClientCore;

    let mut executor: Executor = executor_with_clock(MockSession::new());
    let nid = executor.node_builder("act_seq").build().unwrap();
    let accepted = alloc::sync::Arc::new(std::sync::Mutex::new(alloc::vec::Vec::new()));
    let acc2 = accepted.clone();
    let mut client = executor
        .node_mut(nid)
        .create_action_client_with_callbacks::<TestAction, _, _, _>(
            "/act",
            move |id: &nros_core::GoalUuid, ok: bool| {
                let mut c = [0u8; 8];
                c.copy_from_slice(&id.uuid[..8]);
                acc2.lock().unwrap().push((u64::from_le_bytes(c), ok));
            },
            |_id: &nros_core::GoalUuid, _fb: &TestFeedback| {},
            |_id: &nros_core::GoalUuid, _st: nros_core::GoalStatusCode, _r: &TestResult| {},
        )
        .unwrap();
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

    let g1 = client.send_goal(&TestGoal { order: 1 }).unwrap();
    let seq1 = core.send_goal_client.last_sent_seq.get().unwrap();
    let g2 = client.send_goal(&TestGoal { order: 2 }).unwrap();
    assert_ne!(g1.uuid, g2.uuid);

    // Goal 1's acceptance arrives AFTER goal 2 was sent.
    let mut b = [0u8; 256];
    b[..5].copy_from_slice(&[0, 1, 0, 0, 1]);
    core.send_goal_client.load_reply_for(seq1, b, 5);
    executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(
        *accepted.lock().unwrap(),
        alloc::vec![(1u64, true)],
        "goal 1's acceptance is attributed to goal 1, not the latest goal"
    );

    // Two more fill the send_goal sub-client (MAX_CONCURRENT_GOALS = 4 with
    // goal 2 still pending); the next is refused before it is sent.
    let _g3 = client.send_goal(&TestGoal { order: 3 }).unwrap();
    let _g4 = client.send_goal(&TestGoal { order: 4 }).unwrap();
    let _g5 = client.send_goal(&TestGoal { order: 5 }).unwrap();
    let sent_before = core.send_goal_client.next_seq.get();
    assert!(
        matches!(
            client.send_goal(&TestGoal { order: 6 }),
            Err(NodeError::RequestInFlight)
        ),
        "a full pending table refuses the send"
    );
    assert_eq!(
        core.send_goal_client.next_seq.get(),
        sent_before,
        "a refused goal never reached the transport"
    );

    // Issue 2027 — the binding whose last owner of goal 3's request let go
    // (a dropped C++ future) forgets it: the slot is free again, and goal 3's
    // late acceptance is dropped rather than attributed to anyone.
    let seq3 = sent_before - 3;
    let core_mut = unsafe {
        &mut *(executor.arena.as_mut_ptr().cast::<u8>().add(off)
            as *mut ActionClientCore<
                { crate::config::DEFAULT_RX_BUF_SIZE },
                { crate::config::DEFAULT_RX_BUF_SIZE },
                { crate::config::DEFAULT_RX_BUF_SIZE },
            >)
    };
    assert!(
        core_mut.forget_send_goal(&_g3),
        "goal 3's request was pending"
    );
    assert!(
        !core_mut.forget_send_goal(&_g3),
        "...and is forgotten exactly once"
    );
    let _g6 = client
        .send_goal(&TestGoal { order: 6 })
        .expect("the freed slot takes the next goal");
    core_mut.send_goal_client.load_reply_for(seq3, b, 5);
    accepted.lock().unwrap().clear();
    executor.spin_once(core::time::Duration::from_millis(0));
    assert!(
        accepted.lock().unwrap().is_empty(),
        "a forgotten goal's late acceptance reaches no callback"
    );
}

private_test_clock!(no_timeout_credit_clock);

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
// The fix: measure the clock's delta and carry the sub-ms remainder across
// calls. This test asserts a 50 ms timer does NOT fire after a single 1 s
// `spin_once` against a no-op session.
//
// Issue 1944 — on a PRIVATE clock that does not move, so the right answer is
// exactly zero fires. On the wall clock the bound was derived from a measured
// elapsed that began at `start`, while the executor's delta began at its
// construction: a host that descheduled the test between the two for over
// 100 ms fired the timer and failed it, with nothing wrong in the executor.
#[test]
fn test_spin_once_does_not_credit_timeout_to_timer_delta() {
    no_timeout_credit_clock::claim_at_us(1_000_000);
    use core::{
        sync::atomic::{AtomicU32, Ordering},
        time::Duration,
    };
    static FIRES: AtomicU32 = AtomicU32::new(0);
    FIRES.store(0, Ordering::SeqCst);

    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock_fn(session, no_timeout_credit_clock::now_us);

    // 50 ms periodic timer.
    let _timer = executor
        .register_timer(TimerDuration::from_millis(50), || {
            FIRES.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap();

    // Ask for a 1 s spin. MockSession::drive_io returns instantly and the
    // clock does not move, so no time elapsed. With the bug the timer would
    // fire ~20 times (1000 ms / 50 ms). Without the bug, 0 fires.
    executor.spin_once(Duration::from_millis(1000));
    assert_eq!(
        FIRES.load(Ordering::SeqCst),
        0,
        "timer fired with no clock time elapsed. The pre-fix bug credited the \
         requested timeout (1000 ms) to the timer delta."
    );

    // The positive control: the SAME timer, once the clock really moves one
    // period, fires once — so the zero above is not a timer that never runs.
    let _ = no_timeout_credit_clock::elapse_then_spin_once(&mut executor, 50);
    assert_eq!(
        FIRES.load(Ordering::SeqCst),
        1,
        "one period elapsed, one fire"
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
    let mut executor: Executor = executor_with_clock(MockSession::new());
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
    use nros_core::{GoalStatusCode, GoalUuid};

    let mut core: ActionServerCore = ActionServerCore::from_channels(
        MockServiceServer::new(),
        MockServiceServer::new(),
        MockServiceServer::new(),
        MockPublisher::new(),
        MockPublisher::new(),
    );

    let g1 = GoalUuid { uuid: [1u8; 16] };
    let g2 = GoalUuid { uuid: [2u8; 16] };
    let _ = core.active_goals.push(RawActiveGoal {
        goal_id: g1,
        status: GoalStatusCode::Executing,
        stamp: nros_core::Time::default(),
    });
    let _ = core.active_goals.push(RawActiveGoal {
        goal_id: g2,
        status: GoalStatusCode::Executing,
        stamp: nros_core::Time::default(),
    });

    // A get_result request is [CDR-LE header][fixed uint8[16] goal_id].
    let mk_req = |g: &GoalUuid| -> ([u8; 256], usize) {
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
    core.complete_goal_raw(&g2, GoalStatusCode::Succeeded, &res2)
        .unwrap();
    {
        let sent = core.get_result_server.sent.borrow();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, 1, "g2 reply must use g2's correlation token");
        assert_eq!(sent[0].1[4], GoalStatusCode::Succeeded as u8 as i8 as u8);
        assert_eq!(&sent[0].1[RESULT_OFF..RESULT_OFF + res2.len()], &res2);
    }
    assert_eq!(core.pending_get_results.len(), 1);

    // Complete g1 → flush g1's held request with g1's token (seq 0).
    let res1 = [0xAAu8; 8];
    core.complete_goal_raw(&g1, GoalStatusCode::Succeeded, &res1)
        .unwrap();
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
    use nros_core::{GoalStatusCode, GoalUuid};

    let mut core: ActionServerCore = ActionServerCore::from_channels(
        MockServiceServer::new(),
        MockServiceServer::new(),
        MockServiceServer::new(),
        MockPublisher::new(),
        MockPublisher::new(),
    );

    let g = GoalUuid { uuid: [7u8; 16] };
    // Goal completes before any get_result arrives.
    let res = [0xCCu8; 8];
    core.complete_goal_raw(&g, GoalStatusCode::Succeeded, &res)
        .unwrap();
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

// ============================================================================
// Issue 0796 — the completed-result slab must RECLAIM
// ============================================================================

/// A `get_result` request wire frame: `[CDR-LE header][fixed uint8[16] goal_id]`.
#[cfg(test)]
fn mk_get_result_req(goal_id: &nros_core::GoalUuid) -> ([u8; 256], usize) {
    let mut b = [0u8; 256];
    b[..4].copy_from_slice(&[0x00, 0x01, 0x00, 0x00]);
    b[4..20].copy_from_slice(&goal_id.uuid);
    (b, 20)
}

/// Reply layout is `[4-byte CDR header][i8 status][3 pad][result CDR]`, so the
/// status byte is at 4 and the result bytes start at 8.
#[cfg(test)]
const REPLY_STATUS_OFF: usize = 4;
#[cfg(test)]
const REPLY_RESULT_OFF: usize = 8;

/// Fetch `goal_id`'s result and return `(status_byte, result_bytes)` from the
/// single reply the mock recorded. Clears the mock's reply log, which holds
/// only 8 entries and DROPS the overflow — a loop that trusted `sent.len()`
/// would stop seeing replies rather than fail.
#[cfg(test)]
fn fetch_result<const G: usize, const R: usize, const F: usize, const M: usize>(
    core: &mut super::action_core::ActionServerCore<G, R, F, M>,
    goal_id: &nros_core::GoalUuid,
    default_result: &[u8],
    want_len: usize,
) -> (u8, heapless::Vec<u8, 256>) {
    let (req, len) = mk_get_result_req(goal_id);
    core.get_result_server.load(req, len);
    core.try_handle_get_result_raw(default_result).unwrap();
    let (status, bytes) = {
        let sent = core.get_result_server.sent.borrow();
        assert_eq!(sent.len(), 1, "expected exactly one reply for this fetch");
        let mut out: heapless::Vec<u8, 256> = heapless::Vec::new();
        out.extend_from_slice(&sent[0].1[REPLY_RESULT_OFF..REPLY_RESULT_OFF + want_len])
            .unwrap();
        (sent[0].1[REPLY_STATUS_OFF], out)
    };
    core.get_result_server.sent.borrow_mut().clear();
    (status, bytes)
}

/// Issue 0796, problem 1 — completing goals whose results TOTAL more than
/// `RESULT_BUF` must keep delivering.
///
/// The pre-fix slab was a bump allocator with no free: `result_slab_used` only
/// ever moved forward, so once the accumulated results crossed `RESULT_BUF`
/// every later completion was dropped on the floor (`stored = false`, no entry
/// pushed, `complete_goal_raw` returning `()`), and every subsequent
/// `get_result` was answered `UNKNOWN` with the default payload. Here
/// `RESULT_BUF` is 128 and each result is 40 bytes, so the third completion
/// fills it and the fourth is where the old code starts lying.
#[test]
fn action_results_keep_being_delivered_past_the_slab_capacity() {
    use super::action_core::ActionServerCore;
    use crate::mock::MockPublisher;
    use nros_core::{GoalStatusCode, GoalUuid};

    // GOAL_BUF 256, RESULT_BUF 128, FEEDBACK_BUF 128, MAX_GOALS 4.
    let mut core: ActionServerCore<256, 128, 128, 4> = ActionServerCore::from_channels(
        MockServiceServer::new(),
        MockServiceServer::new(),
        MockServiceServer::new(),
        MockPublisher::new(),
        MockPublisher::new(),
    );

    const RESULT_LEN: usize = 40;
    // 10 goals x 40 bytes = 400 bytes through a 128-byte slab.
    const GOALS: u8 = 10;
    let default_result = [0u8; 4];

    for i in 1..=GOALS {
        let g = GoalUuid { uuid: [i; 16] };
        let res = [0xA0u8.wrapping_add(i); RESULT_LEN];

        core.complete_goal_raw(&g, GoalStatusCode::Succeeded, &res)
            .unwrap_or_else(|e| panic!("goal {i} could not be stored: {e:?}"));

        let (status, bytes) = fetch_result(&mut core, &g, &default_result, RESULT_LEN);
        assert_eq!(
            status,
            GoalStatusCode::Succeeded as u8 as i8 as u8,
            "goal {i} was answered with a non-terminal status - the slab dropped it"
        );
        assert_eq!(
            &bytes[..],
            &res[..],
            "goal {i} was answered with the wrong result bytes"
        );

        // The reclamation is real, not just "it still answers": live bytes
        // never exceed the slab, and the entry table never exceeds MAX_GOALS.
        assert!(core.result_slab_used() <= 128);
        assert!(core.completed_result_count() <= 4);
    }
}

/// Same overflow, through the DEFERRED path: the client sends `get_result`
/// while the goal is still active (what `rclcpp_action` does), so the reply is
/// flushed by `complete_goal_raw`.
///
/// Pre-fix this was the worse half of the bug — the flush was inside
/// `if stored`, so a client that had already asked was never answered at all
/// and waited on its result future forever.
#[test]
fn deferred_get_result_is_answered_past_the_slab_capacity() {
    use super::action_core::{ActionServerCore, RawActiveGoal};
    use crate::mock::MockPublisher;
    use nros_core::{GoalStatusCode, GoalUuid};

    let mut core: ActionServerCore<256, 128, 128, 4> = ActionServerCore::from_channels(
        MockServiceServer::new(),
        MockServiceServer::new(),
        MockServiceServer::new(),
        MockPublisher::new(),
        MockPublisher::new(),
    );

    const RESULT_LEN: usize = 40;
    let default_result = [0u8; 4];

    for i in 1..=10u8 {
        let g = GoalUuid { uuid: [i; 16] };
        let res = [0x50u8.wrapping_add(i); RESULT_LEN];

        core.active_goals
            .push(RawActiveGoal {
                goal_id: g,
                status: GoalStatusCode::Executing,
                stamp: nros_core::Time::default(),
            })
            .ok()
            .unwrap();

        // Client asks while the goal is still running -> deferred.
        let (req, len) = mk_get_result_req(&g);
        core.get_result_server.load(req, len);
        core.try_handle_get_result_raw(&default_result).unwrap();
        assert_eq!(core.pending_get_results.len(), 1, "goal {i} not deferred");
        assert_eq!(core.get_result_server.sent.borrow().len(), 0);

        core.complete_goal_raw(&g, GoalStatusCode::Succeeded, &res)
            .unwrap_or_else(|e| panic!("goal {i} could not be stored: {e:?}"));

        assert_eq!(
            core.pending_get_results.len(),
            0,
            "goal {i}: the held get_result was never flushed"
        );
        {
            let sent = core.get_result_server.sent.borrow();
            assert_eq!(sent.len(), 1, "goal {i}: no reply reached the waiter");
            assert_eq!(
                &sent[0].1[REPLY_RESULT_OFF..REPLY_RESULT_OFF + RESULT_LEN],
                &res[..],
                "goal {i}: waiter got the wrong result bytes"
            );
        }
        core.get_result_server.sent.borrow_mut().clear();
    }
}

/// A completed result survives until the slab is actually under pressure, and
/// when something must go the FETCHED result goes first.
///
/// Issue 0796's eviction contract: a delivered result does not pin storage, and
/// a client that never asks cannot wedge the server.
#[test]
fn completed_result_survives_until_pressure_then_evicts_the_fetched_one_first() {
    use super::action_core::ActionServerCore;
    use crate::mock::MockPublisher;
    use nros_core::{GoalStatusCode, GoalUuid};

    // 128-byte slab: exactly three 40-byte results fit.
    let mut core: ActionServerCore<256, 128, 128, 4> = ActionServerCore::from_channels(
        MockServiceServer::new(),
        MockServiceServer::new(),
        MockServiceServer::new(),
        MockPublisher::new(),
        MockPublisher::new(),
    );

    const RESULT_LEN: usize = 40;
    let default_result = [0u8; 4];
    let g = |i: u8| GoalUuid { uuid: [i; 16] };
    let payload = |i: u8| [i; RESULT_LEN];

    for i in 1..=3u8 {
        core.complete_goal_raw(&g(i), GoalStatusCode::Succeeded, &payload(i))
            .unwrap();
    }
    assert_eq!(core.completed_result_count(), 3);
    assert_eq!(core.result_slab_used(), 120);

    // Nothing has been evicted yet: a client that asks AFTER completion still
    // gets its result, including the oldest one.
    let (status, bytes) = fetch_result(&mut core, &g(2), &default_result, RESULT_LEN);
    assert_eq!(status, GoalStatusCode::Succeeded as u8 as i8 as u8);
    assert_eq!(&bytes[..], &payload(2)[..]);

    // A fourth completion needs 40 bytes and only 8 are free -> reclaim. g2 was
    // fetched, so g2 is what goes, NOT the older-but-unfetched g1.
    core.complete_goal_raw(&g(4), GoalStatusCode::Succeeded, &payload(4))
        .unwrap();

    assert!(core.has_completed_result(&g(1)), "unfetched g1 was evicted");
    assert!(core.has_completed_result(&g(3)), "unfetched g3 was evicted");
    assert!(core.has_completed_result(&g(4)));
    assert!(
        !core.has_completed_result(&g(2)),
        "the already-delivered g2 should have been reclaimed first"
    );

    // Compaction rewrote the survivors' offsets correctly - each still reads back
    // as its own payload.
    for i in [1u8, 3, 4] {
        let (status, bytes) = fetch_result(&mut core, &g(i), &default_result, RESULT_LEN);
        assert_eq!(status, GoalStatusCode::Succeeded as u8 as i8 as u8, "g{i}");
        assert_eq!(&bytes[..], &payload(i)[..], "g{i} read back corrupted");
    }

    // The evicted goal is answered honestly (UNKNOWN + default), never left
    // hanging.
    let (status, _) = fetch_result(&mut core, &g(2), &default_result, default_result.len());
    assert_eq!(status, GoalStatusCode::Unknown as u8 as i8 as u8);
}

/// A result larger than `RESULT_BUF` can never be retained — that is the one
/// remaining failure, and `complete_goal_raw` now REPORTS it (issue 0796: it
/// returned `()`).  The waiting requester is still answered, from the caller's
/// own bytes.
#[test]
fn an_unretainable_result_is_reported_and_the_waiter_is_still_answered() {
    use super::{
        action_core::{ActionServerCore, RawActiveGoal},
        types::NodeError,
    };
    use crate::mock::MockPublisher;
    use nros_core::{GoalStatusCode, GoalUuid};

    let mut core: ActionServerCore<256, 128, 128, 4> = ActionServerCore::from_channels(
        MockServiceServer::new(),
        MockServiceServer::new(),
        MockServiceServer::new(),
        MockPublisher::new(),
        MockPublisher::new(),
    );

    let g = GoalUuid { uuid: [9u8; 16] };
    let res = [0x77u8; 200]; // > RESULT_BUF (128)
    core.active_goals
        .push(RawActiveGoal {
            goal_id: g,
            status: GoalStatusCode::Executing,
            stamp: nros_core::Time::default(),
        })
        .ok()
        .unwrap();

    let (req, len) = mk_get_result_req(&g);
    core.get_result_server.load(req, len);
    core.try_handle_get_result_raw(&[0u8; 4]).unwrap();
    assert_eq!(core.pending_get_results.len(), 1);

    let err = core
        .complete_goal_raw(&g, GoalStatusCode::Succeeded, &res)
        .expect_err("a result larger than RESULT_BUF must not report success");
    assert!(matches!(err, NodeError::BufferTooSmall));

    // ... and the client that was already waiting is not stranded.
    assert_eq!(core.pending_get_results.len(), 0);
    let sent = core.get_result_server.sent.borrow();
    assert_eq!(sent.len(), 1);
    assert_eq!(
        &sent[0].1[REPLY_RESULT_OFF..REPLY_RESULT_OFF + res.len()],
        &res[..]
    );
}

/// `expire_completed_results` is the eager analogue of
/// `rcl_action_expire_goals()`: it reclaims exactly the delivered results and
/// leaves the rest.
#[test]
fn expire_completed_results_reclaims_only_delivered_results() {
    use super::action_core::ActionServerCore;
    use crate::mock::MockPublisher;
    use nros_core::{GoalStatusCode, GoalUuid};

    let mut core: ActionServerCore<256, 128, 128, 4> = ActionServerCore::from_channels(
        MockServiceServer::new(),
        MockServiceServer::new(),
        MockServiceServer::new(),
        MockPublisher::new(),
        MockPublisher::new(),
    );

    const RESULT_LEN: usize = 40;
    let g = |i: u8| GoalUuid { uuid: [i; 16] };
    let payload = |i: u8| [i; RESULT_LEN];
    for i in 1..=3u8 {
        core.complete_goal_raw(&g(i), GoalStatusCode::Succeeded, &payload(i))
            .unwrap();
    }

    // Nothing fetched yet -> nothing to expire.
    assert_eq!(core.expire_completed_results(), 0);
    assert_eq!(core.result_slab_used(), 120);

    let _ = fetch_result(&mut core, &g(1), &[0u8; 4], RESULT_LEN);
    let _ = fetch_result(&mut core, &g(3), &[0u8; 4], RESULT_LEN);

    assert_eq!(core.expire_completed_results(), 2);
    assert_eq!(core.completed_result_count(), 1);
    assert_eq!(core.result_slab_used(), RESULT_LEN);
    assert!(core.has_completed_result(&g(2)));

    // The survivor was compacted down to offset 0 and still reads back intact.
    let (_, bytes) = fetch_result(&mut core, &g(2), &[0u8; 4], RESULT_LEN);
    assert_eq!(&bytes[..], &payload(2)[..]);
}

/// `goal_exists` answers rcl's question, which is WIDER than a status lookup —
/// phase-417 W4.b, ledger row `c:action_server_goal_exists`.
///
/// The row sat open on the claim that `rclc_action_get_goal_status` already
/// answered it ("ours answers the stronger question"), so it might be a
/// documentation fix. It is not, and this is the measurement that says so. A
/// status lookup reads the ACTIVE set; rcl's `rcl_action_server_goal_exists`
/// searches the goal-handle table, which retains a goal after it terminates
/// until `rcl_action_expire_goals` reclaims it. The three states below are
/// where the two answers differ, and the middle one is exactly the window in
/// which a client is expected to call `get_result`:
///
/// | state                              | status lookup | `goal_exists` |
/// | ---------------------------------- | ------------- | ------------- |
/// | active                             | a status      | true          |
/// | completed, result still retained   | NOT FOUND     | **true**      |
/// | completed, result fetched + expired| NOT FOUND     | false         |
#[test]
fn goal_exists_covers_active_and_retained_results_not_just_active() {
    use super::action_core::ActionServerCore;
    use crate::mock::MockPublisher;
    use nros_core::{GoalStatusCode, GoalUuid};

    let mut core: ActionServerCore<256, 128, 128, 4> = ActionServerCore::from_channels(
        MockServiceServer::new(),
        MockServiceServer::new(),
        MockServiceServer::new(),
        MockPublisher::new(),
        MockPublisher::new(),
    );

    const RESULT_LEN: usize = 40;
    let live = GoalUuid { uuid: [1u8; 16] };
    let done = GoalUuid { uuid: [2u8; 16] };
    let never = GoalUuid { uuid: [9u8; 16] };

    // A goal nobody ever sent is unknown in every state below.
    assert!(!core.goal_exists(&never));

    // ACTIVE — in the active set, and `goal_exists` says so.
    core.accept_goal(live, 1).unwrap();
    assert!(core.goal_exists(&live));
    assert!(!core.goal_exists(&done));

    // COMPLETED, RESULT RETAINED — this is the row. `complete_goal_raw` removes
    // the goal from the active set, so an active-set lookup reports NOT FOUND
    // while the result is still sitting there to be fetched.
    core.accept_goal(done, 2).unwrap();
    core.complete_goal_raw(&done, GoalStatusCode::Succeeded, &[7u8; RESULT_LEN])
        .unwrap();
    assert!(
        !core
            .active_goals
            .iter()
            .any(|g| g.goal_id.uuid == done.uuid),
        "a completed goal must have left the active set — otherwise this test \
         proves nothing about the two answers differing"
    );
    assert!(core.has_completed_result(&done));
    assert!(
        core.goal_exists(&done),
        "a completed goal whose result is still retained EXISTS, which is what \
         rcl reports and what an active-set lookup cannot"
    );

    // COMPLETED, FETCHED, EXPIRED — now it is gone from both.
    let _ = fetch_result(&mut core, &done, &[0u8; 4], RESULT_LEN);
    assert_eq!(core.expire_completed_results(), 1);
    assert!(!core.has_completed_result(&done));
    assert!(!core.goal_exists(&done));

    // The still-active goal was never a candidate for expiry.
    assert!(core.goal_exists(&live));
}

// ============================================================================
// Issue 1361 — the published status array carries TERMINAL statuses
//
// These assert on the BYTES the status publisher was handed, not on the core's
// internal tables. That distinction is the issue: the tables were always right
// and the published array was empty, so no table assertion could have caught it.
// ============================================================================

/// Decode a published `GoalStatusArray` into `(goal_id, status)` pairs.
fn parse_status_array(
    bytes: &[u8],
) -> heapless::Vec<(nros_core::GoalUuid, nros_core::GoalStatusCode), 32> {
    use nros_core::{CdrReader, Deserialize, GoalStatusStamped};

    let mut reader = CdrReader::new_with_header(bytes).expect("status array has a CDR header");
    let count = reader.read_u32().expect("status array has a length prefix") as usize;
    let mut out = heapless::Vec::new();
    for _ in 0..count {
        let stamped = GoalStatusStamped::deserialize(&mut reader).expect("entry decodes");
        out.push((stamped.goal_info.goal_id, stamped.status))
            .expect("test array fits");
    }
    assert!(
        reader.position() + 3 >= bytes.len(),
        "decoded {} entries but left {} of {} bytes unread — the array carries \
         more than it declared",
        count,
        bytes.len() - reader.position(),
        bytes.len()
    );
    out
}

/// The last sample published for a goal that ran to completion must NAME that
/// goal with its terminal status.
///
/// **Negative control.** Against the pre-fix `publish_status_array` (which
/// serialised `active_goals` alone, after `complete_goal_raw` had already
/// `swap_remove`d the goal) this fails: the published array is empty. That
/// empty array is what a live `rmw_zenoh_cpp` subscriber measured on
/// 2026-09-13 and what the transient-local publisher retained for a late
/// joiner (issues 1361, 1341).
#[test]
fn completed_goal_appears_in_the_published_status_array() {
    use super::action_core::{ActionServerCore, RawActiveGoal};
    use crate::mock::MockPublisher;
    use nros_core::{GoalStatusCode, GoalUuid};

    let mut core: ActionServerCore<256, 128, 128, 4> = ActionServerCore::from_channels(
        MockServiceServer::new(),
        MockServiceServer::new(),
        MockServiceServer::new(),
        MockPublisher::new(),
        MockPublisher::new(),
    );

    let g = GoalUuid { uuid: [9u8; 16] };
    let _ = core.active_goals.push(RawActiveGoal {
        goal_id: g,
        status: GoalStatusCode::Executing,
        stamp: nros_core::Time::default(),
    });
    core.publish_status_array().unwrap();

    // While active: one entry, EXECUTING.
    let (bytes, len) = core.status_publisher.last_published().unwrap();
    let entries = parse_status_array(&bytes[..len]);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0], (g, GoalStatusCode::Executing));

    core.complete_goal_raw(&g, GoalStatusCode::Succeeded, &[0xAAu8; 8])
        .unwrap();

    // The goal has left `active_goals` — that part never was the bug.
    assert_eq!(core.active_goal_count(), 0);

    // What the subscriber receives must still name it, SUCCEEDED.
    let (bytes, len) = core.status_publisher.last_published().unwrap();
    let entries = parse_status_array(&bytes[..len]);
    assert_eq!(
        entries.len(),
        1,
        "the terminal publish must carry the goal, not an empty array"
    );
    assert_eq!(entries[0], (g, GoalStatusCode::Succeeded));
}

/// A terminated goal is visible alongside goals that are still running, and an
/// ABORTED goal is reported as aborted — the array is a status array, not an
/// "active goals" list.
#[test]
fn status_array_mixes_active_and_terminated_goals() {
    use super::action_core::{ActionServerCore, RawActiveGoal};
    use crate::mock::MockPublisher;
    use nros_core::{GoalStatusCode, GoalUuid};

    let mut core: ActionServerCore<256, 128, 128, 4> = ActionServerCore::from_channels(
        MockServiceServer::new(),
        MockServiceServer::new(),
        MockServiceServer::new(),
        MockPublisher::new(),
        MockPublisher::new(),
    );

    let running = GoalUuid { uuid: [1u8; 16] };
    let aborted = GoalUuid { uuid: [2u8; 16] };
    for id in [running, aborted] {
        let _ = core.active_goals.push(RawActiveGoal {
            goal_id: id,
            status: GoalStatusCode::Executing,
            stamp: nros_core::Time::default(),
        });
    }
    core.complete_goal_raw(&aborted, GoalStatusCode::Aborted, &[0u8; 4])
        .unwrap();

    let (bytes, len) = core.status_publisher.last_published().unwrap();
    let entries = parse_status_array(&bytes[..len]);
    assert_eq!(entries.len(), 2);
    assert!(entries.contains(&(running, GoalStatusCode::Executing)));
    assert!(entries.contains(&(aborted, GoalStatusCode::Aborted)));

    // The entry count is NOT the active-goal count any more; `active_goal_count`
    // still is.
    assert_eq!(core.active_goal_count(), 1);
}

/// The promise `publish_status_array` documents: a terminated goal is visible
/// for exactly as long as its RESULT is retained. Not 15 minutes — nano-ros has
/// no clock — so this test is the written form of that divergence from
/// `rcl_action_expire_goals`.
#[test]
fn terminal_status_is_visible_until_its_result_is_reclaimed() {
    use super::action_core::ActionServerCore;
    use crate::mock::MockPublisher;
    use nros_core::{GoalStatusCode, GoalUuid};

    let mut core: ActionServerCore<256, 128, 128, 4> = ActionServerCore::from_channels(
        MockServiceServer::new(),
        MockServiceServer::new(),
        MockServiceServer::new(),
        MockPublisher::new(),
        MockPublisher::new(),
    );

    const RESULT_LEN: usize = 8;
    let g = GoalUuid { uuid: [5u8; 16] };
    core.complete_goal_raw(&g, GoalStatusCode::Succeeded, &[0u8; RESULT_LEN])
        .unwrap();

    let (bytes, len) = core.status_publisher.last_published().unwrap();
    assert_eq!(parse_status_array(&bytes[..len]).len(), 1);

    // The client fetches the result. Reclamation does NOT republish — the last
    // retained sample still names the terminal status, which is what a
    // transient-local late joiner receives.
    let _ = fetch_result(&mut core, &g, &[0u8; 4], RESULT_LEN);
    let (bytes, len) = core.status_publisher.last_published().unwrap();
    assert_eq!(
        parse_status_array(&bytes[..len])[0],
        (g, GoalStatusCode::Succeeded)
    );

    // Once the result is reclaimed the goal leaves the array on the NEXT
    // publish. That is the end of the visibility window.
    assert_eq!(core.expire_completed_results(), 1);
    core.publish_status_array().unwrap();
    let (bytes, len) = core.status_publisher.last_published().unwrap();
    assert!(parse_status_array(&bytes[..len]).is_empty());
}

/// The 512-byte scratch buffer must hold the array's WORST case, which issue
/// 1361 doubled to `2 * MAX_GOALS` entries. Measures the real serialized size
/// rather than trusting the "roughly 32 bytes an entry" estimate the issue was
/// written from, and pins the layout constants the compile-time bound
/// (`ActionServerCore::STATUS_ARRAY_FITS`) is computed from.
#[test]
fn status_array_entry_layout_is_measured() {
    use super::action_core::{
        ActionServerCore, RawActiveGoal, STATUS_ARRAY_BUF, STATUS_ARRAY_PREAMBLE,
        STATUS_ENTRY_STRIDE, STATUS_ENTRY_TAIL, status_array_bytes,
    };
    use crate::mock::MockPublisher;
    use nros_core::{GoalStatusCode, GoalUuid};

    let mut core: ActionServerCore<256, 128, 128, 4> = ActionServerCore::from_channels(
        MockServiceServer::new(),
        MockServiceServer::new(),
        MockServiceServer::new(),
        MockPublisher::new(),
        MockPublisher::new(),
    );

    fn published_len(core: &ActionServerCore<256, 128, 128, 4>) -> usize {
        core.publish_status_array().unwrap();
        core.status_publisher.last_published().unwrap().1
    }

    // 0, 1 and 2 entries pin preamble, tail and stride against the real writer.
    let empty = published_len(&core);
    let _ = core.active_goals.push(RawActiveGoal {
        goal_id: GoalUuid { uuid: [1u8; 16] },
        status: GoalStatusCode::Executing,
        stamp: nros_core::Time::default(),
    });
    let one = published_len(&core);
    let _ = core.active_goals.push(RawActiveGoal {
        goal_id: GoalUuid { uuid: [2u8; 16] },
        status: GoalStatusCode::Executing,
        stamp: nros_core::Time::default(),
    });
    let two = published_len(&core);

    assert_eq!(empty, STATUS_ARRAY_PREAMBLE, "preamble");
    assert_eq!(one - empty, STATUS_ENTRY_TAIL, "tail entry size");
    assert_eq!(two - one, STATUS_ENTRY_STRIDE, "entry stride");
    assert_eq!(one, status_array_bytes(1));
    assert_eq!(two, status_array_bytes(2));

    // The measured worst case at the DEFAULT MAX_GOALS = 4: four active goals
    // plus four retained completed results.
    const MAX_GOALS: usize = 4;
    for i in 3..=MAX_GOALS as u8 {
        let _ = core.active_goals.push(RawActiveGoal {
            goal_id: GoalUuid { uuid: [i; 16] },
            status: GoalStatusCode::Executing,
            stamp: nros_core::Time::default(),
        });
    }
    for i in 0..MAX_GOALS as u8 {
        core.complete_goal_raw(
            &GoalUuid {
                uuid: [100 + i; 16],
            },
            GoalStatusCode::Succeeded,
            &[0u8; 4],
        )
        .unwrap();
    }
    assert_eq!(core.active_goal_count(), MAX_GOALS);
    assert_eq!(core.completed_result_count(), MAX_GOALS);

    let worst = published_len(&core);
    assert_eq!(worst, status_array_bytes(2 * MAX_GOALS));
    assert!(
        worst <= STATUS_ARRAY_BUF,
        "2 * MAX_GOALS entries are {worst} bytes, over the {STATUS_ARRAY_BUF}-byte buffer"
    );
    let (bytes, len) = core.status_publisher.last_published().unwrap();
    assert_eq!(parse_status_array(&bytes[..len]).len(), 2 * MAX_GOALS);
}

/// The bound holds at the LARGEST `MAX_GOALS` the compile-time guard admits.
/// One higher does not compile — that is what
/// `ActionServerCore::STATUS_ARRAY_FITS` is for, and it is why this number is 9
/// rather than something rounder.
#[test]
fn status_array_fits_at_the_largest_admitted_max_goals() {
    use super::action_core::{
        ActionServerCore, RawActiveGoal, STATUS_ARRAY_BUF, status_array_bytes,
    };
    use crate::mock::MockPublisher;
    use nros_core::{GoalStatusCode, GoalUuid};

    const MAX_GOALS: usize = 9;
    let mut core: ActionServerCore<256, 128, 128, MAX_GOALS> = ActionServerCore::from_channels(
        MockServiceServer::new(),
        MockServiceServer::new(),
        MockServiceServer::new(),
        MockPublisher::new(),
        MockPublisher::new(),
    );

    for i in 0..MAX_GOALS as u8 {
        let _ = core.active_goals.push(RawActiveGoal {
            goal_id: GoalUuid { uuid: [i; 16] },
            status: GoalStatusCode::Executing,
            stamp: nros_core::Time::default(),
        });
        core.complete_goal_raw(
            &GoalUuid {
                uuid: [100 + i; 16],
            },
            GoalStatusCode::Succeeded,
            &[0u8; 4],
        )
        .unwrap();
    }
    assert_eq!(core.active_goal_count(), MAX_GOALS);
    assert_eq!(core.completed_result_count(), MAX_GOALS);

    let (bytes, len) = core.status_publisher.last_published().unwrap();
    assert_eq!(len, status_array_bytes(2 * MAX_GOALS));
    assert!(
        len <= STATUS_ARRAY_BUF,
        "{len} bytes over {STATUS_ARRAY_BUF}"
    );
    assert_eq!(parse_status_array(&bytes[..len]).len(), 2 * MAX_GOALS);
}

// ============================================================================
// Phase 272 (RFC-0047) — node_name → sched-context table + node_builder lookup
// ============================================================================

/// Seeded name resolves to the bound SC, and a callback registered under
/// that node inherits SC 2 via `apply_node_default_sched`.
#[test]
fn test_bind_node_name_sched_seeded_resolves() {
    use crate::executor::sched_context::{SchedContext, SchedContextId};

    let session = MockSession::new();
    let mut executor = executor_with_clock(session);

    // SC slots 1 and 2 must exist for apply_node_default_sched to wire the
    // binding. create_sched_context fills slots in order starting at 1.
    let _sc1 = executor
        .create_sched_context(SchedContext::default())
        .unwrap();
    let sc2 = executor
        .create_sched_context(SchedContext::default())
        .unwrap();
    assert_eq!(sc2, SchedContextId(2));

    // Seed the table before building.
    executor
        .bind_node_name_sched("talker", "/", SchedContextId(2))
        .expect("fixture binding must fit");

    let nid = executor.node_builder("talker").build().unwrap();

    // 1. The NodeRecord itself carries the seeded SC.
    assert_eq!(executor.nodes[nid.index()].default_sched, SchedContextId(2));

    // 2. A callback registered under the node inherits SC 2 via
    //    apply_node_default_sched (called inside create_subscription).
    executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/t", |_: &TestMsg| {})
        .unwrap();
    assert_eq!(executor.sched_context_bindings[0], SchedContextId(2));
}

/// An unseeded name always produces SchedContextId(0) (default path unchanged).
#[test]
fn test_bind_node_name_sched_unseeded_defaults_to_zero() {
    use crate::executor::sched_context::SchedContextId;

    let session = MockSession::new();
    let mut executor = executor_with_clock(session);

    // No bind call — table is empty.
    let nid = executor.node_builder("listener").build().unwrap();
    assert_eq!(executor.nodes[nid.index()].default_sched, SchedContextId(0));
}

/// An explicit `.sched(id)` on the builder wins over a conflicting table entry.
#[test]
fn test_bind_node_name_sched_explicit_beats_table() {
    use crate::executor::sched_context::{SchedContext, SchedContextId};

    let session = MockSession::new();
    let mut executor = executor_with_clock(session);

    executor
        .create_sched_context(SchedContext::default())
        .unwrap(); // SchedContextId(1)
    executor
        .create_sched_context(SchedContext::default())
        .unwrap(); // SchedContextId(2)

    // Table says SC 2, but explicit .sched(1) should win.
    executor
        .bind_node_name_sched("talker", "/", SchedContextId(2))
        .expect("fixture binding must fit");

    let nid = executor
        .node_builder("talker")
        .sched(SchedContextId(1))
        .build()
        .unwrap();

    assert_eq!(
        executor.nodes[nid.index()].default_sched,
        SchedContextId(1),
        "explicit .sched() must override the table entry"
    );
}

/// A namespace-qualified key disambiguates nodes with the same name in
/// different namespaces — seeding "/ns"/"talker" does not affect "/"/"talker".
#[test]
fn test_bind_node_name_sched_namespace_disambiguates() {
    use crate::executor::sched_context::{SchedContext, SchedContextId};

    let session = MockSession::new();
    let mut executor = executor_with_clock(session);

    executor
        .create_sched_context(SchedContext::default())
        .unwrap(); // SchedContextId(1)

    // Seed for the namespaced node only.
    executor
        .bind_node_name_sched("talker", "/ns", SchedContextId(1))
        .expect("fixture binding must fit");

    // Node in "/ns" namespace → gets SC 1.
    let nid_ns = executor
        .node_builder("talker")
        .namespace("/ns")
        .build()
        .unwrap();
    assert_eq!(
        executor.nodes[nid_ns.index()].default_sched,
        SchedContextId(1)
    );

    // Node in root "/" namespace (executor default) → not seeded → SC 0.
    let nid_root = executor.node_builder("talker").build().unwrap();
    assert_eq!(
        executor.nodes[nid_root.index()].default_sched,
        SchedContextId(0),
        "root-ns node must not inherit the /ns-keyed table entry"
    );
}

// =========================================================================
// Phase 273 (RFC-0047) — group_sched_table tests
// =========================================================================

/// Seeding a group binding causes a callback created under that group to
/// bind to the group's SC, not the node default (SC 0).
#[test]
fn test_bind_group_sched_seeded_resolves() {
    use crate::executor::sched_context::{SchedContext, SchedContextId};

    let session = MockSession::new();
    let mut executor = executor_with_clock(session);

    // Create SC slots: 1 and 2 must exist so the validity checks pass.
    let _sc1 = executor
        .create_sched_context(SchedContext::default())
        .unwrap(); // SchedContextId(1)
    let sc2 = executor
        .create_sched_context(SchedContext::default())
        .unwrap(); // SchedContextId(2)
    assert_eq!(sc2, SchedContextId(2));

    // Seed the group table: node "node" / group "ctrl" → SC 2.
    executor
        .bind_group_sched("node", "/", "ctrl", SchedContextId(2))
        .expect("fixture binding must fit");

    // Build the node (no default_sched seeded — node stays at SC 0).
    let nid = executor.node_builder("node").build().unwrap();
    assert_eq!(
        executor.nodes[nid.index()].default_sched,
        SchedContextId(0),
        "node default must remain 0 — group table is the override"
    );

    // Manually call apply_node_default_sched with group = Some("ctrl").
    // slot 0 is unused here; use it directly.
    executor.apply_node_default_sched(0, Some(nid), Some("ctrl"));
    assert_eq!(
        executor.sched_context_bindings[0],
        SchedContextId(2),
        "group table entry must win over node default (SC 0)"
    );
}

/// Two groups on the SAME node each resolve to their own SC (sub-node split).
#[test]
fn test_bind_group_sched_sub_node_split() {
    use crate::executor::sched_context::{SchedContext, SchedContextId};

    let session = MockSession::new();
    let mut executor = executor_with_clock(session);

    let _sc1 = executor
        .create_sched_context(SchedContext::default())
        .unwrap(); // SchedContextId(1)
    let sc2 = executor
        .create_sched_context(SchedContext::default())
        .unwrap(); // SchedContextId(2)
    let sc3 = executor
        .create_sched_context(SchedContext::default())
        .unwrap(); // SchedContextId(3)
    assert_eq!(sc2, SchedContextId(2));
    assert_eq!(sc3, SchedContextId(3));

    // One node, two groups, two SCs — the capability the node-name table can't express.
    executor
        .bind_group_sched("node", "/", "ctrl", SchedContextId(2))
        .expect("fixture binding must fit");
    executor
        .bind_group_sched("node", "/", "telem", SchedContextId(3))
        .expect("fixture binding must fit");

    let nid = executor.node_builder("node").build().unwrap();

    // ctrl group callback → SC 2.
    executor.apply_node_default_sched(0, Some(nid), Some("ctrl"));
    assert_eq!(executor.sched_context_bindings[0], SchedContextId(2));

    // telem group callback → SC 3.
    executor.apply_node_default_sched(1, Some(nid), Some("telem"));
    assert_eq!(executor.sched_context_bindings[1], SchedContextId(3));
}

/// No group passed (None) → falls through to node default (phase-272 behaviour).
#[test]
fn test_bind_group_sched_no_group_uses_node_default() {
    use crate::executor::sched_context::{SchedContext, SchedContextId};

    let session = MockSession::new();
    let mut executor = executor_with_clock(session);

    let _sc1 = executor
        .create_sched_context(SchedContext::default())
        .unwrap(); // SchedContextId(1)
    let sc2 = executor
        .create_sched_context(SchedContext::default())
        .unwrap(); // SchedContextId(2)
    assert_eq!(sc2, SchedContextId(2));

    // Seed both the node-name table and the group table.
    executor
        .bind_node_name_sched("node", "/", SchedContextId(2))
        .expect("fixture binding must fit");
    executor
        .bind_group_sched("node", "/", "ctrl", SchedContextId(1))
        .expect("fixture binding must fit");

    let nid = executor.node_builder("node").build().unwrap();
    assert_eq!(executor.nodes[nid.index()].default_sched, SchedContextId(2));

    // group = None → node default (SC 2), not the group entry.
    executor.apply_node_default_sched(0, Some(nid), None);
    assert_eq!(executor.sched_context_bindings[0], SchedContextId(2));
}

/// An unmapped group name falls back to the node default.
#[test]
fn test_bind_group_sched_unmapped_group_falls_back_to_node_default() {
    use crate::executor::sched_context::{SchedContext, SchedContextId};

    let session = MockSession::new();
    let mut executor = executor_with_clock(session);

    let _sc1 = executor
        .create_sched_context(SchedContext::default())
        .unwrap(); // SchedContextId(1)
    let sc2 = executor
        .create_sched_context(SchedContext::default())
        .unwrap(); // SchedContextId(2)
    assert_eq!(sc2, SchedContextId(2));

    // Node default = SC 2; no group entry for "unknown_group".
    executor
        .bind_node_name_sched("node", "/", SchedContextId(2))
        .expect("fixture binding must fit");

    let nid = executor.node_builder("node").build().unwrap();
    assert_eq!(executor.nodes[nid.index()].default_sched, SchedContextId(2));

    // Passing an unmapped group → falls back to node default (SC 2).
    executor.apply_node_default_sched(0, Some(nid), Some("unknown_group"));
    assert_eq!(
        executor.sched_context_bindings[0],
        SchedContextId(2),
        "unmapped group must fall back to node default"
    );
}

/// Group table entry wins over node default when both are seeded (precedence).
#[test]
fn test_bind_group_sched_group_beats_node_default() {
    use crate::executor::sched_context::{SchedContext, SchedContextId};

    let session = MockSession::new();
    let mut executor = executor_with_clock(session);

    // SC 1, 2, 5 (skipping 3 and 4 to test non-contiguous ids).
    let _sc1 = executor
        .create_sched_context(SchedContext::default())
        .unwrap(); // SchedContextId(1)
    let _sc2 = executor
        .create_sched_context(SchedContext::default())
        .unwrap(); // SchedContextId(2)
    let _sc3 = executor
        .create_sched_context(SchedContext::default())
        .unwrap(); // SchedContextId(3)
    let _sc4 = executor
        .create_sched_context(SchedContext::default())
        .unwrap(); // SchedContextId(4)
    let sc5 = executor
        .create_sched_context(SchedContext::default())
        .unwrap(); // SchedContextId(5)
    let sc2 = SchedContextId(2);
    assert_eq!(sc5, SchedContextId(5));

    // node default = SC 5; "ctrl" group → SC 2.
    executor
        .bind_node_name_sched("node", "/", SchedContextId(5))
        .expect("fixture binding must fit");
    executor
        .bind_group_sched("node", "/", "ctrl", sc2)
        .expect("fixture binding must fit");

    let nid = executor.node_builder("node").build().unwrap();
    assert_eq!(executor.nodes[nid.index()].default_sched, SchedContextId(5));

    // group "ctrl" → SC 2 (group beats node default).
    executor.apply_node_default_sched(0, Some(nid), Some("ctrl"));
    assert_eq!(
        executor.sched_context_bindings[0],
        SchedContextId(2),
        "group table entry must beat node default"
    );
}

// Phase 273 W3 — high-level CallbackGroup API tests
// =========================================================================
// These tests exercise the user-facing create_callback_group / create_timer_in_group /
// create_subscription_in_group API end-to-end through the Node, confirming that
// the group name is threaded through to apply_node_default_sched and the
// executor's sched_context_bindings reflect the group's SC (not the node
// default or SC 0).

/// Timer created **in** a named group binds to the group's SC, not SC 0.
#[test]
fn test_callback_group_timer_in_group_binds_to_group_sc() {
    use crate::executor::sched_context::{SchedContext, SchedContextId};

    let session = MockSession::new();
    let mut executor = executor_with_clock(session);

    // Create SC 1 and SC 2.
    let _sc1 = executor
        .create_sched_context(SchedContext::default())
        .unwrap();
    let sc2 = executor
        .create_sched_context(SchedContext::default())
        .unwrap();
    assert_eq!(sc2, SchedContextId(2));

    // Seed: group "ctrl" on node "node" → SC 2.
    executor
        .bind_group_sched("node", "/", "ctrl", SchedContextId(2))
        .expect("fixture binding must fit");

    let nid = executor.node_builder("node").build().unwrap();
    // Node default is SC 0 (no bind_node_name_sched).
    assert_eq!(executor.nodes[nid.index()].default_sched, SchedContextId(0));

    // create_callback_group returns a value type — no borrow conflict.
    let group = executor.node_mut(nid).create_callback_group("ctrl");
    // create_timer_in_group threads the group name through register_timer_on.
    executor
        .node_mut(nid)
        .create_timer_in_group(&group, TimerDuration::from_millis(100), || {})
        .expect("create_timer_in_group");

    // The timer occupies slot 0; its SC binding must be the group's SC 2.
    assert_eq!(
        executor.sched_context_bindings[0],
        SchedContextId(2),
        "timer created in 'ctrl' group must bind to SC 2 (not node default SC 0)"
    );
}

/// Subscription created **in** a named group binds to the group's SC.
#[test]
fn test_callback_group_subscription_in_group_binds_to_group_sc() {
    use crate::executor::sched_context::{SchedContext, SchedContextId};

    let session = MockSession::new();
    let mut executor = executor_with_clock(session);

    let _sc1 = executor
        .create_sched_context(SchedContext::default())
        .unwrap();
    let sc2 = executor
        .create_sched_context(SchedContext::default())
        .unwrap();
    assert_eq!(sc2, SchedContextId(2));

    // Seed: group "telem" on node "sensor" → SC 2.
    executor
        .bind_group_sched("sensor", "/", "telem", SchedContextId(2))
        .expect("fixture binding must fit");

    let nid = executor.node_builder("sensor").build().unwrap();

    let group = executor.node_mut(nid).create_callback_group("telem");
    executor
        .node_mut(nid)
        .create_subscription_in_group::<TestMsg, _>(&group, "/sensor/data", |_: &TestMsg| {})
        .expect("create_subscription_in_group");

    // Subscription occupies slot 0; SC must be the group's SC 2.
    assert_eq!(
        executor.sched_context_bindings[0],
        SchedContextId(2),
        "subscription created in 'telem' group must bind to SC 2"
    );
}

/// create_callback_group with no matching group_sched_table entry falls back
/// to the node default SC (same behavior as passing no group at all).
#[test]
fn test_callback_group_unmapped_group_falls_back_to_node_default() {
    use crate::executor::sched_context::{SchedContext, SchedContextId};

    let session = MockSession::new();
    let mut executor = executor_with_clock(session);

    let _sc1 = executor
        .create_sched_context(SchedContext::default())
        .unwrap();
    let sc2 = executor
        .create_sched_context(SchedContext::default())
        .unwrap();
    assert_eq!(sc2, SchedContextId(2));

    // node "ctrl_node" has a default SC of 2; the group "unknown" has no entry.
    executor
        .bind_node_name_sched("ctrl_node", "/", SchedContextId(2))
        .expect("fixture binding must fit");

    let nid = executor.node_builder("ctrl_node").build().unwrap();
    assert_eq!(executor.nodes[nid.index()].default_sched, SchedContextId(2));

    let group = executor.node_mut(nid).create_callback_group("unknown");
    executor
        .node_mut(nid)
        .create_timer_in_group(&group, TimerDuration::from_millis(50), || {})
        .expect("create_timer_in_group");

    // Falls back to node default (SC 2), not SC 0.
    assert_eq!(
        executor.sched_context_bindings[0],
        SchedContextId(2),
        "unmapped group must fall back to node default SC 2"
    );
}

// ====================================================================
// RFC-0052 / phase-296 W3b.5 — deadline-miss actions
// ====================================================================

private_test_clock!(deadline_skip_clock);

/// `DeadlineAction::Skip`: a callback overrunning its SC deadline masks
/// the SAME SC's remaining callbacks for the rest of that spin cycle,
/// and the miss lands on the violation drain.
#[test]
fn deadline_skip_masks_remaining_same_sc_callbacks() {
    deadline_skip_clock::claim_at_us(1_000_000);
    use crate::executor::sched_context::{DeadlineAction, OptUs, SchedContext};
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock_fn(session, deadline_skip_clock::now_us);

    let fired = alloc::sync::Arc::new(std::sync::Mutex::new(alloc::vec::Vec::<i32>::new()));
    let f_slow = fired.clone();
    let f_victim = fired.clone();

    let nid = executor.node_builder("deadline_skip").build().unwrap();
    let h_slow = executor
        .node_mut(nid)
        .create_service::<TestService, _>("/slow", move |req: &TestServiceRequest| {
            f_slow.lock().unwrap().push(req.a);
            deadline_skip_clock::advance_ms(5);
            TestServiceReply { sum: req.a }
        })
        .unwrap();
    let h_victim = executor
        .node_mut(nid)
        .create_service::<TestService, _>("/victim", move |req: &TestServiceRequest| {
            f_victim.lock().unwrap().push(req.a);
            TestServiceReply { sum: req.a }
        })
        .unwrap();

    // One SC, 1 ms deadline, skip on miss — both callbacks bound to it.
    let sc = executor
        .create_sched_context(SchedContext {
            deadline_us: OptUs::from_us(1_000),
            deadline_action: DeadlineAction::Skip,
            ..Default::default()
        })
        .unwrap();
    executor.bind_handle_to_sched_context(h_slow, sc).unwrap();
    executor.bind_handle_to_sched_context(h_victim, sc).unwrap();

    let (d_slow, n_slow) = encode_test_msg(1);
    let (d_victim, n_victim) = encode_test_msg(2);
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    let off_slow = executor.entries[0].as_ref().unwrap().offset;
    let off_victim = executor.entries[1].as_ref().unwrap().offset;
    unsafe { &*(arena_ptr.add(off_slow) as *const MockServiceServer) }.load(d_slow, n_slow);
    unsafe { &*(arena_ptr.add(off_victim) as *const MockServiceServer) }.load(d_victim, n_victim);

    let _ = executor.spin_once(core::time::Duration::from_millis(0));

    // The slow callback ran 5 ms past a 1 ms deadline; its SC sibling
    // must NOT have fired in the same cycle.
    assert_eq!(*fired.lock().unwrap(), alloc::vec![1], "victim skipped");
    let mut rules = alloc::vec::Vec::new();
    executor.drain_violations(|v| rules.push((v.rule, v.declared)));
    assert!(
        rules
            .iter()
            .any(|(r, d)| *r == "deadline-miss-runtime" && *d == 1_000),
        "miss reported: {rules:?}"
    );

    // Next cycle: the mask is per-cycle — the victim fires now.
    let (d2, n2) = encode_test_msg(3);
    unsafe { &*(arena_ptr.add(off_victim) as *const MockServiceServer) }.load(d2, n2);
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(
        *fired.lock().unwrap(),
        alloc::vec![1, 3],
        "mask cleared next cycle"
    );
}

private_test_clock!(deadline_warn_clock);

/// `DeadlineAction::Warn`: reports the miss but never masks siblings.
#[test]
fn deadline_warn_reports_without_skipping() {
    deadline_warn_clock::claim_at_us(1_000_000);
    use crate::executor::sched_context::{DeadlineAction, OptUs, SchedContext};
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock_fn(session, deadline_warn_clock::now_us);

    let fired = alloc::sync::Arc::new(std::sync::Mutex::new(alloc::vec::Vec::<i32>::new()));
    let f_slow = fired.clone();
    let f_peer = fired.clone();

    let nid = executor.node_builder("deadline_warn").build().unwrap();
    let h_slow = executor
        .node_mut(nid)
        .create_service::<TestService, _>("/slow", move |req: &TestServiceRequest| {
            f_slow.lock().unwrap().push(req.a);
            deadline_warn_clock::advance_ms(5);
            TestServiceReply { sum: req.a }
        })
        .unwrap();
    let h_peer = executor
        .node_mut(nid)
        .create_service::<TestService, _>("/peer", move |req: &TestServiceRequest| {
            f_peer.lock().unwrap().push(req.a);
            TestServiceReply { sum: req.a }
        })
        .unwrap();
    let sc = executor
        .create_sched_context(SchedContext {
            deadline_us: OptUs::from_us(1_000),
            deadline_action: DeadlineAction::Warn,
            ..Default::default()
        })
        .unwrap();
    executor.bind_handle_to_sched_context(h_slow, sc).unwrap();
    executor.bind_handle_to_sched_context(h_peer, sc).unwrap();

    let (d1, n1) = encode_test_msg(1);
    let (d2, n2) = encode_test_msg(2);
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    let off1 = executor.entries[0].as_ref().unwrap().offset;
    let off2 = executor.entries[1].as_ref().unwrap().offset;
    unsafe { &*(arena_ptr.add(off1) as *const MockServiceServer) }.load(d1, n1);
    unsafe { &*(arena_ptr.add(off2) as *const MockServiceServer) }.load(d2, n2);

    let _ = executor.spin_once(core::time::Duration::from_millis(0));

    assert_eq!(
        *fired.lock().unwrap(),
        alloc::vec![1, 2],
        "peer still fires"
    );
    let mut misses = 0;
    executor.drain_violations(|v| {
        if v.rule == "deadline-miss-runtime" {
            misses += 1;
        }
    });
    assert!(misses >= 1, "warn reported the miss");
}

private_test_clock!(stalled_timer_clock);

/// Issue #505 — a stalled tier must SAY it dropped activations.
///
/// The rate rule cannot: `check_rate` samples publish counts over a ~5 s
/// window, so an isolated stall is a fraction of a percent, and under
/// `CatchUp` the replayed activations refill the window entirely. The
/// overrun counter is exact and immediate, and `run_contract_monitors`
/// turns its delta into a `timer-overrun-runtime` violation.
#[test]
fn a_stalled_timer_reports_a_timer_overrun_violation() {
    // A PRIVATE clock the test moves by hand (issue 1945): on the real clock
    // the quiet step below sleeps exactly one period, so a preemption under a
    // parallel gate drops a period and the "no repeat" check reads a new stall.
    stalled_timer_clock::claim_at_us(1_000_000);
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock_fn(session, stalled_timer_clock::now_us);
    let fires = alloc::sync::Arc::new(std::sync::Mutex::new(0u32));
    let f = fires.clone();
    let id = executor
        .register_timer(TimerDuration::from_millis(10), move || {
            *f.lock().unwrap() += 1;
        })
        .unwrap();

    // A stall worth ~12 periods: one activation, the rest dropped.
    let _ = stalled_timer_clock::elapse_then_spin_once(&mut executor, 120);
    assert_eq!(*fires.lock().unwrap(), 1, "Skip coalesces the backlog");
    let overruns = executor.timer_overruns(id).unwrap();
    assert!(overruns >= 10, "dropped periods counted: {overruns}");

    let mut reported = alloc::vec::Vec::new();
    executor.drain_violations(|v| reported.push((v.rule, v.measured)));
    let overrun_rows: alloc::vec::Vec<_> = reported
        .iter()
        .filter(|(r, _)| *r == "timer-overrun-runtime")
        .collect();
    assert_eq!(overrun_rows.len(), 1, "one report: {reported:?}");
    assert_eq!(
        overrun_rows[0].1, overruns,
        "the violation carries the dropped count"
    );

    // A second check with no new stall must stay silent — the rule
    // reports newly dropped activations, not the running total.
    let _ = stalled_timer_clock::elapse_then_spin_once(&mut executor, 10);
    let mut again = alloc::vec::Vec::new();
    executor.drain_violations(|v| again.push(v.rule));
    assert!(
        !again.contains(&"timer-overrun-runtime"),
        "no repeat without a new drop: {again:?}"
    );
}

private_test_clock!(violation_logged_clock);

/// Issue #514 — a detected violation must reach a SINK, not just a ring.
///
/// The ring itself was never the problem: `drain_violations` worked, it
/// simply had no caller outside tests, so every rule ran each spin and
/// discarded its verdict. Logging happens at detection, which is why
/// this test can still drain the ring afterwards — the two paths are
/// independent, and an application that reports violations its own way
/// is unaffected by the default.
#[test]
// Needs time to advance INDEPENDENTLY of how often it is read, which the
// alloc-only shared clock (a per-read counter) cannot do. It ran on `std`
// only for that reason; on a private clock (issue 1944) it runs on both.
fn a_violation_is_logged_and_still_drainable() {
    violation_logged_clock::claim_at_us(1_000_000);
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock_fn(session, violation_logged_clock::now_us);
    executor
        .register_timer(TimerDuration::from_millis(10), || {})
        .unwrap();

    // Stall past several periods so the overrun rule fires.
    let _ = violation_logged_clock::elapse_then_spin_once(&mut executor, 120);

    // Reporting is on by default...
    let mut drained = alloc::vec::Vec::new();
    executor.drain_violations(|v| drained.push(v.rule));
    assert!(
        drained.contains(&"timer-overrun-runtime"),
        "the ring still carries the violation: {drained:?}"
    );
    assert_eq!(executor.violations_dropped(), 0);

    // ...and can be turned off by an application that reports its own way.
    executor.set_report_violations(false);
    let _ = violation_logged_clock::elapse_then_spin_once(&mut executor, 120);
    let mut again = alloc::vec::Vec::new();
    executor.drain_violations(|v| again.push(v.rule));
    assert!(
        again.contains(&"timer-overrun-runtime"),
        "silencing the log must not silence the ring: {again:?}"
    );
}

private_test_clock!(violation_ring_clock);

/// Issue #514 — faults produced faster than they are reported must be
/// COUNTED, not silently lost. The ring holds `MAX_VIOLATIONS`; an
/// application that never drains would otherwise report a stale prefix
/// of its faults and no indication that a prefix is all it is.
#[test]
fn violations_beyond_the_ring_are_counted() {
    violation_ring_clock::claim_at_us(1_000_000);
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock_fn(session, violation_ring_clock::now_us);
    // ONE timer, stalled repeatedly — not `MAX_VIOLATIONS + 4` timers.
    //
    // This test asked for 12 callback slots against a `MAX_CBS` that has
    // defaulted to 4 since 2026-03, so `register_timer` returned
    // `ExecutorFull` on the fifth and the `.unwrap()` panicked. It never
    // passed; it was simply never run (see the module note on the `std`
    // feature gate), so the ring-overflow behaviour it exists to prove has
    // been unverified since #514 landed.
    //
    // Overflowing the ring does not need one timer per violation: a single
    // overrunning timer produces a fresh violation on every stalled spin, and
    // nothing here drains, so `MAX_VIOLATIONS + 4` spins overflow a ring of
    // `MAX_VIOLATIONS`. That also keeps the test independent of `MAX_CBS`,
    // which is a build-time knob any consumer may set.
    executor
        .register_timer(TimerDuration::from_millis(10), || {})
        .unwrap();
    for _ in 0..(super::monitor::MAX_VIOLATIONS + 4) {
        let _ = violation_ring_clock::elapse_then_spin_once(&mut executor, 120);
    }
    // Exact: every 120 ms stall of the 10 ms timer is one verdict, so the
    // spins past the ring's capacity are the drops, one each.
    assert_eq!(
        executor.violations_dropped(),
        4,
        "overflow counted: the 4 verdicts beyond MAX_VIOLATIONS"
    );
}

/// Issue #515 — a period the spin cadence cannot express is announced
/// once, at the point the executor first learns its spin period.
///
/// The audit is a log-side effect, so what is asserted here is the
/// mechanism around it: it runs exactly once, only on a non-zero
/// timeout, and it never touches timer behaviour (the alternation it
/// warns about is legal — the mean cadence is preserved).
#[test]
fn spin_quantization_audit_runs_once_on_the_first_timed_spin() {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);
    // 33 ms on a 5 ms spin: alternates 35/30.
    executor
        .register_timer(TimerDuration::from_millis(33), || {})
        .unwrap();

    // A zero-timeout spin carries no cadence information, so the audit
    // must not consume its one shot on it.
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    assert!(
        !executor.spin_quantization_checked,
        "a zero timeout says nothing about the tier's spin period"
    );

    let _ = executor.spin_once(core::time::Duration::from_millis(5));
    assert!(
        executor.spin_quantization_checked,
        "audited on first timed spin"
    );

    // Idempotent: later spins do not re-audit (and so cannot re-warn
    // every cycle for the lifetime of the image).
    let _ = executor.spin_once(core::time::Duration::from_millis(5));
    assert!(executor.spin_quantization_checked);
}

/// Issue #515 — a period that divides the spin period evenly is not a
/// finding, and neither is a timer with no period.
#[test]
fn spin_quantization_audit_accepts_exact_multiples() {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);
    let id = executor
        .register_timer(TimerDuration::from_millis(50), || {})
        .unwrap();
    let _ = executor.spin_once(core::time::Duration::from_millis(10));
    // 50 ms on a 10 ms spin is expressible; the timer is untouched by
    // the audit either way.
    assert_eq!(executor.timer_period_us(id), Some(50_000));
    assert_eq!(executor.timer_overruns(id), Some(0));
}

/// Issue 0563 — `Executor` must stay small enough that CONSTRUCTING one is not
/// a large stack temporary.
///
/// It is returned by value from `assemble` -> `from_session_in` -> `open_in`
/// before being written into the caller's (static) storage, so its size is paid
/// on the stack of whatever thread opens it. On Zephyr Cortex-M that thread is
/// `main`, and at 11632 bytes it overflowed a 16 KB stack into the idle
/// thread's saved context — issue 0552, which presented as `PC=0` and read for
/// a day as a call through a NULL function pointer.
///
/// Measured breakdown at the time (host, 64-bit):
///
/// ```text
/// Executor          = 11632
///   remap_table     =  6664   <- 57%, the seventh sized table, still inline
///   nodes           =  1448
///   monitor_states  =   128
///   component_slots =   104
///   dispatch_slots  =    72
/// ```
///
/// Moving `remap_table` into the carved backing (where phase-271 had already
/// put the other six) removed that 6664. The bound below is deliberately loose
/// — this guards against a NEW multi-KB inline table appearing, not against
/// small drift, and host `size_of` is not the target's anyway.
#[test]
fn executor_stays_small_enough_to_construct_on_a_stack() {
    let size = core::mem::size_of::<Executor<'static>>();
    assert!(
        size <= 6 * 1024,
        "size_of::<Executor>() = {size} — a new inline table has landed in the \
         struct. It is built as a stack temporary before being moved into the \
         caller's storage, so this is main-stack cost on every embedded board \
         (issue 0563). Carve it from the backing like `entries`/`remap_table` \
         instead of holding it inline."
    );
}

/// Issue 0757 — a take that FAILS must be counted, not swallowed.
///
/// The drain loop had four copies (typed, raw, borrowed/zero-copy, and the C
/// one), and three of them treated any non-`Ok` take as "the queue is empty":
/// `else { break }`. That made a dropped sample indistinguishable from an idle
/// subscription — the executor reported a clean spin, the message was gone, and
/// nothing anywhere counted it.
///
/// The unit suite could not have caught it: `MockSubscriber` held a queue of
/// canned MESSAGES, so no test could express a failing take at all. Injecting
/// the failure is the other half of the fix.
///
/// The assertion is `subscription_errors`, deliberately, rather than the log
/// line: issue 0737 had already fixed the C copy by PROPAGATING to
/// `spin_once`, so the remedy for this class already existed one function over,
/// and a log-only fix would have been a second spelling of it.
#[test]
fn failed_subscription_take_is_counted_not_swallowed() {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);

    let nid = executor
        .node_builder("failed_take_is_counted")
        .build()
        .unwrap();
    let count = alloc::sync::Arc::new(portable_atomic::AtomicUsize::new(0));
    let count_cb = count.clone();
    executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/lossy", move |_msg: &TestMsg| {
            count_cb.fetch_add(1, portable_atomic::Ordering::SeqCst);
        })
        .unwrap();

    let arena_ptr = executor.arena.as_ptr() as *const u8;
    let off = executor.entries[0].as_ref().unwrap().offset;
    unsafe { &*(arena_ptr.add(off) as *const MockSubscriber) }
        .load_error(nros_rmw::TransportError::MessageTooLarge);

    let result = executor.spin_once(core::time::Duration::from_millis(0));

    assert_eq!(
        result.subscription_errors, 1,
        "a failing take must reach subscription_errors — the pre-0757 drain \
         loop broke out of the loop on any non-Ok take, so this read as an \
         empty queue and the spin reported clean"
    );
    assert!(
        result.any_errors(),
        "a spin that dropped a sample is not a clean spin"
    );
    assert_eq!(
        count.load(portable_atomic::Ordering::SeqCst),
        0,
        "no callback can run for a message that never arrived"
    );
}

/// Issue 0757, the other direction — an EMPTY subscription is still not an
/// error.
///
/// Negative control for the test above. The failure mode being fixed is
/// conflating "take failed" with "nothing to take", and a fix that counted both
/// would be the same conflation with the sign flipped: every idle spin on every
/// board would report errors, which is how a counter stops being read.
#[test]
fn empty_subscription_is_not_an_error() {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);

    let nid = executor
        .node_builder("empty_is_not_an_error")
        .build()
        .unwrap();
    executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/idle", move |_msg: &TestMsg| {})
        .unwrap();

    let result = executor.spin_once(core::time::Duration::from_millis(0));

    assert_eq!(
        result.subscription_errors, 0,
        "an idle subscription must not count as an error"
    );
    assert!(!result.any_errors(), "an idle spin is a clean spin");
}

/// Phase 376 W5/B1 — `create_node` REGISTERS, and deduplicates by name.
///
/// Until 2026-08-24 it registered nothing: it built a `NodeHandle` and returned
/// it, so the executor had never heard of a node the caller had just created.
/// The `create_node` VTABLE slot's contract is that the runtime calls it once
/// per distinct `(name, namespace)`, and with no registry to consult there is
/// nothing to make that true — every backend would need its own dedup, which is
/// the registry the slot exists to delete.
#[test]
fn create_node_registers_and_dedups_by_name() {
    let mut executor: Executor = executor_with_clock(MockSession::new());

    assert!(
        executor
            .node_id_by_name("talker", executor.namespace.as_str())
            .is_none(),
        "precondition: the table starts without this node"
    );

    let ns: heapless::String<64> = executor.namespace.clone();
    let _ = executor.create_node("talker").unwrap();
    let first = executor
        .node_id_by_name("talker", ns.as_str())
        .expect("create_node must put the node in the table");

    // The second call is the one that used to hand out an unregistered
    // duplicate. It must reuse the record, not push a second one.
    let _ = executor.create_node("talker").unwrap();
    let second = executor
        .node_id_by_name("talker", ns.as_str())
        .expect("still registered");
    assert_eq!(
        first, second,
        "a repeated name must reuse its record, not create a second"
    );

    // A DIFFERENT name is a different node and does get its own record.
    let _ = executor.create_node("listener").unwrap();
    let other = executor
        .node_id_by_name("listener", ns.as_str())
        .expect("a distinct name registers separately");
    assert_ne!(first, other, "distinct names are distinct nodes");
}

/// phase-515 R2 -- rclrs's `executor.create_node(impl IntoNodeOptions)`: a
/// bare name keeps the executor's namespace, the `.namespace(..)` step on a
/// name places the node there, and the same name in two namespaces is two
/// nodes.
#[test]
fn create_node_takes_rclrs_into_node_options() {
    use crate::{IntoNodeOptions, NodeOptions};
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let ns: heapless::String<64> = executor.namespace.clone();

    let _ = executor.create_node("talker").unwrap();
    let bare = executor
        .node_id_by_name("talker", ns.as_str())
        .expect("a bare name lands in the executor's namespace");

    let _ = executor.create_node("talker".namespace("/robot")).unwrap();
    let placed = executor
        .node_id_by_name("talker", "/robot")
        .expect("`.namespace` places the node");
    assert_ne!(bare, placed, "one name in two namespaces is two nodes");

    // A full `NodeOptions` with the defaults is the bare name: it dedups
    // onto the first record rather than registering a third.
    let _ = executor.create_node(NodeOptions::new("talker")).unwrap();
    assert_eq!(
        executor.node_id_by_name("talker", ns.as_str()),
        Some(bare),
        "default options mean the executor's namespace"
    );
    assert_eq!(executor.nodes().len(), 2);
}

// ============================================================================
// Shutdown hooks (issue 0790)
// ============================================================================

// The hook context is a raw pointer the executor holds for its whole life, so
// every fixture below is `&'static` via a leaked box. `alloc`, not `std`: this
// crate is `no_std` and the test module inherits that.
use alloc::{boxed::Box, vec::Vec};

/// The ordering test's shared observation point.
///
/// A hook is an `extern "C" fn(*mut c_void)` — it gets its own context and
/// nothing else — so what it can SAY about the world has to be reachable
/// through that pointer. Each hook is handed one of these and stamps it with
/// the two numbers that make the ORDER checkable rather than just the fact that
/// it ran: how many times the session had been closed when it fired, and its
/// position in a global run sequence.
#[derive(Debug, Default)]
struct HookWitness {
    /// How many times this hook ran. The whole point of "exactly once".
    runs: core::sync::atomic::AtomicUsize,
    /// `closes` observed at the moment it ran. `0` for a pre-shutdown hook
    /// means the session was still open; `1` for an on-shutdown hook means the
    /// close already happened.
    closes_seen: core::sync::atomic::AtomicUsize,
    /// Its ticket from the shared run sequence, so "pre before post" is an
    /// assertion and not an inference.
    sequence: core::sync::atomic::AtomicUsize,
}

/// What every hook in these tests reads and writes. `&'static` because the
/// registration hands the executor a raw pointer that must outlive it, and a
/// leaked box is the honest way to say so in a test.
struct ShutdownFixture {
    closes: &'static core::sync::atomic::AtomicUsize,
    ticket: core::sync::atomic::AtomicUsize,
    pre: HookWitness,
    post: HookWitness,
}

impl ShutdownFixture {
    /// A fixture plus the `MockSession` whose `close()` feeds it. Leaked, not
    /// dropped: both the session and the executor's hook table hold references
    /// into it for as long as the executor lives, and the test's assertions run
    /// after the executor is gone.
    fn new() -> (&'static ShutdownFixture, MockSession) {
        let closes: &'static core::sync::atomic::AtomicUsize =
            Box::leak(Box::new(core::sync::atomic::AtomicUsize::new(0)));
        let fixture: &'static ShutdownFixture = Box::leak(Box::new(ShutdownFixture {
            closes,
            ticket: core::sync::atomic::AtomicUsize::new(0),
            pre: HookWitness::default(),
            post: HookWitness::default(),
        }));
        (fixture, MockSession::with_close_observer(closes))
    }

    fn closes(&self) -> usize {
        self.closes.load(core::sync::atomic::Ordering::SeqCst)
    }

    fn record(&self, witness: &HookWitness) {
        use core::sync::atomic::Ordering::SeqCst;
        witness.runs.fetch_add(1, SeqCst);
        witness.closes_seen.store(self.closes(), SeqCst);
        witness
            .sequence
            .store(self.ticket.fetch_add(1, SeqCst) + 1, SeqCst);
    }

    fn as_ctx(&'static self) -> *mut core::ffi::c_void {
        self as *const ShutdownFixture as *mut core::ffi::c_void
    }
}

fn runs(witness: &HookWitness) -> usize {
    witness.runs.load(core::sync::atomic::Ordering::SeqCst)
}

fn closes_seen(witness: &HookWitness) -> usize {
    witness
        .closes_seen
        .load(core::sync::atomic::Ordering::SeqCst)
}

fn sequence(witness: &HookWitness) -> usize {
    witness.sequence.load(core::sync::atomic::Ordering::SeqCst)
}

/// # Safety
/// `ctx` must be the `&'static ShutdownFixture` the registration passed.
unsafe extern "C" fn record_pre(ctx: *mut core::ffi::c_void) {
    let fixture = unsafe { &*(ctx as *const ShutdownFixture) };
    fixture.record(&fixture.pre);
}

/// # Safety
/// `ctx` must be the `&'static ShutdownFixture` the registration passed.
unsafe extern "C" fn record_post(ctx: *mut core::ffi::c_void) {
    let fixture = unsafe { &*(ctx as *const ShutdownFixture) };
    fixture.record(&fixture.post);
}

/// # Safety
/// `ctx` must be a `&'static AtomicUsize`.
unsafe extern "C" fn bump(ctx: *mut core::ffi::c_void) {
    let counter = unsafe { &*(ctx as *const core::sync::atomic::AtomicUsize) };
    counter.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
}

/// Issue 0790 — the ORDER is the feature, so this is the test that has to hold.
///
/// It asserts four separate things, because three of them pass on an
/// implementation that has the ordering backwards:
///
/// * the pre-shutdown hook ran while the session was still OPEN (`closes_seen
///   == 0`) — this is the half with no workaround, the one a node needs to
///   publish a final state or park an actuator;
/// * the on-shutdown hook ran AFTER the close (`closes_seen == 1`);
/// * the pre hook's sequence ticket precedes the post hook's;
/// * each ran exactly once, including across a SECOND `close()` and the drop
///   that follows.
#[test]
fn a_pre_shutdown_hook_sees_a_live_session_and_an_on_shutdown_hook_sees_a_closed_one() {
    let (fixture, session) = ShutdownFixture::new();
    let mut executor: Executor = executor_with_clock(session);

    // SAFETY: `fixture` is `&'static`, so the context outlives the executor,
    // and both trampolines expect exactly that pointer.
    let pre_handle = unsafe {
        executor
            .add_pre_shutdown_callback(record_pre, fixture.as_ctx())
            .expect("a fresh executor has a free pre-shutdown slot")
    };
    let post_handle = unsafe {
        executor
            .add_on_shutdown_callback(record_post, fixture.as_ctx())
            .expect("a fresh executor has a free on-shutdown slot")
    };
    assert_eq!(pre_handle.phase(), Some(ShutdownPhase::Pre));
    assert_eq!(post_handle.phase(), Some(ShutdownPhase::Post));
    assert_eq!(executor.shutdown_callback_count(ShutdownPhase::Pre), 1);
    assert_eq!(executor.shutdown_callback_count(ShutdownPhase::Post), 1);

    // Nothing has run yet: registration is not invocation.
    assert_eq!(runs(&fixture.pre), 0, "pre-shutdown hook ran too early");
    assert_eq!(runs(&fixture.post), 0, "on-shutdown hook ran too early");

    executor.close().expect("MockSession::close succeeds");

    assert_eq!(runs(&fixture.pre), 1, "pre-shutdown hook did not run");
    assert_eq!(runs(&fixture.post), 1, "on-shutdown hook did not run");
    assert_eq!(
        closes_seen(&fixture.pre),
        0,
        "the pre-shutdown hook ran AFTER the session closed — that is the phase \
         with no workaround, and it is useless if the entities are already gone"
    );
    assert_eq!(
        closes_seen(&fixture.post),
        1,
        "the on-shutdown hook ran BEFORE the session closed"
    );
    assert!(
        sequence(&fixture.pre) < sequence(&fixture.post),
        "pre-shutdown must precede on-shutdown (pre ticket {}, post ticket {})",
        sequence(&fixture.pre),
        sequence(&fixture.post),
    );

    // Exactly once, part 1: a second close finds both tables empty.
    executor.close().expect("a second close is not an error");
    assert_eq!(runs(&fixture.pre), 1, "pre-shutdown hook ran twice");
    assert_eq!(runs(&fixture.post), 1, "on-shutdown hook ran twice");

    // Exactly once, part 2: the drop sweep must not re-run what close ran.
    drop(executor);
    assert_eq!(runs(&fixture.pre), 1, "drop re-ran the pre-shutdown hook");
    assert_eq!(runs(&fixture.post), 1, "drop re-ran the on-shutdown hook");
}

/// Issue 0790 — the C API's `rclc_executor_fini` drops the executor in place
/// and never calls `close()`. If `Drop` did not sweep the tables the whole
/// facility would be silently inert for every C entry, which is the failure
/// mode this repo calls a museum binary: the code is there, the gate is green,
/// and nothing runs.
#[test]
fn dropping_an_executor_without_closing_it_still_runs_both_phases_once() {
    let (fixture, session) = ShutdownFixture::new();
    let mut executor: Executor = executor_with_clock(session);
    // SAFETY: as above — `fixture` is `&'static`.
    unsafe {
        executor
            .add_pre_shutdown_callback(record_pre, fixture.as_ctx())
            .unwrap();
        executor
            .add_on_shutdown_callback(record_post, fixture.as_ctx())
            .unwrap();
    }

    drop(executor);

    assert_eq!(runs(&fixture.pre), 1, "drop skipped the pre-shutdown hook");
    assert_eq!(runs(&fixture.post), 1, "drop skipped the on-shutdown hook");
    assert!(
        sequence(&fixture.pre) < sequence(&fixture.post),
        "drop must keep the phase order"
    );
}

/// Issue 0790 — the handle exists so a registration can be undone.
#[test]
fn a_removed_shutdown_hook_does_not_run() {
    let (fixture, session) = ShutdownFixture::new();
    let mut executor: Executor = executor_with_clock(session);
    // SAFETY: as above.
    let handle = unsafe {
        executor
            .add_pre_shutdown_callback(record_pre, fixture.as_ctx())
            .unwrap()
    };

    assert!(
        executor.remove_pre_shutdown_callback(handle),
        "removing a live hook reports true"
    );
    assert_eq!(executor.shutdown_callback_count(ShutdownPhase::Pre), 0);
    assert!(
        !executor.remove_pre_shutdown_callback(handle),
        "removing it twice reports false rather than erroring"
    );

    executor.close().unwrap();
    assert_eq!(runs(&fixture.pre), 0, "a removed hook still ran");
}

/// Issue 0790 — a slot index means nothing without the table it indexes, and
/// there are two tables. The phase tag in the handle is what keeps a pre-phase
/// handle from removing the on-phase hook that happens to share its index.
#[test]
fn a_handle_from_one_phase_cannot_remove_the_other_phases_hook() {
    let (fixture, session) = ShutdownFixture::new();
    let mut executor: Executor = executor_with_clock(session);
    // SAFETY: as above.
    let (pre_handle, post_handle) = unsafe {
        (
            executor
                .add_pre_shutdown_callback(record_pre, fixture.as_ctx())
                .unwrap(),
            executor
                .add_on_shutdown_callback(record_post, fixture.as_ctx())
                .unwrap(),
        )
    };
    // Same slot index, different phase — the exact collision an untagged
    // index-only handle would not survive.
    assert_eq!(pre_handle.index(), post_handle.index());
    assert_ne!(pre_handle, post_handle);

    assert!(!executor.remove_on_shutdown_callback(pre_handle));
    assert!(!executor.remove_pre_shutdown_callback(post_handle));
    assert_eq!(executor.shutdown_callback_count(ShutdownPhase::Pre), 1);
    assert_eq!(executor.shutdown_callback_count(ShutdownPhase::Post), 1);

    executor.close().unwrap();
    assert_eq!(runs(&fixture.pre), 1);
    assert_eq!(runs(&fixture.post), 1);
}

/// Issue 0790 — the tables are fixed-capacity by design (issue 0460's
/// precedent: a static slot is a cost every image pays). Overflow must name the
/// knob rather than silently dropping the registration, because a dropped
/// shutdown hook is invisible until the day it was needed.
#[test]
fn registering_past_capacity_reports_the_knob_and_frees_on_removal() {
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);
    let counter: &'static core::sync::atomic::AtomicUsize =
        Box::leak(Box::new(core::sync::atomic::AtomicUsize::new(0)));
    let ctx = counter as *const _ as *mut core::ffi::c_void;

    let mut handles = Vec::new();
    for slot in 0..crate::config::MAX_SHUTDOWN_CBS {
        // SAFETY: `counter` is `&'static` and `bump` expects exactly it.
        handles.push(unsafe {
            executor
                .add_pre_shutdown_callback(bump, ctx)
                .unwrap_or_else(|e| panic!("slot {slot} should be free, got {e:?}"))
        });
    }
    // SAFETY: as above.
    let overflow = unsafe { executor.add_pre_shutdown_callback(bump, ctx) };
    assert_eq!(
        overflow.unwrap_err(),
        NodeError::ShutdownCallbacksFull,
        "overflow must name the shutdown-hook table, not a generic full error"
    );

    // A freed slot is reusable — the tables are arrays with holes, not a fill
    // cursor that can only grow.
    assert!(executor.remove_pre_shutdown_callback(handles[0]));
    // SAFETY: as above.
    let reused = unsafe { executor.add_pre_shutdown_callback(bump, ctx) }
        .expect("a removed slot is available again");
    assert_eq!(reused, handles[0], "the freed slot is the one reused");

    executor.close().unwrap();
    assert_eq!(
        counter.load(core::sync::atomic::Ordering::SeqCst),
        crate::config::MAX_SHUTDOWN_CBS,
        "every occupied slot runs exactly once"
    );
}

// ============================================================================
// Issue 0796 — the raw action server's post-accept hook
// ============================================================================

/// How many times the accepted-goal hook fired, and for which goal.
///
/// Statics rather than a captured counter because the hook is an
/// `extern "C" fn` — the same constraint that makes the C and C++ tiers pass a
/// context pointer. The test is single-threaded and resets both up front.
static ACCEPTED_CALLS: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);
static ACCEPTED_UUID_FIRST_BYTE: core::sync::atomic::AtomicU8 =
    core::sync::atomic::AtomicU8::new(0);

unsafe extern "C" fn defer_every_goal(
    _goal_id: *const nros_core::GoalUuid,
    _data: *const u8,
    _len: usize,
    _ctx: *mut core::ffi::c_void,
) -> nros_core::GoalResponse {
    nros_core::GoalResponse::AcceptAndDefer
}

unsafe extern "C" fn reject_every_goal(
    _goal_id: *const nros_core::GoalUuid,
    _data: *const u8,
    _len: usize,
    _ctx: *mut core::ffi::c_void,
) -> nros_core::GoalResponse {
    nros_core::GoalResponse::Reject
}

unsafe extern "C" fn refuse_every_cancel(
    _goal_id: *const nros_core::GoalUuid,
    _status: nros_core::GoalStatusCode,
    _ctx: *mut core::ffi::c_void,
) -> nros_core::CancelResponse {
    nros_core::CancelResponse::Reject
}

unsafe extern "C" fn count_accepted(
    goal_id: *const nros_core::GoalUuid,
    _ctx: *mut core::ffi::c_void,
) {
    use core::sync::atomic::Ordering::SeqCst;
    ACCEPTED_CALLS.fetch_add(1, SeqCst);
    ACCEPTED_UUID_FIRST_BYTE.store(unsafe { (*goal_id).uuid[0] }, SeqCst);
}

/// A `send_goal` wire frame: `[CDR-LE header][fixed uint8[16] goal_id]`.
/// The goal FIELDS would follow; this action's callbacks ignore them.
#[cfg(test)]
fn mk_send_goal_req(goal_id: &nros_core::GoalUuid) -> ([u8; 256], usize) {
    let mut b = [0u8; 256];
    b[..4].copy_from_slice(&[0x00, 0x01, 0x00, 0x00]);
    b[4..20].copy_from_slice(&goal_id.uuid);
    (b, 20)
}

/// Issue 0796 — a goal accepted with `AcceptAndDefer` must reach the
/// accepted-goal hook exactly once.
///
/// This is the mechanism the C++ callback tier was missing: it registered
/// `accepted_callback: None`, so a C++ user who deferred a goal was never told
/// it had been accepted — the moment they are supposed to start executing it.
/// The hook is registered unconditionally now and dispatched here, in
/// `action_server_raw_try_process`.
///
/// Three things are pinned: it fires (once, for the right goal), it fires
/// AFTER the accept reply is on the wire (a long-running body must not delay
/// acceptance), and a REJECTED goal does not reach it at all.
#[test]
fn a_deferred_goal_reaches_the_accepted_callback_exactly_once() {
    use super::{
        action_core::ActionServerCore,
        arena::{ActionServerRawArenaEntry, action_server_raw_try_process},
    };
    use crate::mock::MockPublisher;
    use core::sync::atomic::Ordering::SeqCst;
    use nros_core::GoalUuid;

    const GB: usize = 128;
    const RB: usize = 128;
    const FB: usize = 128;
    const MG: usize = 2;

    ACCEPTED_CALLS.store(0, SeqCst);
    ACCEPTED_UUID_FIRST_BYTE.store(0, SeqCst);

    let core: ActionServerCore<GB, RB, FB, MG> = ActionServerCore::from_channels(
        MockServiceServer::new(),
        MockServiceServer::new(),
        MockServiceServer::new(),
        MockPublisher::new(),
        MockPublisher::new(),
    );
    let mut entry: ActionServerRawArenaEntry<GB, RB, FB, MG> = ActionServerRawArenaEntry {
        core,
        goal_callback: defer_every_goal,
        cancel_callback: refuse_every_cancel,
        accepted_callback: Some(count_accepted),
        context: core::ptr::null_mut(),
    };
    let ptr = (&mut entry) as *mut _ as *mut u8;

    // No request pending → no work, and certainly no accept.
    let did_work = unsafe { action_server_raw_try_process::<GB, RB, FB, MG>(ptr, 0, 0) }.unwrap();
    assert!(!did_work, "an idle server must report no work");
    assert_eq!(ACCEPTED_CALLS.load(SeqCst), 0);

    let g = GoalUuid { uuid: [0xA5u8; 16] };
    let (req, len) = mk_send_goal_req(&g);
    entry.core.send_goal_server.load(req, len);

    let did_work = unsafe { action_server_raw_try_process::<GB, RB, FB, MG>(ptr, 0, 0) }.unwrap();
    assert!(did_work);
    assert_eq!(
        ACCEPTED_CALLS.load(SeqCst),
        1,
        "a deferred goal must reach the accepted callback exactly once"
    );
    assert_eq!(
        ACCEPTED_UUID_FIRST_BYTE.load(SeqCst),
        0xA5,
        "the hook must be told WHICH goal was accepted"
    );

    // The accept reply is on the wire, and the goal is live in the arena —
    // i.e. the hook ran after acceptance, not instead of it.
    {
        let sent = entry.core.send_goal_server.sent.borrow();
        assert_eq!(sent.len(), 1, "exactly one accept reply");
        assert_eq!(sent[0].1[4], 1, "the reply must say ACCEPTED");
    }
    assert_eq!(entry.core.active_goals.len(), 1);

    // Spinning again with nothing pending must not re-fire the hook.
    let did_work = unsafe { action_server_raw_try_process::<GB, RB, FB, MG>(ptr, 0, 0) }.unwrap();
    assert!(!did_work);
    assert_eq!(
        ACCEPTED_CALLS.load(SeqCst),
        1,
        "the hook is per-acceptance, not per-spin"
    );

    // A REJECTED goal must not reach it.
    entry.goal_callback = reject_every_goal;
    let g2 = GoalUuid { uuid: [0x5Au8; 16] };
    let (req2, len2) = mk_send_goal_req(&g2);
    entry.core.send_goal_server.load(req2, len2);
    let did_work = unsafe { action_server_raw_try_process::<GB, RB, FB, MG>(ptr, 0, 0) }.unwrap();
    assert!(did_work);
    assert_eq!(
        ACCEPTED_CALLS.load(SeqCst),
        1,
        "a rejected goal must not reach the accepted callback"
    );
    assert_eq!(
        entry.core.active_goals.len(),
        1,
        "still only the first goal"
    );
}

// ====================================================================
// issue 1496 — detaching an arena action entry from its owner's storage
// ====================================================================

static LATE_GOAL_CALLS: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);
static LAST_GOAL_CONTEXT: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

/// Stands in for `goal_callback_trampoline` in the C++ tier: it READS the
/// registered context, which after the owner is destroyed is freed storage.
unsafe extern "C" fn accept_and_record_context(
    _goal_id: *const nros_core::GoalUuid,
    _data: *const u8,
    _len: usize,
    ctx: *mut core::ffi::c_void,
) -> nros_core::GoalResponse {
    use core::sync::atomic::Ordering::SeqCst;
    LATE_GOAL_CALLS.fetch_add(1, SeqCst);
    LAST_GOAL_CONTEXT.store(ctx as usize, SeqCst);
    nros_core::GoalResponse::AcceptAndExecute
}

/// Issue 1496 — a `send_goal` arriving after the owner of an arena action
/// server's storage is gone must be REJECTED, not dispatched through a context
/// that names freed memory.
///
/// `nros_cpp_action_server_register` hands the arena `storage` itself as the
/// callback context, and a DETACH leaves the entry in place (only
/// `release_action_server_raw` removes it), so the entry and its three service
/// servers survive the owner.
/// `Executor::detach_action_server_raw` is the cut: the two callbacks become
/// context-free stubs and the context goes null.
///
/// Measured here rather than reasoned about, on both sides of the detach: the
/// same request frame is fed to the same entry twice, and the difference is
/// whether the user callback runs at all and what the client is told.
#[test]
fn detaching_a_raw_action_server_rejects_a_late_goal_and_never_reads_its_context() {
    use super::{
        action::RawActionServerSpec,
        arena::{ActionServerRawArenaEntry, action_server_raw_try_process},
    };
    use core::sync::atomic::Ordering::SeqCst;
    use nros_core::GoalUuid;

    const B: usize = crate::config::DEFAULT_RX_BUF_SIZE;
    const MG: usize = 4;

    LATE_GOAL_CALLS.store(0, SeqCst);
    LAST_GOAL_CONTEXT.store(0, SeqCst);

    // The stand-in for a C++ `Server<A>::storage_`: the address the arena entry
    // is told to call back through.
    let mut owner_storage = [0u8; 16];
    let owner_ptr = owner_storage.as_mut_ptr() as *mut core::ffi::c_void;

    let session = MockSession::new();
    let mut executor = executor_with_clock(session);
    let handle = executor
        .register_action_server_raw(RawActionServerSpec {
            node_id: None,
            action_name: "/verb",
            type_name: "test_msgs/action/Fibonacci",
            type_hash: "RIHS01_0000",
            hashes: nros_rmw::ActionEndpointHashes::uniform("RIHS01_0000"),
            qos: nros_rmw::QoSProfile::services_default(),
            goal_callback: accept_and_record_context,
            cancel_callback: refuse_every_cancel,
            accepted_callback: None,
            context: owner_ptr,
        })
        .expect("the mock session registers a raw action server");

    // Reach the entry the way `spin_once` does, so the assertions below are
    // about the arena's real bytes and not a copy.
    let offset = executor.entries[handle.entry_index]
        .as_ref()
        .expect("the slot is live")
        .offset;
    let entry_ptr = unsafe { (executor.arena.as_mut_ptr() as *mut u8).add(offset) };
    let entry = || unsafe { &mut *(entry_ptr as *mut ActionServerRawArenaEntry<B, B, B, MG>) };

    // BEFORE: a goal is dispatched, through the owner's context.
    let g = GoalUuid { uuid: [0x11u8; 16] };
    let (req, len) = mk_send_goal_req(&g);
    entry().core.send_goal_server.load(req, len);
    let did_work =
        unsafe { action_server_raw_try_process::<B, B, B, MG>(entry_ptr, 0, 0) }.unwrap();
    assert!(did_work);
    assert_eq!(LATE_GOAL_CALLS.load(SeqCst), 1, "the goal must dispatch");
    assert_eq!(
        LAST_GOAL_CONTEXT.load(SeqCst),
        owner_ptr as usize,
        "and it must dispatch through the owner's storage — that is the edge \
         issue 1496 is about"
    );
    assert_eq!(entry().core.active_goals.len(), 1);
    {
        let sent = entry().core.send_goal_server.sent.borrow();
        assert_eq!(sent.last().unwrap().1[4], 1, "reply says ACCEPTED");
    }

    // The owner goes away. This is what its destructor does first.
    // SAFETY: registered through the default-sized `register_action_server_raw`.
    assert!(
        unsafe { executor.detach_action_server_raw(&handle) },
        "the entry is live and is an action server, so the detach applies"
    );
    assert!(
        entry().context.is_null(),
        "the context must no longer name the owner's storage"
    );
    assert!(
        entry().accepted_callback.is_none(),
        "the post-accept hook read the same context"
    );

    // AFTER: the same frame, the same entry. The user callback must not run,
    // and the client must get a well-formed rejection rather than silence.
    let g2 = GoalUuid { uuid: [0x22u8; 16] };
    let (req2, len2) = mk_send_goal_req(&g2);
    entry().core.send_goal_server.load(req2, len2);
    let did_work =
        unsafe { action_server_raw_try_process::<B, B, B, MG>(entry_ptr, 0, 0) }.unwrap();
    assert!(
        did_work,
        "the entry still answers — it is abandoned, not gone"
    );
    assert_eq!(
        LATE_GOAL_CALLS.load(SeqCst),
        1,
        "the late goal must NOT reach the callback that reads the freed storage"
    );
    {
        let sent = entry().core.send_goal_server.sent.borrow();
        assert_eq!(sent.len(), 2, "the late goal is answered, not dropped");
        assert_eq!(sent.last().unwrap().1[4], 0, "reply says REJECTED");
    }
    assert_eq!(
        entry().core.active_goals.len(),
        1,
        "no new goal is accepted after the detach"
    );

    // `owner_storage` may now die; nothing names it.
    let _ = owner_storage;
}

/// Issue 1496, the client half — `nros_cpp_action_client_destroy` detaches the
/// arena entry before the `drop_in_place` that runs no destructor.
///
/// The client entry's three callbacks are `Option`s that
/// `action_client_raw_try_process` guards with `if let Some(cb)`, so clearing
/// them IS the rejection path: a reply, feedback sample or result arriving after
/// destruction is consumed and dropped instead of dispatched through a context
/// that names freed C++ storage.
#[test]
fn detaching_a_raw_action_client_clears_the_callbacks_that_read_its_context() {
    use super::{action::RawActionClientSpec, arena::ActionClientRawArenaEntry};

    const B: usize = crate::config::DEFAULT_RX_BUF_SIZE;

    let mut owner_storage = [0u8; 16];
    let owner_ptr = owner_storage.as_mut_ptr() as *mut core::ffi::c_void;

    let session = MockSession::new();
    let mut executor = executor_with_clock(session);
    let handle = executor
        .register_action_client_raw(RawActionClientSpec {
            node_id: None,
            action_name: "/verb",
            type_name: "test_msgs/action/Fibonacci",
            type_hash: "RIHS01_0000",
            hashes: nros_rmw::ActionEndpointHashes::uniform("RIHS01_0000"),
            goal_response_callback: Some(no_op_goal_response),
            feedback_callback: None,
            result_callback: None,
            cancel_response_callback: None,
            context: owner_ptr,
        })
        .expect("the mock session registers a raw action client");

    let index = handle.entry_index();
    let offset = executor.entries[index]
        .as_ref()
        .expect("the slot is live")
        .offset;
    let entry_ptr = unsafe { (executor.arena.as_mut_ptr() as *mut u8).add(offset) };
    let entry = || unsafe { &mut *(entry_ptr as *mut ActionClientRawArenaEntry<B, B, B>) };

    assert!(entry().goal_response_callback.is_some());
    assert_eq!(entry().context as usize, owner_ptr as usize);

    // SAFETY: registered through the default-sized `register_action_client_raw`.
    assert!(unsafe { executor.detach_action_client_raw(index) });

    assert!(entry().goal_response_callback.is_none());
    assert!(entry().feedback_callback.is_none());
    assert!(entry().result_callback.is_none());
    assert!(entry().context.is_null());

    let _ = owner_storage;
}

unsafe extern "C" fn no_op_goal_response(
    _goal_id: *const nros_core::GoalUuid,
    _accepted: bool,
    _ctx: *mut core::ffi::c_void,
) {
}

// ====================================================================
// Phase 403 step 3 -- the ARENA COST OF ONE ENTITY, MEASURED
//
// Step 3 derives the arena as a sum over what the entity inventory says
// the image creates, so it needs a per-kind cost. These MEASURE it:
// `arena_alloc` is a BUMP allocator, so an `arena_used()` delta across
// exactly one registration IS that entity's arena footprint, including
// alignment padding at the region boundary and the trailing buffer that
// `arena_alloc_with_trailing` places after the entry.
//
// Measured, never modelled. A table derived by reading `size_of` on the
// entry structs would agree with the allocator right up until an
// alignment or a trailing term moved, and this campaign has produced six
// mechanisms that were arithmetically right and did not reach the thing
// they sized. The derivation must be checked against what the allocator
// ACTUALLY claims, which is what these deltas are.
//
// WHY THIS MATTERS MORE THAN THE OTHER KNOBS. An under-sized arena halts
// DURING entity creation, before the first spin, so issue 0900's advisory
// never prints. `MAX_CBS` fails at registration with `ExecutorFull` and
// `MAX_NODES` with `NodeTableFull`, both naming their knob; the arena is
// the one number whose failure mode cannot report itself. So its inputs
// are measured before anything derives from them.
// ====================================================================

/// The arena cost of one TIMER, which carries no payload buffer and so is
/// the floor every other kind is measured against.
fn arena_cost_timer(executor: &mut Executor<'_>) -> usize {
    let before = executor.arena_used();
    executor
        .register_timer(TimerDuration::from_millis(1000), || {})
        .expect("timer registration must succeed");
    executor.arena_used() - before
}

/// The arena cost of one SERVICE SERVER. A service carries TWO payload
/// buffers (request and reply), so it is the kind whose cost is least
/// predictable from a subscription's.
fn arena_cost_service(executor: &mut Executor<'_>, name: &str) -> usize {
    let before = executor.arena_used();
    executor
        .register_service_sized::<TestService, _, 512, 512>(name, |req: &TestServiceRequest| {
            TestServiceReply { sum: req.a }
        })
        .expect("service registration must succeed");
    executor.arena_used() - before
}

#[test]
fn a_service_server_costs_a_measurable_and_constant_arena_slice() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let first = arena_cost_service(&mut executor, "/svc_a");
    assert!(
        first > 0,
        "a service server must claim arena; a zero delta means the probe is \
         not measuring the allocation it names"
    );
    let second = arena_cost_service(&mut executor, "/svc_b");
    assert_eq!(
        first, second,
        "two service servers must cost the same; step 3 sums a per-kind \
         constant over the declared count, which is only valid if it IS \
         constant"
    );
}

#[test]
fn an_unbounded_subscription_costs_more_than_a_bounded_one() {
    // Measured 2316 bounded against 5356 unbounded on x86_64 -- 2.3x. This is
    // why step 3 must REFUSE when a subscribed type is unbounded rather than
    // budgeting the fallback: the fallback is the expensive arm, so an image
    // that silently took it would be sized for a number nobody chose.
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let nid = executor
        .node_builder("bounded_vs_not")
        .build()
        .expect("node");
    let bounded = arena_delta(&mut executor, nid, "/b", qos_with_depth(1), RxAsk::FromType);
    // phase-392 W3c -- the fallback is now spelled `.rx_buffer::<GLOBAL>()`.
    // A subscription that names nothing DERIVES where the bound is reachable,
    // so it is no longer the fallback and would measure equal on that arm.
    let unbounded = arena_delta(&mut executor, nid, "/u", qos_with_depth(1), RxAsk::Global);
    assert!(
        unbounded > bounded,
        "the unbounded fallback must cost MORE than a derived bound \
         (got {unbounded} vs {bounded}); if it were cheaper, refusing to \
         derive would be the conservative choice and it is not"
    );
}

/// The per-kind arena cost table, as a MEASUREMENT PROBE.
///
///     cargo test -p nros-node --features std --lib report_arena_costs -- --ignored
///
/// Reports through `panic!` rather than `println!` deliberately: this crate is
/// `#![no_std]`, `std::println!` is banned in its `src/` (issue 0589 -- it
/// SIGSEGVs a Zephyr native_sim image), and the panic payload is the one
/// output channel that needs no stdio. `#[ignore]` because it is a tool for a
/// human, not a verdict; the assertions below are the verdicts and they run
/// every time.
///
/// HOST NUMBERS. The entry structs hold pointers, so a 32-bit target's table
/// differs. Step 3 must compute per target and may never copy these.
///
/// Measured 2026-09-03, x86_64:
///     timer=32  sub_d1=2316  sub_d10=2504  sub_unbounded=5356
///     service_512_512=3496  per_depth=20
///
/// The d1->d10 delta of 188 does NOT match
/// `(depth + 1) * bound + (depth + 1) * 8` exactly for any plausible bound of
/// `TestMsg` (a bound of 12 predicts 180), so the region carries alignment or
/// padding terms that formula omits. That gap is the whole reason step 3
/// derives against THIS probe rather than against the formula.
#[test]
#[ignore = "measurement probe: run with --ignored to read the table"]
fn report_arena_costs() {
    // One executor PER measurement. The default MAX_CBS is 4, and this probe
    // registers more entities than that -- the first version died with
    // `ExecutorFull`, which is the knob naming itself and is exactly the
    // failure mode the arena does NOT have. A bump allocator's deltas are
    // independent, so per-kind executors measure the same thing.
    let timer = {
        let mut e: Executor = executor_with_clock(MockSession::new());
        arena_cost_timer(&mut e)
    };
    let (d1, d10, unb) = {
        let mut e: Executor = executor_with_clock(MockSession::new());
        let nid = e.node_builder("arena_report").build().expect("node");
        let a = arena_delta(&mut e, nid, "/r1", qos_with_depth(1), RxAsk::FromType);
        let b = arena_delta(&mut e, nid, "/r10", qos_with_depth(10), RxAsk::FromType);
        let c = arena_delta(&mut e, nid, "/ru", qos_with_depth(1), RxAsk::Default);
        (a, b, c)
    };
    let svc = {
        let mut e: Executor = executor_with_clock(MockSession::new());
        arena_cost_service(&mut e, "/rsvc")
    };
    panic!(
        "ARENA timer={timer} sub_d1={d1} sub_d10={d10} sub_unbounded={unb} \
         service_512_512={svc} per_depth={}",
        (d10 as isize - d1 as isize) / 9
    );
}

#[test]
fn a_timer_costs_a_measurable_and_depth_independent_arena_slice() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let first = arena_cost_timer(&mut executor);
    assert!(
        first > 0,
        "a timer must claim arena; a zero delta means the probe is not \
         measuring the allocation it names"
    );

    // A second timer costs the same: the per-kind cost is a CONSTANT the
    // derivation may multiply by a declared count. If this ever differs,
    // the arena is not a straight sum and step 3's premise is wrong.
    let second = arena_cost_timer(&mut executor);
    assert_eq!(
        first, second,
        "two timers must cost the same; step 3 sums a per-kind constant \
         over the declared count, which is only valid if it IS constant"
    );
}

#[test]
fn a_subscriptions_arena_cost_scales_with_declared_depth() {
    // The whole reason step 2 had to land first. Depth is a MULTIPLIER on
    // the bound -- `buffered_region_size(depth, bound)` is
    // `(depth + 1) * bound + (depth + 1) * 8` -- so a derivation that
    // ignores it is wrong by up to 10x, in whichever direction hurts.
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let nid = executor.node_builder("arena_probe").build().expect("node");

    let d1 = arena_delta(
        &mut executor,
        nid,
        "/d1",
        qos_with_depth(1),
        RxAsk::FromType,
    );
    let d10 = arena_delta(
        &mut executor,
        nid,
        "/d10",
        qos_with_depth(10),
        RxAsk::FromType,
    );

    assert!(
        d10 > d1,
        "depth 10 must claim more arena than depth 1 (got {d10} vs {d1}); \
         if these are equal the buffered region is not depth-scaled and \
         the arena cannot be derived from a declared depth"
    );
}

/// Issue #515's audit is STATIC: it warns when a declared period is not a
/// multiple of the spin period, and says in its own comment that "the jitter
/// stays invisible until someone measures cadence on target". These cover the
/// measurement that makes it visible.
#[test]
fn release_jitter_starts_empty_and_clears() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    assert_eq!(
        executor.release_jitter(),
        (0, 0, 0),
        "nothing has been measured yet, so it must report nothing rather than \
         a confident zero-jitter"
    );
    executor.clear_release_jitter_stats();
    assert_eq!(executor.release_jitter(), (0, 0, 0));
}

/// The measurement must live on the path the C++ entry actually drives.
/// nros-cpp paces its tiers with a `spin_once` loop, not `spin_period`, so a
/// probe on `spin_period` alone records zero forever on that lane -- issue
/// 0736's mistake one layer up.
#[test]
fn spin_once_counts_wakes_without_spin_period() {
    // phase-474 I11 -- a wake is a grid RELEASE, so the spins must actually
    // reach their grid points: the park advances this test's clock.
    unsafe extern "C" fn park(_c: *mut core::ffi::c_void, us: u64) -> i8 {
        spin_once_wakes_clock::set_us(spin_once_wakes_clock::now_us() + us);
        1
    }
    spin_once_wakes_clock::claim_at_us(0);
    let mut executor = executor_with_clock_fn(MockSession::new(), spin_once_wakes_clock::now_us);
    executor.set_park_primitive(park, core::ptr::null_mut(), 1);
    for _ in 0..5 {
        executor.spin_once(core::time::Duration::from_millis(2));
    }
    let (_max, late, total) = executor.release_jitter();
    assert_eq!(
        total, 5,
        "five spin_once calls on the grid are five releases"
    );
    assert!(late <= total);
}

/// A zero timeout claims no cadence, so it must not enter the statistic --
/// otherwise every `Future::wait` busy spin would register as a late wake.
#[test]
fn a_zero_timeout_spin_claims_no_cadence() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    for _ in 0..5 {
        executor.spin_once(core::time::Duration::ZERO);
    }
    assert_eq!(
        executor.release_jitter(),
        (0, 0, 0),
        "a polling spin declares no period, so it cannot be late for one"
    );
}

/// `spin_once`'s argument is a blocking BOUND, not a cadence. nros-cpp's tier
/// loops pass a fixed 10 ms and then pace themselves with `platform_sleep_us`,
/// so the rule was judging every tier against 10 ms whatever period the
/// contract declared. A declared cadence must win.
#[test]
fn a_declared_cadence_wins_over_the_spin_timeout() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    executor.set_spin_nominal_us(1_000);
    executor.spin_once(core::time::Duration::from_millis(10));
    assert_eq!(
        executor.spin_nominal_us, 1_000,
        "the declared 1 ms cadence must be the yardstick, not the 10 ms bound"
    );
}

/// ... and with nothing declared the timeout still stands in, so every caller
/// that never learns about the setter keeps the behaviour it had.
#[test]
fn an_undeclared_cadence_falls_back_to_the_timeout() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    executor.spin_once(core::time::Duration::from_millis(10));
    assert_eq!(
        executor.spin_nominal_us, 10_000,
        "with no declaration the timeout is the only cadence on offer"
    );
}

private_test_clock!(tier_slower_cadence_clock);

/// The bug this fixes, end to end: a 1 kHz tier that actually wakes every
/// 10 ms is nine periods late every time, and against the old 10 ms yardstick
/// it looked perfectly on time.
///
/// Issue 1944 — the 10 ms between wakes is an advance of a private clock, so
/// every interval is exactly 10 ms and the lateness exactly 9 ms. It was a
/// real sleep, which only ever made the numbers larger; the bounds held, but
/// as "at least" where they can now be "exactly".
#[test]
fn a_tier_slower_than_its_declared_cadence_is_late() {
    tier_slower_cadence_clock::claim_at_us(1_000_000);
    let mut executor: Executor =
        executor_with_clock_fn(MockSession::new(), tier_slower_cadence_clock::now_us);
    executor.set_spin_nominal_us(1_000);
    for _ in 0..3 {
        executor.spin_once(core::time::Duration::ZERO);
        tier_slower_cadence_clock::advance_ms(10);
    }
    executor.spin_once(core::time::Duration::ZERO);

    let (max_us, late, total) = executor.release_jitter();
    assert_eq!(total, 3, "four spins are three intervals");
    assert_eq!(
        late, 3,
        "every 10 ms interval overruns a declared 1 ms cadence"
    );
    assert_eq!(
        max_us, 9_000,
        "a 10 ms wake against a 1 ms cadence is 9 ms late"
    );
}

/// `spin_period` must actually COUNT its wakes. Before this, `next_us` and
/// `now_us()` were both in hand every cycle and the difference was discarded,
/// so a loop that never met its period looked identical to one that always did.
#[test]
fn spin_period_counts_its_wakes_and_keeps_late_within_total() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let flag = executor.halt_flag();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(60));
        flag.store(true, portable_atomic::Ordering::SeqCst);
    });
    executor
        .spin_period(core::time::Duration::from_millis(2))
        .expect("a clock is installed, so spin_period must run");

    let (max_us, late, total) = executor.release_jitter();
    assert!(total > 0, "the loop ran, so wakes must have been counted");
    assert!(
        late <= total,
        "late wakes ({late}) cannot exceed total wakes ({total})"
    );
    // Deliberately NOT asserting max_us > 0: a host that always meets a 2 ms
    // period is a legitimate outcome, and a test that demanded lateness would
    // fail on the machine behaving best.
    assert!(
        max_us == 0 || late > 0,
        "a non-zero maximum ({max_us} us) with zero late wakes is contradictory"
    );
}

// ===========================================================================
// phase-474 I11 -- the release grid of a `spin_once(period)` loop.
//
// Each test owns a private clock and a park primitive that ADVANCES it: the
// park is where a real port sleeps, so "sleeping" here means moving the clock
// by what the executor asked for (granularity 1 us, no rounding). Work done
// after a release is simulated by moving the clock between spins -- dispatch
// is the tail of `spin_once`, so the executor sees the same thing.
// ===========================================================================

private_test_clock!(grid_idle_clock);
private_test_clock!(spin_once_wakes_clock);
private_test_clock!(grid_skip_clock);
private_test_clock!(grid_early_clock);
private_test_clock!(grid_judge_clock);

/// An idle executor on a 10 ms period is released on `anchor + k * 10 ms`
/// over 1000 spins, to the clock's resolution, although every pass of the
/// caller's loop costs 300 us. Before I11 each park was a whole period from
/// the spin's entry, so every release slid by the loop's own cost.
#[test]
fn an_idle_periodic_spin_releases_on_the_grid() {
    unsafe extern "C" fn park(_c: *mut core::ffi::c_void, us: u64) -> i8 {
        grid_idle_clock::set_us(grid_idle_clock::now_us() + us);
        1
    }
    grid_idle_clock::claim_at_us(1_000_000);
    let mut ex = executor_with_clock_fn(MockSession::new(), grid_idle_clock::now_us);
    ex.set_park_primitive(park, core::ptr::null_mut(), 1);
    let period = core::time::Duration::from_millis(10);
    for k in 1..=1000u64 {
        ex.spin_once(period);
        assert_eq!(
            grid_idle_clock::now_us(),
            1_000_000 + k * 10_000,
            "release {k} must land on the grid, not 10 ms after the previous end"
        );
        // The caller's loop between spins (yield, bookkeeping).
        grid_idle_clock::set_us(grid_idle_clock::now_us() + 300);
    }
    assert_eq!(ex.release_jitter(), (0, 0, 1000), "on the grid: never late");
    assert_eq!(ex.skipped_releases(), 0);
}

/// Work of 1.5 periods after a release: the 20 ms point is due when the loop
/// comes back at 25 ms, so it is released at once, 5 ms late, and the next
/// release is the 30 ms point -- once, no catch-up burst, nothing skipped.
/// Work of 2.5 periods: the 20 ms point is a whole period overdue, so it is
/// dropped and counted (and judged: 15 ms late when dropped, a verdict), the
/// 30 ms point is served once at 35 ms, and the loop resumes at 40 ms.
#[test]
fn an_overrun_releases_once_and_skips_whole_missed_periods() {
    unsafe extern "C" fn park(_c: *mut core::ffi::c_void, us: u64) -> i8 {
        grid_skip_clock::set_us(grid_skip_clock::now_us() + us);
        1
    }
    grid_skip_clock::claim_at_us(0);
    let mut ex = executor_with_clock_fn(MockSession::new(), grid_skip_clock::now_us);
    ex.set_park_primitive(park, core::ptr::null_mut(), 1);
    let period = core::time::Duration::from_millis(10);

    ex.spin_once(period);
    assert_eq!(grid_skip_clock::now_us(), 10_000);
    // 1.5 periods of work after the 10 ms release.
    grid_skip_clock::set_us(25_000);
    ex.spin_once(period);
    assert_eq!(
        grid_skip_clock::now_us(),
        25_000,
        "the due 20 ms point, at once"
    );
    assert_eq!(ex.release_jitter(), (5_000, 1, 2));
    ex.spin_once(period);
    assert_eq!(
        grid_skip_clock::now_us(),
        30_000,
        "then the 30 ms point, not 35"
    );
    assert_eq!(ex.release_jitter(), (5_000, 1, 3));
    assert_eq!(ex.skipped_releases(), 0);
    let mut verdicts = 0;
    ex.drain_violations(|v| {
        if v.rule == "release-jitter-runtime" {
            verdicts += 1;
        }
    });
    assert_eq!(verdicts, 0, "5 ms late is under the one-period tolerance");

    // 2.5 periods of work after the 30 ms release.
    grid_skip_clock::set_us(55_000);
    ex.spin_once(period);
    assert_eq!(
        grid_skip_clock::now_us(),
        55_000,
        "the latest due point (50 ms), once"
    );
    assert_eq!(ex.skipped_releases(), 1, "the 40 ms point was dropped");
    let (max_us, late, total) = ex.release_jitter();
    assert_eq!((late, total), (2, 4), "one release for the two due points");
    assert_eq!(
        max_us, 15_000,
        "the dropped 40 ms point, 15 ms late when dropped"
    );
    let mut measured = None;
    ex.drain_violations(|v| {
        if v.rule == "release-jitter-runtime" {
            measured = Some(v.measured);
        }
    });
    assert_eq!(measured, Some(15_000));
    ex.spin_once(period);
    assert_eq!(grid_skip_clock::now_us(), 60_000, "back on the grid");
    assert_eq!(ex.release_jitter().2, 5);
}

/// Data arriving mid-period breaks the park: that spin runs early, is not a
/// release, and does not move the grid -- the next release is still on it.
#[test]
fn a_data_driven_early_spin_keeps_the_grid() {
    use portable_atomic::{AtomicBool, Ordering};
    static DATA_AT_4MS: AtomicBool = AtomicBool::new(false);
    unsafe extern "C" fn park(_c: *mut core::ffi::c_void, us: u64) -> i8 {
        if DATA_AT_4MS.swap(false, Ordering::SeqCst) {
            grid_early_clock::set_us(grid_early_clock::now_us() + 4_000);
            return 0;
        }
        grid_early_clock::set_us(grid_early_clock::now_us() + us);
        1
    }
    grid_early_clock::claim_at_us(0);
    let mut ex = executor_with_clock_fn(MockSession::new(), grid_early_clock::now_us);
    ex.set_park_primitive(park, core::ptr::null_mut(), 1);
    let period = core::time::Duration::from_millis(10);

    ex.spin_once(period);
    assert_eq!(grid_early_clock::now_us(), 10_000);

    DATA_AT_4MS.store(true, Ordering::SeqCst);
    ex.spin_once(period);
    assert_eq!(
        grid_early_clock::now_us(),
        14_000,
        "the sample broke the park"
    );
    assert_eq!(ex.release_jitter().2, 1, "an early spin is not a release");
    // Its dispatch costs 700 us.
    grid_early_clock::set_us(14_700);

    ex.spin_once(period);
    assert_eq!(
        grid_early_clock::now_us(),
        20_000,
        "the next release is on the grid, not 10 ms after the early spin"
    );
    assert_eq!(ex.release_jitter(), (0, 0, 2));
    assert_eq!(ex.skipped_releases(), 0);
}

/// `release-jitter-runtime` judges release minus scheduled release. 9 ms of
/// work after a release and then a park that wakes 1.5 ms late is 1.5 ms of
/// jitter (the old entry-to-entry rule read 9 ms); a park that oversleeps
/// its grid point by a whole period is a verdict, measured against the grid.
#[test]
fn release_jitter_is_judged_against_the_grid() {
    use portable_atomic::{AtomicU64, Ordering};
    static OVERSLEEP_US: AtomicU64 = AtomicU64::new(0);
    unsafe extern "C" fn park(_c: *mut core::ffi::c_void, us: u64) -> i8 {
        let extra = OVERSLEEP_US.swap(0, Ordering::SeqCst);
        grid_judge_clock::set_us(grid_judge_clock::now_us() + us + extra);
        1
    }
    grid_judge_clock::claim_at_us(0);
    let mut ex = executor_with_clock_fn(MockSession::new(), grid_judge_clock::now_us);
    ex.set_park_primitive(park, core::ptr::null_mut(), 1);
    let period = core::time::Duration::from_millis(10);

    ex.spin_once(period);
    grid_judge_clock::set_us(19_000);
    OVERSLEEP_US.store(1_500, Ordering::SeqCst);
    ex.spin_once(period);
    assert_eq!(grid_judge_clock::now_us(), 21_500);
    assert_eq!(
        ex.release_jitter(),
        (1_500, 1, 2),
        "late by the wake, not by the 9 ms of work before it"
    );
    let mut verdicts = 0;
    ex.drain_violations(|v| {
        if v.rule == "release-jitter-runtime" {
            verdicts += 1;
        }
    });
    assert_eq!(verdicts, 0, "1.5 ms is under the one-period tolerance");

    // The reader holds the CPU: the park for the 30 ms point ends at 42 ms.
    OVERSLEEP_US.store(12_000, Ordering::SeqCst);
    ex.spin_once(period);
    assert_eq!(grid_judge_clock::now_us(), 42_000);
    let mut measured = None;
    ex.drain_violations(|v| {
        if v.rule == "release-jitter-runtime" {
            measured = Some((v.measured, v.declared));
        }
    });
    assert_eq!(
        measured,
        Some((12_000, 10_000)),
        "12 ms past the 30 ms grid point"
    );

    // The 40 ms point was overslept too: skipped at the next entry, and the
    // loop resumes at 50 ms.
    ex.spin_once(period);
    assert_eq!(grid_judge_clock::now_us(), 50_000);
    assert_eq!(ex.skipped_releases(), 1);
}

#[cfg(feature = "sim-time")]
private_test_clock!(node_ros_timer_clock);

/// phase-430 W4 — the clock axis reaches the NODE-LEVEL surface, not just the
/// executor's `register_timer_on_clock`.
///
/// `ros_time_timer_follows_the_simulated_clock` above proves the executor's
/// registrar honours `/clock`; it says nothing about whether a user who writes
/// nodes the way the workspace examples do can ASK for one. Until W4 they could
/// not: `Node`'s only timer verb was `create_timer_in_group`, which lowers to
/// `register_timer_on` with `TimerClockSource::Steady` hardcoded.
///
/// Both halves are asserted in one test for the reason the sibling gives: the
/// ROS-time override is process-global, so two tests would race inside the one
/// binary. `SimTimeGuard` orders this against every other test that touches it.
#[cfg(feature = "sim-time")]
#[test]
fn a_node_level_ros_time_timer_follows_the_simulated_clock() {
    node_ros_timer_clock::claim_at_us(1_000_000);
    let _sim_time = SimTimeGuard::acquire();
    use crate::executor::TimerClockSource;
    use nros_core::clock::Clock;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const MS: i64 = 1_000_000;

    let mut executor: Executor =
        executor_with_clock_fn(MockSession::new(), node_ros_timer_clock::now_us);
    let nid = executor.node_builder("node_level_clock").build().unwrap();

    Clock::set_ros_time_override(10_000 * MS);

    let ros_ticks = std::sync::Arc::new(AtomicUsize::new(0));
    let group_ticks = std::sync::Arc::new(AtomicUsize::new(0));
    let wall_ticks = std::sync::Arc::new(AtomicUsize::new(0));
    let r = ros_ticks.clone();
    let g = group_ticks.clone();
    let w = wall_ticks.clone();

    {
        let mut node = executor.node_mut(nid);
        let telem = node.create_callback_group("telem");
        node.create_timer_on_clock(
            TimerDuration::from_millis(100),
            TimerClockSource::Ros,
            move || {
                r.fetch_add(1, Ordering::SeqCst);
            },
        )
        .unwrap();
        // The group form takes the same axis, and the two axes are independent.
        node.create_timer_on_clock_in_group(
            &telem,
            TimerDuration::from_millis(100),
            TimerClockSource::Ros,
            move || {
                g.fetch_add(1, Ordering::SeqCst);
            },
        )
        .unwrap();
        // The pre-W4 spelling, unchanged: still the wall case.
        node.create_timer_in_group(&telem, TimerDuration::from_millis(100), move || {
            w.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap();
    }

    // 1. STOPPED. Real time passes, simulated time does not.
    for _ in 0..3 {
        let _ = node_ros_timer_clock::elapse_then_spin_once(&mut executor, 120);
    }
    assert_eq!(
        ros_ticks.load(Ordering::SeqCst),
        0,
        "a node-level ROS-time timer fired while the simulated clock was \
         stopped: 360 ms of REAL time passed and /clock did not move"
    );
    assert_eq!(
        group_ticks.load(Ordering::SeqCst),
        0,
        "the callback-group form must take the clock too -- a group is a \
         scheduling choice, not a clock choice"
    );
    // Exact, not `>= 3`: each 120 ms step on the private clock is one fire of
    // the 100 ms wall timer (20, 40, 60 ms carried), and nothing else moves it.
    assert_eq!(
        wall_ticks.load(Ordering::SeqCst),
        3,
        "`create_timer_in_group` is the WALL spelling and must be unaffected by the \
         stopped simulator; fired {} times",
        wall_ticks.load(Ordering::SeqCst)
    );

    // 2. ADVANCING. Four 100 ms steps of simulated time in ~40 ms of real time.
    let wall_before = wall_ticks.load(Ordering::SeqCst);
    for i in 1..=4 {
        Clock::set_ros_time_override(10_000 * MS + i * 100 * MS);
        let _ = node_ros_timer_clock::elapse_then_spin_once(&mut executor, 10);
    }
    assert_eq!(
        ros_ticks.load(Ordering::SeqCst),
        4,
        "four 100 ms steps of simulated time on a 100 ms node-level ROS timer \
         must be four activations"
    );
    assert_eq!(
        group_ticks.load(Ordering::SeqCst),
        4,
        "the group form follows the same clock as the plain one"
    );
    // 60 ms carried + four 10 ms steps = exactly one wall period. On the wall
    // clock this was an UPPER bound (`<= 1`) that a descheduled host broke.
    assert_eq!(
        wall_ticks.load(Ordering::SeqCst) - wall_before,
        1,
        "40 ms of steady time cannot be four activations of a 100 ms wall timer"
    );
}

// =============================================================================
// phase-427 W5 — two nodes in one image, two logger names
// =============================================================================

/// phase-427 W5's RUNTIME acceptance: *two nodes in one image emit records
/// under distinct logger names*.
///
/// The compile probe (`packages/api/nros-cpp/tests/compile/one_node_type.cpp`,
/// `two_nodes_two_logger_names`) pins that `get_logger()` is BUILT from the
/// node's name and that the handle still converts to `rclc_logger_t`. Neither
/// is the claim: a name a caller reads off the accessor says nothing about
/// what a dispatched `Record` carries, and until this landed the two answers
/// disagreed. `Node::logger` — and the C and C++ accessors beside it — called
/// `nros_log::get_logger`, a LOOKUP that answers `DEFAULT_LOGGER` for any name
/// nobody `register_logger`ed. Both nodes below resolved to that one logger,
/// so every record in the image was emitted under the name `"nros"`: the
/// `"nros.compat"` sentinel W5 deleted, reached by a different route.
///
/// Asserted on the RECORDS, through a real `LogSink`, because that is where
/// the claim lives. The nodes are created on ONE executor; the handles do not
/// overlap because `create_node` borrows the executor mutably, but the loggers
/// are `&'static` and both emits happen after both nodes exist.
///
/// Host-only, like every test in this file: capture is a `LogSink` installed
/// with `nros_log::init`, which works on any target, but the buffer here is a
/// `std::sync::Mutex<Vec<_>>`.
#[test]
fn two_nodes_on_one_executor_emit_under_their_own_names() {
    use alloc::{string::String, vec::Vec};
    let _sinks = log_sinks_lock();

    /// One captured record, owned. `(logger name, message)` is the pair the
    /// claim is about: a name with no message cannot be attributed to a node.
    static CAPTURED: std::sync::Mutex<Vec<(String, String)>> = std::sync::Mutex::new(Vec::new());

    struct CapturingSink;
    impl nros_log::LogSink for CapturingSink {
        fn log(&self, record: &nros_log::Record<'_>) {
            // unwrap: poisoned only if another thread already panicked, which
            // would be the failure worth reading rather than this one.
            CAPTURED.lock().unwrap().push((
                alloc::string::ToString::to_string(record.logger_name),
                alloc::string::ToString::to_string(record.message),
            ));
        }
    }
    static SINK: CapturingSink = CapturingSink;
    static SINKS: &[&dyn nros_log::LogSink] = &[&SINK];
    nros_log::init(SINKS);

    // Names unique to this test: the sink is process-global and sibling tests
    // in this binary emit through the catch-all logger while it is installed.
    // The marker filter below makes their EXTRA records harmless. A marker
    // raised while a sibling thread is inside a sink is HELD and delivered by
    // that thread (issue 2000), so it can land after `log_info!` returns — or
    // be LOST, which `emit_until_delivered` handles.
    const TALKER: &str = "w5_two_names_talker";
    const LISTENER: &str = "w5_two_names_listener";
    const TALKER_MARKER: &str = "w5 marker: talker is up";
    const LISTENER_MARKER: &str = "w5 marker: listener is up";

    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);

    let talker_logger = executor
        .create_node(TALKER)
        .expect("create the first node")
        .logger();
    let listener_logger = executor
        .create_node(LISTENER)
        .expect("create the second node")
        .logger();

    assert!(
        !core::ptr::eq(talker_logger, listener_logger),
        "both nodes resolved to ONE logger, named `{}`. That is the shape \
         phase-427 W5 removed: a lookup-only `get_logger` answers \
         `DEFAULT_LOGGER` for every name no `'static` Logger was registered \
         under, so no record can say which node emitted it.",
        talker_logger.name()
    );
    assert_eq!(talker_logger.name(), TALKER);
    assert_eq!(listener_logger.name(), LISTENER);

    // The loggers whose records carried `marker`, one entry per record.
    let names_for = |marker: &str| -> Vec<String> {
        CAPTURED
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, message)| message == marker)
            .map(|(name, _)| name.clone())
            .collect()
    };
    let lost_before = nros_log::early::overflowed();
    let talker_seen = emit_until_delivered(
        || nros_log::log_info!(talker_logger, "{}", TALKER_MARKER),
        || !names_for(TALKER_MARKER).is_empty(),
    );
    let listener_seen = emit_until_delivered(
        || nros_log::log_info!(listener_logger, "{}", LISTENER_MARKER),
        || !names_for(LISTENER_MARKER).is_empty(),
    );
    let talker_names = names_for(TALKER_MARKER);
    let listener_names = names_for(LISTENER_MARKER);
    assert!(
        talker_seen && listener_seen,
        "a marker never reached the sink: talker {talker_names:?}, listener \
         {listener_names:?}; nros_log counted {} lost record(s) meanwhile",
        nros_log::early::overflowed() - lost_before
    );

    // Attribution, per record: a re-raised marker is the same node's record
    // again, so every copy must carry that node's name.
    assert!(
        talker_names.iter().all(|n| n == TALKER),
        "the first node's record must be attributed to the first node: {talker_names:?}"
    );
    assert!(
        listener_names.iter().all(|n| n == LISTENER),
        "the second node's record must be attributed to the second node: {listener_names:?}"
    );
    assert_ne!(
        talker_names[0], listener_names[0],
        "the acceptance criterion itself: two nodes in one image must emit \
         under DISTINCT logger names"
    );
    assert_ne!(
        talker_names[0],
        nros_log::DEFAULT_LOGGER.name(),
        "a record emitted under the catch-all logger names no node"
    );
}

// ===========================================================================
// phase-436 E3 — an armed stack-headroom rule on a port that reports no stack
// data never judges (`check_stack_headroom_rule` skips on 0), so its silence
// reads exactly like a clean result. The executor must say which one it is.
// ===========================================================================

/// Blind exactly when a bound is set AND the port answers 0 — stated against
/// what this host's port actually answers, so the test holds on any host.
#[test]
fn a_headroom_bound_on_a_port_without_stack_data_is_reported_blind() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    executor.set_min_stack_headroom_bytes(1024);
    assert_eq!(
        executor.stack_headroom_blind(),
        nros_platform_api::stack_unused_bytes_upto(1) == 0,
        "blind must mean exactly: a bound is set and the port reports no stack data"
    );
}

/// A rule that is off cannot be blind, including one switched off again.
#[test]
fn no_headroom_bound_is_never_blind() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    assert!(
        !executor.stack_headroom_blind(),
        "a fresh executor sets no bound"
    );
    executor.set_min_stack_headroom_bytes(1024);
    executor.set_min_stack_headroom_bytes(0);
    assert!(
        !executor.stack_headroom_blind(),
        "removing the bound clears blindness"
    );
}

// ===========================================================================
// phase-436 E4 — on a painted Zephyr stack the headroom query is a byte scan
// over every UNUSED byte (`z_stack_space_get`), and the rule used to run it on
// every spin. ASI's control tier runs on a 512 KiB main stack every 5 ms. The
// high-water mark only grows, so a throttled check sees the same low; it only
// sees it later.
// ===========================================================================

private_test_clock!(headroom_throttle_clock);

#[test]
fn the_headroom_query_runs_at_most_once_per_interval() {
    headroom_throttle_clock::claim_at_us(1_000_000);
    let mut executor = executor_with_clock_fn(MockSession::new(), headroom_throttle_clock::now_us);
    executor.set_min_stack_headroom_bytes(1024);
    for _ in 0..50 {
        executor.spin_once(core::time::Duration::ZERO);
    }
    assert_eq!(
        executor.worker.stack_headroom_checks, 1,
        "fifty spins inside one interval must query the port once, not fifty times"
    );
    headroom_throttle_clock::set_us(
        1_000_000 + crate::executor::monitor::STACK_HEADROOM_CHECK_INTERVAL_US,
    );
    executor.spin_once(core::time::Duration::ZERO);
    assert_eq!(
        executor.worker.stack_headroom_checks, 2,
        "one full interval later it queries again"
    );
}

// ===========================================================================
// phase-444 — the graph family on upstream's receiver.
//
// `Node` (which the api-parity correlator sees as `Node`, the receiver
// rclrs hangs these off) forwards each graph verb to the executor it already
// borrows. The capability is phase-381 W4's and is tested where it lives;
// what these tests own is the FORWARDING, and the bug a forwarder actually
// has is reaching the WRONG SLOT — `count_subscriptions` wired to
// `count_publishers`, a `by_node` walk passing the wrong `GraphEntityKind`,
// or the node name and namespace swapped. A canned graph that answered every
// slot alike could not fail on any of those, so `MockSession::with_graph`
// answers each one distinguishably and these assert the mapping.
// ===========================================================================

/// Collect a `(name, types)` walk into something comparable.
fn collect_names_and_types(
    walk: impl FnOnce(&mut dyn FnMut(&str, &[&str]) -> bool) -> Result<(), NodeError>,
) -> std::vec::Vec<(std::string::String, std::vec::Vec<std::string::String>)> {
    use crate::alloc::string::ToString;
    let mut seen = std::vec::Vec::new();
    walk(&mut |name, types| {
        seen.push((
            name.to_string(),
            types.iter().map(|t| t.to_string()).collect(),
        ));
        true
    })
    .expect("the canned graph answers this slot");
    seen
}

#[test]
fn node_ctx_forwards_each_graph_verb_to_its_own_slot() {
    use crate::{alloc::string::ToString, mock::canned};

    let mut executor: Executor = executor_with_clock(MockSession::with_graph());
    let nid = executor.node_builder("grapher").build().unwrap();

    // --- get_node_names: namespace, and NOT the enclave ---
    let mut names = std::vec::Vec::new();
    executor
        .node_mut(nid)
        .get_node_names(&mut |name, ns| {
            names.push((name.to_string(), ns.to_string()));
            true
        })
        .expect("the canned graph answers get_node_names");
    let expect_names: std::vec::Vec<_> = canned::NODES
        .iter()
        .map(|(n, ns, _)| (n.to_string(), ns.to_string()))
        .collect();
    assert_eq!(
        names, expect_names,
        "get_node_names must visit every node with its namespace"
    );

    // --- get_node_names_with_enclaves: the SAME nodes, plus the enclave.
    // One node's enclave is `None` on purpose: that is a partial answer the
    // contract admits, not an error, and a forwarder that filtered it would
    // drop a node the graph reported.
    let mut with_enclaves = std::vec::Vec::new();
    executor
        .node_mut(nid)
        .get_node_names_with_enclaves(&mut |name, ns, enclave| {
            with_enclaves.push((
                name.to_string(),
                ns.to_string(),
                enclave.map(|e| e.to_string()),
            ));
            true
        })
        .expect("the canned graph answers get_node_names_with_enclaves");
    let expect_enclaves: std::vec::Vec<_> = canned::NODES
        .iter()
        .map(|(n, ns, e)| (n.to_string(), ns.to_string(), e.map(|e| e.to_string())))
        .collect();
    assert_eq!(
        with_enclaves, expect_enclaves,
        "the enclave form must report the same node set, with enclaves"
    );
    assert!(
        expect_enclaves.iter().any(|(_, _, e)| e.is_none()),
        "the canned graph must keep a None enclave, or this asserts nothing"
    );

    // --- topics: ONE visit per topic, carrying BOTH its types ---
    let topics = collect_names_and_types(|v| executor.node_mut(nid).get_topic_names_and_types(v));
    let (topic, topic_types) = canned::TOPIC;
    assert_eq!(
        topics,
        std::vec![(
            topic.to_string(),
            topic_types
                .iter()
                .map(|t| t.to_string())
                .collect::<std::vec::Vec<_>>()
        )],
        "a topic carrying two types is one visit with two entries, not two visits"
    );

    // --- services: a DIFFERENT slot from topics ---
    let services =
        collect_names_and_types(|v| executor.node_mut(nid).get_service_names_and_types(v));
    let (svc, svc_types) = canned::SERVICE;
    assert_eq!(
        services,
        std::vec![(
            svc.to_string(),
            svc_types
                .iter()
                .map(|t| t.to_string())
                .collect::<std::vec::Vec<_>>()
        )],
        "get_service_names_and_types must reach the service slot, not the topic one"
    );

    // --- the two counters, which differ so a swap cannot pass ---
    assert_eq!(
        executor.node_mut(nid).count_publishers("/chatter").unwrap(),
        canned::PUBLISHERS,
        "count_publishers must reach the publisher counter"
    );
    assert_eq!(
        executor
            .node_mut(nid)
            .count_subscriptions("/chatter")
            .unwrap(),
        canned::SUBSCRIBERS,
        "count_subscriptions is rclrs's spelling of the SUBSCRIBER counter — \
         a forward to count_publishers reads the other number"
    );
    // Issue 1978 — the Jazzy service twins, each reaching its own counter.
    assert_eq!(
        executor
            .node_mut(nid)
            .count_clients("/add_two_ints")
            .unwrap(),
        canned::CLIENTS,
        "count_clients must reach the client counter"
    );
    assert_eq!(
        executor
            .node_mut(nid)
            .count_services("/add_two_ints")
            .unwrap(),
        canned::SERVICES,
        "count_services must reach the service counter, not the client one"
    );

    // --- the four by_node walks: right kind, and the two names in the right
    // order. The mock echoes `node_name` then `node_namespace` after its kind
    // marker, so a swapped pair fails here and nowhere else.
    //
    // Spelled out per verb rather than driven from a table of fn pointers:
    // the four signatures are higher-ranked over the visitor's lifetimes and
    // a `dyn Fn` table cannot hold them without pinning `Node`'s own two.
    macro_rules! assert_by_node {
        ($verb:ident, $marker:expr) => {{
            let seen =
                collect_names_and_types(|v| executor.node_mut(nid).$verb("talker", "/demo", v));
            let got: std::vec::Vec<&str> = seen.iter().map(|(n, _)| n.as_str()).collect();
            assert_eq!(
                got,
                std::vec![$marker, "talker", "/demo"],
                concat!(
                    stringify!($verb),
                    " must pass its own entity kind, and the node name before \
                     the namespace"
                )
            );
        }};
    }
    assert_by_node!(
        get_publisher_names_and_types_by_node,
        canned::BY_NODE_PUBLISHER
    );
    assert_by_node!(
        get_subscription_names_and_types_by_node,
        canned::BY_NODE_SUBSCRIBER
    );
    assert_by_node!(get_service_names_and_types_by_node, canned::BY_NODE_SERVICE);
    assert_by_node!(get_client_names_and_types_by_node, canned::BY_NODE_CLIENT);
}

#[test]
fn node_ctx_graph_reports_unsupported_rather_than_an_empty_graph() {
    // RFC-0036: a backend that CANNOT tell you (XRCE) is a different answer
    // from one that has seen nobody yet. The plain `MockSession` is such a
    // backend — every graph slot keeps the trait default — so this pins the
    // distinction the whole contract rests on: `Err`, and NOT a walk that
    // visits nothing and returns `Ok`. Collapsing the two is the failure
    // mode issue 1087 already cost us one entity kind over.
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let nid = executor.node_builder("no_graph").build().unwrap();

    let mut visited = 0usize;
    let ret = executor.node_mut(nid).get_node_names(&mut |_, _| {
        visited += 1;
        true
    });
    assert!(
        matches!(
            ret,
            Err(NodeError::Transport(nros_rmw::TransportError::Unsupported))
        ),
        "a backend with no graph must say Unsupported, not report an empty graph: {ret:?}"
    );
    assert_eq!(
        visited, 0,
        "an Unsupported answer must visit nothing — a zero-visit Ok would be \
         indistinguishable from a graph nobody has joined"
    );

    assert!(
        matches!(
            executor.node_mut(nid).count_publishers("/chatter"),
            Err(NodeError::Transport(nros_rmw::TransportError::Unsupported))
        ),
        "a count from a backend with no graph must not read as 0"
    );
    assert!(
        matches!(
            executor.node_mut(nid).count_subscriptions("/chatter"),
            Err(NodeError::Transport(nros_rmw::TransportError::Unsupported))
        ),
        "a count from a backend with no graph must not read as 0"
    );
    // Issue 1978 — the service twins keep the same distinction.
    for ret in [
        executor.node_mut(nid).count_clients("/add_two_ints"),
        executor.node_mut(nid).count_services("/add_two_ints"),
    ] {
        assert!(
            matches!(
                ret,
                Err(NodeError::Transport(nros_rmw::TransportError::Unsupported))
            ),
            "a service count from a backend with no graph must not read as 0: {ret:?}"
        );
    }
}

/// phase-444 — every entity handle reads back the name it was created on, and
/// reads back ITS OWN.
///
/// Eight handles, eight DISTINCT names, on purpose: the bug an accessor of this
/// shape actually has is reaching the wrong handle, and a fixture where two
/// entities share a name cannot see it. Each assertion fails on its own if the
/// forward is cross-wired.
#[test]
fn every_entity_handle_reads_back_its_own_name() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let mut node = executor.create_node("names").expect("node");

    let publisher = node
        .create_publisher::<TestMsg>("/typed_pub")
        .expect("typed publisher");
    let subscription = node
        .create_polling_subscription::<TestMsg>("/typed_sub")
        .expect("typed subscription");
    let raw_publisher = node
        .create_publisher_raw("/raw_pub", "test/msg/TestMsg", "test_hash")
        .expect("raw publisher");
    let raw_subscription = node
        .create_polling_subscription_raw("/raw_sub", "test/msg/TestMsg", "test_hash")
        .expect("raw subscription");
    let server = node
        .create_polling_service::<TestService>("/typed_server")
        .expect("typed service server");
    let client = node
        .create_client::<TestService>("/typed_client")
        .expect("typed service client");
    let raw_server = node
        .create_polling_service_raw("/raw_server", "test/srv/TestService", "test_hash")
        .expect("raw service server");
    let raw_client = node
        .create_client_raw("/raw_client", "test/srv/TestService", "test_hash")
        .expect("raw service client");

    assert_eq!(publisher.topic_name(), "/typed_pub");
    assert_eq!(subscription.topic_name(), "/typed_sub");
    assert_eq!(raw_publisher.topic_name(), "/raw_pub");
    assert_eq!(raw_subscription.topic_name(), "/raw_sub");
    assert_eq!(server.service_name(), "/typed_server");
    assert_eq!(client.service_name(), "/typed_client");
    assert_eq!(raw_server.service_name(), "/raw_server");
    assert_eq!(raw_client.service_name(), "/raw_client");
}

/// phase-515 R5 — `publish` takes the message borrowed or owned, as rclrs's
/// `publish(impl MessageCow)` does, and both put the same bytes on the wire.
#[test]
fn publish_takes_the_message_borrowed_or_owned() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let mut node = executor.create_node("cow").expect("node");
    let publisher = node.create_publisher::<TestMsg>("/cow").expect("publisher");

    let msg = TestMsg { data: 42 };
    publisher.publish(&msg).expect("borrowed publish");
    let borrowed = publisher.handle.last_published().expect("one sample");
    publisher.publish(msg).expect("owned publish");
    let owned = publisher.handle.last_published().expect("two samples");

    assert_eq!(publisher.handle.publish_count(), 2);
    assert_eq!(borrowed.1, owned.1, "same length either way");
    assert_eq!(
        &borrowed.0[..borrowed.1],
        &owned.0[..owned.1],
        "same bytes either way"
    );
}

/// phase-515 R1 — the node's create sites take rclrs's
/// `impl IntoPrimitiveOptions`: a bare name keeps the entity kind's default
/// profile, and chained steps reach the backend's create call.
#[test]
fn create_sites_take_rclrs_primitive_options() {
    use crate::IntoPrimitiveOptions;
    use nros_rmw::{QoSDurabilityPolicy, QoSReliabilityPolicy};
    let mut executor: Executor = executor_with_clock(MockSession::new());
    {
        let mut node = executor.create_node("opts").expect("node");
        let _p = node
            .create_publisher::<TestMsg>("/opts_pub".keep_last(3).best_effort().transient_local())
            .expect("publisher");
        let _plain = node
            .create_publisher::<TestMsg>("/opts_plain")
            .expect("bare name");
        let name = alloc::string::String::from("/opts_sub");
        node.create_subscription::<TestMsg, _>(&name, |_m: &TestMsg| {})
            .expect("a borrowed String names an entity");
        node.create_service::<TestService, _>("/opts_srv".keep_last(5), |_r| TestServiceReply {
            sum: 0,
        })
        .expect("service");
        let _c = node
            .create_client::<TestService>("/opts_cli".best_effort())
            .expect("client");
    }
    let session = executor.session();
    let qos = |n: &str| {
        session
            .created_qos(n)
            .unwrap_or_else(|| panic!("{n} created"))
    };

    let p = qos("/opts_pub");
    assert_eq!(
        (p.depth, p.reliability),
        (3, QoSReliabilityPolicy::BestEffort)
    );
    assert_eq!(p.durability, QoSDurabilityPolicy::TransientLocal);
    assert_eq!(
        qos("/opts_plain"),
        QoSProfile::default(),
        "a bare name keeps the default"
    );
    assert_eq!(qos("/opts_sub"), QoSProfile::default());

    let srv = qos("/opts_srv");
    let services = QoSProfile::services_default();
    assert_eq!(srv.depth, 5);
    assert_eq!(
        srv.reliability, services.reliability,
        "unnamed policies keep the services profile"
    );
    let cli = qos("/opts_cli");
    assert_eq!(cli.reliability, QoSReliabilityPolicy::BestEffort);
    assert_eq!(cli.depth, services.depth);
}

/// phase-444 — the matched-count pair reaches ITS OWN graph counter.
///
/// `MockSession::with_graph` answers `count_publishers` and `count_subscribers`
/// with DIFFERENT numbers on purpose, so a publisher wired to the publisher
/// counter (or a subscription to the subscriber one) fails here rather than
/// at a user's build.
#[test]
fn the_matched_counts_reach_their_own_graph_counter() {
    let mut executor: Executor = executor_with_clock(MockSession::with_graph());
    let (publisher, subscription) = {
        let mut node = executor.create_node("counts").expect("node");
        let publisher = node
            .create_publisher::<TestMsg>("/counted")
            .expect("publisher");
        let subscription = node
            .create_polling_subscription::<TestMsg>("/counted")
            .expect("subscription");
        (publisher, subscription)
    };

    assert_eq!(
        publisher.get_subscription_count(&mut executor).unwrap(),
        crate::mock::canned::SUBSCRIBERS,
        "a publisher counts SUBSCRIBERS on its topic, not publishers"
    );
    assert_eq!(
        subscription.get_publisher_count(&mut executor).unwrap(),
        crate::mock::canned::PUBLISHERS,
        "a subscription counts PUBLISHERS on its topic, not subscribers"
    );
}

/// Issue 1643 — a backend that fills the matched-count slots is ASKED, and
/// its answer wins over the topic-wide graph count.
///
/// The graph says 7 subscribers and 3 publishers; the backend says 2 and 5
/// MATCHED. Upstream's `get_subscription_count` is the matched count, so a
/// forwarder that still reads the graph first fails here. The matched counts
/// are also distinct from each other and from the graph's, so a publisher
/// wired to the subscription's slot fails too.
#[test]
fn a_backend_matched_count_wins_over_the_graph_count() {
    let mut executor: Executor = executor_with_clock(MockSession::with_graph());
    let (publisher, subscription) = {
        let mut node = executor.create_node("counts").expect("node");
        let publisher = node
            .create_publisher::<TestMsg>("/counted")
            .expect("publisher");
        let subscription = node
            .create_polling_subscription::<TestMsg>("/counted")
            .expect("subscription");
        (publisher, subscription)
    };
    publisher.handle.matched.set(Some(2));
    subscription.handle.matched.set(Some(5));

    assert_eq!(publisher.get_subscription_count(&mut executor).unwrap(), 2);
    assert_eq!(subscription.get_publisher_count(&mut executor).unwrap(), 5);

    // A matched count of ZERO is an answer, not an absence: it must not fall
    // through to the graph's 7 (an incompatible peer is on the topic).
    publisher.handle.matched.set(Some(0));
    assert_eq!(publisher.get_subscription_count(&mut executor).unwrap(), 0);
}

/// Issue 1972 — the matched and incompatible-type callbacks reach the
/// application with the backend's payload, each through its OWN kind.
///
/// The mock backend fires through the same `(kind, payload, ctx)` channel a
/// real one uses. The payloads are distinct per entity and per kind, so a
/// trampoline that decoded the wrong union arm, or a publisher wired to the
/// subscription's kind, reads a different number and fails.
#[test]
fn matched_and_incompatible_type_callbacks_reach_the_application() {
    use std::{
        sync::{Arc, Mutex},
        vec,
        vec::Vec,
    };
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let (mut publisher, mut subscription) = {
        let mut node = executor.create_node("matched").expect("node");
        let publisher = node
            .create_publisher::<TestMsg>("/matched")
            .expect("publisher");
        let subscription = node
            .create_polling_subscription::<TestMsg>("/matched")
            .expect("subscription");
        (publisher, subscription)
    };

    let pub_seen: Arc<Mutex<Vec<nros_rmw::MatchedStatus>>> = Arc::default();
    let sub_seen: Arc<Mutex<Vec<nros_rmw::MatchedStatus>>> = Arc::default();
    let pub_incompat: Arc<Mutex<Vec<nros_rmw::IncompatibleTypeStatus>>> = Arc::default();
    let sub_incompat: Arc<Mutex<Vec<nros_rmw::IncompatibleTypeStatus>>> = Arc::default();

    assert!(publisher.supports_event(nros_rmw::EventKind::PublicationMatched));
    assert!(subscription.supports_event(nros_rmw::EventKind::SubscriptionMatched));
    {
        let v = pub_seen.clone();
        publisher
            .on_matched(move |s| v.lock().unwrap().push(s))
            .expect("publisher on_matched");
        let v = pub_incompat.clone();
        publisher
            .on_incompatible_type(move |s| v.lock().unwrap().push(s))
            .expect("publisher on_incompatible_type");
        let v = sub_seen.clone();
        subscription
            .on_matched(move |s| v.lock().unwrap().push(s))
            .expect("subscription on_matched");
        let v = sub_incompat.clone();
        subscription
            .on_incompatible_type(move |s| v.lock().unwrap().push(s))
            .expect("subscription on_incompatible_type");
    }

    let appeared = nros_rmw::MatchedStatus {
        total_count: 1,
        total_count_change: 1,
        current_count: 1,
        current_count_change: 1,
    };
    let left = nros_rmw::MatchedStatus {
        total_count: 1,
        total_count_change: 0,
        current_count: 0,
        current_count_change: -1,
    };
    let sub_appeared = nros_rmw::MatchedStatus {
        total_count: 3,
        total_count_change: 3,
        current_count: 3,
        current_count_change: 3,
    };
    assert!(
        publisher
            .handle
            .events
            .fire(nros_rmw::EventKind::PublicationMatched, &appeared)
    );
    assert!(
        publisher
            .handle
            .events
            .fire(nros_rmw::EventKind::PublicationMatched, &left)
    );
    assert!(
        subscription
            .handle
            .events
            .fire(nros_rmw::EventKind::SubscriptionMatched, &sub_appeared)
    );
    let it = nros_rmw::IncompatibleTypeStatus {
        total_count: 2,
        total_count_change: 2,
    };
    let it_sub = nros_rmw::IncompatibleTypeStatus {
        total_count: 9,
        total_count_change: 1,
    };
    assert!(
        publisher
            .handle
            .events
            .fire(nros_rmw::EventKind::PublisherIncompatibleType, &it)
    );
    assert!(
        subscription
            .handle
            .events
            .fire(nros_rmw::EventKind::SubscriptionIncompatibleType, &it_sub)
    );

    assert_eq!(*pub_seen.lock().unwrap(), vec![appeared, left]);
    assert_eq!(*sub_seen.lock().unwrap(), vec![sub_appeared]);
    assert_eq!(*pub_incompat.lock().unwrap(), vec![it]);
    assert_eq!(*sub_incompat.lock().unwrap(), vec![it_sub]);
}

/// phase-444 — a backend with no graph answers `Unsupported`, and that is a
/// DIFFERENT answer from `0`.
///
/// The plain `MockSession` keeps the trait default on every graph slot, which
/// is what XRCE reports. Collapsing it into a count is the failure this pins:
/// `0` means "nobody is listening, skip the work" and a caller that acts on a
/// fabricated `0` stops publishing on a backend that simply cannot tell.
#[test]
fn a_backend_with_no_graph_does_not_report_a_count_of_zero() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let (publisher, subscription) = {
        let mut node = executor.create_node("counts").expect("node");
        let publisher = node
            .create_publisher::<TestMsg>("/counted")
            .expect("publisher");
        let subscription = node
            .create_polling_subscription::<TestMsg>("/counted")
            .expect("subscription");
        (publisher, subscription)
    };

    assert!(
        matches!(
            publisher.get_subscription_count(&mut executor),
            Err(NodeError::Transport(nros_rmw::TransportError::Unsupported))
        ),
        "a count from a backend with no graph must not read as 0"
    );
    assert!(
        matches!(
            subscription.get_publisher_count(&mut executor),
            Err(NodeError::Transport(nros_rmw::TransportError::Unsupported))
        ),
        "a count from a backend with no graph must not read as 0"
    );
}

/// phase-444 — `Node`'s two `*_info_by_topic` forwarders reach their OWN
/// side of the backend slot.
///
/// One backend slot serves both, discriminated by a single `bool`, so the
/// defect a forwarder of this shape has is passing the wrong one — and it is
/// invisible unless the two sides answer differently. `MockSession::with_graph`
/// makes them differ in node name, type AND the `is_publisher` flag, so each
/// assertion fails on its own.
#[test]
fn the_endpoint_info_forwarders_reach_their_own_side() {
    use crate::mock::canned;

    let mut executor: Executor = executor_with_clock(MockSession::with_graph());
    let nid = executor.node_builder("endpoints").build().unwrap();

    let mut visits = 0usize;
    executor
        .node_mut(nid)
        .get_publishers_info_by_topic("/chatter", &mut |info| {
            visits += 1;
            assert_eq!(
                info.node_name,
                canned::ENDPOINT_PUBLISHER_NODE,
                "get_publishers_info_by_topic must reach the PUBLISHER side of the slot"
            );
            assert_eq!(info.topic_type, canned::ENDPOINT_PUBLISHER_TYPE);
            assert!(info.is_publisher, "the publisher side must say so");
            assert_eq!(
                info.node_namespace, "/chatter",
                "the topic must be forwarded"
            );
            true
        })
        .expect("publisher side answers");
    assert_eq!(
        visits, 1,
        "the canned graph reports exactly one endpoint per side"
    );

    visits = 0;
    executor
        .node_mut(nid)
        .get_subscriptions_info_by_topic("/chatter", &mut |info| {
            visits += 1;
            assert_eq!(
                info.node_name,
                canned::ENDPOINT_SUBSCRIBER_NODE,
                "get_subscriptions_info_by_topic must reach the SUBSCRIPTION side of the slot"
            );
            assert_eq!(info.topic_type, canned::ENDPOINT_SUBSCRIBER_TYPE);
            assert!(!info.is_publisher, "the subscription side must say so");
            true
        })
        .expect("subscription side answers");
    assert_eq!(
        visits, 1,
        "the canned graph reports exactly one endpoint per side"
    );
}

/// phase-444 — and the same backend with no graph answers `Unsupported` here
/// too, rather than walking nothing and returning `Ok`.
///
/// A zero-visit `Ok` is indistinguishable from a topic nobody has joined,
/// which is the collapse issue 1087 already cost one entity kind over.
#[test]
fn the_endpoint_info_forwarders_do_not_collapse_unsupported_into_empty() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let nid = executor.node_builder("no_graph").build().unwrap();

    let mut visited = 0usize;
    let ret = executor
        .node_mut(nid)
        .get_publishers_info_by_topic("/chatter", &mut |_| {
            visited += 1;
            true
        });
    assert!(
        matches!(
            ret,
            Err(NodeError::Transport(nros_rmw::TransportError::Unsupported))
        ),
        "a backend with no graph must say Unsupported, not report an empty topic: {ret:?}"
    );
    assert_eq!(visited, 0, "an Unsupported answer must visit nothing");

    assert!(
        matches!(
            executor
                .node_mut(nid)
                .get_subscriptions_info_by_topic("/chatter", &mut |_| true),
            Err(NodeError::Transport(nros_rmw::TransportError::Unsupported))
        ),
        "the subscription side must say Unsupported too"
    );
}

/// Issue 1473 — "the caller named no namespace" and "the caller asked for the
/// root" are DIFFERENT requests, and `NodeBuilder` is where the difference is
/// decided for every language.
///
/// This is the half of 1473 that was already correct and had no test, so
/// nothing stopped a fix at the C++ FFI from being applied here instead — the
/// two C++ entry points disagreed precisely because one of them collapsed the
/// distinction one layer up. MEASURED at this builder on 2026-09-25, before
/// any change: `.build()` alone recorded `/island`, `.namespace("/")` recorded
/// `/`. Those are the two answers `nros_cpp_node_create_ex` and
/// `nros_cpp_node_create` were handing the SAME stated input.
///
/// The `""` arm is here because it is the one spelling a C caller can reach by
/// accident (an unresolved bake macro expands to it). It is recorded verbatim
/// and normalises to the root downstream — see
/// `an_empty_namespace_normalises_to_root` — which is why the FFI edge treats
/// it as UNSET rather than passing it through.
#[test]
fn an_unnamed_namespace_inherits_the_executors_and_an_explicit_root_does_not() {
    let cfg = ExecutorConfig::default()
        .namespace("/island")
        .clock_us(test_clock_us);
    let mut exec: Executor = Executor::from_session_with(MockSession::new(), &cfg);
    // What `Executor::open_with_rmw` does with `config.namespace`; the
    // `from_session*` constructors take a session that is already open, so they
    // never see the config's identity.
    exec.set_node_identity("session", "/island");

    let unset = exec.node_builder("unset").build().unwrap();
    assert_eq!(
        exec.node(unset).unwrap().namespace.as_str(),
        "/island",
        "a node whose builder was never told a namespace must land in the \
         executor's — that is what makes `nros::init(.., node_namespace)` \
         (issue 1434) reach anything the image creates afterwards"
    );

    let root = exec.node_builder("root").namespace("/").build().unwrap();
    assert_eq!(
        exec.node(root).unwrap().namespace.as_str(),
        "/",
        "and a node that ASKED for the root must get the root, on the same \
         executor — if these two agreed there would be no way to place a node \
         outside a namespaced executor at all"
    );

    let empty = exec.node_builder("empty").namespace("").build().unwrap();
    assert_eq!(
        exec.node(empty).unwrap().namespace.as_str(),
        "",
        "the builder records `\"\"` verbatim; normalising it to the root is \
         `nros_node::names`' job and treating it as UNSET is the FFI edge's"
    );
}

// ====================================================================
// Issue 1496 — RELEASING an arena entry, measured
// ====================================================================

fn register_raw_server(
    executor: &mut Executor<'_>,
) -> Result<super::action::ActionServerRawHandle, NodeError> {
    executor.register_action_server_raw(super::action::RawActionServerSpec {
        node_id: None,
        action_name: "/verb",
        type_name: "test_msgs/action/Fibonacci",
        type_hash: "RIHS01_0000",
        hashes: nros_rmw::ActionEndpointHashes::uniform("RIHS01_0000"),
        qos: nros_rmw::QoSProfile::services_default(),
        goal_callback: accept_and_record_context,
        cancel_callback: refuse_every_cancel,
        accepted_callback: None,
        context: core::ptr::null_mut(),
    })
}

fn register_raw_client(executor: &mut Executor<'_>) -> Result<usize, NodeError> {
    executor
        .register_action_client_raw(super::action::RawActionClientSpec {
            node_id: None,
            action_name: "/verb",
            type_name: "test_msgs/action/Fibonacci",
            type_hash: "RIHS01_0000",
            hashes: nros_rmw::ActionEndpointHashes::uniform("RIHS01_0000"),
            goal_response_callback: Some(no_op_goal_response),
            feedback_callback: None,
            result_callback: None,
            cancel_response_callback: None,
            context: core::ptr::null_mut(),
        })
        .map(|h| h.entry_index())
}

/// The acceptance the issue names: create and destroy an action SERVER many
/// more times than the executor has slots or arena for, and never run out.
/// The arena's high-water mark is set by the FIRST registration and does not
/// move again, which is the measurement that the bytes are reused rather than
/// merely not yet exhausted.
#[test]
fn an_action_server_created_and_released_in_a_loop_never_exhausts() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let first = register_raw_server(&mut executor).expect("first registration");
    let high_water = executor.arena_used();
    assert!(high_water > 0);
    assert!(unsafe { executor.release_action_server_raw(&first) });
    assert!(
        executor.entries[first.entry_index].is_none(),
        "the slot is free"
    );
    assert_eq!(
        executor.arena_used(),
        0,
        "and its bytes went back to the bump pointer (it was the top region)"
    );

    const N: usize = 200;
    for i in 0..N {
        let h = register_raw_server(&mut executor)
            .unwrap_or_else(|e| panic!("iteration {i}: registration failed with {e:?}"));
        assert_eq!(
            executor.arena_used(),
            high_water,
            "iteration {i}: the arena grew; the released region was not reused"
        );
        assert!(unsafe { executor.release_action_server_raw(&h) });
    }
    assert!(
        N > crate::config::MAX_CBS && N * high_water > crate::config::ARENA_SIZE,
        "precondition: the loop out-runs both the slot table and the arena"
    );
}

/// The negative control: WITHOUT the release the same loop fails, inside the
/// slot table. Otherwise the test above could pass on an executor that never
/// ran out in the first place.
#[test]
fn without_the_release_the_same_loop_exhausts() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let mut failed = None;
    for i in 0..64 {
        if let Err(e) = register_raw_server(&mut executor) {
            failed = Some((i, e));
            break;
        }
    }
    let (i, e) = failed.expect("an executor that never runs out proves nothing");
    assert!(
        matches!(e, NodeError::ExecutorFull | NodeError::BufferTooSmall),
        "iteration {i}: {e:?}"
    );
}

#[test]
fn an_action_client_created_and_released_in_a_loop_never_exhausts() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let first = register_raw_client(&mut executor).expect("first registration");
    let high_water = executor.arena_used();
    assert!(unsafe { executor.release_action_client_raw(first) });
    for i in 0..200 {
        let idx =
            register_raw_client(&mut executor).unwrap_or_else(|e| panic!("iteration {i}: {e:?}"));
        assert_eq!(executor.arena_used(), high_water, "iteration {i}");
        assert!(unsafe { executor.release_action_client_raw(idx) });
    }
}

/// A release names its kind: an action-server release of a CLIENT slot (or of
/// an empty one) refuses and touches nothing.
#[test]
fn a_release_of_the_wrong_kind_or_an_empty_slot_is_refused() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let idx = register_raw_client(&mut executor).expect("client");
    let used = executor.arena_used();
    assert!(!unsafe {
        executor.release_action_server_raw_sized::<
            { crate::config::DEFAULT_RX_BUF_SIZE },
            { crate::config::DEFAULT_RX_BUF_SIZE },
            { crate::config::DEFAULT_RX_BUF_SIZE },
            4,
        >(idx)
    });
    assert!(executor.entries[idx].is_some());
    assert_eq!(executor.arena_released(), 0);
    assert!(unsafe { executor.release_action_client_raw(idx) });
    assert!(
        !unsafe { executor.release_action_client_raw(idx) },
        "twice is refused"
    );
    assert!(used > 0);
    assert_eq!(
        executor.arena_used(),
        0,
        "the only entry's bytes are all back"
    );
}

/// A released hole is reused by a SMALLER entry too, and two adjacent
/// releases coalesce into one hole the larger entry fits again.
#[test]
fn released_regions_split_and_coalesce() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let a = register_raw_client(&mut executor).expect("a");
    let b = register_raw_client(&mut executor).expect("b");
    // Issue 1667 — a hole at the TOP goes back to the bump pointer, so pin the
    // top with a third entry to keep the two holes interior.
    executor
        .register_timer(TimerDuration::from_millis(1000), || {})
        .expect("pin");
    let high_water = executor.arena_used();
    assert!(unsafe { executor.release_action_client_raw(a) });
    assert!(unsafe { executor.release_action_client_raw(b) });
    assert_eq!(executor.arena_holes(), 1, "adjacent holes coalesce");
    // A timer is far smaller than a client; it takes the front of the hole.
    executor
        .register_timer(TimerDuration::from_millis(1000), || {})
        .expect("timer");
    assert_eq!(
        executor.arena_used(),
        high_water,
        "the timer came from the hole"
    );
    // And a client still fits in what is left.
    register_raw_client(&mut executor).expect("client after split");
    assert_eq!(executor.arena_used(), high_water);
}

// ====================================================================
// Issue 1631 — every entry kind is releasable, by its RECORDED length
// ====================================================================

unsafe extern "C" fn release_probe_sub(_d: *const u8, _l: usize, _c: *mut core::ffi::c_void) {}

unsafe extern "C" fn release_probe_srv(
    _req: *const u8,
    _req_len: usize,
    _resp: *mut u8,
    _resp_cap: usize,
    _resp_len: *mut usize,
    _ctx: *mut core::ffi::c_void,
) -> bool {
    false
}

unsafe extern "C" fn release_probe_reply(_d: *const u8, _l: usize, _c: *mut core::ffi::c_void) {}

/// A BUFFERED subscription: depth 4 and a stated receive size, so the entry
/// carries a runtime-sized trailing ring. 1496's caller-stated size could not
/// release this one — no caller can name its length.
fn register_buffered_sub(executor: &mut Executor<'_>) -> Result<HandleId, NodeError> {
    executor.add_arena_subscription_c_callback::<{ crate::config::DEFAULT_RX_BUF_SIZE }>(
        None,
        "/release_probe",
        "test/msg/TestMsg",
        "test_hash",
        QoSProfile::default().keep_last(4),
        release_probe_sub,
        core::ptr::null_mut(),
        None,
        96,
    )
}

/// The four kinds 1631 adds a remover for, each in a create/release loop far
/// past the slot table and the arena. The high-water mark is set by the
/// FIRST registration and never moves: the released bytes are reused.
#[test]
fn every_entry_kind_created_and_released_in_a_loop_never_exhausts() {
    type Reg = fn(&mut Executor<'_>) -> Result<HandleId, NodeError>;
    type Rel = unsafe fn(&mut Executor<'_>, HandleId) -> bool;
    let kinds: [(&str, Reg, Rel); 4] = [
        ("subscription", register_buffered_sub, |e, h| unsafe {
            e.release_subscription(h)
        }),
        (
            "timer",
            |e| e.register_timer(TimerDuration::from_millis(1000), || {}),
            |e, h| unsafe { e.release_timer(h) },
        ),
        (
            "service",
            |e| {
                e.register_service_raw(
                    "/release_probe",
                    "test/srv/T",
                    "h",
                    release_probe_srv,
                    core::ptr::null_mut(),
                )
            },
            |e, h| unsafe { e.release_service(h) },
        ),
        (
            "service client",
            |e| {
                e.register_service_client_raw(
                    "/release_probe",
                    "test/srv/T",
                    "h",
                    Some(release_probe_reply),
                    core::ptr::null_mut(),
                )
            },
            |e, h| unsafe { e.release_service_client(h) },
        ),
    ];
    for (name, register, release) in kinds {
        let mut executor: Executor = executor_with_clock(MockSession::new());
        let first = register(&mut executor).unwrap_or_else(|e| panic!("{name}: first: {e:?}"));
        let high_water = executor.arena_used();
        assert!(high_water > 0, "{name}: the registration claimed bytes");
        assert_eq!(
            executor.entries[first.slot()].unwrap().arena_len.get(),
            Some(high_water),
            "{name}: the only entry's recorded region is the whole used arena"
        );
        assert!(unsafe { release(&mut executor, first) }, "{name}: release");
        assert!(
            executor.entries[first.slot()].is_none(),
            "{name}: slot freed"
        );
        assert_eq!(
            executor.arena_used(),
            0,
            "{name}: the only entry's bytes went back to the bump pointer"
        );
        assert!(
            !unsafe { release(&mut executor, first) },
            "{name}: a second release finds nothing"
        );
        const N: usize = 200;
        for i in 0..N {
            let h = register(&mut executor)
                .unwrap_or_else(|e| panic!("{name} iteration {i}: registration failed: {e:?}"));
            assert_eq!(
                executor.arena_used(),
                high_water,
                "{name} iteration {i}: the arena grew; the released region was not reused"
            );
            assert!(unsafe { release(&mut executor, h) }, "{name} iteration {i}");
        }
        // A compile-time fact, so a compile-time check: as a runtime
        // `assert!` clippy's `assertions_on_constants` refuses it under
        // `-D warnings` whenever `MAX_CBS` is a plain constant (the `std`
        // feature set).
        const {
            assert!(
                N > crate::config::MAX_CBS,
                "precondition: the loop out-runs the slot table"
            )
        };
    }
}

/// The buffered subscription's TRAILING region is released with it. Released
/// at the entry's `size_of` alone (1496's rule), the ring would stay claimed
/// and every cycle would bump the arena by its length.
#[test]
fn a_buffered_subscription_releases_its_trailing_region_too() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let h = register_buffered_sub(&mut executor).expect("sub");
    let recorded = executor.entries[h.slot()]
        .unwrap()
        .arena_len
        .get()
        .expect("recorded");
    let used = executor.arena_used();
    assert_eq!(recorded, used, "the only entry spans the whole used arena");
    assert!(unsafe { executor.release_subscription(h) });
    assert_eq!(
        executor.arena_used(),
        0,
        "every recorded byte went back (to the bump pointer: it was the top region)"
    );
}

/// A release names its kind, so a timer handle never releases a subscription
/// that happens to occupy the slot.
#[test]
fn a_kind_release_of_another_kind_is_refused() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let h = register_buffered_sub(&mut executor).expect("sub");
    assert!(!unsafe { executor.release_timer(h) });
    assert!(!unsafe { executor.release_service(h) });
    assert!(!unsafe { executor.release_service_client(h) });
    assert!(executor.entries[h.slot()].is_some());
    assert_eq!(executor.arena_released(), 0);
}

// ============================================================================
// phase-476 W0 (issue 1667) — handles carry a generation
// ============================================================================

/// The case issue 1667 named: release an entry, let a DIFFERENT kind take the
/// very slot it held, then use the stale handle. Every lookup through it must
/// fail, and none may touch the new occupant.
#[test]
fn a_stale_handle_never_reaches_the_slots_next_occupant() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let timer = executor
        .register_timer(TimerDuration::from_millis(1000), || {})
        .expect("timer");
    assert!(unsafe { executor.release_timer(timer) });

    // `next_entry_slot` hands out the first free index, so the subscription
    // lands exactly where the timer was.
    let sub = register_buffered_sub(&mut executor).expect("sub");
    assert_eq!(
        sub.slot(),
        timer.slot(),
        "precondition: the slot was reused"
    );
    assert_ne!(sub, timer, "the new occupant has a different generation");

    assert_eq!(executor.resolve_handle(timer), None);
    assert!(executor.cancel_timer(timer).is_err());
    assert!(!executor.timer_is_canceled(timer));
    assert_eq!(executor.timer_period_ms(timer), None);
    // A release through the stale handle — even of the RIGHT kind for the new
    // occupant — finds nothing.
    assert!(!unsafe { executor.release_subscription(timer) });
    assert!(
        executor.entries[sub.slot()].is_some(),
        "the new occupant is untouched"
    );
    assert_eq!(executor.resolve_handle(sub), Some(sub.slot()));
}

/// The same slot reused by the SAME kind: the stale handle is still refused,
/// because a timer handle that outlived its timer must not cancel someone
/// else's.
#[test]
fn a_stale_handle_is_refused_even_when_the_same_kind_reuses_the_slot() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let old = executor
        .register_timer(TimerDuration::from_millis(1000), || {})
        .expect("timer");
    assert!(unsafe { executor.release_timer(old) });
    let new = executor
        .register_timer(TimerDuration::from_millis(1000), || {})
        .expect("timer");
    assert_eq!(new.slot(), old.slot());
    assert!(executor.cancel_timer(old).is_err());
    assert!(
        !executor.timer_is_canceled(new),
        "the new timer is not cancelled"
    );
    assert!(executor.cancel_timer(new).is_ok());
}

/// A handle's generation is never 0, so the first registration of a slot is
/// already distinguishable from a bare index — a reader of the packed value
/// as an index fails on first use, not on the first reuse.
#[test]
fn an_issued_handle_is_never_a_bare_index() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let h = executor
        .register_timer(TimerDuration::from_millis(1000), || {})
        .expect("timer");
    assert!(h.generation() >= 1);
    assert_ne!(h.to_raw(), h.slot());
    assert_eq!(HandleId::from_raw(h.to_raw()), h);
}

/// Issue 1703 — the by-index accessors take what an FFI holds: the PACKED
/// handle (`to_raw`, nros-cpp's `handle_id_`, nros-c's `subscription.handle_id`)
/// as well as a bare slot (nros-c's `arena_entry_index`, a resolved slot). W0
/// packed the generation into the FFI value and these five went on indexing
/// `entries` with it, so a C++ callback client's `async_send_request` was
/// `INVALID_ARGUMENT` on every call. Both spellings must reach the entry; a
/// stale packed handle must not.
#[test]
fn by_index_accessors_resolve_a_packed_handle_and_a_bare_slot() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let client = executor
        .register_service_client_raw(
            "/accessor_probe",
            "test/srv/T",
            "h",
            Some(release_probe_reply),
            core::ptr::null_mut(),
        )
        .expect("client");
    assert_ne!(client.to_raw(), client.slot(), "precondition: packed");
    unsafe {
        assert!(executor.service_client_entry_mut(client.to_raw()).is_some());
        assert!(executor.service_client_entry_mut(client.slot()).is_some());
        assert!(executor.service_client_handle(client.to_raw()).is_some());
        assert!(executor.service_client_handle(client.slot()).is_some());
    }
    let sub = register_buffered_sub(&mut executor).expect("sub");
    unsafe {
        assert!(executor.subscription_handle(sub.to_raw()).is_some());
        assert!(executor.subscription_handle(sub.slot()).is_some());
        // A kind mismatch is still refused through either spelling.
        assert!(executor.service_client_entry_mut(sub.to_raw()).is_none());
        assert!(executor.subscription_handle(client.to_raw()).is_none());
    }

    // Stale: release, reuse the slot with the same kind, and the OLD packed
    // handle must find nothing.
    assert!(unsafe { executor.release_service_client(client) });
    let again = executor
        .register_service_client_raw(
            "/accessor_probe",
            "test/srv/T",
            "h",
            Some(release_probe_reply),
            core::ptr::null_mut(),
        )
        .expect("client again");
    assert_eq!(again.slot(), client.slot(), "precondition: slot reused");
    unsafe {
        assert!(executor.service_client_entry_mut(client.to_raw()).is_none());
        assert!(executor.service_client_entry_mut(again.to_raw()).is_some());
    }
}

/// Generations wrap within 15 bits and skip 0, so the packed value always fits
/// the `i32` nros-c stores some handles in, and 0 stays reserved for
/// `HandleId::for_owned_slot`.
#[test]
fn the_generation_wraps_to_one_inside_fifteen_bits() {
    let tag = super::types::SlotTag::new(super::types::HANDLE_GENERATION_MAX, 0);
    assert_eq!(tag.next_generation(), 1);
    let h = HandleId::new(63, super::types::HANDLE_GENERATION_MAX);
    assert!(i32::try_from(h.to_raw()).is_ok());
}

/// `release_node` releases what the node registered and nothing else.
#[test]
fn releasing_a_node_releases_only_its_own_entries() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let a = executor.node_builder("a").build().unwrap();
    let b = executor.node_builder("b").build().unwrap();
    let a_timer = executor
        .register_timer(TimerDuration::from_millis(1000), || {})
        .unwrap();
    let a_sub = register_buffered_sub(&mut executor).unwrap();
    let b_timer = executor
        .register_timer(TimerDuration::from_millis(1000), || {})
        .unwrap();
    let unowned = executor
        .register_timer(TimerDuration::from_millis(1000), || {})
        .unwrap();
    assert!(executor.set_entry_owner(a_timer, a));
    assert!(executor.set_entry_owner(a_sub, a));
    assert!(executor.set_entry_owner(b_timer, b));

    assert_eq!(unsafe { executor.release_node(a) }, 2);
    assert_eq!(executor.resolve_handle(a_timer), None);
    assert_eq!(executor.resolve_handle(a_sub), None);
    assert!(executor.resolve_handle(b_timer).is_some());
    assert!(executor.resolve_handle(unowned).is_some());
    // Idempotent: nothing of `a` is left.
    assert_eq!(unsafe { executor.release_node(a) }, 0);
}

/// A slot's owner belongs to its OCCUPANT: once released and reused by an
/// entry nobody claimed, a later `release_node` of the old owner leaves it be.
#[test]
fn a_reused_slot_does_not_inherit_its_previous_owner() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let a = executor.node_builder("a").build().unwrap();
    let t = executor
        .register_timer(TimerDuration::from_millis(1000), || {})
        .unwrap();
    assert!(executor.set_entry_owner(t, a));
    assert!(unsafe { executor.release_timer(t) });
    let other = executor
        .register_timer(TimerDuration::from_millis(1000), || {})
        .unwrap();
    assert_eq!(other.slot(), t.slot());
    assert_eq!(unsafe { executor.release_node(a) }, 0);
    assert!(executor.resolve_handle(other).is_some());
}

/// A CAPTURE is released with its entry (phase-476 W0). It used to be a
/// separate arena allocation the release never gave back, so a capturing
/// registration in a create/release loop grew the arena by the capture every
/// cycle.
#[test]
fn a_capturing_subscription_releases_its_capture_too() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let capture = [0xA5u8; 40];
    let register = |e: &mut Executor<'_>, depth: u32| {
        e.add_arena_subscription_c_callback_with_capture::<{ crate::config::DEFAULT_RX_BUF_SIZE }>(
            None,
            "/capture_release",
            "test/msg/TestMsg",
            "test_hash",
            QoSProfile::default().keep_last(depth),
            release_probe_sub,
            core::ptr::null_mut(),
            None,
            96,
            Some(&capture),
            None,
        )
    };
    // Both dispatch shapes: depth 1 and depth 4 take different entry types.
    for depth in [1u32, 4] {
        let first = register(&mut executor, depth).expect("first");
        let high_water = executor.arena_used();
        assert!(unsafe { executor.release_subscription(first) });
        for i in 0..200 {
            let h = register(&mut executor, depth).expect("re-register");
            assert_eq!(
                executor.arena_used(),
                high_water,
                "depth {depth} iteration {i}: the arena grew; the capture was not released"
            );
            assert!(unsafe { executor.release_subscription(h) });
        }
    }
}

// ============================================================================
// phase-476 W2 — a C timer whose capture the arena holds
// ============================================================================

static CAPTURED_TIMER_SEEN: portable_atomic::AtomicUsize = portable_atomic::AtomicUsize::new(0);

unsafe extern "C" fn captured_timer_cb(ctx: *mut core::ffi::c_void) {
    // The capture's LAST word is the one a truncating copy would lose.
    let words = ctx as *const usize;
    CAPTURED_TIMER_SEEN.store(unsafe { *words.add(2) }, portable_atomic::Ordering::SeqCst);
}

private_test_clock!(capturing_timer_clock);

/// The capture is the ARENA's copy: the caller's bytes are gone before the
/// timer first fires, and the callback still reads them.
#[test]
fn a_capturing_timer_dispatches_with_the_arenas_copy() {
    capturing_timer_clock::claim_at_us(1_000_000);
    let mut executor: Executor =
        executor_with_clock_fn(MockSession::new(), capturing_timer_clock::now_us);
    {
        let capture: [usize; 3] = [1, 2, 0xBEEF];
        let bytes = unsafe {
            core::slice::from_raw_parts(
                capture.as_ptr() as *const u8,
                core::mem::size_of_val(&capture),
            )
        };
        executor
            .register_timer_c_capturing(
                None,
                TimerDuration::from_millis(10),
                TimerClockSource::Steady,
                captured_timer_cb,
                Some(bytes),
                None,
            )
            .expect("register");
    }
    CAPTURED_TIMER_SEEN.store(0, portable_atomic::Ordering::SeqCst);
    let fired = capturing_timer_clock::elapse_then_spin_once(&mut executor, 20).timers_fired;
    assert_eq!(fired, 1);
    assert_eq!(
        CAPTURED_TIMER_SEEN.load(portable_atomic::Ordering::SeqCst),
        0xBEEF
    );
}

static CAPTURED_TIMER_DROPS: portable_atomic::AtomicUsize = portable_atomic::AtomicUsize::new(0);

unsafe extern "C" fn captured_timer_drop(ctx: *mut core::ffi::c_void) {
    let words = ctx as *const usize;
    if unsafe { *words.add(2) } == 0xD509 {
        CAPTURED_TIMER_DROPS.fetch_add(1, portable_atomic::Ordering::SeqCst);
    }
}

/// The capture's destructor hook runs once when the timer is released, reading
/// the capture's bytes while they are still the capture's.
#[test]
fn a_released_capturing_timer_destroys_its_capture_exactly_once() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let capture: [usize; 3] = [0, 0, 0xD509];
    let bytes = unsafe {
        core::slice::from_raw_parts(
            capture.as_ptr() as *const u8,
            core::mem::size_of_val(&capture),
        )
    };
    let before = CAPTURED_TIMER_DROPS.load(portable_atomic::Ordering::SeqCst);
    let h = executor
        .register_timer_c_capturing(
            None,
            TimerDuration::from_millis(1000),
            TimerClockSource::Steady,
            captured_timer_cb,
            Some(bytes),
            Some(captured_timer_drop),
        )
        .expect("register");
    assert_eq!(
        CAPTURED_TIMER_DROPS.load(portable_atomic::Ordering::SeqCst),
        before
    );
    assert!(unsafe { executor.release_timer(h) });
    assert_eq!(
        CAPTURED_TIMER_DROPS.load(portable_atomic::Ordering::SeqCst),
        before + 1
    );
    assert!(!unsafe { executor.release_timer(h) });
    assert_eq!(
        CAPTURED_TIMER_DROPS.load(portable_atomic::Ordering::SeqCst),
        before + 1
    );
}

// Issue 1667 — a callback that releases its OWN entry (`timer_.reset()` inside
// the timer's callback, the upstream one-shot idiom). The release must wait for
// the callback to return: dropping the entry under it destroys the capture the
// running callback is still reading.
static SELF_RELEASE_EXEC: portable_atomic::AtomicPtr<Executor<'static>> =
    portable_atomic::AtomicPtr::new(core::ptr::null_mut());
static SELF_RELEASE_HANDLE: portable_atomic::AtomicUsize = portable_atomic::AtomicUsize::new(0);
static SELF_RELEASE_DROPS: portable_atomic::AtomicUsize = portable_atomic::AtomicUsize::new(0);
static SELF_RELEASE_CALLS: portable_atomic::AtomicUsize = portable_atomic::AtomicUsize::new(0);
static SELF_RELEASE_SAW_AFTER: portable_atomic::AtomicUsize = portable_atomic::AtomicUsize::new(0);
static SELF_RELEASE_DROPS_DURING: portable_atomic::AtomicUsize =
    portable_atomic::AtomicUsize::new(usize::MAX);

const SELF_RELEASE_MARK: usize = 0x5E1F;

unsafe extern "C" fn self_releasing_timer_cb(ctx: *mut core::ffi::c_void) {
    use portable_atomic::Ordering::SeqCst;
    SELF_RELEASE_CALLS.fetch_add(1, SeqCst);
    let exec = SELF_RELEASE_EXEC.load(SeqCst);
    let h = HandleId::from_raw(SELF_RELEASE_HANDLE.load(SeqCst));
    // SAFETY: the test's executor, reached the way a C++ callback reaches it
    // (through the raw handle its capture holds).
    let released = unsafe { (*exec).release_timer(h) };
    assert!(
        released,
        "the first release from inside the callback is accepted"
    );
    // A second request — another copy of the handle — is refused: the handle
    // stopped resolving at the first.
    assert!(!unsafe { (*exec).release_timer(h) });
    assert_eq!(unsafe { (*exec).resolve_handle(h) }, None);
    SELF_RELEASE_DROPS_DURING.store(SELF_RELEASE_DROPS.load(SeqCst), SeqCst);
    // The capture is still the capture.
    SELF_RELEASE_SAW_AFTER.store(unsafe { *(ctx as *const usize).add(2) }, SeqCst);
}

unsafe extern "C" fn self_releasing_timer_drop(ctx: *mut core::ffi::c_void) {
    if unsafe { *(ctx as *const usize).add(2) } == SELF_RELEASE_MARK {
        SELF_RELEASE_DROPS.fetch_add(1, portable_atomic::Ordering::SeqCst);
    }
}

private_test_clock!(self_release_clock);

#[test]
fn a_callback_releasing_its_own_entry_is_deferred_until_it_returns() {
    self_release_clock::claim_at_us(1_000_000);
    use portable_atomic::Ordering::SeqCst;
    let mut executor: Executor =
        executor_with_clock_fn(MockSession::new(), self_release_clock::now_us);
    let capture: [usize; 3] = [0, 0, SELF_RELEASE_MARK];
    let bytes = unsafe {
        core::slice::from_raw_parts(
            capture.as_ptr() as *const u8,
            core::mem::size_of_val(&capture),
        )
    };
    let h = executor
        .register_timer_c_capturing(
            None,
            TimerDuration::from_millis(1),
            TimerClockSource::Steady,
            self_releasing_timer_cb,
            Some(bytes),
            Some(self_releasing_timer_drop),
        )
        .expect("register");
    let high_water = executor.arena_used();
    SELF_RELEASE_HANDLE.store(h.to_raw(), SeqCst);
    SELF_RELEASE_EXEC.store(&mut executor as *mut Executor<'static>, SeqCst);

    for _ in 0..3 {
        self_release_clock::elapse_then_spin_once(&mut executor, 5);
    }
    SELF_RELEASE_EXEC.store(core::ptr::null_mut(), SeqCst);

    assert_eq!(
        SELF_RELEASE_CALLS.load(SeqCst),
        1,
        "a released timer never fires again"
    );
    assert_eq!(
        SELF_RELEASE_DROPS_DURING.load(SeqCst),
        0,
        "the capture was destroyed while its callback was still running"
    );
    assert_eq!(SELF_RELEASE_SAW_AFTER.load(SeqCst), SELF_RELEASE_MARK);
    assert_eq!(
        SELF_RELEASE_DROPS.load(SeqCst),
        1,
        "destroyed exactly once, after the callback"
    );
    assert_eq!(executor.resolve_handle(h), None);
    // The deferred release gave the bytes back: the next registration reuses them.
    let again = executor
        .register_timer(TimerDuration::from_millis(1000), || {})
        .unwrap();
    assert_eq!(again.slot(), h.slot());
    assert!(executor.arena_used() <= high_water);
}

/// Releasing a capturing timer gives its capture back: a create/release loop
/// holds the arena at the first registration's high-water mark.
#[test]
fn a_capturing_timer_releases_its_capture_too() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let capture = [0x5Au8; 24];
    let register = |e: &mut Executor<'_>| {
        e.register_timer_c_capturing(
            None,
            TimerDuration::from_millis(1000),
            TimerClockSource::Steady,
            captured_timer_cb,
            Some(&capture),
            None,
        )
    };
    let first = register(&mut executor).expect("first");
    let high_water = executor.arena_used();
    assert!(unsafe { executor.release_timer(first) });
    for i in 0..200 {
        let h = register(&mut executor).expect("re-register");
        assert_eq!(
            executor.arena_used(),
            high_water,
            "iteration {i}: the arena grew; the capture was not released"
        );
        assert!(unsafe { executor.release_timer(h) });
    }
}

/// The recorded length rides in `CallbackMeta`'s tail padding: the entry
/// table — and so every stated executor backing size — does not grow.
#[test]
fn callback_meta_len_rides_in_padding() {
    assert_eq!(
        core::mem::size_of::<Option<super::arena::CallbackMeta>>(),
        6 * core::mem::size_of::<usize>(),
        "offset + four fn pointers + one word for kind, invocation and arena_len"
    );
}

/// Issue 1036 — with the fatal knob on, arena exhaustion reaches the fatal
/// hook (a latch in a test build), after the record is written.
#[cfg(nros_arena_exhaustion_fatal)]
#[test]
fn arena_exhaustion_reaches_the_fatal_hook() {
    let _ = super::arena::ARENA_FATAL_HOOK_REACHED.swap(false, portable_atomic::Ordering::Relaxed);
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let mut hit = false;
    for _ in 0..64 {
        match register_raw_server(&mut executor) {
            Err(NodeError::BufferTooSmall) => {
                hit = true;
                break;
            }
            Err(NodeError::ExecutorFull) => break,
            _ => {}
        }
    }
    if !hit {
        // The slot table filled first; free slots and keep allocating until
        // the ARENA refuses, which is what this test is about.
        panic!("precondition: the arena must run out before the slot table");
    }
    assert!(
        super::arena::ARENA_FATAL_HOOK_REACHED.load(portable_atomic::Ordering::Relaxed),
        "arena exhaustion must reach the fatal hook when the image asked for it"
    );
}

#[test]
fn the_arena_refusal_rule_is_released_bytes_versus_the_request() {
    use super::arena::arena_refusal_is_fragmented as frag;
    assert!(frag(100, 100));
    assert!(!frag(100, 99));
    assert!(!frag(0, 0));
}

/// Issue 1496 — a moved C++ client re-points its arena entry's context.
#[test]
fn retargeting_a_raw_action_client_moves_its_context() {
    use super::arena::ActionClientRawArenaEntry;
    const B: usize = crate::config::DEFAULT_RX_BUF_SIZE;
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let idx = register_raw_client(&mut executor).expect("client");
    let mut new_home = [0u8; 16];
    let new_ptr = new_home.as_mut_ptr() as *mut core::ffi::c_void;
    assert!(unsafe { executor.retarget_action_client_raw(idx, new_ptr) });
    let offset = executor.entries[idx].as_ref().unwrap().offset;
    let entry = unsafe {
        &*((executor.arena.as_mut_ptr() as *mut u8).add(offset)
            as *const ActionClientRawArenaEntry<B, B, B>)
    };
    assert_eq!(entry.context as usize, new_ptr as usize);
    assert!(
        !unsafe { executor.retarget_action_client_raw(idx + 1, new_ptr) },
        "an empty slot refuses"
    );
    let _ = new_home;
}

private_test_clock!(violation_sink_clock);

/// Issue 1635 -- with a sink installed, every violation reaches it (at the end
/// of a spin since phase-474 I7, not at detection), so an image whose entry
/// never calls `drain_violations` still reports. The ring is filled as before, so a hand-draining application or
/// fixture sees exactly what it did; removing the sink stops the feed.
#[test]
fn an_installed_violation_sink_sees_every_violation() {
    static SEEN: std::sync::Mutex<alloc::vec::Vec<(&'static str, u32)>> =
        std::sync::Mutex::new(alloc::vec::Vec::new());
    unsafe fn sink(ctx: *mut core::ffi::c_void, pending: &[super::monitor::Violation]) -> usize {
        assert_eq!(
            ctx as usize, 0x1635,
            "the sink gets the context it was installed with"
        );
        for v in pending {
            SEEN.lock().unwrap().push((v.rule, v.measured));
        }
        pending.len()
    }

    // A PRIVATE clock (issue 1945): the 10 ms step after each stall is one
    // timer period, so on the real clock a preempted step drops a period and
    // a second verdict no longer matches the running total below.
    violation_sink_clock::claim_at_us(1_000_000);
    let mut executor: Executor =
        executor_with_clock_fn(MockSession::new(), violation_sink_clock::now_us);
    // The log line is not what this asserts, and the log sink is process-wide:
    // left on, these two lines reached T4's capture when the two tests ran
    // side by side, and T4 failed "nothing reported" (phase-474 I7).
    executor.set_report_violations(false);
    let id = executor
        .register_timer(TimerDuration::from_millis(10), || {})
        .unwrap();
    unsafe { executor.set_violation_sink(Some((sink, 0x1635usize as *mut core::ffi::c_void))) };

    // A stall worth ~12 periods: one overrun verdict.
    let _ = violation_sink_clock::elapse_then_spin_once(&mut executor, 120);
    let _ = violation_sink_clock::elapse_then_spin_once(&mut executor, 10);
    let overruns = executor.timer_overruns(id).unwrap();
    let seen = SEEN.lock().unwrap().clone();
    assert_eq!(
        seen.iter()
            .filter(|&&(r, m)| r == "timer-overrun-runtime" && m == overruns)
            .count(),
        1,
        "the sink received the overrun verdict once: {seen:?}"
    );
    let mut ring = alloc::vec::Vec::new();
    executor.drain_violations(|v| ring.push(v.rule));
    assert!(
        ring.contains(&"timer-overrun-runtime"),
        "the ring keeps the verdict for a hand drain too: {ring:?}"
    );

    // Removing the sink stops the feed; the ring still fills.
    unsafe { executor.set_violation_sink(None) };
    SEEN.lock().unwrap().clear();
    let _ = violation_sink_clock::elapse_then_spin_once(&mut executor, 120);
    let _ = violation_sink_clock::elapse_then_spin_once(&mut executor, 10);
    assert!(SEEN.lock().unwrap().is_empty(), "no sink, no feed");
    let mut kept = alloc::vec::Vec::new();
    executor.drain_violations(|v| kept.push(v.rule));
    assert!(kept.contains(&"timer-overrun-runtime"), "{kept:?}");
}

/// Issue 1676 — the RAW publisher the Rust component road creates
/// (`create_generic_publisher` -> `create_publisher_raw_on`) attaches its
/// contracted endpoint's cell and bumps it on every publish, as the typed
/// `EmbeddedPublisher` always did. Before this a contracted Rust talker
/// publishing at 1 Hz was measured at 0.
#[test]
fn a_raw_publisher_on_a_contracted_topic_bumps_its_cell() {
    use super::monitor::{MonitorSpec, PubMonitorCell};
    use core::sync::atomic::Ordering;
    static CELL: PubMonitorCell = PubMonitorCell::new();
    static TABLE: [MonitorSpec; 1] = [MonitorSpec {
        topic: "/chatter",
        fqn: "/talker/chatter",
        min_rate_hz_milli: 1_000,
        max_latency_ms: 0,
        route_trigger: "",
        route_max_latency_ms: 0,
        route_deadline_ms: 0,
        reaction_id: 0,
        mechanism: 0,
        cell: &CELL,
    }];

    let mut executor: Executor = executor_with_clock(MockSession::new());
    executor.set_monitor_table(&TABLE);
    let nid = executor.node_builder("talker").build().expect("node");
    let contracted = executor
        .node_mut(nid)
        .create_generic_publisher("/chatter", "std_msgs::msg::dds_::Int32_", "")
        .expect("contracted publisher");
    let free = executor
        .node_mut(nid)
        .create_generic_publisher("/other", "std_msgs::msg::dds_::Int32_", "")
        .expect("uncontracted publisher");

    let before = CELL.count.load(Ordering::Relaxed);
    contracted
        .publish_serialized(&[0, 1, 0, 0, 7, 0, 0, 0])
        .expect("publish");
    contracted
        .publish_serialized_with_attachment(&[0, 1, 0, 0, 7, 0, 0, 0], &[])
        .expect("publish with attachment");
    assert_eq!(
        CELL.count.load(Ordering::Relaxed) - before,
        2,
        "every publish route bumps the contracted cell"
    );
    free.publish_serialized(&[0, 1, 0, 0, 7, 0, 0, 0])
        .expect("publish");
    assert_eq!(
        CELL.count.load(Ordering::Relaxed) - before,
        2,
        "a publisher on another topic carries no cell"
    );
}

/// Issue 1706 -- a heap too small for the parameter store REFUSES the
/// declaration instead of halting the image.
///
/// The store is one allocation of `PARAMETER_STORE_BYTES` (285,696 at the
/// default sizing on x86_64). Through `Box::new_uninit` a refused request went
/// to the OOM handler: this test process aborted, as a Zephyr image halted
/// (`HEAP EXHAUSTED`, then issue 0589's recursion) and a FreeRTOS one hung in
/// `MALLOC FAILED`. Every caller has a refusal to return, so it now returns it,
/// once, without retrying.
#[cfg(feature = "param-store")]
#[test]
fn a_heap_too_small_for_the_parameter_store_refuses_the_declaration() {
    use nros_params::ParameterValue;
    let store = core::mem::size_of::<nros_params::ParameterStorage>();
    assert!(
        store > 64 * 1024,
        "the store is the large allocation this test caps under ({store} B)"
    );
    let mut executor: Executor = executor_with_clock(MockSession::new());
    {
        // The platform heap refuses the store's request (the seam stands in
        // for an allocator returning NULL -- a second `#[global_allocator]`
        // is refused by `check-feature-contract` clause (e)).
        let _refuse = super::spin::RefuseParameterStore::arm();
        assert!(
            !executor.declare_parameter("rate", ParameterValue::Integer(10)),
            "a store the heap cannot supply must refuse the declaration"
        );
        #[cfg(feature = "param-services")]
        assert!(
            executor.register_parameter_services().is_err(),
            "six services over no store is an error, not a silent no-op"
        );
    }
    // Recorded, not retried: a heap that refused once is not asked again,
    // even though this thread could now supply it.
    assert!(!executor.declare_parameter("rate", ParameterValue::Integer(10)));

    // Negative control: the same executor shape with an uncapped heap
    // declares, so the refusal above is the cap and not the call.
    let mut ok: Executor = executor_with_clock(MockSession::new());
    assert!(ok.declare_parameter("rate", ParameterValue::Integer(10)));
}

/// phase-382 W3' — a backing that CARVED the store supplies it, and the heap is
/// never asked.
///
/// The shape of the test above with one difference, the sizing: this executor
/// carves `slots` parameter slots from its own backing. The heap refusal is
/// armed for the whole test, so a store reached through
/// `leak_parameter_storage` would refuse the declaration exactly as above.
/// Before W3' the backing had no such region and every store came from the
/// heap, so this declaration FAILED; it succeeds now, which is the store
/// coming from the arena and nothing else.
///
/// The capacity is the carved length and not `MAX_PARAMETERS` — a fourth
/// declaration over three carved slots is refused as FULL, which also rules
/// out a store that came from somewhere else and merely happened to fit.
#[cfg(feature = "param-store")]
#[test]
fn a_store_carved_from_the_backing_never_asks_the_heap() {
    use nros_params::ParameterValue;
    // phase-430 W2 -- under `sim-time` the store seeds `use_sim_time` on the
    // PRIMARY node the moment it exists, and that placeholder is a real slot.
    let seeded = usize::from(cfg!(feature = "sim-time"));
    let slots = 3 + seeded;
    let sizing = super::storage::ExecutorSizing {
        params: slots,
        ..super::storage::ExecutorSizing::DEFAULT
    };
    let backing: &'static mut [core::mem::MaybeUninit<u64>] =
        alloc::boxed::Box::leak(alloc::boxed::Box::new_uninit_slice(sizing.u64_len()));
    #[cfg(feature = "param-services")]
    let carved = backing.as_ptr_range();
    #[cfg(feature = "param-services")]
    let carved = carved.start as usize..carved.end as usize;
    let _refuse = super::spin::RefuseParameterStore::arm();
    // SAFETY: `backing` is exactly `sizing.u64_len()` words, `'static`, and is
    // handed to this executor alone.
    let mut executor: Executor =
        unsafe { Executor::from_session_in(MockSession::new(), backing, sizing) };
    executor.clock_us_fn = Some(test_clock_us);
    for (i, name) in ["a", "b", "c"].iter().enumerate() {
        assert!(
            executor.declare_parameter(name, ParameterValue::Integer(i as i64)),
            "declaration {i} must land in the CARVED store; the heap is refused"
        );
    }
    assert!(
        !executor.declare_parameter("d", ParameterValue::Integer(3)),
        "the carved store holds exactly {slots} slots ({seeded} seeded)"
    );
    #[cfg(feature = "param-services")]
    assert!(
        executor.register_parameter_services().is_ok(),
        "the services attach to the carved store, no heap store needed"
    );
    assert_eq!(
        executor.get_parameter("b").and_then(|v| v.as_integer()),
        Some(1),
        "values read back from the carved slots"
    );
    assert!(
        matches!(
            executor.params.as_ref(),
            Some(crate::param_store::Placed::Carved(_))
        ),
        "a backing with a store carves the store's state"
    );

    #[cfg(feature = "param-services")]
    {
        // Issue 1812 — and the SERVICES' state is the backing's too: the state,
        // its (inline) set of six, the shared buffer pair and the staging slot.
        // Before 1812 each of these was a heap allocation even over a carved
        // store, which is what kept `param-services` behind `alloc`.
        executor.set_node_identity("carved", "/");
        executor
            .reconcile_parameter_services()
            .expect("one set of six over the mock session");
        let params = executor.params.as_ref().expect("the state exists");
        assert!(
            matches!(params, crate::param_store::Placed::Carved(_)),
            "a backing with a store carves the services' state too"
        );
        let state: &crate::param_store::ParamState<'_> = params;
        let within = |addr: usize, what: &str| {
            assert!(
                carved.contains(&addr),
                "{what} at {addr:#x} is outside the executor backing {carved:x?}"
            );
        };
        within(state as *const _ as usize, "the parameter-service state");
        within(&state.staged as *const _ as usize, "the staging slot");
        assert_eq!(state.services.len(), 1, "one set of six, held inline");
        within(&state.services[0] as *const _ as usize, "the set of six");
        within(
            state
                .buffers
                .as_ref()
                .expect("a carved home brings its pair")
                .request_addr(),
            "the request buffer",
        );
    }
}

/// Issue 1856 -- the services' half of the carved home follows the build, and
/// the store does not.
///
/// Issue 1897 -- "the build" is the `param-services` cargo feature, which
/// replaced the `NROS_CAPABILITY_PARAM_SERVICES=0` statement this test used to
/// branch on. `node-std-tests` runs it with `param-services` (issue 1812's
/// whole home) and with `param-store` alone (an image that HOLDS a store and
/// serves none, the safety island's shape). Each build compiles the branch it
/// must satisfy, so neither run passes by skipping.
#[cfg(feature = "param-store")]
#[test]
fn the_carved_home_reserves_the_services_only_when_the_image_serves_them() {
    use crate::param_store::{HOME_BUFFER_BYTES, ParamHome, ParamState, Placed};
    use core::mem::size_of;
    use nros_params::ParameterValue;

    let seeded = usize::from(cfg!(feature = "sim-time"));
    let sizing = super::storage::ExecutorSizing {
        params: 2 + seeded,
        ..super::storage::ExecutorSizing::DEFAULT
    };
    let backing: &'static mut [core::mem::MaybeUninit<u64>] =
        alloc::boxed::Box::leak(alloc::boxed::Box::new_uninit_slice(sizing.u64_len()));
    let _refuse = super::spin::RefuseParameterStore::arm();
    // SAFETY: `backing` is exactly `sizing.u64_len()` words, `'static`, and is
    // handed to this executor alone.
    let mut executor: Executor =
        unsafe { Executor::from_session_in(MockSession::new(), backing, sizing) };
    executor.clock_us_fn = Some(test_clock_us);
    executor.set_node_identity("held", "/");
    // Either way the store is the backing's, and it holds a declaration.
    assert!(
        executor.declare_parameter("rate", ParameterValue::Integer(10)),
        "the carved store accepts a declaration whether or not it is served"
    );
    assert!(matches!(executor.params.as_ref(), Some(Placed::Carved(_))));

    #[cfg(feature = "param-services")]
    {
        use crate::{
            param_sizing::{MAX_SERVICE_SETS, param_service_buffer_bytes},
            parameter_services::ParameterServiceServers,
        };
        // Issue 1812's guarantee, held: a home with the services in it, so an
        // image that serves them asks the heap for nothing.
        assert_eq!(HOME_BUFFER_BYTES, param_service_buffer_bytes());
        assert!(
            size_of::<ParamState<'_>>() >= MAX_SERVICE_SETS * size_of::<ParameterServiceServers>(),
            "the sets are inline, one per node slot"
        );
        assert!(size_of::<ParamHome>() >= size_of::<ParamState<'_>>() + 2 * HOME_BUFFER_BYTES);
        assert!(executor.register_parameter_services().is_ok());
        executor
            .reconcile_parameter_services()
            .expect("one set of six over the mock session");
        assert_eq!(executor.params.as_ref().map(|p| p.services.len()), Some(1));
    }
    #[cfg(not(feature = "param-services"))]
    {
        // The services' half is gone: no sets, no buffer pair, and no method
        // that could ask for them. The home is the state alone.
        assert_eq!(HOME_BUFFER_BYTES, 0);
        assert_eq!(size_of::<ParamHome>(), size_of::<ParamState<'_>>());
        // And the store still works without them.
        assert_eq!(
            executor.get_parameter("rate").and_then(|v| v.as_integer()),
            Some(10)
        );
    }
}

/// phase-382 W3' — the negative control's other half: a sizing that carves
/// NOTHING still gets a store, from the heap, exactly as before W3'. Zero
/// slots is the default for every image the build does not know to build a
/// store, so this is the road an undeclared store takes.
#[cfg(feature = "param-store")]
#[test]
fn a_backing_that_carved_no_store_falls_back_to_the_heap() {
    use nros_params::ParameterValue;
    let sizing = super::storage::ExecutorSizing {
        params: 0,
        ..super::storage::ExecutorSizing::DEFAULT
    };
    let backing: &'static mut [core::mem::MaybeUninit<u64>] =
        alloc::boxed::Box::leak(alloc::boxed::Box::new_uninit_slice(sizing.u64_len()));
    // SAFETY: as above.
    let mut executor: Executor =
        unsafe { Executor::from_session_in(MockSession::new(), backing, sizing) };
    executor.clock_us_fn = Some(test_clock_us);
    {
        let _refuse = super::spin::RefuseParameterStore::arm();
        assert!(
            !executor.declare_parameter("rate", ParameterValue::Integer(10)),
            "no carved slots: the store is the heap's, and the heap refused"
        );
    }
    let mut heaped: Executor = unsafe {
        Executor::from_session_in(
            MockSession::new(),
            alloc::boxed::Box::leak(alloc::boxed::Box::new_uninit_slice(sizing.u64_len())),
            sizing,
        )
    };
    heaped.clock_us_fn = Some(test_clock_us);
    assert!(
        heaped.declare_parameter("rate", ParameterValue::Integer(10)),
        "an uncapped heap supplies the fallback store"
    );
    assert!(
        heaped.declare_parameter("burst", ParameterValue::Integer(1)),
        "the fallback store is MAX_PARAMETERS long, not the carved zero"
    );
    // Issue 1812 — no carved home either, so the state is the heap's.
    assert!(
        matches!(heaped.params, Some(crate::param_store::Placed::Heap(_))),
        "a backing with no store carves no services' home"
    );
}

/// Issue 1667 — the workload issue 1496's 8-region table was never measured
/// against: entries of FOUR sizes (40 B to 4.6 KiB) created and released in a
/// pseudo-random order, at most three live at once, for 100 000 rounds.
///
/// Measured on the table: the arena filled (73 248 of 74 240 B) with at most
/// ~14 KiB live and only 5 424 B listed as released — the rest was dropped by
/// the table (a full table discarded a region; a split tail under 64 B was
/// discarded too), and after that every registration failed. The free list
/// drops nothing, so the arena stays near what is live.
#[test]
fn a_churning_mixed_workload_never_exhausts_the_arena() {
    type Reg = fn(&mut Executor<'_>) -> Result<HandleId, NodeError>;
    type Rel = unsafe fn(&mut Executor<'_>, HandleId) -> bool;
    let kinds: [(Reg, Rel); 4] = [
        (register_buffered_sub, |e, h| unsafe {
            e.release_subscription(h)
        }),
        (
            |e| e.register_timer(TimerDuration::from_millis(1000), || {}),
            |e, h| unsafe { e.release_timer(h) },
        ),
        (
            |e| {
                e.register_service_raw(
                    "/churn",
                    "test/srv/T",
                    "h",
                    release_probe_srv,
                    core::ptr::null_mut(),
                )
            },
            |e, h| unsafe { e.release_service(h) },
        ),
        (
            |e| {
                e.register_service_client_raw(
                    "/churn",
                    "test/srv/T",
                    "h",
                    Some(release_probe_reply),
                    core::ptr::null_mut(),
                )
            },
            |e, h| unsafe { e.release_service_client(h) },
        ),
    ];
    // The largest single entry, so the bound below is stated in its terms.
    let largest = kinds
        .iter()
        .map(|(reg, _)| {
            let mut e: Executor = executor_with_clock(MockSession::new());
            reg(&mut e).expect("register alone");
            e.arena_used()
        })
        .max()
        .unwrap();

    const LIVE: usize = 3;
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let mut live: std::vec::Vec<(usize, HandleId)> = std::vec::Vec::new();
    let mut seed: u64 = 0x1667;
    let mut next = || {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (seed >> 33) as usize
    };
    let mut peak = 0;
    for round in 0..100_000 {
        if live.len() < LIVE && (live.is_empty() || next() % 2 == 0) {
            let k = next() % kinds.len();
            let h = kinds[k].0(&mut executor).unwrap_or_else(|e| {
                panic!(
                    "round {round}: registration failed with {e:?} — {} B used, {} B in {} holes, \
                     {} live",
                    executor.arena_used(),
                    executor.arena_released(),
                    executor.arena_holes(),
                    live.len()
                )
            });
            live.push((k, h));
        } else {
            let (k, h) = live.swap_remove(next() % live.len());
            assert!(unsafe { kinds[k].1(&mut executor, h) }, "round {round}");
        }
        peak = peak.max(executor.arena_used());
    }
    assert!(
        peak <= 2 * LIVE * largest,
        "the arena reached {peak} B for at most {LIVE} live entries of at most {largest} B"
    );
    for (k, h) in live.drain(..) {
        assert!(unsafe { kinds[k].1(&mut executor, h) });
    }
    assert_eq!(
        (executor.arena_used(), executor.arena_holes()),
        (0, 0),
        "with nothing live, every byte coalesced back into the bump pointer"
    );
}
/// phase-474 I1 -- the ring keeps the LATEST `MAX_VIOLATIONS` verdicts, counts
/// what it evicted, and numbers each one, so a late violation is never lost to
/// start-up noise (the Autoware Safety Island's ring held its first 8 since
/// boot, all start-up).
#[test]
fn the_violation_ring_keeps_the_latest_and_numbers_them() {
    use super::monitor::{MAX_VIOLATIONS, Violation};
    let mut executor: Executor = executor_with_clock(MockSession::new());
    executor.set_report_violations(false);
    let cap = MAX_VIOLATIONS as u32;
    for m in 1..=cap + 3 {
        executor.violations.record(Violation {
            rule: "timer-overrun-runtime",
            fqn: "timer",
            measured: m,
            declared: 0,
        });
    }
    assert_eq!(executor.violations_total(), cap + 3);
    assert_eq!(executor.violations_dropped(), 3, "the three OLDEST evicted");
    let mut got = alloc::vec::Vec::new();
    executor.drain_violations_numbered(|seq, v| got.push((seq, v.measured)));
    let want: alloc::vec::Vec<(u32, u32)> = (4..=cap + 3).map(|m| (m, m)).collect();
    assert_eq!(got, want, "oldest first, the newest kept, numbered");

    // A drain empties the ring and keeps the counters; numbering continues.
    let mut again = 0;
    executor.drain_violations(|_| again += 1);
    assert_eq!(again, 0);
    executor.violations.record(Violation {
        rule: "silence-runtime",
        fqn: "/n/in",
        measured: 0,
        declared: 500,
    });
    let mut last = alloc::vec::Vec::new();
    executor.drain_violations_numbered(|seq, v| last.push((seq, v.rule)));
    assert_eq!(last, [(cap + 4, "silence-runtime")]);
    assert_eq!(executor.violations_dropped(), 3);
}

/// phase-474 I1 -- the drain-and-report hook drains the ring at the end of the
/// spin that detected the verdict; with it off the ring keeps the verdict for
/// a hand drain.
#[test]
fn the_drain_report_hook_drains_at_the_end_of_the_spin() {
    use super::monitor::Violation;
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let one = || Violation {
        rule: "max-latency-runtime",
        fqn: "/n/out",
        measured: 25,
        declared: 10,
    };
    executor.violations.record(one());
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    let mut kept = 0;
    executor.drain_violations(|_| kept += 1);
    assert_eq!(kept, 1, "hook off: the ring keeps it");

    executor.set_violation_drain_report(true);
    executor.violations.record(one());
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    let mut left = 0;
    executor.drain_violations(|_| left += 1);
    assert_eq!(left, 0, "hook on: the spin drained and reported it");
    assert_eq!(executor.violations_total(), 2);
    assert_eq!(executor.violations_dropped(), 0);
}

/// phase-474 I1 -- with callback tracing compiled in, a stored violation is
/// four marker events on the installed sink.
#[cfg(feature = "trace-callbacks")]
#[test]
fn a_stored_violation_emits_its_trace_markers() {
    use super::monitor::{Violation, violation_marker_words};
    use core::cell::RefCell;
    // Thread-local: the sink is process-global, and other tests' executors on
    // other threads emit through it too. This test's executor runs on this
    // thread, so its events are the ones recorded here.
    std::thread_local! {
        static EVENTS: RefCell<alloc::vec::Vec<(u32, u32)>> =
            const { RefCell::new(alloc::vec::Vec::new()) };
    }
    unsafe extern "C" fn sink(id: u32, arg: u32) {
        if (21..=24).contains(&id) {
            EVENTS.with(|e| e.borrow_mut().push((id, arg)));
        }
    }
    let _g = trace_sink_lock();
    super::callback_trace::set_trace_sink(Some(sink));
    let mut executor: Executor = executor_with_clock(MockSession::new());
    executor.set_report_violations(false);
    let v = Violation {
        rule: "max-latency-runtime",
        fqn: "/trace/out",
        measured: 30,
        declared: 10,
    };
    executor.violations.record(v.clone());
    let seen = EVENTS.with(|e| e.borrow().clone());
    let want = violation_marker_words(1, &v);
    assert!(
        seen.windows(4).any(|w| w == want),
        "the four violation events, in order: {seen:?}"
    );
}

/// Tests that install `nros_log`'s process-global sink list (`nros_log::init`
/// swaps it) and assert on what reached THEIR sink. Serialised so one cannot
/// swap the other's sink out mid-assertion (phase-474 T4).
fn log_sinks_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// [`nros_log::early::emit_until_delivered`] with this binary's budget: 5 s,
/// yielding between polls. Every test here may log from its own thread, so a
/// record a test asserts on can be lost outright under load (issue 2000's
/// bounded hold queue); see the callee for why a counted loss re-raises it
/// and why the caller must assert on every copy.
fn emit_until_delivered(emit: impl Fn(), delivered: impl Fn() -> bool) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    nros_log::early::emit_until_delivered(emit, delivered, || {
        std::thread::yield_now();
        std::time::Instant::now() < deadline
    })
}

/// phase-474 I2 -- tests that put an executor in "armed on call" mode, which
/// the process-global arm request reaches. Serialised so one test's request
/// cannot arm another's executor mid-assertion.
fn arming_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn startup_verdict(measured: u32) -> super::monitor::Violation {
    super::monitor::Violation {
        rule: "release-jitter-runtime",
        fqn: "spin",
        measured,
        declared: 10_000,
    }
}

/// phase-474 I2 -- the default is today's behaviour: armed from the start, so
/// the first verdict is stored.
#[test]
fn monitors_arm_at_the_first_spin_by_default() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    assert_eq!(
        super::monitor::MonitorArming::DEFAULT,
        super::monitor::MonitorArming {
            on_call: false,
            grace_ms: 0
        },
        "this build states no arming knob"
    );
    assert!(executor.monitors_armed());
    executor.set_report_violations(false);
    executor.violations.record(startup_verdict(57_751));
    assert_eq!(executor.violations_total(), 1);
    assert_eq!(executor.violations_suppressed_before_arm(), 0);
}

/// phase-474 I2 -- armed on call: a verdict before the call is counted and not
/// stored; the call arms, and the next one is stored. The app's call is the
/// global request (`rclc_monitors_arm`), seen at the next spin.
#[test]
fn monitors_armed_on_call_count_but_do_not_store_start_up() {
    let _g = arming_lock();
    let mut executor: Executor = executor_with_clock(MockSession::new());
    executor.set_report_violations(false);
    executor.set_monitor_arming(true, 0);
    assert!(!executor.monitors_armed());
    for _ in 0..3 {
        let _ = executor.spin_once(core::time::Duration::from_millis(0));
        executor.violations.record(startup_verdict(57_751));
    }
    assert!(!executor.monitors_armed(), "nothing asked yet");
    assert_eq!(executor.violations_suppressed_before_arm(), 3);
    assert_eq!(
        executor.violations_total(),
        0,
        "nothing stored before arming"
    );
    let mut ring = 0;
    executor.drain_violations(|_| ring += 1);
    assert_eq!(ring, 0);

    super::monitor::request_monitor_arming();
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    assert!(
        executor.monitors_armed(),
        "the request armed it at the next spin"
    );
    executor.violations.record(startup_verdict(20_001));
    let mut kept = alloc::vec::Vec::new();
    executor.drain_violations_numbered(|seq, v| kept.push((seq, v.measured)));
    assert_eq!(kept, [(1, 20_001)]);
    assert_eq!(
        executor.violations_suppressed_before_arm(),
        3,
        "kept, not reset"
    );

    // A request made before an executor existed does not arm it.
    let mut later: Executor = executor_with_clock(MockSession::new());
    later.set_monitor_arming(true, 0);
    let _ = later.spin_once(core::time::Duration::from_millis(0));
    assert!(!later.monitors_armed());
    later.arm_monitors();
    assert!(later.monitors_armed(), "the executor's own call arms it");
}

private_test_clock!(arm_grace_clock);

/// phase-474 I2 -- a grace period arms the monitors that long after the first
/// spin, for an application with no lifecycle to call from.
#[test]
fn a_grace_period_arms_after_the_first_spin() {
    let _g = arming_lock();
    arm_grace_clock::claim_at_us(1_000_000);
    let mut executor: Executor =
        executor_with_clock_fn(MockSession::new(), arm_grace_clock::now_us);
    executor.set_report_violations(false);
    executor.set_monitor_arming(false, 200);
    assert!(!executor.monitors_armed());
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    executor.violations.record(startup_verdict(1));
    arm_grace_clock::set_us(1_150_000);
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    assert!(!executor.monitors_armed(), "150 ms < 200 ms grace");
    arm_grace_clock::set_us(1_200_000);
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    assert!(executor.monitors_armed(), "200 ms after the first spin");
    assert_eq!(executor.violations_suppressed_before_arm(), 1);
    assert_eq!(executor.violations_total(), 0);
}

private_test_clock!(t4_overrun_clock);

/// phase-474 T4 -- the end-to-end violation test, on the host executor (the
/// `MockSession` harness every rule test here uses), driving the REAL rules
/// rather than injecting verdicts.
///
/// A node with an INIT/RUN lifecycle, as the safety island's `mrm_handler`
/// has: a 100 ms timer publishes a contracted topic whose node-path budget
/// is 5 ms (`max-latency-runtime`). The overruns stay well inside the period,
/// so `timer-overrun-runtime` has nothing to say and the one verdict is the
/// latency rule's. In INIT its first tick overruns (start-up
/// work), and its third tick enters RUN and calls
/// `monitor::request_monitor_arming()` -- the island's INIT_DONE point. In
/// RUN one tick overruns deliberately, by busy-waiting past the budget.
///
/// Asserted: before arming nothing is stored and the suppressed count
/// advances; after arming exactly ONE violation is stored, with the right
/// rule, endpoint and `measured >= declared`; the drain-and-report hook
/// prints it with its number; the trace markers carry it. Then a burst of
/// overruns overfills the ring and the ring keeps the LATEST.
#[test]
fn t4_an_overrun_after_arming_is_the_one_stored_violation() {
    use super::monitor::{MAX_VIOLATIONS, MonitorSpec, PubMonitorCell};
    use core::{
        cell::RefCell,
        sync::atomic::{AtomicU32, Ordering},
    };
    let _g = arming_lock();
    let _sinks = log_sinks_lock();

    static CELL: PubMonitorCell = PubMonitorCell::new();
    static TABLE: [MonitorSpec; 1] = [MonitorSpec {
        topic: "/t4/status",
        fqn: "/t4/handler/status",
        min_rate_hz_milli: 0,
        max_latency_ms: 5,
        route_trigger: "",
        route_max_latency_ms: 0,
        route_deadline_ms: 0,
        reaction_id: 0,
        mechanism: 0,
        cell: &CELL,
    }];
    // The lifecycle's knobs, set by the test, read by the callback.
    static BUSY_MS: AtomicU32 = AtomicU32::new(0);
    static TICKS: AtomicU32 = AtomicU32::new(0);
    const RUN_AT_TICK: u32 = 3;

    // What the executor logs, captured PROCESS-wide and filtered to this
    // test's endpoint (issue 2102). Not thread-local: a record raised while
    // another thread is delivering is held, and the thread holding the
    // dispatch guard delivers it (`nros_log`'s `drain_and_release`, issue
    // 2000). Under a loaded parallel run that was another test's thread, so
    // the line landed in ITS buffer and this one read `[]`. The endpoint is
    // unique to this test, so a sibling's record cannot match.
    static LINES: std::sync::Mutex<alloc::vec::Vec<alloc::string::String>> =
        std::sync::Mutex::new(alloc::vec::Vec::new());
    const ENDPOINT: &str = "/t4/handler/status";
    let lines = || LINES.lock().unwrap().clone();
    // The trace markers ARE thread-local: the executor calls the trace sink
    // inline, on the thread that spins it.
    std::thread_local! {
        static MARKS: RefCell<alloc::vec::Vec<(u32, u32)>> =
            const { RefCell::new(alloc::vec::Vec::new()) };
    }
    struct Capture;
    impl nros_log::LogSink for Capture {
        fn log(&self, r: &nros_log::Record<'_>) {
            let msg = r.message;
            if msg.starts_with("contract violation") && msg.contains(ENDPOINT) {
                LINES.lock().unwrap().push(alloc::string::String::from(msg));
            }
        }
    }
    static CAPTURE: Capture = Capture;
    static SINKS: &[&dyn nros_log::LogSink] = &[&CAPTURE];
    nros_log::init(SINKS);
    // Issue 1939 -- the sink stays installed after this test returns, and
    // libtest REUSES its worker threads. Start from empty.
    LINES.lock().unwrap().clear();
    MARKS.with(|m| m.borrow_mut().clear());
    let lost_at_start = nros_log::early::overflowed();
    #[cfg(feature = "trace-callbacks")]
    let _trace_guard = trace_sink_lock();
    #[cfg(feature = "trace-callbacks")]
    {
        unsafe extern "C" fn mark(id: u32, arg: u32) {
            if (21..=24).contains(&id) {
                MARKS.with(|m| m.borrow_mut().push((id, arg)));
            }
        }
        super::callback_trace::set_trace_sink(Some(mark));
    }

    // A PRIVATE clock the test moves by hand (issue 1939): the latency rule
    // judges 5 ms, and on the shared real-time clock a quiet tick preempted
    // for 5 ms under a parallel gate was a second, spurious violation. Here a
    // callback's "work" advances the clock by exactly its duration, so every
    // measured latency is a constant of the test.
    t4_overrun_clock::claim_at_us(1_000_000);
    fn work_ms(ms: u64) {
        t4_overrun_clock::set_us(t4_overrun_clock::now_us() + ms * 1_000);
    }
    fn step(executor: &mut Executor, ms: u64) {
        work_ms(ms);
        let _ = executor.spin_once(core::time::Duration::from_millis(0));
    }
    let mut executor: Executor =
        executor_with_clock_fn(MockSession::new(), t4_overrun_clock::now_us);
    executor.set_monitor_table(&TABLE);
    executor.set_monitor_arming(true, 0);
    executor.set_violation_drain_report(true);
    let nid = executor.node_builder("handler").build().expect("node");
    let status = executor
        .node_mut(nid)
        .create_generic_publisher("/t4/status", "std_msgs::msg::dds_::Int32_", "")
        .expect("contracted publisher");
    BUSY_MS.store(20, Ordering::SeqCst); // INIT's first tick: start-up work
    TICKS.store(0, Ordering::SeqCst);
    executor
        .register_timer(TimerDuration::from_millis(100), move || {
            let tick = TICKS.fetch_add(1, Ordering::SeqCst) + 1;
            let busy = BUSY_MS.swap(0, Ordering::SeqCst);
            if busy > 0 {
                work_ms(busy as u64);
            }
            let _ = status.publish_serialized(&[0, 1, 0, 0, tick as u8, 0, 0, 0]);
            if tick == RUN_AT_TICK {
                // INIT -> RUN: every input established. The island's handler
                // writes INIT_DONE here.
                super::monitor::request_monitor_arming();
            }
        })
        .expect("timer");

    // INIT: three ticks, the first one overrunning, and the spin after the
    // third arms.
    let mut spins = 0;
    while TICKS.load(Ordering::SeqCst) < RUN_AT_TICK && spins < 40 {
        step(&mut executor, 102);
        spins += 1;
    }
    assert!(
        !executor.monitors_armed(),
        "the request is seen at the NEXT spin"
    );
    let suppressed = executor.violations_suppressed_before_arm();
    assert!(
        suppressed >= 1,
        "INIT's overrun was judged and suppressed: {suppressed}"
    );
    assert_eq!(
        executor.violations_total(),
        0,
        "nothing stored before arming"
    );
    nros_log::flush();
    assert!(lines().is_empty(), "nothing reported: {:?}", lines());

    step(&mut executor, 102);
    assert!(executor.monitors_armed(), "RUN armed the monitors");
    assert_eq!(executor.violations_total(), 0);

    // RUN: one deliberate overrun, then quiet ticks until it is judged.
    BUSY_MS.store(20, Ordering::SeqCst);
    for _ in 0..4 {
        step(&mut executor, 102);
    }
    assert_eq!(
        executor.violations_total(),
        1,
        "exactly one violation stored after arming: {:?}",
        lines()
    );
    assert_eq!(executor.violations_dropped(), 0);
    assert_eq!(
        executor.violations_suppressed_before_arm(),
        suppressed,
        "nothing more suppressed once armed"
    );
    // A held record is delivered by whichever thread holds the dispatch guard,
    // so give it a bounded moment to arrive. A record that did not fit the
    // hold queue is LOST and counted (issue 2000), and no wait brings it
    // back; say so rather than reporting an empty capture as the hook's fault.
    let deadline = std::time::Instant::now() + core::time::Duration::from_secs(2);
    while lines().is_empty() && std::time::Instant::now() < deadline {
        nros_log::flush();
        std::thread::yield_now();
    }
    let lines = lines();
    assert_eq!(
        lines.len(),
        1,
        "the drain hook printed it once: {lines:?} ({} record(s) lost to nros-log's hold \
         queue during this test)",
        nros_log::early::overflowed() - lost_at_start
    );
    let line = &lines[0];
    assert!(
        line.starts_with("contract violation #1: max-latency-runtime /t4/handler/status measured="),
        "{line}"
    );
    let measured: u32 = line
        .split("measured=")
        .nth(1)
        .and_then(|t| t.split(' ').next())
        .and_then(|n| n.parse().ok())
        .expect("measured in the line");
    assert!(measured >= 5, "measured {measured} >= declared 5: {line}");
    assert!(line.contains("declared=5 (total=1 dropped=0)"), "{line}");
    #[cfg(feature = "trace-callbacks")]
    {
        let marks = MARKS.with(|m| m.borrow().clone());
        assert_eq!(
            marks,
            [
                (
                    21,
                    (1 << 8) | super::monitor::rule_code("max-latency-runtime")
                ),
                (22, super::monitor::fqn_hash("/t4/handler/status")),
                (23, measured),
                (24, 5),
            ],
            "one violation, four markers"
        );
    }

    // Overfill: overruns on alternate ticks (a quiet tick between is the
    // rule's recovery), with the drain hook off so the ring holds them.
    executor.set_violation_drain_report(false);
    let burst = MAX_VIOLATIONS as u32 + 2;
    for _ in 0..burst {
        BUSY_MS.store(8, Ordering::SeqCst);
        step(&mut executor, 102);
        step(&mut executor, 102);
    }
    step(&mut executor, 102);
    let total = executor.violations_total();
    // Every overrun is stored. The timer can also drop an activation over a
    // long run (each spin lands 2 ms past the period, and the drift adds up),
    // which `timer-overrun-runtime` then reports as well; that is a real
    // verdict, so the count is a floor and the ring's arithmetic is checked
    // against the total.
    assert!(total > burst, "every overrun stored: total={total}");
    let dropped = executor.violations_dropped();
    assert_eq!(
        dropped,
        total - 1 - MAX_VIOLATIONS as u32,
        "the ring held all but #1 (drained by the hook) and evicted the OLDEST"
    );
    let mut kept = alloc::vec::Vec::new();
    let mut latency = 0;
    executor.drain_violations_numbered(|seq, v| {
        assert!(
            v.rule == "max-latency-runtime" || v.rule == "timer-overrun-runtime",
            "{seq}: {}",
            v.rule
        );
        assert!(v.measured > v.declared || v.rule == "max-latency-runtime");
        assert!(v.measured >= v.declared);
        latency += (v.rule == "max-latency-runtime") as u32;
        kept.push(seq);
    });
    let want: alloc::vec::Vec<u32> = (total + 1 - MAX_VIOLATIONS as u32..=total).collect();
    assert_eq!(kept, want, "the ring keeps the latest");
    assert!(latency >= MAX_VIOLATIONS as u32 - 2, "{latency}");

    #[cfg(feature = "trace-callbacks")]
    super::callback_trace::set_trace_sink(None);
}

private_test_clock!(t4_idle_sub_clock);

/// phase-474 I6 -- T4's overrun again, on the shape the safety island actually
/// has: the timer's node ALSO subscribes. With a subscription registered and
/// idle (no sample this spin), the default `Trigger::Any` does not pass, and
/// due timers fire in the trigger-miss sweep of `spin_once` instead of the
/// measured drains. That sweep used to time nothing, so `attribute_latency`
/// never saw the dispatch and `max-latency-runtime` never judged a timer path
/// on the island (250 ms overrun of a 206 ms path, no verdict). T4 above
/// passed because its node has no subscription, so its trigger always passes.
///
/// Asserted: the timer's 20 ms overrun of a 5 ms path, after arming, is ONE
/// stored `max-latency-runtime` verdict on the publisher the timer outputs,
/// with `measured >= declared`, and the timer really fired from the sweep
/// (the subscription never had data).
#[test]
fn t4_a_timer_overrun_beside_an_idle_subscription_is_judged() {
    use super::monitor::{MonitorSpec, PubMonitorCell};
    use core::sync::atomic::{AtomicU32, Ordering};
    let _g = arming_lock();

    static CELL: PubMonitorCell = PubMonitorCell::new();
    static TABLE: [MonitorSpec; 1] = [MonitorSpec {
        topic: "/t4i6/mrm_state",
        fqn: "/t4i6/handler/mrm_state",
        min_rate_hz_milli: 0,
        max_latency_ms: 5,
        route_trigger: "",
        route_max_latency_ms: 0,
        route_deadline_ms: 0,
        reaction_id: 0,
        mechanism: 0,
        cell: &CELL,
    }];
    static BUSY_MS: AtomicU32 = AtomicU32::new(0);
    static TICKS: AtomicU32 = AtomicU32::new(0);
    static TAKES: AtomicU32 = AtomicU32::new(0);

    // A PRIVATE clock the test moves by hand (issue 1939): a quiet tick preempted for 5 ms under a
    // parallel gate would be a spurious verdict against the 5 ms budget.
    t4_idle_sub_clock::claim_at_us(1_000_000);
    fn work_ms(ms: u64) {
        t4_idle_sub_clock::set_us(t4_idle_sub_clock::now_us() + ms * 1_000);
    }
    fn step(executor: &mut Executor, ms: u64) {
        work_ms(ms);
        let _ = executor.spin_once(core::time::Duration::from_millis(0));
    }
    let mut executor: Executor =
        executor_with_clock_fn(MockSession::new(), t4_idle_sub_clock::now_us);
    executor.set_monitor_table(&TABLE);
    executor.set_report_violations(false);
    let nid = executor.node_builder("handler").build().expect("node");
    // The island's handler subscribes to its inputs; none arrives here.
    executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/t4i6/availability", move |_m: &TestMsg| {
            TAKES.fetch_add(1, Ordering::SeqCst);
        })
        .expect("idle subscription");
    let state = executor
        .node_mut(nid)
        .create_generic_publisher("/t4i6/mrm_state", "std_msgs::msg::dds_::Int32_", "")
        .expect("contracted publisher");
    TICKS.store(0, Ordering::SeqCst);
    BUSY_MS.store(0, Ordering::SeqCst);
    executor
        .register_timer(TimerDuration::from_millis(100), move || {
            let tick = TICKS.fetch_add(1, Ordering::SeqCst) + 1;
            let _ = state.publish_serialized(&[0, 1, 0, 0, tick as u8, 0, 0, 0]);
            // The island's debug overrun busy-waits AFTER its publishes.
            let busy = BUSY_MS.swap(0, Ordering::SeqCst);
            if busy > 0 {
                work_ms(busy as u64);
            }
        })
        .expect("timer");

    // Armed at the first spin (the default); two quiet ticks first.
    let mut spins = 0;
    while TICKS.load(Ordering::SeqCst) < 2 && spins < 40 {
        step(&mut executor, 102);
        spins += 1;
    }
    assert!(executor.monitors_armed());
    assert_eq!(executor.violations_total(), 0, "quiet ticks are clean");

    // One deliberate overrun, then quiet ticks until it is judged.
    BUSY_MS.store(20, Ordering::SeqCst);
    let fired_before = TICKS.load(Ordering::SeqCst);
    for _ in 0..4 {
        step(&mut executor, 102);
    }
    assert!(
        TICKS.load(Ordering::SeqCst) > fired_before,
        "the timer kept firing"
    );
    assert_eq!(
        TAKES.load(Ordering::SeqCst),
        0,
        "the subscription stayed idle"
    );
    let mut kept = alloc::vec::Vec::new();
    executor.drain_violations_numbered(|seq, v| {
        kept.push((seq, v.rule, v.fqn, v.measured, v.declared));
    });
    let latency: alloc::vec::Vec<_> = kept
        .iter()
        .filter(|k| k.1 == "max-latency-runtime")
        .collect();
    assert_eq!(
        latency.len(),
        1,
        "one max-latency-runtime verdict for the timer's overrun: {kept:?}"
    );
    let (_, _, fqn, measured, declared) = *latency[0];
    assert_eq!(fqn, "/t4i6/handler/mrm_state");
    assert_eq!(declared, 5);
    assert!(measured >= declared, "measured {measured} >= declared 5");
}

// ---- phase-482 W5 — `-p` / `--params-file` overrides ----------------------

/// Parse `argv` the way `nros::init_with_args` / `rclcpp::init(argc, argv)` do
/// and install its parameter overrides, from the command line and from a file
/// whose TEXT `file` stands in for.
#[cfg(feature = "param-store")]
fn install_overrides(executor: &mut Executor<'static>, argv: &[&str], file: &str) {
    let mut params = alloc::vec::Vec::new();
    let mut files = alloc::vec::Vec::new();
    crate::ros_args::parse_ros_args_events(argv.iter().copied(), |a| match a {
        crate::ros_args::RosArg::Param(p) => params.push((p.node, p.name, p.value)),
        crate::ros_args::RosArg::ParamsFile(path) => files.push(path),
        crate::ros_args::RosArg::Remap(_) => {}
    })
    .unwrap();
    let mut owned: alloc::vec::Vec<(
        Option<alloc::string::String>,
        alloc::string::String,
        alloc::string::String,
    )> = alloc::vec::Vec::new();
    for _ in &files {
        crate::ros_args::parse_params_yaml(file, |p| {
            owned.push((
                p.node.map(alloc::string::String::from),
                alloc::string::String::from(p.name),
                alloc::string::String::from(p.value),
            ))
        })
        .unwrap();
    }
    // argv order: the file's values where the flag stood are approximated by
    // installing them first; these tests do not mix the two for one name.
    executor
        .install_argv_params(owned.iter().map(|(n, k, v)| crate::ros_args::ParamArg {
            node: n.as_deref(),
            name: k,
            value: v,
        }))
        .unwrap();
    executor
        .install_argv_params(
            params
                .iter()
                .map(|&(node, name, value)| crate::ros_args::ParamArg { node, name, value }),
        )
        .unwrap();
}

/// `-p rate:=25` replaces the declared default, typed by it; a later `-p` for
/// the same name wins; a `node:` override reaches only the node of that name.
#[cfg(feature = "param-store")]
#[test]
fn argv_p_overrides_the_declared_default_per_node_and_last_wins() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let talker = executor.node_builder("talker").build().unwrap();
    let listener = executor.node_builder("listener").build().unwrap();
    install_overrides(
        &mut executor,
        &[
            "prog",
            "--ros-args",
            "-p",
            "rate:=1",
            "-p",
            "rate:=25",
            "-p",
            "listener:rate:=40",
            "-p",
            "enabled:=false",
        ],
        "",
    );

    assert!(executor.declare_parameter_on(
        talker,
        "rate",
        nros_params::ParameterValue::Integer(10)
    ));
    assert!(executor.declare_parameter_on(
        listener,
        "rate",
        nros_params::ParameterValue::Integer(10)
    ));
    assert!(executor.declare_parameter_on(
        talker,
        "enabled",
        nros_params::ParameterValue::Bool(true)
    ));
    assert!(executor.declare_parameter_on(
        talker,
        "other",
        nros_params::ParameterValue::Integer(3)
    ));

    let get = |e: &Executor, n, k| e.get_parameter_on(n, k).cloned();
    assert_eq!(
        get(&executor, talker, "rate").and_then(|v| v.as_integer()),
        Some(25),
        "last -p wins"
    );
    assert_eq!(
        get(&executor, listener, "rate").and_then(|v| v.as_integer()),
        Some(40),
        "`listener:` reaches the listener, and comes later"
    );
    assert_eq!(
        get(&executor, talker, "enabled").and_then(|v| v.as_bool()),
        Some(false)
    );
    assert_eq!(
        get(&executor, talker, "other").and_then(|v| v.as_integer()),
        Some(3),
        "no override, default"
    );
}

/// An override that does not parse as the declared type is not applied: the
/// default stands (RFC-0015 §9.6), and the declaration still succeeds.
#[cfg(feature = "param-store")]
#[test]
fn argv_p_with_the_wrong_type_keeps_the_default() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let talker = executor.node_builder("talker").build().unwrap();
    install_overrides(
        &mut executor,
        &["prog", "--ros-args", "-p", "rate:=fast"],
        "",
    );
    assert!(executor.declare_parameter_on(
        talker,
        "rate",
        nros_params::ParameterValue::Integer(10)
    ));
    assert_eq!(
        executor
            .get_parameter_on(talker, "rate")
            .and_then(|v| v.as_integer()),
        Some(10)
    );
}

/// Issue 2057 — the overrides a node would receive are listable before any
/// declaration: one per name, the last one winning, typed with no declared
/// type to follow, and a `node:` override listed only for that node.
#[cfg(feature = "param-store")]
#[test]
fn argv_overrides_are_listed_per_node_last_winning() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let talker = executor.node_builder("talker").build().unwrap();
    let listener = executor.node_builder("listener").build().unwrap();
    install_overrides(
        &mut executor,
        &[
            "prog",
            "--ros-args",
            "-p",
            "a:=1",
            "-p",
            "listener:b:=x",
            "-p",
            "a:=2",
            "-p",
            "listener:a:=2.5",
        ],
        "",
    );
    // `ParameterValue` has no `PartialEq`; its `Debug` names the variant and
    // the value, which is what is being asserted.
    let list = |e: &Executor, n| {
        e.argv_param_overrides_on(n)
            .map(|(k, v)| alloc::format!("{k}={v:?}"))
            .collect::<alloc::vec::Vec<_>>()
    };
    assert_eq!(
        list(&executor, talker),
        ["a=Integer(2)"],
        "the last global `a` wins; `listener:` overrides are not the talker's"
    );
    assert_eq!(
        list(&executor, listener),
        ["b=String(\"x\")", "a=Double(2.5)"],
        "the listener's own `a` comes last, so it wins over both global ones"
    );
    // Issue 2104 -- the borrowing walk lists the same overrides, in the same
    // order, and counts them.
    for n in [talker, listener] {
        let mut walked = alloc::vec::Vec::new();
        let count = executor.for_each_argv_param_override_on(n, |k, v| {
            walked.push(alloc::format!("{k}={v:?}"));
        });
        assert_eq!(walked, list(&executor, n));
        assert_eq!(count, walked.len());
    }
}

/// `--params-file`: `/**` reaches every node, a node key only that node, and
/// nested keys become dotted names; strings, doubles and quoted values type by
/// the declaration.
#[cfg(feature = "param-store")]
#[test]
fn params_file_overrides_reach_their_nodes_with_dotted_names() {
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let talker = executor.node_builder("talker").build().unwrap();
    let listener = executor.node_builder("listener").build().unwrap();
    let file = "\
# a ROS 2 parameter file
/**:
  ros__parameters:
    frame: \"base_link\"   # every node
/robot/talker:
  ros__parameters:
    gains:
      kp: 1.5
    rate: 30
";
    install_overrides(
        &mut executor,
        &["prog", "--ros-args", "--params-file", "p.yaml"],
        file,
    );

    for n in [talker, listener] {
        assert!(executor.declare_parameter_on(
            n,
            "frame",
            nros_params::ParameterValue::String(heapless::String::try_from("map").unwrap())
        ));
    }
    assert!(executor.declare_parameter_on(
        talker,
        "gains.kp",
        nros_params::ParameterValue::Double(0.0)
    ));
    assert!(executor.declare_parameter_on(
        talker,
        "rate",
        nros_params::ParameterValue::Integer(10)
    ));
    assert!(executor.declare_parameter_on(
        listener,
        "rate",
        nros_params::ParameterValue::Integer(10)
    ));

    assert_eq!(
        executor
            .get_parameter_on(listener, "frame")
            .and_then(|v| v.as_string().map(alloc::string::String::from)),
        Some(alloc::string::String::from("base_link"))
    );
    assert_eq!(
        executor
            .get_parameter_on(talker, "gains.kp")
            .and_then(|v| v.as_double()),
        Some(1.5)
    );
    assert_eq!(
        executor
            .get_parameter_on(talker, "rate")
            .and_then(|v| v.as_integer()),
        Some(30)
    );
    assert_eq!(
        executor
            .get_parameter_on(listener, "rate")
            .and_then(|v| v.as_integer()),
        Some(10),
        "`/robot/talker:` does not reach the listener"
    );
}
private_test_clock!(i7_report_clock);

/// phase-474 I7 -- the `/diagnostics` report leaves OFF the judged tick, and
/// what it costs is not judged as the spin's lateness.
///
/// The safety island's board showed the reporter at detection: each report
/// held the spin 15-20 ms over its serial link, inside the tick being judged,
/// and five in a row (81 ms) tripped `timer-overrun-runtime` on a 30 Hz timer
/// and `release-jitter-runtime` on the spin -- one verdict bred more.
///
/// Here a handler timer (80 ms) overruns its 5 ms latency budget once, on two
/// contracted publishers, so two verdicts are pending at once; a second timer
/// (40 ms) is due on the same grid; the sink fakes a slow link (40 ms per
/// message). Sent at detection, one message per verdict, that is 80 ms with
/// the 40 ms timer waiting -- a dropped activation -- and an 80 ms spin
/// interval against a 30 ms cadence. Asserted: both latency verdicts reach the
/// sink in ONE coalesced call, and the ring holds no overrun or jitter verdict.
#[test]
fn i7_a_slow_report_breeds_no_overrun_or_jitter_verdict() {
    use super::monitor::{MonitorSpec, PubMonitorCell, Violation};
    use core::sync::atomic::{AtomicU32, Ordering};
    let _g = arming_lock();

    static CELL_A: PubMonitorCell = PubMonitorCell::new();
    static CELL_B: PubMonitorCell = PubMonitorCell::new();
    static TABLE: [MonitorSpec; 2] = [
        MonitorSpec {
            topic: "/i7/mrm_state",
            fqn: "/i7/handler/mrm_state",
            min_rate_hz_milli: 0,
            max_latency_ms: 5,
            route_trigger: "",
            route_max_latency_ms: 0,
            route_deadline_ms: 0,
            reaction_id: 0,
            mechanism: 0,
            cell: &CELL_A,
        },
        MonitorSpec {
            topic: "/i7/takeover_request_state",
            fqn: "/i7/handler/takeover_request_state",
            min_rate_hz_milli: 0,
            max_latency_ms: 5,
            route_trigger: "",
            route_max_latency_ms: 0,
            route_deadline_ms: 0,
            reaction_id: 0,
            mechanism: 0,
            cell: &CELL_B,
        },
    ];
    static BUSY_MS: AtomicU32 = AtomicU32::new(0);
    static HANDLER_TICKS: AtomicU32 = AtomicU32::new(0);
    static SINK_CALLS: AtomicU32 = AtomicU32::new(0);
    static SINK_SEEN: AtomicU32 = AtomicU32::new(0);

    // A PRIVATE clock the test moves by hand (issue 1939). "Work" -- the
    // handler's overrun, the slow link -- advances it by exactly its duration,
    // so no verdict here depends on how the host scheduled the test thread:
    // on the shared real-time clock a 2 ms sleep preempted past 40 ms under a
    // parallel gate dropped an activation and bred the overrun this test
    // asserts the report does NOT breed.
    fn busy_ms(ms: u64) {
        i7_report_clock::set_us(i7_report_clock::now_us() + ms * 1_000);
    }
    fn step(executor: &mut Executor, ms: u64) {
        busy_ms(ms);
        let _ = executor.spin_once(core::time::Duration::from_millis(0));
    }
    i7_report_clock::claim_at_us(1_000_000);
    unsafe fn slow_sink(_ctx: *mut core::ffi::c_void, pending: &[Violation]) -> usize {
        SINK_CALLS.fetch_add(1, Ordering::SeqCst);
        SINK_SEEN.fetch_add(pending.len() as u32, Ordering::SeqCst);
        // One message over a slow link, whatever it carries.
        busy_ms(40);
        pending.len()
    }

    let mut executor: Executor =
        executor_with_clock_fn(MockSession::new(), i7_report_clock::now_us);
    executor.set_monitor_table(&TABLE);
    executor.set_report_violations(false);
    // The cadence the release-jitter rule judges: 30 ms.
    executor.set_spin_nominal_us(30_000);
    let nid = executor.node_builder("handler").build().expect("node");
    let state = executor
        .node_mut(nid)
        .create_generic_publisher("/i7/mrm_state", "std_msgs::msg::dds_::Int32_", "")
        .expect("contracted publisher");
    let tor = executor
        .node_mut(nid)
        .create_generic_publisher(
            "/i7/takeover_request_state",
            "std_msgs::msg::dds_::Int32_",
            "",
        )
        .expect("contracted publisher");
    HANDLER_TICKS.store(0, Ordering::SeqCst);
    BUSY_MS.store(0, Ordering::SeqCst);
    SINK_CALLS.store(0, Ordering::SeqCst);
    SINK_SEEN.store(0, Ordering::SeqCst);
    executor
        .register_timer(TimerDuration::from_millis(80), move || {
            let tick = HANDLER_TICKS.fetch_add(1, Ordering::SeqCst) + 1;
            let _ = state.publish_serialized(&[0, 1, 0, 0, tick as u8, 0, 0, 0]);
            let _ = tor.publish_serialized(&[0, 1, 0, 0, tick as u8, 0, 0, 0]);
            let busy = BUSY_MS.swap(0, Ordering::SeqCst);
            if busy > 0 {
                busy_ms(busy as u64);
            }
        })
        .expect("handler timer");
    let operator = executor
        .register_timer(TimerDuration::from_millis(40), || {})
        .expect("operator timer");
    unsafe { executor.set_violation_sink(Some((slow_sink, core::ptr::null_mut()))) };

    // Two quiet handler ticks: nothing to report.
    let mut spins = 0;
    while HANDLER_TICKS.load(Ordering::SeqCst) < 2 && spins < 400 {
        step(&mut executor, 2);
        spins += 1;
    }
    assert_eq!(executor.violations_total(), 0, "quiet ticks are clean");

    // One overrun of the handler's tick, then ~400 ms of ordinary spins.
    BUSY_MS.store(10, Ordering::SeqCst);
    for _ in 0..200 {
        step(&mut executor, 2);
    }

    let mut kept = alloc::vec::Vec::new();
    executor.drain_violations(|v| kept.push((v.rule, v.fqn, v.measured)));
    let latency = kept.iter().filter(|k| k.0 == "max-latency-runtime").count();
    assert_eq!(
        latency, 2,
        "one latency verdict per contracted publisher: {kept:?}"
    );
    assert!(
        !kept
            .iter()
            .any(|k| k.0 == "timer-overrun-runtime" || k.0 == "release-jitter-runtime"),
        "the report itself bred a verdict: {kept:?}"
    );
    assert_eq!(
        executor.timer_overruns(operator),
        Some(0),
        "the 40 ms timer kept its grid"
    );
    assert_eq!(
        SINK_CALLS.load(Ordering::SeqCst),
        1,
        "both verdicts went in one coalesced report"
    );
    assert_eq!(SINK_SEEN.load(Ordering::SeqCst), 2);
    let (last_us, max_us, pending, overflowed) = executor.violation_report_cost_us();
    assert!(
        last_us >= 40_000 && max_us >= last_us,
        "the cost is stated: {last_us} {max_us}"
    );
    assert_eq!((pending, overflowed), (0, 0));
}

/// phase-474 I3 -- tests that install the process-global trace sink or touch
/// the per-slot trace settings. Serialised so one cannot swap the other's sink
/// or settings out mid-assertion.
#[cfg(feature = "trace-callbacks")]
fn trace_sink_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// phase-474 I3 -- a subscription opted in to take tracing emits one TAKE
/// event per sample, before its callback's start, and the stamp pair when the
/// slot names where the stamp is; a subscription not opted in emits none.
#[cfg(feature = "trace-callbacks")]
#[test]
fn i3_an_opted_in_subscription_traces_each_take_and_its_stamp() {
    use super::callback_trace::{
        MARKER_START, MARKER_TAKE, MARKER_TAKE_STAMP_NSEC, MARKER_TAKE_STAMP_SEC,
        reset_trace_settings, set_take_trace, set_trace_sink,
    };
    use core::cell::RefCell;
    let _g = trace_sink_lock();
    std::thread_local! {
        static EVENTS: RefCell<alloc::vec::Vec<(u32, u32)>> =
            const { RefCell::new(alloc::vec::Vec::new()) };
    }
    unsafe extern "C" fn sink(id: u32, arg: u32) {
        if (18..=19).contains(&id) || (25..=27).contains(&id) {
            EVENTS.with(|e| e.borrow_mut().push((id, arg)));
        }
    }
    set_trace_sink(Some(sink));
    reset_trace_settings();

    let mut executor: Executor = executor_with_clock(MockSession::new());
    let nid = executor.node_builder("i3_take").build().unwrap();
    executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/i3/in", |_m: &TestMsg| {})
        .unwrap();
    let meta = executor.entries[0].as_ref().unwrap();
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    let sub_ptr = unsafe { arena_ptr.add(meta.offset) } as *const MockSubscriber;

    // Not opted in (the image default is off): no take event.
    let (data, len) = encode_test_msg(42);
    unsafe { &*sub_ptr }.load(data, len);
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    let seen = EVENTS.with(|e| core::mem::take(&mut *e.borrow_mut()));
    assert!(
        seen.iter().any(|&(id, a)| id == MARKER_START && a == 0),
        "{seen:?}"
    );
    assert!(!seen.iter().any(|&(id, _)| id == MARKER_TAKE), "{seen:?}");

    // Opted in, with the "stamp" at byte 0: the encapsulation word and the
    // value stand in for sec and nanosec.
    set_take_trace(0, true, Some(0));
    unsafe { &*sub_ptr }.load(data, len);
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    let seen = EVENTS.with(|e| core::mem::take(&mut *e.borrow_mut()));
    let take = seen
        .iter()
        .position(|&(id, _)| id == MARKER_TAKE)
        .expect("a take");
    assert_eq!(seen[take].1 >> 24, 0, "the take carries the slot");
    let sec = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    assert_eq!(seen[take + 1], (MARKER_TAKE_STAMP_SEC, sec));
    assert_eq!(seen[take + 2], (MARKER_TAKE_STAMP_NSEC, 42));
    assert_eq!(
        seen[take + 3],
        (MARKER_START, 0),
        "the take precedes the start"
    );

    // Off again.
    set_take_trace(0, false, None);
    unsafe { &*sub_ptr }.load(data, len);
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    let seen = EVENTS.with(|e| core::mem::take(&mut *e.borrow_mut()));
    assert!(!seen.iter().any(|&(id, _)| id == MARKER_TAKE), "{seen:?}");
    reset_trace_settings();
    set_trace_sink(None);
}

#[cfg(feature = "trace-callbacks")]
private_test_clock!(i3_thinned_clock);

/// phase-474 I3 -- a thinned timer traces one tick in N; the default traces
/// every tick.
#[cfg(feature = "trace-callbacks")]
#[test]
fn i3_a_thinned_timer_traces_one_tick_in_n() {
    i3_thinned_clock::claim_at_us(1_000_000);
    use super::callback_trace::{
        MARKER_END, MARKER_START, reset_trace_settings, set_timer_trace_every, set_trace_sink,
    };
    use core::{
        cell::RefCell,
        sync::atomic::{AtomicU32, Ordering},
    };
    let _g = trace_sink_lock();
    std::thread_local! {
        static STARTS: RefCell<u32> = const { RefCell::new(0) };
        static ENDS: RefCell<u32> = const { RefCell::new(0) };
    }
    unsafe extern "C" fn sink(id: u32, arg: u32) {
        if arg == 0 && id == MARKER_START {
            STARTS.with(|s| *s.borrow_mut() += 1);
        }
        if arg == 0 && id == MARKER_END {
            ENDS.with(|s| *s.borrow_mut() += 1);
        }
    }
    set_trace_sink(Some(sink));
    reset_trace_settings();
    static TICKS: AtomicU32 = AtomicU32::new(0);
    TICKS.store(0, Ordering::SeqCst);

    let mut executor: Executor =
        executor_with_clock_fn(MockSession::new(), i3_thinned_clock::now_us);
    // A late tick is not what this asserts; keep the process-wide log quiet.
    executor.set_report_violations(false);
    executor
        .register_timer(TimerDuration::from_millis(5), || {
            TICKS.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap();
    let tick_until = |executor: &mut Executor, n: u32| {
        let mut spins = 0;
        while TICKS.load(Ordering::SeqCst) < n && spins < 400 {
            let _ = i3_thinned_clock::elapse_then_spin_once(executor, 6);
            spins += 1;
        }
    };
    tick_until(&mut executor, 3);
    assert_eq!(
        STARTS.with(|s| *s.borrow()),
        3,
        "the default traces every tick"
    );

    set_timer_trace_every(0, 3);
    STARTS.with(|s| *s.borrow_mut() = 0);
    ENDS.with(|s| *s.borrow_mut() = 0);
    tick_until(&mut executor, 12);
    assert_eq!(STARTS.with(|s| *s.borrow()), 3, "one tick in three of nine");
    assert_eq!(ENDS.with(|s| *s.borrow()), 3, "start and end stay paired");

    set_timer_trace_every(0, 0);
    STARTS.with(|s| *s.borrow_mut() = 0);
    tick_until(&mut executor, 15);
    assert_eq!(STARTS.with(|s| *s.borrow()), 0, "0 = none");
    reset_trace_settings();
    set_trace_sink(None);
}

// Issue 2023 — the decoder both `/parameter_events` tests read frames with.
/// `(name, type, integer)` of the one parameter a slot carries.
#[cfg(feature = "param-services")]
type Slot = Option<(alloc::string::String, u8, i64)>;
/// `(stamp_sec, node, [new, changed, deleted])`.
#[cfg(feature = "param-services")]
type Decoded = (i32, alloc::string::String, [Slot; 3]);

#[cfg(feature = "param-services")]
fn decode(bytes: &[u8]) -> Decoded {
    use nros_core::{CdrReader, Deserialize};
    use nros_rcl_interfaces::msg::Parameter;
    let mut r = CdrReader::new(&bytes[4..]);
    let sec = r.read_i32().unwrap();
    let _nanosec = r.read_u32().unwrap();
    let node = alloc::string::String::from(r.read_string().unwrap());
    let mut slots: [Slot; 3] = [None, None, None];
    for slot in &mut slots {
        let n = r.read_sequence_len().unwrap();
        assert!(n <= 1, "one parameter per event");
        if n == 1 {
            let p = Box::new(Parameter::deserialize(&mut r).unwrap());
            *slot = Some((
                alloc::string::String::from(p.name.as_str()),
                p.value.type_,
                p.value.integer_value,
            ));
        }
    }
    (sec, node, slots)
}

/// phase-491 W3 (RFC-0105 D8) — every change on a node that asked for events
/// is published as ONE `rcl_interfaces/msg/ParameterEvent` carrying one
/// parameter, in the slot its kind names, with the value the store holds.
/// Decoded with the GENERATED `Time` / `Parameter` decoders, so this is about
/// the bytes a subscriber receives.
#[cfg(feature = "param-services")]
#[test]
fn parameter_events_publish_each_change_in_its_slot() {
    use nros_params::ParameterValue;

    let mut executor = executor_with_clock(MockSession::new());
    let _talker = executor.node_builder("talker").build().unwrap();
    // Issue 2023 — a declaration made BEFORE events are on is reported by
    // the node's START-UP event, as rclcpp's (whose publisher predates every
    // declaration) would report it.
    assert!(executor.declare_parameter("early", ParameterValue::Integer(1)));
    executor
        .enable_parameter_events_on(NodeId::PRIMARY)
        .expect("one /parameter_events publisher over the mock session");
    executor.flush_parameter_events();
    fn publisher<'a>(executor: &'a Executor<'_>) -> &'a crate::mock::MockPublisher {
        executor
            .params
            .as_ref()
            .and_then(|p| p.event_publisher.as_ref())
            .expect("the publisher exists")
    }
    assert_eq!(publisher(&executor).topic_name(), "/parameter_events");
    let start_up = publisher(&executor).publish_count();
    let early = (0..start_up)
        .filter_map(|i| publisher(&executor).retained(i))
        .map(|(bytes, len)| decode(&bytes[..len]))
        .find(|(_, _, slots)| slots[0].as_ref().is_some_and(|s| s.0 == "early"))
        .expect("the start-up event names `early`");
    assert_eq!(early.1, "/talker");
    assert_eq!(early.2[0], Some(("early".into(), 2, 1)));
    executor.flush_parameter_events();
    assert_eq!(
        publisher(&executor).publish_count(),
        start_up,
        "the start-up event is published once"
    );
    publisher(&executor).clear_published();

    assert!(executor.declare_parameter("rate", ParameterValue::Integer(5)));
    executor.flush_parameter_events();
    let (bytes, len) = publisher(&executor).last_published().unwrap();
    let (_, node, slots) = decode(&bytes[..len]);
    assert_eq!(node, "/talker");
    assert_eq!(
        slots[0],
        Some(("rate".into(), 2, 5)),
        "a declaration is NEW"
    );
    assert!(slots[1].is_none() && slots[2].is_none());

    assert_eq!(
        executor.set_parameter_local_on(NodeId::PRIMARY, "rate", ParameterValue::Integer(6)),
        Ok(())
    );
    executor.flush_parameter_events();
    let (bytes, len) = publisher(&executor).last_published().unwrap();
    let (_, _, slots) = decode(&bytes[..len]);
    assert_eq!(
        slots[1],
        Some(("rate".into(), 2, 6)),
        "a set is CHANGED, with the new value"
    );

    assert!(executor.undeclare_parameter("rate"));
    executor.flush_parameter_events();
    let (bytes, len) = publisher(&executor).last_published().unwrap();
    let (_, _, slots) = decode(&bytes[..len]);
    assert_eq!(
        slots[2],
        Some(("rate".into(), 0, 0)),
        "a deletion names the parameter"
    );
    assert_eq!(publisher(&executor).publish_count(), 3);

    // Two changes to one name between flushes merge into one event.
    assert!(executor.declare_parameter("gain", ParameterValue::Integer(1)));
    let _ = executor.set_parameter_local_on(NodeId::PRIMARY, "gain", ParameterValue::Integer(2));
    executor.flush_parameter_events();
    assert_eq!(publisher(&executor).publish_count(), 4);
    let (bytes, len) = publisher(&executor).last_published().unwrap();
    let (_, _, slots) = decode(&bytes[..len]);
    assert_eq!(
        slots[0],
        Some(("gain".into(), 2, 2)),
        "new-then-set is still NEW, at the latest value"
    );
}

/// Issue 2023 — `register_parameter_services`, the road the C++ and Rust
/// surfaces take, brings `/parameter_events` up with the six services: ONE
/// publisher for every node, each node's START-UP event naming what it holds
/// (its `use_sim_time` too, under `sim-time`, as upstream's), then one event
/// per change — and a node that opted out publishes nothing.
#[cfg(feature = "param-services")]
#[test]
fn registering_parameter_services_publishes_parameter_events_for_every_node() {
    use nros_params::ParameterValue;
    let mut executor = executor_with_clock(MockSession::new());
    executor.set_node_identity("alpha", "/");
    let _alpha = executor.node_builder("alpha").build().unwrap();
    let beta = executor
        .node_builder("beta")
        .namespace("/ns")
        .build()
        .unwrap();
    let gamma = executor.node_builder("gamma").build().unwrap();
    executor.disable_parameter_events_on(gamma);
    assert!(executor.declare_parameter_on(beta, "gain", ParameterValue::Integer(3)));
    assert!(executor.declare_parameter_on(gamma, "quiet", ParameterValue::Integer(0)));
    executor
        .register_parameter_services()
        .expect("mock services register");
    let _ = executor.spin_once(core::time::Duration::from_millis(0));

    fn publisher<'a>(executor: &'a Executor<'_>) -> &'a crate::mock::MockPublisher {
        executor
            .params
            .as_ref()
            .and_then(|p| p.event_publisher.as_ref())
            .expect("one /parameter_events publisher for the executor")
    }
    assert_eq!(publisher(&executor).topic_name(), "/parameter_events");
    let frames: alloc::vec::Vec<Decoded> = (0..publisher(&executor).publish_count())
        .filter_map(|i| publisher(&executor).retained(i))
        .map(|(bytes, len)| decode(&bytes[..len]))
        .collect();
    assert!(
        frames.iter().any(|(_, node, _)| node == "/alpha"),
        "alpha's start-up event: {frames:?}"
    );
    assert!(
        frames
            .iter()
            .any(|(_, node, s)| node == "/ns/beta" && s[0] == Some(("gain".into(), 2, 3))),
        "beta's start-up event names `gain`: {frames:?}"
    );
    assert!(
        frames.iter().all(|(_, node, _)| node != "/gamma"),
        "gamma opted out: {frames:?}"
    );

    publisher(&executor).clear_published();
    assert_eq!(
        executor.set_parameter_local_on(beta, "gain", ParameterValue::Integer(4)),
        Ok(())
    );
    let _ = executor.set_parameter_local_on(gamma, "quiet", ParameterValue::Integer(1));
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(
        publisher(&executor).publish_count(),
        1,
        "gamma's set is not published"
    );
    let (bytes, len) = publisher(&executor).last_published().unwrap();
    let (_, node, slots) = decode(&bytes[..len]);
    assert_eq!(node, "/ns/beta");
    assert_eq!(slots[1], Some(("gain".into(), 2, 4)), "a set is CHANGED");

    assert!(executor.undeclare_parameter_on(beta, "gain"));
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    let (bytes, len) = publisher(&executor).last_published().unwrap();
    let (_, _, slots) = decode(&bytes[..len]);
    assert_eq!(
        slots[2],
        Some(("gain".into(), 0, 0)),
        "an undeclare is DELETED"
    );
}

/// Issue 2023 — a node that holds NO parameter still announces itself: one
/// event with its name and three empty lists.
#[cfg(all(feature = "param-services", not(feature = "sim-time")))]
#[test]
fn a_node_holding_no_parameter_announces_itself() {
    let mut executor = executor_with_clock(MockSession::new());
    let _bare = executor.node_builder("bare").build().unwrap();
    executor
        .enable_parameter_events_on(NodeId::PRIMARY)
        .expect("one publisher");
    executor.flush_parameter_events();
    let p = executor
        .params
        .as_ref()
        .and_then(|p| p.event_publisher.as_ref())
        .unwrap();
    assert_eq!(p.publish_count(), 1);
    let (bytes, len) = p.last_published().unwrap();
    let (_, node, slots) = decode(&bytes[..len]);
    assert_eq!(node, "/bare");
    assert_eq!(slots, [None, None, None]);
}

// ===========================================================================
// phase-436 A2b / A2c -- a wait-paced cadence, and lateness the granularity
// can resolve.
// ===========================================================================

private_test_clock!(wait_paced_grid_clock);
private_test_clock!(wait_paced_granule_clock);
private_test_clock!(late_beyond_granule_clock);

/// A loop that passes whole milliseconds for a 1.5 ms cadence. Undeclared,
/// the 1 ms timeout would be the period. Declared wait-paced, the grid runs at
/// 1.5 ms: the timeout still bounds each wait, the wake at 1 ms is not a
/// release, and the next spin parks the remaining 0.5 ms onto the grid point.
#[test]
fn a_wait_paced_cadence_sets_the_grid_and_the_timeout_still_bounds_each_wait() {
    unsafe extern "C" fn park(_c: *mut core::ffi::c_void, us: u64) -> i8 {
        wait_paced_grid_clock::set_us(wait_paced_grid_clock::now_us() + us);
        1
    }
    wait_paced_grid_clock::claim_at_us(1_000_000);
    let mut ex = executor_with_clock_fn(MockSession::new(), wait_paced_grid_clock::now_us);
    ex.set_park_primitive(park, core::ptr::null_mut(), 1);
    ex.set_spin_nominal_wait_paced_us(1_500);
    let timeout = core::time::Duration::from_millis(1);
    for k in 1..=100u64 {
        ex.spin_once(timeout);
        assert_eq!(
            wait_paced_grid_clock::now_us(),
            1_000_000 + (k - 1) * 1_500 + 1_000,
            "spin {k}: the first wait is bounded by the 1 ms timeout"
        );
        ex.spin_once(timeout);
        assert_eq!(
            wait_paced_grid_clock::now_us(),
            1_000_000 + k * 1_500,
            "spin {k}: the second wait lands on the 1.5 ms grid point"
        );
    }
    assert_eq!(
        ex.spin_nominal_us, 1_500,
        "judged against the declared cadence"
    );
    assert_eq!(
        ex.release_jitter(),
        (0, 0, 100),
        "the early wakes are not releases; the 100 releases are on time"
    );
    assert_eq!(ex.skipped_releases(), 0);
}

/// The coupled setter reports 1 us because the caller's own sleep paces the
/// loop. A wait-paced declaration is worth one park granule, the same as an
/// undeclared loop, and declaring through the coupled setter afterwards takes
/// the self-paced reading back.
#[test]
fn a_wait_paced_cadence_reports_the_park_granularity() {
    unsafe extern "C" fn park(_c: *mut core::ffi::c_void, us: u64) -> i8 {
        wait_paced_granule_clock::set_us(wait_paced_granule_clock::now_us() + us);
        1
    }
    wait_paced_granule_clock::claim_at_us(0);
    let mut ex = executor_with_clock_fn(MockSession::new(), wait_paced_granule_clock::now_us);
    ex.set_park_primitive(park, core::ptr::null_mut(), 250);
    assert_eq!(ex.release_jitter_granularity_us(), 250);
    ex.set_spin_nominal_wait_paced_us(5_000);
    assert_eq!(
        ex.release_jitter_granularity_us(),
        250,
        "a wait-paced loop cannot be judged finer than the park"
    );
    ex.set_spin_nominal_us(5_000);
    assert_eq!(
        ex.release_jitter_granularity_us(),
        1,
        "the coupled setter still claims a self-paced loop"
    );
    ex.set_spin_nominal_wait_paced_us(5_000);
    ex.set_spin_nominal_wait_paced_us(0);
    assert_eq!(
        ex.spin_nominal_declared_us, 0,
        "zero withdraws the declaration"
    );
    assert_eq!(ex.release_jitter_granularity_us(), 250);
}

/// The A3 capture: a 10 ms grid on a 1 ms-tick park, with 300 us of work after
/// each release. Every park rounds up to the tick, so most releases land a
/// fraction of a tick late -- `late` counts them, as it always did, and none
/// is late beyond the 1 ms the measurement is worth. Then one real 2.5 ms
/// overrun: that one is beyond the granule.
#[test]
fn rounding_to_the_park_tick_is_late_but_not_beyond_the_granule() {
    unsafe extern "C" fn park(_c: *mut core::ffi::c_void, us: u64) -> i8 {
        late_beyond_granule_clock::set_us(late_beyond_granule_clock::now_us() + us);
        1
    }
    late_beyond_granule_clock::claim_at_us(0);
    let mut ex = executor_with_clock_fn(MockSession::new(), late_beyond_granule_clock::now_us);
    ex.set_park_primitive(park, core::ptr::null_mut(), 1_000);
    let period = core::time::Duration::from_millis(10);
    ex.spin_once(period);
    for _ in 0..50 {
        late_beyond_granule_clock::set_us(late_beyond_granule_clock::now_us() + 300);
        ex.spin_once(period);
    }
    let (max_us, late, total) = ex.release_jitter();
    assert_eq!(total, 51);
    // Rounding lands exactly on the point only when the remaining budget is
    // a whole number of ticks, so most wakes -- not all -- are late.
    assert!(
        late > total / 2,
        "tick rounding is late on most wakes: {late} of {total}"
    );
    assert!(
        max_us <= 1_000,
        "and never by more than a tick: {max_us} us"
    );
    assert_eq!(
        ex.late_wakes_beyond_granularity(),
        0,
        "nothing was late by more than the granularity reports"
    );

    // One genuine overrun: 2.5 ms of work past where the next release is due.
    late_beyond_granule_clock::set_us(late_beyond_granule_clock::now_us() + 12_500);
    ex.spin_once(period);
    assert_eq!(ex.late_wakes_beyond_granularity(), 1);

    ex.clear_release_jitter_stats();
    assert_eq!(
        ex.late_wakes_beyond_granularity(),
        0,
        "cleared with the rest"
    );
}

private_test_clock!(b3_route_clock);

/// phase10-B3 -- `route-latency-runtime`, the route rule, on the host
/// executor with real time.
///
/// The shape of the safety island's `mrm_handler`: a subscription takes the
/// path's trigger input and a 100 ms timer reads it and publishes the output,
/// so the route from the take to the output is the wait for the next tick plus
/// the tick's work. The path's budget is 149 ms; the output's callback-level
/// budget (`max-latency-runtime`) is 150 ms, and every dispatch here is far
/// inside it.
///
/// On time: a take right after a tick is answered by the next tick, ~100 ms
/// later, and nothing is recorded. Delayed: the executor does not spin for
/// 260 ms after a take (the tick is late), the tick then publishes in a short
/// dispatch, and the route rule records ONE verdict naming the output with
/// `measured >= 260` and `declared = 149`, while the callback-level measure
/// of the same dispatch stays far below its 150 ms. The verdict is recorded
/// only: the next on-time route is judged and clean, and nothing else changes.
#[test]
fn b3_a_late_tick_exceeds_the_route_while_every_callback_passes() {
    use super::monitor::{MonitorSpec, PubMonitorCell};
    use crate::executor::arena::EntryKind;
    use core::sync::atomic::{AtomicU32, Ordering};
    let _g = arming_lock();

    static CELL: PubMonitorCell = PubMonitorCell::new();
    static TABLE: [MonitorSpec; 1] = [MonitorSpec {
        topic: "/b3/state",
        fqn: "/b3/handler/state",
        min_rate_hz_milli: 0,
        max_latency_ms: 150,
        route_trigger: "/b3/availability",
        route_max_latency_ms: 149,
        route_deadline_ms: 0,
        reaction_id: 0,
        mechanism: 0,
        cell: &CELL,
    }];
    static TICKS: AtomicU32 = AtomicU32::new(0);
    static TAKES: AtomicU32 = AtomicU32::new(0);

    // A PRIVATE clock the test moves by hand (issue 1939): the on-time route has 49 ms of margin, and
    // a 10 ms real sleep preempted past it under a parallel gate would be a
    // spurious route verdict.
    b3_route_clock::claim_at_us(1_000_000);
    fn work_ms(ms: u64) {
        b3_route_clock::set_us(b3_route_clock::now_us() + ms * 1_000);
    }
    fn step(executor: &mut Executor, ms: u64) {
        work_ms(ms);
        let _ = executor.spin_once(core::time::Duration::from_millis(0));
    }
    let mut executor: Executor = executor_with_clock_fn(MockSession::new(), b3_route_clock::now_us);
    executor.set_monitor_table(&TABLE);
    let nid = executor.node_builder("handler").build().expect("node");
    let state = executor
        .node_mut(nid)
        .create_generic_publisher("/b3/state", "std_msgs::msg::dds_::Int32_", "")
        .expect("contracted publisher");
    executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/b3/availability", |_m: &TestMsg| {
            TAKES.fetch_add(1, Ordering::SeqCst);
        })
        .expect("trigger subscription");
    executor
        .register_timer(TimerDuration::from_millis(100), move || {
            let tick = TICKS.fetch_add(1, Ordering::SeqCst) + 1;
            let _ = state.publish_serialized(&[0, 1, 0, 0, tick as u8, 0, 0, 0]);
        })
        .expect("timer");

    let sub_slot = executor
        .entries
        .iter()
        .position(|e| {
            e.as_ref()
                .is_some_and(|m| m.kind == EntryKind::Subscription)
        })
        .expect("the subscription's slot");
    let off = executor.entries[sub_slot].as_ref().unwrap().offset;
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    let load = |v: i32| {
        let (d, n) = encode_test_msg(v);
        unsafe { &*(arena_ptr.add(off) as *const MockSubscriber) }.load(d, n);
    };
    assert_eq!(
        CELL.route.load(Ordering::Relaxed) & super::monitor::ROUTE_SLOT_MASK,
        sub_slot as u32 + 1,
        "the trigger subscription bound itself to the route at registration"
    );

    let mut route_verdicts = alloc::vec::Vec::new();
    let drain = |ex: &mut Executor, out: &mut alloc::vec::Vec<(u32, u32)>| {
        ex.drain_violations(|v| {
            if v.rule == "route-latency-runtime" {
                assert_eq!(v.fqn, "/b3/handler/state");
                out.push((v.measured, v.declared));
            }
        });
    };
    // Spin in 10 ms steps until the next tick has run.
    let next_tick = |ex: &mut Executor| {
        let t = TICKS.load(Ordering::SeqCst);
        let mut spins = 0;
        while TICKS.load(Ordering::SeqCst) == t && spins < 100 {
            step(ex, 10);
            spins += 1;
        }
        assert!(TICKS.load(Ordering::SeqCst) > t, "the timer ticked");
    };

    // On time, three times: a take right after a tick, answered by the next.
    next_tick(&mut executor);
    for k in 0..3 {
        load(k);
        let _ = executor.spin_once(core::time::Duration::from_millis(0));
        assert_eq!(TAKES.load(Ordering::SeqCst), k as u32 + 1, "taken");
        next_tick(&mut executor);
        drain(&mut executor, &mut route_verdicts);
    }
    assert!(
        route_verdicts.is_empty(),
        "an on-time tick records nothing: {route_verdicts:?}"
    );
    assert_eq!(
        CELL.route.load(Ordering::Relaxed) & super::monitor::ROUTE_PENDING,
        0,
        "every take was answered"
    );

    // Delayed: a take, then 260 ms without a spin, then the late tick.
    let _ = CELL.max_latency_us.swap(0, Ordering::Relaxed);
    load(10);
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    let ticks = TICKS.load(Ordering::SeqCst);
    step(&mut executor, 260);
    assert!(TICKS.load(Ordering::SeqCst) > ticks, "the late tick ran");
    drain(&mut executor, &mut route_verdicts);
    assert_eq!(
        route_verdicts.len(),
        1,
        "one route verdict: {route_verdicts:?}"
    );
    let (measured, declared) = route_verdicts[0];
    assert_eq!(declared, 149);
    assert!(
        measured >= 260,
        "take to output, the late tick inside: {measured}"
    );
    let callback_us = CELL.max_latency_us.load(Ordering::Relaxed);
    assert!(
        callback_us < 150_000,
        "the tick's own dispatch is inside its callback budget: {callback_us} us"
    );

    // Record only: the next on-time route is judged again and is clean.
    route_verdicts.clear();
    next_tick(&mut executor);
    load(11);
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    next_tick(&mut executor);
    drain(&mut executor, &mut route_verdicts);
    assert!(route_verdicts.is_empty(), "{route_verdicts:?}");
    assert_eq!(
        CELL.route.load(Ordering::Relaxed)
            & (super::monitor::ROUTE_PENDING | super::monitor::ROUTE_VIOLATED),
        0,
        "answered, and the report-once state cleared by a clean route"
    );
}

private_test_clock!(route_deadline_clock);

/// phase-503 -- `route-deadline-runtime` on the host executor, with a clock
/// the test sets: the trigger is taken, the output NEVER comes.
///
/// Before this phase the route stayed pending forever and nothing was
/// recorded, because `route-latency-runtime` judges only at the output. Now
/// the executor offers the take's deadline as a wake source
/// (`WakeSourceId::RouteDeadline`), judges nothing one microsecond before it,
/// and at the deadline plus one 1 ms tick records one verdict and calls the
/// reaction hook with the row's reaction id, index and mechanism. The
/// verdict is recorded once; a second take that misses requests the reaction
/// again.
#[test]
fn route_deadline_fires_when_the_output_never_comes() {
    use super::monitor::{MECHANISM_TIMER, MonitorSpec, PubMonitorCell, ReactionEvent};
    use core::sync::atomic::Ordering;
    use std::sync::Mutex;
    let _g = arming_lock();

    static CELL: PubMonitorCell = PubMonitorCell::new();
    static TABLE: [MonitorSpec; 1] = [MonitorSpec {
        topic: "/s3/state",
        fqn: "/s3/handler/state",
        min_rate_hz_milli: 0,
        max_latency_ms: 0,
        route_trigger: "/s3/availability",
        route_max_latency_ms: 149,
        route_deadline_ms: 149,
        reaction_id: 7,
        mechanism: MECHANISM_TIMER,
        cell: &CELL,
    }];
    static EVENTS: Mutex<alloc::vec::Vec<(ReactionEvent, usize)>> =
        Mutex::new(alloc::vec::Vec::new());
    unsafe extern "C" fn hook(ev: *const ReactionEvent, ctx: *mut core::ffi::c_void) {
        let ev = unsafe { *ev };
        EVENTS.lock().unwrap().push((ev, ctx as usize));
    }

    route_deadline_clock::claim_at_us(1_000_000);
    let mut executor = executor_with_clock_fn(MockSession::new(), route_deadline_clock::now_us);
    executor.set_monitor_table(&TABLE);
    executor.set_reaction_hook(Some((hook, 0x5a as *mut core::ffi::c_void)));
    let nid = executor.node_builder("handler").build().expect("node");
    let _state = executor
        .node_mut(nid)
        .create_generic_publisher("/s3/state", "std_msgs::msg::dds_::Int32_", "")
        .expect("contracted publisher");
    executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/s3/availability", |_m: &TestMsg| {})
        .expect("trigger subscription");
    let sub_slot = executor
        .entries
        .iter()
        .position(|e| {
            e.as_ref()
                .is_some_and(|m| m.kind == super::arena::EntryKind::Subscription)
        })
        .expect("the subscription's slot");
    let off = executor.entries[sub_slot].as_ref().unwrap().offset;
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    let load = |v: i32| {
        let (d, n) = encode_test_msg(v);
        unsafe { &*(arena_ptr.add(off) as *const MockSubscriber) }.load(d, n);
    };
    let deadline_verdicts = |ex: &mut Executor| {
        let mut out = alloc::vec::Vec::new();
        ex.drain_violations(|v| {
            if v.rule == "route-deadline-runtime" {
                assert_eq!(v.fqn, "/s3/handler/state");
                out.push((v.measured, v.declared));
            }
        });
        out
    };

    // Nothing pending: no wake source.
    assert_eq!(executor.route_next_due_us(), None);
    // The take, at 1 s.
    load(1);
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    assert_ne!(
        CELL.route.load(Ordering::Relaxed) & super::monitor::ROUTE_PENDING,
        0,
        "the take is waiting for the output"
    );
    // The park ends at the deadline, and says why.
    assert_eq!(executor.route_next_due_us(), Some(149_000));
    assert_eq!(
        executor.next_wake_bound_attributed_us(1_000_000, None),
        (149_000, super::spin::WakeSourceId::RouteDeadline)
    );
    // One microsecond before it: nothing.
    route_deadline_clock::set_us(1_148_999);
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    assert!(EVENTS.lock().unwrap().is_empty());
    assert!(deadline_verdicts(&mut executor).is_empty());
    // The deadline plus one 1 ms tick: one verdict, one reaction.
    route_deadline_clock::set_us(1_150_000);
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    {
        let ev = EVENTS.lock().unwrap();
        assert_eq!(ev.len(), 1, "{ev:?}");
        let (e, ctx) = ev[0];
        assert_eq!(e.reaction_id, 7);
        assert_eq!(e.row, 0);
        assert_eq!(
            e.rule_code,
            super::monitor::rule_code("route-deadline-runtime")
        );
        assert_eq!((e.measured_ms, e.declared_ms), (150, 149));
        assert_eq!(e.mechanism, MECHANISM_TIMER);
        assert_eq!(ctx, 0x5a, "the registered context");
    }
    assert_eq!(deadline_verdicts(&mut executor), [(150, 149)]);
    assert_eq!(
        executor.route_next_due_us(),
        None,
        "judged: no longer pending"
    );
    // A later spin judges the same take no more.
    route_deadline_clock::set_us(2_000_000);
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(EVENTS.lock().unwrap().len(), 1);
    // A second take that also misses: the reaction again, the verdict not.
    load(2);
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    route_deadline_clock::set_us(2_149_000);
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(EVENTS.lock().unwrap().len(), 2);
    assert!(deadline_verdicts(&mut executor).is_empty(), "reported once");
}

private_test_clock!(route_late_clock);

/// phase-503 -- the board finding: a supervised route under
/// `mechanism: executor` whose ONE long callback takes the trigger and
/// publishes the output after the deadline. The executor's overdue check
/// cannot run while that callback holds the spin thread, and once it returns
/// the route is answered, so before the fix only `route-latency-runtime` was
/// recorded and the hook never ran. The completion now reports the deadline
/// miss and calls the hook, once; the overdue check that follows does not.
#[test]
fn a_late_completion_of_a_supervised_route_calls_the_reaction_hook() {
    use super::monitor::{MECHANISM_EXECUTOR, MonitorSpec, PubMonitorCell, ReactionEvent};
    use std::sync::Mutex;
    let _g = arming_lock();

    static CELL: PubMonitorCell = PubMonitorCell::new();
    static TABLE: [MonitorSpec; 1] = [MonitorSpec {
        topic: "/late/state",
        fqn: "/late/handler/state",
        min_rate_hz_milli: 0,
        max_latency_ms: 0,
        route_trigger: "/late/availability",
        route_max_latency_ms: 149,
        route_deadline_ms: 149,
        reaction_id: 4,
        mechanism: MECHANISM_EXECUTOR,
        cell: &CELL,
    }];
    static EVENTS: Mutex<alloc::vec::Vec<ReactionEvent>> = Mutex::new(alloc::vec::Vec::new());
    unsafe extern "C" fn hook(ev: *const ReactionEvent, _ctx: *mut core::ffi::c_void) {
        EVENTS.lock().unwrap().push(unsafe { *ev });
    }

    route_late_clock::claim_at_us(1_000_000);
    let mut executor = executor_with_clock_fn(MockSession::new(), route_late_clock::now_us);
    executor.set_monitor_table(&TABLE);
    executor.set_reaction_hook(Some((hook, core::ptr::null_mut())));
    let nid = executor.node_builder("handler").build().expect("node");
    let _state = executor
        .node_mut(nid)
        .create_generic_publisher("/late/state", "std_msgs::msg::dds_::Int32_", "")
        .expect("contracted publisher");
    // The callback: runs `v` ms, then "publishes" the output (the counter the
    // publish path bumps).
    executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/late/availability", |m: &TestMsg| {
            route_late_clock::set_us(route_late_clock::now_us() + m.data as u64 * 1_000);
            CELL.count
                .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        })
        .expect("trigger subscription");
    let sub_slot = executor
        .entries
        .iter()
        .position(|e| {
            e.as_ref()
                .is_some_and(|m| m.kind == super::arena::EntryKind::Subscription)
        })
        .expect("the subscription's slot");
    let off = executor.entries[sub_slot].as_ref().unwrap().offset;
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    let load = |v: i32| {
        let (d, n) = encode_test_msg(v);
        unsafe { &*(arena_ptr.add(off) as *const MockSubscriber) }.load(d, n);
    };
    let verdicts = |ex: &mut Executor| {
        let mut out = alloc::vec::Vec::new();
        ex.drain_violations(|v| {
            if v.rule.starts_with("route-") {
                out.push((v.rule, v.measured, v.declared));
            }
        });
        out
    };

    // A callback of 327 ms against a 149 ms deadline.
    load(327);
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    {
        let ev = EVENTS.lock().unwrap();
        assert_eq!(ev.len(), 1, "{ev:?}");
        let e = ev[0];
        assert_eq!((e.reaction_id, e.row), (4, 0));
        assert_eq!(
            e.rule_code,
            super::monitor::rule_code("route-deadline-runtime")
        );
        assert_eq!((e.measured_ms, e.declared_ms), (327, 149));
        assert_eq!(e.mechanism, MECHANISM_EXECUTOR);
    }
    assert_eq!(
        verdicts(&mut executor),
        [
            ("route-latency-runtime", 327, 149),
            ("route-deadline-runtime", 327, 149),
        ]
    );
    // The overdue check after it has nothing left: no second call.
    route_late_clock::set_us(2_000_000);
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(EVENTS.lock().unwrap().len(), 1, "exactly once per take");
    // On time: no call, no verdict.
    load(10);
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(EVENTS.lock().unwrap().len(), 1);
    assert!(verdicts(&mut executor).is_empty());
}

private_test_clock!(route_watch_clock);

/// phase-503 W7 -- the board finding behind `RouteWatch`: a port supervises a
/// route with its own timer (`mechanism: timer`), armed from
/// `route_next_due_us` after each spin. The take is in one spin; a later spin
/// ANSWERS the route 4 ms before its due and then runs on, in another
/// callback, past the due. The timer expires there, mid-spin. Armed from the
/// last spin's due, the port saw a miss (a false safe state) although the
/// executor judged the route on time. The watch, read at that instant from
/// inside the running spin, says answered; its real-miss twin (the same spin,
/// no answer) says due now, at the same instant.
#[test]
fn a_route_answered_before_its_due_reads_answered_while_the_spin_runs_on() {
    use super::monitor::{MECHANISM_TIMER, MonitorSpec, PubMonitorCell, RouteWatch};
    use core::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Mutex;
    let _g = arming_lock();

    static CELL: PubMonitorCell = PubMonitorCell::new();
    static TABLE: [MonitorSpec; 1] = [MonitorSpec {
        topic: "/rw/state",
        fqn: "/rw/handler/state",
        min_rate_hz_milli: 0,
        max_latency_ms: 0,
        route_trigger: "/rw/availability",
        route_max_latency_ms: 149,
        route_deadline_ms: 149,
        reaction_id: 1,
        mechanism: MECHANISM_TIMER,
        cell: &CELL,
    }];
    // The watch the "port" holds, and what it read at the timer's expiry.
    static WATCH: Mutex<Option<RouteWatch>> = Mutex::new(None);
    static AT_EXPIRY: Mutex<Option<Option<u64>>> = Mutex::new(None);
    static HOOK_CALLS: AtomicU32 = AtomicU32::new(0);
    unsafe extern "C" fn hook(
        _ev: *const super::monitor::ReactionEvent,
        _ctx: *mut core::ffi::c_void,
    ) {
        HOOK_CALLS.fetch_add(1, Ordering::SeqCst);
    }

    route_watch_clock::claim_at_us(1_000_000);
    let mut executor = executor_with_clock_fn(MockSession::new(), route_watch_clock::now_us);
    executor.set_monitor_table(&TABLE);
    executor.set_reaction_hook(Some((hook, core::ptr::null_mut())));
    let nid = executor.node_builder("handler").build().expect("node");
    let _state = executor
        .node_mut(nid)
        .create_generic_publisher("/rw/state", "std_msgs::msg::dds_::Int32_", "")
        .expect("contracted publisher");
    // The trigger: 0 = a take only; 1 = (pending already, so no new take)
    // publish the output, which answers the route at this dispatch's end.
    executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/rw/availability", |m: &TestMsg| {
            if m.data == 1 {
                CELL.count.fetch_add(1, Ordering::Relaxed);
            }
        })
        .expect("trigger subscription");
    // The rest of the spin: runs `v` ms, and the port's timer expires at its
    // end, on another context: read the watch there.
    executor
        .node_mut(nid)
        .create_subscription::<TestMsg, _>("/rw/other", |m: &TestMsg| {
            route_watch_clock::set_us(route_watch_clock::now_us() + m.data as u64 * 1_000);
            let w = WATCH.lock().unwrap().expect("the port took its watch");
            *AT_EXPIRY.lock().unwrap() = Some(w.next_due_us());
        })
        .expect("second subscription");
    let subs: alloc::vec::Vec<usize> = executor
        .entries
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            e.as_ref()
                .is_some_and(|m| m.kind == super::arena::EntryKind::Subscription)
        })
        .map(|(i, _)| i)
        .collect();
    assert_eq!(subs.len(), 2);
    let arena_ptr = executor.arena.as_ptr() as *const u8;
    let offs: alloc::vec::Vec<usize> = subs
        .iter()
        .map(|&i| executor.entries[i].as_ref().unwrap().offset)
        .collect();
    let load = |sub: usize, v: i32| {
        let (d, n) = encode_test_msg(v);
        unsafe { &*(arena_ptr.add(offs[sub]) as *const MockSubscriber) }.load(d, n);
    };
    *WATCH.lock().unwrap() = Some(executor.route_watch().expect("table and clock"));

    for answer in [true, false] {
        *AT_EXPIRY.lock().unwrap() = None;
        let base = route_watch_clock::now_us();
        // Spin A: the take. The port re-arms its timer after it: due =
        // now + 149 ms (the old port's only source).
        load(0, 0);
        let _ = executor.spin_once(core::time::Duration::from_millis(0));
        let armed_due_us = route_watch_clock::now_us()
            + executor.route_next_due_us().expect("the take is waiting");
        assert_eq!(armed_due_us, base + 149_000);
        // Spin B starts 145 ms in: the trigger's callback answers there (or
        // does not), then the other callback runs 10 ms, to 155 ms. The
        // timer expires at 149 ms, inside it.
        route_watch_clock::set_us(base + 145_000);
        if answer {
            load(0, 1);
        }
        load(1, 10);
        let _ = executor.spin_once(core::time::Duration::from_millis(0));
        let expiry_read = AT_EXPIRY.lock().unwrap().expect("the other callback ran");
        let read_at = route_watch_clock::now_us();
        assert!(
            read_at >= armed_due_us,
            "the expiry is inside the spin, past the due armed after spin A"
        );
        // Before W7 the port knew nothing newer than spin A's due: armed and
        // past at the expiry, so it latched either way.
        if answer {
            assert_eq!(
                expiry_read, None,
                "answered at 145 ms of 149: the watch reads answered mid-spin"
            );
            assert_eq!(
                HOOK_CALLS.load(Ordering::SeqCst),
                0,
                "on time for the executor too"
            );
        } else {
            assert_eq!(
                expiry_read,
                Some(0),
                "unanswered at the due: the watch reads the miss at the same instant"
            );
        }
        // Leave nothing pending for the next round.
        let _ = executor.spin_once(core::time::Duration::from_millis(0));
        executor.drain_violations(|_| {});
        assert_eq!(executor.route_watch().unwrap().next_due_us(), None);
        route_watch_clock::set_us(route_watch_clock::now_us() + 1_000_000);
    }
    assert_eq!(HOOK_CALLS.load(Ordering::SeqCst), 1, "the miss, once");
}

/// Issue 1913 — a raw action server and client key each endpoint on its OWN
/// hash. An Iron+ `rmw_zenoh_cpp` peer keys all five on different types, and
/// every raw registration used to pass the one `type_hash` to all of them, so
/// a C, C++ or `nros::main!` action matched no Jazzy peer.
#[test]
fn raw_action_endpoints_are_keyed_on_their_own_hashes() {
    use super::action::{RawActionClientSpec, RawActionServerSpec};

    let hashes = nros_rmw::ActionEndpointHashes {
        send_goal: "SEND_GOAL",
        cancel_goal: "CANCEL_GOAL",
        get_result: "GET_RESULT",
        feedback: "FEEDBACK",
        status: "STATUS",
    };
    let mut executor = executor_with_clock(MockSession::new());
    executor
        .register_action_server_raw(RawActionServerSpec {
            node_id: None,
            action_name: "/srv_act",
            type_name: "test_msgs/action/Fibonacci",
            type_hash: "ACTION",
            hashes,
            qos: nros_rmw::QoSProfile::services_default(),
            goal_callback: accept_and_record_context,
            cancel_callback: refuse_every_cancel,
            accepted_callback: None,
            context: core::ptr::null_mut(),
        })
        .expect("the mock session registers a raw action server");
    executor
        .register_action_client_raw(RawActionClientSpec {
            node_id: None,
            action_name: "/cli_act",
            type_name: "test_msgs/action/Fibonacci",
            type_hash: "ACTION",
            hashes,
            goal_response_callback: None,
            feedback_callback: None,
            result_callback: None,
            cancel_response_callback: None,
            context: core::ptr::null_mut(),
        })
        .expect("the mock session registers a raw action client");

    let session = executor.session();
    let expect = [
        ("/srv_act/_action/send_goal", "SEND_GOAL"),
        ("/srv_act/_action/cancel_goal", "CANCEL_GOAL"),
        ("/srv_act/_action/get_result", "GET_RESULT"),
        ("/srv_act/_action/feedback", "FEEDBACK"),
        ("/srv_act/_action/status", "STATUS"),
        ("/cli_act/_action/send_goal", "SEND_GOAL"),
        ("/cli_act/_action/cancel_goal", "CANCEL_GOAL"),
        ("/cli_act/_action/get_result", "GET_RESULT"),
        ("/cli_act/_action/feedback", "FEEDBACK"),
    ];
    for (name, want) in expect {
        let got = session.created_hash(name);
        assert_eq!(
            got.as_ref().map(|h| h.as_str()),
            Some(want),
            "{name}: created {:?}",
            session.created.borrow()
        );
    }
}

/// A default sink (the Zephyr tier runner's) installs only into an empty
/// slot: a sink the application installed first is kept.
#[cfg(feature = "trace-callbacks")]
#[test]
fn a_default_trace_sink_keeps_the_applications_sink() {
    use super::callback_trace::{set_trace_sink, set_trace_sink_if_unset};
    unsafe extern "C" fn app_sink(_: u32, _: u32) {}
    unsafe extern "C" fn default_sink(_: u32, _: u32) {}
    let _g = trace_sink_lock();

    set_trace_sink(None);
    assert!(
        set_trace_sink_if_unset(default_sink),
        "an empty slot takes the default"
    );
    set_trace_sink(None);

    set_trace_sink(Some(app_sink));
    assert!(
        !set_trace_sink_if_unset(default_sink),
        "the application's sink is kept"
    );
    set_trace_sink(None);
}

private_test_clock!(hazard_reaction_clock);

/// phase-462 W3c -- a hazard whose reacting path states a `safe_state` asks
/// the reaction hook to start it when the hazard is DETECTED, once per
/// detection, with the hazard's reaction id and its index in the hazard table.
/// The verdicts land on the drain beside it: the guard's `silence-runtime`
/// and the observer's `hazard-detected`.
#[test]
fn a_detected_hazard_starts_its_safe_state_reaction() {
    use super::monitor::{
        AgeMonitorSpec, GuardRef, GuardSpec, HazardCell, HazardSpec, NO_SINK, ReactionEvent,
        SubMonitorCell, rule_code,
    };
    use std::sync::Mutex;
    let _g = arming_lock();

    static SUB: SubMonitorCell = SubMonitorCell::new();
    static AGES: [AgeMonitorSpec; 1] = [AgeMonitorSpec {
        topic: "/h3/scan",
        fqn: "/h3/handler/scan",
        max_age_ms: 100,
        deadline_ms: 0,
        cell: &SUB,
    }];
    static CELL: HazardCell = HazardCell::new();
    static GUARDS: [GuardSpec; 1] = [GuardSpec {
        all_of: false,
        members: &[GuardRef::Age(0)],
    }];
    static HAZARDS: [HazardSpec; 1] = [HazardSpec {
        key: "d.launch.xml/scan_lost",
        ftti_ms: 500,
        settle_ms: 0,
        guards: &GUARDS,
        sink: NO_SINK,
        reaction_id: 3,
        rung_row: NO_SINK,
        rung_route_ms: 0,
        cell: &CELL,
    }];
    static EVENTS: Mutex<alloc::vec::Vec<ReactionEvent>> = Mutex::new(alloc::vec::Vec::new());
    unsafe extern "C" fn hook(ev: *const ReactionEvent, _ctx: *mut core::ffi::c_void) {
        EVENTS.lock().unwrap().push(unsafe { *ev });
    }

    hazard_reaction_clock::claim_at_us(1_000_000);
    let mut executor = executor_with_clock_fn(MockSession::new(), hazard_reaction_clock::now_us);
    executor.set_age_table(&AGES);
    executor.set_hazard_table(&HAZARDS);
    executor.set_reaction_hook(Some((hook, core::ptr::null_mut())));

    // Opens the guard's lease; nothing is silent yet.
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    assert!(EVENTS.lock().unwrap().is_empty());

    // 150 ms with nothing taken: the guard is silent, the hazard is detected
    // and its reaction requested.
    hazard_reaction_clock::set_us(1_150_000);
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    let events = EVENTS.lock().unwrap().clone();
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0].reaction_id, 3);
    assert_eq!(events[0].row, 0);
    assert_eq!(events[0].rule_code, rule_code("hazard-detected"));
    assert_eq!((events[0].measured_ms, events[0].declared_ms), (150, 500));
    // play_launch phase 96 W4.2 -- the event carries the fire: the detection
    // tick, where the hazard's rung routes start.
    assert_eq!(events[0].fired_at_us, 1_150_000);

    // Still silent: detected once, so no second request.
    hazard_reaction_clock::set_us(1_300_000);
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(EVENTS.lock().unwrap().len(), 1);

    let mut rules = alloc::vec::Vec::new();
    executor.drain_violations(|v| rules.push((v.rule, v.fqn)));
    assert!(
        rules.contains(&("silence-runtime", "/h3/handler/scan")),
        "{rules:?}"
    );
    assert!(
        rules.contains(&("hazard-detected", "d.launch.xml/scan_lost")),
        "{rules:?}"
    );
}

private_test_clock!(mode_switch_clock);

/// phase-462 W4 -- a contract mode is a variant of the base monitor table,
/// sharing its cells, and the executor judges by the requested mode's
/// numbers from its next spin.
///
/// One 100 ms timer publishes on a contracted topic and takes 20 ms. The base
/// budget is 5 ms, mode 1's is 50 ms. In the base the overrun is judged;
/// after `request_mode(1)` the same 20 ms is inside the budget and nothing
/// more is stored; a request for a mode the executor has no table for keeps
/// it in mode 1.
#[test]
fn a_requested_mode_judges_by_its_variant_table() {
    use super::monitor::{ModeTables, MonitorSpec, PubMonitorCell, request_mode};
    let _g = arming_lock();

    static CELL: PubMonitorCell = PubMonitorCell::new();
    const fn spec(max_latency_ms: u32) -> MonitorSpec {
        MonitorSpec {
            topic: "/mode/status",
            fqn: "/mode/handler/status",
            min_rate_hz_milli: 0,
            max_latency_ms,
            route_trigger: "",
            route_max_latency_ms: 0,
            route_deadline_ms: 0,
            reaction_id: 0,
            mechanism: 0,
            cell: &CELL,
        }
    }
    static BASE: [MonitorSpec; 1] = [spec(5)];
    static RELAXED: [MonitorSpec; 1] = [spec(50)];
    static MODES: [ModeTables; 2] = [
        ModeTables {
            monitors: &BASE,
            ages: &[],
            deadlines: &[],
            switch_budget_ms: 0,
            reaction: "",
        },
        ModeTables {
            monitors: &RELAXED,
            ages: &[],
            deadlines: &[],
            switch_budget_ms: 0,
            reaction: "",
        },
    ];
    static SHORT: [ModeTables; 1] = [ModeTables {
        monitors: &[],
        ages: &[],
        deadlines: &[],
        switch_budget_ms: 0,
        reaction: "",
    }];

    mode_switch_clock::claim_at_us(1_000_000);
    fn step(executor: &mut Executor, ms: u64) {
        mode_switch_clock::set_us(mode_switch_clock::now_us() + ms * 1_000);
        let _ = executor.spin_once(core::time::Duration::from_millis(0));
    }
    request_mode(0);
    let mut executor: Executor =
        executor_with_clock_fn(MockSession::new(), mode_switch_clock::now_us);
    executor.set_monitor_table(&BASE);
    executor.set_report_violations(false);
    assert!(
        executor.set_mode_tables(&SHORT).is_err(),
        "a variant of another length is refused"
    );
    executor
        .set_mode_tables(&MODES)
        .expect("same-shape variants");
    let nid = executor.node_builder("handler").build().expect("node");
    let status = executor
        .node_mut(nid)
        .create_generic_publisher("/mode/status", "std_msgs::msg::dds_::Int32_", "")
        .expect("contracted publisher");
    executor
        .register_timer(TimerDuration::from_millis(100), move || {
            let _ = status.publish_serialized(&[0, 1, 0, 0, 1, 0, 0, 0]);
            mode_switch_clock::set_us(mode_switch_clock::now_us() + 20_000);
        })
        .expect("timer");

    for _ in 0..3 {
        step(&mut executor, 80);
    }
    let latency = |ex: &mut Executor| {
        let mut n = 0;
        ex.drain_violations(|v| {
            assert_eq!(
                v.rule, "max-latency-runtime",
                "only the latency rule speaks: {v:?}"
            );
            n += 1;
        });
        n
    };
    assert!(
        latency(&mut executor) >= 1,
        "the base judges 20 ms against 5 ms"
    );
    assert_eq!(executor.current_mode(), 0);

    request_mode(1);
    step(&mut executor, 80);
    assert_eq!(executor.current_mode(), 1, "entered at the next spin");
    assert_eq!(executor.monitor_table()[0].max_latency_ms, 50);
    let _ = latency(&mut executor);
    for _ in 0..4 {
        step(&mut executor, 80);
    }
    assert_eq!(latency(&mut executor), 0, "20 ms is inside mode 1's 50 ms");

    request_mode(7);
    step(&mut executor, 80);
    assert_eq!(executor.current_mode(), 1, "no table for mode 7: stays");
    request_mode(0);
}

private_test_clock!(mode_alias_clock);

/// A mode whose tables ARE the base's (the C/C++ entry bakes a mode with the
/// base's numbers as an alias of the base slice) is still a mode: it is
/// entered at the next spin and judges by the base numbers, and a mode with
/// numbers of its own beside it still applies them.
#[test]
fn a_mode_aliasing_the_base_is_entered_and_a_differing_one_still_applies() {
    use super::monitor::{ModeTables, MonitorSpec, PubMonitorCell, request_mode};
    let _g = arming_lock();

    static CELL: PubMonitorCell = PubMonitorCell::new();
    const fn spec(max_latency_ms: u32) -> MonitorSpec {
        MonitorSpec {
            topic: "/alias/status",
            fqn: "/alias/handler/status",
            min_rate_hz_milli: 0,
            max_latency_ms,
            route_trigger: "",
            route_max_latency_ms: 0,
            route_deadline_ms: 0,
            reaction_id: 0,
            mechanism: 0,
            cell: &CELL,
        }
    }
    static BASE: [MonitorSpec; 1] = [spec(5)];
    static RELAXED: [MonitorSpec; 1] = [spec(50)];
    static MODES: [ModeTables; 3] = [
        ModeTables {
            monitors: &BASE,
            ages: &[],
            deadlines: &[],
            switch_budget_ms: 0,
            reaction: "",
        },
        ModeTables {
            monitors: &BASE,
            ages: &[],
            deadlines: &[],
            switch_budget_ms: 1_000,
            reaction: "island.tor",
        },
        ModeTables {
            monitors: &RELAXED,
            ages: &[],
            deadlines: &[],
            switch_budget_ms: 0,
            reaction: "",
        },
    ];

    mode_alias_clock::claim_at_us(1_000_000);
    fn step(executor: &mut Executor, ms: u64) {
        mode_alias_clock::set_us(mode_alias_clock::now_us() + ms * 1_000);
        let _ = executor.spin_once(core::time::Duration::from_millis(0));
    }
    request_mode(0);
    let mut executor: Executor =
        executor_with_clock_fn(MockSession::new(), mode_alias_clock::now_us);
    executor.set_monitor_table(&BASE);
    executor.set_report_violations(false);
    executor
        .set_mode_tables(&MODES)
        .expect("an aliased mode has the base's shape");
    let nid = executor.node_builder("handler").build().expect("node");
    let status = executor
        .node_mut(nid)
        .create_generic_publisher("/alias/status", "std_msgs::msg::dds_::Int32_", "")
        .expect("contracted publisher");
    executor
        .register_timer(TimerDuration::from_millis(100), move || {
            let _ = status.publish_serialized(&[0, 1, 0, 0, 1, 0, 0, 0]);
            mode_alias_clock::set_us(mode_alias_clock::now_us() + 20_000);
        })
        .expect("timer");
    let latency = |ex: &mut Executor| {
        let mut n = 0;
        ex.drain_violations(|v| {
            if v.rule == "max-latency-runtime" {
                n += 1;
            }
        });
        n
    };

    request_mode(1);
    step(&mut executor, 80);
    assert_eq!(executor.current_mode(), 1, "the aliased mode is entered");
    assert!(
        core::ptr::eq(executor.monitor_table(), &BASE[..]),
        "and judges by the base table itself"
    );
    let _ = latency(&mut executor);
    for _ in 0..4 {
        step(&mut executor, 80);
    }
    assert!(
        latency(&mut executor) >= 1,
        "the base's 5 ms still judges 20 ms in the aliased mode"
    );

    request_mode(2);
    step(&mut executor, 80);
    assert_eq!(executor.current_mode(), 2);
    assert_eq!(executor.monitor_table()[0].max_latency_ms, 50);
    let _ = latency(&mut executor);
    for _ in 0..4 {
        step(&mut executor, 80);
    }
    assert_eq!(latency(&mut executor), 0, "20 ms is inside mode 2's 50 ms");
    request_mode(0);
}

private_test_clock!(unserved_deadline_clock);

/// phase-462 W5b -- a contract `deadline` the backend is never asked to judge
/// (this registration path hands the backend the caller's profile, which
/// carries none) is armed on its age row at create and judged by the silence
/// rule over the deadline's window. A subscription whose profile DOES carry the
/// deadline, on a backend that honours it, is left to the backend; a released
/// subscription stops being evidence for its row.
#[test]
fn a_contract_deadline_the_backend_does_not_judge_is_judged_by_silence() {
    use super::monitor::{AgeMonitorSpec, SubMonitorCell};
    use core::sync::atomic::Ordering;
    let _g = arming_lock();

    static UNJUDGED: SubMonitorCell = SubMonitorCell::new();
    static JUDGED: SubMonitorCell = SubMonitorCell::new();
    static AGES: [AgeMonitorSpec; 2] = [
        AgeMonitorSpec {
            topic: "/w5b/unjudged",
            fqn: "/w5b/node/unjudged",
            max_age_ms: 0,
            deadline_ms: 50,
            cell: &UNJUDGED,
        },
        AgeMonitorSpec {
            topic: "/w5b/judged",
            fqn: "/w5b/node/judged",
            max_age_ms: 0,
            deadline_ms: 50,
            cell: &JUDGED,
        },
    ];
    fn register(
        executor: &mut Executor<'_>,
        topic: &str,
        qos: QoSProfile,
    ) -> Result<HandleId, NodeError> {
        executor.add_arena_subscription_c_callback::<{ crate::config::DEFAULT_RX_BUF_SIZE }>(
            None,
            topic,
            "test/msg/TestMsg",
            "test_hash",
            qos,
            release_probe_sub,
            core::ptr::null_mut(),
            None,
            96,
        )
    }

    unserved_deadline_clock::claim_at_us(1_000_000);
    let mut executor = executor_with_clock_fn(MockSession::new(), unserved_deadline_clock::now_us);
    executor.set_age_table(&AGES);
    let unjudged = register(&mut executor, "/w5b/unjudged", QoSProfile::default()).unwrap();
    let judged = QoSProfile {
        deadline_ms: 50,
        ..QoSProfile::default()
    };
    register(&mut executor, "/w5b/judged", judged).unwrap();
    assert_ne!(UNJUDGED.deadline_slot.load(Ordering::Relaxed), 0, "armed");
    assert_eq!(
        JUDGED.deadline_slot.load(Ordering::Relaxed),
        0,
        "the backend judges a deadline it honours and was given"
    );

    // Opens the window; then 60 ms with nothing ready is past the deadline.
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    unserved_deadline_clock::set_us(1_060_000);
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    let mut got = alloc::vec::Vec::new();
    executor.drain_violations(|v| got.push((v.rule, v.fqn, v.declared)));
    assert_eq!(
        got,
        [("silence-runtime", "/w5b/node/unjudged", 50)],
        "only the armed row judges, over the deadline's window"
    );

    assert!(unsafe { executor.release_subscription(unjudged) });
    assert_eq!(
        UNJUDGED.deadline_slot.load(Ordering::Relaxed),
        0,
        "disarmed"
    );
}

/// Issue 1203 — `use_sim_time` declared through `Executor::parameter`'s
/// builder attaches the `/clock` source, like the two `declare_parameter`
/// paths.
///
/// The builder declares through its own borrow of the store, so the executor
/// never saw it. Once phase-430 W2 seeded a `use_sim_time = false` placeholder
/// on every node, the symptom changed: measured on the pre-fix code the
/// builder's declaration was REFUSED (`Err(TypeMismatch)`) and the store kept
/// the seeded `false`, so the app's `default(true)` was lost entirely.
#[cfg(all(feature = "sim-time", feature = "param-store"))]
#[test]
fn use_sim_time_through_the_builder_attaches_the_clock_source() {
    let _sim_time = SimTimeGuard::acquire();
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);
    let _ = executor.create_node("builder_node").expect("create node");
    // Any earlier declaration builds the store, and with it the seed.
    assert!(executor.declare_parameter("other", nros_params::ParameterValue::Integer(1)));
    let primary = super::node_record::NodeId::PRIMARY.into();
    assert_eq!(
        executor
            .params()
            .and_then(|p| p.get_bool(primary, "use_sim_time")),
        Some(false),
        "precondition: the store seeds the placeholder"
    );

    let declared = executor
        .parameter::<bool>("use_sim_time")
        .expect("the store exists")
        .default(true)
        .mandatory();
    assert!(
        declared.is_ok(),
        "the app's declaration must replace the placeholder, not be refused by it: {:?}",
        declared.err()
    );
    assert_eq!(
        executor
            .params()
            .and_then(|p| p.get_bool(primary, "use_sim_time")),
        Some(true),
        "the builder's default is what the store holds"
    );

    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    assert!(
        executor.ros_time_source_installed(),
        "use_sim_time = true through the builder must attach /clock on the next spin"
    );
    assert!(
        crate::time_source::is_active(),
        "attached and not armed drops every sample"
    );
}

/// Issue 1203 — a builder for `use_sim_time` that is dropped without declaring
/// leaves the node with its placeholder, not with no `use_sim_time` at all.
#[cfg(all(feature = "sim-time", feature = "param-store"))]
#[test]
fn an_undeclared_use_sim_time_builder_puts_the_placeholder_back() {
    let _sim_time = SimTimeGuard::acquire();
    let session = MockSession::new();
    let mut executor: Executor = executor_with_clock(session);
    let _ = executor.create_node("builder_node").expect("create node");
    assert!(executor.declare_parameter("other", nros_params::ParameterValue::Integer(1)));
    let primary = super::node_record::NodeId::PRIMARY.into();

    let _unused_builder = executor
        .parameter::<bool>("use_sim_time")
        .expect("the store exists");
    let _ = executor.spin_once(core::time::Duration::from_millis(0));
    assert_eq!(
        executor
            .params()
            .and_then(|p| p.get_bool(primary, "use_sim_time")),
        Some(false),
        "the placeholder stepped aside for a builder that declared nothing"
    );
    assert!(
        !executor.ros_time_source_installed(),
        "the placeholder is nobody's statement and must attach nothing"
    );
}

private_test_clock!(mode_deadline_clock);

/// phase-462 W4c -- entering a mode moves the SchedContext deadlines its table
/// lists, and entering the base restores them; a switch whose request could
/// have waited longer than the mode's budget is reported as
/// `mode-switch-runtime`, measured as the gap since the previous check.
#[test]
fn a_mode_switch_moves_the_tier_deadline_and_judges_its_latency() {
    use super::{
        monitor::{ModeDeadline, ModeTables, request_mode},
        sched_context::{OptUs, SchedContext, SchedContextId},
    };
    let _g = arming_lock();

    static BASE_D: [ModeDeadline; 1] = [ModeDeadline {
        sc: 1,
        deadline_us: 5_000,
    }];
    static TIGHT_D: [ModeDeadline; 1] = [ModeDeadline {
        sc: 1,
        deadline_us: 2_000,
    }];
    static MODES: [ModeTables; 2] = [
        ModeTables {
            monitors: &[],
            ages: &[],
            deadlines: &BASE_D,
            switch_budget_ms: 0,
            reaction: "",
        },
        ModeTables {
            monitors: &[],
            ages: &[],
            deadlines: &TIGHT_D,
            switch_budget_ms: 20,
            reaction: "//enter_tight",
        },
    ];
    static MISSING: [ModeTables; 1] = [ModeTables {
        monitors: &[],
        ages: &[],
        deadlines: &[ModeDeadline {
            sc: 9,
            deadline_us: 1,
        }],
        switch_budget_ms: 0,
        reaction: "",
    }];

    request_mode(0);
    mode_deadline_clock::claim_at_us(1_000_000);
    let mut executor = executor_with_clock_fn(MockSession::new(), mode_deadline_clock::now_us);
    let sc = executor
        .create_sched_context(SchedContext {
            deadline_us: OptUs::from_us(5_000),
            ..SchedContext::new_fifo()
        })
        .expect("sc");
    assert_eq!(sc, SchedContextId(1));
    assert!(
        executor.set_mode_tables(&MISSING).is_err(),
        "a row naming a context this executor lacks is refused"
    );
    executor.set_mode_tables(&MODES).expect("tables");
    let deadline = |e: &Executor<'_>| e.sched_context(sc).unwrap().deadline_us.raw();
    let spin = |e: &mut Executor<'_>, at_us: u64| {
        mode_deadline_clock::set_us(at_us);
        let _ = e.spin_once(core::time::Duration::from_millis(0));
    };

    spin(&mut executor, 1_000_000);
    request_mode(1);
    spin(&mut executor, 1_010_000);
    assert_eq!(deadline(&executor), 2_000, "the mode's deadline");
    request_mode(0);
    spin(&mut executor, 1_020_000);
    assert_eq!(deadline(&executor), 5_000, "the base restored");

    let mut got = alloc::vec::Vec::new();
    executor.drain_violations(|v| got.push(v.rule));
    assert!(
        !got.contains(&"mode-switch-runtime"),
        "10 ms within 20: {got:?}"
    );

    // The negative control: a request seen 50 ms after the previous check.
    request_mode(1);
    spin(&mut executor, 1_070_000);
    let mut got = alloc::vec::Vec::new();
    executor.drain_violations(|v| got.push((v.rule, v.fqn, v.measured, v.declared)));
    assert!(
        got.contains(&("mode-switch-runtime", "//enter_tight", 50, 20)),
        "{got:?}"
    );
    request_mode(0);
}

/// Issue 1971 (RFC-0107 step 1) -- an executor with no runtime remap table
/// (bare metal's default) refuses `declare_remap` BY NAME, and still resolves
/// the flash launch table.
#[test]
fn declare_remap_without_a_runtime_table_is_refused_by_name() {
    static LAUNCH: [super::LaunchRemap; 1] = [super::LaunchRemap::new(
        c"talker",
        c"/",
        c"chatter",
        c"/from_flash",
    )];
    let sizing = super::storage::ExecutorSizing {
        remaps: 0,
        ..super::storage::ExecutorSizing::DEFAULT
    };
    let backing: &'static mut [core::mem::MaybeUninit<u64>] =
        alloc::boxed::Box::leak(alloc::boxed::Box::new_uninit_slice(sizing.u64_len()));
    // SAFETY: `backing` is exactly `sizing.u64_len()` words, `'static`, and is
    // handed to this executor alone.
    let mut executor: Executor =
        unsafe { Executor::from_session_in(MockSession::new(), backing, sizing) };
    assert_eq!(
        executor.declare_remap("talker", "/", "chatter", "/x"),
        Err(super::RemapRefused::NoRuntimeTable)
    );
    executor.set_launch_remaps(&LAUNCH);
    assert_eq!(
        executor
            .resolve_entity_name_for("talker", "/", "chatter")
            .unwrap()
            .as_str(),
        "/from_flash"
    );
}

/// Issue 1971 -- a full runtime table is `TableFull`, not a dropped rule.
#[test]
fn declare_remap_into_a_full_runtime_table_is_table_full() {
    let mut executor: Executor = Executor::from_session(MockSession::new());
    for i in 0..super::spin::MAX_REMAPS {
        let from = alloc::format!("/f{i}");
        executor.declare_remap("n", "/", &from, "/t").unwrap();
    }
    assert_eq!(
        executor.declare_remap("n", "/", "/one_more", "/t"),
        Err(super::RemapRefused::TableFull)
    );
}

private_test_clock!(timer_call_info_clock);

/// Issue 1974 — a `*_with_info` timer receives the dispatcher's call info:
/// `actual - expected` is exactly how far the activation ran past its
/// deadline, and is never negative.
///
/// On a PRIVATE spin clock so each step is the delta it names: a 25 ms step
/// against a 10 ms period is due 15 ms late, and (`Skip`) the 5 ms remainder
/// makes the next 5 ms step exactly on time.
#[test]
fn timer_with_info_reports_expected_and_actual_call_times() {
    use std::sync::{Arc, Mutex};
    timer_call_info_clock::claim_at_us(1_000_000);
    let mut executor: Executor =
        executor_with_clock_fn(MockSession::new(), timer_call_info_clock::now_us);

    let seen: Arc<Mutex<Vec<crate::TimerCallInfo>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    executor
        .register_timer_with_info(
            TimerDuration::from_millis(10),
            super::arena::TimerClockSource::Steady,
            true,
            move |info| sink.lock().unwrap().push(*info),
        )
        .unwrap();

    let r = timer_call_info_clock::elapse_then_spin_once(&mut executor, 4);
    assert_eq!(r.timers_fired, 0, "4 ms is short of the 10 ms period");
    let r = timer_call_info_clock::elapse_then_spin_once(&mut executor, 21);
    assert_eq!(r.timers_fired, 1, "25 ms >= 10 ms");
    let r = timer_call_info_clock::elapse_then_spin_once(&mut executor, 5);
    assert_eq!(r.timers_fired, 1, "the 5 ms remainder + 5 ms is one period");

    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    for info in seen.iter() {
        assert!(
            info.actual_call_time >= info.expected_call_time,
            "an activation can never run BEFORE its deadline: {info:?}"
        );
    }
    assert_eq!(
        seen[0].lateness_ns(),
        15_000_000,
        "25 ms of elapsed time against a 10 ms period is 15 ms late"
    );
    assert_eq!(seen[1].lateness_ns(), 0, "back on the phase grid");
}

private_test_clock!(timer_autostart_clock);

/// Issue 1974 — `autostart = false` creates the timer CANCELLED: it never
/// fires, however long it waits, until `reset_timer` starts it, and then its
/// first period counts from the reset.
#[test]
fn timer_created_without_autostart_fires_only_after_reset() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    timer_autostart_clock::claim_at_us(1_000_000);
    let mut executor: Executor =
        executor_with_clock_fn(MockSession::new(), timer_autostart_clock::now_us);

    let ticks = std::sync::Arc::new(AtomicUsize::new(0));
    let t = ticks.clone();
    let id = executor
        .register_timer_with_info(
            TimerDuration::from_millis(10),
            super::arena::TimerClockSource::Steady,
            false,
            move |_| {
                t.fetch_add(1, Ordering::SeqCst);
            },
        )
        .unwrap();
    assert!(
        executor.timer_is_canceled(id),
        "autostart=false is cancelled"
    );

    for _ in 0..10 {
        let _ = timer_autostart_clock::elapse_then_spin_once(&mut executor, 25);
    }
    assert_eq!(
        ticks.load(Ordering::SeqCst),
        0,
        "250 ms passed and a timer created without autostart fired"
    );

    executor.reset_timer(id).unwrap();
    assert!(!executor.timer_is_canceled(id));
    let r = timer_autostart_clock::elapse_then_spin_once(&mut executor, 9);
    assert_eq!(r.timers_fired, 0, "the period counts from the reset");
    let r = timer_autostart_clock::elapse_then_spin_once(&mut executor, 1);
    assert_eq!(r.timers_fired, 1);
    assert_eq!(ticks.load(Ordering::SeqCst), 1);
}

#[cfg(feature = "sim-time")]
private_test_clock!(ros_timer_info_clock);

/// Issue 1974 — on a CLOCK timer both call times are on that clock, and a
/// timer started late by `reset_timer` does not count the interval it spent
/// cancelled (the reset re-seeds the clock reading, else the first poll would
/// credit the whole cancelled span and fire at once).
#[cfg(feature = "sim-time")]
#[test]
fn ros_time_timer_info_is_on_the_ros_clock_and_reset_reseeds() {
    use nros_core::clock::Clock;
    use std::sync::{Arc, Mutex};
    ros_timer_info_clock::claim_at_us(1_000_000);
    let _sim_time = SimTimeGuard::acquire();
    const MS: i64 = 1_000_000;

    let mut executor: Executor =
        executor_with_clock_fn(MockSession::new(), ros_timer_info_clock::now_us);
    Clock::set_ros_time_override(10_000 * MS);

    let seen: Arc<Mutex<Vec<crate::TimerCallInfo>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    let id = executor
        .register_timer_with_info(
            TimerDuration::from_millis(100),
            super::arena::TimerClockSource::Ros,
            false,
            move |info| sink.lock().unwrap().push(*info),
        )
        .unwrap();

    // Cancelled for 5 s of simulated time.
    Clock::set_ros_time_override(15_000 * MS);
    let _ = ros_timer_info_clock::elapse_then_spin_once(&mut executor, 10);
    assert!(seen.lock().unwrap().is_empty(), "cancelled: never fires");

    executor.reset_timer(id).unwrap();
    let _ = ros_timer_info_clock::elapse_then_spin_once(&mut executor, 10);
    assert!(
        seen.lock().unwrap().is_empty(),
        "reset must start a FRESH period, not credit the 5 s spent cancelled"
    );

    // 130 ms of simulated time after the reset: due 30 ms late.
    Clock::set_ros_time_override(15_130 * MS);
    let _ = ros_timer_info_clock::elapse_then_spin_once(&mut executor, 10);

    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(
        seen[0].actual_call_time,
        15_130 * MS,
        "now on the ROS clock"
    );
    assert_eq!(
        seen[0].expected_call_time,
        15_100 * MS,
        "one period after reset"
    );
    assert!(seen[0].actual_call_time >= seen[0].expected_call_time);
}

private_test_clock!(node_timer_info_clock);

/// Issue 1974 — the node-level spelling, `Node::create_timer_with_info`,
/// reaches the same dispatcher: `autostart = false` waits for `reset_timer`,
/// and the activation reports `actual >= expected` with the exact lateness.
#[test]
fn a_node_level_timer_with_info_honours_autostart_and_reports_lateness() {
    use std::sync::{Arc, Mutex};
    node_timer_info_clock::claim_at_us(1_000_000);
    let mut executor: Executor =
        executor_with_clock_fn(MockSession::new(), node_timer_info_clock::now_us);
    let nid = executor.node_builder("node_timer_info").build().unwrap();

    let seen: Arc<Mutex<Vec<crate::TimerCallInfo>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    let id = executor
        .node_mut(nid)
        .create_timer_with_info(
            TimerDuration::from_millis(20),
            crate::executor::TimerClockSource::Steady,
            false,
            move |info| sink.lock().unwrap().push(*info),
        )
        .unwrap();

    let _ = node_timer_info_clock::elapse_then_spin_once(&mut executor, 100);
    assert!(
        seen.lock().unwrap().is_empty(),
        "autostart=false never fires"
    );

    executor.reset_timer(id).unwrap();
    let _ = node_timer_info_clock::elapse_then_spin_once(&mut executor, 27);
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1, "one period after the reset");
    assert!(seen[0].actual_call_time >= seen[0].expected_call_time);
    assert_eq!(
        seen[0].lateness_ns(),
        7_000_000,
        "27 ms against a 20 ms period"
    );
    assert_eq!(seen[0].clock, crate::executor::TimerClockSource::Steady);
}

/// phase-509 P9b item 2 (RFC-0108 "As built in P9b") -- worker passes over
/// one executor that release its lock around each callback.
mod p9b_windows {
    use super::*;
    use crate::executor::{
        Band, BandWorker,
        exec_lock::{ExecGuard, ExecLock},
    };
    use std::{
        sync::{
            Arc, Condvar, Mutex,
            atomic::{AtomicUsize, Ordering},
        },
        vec::Vec,
    };

    /// A recursive lock standing in for the port's
    /// (`nros_platform_mutex_rec_*`): owner thread and depth under a mutex.
    struct RecLock {
        state: Mutex<(usize, u32)>,
        cv: Condvar,
    }

    unsafe extern "C" fn thread_id() -> usize {
        std::thread_local!(static ID: u8 = const { 0 });
        ID.with(|x| x as *const u8 as usize)
    }

    unsafe extern "C" fn rec_lock(c: *mut core::ffi::c_void) -> i8 {
        // SAFETY: the tests pass a leaked `RecLock` as the context.
        let l = unsafe { &*(c as *const RecLock) };
        // SAFETY: `thread_id` is callable from any thread.
        let me = unsafe { thread_id() };
        let mut g = l.state.lock().unwrap();
        while g.1 > 0 && g.0 != me {
            g = l.cv.wait(g).unwrap();
        }
        g.0 = me;
        g.1 += 1;
        0
    }

    unsafe extern "C" fn rec_unlock(c: *mut core::ffi::c_void) -> i8 {
        // SAFETY: as `rec_lock`.
        let l = unsafe { &*(c as *const RecLock) };
        let mut g = l.state.lock().unwrap();
        assert!(g.1 > 0, "unlock of a lock this thread does not hold");
        g.1 -= 1;
        if g.1 == 0 {
            g.0 = 0;
            l.cv.notify_all();
        }
        0
    }

    /// The executor's lock, with a thread identity: passes open windows.
    fn install_lock(executor: &mut Executor<'static>) {
        let lock: &'static RecLock = std::boxed::Box::leak(std::boxed::Box::new(RecLock {
            state: Mutex::new((0, 0)),
            cv: Condvar::new(),
        }));
        // SAFETY: the lock is leaked, so it outlives the executor; `thread_id`
        // tells live threads apart (a thread-local's address).
        let l = unsafe {
            ExecLock::new(lock as *const RecLock as *mut _, rec_lock, rec_unlock)
                .unwrap()
                .with_thread_id(thread_id)
        };
        executor.set_exec_lock(Some(l));
    }

    /// The executor, reachable from several threads; every use takes its
    /// lock first.
    #[derive(Clone, Copy)]
    struct Shared(*mut Executor<'static>);
    // SAFETY: every thread that dereferences it holds the executor's lock
    // (`ExecGuard::acquire`) except inside a callback window, where it touches
    // nothing the window does not allow.
    unsafe impl Send for Shared {}
    // SAFETY: as above.
    unsafe impl Sync for Shared {}

    impl Shared {
        /// The pointer, by a method so a closure captures the whole `Shared`.
        fn ptr(self) -> *mut Executor<'static> {
            self.0
        }

        fn spin_shared(self) -> SpinOnceResult {
            // SAFETY: the lock is taken before the pass and held once.
            let _g = unsafe { ExecGuard::acquire(self.0) };
            unsafe { Executor::spin_once_shared(self.0, core::time::Duration::ZERO) }
        }

        fn spin_band(self, w: &mut BandWorker) -> SpinOnceResult {
            // SAFETY: as `spin_shared`.
            let _g = unsafe { ExecGuard::acquire(self.0) };
            unsafe { Executor::spin_band_once(self.0, w, core::time::Duration::ZERO) }
        }
    }

    /// A node of contract rank `rank` with one 1 ms timer running `f`.
    fn ranked_timer(
        executor: &mut Executor<'static>,
        name: &str,
        rank: u8,
        f: impl FnMut(&crate::TimerCallInfo) + Send + 'static,
    ) -> HandleId {
        executor.bind_node_rank(name, "/", rank).unwrap();
        let nid = executor.node_builder(name).build().unwrap();
        executor
            .node_mut(nid)
            .create_timer_with_info(
                TimerDuration::from_millis(1),
                crate::executor::TimerClockSource::Steady,
                true,
                f,
            )
            .unwrap()
    }

    /// Item 2, test 1 -- two threads spin one executor and one timer is due
    /// on every pass: its callback never runs twice at once (the running
    /// mark), and the lock IS released while it runs (another thread takes
    /// it from inside the callback).
    #[test]
    fn two_workers_never_run_one_callback_at_once() {
        let mut executor = executor_with_clock(MockSession::new());
        install_lock(&mut executor);
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let runs = Arc::new(AtomicUsize::new(0));
        let reached = Arc::new(AtomicUsize::new(0));
        let shared = Shared(&mut executor);
        let (a, p, r, re) = (active.clone(), peak.clone(), runs.clone(), reached.clone());
        let id = std::sync::OnceLock::<HandleId>::new();
        let id = std::boxed::Box::leak(std::boxed::Box::new(id));
        let idr: &'static std::sync::OnceLock<HandleId> = id;
        let t = ranked_timer(&mut executor, "solo", 0, move |_| {
            let now = a.fetch_add(1, Ordering::SeqCst) + 1;
            p.fetch_max(now, Ordering::SeqCst);
            r.fetch_add(1, Ordering::SeqCst);
            // The window is open: a third thread takes the lock and asks
            // about this very timer. Under a held lock this would deadlock.
            std::thread::scope(|sc| {
                sc.spawn(|| {
                    // SAFETY: the lock is taken before the executor is read.
                    let _g = unsafe { ExecGuard::acquire(shared.ptr()) };
                    let _ = unsafe { &*shared.ptr() }.timer_is_canceled(*idr.get().unwrap());
                    re.fetch_add(1, Ordering::SeqCst);
                });
            });
            std::thread::sleep(std::time::Duration::from_micros(300));
            a.fetch_sub(1, Ordering::SeqCst);
        });
        id.set(t).unwrap();
        std::thread::scope(|sc| {
            for _ in 0..2 {
                sc.spawn(move || {
                    for _ in 0..200 {
                        shared.spin_shared();
                        std::thread::sleep(std::time::Duration::from_micros(200));
                    }
                });
            }
        });
        assert!(runs.load(Ordering::SeqCst) >= 2, "the timer ran");
        assert_eq!(peak.load(Ordering::SeqCst), 1, "never two runs at once");
        assert_eq!(
            reached.load(Ordering::SeqCst),
            runs.load(Ordering::SeqCst),
            "every callback ran with the lock released"
        );
        drop(executor);
    }

    /// Item 2, test 2 -- two bands over one executor: each worker runs only
    /// its own band's callbacks, and the urgent band's worker runs while a
    /// less urgent callback is in its window, without taking that band's work.
    #[test]
    fn a_band_worker_runs_only_its_band() {
        let mut executor = executor_with_clock(MockSession::new());
        install_lock(&mut executor);
        let log = Arc::new(Mutex::new(Vec::<&'static str>::new()));
        let shared = Shared(&mut executor);
        let hi_band = Band::new(0, 1).unwrap(); // rank 0 -> level 1
        let lo_band = Band::new(2, 31).unwrap(); // rank 1 -> level 2
        let l = log.clone();
        ranked_timer(&mut executor, "hi", 0, move |_| {
            l.lock().unwrap().push("hi")
        });
        let l = log.clone();
        let nested = Arc::new(AtomicUsize::new(0));
        let n = nested.clone();
        ranked_timer(&mut executor, "lo", 1, move |_| {
            l.lock().unwrap().push("lo");
            if n.fetch_add(1, Ordering::SeqCst) == 0 {
                // Preempted, in effect: the urgent worker runs its band to
                // completion while this callback is in its window.
                let l2 = l.clone();
                std::thread::scope(|sc| {
                    sc.spawn(move || {
                        let mut hi = BandWorker::new(0, hi_band);
                        for _ in 0..20 {
                            shared.spin_band(&mut hi);
                            std::thread::sleep(std::time::Duration::from_micros(300));
                        }
                        l2.lock().unwrap().push("hi-done");
                    });
                });
            }
            l.lock().unwrap().push("lo-end");
        });
        // The lo worker alone: only lo runs (hi stays due, untaken).
        let mut lo = BandWorker::new(1, lo_band);
        for _ in 0..3 {
            shared.spin_band(&mut lo);
            std::thread::sleep(std::time::Duration::from_micros(1_500));
        }
        let got = log.lock().unwrap().clone();
        let first_lo = got.iter().position(|e| *e == "lo").expect("lo ran");
        let lo_end = got.iter().position(|e| *e == "lo-end").unwrap();
        // Everything between lo's start and end came from the hi worker.
        let inner = &got[first_lo + 1..lo_end];
        assert!(inner.contains(&"hi"), "hi ran inside lo's window: {got:?}");
        assert_eq!(inner.last(), Some(&"hi-done"), "{got:?}");
        assert!(
            inner.iter().all(|e| *e == "hi" || *e == "hi-done"),
            "the hi worker ran only hi: {got:?}"
        );
        // Outside the window only the lo worker ran: never hi.
        assert!(
            got[..first_lo].is_empty() && got[lo_end + 1..].iter().all(|e| e.starts_with("lo")),
            "the lo worker never ran hi: {got:?}"
        );
        drop(executor);
    }

    /// Issue 2020 -- a component enrolled for the tests: its `tick` logs its
    /// tag and the thread it ran on, and, given an action server, completes
    /// every active goal of it.
    struct Ticker {
        tag: &'static str,
        log: Arc<Mutex<Vec<(&'static str, usize)>>>,
        server: Option<crate::executor::ActionServerRawHandle>,
    }

    unsafe extern "C" fn ticker_tick(state: *mut core::ffi::c_void, exec: *mut core::ffi::c_void) {
        // SAFETY: the tests enrol a leaked `Ticker` and the executor hands
        // itself as `exec`, the lock held.
        let t = unsafe { &*(state as *const Ticker) };
        let me = unsafe { thread_id() };
        t.log.lock().unwrap().push((t.tag, me));
        if let Some(server) = t.server {
            let exec = unsafe { &mut *(exec as *mut Executor<'static>) };
            let mut goals = Vec::new();
            server.for_each_active_goal(exec, |g| goals.push(g.goal_id));
            for g in goals {
                server
                    .complete_goal_raw(exec, &g, nros_core::GoalStatusCode::Succeeded, &[0u8; 4])
                    .unwrap();
                t.log.lock().unwrap().push(("completed", me));
            }
        }
    }

    unsafe extern "C" fn ticker_drop(_state: *mut core::ffi::c_void) {}

    fn enrol_ticker(
        executor: &mut Executor<'static>,
        tag: &'static str,
        log: &Arc<Mutex<Vec<(&'static str, usize)>>>,
        server: Option<crate::executor::ActionServerRawHandle>,
    ) {
        let t = std::boxed::Box::leak(std::boxed::Box::new(Ticker {
            tag,
            log: log.clone(),
            server,
        }));
        // SAFETY: `t` is leaked, so it outlives the executor; the drop is a
        // no-op for it.
        unsafe {
            executor
                .enroll_component(t as *mut Ticker as *mut _, ticker_tick, ticker_drop)
                .unwrap()
        };
    }

    fn tags(log: &Arc<Mutex<Vec<(&'static str, usize)>>>) -> Vec<&'static str> {
        log.lock().unwrap().iter().map(|(t, _)| *t).collect()
    }

    /// Issue 2020, test 1 -- on an executor whose levels own their slots
    /// (a level runner), a component ticks on the pass of the level that
    /// registered it: a band pass ticks only its level's, the executor's own
    /// worker (the boot level) only its own. Without level ownership the
    /// executor's own worker ticks every component and a band pass none, as
    /// before.
    #[test]
    fn a_component_ticks_on_the_level_that_registered_it() {
        let mut executor = executor_with_clock(MockSession::new());
        install_lock(&mut executor);
        let log = Arc::new(Mutex::new(Vec::new()));

        // Before levels: every tick is the executor's own worker's.
        enrol_ticker(&mut executor, "plain", &log, None);
        let shared = Shared(&mut executor);
        let mut w1 = BandWorker::new(1, Band::ALL);
        shared.spin_band(&mut w1);
        assert!(
            tags(&log).is_empty(),
            "a band pass ticked: {:?}",
            tags(&log)
        );
        shared.spin_shared();
        assert_eq!(tags(&log), ["plain"]);
        log.lock().unwrap().clear();

        // A level runner: boot level 1 of 2, the urgent level 0 set up second.
        assert!(executor.set_level(1));
        executor.set_registering_level(1);
        enrol_ticker(&mut executor, "boot", &log, None);
        executor.set_registering_level(0);
        enrol_ticker(&mut executor, "urgent", &log, None);
        let shared = Shared(&mut executor);
        let mut w0 = BandWorker::new(0, Band::ALL);

        shared.spin_band(&mut w0);
        assert_eq!(tags(&log), ["urgent"], "the urgent pass ticks its own");
        log.lock().unwrap().clear();
        shared.spin_shared();
        // `plain` was enrolled before any level owned a slot: the boot
        // level keeps it.
        assert_eq!(tags(&log), ["plain", "boot"], "the boot pass ticks its own");
        drop(executor);
    }

    /// Issue 2050 -- the spin-quantization audit judges only the timers the
    /// executor's own worker runs. In a level runner that worker is the boot
    /// level, paced at the boot level's spin period; a 1 ms timer a spawned
    /// level registered runs on that level's band worker and was warned about
    /// against the boot level's 10 ms spin.
    #[test]
    fn the_spin_quantization_audit_judges_only_its_own_levels_timers() {
        let mut executor = executor_with_clock(MockSession::new());
        let quantized = |e: &Executor, spin_us| {
            let mut v = Vec::new();
            e.spin_quantized_timers(spin_us, |p, _, _| v.push(p));
            v
        };

        // A one-level executor: every timer is its own worker's.
        executor
            .register_timer(TimerDuration::from_millis(25), || {})
            .unwrap();
        assert_eq!(quantized(&executor, 10_000), [25_000]);

        // A level runner: boot level 1 (the slow one), urgent level 0.
        let mut executor = executor_with_clock(MockSession::new());
        assert!(executor.set_level(1));
        executor.set_registering_level(0);
        executor
            .register_timer(TimerDuration::from_millis(1), || {})
            .unwrap();
        executor.set_registering_level(1);
        executor
            .register_timer(TimerDuration::from_millis(15), || {})
            .unwrap();
        // The urgent level's 1 ms timer is not judged against the boot
        // level's spin; the boot level's own 15 ms timer still is.
        assert_eq!(quantized(&executor, 10_000), [15_000]);
    }

    /// Issue 2020, test 2 -- the issue's fixture: an urgent level's action
    /// server completes its goal on the urgent level's own thread. The goal
    /// is accepted by the urgent band pass and completed by the tick that
    /// band pass runs; the boot level, spinning beside it on another thread,
    /// neither accepts nor completes it.
    #[test]
    fn an_urgent_levels_action_server_completes_on_its_own_level() {
        use crate::executor::{RawActionServerSpec, arena::ActionServerRawArenaEntry};
        const B: usize = crate::config::DEFAULT_RX_BUF_SIZE;
        const MG: usize = 4;

        let mut executor = executor_with_clock(MockSession::new());
        install_lock(&mut executor);
        let log = Arc::new(Mutex::new(Vec::new()));
        assert!(executor.set_level(1));

        // The urgent level's setup: the server and the component that
        // completes its goals.
        executor.set_registering_level(0);
        let server = executor
            .register_action_server_raw(RawActionServerSpec {
                node_id: None,
                action_name: "/urgent_verb",
                type_name: "test_msgs/action/Fibonacci",
                type_hash: "RIHS01_0000",
                hashes: nros_rmw::ActionEndpointHashes::uniform("RIHS01_0000"),
                qos: nros_rmw::QoSProfile::services_default(),
                goal_callback: defer_every_goal,
                cancel_callback: refuse_every_cancel,
                accepted_callback: None,
                context: core::ptr::null_mut(),
            })
            .unwrap();
        enrol_ticker(&mut executor, "urgent", &log, Some(server));
        // The boot level's setup.
        executor.set_registering_level(1);
        enrol_ticker(&mut executor, "boot", &log, None);

        let offset = executor.entries[server.entry_index]
            .as_ref()
            .unwrap()
            .offset;
        let entry_ptr = unsafe { (executor.arena.as_mut_ptr() as *mut u8).add(offset) };
        let entry = unsafe { &mut *(entry_ptr as *mut ActionServerRawArenaEntry<B, B, B, MG>) };
        let g = nros_core::GoalUuid { uuid: [0x20u8; 16] };
        let (req, len) = mk_send_goal_req(&g);
        entry.core.send_goal_server.load(req, len);

        let shared = Shared(&mut executor);
        // The boot level alone: the goal waits for its level.
        for _ in 0..3 {
            shared.spin_shared();
        }
        assert_eq!(server.active_goal_count(unsafe { &*shared.ptr() }), 0);
        assert!(
            !tags(&log).contains(&"urgent") && !tags(&log).contains(&"completed"),
            "the boot level ran the urgent level's work: {:?}",
            tags(&log)
        );

        let urgent_log = log.clone();
        let urgent_thread = std::thread::scope(|sc| {
            let urgent = sc.spawn(move || {
                let mut w0 = BandWorker::new(0, Band::ALL);
                for _ in 0..200 {
                    shared.spin_band(&mut w0);
                    if log_has(&urgent_log, "completed") {
                        break;
                    }
                }
                // SAFETY: callable from any thread.
                unsafe { thread_id() }
            });
            // The boot level keeps spinning on this thread meanwhile.
            while !urgent.is_finished() {
                shared.spin_shared();
                std::thread::sleep(std::time::Duration::from_micros(200));
            }
            urgent.join().unwrap()
        });

        let got = log.lock().unwrap().clone();
        let completed: Vec<usize> = got
            .iter()
            .filter(|(t, _)| *t == "completed")
            .map(|(_, th)| *th)
            .collect();
        assert_eq!(
            completed,
            [urgent_thread],
            "completed once, on the urgent level: {got:?}"
        );
        assert!(
            got.iter()
                .filter(|(t, _)| *t == "urgent")
                .all(|(_, th)| *th == urgent_thread),
            "the urgent component ticked only on its level: {got:?}"
        );
        assert!(
            got.iter()
                .filter(|(t, _)| *t == "boot")
                .all(|(_, th)| *th != urgent_thread),
            "the boot component never ticked on the urgent level: {got:?}"
        );
        // Accepted, then completed: the reply said ACCEPTED and no goal is
        // left active.
        assert_eq!(entry.core.send_goal_server.sent.borrow()[0].1[4], 1);
        assert_eq!(server.active_goal_count(unsafe { &*shared.ptr() }), 0);
        drop(executor);
    }

    fn log_has(log: &Arc<Mutex<Vec<(&'static str, usize)>>>, tag: &str) -> bool {
        log.lock().unwrap().iter().any(|(t, _)| *t == tag)
    }

    /// A port wake object for the tests: a counting semaphore.
    struct Sem {
        count: Mutex<u32>,
        cv: Condvar,
        /// A worker is inside `sem_take` now.
        waiting: core::sync::atomic::AtomicBool,
    }

    unsafe extern "C" fn sem_give(c: *mut core::ffi::c_void) {
        // SAFETY: the tests pass a leaked `Sem`.
        let s = unsafe { &*(c as *const Sem) };
        *s.count.lock().unwrap() += 1;
        s.cv.notify_all();
    }

    unsafe extern "C" fn sem_take(c: *mut core::ffi::c_void, us: u64) -> i32 {
        // SAFETY: as `sem_give`.
        let s = unsafe { &*(c as *const Sem) };
        let g = s.count.lock().unwrap();
        s.waiting.store(true, Ordering::SeqCst);
        let (mut g, _) =
            s.cv.wait_timeout_while(g, std::time::Duration::from_micros(us), |c| *c == 0)
                .unwrap();
        s.waiting.store(false, Ordering::SeqCst);
        if *g > 0 {
            *g -= 1;
            0
        } else {
            -1
        }
    }

    fn leaked_waiter() -> (&'static crate::executor::BandWaiter, *mut core::ffi::c_void) {
        let sem: &'static Sem = std::boxed::Box::leak(std::boxed::Box::new(Sem {
            count: Mutex::new(0),
            cv: Condvar::new(),
            waiting: core::sync::atomic::AtomicBool::new(false),
        }));
        let ctx = sem as *const Sem as *mut core::ffi::c_void;
        // SAFETY: the semaphore is leaked; both entries are thread-safe.
        let w = unsafe { crate::executor::BandWaiter::new(sem_give, sem_take, ctx) };
        (std::boxed::Box::leak(std::boxed::Box::new(w)), ctx)
    }

    /// Item 3 -- a band worker parks, with the lock released, until its
    /// band's next timer is due: a 50 ms timer under a 2 s budget is served
    /// well inside a second, and another thread takes the lock while the
    /// worker is parked (it would block until the pass ended otherwise).
    #[test]
    fn a_band_worker_parks_until_its_timer_is_due() {
        let mut executor = executor_with_clock(MockSession::new());
        install_lock(&mut executor);
        let fired = Arc::new(AtomicUsize::new(0));
        let f = fired.clone();
        executor.bind_node_rank("t", "/", 0).unwrap();
        let nid = executor.node_builder("t").build().unwrap();
        executor
            .node_mut(nid)
            .create_timer_with_info(
                TimerDuration::from_millis(50),
                crate::executor::TimerClockSource::Steady,
                true,
                move |_| {
                    f.fetch_add(1, Ordering::SeqCst);
                },
            )
            .unwrap();
        let (waiter, ctx) = leaked_waiter();
        let sem = ctx as usize;
        let mut w = BandWorker::new(0, Band::new(0, 1).unwrap()).with_waiter(waiter);
        executor.attach_band_worker(&w);
        let shared = Shared(&mut executor);
        let t0 = std::time::Instant::now();
        let taken_while_parked = Arc::new(AtomicUsize::new(0));
        let p = taken_while_parked.clone();
        std::thread::scope(|sc| {
            sc.spawn(move || {
                // SAFETY: the leaked semaphore.
                let s = unsafe { &*(sem as *const Sem) };
                while !s.waiting.load(Ordering::SeqCst) {
                    std::thread::yield_now();
                }
                // SAFETY: the lock is taken before the executor is read.
                let _g = unsafe { ExecGuard::acquire(shared.ptr()) };
                if s.waiting.load(Ordering::SeqCst) {
                    p.fetch_add(1, Ordering::SeqCst);
                }
            });
            while fired.load(Ordering::SeqCst) == 0 {
                // SAFETY: the lock is taken before the pass and held once.
                let _g = unsafe { ExecGuard::acquire(shared.ptr()) };
                unsafe {
                    Executor::spin_band_once(
                        shared.ptr(),
                        &mut w,
                        core::time::Duration::from_secs(2),
                    )
                };
                assert!(
                    t0.elapsed() < std::time::Duration::from_secs(1),
                    "parked past the timer"
                );
            }
        });
        assert_eq!(
            taken_while_parked.load(Ordering::SeqCst),
            1,
            "another thread took the lock during the park"
        );
        drop(executor);
    }

    /// Item 3 -- a band worker with nothing due parks on its wake object, and
    /// a signal ends the park early.
    #[test]
    fn a_signal_ends_a_band_park() {
        let mut executor = executor_with_clock(MockSession::new());
        install_lock(&mut executor);
        let (waiter, ctx) = leaked_waiter();
        let mut w = BandWorker::new(0, Band::new(0, 1).unwrap()).with_waiter(waiter);
        executor.attach_band_worker(&w);
        let shared = Shared(&mut executor);
        let ctx = ctx as usize;
        let t0 = std::time::Instant::now();
        std::thread::scope(|sc| {
            sc.spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(20));
                // SAFETY: the leaked semaphore.
                unsafe { sem_give(ctx as *mut core::ffi::c_void) };
            });
            // SAFETY: the lock is taken before the pass and held once.
            let _g = unsafe { ExecGuard::acquire(shared.ptr()) };
            unsafe {
                Executor::spin_band_once(shared.ptr(), &mut w, core::time::Duration::from_secs(5))
            };
        });
        let took = t0.elapsed();
        assert!(
            took >= std::time::Duration::from_millis(15),
            "it parked: {took:?}"
        );
        assert!(
            took < std::time::Duration::from_secs(2),
            "the signal ended it: {took:?}"
        );
        drop(executor);
    }

    /// A band filter is not a slot filter by registration: with no lock, an
    /// owned `spin_once` still runs every band (the one-level worker).
    #[test]
    fn the_one_level_worker_still_runs_every_band() {
        let mut executor = executor_with_clock(MockSession::new());
        let log = Arc::new(Mutex::new(Vec::<&'static str>::new()));
        let l = log.clone();
        ranked_timer(&mut executor, "hi", 0, move |_| {
            l.lock().unwrap().push("hi")
        });
        let l = log.clone();
        ranked_timer(&mut executor, "lo", 1, move |_| {
            l.lock().unwrap().push("lo")
        });
        for _ in 0..3 {
            executor.spin_once(core::time::Duration::ZERO);
            std::thread::sleep(std::time::Duration::from_micros(1_500));
        }
        let got = log.lock().unwrap().clone();
        assert!(got.contains(&"hi") && got.contains(&"lo"), "{got:?}");
    }
}

// ============================================================================
// Issue 2029 — CancelGoal's zero-id forms (cancel-all, cancel-before)
// ============================================================================

/// A core holding three EXECUTING goals accepted at 10 s, 20 s and 30 s, plus
/// one already CANCELING goal at 5 s (not cancelable again).
#[cfg(test)]
fn cancel_fixture() -> super::action_core::ActionServerCore<256, 128, 128, 4> {
    use super::action_core::{ActionServerCore, RawActiveGoal};
    use crate::mock::MockPublisher;
    use nros_core::{GoalStatusCode, GoalUuid, Time};

    let mut core: ActionServerCore<256, 128, 128, 4> = ActionServerCore::from_channels(
        MockServiceServer::new(),
        MockServiceServer::new(),
        MockServiceServer::new(),
        MockPublisher::new(),
        MockPublisher::new(),
    );
    for (b, sec, status) in [
        (1u8, 10, GoalStatusCode::Executing),
        (2, 20, GoalStatusCode::Executing),
        (3, 30, GoalStatusCode::Executing),
        (4, 5, GoalStatusCode::Canceling),
    ] {
        let _ = core.active_goals.push(RawActiveGoal {
            goal_id: GoalUuid { uuid: [b; 16] },
            status,
            stamp: Time::new(sec, 0),
        });
    }
    core
}

/// Load a `CancelGoal` request `[CDR header][uuid 16][stamp sec i32][nsec u32]`.
#[cfg(test)]
fn load_cancel(
    core: &super::action_core::ActionServerCore<256, 128, 128, 4>,
    id: u8,
    sec: i32,
    nanosec: u32,
) {
    let mut b = [0u8; 256];
    b[..4].copy_from_slice(&[0x00, 0x01, 0x00, 0x00]);
    b[4..20].copy_from_slice(&[id; 16]);
    b[20..24].copy_from_slice(&sec.to_le_bytes());
    b[24..28].copy_from_slice(&nanosec.to_le_bytes());
    core.cancel_goal_server.load(b, 28);
}

/// The reply's `(return_code, [(uuid first byte, stamp sec)])`.
#[cfg(test)]
fn cancel_reply(
    core: &super::action_core::ActionServerCore<256, 128, 128, 4>,
) -> (i8, Vec<(u8, i32)>) {
    let sent = core.cancel_goal_server.sent.borrow();
    let (_seq, buf, len) = sent.last().expect("a cancel reply was sent");
    let r = &buf[..*len];
    let rc = r[4] as i8;
    let n = u32::from_le_bytes(r[8..12].try_into().unwrap()) as usize;
    let mut goals = Vec::new();
    for i in 0..n {
        let at = 12 + i * 24;
        let sec = i32::from_le_bytes(r[at + 16..at + 20].try_into().unwrap());
        goals.push((r[at], sec));
    }
    (rc, goals)
}

#[cfg(test)]
fn status_of(
    core: &super::action_core::ActionServerCore<256, 128, 128, 4>,
    b: u8,
) -> nros_core::GoalStatusCode {
    core.find_goal_status(&nros_core::GoalUuid { uuid: [b; 16] })
}

#[test]
fn a_zero_id_zero_stamp_cancel_cancels_every_cancelable_goal() {
    use nros_core::{CancelResponse, GoalStatusCode};
    let mut core = cancel_fixture();
    load_cancel(&core, 0, 0, 0);
    let mut asked = Vec::new();
    let out = core
        .try_handle_cancel(|id, st| {
            asked.push((id.uuid[0], st));
            CancelResponse::Accept
        })
        .unwrap();
    assert_eq!(
        out.map(|(id, r)| (id.uuid[0], r)),
        Some((0, CancelResponse::Accept))
    );
    assert_eq!(
        asked,
        std::vec![
            (1, GoalStatusCode::Executing),
            (2, GoalStatusCode::Executing),
            (3, GoalStatusCode::Executing)
        ],
        "the callback is asked once per cancelable goal, never about the CANCELING one"
    );
    assert_eq!(
        cancel_reply(&core),
        (0, std::vec![(1, 10), (2, 20), (3, 30)]),
        "ERROR_NONE, every goal listed with its acceptance stamp"
    );
    for b in 1..=3 {
        assert_eq!(status_of(&core, b), GoalStatusCode::Canceling);
    }
}

#[test]
fn a_zero_id_stamped_cancel_cancels_the_goals_accepted_at_or_before_it() {
    use nros_core::{CancelResponse, GoalStatusCode};
    let mut core = cancel_fixture();
    load_cancel(&core, 0, 20, 0);
    core.try_handle_cancel(|_, _| CancelResponse::Accept)
        .unwrap();
    assert_eq!(cancel_reply(&core), (0, std::vec![(1, 10), (2, 20)]));
    assert_eq!(
        status_of(&core, 3),
        GoalStatusCode::Executing,
        "30 s is after the stamp"
    );
}

#[test]
fn a_named_stamped_cancel_cancels_that_goal_and_those_before_the_stamp() {
    use nros_core::CancelResponse;
    let mut core = cancel_fixture();
    load_cancel(&core, 3, 10, 0);
    core.try_handle_cancel(|_, _| CancelResponse::Accept)
        .unwrap();
    assert_eq!(cancel_reply(&core), (0, std::vec![(1, 10), (3, 30)]));
}

#[test]
fn a_named_unstamped_cancel_cancels_that_goal_alone() {
    use nros_core::CancelResponse;
    let mut core = cancel_fixture();
    load_cancel(&core, 2, 0, 0);
    let mut calls = 0;
    core.try_handle_cancel(|_, _| {
        calls += 1;
        CancelResponse::Accept
    })
    .unwrap();
    assert_eq!(calls, 1);
    assert_eq!(cancel_reply(&core), (0, std::vec![(2, 20)]));
}

#[test]
fn the_cancel_callback_decides_per_goal() {
    use nros_core::{CancelResponse, GoalStatusCode};
    let mut core = cancel_fixture();
    load_cancel(&core, 0, 0, 0);
    core.try_handle_cancel(|id, _| {
        if id.uuid[0] == 2 {
            CancelResponse::Reject
        } else {
            CancelResponse::Accept
        }
    })
    .unwrap();
    assert_eq!(cancel_reply(&core), (0, std::vec![(1, 10), (3, 30)]));
    assert_eq!(
        status_of(&core, 2),
        GoalStatusCode::Executing,
        "the refused goal runs on"
    );

    // Every goal refused: ERROR_REJECTED, nothing listed.
    let mut core = cancel_fixture();
    load_cancel(&core, 0, 0, 0);
    let out = core
        .try_handle_cancel(|_, _| CancelResponse::Reject)
        .unwrap();
    assert_eq!(out.map(|(_, r)| r), Some(CancelResponse::Reject));
    assert_eq!(cancel_reply(&core), (1, std::vec![]));
}

#[test]
fn an_unknown_or_finished_goal_is_answered_without_asking_the_callback() {
    use nros_core::{CancelResponse, GoalStatusCode, GoalUuid};
    let mut core = cancel_fixture();
    load_cancel(&core, 9, 0, 0);
    let mut calls = 0;
    core.try_handle_cancel(|_, _| {
        calls += 1;
        CancelResponse::Accept
    })
    .unwrap();
    assert_eq!(calls, 0);
    assert_eq!(
        cancel_reply(&core),
        (2, std::vec![]),
        "ERROR_UNKNOWN_GOAL_ID"
    );

    core.complete_goal_raw(
        &GoalUuid { uuid: [1; 16] },
        GoalStatusCode::Succeeded,
        &[0u8; 4],
    )
    .unwrap();
    load_cancel(&core, 1, 0, 0);
    core.try_handle_cancel(|_, _| {
        calls += 1;
        CancelResponse::Accept
    })
    .unwrap();
    assert_eq!(calls, 0);
    assert_eq!(
        cancel_reply(&core),
        (3, std::vec![]),
        "ERROR_GOAL_TERMINATED"
    );
}

/// No cancelable goal at all: a cancel-all is REJECTED, as `rcl_action` does.
#[test]
fn a_cancel_all_with_nothing_to_cancel_is_rejected() {
    use nros_core::CancelResponse;
    let mut core = cancel_fixture();
    core.active_goals.clear();
    load_cancel(&core, 0, 0, 0);
    core.try_handle_cancel(|_, _| CancelResponse::Accept)
        .unwrap();
    assert_eq!(cancel_reply(&core), (1, std::vec![]));
}

// =========================================================================
// Issue 2018 -- a level's own scheduling context, and the kernel hand-over
// =========================================================================

private_test_clock!(level_budget_coop_clock);
private_test_clock!(level_budget_kernel_clock);

/// Run `spins` cycles of a level whose one timer costs 2 ms against a 1 us
/// budget per 60 s, and count its dispatches. `kernel` hands the budget to
/// the kernel before the spins.
fn level_budget_dispatches(
    now_us: fn() -> u64,
    advance_ms: fn(u64),
    spin: fn(&mut Executor<'static>, u64),
    kernel: bool,
) -> (usize, bool) {
    use crate::executor::sched_context::{OptUs, SchedClass, SchedContext};
    use core::sync::atomic::{AtomicUsize, Ordering};
    static COOP: AtomicUsize = AtomicUsize::new(0);
    static KERNEL: AtomicUsize = AtomicUsize::new(0);
    let counter: &'static AtomicUsize = if kernel { &KERNEL } else { &COOP };

    let mut executor: Executor = executor_with_clock_fn(MockSession::new(), now_us);
    // The executor runs level 1 itself, as a runner's adopted level does.
    assert!(executor.set_level(1));
    executor.set_registering_level(1);
    let sc = executor
        .set_level_sched_context(SchedContext {
            class: SchedClass::Sporadic,
            budget_us: OptUs::from_us(1),
            period_us: OptUs::from_us(60_000_000),
            ..Default::default()
        })
        .expect("a free context slot");
    let spender = executor
        .register_timer(TimerDuration::from_millis(1), move || {
            counter.fetch_add(1, Ordering::SeqCst);
            advance_ms(2);
        })
        .unwrap();
    let slot = executor.live_slot(spender);
    assert_eq!(
        executor.sched_context_bindings[slot], sc,
        "a callback the level registers binds to the level's own context"
    );
    if kernel {
        assert!(executor.defer_budget_to_kernel(sc));
        assert!(!executor.enforces_budget(sc));
    } else {
        assert!(executor.enforces_budget(sc));
    }
    for _ in 0..5 {
        spin(&mut executor, 50);
    }
    // The next level starts on the default context, not this one.
    executor.set_registering_level(2);
    let other = executor
        .register_timer(TimerDuration::from_millis(1000), || {})
        .unwrap();
    let other_slot = executor.live_slot(other);
    let other_default = executor.sched_context_bindings[other_slot]
        == crate::executor::sched_context::SchedContextId(0);
    (counter.load(Ordering::SeqCst), other_default)
}

/// Issue 2018 -- the executor enforces a level's budget itself (the
/// fallback when the kernel does not): one dispatch spends it and the level
/// is skipped for the rest of the period.
#[test]
fn a_level_budget_the_executor_enforces_skips_the_level() {
    level_budget_coop_clock::claim_at_us(1_000_000);
    let (dispatches, other_default) = level_budget_dispatches(
        level_budget_coop_clock::now_us,
        level_budget_coop_clock::advance_ms,
        |e, ms| {
            let _ = level_budget_coop_clock::elapse_then_spin_once(e, ms);
        },
        false,
    );
    assert_eq!(dispatches, 1, "the cooperative budget throttles the level");
    assert!(other_default, "the next level does not inherit the context");
}

/// Issue 2018 -- once the kernel enforces the level's budget, the executor
/// does not throttle it a second time: the same overrunning callback
/// dispatches on every cycle, and only the kernel demotes the thread.
#[test]
fn a_level_budget_handed_to_the_kernel_is_not_enforced_twice() {
    level_budget_kernel_clock::claim_at_us(1_000_000);
    let (dispatches, other_default) = level_budget_dispatches(
        level_budget_kernel_clock::now_us,
        level_budget_kernel_clock::advance_ms,
        |e, ms| {
            let _ = level_budget_kernel_clock::elapse_then_spin_once(e, ms);
        },
        true,
    );
    assert_eq!(
        dispatches, 5,
        "no cooperative skip once the kernel enforces"
    );
    assert!(other_default, "the next level does not inherit the context");
}

/// Releasing an entry must not touch the budget of the sched context that
/// happens to share its slot number: `sporadic_states` is indexed by context,
/// and `release_entry` used to clear it by entry slot, silently un-budgeting
/// context 1 when the entry in slot 1 was released.
#[test]
fn releasing_an_entry_keeps_the_budget_of_the_context_with_its_number() {
    use crate::executor::sched_context::{OptUs, SchedClass, SchedContext, SchedContextId};
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let sc = executor
        .create_sched_context(SchedContext {
            class: SchedClass::Sporadic,
            budget_us: OptUs::from_us(1_000),
            period_us: OptUs::from_us(10_000),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(sc, SchedContextId(1));
    let budgeted = executor
        .register_timer(TimerDuration::from_millis(10), || {})
        .unwrap();
    executor.bind_handle_to_sched_context(budgeted, sc).unwrap();
    let other = executor
        .register_timer(TimerDuration::from_millis(10), || {})
        .unwrap();
    assert_eq!(
        executor.live_slot(other),
        1,
        "the released entry sits in slot 1"
    );
    assert!(executor.enforces_budget(sc));
    // SAFETY: `other` is not used again.
    assert!(unsafe { executor.release_timer(other) });
    assert!(
        executor.enforces_budget(sc),
        "context 1 keeps its budget when the entry in slot 1 is released"
    );
}

// ============================================================================
// phase-488 — a raw service whose handler's capture the arena holds
// ============================================================================

static CAPTURED_SRV_DROPS: portable_atomic::AtomicUsize = portable_atomic::AtomicUsize::new(0);
const CAPTURED_SRV_MARK: usize = 0x5E7C;

unsafe extern "C" fn captured_srv_drop(ctx: *mut core::ffi::c_void) {
    let words = ctx as *const usize;
    if unsafe { *words.add(1) } == CAPTURED_SRV_MARK {
        CAPTURED_SRV_DROPS.fetch_add(1, portable_atomic::Ordering::SeqCst);
    }
}

/// A C++ service handler with state (`std::bind(&X::f, this, _1, _2)`, a
/// capturing lambda) is a capture, not a context word. Its bytes live at the
/// end of the service's own entry: the destructor hook runs exactly once, on
/// release, reading the capture while it is still the capture; and a
/// create/release loop reuses the region instead of growing the arena.
#[test]
fn a_capturing_raw_service_destroys_and_releases_its_capture() {
    use portable_atomic::Ordering::SeqCst;
    let mut executor: Executor = executor_with_clock(MockSession::new());
    let capture: [usize; 4] = [0, CAPTURED_SRV_MARK, 0, 0];
    let bytes = unsafe {
        core::slice::from_raw_parts(
            capture.as_ptr() as *const u8,
            core::mem::size_of_val(&capture),
        )
    };
    let register = |e: &mut Executor<'_>| {
        e.register_service_raw_capturing::<
            { crate::config::DEFAULT_RX_BUF_SIZE },
            { crate::config::DEFAULT_RX_BUF_SIZE },
        >(
            None,
            "/capture_srv",
            "test/srv/T",
            "h",
            QoSProfile::default(),
            release_probe_srv,
            Some(bytes),
            Some(captured_srv_drop),
        )
    };
    let before = CAPTURED_SRV_DROPS.load(SeqCst);
    let first = register(&mut executor).expect("register");
    assert_eq!(
        CAPTURED_SRV_DROPS.load(SeqCst),
        before,
        "registering drops nothing"
    );
    let high_water = executor.arena_used();
    assert!(unsafe { executor.release_service(first) });
    assert_eq!(
        CAPTURED_SRV_DROPS.load(SeqCst),
        before + 1,
        "release drops the capture once"
    );
    assert!(!unsafe { executor.release_service(first) });
    assert_eq!(
        CAPTURED_SRV_DROPS.load(SeqCst),
        before + 1,
        "a second release drops nothing"
    );
    for i in 0..200 {
        let h = register(&mut executor).expect("re-register");
        assert_eq!(
            executor.arena_used(),
            high_water,
            "iteration {i}: the arena grew; the capture was not released with its entry"
        );
        assert!(unsafe { executor.release_service(h) });
    }
    assert_eq!(CAPTURED_SRV_DROPS.load(SeqCst), before + 201);
}

/// phase-518 W2 — a `Skip` timer whose callback reset it (elapsed below one
/// period when the callback returns) misses nothing and keeps no remainder,
/// where `elapsed / period - 1` underflowed.
#[test]
fn skip_overrun_of_a_timer_reset_by_its_own_callback_is_zero() {
    use super::arena::skip_overrun;
    assert_eq!(skip_overrun(0, 1_000), (0, 0), "reset inside the callback");
    assert_eq!(skip_overrun(1_000, 1_000), (0, 0), "fired on time");
    assert_eq!(skip_overrun(3_250, 1_000), (2, 250), "two periods missed");
}

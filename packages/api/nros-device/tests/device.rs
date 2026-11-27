//! Device ports driven through a real executor, over a stub RMW backend
//! registered by name: no network, no router.

extern crate nros_platform_cffi as _;

use core::{
    ffi::{c_char, c_void},
    task::Poll,
    time::Duration,
};
use std::sync::{Arc, Mutex, Once};

use nros_device::{
    Capture, Completion, DeviceError, PortCell, Raw, ResultSink, Source, SourceHandle, Stage,
    StageResult, Worker, WorkerHandle, WorkerPort, WorkerPortOps,
    conformance::{self, ConformanceFailure},
    mock::{MockControl, MockMode, MockSource, MockSourceControl, MockWorker},
    sys,
};
use nros_node::executor::{Executor, ExecutorConfig};
use nros_rmw_cffi::{
    EMPTY_VTABLE, NROS_RMW_READINESS_NOTIFY, NROS_RMW_RET_OK, NROS_RMW_RET_UNSUPPORTED,
    NrosRmwClient, NrosRmwNode, NrosRmwPublisher, NrosRmwQos, NrosRmwRet, NrosRmwService,
    NrosRmwSession, NrosRmwSessionOptions, NrosRmwSubscription, NrosRmwVtable,
    nros_rmw_cffi_register_named, rmw_session_t,
};

// ------------------------------------------------------------ stub backend

// The full stub set: registration refuses a vtable with a missing slot.
unsafe extern "C" fn stub_open(
    _: *const c_char,
    _: u8,
    _: u32,
    _: *const c_char,
    _: *const NrosRmwSessionOptions,
    out: *mut NrosRmwSession,
) -> NrosRmwRet {
    unsafe { (*out).backend_data = std::ptr::dangling_mut::<c_void>() };
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_close(_: *mut NrosRmwSession) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_drive_io(_: *mut NrosRmwSession, _: i32) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_create_publisher(
    _: *const NrosRmwNode,
    _: *const nros_rmw_cffi::generated::rmw_message_type_support_t,
    _: *const c_char,
    _: u32,
    _: *const NrosRmwQos,
    _: *const nros_rmw_cffi::rmw_publisher_options_t,
    _: *mut NrosRmwPublisher,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn stub_destroy_publisher(_: *mut NrosRmwPublisher) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_publish_raw(
    _: *const NrosRmwPublisher,
    _: nros_rmw_cffi::generated::rmw_byte_span_t,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn stub_create_subscription(
    _: *const NrosRmwNode,
    _: *const nros_rmw_cffi::generated::rmw_message_type_support_t,
    _: *const core::ffi::c_char,
    _: u32,
    _: *const NrosRmwQos,
    _: *const nros_rmw_cffi::rmw_subscription_options_t,
    _: *mut NrosRmwSubscription,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn stub_destroy_subscription(_: *mut NrosRmwSubscription) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_take(
    _: *const NrosRmwSubscription,
    _: *mut nros_rmw_cffi::generated::rmw_mut_byte_span_t,
    taken: *mut bool,
) -> NrosRmwRet {
    unsafe { *taken = false };
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_has_data(
    _: *mut NrosRmwSubscription,
    out_has_data: *mut bool,
) -> NrosRmwRet {
    // Phase 376 W3.d step A — flag out, status returned.
    unsafe { *out_has_data = false };
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_create_service(
    _: *const NrosRmwNode,
    _: *const nros_rmw_cffi::generated::rmw_service_type_support_t,
    _: *const core::ffi::c_char,
    _: u32,
    _: *const NrosRmwQos,
    _: *mut NrosRmwService,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn stub_destroy_service(_: *mut NrosRmwService) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_take_request(
    _: *const NrosRmwService,
    _: *mut nros_rmw_cffi::generated::rmw_mut_byte_span_t,
    _: *mut i64,
    taken: *mut bool,
) -> NrosRmwRet {
    unsafe { *taken = false };
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_has_request(
    _: *mut NrosRmwService,
    out_has_request: *mut bool,
) -> NrosRmwRet {
    // Phase 376 W3.d step A — flag out, status returned.
    unsafe { *out_has_request = false };
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_send_response(
    _: *const NrosRmwService,
    _: i64,
    _: nros_rmw_cffi::generated::rmw_byte_span_t,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn stub_create_client(
    _: *const NrosRmwNode,
    _: *const nros_rmw_cffi::generated::rmw_service_type_support_t,
    _: *const c_char,
    _: u32,
    _: *const NrosRmwQos,
    out: *mut NrosRmwClient,
) -> NrosRmwRet {
    unsafe { (*out).backend_data = std::ptr::dangling_mut::<c_void>() };
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_destroy_client(_: *mut NrosRmwClient) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
/// The server is never visible, so the wait runs to its deadline.
unsafe extern "C" fn stub_server_available(
    _: *const NrosRmwClient,
    out_available: *mut bool,
) -> NrosRmwRet {
    unsafe { *out_available = false };
    NROS_RMW_RET_OK
}
/// The stub honours what the test asks for: RELIABLE, VOLATILE, KEEP_LAST, depth.
unsafe extern "C" fn stub_qos(_: *const rmw_session_t, out_mask: *mut u32) -> NrosRmwRet {
    unsafe { *out_mask = 0b1_1011 };
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_readiness(
    _: *const rmw_session_t,
    out_kind: *mut u8,
    out_handle: *mut i32,
) -> NrosRmwRet {
    unsafe {
        *out_kind = NROS_RMW_READINESS_NOTIFY as u8;
        *out_handle = -1;
    }
    NROS_RMW_RET_OK
}

static VTABLE: NrosRmwVtable = NrosRmwVtable {
    create_session: Some(stub_open),
    destroy_session: Some(stub_close),
    drive_io: Some(stub_drive_io),
    create_publisher: Some(stub_create_publisher),
    destroy_publisher: Some(stub_destroy_publisher),
    publish: Some(stub_publish_raw),
    create_subscription: Some(stub_create_subscription),
    destroy_subscription: Some(stub_destroy_subscription),
    take: Some(stub_take),
    has_data: Some(stub_has_data),
    create_service: Some(stub_create_service),
    destroy_service: Some(stub_destroy_service),
    take_request: Some(stub_take_request),
    has_request: Some(stub_has_request),
    send_response: Some(stub_send_response),
    create_client: Some(stub_create_client),
    destroy_client: Some(stub_destroy_client),
    service_server_is_available: Some(stub_server_available),
    readiness_source: Some(stub_readiness),
    supported_qos_policies: Some(stub_qos),
    ..EMPTY_VTABLE
};

fn executor() -> Executor<'static> {
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| {
        // SAFETY: a NUL-terminated name and a vtable that lives for the process.
        let ret = unsafe { nros_rmw_cffi_register_named(c"device_stub".as_ptr(), &VTABLE) };
        assert_eq!(ret, NROS_RMW_RET_OK, "register the stub backend");
    });
    Executor::open(&ExecutorConfig::new("stub/0")).expect("an executor over the stub")
}

fn spin(executor: &mut Executor<'static>) {
    executor.spin_once(Duration::ZERO);
}

fn job(input: &'static [u8], output: &'static mut [u8]) -> sys::nros_device_job_t {
    sys::nros_device_job_t {
        op: core::ptr::null(),
        input: sys::nros_device_buffer_t {
            data: input.as_ptr() as *mut c_void,
            size: input.len(),
        },
        output: sys::nros_device_buffer_t {
            data: output.as_mut_ptr().cast(),
            size: output.len(),
        },
    }
}

fn leak_buf(n: usize) -> &'static mut [u8] {
    Box::leak(vec![0u8; n].into_boxed_slice())
}

type Results = Arc<Mutex<Vec<StageResult>>>;

fn stage(
    executor: &mut Executor<'static>,
    device: &'static WorkerHandle<Raw>,
) -> (Stage<Raw>, Results) {
    let results: Results = Arc::default();
    let sink = results.clone();
    let worker = Worker::open(executor, device).expect("open the worker");
    let stage = Stage::open(executor, worker, move |r| sink.lock().unwrap().push(r))
        .expect("register the stage");
    (stage, results)
}

// ------------------------------------------------------------ workers

/// A job completed inside `submit` reaches the stage's callback at the next
/// spin, with the input copied out and the input's capture time carried.
#[test]
fn an_immediate_job_reaches_the_stage_with_its_capture_time() {
    static CTRL: MockControl = MockControl::new(MockMode::Immediate);
    static NPU: PortCell<MockWorker> = PortCell::new(MockWorker::new(&CTRL));
    static NPU0: WorkerHandle<Raw> = WorkerHandle::new(&NPU);

    let mut ex = executor();
    let (stage, results) = stage(&mut ex, &NPU0);
    let out = leak_buf(4);
    let out_ptr = out.as_ptr();
    let capture = Capture::new(Duration::from_millis(7), 1_700_000_000_123_456_789);
    stage.submit(&job(b"abcd", out), capture).unwrap();
    assert!(results.lock().unwrap().is_empty(), "nothing before a spin");

    spin(&mut ex);
    let got = results.lock().unwrap();
    assert_eq!(got.len(), 1);
    let r = got[0];
    assert_eq!(r.status(), Ok(()));
    assert_eq!(r.length(), 4);
    assert_eq!(r.output().data as *const u8, out_ptr);
    // SAFETY: the leaked output buffer, written by the mock.
    assert_eq!(unsafe { std::slice::from_raw_parts(out_ptr, 4) }, b"abcd");
    assert_eq!(r.capture(), capture, "the capture pair travels unchanged");
    assert_eq!(r.stamp().sec, 1_700_000_000);
    assert_eq!(r.stamp().nanosec, 123_456_789);
}

/// A device still working holds no result; one job at a time; the result
/// arrives once the device finishes.
#[test]
fn a_busy_worker_refuses_a_second_job_until_its_result_arrives() {
    static CTRL: MockControl = MockControl::new(MockMode::Manual);
    static NPU: PortCell<MockWorker> = PortCell::new(MockWorker::new(&CTRL));
    static NPU0: WorkerHandle<Raw> = WorkerHandle::new(&NPU);

    let mut ex = executor();
    let (stage, results) = stage(&mut ex, &NPU0);
    stage
        .submit(&job(b"xy", leak_buf(2)), Capture::default())
        .unwrap();
    assert_eq!(
        stage.submit(&job(b"zz", leak_buf(2)), Capture::default()),
        Err(DeviceError::Busy)
    );
    spin(&mut ex);
    assert!(results.lock().unwrap().is_empty(), "not finished yet");

    assert!(CTRL.finish());
    spin(&mut ex);
    assert_eq!(results.lock().unwrap().len(), 1);
    assert_eq!(
        CTRL.submitted(),
        1,
        "the refused job never reached the port"
    );

    stage
        .submit(&job(b"zz", leak_buf(2)), Capture::default())
        .unwrap();
    assert!(CTRL.finish());
    spin(&mut ex);
    assert_eq!(
        results.lock().unwrap().len(),
        2,
        "idle again after the result"
    );
}

/// A fault the device reports is the result's outcome, not a lost result.
#[test]
fn a_device_fault_is_the_results_outcome() {
    static CTRL: MockControl = MockControl::new(MockMode::Immediate);
    static NPU: PortCell<MockWorker> = PortCell::new(MockWorker::new(&CTRL));
    static NPU0: WorkerHandle<Raw> = WorkerHandle::new(&NPU);

    let mut ex = executor();
    let (stage, results) = stage(&mut ex, &NPU0);
    CTRL.fail_next(DeviceError::Fault);
    stage
        .submit(&job(b"q", leak_buf(1)), Capture::default())
        .unwrap();
    spin(&mut ex);
    assert_eq!(results.lock().unwrap()[0].status(), Err(DeviceError::Fault));
}

/// A device has one owner.
#[test]
fn a_device_is_opened_once() {
    static CTRL: MockControl = MockControl::new(MockMode::Immediate);
    static NPU: PortCell<MockWorker> = PortCell::new(MockWorker::new(&CTRL));
    static NPU0: WorkerHandle<Raw> = WorkerHandle::new(&NPU);

    let mut ex = executor();
    Worker::open(&mut ex, &NPU0).unwrap();
    assert!(matches!(
        Worker::open(&mut ex, &NPU0),
        Err(DeviceError::AlreadyClaimed)
    ));
}

/// A table of another ABI version is refused at open, never at a job.
#[test]
fn a_table_of_another_abi_version_is_refused() {
    static CTRL: MockControl = MockControl::new(MockMode::Immediate);
    static NPU: PortCell<MockWorker> = PortCell::new(MockWorker::new(&CTRL));
    static GOOD: WorkerPortOps = WorkerPortOps::of(&NPU);
    // A C table in a Rust static needs a `Sync` wrapper, as C's own does not.
    #[repr(transparent)]
    struct CTable(sys::nros_worker_port_ops_t);
    // SAFETY: immutable, and its `user_data` is a `PortCell`.
    unsafe impl Sync for CTable {}
    static BAD_OPS: CTable = CTable(sys::nros_worker_port_ops_t {
        abi_version: 2,
        ..*GOOD.raw()
    });
    // SAFETY: the table is a Raw worker's, version aside.
    static BAD: WorkerHandle<Raw> = unsafe { WorkerHandle::from_c(&BAD_OPS.0) };

    let mut ex = executor();
    assert!(matches!(
        Worker::open(&mut ex, &BAD),
        Err(DeviceError::AbiMismatch)
    ));
}

// A port written in Rust, exported as the C table a C node would name, and
// named back from Rust through that table as a C port is.
static EXPORTED_CTRL: MockControl = MockControl::new(MockMode::Immediate);
static EXPORTED_NPU: PortCell<MockWorker> = PortCell::new(MockWorker::new(&EXPORTED_CTRL));
#[unsafe(no_mangle)]
pub static nros_device_test_npu_ops: WorkerPortOps = WorkerPortOps::of(&EXPORTED_NPU);

mod from_c {
    use super::*;
    unsafe extern "C" {
        static nros_device_test_npu_ops: sys::nros_worker_port_ops_t;
    }
    // SAFETY: the exported table is a Raw worker's.
    pub static NPU0: WorkerHandle<Raw> = unsafe { WorkerHandle::from_c(&nros_device_test_npu_ops) };
}

/// The ABI is the seam: a Rust port reached only through its exported C table
/// runs a job.
#[test]
fn a_rust_port_runs_through_its_exported_c_table() {
    let mut ex = executor();
    let (stage, results) = stage(&mut ex, &from_c::NPU0);
    stage
        .submit(&job(b"seam", leak_buf(4)), Capture::default())
        .unwrap();
    spin(&mut ex);
    assert_eq!(results.lock().unwrap().len(), 1);
    assert_eq!(EXPORTED_CTRL.submitted(), 1);
}

// ------------------------------------------------------------ sources

/// Each item a source produces reaches the callback once, with the capture
/// time the source stamped it with.
#[test]
fn a_source_delivers_each_item_with_its_capture_time() {
    static CTRL: MockSourceControl = MockSourceControl::new();
    static CAM: PortCell<MockSource> = PortCell::new(MockSource::new(&CTRL));
    static CAMERA0: SourceHandle<Raw> = SourceHandle::new(&CAM);

    let mut ex = executor();
    let items: Arc<Mutex<Vec<(usize, Capture)>>> = Arc::default();
    let sink = items.clone();
    let source = Source::open(&mut ex, &CAMERA0, move |item| {
        sink.lock().unwrap().push((item.length(), item.capture()))
    })
    .unwrap();

    assert!(CTRL.push(b"frame-1", 10, 100));
    assert!(CTRL.push(b"frame-22", 20, 200));
    spin(&mut ex);
    assert_eq!(
        *items.lock().unwrap(),
        vec![
            (7, Capture::new(Duration::from_nanos(10), 100)),
            (8, Capture::new(Duration::from_nanos(20), 200)),
        ]
    );
    spin(&mut ex);
    assert_eq!(items.lock().unwrap().len(), 2, "each item once");

    source.stop();
    assert!(
        !CTRL.push(b"late", 30, 300),
        "a stopped source takes nothing"
    );
}

// ------------------------------------------------------------ conformance

/// The mocks keep every rule a port must keep, through their tables.
#[test]
fn the_mocks_conform() {
    static WCTRL: MockControl = MockControl::new(MockMode::Manual);
    static NPU: PortCell<MockWorker> = PortCell::new(MockWorker::new(&WCTRL));
    static NPU0: WorkerHandle<Raw> = WorkerHandle::new(&NPU);
    static ICTRL: MockControl = MockControl::new(MockMode::Immediate);
    static FAST: PortCell<MockWorker> = PortCell::new(MockWorker::new(&ICTRL));
    static FAST0: WorkerHandle<Raw> = WorkerHandle::new(&FAST);
    static SCTRL: MockSourceControl = MockSourceControl::new();
    static CAM: PortCell<MockSource> = PortCell::new(MockSource::new(&SCTRL));
    static CAMERA0: SourceHandle<Raw> = SourceHandle::new(&CAM);

    let mut ex = executor();
    conformance::worker(&mut ex, &NPU0, &job(b"c", leak_buf(1)), || {
        WCTRL.finish();
    })
    .expect("a manual mock worker conforms");
    conformance::worker(&mut ex, &FAST0, &job(b"c", leak_buf(1)), || {})
        .expect("an immediate mock worker conforms");
    conformance::source(&mut ex, &CAMERA0, || {
        SCTRL.push(b"f", 1, 1);
    })
    .expect("the mock source conforms");
}

/// A port that hands out a result it never produced fails conformance: the
/// checks' negative control.
#[test]
fn a_port_with_a_result_before_any_job_fails_conformance() {
    struct Eager;
    impl WorkerPort for Eager {
        type Class = Raw;
        fn submit(
            &mut self,
            _: &sys::nros_device_job_t,
            done: Completion,
        ) -> Result<(), DeviceError> {
            done.signal();
            Ok(())
        }
        fn take(&mut self, _: &mut ResultSink<'_>) -> Poll<Result<(), DeviceError>> {
            Poll::Ready(Ok(()))
        }
    }
    static EAGER: PortCell<Eager> = PortCell::new(Eager);
    static EAGER0: WorkerHandle<Raw> = WorkerHandle::new(&EAGER);

    let mut ex = executor();
    assert_eq!(
        conformance::worker(&mut ex, &EAGER0, &job(b"c", leak_buf(1)), || {}),
        Err(ConformanceFailure::ResultBeforeWork)
    );
}

/// A port that never signals fails conformance rather than hanging it.
#[test]
fn a_port_that_never_signals_fails_conformance() {
    static CTRL: MockControl = MockControl::new(MockMode::Manual);
    static NPU: PortCell<MockWorker> = PortCell::new(MockWorker::new(&CTRL));
    static NPU0: WorkerHandle<Raw> = WorkerHandle::new(&NPU);

    let mut ex = executor();
    assert_eq!(
        conformance::worker(&mut ex, &NPU0, &job(b"c", leak_buf(1)), || {}),
        Err(ConformanceFailure::NeverSignalled)
    );
}

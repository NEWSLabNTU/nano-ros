// RFC-0139 R6: the implementor side. A port is a trait implementation; the
// crate builds the C operation table from it in a `const fn`, so a port written
// in Rust serves a C, C++ or Rust node through the same table.

use core::{cell::UnsafeCell, ffi::c_void, task::Poll};

use crate::{Completion, DeviceError, sys};

/// A device class: what a job and a result are for one kind of device.
///
/// The port ABI carries a type-erased job ([`sys::nros_device_job_t`]); a
/// class gives it a typed view. [`Raw`] is the class whose job is the ABI's
/// own, for a device that takes plain buffers.
pub trait JobClass: 'static {
    /// The job a node submits and a port receives.
    type Job;
    /// The class's view of a type-erased job.
    ///
    /// # Safety
    /// `raw` must be a job built by [`to_raw`](Self::to_raw) of this class.
    unsafe fn view(raw: &sys::nros_device_job_t) -> &Self::Job;
    /// The type-erased job a typed one travels as.
    fn to_raw(job: &Self::Job) -> sys::nros_device_job_t;
}

/// A source class: what one item a source produces is.
pub trait SourceClass: 'static {}

/// The class of a device that takes and produces plain buffers: its job is
/// the ABI's own [`sys::nros_device_job_t`].
pub enum Raw {}

impl JobClass for Raw {
    type Job = sys::nros_device_job_t;
    unsafe fn view(raw: &sys::nros_device_job_t) -> &Self::Job {
        raw
    }
    fn to_raw(job: &Self::Job) -> sys::nros_device_job_t {
        *job
    }
}

impl SourceClass for Raw {}

/// Where a port writes a result: the output buffer, how many bytes the device
/// wrote, and (for a source) when the item was captured.
pub struct ResultSink<'a> {
    raw: &'a mut sys::nros_device_result_t,
}

impl<'a> ResultSink<'a> {
    pub(crate) fn new(raw: &'a mut sys::nros_device_result_t) -> Self {
        Self { raw }
    }

    /// The buffer the result goes in.
    pub fn output(&self) -> sys::nros_device_buffer_t {
        self.raw.output
    }

    /// Name the buffer the result is in (a source's frame buffer, say).
    pub fn set_output(&mut self, output: sys::nros_device_buffer_t) {
        self.raw.output = output;
    }

    /// The bytes of the output the device wrote.
    pub fn set_length(&mut self, length: usize) {
        self.raw.length = length;
    }

    /// When the item was captured: monotonic nanoseconds, and the wall clock
    /// read at the same instant. A source sets it; for a worker the runtime
    /// carries the capture time of the job's input and overwrites this.
    pub fn set_capture(&mut self, captured_at_ns: i64, captured_stamp_ns: i64) {
        self.raw.captured_at_ns = captured_at_ns;
        self.raw.captured_stamp_ns = captured_stamp_ns;
    }
}

/// A worker port: a job goes in, a result comes back.
///
/// The runtime calls a port's operations from the thread of the executor that
/// owns it, one at a time. Only a [`Completion`]'s
/// [`signal_from_isr`](Completion::signal_from_isr) may run in an interrupt
/// handler.
pub trait WorkerPort: Send + 'static {
    /// The device's class.
    type Class: JobClass;

    /// Start `job` on the device and return at once; never block. Called only
    /// when the device is idle. Signal `done` when the device has finished.
    fn submit(
        &mut self,
        job: &<Self::Class as JobClass>::Job,
        done: Completion,
    ) -> Result<(), DeviceError>;

    /// Collect the finished job's result into `out`; never block.
    /// `Poll::Pending` while the device is still working. `Ready(Err(_))` is a
    /// result whose outcome is that error (a fault the device reported).
    fn take(&mut self, out: &mut ResultSink<'_>) -> Poll<Result<(), DeviceError>>;

    /// Whether [`cancel`](Self::cancel) is implemented. When `false`, the
    /// operation table's `cancel` is null, as the ABI allows.
    const CAN_CANCEL: bool = false;

    /// Abandon the running job.
    fn cancel(&mut self) -> Result<(), DeviceError> {
        Err(DeviceError::Unsupported)
    }
}

/// A source port: data produced on the device's own clock.
pub trait SourcePort: Send + 'static {
    /// The device's class.
    type Class: SourceClass;

    /// Start producing. Signal `ready` each time an item is ready.
    fn start(&mut self, ready: Completion) -> Result<(), DeviceError>;

    /// Collect the next ready item into `out`; never block. `Poll::Pending`
    /// when none is ready.
    fn take(&mut self, out: &mut ResultSink<'_>) -> Poll<Result<(), DeviceError>>;

    /// Stop producing.
    fn stop(&mut self);
}

/// A port's state, in a `static`.
///
/// `Sync` because the runtime calls one port's operations only from the
/// executor that owns it, one at a time; this is the one place the crate
/// relies on that. Anything an interrupt handler touches belongs outside the
/// cell, in atomics or a [`CompletionCell`](crate::CompletionCell).
pub struct PortCell<P> {
    port: UnsafeCell<P>,
}

// SAFETY: see the type's documentation. The runtime serialises every access to
// the inner port through the one executor that owns the device, and the port
// itself is `Send`.
unsafe impl<P: Send> Sync for PortCell<P> {}

impl<P> PortCell<P> {
    /// A cell holding `port`.
    pub const fn new(port: P) -> Self {
        Self {
            port: UnsafeCell::new(port),
        }
    }

    const fn user_data(&'static self) -> *mut c_void {
        self as *const Self as *mut c_void
    }

    /// The port behind a table's `user_data`.
    ///
    /// # Safety
    /// `ud` must be the `user_data` of a table built over a `PortCell<P>`, and
    /// no other reference to the port may be live.
    unsafe fn port<'a>(ud: *mut c_void) -> &'a mut P {
        // SAFETY: the caller's contract.
        unsafe { &mut *(*(ud as *const Self)).port.get() }
    }
}

const ABI_V1: u32 = sys::NROS_DEVICE_PORT_ABI_V1 as u32;

fn poll_to_ret(
    polled: Poll<Result<(), DeviceError>>,
    out: *mut sys::nros_device_result_t,
) -> sys::nros_device_ret_t {
    match polled {
        Poll::Pending => sys::NROS_DEVICE_RET_PENDING,
        Poll::Ready(outcome) => {
            // SAFETY: the runtime passes a valid result.
            unsafe {
                (*out).status = match outcome {
                    Ok(()) => sys::NROS_DEVICE_RET_OK,
                    Err(e) => e.to_ret(),
                }
            };
            sys::NROS_DEVICE_RET_OK
        }
    }
}

unsafe extern "C" fn worker_submit<P: WorkerPort>(
    ud: *mut c_void,
    job: *const sys::nros_device_job_t,
    done: sys::nros_completion_t,
) -> sys::nros_device_ret_t {
    if job.is_null() || done._slot.is_null() {
        return sys::NROS_DEVICE_RET_REJECTED;
    }
    // SAFETY: `ud` is this table's own cell; the runtime calls one operation
    // at a time. `job` is a job of the port's class (the handle is typed), and
    // `done` is a handle the executor gave out.
    let (port, job, done) = unsafe {
        (
            PortCell::<P>::port(ud),
            <P::Class as JobClass>::view(&*job),
            Completion::from_raw(done._slot),
        )
    };
    match port.submit(job, done) {
        Ok(()) => sys::NROS_DEVICE_RET_OK,
        Err(e) => e.to_ret(),
    }
}

unsafe extern "C" fn worker_take<P: WorkerPort>(
    ud: *mut c_void,
    out: *mut sys::nros_device_result_t,
) -> sys::nros_device_ret_t {
    if out.is_null() {
        return sys::NROS_DEVICE_RET_REJECTED;
    }
    // SAFETY: as `worker_submit`; `out` is valid for the call.
    let (port, raw) = unsafe { (PortCell::<P>::port(ud), &mut *out) };
    let polled = port.take(&mut ResultSink::new(raw));
    poll_to_ret(polled, out)
}

unsafe extern "C" fn worker_cancel<P: WorkerPort>(ud: *mut c_void) -> sys::nros_device_ret_t {
    // SAFETY: as `worker_submit`.
    match unsafe { PortCell::<P>::port(ud) }.cancel() {
        Ok(()) => sys::NROS_DEVICE_RET_OK,
        Err(e) => e.to_ret(),
    }
}

unsafe extern "C" fn source_start<P: SourcePort>(
    ud: *mut c_void,
    ready: sys::nros_completion_t,
) -> sys::nros_device_ret_t {
    if ready._slot.is_null() {
        return sys::NROS_DEVICE_RET_REJECTED;
    }
    // SAFETY: as `worker_submit`.
    let (port, ready) = unsafe { (PortCell::<P>::port(ud), Completion::from_raw(ready._slot)) };
    match port.start(ready) {
        Ok(()) => sys::NROS_DEVICE_RET_OK,
        Err(e) => e.to_ret(),
    }
}

unsafe extern "C" fn source_take<P: SourcePort>(
    ud: *mut c_void,
    out: *mut sys::nros_device_result_t,
) -> sys::nros_device_ret_t {
    if out.is_null() {
        return sys::NROS_DEVICE_RET_REJECTED;
    }
    // SAFETY: as `worker_take`.
    let (port, raw) = unsafe { (PortCell::<P>::port(ud), &mut *out) };
    let polled = port.take(&mut ResultSink::new(raw));
    poll_to_ret(polled, out)
}

unsafe extern "C" fn source_stop<P: SourcePort>(ud: *mut c_void) {
    // SAFETY: as `worker_submit`.
    unsafe { PortCell::<P>::port(ud) }.stop();
}

/// A worker port's C operation table, built from a [`WorkerPort`] at compile
/// time. Export it to C and C++ nodes with
/// `#[unsafe(no_mangle)] pub static board_npu0_ops: WorkerPortOps = WorkerPortOps::of(&NPU);`
/// (the symbol's type in C is `nros_worker_port_ops_t`).
#[repr(transparent)]
pub struct WorkerPortOps(sys::nros_worker_port_ops_t);

// SAFETY: the table is immutable after construction; `user_data` points at a
// `PortCell`, which is `Sync`.
unsafe impl Sync for WorkerPortOps {}

impl WorkerPortOps {
    /// The table for the port in `cell`.
    pub const fn of<P: WorkerPort>(cell: &'static PortCell<P>) -> Self {
        Self(sys::nros_worker_port_ops_t {
            abi_version: ABI_V1,
            _reserved: 0,
            user_data: cell.user_data(),
            submit: Some(worker_submit::<P>),
            take: Some(worker_take::<P>),
            cancel: if P::CAN_CANCEL {
                Some(worker_cancel::<P>)
            } else {
                None
            },
        })
    }

    pub(crate) const fn empty() -> Self {
        Self(sys::nros_worker_port_ops_t {
            abi_version: 0,
            _reserved: 0,
            user_data: core::ptr::null_mut(),
            submit: None,
            take: None,
            cancel: None,
        })
    }

    /// The table, as C sees it.
    pub const fn raw(&self) -> &sys::nros_worker_port_ops_t {
        &self.0
    }
}

/// A source port's C operation table, built from a [`SourcePort`] at compile
/// time; exported as [`WorkerPortOps`] is.
#[repr(transparent)]
pub struct SourcePortOps(sys::nros_source_port_ops_t);

// SAFETY: as `WorkerPortOps`.
unsafe impl Sync for SourcePortOps {}

impl SourcePortOps {
    /// The table for the port in `cell`.
    pub const fn of<P: SourcePort>(cell: &'static PortCell<P>) -> Self {
        Self(sys::nros_source_port_ops_t {
            abi_version: ABI_V1,
            _reserved: 0,
            user_data: cell.user_data(),
            start: Some(source_start::<P>),
            take: Some(source_take::<P>),
            stop: Some(source_stop::<P>),
        })
    }

    pub(crate) const fn empty() -> Self {
        Self(sys::nros_source_port_ops_t {
            abi_version: 0,
            _reserved: 0,
            user_data: core::ptr::null_mut(),
            start: None,
            take: None,
            stop: None,
        })
    }

    /// The table, as C sees it.
    pub const fn raw(&self) -> &sys::nros_source_port_ops_t {
        &self.0
    }
}

// RFC-0139 R3: the closure tier, over the executor. The component tier (R2)
// and per-stage queue policies (D5, phase-579 W5) build on these.

use core::time::Duration;

use nros_node::executor::{Executor, HandleId};

use crate::{
    Completion, DeviceError,
    handle::{SourceHandle, WorkerHandle},
    port::{JobClass, SourceClass},
    sys,
};

/// When an input was captured: a monotonic time, which a stage's maximum age
/// is measured against, and the wall clock read at the same instant, which a
/// message header takes. The two travel together and never split.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Capture {
    /// Monotonic time of capture.
    pub at: Duration,
    /// The wall clock at the same instant, in nanoseconds since the epoch.
    pub stamp_ns: i64,
}

impl Capture {
    /// A capture at monotonic `at`, with wall-clock `stamp_ns`.
    pub const fn new(at: Duration, stamp_ns: i64) -> Self {
        Self { at, stamp_ns }
    }

    /// The wall-clock stamp as a message header's time.
    pub fn stamp(&self) -> nros::Time {
        let ns = self.stamp_ns;
        nros::Time {
            sec: ns.div_euclid(1_000_000_000) as i32,
            nanosec: ns.rem_euclid(1_000_000_000) as u32,
        }
    }

    fn to_ns(self) -> (i64, i64) {
        let at = i64::try_from(self.at.as_nanos()).unwrap_or(i64::MAX);
        (at, self.stamp_ns)
    }

    fn from_ns(at_ns: i64, stamp_ns: i64) -> Self {
        Self {
            at: Duration::from_nanos(at_ns.max(0) as u64),
            stamp_ns,
        }
    }
}

fn empty_result() -> sys::nros_device_result_t {
    sys::nros_device_result_t {
        output: sys::nros_device_buffer_t {
            data: core::ptr::null_mut(),
            size: 0,
        },
        length: 0,
        captured_at_ns: 0,
        captured_stamp_ns: 0,
        status: sys::NROS_DEVICE_RET_OK,
        _reserved: 0,
    }
}

/// A finished job's result, handed to a stage's callback.
#[derive(Clone, Copy)]
pub struct StageResult {
    raw: sys::nros_device_result_t,
}

impl StageResult {
    /// The job's outcome.
    pub fn status(&self) -> Result<(), DeviceError> {
        DeviceError::check(self.raw.status)
    }

    /// The buffer the result is in.
    pub fn output(&self) -> sys::nros_device_buffer_t {
        self.raw.output
    }

    /// The bytes of the output the device wrote.
    pub fn length(&self) -> usize {
        self.raw.length
    }

    /// When the job's input was captured.
    pub fn capture(&self) -> Capture {
        Capture::from_ns(self.raw.captured_at_ns, self.raw.captured_stamp_ns)
    }

    /// The input's wall-clock capture time, for a message header.
    pub fn stamp(&self) -> nros::Time {
        self.capture().stamp()
    }

    /// The result as the port ABI carries it.
    pub fn raw(&self) -> &sys::nros_device_result_t {
        &self.raw
    }
}

/// One item a source produced, handed to its callback.
#[derive(Clone, Copy)]
pub struct Item {
    raw: sys::nros_device_result_t,
}

impl Item {
    /// The item's outcome.
    pub fn status(&self) -> Result<(), DeviceError> {
        DeviceError::check(self.raw.status)
    }

    /// The buffer the item is in.
    pub fn output(&self) -> sys::nros_device_buffer_t {
        self.raw.output
    }

    /// The bytes of the buffer the item holds.
    pub fn length(&self) -> usize {
        self.raw.length
    }

    /// When the source captured it.
    pub fn capture(&self) -> Capture {
        Capture::from_ns(self.raw.captured_at_ns, self.raw.captured_stamp_ns)
    }

    /// The item as the port ABI carries it.
    pub fn raw(&self) -> &sys::nros_device_result_t {
        &self.raw
    }
}

/// A worker device, opened for an executor's stages. A device has one owner:
/// opening it a second time is refused.
#[derive(Clone, Copy)]
pub struct Worker<C: JobClass> {
    handle: &'static WorkerHandle<C>,
}

impl<C: JobClass> Worker<C> {
    /// Open `device` for stages on `executor`.
    pub fn open(
        executor: &mut Executor<'_>,
        device: &'static WorkerHandle<C>,
    ) -> Result<Self, DeviceError> {
        // The executor is the owner the stages register on; the device is
        // claimed once, whichever executor that is.
        let _ = executor;
        device.check()?;
        device.claim()?;
        Ok(Self { handle: device })
    }
}

/// A stage: jobs go to a worker, and each result runs a callback on the
/// executor at the priority of the stage's sched context.
///
/// One job runs at a time on a worker; a submit while it is busy returns
/// [`DeviceError::Busy`].
#[derive(Clone, Copy)]
pub struct Stage<C: JobClass> {
    worker: &'static WorkerHandle<C>,
    done: Completion,
    handle: HandleId,
}

impl<C: JobClass> Stage<C> {
    /// Register a stage on `executor`, before its first spin. `on_result` runs
    /// with each finished job's result.
    pub fn open<F>(
        executor: &mut Executor<'_>,
        worker: Worker<C>,
        mut on_result: F,
    ) -> Result<Self, DeviceError>
    where
        F: FnMut(StageResult) + 'static,
    {
        let device = worker.handle;
        let (handle, done) = executor
            .register_device(move || {
                let ops = device.ops();
                let mut raw = empty_result();
                let Some(take) = ops.take else { return };
                // SAFETY: the table's own operation, on the executor that
                // owns the device; `raw` is valid for the call.
                let ret = unsafe { take(ops.user_data, &mut raw) };
                if ret == sys::NROS_DEVICE_RET_PENDING {
                    // Signalled with nothing to take: a spurious completion.
                    return;
                }
                if ret != sys::NROS_DEVICE_RET_OK {
                    raw.status = ret;
                }
                // SAFETY: written by `submit` on this same thread.
                let (at, stamp) = unsafe { *device.capture.get() };
                raw.captured_at_ns = at;
                raw.captured_stamp_ns = stamp;
                device.busy.store(false, portable_atomic::Ordering::Release);
                on_result(StageResult { raw });
            })
            .map_err(|_| DeviceError::Registration)?;
        // The Rust registration route's census entry; the C route counts in
        // `nros_executor_register_device` (issue 2984).
        nros::census_hooks::on_device_create();
        Ok(Self {
            worker: device,
            done,
            handle,
        })
    }

    /// Submit `job` to the worker, with its input's capture time. Returns at
    /// once; the result arrives at the stage's callback. The job's buffers
    /// must stay valid until then.
    pub fn submit(&self, job: &C::Job, capture: Capture) -> Result<(), DeviceError> {
        let device = self.worker;
        if device.busy.swap(true, portable_atomic::Ordering::AcqRel) {
            return Err(DeviceError::Busy);
        }
        // SAFETY: the owning executor's thread; the continuation reads it
        // only after the device signals.
        unsafe { *device.capture.get() = capture.to_ns() };
        let ops = device.ops();
        let raw = C::to_raw(job);
        let done = sys::nros_completion_t {
            _slot: self.done.as_raw(),
        };
        let ret = match ops.submit {
            // SAFETY: the table's own operation; `raw` is valid for the call.
            Some(submit) => unsafe { submit(ops.user_data, &raw, done) },
            None => sys::NROS_DEVICE_RET_ABI_MISMATCH,
        };
        if let Err(e) = DeviceError::check(ret) {
            device.busy.store(false, portable_atomic::Ordering::Release);
            return Err(e);
        }
        Ok(())
    }

    /// The stage's slot, for binding it to a sched context.
    pub fn handle(&self) -> HandleId {
        self.handle
    }
}

/// A source: each item the device produces runs a callback on the executor.
#[derive(Clone, Copy)]
pub struct Source<C: SourceClass> {
    device: &'static SourceHandle<C>,
    handle: HandleId,
}

/// The most items one completion delivers, so a port that never reports
/// `PENDING` cannot hold the executor.
const MAX_ITEMS_PER_COMPLETION: usize = 64;

impl<C: SourceClass> Source<C> {
    /// Register `device` on `executor`, before its first spin, and start it.
    /// `on_item` runs with each item it produces.
    pub fn open<F>(
        executor: &mut Executor<'_>,
        device: &'static SourceHandle<C>,
        mut on_item: F,
    ) -> Result<Self, DeviceError>
    where
        F: FnMut(Item) + 'static,
    {
        device.check()?;
        device.claim()?;
        let (handle, ready) = executor
            .register_device(move || {
                let ops = device.ops();
                let Some(take) = ops.take else { return };
                for _ in 0..MAX_ITEMS_PER_COMPLETION {
                    let mut raw = empty_result();
                    // SAFETY: as in `Stage::open`.
                    let ret = unsafe { take(ops.user_data, &mut raw) };
                    if ret == sys::NROS_DEVICE_RET_PENDING {
                        break;
                    }
                    if ret != sys::NROS_DEVICE_RET_OK {
                        raw.status = ret;
                    }
                    on_item(Item { raw });
                }
            })
            .map_err(|_| DeviceError::Registration)?;
        // Issue 2984 -- the Rust route's census entry, as for a stage.
        nros::census_hooks::on_device_create();
        let ops = device.ops();
        let ready = sys::nros_completion_t {
            _slot: ready.as_raw(),
        };
        let ret = match ops.start {
            // SAFETY: the table's own operation.
            Some(start) => unsafe { start(ops.user_data, ready) },
            None => sys::NROS_DEVICE_RET_ABI_MISMATCH,
        };
        DeviceError::check(ret)?;
        Ok(Self { device, handle })
    }

    /// Stop the device producing.
    pub fn stop(&self) {
        let ops = self.device.ops();
        if let Some(stop) = ops.stop {
            // SAFETY: the table's own operation.
            unsafe { stop(ops.user_data) };
        }
    }

    /// The source's slot, for binding it to a sched context.
    pub fn handle(&self) -> HandleId {
        self.handle
    }
}

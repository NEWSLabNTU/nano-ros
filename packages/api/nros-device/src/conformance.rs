// RFC-0139 D11: the companion package's conformance tests, run against any
// port, real or mock, in any language. A board package runs them in its own
// tests against its device handle.
//! The rules every port keeps, checked against a device handle.
//!
//! Each check drives the port through its operation table, the same path a
//! node takes, so a port written in C is checked exactly as one written in
//! Rust. A check registers one probe slot on the executor to get a completion
//! handle; run it on an executor and a handle used for nothing else.
//!
//! `drive` is called while a check waits for the device: a test passes what
//! makes its device finish (a mock's `finish`, a loop that steps a simulator),
//! or `|| {}` for a device that completes on its own.
//!
//! What a check cannot see: that `submit` never blocks (a blocking `submit`
//! still returns), and that a real interrupt handler calls only the
//! interrupt-safe signal. Both are the port's to keep.

use nros_node::executor::Executor;

use crate::{
    DeviceError,
    handle::{SourceHandle, WorkerHandle},
    port::{JobClass, SourceClass},
    sys,
};

/// The rule a port broke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConformanceFailure {
    /// The operation table is another ABI version, or lacks an operation every
    /// port must have.
    Abi,
    /// The executor refused the probe slot.
    Registration,
    /// `take` returned a result before anything was submitted (or produced).
    ResultBeforeWork,
    /// `submit` (or `start`) failed.
    Refused(DeviceError),
    /// `take` returned a result before the port signalled completion.
    ResultBeforeCompletion,
    /// The port never signalled completion while `drive` ran.
    NeverSignalled,
    /// After completion, `take` returned no result.
    NoResultAfterCompletion,
    /// `take` returned a second result for one job.
    SecondResult,
}

/// How many times a check calls `drive` before deciding the port never
/// signals.
pub const DRIVE_ATTEMPTS: usize = 1000;

fn empty() -> sys::nros_device_result_t {
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

/// Check a worker: nothing to take before a job, a job submitted, nothing to
/// take before completion, completion signalled, one result after it.
pub fn worker<C: JobClass>(
    executor: &mut Executor<'_>,
    device: &'static WorkerHandle<C>,
    job: &C::Job,
    mut drive: impl FnMut(),
) -> Result<(), ConformanceFailure> {
    device.check().map_err(|_| ConformanceFailure::Abi)?;
    let ops = device.ops();
    let (Some(submit), Some(take)) = (ops.submit, ops.take) else {
        return Err(ConformanceFailure::Abi);
    };
    let (_slot, done) = executor
        .register_device(|| {})
        .map_err(|_| ConformanceFailure::Registration)?;
    let mut out = empty();

    // SAFETY (each call below): the table's own operations, called one at a
    // time from this thread, with arguments valid for the call.
    if unsafe { take(ops.user_data, &mut out) } != sys::NROS_DEVICE_RET_PENDING {
        return Err(ConformanceFailure::ResultBeforeWork);
    }
    let raw = C::to_raw(job);
    let handle = sys::nros_completion_t {
        _slot: done.as_raw(),
    };
    if let Err(e) = DeviceError::check(unsafe { submit(ops.user_data, &raw, handle) }) {
        return Err(ConformanceFailure::Refused(e));
    }
    if !done.is_signalled()
        && unsafe { take(ops.user_data, &mut out) } != sys::NROS_DEVICE_RET_PENDING
    {
        return Err(ConformanceFailure::ResultBeforeCompletion);
    }
    wait(done, &mut drive)?;
    if unsafe { take(ops.user_data, &mut out) } != sys::NROS_DEVICE_RET_OK {
        return Err(ConformanceFailure::NoResultAfterCompletion);
    }
    if unsafe { take(ops.user_data, &mut out) } != sys::NROS_DEVICE_RET_PENDING {
        return Err(ConformanceFailure::SecondResult);
    }
    Ok(())
}

/// Check a source: it starts, has nothing to take before it produces,
/// signals when it does, and yields the item. It is stopped at the end.
pub fn source<C: SourceClass>(
    executor: &mut Executor<'_>,
    device: &'static SourceHandle<C>,
    mut drive: impl FnMut(),
) -> Result<(), ConformanceFailure> {
    device.check().map_err(|_| ConformanceFailure::Abi)?;
    let ops = device.ops();
    let (Some(start), Some(take), Some(stop)) = (ops.start, ops.take, ops.stop) else {
        return Err(ConformanceFailure::Abi);
    };
    let (_slot, ready) = executor
        .register_device(|| {})
        .map_err(|_| ConformanceFailure::Registration)?;
    let mut out = empty();
    let handle = sys::nros_completion_t {
        _slot: ready.as_raw(),
    };

    // SAFETY (each call below): as in `worker`.
    if let Err(e) = DeviceError::check(unsafe { start(ops.user_data, handle) }) {
        return Err(ConformanceFailure::Refused(e));
    }
    let result = (|| {
        if !ready.is_signalled()
            && unsafe { take(ops.user_data, &mut out) } != sys::NROS_DEVICE_RET_PENDING
        {
            return Err(ConformanceFailure::ResultBeforeCompletion);
        }
        wait(ready, &mut drive)?;
        if unsafe { take(ops.user_data, &mut out) } != sys::NROS_DEVICE_RET_OK {
            return Err(ConformanceFailure::NoResultAfterCompletion);
        }
        Ok(())
    })();
    unsafe { stop(ops.user_data) };
    result
}

fn wait(done: crate::Completion, drive: &mut impl FnMut()) -> Result<(), ConformanceFailure> {
    for _ in 0..DRIVE_ATTEMPTS {
        if done.is_signalled() {
            return Ok(());
        }
        drive();
    }
    if done.is_signalled() {
        Ok(())
    } else {
        Err(ConformanceFailure::NeverSignalled)
    }
}

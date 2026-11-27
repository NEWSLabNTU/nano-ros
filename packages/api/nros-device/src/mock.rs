// RFC-0139 D11: a mock worker and a mock source, so a node's stages are tested
// with no device at all. The same node binary runs against a mock or a real
// port; which one is the board's choice.
//! A worker and a source that need no hardware.
//!
//! [`MockWorker`] copies a job's input to its output. It completes either at
//! once (inside `submit`) or when a test says so ([`MockControl::finish`],
//! which an interrupt handler may call as
//! [`finish_from_isr`](MockControl::finish_from_isr)), and it can be told to
//! fail the next job.
//!
//! [`MockSource`] delivers the items a test pushes with
//! [`MockSourceControl::push`], each stamped with the capture time given.
//!
//! The control blocks live in `static`s beside the ports, so a test (or a
//! timer interrupt) drives the device without touching the port's state.

use core::{cell::UnsafeCell, task::Poll};

use portable_atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicUsize, Ordering};

use crate::{
    Completion, CompletionCell, DeviceError,
    port::{Raw, ResultSink, SourcePort, WorkerPort},
    sys,
};

/// How a [`MockWorker`] completes a job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MockMode {
    /// Inside `submit`, before it returns.
    Immediate,
    /// When [`MockControl::finish`] is called.
    Manual,
}

/// The test's side of a [`MockWorker`].
pub struct MockControl {
    mode: MockMode,
    done: CompletionCell,
    finished: AtomicBool,
    fault: AtomicI32,
    submitted: AtomicU32,
}

impl MockControl {
    /// A control block for a worker that completes in `mode`.
    pub const fn new(mode: MockMode) -> Self {
        Self {
            mode,
            done: CompletionCell::new(),
            finished: AtomicBool::new(false),
            fault: AtomicI32::new(sys::NROS_DEVICE_RET_OK),
            submitted: AtomicU32::new(0),
        }
    }

    /// Finish the job in flight from a thread. `false` when no job was ever
    /// submitted.
    pub fn finish(&self) -> bool {
        self.finished.store(true, Ordering::Release);
        self.done.signal()
    }

    /// Finish the job in flight from an interrupt handler.
    pub fn finish_from_isr(&self) -> bool {
        self.finished.store(true, Ordering::Release);
        self.done.signal_from_isr()
    }

    /// Fail the next job's result with `error`.
    pub fn fail_next(&self, error: DeviceError) {
        self.fault.store(error.to_ret(), Ordering::Release);
    }

    /// How many jobs were submitted.
    pub fn submitted(&self) -> u32 {
        self.submitted.load(Ordering::Acquire)
    }
}

/// A worker that copies each job's input to its output.
pub struct MockWorker {
    control: &'static MockControl,
    job: Option<sys::nros_device_job_t>,
}

// SAFETY: the job's raw buffer pointers are only dereferenced on the owning
// executor's thread, inside `take`, as the port contract requires.
unsafe impl Send for MockWorker {}

impl MockWorker {
    /// A worker driven by `control`.
    pub const fn new(control: &'static MockControl) -> Self {
        Self { control, job: None }
    }
}

impl WorkerPort for MockWorker {
    type Class = Raw;

    fn submit(
        &mut self,
        job: &sys::nros_device_job_t,
        done: Completion,
    ) -> Result<(), DeviceError> {
        self.job = Some(*job);
        self.control.done.set(done);
        self.control.finished.store(false, Ordering::Release);
        self.control.submitted.fetch_add(1, Ordering::AcqRel);
        if self.control.mode == MockMode::Immediate {
            self.control.finish();
        }
        Ok(())
    }

    fn take(&mut self, out: &mut ResultSink<'_>) -> Poll<Result<(), DeviceError>> {
        if !self.control.finished.swap(false, Ordering::AcqRel) {
            return Poll::Pending;
        }
        let Some(job) = self.job.take() else {
            return Poll::Pending;
        };
        let n = job.input.size.min(job.output.size);
        if n > 0 && !job.input.data.is_null() && !job.output.data.is_null() {
            // SAFETY: the job's buffers are valid until its result is taken
            // (the submitter's contract), and `n` fits both.
            unsafe {
                core::ptr::copy_nonoverlapping(
                    job.input.data as *const u8,
                    job.output.data as *mut u8,
                    n,
                )
            };
        }
        out.set_output(job.output);
        out.set_length(n);
        match DeviceError::from_ret(
            self.control
                .fault
                .swap(sys::NROS_DEVICE_RET_OK, Ordering::AcqRel),
        ) {
            None => Poll::Ready(Ok(())),
            Some(e) => Poll::Ready(Err(e)),
        }
    }
}

/// How many pushed items a [`MockSourceControl`] holds before `push` refuses.
pub const MOCK_SOURCE_DEPTH: usize = 8;

#[derive(Clone, Copy)]
struct MockItem {
    data: *const u8,
    len: usize,
    at_ns: i64,
    stamp_ns: i64,
}

/// The test's side of a [`MockSource`]: a single-producer, single-consumer
/// ring of items. One producer (a test thread, or an interrupt handler) pushes;
/// the source takes on the executor.
pub struct MockSourceControl {
    ready: CompletionCell,
    started: AtomicBool,
    head: AtomicUsize,
    tail: AtomicUsize,
    items: [UnsafeCell<MockItem>; MOCK_SOURCE_DEPTH],
}

// SAFETY: a slot is written only by the one producer before `tail` publishes
// it, and read only by the one consumer before `head` releases it.
unsafe impl Sync for MockSourceControl {}

const EMPTY_ITEM: MockItem = MockItem {
    data: core::ptr::null(),
    len: 0,
    at_ns: 0,
    stamp_ns: 0,
};

impl MockSourceControl {
    /// An empty control block.
    pub const fn new() -> Self {
        Self {
            ready: CompletionCell::new(),
            started: AtomicBool::new(false),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            items: [const { UnsafeCell::new(EMPTY_ITEM) }; MOCK_SOURCE_DEPTH],
        }
    }

    /// Deliver `data`, captured at monotonic `at_ns` with wall clock
    /// `stamp_ns`. `false` when the ring is full or the source is stopped.
    pub fn push(&self, data: &'static [u8], at_ns: i64, stamp_ns: i64) -> bool {
        if !self.started.load(Ordering::Acquire) {
            return false;
        }
        let tail = self.tail.load(Ordering::Relaxed);
        if tail.wrapping_sub(self.head.load(Ordering::Acquire)) >= MOCK_SOURCE_DEPTH {
            return false;
        }
        // SAFETY: the one producer owns slot `tail` until it publishes it.
        unsafe {
            *self.items[tail % MOCK_SOURCE_DEPTH].get() = MockItem {
                data: data.as_ptr(),
                len: data.len(),
                at_ns,
                stamp_ns,
            }
        };
        self.tail.store(tail.wrapping_add(1), Ordering::Release);
        self.ready.signal();
        true
    }

    fn pop(&self) -> Option<MockItem> {
        let head = self.head.load(Ordering::Relaxed);
        if head == self.tail.load(Ordering::Acquire) {
            return None;
        }
        // SAFETY: the producer published slot `head`; the one consumer reads
        // it before releasing it.
        let item = unsafe { *self.items[head % MOCK_SOURCE_DEPTH].get() };
        self.head.store(head.wrapping_add(1), Ordering::Release);
        Some(item)
    }
}

impl Default for MockSourceControl {
    fn default() -> Self {
        Self::new()
    }
}

/// A source that delivers the items its control block is given.
pub struct MockSource {
    control: &'static MockSourceControl,
}

impl MockSource {
    /// A source fed by `control`.
    pub const fn new(control: &'static MockSourceControl) -> Self {
        Self { control }
    }
}

impl SourcePort for MockSource {
    type Class = Raw;

    fn start(&mut self, ready: Completion) -> Result<(), DeviceError> {
        self.control.ready.set(ready);
        self.control.started.store(true, Ordering::Release);
        Ok(())
    }

    fn take(&mut self, out: &mut ResultSink<'_>) -> Poll<Result<(), DeviceError>> {
        let Some(item) = self.control.pop() else {
            return Poll::Pending;
        };
        out.set_output(sys::nros_device_buffer_t {
            data: item.data as *mut core::ffi::c_void,
            size: item.len,
        });
        out.set_length(item.len);
        out.set_capture(item.at_ns, item.stamp_ns);
        Poll::Ready(Ok(()))
    }

    fn stop(&mut self) {
        self.control.started.store(false, Ordering::Release);
    }
}

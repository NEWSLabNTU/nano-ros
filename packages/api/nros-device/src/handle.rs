// RFC-0139 R5: the typed device handle a board exports, one `static` per
// device (ruling Q3: `<board>::devices::NPU0`).

use core::{cell::UnsafeCell, marker::PhantomData};

use portable_atomic::{AtomicBool, Ordering};

use crate::{
    DeviceError,
    port::{JobClass, PortCell, SourceClass, SourcePort, SourcePortOps, WorkerPort, WorkerPortOps},
    sys,
};

const ABI_V1: u32 = sys::NROS_DEVICE_PORT_ABI_V1 as u32;

/// A worker device, as a board exports it: one `static` per device.
///
/// ```ignore
/// static NPU: PortCell<AcmeNpu> = PortCell::new(AcmeNpu::new());
/// pub static NPU0: WorkerHandle<Raw> = WorkerHandle::new(&NPU);
/// ```
///
/// The class is a type parameter, so a stage of another class cannot be bound
/// to the device: that is a compile error, not a refusal at run time.
pub struct WorkerHandle<C: JobClass> {
    inline: WorkerPortOps,
    external: *const sys::nros_worker_port_ops_t,
    claimed: AtomicBool,
    pub(crate) busy: AtomicBool,
    /// The capture time of the job in flight: monotonic ns, wall-clock ns.
    /// Written by `submit` and read by the continuation, both on the thread
    /// of the one executor that claimed the device.
    pub(crate) capture: UnsafeCell<(i64, i64)>,
    _class: PhantomData<fn() -> C>,
}

// SAFETY: the operation table is immutable; `claimed` and `busy` are atomic;
// `capture` is touched only on the thread of the executor that claimed the
// device (a device has one owner, enforced by `claim`).
unsafe impl<C: JobClass> Sync for WorkerHandle<C> {}
// SAFETY: as `Sync`.
unsafe impl<C: JobClass> Send for WorkerHandle<C> {}

impl<C: JobClass> WorkerHandle<C> {
    /// The handle of a port written in Rust.
    pub const fn new<P: WorkerPort<Class = C>>(cell: &'static PortCell<P>) -> Self {
        Self::build(WorkerPortOps::of(cell), core::ptr::null())
    }

    /// The handle of a port written in C, from its exported operation table.
    ///
    /// ```ignore
    /// unsafe extern "C" { static acme_npu0_ops: nros_device::sys::nros_worker_port_ops_t; }
    /// pub static NPU0: WorkerHandle<Raw> = unsafe { WorkerHandle::from_c(&acme_npu0_ops) };
    /// ```
    ///
    /// # Safety
    /// The table must implement a port of class `C`: its `submit` receives jobs
    /// built by `C::to_raw`.
    pub const unsafe fn from_c(ops: &'static sys::nros_worker_port_ops_t) -> Self {
        Self::build(WorkerPortOps::empty(), ops)
    }

    const fn build(inline: WorkerPortOps, external: *const sys::nros_worker_port_ops_t) -> Self {
        Self {
            inline,
            external,
            claimed: AtomicBool::new(false),
            busy: AtomicBool::new(false),
            capture: UnsafeCell::new((0, 0)),
            _class: PhantomData,
        }
    }

    pub(crate) fn ops(&self) -> &sys::nros_worker_port_ops_t {
        if self.external.is_null() {
            self.inline.raw()
        } else {
            // SAFETY: `from_c` took a `&'static` table.
            unsafe { &*self.external }
        }
    }

    /// Check the table speaks this ABI and has the operations every worker
    /// must have.
    pub(crate) fn check(&self) -> Result<(), DeviceError> {
        let ops = self.ops();
        if ops.abi_version != ABI_V1 || ops.submit.is_none() || ops.take.is_none() {
            return Err(DeviceError::AbiMismatch);
        }
        Ok(())
    }

    pub(crate) fn claim(&self) -> Result<(), DeviceError> {
        if self.claimed.swap(true, Ordering::AcqRel) {
            return Err(DeviceError::AlreadyClaimed);
        }
        Ok(())
    }
}

/// A source device, as a board exports it: one `static` per device, built as
/// [`WorkerHandle`] is.
pub struct SourceHandle<C: SourceClass> {
    inline: SourcePortOps,
    external: *const sys::nros_source_port_ops_t,
    claimed: AtomicBool,
    _class: PhantomData<fn() -> C>,
}

// SAFETY: the table is immutable and `claimed` is atomic.
unsafe impl<C: SourceClass> Sync for SourceHandle<C> {}
// SAFETY: as `Sync`.
unsafe impl<C: SourceClass> Send for SourceHandle<C> {}

impl<C: SourceClass> SourceHandle<C> {
    /// The handle of a port written in Rust.
    pub const fn new<P: SourcePort<Class = C>>(cell: &'static PortCell<P>) -> Self {
        Self {
            inline: SourcePortOps::of(cell),
            external: core::ptr::null(),
            claimed: AtomicBool::new(false),
            _class: PhantomData,
        }
    }

    /// The handle of a port written in C, from its exported operation table.
    ///
    /// # Safety
    /// The table must implement a source of class `C`.
    pub const unsafe fn from_c(ops: &'static sys::nros_source_port_ops_t) -> Self {
        Self {
            inline: SourcePortOps::empty(),
            external: ops,
            claimed: AtomicBool::new(false),
            _class: PhantomData,
        }
    }

    pub(crate) fn ops(&self) -> &sys::nros_source_port_ops_t {
        if self.external.is_null() {
            self.inline.raw()
        } else {
            // SAFETY: `from_c` took a `&'static` table.
            unsafe { &*self.external }
        }
    }

    pub(crate) fn check(&self) -> Result<(), DeviceError> {
        let ops = self.ops();
        if ops.abi_version != ABI_V1
            || ops.start.is_none()
            || ops.take.is_none()
            || ops.stop.is_none()
        {
            return Err(DeviceError::AbiMismatch);
        }
        Ok(())
    }

    pub(crate) fn claim(&self) -> Result<(), DeviceError> {
        if self.claimed.swap(true, Ordering::AcqRel) {
            return Err(DeviceError::AlreadyClaimed);
        }
        Ok(())
    }
}

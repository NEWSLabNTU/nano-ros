// Phase 579 W4a-1; RFC-0139 D7, D11 and the Rust-shape amendment (R1-R7).
//! Device ports for nano-ros: accelerators, cameras and other devices that do
//! work on their own and report completion, as executor event sources.
//!
//! Two roles:
//!
//! - a **worker** takes a job and completes it (an NPU, a codec, a 2D engine);
//! - a **source** produces data on its own clock (a camera, an IMU FIFO).
//!
//! A device's completion becomes an executor event: the continuation runs as
//! an ordinary callback at the priority of the sched context it is bound to,
//! and the executor is never held while the device works.
//!
//! # Two ends
//!
//! - **Implementing a device** (a board package): implement [`WorkerPort`] or
//!   [`SourcePort`], keep its state in a [`PortCell`], and export a
//!   [`WorkerHandle`] or [`SourceHandle`] built from it. The same port can also
//!   be exported to C and C++ nodes as a [`WorkerPortOps`] / [`SourcePortOps`]
//!   table. A device whose port is written in C is named from Rust with
//!   [`WorkerHandle::from_c`] / [`SourceHandle::from_c`].
//! - **Using a device** (a node): open a [`Worker`] on an executor, then a
//!   [`Stage`] that submits jobs to it and receives their results, or a
//!   [`Source`] that delivers each item the device produces.
//!
//! # What this crate keeps
//!
//! It is `#![no_std]` and needs no allocator. A port's state is a `static`, a
//! device's callback lives in the executor's arena, and every registration
//! happens before the first spin. The port ABI's types are the committed
//! bindings of `<nros/device_port.h>`, re-exported as [`sys`].
//!
//! [`mock`] has a worker and a source that need no hardware, and
//! [`conformance`] checks the rules every port keeps, against any handle.
#![no_std]

pub mod conformance;
mod error;
mod handle;
pub mod mock;
mod port;
mod runtime;

pub use error::DeviceError;
pub use handle::{SourceHandle, WorkerHandle};
pub use nros_node::executor::{Completion, CompletionCell};
pub use port::{
    JobClass, PortCell, Raw, ResultSink, SourceClass, SourcePort, SourcePortOps, WorkerPort,
    WorkerPortOps,
};
pub use runtime::{Capture, Item, Source, Stage, StageResult, Worker};

/// The port ABI of `<nros/device_port.h>`: the generated bindings, never a
/// hand-written mirror.
pub mod sys {
    pub use nros_rmw_cffi::generated::{
        NROS_DEVICE_PORT_ABI_V1, NROS_DEVICE_RET_ABI_MISMATCH, NROS_DEVICE_RET_FAULT,
        NROS_DEVICE_RET_OK, NROS_DEVICE_RET_PENDING, NROS_DEVICE_RET_REJECTED,
        NROS_DEVICE_RET_TIMEOUT, NROS_DEVICE_RET_UNSUPPORTED, nros_completion_t,
        nros_device_buffer_t, nros_device_job_t, nros_device_result_t, nros_device_ret_t,
        nros_source_port_ops_t, nros_worker_port_ops_t,
    };
}

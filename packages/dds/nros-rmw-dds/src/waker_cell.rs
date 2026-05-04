//! Shared waker storage for DDS readers.
//!
//! Items here are referenced only from `feature = "std"` /
//! `feature = "nostd-runtime"` paths in `session.rs` / `subscriber.rs`,
//! so they appear dead in the bare `--no-default-features` build.

#![allow(dead_code)]
//!
//! `nros-rmw`'s async `Subscriber` / `ServiceClient` futures call
//! `register_waker(cx.waker())` whenever they yield `Pending`, expecting
//! the backend to invoke `waker.wake()` once data has arrived. dust-dds
//! exposes the data-arrival event via a `DataReaderListener` callback;
//! this module bridges the two by storing the most recently registered
//! waker in a shared cell that the listener wakes from
//! `on_data_available`.

use crate::sync::Arc;
use core::{ffi::c_void, future::Future, task::Waker};
use spin::Mutex;

use dust_dds::dds_async::{data_reader::DataReaderAsync, data_reader_listener::DataReaderListener};

use crate::raw_type::RawCdrPayload;

/// Single-slot waker holder. Cheap to clone (one `Arc`).
#[derive(Default)]
pub(crate) struct WakerCell {
    inner: Mutex<Option<Waker>>,
}

impl WakerCell {
    /// Replace the stored waker (or keep the existing one if it would
    /// wake the same task — the standard `AtomicWaker`-style policy).
    pub fn register(&self, waker: &Waker) {
        let mut guard = self.inner.lock();
        match guard.as_ref() {
            Some(existing) if existing.will_wake(waker) => {}
            _ => *guard = Some(waker.clone()),
        }
    }

    /// Wake and clear the stored waker, if any. Subsequent registers
    /// install a fresh waker.
    pub fn wake(&self) {
        if let Some(w) = self.inner.lock().take() {
            w.wake();
        }
    }
}

/// One Phase-108 status-event-callback registration. Stored under the
/// `EventSlot` for an entity; populated by `register_event_callback`,
/// invoked by the dust-dds listener bridge on event fire.
#[derive(Clone, Copy)]
pub(crate) struct EventReg {
    pub cb: nros_rmw::EventCallback,
    pub user_ctx: *mut c_void,
}

// SAFETY: dust-dds listener tasks are spawned on a runtime that may
// move them across threads; the cb fn pointer is `unsafe extern "C"`
// (always Send) and user_ctx is owned by the entity's Box and stays
// valid for the entity's lifetime (entity Drop frees, see Phase 108.A.7).
unsafe impl Send for EventReg {}
unsafe impl Sync for EventReg {}

#[derive(Default)]
pub(crate) struct EventSlot(Mutex<Option<EventReg>>);

impl EventSlot {
    pub fn set(&self, reg: EventReg) {
        *self.0.lock() = Some(reg);
    }
    pub fn fire(
        &self,
        kind: nros_rmw::EventKind,
        payload_ptr: *const c_void,
    ) {
        // Snapshot under the lock to avoid holding it across the cb.
        let reg = *self.0.lock();
        if let Some(reg) = reg {
            // SAFETY: cb is `unsafe extern "C" fn` matching the
            // EventCallback signature; user_ctx outlives this call
            // (entity owns the Box backing it).
            unsafe { (reg.cb)(kind, payload_ptr, reg.user_ctx) };
        }
    }
}

/// Shared state between a dust-dds listener and the owning nros entity.
/// Holds the data-arrival waker (existing) plus per-event-kind callback
/// slots (Phase 108.A wiring).
#[derive(Default)]
pub(crate) struct SubscriberShared {
    pub waker_cell: WakerCell,
    pub liveliness: EventSlot,
    pub deadline: EventSlot,
    pub message_lost: EventSlot,
}

/// `DataReaderListener` impl that wakes a shared `WakerCell` when
/// `on_data_available` fires AND fires registered nros status-event
/// callbacks for liveliness / deadline / sample-lost.
pub(crate) struct DataAvailableListener {
    shared: Arc<SubscriberShared>,
}

impl DataAvailableListener {
    pub fn new(shared: Arc<SubscriberShared>) -> Self {
        Self { shared }
    }
}

impl DataReaderListener<RawCdrPayload> for DataAvailableListener {
    fn on_data_available(
        &mut self,
        _the_reader: DataReaderAsync<RawCdrPayload>,
    ) -> impl Future<Output = ()> + Send {
        let shared = self.shared.clone();
        async move {
            shared.waker_cell.wake();
        }
    }

    fn on_liveliness_changed(
        &mut self,
        _the_reader: DataReaderAsync<RawCdrPayload>,
        status: dust_dds::infrastructure::status::LivelinessChangedStatus,
    ) -> impl Future<Output = ()> + Send {
        let shared = self.shared.clone();
        async move {
            let nros_status = nros_rmw::LivelinessChangedStatus {
                alive_count: clamp_u16(status.alive_count),
                not_alive_count: clamp_u16(status.not_alive_count),
                alive_count_change: clamp_i16(status.alive_count_change),
                not_alive_count_change: clamp_i16(status.not_alive_count_change),
            };
            shared.liveliness.fire(
                nros_rmw::EventKind::LivelinessChanged,
                &nros_status as *const _ as *const c_void,
            );
        }
    }

    fn on_requested_deadline_missed(
        &mut self,
        _the_reader: DataReaderAsync<RawCdrPayload>,
        status: dust_dds::infrastructure::status::RequestedDeadlineMissedStatus,
    ) -> impl Future<Output = ()> + Send {
        let shared = self.shared.clone();
        async move {
            let nros_status = nros_rmw::CountStatus {
                total_count: status.total_count.max(0) as u32,
                total_count_change: status.total_count_change.max(0) as u32,
            };
            shared.deadline.fire(
                nros_rmw::EventKind::RequestedDeadlineMissed,
                &nros_status as *const _ as *const c_void,
            );
        }
    }

    fn on_sample_lost(
        &mut self,
        _the_reader: DataReaderAsync<RawCdrPayload>,
        status: dust_dds::infrastructure::status::SampleLostStatus,
    ) -> impl Future<Output = ()> + Send {
        let shared = self.shared.clone();
        async move {
            let nros_status = nros_rmw::CountStatus {
                total_count: status.total_count.max(0) as u32,
                total_count_change: status.total_count_change.max(0) as u32,
            };
            shared.message_lost.fire(
                nros_rmw::EventKind::MessageLost,
                &nros_status as *const _ as *const c_void,
            );
        }
    }
}

#[inline]
fn clamp_u16(v: i32) -> u16 {
    v.max(0).min(u16::MAX as i32) as u16
}

#[inline]
fn clamp_i16(v: i32) -> i16 {
    v.clamp(i16::MIN as i32, i16::MAX as i32) as i16
}

/// Phase 108.A.dds — pub-side listener shared state. Holds Tier-1
/// publisher event-callback slots for `LivelinessLost` and
/// `OfferedDeadlineMissed`.
#[derive(Default)]
pub(crate) struct PublisherShared {
    pub liveliness_lost: EventSlot,
    pub offered_deadline: EventSlot,
}

/// `DataWriterListener` impl that fires registered nros publisher
/// status-event callbacks when dust-dds detects the corresponding DDS
/// status.
pub(crate) struct PublisherEventListener {
    shared: Arc<PublisherShared>,
}

impl PublisherEventListener {
    pub fn new(shared: Arc<PublisherShared>) -> Self {
        Self { shared }
    }
}

impl dust_dds::dds_async::data_writer_listener::DataWriterListener<RawCdrPayload>
    for PublisherEventListener
{
    fn on_liveliness_lost(
        &mut self,
        _the_writer: dust_dds::dds_async::data_writer::DataWriterAsync<RawCdrPayload>,
        status: dust_dds::infrastructure::status::LivelinessLostStatus,
    ) -> impl Future<Output = ()> + Send {
        let shared = self.shared.clone();
        async move {
            let nros_status = nros_rmw::CountStatus {
                total_count: status.total_count.max(0) as u32,
                total_count_change: status.total_count_change.max(0) as u32,
            };
            shared.liveliness_lost.fire(
                nros_rmw::EventKind::LivelinessLost,
                &nros_status as *const _ as *const c_void,
            );
        }
    }

    fn on_offered_deadline_missed(
        &mut self,
        _the_writer: dust_dds::dds_async::data_writer::DataWriterAsync<RawCdrPayload>,
        status: dust_dds::infrastructure::status::OfferedDeadlineMissedStatus,
    ) -> impl Future<Output = ()> + Send {
        let shared = self.shared.clone();
        async move {
            let nros_status = nros_rmw::CountStatus {
                total_count: status.total_count.max(0) as u32,
                total_count_change: status.total_count_change.max(0) as u32,
            };
            shared.offered_deadline.fire(
                nros_rmw::EventKind::OfferedDeadlineMissed,
                &nros_status as *const _ as *const c_void,
            );
        }
    }
}

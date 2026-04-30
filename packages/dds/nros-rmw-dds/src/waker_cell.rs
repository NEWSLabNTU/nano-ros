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
use core::future::Future;
use core::task::Waker;
use spin::Mutex;

use dust_dds::dds_async::data_reader::DataReaderAsync;
use dust_dds::dds_async::data_reader_listener::DataReaderListener;

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

/// `DataReaderListener` impl that wakes a shared `WakerCell` when
/// `on_data_available` fires. Attached to every `DataReader` /
/// `DataReaderAsync` we hand out so the corresponding `DdsSubscriber` /
/// `DdsServiceClient` can be polled lazily without a busy loop.
pub(crate) struct DataAvailableListener {
    waker_cell: Arc<WakerCell>,
}

impl DataAvailableListener {
    pub fn new(waker_cell: Arc<WakerCell>) -> Self {
        Self { waker_cell }
    }
}

impl DataReaderListener<RawCdrPayload> for DataAvailableListener {
    fn on_data_available(
        &mut self,
        _the_reader: DataReaderAsync<RawCdrPayload>,
    ) -> impl Future<Output = ()> + Send {
        let cell = self.waker_cell.clone();
        async move {
            cell.wake();
        }
    }
}

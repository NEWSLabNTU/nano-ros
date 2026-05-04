//! DDS publisher — implements `nros_rmw::Publisher`.

use crate::sync::Arc;
use nros_rmw::{Publisher, TransportError};

use crate::waker_cell::{EventReg, PublisherShared};

#[cfg(all(feature = "nostd-runtime", not(feature = "std")))]
use crate::runtime::NrosPlatformRuntime;

/// DDS publisher backed by a dust-dds `DataWriter` (`std + posix`) or a
/// `DataWriterAsync` driven through `NrosPlatformRuntime::block_on()`
/// (every other platform).
pub struct DdsPublisher {
    #[cfg(feature = "std")]
    writer: dust_dds::publication::data_writer::DataWriter<crate::raw_type::RawCdrPayload>,
    #[cfg(all(feature = "nostd-runtime", not(feature = "std")))]
    writer_async: dust_dds::dds_async::data_writer::DataWriterAsync<crate::raw_type::RawCdrPayload>,
    #[cfg(all(feature = "nostd-runtime", not(feature = "std")))]
    runtime: Arc<NrosPlatformRuntime<nros_platform::ConcretePlatform>>,
    /// Phase 108.A.dds — publisher-side event-callback slots
    /// (LivelinessLost / OfferedDeadlineMissed). Shared with the
    /// `PublisherEventListener` attached to the underlying DataWriter.
    shared: Arc<PublisherShared>,
}

impl DdsPublisher {
    #[cfg(feature = "std")]
    pub(crate) fn new(
        writer: dust_dds::publication::data_writer::DataWriter<crate::raw_type::RawCdrPayload>,
        shared: Arc<PublisherShared>,
    ) -> Self {
        Self { writer, shared }
    }

    #[cfg(all(feature = "nostd-runtime", not(feature = "std")))]
    pub(crate) fn new_async(
        writer_async: dust_dds::dds_async::data_writer::DataWriterAsync<
            crate::raw_type::RawCdrPayload,
        >,
        runtime: Arc<NrosPlatformRuntime<nros_platform::ConcretePlatform>>,
        shared: Arc<PublisherShared>,
    ) -> Self {
        Self {
            writer_async,
            runtime,
            shared,
        }
    }
}

impl Publisher for DdsPublisher {
    type Error = TransportError;

    fn publish_raw(&self, data: &[u8]) -> Result<(), Self::Error> {
        #[cfg(feature = "std")]
        {
            use crate::raw_type::RawCdrPayload;
            let payload = RawCdrPayload {
                data: alloc::vec::Vec::from(data),
            };
            self.writer
                .write(payload, None)
                .map_err(|_| TransportError::PublishFailed)
        }

        #[cfg(all(feature = "nostd-runtime", not(feature = "std")))]
        {
            use crate::raw_type::RawCdrPayload;
            let payload = RawCdrPayload {
                data: alloc::vec::Vec::from(data),
            };
            self.runtime
                .block_on(self.writer_async.write(payload, None))
                .map_err(|_| TransportError::PublishFailed)
        }

        #[cfg(not(any(feature = "std", feature = "nostd-runtime")))]
        {
            let _ = data;
            Err(TransportError::PublishFailed)
        }
    }

    fn buffer_error(&self) -> Self::Error {
        TransportError::BufferTooSmall
    }

    fn serialization_error(&self) -> Self::Error {
        TransportError::SerializationError
    }

    fn assert_liveliness(&self) -> Result<(), Self::Error> {
        // Phase 108.B — manual liveliness assertion. dust-dds exposes
        // it natively on DataWriter (sync) and DataWriterAsync (no_std).
        // For AUTOMATIC liveliness this is unnecessary but cheap; we
        // forward unconditionally to keep the Publisher trait simple.
        #[cfg(feature = "std")]
        {
            self.writer
                .assert_liveliness()
                .map_err(|_| TransportError::Backend("dust-dds assert_liveliness failed"))
        }

        #[cfg(all(feature = "nostd-runtime", not(feature = "std")))]
        {
            self.runtime
                .block_on(self.writer_async.assert_liveliness())
                .map_err(|_| TransportError::Backend("dust-dds assert_liveliness failed"))
        }

        #[cfg(not(any(feature = "std", feature = "nostd-runtime")))]
        Err(TransportError::Unsupported)
    }

    fn supports_event(&self, kind: nros_rmw::EventKind) -> bool {
        // Phase 108.A.dds — Tier-1 pub-side events surfaced by
        // dust-dds DataWriterListener.
        matches!(
            kind,
            nros_rmw::EventKind::LivelinessLost | nros_rmw::EventKind::OfferedDeadlineMissed
        )
    }

    unsafe fn register_event_callback(
        &mut self,
        kind: nros_rmw::EventKind,
        _deadline_ms: u32,
        cb: nros_rmw::EventCallback,
        user_ctx: *mut core::ffi::c_void,
    ) -> Result<(), TransportError> {
        let slot = match kind {
            nros_rmw::EventKind::LivelinessLost => &self.shared.liveliness_lost,
            nros_rmw::EventKind::OfferedDeadlineMissed => &self.shared.offered_deadline,
            _ => return Err(TransportError::Unsupported),
        };
        slot.set(EventReg { cb, user_ctx });
        Ok(())
    }
}

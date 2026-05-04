//! DDS publisher — implements `nros_rmw::Publisher`.

use nros_rmw::{Publisher, TransportError};

#[cfg(all(feature = "nostd-runtime", not(feature = "std")))]
use crate::sync::Arc;

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
}

impl DdsPublisher {
    #[cfg(feature = "std")]
    pub(crate) fn new(
        writer: dust_dds::publication::data_writer::DataWriter<crate::raw_type::RawCdrPayload>,
    ) -> Self {
        Self { writer }
    }

    #[cfg(all(feature = "nostd-runtime", not(feature = "std")))]
    pub(crate) fn new_async(
        writer_async: dust_dds::dds_async::data_writer::DataWriterAsync<
            crate::raw_type::RawCdrPayload,
        >,
        runtime: Arc<NrosPlatformRuntime<nros_platform::ConcretePlatform>>,
    ) -> Self {
        Self {
            writer_async,
            runtime,
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
}

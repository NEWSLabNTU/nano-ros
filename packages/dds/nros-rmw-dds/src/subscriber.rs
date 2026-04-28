//! DDS subscriber — implements `nros_rmw::Subscriber`.

use alloc::sync::Arc;

use nros_rmw::{Subscriber, TransportError};

use crate::waker_cell::WakerCell;

#[cfg(all(feature = "nostd-runtime", not(feature = "std")))]
use crate::runtime::NrosPlatformRuntime;

/// DDS subscriber backed by a dust-dds `DataReader` (`std + posix`) or a
/// `DataReaderAsync` driven through `NrosPlatformRuntime::block_on()`
/// (every other platform).
pub struct DdsSubscriber {
    #[cfg(feature = "std")]
    reader: dust_dds::subscription::data_reader::DataReader<crate::raw_type::RawCdrPayload>,
    #[cfg(all(feature = "nostd-runtime", not(feature = "std")))]
    reader_async: dust_dds::dds_async::data_reader::DataReaderAsync<crate::raw_type::RawCdrPayload>,
    #[cfg(all(feature = "nostd-runtime", not(feature = "std")))]
    runtime: Arc<NrosPlatformRuntime<nros_platform::ConcretePlatform>>,
    /// Shared with the `DataAvailableListener` attached to the reader at
    /// construction time. The listener fires `on_data_available` from
    /// dust-dds's internal task pool whenever the reader's history queue
    /// gains a sample; this in turn wakes whatever future last called
    /// `register_waker`. Phase 71.29 follow-up.
    waker_cell: Arc<WakerCell>,
}

impl DdsSubscriber {
    #[cfg(feature = "std")]
    pub(crate) fn new(
        reader: dust_dds::subscription::data_reader::DataReader<crate::raw_type::RawCdrPayload>,
        waker_cell: Arc<WakerCell>,
    ) -> Self {
        Self { reader, waker_cell }
    }

    #[cfg(all(feature = "nostd-runtime", not(feature = "std")))]
    pub(crate) fn new_async(
        reader_async: dust_dds::dds_async::data_reader::DataReaderAsync<
            crate::raw_type::RawCdrPayload,
        >,
        runtime: Arc<NrosPlatformRuntime<nros_platform::ConcretePlatform>>,
        waker_cell: Arc<WakerCell>,
    ) -> Self {
        Self {
            reader_async,
            runtime,
            waker_cell,
        }
    }

}

impl Subscriber for DdsSubscriber {
    type Error = TransportError;

    fn try_recv_raw(&mut self, buf: &mut [u8]) -> Result<Option<usize>, Self::Error> {
        #[cfg(feature = "std")]
        {
            use dust_dds::infrastructure::sample_info::{
                ANY_INSTANCE_STATE, ANY_SAMPLE_STATE, ANY_VIEW_STATE,
            };
            return match self
                .reader
                .take(1, ANY_SAMPLE_STATE, ANY_VIEW_STATE, ANY_INSTANCE_STATE)
            {
                Ok(samples) => {
                    if let Some(sample) = samples.into_iter().next()
                        && let Some(payload) = sample.data
                    {
                        let len = payload.data.len();
                        if len > buf.len() {
                            return Err(TransportError::MessageTooLarge);
                        }
                        buf[..len].copy_from_slice(&payload.data);
                        return Ok(Some(len));
                    }
                    Ok(None)
                }
                Err(dust_dds::infrastructure::error::DdsError::NoData) => Ok(None),
                Err(_) => Err(TransportError::PollFailed),
            };
        }

        #[cfg(all(feature = "nostd-runtime", not(feature = "std")))]
        {
            use dust_dds::infrastructure::sample_info::{
                ANY_INSTANCE_STATE, ANY_SAMPLE_STATE, ANY_VIEW_STATE,
            };
            return match self.runtime.block_on(self.reader_async.take(
                1,
                ANY_SAMPLE_STATE,
                ANY_VIEW_STATE,
                ANY_INSTANCE_STATE,
            )) {
                Ok(samples) => {
                    if let Some(sample) = samples.into_iter().next()
                        && let Some(payload) = sample.data
                    {
                        let len = payload.data.len();
                        if len > buf.len() {
                            return Err(TransportError::MessageTooLarge);
                        }
                        buf[..len].copy_from_slice(&payload.data);
                        return Ok(Some(len));
                    }
                    Ok(None)
                }
                Err(dust_dds::infrastructure::error::DdsError::NoData) => Ok(None),
                Err(_) => Err(TransportError::PollFailed),
            };
        }

        #[cfg(not(any(feature = "std", feature = "nostd-runtime")))]
        {
            let _ = buf;
            Err(TransportError::PollFailed)
        }
    }

    fn has_data(&self) -> bool {
        true
    }

    fn register_waker(&self, waker: &core::task::Waker) {
        self.waker_cell.register(waker);
    }

    fn deserialization_error(&self) -> Self::Error {
        TransportError::DeserializationError
    }
}

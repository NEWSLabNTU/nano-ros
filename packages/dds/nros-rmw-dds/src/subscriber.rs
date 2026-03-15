//! DDS subscriber — implements `nros_rmw::Subscriber`.

use nros_rmw::{Subscriber, TransportError};

/// DDS subscriber backed by a dust-dds `DataReader`.
pub struct DdsSubscriber {
    #[cfg(feature = "std")]
    reader: dust_dds::subscription::data_reader::DataReader<crate::raw_type::RawCdrPayload>,
}

impl DdsSubscriber {
    #[cfg(feature = "std")]
    pub(crate) fn new(
        reader: dust_dds::subscription::data_reader::DataReader<crate::raw_type::RawCdrPayload>,
    ) -> Self {
        Self { reader }
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
            match self
                .reader
                .take(1, ANY_SAMPLE_STATE, ANY_VIEW_STATE, ANY_INSTANCE_STATE)
            {
                Ok(samples) => {
                    if let Some(sample) = samples.into_iter().next() {
                        if let Some(payload) = sample.data {
                            let len = payload.data.len();
                            if len > buf.len() {
                                return Err(TransportError::MessageTooLarge);
                            }
                            buf[..len].copy_from_slice(&payload.data);
                            return Ok(Some(len));
                        }
                    }
                    Ok(None)
                }
                Err(dust_dds::infrastructure::error::DdsError::NoData) => Ok(None),
                Err(_) => Err(TransportError::PollFailed),
            }
        }

        #[cfg(not(feature = "std"))]
        {
            let _ = buf;
            Err(TransportError::PollFailed)
        }
    }

    fn has_data(&self) -> bool {
        true
    }

    fn deserialization_error(&self) -> Self::Error {
        TransportError::DeserializationError
    }
}

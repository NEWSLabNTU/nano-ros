//! DDS publisher — implements `nros_rmw::Publisher`.

use nros_rmw::{Publisher, TransportError};

/// DDS publisher backed by a dust-dds `DataWriter`.
pub struct DdsPublisher {
    #[cfg(feature = "std")]
    writer: dust_dds::publication::data_writer::DataWriter<crate::raw_type::RawCdrPayload>,
}

impl DdsPublisher {
    #[cfg(feature = "std")]
    pub(crate) fn new(
        writer: dust_dds::publication::data_writer::DataWriter<crate::raw_type::RawCdrPayload>,
    ) -> Self {
        Self { writer }
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

        #[cfg(not(feature = "std"))]
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
}

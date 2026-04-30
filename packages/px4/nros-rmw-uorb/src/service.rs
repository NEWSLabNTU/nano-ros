//! Stub service / client server impls for the Phase 99.L byte-shaped
//! redesign. Real uORB service support (paired-topic protocol) was
//! prototyped in Phase 90.4 atop the registry path; with the registry
//! gone, services are temporarily unsupported and return
//! [`TransportError::InvalidConfig`] from every operation.
//!
//! Re-introduce a byte-shaped service implementation as a separate
//! follow-up phase once the publish/subscribe path stabilises.

use nros_rmw::{ServiceClientTrait, ServiceRequest, ServiceServerTrait, TransportError};

/// Stub uORB service server. Never functional; every method returns
/// `InvalidConfig`.
#[derive(Debug, Default)]
pub struct UorbServiceServer;

impl ServiceServerTrait for UorbServiceServer {
    type Error = TransportError;

    fn try_recv_request<'a>(
        &mut self,
        _buf: &'a mut [u8],
    ) -> Result<Option<ServiceRequest<'a>>, Self::Error> {
        Err(TransportError::InvalidConfig)
    }

    fn send_reply(&mut self, _sequence_number: i64, _data: &[u8]) -> Result<(), Self::Error> {
        Err(TransportError::InvalidConfig)
    }
}

/// Stub uORB service client. Never functional; every method returns
/// `InvalidConfig`.
#[derive(Debug, Default)]
pub struct UorbServiceClient;

impl ServiceClientTrait for UorbServiceClient {
    type Error = TransportError;

    fn send_request_raw(&mut self, _request: &[u8]) -> Result<(), Self::Error> {
        Err(TransportError::InvalidConfig)
    }

    fn try_recv_reply_raw(
        &mut self,
        _reply_buf: &mut [u8],
    ) -> Result<Option<usize>, Self::Error> {
        Err(TransportError::InvalidConfig)
    }
}

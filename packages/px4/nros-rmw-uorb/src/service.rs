//! Service / action stubs.
//!
//! uORB has no native request/response semantics. Phase 90.4 documents this
//! gap; a follow-up phase will define a paired-topic protocol (request topic
//! `<name>_request` + reply topic `<name>_reply` + a correlation `seq` field)
//! or recommend XRCE for service-heavy workloads.
//!
//! Until then every service/action operation returns
//! [`TransportError::Backend`].

use nros_rmw::{ServiceClientTrait, ServiceRequest, ServiceServerTrait, TransportError};

const NOT_SUPPORTED: TransportError =
    TransportError::Backend("uORB: services not yet supported (Phase 90.4)");

#[derive(Debug)]
pub struct UorbServiceServer;

impl ServiceServerTrait for UorbServiceServer {
    type Error = TransportError;

    fn try_recv_request<'a>(
        &mut self,
        _buf: &'a mut [u8],
    ) -> Result<Option<ServiceRequest<'a>>, Self::Error> {
        Err(NOT_SUPPORTED)
    }

    fn send_reply(&mut self, _seq: i64, _data: &[u8]) -> Result<(), Self::Error> {
        Err(NOT_SUPPORTED)
    }
}

#[derive(Debug)]
pub struct UorbServiceClient;

impl ServiceClientTrait for UorbServiceClient {
    type Error = TransportError;

    fn send_request_raw(&mut self, _request: &[u8]) -> Result<(), Self::Error> {
        Err(NOT_SUPPORTED)
    }

    fn try_recv_reply_raw(&mut self, _reply_buf: &mut [u8]) -> Result<Option<usize>, Self::Error> {
        Err(NOT_SUPPORTED)
    }
}

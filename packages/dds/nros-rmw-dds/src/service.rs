//! DDS service server/client — implements `nros_rmw::ServiceServerTrait` and
//! `nros_rmw::ServiceClientTrait`.
//!
//! DDS services use the ROS 2 request/reply convention: two topics per service
//! (`rq/<service>Request` and `rr/<service>Reply`). Sequence numbers correlate
//! requests to replies.

use nros_rmw::{
    Publisher, ServiceClientTrait, ServiceRequest, ServiceServerTrait, Subscriber, TransportError,
};

use crate::publisher::DdsPublisher;
use crate::subscriber::DdsSubscriber;

/// DDS service server (request DataReader + reply DataWriter).
///
/// Receives requests on `rq/<service>Request`, sends replies on
/// `rr/<service>Reply`. The sequence number from the request is echoed
/// in the reply for correlation.
pub struct DdsServiceServer {
    request_reader: DdsSubscriber,
    reply_writer: DdsPublisher,
}

impl DdsServiceServer {
    #[cfg(feature = "std")]
    pub(crate) fn new(request_reader: DdsSubscriber, reply_writer: DdsPublisher) -> Self {
        Self {
            request_reader,
            reply_writer,
        }
    }
}

impl ServiceServerTrait for DdsServiceServer {
    type Error = TransportError;

    fn has_request(&self) -> bool {
        self.request_reader.has_data()
    }

    fn try_recv_request<'a>(
        &mut self,
        buf: &'a mut [u8],
    ) -> Result<Option<ServiceRequest<'a>>, Self::Error> {
        match self.request_reader.try_recv_raw(buf)? {
            Some(len) => {
                // Extract sequence number from the first 8 bytes of the payload.
                // ROS 2 DDS services prepend an 8-byte header (GUID prefix + seq).
                // For nano-ros-to-nano-ros, we use a simple i64 sequence number
                // at the start of the CDR payload.
                let seq = if len >= 8 {
                    i64::from_le_bytes(buf[..8].try_into().unwrap_or([0; 8]))
                } else {
                    0
                };
                Ok(Some(ServiceRequest {
                    data: &buf[8..len],
                    sequence_number: seq,
                }))
            }
            None => Ok(None),
        }
    }

    fn send_reply(&mut self, sequence_number: i64, data: &[u8]) -> Result<(), Self::Error> {
        // Prepend the sequence number to the reply payload for correlation.
        let seq_bytes = sequence_number.to_le_bytes();
        let total_len = 8 + data.len();

        // Build the reply: [seq_number (8 bytes)][reply CDR data]
        let mut payload = alloc::vec![0u8; total_len];
        payload[..8].copy_from_slice(&seq_bytes);
        payload[8..].copy_from_slice(data);

        self.reply_writer.publish_raw(&payload)
    }
}

/// DDS service client (request DataWriter + reply DataReader).
///
/// Sends requests on `rq/<service>Request`, receives replies on
/// `rr/<service>Reply`. Each request gets a monotonically increasing
/// sequence number for correlation.
pub struct DdsServiceClient {
    request_writer: DdsPublisher,
    reply_reader: DdsSubscriber,
    next_sequence: i64,
    pending_sequence: i64,
}

impl DdsServiceClient {
    #[cfg(feature = "std")]
    pub(crate) fn new(request_writer: DdsPublisher, reply_reader: DdsSubscriber) -> Self {
        Self {
            request_writer,
            reply_reader,
            next_sequence: 1,
            pending_sequence: 0,
        }
    }
}

impl ServiceClientTrait for DdsServiceClient {
    type Error = TransportError;

    fn call_raw(&mut self, request: &[u8], reply_buf: &mut [u8]) -> Result<usize, Self::Error> {
        self.send_request_raw(request)?;

        // Poll for reply with timeout (simple spin for now).
        // In production, this should use a WaitSet or async mechanism.
        #[cfg(feature = "std")]
        {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                if let Some(len) = self.try_recv_reply_raw(reply_buf)? {
                    return Ok(len);
                }
                if std::time::Instant::now() >= deadline {
                    return Err(TransportError::Timeout);
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }

        #[cfg(not(feature = "std"))]
        {
            let _ = reply_buf;
            Err(TransportError::Timeout)
        }
    }

    fn send_request_raw(&mut self, request: &[u8]) -> Result<(), Self::Error> {
        use nros_rmw::Publisher;

        let seq = self.next_sequence;
        self.next_sequence += 1;
        self.pending_sequence = seq;

        // Prepend sequence number to request payload.
        let seq_bytes = seq.to_le_bytes();
        let total_len = 8 + request.len();

        let mut payload = alloc::vec![0u8; total_len];
        payload[..8].copy_from_slice(&seq_bytes);
        payload[8..].copy_from_slice(request);

        self.request_writer.publish_raw(&payload)
    }

    fn try_recv_reply_raw(&mut self, reply_buf: &mut [u8]) -> Result<Option<usize>, Self::Error> {
        use nros_rmw::Subscriber;

        // Read into a temporary buffer to check the sequence number.
        let mut tmp = [0u8; 8192];
        match self.reply_reader.try_recv_raw(&mut tmp)? {
            Some(len) if len >= 8 => {
                let seq = i64::from_le_bytes(tmp[..8].try_into().unwrap_or([0; 8]));
                if seq != self.pending_sequence {
                    // Not our reply — discard and return None.
                    return Ok(None);
                }
                let data_len = len - 8;
                if data_len > reply_buf.len() {
                    return Err(TransportError::MessageTooLarge);
                }
                reply_buf[..data_len].copy_from_slice(&tmp[8..len]);
                Ok(Some(data_len))
            }
            Some(_) => Ok(None), // Too short to contain a sequence number.
            None => Ok(None),
        }
    }
}

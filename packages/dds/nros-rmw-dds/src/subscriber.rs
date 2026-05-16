//! DDS subscriber — implements `nros_rmw::Subscriber`.

use crate::sync::Arc;

use nros_rmw::{Subscriber, TransportError};

use crate::waker_cell::{EventReg, SubscriberShared};

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
    /// construction time. Holds the per-future waker (data-arrival)
    /// AND the Phase 108 status-event callback slots
    /// (LivelinessChanged / RequestedDeadlineMissed / MessageLost).
    shared: Arc<SubscriberShared>,
}

impl DdsSubscriber {
    #[cfg(feature = "std")]
    pub(crate) fn new(
        reader: dust_dds::subscription::data_reader::DataReader<crate::raw_type::RawCdrPayload>,
        shared: Arc<SubscriberShared>,
    ) -> Self {
        Self { reader, shared }
    }

    #[cfg(all(feature = "nostd-runtime", not(feature = "std")))]
    pub(crate) fn new_async(
        reader_async: dust_dds::dds_async::data_reader::DataReaderAsync<
            crate::raw_type::RawCdrPayload,
        >,
        runtime: Arc<NrosPlatformRuntime<nros_platform::ConcretePlatform>>,
        shared: Arc<SubscriberShared>,
    ) -> Self {
        Self {
            reader_async,
            runtime,
            shared,
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
                Err(
                    dust_dds::infrastructure::error::DdsError::NoData
                    | dust_dds::infrastructure::error::DdsError::Timeout,
                ) => Ok(None),
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
                Err(
                    dust_dds::infrastructure::error::DdsError::NoData
                    | dust_dds::infrastructure::error::DdsError::Timeout,
                ) => Ok(None),
                Err(_) => Err(TransportError::PollFailed),
            };
        }

        #[cfg(not(any(feature = "std", feature = "nostd-runtime")))]
        {
            let _ = buf;
            Err(TransportError::PollFailed)
        }
    }

    // Phase 124.D.3 — native batch take. dust-dds's DataReader::take
    // accepts max_samples and returns Vec<Sample<Foo>> in one call.
    // Iterate the returned vec into the caller's contiguous slot
    // buffer. Saves N × await/block_on round-trips compared to the
    // trait default's per-slot try_recv_raw loop.
    fn try_recv_sequence(
        &mut self,
        buf: &mut [u8],
        per_msg_cap: usize,
        max_msgs: usize,
        out_lens: &mut [usize],
    ) -> Result<usize, Self::Error> {
        if per_msg_cap == 0 || max_msgs == 0 {
            return Ok(0);
        }
        let limit = max_msgs.min(out_lens.len());
        let need = limit
            .checked_mul(per_msg_cap)
            .ok_or(TransportError::PollFailed)?;
        if buf.len() < need {
            return Err(TransportError::MessageTooLarge);
        }

        #[cfg(feature = "std")]
        {
            use dust_dds::infrastructure::sample_info::{
                ANY_INSTANCE_STATE, ANY_SAMPLE_STATE, ANY_VIEW_STATE,
            };
            let samples = match self.reader.take(
                limit as i32,
                ANY_SAMPLE_STATE,
                ANY_VIEW_STATE,
                ANY_INSTANCE_STATE,
            ) {
                Ok(s) => s,
                Err(
                    dust_dds::infrastructure::error::DdsError::NoData
                    | dust_dds::infrastructure::error::DdsError::Timeout,
                ) => return Ok(0),
                Err(_) => return Err(TransportError::PollFailed),
            };
            let mut produced = 0usize;
            for sample in samples.into_iter() {
                if produced >= limit {
                    break;
                }
                let Some(payload) = sample.data else { continue };
                let len = payload.data.len();
                if len > per_msg_cap {
                    return Err(TransportError::MessageTooLarge);
                }
                let off = produced * per_msg_cap;
                buf[off..off + len].copy_from_slice(&payload.data);
                out_lens[produced] = len;
                produced += 1;
            }
            return Ok(produced);
        }

        #[cfg(all(feature = "nostd-runtime", not(feature = "std")))]
        {
            use dust_dds::infrastructure::sample_info::{
                ANY_INSTANCE_STATE, ANY_SAMPLE_STATE, ANY_VIEW_STATE,
            };
            let samples = match self.runtime.block_on(self.reader_async.take(
                limit as i32,
                ANY_SAMPLE_STATE,
                ANY_VIEW_STATE,
                ANY_INSTANCE_STATE,
            )) {
                Ok(s) => s,
                Err(
                    dust_dds::infrastructure::error::DdsError::NoData
                    | dust_dds::infrastructure::error::DdsError::Timeout,
                ) => return Ok(0),
                Err(_) => return Err(TransportError::PollFailed),
            };
            let mut produced = 0usize;
            for sample in samples.into_iter() {
                if produced >= limit {
                    break;
                }
                let Some(payload) = sample.data else { continue };
                let len = payload.data.len();
                if len > per_msg_cap {
                    return Err(TransportError::MessageTooLarge);
                }
                let off = produced * per_msg_cap;
                buf[off..off + len].copy_from_slice(&payload.data);
                out_lens[produced] = len;
                produced += 1;
            }
            return Ok(produced);
        }

        #[cfg(not(any(feature = "std", feature = "nostd-runtime")))]
        {
            let _ = (buf, out_lens);
            Err(TransportError::PollFailed)
        }
    }

    fn has_data(&self) -> bool {
        true
    }

    fn register_waker(&self, waker: &core::task::Waker) {
        self.shared.waker_cell.register(waker);
    }

    fn deserialization_error(&self) -> Self::Error {
        TransportError::DeserializationError
    }

    fn supports_event(&self, kind: nros_rmw::EventKind) -> bool {
        // Phase 108.A.dds — Tier-1 sub-side events are surfaced by
        // dust-dds DataReaderListener.
        matches!(
            kind,
            nros_rmw::EventKind::LivelinessChanged
                | nros_rmw::EventKind::RequestedDeadlineMissed
                | nros_rmw::EventKind::MessageLost
        )
    }

    unsafe fn register_event_callback(
        &mut self,
        kind: nros_rmw::EventKind,
        _deadline_ms: u32,
        cb: nros_rmw::EventCallback,
        user_ctx: *mut core::ffi::c_void,
    ) -> Result<(), TransportError> {
        // Deadline is configured at QoS-create time (DataReaderQos.deadline),
        // not on the listener. nros-node only calls register_event_callback
        // after a non-zero deadline_ms is set on QoS, so we don't need to
        // forward _deadline_ms here.
        let slot = match kind {
            nros_rmw::EventKind::LivelinessChanged => &self.shared.liveliness,
            nros_rmw::EventKind::RequestedDeadlineMissed => &self.shared.deadline,
            nros_rmw::EventKind::MessageLost => &self.shared.message_lost,
            _ => return Err(TransportError::Unsupported),
        };
        slot.set(EventReg { cb, user_ctx });
        Ok(())
    }
}

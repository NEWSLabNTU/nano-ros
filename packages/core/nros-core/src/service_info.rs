//! Per-request metadata handed to a service callback — rclrs's `ServiceInfo`.
//!
//! Issue 1942. rclrs 0.7 can pass a service callback a `ServiceInfo` beside the
//! request: who sent it (the client's GID and its sequence number, together
//! the upstream `rmw_request_id_t`) and when (source and received timestamps).
//! This is the counterpart, registered through `Node::create_service_with_info`
//! or `node.service(name).service_info()`.
//!
//! **Not to be confused with `nros_rmw::ServiceInfo`**, the RMW seam's service
//! DESCRIPTOR (name, type, hash, domain). That type is not re-exported by the
//! `nros` facade (RFC-0071 D12, phase-506 W1), which is what lets this one take
//! rclrs's name there.
//!
//! # What a backend reports
//!
//! Every field is an `Option`, because whether a request carries it is a
//! BACKEND property, and a backend that cannot say leaves the field `None`
//! rather than inventing a value — the `MessageInfo` precedent, where a backend
//! that reports no metadata hands the callback `None` instead of a zeroed
//! struct. rclrs's fields are not optional; ours are, and that is the one
//! shape divergence.
//!
//! | backend | `request_id` | `source_timestamp` | `received_timestamp` |
//! | --- | --- | --- | --- |
//! | zenoh | from the query attachment (`rmw_zenoh_cpp` layout) | from the attachment | `None` |
//! | Cyclone DDS | request header guid + publication handle, header seq (stock `rmw_cyclonedds_cpp`'s layout) | the DDS sample's source timestamp | `None` (Cyclone 0.10 has none) |
//! | XRCE-DDS | the Agent's `SampleIdentity` (client writer GUID + sequence) | `None` | `None` |
//! | a C-ABI backend without `take_request_with_info` | `None` | `None` | `None` |
//!
//! A C-ABI backend reports through the vtable's `take_request_with_info` slot
//! (issue 2081), whose `rmw_service_info_t` spells "not reported" as zero; the
//! runtime turns each zero into `None`.

use crate::Time;

/// Width of [`RequestId::writer_guid`], in bytes.
///
/// 16, which is upstream's `rmw_request_id_t::writer_guid` (`int8_t[16]`) and
/// rclrs's `RequestId::writer_guid: [u8; 16]`. It is NOT
/// [`PUBLISHER_GID_SIZE`](crate::PUBLISHER_GID_SIZE) (24, `RMW_GID_STORAGE_SIZE`):
/// upstream keeps the two widths apart, and so does this.
pub const REQUEST_WRITER_GUID_SIZE: usize = 16;

/// Identity of one service request — rclrs's `RequestId`, upstream's
/// `rmw_request_id_t`.
///
/// `sequence_number` is the number the CLIENT assigned to the request, which
/// is what makes `(writer_guid, sequence_number)` unique across clients. It is
/// not necessarily the token a backend replies against: the zenoh backend
/// replies against a local reply-slot index, and that index is never reported
/// here.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct RequestId {
    /// The requesting client's GID. On zenoh this is XXH3-128 of the client's
    /// liveliness keyexpr — `rmw_zenoh_cpp`'s derivation, so it equals the gid
    /// that client's `get_gid` reports, ours or a stock peer's.
    pub writer_guid: [u8; REQUEST_WRITER_GUID_SIZE],
    /// The client-assigned sequence number of this request.
    pub sequence_number: i64,
}

/// Metadata about one received service request — rclrs's `ServiceInfo`.
///
/// See the [module docs](self) for which backend fills which field. A field a
/// backend cannot report is `None`; it is never a zero standing in for one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ServiceInfo {
    /// When the client sent the request, on the CLIENT's clock.
    pub source_timestamp: Option<Time>,
    /// When this server received the request, on the server's clock.
    pub received_timestamp: Option<Time>,
    /// Who sent the request: client GID plus client sequence number.
    pub request_id: Option<RequestId>,
}

impl ServiceInfo {
    /// Nothing reported — what a backend with no per-request metadata hands
    /// every callback.
    pub const NOT_REPORTED: Self = Self {
        source_timestamp: None,
        received_timestamp: None,
        request_id: None,
    };

    /// Whether the backend reported ANY field for this request.
    ///
    /// The Rust spelling of the C++ `MessageInfo::is_reported()`: `false` means
    /// "this backend said nothing", which is a different statement from any
    /// particular field value.
    pub const fn is_reported(&self) -> bool {
        self.source_timestamp.is_some()
            || self.received_timestamp.is_some()
            || self.request_id.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_reported_is_the_default_and_reports_nothing() {
        assert_eq!(ServiceInfo::default(), ServiceInfo::NOT_REPORTED);
        assert!(!ServiceInfo::NOT_REPORTED.is_reported());
    }

    #[test]
    fn any_one_field_makes_it_reported() {
        let only_id = ServiceInfo {
            request_id: Some(RequestId {
                writer_guid: [7; REQUEST_WRITER_GUID_SIZE],
                sequence_number: 3,
            }),
            ..ServiceInfo::NOT_REPORTED
        };
        assert!(only_id.is_reported());
        let only_ts = ServiceInfo {
            source_timestamp: Some(Time::new(1, 2)),
            ..ServiceInfo::NOT_REPORTED
        };
        assert!(only_ts.is_reported());
    }

    /// Upstream `rmw_request_id_t::writer_guid` is `int8_t[16]`; rclrs keeps
    /// `[u8; 16]`. Not the 24-byte gid storage size.
    #[test]
    fn writer_guid_is_upstreams_request_id_width() {
        assert_eq!(REQUEST_WRITER_GUID_SIZE, 16);
    }
}

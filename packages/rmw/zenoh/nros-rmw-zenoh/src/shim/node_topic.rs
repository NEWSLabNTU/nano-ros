// Issue 2493.
//! A node's writer on a data key several nodes share.
//!
//! One ROS publisher in the graph with no zenoh publisher of its own: a
//! publisher liveliness token under its node, its own gid and sequence number
//! in the rmw_zenoh attachment, and the data put on the topic's data keyexpr
//! through the ONE zenoh publisher the session holds for that key. That is
//! the shape stock ROS 2 shows for `/parameter_events` (every node is a
//! publisher of it) without a zenoh publisher slot per node.
//!
//! Declared by [`ZenohSession::declare_node_topic_writer`]. VOLATILE only:
//! a TRANSIENT_LOCAL writer would need a retention cache of its own, which is
//! a publisher's, so it is refused by name.
//!
//! The ROS publisher a runtime creates through the ordinary publisher path
//! gets the same sharing from `ZenohSession::create_publisher`; this type is
//! the same thing for a caller that holds the session directly.

use nros_rmw::TransportError;

use super::{LivelinessToken, RMW_ATTACHMENT_SIZE, RMW_GID_SIZE, ZenohSession};

#[cfg(feature = "safety-e2e")]
use super::RMW_ATTACHMENT_SIZE_WITH_CRC;

/// One node's writer on a shared data key. See the module doc.
pub struct NodeTopicWriter {
    /// A counted holder of the session's zenoh publisher on the data key
    /// (`crate::zpico::Context::share_publisher`), so the data key stays
    /// declared while any writer or publisher on it lives. Stands in for the
    /// data key itself: holding the publisher is what lets a put reach it.
    data_key: crate::zpico::Publisher<'static>,
    /// The publisher liveliness token under this writer's node. Undeclared
    /// when the writer drops.
    token: Option<LivelinessToken>,
    /// XXH3-128 of the token's keyexpr, rmw_zenoh_cpp's derivation.
    gid: [u8; RMW_GID_SIZE],
    /// This writer's sequence number: the last one put.
    seq: u64,
}

impl NodeTopicWriter {
    pub(super) fn new(
        data_key: crate::zpico::Publisher<'static>,
        token: Option<LivelinessToken>,
        gid: [u8; RMW_GID_SIZE],
    ) -> Self {
        Self {
            data_key,
            token,
            gid,
            seq: 0,
        }
    }

    /// This writer's gid, as its attachments carry it.
    pub fn gid(&self) -> [u8; RMW_GID_SIZE] {
        self.gid
    }

    /// Whether this writer declared its liveliness token (it does not on a
    /// session that declares none).
    pub fn has_liveliness(&self) -> bool {
        self.token.is_some()
    }

    /// The zenoh publisher handle the writer puts through. Several writers
    /// on one data key of one session answer the same handle.
    pub fn zenoh_handle(&self) -> i32 {
        self.data_key.handle()
    }

    /// Put `payload` (a CDR message) on the data key, with this writer's own
    /// sequence number and gid in the attachment.
    ///
    /// `session` must be the session that declared the writer; any other is
    /// refused with [`TransportError::InvalidArgument`] rather than put on a
    /// handle that names another session's publisher.
    pub fn put(&mut self, session: &ZenohSession, payload: &[u8]) -> Result<(), TransportError> {
        if session.zenoh_session() != self.data_key.session() {
            return Err(TransportError::InvalidArgument);
        }
        self.seq = self.seq.wrapping_add(1);
        let seq = self.seq as i64;
        let ms = super::publisher::now_ms();
        let ts = (ms.min(i64::MAX as u64) as i64).saturating_mul(1_000_000);

        #[cfg(not(feature = "safety-e2e"))]
        let r = {
            let mut att = [0u8; RMW_ATTACHMENT_SIZE];
            super::publisher::serialize_rmw_attachment(seq, ts, &self.gid, &mut att);
            self.data_key.publish_with_attachment(payload, Some(&att))
        };
        #[cfg(feature = "safety-e2e")]
        let r = {
            let mut att = [0u8; RMW_ATTACHMENT_SIZE_WITH_CRC];
            let (head, tail) = att.split_at_mut(RMW_ATTACHMENT_SIZE);
            super::publisher::serialize_rmw_attachment(
                seq,
                ts,
                &self.gid,
                head.try_into()
                    .map_err(|_| TransportError::BufferTooSmall)?,
            );
            tail.copy_from_slice(&nros_rmw::crc32(payload).to_le_bytes());
            self.data_key.publish_with_attachment(payload, Some(&att))
        };
        r.map_err(TransportError::from)
    }
}

impl Drop for NodeTopicWriter {
    fn drop(&mut self) {
        // The token first: the graph stops listing this writer before its
        // hold on the data key is released (the last holder's release
        // undeclares the zenoh publisher, `crate::zpico::Publisher`'s drop).
        drop(self.token.take());
    }
}

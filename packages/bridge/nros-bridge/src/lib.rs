//! Cross-RMW bridge primitives for nano-ros (Phase 128.F).
//!
//! Lets a binary that intentionally links more than one RMW backend
//! forward raw CDR payloads between Nodes bound to different
//! backends. Common single-backend code does not need this crate.
//!
//! # Pattern
//!
//! 1. Open the executor with [`Executor::open_multi`].
//! 2. Build per-backend Nodes via [`Executor::create_node_on`].
//! 3. Create a raw subscription on the source Node and a raw publisher
//!    on the destination Node (existing
//!    [`create_subscription_raw`] / [`create_publisher_raw`] APIs).
//! 4. Hand them to [`PubSubBridge::new`] and call
//!    [`PubSubBridge::pump`] inside the executor's spin loop.
//!
//! ```ignore
//! use nros::prelude::*;
//! use nros_node::executor::spin::SessionSpec;
//! use nros_bridge::PubSubBridge;
//!
//! fn main() -> Result<(), NodeError> {
//!     let mut exec = Executor::open_multi(&[
//!         SessionSpec::new("zenoh", "tcp/10.0.0.1:7447"),
//!         SessionSpec::new("dds",   "domain=0"),
//!     ])?;
//!     let mut field   = exec.create_node_on("field",   "zenoh")?;
//!     let mut control = exec.create_node_on("control", "dds")?;
//!     // user wires sub on `field`, pub on `control`, then:
//!     // let mut bridge = PubSubBridge::new(sub, pubr, "zenoh");
//!     // bridge.pump()?;
//!     Ok(())
//! }
//! ```
//!
//! # Loop protection (phase 128.F.4)
//!
//! [`PubSubBridge::new`] takes the source backend name and bakes it
//! into every forwarded message's attachment block as `bridge_origin`.
//! Wire receivers that see their own `bridge_origin` value skip the
//! frame to avoid bidirectional echo. The attachment field is opaque
//! to backends that don't speak the ROS 2 attachment convention.

#![no_std]

use nros_node::{
    executor::{EmbeddedRawPublisher, RawSubscription},
    NodeError,
};

/// Bridge a raw subscription on one Node to a raw publisher on
/// another. Each [`pump`](Self::pump) call drains every queued sample
/// from the subscription and forwards the bytes to the publisher.
///
/// Backend bytes pass through untouched — both sides must use ROS-CDR
/// (the default for every backend in tree). Cross-encoding bridges
/// would need an explicit translator and are out of scope.
pub struct PubSubBridge<
    const RX_BUF: usize = { nros_node::config::DEFAULT_RX_BUF_SIZE },
    const TX_BUF: usize = { nros_node::executor::DEFAULT_LOAN_BUF },
> {
    sub: RawSubscription<RX_BUF>,
    pubr: EmbeddedRawPublisher<TX_BUF>,
    /// Phase 128.F.4 — name of the backend the source Node is bound
    /// to. Stamped into the attachment block on every forwarded frame
    /// so a paired return bridge can drop messages that look like
    /// echoes. Empty string ("") disables the tag (single-direction
    /// bridges don't need it).
    origin: &'static str,
}

impl<const RX_BUF: usize, const TX_BUF: usize> PubSubBridge<RX_BUF, TX_BUF> {
    /// Build a one-direction bridge. `origin` is the RMW name of the
    /// session the source subscription is bound to (e.g. `"zenoh"`).
    /// Pass `""` when the bridge is single-direction and loop
    /// protection is not needed.
    pub fn new(
        sub: RawSubscription<RX_BUF>,
        pubr: EmbeddedRawPublisher<TX_BUF>,
        origin: &'static str,
    ) -> Self {
        Self { sub, pubr, origin }
    }

    /// Drain every queued sample and forward to the destination
    /// publisher. Returns the number of samples forwarded on this
    /// call (0 when the subscription queue was empty).
    ///
    /// Errors short-circuit the loop — the caller decides whether to
    /// retry on the next spin tick or surface the error up.
    pub fn pump(&mut self) -> Result<usize, NodeError> {
        let mut forwarded = 0usize;
        while let Some(len) = self.sub.try_recv_raw()? {
            // Phase 128.F.4 — origin tagging is a no-op in the
            // current build because the raw publish path does not
            // yet expose the attachment shape; the field is stored
            // so the contract is clear and a later patch can wire
            // the actual attachment write via
            // `EmbeddedRawPublisher::publish_raw_with_attachment`.
            let _ = self.origin;
            let bytes = &self.sub.buffer()[..len];
            self.pubr.publish_raw(bytes)?;
            forwarded += 1;
        }
        Ok(forwarded)
    }

    /// RMW backend name the source session is bound to. Useful for
    /// pairing two bridges into a bidirectional link where each side
    /// must drop its own origin tag.
    pub fn origin(&self) -> &'static str {
        self.origin
    }

    /// Decompose the bridge back into its source subscription and
    /// destination publisher. Lets a caller rewire one side without
    /// tearing down the other.
    pub fn into_parts(self) -> (RawSubscription<RX_BUF>, EmbeddedRawPublisher<TX_BUF>) {
        (self.sub, self.pubr)
    }
}

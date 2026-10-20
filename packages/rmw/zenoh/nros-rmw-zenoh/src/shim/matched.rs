//! Issue 1972 — matched and incompatible-type events, derived from the graph
//! cache.
//!
//! `rmw_zenoh_cpp` raises `RMW_EVENT_{PUBLICATION,SUBSCRIPTION}_MATCHED` from
//! its graph cache: a liveliness token for an endpoint of the OPPOSITE kind on
//! the same topic arrives or leaves. This shim keeps the same cache — one
//! standing liveliness subscriber per session (`zpico_graph_cache_start`) —
//! so the answer is a walk of it, keyed the way `rmw_zenoh_cpp` keys a topic:
//! the mangled topic NAME plus the DDS TYPE name. An opposite-kind endpoint on
//! the same name with a DIFFERENT type is the incompatible-type case
//! (`RMW_EVENT_*_INCOMPATIBLE_TYPE`), the liveliness token's type segment
//! being what tells the two apart.
//!
//! **Delivery is polled**, from the entity's ordinary data path — exactly
//! where every other status event of this backend fires (`has_data` / take on
//! a subscription, `publish_raw` / `assert_liveliness` on a publisher). So a
//! publisher observes a new match on its next publish, not at the instant the
//! token arrived. The walk is rate-limited to [`MATCH_POLL_INTERVAL_MS`].
//!
//! **Bounds, stated rather than hidden:**
//! * Snapshots, not edges: `total_count` counts increases seen between polls
//!   ([`nros_rmw::MatchedCounter`]).
//! * Matching is by name and type; QoS compatibility is not consulted (zenoh
//!   delivers across QoS mismatches that DDS would refuse to match).
//! * Local endpoints count: a subscription in this same session is matched to
//!   a publisher here, as it is under DDS.
//! * When the cache has DROPPED tokens for lack of room, a lower count proves
//!   nothing, so only increases are reported until it recovers.

use core::cell::Cell;

use nros_rmw::{EventCallback, EventKind, IncompatibleTypeCounter, MatchedCounter, TransportError};

use super::{EntityKind, KEYEXPR_STRING_SIZE, Ros2Liveliness};

/// FNV-1a, 64-bit. Never 0 for a non-empty input in practice; `0` is
/// reserved by [`MatchWatch`] to mean "not rendered".
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Shortest gap between two graph walks for one entity, in ms.
pub(super) const MATCH_POLL_INTERVAL_MS: u64 = 50;

#[derive(Clone, Copy)]
struct Reg {
    cb: EventCallback,
    user_ctx: *mut core::ffi::c_void,
}

/// Per-entity matched / incompatible-type watcher. Inert (no walk, no cost)
/// until a callback is registered.
pub(super) struct MatchWatch {
    handle: *mut zpico_sys::zpico_session_t,
    domain_id: u32,
    /// The kind a MATCHING endpoint has: `Subscriber` for a publisher's
    /// watcher, `Publisher` for a subscription's.
    peer_kind: EntityKind,
    /// The event kinds this watcher raises, per side.
    matched_kind: EventKind,
    incompatible_kind: EventKind,
    /// FNV-1a 64 of the mangled topic name and of the DDS type name, the two
    /// strings a liveliness token is matched on. Hashes, not copies: two
    /// `KEYEXPR_STRING_SIZE` strings would add half a KiB to EVERY publisher
    /// and subscription whether or not anyone registers, which an embedded
    /// image pays for in RAM. `0` = could not be rendered (refused at
    /// registration).
    topic_hash: u64,
    type_hash: u64,
    matched: Cell<MatchedCounter>,
    incompatible: Cell<IncompatibleTypeCounter>,
    matched_cb: Cell<Option<Reg>>,
    incompatible_cb: Cell<Option<Reg>>,
    last_walk_ms: Cell<Option<u64>>,
}

impl MatchWatch {
    /// A watcher for an endpoint on `topic`. `publisher` selects the side.
    pub(super) fn new(
        handle: *mut zpico_sys::zpico_session_t,
        topic: &nros_rmw::TopicInfo,
        publisher: bool,
    ) -> Self {
        // Rendered with the same mangler and type renderer the token builders
        // use, so what we look for and what a peer declares cannot drift.
        let topic_mangled: heapless::String<KEYEXPR_STRING_SIZE> =
            Ros2Liveliness::mangle_topic_name_pub(topic.name);
        let mut dds_type: heapless::String<KEYEXPR_STRING_SIZE> = heapless::String::new();
        let type_ok = core::fmt::write(
            &mut dds_type,
            format_args!("{}", crate::keyexpr::DdsTypeName(topic.type_name)),
        )
        .is_ok();
        // A name too long for the buffer would be TRUNCATED, and a truncated
        // name matches nobody — `register` refuses that case (hash 0) rather
        // than reporting every peer as incompatible.
        let topic_hash = if topic_mangled.is_empty() {
            0
        } else {
            fnv1a(topic_mangled.as_bytes())
        };
        let type_hash = if !type_ok || dds_type.is_empty() {
            0
        } else {
            fnv1a(dds_type.as_bytes())
        };
        let (peer_kind, matched_kind, incompatible_kind) = if publisher {
            (
                EntityKind::Subscriber,
                EventKind::PublicationMatched,
                EventKind::PublisherIncompatibleType,
            )
        } else {
            (
                EntityKind::Publisher,
                EventKind::SubscriptionMatched,
                EventKind::SubscriptionIncompatibleType,
            )
        };
        Self {
            handle,
            domain_id: topic.domain_id,
            peer_kind,
            matched_kind,
            incompatible_kind,
            topic_hash,
            type_hash,
            matched: Cell::new(MatchedCounter::new()),
            incompatible: Cell::new(IncompatibleTypeCounter::new()),
            matched_cb: Cell::new(None),
            incompatible_cb: Cell::new(None),
            last_walk_ms: Cell::new(None),
        }
    }

    /// Whether `kind` is one this watcher raises.
    pub(super) fn handles(&self, kind: EventKind) -> bool {
        kind == self.matched_kind || kind == self.incompatible_kind
    }

    /// Register `cb` for `kind` (one of [`Self::handles`]).
    ///
    /// Starts the session's graph cache if nothing has yet (idempotent on the
    /// C side). `Err(Unsupported)` when the cache cannot run — graph discovery
    /// compiled out (`ZPICO_GRAPH_DISCOVERY=0`) or the declaration refused —
    /// because a watcher with no cache would report "nobody matched" forever,
    /// which is a plausible wrong answer rather than an absent one.
    pub(super) fn register(
        &self,
        kind: EventKind,
        cb: EventCallback,
        user_ctx: *mut core::ffi::c_void,
    ) -> Result<(), TransportError> {
        if !self.handles(kind) {
            return Err(TransportError::Unsupported);
        }
        if self.topic_hash == 0 || self.type_hash == 0 {
            return Err(TransportError::Unsupported);
        }
        let rc = super::session::start_graph_cache(self.handle, self.domain_id)?;
        if rc < 0 {
            return Err(TransportError::Unsupported);
        }
        let reg = Some(Reg { cb, user_ctx });
        if kind == self.matched_kind {
            self.matched_cb.set(reg);
        } else {
            self.incompatible_cb.set(reg);
        }
        // Force the first poll to walk at once.
        self.last_walk_ms.set(None);
        Ok(())
    }

    /// Count `(matched, incompatible)` endpoints in the cache right now, plus
    /// whether the cache dropped tokens. `None` when the cache is not running.
    fn count(&self) -> Option<(u32, u32, bool)> {
        let mut matched = 0u32;
        let mut incompatible = 0u32;
        let dropped = super::session::graph_cache_for_each(self.handle, "matched", &mut |e| {
            if e.kind == self.peer_kind
                && e.domain_id == self.domain_id
                && e.topic.map(|t| fnv1a(t.as_bytes())) == Some(self.topic_hash)
            {
                if e.type_name.map(|t| fnv1a(t.as_bytes())) == Some(self.type_hash) {
                    matched = matched.saturating_add(1);
                } else {
                    incompatible = incompatible.saturating_add(1);
                }
            }
            true
        })
        .ok()?;
        Some((matched, incompatible, dropped != 0))
    }

    /// The poll point. Walks the cache (at most every
    /// [`MATCH_POLL_INTERVAL_MS`]) and fires the registered callbacks for any
    /// movement. A no-op when nothing is registered.
    pub(super) fn poll(&self, now_ms: u64) {
        let m_reg = self.matched_cb.get();
        let i_reg = self.incompatible_cb.get();
        if m_reg.is_none() && i_reg.is_none() {
            return;
        }
        if let Some(last) = self.last_walk_ms.get()
            && now_ms.saturating_sub(last) < MATCH_POLL_INTERVAL_MS
        {
            return;
        }
        self.last_walk_ms.set(Some(now_ms));
        let Some((matched_now, incompatible_now, dropped)) = self.count() else {
            return;
        };
        if let Some(reg) = m_reg {
            let mut counter = self.matched.get();
            // A dropped token may be one of ours: a LOWER count is not proof
            // that anyone left, so hold the old value until the cache recovers.
            let observed = if dropped && matched_now < counter.current() {
                None
            } else {
                counter.observe(matched_now)
            };
            self.matched.set(counter);
            if let Some(status) = observed {
                // SAFETY: `cb`/`user_ctx` were registered together and the
                // registrant keeps `user_ctx` alive for the entity's lifetime;
                // `status` outlives the call.
                unsafe {
                    (reg.cb)(
                        self.matched_kind,
                        &status as *const _ as *const core::ffi::c_void,
                        reg.user_ctx,
                    )
                };
            }
        }
        if let Some(reg) = i_reg {
            let mut counter = self.incompatible.get();
            let observed = counter.observe(incompatible_now);
            self.incompatible.set(counter);
            if let Some(status) = observed {
                // SAFETY: as above.
                unsafe {
                    (reg.cb)(
                        self.incompatible_kind,
                        &status as *const _ as *const core::ffi::c_void,
                        reg.user_ctx,
                    )
                };
            }
        }
    }
}

//! Request/reply correlation for a service client — issue 2009.
//!
//! [`ClientTrait::send_request_raw`] hands back the id the backend assigned to
//! a request and [`ClientTrait::take_response_raw`] hands back the id of the
//! request a reply answers (issue 0778). This module is the ONE place that
//! decides what a client does with the pair, so every surface that takes a
//! reply — the Rust `Promise`, the executor's callback clients, the action
//! client's three sub-clients, the C and C++ blocking paths — applies the same
//! rule:
//!
//! * a reply is delivered only to the request whose id it carries;
//! * a reply whose request is not pending — cancelled, abandoned, timed out,
//!   or a duplicate of one already answered — is DROPPED, never handed to the
//!   next request the client sends;
//! * a backend that cannot supply ids ([`SEQUENCE_ID_UNKNOWN`]) gets a defined
//!   behaviour instead of a guess: one request in flight per client, and a
//!   reply without an id is delivered only when exactly one request is
//!   pending.
//!
//! Bounded and allocation-free: a [`RequestCorrelator`] is a fixed table of
//! `N` `(id, key)` pairs held inline by the client that owns it. A full table
//! refuses the next send ([`SendRefusal::Full`]) before anything reaches the
//! wire, so a request is never sent that its reply could not be matched to.
//!
//! The C++ `rclcpp::Future` keeps several requests in flight on ONE client
//! with a reply buffer per future; it applies this same rule over an intrusive
//! list of its futures (`rclcpp/future.hpp`), because the buffers it delivers
//! into live in the futures, not in the client.
//!
//! [`ClientTrait::send_request_raw`]: crate::ClientTrait::send_request_raw
//! [`ClientTrait::take_response_raw`]: crate::ClientTrait::take_response_raw

use crate::ClientTrait;

/// The id a backend reports when it cannot supply one — the Rust spelling of
/// the ABI's `NROS_RMW_SEQUENCE_ID_UNKNOWN` (`INT64_MIN`). The C vtable
/// adapter pre-sets both out-parameters to it, so a backend that never writes
/// them reads as "no id" rather than as id 0, which is a legal id.
pub const SEQUENCE_ID_UNKNOWN: i64 = i64::MIN;

/// How many stale replies one [`take_correlated`] call drops before it gives
/// up for this poll and reports "nothing yet". Bounds the work a burst of
/// stray replies can make a single poll do; the rest are dropped by the next
/// poll.
pub const MAX_STALE_DRAIN_PER_POLL: usize = 8;

/// Why a send was refused BEFORE it reached the backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendRefusal {
    /// The pending table already holds `N` requests.
    Full,
    /// A request the backend sent WITHOUT an id is still pending, so a second
    /// one could not be told apart from it. Backends that cannot supply ids
    /// keep one request in flight per client.
    Unkeyed,
}

/// A fixed table of the requests a client has in flight, each keyed by the
/// sequence id the backend assigned and carrying a caller-chosen `K` (`()`
/// for a plain service client, the `GoalId` for an action sub-client).
///
/// See the [module docs](self) for the rule it applies.
#[derive(Clone, Copy, Debug)]
pub struct RequestCorrelator<K: Copy, const N: usize> {
    entries: [(i64, K); N],
    len: u8,
    /// Requests sent WITHOUT an id that were abandoned (removed or cleared)
    /// before their reply came. Each costs the next unkeyed reply, which is
    /// dropped rather than handed to a newer request it cannot be told apart
    /// from.
    orphans: u8,
    /// Replies dropped because no pending request matched them. Wrapping;
    /// diagnostic only.
    dropped: u32,
}

impl<K: Copy + Default, const N: usize> Default for RequestCorrelator<K, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Copy + Default, const N: usize> RequestCorrelator<K, N> {
    /// An empty table.
    pub fn new() -> Self {
        const { assert!(N >= 1 && N <= u8::MAX as usize, "N must be 1..=255") };
        Self {
            entries: [(SEQUENCE_ID_UNKNOWN, K::default()); N],
            len: 0,
            orphans: 0,
            dropped: 0,
        }
    }
}

impl<K: Copy, const N: usize> RequestCorrelator<K, N> {
    /// The table's capacity.
    pub const CAPACITY: usize = N;

    /// Requests currently pending.
    pub fn len(&self) -> usize {
        self.len as usize
    }

    /// No request pending.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Replies dropped so far because no pending request matched them.
    pub fn dropped(&self) -> u32 {
        self.dropped
    }

    fn live(&self) -> &[(i64, K)] {
        &self.entries[..self.len as usize]
    }

    /// Whether a new request may be SENT now. Ask this before the send, so a
    /// refused request never reaches the wire.
    pub fn can_send(&self) -> Result<(), SendRefusal> {
        if self.live().iter().any(|(s, _)| *s == SEQUENCE_ID_UNKNOWN) {
            return Err(SendRefusal::Unkeyed);
        }
        if self.len as usize >= N {
            return Err(SendRefusal::Full);
        }
        Ok(())
    }

    /// Whether `seq` is pending.
    pub fn contains(&self, seq: i64) -> bool {
        self.live().iter().any(|(s, _)| *s == seq)
    }

    /// Record a request the backend accepted. Call after a send
    /// [`can_send`](Self::can_send) allowed; `Err(Full)` if the table is full
    /// anyway (nothing is recorded then).
    pub fn insert(&mut self, seq: i64, key: K) -> Result<(), SendRefusal> {
        if self.len as usize >= N {
            return Err(SendRefusal::Full);
        }
        self.entries[self.len as usize] = (seq, key);
        self.len += 1;
        Ok(())
    }

    /// Record a request, evicting the OLDEST pending one when the table is
    /// full. For a RE-SEND of one logical call (`rclc_client_call`'s
    /// discovery-race retry), where every generation answers the same
    /// question and the oldest is the least likely to still be answered.
    pub fn insert_evicting(&mut self, seq: i64, key: K) {
        if self.len as usize >= N {
            let evicted = self.entries[0].0;
            self.remove_at(0);
            self.note_abandoned(evicted);
        }
        let _ = self.insert(seq, key);
    }

    fn remove_at(&mut self, i: usize) {
        let len = self.len as usize;
        self.entries.copy_within(i + 1..len, i);
        self.len -= 1;
    }

    fn note_abandoned(&mut self, seq: i64) {
        if seq == SEQUENCE_ID_UNKNOWN {
            self.orphans = self.orphans.saturating_add(1);
        }
    }

    /// Forget a pending request (cancelled / removed). Its reply, if it still
    /// comes, is dropped. `true` if it was pending.
    pub fn remove(&mut self, seq: i64) -> bool {
        match self.live().iter().position(|(s, _)| *s == seq) {
            Some(i) => {
                self.remove_at(i);
                self.note_abandoned(seq);
                true
            }
            None => false,
        }
    }

    /// Forget every pending request (the caller abandoned them all — a
    /// timed-out blocking call, `reset_in_flight`). Their replies are dropped.
    pub fn clear(&mut self) {
        for i in 0..self.len as usize {
            let seq = self.entries[i].0;
            self.note_abandoned(seq);
        }
        self.len = 0;
    }

    /// Match a reply carrying `reply_seq` to a pending request. `Some(key)`
    /// removes and returns the request it answers; `None` means the reply is
    /// STALE and the caller drops it.
    pub fn resolve(&mut self, reply_seq: i64) -> Option<K> {
        let matched = if reply_seq == SEQUENCE_ID_UNKNOWN {
            if self.orphans > 0 {
                // An abandoned unkeyed request may be the one answering; the
                // reply cannot say, so it goes to nobody.
                self.orphans -= 1;
                None
            } else if self.len == 1 {
                Some(0)
            } else {
                None
            }
        } else {
            match self.live().iter().position(|(s, _)| *s == reply_seq) {
                Some(i) => Some(i),
                // The backend gave the REPLY an id but not the request: the
                // sole pending request is the only candidate.
                None if self.len == 1 && self.entries[0].0 == SEQUENCE_ID_UNKNOWN => Some(0),
                None => None,
            }
        };
        match matched {
            Some(i) => {
                let key = self.entries[i].1;
                self.remove_at(i);
                Some(key)
            }
            None => {
                self.dropped = self.dropped.wrapping_add(1);
                None
            }
        }
    }
}

/// Type-erased [`RequestCorrelator::resolve`], so one `Promise` type can poll
/// a plain service client and an action sub-client alike.
pub trait ReplyMatcher {
    /// `true` if `reply_seq` answers a pending request (which is then retired),
    /// `false` if the reply is stale and must be dropped.
    fn match_reply(&mut self, reply_seq: i64) -> bool;
}

impl<K: Copy, const N: usize> ReplyMatcher for RequestCorrelator<K, N> {
    fn match_reply(&mut self, reply_seq: i64) -> bool {
        self.resolve(reply_seq).is_some()
    }
}

/// Take the next reply that answers a pending request, dropping stale ones.
///
/// `Ok(Some((len, seq, key)))` — a reply for the pending request `key`, its
/// bytes in `buf[..len]`. `Ok(None)` — nothing for a pending request yet (any
/// stale replies met on the way were dropped, at most
/// [`MAX_STALE_DRAIN_PER_POLL`] per call). Errors pass through unchanged.
pub fn take_correlated<C, K, const N: usize>(
    client: &mut C,
    buf: &mut [u8],
    pending: &mut RequestCorrelator<K, N>,
) -> Result<Option<(usize, i64, K)>, C::Error>
where
    C: ClientTrait + ?Sized,
    K: Copy,
{
    for _ in 0..MAX_STALE_DRAIN_PER_POLL {
        match client.take_response_raw(buf)? {
            None => return Ok(None),
            Some((len, seq)) => {
                if let Some(key) = pending.resolve(seq) {
                    return Ok(Some((len, seq, key)));
                }
            }
        }
    }
    Ok(None)
}

/// [`take_correlated`] through the type-erased [`ReplyMatcher`].
pub fn take_matched<C>(
    client: &mut C,
    buf: &mut [u8],
    pending: &mut dyn ReplyMatcher,
) -> Result<Option<(usize, i64)>, C::Error>
where
    C: ClientTrait + ?Sized,
{
    for _ in 0..MAX_STALE_DRAIN_PER_POLL {
        match client.take_response_raw(buf)? {
            None => return Ok(None),
            Some((len, seq)) => {
                if pending.match_reply(seq) {
                    return Ok(Some((len, seq)));
                }
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TransportError;
    use std::vec::Vec;

    type Table = RequestCorrelator<u32, 4>;

    #[test]
    fn a_reply_reaches_only_the_request_it_names() {
        let mut t = Table::new();
        t.insert(10, 1).unwrap();
        t.insert(11, 2).unwrap();
        // Out of order: the second request answers first.
        assert_eq!(t.resolve(11), Some(2));
        assert_eq!(t.resolve(10), Some(1));
        assert!(t.is_empty());
    }

    #[test]
    fn a_reply_for_a_removed_request_is_dropped() {
        let mut t = Table::new();
        t.insert(10, 1).unwrap();
        assert!(t.remove(10));
        t.insert(11, 2).unwrap();
        assert_eq!(t.resolve(10), None, "the late reply goes to nobody");
        assert_eq!(t.dropped(), 1);
        assert_eq!(
            t.resolve(11),
            Some(2),
            "the next request still gets its own"
        );
    }

    #[test]
    fn a_full_table_refuses_the_send() {
        let mut t = RequestCorrelator::<(), 2>::new();
        t.insert(1, ()).unwrap();
        t.insert(2, ()).unwrap();
        assert_eq!(t.can_send(), Err(SendRefusal::Full));
        assert_eq!(t.insert(3, ()), Err(SendRefusal::Full));
        assert_eq!(t.len(), 2, "a refused insert records nothing");
        assert_eq!(t.resolve(1), Some(()));
        assert_eq!(t.can_send(), Ok(()));
    }

    #[test]
    fn evicting_insert_retires_the_oldest_generation() {
        let mut t = RequestCorrelator::<(), 2>::new();
        t.insert_evicting(1, ());
        t.insert_evicting(2, ());
        t.insert_evicting(3, ());
        assert!(!t.contains(1));
        assert!(t.contains(2) && t.contains(3));
    }

    #[test]
    fn unkeyed_backend_keeps_one_request_in_flight() {
        let mut t = Table::new();
        t.insert(SEQUENCE_ID_UNKNOWN, 7).unwrap();
        assert_eq!(t.can_send(), Err(SendRefusal::Unkeyed));
        assert_eq!(t.resolve(SEQUENCE_ID_UNKNOWN), Some(7));
        assert_eq!(t.can_send(), Ok(()));
    }

    #[test]
    fn an_unkeyed_reply_is_ambiguous_with_two_pending() {
        let mut t = Table::new();
        t.insert(1, 1).unwrap();
        t.insert(2, 2).unwrap();
        assert_eq!(t.resolve(SEQUENCE_ID_UNKNOWN), None);
        assert_eq!(
            t.len(),
            2,
            "neither pending request was given someone else's reply"
        );
    }

    #[test]
    fn an_abandoned_unkeyed_request_costs_the_next_unkeyed_reply() {
        let mut t = Table::new();
        t.insert(SEQUENCE_ID_UNKNOWN, 1).unwrap();
        t.clear();
        t.insert(SEQUENCE_ID_UNKNOWN, 2).unwrap();
        assert_eq!(
            t.resolve(SEQUENCE_ID_UNKNOWN),
            None,
            "the reply may be the abandoned request's: dropped, not mismatched"
        );
        assert_eq!(t.resolve(SEQUENCE_ID_UNKNOWN), Some(2));
    }

    /// A client whose replies are scripted, to drive `take_correlated`.
    struct Scripted(Vec<(u8, i64)>);
    impl ClientTrait for Scripted {
        type Error = TransportError;
        fn send_request_raw(&mut self, _r: &[u8]) -> Result<i64, TransportError> {
            Ok(0)
        }
        fn take_response_raw(
            &mut self,
            buf: &mut [u8],
        ) -> Result<Option<(usize, i64)>, TransportError> {
            if self.0.is_empty() {
                return Ok(None);
            }
            let (byte, seq) = self.0.remove(0);
            buf[0] = byte;
            Ok(Some((1, seq)))
        }
    }

    #[test]
    fn take_correlated_skips_stale_replies_to_reach_a_pending_one() {
        let mut c = Scripted(std::vec![(0xAA, 99), (0xBB, 5)]);
        let mut t = Table::new();
        t.insert(5, 42).unwrap();
        let mut buf = [0u8; 4];
        let got = take_correlated(&mut c, &mut buf, &mut t).unwrap();
        assert_eq!(got, Some((1, 5, 42)));
        assert_eq!(buf[0], 0xBB);
        assert_eq!(t.dropped(), 1);
    }

    #[test]
    fn take_matched_reports_nothing_when_only_stale_replies_arrive() {
        let mut c = Scripted(std::vec![(0xAA, 99)]);
        let mut t = RequestCorrelator::<(), 1>::new();
        t.insert(5, ()).unwrap();
        let mut buf = [0u8; 4];
        assert_eq!(take_matched(&mut c, &mut buf, &mut t).unwrap(), None);
        assert!(t.contains(5), "the pending request is still waiting");
    }
}

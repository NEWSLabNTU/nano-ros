//! phase-461 W2 -- the caller-owned service inbox, stated once and
//! RMW-agnostically.
//!
//! A service server is a queryable, and every queryable needs somewhere for a
//! request to land between the transport's read task (the producer) and the
//! executor's spin (the consumer). Until phase-461 the backend owned that ring
//! and every family paid the same geometry: on the safety island 26 queryables
//! x 4 slots x 1,024 B = 115,128 B, 24 of them parameter services whose worst
//! well-formed request is 669 B and whose clients are sequential (issue 1352).
//!
//! W1 made the zenoh ring a HEADER over storage its owner supplies. This
//! module is the other half of that seam: the description a caller hands the
//! backend, in the one crate both the runtime (`nros-node`) and every backend
//! already depend on. Two types:
//!
//! * [`CallerInboxStorage`] -- the backing a family declares as a `static`,
//!   `DEPTH` slots of `SLOT_BYTES` bytes with one [`InboxEntry`] each;
//! * [`CallerInbox`] -- the `Copy` header over it (geometry plus two
//!   pointers), which is what crosses the seam and what an ABI slot would
//!   carry.
//!
//! **Which backends accept one.** [`Session::SUPPORTS_CALLER_INBOX`] is a
//! const, so the caller picks at compile time and a family that cannot bring
//! its own ring says so in a `const` rather than falling back silently
//! (RFC-0052: no dropped knob). Every backend in tree answers `false` today
//! and the reason is per backend, stated at each one: Cyclone holds requests
//! in reader history, XRCE in a C `req_ring`, and `nros-rmw-cffi` FORWARDS
//! over a C vtable that has no slot to carry a ring through. zenoh's own ring
//! is already caller-visible (`nros_rmw_zenoh::InboxStorage`, W1) and answers
//! `true` the moment that vtable can reach it -- see this module's note on
//! `CallerInbox` for what that costs.
//!
//! [`Session::SUPPORTS_CALLER_INBOX`]: crate::Session::SUPPORTS_CALLER_INBOX

use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, AtomicUsize},
};

/// One ring entry's bookkeeping: how many bytes of its slot are valid, the
/// reply-correlation token the backend replies against, and whether the
/// request that arrived was larger than the slot.
///
/// Per ENTRY, whatever storage the ring is over, and never inline in the
/// per-queryable header -- that is the split W1 made and the reason a family
/// can size its own ring without the backend's table growing a variant.
#[repr(C)]
pub struct InboxEntry {
    /// Valid bytes in this entry's slot.
    pub len: AtomicUsize,
    /// Reply-correlation token. What it means is the backend's business (the
    /// zenoh shim stores a reply-slot index); the ring only carries it from
    /// the producer to the consumer.
    pub seq: AtomicUsize,
    /// Set when the incoming request exceeded `slot_bytes`. The payload is
    /// then skipped and the consumer reports it, rather than the ring
    /// truncating a request into a well-formed-looking short one.
    pub overflow: AtomicBool,
}

impl InboxEntry {
    /// A free entry.
    pub const fn new() -> Self {
        Self {
            len: AtomicUsize::new(0),
            seq: AtomicUsize::new(0),
            overflow: AtomicBool::new(false),
        }
    }
}

impl Default for InboxEntry {
    fn default() -> Self {
        Self::new()
    }
}

/// Backing storage for one queryable's request ring: `DEPTH` slots of
/// `SLOT_BYTES` bytes, plus one [`InboxEntry`] each.
///
/// Declared as a `static` by the family that knows its own bound -- for the
/// ROS parameter services that is `nros-node`, the only crate that can see
/// both the contract's declared parameters and the store's capacities
/// (phase-446 F3's `param_service_bound`). It is `static` and not heap for
/// the reason phase-391 keeps every payload buffer static: the Zephyr heap
/// gate prices `arena + 24576` and a new term in it is a new way for an image
/// to stop linking.
///
/// `Sync` because the ring over it is SPSC. The producer is the transport's
/// read task, the consumer is the executor's spin, they are never on the same
/// slot, and the ordering is carried by the cursors in the backend's
/// per-queryable header -- the contract a backend-owned inline array already
/// relied on, now stated on the type that leaves the backend.
#[repr(C)]
pub struct CallerInboxStorage<const SLOT_BYTES: usize, const DEPTH: usize> {
    entries: [InboxEntry; DEPTH],
    data: UnsafeCell<[[u8; SLOT_BYTES]; DEPTH]>,
}

// SAFETY: see the type's doc -- one producer, one consumer, never on the same
// slot, ordered by the cursors in the backend's header.
unsafe impl<const S: usize, const D: usize> Sync for CallerInboxStorage<S, D> {}

impl<const S: usize, const D: usize> CallerInboxStorage<S, D> {
    /// What one queryable's ring costs at this geometry: the slots plus their
    /// entries. The per-queryable header is NOT in it -- that is paid once per
    /// queryable whatever the ring is, and it stays the backend's.
    pub const BYTES: usize = D * (S + core::mem::size_of::<InboxEntry>());

    /// An empty ring.
    pub const fn new() -> Self {
        Self {
            entries: [const { InboxEntry::new() }; D],
            data: UnsafeCell::new([[0u8; S]; D]),
        }
    }

    /// Bytes one request slot holds.
    pub const fn slot_bytes(&self) -> usize {
        S
    }

    /// Slots in the ring.
    pub const fn depth(&self) -> usize {
        D
    }
}

impl<const S: usize, const D: usize> Default for CallerInboxStorage<S, D> {
    fn default() -> Self {
        Self::new()
    }
}

/// The header a caller hands the backend: a ring's geometry and where its
/// bytes are.
///
/// `#[repr(C)]` and four plain words, because this is the shape an ABI slot
/// has to carry. The road from `nros-node` to the zenoh shim runs through
/// `nros-rmw-cffi`'s C vtable (`packages/core/nros-rmw-abi/include/nros/`),
/// which today has no `create_service` variant that takes one; until it does,
/// [`crate::Session::SUPPORTS_CALLER_INBOX`] is `false` on every backend the
/// runtime can reach and a family's [`ServiceInboxSpec`] resolves to
/// [`ServiceInboxSpec::Backend`]. Laying this out as an ABI struct now is what
/// keeps that a forwarding problem rather than a redesign.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct CallerInbox {
    slot_bytes: usize,
    depth: usize,
    entries: *const InboxEntry,
    data: *mut u8,
}

// SAFETY: the pointers address a `'static` `CallerInboxStorage`, whose own
// `Sync` argument covers every access made through them.
unsafe impl Sync for CallerInbox {}
unsafe impl Send for CallerInbox {}

impl CallerInbox {
    /// A header over storage the caller owns for the life of the program.
    ///
    /// `const`, so a family writes
    /// `static INBOX: CallerInbox = CallerInbox::over(&STORAGE);` and hands
    /// the reference to [`ServiceInboxSpec::Caller`].
    pub const fn over<const S: usize, const D: usize>(
        storage: &'static CallerInboxStorage<S, D>,
    ) -> Self {
        Self {
            slot_bytes: S,
            depth: D,
            entries: storage.entries.as_ptr(),
            data: storage.data.get().cast::<u8>(),
        }
    }

    /// Bytes one request slot holds.
    pub const fn slot_bytes(&self) -> usize {
        self.slot_bytes
    }

    /// Slots in the ring.
    pub const fn depth(&self) -> usize {
        self.depth
    }

    /// Bytes this ring's storage occupies (slots and entries).
    pub const fn storage_bytes(&self) -> usize {
        self.depth * (self.slot_bytes + core::mem::size_of::<InboxEntry>())
    }

    /// Does this header sit over `storage`? For tests, which need to say "the
    /// bytes landed in the caller's static" rather than infer it.
    pub fn is_over<const S: usize, const D: usize>(
        &self,
        storage: &'static CallerInboxStorage<S, D>,
    ) -> bool {
        core::ptr::eq(self.entries, storage.entries.as_ptr())
    }
}

/// Which inbox a service server receives through.
///
/// [`Backend`](Self::Backend) is what every user service passes and what the
/// transport has always done. [`Caller`](Self::Caller) is a builtin family
/// bringing storage it sized itself, and is accepted only where
/// [`crate::Session::SUPPORTS_CALLER_INBOX`] is `true`.
#[derive(Clone, Copy, Default)]
pub enum ServiceInboxSpec {
    /// The backend's own table, at the backend's geometry.
    #[default]
    Backend,
    /// The caller's ring, over storage it owns for the life of the program.
    Caller(&'static CallerInbox),
}

impl ServiceInboxSpec {
    /// The caller's ring, if this spec names one.
    pub const fn caller(&self) -> Option<&'static CallerInbox> {
        match self {
            Self::Backend => None,
            Self::Caller(inbox) => Some(inbox),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static STORAGE: CallerInboxStorage<672, 1> = CallerInboxStorage::new();
    static INBOX: CallerInbox = CallerInbox::over(&STORAGE);

    #[test]
    fn a_header_reports_the_storages_geometry() {
        assert_eq!(INBOX.slot_bytes(), 672);
        assert_eq!(INBOX.depth(), 1);
        assert!(INBOX.is_over(&STORAGE));
        assert_eq!(
            INBOX.storage_bytes(),
            CallerInboxStorage::<672, 1>::BYTES,
            "the header and the storage must price the same ring"
        );
    }

    #[test]
    fn the_ring_costs_its_slots_plus_one_entry_each() {
        assert_eq!(
            CallerInboxStorage::<672, 1>::BYTES,
            672 + core::mem::size_of::<InboxEntry>()
        );
        // Depth is the multiplier the parameter family's saving comes from:
        // the same slot at depth 4 is four rings' worth of bytes.
        assert_eq!(
            CallerInboxStorage::<672, 4>::BYTES,
            4 * CallerInboxStorage::<672, 1>::BYTES
        );
    }

    #[test]
    fn a_backend_spec_names_no_ring() {
        assert!(ServiceInboxSpec::Backend.caller().is_none());
        assert!(ServiceInboxSpec::default().caller().is_none());
        assert!(ServiceInboxSpec::Caller(&INBOX).caller().is_some());
    }
}

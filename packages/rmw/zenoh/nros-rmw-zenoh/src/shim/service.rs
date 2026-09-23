//! ZenohServiceServer and ZenohServiceClient implementations

use core::{cell::UnsafeCell, marker::PhantomData};

use atomic_waker::AtomicWaker;
use portable_atomic::{AtomicBool, AtomicUsize, Ordering};

use nros_rmw::{ClientTrait, ServiceInfo, ServiceRequest, ServiceTrait, TransportError};

use super::{
    AtomicSeqCounter, Context, KEYEXPR_BUFFER_SIZE, KEYEXPR_STRING_SIZE, RMW_ATTACHMENT_SIZE,
    RMW_GID_SIZE, RmwAttachment, SeqScalar,
};
use crate::{
    config::{
        ACTION_INBOX_BYTES, ACTION_INBOX_DEPTH, ACTION_INBOX_QUERYABLES, BUILTIN_INBOX_BYTES,
        BUILTIN_INBOX_DEPTH, DECLARED_APP_QUERYABLES, SERVICE_INBOX_BYTES, SERVICE_INBOX_DEPTH,
    },
    keyexpr::ServiceKeyExpr,
    zpico::{
        self, Queryable, ZPICO_MAX_QUERYABLES, ZPICO_MAX_SESSIONS, ZPICO_QUERYABLE_TABLE_DECLARED,
    },
};

#[cfg(feature = "std")]
use super::signal_executor_wake;

// ============================================================================
// ServiceBuffer
// ============================================================================

/// The user-service family's ring depth -- the number `shim/qos.rs` grants a
/// service's KEEP_LAST depth against.
///
/// Phase 237 follow-up chose 4 for "a burst of queries delivered in one
/// read-task batch -- concurrent goals under load", and every family paid it
/// (issue 1352). phase-461 W1 makes it a knob, `NROS_SERVICE_INBOX_DEPTH`
/// (default 4), beside a separate one for the action family, and a builtin
/// family brings its own through [`InboxSpec::Caller`].
pub(super) const SERVICE_REQUEST_RING_DEPTH: usize = SERVICE_INBOX_DEPTH;

/// One ring entry's bookkeeping: the request's length in its slot, the
/// reply-correlation token, and the overflow flag. 12 bytes on a 32-bit
/// target, and it stays PER ENTRY whatever storage the ring is over.
pub struct InboxEntry {
    /// Length of valid data in the slot.
    pub(super) len: AtomicUsize,
    /// Reply-correlation token (the C shim's reply-slot index).
    pub(super) seq: AtomicSeqCounter,
    /// Set when the incoming request exceeded the slot.
    pub(super) overflow: AtomicBool,
}

impl InboxEntry {
    pub const fn new() -> Self {
        Self {
            len: AtomicUsize::new(0),
            seq: AtomicSeqCounter::new(0),
            overflow: AtomicBool::new(false),
        }
    }
}

impl Default for InboxEntry {
    fn default() -> Self {
        Self::new()
    }
}

/// Backing storage for one queryable's request ring: `DEPTH` entries of
/// `SLOT_BYTES` bytes each, plus the per-entry bookkeeping.
///
/// phase-461 W1 -- the ring is a HEADER ([`InboxRing`]) over storage the owner
/// supplies, so a family that knows its own bound brings its own. The zenoh
/// shim's three tables below (`USER_SERVICE_INBOX`, `ACTION_INBOX` and
/// phase-461 W2b's `BUILTIN_INBOX`) are instances of this type, and so is what
/// a caller registers through [`InboxSpec::Caller`].
///
/// `Sync` because the ring over it is SPSC: the zenoh read task writes a slot
/// the executor is not reading, and the `head` / `tail` cursors in the
/// per-queryable header carry the Release / Acquire pair -- the contract the
/// inline `[ServiceRequestSlot; 4]` array had, unchanged.
#[repr(C)]
pub struct InboxStorage<const SLOT_BYTES: usize, const DEPTH: usize> {
    entries: [InboxEntry; DEPTH],
    data: UnsafeCell<[[u8; SLOT_BYTES]; DEPTH]>,
}

// SAFETY: see the type's doc -- one producer, one consumer, on different
// slots, ordered by the header's cursors.
unsafe impl<const S: usize, const D: usize> Sync for InboxStorage<S, D> {}

impl<const S: usize, const D: usize> InboxStorage<S, D> {
    /// What one queryable's ring costs at this geometry, in bytes: the slots
    /// plus their entries. The per-queryable header (`ServiceBuffer`) is not
    /// in it; that is paid once per queryable whatever the ring is.
    pub const BYTES: usize = D * (S + core::mem::size_of::<InboxEntry>());

    pub const fn new() -> Self {
        Self {
            entries: [const { InboxEntry::new() }; D],
            data: UnsafeCell::new([[0u8; S]; D]),
        }
    }

    pub const fn slot_bytes(&self) -> usize {
        S
    }

    pub const fn depth(&self) -> usize {
        D
    }
}

impl<const S: usize, const D: usize> Default for InboxStorage<S, D> {
    fn default() -> Self {
        Self::new()
    }
}

/// The ring header: its geometry and where its bytes are.
///
/// `queryable_callback` and `take_request` index by `slot_bytes` and `depth`
/// rather than by two crate-wide consts, which is what lets one queryable's
/// ring differ from the next. A header is `Copy`: the per-queryable
/// `ServiceBuffer` holds one by value, bound at registration and never
/// rebound, and a caller's `static` ring is copied into it.
#[derive(Clone, Copy)]
pub struct InboxRing {
    slot_bytes: usize,
    depth: usize,
    entries: *const InboxEntry,
    data: *mut u8,
}

// SAFETY: the pointers address a `'static` `InboxStorage`, whose own `Sync`
// argument covers every access made through them.
unsafe impl Sync for InboxRing {}
unsafe impl Send for InboxRing {}

impl InboxRing {
    /// A ring over storage the caller owns for the life of the program.
    ///
    /// `const`, so a builtin family can write
    /// `static RING: InboxRing = InboxRing::over(&STORAGE);` and hand the
    /// reference to [`InboxSpec::Caller`].
    pub const fn over<const S: usize, const D: usize>(
        storage: &'static InboxStorage<S, D>,
    ) -> Self {
        Self {
            slot_bytes: S,
            depth: D,
            entries: storage.entries.as_ptr(),
            data: storage.data.get().cast::<u8>(),
        }
    }

    /// A header with no storage: what a `ServiceBuffer` holds before
    /// registration binds one. The callback refuses to write through it.
    pub(super) const fn unbound() -> Self {
        Self {
            slot_bytes: 0,
            depth: 0,
            entries: core::ptr::null(),
            data: core::ptr::null_mut(),
        }
    }

    pub const fn slot_bytes(&self) -> usize {
        self.slot_bytes
    }

    pub const fn depth(&self) -> usize {
        self.depth
    }

    pub const fn is_bound(&self) -> bool {
        self.depth != 0
    }

    /// Bytes this ring's storage occupies (entries and slots).
    pub const fn storage_bytes(&self) -> usize {
        self.depth * (self.slot_bytes + core::mem::size_of::<InboxEntry>())
    }

    /// Does this header sit over `storage`? For tests, which need to say
    /// "the bytes landed in the caller's static" rather than infer it.
    pub fn is_over<const S: usize, const D: usize>(
        &self,
        storage: &'static InboxStorage<S, D>,
    ) -> bool {
        core::ptr::eq(self.entries, storage.entries.as_ptr())
    }

    /// Entry `i` of the ring; `i < depth()` and the ring is bound.
    pub(super) fn entry(&self, i: usize) -> &InboxEntry {
        debug_assert!(i < self.depth, "inbox ring entry {i} out of {}", self.depth);
        // SAFETY: a bound ring points at `depth` entries of a `'static`
        // storage, and the index is in range by the caller's contract.
        unsafe { &*self.entries.add(i) }
    }

    /// The first byte of slot `i`; `i < depth()` and the ring is bound.
    pub(super) fn slot_ptr(&self, i: usize) -> *mut u8 {
        debug_assert!(i < self.depth, "inbox ring slot {i} out of {}", self.depth);
        // SAFETY: a bound ring's `data` is `depth * slot_bytes` bytes of a
        // `'static` storage; the offset stays inside it.
        unsafe { self.data.add(i * self.slot_bytes) }
    }
}

/// Which inbox a service server receives through (phase-461 W1).
#[derive(Clone, Copy)]
pub enum InboxSpec {
    /// The shim's user-service table, `NROS_SERVICE_INBOX_BYTES` x
    /// `NROS_SERVICE_INBOX_DEPTH` per queryable.
    UserService,
    /// The shim's action table (`send_goal`, `cancel_goal`, `get_result`),
    /// `NROS_ACTION_INBOX_BYTES` x `NROS_ACTION_INBOX_DEPTH` per queryable.
    Action,
    /// The shim's builtin table -- the six ROS parameter services of a node
    /// and the five REP-2002 lifecycle ones -- `NROS_PARAM_SERVICE_INBOX_BYTES`
    /// x `NROS_PARAM_SERVICE_INBOX_DEPTH` per queryable (phase-461 W2b).
    Builtin,
    /// The caller's own ring, over storage it sized for its family.
    Caller(&'static InboxRing),
}

/// Shared buffer for service server callbacks -- a single-producer (the queryable
/// callback on the zenoh read task) single-consumer (`take_request` on the
/// executor) ring. `head`/`tail` are monotonic wrapping counters; the slot index
/// is `counter % depth`. `tail - head` is the queued count; full -> the callback
/// drops the newest (preserving in-order delivery).
///
/// phase-461 W1 -- this is the per-queryable HEADER: the ring's bytes are no
/// longer inline. What stays here is paid once per queryable whatever the ring
/// is: the reply keyexpr, the cursors, the waker, the session, and now the
/// ring header itself.
pub(super) struct ServiceBuffer {
    /// The ring this queryable receives through, bound at registration by an
    /// [`InboxSpec`] and read by the callback. Unbound until then.
    pub(super) ring: InboxRing,
    /// Consumer cursor (written only by `take_request`).
    pub(super) head: AtomicUsize,
    /// Producer cursor (written only by the callback).
    pub(super) tail: AtomicUsize,
    /// Reply keyexpr -- constant per server (same rr/ topic for every request),
    /// so a single copy suffices.
    pub(super) keyexpr: [u8; 256],
    /// Length of keyexpr.
    pub(super) keyexpr_len: AtomicUsize,
    /// Phase 122.3.c.6.e -- waker registered by event-driven service
    /// servers. Woken by `queryable_callback` after a request lands.
    pub(super) waker: AtomicWaker,
    /// phase-328 (issue 0348) -- the owning zpico session pool slot, recorded
    /// at server-registration time. `queryable_callback` reads it back so
    /// `zpico_queryable_take_reply_seq(session, ...)` addresses the correct
    /// session's reply-slot table (this buffer array is process-global, so the
    /// handle cannot be recovered from the buffer index alone).
    pub(super) session: core::sync::atomic::AtomicPtr<zpico_sys::zpico_session_t>,
}

impl ServiceBuffer {
    pub(super) const fn new() -> Self {
        Self {
            ring: InboxRing::unbound(),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            keyexpr: [0u8; 256],
            keyexpr_len: AtomicUsize::new(0),
            waker: AtomicWaker::new(),
            session: core::sync::atomic::AtomicPtr::new(core::ptr::null_mut()),
        }
    }
}

/// Static headers for service servers, one per queryable.
///
/// phase-328 / issue 0376 -- sized `ZPICO_MAX_SESSIONS * ZPICO_MAX_QUERYABLES`
/// and indexed by `session_index * ZPICO_MAX_QUERYABLES + local`, so two zenoh
/// sessions in one process get disjoint buffer ranges (the C shim's queryable
/// tables are already per-session). At the default `ZPICO_MAX_SESSIONS == 1`
/// this is `[ServiceBuffer; ZPICO_MAX_QUERYABLES]` with `session_index == 0`.
///
/// The index handed to the C shim as the callback context is the index into
/// THIS table, and `queryable_callback` also passes it to
/// `zpico_queryable_take_reply_seq` as the queryable handle. So the header
/// index space is one per session, allocated in the order the C shim allocates
/// its queryable slots; phase-461 W1 partitions the RING storage by family
/// (below) and leaves this space alone.
///
/// # Why there is no `// nros-pool:` annotation
///
/// phase-454 W6.a. Before phase-461 W1 this table held the rings inline and
/// was the largest single consumer of static RAM in a native zenoh image --
/// **144,128 bytes on a native talker**, a node with no service server at all
/// -- so its absence from `book/src/reference/static-pool-inventory.md` is
/// exactly the enumeration failure issue 0271 cost ~145 KB to. It is absent on
/// purpose, and this is the purpose, stated rather than left to be
/// re-discovered:
///
/// `scripts/gen-pool-inventory.py` evaluates a pool as a PRODUCT of knobs at
/// their literal defaults. Two independent things make that impossible here,
/// and `scripts/nros-mem-report.py`'s own header already names the first:
///
/// * the element is a STRUCT, not a byte. `sizeof(ServiceBuffer)` is a
///   `[u8; 256]` reply keyexpr, two cursors, an `AtomicWaker`, a session
///   pointer and a ring header -- a SUM with target-dependent terms, where the
///   grammar has only products. A measured figure is right for one build and
///   wrong for the next appended field, which is the drift class
///   `check-ffi-struct-mirrors` exists for one layer down;
/// * `ZPICO_MAX_QUERYABLES` has a COMPUTED default, so there is no integer to
///   put in the comment even for the count.
///
/// The same holds of the three ring tables below, whose element is an
/// `InboxStorage` whose size is `DEPTH x (SLOT_BYTES + an entry)`: a product
/// with a struct-sized term. So all three follow the documented deliberate
/// non-annotations -- `shim/publisher.rs`'s `LendArena`, `nros_rmw_cffi`'s
/// `MESSAGE_INFO_TABLE`, and `nros_node::executor::backing` -- and their
/// shared principle: **the size is known to the compiler, so read it from the
/// compiler's output.** `just mem-report <elf>` prices each symbol from the
/// ELF, exactly, with no formula to drift. The knobs that size them are still
/// enumerated in the inventory with their defaults, which is what issue 0739
/// asked for -- the table says "no byte figure", which is true, rather than
/// implying they are free.
const SERVICE_BUFFER_COUNT: usize = ZPICO_MAX_SESSIONS * ZPICO_MAX_QUERYABLES;
static mut SERVICE_BUFFERS: [ServiceBuffer; SERVICE_BUFFER_COUNT] =
    [const { ServiceBuffer::new() }; SERVICE_BUFFER_COUNT];

/// Next available LOCAL service-buffer index, per session pool slot. The global
/// index handed to the callback is `session_index * ZPICO_MAX_QUERYABLES + local`.
static NEXT_SERVICE_BUFFER_INDEX: [AtomicUsize; ZPICO_MAX_SESSIONS] =
    [const { AtomicUsize::new(0) }; ZPICO_MAX_SESSIONS];

const fn const_min(a: usize, b: usize) -> usize {
    if a < b { a } else { b }
}

/// The action family's share of one session's queryables: three per declared
/// action server (`send_goal`, `cancel_goal`, `get_result`; the `/status`
/// cache queryable is a transient-local publisher's and takes no inbox),
/// which `build.rs` reads from the sizing descriptor. ZERO on an image that
/// declares nothing -- the action table is then empty and an action queryable
/// draws a user ring, which is the single-table behaviour this phase splits,
/// byte for byte, until W3 prices the two families apart.
const ACTION_INBOX_PER_SESSION: usize = const_min(ACTION_INBOX_QUERYABLES, ZPICO_MAX_QUERYABLES);

/// phase-461 W2b -- the BUILTIN family's share of one session's queryables:
/// the slots this image's own declaration did NOT attribute to the
/// application.
///
/// Derived by SUBTRACTION and never by counting the two families, because a
/// service server IS a queryable and `ZPICO_MAX_QUERYABLES` is already
/// `app + infra + transient-local` on an image that declared
/// (`nros-zpico-build::queryable_default_from`). Restating "six per node and
/// five" here is what issue 0827 measured and `check-infra-queryable-counts`
/// refuses: this crate does not depend on `nros-node` and can see neither the
/// constants nor whether their features are compiled in.
///
/// ZERO on an image that declares nothing (`DECLARED_APP_QUERYABLES` is then
/// `usize::MAX`), so every queryable keeps the user-service geometry it has
/// today, byte for byte -- the rule W1 set for the action table, and the
/// reason an absent declaration means the OPPOSITE of what it means for the
/// table's SIZE. Over-reserving a table costs RAM; under-sizing a ring drops a
/// well-formed request, so the ring partition abstains where the table
/// assumes.
const BUILTIN_INBOX_PER_SESSION: usize = const_min(
    ZPICO_MAX_QUERYABLES - const_min(DECLARED_APP_QUERYABLES, ZPICO_MAX_QUERYABLES),
    ZPICO_MAX_QUERYABLES - ACTION_INBOX_PER_SESSION,
);
const USER_SERVICE_INBOX_PER_SESSION: usize =
    ZPICO_MAX_QUERYABLES - ACTION_INBOX_PER_SESSION - BUILTIN_INBOX_PER_SESSION;
const USER_SERVICE_INBOX_COUNT: usize = ZPICO_MAX_SESSIONS * USER_SERVICE_INBOX_PER_SESSION;
const ACTION_INBOX_COUNT: usize = ZPICO_MAX_SESSIONS * ACTION_INBOX_PER_SESSION;
const BUILTIN_INBOX_COUNT: usize = ZPICO_MAX_SESSIONS * BUILTIN_INBOX_PER_SESSION;

/// The user-service family's rings, `NROS_SERVICE_INBOX_BYTES` x
/// `NROS_SERVICE_INBOX_DEPTH` each. Together with `ACTION_INBOX` this is the
/// storage the inline `[ServiceRequestSlot; 4]` arrays used to be: the two
/// tables hold `ZPICO_MAX_SESSIONS * ZPICO_MAX_QUERYABLES` rings between them.
static USER_SERVICE_INBOX: [InboxStorage<SERVICE_INBOX_BYTES, SERVICE_INBOX_DEPTH>;
    USER_SERVICE_INBOX_COUNT] = [const { InboxStorage::new() }; USER_SERVICE_INBOX_COUNT];

/// The action family's rings, `NROS_ACTION_INBOX_BYTES` x
/// `NROS_ACTION_INBOX_DEPTH` each. The depth stays the twin of
/// `ZPICO_MAX_PENDING_REPLIES`, the C shim's reply-slot table, because that is
/// the family depth 4 was designed for.
static ACTION_INBOX: [InboxStorage<ACTION_INBOX_BYTES, ACTION_INBOX_DEPTH>; ACTION_INBOX_COUNT] =
    [const { InboxStorage::new() }; ACTION_INBOX_COUNT];

/// phase-461 W2b -- the builtin family's rings,
/// `NROS_PARAM_SERVICE_INBOX_BYTES` x `NROS_PARAM_SERVICE_INBOX_DEPTH` each.
///
/// ONE table for the parameter and the lifecycle services, because their
/// geometry is equal: both carry `rcl_interfaces` requests bounded by the
/// contract's declared parameters, and every lifecycle request
/// (`ChangeState`, and four with an empty body) is smaller than every
/// parameter one. Two tables would always hold the same number.
///
/// This is the table the safety island's overflow is in. Twenty-four of its
/// twenty-six queryables are parameter services, and each was paying the
/// transport's 4 x 1,024 B for requests that cannot exceed 669.
static BUILTIN_INBOX: [InboxStorage<BUILTIN_INBOX_BYTES, BUILTIN_INBOX_DEPTH>;
    BUILTIN_INBOX_COUNT] = [const { InboxStorage::new() }; BUILTIN_INBOX_COUNT];

/// Next free ring in each family's table, per session pool slot.
static NEXT_USER_SERVICE_INBOX: [AtomicUsize; ZPICO_MAX_SESSIONS] =
    [const { AtomicUsize::new(0) }; ZPICO_MAX_SESSIONS];
static NEXT_ACTION_INBOX: [AtomicUsize; ZPICO_MAX_SESSIONS] =
    [const { AtomicUsize::new(0) }; ZPICO_MAX_SESSIONS];
static NEXT_BUILTIN_INBOX: [AtomicUsize; ZPICO_MAX_SESSIONS] =
    [const { AtomicUsize::new(0) }; ZPICO_MAX_SESSIONS];

/// The shim's own three families.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ShimFamily {
    UserService,
    Action,
    Builtin,
}

/// phase-461 W2b -- the eleven well-known endpoints a NODE serves because the
/// runtime registered them, not because the application asked.
///
/// The six ROS parameter services (`rcl_interfaces`, one set per node) and the
/// five REP-2002 lifecycle services (`lifecycle_msgs`, one set per lifecycle
/// node). The list is the WIRE's, which is why it can live here: these are
/// spellings ROS 2 fixes, the same eleven `rclcpp` builds from a node's FQN,
/// and not counts of what this image compiled in -- issue 0827's rule is about
/// the COUNTS, which this file still refuses to restate.
const BUILTIN_SERVICE_ENDPOINTS: [&str; 11] = [
    // rcl_interfaces, per node
    "get_parameters",
    "get_parameter_types",
    "set_parameters",
    "set_parameters_atomically",
    "describe_parameters",
    "list_parameters",
    // lifecycle_msgs, REP-2002
    "change_state",
    "get_state",
    "get_available_states",
    "get_available_transitions",
    "get_transition_graph",
];

/// Is `name` one of the builtin families' endpoints?
///
/// # The guard, and what it is guarding against
///
/// A service name is all this layer gets, and a user service MAY be called
/// `set_parameters`. Two things stop such a look-alike quietly receiving a
/// 672-byte ring and dropping a well-formed request, which is the defect half
/// of issue 1352 and would be a poor way to close its RAM half.
///
/// **The name must be a whole last SEGMENT under a node.** `contains` is what
/// the action family can afford -- `/_action/` is an infix ROS reserves -- and
/// it is not enough here. The match is the final `/`-segment, compared whole,
/// with a non-empty node FQN in front of it: `<ns...>/<node>/set_parameters`
/// matches, and `.../set_parameters_v2`, `.../my_set_parameters`,
/// `.../set_parameters/extra` and a bare `set_parameters` with no node in
/// front do not.
///
/// **And the image must have DECLARED the capability.** The name only selects
/// a TABLE; the table is empty unless this image's own declaration left slots
/// to the runtime (`BUILTIN_INBOX_PER_SESSION`, which is
/// `ZPICO_MAX_QUERYABLES - <what the declaration attributed to the
/// application>`). An image that declares nothing, and an image that declares
/// every one of its queryables as its own, both have an empty builtin table --
/// so on either one a look-alike draws exactly the ring it draws today. The
/// two halves are independent on purpose: the first is about the NAME being
/// well formed, the second about the image having said there is a runtime
/// service to name.
/// The family a service name selects, lifted out of [`ZenohServiceServer::new`]
/// so it can be tested without a zenoh session.
///
/// Action first: `/_action/` is an infix ROS reserves, and an action channel is
/// never one of the eleven builtin endpoints. Then the builtin families, then
/// everything else.
fn inbox_for(name: &str) -> InboxSpec {
    if name.contains("/_action/") {
        InboxSpec::Action
    } else if is_builtin_service(name) {
        InboxSpec::Builtin
    } else {
        InboxSpec::UserService
    }
}

fn is_builtin_service(name: &str) -> bool {
    let Some((fqn, endpoint)) = name.rsplit_once('/') else {
        // No `/` at all: not a node-qualified name.
        return false;
    };
    // A leading `/` alone is not a node. `rcl` resolves these under the node's
    // fully-qualified name, so there is always at least one segment in front.
    if fqn.trim_matches('/').is_empty() {
        return false;
    }
    BUILTIN_SERVICE_ENDPOINTS.contains(&endpoint)
}

/// Take the next ring of `family`'s table for `session_index`, or `None` when
/// that table is spent.
fn draw_from(session_index: usize, family: ShimFamily) -> Option<InboxRing> {
    let (cursor, per_session) = match family {
        ShimFamily::UserService => (
            &NEXT_USER_SERVICE_INBOX[session_index],
            USER_SERVICE_INBOX_PER_SESSION,
        ),
        ShimFamily::Action => (&NEXT_ACTION_INBOX[session_index], ACTION_INBOX_PER_SESSION),
        ShimFamily::Builtin => (
            &NEXT_BUILTIN_INBOX[session_index],
            BUILTIN_INBOX_PER_SESSION,
        ),
    };
    let local = cursor.fetch_add(1, Ordering::SeqCst);
    if local >= per_session {
        cursor.fetch_sub(1, Ordering::SeqCst);
        return None;
    }
    let index = session_index * per_session + local;
    Some(match family {
        ShimFamily::UserService => InboxRing::over(&USER_SERVICE_INBOX[index]),
        ShimFamily::Action => InboxRing::over(&ACTION_INBOX[index]),
        ShimFamily::Builtin => InboxRing::over(&BUILTIN_INBOX[index]),
    })
}

/// The ring a shim-family registration receives through, and which table it
/// came from (so a failed declaration can hand it back).
///
/// Own family first, then the other's spare. The two tables hold exactly one
/// ring per header between them, so a header that is free always finds a
/// ring; which family's geometry it gets is exact on an image whose
/// declaration priced the action share and today's single geometry on one
/// that did not.
fn draw_shim_ring(session_index: usize, family: ShimFamily) -> Option<(InboxRing, ShimFamily)> {
    let order = match family {
        ShimFamily::UserService => [
            ShimFamily::UserService,
            ShimFamily::Action,
            ShimFamily::Builtin,
        ],
        ShimFamily::Action => [
            ShimFamily::Action,
            ShimFamily::UserService,
            ShimFamily::Builtin,
        ],
        // phase-461 W2b -- a builtin service that finds its own table spent
        // takes a LARGER ring rather than failing. The three tables hold one
        // ring per header between them, so a free header always finds one.
        ShimFamily::Builtin => [
            ShimFamily::Builtin,
            ShimFamily::UserService,
            ShimFamily::Action,
        ],
    };
    order
        .into_iter()
        .find_map(|f| draw_from(session_index, f).map(|ring| (ring, f)))
}

fn release_shim_ring(session_index: usize, family: ShimFamily) {
    match family {
        ShimFamily::UserService => &NEXT_USER_SERVICE_INBOX[session_index],
        ShimFamily::Action => &NEXT_ACTION_INBOX[session_index],
        ShimFamily::Builtin => &NEXT_BUILTIN_INBOX[session_index],
    }
    .fetch_sub(1, Ordering::SeqCst);
}

/// Bind `ring` to header `buffer_index` and reset its cursors. Called once
/// per header, before the queryable that fills it is declared.
fn bind_ring(buffer_index: usize, ring: InboxRing) {
    let mut buf_ref = ServiceBufferRef::new(buffer_index);
    let buffer = buf_ref.get_mut();
    buffer.ring = ring;
    buffer.head.store(0, Ordering::Release);
    buffer.tail.store(0, Ordering::Release);
}

// ============================================================================
// ServiceBufferRef — safe accessor wrapper
// ============================================================================

/// Safe accessor for a statically-allocated service buffer.
///
/// Encapsulates the `unsafe` access to `SERVICE_BUFFERS` by validating
/// the index once at construction time. Subsequent accesses via [`get()`]
/// are safe because the index is guaranteed in-bounds.
///
/// # Safety invariant
///
/// `SERVICE_BUFFERS` is a module-level `static mut` with a fixed address
/// and element count equal to `SERVICE_BUFFER_COUNT`. The index is validated
/// at construction and never changes, so every `get()` / `get_mut()` call
/// dereferences a valid, in-bounds element.
pub(super) struct ServiceBufferRef {
    index: usize,
}

impl ServiceBufferRef {
    /// Create a new buffer reference with bounds validation.
    ///
    /// # Panics
    ///
    /// Panics if `index >= SERVICE_BUFFER_COUNT`.
    pub(super) fn new(index: usize) -> Self {
        assert!(
            index < SERVICE_BUFFER_COUNT,
            "service buffer index out of bounds: {index} >= {SERVICE_BUFFER_COUNT}"
        );
        Self { index }
    }

    /// Get an immutable reference to the service buffer.
    ///
    /// Safety is guaranteed by the bounds check at construction time.
    /// All shared fields use atomic types, preventing data races.
    pub(super) fn get(&self) -> &ServiceBuffer {
        // Safety: index was validated at construction time.
        // SERVICE_BUFFERS is a module-level static with fixed address.
        unsafe { &SERVICE_BUFFERS[self.index] }
    }

    /// Get a mutable reference to the service buffer.
    ///
    /// Only called from callbacks, which are invoked synchronously
    /// (single-threaded) by zenoh-pico — no concurrent mutable access.
    pub(super) fn get_mut(&mut self) -> &mut ServiceBuffer {
        // Safety: index was validated at construction time.
        // Mutable access is only used by callbacks invoked synchronously
        // by zenoh-pico, so there are no concurrent mutable accesses.
        unsafe { &mut SERVICE_BUFFERS[self.index] }
    }
}

/// Sequence counter for service requests
// Phase 237 — the production queryable callback now records the C shim's
// reply-slot index as the correlation token (seq); only the `#[cfg(test)]`
// service-buffer simulator still hands out monotonic counter values.
#[allow(dead_code)]
pub(super) static SERVICE_SEQ_COUNTER: AtomicSeqCounter = AtomicSeqCounter::new(0);

/// Callback function invoked by the C shim when queries arrive
// `c_char` is `u8` on ARM/aarch64 and `i8` on x86 — so `ptr as *const u8` is a
// no-op on one and a real reinterpret on the other, and `clippy::unnecessary_cast`
// fires under `-D warnings` on ARM hosts only. Repo-wide idiom is `.cast::<u8>()`,
// which compiles identically on both and is never linted; never an `as` cast plus
// an `#[allow]`, which only silences the site it is written on.
extern "C" fn queryable_callback(
    keyexpr: *const core::ffi::c_char,
    keyexpr_len: usize,
    payload: *const u8,
    payload_len: usize,
    ctx: *mut core::ffi::c_void,
) {
    // phase-328/#376 — `ctx` is the GLOBAL buffer index
    // (session_index * ZPICO_MAX_QUERYABLES + local), set at server registration.
    let buffer_index = ctx as usize;
    if buffer_index >= SERVICE_BUFFER_COUNT {
        return;
    }

    let mut buf_ref = ServiceBufferRef {
        index: buffer_index,
    };
    let buffer = buf_ref.get_mut();

    // Copy keyexpr
    let keyexpr_copy_len = keyexpr_len.min(buffer.keyexpr.len() - 1);
    // Safety: keyexpr pointer is valid for keyexpr_copy_len bytes (from C shim)
    unsafe {
        core::ptr::copy_nonoverlapping(
            keyexpr.cast::<u8>(),
            buffer.keyexpr.as_mut_ptr(),
            keyexpr_copy_len,
        );
    }
    buffer.keyexpr[keyexpr_copy_len] = 0; // Null terminate
    buffer
        .keyexpr_len
        .store(keyexpr_copy_len, Ordering::Release);

    // Drop empty-payload queries — they come from background discovery /
    // liveliness probes that zenoh-pico delivers through the same
    // queryable callback as real service requests. Flagging them as
    // `has_request` consumes the slot before the actual CDR-prefixed
    // request lands; the deserializer then trips on the empty buffer
    // and `handle_request` reports `ServiceReplyFailed`.
    if payload.is_null() || payload_len == 0 {
        return;
    }

    // phase-461 W1 - the ring is whatever this header was bound to at
    // registration; an unbound header has nowhere to put the bytes. Cannot
    // happen for a declared queryable (the bind precedes the declaration), so
    // this is a guard, not a path.
    let ring = buffer.ring;
    if !ring.is_bound() {
        return;
    }

    // Phase 237 follow-up — enqueue into the request ring. Drop the newest when
    // full (preserves in-order delivery of buffered requests), so a burst of
    // concurrent arrivals doesn't clobber an unread request.
    let head = buffer.head.load(Ordering::Acquire);
    let tail = buffer.tail.load(Ordering::Relaxed);
    if tail.wrapping_sub(head) >= ring.depth() {
        return;
    }
    let index = tail % ring.depth();
    let slot = ring.entry(index);

    // Phase 237 — the reply correlation token is the C shim's reply-slot index
    // (the cloned query held for a possibly-deferred reply), not a free-running
    // counter. `buffer_index` is the queryable handle.
    // FFI returns i64; narrow to the counter's native width (i32 on 32-bit
    // targets, where AtomicSeqCounter is AtomicI32). Reply-slot indices are
    // small and fit. Symmetric with the `.into()` widening on load.
    let session = buffer.session.load(Ordering::Acquire);
    let seq = unsafe { zpico_sys::zpico_queryable_take_reply_seq(session, buffer_index as i32) };
    slot.seq.store(seq as SeqScalar, Ordering::Relaxed);

    if payload_len > ring.slot_bytes() {
        // Request exceeds the slot - flag overflow, skip payload.
        slot.overflow.store(true, Ordering::Relaxed);
        slot.len.store(0, Ordering::Relaxed);
    } else {
        slot.overflow.store(false, Ordering::Relaxed);
        // Safety: payload pointer is valid for payload_len bytes (from C shim);
        // the slot is `slot_bytes` long and no reader holds it (SPSC).
        unsafe {
            core::ptr::copy_nonoverlapping(payload, ring.slot_ptr(index), payload_len);
        }
        slot.len.store(payload_len, Ordering::Relaxed);
    }

    // Publish the slot: the Release pairs with the consumer's Acquire load of
    // `tail`, so the data/len/seq writes above are visible before the request.
    buffer.tail.store(tail.wrapping_add(1), Ordering::Release);

    // Phase 122.3.c.6.e — wake any task that registered a Waker on
    // this server (event-driven callers).
    buffer.waker.wake();

    // Wake the executor spin loop (if waiting)
    #[cfg(feature = "std")]
    signal_executor_wake();
}

// ============================================================================
// ZenohServiceServer
// ============================================================================

/// Zenoh service server using queryables
///
/// Receives service requests via queryable callbacks.
/// Note: The reply mechanism is limited due to the callback model.
pub struct ZenohServiceServer {
    /// The queryable handle (kept alive to maintain registration)
    _queryable: Queryable,
    /// Safe accessor for the static service buffer
    buf: ServiceBufferRef,
    /// Liveliness token for ROS 2 graph discovery (kept alive for server lifetime)
    _liveliness: Option<super::LivelinessToken>,
    /// Keyexpr buffer for replying (copied from last request)
    reply_keyexpr: [u8; 256],
    /// Keyexpr length
    reply_keyexpr_len: usize,
    /// Reference to context for replying
    context: *const Context,
    /// issue 1437 — the profile `qos::admit` GRANTED for this entity.
    ///
    /// ONE profile for BOTH directions, and that is the honest answer here
    /// rather than a shortcut: zenoh-pico has no per-endpoint QoS slot on a
    /// queryable, so nothing about this entity is negotiated per direction —
    /// the refusals, the request ring and the graph declaration are all
    /// whole-entity. A DDS backend answers the two directions separately
    /// because DDS really does negotiate them separately; this one would be
    /// inventing a distinction to report two values.
    ///
    /// `QOS_PROFILE_UNKNOWN` until `set_granted_qos`, which `create_*` calls
    /// with `admit`'s output.
    granted_qos: nros_rmw::QoSProfile,
    /// Phantom to indicate ownership
    _phantom: PhantomData<()>,
}

impl ZenohServiceServer {
    /// Create a new service server for the given service.
    ///
    /// phase-461 W1 -- the inbox family is read off the name: the three
    /// queryables of an action server are `<action>/_action/{send_goal,
    /// cancel_goal,get_result}` (`nros_rmw::ActionInfo`); everything else is a
    /// user service. A caller that brings its OWN storage registers through
    /// [`Self::new_with_inbox`] instead.
    ///
    /// phase-461 W2b -- and the BUILTIN families the same way: the six ROS
    /// parameter services and the five REP-2002 lifecycle services are
    /// well-known endpoints under a node's FQN, so the shim can select their
    /// table from the name exactly as it selects the action one, with no ABI
    /// hop and no caller-owned storage. `is_builtin_service` is the match and
    /// states what stops a look-alike user service from taking a small ring.
    /// (Named, not linked: it is private, and a public item may not link to
    /// one under `NROS_RUSTDOC_LINKS_STRICT`.)
    pub fn new(
        context: &Context,
        service: &ServiceInfo,
        liveliness: Option<super::LivelinessToken>,
    ) -> Result<Self, TransportError> {
        Self::new_with_inbox(context, service, liveliness, inbox_for(service.name))
    }

    /// Create a service server that receives through `inbox`.
    pub fn new_with_inbox(
        context: &Context,
        service: &ServiceInfo,
        liveliness: Option<super::LivelinessToken>,
        inbox: InboxSpec,
    ) -> Result<Self, TransportError> {
        // phase-328/#376 — allocate a per-session LOCAL buffer index and map it
        // to a global `SERVICE_BUFFERS` slot, so two sessions' servers never
        // share a slot. `session_index` is the C shim's session pool slot.
        let session_index = unsafe { zpico_sys::zpico_session_index(context.handle()) };
        if session_index < 0 || (session_index as usize) >= ZPICO_MAX_SESSIONS {
            return Err(TransportError::ServiceServerCreationFailed);
        }
        let session_index = session_index as usize;
        let local = NEXT_SERVICE_BUFFER_INDEX[session_index].fetch_add(1, Ordering::SeqCst);
        if local >= ZPICO_MAX_QUERYABLES {
            NEXT_SERVICE_BUFFER_INDEX[session_index].fetch_sub(1, Ordering::SeqCst);
            // issue 0406 — this table is the reason, and the bare
            // `ServiceServerCreationFailed` never said so. A service server IS a
            // queryable, and the runtime registers its own before the
            // application declares anything, so an entry enabling the parameter
            // and lifecycle services overflowed an 8-slot table AT BOOT and
            // reported only "creation failed" — which reads as a transport or
            // naming fault, not a capacity limit. Name the knob.
            //
            // issue 0827 — this message used to quote those counts ("param
            // services use 6 and lifecycle services use 6"). It was wrong
            // (lifecycle is 5) and it could not be otherwise: this crate does
            // not depend on `nros-node` and cannot see either the counts or
            // whether their features are even compiled in. A number stated
            // where it cannot be derived is a number that drifts, and this was
            // one of six such spellings. The counts now live beside the code
            // that creates them, as `PARAM_SERVICE_QUERYABLES` and
            // `LIFECYCLE_SERVICE_QUERYABLES`; the message names the knob and
            // the cause, which is all this layer actually knows.
            // phase-392 W5.e — the same exhaustion, TWO different faults, and
            // the message has to say which. Sized from the backend's own
            // budget, the table is simply too small and the fix is the knob.
            // Sized from the entry's DECLARATION (`ZPICO_QUERYABLE_TABLE_DECLARED`),
            // it holds exactly what the model said this image would create, so
            // reaching this point means the image created a service server the
            // model does not declare — and pointing that reader at the knob
            // sends them to enlarge a table that was already right.
            //
            // Being authoritative costs precisely this: "the declaration is
            // wrong" and "the table is too small" become the same event, so the
            // message must name the first (phase-392 W5.b2).
            #[cfg(feature = "std")]
            if ZPICO_QUERYABLE_TABLE_DECLARED {
                log::error!(
                    "service server rejected: this image created an UNDECLARED service \
                     server. Its queryable table holds {} slot(s) for session {}, sized \
                     from what the resolved SystemModel declares — so the table is not \
                     too small, the declaration is incomplete. Declare the service \
                     server in the launch file (a service server is a queryable, and an \
                     action server is three). Raising ZPICO_MAX_QUERYABLES also works \
                     and leaves the model disagreeing with the image.",
                    ZPICO_MAX_QUERYABLES,
                    session_index,
                );
            } else {
                log::error!(
                    "service server rejected: ZPICO_MAX_QUERYABLES={} exhausted for \
                     session {} (a service server is a queryable, and the ROS \
                     parameter and REP-2002 lifecycle services claim theirs before \
                     the application declares anything). Raise ZPICO_MAX_QUERYABLES \
                     and rebuild.",
                    ZPICO_MAX_QUERYABLES,
                    session_index,
                );
            }
            // issue 0460 — the `log::error!` above is the ONLY place that named
            // the knob, and it is `cfg(feature = "std")`: on every embedded
            // image the caller got a bare `ServiceServerCreationFailed` and no
            // explanation. `Backend` carries a `&'static str` through `no_std`
            // with no logger and no allocator, and the capability seam in
            // `nros` prints it verbatim — which is how the three zephyr
            // `workspaces/features` entries finally named their own failure
            // instead of dying quietly after "Network ready".
            // issue 0827 — this string used to quote the counts (6 and 5, and
            // 11 for the pair). Correct at the time, and still the wrong place
            // for them: this crate cannot see `nros-node`'s constants or
            // whether their features are compiled in, so the numbers could only
            // ever be copies. Seven copies existed; two had drifted to the
            // wrong value. Say what this layer knows — the table, the knob, and
            // that the runtime claims slots first.
            // The `no_std` half of the same split. `Backend` carries a
            // `&'static str`, so both spellings are compile-time constants and
            // the branch costs nothing at runtime.
            return Err(TransportError::Backend(if ZPICO_QUERYABLE_TABLE_DECLARED {
                "undeclared service server — the zenoh queryable table was sized from \
                 what this entry's SystemModel declares, and it is full. The table is \
                 not too small; the declaration is incomplete. Declare the service \
                 server in the launch file (an action server declares three)."
            } else {
                "zenoh queryable table exhausted — raise CONFIG_NROS_MAX_QUERYABLES \
                 (env ZPICO_MAX_QUERYABLES). A service server IS a queryable, and the \
                 ROS parameter and REP-2002 lifecycle services claim theirs before an \
                 entry's own callbacks."
            }));
        }
        let buffer_index = session_index * ZPICO_MAX_QUERYABLES + local;

        // phase-461 W1 - bind the ring BEFORE declaring, for the reason the
        // session is recorded first below: a query that arrives during
        // declaration must find storage to land in.
        let (ring, drawn_from) = match inbox {
            InboxSpec::Caller(ring) => (*ring, None),
            InboxSpec::UserService | InboxSpec::Action | InboxSpec::Builtin => {
                let family = match inbox {
                    InboxSpec::Action => ShimFamily::Action,
                    InboxSpec::Builtin => ShimFamily::Builtin,
                    _ => ShimFamily::UserService,
                };
                match draw_shim_ring(session_index, family) {
                    Some((ring, from)) => (ring, Some(from)),
                    None => {
                        // Unreachable while the two tables hold one ring per
                        // header, which they do by construction; said rather
                        // than assumed, because the header space above can be
                        // raised without this file noticing.
                        NEXT_SERVICE_BUFFER_INDEX[session_index].fetch_sub(1, Ordering::SeqCst);
                        return Err(TransportError::Backend(
                            "zenoh service inbox tables exhausted - a queryable header was \
                             free but no ring was; USER_SERVICE_INBOX, ACTION_INBOX and \
                             BUILTIN_INBOX must hold ZPICO_MAX_QUERYABLES rings per session \
                             between them",
                        ));
                    }
                }
            }
        };
        bind_ring(buffer_index, ring);

        // Generate the service key
        let key: heapless::String<KEYEXPR_STRING_SIZE> = service.to_key();

        // Create null-terminated keyexpr
        let mut keyexpr_buf = [0u8; KEYEXPR_BUFFER_SIZE];
        let bytes = key.as_bytes();
        if bytes.len() >= keyexpr_buf.len() {
            return Err(TransportError::TopicNameInvalid);
        }
        keyexpr_buf[..bytes.len()].copy_from_slice(bytes);
        keyexpr_buf[bytes.len()] = 0;

        // phase-328 — record the owning session BEFORE declaring, so a query
        // that arrives during declaration finds the right pool slot.
        ServiceBufferRef::new(buffer_index)
            .get()
            .session
            .store(context.handle(), Ordering::Release);

        // Create queryable with callback
        let queryable = unsafe {
            context.declare_queryable_raw(
                &keyexpr_buf,
                queryable_callback,
                buffer_index as *mut core::ffi::c_void,
            )
        }
        .map_err(|e| {
            NEXT_SERVICE_BUFFER_INDEX[session_index].fetch_sub(1, Ordering::SeqCst);
            if let Some(family) = drawn_from {
                release_shim_ring(session_index, family);
            }
            TransportError::from(e)
        })?;

        Ok(Self {
            _queryable: queryable,
            buf: ServiceBufferRef::new(buffer_index),
            _liveliness: liveliness,
            reply_keyexpr: [0u8; 256],
            reply_keyexpr_len: 0,
            context: context as *const Context,
            _phantom: PhantomData,
            granted_qos: nros_rmw::QoSProfile::QOS_PROFILE_UNKNOWN,
        })
    }

    pub(super) fn set_liveliness(&mut self, liveliness: Option<super::LivelinessToken>) {
        self._liveliness = liveliness;
    }

    /// Record what `qos::admit` granted, for `*_actual_qos` to answer with.
    pub(super) fn set_granted_qos(&mut self, qos: nros_rmw::QoSProfile) {
        self.granted_qos = qos;
    }

    /// issue 1332 / phase-455 W2.b — this server's zenoh queryable handle, the
    /// argument `Context::reply_slot_stats` / `reply_slot_declines` take.
    ///
    /// The reply-slot table is per-queryable, so reading it needs the handle of
    /// the server that owns it. The process-global
    /// [`crate::reply_slot_refusals_total`] exists for a caller several layers
    /// above the `Context` (phase-455 W2's probe binary); a caller holding the
    /// server itself should name it, so a count cannot be credited to the wrong
    /// queryable — the runtime declares its own before an entry's do.
    pub fn queryable_handle(&self) -> i32 {
        self._queryable.handle()
    }
}

impl ServiceTrait for ZenohServiceServer {
    type Error = TransportError;

    fn has_request(&self) -> bool {
        let b = self.buf.get();
        b.head.load(Ordering::Relaxed) != b.tail.load(Ordering::Acquire)
    }

    fn register_waker(&self, waker: &core::task::Waker) {
        self.buf.get().waker.register(waker);
    }

    fn take_request<'a>(
        &mut self,
        buf: &'a mut [u8],
    ) -> Result<Option<ServiceRequest<'a>>, Self::Error> {
        let buffer = self.buf.get();

        // Phase 237 follow-up — dequeue the head of the request ring. The Acquire
        // load of `tail` pairs with the callback's Release store, making the
        // slot's data/len/seq visible before we read them.
        let head = buffer.head.load(Ordering::Relaxed);
        let tail = buffer.tail.load(Ordering::Acquire);
        if head == tail {
            return Ok(None);
        }
        let ring = buffer.ring;
        if !ring.is_bound() {
            return Ok(None);
        }
        let index = head % ring.depth();
        let slot = ring.entry(index);

        // Advance past the head entry (drop it).
        let pop = || buffer.head.store(head.wrapping_add(1), Ordering::Release);

        if slot.overflow.load(Ordering::Acquire) {
            pop();
            return Err(TransportError::MessageTooLarge);
        }

        let len = slot.len.load(Ordering::Acquire);
        if len > buf.len() {
            // Oversized request dropped; the service recovers on the next one.
            pop();
            return Err(TransportError::BufferTooSmall);
        }

        // Copy data + keyexpr under FFI guard so the callback can't write a slot
        // mid-read (the ring keeps producer/consumer on different slots, but the
        // shared keyexpr is copied here too).
        zpico::ffi_guard(|| {
            // Safety: slot data + keyexpr are valid up to their respective lengths.
            unsafe {
                core::ptr::copy_nonoverlapping(
                    ring.slot_ptr(index).cast_const(),
                    buf.as_mut_ptr(),
                    len,
                );

                // Save keyexpr for potential reply (constant per server).
                let keyexpr_len = buffer.keyexpr_len.load(Ordering::Acquire);
                core::ptr::copy_nonoverlapping(
                    buffer.keyexpr.as_ptr(),
                    self.reply_keyexpr.as_mut_ptr(),
                    keyexpr_len,
                );
                self.reply_keyexpr[keyexpr_len] = 0;
                self.reply_keyexpr_len = keyexpr_len;
            }
        });

        #[allow(clippy::useless_conversion)] // i32→i64 on embedded, no-op on std
        let seq: i64 = slot.seq.load(Ordering::Acquire).into();
        pop();

        Ok(Some(ServiceRequest {
            data: &buf[..len],
            sequence_number: seq,
        }))
    }

    fn send_response(&mut self, sequence_number: i64, data: &[u8]) -> Result<(), Self::Error> {
        if self.reply_keyexpr_len == 0 {
            return Err(TransportError::ServiceReplyFailed);
        }

        // Get context reference
        let context = unsafe { &*self.context };

        /* issue 0902 / phase-455 W1 — a NEGATIVE seq is not "the reply
         * failed"; it is "there was never a reply slot to fail with". The C
         * shim clones each query into one of `ZPICO_MAX_PENDING_REPLIES`
         * slots BEFORE the callback runs, and hands -1 on when the table is
         * full. Folding that into `ServiceReplyFailed` is what made a
         * saturated server read as a broken one — the same conflation issue
         * 1088 removed one layer up (`arena.rs`, where a `WouldBlock` from a
         * saturated `take_request` stopped being rewritten), and the reason
         * 0902's 20-90 % completion spread had no observable cause.
         *
         * Say it ONCE. The transition is latched in the C shim, which is the
         * only place that sees the allocation, and taken here — so a
         * permanently saturated table costs one line, not one per spin
         * (phase-444 W6 landed exactly this on the Cyclone parameter
         * services). `nros_log`, never `eprintln!`: this crate reaches
         * `no_std` targets, and std stdio SIGSEGVs a Zephyr native_sim image
         * (issue 0589). The C shim does not print it either — `printk`
         * compiles to a no-op under `ZPICO_SMOLTCP` / `ZPICO_SERIAL`, which is
         * the bare-metal serial board 0902 was measured on. */
        if sequence_number < 0 {
            let handle = self._queryable.handle();
            if context.take_reply_slot_announcement(handle) {
                let (held, refusals, capacity) =
                    context.reply_slot_stats(handle).unwrap_or((0, 0, 0));
                nros_log::log_error!(
                    nros_log::get_logger("nros_rmw_zenoh"),
                    "zenoh reply-slot table exhausted on queryable {}: {}/{} slots held by \
                     deferred replies, {} request(s) refused. Every later request on this \
                     server is accepted and never answered until a slot is released. Raise \
                     ZPICO_MAX_PENDING_REPLIES if this server fields more concurrent \
                     in-flight requests than that.",
                    handle,
                    held,
                    capacity,
                    refusals
                );
            }
            return Err(TransportError::Backend(
                "zenoh reply-slot table exhausted — the query this reply answers was never \
                 cloned into one of ZPICO_MAX_PENDING_REPLIES slots, so there is nothing to \
                 reply to. Distinct from a failed reply: nothing was attempted.",
            ));
        }

        // Phase 237 — `sequence_number` selects the cloned query the C shim is
        // holding for this request (the reply-slot index from `take_request`),
        // so a deferred get_result reply reaches the original requester even
        // after later requests arrived. The reply keyexpr is constant per server
        // (same rr/ topic), so it is NOT cleared — subsequent deferred replies
        // reuse it; it is re-set on every `take_request` regardless.
        context
            .query_reply(
                self._queryable.handle(),
                sequence_number,
                &self.reply_keyexpr[..=self.reply_keyexpr_len],
                data,
                None,
            )
            .map_err(|_| TransportError::ServiceReplyFailed)?;

        Ok(())
    }

    /// See this server's `granted_qos` field — one granted profile serves
    /// both directions on this backend.
    fn request_subscription_actual_qos(&self) -> nros_rmw::QoSProfile {
        self.granted_qos
    }

    /// See [`Self::request_subscription_actual_qos`].
    fn response_publisher_actual_qos(&self) -> nros_rmw::QoSProfile {
        self.granted_qos
    }
}

// ============================================================================
// Reply Wakers (for async service client)
// ============================================================================

use crate::zpico::ZPICO_MAX_PENDING_GETS;

/// One AtomicWaker per (session, pending-get slot). phase-328 / issue 0376 —
/// sized `ZPICO_MAX_SESSIONS * ZPICO_MAX_PENDING_GETS` and indexed by
/// `session_index * ZPICO_MAX_PENDING_GETS + slot`, so a reply on session A's
/// slot N wakes A's future, not session B's future parked on the same C slot
/// index. At the default `ZPICO_MAX_SESSIONS == 1` this is unchanged.
/// Registered by `Promise::poll()`, woken from the C shim when a reply arrives
/// or the channel closes.
const REPLY_WAKER_COUNT: usize = ZPICO_MAX_SESSIONS * ZPICO_MAX_PENDING_GETS;
static REPLY_WAKERS: [AtomicWaker; REPLY_WAKER_COUNT] =
    [const { AtomicWaker::new() }; REPLY_WAKER_COUNT];

/// C callback invoked by zpico.c when a pending get slot gets a reply.
///
/// # Safety
///
/// Called from C (pending_get_reply_handler / pending_get_dropper) with the
/// owning session's pool index and the per-session slot (issue 0376).
/// `slot` must be in [0, ZPICO_MAX_PENDING_GETS); `session_index` in
/// [0, ZPICO_MAX_SESSIONS).
unsafe extern "C" fn reply_waker_callback(session_index: i32, slot: i32) {
    if session_index >= 0
        && (session_index as usize) < ZPICO_MAX_SESSIONS
        && slot >= 0
        && (slot as usize) < ZPICO_MAX_PENDING_GETS
    {
        let idx = session_index as usize * ZPICO_MAX_PENDING_GETS + slot as usize;
        REPLY_WAKERS[idx].wake();
    }
}

/// Register the reply waker callback with the C shim.
///
/// Called once during session initialization.
pub(super) fn register_reply_waker(session: *mut zpico_sys::zpico_session_t) {
    unsafe {
        zpico_sys::zpico_set_reply_waker(session, Some(reply_waker_callback));
    }
}

// ============================================================================
// Service Client
// ============================================================================

// SERVICE_DEFAULT_TIMEOUT_MS is generated by build.rs from the
// NROS_SERVICE_TIMEOUT_MS env var (default 30000).
use crate::config::SERVICE_DEFAULT_TIMEOUT_MS;

/// Zenoh service client using z_get queries
///
/// Service clients send requests via z_get and receive responses from queryables.
pub struct ZenohServiceClient {
    /// Service key expression (null-terminated)
    keyexpr: [u8; 257],
    /// Length of valid keyexpr
    keyexpr_len: usize,
    /// phase-428 W13 — what a matching `SS` liveliness token carries for
    /// this service, rendered the way the token carries it: the mangled
    /// service name (`/add_two_ints` -> `%add_two_ints`) and the DDS type
    /// spelling (`example_interfaces::srv::dds_::AddTwoInts_`). Computed once
    /// at construction; `service_is_ready` compares against them on every
    /// call. The type hash and QoS are NOT matched — a server whose QoS is
    /// incompatible is a question upstream answers through
    /// `rmw_service_server_is_available` too, and it is not answered here
    /// either (issue 1087 leaves QoS matching to the request path).
    service_mangled: heapless::String<KEYEXPR_STRING_SIZE>,
    /// See `service_mangled`.
    dds_type: heapless::String<KEYEXPR_STRING_SIZE>,
    /// The domain the service was created in. The graph cache is already
    /// scoped to the session's domain by its keyexpr; this is the belt to
    /// that brace, because a wrong-domain match is the plausible-wrong-answer
    /// shape.
    domain_id: u32,
    /// Liveliness token for ROS 2 graph discovery (kept alive for client lifetime)
    _liveliness: Option<super::LivelinessToken>,
    /// Reference to context for making queries
    context: *const Context,
    /// phase-328/#376 — the owning session's pool index, cached at construction.
    /// Scopes this client's `REPLY_WAKERS` registrations so a reply on another
    /// session's same-numbered slot cannot wake this client's future.
    session_index: usize,
    /// Timeout in milliseconds
    timeout_ms: u32,
    /// Handles for outstanding non-blocking get operations.
    ///
    /// Was `Option<i32>` (single handle). The C-API blocking
    /// `nros_client_call` resends the request every ~500 ms during a
    /// discovery race (Phase 89.12 cold-boot fix), each resend calling
    /// `send_request_raw` → `zpico_get_start` → fresh slot. Storing only
    /// the latest handle dropped the older slots: when the server's
    /// reply finally arrived on slot N (older than the current handle),
    /// `pending_get_reply_handler` set `received=true` on slot N but
    /// nothing polled it. The slot eventually had its dropper fire on
    /// zenoh-pico's query timeout (`Z_CONFIG_SOCKET_TIMEOUT`, 5 s on
    /// Zephyr), so `zpico_get_check` never returned the data to the
    /// caller. Tracking ALL outstanding handles + polling each in
    /// `take_response_raw` returns the first reply that lands,
    /// regardless of which generation of resend produced it.
    /// Capacity matches the C-side slot pool so we can never lose a
    /// handle the C allocator successfully returned.
    /// Issue 0778 — each outstanding get, PAIRED WITH THE SEQUENCE ID of the
    /// request that produced it. It was a bare `Vec<i32>` of handles, which is
    /// why the reply poll below could only take "first reply wins": with no id
    /// on either side there was nothing to tell a retry generation of the
    /// current request from a reply to a DIFFERENT one.
    pending_handles: heapless::Vec<(i32, i64), ZPICO_MAX_PENDING_GETS>,
    /// Issue 0153 — client GID for the rmw request attachment. rmw_zenoh_cpp
    /// service servers REQUIRE the (sequence_number, source_timestamp, gid)
    /// attachment on the query — `service_take_request` errors without it and
    /// the ROS 2 server never replies (nano↔nano tolerates its absence,
    /// which kept this invisible in-tree).
    rmw_gid: [u8; RMW_GID_SIZE],
    /// Issue 0153 — per-client request sequence counter for the attachment.
    request_seq: AtomicSeqCounter,
    /// issue 1437 — the profile `qos::admit` GRANTED. One profile for both
    /// directions; see `ZenohServiceServer::granted_qos` for why that is the
    /// honest answer on this backend and not a shortcut.
    granted_qos: nros_rmw::QoSProfile,
    /// Phantom to indicate ownership
    _phantom: PhantomData<()>,
}

/// Issue 0153 — platform clock in ms → attachment source_timestamp.
/// Mirrors `publisher.rs`'s `now_ms` (canonical `nros_platform_*` C symbol);
/// falls back to the sequence number when no real clock exists, preserving
/// monotonicity like the publisher path.
fn current_timestamp_ms(fallback_seq: i64) -> i64 {
    unsafe extern "C" {
        fn nros_platform_time_now_ns() -> u64;
    }
    // Issue 0532 item 5 — the ABI is nanoseconds now; this caller wants ms.
    let ms = unsafe { nros_platform_time_now_ns() / 1_000_000 };
    if ms == 0 { fallback_seq } else { ms as i64 }
}

impl ZenohServiceClient {
    /// Create a new service client for the given service
    pub fn new(
        context: &Context,
        service: &ServiceInfo,
        liveliness: Option<super::LivelinessToken>,
    ) -> Result<Self, TransportError> {
        // Generate wildcard service key for queries (matches any type hash from ROS 2).
        let key: heapless::String<KEYEXPR_STRING_SIZE> = service.to_key_wildcard();

        // Create null-terminated keyexpr
        let mut keyexpr_buf = [0u8; KEYEXPR_BUFFER_SIZE];
        let bytes = key.as_bytes();
        if bytes.len() >= keyexpr_buf.len() {
            return Err(TransportError::TopicNameInvalid);
        }
        keyexpr_buf[..bytes.len()].copy_from_slice(bytes);
        keyexpr_buf[bytes.len()] = 0;

        // phase-428 W13 — the two fields a matching server's liveliness token
        // is recognised by. Rendered ONCE here, with the same mangler and the
        // same type renderer the token builders use, so what we look for and
        // what a peer declares cannot drift apart.
        let service_mangled: heapless::String<KEYEXPR_STRING_SIZE> =
            super::Ros2Liveliness::mangle_topic_name_pub(service.name);
        let mut dds_type: heapless::String<KEYEXPR_STRING_SIZE> = heapless::String::new();
        if core::fmt::write(
            &mut dds_type,
            format_args!("{}", crate::keyexpr::DdsTypeName(service.type_name)),
        )
        .is_err()
        {
            return Err(TransportError::TopicNameInvalid);
        }

        #[cfg(feature = "std")]
        log::debug!("Service client keyexpr: {}", key.as_str());

        // phase-328/#376 — cache the owning session's pool slot for
        // session-scoped REPLY_WAKERS indexing.
        let session_index = unsafe { zpico_sys::zpico_session_index(context.handle()) };
        if session_index < 0 || (session_index as usize) >= ZPICO_MAX_SESSIONS {
            return Err(TransportError::ServiceClientCreationFailed);
        }

        Ok(Self {
            keyexpr: keyexpr_buf,
            keyexpr_len: bytes.len(),
            service_mangled,
            dds_type,
            domain_id: service.domain_id,
            _liveliness: liveliness,
            context: context as *const Context,
            session_index: session_index as usize,
            timeout_ms: SERVICE_DEFAULT_TIMEOUT_MS,
            pending_handles: heapless::Vec::new(),
            rmw_gid: RmwAttachment::generate_gid(),
            request_seq: AtomicSeqCounter::new(0),
            granted_qos: nros_rmw::QoSProfile::QOS_PROFILE_UNKNOWN,
            _phantom: PhantomData,
        })
    }

    /// Record what `qos::admit` granted, for `*_actual_qos` to answer with.
    pub(super) fn set_granted_qos(&mut self, qos: nros_rmw::QoSProfile) {
        self.granted_qos = qos;
    }

    /// Set the timeout for service calls
    pub fn set_timeout(&mut self, timeout_ms: u32) {
        self.timeout_ms = timeout_ms;
    }

    /// Append a newly-allocated slot handle to the outstanding list.
    ///
    /// When the list is full we drop the OLDEST handle, not the new
    /// one — the C side has handed us a real slot and refusing to
    /// remember it would lose its reply. The dropped handle's reply
    /// (if any) is forfeited; that slot is recycled by the C
    /// allocator once its dropper fires (zenoh-pico query timeout).
    /// In practice this only triggers when `nros_client_call`'s
    /// resend loop produces more than `ZPICO_MAX_PENDING_GETS`
    /// generations in a single user-visible call — unusual.
    fn track_outstanding(&mut self, handle: i32, seq: i64) {
        if self.pending_handles.is_full() {
            self.pending_handles.remove(0);
        }
        // Cannot fail — we just made room above.
        let _ = self.pending_handles.push((handle, seq));
    }
}

impl ClientTrait for ZenohServiceClient {
    type Error = TransportError;

    fn register_waker(&self, waker: &core::task::Waker) {
        // Wake on any outstanding handle — `nros_client_call`'s resend
        // can leave several gens in flight; any of them could complete
        // first (see `pending_handles` docs).
        for &(handle, _seq) in &self.pending_handles {
            if (handle as usize) < ZPICO_MAX_PENDING_GETS {
                // phase-328/#376 — session-scoped slot (matches reply_waker_callback).
                let idx = self.session_index * ZPICO_MAX_PENDING_GETS + handle as usize;
                REPLY_WAKERS[idx].register(waker);
            }
        }
    }

    fn send_request_raw(&mut self, request: &[u8]) -> Result<i64, Self::Error> {
        let context = unsafe { &*self.context };

        // Issue 0153 — rmw request attachment, same 33-byte layout as the
        // publisher path ([seq le][ts le][gid len][gid]). Built once; the
        // retry loop below re-sends the SAME logical request, so it keeps
        // one sequence number.
        #[allow(clippy::useless_conversion)] // i32→i64 on embedded, no-op on std
        let seq: i64 = (self.request_seq.fetch_add(1, Ordering::Relaxed) + 1).into();
        let ts = current_timestamp_ms(seq);
        let mut attachment = [0u8; RMW_ATTACHMENT_SIZE];
        attachment[0..8].copy_from_slice(&seq.to_le_bytes());
        attachment[8..16].copy_from_slice(&ts.to_le_bytes());
        attachment[16] = RMW_GID_SIZE as u8;
        attachment[17..33].copy_from_slice(&self.rmw_gid);

        // Phase 89.12 #14 + Phase 89.13 flake fix: retry `zpico_get_start`
        // with a bounded wall-clock budget to cover two distinct race
        // classes on multi-threaded zpico backends (POSIX / Zephyr /
        // NuttX / FreeRTOS+lwIP):
        //
        // 1. **Dropper-pending race** (tens of microseconds). A z_get
        //    issued while zenoh-pico is mid-finalization of a *previous*
        //    query — the dropper callback for the prior get_check is
        //    enqueued but hasn't run yet — can be transiently rejected
        //    by the session's pending-query table. Typical surface:
        //        let (_, mut p) = client.send_goal(&g)?;
        //        ... p.take() sees the accept reply ...
        //        let r = client.get_result(&id)?;  // flaked here
        //    Resolves within a few μs once the scheduler runs the
        //    lease / read tasks.
        //
        // 2. **Cold-boot discovery race** (hundreds of milliseconds).
        //    On NuttX QEMU cold start, the Rust client boots in
        //    parallel with the server (the test harness can't delay
        //    the in-guest client, and the pubsub shape already
        //    requires parallel launch). The first `call()` can fire
        //    before zenoh-pico has discovered the server's queryable
        //    via router gossip. 3 tight retries all hit the same
        //    unresolved state within microseconds — the test saw
        //    `Application error: ServiceRequestFailed` as the first
        //    call on NuttX Rust service / action E2E.
        //
        // 800 ms total budget on std covers both cases comfortably
        // (cold-boot discovery empirically lands in 200–600 ms on
        // QEMU NuttX). Between attempts, yield ~5 ms via
        // `thread::sleep` so zenoh-pico's background pthread(s) can
        // advance the session state — spin-looping here starves the
        // lease / read task on single-core QEMU hosts. On no_std
        // fallback we keep the original tight 3-retry count: bare
        // metal / single-threaded zpico has no parallel progress to
        // wait on, and the dropper-pending race there is the only
        // reproducible failure mode.
        // rustc warns "value assigned to `last_err` is never read" because
        // only the *last* assignment in the loop is observable, and the
        // happy path exits via `return Ok(())`. Suppress — the value IS
        // read on the timeout/exhaustion fallthrough at the bottom.
        #[allow(unused_assignments)]
        let mut last_err = None;
        #[cfg(feature = "std")]
        {
            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(800);
            loop {
                match context.get_start_with_attachment(
                    &self.keyexpr[..=self.keyexpr_len],
                    request,
                    &attachment,
                    self.timeout_ms,
                ) {
                    Ok(handle) => {
                        self.track_outstanding(handle, seq);
                        return Ok(seq);
                    }
                    Err(e) => last_err = Some(e),
                }
                if std::time::Instant::now() >= deadline {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
        #[cfg(not(feature = "std"))]
        {
            // 80 × 5 ms = 400 ms budget. Covers cold-boot discovery on
            // multi-threaded zpico backends (FreeRTOS+lwIP, ThreadX+NetX)
            // where the lease / read task needs scheduler quanta to
            // advance the session state past pending-query / queryable
            // gossip. `z_sleep_ms` yields cooperatively on those
            // backends; on bare-metal single-threaded zpico it's a
            // busy-loop fallback but the count keeps it bounded.
            #[cfg(not(feature = "platform-threadx"))]
            unsafe extern "C" {
                fn z_sleep_ms(time: usize) -> i8;
            }
            const MAX_ATTEMPTS: u32 = 80;
            const SLEEP_MS: usize = 5;
            for attempt in 0..MAX_ATTEMPTS {
                match context.get_start_with_attachment(
                    &self.keyexpr[..=self.keyexpr_len],
                    request,
                    &attachment,
                    self.timeout_ms,
                ) {
                    Ok(handle) => {
                        self.track_outstanding(handle, seq);
                        return Ok(seq);
                    }
                    Err(e) => last_err = Some(e),
                }
                if attempt + 1 < MAX_ATTEMPTS {
                    #[cfg(feature = "platform-threadx")]
                    unsafe {
                        let _ = zpico_sys::zpico_spin_once(context.handle(), SLEEP_MS as u32);
                    }
                    #[cfg(not(feature = "platform-threadx"))]
                    unsafe {
                        z_sleep_ms(SLEEP_MS)
                    };
                }
            }
        }
        Err(TransportError::from(last_err.unwrap()))
    }

    fn take_response_raw(
        &mut self,
        reply_buf: &mut [u8],
    ) -> Result<Option<(usize, i64)>, Self::Error> {
        if self.pending_handles.is_empty() {
            return Ok(None);
        }

        let context = unsafe { &*self.context };

        #[cfg(not(feature = "std"))]
        {
            let _ = context.spin_once(0);
        }

        // Poll every outstanding handle and REPORT WHICH REQUEST replied.
        //
        // Issue 0778 — this used to be "first reply wins", justified by
        // "queryable is idempotent at the application layer". That is true of
        // the resend loop in the C-API blocking caller, which leaves several
        // generations of ONE logical request in flight — and false the moment
        // two DIFFERENT requests are outstanding, which this list cannot
        // distinguish on its own. `send_goal` and `SetParameters` both travel
        // this path and neither is idempotent. Each handle now carries the
        // sequence id of the request that produced it, so the caller can tell
        // which one it got back instead of the ABI assuming it does not matter.
        //
        // Newest first matches the common case where the latest send
        // is what completed — most calls allocate only one slot, so
        // we get out in one iteration.
        let mut hit_idx: Option<usize> = None;
        let mut hit_len: usize = 0;
        let mut hit_seq: i64 = 0;
        let mut hard_err: Option<Self::Error> = None;
        for (idx, &(handle, seq)) in self.pending_handles.iter().enumerate().rev() {
            match context.get_check(handle, reply_buf) {
                Ok(Some(len)) => {
                    hit_idx = Some(idx);
                    hit_len = len;
                    hit_seq = seq;
                    break;
                }
                Ok(None) => continue,
                Err(e) => {
                    // Note the error but keep checking the others —
                    // one slot's dropper-only timeout shouldn't lose
                    // a sibling's still-pending reply. If everyone
                    // errored we'll surface the last one.
                    hard_err = Some(TransportError::from(e));
                }
            }
        }

        if let Some(idx) = hit_idx {
            // Issue 0778 — drop only the generations of the request that
            // ANSWERED (everything sharing its sequence id), not every
            // outstanding slot. Clearing the lot is what made a second
            // in-flight request disappear when the first one replied.
            let answered = self.pending_handles[idx].1;
            self.pending_handles.retain(|&(_, seq)| seq != answered);
            return Ok(Some((hit_len, hit_seq)));
        }

        if let Some(e) = hard_err {
            // Every outstanding handle errored (e.g. each got a
            // dropper-only timeout without data). Surface the failure.
            self.pending_handles.clear();
            return Err(e);
        }

        Ok(None)
    }

    /// phase-428 W13 — answered from the session's MATCHED-SERVER SET, not a
    /// latch and not a query.
    ///
    /// The set is the graph cache: one standing liveliness subscriber per
    /// session on `@ros2_lv/<domain>/**`, fed PUT on declare and DELETE on
    /// undeclare (`zpico_graph_cache_start`, `zpico_graph_set_apply`). This
    /// is what upstream's DDS discovery cache is to
    /// `rmw_service_server_is_available`: the middleware already knows which
    /// endpoints are matched, and no query is issued at call time. So the
    /// answer is synchronous, CURRENT, and can go from true back to false —
    /// the property issue 1087 was about, which the `server_seen` latch this
    /// replaces did not have (set on the first liveliness reply, never
    /// cleared, so a server that died read available for the client's
    /// lifetime).
    ///
    /// Three answers, kept apart:
    ///   * `Ok(true)`  — at least one `SS` token for this service name AND
    ///     type is in the set.
    ///   * `Ok(false)` — none is, and the set is complete.
    ///   * `Err(Unsupported)` — the cache is not running (the platform stubs
    ///     the subscriber, or the session could not declare it), OR it has
    ///     DROPPED tokens for lack of room, so an absence proves nothing.
    ///     Every caller reads this through `matches!(.., Ok(true))` (issue
    ///     1008) and keeps waiting.
    ///
    /// The match is on `(service name, type)`, the pair `rmw_zenoh_cpp` keys a
    /// service on (`liveliness_utils.cpp`, `Entity::Entity`); zid, node,
    /// namespace, type hash and QoS are not consulted.
    fn service_is_ready(&self) -> Result<bool, TransportError> {
        let context = unsafe { &*self.context };
        let mut found = false;
        let dropped =
            super::session::graph_cache_for_each(context.handle(), "service_is_ready", &mut |e| {
                if e.kind == super::EntityKind::ServiceServer
                    && e.domain_id == self.domain_id
                    && e.topic == Some(self.service_mangled.as_str())
                    && e.type_name == Some(self.dds_type.as_str())
                {
                    found = true;
                    return false; // one is enough
                }
                true
            })?;
        if found {
            Ok(true)
        } else if dropped == 0 {
            Ok(false)
        } else {
            // The token we want may be among the ones that did not fit.
            // "Cannot say" is the honest answer, and the caller waits on it.
            Err(TransportError::Unsupported)
        }
    }

    /// See this server's `granted_qos` field — one granted profile serves
    /// both directions on this backend.
    fn request_publisher_actual_qos(&self) -> nros_rmw::QoSProfile {
        self.granted_qos
    }

    /// See [`Self::request_publisher_actual_qos`].
    fn response_subscription_actual_qos(&self) -> nros_rmw::QoSProfile {
        self.granted_qos
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use nros_rmw::TransportError;

    // --- Service buffer helpers ---

    /// The ring a test slot receives through: the shim's user table, bound on
    /// first use the way `new_with_inbox` binds it for a real server. A slot
    /// already bound (to a caller ring, below) keeps its binding.
    fn bind_shim_ring_for_test(slot: usize) {
        if !ServiceBufferRef::new(slot).get().ring.is_bound() {
            bind_ring(slot, InboxRing::over(&USER_SERVICE_INBOX[slot]));
        }
    }

    /// Give test slot `slot` the caller's ring, as `InboxSpec::Caller` would.
    pub(in crate::shim) fn bind_caller_ring_for_test(slot: usize, ring: &'static InboxRing) {
        bind_ring(slot, *ring);
    }

    /// Simulate a service request callback by enqueuing into the buffer ring:
    /// the producer half of `queryable_callback`, ring-full drop and overflow
    /// flag included.
    pub(in crate::shim) fn simulate_service_request(slot: usize, payload: &[u8], keyexpr: &[u8]) {
        bind_shim_ring_for_test(slot);
        let mut buf_ref = ServiceBufferRef::new(slot);
        let buffer = buf_ref.get_mut();

        let klen = keyexpr.len().min(buffer.keyexpr.len() - 1);
        buffer.keyexpr[..klen].copy_from_slice(&keyexpr[..klen]);
        buffer.keyexpr[klen] = 0;
        buffer.keyexpr_len.store(klen, Ordering::Release);

        let ring = buffer.ring;
        let head = buffer.head.load(Ordering::Acquire);
        let tail = buffer.tail.load(Ordering::Relaxed);
        if tail.wrapping_sub(head) >= ring.depth() {
            return;
        }
        let index = tail % ring.depth();
        let entry = ring.entry(index);
        if payload.len() > ring.slot_bytes() {
            entry.overflow.store(true, Ordering::Relaxed);
            entry.len.store(0, Ordering::Relaxed);
        } else {
            entry.overflow.store(false, Ordering::Relaxed);
            // Safety: the slot is `slot_bytes` long and nothing reads it yet.
            unsafe {
                core::ptr::copy_nonoverlapping(
                    payload.as_ptr(),
                    ring.slot_ptr(index),
                    payload.len(),
                );
            }
            entry.len.store(payload.len(), Ordering::Relaxed);
        }
        let seq = SERVICE_SEQ_COUNTER.fetch_add(1, Ordering::Relaxed);
        entry.seq.store(seq, Ordering::Relaxed);
        buffer.tail.store(tail.wrapping_add(1), Ordering::Release);
    }

    /// Reset a service buffer to idle state (empty ring).
    pub(in crate::shim) fn reset_service_buffer(slot: usize) {
        bind_shim_ring_for_test(slot);
        let mut buf_ref = ServiceBufferRef::new(slot);
        let buffer = buf_ref.get_mut();
        buffer.head.store(0, Ordering::Release);
        buffer.tail.store(0, Ordering::Release);
        buffer.keyexpr_len.store(0, Ordering::Release);
    }

    /// Try to receive a service request from a buffer slot.
    /// Replicates `take_request` logic for testing without a zenoh queryable.
    pub(in crate::shim) fn take_service(
        slot: usize,
        recv_buf: &mut [u8],
    ) -> Result<Option<usize>, TransportError> {
        let buf_ref = ServiceBufferRef::new(slot);
        let buffer = buf_ref.get();

        let head = buffer.head.load(Ordering::Relaxed);
        let tail = buffer.tail.load(Ordering::Acquire);
        if head == tail {
            return Ok(None);
        }
        let ring = buffer.ring;
        let index = head % ring.depth();
        let entry = ring.entry(index);

        if entry.overflow.load(Ordering::Acquire) {
            buffer.head.store(head.wrapping_add(1), Ordering::Release);
            return Err(TransportError::MessageTooLarge);
        }

        let len = entry.len.load(Ordering::Acquire);
        if len > recv_buf.len() {
            buffer.head.store(head.wrapping_add(1), Ordering::Release);
            return Err(TransportError::BufferTooSmall);
        }

        // Safety: Data is valid up to len bytes
        unsafe {
            core::ptr::copy_nonoverlapping(
                ring.slot_ptr(index).cast_const(),
                recv_buf.as_mut_ptr(),
                len,
            );
        }

        buffer.head.store(head.wrapping_add(1), Ordering::Release);
        Ok(Some(len))
    }

    /// Read the keyexpr from a service buffer slot (for keyexpr preservation tests).
    fn read_service_keyexpr(slot: usize) -> heapless::Vec<u8, 256> {
        let buf_ref = ServiceBufferRef::new(slot);
        let buffer = buf_ref.get();
        let klen = buffer.keyexpr_len.load(Ordering::Acquire);
        let mut v = heapless::Vec::new();
        for i in 0..klen {
            let _ = v.push(buffer.keyexpr[i]);
        }
        v
    }

    /// Read the sequence number of the next-to-consume request in a slot's ring.
    fn read_service_seq(slot: usize) -> i64 {
        let buf_ref = ServiceBufferRef::new(slot);
        let b = buf_ref.get();
        let head = b.head.load(Ordering::Relaxed);
        b.ring
            .entry(head % b.ring.depth())
            .seq
            .load(Ordering::Acquire)
    }

    /// Test-only: does the slot's request ring hold an unread request?
    fn service_buf_has_request(slot: usize) -> bool {
        let buf_ref = ServiceBufferRef::new(slot);
        let b = buf_ref.get();
        b.head.load(Ordering::Acquire) != b.tail.load(Ordering::Acquire)
    }

    // ========================================================================
    // 37.1: Service buffer bug fix tests
    // ========================================================================

    #[test]
    fn service_buf_oversized_request_clears_has_request() {
        let slot = 6;
        reset_service_buffer(slot);

        let payload = [0xABu8; 512];
        simulate_service_request(slot, &payload, b"test/service");

        let mut small_buf = [0u8; 256];
        let result = take_service(slot, &mut small_buf);
        assert!(matches!(result, Err(TransportError::BufferTooSmall)));

        assert!(
            !service_buf_has_request(slot),
            "ring must be drained after BufferTooSmall to avoid stuck state"
        );

        simulate_service_request(slot, b"hello", b"test/service");
        let mut recv_buf = [0u8; 1024];
        let result = take_service(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(5))));
        assert_eq!(&recv_buf[..5], b"hello");

        reset_service_buffer(slot);
    }

    #[test]
    fn service_buf_normal_request_after_stuck_recovery() {
        let slot = 5;
        reset_service_buffer(slot);

        simulate_service_request(slot, b"first", b"svc/a");
        let mut buf = [0u8; 1024];
        let result = take_service(slot, &mut buf);
        assert!(matches!(result, Ok(Some(5))));
        assert_eq!(&buf[..5], b"first");

        let result = take_service(slot, &mut buf);
        assert!(matches!(result, Ok(None)));

        simulate_service_request(slot, b"second", b"svc/a");
        let result = take_service(slot, &mut buf);
        assert!(matches!(result, Ok(Some(6))));
        assert_eq!(&buf[..6], b"second");

        reset_service_buffer(slot);
    }

    // ========================================================================
    // 37.1a: Service buffer state machine tests
    // ========================================================================

    #[test]
    fn svc_buf_idle_poll() {
        let slot = 0;
        reset_service_buffer(slot);

        let mut buf = [0u8; 1024];
        let result = take_service(slot, &mut buf);
        assert!(matches!(result, Ok(None)));

        assert!(!service_buf_has_request(slot));
    }

    #[test]
    fn svc_buf_normal_request() {
        let slot = 1;
        reset_service_buffer(slot);

        simulate_service_request(slot, b"request_data", b"svc/test");

        assert!(service_buf_has_request(slot));

        let mut recv_buf = [0u8; 1024];
        let result = take_service(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(12))));
        assert_eq!(&recv_buf[..12], b"request_data");

        assert!(!service_buf_has_request(slot));
    }

    #[test]
    fn svc_buf_max_payload() {
        let slot = 2;
        reset_service_buffer(slot);

        // Exactly 1024 bytes = max capacity
        let payload = [0xCCu8; 1024];
        simulate_service_request(slot, &payload, b"svc/big");

        let mut recv_buf = [0u8; 1024];
        let result = take_service(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(1024))));
        assert_eq!(&recv_buf, &payload);
    }

    #[test]
    fn svc_buf_caller_too_small_recovery() {
        let slot = 3;
        reset_service_buffer(slot);

        // Store 512 bytes, receive into 256-byte buffer
        let payload = [0xDDu8; 512];
        simulate_service_request(slot, &payload, b"svc/test");

        let mut small_buf = [0u8; 256];
        let result = take_service(slot, &mut small_buf);
        assert!(matches!(result, Err(TransportError::BufferTooSmall)));

        // Oversized head entry dropped → ring drained.
        assert!(!service_buf_has_request(slot));

        // Next request accepted
        simulate_service_request(slot, b"ok", b"svc/test");
        let mut recv_buf = [0u8; 1024];
        let result = take_service(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(2))));
        assert_eq!(&recv_buf[..2], b"ok");
    }

    #[test]
    fn svc_buf_ring_buffers_unread() {
        // Phase 237 follow-up — two requests arriving before a drain are both
        // buffered in the ring (in order), not overwritten (the old single-buffer
        // behaviour dropped the first).
        let slot = 4;
        reset_service_buffer(slot);

        simulate_service_request(slot, b"first_req", b"svc/a");
        simulate_service_request(slot, b"second_req", b"svc/a");

        let mut recv_buf = [0u8; 1024];
        let r1 = take_service(slot, &mut recv_buf);
        assert!(matches!(r1, Ok(Some(9))));
        assert_eq!(&recv_buf[..9], b"first_req");

        let r2 = take_service(slot, &mut recv_buf);
        assert!(matches!(r2, Ok(Some(10))));
        assert_eq!(&recv_buf[..10], b"second_req");

        assert!(!service_buf_has_request(slot));
    }

    #[test]
    fn svc_buf_double_consume() {
        let slot = 0;
        reset_service_buffer(slot);

        simulate_service_request(slot, b"once", b"svc/a");

        let mut recv_buf = [0u8; 1024];
        let result = take_service(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(4))));

        let result = take_service(slot, &mut recv_buf);
        assert!(matches!(result, Ok(None)));
    }

    #[test]
    fn svc_buf_sequence_numbers() {
        let slot = 7;
        reset_service_buffer(slot);

        // Three sequential requests — sequence numbers should increment
        simulate_service_request(slot, b"r1", b"svc/a");
        let seq1 = read_service_seq(slot);

        // Consume before next request
        let mut buf = [0u8; 1024];
        let _ = take_service(slot, &mut buf);

        simulate_service_request(slot, b"r2", b"svc/a");
        let seq2 = read_service_seq(slot);
        let _ = take_service(slot, &mut buf);

        simulate_service_request(slot, b"r3", b"svc/a");
        let seq3 = read_service_seq(slot);
        let _ = take_service(slot, &mut buf);

        assert!(seq2 > seq1, "seq2 ({seq2}) should be > seq1 ({seq1})");
        assert!(seq3 > seq2, "seq3 ({seq3}) should be > seq2 ({seq2})");
    }

    #[test]
    fn svc_buf_keyexpr_preserved() {
        let slot = 1;
        reset_service_buffer(slot);

        let keyexpr = b"0/my_service/example_interfaces::srv::dds_::AddTwoInts/Reply";
        simulate_service_request(slot, b"payload", keyexpr);

        let stored = read_service_keyexpr(slot);
        assert_eq!(stored.as_slice(), keyexpr);

        // Consume and verify keyexpr was available during request
        let mut recv_buf = [0u8; 1024];
        let result = take_service(slot, &mut recv_buf);
        assert!(matches!(result, Ok(Some(7))));
    }

    #[test]
    fn svc_buf_all_slots_independent() {
        let slot_a = 0;
        let slot_b = 7;
        reset_service_buffer(slot_a);
        reset_service_buffer(slot_b);

        simulate_service_request(slot_a, b"req_zero", b"svc/0");
        simulate_service_request(slot_b, b"req_seven", b"svc/7");

        // Consume slot_b first
        let mut recv_buf = [0u8; 1024];
        let result = take_service(slot_b, &mut recv_buf);
        assert!(matches!(result, Ok(Some(9))));
        assert_eq!(&recv_buf[..9], b"req_seven");

        // slot_a still has its request
        assert!(service_buf_has_request(slot_a));

        let result = take_service(slot_a, &mut recv_buf);
        assert!(matches!(result, Ok(Some(8))));
        assert_eq!(&recv_buf[..8], b"req_zero");
    }

    // ========================================================================
    // phase-461 W1: the ring is a header over caller-visible storage
    // ========================================================================

    /// A family that knows its bound brings its own storage. 300-byte slots,
    /// two deep: neither number is a shim default, so a read that lands here
    /// used the caller's geometry and not the crate's.
    static CALLER_STORAGE: InboxStorage<300, 2> = InboxStorage::new();
    static CALLER_RING: InboxRing = InboxRing::over(&CALLER_STORAGE);

    /// The header slot the caller-ring test binds: the last one, above every
    /// slot the older tests in this module and in `shim/mod.rs` use (0..=7).
    const CALLER_SLOT: usize = SERVICE_BUFFER_COUNT - 1;

    #[test]
    fn an_inbox_ring_carries_the_callers_geometry() {
        assert!(CALLER_RING.is_bound());
        assert_eq!(CALLER_RING.slot_bytes(), 300);
        assert_eq!(CALLER_RING.depth(), 2);
        assert!(CALLER_RING.is_over(&CALLER_STORAGE));
        assert_eq!(CALLER_RING.storage_bytes(), InboxStorage::<300, 2>::BYTES);
        assert_eq!(
            InboxStorage::<300, 2>::BYTES,
            2 * (300 + core::mem::size_of::<InboxEntry>()),
            "a ring costs depth x (slot + entry); the entry is the 12 B of len, seq, overflow"
        );
        assert_ne!(
            (SERVICE_INBOX_BYTES, SERVICE_INBOX_DEPTH),
            (300, 2),
            "the caller's geometry must differ from the shim's for the test below to mean anything"
        );
    }

    #[test]
    fn a_caller_ring_receives_at_its_own_depth_and_slot_size() {
        // A const block, because the condition is a constant: clippy's
        // `assertions_on_constants` refuses the runtime form, and the compile-time
        // one is what this check wanted anyway. The interpolated value goes with
        // it -- a const panic message cannot carry format arguments.
        const {
            assert!(
                CALLER_SLOT >= 8,
                "CALLER_SLOT (SERVICE_BUFFER_COUNT - 1) collides with the slots the older tests use"
            )
        };
        bind_caller_ring_for_test(CALLER_SLOT, &CALLER_RING);
        reset_service_buffer(CALLER_SLOT);
        assert!(
            ServiceBufferRef::new(CALLER_SLOT)
                .get()
                .ring
                .is_over(&CALLER_STORAGE),
            "reset must keep a caller binding"
        );

        // Slot size is the caller's: 300 bytes land, 301 overflow.
        let fits = [0x5Au8; 300];
        simulate_service_request(CALLER_SLOT, &fits, b"svc/caller");
        let mut recv = [0u8; 512];
        assert!(matches!(
            take_service(CALLER_SLOT, &mut recv),
            Ok(Some(300))
        ));
        assert_eq!(&recv[..300], &fits[..]);

        let too_big = [0xA5u8; 301];
        simulate_service_request(CALLER_SLOT, &too_big, b"svc/caller");
        assert!(matches!(
            take_service(CALLER_SLOT, &mut recv),
            Err(TransportError::MessageTooLarge)
        ));
        assert!(!service_buf_has_request(CALLER_SLOT));

        // Depth is the caller's: two are held, the third is the ring-full drop.
        simulate_service_request(CALLER_SLOT, b"one", b"svc/caller");
        simulate_service_request(CALLER_SLOT, b"two", b"svc/caller");
        simulate_service_request(CALLER_SLOT, b"three", b"svc/caller");
        assert!(matches!(take_service(CALLER_SLOT, &mut recv), Ok(Some(3))));
        assert_eq!(&recv[..3], b"one");
        assert!(matches!(take_service(CALLER_SLOT, &mut recv), Ok(Some(3))));
        assert_eq!(&recv[..3], b"two");
        assert!(matches!(take_service(CALLER_SLOT, &mut recv), Ok(None)));

        // And the bytes are the caller's: read them back off the static.
        simulate_service_request(CALLER_SLOT, b"mine", b"svc/caller");
        let head = ServiceBufferRef::new(CALLER_SLOT)
            .get()
            .head
            .load(Ordering::Acquire);
        let index = head % CALLER_RING.depth();
        // SAFETY: a test-only read of the caller's storage after the producer
        // published the slot; nothing writes it until it is taken below.
        let landed =
            unsafe { core::slice::from_raw_parts(CALLER_RING.slot_ptr(index).cast_const(), 4) };
        assert_eq!(landed, b"mine");
        let _ = take_service(CALLER_SLOT, &mut recv);
    }

    /// The shim's tables at their defaults ARE the single table this phase
    /// split: one ring per header between them, each `NROS_SERVICE_INBOX_DEPTH`
    /// x `NROS_SERVICE_INBOX_BYTES`, and the old name still spells the size.
    #[test]
    fn the_shim_tables_are_the_single_table_by_default() {
        assert_eq!(SERVICE_INBOX_DEPTH, SERVICE_REQUEST_RING_DEPTH);
        assert_eq!(
            SERVICE_INBOX_BYTES,
            crate::config::SERVICE_BUFFER_SIZE,
            "SERVICE_BUFFER_SIZE is the one-release alias of NROS_SERVICE_INBOX_BYTES"
        );
        assert_eq!(
            USER_SERVICE_INBOX_COUNT + ACTION_INBOX_COUNT,
            SERVICE_BUFFER_COUNT,
            "one ring per header, between the two tables"
        );
        let rings =
            core::mem::size_of_val(&USER_SERVICE_INBOX) + core::mem::size_of_val(&ACTION_INBOX);
        let user = USER_SERVICE_INBOX_COUNT
            * InboxStorage::<SERVICE_INBOX_BYTES, SERVICE_INBOX_DEPTH>::BYTES;
        let action =
            ACTION_INBOX_COUNT * InboxStorage::<ACTION_INBOX_BYTES, ACTION_INBOX_DEPTH>::BYTES;
        assert_eq!(
            rings,
            user + action,
            "the tables are priced by their own formula"
        );
        // This test build states no knob and declares nothing, so the crate
        // defaults are in force: 4 x 1024 per queryable, every queryable, and
        // an empty action table.
        assert_eq!(
            (
                SERVICE_INBOX_BYTES,
                SERVICE_INBOX_DEPTH,
                ACTION_INBOX_QUERYABLES
            ),
            (1024, 4, 0),
            "the crate defaults are today's single table (set a knob and this test is not the gate)"
        );
        assert_eq!(ACTION_INBOX_COUNT, 0);
        assert_eq!(
            rings,
            SERVICE_BUFFER_COUNT * 4 * (1024 + core::mem::size_of::<InboxEntry>()),
            "the ring bytes are exactly what the inline [ServiceRequestSlot; 4] arrays held"
        );
        // The per-queryable header no longer embeds a ring.
        assert!(core::mem::size_of::<ServiceBuffer>() < SERVICE_INBOX_BYTES);
    }

    /// phase-461 W2b -- a parameter- or lifecycle-named queryable selects the
    /// builtin table, and a look-alike user service does not.
    ///
    /// The eleven are matched as a whole last segment under a node FQN. The
    /// negatives are the mis-detections that a `contains` would wave through:
    /// a longer name with the endpoint as a prefix or a suffix, the endpoint
    /// as an inner segment, and the bare endpoint with no node in front.
    #[test]
    fn the_builtin_endpoints_are_matched_whole_and_under_a_node() {
        // `heapless` and not `format!`: this crate is `no_std` and its test
        // build has no allocator either.
        fn name(parts: &[&str]) -> heapless::String<128> {
            let mut s = heapless::String::new();
            for p in parts {
                s.push_str(p).expect("service name fits 128 bytes");
            }
            s
        }
        for endpoint in BUILTIN_SERVICE_ENDPOINTS {
            let under_node = name(&["/island/planner/", endpoint]);
            assert!(
                is_builtin_service(&under_node),
                "{endpoint} under a node FQN is a builtin service"
            );
            assert!(
                matches!(inbox_for(&under_node), InboxSpec::Builtin),
                "{endpoint} selects the builtin table"
            );
            assert!(
                is_builtin_service(&name(&["/planner/", endpoint])),
                "{endpoint} under a node with no namespace is a builtin service"
            );
            // The look-alikes a `contains` would wave through.
            for parts in [
                &["/island/planner/", endpoint, "_v2"][..],
                &["/island/planner/my_", endpoint][..],
                &["/island/planner/", endpoint, "/extra"][..],
                &["/", endpoint][..],
                &[endpoint][..],
            ] {
                let n = name(parts);
                assert!(
                    !is_builtin_service(&n),
                    "{n} is a user service, not a builtin one"
                );
                assert!(
                    matches!(inbox_for(&n), InboxSpec::UserService),
                    "{n} selects the user-service table"
                );
            }
        }
        // An action channel is neither, and is selected first.
        assert!(!is_builtin_service("/island/dock/_action/send_goal"));
        assert!(matches!(
            inbox_for("/island/dock/_action/send_goal"),
            InboxSpec::Action
        ));
    }

    /// The capability half of the same guard: the builtin TABLE is empty
    /// unless the image's own declaration left slots to the runtime, so on a
    /// build that declares nothing -- this one -- a parameter-named queryable
    /// draws exactly the ring it draws today.
    #[test]
    fn an_undeclared_image_has_no_builtin_table() {
        assert_eq!(
            DECLARED_APP_QUERYABLES,
            usize::MAX,
            "this test build declares no application service surface"
        );
        assert_eq!(BUILTIN_INBOX_PER_SESSION, 0);
        assert_eq!(BUILTIN_INBOX_COUNT, 0);
        assert_eq!(core::mem::size_of_val(&BUILTIN_INBOX), 0);
        assert_eq!(
            USER_SERVICE_INBOX_PER_SESSION + ACTION_INBOX_PER_SESSION,
            ZPICO_MAX_QUERYABLES,
            "with no builtin share the partition is W1's, byte for byte"
        );
        let (ring, from) = draw_shim_ring(0, ShimFamily::Builtin).expect("a user ring is spare");
        assert_eq!(from, ShimFamily::UserService);
        assert_eq!(
            (ring.slot_bytes(), ring.depth()),
            (SERVICE_INBOX_BYTES, SERVICE_INBOX_DEPTH)
        );
        release_shim_ring(0, from);
        assert_eq!(NEXT_USER_SERVICE_INBOX[0].load(Ordering::SeqCst), 0);
        assert_eq!(NEXT_BUILTIN_INBOX[0].load(Ordering::SeqCst), 0);
    }

    /// The three tables still hold exactly one ring per header, which is what
    /// makes the fallback above total.
    #[test]
    fn the_three_tables_hold_one_ring_per_header() {
        assert_eq!(
            USER_SERVICE_INBOX_COUNT + ACTION_INBOX_COUNT + BUILTIN_INBOX_COUNT,
            SERVICE_BUFFER_COUNT
        );
    }

    /// On an image that declares no action server the action table is empty,
    /// and an action queryable draws a user ring: today's behaviour, byte for
    /// byte, until W3 prices the families apart.
    #[test]
    fn an_action_queryable_draws_a_user_ring_when_its_table_is_empty() {
        assert_eq!(
            ACTION_INBOX_PER_SESSION, 0,
            "this test build declares no action server"
        );
        let (ring, from) = draw_shim_ring(0, ShimFamily::Action).expect("a user ring is spare");
        assert_eq!(from, ShimFamily::UserService);
        assert_eq!(
            (ring.slot_bytes(), ring.depth()),
            (SERVICE_INBOX_BYTES, SERVICE_INBOX_DEPTH)
        );
        release_shim_ring(0, from);
        assert_eq!(NEXT_USER_SERVICE_INBOX[0].load(Ordering::SeqCst), 0);
        assert_eq!(NEXT_ACTION_INBOX[0].load(Ordering::SeqCst), 0);
    }
}

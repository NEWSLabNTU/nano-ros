//! The parameter STORE's executor-side state (issue 1897).
//!
//! What every image with parameters holds, whether or not it serves them:
//! the [`ParameterServer`] over the executor's carved slots, argv overrides,
//! the `use_sim_time` placeholders, and the carved home they live in. The six
//! `rcl_interfaces` services, their buffers and `/parameter_events` are
//! `parameter_services`', and are fields here only under `param-services`.
//!
//! Split out of `parameter_services` so a `param-store` build compiles none of
//! the service code. Before the split the store and the servers shared one
//! cargo feature, so a store-only image linked the servers dormant.

#[cfg(feature = "alloc")]
extern crate alloc;
#[cfg(feature = "alloc")]
use alloc::boxed::Box;

use core::mem::MaybeUninit;

use nros_params::ParameterServer;

#[cfg(any(feature = "param-services", feature = "sim-time"))]
use crate::param_sizing::MAX_SERVICE_SETS;
#[cfg(feature = "param-services")]
use crate::parameter_services::{
    ParamServiceBuffers, ParamServiceReconcileFailure, ParameterServiceServers,
};
#[cfg(feature = "param-services")]
use nros_params::ParameterValue as InternalValue;

/// Issue 1812 — where a piece of the parameter services' state lives.
///
/// `Carved` is memory the executor's backing set aside for it (the same
/// mechanism phase-382 W3' uses for the store's slots); `Heap` is the `alloc`
/// fallback for an executor whose sizing carves none. Without `alloc` there is
/// no `Heap`, and an executor with no carved home refuses parameters instead
/// (see `Executor::ensure_parameter_store`).
///
/// A carved value is DROPPED IN PLACE when this goes, exactly as a `Box`
/// would drop it, and never freed: the backing is the caller's. That matters
/// for [`ParamState`], whose service sets own backend handles.
pub(crate) enum Placed<'s, T: ?Sized> {
    /// Borrowed from the executor backing for the executor's lifetime.
    Carved(&'s mut T),
    /// One heap allocation (the `alloc` fallback).
    #[cfg(feature = "alloc")]
    Heap(Box<T>),
}

impl<T: ?Sized> core::ops::Deref for Placed<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        match self {
            Placed::Carved(r) => r,
            #[cfg(feature = "alloc")]
            Placed::Heap(b) => b,
        }
    }
}

impl<T: ?Sized> core::ops::DerefMut for Placed<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        match self {
            Placed::Carved(r) => r,
            #[cfg(feature = "alloc")]
            Placed::Heap(b) => b,
        }
    }
}

impl<T: ?Sized> Drop for Placed<'_, T> {
    fn drop(&mut self) {
        #[allow(irrefutable_let_patterns)] // only one arm without `alloc`
        if let Placed::Carved(r) = self {
            // SAFETY: a `Carved` value was initialised in place by its maker
            // (`ParamHome::init_in`) and is reachable only through this
            // borrow, which ends here; nothing reads it after. The memory
            // itself stays the backing's.
            unsafe { core::ptr::drop_in_place::<T>(&mut **r) }
        }
    }
}

/// Holds parameter server state for the executor.
///
/// Stored outside the arena so it doesn't consume `MAX_CBS` slots.
/// One `-p` / `--params-file` override, owned — phase-482 W5.
///
/// Held in [`ParamState`] (boxed, allocated with the store) rather than in the
/// executor's inline table, so an image without parameter overrides pays
/// nothing for them.
#[cfg(feature = "alloc")]
#[derive(Debug, Clone)]
pub(crate) struct ArgvParamOverride {
    /// `Some(name)`: only the node of that NAME. `None`: every node.
    pub(crate) node: Option<alloc::string::String>,
    pub(crate) name: alloc::string::String,
    /// As written; typed against the default a node declares.
    pub(crate) value: alloc::string::String,
}

pub(crate) struct ParamState<'s> {
    /// phase-382 W2' — the server BORROWS its slot table now, so `ParamState`
    /// carries the storage's lifetime. `'s` is the executor's own borrow
    /// lifetime, which is what W3' will hand the carved table under.
    pub(crate) server: ParameterServer<'s>,
    /// phase-482 W5 — `-p` / `--params-file` overrides, in argv order; the
    /// LAST one matching a declaration wins. See
    /// `Executor::install_argv_params`. Argv is a hosted-edge input, so this
    /// rides `alloc` (issue 1812: a heap-free image has no argv to read).
    #[cfg(feature = "alloc")]
    pub(crate) argv_overrides: alloc::vec::Vec<ArgvParamOverride>,
    /// phase-426 W3 — ONE SET OF SIX PER NODE, because upstream's parameter
    /// services are per node and `ros2 param list` enumerates nodes.
    ///
    /// Issue 0745 — EMPTY until the reconcile pass attaches service servers.
    /// The STORE half exists earlier: launch-param seeding
    /// (`declare_parameter`) runs BEFORE any node is constructed — service
    /// servers cannot be created that early, but initial values must be, or
    /// ctor-read parameters silently use compiled defaults.
    ///
    /// Bounded by [`MAX_SERVICE_SETS`] rather than heap-grown: the
    /// executor's node table is a fixed arena, so the number of sets is
    /// already bounded and a `Vec` would only hide where.
    ///
    /// Issue 1812 — held INLINE, no longer one `Box<dyn …>` per set. There
    /// was only ever one implementation, and `ParamState` itself is carved
    /// from the executor backing (or, with `alloc`, one allocation), so the
    /// sets cost the backing exactly what the boxes cost the heap.
    ///
    /// Present only with `param-services` (issue 1897).
    #[cfg(feature = "param-services")]
    pub(crate) services: heapless::Vec<ParameterServiceServers, MAX_SERVICE_SETS>,
    /// phase-426 W3 — whether a caller asked for parameter services at all.
    ///
    /// Registration cannot happen where the request is made. `nros::main!`
    /// emits `apply_param_services` BEFORE its per-node `register` calls by
    /// design (the store must exist when each cell captures it), so at request
    /// time the node table is empty and there is no FQN to publish under. The
    /// request is therefore recorded here and satisfied by
    /// `Executor::reconcile_parameter_services` once the nodes exist — the
    /// same shape `reconcile_ros_time_source` uses for the same reason.
    #[cfg(feature = "param-services")]
    pub(crate) requested: bool,
    /// Issue 1270 -- the ONE request/reply buffer pair every set in
    /// [`ParamState::services`] is answered through. On the heap fallback it
    /// is `None` until the first set is built, so an image that declares
    /// parameters and serves none pays nothing for it; a carved home
    /// (issue 1812) hands it over with the state.
    #[cfg(feature = "param-services")]
    pub(crate) buffers: Option<ParamServiceBuffers<'s>>,
    /// RFC-0105 D10.2 / issue 1812 — the ONE slot a remote set decodes its
    /// incoming value into and applies it from
    /// (`ParameterServer::apply_staged`). A `ParameterValue` is ~8.5 KiB at
    /// the default bounds, and it used to be a stack temporary of every
    /// `set_parameters` request, twice over. One per executor: the six
    /// services of every node are answered serially, so no two requests are
    /// ever in flight at once. Its content between requests is scratch.
    #[cfg(feature = "param-services")]
    pub(crate) staged: InternalValue,
    /// phase-491 W3 (RFC-0105 D8) — the ONE `/parameter_events` publisher,
    /// created when the first node asks for events
    /// (`Executor::enable_parameter_events_on`).
    ///
    /// One per executor, not one per node: the topic is global and every
    /// event names its node in its own `node` field, so a subscriber reads
    /// the same messages either way, and one handle is what an RTOS image can
    /// afford beside the store (a handle per node slot is an array of them in
    /// every image that carves a store). What differs is the graph: the
    /// publisher is announced under the node that enabled it first.
    #[cfg(feature = "param-services")]
    pub(crate) event_publisher: Option<crate::session::RmwPublisher>,
    /// Issue 1271 -- whether a failed reconcile has already been logged.
    ///
    /// The spin retries the reconcile every pass while a node lacks its six,
    /// and its result used to be discarded: on Cyclone it fails with
    /// `Transport(Unsupported)` on every spin (issue 1268) and nothing on the
    /// image said so. One line per run of failures; cleared once every node
    /// is served, so a later failure is news again.
    #[cfg(feature = "param-services")]
    pub(crate) reconcile_failure_reported: bool,
    /// Issue 1268 -- WHICH node and WHICH service the last reconcile failed on,
    /// and whether that failure can change.
    ///
    /// The 1271 log said only "could not be created ({:?})", which names
    /// neither, so the reader could not tell a name too long for one node from
    /// a backend that will never serve any of the six. Both halves of the
    /// answer are known at the `create_service` call and nowhere above it, so
    /// they are recorded here as the call fails.
    #[cfg(feature = "param-services")]
    pub(crate) reconcile_failure: Option<ParamServiceReconcileFailure>,
    /// phase-430 W2 — for each node key, whether that node's `use_sim_time`
    /// entry is the AUTO-DECLARED `false` the executor seeded (as rclcpp does
    /// in every node constructor) rather than a value the application named.
    ///
    /// A seed is a PLACEHOLDER, and that is load-bearing: the store refuses a
    /// duplicate name, so without this the auto-declare would turn an app's own
    /// `declare_parameter("use_sim_time", Bool(true))` into a refusal and the
    /// default would silently beat the app.
    /// `Executor::yield_seeded_use_sim_time` steps the placeholder aside for
    /// the first real declaration, after which a second declaration of the same
    /// name is refused exactly as it was before W2.
    ///
    /// PER NODE, because the store is keyed by node and the six services are
    /// published per node: an app that declares `use_sim_time` on one of an
    /// image's nodes has said nothing about the others, whose defaults must
    /// stay placeholders.
    ///
    /// It lives HERE rather than beside `sim_time_stated` on the executor
    /// because it is a fact about the STORE's contents, and because a table
    /// that scales with a knob does not belong in the `Executor` value —
    /// `the_executor_value_does_not_scale_with_the_knobs` (issue 0961) is the
    /// test that says so. `ParamState` is boxed, and the bound is the one
    /// [`ParamState::services`] already uses: one entry per node key.
    #[cfg(feature = "sim-time")]
    pub(crate) sim_time_seeded: [bool; MAX_SERVICE_SETS],
    /// Issue 1203 — a node whose `use_sim_time` was handed to a
    /// `ParameterBuilder`. The builder declares through its own borrow of the
    /// store, so the executor learns what it declared only on the next spin,
    /// which reads the store back (`Executor::settle_builder_reserved_parameters`).
    #[cfg(feature = "sim-time")]
    pub(crate) sim_time_via_builder: [bool; MAX_SERVICE_SETS],
}

impl<'s> ParamState<'s> {
    /// Initialise a `ParamState` IN PLACE at `dst`, one field at a time.
    ///
    /// Issue 1812 — the ONE constructor, for both homes. `ParamState` now
    /// holds its service sets inline and the ~8.5 KiB staging slot, so it is
    /// no longer "tens of bytes" and must never be built by value: Rust has no
    /// placement-new, so `Box::new(ParamState { … })` would materialise it on
    /// the caller's stack first (issue 0756's shape, one type over).
    ///
    /// # Safety
    /// `dst` must be non-null, aligned and valid for writes of one
    /// `ParamState`; its prior content is ignored (not dropped). On return it
    /// is fully initialised.
    pub(crate) unsafe fn init_at(
        dst: *mut Self,
        server: ParameterServer<'s>,
        #[cfg(feature = "param-services")] buffers: Option<ParamServiceBuffers<'s>>,
    ) {
        use core::ptr::addr_of_mut;
        // SAFETY: the caller's contract on `dst`; each field is written exactly
        // once through its own place, so nothing reads uninitialised memory.
        unsafe {
            addr_of_mut!((*dst).server).write(server);
            #[cfg(feature = "alloc")]
            addr_of_mut!((*dst).argv_overrides).write(alloc::vec::Vec::new());
            // phase-426 W3 — the sets attach in `reconcile_parameter_services`,
            // one per node, once the node table is populated.
            #[cfg(feature = "param-services")]
            {
                addr_of_mut!((*dst).services).write(heapless::Vec::new());
                addr_of_mut!((*dst).requested).write(false);
                addr_of_mut!((*dst).buffers).write(buffers);
                addr_of_mut!((*dst).staged).write(InternalValue::NotSet);
                addr_of_mut!((*dst).event_publisher).write(None);
                addr_of_mut!((*dst).reconcile_failure_reported).write(false);
                addr_of_mut!((*dst).reconcile_failure).write(None);
            }
            // phase-430 W2 — no node has been auto-declared yet; the caller
            // (`ensure_parameter_store`) seeds PRIMARY immediately after.
            #[cfg(feature = "sim-time")]
            addr_of_mut!((*dst).sim_time_seeded).write([false; MAX_SERVICE_SETS]);
            #[cfg(feature = "sim-time")]
            addr_of_mut!((*dst).sim_time_via_builder).write([false; MAX_SERVICE_SETS]);
        }
    }

    /// `ParamState` on the heap — the `alloc` fallback for an executor whose
    /// backing carves no [`ParamHome`]. One fallible allocation, initialised
    /// through the allocation ([`Self::init_at`]); `None` when the heap
    /// refuses it (issue 1551's rule: an exhausted heap is an error the caller
    /// returns, not an abort).
    #[cfg(feature = "alloc")]
    pub(crate) fn try_boxed(server: ParameterServer<'s>) -> Option<Placed<'s, Self>> {
        let mut uninit = nros_rmw::fallible::try_box_uninit::<Self>()?;
        // SAFETY: `try_box_uninit` gives a correctly sized and aligned
        // allocation for exactly this type, and `init_at` writes every field
        // before `assume_init` observes it.
        unsafe {
            Self::init_at(
                uninit.as_mut_ptr(),
                server,
                #[cfg(feature = "param-services")]
                None,
            );
            Some(Placed::Heap(uninit.assume_init()))
        }
    }
}

/// Bytes of each carved request/reply buffer: the build's resolved
/// `param_service_buffer_bytes()` (a stated `NROS_PARAM_SERVICE_BUFFER_SIZE`,
/// or the bound the contract's declared parameters derive).
///
/// Issue 1897 -- ZERO in a build without `param-services`: nothing there reads
/// the pair, because the six services and the `/parameter_events` publisher,
/// its only users, are not compiled. The feature is the one statement of
/// whether an image serves its store (it replaced issue 1856's
/// `NROS_CAPABILITY_PARAM_SERVICES=0` and phase-461 W6's descriptor read).
#[cfg(feature = "param-services")]
pub(crate) const HOME_BUFFER_BYTES: usize = crate::param_sizing::param_service_buffer_bytes();
#[cfg(not(feature = "param-services"))]
pub(crate) const HOME_BUFFER_BYTES: usize = 0;

/// Issue 1812 — everything the parameter SERVICES own, as one region of the
/// executor backing.
///
/// Carved next to the store's slots and on the same condition (the sizing
/// carves a store), so an image with no store pays nothing and an image with
/// one asks the heap for nothing: the state with its inline service sets and
/// staging slot (items 1 and 3 of the issue, and RFC-0105 D10.2), and the
/// shared buffer pair (item 4). Handed over UNINITIALISED, like the slots;
/// [`ParamHome::init_in`] is the one way to turn it into a state.
///
/// What the region costs is `size_of::<ParamHome>()`, which the backing's
/// layout reads directly (`NATIVE_UNITS.param_home`), so no number here can
/// drift from the carve.
///
/// Issue 1856 -- the services' half (the sets and the buffer pair) exists only
/// in a build with `param-services` (issue 1897: the cargo feature, not a
/// knob, says so). A store-only build carves a home of the state alone:
/// 24,744 -> 1,472 B on the safety island (thumbv7em, 22 slots, four node
/// slots), measured when a knob made that choice.
#[repr(C)]
pub(crate) struct ParamHome {
    state: MaybeUninit<ParamState<'static>>,
    request: [MaybeUninit<u8>; HOME_BUFFER_BYTES],
    reply: [MaybeUninit<u8>; HOME_BUFFER_BYTES],
}

impl ParamHome {
    /// Initialise the home IN PLACE around `server` and borrow its state.
    ///
    /// The buffers are zeroed (a `&mut [u8]` must be initialised) and handed
    /// to the state, which owns them for the executor's lifetime; the state
    /// is built through [`ParamState::init_at`]. Nothing here is a by-value
    /// temporary of the region.
    pub(crate) fn init_in<'s>(
        home: &'s mut MaybeUninit<ParamHome>,
        server: ParameterServer<'s>,
    ) -> Placed<'s, ParamState<'s>> {
        use core::ptr::addr_of_mut;
        let base = home.as_mut_ptr();
        // SAFETY: `base` is the home's own, aligned, exclusive pointer for
        // `'s`. The three places below are disjoint fields of it, so the three
        // borrows built from them never alias; each is initialised before it
        // is borrowed. `ParamState<'static>` and `ParamState<'s>` differ only
        // in a lifetime, so the cast is layout-identical.
        unsafe {
            #[cfg(feature = "param-services")]
            let buffers = {
                let request = addr_of_mut!((*base).request).cast::<u8>();
                let reply = addr_of_mut!((*base).reply).cast::<u8>();
                core::ptr::write_bytes(request, 0, HOME_BUFFER_BYTES);
                core::ptr::write_bytes(reply, 0, HOME_BUFFER_BYTES);
                ParamServiceBuffers::carved(
                    core::slice::from_raw_parts_mut(request, HOME_BUFFER_BYTES),
                    core::slice::from_raw_parts_mut(reply, HOME_BUFFER_BYTES),
                )
            };
            let state = addr_of_mut!((*base).state).cast::<ParamState<'s>>();
            ParamState::init_at(
                state,
                server,
                #[cfg(feature = "param-services")]
                Some(buffers),
            );
            Placed::Carved(&mut *state)
        }
    }
}

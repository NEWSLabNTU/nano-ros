//! What every `nros_cpp_*_destroy` FFI ACTUALLY drops — issue 1496.
//!
//! Each of those functions is a null guard followed by one
//! `core::ptr::drop_in_place`, which reads as a release and is one only when the
//! type it names has something to run. Issue 1496 is what it costs when it does
//! not: `nros_cpp_action_server_destroy` drops a struct whose every field is
//! `Copy` or a raw pointer, so it runs no destructor — while the five RMW
//! entities, the goal table and the result slab it appears to release live in
//! an `ActionServerRawArenaEntry` in the executor arena. When this table was
//! written the arena had no removal path; since `9768795b1d` (issue 1496) both
//! ACTION entries are released through the executor — the client's inside
//! `nros_cpp_action_client_destroy` before its `drop_in_place`, the server's in
//! `nros_cpp_action_server_detach`, which `~Server` calls before
//! `nros_cpp_action_server_destroy`. So for the action rows `NO_OP` describes
//! the DROP, not the entity's fate. The guard-condition row is still a no-op
//! destroy (issue 1667).
//!
//! WHY A TABLE, AND WHY THE COMPILER ANSWERS IT
//!
//! "Is this `drop_in_place` a no-op?" is a question about the TYPE, and no
//! source scan can answer it: `CppActionServer`'s fields are three function
//! pointers, a `Copy` handle and a QoS block, and reading that off the struct
//! means re-implementing drop-glue analysis in a checker. `needs_drop` is the
//! compiler's own answer, so each row below states the classification and the
//! assert holds the compiler to it. A type that GAINS a `Drop` (an RMW handle
//! that starts undeclaring its entity, say) fails the build here rather than
//! leaving a doc comment quietly wrong.
//!
//! The other half of the rule is coverage, which a Rust file cannot state:
//! nothing here forces a NEW `*_destroy` to appear in this table.
//! `check-cpp-destroy-shape` reads both sides — every destroy function in this
//! crate must have a row, every row must name a real function and the type it
//! drops, and a `NO_OP` row's function must SAY SO in its doc comment, naming
//! issue 1496. That is resolution 3 of the issue: the class, not the site.
//!
//! Adding a row is not a way to bless a no-op. It records which one it is; if
//! the answer is `NO_OP`, the function's documentation has to carry the
//! consequence for whoever calls it.

use crate::{
    action::{CppActionClient, CppActionServer, PollingActionClientCore, PollingActionServerCore},
    publisher::CppPublisher,
};

/// The `drop_in_place` runs drop glue: dropping this type releases something.
const RELEASES: bool = true;

/// The `drop_in_place` runs nothing at all. Whatever the entity owns is
/// elsewhere — for the three below, in the executor arena. That is a statement
/// about the DROP: both action entries are now released through the executor
/// (`9768795b1d` — the client in its destroy, the server in the detach `~Server`
/// calls first), and only the guard condition's entry is never released (issue
/// 1667).
const NO_OP: bool = false;

/// phase-476 W0 — the destroy drops NO storage: its argument is a plain handle
/// (no drop glue, which the assert holds), and what it releases are arena
/// entries, through the executor. `check-cpp-destroy-shape` requires such a
/// function to have no `drop_in_place` and to say so in its doc.
const EXECUTOR: bool = false;

macro_rules! destroy_shapes {
    ($($func:ident drops $ty:ty => $shape:ident;)+) => {
        $(
            const _: () = assert!(
                core::mem::needs_drop::<$ty>() == $shape,
                concat!(
                    "issue 1496: `", stringify!($func), "` is classified `",
                    stringify!($shape), "` in destroy_shape.rs, and \
                     `needs_drop::<", stringify!($ty), ">()` now disagrees. \
                     Move the row, and move the FFI function's doc comment with \
                     it — a `NO_OP` must say it releases nothing, and a \
                     `RELEASES` must not."
                ),
            );
        )+
    };
}

destroy_shapes! {
    // The three arena-registered entities. Their state is an arena entry the
    // bump allocator never reclaims; the storage these drop is a handle.
    nros_cpp_action_server_destroy drops CppActionServer => NO_OP;
    nros_cpp_action_client_destroy drops CppActionClient => NO_OP;
    // `nros_node::GuardCondition` is `{ &'static AtomicBool, Option<fn>, *mut
    // c_void }` — the flag lives in the arena and the registered closure entry
    // stays there for the executor's lifetime. Unlike the two above, the arena
    // entry does NOT hold the destroyed object's address: the context it keeps
    // is the caller's own (`nros_cpp_guard_condition_create` takes a C callback
    // plus a user context), so there is no dangling-context arm to fix here
    // today. `GuardCondition::closure_` in the C++ header would create one —
    // it is freed by the destructor and nothing in the tree attaches a block
    // to a guard condition yet.
    nros_cpp_guard_condition_destroy drops nros_node::GuardCondition => NO_OP;

    // The entities that own their state INLINE in the caller's storage. These
    // are real releases: each RMW handle's `Drop` destroys the backend entity.
    nros_cpp_publisher_destroy drops CppPublisher => RELEASES;
    nros_cpp_subscription_destroy drops nros::internals::RmwSubscriber => RELEASES;
    nros_cpp_service_server_destroy drops nros::internals::RmwServiceServer => RELEASES;
    nros_cpp_service_client_destroy drops nros::internals::RmwServiceClient => RELEASES;
    // The L1 POLLING action tiers, and the contrast that makes the point: they
    // hold an `ActionServerCore` / `ActionClientCore` — the same core the arena
    // holds for the callback tier — in the caller's own storage, so dropping it
    // destroys the five entities. Same API, same verb, opposite answer, and the
    // difference is WHERE the state was put.
    nros_cpp_action_server_destroy_polling drops PollingActionServerCore => RELEASES;
    nros_cpp_action_client_destroy_polling drops PollingActionClientCore => RELEASES;

    // phase-476 W0 — a node's handle is plain data; destroying it releases the
    // arena entries the node registered through a copyable handle.
    nros_cpp_node_destroy drops crate::nros_cpp_node_t => EXECUTOR;
}

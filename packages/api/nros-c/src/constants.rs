//! Shared constants for nros-c
//!
//! The canonical values live in [`nros_node::limits`]. Literals are mirrored
//! here so `cbindgen` (run with `parse_deps = false`) can emit `#define`
//! values in the generated C header without crossing crate boundaries.
//! A `const _` assertion below catches any drift between the two sites.

/// Maximum length of a zenoh locator string (e.g., "tcp/127.0.0.1:7447")
pub const MAX_LOCATOR_LEN: usize = 128;

/// Maximum length of a node name
pub const MAX_NAME_LEN: usize = 64;

/// Maximum length of a node namespace
pub const MAX_NAMESPACE_LEN: usize = 128;

/// Maximum length of an RMW backend name (e.g., "zenoh", "cyclonedds", "xrce").
///
/// Matches `BACKEND_NAME_MAX` in `nros-rmw-cffi/src/lib.rs`. Lifted to a
/// public constant here so `nros_node_options_t` can declare a fixed-size
/// buffer that round-trips through the registry without truncation.
pub const MAX_RMW_NAME_LEN: usize = 32;

/// Maximum length of a topic name
pub const MAX_TOPIC_LEN: usize = 256;

/// Maximum length of a service name
pub const MAX_SERVICE_NAME_LEN: usize = 256;

/// Maximum length of an action name
pub const MAX_ACTION_NAME_LEN: usize = 256;

/// Maximum length of a type name (e.g., "std_msgs::msg::dds_::Int32_")
pub const MAX_TYPE_NAME_LEN: usize = 256;

/// Maximum length of a type hash (RIHS format)
pub const MAX_TYPE_HASH_LEN: usize = 128;

/// Maximum number of concurrent goals per action server.
///
/// This is a fixed constant (not configurable via env var) because it
/// affects `nros_action_server_t` struct layout. Changing it requires
/// rebuilding the runtime and the consuming application together.
pub const NROS_MAX_CONCURRENT_GOALS: usize = 4;

/// Inline storage (in `u64`) for `nros_lifecycle_state_machine_t`.
///
/// `core::mem::size_of`, no longer a hand-coded upper bound. The C-side
/// counterpart `NROS_LIFECYCLE_CTX_SIZE` (in `nros_config_generated.h`) is
/// the same value in bytes.
pub const NROS_LIFECYCLE_CTX_OPAQUE_U64S: usize =
    core::mem::size_of::<nros_node::lifecycle::LifecyclePollingNodeCtx>().div_ceil(8);

// Compile-time drift check: these literals must match the canonical values
// exported from `nros_node::limits`.
const _: () = {
    assert!(MAX_LOCATOR_LEN == nros_node::limits::MAX_LOCATOR_LEN);
    assert!(MAX_NAME_LEN == nros_node::limits::MAX_NAME_LEN);
    assert!(MAX_NAMESPACE_LEN == nros_node::limits::MAX_NAMESPACE_LEN);
    assert!(MAX_TOPIC_LEN == nros_node::limits::MAX_TOPIC_LEN);
    assert!(MAX_SERVICE_NAME_LEN == nros_node::limits::MAX_SERVICE_NAME_LEN);
    assert!(MAX_ACTION_NAME_LEN == nros_node::limits::MAX_ACTION_NAME_LEN);
    assert!(MAX_TYPE_NAME_LEN == nros_node::limits::MAX_TYPE_NAME_LEN);
    assert!(MAX_TYPE_HASH_LEN == nros_node::limits::MAX_TYPE_HASH_LEN);
    assert!(NROS_MAX_CONCURRENT_GOALS == nros_node::limits::MAX_CONCURRENT_GOALS);
};

// ── Inline opaque storage sizes ─────────────────────────────────────────
//
// Computed from `core::mem::size_of` at compile time — always matches the
// actual Rust type layout. See `opaque_sizes.rs`.
pub use crate::opaque_sizes::{
    GUARD_HANDLE_OPAQUE_U64S, PUBLISHER_OPAQUE_U64S, SESSION_OPAQUE_U64S,
};

// ── Serialization format (RFC-0088 D5) ──────────────────────────────────
//
// The format the linked backend speaks, as a `u8` discriminant and as its
// cross-image identity string. `cbindgen` lowers the DISCRIMINANT into
// `nros_generated.h` as `#define NROS_SERIALIZATION_FORMAT_ID`; the string is
// NOT lowered (cbindgen maps no Rust `&str` to a C constant), so
// `nros/serialization_format.h` derives `NROS_SERIALIZATION_FORMAT` from the
// discriminant instead of carrying a second authored spelling. Those macros are
// what the per-message `_Static_assert` codegen emits compares against, and what
// `nros/serialization_format.hpp` lifts into `nros::SerializationFormat`.
//
// **Only meaningful in a single-backend image.** A bridge image links two
// backends and has no single answer; it asks each session instead
// (`nros_node_get_serialization_format()`). `check-format-macro-scope` refuses a
// bridge-linked image that references either macro.
//
// The literals are MIRRORED, not authored: they exist so `cbindgen` — which
// runs with `parse_deps = false` and evaluates no expressions — can emit a C
// constant at all. The `const _` below is what makes them true: it compares
// each against `nros_node::session::IMAGE_SERIALIZATION_FORMAT{,_ID}`, the same
// constant the Rust API's `format_check` asserts on, so a backend whose format
// is not CDR fails this crate's build with a message naming the drift rather
// than shipping a header that quietly disagrees with the linked backend.
// (Same contract as the `MAX_*` mirrors above; see the module docs.)

/// Image-local discriminant of the linked backend's serialization format
/// (`nros_serdes::format::SerializationFormatId`). RFC-0088 D2 — image-local:
/// never persist it, never compare it across images.
pub const NROS_SERIALIZATION_FORMAT_ID: u8 = 1;

/// Cross-image identity of the linked backend's serialization format.
pub const NROS_SERIALIZATION_FORMAT: &str = "cdr";

// Compile-time drift check — the mirrors above must equal what the linked
// backend actually declares. A `const _` is evaluated by `cargo check`, so this
// fires before codegen, unlike the `const {}` inside the generic Rust entity
// creators.
//
// GATED on `rmw-cffi` (phase-421 W4 residue, fixed 2026-09-05). `nros_node::session`
// is `#[cfg(any(has_rmw, test))]`, and `has_rmw` is exactly nros-node's `rmw-cffi`
// feature (see its build.rs) — which `nros-c`'s own `rmw-cffi` forwards to. Without
// the gate this block referenced a module that does not exist in any RMW-less
// combo, so `check-workspace-features` failed to COMPILE the crate:
//
//     error[E0433]: cannot find `session` in `nros_node`
//
// There is nothing to mirror when no backend is linked, so the assert has no
// meaning in that combo rather than a different answer.
#[cfg(feature = "rmw-cffi")]
const _: () = {
    assert!(
        NROS_SERIALIZATION_FORMAT_ID == nros_node::session::IMAGE_SERIALIZATION_FORMAT_ID.as_u8(),
        "RFC-0088: NROS_SERIALIZATION_FORMAT_ID no longer matches the linked \
         backend — update the mirror (and the C/C++ headers that assert on it)"
    );
    let mirrored = NROS_SERIALIZATION_FORMAT.as_bytes();
    let linked = nros_node::session::IMAGE_SERIALIZATION_FORMAT.as_bytes();
    assert!(
        mirrored.len() == linked.len(),
        "RFC-0088: NROS_SERIALIZATION_FORMAT no longer matches the linked backend"
    );
    let mut i = 0;
    while i < mirrored.len() {
        assert!(
            mirrored[i] == linked[i],
            "RFC-0088: NROS_SERIALIZATION_FORMAT no longer matches the linked backend"
        );
        i += 1;
    }
};

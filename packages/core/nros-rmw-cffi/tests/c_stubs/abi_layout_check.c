/*
 * ABI layout single-source-of-truth — C half (issue #238 / #239).
 *
 * The `nros_rmw_*` types are a HAND-MIRRORED C ABI: the C definitions
 * in the `include/nros` headers and the Rust `#[repr(C)]` mirrors in
 * `src/lib.rs` are kept in lockstep by hand, with no codegen. These
 * `_Static_assert`s pin the C-side widths at compile time; the Rust
 * half lives in the `abi_layout` `const _: () = { assert!(...) }`
 * block in `src/lib.rs`. If a future edit changes one side's layout
 * without the other, exactly one of these two guards fails the build —
 * closing the drift class that #238 slipped through (an event-kind
 * enum that was int-sized in C but byte-sized in Rust, passed BY VALUE
 * across three vtable slots).
 *
 * Compiled (not linked into anything meaningful — the TU has no runtime
 * symbols) by build.rs under the `c-stub-test` feature.
 */

#include <nros/rmw_entity.h>
#include <nros/rmw_event.h>
#include <nros/rmw_vtable.h>

#include <stddef.h>

/* The #238 core: the event-kind enum is an UNFIXED C enum. This TU is
 * compiled HOST-side (int-enum ABI, no `-fshort-enums`), so it is
 * int-sized here — 4 bytes. It is passed by value into
 * `register_subscription_event` / `register_publisher_event` and out
 * through `nros_rmw_event_callback_t`. The Rust mirror MUST be
 * `#[repr(C)]` (tracks the C ABI per-target), never a fixed
 * `#[repr(u8)]`/`#[repr(i32)]`. On ARM EABI the same enum is 1 byte on
 * BOTH sides — that target is not checked here (see the Rust
 * `abi_layout` block, which gates its width pin to non-ARM). */
_Static_assert(sizeof(nros_rmw_event_kind_t) == 4,
               "nros_rmw_event_kind_t must be C-int-sized host-side (issue #238)");

/* QoS mirror — the by-value struct crossing the create_* slots. Must
 * match `size_of::<NrosRmwQos>() == 24` in the Rust abi_layout block.
 * (phase-301 / issue 0240: the transport hints left this struct for
 * the options structs; 28 -> 24 bytes.) */
_Static_assert(sizeof(nros_rmw_qos_t) == 24,
               "nros_rmw_qos_t size drifted from the Rust mirror (24)");

/* phase-301 options structs — NULLable trailing create_* params. */
_Static_assert(sizeof(nros_rmw_publisher_options_t) == 8,
               "nros_rmw_publisher_options_t size drifted (8)");
_Static_assert(sizeof(nros_rmw_subscription_options_t) == 8,
               "nros_rmw_subscription_options_t size drifted (8)");

/* Opaque handle structs are pointer-aligned (they carry a `void*`
 * backend_data / backend pointer). Rust mirror asserts the same via
 * `align_of >= size_of::<*mut c_void>()`. */
_Static_assert(_Alignof(nros_rmw_session_t) >= sizeof(void*),
               "nros_rmw_session_t under-aligned vs pointer");
_Static_assert(_Alignof(nros_rmw_publisher_t) >= sizeof(void*),
               "nros_rmw_publisher_t under-aligned vs pointer");
_Static_assert(_Alignof(nros_rmw_subscription_t) >= sizeof(void*),
               "nros_rmw_subscription_t under-aligned vs pointer");
_Static_assert(_Alignof(nros_rmw_service_t) >= sizeof(void*),
               "nros_rmw_service_t under-aligned vs pointer");
_Static_assert(_Alignof(nros_rmw_client_t) >= sizeof(void*),
               "nros_rmw_client_t under-aligned vs pointer");

/* The vtable is all function pointers — its size must be a whole number
 * of pointer slots. Mirrors the Rust
 * `size_of::<NrosRmwVtable>() % ptr == 0` assertion. */
_Static_assert(sizeof(nros_rmw_vtable_t) % sizeof(void*) == 0,
               "nros_rmw_vtable_t is not a whole number of pointer slots");

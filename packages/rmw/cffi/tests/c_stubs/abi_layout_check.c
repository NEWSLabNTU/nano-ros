/*
 * ABI layout single-source-of-truth — C half (issue #238 / #239).
 *
 * The `nros_rmw_*` types were a HAND-MIRRORED C ABI: the C definitions
 * in the `include/nros` headers and Rust `#[repr(C)]` mirrors in
 * `src/lib.rs`, kept in lockstep by hand. These `_Static_assert`s pin
 * the C-side widths at compile time; the Rust half was the
 * ~~`abi_layout`~~ `const _: () = { assert!(...) }` block in `src/lib.rs`,
 * so a layout change on one side alone failed exactly one guard —
 * closing the drift class that #238 slipped through (an event-kind
 * enum that was int-sized in C but byte-sized in Rust, passed BY VALUE
 * across three vtable slots). RFC-0054 then made the Rust side consume
 * bindgen output of these headers (`src/generated.rs`), so there is no
 * hand mirror left to pin and that block is gone; this TU still pins
 * the C side.
 *
 * Compiled (not linked into anything meaningful — the TU has no runtime
 * symbols) by build.rs under the `c-stub-test` feature.
 */

#include <nros/rmw_entity.h>
#include <nros/rmw_transport.h>
#include <nros/rmw_event.h>
#include <nros/rmw_vtable.h>
#include <nros/device_port.h>

#include <stddef.h>

/* The #238 core, re-stated by issue 2758: the Rust side of every rmw ABI
 * enum is bindgen's host answer, `c_uint` (4 bytes), committed and compiled
 * for every target. An UNFIXED C enum is 1 byte on arm-none-eabi (AAPCS
 * variable-size enums), so each enum in the headers now carries a
 * `*_FORCE_INT_WIDTH_ = 0x7fffffff` pin and asserts its own int width in the
 * header (`NROS_RMW_ASSERT_ENUM_INT_WIDTH`), which every ARM TU that includes
 * it checks. This TU is compiled host-side, so the line below proves only the
 * host half; the in-header assert is the one that covers a target, and
 * `just check abi-enum-width` holds every ABI enum to having one. */
_Static_assert(sizeof(rmw_event_type_t) == 4,
               "rmw_event_type_t must be C-int-sized (issues #238, 2758)");

/* QoS mirror — the by-value struct crossing the create_* slots. Must
 * match the Rust side's `size_of::<NrosRmwQos>() == 24`.
 * (phase-301 / issue 0240: the transport hints left this struct for
 * the options structs; 28 -> 24 bytes.) */
_Static_assert(sizeof(rmw_qos_profile_t) == 24,
               "rmw_qos_profile_t size drifted from the Rust mirror (24)");

/* phase-301 options structs — NULLable trailing create_* params. */
_Static_assert(sizeof(rmw_publisher_options_t) == 8,
               "rmw_publisher_options_t size drifted (8)");
_Static_assert(sizeof(rmw_subscription_options_t) == 8,
               "rmw_subscription_options_t size drifted (8)");

/* issue 0331 — `nros_transport_ops_t` had NO layout assert on either side,
 * while `nros_rmw_cffi_set_custom_transport` bridged the C type to the Rust
 * `NrosTransportOps` by reinterpretation. Nothing would have caught an append
 * to one and not the other.
 *
 * Expressed in terms of `sizeof(void*)` rather than a literal so the assert
 * holds on a 32-bit host too: 2 x uint32_t, then user_data + 4 function
 * pointers. The Rust half is the size/align `const _` block next to that
 * export in nros-rmw-cffi/src/lib.rs. */
_Static_assert(sizeof(nros_transport_ops_t) == 2 * sizeof(uint32_t) + 5 * sizeof(void*),
               "nros_transport_ops_t size drifted from the Rust mirror");
_Static_assert(_Alignof(nros_transport_ops_t) >= sizeof(void*),
               "nros_transport_ops_t under-aligned vs pointer");

/* Opaque handle structs are pointer-aligned (they carry a `void*`
 * backend_data / backend pointer). Rust mirror asserts the same via
 * `align_of >= size_of::<*mut c_void>()`. */
_Static_assert(_Alignof(rmw_session_t) >= sizeof(void*),
               "rmw_session_t under-aligned vs pointer");
_Static_assert(_Alignof(rmw_publisher_t) >= sizeof(void*),
               "rmw_publisher_t under-aligned vs pointer");
_Static_assert(_Alignof(rmw_subscription_t) >= sizeof(void*),
               "rmw_subscription_t under-aligned vs pointer");
_Static_assert(_Alignof(rmw_service_t) >= sizeof(void*),
               "rmw_service_t under-aligned vs pointer");
_Static_assert(_Alignof(rmw_client_t) >= sizeof(void*),
               "rmw_client_t under-aligned vs pointer");

/* The vtable is all function pointers — its size must be a whole number
 * of pointer slots. Mirrors the Rust
 * `size_of::<NrosRmwVtable>() % ptr == 0` assertion. */
_Static_assert(sizeof(nros_rmw_vtable_t) % sizeof(void*) == 0,
               "nros_rmw_vtable_t is not a whole number of pointer slots");

/* phase-579 W2 (RFC-0139 D3) -- the device port ABI. Every field's OFFSET is
 * pinned, not only each struct's size, so a field moved on one side alone
 * fails here or in the Rust half (the device-port `const _` block in
 * nros-rmw-cffi/src/lib.rs), which states the same offsets in the same terms.
 * Pointer-sized and `int64_t`-aligned terms rather than literals, so both
 * halves hold on a 32-bit target as well as a 64-bit host. */
#define NROS_DP_P sizeof(void*)
#define NROS_DP_UP(x, a) ((((x) + (a)-1) / (a)) * (a))
#define NROS_DP_I64 _Alignof(int64_t)
#define NROS_DP_AT(T, f, off)                                                                      \
    _Static_assert(offsetof(T, f) == (off), #T "." #f " moved: the device port ABI changed")

_Static_assert(sizeof(nros_device_ret_t) == 4, "nros_device_ret_t must be 4 bytes");

NROS_DP_AT(nros_completion_t, _slot, 0);
_Static_assert(sizeof(nros_completion_t) == NROS_DP_P, "nros_completion_t size");

NROS_DP_AT(nros_device_buffer_t, data, 0);
NROS_DP_AT(nros_device_buffer_t, size, NROS_DP_P);
_Static_assert(sizeof(nros_device_buffer_t) == 2 * NROS_DP_P, "nros_device_buffer_t size");

NROS_DP_AT(nros_device_job_t, op, 0);
NROS_DP_AT(nros_device_job_t, input, NROS_DP_P);
NROS_DP_AT(nros_device_job_t, output, 3 * NROS_DP_P);
_Static_assert(sizeof(nros_device_job_t) == 5 * NROS_DP_P, "nros_device_job_t size");

#define NROS_DP_RES_AT NROS_DP_UP(3 * NROS_DP_P, NROS_DP_I64)
NROS_DP_AT(nros_device_result_t, output, 0);
NROS_DP_AT(nros_device_result_t, length, 2 * NROS_DP_P);
NROS_DP_AT(nros_device_result_t, captured_at_ns, NROS_DP_RES_AT);
NROS_DP_AT(nros_device_result_t, captured_stamp_ns, NROS_DP_RES_AT + 8);
NROS_DP_AT(nros_device_result_t, status, NROS_DP_RES_AT + 16);
NROS_DP_AT(nros_device_result_t, _reserved, NROS_DP_RES_AT + 20);
_Static_assert(sizeof(nros_device_result_t) ==
                   NROS_DP_UP(NROS_DP_RES_AT + 24,
                              NROS_DP_I64 > NROS_DP_P ? NROS_DP_I64 : NROS_DP_P),
               "nros_device_result_t size");

#define NROS_DP_OPS_AT NROS_DP_UP(8, NROS_DP_P)
NROS_DP_AT(nros_worker_port_ops_t, abi_version, 0);
NROS_DP_AT(nros_worker_port_ops_t, _reserved, 4);
NROS_DP_AT(nros_worker_port_ops_t, user_data, NROS_DP_OPS_AT);
NROS_DP_AT(nros_worker_port_ops_t, submit, NROS_DP_OPS_AT + NROS_DP_P);
NROS_DP_AT(nros_worker_port_ops_t, take, NROS_DP_OPS_AT + 2 * NROS_DP_P);
NROS_DP_AT(nros_worker_port_ops_t, cancel, NROS_DP_OPS_AT + 3 * NROS_DP_P);
_Static_assert(sizeof(nros_worker_port_ops_t) == NROS_DP_OPS_AT + 4 * NROS_DP_P,
               "nros_worker_port_ops_t size");

NROS_DP_AT(nros_source_port_ops_t, abi_version, 0);
NROS_DP_AT(nros_source_port_ops_t, _reserved, 4);
NROS_DP_AT(nros_source_port_ops_t, user_data, NROS_DP_OPS_AT);
NROS_DP_AT(nros_source_port_ops_t, start, NROS_DP_OPS_AT + NROS_DP_P);
NROS_DP_AT(nros_source_port_ops_t, take, NROS_DP_OPS_AT + 2 * NROS_DP_P);
NROS_DP_AT(nros_source_port_ops_t, stop, NROS_DP_OPS_AT + 3 * NROS_DP_P);
_Static_assert(sizeof(nros_source_port_ops_t) == NROS_DP_OPS_AT + 4 * NROS_DP_P,
               "nros_source_port_ops_t size");

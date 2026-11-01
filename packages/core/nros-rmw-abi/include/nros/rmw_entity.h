#ifndef NROS_RMW_ENTITY_H
#define NROS_RMW_ENTITY_H

#include <stdbool.h>
#include <stdint.h>
#include <stddef.h>
#include "nros/rmw_ret.h"   /* the two pure functions below return rmw_ret_t */

/**
 * @file rmw_entity.h
 * @brief Typed entity structs for the nros RMW C surface.
 *
 * Same shape as upstream `rmw.h`'s `rmw_publisher_t` /
 * `rmw_subscription_t` family: visible metadata + a `void * data`
 * tail (named `backend_data` here). No generic-handle typedef.
 *
 * **Lifetime rule.** All `const char *` string fields are
 * **borrowed pointers** — the storage pointing at them is owned by
 * the caller (the runtime) and must outlive the entity. Backends
 * never free or reallocate these strings; they hold them as-is for
 * the entity's lifetime.
 *
 * **ABI commitment.** These structs are part of the public ABI.
 * Adding or reordering fields is a major version bump. Backends
 * compile against this header and consumers compile against backend
 * libraries — both sides must agree on the layout.
 *
 * **Forward-compat reserved bytes.** Each entity carries an explicit
 * `_reserved[N]` byte array sized to fill the natural alignment slot
 * before `backend_data`. New fields up to N bytes can be added later
 * without changing the struct's overall size or any field's offset
 * after `backend_data`. Backends and runtime must zero the reserved
 * bytes; the runtime relies on them being zero on read.
 *
 * **No-alloc + no-std preserved.** No struct here owns heap-allocated
 * storage. All metadata is either inline POD or a borrowed pointer.
 */

/* ------------------------------------------------------------------ */
/* QoS profile — full DDS shape (matches `rmw_qos_profile_t`)          */
/* ------------------------------------------------------------------ */

/* Policy values carry UPSTREAM's numbering — phase-376 W5/B2.
 *
 * They did not until 2026-08-24, and the disagreement was not academic. Ours
 * were a dense 0/1 pair per policy, which put a DIFFERENT meaning on the same
 * integer:
 *
 *     value | upstream                          | ours (before)
 *     ------|-----------------------------------|-------------------
 *       0   | *_SYSTEM_DEFAULT                  | BEST_EFFORT / VOLATILE / KEEP_LAST
 *       1   | RELIABLE / TRANSIENT_LOCAL / KEEP_LAST | RELIABLE / TRANSIENT_LOCAL / KEEP_ALL
 *
 * so `HISTORY == 1` meant KEEP_LAST to upstream and KEEP_ALL to us — the two
 * opposite answers to the question. Liveliness was worse: MANUAL_BY_NODE and
 * MANUAL_BY_TOPIC were SWAPPED (2 and 3).
 *
 * These values cross the ABI. `rmw_qos_profile_check_compatible` is a name
 * upstream owns and we export, and the cyclonedds backend translates this
 * struct into real DDS QoS that a ROS peer matches against. A caller reasoning
 * in upstream's vocabulary got silently wrong policies. Same argument as W3.d
 * step B for the return codes: where a value crosses the boundary, upstream's
 * numbering is the only one that cannot be wrong.
 *
 * The names keep the `NROS_RMW_` prefix — as the return codes do — because the
 * set is ours to extend, not upstream's to define.
 */

/** Reliability policy values for `rmw_qos_profile_t::reliability`. */
#define NROS_RMW_RELIABILITY_SYSTEM_DEFAULT 0
#define NROS_RMW_RELIABILITY_RELIABLE       1
#define NROS_RMW_RELIABILITY_BEST_EFFORT    2
/** The backend could not determine this policy. See the note on
 *  `rmw_qos_profile_t` about partial answers. */
#define NROS_RMW_RELIABILITY_UNKNOWN        3
/** Resolve at creation against the discovered graph — upstream's
 *  `RMW_QOS_POLICY_RELIABILITY_BEST_AVAILABLE` (Iron+), issue 1973. A REQUEST
 *  value only: the runtime (`nros-rmw-cffi`) resolves it with
 *  `QoSProfile::resolve_best_available` before calling a backend's create
 *  slot, so no backend ever receives it and no read-back reports it. */
#define NROS_RMW_RELIABILITY_BEST_AVAILABLE 4

/** Durability policy values for `rmw_qos_profile_t::durability`. */
#define NROS_RMW_DURABILITY_SYSTEM_DEFAULT   0
#define NROS_RMW_DURABILITY_TRANSIENT_LOCAL  1
#define NROS_RMW_DURABILITY_VOLATILE         2
#define NROS_RMW_DURABILITY_UNKNOWN          3
/** Upstream's `RMW_QOS_POLICY_DURABILITY_BEST_AVAILABLE` (Iron+), issue 1973.
 *  A request value only — see `NROS_RMW_RELIABILITY_BEST_AVAILABLE`. */
#define NROS_RMW_DURABILITY_BEST_AVAILABLE   4

/** History policy values for `rmw_qos_profile_t::history`. */
#define NROS_RMW_HISTORY_SYSTEM_DEFAULT 0
#define NROS_RMW_HISTORY_KEEP_LAST      1
#define NROS_RMW_HISTORY_KEEP_ALL       2
#define NROS_RMW_HISTORY_UNKNOWN        3

/** Upstream `rmw_feature_t` — an optional piece of CONTENT a backend may or may
 *  not populate. Upstream defines exactly these two, both about whether
 *  message-info sequence numbers are real. Values mirror upstream's. */
typedef enum rmw_feature_t {
    RMW_FEATURE_MESSAGE_INFO_PUBLICATION_SEQUENCE_NUMBER = 0,
    RMW_FEATURE_MESSAGE_INFO_RECEPTION_SEQUENCE_NUMBER   = 1,
} rmw_feature_t;

/** Nanoseconds since a clock's epoch — upstream's `rmw_time_point_value_t`. */
typedef int64_t rmw_time_point_value_t;

/** Sentinel for a sequence number the backend does not populate. Upstream's
 *  `RMW_MESSAGE_INFO_SEQUENCE_NUMBER_UNSUPPORTED`. */
#define RMW_MESSAGE_INFO_SEQUENCE_NUMBER_UNSUPPORTED UINT64_MAX

/** Storage size of a GID, in bytes. Upstream's `RMW_GID_STORAGE_SIZE`. */
#define RMW_GID_STORAGE_SIZE 24u

/** Global identifier for a publisher — upstream `rmw_gid_t`, field for field.
 *
 *  Phase 376 W4. Mirrors upstream exactly, including the 24-byte width and the
 *  `implementation_identifier`. The identifier matters MORE here than upstream:
 *  `nros_rmw_cffi_register_named` admits several backends in one image, so two
 *  gids are comparable only when it matches.
 *
 *  Comparison is over the whole array, so a producer MUST zero-pad an
 *  identifier shorter than 24 bytes rather than leave the tail undefined —
 *  otherwise two gids naming the same entity compare unequal on stack garbage.
 *
 *  **One type and one value.** A gid obtained from a take
 *  (`rmw_message_info_t::publisher_gid`) and one obtained from
 *  `get_gid_for_publisher` are the same width (the phase-467 RMW gap-closure
 *  design study's Q1(a)) and, since issue 1495, produced from ONE source on
 *  every backend that produces either: Cyclone's writer GUID on both sides
 *  (`take_with_info`); zenoh's XXH3-128 of the entity's liveliness keyexpr,
 *  which is `rmw_zenoh_cpp`'s own derivation and so also the gid a stock
 *  graph query prints for the entity. */
/** Bytes to READ — phase-406 W2.
 *
 *  `len` is a FACT: how many bytes exist. Nothing is written through this, and
 *  `const` says so — `publish` handing a backend a mutable pointer is an
 *  invitation.
 *
 *  PASSED BY VALUE. Two words, and there is nothing to report back. */
typedef struct rmw_byte_span_t {
    const uint8_t *data;
    size_t len;
} rmw_byte_span_t;

/** Room to WRITE — phase-406 W2.
 *
 *  This is upstream's `rmw_serialized_message_t` MINUS THE ALLOCATOR. That is
 *  not a coincidence and it is the whole argument: upstream's is an
 *  `rcutils_uint8_array_t`, `{buffer, buffer_length, buffer_capacity,
 *  allocator}`, and this ABI declined it because of the last field. Drop that
 *  field and the remaining three are exactly what a caller-owned destination
 *  needs, so the "carries an allocator" objection does not transfer to this.
 *
 *  `capacity` is a LIMIT (in) and `len` is a RESULT (out). They are separate
 *  fields rather than one overloaded `len` because "capacity on the way in,
 *  length on the way out" is the `snprintf` ambiguity, and it is a bug
 *  generator.
 *
 *  PASSED BY POINTER, always. The callee must set `len`, and a by-value copy
 *  would discard it — code that compiles, runs, and yields zero-length
 *  messages. */
typedef struct rmw_mut_byte_span_t {
    uint8_t *data;
    /** In: bytes available at `data`. */
    size_t capacity;
    /** Out: bytes actually written.
     *
     *  On `NROS_RMW_RET_BUFFER_TOO_SMALL` from any of the three TAKE slots —
     *  the subscription `take` (issue 1612), the service `take_request` and
     *  the client `take_response` (issue 1632) — it is the size the refused
     *  sample, request or reply NEEDED, or `NROS_RMW_TAKE_LEN_UNKNOWN` when
     *  the backend cannot know it. The caller pre-sets it to that value on
     *  all three. Undefined on every other failure. */
    size_t len;
} rmw_mut_byte_span_t;

/** `rmw_mut_byte_span_t.len` after a too-small `take`, `take_request` or
 *  `take_response` whose payload size the backend does not know — issues
 *  1612 and 1632.
 *
 *  ZERO, and that is what makes the rule additive rather than a flag day: a
 *  sample of zero bytes fits every buffer, so no real refusal can need zero,
 *  and the CALLER sets `len` to zero before the call. A backend written before
 *  this rule, which never touches `len` on failure, therefore reports
 *  "unknown" rather than garbage, and the slot's signature does not move.
 *
 *  Before it, a too-small take could only say THAT the sample did not fit:
 *  "non-negative = bytes, negative = error" left no room for a size, so the
 *  drop log named the buffer and never the sample. */
#define NROS_RMW_TAKE_LEN_UNKNOWN 0

/** The request id a backend reports when it CANNOT supply one — issue 2009.
 *
 *  `send_request`'s `*sequence_id` and `take_response`'s `*seq_out` are how a
 *  client with more than one call outstanding matches a reply to its request
 *  (issue 0778). The CALLER pre-sets both to this value, exactly as it pre-sets
 *  `len` to `NROS_RMW_TAKE_LEN_UNKNOWN`, so a backend that never writes them —
 *  or writes this explicitly — reads as "no id" rather than as id 0, which is a
 *  legal id. Every in-tree backend that implements the client slots writes a
 *  real id.
 *
 *  What the runtime does with "no id" is defined, not guessed: a client whose
 *  backend sent a request without an id keeps ONE request in flight (a second
 *  send is refused), and a reply without an id is delivered only when exactly
 *  one request is pending. It is never handed to a different request.
 *
 *  INT64_MIN, spelled so bindgen can evaluate it. No backend counts down to
 *  it: zenoh and the stub count up from 1, cyclonedds reports its writer
 *  sequence number, XRCE its 16-bit request id. */
#define NROS_RMW_SEQUENCE_ID_UNKNOWN (-9223372036854775807LL - 1)

/** A message type's identity — phase-406 W1.
 *
 *  Upstream passes `const rosidl_message_type_support_t *`: a runtime-dispatch
 *  handle carrying `{typesupport_identifier, data, func}`, where `func` walks a
 *  type description at run time. This ABI resolves types at BUILD time, so
 *  there is nothing for `func` to do and the handle's contents do not cross.
 *
 *  What DOES cross is the identity, and it was crossing as two loose
 *  `const char *` wedged between `topic_name` and `qos` — so the argument order
 *  did not even line up with upstream's, and every create slot took two
 *  arguments where ROS 2 takes one. Grouping them costs nothing: codegen emits
 *  one `static const rmw_message_type_support_t` per type, exactly as
 *  `ROSIDL_GET_MSG_TYPE_SUPPORT(...)` already hands back a pointer to a static.
 *
 *  NAMED `rmw_`, NOT `rosidl_`, and not `nros_`. This ABI is a standard
 *  interface, so a vendor prefix would say the interface is ours — but
 *  `rosidl_message_type_support_t` belongs to `rosidl_runtime_c`, a package we
 *  do not implement, and redefining it would collide with a host build that
 *  legitimately has it in scope. Reusing `rmw_publisher_t` is safe because we
 *  ARE the rmw implementation and own that name; `rmw_` is ours to spend and
 *  neutral to a reader.
 *
 *  Future type-carried data (a serialize/deserialize pair, a bounded-size hint)
 *  lands here without changing any slot's arity again — which matters, because
 *  appending to a hand-mirrored FFI struct is what `check-ffi-struct-mirrors`
 *  exists for. */
typedef struct rmw_message_type_support_t {
    /** Fully-qualified ROS type, e.g. `"std_msgs/msg/String"`. Borrowed;
     *  must outlive every entity created with it, which a codegen `static`
     *  satisfies by construction. */
    const char *type_name;
    /** RIHS type hash, e.g. `"RIHS01_..."`, or NULL where the backend does not
     *  carry one. NULL is "not supplied", never "empty". */
    const char *type_hash;
} rmw_message_type_support_t;

/** A service type's identity. See @ref rmw_message_type_support_t.
 *
 *  Separate from the message form for the reason upstream separates them:
 *  `rosidl_service_type_support_t` and `rosidl_message_type_support_t` are
 *  distinct types there, and collapsing them here would let a service type be
 *  passed where a message type is required with no diagnostic. */
typedef struct rmw_service_type_support_t {
    /** Fully-qualified ROS service type, e.g. `"example_interfaces/srv/AddTwoInts"`. */
    const char *type_name;
    /** RIHS type hash, or NULL. */
    const char *type_hash;
} rmw_service_type_support_t;

typedef struct rmw_gid_t {
    /** Which backend produced this gid; gids from different backends are not
     *  comparable. Borrowed, static for the life of the image. */
    const char *implementation_identifier;
    /** The identifier bytes, zero-padded to the full width. */
    uint8_t data[RMW_GID_STORAGE_SIZE];
} rmw_gid_t;


/** Per-sample metadata — upstream `rmw_message_info_t`, field for field.
 *
 *  Phase 376 W4. Two channels carry this metadata to Rust callers today. A
 *  backend that fills `take_with_info` reports it here, in caller-owned
 *  storage, and the runtime dispatches that slot whenever it is non-NULL
 *  (issue 1495 — before that nothing called it, and a C/C++ backend such as
 *  Cyclone delivered every sample with no metadata at all). The Rust adapter
 *  instead writes `MESSAGE_INFO_TABLE`, a side table in `nros-rmw-cffi` keyed
 *  on the subscription's `backend_data` ADDRESS. That table is a workaround,
 *  not a design: it claims a pool slot per subscription and never releases
 *  it, so a reused handle address inherits the previous subscription's
 *  metadata.
 *
 *  Passing the struct by pointer on the take call — which is what upstream does
 *  — removes all of that: the caller owns the storage, it lives exactly as long
 *  as the call, and every backend can fill it.
 *
 *  Retiring the side table is NOT part of this change; the `take_with_info`
 *  slots are the mechanism that makes retiring it possible. */
typedef struct rmw_message_info_t {
    /** Publisher's clock at publication, ns. 0 = no source timestamp. */
    rmw_time_point_value_t source_timestamp;
    /** Subscriber's clock at reception, ns. 0 = receptions are not stamped. */
    rmw_time_point_value_t received_timestamp;
    /** Publisher-side sequence, or
     *  `RMW_MESSAGE_INFO_SEQUENCE_NUMBER_UNSUPPORTED`. Whether this is real is
     *  what `feature_supported` answers. */
    uint64_t publication_sequence_number;
    /** Subscriber-side reception count, or the same sentinel. */
    uint64_t reception_sequence_number;
    /** Which publisher sent it. All-zero `data` = unknown. */
    rmw_gid_t publisher_gid;
    /** True when the sample never left the image (Zephyr's
     *  `Z_FEATURE_LOCAL_SUBSCRIBER`, DDS intra-process). */
    bool from_intra_process;
} rmw_message_info_t;

/** Width of `rmw_request_id_t::writer_guid`, in bytes — upstream's `int8_t[16]`.
 *
 *  Issue 2081. NOT `RMW_GID_STORAGE_SIZE` (24): upstream keeps the request id's
 *  GUID and the publisher gid at different widths, and so does this ABI. */
#define RMW_REQUEST_ID_WRITER_GUID_SIZE 16u

/* Issue 2081 — the API headers (`<rclc/detail/request_id.h>`, rclcpp's
 * `service.hpp`) declare the SAME struct for a service callback, under the same
 * guard, so a translation unit that reaches two of them sees one definition. */
#ifndef NROS_RMW_REQUEST_ID_T_DEFINED
#define NROS_RMW_REQUEST_ID_T_DEFINED
/** Identity of one service request — upstream `rmw_request_id_t`, field for
 *  field.
 *
 *  `sequence_number` is the number the CLIENT assigned to the request, which
 *  together with `writer_guid` makes the pair unique across clients. It is not
 *  the backend's reply-correlation token (`take_request_with_info`'s
 *  `seq_out`), and the two need not agree. */
typedef struct rmw_request_id_t {
    /** The requesting client's identity. All zero = not reported. */
    int8_t writer_guid[16];
    /** The client-assigned sequence number of this request. */
    int64_t sequence_number;
} rmw_request_id_t;
#endif

/** Per-request metadata — upstream `rmw_service_info_t`, field for field.
 *
 *  Issue 2081. Filled by `take_request_with_info` for a request and, since
 *  issue 2136, by `take_response_with_info` for a reply. As in
 *  `rmw_message_info_t`, a field the backend cannot report is ZERO: a
 *  timestamp of 0 means "not stamped", and an all-zero
 *  `request_id.writer_guid` means the request id is not reported. The runtime
 *  turns each into `None`, never into a value.
 *
 *  The timestamps are the SENDER's and the RECEIVER's, as upstream's are: for
 *  a request, the client's send time and the server's arrival time; for a
 *  reply, the server's send time and the client's arrival time. */
typedef struct rmw_service_info_t {
    /** Sender's clock when the message was sent, ns. 0 = not reported. */
    rmw_time_point_value_t source_timestamp;
    /** Receiver's clock when the message was received, ns. 0 = not reported. */
    rmw_time_point_value_t received_timestamp;
    /** The request: who sent it, and its client-assigned sequence number. */
    rmw_request_id_t request_id;
} rmw_service_info_t;

/** Liveliness kind values for `rmw_qos_profile_t::liveliness_kind`.
 *
 *  Upstream's numbering (W5/B2). `MANUAL_BY_NODE` and `MANUAL_BY_TOPIC` were
 *  SWAPPED here until 2026-08-24 — 2 meant BY_TOPIC to us and BY_NODE to
 *  upstream — which the cyclonedds backend then translated into a real DDS
 *  liveliness kind a ROS peer matches on. */
typedef enum rmw_liveliness_kind_t {
    /** Let the middleware choose. Spelled `NONE` before W5/B2 and used the same
     *  way: nothing is asserted and nothing is tracked. Upstream has no
     *  separate `NONE`, so the two collapse onto value 0. */
    NROS_RMW_LIVELINESS_SYSTEM_DEFAULT    = 0,
    /** Backend's keepalive task asserts liveliness automatically. */
    NROS_RMW_LIVELINESS_AUTOMATIC         = 1,
    /** Application calls `assert_liveliness()` at the node level. */
    NROS_RMW_LIVELINESS_MANUAL_BY_NODE    = 2,
    /** Application calls `assert_liveliness()` per topic explicitly. */
    NROS_RMW_LIVELINESS_MANUAL_BY_TOPIC   = 3,
    /** The backend could not determine this policy. */
    NROS_RMW_LIVELINESS_UNKNOWN           = 4,
    /** Upstream's `RMW_QOS_POLICY_LIVELINESS_BEST_AVAILABLE` (Iron+), issue
     *  1973. A request value only — see `NROS_RMW_RELIABILITY_BEST_AVAILABLE`. */
    NROS_RMW_LIVELINESS_BEST_AVAILABLE    = 5,
} rmw_liveliness_kind_t;

/**
 * Full DDS-shaped QoS profile.
 *
 * Matches the field set of upstream `rmw_qos_profile_t`. Backends
 * advertise per-policy support via the runtime's
 * `supported_qos_policies()` query; entities created with a profile
 * the active backend can't honour return
 * `NROS_RMW_RET_INCOMPATIBLE_QOS` synchronously at create time
 * — no silent downgrade.
 *
 * Zero-valued fields ("off") preserve the cheap default for apps
 * that don't request the policy:
 *  - `deadline_ms = 0`            → infinite deadline (no check).
 *  - `lifespan_ms = 0`            → infinite lifespan (no expiry).
 *  - `liveliness_kind = NONE`     → no liveliness tracking.
 *  - `liveliness_lease_ms = 0`    → infinite lease.
 *
 * **Boundary semantics (phase-301, issue 0241).** Durations are u32
 * MILLISECONDS; that width is part of the contract:
 *  - `0` = unset/no-check (matches upstream `RMW_QOS_*_DEFAULT`, the
 *    zero time — a "real 0-duration" is inexpressible upstream too).
 *  - `NROS_RMW_DURATION_INFINITE_MS` = explicit infinite.
 *  - Callers lowering finer-grained times MUST round sub-ms values UP
 *    to 1 ms (rounding down would silently turn a real deadline into
 *    "no deadline") and MUST reject values past the u32-ms range
 *    (other than the infinite sentinel) at create time
 *    (`NROS_RMW_RET_INVALID_ARGUMENT`) — never clamp.
 *
 * `depth` is `uint16_t` (max 65 535). Embedded ROS application queue
 * depths are typically 1–100; the 16-bit width saves two bytes per
 * entity vs the upstream 32-bit choice. A requested depth the width
 * cannot represent is a create-time error, never a silent saturate
 * (phase-301, issue 0241).
 *
 * **Pure policy mirror (phase-301, issue 0240).** Transport hints
 * (`tx_express`, `rx_buffer_hint`) moved OUT of this struct into
 * `rmw_publisher_options_t` / `rmw_subscription_options_t` —
 * the upstream `rmw_publisher_options_t` / `rmw_subscription_options_t`
 * home for exactly that class. QoS carries DDS policy only; hint growth
 * no longer churns this ABI.
 */
typedef struct rmw_qos_profile_t {
    /* ---- 8-byte core, layout-equivalent to the original subset ---- */
    uint8_t  reliability;     /**< @see NROS_RMW_RELIABILITY_*    */
    uint8_t  durability;      /**< @see NROS_RMW_DURABILITY_*     */
    uint8_t  history;         /**< @see NROS_RMW_HISTORY_*        */
    uint8_t  liveliness_kind; /**< @see rmw_liveliness_kind_t */
    uint16_t depth;
    uint16_t _reserved0;      /**< Reserved; must be zero. */

    /* ---- 16-byte extension (Phase 109) ---- */
    /** Subscription: max acceptable inter-arrival time, ms. Publisher:
     *  max acceptable inter-publish (offered rate), ms.
     *  0 = infinite (no deadline). */
    uint32_t deadline_ms;

    /** Sample expiry, ms. Subscription filters samples older than
     *  this. 0 = infinite (no expiry). */
    uint32_t lifespan_ms;

    /** Liveliness lease, ms. Publisher must assert liveliness
     *  within this window or be considered dead. 0 = infinite. */
    uint32_t liveliness_lease_ms;

    /** If non-zero, topic-name encoding skips the ROS `/rt/` prefix
     *  and uses raw application names. Matches upstream
     *  `avoid_ros_namespace_conventions`. `0` = false, non-zero =
     *  true. (`uint8_t` instead of `bool`; `sizeof(_Bool)` is impl-
     *  defined per C99 — `uint8_t` keeps the layout stable across
     *  toolchains.) */
    uint8_t  avoid_ros_namespace_conventions;
    uint8_t  _reserved1[3];   /**< Reserved; must be zero. */
} rmw_qos_profile_t;


/* ====================================================================
 * Phase 376 W4 — the two PURE functions.
 *
 * Plain exported C functions, deliberately NOT vtable slots, and the reason is
 * the campaign's own rule read the other way round: a vtable slot is the
 * mechanism for letting backends DIFFER, and these two must not. Both compute
 * over types this header defines; if two backends disagreed about whether a QoS
 * pair is compatible, or whether two gids name the same entity, that is a
 * DEFECT with as many places to fix it as there are backends.
 *
 * Three supporting reasons:
 *
 *  - The useful call sites have no vtable to dispatch through. QoS
 *    compatibility is wanted at `create_*` time (that is what produces
 *    `NROS_RMW_RET_INCOMPATIBLE_QOS`), in codegen'd validation, and in host
 *    tooling with no session. Neither function takes an entity, so a slot would
 *    force a caller to invent a session — and neither could be called BEFORE a
 *    backend registers, which is exactly when create-time validation runs.
 *
 *  - Upstream is not evidence for a slot. `rmw_qos_profile_check_compatible`
 *    lives in `rmw/qos_profiles.h` and is defined by librmw ITSELF; it appears
 *    in the implementation contract only because each `librmw_*_cpp.so`
 *    statically links librmw and re-exports it. That is plugin packaging, not
 *    semantics, and we load no plugin.
 *
 *  - Precedent: `nros_rmw_cffi_register_named` and friends are declared here
 *    and defined once in Rust with `#[no_mangle]`, so `nros-rmw-abi` stays a
 *    header-only INTERFACE target — no new compiled TU, no new link edge.
 *    (`static inline` was the alternative and is worse: bindgen does not emit
 *    it, so RFC-0054 would force a SECOND Rust implementation, which is the
 *    first reason again.)
 * ==================================================================== */

/** Verdict of a QoS compatibility check. Upstream `rmw_qos_compatibility_type_t`,
 *  values included.
 *
 *  `WARNING` means "compatible as far as could be checked, but at least one
 *  policy on one side is `*_UNKNOWN`" — the backend could not read it back.
 *  Reachable since W5/B2 gave the policies an UNKNOWN encoding; it was defined
 *  and unreachable before that, so the value could not be reused for anything
 *  else in the meantime.
 *
 *  A definite clash OUTRANKS an unknown: if the policies that COULD be compared
 *  are already incompatible the verdict is `ERROR`, because softening it to a
 *  warning would hide something the caller can act on. */
typedef enum rmw_qos_compatibility_type_t {
    RMW_QOS_COMPATIBILITY_OK      = 0,
    RMW_QOS_COMPATIBILITY_WARNING = 1,
    RMW_QOS_COMPATIBILITY_ERROR   = 2,
} rmw_qos_compatibility_type_t;

/** Which policies clashed, as a bitmask. A nano-ros extension: upstream reports
 *  the reason only as prose, which a target cannot act on. */
typedef enum nros_rmw_qos_clash_t {
    NROS_RMW_QOS_CLASH_NONE             = 0,
    NROS_RMW_QOS_CLASH_RELIABILITY      = 1u << 0,
    NROS_RMW_QOS_CLASH_DURABILITY       = 1u << 1,
    NROS_RMW_QOS_CLASH_DEADLINE         = 1u << 2,
    NROS_RMW_QOS_CLASH_LIVELINESS_KIND  = 1u << 3,
    NROS_RMW_QOS_CLASH_LIVELINESS_LEASE = 1u << 4,
} nros_rmw_qos_clash_t;

/** Which policies of `offered` (a publisher's) and `requested` (a
 *  subscription's) are incompatible, as a bitmask — no strings, so an image
 *  that only needs the verdict never links the reason table.
 *
 *  Argument order is upstream's: publisher profile first.
 *
 *  Writes `*compatibility` and `*clash_mask` on `NROS_RMW_RET_OK`;
 *  `NROS_RMW_RET_INVALID_ARGUMENT` if either out-parameter is NULL. */
rmw_ret_t nros_rmw_qos_incompatibility_mask(
    rmw_qos_profile_t offered, rmw_qos_profile_t requested,
    rmw_qos_compatibility_type_t *compatibility, uint32_t *clash_mask);

/** Upstream `rmw_qos_profile_check_compatible`. Exact parity.
 *
 *  `reason` may be NULL with `reason_size` 0 — the create-time path, which
 *  wants the verdict and nothing else.
 *
 *  The reason is SELECTED, never FORMATTED: each clash bit maps to one
 *  `static const char[]` and they are appended by a bounded copy. Upstream's
 *  implementations use `snprintf`, which would drag the printf engine into
 *  images that deliberately excluded it.
 *
 *  Truncation is NOT failure: the buffer is always NUL-terminated and the
 *  verdict is still written. Returning `BUFFER_TOO_SMALL` would make a
 *  small-buffer caller lose the load-bearing half of the answer. */
rmw_ret_t rmw_qos_profile_check_compatible(
    rmw_qos_profile_t publisher_profile, rmw_qos_profile_t subscription_profile,
    rmw_qos_compatibility_type_t *compatibility, char *reason, size_t reason_size);

/** Upstream `rmw_compare_gids_equal`. Exact parity.
 *
 *  Equal means the same `implementation_identifier` AND the same 24 bytes. Gids
 *  from different backends are never equal — which matters more here than
 *  upstream, because `nros_rmw_cffi_register_named` admits several backends in
 *  one image.
 *
 *  Comparison is over the WHOLE array, so a producer must zero-pad; see
 *  `rmw_gid_t`. */
rmw_ret_t rmw_compare_gids_equal(const rmw_gid_t *gid1, const rmw_gid_t *gid2,
    bool *result);

/** Log severity — upstream `rmw_log_severity_t`, values included.
 *
 *  The values are `rcutils`' ladder (`DEBUG 10`, `INFO 20`, …), not a dense
 *  0..N, so they are written out rather than renumbered: a caller that has an
 *  `rcutils` severity in hand can pass it straight through.
 *
 *  There is no `TRACE`. `nros_log::Severity` has one, and it maps to `DEBUG`
 *  crossing this seam — losing a distinction upstream never had is better than
 *  inventing a value a ROS-side caller cannot produce. */
typedef enum rmw_log_severity_t {
    RMW_LOG_SEVERITY_UNSET = 0,
    RMW_LOG_SEVERITY_DEBUG = 10,
    RMW_LOG_SEVERITY_INFO  = 20,
    RMW_LOG_SEVERITY_WARN  = 30,
    RMW_LOG_SEVERITY_ERROR = 40,
    RMW_LOG_SEVERITY_FATAL = 50,
} rmw_log_severity_t;

/** Which end of a topic an endpoint is — upstream `rmw_endpoint_type_t`. */
typedef enum rmw_endpoint_type_t {
    RMW_ENDPOINT_INVALID      = 0,
    RMW_ENDPOINT_PUBLISHER    = 1,
    RMW_ENDPOINT_SUBSCRIPTION = 2,
} rmw_endpoint_type_t;

/** One discovered endpoint — upstream `rmw_topic_endpoint_info_t`.
 *
 *  Every string is BORROWED for the duration of the visit that hands this out;
 *  a caller that needs one past the callback copies it. That is what lets the
 *  graph slots stream without an allocator. */
typedef struct rmw_topic_endpoint_info_t {
    /** Node that owns the endpoint. */
    const char *node_name;
    /** That node's namespace. */
    const char *node_namespace;
    /** Fully-qualified type on the wire, e.g. `"std_msgs/msg/Int32"`. */
    const char *topic_type;
    /** Publisher or subscription. */
    rmw_endpoint_type_t endpoint_type;
    /** The endpoint's identity; `data` all-zero when the backend has none. */
    rmw_gid_t endpoint_gid;
    /** The GRANTED profile, not the requested one — which is the whole reason a
     *  consumer asks. A backend that cannot read back a remote's granted QoS
     *  reports what it MATCHED on, and must not substitute the local entity's
     *  requested profile. */
    rmw_qos_profile_t qos_profile;
} rmw_topic_endpoint_info_t;
             /* 24 bytes */

/** Transport protocol of a network flow — upstream `rmw_transport_protocol_t`,
 *  values included. */
typedef enum rmw_transport_protocol_t {
    RMW_TRANSPORT_PROTOCOL_UNKNOWN = 0,
    RMW_TRANSPORT_PROTOCOL_UDP     = 1,
    RMW_TRANSPORT_PROTOCOL_TCP     = 2,
    RMW_TRANSPORT_PROTOCOL_COUNT   = 3,
} rmw_transport_protocol_t;

/** Internet protocol of a network flow — upstream `rmw_internet_protocol_t`. */
typedef enum rmw_internet_protocol_t {
    RMW_INTERNET_PROTOCOL_UNKNOWN = 0,
    RMW_INTERNET_PROTOCOL_IPV4    = 1,
    RMW_INTERNET_PROTOCOL_IPV6    = 2,
    RMW_INTERNET_PROTOCOL_COUNT   = 3,
} rmw_internet_protocol_t;

/** Upstream's value, kept exactly: it sizes a field that crosses the ABI, and
 *  upstream took it from `linux/inet.h` for the same reason. */
#define RMW_INET_ADDRSTRLEN 48

/** One network flow endpoint — upstream `rmw_network_flow_endpoint_t`, field
 *  for field. Unlike the graph structs this one carries no pointers, so it
 *  costs nothing to mirror exactly and a caller may copy it wholesale. */
typedef struct rmw_network_flow_endpoint_t {
    rmw_transport_protocol_t transport_protocol;
    rmw_internet_protocol_t  internet_protocol;
    uint16_t                 transport_port;
    /** Publisher-side only; 0 elsewhere. */
    uint32_t                 flow_label;
    /** Differentiated Services Code Point. Publisher-side only; 0 elsewhere. */
    uint8_t                  dscp;
    char                     internet_address[RMW_INET_ADDRSTRLEN];
} rmw_network_flow_endpoint_t;

/** Visit one network flow endpoint. Return `false` to stop.
 *
 *  Upstream fills an ALLOCATING `rmw_network_flow_endpoint_array_t` through an
 *  `rcutils_allocator_t *`. There is no allocator at this seam and the flow
 *  count is a property of the OS's routing, not of anything the caller can
 *  size in advance — so it streams, exactly like the graph slots. */
typedef bool (*rmw_network_flow_endpoint_visit_fn)(void *ctx,
    const rmw_network_flow_endpoint_t *endpoint);

/** Visit a subscription's content filter. Return value ignored: there is
 *  exactly one filter per subscription, so this is a callback only to avoid
 *  handing back an allocated `rmw_subscription_content_filter_options_t`.
 *
 *  `expression` and every `parameters[i]` are BORROWED for the call. A
 *  subscription with no filter is reported as `expression == NULL`, which is
 *  what upstream's empty options struct means. */
typedef void (*rmw_content_filter_visit_fn)(void *ctx,
    const char *expression, const char *const *parameters, size_t parameter_count);

/** Explicit infinite spelling for the u32-ms duration fields
 *  (phase-301, issue 0241). Semantically identical to 0 (no check) but
 *  lets a caller distinguish "I mean infinite" from "I left it unset". */
#define NROS_RMW_DURATION_INFINITE_MS  UINT32_MAX

/** "Best available" spelling for `deadline_ms` and `liveliness_lease_ms` —
 *  upstream's `RMW_QOS_DEADLINE_BEST_AVAILABLE` /
 *  `RMW_QOS_LIVELINESS_LEASE_DURATION_BEST_AVAILABLE` (both
 *  `RMW_DURATION_INFINITE - 1`), issue 1973: the infinite sentinel minus one.
 *  A request value only, resolved by the runtime before a backend sees the
 *  profile; no finite duration lowers onto it (the u32-ms range for a finite
 *  window ends one below). */
#define NROS_RMW_DURATION_BEST_AVAILABLE_MS  (UINT32_MAX - 1u)

/* ------------------------------------------------------------------ */
/* Entity options — transport hints (phase-301, issue 0240)           */
/* ------------------------------------------------------------------ */

/**
 * Publisher creation options — the home for publisher-side transport
 * hints (upstream: `rmw_publisher_options_t`). Passed as a NULLable
 * trailing param to `create_publisher`; NULL = all defaults.
 */
/**
 * Session creation options — the home for init-time context that
 * `create_session`'s flat argument list cannot grow without another ABI break
 * (issue 0808). Passed as a NULLable trailing param; NULL = all defaults.
 *
 * The carrier question 0808 opened had two candidates: encode this behind the
 * locator string, or take one options struct. The struct wins on precedent —
 * `rmw_publisher_options_t` and `rmw_subscription_options_t` already solved
 * exactly this problem for entities, with the same NULLable-trailing-param
 * shape — and on cost: parsing config out of a locator means every backend
 * reimplements a parser, which is code size on a target plus a new class of
 * silent misparse. One break, then the struct grows.
 *
 * Of Humble's eight `rmw_init_options_t` fields this carries the two that were
 * GAPS (issue 0785). `domain_id` stays a named argument because every backend
 * needs it; `security_options` remains declined on the target (a DDS-SROS2
 * keystore path, and there is neither a filesystem nor a security plugin
 * where this ABI runs); `allocator`, `instance_id`, `impl` and
 * `implementation_identifier` are answered elsewhere or declined ABI-wide.
 */
/**
 * One backend-specific session configuration property.
 *
 * The counterpart of `RmwConfig::properties` on the Rust side, which every
 * backend already accepts and which — until phase-206 W3 — NO non-Rust caller
 * could reach: the cffi adapter built `properties: &[]` and threw the options
 * pointer away, so a C or C++ entry could state no transport configuration at
 * all, on any platform.
 *
 * Backend-specific by design. For zenoh this is zenoh-pico's run-time option
 * set — `zp_config_insert(config, Z_CONFIG_<X>_KEY, value)`, which upstream
 * calls "the primary configuration method" and which has no file format for
 * the pico client — with the accepted names derived from zenoh-pico's own
 * `config.h` (`zpico_config_keys.h`). A backend that does not recognise a name
 * MUST fail the session rather than drop it: a silently ignored configuration
 * line is indistinguishable from one that took effect.
 *
 * Both strings are NUL-terminated and BORROWED for the duration of the
 * `create_session` call.
 */
typedef struct rmw_session_property_t {
    const char *key;   /**< Property name; never NULL. */
    const char *value; /**< Property value; never NULL. */
} rmw_session_property_t;

/** Upper bound on `rmw_session_options_t::property_count`.
 *
 *  The seam marshals the properties through a stack array, so it needs a
 *  bound; 16 is chosen to clear the largest configuration anyone can actually
 *  state at once (zenoh-pico defines 23 run-time keys, but `mode` and
 *  `connect` arrive as dedicated `create_session` arguments and the 13 TLS
 *  keys split into a listen-side and a connect-side set — a full mTLS client
 *  is ~10). A caller that exceeds it is REFUSED, never truncated. */
#define RMW_SESSION_MAX_PROPERTIES 16

typedef struct rmw_session_options_t {
    /** Restrict discovery to this host. Upstream `rmw_localhost_only_t`,
     *  narrowed to a flag: 0 = the system default, non-zero = localhost only.
     *
     *  A backend that cannot restrict discovery must IGNORE this rather than
     *  fail — same contract as `mode`. Cyclone is the one that can honour it. */
    uint8_t localhost_only;
    uint8_t _reserved[7];     /**< Reserved; must be zero. */
    /** The security enclave this session belongs to, or NULL.
     *
     *  Borrowed for the duration of the call. Carried so
     *  `rmw_get_node_names_with_enclaves` stops being a HOLLOW grouping: the
     *  visitor's `enclave` argument was structurally always NULL because
     *  nothing in this ABI accepted one (issue 0785). A backend that does not
     *  track enclaves still reports NULL, which is now a fact about the
     *  backend rather than about the seam. */
    const char *enclave;
    /** Backend-specific configuration properties, or NULL.
     *
     *  Borrowed for the duration of the call, as is every string they point
     *  at. `property_count` must be 0 when this is NULL, and must not exceed
     *  `RMW_SESSION_MAX_PROPERTIES`; a violation is
     *  `RMW_RET_INVALID_ARGUMENT`, not a truncation. */
    const rmw_session_property_t *properties;
    size_t property_count; /**< Number of entries in `properties`. */
} rmw_session_options_t;

typedef struct rmw_publisher_options_t {
    /** phase-279 (#145) — express hint (`TopicInfo::tx_express` across
     *  the C ABI): non-zero = this publisher's samples bypass transport
     *  tx batching. A transport hint, not a DDS policy — no RxO
     *  matching. */
    uint8_t tx_express;
    uint8_t _reserved[7];     /**< Reserved; must be zero. */
} rmw_publisher_options_t;

/**
 * Subscription creation options — the home for subscription-side
 * transport hints (upstream: `rmw_subscription_options_t`). Passed as a
 * NULLable trailing param to `create_subscription`; NULL = all defaults.
 */
typedef struct rmw_subscription_options_t {
    /** Phase 231 (RFC-0038) — receive-buffer size hint, bytes, so a
     *  size-classing backend (zenoh-pico) can pick a small/large receive
     *  buffer. `0` = unset. A transport hint, not a DDS policy.
     *
     *  Phase 403 W1 — the paragraph above is what this field MEANS; the rules
     *  below are what it OBLIGES, and they are normative. They were prose in
     *  RFC-0005 ("Receive-buffer sizing: what a backend is obliged to do") and
     *  nowhere else, so a third party reading only this header could not tell
     *  whether ignoring the hint was conformant. It is.
     *
     *  - ADVISORY. A backend MAY ignore this field entirely. A backend with a
     *    single receive-buffer size is conformant, and so is one that rounds
     *    68 up to 1024.
     *  - `0` IS "THIS CALLER SAID NOTHING", NOT "THIS TYPE HAS NO BOUND".
     *    Every message type carries a derived upper bound — bounded in the
     *    `.msg` (`string<=64`) or capped in `nros-codegen.toml`, and an
     *    unbounded type is a BUILD ERROR rather than something that falls back
     *    to a configured default — so the runtime always has a number to put
     *    here. `0` therefore reaches a backend only from a caller that
     *    supplied no options at all (`options == NULL`, or a zero-filled
     *    struct from a hand-rolled C caller). A backend must never read it as
     *    a claim that the type is unbounded, and must not treat a non-zero
     *    hint as a tighter bound than the type's own.
     *  - IT IS NOT THE TAKE BUFFER. `take`'s `buf` / `buf_len` are owned by
     *    the RUNTIME, and `buf_len` is authoritative on every call — including
     *    when it disagrees with this hint, and when it changes between calls
     *    for the same subscription. A backend must not cache a length from
     *    here and write `buf` against it.
     *  - MUST NOT LIE. A backend MUST NOT copy a sample that does not fit the
     *    caller's take buffer and return `NROS_RMW_RET_OK`. A sample larger
     *    than `buf_len` is reported as a failure — `*taken = false` with
     *    `NROS_RMW_RET_BUFFER_TOO_SMALL` — never as a truncated success. The
     *    runtime turns that into a diagnostic naming the buffer
     *    (`report_dropped_take`); a silent truncation is a corrupt message
     *    with no symptom at all. This is the ONE guarantee the hint carries,
     *    and it holds whether or not the backend honoured the hint.
     *  - NOT A WIRE BOUND. A remote publisher is bound by the `.msg`, never by
     *    our config, so this is no promise about incoming sample size. A
     *    backend must not size a fixed structure from it in a way that makes a
     *    larger sample undeliverable WITHOUT a diagnostic.
     *
     *  There is no reverse channel: the vtable's `required_rx_bytes` query
     *  (a backend saying what it would rather the take buffer were) was
     *  deleted unread — see the note where it stood in `rmw_vtable.h`. */
    uint32_t rx_buffer_hint;
    /** Phase-519 W3 (RFC-0074, "On XRCE") — the rate this subscription
     *  DECLARED it can take, `ingress.rate_hz`, in millihertz. `0` =
     *  undeclared, which is what every caller that names nothing passes, so
     *  the field took the old `_reserved[4]` bytes and the struct is the same
     *  8 bytes it was.
     *
     *  - ADVISORY, like `rx_buffer_hint`. A backend MAY ignore it. zenoh does:
     *    its enforcement point 1 is the router's downsampling rule, which a
     *    deployment step installs, so the device has nothing to ask for.
     *  - The XRCE backend asks the agent for
     *    `max_bytes_per_second = rate_hz x rx_buffer_hint`, and only when that
     *    is safe to ask for: `rate_hz >= 1` (the agent's token bucket holds
     *    one second of the rate, and a sample bigger than the bucket stops the
     *    stream for good), a non-zero `rx_buffer_hint`, and a product that
     *    fits the wire field's `uint16_t`. Otherwise it asks for everything,
     *    as before, and logs why.
     *  - IT PACES, IT DOES NOT FILTER. The agent takes a sample and then waits
     *    for tokens, so what arrives is up to one period (`1 / rate_hz`) old,
     *    plus more for every sample the agent-side reader's history holds.
     *    Measured in RFC-0074: median age 104 ms at depth 1 for a 10 Hz
     *    declaration, against 0.2 ms unpaced. */
    uint32_t ingress_rate_mhz;
} rmw_subscription_options_t;

/* ---- Standard QoS profile constants ---- */
/*
 * Compound-literal macros rather than `static const` objects, so a profile is
 * usable in every TU that includes this header with no storage and no
 * initialisation order.
 *
 * **ONE STATEMENT PER PROFILE (phase-428 W10).** Every profile below is a
 * single line through `NROS_RMW_QOS_PROFILE_FROM_POLICIES`, which is the ONLY
 * place the fields that never vary between profiles — `_reserved0`, the three
 * durations, `avoid_ros_namespace_conventions`, `_reserved1` — are written.
 * They used to be written out per profile: 26 constants in one file held in
 * agreement by eye, and the four profiles carrying a concrete liveliness had
 * all drifted the same way while their doc comments asserted the drifted value
 * was upstream's.
 *
 * **The values are UPSTREAM's, and this file is a TRANSCRIPTION of them.** An
 * embedded image cannot include `rmw/qos_profiles.h` and an offline build has
 * no ROS install to read, so the numbers have to live here. That makes them
 * exactly the kind of copy that drifts, so they are not maintained by hand
 * alone: `scripts/check-qos-profile-ssot.py` binds every profile below to the
 * Rust SSoT (`nros_rmw::QoSProfile`'s `QOS_PROFILE_*` constants) and, when a
 * ROS install is resolvable, re-derives both against
 * `<prefix>/include/rmw/rmw/qos_profiles.h`. Edit a value here without the
 * gate agreeing and the gate is what tells you.
 *
 * That named `scripts/check-qos-profile-table.py` from 2026-09-05 to phase-454
 * W1, and NO SUCH SCRIPT EXISTED for any of it. Two gates were written for W10
 * and the duplicate was deleted — correctly, one parser of one table — but this
 * prose kept the deleted name, so the transcription claiming loudest to be
 * checked was the one checked by nothing. W1 wrote the coverage the sentence
 * had been describing, into the gate that survived.
 */

/** Build a profile from the five policies that actually differ between
 *  profiles.
 *
 *  INTERNAL to this profile list — it is not itself a profile, and a backend
 *  has no reason to call it. The arguments are BARE policy suffixes
 *  (`RELIABLE`, `VOLATILE`, `KEEP_LAST`, `SYSTEM_DEFAULT`, …) token-pasted
 *  onto the `NROS_RMW_*` prefixes, so a policy that is renamed becomes a
 *  compile error here instead of a silently wrong integer — phase-376 W5/B2's
 *  lesson ("a comment is not a binding") applied to the profiles as well as to
 *  the enums. */
#define NROS_RMW_QOS_PROFILE_FROM_POLICIES(rel, dur, hist, dep, live)       \
    ((rmw_qos_profile_t){                                                   \
        .reliability     = NROS_RMW_RELIABILITY_##rel,                      \
        .durability      = NROS_RMW_DURABILITY_##dur,                       \
        .history         = NROS_RMW_HISTORY_##hist,                         \
        .liveliness_kind = NROS_RMW_LIVELINESS_##live,                      \
        .depth           = (dep),                                           \
        ._reserved0      = 0,                                               \
        .deadline_ms     = 0,                                               \
        .lifespan_ms     = 0,                                               \
        .liveliness_lease_ms = 0,                                           \
        .avoid_ros_namespace_conventions = 0,                               \
        ._reserved1      = {0, 0, 0},                                       \
    })

/* LIVELINESS IS THE SENTINEL IN EVERY CONCRETE PROFILE — phase-428 W10.
 *
 * `DEFAULT`, `SENSOR_DATA`, `SERVICES_DEFAULT` and `PARAMETERS` carried
 * `NROS_RMW_LIVELINESS_AUTOMATIC` (1) until 2026-09-05, under doc comments
 * reading "`rmw_qos_profile_default`-equivalent … automatic liveliness".
 * Upstream's own initialisers say `RMW_QOS_POLICY_LIVELINESS_SYSTEM_DEFAULT`
 * for all seven profiles, `rmw_qos_profile_unknown` excepted — there is no
 * upstream profile that asks for AUTOMATIC. This is the F2 class: 0829 fixed
 * `SYSTEM_DEFAULT`'s liveliness because that was the profile being looked at,
 * and the four siblings carrying the same wrong value were not swept.
 *
 * THIS CHANGES WHAT A BACKEND COMPILES AGAINST, so what it does to the wire is
 * stated rather than assumed. The two in-tree consumers of a changed macro are
 * `nros-rmw-cyclonedds`'s `create_service` / `create_client`
 * (`NROS_RMW_QOS_PROFILE_SERVICES_DEFAULT` when the caller passes NULL) and
 * `nros-rmw-xrce`'s (same); the cyclone unit tests use `_DEFAULT`.
 *   - cyclonedds: `make_dds_qos` calls `dds_qset_liveliness` only when the kind
 *     is NOT the sentinel (`qos.cpp:99`), so AUTOMATIC(1) set
 *     `DDS_LIVELINESS_AUTOMATIC` + `DDS_INFINITY` explicitly and the sentinel
 *     omits the call — leaving Cyclone's own default, which the DDS spec fixes
 *     at AUTOMATIC with an infinite lease. Same offered QoS on the wire.
 *   - xrce: drops liveliness entirely (it lowers no liveliness field), so the
 *     value is unobservable there either way.
 *   - zenoh: reads no QoS field at all (phase-428 F1).
 * A backend that starts honouring liveliness later sees the correct
 * "middleware's choice" here rather than an unrequested AUTOMATIC.
 */

/** `rmw_qos_profile_default` — reliable + volatile + keep-last(10). */
#define NROS_RMW_QOS_PROFILE_DEFAULT                                        \
    NROS_RMW_QOS_PROFILE_FROM_POLICIES(RELIABLE, VOLATILE, KEEP_LAST, 10, SYSTEM_DEFAULT)

/** `rmw_qos_profile_sensor_data` — best-effort + volatile + keep-last(5). */
#define NROS_RMW_QOS_PROFILE_SENSOR_DATA                                    \
    NROS_RMW_QOS_PROFILE_FROM_POLICIES(BEST_EFFORT, VOLATILE, KEEP_LAST, 5, SYSTEM_DEFAULT)

/** `rmw_qos_profile_services_default` — reliable + volatile + keep-last(10).
 *
 *  Spelled out rather than aliased to `NROS_RMW_QOS_PROFILE_DEFAULT`. The two
 *  are field-identical in every ROS distro we target, but upstream states them
 *  as two independent initialisers and is free to move one; an alias makes
 *  that divergence unrepresentable and hides which profile a backend actually
 *  asked for. With the shared expander the duplication is five tokens. */
#define NROS_RMW_QOS_PROFILE_SERVICES_DEFAULT                               \
    NROS_RMW_QOS_PROFILE_FROM_POLICIES(RELIABLE, VOLATILE, KEEP_LAST, 10, SYSTEM_DEFAULT)

/** `rmw_qos_profile_parameters` — reliable + volatile + keep-last(1000). */
#define NROS_RMW_QOS_PROFILE_PARAMETERS                                     \
    NROS_RMW_QOS_PROFILE_FROM_POLICIES(RELIABLE, VOLATILE, KEEP_LAST, 1000, SYSTEM_DEFAULT)

/** `rmw_qos_profile_parameter_events` — reliable + volatile + keep-last(1000).
 *
 *  NEW at phase-428 W10; the C ABI had no name for this profile while the Rust
 *  side did, which is half of why the Rust copy could sit at `KEEP_ALL, 0` for
 *  two phases with nothing to disagree with it. KEEP_LAST(1000) is not a
 *  cosmetic difference from KEEP_ALL: a RELIABLE writer with KEEP_ALL BLOCKS
 *  when history fills, where KEEP_LAST overwrites. */
#define NROS_RMW_QOS_PROFILE_PARAMETER_EVENTS                               \
    NROS_RMW_QOS_PROFILE_FROM_POLICIES(RELIABLE, VOLATILE, KEEP_LAST, 1000, SYSTEM_DEFAULT)

/** `rmw_qos_profile_system_default` — **every field is the sentinel**.
 *
 * issue 0829. This aliased `NROS_RMW_QOS_PROFILE_DEFAULT` until 2026-09-03,
 * which made `SYSTEM_DEFAULT` a byte-for-byte synonym for a concrete
 * RELIABLE / VOLATILE / KEEP_LAST(10) profile — i.e. the constant said nothing
 * of its own. Upstream's `rmw_qos_profile_system_default` names no concrete
 * policy either; it is all sentinel, and the RMW resolves it. The two
 * reference RMWs resolve the SAME sentinel differently —
 * `rmw_cyclonedds_cpp`'s `create_readwrite_qos` gives `KEEP_LAST, 1`, while
 * `rmw_zenoh_cpp`'s `QoS::QoS()` gives `RMW_ZENOH_DEFAULT_HISTORY_DEPTH`,
 * which is 42 — so no number baked here could be right.
 *
 * NOT AN ABI BREAK: no struct layout moved and no policy value was
 * renumbered. What changed is the BYTES this macro expands to, so a caller
 * that recompiles against this header and uses this constant now sends
 * sentinels where it used to send depth 10. That is the intended fix, and
 * nothing in this repository used the macro. Every field being zero also means
 * this is what a `memset` of `rmw_qos_profile_t` gives, so the ABI stops
 * having two answers for the same bytes.
 *
 * Which concrete values a sentinel becomes is the BACKEND's answer, applied at
 * its create entry: cyclonedds resolves to RELIABLE / VOLATILE / KEEP_LAST(1),
 * mirroring `rmw_cyclonedds_cpp`; xrce resolves the three policies the same way
 * and leaves the depth for the Agent; zenoh resolves the depth to the ring the
 * shim actually enforces. */
#define NROS_RMW_QOS_PROFILE_SYSTEM_DEFAULT                                 \
    NROS_RMW_QOS_PROFILE_FROM_POLICIES(SYSTEM_DEFAULT, SYSTEM_DEFAULT, SYSTEM_DEFAULT, 0,  \
                                       SYSTEM_DEFAULT)

/** `rmw_qos_profile_unknown` — **"the backend could not determine this"**, and
 *  distinct from `SYSTEM_DEFAULT`.
 *
 *  NEW at phase-428 W10, completing upstream's set of seven. `SYSTEM_DEFAULT`
 *  means "nobody asked, so the middleware chooses"; `UNKNOWN` means "we looked
 *  and cannot say", which is what a `*_get_actual_qos` slot owes its caller for
 *  a policy it cannot read back. The `*_UNKNOWN` policy values have existed
 *  since phase-376 W5/B2 with no profile naming them, which is why
 *  `get_actual_qos` echoes the REQUEST today (phase-428 F1) — an unreportable
 *  field reads as *granted*, the inverse of upstream's meaning. Depth is 0
 *  because upstream's is `RMW_QOS_POLICY_DEPTH_SYSTEM_DEFAULT`; there is no
 *  "unknown depth" spelling. */
#define NROS_RMW_QOS_PROFILE_UNKNOWN                                        \
    NROS_RMW_QOS_PROFILE_FROM_POLICIES(UNKNOWN, UNKNOWN, UNKNOWN, 0, UNKNOWN)

/* ------------------------------------------------------------------ */
/* Which QoS policies a backend honours (issue 1329)                  */
/* ------------------------------------------------------------------ */

/** One bit per policy a caller can REQUEST, for
 *  `nros_rmw_vtable_t::supported_qos_policies`.
 *
 *  These mirror `nros_rmw::QoSPolicyMask`'s `pub const` block bit for bit, and
 *  `check-qos-mask-derivation` compares the two — a hand mirror at a distance
 *  is still a hand mirror (issue 0160), and this one decides whether an
 *  application's entity is created.
 *
 *  A bit means "I read this field and either APPLY the request or REFUSE a
 *  value I cannot serve". It does NOT mean "I pass the field along": handing a
 *  profile to a discovery keyexpr or to an Agent that may ignore it is what
 *  made the union over-claim in the first place. The mask is per BACKEND, not
 *  per entity — a backend that honours a policy for publishers and ignores it
 *  for services does not get the bit.
 *
 *  The granularity is a VALUE where the values differ in cost: DURABILITY has
 *  two bits because a backend with no historical cache honours VOLATILE and
 *  cannot serve TRANSIENT_LOCAL, and liveliness has one bit per kind plus one
 *  for a finite lease. That is what lets a backend say what is true of it
 *  instead of rounding to the nearest whole policy. */
#define NROS_RMW_QOS_POLICY_RELIABILITY                  (1u << 0)
#define NROS_RMW_QOS_POLICY_DURABILITY_VOLATILE          (1u << 1)
#define NROS_RMW_QOS_POLICY_DURABILITY_TRANSIENT_LOCAL   (1u << 2)
#define NROS_RMW_QOS_POLICY_HISTORY                      (1u << 3)
#define NROS_RMW_QOS_POLICY_DEPTH                        (1u << 4)
#define NROS_RMW_QOS_POLICY_DEADLINE                     (1u << 5)
#define NROS_RMW_QOS_POLICY_LIFESPAN                     (1u << 6)
#define NROS_RMW_QOS_POLICY_LIVELINESS_AUTOMATIC         (1u << 7)
#define NROS_RMW_QOS_POLICY_LIVELINESS_MANUAL_BY_TOPIC   (1u << 8)
#define NROS_RMW_QOS_POLICY_LIVELINESS_MANUAL_BY_NODE    (1u << 9)
#define NROS_RMW_QOS_POLICY_LIVELINESS_LEASE             (1u << 10)
#define NROS_RMW_QOS_POLICY_AVOID_ROS_NAMESPACE_CONVENTIONS (1u << 11)

/** Honours nothing. What a backend that has not been asked, or has nothing to
 *  say, answers — and what the runtime assumes for a NULL
 *  `supported_qos_policies` slot. See that slot for why the absence resolves
 *  this way and not to a union. */
#define NROS_RMW_QOS_POLICY_NONE                         0u

/** The four policies every `rmw_qos_profile_t` states a value for.
 *
 *  A SHAPE, not a floor: a caller who states nothing else still states these,
 *  and a backend still has to earn each of the four. It is not
 *  "what every backend supports" — the zenoh shim honoured none of the four
 *  until phase-428 W9 measured it. */
#define NROS_RMW_QOS_POLICY_CORE                                             \
    (NROS_RMW_QOS_POLICY_RELIABILITY | NROS_RMW_QOS_POLICY_DURABILITY_VOLATILE | \
     NROS_RMW_QOS_POLICY_HISTORY | NROS_RMW_QOS_POLICY_DEPTH)

/* ------------------------------------------------------------------ */
/* Entity structs                                                     */
/* ------------------------------------------------------------------ */

/**
 * Per-process RMW session — the entity returned by `vtable->create_session`.
 *
 * Carries the node identity (used for diagnostics + wire-level
 * topic-key derivation in some backends) plus the opaque
 * backend-private state.
 *
 * The 8-byte `_reserved` slot is sized for a forthcoming
 * `vtable: const struct nros_rmw_vtable_t *` field that Phase 104's
 * multi-instance work will land here. Backends and runtime keep
 * these bytes zero.
 */
typedef struct rmw_session_t {
    /** Node name (borrowed from caller; outlives the session). */
    const char *node_name;
    /** Node namespace (borrowed from caller; outlives the session). */
    const char *namespace_;
    /** Reserved for future fields (Phase 104 vtable pointer slot);
     *  must be zero. */
    uint8_t     _reserved[8];
    /** Opaque backend state. NULL for an uninitialised session. */
    void       *backend_data;
} rmw_session_t;

/** A graph node — upstream `rmw_node_t`, minus what an image has no use for.
 *
 *  Phase 376 W4. Storage is CALLER-OWNED, like every other entity here: the
 *  runtime hands `create_node` a zero-initialised shell and the backend writes
 *  its `backend_data` into it.
 *
 *  **Why a node exists at all when an image opens ONE session.** The session
 *  half of that statement holds; the node half does not, and our own code says
 *  so. `Executor` keeps a node table, and `CffiSession::entity_view` exists
 *  SOLELY to fabricate a per-call session carrying the entity's owning-node
 *  identity — its own comment reads "one session can host N graph nodes". The
 *  zenoh backend then re-derives a node registry from that string by
 *  linear-scanning declared tokens. So node identity already reaches the
 *  backend, through a side channel, in every image.
 *
 *  **Done, W5/B1 (2026-08-24).** `create_publisher` / `create_subscription` /
 *  `create_service` / `create_client` take `const rmw_node_t *` the way
 *  upstream does. That retired the entity_view fabrication — the shim now
 *  owns a node table and calls `create_node` once per distinct
 *  `(name, namespace)`, which is only true because `Executor::create_node`
 *  deduplicates (W5/B1.a).
 *
 *  **Still owed:** zenoh's `ensure_node_liveliness` still linear-scans its own
 *  `per_node_liveliness` table. Retiring it needs a `create_node` method on the
 *  Rust `Rmw`/`Session` trait plus a trampoline in `RustBackendAdapter`, so
 *  that a Rust backend can be TOLD about a node the way a C one is. The slot
 *  and the table it needs both exist now; only that trait hop is missing. Do
 *  not read this paragraph as done — the first draft of this comment said the
 *  registry was retired, which it was not.
 *
 *  Not carried from upstream: `implementation_identifier` and `data` (one
 *  image links one backend per session, so there is nothing to disambiguate).
 *
 *  `session` IS carried, and is our `context`. Upstream's node reaches its
 *  context that way and every `rmw_create_*` relies on it; a node with no route
 *  to its session cannot be the only argument those slots get, which is what
 *  made this field the precondition for the whole change rather than a
 *  convenience. Set by the runtime BEFORE `create_node`, and stable for the
 *  node's life. */
typedef struct rmw_node_t {
    /** Node name. Borrowed; outlives the node. */
    const char *name;
    /** Node namespace. Borrowed; outlives the node. */
    const char *namespace_;
    /** The session this node lives on — upstream's `context`. Set by the
     *  runtime before `create_node`; never NULL in a node the runtime hands to
     *  a slot. A backend reaches its own session state through
     *  `node->session->backend_data`. */
    rmw_session_t *session;
    /** Reserved; must be zero. */
    uint8_t _reserved[8];
    /** Opaque backend state. NULL until `create_node` succeeds. */
    void *backend_data;
} rmw_node_t;

/**
 * Publisher entity.
 *
 * Created by `vtable->create_publisher`; destroyed by
 * `vtable->destroy_publisher`. The runtime owns the storage; the
 * runtime fills `topic_name` / `type_name` / `qos` before the
 * create call. The backend writes `backend_data`.
 *
 * `can_loan_messages` matches upstream `rmw_publisher_t`'s field of
 * the same name — `true` means the backend exposes the
 * `borrow_loaned_message` / `publish_loaned_message` primitive.
 *
 * **The runtime DERIVES it; a backend does not write it** (issue 0814).
 * Its value is exactly `vtable->borrow_loaned_message != NULL`, because
 * that is the same fact, and a fact with two spellings drifts — this one
 * had drifted in both directions at once. Anything a backend writes here
 * is overwritten.
 *
 * This paragraph used to read "the runtime reads it once at create time
 * and picks the publish path accordingly; no per-call probe". That was
 * never true: nothing has ever branched on the field, and the publish
 * path is chosen per call from `vtable->borrow_loaned_message` itself.
 *
 * To refuse a loan for a PARTICULAR entity, return
 * `RMW_RET_UNSUPPORTED` from `borrow_loaned_message`. The per-entity
 * answer belongs on the call, which the runtime consults; not on this
 * flag, which it does not.
 */
typedef struct rmw_publisher_t {
    /** Topic name (borrowed; outlives the publisher). */
    const char    *topic_name;
    /** ROS-2-style fully-qualified type name
     *  (e.g., `"std_msgs/msg/Int32"`). Borrowed; outlives the publisher. */
    const char    *type_name;
    /** QoS subset honoured by this publisher. */
    rmw_qos_profile_t qos;
    /** Backend exposes `borrow_loaned_message` / `publish_loaned_message`.
     *  DERIVED by the runtime from `vtable->borrow_loaned_message`;
     *  a backend's write is overwritten (issue 0814). */
    bool           can_loan_messages;
    /** Reserved for future fields; must be zero. */
    uint8_t        _reserved[7];
    /** Opaque backend state. NULL if creation failed. */
    void          *backend_data;
} rmw_publisher_t;

/**
 * Subscription entity (phase-301: renamed from `subscriber` to the upstream `rmw_subscription_t` term). Same shape as the publisher; `can_loan_messages`
 * means the backend exposes the receive-side loan primitive.
 */
typedef struct rmw_subscription_t {
    /** Topic name (borrowed; outlives the subscription). */
    const char    *topic_name;
    /** Fully-qualified type name. Borrowed. */
    const char    *type_name;
    /** QoS subset honoured by this subscription. */
    rmw_qos_profile_t qos;
    /** Backend exposes loan_recv / release_recv (Phase 99).
     *  DERIVED by the runtime from `vtable->take_loaned_message`;
     *  a backend's write is overwritten (issue 0814). */
    bool           can_loan_messages;
    /** Reserved for future fields; must be zero. */
    uint8_t        _reserved[7];
    /** Opaque backend state. NULL if creation failed. */
    void          *backend_data;
} rmw_subscription_t;

/**
 * Service entity (phase-301: renamed from `service_server` to the upstream `rmw_service_t` term).
 *
 * Service entities have no QoS in the nros subset (the upstream
 * `rmw_qos_profile_services_default` distinction does not generalise
 * across non-DDS backends — see book `concepts/ros2-comparison.md`).
 *
 * No `can_loan_messages` field — service request/reply currently
 * always goes through `take_request` / `send_response` byte-buffer
 * APIs. If a future backend wants service-side lending, the
 * `_reserved[8]` block accommodates the bool + 7 padding bytes
 * without an ABI break.
 */
typedef struct rmw_service_t {
    /** Service name (borrowed; outlives the server). */
    const char *service_name;
    /** Fully-qualified service type name (e.g.,
     *  `"example_interfaces/srv/AddTwoInts"`). Borrowed. */
    const char *type_name;
    /** Reserved for future fields; must be zero. */
    uint8_t     _reserved[8];
    /** Opaque backend state. NULL if creation failed. */
    void       *backend_data;
} rmw_service_t;

/**
 * Client entity (phase-301: renamed from `service_client` to the upstream `rmw_client_t` term). Same shape as the service.
 */
typedef struct rmw_client_t {
    /** Service name (borrowed; outlives the client). */
    const char *service_name;
    /** Fully-qualified service type name. Borrowed. */
    const char *type_name;
    /** Reserved for future fields; must be zero. */
    uint8_t     _reserved[8];
    /** Opaque backend state. NULL if creation failed. */
    void       *backend_data;
} rmw_client_t;

#endif /* NROS_RMW_ENTITY_H */

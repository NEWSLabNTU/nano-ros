#ifndef NROS_RMW_ENTITY_H
#define NROS_RMW_ENTITY_H

#include <stdbool.h>
#include <stdint.h>
#include <stddef.h>

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

/** Reliability policy values for `nros_rmw_qos_t::reliability`. */
#define NROS_RMW_RELIABILITY_BEST_EFFORT 0
#define NROS_RMW_RELIABILITY_RELIABLE    1

/** Durability policy values for `nros_rmw_qos_t::durability`. */
#define NROS_RMW_DURABILITY_VOLATILE         0
#define NROS_RMW_DURABILITY_TRANSIENT_LOCAL  1

/** History policy values for `nros_rmw_qos_t::history`. */
#define NROS_RMW_HISTORY_KEEP_LAST 0
#define NROS_RMW_HISTORY_KEEP_ALL  1

/** Liveliness kind values for `nros_rmw_qos_t::liveliness_kind`. */
typedef enum nros_rmw_liveliness_kind_t {
    /** No liveliness assertion or tracking. Default for entities
     *  that don't care about liveliness. */
    NROS_RMW_LIVELINESS_NONE              = 0,
    /** Backend's keepalive task asserts liveliness automatically. */
    NROS_RMW_LIVELINESS_AUTOMATIC         = 1,
    /** Application calls `assert_liveliness()` per topic explicitly. */
    NROS_RMW_LIVELINESS_MANUAL_BY_TOPIC   = 2,
    /** Application calls `assert_liveliness()` at the node level. */
    NROS_RMW_LIVELINESS_MANUAL_BY_NODE    = 3,
} nros_rmw_liveliness_kind_t;

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
 * `nros_rmw_publisher_options_t` / `nros_rmw_subscription_options_t` —
 * the upstream `rmw_publisher_options_t` / `rmw_subscription_options_t`
 * home for exactly that class. QoS carries DDS policy only; hint growth
 * no longer churns this ABI.
 */
typedef struct nros_rmw_qos_t {
    /* ---- 8-byte core, layout-equivalent to the original subset ---- */
    uint8_t  reliability;     /**< @see NROS_RMW_RELIABILITY_*    */
    uint8_t  durability;      /**< @see NROS_RMW_DURABILITY_*     */
    uint8_t  history;         /**< @see NROS_RMW_HISTORY_*        */
    uint8_t  liveliness_kind; /**< @see nros_rmw_liveliness_kind_t */
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
} nros_rmw_qos_t;             /* 24 bytes */

/** Explicit infinite spelling for the u32-ms duration fields
 *  (phase-301, issue 0241). Semantically identical to 0 (no check) but
 *  lets a caller distinguish "I mean infinite" from "I left it unset". */
#define NROS_RMW_DURATION_INFINITE_MS  UINT32_MAX

/* ------------------------------------------------------------------ */
/* Entity options — transport hints (phase-301, issue 0240)           */
/* ------------------------------------------------------------------ */

/**
 * Publisher creation options — the home for publisher-side transport
 * hints (upstream: `rmw_publisher_options_t`). Passed as a NULLable
 * trailing param to `create_publisher`; NULL = all defaults.
 */
typedef struct nros_rmw_publisher_options_t {
    /** phase-279 (#145) — express hint (`TopicInfo::tx_express` across
     *  the C ABI): non-zero = this publisher's samples bypass transport
     *  tx batching. A transport hint, not a DDS policy — no RxO
     *  matching. */
    uint8_t tx_express;
    uint8_t _reserved[7];     /**< Reserved; must be zero. */
} nros_rmw_publisher_options_t;

/**
 * Subscription creation options — the home for subscription-side
 * transport hints (upstream: `rmw_subscription_options_t`). Passed as a
 * NULLable trailing param to `create_subscription`; NULL = all defaults.
 */
typedef struct nros_rmw_subscription_options_t {
    /** Phase 231 (RFC-0038) — receive-buffer size hint, bytes, so a
     *  size-classing backend (zenoh-pico) can pick a small/large receive
     *  buffer. `0` = unset. A transport hint, not a DDS policy. */
    uint32_t rx_buffer_hint;
    uint8_t  _reserved[4];    /**< Reserved; must be zero. */
} nros_rmw_subscription_options_t;

/* ---- Standard QoS profile constants ---- */
/* Defined as static const initialisers at the bottom of this
 * header so they're available in every compilation unit that
 * includes it. Match the field set of upstream
 * `rmw_qos_profile_default` etc. */

/** `rmw_qos_profile_default`-equivalent: reliable + volatile +
 *  keep-last(10), automatic liveliness, no deadline / lifespan. */
#define NROS_RMW_QOS_PROFILE_DEFAULT \
    ((nros_rmw_qos_t){                                                   \
        .reliability = NROS_RMW_RELIABILITY_RELIABLE,                    \
        .durability  = NROS_RMW_DURABILITY_VOLATILE,                     \
        .history     = NROS_RMW_HISTORY_KEEP_LAST,                       \
        .liveliness_kind = NROS_RMW_LIVELINESS_AUTOMATIC,                \
        .depth       = 10,                                               \
        ._reserved0  = 0,                                                \
        .deadline_ms = 0,                                                \
        .lifespan_ms = 0,                                                \
        .liveliness_lease_ms = 0,                                        \
        .avoid_ros_namespace_conventions = 0,                            \
        ._reserved1  = {0, 0, 0},                                        \
    })

/** `rmw_qos_profile_sensor_data`-equivalent: best-effort +
 *  volatile + keep-last(5). */
#define NROS_RMW_QOS_PROFILE_SENSOR_DATA \
    ((nros_rmw_qos_t){                                                   \
        .reliability = NROS_RMW_RELIABILITY_BEST_EFFORT,                 \
        .durability  = NROS_RMW_DURABILITY_VOLATILE,                     \
        .history     = NROS_RMW_HISTORY_KEEP_LAST,                       \
        .liveliness_kind = NROS_RMW_LIVELINESS_AUTOMATIC,                \
        .depth       = 5,                                                \
        ._reserved0  = 0,                                                \
        .deadline_ms = 0,                                                \
        .lifespan_ms = 0,                                                \
        .liveliness_lease_ms = 0,                                        \
        .avoid_ros_namespace_conventions = 0,                            \
        ._reserved1  = {0, 0, 0},                                        \
    })

/** `rmw_qos_profile_services_default`-equivalent: reliable +
 *  volatile + keep-last(10). */
#define NROS_RMW_QOS_PROFILE_SERVICES_DEFAULT  NROS_RMW_QOS_PROFILE_DEFAULT

/** `rmw_qos_profile_parameters`-equivalent: reliable + volatile +
 *  keep-last(1000). */
#define NROS_RMW_QOS_PROFILE_PARAMETERS \
    ((nros_rmw_qos_t){                                                   \
        .reliability = NROS_RMW_RELIABILITY_RELIABLE,                    \
        .durability  = NROS_RMW_DURABILITY_VOLATILE,                     \
        .history     = NROS_RMW_HISTORY_KEEP_LAST,                       \
        .liveliness_kind = NROS_RMW_LIVELINESS_AUTOMATIC,                \
        .depth       = 1000,                                             \
        ._reserved0  = 0,                                                \
        .deadline_ms = 0,                                                \
        .lifespan_ms = 0,                                                \
        .liveliness_lease_ms = 0,                                        \
        .avoid_ros_namespace_conventions = 0,                            \
        ._reserved1  = {0, 0, 0},                                        \
    })

/** `rmw_qos_profile_system_default`-equivalent: same as DEFAULT. */
#define NROS_RMW_QOS_PROFILE_SYSTEM_DEFAULT  NROS_RMW_QOS_PROFILE_DEFAULT

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
typedef struct nros_rmw_session_t {
    /** Node name (borrowed from caller; outlives the session). */
    const char *node_name;
    /** Node namespace (borrowed from caller; outlives the session). */
    const char *namespace_;
    /** Reserved for future fields (Phase 104 vtable pointer slot);
     *  must be zero. */
    uint8_t     _reserved[8];
    /** Opaque backend state. NULL for an uninitialised session. */
    void       *backend_data;
} nros_rmw_session_t;

/**
 * Publisher entity.
 *
 * Created by `vtable->create_publisher`; destroyed by
 * `vtable->destroy_publisher`. The runtime owns the storage; the
 * runtime fills `topic_name` / `type_name` / `qos` before the
 * create call. The backend writes `can_loan_messages` and
 * `backend_data`.
 *
 * `can_loan_messages` matches upstream `rmw_publisher_t`'s field of
 * the same name — `true` means the backend exposes the
 * `loan_publish` / `commit_publish` primitive (Phase 99). The
 * runtime reads it once at create time and picks the publish path
 * accordingly; no per-call probe.
 */
typedef struct nros_rmw_publisher_t {
    /** Topic name (borrowed; outlives the publisher). */
    const char    *topic_name;
    /** ROS-2-style fully-qualified type name
     *  (e.g., `"std_msgs/msg/Int32"`). Borrowed; outlives the publisher. */
    const char    *type_name;
    /** QoS subset honoured by this publisher. */
    nros_rmw_qos_t qos;
    /** Backend exposes loan_publish / commit_publish (Phase 99). */
    bool           can_loan_messages;
    /** Reserved for future fields; must be zero. */
    uint8_t        _reserved[7];
    /** Opaque backend state. NULL if creation failed. */
    void          *backend_data;
} nros_rmw_publisher_t;

/**
 * Subscription entity (phase-301: renamed from `subscriber` to the upstream `rmw_subscription_t` term). Same shape as the publisher; `can_loan_messages`
 * means the backend exposes the receive-side loan primitive.
 */
typedef struct nros_rmw_subscription_t {
    /** Topic name (borrowed; outlives the subscription). */
    const char    *topic_name;
    /** Fully-qualified type name. Borrowed. */
    const char    *type_name;
    /** QoS subset honoured by this subscription. */
    nros_rmw_qos_t qos;
    /** Backend exposes loan_recv / release_recv (Phase 99). */
    bool           can_loan_messages;
    /** Reserved for future fields; must be zero. */
    uint8_t        _reserved[7];
    /** Opaque backend state. NULL if creation failed. */
    void          *backend_data;
} nros_rmw_subscription_t;

/**
 * Service entity (phase-301: renamed from `service_server` to the upstream `rmw_service_t` term).
 *
 * Service entities have no QoS in the nros subset (the upstream
 * `rmw_qos_profile_services_default` distinction does not generalise
 * across non-DDS backends — see book `concepts/ros2-comparison.md`).
 *
 * No `can_loan_messages` field — service request/reply currently
 * always goes through `try_recv_request` / `send_reply` byte-buffer
 * APIs. If a future backend wants service-side lending, the
 * `_reserved[8]` block accommodates the bool + 7 padding bytes
 * without an ABI break.
 */
typedef struct nros_rmw_service_t {
    /** Service name (borrowed; outlives the server). */
    const char *service_name;
    /** Fully-qualified service type name (e.g.,
     *  `"example_interfaces/srv/AddTwoInts"`). Borrowed. */
    const char *type_name;
    /** Reserved for future fields; must be zero. */
    uint8_t     _reserved[8];
    /** Opaque backend state. NULL if creation failed. */
    void       *backend_data;
} nros_rmw_service_t;

/**
 * Client entity (phase-301: renamed from `service_client` to the upstream `rmw_client_t` term). Same shape as the service.
 */
typedef struct nros_rmw_client_t {
    /** Service name (borrowed; outlives the client). */
    const char *service_name;
    /** Fully-qualified service type name. Borrowed. */
    const char *type_name;
    /** Reserved for future fields; must be zero. */
    uint8_t     _reserved[8];
    /** Opaque backend state. NULL if creation failed. */
    void       *backend_data;
} nros_rmw_client_t;

#endif /* NROS_RMW_ENTITY_H */

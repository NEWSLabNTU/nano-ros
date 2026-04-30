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
/* Common QoS shape                                                   */
/* ------------------------------------------------------------------ */

/** QoS reliability policy values for `nros_rmw_qos_t::reliability`. */
#define NROS_RMW_RELIABILITY_BEST_EFFORT 0
#define NROS_RMW_RELIABILITY_RELIABLE    1

/** QoS durability policy values for `nros_rmw_qos_t::durability`. */
#define NROS_RMW_DURABILITY_VOLATILE         0
#define NROS_RMW_DURABILITY_TRANSIENT_LOCAL  1

/** QoS history policy values for `nros_rmw_qos_t::history`. */
#define NROS_RMW_HISTORY_KEEP_LAST 0
#define NROS_RMW_HISTORY_KEEP_ALL  1

/**
 * Minimal QoS subset honoured by every nros RMW backend.
 *
 * The full DDS QoS profile family (deadline, lifespan, liveliness,
 * partition, ownership, …) is not represented here — backends honour
 * the subset they natively implement, no more. See the book
 * `concepts/ros2-comparison.md` "QoS subset, not full DDS profiles"
 * section for the rationale.
 *
 * `depth` is `uint16_t` (max 65 535). Embedded ROS application queue
 * depths are typically 1–100; the 16-bit width saves two bytes per
 * entity vs the upstream 32-bit choice.
 */
typedef struct nros_rmw_qos_t {
    uint8_t  reliability;   /**< @see NROS_RMW_RELIABILITY_* */
    uint8_t  durability;    /**< @see NROS_RMW_DURABILITY_*  */
    uint8_t  history;       /**< @see NROS_RMW_HISTORY_*     */
    uint8_t  _reserved0;    /**< Reserved for future fields; must be zero. */
    uint16_t depth;
    uint16_t _reserved1;    /**< Reserved for future fields; must be zero. */
} nros_rmw_qos_t;

/* ------------------------------------------------------------------ */
/* Entity structs                                                     */
/* ------------------------------------------------------------------ */

/**
 * Per-process RMW session — the entity returned by `vtable->open`.
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
 * Subscriber entity. Same shape as the publisher; `can_loan_messages`
 * means the backend exposes the receive-side loan primitive.
 */
typedef struct nros_rmw_subscriber_t {
    /** Topic name (borrowed; outlives the subscriber). */
    const char    *topic_name;
    /** Fully-qualified type name. Borrowed. */
    const char    *type_name;
    /** QoS subset honoured by this subscriber. */
    nros_rmw_qos_t qos;
    /** Backend exposes loan_recv / release_recv (Phase 99). */
    bool           can_loan_messages;
    /** Reserved for future fields; must be zero. */
    uint8_t        _reserved[7];
    /** Opaque backend state. NULL if creation failed. */
    void          *backend_data;
} nros_rmw_subscriber_t;

/**
 * Service-server entity.
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
typedef struct nros_rmw_service_server_t {
    /** Service name (borrowed; outlives the server). */
    const char *service_name;
    /** Fully-qualified service type name (e.g.,
     *  `"example_interfaces/srv/AddTwoInts"`). Borrowed. */
    const char *type_name;
    /** Reserved for future fields; must be zero. */
    uint8_t     _reserved[8];
    /** Opaque backend state. NULL if creation failed. */
    void       *backend_data;
} nros_rmw_service_server_t;

/**
 * Service-client entity. Same shape as the service server.
 */
typedef struct nros_rmw_service_client_t {
    /** Service name (borrowed; outlives the client). */
    const char *service_name;
    /** Fully-qualified service type name. Borrowed. */
    const char *type_name;
    /** Reserved for future fields; must be zero. */
    uint8_t     _reserved[8];
    /** Opaque backend state. NULL if creation failed. */
    void       *backend_data;
} nros_rmw_service_client_t;

#endif /* NROS_RMW_ENTITY_H */

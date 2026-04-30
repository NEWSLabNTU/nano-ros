#ifndef NROS_RMW_ENTITY_H
#define NROS_RMW_ENTITY_H

#include <stdint.h>
#include <stddef.h>

/**
 * @file rmw_entity.h
 * @brief Typed entity structs for the nros RMW C surface.
 *
 * Typed entity structs that expose the metadata fields the runtime
 * reads (topic name, QoS, lending capabilities) while keeping
 * backend-private state behind an opaque `backend_data` pointer.
 * Same shape as upstream `rmw.h`'s `rmw_publisher_t` /
 * `rmw_subscription_t` family: visible metadata + `void * data`
 * tail, no generic-handle typedef.
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
    uint8_t  _pad0;         /**< Reserved; must be zero.     */
    uint16_t depth;
    uint16_t _pad1;         /**< Reserved; must be zero.     */
} nros_rmw_qos_t;

/* ------------------------------------------------------------------ */
/* Lending capability bits                                            */
/* ------------------------------------------------------------------ */

/**
 * Lending capabilities advertised by a publisher / subscriber.
 *
 * The runtime reads this once at create time and picks the publish /
 * receive code path accordingly. Backends fill it from their static
 * capability set — there is no per-call probe.
 *
 * Currently a single bit: whether the backend exposes a raw-byte
 * loan slot at all. The runtime fills the slot with whatever the
 * publisher's transport convention requires (CDR-encoded bytes for
 * wire transports; the typed-memory bypass for intra-process
 * backends like uORB has its own separate API and does not consult
 * this flag). Bits 1..7 reserved for future capability flags.
 *
 * **C-ABI note.** Plain `uint8_t bits` (not a C bitfield). Bitfield
 * ordering is implementation-defined across compilers; using a flat
 * byte with named bit-mask macros guarantees identical layout on
 * GCC, Clang, MSVC, and any other compiler we might cross to.
 */
typedef struct nros_rmw_loan_caps_t {
    uint8_t bits;
} nros_rmw_loan_caps_t;

/** Backend exposes `loan_publish` / `commit_publish` (Phase 99). */
#define NROS_RMW_LOAN_SUPPORTED  (1u << 0)
/* Bits 1..7 reserved; must be zero. */

/* ------------------------------------------------------------------ */
/* Entity structs                                                     */
/* ------------------------------------------------------------------ */

/**
 * Per-process RMW session — the entity returned by `vtable->open`.
 *
 * Carries the node identity (used for diagnostics + wire-level
 * topic-key derivation in some backends) plus the opaque
 * backend-private state.
 */
typedef struct nros_rmw_session_t {
    /** Node name (borrowed from caller; outlives the session). */
    const char *node_name;
    /** Node namespace (borrowed from caller; outlives the session). */
    const char *namespace_;
    /** Opaque backend state. NULL for an uninitialised session. */
    void *backend_data;
} nros_rmw_session_t;

/**
 * Publisher entity.
 *
 * Created by `vtable->create_publisher`; destroyed by
 * `vtable->destroy_publisher`. The runtime owns the storage; the
 * backend fills the fields via the create call.
 */
typedef struct nros_rmw_publisher_t {
    /** Topic name (borrowed; outlives the publisher). */
    const char *topic_name;
    /** ROS-2-style fully-qualified type name
     *  (e.g., `"std_msgs/msg/Int32"`). Borrowed; outlives the publisher. */
    const char *type_name;
    /** QoS subset honoured by this publisher. */
    nros_rmw_qos_t qos;
    /** Lending capabilities. */
    nros_rmw_loan_caps_t loan_caps;
    /** Opaque backend state. NULL if creation failed. */
    void *backend_data;
} nros_rmw_publisher_t;

/**
 * Subscriber entity. Same shape as the publisher.
 */
typedef struct nros_rmw_subscriber_t {
    /** Topic name (borrowed; outlives the subscriber). */
    const char *topic_name;
    /** Fully-qualified type name. Borrowed. */
    const char *type_name;
    /** QoS subset honoured by this subscriber. */
    nros_rmw_qos_t qos;
    /** Lending capabilities. */
    nros_rmw_loan_caps_t loan_caps;
    /** Opaque backend state. NULL if creation failed. */
    void *backend_data;
} nros_rmw_subscriber_t;

/**
 * Service-server entity.
 *
 * Service entities have no QoS in the nros subset (the upstream
 * `rmw_qos_profile_services_default` distinction does not generalise
 * across non-DDS backends — see book `concepts/ros2-comparison.md`).
 */
typedef struct nros_rmw_service_server_t {
    /** Service name (borrowed; outlives the server). */
    const char *service_name;
    /** Fully-qualified service type name (e.g.,
     *  `"example_interfaces/srv/AddTwoInts"`). Borrowed. */
    const char *type_name;
    /** Opaque backend state. NULL if creation failed. */
    void *backend_data;
} nros_rmw_service_server_t;

/**
 * Service-client entity. Same shape as the service server.
 */
typedef struct nros_rmw_service_client_t {
    /** Service name (borrowed; outlives the client). */
    const char *service_name;
    /** Fully-qualified service type name. Borrowed. */
    const char *type_name;
    /** Opaque backend state. NULL if creation failed. */
    void *backend_data;
} nros_rmw_service_client_t;

#endif /* NROS_RMW_ENTITY_H */

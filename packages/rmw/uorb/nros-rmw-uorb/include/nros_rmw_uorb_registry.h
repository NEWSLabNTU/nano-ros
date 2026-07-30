#ifndef NROS_RMW_UORB_REGISTRY_H
#define NROS_RMW_UORB_REGISTRY_H

#include "nros/rmw_ret.h"

#include <stddef.h>

/**
 * @file nros_rmw_uorb_registry.h
 * @brief Topic-metadata registry for the uORB RMW backend.
 *
 * Phase 115.K.4.3 — uORB has no built-in name-keyed metadata
 * lookup. The host PX4 module registers each topic it wants to
 * expose via the cffi vtable by mapping a ROS-style
 * `(topic_name, type_name)` pair to the static
 * `const struct orb_metadata *` emitted by PX4's `msggen.py`.
 *
 * Typical wiring inside a PX4 module's startup:
 *
 *     #include <uORB/topics/vehicle_status.h>
 *     #include "nros_rmw_uorb.h"
 *     #include "nros_rmw_uorb_registry.h"
 *
 *     nros_rmw_uorb_register_topic("/vehicle_status",
 *         "px4_msgs::msg::VehicleStatus",
 *         ORB_ID(vehicle_status));
 *     nros_rmw_uorb_register();
 *     // ...
 *
 * `ORB_ID(name)` expands to `&__orb_<name>`, the static topic
 * descriptor; the registry stashes that pointer so a later
 * `create_publisher` / `create_subscription` call can resolve
 * `topic_name → meta *`.
 *
 * Capacity is bounded at compile time
 * (`NROS_RMW_UORB_REGISTRY_CAPACITY`, default 64). Exceeding the
 * cap returns `NROS_RMW_RET_BAD_ALLOC`.
 */

#ifdef __cplusplus
extern "C" {
#endif

/** Opaque uORB topic descriptor pointer. Forward-declared because
 *  the registry doesn't need the full layout — it only stashes the
 *  pointer for later handoff to `orb_advertise_multi` / friends. */
struct orb_metadata;

/**
 * Register a `(topic_name, type_name) → orb_metadata *` mapping.
 *
 * Idempotent: re-registering the same triple is a no-op (the
 * runtime returns OK without growing the table).
 *
 * @retval NROS_RMW_RET_OK           on success.
 * @retval NROS_RMW_RET_INVALID_ARGUMENT if any pointer is NULL.
 * @retval NROS_RMW_RET_BAD_ALLOC    if the table is full.
 */
nros_rmw_ret_t nros_rmw_uorb_register_topic(const char *topic_name,
                                            const char *type_name,
                                            const struct orb_metadata *meta);

/**
 * Look up a previously-registered topic. Returns the orb_metadata
 * pointer or NULL if no match.
 *
 * Lookup keys on `topic_name`; `type_name` is currently ignored
 * (PX4 topics are name-unique already). A future K.4.x revision
 * may tighten this to (name, type) once we encounter a collision.
 */
const struct orb_metadata *nros_rmw_uorb_lookup_topic(const char *topic_name);

/** Clear the registry. Test-only. */
void nros_rmw_uorb_clear_registry(void);

#ifdef __cplusplus
}
#endif

#endif /* NROS_RMW_UORB_REGISTRY_H */

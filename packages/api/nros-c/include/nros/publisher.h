/**
 * @file publisher.h
 * @ingroup grp_pubsub
 * @brief Topic publisher API.
 *
 * Create publishers with nros_publisher_init() and publish serialised
 * messages with nros_publish_raw().
 */

#ifndef NROS_PUBLISHER_H
#define NROS_PUBLISHER_H

/* Type and function definitions live in <nros/nros_generated.h>.
 * This per-module header is kept as a thin shim so existing code that
 * does `#include <nros/publisher.h>` continues to compile. */
#include "nros/types.h"

/**
 * @brief rclc's best-effort preset constructor for a publisher.
 *
 * phase-417 W5.d. rclc ships one named entry point per QoS preset
 * (`_default`, `_best_effort`) and nano-ros ships the QoS-taking form
 * `nros_publisher_init_with_qos()`. Ledger row `c:publisher_init_best_effort`
 * filed that as a `divergence` on the argument that "a named function per
 * preset does not scale past the two rclc chose" — which is a reason not to
 * GROW the family, not a reason to leave the two rclc actually ships unported.
 * RFC-0089's rule points the other way: a ported rclc file writes this line, so
 * it must compile and mean the same thing.
 *
 * **What rclc means by it, read from rclc's own source** (`rclc/src/rclc/
 * publisher.c` @ `10eadcc`): `rclc_publisher_init(..., &rmw_qos_profile_sensor_data)`
 * — best-effort, volatile, KEEP_LAST(5). Not "the default profile with
 * reliability flipped": the DEPTH changes too, from 10 to 5.
 * ::NROS_QOS_SENSOR_DATA is our mirror of that profile, so this forwards to it
 * by name rather than restating nine fields (issue 0160's hand-mirror class).
 *
 * A `static inline` rather than an exported symbol, for the same reason
 * nros_difference_times() is one: no state sits behind it and nothing is
 * computed here that is not already an entry point. It adds no symbol to the
 * link and no writable data to the image.
 *
 * The typesupport parameter keeps OUR type, and the envelope is inherited
 * whole from nros_publisher_init_with_qos() — including that neither namespace
 * expansion nor remap is applied (see ledger row `c:publisher_init`). This
 * forwarder is exactly as faithful as `rclc_publisher_init_default` beside it,
 * and no more.
 *
 * @param[out] publisher   Zero-initialised publisher to fill in.
 * @param[in]  node        An initialised node.
 * @param[in]  type_info   Generated message type descriptor.
 * @param[in]  topic_name  Topic name, null-terminated.
 * @return Whatever nros_publisher_init_with_qos() returns.
 */
static inline nros_ret_t
rclc_publisher_init_best_effort(struct nros_publisher_t* publisher, const struct nros_node_t* node,
                                const struct nros_message_type_t* type_info,
                                const char* topic_name) {
    return nros_publisher_init_with_qos(publisher, node, type_info, topic_name,
                                        &NROS_QOS_SENSOR_DATA);
}

#endif /* NROS_PUBLISHER_H */

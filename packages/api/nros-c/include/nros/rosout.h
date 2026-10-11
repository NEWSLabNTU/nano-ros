/**
 * @file rosout.h
 * @ingroup grp_log
 * @brief `/rosout` from C — the log bridge rcl gives every node (issue 1589).
 *
 * Upstream republishes every node's log records on `/rosout` as
 * `rcl_interfaces/msg/Log`; that topic is what `ros2 topic echo /rosout` and
 * `rqt_console` read. Here it is three calls, and the application makes them:
 *
 * @code
 * rcl_publisher_t rosout = rcl_get_zero_initialized_publisher();
 * rclc_rosout_publisher_init(&rosout, &node, NULL);   // NULL = bounded QoS
 * rclc_rosout_enable();                               // AFTER the publisher
 * for (;;) {
 *     rclc_executor_spin_some(&executor, 10000000);
 *     rclc_rosout_pump(&rosout, NULL);
 * }
 * @endcode
 *
 * **Not automatic, deliberately.** The publisher is an ENTITY — it counts
 * against the image's pools and its sizing descriptor — so it is created by
 * the program like any other, never conjured by the runtime (issue 1341's
 * shape). The Rust bridge (`nros::rosout`) has the same three steps.
 *
 * **Which records.** Decided by the image's ROS release (RFC-0102 D4): on
 * Humble, and with no release named, records from node loggers only (the
 * logger `rclc_node_get_logger()` returns); on Iron/Jazzy, node loggers and
 * their `rclc_logger_get_child()` descendants. Free loggers
 * (`rclc_log_get_logger()`) never.
 *
 * **Build.** Declare the `rosout` capability (`[system].features =
 * ["rosout"]`, or `NROS_FEATURES` for a bare CMake build). Without it every
 * symbol below still links: rcl_logging_rosout_enabled() answers false and
 * the others ::RCL_RET_UNSUPPORTED, so a ported program compiles and says why.
 */

#ifndef NROS_ROSOUT_H
#define NROS_ROSOUT_H

#include <stdbool.h>
#include <stddef.h>

#include "nros/types.h"

#ifdef __cplusplus
extern "C" {
#endif

/**
 * `rcl_logging_rosout_enabled()`: true iff this image was built with the
 * `rosout` capability. A build-time answer, as upstream's is — it does not
 * report whether rclc_rosout_enable() has been called.
 */
bool rcl_logging_rosout_enabled(void);

/**
 * Create the `/rosout` publisher on `node` (topic `/rosout`, absolute, so no
 * namespace or remap moves it; type `rcl_interfaces/msg/Log`).
 *
 * @param qos NULL for the bounded profile — KEEP_LAST(queue depth), RELIABLE,
 *            VOLATILE, 10 s lifespan — which needs no transient-local slot.
 *            A tool that subscribes after boot then misses the boot records;
 *            `ros2 topic echo /rosout` still matches it. Pass
 *            rclc_rosout_qos_default() for upstream's TRANSIENT_LOCAL profile
 *            on a target that has budgeted for it.
 * @return what rclc_publisher_init_with_qos() returns, or
 *         ::RCL_RET_UNSUPPORTED without the `rosout` capability.
 */
rcl_ret_t rclc_rosout_publisher_init(rcl_publisher_t* publisher, const rcl_node_t* node,
                                     const rclc_qos_t* qos);

/**
 * Upstream's `rcl_qos_profile_rosout_default`: KEEP_LAST(1000), RELIABLE,
 * TRANSIENT_LOCAL, 10 s lifespan. On the zenoh backend that is one retention
 * slot plus one queryable, out of a budget of 8 on an embedded build.
 */
rclc_qos_t rclc_rosout_qos_default(void);

/**
 * Start queueing log records for `/rosout`. Call it after
 * rclc_rosout_publisher_init(): records queued with nowhere to drain to are
 * only counted as dropped.
 *
 * @return ::RCL_RET_OK; ::RCL_RET_ERROR if the queueing sink could not be
 *         registered (sink table full); ::RCL_RET_UNSUPPORTED without the
 *         `rosout` capability.
 */
rcl_ret_t rclc_rosout_enable(void);

/**
 * Publish every queued record on `publisher`. Call it from the spin loop; it
 * never blocks on the queue and never allocates. Records lost to a full queue
 * since the previous pump are reported on `/rosout` itself, as one WARN from
 * the logger `nros_rosout`.
 *
 * @param out_sent NULL, or receives the number of messages that reached the
 *                 transport (on failure: those before the first refusal).
 * @return ::RCL_RET_OK; ::RCLC_RET_PUBLISH_FAILED on the first transport
 *         refusal (the queue is still drained, so it never wedges);
 *         ::RCL_RET_UNSUPPORTED without the `rosout` capability.
 */
rcl_ret_t rclc_rosout_pump(const rcl_publisher_t* publisher, size_t* out_sent);

#ifdef __cplusplus
}
#endif

#endif /* NROS_ROSOUT_H */

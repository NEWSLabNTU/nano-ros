/*
 * Phase 88.13 — minimal C logging demo.
 *
 * Brings up a Node, fetches its Logger via `rclc_node_get_logger`,
 * and exercises every severity macro from <rclc/log.h>. Threshold
 * adjustment uses the same `rclc_logger_t` handle through a small
 * helper that calls into the Rust facade via the executor handle.
 *
 * Run after starting zenohd:
 *
 *     ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:7447"];scouting/multicast/enabled=false'
 * ros2 run rmw_zenoh_cpp rmw_zenohd
 *     ./build/c_logging
 */

#include <rclc/executor.h>
#include <rclc/init.h>
#include <rclc/node.h>

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

int main(void) {
    // Line-buffer stdout: glibc full-buffers non-tty stdout, so when piped to
    // a test harness each line must flush on its newline (Phase 177.34).
#ifdef _IOLBF /* absent on the bare-metal riscv64-threadx libc */
    setvbuf(stdout, NULL, _IOLBF, 0);
#endif

    rclc_support_t support = rclc_support_get_zero_initialized();
    rcl_allocator_t allocator = rcl_get_default_allocator();
    if (rclc_support_init(&support, 0, NULL, &allocator) != 0) {
        fprintf(stderr, "rclc_support_init failed\n");
        return 1;
    }

    // Executor first, then the node bound to it (static: keeps the executor
    // off the stack, as the other C examples do).
    static rclc_executor_t executor;
    executor = rclc_executor_get_zero_initialized_executor();
    if (rclc_executor_init(&executor, &support.context, 1, NULL) != 0) {
        fprintf(stderr, "rclc_executor_init failed\n");
        rclc_support_fini(&support);
        return 1;
    }

    rcl_node_t node = rcl_get_zero_initialized_node();
    if (rclc_executor_node_init(&executor, &node, "demo", NULL) != 0) {
        fprintf(stderr, "rclc_executor_node_init failed\n");
        rclc_executor_fini(&executor);
        rclc_support_fini(&support);
        return 1;
    }

    rclc_logger_t logger = rclc_node_get_logger(&node);
    if (logger == NULL) {
        fprintf(stderr, "rclc_node_get_logger returned NULL\n");
        rcl_node_fini(&node);
        rclc_executor_fini(&executor);
        rclc_support_fini(&support);
        return 1;
    }

    /* Default threshold = Info. Below-Info severities go quiet
     * unless caller bumps via the Rust API; mirror that here by
     * just exercising every macro and observing the filter. */
    RCLC_LOG_TRACE(logger, "round 1: trace=%d (dropped at default Info threshold)", 1);
    RCLC_LOG_DEBUG(logger, "round 1: debug=%d (dropped at default Info threshold)", 1);
    RCLC_LOG_INFO(logger, "round 1: info=%d (visible)", 1);
    RCLC_LOG_WARN(logger, "round 1: warn=%d", 1);
    RCLC_LOG_ERROR(logger, "round 1: error=%d", 1);
    RCLC_LOG_FATAL(logger, "round 1: fatal=%d", 1);

    rcl_node_fini(&node);
    rclc_executor_fini(&executor);
    rclc_support_fini(&support);
    return 0;
}

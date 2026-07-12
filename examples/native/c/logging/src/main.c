/*
 * Phase 88.13 — minimal C logging demo.
 *
 * Brings up a Node, fetches its Logger via `nros_node_get_logger`,
 * and exercises every severity macro from <nros/log.h>. Threshold
 * adjustment uses the same `nros_logger_t` handle through a small
 * helper that calls into the Rust facade via the executor handle.
 *
 * Run after starting zenohd:
 *
 *     zenohd --listen tcp/127.0.0.1:7447
 *     ./build/c_logging
 */

#include <nros/app_main.h>
#include <nros/init.h>
#include <nros/node.h>
#include <nros/log.h>

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

int main(void) {
    // Line-buffer stdout: glibc full-buffers non-tty stdout, so when piped to
    // a test harness each line must flush on its newline (Phase 177.34).
#ifdef _IOLBF /* absent on the bare-metal riscv64-threadx libc */
    setvbuf(stdout, NULL, _IOLBF, 0);
#endif

    const char* locator = getenv("NROS_LOCATOR");
    if (locator == NULL) {
        locator = NROS_ENTRY_LOCATOR;
    }

    nros_support_t support = nros_support_get_zero_initialized();
    if (nros_support_init(&support, locator, 0) != 0) {
        fprintf(stderr, "nros_support_init failed (locator=%s)\n", locator);
        return 1;
    }

    nros_node_t node = nros_node_get_zero_initialized();
    if (nros_node_init(&node, &support, "demo", "/") != 0) {
        fprintf(stderr, "nros_node_init failed\n");
        nros_support_fini(&support);
        return 1;
    }

    nros_logger_t logger = nros_node_get_logger(&node);
    if (logger == NULL) {
        fprintf(stderr, "nros_node_get_logger returned NULL\n");
        nros_node_fini(&node);
        nros_support_fini(&support);
        return 1;
    }

    /* Default threshold = Info. Below-Info severities go quiet
     * unless caller bumps via the Rust API; mirror that here by
     * just exercising every macro and observing the filter. */
    NROS_LOG_TRACE(logger, "round 1: trace=%d (dropped at default Info threshold)", 1);
    NROS_LOG_DEBUG(logger, "round 1: debug=%d (dropped at default Info threshold)", 1);
    NROS_LOG_INFO(logger, "round 1: info=%d (visible)", 1);
    NROS_LOG_WARN(logger, "round 1: warn=%d", 1);
    NROS_LOG_ERROR(logger, "round 1: error=%d", 1);
    NROS_LOG_FATAL(logger, "round 1: fatal=%d", 1);

    nros_node_fini(&node);
    nros_support_fini(&support);
    return 0;
}

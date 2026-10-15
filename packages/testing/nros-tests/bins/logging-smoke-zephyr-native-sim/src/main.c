/*
 * Phase 88.15.e — Zephyr native_sim nros-log smoke fixture.
 *
 * Boots Zephyr `native_sim`, installs the nros-log dispatcher
 * (`rclc_log_init`), grabs the catch-all logger handle
 * (`rclc_log_default_logger`), drives every Severity through
 * `NROS_LOG_*` plus one record through a child logger (`smoke.child`),
 * then exits via `posix_exit`. The harness drains
 * the native_sim process's stdout + stderr and asserts every
 * `[<LEVEL>] nros: <payload>` line appears.
 */

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>

LOG_MODULE_REGISTER(nros_log_smoke, LOG_LEVEL_INF);

#include <rclc/log.h>

#include <stdlib.h>

/* Zephyr native_sim cleanly halts the simulator process via
 * `nsi_exit(int)`. Plain libc `exit()` from a Zephyr thread leaves
 * the simulator main loop spinning. */
extern void nsi_exit(int exit_code);

int main(void) {
    LOG_INF("logging-smoke-zephyr-native-sim starting");

    /* Pin the nros-log dispatcher's `init` as a linker root and
     * install the default sink list. Phase 88.16.H. */
    rclc_log_init();

    rclc_logger_t logger = rclc_log_default_logger();

    RCLC_LOG_TRACE(logger, "trace payload");
    RCLC_LOG_DEBUG(logger, "debug payload");
    RCLC_LOG_INFO(logger,  "info payload");

    /* phase-479 W6 (RFC-0102) — one CHILD logger. A child of the catch-all is
     * a TOP-LEVEL name, so take a named parent first: the record must reach
     * Zephyr's LOG under the dotted name `smoke.child`. */
    rclc_logger_t child = rclc_logger_get_child(rclc_log_get_logger("smoke"), "child");
    if (child != NULL) {
        RCLC_LOG_INFO(child, "child payload");
    } else {
        RCLC_LOG_ERROR(logger, "rclc_logger_get_child failed");
    }

    RCLC_LOG_WARN(logger,  "warn payload");
    RCLC_LOG_ERROR(logger, "error payload");
    RCLC_LOG_FATAL(logger, "fatal payload");

    /* Give Zephyr's deferred LOG mode a chance to flush. */
    k_sleep(K_MSEC(100));

    /* `LOG_MODE_IMMEDIATE=y` makes records synchronous, but exit
     * with a small delay anyway for harness drain robustness. */
    nsi_exit(0);
    return 0;
}

/*
 * Phase 88.15.e — Zephyr native_sim nros-log smoke fixture.
 *
 * Boots Zephyr `native_sim`, installs the nros-log dispatcher
 * (`nros_log_init`), grabs the catch-all logger handle
 * (`nros_log_default_logger`), drives every Severity through
 * `NROS_LOG_*`, then exits via `posix_exit`. The harness drains
 * the native_sim process's stdout + stderr and asserts every
 * `[<LEVEL>] nros: <payload>` line appears.
 */

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>

LOG_MODULE_REGISTER(nros_log_smoke, LOG_LEVEL_INF);

#include <nros/log.h>

#include <stdlib.h>

/* Zephyr native_sim cleanly halts the simulator process via
 * `nsi_exit(int)`. Plain libc `exit()` from a Zephyr thread leaves
 * the simulator main loop spinning. */
extern void nsi_exit(int exit_code);

int main(void) {
    LOG_INF("logging-smoke-zephyr-native-sim starting");

    /* Pin the nros-log dispatcher's `init` as a linker root and
     * install the default sink list. Phase 88.16.H. */
    nros_log_init();

    nros_logger_t logger = nros_log_default_logger();

    NROS_LOG_TRACE(logger, "trace payload");
    NROS_LOG_DEBUG(logger, "debug payload");
    NROS_LOG_INFO(logger,  "info payload");
    NROS_LOG_WARN(logger,  "warn payload");
    NROS_LOG_ERROR(logger, "error payload");
    NROS_LOG_FATAL(logger, "fatal payload");

    /* Give Zephyr's deferred LOG mode a chance to flush. */
    k_sleep(K_MSEC(100));

    /* `LOG_MODE_IMMEDIATE=y` makes records synchronous, but exit
     * with a small delay anyway for harness drain robustness. */
    nsi_exit(0);
    return 0;
}

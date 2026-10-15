/*
 * issue 1712 — what this C image compiled against, on one line.
 *
 *   max_handles   NROS_EXECUTOR_MAX_HANDLES, which nros-c's build script
 *                 writes into the generated header from NROS_EXECUTOR_MAX_CBS;
 *   info_enabled  whether an INFO record passes a logger set to DEBUG at run
 *                 time — 0 exactly when nros-log was built with a compile-time
 *                 ceiling above INFO (NROS_LOG_MAX_LEVEL);
 *   warn_enabled  the same for WARN, so `info_enabled=0` cannot mean "logging
 *                 is off altogether".
 */

#include <rclc/rclc.h>

#include <stdio.h>

int main(int argc, char** argv) {
    (void)argc;
    (void)argv;
    rclc_logger_t logger = rclc_log_get_logger("image_env_probe");
    rclc_logger_set_level(logger, RCLC_LOG_SEVERITY_DEBUG);
    printf("image-env-probe-c: max_handles=%d info_enabled=%d warn_enabled=%d\n",
           (int)NROS_EXECUTOR_MAX_HANDLES,
           rclc_logger_is_enabled(logger, RCLC_LOG_SEVERITY_INFO) ? 1 : 0,
           rclc_logger_is_enabled(logger, RCLC_LOG_SEVERITY_WARN) ? 1 : 0);
    return 0;
}

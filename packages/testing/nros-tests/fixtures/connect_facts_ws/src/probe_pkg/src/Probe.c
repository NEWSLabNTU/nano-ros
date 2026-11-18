/*
 * phase-481 W4 — what this image's runtime build compiled in, on one line:
 *
 *   max_handles   NROS_EXECUTOR_MAX_HANDLES, written by nros-c's build script
 *                 from NROS_EXECUTOR_MAX_CBS;
 *   info_enabled  whether INFO passes a logger set to DEBUG at run time — 0
 *                 exactly when nros-log was built with a ceiling above INFO
 *                 (NROS_LOG_MAX_LEVEL);
 *   warn_enabled  the same for WARN, so info_enabled=0 cannot mean "off".
 *
 * The 1 s timer does nothing. It is what the metadata probe records: a
 * component that declares nothing is a failed probe, and a failed probe stops
 * the fixture build. So `plain`'s max_handles is the one slot it derives.
 */
#include <stdint.h>
#include <stdio.h>

#include <rclc/component.h>
#include <rclc/rclc.h>

typedef struct {
    rcl_timer_t timer;
} probe_t;

static void on_tick(rcl_timer_t* timer, int64_t last_call_time) {
    (void)timer;
    (void)last_call_time;
}

static rcl_ret_t probe_configure(rcl_node_t* node, rclc_executor_t* executor, probe_t* self) {
    (void)node;
    self->timer = rcl_get_zero_initialized_timer();
    rcl_ret_t rc = rclc_timer_init_default(&self->timer, NULL, 1000000000ULL, on_tick);
    if (rc == RCL_RET_OK) {
        rc = rclc_executor_add_timer(executor, &self->timer);
    }
    if (rc != RCL_RET_OK) {
        return rc;
    }
    setvbuf(stdout, NULL, _IOLBF, 0);
    rclc_logger_t logger = rclc_log_get_logger("image_config_probe");
    rclc_logger_set_level(logger, RCLC_LOG_SEVERITY_DEBUG);
    printf("image-config-probe: max_handles=%d info_enabled=%d warn_enabled=%d\n",
           (int)NROS_EXECUTOR_MAX_HANDLES,
           rclc_logger_is_enabled(logger, RCLC_LOG_SEVERITY_INFO) ? 1 : 0,
           rclc_logger_is_enabled(logger, RCLC_LOG_SEVERITY_WARN) ? 1 : 0);
    return 0;
}

RCLC_C_COMPONENT(probe_t, probe_configure)

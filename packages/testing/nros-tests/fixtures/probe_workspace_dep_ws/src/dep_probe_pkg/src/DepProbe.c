/*
 * Issue 2680 -- a component whose source includes a sibling workspace
 * package's header. The metadata probe compiles it only if it adds
 * `period_lib` before this package. The timer is what the probe records.
 */
#include <stdint.h>

#include <period_lib/period.h>
#include <rclc/component.h>
#include <rclc/rclc.h>

typedef struct {
    rcl_timer_t timer;
} dep_probe_t;

static void on_tick(rcl_timer_t* timer, int64_t last_call_time) {
    (void)timer;
    (void)last_call_time;
}

static rcl_ret_t dep_probe_configure(rcl_node_t* node, rclc_executor_t* executor,
                                     dep_probe_t* self) {
    (void)node;
    self->timer = rcl_get_zero_initialized_timer();
    rcl_ret_t rc = rclc_timer_init_default(&self->timer, NULL, PERIOD_LIB_TICK_NS, on_tick);
    if (rc == RCL_RET_OK) {
        rc = rclc_executor_add_timer(executor, &self->timer);
    }
    return rc;
}

RCLC_C_COMPONENT(dep_probe_t, dep_probe_configure)

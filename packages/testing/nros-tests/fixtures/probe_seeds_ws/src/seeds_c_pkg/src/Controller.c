/*
 * Issue 2640 -- a C component whose period has no default in code.
 *
 * `ctrl_period_ms` is declared with `rclc_add_parameter`, which declares the
 * type's zero value and keeps a launch value already seeded under the name.
 * The launch file sets it; with no launch value the period is 0 and the
 * component refuses to configure, as a node dividing by it would fail. The 40
 * `gain_NN` parameters are more than a default parameter store holds (32).
 *
 * The host metadata probe seeds the launch values before it configures the
 * component, so its sidecar records the timer and all 41 parameters.
 */
#include <stdint.h>

#include <rclc/rclc.h>
#include <rclc_parameter/rclc_parameter.h>

#define SEEDS_C_GAINS 40

static const char* const GAINS[SEEDS_C_GAINS] = {
    "gain_00", "gain_01", "gain_02", "gain_03", "gain_04", "gain_05", "gain_06", "gain_07",
    "gain_08", "gain_09", "gain_10", "gain_11", "gain_12", "gain_13", "gain_14", "gain_15",
    "gain_16", "gain_17", "gain_18", "gain_19", "gain_20", "gain_21", "gain_22", "gain_23",
    "gain_24", "gain_25", "gain_26", "gain_27", "gain_28", "gain_29", "gain_30", "gain_31",
    "gain_32", "gain_33", "gain_34", "gain_35", "gain_36", "gain_37", "gain_38", "gain_39",
};

typedef struct {
    rcl_timer_t timer;
    rclc_parameter_server_t params;
} controller_t;

static void on_tick(rcl_timer_t* timer, int64_t last_call_time) {
    (void)timer;
    (void)last_call_time;
}

static rcl_ret_t controller_configure(rcl_node_t* node, rclc_executor_t* executor,
                                      controller_t* self) {
    const rclc_parameter_options_t options = {
        .notify_changed_over_dds = false,
        .max_params = SEEDS_C_GAINS + 1,
        .allow_undeclared_parameters = false,
        .low_mem_mode = false,
    };
    rcl_ret_t rc = rclc_parameter_server_init_with_option(&self->params, node, &options);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    rc = rclc_add_parameter(&self->params, "ctrl_period_ms", RCLC_PARAMETER_INT);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    for (int i = 0; i < SEEDS_C_GAINS; i++) {
        rc = rclc_parameter_declare_int(&self->params, GAINS[i], i);
        if (rc != RCL_RET_OK) {
            return rc;
        }
    }
    int64_t period_ms = 0;
    rc = rclc_parameter_get_int(&self->params, "ctrl_period_ms", &period_ms);
    if (rc != RCL_RET_OK || period_ms <= 0) {
        return RCL_RET_ERROR;
    }
    self->timer = rcl_get_zero_initialized_timer();
    rc = rclc_timer_init_default(&self->timer, NULL, (uint64_t)period_ms * 1000000ULL, on_tick);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    return rclc_executor_add_timer(executor, &self->timer);
}

RCLC_C_COMPONENT(controller_t, controller_configure)

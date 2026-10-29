/// @file param_probe.c
/// @brief The C half of phase-426 W5 — declares one parameter and reads the
///        one C++ declared, on the SAME node.
///
/// Compiled as C (`-std=c11`), linked into a package whose component is C++.
/// That is the point: W5's claim is that `rclc_parameter_*`'s executor-backed
/// family (rclc's parameter API, since phase-492 W4) and `rclcpp::Node`'s parameter methods "point
/// at the SAME table, so C and C++ cannot disagree about what a node's parameters are", and until
/// this file there was no image where both languages touched one node's store. `c_params` and
/// `cpp_params` are single-language images that each read a launch-seeded value; neither crosses.

#include "param_probe.h"

#include <stdio.h>

#include <rclc_parameter/rclc_parameter.h>

/* The node's parameters, as rclc reaches them. The store is the executor's and
 * the parameter services are the entry's, so this handle is never added to the
 * executor. */
static rclc_parameter_server_t s_params;

rcl_ret_t mixed_param_c_attach(rcl_node_t* node) {
    rcl_ret_t rc = rclc_parameter_server_init_default(&s_params, node);
    if (rc != RCL_RET_OK) {
        fprintf(stderr, "[c] parameter handle over the C++ node failed: %d\n", (int)rc);
    }
    return rc;
}

rcl_ret_t mixed_param_c_declare_scale(double value) {
    rcl_ret_t rc = rclc_add_parameter(&s_params, "scale", RCLC_PARAMETER_DOUBLE);
    if (rc == RCL_RET_OK) {
        rc = rclc_parameter_set_double(&s_params, "scale", value);
    }
    if (rc != RCL_RET_OK) {
        fprintf(stderr, "[c] declare scale failed: %d\n", (int)rc);
    }
    return rc;
}

rcl_ret_t mixed_param_c_read_period(int64_t* out) {
    if (out == NULL) {
        return RCL_RET_INVALID_ARGUMENT;
    }
    *out = -1;
    rcl_ret_t rc = rclc_parameter_get_int(&s_params, "publish_period_ms", out);
    if (rc != RCL_RET_OK) {
        /* Loud, because a silent -1 on the wire reads as "the app is wrong"
         * rather than "the two languages are looking at different stores". */
        fprintf(stderr, "[c] read publish_period_ms (declared in C++) failed: %d\n", (int)rc);
    }
    return rc;
}

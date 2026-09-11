/// @file param_probe.c
/// @brief The C half of phase-426 W5 — declares one parameter and reads the
///        one C++ declared, on the SAME node.
///
/// Compiled as C (`-std=c11`), linked into a package whose component is C++.
/// That is the point: W5's claim is that `nros_parameter_*`'s executor-backed
/// family and `rclcpp::Node`'s parameter methods "point at the SAME table, so C
/// and C++ cannot disagree about what a node's parameters are", and until this
/// file there was no image where both languages touched one node's store.
/// `c_params` and `cpp_params` are single-language images that each read a
/// launch-seeded value; neither crosses.

#include "param_probe.h"

#include <stdio.h>

nros_cpp_ret_t mixed_param_c_declare_scale(const nros_cpp_node_t* node, double value) {
    nros_cpp_ret_t rc = nros_cpp_node_declare_param_double(node, "scale", value);
    if (rc != NROS_CPP_RET_OK && rc != NROS_CPP_RET_ALREADY_EXISTS) {
        fprintf(stderr, "[c] declare scale failed: %d\n", (int)rc);
    }
    return rc;
}

nros_cpp_ret_t mixed_param_c_read_period(const nros_cpp_node_t* node, int64_t* out) {
    if (out == NULL) {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    *out = -1;
    nros_cpp_ret_t rc = nros_cpp_node_get_param_integer(node, "publish_period_ms", out);
    if (rc != NROS_CPP_RET_OK) {
        /* Loud, because a silent -1 on the wire reads as "the app is wrong"
         * rather than "the two languages are looking at different stores". */
        fprintf(stderr, "[c] read publish_period_ms (declared in C++) failed: %d\n", (int)rc);
    }
    return rc;
}

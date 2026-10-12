/// @file param_probe.h
/// @brief The C half of phase-426 W5's cross-language parameter acceptance.
///
/// Functions over the node's C API handle, implemented in C (`param_probe.c`)
/// and called from C++ (`ParamTalker.cpp`). They are deliberately NOT wrappers
/// around anything the C++ side could call itself: each goes through rclc's
/// parameter API (`rclc_parameter_server_t` over the executor's store) — the
/// same store `rclcpp::Node::declare_parameter<T>` reaches one layer up. The
/// C++ side binds its node to an `rcl_node_t` with `nros_cpp_node_bind_c`
/// (phase-492 W4), which is how a C half meets a C++ node.

#ifndef MIXED_PARAM_TALKER_PKG_PARAM_PROBE_H
#define MIXED_PARAM_TALKER_PKG_PARAM_PROBE_H

#include <stdint.h>

#include <nros/types.h> /* rcl_node_t, rcl_ret_t */

#ifdef __cplusplus
extern "C" {
#endif

/// Open the C half's parameter handle over @p node. Call once, at configure.
rcl_ret_t mixed_param_c_attach(rcl_node_t* node);

/// Declare `scale` on the node, from C. The C++ half reads it back.
rcl_ret_t mixed_param_c_declare_scale(double value);

/// Read `publish_period_ms` off the node, from C. The C++ half declared it.
///
/// Writes `*out` and returns `RCL_RET_OK` on success. A second store anywhere
/// in the chain answers an error here, which is the failure this whole
/// fixture exists to detect.
rcl_ret_t mixed_param_c_read_period(int64_t* out);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* MIXED_PARAM_TALKER_PKG_PARAM_PROBE_H */

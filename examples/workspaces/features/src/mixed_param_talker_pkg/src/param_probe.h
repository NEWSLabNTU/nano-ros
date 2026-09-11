/// @file param_probe.h
/// @brief The C half of phase-426 W5's cross-language parameter acceptance.
///
/// Two functions over a node handle, implemented in C (`param_probe.c`) and
/// called from C++ (`ParamTalker.cpp`). They are deliberately NOT wrappers
/// around anything the C++ side could call itself: each one goes straight to
/// the `nros_cpp_node_*_param_*` FFI, which is the C entry point to the
/// executor's `nros_params::ParameterServer` — the same store
/// `rclcpp::Node::declare_parameter<T>` reaches one layer up.

#ifndef MIXED_PARAM_TALKER_PKG_PARAM_PROBE_H
#define MIXED_PARAM_TALKER_PKG_PARAM_PROBE_H

#include <stdint.h>

#include <nros/nros_cpp_ffi.h>

#ifdef __cplusplus
extern "C" {
#endif

/// Declare `scale` on @p node, from C. The C++ half reads it back.
///
/// Returns `NROS_CPP_RET_OK`, or `NROS_CPP_RET_ALREADY_EXISTS` when a launch
/// `<param>` seeded it first — which is a success, exactly as it is for
/// `rclcpp::Node::declare_parameter`.
nros_cpp_ret_t mixed_param_c_declare_scale(const nros_cpp_node_t* node, double value);

/// Read `publish_period_ms` off @p node, from C. The C++ half declared it.
///
/// Writes `*out` and returns `NROS_CPP_RET_OK` on success. A second store
/// anywhere in the chain answers `NROS_CPP_RET_NOT_FOUND` here, which is the
/// failure this whole fixture exists to detect.
nros_cpp_ret_t mixed_param_c_read_period(const nros_cpp_node_t* node, int64_t* out);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* MIXED_PARAM_TALKER_PKG_PARAM_PROBE_H */

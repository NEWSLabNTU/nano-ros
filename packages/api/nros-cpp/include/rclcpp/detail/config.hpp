// nros-cpp: inline storage size constants.
// Freestanding C++ — no exceptions, no STL required.
//
// All NROS_CPP_*_STORAGE_SIZE macros are auto-generated into
// `nros_cpp_config_generated.h`. The runtime ships compile-time
// assertions that catch any undercount — the generated values are
// safe upper bounds for the target pointer width.

/**
 * @file config.hpp
 * @ingroup grp_support
 * @brief Inline storage-size macros for opaque entity buffers.
 */

#ifndef NROS_RCLCPP_DETAIL_CONFIG_HPP
#define NROS_RCLCPP_DETAIL_CONFIG_HPP

#include "rclcpp/detail/nros_cpp_config_generated.h"

/**
 * Bytes a C++ publisher's opaque storage holds: the backend handle
 * (`NROS_PUBLISHER_SIZE`) plus three pointer widths -- the contract-monitor
 * cell and the node's lifecycle gate with its warned flag (`CppPublisher` in
 * nros-cpp's publisher.rs, whose compile-time assert is the other half of this
 * number). The ONE spelling: every publisher storage uses it, so a field added
 * there moves every reservation together.
 */
#define NROS_CPP_PUBLISHER_STORAGE_SIZE (NROS_PUBLISHER_SIZE + 3 * sizeof(void*))

#endif // NROS_RCLCPP_DETAIL_CONFIG_HPP

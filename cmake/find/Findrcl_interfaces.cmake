# Find module for rcl_interfaces — part of nano-ros's CMake package.
#
# A message package, so it routes to nano-ros codegen like every other one.
# It was a no-op (phase-209 B, before the resolver existed), so
# `rcl_interfaces/msg/parameter_descriptor.hpp` existed nowhere and a ported
# source that builds one (Nav2's `nav2_util::LifecycleNode::add_parameter`)
# failed on the include (phase-488).
include("${CMAKE_CURRENT_LIST_DIR}/_NrosFindRosMsgPackage.cmake")
_nros_find_ros_msg_package(rcl_interfaces)

# Find-stub for example_interfaces — Phase 210.E.3.a delegator.
# Layered search path: NROS_INTERFACE_SEARCH_PATH > AMENT_PREFIX_PATH > bundled.
include("${CMAKE_CURRENT_LIST_DIR}/_NrosFindRosMsgPackage.cmake")
_nros_find_ros_msg_package(example_interfaces)

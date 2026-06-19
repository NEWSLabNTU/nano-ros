cmake_minimum_required(VERSION 3.22)
project(c_safety_listener LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Phase 261 W5 — declare the E2E-safety capability via the language-neutral
# `NANO_ROS_FEATURES` surface (the C/C++ projection of `system.toml`
# `[system].features = ["safety"]`). The root `nros_lower_system_features` lowers
# it to `NANO_ROS_SAFETY_E2E=ON` before nros-cpp/nros-c read the option, building
# the zenoh backend with the CRC + validated-recv API. (Was a hand-set
# `NANO_ROS_SAFETY_E2E` knob; now the declarative axis the registry owns.)
set(NANO_ROS_FEATURES "safety")

# Phase 219.I — bootstrap nano-ros workspace helpers. Solo build walks up four
# levels for the nano-ros root. RMW override: `-DNROS_RMW=<x>` (default zenoh —
# the only backend with the CRC path).
if(NOT COMMAND nano_ros_workspace_pkg_guard)
    include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosWorkspace.cmake")
endif()
nano_ros_workspace_pkg_guard()

nros_find_interfaces(LANGUAGE C SKIP_INSTALL)

nano_ros_entry(
    NAME c_safety_listener
    SOURCES src/main.c
    DEPLOY native)

target_link_libraries(c_safety_listener PRIVATE std_msgs__nano_ros_c)
nros_platform_link_app(c_safety_listener)

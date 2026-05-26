cmake_minimum_required(VERSION 3.22)
project(c_talker LANGUAGES C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Phase 118.A.3 — collapsed-shape C talker. RMW selected at cmake
# configure time via `-DNROS_RMW=<rmw>` (or the default `zenoh`).
# The `nano_ros_link_rmw()` function the nano-ros add_subdirectory
# exposes emits the strong `nros_app_register_backends()` stub that
# calls `nros_rmw_<rmw>_register()`; main.c stays RMW-agnostic.
set(NANO_ROS_PLATFORM posix)
set(NROS_RMW "zenoh" CACHE STRING
    "Active RMW (zenoh|dds|xrce|cyclonedds) — selects the backend linked into c_talker.")
set(NANO_ROS_RMW "${NROS_RMW}")

# The Cyclone DDS backend is C++ (operator new/delete, std::nothrow). A C app
# linking it gets LINKER_LANGUAGE CXX (set by nano_ros_link_rmw), which needs
# CMAKE_CXX_LINK_EXECUTABLE — i.e. CXX enabled in *this* directory scope. The
# nano-ros add_subdirectory enables CXX only in its own subtree, so enable it
# here for the Cyclone RMW. (zenoh / xrce stay pure C.)
if(NROS_RMW STREQUAL "cyclonedds")
    enable_language(CXX)
endif()

add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

nros_generate_interfaces(builtin_interfaces SKIP_INSTALL)
nros_generate_interfaces(std_msgs DEPENDENCIES builtin_interfaces SKIP_INSTALL)

add_executable(c_talker src/main.c)
target_link_libraries(c_talker PRIVATE
    std_msgs__nano_ros_c
    NanoRos::NanoRos)
nros_platform_link_app(c_talker)

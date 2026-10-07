# nros-diagnostic-updater — phase-209 D; under packages/api since phase-482 W1
#
# Header-only `diagnostic_updater` for ROS 2 C++ nodes
# ported to nano-ros. Sees `diagnostic_msgs/msg/{DiagnosticArray,DiagnosticStatus,
# KeyValue}.hpp` via the consuming project's `nano_ros_generate_interfaces
# (diagnostic_msgs ... LANGUAGE CPP)` codegen — this target itself just exports
# the include dir + the nros-cpp runtime link.

cmake_minimum_required(VERSION 3.20)
project(nros-diagnostic-updater LANGUAGES CXX)

add_library(nros-diagnostic-updater INTERFACE)
target_include_directories(nros-diagnostic-updater INTERFACE
    $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
    $<INSTALL_INTERFACE:include>
)
# issue 1118 — at or above the floor nros-cpp declares (`cxx_std_17`).
target_compile_features(nros-diagnostic-updater INTERFACE cxx_std_17)
# issue 1467 — one home for the umbrella choice; see the note in cmake/find/Findrclcpp.cmake.
include("${CMAKE_CURRENT_LIST_DIR}/../../../cmake/NanoRosRuntimeUmbrella.cmake")
nros_link_runtime_umbrella(nros-diagnostic-updater INTERFACE
    CANDIDATES NanoRos::NanoRosCpp)

# Canonical aliases consumers can link against:
#   diagnostic_updater::diagnostic_updater (matches upstream package name)
#   NanoRos::DiagnosticUpdater             (matches nano-ros umbrella shape)
if(NOT TARGET diagnostic_updater::diagnostic_updater)
    add_library(diagnostic_updater::diagnostic_updater ALIAS nros-diagnostic-updater)
endif()
if(NOT TARGET NanoRos::DiagnosticUpdater)
    add_library(NanoRos::DiagnosticUpdater ALIAS nros-diagnostic-updater)
endif()

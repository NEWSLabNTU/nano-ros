cmake_minimum_required(VERSION 3.22)
project(reading_talker_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

if(NOT COMMAND nano_ros_workspace_pkg_guard)
    if(NOT NANO_ROS_ROOT)
        message(FATAL_ERROR
            "nano-ros: set -DNANO_ROS_ROOT=<path-to-nano-ros> for "
            "standalone builds, or build via the workspace root.")
    endif()
    include("${NANO_ROS_ROOT}/cmake/NanoRosWorkspace.cmake")
endif()
nano_ros_workspace_pkg_guard()

# Raw-CDR typed component (RFC-0043): the publisher carries the workspace-local
# type name (`custom_msgs::msg::dds_::Reading_`) as a string and hand-encodes the
# CDR payload (see src/ReadingTalker.cpp) — identical wire shape to the committed
# C `reading_talker_pkg`, just expressed in C++. No `nros_find_interfaces`, so NO
# generated interface archive is linked: the raw-CDR path dodges any cpp codegen
# edge AND the double-`builtin_interfaces`-glue a typed cpp interface link can hit.
nano_ros_node_register(
    NAME    reading_talker
    CLASS   reading_talker_pkg::ReadingTalker
    SOURCES src/ReadingTalker.cpp)

cmake_minimum_required(VERSION 3.22)
# §212.L.4 — class prefix must equal PROJECT_NAME.
project(talker_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)


# Phase 240.2b — typed component. CLASS + (derived) HEADER are recorded in
# the cmake metadata so the typed Entry can `#include "talker_pkg/Talker.hpp"`
# and construct `talker_pkg::Talker`. The header lives at
# `include/talker_pkg/Talker.hpp` (the component lib's PUBLIC include dir), so
# the convention-derived `class_header` (`talker_pkg/Talker.hpp`) resolves for
# the Entry pkg via the auto-linked component lib.
nano_ros_add_node(talker CLASS talker_pkg::Talker src/Talker.cpp DEPLOY native)

target_link_libraries(talker_pkg_talker_component
    PUBLIC std_msgs__nano_ros_cpp)

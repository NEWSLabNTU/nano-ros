cmake_minimum_required(VERSION 3.22)
project(dep_probe_pkg VERSION 0.1.0 LANGUAGES C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

nano_ros_auto_add_library(dep_probe_lib STATIC src/DepProbe.c)
# Issue 2680 -- `period_lib` is NOT added here: it is a workspace package this
# one <depend>s on, so whoever configures this package adds it first.
target_link_libraries(dep_probe_lib PUBLIC period_lib)
nros_components_register_node(dep_probe_lib
    PLUGIN dep_probe_pkg::DepProbe
    EXECUTABLE dep_probe
    SHAPE configure
    TYPED)

cmake_minimum_required(VERSION 3.22)
project(smoke C)

# Build-stage fixture for issue 0041 (no compilation inside tests). Smoke-tests
# that a user project can `add_subdirectory(nano-ros)` + link NanoRos::NanoRos.
# The repo root is 5 levels up from this fixture dir; out-of-tree
# add_subdirectory needs the explicit binary dir (2nd arg).
set(NANO_ROS_PLATFORM posix)
set(NANO_ROS_RMW     zenoh)

get_filename_component(NANO_ROS_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../../../.." ABSOLUTE)
add_subdirectory("${NANO_ROS_ROOT}" nano_ros)

add_executable(smoke main.c)
target_link_libraries(smoke PRIVATE NanoRos::NanoRos)
nros_platform_link_app(smoke)

cmake_minimum_required(VERSION 3.20)
project(consumer LANGUAGES CXX)

find_package(ament_cmake REQUIRED)
find_package(rclcpp REQUIRED)

# Workspace msg pkgs — smart Find-stub finds them under
# NROS_INTERFACE_SEARCH_PATH (set by the umbrella CMakeLists.txt one
# level up).
find_package(local_msgs REQUIRED)
find_package(extra_msgs REQUIRED)

# AMENT msg pkgs — smart Find-stub falls through to AMENT_PREFIX_PATH
# (the system ROS install). The codegen + linkage path is identical to
# the workspace pkgs above.
find_package(std_msgs REQUIRED)
find_package(geometry_msgs REQUIRED)
find_package(sensor_msgs REQUIRED)

add_executable(consumer src/consumer.cpp)

# Canonical ROS 2 ament_cmake shape — `ament_target_dependencies` wires
# include dirs + link libraries for each named pkg (under stock ROS the
# real macro walks each pkg's `_LIBRARIES`/`_INCLUDE_DIRS`; under nano-ros
# the compat shim in `NrosRclcppCompat.cmake` aliases to `${pkg}::${pkg}`).
# Works verbatim under BOTH `colcon build` and the nano-ros umbrella.
ament_target_dependencies(consumer
    rclcpp
    local_msgs
    extra_msgs
    std_msgs
    geometry_msgs
    sensor_msgs
)

# Stock-ROS install (so colcon merge-install layout puts the binary at
# install/lib/consumer/consumer). The nano-ros build path doesn't need
# this — the umbrella keeps the binary under build/. ament_cmake
# substitutes a no-op under our compat module.
install(TARGETS consumer DESTINATION lib/${PROJECT_NAME})

ament_package()
